//! SOURCE ONLY: undeclared legacy-specific accessor. Reuses Cache's validated
//! private connection/schema; no global metadata cap or permissions change.
use super::*;
use crate::{forms::PrFormKind, pr_write_journal::PrWriteTarget};
use std::time::{Duration, Instant};

pub(crate) struct LegacyPrDraftSnapshot {
    pub(crate) body: Option<String>,
    pub(crate) unresolved: bool,
    pub(crate) original: Option<crate::pr_write_journal::PrWriteMarker>,
    pub(crate) original_invalid: bool,
}
impl Cache {
    /// Narrow read-only OPEN: same descriptor/alias/ownership/0600 policy, no
    /// schema/corpus migration or public writable Cache authority. Preparation
    /// may create/repair local files; this is NOT a zero-filesystem-mutation API.
    pub(crate) fn legacy_pr_draft_snapshot_file_before_v1(
        path: &Path,
        scope: &Scope,
        resource: &str,
        target: &PrWriteTarget,
        max_bytes: usize,
        deadline: Instant,
    ) -> Result<LegacyPrDraftSnapshot> {
        if deadline <= Instant::now() {
            bail!("Original private draft deadline expired before any file preparation");
        }
        let private_files = crate::storage_private::prepare(path)?;
        if deadline <= Instant::now() {
            bail!("Original private draft deadline expired before opening SQLite");
        }
        let connection = Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        drop(private_files);
        before_query(&connection, deadline)?;
        let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        before_query(&connection, deadline)?;
        if version == 0 {
            let tables: u64 = connection.query_row(
                "SELECT count(*) FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'",
                [],
                |row| row.get(0),
            )?;
            before_query(&connection, deadline)?;
            if tables != 0 {
                bail!(
                    "Version-zero private database contains user tables; no absent-draft fallback"
                );
            }
            validate_key(scope, resource, target, max_bytes)?;
            return Ok(LegacyPrDraftSnapshot {
                body: None,
                unresolved: false,
                original: None,
                original_invalid: false,
            });
        }
        if version > 4 {
            bail!("Future private draft schema is not supported; no fields restored");
        }
        validate_columns(
            &connection,
            "drafts",
            &[
                ("host", "TEXT", true, 1),
                ("login", "TEXT", true, 2),
                ("resource", "TEXT", true, 3),
                ("body", "TEXT", true, 0),
                ("updated_at", "INTEGER", true, 0),
            ],
            deadline,
        )?;
        before_query(&connection, deadline)?;
        let journal_kind: Option<String> = connection
            .query_row(
                "SELECT type FROM sqlite_master WHERE name='pr_write_attempts'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(kind) = journal_kind {
            if kind != "table" {
                bail!("Original journal is not a canonical table");
            }
            validate_columns(
                &connection,
                "pr_write_attempts",
                &[
                    ("attempt_id", "INTEGER", false, 1),
                    ("host", "TEXT", true, 0),
                    ("principal", "TEXT", true, 0),
                    ("pr_node", "TEXT", true, 0),
                    ("repository", "TEXT", true, 0),
                    ("pr_number", "INTEGER", true, 0),
                    ("command", "TEXT", true, 0),
                    ("selected_resource", "TEXT", true, 0),
                    ("observed_head", "TEXT", true, 0),
                    ("payload_sha256", "TEXT", true, 0),
                    ("state", "TEXT", true, 0),
                    ("confirmed_json", "TEXT", false, 0),
                    ("created_at", "INTEGER", true, 0),
                ],
                deadline,
            )?;
        }
        snapshot(
            &connection,
            scope,
            resource,
            target,
            max_bytes,
            Some(deadline),
        )
    }
    #[cfg(test)]
    pub(crate) fn legacy_pr_draft_snapshot_v1(
        &self,
        scope: &Scope,
        resource: &str,
        target: &PrWriteTarget,
        max_bytes: usize,
    ) -> Result<LegacyPrDraftSnapshot> {
        snapshot(&self.connection, scope, resource, target, max_bytes, None)
    }
}
fn validate_key(
    scope: &Scope,
    resource: &str,
    target: &PrWriteTarget,
    max_bytes: usize,
) -> Result<()> {
    if scope != target.scope()
        || !(1..=2 * 1024 * 1024).contains(&max_bytes)
        || !PrFormKind::ALL.iter().any(|(kind, _)| {
            resource
                == format!(
                    "{}/pull/{}/composer/{}",
                    target.repository(),
                    target.number(),
                    kind.key()
                )
        })
    {
        bail!("Invalid exact legacy draft scope/key or byte budget");
    }
    Ok(())
}
fn before_query(connection: &Connection, deadline: Instant) -> Result<()> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        bail!("Original private draft read deadline expired; no private result");
    }
    connection.busy_timeout(remaining.min(Duration::from_secs(2)))?;
    Ok(())
}
fn validate_columns(
    connection: &Connection,
    table: &str,
    expected: &[(&str, &str, bool, u32)],
    deadline: Instant,
) -> Result<()> {
    // table is one of TWO fixed literals, never a user/SQL identifier.
    before_query(connection, deadline)?;
    let kind: Option<String> = connection
        .query_row(
            "SELECT type FROM sqlite_master WHERE name=?1",
            [table],
            |row| row.get(0),
        )
        .optional()?;
    if kind.as_deref() != Some("table") {
        bail!("Supported private schema is missing a required canonical table");
    }
    before_query(connection, deadline)?;
    let mut statement=connection.prepare(&format!("SELECT name,type,\"notnull\",pk FROM pragma_table_info('{table}') WHERE length(CAST(name AS BLOB))<=256 AND length(CAST(type AS BLOB))<=64 LIMIT 33"))?;
    let columns = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, bool>(2)?,
                row.get::<_, u32>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    before_query(connection, deadline)?;
    let total: u64 = connection.query_row(
        &format!("SELECT count(*) FROM pragma_table_info('{table}')"),
        [],
        |row| row.get(0),
    )?;
    before_query(connection, deadline)?;
    if columns.len() > 32
        || columns.len() != expected.len()
        || u64::try_from(columns.len()).ok() != Some(total)
        || expected.iter().any(|&(name, kind, notnull, pk)| {
            !columns.iter().any(|actual| {
                actual.0 == name
                    && actual.1.eq_ignore_ascii_case(kind)
                    && actual.2 == notnull
                    && actual.3 == pk
            })
        })
    {
        bail!("Private draft/journal columns or types are corrupt/unsupported; no fields restored");
    }
    Ok(())
}
fn snapshot(
    connection: &Connection,
    scope: &Scope,
    resource: &str,
    target: &PrWriteTarget,
    max_bytes: usize,
    deadline: Option<Instant>,
) -> Result<LegacyPrDraftSnapshot> {
    validate_key(scope, resource, target, max_bytes)?;
    let check = || {
        if let Some(deadline) = deadline {
            before_query(connection, deadline)?;
        }
        Ok::<_, anyhow::Error>(())
    };
    check()?;
    let transaction = connection.unchecked_transaction()?;
    check()?;
    let metadata: Option<(String, usize)> = transaction.query_row(
            "SELECT typeof(body),length(CAST(body AS BLOB)) FROM drafts WHERE host=?1 AND login=?2 AND resource=?3",
            params![scope.host,scope.login,resource], |row| Ok((row.get(0)?,row.get(1)?)),
        ).optional()?;
    if metadata
        .as_ref()
        .is_some_and(|(kind, bytes)| kind != "text" || *bytes > max_bytes)
    {
        bail!("Private legacy draft type/UTF8 byte budget is invalid; no body copied");
    }
    check()?;
    let body: Option<String> = transaction.query_row(
            "SELECT body FROM drafts WHERE host=?1 AND login=?2 AND resource=?3 AND typeof(body)='text' AND length(CAST(body AS BLOB))<=?4",
            params![scope.host,scope.login,resource,max_bytes], |row| row.get(0),
        ).optional()?;
    if metadata.is_some() && body.is_none() {
        bail!("Exact local draft changed during bounded read");
    }
    check()?;
    let has_journal: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='pr_write_attempts')",
            [], |row| row.get(0),
        )?;
    // A metadata-only barrier test. Match the journal's scope + opaque PR
    // node barrier: a corrupt/moved row's repository or number must not
    // make an unresolved attempt disappear. This query never copies receipt/
    // intent/body, asserts invocation, acknowledges or creates a journal.
    let unresolved = if has_journal {
        check()?;
        transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND pr_node=?3 AND (state NOT IN('confirmed','acknowledged') OR state IS NULL))",
                params![scope.host,scope.login,target.node_id()],
                |row|row.get(0),
            )?
    } else {
        false
    };
    check()?;
    transaction.commit()?;
    check()?;
    // Bounded header traversal ignores edited values. Original lookup is done
    // on this validated READ_ONLY connection before any editor decoding. It is
    // a separate short exact-reference transaction; no journal open/migration.
    let mut original = None;
    let mut original_invalid = false;
    if !unresolved {
        if let Some(raw) = &body {
            if let Ok(header) = crate::legacy_pr_draft_binding_v1::draft_header(raw) {
                if let Some(reference) = &header.publication {
                    if has_journal {
                        match crate::pr_write_journal::readonly_original_publication_v1(
                            connection, target, reference, deadline,
                        ) {
                            Ok(Some(marker)) => original = Some(marker),
                            Ok(None) | Err(_) => original_invalid = true,
                        }
                    } else {
                        original_invalid = true;
                    }
                }
            }
        }
    }
    check()?;
    Ok(LegacyPrDraftSnapshot {
        body,
        unresolved,
        original,
        original_invalid,
    })
}
