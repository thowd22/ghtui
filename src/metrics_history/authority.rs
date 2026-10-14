use super::*;
use crate::storage::{MetricAuthorityPhase, MetricsRetentionAuthority};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};

struct ExpectedSource {
    target: MetricEventTarget,
    generation: u64,
    prior_checkpoint: Option<u64>,
    committed: bool,
    reservation_payload: Value,
}
pub(super) struct SqlExpectations {
    as_of: u64,
    limits: MetricsRetentionLimits,
    sources: BTreeMap<String, ExpectedSource>,
}
impl SqlExpectations {
    pub fn new(as_of: u64, limits: &MetricsRetentionLimits) -> Self {
        Self {
            as_of,
            limits: limits.clone(),
            sources: BTreeMap::new(),
        }
    }
    pub(super) fn check(
        &mut self,
        database: &Path,
        target: &MetricEventTarget,
        parent: &MetricsPrObservation,
        phase: MetricAuthorityPhase,
    ) -> Result<()> {
        let connection = Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let tx = connection.unchecked_transaction()?;
        let source = target.source()?;
        let reservation: Option<(u64, bool)> = tx.query_row(
            "SELECT generation,active FROM sync_reservations WHERE host=?1 AND login=?2 AND source=?3",
            params![target.scope().host, target.scope().login, source],
            |row| Ok((row.get(0)?,row.get(1)?)),
        ).optional()?;
        let checkpoint: Option<u64> = tx
            .query_row(
                "SELECT generation FROM checkpoints WHERE host=?1 AND login=?2 AND source=?3",
                params![target.scope().host, target.scope().login, source],
                |row| row.get(0),
            )
            .optional()?;
        if phase == MetricAuthorityPhase::BeforeReserve {
            if self.sources.contains_key(&source) {
                bail!("Original history source was already reserved; no implicit resend or rebase");
            }
            let prior = reservation
                .map_or(0, |row| row.0)
                .max(checkpoint.unwrap_or(0));
            let generation = prior
                .checked_add(1)
                .filter(|generation| *generation <= i64::MAX as u64)
                .context("Original history source generation exhausted")?;
            let intent = MetricReadIntent {
                requested_from: Some(
                    parent
                        .created_at
                        .as_ref()
                        .context("Original historical creation boundary missing")?
                        .unix_seconds(),
                ),
                requested_through: self.as_of,
            };
            let canonical = serde_json::to_value((target, &intent, &self.limits, generation))?;
            let sha = format!("{:x}", Sha256::digest(serde_json::to_vec(&canonical)?));
            self.sources.insert(source, ExpectedSource {
                target: target.clone(), generation, prior_checkpoint: checkpoint, committed: false,
                reservation_payload: json!({"target":target,"intent":intent,"limits":self.limits,"generation":generation,"intent_sha256":sha}),
            });
            tx.commit()?;
            return Ok(());
        }
        let expected = self
            .sources
            .get_mut(&source)
            .context("History source has no original pre-reserve SQL expectation")?;
        let checkpoint_expected =
            if phase == MetricAuthorityPhase::AfterCommit || expected.committed {
                Some(expected.generation)
            } else {
                expected.prior_checkpoint
            };
        if &expected.target != target
            || reservation != Some((expected.generation, true))
            || checkpoint != checkpoint_expected
        {
            bail!("Original metric reservation/checkpoint was fenced/replaced; no earlier proof is reused");
        }
        let row: Option<(String,bool,u64,Option<String>)> = tx.query_row(
            "SELECT repository,complete,length(CAST(payload AS BLOB)),CASE WHEN length(CAST(payload AS BLOB))<=?5 THEN payload ELSE NULL END
             FROM records WHERE host=?1 AND login=?2 AND kind='metrics-reservation-v1' AND id=?3 AND repository=?4",
            params![target.scope().host,target.scope().login,source,target.repository(),self.limits.max_state_bytes],
            |row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)),
        ).optional()?;
        let (repository, complete, bytes, payload) =
            row.context("Original immutable history intent record disappeared")?;
        if repository != target.repository()
            || complete
            || bytes > self.limits.max_state_bytes as u64
            || serde_json::from_str::<Value>(
                &payload.context("Overbudget original history intent not copied")?,
            )? != expected.reservation_payload
        {
            bail!("Original history target/creation/as-of/generation/intent changed; source remains hidden");
        }
        if phase == MetricAuthorityPhase::AfterCommit {
            expected.committed = true;
        }
        tx.commit()?;
        Ok(())
    }
}

enum Receipt<'a> {
    Single {
        original: &'a MetricsPrObservation,
        receipt: &'a MetricsScopeReceipt,
    },
    Batch(&'a MetricsScopeReceiptBatch),
}
pub(super) struct AuthorityContext<'a> {
    request: &'a MetricsHistoryRequest,
    identity: &'a HostIdentity,
    guard: &'a MetricsReadGuard,
    budget: &'a HistoryBudget,
    cancel: &'a CancellationToken,
}
impl<'a> AuthorityContext<'a> {
    pub fn new(
        request: &'a MetricsHistoryRequest,
        identity: &'a HostIdentity,
        guard: &'a MetricsReadGuard,
        budget: &'a HistoryBudget,
        cancel: &'a CancellationToken,
    ) -> Self {
        Self {
            request,
            identity,
            guard,
            budget,
            cancel,
        }
    }
}
pub(super) struct HistoryAuthority<'a> {
    request: &'a MetricsHistoryRequest,
    identity: &'a HostIdentity,
    guard: &'a MetricsReadGuard,
    receipt: Receipt<'a>,
    expected: &'a mut SqlExpectations,
    budget: &'a HistoryBudget,
    cancel: &'a CancellationToken,
}
impl<'a> HistoryAuthority<'a> {
    pub fn single(
        context: AuthorityContext<'a>,
        original: &'a MetricsPrObservation,
        receipt: &'a MetricsScopeReceipt,
        expected: &'a mut SqlExpectations,
    ) -> Self {
        let AuthorityContext {
            request,
            identity,
            guard,
            budget,
            cancel,
        } = context;
        Self {
            request,
            identity,
            guard,
            receipt: Receipt::Single { original, receipt },
            expected,
            budget,
            cancel,
        }
    }
    pub fn batch(
        context: AuthorityContext<'a>,
        receipt: &'a MetricsScopeReceiptBatch,
        expected: &'a mut SqlExpectations,
    ) -> Self {
        let AuthorityContext {
            request,
            identity,
            guard,
            budget,
            cancel,
        } = context;
        Self {
            request,
            identity,
            guard,
            receipt: Receipt::Batch(receipt),
            expected,
            budget,
            cancel,
        }
    }
}
impl MetricsRetentionAuthority for HistoryAuthority<'_> {
    fn verify(&mut self, target: &MetricEventTarget, phase: MetricAuthorityPhase) -> Result<()> {
        self.budget.check(self.cancel)?;
        let original = match &self.receipt {
            Receipt::Single { original, receipt } => {
                receipt.validate_for(
                    self.identity,
                    original,
                    self.request.limits.sql_lease,
                    self.cancel,
                )?;
                *original
            }
            Receipt::Batch(batch) => {
                batch.validate_repositories(
                    self.identity,
                    &self.request.workspace.repositories,
                    self.request.limits.sql_lease,
                    self.cancel,
                )?;
                let receipt = batch
                    .receipts()
                    .iter()
                    .find(|receipt| {
                        target
                            .validate_provider_parent(self.identity, receipt.parent())
                            .is_ok()
                    })
                    .context("Final whole-scope receipt has no exact original parent")?;
                receipt.validate_for(
                    self.identity,
                    receipt.parent(),
                    self.request.limits.sql_lease,
                    self.cancel,
                )?;
                self.request
                    .workspace
                    .observations
                    .iter()
                    .find(|parent| {
                        target
                            .validate_provider_parent(self.identity, parent)
                            .is_ok()
                    })
                    .context("Original canonical parent disappeared during publication")?
            }
        };
        target.validate_provider_parent(self.identity, original)?;
        self.guard
            .validate(&self.request.database)
            .map_err(anyhow::Error::msg)?;
        self.guard
            .validate_parents(&self.request.workspace.observations)
            .map_err(anyhow::Error::msg)?;
        self.expected
            .check(&self.request.database, target, original, phase)?;
        self.guard
            .validate_parents(&self.request.workspace.observations)
            .map_err(anyhow::Error::msg)?;
        self.budget.check(self.cancel)?;
        // Local metadata reads may consume the short lease. Never treat a
        // receipt as fresh merely because it was checked before SQL copying.
        match &self.receipt {
            Receipt::Single { receipt, .. } => receipt.validate_for(
                self.identity,
                original,
                self.request.limits.sql_lease,
                self.cancel,
            )?,
            Receipt::Batch(batch) => batch.validate_repositories(
                self.identity,
                &self.request.workspace.repositories,
                self.request.limits.sql_lease,
                self.cancel,
            )?,
        }
        Ok(())
    }
}
