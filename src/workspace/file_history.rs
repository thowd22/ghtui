//! Native immutable-file provenance; all reads run on Workspace workers.
use super::*;
use crate::{
    git::{FileBlameLine, FileHistoryEntry, FileHistoryPage},
    repository_view::BlobPreviewKind,
};

#[path = "file_history/original_saved_restore_v1.rs"]
mod original_saved_restore_v1;
const HISTORY_PAGE: usize = 10;
const BLAME_PAGE: usize = 100;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvenanceView {
    File,
    History,
    Blame,
    Detail,
    Graph,
    Diff,
    OriginalFile,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileAction {
    History,
    Blame,
    BlameAt(u32),
    HistoryEntry(String),
    BlameLine(u32),
    Previous,
    Next,
    Graph,
    Diff,
    OriginalFile,
    Back,
}
#[derive(Debug, Clone)]
pub struct FileSelection {
    pub commit: String,
    pub path: Option<RawPath>,
    pub original_line: Option<u32>,
    pub final_line: Option<u32>,
    pub boundary: bool,
    pub exists: bool,
}
#[derive(Debug, Clone)]
pub struct FileProvenance {
    pub revision: String,
    pub path: RawPath,
    pub view: ProvenanceView,
    pub history: Option<FileHistoryPage>,
    pub blame: Vec<FileBlameLine>,
    pub blame_start: usize,
    pub line_count: usize,
    pub selection: Option<FileSelection>,
    pub return_view: ProvenanceView,
}
impl FileProvenance {
    pub fn new(revision: String, path: RawPath) -> Self {
        Self {
            revision,
            path,
            view: ProvenanceView::File,
            history: None,
            blame: vec![],
            blame_start: 1,
            line_count: 0,
            selection: None,
            return_view: ProvenanceView::File,
        }
    }
    pub fn can_previous(&self) -> bool {
        match self.view {
            ProvenanceView::History => self.history.as_ref().is_some_and(|page| page.offset > 0),
            ProvenanceView::Blame => self.blame_start > 1,
            _ => false,
        }
    }
    pub fn can_next(&self) -> bool {
        match self.view {
            ProvenanceView::History => self
                .history
                .as_ref()
                .is_some_and(|page| page.has_more && page.offset + page.entries.len() < 10_000),
            ProvenanceView::Blame => {
                self.blame_start.saturating_add(self.blame.len()) <= self.line_count
            }
            _ => false,
        }
    }
}
impl Workspace {
    pub fn provenance_focus_row(&self) -> Option<usize> {
        let state = self.file_provenance.as_ref()?;
        let selected = state.selection.as_ref();
        let row=match state.view {
            ProvenanceView::Graph=>return self.graph_focus_row_v1(),
            ProvenanceView::History=>self.rows.iter().position(|row|matches!(&row.action,Some(RowAction::FileProvenance(FileAction::HistoryEntry(id))) if selected.is_some_and(|selection|&selection.commit==id))).or_else(||self.rows.iter().position(|row|matches!(&row.action,Some(RowAction::FileProvenance(FileAction::HistoryEntry(_)))))),
            ProvenanceView::Blame=>self.rows.iter().position(|row|matches!(&row.action,Some(RowAction::FileProvenance(FileAction::BlameLine(line))) if selected.and_then(|selection|selection.final_line)==Some(*line))).or_else(||self.rows.iter().position(|row|matches!(&row.action,Some(RowAction::FileProvenance(FileAction::BlameLine(_)))))),
            ProvenanceView::OriginalFile=>self.rows.iter().position(|row|matches!(&row.action,Some(RowAction::FileProvenance(FileAction::BlameAt(line))) if selected.and_then(|selection|selection.original_line)==Some(*line))),
            _=>None,
        };
        Some(row.unwrap_or(0))
    }
    /// The saved-tree file currently previewed, when Back can simply return to
    /// that same tree (expansion kept, file row selected).
    pub fn tree_file_return_v1(&self) -> Option<&RawPath> {
        let state = self.file_provenance.as_ref()?;
        let tree = self.tree.as_ref()?;
        (self.mode == RepositoryMode::Tree
            && state.view == ProvenanceView::File
            && self.preview.is_some()
            && tree.revision == self.revision
            && tree.focus.as_ref() == Some(&state.path))
        .then_some(&state.path)
    }
    /// Plain saved tree row of the file or directory last opened from it.
    pub fn tree_focus_row_v1(&self) -> Option<usize> {
        if self.mode != RepositoryMode::Tree
            || self.file_provenance.is_some()
            || self.preview.is_some()
        {
            return None;
        }
        let focus = self.tree.as_ref()?.focus.as_ref()?;
        self.display_rows_v1().iter().position(|row| {
            matches!(&row.action, Some(RowAction::File(path) | RowAction::Directory(path)) if path == focus)
        })
    }
    pub fn provenance_available(&self, action: &FileAction) -> Result<(), String> {
        let state = self.file_provenance.as_ref();
        if state.is_none()
            && self
                .file_preview_v1()
                .is_none_or(|preview| !crate::search::model::valid_sha(&preview.revision))
        {
            return Err("Choose a file at a saved version first; working-file lines are not immutable blame targets".into());
        }
        match action {
            FileAction::Back if state.is_none_or(|state|state.view==ProvenanceView::File)=>Err("Already at the original saved file; choose History or Blame to follow its changes".into()),
            FileAction::Diff if self.status.shallow && state.and_then(|state|state.selection.as_ref()).is_some_and(|selection|selection.boundary) && self.selected_commit.as_ref().is_some_and(|commit|!commit.parents.is_empty())=>Err("Shallow boundary: the responsible parent comparison is unavailable in loaded history; no fetch or invented empty-tree diff".into()),
            FileAction::Previous if state.is_none_or(|state|!state.can_previous())=>Err("First source page; no earlier page is available".into()),
            FileAction::Next if state.is_none_or(|state|!state.can_next())=>Err("No next page within this chosen source or safety budget; absence is not proof of complete ancestry".into()),
            FileAction::Graph|FileAction::Diff|FileAction::OriginalFile if state.and_then(|state|state.selection.as_ref()).is_none()=>Err("Select an exact history entry or attributed line before navigating its commit".into()),
            FileAction::OriginalFile if state.and_then(|state|state.selection.as_ref()).is_some_and(|selection|!selection.exists || selection.path.is_none())=>Err("This commit has no verified file target; open its graph/diff instead".into()),
            _=>Ok(()),
        }
    }
    pub fn provenance_action(
        &mut self,
        action: FileAction,
        cancellation: CancellationToken,
    ) -> Result<(), String> {
        self.open_with_cancellation(RowAction::FileProvenance(action), cancellation)
    }
    pub(crate) fn provenance_action_from_repository_v1(
        &mut self,
        repo: &GitRepository,
        action: FileAction,
        cancellation: CancellationToken,
    ) -> Result<(), String> {
        self.provenance_available(&action)?;
        let retained_side = self
            .original_diff_side_v1
            .as_ref()
            .filter(|side| side.owns(self))
            .cloned();
        let mut state = self
            .file_provenance
            .clone()
            .or_else(|| {
                self.file_preview_v1()
                    .filter(|preview| crate::search::model::valid_sha(&preview.revision))
                    .map(|preview| {
                        FileProvenance::new(preview.revision.clone(), preview.path.clone())
                    })
            })
            .ok_or("Choose a file at an immutable saved version before opening history/blame")?;
        if matches!(
            action,
            FileAction::History | FileAction::Blame | FileAction::BlameAt(_)
        ) && matches!(
            state.view,
            ProvenanceView::File | ProvenanceView::OriginalFile
        ) {
            if let Some(preview) = self.file_preview_v1() {
                if crate::search::model::valid_sha(&preview.revision)
                    && (preview.revision != state.revision || preview.path != state.path)
                {
                    state = FileProvenance::new(preview.revision.clone(), preview.path.clone());
                }
            }
        }
        match action {
            FileAction::History => {
                state.view = ProvenanceView::History;
                state.history = Some(
                    repo.file_history_page(&state.revision, &state.path, 0, HISTORY_PAGE)
                        .map_err(|error| error.to_string())?,
                );
                state.selection = None;
                self.render_history(repo, &state)?;
            }
            FileAction::Blame | FileAction::BlameAt(_) => {
                let line = if let FileAction::BlameAt(line) = action {
                    line as usize
                } else {
                    1
                };
                let start = (line.saturating_sub(1) / BLAME_PAGE) * BLAME_PAGE + 1;
                if !self.load_blame(repo, &mut state, start)? {
                    // Unsupported content has no attributed member, but its
                    // keyboard target actions still need clickable explanations.
                    for (label, target) in [
                        ("[G Responsible graph]", FileAction::Graph),
                        ("[D Introduced diff]", FileAction::Diff),
                        ("[F Original file]", FileAction::OriginalFile),
                    ] {
                        self.push(
                            label,
                            SemanticRole::Boundary,
                            Some(RowAction::FileProvenance(target)),
                        );
                    }
                    self.file_provenance = Some(state);
                    return Ok(());
                }
                if let FileAction::BlameAt(line) = action {
                    self.select_blame(repo, &mut state, line)?;
                } else {
                    self.render_blame(repo, &state)?;
                }
            }
            FileAction::Previous | FileAction::Next => {
                let next = action == FileAction::Next;
                if next && !state.can_next() || !next && !state.can_previous() {
                    return Err(
                        "No further page is available within this explicit immutable source budget"
                            .into(),
                    );
                }
                match state.view {
                    ProvenanceView::History => {
                        let offset = state.history.as_ref().ok_or("History page missing")?.offset;
                        let offset = if next {
                            offset + HISTORY_PAGE
                        } else {
                            offset.saturating_sub(HISTORY_PAGE)
                        };
                        state.history = Some(
                            repo.file_history_page(
                                &state.revision,
                                &state.path,
                                offset,
                                HISTORY_PAGE,
                            )
                            .map_err(|error| error.to_string())?,
                        );
                        self.render_history(repo, &state)?;
                    }
                    ProvenanceView::Blame => {
                        let start = if next {
                            state.blame_start + BLAME_PAGE
                        } else {
                            state.blame_start.saturating_sub(BLAME_PAGE).max(1)
                        };
                        if self.load_blame(repo, &mut state, start)? {
                            self.render_blame(repo, &state)?;
                        }
                    }
                    _ => return Err("Return to History or Blame before paging".into()),
                }
            }
            FileAction::HistoryEntry(id) => {
                let entry = state
                    .history
                    .as_ref()
                    .and_then(|page| page.entries.iter().find(|entry| entry.commit.id == id))
                    .ok_or("History entry is no longer in the displayed immutable page")?;
                state.selection = Some(FileSelection {
                    commit: entry.commit.id.clone(),
                    path: entry.path.clone(),
                    original_line: None,
                    final_line: None,
                    boundary: false,
                    exists: entry.exists,
                });
                state.return_view = ProvenanceView::History;
                state.view = ProvenanceView::Detail;
                self.render_selection(repo, &state)?;
            }
            FileAction::BlameLine(line) => self.select_blame(repo, &mut state, line)?,
            FileAction::Graph | FileAction::Diff | FileAction::OriginalFile => {
                let selection = state
                    .selection
                    .as_ref()
                    .ok_or("Select a current history entry or attributed line first")?;
                let mut next = if action == FileAction::OriginalFile {
                    if !selection.exists {
                        return Err("This history entry has no verified file at that saved version; open its graph or introduced diff".into());
                    }
                    Self::open_search_file_from_repository_v1(
                        repo,
                        selection
                            .path
                            .as_ref()
                            .ok_or("Original filename is unavailable")?,
                        Some(&selection.commit),
                        selection.original_line.unwrap_or(1) as usize,
                        cancellation.clone(),
                        self.actor_origin.clone(),
                    )?
                } else {
                    Self::load_from_repository_v1(
                        repo,
                        if action == FileAction::Graph {
                            RepositoryMode::Graph
                        } else {
                            RepositoryMode::CommitChanges
                        },
                        &selection.commit,
                        None,
                        self.actor_origin.clone(),
                    )?
                    .ok_or("Selected responsible repository is unavailable")?
                };
                state.view = match action {
                    FileAction::Graph => ProvenanceView::Graph,
                    FileAction::Diff => ProvenanceView::Diff,
                    _ => ProvenanceView::OriginalFile,
                };
                if action == FileAction::OriginalFile {
                    next.rows.insert(
                        0,
                        DisplayRow {
                            label: "Original immutable file — exact responsible commit/path/line"
                                .into(),
                            role: SemanticRole::Reference,
                            action: None,
                        },
                    );
                }
                next.file_provenance = Some(state);
                if let Some(side) = retained_side {
                    next.carry_original_diff_side_v1(side)?;
                }
                *self = next;
                return Ok(());
            }
            FileAction::Back => {
                if matches!(
                    state.view,
                    ProvenanceView::Detail
                        | ProvenanceView::Graph
                        | ProvenanceView::Diff
                        | ProvenanceView::OriginalFile
                ) {
                    state.view = state.return_view;
                    match state.view {
                        ProvenanceView::History => self.render_history(repo, &state)?,
                        ProvenanceView::Blame => self.render_blame(repo, &state)?,
                        _ => self.return_origin_file_from_repository_v1(
                            repo,
                            &state,
                            cancellation.clone(),
                        )?,
                    }
                } else {
                    self.return_origin_file_from_repository_v1(repo, &state, cancellation.clone())?;
                    state.view = ProvenanceView::File;
                }
            }
        }
        if cancellation.is_cancelled() {
            return Err("File provenance read cancelled before presentation".into());
        }
        self.file_provenance = Some(state);
        Ok(())
    }
    fn provenance_header(
        &mut self,
        repo: &GitRepository,
        state: &FileProvenance,
        title: &str,
    ) -> Result<(), String> {
        self.rows.clear();
        self.preview = None;
        self.comparison = None;
        self.mode = RepositoryMode::Tree;
        self.revision = state.revision.clone();
        self.selected_commit = repo
            .log_page(&state.revision, 0, 1)
            .map_err(|error| error.to_string())?
            .into_iter()
            .next();
        self.push(title, SemanticRole::Reference, None);
        self.push(
            &format!("Chosen path: {}", state.path.display()),
            SemanticRole::Reference,
            None,
        );
        if self.status.shallow {
            self.push(
                "SHALLOW: older ancestor history is unavailable; no fetch was performed",
                SemanticRole::Boundary,
                None,
            );
        }
        self.push(
            "Related PR: unavailable — no verified host/repository association",
            SemanticRole::Boundary,
            None,
        );
        Ok(())
    }
    fn render_history(
        &mut self,
        repo: &GitRepository,
        state: &FileProvenance,
    ) -> Result<(), String> {
        self.provenance_header(
            repo,
            state,
            "File history — chosen saved version; read-only",
        )?;
        let page = state.history.as_ref().ok_or("No bounded history page")?;
        self.push(
            &format!(
                "History entries {}–{} • page {}",
                page.offset + usize::from(!page.entries.is_empty()),
                page.offset + page.entries.len(),
                page.offset / HISTORY_PAGE + 1
            ),
            SemanticRole::Reference,
            None,
        );
        self.push(
            "Limits: single-file rename follow; merge/copy/move history may be incomplete",
            SemanticRole::Boundary,
            None,
        );
        for entry in &page.entries {
            let path = entry
                .path
                .as_ref()
                .map_or_else(|| "historical path unavailable".into(), RawPath::display);
            let label = format!(
                "{} {} • {} • {} • {}",
                &entry.commit.id[..8],
                short(&entry.commit.subject, 100),
                crate::git_actor_labels_v1::commit_summary(&entry.commit),
                change_word_v1(&entry.change),
                path
            );
            self.push(
                &label,
                change_role(entry),
                Some(RowAction::FileProvenance(FileAction::HistoryEntry(
                    entry.commit.id.clone(),
                ))),
            );
            if let Some(previous) = &entry.previous_path {
                self.push(
                    &format!("  Rename: {} → {path}", previous.display()),
                    SemanticRole::Reference,
                    None,
                );
            }
        }
        if page.entries.is_empty() {
            self.push("No file changes found in this loaded history; this is not proof of complete ancestry",SemanticRole::Boundary,None);
        }
        if page.walk_limited {
            self.push(
                "History walk cap reached: 10,000 changes; older entries remain unavailable",
                SemanticRole::Boundary,
                None,
            );
        }
        Ok(())
    }
    fn load_blame(
        &mut self,
        repo: &GitRepository,
        state: &mut FileProvenance,
        start: usize,
    ) -> Result<bool, String> {
        // A rejected/unsupported page must not retain a previous line attribution.
        state.blame.clear();
        state.blame_start = start;
        state.line_count = 0;
        state.selection = None;
        let entry = repo
            .tree_entry(&state.revision, &state.path)
            .map_err(|error| error.to_string())?;
        if entry.kind != "blob" || !matches!(entry.mode.as_str(), "100644" | "100755") {
            self.provenance_header(
                repo,
                state,
                "Line provenance unavailable: symlink/submodule is not a regular text file",
            )?;
            state.view = ProvenanceView::Blame;
            return Ok(false);
        }
        let size = repo
            .object_sizes(std::slice::from_ref(&entry.id))
            .map_err(|error| error.to_string())?
            .into_iter()
            .next()
            .ok_or("Chosen immutable blob size is unavailable")?;
        if size > 2 * 1024 * 1024 {
            self.provenance_header(
                repo,
                state,
                "Line provenance unavailable: chosen blob exceeds the2 MiB safety budget",
            )?;
            state.view = ProvenanceView::Blame;
            return Ok(false);
        }
        let bytes = repo
            .blob(&state.revision, &state.path)
            .map_err(|error| error.to_string())?;
        let preview = file_preview(state.revision.clone(), &entry, bytes, 10_000);
        if preview.kind != BlobPreviewKind::Text {
            self.provenance_header(
                repo,
                state,
                "Line provenance unavailable: binary/LFS content has no verified source lines",
            )?;
            state.view = ProvenanceView::Blame;
            return Ok(false);
        }
        state.line_count = preview
            .content
            .iter()
            .filter(|byte| **byte == b'\n')
            .count()
            + usize::from(!preview.content.is_empty() && !preview.content.ends_with(b"\n"));
        if state.line_count == 0 {
            self.provenance_header(repo, state, "Empty saved text file — no lines to attribute")?;
            state.blame.clear();
            state.view = ProvenanceView::Blame;
            return Ok(false);
        }
        if start > state.line_count {
            return Err("Selected line is outside the chosen immutable file".into());
        }
        state.blame = repo
            .blame_lines(
                &state.revision,
                &state.path,
                start,
                BLAME_PAGE.min(state.line_count - start + 1),
            )
            .map_err(|error| error.to_string())?;
        state.blame_start = start;
        state.view = ProvenanceView::Blame;
        state.selection = None;
        Ok(true)
    }
    fn render_blame(&mut self, repo: &GitRepository, state: &FileProvenance) -> Result<(), String> {
        self.provenance_header(repo, state, "Line blame — chosen saved text; read-only")?;
        self.push(
            &format!(
                "Blame lines {}–{} of {} • page {}",
                state.blame_start,
                state.blame_start + state.blame.len().saturating_sub(1),
                state.line_count,
                (state.blame_start - 1) / BLAME_PAGE + 1
            ),
            SemanticRole::Reference,
            None,
        );
        self.push(
            "Limits: rename-follow yes; moved/copied line detection (-M/-C) is off",
            SemanticRole::Boundary,
            None,
        );
        self.push(
            "Boundary marks a root/incomplete ancestor, not invented earlier provenance",
            SemanticRole::Boundary,
            None,
        );
        for record in &state.blame {
            let line = &record.line;
            self.push(
                &format!(
                    "{:>5} │ {} {} │ {}{}",
                    line.final_line,
                    &line.commit[..8],
                    short(&line.author, 18),
                    short(&crate::git::display_bytes(&line.text), 120),
                    if record.boundary { " • boundary" } else { "" }
                ),
                if record.boundary {
                    SemanticRole::Boundary
                } else {
                    SemanticRole::Normal
                },
                Some(RowAction::FileProvenance(FileAction::BlameLine(
                    line.final_line,
                ))),
            );
        }
        Ok(())
    }
    fn select_blame(
        &mut self,
        repo: &GitRepository,
        state: &mut FileProvenance,
        line: u32,
    ) -> Result<(), String> {
        let record = state
            .blame
            .iter()
            .find(|record| record.line.final_line == line)
            .ok_or("Line is no longer in the current immutable blame page")?;
        if record.ignored || record.unblamable {
            return Err(
                "Git marks this line ignored/unblamable; no responsible commit is invented".into(),
            );
        }
        state.selection = Some(FileSelection {
            commit: record.line.commit.clone(),
            path: Some(record.line.path.clone()),
            original_line: Some(record.line.original_line),
            final_line: Some(record.line.final_line),
            boundary: record.boundary,
            exists: true,
        });
        state.return_view = ProvenanceView::Blame;
        state.view = ProvenanceView::Detail;
        self.render_selection(repo, state)
    }
    fn render_selection(
        &mut self,
        repo: &GitRepository,
        state: &FileProvenance,
    ) -> Result<(), String> {
        self.provenance_header(
            repo,
            state,
            "File/line provenance — exact responsible saved version",
        )?;
        let selection = state
            .selection
            .as_ref()
            .ok_or("No selected canonical provenance")?;
        self.selected_commit = repo
            .log_page(&selection.commit, 0, 1)
            .map_err(|error| error.to_string())?
            .into_iter()
            .next();
        self.revision = selection.commit.clone();
        self.push(
            &format!("Origin saved file: {}", state.revision),
            SemanticRole::Reference,
            None,
        );
        self.push(
            &format!(
                "Original path: {}",
                selection.path.as_ref().map_or_else(
                    || "unavailable — merge/ambiguous path diff".into(),
                    RawPath::display
                )
            ),
            SemanticRole::Reference,
            None,
        );
        if let Some(line) = selection.original_line {
            self.push(
                &format!(
                    "Original line: {line} • chosen file line: {}",
                    selection.final_line.unwrap_or(line)
                ),
                SemanticRole::Reference,
                None,
            );
        }
        if selection.boundary {
            self.push(
                "BOUNDARY: root or missing older ancestry; attribution is limited",
                SemanticRole::Boundary,
                None,
            );
        }
        if let Some(commit) = &self.selected_commit {
            self.push(
                &format!("Change: {}", short(&commit.subject, 200)),
                SemanticRole::Normal,
                None,
            );
        }
        self.push(
            "[G Responsible graph]",
            SemanticRole::Reference,
            Some(RowAction::FileProvenance(FileAction::Graph)),
        );
        let unavailable_parent = self.status.shallow
            && selection.boundary
            && self
                .selected_commit
                .as_ref()
                .is_some_and(|commit| !commit.parents.is_empty());
        self.push(
            if unavailable_parent {
                "[D Introduced diff] — unavailable: shallow parent"
            } else {
                "[D Introduced diff]"
            },
            if unavailable_parent {
                SemanticRole::Boundary
            } else {
                SemanticRole::Reference
            },
            Some(RowAction::FileProvenance(FileAction::Diff)),
        );
        if selection.exists && selection.path.is_some() {
            self.push(
                "[F Original file]",
                SemanticRole::Reference,
                Some(RowAction::FileProvenance(FileAction::OriginalFile)),
            );
        } else {
            self.push(
                "Original file unavailable at this commit; no filename is guessed",
                SemanticRole::Boundary,
                None,
            );
        }
        Ok(())
    }
    fn return_origin_file_from_repository_v1(
        &mut self,
        repo: &GitRepository,
        state: &FileProvenance,
        cancel: CancellationToken,
    ) -> Result<(), String> {
        let retained_tree = self.tree_navigation_v1.clone();
        let retained_strip = self.tree_breadcrumbs_v2.clone();
        let retained_side = self
            .original_diff_side_v1
            .as_ref()
            .filter(|side| side.owns(self))
            .cloned();
        let mut fresh = Self::open_search_file_from_repository_v1(
            repo,
            &state.path,
            Some(&state.revision),
            1,
            cancel,
            self.actor_origin.clone(),
        )?;
        if let Some(tree) = retained_tree.filter(|tree| tree.matches_fresh_file_v1(&fresh)) {
            fresh.tree_breadcrumbs_v2 =
                retained_strip.filter(|strip| Arc::ptr_eq(strip.source(), &tree));
            fresh.tree_navigation_v1 = Some(tree);
        }
        if let Some(side) = retained_side {
            fresh.carry_original_diff_side_v1(side)?;
        }
        // Keep the plain saved tree this file was opened from, so Back to tree
        // still works after History/Blame.
        if let Some(tree) = self.tree.take().filter(|tree| {
            tree.revision == state.revision && tree.focus.as_ref() == Some(&state.path)
        }) {
            fresh.tree = Some(tree);
        }
        *self = fresh;
        Ok(())
    }
}
fn short(value: &str, characters: usize) -> String {
    value.chars().take(characters).collect()
}
/// Plain word for a Git name-status letter (`M` → `modified`, `R100` → `renamed`).
pub fn change_word_v1(change: &str) -> String {
    match change.as_bytes().first() {
        Some(b'A') => "added".into(),
        Some(b'M') => "modified".into(),
        Some(b'D') => "deleted".into(),
        Some(b'R') => "renamed".into(),
        Some(b'C') => "copied".into(),
        Some(b'T') => "type changed".into(),
        _ => change.into(),
    }
}
fn change_role(entry: &FileHistoryEntry) -> SemanticRole {
    match entry.change.as_bytes().first() {
        Some(b'A') => SemanticRole::Added,
        Some(b'D') => SemanticRole::Removed,
        Some(b'R') => SemanticRole::Reference,
        _ => SemanticRole::Changed,
    }
}
