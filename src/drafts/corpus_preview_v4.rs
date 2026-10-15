//! Queue acknowledgements only: no SQL, body replay, provider or file authority.
use super::*;
use crate::providers::runner::CancellationToken;
use std::{
    collections::BTreeSet,
    path::Path,
    sync::{Condvar, Mutex, MutexGuard},
    time::{Duration, Instant},
};

const OUTSTANDING_LIMIT_V4: usize = 25; // eight queued, sixteen coalesced, one writing

pub(super) struct PendingDraftV4 {
    pub revision: u64,
    pub body: String,
}
struct LatestV4 {
    revision: u64,
    scope: Scope,
    resource: String,
    outcome: Option<Result<(), String>>,
}
struct PublicationWaitV2 {
    ticket: u64,
    cutoff: u64,
    failed: bool,
}
#[derive(Default)]
pub(super) struct StateV4 {
    pub(super) revision: u64,
    epoch: u64,
    pub(super) outstanding: BTreeSet<u64>,
    latest: Option<LatestV4>,
    active: Option<(u64, Option<String>)>,
    closed: bool,
    pub(super) legacy_active: usize,
    publication_ticket_v2: u64,
    publication_wait_v2: Option<PublicationWaitV2>,
}
pub(super) struct DraftProducerV4 {
    database: PathBuf,
    state: Mutex<StateV4>,
    changed: Condvar,
}
impl DraftProducerV4 {
    /// Publication-only metadata bridge. Ordinary and Corpus admission retain
    /// their existing semantics; no per-key history or body replay is added.
    pub(super) fn publication_snapshot_v2(
        &self,
        database: &Path,
        scope: &Scope,
        resource: &str,
    ) -> Result<super::publication_v2::PublicationSnapshotV2, String> {
        let mut state = self.lock()?;
        if self.database != database || state.closed || state.legacy_active != 0 {
            return Err("Original publication draft database/worker/activity unavailable".into());
        }
        // Only the affirmative brand-new producer state. A prior generic or
        // publication activity has already advanced epoch even without a UI
        // revision, and must register an actual initial autosave instead.
        let observed_absence = state.revision == 0
            && state.epoch == 0
            && state.latest.is_none()
            && state.outstanding.is_empty();
        if !observed_absence
            && state
                .latest
                .as_ref()
                .is_none_or(|latest| latest.scope != *scope || latest.resource != resource)
        {
            return Err("Register this untouched publication intent through ordinary autosave before capture".into());
        }
        let ticket = state.publication_ticket_v2.checked_add(1).ok_or_else(|| {
            state.closed = true;
            "Draft publication capture ticket exhausted".to_owned()
        })?;
        let failed = state
            .latest
            .as_ref()
            .is_some_and(|latest| latest.outcome.as_ref().is_some_and(Result::is_err));
        state.publication_ticket_v2 = ticket;
        state.publication_wait_v2 = Some(PublicationWaitV2 {
            ticket,
            cutoff: state.revision,
            failed,
        });
        self.changed.notify_all();
        Ok(super::publication_v2::PublicationSnapshotV2 {
            revision: state.revision,
            epoch: state.epoch,
            ticket,
            scope: scope.clone(),
            resource: resource.into(),
            observed_absence,
        })
    }
    pub(super) fn publication_current_v2(
        &self,
        state: &StateV4,
        snapshot: &super::publication_v2::PublicationSnapshotV2,
        own_activity: bool,
    ) -> Result<(), String> {
        if state.closed {
            return Err("Original draft writer closed".into());
        }
        let wait = state
            .publication_wait_v2
            .as_ref()
            .filter(|wait| wait.ticket == snapshot.ticket && wait.cutoff == snapshot.revision)
            .ok_or("Original publication capture ticket was superseded")?;
        if wait.failed {
            return Err("A captured outstanding or latest draft save failed; no publication draft or POST was started".into());
        }
        if state.revision != snapshot.revision
            || state.epoch != snapshot.epoch
            || state.legacy_active != usize::from(own_activity)
            || if snapshot.observed_absence {
                state.latest.is_some() || state.revision != 0
            } else {
                state.latest.as_ref().is_none_or(|latest| {
                    latest.revision != snapshot.revision
                        || latest.scope != snapshot.scope
                        || latest.resource != snapshot.resource
                })
            }
        {
            return Err("Original publication draft revision was superseded/transferred or another publication started".into());
        }
        Ok(())
    }
    pub(super) fn publication_enter_v2(
        self: &Arc<Self>,
        state: &mut StateV4,
        snapshot: &mut super::publication_v2::PublicationSnapshotV2,
    ) -> Result<LegacyActivityV4, String> {
        self.publication_current_v2(state, snapshot, false)?;
        let epoch = state
            .epoch
            .checked_add(1)
            .ok_or("Draft publication epoch exhausted")?;
        state.legacy_active = 1;
        state.epoch = epoch;
        state.active = None;
        snapshot.epoch = epoch;
        self.changed.notify_all();
        Ok(LegacyActivityV4(self.clone()))
    }
    pub(super) fn publication_wait_v2<'a>(
        &self,
        state: MutexGuard<'a, StateV4>,
        duration: Duration,
    ) -> Result<MutexGuard<'a, StateV4>, String> {
        self.changed
            .wait_timeout(state, duration)
            .map(|waited| waited.0)
            .map_err(|_| "Draft publication acknowledgement state unavailable".into())
    }
    /// Dequeue/capacity and enqueue/wait use the SAME mutex. Completion cannot
    /// notify between a full-queue predicate and installation of its waiter.
    pub(super) fn publication_capacity_v2(&self) {
        if let Ok(state) = self.lock() {
            drop(state);
        }
        self.changed.notify_all();
    }
    pub fn new(database: PathBuf) -> Self {
        Self {
            database,
            state: Mutex::new(StateV4::default()),
            changed: Condvar::new(),
        }
    }
    pub(super) fn lock(&self) -> Result<MutexGuard<'_, StateV4>, String> {
        self.state
            .lock()
            .map_err(|_| "Draft acknowledgement state unavailable".into())
    }
    pub fn retire(&self, _reason: &str) {
        if let Ok(mut state) = self.lock() {
            match state.epoch.checked_add(1) {
                Some(epoch) => state.epoch = epoch,
                None => state.closed = true,
            }
            state.active = None;
        }
        self.changed.notify_all();
    }
    /// An active legacy sender may be blocked on this SAME queue and enqueue
    /// later. Capture must exclude it atomically, not merely observe one epoch
    /// retirement before a still-running batch. No body is retained here.
    pub fn begin_legacy(self: &Arc<Self>) -> Result<LegacyActivityV4, String> {
        let mut state = self.lock()?;
        if state.closed {
            return Err("Draft writer closed".into());
        }
        let count = state
            .legacy_active
            .checked_add(1)
            .filter(|count| *count <= OUTSTANDING_LIMIT_V4);
        let epoch = state.epoch.checked_add(1);
        let (Some(count), Some(epoch)) = (count, epoch) else {
            state.closed = true;
            self.changed.notify_all();
            return Err("Draft publication activity limit exhausted".into());
        };
        state.legacy_active = count;
        state.epoch = epoch;
        state.active = None;
        self.changed.notify_all();
        Ok(LegacyActivityV4(self.clone()))
    }
    pub fn close(&self) {
        if let Ok(mut state) = self.lock() {
            state.closed = true;
        }
        self.changed.notify_all();
    }
    pub fn transferred(&self, revision: u64) {
        if let Ok(mut state) = self.lock() {
            state.outstanding.remove(&revision);
        }
        self.changed.notify_all();
    }
    pub fn complete(&self, revision: Option<u64>, result: &Result<(), String>) {
        let Some(revision) = revision else {
            return;
        };
        if let Ok(mut state) = self.lock() {
            // Only an ACTUAL outstanding completion can affect the current
            // bounded cutoff. A stale duplicate cannot poison a newer ticket.
            if state.outstanding.contains(&revision) && result.is_err() {
                if let Some(wait) = state
                    .publication_wait_v2
                    .as_mut()
                    .filter(|wait| revision <= wait.cutoff)
                {
                    wait.failed = true;
                }
            }
            if let Some(latest) = state
                .latest
                .as_mut()
                .filter(|latest| latest.revision == revision)
            {
                latest.outcome = Some(result.clone());
            }
            if let Some((cutoff, error)) = &mut state.active {
                if revision <= *cutoff && error.is_none() {
                    *error = result.as_ref().err().cloned();
                }
            }
            state.outstanding.remove(&revision);
        }
        self.changed.notify_all();
    }
}
pub(super) struct LegacyActivityV4(Arc<DraftProducerV4>);
impl Drop for LegacyActivityV4 {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.lock() {
            match state.legacy_active.checked_sub(1) {
                Some(count) => state.legacy_active = count,
                None => state.closed = true,
            }
        }
        self.0.changed.notify_all();
    }
}
pub(super) struct WorkerLifetimeV4(pub Arc<DraftProducerV4>);
impl Drop for WorkerLifetimeV4 {
    fn drop(&mut self) {
        self.0.close();
    }
}

/// Closed original queue snapshot, not cache, account or publication permission.
/// Its absolute clock starts before queueing Preview and is never renewed.
pub struct CorpusDraftFenceV4 {
    producer: Arc<DraftProducerV4>,
    scope: Scope,
    resource: String,
    revision: u64,
    epoch: u64,
    deadline: Instant,
}
impl std::fmt::Debug for CorpusDraftFenceV4 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CorpusDraftFenceV4")
            .field("revision", &self.revision)
            .finish_non_exhaustive()
    }
}
impl CorpusDraftFenceV4 {
    /// Bounds only; never a provider identity or access receipt.
    pub fn deadline(&self) -> Instant {
        self.deadline
    }
    fn check_state(&self, state: &StateV4, cancel: &CancellationToken) -> Result<(), String> {
        if cancel.is_cancelled() {
            return Err("Repository preview cancelled; no plan published".into());
        }
        if Instant::now() >= self.deadline {
            return Err("Repository preview deadline expired while saving original drafts".into());
        }
        if state.closed {
            return Err("Draft writer closed before original preview became ready".into());
        }
        if state.legacy_active != 0 {
            return Err(
                "Original draft publication is still active; preview again after it completes"
                    .into(),
            );
        }
        if state.epoch != self.epoch
            || !state.latest.as_ref().is_some_and(|latest| {
                latest.revision == self.revision
                    && latest.scope == self.scope
                    && latest.resource == self.resource
            })
        {
            return Err("Original repository preview draft was superseded or transferred; preview again explicitly".into());
        }
        let Some((revision, error)) = &state.active else {
            return Err("Original repository preview acknowledgement retired".into());
        };
        if *revision != self.revision {
            return Err("Original repository preview acknowledgement replaced".into());
        }
        if let Some(error) = error {
            return Err(format!(
                "Original draft was not saved; preview refused: {error}"
            ));
        }
        Ok(())
    }
    /// Recheck after preparation, before any private prepared state is returned.
    /// This is queue ownership only; the normal account/source checks still run.
    pub fn check_current(&self, cancel: &CancellationToken) -> Result<(), String> {
        let state = self.producer.lock()?;
        self.check_state(&state, cancel)
    }
}
impl DraftWriter {
    pub(super) fn accept_corpus_revision_v4(
        &mut self,
        scope: Scope,
        resource: String,
        body: String,
        deadline: Option<Instant>,
    ) -> Result<(u64, u64), String> {
        let mut state = self.producer.lock()?;
        if state.closed {
            return Err("Draft writer is unavailable; keep this composer open".into());
        }
        if deadline.is_some() && state.legacy_active != 0 {
            return Err(
                "Original draft publication is still active; preview again after it completes"
                    .into(),
            );
        }
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return Err("Repository preview deadline expired before draft submission".into());
        }
        let revision = state
            .revision
            .checked_add(1)
            .ok_or("Draft revision exhausted; keep this composer open")?;
        let key = (scope.host.clone(), scope.login.clone(), resource.clone());
        let message = Message::Save(scope.clone(), resource.clone(), body, Some(revision));
        let coalesced = match self.sender.try_send(message) {
            Ok(()) => None,
            Err(TrySendError::Full(Message::Save(_, _, body, _))) => {
                if self.pending.len() >= 16 && !self.pending.contains_key(&key) {
                    return Err("Draft queue is full; keep this composer open".into());
                }
                Some(PendingDraftV4 { revision, body })
            }
            Err(TrySendError::Disconnected(_)) => {
                state.closed = true;
                self.producer.changed.notify_all();
                return Err("Draft writer is unavailable; keep this composer open".into());
            }
            Err(_) => unreachable!("accept only sends ordinary draft saves"),
        };
        if let Some(old) = self.pending.remove(&key) {
            state.outstanding.remove(&old.revision);
        }
        if let Some(pending) = coalesced {
            self.pending.insert(key, pending);
        }
        state.outstanding.insert(revision);
        // The sender capacity and coalescer bound make this invariant structural.
        debug_assert!(state.outstanding.len() <= OUTSTANDING_LIMIT_V4);
        state.revision = revision;
        state.latest = Some(LatestV4 {
            revision,
            scope,
            resource,
            outcome: None,
        });
        state.active = deadline.map(|_| (revision, None));
        self.producer.changed.notify_all();
        Ok((revision, state.epoch))
    }

    /// Capture by moving the current encoded body into the existing nonblocking
    /// autosave path. This never drains or resends an unrelated draft body.
    pub fn capture_corpus_preview_v4(
        &mut self,
        scope: Scope,
        resource: String,
        body: String,
        database: &Path,
        deadline: Instant,
    ) -> Result<Arc<CorpusDraftFenceV4>, String> {
        let id = scope
            .login
            .strip_prefix("user-id:")
            .and_then(|id| id.parse::<u64>().ok())
            .filter(|id| *id > 0);
        if id.is_none_or(|id| scope.login != format!("user-id:{id}"))
            || scope.host.trim().is_empty()
            || !resource.starts_with("corpus-management:")
            || resource.len() == "corpus-management:".len()
            || self.producer.database != database
        {
            return Err(
                "Original repository draft account, resource or database unavailable".into(),
            );
        }
        let (revision, epoch) =
            self.accept_corpus_revision_v4(scope.clone(), resource.clone(), body, Some(deadline))?;
        Ok(Arc::new(CorpusDraftFenceV4 {
            producer: self.producer.clone(),
            scope,
            resource,
            revision,
            epoch,
            deadline,
        }))
    }
}
impl DraftClient {
    /// Wait for all accepted outstanding revisions at the original cutoff, not
    /// merely this key. No SQL, send, body replay or future-intent drain occurs.
    pub fn wait_corpus_preview_v4(
        &self,
        fence: &CorpusDraftFenceV4,
        cancel: &CancellationToken,
    ) -> Result<(), String> {
        if !Arc::ptr_eq(&self.producer, &fence.producer) {
            return Err("Repository preview belongs to another draft producer".into());
        }
        let mut state = self.producer.lock()?;
        loop {
            fence.check_state(&state, cancel)?;
            if state.outstanding.range(..=fence.revision).next().is_none() {
                let latest = state
                    .latest
                    .as_ref()
                    .expect("checked exact latest revision");
                return latest
                    .outcome
                    .as_ref()
                    .ok_or("Original draft acknowledgement unavailable")?
                    .clone();
            }
            let remaining = fence
                .deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(20));
            // Predicate and sleep share the mutex: completion cannot be lost.
            state = self
                .producer
                .changed
                .wait_timeout(state, remaining)
                .map_err(|_| "Draft acknowledgement state unavailable".to_owned())?
                .0;
        }
    }
}
