//! Local workspace state retains Git identities even when labels are simplified.
mod file_history;
pub(crate) mod original_diff_sides_route_v1;
mod original_diff_sides_v1;
pub use original_diff_sides_v1::{
    OriginalDiffReturnTicketV1, PreparedOriginalDiffBackV1, PreparedOriginalDiffSideV1,
};
pub(crate) mod tree_jump_route_v1;
mod tree_jump_v1;
pub use tree_jump_v1::{
    OriginalTreeNavigationActionV1, OriginalTreeNodeTicketV1, PreparedOriginalTreeNavigationV1,
};
#[path = "workspace/graph_refresh_v3.rs"]
mod graph_refresh_v3;
#[path = "workspace/graph_restore_v2.rs"]
mod graph_restore_v2;
#[path = "workspace/graph_v1.rs"]
mod graph_v1;
#[cfg(test)]
#[path = "workspace/original_saved_boundaries_v1_tests.rs"]
mod original_saved_boundaries_v1_tests;
pub(crate) mod original_saved_navigation_v1;
mod remote_handoff_v1;
use crate::{
    git::{Commit, Comparison, ComparisonResult, GitRepository, RawPath, RepoStatus, TreeEntry},
    identity::GitRole,
    providers::runner::CancellationToken,
    repository_view::{
        diff_rows, file_preview, workspace_rows, CommitGraph, DiffAnchor, DiffLayout, FilePreview,
        SemanticRole, TreeBrowser, TreeKind,
    },
};
pub use file_history::{change_word_v1, FileAction, FileProvenance, FileSelection, ProvenanceView};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RepositoryMode {
    #[default]
    Overview,
    Graph,
    Tree,
    Working,
    Staged,
    Branch,
    CommitChanges,
    /// All tracked index/working edits relative to the observed saved baseline.
    Uncommitted,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, process::Command};

    fn git(directory: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(directory)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    #[test]
    fn commit_changes_workspace_labels_ordered_parents_and_keeps_selected_saved_identity() {
        let directory = tempfile::tempdir().unwrap();
        git(directory.path(), &["init", "--initial-branch=main"]);
        git(directory.path(), &["config", "user.name", "Fixture"]);
        git(
            directory.path(),
            &["config", "user.email", "fixture@example.invalid"],
        );
        fs::write(directory.path().join("root"), b"root bytes\n").unwrap();
        git(directory.path(), &["add", "root"]);
        git(directory.path(), &["commit", "-m", "root"]);
        let root = git(directory.path(), &["rev-parse", "HEAD"])
            .trim()
            .to_owned();
        git(directory.path(), &["switch", "-c", "feature"]);
        fs::write(directory.path().join("feature"), b"feature content\n").unwrap();
        git(directory.path(), &["add", "feature"]);
        git(directory.path(), &["commit", "-m", "feature"]);
        git(directory.path(), &["switch", "main"]);
        fs::write(directory.path().join("main"), b"main content\n").unwrap();
        git(directory.path(), &["add", "main"]);
        git(directory.path(), &["commit", "-m", "main"]);
        git(
            directory.path(),
            &["merge", "--no-ff", "feature", "-m", "merge"],
        );
        let merge = git(directory.path(), &["rev-parse", "HEAD"])
            .trim()
            .to_owned();
        fs::write(directory.path().join("root"), b"dirty bytes\n").unwrap();
        let index = fs::read(directory.path().join(".git/index")).unwrap();
        let mut view = Workspace::load(
            directory.path(),
            RepositoryMode::CommitChanges,
            &merge,
            None,
        )
        .unwrap()
        .unwrap();
        assert!(view
            .rows
            .iter()
            .any(|row| row.label.contains("+feature content")));
        assert!(view
            .rows
            .iter()
            .any(|row| row.label.starts_with("Compare to first parent (1):")));
        assert_eq!(view.revision, merge);
        view.open(RowAction::ComparisonParent {
            commit: merge.clone(),
            parent: 1,
        })
        .unwrap();
        assert!(view
            .rows
            .iter()
            .any(|row| row.label.contains("+main content")));
        assert!(!view
            .rows
            .iter()
            .any(|row| row.label.contains("+feature content")));
        assert_eq!(view.selected_commit.as_ref().unwrap().id, merge);
        let first = Workspace::load(directory.path(), RepositoryMode::CommitChanges, &root, None)
            .unwrap()
            .unwrap();
        assert_eq!(first.revision, root);
        assert!(first
            .rows
            .iter()
            .any(|row| row.label.contains("+root bytes")));
        assert!(first
            .rows
            .iter()
            .any(|row| row.label.contains("empty starting tree")));
        assert_eq!(git(directory.path(), &["rev-parse", "HEAD"]).trim(), merge);
        assert_eq!(
            fs::read(directory.path().join(".git/index")).unwrap(),
            index
        );
        assert_eq!(
            fs::read(directory.path().join("root")).unwrap(),
            b"dirty bytes\n"
        );
    }

    #[test]
    fn git_provider_revision_tree_workspace_retains_raw_identity_and_checkout() {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "--initial-branch=main"]);
        git(dir.path(), &["config", "user.name", "Fixture"]);
        git(
            dir.path(),
            &["config", "user.email", "fixture@example.invalid"],
        );
        fs::create_dir(dir.path().join("nested")).unwrap();
        fs::write(dir.path().join("nested/日本語.txt"), b"saved text\n").unwrap();
        git(dir.path(), &["add", "."]);
        git(dir.path(), &["commit", "-m", "saved"]);
        let head = git(dir.path(), &["rev-parse", "HEAD"]);
        fs::write(
            dir.path().join("nested/日本語.txt"),
            b"local uncommitted text\n",
        )
        .unwrap();
        let index = fs::read(dir.path().join(".git/index")).unwrap();
        let mut workspace = Workspace::load(dir.path(), RepositoryMode::Tree, "HEAD", None)
            .unwrap()
            .unwrap();
        workspace
            .open(RowAction::Directory(RawPath::new(b"nested".to_vec())))
            .unwrap();
        let file = RawPath::new("nested/日本語.txt".as_bytes().to_vec());
        assert!(workspace
            .rows
            .iter()
            .any(|row| row.action == Some(RowAction::File(file.clone()))));
        workspace.open(RowAction::File(file.clone())).unwrap();
        assert_eq!(workspace.preview.as_ref().unwrap().path, file);
        assert_eq!(workspace.preview.as_ref().unwrap().content, b"saved text\n");
        assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), head);
        assert_eq!(fs::read(dir.path().join(".git/index")).unwrap(), index);
        assert_eq!(
            fs::read(dir.path().join("nested/日本語.txt")).unwrap(),
            b"local uncommitted text\n"
        );
    }

    #[test]
    fn local_workspace_unborn_branch_shows_useful_empty_state() {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "--initial-branch=main"]);
        let workspace = Workspace::load(dir.path(), RepositoryMode::Branch, "HEAD", None)
            .unwrap()
            .unwrap();
        assert!(workspace
            .rows
            .iter()
            .any(|row| row.label.contains("No saved versions yet")));
        assert_eq!(workspace.status.branch.as_deref(), Some("main"));
    }
    #[test]
    fn exact_search_native_destinations_preserve_saved_objects_and_reject_symlink_ancestors() {
        use std::os::unix::fs::symlink;
        let directory = tempfile::tempdir().unwrap();
        git(directory.path(), &["init", "--initial-branch=main"]);
        git(directory.path(), &["config", "user.name", "Fixture"]);
        git(
            directory.path(),
            &["config", "user.email", "fixture@example.invalid"],
        );
        fs::create_dir(directory.path().join("nested")).unwrap();
        fs::write(directory.path().join("nested/file.txt"), b"saved text\n").unwrap();
        git(directory.path(), &["add", "."]);
        git(directory.path(), &["commit", "-m", "saved"]);
        let head = git(directory.path(), &["rev-parse", "HEAD"]);
        let index = fs::read(directory.path().join(".git/index")).unwrap();
        fs::write(
            directory.path().join("nested/file.txt"),
            b"fresh working text\n",
        )
        .unwrap();
        let path = RawPath::new(b"nested/file.txt".to_vec());
        let saved = Workspace::open_search_file(
            directory.path(),
            &path,
            Some(head.trim()),
            1,
            Default::default(),
        )
        .unwrap();
        assert_eq!(saved.preview.unwrap().content, b"saved text\n");
        let working =
            Workspace::open_search_file(directory.path(), &path, None, 1, Default::default())
                .unwrap();
        assert_eq!(working.preview.unwrap().content, b"fresh working text\n");
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("private.txt"), b"not in corpus").unwrap();
        symlink(outside.path(), directory.path().join("link")).unwrap();
        assert!(Workspace::open_search_file(
            directory.path(),
            &RawPath::new(b"link/private.txt".to_vec()),
            None,
            1,
            Default::default()
        )
        .is_err());
        assert!(Workspace::open_search_file(
            directory.path(),
            &RawPath::new(b"../private.txt".to_vec()),
            None,
            1,
            Default::default()
        )
        .is_err());
        assert_eq!(git(directory.path(), &["rev-parse", "HEAD"]), head);
        assert_eq!(
            fs::read(directory.path().join(".git/index")).unwrap(),
            index
        );
        assert_eq!(
            fs::read(directory.path().join("nested/file.txt")).unwrap(),
            b"fresh working text\n"
        );
    }
}

#[cfg(test)]
#[path = "workspace/native_uncommitted_v3_tests.rs"]
mod native_uncommitted_v3_tests;

impl RepositoryMode {
    pub const ALL: [(Self, &'static str, char); 8] = [
        (Self::Overview, "Overview", 'o'),
        (Self::Graph, "Graph", 'g'),
        (Self::Tree, "Tree", 't'),
        (Self::Working, "Working diff", 'w'),
        (Self::Staged, "Staged diff", 'i'),
        (Self::Branch, "Branch changes", 'b'),
        (Self::CommitChanges, "Changes", 'd'),
        (Self::Uncommitted, "All edits", 'e'),
    ];
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowAction {
    OriginalDiffReturn(OriginalDiffReturnTicketV1),
    FileProvenance(FileAction),
    Tree(OriginalTreeNavigationActionV1),
    Commit(String),
    Directory(RawPath),
    File(RawPath),
    Conflict(RawPath),
    WorkingFile(RawPath),
    DiffLine(DiffAnchor),
    ComparisonParent {
        commit: String,
        parent: usize,
    },
    Legend,
    Graph(crate::repository_graph_v1::GraphActionV1),
    Actor(GitRole),
    /// A paused-operation shortcut row; App opens the same LocalCommand form as the menu.
    Local(crate::local_actions::LocalCommand),
}

#[derive(Clone, Debug)]
pub struct DisplayRow {
    pub label: String,
    pub role: SemanticRole,
    pub action: Option<RowAction>,
    /// Table cells next to the `label` (which palette, plans and logs keep): Overview's
    /// recent commits are drawn as aligned COMMIT/SUBJECT/AUTHOR/AGE/DATE columns.
    pub cells: Option<Box<CommitCellsV91>>,
}

/// One recent commit of the Overview as table cells (TASK-91). The full id and the authored
/// UTC date stay here and in the Saved/actor detail rows above the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitCellsV91 {
    pub id: String,
    pub subject: String,
    /// One local name when author and committer agree, both roles otherwise.
    pub actors: String,
    /// Authored time, seconds since the epoch.
    pub authored: i64,
    /// Full `name <email>` of both roles and the committed time, for the Repository preview
    /// (TASK-93); the list cells stay short.
    pub author: String,
    pub committer: String,
    pub committed: i64,
    /// The one local actor when author and committer agree, for the AUTHOR initials chip.
    pub actor: Option<crate::git::GitActor>,
}

#[derive(Clone, Debug)]
pub struct Workspace {
    pub directory: PathBuf,
    pub revision: String,
    pub mode: RepositoryMode,
    pub status: RepoStatus,
    pub graph: CommitGraph,
    pub graph_state_v1: Option<Arc<crate::repository_graph_v1::OriginalGraphStateV1>>,
    pub graph_display_v1: Option<Arc<crate::repository_graph_v1::GraphPresentationV1>>,
    pub graph_view_v1: crate::repository_graph_v1::GraphViewV1,
    pub graph_horizontal_v1: usize,
    pub(crate) graph_projection_start_v2: usize,
    pub tree: Option<TreeBrowser>,
    pub tree_navigation_v1: Option<Arc<PreparedOriginalTreeNavigationV1>>,
    pub tree_breadcrumbs_v2: Option<Arc<crate::repository_tree_viewport_v2::TreeBreadcrumbStripV2>>,
    pub comparison: Option<ComparisonResult>,
    pub original_comparison_v1: Option<Arc<crate::git::OriginalComparisonSourceV1>>,
    pub original_diff_side_v1: Option<Arc<PreparedOriginalDiffSideV1>>,
    pub preview: Option<FilePreview>,
    pub file_provenance: Option<FileProvenance>,
    pub selected_commit: Option<Commit>,
    /// Genuine original worker source, not host association/permission.
    pub actor_origin: Option<Arc<crate::git::OriginalLocalActorOriginV1>>,
    /// Actual accepted Actor(role) intent; never inferred from prepared labels.
    pub actor_profile: Option<GitRole>,
    pub rows: Vec<DisplayRow>,
}

impl Workspace {
    /// Borrow only the currently admitted immutable projection. No model/body clone.
    pub fn display_rows_v1(&self) -> &[DisplayRow] {
        if self.actor_profile.is_some()
            || self.file_provenance.as_ref().is_some_and(|state| {
                !matches!(state.view, ProvenanceView::File | ProvenanceView::Graph)
            })
            || self.preview.is_some()
        {
            return &self.rows;
        }
        if self.graph_visible_v1() {
            if let Some(graph) = &self.graph_display_v1 {
                return graph.rows();
            }
        }
        // Local TreeA inserts its exact visible tree Arc clause here, not another helper.
        if let Some(tree) = self
            .tree_navigation_v1
            .as_ref()
            .filter(|tree| tree.visible_for(self))
        {
            return tree.rows();
        }
        if let Some(side) = self
            .original_diff_side_v1
            .as_ref()
            .filter(|side| side.visible_for(self))
        {
            return side.rows();
        }
        if let Some(source) = self
            .original_comparison_v1
            .as_ref()
            .filter(|source| source.visible_for(self))
        {
            return source.rows();
        }
        &self.rows
    }
    /// Open a canonical search destination without checkout/index writes. Working
    /// files are freshly read and labelled mutable; saved files retain exact SHA.
    pub fn open_search_file(
        directory: &Path,
        path: &RawPath,
        revision: Option<&str>,
        line: usize,
        cancellation: CancellationToken,
    ) -> Result<Self, String> {
        let repo = GitRepository::discover_with_cancellation(directory, cancellation.clone())
            .map_err(|error| error.to_string())?
            .ok_or("Search repository no longer exists")?;
        let origin = crate::git::OriginalLocalActorOriginV1::capture(&repo).ok();
        Self::open_search_file_from_repository_v1(&repo, path, revision, line, cancellation, origin)
    }
    pub(crate) fn open_search_file_from_repository_v1(
        repo: &GitRepository,
        path: &RawPath,
        revision: Option<&str>,
        line: usize,
        cancellation: CancellationToken,
        origin: Option<Arc<crate::git::OriginalLocalActorOriginV1>>,
    ) -> Result<Self, String> {
        let mode = if revision.is_some() {
            RepositoryMode::Tree
        } else {
            RepositoryMode::Overview
        };
        let mut workspace =
            Self::load_from_repository_v1(repo, mode, revision.unwrap_or("HEAD"), None, origin)?
                .ok_or("Search repository is no longer accessible")?;
        if cancellation.is_cancelled() {
            return Err("Opening search result cancelled".into());
        }
        let (entry, content, source) = if revision.is_some() {
            let entry = repo
                .tree_entry(&workspace.revision, path)
                .map_err(|error| error.to_string())?;
            if entry.kind != "blob" || !matches!(entry.mode.as_str(), "100644" | "100755") {
                return Err("Search destination is not a regular saved file".into());
            }
            let sizes = repo
                .object_sizes(std::slice::from_ref(&entry.id))
                .map_err(|error| error.to_string())?;
            if sizes.first().is_none_or(|size| *size > 2 * 1024 * 1024) {
                return Err("Search file viewer limit is 2 MiB; file was not read".into());
            }
            let content = repo
                .blob(&workspace.revision, path)
                .map_err(|error| error.to_string())?;
            (
                entry,
                content,
                format!("saved version {} • immutable", workspace.revision),
            )
        } else {
            let content = read_working_file(&workspace.directory, path, 2 * 1024 * 1024)?;
            (
                TreeEntry {
                    path: path.clone(),
                    id: "working file (not a saved object)".into(),
                    kind: "blob".into(),
                    mode: "100644".into(),
                },
                content,
                "working files • mutable, freshly read".into(),
            )
        };
        if cancellation.is_cancelled() {
            return Err("Opening search result cancelled".into());
        }
        workspace.rows.clear();
        workspace.push(
            &format!("{}:{line} / {source}", path.display()),
            SemanticRole::Reference,
            None,
        );
        let preview = file_preview(
            revision.map_or_else(|| "WORKTREE".into(), |_| workspace.revision.clone()),
            &entry,
            content,
            10_000,
        );
        workspace.file_provenance =
            revision.map(|_| FileProvenance::new(workspace.revision.clone(), path.clone()));
        let start = line.saturating_sub(9);
        for row in preview.rows.iter().skip(start) {
            workspace.push(
                &format!(
                    "{} {:>5} {}",
                    if row.line.map(|value| value as usize) == Some(line) {
                        ">"
                    } else {
                        " "
                    },
                    row.line.map_or(String::new(), |line| line.to_string()),
                    row.label
                ),
                SemanticRole::Normal,
                if revision.is_some()
                    && preview.kind == crate::repository_view::BlobPreviewKind::Text
                {
                    row.line
                        .map(|line| RowAction::FileProvenance(FileAction::BlameAt(line)))
                } else {
                    None
                },
            );
        }
        if preview.truncated {
            workspace.push(
                "Preview stops after 10000 lines.",
                SemanticRole::Boundary,
                None,
            );
        }
        workspace.preview = Some(preview);
        if let Some(origin) = &workspace.actor_origin {
            origin
                .recheck(&cancellation)
                .map_err(|error| error.to_string())?;
        }
        Ok(workspace)
    }
    pub fn load(
        directory: &Path,
        mode: RepositoryMode,
        revision: &str,
        previous_tree: Option<TreeBrowser>,
    ) -> Result<Option<Self>, String> {
        Self::load_with_cancellation(
            directory,
            mode,
            revision,
            previous_tree,
            CancellationToken::default(),
        )
    }

    pub fn load_with_cancellation(
        directory: &Path,
        mode: RepositoryMode,
        revision: &str,
        previous_tree: Option<TreeBrowser>,
        cancellation: CancellationToken,
    ) -> Result<Option<Self>, String> {
        let Some(repo) = GitRepository::discover_with_cancellation(directory, cancellation)
            .map_err(|e| e.to_string())?
        else {
            return Ok(None);
        };
        let actor_origin = crate::git::OriginalLocalActorOriginV1::capture(&repo).ok();
        Self::load_from_repository_v1(&repo, mode, revision, previous_tree, actor_origin)
    }
    pub(crate) fn load_from_repository_v1(
        repo: &GitRepository,
        mode: RepositoryMode,
        revision: &str,
        previous_tree: Option<TreeBrowser>,
        actor_origin: Option<Arc<crate::git::OriginalLocalActorOriginV1>>,
    ) -> Result<Option<Self>, String> {
        let status = repo.status().map_err(|e| e.to_string())?;
        let revision = if status.head.is_none() {
            "HEAD".into()
        } else {
            repo.resolve_original(
                if matches!(
                    mode,
                    RepositoryMode::Graph | RepositoryMode::Tree | RepositoryMode::CommitChanges
                ) {
                    revision
                } else {
                    "HEAD"
                },
            )
            .map_err(|e| e.to_string())?
        };
        let mut workspace = Self {
            directory: repo.root.clone(),
            revision,
            mode,
            status,
            graph: CommitGraph::default(),
            graph_state_v1: None,
            graph_display_v1: None,
            graph_view_v1: Default::default(),
            graph_horizontal_v1: 0,
            graph_projection_start_v2: 0,
            tree: None,
            tree_navigation_v1: None,
            tree_breadcrumbs_v2: None,
            comparison: None,
            original_comparison_v1: None,
            original_diff_side_v1: None,
            preview: None,
            file_provenance: None,
            selected_commit: None,
            actor_origin,
            actor_profile: None,
            rows: Vec::new(),
        };
        match mode {
            RepositoryMode::Overview => {
                let paused = crate::repository_view::paused_operation_rows_v1(&workspace.status);
                workspace
                    .rows
                    .extend(workspace_rows(&workspace.status).into_iter().map(|row| {
                        let action = match row.path {
                            Some(path) if row.role == SemanticRole::Conflict => {
                                Some(RowAction::Conflict(path))
                            }
                            Some(path) => Some(RowAction::WorkingFile(path)),
                            None => paused.iter().find_map(|(label, _, command)| {
                                (*label == row.label)
                                    .then_some(*command)
                                    .flatten()
                                    .map(RowAction::Local)
                            }),
                        };
                        DisplayRow {
                            label: row.label,
                            role: row.role,
                            action,
                            cells: None,
                        }
                    }));
                // Saved stashes and linked worktrees are easy to forget; show them read-only.
                let stashes = repo.stash_overview_v1().unwrap_or_default();
                if let Some(newest) = stashes.first() {
                    workspace.push(
                        &format!(
                            "Saved stashes: {} • newest {} {} • m → Apply/Apply and remove/Delete stash (F4 choose)",
                            stashes.len(),
                            crate::providers::runner::sanitize_terminal(
                                newest.selector.strip_prefix("refs/").unwrap_or(&newest.selector)
                            ),
                            crate::providers::runner::sanitize_terminal(&newest.subject)
                        ),
                        SemanticRole::Reference,
                        None,
                    );
                }
                let linked = repo.linked_worktree_count_v1();
                if linked > 0 {
                    workspace.push(
                        &format!(
                            "Linked worktrees: {} besides this one • m → Add/Remove linked worktree",
                            linked
                        ),
                        SemanticRole::Reference,
                        None,
                    );
                }
                workspace.push(
                    "Recent commits — saved versions",
                    SemanticRole::Reference,
                    None,
                );
                let mut recent = vec![];
                for commit in repo.log(12).map_err(|e| e.to_string())? {
                    if workspace.selected_commit.is_none() {
                        workspace.selected_commit = Some(commit.clone());
                    }
                    let label = format!(
                        "● {} {} • {}",
                        &commit.id[..8.min(commit.id.len())],
                        commit.subject,
                        crate::git_actor_labels_v1::commit_summary(&commit)
                    );
                    recent.push(commit.id.clone());
                    let cells = CommitCellsV91 {
                        id: commit.id.clone(),
                        subject: commit.subject.clone(),
                        actors: crate::git_actor_labels_v1::commit_actors(&commit),
                        authored: commit.author.timestamp,
                        author: crate::git_actor_labels_v1::actor_text(&commit.author),
                        committer: crate::git_actor_labels_v1::actor_text(&commit.committer),
                        committed: commit.committer.timestamp,
                        actor: (commit.author.name == commit.committer.name
                            && commit.author.email == commit.committer.email)
                            .then(|| commit.author.clone()),
                    };
                    workspace.push(
                        &label,
                        SemanticRole::Normal,
                        Some(RowAction::Commit(commit.id)),
                    );
                    if let Some(row) = workspace.rows.last_mut() {
                        row.cells = Some(Box::new(cells));
                    }
                }
                // Raw local HEAD reflog file (no Git child); bounded to its newest 1 MiB.
                let log = std::fs::read(repo.git_dir.join("logs/HEAD")).unwrap_or_default();
                let tail = &log[log.len().saturating_sub(1024 * 1024)..];
                let candidates = crate::repository_view::rescue_candidates_v1(tail, &recent, 8);
                if !candidates.is_empty() {
                    workspace.push(
                        "Left behind by amend/reset/rebase — local reflog only, entries expire; not a universal undo",
                        SemanticRole::Boundary,
                        None,
                    );
                    workspace.push(
                        "Recover: select one, Enter to inspect it, then m → Create rescue branch (version prefilled)",
                        SemanticRole::Boundary,
                        None,
                    );
                    for candidate in candidates {
                        workspace.push(
                            &format!(
                                "↺ {} {} • {} • {}",
                                &candidate.id[..8],
                                if candidate.subject.is_empty() {
                                    "(subject not in reflog)"
                                } else {
                                    &candidate.subject
                                },
                                candidate.reason,
                                candidate.selector
                            ),
                            SemanticRole::Changed,
                            Some(RowAction::Commit(candidate.id)),
                        );
                    }
                }
            }
            RepositoryMode::Graph => workspace.initialize_graph_v1(repo)?,
            RepositoryMode::Tree => {
                if workspace.status.head.is_none() {
                    workspace.push(
                        "No saved version yet. Commit files before browsing their saved tree.",
                        SemanticRole::Boundary,
                        None,
                    );
                } else {
                    let tree = match previous_tree {
                        Some(tree) if tree.revision == workspace.revision => tree,
                        _ => TreeBrowser::new(
                            workspace.revision.clone(),
                            repo.tree_children(&workspace.revision, None)
                                .map_err(|e| e.to_string())?,
                        ),
                    };
                    workspace.set_tree(tree);
                }
            }
            RepositoryMode::CommitChanges => {
                if workspace.status.head.is_none() {
                    workspace.push(
                        "No saved versions yet. Save a commit before inspecting introduced changes.",
                        SemanticRole::Boundary,
                        None,
                    );
                } else {
                    workspace.set_commit_changes(repo, 0)?;
                }
            }
            RepositoryMode::Working
            | RepositoryMode::Staged
            | RepositoryMode::Uncommitted
            | RepositoryMode::Branch => {
                if workspace.status.head.is_none() && mode == RepositoryMode::Branch {
                    workspace.push(
                        "No saved versions yet. Commit files before comparing branch history.",
                        SemanticRole::Boundary,
                        None,
                    );
                    return Ok(Some(workspace));
                }
                let comparison = match mode {
                    RepositoryMode::Working => Comparison::Working,
                    RepositoryMode::Staged => Comparison::Staged,
                    RepositoryMode::Uncommitted => Comparison::Uncommitted,
                    RepositoryMode::Branch => Comparison::BranchChanges {
                        base: workspace
                            .status
                            .upstream
                            .clone()
                            .unwrap_or_else(|| "HEAD".into()),
                        head: "HEAD".into(),
                    },
                    _ => unreachable!(),
                };
                if mode == RepositoryMode::Branch && workspace.status.upstream.is_none() {
                    workspace.push("No upstream branch is configured; comparing HEAD to itself. Choose an upstream to compare proposed changes.",SemanticRole::Boundary,None);
                }
                let result = repo.compare(&comparison).map_err(|e| e.to_string())?;
                workspace.push(&result.description, SemanticRole::Reference, None);
                if mode == RepositoryMode::Uncommitted {
                    workspace.push(
                        if workspace.status.head.is_some() {
                            "All tracked edits since saved HEAD (staged + unstaged); untracked files are separate and not included."
                        } else {
                            "All tracked edits before the first saved HEAD; untracked files are separate and not included."
                        },
                        SemanticRole::Reference,
                        None,
                    );
                }
                if result.diff.files.is_empty() {
                    workspace.push("No changes in this comparison.", SemanticRole::Normal, None);
                }
                for row in diff_rows(&result.diff, DiffLayout::Unified, true) {
                    workspace.rows.push(DisplayRow {
                        label: row.left.label,
                        role: row.left.role,
                        action: row.left.anchor.map(RowAction::DiffLine),
                        cells: None,
                    });
                }
                workspace.comparison = Some(result);
            }
        }
        if workspace.selected_commit.is_none() && workspace.status.head.is_some() {
            workspace.selected_commit = repo
                .log_page(&workspace.revision, 0, 1)
                .map_err(|error| error.to_string())?
                .into_iter()
                .next();
        }
        if workspace
            .actor_origin
            .as_ref()
            .is_some_and(|origin| origin.recheck(repo.actual_cancellation_v1()).is_err())
        {
            workspace.actor_origin = None;
        }
        Ok(Some(workspace))
    }

    fn push(&mut self, label: &str, role: SemanticRole, action: Option<RowAction>) {
        self.rows.push(DisplayRow {
            label: label.into(),
            role,
            action,
            cells: None,
        });
    }

    fn set_commit_changes(&mut self, repo: &GitRepository, parent: usize) -> Result<(), String> {
        let selected = repo
            .log_page(&self.revision, 0, 1)
            .map_err(|error| error.to_string())?
            .into_iter()
            .next()
            .ok_or("Selected saved version is unavailable")?;
        let result = repo
            .compare(&Comparison::CommitChanges {
                commit: selected.id.clone(),
                parent,
            })
            .map_err(|error| error.to_string())?;
        self.rows.clear();
        self.push(
            &format!("Saved commit changes • {}", selected.id),
            SemanticRole::Reference,
            None,
        );
        self.push(&result.description, SemanticRole::Reference, None);
        if selected.parents.is_empty() {
            self.push(
                "Root commit — compared to an empty starting tree",
                SemanticRole::Reference,
                None,
            );
        } else {
            for (index, id) in selected.parents.iter().enumerate() {
                self.push(
                    &if index == 0 {
                        format!("Compare to first parent (1): {id}")
                    } else {
                        format!("Compare to parent {}: {id}", index + 1)
                    },
                    SemanticRole::Reference,
                    Some(RowAction::ComparisonParent {
                        commit: selected.id.clone(),
                        parent: index,
                    }),
                );
            }
            self.push(
                &format!(
                    "Compared parent {}: {} → {}",
                    parent + 1,
                    selected.parents[parent],
                    selected.id
                ),
                SemanticRole::Reference,
                None,
            );
        }
        if result.diff.files.is_empty() {
            self.push("No changes in this comparison.", SemanticRole::Normal, None);
        }
        self.rows.extend(
            diff_rows(&result.diff, DiffLayout::Unified, true)
                .into_iter()
                .map(|row| DisplayRow {
                    label: row.left.label,
                    role: row.left.role,
                    action: row.left.anchor.map(RowAction::DiffLine),
                    cells: None,
                }),
        );
        self.mode = RepositoryMode::CommitChanges;
        self.revision = selected.id.clone();
        self.selected_commit = Some(selected);
        self.preview = None;
        self.tree = None;
        self.comparison = Some(result);
        Ok(())
    }

    pub fn set_tree(&mut self, tree: TreeBrowser) {
        self.rows.clear();
        self.push(
            "Saved file tree — browsing never checks out or changes your working files",
            SemanticRole::Reference,
            None,
        );
        for row in tree.visible_rows() {
            let action = match row.kind {
                TreeKind::Directory => Some(RowAction::Directory(row.path)),
                TreeKind::File | TreeKind::Symlink | TreeKind::Submodule => {
                    Some(RowAction::File(row.path))
                }
            };
            self.push(&row.label, SemanticRole::Normal, action);
        }
        self.tree = Some(tree);
    }

    pub fn open(&mut self, action: RowAction) -> Result<(), String> {
        if matches!(action, RowAction::OriginalDiffReturn(_)) {
            return Err("Original comparison Back requires its captured typed worker owner; no source/clock was inferred".into());
        }
        self.open_with_cancellation(action, CancellationToken::default())
    }

    pub fn open_with_cancellation(
        &mut self,
        action: RowAction,
        cancellation: CancellationToken,
    ) -> Result<(), String> {
        if matches!(action, RowAction::OriginalDiffReturn(_)) {
            return Err("Original comparison Back requires its captured typed worker owner; no source/clock was inferred".into());
        }
        let pure = matches!(&action, RowAction::Graph(action) if !matches!(action, crate::repository_graph_v1::GraphActionV1::More | crate::repository_graph_v1::GraphActionV1::CompareParent(_)))
            || matches!(
                &action,
                RowAction::Actor(_)
                    | RowAction::WorkingFile(_)
                    | RowAction::DiffLine(_)
                    | RowAction::Legend
            )
            || matches!(&action,RowAction::Commit(sha) if self.graph_state_v1.as_ref().is_some_and(|state| state.selected_commit(sha).is_some()) || self.graph.rows.iter().any(|row|row.commit.id==*sha));
        if pure {
            return self.open_from_repository_v1(action, cancellation, None);
        }
        let origin = self
            .actor_origin
            .as_ref()
            .ok_or("Original loaded Git source unavailable; no path rediscovery")?
            .clone();
        let operation =
            crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1::begin(
                &origin,
                cancellation,
            )?;
        let result = self.open_original_with_operation_v1(action, &operation);
        let sealed = operation.seal();
        match (result, sealed) {
            (Err(reason), _) => Err(reason),
            (Ok(_), Err(reason)) => Err(reason),
            (Ok(_), Ok(_)) => Ok(()),
        }
    }
    pub(crate) fn open_from_repository_v1(
        &mut self,
        action: RowAction,
        cancellation: CancellationToken,
        repository: Option<&GitRepository>,
    ) -> Result<(), String> {
        if matches!(action, RowAction::OriginalDiffReturn(_)) {
            return Err("Original comparison Back requires its captured typed worker owner; no source/clock was inferred".into());
        }
        let was_graph = self.graph_visible_v1();
        self.actor_profile = None;
        match action {
            RowAction::OriginalDiffReturn(_) => return Err("Original comparison Back requires its captured typed worker owner; no source/clock was inferred".into()),
            RowAction::Local(_) => {
                return Err("Local operation rows open their explicit preview form".into())
            }
            RowAction::Graph(action) => return self.graph_action_v1(action, repository),
            RowAction::FileProvenance(action) => {
                return self.provenance_action_from_repository_v1(
                    repository.ok_or("Original provenance source unavailable")?,
                    action,
                    cancellation,
                )
            }
            RowAction::Tree(_) => return Err("Original tree rows require the captured typed worker owner; no new source/clock inferred".into()),
            RowAction::ComparisonParent { commit, parent } => {
                let repo = repository.ok_or("Original nested repository handle unavailable")?;
                self.revision = repo
                    .resolve_original(&commit)
                    .map_err(|error| error.to_string())?;
                self.set_commit_changes(repo, parent)?;
            }
            RowAction::WorkingFile(path) => {
                // Retain the exact selection for the local operation menu; opening is read-only.
                // Only the latest selection note is kept, not one per Enter/click.
                self.rows.retain(|row| {
                    !(matches!(row.action, Some(RowAction::WorkingFile(_)))
                        && row.label.starts_with("Selected working file: "))
                });
                self.push(
                    &format!(
                        "Selected working file: {}. + Stage • - Unstage • K Commit • m Local operations.",
                        path.display()
                    ),
                    SemanticRole::Reference,
                    Some(RowAction::WorkingFile(path)),
                );
            }
            RowAction::DiffLine(anchor) => {
                // Selection is an immutable canonical coordinate, never inferred from labels.
                self.push(
                    "Selected change line. Use Local operations to preview staging this line.",
                    SemanticRole::Reference,
                    Some(RowAction::DiffLine(anchor)),
                );
            }
            RowAction::Commit(id) => {
                if self.graph_cached_commit_v1(&id)? {
                    return Ok(());
                }
                let row = self.graph.rows.iter().find(|row| row.commit.id == id);
                let explanation=row.map(|row|row.explanation.clone()).unwrap_or_else(||format!("Commit {id} — a saved version. Select Tree to browse files at this version."));
                self.selected_commit = if let Some(row) = row {
                    Some(row.commit.clone())
                } else {
                    let repo = repository.ok_or("Original nested repository handle unavailable")?;
                    if !crate::git::original_actor_saved_sha(&id) {
                        return Err("Uncached commit needs its original full saved SHA".into());
                    }
                    repo.log_page(&id, 0, 1)
                        .map_err(|error| error.to_string())?
                        .into_iter()
                        .next()
                };
                self.revision = id;
                self.push(&explanation, SemanticRole::Reference, None);
            }
            RowAction::Directory(path) => {
                let repo = repository.ok_or("Original nested repository handle unavailable")?;
                let mut tree = self.tree.take().ok_or("No file tree selected")?;
                if tree.revision != self.revision {
                    return Err("Original tree revision changed; no path retarget".into());
                }
                if !tree.expanded_v1(&path) {
                    tree.merge_bounded_keyed_v1(
                        repo.tree_children(&tree.revision, Some(&path))
                            .map_err(|e| e.to_string())?,
                    )?;
                }
                tree.toggle(&path);
                tree.focus = Some(path.clone());
                self.set_tree(tree);
            }
            RowAction::File(path) => {
                let repo = repository.ok_or("Original nested repository handle unavailable")?;
                if self
                    .tree
                    .as_ref()
                    .is_none_or(|tree| tree.revision != self.revision)
                {
                    return Err("Original tree revision changed; no path retarget".into());
                }
                let tree_row = self
                    .tree
                    .as_ref()
                    .and_then(|tree| tree.visible_rows().into_iter().find(|row| row.path == path))
                    .ok_or("Selected file is no longer visible at this revision")?;
                if let Some(tree) = self.tree.as_mut() {
                    tree.focus = Some(path.clone());
                }
                let entry = TreeEntry {
                    path: path.clone(),
                    id: tree_row.object_id,
                    kind: if tree_row.kind == TreeKind::Submodule {
                        "commit"
                    } else {
                        "blob"
                    }
                    .into(),
                    mode: match tree_row.kind {
                        TreeKind::Symlink => "120000",
                        TreeKind::Submodule => "160000",
                        _ => "100644",
                    }
                    .into(),
                };
                self.file_provenance =
                    Some(FileProvenance::new(self.revision.clone(), path.clone()));
                self.preview = None;
                let large = tree_row.kind != TreeKind::Submodule
                    && repo
                        .object_sizes(std::slice::from_ref(&entry.id))
                        .map_err(|error| error.to_string())?
                        .first()
                        .is_none_or(|size| *size > 2 * 1024 * 1024);
                let bytes = if tree_row.kind == TreeKind::Submodule || large {
                    Vec::new()
                } else {
                    repo.blob(&self.revision, &path)
                        .map_err(|e| e.to_string())?
                };
                self.rows.clear();
                self.push(
                    &format!(
                        "{} at saved version {}",
                        path.display(),
                        &self.revision[..8.min(self.revision.len())]
                    ),
                    SemanticRole::Reference,
                    None,
                );
                if large {
                    self.push(
                        "Large file — preview limit is 2 MiB.",
                        SemanticRole::Boundary,
                        None,
                    );
                } else {
                    let preview = file_preview(self.revision.clone(), &entry, bytes, 10_000);
                    for row in &preview.rows {
                        self.push(
                            &format!(
                                "{:>5} {}",
                                row.line.map_or(String::new(), |line| line.to_string()),
                                row.label
                            ),
                            SemanticRole::Normal,
                            if preview.kind == crate::repository_view::BlobPreviewKind::Text {
                                row.line.map(|line| {
                                    RowAction::FileProvenance(FileAction::BlameAt(line))
                                })
                            } else {
                                None
                            },
                        );
                    }
                    if preview.truncated {
                        self.push(
                            "Preview is limited to 10000 lines.",
                            SemanticRole::Boundary,
                            None,
                        );
                    }
                    self.preview = Some(preview);
                }
            }
            RowAction::Conflict(path) => {
                let repo = repository.ok_or("Original nested repository handle unavailable")?;
                self.rows.clear();
                self.push(
                    &format!(
                        "Conflict in {} — compare common starting point, ours and theirs",
                        path.display()
                    ),
                    SemanticRole::Conflict,
                    Some(RowAction::Conflict(path.clone())),
                );
                for stage in repo
                    .conflict_stages()
                    .map_err(|e| e.to_string())?
                    .into_iter()
                    .filter(|stage| stage.path == path)
                {
                    self.push(
                        &format!("Stage {}: saved object {}", stage.stage, stage.id),
                        SemanticRole::Reference,
                        Some(RowAction::Conflict(path.clone())),
                    );
                    self.push(
                        match stage.stage {
                            1 => "Base — common starting content",
                            2 => "Ours — current branch content",
                            3 => "Theirs — incoming branch content",
                            _ => "Unknown index stage",
                        },
                        SemanticRole::Reference,
                        Some(RowAction::Conflict(path.clone())),
                    );
                    let content = repo.object(&stage.id).map_err(|error| error.to_string())?;
                    if content.contains(&0) {
                        self.push(
                            "Binary version — no text rendering",
                            SemanticRole::Boundary,
                            Some(RowAction::Conflict(path.clone())),
                        );
                    } else {
                        for line in content.split(|byte| *byte == b'\n').take(120) {
                            self.push(
                                &crate::git::display_bytes(line),
                                SemanticRole::Normal,
                                Some(RowAction::Conflict(path.clone())),
                            );
                        }
                    }
                }
                // The current working file (with any conflict markers) is what Stage records.
                let working = repo.root.join(path.as_os_str());
                match std::fs::symlink_metadata(&working) {
                    Ok(meta) if meta.is_file() && meta.len() <= 512 * 1024 => {
                        self.push(
                            "Working file now — this is what Stage selected file will record",
                            SemanticRole::Reference,
                            Some(RowAction::Conflict(path.clone())),
                        );
                        let content = std::fs::read(&working).unwrap_or_default();
                        if content.contains(&0) {
                            self.push(
                                "Binary working file — no text rendering",
                                SemanticRole::Boundary,
                                Some(RowAction::Conflict(path.clone())),
                            );
                        } else {
                            for line in content.split(|byte| *byte == b'\n').take(120) {
                                let marker = line.starts_with(b"<<<<<<<")
                                    || line.starts_with(b"=======")
                                    || line.starts_with(b">>>>>>>");
                                self.push(
                                    &crate::git::display_bytes(line),
                                    if marker {
                                        SemanticRole::Conflict
                                    } else {
                                        SemanticRole::Normal
                                    },
                                    Some(RowAction::Conflict(path.clone())),
                                );
                            }
                        }
                    }
                    Ok(_) => self.push(
                        "Working path is not a small regular file; inspect it outside ghtui",
                        SemanticRole::Boundary,
                        Some(RowAction::Conflict(path.clone())),
                    ),
                    Err(_) => self.push(
                        "Working file is absent (deleted on one side)",
                        SemanticRole::Boundary,
                        Some(RowAction::Conflict(path.clone())),
                    ),
                }
                self.push(
                    "Next: m → Choose conflict version or edit the file, then m → Stage selected file • o: Overview",
                    SemanticRole::Boundary,
                    Some(RowAction::Conflict(path)),
                );
            }
            RowAction::Legend if was_graph => {
                return self.graph_action_v1(
                    crate::repository_graph_v1::GraphActionV1::Legend,
                    repository,
                );
            }
            RowAction::Legend => {
                self.rows.clear();
                for label in crate::theme::Theme::from_settings(0, false).legend() {
                    self.push(&label, SemanticRole::Reference, None);
                }
            }
            RowAction::Actor(role) => {
                self.actor_profile = Some(role);
                self.rows.clear();
                self.push(
                    match role {
                        GitRole::Author => "Actor profile — Git author",
                        GitRole::Committer => "Actor profile — Git committer",
                    },
                    SemanticRole::Reference,
                    None,
                );
                if let Some(commit) = self.selected_commit.as_ref() {
                    let actor = match role {
                        GitRole::Author => &commit.author,
                        GitRole::Committer => &commit.committer,
                    };
                    let identity = crate::git_actor_labels_v1::display_identity(actor, role);
                    let label = format!("[{}] {}", identity.initials(), identity.label());
                    let saved_version = format!("Saved version: {}", commit.id);
                    self.push(&label, SemanticRole::Head, None);
                    self.push(&saved_version, SemanticRole::Reference, None);
                    self.push(
                        "GitHub account association: unavailable",
                        SemanticRole::Boundary,
                        None,
                    );
                    self.push(
                        "Git author created this change; Git committer saved it into this history.",
                        SemanticRole::Normal,
                        None,
                    );
                } else {
                    self.push(
                        "No saved version yet; no Git actor is available.",
                        SemanticRole::Boundary,
                        None,
                    );
                }
            }
        }
        Ok(())
    }
}

/// Walk beneath the verified repository using directory descriptors. Reject
/// symlinks in every component, not just the leaf, even if paths race a scan.
pub(crate) fn read_working_file(
    directory: &Path,
    path: &RawPath,
    limit: usize,
) -> Result<Vec<u8>, String> {
    use std::{
        ffi::CString,
        fs::File,
        io::Read,
        os::{
            fd::{AsRawFd, FromRawFd, OwnedFd},
            unix::ffi::OsStrExt,
        },
    };
    let components = path.0.split(|byte| *byte == b'/').collect::<Vec<_>>();
    if components.is_empty()
        || components.iter().any(|component| {
            component.is_empty()
                || *component == b"."
                || *component == b".."
                || component.contains(&0)
        })
    {
        return Err("Invalid working-file destination".into());
    }
    let root = CString::new(directory.as_os_str().as_bytes()).map_err(|error| error.to_string())?;
    // SAFETY: C strings live through each call. Successful descriptors become
    // uniquely-owned OwnedFd and are closed on every success/error return.
    let raw = unsafe {
        libc::open(
            root.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        )
    };
    if raw < 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let mut parent = unsafe { OwnedFd::from_raw_fd(raw) };
    for (index, component) in components.iter().enumerate() {
        let name = CString::new(*component).map_err(|error| error.to_string())?;
        let leaf = index + 1 == components.len();
        let flags = libc::O_RDONLY
            | libc::O_CLOEXEC
            | libc::O_NOFOLLOW
            | if leaf {
                libc::O_NONBLOCK
            } else {
                libc::O_DIRECTORY
            };
        let raw = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags) };
        if raw < 0 {
            return Err(format!(
                "Cannot open regular search file: {}",
                std::io::Error::last_os_error()
            ));
        }
        let next = unsafe { OwnedFd::from_raw_fd(raw) };
        if leaf {
            let file = File::from(next);
            let metadata = file.metadata().map_err(|error| error.to_string())?;
            if !metadata.is_file() {
                return Err("Search destination is not a regular working file".into());
            }
            if metadata.len() > limit as u64 {
                return Err("Search file viewer limit is 2 MiB; file was not read".into());
            }
            let mut bytes = Vec::new();
            file.take(limit as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|error| error.to_string())?;
            if bytes.len() > limit {
                return Err("Working file grew beyond the preview limit".into());
            }
            return Ok(bytes);
        }
        parent = next;
    }
    Err("Invalid working-file destination".into())
}
