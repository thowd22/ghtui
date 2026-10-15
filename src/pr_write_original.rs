//! Bounded exact-reference recovery reads. This is a journal child, intentionally
//! undeclared until the shared integration owner activates and verifies it.
//! A recovered marker is read-only history, never a publication permit.
use super::*;

impl PrWriteJournal {
    /// Read the exact recorded attempt without deriving a new intent from a
    /// draft, current head, selected branch or restored approval. The worker
    /// must independently verify the current principal before presenting any
    /// private target and must not turn any returned state into a retry.
    pub fn original_publication(
        &self,
        target: &PrWriteTarget,
        reference: &PrPublicationReference,
    ) -> Result<Option<PrWriteMarker>> {
        if reference.attempt_id <= 0
            || reference.pr_node_id != target.pr_node_id
            || !full_hex(&reference.observed_head, &[40, 64])
            || !full_hex(&reference.payload_sha256, &[64])
        {
            bail!("Original publication reference does not identify an exact PR attempt");
        }
        let transaction = self.connection.unchecked_transaction()?;
        let bytes_sql = source_metadata::METADATA_BYTES_SQL;
        // Measure bytes inside SQLite before reading/cloning or decoding the
        // stored strings. Both reads share the same short read transaction.
        let selected: Option<(i64, usize)> = transaction
            .query_row(
                &format!("SELECT attempt_id,{bytes_sql} FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND pr_node=?3 AND attempt_id=?4 AND observed_head=?5 AND payload_sha256=?6"),
                params![target.scope.host, target.scope.login, target.pr_node_id, reference.attempt_id, reference.observed_head, reference.payload_sha256],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((attempt, bytes)) = selected else {
            transaction.commit()?;
            return Ok(None);
        };
        if bytes > 8192 {
            bail!("Original publication metadata exceeds its 8 KiB UTF8 recovery budget");
        }
        let row = transaction
            .query_row(
                &format!("SELECT attempt_id,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND pr_node=?3 AND attempt_id=?4 AND observed_head=?5 AND payload_sha256=?6 AND {bytes_sql}<=8192"),
                params![target.scope.host, target.scope.login, target.pr_node_id, attempt, reference.observed_head, reference.payload_sha256],
                read_marker,
            )
            .optional()?
            .context("Exact original metadata changed during recovery; no state was released")?;
        if let Some(encoded) = &row.8 {
            let value: serde_json::Value = serde_json::from_str(encoded)
                .context("Invalid original confirmation receipt JSON")?;
            let object = value
                .as_object()
                .context("Original receipt must be a closed object")?;
            if object.len() != 3
                || !["kind", "resource_id", "node_id"]
                    .iter()
                    .all(|field| object.contains_key(*field))
            {
                bail!("Original confirmation receipt has unknown or missing metadata fields");
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
            bail!(
                "Recorded original attempt belongs to a different canonical repository/PR target"
            );
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
            | (
                PrWriteState::Pending | PrWriteState::Uncertain | PrWriteState::Acknowledged,
                Some(_),
            ) => {
                bail!("Recorded original state and canonical confirmation receipt disagree");
            }
            (
                PrWriteState::Pending | PrWriteState::Uncertain | PrWriteState::Acknowledged,
                None,
            ) => {}
        }
        transaction.commit()?;
        Ok(Some(marker))
    }
}
pub(super) fn full_hex(value: &str, lengths: &[usize]) -> bool {
    lengths.contains(&value.len()) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
#[path = "pr_write_original_tests.rs"]
mod tests;
