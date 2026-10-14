//! Bounded candidate scan; callers supply a coherent read/write snapshot.
use super::*;

pub(super) fn blocking(
    connection: &Connection,
    target: &ActionsWriteTarget,
    kind: ActionsWriteKind,
) -> Result<Vec<ActionsWriteMarker>> {
    const MAX_CANDIDATES: usize = 256;
    let provider = availability::validate_scope(&target.scope)?;
    let mut ids = BTreeSet::new();
    for key in &target.resources {
        let mut statement = connection.prepare("SELECT a.attempt_id FROM actions_write_resources r JOIN actions_write_attempts a ON a.attempt_id=r.attempt_id WHERE r.host=?1 AND r.principal=?2 AND r.resource_key=?3 AND a.state IN('pending','uncertain','accepted_pending') ORDER BY r.attempt_id DESC LIMIT 257")?;
        for row in statement
            .query_map(params![target.scope.host, target.scope.login, key], |row| {
                row.get::<_, i64>(0)
            })?
        {
            ids.insert(row?);
            if ids.len() > MAX_CANDIDATES {
                bail!("Actions candidate read budget exceeded (256 distinct unresolved candidates); no complete overlap decision or new publication is authorized");
            }
        }
    }
    let mut markers = Vec::new();
    for id in ids {
        let bytes: i64 = connection.query_row("SELECT length(CAST(target_json AS BLOB))+length(CAST(resources_json AS BLOB))+length(CAST(kind_json AS BLOB))+length(CAST(context_json AS BLOB))+length(CAST(options_json AS BLOB))+length(CAST(fingerprint_sha256 AS BLOB))+COALESCE(length(CAST(receipt_json AS BLOB)),0) FROM actions_write_attempts WHERE host=?1 AND principal=?2 AND attempt_id=?3",params![target.scope.host,target.scope.login,id],|row|row.get(0))?;
        if id <= 0 || !(1..=128 * 1024).contains(&bytes) {
            bail!("Actions candidate exceeds its bounded metadata budget; no overlap decision or new publication is authorized");
        }
        let marker = read_attempt(connection, &target.scope, id)?
            .context("Original Actions candidate changed during its coherent snapshot")?;
        availability::validate_saved_marker(&provider, &marker)?;
        if marker.blocks_target(target, kind) {
            markers.push(marker);
        }
    }
    Ok(markers)
}
