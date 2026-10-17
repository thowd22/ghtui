//! Mechanical read ordering only. No body, SQL, provider, save or publication
//! authority is captured, and no existing Corpus/publication ticket is changed.
use super::{corpus_preview_v4::ConversationReadWaitV20, DraftClient, DraftProducerV4};
use crate::providers::runner::CancellationToken;
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReadCutoffFailureV20 {
    SaveFailed,
    Unavailable,
}

pub(crate) struct ConversationOpenReadCutoffV20 {
    producer: Arc<DraftProducerV4>,
    ticket: Arc<()>,
    revision: u64,
    epoch: u64,
    cancel: CancellationToken,
    deadline: Instant,
    pending_at_capture: bool,
}
impl ConversationOpenReadCutoffV20 {
    pub(crate) fn pending_at_capture(&self) -> bool {
        self.pending_at_capture
    }
    pub(crate) fn check_original(&self) -> Result<(), ReadCutoffFailureV20> {
        let state = self
            .producer
            .lock()
            .map_err(|_| ReadCutoffFailureV20::Unavailable)?;
        self.current(&state, &self.cancel)
    }
    fn current(
        &self,
        state: &super::corpus_preview_v4::StateV4,
        cancel: &CancellationToken,
    ) -> Result<(), ReadCutoffFailureV20> {
        if !self.cancel.same_instance(cancel)
            || cancel.is_cancelled()
            || Instant::now() >= self.deadline
            || state.closed
            || state.legacy_active != 0
            || state.revision != self.revision
            || state.epoch != self.epoch
        {
            return Err(ReadCutoffFailureV20::Unavailable);
        }
        let wait = state
            .conversation_read_v20
            .as_ref()
            .filter(|w| {
                w.cutoff == self.revision
                    && w.epoch == self.epoch
                    && w.ticket
                        .upgrade()
                        .is_some_and(|t| Arc::ptr_eq(&t, &self.ticket))
            })
            .ok_or(ReadCutoffFailureV20::Unavailable)?;
        if wait.failed {
            return Err(ReadCutoffFailureV20::SaveFailed);
        }
        Ok(())
    }
    pub(crate) fn check(
        &self,
        client: &DraftClient,
        cancel: &CancellationToken,
    ) -> Result<(), ReadCutoffFailureV20> {
        if !Arc::ptr_eq(&self.producer, &client.producer) {
            return Err(ReadCutoffFailureV20::Unavailable);
        }
        let state = self
            .producer
            .lock()
            .map_err(|_| ReadCutoffFailureV20::Unavailable)?;
        self.current(&state, cancel)
    }
}
impl Drop for ConversationOpenReadCutoffV20 {
    fn drop(&mut self) {
        if let Ok(mut state) = self.producer.lock() {
            if state.conversation_read_v20.as_ref().is_some_and(|w| {
                w.ticket
                    .upgrade()
                    .is_some_and(|t| Arc::ptr_eq(&t, &self.ticket))
            }) {
                state.conversation_read_v20 = None;
            }
        }
        self.producer.changed.notify_all();
    }
}
impl DraftClient {
    /// Prequeue, O(1), no send/SQL/body copy. Occupied read tickets are refused,
    /// never replaced; failed queue admission simply drops its own new ticket.
    pub(crate) fn capture_conversation_open_cutoff_v20(
        &self,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<ConversationOpenReadCutoffV20, ReadCutoffFailureV20> {
        let mut state = self
            .producer
            .lock()
            .map_err(|_| ReadCutoffFailureV20::Unavailable)?;
        if database != self.producer.database
            || cancel.is_cancelled()
            || Instant::now() >= deadline
            || state.closed
            || state.legacy_active != 0
            || state
                .conversation_read_v20
                .as_ref()
                .is_some_and(|w| w.ticket.upgrade().is_some())
        {
            return Err(ReadCutoffFailureV20::Unavailable);
        }
        let ticket = Arc::new(());
        state.conversation_read_v20 = Some(ConversationReadWaitV20 {
            ticket: Arc::downgrade(&ticket),
            cutoff: state.revision,
            epoch: state.epoch,
            failed: DraftProducerV4::latest_save_failed_v20(&state),
        });
        Ok(ConversationOpenReadCutoffV20 {
            producer: self.producer.clone(),
            ticket,
            revision: state.revision,
            epoch: state.epoch,
            cancel: cancel.clone(),
            deadline,
            pending_at_capture: state.outstanding.range(..=state.revision).next().is_some(),
        })
    }
    /// Worker only, BEFORE private Core/provider/local draft loading. The SAME
    /// actual accepted coalesced ledger settles through normal main-loop poll.
    pub(crate) fn wait_conversation_open_cutoff_v20(
        &self,
        cutoff: &ConversationOpenReadCutoffV20,
        cancel: &CancellationToken,
    ) -> Result<(), ReadCutoffFailureV20> {
        if !Arc::ptr_eq(&self.producer, &cutoff.producer) {
            return Err(ReadCutoffFailureV20::Unavailable);
        }
        let mut state = self
            .producer
            .lock()
            .map_err(|_| ReadCutoffFailureV20::Unavailable)?;
        loop {
            cutoff.current(&state, cancel)?;
            if state.outstanding.range(..=cutoff.revision).next().is_none() {
                return Ok(());
            }
            let duration = Duration::from_millis(20)
                .min(cutoff.deadline.saturating_duration_since(Instant::now()));
            state = self
                .producer
                .publication_wait_v2(state, duration)
                .map_err(|_| ReadCutoffFailureV20::Unavailable)?;
        }
    }
}

#[cfg(test)]
#[path = "conversation_open_cutoff_v20_tests.rs"]
mod tests;
