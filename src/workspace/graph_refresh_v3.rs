//! NEW undeclared Workspace child. SAME already captured original operation;
//! no begin/HEAD/full-walk refresh, report restoration or pool extension here.
use super::*;
use crate::{
    navigation_local_source_v1::OriginalSavedVisitV1,
    providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
    repository_graph_v1::{
        restore_v2::{GraphPositionV2, GraphSnapshotV2},
        GraphPresentationV1, GraphViewV1,
    },
};

impl Workspace {
    pub(crate) fn refresh_original_graph_v3(
        &self,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<Self, String> {
        if !self.graph_retention_visible_v2() {
            return Err("This view has no genuine original Graph snapshot".into());
        }
        // Captures at most16KiB original-file intent, not cached History/Blame
        // bodies. The shared patched V2 factory freshly verifies that member.
        let visit = OriginalSavedVisitV1::capture(self)?;
        let state = self.graph_state_v1.as_ref().unwrap();
        Self::restore_graph_snapshot_v2(
            &state.snapshot_v2(),
            &self.graph_position_v2()?,
            visit.graph_file_intent_v3(),
            operation,
        )
    }

    pub(crate) fn reread_original_graph_v3(
        snapshot: &Arc<GraphSnapshotV2>,
        position: &GraphPositionV2,
        visit: &OriginalSavedVisitV1,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<Self, String> {
        operation.check()?;
        if !Arc::ptr_eq(snapshot.origin(), visit.origin())
            || visit
                .graph_lease_v2()
                .is_none_or(|lease| !lease.snapshot().same_source(snapshot))
        {
            return Err("Original reread lost its retained source/visit".into());
        }
        // This bounded factory also handles genuine responsible-file member
        // verification and exact parent comparison; never resurrect old pages
        // or drop the History/Blame return source by copying old presentation.
        let mut fresh = Self::restore_graph_snapshot_v2(
            snapshot,
            position,
            visit.graph_file_intent_v3(),
            operation,
        )?;
        snapshot
            .origin()
            .with_saved_navigation_read_v1(operation, |repository| {
                let state = super::graph_v1::read_graph_through_v991(
                    repository,
                    snapshot.origin(),
                    snapshot.walk_tip(),
                    operation.deadline(),
                )?;
                let selected = state
                    .selected_commit(snapshot.walk_tip())
                    .ok_or("Fresh walk omitted the original walk tip")?
                    .clone();
                let start = state
                    .loaded_member_v2(&selected.id)
                    .ok_or("Fresh walk member missing")?
                    .node as usize
                    / 200
                    * 200;
                let display = GraphPresentationV1::prepare_window_v2(
                    &state,
                    snapshot.walk_tip(),
                    GraphViewV1::Timeline,
                    start,
                )?;
                fresh.revision = snapshot.walk_tip().into();
                fresh.selected_commit = Some(selected);
                fresh.mode = RepositoryMode::Graph;
                fresh.graph_state_v1 = Some(state);
                fresh.graph_display_v1 = Some(display);
                fresh.graph_view_v1 = GraphViewV1::Timeline;
                fresh.graph_projection_start_v2 = start;
                fresh.graph_horizontal_v1 = 0;
                fresh.comparison = None;
                // First-page reread is an explicit ORIGINAL WALK reset, not the
                // selected responsible file. That intent remains in actual Back.
                fresh.file_provenance = None;
                operation.check()?;
                Ok(fresh)
            })
    }
}
