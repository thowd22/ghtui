//! W9 remote operations share the existing durable Actions transaction. Local
//! step receipts, Preview text and saved references are never write permits.
use super::{
    remote_transaction_v3::{self, RemoteTransactionPolicyV3},
    *,
};
use crate::{
    actions_workspace::{ActionsPublication, ActionsRefresh},
    actions_write_journal::{
        ActionsClaimStampV3, ActionsOwnedWorkflowRemoteHistoryV9, ActionsWritePermit,
        ActionsWriteState,
    },
    drafts::{DraftClient, DraftPublicationAckV2, DraftPublicationFenceV2},
    git::workflow_edit_v9::{
        WorkflowMutationLockV9, WorkflowPreparedPushV9, WorkflowRemoteGitObservationV9,
    },
    prepared_form_text_v1::PreparedFormTextV1,
    providers::gh::*,
    scoped_actions_publication_v3 as shared,
    scoped_actions_workflow_edit_v9::{
        PreparedWorkflowBranchV9, WorkflowPublishedBranchV9, WorkflowRemoteConfirmV9,
        WorkflowRemotePreviewV9, WorkflowRemoteReleaseV9,
    },
};
use std::{cell::RefCell, path::Path, sync::Arc, time::Instant};

pub(crate) struct WorkflowRemoteExecutionV9 {
    cleanup_failure: Option<crate::drafts::WorkflowCleanupFailureV9>,
    preview: Arc<WorkflowRemotePreviewV9>,
    execution: ActionsExecution,
    history: Option<Arc<ActionsOwnedWorkflowRemoteHistoryV9>>,
    document: Option<Arc<PreparedFormTextV1>>,
    branch: Option<Arc<WorkflowPublishedBranchV9>>,
}
pub(crate) struct WorkflowRemotePublishedV9 {
    pub(crate) reference: ActionsPublicationReference,
    pub(crate) history: Option<Arc<ActionsOwnedWorkflowRemoteHistoryV9>>,
    pub(crate) document: Arc<PreparedFormTextV1>,
    pub(crate) branch: Option<Arc<WorkflowPublishedBranchV9>>,
}
impl WorkflowRemoteExecutionV9 {
    pub(crate) fn cleanup_failure_v9(&self) -> Option<crate::drafts::WorkflowCleanupFailureV9> {
        self.cleanup_failure
    }
    pub(crate) fn matches_preview_v9(&self, preview: &Arc<WorkflowRemotePreviewV9>) -> bool {
        if !Arc::ptr_eq(preview, &self.preview) {
            return false;
        }
        match (&self.execution, &self.history) {
            (ActionsExecution::Confirmed { publication, .. }, Some(history)) => {
                *publication == history.reference() && self.document.is_some()
            }
            (_, None) => true,
            _ => false,
        }
    }
    pub(crate) fn into_published_v9(self) -> Option<WorkflowRemotePublishedV9> {
        let ActionsExecution::Confirmed { publication, .. } = self.execution else {
            return None;
        };
        let history = self.history?;
        if history.reference() != publication {
            return None;
        }
        Some(WorkflowRemotePublishedV9 {
            reference: publication,
            // The Branch operational witness already retains this SAME
            // read-only proof. Keeping it does not grant CreatePR authority;
            // it lets the actual retained result use its own View/ACK route.
            history: Some(history),
            document: self.document?,
            branch: self.branch,
        })
    }
    pub(crate) fn private_visible_v9(&self) -> bool {
        match &self.execution {
            ActionsExecution::Confirmed { .. } => true,
            ActionsExecution::Refused {
                private_visible, ..
            }
            | ActionsExecution::Unknown {
                private_visible, ..
            }
            | ActionsExecution::ReceiptPersistenceFailed {
                private_visible, ..
            }
            | ActionsExecution::EffectConfirmedButRefreshFailed {
                private_visible, ..
            }
            | ActionsExecution::AcceptedButRefreshFailed {
                private_visible, ..
            } => *private_visible,
            _ => false,
        }
    }
}
fn safe() -> GithubError {
    GithubError { kind: GithubErrorKind::IdentityChanged,
    message: "Original workflow remote source/claim/ref/principal unavailable; private diagnostics omitted; no retry".into() }
}

struct PolicyV9<'a, 'lock> {
    cleanup_failure: Option<crate::drafts::WorkflowCleanupFailureV9>,
    request: &'a WorkflowRemoteConfirmV9,
    lock: &'a WorkflowMutationLockV9<'lock>,
    original: &'a GithubProvider,
    measured: GithubProvider,
    session: Option<RepositoryCatalogueAccessSession<'a>>,
    post: WorkflowPostCaptureV9,
    identity: HostIdentity,
    repositories: Arc<WorkflowRemoteRepositoriesV9>,
    refs: Arc<WorkflowRemoteRefsV9>,
    git: Arc<WorkflowRemoteGitObservationV9>,
    push: Option<WorkflowPreparedPushV9>,
    payload: Option<Arc<WorkflowPrPayloadV9>>,
    database: &'a Path,
    drafts: &'a DraftClient,
    ack: &'a RefCell<Option<DraftPublicationAckV2>>,
    encoded: &'a RefCell<Option<String>>,
    claim: Option<Arc<ActionsClaimStampV3>>,
    result: Option<ActionsReceipt>,
    document: Option<Arc<PreparedFormTextV1>>,
    history: Option<Arc<ActionsOwnedWorkflowRemoteHistoryV9>>,
    material: usize,
    effective: Instant,
    closed: bool,
    branch: Option<Arc<WorkflowPublishedBranchV9>>,
    json_heap: Arc<WorkflowJsonLedgerV9>,
    _models_heap: WorkflowJsonChargeV9,
    _payload_heap: Option<WorkflowJsonChargeV9>,
    _journal_heap: WorkflowJsonChargeV9,
    document_heap: Option<WorkflowJsonChargeV9>,
}
impl RemoteTransactionPolicyV3 for PolicyV9<'_, '_> {
    fn begin(
        &mut self,
        journal: &mut ActionsWriteJournal,
        descriptor: &crate::actions_workspace::ActionsPublicationDescriptor,
    ) -> anyhow::Result<ActionsWritePermit> {
        match self.request.published_branch_v9() {
            Some(branch) if self.request.preview().phase() == WorkflowEditPhaseV9::CreatePr => {
                journal.begin_workflow_pr_after_branch_v9(
                    &descriptor.target,
                    &descriptor.intent,
                    branch,
                )
            }
            Some(_) => Err(anyhow::anyhow!(
                "No prior Branch transition for this operation"
            )),
            None => journal.begin(&descriptor.target, &descriptor.intent),
        }
    }
    fn private_on_local_refusal(&self) -> bool {
        false
    }
    fn begin_error(&self, _: &anyhow::Error) -> String {
        "Original workflow remote publication protected or unavailable; no new request".into()
    }
    fn claim_error(&self, _: &anyhow::Error) -> String {
        "Original workflow permit/draft changed; no invocation; protection retained".into()
    }
    fn publication_error(&self, _: &GithubError, _: ActionsWriteKind, _: bool) -> String {
        "Original workflow remote outcome unknown; exact protected marker retained; inspect, never resend".into()
    }
    fn publish(
        &mut self,
        journal: &ActionsWriteJournal,
        permit: &ActionsWritePermit,
    ) -> Result<ActionsPublication, ActionsInvocationFailureV3> {
        use ActionsInvocationFailureV3::{NotStarted, WriteMayHaveStarted};
        let cancel = self.request.cancellation();
        let opened = self
            .request
            .preview()
            .selection()
            .committed()
            .staged()
            .saved()
            .opened();
        let intent = opened.intent();
        let source = intent.source();
        let claim = Arc::new(
            journal
                .capture_claim_stamp_v3(permit)
                .map_err(|_| NotStarted(safe()))?,
        );
        self.claim = Some(claim.clone());
        // Fixed ordered opening observations are now closed again BEFORE
        // source SQL and the final pre-write numeric UID. No old pool is used
        // after close; only the original fixed prepaid cursor survives.
        self.refs
            .close(&self.measured, &self.json_heap, cancel)
            .map_err(NotStarted)?;
        self.repositories
            .close(&self.measured, &self.json_heap, cancel)
            .map_err(NotStarted)?;
        let mut session = self.session.take().ok_or_else(|| NotStarted(safe()))?;
        let current = session
            .observed_original_v1(source.reference().id)
            .ok_or_else(|| NotStarted(safe()))?;
        let last = self
            .measured
            .scoped_actions_workflow_snapshot_heap_v9(
                current,
                intent.original(),
                true,
                &self.json_heap,
                cancel,
            )
            .map_err(NotStarted)?;
        if !last.same_pin_v6(intent.original(), true) {
            return Err(NotStarted(safe()));
        }
        drop(last);
        if let Some(pr) = source.pr_origin_v2() {
            pr.fresh_workflow_edit_parent_v9(
                &self.measured,
                &self.identity,
                current,
                &self.json_heap,
                cancel,
            )
            .map_err(NotStarted)?;
        }
        session
            .verify(source.reference(), cancel)
            .map_err(NotStarted)?;
        self.post
            .validate_prewrite(&self.measured, cancel)
            .map_err(NotStarted)?;
        let closed = session
            .close_with_guard(cancel, || {
                opened.validate_owned_remote_v9(self.database, cancel, journal, &claim, None)?;
                journal
                    .validate_claim_stamp_v3(&claim)
                    .map_err(|_| safe())?;
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
                if crate::storage::Cache::open(self.database)
                    .map_err(|_| safe())?
                    .bounded_workflow_edit_draft_v9(intent)
                    .map_err(|_| safe())?
                    .as_deref()
                    != self.encoded.borrow().as_deref()
                {
                    return Err(safe());
                }
                shared::clock(cancel, self.effective)
            })
            .map_err(NotStarted)?;
        shared::scope(source, closed.identity()).map_err(NotStarted)?;
        let release = WorkflowRemoteReleaseV9::from_closed_v9(
            self.request,
            self.lock,
            closed,
            self.git.clone(),
            self.refs.clone(),
            self.effective,
        )
        .map_err(NotStarted)?;
        self.closed = true;
        let publication = match self.request.preview().phase() {
            WorkflowEditPhaseV9::PublishBranch => {
                let (write, read) = self
                    .push
                    .take()
                    .ok_or_else(|| NotStarted(safe()))?
                    .release_v9(&release)
                    .map_err(NotStarted)?;
                let write = self.post.remote_git_v9(self.original, write, cancel);
                let read = self.post.remote_git_v9(self.original, read, cancel);
                let observed = self.post.observe_pushed_branch_v9(
                    self.original,
                    &self.refs,
                    &self.json_heap,
                    cancel,
                );
                write.map_err(WriteMayHaveStarted)?;
                read.map_err(WriteMayHaveStarted)?;
                ActionsPublication::observed_workflow_branch_v9(
                    &self.identity,
                    &observed.map_err(WriteMayHaveStarted)?,
                    permit.marker().attempt_id,
                )
                .map_err(WriteMayHaveStarted)?
            }
            WorkflowEditPhaseV9::CreatePr => {
                let payload = self.payload.as_ref().ok_or_else(|| NotStarted(safe()))?;
                release.check_refs_v9(payload.refs()).map_err(NotStarted)?;
                let observed = self
                    .post
                    .create_workflow_pr_v9(
                        self.original,
                        payload,
                        &release,
                        &self.json_heap,
                        cancel,
                    )
                    .map_err(WriteMayHaveStarted)?;
                ActionsPublication::observed_workflow_pr_v9(
                    &self.identity,
                    &self.refs,
                    &observed,
                    permit.marker().attempt_id,
                )
                .map_err(WriteMayHaveStarted)?
            }
            _ => return Err(NotStarted(safe())),
        };
        let ActionsDisposition::VerifiedEffect { receipt } = &publication.disposition else {
            return Err(WriteMayHaveStarted(safe()));
        };
        self.result = Some(receipt.clone());
        let lines = &publication
            .workspace
            .publication_inspection
            .as_ref()
            .ok_or_else(|| WriteMayHaveStarted(safe()))?
            .lines;
        // Fixed metadata-only rows, already bounded by repository/ref/ID/SHA
        // factories. Their prepared document is charged before final UID.
        self.request
            .check_material_v9(
                self.material
                    .checked_add(512 * 1024)
                    .ok_or_else(|| WriteMayHaveStarted(safe()))?,
                4 * 1024 * 1024,
            )
            .map_err(WriteMayHaveStarted)?;
        let mut doc_heap = self
            .json_heap
            .reserve_model_v9(1024 * 1024)
            .map_err(WriteMayHaveStarted)?;
        let doc = PreparedFormTextV1::prepare_with_remaining_v9(
            lines,
            cancel,
            self.effective,
            512 * 1024,
            1024 * 1024,
        )
        .map_err(|_| WriteMayHaveStarted(safe()))?;
        doc_heap
            .retain_actual_v9(doc.retained_bytes())
            .map_err(WriteMayHaveStarted)?;
        self.document_heap = Some(doc_heap);
        self.document = Some(doc);
        Ok(publication)
    }
    fn refresh(&mut self, publication: ActionsPublication) -> ActionsRefresh {
        publication.refresh_checked(self.original, self.request.cancellation())
    }
    fn final_identity(&mut self, journal: &ActionsWriteJournal) -> Result<HostIdentity, String> {
        let cancel = self.request.cancellation();
        let opened = self
            .request
            .preview()
            .selection()
            .committed()
            .staged()
            .saved()
            .opened();
        let source = opened.intent().source();
        let claim = self
            .claim
            .as_ref()
            .ok_or("Actual original workflow claim unavailable")?;
        let receipt = self
            .result
            .as_ref()
            .ok_or("Original remote result uncertain; protected marker retained")?;
        if !self.closed || self.document.is_none() {
            return Err("Original remote result document unavailable".into());
        }
        opened
            .validate_owned_remote_v9(
                self.database,
                cancel,
                journal,
                claim,
                Some((ActionsWriteState::Confirmed, receipt)),
            )
            .map_err(|_| "Original workflow source changed")?;
        journal
            .validate_result_stamp_v3(claim, ActionsWriteState::Confirmed, Some(receipt))
            .map_err(|_| "Original workflow raw result changed")?;
        let ack = self
            .ack
            .borrow_mut()
            .take()
            .ok_or("Actual workflow draft ACK unavailable")?;
        ack.check_current(
            self.drafts,
            self.database,
            source.original().scope(),
            opened.intent().key(),
            cancel,
        )
        .map_err(|_| "Original workflow draft changed")?;
        opened
            .validate_owned_remote_v9(
                self.database,
                cancel,
                journal,
                claim,
                Some((ActionsWriteState::Confirmed, receipt)),
            )
            .map_err(|_| "Original workflow boundary changed before final UID")?;
        shared::clock(cancel, self.effective)
            .map_err(|_| "Original workflow attempt ended before result material")?;
        let mut branch = if self.request.preview().phase() == WorkflowEditPhaseV9::PublishBranch {
            Some(
                PreparedWorkflowBranchV9::prepare_actual_branch_v9(
                    self.request,
                    &self.refs,
                    journal,
                    claim.clone(),
                    receipt,
                )
                .map_err(|_| "Actual original branch transition unavailable")?,
            )
        } else {
            None
        };
        let history = match &branch {
            Some(branch) => branch.proof_v9().clone(),
            None => journal
                .prepare_owned_workflow_remote_history_v9(claim.clone(), receipt)
                .map_err(|_| "Original workflow own result fence unavailable")?,
        };
        self.request
            .check_material_v9(
                self.material
                    .checked_add(history.retained_bytes())
                    .and_then(|n| {
                        n.checked_add(self.document.as_ref().map_or(0, |d| d.retained_bytes()))
                    })
                    .and_then(|n| n.checked_add(128 * 1024))
                    .ok_or("Workflow result capacity overflow")?,
                4 * 1024 * 1024,
            )
            .map_err(|_| "Original workflow result exceeds retained bound")?;
        let encoded = self
            .encoded
            .borrow_mut()
            .take()
            .ok_or("Actual original workflow reference unavailable")?;
        let binding = ack
            .prepare_workflow_cleanup_binding_v9(
                self.drafts,
                crate::drafts::WorkflowCleanupInputV9 {
                    request: self.request,
                    history: &history,
                    claim,
                    encoded,
                    heap: &self.json_heap,
                    cancel,
                    effective: self.effective,
                },
            )
            .map_err(|e| e.to_string())?;
        let cache = crate::storage::Cache::open(self.database)
            .map_err(|_| "Original cleanup cache unavailable")?;
        let prepared = cache
            .prepare_workflow_cleanup_v9(self.request, &history, binding, &self.json_heap)
            .map_err(|e| e.to_string())?;
        if let Some(branch) = &mut branch {
            branch
                .bind_cleanup_v9(&prepared)
                .map_err(|_| "Actual Branch cleanup binding differs")?;
        }
        let prepared = self
            .drafts
            .prepare_workflow_cleanup_message_v9(ack, prepared, &self.json_heap, cancel)
            .map_err(|e| e.to_string())?;
        let (current, closed) = self
            .post
            .close_workflow_cleanup_v9(
                self.original,
                &self.identity,
                &self.json_heap,
                prepared,
                cancel,
            )
            .map_err(|_| "Original final numeric principal unavailable")?;
        let committed = match self
            .drafts
            .commit_workflow_cleanup_v9(closed, self.request, cancel)
        {
            crate::drafts::WorkflowCleanupOutcomeV9::Removed(done) => done,
            crate::drafts::WorkflowCleanupOutcomeV9::Retained => {
                self.cleanup_failure = Some(crate::drafts::WorkflowCleanupFailureV9::Retained);
                return Err(crate::drafts::WorkflowCleanupFailureV9::Retained.to_string());
            }
            crate::drafts::WorkflowCleanupOutcomeV9::Unknown => {
                self.cleanup_failure = Some(crate::drafts::WorkflowCleanupFailureV9::Unknown);
                return Err(crate::drafts::WorkflowCleanupFailureV9::Unknown.to_string());
            }
        };
        // This is the ONLY operational release: same ticket, same raw result,
        // same history Arc and positively acknowledged one-Immediate cleanup.
        let branch = branch
            .map(|branch| branch.release_after_cleanup_v9(&committed))
            .transpose()
            .map_err(|_| "Actual Branch cleanup acknowledgement differs")?;
        self.history = Some(history);
        self.branch = branch;
        Ok(current)
    }
}

pub(crate) fn execute(
    request: &WorkflowRemoteConfirmV9,
    provider: &GithubProvider,
    database: &Path,
    drafts: &DraftClient,
    fence: DraftPublicationFenceV2,
    lock: &WorkflowMutationLockV9<'_>,
) -> GithubResult<WorkflowRemoteExecutionV9> {
    let cancel = request.cancellation();
    let deadline = request.deadline().min(fence.deadline());
    shared::clock(cancel, deadline)?;
    let preview = request.preview();
    let selection = preview.selection();
    let committed = selection.committed();
    let opened = committed.staged().saved().opened();
    let intent = opened.intent();
    let source = intent.source();
    request.validate_opened_v9(database)?;
    request.check_original_v9(opened.binding())?;
    let core_policy = WorkflowCoreJsonV9::for_confirm_v9(provider, request, cancel, deadline)?;
    let phase = provider.clone().with_catalogue_read_deadline(deadline);
    let mut session = phase.begin_workflow_edit_access_v9(
        source.original().source(),
        &shared::access_limits(),
        &core_policy,
        cancel,
    )?;
    shared::scope(source, session.identity())?;
    let identity = session.identity().clone();
    session.verify(source.reference(), cancel)?;
    let (window, post) = session.reserve_workflow_edit_transport_v9(
        intent.original(),
        preview.phase(),
        1,
        cancel,
    )?;
    let post = post.ok_or_else(safe)?;
    let effective = post.deadline();
    if effective != window.deadline() || effective > deadline {
        return Err(safe());
    }
    request.check_material_v9(
        post.reserved_bytes()
            .checked_add(1024 * 1024)
            .ok_or_else(safe)?,
        4 * 1024 * 1024,
    )?;
    let measured = phase.clone().with_measured_read_window(window)?;
    let json_heap = core_policy.bind_measured_v9(&measured, cancel)?;
    let models_heap = json_heap.reserve_model_v9(1024 * 1024)?;
    let current = session
        .observed_original_v1(source.reference().id)
        .ok_or_else(safe)?;
    if let Some(pr) = source.pr_origin_v2() {
        pr.fresh_workflow_edit_parent_v9(&measured, &identity, current, &json_heap, cancel)?;
    }
    let first = measured.scoped_actions_workflow_snapshot_heap_v9(
        current,
        intent.original(),
        true,
        &json_heap,
        cancel,
    )?;
    if !first.same_pin_v6(intent.original(), true) {
        return Err(safe());
    }
    let repositories = Arc::new(measured.workflow_remote_repositories_v9(
        intent,
        selection.member(),
        &json_heap,
        cancel,
    )?);
    if !preview.repositories().same_observation_v9(&repositories) {
        return Err(safe());
    }
    let repository = opened
        .binding()
        .current_repository_v9(measured.remote_git_read_window_v1(cancel)?, cancel)
        .map_err(|_| safe())?;
    let git = WorkflowRemoteGitObservationV9::observe(
        committed,
        &repository,
        repositories.head(),
        selection.remote_name(),
        preview.phase(),
    )
    .map_err(|_| safe())?;
    if !preview.git().same_observation(&git) {
        return Err(safe());
    }
    let refs = Arc::new(measured.workflow_remote_refs_v9(
        &repositories,
        committed,
        selection.base_branch(),
        preview.phase(),
        &json_heap,
        cancel,
    )?);
    if !preview.refs().same_observation_v9(&refs) {
        return Err(safe());
    }
    let mut payload_heap = None;
    let payload = if preview.phase() == WorkflowEditPhaseV9::CreatePr {
        let values = committed.staged().saved().values();
        let bound = values
            .pr_title()
            .len()
            .checked_add(values.pr_body().len())
            .and_then(|n| n.checked_mul(10))
            .and_then(|n| n.checked_add(68 * 1024))
            .ok_or_else(safe)?;
        request.check_material_v9(
            bound
                .checked_add(post.reserved_bytes())
                .and_then(|n| n.checked_add(1024 * 1024))
                .ok_or_else(safe)?,
            4 * 1024 * 1024,
        )?;
        payload_heap = Some(json_heap.reserve_model_v9(bound)?);
        let payload = WorkflowPrPayloadV9::prepare(&refs, values)?;
        if payload.owned_bytes_v9() > bound {
            return Err(safe());
        }
        payload.verify_duplicates_v9(&measured, &json_heap, cancel)?;
        Some(payload)
    } else {
        None
    };
    let descriptor = refs.publication_descriptor_v9(&git, payload.as_ref())?;
    let push = if preview.phase() == WorkflowEditPhaseV9::PublishBranch {
        Some(WorkflowPreparedPushV9::prepare_v9(&git)?)
    } else {
        None
    };
    let material = repositories
        .owned_bytes_v9()
        .checked_add(refs.owned_bytes_v9())
        .and_then(|n| n.checked_add(git.owned_bytes_v9()))
        .and_then(|n| n.checked_add(payload.as_ref().map_or(0, |p| p.owned_bytes_v9())))
        .and_then(|n| n.checked_add(push.as_ref().map_or(0, |p| p.retained_bytes_v9())))
        .and_then(|n| n.checked_add(post.reserved_bytes()))
        .and_then(|n| n.checked_add(128 * 1024))
        .ok_or_else(safe)?;
    request.check_material_v9(material, 4 * 1024 * 1024)?;
    let journal_heap = json_heap.reserve_model_v9(512 * 1024)?;
    let mut journal = ActionsWriteJournal::open(database).map_err(|_| safe())?;
    let ack = RefCell::new(None);
    let encoded = RefCell::new(None);
    let mut policy = PolicyV9 {
        cleanup_failure: None,
        request,
        lock,
        original: &phase,
        measured,
        session: Some(session),
        post,
        identity,
        repositories,
        refs,
        git,
        push,
        payload,
        database,
        drafts,
        ack: &ack,
        encoded: &encoded,
        claim: None,
        result: None,
        document: None,
        history: None,
        material,
        effective,
        closed: false,
        branch: None,
        json_heap,
        _models_heap: models_heap,
        _payload_heap: payload_heap,
        _journal_heap: journal_heap,
        document_heap: None,
    };
    let execution = remote_transaction_v3::execute(
        &mut journal,
        &descriptor,
        |marker| {
            shared::clock(cancel, effective)
                .map_err(|_| "Original workflow remote attempt ended")?;
            let body=serde_json::to_string(&serde_json::json!({"version":9,"purpose":"scoped-actions-workflow-remote-v9","actions_publication":marker.publication_reference()})).map_err(|_|"Original workflow reference encoding unavailable")?;
            if body.len() > 16 * 1024 {
                return Err("Original workflow reference exceeds bound".into());
            }
            let saved = drafts
                .save_publication_and_wait_v2(fence, body.clone(), cancel)
                .map_err(|_| "Original workflow ordered metadata save failed; pending retained")?;
            saved
                .check_current(
                    drafts,
                    database,
                    source.original().scope(),
                    intent.key(),
                    cancel,
                )
                .map_err(|_| "Original workflow ordered metadata changed")?;
            *ack.borrow_mut() = Some(saved);
            *encoded.borrow_mut() = Some(body);
            shared::clock(cancel, effective)
                .map_err(|_| "Original workflow remote attempt ended".into())
        },
        &mut policy,
    );
    let visible_confirmed = matches!(execution, ActionsExecution::Confirmed { .. });
    Ok(WorkflowRemoteExecutionV9 {
        cleanup_failure: policy.cleanup_failure,
        preview: preview.clone(),
        execution,
        history: if visible_confirmed {
            policy.history.take()
        } else {
            None
        },
        document: if visible_confirmed {
            policy.document.take()
        } else {
            None
        },
        branch: if visible_confirmed {
            policy.branch.take()
        } else {
            None
        },
    })
}
