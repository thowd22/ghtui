//! NEW undeclared child of restore_v2. Bounds-only sharing of the EXISTING
//! ledger; neither a new budget nor Git/source/access authority.
use super::*;

/// A genuinely allocated bounded route, not another page/source. Every live
/// distinct visit pays its own actual capacities until the last Arc drops.
pub(crate) struct GraphVisitMetadataV3 {
    ledger: Arc<Mutex<GraphLedgerV2>>,
    bytes: usize,
}
impl GraphLeaseV2 {
    pub(crate) fn retain_visit_metadata_v3(
        &self,
        bytes: usize,
    ) -> Result<Arc<GraphVisitMetadataV3>, String> {
        let bytes = bytes
            .checked_add(size_of::<GraphVisitMetadataV3>() + 32)
            .filter(|bytes| *bytes <= 32 * 1024)
            .ok_or("Original Graph visit metadata exceeds its bounded allocation")?;
        let mut ledger = self
            .ledger
            .lock()
            .map_err(|_| "Graph retention lock unavailable")?;
        let next = ledger
            .bytes
            .checked_add(bytes)
            .filter(|next| *next <= GRAPH_VISIT_BYTES_V2)
            .ok_or("Graph visit metadata exceeds unchanged16MiB retention")?;
        ledger.bytes = next;
        Ok(Arc::new(GraphVisitMetadataV3 {
            ledger: self.ledger.clone(),
            bytes,
        }))
    }
}
impl Drop for GraphVisitMetadataV3 {
    fn drop(&mut self) {
        let mut ledger = self
            .ledger
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        ledger.bytes = ledger.bytes.saturating_sub(self.bytes);
    }
}

impl GraphLeaseV2 {
    pub(crate) fn retention_v3(&self) -> GraphRetentionV2 {
        GraphRetentionV2 {
            ledger: self.ledger.clone(),
        }
    }
}

impl GraphRetentionV2 {
    pub(crate) fn can_add_source_v3(&self) -> Result<(), String> {
        let accounting = self.accounting()?;
        if accounting.sources >= GRAPH_VISITS_V2 {
            return Err(
                "Graph Back limit64 sources • discard retained Graph Back before re-reading".into(),
            );
        }
        // Do not reserve a fictional page/model size. The worker charges the
        // actual freshly minted component capacities before final closure.
        if accounting.bytes >= GRAPH_VISIT_BYTES_V2 {
            return Err(
                "Graph Back limit16MiB • discard retained Graph Back before re-reading".into(),
            );
        }
        Ok(())
    }

    pub(crate) fn owns_lease_v3(&self, lease: &GraphLeaseV2) -> bool {
        Arc::ptr_eq(&self.ledger, &lease.ledger)
    }
}
