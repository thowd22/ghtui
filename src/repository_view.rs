//! Prepared repository presentation models. These contain no subprocess or filesystem work.

mod tree_merge_v1;
use crate::diff::{Diff, LineKind};
use crate::git::{
    display_bytes, display_path_bytes, Commit, GitActor, RawPath, RepoStatus, TreeEntry,
};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticRole {
    Normal,
    Added,
    Removed,
    Changed,
    Conflict,
    Head,
    Reference,
    Boundary,
}

pub fn actor_label(actor: &GitActor) -> String {
    // A local author name/email is honest provenance, not an inferred GitHub login.
    crate::git_actor_labels_v1::actor_text(actor)
}

pub fn explain_commit(commit: &Commit) -> String {
    let history = match commit.parents.len() {
        0 => "This is a root commit: the first saved version in this history.".to_owned(),
        1 => format!(
            "This saved version follows parent {}.",
            short_id(&commit.parents[0])
        ),
        count => format!(
            "This merge joins {count} histories. Its ordered parents are {}.",
            commit
                .parents
                .iter()
                .map(|id| short_id(id))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    let refs = if commit.refs.is_empty() {
        String::new()
    } else {
        format!(" Names pointing here: {}.", commit.refs.join(", "))
    };
    format!("{history}{refs} Graph colors trace parent paths; a lane is not a permanent branch.")
}

fn short_id(id: &str) -> &str {
    &id[..id.len().min(8)]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeKind {
    Directory,
    File,
    Symlink,
    Submodule,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlobPreviewKind {
    Text,
    Binary,
    Symlink,
    Submodule,
    LfsPointer { object: String, size: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePreviewLine {
    pub line: Option<u32>,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePreview {
    pub revision: String,
    pub path: RawPath,
    pub object_id: String,
    pub kind: BlobPreviewKind,
    pub content: Vec<u8>,
    pub rows: Vec<FilePreviewLine>,
    pub truncated: bool,
}

pub fn file_preview(
    revision: String,
    entry: &TreeEntry,
    content: Vec<u8>,
    max_lines: usize,
) -> FilePreview {
    let kind = file_content_kind_v3(&entry.mode, entry.kind == "commit", &content);
    let (rows, truncated) = content_preview_rows_v3(
        &kind,
        &content,
        max_lines,
        FilePreviewSourceV3::Saved(&entry.id),
    );
    FilePreview {
        revision,
        path: entry.path.clone(),
        object_id: entry.id.clone(),
        kind,
        content,
        rows,
        truncated,
    }
}

/// Content classification is shared by saved files and explicitly unsaved
/// native full-side snapshots. It does not assign a Git object identity.
pub(crate) fn file_content_kind_v3(mode: &str, gitlink: bool, content: &[u8]) -> BlobPreviewKind {
    if gitlink {
        BlobPreviewKind::Submodule
    } else if mode == "120000" {
        BlobPreviewKind::Symlink
    } else if let Some((object, size)) = lfs_pointer(content) {
        BlobPreviewKind::LfsPointer { object, size }
    } else if content.contains(&0) || std::str::from_utf8(content).is_err() {
        BlobPreviewKind::Binary
    } else {
        BlobPreviewKind::Text
    }
}

#[derive(Clone, Copy)]
pub(crate) enum FilePreviewSourceV3<'a> {
    Saved(&'a str),
    Index(&'a str),
    ObservedHead(&'a str),
    RawWorktree,
}
/// The one full-file line index. These are read-only byte ranges, NOT patch
/// hunks, review anchors, or changed membership. Avoid a phantom trailing row.
pub(crate) fn preview_line_ranges_v3(
    content: &[u8],
    max_lines: usize,
) -> (Vec<std::ops::Range<usize>>, bool) {
    let cap = max_lines.clamp(1, 10_000);
    let mut ranges = Vec::new();
    let mut offset = 0;
    let mut lines = content.split(|byte| *byte == b'\n').peekable();
    while let Some(line) = lines.next() {
        if line.is_empty() && lines.peek().is_none() && content.ends_with(b"\n") {
            break;
        }
        if ranges.len() >= cap {
            return (ranges, true);
        }
        ranges.push(offset..offset + line.len());
        offset += line.len() + usize::from(lines.peek().is_some());
    }
    (ranges, false)
}
pub(crate) fn content_preview_rows_v3(
    kind: &BlobPreviewKind,
    content: &[u8],
    max_lines: usize,
    source: FilePreviewSourceV3<'_>,
) -> (Vec<FilePreviewLine>, bool) {
    if *kind == BlobPreviewKind::Text {
        let (ranges, truncated) = preview_line_ranges_v3(content, max_lines);
        return (
            ranges
                .into_iter()
                .enumerate()
                .map(|(index, bytes)| FilePreviewLine {
                    line: Some(index as u32 + 1),
                    label: display_bytes(&content[bytes]),
                })
                .collect(),
            truncated,
        );
    }
    let label = match kind {
        BlobPreviewKind::Binary => match source {
            FilePreviewSourceV3::Saved(id) => {
                format!("Binary file: {} bytes; saved object {id}", content.len())
            }
            FilePreviewSourceV3::Index(id) => format!(
                "Binary file: {} bytes; observed index object {id}",
                content.len()
            ),
            FilePreviewSourceV3::ObservedHead(id) => format!(
                "Binary file: {} bytes; observed HEAD object {id}",
                content.len()
            ),
            FilePreviewSourceV3::RawWorktree => format!(
                "Binary file: {} raw unsaved bytes; no Git blob identity",
                content.len()
            ),
        },
        BlobPreviewKind::Symlink => {
            format!("Symbolic link pointing to {}", display_path_bytes(content))
        }
        BlobPreviewKind::Submodule => match source {
            FilePreviewSourceV3::Saved(id) => {
                format!("Submodule: another repository at saved version {id}")
            }
            FilePreviewSourceV3::Index(id) => {
                format!("Submodule: observed index target {id}; nested files were not loaded")
            }
            FilePreviewSourceV3::ObservedHead(id) => {
                format!("Submodule: observed HEAD target {id}; nested files were not loaded")
            }
            FilePreviewSourceV3::RawWorktree => {
                "Submodule working directory; nested version/files were not loaded".into()
            }
        },
        BlobPreviewKind::LfsPointer { object, size } => format!(
            "Git LFS pointer: {size} bytes, sha256:{object}. Actual content has not been fetched."
        ),
        BlobPreviewKind::Text => unreachable!("text was handled by the shared line index"),
    };
    (vec![FilePreviewLine { line: None, label }], false)
}

fn lfs_pointer(content: &[u8]) -> Option<(String, u64)> {
    let value = std::str::from_utf8(content).ok()?;
    let mut lines = value.lines();
    if lines.next()? != "version https://git-lfs.github.com/spec/v1" {
        return None;
    }
    let object = lines.next()?.strip_prefix("oid sha256:")?;
    if object.len() != 64 || !object.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let size = lines.next()?.strip_prefix("size ")?.parse().ok()?;
    Some((object.to_owned(), size))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeRow {
    pub path: RawPath,
    pub object_id: String,
    pub depth: usize,
    pub kind: TreeKind,
    pub expanded: bool,
    pub label: String,
}

/// File-manager order for a saved tree: each directory's children directly
/// below it (even when a sibling such as `src-old` sorts between `src` and
/// `src/` bytewise), directories before files at each level, then raw name
/// bytes.
pub(crate) fn tree_order_v1(left: &TreeEntry, right: &TreeEntry) -> std::cmp::Ordering {
    let a = left.path.0.split(|byte| *byte == b'/').collect::<Vec<_>>();
    let b = right.path.0.split(|byte| *byte == b'/').collect::<Vec<_>>();
    for index in 0..a.len().min(b.len()) {
        if a[index] != b[index] {
            let a_directory = index + 1 < a.len() || left.kind == "tree";
            let b_directory = index + 1 < b.len() || right.kind == "tree";
            return b_directory
                .cmp(&a_directory)
                .then_with(|| a[index].cmp(b[index]));
        }
    }
    a.len().cmp(&b.len())
}

#[derive(Debug, Clone)]
pub struct TreeBrowser {
    pub revision: String,
    entries: Vec<TreeEntry>,
    expanded: BTreeSet<Vec<u8>>,
    /// The last file opened from this tree; returning to the tree selects it.
    pub(crate) focus: Option<RawPath>,
}

impl TreeBrowser {
    pub fn new(revision: String, mut entries: Vec<TreeEntry>) -> Self {
        entries.sort_by(tree_order_v1);
        Self {
            revision,
            entries,
            expanded: BTreeSet::new(),
            focus: None,
        }
    }

    pub(crate) fn expanded_paths_v1(&self) -> impl Iterator<Item = &[u8]> {
        self.expanded.iter().map(Vec::as_slice)
    }
    pub fn toggle(&mut self, path: &RawPath) -> bool {
        if !self
            .entries
            .iter()
            .any(|entry| entry.path == *path && entry.kind == "tree")
        {
            return false;
        }
        if !self.expanded.remove(&path.0) {
            self.expanded.insert(path.0.clone());
        }
        true
    }

    /// Merge a lazily loaded directory without losing expanded or selected identities.
    pub fn add_children(&mut self, entries: Vec<TreeEntry>) {
        for entry in entries {
            if !self
                .entries
                .iter()
                .any(|existing| existing.path == entry.path)
            {
                self.entries.push(entry);
            }
        }
        self.entries.sort_by(tree_order_v1);
    }

    pub fn visible_rows(&self) -> Vec<TreeRow> {
        self.entries
            .iter()
            .filter(|entry| self.parents_expanded(&entry.path))
            .map(|entry| {
                let kind = match (entry.kind.as_str(), entry.mode.as_str()) {
                    ("tree", _) => TreeKind::Directory,
                    ("commit", _) => TreeKind::Submodule,
                    (_, "120000") => TreeKind::Symlink,
                    _ => TreeKind::File,
                };
                let expanded = self.expanded.contains(&entry.path.0);
                let name = entry
                    .path
                    .0
                    .rsplit(|byte| *byte == b'/')
                    .next()
                    .unwrap_or_default();
                let marker = match kind {
                    TreeKind::Directory => {
                        if expanded {
                            "▾"
                        } else {
                            "▸"
                        }
                    }
                    TreeKind::File => "·",
                    TreeKind::Symlink => "↗",
                    TreeKind::Submodule => "◇",
                };
                let depth = entry.path.0.iter().filter(|byte| **byte == b'/').count();
                TreeRow {
                    path: entry.path.clone(),
                    object_id: entry.id.clone(),
                    depth,
                    kind,
                    expanded,
                    label: format!(
                        "{}{} {}{}",
                        "  ".repeat(depth),
                        marker,
                        display_path_bytes(name),
                        if kind == TreeKind::Submodule {
                            " [submodule]"
                        } else {
                            ""
                        }
                    ),
                }
            })
            .collect()
    }

    fn parents_expanded(&self, path: &RawPath) -> bool {
        path.0
            .iter()
            .enumerate()
            .filter(|(_, byte)| **byte == b'/')
            .all(|(offset, _)| self.expanded.contains(&path.0[..offset]))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffSide {
    Old,
    New,
    Both,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffAnchor {
    pub base: Option<String>,
    pub head: Option<String>,
    pub old_path: Option<RawPath>,
    pub new_path: Option<RawPath>,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    pub side: DiffSide,
    pub file_index: usize,
    pub hunk_index: usize,
    pub line_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffCell {
    pub label: String,
    pub role: SemanticRole,
    pub anchor: Option<DiffAnchor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffDisplayRow {
    pub left: DiffCell,
    pub right: Option<DiffCell>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLayout {
    Unified,
    Split,
}

/// Changing display layout never changes canonical review/staging coordinates.
pub fn diff_rows(diff: &Diff, layout: DiffLayout, show_context: bool) -> Vec<DiffDisplayRow> {
    diff_rows_for_files_v1(diff, layout, show_context, diff.files.iter().enumerate())
}
pub(crate) fn diff_rows_for_files_v1<'a>(
    diff: &'a Diff,
    layout: DiffLayout,
    show_context: bool,
    files: impl IntoIterator<Item = (usize, &'a crate::diff::FileDiff)>,
) -> Vec<DiffDisplayRow> {
    diff_rows_for_members_plain_v1(
        (diff.base.as_deref(), diff.head.as_deref()),
        layout,
        show_context,
        files,
        true,
    )
}
/// Same canonical pairing implementation, borrowing actual source endpoints and
/// members; remote files need not masquerade as a local ComparisonResult.
/// The native patch view keeps the original `old → new` heading and every raw
/// Git header line.
pub(crate) fn diff_rows_for_members_v3<'a>(
    endpoints: (Option<&str>, Option<&str>),
    layout: DiffLayout,
    show_context: bool,
    files: impl IntoIterator<Item = (usize, &'a crate::diff::FileDiff)>,
) -> Vec<DiffDisplayRow> {
    diff_rows_for_members_plain_v1(endpoints, layout, show_context, files, false)
}
/// `plain` names each file plainly (see `diff_file_title_v1`) and leaves out
/// raw header lines that heading already says.
fn diff_rows_for_members_plain_v1<'a>(
    endpoints: (Option<&str>, Option<&str>),
    layout: DiffLayout,
    show_context: bool,
    files: impl IntoIterator<Item = (usize, &'a crate::diff::FileDiff)>,
    plain: bool,
) -> Vec<DiffDisplayRow> {
    let mut rows = Vec::new();
    for (file_index, file) in files {
        let title = if plain {
            diff_file_title_v1(file)
        } else {
            format!(
                "{} → {}",
                file.old_path
                    .as_deref()
                    .map(display_path_bytes)
                    .unwrap_or_else(|| "new file".into()),
                file.new_path
                    .as_deref()
                    .map(display_path_bytes)
                    .unwrap_or_else(|| "deleted file".into())
            )
        };
        rows.push(DiffDisplayRow {
            left: DiffCell {
                label: format!(
                    "{title}{}",
                    if file.binary {
                        " [binary: no text preview]"
                    } else {
                        ""
                    }
                ),
                role: SemanticRole::Reference,
                anchor: None,
            },
            right: None,
        });
        for metadata in file
            .metadata
            .iter()
            .filter(|line| !plain || !diff_metadata_in_title_v1(line))
        {
            rows.push(DiffDisplayRow {
                left: DiffCell {
                    label: display_bytes(metadata),
                    role: SemanticRole::Changed,
                    anchor: None,
                },
                right: None,
            });
        }
        for (hunk_index, hunk) in file.hunks.iter().enumerate() {
            rows.push(DiffDisplayRow {
                left: DiffCell {
                    label: format!(
                        "@@ -{},{} +{},{} @@ {}",
                        hunk.old_start,
                        hunk.old_count,
                        hunk.new_start,
                        hunk.new_count,
                        display_bytes(&hunk.heading)
                    ),
                    role: SemanticRole::Reference,
                    anchor: None,
                },
                right: None,
            });
            let mut line_index = 0;
            while line_index < hunk.lines.len() {
                let line = &hunk.lines[line_index];
                if !show_context && line.kind == LineKind::Context {
                    line_index += 1;
                    continue;
                }
                if layout == DiffLayout::Split && line.kind == LineKind::Removed {
                    let start = line_index;
                    while line_index < hunk.lines.len()
                        && hunk.lines[line_index].kind == LineKind::Removed
                    {
                        line_index += 1;
                    }
                    let added_start = line_index;
                    while line_index < hunk.lines.len()
                        && hunk.lines[line_index].kind == LineKind::Added
                    {
                        line_index += 1;
                    }
                    let removed_count = added_start - start;
                    let added_count = line_index - added_start;
                    for offset in 0..removed_count.max(added_count) {
                        let left = if offset < removed_count {
                            diff_cell(endpoints, file, file_index, hunk_index, start + offset)
                        } else {
                            empty_diff_cell()
                        };
                        let right = if offset < added_count {
                            diff_cell(
                                endpoints,
                                file,
                                file_index,
                                hunk_index,
                                added_start + offset,
                            )
                        } else {
                            empty_diff_cell()
                        };
                        rows.push(DiffDisplayRow {
                            left,
                            right: Some(right),
                        });
                    }
                } else {
                    let cell = diff_cell(endpoints, file, file_index, hunk_index, line_index);
                    let (left, right) = match (layout, line.kind) {
                        (DiffLayout::Split, LineKind::Added) => (empty_diff_cell(), Some(cell)),
                        (DiffLayout::Split, LineKind::Context) => (cell.clone(), Some(cell)),
                        _ => (cell, None),
                    };
                    rows.push(DiffDisplayRow { left, right });
                    line_index += 1;
                }
            }
        }
    }
    rows
}

/// Plain file heading: the path for an edit, `Added:`/`Deleted:` for a
/// created/removed file and `Renamed: old → new` only for a real rename.
pub(crate) fn diff_file_title_v1(file: &crate::diff::FileDiff) -> String {
    match (file.old_path.as_deref(), file.new_path.as_deref()) {
        (Some(old), Some(new)) if old == new => display_path_bytes(new),
        (Some(old), Some(new)) => format!(
            "Renamed: {} → {}",
            display_path_bytes(old),
            display_path_bytes(new)
        ),
        (None, Some(new)) => format!("Added: {}", display_path_bytes(new)),
        (Some(old), None) => format!("Deleted: {}", display_path_bytes(old)),
        (None, None) => "File with unavailable name".into(),
    }
}
/// Header rows `diff_rows` shows for `file` (plain headings omit some).
pub(crate) fn diff_plain_metadata_len_v1(file: &crate::diff::FileDiff) -> usize {
    file.metadata
        .iter()
        .filter(|line| !diff_metadata_in_title_v1(line))
        .count()
}
/// Raw Git header lines already expressed by the plain file heading (object
/// IDs, rename/similarity bookkeeping, create/delete modes) are not repeated.
fn diff_metadata_in_title_v1(line: &[u8]) -> bool {
    [
        b"index ".as_slice(),
        b"similarity index ",
        b"dissimilarity index ",
        b"rename from ",
        b"rename to ",
        b"new file mode ",
        b"deleted file mode ",
    ]
    .iter()
    .any(|prefix| line.starts_with(prefix))
}
fn empty_diff_cell() -> DiffCell {
    DiffCell {
        label: String::new(),
        role: SemanticRole::Normal,
        anchor: None,
    }
}
fn diff_cell(
    endpoints: (Option<&str>, Option<&str>),
    file: &crate::diff::FileDiff,
    file_index: usize,
    hunk_index: usize,
    line_index: usize,
) -> DiffCell {
    let line = &file.hunks[hunk_index].lines[line_index];
    let (prefix, role, side) = match line.kind {
        LineKind::Added => ('+', SemanticRole::Added, DiffSide::New),
        LineKind::Removed => ('-', SemanticRole::Removed, DiffSide::Old),
        LineKind::Context => (' ', SemanticRole::Normal, DiffSide::Both),
        LineKind::NoNewline => ('\\', SemanticRole::Boundary, DiffSide::Both),
    };
    let anchor = if line.kind == LineKind::NoNewline {
        None
    } else {
        Some(DiffAnchor {
            base: endpoints.0.map(str::to_owned),
            head: endpoints.1.map(str::to_owned),
            old_path: file.old_path.clone().map(RawPath),
            new_path: file.new_path.clone().map(RawPath),
            old_line: line.old_line,
            new_line: line.new_line,
            side,
            file_index,
            hunk_index,
            line_index,
        })
    };
    DiffCell {
        label: format!(
            "{:>5} {:>5} {prefix}{}",
            line.old_line.map(|n| n.to_string()).unwrap_or_default(),
            line.new_line.map(|n| n.to_string()).unwrap_or_default(),
            display_bytes(&line.text)
        ),
        role,
        anchor,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceRow {
    pub path: Option<RawPath>,
    pub label: String,
    pub role: SemanticRole,
}

/// Prominent paused merge/rebase/cherry-pick/revert state with the next step
/// and clickable Continue/Skip/Abort rows (the same LocalCommand forms as the
/// Local operations menu). Bisect keeps its own guided rows.
pub fn paused_operation_rows_v1(
    status: &RepoStatus,
) -> Vec<(
    String,
    SemanticRole,
    Option<crate::local_actions::LocalCommand>,
)> {
    use crate::git::Sequencer;
    use crate::local_actions::LocalCommand;
    let Some(operation) = &status.operation else {
        // A conflicted stash apply/pop has no sequencer state; the stash is kept.
        let conflicts = status.conflicts();
        if conflicts == 0 {
            return vec![];
        }
        return vec![
            (
                format!(
                    "⚠ {conflicts} unresolved conflict{} without a paused operation (for example a stash apply) • a conflicted stash is kept",
                    if conflicts == 1 { "" } else { "s" }
                ),
                SemanticRole::Conflict,
                None,
            ),
            (
                "Next: open each [conflicted] file below, choose ours/theirs or edit it, then Stage selected file (m)".into(),
                SemanticRole::Boundary,
                None,
            ),
        ];
    };
    let name = crate::local_availability::operation_name_v1(operation);
    if *operation == Sequencer::Bisect {
        // Also shown after ghtui restarts mid-search: Git keeps the bisect state until
        // End bug search, so the recovery choices stay one click or Enter away.
        return vec![
            (
                "Bug search in progress • mark versions good/bad/skip or End bug search (m)".into(),
                SemanticRole::Conflict,
                None,
            ),
            (
                "Next: test the current saved version, then mark it • an interrupted search resumes here".into(),
                SemanticRole::Boundary,
                None,
            ),
            (
                "[Mark version good] • the bug is absent at the current version".into(),
                SemanticRole::Conflict,
                Some(LocalCommand::BisectGood),
            ),
            (
                "[Mark version bad] • the bug is present at the current version".into(),
                SemanticRole::Conflict,
                Some(LocalCommand::BisectBad),
            ),
            (
                "[Skip untestable version] • choose another nearby candidate".into(),
                SemanticRole::Conflict,
                Some(LocalCommand::BisectSkip),
            ),
            (
                "[End bug search] • restore the starting branch and version".into(),
                SemanticRole::Conflict,
                Some(LocalCommand::BisectReset),
            ),
        ];
    }
    let conflicts = status.conflicts();
    let upper = name.to_ascii_uppercase();
    let mut rows = vec![(
        if conflicts > 0 {
            format!(
                "⚠ {upper} PAUSED • {conflicts} unresolved conflict{} • working files are not committed yet",
                if conflicts == 1 { "" } else { "s" }
            )
        } else {
            format!("⚠ {upper} PAUSED • all conflicts resolved and staged")
        },
        SemanticRole::Conflict,
        None,
    )];
    rows.push((
        if conflicts > 0 {
            "Next: open each [conflicted] file below, choose ours/theirs or edit it, then Stage selected file (m)".into()
        } else {
            format!("Next: Continue to finish the {name}, or Abort to return to the state before it")
        },
        SemanticRole::Boundary,
        None,
    ));
    rows.push((
        if conflicts > 0 {
            format!("[Continue {name}] • available after every conflicted file is staged")
        } else {
            format!("[Continue {name}] • record the resolved result")
        },
        SemanticRole::Conflict,
        Some(LocalCommand::Continue),
    ));
    if *operation != Sequencer::Merge {
        rows.push((
            "[Skip this commit] • drop the current commit's changes and move on".into(),
            SemanticRole::Conflict,
            Some(LocalCommand::Skip),
        ));
    }
    rows.push((
        format!("[Abort {name}] • restore the branch and files from before the {name} started"),
        SemanticRole::Conflict,
        Some(LocalCommand::Abort),
    ));
    rows
}

/// Plain-language kind of an unmerged path from its porcelain-v2 XY code,
/// plus how to resolve it. `aside_of` is the directory Git found in the way
/// when it moved this file aside to `<directory>~<side>`.
pub fn conflict_kind_v1(index: char, worktree: char, aside_of: Option<&str>) -> Vec<String> {
    let kind = match (index, worktree) {
        ('U', 'U') => "Content conflict: both sides changed this file",
        ('A', 'A') => "Add/add conflict: both sides added a different file at this path",
        ('D', 'U') => "Delete conflict: deleted on our side, kept or renamed to this path on theirs (modify/delete or rename/delete)",
        ('U', 'D') => "Delete conflict: changed on our side, deleted or renamed away on theirs (modify/delete or rename/delete)",
        ('A', 'U') => "Added only on our side: usually a rename or file/directory conflict",
        ('U', 'A') => "Added only on their side: usually a rename or file/directory conflict",
        ('D', 'D') => "Deleted on both sides: each side renamed it elsewhere (rename/rename)",
        _ => "Unmerged path",
    };
    let mut lines = vec![kind.to_string()];
    if let Some(directory) = aside_of {
        lines.push(format!(
            "File/directory conflict: a directory now occupies {directory}, so Git moved this file aside to this name. Resolve it here, then keep, rename or delete it."
        ));
    }
    if matches!((index, worktree), ('D', _) | (_, 'D')) {
        lines.push(
            "Keep the file: choose the remaining version, then Stage. Drop it: delete the working file, then Stage selected file records the removal.".into(),
        );
    }
    lines
}

/// The path a file/directory conflict moved `path` aside from: Git names the
/// moved file `<path>~<side>`. Pure; the caller checks that it is a directory.
pub fn conflict_aside_prefix_v1(path: &[u8]) -> Option<&[u8]> {
    let tilde = path.iter().rposition(|byte| *byte == b'~')?;
    let name_start = path[..tilde]
        .iter()
        .rposition(|byte| *byte == b'/')
        .map_or(0, |slash| slash + 1);
    (tilde > name_start && !path[tilde + 1..].is_empty()).then(|| &path[..tilde])
}

/// A commit HEAD left behind by an amend, reset or rebase, from the raw local
/// HEAD reflog file. It may still be reachable elsewhere or already expired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RescueCandidateV1 {
    pub id: String,
    pub selector: String,
    pub reason: &'static str,
    pub subject: String,
}

/// Pure, bounded parse of `logs/HEAD` (oldest line first). Only the "old" side
/// of amend/reset/rebase movements is offered; ids in `exclude` (already
/// visible recent history) and duplicates are skipped. Newest first.
pub fn rescue_candidates_v1(
    log: &[u8],
    exclude: &[String],
    limit: usize,
) -> Vec<RescueCandidateV1> {
    rescue_candidates_for_ref_v1(log, "HEAD", exclude, limit)
}

/// The same parse for any local reflog (`HEAD` or a branch's
/// `logs/refs/heads/<name>`). A branch reflog also records moves HEAD never
/// saw: `git branch -f` ("branch: Reset to") and the finished rebase of a
/// branch ("rebase (finish)"), whose old side is the pre-rebase tip.
pub fn rescue_candidates_for_ref_v1(
    log: &[u8],
    reference: &str,
    exclude: &[String],
    limit: usize,
) -> Vec<RescueCandidateV1> {
    let branch = reference != "HEAD";
    let lines: Vec<(&str, &str, &str)> = log
        .split(|byte| *byte == b'\n')
        .filter_map(|line| std::str::from_utf8(line).ok())
        .filter_map(|line| {
            let (head, message) = line.split_once('\t')?;
            let mut fields = head.split(' ');
            let old = fields.next()?;
            let new = fields.next()?;
            (old.len() >= 40 && new.len() >= 40 && old.bytes().all(|b| b.is_ascii_hexdigit()))
                .then_some((old, new, message))
        })
        .collect();
    let total = lines.len();
    let mut seen: Vec<&str> = vec![];
    let mut result = vec![];
    for (index, (old, _, message)) in lines.iter().enumerate().rev() {
        let reason = if message.starts_with("commit (amend)") {
            "before amend"
        } else if message.starts_with("reset:") {
            "before reset"
        } else if message.starts_with("rebase")
            && (message.contains("(start)") || branch && message.contains("(finish)"))
        {
            "before rebase"
        } else if branch && message.starts_with("branch: Reset to") {
            "before branch move"
        } else {
            continue;
        };
        if old.bytes().all(|byte| byte == b'0')
            || seen.contains(old)
            || exclude.iter().any(|id| id == old)
        {
            continue;
        }
        seen.push(old);
        let subject = lines[..index]
            .iter()
            .rev()
            .find(|(_, new, _)| new == old)
            .map(|(_, _, message)| {
                message
                    .split_once(": ")
                    .map_or(*message, |(_, subject)| subject)
            })
            .unwrap_or("");
        result.push(RescueCandidateV1 {
            id: (*old).to_owned(),
            selector: format!("{reference}@{{{}}}", total - 1 - index),
            reason,
            subject: crate::providers::runner::sanitize_terminal(subject),
        });
        if result.len() >= limit {
            break;
        }
    }
    result
}

pub fn workspace_rows(status: &RepoStatus) -> Vec<WorkspaceRow> {
    let branch = status.branch.as_deref().unwrap_or(if status.detached {
        "detached HEAD"
    } else {
        "no branch"
    });
    let mut rows = vec![WorkspaceRow {
        path: None,
        label: format!(
            "Branch: {branch} | saved version: {}{}",
            status.head.as_deref().map(short_id).unwrap_or("none yet"),
            if status.branch.is_some() && status.upstream.is_none() && status.head.is_some() {
                " | no upstream: Push with Set tracking branch to publish"
            } else {
                ""
            }
        ),
        role: SemanticRole::Head,
    }];
    if status.has_history_overrides() {
        rows.push(WorkspaceRow {
            path: None,
            label: format!(
                "History overrides: {} replacement refs; legacy graft file {}",
                status.replacement_refs,
                if status.grafted { "present" } else { "absent" }
            ),
            role: SemanticRole::Boundary,
        });
        rows.push(WorkspaceRow {
            path: None,
            label: "Saved graph/tree/diffs show original objects, not replacement/graft views."
                .into(),
            role: SemanticRole::Boundary,
        });
        rows.push(WorkspaceRow{path:None,label:"History writes are guarded. Staging and explicit fetch keep installed-Git semantics.".into(),role:SemanticRole::Boundary});
    }
    if let Some(upstream) = &status.upstream {
        rows.push(WorkspaceRow {
            path: None,
            label: if status.has_history_overrides() {
                format!(
                    "Git-effective upstream {upstream}: ahead {}, behind {} (overrides active)",
                    status.ahead, status.behind
                )
            } else {
                format!(
                    "Upstream {upstream}: {} commits to send, {} commits to receive{}",
                    status.ahead,
                    status.behind,
                    match (status.ahead, status.behind) {
                        (0, 0) => " • up to date as of last fetch",
                        (_, 0) => " • push to share",
                        (0, _) => " • behind: pull to update",
                        _ => " • DIVERGED: pull with merge or rebase before pushing",
                    }
                )
            },
            role: SemanticRole::Reference,
        });
    }
    if status.shallow {
        rows.push(WorkspaceRow {
            path: None,
            label: "Shallow clone: earlier saved versions may be unavailable".into(),
            role: SemanticRole::Boundary,
        });
    }
    if status.sparse {
        rows.push(WorkspaceRow {
            path: None,
            label: "Sparse checkout: some tracked files are hidden from working files".into(),
            role: SemanticRole::Boundary,
        });
    }
    let paused = paused_operation_rows_v1(status);
    for (label, role, _) in paused.iter().filter(|row| row.2.is_none()).cloned() {
        rows.push(WorkspaceRow {
            path: None,
            label,
            role,
        });
    }
    if status.entries.is_empty() {
        rows.push(WorkspaceRow {
            path: None,
            label: "Working files are clean; no uncommitted changes".into(),
            role: SemanticRole::Normal,
        });
    }
    for entry in &status.entries {
        let role = if entry.conflict {
            SemanticRole::Conflict
        } else if entry.untracked() || entry.index == 'A' {
            SemanticRole::Added
        } else if entry.index == 'D' || entry.worktree == 'D' {
            SemanticRole::Removed
        } else {
            SemanticRole::Changed
        };
        let state = if entry.conflict {
            "conflicted"
        } else if entry.staged() && entry.worktree != '.' {
            "staged + unstaged"
        } else if entry.staged() {
            "staged"
        } else if entry.untracked() {
            "untracked"
        } else {
            "unstaged"
        };
        let rename = entry
            .original_path
            .as_ref()
            .map(|path| format!(" (from {})", path.display()))
            .unwrap_or_default();
        rows.push(WorkspaceRow {
            path: Some(entry.path.clone()),
            label: format!(
                "{} [{state}] {}{rename}{}",
                entry.label(),
                entry.path.display(),
                if entry.conflict {
                    " • Enter: compare base/ours/theirs"
                } else {
                    ""
                }
            ),
            role,
        });
    }
    rows.extend(
        paused
            .into_iter()
            .filter(|row| row.2.is_some())
            .map(|(label, role, _)| WorkspaceRow {
                path: None,
                label,
                role,
            }),
    );
    rows
}

/// Selection preserves row identity and cannot create GitHub anchors for metadata or binary lines.
pub fn selected_anchors(
    rows: &[DiffDisplayRow],
    start: usize,
    end: usize,
    side: DiffSide,
) -> Vec<DiffAnchor> {
    let mut result = Vec::new();
    for row in rows
        .iter()
        .take(start.max(end).saturating_add(1))
        .skip(start.min(end))
    {
        for cell in [&row.left, row.right.as_ref().unwrap_or(&row.left)] {
            if let Some(anchor) = &cell.anchor {
                if (side == DiffSide::Both || anchor.side == side || anchor.side == DiffSide::Both)
                    && !result.contains(anchor)
                {
                    result.push(anchor.clone());
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn revision_tree_expand_collapse_keeps_raw_identity() {
        let entries = vec![
            TreeEntry {
                path: RawPath::new(b"dir".to_vec()),
                mode: "040000".into(),
                kind: "tree".into(),
                id: "tree".into(),
            },
            TreeEntry {
                path: RawPath::new(b"dir/tab\t\xff".to_vec()),
                mode: "100644".into(),
                kind: "blob".into(),
                id: "blob".into(),
            },
        ];
        let mut tree = TreeBrowser::new("immutable-revision".into(), entries);
        assert_eq!(tree.visible_rows().len(), 1);
        assert!(tree.toggle(&RawPath::new(b"dir".to_vec())));
        let rows = tree.visible_rows();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].path.0, b"dir/tab\t\xff");
        assert_eq!(rows[1].depth, 1);
        assert!(tree.toggle(&RawPath::new(b"dir".to_vec())));
        assert_eq!(tree.visible_rows().len(), 1);
    }
    #[test]
    fn native_diff_split_and_unified_preserve_review_coordinates() {
        let mut diff = Diff::parse(
            b"diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -3,2 +3,2 @@\n same\n-old\n+new\n",
        )
        .unwrap();
        diff.base = Some("base".into());
        diff.head = Some("head".into());
        let unified = diff_rows(&diff, DiffLayout::Unified, true);
        let split = diff_rows(&diff, DiffLayout::Split, true);
        let left = selected_anchors(&unified, 0, unified.len() - 1, DiffSide::Old);
        let right = selected_anchors(&split, 0, split.len() - 1, DiffSide::Old);
        assert_eq!(left, right);
        assert_eq!(left[1].old_line, Some(4));
        assert_eq!(left[1].side, DiffSide::Old);
        assert_eq!(left[1].base.as_deref(), Some("base"));
        let added = selected_anchors(&split, 0, split.len() - 1, DiffSide::New);
        assert_eq!(added[1].new_line, Some(4));
        assert_eq!(added[1].new_path, Some(RawPath::new(b"x".to_vec())));
    }
    #[test]
    fn local_workspace_labels_unborn_and_incomplete_history() {
        let status = RepoStatus {
            branch: Some("main".into()),
            shallow: true,
            sparse: true,
            ..RepoStatus::default()
        };
        let rows = workspace_rows(&status);
        assert!(rows[0].label.contains("none yet"));
        assert!(rows
            .iter()
            .any(|row| row.label.contains("earlier saved versions")));
        assert!(rows.iter().any(|row| row.label.contains("Sparse checkout")));
        assert!(rows.iter().any(|row| row.label.contains("clean")));
    }

    #[test]
    fn revision_tree_preview_labels_special_objects_and_bounds_text() {
        let entry = TreeEntry {
            path: RawPath::new(b"file".to_vec()),
            id: "blob-id".into(),
            kind: "blob".into(),
            mode: "100644".into(),
        };
        let preview = file_preview(
            "saved-version".into(),
            &entry,
            b"one\ntwo\nthree\n".to_vec(),
            2,
        );
        assert!(preview.truncated);
        assert_eq!(preview.rows.len(), 2);
        assert_eq!(preview.rows[1].line, Some(2));
        assert_eq!(preview.revision, "saved-version");
        let preview = file_preview("saved-version".into(), &entry, b"binary\0".to_vec(), 2);
        assert_eq!(preview.kind, BlobPreviewKind::Binary);
        assert!(preview.rows[0].label.contains("7 bytes"));
        let pointer = format!(
            "version https://git-lfs.github.com/spec/v1\noid sha256:{}\nsize 1234\n",
            "a".repeat(64)
        );
        let preview = file_preview("saved-version".into(), &entry, pointer.into_bytes(), 2);
        assert_eq!(
            preview.kind,
            BlobPreviewKind::LfsPointer {
                object: "a".repeat(64),
                size: 1234
            }
        );
        assert!(preview.rows[0].label.contains("not been fetched"));
    }
}
