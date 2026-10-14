//! Bounded source-visibility journal reads, not invocation/effect confirmation.
use super::*;

pub(super) const METADATA_BYTES_SQL: &str = "length(CAST(repository AS BLOB))+length(CAST(command AS BLOB))+length(CAST(selected_resource AS BLOB))+length(CAST(observed_head AS BLOB))+length(CAST(payload_sha256 AS BLOB))+length(CAST(COALESCE(confirmed_json,'') AS BLOB))";

impl PrWriteJournal {
    /// The source guard must not load malformed pending receipt metadata before
    /// applying its byte budget. This is not invocation/effect confirmation.
    pub(crate) fn source_blocking(&self, target: &PrWriteTarget) -> Result<Option<PrWriteMarker>> {
        let transaction = self.connection.unchecked_transaction()?;
        let selected: Option<(i64, u64)> = transaction
            .query_row(
                &format!("SELECT attempt_id,{METADATA_BYTES_SQL} FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND pr_node=?3 AND state IN('pending','uncertain') ORDER BY attempt_id DESC LIMIT 1"),
                params![target.scope.host, target.scope.login, target.pr_node_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((attempt, bytes)) = selected else {
            transaction.commit()?;
            return Ok(None);
        };
        if bytes > 8192 {
            bail!("Unresolved source publication metadata exceeds its 8 KiB UTF8 guard budget; private text remains hidden");
        }
        let row = transaction
            .query_row(
                &format!("SELECT attempt_id,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND pr_node=?3 AND attempt_id=?4 AND state IN('pending','uncertain') AND {METADATA_BYTES_SQL}<=8192"),
                params![target.scope.host, target.scope.login, target.pr_node_id, attempt],
                read_marker,
            )
            .optional()?
            .context("Unresolved source metadata changed during its bounded read; private text remains hidden")?;
        let marker = decode_marker(target, row)?;
        transaction.commit()?;
        Ok(Some(marker))
    }
}
