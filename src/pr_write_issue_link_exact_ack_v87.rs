//! One closed Issue profile of the SAME exact-ACK Immediate transaction.
//! All bytes are captured from its genuine journal witness before final UID.
use super::*;
use issue_link_origin_v87::history_v87::IssueLinkRecoveryWitnessV87;

#[derive(Debug)]
pub(super) struct RawIssueOriginV87 {
    host: String,
    principal: String,
    node: String,
    id: u64,
    repository: u64,
    json: String,
    base: String,
    action: String,
    parent: Option<i64>,
}
impl ExactAckRowV17 {
    pub(in crate::pr_write_journal) fn issue_link_v87(
        journal: &PrWriteJournal,
        witness: &IssueLinkRecoveryWitnessV87,
    ) -> Result<Self> {
        journal.validate_issue_link_inspection_v87(witness)?;
        let database = DatabaseV11::capture(&journal.path)?;
        let tx = journal.connection.unchecked_transaction()?;
        if generation(&tx, &witness.target)? != witness.generation {
            bail!("Issue ACK generation changed before raw capture");
        }
        let bytes: usize = tx.query_row(
            &format!("SELECT {ROW_BYTES} FROM pr_write_attempts WHERE attempt_id=?1"),
            [witness.marker.attempt_id],
            |r| r.get(0),
        )?;
        if bytes > 8192 {
            bail!("Issue ACK raw attempt exceeds8KiB before copy");
        }
        let raw=tx.query_row("SELECT attempt_id,host,principal,pr_node,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at FROM pr_write_attempts WHERE attempt_id=?1",[witness.marker.attempt_id],|r|Ok(RawAttemptV17 {
            id:r.get(0)?,host:r.get(1)?,principal:r.get(2)?,pr_node:r.get(3)?,repository:r.get(4)?,number:r.get(5)?,command:r.get(6)?,selected:r.get(7)?,head:r.get(8)?,payload:r.get(9)?,state:r.get(10)?,receipt:r.get(11)?,created:r.get(12)?,
        }))?;
        let marker = &witness.marker;
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
            bail!("Exact raw Issue ACK row differs from genuine witness");
        }
        // load_record already checked scalar typeof/length/opposite-origin
        // predicates before parsing; repeat inside this capture transaction.
        let current =
            issue_link_origin_v87::load_record(&tx, &journal.path, &witness.target, marker)?;
        if review_operation_history_v3::bounded_encode(&current)?
            != review_operation_history_v3::bounded_encode(&witness.origin)?
        {
            bail!("Issue origin changed during exact ACK capture");
        }
        let origin=tx.query_row("SELECT host,principal,issue_node,issue_id,issue_repository_id,origin_json,base_key,action_tag,parent_ack_attempt FROM pr_write_issue_link_origins_v87 WHERE attempt_id=?1",[raw.id],|r|Ok(RawIssueOriginV87 {
            host:r.get(0)?,principal:r.get(1)?,node:r.get(2)?,id:r.get(3)?,repository:r.get(4)?,json:r.get(5)?,base:r.get(6)?,action:r.get(7)?,parent:r.get(8)?,
        }))?;
        if origin.json != review_operation_history_v3::bounded_encode(&witness.origin)? {
            bail!("Issue exact ACK serialization changed");
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
            state_v74: None,
            issue_v87: Some(origin),
        };
        row.validate_current(journal)?;
        Ok(row)
    }
    pub(super) fn match_issue_link_v87(
        &self,
        connection: &Connection,
        update: bool,
        origin: &RawIssueOriginV87,
    ) -> Result<usize> {
        let r = &self.attempt;
        // Literal schema and purpose, not a caller-supplied predicate. Both
        // original tables' typeof/BLOB/NULL equality and secondary latest
        // Issue barrier are IN the same UPDATE as the original thirteen cols.
        let sql="SELECT count(*) FROM pr_write_attempts a JOIN pr_write_issue_link_origins_v87 o ON o.attempt_id=a.attempt_id WHERE
            typeof(a.attempt_id)='integer' AND a.attempt_id=?1 AND typeof(a.host)='text' AND CAST(a.host AS BLOB)=CAST(?2 AS BLOB)
            AND typeof(a.principal)='text' AND CAST(a.principal AS BLOB)=CAST(?3 AS BLOB)
            AND typeof(a.pr_node)='text' AND CAST(a.pr_node AS BLOB)=CAST(?4 AS BLOB)
            AND typeof(a.repository)='text' AND CAST(a.repository AS BLOB)=CAST(?5 AS BLOB)
            AND typeof(a.pr_number)='integer' AND a.pr_number=?6
            AND typeof(a.command)='text' AND CAST(a.command AS BLOB)=CAST(?7 AS BLOB)
            AND a.command IN('issue-link-v87','issue-unlink-manual-v87')
            AND typeof(a.selected_resource)='text' AND CAST(a.selected_resource AS BLOB)=CAST(?8 AS BLOB)
            AND typeof(a.observed_head)='text' AND CAST(a.observed_head AS BLOB)=CAST(?9 AS BLOB)
            AND typeof(a.payload_sha256)='text' AND CAST(a.payload_sha256 AS BLOB)=CAST(?10 AS BLOB)
            AND typeof(a.state)='text' AND CAST(a.state AS BLOB)=CAST(?11 AS BLOB) AND a.state IN('pending','uncertain')
            AND typeof(a.confirmed_json)='null' AND CAST(a.confirmed_json AS BLOB) IS CAST(?12 AS BLOB)
            AND typeof(a.created_at)='integer' AND a.created_at=?13
            AND typeof(o.host)='text' AND CAST(o.host AS BLOB)=CAST(?14 AS BLOB)
            AND typeof(o.principal)='text' AND CAST(o.principal AS BLOB)=CAST(?15 AS BLOB)
            AND typeof(o.issue_node)='text' AND CAST(o.issue_node AS BLOB)=CAST(?16 AS BLOB)
            AND typeof(o.issue_id)='integer' AND o.issue_id=?17
            AND typeof(o.issue_repository_id)='integer' AND o.issue_repository_id=?18
            AND typeof(o.origin_json)='text' AND CAST(o.origin_json AS BLOB)=CAST(?19 AS BLOB)
            AND typeof(o.base_key)='text' AND CAST(o.base_key AS BLOB)=CAST(?20 AS BLOB)
            AND typeof(o.action_tag)='text' AND CAST(o.action_tag AS BLOB)=CAST(?21 AS BLOB)
            AND typeof(o.parent_ack_attempt)=typeof(?22) AND o.parent_ack_attempt IS ?22
            AND (SELECT count(*) FROM pr_write_attempts WHERE host=?2 AND principal=?3 AND pr_node=?4 AND state IN('pending','uncertain'))=1
            AND (SELECT MAX(attempt_id) FROM pr_write_issue_link_origins_v87 WHERE host=?14 AND principal=?15 AND issue_node=?16)=?1
            AND (SELECT count(*) FROM pr_write_issue_link_origins_v87 s JOIN pr_write_attempts b ON b.attempt_id=s.attempt_id WHERE s.host=?14 AND s.principal=?15 AND s.issue_node=?16 AND b.state IN('pending','uncertain'))=1
            AND NOT EXISTS(SELECT 1 FROM pr_write_review_operation_origins_v3 WHERE attempt_id=a.attempt_id)
            AND NOT EXISTS(SELECT 1 FROM pr_write_conversation_origins_v17 WHERE attempt_id=a.attempt_id)
            AND NOT EXISTS(SELECT 1 FROM pr_write_metadata_origins_v25 WHERE attempt_id=a.attempt_id)
            AND NOT EXISTS(SELECT 1 FROM pr_write_metadata_base_origins_v32 WHERE attempt_id=a.attempt_id)
            AND NOT EXISTS(SELECT 1 FROM pr_write_thread_origins_v1 WHERE attempt_id=a.attempt_id)
            AND NOT EXISTS(SELECT 1 FROM pr_write_thread_comment_origins_v2 WHERE attempt_id=a.attempt_id)
            AND NOT EXISTS(SELECT 1 FROM pr_write_thread_reaction_origins_v3 WHERE attempt_id=a.attempt_id)
            AND NOT EXISTS(SELECT 1 FROM pr_write_file_comment_origins_v75 WHERE attempt_id=a.attempt_id)
            AND NOT EXISTS(SELECT 1 FROM pr_write_state_origins_v68 WHERE attempt_id=a.attempt_id)
            AND NOT EXISTS(SELECT 1 FROM pr_write_state_dispositions_v68 WHERE attempt_id=a.attempt_id)";
        let values = params![
            r.id,
            r.host,
            r.principal,
            r.pr_node,
            r.repository,
            r.number,
            r.command,
            r.selected,
            r.head,
            r.payload,
            r.state,
            r.receipt,
            r.created,
            origin.host,
            origin.principal,
            origin.node,
            origin.id,
            origin.repository,
            origin.json,
            origin.base,
            origin.action,
            origin.parent
        ];
        if update {
            let selected = sql.replacen("SELECT count(*)", "SELECT a.attempt_id", 1);
            Ok(connection.execute(&format!("UPDATE pr_write_attempts SET state='acknowledged' WHERE attempt_id IN({selected})"),values)?)
        } else {
            Ok(connection.query_row(sql, values, |r| r.get(0))?)
        }
    }
}
