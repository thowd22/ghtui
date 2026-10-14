use super::*;

fn bounded_record(
    connection: &Connection,
    target: &MetricEventTarget,
    kind: &str,
    id: &str,
    limit: usize,
) -> Result<Option<Value>> {
    let row: Option<(String, bool, u64, Option<String>)> = connection
        .query_row(
            "SELECT repository,complete,length(CAST(payload AS BLOB)),
             CASE WHEN length(CAST(payload AS BLOB))<=?5 THEN payload ELSE NULL END
             FROM records WHERE host=?1 AND login=?2 AND kind=?3 AND id=?4",
            params![target.scope.host, target.scope.login, kind, id, limit],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let Some((repository, complete, bytes, payload)) = row else {
        return Ok(None);
    };
    if repository != target.repository || complete || bytes > limit as u64 {
        bail!("Metric record repository/completeness/UTF8 footprint is invalid");
    }
    Ok(Some(serde_json::from_str(&payload.context(
        "Oversized metric record was not copied or parsed",
    )?)?))
}
fn put_record(
    transaction: &rusqlite::Transaction<'_>,
    target: &MetricEventTarget,
    kind: &str,
    id: &str,
    payload: &impl Serialize,
) -> Result<()> {
    transaction.execute(
        "INSERT INTO records VALUES(?1,?2,?3,?4,?5,?6,?7,NULL,0)
         ON CONFLICT(host,login,kind,id) DO UPDATE SET repository=excluded.repository,
         payload=excluded.payload,fetched_at=excluded.fetched_at,source_updated_at=NULL,complete=0",
        params![
            target.scope.host,
            target.scope.login,
            kind,
            id,
            target.repository,
            serde_json::to_string(&serde_json::to_value(payload)?)?,
            now()
        ],
    )?;
    Ok(())
}
pub(super) fn event_key(source: &str, fact: &Fact) -> Result<String> {
    digest(&(source, fact.event.id))
}
fn validate_fact(target: &MetricEventTarget, fact: &Fact) -> Result<()> {
    if fact.source != target.source()?
        || !positive(fact.event.id)
        || !opaque(&fact.provider_node)
        || fact.event.actor.account_id == Some(0)
    {
        bail!("Retained event lacks its exact source/positive ID/opaque node/actor identity");
    }
    match (&target.category, &fact.event.kind) {
        (EventCategory::Reviews, EventKind::ReviewSubmitted { review_id, .. })
            if *review_id == fact.event.id => {}
        (
            EventCategory::Timeline,
            EventKind::ReviewRequested { reviewer_id }
            | EventKind::ReviewRequestRemoved { reviewer_id },
        ) if positive(*reviewer_id) => {}
        (
            EventCategory::Timeline,
            EventKind::Merged
            | EventKind::Closed
            | EventKind::Reopened
            | EventKind::ReadyForReview
            | EventKind::ConvertedToDraft,
        ) => {}
        _ => bail!("Retained event kind/ID belongs to another original metric category"),
    }
    Ok(())
}
fn validate_saved(saved: &ReservedIntent) -> Result<()> {
    saved.target.validate()?;
    saved.intent.validate()?;
    saved.limits.validate()?;
    if !positive(saved.generation)
        || saved.intent_sha256
            != digest(&(
                &saved.target,
                &saved.intent,
                &saved.limits,
                saved.generation,
            ))?
    {
        bail!("Original metric reservation intent/checksum/generation is invalid");
    }
    Ok(())
}
fn validate_coverage(coverage: &ObservationCoverage) -> Result<()> {
    if coverage.fetched_pages > 100
        || coverage.fetched_rows > 10000
        || coverage.excluded_rows > coverage.fetched_rows
        || coverage.response_bytes > 32 * 1024 * 1024
        || coverage.reasons.len() > 64
        || coverage.reasons.iter().any(|reason| reason.len() > 1024)
        || coverage.semantic_complete
            && (!coverage.enumeration_complete
                || coverage.unavailable
                || coverage.excluded_rows > 0)
    {
        bail!("Metric observation coverage contradicts its explicit producer budgets");
    }
    Ok(())
}
fn reservation_epoch(connection: &Connection, target: &MetricEventTarget) -> Result<(u64, bool)> {
    connection
        .query_row(
            "SELECT generation,active FROM sync_reservations WHERE host=?1 AND login=?2 AND source=?3",
            params![target.scope.host, target.scope.login, target.source()?],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .context("Metric source reservation is absent; no generation authority exists")
}

/// Capture under the caller's read/write transaction, with SQL byte/count
/// preflight before any event payload is selected or deserialized.
fn capture(
    connection: &Connection,
    issuer: Issuer,
    target: &MetricEventTarget,
    limits: &MetricsRetentionLimits,
) -> Result<Option<StoredMetricFrame>> {
    target.validate()?;
    limits.validate()?;
    let source = target.source()?;
    let checkpoint: Option<(u64, bool)> = connection
        .query_row(
            "SELECT generation,complete FROM checkpoints WHERE host=?1 AND login=?2 AND source=?3",
            params![target.scope.host, target.scope.login, source],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let anchor = bounded_record(connection, target, ANCHOR, &source, limits.max_state_bytes)?;
    let Some((generation, complete)) = checkpoint else {
        if anchor.is_some() {
            bail!("Metric commit anchor has no actual source checkpoint");
        }
        return Ok(None);
    };
    if !positive(generation) || complete {
        bail!("Initial metric retention checkpoint cannot assert historical completeness");
    }
    let epoch = reservation_epoch(connection, target)?;
    if epoch.0 < generation {
        bail!("Metric checkpoint exceeds actual reserved generation");
    }
    let (count, bytes, oversized): (u64, u64, u64) = connection.query_row(
        "SELECT COUNT(*),COALESCE(SUM(length(CAST(r.payload AS BLOB))),0),
         COALESCE(SUM(CASE WHEN length(CAST(r.payload AS BLOB))>
         CASE WHEN r.kind=?4 THEN ?5 ELSE ?6 END THEN 1 ELSE 0 END),0)
         FROM records r JOIN checkpoint_records m ON m.host=r.host AND m.login=r.login
         AND m.kind=r.kind AND m.id=r.id WHERE m.host=?1 AND m.login=?2 AND m.source=?3",
        params![
            target.scope.host,
            target.scope.login,
            source,
            EVENT,
            limits.max_document_bytes,
            limits.max_state_bytes
        ],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    if count > (limits.max_events + 2) as u64
        || bytes > limits.max_total_bytes as u64
        || oversized != 0
    {
        bail!("Metric source count/UTF8 document/aggregate footprint exceeds selected budget; no payload copied");
    }
    let mut statement = connection.prepare(
        "SELECT r.kind,r.id,r.repository,r.payload,r.complete FROM records r
         JOIN checkpoint_records m ON m.host=r.host AND m.login=r.login AND m.kind=r.kind AND m.id=r.id
         WHERE m.host=?1 AND m.login=?2 AND m.source=?3 ORDER BY r.kind,r.id LIMIT ?4",
    )?;
    let rows = statement
        .query_map(
            params![
                target.scope.host,
                target.scope.login,
                source,
                limits.max_events + 3
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, bool>(4)?,
                ))
            },
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut state = None;
    let mut manifest = None;
    let mut state_sha = None;
    let mut manifest_sha = None;
    let mut members = BTreeMap::new();
    let mut facts = vec![];
    let mut ids = BTreeSet::new();
    let mut nodes = BTreeSet::new();
    for (kind, id, repository, payload, complete) in rows {
        if repository != target.repository || complete {
            bail!("Metric source membership has contradictory repository/completeness metadata");
        }
        let value: Value = serde_json::from_str(&payload)?;
        match kind.as_str() {
            STATE if id == source && state.is_none() => {
                let decoded: RetentionState = serde_json::from_value(value.clone())?;
                if digest(&decoded)? != digest(&value)? {
                    bail!("Metric STATE has unknown or noncanonical fields");
                }
                state_sha = Some(digest(&value)?);
                state = Some(decoded);
            }
            MANIFEST if id == source && manifest.is_none() => {
                let decoded: Manifest = serde_json::from_value(value.clone())?;
                if digest(&decoded)? != digest(&value)? {
                    bail!("Metric manifest has unknown or noncanonical fields");
                }
                manifest_sha = Some(digest(&value)?);
                manifest = Some(decoded);
            }
            EVENT => {
                let fact: Fact = serde_json::from_value(value.clone())?;
                validate_fact(target, &fact)?;
                if digest(&fact)? != digest(&value)?
                    || id != event_key(&source, &fact)?
                    || !ids.insert(fact.event.id)
                    || !nodes.insert(fact.provider_node.clone())
                {
                    bail!("Metric member identity/hash/node is contradictory");
                }
                members.insert(id, digest(&value)?);
                facts.push(fact);
            }
            _ => bail!("Unexpected/missing metric STATE/manifest/category member"),
        }
    }
    let state = state.context("Metric STATE is not an actual source member")?;
    let manifest = manifest.context("Metric manifest is not an actual source member")?;
    let anchor: CommitAnchor =
        serde_json::from_value(anchor.context("Metric commit anchor absent")?)?;
    validate_saved(&state.reserved)?;
    validate_coverage(&state.coverage)?;
    if state.reserved.target != *target
        || state.reserved.generation != generation
        || state.observed_from > state.observed_through
        || state.history_unavailable != HISTORY_UNAVAILABLE
        || manifest.source != source
        || manifest.generation != generation
        || Some(&manifest.state_sha256) != state_sha.as_ref()
        || manifest.events != facts.len()
        || manifest.members_sha256 != digest(&members)?
        || count != facts.len() as u64 + 2
        || anchor.target != *target
        || anchor.generation != generation
        || Some(&anchor.manifest_sha256) != manifest_sha.as_ref()
    {
        bail!("Metric committed generation/target/intent/member checksums disagree");
    }
    facts.sort_by_key(|fact| (fact.event.at, fact.event.id));
    Ok(Some(StoredMetricFrame {
        issuer,
        source,
        state,
        manifest,
        facts,
        reservation_epoch: epoch,
    }))
}
fn compatible_observation(old: &Fact, new: &Fact) -> bool {
    let kind_compatible = old.event.kind == new.event.kind
        || matches!(
            (&old.event.kind, &new.event.kind),
            (
                EventKind::ReviewSubmitted { review_id: old_id, .. },
                EventKind::ReviewSubmitted {
                    review_id: new_id,
                    state: SubmittedReviewState::Dismissed
                }
            ) if old_id == new_id
        );
    old.source == new.source
        && old.provider_node == new.provider_node
        && old.event.id == new.event.id
        && old.event.at == new.event.at
        && kind_compatible
        && match (old.event.actor.account_id, new.event.actor.account_id) {
            (Some(old), Some(new)) => old == new,
            _ => true,
        }
}

impl Cache {
    pub fn reserve_metric_events(
        &mut self,
        target: &MetricEventTarget,
        intent: &MetricReadIntent,
        limits: &MetricsRetentionLimits,
        authority: &mut impl MetricsRetentionAuthority,
    ) -> Result<MetricEventsReservation> {
        target.validate()?;
        intent.validate()?;
        limits.validate()?;
        authority.verify(target, MetricAuthorityPhase::BeforeReserve)?;
        let issuer = Issuer::from_cache(self)?;
        let source = target.source()?;
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let reserved: Option<u64> = transaction
            .query_row(
                "SELECT generation FROM sync_reservations WHERE host=?1 AND login=?2 AND source=?3",
                params![target.scope.host, target.scope.login, source],
                |row| row.get(0),
            )
            .optional()?;
        let checkpoint: Option<u64> = transaction
            .query_row(
                "SELECT generation FROM checkpoints WHERE host=?1 AND login=?2 AND source=?3",
                params![target.scope.host, target.scope.login, source],
                |row| row.get(0),
            )
            .optional()?;
        let prior = reserved.unwrap_or(0).max(checkpoint.unwrap_or(0));
        if prior >= i64::MAX as u64 {
            bail!("Metric source generation exhausted; original request not overwritten");
        }
        let generation = prior + 1;
        let saved = ReservedIntent {
            target: target.clone(),
            intent: intent.clone(),
            limits: limits.clone(),
            generation,
            intent_sha256: digest(&(target, intent, limits, generation))?,
        };
        if serde_json::to_vec(&saved)?.len() > limits.max_state_bytes {
            bail!("Original metric request exceeds its bounded metadata budget");
        }
        transaction.execute(
            "INSERT INTO sync_reservations VALUES(?1,?2,?3,?4,1)
             ON CONFLICT(host,login,source) DO UPDATE SET generation=excluded.generation,active=1",
            params![target.scope.host, target.scope.login, source, generation],
        )?;
        put_record(&transaction, target, RESERVATION, &source, &saved)?;
        transaction.commit()?;
        authority.verify(target, MetricAuthorityPhase::AfterReserve)?;
        if reservation_epoch(&self.connection, target)? != (generation, true)
            || bounded_record(
                &self.connection,
                target,
                RESERVATION,
                &source,
                limits.max_state_bytes,
            )? != Some(serde_json::to_value(&saved)?)
        {
            bail!("Metric reservation changed during final fresh authority proof");
        }
        Ok(MetricEventsReservation { issuer, saved })
    }

    pub fn commit_metric_events(
        &mut self,
        reservation: &MetricEventsReservation,
        batch: &MetricCategoryBatch,
        authority: &mut impl MetricsRetentionAuthority,
    ) -> Result<StoredMetricFrame> {
        let saved = &reservation.saved;
        validate_saved(saved)?;
        validate_coverage(&batch.coverage)?;
        if reservation.issuer != Issuer::from_cache(self)?
            || batch.target != saved.target
            || batch.observed_from > batch.observed_through
            || batch.facts.len() > batch.coverage.fetched_rows
        {
            bail!("Metric batch differs from original issuer/parent/category/observation interval");
        }
        let target = &saved.target;
        authority.verify(target, MetricAuthorityPhase::BeforeCommit)?;
        let source = target.source()?;
        let limits = &saved.limits;
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if reservation_epoch(&transaction, target)? != (saved.generation, true)
            || bounded_record(
                &transaction,
                target,
                RESERVATION,
                &source,
                limits.max_state_bytes,
            )? != Some(serde_json::to_value(saved)?)
        {
            bail!("Original metric reservation was replaced/fenced/edited; no source advanced");
        }
        let previous = capture(&transaction, reservation.issuer.clone(), target, limits)?;
        let mut union: BTreeMap<u64, Fact> = previous
            .into_iter()
            .flat_map(|frame| frame.facts)
            .map(|fact| (fact.event.id, fact))
            .collect();
        let mut incoming = BTreeSet::new();
        for fact in &batch.facts {
            validate_fact(target, fact)?;
            if fact.event.at > batch.observed_through {
                bail!("Metric event is after its actual observation; no timeline is invented");
            }
            if !incoming.insert(fact.event.id) {
                bail!("Duplicate numeric metric observation cannot select an arbitrary body");
            }
            let mut fact = fact.clone();
            if let Some(old) = union.get(&fact.event.id) {
                if !compatible_observation(old, &fact) {
                    bail!(
                        "Immutable event timestamp/node/target/actor changed; no history advanced"
                    );
                }
                if fact.event.actor.account_id.is_none() {
                    fact.event.actor.account_id = old.event.actor.account_id;
                }
            }
            union.insert(fact.event.id, fact);
        }
        if union.len() > limits.max_events {
            bail!("Retained metric union exceeds event budget; no pruning or partial commit");
        }
        let mut nodes = BTreeSet::new();
        let mut records = vec![];
        let mut members = BTreeMap::new();
        for fact in union.values() {
            if !nodes.insert(&fact.provider_node) {
                bail!("Metric opaque node is shared by contradictory numeric event IDs");
            }
            let payload = serde_json::to_value(fact)?;
            if serde_json::to_vec(&payload)?.len() > limits.max_document_bytes {
                bail!("Metric event exceeds selected document bytes; no source advanced");
            }
            let id = event_key(&source, fact)?;
            members.insert(id.clone(), digest(&payload)?);
            records.push(Record::new(EVENT, id, &target.repository, payload));
        }
        let state = RetentionState {
            reserved: saved.clone(),
            observed_from: batch.observed_from,
            observed_through: batch.observed_through,
            coverage: batch.coverage.clone(),
            history_unavailable: HISTORY_UNAVAILABLE.into(),
        };
        let manifest = Manifest {
            source: source.clone(),
            generation: saved.generation,
            state_sha256: digest(&state)?,
            members_sha256: digest(&members)?,
            events: union.len(),
        };
        for (kind, payload) in [
            (STATE, serde_json::to_value(&state)?),
            (MANIFEST, serde_json::to_value(&manifest)?),
        ] {
            if serde_json::to_vec(&payload)?.len() > limits.max_state_bytes {
                bail!("Metric STATE/manifest exceeds bounded metadata bytes");
            }
            records.push(Record::new(kind, &source, &target.repository, payload));
        }
        let total = records.iter().try_fold(0usize, |sum, record| {
            sum.checked_add(serde_json::to_vec(&record.payload)?.len())
                .context("Metric aggregate byte budget overflow")
        })?;
        if total > limits.max_total_bytes {
            bail!("Metric retained UTF8 payload union exceeds aggregate bytes; no source advanced");
        }
        if !Self::publish_sync(
            &transaction,
            &target.scope,
            &source,
            saved.generation,
            &records,
            SyncPolicy {
                complete: false,
                prune_missing: false,
                reservation_authoritative: true,
            },
        )? {
            bail!("A newer/equal metric checkpoint already committed; no late source replacement");
        }
        put_record(
            &transaction,
            target,
            ANCHOR,
            &source,
            &CommitAnchor {
                target: target.clone(),
                generation: saved.generation,
                manifest_sha256: digest(&manifest)?,
            },
        )?;
        let frame = capture(&transaction, reservation.issuer.clone(), target, limits)?
            .context("Metric committed source unexpectedly absent")?;
        transaction.commit()?;
        authority.verify(target, MetricAuthorityPhase::AfterCommit)?;
        // A fresh hook may discover/fence a mutation or account transition. Never
        // return an earlier captured frame after its issuer/keyspace changed.
        let final_frame = capture_transaction(self, target, limits)?;
        if final_frame.as_ref() != Some(&frame) {
            bail!("Metric retained source changed during final authority proof; output hidden");
        }
        Ok(frame)
    }

    pub fn read_metric_events(
        &self,
        target: &MetricEventTarget,
        limits: &MetricsRetentionLimits,
        authority: &mut impl MetricsRetentionAuthority,
    ) -> Result<Option<StoredMetricFrame>> {
        target.validate()?;
        limits.validate()?;
        authority.verify(target, MetricAuthorityPhase::BeforeRead)?;
        let frame = capture_transaction(self, target, limits)?;
        authority.verify(target, MetricAuthorityPhase::AfterRead)?;
        if capture_transaction(self, target, limits)? != frame {
            bail!("Metric source changed during final authority proof; retained output hidden");
        }
        Ok(frame)
    }
}
fn capture_transaction(
    cache: &Cache,
    target: &MetricEventTarget,
    limits: &MetricsRetentionLimits,
) -> Result<Option<StoredMetricFrame>> {
    let issuer = Issuer::from_cache(cache)?;
    let transaction = cache.connection.unchecked_transaction()?;
    let frame = capture(&transaction, issuer, target, limits)?;
    transaction.commit()?;
    Ok(frame)
}
impl StoredMetricFrame {
    pub fn validate(
        &self,
        cache: &Cache,
        limits: &MetricsRetentionLimits,
        authority: &mut impl MetricsRetentionAuthority,
    ) -> Result<()> {
        if cache
            .read_metric_events(self.target(), limits, authority)?
            .as_ref()
            != Some(self)
        {
            bail!("Metric snapshot issuer/generation/member/source epoch is no longer current");
        }
        Ok(())
    }
}
