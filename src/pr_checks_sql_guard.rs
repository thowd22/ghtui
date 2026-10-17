//! Worker-local PR-checks cache/publication fence. Not authentication,
//! remote access authority, effect confirmation, or a mutation permit.
//! Capture/validate may initialize the normal private cache/journal. Decorate is
//! pure: no database/file/provider access during input or rendering.
#[path = "pr_checks_sql_guard_cleanup_v9.rs"]
mod cleanup_v9;
#[path = "pr_checks_sql_guard_publication_v4.rs"]
mod publication_v4;
#[path = "pr_checks_sql_guard_scoped_v1.rs"]
mod scoped_v1;

use crate::{
    actions_workspace::{
        ActionsAction, ActionsPublicationLeaseSnapshot, ActionsPublicationRow, ActionsWorkspace,
    },
    actions_write_journal::{ActionsUnresolvedPage, ActionsWriteJournal, ActionsWriteState},
    pr_write_journal::{PrWriteJournal, PrWriteTarget},
    providers::{
        gh::{HostIdentity, PrSummary},
        runner::sanitize_terminal,
    },
    storage::Scope,
    theme::SemanticToken,
};
use rusqlite::{params, Connection, OpenFlags};
use sha2::{Digest, Sha256};
use std::{
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

const MAX_EPOCH_ROWS: usize = 10_000;
const MAX_METADATA_BYTES: usize = 8 * 1024 * 1024;
const MAX_ACTION_ROWS: usize = 256;
const MAX_ACTION_BYTES: usize = 128 * 1024;
// Same hard preparse bound as the shared list: 256 rows * 128 KiB each. A
// separate aggregate preflight can race with a writer, so it must not claim a
// smaller hard copy bound than that existing coherent per-row reader provides.
const MAX_ACTION_PAGE_BYTES: usize = MAX_ACTION_ROWS * MAX_ACTION_BYTES;
const MAX_DISPLAY_ROWS: usize = 8192;
const MAX_DISPLAY_BYTES: usize = 1024 * 1024;

pub struct PrChecksSqlGuard {
    database: PathBuf,
    file: (u64, u64),
    target: PrWriteTarget,
    pr_generation: i64,
    metadata: [u8; 32],
    cache_metadata_v4: [u8; 32],
    actions: [u8; 32],
    leases: Arc<ActionsPublicationLeaseSnapshot>,
    rows: Vec<ActionsPublicationRow>,
    total: u64,
    complete: bool,
}
impl std::fmt::Debug for PrChecksSqlGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrChecksSqlGuard")
            .field("listed_originals", &self.rows.len())
            .field("complete", &self.complete)
            .finish_non_exhaustive()
    }
}
impl PrChecksSqlGuard {
    pub(crate) fn workflow_owned_bytes_v9(&self) -> Option<usize> {
        use crate::providers::gh::workflow_capacity_v9 as b;
        let mut n = b::sum([
            b::owner::<Self>(),
            self.database.capacity(),
            self.target.workflow_owned_bytes_v9()?,
            self.leases.workflow_owned_bytes_v9()?,
            b::vector(&self.rows)?,
        ])?;
        for row in &self.rows {
            n = b::sum([n, row.heading.capacity(), b::lines(&row.details)?])?;
        }
        Some(n)
    }
    /// The caller must verify fresh remote numeric identity/access BEFORE this
    /// private SQL read. Local file ownership and a Scope are not that proof.
    /// All calls belong inside the caller's original operation-deadline hooks.
    pub fn capture(
        database: &Path,
        identity: &HostIdentity,
        original: &PrSummary,
    ) -> Result<Self, String> {
        if identity
            .account_id
            .is_none_or(|id| id == 0 || id > i64::MAX as u64)
            || identity.host.len() > 253
            || identity.host.is_empty()
            || identity.host.starts_with('-')
            || !identity
                .host
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b".-".contains(&byte))
            || !full_sha(&original.head_sha)
            || !full_sha(&original.base_sha)
        {
            return Err("PR checks local fence requires a positive original numeric scope and full comparison; it grants no authentication".into());
        }
        let target = PrWriteTarget::new(identity, original).map_err(|error| error.to_string())?;
        let journal = PrWriteJournal::open(database).map_err(|error| error.to_string())?;
        let actions_journal =
            ActionsWriteJournal::open(database).map_err(|error| error.to_string())?;
        let database = journal.database_path().to_owned();
        let file = private_file(&database)?;
        let metadata = metadata_stamp(&database, &target)?;
        let cache_metadata_v4 = metadata_stamp_without_actions_v4(&database, &target)?;
        let pr_generation = journal
            .source_generation(&target)
            .map_err(|error| error.to_string())?;
        if pr_generation < 0 {
            return Err("Malformed original PR source generation; checks remain hidden".into());
        }
        if journal
            .source_blocking(&target)
            .map_err(|error| error.to_string())?
            .is_some()
        {
            return Err("Original PR publication is unresolved; checks remain unavailable until explicit inspection. This proves neither invocation nor success; nothing is resent.".into());
        }
        preflight_actions_page(&database, target.scope())?;
        let page = actions_journal
            .list_unresolved(target.scope(), MAX_ACTION_ROWS)
            .map_err(|error| error.to_string())?;
        let snapshot = ActionsPublicationLeaseSnapshot::prepare(identity, &page)
            .map_err(|error| error.to_string())?;
        let actions = page_stamp(&page)?;
        let rows = publication_rows(&page)?;
        // A truncated list is NOT exact-target absence. Installing an unavailable
        // lease keeps every publication disabled across future local rebuilds.
        // Actual known rows/count remain separately available for honest viewing.
        let leases = if page.complete {
            snapshot
        } else {
            ActionsPublicationLeaseSnapshot::unavailable(identity)
                .map_err(|error| error.to_string())?
        };
        if private_file(&database)? != file
            || metadata_stamp(&database, &target)? != metadata
            || metadata_stamp_without_actions_v4(&database, &target)? != cache_metadata_v4
            || journal
                .source_generation(&target)
                .map_err(|error| error.to_string())?
                != pr_generation
        {
            return Err("Original local PR/checks fences changed during capture; no prepared result admitted".into());
        }
        Ok(Self {
            database,
            file,
            target,
            pr_generation,
            metadata,
            cache_metadata_v4,
            actions,
            leases: Arc::new(leases),
            rows,
            total: page.total,
            complete: page.complete,
        })
    }

    /// Recheck ORIGINAL metadata, not a replacement/rebased guard. Root must do
    /// this before its closing API resource/principal reads; no SQL afterward.
    pub fn validate(&self, database: &Path) -> Result<(), String> {
        let resolved = std::fs::canonicalize(database).map_err(|error| error.to_string())?;
        if resolved != self.database || private_file(database)? != self.file {
            return Err(
                "Original private PR checks database was replaced or aliased; prior results hidden"
                    .into(),
            );
        }
        let journal = PrWriteJournal::open(database).map_err(|error| error.to_string())?;
        let actions_journal =
            ActionsWriteJournal::open(database).map_err(|error| error.to_string())?;
        if metadata_stamp(&self.database, &self.target)? != self.metadata
            || journal
                .source_generation(&self.target)
                .map_err(|error| error.to_string())?
                != self.pr_generation
            || journal
                .source_blocking(&self.target)
                .map_err(|error| error.to_string())?
                .is_some()
        {
            return Err("Original PR checks cache/publication fences changed; refresh the read, never resend a write".into());
        }
        preflight_actions_page(&self.database, self.target.scope())?;
        let page = actions_journal
            .list_unresolved(self.target.scope(), MAX_ACTION_ROWS)
            .map_err(|error| error.to_string())?;
        if page_stamp(&page)? != self.actions
            || metadata_stamp(&self.database, &self.target)? != self.metadata
            || private_file(database)? != self.file
        {
            return Err("Original Actions/checks metadata changed before final source closure; prior results hidden".into());
        }
        Ok(())
    }

    /// Pure presentation. Does NOT validate SQL or prove current remote access.
    pub fn decorate(&self, workspace: &mut ActionsWorkspace) -> Result<(), String> {
        let scope = Scope::github(
            workspace.identity.host.to_ascii_lowercase(),
            workspace.identity.account_id,
        )
        .map_err(|error| error.to_string())?;
        if scope != *self.target.scope()
            || !workspace
                .repository()
                .is_ok_and(|name| name.eq_ignore_ascii_case(self.target.repository()))
        {
            return Err("Captured checks publication rows belong to another exact account/repository; no private rows installed".into());
        }
        self.decorate_matched_read_v1(workspace)
    }
    fn decorate_matched_read_v1(&self, workspace: &mut ActionsWorkspace) -> Result<(), String> {
        workspace.publication_rows = self.rows.clone();
        workspace.publication_total = Some(self.total);
        workspace.publication_complete = self.complete;
        workspace.publication_inspection = None;
        workspace.publication_leases = Some(self.leases.clone());
        workspace.prepare_rows();
        if !self.complete {
            for control in &mut workspace.controls {
                if let ActionsAction::Compose(command) = control.action {
                    if command.publication_kind().is_some() {
                        control.disabled_reason = Some("Publication list is partial; unseen original protection is unknown. All publication controls are unavailable until a fresh complete bounded view. Nothing is resent.".into());
                    }
                }
            }
        }
        Ok(())
    }
}
fn full_sha(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
fn private_file(path: &Path) -> Result<(u64, u64), String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    // SAFETY: geteuid is side-effect-free on both supported Unix platforms.
    let owner = unsafe { libc::geteuid() };
    if !metadata.is_file()
        || metadata.uid() != owner
        || metadata.nlink() != 1
        || metadata.mode() & 0o077 != 0
    {
        return Err("Checks fence needs its original owned private regular SQLite file, never a symlink/hardlink/device".into());
    }
    Ok((metadata.dev(), metadata.ino()))
}
fn connection(path: &Path) -> Result<Connection, String> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| error.to_string())?;
    connection
        .busy_timeout(Duration::from_secs(2))
        .map_err(|error| error.to_string())?;
    Ok(connection)
}

/// Scalar UTF8 preflight is BEFORE list_unresolved copies/parses its bounded
/// metadata rows. No payload/secret/draft/document column is selected here.
fn preflight_actions_page(path: &Path, scope: &Scope) -> Result<(), String> {
    let connection = connection(path)?;
    let sql = "SELECT COUNT(*),COALESCE(SUM(bytes),0),COALESCE(MAX(bytes),0) FROM (SELECT length(CAST(target_json AS BLOB))+length(CAST(resources_json AS BLOB))+length(CAST(kind_json AS BLOB))+length(CAST(context_json AS BLOB))+length(CAST(options_json AS BLOB))+length(CAST(fingerprint_sha256 AS BLOB))+COALESCE(length(CAST(receipt_json AS BLOB)),0) AS bytes FROM actions_write_attempts WHERE host=?1 AND principal=?2 AND state IN('pending','uncertain','accepted_pending') ORDER BY attempt_id DESC LIMIT 256)";
    let (rows, bytes, largest): (usize, usize, usize) = connection
        .query_row(sql, params![scope.host, scope.login], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .map_err(|error| error.to_string())?;
    if rows > MAX_ACTION_ROWS || bytes > MAX_ACTION_PAGE_BYTES || largest > MAX_ACTION_BYTES {
        return Err("PR checks publication metadata exceeds its UTF8 byte budget before copy; protection remains".into());
    }
    Ok(())
}

struct Table {
    name: &'static str,
    fields: &'static str,
    sizes: &'static str,
    predicate: &'static str,
    order: &'static str,
    keys: usize,
    numbers: usize,
    boolean: bool,
}
const TABLES: &[Table] = &[
    Table {
        name: "checkpoints",
        fields: "source,generation,fetched_at,complete",
        sizes: "length(CAST(source AS BLOB))",
        predicate: "host=?1 AND login=?2",
        order: "source",
        keys: 1,
        numbers: 3,
        boolean: true,
    },
    Table {
        name: "sync_reservations",
        fields: "source,generation,active",
        sizes: "length(CAST(source AS BLOB))",
        predicate: "host=?1 AND login=?2",
        order: "source",
        keys: 1,
        numbers: 2,
        boolean: true,
    },
    Table {
        name: "actions_write_generations",
        fields: "resource_key,generation",
        sizes: "length(CAST(resource_key AS BLOB))",
        predicate: "host=?1 AND principal=?2",
        order: "resource_key",
        keys: 1,
        numbers: 1,
        boolean: false,
    },
    Table {
        name: "records",
        fields: "kind,id,repository,fetched_at,complete",
        sizes:
            "length(CAST(kind AS BLOB))+length(CAST(id AS BLOB))+length(CAST(repository AS BLOB))",
        predicate: "host=?1 AND login=?2 AND repository=?3 COLLATE NOCASE",
        order: "kind,id",
        keys: 3,
        numbers: 2,
        boolean: true,
    },
    Table {
        name: "sync_binding_lookup_epochs",
        fields: "namespace,repository,generation",
        sizes: "length(CAST(namespace AS BLOB))+length(CAST(repository AS BLOB))",
        predicate: "host=?1 AND login=?2 AND repository=?3 COLLATE NOCASE",
        order: "namespace,repository",
        keys: 2,
        numbers: 1,
        boolean: false,
    },
];
fn metadata_stamp(path: &Path, target: &PrWriteTarget) -> Result<[u8; 32], String> {
    metadata_stamp_inner_v4(path, target, false)
}
fn metadata_stamp_without_actions_v4(
    path: &Path,
    target: &PrWriteTarget,
) -> Result<[u8; 32], String> {
    metadata_stamp_inner_v4(path, target, true)
}
fn metadata_stamp_inner_v4(
    path: &Path,
    target: &PrWriteTarget,
    claimed_actions: bool,
) -> Result<[u8; 32], String> {
    let connection = connection(path)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    let mut hash = Sha256::new();
    hash.update(b"ghtui-pr-checks-local-fence:v1");
    for value in [
        target.scope().host.as_str(),
        target.scope().login.as_str(),
        target.repository(),
        target.node_id(),
    ] {
        hash.update(value.len().to_le_bytes());
        hash.update(value.as_bytes());
    }
    hash.update(target.number().to_le_bytes());
    let mut total_rows = 0usize;
    let mut total_bytes = 0usize;
    for table in TABLES {
        // Only the purpose-closed live claimed-publication validator uses this
        // projection. Ordinary guards still hash the full Actions generation
        // table and unresolved page exactly as before.
        if claimed_actions && table.name == "actions_write_generations" {
            continue;
        }
        let lengths = table
            .fields
            .split(',')
            .take(table.keys)
            .map(|field| format!("length(CAST({field} AS BLOB))"))
            .collect::<Vec<_>>();
        let largest = if lengths.len() == 1 {
            lengths[0].clone()
        } else {
            format!("max({})", lengths.join(","))
        };
        let mut statement = transaction
            .prepare(&format!(
                "SELECT COUNT(*),COALESCE(SUM({}),0),COALESCE(MAX({}),0) FROM {} WHERE {}",
                table.sizes, largest, table.name, table.predicate
            ))
            .map_err(|error| error.to_string())?;
        let parameters = || {
            let mut args = vec![target.scope().host.as_str(), target.scope().login.as_str()];
            if table.predicate.contains("?3") {
                args.push(target.repository());
            }
            args
        };
        let (rows, bytes, largest): (usize, usize, usize) = statement
            .query_row(rusqlite::params_from_iter(parameters()), |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .map_err(|error| error.to_string())?;
        total_rows = total_rows
            .checked_add(rows)
            .ok_or("Checks metadata row accounting overflow")?;
        total_bytes = total_bytes
            .checked_add(bytes)
            .ok_or("Checks metadata byte accounting overflow")?;
        if total_rows > MAX_EPOCH_ROWS || total_bytes > MAX_METADATA_BYTES || largest > 4096 {
            return Err("PR checks local metadata budget exceeded before epoch/key copy; prior frames hidden".into());
        }
        hash.update(table.name.as_bytes());
        hash.update(rows.to_le_bytes());
        let mut read = transaction
            .prepare(&format!(
                "SELECT {} FROM {} WHERE {} ORDER BY {}",
                table.fields, table.name, table.predicate, table.order
            ))
            .map_err(|error| error.to_string())?;
        let mut cursor = read
            .query(rusqlite::params_from_iter(parameters()))
            .map_err(|error| error.to_string())?;
        let mut copied = 0;
        while let Some(row) = cursor.next().map_err(|error| error.to_string())? {
            for index in 0..table.keys {
                let key: String = row.get(index).map_err(|error| error.to_string())?;
                if key.is_empty() || key.len() > 4096 || key.chars().any(char::is_control) {
                    return Err("Malformed bounded checks metadata key; prior frames hidden".into());
                }
                hash.update(key.len().to_le_bytes());
                hash.update(key.as_bytes());
            }
            for index in 0..table.numbers {
                let value: i64 = row
                    .get(table.keys + index)
                    .map_err(|error| error.to_string())?;
                if value < 0 || (table.boolean && index + 1 == table.numbers && value > 1) {
                    return Err(
                        "Malformed bounded checks metadata generation/state; prior frames hidden"
                            .into(),
                    );
                }
                hash.update(value.to_le_bytes());
            }
            copied += 1;
        }
        if copied != rows {
            return Err("Checks metadata changed during coherent local read".into());
        }
    }
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(hash.finalize().into())
}
fn page_stamp(page: &ActionsUnresolvedPage) -> Result<[u8; 32], String> {
    let mut hash = Sha256::new();
    hash.update(b"ghtui-pr-checks-original-actions:v1");
    hash.update(page.total.to_le_bytes());
    hash.update([u8::from(page.complete)]);
    let mut bytes = 0usize;
    for marker in &page.markers {
        let encoded = serde_json::to_vec(&(
            marker.attempt_id,
            marker.target.scope(),
            marker.target.target(),
            marker.target.resource_keys(),
            marker.kind,
            &marker.context,
            &marker.options,
            &marker.fingerprint_sha256,
            marker.state,
            &marker.receipt,
            marker.created_at,
        ))
        .map_err(|error| error.to_string())?;
        bytes = bytes
            .checked_add(encoded.len())
            .ok_or("Checks original metadata accounting overflow")?;
        if encoded.len() > MAX_ACTION_BYTES || bytes > MAX_ACTION_PAGE_BYTES {
            return Err(
                "Checks original marker preparation budget exceeded; protection retained".into(),
            );
        }
        hash.update(encoded.len().to_le_bytes());
        hash.update(encoded);
    }
    Ok(hash.finalize().into())
}
fn publication_rows(page: &ActionsUnresolvedPage) -> Result<Vec<ActionsPublicationRow>, String> {
    let mut rows = Vec::with_capacity(page.markers.len());
    let mut lines = 0usize;
    let mut bytes = 0usize;
    for marker in &page.markers {
        // Typed metadata is already byte-preflighted/validated, not body data.
        let raw = crate::actions_routes::original_lines(marker);
        let mut details = vec![];
        for line in raw {
            for line in crate::ui::notice_lines(&sanitize_terminal(&line), 68) {
                let value = line
                    .spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>();
                lines += 1;
                bytes = bytes
                    .checked_add(value.len())
                    .ok_or("Checks row byte overflow")?;
                if lines > MAX_DISPLAY_ROWS || bytes > MAX_DISPLAY_BYTES {
                    return Err("Checks original publication rows exceed bounded display preparation; protection retained".into());
                }
                details.push(value);
            }
        }
        rows.push(ActionsPublicationRow {
            attempt_id: marker.attempt_id,
            heading: format!(
                "ORIGINAL ACTIONS ATTEMPT #{} • {:?} • {:?}",
                marker.attempt_id, marker.kind, marker.state
            ),
            details,
            role: match marker.state {
                ActionsWriteState::Pending | ActionsWriteState::AcceptedPending => {
                    SemanticToken::Pending
                }
                ActionsWriteState::Uncertain => SemanticToken::Failure,
                _ => SemanticToken::Muted,
            },
        });
    }
    Ok(rows)
}

#[cfg(test)]
#[path = "pr_checks_sql_guard_tests.rs"]
mod tests;
