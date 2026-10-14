//! Worker metadata lookup. No discussion documents or source frames read.
use super::*;
use rusqlite::{params_from_iter, types::Value as SqlValue, Transaction};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

const STATE_KIND: &str = "collaboration-state-v1";
const STATE_BYTES: u64 = 128 * 1024;
const TOTAL_STATE_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalBindingStateMetadata {
    /// Canonical state record only, never document/body payloads. Worker decodes.
    /// None means missing or over the explicit per-record/aggregate byte budget.
    pub payload: Option<String>,
    pub payload_sha256: Option<String>,
    pub payload_bytes: Option<u64>,
    pub record_repository: Option<String>,
    pub record_complete: Option<bool>,
    pub fetched_at: Option<u64>,
    pub member: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceBindingMetadata {
    pub source: String,
    pub status: SyncBindingStatus,
    pub checkpoint_generation: Option<u64>,
    pub source_generation: Option<u64>,
    pub source_active: Option<bool>,
    pub state: CanonicalBindingStateMetadata,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncMutationTombstone {
    pub namespace: SyncNamespace,
    pub repository: String,
    pub resource: String,
    pub kind: SyncBindingKind,
    pub mutation_ordinal: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct Request {
    scope: Scope,
    repository: String,
    nodes: Option<Vec<String>>,
    kinds: Vec<SyncBindingKind>,
    limit: usize,
}
/// Opaque issuer/request/keyspace proof. A partial page cannot prove absence.
#[derive(Debug)]
pub struct ResourceBindingSnapshot {
    issuer: PathBuf,
    request: Request,
    epoch: u64,
    rows: Vec<ResourceBindingMetadata>,
    tombstones: Vec<SyncMutationTombstone>,
    total_bindings: u64,
    total_tombstones: u64,
    visible_none_count: u64,
}
impl ResourceBindingSnapshot {
    pub fn rows(&self) -> &[ResourceBindingMetadata] {
        &self.rows
    }
    pub fn tombstones(&self) -> &[SyncMutationTombstone] {
        &self.tombstones
    }
    pub fn total_bindings(&self) -> u64 {
        self.total_bindings
    }
    pub fn total_tombstones(&self) -> u64 {
        self.total_tombstones
    }
    pub fn visible_none_count(&self) -> u64 {
        self.visible_none_count
    }
    pub fn complete(&self) -> bool {
        self.rows.len() as u64 == self.total_bindings
            && self.tombstones.len() as u64 == self.total_tombstones
    }
    /// Repeats the same scoped metadata read and checks newly matching keys as
    /// well as returned row epochs, canonical state bytes and issuer identity.
    pub fn validate(&self, cache: &Cache) -> Result<()> {
        let current = capture(cache, self.request.clone())?;
        if current.issuer != self.issuer
            || current.epoch != self.epoch
            || current.rows != self.rows
            || current.tombstones != self.tombstones
            || current.total_bindings != self.total_bindings
            || current.total_tombstones != self.total_tombstones
            || current.visible_none_count != self.visible_none_count
        {
            bail!("Canonical binding visibility metadata changed; repeat the scoped lookup before displaying fragments");
        }
        Ok(())
    }
}
impl Cache {
    pub(crate) fn binding_lookup_issuer(&self) -> Result<PathBuf> {
        Ok(std::fs::canonicalize(self.connection.path().context(
            "Binding proof requires its file-backed cache issuer",
        )?)?)
    }
    pub fn lookup_resource_bindings(
        &self,
        scope: &Scope,
        repository: &str,
        nodes: Option<&[String]>,
        kinds: &[SyncBindingKind],
        max_rows: usize,
    ) -> Result<ResourceBindingSnapshot> {
        positive_scope(scope)?;
        crate::config::validate_repository(repository)?;
        if repository.len() > 4096
            || !(1..=5000).contains(&max_rows)
            || kinds.is_empty()
            || kinds.len() > 5
            || kinds.iter().collect::<BTreeSet<_>>().len() != kinds.len()
        {
            bail!("Binding lookup requires bounded canonical repository/categories and row budget");
        }
        if let Some(nodes) = nodes {
            if nodes.is_empty() || nodes.len() > 1000 {
                bail!("Binding lookup requires 1..1000 actual opaque parent nodes, or explicit repo-only provenance");
            }
            for node in nodes {
                validate_resource(node)?;
            }
        }
        let mut nodes = nodes.map(<[String]>::to_vec);
        if let Some(nodes) = &mut nodes {
            nodes.sort();
            nodes.dedup();
        }
        let mut kinds = kinds.to_vec();
        kinds.sort();
        capture(
            self,
            Request {
                scope: scope.clone(),
                repository: repository.to_ascii_lowercase(),
                nodes,
                kinds,
                limit: max_rows,
            },
        )
    }
}

fn predicate(request: &Request, alias: &str) -> (String, Vec<SqlValue>) {
    let mut values = vec![
        SqlValue::Text(request.scope.host.clone()),
        SqlValue::Text(request.scope.login.clone()),
        SqlValue::Text(request.repository.clone()),
    ];
    let mut sql = format!("{alias}.host=?1 AND {alias}.login=?2 AND {alias}.namespace='collaboration' AND {alias}.repository=?3 AND {alias}.kind IN(");
    for (index, value) in request.kinds.iter().enumerate() {
        if index > 0 {
            sql.push(',');
        }
        values.push(SqlValue::Text(kind(*value).into()));
        sql.push_str(&format!("?{}", values.len()));
    }
    sql.push(')');
    if let Some(nodes) = &request.nodes {
        sql.push_str(&format!(" AND {alias}.resource IN("));
        for (index, node) in nodes.iter().enumerate() {
            if index > 0 {
                sql.push(',');
            }
            values.push(SqlValue::Text(node.clone()));
            sql.push_str(&format!("?{}", values.len()));
        }
        sql.push(')');
    }
    (sql, values)
}
fn parse_kind(value: &str) -> Result<SyncBindingKind> {
    Ok(match value {
        "pull_request_body" => SyncBindingKind::PullRequestBody,
        "issue_body" => SyncBindingKind::IssueBody,
        "conversation_comment" => SyncBindingKind::ConversationComment,
        "review" => SyncBindingKind::Review,
        "inline_comment" => SyncBindingKind::InlineComment,
        _ => bail!("Unknown canonical binding category"),
    })
}
fn capture(cache: &Cache, request: Request) -> Result<ResourceBindingSnapshot> {
    let issuer = cache.binding_lookup_issuer()?;
    let transaction = cache.connection.unchecked_transaction()?;
    let epoch = transaction.query_row("SELECT generation FROM sync_binding_lookup_epochs WHERE host=?1 AND login=?2 AND namespace='collaboration' AND repository=?3",params![request.scope.host,request.scope.login,request.repository],|row|row.get::<_,u64>(0)).optional()?.unwrap_or(0);
    let (binding_predicate, binding_values) = predicate(&request, "b");
    let (total_bindings,visible_none_count):(u64,u64) = transaction.query_row(&format!("SELECT COUNT(*),COALESCE(SUM(b.visible_generation IS NULL),0) FROM sync_resource_bindings b WHERE {binding_predicate}"),params_from_iter(&binding_values),|row|Ok((row.get(0)?,row.get(1)?)))?;
    let rows = binding_rows(&transaction, &request, &binding_predicate, binding_values)?;
    let (fence_predicate, mut fence_values) = predicate(&request, "f");
    let total_tombstones: u64 = transaction.query_row(
        &format!("SELECT COUNT(*) FROM sync_mutation_fences f WHERE {fence_predicate}"),
        params_from_iter(&fence_values),
        |row| row.get(0),
    )?;
    let remaining = request.limit.saturating_sub(rows.len());
    fence_values.push(SqlValue::Integer(remaining as i64));
    let tombstones = {
        let mut statement = transaction.prepare(&format!("SELECT f.repository,f.resource,f.kind,f.mutation_ordinal,length(CAST(f.repository AS BLOB))+length(CAST(f.resource AS BLOB))+length(CAST(f.kind AS BLOB)) FROM sync_mutation_fences f WHERE {fence_predicate} ORDER BY f.resource,f.kind LIMIT ?{}",fence_values.len()))?;
        let mut selected = statement.query(params_from_iter(&fence_values))?;
        let mut result = Vec::new();
        while let Some(row) = selected.next()? {
            let bytes: u64 = row.get(4)?;
            if bytes > 8192 {
                bail!("Mutation tombstone metadata exceeds its byte budget");
            }
            let value = SyncMutationTombstone {
                namespace: SyncNamespace::Collaboration,
                repository: row.get(0)?,
                resource: row.get(1)?,
                kind: parse_kind(&row.get::<_, String>(2)?)?,
                mutation_ordinal: row.get(3)?,
            };
            validate_resource(&value.resource)?;
            if value.mutation_ordinal == 0 || value.mutation_ordinal > i64::MAX as u64 {
                bail!("Invalid mutation tombstone ordinal");
            }
            result.push(value);
        }
        result
    };
    transaction.commit()?;
    Ok(ResourceBindingSnapshot {
        issuer,
        request,
        epoch,
        rows,
        tombstones,
        total_bindings,
        total_tombstones,
        visible_none_count,
    })
}

fn binding_rows(
    transaction: &Transaction<'_>,
    request: &Request,
    predicate: &str,
    mut values: Vec<SqlValue>,
) -> Result<Vec<ResourceBindingMetadata>> {
    values.push(SqlValue::Integer(request.limit as i64));
    let mut statement = transaction.prepare(&format!("SELECT b.source,b.repository_id,b.repository,b.resource,b.kind,b.reservation_generation,b.visible_generation,b.active,c.generation,s.generation,s.active,length(CAST(b.source AS BLOB))+length(CAST(b.repository AS BLOB))+length(CAST(b.resource AS BLOB))+length(CAST(b.kind AS BLOB)),length(CAST(m.payload AS BLOB)),m.repository,m.complete,m.fetched_at,EXISTS(SELECT 1 FROM checkpoint_records cr WHERE cr.host=b.host AND cr.login=b.login AND cr.source=b.source AND cr.kind='collaboration-state-v1' AND cr.id=b.source) FROM sync_resource_bindings b LEFT JOIN checkpoints c ON c.host=b.host AND c.login=b.login AND c.source=b.source LEFT JOIN sync_reservations s ON s.host=b.host AND s.login=b.login AND s.source=b.source LEFT JOIN records m ON m.host=b.host AND m.login=b.login AND m.kind='collaboration-state-v1' AND m.id=b.source WHERE {predicate} ORDER BY b.source LIMIT ?{}",values.len()))?;
    let mut selected = statement.query(params_from_iter(&values))?;
    let mut rows = Vec::new();
    let mut metadata_bytes = 0u64;
    while let Some(row) = selected.next()? {
        if row.get::<_, u64>(11)? > 8192 {
            bail!("Canonical binding metadata exceeds its byte budget");
        }
        let source: String = row.get(0)?;
        validate_bound_source(&source)?;
        let binding = SyncBinding {
            namespace: SyncNamespace::Collaboration,
            repository_id: row.get(1)?,
            repository: row.get(2)?,
            resource: row.get(3)?,
            kind: parse_kind(&row.get::<_, String>(4)?)?,
        };
        validate_binding(&binding)?;
        let payload_bytes: Option<u64> = row.get(12)?;
        let repository_bytes:Option<u64> = transaction.query_row("SELECT length(CAST(repository AS BLOB)) FROM records WHERE host=?1 AND login=?2 AND kind=?3 AND id=?4",params![request.scope.host,request.scope.login,STATE_KIND,source],|row|row.get(0)).optional()?;
        if repository_bytes.is_some_and(|bytes| bytes > 4096) {
            bail!("Canonical state repository metadata exceeds its byte budget");
        }
        let payload = if let Some(bytes) = payload_bytes.filter(|bytes| {
            *bytes <= STATE_BYTES && metadata_bytes.saturating_add(*bytes) <= TOTAL_STATE_BYTES
        }) {
            metadata_bytes += bytes;
            Some(transaction.query_row(
                "SELECT payload FROM records WHERE host=?1 AND login=?2 AND kind=?3 AND id=?4",
                params![request.scope.host, request.scope.login, STATE_KIND, source],
                |row| row.get::<_, String>(0),
            )?)
        } else {
            None
        };
        let payload_sha256 = payload
            .as_ref()
            .map(|payload| format!("{:x}", Sha256::digest(payload.as_bytes())));
        rows.push(ResourceBindingMetadata {
            source,
            status: SyncBindingStatus {
                binding,
                reservation_generation: row.get(5)?,
                visible_generation: row.get(6)?,
                active: row.get(7)?,
            },
            checkpoint_generation: row.get(8)?,
            source_generation: row.get(9)?,
            source_active: row.get(10)?,
            state: CanonicalBindingStateMetadata {
                payload,
                payload_sha256,
                payload_bytes,
                record_repository: row.get(13)?,
                record_complete: row.get(14)?,
                fetched_at: row.get(15)?,
                member: row.get(16)?,
            },
        });
    }
    Ok(rows)
}

pub(super) fn initialize_lookup(transaction: &Transaction<'_>) -> Result<()> {
    transaction.execute_batch("CREATE INDEX IF NOT EXISTS sync_resource_repo_kind_visibility ON sync_resource_bindings(host,login,namespace,repository,kind,visible_generation);
      CREATE INDEX IF NOT EXISTS sync_mutation_repo_kind ON sync_mutation_fences(host,login,namespace,repository,kind);
      CREATE TABLE IF NOT EXISTS sync_binding_lookup_epochs(host TEXT NOT NULL,login TEXT NOT NULL,namespace TEXT NOT NULL,repository TEXT NOT NULL,generation INTEGER NOT NULL CHECK(generation>=0 AND typeof(generation)='integer'),PRIMARY KEY(host,login,namespace,repository));")?;
    for table in ["sync_resource_bindings", "sync_mutation_fences"] {
        for (event, reference) in [("INSERT", "NEW"), ("DELETE", "OLD")] {
            transaction.execute_batch(&format!("CREATE TRIGGER IF NOT EXISTS binding_lookup_{table}_{event} AFTER {event} ON {table} BEGIN INSERT INTO sync_binding_lookup_epochs VALUES({reference}.host,{reference}.login,{reference}.namespace,{reference}.repository,1) ON CONFLICT(host,login,namespace,repository) DO UPDATE SET generation=generation+1; END;"))?;
        }
        transaction.execute_batch(&format!("CREATE TRIGGER IF NOT EXISTS binding_lookup_{table}_UPDATE AFTER UPDATE ON {table} BEGIN INSERT INTO sync_binding_lookup_epochs VALUES(OLD.host,OLD.login,OLD.namespace,OLD.repository,1) ON CONFLICT(host,login,namespace,repository) DO UPDATE SET generation=generation+1; INSERT INTO sync_binding_lookup_epochs VALUES(NEW.host,NEW.login,NEW.namespace,NEW.repository,1) ON CONFLICT(host,login,namespace,repository) DO UPDATE SET generation=generation+1 WHERE OLD.host!=NEW.host OR OLD.login!=NEW.login OR OLD.namespace!=NEW.namespace OR OLD.repository!=NEW.repository; END;"))?;
    }
    for table in [
        "checkpoints",
        "sync_reservations",
        "records",
        "checkpoint_records",
    ] {
        for (event, reference) in [
            ("INSERT", "NEW"),
            ("DELETE", "OLD"),
            ("UPDATE", "OLD"),
            ("UPDATE", "NEW"),
        ] {
            let (condition, source) = if table == "records" || table == "checkpoint_records" {
                (
                    format!(" WHEN {reference}.kind='collaboration-state-v1'"),
                    format!("{reference}.id"),
                )
            } else {
                (String::new(), format!("{reference}.source"))
            };
            transaction.execute_batch(&format!("CREATE TRIGGER IF NOT EXISTS binding_lookup_{table}_{event}_{reference} AFTER {event} ON {table}{condition} BEGIN INSERT INTO sync_binding_lookup_epochs SELECT b.host,b.login,b.namespace,b.repository,1 FROM sync_resource_bindings b WHERE b.host={reference}.host AND b.login={reference}.login AND b.source={source} ON CONFLICT(host,login,namespace,repository) DO UPDATE SET generation=generation+1; END;"))?;
        }
    }
    Ok(())
}
