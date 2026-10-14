//! Resource-bound read reservations. Metadata only: this table never stores bodies.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[path = "bindings_lookup.rs"]
mod lookup;
pub use lookup::{
    CanonicalBindingStateMetadata, ResourceBindingMetadata, ResourceBindingSnapshot,
    SyncMutationTombstone,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncNamespace {
    Collaboration,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncBindingKind {
    PullRequestBody,
    IssueBody,
    ConversationComment,
    Review,
    InlineComment,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncBinding {
    pub namespace: SyncNamespace,
    pub repository_id: u64,
    pub repository: String,
    pub resource: String,
    pub kind: SyncBindingKind,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncBindingStatus {
    pub binding: SyncBinding,
    pub reservation_generation: u64,
    /// None after invalidation, including while a replacement fetch is pending.
    pub visible_generation: Option<u64>,
    pub active: bool,
}
#[derive(Debug, Clone)]
pub struct SyncFence {
    pub namespace: SyncNamespace,
    pub repository_id: Option<u64>,
    pub repository: String,
    pub resource: String,
    pub kinds: Vec<SyncBindingKind>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncFenceResult {
    pub sources: Vec<String>,
    /// Includes derived rank reads; their previously collected bodies cannot publish late.
    pub active_reservations_fenced: usize,
}

pub(super) fn initialize_bindings(transaction: &rusqlite::Transaction<'_>) -> Result<()> {
    transaction.execute_batch("CREATE TABLE IF NOT EXISTS sync_resource_bindings(
      host TEXT NOT NULL,login TEXT NOT NULL,source TEXT NOT NULL,namespace TEXT NOT NULL,
      repository_id INTEGER NOT NULL CHECK(repository_id>0),repository TEXT NOT NULL,
      resource TEXT NOT NULL,kind TEXT NOT NULL,reservation_generation INTEGER NOT NULL CHECK(reservation_generation>0),
      visible_generation INTEGER CHECK(visible_generation IS NULL OR visible_generation>0),active INTEGER NOT NULL CHECK(active IN(0,1)),
      PRIMARY KEY(host,login,source));
      CREATE INDEX IF NOT EXISTS sync_resource_exact ON sync_resource_bindings(host,login,namespace,repository,resource,kind);
      CREATE TABLE IF NOT EXISTS sync_mutation_fences(
      host TEXT NOT NULL,login TEXT NOT NULL,namespace TEXT NOT NULL,repository TEXT NOT NULL,
      resource TEXT NOT NULL,kind TEXT NOT NULL,mutation_ordinal INTEGER NOT NULL CHECK(mutation_ordinal>0),
      PRIMARY KEY(host,login,namespace,repository,resource,kind));")?;
    lookup::initialize_lookup(transaction)?;
    Ok(())
}
/// Parent commit_reserved_batch invokes this within the same transaction as source publication.
pub(super) fn mark_binding_published(
    transaction: &rusqlite::Transaction<'_>,
    scope: &Scope,
    source: &str,
    generation: u64,
) -> Result<()> {
    let exists: bool = transaction.query_row("SELECT EXISTS(SELECT 1 FROM sync_resource_bindings WHERE host=?1 AND login=?2 AND source=?3)",params![scope.host,scope.login,source],|row|row.get(0))?;
    if !exists {
        return Ok(());
    } // Legacy/unbound non-discussion sources are unaffected.
    let updated = transaction.execute("UPDATE sync_resource_bindings SET visible_generation=?4 WHERE host=?1 AND login=?2 AND source=?3 AND reservation_generation=?4 AND active=1",params![scope.host,scope.login,source,generation])?;
    if updated != 1 {
        bail!("Resource-bound publication was invalidated; stale bodies remain hidden");
    }
    Ok(())
}
/// Parent clear/revoke calls after fencing sync_reservations, in its existing transaction.
pub(super) fn fence_binding_visibility(
    transaction: &rusqlite::Transaction<'_>,
    scope: &Scope,
    repository: Option<&str>,
) -> Result<()> {
    transaction.execute(
        "UPDATE sync_resource_bindings SET active=0 WHERE host=?1 AND login=?2",
        params![scope.host, scope.login],
    )?;
    transaction.execute("UPDATE sync_resource_bindings SET visible_generation=NULL WHERE host=?1 AND login=?2 AND (?3 IS NULL OR repository=?3)",params![scope.host,scope.login,repository.map(str::to_ascii_lowercase)])?;
    Ok(())
}
impl Cache {
    /// Atomically register the canonical target BEFORE fetching and reserve its generation.
    pub fn reserve_resource_sync(
        &mut self,
        scope: &Scope,
        source: &str,
        binding: &SyncBinding,
    ) -> Result<u64> {
        positive_scope(scope)?;
        validate_binding(binding)?;
        validate_bound_source(source)?;
        let mut binding = binding.clone();
        binding.repository.make_ascii_lowercase();
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if let Some(existing) = read_binding(&transaction, scope, source)? {
            if existing.binding != binding {
                bail!("Canonical resource binding changed for an existing sync source");
            }
        }
        // Same counter authority as ordinary reservations, without opening a second transaction.
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
            bail!("Resource sync generation exhausted; no binding/reservation changed");
        }
        let generation = previous + 1;
        transaction.execute("INSERT INTO sync_reservations VALUES(?1,?2,?3,?4,1) ON CONFLICT(host,login,source) DO UPDATE SET generation=excluded.generation,active=1",params![scope.host,scope.login,source,generation])?;
        // Reserving does not create visibility, especially after a mutation fence.
        transaction.execute("INSERT INTO sync_resource_bindings VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,NULL,1) ON CONFLICT(host,login,source) DO UPDATE SET reservation_generation=excluded.reservation_generation,active=1",params![scope.host,scope.login,source,namespace(binding.namespace),binding.repository_id,binding.repository,binding.resource,kind(binding.kind),generation])?;
        transaction.commit()?;
        Ok(generation)
    }
    pub fn binding_status(&self, scope: &Scope, source: &str) -> Result<Option<SyncBindingStatus>> {
        positive_scope(scope)?;
        validate_bound_source(source)?;
        read_binding(&self.connection, scope, source)
    }
    /// Exact-category visibility plus conservative scope-wide in-flight read fencing.
    /// No records, source members, drafts, receipts or unrelated visibility are erased.
    pub fn fence_resource_sync(
        &mut self,
        scope: &Scope,
        fence: &SyncFence,
    ) -> Result<SyncFenceResult> {
        self.fence_resource_sync_inner(scope, fence, None)
    }
    /// Late/replayed confirmed receipts cannot invalidate a newer fresh source.
    /// The ordinal is the durable publication attempt ID, never wall-clock time.
    pub fn fence_resource_sync_once(
        &mut self,
        scope: &Scope,
        fence: &SyncFence,
        mutation_ordinal: u64,
    ) -> Result<SyncFenceResult> {
        if mutation_ordinal == 0 || mutation_ordinal > i64::MAX as u64 {
            bail!("Confirmed invalidation needs a positive bounded publication attempt");
        }
        self.fence_resource_sync_inner(scope, fence, Some(mutation_ordinal))
    }
    fn fence_resource_sync_inner(
        &mut self,
        scope: &Scope,
        fence: &SyncFence,
        mutation_ordinal: Option<u64>,
    ) -> Result<SyncFenceResult> {
        positive_scope(scope)?;
        validate_fence(fence)?;
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let mut categories = Vec::new();
        for category in &fence.kinds {
            if let Some(ordinal) = mutation_ordinal {
                let previous: Option<u64> = transaction.query_row("SELECT mutation_ordinal FROM sync_mutation_fences WHERE host=?1 AND login=?2 AND namespace=?3 AND repository=?4 AND resource=?5 AND kind=?6",params![scope.host,scope.login,namespace(fence.namespace),fence.repository.to_ascii_lowercase(),fence.resource,kind(*category)],|row|row.get(0)).optional()?;
                if previous.is_some_and(|previous| previous >= ordinal) {
                    continue;
                }
            }
            categories.push(*category);
        }
        if categories.is_empty() {
            return Ok(SyncFenceResult {
                sources: vec![],
                active_reservations_fenced: 0,
            });
        }
        let mut sources = BTreeSet::new();
        for category in &categories {
            let mut statement=transaction.prepare("SELECT source FROM sync_resource_bindings WHERE host=?1 AND login=?2 AND namespace=?3 AND repository=?4 AND resource=?5 AND kind=?6 AND (?7 IS NULL OR repository_id=?7) ORDER BY source LIMIT 33")?;
            let rows = statement.query_map(
                params![
                    scope.host,
                    scope.login,
                    namespace(fence.namespace),
                    fence.repository.to_ascii_lowercase(),
                    fence.resource,
                    kind(*category),
                    fence.repository_id
                ],
                |row| row.get::<_, String>(0),
            )?;
            for source in rows {
                sources.insert(source?);
                if sources.len() > 32 {
                    bail!("Resource invalidation exceeds 32 canonical source bindings; no partial fence was applied");
                }
            }
        }
        if let Some(ordinal) = mutation_ordinal {
            for category in &categories {
                transaction.execute("INSERT INTO sync_mutation_fences VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(host,login,namespace,repository,resource,kind) DO UPDATE SET mutation_ordinal=excluded.mutation_ordinal",params![scope.host,scope.login,namespace(fence.namespace),fence.repository.to_ascii_lowercase(),fence.resource,kind(*category),ordinal])?;
            }
        }
        let active_reservations_fenced: usize = transaction.query_row(
            "SELECT COUNT(*) FROM sync_reservations WHERE host=?1 AND login=?2 AND active=1",
            params![scope.host, scope.login],
            |row| row.get(0),
        )?;
        // This also fences unbound search-index:<context> tokens derived from old text.
        Cache::fence_reservations(&transaction, scope)?;
        transaction.execute(
            "UPDATE sync_resource_bindings SET active=0 WHERE host=?1 AND login=?2",
            params![scope.host, scope.login],
        )?;
        for source in &sources {
            transaction.execute("UPDATE sync_resource_bindings SET visible_generation=NULL WHERE host=?1 AND login=?2 AND source=?3",params![scope.host,scope.login,source])?;
        }
        transaction.commit()?;
        Ok(SyncFenceResult {
            sources: sources.into_iter().collect(),
            active_reservations_fenced,
        })
    }
}

fn positive_scope(scope: &Scope) -> Result<()> {
    let id = scope
        .login
        .strip_prefix("user-id:")
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|id| *id > 0)
        .context("Resource sync requires a verified positive GitHub account")?;
    if Scope::github(scope.host.to_ascii_lowercase(), Some(id))? != *scope {
        bail!("Resource sync requires a canonical host/principal scope");
    }
    if scope.host.is_empty()
        || !scope
            .host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b".-".contains(&byte))
    {
        bail!("Invalid resource sync host");
    }
    Ok(())
}
fn validate_bound_source(source: &str) -> Result<()> {
    let suffix = source
        .strip_prefix("collaboration-v1:")
        .context("Resource binding requires a canonical discussion source")?;
    if suffix.len() != 64 || !suffix.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("Invalid canonical discussion source identity");
    }
    Ok(())
}
fn validate_binding(binding: &SyncBinding) -> Result<()> {
    crate::config::validate_repository(&binding.repository)?;
    if binding.repository_id == 0 || binding.repository_id > i64::MAX as u64 {
        bail!("Binding requires a positive bounded canonical repository ID");
    }
    validate_resource(&binding.resource)
}
fn validate_resource(resource: &str) -> Result<()> {
    if resource.is_empty() || resource.len() > 1024 || resource.chars().any(char::is_control) {
        bail!("Binding needs an exact bounded opaque parent node");
    }
    Ok(())
}
fn validate_fence(fence: &SyncFence) -> Result<()> {
    crate::config::validate_repository(&fence.repository)?;
    validate_resource(&fence.resource)?;
    if fence
        .repository_id
        .is_some_and(|id| id == 0 || id > i64::MAX as u64)
        || fence.kinds.is_empty()
        || fence.kinds.len() > 5
        || fence.kinds.iter().collect::<BTreeSet<_>>().len() != fence.kinds.len()
    {
        bail!("Invalid canonical source invalidation selector");
    }
    Ok(())
}
fn namespace(value: SyncNamespace) -> &'static str {
    match value {
        SyncNamespace::Collaboration => "collaboration",
    }
}
fn kind(value: SyncBindingKind) -> &'static str {
    match value {
        SyncBindingKind::PullRequestBody => "pull_request_body",
        SyncBindingKind::IssueBody => "issue_body",
        SyncBindingKind::ConversationComment => "conversation_comment",
        SyncBindingKind::Review => "review",
        SyncBindingKind::InlineComment => "inline_comment",
    }
}
fn read_binding(
    connection: &Connection,
    scope: &Scope,
    source: &str,
) -> Result<Option<SyncBindingStatus>> {
    type Saved = (String, u64, String, String, String, u64, Option<u64>, bool);
    let row:Option<Saved>=connection.query_row("SELECT namespace,repository_id,repository,resource,kind,reservation_generation,visible_generation,active FROM sync_resource_bindings WHERE host=?1 AND login=?2 AND source=?3",params![scope.host,scope.login,source],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?,row.get(7)?))).optional()?;
    row.map(|row| {
        if row.0 != "collaboration" {
            bail!("Unknown cached resource binding namespace");
        }
        let kind = match row.4.as_str() {
            "pull_request_body" => SyncBindingKind::PullRequestBody,
            "issue_body" => SyncBindingKind::IssueBody,
            "conversation_comment" => SyncBindingKind::ConversationComment,
            "review" => SyncBindingKind::Review,
            "inline_comment" => SyncBindingKind::InlineComment,
            _ => bail!("Unknown cached resource binding category"),
        };
        let binding = SyncBinding {
            namespace: SyncNamespace::Collaboration,
            repository_id: row.1,
            repository: row.2,
            resource: row.3,
            kind,
        };
        validate_binding(&binding)?;
        Ok(SyncBindingStatus {
            binding,
            reservation_generation: row.5,
            visible_generation: row.6,
            active: row.7,
        })
    })
    .transpose()
}

#[cfg(test)]
#[path = "bindings_tests.rs"]
mod tests;
