//! Ephemeral original local route evidence, not decoded navigation authority.
//! No content bodies, host reports, actor assets, old read budgets or SQL here.
use crate::{
    git::{Comparison, OriginalLocalActorOriginV1, RawPath},
    identity::GitRole,
    providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
    workspace::{FileSelection, ProvenanceView, RepositoryMode, Workspace},
};
use serde::Serialize;
#[path = "navigation_graph_refresh_v3.rs"]
mod graph_refresh_v3;
#[path = "navigation_graph_restore_v2.rs"]
mod graph_restore_v2;
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    io::{self, Write},
    os::unix::ffi::OsStrExt,
    path::Path,
    sync::Arc,
};

const MAX_ROUTE_BYTES: usize = 16 * 1024;
const MAX_RAW_PATH: usize = 4096;
const MAX_EXPANDED: usize = 64;
const MAX_VISITS: usize = 64;

#[derive(Clone, Debug, Serialize)]
pub(crate) struct OriginalFileSelectionV1 {
    pub commit: String,
    pub path: Option<RawPath>,
    pub original_line: Option<u32>,
    pub final_line: Option<u32>,
    pub boundary: bool,
    pub exists: bool,
}
impl OriginalFileSelectionV1 {
    pub(crate) fn matches(&self, actual: &FileSelection) -> bool {
        self.commit == actual.commit
            && self.path == actual.path
            && self.original_line == actual.original_line
            && self.final_line == actual.final_line
            && self.boundary == actual.boundary
            && self.exists == actual.exists
    }
}
#[derive(Clone, Debug, Serialize)]
pub(crate) struct OriginalFileRouteV1 {
    pub revision: String,
    pub path: RawPath,
    pub view: u8,
    pub return_view: u8,
    pub blame_start: usize,
    pub selection: Option<OriginalFileSelectionV1>,
}
#[derive(Clone, Debug, Serialize)]
pub(crate) struct OriginalComparisonRouteV1 {
    pub kind: u8,
    pub base: Option<String>,
    pub head: String,
    pub parent: Option<usize>,
}
#[derive(Clone, Debug, Serialize)]
pub(crate) struct OriginalGraphRouteV1 {
    pub walk_tip: String,
    /// The walk started from every ref, not from `walk_tip` alone (a branch scope).
    pub all_refs: bool,
    /// Fingerprint of the walk tips (branches, tags, HEAD) at this visit.
    pub walk: [u8; 32],
    pub selected: String,
    pub loaded: usize,
    pub epoch: [u8; 32],
    pub view: crate::repository_graph_v1::GraphViewV1,
    pub horizontal: usize,
    pub comparison_parent: Option<usize>,
}
#[derive(Clone, Debug, Serialize)]
pub(crate) struct OriginalLocalRouteV1 {
    pub mode: u8,
    pub graph: Option<OriginalGraphRouteV1>,
    pub revision: String,
    pub selected: Option<String>,
    pub role: Option<u8>,
    pub preview: Option<(String, RawPath, String)>,
    pub provenance: Option<OriginalFileRouteV1>,
    pub comparison: Option<OriginalComparisonRouteV1>,
    pub mutable_source: Option<[u8; 8]>,
    pub expanded: Vec<RawPath>,
}

pub struct OriginalSavedVisitV1 {
    graph_lease_v2: Option<Arc<crate::repository_graph_v1::restore_v2::GraphLeaseV2>>,
    graph_metadata_v3: Option<Arc<crate::repository_graph_v1::restore_v2::GraphVisitMetadataV3>>,
    origin: Arc<OriginalLocalActorOriginV1>,
    route: OriginalLocalRouteV1,
    key: [u8; 32],
    tree_v1: Option<Arc<crate::workspace::tree_jump_route_v1::OriginalTreeSavedRouteV1>>,
    diff_v1: Option<Arc<crate::workspace::original_diff_sides_route_v1::OriginalDiffSavedRouteV1>>,
}
impl std::fmt::Debug for OriginalSavedVisitV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OriginalSavedVisitV1(<ephemeral exact local route; no access>)")
    }
}
impl OriginalSavedVisitV1 {
    /// Capture from an already admitted genuine Workspace; no filesystem work.
    /// Raw metadata is preflighted before copies and never parsed from labels.
    pub fn capture(workspace: &Workspace) -> Result<Arc<Self>, String> {
        let origin = workspace
            .actor_origin
            .as_ref()
            .ok_or("Original saved source is unavailable")?;
        if origin.source().root() != workspace.directory {
            return Err("Original local source does not match this Workspace".into());
        }
        let mut meter = MAX_ROUTE_BYTES;
        charge(
            &mut meter,
            workspace.directory.as_os_str().as_bytes(),
            MAX_RAW_PATH,
        )?;
        sha(&workspace.revision)?;
        charge(&mut meter, workspace.revision.as_bytes(), 64)?;
        if let Some(commit) = &workspace.selected_commit {
            sha(&commit.id)?;
            charge(&mut meter, commit.id.as_bytes(), 64)?;
        }
        if workspace.actor_profile.is_some() && workspace.selected_commit.is_none() {
            return Err("Actor route has no actual selected saved commit".into());
        }
        if let Some(preview) = workspace.file_preview_v1() {
            sha(&preview.revision)?;
            sha(&preview.object_id)?;
            charge(&mut meter, &preview.path.0, MAX_RAW_PATH)?;
            charge(&mut meter, preview.revision.as_bytes(), 64)?;
            charge(&mut meter, preview.object_id.as_bytes(), 64)?;
        }
        if let Some(state) = &workspace.file_provenance {
            sha(&state.revision)?;
            charge(&mut meter, state.revision.as_bytes(), 64)?;
            charge(&mut meter, &state.path.0, MAX_RAW_PATH)?;
            if state.blame_start == 0 || state.blame_start > 2_000_000 {
                return Err("Original file page exceeds its source bounds".into());
            }
            if let Some(selection) = &state.selection {
                sha(&selection.commit)?;
                charge(&mut meter, selection.commit.as_bytes(), 64)?;
                if let Some(path) = &selection.path {
                    charge(&mut meter, &path.0, MAX_RAW_PATH)?;
                }
                validate_selected_member(state, selection)?;
            }
        }
        if let Some(comparison) = workspace.comparison_v1() {
            for value in [&comparison.base, &comparison.head].into_iter().flatten() {
                sha(value)?;
                charge(&mut meter, value.as_bytes(), 64)?;
            }
            if matches!(
                comparison.comparison,
                Comparison::Working | Comparison::Staged | Comparison::Uncommitted
            ) && comparison.observed.is_none()
            {
                return Err("Mutable comparison has no original observed source".into());
            }
        }
        let mut expanded = Vec::new();
        if let Some(tree) = workspace.tree.as_ref().filter(|_| {
            workspace.file_provenance.is_none() && workspace.file_preview_v1().is_none()
        }) {
            if tree.revision != workspace.revision {
                return Err("Tree is not at the chosen saved version".into());
            }
            for path in tree.expanded_paths_v1().take(MAX_EXPANDED + 1) {
                if expanded.len() == MAX_EXPANDED {
                    return Err("Original expanded tree exceeds64 directories".into());
                }
                charge(&mut meter, path, MAX_RAW_PATH)?;
                expanded.push(RawPath(path.to_vec()));
            }
        }
        let provenance = workspace.file_provenance.as_ref().map(|state| {
            // Only the active provenance source belongs to this route.
            // Cached pages from a previous mode are neither selected
            // membership nor a reason to change an exact restore key.
            let source = if state.selection.is_some() {
                state.return_view
            } else {
                state.view
            };
            OriginalFileRouteV1 {
                revision: state.revision.clone(),
                path: state.path.clone(),
                view: view_number(state.view),
                return_view: view_number(source),
                blame_start: if source == ProvenanceView::Blame {
                    state.blame_start
                } else {
                    1
                },
                selection: state
                    .selection
                    .as_ref()
                    .map(|selection| OriginalFileSelectionV1 {
                        commit: selection.commit.clone(),
                        path: selection.path.clone(),
                        original_line: selection.original_line,
                        final_line: selection.final_line,
                        boundary: selection.boundary,
                        exists: selection.exists,
                    }),
            }
        });
        let comparison = workspace.comparison_v1().and_then(|result| {
            let (kind, parent) = match result.comparison {
                Comparison::Snapshots { .. } => (1, None),
                Comparison::BranchChanges { .. } => (2, None),
                Comparison::CommitChanges { parent, .. } => (3, Some(parent)),
                _ => return None,
            };
            Some(OriginalComparisonRouteV1 {
                kind,
                base: result.base.clone(),
                head: result.head.clone()?,
                parent,
            })
        });
        let route = OriginalLocalRouteV1 {
            graph: workspace.graph_route_v1()?,
            mode: mode_number(workspace.mode),
            revision: workspace.revision.clone(),
            selected: workspace
                .selected_commit
                .as_ref()
                .map(|commit| commit.id.clone()),
            role: workspace.actor_profile.map(|role| match role {
                GitRole::Author => 0,
                GitRole::Committer => 1,
            }),
            preview: workspace.file_preview_v1().map(|preview| {
                (
                    preview.revision.clone(),
                    preview.path.clone(),
                    preview.object_id.clone(),
                )
            }),
            provenance,
            comparison,
            mutable_source: workspace.comparison_v1().and_then(|comparison| {
                comparison
                    .observed
                    .map(|observed| observed.navigation_identity())
            }),
            expanded,
        };
        let tree_v1 = workspace
            .tree_navigation_v1
            .as_ref()
            .filter(|tree| tree.visible_for(workspace))
            .map(|tree| {
                crate::workspace::tree_jump_route_v1::OriginalTreeSavedRouteV1::capture(tree)
                    .map(Arc::new)
            })
            .transpose()?;
        let diff_v1 =
            crate::workspace::original_diff_sides_route_v1::OriginalDiffSavedRouteV1::capture(
                workspace,
            )?
            .map(Arc::new);
        let mut hash = RouteHash {
            hash: Sha256::new(),
            left: MAX_ROUTE_BYTES,
        };
        // Address is only ephemeral owner identity. Registry retains this Arc,
        // so a freed-address/source collision cannot mint a new route proof.
        hash.write_all(&(Arc::as_ptr(origin) as usize).to_le_bytes())
            .map_err(|_| "Original route bound")?;
        serde_json::to_writer(&mut hash, &route).map_err(|_| "Original route exceeds16KiB")?;
        if let Some(tree) = &tree_v1 {
            hash.write_all(&tree.identity_bytes()?)
                .map_err(|_| "Original tree route exceeds shared16KiB metadata limit")?;
        }
        if let Some(diff) = &diff_v1 {
            hash.write_all(&diff.identity_bytes()?)
                .map_err(|_| "Original comparison side route exceeds shared16KiB metadata limit")?;
        }
        let key = hash.hash.finalize().into();
        Ok(Arc::new(Self {
            origin: origin.clone(),
            route,
            key,
            graph_lease_v2: None,
            graph_metadata_v3: None,
            tree_v1,
            diff_v1,
        }))
    }
    pub fn key(&self) -> &[u8; 32] {
        &self.key
    }
    pub fn origin(&self) -> &Arc<OriginalLocalActorOriginV1> {
        &self.origin
    }
    pub fn label(&self) -> &'static str {
        if let Some(role) = self.route.role {
            return if role == 0 {
                "Repository • Author"
            } else {
                "Repository • Committer"
            };
        }
        if let Some(source) = &self.route.provenance {
            return match source.view {
                2 => "Repository • Blame",
                3 => "Repository • Line source",
                5 => "Repository • Source diff",
                6 => "Repository • Original file",
                _ => "Repository • Saved file",
            };
        }
        if self.route.preview.is_some() {
            return "Repository • Saved file";
        }
        match self.route.mode {
            0 => "Repository • Overview",
            1 => "Repository • History",
            2 => "Repository • Tree",
            3 => "Repository • Working diff",
            4 => "Repository • Staged diff",
            5 => "Repository • Branch changes",
            7 => "Repository • All tracked edits",
            _ => "Repository • Commit changes",
        }
    }
    pub fn same_original_source(&self, workspace: &Workspace) -> bool {
        workspace
            .actor_origin
            .as_ref()
            .is_some_and(|origin| Arc::ptr_eq(origin, &self.origin))
    }
    /// Worker-only fresh exact read; no old host report/private model is stored.
    pub fn restore(&self, operation: &LocalGitNavigationReadV1) -> Result<Workspace, String> {
        self.origin
            .with_saved_navigation_read_v1(operation, |repository| {
                let mut workspace = Workspace::restore_original_route_v1(
                    repository,
                    self.origin.clone(),
                    &self.route,
                    operation.cancellation().clone(),
                )?;
                if let Some(tree) = &self.tree_v1 {
                    // The folder strip goes with the restored tree, as when it was first
                    // opened: a deep path stays one compact `[Folders · N]` row on return.
                    let projection = tree.restore(operation)?;
                    let strip = crate::repository_tree_viewport_v2::TreeBreadcrumbStripV2::prepare(
                        projection.clone(),
                        operation,
                    )?;
                    projection.install(&mut workspace)?;
                    workspace.tree_breadcrumbs_v2 = Some(strip);
                }
                if let Some(diff) = &self.diff_v1 {
                    diff.restore(self.origin.clone(), operation, &mut workspace)?;
                }
                Ok(workspace)
            })
    }
}

#[derive(Default)]
pub struct OriginalSavedVisitsV1 {
    entries: VecDeque<Arc<OriginalSavedVisitV1>>,
    graph_retention_v2: Option<crate::repository_graph_v1::restore_v2::GraphRetentionV2>,
}
impl OriginalSavedVisitsV1 {
    pub fn remember(&mut self, visit: Arc<OriginalSavedVisitV1>) {
        let _ = self.remember_checked_v2(visit);
    }
    pub fn find(&self, key: &[u8], directory: &Path) -> Option<Arc<OriginalSavedVisitV1>> {
        self.entries
            .iter()
            .find(|visit| visit.key.as_slice() == key && visit.origin.source().root() == directory)
            .cloned()
    }
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}
pub(crate) fn view_number(view: ProvenanceView) -> u8 {
    match view {
        // 1 (file History) and 4 (Graph) were retired when History became one view (TASK-99).
        ProvenanceView::File => 0,
        ProvenanceView::Blame => 2,
        ProvenanceView::Detail => 3,
        ProvenanceView::Diff => 5,
        ProvenanceView::OriginalFile => 6,
    }
}
pub(crate) fn mode_number(mode: RepositoryMode) -> u8 {
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
pub(crate) fn decode_mode(number: u8) -> Result<RepositoryMode, String> {
    Ok(match number {
        0 => RepositoryMode::Overview,
        1 => RepositoryMode::Graph,
        2 => RepositoryMode::Tree,
        3 => RepositoryMode::Working,
        4 => RepositoryMode::Staged,
        5 => RepositoryMode::Branch,
        6 => RepositoryMode::CommitChanges,
        7 => RepositoryMode::Uncommitted,
        _ => return Err("Original local mode unavailable".into()),
    })
}
fn sha(value: &str) -> Result<(), String> {
    if crate::git::original_actor_saved_sha(value) {
        Ok(())
    } else {
        Err("Original route needs full canonical40/64 saved/object SHA".into())
    }
}
fn charge(left: &mut usize, bytes: &[u8], cap: usize) -> Result<(), String> {
    if bytes.is_empty() || bytes.len() > cap || bytes.contains(&0) {
        return Err("Original raw route field exceeds its bound".into());
    }
    *left = left
        .checked_sub(bytes.len() + 16)
        .ok_or("Original route metadata exceeds16KiB")?;
    Ok(())
}
fn validate_selected_member(
    state: &crate::workspace::FileProvenance,
    selection: &FileSelection,
) -> Result<(), String> {
    match state.return_view {
        ProvenanceView::Blame => {
            if state.blame.len() > 1000
                || !state.blame.iter().any(|record| {
                    !record.ignored
                        && !record.unblamable
                        && record.line.commit == selection.commit
                        && Some(&record.line.path) == selection.path.as_ref()
                        && Some(record.line.original_line) == selection.original_line
                        && Some(record.line.final_line) == selection.final_line
                        && record.boundary == selection.boundary
                        && selection.exists
                })
            {
                return Err("Selected line is not an original exact blame member".into());
            }
        }
        _ => return Err("Original selected provenance return-source unavailable".into()),
    }
    Ok(())
}
struct RouteHash {
    hash: Sha256,
    left: usize,
}
impl Write for RouteHash {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.left = self
            .left
            .checked_sub(bytes.len())
            .ok_or_else(|| io::Error::other("bounded original route"))?;
        self.hash.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
#[path = "navigation_local_source_v1_tests.rs"]
mod tests;
