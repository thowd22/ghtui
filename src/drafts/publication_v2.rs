//! Bounded publication-only enqueue/ACK on the existing DraftClient queue.
//! Not storage/source/account/write authority. Accepted messages are NEVER
//! resent, and completion owns activity even after the caller times out.
use super::corpus_preview_v4::LegacyActivityV4;
use super::*;
use crate::providers::runner::CancellationToken;
use std::{
    fmt,
    path::Path,
    time::{Duration, Instant},
};
#[path = "atomic_publication_v85.rs"]
mod atomic_v85;
#[path = "workflow_cleanup_v9.rs"]
mod workflow_cleanup_v9;
pub(crate) use atomic_v85::{
    AcknowledgedProjectionCleanupV85, AtomicCleanupBindingV85, AtomicDraftInputV85,
    AtomicPublicationFailureV85,
};
pub(crate) use workflow_cleanup_v9::{
    AcknowledgedWorkflowCleanupV9, ClosedWorkflowCleanupV9, PreparedWorkflowCleanupMessageV9,
    WorkflowCleanupBindingV9, WorkflowCleanupFailureV9, WorkflowCleanupInputV9,
    WorkflowCleanupOutcomeV9,
};

pub(super) struct PublicationSnapshotV2 {
    pub(super) revision: u64,
    pub(super) epoch: u64,
    pub(super) ticket: u64,
    pub(super) scope: Scope,
    pub(super) resource: String,
    pub(super) observed_absence: bool,
}
pub struct DraftPublicationFenceV2 {
    producer: Arc<DraftProducerV4>,
    database: PathBuf,
    snapshot: PublicationSnapshotV2,
    cancel: CancellationToken,
    deadline: Instant,
    // This exact fixed profile travels through the SAME message/ACK; decoded
    // text and resource spelling can never select cleanup policy.
    body_profile_v9: DraftBodyProfileV9,
}
#[derive(Clone, Copy)]
enum DraftBodyProfileV9 {
    Standard128,
    WorkflowEdit361,
}
impl fmt::Debug for DraftPublicationFenceV2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DraftPublicationFenceV2")
            .field("revision", &self.snapshot.revision)
            .finish_non_exhaustive()
    }
}
#[derive(Debug)]
pub enum DraftPublicationErrorV2 {
    NotSubmitted(String),
    AcknowledgementUnknown(String),
    Storage(String),
}
impl DraftPublicationErrorV2 {
    pub fn acknowledgement_unknown(&self) -> bool {
        matches!(self, Self::AcknowledgementUnknown(_) | Self::Storage(_))
    }
}
impl fmt::Display for DraftPublicationErrorV2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::NotSubmitted(message)
            | Self::AcknowledgementUnknown(message)
            | Self::Storage(message) => message,
        };
        f.write_str(message)
    }
}
impl std::error::Error for DraftPublicationErrorV2 {}

pub struct DraftPublicationAckV2 {
    fence: DraftPublicationFenceV2,
    // SAME accepted bounded Save allocation, moved once; no body history,
    // replay, reconstructed reference or independent cleanup capability.
    saved_body: String,
}
impl fmt::Debug for DraftPublicationAckV2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DraftPublicationAckV2(<actual same-queue SQL ACK; not publication authority>)")
    }
}
impl DraftPublicationFenceV2 {
    fn clock(&self, cancel: &CancellationToken) -> Result<(), String> {
        if !self.cancel.same_instance(cancel) || cancel.is_cancelled() {
            return Err("Original publication draft token changed/cancelled".into());
        }
        if Instant::now() >= self.deadline {
            return Err(
                "Original publication deadline expired during draft acknowledgement".into(),
            );
        }
        Ok(())
    }
    fn quantum(&self) -> Duration {
        self.deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(20))
    }
    pub fn deadline(&self) -> Instant {
        self.deadline
    }
}
impl DraftPublicationAckV2 {
    pub fn check_current(
        &self,
        client: &DraftClient,
        database: &Path,
        scope: &Scope,
        resource: &str,
        cancel: &CancellationToken,
    ) -> Result<(), String> {
        self.fence.clock(cancel)?;
        if !Arc::ptr_eq(&self.fence.producer, &client.producer)
            || self.fence.database != database
            || self.fence.snapshot.scope != *scope
            || self.fence.snapshot.resource != resource
        {
            return Err(
                "Publication ACK belongs to another original client/database/account/resource"
                    .into(),
            );
        }
        let state = self.fence.producer.lock()?;
        self.fence
            .producer
            .publication_current_v2(&state, &self.fence.snapshot, false)
    }
}

enum PublicationEffectV2 {
    Save(String),
    Remove(String),
    CommitConversation(Box<atomic_v85::AtomicProjectionV85>),
    CommitWorkflowCleanupV9(Box<workflow_cleanup_v9::WorkflowCleanupPayloadV9>),
}
enum PublicationReplyV85 {
    Draft(SyncSender<Result<DraftPublicationAckV2, DraftPublicationErrorV2>>),
    Atomic(SyncSender<Result<atomic_v85::AtomicCommitReplyV85, AtomicPublicationFailureV85>>),
    WorkflowCleanupV9(
        SyncSender<Result<workflow_cleanup_v9::WorkflowCleanupReplyV9, WorkflowCleanupFailureV9>>,
    ),
}
pub(super) struct PublicationMessageV2 {
    fence: DraftPublicationFenceV2,
    effect: PublicationEffectV2,
    activity: LegacyActivityV4,
    reply: PublicationReplyV85,
}
impl PublicationMessageV2 {
    pub(super) fn run(self: Box<Self>, cache: &Result<Cache, String>) {
        let Self {
            fence,
            effect,
            activity,
            reply,
        } = *self;
        let (effect, reply) = match (effect, reply) {
            (
                PublicationEffectV2::CommitWorkflowCleanupV9(data),
                PublicationReplyV85::WorkflowCleanupV9(reply),
            ) => {
                return workflow_cleanup_v9::run_workflow_cleanup_v9(
                    fence, *data, activity, reply, cache,
                );
            }
            (PublicationEffectV2::CommitConversation(data), PublicationReplyV85::Atomic(reply)) => {
                return atomic_v85::run_atomic_v85(fence, *data, activity, reply);
            }
            (effect, PublicationReplyV85::Draft(reply)) => (effect, reply),
            (_, PublicationReplyV85::Atomic(reply)) => {
                drop(activity);
                let _ = reply.try_send(Err(AtomicPublicationFailureV85::Retained(
                    "Closed publication message/reply mismatch".into(),
                )));
                return;
            }
            (_, PublicationReplyV85::WorkflowCleanupV9(reply)) => {
                drop(activity);
                let _ = reply.try_send(Err(WorkflowCleanupFailureV9::Retained));
                return;
            }
        };
        let current = (|| {
            fence.clock(&fence.cancel)?;
            let state = fence.producer.lock()?;
            fence
                .producer
                .publication_current_v2(&state, &fence.snapshot, true)
        })();
        if let Err(error) = current {
            drop(activity); // actual queued skip, no SQL was invoked
            let _ = reply.try_send(Err(DraftPublicationErrorV2::NotSubmitted(error)));
            return;
        }
        // Single existing SQL call, no retry. A newer UI save arriving while
        // this runs remains later in this queue/coalescer and is never replayed.
        let result = cache.as_ref().map_err(Clone::clone).and_then(|cache| {
            match &effect {
                PublicationEffectV2::Save(body) => {
                    cache.put_draft(&fence.snapshot.scope, &fence.snapshot.resource, body)
                }
                PublicationEffectV2::Remove(body) => match fence.body_profile_v9 {
                    DraftBodyProfileV9::Standard128 => cache.remove_publication_draft_exact_v2(
                        &fence.snapshot.scope,
                        &fence.snapshot.resource,
                        body,
                    ),
                    DraftBodyProfileV9::WorkflowEdit361 => cache
                        .remove_workflow_edit_draft_exact_v9(
                            &fence.snapshot.scope,
                            &fence.snapshot.resource,
                            body,
                        ),
                },
                PublicationEffectV2::CommitConversation(_) => {
                    return Err("Closed atomic publication used an ordinary reply".into());
                }
                PublicationEffectV2::CommitWorkflowCleanupV9(_) => {
                    return Err("Closed W9 cleanup used an ordinary reply".into())
                }
            }
            .map_err(|error| error.to_string())?;
            Ok(match effect {
                PublicationEffectV2::Save(body) | PublicationEffectV2::Remove(body) => body,
                PublicationEffectV2::CommitConversation(_) => unreachable!("closed reply checked"),
                PublicationEffectV2::CommitWorkflowCleanupV9(_) => {
                    unreachable!("closed reply checked")
                }
            })
        });
        let still_current = (|| {
            fence.clock(&fence.cancel)?;
            let state = fence.producer.lock()?;
            fence
                .producer
                .publication_current_v2(&state, &fence.snapshot, true)
        })();
        // NOT caller RAII: this happens after actual SQL completion or skip.
        drop(activity);
        let outcome = result
            .map_err(DraftPublicationErrorV2::Storage)
            .and_then(|saved_body| {
                still_current
                    .map_err(DraftPublicationErrorV2::AcknowledgementUnknown)
                    .map(|()| DraftPublicationAckV2 { fence, saved_body })
            });
        let _ = reply.try_send(outcome);
    }
}

impl DraftClient {
    /// Tiny foreground capture: no SQL, body clone, send or wait. An untouched
    /// form either registers its actual initial autosave or uses the affirmative
    /// virgin-producer absence state; an unknown/missing ledger is never ready.
    pub fn capture_publication_v2(
        &self,
        database: &Path,
        scope: &Scope,
        resource: &str,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<DraftPublicationFenceV2, DraftPublicationErrorV2> {
        let uid = scope
            .login
            .strip_prefix("user-id:")
            .and_then(|id| id.parse::<u64>().ok())
            .filter(|id| *id > 0 && *id <= i64::MAX as u64);
        if uid.is_none_or(|uid| Scope::github(&scope.host, Some(uid)).ok().as_ref() != Some(scope))
            || resource.is_empty()
            || resource.len() > 1024
            || resource.chars().any(char::is_control)
            || cancel.is_cancelled()
            || Instant::now() >= deadline
        {
            return Err(DraftPublicationErrorV2::NotSubmitted(
                "Original publication draft scope/resource/token/deadline unavailable".into(),
            ));
        }
        let snapshot = self
            .producer
            .publication_snapshot_v2(database, scope, resource)
            .map_err(DraftPublicationErrorV2::NotSubmitted)?;
        Ok(DraftPublicationFenceV2 {
            producer: self.producer.clone(),
            database: database.into(),
            snapshot,
            cancel: cancel.clone(),
            deadline,
            body_profile_v9: DraftBodyProfileV9::Standard128,
        })
    }
    /// Mechanical fence only, derived from an actual workflow intent. The
    /// decoded draft cannot mint this fence or grant remote publication.
    pub(crate) fn capture_workflow_edit_publication_v9(
        &self,
        database: &Path,
        intent: &crate::scoped_actions_workflow_edit_v9::WorkflowEditIntentV9,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<DraftPublicationFenceV2, DraftPublicationErrorV2> {
        let mut fence = self.capture_publication_v2(
            database,
            intent.source().original().scope(),
            intent.key(),
            cancel,
            deadline,
        )?;
        fence.body_profile_v9 = DraftBodyProfileV9::WorkflowEdit361;
        Ok(fence)
    }
    /// Worker-only. Caller captures BEFORE queueing the original remote work.
    /// A body is moved once; after queue acceptance there is never a re-send.
    pub fn save_publication_and_wait_v2(
        &self,
        fence: DraftPublicationFenceV2,
        body: String,
        cancel: &CancellationToken,
    ) -> Result<DraftPublicationAckV2, DraftPublicationErrorV2> {
        if matches!(fence.body_profile_v9, DraftBodyProfileV9::Standard128)
            && body.len() > 128 * 1024
        {
            return Err(DraftPublicationErrorV2::NotSubmitted(
                "Encoded publication draft exceeds128KiB".into(),
            ));
        }
        if matches!(fence.body_profile_v9, DraftBodyProfileV9::WorkflowEdit361)
            && body.len()
                > crate::scoped_actions_workflow_edit_v9::draft_v9::WORKFLOW_DRAFT_BYTES_V9
        {
            return Err(DraftPublicationErrorV2::NotSubmitted(
                "Encoded workflow publication draft exceeds361KiB".into(),
            ));
        }
        self.publication_wait_v2(fence, PublicationEffectV2::Save(body), cancel)
    }
    /// Confirmed-effect cleanup only; SAME original ACK/clock, never a restored
    /// reference or a renewed queue snapshot. The one actual DELETE also matches
    /// the SAME acknowledged encoded body, atomically preserving external edits.
    /// Failure leaves retention unknown; queue-current alone is NOT SQL equality.
    pub fn remove_publication_and_wait_v2(
        &self,
        ack: DraftPublicationAckV2,
        cancel: &CancellationToken,
    ) -> Result<(), DraftPublicationErrorV2> {
        self.publication_wait_v2(
            ack.fence,
            PublicationEffectV2::Remove(ack.saved_body),
            cancel,
        )
        .map(|_| ())
    }
    /// Restrict the SAME acknowledged cleanup to its actual enclosing phase.
    /// No new queue snapshot, token, body copy or later clock is issued.
    #[cfg(test)]
    pub(crate) fn remove_publication_before_v82(
        &self,
        mut ack: DraftPublicationAckV2,
        cancel: &CancellationToken,
        cutoff: Instant,
    ) -> Result<(), DraftPublicationErrorV2> {
        ack.fence.deadline = ack.fence.deadline.min(cutoff);
        self.remove_publication_and_wait_v2(ack, cancel)
    }
    fn publication_wait_v2(
        &self,
        fence: DraftPublicationFenceV2,
        effect: PublicationEffectV2,
        cancel: &CancellationToken,
    ) -> Result<DraftPublicationAckV2, DraftPublicationErrorV2> {
        let (reply, result) = mpsc::sync_channel(1);
        let (token, deadline) =
            self.enqueue_publication_v85(fence, effect, PublicationReplyV85::Draft(reply), cancel)?;
        loop {
            if token.is_cancelled() || Instant::now() >= deadline {
                return Err(DraftPublicationErrorV2::AcknowledgementUnknown("Publication draft accepted; original token/deadline ended before ACK. Inspect durable state; no POST or resend".into()));
            }
            match result.recv_timeout(
                deadline
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(20)),
            ) {
                Ok(ack) => {
                    return ack.and_then(|ack| {
                        ack.check_current(
                            self,
                            &ack.fence.database,
                            &ack.fence.snapshot.scope,
                            &ack.fence.snapshot.resource,
                            cancel,
                        )
                        .map_err(DraftPublicationErrorV2::AcknowledgementUnknown)?;
                        Ok(ack)
                    })
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(DraftPublicationErrorV2::AcknowledgementUnknown(
                        "Draft worker exited after publication acceptance; no durable ACK".into(),
                    ))
                }
            }
        }
    }
    fn enqueue_publication_v85(
        &self,
        mut fence: DraftPublicationFenceV2,
        effect: PublicationEffectV2,
        reply: PublicationReplyV85,
        cancel: &CancellationToken,
    ) -> Result<(CancellationToken, Instant), DraftPublicationErrorV2> {
        if !Arc::ptr_eq(&fence.producer, &self.producer) {
            return Err(DraftPublicationErrorV2::NotSubmitted(
                "Publication fence belongs to another producer".into(),
            ));
        }
        let producer = fence.producer.clone();
        let mut state = producer
            .lock()
            .map_err(DraftPublicationErrorV2::NotSubmitted)?;
        loop {
            fence
                .clock(cancel)
                .map_err(DraftPublicationErrorV2::NotSubmitted)?;
            producer
                .publication_current_v2(&state, &fence.snapshot, false)
                .map_err(DraftPublicationErrorV2::NotSubmitted)?;
            if state
                .outstanding
                .range(..=fence.snapshot.revision)
                .next()
                .is_none()
            {
                break;
            }
            state = producer
                .publication_wait_v2(state, fence.quantum())
                .map_err(DraftPublicationErrorV2::NotSubmitted)?;
        }
        let activity = producer
            .publication_enter_v2(&mut state, &mut fence.snapshot)
            .map_err(DraftPublicationErrorV2::NotSubmitted)?;
        // Capacity checks share the producer mutex with dequeue notification.
        drop(state);
        let deadline = fence.deadline;
        let token = fence.cancel.clone();
        let mut message = Message::PublicationV2(Box::new(PublicationMessageV2 {
            fence,
            effect,
            activity,
            reply,
        }));
        loop {
            let state = producer
                .lock()
                .map_err(DraftPublicationErrorV2::NotSubmitted)?;
            let Message::PublicationV2(ref publication) = message else {
                unreachable!("owned publication message")
            };
            let ready = publication.fence.clock(cancel).and_then(|()| {
                producer.publication_current_v2(&state, &publication.fence.snapshot, true)
            });
            if let Err(error) = ready {
                drop(state);
                drop(message);
                return Err(DraftPublicationErrorV2::NotSubmitted(error));
            }
            match self.sender.try_send(message) {
                Ok(()) => {
                    drop(state);
                    break;
                }
                Err(TrySendError::Full(original)) => {
                    message = original;
                    let waited = producer.publication_wait_v2(
                        state,
                        deadline
                            .saturating_duration_since(Instant::now())
                            .min(Duration::from_millis(20)),
                    );
                    if let Err(error) = waited {
                        drop(message);
                        return Err(DraftPublicationErrorV2::NotSubmitted(error));
                    }
                }
                Err(TrySendError::Disconnected(original)) => {
                    drop(state);
                    drop(original);
                    return Err(DraftPublicationErrorV2::NotSubmitted(
                        "Draft writer disconnected before publication enqueue".into(),
                    ));
                }
            }
        }
        Ok((token, deadline))
    }
}

#[cfg(test)]
#[path = "publication_v2_tests.rs"]
mod tests;
