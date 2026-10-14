//! Bounded account journal epoch for prepared Dashboard reads, not remote
//! authentication, access authority, or publication success. No body is read.
use crate::{
    pr_write_journal::{PrWriteJournal, PrWriteTarget},
    providers::gh::MetricsPrObservation,
    storage::Scope,
};
use rusqlite::{params, Connection, OpenFlags};
use sha2::{Digest, Sha256};
use std::{
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

pub struct MetricsReadGuard {
    database: PathBuf,
    scope: Scope,
    file_identity: (u64, u64),
    digest: [u8; 32],
    rows: usize,
}
impl std::fmt::Debug for MetricsReadGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MetricsReadGuard")
            .field("bounded_source_epochs", &self.rows)
            .finish_non_exhaustive()
    }
}
impl MetricsReadGuard {
    pub fn capture(database: &Path, scope: &Scope) -> Result<Self, String> {
        let uid = scope
            .login
            .strip_prefix("user-id:")
            .and_then(|id| id.parse::<u64>().ok())
            .filter(|id| *id > 0)
            .ok_or("Dashboard journal guard needs an original verified positive principal")?;
        if Scope::github(&scope.host, Some(uid)).map_err(|error| error.to_string())? != *scope {
            return Err("Dashboard journal scope is not canonical".into());
        }
        let journal = PrWriteJournal::open(database).map_err(|error| error.to_string())?;
        let database = journal.database_path().to_owned();
        let original_file_identity = file_identity(&database)?;
        let connection = Connection::open_with_flags(&database, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|error| error.to_string())?;
        let transaction = connection
            .unchecked_transaction()
            .map_err(|error| error.to_string())?;
        let (rows,bytes):(usize,usize)=transaction.query_row(
            "SELECT COUNT(*),COALESCE(SUM(length(CAST(pr_node AS BLOB))),0) FROM pr_write_scopes WHERE host=?1 AND principal=?2",
            params![scope.host,scope.login], |row| Ok((row.get(0)?,row.get(1)?)),
        ).map_err(|error|error.to_string())?;
        if rows > 10_000 || bytes > 8 * 1024 * 1024 {
            return Err(
                "Dashboard journal metadata budget exceeded before source epochs were copied"
                    .into(),
            );
        }
        let mut hash = Sha256::new();
        hash.update(b"ghtui-dashboard-journal-epoch:v1");
        for field in [&scope.host, &scope.login] {
            hash.update(field.len().to_le_bytes());
            hash.update(field.as_bytes());
        }
        {
            let mut statement=transaction.prepare("SELECT pr_node,generation FROM pr_write_scopes WHERE host=?1 AND principal=?2 ORDER BY pr_node").map_err(|error|error.to_string())?;
            let mut cursor = statement
                .query(params![scope.host, scope.login])
                .map_err(|error| error.to_string())?;
            let mut copied = 0;
            while let Some(row) = cursor.next().map_err(|error| error.to_string())? {
                let node: String = row.get(0).map_err(|error| error.to_string())?;
                let generation: i64 = row.get(1).map_err(|error| error.to_string())?;
                if node.is_empty()
                    || node.len() > 1024
                    || node.chars().any(char::is_control)
                    || generation < 0
                {
                    return Err(
                        "Malformed original Dashboard source epoch; results remain hidden".into(),
                    );
                }
                hash.update(node.len().to_le_bytes());
                hash.update(node.as_bytes());
                hash.update(generation.to_le_bytes());
                copied += 1;
            }
            if copied != rows {
                return Err("Dashboard journal source epochs changed during coherent read".into());
            }
        }
        transaction.commit().map_err(|error| error.to_string())?;
        if file_identity(&database)? != original_file_identity {
            return Err("Dashboard journal database was replaced during source admission".into());
        }
        Ok(Self {
            database,
            scope: scope.clone(),
            file_identity: original_file_identity,
            digest: hash.finalize().into(),
            rows,
        })
    }
    pub fn validate(&self, database: &Path) -> Result<(), String> {
        let current = Self::capture(database, &self.scope)?;
        if current.database != self.database
            || current.file_identity != self.file_identity
            || current.digest != self.digest
        {
            return Err("Original PR publication changed during Dashboard read; refresh counts rather than resend any write".into());
        }
        Ok(())
    }
    /// Current unresolved attempts cannot establish any measured state. The
    /// immutable guard is captured BEFORE source fetch, including first parents
    /// unknown at that point, and validated after this exact-parent check.
    pub fn validate_parents(&self, observations: &[MetricsPrObservation]) -> Result<(), String> {
        if observations.len() > 10_000 {
            return Err("Dashboard parent journal budget exceeded".into());
        }
        let journal = PrWriteJournal::open(&self.database).map_err(|error| error.to_string())?;
        for parent in observations {
            if parent.repository.host != self.scope.host
                || parent.id == 0
                || parent.repository.id == 0
            {
                return Err("Dashboard parent differs from its original journal scope".into());
            }
            let key = PrWriteTarget::for_source(
                &self.scope,
                &parent.repository.full_name,
                &parent.node_id,
                parent.number,
            )
            .map_err(|error| error.to_string())?;
            if journal
                .source_blocking(&key)
                .map_err(|error| error.to_string())?
                .is_some()
            {
                return Err("Original PR publication is unresolved; Dashboard state remains unavailable until explicit inspection/acknowledgement".into());
            }
        }
        self.validate(&self.database)
    }
}
fn file_identity(path: &Path) -> Result<(u64, u64), String> {
    let metadata = std::fs::metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err("Dashboard journal needs its original regular SQLite file".into());
    }
    Ok((metadata.dev(), metadata.ino()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    fn fixture() -> tempfile::TempDir {
        tempfile::Builder::new()
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap()
    }
    fn scope(uid: u64) -> Scope {
        Scope::github("github.invalid", Some(uid)).unwrap()
    }
    #[test]
    fn metrics_journal_guard_detects_actual_original_and_first_parent_epoch_changes() {
        let fixture = fixture();
        let database = fixture.path().join("cache.sqlite3");
        let before = MetricsReadGuard::capture(&database, &scope(42)).unwrap();
        before.validate(&database).unwrap();
        let sql = Connection::open(&database).unwrap();
        sql.execute("INSERT INTO pr_write_scopes(host,principal,pr_node,generation) VALUES(?1,?2,'PR_First',1)",params![scope(42).host,scope(42).login]).unwrap();
        assert!(before
            .validate(&database)
            .unwrap_err()
            .contains("publication changed"));
        let current = MetricsReadGuard::capture(&database, &scope(42)).unwrap();
        current.validate(&database).unwrap();
        sql.execute(
            "UPDATE pr_write_scopes SET generation=generation+1 WHERE host=?1 AND principal=?2",
            params![scope(42).host, scope(42).login],
        )
        .unwrap();
        assert!(current.validate(&database).is_err());
    }
    #[test]
    fn metrics_journal_guard_ignores_other_principal_and_reads_no_body_or_receipt_payload() {
        let fixture = fixture();
        let database = fixture.path().join("cache.sqlite3");
        let guard = MetricsReadGuard::capture(&database, &scope(42)).unwrap();
        let sql = Connection::open(&database).unwrap();
        sql.execute("INSERT INTO pr_write_scopes(host,principal,pr_node,generation) VALUES(?1,?2,'PR_Other',7)",params![scope(43).host,scope(43).login]).unwrap();
        sql.execute_batch("DROP TABLE pr_write_attempts;").unwrap();
        // Opening the journal recreates its empty schema; the epoch only copies
        // bounded node/generation metadata, never similarly named raw bodies.
        guard.validate(&database).unwrap();
        assert!(!format!("{guard:?}").contains("github.invalid"));
        assert!(!format!("{guard:?}").contains("cache.sqlite3"));
    }
    #[test]
    fn metrics_journal_guard_rejects_foreign_database_scope_and_utf8_before_copy() {
        let fixture = fixture();
        let database = fixture.path().join("cache.sqlite3");
        let guard = MetricsReadGuard::capture(&database, &scope(42)).unwrap();
        assert!(guard
            .validate(&fixture.path().join("other.sqlite3"))
            .is_err());
        assert!(MetricsReadGuard::capture(
            &database,
            &Scope::new("github.invalid", "reused-login").unwrap()
        )
        .is_err());
        let sql = Connection::open(&database).unwrap();
        sql.execute(
            "INSERT INTO pr_write_scopes(host,principal,pr_node,generation) VALUES(?1,?2,?3,1)",
            params![
                scope(42).host,
                scope(42).login,
                "💫".repeat(2 * 1024 * 1024 + 1)
            ],
        )
        .unwrap();
        assert!(MetricsReadGuard::capture(&database, &scope(42))
            .unwrap_err()
            .contains("before source epochs were copied"));
    }
}
