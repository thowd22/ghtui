//! Exact original DB/query/account generation. Never remote permission or a
//! generic source/transaction callback. Clear invalidates this existing key.
use super::repository_catalogue_intents_v1::{issuer, Issuer};
use super::*;
use crate::providers::{
    gh::notifications_v63::NotificationReadContextV63, runner::CancellationToken,
};
use std::time::Instant;

pub(crate) struct NotificationSourceWitnessV63 {
    issuer: Issuer,
    scope: Scope,
    source: String,
    generation: u64,
    confirmed_attempt: i64,
}
impl std::fmt::Debug for NotificationSourceWitnessV63 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "NotificationSourceWitnessV63(<original DB/account/query generation; not access>)",
        )
    }
}
impl NotificationSourceWitnessV63 {
    pub(super) fn issuer_v63(&self) -> &Issuer {
        &self.issuer
    }
    pub(super) fn validate_transaction_v63(
        &self,
        transaction: &rusqlite::Transaction<'_>,
    ) -> Result<()> {
        let current:Option<u64>=transaction.query_row(
            "SELECT generation FROM sync_reservations WHERE host=?1 AND login=?2 AND source=?3 AND active=1",
            params![self.scope.host,self.scope.login,self.source],|row|row.get(0)).optional()?;
        if current != Some(self.generation) || !self.issuer.file_still_matches_v63() {
            bail!("Original notification SQL generation/database changed; no claim committed");
        }
        let confirmed: i64 = transaction.query_row(
            "SELECT COALESCE(MAX(id),0) FROM notification_write_attempts_v63 WHERE host=?1 AND account_id=?2 AND receipt_json IS NOT NULL",
            params![self.scope.host, self.scope.login.strip_prefix("user-id:").context("Numeric notification account unavailable")?],
            |row| row.get(0),
        )?;
        if confirmed != self.confirmed_attempt {
            bail!(
                "A newer actual notification receipt retired this observation; refresh explicitly"
            );
        }
        Ok(())
    }
    pub(crate) fn scope(&self) -> &Scope {
        &self.scope
    }
    pub(crate) fn source(&self) -> &str {
        &self.source
    }
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }
}
fn live(token: &CancellationToken, deadline: Instant) -> Result<()> {
    if token.is_cancelled() || Instant::now() >= deadline {
        bail!("Original notification SQL phase cancelled/expired; no source renewed");
    }
    Ok(())
}
impl Cache {
    pub(crate) fn reserve_notification_source_v63(
        &mut self,
        context: &NotificationReadContextV63,
        previous: Option<&NotificationSourceWitnessV63>,
        token: &CancellationToken,
        deadline: Instant,
    ) -> Result<NotificationSourceWitnessV63> {
        live(token, deadline)?;
        let scope = Scope::github(
            context.identity().host.clone(),
            context.identity().account_id,
        )?;
        let original = issuer(self).map_err(anyhow::Error::new)?;
        if let Some(old) = previous {
            self.validate_notification_source_v63(old, token, deadline)?;
            if old.scope != scope || old.source != context.sync_source() {
                bail!("Original notification query/account changed; explicit new load required");
            }
        }
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        live(token, deadline)?;
        if !original.file_still_matches_v63() {
            bail!("Original notification database file changed");
        }
        let source = context.sync_source();
        let reservation:Option<(u64,bool)>=transaction.query_row("SELECT generation,active FROM sync_reservations WHERE host=?1 AND login=?2 AND source=?3",params![scope.host,scope.login,source],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if previous.is_some_and(|old| reservation != Some((old.generation, true))) {
            bail!("Original notification generation changed while awaiting SQL; no rebase");
        }
        let published: Option<u64> = transaction
            .query_row(
                "SELECT generation FROM checkpoints WHERE host=?1 AND login=?2 AND source=?3",
                params![scope.host, scope.login, source],
                |r| r.get(0),
            )
            .optional()?;
        let generation = reservation
            .map_or(0, |v| v.0)
            .max(published.unwrap_or(0))
            .checked_add(1)
            .filter(|v| *v <= i64::MAX as u64)
            .context("Notification generation exhausted")?;
        let confirmed_attempt:i64=transaction.query_row("SELECT COALESCE(MAX(id),0) FROM notification_write_attempts_v63 WHERE host=?1 AND account_id=?2 AND receipt_json IS NOT NULL",
            params![scope.host,context.identity().account_id],|row|row.get(0))?;
        transaction.execute("INSERT INTO sync_reservations VALUES(?1,?2,?3,?4,1) ON CONFLICT(host,login,source) DO UPDATE SET generation=excluded.generation,active=1",params![scope.host,scope.login,source,generation])?;
        live(token, deadline)?;
        if !original.file_still_matches_v63() {
            bail!("Original notification database file changed before commit");
        }
        transaction.commit()?;
        live(token, deadline)?;
        if issuer(self).map_err(anyhow::Error::new)? != original {
            bail!("Notification database issuer changed after commit; private result hidden");
        }
        Ok(NotificationSourceWitnessV63 {
            issuer: original,
            scope,
            source: source.to_owned(),
            generation,
            confirmed_attempt,
        })
    }
    pub(crate) fn validate_notification_source_v63(
        &self,
        original: &NotificationSourceWitnessV63,
        token: &CancellationToken,
        deadline: Instant,
    ) -> Result<()> {
        live(token, deadline)?;
        self.validate_notification_source_allocation_v63(original)?;
        live(token, deadline)
    }
    /// Lasting local allocation/generation policy only. No old request clock
    /// is renewed and this method performs no remote access or publication.
    pub(crate) fn validate_notification_source_allocation_v63(
        &self,
        original: &NotificationSourceWitnessV63,
    ) -> Result<()> {
        if issuer(self).map_err(anyhow::Error::new)? != original.issuer
            || self.sync_reservation(&original.scope, &original.source)?
                != Some(original.generation)
        {
            bail!("Original notification database/account/query generation no longer current");
        }
        let confirmed:i64=self.connection.query_row("SELECT COALESCE(MAX(id),0) FROM notification_write_attempts_v63 WHERE host=?1 AND account_id=?2 AND receipt_json IS NOT NULL",
            params![original.scope.host,original.scope.login.strip_prefix("user-id:").context("Numeric notification account unavailable")?],|row|row.get(0))?;
        if confirmed != original.confirmed_attempt {
            bail!(
                "A newer actual notification receipt retired this observation; refresh explicitly"
            );
        }
        if !original.issuer.file_still_matches_v63() {
            bail!("Original notification database file changed during validation");
        }
        Ok(())
    }
}
