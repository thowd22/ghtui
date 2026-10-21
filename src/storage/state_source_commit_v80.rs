//! SAME closed State-source raw/Clear CAS -> scope-wide fence -> exact new
//! source generation, encoded rows and watermarks. No generic repair/hydrate
//! API, no source from saved IDs, and no payload materialization after UID.
use super::*;
use crate::{
    github_workspace::{PrKey, PreparedStateWorkspaceV80},
    providers::gh::{
        ClosedStateSourceV80, StateSourceChargeV80, StateSourcePreparedBindingV80,
        StateSourceReadV80,
    },
    state_inbox_sync_v76::{StateSourceOriginV80, StateSourceSyncV76},
};
use repository_catalogue_intents_v1::state_source_v80::PreparedCatalogueStateSourceV80;
use sha2::{Digest, Sha256};
use std::{io::Write, mem::size_of, sync::Arc, time::Instant};
const CAP: usize = 8 * 1024 * 1024;
struct CountV80 {
    bytes: usize,
}
impl Write for CountV80 {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(bytes.len())
            .filter(|n| *n <= CAP)
            .ok_or_else(|| std::io::Error::other("State source encoding cap"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn counted(value: &impl serde::Serialize) -> Result<usize> {
    let mut c = CountV80 { bytes: 0 };
    serde_json::to_writer(&mut c, value)?;
    Ok(c.bytes)
}
struct WatermarkV80 {
    node: String,
    repository: u64,
    pr: u64,
    ordinal: i64,
    origin: String,
    disposition: Option<String>,
}
pub(crate) struct PreparedStateSourceCommitV80 {
    cache: Cache,
    original: Arc<StateSourceSyncV76>,
    binding: StateSourcePreparedBindingV80,
    source: String,
    generation: u64,
    rows: EncodedSourceV19,
    old_rows: [u8; 32],
    watermarks: Vec<WatermarkV80>,
    catalogue: Option<PreparedCatalogueStateSourceV80>,
    workspace: PreparedStateWorkspaceV80,
    context: String,
    result: CommittedStateSourceV80,
    // All material above drops before the retained allocation debit.
    _charge: StateSourceChargeV80,
}
struct CommittedDataV80 {
    closed: ClosedStateSourceV80,
    catalogue: Option<Arc<CatalogueIntentSource>>,
    context: String,
}
pub(crate) struct CommittedStateSourceV80 {
    // Allocated before final UID. None cannot escape as a successful result.
    data: Box<Option<CommittedDataV80>>,
}
impl CommittedStateSourceV80 {
    pub(crate) fn matches_original(&self, original: &Arc<StateSourceSyncV76>) -> bool {
        self.data
            .as_ref()
            .as_ref()
            .is_some_and(|d| Arc::ptr_eq(d.closed.original(), original))
            && !original.original().cancellation().is_cancelled()
            && Instant::now() < original.original().deadline()
    }
    pub(crate) fn into_parts(
        mut self,
    ) -> Result<(
        crate::github_workspace::GithubWorkspace,
        Option<Arc<CatalogueIntentSource>>,
        String,
    )> {
        let data = self
            .data
            .take()
            .context("State source result was not atomically committed")?;
        Ok((data.closed.into_workspace(), data.catalogue, data.context))
    }
}
impl PreparedStateSourceCommitV80 {
    pub(crate) fn prepare(read: &StateSourceReadV80) -> Result<Self> {
        read.original()
            .validate_original()
            .map_err(anyhow::Error::msg)?;
        let original = read.original().original();
        let workspace = read.workspace();
        if workspace.scope()? != *original.scope()
            || !workspace.inbox.coverage.complete
            || workspace.pr.is_some()
            || workspace.scoped_pr_navigation.is_some()
            || workspace.scoped_pr_resource.is_some()
            || workspace.personal_pr_page_ticket_v13.is_some()
            || workspace.notification_pr_page_ticket_v63.is_some()
            || workspace.repositories != original.repositories()
            || &workspace.exclusions != original.exclusions()
        {
            bail!("Actual unclosed State source differs from its exact captured family");
        }
        let source = original.source();
        // Count serialization and all actually retained metadata BEFORE any
        // Value/Record/encoded String/key/vector/source-descriptor clone.
        let coverage = counted(&workspace.inbox.coverage)?;
        let encoded = workspace.inbox.pull_requests.iter().try_fold(
            coverage + 16,
            |n, pr| -> Result<usize> {
                n.checked_add(counted(pr)?)
                    .context("State payload count overflow")
            },
        )?;
        let metadata = workspace
            .inbox
            .pull_requests
            .iter()
            .try_fold(0usize, |n, pr| {
                n.checked_add(pr.repository.capacity().checked_mul(4)?)?
                    .checked_add(pr.host.capacity())?
                    .checked_add(pr.updated_at.capacity())?
                    .checked_add(2048)
            })
            .context("State record metadata count overflow")?;
        let catalogue_bytes = match original.origin() {
            StateSourceOriginV80::Catalogue { source, .. } => source
                .workflow_owned_bytes_v9()
                .context("State catalogue descriptor layout overflow")?,
            _ => 0,
        };
        let bound = encoded
            .checked_mul(4)
            .and_then(|n| n.checked_add(metadata))
            .and_then(|n| n.checked_add(catalogue_bytes))
            .and_then(|n| {
                n.checked_add(
                    (workspace.inbox.pull_requests.len() + 1)
                        .checked_mul(4 * size_of::<EncodedRecordV19>())?,
                )
            })
            .and_then(|n| n.checked_add(1024 * 1024))
            .and_then(|n| {
                n.checked_add(size_of::<Option<CommittedDataV80>>() + 2 * size_of::<usize>())
            })
            .filter(|n| *n <= CAP)
            .context("State prepared graph exceeds8MiB before encoding")?;
        let charge = read
            .precharge_storage(bound)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let binding = read
            .prepared_binding()
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        // Existing private file only. No prepare()/migration/chmod/create or
        // reservation write happens before the final authenticated close.
        let connection = Connection::open_with_flags(
            original.database(),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        let cache = Cache { connection };
        let tx = cache.connection.unchecked_transaction()?;
        read.original()
            .validate_prepared_transaction_v80(&tx)
            .map_err(anyhow::Error::msg)?;
        let generation = next_generation(&tx, original.scope(), source)?;
        let watermarks = prepare_watermarks(&tx, original.database(), original.scope())?;
        let old_rows = rows_digest(&tx, original.scope(), source)?;
        let (old_count,old_bytes):(usize,usize)=tx.query_row("SELECT count(*),COALESCE(sum(length(CAST(kind AS BLOB))+length(CAST(id AS BLOB))),0) FROM checkpoint_records WHERE host=?1 AND login=?2 AND source=?3",params![original.scope().host,original.scope().login,source],|r|Ok((r.get(0)?,r.get(1)?)))?;
        if old_count > 6001 || old_bytes > 512 * 1024 {
            bail!("State old membership exceeds bounded pre-copy profile");
        }
        let mut statement=tx.prepare("SELECT kind,id FROM checkpoint_records WHERE host=?1 AND login=?2 AND source=?3 ORDER BY kind,id")?;
        let previous_members = statement
            .query_map(
                params![original.scope().host, original.scope().login, source],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?
            .collect::<rusqlite::Result<Vec<(String, String)>>>()?;
        drop(statement);
        let fetched_at = now();
        let mut records = Vec::with_capacity(workspace.inbox.pull_requests.len() + 1);
        records.push(EncodedRecordV19 {
            kind: "inbox-meta".into(),
            id: source.into(),
            repository: String::new(),
            payload: format!(
                "{{\"coverage\":{}}}",
                serde_json::to_string(&workspace.inbox.coverage)?
            ),
            fetched_at,
            source_updated_at: None,
            complete: true,
        });
        for pr in &workspace.inbox.pull_requests {
            records.push(EncodedRecordV19 {
                kind: "pr".into(),
                id: PrKey::from(pr).label(),
                repository: pr.repository.clone(),
                payload: serde_json::to_string(pr)?,
                fetched_at,
                source_updated_at: Some(pr.updated_at.clone()),
                complete: true,
            });
        }
        read.original()
            .validate_prepared_transaction_v80(&tx)
            .map_err(anyhow::Error::msg)?;
        tx.commit()?;
        let catalogue = PreparedCatalogueStateSourceV80::prepare(&cache, read)?;
        let workspace = PreparedStateWorkspaceV80::prepare(read, generation, fetched_at)?;
        let context = read.workspace().context();
        read.original()
            .validate_original()
            .map_err(anyhow::Error::msg)?;
        Ok(Self {
            cache,
            original: Arc::clone(read.original()),
            binding,
            source: source.into(),
            generation,
            rows: EncodedSourceV19 {
                records,
                previous_members,
                published_at: fetched_at,
            },
            old_rows,
            watermarks,
            catalogue,
            workspace,
            context,
            result: CommittedStateSourceV80 {
                data: Box::new(None),
            },
            _charge: charge,
        })
    }
    pub(crate) fn commit(
        mut self,
        mut closed: ClosedStateSourceV80,
    ) -> Result<CommittedStateSourceV80> {
        self.binding
            .validate(&closed)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        if !Arc::ptr_eq(&self.original, closed.original()) {
            bail!("Foreign State source closed result");
        }
        let deadline = self.binding.deadline().min(closed.deadline());
        let _busy = crate::storage::busy_v1::until(&self.cache.connection, deadline)?;
        let tx = self
            .cache
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        self.binding
            .validate(&closed)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        self.original
            .validate_prepared_transaction_v80(&tx)
            .map_err(anyhow::Error::msg)?;
        let scope = self.original.original().scope();
        if rows_digest(&tx, scope, &self.source)? != self.old_rows
            || next_generation(&tx, scope, &self.source)? != self.generation
        {
            bail!("State prepared source predecessor changed");
        }
        for watermark in &self.watermarks {
            validate_watermark(&tx, scope, watermark)?;
        }
        if let Some(catalogue) = &self.catalogue {
            catalogue.validate_before_fence(&tx)?;
        }
        // LINEARIZATION: every raw/source/file/Clear witness checked under
        // THIS Immediate lock, then unchanged scope-wide fencing and exact
        // new-source publication. No other source/reservation is exempted.
        Cache::fence_reservations(&tx, scope)?;
        bindings::fence_binding_visibility(&tx, scope, None)?;
        if let Some(catalogue) = &self.catalogue {
            catalogue.apply_after_fence(&tx)?;
        }
        let actual = Cache::reserve_sync_generation_row_v20(&tx, scope, &self.source)?;
        if actual != self.generation {
            bail!("State source next generation differs from prepared fence result");
        }
        if !Cache::publish_sync_rows_v19(
            &tx,
            scope,
            &self.source,
            self.generation,
            SyncRowsV19::Prepared(&self.rows),
            SyncPolicy {
                complete: true,
                prune_missing: true,
                reservation_authoritative: true,
            },
        )? {
            bail!("State exact source was not published");
        }
        bindings::mark_binding_published(&tx, scope, &self.source, self.generation)?;
        for watermark in &self.watermarks {
            tx.execute("INSERT INTO pr_write_state_source_watermarks_v73 VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(host,principal,pr_node,source) DO UPDATE SET source_generation=excluded.source_generation,mutation_ordinal=excluded.mutation_ordinal,repository_id=excluded.repository_id,pr_id=excluded.pr_id",params![scope.host,scope.login,watermark.node,self.source,self.generation,watermark.ordinal,watermark.repository,watermark.pr])?;
        }
        // Positive source-bound watermarks, never a global status permission.
        super::state_filter_v68::guard_source(&tx, scope, &self.source)?;
        if self.original.original().cancellation().is_cancelled() || Instant::now() >= deadline {
            bail!("State source expired under final publication lock");
        }
        tx.commit()?;
        closed
            .install_prepared_workspace(self.workspace)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        *self.result.data = Some(CommittedDataV80 {
            closed,
            catalogue: self
                .catalogue
                .map(PreparedCatalogueStateSourceV80::committed),
            context: self.context,
        });
        Ok(self.result)
    }
}
fn next_generation(tx: &Connection, scope: &Scope, source: &str) -> Result<u64> {
    let (r,p):(Option<u64>,Option<u64>)=tx.query_row("SELECT (SELECT generation FROM sync_reservations WHERE host=?1 AND login=?2 AND source=?3),(SELECT generation FROM checkpoints WHERE host=?1 AND login=?2 AND source=?3)",params![scope.host,scope.login,source],|r|Ok((r.get(0)?,r.get(1)?)))?;
    r.unwrap_or(0)
        .checked_add(u64::from(r.is_some()))
        .map(|n| n.max(p.unwrap_or(0)))
        .and_then(|n| n.checked_add(1))
        .filter(|n| *n <= i64::MAX as u64)
        .context("State source generation exhausted")
}
fn prepare_watermarks(tx: &Connection, path: &Path, scope: &Scope) -> Result<Vec<WatermarkV80>> {
    super::state_filter_v68::validate_startup_rows_v80(tx, scope)?;
    let mut statement=tx.prepare("SELECT pr_node,repository_id,pr_id,attempt_id,raw_origin_sha256,raw_disposition_sha256 FROM pr_write_state_filter_fences_v68 WHERE host=?1 AND principal=?2 ORDER BY pr_node")?;
    let mut rows = statement.query(params![scope.host, scope.login])?;
    let mut result = Vec::with_capacity(32);
    while let Some(row) = rows.next()? {
        let node = row.get_ref(0)?.as_str()?;
        let ordinal: i64 = row.get(3)?;
        let profile=crate::pr_write_journal::state_origin_v68::source_terminal_v80::validate_source_terminal_v80(tx,path,scope,node,ordinal)?;
        let disposition = match row.get_ref(5)? {
            rusqlite::types::ValueRef::Null => None,
            rusqlite::types::ValueRef::Text(v) => Some(std::str::from_utf8(v)?),
            _ => bail!("State source disposition raw type changed"),
        };
        match profile {
            crate::pr_write_journal::state_origin_v68::source_terminal_v80::StateSourceTerminalV80::ConfirmedEffect if disposition.is_some()=>{},
            crate::pr_write_journal::state_origin_v68::source_terminal_v80::StateSourceTerminalV80::AcknowledgedNoEffect if disposition.is_none()=>{},
            _=>bail!("State source terminal/disposition profile differs"),
        }
        result.push(WatermarkV80 {
            node: node.into(),
            repository: row.get(1)?,
            pr: row.get(2)?,
            ordinal,
            origin: row.get_ref(4)?.as_str()?.into(),
            disposition: disposition.map(str::to_owned),
        });
    }
    if result.is_empty() || result.len() > 32 {
        bail!("State complete source must cover1..32 exact affected keys");
    }
    Ok(result)
}
fn validate_watermark(tx: &Connection, scope: &Scope, w: &WatermarkV80) -> Result<()> {
    let exact:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM pr_write_state_filter_fences_v68 WHERE host=?1 AND principal=?2 AND pr_node=?3 AND typeof(repository_id)='integer' AND repository_id=?4 AND typeof(pr_id)='integer' AND pr_id=?5 AND typeof(attempt_id)='integer' AND attempt_id=?6 AND typeof(raw_origin_sha256)='text' AND CAST(raw_origin_sha256 AS BLOB)=CAST(?7 AS BLOB) AND typeof(raw_disposition_sha256)=typeof(?8) AND CAST(raw_disposition_sha256 AS BLOB) IS CAST(?8 AS BLOB))",params![scope.host,scope.login,w.node,w.repository,w.pr,w.ordinal,w.origin,w.disposition],|r|r.get(0))?;
    if !exact {
        bail!("State source raw watermark predecessor changed");
    }
    Ok(())
}
fn rows_digest(tx: &Connection, scope: &Scope, source: &str) -> Result<[u8; 32]> {
    let (count,bytes):(usize,usize)=tx.query_row("SELECT count(*),COALESCE(sum(length(CAST(m.kind AS BLOB))+length(CAST(m.id AS BLOB))+COALESCE(length(CAST(r.payload AS BLOB)),0)),0) FROM checkpoint_records m LEFT JOIN records r ON r.host=m.host AND r.login=m.login AND r.kind=m.kind AND r.id=m.id WHERE m.host=?1 AND m.login=?2 AND m.source=?3",params![scope.host,scope.login,source],|r|Ok((r.get(0)?,r.get(1)?)))?;
    if count > 6001 || bytes > CAP {
        bail!("State source old raw rows exceed bounded scan");
    }
    let mut h = Sha256::new();
    h.update((count as u64).to_le_bytes());
    let mut statement=tx.prepare("SELECT m.kind,m.id,r.repository,r.payload,r.fetched_at,r.source_updated_at,r.complete FROM checkpoint_records m LEFT JOIN records r ON r.host=m.host AND r.login=m.login AND r.kind=m.kind AND r.id=m.id WHERE m.host=?1 AND m.login=?2 AND m.source=?3 ORDER BY m.kind,m.id")?;
    let mut rows = statement.query(params![scope.host, scope.login, source])?;
    while let Some(row) = rows.next()? {
        for col in 0..7 {
            match row.get_ref(col)? {
                rusqlite::types::ValueRef::Null => h.update([0]),
                rusqlite::types::ValueRef::Integer(n) => {
                    h.update([1]);
                    h.update(n.to_le_bytes());
                }
                rusqlite::types::ValueRef::Text(v) => {
                    h.update([2]);
                    h.update((v.len() as u64).to_le_bytes());
                    h.update(v);
                }
                _ => bail!("State source old row raw type changed"),
            }
        }
    }
    Ok(h.finalize().into())
}
