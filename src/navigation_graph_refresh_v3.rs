//! NEW undeclared child of navigation_local_source_v1. Tiny intent/lease
//! plumbing only; no cached bodies, new registry, read clocks or permission.
use super::*;
use crate::repository_graph_v1::restore_v2::GraphLeaseV2;

impl OriginalSavedVisitV1 {
    pub(crate) fn same_graph_file_intent_v3(&self, workspace: &Workspace) -> bool {
        match (&self.route.provenance, &workspace.file_provenance) {
            (None, None) => true,
            (Some(route), Some(actual)) => {
                let source = if actual.selection.is_some() {
                    actual.return_view
                } else {
                    actual.view
                };
                route.revision == actual.revision
                    && route.path == actual.path
                    && route.view == view_number(actual.view)
                    && route.return_view == view_number(source)
                    && route.history_offset
                        == if source == ProvenanceView::History {
                            actual.history.as_ref().map(|page| page.offset)
                        } else {
                            None
                        }
                    && route.blame_start
                        == if source == ProvenanceView::Blame {
                            actual.blame_start
                        } else {
                            1
                        }
                    && match (&route.selection, &actual.selection) {
                        (None, None) => true,
                        (Some(original), Some(current)) => original.matches(current),
                        _ => false,
                    }
            }
            _ => false,
        }
    }
    fn actual_graph_metadata_bytes_v3(&self) -> Result<usize, String> {
        let route = &self.route;
        let mut bytes = std::mem::size_of::<Self>() + 32 + route.revision.capacity();
        bytes += route.selected.as_ref().map_or(0, String::capacity);
        if let Some((revision, path, object)) = &route.preview {
            bytes += revision.capacity() + path.0.capacity() + object.capacity();
        }
        if let Some(graph) = &route.graph {
            bytes += graph.walk_tip.capacity() + graph.selected.capacity();
        }
        if let Some(file) = &route.provenance {
            bytes += file.revision.capacity() + file.path.0.capacity();
            if let Some(selection) = &file.selection {
                bytes += selection.commit.capacity()
                    + selection.path.as_ref().map_or(0, |path| path.0.capacity());
            }
        }
        if let Some(comparison) = &route.comparison {
            bytes +=
                comparison.base.as_ref().map_or(0, String::capacity) + comparison.head.capacity();
        }
        bytes += route.expanded.capacity() * std::mem::size_of::<RawPath>();
        bytes += route
            .expanded
            .iter()
            .map(|path| path.0.capacity())
            .sum::<usize>();
        if self.tree_v1.is_some() || bytes > MAX_ROUTE_BYTES + std::mem::size_of::<Self>() + 32 {
            return Err(
                "Graph visit contains another tree or excessive actual route capacities".into(),
            );
        }
        Ok(bytes)
    }
    pub(crate) fn graph_file_intent_v3(&self) -> Option<&OriginalFileRouteV1> {
        self.route.provenance.as_ref()
    }

    pub(crate) fn from_graph_lease_v3(
        workspace: &Workspace,
        lease: Arc<GraphLeaseV2>,
    ) -> Result<Arc<Self>, String> {
        if !workspace.graph_retention_visible_v2()
            || !workspace
                .graph_state_v1
                .as_ref()
                .is_some_and(|state| lease.snapshot().same_source(&state.snapshot_v2()))
        {
            return Err("Fresh retained visit does not own this exact Graph source".into());
        }
        let mut visit = Arc::try_unwrap(Self::capture(workspace)?)
            .map_err(|_| "New Graph metadata unexpectedly has another owner")?;
        visit.key = lease.snapshot().source_key();
        visit.graph_metadata_v3 =
            Some(lease.retain_visit_metadata_v3(visit.actual_graph_metadata_bytes_v3()?)?);
        visit.graph_lease_v2 = Some(lease);
        Ok(Arc::new(visit))
    }
}

impl OriginalSavedVisitsV1 {
    /// BEFORE any reread child: no silent eviction of even a non-Graph visit.
    pub(crate) fn can_remember_new_graph_v3(&self) -> Result<(), String> {
        if self.entries.len() >= MAX_VISITS {
            return Err(
                "Back has64 original visits • explicitly release Graph Back before re-reading"
                    .into(),
            );
        }
        if let Some(budget) = &self.graph_retention_v2 {
            budget.can_add_source_v3()?;
        }
        Ok(())
    }

    /// Invoked only AFTER the existing exact original request-owner/generation
    /// gates, before any prepared Workspace/status/focus publication.
    pub(crate) fn accept_graph_reread_v3(
        &mut self,
        workspace: &Workspace,
        visit: Arc<OriginalSavedVisitV1>,
    ) -> Result<(), String> {
        let budget = self
            .graph_retention_v2
            .as_ref()
            .ok_or("Original Graph retention was discarded during this request")?;
        let lease = visit
            .graph_lease_v2()
            .ok_or("Reread has no actual worker-admitted Graph lease")?;
        if !budget.owns_lease_v3(lease)
            || !workspace.graph_retention_visible_v2()
            || !workspace
                .graph_state_v1
                .as_ref()
                .is_some_and(|state| lease.snapshot().same_source(&state.snapshot_v2()))
            || *visit.key() != lease.snapshot().source_key()
            || !Arc::ptr_eq(visit.origin(), lease.snapshot().origin())
        {
            return Err("Reread result belongs to another original Graph registry/source".into());
        }
        if self.entries.len() >= MAX_VISITS
            && !self.entries.iter().any(|old| old.key() == visit.key())
        {
            return Err(
                "Back reached64 original visits before reread publication • old view retained"
                    .into(),
            );
        }
        self.remember_checked_v2(visit)
    }
}
