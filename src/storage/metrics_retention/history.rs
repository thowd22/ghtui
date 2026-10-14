//! Source-only reservation/proof conversion. Declare only with the atomic core
//! history-field/commit/projection compatibility hunk, never as a public bool.
use super::*;
use crate::providers::gh::{
    ApiVisibleMetricBackfill, MetricsBackfillBinding, MetricsBackfillCategory,
    MetricsBackfillRequest,
};
use std::{fmt, os::unix::fs::MetadataExt};

fn issuer_sha256(cache: &Cache) -> Result<String> {
    match cache.connection.path().filter(|path| !path.is_empty()) {
        Some(path) => {
            let path = std::fs::canonicalize(path)?;
            let metadata = std::fs::metadata(&path)?;
            // Only the fingerprint is carried by the provider; path/inode/SQL
            // details never reach gh or public Debug output.
            digest(&("file", path, metadata.dev(), metadata.ino()))
        }
        None => digest(&("memory", std::ptr::from_ref(&cache.connection) as usize)),
    }
}
fn current_reservation(cache: &Cache, reservation: &MetricEventsReservation) -> Result<()> {
    let saved = &reservation.saved;
    saved.target.validate()?;
    saved.intent.validate()?;
    saved.limits.validate()?;
    if reservation.issuer != Issuer::from_cache(cache)?
        || !positive(saved.generation)
        || saved.intent_sha256
            != digest(&(
                &saved.target,
                &saved.intent,
                &saved.limits,
                saved.generation,
            ))?
    {
        bail!("Original metrics reservation belongs to another issuer/intent/generation");
    }
    let transaction = cache.connection.unchecked_transaction()?;
    let epoch: Option<(u64, bool)> = transaction
        .query_row(
            "SELECT generation,active FROM sync_reservations WHERE host=?1 AND login=?2 AND source=?3",
            params![saved.target.scope.host, saved.target.scope.login, saved.target.source()?],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let row: Option<(String, bool, u64, Option<String>)> = transaction
        .query_row(
            "SELECT repository,complete,length(CAST(payload AS BLOB)),
             CASE WHEN length(CAST(payload AS BLOB))<=?5 THEN payload ELSE NULL END
             FROM records WHERE host=?1 AND login=?2 AND kind=?3 AND id=?4",
            params![
                saved.target.scope.host,
                saved.target.scope.login,
                RESERVATION,
                saved.target.source()?,
                saved.limits.max_state_bytes
            ],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let (repository, complete, bytes, payload) =
        row.context("Original metric reservation Record absent")?;
    if epoch != Some((saved.generation, true))
        || repository != saved.target.repository
        || complete
        || bytes > saved.limits.max_state_bytes as u64
    {
        bail!(
            "Original backfill reservation was replaced/fenced/overbudget before proof conversion"
        );
    }
    let payload: Value =
        serde_json::from_str(&payload.context("Original intent bytes not copied")?)?;
    if payload != serde_json::to_value(saved)? {
        bail!("Original reserved metric interval/target/intent was edited; no history authority");
    }
    transaction.commit()?;
    Ok(())
}
fn category(value: EventCategory) -> MetricsBackfillCategory {
    match value {
        EventCategory::Reviews => MetricsBackfillCategory::Reviews,
        EventCategory::Timeline => MetricsBackfillCategory::Timeline,
    }
}
impl MetricEventsReservation {
    /// Read the actual current immutable SQL reservation before issuing a
    /// non-Deserialize transport token. Positive metadata alone is not access.
    pub fn backfill_request(
        &self,
        cache: &Cache,
        identity: &HostIdentity,
        parent: &MetricsPrObservation,
        authority: &mut impl MetricsRetentionAuthority,
    ) -> Result<MetricsBackfillRequest> {
        self.target().validate_provider_parent(identity, parent)?;
        authority.verify(self.target(), MetricAuthorityPhase::BeforeRead)?;
        current_reservation(cache, self)?;
        let binding = MetricsBackfillBinding {
            source: self.target().source()?,
            generation: self.generation(),
            intent_sha256: self.saved.intent_sha256.clone(),
            issuer_sha256: issuer_sha256(cache)?,
            requested_from: self.original_intent().requested_from.context(
                "Available-history intent must name the genuine canonical creation boundary",
            )?,
            requested_through: self.original_intent().requested_through,
        };
        let request = MetricsBackfillRequest::issue(
            identity,
            parent,
            category(self.target().category),
            binding,
        )
        .map_err(anyhow::Error::msg)?;
        authority.verify(self.target(), MetricAuthorityPhase::AfterRead)?;
        current_reservation(cache, self)?;
        Ok(request)
    }
}

/// Immutable identity/time/target attribution used to preserve proven earlier
/// available intervals across partial rereads without pruning known events.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct HistoryMember {
    #[serde(rename = "n")]
    provider_node: String,
    #[serde(rename = "t")]
    at: u64,
    #[serde(rename = "k")]
    kind: EventKind,
    #[serde(rename = "a")]
    actor_id: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct StoredAvailableHistory {
    target: MetricEventTarget,
    original_generation: u64,
    original_intent_sha256: String,
    from: u64,
    through: u64,
    observed_from: u64,
    observed_through: u64,
    limitations: Vec<String>,
    members: BTreeMap<u64, HistoryMember>,
    members_sha256: String,
}
impl StoredAvailableHistory {
    pub(super) fn carry_forward(
        old: Option<Self>,
        new: Option<Self>,
        saved: &ReservedIntent,
        facts: &[Fact],
    ) -> Result<Option<Self>> {
        if let Some(new) = &new {
            if new.target != saved.target
                || new.original_generation != saved.generation
                || new.original_intent_sha256 != saved.intent_sha256
                || saved.intent.requested_from != Some(new.from)
                || saved.intent.requested_through != new.through
            {
                bail!(
                    "Positive history does not belong to this exact original reserved publication"
                );
            }
        }
        // Validate prior and new proof members before composing them. A current
        // observation is not itself an available-history proof, even if another
        // interval/category already has one.
        for history in [&old, &new].into_iter().flatten() {
            history.validate_known_members(&saved.target, saved.generation, facts)?;
        }
        let chosen = match (old, new) {
            (Some(old), Some(new)) => {
                if old.from != new.from {
                    bail!("Canonical original creation boundary changed");
                }
                let (mut chosen, earlier) = if new.through >= old.through {
                    (new, old)
                } else {
                    (old, new)
                };
                // Preserve facts previously proven by an exhausted API view,
                // including ones subsequently erased from that API. Never
                // bless arbitrary retained facts from a partial observation.
                for (id, member) in earlier.members {
                    if member.at >= chosen.from && member.at <= chosen.through {
                        chosen.members.entry(id).or_insert(member);
                    }
                }
                chosen.members_sha256 = digest(&chosen.members)?;
                Some(chosen)
            }
            (old, new) => new.or(old),
        };
        if let Some(history) = &chosen {
            history.validate_known_members(&saved.target, saved.generation, facts)?;
            if !history.covers_retained_interval(facts) {
                // A capped reread may discover an earlier review/request or a
                // removal inside a previously proven interval. Keep every
                // observation, but drop completeness until another genuine
                // exhausted traversal proves that in-interval membership.
                return Ok(None);
            }
        }
        Ok(chosen)
    }
    pub(super) fn validate(
        &self,
        target: &MetricEventTarget,
        current_generation: u64,
        facts: &[Fact],
    ) -> Result<()> {
        self.validate_known_members(target, current_generation, facts)?;
        if !self.covers_retained_interval(facts) {
            bail!("Retained in-interval observation lacks original available-history membership");
        }
        Ok(())
    }
    fn covers_retained_interval(&self, facts: &[Fact]) -> bool {
        facts
            .iter()
            .filter(|fact| fact.event.at >= self.from && fact.event.at <= self.through)
            .all(|fact| self.members.contains_key(&fact.event.id))
    }
    fn validate_known_members(
        &self,
        target: &MetricEventTarget,
        current_generation: u64,
        facts: &[Fact],
    ) -> Result<()> {
        if &self.target != target
            || !positive(self.original_generation)
            || self.original_generation > current_generation
            || self.from > self.through
            || self.through > self.observed_from
            || self.observed_from > self.observed_through
            || self.original_intent_sha256.len() != 64
            || !self
                .original_intent_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || self.limitations.is_empty()
            || self.limitations.len() > 32
            || self
                .limitations
                .iter()
                .any(|limitation| limitation.len() > 4096)
            || self.members.len() > 10000
            || digest(&self.members)? != self.members_sha256
        {
            bail!("Retained API-visible history original target/interval/member/limitation proof is invalid");
        }
        let current = facts
            .iter()
            .map(|fact| (fact.event.id, fact))
            .collect::<BTreeMap<_, _>>();
        for (id, member) in &self.members {
            let fact = current
                .get(id)
                .context("Previously proven historical member was removed")?;
            if member.at < self.from
                || member.at > self.through
                || member.provider_node != fact.provider_node
                || member.at != fact.event.at
                || !compatible_history_kind(&member.kind, &fact.event.kind)
                || matches!((member.actor_id,fact.event.actor.account_id),(Some(old),Some(new))if old!=new)
            {
                bail!("Previously proven original historical time/node/target/actor was changed");
            }
        }
        Ok(())
    }
    pub(super) fn project(
        &self,
        frame: &StoredMetricFrame,
        scope: &MetricsScope,
        parent: &MetricsPrObservation,
    ) -> Result<EventCollection> {
        self.validate(frame.target(), frame.generation(), &frame.facts)?;
        if parent.created_at.as_ref().map(|at| at.unix_seconds()) != Some(self.from) {
            bail!("Current canonical creation boundary differs from original historical proof");
        }
        let mut collection = frame.validated_current_projection(scope, parent)?;
        let through = self.through.min(frame.original_intent().requested_through);
        collection.events.retain(|event| {
            event.at >= self.from && event.at <= through && self.members.contains_key(&event.id)
        });
        collection.from = Some(self.from);
        collection.through = Some(through);
        collection.complete = true;
        collection.basis = crate::metrics::HistoryBasis::ApiVisibleRetainedBackfill {
            limitations: self.limitations.clone(),
        };
        collection
            .reasons
            .retain(|reason| reason != HISTORY_UNAVAILABLE);
        collection.reasons.extend(self.limitations.clone());
        collection.reasons.sort();
        collection.reasons.dedup();
        Ok(collection)
    }
}
fn compatible_history_kind(old: &EventKind, new: &EventKind) -> bool {
    old == new
        || matches!((old,new),(
        EventKind::ReviewSubmitted{review_id:old,..},
        EventKind::ReviewSubmitted{review_id:new,state:SubmittedReviewState::Dismissed}
    )if old==new)
}
/// Positive available-history batch has no public constructor/complete bool.
/// It is NOT committed evidence until the atomic private core commit succeeds.
pub struct AvailableMetricCategoryBatch {
    current: MetricCategoryBatch,
    history: StoredAvailableHistory,
}
impl fmt::Debug for AvailableMetricCategoryBatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AvailableMetricCategoryBatch(<verified original traversal>)")
    }
}
impl AvailableMetricCategoryBatch {
    pub fn from_backfill(
        cache: &Cache,
        reservation: &MetricEventsReservation,
        backfill: &ApiVisibleMetricBackfill,
        authority: &mut impl MetricsRetentionAuthority,
    ) -> Result<Option<Self>> {
        authority.verify(reservation.target(), MetricAuthorityPhase::BeforeRead)?;
        current_reservation(cache, reservation)?;
        let Some(proof) = backfill.category_proof(category(reservation.target().category)) else {
            authority.verify(reservation.target(), MetricAuthorityPhase::AfterRead)?;
            current_reservation(cache, reservation)?;
            return Ok(None); // Partial/unsupported observations remain the ordinary current bridge.
        };
        proof
            .validates_observation(backfill.observations())
            .map_err(anyhow::Error::msg)?;
        let binding = proof.binding();
        if binding.source != reservation.target().source()?
            || binding.generation != reservation.generation()
            || binding.intent_sha256 != reservation.saved.intent_sha256
            || binding.issuer_sha256 != issuer_sha256(cache)?
            || reservation.original_intent().requested_from != Some(binding.requested_from)
            || reservation.original_intent().requested_through != binding.requested_through
        {
            bail!(
                "Backfill completion belongs to another original cache/generation/intent/category"
            );
        }
        let current = MetricCategoryBatch::from_provider(
            backfill.observations(),
            reservation.target().category,
        )?;
        if current.target != *reservation.target()
            || !current.coverage.enumeration_complete
            || !current.coverage.semantic_complete
            || current.coverage.unavailable
        {
            bail!(
                "Available backfill category differs from actual semantically complete observation"
            );
        }
        let members = current
            .facts
            .iter()
            .filter(|fact| fact.event.at <= binding.requested_through)
            .map(|fact| {
                (
                    fact.event.id,
                    HistoryMember {
                        provider_node: fact.provider_node.clone(),
                        at: fact.event.at,
                        kind: fact.event.kind.clone(),
                        actor_id: fact.event.actor.account_id,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        let history = StoredAvailableHistory {
            target: current.target.clone(),
            original_generation: reservation.generation(),
            original_intent_sha256: reservation.saved.intent_sha256.clone(),
            from: binding.requested_from,
            through: binding.requested_through,
            observed_from: current.observed_from,
            observed_through: current.observed_through,
            limitations: proof
                .limitations()
                .iter()
                .map(|limitation| (*limitation).to_owned())
                .collect(),
            members_sha256: digest(&members)?,
            members,
        };
        history.validate(
            reservation.target(),
            reservation.generation(),
            &current.facts,
        )?;
        if serde_json::to_vec(&history)?.len() > reservation.saved.limits.max_state_bytes {
            // API traversal can be complete while retained proof budget is not.
            // That cannot authorize positive history or silently drop members.
            authority.verify(reservation.target(), MetricAuthorityPhase::AfterRead)?;
            current_reservation(cache, reservation)?;
            return Ok(None);
        }
        authority.verify(reservation.target(), MetricAuthorityPhase::AfterRead)?;
        current_reservation(cache, reservation)?;
        Ok(Some(Self { current, history }))
    }
    pub fn interval(&self) -> (u64, u64) {
        (self.history.from, self.history.through)
    }
    pub(super) fn into_parts(self) -> (MetricCategoryBatch, StoredAvailableHistory) {
        (self.current, self.history)
    }
}

#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;
