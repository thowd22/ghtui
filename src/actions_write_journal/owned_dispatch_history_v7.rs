//! Dispatch-only wrapper around SAME mechanical actual-result fence.
//! Never serialized, a Run/Workflow proof, source or publication permit.
use super::owned_history_fence_v8::{OwnedHistoryFenceV8, OwnedHistoryKindV8};
use super::*;
use std::sync::Arc;
pub(crate) struct ActionsOwnedDispatchHistoryV7 {
    core: Arc<OwnedHistoryFenceV8>,
}
impl ActionsOwnedDispatchHistoryV7 {
    pub(crate) fn matches_returned_run_v7(
        &self,
        run: &crate::providers::gh::ScopedActionsRunTargetV1,
    ) -> bool {
        let marker = &self.core.marker;
        let Some(receipt) = marker
            .receipt
            .as_ref()
            .and_then(|receipt| receipt.original_dispatch_run_v7.as_ref())
        else {
            return false;
        };
        let Some(dispatch) = marker.context.original_dispatch_v7.as_ref() else {
            return false;
        };
        receipt.repository_id == run.repository_id()
            && Some(receipt.repository_node.as_str()) == run.repository_node()
            && receipt.run_id == run.run().id
            && Some(receipt.run_node.as_str()) == run.node_v2()
            && receipt.workflow_id == run.run().workflow_id
            && receipt.attempt == run.run().attempt
            && receipt.head_sha == run.run().head_sha
            && receipt.head_sha == dispatch.commit
            && receipt.event == run.run().event
            && Some(receipt.check_suite_id) == run.suite_v3()
    }
    pub(crate) fn reference(&self) -> ActionsPublicationReference {
        self.core.marker.publication_reference()
    }
    pub(crate) fn matches_pr_origin_v7(
        &self,
        target: &crate::pr_write_journal::PrWriteTarget,
        database: &Path,
        file: (u64, u64),
    ) -> bool {
        self.core.claim.matches_pr_origin_v4(target, database, file)
            && self.core.marker.context.original_workflow_v6.is_some()
            && self.core.marker.context.original_dispatch_v7.is_some()
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
    pub(crate) fn prepare_owned_dispatch_history_v7(
        &self,
        claim: Arc<ActionsClaimStampV3>,
        state: ActionsWriteState,
        receipt: &ActionsReceipt,
    ) -> Result<Arc<ActionsOwnedDispatchHistoryV7>> {
        Ok(Arc::new(ActionsOwnedDispatchHistoryV7 {
            core: self.prepare_owned_history_fence_v8(
                claim,
                state,
                receipt,
                OwnedHistoryKindV8::Dispatch,
            )?,
        }))
    }
    pub(crate) fn validate_owned_dispatch_history_v7(
        &self,
        proof: &ActionsOwnedDispatchHistoryV7,
    ) -> Result<()> {
        self.validate_owned_history_fence_v8(&proof.core, OwnedHistoryKindV8::Dispatch)
    }
}
