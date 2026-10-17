//! Closed State profile of the SAME exact raw ACK CAS; no second transition.
use super::*;
impl ExactAckRowV17 {
    pub(in crate::pr_write_journal) fn state_v74(
        journal: &PrWriteJournal,
        witness: &state_origin_v68::history_v74::StateRecoveryWitnessV74,
        original_cache: std::sync::Arc<crate::storage::StateAckQueueBoundaryV78>,
    ) -> Result<Self> {
        journal.validate_state_inspection_v74(witness)?;
        let database = DatabaseV11::capture(&journal.path)?;
        let tx = journal.connection.unchecked_transaction()?;
        original_cache.validate(&tx)?;
        let marker = &witness.marker;
        if generation(&tx, &witness.target)? != witness.generation {
            bail!("State ACK generation changed");
        }
        let bytes: usize = tx.query_row(
            &format!("SELECT {ROW_BYTES} FROM pr_write_attempts WHERE attempt_id=?1"),
            [marker.attempt_id],
            |r| r.get(0),
        )?;
        if bytes > 8192 {
            bail!("State raw ACK row exceeds8KiB before copy");
        }
        let raw=tx.query_row("SELECT attempt_id,host,principal,pr_node,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at FROM pr_write_attempts WHERE attempt_id=?1",[marker.attempt_id],|r|Ok(RawAttemptV17 {
            id:r.get(0)?,host:r.get(1)?,principal:r.get(2)?,pr_node:r.get(3)?,repository:r.get(4)?,number:r.get(5)?,command:r.get(6)?,selected:r.get(7)?,head:r.get(8)?,payload:r.get(9)?,state:r.get(10)?,receipt:r.get(11)?,created:r.get(12)?,
        }))?;
        if raw.host != witness.target.scope.host
            || raw.principal != witness.target.scope.login
            || raw.pr_node != witness.target.pr_node_id
            || raw.repository != witness.target.repository
            || raw.number != witness.target.number
            || raw.command != marker.command
            || raw.selected != marker.selected_resource
            || raw.head != marker.observed_head
            || raw.payload != marker.payload_sha256
            || raw.state != state_name(marker.state)
            || raw.receipt.is_some()
            || raw.created != marker.created_at
            || !matches!(
                marker.state,
                PrWriteState::Pending | PrWriteState::Uncertain
            )
        {
            bail!("Exact raw State blocker differs from genuine journal witness");
        }
        // state_origin_v68 already proved strict raw types, all opposite
        // purposes absent and <=16KiB origin. Recheck before raw copies.
        let bounded:bool=tx.query_row("SELECT typeof(repository_id)='integer' AND typeof(pr_id)='integer' AND typeof(issue_id)='integer' AND typeof(command)='text' AND length(CAST(command AS BLOB))<=64 AND typeof(origin_json)='text' AND length(CAST(origin_json AS BLOB))<=16384 AND typeof(base_key_v71)='text' AND length(CAST(base_key_v71 AS BLOB))<=256 AND typeof(action_tag_v71)='text' AND length(CAST(action_tag_v71 AS BLOB))<=64 AND (typeof(parent_ack_attempt_v71)='null' OR(typeof(parent_ack_attempt_v71)='integer' AND parent_ack_attempt_v71>0)) FROM pr_write_state_origins_v68 WHERE attempt_id=?1",[raw.id],|r|r.get(0))?;
        if !bounded {
            bail!("State ACK origin scalar bound/type differs");
        }
        let cache_epoch = crate::storage::StateAckCacheEpochV77::capture(&tx, witness)?;
        let origin=tx.query_row("SELECT repository_id,pr_id,issue_id,command,origin_json,base_key_v71,action_tag_v71,parent_ack_attempt_v71 FROM pr_write_state_origins_v68 WHERE attempt_id=?1",[raw.id],|r|Ok(StateAckOriginV74 {repository:r.get(0)?,pr:r.get(1)?,issue:r.get(2)?,command:r.get(3)?,json:r.get(4)?,base:r.get(5)?,action:r.get(6)?,parent_ack:r.get(7)?,cache_epoch,original_cache}))?;
        if origin.json != review_operation_history_v3::bounded_encode(&witness.origin)? {
            bail!("Exact raw State origin/index differs");
        }
        tx.commit()?;
        let row = Self {
            database,
            issuer: journal.path.clone(),
            target: witness.target.clone(),
            generation: witness.generation,
            attempt: raw,
            origin: None,
            conversation: None,
            metadata: None,
            base_v32: None,
            state_v74: Some(origin),
            issue_v87: None,
        };
        row.validate_current(journal)?;
        Ok(row)
    }
}
