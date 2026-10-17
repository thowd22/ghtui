//! Workflow-only wrapper around SAME mechanical actual-result fence.
//! Public(crate) W6 interface preserved; never serialized or a Run/source permit.
use super::owned_history_fence_v8::{OwnedHistoryFenceV8, OwnedHistoryKindV8};
use super::*;
use std::sync::Arc;
pub(crate) struct ActionsOwnedWorkflowHistoryV6 {
    core: Arc<OwnedHistoryFenceV8>,
}
impl ActionsOwnedWorkflowHistoryV6 {
    pub(crate) fn reference(&self) -> ActionsPublicationReference {
        self.core.marker.publication_reference()
    }
    pub(crate) fn matches_pr_origin_v6(
        &self,
        target: &crate::pr_write_journal::PrWriteTarget,
        database: &Path,
        file: (u64, u64),
    ) -> bool {
        self.core.claim.matches_pr_origin_v4(target, database, file)
            && self.core.marker.context.original_workflow_v6.is_some()
            && self.core.marker.context.original_run_v3.is_none()
    }
    pub(crate) fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.core.retained_bytes()
    }
    pub(in crate::actions_write_journal) fn next_for_ack(
        &self,
        actual: &ActionsWriteMarker,
        next: ActionsWriteState,
    ) -> Result<Arc<Self>> {
        Ok(Arc::new(Self {
            core: self.core.next_for_ack(actual, next)?,
        }))
    }
}
impl ActionsWriteJournal {
    pub(crate) fn prepare_owned_workflow_history_v6(
        &self,
        claim: Arc<ActionsClaimStampV3>,
        state: ActionsWriteState,
        receipt: &ActionsReceipt,
    ) -> Result<Arc<ActionsOwnedWorkflowHistoryV6>> {
        Ok(Arc::new(ActionsOwnedWorkflowHistoryV6 {
            core: self.prepare_owned_history_fence_v8(
                claim,
                state,
                receipt,
                OwnedHistoryKindV8::Workflow,
            )?,
        }))
    }
    pub(crate) fn validate_owned_workflow_history_v6(
        &self,
        proof: &ActionsOwnedWorkflowHistoryV6,
    ) -> Result<()> {
        self.validate_owned_history_fence_v8(&proof.core, OwnedHistoryKindV8::Workflow)
    }
}
