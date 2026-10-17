//! Genuine completed PARENT-only local boundary. Neither a Files source nor
//! access/publication approval. Existing reservation/table/global blockers stay.
use super::*;

pub(crate) struct PreparedPersonalParentBoundaryV16 {
    reservation: PersonalPrReservationV13,
    parent: Arc<PrResourceParentV1>,
    ticket: Arc<()>,
    issuer: crate::providers::gh::ThreadPhaseIssuerV16,
}
/// Issued only after full closing-parent and SAME actual original allocation
/// checks BEFORE final UID. Not Clone/serde/publicly constructible.
pub(crate) struct PersonalParentCloseTicketV16 {
    prepared: Arc<()>,
    original: Arc<PersonalRepositoryOriginalV13>,
}
pub(crate) struct ClosedPersonalParentBoundaryV16 {
    reservation: PersonalPrReservationV13,
    parent: Arc<PrResourceParentV1>,
    closed: ClosedPersonalPrAccessV13,
    issuer: crate::providers::gh::ThreadPhaseIssuerV16,
}
impl std::fmt::Debug for ClosedPersonalParentBoundaryV16 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ClosedPersonalParentBoundaryV16(<actual original parent/cache epoch; no Files or write permission>)")
    }
}
impl Cache {
    /// SAME private existing reservation transaction/key. No new table, fake
    /// read or source is created; original Core/raw parent MUST be genuine.
    pub(crate) fn prepare_personal_parent_boundary_v16(
        &mut self,
        original: &Arc<PersonalRepositoryOriginalV13>,
        parent: Arc<PrResourceParentV1>,
        issuer: crate::providers::gh::ThreadPhaseIssuerV16,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> BoundaryResult<PreparedPersonalParentBoundaryV16> {
        live(cancel, deadline)?;
        let reservation =
            self.reserve_personal_pr_source_v13(original, &parent, cancel, deadline)?;
        if !Arc::ptr_eq(&reservation.original, original) || !reservation.parent.matches(&parent) {
            return Err(PersonalPrBoundaryFailureV13::OriginalParent);
        }
        Ok(PreparedPersonalParentBoundaryV16 {
            reservation,
            parent,
            ticket: Arc::new(()),
            issuer,
        })
    }
    /// Completed read provenance is not an expiring operation lease. EACH later
    /// read/write still needs its own fresh original Core/source/current UID.
    /// Clear/replacement/foreign issuer/generation refuses with no renewal.
    pub(crate) fn validate_personal_parent_boundary_v16(
        &self,
        source: &ClosedPersonalParentBoundaryV16,
    ) -> BoundaryResult<()> {
        let original = &source.reservation;
        if issuer(self).map_err(|_| PersonalPrBoundaryFailureV13::Database)? != original.issuer
            || self
                .sync_reservation(&original.scope, &original.sync_source)
                .map_err(|_| PersonalPrBoundaryFailureV13::Database)?
                != Some(original.generation)
            || issuer(self).map_err(|_| PersonalPrBoundaryFailureV13::Database)? != original.issuer
        {
            return Err(PersonalPrBoundaryFailureV13::OriginalSource);
        }
        Ok(())
    }
}
impl PreparedPersonalParentBoundaryV16 {
    pub(crate) fn reservation(&self) -> &PersonalPrReservationV13 {
        &self.reservation
    }
    /// Run BEFORE final SQL/current UID. Full actual raw parent equality is not
    /// reduced to the numeric key or H alone; actual original Arc also matches.
    pub(crate) fn verify_for_close(
        &self,
        original: &Arc<PersonalRepositoryOriginalV13>,
        closing_parent: &PrResourceParentV1,
    ) -> BoundaryResult<PersonalParentCloseTicketV16> {
        live(&self.reservation.cancel, self.reservation.deadline)?;
        if !Arc::ptr_eq(original, &self.reservation.original)
            || !self.reservation.parent.matches(closing_parent)
            || !crate::providers::gh::GithubProvider::same_thread_history_parent_v1(
                &self.parent,
                closing_parent,
            )
        {
            return Err(PersonalPrBoundaryFailureV13::OriginalParent);
        }
        Ok(PersonalParentCloseTicketV16 {
            prepared: self.ticket.clone(),
            original: original.clone(),
        })
    }
    /// Only O(1) pure moves/pointer/clock checks follow final actual UID. All
    /// parent/string/body/SQL checks already finished before close above.
    pub(crate) fn bind_after_close(
        self,
        closed: ClosedPersonalPrAccessV13,
        ticket: PersonalParentCloseTicketV16,
    ) -> BoundaryResult<ClosedPersonalParentBoundaryV16> {
        live(&self.reservation.cancel, self.reservation.deadline)?;
        if !Arc::ptr_eq(&ticket.prepared, &self.ticket)
            || !Arc::ptr_eq(&ticket.original, &self.reservation.original)
            || !Arc::ptr_eq(closed.original(), &self.reservation.original)
        {
            return Err(PersonalPrBoundaryFailureV13::OriginalSource);
        }
        Ok(ClosedPersonalParentBoundaryV16 {
            reservation: self.reservation,
            parent: self.parent,
            closed,
            issuer: self.issuer,
        })
    }
}
impl ClosedPersonalParentBoundaryV16 {
    /// Borrow the SAME genuine final closed observation. Not a new current
    /// check, Clone/constructor, read window or publication permission.
    pub(crate) fn observation_v17(&self) -> &crate::providers::gh::CurrentCatalogueRepository {
        self.closed.observation()
    }
    pub(crate) fn admits_producer(&self, provider: &crate::providers::gh::GithubProvider) -> bool {
        self.issuer.matches(provider)
    }
    pub(crate) fn scope(&self) -> &Scope {
        &self.reservation.scope
    }
    pub(crate) fn parent(&self) -> &Arc<PrResourceParentV1> {
        &self.parent
    }
    pub(crate) fn original(&self) -> &Arc<PersonalRepositoryOriginalV13> {
        self.closed.original()
    }
}

/// Exact LOCAL result of the actual Conversation's successful privacy fence,
/// not a new source/access boundary or a clock/token renewal.
pub(in crate::storage) struct PersonalConversationFenceV19 {
    issuer: Issuer,
    scope: Scope,
    key: String,
    generation: u64,
    active: bool,
    parent: ActualParentKeyV13,
}
pub(in crate::storage) struct PreparedPersonalConversationFenceV19(
    Box<PersonalConversationFenceV19>,
);
impl Cache {
    pub(in crate::storage) fn prepare_personal_conversation_fence_v19(
        &self,
        source: &ClosedPersonalParentBoundaryV16,
        fenced: bool,
    ) -> Result<PreparedPersonalConversationFenceV19> {
        self.validate_personal_parent_boundary_v16(source)?;
        let old = &source.reservation;
        let generation = old
            .generation
            .checked_add(u64::from(fenced))
            .filter(|n| *n <= i64::MAX as u64)
            .context("Personal Conversation local fence generation exhausted")?;
        Ok(PreparedPersonalConversationFenceV19(Box::new(
            PersonalConversationFenceV19 {
                issuer: old.issuer.clone(),
                scope: old.scope.clone(),
                key: old.sync_source.clone(),
                generation,
                active: !fenced,
                parent: ActualParentKeyV13::capture(&source.parent)?,
            },
        )))
    }
    pub(in crate::storage) fn validate_personal_conversation_fence_v19(
        &self,
        own: &PersonalConversationFenceV19,
    ) -> Result<()> {
        if issuer(self)? != own.issuer {
            bail!("Original Personal Conversation fence issuer changed");
        }
        let row:Option<(u64,bool)>=self.connection.query_row("SELECT generation,active FROM sync_reservations WHERE host=?1 AND login=?2 AND source=?3",params![own.scope.host,own.scope.login,own.key],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if row != Some((own.generation, own.active)) {
            bail!("Original Personal own-fence/Clear epoch changed; no fresh read rebound");
        }
        Ok(())
    }
    /// A NEW actual Personal initial Core makes exactly its normal ONE parent
    /// reservation. Validate that sole known transition rather than exempting
    /// it, reusing the old boundary, or overlooking a concurrent Clear.
    pub(in crate::storage) fn validate_personal_conversation_read_v19(
        &self,
        own: &PersonalConversationFenceV19,
        current: &PersonalPrReservationV13,
    ) -> Result<()> {
        self.validate_personal_pr_reservation_v13(current)?;
        if issuer(self)? != own.issuer
            || current.issuer != own.issuer
            || current.scope != own.scope
            || current.sync_source != own.key
            || current.generation
                != own
                    .generation
                    .checked_add(1)
                    .context("Personal explicit read generation exhausted")?
            || current.parent.repository_id != own.parent.repository_id
            || current.parent.repository_node != own.parent.repository_node
            || current.parent.pr_id != own.parent.pr_id
            || current.parent.pr_node != own.parent.pr_node
            || current.parent.number != own.parent.number
            || current.parent.head != own.parent.head
            || current.parent.base != own.parent.base
        {
            bail!("Actual new Personal Core/reservation differs from exact own-fence transition");
        }
        Ok(())
    }
}
impl PreparedPersonalConversationFenceV19 {
    pub(in crate::storage) fn bind_committed_v19(
        self,
        tx: &rusqlite::Transaction<'_>,
    ) -> Result<Box<PersonalConversationFenceV19>> {
        let own = self.0;
        let exact:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM sync_reservations WHERE host=?1 AND login=?2 AND source=?3 AND generation=?4 AND active=?5)",params![own.scope.host,own.scope.login,own.key,own.generation,own.active],|r|r.get(0))?;
        if !exact {
            bail!("Actual Personal own-fence transaction result differs");
        }
        Ok(own)
    }
}

/// Actual one-transaction row observation. Neither a source nor an expired
/// phase lease: only the closed Conversation attempt may retain/use this.
pub(crate) struct PersonalConversationReservationRowV20 {
    issuer: Issuer,
    scope: Scope,
    source: String,
    generation: u64,
    original: Arc<PersonalRepositoryOriginalV13>,
    parent: Arc<PrResourceParentV1>,
}
impl PersonalConversationReservationRowV20 {
    pub(in crate::storage) fn validate_file(&self, cache: &Cache) -> Result<()> {
        if issuer(cache)? != self.issuer {
            bail!("Actual Personal read reservation database changed");
        }
        Ok(())
    }
    pub(in crate::storage) fn validate_transaction(
        &self,
        tx: &rusqlite::Transaction<'_>,
    ) -> Result<()> {
        let exact: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_reservations WHERE host=?1 AND login=?2 AND source=?3 AND generation=?4 AND active=1)",
            params![self.scope.host,self.scope.login,self.source,self.generation],
            |r|r.get(0),
        )?;
        if !exact {
            bail!("Actual Personal committed read reservation changed; no counter rebased");
        }
        Ok(())
    }
    pub(in crate::storage) fn matches_current(&self, current: &PersonalPrReservationV13) -> bool {
        current.issuer == self.issuer
            && current.scope == self.scope
            && current.sync_source == self.source
            && current.generation == self.generation
            && Arc::ptr_eq(&current.original, &self.original)
            && current.parent.matches(&self.parent)
    }
    pub(in crate::storage) fn matches_parent(&self, parent: &PrResourceParentV1) -> bool {
        crate::providers::gh::GithubProvider::same_thread_history_parent_v1(&self.parent, parent)
    }
}
impl Cache {
    /// SAME normal reservation kernel. The closed attempt captures the actual
    /// post-update metadata INSIDE this transaction, before any later live
    /// check can fail after commit. No callback, scalar renewal or new table.
    pub(crate) fn prepare_personal_conversation_read_boundary_v20(
        &mut self,
        attempt: &crate::storage::conversation_refresh_v19::PersonalConversationReadAttemptV20,
        original: &Arc<PersonalRepositoryOriginalV13>,
        parent: Arc<PrResourceParentV1>,
        producer: crate::providers::gh::ThreadPhaseIssuerV16,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> BoundaryResult<PreparedPersonalParentBoundaryV16> {
        let material =
            self.prepare_personal_reservation_material_v20(original, &parent, cancel, deadline)?;
        let ticket = Arc::new(());
        attempt
            .matches_phase(cancel, deadline)
            .map_err(|_| PersonalPrBoundaryFailureV13::OriginalSource)?;
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|_| PersonalPrBoundaryFailureV13::Database)?;
        attempt
            .validate_before_reservation(&tx, &parent)
            .map_err(|_| PersonalPrBoundaryFailureV13::OriginalSource)?;
        let generation =
            Cache::reserve_sync_generation_row_v20(&tx, &material.scope, &material.sync_source)
                .map_err(|_| PersonalPrBoundaryFailureV13::Database)?;
        let reservation = PersonalPrReservationV13 {
            issuer: material.issuer,
            scope: material.scope,
            sync_source: material.sync_source,
            generation,
            original: Arc::clone(original),
            parent: material.parent,
            cancel: cancel.clone(),
            deadline,
        };
        let row = PersonalConversationReservationRowV20 {
            issuer: reservation.issuer.clone(),
            scope: reservation.scope.clone(),
            source: reservation.sync_source.clone(),
            generation,
            original: Arc::clone(original),
            parent: Arc::clone(&parent),
        };
        let committed = attempt
            .prepare_commit_witness(&tx, row)
            .map_err(|_| PersonalPrBoundaryFailureV13::OriginalSource)?;
        live(cancel, deadline)?;
        attempt.mark_commit_unproven();
        tx.commit()
            .map_err(|_| PersonalPrBoundaryFailureV13::Database)?;
        // This preallocated O(1) handoff MUST precede issuer/live checks.
        attempt
            .record_committed(committed)
            .map_err(|_| PersonalPrBoundaryFailureV13::OriginalSource)?;
        #[cfg(test)]
        crate::storage::conversation_refresh_v19::after_actual_reservation_v20();
        let current_issuer = match issuer(self) {
            Ok(current) => current,
            Err(_) => {
                attempt.mark_commit_unproven();
                return Err(PersonalPrBoundaryFailureV13::Database);
            }
        };
        if current_issuer != reservation.issuer {
            attempt.mark_commit_unproven();
            return Err(PersonalPrBoundaryFailureV13::OriginalSource);
        }
        live(cancel, deadline)?;
        Ok(PreparedPersonalParentBoundaryV16 {
            reservation,
            parent,
            ticket,
            issuer: producer,
        })
    }
}
