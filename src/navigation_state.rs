//! Native navigation retains bounded identities, never a Workspace, editor,
//! result body or permission. Missing sources are exact reads or explanations.
//! Private-page restore adapters are deliberately not enabled by local history.
use crate::{
    app::{Action, App, View},
    navigation_trail::{
        NavigationAction, NavigationBookmark, NavigationPosition, NavigationScope, NavigationToken,
    },
    providers::runner::CancellationToken,
    workspace::{RepositoryMode, RowAction, Workspace},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{self, Write},
    os::unix::ffi::OsStrExt,
};

#[path = "navigation_graph_admission_v2.rs"]
mod graph_admission_v2;
pub(crate) use graph_admission_v2::handle_retention_action_v2;

pub struct NavigationRestore {
    pub(crate) graph_position_v2:
        Option<std::sync::Arc<crate::repository_graph_v1::restore_v2::GraphPositionLeaseV2>>,
    pub token: NavigationToken,
    pub bookmark: NavigationBookmark,
    pub original_local:
        Option<std::sync::Arc<crate::navigation_local_source_v1::OriginalSavedVisitV1>>,
}
impl std::fmt::Debug for NavigationRestore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NavigationRestore")
            .field("bookmark", &self.bookmark)
            .finish_non_exhaustive()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RepositoryRecipe {
    version: u8,
    mode: u8,
    revision: String,
    parent: Option<usize>,
    mutable_source: Option<[u8; 8]>,
    /// A nested source is not silently reduced to its top-level mode.
    nested: Option<Vec<u8>>,
}
fn mode_number(mode: RepositoryMode) -> u8 {
    match mode {
        RepositoryMode::Overview => 0,
        RepositoryMode::Graph => 1,
        RepositoryMode::Tree => 2,
        RepositoryMode::Working => 3,
        RepositoryMode::Staged => 4,
        RepositoryMode::Branch => 5,
        RepositoryMode::CommitChanges => 6,
        RepositoryMode::Uncommitted => 7,
    }
}
fn full_revision(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
struct BoundedHash {
    hash: Sha256,
    left: usize,
}
impl BoundedHash {
    fn new() -> Self {
        Self {
            hash: Sha256::new(),
            left: 64 * 1024,
        }
    }
    fn field(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.write_all(&(bytes.len() as u64).to_le_bytes())
            .map_err(|_| "Navigation source exceeds its metadata budget")?;
        self.write_all(bytes)
            .map_err(|_| "Navigation source exceeds its metadata budget".to_string())
    }
    fn json(&mut self, value: &impl Serialize) -> Result<(), String> {
        serde_json::to_writer(self, value)
            .map_err(|_| "Navigation source exceeds its metadata budget".into())
    }
    fn finish(self) -> Vec<u8> {
        self.hash.finalize().to_vec()
    }
}
impl Write for BoundedHash {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.left {
            return Err(io::Error::other("bounded navigation metadata"));
        }
        self.left -= bytes.len();
        self.hash.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(crate) fn row_key(action: &RowAction) -> Result<Vec<u8>, String> {
    let mut hash = BoundedHash::new();
    match action {
        RowAction::OriginalDiffReturn(ticket) => {
            let side = ticket.side();
            hash.field(b"original-comparison-return")?;
            hash.field(side.source().intent().head().as_bytes())?;
            hash.field(side.source().intent().content_base().as_bytes())?;
            hash.json(&side.index())?;
            hash.field(side.side().label().as_bytes())?;
            hash.field(&side.entry().path.0)?;
            hash.field(side.entry().id.as_bytes())?;
        }
        RowAction::Commit(id) => {
            hash.field(b"commit")?;
            hash.field(id.as_bytes())?;
        }
        RowAction::Directory(path)
        | RowAction::File(path)
        | RowAction::Conflict(path)
        | RowAction::WorkingFile(path) => {
            hash.field(match action {
                RowAction::Directory(_) => b"directory",
                RowAction::File(_) => b"file",
                RowAction::Conflict(_) => b"conflict",
                _ => b"working-file",
            })?;
            hash.field(&path.0)?;
        }
        RowAction::ComparisonParent { commit, parent } => {
            hash.field(b"parent")?;
            hash.field(commit.as_bytes())?;
            hash.json(parent)?;
        }
        RowAction::DiffLine(anchor) => {
            hash.field(b"canonical-diff")?;
            hash.json(&(
                &anchor.base,
                &anchor.head,
                &anchor.old_path,
                &anchor.new_path,
                anchor.old_line,
                anchor.new_line,
            ))?;
            hash.field(match anchor.side {
                crate::repository_view::DiffSide::Old => b"old",
                crate::repository_view::DiffSide::New => b"new",
                crate::repository_view::DiffSide::Both => b"both",
            })?;
            hash.json(&(anchor.file_index, anchor.hunk_index, anchor.line_index))?;
        }
        RowAction::FileProvenance(action) => {
            hash.field(b"file-provenance")?;
            use crate::workspace::FileAction;
            match action {
                FileAction::BlameLine(line) | FileAction::BlameAt(line) => {
                    hash.field(if matches!(action, FileAction::BlameLine(_)) {
                        b"blame-line"
                    } else {
                        b"blame-at"
                    })?;
                    hash.json(line)?;
                }
                _ => hash.field(match action {
                    FileAction::PathHistory => b"path-history",
                    FileAction::Blame => b"blame",
                    FileAction::Previous => b"previous",
                    FileAction::Next => b"next",
                    FileAction::Graph => b"graph",
                    FileAction::Diff => b"diff",
                    FileAction::OriginalFile => b"original-file",
                    FileAction::Back => b"back",
                    _ => unreachable!(),
                })?,
            }
        }
        RowAction::Graph(action) => {
            hash.field(b"original-graph")?;
            hash.json(action)?;
        }
        RowAction::Tree(action) => {
            use crate::workspace::OriginalTreeNavigationActionV1;
            let ticket = match action {
                OriginalTreeNavigationActionV1::Toggle(ticket) => {
                    hash.field(b"original-tree-toggle")?;
                    ticket
                }
                OriginalTreeNavigationActionV1::Directory(ticket) => {
                    hash.field(b"original-tree-directory")?;
                    ticket
                }
            };
            hash.field(ticket.saved_sha().as_bytes())?;
            if let Some(member) = ticket.member() {
                hash.field(&member.path().0)?;
                hash.field(member.mode().as_bytes())?;
                hash.field(member.object_id().as_bytes())?;
            } else {
                hash.field(b"root")?;
            }
        }
        RowAction::Legend => hash.field(b"legend")?,
        RowAction::Local(command) => {
            hash.field(b"local-command")?;
            hash.field(command.key().as_bytes())?;
        }
        RowAction::Actor(role) => hash.field(match role {
            crate::identity::GitRole::Author => b"author",
            crate::identity::GitRole::Committer => b"committer",
        })?,
    }
    Ok(hash.finish())
}
fn focused_id(action: &Action) -> Option<String> {
    Some(match action {
        Action::View(view) => format!("view-{}", view.index()),
        Action::Navigation(NavigationAction::Back) => "navigation-back".into(),
        Action::RepositoryMode(mode) => format!("repository-mode-{}", mode_number(*mode)),
        Action::Help => "help".into(),
        Action::Palette => "palette".into(),
        Action::Refresh => "refresh".into(),
        Action::LocalMenu => "local-menu".into(),
        _ => return None,
    })
}
fn repository_bookmark(app: &App, workspace: &Workspace) -> Result<NavigationBookmark, String> {
    if workspace.revision.len() > 64
        || (!full_revision(&workspace.revision) && workspace.status.head.is_some())
    {
        return Err("Original repository saved version is unavailable".into());
    }
    if workspace.graph_retention_visible_v2() {
        let visit = app
            .original_saved_visits
            .graph_visit_v2(workspace)
            .ok_or("Original Graph snapshot unavailable • no body/source/HEAD fallback")?;
        let selected_key = workspace
            .display_rows_v1()
            .get(app.repository_list.selected)
            .and_then(|row| row.action.as_ref())
            .map(row_key)
            .transpose()?;
        return NavigationBookmark::new(
            View::Repository,
            NavigationScope::local(workspace.directory.clone())?,
            visit.key().to_vec(),
            "local-saved-source-v1".into(),
            vec![],
            visit.label().into(),
            NavigationPosition {
                selected_key,
                focused_control: app
                    .hits
                    .get(app.focus)
                    .and_then(|hit| focused_id(&hit.action)),
                content_focused: app.repository_content_focused,
                scroll: app.repository_list.top.min(1_000_000),
                graph_v2: Some(workspace.graph_position_v2()?),
            },
        );
    }
    let mut detour = BoundedHash::new();
    let mut nested = false;
    if let Some(preview) = workspace.file_preview_v1() {
        nested = true;
        detour.field(b"file-preview")?;
        detour.field(preview.revision.as_bytes())?;
        detour.field(preview.object_id.as_bytes())?;
        detour.field(&preview.path.0)?;
    }
    if let Some(state) = &workspace.file_provenance {
        nested = true;
        detour.field(b"file-provenance")?;
        detour.field(state.revision.as_bytes())?;
        detour.field(&state.path.0)?;
        detour.field(match state.view {
            crate::workspace::ProvenanceView::File => b"file",
            crate::workspace::ProvenanceView::Blame => b"blame",
            crate::workspace::ProvenanceView::Detail => b"detail",
            crate::workspace::ProvenanceView::Diff => b"diff",
            crate::workspace::ProvenanceView::OriginalFile => b"original-file",
        })?;
        detour.json(&state.blame_start)?;
        if let Some(selection) = &state.selection {
            detour.field(selection.commit.as_bytes())?;
            detour.json(&(
                &selection.path,
                selection.original_line,
                selection.final_line,
                selection.boundary,
                selection.exists,
            ))?;
        }
    }
    let mut parent = None;
    if let Some(comparison) = workspace.comparison_v1() {
        use crate::git::Comparison;
        detour.field(match &comparison.comparison {
            Comparison::Working => b"working",
            Comparison::Staged => b"staged",
            Comparison::Uncommitted => b"uncommitted",
            Comparison::Snapshots { .. } => b"snapshots",
            Comparison::BranchChanges { .. } => b"branch-changes",
            Comparison::CommitChanges { .. } => b"commit-changes",
        })?;
        match &comparison.comparison {
            Comparison::Working if workspace.mode == RepositoryMode::Working => {}
            Comparison::Staged if workspace.mode == RepositoryMode::Staged => {}
            Comparison::Uncommitted if workspace.mode == RepositoryMode::Uncommitted => {}
            Comparison::BranchChanges { .. } if workspace.mode == RepositoryMode::Branch => {
                nested = true;
            }
            Comparison::CommitChanges {
                commit,
                parent: index,
            } if workspace.mode == RepositoryMode::CommitChanges
                && commit == &workspace.revision =>
            {
                parent = Some(*index);
            }
            _ => nested = true,
        }
        // Exact resolved endpoints distinguish saved comparisons and detours.
        detour.json(&(
            &comparison.base,
            &comparison.head,
            &comparison.common_ancestor,
        ))?;
    }
    let recipe = RepositoryRecipe {
        version: 1,
        mode: mode_number(workspace.mode),
        revision: workspace.revision.clone(),
        parent,
        mutable_source: workspace.comparison_v1().and_then(|comparison| {
            comparison
                .observed
                .map(|observed| observed.navigation_identity())
        }),
        nested: nested.then(|| detour.finish()),
    };
    let source =
        serde_json::to_vec(&recipe).map_err(|_| "Original repository source is unavailable")?;
    let original = if workspace.graph_retention_visible_v2() {
        Some(
            app.original_saved_visits
                .graph_visit_v2(workspace)
                .ok_or("Original Graph snapshot unavailable • no body/source/HEAD fallback")?,
        )
    } else {
        crate::navigation_local_source_v1::OriginalSavedVisitV1::capture(workspace).ok()
    };
    let source = original
        .as_ref()
        .map_or(source, |visit| visit.key().to_vec());
    let route = if original.is_some() {
        "local-saved-source-v1"
    } else {
        "local-repository-v1"
    };
    let selected_key = workspace
        .display_rows_v1()
        .get(app.repository_list.selected)
        .and_then(|row| row.action.as_ref())
        .map(row_key)
        .transpose()?;
    let label = RepositoryMode::ALL
        .iter()
        .find(|(mode, _, _)| mode == &workspace.mode)
        .map_or("Unavailable", |(_, label, _)| *label);
    NavigationBookmark::new(
        View::Repository,
        NavigationScope::local(workspace.directory.clone())?,
        source,
        route.into(),
        vec![],
        original.as_ref().map_or_else(
            || format!("Repository • {label}"),
            |visit| visit.label().into(),
        ),
        NavigationPosition {
            selected_key,
            focused_control: app
                .hits
                .get(app.focus)
                .and_then(|hit| focused_id(&hit.action)),
            content_focused: app.repository_content_focused,
            scroll: app.repository_list.top.min(1_000_000),
            graph_v2: workspace
                .graph_retention_visible_v2()
                .then(|| workspace.graph_position_v2())
                .transpose()?,
        },
    )
}
pub fn capture(app: &App) -> Result<Option<NavigationBookmark>, String> {
    if app.loading[app.view.index()] || app.overlay.is_some() {
        return Ok(None);
    }
    match app.view {
        View::Repository => {
            if app.repository_load_requested || app.repository_action.is_some() { return Ok(None); }
            app.workspace.as_ref().map(|workspace| repository_bookmark(app, workspace)).transpose()
        }
        View::Search => {
            if app.search_load_requested || app.search_action.is_some() { return Ok(None); }
            let Some(search) = &app.search else { return Ok(None); };
            if search.scope.host != "local" || search.remote_identity.is_some() || search.mode == crate::search::SearchMode::GitHub {
                return Err("Private Search history needs its exact current account/source restore adapter; no default scope will be opened".into());
            }
            if search.selections.len() > 128 { return Err("Search navigation scope exceeds 128 repositories".into()); }
            let first = search.selections.first().ok_or("Original local Search scope is unavailable")?;
            let mut hash = BoundedHash::new(); hash.field(b"local-search-v1")?;
            hash.json(&search.mode)?; hash.field(search.input.as_bytes())?;
            for selection in &search.selections {
                hash.field(selection.repository.as_bytes())?; hash.field(selection.directory.as_os_str().as_bytes())?; hash.json(&selection.source)?;
            }
            if let Some(page) = &search.page { hash.json(&page.query)?; }
            NavigationBookmark::new(View::Search, NavigationScope::local(first.directory.clone())?, hash.finish(), "local-search-v1".into(), vec![], "Search • local source".into(), NavigationPosition { scroll: app.content_list.top.min(1_000_000), ..Default::default() }).map(Some)
        }
        _ => Err("This page's exact source restoration is not yet available; no broader repository or private page will be opened".into()),
    }
}

impl App {
    /// Runs outside draw, after actual source publication and before intent resets.
    pub fn observe_navigation(&mut self) {
        if self.navigation_trail.is_pending() || self.navigation_restore_inflight {
            return;
        }
        if self.view == View::Repository {
            if let Some(workspace) = self
                .workspace
                .as_ref()
                .filter(|workspace| !workspace.graph_retention_visible_v2())
            {
                if let Ok(visit) =
                    crate::navigation_local_source_v1::OriginalSavedVisitV1::capture(workspace)
                {
                    let _ = self.original_saved_visits.remember_checked_v2(visit);
                }
            }
        }
        if graph_admission_v2::retain_graph_before_leave_v2(self).is_err() {
            return;
        }
        let observed = match capture(self) {
            Ok(Some(bookmark)) => bookmark,
            _ => return,
        };
        if self
            .navigation_trail
            .current()
            .is_some_and(|current| current.same_location(&observed))
        {
            let _ = self.navigation_trail.update_position(&observed);
        } else if observed.view() == View::Search
            && self
                .navigation_trail
                .current()
                .is_some_and(|current| current.view() == View::Search)
        {
            // Editing/executing a query replaces the prepared Search source;
            // it is not another page visit. Back still returns to its origin.
            let _ = self.navigation_trail.observe_current(observed);
            self.hits.clear();
        } else if let Ok(token) = self.navigation_trail.begin(observed.clone()) {
            let _ = self.navigation_trail.complete(token, observed);
            self.hits.clear();
        }
    }
    pub fn abandon_navigation_restore(&mut self) {
        if self.navigation_trail.is_pending() {
            let _ = self.navigation_trail.abandon();
        }
        self.navigation_restore = None;
        self.navigation_restore_inflight = false;
        self.hits.clear();
    }
    pub fn request_navigation(&mut self, action: NavigationAction) {
        if self.loading.iter().any(|loading| *loading) || self.navigation_restore_inflight {
            self.dispatch(Action::Explain(
                "Wait for the current read, or cancel it before navigating Back".into(),
            ));
            return;
        }
        // History's diff and drill-down are views inside one History location: Back returns
        // to the table with the bar on the same commit, as `u` does (TASK-99).
        if action == NavigationAction::Back
            && self.view == View::Repository
            && self.overlay.is_none()
            && self.workspace.as_ref().is_some_and(|workspace| {
                workspace.graph_view_v1 != crate::repository_graph_v1::GraphViewV1::Timeline
                    && crate::repository_graph_ui_v1::key(
                        workspace,
                        crossterm::event::KeyCode::Char('u'),
                    ) == Some(crate::repository_graph_v1::GraphActionV1::Back)
            })
        {
            self.dispatch(Action::RepositoryRow(crate::workspace::RowAction::Graph(
                crate::repository_graph_v1::GraphActionV1::Back,
            )));
            return;
        }
        if self.navigation_trail.current().is_some() {
            let current_matches = capture(self).is_ok_and(|observed| {
                observed.is_some_and(|observed| {
                    self.navigation_trail
                        .current()
                        .is_some_and(|current| current.same_location(&observed))
                })
            });
            if !current_matches {
                self.dispatch(Action::Explain("The current page's original source is unavailable for navigation. No earlier page was retargeted or broader scope opened".into()));
                return;
            }
        }
        let token = match self.navigation_trail.action(action) {
            Ok(token) => token,
            Err(reason) => {
                self.dispatch(Action::Explain(reason));
                return;
            }
        };
        let bookmark = match self.navigation_trail.request(&token) {
            Ok(bookmark) => bookmark.clone(),
            Err(reason) => {
                self.dispatch(Action::Explain(reason));
                return;
            }
        };
        let current = capture_for_destination(self, &bookmark);
        if current
            .as_ref()
            .is_ok_and(|current| current.same_location(&bookmark))
        {
            self.finish_navigation(NavigationRestore {
                token,
                bookmark,
                original_local: None,
                graph_position_v2: None,
            });
        } else if bookmark.view() == View::Repository
            && matches!(bookmark.scope(), NavigationScope::Local(_))
        {
            let original = match bookmark.scope() {
                NavigationScope::Local(directory)
                    if bookmark.route_key() == "local-saved-source-v1" =>
                {
                    self.original_saved_visits
                        .find(bookmark.source_key(), directory)
                }
                _ => None,
            };
            if let Some(original_local) = original {
                let graph_position_v2 = match graph_admission_v2::graph_requested_position_v2(
                    &original_local,
                    &bookmark,
                ) {
                    Ok(position) => position,
                    Err(reason) => {
                        let _ = self.navigation_trail.cancel(token);
                        self.dispatch(Action::Explain(reason));
                        return;
                    }
                };
                self.navigation_restore = Some(NavigationRestore {
                    token,
                    bookmark,
                    original_local: Some(original_local),
                    graph_position_v2,
                });
                self.status = "Restoring original repository source • read-only".into();
                self.hits.clear();
            } else {
                let _ = self.navigation_trail.cancel(token);
                self.dispatch(Action::Explain(if bookmark.position().graph_v2.is_some(){
                    "Original Graph snapshot unavailable • discarded/retired bookmark, no legacy/body/HEAD fallback".into()
                }else{"Original opaque local source is unavailable; no decoded directory, HEAD fallback or broader read opened".into()}));
            }
        } else {
            let _ = self.navigation_trail.cancel(token);
            self.dispatch(Action::Explain("Original source is no longer prepared. An exact source restore is unavailable; no broader/default read was sent".into()));
        }
    }
    pub fn finish_navigation(&mut self, restore: NavigationRestore) {
        let observed = capture_for_destination(self, &restore.bookmark);
        let Ok(observed) = observed else {
            let _ = self.navigation_trail.cancel(restore.token);
            self.navigation_restore_inflight = false;
            self.dispatch(Action::Explain(
                "Original navigation source changed; history preserved without retargeting".into(),
            ));
            return;
        };
        let position = restore.bookmark.position().clone();
        let destination = restore.bookmark.view();
        if let Err(reason) = self.navigation_trail.complete(restore.token, observed) {
            self.navigation_restore_inflight = false;
            self.dispatch(Action::Explain(reason));
            return;
        }
        self.navigation_restore_inflight = false;
        self.view = destination;
        self.user_navigated = true;
        self.overlay = None;
        self.repository_selection = None;
        self.search_controls.editing = None;
        self.repository_content_focused =
            destination == View::Repository && position.content_focused;
        self.github_content_focused = false;
        self.actions_content_focused = false;
        self.metrics_content_focused = false;
        self.search_content_focused = false;
        let selected = if destination == View::Repository {
            self.workspace
                .as_ref()
                .and_then(|workspace| {
                    position.selected_key.as_ref().and_then(|key| {
                        if workspace.graph_visible_v1() {
                            workspace.graph_navigation_row_v1(key)
                        } else {
                            workspace.display_rows_v1().iter().position(|row| {
                                row.action
                                    .as_ref()
                                    .and_then(|action| row_key(action).ok())
                                    .as_ref()
                                    == Some(key)
                            })
                        }
                    })
                })
                .unwrap_or(0)
        } else {
            0
        };
        self.reset_lists_v92();
        *self.list_for_mut(destination) = crate::ui_table::ListState {
            selected,
            top: position.scroll,
        };
        if let Some(workspace) = &self.workspace {
            if destination == View::Repository {
                self.repository_mode = workspace.mode;
                self.context = format!(
                    "Local {} / {} / {}",
                    workspace.directory.display(),
                    workspace
                        .status
                        .branch
                        .as_deref()
                        .unwrap_or("detached HEAD"),
                    workspace.revision
                );
            }
        }
        self.navigation_restore_focus = position.focused_control;
        self.retire_command_context();
        self.hits.clear();
        self.status = "Original navigation source restored • read-only".into();
    }
}
fn capture_for_destination(
    app: &App,
    destination: &NavigationBookmark,
) -> Result<NavigationBookmark, String> {
    match destination.view() {
        View::Repository => repository_bookmark(
            app,
            app.workspace
                .as_ref()
                .ok_or("Original repository is unavailable")?,
        ),
        View::Search if app.view == View::Search => {
            capture(app)?.ok_or_else(|| "Original Search source is unavailable".into())
        }
        _ => Err("Original source restoration is unavailable".into()),
    }
}
/// Worker-only exact read. Never checkout, discovery, gh or default scope.
pub fn read_repository(
    restore: &NavigationRestore,
    cancellation: CancellationToken,
) -> Result<Workspace, String> {
    let visit = restore
        .original_local
        .as_ref()
        .ok_or("Original opaque local source is unavailable")?;
    let operation =
        crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1::begin(
            visit.origin(),
            cancellation,
        )?;
    let result = read_repository_with_operation_v1(restore, &operation);
    let sealed = operation.seal();
    match (result, sealed) {
        (Ok(workspace), Ok(_)) => Ok(workspace),
        (Err(reason), _) => Err(reason),
        (Ok(_), Err(reason)) => Err(reason),
    }
}
/// Background supplies its actual pre-queue token/clock and original runner.
pub fn read_repository_with_operation_v1(
    restore: &NavigationRestore,
    operation: &crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
) -> Result<Workspace, String> {
    let NavigationScope::Local(directory) = restore.bookmark.scope() else {
        return Err("Original local directory is unavailable".into());
    };
    let visit = restore
        .original_local
        .as_ref()
        .ok_or("Original opaque local source is unavailable; no decoded recipe opened")?;
    if restore.bookmark.route_key() != "local-saved-source-v1"
        || visit.key().as_slice() != restore.bookmark.source_key()
        || visit.origin().source().root() != directory
    {
        return Err("Original local route/source membership changed; no retarget".into());
    }
    match (&restore.graph_position_v2, visit.graph_lease_v2()) {
        (Some(position), Some(_)) => visit.restore_graph_position_v2(position, operation),
        (None, None) => visit.restore(operation),
        _ => Err("Original Graph snapshot unavailable • no legacy rebuild or HEAD fallback".into()),
    }
}
pub(crate) fn restore_focus(app: &mut App) {
    if let Some(id) = app.navigation_restore_focus.take() {
        if let Some(index) = app
            .hits
            .iter()
            .position(|hit| focused_id(&hit.action).as_deref() == Some(&id))
        {
            app.focus = index;
        }
    }
}
pub(crate) fn validate_repository_completion(
    app: &App,
    restore: &NavigationRestore,
    workspace: Option<&Workspace>,
) -> bool {
    app.navigation_restore_inflight
        && app
            .navigation_trail
            .request(&restore.token)
            .is_ok_and(|request| request.same_location(&restore.bookmark))
        && workspace.is_some_and(|workspace| {
            repository_bookmark(app, workspace)
                .is_ok_and(|observed| observed.same_location(&restore.bookmark))
        })
}

/// Cells the header keeps for Back at least: its compact `Back` label.
pub(crate) const NAVIGATION_MIN_WIDTH: u16 = 4;
/// A crumb cut to fit keeps at least this many cells, ellipsis included.
const CRUMB_MIN_WIDTH: u16 = 8;

/// The header's Back and breadcrumb tokens, in the footer key-bar style: `Alt-← Back` (the
/// configured key in the accent hue, then the label) and the two most recent crumbs as
/// labels, two cells apart. Nothing wraps: Back drops its key before it is left out, the
/// older crumb leaves first, and the most recent one ends with `…` while at least
/// [`CRUMB_MIN_WIDTH`] cells remain. Every painted token is a click target; Tab focus
/// underlines it.
pub(crate) fn draw_navigation(
    frame: &mut ratatui::Frame,
    app: &mut App,
    area: ratatui::layout::Rect,
) {
    use crate::theme::Role;
    use ratatui::style::{Modifier, Style};
    use ratatui::{layout::Rect, text::Line, widgets::Paragraph};
    let theme = app.theme();
    let key = crate::ui_chrome::key_token(app.shortcut("back"));
    let full = crate::editor::display_width(&key) + 5;
    let (key, width) = if !key.is_empty() && full <= usize::from(area.width) {
        (key, full as u16)
    } else if area.width >= NAVIGATION_MIN_WIDTH {
        (String::new(), NAVIGATION_MIN_WIDTH)
    } else {
        return;
    };
    let rect = Rect::new(area.x, area.y, width, 1);
    let focused = app.focus == app.hits.len();
    frame.render_widget(
        Paragraph::new(crate::ui_chrome::key_token_line(
            theme, &key, "Back", focused,
        )),
        rect,
    );
    app.hits.push(crate::app::HitRegion {
        rect,
        action: Action::Navigation(NavigationAction::Back),
    });
    let mut x = rect.right().saturating_add(2);
    let crumbs = app.navigation_trail.crumbs();
    let mut remaining = area.right().saturating_sub(x);
    // Display recent labels only, newest first while choosing. Older entries stay owned in
    // the bounded trail; a complete history picker is a separate pending adapter.
    let mut selected = vec![];
    for crumb in crumbs.iter().rev().take(2) {
        let label = crate::app::safe_text(crumb.label);
        let width = crate::editor::display_width(&label);
        let label = if width <= usize::from(remaining) {
            label
        } else if selected.is_empty() && remaining >= CRUMB_MIN_WIDTH {
            crate::ui_chrome::fit(&label, usize::from(remaining))
        } else {
            break;
        };
        let width = crate::editor::display_width(&label) as u16;
        remaining = remaining.saturating_sub(width.saturating_add(2));
        selected.push((label, width, crumb.link.clone()));
    }
    for (label, width, link) in selected.into_iter().rev() {
        let rect = Rect::new(x, area.y, width, 1);
        let mut line = Line::styled(label, theme.role(Role::Text));
        if app.focus == app.hits.len() {
            line = line.patch_style(Style::default().add_modifier(Modifier::UNDERLINED));
        }
        frame.render_widget(Paragraph::new(line), rect);
        app.hits.push(crate::app::HitRegion {
            rect,
            action: Action::Navigation(NavigationAction::Breadcrumb(link)),
        });
        x = rect.right().saturating_add(2);
    }
}

#[cfg(test)]
#[path = "navigation_state_tests.rs"]
mod tests;
