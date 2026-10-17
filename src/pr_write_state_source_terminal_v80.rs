//! Restriction-only exact persisted terminal validation for a NEW genuine
//! complete-source read. This never issues a Source/Base/write/ACK/receipt.
use super::*;
pub(crate) enum StateSourceTerminalV80 {
    ConfirmedEffect,
    AcknowledgedNoEffect,
}
pub(crate) fn validate_source_terminal_v80(
    connection: &Connection,
    database: &Path,
    scope: &Scope,
    node: &str,
    ordinal: i64,
) -> Result<StateSourceTerminalV80> {
    let (bytes,typed):(usize,bool)=connection.query_row(
        "SELECT length(CAST(host AS BLOB))+length(CAST(principal AS BLOB))+length(CAST(pr_node AS BLOB))+length(CAST(repository AS BLOB))+length(CAST(command AS BLOB))+length(CAST(selected_resource AS BLOB))+length(CAST(observed_head AS BLOB))+length(CAST(payload_sha256 AS BLOB))+length(CAST(state AS BLOB))+COALESCE(length(CAST(confirmed_json AS BLOB)),0)+24,typeof(attempt_id)='integer' AND attempt_id>0 AND typeof(host)='text' AND typeof(principal)='text' AND typeof(pr_node)='text' AND typeof(repository)='text' AND typeof(pr_number)='integer' AND pr_number>0 AND typeof(command)='text' AND typeof(selected_resource)='text' AND typeof(observed_head)='text' AND typeof(payload_sha256)='text' AND typeof(state)='text' AND (typeof(confirmed_json)='null' OR typeof(confirmed_json)='text') AND typeof(created_at)='integer' AND created_at>=0 FROM pr_write_attempts WHERE attempt_id=?1 AND host=?2 AND principal=?3 AND pr_node=?4",
        params![ordinal,scope.host,scope.login,node],|r|Ok((r.get(0)?,r.get(1)?)))?;
    if bytes > 8192 || !typed {
        bail!("State source terminal exceeds exact typed metadata bounds");
    }
    let latest:Option<i64>=connection.query_row("SELECT MAX(a.attempt_id) FROM pr_write_attempts a WHERE a.host=?1 AND a.principal=?2 AND a.pr_node=?3 AND (a.command LIKE 'state-%' OR a.selected_resource LIKE 'state-v68/%' OR EXISTS(SELECT 1 FROM pr_write_state_origins_v68 o WHERE o.attempt_id=a.attempt_id))",params![scope.host,scope.login,node],|r|r.get(0))?;
    let (repository, number): (String, u64) = connection.query_row(
        "SELECT repository,pr_number FROM pr_write_attempts WHERE attempt_id=?1",
        [ordinal],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let target = PrWriteTarget::for_source(scope, &repository, node, number)?;
    let row=connection.query_row("SELECT attempt_id,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at FROM pr_write_attempts WHERE attempt_id=?1",[ordinal],read_marker)?;
    let marker = decode_marker(&target, row)?;
    // The existing strict decoder validates raw index/nullable predecessor,
    // original protected key/digest/reference and typed effect/disposition.
    read_origin_v68(connection, database, &target, &marker)?;
    if latest != Some(ordinal) {
        bail!("Newer or malformed State attempt blocks source hydration");
    }
    match marker.state {
        PrWriteState::Confirmed => Ok(StateSourceTerminalV80::ConfirmedEffect),
        PrWriteState::Acknowledged if marker.confirmed.is_none() => {
            Ok(StateSourceTerminalV80::AcknowledgedNoEffect)
        }
        _ => bail!("Unresolved State attempt cannot issue a source watermark"),
    }
}
