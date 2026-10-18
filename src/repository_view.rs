//! Prepared repository presentation models. These contain no subprocess or filesystem work.
//! Native graph lanes are temporary tracing aids; refs label actual branch identities.

mod tree_merge_v1;
use crate::diff::{Diff, LineKind};
use crate::git::{
    display_bytes, display_path_bytes, Commit, GitActor, RawPath, RepoStatus, TreeEntry,
};
use std::collections::{BTreeSet, HashMap, HashSet};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphEdge {
    pub from_column: usize,
    pub to_column: usize,
    pub parent_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphRow {
    pub commit: Commit,
    pub column: usize,
    pub lanes_before: Vec<String>,
    pub lanes_after: Vec<String>,
    /// Palette slots follow continuous parent paths; lane collapse does not recolor neighbors.
    pub colors_before: Vec<usize>,
    pub colors_after: Vec<usize>,
    pub edges: Vec<GraphEdge>,
    pub glyphs: String,
    pub explanation: String,
}

#[derive(Debug, Clone, Default)]
pub struct CommitGraph {
    lanes: Vec<String>,
    loaded: HashSet<String>,
    colors: HashMap<String, usize>,
    next_color: usize,
    pub rows: Vec<GraphRow>,
}

impl CommitGraph {
    /// Append a topologically ordered page. Pending lane identities survive pagination.
    pub fn append(&mut self, commits: &[Commit]) {
        for commit in commits {
            if !self.loaded.insert(commit.id.clone()) {
                continue;
            }
            let column = match self.lanes.iter().position(|id| id == &commit.id) {
                Some(column) => column,
                None => {
                    self.colors.insert(commit.id.clone(), self.next_color % 8);
                    self.next_color += 1;
                    self.lanes.push(commit.id.clone());
                    self.lanes.len() - 1
                }
            };
            let lanes_before = self.lanes.clone();
            let colors_before = self
                .lanes
                .iter()
                .map(|id| self.colors[id])
                .collect::<Vec<_>>();
            let commit_color = self.colors.remove(&commit.id).unwrap_or_default();
            self.lanes.remove(column);
            for (index, parent) in commit.parents.iter().enumerate() {
                if self.lanes.contains(parent) || self.loaded.contains(parent) {
                    continue;
                }
                if index == 0 {
                    self.colors.insert(parent.clone(), commit_color);
                    self.lanes
                        .insert(column.min(self.lanes.len()), parent.clone());
                } else {
                    self.colors.insert(parent.clone(), self.next_color % 8);
                    self.next_color += 1;
                    self.lanes.push(parent.clone());
                }
            }
            let edges = commit
                .parents
                .iter()
                .filter_map(|parent| {
                    self.lanes
                        .iter()
                        .position(|id| id == parent)
                        .map(|to_column| GraphEdge {
                            from_column: column,
                            to_column,
                            parent_id: parent.clone(),
                        })
                })
                .collect();
            let mut glyphs = String::new();
            for index in 0..lanes_before.len() {
                glyphs.push(if index == column { '●' } else { '│' });
                glyphs.push(' ');
            }
            if commit.parents.len() > 1 {
                glyphs.push('╲');
            }
            self.rows.push(GraphRow {
                commit: commit.clone(),
                column,
                lanes_before,
                lanes_after: self.lanes.clone(),
                colors_before,
                colors_after: self.lanes.iter().map(|id| self.colors[id]).collect(),
                edges,
                glyphs,
                explanation: explain_commit(commit),
            });
        }
    }

    /// Unloaded parent identities are boundaries, never silently presented as root commits.
    pub fn boundaries(&self) -> &[String] {
        &self.lanes
    }

    pub fn visible_ancestors(&self, selected: &str) -> HashSet<String> {
        let commits = self
            .rows
            .iter()
            .map(|row| (row.commit.id.as_str(), &row.commit))
            .collect::<HashMap<_, _>>();
        let mut stack = vec![selected.to_owned()];
        let mut ancestors = HashSet::new();
        while let Some(id) = stack.pop() {
            if ancestors.insert(id.clone()) {
                if let Some(commit) = commits.get(id.as_str()) {
                    stack.extend(commit.parents.iter().cloned());
                }
            }
        }
        ancestors
    }

    pub fn row_label(row: &GraphRow) -> String {
        let short = &row.commit.id[..row.commit.id.len().min(8)];
        let refs = if row.commit.refs.is_empty() {
            String::new()
        } else {
            format!(" [{}]", row.commit.refs.join(", "))
        };
        format!(
            "{}{} {} — {}{}",
            row.glyphs,
            short,
            row.commit.subject,
            crate::git_actor_labels_v1::commit_summary(&row.commit),
            refs
        )
    }
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
        entries.sort_by(|left, right| left.path.0.cmp(&right.path.0));
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
        self.entries
            .sort_by(|left, right| left.path.0.cmp(&right.path.0));
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

pub fn workspace_rows(status: &RepoStatus) -> Vec<WorkspaceRow> {
    let branch = status.branch.as_deref().unwrap_or(if status.detached {
        "detached HEAD"
    } else {
        "no branch"
    });
    let mut rows = vec![WorkspaceRow {
        path: None,
        label: format!(
            "Branch: {branch} | saved version: {}",
            status.head.as_deref().map(short_id).unwrap_or("none yet")
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
                    "Upstream {upstream}: {} commits to send, {} commits to receive",
                    status.ahead, status.behind
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
    if let Some(operation) = &status.operation {
        rows.push(WorkspaceRow {
            path: None,
            label: format!(
                "Operation in progress: {operation:?}; resolve conflicts, continue or abort"
            ),
            role: SemanticRole::Conflict,
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
                "{} [{state}] {}{rename}",
                entry.label(),
                entry.path.display()
            ),
            role,
        });
    }
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
    fn commit(id: &str, parents: &[&str]) -> Commit {
        Commit {
            id: id.into(),
            parents: parents.iter().map(|id| (*id).into()).collect(),
            author: GitActor {
                name: "Local Author".into(),
                email: "author@example.test".into(),
                timestamp: 0,
            },
            committer: GitActor {
                name: "Other Committer".into(),
                email: "committer@example.test".into(),
                timestamp: 0,
            },
            subject: format!("Saved {id}"),
            refs: Vec::new(),
            raw_refs: Vec::new(),
        }
    }
    #[test]
    fn commit_graph_native_merge_edges_survive_page_boundaries() {
        let commits = [
            commit("merge", &["left", "right"]),
            commit("left", &["root"]),
            commit("right", &["root"]),
            commit("root", &[]),
        ];
        let mut graph = CommitGraph::default();
        graph.append(&commits[..2]);
        assert_eq!(graph.boundaries(), &["root".to_owned(), "right".to_owned()]);
        let first_rows = graph.rows.clone();
        graph.append(&commits[2..]);
        assert_eq!(&graph.rows[..2], first_rows);
        assert!(graph.boundaries().is_empty());
        assert_eq!(graph.rows[0].edges.len(), 2);
        assert_eq!(graph.rows[2].edges[0].parent_id, "root");
        assert_eq!(
            graph.visible_ancestors("left"),
            HashSet::from(["left".into(), "root".into()])
        );
        assert!(graph.rows[0].explanation.contains("joins 2 histories"));
    }
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
