//! Dispatch purpose in the SAME durable Actions RemoteTransaction controller.
//! One NEW explicit Confirm owns one Core/pool/prepaid ledger and original clock.
use super::{
    remote_transaction_v3::{self, RemoteTransactionPolicyV3},
    *,
};
use crate::{
    actions_workspace::{ActionsPublication, ActionsRefresh},
    actions_write_journal::{
        ActionsClaimStampV3, ActionsOwnedDispatchHistoryV7, ActionsWritePermit, ActionsWriteState,
    },
    drafts::{DraftClient, DraftPublicationAckV2, DraftPublicationFenceV2},
    providers::gh::*,
    scoped_actions_dispatch_publication_v7::DispatchConfirmV7,
    scoped_actions_publication_v3 as shared,
};
use std::{
    cell::{Cell, RefCell},
    path::Path,
    sync::Arc,
    time::Instant,
};
pub(crate) struct DispatchExecutionV7 {
    execution: ActionsExecution,
    history: Option<Arc<ActionsOwnedDispatchHistoryV7>>,
    run: Option<Arc<ScopedActionsRunTargetV1>>,
}
impl DispatchExecutionV7 {
    pub(crate) fn matches_history(&self) -> bool {
        let (reference, visible) = match &self.execution {
            ActionsExecution::AcceptedPending { publication, .. }
            | ActionsExecution::Confirmed { publication, .. } => (Some(publication), true),
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
            self.history.is_none() && self.run.is_none()
        }
    }
    pub(crate) fn into_parts(
        self,
    ) -> (
        ActionsExecution,
        Option<Arc<ActionsOwnedDispatchHistoryV7>>,
        Option<Arc<ScopedActionsRunTargetV1>>,
    ) {
        (self.execution, self.history, self.run)
    }
}
impl From<ActionsExecution> for DispatchExecutionV7 {
    fn from(execution: ActionsExecution) -> Self {
        Self {
            execution,
            history: None,
            run: None,
        }
    }
}
fn safe() -> GithubError {
    GithubError{kind:GithubErrorKind::IdentityChanged,message:"Original Dispatch source/ref/input/claim/draft/principal changed • no retry/private diagnostics".into()}
}
struct PolicyV7<'a> {
    request: &'a DispatchConfirmV7,
    original: &'a GithubProvider,
    measured: GithubProvider,
    session: Option<RepositoryCatalogueAccessSession<'a>>,
    captures: PostWriteCaptureAllowanceV2,
    identity: HostIdentity,
    schema: Arc<ScopedDispatchSchemaV7>,
    environments: Vec<ScopedDispatchEnvironmentV7>,
    database: &'a Path,
    drafts: &'a DraftClient,
    ack: &'a RefCell<Option<DraftPublicationAckV2>>,
    encoded: &'a RefCell<Option<String>>,
    stamp: Option<Arc<ActionsClaimStampV3>>,
    history: Option<Arc<ActionsOwnedDispatchHistoryV7>>,
    run: Option<Arc<ScopedActionsRunTargetV1>>,
    result: Option<(ActionsWriteState, Option<ActionsReceipt>)>,
    closed: bool,
    deadline: Instant,
}
impl RemoteTransactionPolicyV3 for PolicyV7<'_> {
    fn private_on_local_refusal(&self) -> bool {
        false
    }
    fn begin_error(&self, _: &anyhow::Error) -> String {
        "Original Dispatch protected/unavailable • no new request • diagnostics hidden".into()
    }
    fn claim_error(&self, _: &anyhow::Error) -> String {
        "Original Dispatch permit/draft changed • no invocation • protection retained".into()
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
        let stamp = self.stamp.as_ref().expect("same actual Dispatch claim");
        let preview = self.request.preview();
        let intent = preview.opened().intent();
        let source = intent.source();
        let mut session = self
            .session
            .take()
            .ok_or_else(|| ActionsInvocationFailureV3::NotStarted(safe()))?;
        let schema = &self.schema;
        let environments = &self.environments;
        let measured = &self.measured;
        let cancel = self.request.cancellation();
        let deadline = self.deadline;
        let endpoint = format!(
            "repos/{}/actions/workflows/{}/dispatches",
            schema.repository(),
            schema.original().model().id
        );
        let resource = format!(
            "{}/repository-id/{}/actions/workflow/{}",
            self.identity.host,
            schema.original().repository().id(),
            schema.original().model().id
        );
        let closed = Cell::new(false);
        let before = |boundary: ScopedDispatchInvocationV7<'_>| {
            shared::clock(cancel, deadline)?;
            if !Arc::ptr_eq(boundary.schema(), schema)
                || !Arc::ptr_eq(boundary.inputs(), preview.inputs())
                || !Arc::ptr_eq(boundary.payload(), preview.payload())
                || boundary.endpoint() != endpoint
                || boundary.resource() != resource
                || boundary.fresh().id != schema.original().model().id
                || boundary.fresh().path != schema.original().model().path
                || boundary.fresh().state != "active"
            {
                return Err(safe());
            }
            measured.validate_dispatch_ref_v7(schema, cancel)?;
            for environment in environments {
                let current =
                    measured.scoped_dispatch_environment_v7(schema, environment.name(), cancel)?;
                if current.id() != environment.id() {
                    return Err(safe());
                }
            }
            let current = session
                .observed_original_v1(source.reference().id)
                .ok_or_else(safe)?;
            measured.validate_dispatch_workflow_v7(schema, cancel)?;
            if let Some(pr) = source.pr_origin_v2() {
                pr.fresh_parent(measured, session.identity(), current, cancel)?;
            }
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
        let outcome = preview.plan().publish_dispatch_guarded_v7(
            self.original,
            measured,
            &mut self.captures,
            source,
            schema,
            preview.inputs(),
            environments,
            preview.payload(),
            permit.marker().attempt_id,
            cancel,
            deadline,
            before,
        );
        self.closed = closed.get();
        self.result = Some(match &outcome {
            Ok((publication, _)) => match &publication.disposition {
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
        outcome.map(|(publication, run)| {
            self.run = run;
            publication
        })
    }
    fn refresh(&mut self, publication: ActionsPublication) -> ActionsRefresh {
        publication.refresh_checked(self.original, self.request.cancellation())
    }
    fn final_identity(&mut self, journal: &ActionsWriteJournal) -> Result<HostIdentity, String> {
        let preview = self.request.preview();
        let intent = preview.opened().intent();
        let source = intent.source();
        let cancel = self.request.cancellation();
        let (state, receipt) = self
            .result
            .as_ref()
            .ok_or("Original Dispatch result missing")?;
        let receipt = receipt
            .as_ref()
            .filter(|_| self.closed)
            .ok_or("Private Dispatch result retired • protected marker retained")?;
        let stamp = self
            .stamp
            .as_ref()
            .ok_or("Original Dispatch claim missing")?;
        shared::clock(cancel, self.deadline).map_err(|_| "Original Dispatch phase ended")?;
        preview
            .opened()
            .validate_owned(self.database, journal, stamp, Some((*state, receipt)))
            .map_err(|_| "Original Dispatch source changed")?;
        journal
            .validate_result_stamp_v3(stamp, *state, Some(receipt))
            .map_err(|_| "Original Dispatch durable receipt changed")?;
        let ack = self
            .ack
            .borrow_mut()
            .take()
            .ok_or("Original Dispatch draft ACK missing")?;
        ack.check_current(
            self.drafts,
            self.database,
            source.original().scope(),
            intent.key(),
            cancel,
        )
        .map_err(|_| "Original Dispatch draft changed")?;
        let encoded = self.encoded.borrow();
        let expected = encoded.as_ref().ok_or("Exact Dispatch draft missing")?;
        if crate::storage::Cache::open(self.database)
            .map_err(|_| "Original cache unavailable")?
            .bounded_draft(source.original().scope(), intent.key(), 128 * 1024)
            .map_err(|_| "Original Dispatch draft unavailable")?
            .as_deref()
            != Some(expected.as_str())
        {
            return Err("Original exact Dispatch reference changed".into());
        }
        // Dispatch remains accepted_pending (not successful workflow execution).
        // Preserve safe reference for explicit Inspect/restart, no input values.
        preview
            .opened()
            .validate_owned(self.database, journal, stamp, Some((*state, receipt)))
            .map_err(|_| "Original Dispatch source changed")?;
        journal
            .validate_result_stamp_v3(stamp, *state, Some(receipt))
            .map_err(|_| "Original Dispatch receipt changed")?;
        let history = journal
            .prepare_owned_dispatch_history_v7(stamp.clone(), *state, receipt)
            .map_err(|_| "Actual Dispatch read fence unavailable")?;
        shared::clock(cancel, self.deadline).map_err(|_| "Original Dispatch phase ended")?;
        let identity = self
            .original
            .actions_postwrite_identity_v3(
                &mut self.captures,
                &self.identity,
                cancel,
                self.deadline,
            )
            .map_err(|_| "Original Dispatch final principal unavailable".to_owned())?;
        self.history = Some(history);
        Ok(identity)
    }
    fn publication_error(&self, _: &GithubError, _: ActionsWriteKind, _: bool) -> String {
        "Original Dispatch outcome unknown • exact protection retained • Inspect; no retry/private diagnostics".into()
    }
}
pub(crate) fn execute(
    request: &DispatchConfirmV7,
    provider: &GithubProvider,
    database: &Path,
    drafts: &DraftClient,
    fence: DraftPublicationFenceV2,
) -> DispatchExecutionV7 {
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
            "Original Dispatch ended/replayed/source changed • no POST",
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
        let caps: &[usize] = if preview.schema().protocol().detailed() {
            &[64 * 1024, 512 * 1024, 64 * 1024]
        } else {
            &[16 * 1024, 64 * 1024]
        };
        session.verify(source.reference(), cancel)?;
        let captures = session.reserve_postwrite_capture_v2(caps, 1, cancel)?;
        let window = session.reserve_dispatch_reads_v7(
            intent.original(),
            DISPATCH_POOL_CHILDREN_V7,
            1,
            cancel,
        )?;
        let measured = phase.clone().with_measured_read_window(window)?;
        let current = session
            .observed_original_v1(source.reference().id)
            .ok_or_else(safe)?;
        if let Some(pr) = source.pr_origin_v2() {
            pr.fresh_parent(&measured, &identity, current, cancel)?;
        }
        let versions = measured.scoped_dispatch_versions_v7(preview.schema().protocol(), cancel)?;
        let cli = GhCliVersion::parse_line(&identity.gh_version).map_err(|_| safe())?;
        let schema = measured.scoped_dispatch_schema_v7(
            current,
            intent.original(),
            preview.schema().choice().clone(),
            &versions,
            &cli,
            cancel,
        )?;
        if !schema.same(preview.schema()) {
            return Err(safe());
        }
        let names = dispatch_environment_names_v7(&schema, preview.inputs())?;
        let environments = names
            .iter()
            .map(|name| measured.scoped_dispatch_environment_v7(&schema, name, cancel))
            .collect::<GithubResult<Vec<_>>>()?;
        let payload = dispatch_payload_v7(&schema, preview.inputs(), &environments)?;
        if payload.as_ref() != preview.payload().as_ref() {
            return Err(safe());
        }
        preview.opened().validate_local(database)?;
        let descriptor = preview
            .plan()
            .scoped_dispatch_descriptor_v7(&measured, source, &schema)?;
        shared::clock(cancel, deadline)?;
        Ok((
            session,
            identity,
            captures,
            measured,
            schema,
            environments,
            descriptor,
        ))
    })();
    let (session, identity, captures, measured, schema, environments, descriptor) = match prepared {
        Ok(value) => value,
        Err(_) => return refused(
            "Fresh exact Dispatch ref/schema/protocol/environments/source unavailable • no POST",
            false,
        )
        .into(),
    };
    let mut journal = match ActionsWriteJournal::open(database) {
        Ok(journal) => journal,
        Err(_) => return refused("Original Dispatch journal unavailable • no POST", false).into(),
    };
    let ack = RefCell::new(None);
    let encoded = RefCell::new(None);
    let mut policy = PolicyV7 {
        request,
        original: &phase,
        measured,
        session: Some(session),
        captures,
        identity,
        schema,
        environments,
        database,
        drafts,
        ack: &ack,
        encoded: &encoded,
        stamp: None,
        history: None,
        run: None,
        result: None,
        closed: false,
        deadline,
    };
    let execution = remote_transaction_v3::execute(
        &mut journal,
        &descriptor,
        |marker| {
            shared::clock(cancel, deadline).map_err(|_| "Original Dispatch ended")?;
            // ONLY safe publication reference. Inputs, YAML and their hashes NEVER
            // enter draft/metadata, even when this explicit attempt is uncertain.
            let body=serde_json::to_string(&serde_json::json!({"version":7,"purpose":"scoped-actions-dispatch-v7","actions_publication":marker.publication_reference()})).map_err(|_|"Dispatch reference encoding unavailable")?;
            if body.len() > 16 * 1024 {
                return Err("Dispatch reference exceeds16KiB".into());
            }
            let saved = drafts
                .save_publication_and_wait_v2(fence, body.clone(), cancel)
                .map_err(|_| "Dispatch draft ACK failed • pending retained")?;
            saved
                .check_current(
                    drafts,
                    database,
                    source.original().scope(),
                    intent.key(),
                    cancel,
                )
                .map_err(|_| "Exact Dispatch draft changed")?;
            *ack.borrow_mut() = Some(saved);
            *encoded.borrow_mut() = Some(body);
            shared::clock(cancel, deadline).map_err(|_| "Original Dispatch ended".into())
        },
        &mut policy,
    );
    let visible = matches!(
        &execution,
        ActionsExecution::AcceptedPending { .. } | ActionsExecution::Confirmed { .. }
    );
    DispatchExecutionV7 {
        execution,
        history: if visible { policy.history.take() } else { None },
        run: if visible { policy.run.take() } else { None },
    }
}
