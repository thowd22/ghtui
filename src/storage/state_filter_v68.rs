//! Separate causal State/filter axis. A text fence/category cannot stand in
//! for membership/count freshness. These rows are local invalidation evidence,
//! never a source, current access, write permit or journal acknowledgement.
use super::*;
use crate::pr_write_journal::conversation_refresh_v19::ConversationTerminalWitnessV19;
use rusqlite::{types::ValueRef, Transaction};
use sha2::{Digest, Sha256};

const META_CAP: usize = 64 * 1024;
const ROW_CAP: usize = 512;
#[path = "state_ack_cache_epoch_v77.rs"]
mod ack_cache_epoch_v77;
pub(crate) use ack_cache_epoch_v77::{StateAckCacheEpochV77, StateAckQueueBoundaryV78};

pub(crate) struct PreparedStateCacheRepairV74 {
    cache: Cache,
    repair: crate::pr_write_journal::state_origin_v68::StateCacheRepairV74,
    epoch: super::conversation_refresh_v19::EpochV19,
    fence: PreparedStateFenceV68,
}
impl PreparedStateCacheRepairV74 {
    pub(crate) fn prepare(
        repair: crate::pr_write_journal::state_origin_v68::StateCacheRepairV74,
        cancel: &crate::providers::runner::CancellationToken,
        deadline: std::time::Instant,
    ) -> Result<Self> {
        repair.validate_file_clock(cancel, deadline)?;
        let cache = Cache::open(repair.database())?;
        let tx = cache.connection.unchecked_transaction()?;
        repair.validate_transaction(&tx)?;
        let epoch =
            super::conversation_refresh_v19::EpochV19::capture_state_repair_v74(&tx, &repair)?;
        let fence = PreparedStateFenceV68::prepare_repair(&tx, &repair)?;
        tx.commit()?;
        repair
            .source()
            .validate_state_cache_repair_boundary_v75(&repair)
            .map_err(anyhow::Error::msg)?;
        repair.validate_file_clock(cancel, deadline)?;
        Ok(Self {
            cache,
            repair,
            epoch,
            fence,
        })
    }
    pub(crate) fn commit(
        mut self,
        cancel: &crate::providers::runner::CancellationToken,
        deadline: std::time::Instant,
    ) -> Result<()> {
        let deadline = deadline.min(self.repair.cutoff());
        self.repair.validate_file_clock(cancel, deadline)?;
        self.repair
            .source()
            .validate_state_cache_repair_boundary_v75(&self.repair)
            .map_err(anyhow::Error::msg)?;
        self.cache.connection.busy_timeout(
            deadline
                .saturating_duration_since(std::time::Instant::now())
                .min(std::time::Duration::from_secs(2)),
        )?;
        let tx = self
            .cache
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        self.repair.validate_file_clock(cancel, deadline)?;
        self.repair.validate_transaction(&tx)?;
        self.epoch.validate(&tx)?;
        self.fence.apply(&tx)?;
        self.repair.validate_file_clock(cancel, deadline)?;
        tx.commit()?;
        Ok(())
    }
}

pub(super) fn initialize(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS pr_write_state_filter_fences_v68(
        host TEXT NOT NULL CHECK(typeof(host)='text' AND length(CAST(host AS BLOB)) BETWEEN 1 AND 255),
        principal TEXT NOT NULL CHECK(typeof(principal)='text' AND length(CAST(principal AS BLOB)) BETWEEN 9 AND 64),
        pr_node TEXT NOT NULL CHECK(typeof(pr_node)='text' AND length(CAST(pr_node AS BLOB)) BETWEEN 1 AND 1024),
        repository_id INTEGER NOT NULL CHECK(typeof(repository_id)='integer' AND repository_id>0),
        pr_id INTEGER NOT NULL CHECK(typeof(pr_id)='integer' AND pr_id>0),
        repository TEXT NOT NULL CHECK(typeof(repository)='text' AND length(CAST(repository AS BLOB)) BETWEEN 3 AND 512),
        attempt_id INTEGER NOT NULL CHECK(typeof(attempt_id)='integer' AND attempt_id>0),
        raw_origin_sha256 TEXT NOT NULL CHECK(typeof(raw_origin_sha256)='text' AND length(CAST(raw_origin_sha256 AS BLOB))=64 AND raw_origin_sha256 NOT GLOB '*[^0-9a-f]*'),
        raw_disposition_sha256 TEXT CHECK(typeof(raw_disposition_sha256)='null' OR(typeof(raw_disposition_sha256)='text' AND length(CAST(raw_disposition_sha256 AS BLOB))=64 AND raw_disposition_sha256 NOT GLOB '*[^0-9a-f]*')),
        status TEXT NOT NULL CHECK(typeof(status)='text' AND status IN('unavailable','hydrated')),
        PRIMARY KEY(host,principal,pr_node));
        CREATE TABLE IF NOT EXISTS pr_write_state_source_watermarks_v73(
        host TEXT NOT NULL,principal TEXT NOT NULL,pr_node TEXT NOT NULL,
        source TEXT NOT NULL CHECK(typeof(source)='text' AND length(CAST(source AS BLOB)) BETWEEN 1 AND 2048),
        source_generation INTEGER NOT NULL CHECK(typeof(source_generation)='integer' AND source_generation>0),
        mutation_ordinal INTEGER NOT NULL CHECK(typeof(mutation_ordinal)='integer' AND mutation_ordinal>0),
        repository_id INTEGER NOT NULL CHECK(typeof(repository_id)='integer' AND repository_id>0),
        pr_id INTEGER NOT NULL CHECK(typeof(pr_id)='integer' AND pr_id>0),
        PRIMARY KEY(host,principal,pr_node,source));",
    )?;
    Ok(())
}

/// Hash bounded raw typed SQL fields without allocating/parsing a body. The
/// two fixed statements below are the ONLY callers; SQL/field order is closed.
fn raw_digest(tx: &Connection, sql: &str, attempt: i64, cap: usize) -> Result<String> {
    let mut statement = tx.prepare(sql)?;
    let columns = statement.column_count();
    let mut rows = statement.query([attempt])?;
    let row = rows.next()?.context("State causal raw row missing")?;
    let mut bytes = 0usize;
    let mut digest = Sha256::new();
    for column in 0..columns {
        let value = row.get_ref(column)?;
        hash_field(&mut digest, value, &mut bytes, cap)?;
    }
    if rows.next()?.is_some() {
        bail!("State causal raw row is not singleton");
    }
    Ok(format!("{:x}", digest.finalize()))
}
fn hash_field(
    digest: &mut Sha256,
    value: ValueRef<'_>,
    bytes: &mut usize,
    cap: usize,
) -> Result<()> {
    let size = match value {
        ValueRef::Null => 0,
        ValueRef::Integer(_) | ValueRef::Real(_) => 8,
        ValueRef::Text(v) | ValueRef::Blob(v) => v.len(),
    };
    *bytes = bytes
        .checked_add(size)
        .context("State raw scalar size overflow")?;
    if *bytes > cap {
        bail!("State raw scalar pre-copy bound exceeded");
    }
    // Type tags + length prevent absent/NULL, INTEGER/TEXT, and field
    // concatenation from becoming the same witness.
    digest.update((size as u64).to_le_bytes());
    match value {
        ValueRef::Null => digest.update([0]),
        ValueRef::Integer(v) => {
            digest.update([1]);
            digest.update(v.to_le_bytes());
        }
        ValueRef::Real(v) => {
            digest.update([2]);
            digest.update(v.to_bits().to_le_bytes());
        }
        ValueRef::Text(v) => {
            digest.update([3]);
            digest.update(v);
        }
        ValueRef::Blob(v) => {
            digest.update([4]);
            digest.update(v);
        }
    }
    Ok(())
}
pub(crate) fn state_origin_fingerprint_v74(tx: &Connection, attempt: i64) -> Result<String> {
    raw_digest(tx,"SELECT attempt_id,repository_id,pr_id,issue_id,command,base_key_v71,action_tag_v71,parent_ack_attempt_v71,origin_json FROM pr_write_state_origins_v68 WHERE attempt_id=?1",attempt,18*1024)
}
pub(crate) fn state_disposition_fingerprint_v74(attempt: i64, raw: &str) -> Result<String> {
    if attempt <= 0 || raw.len() > 8192 {
        bail!("State disposition fingerprint scalar bound exceeded");
    }
    let mut digest = Sha256::new();
    let mut bytes = 0;
    for value in [
        ValueRef::Integer(attempt),
        ValueRef::Text(b"confirmed_effect"),
        ValueRef::Text(raw.as_bytes()),
    ] {
        hash_field(&mut digest, value, &mut bytes, 8192 + 64)?;
    }
    Ok(format!("{:x}", digest.finalize()))
}
fn fingerprints(tx: &Connection, attempt: i64) -> Result<(String, Option<String>)> {
    let disposition: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM pr_write_state_dispositions_v68 WHERE attempt_id=?1)",
        [attempt],
        |r| r.get(0),
    )?;
    Ok((
        state_origin_fingerprint_v74(tx,attempt)?,
        disposition.then(||raw_digest(tx,"SELECT attempt_id,kind,receipt_json FROM pr_write_state_dispositions_v68 WHERE attempt_id=?1",attempt,8192+64)).transpose()?,
    ))
}

/// Restriction-only startup recognizer. No PR payload is loaded, no cached
/// membership is admitted, and no row here becomes a Source/ACK/permission.
pub(super) fn validate_startup_rows_v80(tx: &Connection, scope: &Scope) -> Result<()> {
    let mut statement = tx.prepare("SELECT pr_node,attempt_id,raw_origin_sha256,raw_disposition_sha256,typeof(host)='text' AND typeof(principal)='text' AND typeof(pr_node)='text' AND length(CAST(pr_node AS BLOB)) BETWEEN 1 AND 1024 AND typeof(repository_id)='integer' AND repository_id>0 AND typeof(pr_id)='integer' AND pr_id>0 AND typeof(repository)='text' AND length(CAST(repository AS BLOB)) BETWEEN 3 AND 512 AND typeof(attempt_id)='integer' AND attempt_id>0 AND typeof(raw_origin_sha256)='text' AND length(CAST(raw_origin_sha256 AS BLOB))=64 AND raw_origin_sha256 NOT GLOB '*[^0-9a-f]*' AND (typeof(raw_disposition_sha256)='null' OR(typeof(raw_disposition_sha256)='text' AND length(CAST(raw_disposition_sha256 AS BLOB))=64 AND raw_disposition_sha256 NOT GLOB '*[^0-9a-f]*')) AND typeof(status)='text' AND status IN('unavailable','hydrated') FROM pr_write_state_filter_fences_v68 WHERE host=?1 AND principal=?2 ORDER BY pr_node")?;
    let mut rows = statement.query(params![scope.host, scope.login])?;
    let mut count = 0usize;
    while let Some(row) = rows.next()? {
        count = count
            .checked_add(1)
            .filter(|n| *n <= 32)
            .context("Startup State key bound")?;
        if !row.get::<_, bool>(4)? {
            bail!("Startup State fence raw types changed");
        }
        let node = row.get_ref(0)?.as_str()?;
        let attempt: i64 = row.get(1)?;
        let actual = fingerprints(tx, attempt)?;
        let disposition = match row.get_ref(3)? {
            ValueRef::Null => None,
            ValueRef::Text(bytes) => Some(std::str::from_utf8(bytes)?),
            _ => bail!("Startup State disposition raw type changed"),
        };
        let bound: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM pr_write_attempts a JOIN pr_write_state_origins_v68 o ON o.attempt_id=a.attempt_id JOIN pr_write_state_filter_fences_v68 f ON f.attempt_id=a.attempt_id WHERE a.attempt_id=?1 AND a.host=?2 AND a.principal=?3 AND a.pr_node=?4 AND f.host=a.host AND f.principal=a.principal AND f.pr_node=a.pr_node AND o.repository_id=f.repository_id AND o.pr_id=f.pr_id AND o.command=a.command AND a.command IN('state-close','state-reopen') AND a.state IN('pending','uncertain','confirmed','acknowledged') AND typeof(o.origin_json)='text' AND length(CAST(o.origin_json AS BLOB)) BETWEEN 1 AND 16384)",params![attempt,scope.host,scope.login,node],|r|r.get(0))?;
        if !bound || actual.0 != row.get_ref(2)?.as_str()? || actual.1.as_deref() != disposition {
            bail!("Startup raw State origin/disposition does not bind its exact fence");
        }
    }
    if count == 0 {
        bail!("Startup State restriction has no actual keys");
    }
    Ok(())
}

pub(super) struct PreparedStateFenceV68 {
    scope: Scope,
    node: String,
    repository: String,
    repository_id: u64,
    pr_id: u64,
    ordinal: i64,
    hashes: (String, Option<String>),
    apply: bool,
}
impl PreparedStateFenceV68 {
    pub(super) fn prepare(
        tx: &Transaction<'_>,
        terminal: &ConversationTerminalWitnessV19,
    ) -> Result<Option<Self>> {
        let Some(transition) = terminal.state_transition_v68() else {
            return Ok(None);
        };
        if !terminal.categories().is_empty()
            || !std::sync::Arc::ptr_eq(terminal.parent(), transition.parent())
        {
            bail!(
                "State causal fence requires its exact typed terminal/parent, not a text category"
            );
        }
        terminal.validate_transaction_v19(tx)?;
        let target = terminal.target();
        let ordinal = terminal.reference().attempt_id;
        let previous: Option<i64> = tx.query_row("SELECT attempt_id FROM pr_write_state_filter_fences_v68 WHERE host=?1 AND principal=?2 AND pr_node=?3",params![target.scope().host,target.scope().login,target.node_id()],|r|r.get(0)).optional()?;
        if previous.is_some_and(|v| v > ordinal) {
            bail!("Newer State fence exists; no old terminal replay");
        }
        let hashes = fingerprints(tx, ordinal)?;
        let value = Self {
            scope: target.scope().clone(),
            node: target.node_id().into(),
            repository: target.repository().to_ascii_lowercase(),
            repository_id: terminal.repository_id(),
            pr_id: transition.parent().database_id(),
            ordinal,
            hashes,
            apply: previous != Some(ordinal),
        };
        if !value.apply {
            value.validate_once(tx)?;
        }
        Ok(Some(value))
    }
    pub(super) fn actually_fences(&self) -> bool {
        self.apply
    }
    pub(super) fn prepare_repair(
        tx: &Transaction<'_>,
        repair: &crate::pr_write_journal::state_origin_v68::StateCacheRepairV74,
    ) -> Result<Self> {
        repair.validate_transaction(tx)?;
        let target = repair.target();
        let ordinal = repair.ordinal();
        let previous:Option<i64>=tx.query_row("SELECT attempt_id FROM pr_write_state_filter_fences_v68 WHERE host=?1 AND principal=?2 AND pr_node=?3",params![target.scope().host,target.scope().login,target.node_id()],|r|r.get(0)).optional()?;
        if previous.is_some_and(|n| n > ordinal) {
            bail!("Newer State ordinal cannot be revoked by old repair");
        }
        let value = Self {
            scope: target.scope().clone(),
            node: target.node_id().into(),
            repository: target.repository().to_ascii_lowercase(),
            repository_id: repair.repository_id(),
            pr_id: repair.pr_id(),
            ordinal,
            hashes: fingerprints(tx, ordinal)?,
            apply: previous != Some(ordinal),
        };
        if !value.apply {
            value.validate_once(tx)?;
        }
        Ok(value)
    }
    pub(super) fn apply(&self, tx: &Transaction<'_>) -> Result<()> {
        if !self.apply {
            return self.validate_once(tx);
        }
        // SAME inherited scope-wide reservation and binding visibility fence;
        // no exception renews an old alias or old in-flight producer.
        Cache::fence_reservations(tx, &self.scope)?;
        bindings::fence_binding_visibility(tx, &self.scope, None)?;
        tx.execute("INSERT INTO pr_write_state_filter_fences_v68 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,'unavailable') ON CONFLICT(host,principal,pr_node) DO UPDATE SET repository_id=excluded.repository_id,pr_id=excluded.pr_id,repository=excluded.repository,attempt_id=excluded.attempt_id,raw_origin_sha256=excluded.raw_origin_sha256,raw_disposition_sha256=excluded.raw_disposition_sha256,status='unavailable' WHERE pr_write_state_filter_fences_v68.attempt_id<excluded.attempt_id",params![self.scope.host,self.scope.login,self.node,self.repository_id,self.pr_id,self.repository,self.ordinal,self.hashes.0,self.hashes.1])?;
        self.validate_once(tx)
    }
    fn validate_once(&self, tx: &Connection) -> Result<()> {
        let exact:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM pr_write_state_filter_fences_v68 WHERE host=?1 AND principal=?2 AND pr_node=?3 AND typeof(repository_id)='integer' AND repository_id=?4 AND typeof(pr_id)='integer' AND pr_id=?5 AND typeof(repository)='text' AND CAST(repository AS BLOB)=CAST(?6 AS BLOB) AND typeof(attempt_id)='integer' AND attempt_id=?7 AND typeof(raw_origin_sha256)='text' AND CAST(raw_origin_sha256 AS BLOB)=CAST(?8 AS BLOB) AND typeof(raw_disposition_sha256)=typeof(?9) AND CAST(raw_disposition_sha256 AS BLOB) IS CAST(?9 AS BLOB) AND typeof(status)='text' AND status IN('unavailable','hydrated'))",params![self.scope.host,self.scope.login,self.node,self.repository_id,self.pr_id,self.repository,self.ordinal,self.hashes.0,self.hashes.1],|r|r.get(0))?;
        if !exact {
            bail!("Exact State ordinal/raw origin/disposition fence changed");
        }
        Ok(())
    }
    /// ONLY SAME actual post/fresh-read kernel calls this AFTER it published
    /// that exact preencoded source at its prepared active generation.
    pub(super) fn published_source(
        &self,
        tx: &Transaction<'_>,
        source: &str,
        generation: u64,
    ) -> Result<()> {
        self.validate_once(tx)?;
        if self.hashes.1.is_none() {
            bail!("Unresolved State/ACK repair can never issue a fresh-source watermark");
        }
        if source.is_empty()
            || source.len() > 2048
            || source.contains('\0')
            || generation == 0
            || generation > i64::MAX as u64
        {
            bail!("State fresh source exceeds scalar profile");
        }
        let exact:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM checkpoints c JOIN sync_reservations r USING(host,login,source) WHERE c.host=?1 AND c.login=?2 AND c.source=?3 AND typeof(c.generation)='integer' AND c.generation=?4 AND typeof(r.generation)='integer' AND r.generation=?4 AND r.active=1)",params![self.scope.host,self.scope.login,source,generation],|r|r.get(0))?;
        if !exact {
            bail!(
                "State watermark requires SAME actual committed checkpoint and active reservation"
            );
        }
        tx.execute("INSERT INTO pr_write_state_source_watermarks_v73 VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(host,principal,pr_node,source) DO UPDATE SET source_generation=excluded.source_generation,mutation_ordinal=excluded.mutation_ordinal,repository_id=excluded.repository_id,pr_id=excluded.pr_id",params![self.scope.host,self.scope.login,self.node,source,generation,self.ordinal,self.repository_id,self.pr_id])?;
        Ok(())
    }
}

/// Validate the actual source before ANY sync_frame payload/coverage/count
/// copy. A pure cached read is read-only: no repair, hydration or new source.
pub(super) fn guard_source(tx: &Connection, scope: &Scope, source: &str) -> Result<()> {
    let is_pr = source.starts_with("personal-inbox:")||source.starts_with("pr-write-projection:")||source.starts_with("collaboration-v1:")||source.starts_with("search-index:")||tx.query_row("SELECT EXISTS(SELECT 1 FROM checkpoint_records WHERE host=?1 AND login=?2 AND source=?3 AND kind='pr')",params![scope.host,scope.login,source],|r|r.get::<_,bool>(0))?;
    if !is_pr {
        return Ok(());
    }
    let has_state:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='pr_write_state_origins_v68')",[],|r|r.get(0))?;
    if has_state {
        let unfenced:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM pr_write_attempts a WHERE a.host=?1 AND a.principal=?2 AND (a.command LIKE 'state-%' OR a.selected_resource LIKE 'state-v68/%' OR EXISTS(SELECT 1 FROM pr_write_state_origins_v68 o WHERE o.attempt_id=a.attempt_id)) AND NOT EXISTS(SELECT 1 FROM pr_write_state_filter_fences_v68 f WHERE f.host=a.host AND f.principal=a.principal AND f.pr_node=a.pr_node AND typeof(f.attempt_id)='integer' AND f.attempt_id>=a.attempt_id))",params![scope.host,scope.login],|r|r.get(0))?;
        if unfenced {
            bail!("Known State attempt lacks exact local revocation; cached PR data unavailable until explicit current inspection");
        }
    }
    let (count,bytes):(usize,usize)=tx.query_row("SELECT count(*),COALESCE(sum(length(CAST(host AS BLOB))+length(CAST(principal AS BLOB))+length(CAST(pr_node AS BLOB))+length(CAST(repository AS BLOB))+length(CAST(raw_origin_sha256 AS BLOB))+COALESCE(length(CAST(raw_disposition_sha256 AS BLOB)),0)+length(CAST(status AS BLOB))+32),0) FROM pr_write_state_filter_fences_v68 WHERE host=?1 AND principal=?2",params![scope.host,scope.login],|r|Ok((r.get(0)?,r.get(1)?)))?;
    if count == 0 {
        return Ok(());
    }
    if count > ROW_CAP || bytes > META_CAP {
        bail!("State filter metadata exceeds512 rows/64KiB before copy");
    }
    let mut statement=tx.prepare("SELECT pr_node,repository_id,pr_id,attempt_id,raw_origin_sha256,raw_disposition_sha256,typeof(pr_node)='text' AND length(CAST(pr_node AS BLOB)) BETWEEN 1 AND 1024 AND typeof(repository_id)='integer' AND repository_id>0 AND typeof(pr_id)='integer' AND pr_id>0 AND typeof(attempt_id)='integer' AND attempt_id>0 AND typeof(raw_origin_sha256)='text' AND length(CAST(raw_origin_sha256 AS BLOB))=64 AND raw_origin_sha256 NOT GLOB '*[^0-9a-f]*' AND (typeof(raw_disposition_sha256)='null' OR (typeof(raw_disposition_sha256)='text' AND length(CAST(raw_disposition_sha256 AS BLOB))=64 AND raw_disposition_sha256 NOT GLOB '*[^0-9a-f]*')) AND typeof(repository)='text' AND typeof(status)='text' AND status IN('unavailable','hydrated') FROM pr_write_state_filter_fences_v68 WHERE host=?1 AND principal=?2")?;
    let mut rows = statement.query(params![scope.host, scope.login])?;
    while let Some(row) = rows.next()? {
        if !row.get::<_, bool>(6)? {
            bail!("Malformed State fence; cached PR membership unavailable");
        }
        let node = row.get_ref(0)?.as_str()?;
        if let Some(exact) = source.strip_prefix("pr-write-projection:") {
            if exact != node {
                continue;
            }
        } else if source.starts_with("collaboration-v1:") {
            let unrelated:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM sync_resource_bindings WHERE host=?1 AND login=?2 AND source=?3 AND resource<>?4)",params![scope.host,scope.login,source,node],|r|r.get(0))?;
            if unrelated {
                continue;
            }
        }
        let repository: u64 = row.get(1)?;
        let pr: u64 = row.get(2)?;
        let ordinal: i64 = row.get(3)?;
        let origin = row.get_ref(4)?.as_str()?;
        let disposition = match row.get_ref(5)? {
            ValueRef::Null => None,
            ValueRef::Text(raw) => Some(std::str::from_utf8(raw)?),
            _ => bail!("Malformed State disposition; cached PR membership unavailable"),
        };
        let current = fingerprints(tx, ordinal)?;
        if current.0 != origin || current.1.as_deref() != disposition {
            bail!("State raw terminal origin/disposition changed; cache unavailable");
        }
        let terminal: bool = if disposition.is_some() {
            tx.query_row("SELECT EXISTS(SELECT 1 FROM pr_write_attempts a JOIN pr_write_state_origins_v68 o ON o.attempt_id=a.attempt_id JOIN pr_write_state_dispositions_v68 d ON d.attempt_id=a.attempt_id WHERE a.attempt_id=?1 AND a.host=?2 AND a.principal=?3 AND a.pr_node=?4 AND a.state='confirmed' AND typeof(a.confirmed_json)='text' AND o.repository_id=?5 AND o.pr_id=?6 AND d.kind='confirmed_effect' AND a.attempt_id=(SELECT MAX(n.attempt_id) FROM pr_write_attempts n JOIN pr_write_state_origins_v68 s ON s.attempt_id=n.attempt_id WHERE n.host=a.host AND n.principal=a.principal AND n.pr_node=a.pr_node))",params![ordinal,scope.host,scope.login,node,repository,pr],|r|r.get(0))?
        } else {
            // ACK alone never hydrates. Only a LATER independently complete
            // genuine source transaction can issue the exact ordinal/source
            // watermark checked below, after strict typed original validation.
            tx.query_row("SELECT EXISTS(SELECT 1 FROM pr_write_attempts a JOIN pr_write_state_origins_v68 o ON o.attempt_id=a.attempt_id WHERE a.attempt_id=?1 AND a.host=?2 AND a.principal=?3 AND a.pr_node=?4 AND a.state='acknowledged' AND typeof(a.confirmed_json)='null' AND o.repository_id=?5 AND o.pr_id=?6 AND NOT EXISTS(SELECT 1 FROM pr_write_state_dispositions_v68 d WHERE d.attempt_id=a.attempt_id) AND a.attempt_id=(SELECT MAX(n.attempt_id) FROM pr_write_attempts n WHERE n.host=a.host AND n.principal=a.principal AND n.pr_node=a.pr_node AND (n.command LIKE 'state-%' OR n.selected_resource LIKE 'state-v68/%' OR EXISTS(SELECT 1 FROM pr_write_state_origins_v68 s WHERE s.attempt_id=n.attempt_id))))",params![ordinal,scope.host,scope.login,node,repository,pr],|r|r.get(0))?
        };
        let fresh:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM pr_write_state_source_watermarks_v73 w JOIN checkpoints c ON c.host=w.host AND c.login=w.principal AND c.source=w.source JOIN sync_reservations r ON r.host=c.host AND r.login=c.login AND r.source=c.source WHERE w.host=?1 AND w.principal=?2 AND w.pr_node=?3 AND w.source=?4 AND typeof(w.source_generation)='integer' AND w.source_generation>0 AND typeof(w.mutation_ordinal)='integer' AND w.mutation_ordinal=?5 AND typeof(w.repository_id)='integer' AND w.repository_id=?6 AND typeof(w.pr_id)='integer' AND w.pr_id=?7 AND c.generation=w.source_generation AND r.generation=w.source_generation AND r.active=1 AND NOT EXISTS(SELECT 1 FROM sync_resource_bindings b WHERE b.host=c.host AND b.login=c.login AND b.source=c.source AND (b.active<>1 OR b.visible_generation IS NULL OR b.visible_generation<>w.source_generation OR b.reservation_generation<>w.source_generation)))",params![scope.host,scope.login,node,source,ordinal,repository,pr],|r|r.get(0))?;
        if !terminal || !fresh {
            bail!("PR state/filter membership and counts unavailable; refresh this actual source explicitly");
        }
    }
    Ok(())
}
