//! SOURCE ONLY: shared exact-reference policy on an already validated read-only
//! connection. This journal child is undeclared until the coordinated lease.
//! No schema creation, permission inference, claim, acknowledgement or resend.
use super::*;
use std::time::Instant;

pub(crate) fn readonly_original_publication_v1(
    connection: &Connection,
    target: &PrWriteTarget,
    reference: &PrPublicationReference,
    deadline: Option<Instant>,
) -> Result<Option<PrWriteMarker>> {
    let check = || {
        if let Some(deadline) = deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                bail!("Original publication read deadline expired");
            }
            connection.busy_timeout(remaining.min(Duration::from_secs(2)))?;
        }
        Ok::<_, anyhow::Error>(())
    };
    if reference.attempt_id <= 0
        || reference.pr_node_id != target.pr_node_id
        || !full_hex(&reference.observed_head, &[40, 64])
        || !full_hex(&reference.payload_sha256, &[64])
    {
        bail!("Original reference does not identify an exact PR attempt");
    }
    check()?;
    let transaction = connection.unchecked_transaction()?;
    check()?;
    // Columns/types do not prove the canonical CHECK constraint survived a
    // restored/corrupt database. Measure state bytes before read_marker copies
    // that TEXT as well; do not infer its size from an assumed enum constraint.
    let bytes_sql = format!(
        "{}+length(CAST(state AS BLOB))",
        source_metadata::METADATA_BYTES_SQL
    );
    let selected: Option<(i64, usize)> = transaction.query_row(
        &format!("SELECT attempt_id,{bytes_sql} FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND pr_node=?3 AND attempt_id=?4 AND observed_head=?5 AND payload_sha256=?6"),
        params![target.scope.host,target.scope.login,target.pr_node_id,reference.attempt_id,reference.observed_head,reference.payload_sha256],
        |row|Ok((row.get(0)?,row.get(1)?)),
    ).optional()?;
    check()?;
    let Some((attempt, bytes)) = selected else {
        transaction.commit()?;
        check()?;
        return Ok(None);
    };
    if bytes > 8192 {
        bail!("Original metadata exceeds its 8 KiB UTF8 recovery budget");
    }
    let row = transaction.query_row(
        &format!("SELECT attempt_id,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND pr_node=?3 AND attempt_id=?4 AND observed_head=?5 AND payload_sha256=?6 AND {bytes_sql}<=8192"),
        params![target.scope.host,target.scope.login,target.pr_node_id,attempt,reference.observed_head,reference.payload_sha256],
        read_marker,
    ).optional()?.context("Exact original metadata changed during recovery")?;
    check()?;
    if let Some(encoded) = &row.8 {
        let value: serde_json::Value = serde_json::from_str(encoded)?;
        let object = value
            .as_object()
            .context("Original receipt is not a closed object")?;
        if object.len() != 3
            || !["kind", "resource_id", "node_id"]
                .iter()
                .all(|field| object.contains_key(*field))
        {
            bail!("Original receipt has unknown/missing metadata fields");
        }
    }
    let marker = decode_marker(target, row)?;
    if marker.attempt_id != reference.attempt_id
        || marker.publication_reference() != *reference
        || !marker
            .target
            .repository
            .eq_ignore_ascii_case(&target.repository)
        || marker.target.number != target.number
    {
        bail!("Original attempt belongs to another canonical repository/PR");
    }
    canonical_identifier(&marker.command)?;
    if !marker.selected_resource.is_empty() {
        canonical_identifier(&marker.selected_resource)?;
    }
    match (marker.state, marker.confirmed.as_ref()) {
        (PrWriteState::Confirmed, Some(effect)) => {
            canonical_identifier(&effect.resource_id)?;
            if let Some(node) = &effect.node_id {
                canonical_identifier(node)?;
            }
        }
        (PrWriteState::Confirmed, None)
        | (PrWriteState::Pending | PrWriteState::Uncertain | PrWriteState::Acknowledged, Some(_)) =>
        {
            bail!("Original state/receipt disagree")
        }
        (PrWriteState::Pending | PrWriteState::Uncertain | PrWriteState::Acknowledged, None) => {}
    }
    check()?;
    transaction.commit()?;
    check()?;
    Ok(Some(marker))
}

fn full_hex(value: &str, lengths: &[usize]) -> bool {
    lengths.contains(&value.len()) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
#[path = "pr_write_original_readonly_v1_tests.rs"]
mod tests;
