//! Account-scoped SQLite source records, review drafts, and saved searches.
use std::{
    collections::HashSet,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{bail, Context, Result};
mod remote_mapping_candidate_v1;
pub(crate) mod workflow_cleanup_v9;
pub(crate) use remote_mapping_candidate_v1::RemoteMappingCandidateV1;
mod bindings;
pub(crate) mod conversation_refresh_v19;
mod state_filter_v68;
pub use bindings::{
    CanonicalBindingStateMetadata, ResourceBindingMetadata, ResourceBindingSnapshot, SyncBinding,
    SyncBindingKind, SyncBindingStatus, SyncFence, SyncFenceResult, SyncMutationTombstone,
    SyncNamespace,
};
pub(crate) use state_filter_v68::PreparedStateCacheRepairV74;
pub(crate) use state_filter_v68::{
    state_disposition_fingerprint_v74, state_origin_fingerprint_v74,
};
pub(crate) use state_filter_v68::{StateAckCacheEpochV77, StateAckQueueBoundaryV78};
pub(crate) mod busy_v1;
mod metric_trends_v1;
pub use metric_trends_v1::{METRIC_TREND_PAYLOAD_BYTES, METRIC_TREND_RETAINED_DAYS};
mod metrics_retention;
pub use metrics_retention::{
    MetricAuthorityPhase, MetricCategoryBatch, MetricEventTarget, MetricEventsReservation,
    MetricReadIntent, MetricsRetentionAuthority, MetricsRetentionLimits, StoredMetricFrame,
};
mod notification_source_v63;
mod notification_write_v63;
pub(crate) use notification_write_v63::{
    NotificationRequestedV63, NotificationWriteMarkerV63, NotificationWritePermitV63,
    NotificationWriteReferenceV63, NotificationWriteStateV63,
};
mod personal_pr_source_v13;
pub(crate) use personal_pr_source_v13::parent_boundary_v16::ClosedPersonalParentBoundaryV16;
mod repository_catalogue_intents_v1;
pub(crate) use notification_source_v63::NotificationSourceWitnessV63;
pub(crate) use personal_pr_source_v13::{PersonalPrReservationV13, PersonalPrSourceV13};
pub use repository_catalogue_intents_v1::{
    CatalogueIntentBoundaryError, CatalogueIntentReservation, CatalogueIntentSource,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scope {
    pub host: String,
    pub login: String,
}

impl Scope {
    /// The SQLite column retains its historical `login` name, but GitHub scopes
    /// use a tagged immutable principal key. Renamed/reassigned logins must
    /// never unlock another user's cached private records or drafts.
    pub fn github(host: impl Into<String>, account_id: Option<u64>) -> Result<Self> {
        let id = account_id.filter(|id|*id > 0).context("GitHub account ID is unavailable; private cache remains hidden. Refresh with a supported gh host.")?;
        Self::new(host, format!("user-id:{id}"))
    }
    pub fn new(host: impl Into<String>, login: impl Into<String>) -> Result<Self> {
        let scope = Self {
            host: host.into(),
            login: login.into(),
        };
        if scope.host.trim().is_empty() || scope.login.trim().is_empty() {
            bail!("Cache scope requires host and login");
        }
        Ok(scope)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub kind: String,
    pub id: String,
    pub repository: String,
    pub payload: Value,
    pub fetched_at: u64,
    pub source_updated_at: Option<String>,
    pub complete: bool,
}

impl Record {
    pub fn new(
        kind: impl Into<String>,
        id: impl Into<String>,
        repository: impl Into<String>,
        payload: Value,
    ) -> Self {
        Self {
            kind: kind.into(),
            id: id.into(),
            repository: repository.into(),
            payload,
            fetched_at: now(),
            source_updated_at: None,
            complete: false,
        }
    }
}

pub struct Cache {
    connection: Connection,
}

#[path = "storage/state_source_commit_v80.rs"]
mod state_source_commit_v80;
#[path = "storage/state_source_epoch_v76.rs"]
mod state_source_epoch_v76;
pub(crate) use state_source_commit_v80::{CommittedStateSourceV80, PreparedStateSourceCommitV80};
pub(crate) use state_source_epoch_v76::{StateSourceEpochV76, StateStartupReadV80};

/// A committed source generation. `complete` describes actual fetch coverage;
/// explicit reserved batches may prune a bounded authoritative membership union
/// while retaining a truthful partial coverage flag.
#[derive(Debug, Clone, PartialEq)]
pub struct SyncFrame {
    pub source: String,
    pub generation: u64,
    pub fetched_at: u64,
    pub complete: bool,
    pub records: Vec<Record>,
}

/// Fetch completeness and known membership/deletion authority are separate.
/// A partial thread page may prove a deletion in a fully fetched thread while
/// retaining unknown threads, without claiming the whole source was fetched.
pub struct ReservedSyncBatch<'a> {
    pub source: &'a str,
    pub generation: u64,
    pub records: &'a [Record],
    pub complete: bool,
    pub deletion_authoritative: bool,
}
struct SyncPolicy {
    complete: bool,
    prune_missing: bool,
    reservation_authoritative: bool,
}
/// Prepared data ONLY, never a source/access/publication permission. The closed
/// Conversation transaction validates its actual terminal/source witnesses.
struct EncodedRecordV19 {
    kind: String,
    id: String,
    repository: String,
    payload: String,
    fetched_at: u64,
    source_updated_at: Option<String>,
    complete: bool,
}
struct EncodedSourceV19 {
    records: Vec<EncodedRecordV19>,
    previous_members: Vec<(String, String)>,
    published_at: u64,
}
enum SyncRowsV19<'a> {
    Ordinary(&'a [Record]),
    Prepared(&'a EncodedSourceV19),
}
struct SyncRowV19<'a> {
    kind: &'a str,
    id: &'a str,
    repository: &'a str,
    payload: std::borrow::Cow<'a, str>,
    fetched_at: u64,
    source_updated_at: Option<&'a str>,
    complete: bool,
}
impl SyncRowsV19<'_> {
    fn len(&self) -> usize {
        match self {
            Self::Ordinary(rows) => rows.len(),
            Self::Prepared(v) => v.records.len(),
        }
    }
    fn row(&self, index: usize) -> Result<SyncRowV19<'_>> {
        Ok(match self {
            Self::Ordinary(rows) => {
                let r = &rows[index];
                SyncRowV19 {
                    kind: &r.kind,
                    id: &r.id,
                    repository: &r.repository,
                    payload: std::borrow::Cow::Owned(serde_json::to_string(&r.payload)?),
                    fetched_at: r.fetched_at,
                    source_updated_at: r.source_updated_at.as_deref(),
                    complete: r.complete,
                }
            }
            Self::Prepared(v) => {
                let r = &v.records[index];
                SyncRowV19 {
                    kind: &r.kind,
                    id: &r.id,
                    repository: &r.repository,
                    payload: std::borrow::Cow::Borrowed(&r.payload),
                    fetched_at: r.fetched_at,
                    source_updated_at: r.source_updated_at.as_deref(),
                    complete: r.complete,
                }
            }
        })
    }
}

impl Cache {
    pub fn open(path: &Path) -> Result<Self> {
        let private_files = crate::storage_private::prepare(path)?;
        let connection = Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .with_context(|| format!("Open cache {}", path.display()))?;
        let cache = Self::initialize(connection)?;
        drop(private_files);
        Ok(cache)
    }

    pub fn memory() -> Result<Self> {
        Self::initialize(Connection::open_in_memory()?)
    }

    fn initialize(mut connection: Connection) -> Result<Self> {
        crate::storage::busy_v1::install(&connection)?;
        connection.pragma_update(None, "foreign_keys", true)?;
        // TASK-102: an open that has nothing to create or migrate must not queue for the write
        // lock behind slow commits; the check itself only reads.
        if Self::already_initialized(&connection)? {
            return Ok(Self { connection });
        }
        let transaction =
            connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let version: u32 =
            transaction.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version > 4 {
            bail!("Cache schema {version} is newer than this application supports");
        }
        if version == 0 {
            transaction.execute_batch("CREATE TABLE records (
                host TEXT NOT NULL, login TEXT NOT NULL, kind TEXT NOT NULL, id TEXT NOT NULL,
                repository TEXT NOT NULL, payload TEXT NOT NULL, fetched_at INTEGER NOT NULL,
                source_updated_at TEXT, complete INTEGER NOT NULL,
                PRIMARY KEY(host, login, kind, id));
                CREATE INDEX record_repo ON records(host, login, repository, kind);
                CREATE TABLE drafts (host TEXT NOT NULL, login TEXT NOT NULL, resource TEXT NOT NULL,
                body TEXT NOT NULL, updated_at INTEGER NOT NULL, PRIMARY KEY(host, login, resource));
                CREATE TABLE saved_views (host TEXT NOT NULL, login TEXT NOT NULL, name TEXT NOT NULL,
                payload TEXT NOT NULL, PRIMARY KEY(host, login, name));")?;
            transaction.pragma_update(None, "user_version", 1)?;
        }
        if version < 2 {
            transaction.execute_batch("CREATE TABLE checkpoints (
                host TEXT NOT NULL, login TEXT NOT NULL, source TEXT NOT NULL,
                generation INTEGER NOT NULL, fetched_at INTEGER NOT NULL, complete INTEGER NOT NULL,
                PRIMARY KEY(host,login,source));
                CREATE TABLE checkpoint_records (
                host TEXT NOT NULL, login TEXT NOT NULL, source TEXT NOT NULL,
                kind TEXT NOT NULL, id TEXT NOT NULL,
                PRIMARY KEY(host,login,source,kind,id),
                FOREIGN KEY(host,login,kind,id) REFERENCES records(host,login,kind,id) ON DELETE CASCADE);
                CREATE INDEX checkpoint_member ON checkpoint_records(host,login,kind,id);")?;
            transaction.pragma_update(None, "user_version", 2)?;
        }
        if version < 3 {
            transaction.execute_batch(
                "CREATE TABLE IF NOT EXISTS sync_reservations (
                host TEXT NOT NULL, login TEXT NOT NULL, source TEXT NOT NULL,
                generation INTEGER NOT NULL CHECK(generation>0), active INTEGER NOT NULL,
                PRIMARY KEY(host,login,source));",
            )?;
            transaction.pragma_update(None, "user_version", 3)?;
        }
        if version < 4 {
            bindings::initialize_bindings(&transaction)?;
            transaction.pragma_update(None, "user_version", 4)?;
        }
        notification_write_v63::initialize(&transaction)?;
        state_filter_v68::initialize(&transaction)?;
        // Corpus mappings/recovery describe deliberate user intent, not derived
        // cache data. Migrate atomically before any caller can clear records.
        transaction.execute_batch(
            "INSERT OR IGNORE INTO saved_views(host,login,name,payload)
            SELECT host,login,kind||':'||id,payload FROM records
            WHERE kind IN ('corpus-mapping-v1','corpus-recovery-v1');
            DELETE FROM records WHERE kind IN ('corpus-mapping-v1','corpus-recovery-v1');",
        )?;
        transaction.commit()?;
        Ok(Self { connection })
    }

    /// Whether `connection` already holds everything [`Self::initialize`] creates or migrates:
    /// the current schema version, every table and index it creates (by name, as
    /// `CREATE ... IF NOT EXISTS` would decide) and no legacy corpus records left to move.
    fn already_initialized(connection: &Connection) -> Result<bool> {
        static REFERENCE: std::sync::OnceLock<Vec<(String, String)>> = std::sync::OnceLock::new();
        fn objects(connection: &Connection) -> Result<Vec<(String, String)>> {
            let mut statement = connection.prepare(
                "SELECT type,name FROM sqlite_master WHERE type IN ('table','index')
                AND name NOT LIKE 'sqlite_%'",
            )?;
            let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        }
        let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version != 4 {
            return Ok(false);
        }
        let reference = match REFERENCE.get() {
            Some(reference) => reference,
            None => {
                let fresh = Self::memory()?;
                let reference = objects(&fresh.connection)?;
                REFERENCE.get_or_init(|| reference)
            }
        };
        let present: HashSet<(String, String)> = objects(connection)?.into_iter().collect();
        if !reference.iter().all(|object| present.contains(object)) {
            return Ok(false);
        }
        let legacy: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM records
            WHERE kind IN ('corpus-mapping-v1','corpus-recovery-v1'))",
            [],
            |row| row.get(0),
        )?;
        Ok(!legacy)
    }

    /// Publish one source atomically. An older/repeated generation cannot replace
    /// newer visible data, even when background work finishes out of order.
    pub fn commit_sync(
        &mut self,
        scope: &Scope,
        source: &str,
        generation: u64,
        records: &[Record],
        complete: bool,
    ) -> Result<bool> {
        // Take the write lock up front: a deferred read-then-write upgrade gets SQLITE_BUSY at
        // once, without the busy timeout, while the draft writer holds the database.
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let published = Self::publish_sync(
            &transaction,
            scope,
            source,
            generation,
            records,
            SyncPolicy {
                complete,
                prune_missing: complete,
                reservation_authoritative: false,
            },
        )?;
        transaction.commit()?;
        Ok(published)
    }

    /// Reserve before fetching, so an older slow read cannot win by finishing
    /// later. Durable counters are independent of wall-clock timestamps.
    pub fn reserve_sync_generation(&mut self, scope: &Scope, source: &str) -> Result<u64> {
        if source.is_empty() || source.contains('\0') || source.len() > 2048 {
            bail!("A bounded canonical sync source is required");
        }
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let generation = Self::reserve_sync_generation_row_v20(&transaction, scope, source)?;
        transaction.commit()?;
        Ok(generation)
    }

    // Same normal counter kernel, also used by the CLOSED actual Conversation
    // read-attempt transaction. No caller-supplied generation or callback.
    fn reserve_sync_generation_row_v20(
        transaction: &rusqlite::Transaction<'_>,
        scope: &Scope,
        source: &str,
    ) -> Result<u64> {
        let reserved: Option<u64> = transaction
            .query_row(
                "SELECT generation FROM sync_reservations WHERE host=?1 AND login=?2 AND source=?3",
                params![scope.host, scope.login, source],
                |row| row.get(0),
            )
            .optional()?;
        let published: Option<u64> = transaction
            .query_row(
                "SELECT generation FROM checkpoints WHERE host=?1 AND login=?2 AND source=?3",
                params![scope.host, scope.login, source],
                |row| row.get(0),
            )
            .optional()?;
        let previous = reserved.unwrap_or(0).max(published.unwrap_or(0));
        if previous >= i64::MAX as u64 {
            bail!("Sync generation exhausted; no reservation or publication changed");
        }
        let generation = previous + 1;
        transaction.execute(
            "INSERT INTO sync_reservations VALUES(?1,?2,?3,?4,1)
            ON CONFLICT(host,login,source) DO UPDATE SET generation=excluded.generation,active=1",
            params![scope.host, scope.login, source, generation],
        )?;
        Ok(generation)
    }

    pub fn sync_reservation(&self, scope: &Scope, source: &str) -> Result<Option<u64>> {
        Ok(self.connection.query_row("SELECT generation FROM sync_reservations WHERE host=?1 AND login=?2 AND source=?3 AND active=1",
            params![scope.host,scope.login,source], |row|row.get(0)).optional()?)
    }

    /// Reservation check and visible publication share the same write lock.
    /// Clearing/revoking while a read is in flight invalidates its permit.
    pub fn commit_reserved_sync(
        &mut self,
        scope: &Scope,
        source: &str,
        generation: u64,
        records: &[Record],
        complete: bool,
    ) -> Result<bool> {
        self.commit_reserved_batch(
            scope,
            ReservedSyncBatch {
                source,
                generation,
                records,
                complete,
                deletion_authoritative: complete,
            },
        )
    }

    pub fn commit_reserved_batch(
        &mut self,
        scope: &Scope,
        batch: ReservedSyncBatch<'_>,
    ) -> Result<bool> {
        if batch.complete && !batch.deletion_authoritative {
            bail!("Complete source coverage requires authoritative membership");
        }
        let ReservedSyncBatch {
            source,
            generation,
            records,
            complete,
            deletion_authoritative,
        } = batch;
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let current:Option<u64>=transaction.query_row("SELECT generation FROM sync_reservations WHERE host=?1 AND login=?2 AND source=?3 AND active=1",
            params![scope.host,scope.login,source],|row|row.get(0)).optional()?;
        if current != Some(generation) {
            return Ok(false);
        }
        let published = Self::publish_sync(
            &transaction,
            scope,
            source,
            generation,
            records,
            SyncPolicy {
                complete,
                prune_missing: deletion_authoritative,
                reservation_authoritative: true,
            },
        )?;
        if published {
            bindings::mark_binding_published(&transaction, scope, source, generation)?;
        }
        transaction.commit()?;
        Ok(published)
    }

    fn publish_sync(
        transaction: &rusqlite::Transaction<'_>,
        scope: &Scope,
        source: &str,
        generation: u64,
        records: &[Record],
        policy: SyncPolicy,
    ) -> Result<bool> {
        Self::publish_sync_rows_v19(
            transaction,
            scope,
            source,
            generation,
            SyncRowsV19::Ordinary(records),
            policy,
        )
    }
    /// Existing publication kernel, with a closed preencoded-data profile.
    /// Generic callers retain their one-at-a-time JSON serialization/memory
    /// behavior. Conversation supplies all payloads/old keys BEFORE final UID.
    fn publish_sync_rows_v19(
        transaction: &rusqlite::Transaction<'_>,
        scope: &Scope,
        source: &str,
        generation: u64,
        records: SyncRowsV19<'_>,
        policy: SyncPolicy,
    ) -> Result<bool> {
        if source.is_empty() || generation == 0 || generation > i64::MAX as u64 {
            bail!("A sync source and positive bounded generation are required");
        }
        if let SyncRowsV19::Ordinary(rows) = &records {
            let mut keys = std::collections::BTreeSet::new();
            for record in *rows {
                if !keys.insert((&record.kind, &record.id)) {
                    bail!("A source generation contains duplicate record identities");
                }
            }
        }
        let previous: Option<u64> = transaction
            .query_row(
                "SELECT generation FROM checkpoints WHERE host=?1 AND login=?2 AND source=?3",
                params![scope.host, scope.login, source],
                |row| row.get(0),
            )
            .optional()?;
        if previous.is_some_and(|previous| generation <= previous) {
            return Ok(false);
        }
        let previous_members: std::borrow::Cow<'_, [(String, String)]> =
            if let SyncRowsV19::Prepared(prepared) = &records {
                std::borrow::Cow::Borrowed(&prepared.previous_members)
            } else if policy.prune_missing {
                let mut statement = transaction.prepare(
                "SELECT kind,id FROM checkpoint_records WHERE host=?1 AND login=?2 AND source=?3",
            )?;
                let members = statement
                    .query_map(params![scope.host, scope.login, source], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                std::borrow::Cow::Owned(members)
            } else {
                std::borrow::Cow::Borrowed(&[])
            };
        if policy.prune_missing {
            transaction.execute(
                "DELETE FROM checkpoint_records WHERE host=?1 AND login=?2 AND source=?3",
                params![scope.host, scope.login, source],
            )?;
        }
        for index in 0..records.len() {
            let record = records.row(index)?;
            transaction.execute("INSERT INTO records VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
                ON CONFLICT(host,login,kind,id) DO UPDATE SET repository=excluded.repository,payload=excluded.payload,
                fetched_at=excluded.fetched_at,source_updated_at=excluded.source_updated_at,complete=excluded.complete
                WHERE ?10 OR excluded.fetched_at>=records.fetched_at",
                params![scope.host,scope.login,record.kind,record.id,record.repository,record.payload.as_ref(),record.fetched_at,record.source_updated_at,record.complete,policy.reservation_authoritative])?;
            transaction.execute(
                "INSERT OR IGNORE INTO checkpoint_records VALUES (?1,?2,?3,?4,?5)",
                params![scope.host, scope.login, source, record.kind, record.id],
            )?;
        }
        // Records may belong to more than one saved source. Removing one source's
        // member must not erase another source or any user's drafts/saved views.
        if policy.prune_missing {
            for (kind, id) in previous_members.iter() {
                transaction.execute(
                    "DELETE FROM records WHERE host=?1 AND login=?2 AND kind=?3 AND id=?4
                    AND NOT EXISTS (SELECT 1 FROM checkpoint_records m WHERE m.host=records.host
                    AND m.login=records.login AND m.kind=records.kind AND m.id=records.id)",
                    params![scope.host, scope.login, kind, id],
                )?;
            }
        }
        transaction.execute(
            "INSERT INTO checkpoints VALUES (?1,?2,?3,?4,?5,?6)
            ON CONFLICT(host,login,source) DO UPDATE SET generation=excluded.generation,
            fetched_at=excluded.fetched_at,complete=excluded.complete",
            params![
                scope.host,
                scope.login,
                source,
                generation,
                match records {
                    SyncRowsV19::Ordinary(_) => now(),
                    SyncRowsV19::Prepared(v) => v.published_at,
                },
                policy.complete
            ],
        )?;
        Ok(true)
    }

    /// Checkpoint metadata only: never deserialize/copy source document bodies.
    pub fn sync_checkpoint(&self, scope: &Scope, source: &str) -> Result<Option<(u64, bool)>> {
        Ok(self.connection.query_row(
            "SELECT generation,complete FROM checkpoints WHERE host=?1 AND login=?2 AND source=?3",
            params![scope.host,scope.login,source],
            |row|Ok((row.get(0)?,row.get(1)?)),
        ).optional()?)
    }

    /// A cheap change token over one account's records of `kinds` and its checkpoints whose
    /// source starts with `source_prefix`: counts, stored sizes and times, generations and
    /// memberships. It never copies payloads. Any publication, prune, clear or revoke of those
    /// rows changes it, so a caller may keep decoded rows while the token is unchanged.
    pub fn records_change_token(
        &self,
        scope: &Scope,
        kinds: &[&str],
        source_prefix: &str,
    ) -> Result<String> {
        let mut token = String::new();
        for kind in kinds {
            let (count, bytes, latest, total): (i64, f64, i64, f64) = self.connection.query_row(
                "SELECT COUNT(*),TOTAL(length(CAST(payload AS BLOB))),COALESCE(MAX(fetched_at),0),TOTAL(fetched_at)
                FROM records WHERE host=?1 AND login=?2 AND kind=?3",
                params![scope.host, scope.login, kind],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )?;
            token.push_str(&format!("{kind}:{count}:{bytes}:{latest}:{total};"));
        }
        // Prefix match without LIKE wildcards: [prefix, prefix + U+10FFFF).
        let upper = format!("{source_prefix}\u{10FFFF}");
        let mut statement = self.connection.prepare(
            "SELECT c.source,c.generation,c.fetched_at,c.complete,
            (SELECT COUNT(*) FROM checkpoint_records m WHERE m.host=c.host AND m.login=c.login AND m.source=c.source)
            FROM checkpoints c WHERE c.host=?1 AND c.login=?2 AND c.source>=?3 AND c.source<?4 ORDER BY c.source",
        )?;
        let rows = statement.query_map(
            params![scope.host, scope.login, source_prefix, upper],
            |row| {
                Ok(format!(
                    "{}:{}:{}:{}:{};",
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, bool>(3)?,
                    row.get::<_, i64>(4)?
                ))
            },
        )?;
        for row in rows {
            token.push_str(&row?);
        }
        Ok(token)
    }

    /// Canonical, account/source-scoped membership without fetching payloads.
    pub fn sync_members(&self, scope: &Scope, source: &str) -> Result<Vec<(String, String)>> {
        let mut statement = self.connection.prepare(
            "SELECT r.kind,r.id FROM records r JOIN checkpoint_records m ON m.host=r.host AND m.login=r.login AND m.kind=r.kind AND m.id=r.id WHERE m.host=?1 AND m.login=?2 AND m.source=?3 ORDER BY r.kind,r.id"
        )?;
        let rows = statement.query_map(params![scope.host, scope.login, source], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;
        rows.map(|row| Ok(row?)).collect()
    }

    /// Byte/count budget preflight without copying or deserializing bodies.
    /// CAST AS BLOB measures stored UTF-8 bytes, not Unicode character count.
    pub fn sync_footprint(&self, scope: &Scope, source: &str) -> Result<(usize, u64)> {
        let (count,bytes):(u64,u64)=self.connection.query_row("SELECT COUNT(*),COALESCE(SUM(length(CAST(r.payload AS BLOB))),0) FROM records r JOIN checkpoint_records m ON m.host=r.host AND m.login=r.login AND m.kind=r.kind AND m.id=r.id WHERE m.host=?1 AND m.login=?2 AND m.source=?3",
            params![scope.host,scope.login,source],|row|Ok((row.get(0)?,row.get(1)?)))?;
        Ok((
            usize::try_from(count).context("Source membership count exceeds platform budget")?,
            bytes,
        ))
    }

    pub fn sync_frame(&self, scope: &Scope, source: &str) -> Result<Option<SyncFrame>> {
        let tx = self.connection.unchecked_transaction()?;
        state_filter_v68::guard_source(&tx, scope, source)?;
        let checkpoint = tx.query_row(
            "SELECT generation,fetched_at,complete FROM checkpoints WHERE host=?1 AND login=?2 AND source=?3",
            params![scope.host,scope.login,source],
            |row| Ok((row.get::<_,u64>(0)?,row.get::<_,u64>(1)?,row.get::<_,bool>(2)?)),
        ).optional()?;
        let Some((generation, fetched_at, complete)) = checkpoint else {
            tx.commit()?;
            return Ok(None);
        };
        let mut statement = tx.prepare("SELECT r.kind,r.id,r.repository,r.payload,r.fetched_at,r.source_updated_at,r.complete
            FROM records r JOIN checkpoint_records m ON m.host=r.host AND m.login=r.login AND m.kind=r.kind AND m.id=r.id
            WHERE m.host=?1 AND m.login=?2 AND m.source=?3 ORDER BY r.kind,r.id")?;
        let records = statement
            .query_map(params![scope.host, scope.login, source], record_row)?
            .map(|row| parse_record(row?))
            .collect::<Result<Vec<_>>>()?;
        drop(statement);
        tx.commit()?;
        Ok(Some(SyncFrame {
            source: source.into(),
            generation,
            fetched_at,
            complete,
            records,
        }))
    }

    pub fn put(&self, scope: &Scope, record: &Record) -> Result<()> {
        self.connection.execute("INSERT INTO records VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
            ON CONFLICT(host,login,kind,id) DO UPDATE SET repository=excluded.repository,payload=excluded.payload,
            fetched_at=excluded.fetched_at,source_updated_at=excluded.source_updated_at,complete=excluded.complete",
            params![scope.host,scope.login,record.kind,record.id,record.repository,serde_json::to_string(&record.payload)?,record.fetched_at,record.source_updated_at,record.complete])?;
        Ok(())
    }

    pub fn get(&self, scope: &Scope, kind: &str, id: &str) -> Result<Option<Record>> {
        let row = self.connection.query_row("SELECT kind,id,repository,payload,fetched_at,source_updated_at,complete FROM records WHERE host=?1 AND login=?2 AND kind=?3 AND id=?4",
            params![scope.host,scope.login,kind,id], record_row).optional()?;
        row.map(parse_record).transpose()
    }

    pub fn list(&self, scope: &Scope, kind: Option<&str>) -> Result<Vec<Record>> {
        let mut statement = self.connection.prepare("SELECT kind,id,repository,payload,fetched_at,source_updated_at,complete FROM records WHERE host=?1 AND login=?2 AND (?3 IS NULL OR kind=?3) ORDER BY kind,id")?;
        let rows = statement.query_map(params![scope.host, scope.login, kind], record_row)?;
        rows.map(|row| parse_record(row?)).collect()
    }

    pub fn delete_record(&self, scope: &Scope, kind: &str, id: &str) -> Result<()> {
        self.connection.execute(
            "DELETE FROM records WHERE host=?1 AND login=?2 AND kind=?3 AND id=?4",
            params![scope.host, scope.login, kind, id],
        )?;
        Ok(())
    }

    pub fn revoke_repository(&self, scope: &Scope, repository: &str) -> Result<usize> {
        let transaction = self.connection.unchecked_transaction()?;
        Self::fence_reservations(&transaction, scope)?;
        bindings::fence_binding_visibility(&transaction, scope, Some(repository))?;
        transaction.execute("UPDATE checkpoints SET complete=0 WHERE host=?1 AND login=?2 AND source IN
            (SELECT m.source FROM checkpoint_records m JOIN records r ON r.host=m.host AND r.login=m.login AND r.kind=m.kind AND r.id=m.id
            WHERE r.host=?1 AND r.login=?2 AND r.repository=?3)", params![scope.host,scope.login,repository])?;
        let removed = transaction.execute(
            "DELETE FROM records WHERE host=?1 AND login=?2 AND repository=?3",
            params![scope.host, scope.login, repository],
        )?;
        transaction.commit()?;
        Ok(removed)
    }

    pub fn put_draft(&self, scope: &Scope, resource: &str, body: &str) -> Result<()> {
        self.connection.execute("INSERT INTO drafts VALUES (?1,?2,?3,?4,?5) ON CONFLICT(host,login,resource) DO UPDATE SET body=excluded.body,updated_at=excluded.updated_at", params![scope.host,scope.login,resource,body,now()])?;
        Ok(())
    }

    pub fn draft(&self, scope: &Scope, resource: &str) -> Result<Option<String>> {
        Ok(self
            .connection
            .query_row(
                "SELECT body FROM drafts WHERE host=?1 AND login=?2 AND resource=?3",
                params![scope.host, scope.login, resource],
                |row| row.get(0),
            )
            .optional()?)
    }

    /// Native metadata recovery preflights UTF8 bytes in SQLite before copying
    /// or decoding any private local draft. A malformed/oversized original
    /// never silently becomes a fresh intent.
    pub fn bounded_draft(
        &self,
        scope: &Scope,
        resource: &str,
        max_bytes: usize,
    ) -> Result<Option<String>> {
        if max_bytes == 0 || max_bytes > 128 * 1024 {
            bail!("Invalid private draft byte budget");
        }
        self.bounded_draft_bytes_v9(scope, resource, max_bytes)
    }

    /// Closed workflow-edit text profile; never a caller-selected larger cap.
    /// The actual retained intent supplies account/resource, not decoded text.
    pub(crate) fn bounded_workflow_edit_draft_v9(
        &self,
        intent: &crate::scoped_actions_workflow_edit_v9::WorkflowEditIntentV9,
    ) -> Result<Option<String>> {
        self.bounded_draft_bytes_v9(
            intent.source().original().scope(),
            intent.key(),
            crate::scoped_actions_workflow_edit_v9::draft_v9::WORKFLOW_DRAFT_BYTES_V9,
        )
    }

    fn bounded_draft_bytes_v9(
        &self,
        scope: &Scope,
        resource: &str,
        max_bytes: usize,
    ) -> Result<Option<String>> {
        let transaction = self.connection.unchecked_transaction()?;
        let bytes: Option<usize> = transaction.query_row(
            "SELECT length(CAST(body AS BLOB)) FROM drafts WHERE host=?1 AND login=?2 AND resource=?3",
            params![scope.host, scope.login, resource], |row| row.get(0),
        ).optional()?;
        if bytes.is_some_and(|bytes| bytes > max_bytes) {
            bail!("Private draft exceeds its UTF8 recovery byte budget; no fields or approval restored");
        }
        let body = transaction.query_row(
            "SELECT body FROM drafts WHERE host=?1 AND login=?2 AND resource=?3 AND length(CAST(body AS BLOB))<=?4",
            params![scope.host, scope.login, resource, max_bytes], |row| row.get(0),
        ).optional()?;
        transaction.commit()?;
        Ok(body)
    }

    pub fn remove_draft(&self, scope: &Scope, resource: &str) -> Result<()> {
        self.connection.execute(
            "DELETE FROM drafts WHERE host=?1 AND login=?2 AND resource=?3",
            params![scope.host, scope.login, resource],
        )?;
        Ok(())
    }

    /// Publication-only exact-body cleanup. ONE conditional statement, never a
    /// separate observation then unconditional delete. A missing/replaced row
    /// is refusal, not proof that cleanup succeeded or permission to retry.
    pub(crate) fn remove_publication_draft_exact_v2(
        &self,
        scope: &Scope,
        resource: &str,
        body: &str,
    ) -> Result<()> {
        Self::remove_publication_draft_exact_connection_v85(&self.connection, scope, resource, body)
    }
    /// SAME fixed predicate, also used inside the native atomic projection's
    /// existing Immediate transaction. No connection/source constructor.
    pub(crate) fn remove_publication_draft_exact_connection_v85(
        connection: &Connection,
        scope: &Scope,
        resource: &str,
        body: &str,
    ) -> Result<()> {
        if body.len() > 128 * 1024 {
            bail!("Original acknowledged draft exceeds publication cleanup bound");
        }
        let affected=connection.execute(
            "DELETE FROM drafts WHERE host=?1 AND login=?2 AND resource=?3 AND body=?4 COLLATE BINARY",
            params![scope.host,scope.login,resource,body],
        )?;
        if affected != 1 {
            bail!("Original acknowledged draft changed or is absent; newer draft retained, no cleanup retry");
        }
        Ok(())
    }

    pub(crate) fn remove_workflow_edit_draft_exact_v9(
        &self,
        scope: &Scope,
        resource: &str,
        body: &str,
    ) -> Result<()> {
        if body.len() > crate::scoped_actions_workflow_edit_v9::draft_v9::WORKFLOW_DRAFT_BYTES_V9 {
            bail!("Original acknowledged workflow draft exceeds fixed cleanup bound");
        }
        let affected=self.connection.execute(
            "DELETE FROM drafts WHERE host=?1 AND login=?2 AND resource=?3 AND body=?4 COLLATE BINARY",
            params![scope.host,scope.login,resource,body],
        )?;
        if affected != 1 {
            bail!("Original acknowledged draft changed or is absent; newer draft retained, no cleanup retry");
        }
        Ok(())
    }

    pub fn save_view(&self, scope: &Scope, name: &str, value: &Value) -> Result<()> {
        self.connection.execute("INSERT INTO saved_views VALUES (?1,?2,?3,?4) ON CONFLICT(host,login,name) DO UPDATE SET payload=excluded.payload", params![scope.host,scope.login,name,serde_json::to_string(value)?])?;
        Ok(())
    }

    pub fn saved_views(&self, scope: &Scope) -> Result<Vec<(String, Value)>> {
        let mut statement = self.connection.prepare(
            "SELECT name,payload FROM saved_views WHERE host=?1 AND login=?2 ORDER BY name",
        )?;
        let rows = statement.query_map(params![scope.host, scope.login], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.map(|row| {
            let (name, body) = row?;
            Ok((name, serde_json::from_str(&body)?))
        })
        .collect()
    }
    /// Exact account-scoped intentional context; cache clearing does not remove it.
    pub fn saved_view(&self, scope: &Scope, name: &str) -> Result<Option<Value>> {
        self.connection
            .query_row(
                "SELECT payload FROM saved_views WHERE host=?1 AND login=?2 AND name=?3",
                params![scope.host, scope.login, name],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(|payload| serde_json::from_str(&payload).map_err(Into::into))
            .transpose()
    }

    /// Small preference records are bounded in SQL by UTF-8 bytes before any
    /// body is returned or parsed. A corrupt oversized value is not a default.
    pub fn saved_view_bounded(
        &self,
        scope: &Scope,
        name: &str,
        max_bytes: usize,
    ) -> Result<Option<Value>> {
        if !(1..=128 * 1024).contains(&max_bytes) {
            bail!("Saved preference byte budget must be1..128KiB");
        }
        let row:Option<(i64,Option<String>)>=self.connection.query_row(
            "SELECT length(CAST(payload AS BLOB)),CASE WHEN length(CAST(payload AS BLOB))<=?4 THEN payload ELSE NULL END FROM saved_views WHERE host=?1 AND login=?2 AND name=?3",
            params![scope.host,scope.login,name,max_bytes], |row|Ok((row.get(0)?,row.get(1)?)),
        ).optional()?;
        let Some((bytes, payload)) = row else {
            return Ok(None);
        };
        if bytes < 0 || bytes as u64 > max_bytes as u64 {
            bail!("Saved preference exceeds its UTF-8 byte budget; no value was parsed");
        }
        let payload = payload.context("Saved preference payload is unavailable")?;
        Ok(Some(serde_json::from_str(&payload)?))
    }
    pub fn remove_saved_view(&self, scope: &Scope, name: &str) -> Result<()> {
        self.connection.execute(
            "DELETE FROM saved_views WHERE host=?1 AND login=?2 AND name=?3",
            params![scope.host, scope.login, name],
        )?;
        Ok(())
    }

    pub fn clear_scope(&mut self, scope: &Scope, include_drafts: bool) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        Self::fence_reservations(&transaction, scope)?;
        bindings::fence_binding_visibility(&transaction, scope, None)?;
        transaction.execute(
            "DELETE FROM checkpoints WHERE host=?1 AND login=?2",
            params![scope.host, scope.login],
        )?;
        transaction.execute(
            "DELETE FROM records WHERE host=?1 AND login=?2",
            params![scope.host, scope.login],
        )?;
        transaction.execute(
            "DELETE FROM saved_views WHERE host=?1 AND login=?2",
            params![scope.host, scope.login],
        )?;
        if include_drafts {
            transaction.execute(
                "DELETE FROM drafts WHERE host=?1 AND login=?2",
                params![scope.host, scope.login],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Remove derivative source data without deleting drafts or saved searches.
    pub fn clear_records(&mut self, scope: &Scope) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        Self::clear_records_transaction_v5(&transaction, scope)?;
        transaction.commit()?;
        Ok(())
    }

    // SAME local deletion kernel. The closed Catalogue caller validates its
    // original source/file/clock under this SAME transaction before entering.
    fn clear_records_transaction_v5(
        transaction: &rusqlite::Transaction<'_>,
        scope: &Scope,
    ) -> Result<()> {
        Self::fence_reservations(transaction, scope)?;
        bindings::fence_binding_visibility(transaction, scope, None)?;
        transaction.execute(
            "DELETE FROM checkpoints WHERE host=?1 AND login=?2",
            params![scope.host, scope.login],
        )?;
        transaction.execute(
            "DELETE FROM records WHERE host=?1 AND login=?2",
            params![scope.host, scope.login],
        )?;
        Ok(())
    }

    fn fence_reservations(transaction: &rusqlite::Transaction<'_>, scope: &Scope) -> Result<()> {
        transaction.execute("UPDATE sync_reservations SET generation=CASE WHEN generation<9223372036854775807 THEN generation+1 ELSE generation END,active=0 WHERE host=?1 AND login=?2",
            params![scope.host,scope.login])?;
        Ok(())
    }
}

type StoredRecord = (String, String, String, String, u64, Option<String>, bool);
fn record_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredRecord> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
    ))
}
fn parse_record(row: StoredRecord) -> Result<Record> {
    let (kind, id, repository, payload, fetched_at, source_updated_at, complete) = row;
    Ok(Record {
        kind,
        id,
        repository,
        payload: serde_json::from_str(&payload)?,
        fetched_at,
        source_updated_at,
        complete,
    })
}
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;

    #[test]
    fn collaboration_index_reserved_partial_union_prunes_known_deletions_without_claiming_complete_fetch(
    ) {
        let mut cache = Cache::memory().unwrap();
        let scope = Scope::github("github.com", Some(42)).unwrap();
        let other = Scope::github("github.com", Some(43)).unwrap();
        let mut keep = Record::new(
            "comment",
            "unknown-thread",
            "org/repo",
            json!({"body":"retained old body"}),
        );
        keep.fetched_at = 10;
        keep.source_updated_at = Some("old-source-time".into());
        let deleted = Record::new(
            "comment",
            "deleted-known",
            "org/repo",
            json!({"body":"remove known deletion"}),
        );
        let shared = Record::new(
            "comment",
            "shared",
            "org/repo",
            json!({"body":"other saved source"}),
        );
        let first = cache.reserve_sync_generation(&scope, "inline").unwrap();
        cache
            .commit_reserved_sync(
                &scope,
                "inline",
                first,
                &[keep.clone(), deleted, shared.clone()],
                true,
            )
            .unwrap();
        cache
            .commit_sync(&scope, "other-view", 1, std::slice::from_ref(&shared), true)
            .unwrap();
        cache
            .put_draft(&scope, "review", "never delete draft")
            .unwrap();
        let fresh = Record::new(
            "comment",
            "known-thread-fresh",
            "org/repo",
            json!({"body":"fresh admitted body"}),
        );
        let next = cache.reserve_sync_generation(&scope, "inline").unwrap();
        let union = [keep.clone(), fresh.clone()];
        assert!(cache
            .commit_reserved_batch(
                &scope,
                ReservedSyncBatch {
                    source: "inline",
                    generation: next,
                    records: &union,
                    complete: false,
                    deletion_authoritative: true
                }
            )
            .unwrap());
        let frame = cache.sync_frame(&scope, "inline").unwrap().unwrap();
        assert!(!frame.complete);
        assert_eq!(frame.generation, next);
        assert!(frame.records.contains(&keep));
        assert!(frame.records.contains(&fresh));
        assert_eq!(frame.records.len(), 2);
        assert!(cache
            .get(&scope, "comment", "deleted-known")
            .unwrap()
            .is_none());
        assert_eq!(
            cache
                .sync_frame(&scope, "other-view")
                .unwrap()
                .unwrap()
                .records,
            vec![shared]
        );
        assert_eq!(
            cache.draft(&scope, "review").unwrap().as_deref(),
            Some("never delete draft")
        );
        assert!(cache.sync_frame(&other, "inline").unwrap().is_none());
        assert!(!cache
            .commit_reserved_batch(
                &scope,
                ReservedSyncBatch {
                    source: "inline",
                    generation: first,
                    records: &[],
                    complete: false,
                    deletion_authoritative: true
                }
            )
            .unwrap());
        assert_eq!(cache.sync_frame(&scope, "inline").unwrap().unwrap(), frame);
    }

    #[test]
    fn collaboration_index_reserved_membership_policy_refuses_false_complete_and_rolls_back_pruning(
    ) {
        let mut cache = Cache::memory().unwrap();
        let scope = Scope::github("github.com", Some(42)).unwrap();
        let first = cache.reserve_sync_generation(&scope, "inline").unwrap();
        let old = Record::new(
            "comment",
            "old",
            "org/repo",
            json!({"body":"keep on error"}),
        );
        cache
            .commit_reserved_sync(&scope, "inline", first, std::slice::from_ref(&old), true)
            .unwrap();
        let next = cache.reserve_sync_generation(&scope, "inline").unwrap();
        assert!(cache
            .commit_reserved_batch(
                &scope,
                ReservedSyncBatch {
                    source: "inline",
                    generation: next,
                    records: &[],
                    complete: true,
                    deletion_authoritative: false
                }
            )
            .is_err());
        assert_eq!(
            cache.sync_frame(&scope, "inline").unwrap().unwrap().records,
            vec![old.clone()]
        );
        cache.connection.execute_batch("CREATE TRIGGER fail_union BEFORE INSERT ON records WHEN NEW.id='fail' BEGIN SELECT RAISE(ABORT,'injected union error'); END;").unwrap();
        let fresh = [Record::new("comment", "fail", "org/repo", json!({}))];
        assert!(cache
            .commit_reserved_batch(
                &scope,
                ReservedSyncBatch {
                    source: "inline",
                    generation: next,
                    records: &fresh,
                    complete: false,
                    deletion_authoritative: true
                }
            )
            .is_err());
        let frame = cache.sync_frame(&scope, "inline").unwrap().unwrap();
        assert_eq!(frame.generation, first);
        assert!(frame.complete);
        assert_eq!(frame.records, vec![old]);
        assert_eq!(
            cache.sync_reservation(&scope, "inline").unwrap(),
            Some(next)
        );
    }

    #[test]
    fn collaboration_index_sync_footprint_counts_utf8_bytes_without_deserializing_and_is_scoped() {
        let mut cache = Cache::memory().unwrap();
        let scope = Scope::github("github.com", Some(42)).unwrap();
        let other = Scope::github("github.com", Some(43)).unwrap();
        let body = json!({"body":"café 雪"});
        let bytes = serde_json::to_vec(&body).unwrap().len() as u64;
        cache
            .commit_sync(
                &scope,
                "chosen",
                1,
                &[Record::new("comment", "one", "org/repo", body)],
                true,
            )
            .unwrap();
        assert_eq!(cache.sync_footprint(&scope, "chosen").unwrap(), (1, bytes));
        assert_eq!(cache.sync_footprint(&other, "chosen").unwrap(), (0, 0));
        assert_eq!(cache.sync_footprint(&scope, "missing").unwrap(), (0, 0));
        cache
            .connection
            .execute("UPDATE records SET payload='雪-corrupt' WHERE id='one'", [])
            .unwrap();
        assert!(cache.sync_frame(&scope, "chosen").is_err());
        assert_eq!(
            cache.sync_footprint(&scope, "chosen").unwrap(),
            (1, "雪-corrupt".len() as u64)
        );
    }

    #[test]
    fn cache_storage_collaboration_index_reserved_generation_not_wallclock_controls_current_body() {
        let mut cache = Cache::memory().unwrap();
        let scope = Scope::github("github.com", Some(42)).unwrap();
        let first = cache
            .reserve_sync_generation(&scope, "discussion:PR_one")
            .unwrap();
        let mut old = Record::new(
            "owned-comment",
            "one",
            "org/repo",
            json!({"body":"old phrase"}),
        );
        old.fetched_at = 100;
        cache
            .commit_reserved_sync(&scope, "discussion:PR_one", first, &[old], true)
            .unwrap();
        let newer = cache
            .reserve_sync_generation(&scope, "discussion:PR_one")
            .unwrap();
        let mut changed = Record::new(
            "owned-comment",
            "one",
            "org/repo",
            json!({"body":"edited phrase"}),
        );
        changed.fetched_at = 50;
        assert!(cache
            .commit_reserved_sync(
                &scope,
                "discussion:PR_one",
                newer,
                std::slice::from_ref(&changed),
                true
            )
            .unwrap());
        assert_eq!(
            cache.get(&scope, "owned-comment", "one").unwrap().unwrap(),
            changed
        );
        let mut legacy_old = Record::new("legacy", "one", "org/repo", json!({"body":"original"}));
        legacy_old.fetched_at = 100;
        cache
            .commit_sync(&scope, "legacy", 1, &[legacy_old.clone()], true)
            .unwrap();
        let mut legacy_new = legacy_old.clone();
        legacy_new.fetched_at = 50;
        legacy_new.payload = json!({"body":"older observed"});
        cache
            .commit_sync(&scope, "legacy", 2, &[legacy_new], true)
            .unwrap();
        assert_eq!(
            cache.get(&scope, "legacy", "one").unwrap().unwrap(),
            legacy_old
        );
    }

    #[test]
    fn cache_storage_private_database_and_wal_are_0600_and_existing_mode_is_repaired() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let parent = directory.path().join("new/private");
        let path = parent.join("cache.sqlite");
        let cache = Cache::open(&path).unwrap();
        assert_eq!(
            fs::metadata(&parent).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let scope = Scope::github("github.com", Some(42)).unwrap();
        cache
            .connection
            .pragma_update(None, "journal_mode", "WAL")
            .unwrap();
        cache.put_draft(&scope, "review", "private body").unwrap();
        for sidecar in ["cache.sqlite-wal", "cache.sqlite-shm"] {
            assert_eq!(
                fs::metadata(parent.join(sidecar))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        drop(cache);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        let reopened = Cache::open(&path).unwrap();
        assert_eq!(
            reopened.draft(&scope, "review").unwrap().as_deref(),
            Some("private body")
        );
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn cache_storage_private_database_rejects_aliases_and_writable_parent_without_touching_targets()
    {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let directory = tempfile::tempdir().unwrap();
        let original = directory.path().join("original");
        let bytes = b"unrelated non-database bytes";
        fs::write(&original, bytes).unwrap();
        fs::set_permissions(&original, fs::Permissions::from_mode(0o644)).unwrap();
        let alias = directory.path().join("alias");
        symlink(&original, &alias).unwrap();
        assert!(Cache::open(&alias).is_err());
        let hard = directory.path().join("hard");
        fs::hard_link(&original, &hard).unwrap();
        assert!(Cache::open(&hard).is_err());
        assert_eq!(fs::read(&original).unwrap(), bytes);
        assert_eq!(
            fs::metadata(&original).unwrap().permissions().mode() & 0o777,
            0o644
        );
        let writable = directory.path().join("writable");
        fs::create_dir(&writable).unwrap();
        fs::set_permissions(&writable, fs::Permissions::from_mode(0o777)).unwrap();
        assert!(Cache::open(&writable.join("cache.sqlite")).is_err());
        assert!(!writable.join("cache.sqlite").exists());
        let linked_parent = directory.path().join("linked-parent");
        symlink(&writable, &linked_parent).unwrap();
        assert!(Cache::open(&linked_parent.join("cache.sqlite")).is_err());
        assert_eq!(
            fs::metadata(&writable).unwrap().permissions().mode() & 0o777,
            0o777
        );
    }

    #[test]
    fn cache_storage_collaboration_index_reservations_order_concurrent_connections_and_restart() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("private/cache.sqlite");
        let scope = Scope::github("github.com", Some(42)).unwrap();
        let other = Scope::github("github.com", Some(43)).unwrap();
        let mut first = Cache::open(&path).unwrap();
        let mut second = Cache::open(&path).unwrap();
        let older = first
            .reserve_sync_generation(&scope, "discussion:PR_one")
            .unwrap();
        let newer = second
            .reserve_sync_generation(&scope, "discussion:PR_one")
            .unwrap();
        assert!(newer > older);
        let old = Record::new(
            "comment",
            "comment_one",
            "org/repo",
            json!({"body":"old read"}),
        );
        let fresh = Record::new(
            "comment",
            "comment_one",
            "org/repo",
            json!({"body":"new read"}),
        );
        assert!(!first
            .commit_reserved_sync(
                &scope,
                "discussion:PR_one",
                older,
                std::slice::from_ref(&old),
                true
            )
            .unwrap());
        assert!(first
            .sync_frame(&scope, "discussion:PR_one")
            .unwrap()
            .is_none());
        assert!(second
            .commit_reserved_sync(
                &scope,
                "discussion:PR_one",
                newer,
                std::slice::from_ref(&fresh),
                true
            )
            .unwrap());
        assert!(!first
            .commit_reserved_sync(&scope, "discussion:PR_one", older, &[old], true)
            .unwrap());
        assert!(!first
            .commit_reserved_sync(&scope, "discussion:PR_one", newer, &[], true)
            .unwrap());
        drop(first);
        drop(second);
        let mut reopened = Cache::open(&path).unwrap();
        assert_eq!(
            reopened
                .sync_frame(&scope, "discussion:PR_one")
                .unwrap()
                .unwrap()
                .records,
            vec![fresh]
        );
        assert!(
            reopened
                .reserve_sync_generation(&scope, "discussion:PR_one")
                .unwrap()
                > newer
        );
        assert_eq!(
            reopened
                .reserve_sync_generation(&scope, "discussion:PR_two")
                .unwrap(),
            1
        );
        assert_eq!(
            reopened
                .reserve_sync_generation(&other, "discussion:PR_one")
                .unwrap(),
            1
        );
        assert!(reopened
            .sync_frame(&other, "discussion:PR_one")
            .unwrap()
            .is_none());
    }

    #[test]
    fn cache_storage_collaboration_index_clear_and_revocation_fence_inflight_without_losing_intent()
    {
        let mut cache = Cache::memory().unwrap();
        let scope = Scope::github("github.com", Some(42)).unwrap();
        let other = Scope::github("github.com", Some(43)).unwrap();
        cache.put_draft(&scope, "draft", "deliberate text").unwrap();
        cache
            .save_view(
                &scope,
                "corpus-mapping-v1:repo",
                &json!({"directory":"owned checkout"}),
            )
            .unwrap();
        let first = cache
            .reserve_sync_generation(&scope, "discussion:PR_one")
            .unwrap();
        let other_generation = cache
            .reserve_sync_generation(&other, "discussion:PR_one")
            .unwrap();
        let record = Record::new(
            "comment",
            "comment_one",
            "org/repo",
            json!({"body":"private"}),
        );
        cache.clear_records(&scope).unwrap();
        assert_eq!(
            cache.sync_reservation(&scope, "discussion:PR_one").unwrap(),
            None
        );
        assert!(!cache
            .commit_reserved_sync(
                &scope,
                "discussion:PR_one",
                first,
                std::slice::from_ref(&record),
                true
            )
            .unwrap());
        assert_eq!(
            cache.sync_reservation(&other, "discussion:PR_one").unwrap(),
            Some(other_generation)
        );
        let next = cache
            .reserve_sync_generation(&scope, "discussion:PR_one")
            .unwrap();
        assert!(next > first);
        assert!(cache
            .commit_reserved_sync(
                &scope,
                "discussion:PR_one",
                next,
                std::slice::from_ref(&record),
                true
            )
            .unwrap());
        let inflight = cache
            .reserve_sync_generation(&scope, "discussion:PR_new")
            .unwrap();
        assert_eq!(cache.revoke_repository(&scope, "org/repo").unwrap(), 1);
        assert!(!cache
            .commit_reserved_sync(
                &scope,
                "discussion:PR_new",
                inflight,
                std::slice::from_ref(&record),
                true
            )
            .unwrap());
        assert!(
            !cache
                .sync_frame(&scope, "discussion:PR_one")
                .unwrap()
                .unwrap()
                .complete
        );
        assert!(cache.list(&scope, None).unwrap().is_empty());
        assert_eq!(
            cache.draft(&scope, "draft").unwrap().as_deref(),
            Some("deliberate text")
        );
        assert!(cache
            .saved_view(&scope, "corpus-mapping-v1:repo")
            .unwrap()
            .is_some());
        let before_clear = cache
            .reserve_sync_generation(&scope, "discussion:PR_one")
            .unwrap();
        cache.clear_scope(&scope, false).unwrap();
        assert!(!cache
            .commit_reserved_sync(&scope, "discussion:PR_one", before_clear, &[record], true)
            .unwrap());
        assert!(
            cache
                .reserve_sync_generation(&scope, "discussion:PR_one")
                .unwrap()
                > before_clear
        );
        assert!(cache.draft(&scope, "draft").unwrap().is_some());
    }

    #[test]
    fn cache_storage_collaboration_index_reserved_publication_and_fencing_are_atomic_on_sql_failure(
    ) {
        let mut cache = Cache::memory().unwrap();
        let scope = Scope::github("github.com", Some(42)).unwrap();
        let generation = cache
            .reserve_sync_generation(&scope, "discussion:PR_one")
            .unwrap();
        let record = Record::new("comment", "one", "org/repo", json!({"body":"original"}));
        cache
            .commit_reserved_sync(
                &scope,
                "discussion:PR_one",
                generation,
                std::slice::from_ref(&record),
                true,
            )
            .unwrap();
        let next = cache
            .reserve_sync_generation(&scope, "discussion:PR_one")
            .unwrap();
        cache.connection.execute_batch("CREATE TRIGGER fail_reservation_publish BEFORE INSERT ON records WHEN NEW.id='fail' BEGIN SELECT RAISE(ABORT,'injected publication error'); END;").unwrap();
        assert!(cache
            .commit_reserved_sync(
                &scope,
                "discussion:PR_one",
                next,
                &[
                    Record::new("comment", "one", "org/repo", json!({"body":"changed"})),
                    Record::new("comment", "fail", "org/repo", json!({}))
                ],
                true
            )
            .is_err());
        assert_eq!(
            cache.sync_reservation(&scope, "discussion:PR_one").unwrap(),
            Some(next)
        );
        assert_eq!(
            cache
                .sync_frame(&scope, "discussion:PR_one")
                .unwrap()
                .unwrap()
                .records,
            vec![record.clone()]
        );
        cache.connection.execute_batch("CREATE TRIGGER fail_clear BEFORE DELETE ON records BEGIN SELECT RAISE(ABORT,'injected deletion error'); END;").unwrap();
        assert!(cache.clear_records(&scope).is_err());
        assert_eq!(
            cache.sync_reservation(&scope, "discussion:PR_one").unwrap(),
            Some(next)
        );
        assert_eq!(
            cache
                .sync_frame(&scope, "discussion:PR_one")
                .unwrap()
                .unwrap()
                .records,
            vec![record]
        );
        assert!(cache.revoke_repository(&scope, "org/repo").is_err());
        assert_eq!(
            cache.sync_reservation(&scope, "discussion:PR_one").unwrap(),
            Some(next)
        );
        assert!(
            cache
                .sync_frame(&scope, "discussion:PR_one")
                .unwrap()
                .unwrap()
                .complete
        );
    }

    #[test]
    fn cache_storage_collaboration_index_schema2_migration_and_overflow_never_reuse_generations() {
        let mut cache = Cache::memory().unwrap();
        let scope = Scope::github("github.com", Some(42)).unwrap();
        cache.put_draft(&scope, "review", "keep").unwrap();
        cache
            .save_view(&scope, "selection", &json!({"repo":"org/repo"}))
            .unwrap();
        cache
            .commit_sync(
                &scope,
                "discussion:PR_one",
                50,
                &[Record::new(
                    "comment",
                    "one",
                    "org/repo",
                    json!({"body":"cached"}),
                )],
                true,
            )
            .unwrap();
        cache
            .connection
            .execute_batch("DROP TABLE sync_reservations; PRAGMA user_version=2;")
            .unwrap();
        let mut migrated = Cache::initialize(cache.connection).unwrap();
        assert_eq!(
            migrated
                .connection
                .pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))
                .unwrap(),
            4
        );
        assert_eq!(
            migrated
                .reserve_sync_generation(&scope, "discussion:PR_one")
                .unwrap(),
            51
        );
        assert_eq!(
            migrated.draft(&scope, "review").unwrap().as_deref(),
            Some("keep")
        );
        assert!(migrated.saved_view(&scope, "selection").unwrap().is_some());
        migrated
            .connection
            .execute(
                "UPDATE sync_reservations SET generation=?1",
                params![i64::MAX],
            )
            .unwrap();
        assert!(migrated
            .reserve_sync_generation(&scope, "discussion:PR_one")
            .is_err());
        migrated.clear_records(&scope).unwrap();
        assert_eq!(
            migrated
                .sync_reservation(&scope, "discussion:PR_one")
                .unwrap(),
            None
        );
        assert!(!migrated
            .commit_reserved_sync(&scope, "discussion:PR_one", i64::MAX as u64, &[], true)
            .unwrap());
        assert!(migrated
            .reserve_sync_generation(&scope, "discussion:PR_one")
            .is_err());
        assert!(migrated
            .sync_frame(&scope, "discussion:PR_one")
            .unwrap()
            .is_none());
    }

    #[test]
    fn cache_storage_open_of_a_current_cache_skips_the_write_lock_and_repairs_a_missing_table() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new/private/cache.sqlite3");
        drop(Cache::open(&path).unwrap());
        let holder = Connection::open(&path).unwrap();
        holder.execute_batch("BEGIN IMMEDIATE").unwrap();
        let started = std::time::Instant::now();
        let reopened = Cache::open(&path).unwrap();
        assert!(started.elapsed() < crate::storage::busy_v1::BUSY_BUDGET);
        drop(reopened);
        holder
            .execute_batch("DROP TABLE notification_write_attempts_v63; COMMIT")
            .unwrap();
        let repaired = Cache::open(&path).unwrap();
        let restored: bool = repaired
            .connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master
                WHERE name='notification_write_attempts_v63')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(restored);
    }

    #[test]
    fn selected_corpus_storage_migrates_legacy_intent_atomically_and_preserves_it_on_cache_clear() {
        let cache = Cache::memory().unwrap();
        let scope = Scope::github("github.com", Some(42)).unwrap();
        let other = Scope::github("github.com", Some(43)).unwrap();
        for (principal, kind, id, payload) in [
            (
                &scope,
                "corpus-mapping-v1",
                "repo",
                json!({"directory":"raw mapping"}),
            ),
            (
                &scope,
                "corpus-recovery-v1",
                "attempt",
                json!({"recovery":"owned bytes"}),
            ),
            (
                &other,
                "corpus-mapping-v1",
                "repo",
                json!({"directory":"other private mapping"}),
            ),
        ] {
            cache
                .put(principal, &Record::new(kind, id, "org/repo", payload))
                .unwrap();
        }
        cache
            .save_view(&scope, "search:mine", &json!({"query":"auth"}))
            .unwrap();
        let mut migrated = Cache::initialize(cache.connection).unwrap();
        assert_eq!(
            migrated
                .saved_view(&scope, "corpus-mapping-v1:repo")
                .unwrap(),
            Some(json!({"directory":"raw mapping"}))
        );
        assert_eq!(
            migrated
                .saved_view(&other, "corpus-mapping-v1:repo")
                .unwrap(),
            Some(json!({"directory":"other private mapping"}))
        );
        assert!(migrated
            .list(&scope, Some("corpus-mapping-v1"))
            .unwrap()
            .is_empty());
        migrated.clear_records(&scope).unwrap();
        assert_eq!(migrated.saved_views(&scope).unwrap().len(), 3);
        let reopened = Cache::initialize(migrated.connection).unwrap();
        assert_eq!(
            reopened
                .saved_view(&scope, "corpus-recovery-v1:attempt")
                .unwrap(),
            Some(json!({"recovery":"owned bytes"}))
        );
        assert_eq!(reopened.saved_views(&scope).unwrap().len(), 3);
        reopened
            .remove_saved_view(&scope, "corpus-mapping-v1:repo")
            .unwrap();
        assert!(reopened
            .saved_view(&scope, "corpus-mapping-v1:repo")
            .unwrap()
            .is_none());
        assert!(reopened
            .saved_view(&other, "corpus-mapping-v1:repo")
            .unwrap()
            .is_some());
    }

    #[test]
    fn selected_corpus_storage_newer_durable_mapping_wins_over_legacy_cache_copy() {
        let cache = Cache::memory().unwrap();
        let scope = Scope::github("github.com", Some(42)).unwrap();
        cache
            .put(
                &scope,
                &Record::new(
                    "corpus-mapping-v1",
                    "repo",
                    "org/repo",
                    json!({"directory":"old"}),
                ),
            )
            .unwrap();
        cache
            .save_view(
                &scope,
                "corpus-mapping-v1:repo",
                &json!({"directory":"new"}),
            )
            .unwrap();
        let migrated = Cache::initialize(cache.connection).unwrap();
        assert_eq!(
            migrated
                .saved_view(&scope, "corpus-mapping-v1:repo")
                .unwrap(),
            Some(json!({"directory":"new"}))
        );
        assert!(migrated
            .list(&scope, Some("corpus-mapping-v1"))
            .unwrap()
            .is_empty());
    }

    #[test]
    fn selected_corpus_storage_failed_migration_does_not_drop_legacy_intent() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("private/migration.sqlite3");
        let cache = Cache::open(&path).unwrap();
        let scope = Scope::github("github.com", Some(42)).unwrap();
        cache
            .put(
                &scope,
                &Record::new(
                    "corpus-recovery-v1",
                    "attempt",
                    "org/repo",
                    json!({"recovery":"keep"}),
                ),
            )
            .unwrap();
        cache.connection.execute_batch("CREATE TRIGGER fail_intent BEFORE INSERT ON saved_views BEGIN SELECT RAISE(ABORT,'migration failure'); END;").unwrap();
        assert!(Cache::initialize(cache.connection).is_err());
        let raw = Connection::open(&path).unwrap();
        assert_eq!(
            raw.query_row(
                "SELECT count(*) FROM records WHERE kind='corpus-recovery-v1'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            raw.query_row("SELECT count(*) FROM saved_views", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        raw.execute_batch("DROP TRIGGER fail_intent;").unwrap();
        drop(raw);
        assert_eq!(
            Cache::open(&path)
                .unwrap()
                .saved_view(&scope, "corpus-recovery-v1:attempt")
                .unwrap(),
            Some(json!({"recovery":"keep"}))
        );
    }

    #[test]
    fn cache_storage_checkpoint_members_do_not_parse_or_duplicate_document_bodies() {
        let mut cache = Cache::memory().unwrap();
        let scope = Scope::github("github.com", Some(42)).unwrap();
        cache
            .commit_sync(
                &scope,
                "selected",
                7,
                &[
                    Record::new(
                        "search",
                        "two",
                        "org/repo",
                        json!({"body":"x".repeat(2 * 1024 * 1024)}),
                    ),
                    Record::new("search", "one", "org/repo", json!({"body":"small"})),
                ],
                true,
            )
            .unwrap();
        cache
            .connection
            .execute("UPDATE records SET payload='malformed' WHERE id='two'", [])
            .unwrap();
        assert_eq!(
            cache.sync_checkpoint(&scope, "selected").unwrap(),
            Some((7, true))
        );
        assert_eq!(
            cache.sync_members(&scope, "selected").unwrap(),
            vec![
                ("search".into(), "one".into()),
                ("search".into(), "two".into())
            ]
        );
        assert!(cache.sync_frame(&scope, "selected").is_err());
        let other = Scope::github("github.com", Some(43)).unwrap();
        assert!(cache.sync_checkpoint(&other, "selected").unwrap().is_none());
        assert!(cache.sync_members(&other, "selected").unwrap().is_empty());
        assert!(cache.sync_members(&scope, "absent").unwrap().is_empty());
        cache.delete_record(&scope, "search", "two").unwrap();
        assert_eq!(
            cache.sync_members(&scope, "selected").unwrap(),
            vec![("search".into(), "one".into())]
        );
    }

    #[test]
    fn cache_storage_isolates_hosts_accounts_and_resource_kinds() {
        let cache = Cache::memory().unwrap();
        let alice = Scope::new("github.com", "alice").unwrap();
        let bob = Scope::new("github.com", "bob").unwrap();
        let enterprise = Scope::new("git.example.com", "alice").unwrap();
        let mut record = Record::new("pr", "org/a#1", "org/a", json!({"title":"Private"}));
        record.complete = true;
        record.source_updated_at = Some("2026-10-12T12:00:00Z".into());
        cache.put(&alice, &record).unwrap();
        assert_eq!(cache.get(&alice, "pr", "org/a#1").unwrap(), Some(record));
        assert!(cache.get(&bob, "pr", "org/a#1").unwrap().is_none());
        assert!(cache.get(&enterprise, "pr", "org/a#1").unwrap().is_none());
        assert!(cache.get(&alice, "issue", "org/a#1").unwrap().is_none());
    }

    #[test]
    fn cache_storage_reopen_preserves_drafts_views_and_source_provenance() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("private/cache.sqlite3");
        let scope = Scope::new("github.com", "alice").unwrap();
        {
            let cache = Cache::open(&path).unwrap();
            cache
                .put_draft(&scope, "org/a#3/review", "Not published\nSecond line")
                .unwrap();
            cache
                .save_view(&scope, "Mine", &json!({"mode":"ranked","query":"auth"}))
                .unwrap();
            cache
                .put(
                    &scope,
                    &Record::new("pr", "org/a#3", "org/a", json!({"head":"abc"})),
                )
                .unwrap();
        }
        let mut cache = Cache::open(&path).unwrap();
        assert_eq!(
            cache.draft(&scope, "org/a#3/review").unwrap().as_deref(),
            Some("Not published\nSecond line")
        );
        assert_eq!(cache.saved_views(&scope).unwrap().len(), 1);
        cache.clear_scope(&scope, false).unwrap();
        assert!(cache.list(&scope, None).unwrap().is_empty());
        assert!(cache.draft(&scope, "org/a#3/review").unwrap().is_some());
        cache.clear_scope(&scope, true).unwrap();
        assert!(cache.draft(&scope, "org/a#3/review").unwrap().is_none());
    }

    #[test]
    fn cache_storage_revocation_preserves_other_repository() {
        let cache = Cache::memory().unwrap();
        let scope = Scope::new("github.com", "alice").unwrap();
        for repo in ["org/a", "org/b"] {
            cache
                .put(
                    &scope,
                    &Record::new("pr", format!("{repo}#1"), repo, json!({})),
                )
                .unwrap();
        }
        assert_eq!(cache.revoke_repository(&scope, "org/a").unwrap(), 1);
        assert_eq!(cache.list(&scope, None).unwrap()[0].repository, "org/b");
    }

    #[test]
    fn cache_storage_generations_partial_refresh_and_shared_sources() {
        let mut cache = Cache::memory().unwrap();
        let scope = Scope::new("github.com", "alice").unwrap();
        let a = Record::new("pr", "org/a#1", "org/a", json!({"title":"A"}));
        let b = Record::new("pr", "org/b#1", "org/b", json!({"title":"B"}));
        let standalone = Record::new("pr", "unmanaged", "org/c", json!({}));
        cache.put(&scope, &standalone).unwrap();
        cache
            .commit_sync(&scope, "all", 1, &[a.clone(), b.clone()], true)
            .unwrap();
        cache
            .commit_sync(&scope, "saved-b", 1, std::slice::from_ref(&b), true)
            .unwrap();
        cache
            .commit_sync(&scope, "all", 3, std::slice::from_ref(&a), false)
            .unwrap();
        assert_eq!(
            cache
                .sync_frame(&scope, "all")
                .unwrap()
                .unwrap()
                .records
                .len(),
            2
        );
        assert!(!cache.commit_sync(&scope, "all", 2, &[], true).unwrap());
        cache
            .commit_sync(&scope, "all", 4, std::slice::from_ref(&a), true)
            .unwrap();
        assert_eq!(
            cache.sync_frame(&scope, "all").unwrap().unwrap().records,
            vec![a]
        );
        assert_eq!(cache.get(&scope, "pr", "org/b#1").unwrap(), Some(b));
        assert!(cache.get(&scope, "pr", "unmanaged").unwrap().is_some());
        cache.commit_sync(&scope, "saved-b", 2, &[], true).unwrap();
        assert!(cache.get(&scope, "pr", "org/b#1").unwrap().is_none());
        cache.revoke_repository(&scope, "org/a").unwrap();
        let frame = cache.sync_frame(&scope, "all").unwrap().unwrap();
        assert!(!frame.complete);
        assert!(frame.records.is_empty());
    }

    #[test]
    fn cache_storage_interrupted_generation_rolls_back_every_record_and_checkpoint() {
        let mut cache = Cache::memory().unwrap();
        let scope = Scope::new("github.com", "alice").unwrap();
        let original = Record::new("pr", "one", "org/a", json!({"title":"Original"}));
        cache
            .commit_sync(&scope, "inbox", 1, std::slice::from_ref(&original), true)
            .unwrap();
        cache.connection.execute_batch("CREATE TRIGGER interrupted BEFORE INSERT ON records WHEN NEW.id='fail' BEGIN SELECT RAISE(ABORT,'interrupted write'); END;").unwrap();
        let changed = Record::new("pr", "one", "org/a", json!({"title":"Changed"}));
        let failure = Record::new("pr", "fail", "org/a", json!({}));
        assert!(cache
            .commit_sync(&scope, "inbox", 2, &[changed, failure], true)
            .is_err());
        let frame = cache.sync_frame(&scope, "inbox").unwrap().unwrap();
        assert_eq!(frame.generation, 1);
        assert_eq!(frame.records, vec![original]);
    }

    #[test]
    fn cache_storage_v1_migration_and_clear_preserve_personal_work() {
        let cache = Cache::memory().unwrap();
        let scope = Scope::new("github.com", "alice").unwrap();
        cache.put_draft(&scope, "review", "Keep this").unwrap();
        cache
            .save_view(&scope, "Mine", &json!({"query":"auth"}))
            .unwrap();
        cache
            .connection
            .execute_batch(
                "DROP TABLE checkpoint_records; DROP TABLE checkpoints; PRAGMA user_version=1;",
            )
            .unwrap();
        let mut migrated = Cache::initialize(cache.connection).unwrap();
        migrated
            .commit_sync(
                &scope,
                "inbox",
                1,
                &[Record::new("pr", "one", "org/a", json!({}))],
                true,
            )
            .unwrap();
        migrated.clear_records(&scope).unwrap();
        assert!(migrated.sync_frame(&scope, "inbox").unwrap().is_none());
        assert_eq!(
            migrated.draft(&scope, "review").unwrap().as_deref(),
            Some("Keep this")
        );
        assert_eq!(migrated.saved_views(&scope).unwrap().len(), 1);
    }
    #[test]
    fn cache_storage_immutable_principals_never_unlock_reassigned_or_legacy_logins() {
        let cache = Cache::memory().unwrap();
        let original = Scope::github("github.com", Some(42)).unwrap();
        let renamed_same_user = Scope::github("github.com", Some(42)).unwrap();
        let reassigned_login = Scope::github("github.com", Some(43)).unwrap();
        let legacy = Scope::new("github.com", "alice").unwrap();
        cache
            .put(
                &original,
                &Record::new(
                    "private",
                    "secret",
                    "org/private",
                    serde_json::json!({"body":"private record"}),
                ),
            )
            .unwrap();
        cache
            .put_draft(&original, "private", "private draft")
            .unwrap();
        assert_eq!(original, renamed_same_user);
        assert!(cache
            .list(&reassigned_login, Some("private"))
            .unwrap()
            .is_empty());
        assert!(cache.draft(&reassigned_login, "private").unwrap().is_none());
        assert!(cache.list(&legacy, Some("private")).unwrap().is_empty());
        assert!(Scope::github("github.com", None).is_err());
        assert!(Scope::github("github.com", Some(0)).is_err());
        assert_eq!(
            cache
                .draft(&renamed_same_user, "private")
                .unwrap()
                .as_deref(),
            Some("private draft")
        );
    }
}
