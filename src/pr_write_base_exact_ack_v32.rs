//! Closed Base ACK profile of the existing exact raw CAS kernel.
use super::*;
impl ExactAckRowV17 {
    /// ONLY the genuine closed Metadata journal witness, before final UID.
    pub(in crate::pr_write_journal) fn metadata_base_v32(
        journal: &PrWriteJournal,
        witness: &metadata_base_origin_v32::history_v32::BaseRecoveryWitnessV32,
    ) -> Result<Self> {
        journal.validate_base_inspection_v32(witness)?;
        let database = DatabaseV11::capture(&journal.path)?;
        let tx = journal.connection.unchecked_transaction()?;
        let marker = &witness.marker;
        if generation(&tx, &witness.target)? != witness.generation {
            bail!("Metadata ACK generation moved");
        }
        let bytes: usize = tx.query_row(
            &format!("SELECT {ROW_BYTES} FROM pr_write_attempts WHERE attempt_id=?1"),
            [marker.attempt_id],
            |r| r.get(0),
        )?;
        if bytes > 8192 {
            bail!("Exact Metadata ACK row exceeds8KiB before copy");
        }
        let raw = tx.query_row("SELECT attempt_id,host,principal,pr_node,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at FROM pr_write_attempts WHERE attempt_id=?1",[marker.attempt_id],|r|Ok(RawAttemptV17 {
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
            || raw.created != marker.created_at
            || raw.state != state_name(marker.state)
            || raw.receipt.is_some()
            || !matches!(
                marker.state,
                PrWriteState::Pending | PrWriteState::Uncertain
            )
        {
            bail!("Exact raw Metadata ACK attempt differs");
        }
        let bytes:usize=tx.query_row("SELECT length(CAST(origin_json AS BLOB)) FROM pr_write_metadata_base_origins_v32 WHERE attempt_id=?1",[raw.id],|r|r.get(0))?;
        if bytes > 16 * 1024 {
            bail!("Exact Metadata origin exceeds16KiB before copy");
        }
        let origin=tx.query_row("SELECT repository_id,pr_id,origin_json FROM pr_write_metadata_base_origins_v32 WHERE attempt_id=?1",[raw.id],|r|Ok(MetadataOriginV25 {repository:r.get(0)?,pr:r.get(1)?,json:r.get(2)?,base_v48:None,action_v48:None,parent_v48:None}))?;
        if origin.json != review_operation_history_v3::bounded_encode(&witness.origin)?
            || !witness
                .origin
                .matches_index_v32(origin.repository, origin.pr)
        {
            bail!("Exact raw Metadata origin differs");
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
            base_v32: Some(origin),
            state_v74: None,
            issue_v87: None,
        };
        row.validate_current(journal)?;
        Ok(row)
    }
}
