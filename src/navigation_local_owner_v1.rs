//! Actual original source/token ownership, no remote access or restored proof.
#[path = "navigation_graph_reread_owner_v3.rs"]
mod graph_reread_owner_v3;
use crate::{
    git::OriginalLocalActorOriginV1,
    providers::{
        local_git_navigation_read_v1::LocalGitNavigationReadV1, runner::CancellationToken,
    },
    workspace::Workspace,
};
use std::{sync::Arc, time::Instant};

pub struct OriginalLocalNavigationOwnerV1 {
    origin: Arc<OriginalLocalActorOriginV1>,
    target_origin: Arc<OriginalLocalActorOriginV1>,
    revision: String,
    selected: Option<String>,
    graph: Option<Arc<crate::repository_graph_v1::OriginalGraphStateV1>>,
    graph_display: Option<Arc<crate::repository_graph_v1::GraphPresentationV1>>,
    graph_view: crate::repository_graph_v1::GraphViewV1,
    graph_horizontal: usize,
    graph_projection_start: usize,
    graph_comparison_parent_v3: Option<usize>,
    tree_projection: Option<Arc<crate::workspace::PreparedOriginalTreeNavigationV1>>,
    comparison_source: Option<Arc<crate::git::OriginalComparisonSourceV1>>,
    comparison_side: Option<Arc<crate::workspace::PreparedOriginalDiffSideV1>>,
    operation: Arc<LocalGitNavigationReadV1>,
}
impl std::fmt::Debug for OriginalLocalNavigationOwnerV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OriginalLocalNavigationOwnerV1(<same source, original queue clock/token>)")
    }
}
impl OriginalLocalNavigationOwnerV1 {
    /// Called BEFORE queueing with Background's actual newly created token.
    /// No Workspace/model/body clone or filesystem/provider/SQL work.
    pub fn begin(
        workspace: &Workspace,
        actual_request_token: CancellationToken,
    ) -> Result<Arc<Self>, String> {
        Self::begin_target(workspace, None, actual_request_token)
    }
    fn begin_target(
        workspace: &Workspace,
        target: Option<&Arc<OriginalLocalActorOriginV1>>,
        actual_request_token: CancellationToken,
    ) -> Result<Arc<Self>, String> {
        let deadline = Instant::now()
            .checked_add(std::time::Duration::from_secs(30))
            .ok_or("Original local deadline unavailable")?;
        let origin = workspace
            .actor_origin
            .as_ref()
            .ok_or("Original local source unavailable; no guessed directory read")?;
        if workspace.directory != origin.source().root()
            || !(crate::git::original_actor_saved_sha(&workspace.revision)
                || workspace.revision == "HEAD" && workspace.status.head.is_none())
            || workspace
                .selected_commit
                .as_ref()
                .is_some_and(|commit| !crate::git::original_actor_saved_sha(&commit.id))
        {
            return Err("Original local request lacks exact loaded source/version".into());
        }
        let target_origin = target.unwrap_or(origin);
        let operation = Arc::new(LocalGitNavigationReadV1::begin_under_deadline(
            target_origin,
            actual_request_token,
            deadline,
        )?);
        Ok(Arc::new(Self {
            origin: origin.clone(),
            target_origin: target_origin.clone(),
            revision: workspace.revision.clone(),
            graph: workspace.graph_state_v1.clone(),
            graph_display: workspace.graph_display_v1.clone(),
            graph_view: workspace.graph_view_v1,
            graph_horizontal: workspace.graph_horizontal_v1,
            graph_projection_start: workspace.graph_projection_start_v2,
            graph_comparison_parent_v3: workspace
                .graph_position_v2()
                .ok()
                .and_then(|position| position.comparison_parent),
            tree_projection: workspace.tree_navigation_v1.clone(),
            comparison_source: workspace.original_comparison_v1.clone(),
            comparison_side: workspace.original_diff_side_v1.clone(),
            selected: workspace
                .selected_commit
                .as_ref()
                .map(|commit| commit.id.clone()),
            operation,
        }))
    }
    pub fn begin_visit(
        workspace: &Workspace,
        visit: &crate::navigation_local_source_v1::OriginalSavedVisitV1,
        actual_request_token: CancellationToken,
    ) -> Result<Arc<Self>, String> {
        Self::begin_target(workspace, Some(visit.origin()), actual_request_token)
    }
    pub fn operation(&self) -> &Arc<LocalGitNavigationReadV1> {
        &self.operation
    }
    pub fn matches_original(&self, workspace: &Workspace) -> bool {
        !self.operation.cancellation().is_cancelled()
            && workspace
                .actor_origin
                .as_ref()
                .is_some_and(|origin| Arc::ptr_eq(origin, &self.origin))
            && match (&self.graph, &workspace.graph_state_v1) {
                (None, None) => true,
                (Some(original), Some(current)) => Arc::ptr_eq(original, current),
                _ => false,
            }
            && match (&self.graph_display, &workspace.graph_display_v1) {
                (None, None) => true,
                (Some(original), Some(current)) => Arc::ptr_eq(original, current),
                _ => false,
            }
            && workspace.graph_view_v1 == self.graph_view
            && workspace.graph_horizontal_v1 == self.graph_horizontal
            && workspace.graph_projection_start_v2 == self.graph_projection_start
            && workspace
                .graph_position_v2()
                .ok()
                .and_then(|position| position.comparison_parent)
                == self.graph_comparison_parent_v3
            && match (&self.tree_projection, &workspace.tree_navigation_v1) {
                (None, None) => true,
                (Some(original), Some(current)) => Arc::ptr_eq(original, current),
                _ => false,
            }
            && workspace.revision == self.revision
            && match (&self.comparison_source, &workspace.original_comparison_v1) {
                (None, None) => true,
                (Some(original), Some(current)) => Arc::ptr_eq(original, current),
                _ => false,
            }
            && match (&self.comparison_side, &workspace.original_diff_side_v1) {
                (None, None) => true,
                (Some(original), Some(current)) => Arc::ptr_eq(original, current),
                _ => false,
            }
            && workspace
                .selected_commit
                .as_ref()
                .map(|commit| commit.id.as_str())
                == self.selected.as_deref()
    }
    /// Deadline failure may be an honest owned diagnostic, never a success or
    /// new model. Cancel/source/selection loss rejects BOTH success and error.
    pub fn can_admit(&self, current: &Workspace, success: bool) -> bool {
        self.matches_original(current) && (!success || Instant::now() < self.operation.deadline())
    }
    pub fn admits_workspace(&self, result: &Workspace) -> bool {
        !self.operation.cancellation().is_cancelled()
            && Instant::now() < self.operation.deadline()
            && result
                .actor_origin
                .as_ref()
                .is_some_and(|origin| Arc::ptr_eq(origin, &self.target_origin))
            && self
                .operation
                .accounting()
                .is_ok_and(|accounting| accounting.sealed() && accounting.in_flight_requests() == 0)
    }
}

#[cfg(test)]
#[path = "navigation_graph_clock_v2_tests.rs"]
mod graph_clock_v2_tests;

#[cfg(test)]
#[path = "navigation_local_owner_v1_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "navigation_graph_owner_v1_tests.rs"]
mod graph_owner_tests;
