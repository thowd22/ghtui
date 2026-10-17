//! Closed same-queue atomic native publication. No Source, receipt, SQL
//! callback, new capture or renewed clock can be constructed here.
use super::*;
use crate::{
    pr_write_journal::{
        conversation_refresh_v19::ConversationTerminalWitnessV19, PrPublicationReference,
    },
    providers::gh::ConversationRefreshCloseSealV19,
    storage::conversation_refresh_v19::{
        CommittedConversationProjectionV19, PreparedConversationCommitV19,
    },
};
use std::sync::Weak;

#[derive(Debug)]
pub(crate) enum AtomicPublicationFailureV85 {
    Retained(String),
    Unknown(String),
}
impl AtomicPublicationFailureV85 {
    pub(crate) fn unknown(&self) -> bool {
        matches!(self, Self::Unknown(_))
    }
}
impl fmt::Display for AtomicPublicationFailureV85 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Retained(s) | Self::Unknown(s) => f.write_str(s),
        }
    }
}
pub(crate) struct AtomicDraftInputV85<'a> {
    pub(crate) database: &'a Path,
    pub(crate) scope: &'a Scope,
    pub(crate) resource: &'a str,
    // Capacity, not just length, participates in the before-copy live bound.
    pub(crate) encoded: &'a String,
    pub(crate) reference: &'a PrPublicationReference,
    pub(crate) terminal: &'a Arc<ConversationTerminalWitnessV19>,
}
pub(crate) struct AtomicCleanupBindingV85 {
    producer: Weak<DraftProducerV4>,
    ticket: u64,
    revision: u64,
    epoch: u64,
    terminal: Arc<ConversationTerminalWitnessV19>,
    bytes: usize,
}
impl AtomicCleanupBindingV85 {
    pub(crate) fn retained_bytes_v85(&self) -> usize {
        self.bytes
    }
    pub(crate) fn matches_terminal_v85(
        &self,
        terminal: &Arc<ConversationTerminalWitnessV19>,
    ) -> bool {
        Arc::ptr_eq(&self.terminal, terminal)
    }
    fn matches_fence(&self, fence: &DraftPublicationFenceV2, entered: bool) -> bool {
        self.producer.ptr_eq(&Arc::downgrade(&fence.producer))
            && self.ticket == fence.snapshot.ticket
            && self.revision == fence.snapshot.revision
            && self.epoch.checked_add(u64::from(entered)) == Some(fence.snapshot.epoch)
            && self.terminal.database() == fence.database
            && self.terminal.target().scope() == &fence.snapshot.scope
            && self
                .terminal
                .matches_draft_resource_v86(&fence.snapshot.resource)
    }
    pub(crate) fn matches_ack_v85(&self, ack: &DraftPublicationAckV2) -> bool {
        self.matches_fence(&ack.fence, false)
    }
}
impl DraftPublicationAckV2 {
    pub(crate) fn atomic_charge_v85(
        &self,
        client: &DraftClient,
        input: AtomicDraftInputV85<'_>,
        cancel: &CancellationToken,
    ) -> Result<AtomicCleanupBindingV85, DraftPublicationErrorV2> {
        self.check_current(client, input.database, input.scope, input.resource, cancel)
            .map_err(DraftPublicationErrorV2::NotSubmitted)?;
        if self.saved_body.len() > 128 * 1024
            || input.encoded.len() > 128 * 1024
            || self.saved_body != *input.encoded
            || input.terminal.database() != input.database
            || input.terminal.target().scope() != input.scope
            || input.terminal.reference() != input.reference
            || !input.terminal.matches_draft_resource_v86(input.resource)
        {
            return Err(DraftPublicationErrorV2::NotSubmitted(
                "Actual native terminal/ACK/encoded draft binding differs".into(),
            ));
        }
        // Both existing String allocations remain live during preparation.
        // Fixed structs can coexist at old/new move locations and in the Box;
        // no allocator/RSS claim and no body/plan clone is discounted.
        let fixed = [
            std::mem::size_of::<AtomicCleanupBindingV85>(),
            std::mem::size_of::<AtomicProjectionV85>(),
            std::mem::size_of::<PreparedConversationCommitV19>(),
            std::mem::size_of::<ConversationRefreshCloseSealV19>(),
            std::mem::size_of::<PublicationMessageV2>(),
            std::mem::size_of::<PublicationReplyV85>(),
            std::mem::size_of::<AtomicCommitReplyV85>(),
            std::mem::size_of::<DraftPublicationFenceV2>(),
        ]
        .into_iter()
        .try_fold(0usize, |n, x| n.checked_add(x.checked_mul(2)?));
        let bytes = fixed
            .and_then(|n| n.checked_add(4 * std::mem::size_of::<usize>()))
            .and_then(|n| n.checked_add(self.saved_body.capacity()))
            .and_then(|n| n.checked_add(input.encoded.capacity()))
            .and_then(|n| n.checked_add(self.fence.database.capacity()))
            .and_then(|n| n.checked_add(self.fence.snapshot.scope.host.capacity()))
            .and_then(|n| n.checked_add(self.fence.snapshot.scope.login.capacity()))
            .and_then(|n| n.checked_add(self.fence.snapshot.resource.capacity()))
            .filter(|n| *n <= 8 * 1024 * 1024)
            .ok_or_else(|| {
                DraftPublicationErrorV2::NotSubmitted(
                    "Atomic publication allocation bound exceeded before copy".into(),
                )
            })?;
        Ok(AtomicCleanupBindingV85 {
            producer: Arc::downgrade(&self.fence.producer),
            ticket: self.fence.snapshot.ticket,
            revision: self.fence.snapshot.revision,
            epoch: self.fence.snapshot.epoch,
            terminal: Arc::clone(input.terminal),
            bytes,
        })
    }
}

pub(super) struct AtomicProjectionV85 {
    prepared: PreparedConversationCommitV19,
    seal: ConversationRefreshCloseSealV19,
    body: String,
}
pub(super) struct AtomicCommitReplyV85 {
    committed: CommittedConversationProjectionV19,
    ack: DraftPublicationAckV2,
}
// Only this actual message worker can create the borrowed transaction proof.
// It never crosses the channel and exposes only fixed checks/one fixed DELETE.
pub(crate) struct AcknowledgedProjectionCleanupV85<'a> {
    fence: &'a DraftPublicationFenceV2,
    body: &'a str,
}
impl AcknowledgedProjectionCleanupV85<'_> {
    pub(crate) fn matches_binding_v85(&self, binding: &AtomicCleanupBindingV85) -> bool {
        binding.matches_fence(self.fence, true)
    }
    pub(crate) fn check_current_v85(&self) -> Result<(), String> {
        self.fence.clock(&self.fence.cancel)?;
        let state = self.fence.producer.lock()?;
        self.fence
            .producer
            .publication_current_v2(&state, &self.fence.snapshot, true)
    }
    pub(crate) fn deadline_v85(&self) -> Instant {
        self.fence.deadline
    }
    pub(crate) fn same_token_v85(&self, cancel: &CancellationToken) -> bool {
        self.fence.cancel.same_instance(cancel)
    }
    pub(crate) fn delete_in_v85(&self, tx: &rusqlite::Transaction<'_>) -> anyhow::Result<()> {
        Cache::remove_publication_draft_exact_connection_v85(
            tx,
            &self.fence.snapshot.scope,
            &self.fence.snapshot.resource,
            self.body,
        )
    }
}

pub(super) fn run_atomic_v85(
    fence: DraftPublicationFenceV2,
    data: AtomicProjectionV85,
    activity: LegacyActivityV4,
    reply: SyncSender<Result<AtomicCommitReplyV85, AtomicPublicationFailureV85>>,
) {
    let AtomicProjectionV85 {
        prepared,
        seal,
        body,
    } = data;
    let cleanup = AcknowledgedProjectionCleanupV85 {
        fence: &fence,
        body: &body,
    };
    let result = prepared.commit_publication_v85(seal, &cleanup, &fence.cancel);
    // The final SQL transaction is already all-or-nothing. Currentness lost
    // after its last short check is UNKNOWN, never a rollback/retention claim.
    let current = cleanup.check_current_v85();
    drop(activity);
    let result = match result {
        Ok(committed) => match current {
            Ok(()) => Ok(AtomicCommitReplyV85 {
                committed,
                ack: DraftPublicationAckV2 {
                    fence,
                    saved_body: body,
                },
            }),
            Err(e) => Err(AtomicPublicationFailureV85::Unknown(e)),
        },
        Err(e) => Err(e),
    };
    let _ = reply.try_send(result);
}

impl DraftClient {
    pub(crate) fn commit_conversation_publication_v85(
        &self,
        mut ack: DraftPublicationAckV2,
        prepared: PreparedConversationCommitV19,
        seal: ConversationRefreshCloseSealV19,
        cancel: &CancellationToken,
        cutoff: Instant,
    ) -> Result<CommittedConversationProjectionV19, AtomicPublicationFailureV85> {
        ack.check_current(
            self,
            &ack.fence.database,
            &ack.fence.snapshot.scope,
            &ack.fence.snapshot.resource,
            cancel,
        )
        .map_err(AtomicPublicationFailureV85::Retained)?;
        if !prepared.matches_atomic_ack_v85(&ack) {
            return Err(AtomicPublicationFailureV85::Retained(
                "Prepared projection belongs to another actual ACK".into(),
            ));
        }
        ack.fence.deadline = ack
            .fence
            .deadline
            .min(cutoff)
            .min(prepared.atomic_cutoff_v85())
            .min(seal.cutoff_v51());
        let (reply, result) = mpsc::sync_channel(1);
        let effect = PublicationEffectV2::CommitConversation(Box::new(AtomicProjectionV85 {
            prepared,
            seal,
            body: ack.saved_body,
        }));
        let (token, deadline) = self
            .enqueue_publication_v85(
                ack.fence,
                effect,
                PublicationReplyV85::Atomic(reply),
                cancel,
            )
            .map_err(|e| AtomicPublicationFailureV85::Retained(e.to_string()))?;
        loop {
            if token.is_cancelled() || Instant::now() >= deadline {
                return Err(AtomicPublicationFailureV85::Unknown("Atomic publication accepted; completion not acknowledged, inspect durable state; never resend".into()));
            }
            match result.recv_timeout(deadline.saturating_duration_since(Instant::now()).min(Duration::from_millis(20))) {
                Ok(Ok(done))=> {
                    done.ack.check_current(self,&done.ack.fence.database,&done.ack.fence.snapshot.scope,
                        &done.ack.fence.snapshot.resource,cancel).map_err(AtomicPublicationFailureV85::Unknown)?;
                    return Ok(done.committed);
                }
                Ok(Err(e))=> {
                    // The original clock can end while recv_timeout blocks.
                    // Even a genuine worker refusal is not a current caller
                    // acknowledgement after cancellation/deadline expiry.
                    if token.is_cancelled() || Instant::now() >= deadline {
                        return Err(AtomicPublicationFailureV85::Unknown(
                            "Accepted atomic publication reply arrived after original token/deadline; inspect durable state; never resend".into()));
                    }
                    return Err(e);
                }
                Err(mpsc::RecvTimeoutError::Timeout)=>{},
                Err(mpsc::RecvTimeoutError::Disconnected)=>return Err(AtomicPublicationFailureV85::Unknown(
                    "Accepted atomic publication reply lost; inspect durable state; never resend".into())),
            }
        }
    }
}
#[cfg(test)]
#[path = "atomic_publication_v85_tests.rs"]
mod tests;
