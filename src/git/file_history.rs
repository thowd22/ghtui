//! Bounded native file provenance reads through original Git objects.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHistoryEntry {
    pub commit: Commit,
    pub path: Option<RawPath>,
    pub previous_path: Option<RawPath>,
    pub change: String,
    pub exists: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHistoryPage {
    pub revision: String,
    pub path: RawPath,
    pub offset: usize,
    pub entries: Vec<FileHistoryEntry>,
    pub has_more: bool,
    pub walk_limited: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileBlameLine {
    pub line: BlameLine,
    pub boundary: bool,
    pub ignored: bool,
    pub unblamable: bool,
}
impl GitRepository {
    pub fn file_history_page(
        &self,
        revision: &str,
        path: &RawPath,
        offset: usize,
        limit: usize,
    ) -> GitResult<FileHistoryPage> {
        validate_path(path)?;
        if !(1..=200).contains(&limit) || offset >= 10_000 {
            return Err(GitError(
                "File history pages require1..200 entries and a bounded10,000-entry original walk"
                    .into(),
            ));
        }
        let revision = self.resolve_original(revision)?;
        // --skip can suppress the very rename that --follow needs to continue.
        // Repeat only a bounded prefix and select the requested slice afterward.
        let walk = offset.saturating_add(limit).saturating_add(1).min(10_000);
        let args=[
            OsString::from("log"),OsString::from("--follow"),OsString::from("--find-renames"),
            OsString::from("--name-status"),OsString::from("-z"),
            OsString::from(format!("--max-count={walk}")),
            OsString::from("--format=%x00GHTUI_FILE_HISTORY%x00%H%x00%P%x00%an%x00%ae%x00%at%x00%cn%x00%ce%x00%ct%x00%s%x00%D%x00"),
            OsString::from(&revision),OsString::from("--"),path.as_os_str().to_owned(),
        ];
        let entries = parse_file_history(&self.original_bytes(args.iter(), None)?)?;
        if entries.len() > walk {
            return Err(GitError(
                "Git file history exceeded its requested bounded walk".into(),
            ));
        }
        let has_more = entries.len() > offset.saturating_add(limit);
        let walk_limited = entries.len() == 10_000;
        let mut entries = entries
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect::<Vec<_>>();
        let commits =
            self.canonical_parents(entries.iter().map(|entry| entry.commit.clone()).collect())?;
        for (entry, commit) in entries.iter_mut().zip(commits) {
            entry.commit = commit;
        }
        Ok(FileHistoryPage {
            revision,
            path: path.clone(),
            offset,
            entries,
            has_more,
            walk_limited,
        })
    }
    /// Caller computes the exact end from the chosen immutable blob. No full-file
    /// porcelain accumulation is required for a large text-file page.
    pub fn blame_lines(
        &self,
        revision: &str,
        path: &RawPath,
        start: usize,
        count: usize,
    ) -> GitResult<Vec<FileBlameLine>> {
        validate_path(path)?;
        if start == 0 || start > 2_000_000 || !(1..=1000).contains(&count) {
            return Err(GitError(
                "Blame requires an exact positive line and1..1000 lines per page".into(),
            ));
        }
        let revision = self.resolve_original(revision)?;
        let end = start
            .checked_add(count - 1)
            .ok_or_else(|| GitError("Blame line range overflow".into()))?;
        let args = [
            OsString::from("-c"),
            OsString::from("blame.blankBoundary=false"),
            OsString::from("blame"),
            OsString::from("--line-porcelain"),
            OsString::from("--no-textconv"),
            OsString::from("--ignore-revs-file"),
            OsString::from(""),
            OsString::from("-L"),
            OsString::from(format!("{start},{end}")),
            OsString::from(revision),
            OsString::from("--"),
            path.as_os_str().to_owned(),
        ];
        let bytes = self.original_bytes(args.iter(), None)?;
        let lines = parse_blame(&bytes)?;
        if lines.len() != count
            || lines.iter().enumerate().any(|(index, line)| {
                line.final_line as usize != start + index
                    || line.original_line == 0
                    || line.path.0.is_empty()
                    || line.commit.bytes().all(|byte| byte == b'0')
            })
        {
            return Err(GitError(
                "Blame returned incomplete or contradictory canonical line/path coordinates".into(),
            ));
        }
        let mut flags = vec![];
        let mut current = (false, false, false);
        for row in bytes.split(|byte| *byte == b'\n') {
            match row {
                b"boundary" => current.0 = true,
                b"ignored" => current.1 = true,
                b"unblamable" => current.2 = true,
                row if row.starts_with(b"\t") => {
                    flags.push(current);
                    current = (false, false, false);
                }
                _ => {}
            }
        }
        if flags.len() != lines.len() {
            return Err(GitError("Blame boundary metadata is incomplete".into()));
        }
        Ok(lines
            .into_iter()
            .zip(flags)
            .map(|(line, (boundary, ignored, unblamable))| FileBlameLine {
                line,
                boundary,
                ignored,
                unblamable,
            })
            .collect())
    }
}

fn parse_file_history(bytes: &[u8]) -> GitResult<Vec<FileHistoryEntry>> {
    if bytes.len() > MAX_OUTPUT
        || bytes
            .iter()
            .filter(|byte| **byte == 0)
            .take(500_001)
            .count()
            > 500_000
    {
        return Err(GitError(
            "File history machine metadata exceeds its bounded token budget".into(),
        ));
    }
    let fields = bytes.split(|byte| *byte == 0).collect::<Vec<_>>();
    let mut cursor = 0;
    let mut entries = vec![];
    while cursor < fields.len() {
        while cursor < fields.len() && fields[cursor].is_empty() {
            cursor += 1;
        }
        if cursor == fields.len() {
            break;
        }
        if fields[cursor] != b"GHTUI_FILE_HISTORY" {
            return Err(GitError(
                "File history record has no exact machine header".into(),
            ));
        }
        cursor += 1;
        if fields.len().saturating_sub(cursor) < 10 {
            return Err(GitError("Truncated canonical file history commit".into()));
        }
        let mut commit_bytes = vec![];
        for field in &fields[cursor..cursor + 10] {
            commit_bytes.extend_from_slice(field);
            commit_bytes.push(0);
        }
        let commit = parse_commits(&commit_bytes)?
            .into_iter()
            .next()
            .ok_or_else(|| GitError("File history commit is missing".into()))?;
        validate_object_id(&commit.id)?;
        cursor += 10;
        // Git's pretty record terminator precedes the first newline/name-status.
        while cursor < fields.len() && fields[cursor].is_empty() {
            cursor += 1;
        }
        let mut paths = vec![];
        while cursor < fields.len()
            && !fields[cursor].is_empty()
            && fields[cursor] != b"GHTUI_FILE_HISTORY"
        {
            let status = fields[cursor].strip_prefix(b"\n").unwrap_or(fields[cursor]);
            cursor += 1;
            let change = text(status);
            let first = status
                .first()
                .copied()
                .ok_or_else(|| GitError("Missing file history change kind".into()))?;
            if !b"ACDMRTUXB".contains(&first)
                || status[1..].iter().any(|byte| !byte.is_ascii_digit())
            {
                return Err(GitError(
                    "File history has malformed name-status metadata".into(),
                ));
            }
            let take = if matches!(first, b'R' | b'C') { 2 } else { 1 };
            if fields.len().saturating_sub(cursor) < take {
                return Err(GitError("Truncated file history raw path".into()));
            }
            let old = RawPath(fields[cursor].to_vec());
            validate_path(&old)?;
            cursor += 1;
            let (path, previous_path) = if take == 2 {
                let path = RawPath(fields[cursor].to_vec());
                validate_path(&path)?;
                cursor += 1;
                (path, Some(old))
            } else {
                (old, None)
            };
            paths.push((path, previous_path, change, first != b'D'));
        }
        let (path, previous_path, change, exists) = if paths.len() == 1 {
            let (path, previous_path, change, exists) = paths.remove(0);
            (Some(path), previous_path, change, exists)
        } else {
            (
                None,
                None,
                "Merge/ambiguous path diff — exact file target unavailable".into(),
                false,
            )
        };
        entries.push(FileHistoryEntry {
            commit,
            path,
            previous_path,
            change,
            exists,
        });
    }
    Ok(entries)
}
