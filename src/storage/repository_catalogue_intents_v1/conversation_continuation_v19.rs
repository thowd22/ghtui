//! LOCAL cache-boundary continuation only. The SAME genuine inventory Read
//! allocation is retained; current access is obtained by each normal next Core.
//! No old Arc is mutated, scope-wide fence exempted, or saved source restored.
use super::*;
use crate::providers::gh::{ConversationRefreshPreparationV19, PrResourceParentV1};

pub(in crate::storage) struct PreparedCatalogueContinuationV19 {
    original: Arc<CatalogueIntentSource>,
    candidate: Arc<CatalogueIntentSource>,
    transition: LocalTransitionV19,
}
struct LocalTransitionV19 {
    expected: u64,
    active: bool,
    next: u64,
    update: bool,
}
pub(in crate::storage) struct CatalogueOwnFenceV19 {
    original: Arc<CatalogueIntentSource>,
    generation: u64,
    active: bool,
}
pub(in crate::storage) struct PreparedCatalogueOwnFenceV19 {
    original: Arc<CatalogueIntentSource>,
    generation: u64,
    active: bool,
}
impl Cache {
    pub(in crate::storage) fn prepare_catalogue_continuation_v19(
        &self,
        original: &Arc<CatalogueIntentSource>,
        actual: &ConversationRefreshPreparationV19,
        fenced: bool,
    ) -> Result<PreparedCatalogueContinuationV19, CatalogueIntentBoundaryError> {
        self.validate_repository_catalogue_intent_source(original)?;
        prepare_inner(
            original,
            actual.parent(),
            actual.identity(),
            LocalTransitionV19 {
                expected: original
                    .reservation
                    .generation
                    .checked_add(u64::from(fenced))
                    .filter(|n| *n <= i64::MAX as u64)
                    .ok_or(CatalogueIntentBoundaryError::OriginalSource)?,
                active: !fenced,
                next: original
                    .reservation
                    .generation
                    .checked_add(if fenced { 2 } else { 0 })
                    .filter(|n| *n <= i64::MAX as u64)
                    .ok_or(CatalogueIntentBoundaryError::OriginalSource)?,
                update: fenced,
            },
        )
    }
    pub(in crate::storage) fn prepare_catalogue_after_own_fence_v19(
        &self,
        own: &CatalogueOwnFenceV19,
        actual: &crate::scoped_pr_collaboration_source_v17::initial_v17::RefreshReadPreparationV19,
    ) -> Result<PreparedCatalogueContinuationV19, CatalogueIntentBoundaryError> {
        self.validate_catalogue_own_fence_v19(own)?;
        let next = own
            .generation
            .checked_add(u64::from(!own.active))
            .filter(|n| *n <= i64::MAX as u64)
            .ok_or(CatalogueIntentBoundaryError::OriginalSource)?;
        prepare_inner(
            &own.original,
            actual.parent(),
            actual.identity(),
            LocalTransitionV19 {
                expected: own.generation,
                active: own.active,
                next,
                update: !own.active,
            },
        )
    }
    pub(in crate::storage) fn prepare_catalogue_own_fence_v19(
        &self,
        original: &Arc<CatalogueIntentSource>,
        fenced: bool,
    ) -> Result<PreparedCatalogueOwnFenceV19, CatalogueIntentBoundaryError> {
        self.validate_repository_catalogue_intent_source(original)?;
        if fenced && original.reservation.generation >= i64::MAX as u64 {
            return Err(CatalogueIntentBoundaryError::OriginalSource);
        }
        Ok(PreparedCatalogueOwnFenceV19 {
            original: Arc::clone(original),
            generation: original.reservation.generation + u64::from(fenced),
            active: !fenced,
        })
    }
    /// Exact known LOCAL result of this actual native confirmed fence. Only
    /// its explicit NEW actual Core may use this profile; normal validation
    /// remains unchanged and correctly refuses the inactive predecessor.
    pub(in crate::storage) fn validate_catalogue_own_fence_v19(
        &self,
        own: &CatalogueOwnFenceV19,
    ) -> Result<(), CatalogueIntentBoundaryError> {
        let old = &own.original.reservation;
        if issuer(self)? != old.issuer {
            return Err(CatalogueIntentBoundaryError::OriginalSource);
        }
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(|_| CatalogueIntentBoundaryError::Database)?;
        let row:Option<(u64,bool)>=tx.query_row("SELECT generation,active FROM sync_reservations WHERE host=?1 AND login=?2 AND source=?3",params![old.scope.host,old.scope.login,old.sync_source],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(|_|CatalogueIntentBoundaryError::Database)?;
        if row != Some((own.generation, own.active))
            || checkpoint(&tx, &old.scope, &old.sync_source)? != old.checkpoint
        {
            return Err(CatalogueIntentBoundaryError::OriginalSource);
        }
        validate_payload(&tx, old)?;
        tx.commit()
            .map_err(|_| CatalogueIntentBoundaryError::Database)?;
        if issuer(self)? != old.issuer {
            return Err(CatalogueIntentBoundaryError::OriginalSource);
        }
        Ok(())
    }
}
fn prepare_inner(
    original: &Arc<CatalogueIntentSource>,
    parent: &PrResourceParentV1,
    identity: &crate::providers::gh::HostIdentity,
    transition: LocalTransitionV19,
) -> Result<PreparedCatalogueContinuationV19, CatalogueIntentBoundaryError> {
    if Scope::github(&identity.host, identity.account_id)
        .map_err(|_| CatalogueIntentBoundaryError::OriginalSource)?
        != *original.scope()
        || !original.source().repositories().iter().any(|r| {
            r.id == parent.repository_id()
                && r.node_id.as_deref() == Some(parent.repository_node())
                && r.full_name.eq_ignore_ascii_case(parent.repository())
        })
    {
        return Err(CatalogueIntentBoundaryError::OriginalSource);
    }
    let generation = transition.next;
    let old = &original.reservation;
    let candidate = if transition.update {
        Arc::new(CatalogueIntentSource {
            source: Arc::clone(&original.source),
            reservation: CatalogueIntentReservation {
                issuer: old.issuer.clone(),
                scope: old.scope.clone(),
                original: old.original.clone(),
                sync_source: old.sync_source.clone(),
                view_key: old.view_key.clone(),
                generation,
                checkpoint: old.checkpoint,
                expected_payload: old.expected_payload,
                intent: old.intent.clone(),
            },
        })
    } else {
        Arc::clone(original)
    };
    Ok(PreparedCatalogueContinuationV19 {
        original: Arc::clone(original),
        candidate,
        transition,
    })
}
impl PreparedCatalogueContinuationV19 {
    /// Called only AFTER exact metadata/terminal CAS in the SAME transaction.
    /// The original fence advances old→old+1/inactive; only this source gets
    /// its known old+2 fresh LOCAL reservation. All other old reads stay dead.
    pub(in crate::storage) fn apply_after_fence_v19(
        &self,
        tx: &Transaction<'_>,
    ) -> anyhow::Result<()> {
        let scope = self.original.scope();
        let source = self.original.sync_source();
        if self.transition.update {
            let changed=tx.execute("UPDATE sync_reservations SET generation=?4,active=1 WHERE host=?1 AND login=?2 AND source=?3 AND generation=?5 AND active=?6",params![scope.host,scope.login,source,self.candidate.reservation.generation,self.transition.expected,self.transition.active])?;
            if changed != 1 {
                anyhow::bail!("Exact original Catalogue fence successor changed; nothing rebound");
            }
        } else {
            let exact:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM sync_reservations WHERE host=?1 AND login=?2 AND source=?3 AND generation=?4 AND active=?5)",params![scope.host,scope.login,source,self.transition.expected,self.transition.active],|r|r.get(0))?;
            if !exact {
                anyhow::bail!("Original Catalogue source epoch changed; no successor inferred");
            }
        }
        Ok(())
    }
    pub(in crate::storage) fn into_committed_v19(self) -> Arc<CatalogueIntentSource> {
        self.candidate
    }
}
impl PreparedCatalogueOwnFenceV19 {
    pub(in crate::storage) fn bind_committed_v19(
        self,
        tx: &Transaction<'_>,
    ) -> anyhow::Result<CatalogueOwnFenceV19> {
        let scope = self.original.scope();
        let generation = self.generation;
        let matches:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM sync_reservations WHERE host=?1 AND login=?2 AND source=?3 AND generation=?4 AND active=?5)",params![scope.host,scope.login,self.original.sync_source(),generation,self.active],|r|r.get(0))?;
        if !matches {
            anyhow::bail!("Actual own-fence local source result differs; no Refresh descriptor permission inferred");
        }
        Ok(CatalogueOwnFenceV19 {
            original: self.original,
            generation,
            active: self.active,
        })
    }
}
