//! NEW undeclared Workspace child. Fresh original status/object checks before
//! pure admission. No old status/Workspace/actor/host report is replayed.
use super::*;
use crate::{
    navigation_local_source_v1::OriginalFileRouteV1,
    providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
    repository_graph_v1::{
        projection_v2::GraphWindowActionV2,
        restore_v2::{GraphPositionV2, GraphSnapshotV2},
        GraphPresentationV1, GraphViewV1,
    },
};

impl Workspace {
    pub(crate) fn graph_retention_visible_v2(&self) -> bool {
        if self.actor_profile.is_some() || self.file_provenance.is_some() {
            return false;
        }
        let Some(state) = self.graph_state_v1.as_ref() else {
            return false;
        };
        if self
            .actor_origin
            .as_ref()
            .is_none_or(|origin| !Arc::ptr_eq(origin, state.origin()))
        {
            return false;
        }
        if self.graph_visible_v1() {
            return true;
        }
        if self.graph_view_v1 != GraphViewV1::Comparison
            || self.mode != RepositoryMode::CommitChanges
        {
            return false;
        }
        let Some(member) = state.selected_commit(&self.revision) else {
            return false;
        };
        self.comparison_v1()
            .is_some_and(|result| match &result.comparison {
                Comparison::CommitChanges { commit, parent } => {
                    commit == &self.revision
                        && result.head.as_deref() == Some(self.revision.as_str())
                        && member.parents.get(*parent).map(String::as_str) == result.base.as_deref()
                }
                _ => false,
            })
    }
    pub(crate) fn graph_position_v2(&self) -> Result<GraphPositionV2, String> {
        let state = self
            .graph_state_v1
            .as_ref()
            .ok_or("Original Graph source unavailable")?;
        let parent = self
            .comparison_v1()
            .and_then(|comparison| match comparison.comparison {
                Comparison::CommitChanges { parent, .. } => Some(parent),
                _ => None,
            });
        state.position_v2(
            &self.revision,
            self.graph_view_v1,
            self.graph_projection_start_v2,
            self.graph_horizontal_v1,
            parent,
        )
    }
    pub(crate) fn graph_window_action_v2(
        &mut self,
        action: GraphWindowActionV2,
    ) -> Result<(), String> {
        if !self.graph_visible_v1() {
            return Err("This page does not own visible Graph projection".into());
        }
        let state = self.graph_state_v1.as_ref().unwrap().clone();
        let pose = state.projection_position_v2(&self.graph_position_v2()?, action)?;
        let hidden = self
            .history_filter_v993
            .spec
            .hide
            .then(|| self.history_filter_v993.flags(&state))
            .flatten();
        let display = GraphPresentationV1::prepare_window_v993(
            &state,
            &pose.selected,
            pose.view,
            pose.projection_start,
            hidden,
        )?;
        self.revision = pose.selected.clone();
        self.selected_commit = state.selected_commit(&pose.selected).cloned();
        self.graph_projection_start_v2 = pose.projection_start;
        self.graph_view_v1 = pose.view;
        self.graph_display_v1 = Some(display);
        Ok(())
    }
    pub(crate) fn restore_graph_snapshot_v2(
        snapshot: &Arc<GraphSnapshotV2>,
        position: &GraphPositionV2,
        provenance: Option<&OriginalFileRouteV1>,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<Self, String> {
        snapshot.validate_position(position)?;
        operation.check()?;
        snapshot
            .origin()
            .with_saved_navigation_read_v1(operation, |repository| {
                let status = repository.status().map_err(|e| e.to_string())?;
                let restored = snapshot.restore(position, operation)?;
                // ONLY current status + actual shared graph and bounded selected
                // commit. Root composes additive Tree field defaults as needed.
                let selected = restored
                    .state
                    .selected_commit(&position.selected)
                    .cloned()
                    .ok_or("Original restored member unavailable")?;
                // History never carries a file source (TASK-99); a route with one is stale.
                if provenance.is_some() {
                    return Err("Original History route carries a retired file source".into());
                }
                let mut workspace = Self {
                    directory: repository.root.clone(),
                    revision: position.selected.clone(),
                    mode: RepositoryMode::Graph,
                    status,
                    tree: None,
                    comparison: None,
                    preview: None,
                    file_provenance: None,
                    selected_commit: Some(selected),
                    actor_origin: Some(snapshot.origin().clone()),
                    actor_profile: None,
                    rows: Vec::new(),
                    graph_state_v1: Some(restored.state),
                    graph_display_v1: Some(restored.presentation),
                    graph_view_v1: position.view,
                    graph_horizontal_v1: position.horizontal,
                    graph_projection_start_v2: position.projection_start,
                    tree_navigation_v1: None,
                    tree_breadcrumbs_v2: None,
                    original_comparison_v1: None,
                    original_diff_side_v1: None,
                    history_filter_v993: Default::default(),
                };
                if position.view == GraphViewV1::Comparison {
                    workspace.graph_action_v1(
                        crate::repository_graph_v1::GraphActionV1::CompareParent(
                            position
                                .comparison_parent
                                .ok_or("Original comparison parent unavailable")?,
                        ),
                        Some(repository),
                    )?;
                }
                repository
                    .validate_graph_epoch_v1(
                        workspace.graph_state_v1.as_ref().unwrap().last_page(),
                        operation.deadline(),
                    )
                    .map_err(|e| e.to_string())?;
                operation.check()?;
                Ok(workspace)
            })
    }
}
