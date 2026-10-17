//! Fixed raw source/Clear restriction for the genuine State Inbox capture.
//! No source-generation reservation, write, hydration or SQL callback escapes.
use super::*;
use crate::state_inbox_sync_v76::StateInboxOriginalV76;
use rusqlite::types::ValueRef;
use sha2::{Digest, Sha256};
use std::os::unix::fs::MetadataExt;

pub(crate) struct StateSourceEpochV76 {
    file: repository_catalogue_intents_v1::Issuer,
    digest: [u8; 32],
}
// Actual absence after Clear is not generation zero and does not grant a new
// reservation. It remains a distinct raw predecessor restriction until the
// complete fresh read commits its genuine next generation atomically.
enum StartupCheckpointV80 {
    Present(u64),
    Absent,
}
pub(crate) struct StateStartupReadV80 {
    epoch: StateSourceEpochV76,
    checkpoint: StartupCheckpointV80,
    cached_available: bool,
}
impl StateStartupReadV80 {
    pub(crate) fn retained_bytes(&self) -> Option<usize> {
        std::mem::size_of::<Self>().checked_add(self.epoch.file.retained_bytes_v80()?)
    }
    pub(crate) fn capture(
        scope: &Scope,
        source: &str,
        path: &Path,
        cancel: &crate::providers::runner::CancellationToken,
        deadline: std::time::Instant,
    ) -> Result<Option<Self>> {
        match std::fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
            Ok(_) => {}
        }
        let cache = readonly_at(path, cancel, deadline)?;
        // Old/fresh non-State databases keep their original cached loader.
        let table: bool = cache.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='pr_write_state_filter_fences_v68')", [], |r| r.get(0),
        )?;
        if !table {
            return Ok(None);
        }
        let file = repository_catalogue_intents_v1::issuer(&cache)?;
        let tx = cache.connection.unchecked_transaction()?;
        let count: usize = tx.query_row(
            "SELECT count(*) FROM pr_write_state_filter_fences_v68 WHERE host=?1 AND principal=?2",
            params![scope.host, scope.login],
            |r| r.get(0),
        )?;
        if count == 0 {
            tx.commit()?;
            return Ok(None);
        }
        if count > 32 {
            bail!("Startup State metadata exceeds32 actual keys");
        }
        let digest = raw_epoch(&tx, scope)?;
        super::state_filter_v68::validate_startup_rows_v80(&tx, scope)?;
        let cached_available = super::state_filter_v68::guard_source(&tx, scope, source).is_ok();
        let checkpoint = tx.query_row(
            "SELECT generation,typeof(generation)='integer' AND generation>0 AND typeof(fetched_at)='integer' AND fetched_at>=0 AND typeof(complete)='integer' AND complete IN(0,1) FROM checkpoints WHERE host=?1 AND login=?2 AND source=?3",
            params![scope.host,scope.login,source], |r| Ok((r.get::<_,u64>(0)?,r.get::<_,bool>(1)?)),
        ).optional()?;
        let checkpoint = match checkpoint {
            Some((generation, true)) => StartupCheckpointV80::Present(generation),
            Some(_) => bail!("Startup source checkpoint raw types changed"),
            None => StartupCheckpointV80::Absent,
        };
        tx.commit()?;
        if !file.file_still_matches_v63()
            || cancel.is_cancelled()
            || std::time::Instant::now() >= deadline
        {
            bail!("Startup State restriction file/clock changed");
        }
        Ok(Some(Self {
            epoch: StateSourceEpochV76 { file, digest },
            checkpoint,
            cached_available,
        }))
    }
    // Display-only existing Workspace field; actual typed absence is retained
    // above and is compared by the raw epoch, NEVER a synthesized SQL epoch.
    pub(crate) fn generation(&self) -> u64 {
        match self.checkpoint {
            StartupCheckpointV80::Present(n) => n,
            StartupCheckpointV80::Absent => 0,
        }
    }
    pub(crate) fn cached_available(&self) -> bool {
        self.cached_available
    }
    pub(crate) fn validate(
        &self,
        scope: &Scope,
        source: &str,
        path: &Path,
        cancel: &crate::providers::runner::CancellationToken,
        deadline: std::time::Instant,
    ) -> Result<()> {
        if source.len() > 2048 || !self.epoch.file.file_still_matches_v63() {
            bail!("Original startup State source/file changed");
        }
        let cache = readonly_at(path, cancel, deadline)?;
        if repository_catalogue_intents_v1::issuer(&cache)? != self.epoch.file {
            bail!("Original startup State database changed");
        }
        let tx = cache.connection.unchecked_transaction()?;
        if raw_epoch(&tx, scope)? != self.epoch.digest {
            bail!("Original startup State/Clear/raw checkpoint changed");
        }
        tx.commit()?;
        if !self.epoch.file.file_still_matches_v63()
            || cancel.is_cancelled()
            || std::time::Instant::now() >= deadline
        {
            bail!("Original startup State restriction expired/changed");
        }
        Ok(())
    }
    pub(crate) fn validate_file(&self, path: &Path) -> Result<()> {
        let named = std::fs::symlink_metadata(path)?;
        if !named.is_file()
            || named.nlink() != 1
            || named.mode() & 0o777 != 0o600
            || named.uid() != unsafe { libc::geteuid() }
            || !self.epoch.file.file_still_matches_v63()
        {
            bail!("Original startup private database changed");
        }
        // Canonical path equality, not merely a still-existing original inode.
        let connection =
            Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        if repository_catalogue_intents_v1::issuer(&Cache { connection })? != self.epoch.file {
            bail!("Startup intent was moved to a different private database");
        }
        Ok(())
    }
}
impl StateSourceEpochV76 {
    pub(crate) fn capture(original: &StateInboxOriginalV76) -> Result<Self> {
        let cache = readonly(original)?;
        let file = repository_catalogue_intents_v1::issuer(&cache)?;
        let transaction = cache.connection.unchecked_transaction()?;
        let digest = raw_epoch(&transaction, original.scope())?;
        super::state_filter_v68::validate_startup_rows_v80(&transaction, original.scope())?;
        let keys: usize = transaction.query_row(
            "SELECT count(*) FROM pr_write_state_filter_fences_v68 WHERE host=?1 AND principal=?2",
            params![original.scope().host, original.scope().login],
            |row| row.get(0),
        )?;
        if !(1..=32).contains(&keys) {
            bail!("State source recovery requires1..32 actual affected object keys; ordinary refresh remains separate");
        }
        transaction.commit()?;
        if !file.file_still_matches_v63() {
            bail!("Original State queue file changed during capture");
        }
        Ok(Self { file, digest })
    }
    pub(crate) fn validate_original(&self, original: &StateInboxOriginalV76) -> Result<()> {
        if !self.file.file_still_matches_v63() {
            bail!("Original State queue file changed");
        }
        let cache = readonly(original)?;
        if repository_catalogue_intents_v1::issuer(&cache)? != self.file {
            bail!("Original State source database path/identity changed");
        }
        let transaction = cache.connection.unchecked_transaction()?;
        self.validate_transaction(original, &transaction)?;
        transaction.commit()?;
        if !self.file.file_still_matches_v63() {
            bail!("Original State queue file changed after validation");
        }
        Ok(())
    }
    pub(crate) fn validate_transaction(
        &self,
        original: &StateInboxOriginalV76,
        tx: &Connection,
    ) -> Result<()> {
        if !self.file.file_still_matches_v63() || raw_epoch(tx, original.scope())? != self.digest {
            bail!("Original State queue/Clear/raw terminal epoch changed; no source commit");
        }
        Ok(())
    }
}
fn readonly(original: &StateInboxOriginalV76) -> Result<Cache> {
    readonly_at(
        original.database(),
        original.cancellation(),
        original.deadline(),
    )
}
fn readonly_at(
    path: &Path,
    cancel: &crate::providers::runner::CancellationToken,
    deadline: std::time::Instant,
) -> Result<Cache> {
    if cancel.is_cancelled() || std::time::Instant::now() >= deadline {
        bail!("Original State complete-source read cancelled or expired");
    }
    let named = std::fs::symlink_metadata(path)?;
    // Exact existing private database invariants, without prepare()/chmod,
    // migrations or create-on-missing side effects in pure capture/refusal.
    if !named.is_file()
        || named.nlink() != 1
        || named.mode() & 0o777 != 0o600
        || named.uid() != unsafe { libc::geteuid() }
    {
        bail!("Original State queue database privacy identity unavailable");
    }
    let connection = Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    connection.busy_timeout(
        deadline
            .saturating_duration_since(std::time::Instant::now())
            .min(std::time::Duration::from_secs(2)),
    )?;
    let now = std::fs::symlink_metadata(path)?;
    if !now.is_file()
        || now.nlink() != 1
        || now.dev() != named.dev()
        || now.ino() != named.ino()
        || now.mode() & 0o777 != 0o600
        || now.uid() != named.uid()
    {
        bail!("Original State queue database changed while opening read-only restriction");
    }
    Ok(Cache { connection })
}
/// Two-pass count/byte preflight over a CLOSED fixed profile. Every value is
/// borrowed as ValueRef, with framed type/length; no raw private payload copy.
fn raw_epoch(tx: &Connection, scope: &Scope) -> Result<[u8; 32]> {
    let mut digest = Sha256::new();
    let mut all_rows = 0usize;
    let mut all_bytes = 0usize;
    for (table, fields, predicate, order) in [
        ("sync_reservations", "host,login,source,generation,active", "host=?1 AND login=?2", "source"),
        ("checkpoints", "host,login,source,generation,fetched_at,complete", "host=?1 AND login=?2", "source"),
        ("sync_resource_bindings", "host,login,source,namespace,repository_id,repository,resource,kind,reservation_generation,visible_generation,active", "host=?1 AND login=?2", "source"),
        ("sync_binding_lookup_epochs", "host,login,namespace,repository,generation", "host=?1 AND login=?2", "namespace,repository"),
        ("sync_mutation_fences", "host,login,namespace,repository,resource,kind,mutation_ordinal", "host=?1 AND login=?2", "namespace,repository,resource,kind"),
        ("saved_views", "host,login,name,payload", "host=?1 AND login=?2", "name"),
        ("pr_write_scopes", "host,principal,pr_node,generation", "host=?1 AND principal=?2", "pr_node"),
        ("pr_write_attempts", "attempt_id,host,principal,pr_node,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at", "host=?1 AND principal=?2 AND (command LIKE 'state-%' OR selected_resource LIKE 'state-v68/%' OR attempt_id IN(SELECT attempt_id FROM pr_write_state_origins_v68))", "attempt_id"),
        ("pr_write_state_origins_v68", "attempt_id,repository_id,pr_id,issue_id,command,base_key_v71,action_tag_v71,parent_ack_attempt_v71,origin_json", "attempt_id IN(SELECT attempt_id FROM pr_write_attempts WHERE host=?1 AND principal=?2)", "attempt_id"),
        ("pr_write_state_dispositions_v68", "attempt_id,kind,receipt_json", "attempt_id IN(SELECT attempt_id FROM pr_write_attempts WHERE host=?1 AND principal=?2)", "attempt_id"),
        ("pr_write_state_filter_fences_v68", "host,principal,pr_node,repository_id,pr_id,repository,attempt_id,raw_origin_sha256,raw_disposition_sha256,status", "host=?1 AND principal=?2", "pr_node"),
        ("pr_write_state_source_watermarks_v73", "host,principal,pr_node,source,source_generation,mutation_ordinal,repository_id,pr_id", "host=?1 AND principal=?2", "pr_node,source"),
    ] {
        let size = fields.split(',').map(|field| format!("CASE typeof({field}) WHEN 'null' THEN 0 WHEN 'integer' THEN 8 WHEN 'real' THEN 8 ELSE length(CAST({field} AS BLOB)) END")).collect::<Vec<_>>().join("+");
        let (count, bytes): (usize, usize) = tx.query_row(&format!("SELECT count(*),COALESCE(sum({size}),0) FROM {table} WHERE {predicate}"),
            params![scope.host, scope.login], |r| Ok((r.get(0)?, r.get(1)?)))?;
        all_rows = all_rows.checked_add(count).filter(|n| *n <= 512).context("State complete-source raw epoch exceeds512 rows before copy")?;
        all_bytes = all_bytes.checked_add(bytes).filter(|n| *n <= 64 * 1024).context("State complete-source raw epoch exceeds64KiB before copy")?;
        digest.update((table.len() as u64).to_le_bytes()); digest.update(table.as_bytes()); digest.update((count as u64).to_le_bytes());
        let mut statement = tx.prepare(&format!("SELECT {fields} FROM {table} WHERE {predicate} ORDER BY {order}"))?;
        let columns = statement.column_count();
        digest.update((columns as u64).to_le_bytes());
        let mut rows = statement.query(params![scope.host, scope.login])?;
        let (mut observed, mut observed_bytes) = (0usize, 0usize);
        while let Some(row) = rows.next()? {
            observed = observed.checked_add(1).filter(|n| *n <= count).context("State raw epoch changed during observation")?;
            for column in 0..columns {
                let value = row.get_ref(column)?;
                let len = match value { ValueRef::Null => 0, ValueRef::Integer(_) | ValueRef::Real(_) => 8, ValueRef::Text(v) | ValueRef::Blob(v) => v.len() };
                observed_bytes = observed_bytes.checked_add(len).filter(|n| *n <= bytes).context("State raw epoch bytes changed inside observation")?;
                digest.update((len as u64).to_le_bytes());
                match value {
                    ValueRef::Null => digest.update([0]),
                    ValueRef::Integer(n) => { digest.update([1]); digest.update(n.to_le_bytes()); }
                    ValueRef::Real(n) => { digest.update([2]); digest.update(n.to_bits().to_le_bytes()); }
                    ValueRef::Text(v) => { digest.update([3]); digest.update(v); }
                    ValueRef::Blob(v) => { digest.update([4]); digest.update(v); }
                }
            }
        }
        if observed != count || observed_bytes != bytes { bail!("State raw epoch count/bytes changed in same snapshot"); }
    }
    Ok(digest.finalize().into())
}
