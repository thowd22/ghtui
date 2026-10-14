//! NEW undeclared child of navigation_local_source_v1. Position is navigation
//! metadata; only actual state-minted pages/lease can restore original Graph.
use super::*;
use crate::repository_graph_v1::restore_v2::{
    GraphLeaseV2, GraphPositionLeaseV2, GraphPositionV2, GraphRetentionV2,
};

impl OriginalSavedVisitsV1 {
    pub(crate) fn graph_retention_accounting_v2(
        &self,
    ) -> Result<Option<crate::repository_graph_v1::restore_v2::GraphRetentionAccountingV2>, String>
    {
        self.graph_retention_v2
            .as_ref()
            .map(GraphRetentionV2::accounting)
            .transpose()
    }
    /// Pure bounded admission. The worker already observed every body capacity;
    /// this scans at most300 tiny actual component records, never10k commits.
    pub(crate) fn capture_graph_v2(
        &mut self,
        workspace: &Workspace,
    ) -> Result<Arc<OriginalSavedVisitV1>, String> {
        if !workspace.graph_retention_visible_v2() {
            return Err("No genuine visible original Graph source".into());
        }
        if let Some(visit) = self
            .graph_visit_v2(workspace)
            .filter(|visit| visit.same_graph_file_intent_v3(workspace))
        {
            return Ok(visit);
        }
        let state = workspace
            .graph_state_v1
            .as_ref()
            .ok_or("Original Graph state unavailable")?;
        let budget = self
            .graph_retention_v2
            .get_or_insert_with(GraphRetentionV2::new);
        let lease = budget.admit(state.snapshot_v2())?;
        OriginalSavedVisitV1::from_graph_lease_v3(workspace, lease)
    }
    pub(crate) fn remember_checked_v2(
        &mut self,
        visit: Arc<OriginalSavedVisitV1>,
    ) -> Result<(), String> {
        if let Some(index) = self.entries.iter().position(|old| old.key == visit.key) {
            self.entries.remove(index);
            self.entries.push_back(visit);
            return Ok(());
        }
        if self.entries.len() == MAX_VISITS {
            if self
                .entries
                .front()
                .is_some_and(|old| old.graph_lease_v2.is_some())
            {
                return Err("Graph Back has64 retained sources • explicitly Discard graph Back before leaving".into());
            }
            // Preserve existing non-Graph history policy; never silently evict
            // actual retained Graph pages because another mode was admitted.
            self.entries.pop_front();
        }
        self.entries.push_back(visit);
        Ok(())
    }
    /// ONLY Graph retention; ordinary local visits, sources, drafts and cache
    /// are unchanged. Pending worker lease remains charged until actual Drop.
    pub(crate) fn discard_graph_back_v2(&mut self) -> usize {
        let before = self.entries.len();
        self.entries.retain(|visit| visit.graph_lease_v2.is_none());
        before - self.entries.len()
    }
    pub(crate) fn graph_visit_v2(
        &self,
        workspace: &Workspace,
    ) -> Option<Arc<OriginalSavedVisitV1>> {
        let snapshot = workspace.graph_state_v1.as_ref()?.snapshot_v2();
        self.entries
            .iter()
            .find(|visit| {
                visit
                    .graph_lease_v2
                    .as_ref()
                    .is_some_and(|lease| lease.snapshot().same_source(&snapshot))
            })
            .cloned()
    }
}
impl OriginalSavedVisitV1 {
    pub(crate) fn graph_lease_v2(&self) -> Option<&Arc<GraphLeaseV2>> {
        self.graph_lease_v2.as_ref()
    }
    pub(crate) fn graph_request_position_v2(
        &self,
        position: &GraphPositionV2,
    ) -> Result<Arc<GraphPositionLeaseV2>, String> {
        self.graph_lease_v2
            .as_ref()
            .ok_or("Graph Back snapshot unavailable • explicitly reload, no fallback")?
            .position(position)
    }
    pub(crate) fn restore_graph_position_v2(
        &self,
        request: &GraphPositionLeaseV2,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<Workspace, String> {
        let original = self
            .graph_lease_v2
            .as_ref()
            .ok_or("Original Graph snapshot was discarded")?;
        if !Arc::ptr_eq(original, request.source()) {
            return Err("Graph pending request does not own this exact retained source".into());
        }
        let mut workspace = Workspace::restore_graph_snapshot_v2(
            original.snapshot(),
            request.position(),
            self.route.provenance.as_ref(),
            operation,
        )?;
        if let Some(diff) = &self.diff_v1 {
            diff.restore(self.origin.clone(), operation, &mut workspace)?;
        }
        Ok(workspace)
    }
}

#[cfg(test)]
#[path = "navigation_graph_restore_v2_tests.rs"]
mod tests;
