//! Bounded exact-ID original publication reads, before any provider request.
use super::*;

pub(super) fn read_checked_attempt(
    connection: &Connection,
    scope: &Scope,
    attempt_id: i64,
) -> Result<Option<ActionsWriteMarker>> {
    if attempt_id <= 0 {
        bail!("Original Actions publication requires a positive exact attempt ID");
    }
    let provider = availability::validate_scope(scope)?;
    let transaction = connection.unchecked_transaction()?;
    let bytes: Option<i64> = transaction.query_row(
        "SELECT length(CAST(target_json AS BLOB))+length(CAST(resources_json AS BLOB))+length(CAST(kind_json AS BLOB))+length(CAST(context_json AS BLOB))+length(CAST(options_json AS BLOB))+length(CAST(fingerprint_sha256 AS BLOB))+COALESCE(length(CAST(receipt_json AS BLOB)),0) FROM actions_write_attempts WHERE host=?1 AND principal=?2 AND attempt_id=?3",
        params![scope.host,scope.login,attempt_id],
        |row|row.get(0),
    ).optional()?;
    if let Some(bytes) = bytes {
        if !(1..=128 * 1024).contains(&bytes) {
            bail!("Original Actions publication exceeds its bounded metadata budget; protection remains");
        }
    }
    let marker = read_attempt(&transaction, scope, attempt_id)?;
    if let Some(marker) = &marker {
        availability::validate_saved_marker(&provider, marker)?;
    }
    transaction.commit()?;
    Ok(marker)
}
