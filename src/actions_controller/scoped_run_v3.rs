//! SAME durable Actions transaction, actual fresh explicit Confirm source.
//! No Preview API clock, second ledger, name-only writer or auto retry.
use super::{
    remote_transaction_v3::{self, RemoteTransactionPolicyV3},
    *,
};
use crate::{
    actions_workspace::{ActionsPublication, ActionsRefresh},
    actions_write_journal::{ActionsClaimStampV3, ActionsWritePermit, ActionsWriteState},
    drafts::{DraftClient, DraftPublicationAckV2, DraftPublicationFenceV2},
    providers::gh::*,
    scoped_actions_publication_v3::{self as source, ScopedActionsConfirmV3},
};
use std::{cell::RefCell, path::Path, sync::Arc, time::Instant};

pub(crate) struct RunExecutionV8 {
    execution: ActionsExecution,
    history: Option<Arc<crate::actions_write_journal::ActionsOwnedRunHistoryV8>>,
}
impl RunExecutionV8 {
    pub(crate) fn matches_history_v8(&self) -> bool {
        // A same-owner pre-invocation refusal has no result/history reference
        // to bind. Preserve that existing refusal presentation; it grants no
        // new READ/ACK capability and must not leave the Form busy forever.
        if matches!(self.execution, ActionsExecution::Refused { .. }) {
            return self.history.is_none();
        }
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
    pub(crate) fn result_for_test_v8(
        &self,
    ) -> (
        &ActionsExecution,
        Option<&Arc<crate::actions_write_journal::ActionsOwnedRunHistoryV8>>,
    ) {
        (&self.execution, self.history.as_ref())
    }
    pub(crate) fn into_parts(
        self,
    ) -> (
        ActionsExecution,
        Option<Arc<crate::actions_write_journal::ActionsOwnedRunHistoryV8>>,
    ) {
        (self.execution, self.history)
    }
}
impl From<ActionsExecution> for RunExecutionV8 {
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
        message: "Original Actions source, claimed journal or actual draft ACK changed; no retry"
            .into(),
    }
}
fn refused_run(error: impl Into<String>, visible: bool) -> RunExecutionV8 {
    super::refused(error, visible).into()
}
struct ScopedTransactionV3<'a> {
    request: &'a ScopedActionsConfirmV3,
    original: &'a GithubProvider,
    measured: GithubProvider,
    session: Option<RepositoryCatalogueAccessSession<'a>>,
    captures: PostWriteCaptureAllowanceV2,
    identity: HostIdentity,
    target: Arc<ScopedActionsRunTargetV1>,
    database: &'a Path,
    drafts: &'a DraftClient,
    ack: &'a RefCell<Option<DraftPublicationAckV2>>,
    encoded: &'a RefCell<Option<String>>,
    stamp: Option<Arc<ActionsClaimStampV3>>,
    history: Option<Arc<crate::actions_write_journal::ActionsOwnedRunHistoryV8>>,
    result: Option<(ActionsWriteState, Option<ActionsReceipt>)>,
    source_closed: bool,
    deadline: Instant,
    force_escalation_v5: Option<Arc<crate::actions_write_journal::ScopedForceEscalationV5>>,
}
impl RemoteTransactionPolicyV3 for ScopedTransactionV3<'_> {
    fn begin(
        &mut self,
        journal: &mut ActionsWriteJournal,
        descriptor: &crate::actions_workspace::ActionsPublicationDescriptor,
    ) -> anyhow::Result<ActionsWritePermit> {
        if let Some(proof) = &self.force_escalation_v5 {
            if !proof.matches(
                self.request.preview().opened().intent().source(),
                &self.target,
            ) {
                anyhow::bail!("Original escalation source/run changed");
            }
            journal.begin_scoped_force_v5(&descriptor.target, &descriptor.intent, proof.clone())
        } else {
            journal.begin(&descriptor.target, &descriptor.intent)
        }
    }
    fn private_on_local_refusal(&self) -> bool {
        false
    }
    fn begin_error(&self, _failure: &anyhow::Error) -> String {
        "Original Actions publication remains protected or unavailable; no new request started and no private metadata diagnostics are shown".into()
    }
    fn claim_error(&self, _failure: &anyhow::Error) -> String {
        "Original Actions permit/ACK changed; no remote effect was invoked and its pending barrier remains".into()
    }
    fn publish(
        &mut self,
        journal: &ActionsWriteJournal,
        permit: &ActionsWritePermit,
    ) -> Result<ActionsPublication, ActionsInvocationFailureV3> {
        self.stamp = Some(
            journal
                .capture_claim_stamp_v3(permit)
                .map(Arc::new)
                .map_err(|_| ActionsInvocationFailureV3::NotStarted(safe()))?,
        );
        let stamp = self.stamp.as_ref().expect("same actual captured claim");
        let preview = self.request.preview();
        let intent = preview.opened().intent();
        let original_source = intent.source();
        let original = self.original;
        let measured = &self.measured;
        let target = &self.target;
        let cancel = self.request.cancellation();
        let deadline = self.deadline;
        let mut session = self
            .session
            .take()
            .ok_or_else(|| ActionsInvocationFailureV3::NotStarted(safe()))?;
        let database = self.database;
        let drafts = self.drafts;
        let ack = self.ack;
        let encoded = self.encoded;
        let host = self.identity.host.as_str();
        let source_closed = std::cell::Cell::new(false);
        let action = intent.action(preview.options());
        let expected_endpoint = match action {
            RunMutation::Cancel => format!(
                "repos/{}/actions/runs/{}/cancel",
                target.repository(),
                target.run().id
            ),
            RunMutation::RerunAll { .. } => format!(
                "repos/{}/actions/runs/{}/rerun",
                target.repository(),
                target.run().id
            ),
            RunMutation::RerunFailed { .. } => format!(
                "repos/{}/actions/runs/{}/rerun-failed-jobs",
                target.repository(),
                target.run().id
            ),
            RunMutation::RerunJob { job_id, .. } => {
                format!("repos/{}/actions/jobs/{job_id}/rerun", target.repository())
            }
            RunMutation::ForceCancel => format!(
                "repos/{}/actions/runs/{}/force-cancel",
                target.repository(),
                target.run().id
            ),
            RunMutation::DeleteRun => format!(
                "repos/{}/actions/runs/{}",
                target.repository(),
                target.run().id
            ),
            RunMutation::DeleteLogs => format!(
                "repos/{}/actions/runs/{}/logs",
                target.repository(),
                target.run().id
            ),
            _ => return Err(ActionsInvocationFailureV3::NotStarted(safe())),
        };
        let expected_input = if matches!(
            action,
            RunMutation::RerunAll { .. }
                | RunMutation::RerunFailed { .. }
                | RunMutation::RerunJob { .. }
        ) {
            Some(serde_json::json!({"enable_debug_logging":preview.options().debug_logging}))
        } else {
            None
        };
        let before_write = |boundary: ScopedRunInvocationRefV3<'_>| {
            source::clock(cancel, deadline)?;
            if !Arc::ptr_eq(boundary.original(), target)
                || boundary.repository() != target.repository()
                || boundary.fresh().id != target.run().id
                || boundary.method()
                    != if matches!(action, RunMutation::DeleteRun | RunMutation::DeleteLogs) {
                        "DELETE"
                    } else {
                        "POST"
                    }
                || boundary.action_v4() != action
                || boundary.input() != expected_input.as_ref()
                || boundary.endpoint() != expected_endpoint
                || match (boundary.selected_job_v4(), intent.selected_job()) {
                    (None, None) => false,
                    (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
                    _ => true,
                }
                || boundary.resource()
                    != format!(
                        "{}/repository-id/{}/actions/run/{}",
                        host,
                        target.repository_id(),
                        target.run().id
                    )
            {
                return Err(safe());
            }
            let current = session
                .observed_original_v1(original_source.reference().id)
                .ok_or_else(safe)?;
            if let Some(pr) = original_source.pr_origin_v2() {
                pr.fresh_parent(measured, session.identity(), current, cancel)?;
            }
            let closing = measured.scoped_actions_run_snapshot_v3(current, target, cancel)?;
            if !source::same_run(&closing, target) {
                return Err(safe());
            }
            drop(closing);
            session.verify(original_source.reference(), cancel)?;
            let closed = session.close_with_guard(cancel, || {
                source::clock(cancel, deadline)?;
                preview
                    .opened()
                    .validate_owned_publication_local_v4(database, journal, stamp, None)?;
                journal.validate_claim_stamp_v3(stamp).map_err(|_| safe())?;
                let current = ack.borrow();
                current
                    .as_ref()
                    .ok_or_else(safe)?
                    .check_current(
                        drafts,
                        database,
                        original_source.original().scope(),
                        intent.key(),
                        cancel,
                    )
                    .map_err(|_| safe())?;
                let saved = encoded.borrow();
                let saved = saved.as_ref().ok_or_else(safe)?;
                let actual = crate::storage::Cache::open(database)
                    .map_err(|_| safe())?
                    .bounded_draft(original_source.original().scope(), intent.key(), 128 * 1024)
                    .map_err(|_| safe())?;
                if actual.as_deref() != Some(saved.as_str()) {
                    return Err(safe());
                }
                source::clock(cancel, deadline)
            })?;
            source::scope(original_source, closed.identity())?;
            source_closed.set(true);
            source::clock(cancel, deadline)
        };
        let outcome = if matches!(
            action,
            RunMutation::ForceCancel | RunMutation::DeleteRun | RunMutation::DeleteLogs
        ) {
            preview.plan().publish_run_guarded_v5(
                original,
                measured,
                &mut self.captures,
                original_source,
                target,
                action,
                cancel,
                deadline,
                before_write,
            )
        } else if action == RunMutation::Cancel {
            preview.plan().publish_run_guarded_v3(
                original,
                measured,
                &mut self.captures,
                original_source,
                target,
                cancel,
                deadline,
                before_write,
            )
        } else {
            preview.plan().publish_run_guarded_v4(
                original,
                measured,
                &mut self.captures,
                original_source,
                target,
                intent.selected_job(),
                action,
                cancel,
                deadline,
                before_write,
            )
        };
        self.source_closed = source_closed.get();
        self.result = Some(match &outcome {
            Ok(publication) => match &publication.disposition {
                ActionsDisposition::VerifiedEffect { receipt } => {
                    (ActionsWriteState::Confirmed, Some(receipt.clone()))
                }
                ActionsDisposition::Accepted { receipt, .. } => {
                    (ActionsWriteState::AcceptedPending, Some(receipt.clone()))
                }
                ActionsDisposition::LocalOnly => (ActionsWriteState::Uncertain, None),
            },
            Err(ActionsInvocationFailureV3::NotStarted(_)) => (ActionsWriteState::Pending, None),
            Err(ActionsInvocationFailureV3::WriteMayHaveStarted(_)) => {
                (ActionsWriteState::Uncertain, None)
            }
        });
        outcome
    }
    fn refresh(&mut self, publication: ActionsPublication) -> ActionsRefresh {
        // Concrete Cancel has no optional fetch; SAME normal outcome converter.
        publication.refresh_checked(self.original, self.request.cancellation())
    }
    fn final_identity(&mut self, journal: &ActionsWriteJournal) -> Result<HostIdentity, String> {
        let cancel = self.request.cancellation();
        let preview = self.request.preview();
        let intent = preview.opened().intent();
        let (state, receipt) = self
            .result
            .as_ref()
            .ok_or("Original Actions result unavailable")?;
        if !self.source_closed || receipt.is_none() {
            // Error paths retain safe durable metadata but no private underlay.
            // Never skip a prepaid receipt slot or renew source for diagnostics.
            return Err(
                "Original Actions private result retired; protected marker retained".into(),
            );
        }
        source::clock(cancel, self.deadline).map_err(|_| "Original Actions phase ended")?;
        preview
            .opened()
            .validate_owned_publication_local_v4(
                self.database,
                journal,
                self.stamp.as_ref().ok_or("Original claim unavailable")?,
                Some((
                    *state,
                    receipt.as_ref().ok_or("Original receipt unavailable")?,
                )),
            )
            .map_err(|_| "Original Actions source changed")?;
        journal
            .validate_result_stamp_v3(
                self.stamp.as_ref().ok_or("Original claim unavailable")?,
                *state,
                receipt.as_ref(),
            )
            .map_err(|_| "Original Actions durable result changed")?;
        let original_source = intent.source().original();
        let acknowledgement = self
            .ack
            .borrow_mut()
            .take()
            .ok_or("Original draft ACK unavailable")?;
        acknowledgement
            .check_current(
                self.drafts,
                self.database,
                original_source.scope(),
                intent.key(),
                cancel,
            )
            .map_err(|_| "Original Actions draft ACK changed")?;
        let saved = self.encoded.borrow();
        let saved = saved
            .as_ref()
            .ok_or("Original draft reference unavailable")?;
        if crate::storage::Cache::open(self.database)
            .map_err(|_| "Original cache unavailable")?
            .bounded_draft(original_source.scope(), intent.key(), 128 * 1024)
            .map_err(|_| "Original draft unavailable")?
            .as_deref()
            != Some(saved.as_str())
        {
            return Err("Original exact draft reference changed".into());
        }
        if *state == ActionsWriteState::Confirmed {
            self.drafts
                .remove_publication_and_wait_v2(acknowledgement, cancel)
                .map_err(|_| "Confirmed effect retained; exact draft cleanup unavailable")?;
        }
        preview
            .opened()
            .validate_owned_publication_local_v4(
                self.database,
                journal,
                self.stamp.as_ref().ok_or("Original claim unavailable")?,
                Some((
                    *state,
                    receipt.as_ref().ok_or("Original receipt unavailable")?,
                )),
            )
            .map_err(|_| "Original source changed before final principal")?;
        journal
            .validate_result_stamp_v3(
                self.stamp.as_ref().ok_or("Original claim unavailable")?,
                *state,
                receipt.as_ref(),
            )
            .map_err(|_| "Original result changed before final principal")?;
        source::clock(cancel, self.deadline).map_err(|_| "Original Actions phase ended")?;
        let history = journal
            .prepare_owned_run_history_v8(
                self.stamp
                    .as_ref()
                    .ok_or("Original claim unavailable")?
                    .clone(),
                *state,
                receipt.as_ref().ok_or("Original receipt unavailable")?,
                &self.target,
                self.original,
            )
            .map_err(|_| "Actual own Run read fence unavailable")?;
        // LAST data-producing step. No SQL or full body/layout work follows.
        let current = self
            .original
            .actions_postwrite_identity_v3(
                &mut self.captures,
                &self.identity,
                cancel,
                self.deadline,
            )
            .map_err(|_| {
                "Original Actions final numeric principal changed/unavailable".to_owned()
            })?;
        self.history = Some(history);
        Ok(current)
    }
    fn publication_error(
        &self,
        _failure: &GithubError,
        _kind: ActionsWriteKind,
        _private_visible: bool,
    ) -> String {
        "Original Cancel outcome is unknown; its exact protected marker remains. Inspect the original attempt; no request was retried and no private child diagnostics are displayed.".into()
    }
}
/// Exactly one actual NEW prequeue Confirm operation. There is no supplied
/// boolean/restored field that can authorize publication or reset the attempt.
pub(crate) fn execute(
    request: &ScopedActionsConfirmV3,
    provider: &GithubProvider,
    database: &Path,
    drafts: &DraftClient,
    publication: DraftPublicationFenceV2,
) -> RunExecutionV8 {
    let cancel = request.cancellation();
    let deadline = request.deadline().min(publication.deadline());
    let preview = request.preview();
    let source = preview.opened().intent().source();
    if request.start().is_err() || source::clock(cancel, deadline).is_err() {
        return refused_run(
            "Original Confirm already attempted/cancelled/expired; nothing sent",
            false,
        );
    }
    if preview.opened().validate_local(database).is_err() {
        return refused_run(
            "Original Actions source/database changed; nothing sent",
            false,
        );
    }
    let phase = provider.clone().with_catalogue_read_deadline(deadline);
    let prepared = (|| -> GithubResult<_> {
        let mut session = phase.begin_repository_catalogue_access(
            source.original().source(),
            &source::access_limits(),
            cancel,
        )?;
        source::scope(source, session.identity())?;
        let identity = session.identity().clone();
        let action = preview.opened().intent().action(preview.options());
        let caps: &[usize] = match action {
            // Repo parser already has64KiB document cap. A256KiB reservation
            // would not leave the SAME2MiB outer ledger its original closing
            // repository/principal allowance; do not widen or reset it.
            RunMutation::DeleteRun => &[16 * 1024, 512 * 1024, 64 * 1024, 64 * 1024],
            RunMutation::ForceCancel | RunMutation::DeleteLogs => {
                &[16 * 1024, 512 * 1024, 64 * 1024]
            }
            _ => &[64 * 1024, 512 * 1024, 64 * 1024],
        };
        let captures = session.reserve_postwrite_capture_v2(caps, 2, cancel)?;
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
        let target = measured.scoped_actions_run_snapshot_v3(current, preview.target(), cancel)?;
        if !source::same_run(&target, preview.target())
            || target.repository() != preview.target().repository()
        {
            return Err(safe());
        }
        preview.opened().validate_local(database)?;
        let descriptor = preview.plan().scoped_run_descriptor_v4(
            &measured,
            source,
            &target,
            preview.opened().intent().selected_job(),
            preview.opened().intent().action(preview.options()),
        )?;
        source::clock(cancel, deadline)?;
        Ok((session, identity, captures, measured, target, descriptor))
    })();
    let (session,identity,captures,measured,target,mut descriptor)=match prepared{
        Ok(value)=>value,
        Err(_)=>return refused_run("Fresh original Actions source/principal/run changed or bounded read unavailable; no publication started",false),
    };
    let mut journal = match ActionsWriteJournal::open(database) {
        Ok(journal) => journal,
        Err(_) => {
            return refused_run(
                "Original Actions journal unavailable; no publication started",
                false,
            )
        }
    };
    let force_escalation_v5 = if descriptor.intent.kind == ActionsWriteKind::ForceCancel {
        if !preview.options().force_acknowledged {
            return refused_run(
                "Explicit force escalation choice unavailable; nothing sent",
                false,
            );
        }
        let proof = match journal.prepare_force_escalation_v5(
            &measured,
            source,
            &target,
            &descriptor.target,
            &descriptor.intent,
        ) {
            Ok(value) => value,
            Err(_) => {
                return refused_run(
                    "Original accepted Cancel/active source changed; no escalation started",
                    false,
                )
            }
        };
        if proof.as_ref().map(|value| value.metadata().cancel).as_ref()
            != preview.opened().force_candidate_v5()
        {
            return refused_run(
                "The displayed original accepted Cancel changed; no escalation started",
                false,
            );
        }
        if let Some(value) = &proof {
            descriptor.intent.context.force_escalation_v5 = Some(value.metadata());
            descriptor.target =
                match crate::actions_write_journal::ActionsWriteTarget::from_original_context_v3(
                    source.original().scope(),
                    descriptor.target.target(),
                    &descriptor.intent.context,
                ) {
                    Ok(value) => value,
                    Err(_) => return refused_run("Original escalation aliases unavailable", false),
                };
        }
        proof
    } else {
        None
    };
    let ack = RefCell::new(None);
    let encoded = RefCell::new(None);
    let mut policy = ScopedTransactionV3 {
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
        source_closed: false,
        deadline,
        force_escalation_v5,
    };
    let execution = remote_transaction_v3::execute(
        &mut journal,
        &descriptor,
        |marker| {
            source::clock(cancel, deadline).map_err(|_| "Original Confirm phase ended")?;
            // Safe exact reference ONLY; no source capability/body/log/secret bytes.
            let body=serde_json::to_string(&serde_json::json!({
            "version":3,"purpose":"scoped-actions-v3","actions_publication":marker.publication_reference(),
        })).map_err(|_|"Original Actions draft reference encoding failed")?;
            if body.len() > 16 * 1024 {
                return Err("Original Actions reference envelope exceeds16KiB".into());
            }
            let saved = drafts
                .save_publication_and_wait_v2(publication, body.clone(), cancel)
                .map_err(|_| {
                    "Original Actions reference acknowledgement failed; pending barrier retained"
                })?;
            saved
                .check_current(
                    drafts,
                    database,
                    source.original().scope(),
                    preview.opened().intent().key(),
                    cancel,
                )
                .map_err(|_| "Original exact Actions draft queue changed")?;
            *ack.borrow_mut() = Some(saved);
            *encoded.borrow_mut() = Some(body);
            source::clock(cancel, deadline).map_err(|_| "Original Confirm phase ended".into())
        },
        &mut policy,
    );
    RunExecutionV8 {
        execution,
        history: policy.history.take(),
    }
}
