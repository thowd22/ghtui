//! NEW undeclared worker capsule. A snapshot/selection is navigation intent;
//! fresh source/object checks and the already prequeued operation stay required.
use crate::{
    navigation_local_source_v1::{OriginalSavedVisitV1, OriginalSavedVisitsV1},
    providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
    repository_graph_v1::{
        restore_v2::{GraphPositionLeaseV2, GraphRetentionV2},
        GraphPresentationV1, OriginalGraphStateV1,
    },
    workspace::Workspace,
};
use std::sync::{Arc, Weak};

pub struct GraphRereadPreviewV3 {
    original: Arc<OriginalSavedVisitV1>,
    position: Arc<GraphPositionLeaseV2>,
    // Weak control-block ownership prevents pointer-reuse equivalence while
    // avoiding a second retained topology/prepared body outside the ledger.
    state: Weak<OriginalGraphStateV1>,
    display: Weak<GraphPresentationV1>,
    retention: GraphRetentionV2,
    _metadata: Arc<crate::repository_graph_v1::restore_v2::GraphVisitMetadataV3>,
    lines: Vec<String>,
}
impl std::fmt::Debug for GraphRereadPreviewV3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GraphRereadPreviewV3(<bounded original snapshot intent; no authority>)")
    }
}
impl GraphRereadPreviewV3 {
    pub(crate) fn capture(
        workspace: &Workspace,
        registry: &mut OriginalSavedVisitsV1,
    ) -> Result<Arc<Self>, String> {
        registry.can_remember_new_graph_v3()?;
        if !workspace.graph_retention_visible_v2() {
            return Err("Open a genuine original Graph snapshot first".into());
        }
        let original = registry.capture_graph_v2(workspace)?;
        registry.remember_checked_v2(original.clone())?;
        // Same actual source/ledger. Neither new registry nor page-sized
        // guessed reservation is used to make this preview appear affordable.
        let position = original.graph_request_position_v2(&workspace.graph_position_v2()?)?;
        let retention = position.source().retention_v3();
        retention.can_add_source_v3()?;
        let state = workspace.graph_state_v1.as_ref().unwrap().clone();
        let display = workspace
            .graph_display_v1
            .as_ref()
            .ok_or("Original Graph prepared projection unavailable")?
            .clone();
        let lines: Vec<String> = vec![
            "Re-read the ORIGINAL saved walk, not the current checkout HEAD.".into(),
            format!("Original walk tip: {}", state.walk_tip()),
            format!("Currently loaded: {} saved versions", state.loaded()),
            format!(
                "Back retains selected member: {}",
                position.position().selected
            ),
            "This resets the new view to up to200 commits and the original tip.".into(),
            "Earlier loaded pages and your exact position remain in Graph Back.".into(),
            "More is always another explicit original-source read; no automatic walk.".into(),
            "Fresh file/object/shallow checks, SAME producer and one prequeue deadline.".into(),
            "No checkout, fetch, Git mutation, GitHub request or cache write.".into(),
            "Observed ref names remain historical labels, not current branch grants.".into(),
        ];
        let metadata_bytes = std::mem::size_of::<Self>()
            + 32
            + lines.capacity() * std::mem::size_of::<String>()
            + lines.iter().map(String::capacity).sum::<usize>();
        let metadata = position.source().retain_visit_metadata_v3(metadata_bytes)?;
        Ok(Arc::new(Self {
            original,
            position,
            state: Arc::downgrade(&state),
            display: Arc::downgrade(&display),
            retention,
            _metadata: metadata,
            lines,
        }))
    }
    pub fn lines(&self) -> &[String] {
        &self.lines
    }
    pub(crate) fn matches_workspace(&self, workspace: &Workspace) -> bool {
        workspace.graph_retention_visible_v2()
            && workspace
                .graph_state_v1
                .as_ref()
                .is_some_and(|state| Weak::ptr_eq(&Arc::downgrade(state), &self.state))
            && workspace
                .graph_display_v1
                .as_ref()
                .is_some_and(|display| Weak::ptr_eq(&Arc::downgrade(display), &self.display))
            && workspace
                .graph_position_v2()
                .is_ok_and(|position| position == *self.position.position())
            && workspace
                .actor_origin
                .as_ref()
                .is_some_and(|origin| Arc::ptr_eq(origin, self.original.origin()))
    }
    pub(crate) fn execute(
        &self,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<GraphRereadOutcomeV3, String> {
        operation.check()?;
        self.retention.can_add_source_v3()?;
        let workspace = Workspace::reread_original_graph_v3(
            self.position.source().snapshot(),
            self.position.position(),
            &self.original,
            operation,
        )?;
        let state = workspace
            .graph_state_v1
            .as_ref()
            .ok_or("Fresh original Graph source missing")?;
        if state
            .snapshot_v2()
            .same_source(self.position.source().snapshot())
        {
            return Err(
                "A deliberate reread must produce a genuinely new first-page source".into(),
            );
        }
        // Real immutable capacities admitted to the SAME ledger, before final
        // epoch/token/seal. All failure/cancel/foreign outcomes drop this lease.
        let lease = self.retention.admit(state.snapshot_v2())?;
        let visit = OriginalSavedVisitV1::from_graph_lease_v3(&workspace, lease)?;
        state
            .origin()
            .with_saved_navigation_read_v1(operation, |repository| {
                repository
                    .validate_graph_epoch_v1(state.last_page(), operation.deadline())
                    .map_err(|error| error.to_string())?;
                operation.check()
            })?;
        Ok(GraphRereadOutcomeV3 { workspace, visit })
    }

    pub(crate) fn validate_owner_v3(
        &self,
        owner: &crate::navigation_local_owner_v1::OriginalLocalNavigationOwnerV1,
    ) -> Result<(), String> {
        if !owner.matches_graph_original_v3(
            self.original.origin(),
            &self.state,
            &self.display,
            self.position.position(),
        ) {
            return Err(
                "Original reread capsule does not own this exact prequeued Graph source/position"
                    .into(),
            );
        }
        Ok(())
    }
}

pub(crate) struct GraphRereadOutcomeV3 {
    pub(crate) workspace: Workspace,
    pub(crate) visit: Arc<OriginalSavedVisitV1>,
}

#[cfg(test)]
#[path = "repository_graph_reread_v3_tests.rs"]
mod tests;
