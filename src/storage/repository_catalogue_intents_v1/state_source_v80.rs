//! Purpose-closed LOCAL continuation for an actual complete State source read.
//! SAME genuine Read/Active allocation, never access renewal. Normal next PR
//! actions still need their unchanged fresh repository/Core/PR/UID guards.
use super::*;
use crate::{providers::gh::StateSourceReadV80, state_inbox_sync_v76::StateSourceOriginV80};

pub(in crate::storage) struct PreparedCatalogueStateSourceV80 {
    original: Arc<CatalogueIntentSource>,
    candidate: Arc<CatalogueIntentSource>,
    reserved: u64,
    active: bool,
}
impl PreparedCatalogueStateSourceV80 {
    pub(in crate::storage) fn prepare(
        cache: &Cache,
        read: &StateSourceReadV80,
    ) -> anyhow::Result<Option<Self>> {
        let actual = read.original().original();
        let StateSourceOriginV80::Catalogue { source, .. } = actual.origin() else {
            return Ok(None);
        };
        let old = &source.reservation;
        if issuer(cache)? != old.issuer
            || actual.scope() != &old.scope
            || read.workspace().scope()? != old.scope
        {
            anyhow::bail!("Actual State source catalogue file/account changed");
        }
        let tx = cache.connection.unchecked_transaction()?;
        // Exact actual selection/payload remains the captured one. A local
        // fence may have invalidated its reservation, never its genuine Read.
        validate_payload(&tx, old)?;
        if checkpoint(&tx, &old.scope, &old.sync_source)? != old.checkpoint {
            anyhow::bail!("Actual catalogue checkpoint changed before State read preparation");
        }
        let (reserved,active,typed):(u64,bool,bool)=tx.query_row("SELECT generation,active,typeof(generation)='integer' AND generation>0 AND typeof(active)='integer' AND active IN(0,1) FROM sync_reservations WHERE host=?1 AND login=?2 AND source=?3",params![old.scope.host,old.scope.login,old.sync_source],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        let next = reserved
            .checked_add(2)
            .filter(|n| *n <= i64::MAX as u64)
            .ok_or_else(|| anyhow::anyhow!("Actual State catalogue successor exhausted"))?;
        if !typed {
            anyhow::bail!("Actual State catalogue reservation raw types changed");
        }
        tx.commit()?;
        // Caller charged actual source/intent allocation before this factory.
        let candidate = Arc::new(CatalogueIntentSource {
            source: Arc::clone(&source.source),
            reservation: CatalogueIntentReservation {
                issuer: old.issuer.clone(),
                scope: old.scope.clone(),
                original: old.original.clone(),
                sync_source: old.sync_source.clone(),
                view_key: old.view_key.clone(),
                generation: next,
                checkpoint: old.checkpoint,
                expected_payload: old.expected_payload,
                intent: old.intent.clone(),
            },
        });
        Ok(Some(Self {
            original: Arc::clone(source),
            candidate,
            reserved,
            active,
        }))
    }
    pub(in crate::storage) fn apply_after_fence(&self, tx: &Transaction<'_>) -> anyhow::Result<()> {
        let old = &self.original.reservation;
        validate_payload(tx, old)?;
        if checkpoint(tx, &old.scope, &old.sync_source)? != old.checkpoint {
            anyhow::bail!("State catalogue checkpoint changed before successor commit");
        }
        let expected = self
            .reserved
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("State catalogue fence overflow"))?;
        let n=tx.execute("UPDATE sync_reservations SET generation=?4,active=1 WHERE host=?1 AND login=?2 AND source=?3 AND typeof(generation)='integer' AND generation=?5 AND typeof(active)='integer' AND active=0",params![old.scope.host,old.scope.login,old.sync_source,self.candidate.reservation.generation,expected])?;
        if n != 1 {
            anyhow::bail!("Exact State catalogue fenced successor changed");
        }
        Ok(())
    }
    pub(in crate::storage) fn validate_before_fence(
        &self,
        tx: &Transaction<'_>,
    ) -> anyhow::Result<()> {
        let old = &self.original.reservation;
        let actual:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM sync_reservations WHERE host=?1 AND login=?2 AND source=?3 AND typeof(generation)='integer' AND generation=?4 AND typeof(active)='integer' AND active=?5)",params![old.scope.host,old.scope.login,old.sync_source,self.reserved,self.active],|r|r.get(0))?;
        if !actual {
            anyhow::bail!("Actual State catalogue source changed; no recapture");
        }
        Ok(())
    }
    pub(in crate::storage) fn committed(self) -> Arc<CatalogueIntentSource> {
        self.candidate
    }
}
