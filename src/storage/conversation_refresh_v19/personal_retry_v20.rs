//! One last ACTUAL committed Personal reservation, never a current source,
//! counter-rebase permit, old clock/token lease, write/ACK or restored body.
use super::*;
use crate::storage::personal_pr_source_v13::parent_boundary_v16::PersonalConversationReservationRowV20;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    OnceLock,
};

pub(crate) struct PersonalConversationReservationWitnessV20 {
    own: Arc<ConversationOwnFenceV19>,
    row: PersonalConversationReservationRowV20,
    epoch: Arc<EpochV19>,
}
pub(crate) struct PersonalConversationReadAttemptV20 {
    own: Arc<ConversationOwnFenceV19>,
    before: Arc<EpochV19>,
    committed: OnceLock<Arc<PersonalConversationReservationWitnessV20>>,
    unproven: AtomicBool,
    completed: AtomicBool,
    cancel: CancellationToken,
    deadline: Instant,
}
impl ConversationOwnFenceV19 {
    pub(crate) fn is_personal_v20(&self) -> bool {
        matches!(self.family, FamilyV19::Personal(_))
    }
    pub(crate) fn begin_personal_read_attempt_v20(
        self: &Arc<Self>,
        previous: Option<&Arc<PersonalConversationReservationWitnessV20>>,
        provider: &crate::providers::gh::GithubProvider,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<Arc<PersonalConversationReadAttemptV20>> {
        clock(cancel, deadline)?;
        if !self.is_personal_v20() || !self.source().admits_personal_refresh_producer_v20(provider)
        {
            bail!("Original Personal refresh producer differs; no new child started");
        }
        self.terminal.validate_file_v19()?;
        let cache = Cache::open(self.terminal.database())?;
        let tx = cache.connection.unchecked_transaction()?;
        self.terminal.validate_transaction_v19(&tx)?;
        self.validate_once_v19(&cache)?;
        if let Some(previous) = previous {
            if !Arc::ptr_eq(self, &previous.own) {
                bail!("Personal refresh predecessor belongs to another original terminal");
            }
            previous.row.validate_file(&cache)?;
            previous.row.validate_transaction(&tx)?;
            previous.epoch.validate(&tx)?;
        } else if let FamilyV19::Personal(original) = &self.family {
            cache.validate_personal_conversation_fence_v19(original)?;
        }
        // A validated previous epoch is the exact same immutable pre-state;
        // share it, never retain an extra duplicate on every repeated phase.
        let before = match previous {
            Some(previous) => Arc::clone(&previous.epoch),
            None => Arc::new(EpochV19::capture(&tx, &self.terminal, self.source())?),
        };
        tx.commit()?;
        self.terminal.validate_file_v19()?;
        clock(cancel, deadline)?;
        Ok(Arc::new(PersonalConversationReadAttemptV20 {
            own: Arc::clone(self),
            before,
            committed: OnceLock::new(),
            unproven: AtomicBool::new(false),
            completed: AtomicBool::new(false),
            cancel: cancel.clone(),
            deadline,
        }))
    }
}
impl PersonalConversationReadAttemptV20 {
    pub(crate) fn matches_own(&self, own: &ConversationOwnFenceV19) -> bool {
        std::ptr::eq(self.own.as_ref(), own)
    }
    pub(crate) fn matches_phase(
        &self,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<()> {
        if !self.cancel.same_instance(cancel) || deadline > self.deadline {
            bail!("Actual Personal refresh phase token/deadline differs");
        }
        clock(cancel, deadline)
    }
    pub(in crate::storage) fn validate_before_reservation(
        &self,
        tx: &Transaction<'_>,
        parent: &crate::providers::gh::PrResourceParentV1,
    ) -> Result<()> {
        self.matches_phase(&self.cancel, self.deadline)?;
        if self.unproven.load(Ordering::Acquire)
            || self.committed.get().is_some()
            || !crate::providers::gh::GithubProvider::same_thread_history_parent_v1(
                self.own.terminal.parent(),
                parent,
            )
        {
            bail!("Original Personal reservation phase/parent already consumed or changed");
        }
        self.own.terminal.validate_file_v19()?;
        self.own.terminal.validate_transaction_v19(tx)?;
        self.before.validate(tx)
    }
    pub(in crate::storage) fn prepare_commit_witness(
        &self,
        tx: &Transaction<'_>,
        row: PersonalConversationReservationRowV20,
    ) -> Result<Arc<PersonalConversationReservationWitnessV20>> {
        if !row.matches_parent(self.own.terminal.parent()) {
            bail!("Actual Personal committed parent differs");
        }
        self.own.terminal.validate_transaction_v19(tx)?;
        row.validate_transaction(tx)?;
        let epoch = Arc::new(EpochV19::capture(
            tx,
            &self.own.terminal,
            self.own.source(),
        )?);
        Ok(Arc::new(PersonalConversationReservationWitnessV20 {
            own: Arc::clone(&self.own),
            row,
            epoch,
        }))
    }
    pub(in crate::storage) fn mark_commit_unproven(&self) {
        self.unproven.store(true, Ordering::Release);
    }
    pub(in crate::storage) fn record_committed(
        &self,
        witness: Arc<PersonalConversationReservationWitnessV20>,
    ) -> Result<()> {
        self.committed.set(witness).map_err(|_| {
            anyhow::anyhow!("Actual Personal reservation witness already installed")
        })?;
        self.unproven.store(false, Ordering::Release);
        Ok(())
    }
    pub(crate) fn validate_during_read(
        &self,
        cache: &Cache,
        boundary: PrLocalSourceBoundaryV13<'_>,
    ) -> Result<()> {
        self.matches_phase(&self.cancel, self.deadline)?;
        if self.unproven.load(Ordering::Acquire) {
            bail!("Original Personal reservation commit unproven");
        }
        let witness = self
            .committed
            .get()
            .context("No actual Personal reservation was committed")?;
        let PrLocalSourceBoundaryV13::PersonalReservation(actual) = boundary else {
            bail!("Not the actual Personal current-phase reservation");
        };
        cache.validate_personal_pr_reservation_v13(actual)?;
        if !witness.row.matches_current(actual) {
            bail!("Personal read original allocation/row changed");
        }
        witness.row.validate_file(cache)?;
        let tx = cache.connection.unchecked_transaction()?;
        self.own.terminal.validate_transaction_v19(&tx)?;
        witness.row.validate_transaction(&tx)?;
        witness.epoch.validate(&tx)?;
        tx.commit()?;
        self.own.validate_once_v19(cache)?;
        self.own.terminal.validate_file_v19()
    }
    pub(crate) fn completed_page(&self) {
        self.completed.store(true, Ordering::Release);
        self.unproven.store(false, Ordering::Release);
    }
    pub(crate) fn page_completed(&self) -> bool {
        self.completed.load(Ordering::Acquire)
    }
    pub(crate) fn can_settle(&self) -> bool {
        !self.unproven.load(Ordering::Acquire) && !self.completed.load(Ordering::Acquire)
    }
    pub(crate) fn last_committed(&self) -> Option<Arc<PersonalConversationReservationWitnessV20>> {
        self.committed.get().cloned()
    }
}

#[cfg(test)]
std::thread_local! {static AFTER_RESERVATION_V20:std::cell::RefCell<Option<Box<dyn FnOnce()>>>=std::cell::RefCell::new(None);}
#[cfg(test)]
pub(crate) fn after_actual_reservation_v20() {
    AFTER_RESERVATION_V20.with(|cell| {
        let hook = cell.borrow_mut().take();
        if let Some(hook) = hook {
            hook();
        }
    });
}
#[cfg(test)]
pub(crate) fn observe_next_actual_reservation_v20(hook: impl FnOnce() + 'static) {
    AFTER_RESERVATION_V20.with(|cell| {
        assert!(cell.borrow().is_none());
        *cell.borrow_mut() = Some(Box::new(hook));
    });
}
