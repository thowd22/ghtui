//! Distinct Branch/PR READ/ACK-only wrappers on the SAME exact result fence.
//! No serde/Clone/source factory; a saved row cannot issue this live proof.
use super::owned_history_fence_v8::{OwnedHistoryFenceV8, OwnedHistoryKindV8};
use super::*;
use std::sync::Arc;
pub(crate) struct ActionsOwnedWorkflowRemoteHistoryV9 {
    core: Arc<OwnedHistoryFenceV8>,
    phase: WorkflowRemotePhaseV9,
}
/// Restriction only; retained under a new actual CreatePR permit/claim, never
/// serialized or accepted by any other operation or generic legacy begin.
pub(in crate::actions_write_journal) struct WorkflowPrPriorBranchFenceV9 {
    proof: Arc<ActionsOwnedWorkflowRemoteHistoryV9>,
}
impl WorkflowPrPriorBranchFenceV9 {
    pub(super) fn prepare_cleanup_rows_v9(
        &self,
        tx: &rusqlite::Transaction<'_>,
        path: &Path,
        rows: &mut Vec<crate::storage::workflow_cleanup_v9::WorkflowRawRowsV9>,
        heap: &crate::providers::gh::WorkflowJsonLedgerV9,
    ) -> Result<()> {
        self.validate_retained_v9(tx, path, 3)?;
        rows.push(
            crate::storage::workflow_cleanup_v9::WorkflowRawRowsV9::actual_attempt_v9(
                tx,
                &self.proof.core.marker.target.scope,
                self.proof.core.marker.attempt_id,
                heap,
            )?,
        );
        Ok(())
    }
    pub(in crate::actions_write_journal) fn prepare_before_begin_v9(
        transaction: &rusqlite::Transaction<'_>,
        path: &Path,
        target: &ActionsWriteTarget,
        intent: &ActionsWriteIntent,
        branch: &crate::scoped_actions_workflow_edit_v9::WorkflowPublishedBranchV9,
    ) -> Result<Arc<Self>> {
        let proof = branch.proof_v9();
        let original = &proof.core.marker;
        let old = original
            .context
            .workflow_edit_v9
            .as_ref()
            .context("Actual Branch context missing")?;
        let new = intent
            .context
            .workflow_edit_v9
            .as_ref()
            .context("Actual CreatePR context missing")?;
        if proof.phase != WorkflowRemotePhaseV9::Branch
            || original.kind != ActionsWriteKind::PublishWorkflowBranch
            || intent.kind != ActionsWriteKind::CreateWorkflowPr
            || new.phase != WorkflowRemotePhaseV9::Pr
            || old.phase != WorkflowRemotePhaseV9::Branch
            || !original.target.same_resources(target)
            || !matches!((&original.target.target,&target.target),(ActionsTarget::Workflow{repository,id},ActionsTarget::Workflow{repository:other,id:other_id}) if repository==other && id==other_id)
            || original.options != intent.options
            || original.context.original_repository_v3 != intent.context.original_repository_v3
            || original.context.original_workflow_v6 != intent.context.original_workflow_v6
            || original.context.original_pr_v3 != intent.context.original_pr_v3
            || original.context.workflow_commit != intent.context.workflow_commit
            || (
                old.head_repository_id,
                &old.head_repository_node,
                &old.head_original_name,
                &old.head_current_name,
                &old.head_ref,
                &old.base_ref,
                &old.original_commit,
                &old.committed_head,
                &old.base_commit,
            ) != (
                new.head_repository_id,
                &new.head_repository_node,
                &new.head_original_name,
                &new.head_current_name,
                &new.head_ref,
                &new.base_ref,
                &new.original_commit,
                &new.committed_head,
                &new.base_commit,
            )
        {
            bail!("Actual own Branch and new CreatePR source/target/ref/context differ");
        }
        workflow_edit_context_v9::validate_intent(intent)?;
        proof
            .core
            .validate_branch_advance_v9(transaction, path, 0)?;
        // Existing prepared raw-row/alias graph is retained by Arc. No body,
        // parser or additional SQL transaction is allocated under this begin.
        Ok(Arc::new(Self {
            proof: proof.clone(),
        }))
    }
    pub(in crate::actions_write_journal) fn validate_retained_v9(
        &self,
        connection: &Connection,
        path: &Path,
        increments: i64,
    ) -> Result<()> {
        if !(1..=4).contains(&increments) {
            bail!("No known own W9 generation transition");
        }
        self.proof
            .core
            .validate_branch_advance_v9(connection, path, increments)
    }
    pub(in crate::actions_write_journal) fn retained_bytes_v9(&self) -> usize {
        std::mem::size_of::<Self>() + 2 * std::mem::size_of::<usize>() + self.proof.retained_bytes()
    }
}
fn kind(phase: WorkflowRemotePhaseV9) -> OwnedHistoryKindV8 {
    match phase {
        WorkflowRemotePhaseV9::Branch => OwnedHistoryKindV8::WorkflowBranchV9,
        WorkflowRemotePhaseV9::Pr => OwnedHistoryKindV8::WorkflowPrV9,
    }
}
impl ActionsOwnedWorkflowRemoteHistoryV9 {
    #[cfg(test)]
    pub(crate) fn test_cleanup_claim_v9(&self) -> &Arc<ActionsClaimStampV3> {
        &self.core.claim
    }
    pub(crate) fn binds_cleanup_claim_v9(
        &self,
        request: &crate::scoped_actions_workflow_edit_v9::WorkflowRemoteConfirmV9,
        claim: &Arc<ActionsClaimStampV3>,
    ) -> bool {
        let opened = request
            .preview()
            .selection()
            .committed()
            .staged()
            .saved()
            .opened();
        let phase = match request.preview().phase() {
            crate::providers::gh::WorkflowEditPhaseV9::PublishBranch => {
                WorkflowRemotePhaseV9::Branch
            }
            crate::providers::gh::WorkflowEditPhaseV9::CreatePr => WorkflowRemotePhaseV9::Pr,
            _ => return false,
        };
        Arc::ptr_eq(claim, &self.core.claim)
            && self.phase == phase
            && self.core.marker.state == ActionsWriteState::Confirmed
            && self.core.marker.target.scope == *opened.intent().source().original().scope()
            && matches!(self.core.marker.target.target,ActionsTarget::Workflow{id,..} if id==opened.intent().original().model().id)
            && request.started_cleanup_v9()
    }
    pub(crate) fn prepare_cleanup_rows_v9(
        &self,
        tx: &rusqlite::Transaction<'_>,
        rows: &mut Vec<crate::storage::workflow_cleanup_v9::WorkflowRawRowsV9>,
        heap: &crate::providers::gh::WorkflowJsonLedgerV9,
    ) -> Result<()> {
        self.core
            .prepare_cleanup_rows_v9(tx, &self.core.claim.path, tx, rows, heap)
    }
    pub(crate) fn reference(&self) -> ActionsPublicationReference {
        self.core.marker.publication_reference()
    }
    pub(crate) fn phase(&self) -> WorkflowRemotePhaseV9 {
        self.phase
    }
    pub(crate) fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.core.retained_bytes() + 2 * std::mem::size_of::<usize>()
    }
    pub(crate) fn matches_pr_origin_v9(
        &self,
        target: &crate::pr_write_journal::PrWriteTarget,
        database: &Path,
        file: (u64, u64),
    ) -> bool {
        self.core.claim.matches_pr_origin_v4(target, database, file)
            && self
                .core
                .marker
                .context
                .workflow_edit_v9
                .as_ref()
                .is_some_and(|edit| edit.phase == self.phase)
            && self.core.marker.context.original_workflow_v6.is_some()
            && self.core.marker.context.original_dispatch_v7.is_none()
            && self.core.marker.context.original_run_v3.is_none()
    }
    pub(in crate::actions_write_journal) fn next_for_ack(
        &self,
        actual: &ActionsWriteMarker,
        next: ActionsWriteState,
    ) -> Result<Arc<Self>> {
        if next != ActionsWriteState::Acknowledged {
            bail!("Workflow remote observation ACK is a local release, never a new effect or causal transition");
        }
        Ok(Arc::new(Self {
            core: self.core.next_for_ack(actual, next)?,
            phase: self.phase,
        }))
    }
}
impl ActionsWriteJournal {
    pub(crate) fn prepare_owned_workflow_remote_history_v9(
        &self,
        claim: Arc<ActionsClaimStampV3>,
        receipt: &ActionsReceipt,
    ) -> Result<Arc<ActionsOwnedWorkflowRemoteHistoryV9>> {
        let phase = claim
            .marker
            .context
            .workflow_edit_v9
            .as_ref()
            .context("Actual Workflow remote claim omitted its phase")?
            .phase;
        Ok(Arc::new(ActionsOwnedWorkflowRemoteHistoryV9 {
            core: self.prepare_owned_history_fence_v8(
                claim,
                ActionsWriteState::Confirmed,
                receipt,
                kind(phase),
            )?,
            phase,
        }))
    }
    pub(crate) fn validate_owned_workflow_remote_history_v9(
        &self,
        proof: &ActionsOwnedWorkflowRemoteHistoryV9,
    ) -> Result<()> {
        self.validate_owned_history_fence_v8(&proof.core, kind(proof.phase))
    }
}
