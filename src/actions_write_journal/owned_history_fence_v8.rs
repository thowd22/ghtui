//! Shared mechanical SAME-claim/raw-row/generation READ fence. Kind is closed;
//! this is never a provider/source/write permit, and is never serialized.
use super::*;
use crate::actions_write_journal::scoped_history_v5::row_fence_v6::HistoryRowFenceV6;
use std::sync::Arc;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum OwnedHistoryKindV8 {
    Run,
    Workflow,
    Dispatch,
    WorkflowBranchV9,
    WorkflowPrV9,
}
pub(super) struct OwnedHistoryFenceV8 {
    pub(super) claim: Arc<ActionsClaimStampV3>,
    pub(super) marker: ActionsWriteMarker,
    generations: Vec<i64>,
    row: HistoryRowFenceV6,
    kind: OwnedHistoryKindV8,
}
fn kind_matches(marker: &ActionsWriteMarker, kind: OwnedHistoryKindV8) -> bool {
    match kind {
        OwnedHistoryKindV8::WorkflowBranchV9 | OwnedHistoryKindV8::WorkflowPrV9 => {
            let (phase, write) = if kind == OwnedHistoryKindV8::WorkflowBranchV9 {
                (
                    WorkflowRemotePhaseV9::Branch,
                    ActionsWriteKind::PublishWorkflowBranch,
                )
            } else {
                (
                    WorkflowRemotePhaseV9::Pr,
                    ActionsWriteKind::CreateWorkflowPr,
                )
            };
            matches!(marker.target.target(), ActionsTarget::Workflow { .. })
                && marker.context.original_workflow_v6.is_some()
                && marker.context.original_run_v3.is_none()
                && marker.context.original_dispatch_v7.is_none()
                && marker
                    .context
                    .workflow_edit_v9
                    .as_ref()
                    .is_some_and(|edit| edit.phase == phase)
                && marker.kind == write
        }
        OwnedHistoryKindV8::Dispatch => {
            matches!(marker.target.target(), ActionsTarget::Workflow { .. })
                && marker.context.original_workflow_v6.is_some()
                && marker.context.original_dispatch_v7.is_some()
                && marker.context.original_run_v3.is_none()
                && marker.kind == ActionsWriteKind::DispatchWorkflow
        }
        OwnedHistoryKindV8::Workflow => {
            matches!(marker.target.target(), ActionsTarget::Workflow { .. })
                && marker.context.original_workflow_v6.is_some()
                && marker.context.original_run_v3.is_none()
                && matches!(
                    marker.kind,
                    ActionsWriteKind::EnableWorkflow | ActionsWriteKind::DisableWorkflow
                )
        }
        OwnedHistoryKindV8::Run => {
            matches!(marker.target.target(), ActionsTarget::Run { .. })
                && marker.context.original_run_v3.is_some()
                && marker.context.original_workflow_v6.is_none()
                && matches!(
                    marker.kind,
                    ActionsWriteKind::Cancel
                        | ActionsWriteKind::ForceCancel
                        | ActionsWriteKind::RerunAll
                        | ActionsWriteKind::RerunFailed
                        | ActionsWriteKind::RerunJob
                        | ActionsWriteKind::DeleteRun
                        | ActionsWriteKind::DeleteLogs
                )
        }
    }
}
impl OwnedHistoryFenceV8 {
    pub(super) fn prepare_cleanup_rows_v9(
        &self,
        connection: &Connection,
        path: &Path,
        tx: &rusqlite::Transaction<'_>,
        rows: &mut Vec<crate::storage::workflow_cleanup_v9::WorkflowRawRowsV9>,
        heap: &crate::providers::gh::WorkflowJsonLedgerV9,
    ) -> Result<()> {
        use crate::storage::workflow_cleanup_v9::WorkflowRawRowsV9 as Raw;
        if !matches!(
            self.kind,
            OwnedHistoryKindV8::WorkflowBranchV9 | OwnedHistoryKindV8::WorkflowPrV9
        ) || !kind_matches(&self.marker, self.kind)
            || self.marker.state != ActionsWriteState::Confirmed
            || path != self.claim.path
            || file(path)? != self.claim.file
            || !same_metadata(&self.marker, &self.claim.marker)
            || !self.row.matches_state_v6(connection, &self.marker)?
            || generations(connection, &self.marker.target)? != self.generations
        {
            bail!("Actual W9 cleanup own claim/raw result changed");
        }
        rows.push(Raw::actual_attempt_v9(
            tx,
            &self.marker.target.scope,
            self.marker.attempt_id,
            heap,
        )?);
        for key in &self.marker.target.resources {
            rows.extend(Raw::actual_alias_v9(
                tx,
                &self.marker.target.scope,
                key,
                heap,
            )?);
        }
        if let Some(prior) = &self.claim.workflow_pr_prior_branch_v9 {
            prior.prepare_cleanup_rows_v9(tx, path, rows, heap)?;
        }
        Ok(())
    }
    pub(super) fn retained_bytes(&self) -> usize {
        512 * 1024
            + self.row.retained_bytes()
            + self.generations.capacity() * std::mem::size_of::<i64>()
            + self
                .claim
                .workflow_pr_prior_branch_v9
                .as_ref()
                .map_or(0, |prior| prior.retained_bytes_v9())
    }
    pub(super) fn validate_branch_advance_v9(
        &self,
        connection: &Connection,
        path: &Path,
        increments: i64,
    ) -> Result<()> {
        if self.kind != OwnedHistoryKindV8::WorkflowBranchV9
            || !kind_matches(&self.marker, self.kind)
            || self.marker.state != ActionsWriteState::Confirmed
            || self.claim.force_escalation_v5.is_some()
            || self.claim.workflow_pr_prior_branch_v9.is_some()
            || path != self.claim.path
            || file(path)? != self.claim.file
            || !same_metadata(&self.marker, &self.claim.marker)
            || !self.row.matches_state_v6(connection, &self.marker)?
        {
            bail!("Actual original confirmed Branch raw row/file changed");
        }
        let expected = self
            .generations
            .iter()
            .map(|n| {
                n.checked_add(increments)
                    .context("Original Branch own generation overflow")
            })
            .collect::<Result<Vec<_>>>()?;
        if generations(connection, &self.marker.target)? != expected {
            bail!("Original Branch aliases changed outside the exact own PR transition");
        }
        Ok(())
    }
    pub(super) fn next_for_ack(
        &self,
        actual: &ActionsWriteMarker,
        next: ActionsWriteState,
    ) -> Result<Arc<Self>> {
        if actual.publication_reference() != self.marker.publication_reference()
            || !same_metadata(actual, &self.marker)
            || actual.receipt != self.marker.receipt
            || actual.state != self.marker.state
            || actual.created_at != self.marker.created_at
            || !matches!(
                next,
                ActionsWriteState::Acknowledged | ActionsWriteState::TransitionObserved
            )
        {
            bail!("Actual owned history ACK origin/result differs");
        }
        let mut marker = self.marker.clone();
        marker.state = next;
        let generations = self
            .generations
            .iter()
            .map(|n| {
                n.checked_add(1)
                    .context("Owned history ACK generation overflow")
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Arc::new(Self {
            claim: self.claim.clone(),
            marker,
            generations,
            row: self.row.clone(),
            kind: self.kind,
        }))
    }
}
impl ActionsWriteJournal {
    pub(super) fn prepare_owned_history_fence_v8(
        &self,
        claim: Arc<ActionsClaimStampV3>,
        state: ActionsWriteState,
        receipt: &ActionsReceipt,
        kind: OwnedHistoryKindV8,
    ) -> Result<Arc<OwnedHistoryFenceV8>> {
        self.validate_result_stamp_v3(&claim, state, Some(receipt))?;
        if !kind_matches(&claim.marker, kind)
            || !matches!(
                state,
                ActionsWriteState::Confirmed | ActionsWriteState::AcceptedPending
            )
        {
            bail!("No typed actual result to retain for this history kind");
        }
        let marker = bounded_original::read_checked_attempt(
            &self.connection,
            &claim.marker.target.scope,
            claim.marker.attempt_id,
        )?
        .context("Actual owned result unavailable")?;
        if !same_metadata(&marker, &claim.marker)
            || marker.state != state
            || marker.receipt.as_ref() != Some(receipt)
            || marker.created_at != claim.marker.created_at
        {
            bail!("Actual owned result differs");
        }
        let row = HistoryRowFenceV6::capture(&self.connection, &marker)?;
        if !row.claimed_v6() {
            bail!("Original actual result lost its claimed receipt");
        }
        let generations = generations(&self.connection, &marker.target)?;
        let proof = Arc::new(OwnedHistoryFenceV8 {
            claim,
            marker,
            generations,
            row,
            kind,
        });
        self.validate_owned_history_fence_v8(&proof, kind)?;
        Ok(proof)
    }
    pub(super) fn validate_owned_history_fence_v8(
        &self,
        proof: &OwnedHistoryFenceV8,
        kind: OwnedHistoryKindV8,
    ) -> Result<()> {
        if let Some(prior) = &proof.claim.workflow_pr_prior_branch_v9 {
            prior.validate_retained_v9(
                &self.connection,
                &self.path,
                if proof.marker.state == ActionsWriteState::Acknowledged {
                    4
                } else {
                    3
                },
            )?;
        }
        if let Some(force) = &proof.claim.force_escalation_v5 {
            force_escalation_v5::validate_retained(&self.connection, force)?;
        }
        if proof.kind != kind
            || !kind_matches(&proof.marker, kind)
            || self.path != proof.claim.path
            || file(&self.path)? != proof.claim.file
            || !same_metadata(&proof.marker, &proof.claim.marker)
            || generations(&self.connection, &proof.marker.target)? != proof.generations
            || !proof
                .row
                .matches_state_v6(&self.connection, &proof.marker)?
        {
            bail!("Original owned history kind/row/generation/file changed");
        }
        Ok(())
    }
}
