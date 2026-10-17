//! Restriction-only State inspection/Clear epoch. The actual journal witness
//! is the only constructor input. It grants no source, hydrate, claim or write.
use super::*;
use crate::pr_write_journal::state_origin_v68::history_v74::StateRecoveryWitnessV74;
type QueueReservationV94 = Option<(u64, bool)>;
type QueueCheckpointV94 = Option<(u64, u64, bool)>;
type QueueRowsV94 = (QueueReservationV94, QueueCheckpointV94);

/// Exact queue boundary captured BEFORE first Inspect Core child. The own
/// Personal PR-read reservation is a DIFFERENT key and cannot rewrite this
/// witness. Clear's reservation fence/checkpoint deletion must remain visible.
pub(crate) struct StateAckQueueBoundaryV78 {
    file: crate::pr_write_journal::state_origin_v68::history_v74::StateOriginalFileV78,
    scope: Scope,
    source: String,
    reservation: QueueReservationV94,
    checkpoint: Option<(u64, u64, bool)>,
}
impl std::fmt::Debug for StateAckQueueBoundaryV78 {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.write_str("StateAckQueueBoundaryV78(<original pre-read queue/Clear restriction>)")
    }
}
impl StateAckQueueBoundaryV78 {
    pub(crate) fn capture(
        tx: &Transaction<'_>,
        actual: &StateRecoveryWitnessV74,
        original: &crate::pr_collaboration_origin_v17::PrCollaborationOpenIntentV17,
    ) -> Result<Self> {
        let scope = actual.cache_scope_v77();
        let source = original
            .state_cache_source_v78()
            .context("Actual original State queue key absent")?;
        if !actual.matches_original_queue_v78(original)
            || source.len() > 2048
            || !source.starts_with("personal-inbox:")
            || scope.host.len() > 255
            || scope.login.len() > 64
        {
            bail!("Original typed State queue capture differs; no boundary issued");
        }
        let (reservation, checkpoint) = Self::read(tx, scope, source)?;
        // This is the already actual queue reservation, even when a known
        // blocker made it inactive. Its existence is NOT fresh membership.
        if reservation.is_none()
            && !checkpoint.is_some_and(|(generation, _, _)| {
                original.matches_state_legacy_checkpoint_v95(generation)
            })
        {
            bail!("Original queue reservation absent without exact actual legacy checkpoint");
        }
        Ok(Self {
            file: actual.cache_file_v78(),
            scope: scope.clone(),
            source: source.into(),
            reservation,
            checkpoint,
        })
    }
    pub(crate) fn validate(&self, tx: &Connection) -> Result<()> {
        if Self::read(tx, &self.scope, &self.source)? != (self.reservation, self.checkpoint) {
            bail!("Original pre-read State queue/Clear boundary changed; blocker retained");
        }
        Ok(())
    }
    /// Fixed read-only restriction only, called by the closed State inspection
    /// purpose before Core and BEFORE its own Personal reservation can write.
    pub(crate) fn validate_at_database_v78(&self, database: &Path) -> Result<()> {
        self.file.validate(database)?;
        let connection =
            Connection::open_with_flags(database, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        self.validate(&connection)?;
        self.file.validate(database)
    }
    fn read(tx: &Connection, scope: &Scope, source: &str) -> Result<QueueRowsV94> {
        use rusqlite::OptionalExtension;
        let reservation = tx.query_row(
            "SELECT generation,active,typeof(generation)='integer' AND generation>0 AND typeof(active)='integer' AND active IN(0,1) FROM sync_reservations WHERE host=?1 AND login=?2 AND source=?3",
            params![scope.host,scope.login,source],
            |r|Ok((r.get::<_,u64>(0)?,r.get::<_,bool>(1)?,r.get::<_,bool>(2)?)),
        ).optional()?;
        let checkpoint = tx.query_row(
            "SELECT generation,fetched_at,complete,typeof(generation)='integer' AND generation>0 AND typeof(fetched_at)='integer' AND fetched_at>=0 AND typeof(complete)='integer' AND complete IN(0,1) FROM checkpoints WHERE host=?1 AND login=?2 AND source=?3",
            params![scope.host,scope.login,source],
            |r|Ok((r.get::<_,u64>(0)?,r.get::<_,u64>(1)?,r.get::<_,bool>(2)?,r.get::<_,bool>(3)?)),
        ).optional()?;
        if reservation.is_some_and(|r| !r.2) || checkpoint.is_some_and(|r| !r.3) {
            bail!("Original queue boundary raw scalar type/range differs");
        }
        Ok((
            reservation.map(|r| (r.0, r.1)),
            checkpoint.map(|r| (r.0, r.1, r.2)),
        ))
    }
}

pub(crate) struct StateAckCacheEpochV77 {
    scope: Scope,
    digest: [u8; 32],
}
impl std::fmt::Debug for StateAckCacheEpochV77 {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.write_str("StateAckCacheEpochV77(<exact original cache/Clear restriction>)")
    }
}
impl StateAckCacheEpochV77 {
    pub(crate) fn capture(tx: &Transaction<'_>, actual: &StateRecoveryWitnessV74) -> Result<Self> {
        let scope = actual.cache_scope_v77();
        // Scope belongs to the actual already validated typed State original.
        // Bound it before even the small retained scope copy.
        if scope.host.is_empty()
            || scope.host.len() > 255
            || scope.login.len() > 64
            || scope
                .login
                .strip_prefix("user-id:")
                .is_none_or(|id| id.starts_with('0') || id.parse::<u64>().is_err() || id.is_empty())
        {
            bail!("State inspection original scope exceeds its closed scalar profile");
        }
        let digest = Self::current_digest(tx, scope)?;
        Ok(Self {
            scope: scope.clone(),
            digest,
        })
    }
    pub(crate) fn validate(&self, tx: &Connection) -> Result<()> {
        if Self::current_digest(tx, &self.scope)? != self.digest {
            bail!("Original State inspection cache/Clear/source epoch changed; blocker retained");
        }
        Ok(())
    }
    fn current_digest(tx: &Connection, scope: &Scope) -> Result<[u8; 32]> {
        let mut digest = Sha256::new();
        let mut total_rows = 0usize;
        let mut total_bytes = 0usize;
        // Only this fixed closed profile is accepted. In particular no raw
        // PR/draft body or caller-selected SQL, table, predicate or field list.
        for (table, fields, order, principal) in [
            ("sync_reservations", "host,login,source,generation,active", "source", "login"),
            ("checkpoints", "host,login,source,generation,fetched_at,complete", "source", "login"),
            ("sync_resource_bindings", "host,login,source,namespace,repository_id,repository,resource,kind,reservation_generation,visible_generation,active", "source", "login"),
            ("sync_binding_lookup_epochs", "host,login,namespace,repository,generation", "namespace,repository", "login"),
            ("sync_mutation_fences", "host,login,namespace,repository,resource,kind,mutation_ordinal", "namespace,repository,resource,kind", "login"),
            ("saved_views", "host,login,name,payload", "name", "login"),
            ("pr_write_state_filter_fences_v68", "host,principal,pr_node,repository_id,pr_id,repository,attempt_id,raw_origin_sha256,raw_disposition_sha256,status", "pr_node", "principal"),
            ("pr_write_state_source_watermarks_v73", "host,principal,pr_node,source,source_generation,mutation_ordinal,repository_id,pr_id", "pr_node,source", "principal"),
        ] {
            let size = fields.split(',')
                .map(|field| format!("COALESCE(length(CAST({field} AS BLOB)),0)"))
                .collect::<Vec<_>>().join("+");
            let predicate = format!("host=?1 AND {principal}=?2");
            let (expected_rows, bytes):(usize,usize)=tx.query_row(
                &format!("SELECT count(*),COALESCE(SUM({size}),0) FROM {table} WHERE {predicate}"),
                params![scope.host,scope.login],|r|Ok((r.get(0)?,r.get(1)?)))?;
            total_rows=total_rows.checked_add(expected_rows).filter(|n|*n<=ROW_CAP)
                .context("State inspection cache/Clear metadata exceeds512 rows before copy")?;
            total_bytes=total_bytes.checked_add(bytes).filter(|n|*n<=META_CAP)
                .context("State inspection cache/Clear metadata exceeds64KiB before copy")?;
            digest.update((table.len() as u64).to_le_bytes());
            digest.update(table.as_bytes());
            digest.update((expected_rows as u64).to_le_bytes());
            let mut statement=tx.prepare(&format!("SELECT {fields} FROM {table} WHERE {predicate} ORDER BY {order}"))?;
            let columns=statement.column_count();
            digest.update((columns as u64).to_le_bytes());
            let mut observed=0usize;
            let mut rows=statement.query(params![scope.host,scope.login])?;
            let mut actual_bytes=0usize;
            while let Some(row)=rows.next()? {
                observed=observed.checked_add(1).filter(|n|*n<=expected_rows)
                    .context("State inspection cache/Clear count changed during capture")?;
                digest.update((observed as u64).to_le_bytes());
                for column in 0..columns {
                    // hash_field borrows SQLite ValueRef; NULL/type/length are
                    // framed and no retained private payload is materialized.
                    hash_field(&mut digest,row.get_ref(column)?,&mut actual_bytes,META_CAP)?;
                }
            }
            // SUM(length(CAST(...))) counts textual integer widths while
            // hash_field counts each integer as8. Both are independently
            // bounded; neither is mistaken for the other or post-copy proof.
            if observed != expected_rows {
                bail!("State cache/Clear row count changed inside capture");
            }
            total_bytes=total_bytes.checked_add(actual_bytes.saturating_sub(bytes))
                .filter(|n|*n<=META_CAP).context("State typed cache/Clear metadata exceeds64KiB")?;
        }
        Ok(digest.finalize().into())
    }
}
