//! W9-only exact draft cleanup on the existing publication queue. A remote
//! receipt, saved reference, or final UID alone cannot acknowledge local SQL.
use super::*;
use crate::{
    actions_write_journal::{ActionsClaimStampV3, ActionsOwnedWorkflowRemoteHistoryV9},
    providers::gh::{
        WorkflowCleanupSealV9, WorkflowEditPhaseV9, WorkflowJsonChargeV9, WorkflowJsonLedgerV9,
    },
    scoped_actions_workflow_edit_v9::{WorkflowRemoteConfirmV9, WorkflowRemotePreviewV9},
    storage::workflow_cleanup_v9::{CommittedWorkflowCleanupV9, PreparedWorkflowCleanupV9},
};
use std::sync::Weak;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkflowCleanupFailureV9 {
    Retained,
    Unknown,
}
impl WorkflowCleanupFailureV9 {
    fn current_negative_reply_v9(self, cancel: &CancellationToken, deadline: Instant) -> Self {
        if cancel.is_cancelled() || Instant::now() >= deadline {
            Self::Unknown
        } else {
            self
        }
    }
}
impl fmt::Display for WorkflowCleanupFailureV9 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Retained => "Cleanup NOT applied • receipt protected • inspect; NEVER resend",
            Self::Unknown => "Cleanup ACK UNKNOWN • receipt protected • inspect; NEVER resend",
        })
    }
}
impl std::error::Error for WorkflowCleanupFailureV9 {}
pub(crate) enum WorkflowCleanupOutcomeV9 {
    Removed(CommittedWorkflowCleanupV9),
    Retained,
    Unknown,
}
pub(crate) struct WorkflowCleanupBindingV9 {
    producer: Weak<DraftProducerV4>,
    ticket: u64,
    revision: u64,
    epoch: u64,
    database: PathBuf,
    scope: Scope,
    resource: String,
    body: String,
    preview: Arc<WorkflowRemotePreviewV9>,
    history: Arc<ActionsOwnedWorkflowRemoteHistoryV9>,
    cancel: CancellationToken,
    effective: Instant,
    _retained_bytes: usize,
    _retained_charge: WorkflowJsonChargeV9,
}
impl WorkflowCleanupBindingV9 {
    pub(crate) fn database_v9(&self) -> &Path {
        &self.database
    }
    pub(crate) fn resource_v9(&self) -> &str {
        &self.resource
    }
    pub(crate) fn scope_v9(&self) -> &Scope {
        &self.scope
    }
    pub(crate) fn body_v9(&self) -> &str {
        &self.body
    }
    pub(crate) fn effective_v9(&self) -> Instant {
        self.effective
    }
    pub(crate) fn phase_v9(&self) -> WorkflowEditPhaseV9 {
        self.preview.phase()
    }
    pub(crate) fn check_request_v9(
        &self,
        request: &WorkflowRemoteConfirmV9,
        history: &Arc<ActionsOwnedWorkflowRemoteHistoryV9>,
        cancel: &CancellationToken,
        effective: Instant,
    ) -> anyhow::Result<()> {
        if !Arc::ptr_eq(request.preview(), &self.preview)
            || !Arc::ptr_eq(history, &self.history)
            || !self.cancel.same_instance(cancel)
            || !request.cancellation().same_instance(cancel)
            || effective != self.effective
            || effective > request.deadline()
            || cancel.is_cancelled()
            || Instant::now() >= effective
        {
            anyhow::bail!("W9 cleanup belongs to another actual request/proof/token/clock");
        }
        Ok(())
    }
    fn matches_fence(&self, fence: &DraftPublicationFenceV2, entered: bool) -> bool {
        self.producer.ptr_eq(&Arc::downgrade(&fence.producer))
            && self.ticket == fence.snapshot.ticket
            && self.revision == fence.snapshot.revision
            && self.epoch.checked_add(u64::from(entered)) == Some(fence.snapshot.epoch)
            && self.database == fence.database
            && self.scope == fence.snapshot.scope
            && self.resource == fence.snapshot.resource
            && matches!(fence.body_profile_v9, DraftBodyProfileV9::WorkflowEdit361)
            && fence.deadline <= self.effective
            && self.cancel.same_instance(&fence.cancel)
    }
    pub(crate) fn matches_ack_v9(&self, ack: &DraftPublicationAckV2) -> bool {
        // Before narrowing at enqueue, the original ACK may have a wider D.
        self.producer.ptr_eq(&Arc::downgrade(&ack.fence.producer))
            && self.ticket == ack.fence.snapshot.ticket
            && self.revision == ack.fence.snapshot.revision
            && self.epoch == ack.fence.snapshot.epoch
            && self.database == ack.fence.database
            && self.scope == ack.fence.snapshot.scope
            && self.resource == ack.fence.snapshot.resource
            && self.body == ack.saved_body
            && matches!(
                ack.fence.body_profile_v9,
                DraftBodyProfileV9::WorkflowEdit361
            )
            && self.cancel.same_instance(&ack.fence.cancel)
    }
}
impl DraftPublicationAckV2 {
    pub(crate) fn prepare_workflow_cleanup_binding_v9(
        &self,
        client: &DraftClient,
        input: WorkflowCleanupInputV9<'_>,
    ) -> Result<WorkflowCleanupBindingV9, WorkflowCleanupFailureV9> {
        let WorkflowCleanupInputV9 {
            request,
            history,
            claim,
            encoded,
            heap,
            cancel,
            effective,
        } = input;
        let opened = request
            .preview()
            .selection()
            .committed()
            .staged()
            .saved()
            .opened();
        let scope = opened.intent().source().original().scope();
        self.check_current(
            client,
            &self.fence.database,
            scope,
            opened.intent().key(),
            cancel,
        )
        .map_err(|_| WorkflowCleanupFailureV9::Retained)?;
        let bytes = std::mem::size_of::<WorkflowCleanupBindingV9>()
            .checked_add(64 * 1024)
            .and_then(|n| n.checked_add(self.saved_body.capacity()))
            .and_then(|n| n.checked_add(encoded.capacity()))
            .and_then(|n| n.checked_add(self.fence.database.capacity()))
            .and_then(|n| n.checked_add(self.fence.snapshot.resource.capacity()))
            .and_then(|n| n.checked_add(scope.host.capacity() + scope.login.capacity()))
            .ok_or(WorkflowCleanupFailureV9::Retained)?;
        let charge = heap
            .reserve_model_v9(bytes)
            .map_err(|_| WorkflowCleanupFailureV9::Retained)?;
        if !matches!(
            self.fence.body_profile_v9,
            DraftBodyProfileV9::WorkflowEdit361
        ) || encoded.len() > 16 * 1024
            || self.saved_body != encoded
            || !history.binds_cleanup_claim_v9(request, claim)
            || effective > request.deadline()
            || effective > self.fence.deadline
            || cancel.is_cancelled()
            || Instant::now() >= effective
        {
            return Err(WorkflowCleanupFailureV9::Retained);
        }
        let canonical=serde_json::to_string(&serde_json::json!({"version":9,"purpose":"scoped-actions-workflow-remote-v9","actions_publication":history.reference()})).map_err(|_|WorkflowCleanupFailureV9::Retained)?;
        if encoded != canonical {
            return Err(WorkflowCleanupFailureV9::Retained);
        }
        Ok(WorkflowCleanupBindingV9 {
            producer: Arc::downgrade(&self.fence.producer),
            ticket: self.fence.snapshot.ticket,
            revision: self.fence.snapshot.revision,
            epoch: self.fence.snapshot.epoch,
            database: self.fence.database.clone(),
            scope: scope.clone(),
            resource: self.fence.snapshot.resource.clone(),
            body: encoded,
            preview: request.preview().clone(),
            history: history.clone(),
            cancel: cancel.clone(),
            effective,
            _retained_bytes: bytes,
            _retained_charge: charge,
        })
    }
}
pub(crate) struct WorkflowCleanupInputV9<'a> {
    pub(crate) request: &'a WorkflowRemoteConfirmV9,
    pub(crate) history: &'a Arc<ActionsOwnedWorkflowRemoteHistoryV9>,
    pub(crate) claim: &'a Arc<ActionsClaimStampV3>,
    pub(crate) encoded: String,
    pub(crate) heap: &'a WorkflowJsonLedgerV9,
    pub(crate) cancel: &'a CancellationToken,
    pub(crate) effective: Instant,
}
pub(super) struct WorkflowCleanupPayloadV9 {
    prepared: Option<PreparedWorkflowCleanupV9>,
    seal: Option<WorkflowCleanupSealV9>,
    body: String,
    _retained_charge: Option<WorkflowJsonChargeV9>,
}
pub(super) struct WorkflowCleanupReplyV9 {
    committed: CommittedWorkflowCleanupV9,
    ack: DraftPublicationAckV2,
    _retained_charge: WorkflowJsonChargeV9,
}
pub(crate) struct PreparedWorkflowCleanupMessageV9 {
    fence: DraftPublicationFenceV2,
    payload: Box<WorkflowCleanupPayloadV9>,
    reply: SyncSender<Result<WorkflowCleanupReplyV9, WorkflowCleanupFailureV9>>,
    result: Receiver<Result<WorkflowCleanupReplyV9, WorkflowCleanupFailureV9>>,
}
impl PreparedWorkflowCleanupMessageV9 {
    pub(crate) fn matches_cursor_v9(
        &self,
        original: &Arc<crate::providers::gh::ScopedActionsWorkflowV2>,
        phase: WorkflowEditPhaseV9,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> bool {
        self.payload
            .prepared
            .as_ref()
            .is_some_and(|prepared| prepared.matches_cursor_v9(original, phase, cancel, deadline))
    }
    pub(crate) fn ticket_v9(&self) -> Option<&Arc<()>> {
        self.payload
            .prepared
            .as_ref()
            .map(PreparedWorkflowCleanupV9::ticket_v9)
    }
    pub(crate) fn effective_v9(&self) -> Instant {
        self.fence.deadline
    }
    pub(crate) fn close_v9(mut self, seal: WorkflowCleanupSealV9) -> ClosedWorkflowCleanupV9 {
        self.payload.seal = Some(seal);
        ClosedWorkflowCleanupV9 { message: self }
    }
}
pub(crate) struct ClosedWorkflowCleanupV9 {
    message: PreparedWorkflowCleanupMessageV9,
}
pub(crate) struct AcknowledgedWorkflowCleanupV9<'a> {
    fence: &'a DraftPublicationFenceV2,
    body: &'a str,
}
impl AcknowledgedWorkflowCleanupV9<'_> {
    pub(crate) fn body_v9(&self) -> &str {
        self.body
    }
    pub(crate) fn matches_binding_v9(&self, binding: &WorkflowCleanupBindingV9) -> bool {
        binding.matches_fence(self.fence, true) && binding.body == self.body
    }
    pub(crate) fn check_current_v9(&self) -> Result<(), String> {
        self.fence.clock(&self.fence.cancel)?;
        let state = self.fence.producer.lock()?;
        self.fence
            .producer
            .publication_current_v2(&state, &self.fence.snapshot, true)
    }
}
pub(super) fn run_workflow_cleanup_v9(
    fence: DraftPublicationFenceV2,
    mut data: WorkflowCleanupPayloadV9,
    activity: LegacyActivityV4,
    reply: SyncSender<Result<WorkflowCleanupReplyV9, WorkflowCleanupFailureV9>>,
    cache: &Result<Cache, String>,
) {
    let cleanup = AcknowledgedWorkflowCleanupV9 {
        fence: &fence,
        body: &data.body,
    };
    let outcome = match (cache, data.seal.as_ref(), data.prepared.take()) {
        (Ok(cache), Some(seal), Some(prepared)) => {
            prepared.commit_in_writer_v9(cache, seal, &cleanup)
        }
        _ => Err(WorkflowCleanupFailureV9::Retained),
    };
    let current = cleanup.check_current_v9();
    drop(activity);
    let outcome = outcome.and_then(|committed| {
        current.map_err(|_| WorkflowCleanupFailureV9::Unknown)?;
        let charge = data
            ._retained_charge
            .take()
            .ok_or(WorkflowCleanupFailureV9::Unknown)?;
        Ok(WorkflowCleanupReplyV9 {
            committed,
            ack: DraftPublicationAckV2 {
                fence,
                saved_body: std::mem::take(&mut data.body),
            },
            _retained_charge: charge,
        })
    });
    let _ = reply.try_send(outcome);
}
impl DraftClient {
    pub(crate) fn prepare_workflow_cleanup_message_v9(
        &self,
        ack: DraftPublicationAckV2,
        prepared: PreparedWorkflowCleanupV9,
        heap: &WorkflowJsonLedgerV9,
        cancel: &CancellationToken,
    ) -> Result<PreparedWorkflowCleanupMessageV9, WorkflowCleanupFailureV9> {
        ack.check_current(
            self,
            &ack.fence.database,
            &ack.fence.snapshot.scope,
            &ack.fence.snapshot.resource,
            cancel,
        )
        .map_err(|_| WorkflowCleanupFailureV9::Retained)?;
        if !prepared.matches_ack_v9(&ack) {
            return Err(WorkflowCleanupFailureV9::Retained);
        }
        // Both Box layouts, bounded sync channel/reply payload and moving
        // envelope headers are charged BEFORE UID. Existing outer Box stays at
        // its unchanged publication_enter callsite; no unsafe initialization.
        let bytes = 2
            * (std::alloc::Layout::new::<PublicationMessageV2>().size()
                + std::alloc::Layout::new::<WorkflowCleanupPayloadV9>().size()
                + std::mem::size_of::<PreparedWorkflowCleanupMessageV9>()
                + std::mem::size_of::<CommittedWorkflowCleanupV9>()
                + std::alloc::Layout::new::<WorkflowCleanupReplyV9>().size()
                + std::alloc::Layout::new::<WorkflowCleanupSealV9>().size())
            + 4096
            // Fixed observed-wait/cancel caption overlap; the Confirm's
            // actual AtomicBool layout is in the existing size_of header.
            + 512
            + ack.saved_body.capacity();
        let charge = heap
            .reserve_model_v9(bytes)
            .map_err(|_| WorkflowCleanupFailureV9::Retained)?;
        let (reply, result) = mpsc::sync_channel(1);
        let DraftPublicationAckV2 {
            mut fence,
            saved_body,
        } = ack;
        fence.deadline = fence.deadline.min(prepared.effective_v9());
        Ok(PreparedWorkflowCleanupMessageV9 {
            fence,
            payload: Box::new(WorkflowCleanupPayloadV9 {
                prepared: Some(prepared),
                seal: None,
                body: saved_body,
                _retained_charge: Some(charge),
            }),
            reply,
            result,
        })
    }
    pub(crate) fn commit_workflow_cleanup_v9(
        &self,
        closed: ClosedWorkflowCleanupV9,
        request: &WorkflowRemoteConfirmV9,
        cancel: &CancellationToken,
    ) -> WorkflowCleanupOutcomeV9 {
        let PreparedWorkflowCleanupMessageV9 {
            fence,
            payload,
            reply,
            result,
        } = closed.message;
        if payload.seal.is_none()
            || !request.cancellation().same_instance(cancel)
            || payload
                .prepared
                .as_ref()
                .is_none_or(|prepared| !prepared.matches_request_v9(request))
        {
            return WorkflowCleanupOutcomeV9::Retained;
        }
        let (token, deadline) = match self.enqueue_publication_v85(
            fence,
            PublicationEffectV2::CommitWorkflowCleanupV9(payload),
            PublicationReplyV85::WorkflowCleanupV9(reply),
            cancel,
        ) {
            Ok(current) => current,
            Err(_) => return WorkflowCleanupOutcomeV9::Retained,
        };
        // No observation before actual successful TrySend; no authority or
        // heap allocation. The same Confirm owns the flag throughout waiting.
        let _waiting = request.observe_accepted_cleanup_wait_v9();
        loop {
            if token.is_cancelled() || Instant::now() >= deadline {
                return WorkflowCleanupOutcomeV9::Unknown;
            }
            #[cfg(test)]
            actual_queue_tests_v9::before_actual_receive_v9();
            match result.recv_timeout(
                deadline
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(20)),
            ) {
                Ok(Ok(done)) => {
                    if token.is_cancelled() || Instant::now() >= deadline {
                        return WorkflowCleanupOutcomeV9::Unknown;
                    }
                    if done
                        .ack
                        .check_current(
                            self,
                            &done.ack.fence.database,
                            &done.ack.fence.snapshot.scope,
                            &done.ack.fence.snapshot.resource,
                            cancel,
                        )
                        .is_err()
                    {
                        return WorkflowCleanupOutcomeV9::Unknown;
                    }
                    return WorkflowCleanupOutcomeV9::Removed(done.committed);
                }
                Ok(Err(failure)) => {
                    // A known worker rollback is not a current retention
                    // acknowledgement once this original caller has ended.
                    return match failure.current_negative_reply_v9(&token, deadline) {
                        WorkflowCleanupFailureV9::Retained => WorkflowCleanupOutcomeV9::Retained,
                        WorkflowCleanupFailureV9::Unknown => WorkflowCleanupOutcomeV9::Unknown,
                    };
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return WorkflowCleanupOutcomeV9::Unknown
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    }
}

#[cfg(test)]
#[path = "workflow_cleanup_actual_v9_tests.rs"]
mod actual_queue_tests_v9;

#[cfg(test)]
mod reply_clock_tests_v9 {
    use super::*;
    // Pure classification is not a fabricated cleanup seal/transaction and
    // does not replace the real accepted-noACK worker/native contracts.
    #[test]
    fn task54_workflow_edit_v9_cleanup_negative_reply_after_original_cancel_is_unknown() {
        let token = CancellationToken::default();
        let deadline = Instant::now() + Duration::from_secs(30);
        assert_eq!(
            WorkflowCleanupFailureV9::Retained.current_negative_reply_v9(&token, deadline),
            WorkflowCleanupFailureV9::Retained
        );
        token.cancel();
        assert_eq!(
            WorkflowCleanupFailureV9::Retained.current_negative_reply_v9(&token, deadline),
            WorkflowCleanupFailureV9::Unknown
        );
        assert_eq!(
            WorkflowCleanupFailureV9::Unknown.current_negative_reply_v9(&token, deadline),
            WorkflowCleanupFailureV9::Unknown
        );
    }
    #[test]
    fn task54_workflow_edit_v9_cleanup_negative_reply_after_original_deadline_is_unknown() {
        let token = CancellationToken::default();
        let deadline = Instant::now();
        assert_eq!(
            WorkflowCleanupFailureV9::Retained.current_negative_reply_v9(&token, deadline),
            WorkflowCleanupFailureV9::Unknown
        );
        assert!(!token.is_cancelled());
    }
}
