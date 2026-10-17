//! Closed Workflow policy on the SAME Actions durable transaction/controller.
//! One fresh explicit Confirm Core/pool/prepaid cursor, never Preview renewal.
use super::{
    remote_transaction_v3::{self, RemoteTransactionPolicyV3},
    *,
};
use crate::{
    actions_workspace::{ActionsPublication, ActionsRefresh},
    actions_write_journal::{ActionsClaimStampV3, ActionsWritePermit, ActionsWriteState},
    drafts::{DraftClient, DraftPublicationAckV2, DraftPublicationFenceV2},
    providers::gh::*,
    scoped_actions_publication_v3 as shared,
    scoped_actions_workflow_publication_v6::WorkflowConfirmV6,
};
use std::{
    cell::{Cell, RefCell},
    path::Path,
    sync::Arc,
    time::Instant,
};
pub(crate) struct WorkflowExecutionV6 {
    execution: ActionsExecution,
    history: Option<Arc<crate::actions_write_journal::ActionsOwnedWorkflowHistoryV6>>,
}
impl WorkflowExecutionV6 {
    pub(crate) fn matches_history_v6(&self) -> bool {
        let (reference, visible) = match &self.execution {
            ActionsExecution::Confirmed { publication, .. }
            | ActionsExecution::AcceptedPending { publication, .. } => (Some(publication), true),
            ActionsExecution::Unknown {
                publication,
                private_visible,
                ..
            }
            | ActionsExecution::ReceiptPersistenceFailed {
                publication,
                private_visible,
                ..
            }
            | ActionsExecution::EffectConfirmedButRefreshFailed {
                publication,
                private_visible,
                ..
            }
            | ActionsExecution::AcceptedButRefreshFailed {
                publication,
                private_visible,
                ..
            } => (Some(publication), *private_visible),
            ActionsExecution::Refused {
                private_visible, ..
            } => (None, *private_visible),
            _ => return false,
        };
        if visible {
            reference
                .zip(self.history.as_ref())
                .is_some_and(|(reference, proof)| proof.reference() == *reference)
        } else {
            self.history.is_none()
        }
    }
    #[cfg(test)]
    pub(crate) fn result_for_test_v6(
        &self,
    ) -> (
        &ActionsExecution,
        Option<&Arc<crate::actions_write_journal::ActionsOwnedWorkflowHistoryV6>>,
    ) {
        (&self.execution, self.history.as_ref())
    }
    pub(crate) fn into_parts(
        self,
    ) -> (
        ActionsExecution,
        Option<Arc<crate::actions_write_journal::ActionsOwnedWorkflowHistoryV6>>,
    ) {
        (self.execution, self.history)
    }
}
impl From<ActionsExecution> for WorkflowExecutionV6 {
    fn from(execution: ActionsExecution) -> Self {
        Self {
            execution,
            history: None,
        }
    }
}
fn safe() -> GithubError {
    GithubError {
        kind: GithubErrorKind::IdentityChanged,
        message: "Original workflow source/claim/draft/principal changed • no retry".into(),
    }
}
struct PolicyV6<'a> {
    request: &'a WorkflowConfirmV6,
    original: &'a GithubProvider,
    measured: GithubProvider,
    session: Option<RepositoryCatalogueAccessSession<'a>>,
    captures: PostWriteCaptureAllowanceV2,
    identity: HostIdentity,
    target: Arc<ScopedActionsWorkflowV2>,
    database: &'a Path,
    drafts: &'a DraftClient,
    ack: &'a RefCell<Option<DraftPublicationAckV2>>,
    encoded: &'a RefCell<Option<String>>,
    stamp: Option<Arc<ActionsClaimStampV3>>,
    history: Option<Arc<crate::actions_write_journal::ActionsOwnedWorkflowHistoryV6>>,
    result: Option<(ActionsWriteState, Option<ActionsReceipt>)>,
    closed: bool,
    deadline: Instant,
}
impl RemoteTransactionPolicyV3 for PolicyV6<'_> {
    fn private_on_local_refusal(&self) -> bool {
        false
    }
    fn begin_error(&self, _: &anyhow::Error) -> String {
        "Original workflow protected/unavailable • no new request • no private diagnostics".into()
    }
    fn claim_error(&self, _: &anyhow::Error) -> String {
        "Original workflow permit/draft changed • no invocation • protection retained".into()
    }
    fn publish(
        &mut self,
        journal: &ActionsWriteJournal,
        permit: &ActionsWritePermit,
    ) -> Result<ActionsPublication, ActionsInvocationFailureV3> {
        self.stamp =
            Some(Arc::new(journal.capture_claim_stamp_v3(permit).map_err(
                |_| ActionsInvocationFailureV3::NotStarted(safe()),
            )?));
        let stamp = self.stamp.as_ref().expect("same actual Workflow claim");
        let preview = self.request.preview();
        let intent = preview.opened().intent();
        let source = intent.source();
        let mut session = self
            .session
            .take()
            .ok_or_else(|| ActionsInvocationFailureV3::NotStarted(safe()))?;
        let target = &self.target;
        let measured = &self.measured;
        let cancel = self.request.cancellation();
        let deadline = self.deadline;
        let expected_endpoint = format!(
            "repos/{}/actions/workflows/{}/{}",
            target.repository().name(),
            target.model().id,
            if intent.enabled() {
                "enable"
            } else {
                "disable"
            }
        );
        let expected_resource = format!(
            "{}/repository-id/{}/actions/workflow/{}",
            self.identity.host,
            target.repository().id(),
            target.model().id
        );
        let closed = Cell::new(false);
        let before = |boundary: ScopedWorkflowInvocationV6<'_>| {
            shared::clock(cancel, deadline)?;
            if !Arc::ptr_eq(boundary.original(), target)
                || boundary.enabled() != intent.enabled()
                || boundary.endpoint() != expected_endpoint
                || boundary.resource() != expected_resource
                || boundary.fresh().id != target.model().id
                || boundary.fresh().path != target.model().path
                || boundary.fresh().state != target.model().state
            {
                return Err(safe());
            }
            let current = session
                .observed_original_v1(source.reference().id)
                .ok_or_else(safe)?;
            if let Some(pr) = source.pr_origin_v2() {
                pr.fresh_parent(measured, session.identity(), current, cancel)?;
            }
            let last =
                measured.scoped_actions_workflow_snapshot_v6(current, target, true, cancel)?;
            if !last.same_pin_v6(target, true) {
                return Err(safe());
            }
            drop(last);
            session.verify(source.reference(), cancel)?;
            let principal = session.close_with_guard(cancel, || {
                shared::clock(cancel, deadline)?;
                preview
                    .opened()
                    .validate_owned(self.database, journal, stamp, None)?;
                journal.validate_claim_stamp_v3(stamp).map_err(|_| safe())?;
                self.ack
                    .borrow()
                    .as_ref()
                    .ok_or_else(safe)?
                    .check_current(
                        self.drafts,
                        self.database,
                        source.original().scope(),
                        intent.key(),
                        cancel,
                    )
                    .map_err(|_| safe())?;
                let encoded = self.encoded.borrow();
                let expected = encoded.as_ref().ok_or_else(safe)?;
                if crate::storage::Cache::open(self.database)
                    .map_err(|_| safe())?
                    .bounded_draft(source.original().scope(), intent.key(), 128 * 1024)
                    .map_err(|_| safe())?
                    .as_deref()
                    != Some(expected.as_str())
                {
                    return Err(safe());
                }
                shared::clock(cancel, deadline)
            })?;
            shared::scope(source, principal.identity())?;
            closed.set(true);
            shared::clock(cancel, deadline)
        };
        let outcome = preview.plan().publish_workflow_guarded_v6(
            self.original,
            measured,
            &mut self.captures,
            source,
            target,
            permit.marker().attempt_id,
            intent.enabled(),
            cancel,
            deadline,
            before,
        );
        self.closed = closed.get();
        self.result = Some(match &outcome {
            Ok(publication) => match &publication.disposition {
                crate::actions_workspace::ActionsDisposition::VerifiedEffect { receipt } => {
                    (ActionsWriteState::Confirmed, Some(receipt.clone()))
                }
                crate::actions_workspace::ActionsDisposition::Accepted { receipt, .. } => {
                    (ActionsWriteState::AcceptedPending, Some(receipt.clone()))
                }
                crate::actions_workspace::ActionsDisposition::LocalOnly => {
                    (ActionsWriteState::Uncertain, None)
                }
            },
            Err(ActionsInvocationFailureV3::NotStarted(_)) => (ActionsWriteState::Pending, None),
            Err(ActionsInvocationFailureV3::WriteMayHaveStarted(_)) => {
                (ActionsWriteState::Uncertain, None)
            }
        });
        outcome
    }
    fn refresh(&mut self, publication: ActionsPublication) -> ActionsRefresh {
        publication.refresh_checked(self.original, self.request.cancellation())
    }
    fn final_identity(&mut self, journal: &ActionsWriteJournal) -> Result<HostIdentity, String> {
        let cancel = self.request.cancellation();
        let preview = self.request.preview();
        let intent = preview.opened().intent();
        let source = intent.source();
        let (state, receipt) = self
            .result
            .as_ref()
            .ok_or("Original workflow result unavailable")?;
        if !self.closed || receipt.is_none() {
            return Err("Private workflow result retired • protected marker retained".into());
        }
        let receipt = receipt
            .as_ref()
            .ok_or("Original workflow receipt missing")?;
        let stamp = self
            .stamp
            .as_ref()
            .ok_or("Original workflow claim missing")?;
        shared::clock(cancel, self.deadline).map_err(|_| "Original workflow phase ended")?;
        preview
            .opened()
            .validate_owned(self.database, journal, stamp, Some((*state, receipt)))
            .map_err(|_| "Original workflow source changed")?;
        journal
            .validate_result_stamp_v3(stamp, *state, Some(receipt))
            .map_err(|_| "Original workflow durable result changed")?;
        let ack = self
            .ack
            .borrow_mut()
            .take()
            .ok_or("Original workflow draft ACK missing")?;
        ack.check_current(
            self.drafts,
            self.database,
            source.original().scope(),
            intent.key(),
            cancel,
        )
        .map_err(|_| "Original workflow draft ACK changed")?;
        let encoded = self.encoded.borrow();
        let expected = encoded.as_ref().ok_or("Original exact draft missing")?;
        if crate::storage::Cache::open(self.database)
            .map_err(|_| "Original cache unavailable")?
            .bounded_draft(source.original().scope(), intent.key(), 128 * 1024)
            .map_err(|_| "Original draft unavailable")?
            .as_deref()
            != Some(expected.as_str())
        {
            return Err("Original exact workflow draft reference changed".into());
        }
        if *state == ActionsWriteState::Confirmed {
            self.drafts
                .remove_publication_and_wait_v2(ack, cancel)
                .map_err(|_| {
                    "Observed workflow effect retained • exact draft cleanup unavailable"
                })?;
        }
        preview
            .opened()
            .validate_owned(self.database, journal, stamp, Some((*state, receipt)))
            .map_err(|_| "Workflow source changed before final principal")?;
        journal
            .validate_result_stamp_v3(stamp, *state, Some(receipt))
            .map_err(|_| "Workflow receipt changed before final principal")?;
        shared::clock(cancel, self.deadline).map_err(|_| "Original workflow phase ended")?;
        // Prepare the exact own-result read fence BEFORE final UID. It is not
        // released to native state unless that last actual identity succeeds.
        let history = journal
            .prepare_owned_workflow_history_v6(stamp.clone(), *state, receipt)
            .map_err(|_| "Actual workflow result read fence unavailable")?;
        // LAST actual data-producing step; no SQL/body/layout work follows.
        let current = self
            .original
            .actions_postwrite_identity_v3(
                &mut self.captures,
                &self.identity,
                cancel,
                self.deadline,
            )
            .map_err(|_| "Original workflow final numeric principal unavailable".to_owned())?;
        self.history = Some(history);
        Ok(current)
    }
    fn publication_error(&self, _: &GithubError, _: ActionsWriteKind, _: bool) -> String {
        "Original workflow outcome unknown • exact protected marker retained • inspect; no retry/private diagnostics".into()
    }
}
pub(crate) fn execute(
    request: &WorkflowConfirmV6,
    provider: &GithubProvider,
    database: &Path,
    drafts: &DraftClient,
    fence: DraftPublicationFenceV2,
) -> WorkflowExecutionV6 {
    let cancel = request.cancellation();
    let deadline = request.deadline().min(fence.deadline());
    let preview = request.preview();
    let intent = preview.opened().intent();
    let source = intent.source();
    if request.start().is_err()
        || shared::clock(cancel, deadline).is_err()
        || preview.opened().validate_local(database).is_err()
    {
        return refused(
            "Original workflow Confirm ended/replayed/source changed • no PUT",
            false,
        )
        .into();
    }
    let phase = provider.clone().with_catalogue_read_deadline(deadline);
    let prepared = (|| -> GithubResult<_> {
        let mut session = phase.begin_repository_catalogue_access(
            source.original().source(),
            &shared::access_limits(),
            cancel,
        )?;
        shared::scope(source, session.identity())?;
        let identity = session.identity().clone();
        let captures =
            session.reserve_postwrite_capture_v2(&[16 * 1024, 64 * 1024, 64 * 1024], 2, cancel)?;
        let window =
            session.reserve_measured_external_reads(15, 512 * 1024, 1024 * 1024, 2, cancel)?;
        let measured = phase.clone().with_measured_read_window(window)?;
        session.verify(source.reference(), cancel)?;
        let current = session
            .observed_original_v1(source.reference().id)
            .ok_or_else(safe)?;
        if let Some(pr) = source.pr_origin_v2() {
            pr.fresh_parent(&measured, &identity, current, cancel)?;
        }
        let target = measured.scoped_actions_workflow_snapshot_v6(
            current,
            preview.target(),
            true,
            cancel,
        )?;
        if !target.same_pin_v6(preview.target(), true)
            || target.repository().name() != preview.target().repository().name()
        {
            return Err(safe());
        }
        preview.opened().validate_local(database)?;
        let descriptor = preview.plan().scoped_workflow_state_descriptor_v6(
            &measured,
            source,
            &target,
            intent.enabled(),
        )?;
        shared::clock(cancel, deadline)?;
        Ok((session, identity, captures, measured, target, descriptor))
    })();
    let (session, identity, captures, measured, target, descriptor) = match prepared {
        Ok(value) => value,
        Err(_) => return refused(
            "Fresh workflow source/principal/node/path/state unavailable • no publication started",
            false,
        )
        .into(),
    };
    let mut journal = match ActionsWriteJournal::open(database) {
        Ok(journal) => journal,
        Err(_) => {
            return refused(
                "Original workflow journal unavailable • no publication started",
                false,
            )
            .into()
        }
    };
    let ack = RefCell::new(None);
    let encoded = RefCell::new(None);
    let mut policy = PolicyV6 {
        request,
        original: &phase,
        measured,
        session: Some(session),
        captures,
        identity,
        target,
        database,
        drafts,
        ack: &ack,
        encoded: &encoded,
        stamp: None,
        history: None,
        result: None,
        closed: false,
        deadline,
    };
    let execution = remote_transaction_v3::execute(
        &mut journal,
        &descriptor,
        |marker| {
            shared::clock(cancel, deadline).map_err(|_| "Original workflow attempt ended")?;
            let body=serde_json::to_string(&serde_json::json!({"version":6,"purpose":"scoped-actions-workflow-v6","actions_publication":marker.publication_reference()})).map_err(|_|"Workflow reference encoding unavailable")?;
            if body.len() > 16 * 1024 {
                return Err("Original workflow reference exceeds16KiB".into());
            }
            let saved = drafts
                .save_publication_and_wait_v2(fence, body.clone(), cancel)
                .map_err(|_| "Workflow draft publication ACK failed • pending retained")?;
            saved
                .check_current(
                    drafts,
                    database,
                    source.original().scope(),
                    intent.key(),
                    cancel,
                )
                .map_err(|_| "Exact workflow draft queue changed")?;
            *ack.borrow_mut() = Some(saved);
            *encoded.borrow_mut() = Some(body);
            shared::clock(cancel, deadline).map_err(|_| "Original workflow attempt ended".into())
        },
        &mut policy,
    );
    WorkflowExecutionV6 {
        execution,
        history: policy.history.take(),
    }
}
