//! Run-only actual controller result. Existing source/run Arcs are borrowed,
//! never reconstructed from disk or promoted from historical metadata.
use super::owned_history_fence_v8::{OwnedHistoryFenceV8, OwnedHistoryKindV8};
use super::*;
use crate::providers::gh::ScopedActionsRunTargetV1;
use std::sync::Arc;
pub(crate) struct ActionsOwnedRunHistoryV8 {
    core: Arc<OwnedHistoryFenceV8>,
    original: Arc<ScopedActionsRunTargetV1>,
}
impl ActionsOwnedRunHistoryV8 {
    pub(crate) fn reference(&self) -> ActionsPublicationReference {
        self.core.marker.publication_reference()
    }
    pub(crate) fn original(&self) -> &Arc<ScopedActionsRunTargetV1> {
        &self.original
    }
    pub(crate) fn matches_pr_origin_v8(
        &self,
        target: &crate::pr_write_journal::PrWriteTarget,
        database: &Path,
        file: (u64, u64),
    ) -> bool {
        self.core.claim.matches_pr_origin_v4(target, database, file)
            && self.core.marker.context.original_run_v3.is_some()
            && self.core.marker.context.original_workflow_v6.is_none()
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
            original: self.original.clone(),
        }))
    }
}
impl ActionsWriteJournal {
    pub(crate) fn prepare_owned_run_history_v8(
        &self,
        claim: Arc<ActionsClaimStampV3>,
        state: ActionsWriteState,
        receipt: &ActionsReceipt,
        original: &Arc<ScopedActionsRunTargetV1>,
        provider: &GithubProvider,
    ) -> Result<Arc<ActionsOwnedRunHistoryV8>> {
        let context = &claim.marker.context;
        if !original.original_writer_v3(provider)
            || context.run_id != Some(original.run().id)
            || context.attempt != Some(original.run().attempt)
            || context.head_sha.as_deref() != Some(original.run().head_sha.as_str())
            || context.original_repository_v3.as_ref().is_none_or(|r| {
                r.repository_id != original.repository_id()
                    || Some(r.repository_node.as_str()) != original.repository_node()
            })
            || context.original_run_v3.as_ref().is_none_or(|r| {
                Some(r.run_node.as_str()) != original.node_v2()
                    || r.workflow_id != original.run().workflow_id
                    || r.check_suite_id != original.suite_v3()
            })
        {
            bail!("Actual original Run differs from owned claim");
        }
        Ok(Arc::new(ActionsOwnedRunHistoryV8 {
            core: self.prepare_owned_history_fence_v8(
                claim,
                state,
                receipt,
                OwnedHistoryKindV8::Run,
            )?,
            original: original.clone(),
        }))
    }
    pub(crate) fn validate_owned_run_history_v8(
        &self,
        proof: &ActionsOwnedRunHistoryV8,
    ) -> Result<()> {
        self.validate_owned_history_fence_v8(&proof.core, OwnedHistoryKindV8::Run)
    }
}
