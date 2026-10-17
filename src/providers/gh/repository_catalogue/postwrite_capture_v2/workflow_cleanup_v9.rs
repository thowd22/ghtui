//! Only the SAME actual W9 prepaid post cursor can seal local cleanup.
use super::*;
use crate::drafts::{ClosedWorkflowCleanupV9, PreparedWorkflowCleanupMessageV9};
use std::sync::Weak;
pub(crate) struct WorkflowCleanupSealV9 {
    ticket: Weak<()>,
    cancel: CancellationToken,
    effective: Instant,
}
impl WorkflowCleanupSealV9 {
    pub(crate) fn matches_v9(
        &self,
        ticket: &Arc<()>,
        cancel: &CancellationToken,
        effective: Instant,
    ) -> bool {
        self.ticket.ptr_eq(&Arc::downgrade(ticket))
            && self.cancel.same_instance(cancel)
            && self.effective == effective
            && !cancel.is_cancelled()
            && Instant::now() < effective
    }
}
impl WorkflowPostCaptureV9 {
    pub(crate) fn close_workflow_cleanup_v9(
        &mut self,
        provider: &GithubProvider,
        expected: &HostIdentity,
        heap: &WorkflowJsonLedgerV9,
        prepared: PreparedWorkflowCleanupMessageV9,
        cancel: &CancellationToken,
    ) -> GithubResult<(HostIdentity, ClosedWorkflowCleanupV9)> {
        if !matches!(
            self.phase,
            WorkflowEditPhaseV9::PublishBranch | WorkflowEditPhaseV9::CreatePr
        ) || !prepared.matches_cursor_v9(&self.original, self.phase, cancel, self.deadline())
        {
            return Err(refused());
        }
        // Weak clone is allocation-free. Its source ticket and all envelope /
        // seal layouts already belong to the actual pre-UID debit.
        let ticket = Arc::downgrade(prepared.ticket_v9().ok_or_else(refused)?);
        let effective = prepared.effective_v9();
        let current = self.final_principal_inner_v9(provider, expected, Some(heap), cancel)?;
        // Exact UID, issuer/runner/token/I and consumed-last-slot checks above
        // are the existing paid read. No Source/SQL/JSON/Doc follows this seal.
        let seal = WorkflowCleanupSealV9 {
            ticket,
            cancel: cancel.clone(),
            effective,
        };
        Ok((current, prepared.close_v9(seal)))
    }
}
