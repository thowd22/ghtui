//! Read-only History query provider (TASK-99.2/99.3). One installed-Git walk
//! over a captured ref snapshot (or one chosen ref) with typed ref chips,
//! author/path/text filters and match flags. Every child is a bounded read
//! under the repository's cancellation; nothing touches the working tree,
//! the index or refs.
//!
//! Filter semantics:
//! - `author` and `path` chips are Git's own `--author` (case-insensitive,
//!   fixed string) and pathspec, with `--follow` for one renamed file.
//! - Plain `text` matches when Git's `--grep` (case-insensitive, fixed string,
//!   full message) selects the commit, when the author name or email contains
//!   it, or when it is a SHA prefix of four or more hex digits.
//! - Highlight mode walks the unfiltered scope and flags matches, so
//!   non-matching commits stay visible for context. Hide mode walks Git's
//!   chip-filtered history (`--parents` keeps the rewritten graph parents)
//!   and drops commits that fail the text filter.
use super::*;
use std::collections::HashSet;

/// Commits walked by one session, matching the other original graph reads.
const MAX_WALK: usize = 10_000;
const MAX_PAGE: usize = 500;
/// Chips kept per snapshot; the remainder is reported as overflow.
const MAX_REFS: usize = 4096;
const MAX_RAW_PAGE: usize = 16 * 1024 * 1024;
const MAX_RAW_IDS: usize = (MAX_WALK + 1) * 66;
const MAX_RAW_REFS: usize = 8 * 1024 * 1024;
const RECORD: &[u8] = b"GHTUI_HISTORY_V1";
const FORMAT: &str =
    "--format=%x00GHTUI_HISTORY_V1%x00%H%x00%P%x00%an%x00%ae%x00%at%x00%cn%x00%ce%x00%ct%x00%s%x00";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum HistoryRefKindV1 {
    /// Only emitted for a detached HEAD; an attached HEAD is the `current`
    /// flag on its local branch chip.
    Head,
    Local,
    Remote,
    Tag,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryRefV1 {
    pub kind: HistoryRefKindV1,
    /// Short display name (`main`, `origin/main`, `v1.0`, `HEAD`).
    pub name: String,
    /// Full refname bytes as Git reports them (`HEAD` for a detached HEAD).
    pub raw_name: Vec<u8>,
    /// The commit the ref peels to.
    pub target: String,
    /// The local branch HEAD is attached to.
    pub current: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HistoryRefSnapshotV1 {
    refs: Vec<HistoryRefV1>,
    head: Option<String>,
    overflow: usize,
    non_commit: usize,
}
impl HistoryRefSnapshotV1 {
    /// Chips in display order: HEAD, current branch, local, remote, tag.
    pub fn refs(&self) -> &[HistoryRefV1] {
        &self.refs
    }
    /// The commit HEAD resolves to; None for an unborn branch.
    pub fn head(&self) -> Option<&str> {
        self.head.as_deref()
    }
    pub fn head_detached(&self) -> bool {
        self.refs
            .iter()
            .any(|chip| chip.kind == HistoryRefKindV1::Head)
    }
    /// Refs beyond the 4096-chip cap, shown as an explicit overflow.
    pub fn overflow(&self) -> usize {
        self.overflow
    }
    /// Refs (such as tags of trees or blobs) that peel to no commit.
    pub fn non_commit(&self) -> usize {
        self.non_commit
    }
    pub fn chips_for(&self, id: &str) -> Vec<HistoryRefV1> {
        self.refs
            .iter()
            .filter(|chip| chip.target == id)
            .cloned()
            .collect()
    }
    /// Distinct walk tips: every ref target plus HEAD, in snapshot order.
    fn tips(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        self.head
            .iter()
            .chain(self.refs.iter().map(|chip| &chip.target))
            .filter(|id| seen.insert(id.as_str()))
            .cloned()
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistoryScopeV1 {
    /// Local branches, remote branches, tags and HEAD.
    AllRefs,
    /// One branch, tag or revision.
    Ref(String),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HistoryOrderV1 {
    #[default]
    Topo,
    Date,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HistoryFilterModeV1 {
    #[default]
    Highlight,
    Hide,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HistoryFilterV1 {
    pub text: Option<String>,
    pub author: Option<String>,
    pub path: Option<RawPath>,
    /// Follow renames of `path`; meaningful only when it names one file.
    pub follow_renames: bool,
}
impl HistoryFilterV1 {
    pub fn is_empty(&self) -> bool {
        self.text().is_none() && self.author().is_none() && self.path.is_none()
    }
    fn text(&self) -> Option<&str> {
        self.text
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
    }
    fn author(&self) -> Option<&str> {
        self.author
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
    }
    fn follow(&self) -> bool {
        self.follow_renames && self.path.is_some()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryQueryV1 {
    pub scope: HistoryScopeV1,
    pub order: HistoryOrderV1,
    pub filter: HistoryFilterV1,
    pub mode: HistoryFilterModeV1,
}
impl Default for HistoryQueryV1 {
    fn default() -> Self {
        Self {
            scope: HistoryScopeV1::AllRefs,
            order: HistoryOrderV1::Topo,
            filter: HistoryFilterV1::default(),
            mode: HistoryFilterModeV1::Highlight,
        }
    }
}

/// Text matching evidence captured once per session. Git selects `--grep`
/// matches; author and SHA-prefix checks run on each loaded commit.
#[derive(Clone, Debug, Default)]
struct MatchSetV1 {
    /// Ids selected by the author/path chips (Highlight mode only).
    chips: Option<HashSet<String>>,
    grep: Option<HashSet<String>>,
    text: Option<String>,
    capped: bool,
}
impl MatchSetV1 {
    fn matches(&self, commit: &Commit) -> bool {
        if self
            .chips
            .as_ref()
            .is_some_and(|ids| !ids.contains(&commit.id))
        {
            return false;
        }
        let Some(text) = &self.text else {
            return true;
        };
        self.grep
            .as_ref()
            .is_some_and(|ids| ids.contains(&commit.id))
            || commit.author.name.to_lowercase().contains(text)
            || commit.author.email.to_lowercase().contains(text)
            || (text.len() >= 4
                && text.bytes().all(|byte| byte.is_ascii_hexdigit())
                && commit.id.starts_with(text.as_str()))
    }
}

/// One stable History read: the query, its ref snapshot and walk tips, and
/// the match evidence. Pages of one session never mix snapshots.
#[derive(Clone, Debug)]
pub struct HistorySessionV1 {
    query: HistoryQueryV1,
    refs: Arc<HistoryRefSnapshotV1>,
    tips: Arc<[String]>,
    matches: Arc<MatchSetV1>,
}
impl HistorySessionV1 {
    pub fn query(&self) -> &HistoryQueryV1 {
        &self.query
    }
    pub fn refs(&self) -> &HistoryRefSnapshotV1 {
        &self.refs
    }
    pub fn tips(&self) -> &[String] {
        &self.tips
    }
    /// Match evidence stopped at the bounded walk; later commits may be
    /// unflagged.
    pub fn matches_capped(&self) -> bool {
        self.matches.capped
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryCommitV1 {
    /// Canonical commit with actual object parents.
    pub commit: Commit,
    /// Parents as the walk reported them; Hide mode path filters rewrite
    /// these through simplified history.
    pub graph_parents: Vec<String>,
    pub refs: Vec<HistoryRefV1>,
    pub matched: bool,
    /// The followed file's path at this commit, when following renames.
    pub path: Option<RawPath>,
    pub previous_path: Option<RawPath>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryPageV1 {
    pub offset: usize,
    /// Walk position to continue from; Hide mode text filtering can consume
    /// more walk rows than it returns.
    pub next_offset: usize,
    pub commits: Vec<HistoryCommitV1>,
    pub has_more: bool,
    /// The walk reached its 10,000-commit bound.
    pub walk_limited: bool,
}

/// Next matching row after `from` (or from the top), optionally wrapping.
pub fn history_next_match_v1(
    commits: &[HistoryCommitV1],
    from: Option<usize>,
    wrap: bool,
) -> Option<usize> {
    let start = from.map_or(0, |index| index + 1);
    (start..commits.len())
        .chain(if wrap {
            0..start.min(commits.len())
        } else {
            0..0
        })
        .find(|&index| commits[index].matched)
}

/// Previous matching row before `from` (or from the bottom), optionally wrapping.
pub fn history_prev_match_v1(
    commits: &[HistoryCommitV1],
    from: Option<usize>,
    wrap: bool,
) -> Option<usize> {
    let end = from.unwrap_or(commits.len()).min(commits.len());
    (0..end)
        .rev()
        .chain(if wrap {
            (end..commits.len()).rev()
        } else {
            (0..0).rev()
        })
        .find(|&index| commits[index].matched)
}

impl GitRepository {
    /// Typed ref chips from one `for-each-ref` plus the HEAD commit.
    pub fn history_refs_v1(&self) -> GitResult<HistoryRefSnapshotV1> {
        let raw = self.original_bytes_with_capture_limit_v1(
            [
                "for-each-ref",
                "--format=%(objectname)%00%(objecttype)%00%(*objectname)%00%(*objecttype)%00%(HEAD)%00%(refname)",
                "refs/heads",
                "refs/remotes",
                "refs/tags",
            ],
            None,
            MAX_RAW_REFS,
        )?;
        let head = self.run(
            [
                "--no-replace-objects",
                "rev-parse",
                "--verify",
                "-q",
                "--end-of-options",
                "HEAD^{commit}",
            ],
            None,
        )?;
        let head = if head.success {
            let id = text(trim_line(&head.stdout));
            validate_object_id(&id)?;
            Some(id)
        } else {
            None
        };
        parse_refs(&raw, head)
    }

    /// Capture the ref snapshot, walk tips and match evidence for `query`.
    pub fn history_session_v1(&self, query: HistoryQueryV1) -> GitResult<HistorySessionV1> {
        if let Some(path) = &query.filter.path {
            validate_path(path)?;
        }
        let refs = self.history_refs_v1()?;
        let tips = match &query.scope {
            HistoryScopeV1::AllRefs => refs.tips(),
            HistoryScopeV1::Ref(name) => vec![self.resolve_original(name)?],
        };
        let mut matches = MatchSetV1 {
            text: query.filter.text().map(str::to_lowercase),
            ..MatchSetV1::default()
        };
        if !tips.is_empty() {
            if query.mode == HistoryFilterModeV1::Highlight
                && (query.filter.author().is_some() || query.filter.path.is_some())
            {
                let (ids, capped) = self.history_ids(&query, &tips, None)?;
                matches.chips = Some(ids);
                matches.capped |= capped;
            }
            if let Some(text) = query.filter.text() {
                let (ids, capped) = self.history_ids(&query, &tips, Some(text))?;
                matches.grep = Some(ids);
                matches.capped |= capped;
            }
        }
        Ok(HistorySessionV1 {
            query,
            refs: Arc::new(refs),
            tips: tips.into(),
            matches: Arc::new(matches),
        })
    }

    /// Up to `limit` rows from walk position `offset` of the session.
    pub fn history_page_v1(
        &self,
        session: &HistorySessionV1,
        offset: usize,
        limit: usize,
    ) -> GitResult<HistoryPageV1> {
        if !(1..=MAX_PAGE).contains(&limit) || offset > MAX_WALK {
            return Err(GitError(
                "History pages require 1..500 rows inside the bounded 10,000-commit walk".into(),
            ));
        }
        let hide = session.query.mode == HistoryFilterModeV1::Hide;
        let filter_text = hide && session.matches.text.is_some();
        let mut commits = Vec::new();
        let mut position = offset;
        // Walk position after the last commit, once a read has reached it.
        let mut walk_end = session.tips.is_empty().then_some(offset);
        while walk_end.is_none() && commits.len() < limit && position < MAX_WALK {
            // Text filtering in Hide mode may drop rows; read whole chunks.
            let chunk = if filter_text {
                200
            } else {
                limit - commits.len()
            }
            .min(MAX_WALK - position);
            let rows = self.history_rows(session, position, chunk + 1)?;
            if rows.len() <= chunk {
                walk_end = Some(position + rows.len());
            }
            for mut row in rows.into_iter().take(chunk) {
                position += 1;
                row.matched = if hide {
                    !filter_text || session.matches.matches(&row.commit)
                } else {
                    !session.query.filter.is_empty() && session.matches.matches(&row.commit)
                };
                if hide && !row.matched {
                    continue;
                }
                row.refs = session.refs.chips_for(&row.commit.id);
                commits.push(row);
                if commits.len() == limit {
                    break;
                }
            }
        }
        if !commits.is_empty() {
            let canonical =
                self.canonical_parents(commits.iter().map(|row| row.commit.clone()).collect())?;
            for (row, commit) in commits.iter_mut().zip(canonical) {
                row.commit.parents = commit.parents;
            }
        }
        let remaining = walk_end.is_none_or(|end| position < end);
        let walk_limited = remaining && position >= MAX_WALK;
        Ok(HistoryPageV1 {
            offset,
            next_offset: position,
            has_more: remaining && !walk_limited,
            walk_limited,
            commits,
        })
    }

    /// Walk arguments shared by pages and match evidence.
    fn history_args(
        &self,
        query: &HistoryQueryV1,
        filtered: bool,
        grep: Option<&str>,
    ) -> Vec<OsString> {
        let mut args = vec![
            OsString::from("log"),
            OsString::from(match query.order {
                HistoryOrderV1::Topo => "--topo-order",
                HistoryOrderV1::Date => "--date-order",
            }),
            OsString::from("--stdin"),
        ];
        if !filtered {
            return args;
        }
        let filter = &query.filter;
        if filter.author().is_some() || grep.is_some() {
            args.push("--regexp-ignore-case".into());
            args.push("--fixed-strings".into());
        }
        if let Some(author) = filter.author() {
            args.push(format!("--author={author}").into());
        }
        if let Some(text) = grep {
            args.push(format!("--grep={text}").into());
        }
        if filter.follow() {
            args.push("--follow".into());
        }
        args
    }

    fn history_pathspec(query: &HistoryQueryV1, filtered: bool, args: &mut Vec<OsString>) {
        args.push("--".into());
        if let Some(path) = query.filter.path.as_ref().filter(|_| filtered) {
            args.push(path.as_os_str().to_owned());
        }
    }

    fn history_stdin(tips: &[String]) -> Vec<u8> {
        let mut stdin = tips.join("\n").into_bytes();
        stdin.push(b'\n');
        stdin
    }

    /// Ids of the chip-filtered walk, optionally narrowed by `--grep`.
    fn history_ids(
        &self,
        query: &HistoryQueryV1,
        tips: &[String],
        grep: Option<&str>,
    ) -> GitResult<(HashSet<String>, bool)> {
        let mut args = self.history_args(query, true, grep);
        args.push("--format=%H".into());
        args.push(format!("--max-count={}", MAX_WALK + 1).into());
        Self::history_pathspec(query, true, &mut args);
        let raw = self.original_bytes_with_capture_limit_v1(
            args.iter(),
            Some(&Self::history_stdin(tips)),
            MAX_RAW_IDS,
        )?;
        let mut ids = HashSet::new();
        for line in raw
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
        {
            let id = text(line);
            validate_object_id(&id)?;
            ids.insert(id);
        }
        let capped = ids.len() > MAX_WALK;
        Ok((ids, capped))
    }

    /// `count` walk rows starting at `offset`, unflagged and without chips.
    fn history_rows(
        &self,
        session: &HistorySessionV1,
        offset: usize,
        count: usize,
    ) -> GitResult<Vec<HistoryCommitV1>> {
        let query = &session.query;
        let hide = query.mode == HistoryFilterModeV1::Hide;
        let follow = hide && query.filter.follow();
        let mut args = self.history_args(query, hide, None);
        args.push(FORMAT.into());
        args.push("-z".into());
        if hide {
            args.push("--parents".into());
        }
        // --skip can drop the rename --follow needs to continue; read the
        // bounded prefix and slice afterwards instead.
        if follow {
            args.push("--name-status".into());
            args.push("--find-renames".into());
            args.push(format!("--max-count={}", offset + count).into());
        } else {
            args.push(format!("--skip={offset}").into());
            args.push(format!("--max-count={count}").into());
        }
        Self::history_pathspec(query, hide, &mut args);
        let raw = self.original_bytes_with_capture_limit_v1(
            args.iter(),
            Some(&Self::history_stdin(&session.tips)),
            MAX_RAW_PAGE,
        )?;
        let rows = parse_rows(&raw, follow)?;
        Ok(if follow {
            rows.into_iter().skip(offset).collect()
        } else {
            rows
        })
    }
}

fn parse_refs(raw: &[u8], head: Option<String>) -> GitResult<HistoryRefSnapshotV1> {
    let mut refs = Vec::new();
    let mut overflow = 0;
    let mut non_commit = 0;
    let mut attached = false;
    for line in raw
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let fields = line.splitn(6, |byte| *byte == 0).collect::<Vec<_>>();
        let [object, kind, peeled, peeled_kind, current, name] = fields[..] else {
            return Err(GitError("Truncated History ref record".into()));
        };
        let target = match (kind, peeled_kind) {
            (b"commit", _) => object,
            (b"tag", b"commit") => peeled,
            _ => {
                non_commit += 1;
                continue;
            }
        };
        let target = text(target);
        validate_object_id(&target)?;
        let current = current == b"*";
        attached |= current;
        let (kind, short) = if let Some(short) = name.strip_prefix(b"refs/heads/") {
            (HistoryRefKindV1::Local, short)
        } else if let Some(short) = name.strip_prefix(b"refs/remotes/") {
            (HistoryRefKindV1::Remote, short)
        } else if let Some(short) = name.strip_prefix(b"refs/tags/") {
            (HistoryRefKindV1::Tag, short)
        } else {
            return Err(GitError(
                "History ref outside its requested namespaces".into(),
            ));
        };
        if refs.len() == MAX_REFS {
            overflow += 1;
            continue;
        }
        refs.push(HistoryRefV1 {
            kind,
            name: reference_label(short),
            raw_name: name.to_vec(),
            target,
            current,
        });
    }
    if let Some(id) = head.as_ref().filter(|_| !attached) {
        refs.push(HistoryRefV1 {
            kind: HistoryRefKindV1::Head,
            name: "HEAD".into(),
            raw_name: b"HEAD".to_vec(),
            target: id.clone(),
            current: false,
        });
    }
    refs.sort_by(|a, b| (a.kind, !a.current, &a.raw_name).cmp(&(b.kind, !b.current, &b.raw_name)));
    Ok(HistoryRefSnapshotV1 {
        refs,
        head,
        overflow,
        non_commit,
    })
}

fn parse_rows(raw: &[u8], name_status: bool) -> GitResult<Vec<HistoryCommitV1>> {
    let fields = raw.split(|byte| *byte == 0).collect::<Vec<_>>();
    let mut cursor = 0;
    let mut rows = Vec::new();
    let skip_empty = |cursor: &mut usize| {
        while *cursor < fields.len() && matches!(fields[*cursor], b"" | b"\n") {
            *cursor += 1;
        }
    };
    loop {
        skip_empty(&mut cursor);
        if cursor == fields.len() {
            break;
        }
        if fields[cursor] != RECORD || fields.len() - cursor < 10 {
            return Err(GitError(
                "History record has no exact machine header".into(),
            ));
        }
        cursor += 1;
        // The same parser as ordinary commit lists, with an empty %D field.
        let mut commit_bytes = Vec::new();
        for field in &fields[cursor..cursor + 9] {
            commit_bytes.extend_from_slice(field);
            commit_bytes.push(0);
        }
        commit_bytes.push(0);
        cursor += 9;
        let commit = parse_commits(&commit_bytes)?
            .pop()
            .ok_or_else(|| GitError("History commit is missing".into()))?;
        validate_object_id(&commit.id)?;
        let mut row = HistoryCommitV1 {
            graph_parents: commit.parents.clone(),
            commit,
            refs: Vec::new(),
            matched: false,
            path: None,
            previous_path: None,
        };
        if name_status {
            skip_empty(&mut cursor);
            let mut paths = Vec::new();
            while cursor < fields.len() && !fields[cursor].is_empty() && fields[cursor] != RECORD {
                let status = fields[cursor].strip_prefix(b"\n").unwrap_or(fields[cursor]);
                cursor += 1;
                let renamed = matches!(status.first(), Some(b'R' | b'C'));
                let take = if renamed { 2 } else { 1 };
                if fields.len() - cursor < take {
                    return Err(GitError("Truncated History name-status path".into()));
                }
                let first = RawPath(fields[cursor].to_vec());
                let path = if renamed {
                    (RawPath(fields[cursor + 1].to_vec()), Some(first))
                } else {
                    (first, None)
                };
                cursor += take;
                paths.push(path);
            }
            if let [(path, previous)] = &paths[..] {
                row.path = Some(path.clone());
                row.previous_path = previous.clone();
            }
        }
        rows.push(row);
    }
    Ok(rows)
}

#[cfg(test)]
#[path = "history_query_v1_tests.rs"]
mod tests;
