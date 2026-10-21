//! ONE exact local ACK commit. Only genuine purpose-specific journal witnesses
//! issue this nonClone row. No generic origin callback or publication permit.
use super::*;
use review_recovery_v11::{DatabaseV11, ReviewRecoveryJournalWitnessV11};
use std::time::Instant;
#[path = "pr_write_issue_link_exact_ack_v87.rs"]
mod issue_link_v87;
#[path = "pr_write_base_exact_ack_v32.rs"]
mod metadata_base_v32;
#[path = "pr_write_state_exact_ack_v74.rs"]
mod state_v74;

#[derive(Debug)]
struct RawAttemptV17 {
    id: i64,
    host: String,
    principal: String,
    pr_node: String,
    repository: String,
    number: u64,
    command: String,
    selected: String,
    head: String,
    payload: String,
    state: String,
    receipt: Option<String>,
    created: u64,
}
#[derive(Debug)]
struct ReviewOriginV17 {
    repository: u64,
    repository_node: String,
    pr: u64,
    json: String,
    base_key: Option<String>,
    action: Option<String>,
    parent_ack: Option<i64>,
}
#[derive(Debug)]
struct ConversationOriginV17 {
    repository: u64,
    pr: u64,
    json: String,
    base_key: Option<String>,
    action: Option<String>,
    parent_ack: Option<i64>,
}
#[derive(Debug)]
struct MetadataOriginV25 {
    repository: u64,
    pr: u64,
    json: String,
    base_v48: Option<String>,
    action_v48: Option<String>,
    parent_v48: Option<i64>,
}
#[derive(Debug)]
struct StateAckOriginV74 {
    repository: u64,
    pr: u64,
    issue: u64,
    command: String,
    json: String,
    base: String,
    action: String,
    parent_ack: Option<i64>,
    cache_epoch: crate::storage::StateAckCacheEpochV77,
    original_cache: std::sync::Arc<crate::storage::StateAckQueueBoundaryV78>,
}
#[derive(Debug)]
pub(super) struct ExactAckRowV17 {
    database: DatabaseV11,
    issuer: PathBuf,
    target: PrWriteTarget,
    generation: i64,
    attempt: RawAttemptV17,
    origin: Option<ReviewOriginV17>,
    conversation: Option<ConversationOriginV17>,
    metadata: Option<MetadataOriginV25>,
    base_v32: Option<MetadataOriginV25>,
    state_v74: Option<StateAckOriginV74>,
    issue_v87: Option<issue_link_v87::RawIssueOriginV87>,
}
#[derive(Clone, Copy)]
enum MetadataAckWitnessV48<'a> {
    Text(&'a metadata_origin_v25::history_v25::MetadataRecoveryWitnessV25),
    Members(&'a metadata_members_v48::history_v48::MetadataMembersRecoveryWitnessV48),
}
const ROW_BYTES: &str = "length(CAST(host AS BLOB))+length(CAST(principal AS BLOB))+length(CAST(pr_node AS BLOB))+length(CAST(repository AS BLOB))+length(CAST(command AS BLOB))+length(CAST(selected_resource AS BLOB))+length(CAST(observed_head AS BLOB))+length(CAST(payload_sha256 AS BLOB))+length(CAST(state AS BLOB))+COALESCE(length(CAST(confirmed_json AS BLOB)),0)";
impl ExactAckRowV17 {
    /// Actual Review witness only; no target/ref constructor or deserialize.
    /// Called by prepare_review_inspection BEFORE the original final UID.
    pub(super) fn review(
        journal: &PrWriteJournal,
        witness: &ReviewRecoveryJournalWitnessV11,
    ) -> Result<Self> {
        journal.validate_review_inspection_v11(witness)?;
        let target = witness.target();
        let marker = witness.marker();
        let database = DatabaseV11::capture(&journal.path)?;
        let tx = journal.connection.unchecked_transaction()?;
        if generation(&tx, target)? != witness.generation_v17() {
            bail!("Original Review generation changed before raw ACK capture");
        }
        let bytes: usize = tx.query_row(
            &format!("SELECT {ROW_BYTES} FROM pr_write_attempts WHERE attempt_id=?1"),
            [marker.attempt_id],
            |r| r.get(0),
        )?;
        if bytes > 8192 {
            bail!("Exact ACK row exceeds8KiB before materialization");
        }
        let raw = tx.query_row("SELECT attempt_id,host,principal,pr_node,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at FROM pr_write_attempts WHERE attempt_id=?1", [marker.attempt_id], |r| Ok(RawAttemptV17 {
            id:r.get(0)?,host:r.get(1)?,principal:r.get(2)?,pr_node:r.get(3)?,repository:r.get(4)?,number:r.get(5)?,command:r.get(6)?,selected:r.get(7)?,head:r.get(8)?,payload:r.get(9)?,state:r.get(10)?,receipt:r.get(11)?,created:r.get(12)?,
        }))?;
        if raw.host != target.scope.host
            || raw.principal != target.scope.login
            || raw.pr_node != target.pr_node_id
            || raw.repository != target.repository
            || raw.number != target.number
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
            bail!("Exact raw Review ACK row differs from genuine typed witness");
        }
        let origin_bytes: Option<(usize, usize, usize, usize)> = tx.query_row(
            "SELECT length(CAST(origin_json AS BLOB)),length(CAST(repository_node AS BLOB)),COALESCE(length(CAST(base_key_v12 AS BLOB)),0),COALESCE(length(CAST(action_tag_v12 AS BLOB)),0) FROM pr_write_review_operation_origins_v3 WHERE attempt_id=?1",
            [raw.id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
        if origin_bytes.is_some_and(|(json, node, key, action)| {
            json > 16384 || node > 1024 || key > 1024 || action > 128
        }) {
            bail!("Exact ACK original metadata exceeds pre-materialization bounds");
        }
        let origin: Option<ReviewOriginV17> = tx.query_row(
            "SELECT repository_id,repository_node,pr_id,origin_json,base_key_v12,action_tag_v12,parent_ack_attempt_v12 FROM pr_write_review_operation_origins_v3 WHERE attempt_id=?1",
            [raw.id], |r| Ok(ReviewOriginV17 { repository:r.get(0)?, repository_node:r.get(1)?,pr:r.get(2)?,json:r.get(3)?,base_key:r.get(4)?,action:r.get(5)?,parent_ack:r.get(6)? })).optional()?;
        if let Some(origin) = &origin {
            let record = review_action_lineage_v12::ReviewObservedOriginV12::decode(
                &origin.json,
                origin.base_key.is_some(),
                &journal.path,
                target,
                marker,
                origin.repository,
                &origin.repository_node,
                origin.pr,
            )?;
            record.validate_ack_index_v17(
                &tx,
                target,
                marker.attempt_id,
                origin.base_key.as_deref(),
                origin.action.as_deref(),
                origin.parent_ack,
            )?;
        }
        if generation(&tx, target)? != witness.generation_v17() {
            bail!("Review generation changed during exact ACK capture");
        }
        tx.commit()?;
        if DatabaseV11::capture(&journal.path)? != database {
            bail!("Original ACK database replaced during capture");
        }
        Ok(Self {
            database,
            issuer: journal.path.clone(),
            target: target.clone(),
            generation: witness.generation_v17(),
            attempt: raw,
            origin,
            conversation: None,
            metadata: None,
            base_v32: None,
            state_v74: None,
            issue_v87: None,
        })
    }
    /// ONLY the actual closed Conversation journal witness may issue this
    /// second profile, after purpose/origin/blocker validation BEFORE UID.
    pub(super) fn conversation(
        journal: &PrWriteJournal,
        witness: &conversation_history_v17::ConversationRecoveryWitnessV17,
    ) -> Result<Self> {
        journal.validate_conversation_inspection_v17(witness)?;
        let database = DatabaseV11::capture(&journal.path)?;
        let tx = journal.connection.unchecked_transaction()?;
        let marker = &witness.marker;
        if generation(&tx, &witness.target)? != witness.generation {
            bail!("Conversation ACK generation moved");
        }
        let bytes: usize = tx.query_row(
            &format!("SELECT {ROW_BYTES} FROM pr_write_attempts WHERE attempt_id=?1"),
            [marker.attempt_id],
            |r| r.get(0),
        )?;
        if bytes > 8192 {
            bail!("Exact Conversation ACK row exceeds8KiB before materialization");
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
            || raw.created != marker.created_at
            || raw.state != state_name(marker.state)
            || raw.receipt.is_some()
            || !matches!(
                marker.state,
                PrWriteState::Pending | PrWriteState::Uncertain
            )
        {
            bail!("Exact raw Conversation ACK row differs from genuine witness");
        }
        let bytes:(usize,usize,usize)=tx.query_row("SELECT length(CAST(origin_json AS BLOB)),COALESCE(length(CAST(base_key_v18 AS BLOB)),0),COALESCE(length(CAST(action_tag_v18 AS BLOB)),0) FROM pr_write_conversation_origins_v17 WHERE attempt_id=?1",[raw.id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        if bytes.0 > 16384 || bytes.1 > 256 || bytes.2 > 128 {
            bail!("Exact Conversation origin exceeds16KiB before materialization");
        }
        let origin=tx.query_row("SELECT repository_id,pr_id,origin_json,base_key_v18,action_tag_v18,parent_ack_attempt_v18 FROM pr_write_conversation_origins_v17 WHERE attempt_id=?1",[raw.id],|r|Ok(ConversationOriginV17 { repository:r.get(0)?,pr:r.get(1)?,json:r.get(2)?,base_key:r.get(3)?,action:r.get(4)?,parent_ack:r.get(5)? }))?;
        conversation_action_lineage_v18::ObservedConversationOriginV18::decode(
            &origin.json,
            &journal.path,
            &witness.target,
            marker,
            origin.repository,
            origin.pr,
            conversation_action_lineage_v18::ConversationLineageIndexV18 {
                base: origin.base_key.as_deref(),
                action: origin.action.as_deref(),
                parent_ack: origin.parent_ack,
            },
        )?;
        if origin.json != review_operation_history_v3::bounded_encode(&witness.origin)?
            || origin.repository != witness.origin.seed().repository_id
            || origin.pr != witness.origin.seed().pr_id
        {
            bail!("Conversation ACK raw origin/index differs");
        }
        if tx.query_row(
            "SELECT count(*) FROM pr_write_review_operation_origins_v3 WHERE attempt_id=?1",
            [raw.id],
            |r| r.get::<_, usize>(0),
        )? != 0
        {
            bail!("Mixed Review/Conversation origin cannot authorize ACK");
        }
        tx.commit()?;
        let row = Self {
            database,
            issuer: journal.path.clone(),
            target: witness.target.clone(),
            generation: witness.generation,
            attempt: raw,
            origin: None,
            conversation: Some(origin),
            metadata: None,
            base_v32: None,
            state_v74: None,
            issue_v87: None,
        };
        row.validate_current(journal)?;
        Ok(row)
    }
    /// ONLY actual Text/Members journal witnesses; no caller supplied raw row.
    pub(super) fn metadata(
        journal: &PrWriteJournal,
        witness: &metadata_origin_v25::history_v25::MetadataRecoveryWitnessV25,
    ) -> Result<Self> {
        Self::capture_metadata_v48(journal, MetadataAckWitnessV48::Text(witness))
    }
    pub(super) fn metadata_members_v48(
        journal: &PrWriteJournal,
        witness: &metadata_members_v48::history_v48::MetadataMembersRecoveryWitnessV48,
    ) -> Result<Self> {
        Self::capture_metadata_v48(journal, MetadataAckWitnessV48::Members(witness))
    }
    fn capture_metadata_v48(
        journal: &PrWriteJournal,
        witness: MetadataAckWitnessV48<'_>,
    ) -> Result<Self> {
        let (
            target,
            marker,
            expected_generation,
            expected_origin,
            repository,
            pr,
            base,
            action,
            parent,
        ) = match witness {
            MetadataAckWitnessV48::Text(witness) => {
                journal.validate_metadata_inspection_v25(witness)?;
                (
                    &witness.target,
                    &witness.marker,
                    witness.generation,
                    review_operation_history_v3::bounded_encode(&witness.origin)?,
                    witness.origin.seed.repository_id,
                    witness.origin.seed.pr_id,
                    None,
                    None,
                    None,
                )
            }
            MetadataAckWitnessV48::Members(witness) => {
                journal.validate_metadata_members_inspection_v48(witness)?;
                let (repository, pr, base, action, parent) = witness.origin.seed.sql_identity_v48();
                (
                    &witness.target,
                    &witness.marker,
                    witness.generation,
                    review_operation_history_v3::bounded_encode(&witness.origin)?,
                    repository,
                    pr,
                    Some(base),
                    Some(action),
                    parent,
                )
            }
        };
        let database = DatabaseV11::capture(&journal.path)?;
        let tx = journal.connection.unchecked_transaction()?;
        if generation(&tx, target)? != expected_generation {
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
        if raw.host != target.scope.host
            || raw.principal != target.scope.login
            || raw.pr_node != target.pr_node_id
            || raw.repository != target.repository
            || raw.number != target.number
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
        let bytes:usize=tx.query_row("SELECT length(CAST(origin_json AS BLOB)) FROM pr_write_metadata_origins_v25 WHERE attempt_id=?1",[raw.id],|r|r.get(0))?;
        if bytes > 16 * 1024 {
            bail!("Exact Metadata origin exceeds16KiB before copy");
        }
        let origin=match witness {
            MetadataAckWitnessV48::Text(_)=>{
                metadata_members_v48::require_text_null_v48(&tx, raw.id)?;
        tx.query_row("SELECT repository_id,pr_id,origin_json FROM pr_write_metadata_origins_v25 WHERE attempt_id=?1 AND typeof(repository_id)='integer' AND typeof(pr_id)='integer' AND typeof(origin_json)='text' AND length(CAST(origin_json AS BLOB))<=16384 AND typeof(base_key_v48)='null' AND typeof(action_tag_v48)='null' AND typeof(parent_ack_attempt_v48)='null'",[raw.id],|r|Ok(MetadataOriginV25 {repository:r.get(0)?,pr:r.get(1)?,json:r.get(2)?,base_v48:None,action_v48:None,parent_v48:None}))?
            }
            MetadataAckWitnessV48::Members(_)=>tx.query_row("SELECT repository_id,pr_id,origin_json,base_key_v48,action_tag_v48,parent_ack_attempt_v48 FROM pr_write_metadata_origins_v25 WHERE attempt_id=?1 AND typeof(repository_id)='integer' AND typeof(pr_id)='integer' AND typeof(origin_json)='text' AND length(CAST(origin_json AS BLOB))<=16384 AND typeof(base_key_v48)='text' AND length(CAST(base_key_v48 AS BLOB))<=128 AND typeof(action_tag_v48)='text' AND length(CAST(action_tag_v48 AS BLOB))<=64 AND (typeof(parent_ack_attempt_v48)='null' OR (typeof(parent_ack_attempt_v48)='integer' AND parent_ack_attempt_v48>0))",[raw.id],|r|Ok(MetadataOriginV25 {repository:r.get(0)?,pr:r.get(1)?,json:r.get(2)?,base_v48:r.get(3)?,action_v48:r.get(4)?,parent_v48:r.get(5)?}))?,
        };
        if origin.json != expected_origin
            || origin.repository != repository
            || origin.pr != pr
            || origin.base_v48.as_deref() != base
            || origin.action_v48.as_deref() != action
            || origin.parent_v48 != parent
        {
            bail!("Exact raw Metadata origin differs");
        }
        tx.commit()?;
        let row = Self {
            database,
            issuer: journal.path.clone(),
            target: target.clone(),
            generation: expected_generation,
            attempt: raw,
            origin: None,
            conversation: None,
            metadata: Some(origin),
            base_v32: None,
            state_v74: None,
            issue_v87: None,
        };
        row.validate_current(journal)?;
        Ok(row)
    }
    /// Byte-equality only, safe after final UID. No JSON/body read, parse or
    /// owned string materialization. NULL and absent origin are DISTINCT.
    pub(super) fn validate_current(&self, journal: &PrWriteJournal) -> Result<()> {
        if journal.path != self.issuer
            || DatabaseV11::capture(&journal.path)? != self.database
            || journal.source_generation(&self.target)? != self.generation
            || self.matches(&journal.connection)? != 1
        {
            bail!("Exact original ACK row/context/receipt changed; blocker retained");
        }
        Ok(())
    }
    fn matches(&self, connection: &Connection) -> Result<usize> {
        self.match_or_update(connection, false)
    }
    fn match_or_update(&self, connection: &Connection, update: bool) -> Result<usize> {
        if self.state_v74.is_some() && self.issue_v87.is_some() {
            return Ok(0);
        }
        if let Some(state) = &self.state_v74 {
            // This SAME method is called again INSIDE the existing Immediate
            // transaction before its exact UPDATE. Clear cannot race between
            // this restriction and State ACK; all older None profiles exact.
            state.original_cache.validate(connection)?;
            state.cache_epoch.validate(connection)?;
        }
        if let Some(origin) = &self.issue_v87 {
            return self.match_issue_link_v87(connection, update, origin);
        }
        let r = &self.attempt;
        let origin = self.origin.as_ref();
        let sql="SELECT count(*) FROM pr_write_attempts AS a WHERE a.attempt_id=?1 AND CAST(a.host AS BLOB)=CAST(?2 AS BLOB) AND CAST(a.principal AS BLOB)=CAST(?3 AS BLOB) AND CAST(a.pr_node AS BLOB)=CAST(?4 AS BLOB) AND CAST(a.repository AS BLOB)=CAST(?5 AS BLOB) AND a.pr_number=?6 AND CAST(a.command AS BLOB)=CAST(?7 AS BLOB) AND CAST(a.selected_resource AS BLOB)=CAST(?8 AS BLOB) AND CAST(a.observed_head AS BLOB)=CAST(?9 AS BLOB) AND CAST(a.payload_sha256 AS BLOB)=CAST(?10 AS BLOB) AND CAST(a.state AS BLOB)=CAST(?11 AS BLOB) AND CAST(a.confirmed_json AS BLOB) IS CAST(?12 AS BLOB) AND a.created_at=?13 AND ((?14=0 AND NOT EXISTS(SELECT 1 FROM pr_write_review_operation_origins_v3 WHERE attempt_id=a.attempt_id)) OR (?14=1 AND EXISTS(SELECT 1 FROM pr_write_review_operation_origins_v3 AS o WHERE o.attempt_id=a.attempt_id AND o.repository_id=?15 AND CAST(o.repository_node AS BLOB)=CAST(?16 AS BLOB) AND o.pr_id=?17 AND CAST(o.origin_json AS BLOB)=CAST(?18 AS BLOB) AND CAST(o.base_key_v12 AS BLOB) IS CAST(?19 AS BLOB) AND CAST(o.action_tag_v12 AS BLOB) IS CAST(?20 AS BLOB) AND o.parent_ack_attempt_v12 IS ?21))) AND (SELECT count(*) FROM pr_write_attempts WHERE host=?2 AND principal=?3 AND pr_node=?4 AND state IN('pending','uncertain'))=1";
        let sql = format!("{sql} AND ((?22=0 AND NOT EXISTS(SELECT 1 FROM pr_write_conversation_origins_v17 WHERE attempt_id=a.attempt_id)) OR (?22=1 AND EXISTS(SELECT 1 FROM pr_write_conversation_origins_v17 AS c WHERE c.attempt_id=a.attempt_id AND c.repository_id=?23 AND c.pr_id=?24 AND CAST(c.origin_json AS BLOB)=CAST(?25 AS BLOB) AND CAST(c.base_key_v18 AS BLOB) IS CAST(?26 AS BLOB) AND CAST(c.action_tag_v18 AS BLOB) IS CAST(?27 AS BLOB) AND c.parent_ack_attempt_v18 IS ?28)))");
        let conversation = self.conversation.as_ref();
        // Restriction-only foreign-purpose exclusion is repeated in the SAME
        // final Immediate CAS, not merely before the final UID. A grafted
        // State row can never be released by an older typed ACK profile.
        let sql=format!("{sql} AND ((?40=0 AND a.command NOT LIKE 'state-%' AND a.selected_resource NOT LIKE 'state-v68/%' AND NOT EXISTS(SELECT 1 FROM pr_write_state_origins_v68 AS s WHERE s.attempt_id=a.attempt_id) AND NOT EXISTS(SELECT 1 FROM pr_write_state_dispositions_v68 AS d WHERE d.attempt_id=a.attempt_id)) OR (?40=1 AND EXISTS(SELECT 1 FROM pr_write_state_origins_v68 s WHERE s.attempt_id=a.attempt_id AND typeof(s.repository_id)='integer' AND s.repository_id=?41 AND typeof(s.pr_id)='integer' AND s.pr_id=?42 AND typeof(s.issue_id)='integer' AND s.issue_id=?43 AND typeof(s.command)='text' AND CAST(s.command AS BLOB)=CAST(?44 AS BLOB) AND typeof(s.origin_json)='text' AND CAST(s.origin_json AS BLOB)=CAST(?45 AS BLOB) AND typeof(s.base_key_v71)='text' AND CAST(s.base_key_v71 AS BLOB)=CAST(?46 AS BLOB) AND typeof(s.action_tag_v71)='text' AND CAST(s.action_tag_v71 AS BLOB)=CAST(?47 AS BLOB) AND typeof(s.parent_ack_attempt_v71)=typeof(?48) AND s.parent_ack_attempt_v71 IS ?48) AND typeof(a.attempt_id)='integer' AND typeof(a.host)='text' AND typeof(a.principal)='text' AND typeof(a.pr_node)='text' AND typeof(a.repository)='text' AND typeof(a.pr_number)='integer' AND typeof(a.command)='text' AND typeof(a.selected_resource)='text' AND typeof(a.observed_head)='text' AND typeof(a.payload_sha256)='text' AND typeof(a.state)='text' AND typeof(a.confirmed_json)='null' AND typeof(a.created_at)='integer' AND NOT EXISTS(SELECT 1 FROM pr_write_state_dispositions_v68 d WHERE d.attempt_id=a.attempt_id) AND NOT EXISTS(SELECT 1 FROM pr_write_thread_origins_v1 WHERE attempt_id=a.attempt_id) AND NOT EXISTS(SELECT 1 FROM pr_write_thread_comment_origins_v2 WHERE attempt_id=a.attempt_id) AND NOT EXISTS(SELECT 1 FROM pr_write_thread_reaction_origins_v3 WHERE attempt_id=a.attempt_id) AND NOT EXISTS(SELECT 1 FROM pr_write_file_comment_origins_v75 WHERE attempt_id=a.attempt_id)))");
        let sql=format!("{sql} AND ((?29=0 AND NOT EXISTS(SELECT 1 FROM pr_write_metadata_origins_v25 WHERE attempt_id=a.attempt_id)) OR (?29=1 AND EXISTS(SELECT 1 FROM pr_write_metadata_origins_v25 AS m WHERE m.attempt_id=a.attempt_id AND typeof(m.repository_id)='integer' AND typeof(m.pr_id)='integer' AND typeof(m.origin_json)='text' AND m.repository_id=?30 AND m.pr_id=?31 AND CAST(m.origin_json AS BLOB)=CAST(?32 AS BLOB) AND typeof(m.base_key_v48)=typeof(?37) AND CAST(m.base_key_v48 AS BLOB) IS CAST(?37 AS BLOB) AND typeof(m.action_tag_v48)=typeof(?38) AND CAST(m.action_tag_v48 AS BLOB) IS CAST(?38 AS BLOB) AND typeof(m.parent_ack_attempt_v48)=typeof(?39) AND m.parent_ack_attempt_v48 IS ?39) AND typeof(a.attempt_id)='integer' AND typeof(a.host)='text' AND typeof(a.principal)='text' AND typeof(a.pr_node)='text' AND typeof(a.repository)='text' AND typeof(a.pr_number)='integer' AND typeof(a.command)='text' AND typeof(a.selected_resource)='text' AND typeof(a.observed_head)='text' AND typeof(a.payload_sha256)='text' AND typeof(a.state)='text' AND typeof(a.confirmed_json)='null' AND typeof(a.created_at)='integer' AND NOT EXISTS(SELECT 1 FROM pr_write_thread_origins_v1 WHERE attempt_id=a.attempt_id) AND NOT EXISTS(SELECT 1 FROM pr_write_thread_comment_origins_v2 WHERE attempt_id=a.attempt_id) AND NOT EXISTS(SELECT 1 FROM pr_write_thread_reaction_origins_v3 WHERE attempt_id=a.attempt_id) AND NOT EXISTS(SELECT 1 FROM pr_write_file_comment_origins_v75 WHERE attempt_id=a.attempt_id)))");
        let metadata = self.metadata.as_ref();
        let sql = format!("{sql} AND NOT EXISTS(SELECT 1 FROM pr_write_issue_link_origins_v87 WHERE attempt_id=a.attempt_id)");
        let base = self.base_v32.as_ref();
        let state = self.state_v74.as_ref();
        let sql=format!("{sql} AND ((?33=0 AND NOT EXISTS(SELECT 1 FROM pr_write_metadata_base_origins_v32 WHERE attempt_id=a.attempt_id)) OR (?33=1 AND EXISTS(SELECT 1 FROM pr_write_metadata_base_origins_v32 AS b WHERE b.attempt_id=a.attempt_id AND typeof(b.repository_id)='integer' AND typeof(b.pr_id)='integer' AND typeof(b.origin_json)='text' AND b.repository_id=?34 AND b.pr_id=?35 AND CAST(b.origin_json AS BLOB)=CAST(?36 AS BLOB)) AND typeof(a.attempt_id)='integer' AND typeof(a.host)='text' AND typeof(a.principal)='text' AND typeof(a.pr_node)='text' AND typeof(a.repository)='text' AND typeof(a.pr_number)='integer' AND typeof(a.command)='text' AND typeof(a.selected_resource)='text' AND typeof(a.observed_head)='text' AND typeof(a.payload_sha256)='text' AND typeof(a.state)='text' AND typeof(a.confirmed_json)='null' AND typeof(a.created_at)='integer' AND NOT EXISTS(SELECT 1 FROM pr_write_thread_origins_v1 WHERE attempt_id=a.attempt_id) AND NOT EXISTS(SELECT 1 FROM pr_write_thread_comment_origins_v2 WHERE attempt_id=a.attempt_id) AND NOT EXISTS(SELECT 1 FROM pr_write_thread_reaction_origins_v3 WHERE attempt_id=a.attempt_id) AND NOT EXISTS(SELECT 1 FROM pr_write_file_comment_origins_v75 WHERE attempt_id=a.attempt_id)))");
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
            i64::from(origin.is_some()),
            origin.map(|o| o.repository),
            origin.map(|o| o.repository_node.as_str()),
            origin.map(|o| o.pr),
            origin.map(|o| o.json.as_str()),
            origin.and_then(|o| o.base_key.as_deref()),
            origin.and_then(|o| o.action.as_deref()),
            origin.and_then(|o| o.parent_ack),
            i64::from(conversation.is_some()),
            conversation.map(|c| c.repository),
            conversation.map(|c| c.pr),
            conversation.map(|c| c.json.as_str()),
            conversation.and_then(|c| c.base_key.as_deref()),
            conversation.and_then(|c| c.action.as_deref()),
            conversation.and_then(|c| c.parent_ack),
            i64::from(metadata.is_some()),
            metadata.map(|m| m.repository),
            metadata.map(|m| m.pr),
            metadata.map(|m| m.json.as_str()),
            i64::from(base.is_some()),
            base.map(|b| b.repository),
            base.map(|b| b.pr),
            base.map(|b| b.json.as_str()),
            metadata.and_then(|m| m.base_v48.as_deref()),
            metadata.and_then(|m| m.action_v48.as_deref()),
            metadata.and_then(|m| m.parent_v48),
            i64::from(state.is_some()),
            state.map(|s| s.repository),
            state.map(|s| s.pr),
            state.map(|s| s.issue),
            state.map(|s| s.command.as_str()),
            state.map(|s| s.json.as_str()),
            state.map(|s| s.base.as_str()),
            state.map(|s| s.action.as_str()),
            state.and_then(|s| s.parent_ack)
        ];
        if update {
            // Mechanical fixed SQL only; all private bytes are borrowed bind
            // values prepared before UID, not parsed/cloned after closure.
            let selected = sql.replacen("SELECT count(*)", "SELECT a.attempt_id", 1);
            let update = format!(
                "UPDATE pr_write_attempts SET state='acknowledged' WHERE attempt_id IN({selected})"
            );
            Ok(connection.execute(&update, values)?)
        } else {
            Ok(connection.query_row(&sql, values, |row| row.get(0))?)
        }
    }
}
impl PrWriteJournal {
    pub(super) fn commit_exact_ack_v17(
        &mut self,
        row: ExactAckRowV17,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<usize> {
        let clock = || {
            if cancel.is_cancelled() || Instant::now() >= deadline {
                Err(anyhow::anyhow!(
                    "Original exact acknowledgement cancelled/expired"
                ))
            } else {
                Ok(())
            }
        };
        clock()?;
        row.validate_current(self)?;
        let _busy = crate::storage::busy_v1::until(&self.connection, deadline)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        clock()?;
        if DatabaseV11::capture(&row.issuer)? != row.database
            || generation(&transaction, &row.target)? != row.generation
            || row.matches(&transaction)? != 1
        {
            bail!(
                "Original exact acknowledgement retired during lock wait; durable blocker retained"
            );
        }
        // Full exact raw row + origin predicates are IN the UPDATE itself,
        // inside the same bounded Immediate transaction. No split CAS exists.
        let changed = row.match_or_update(&transaction, true)?;
        if changed != 1 {
            bail!("Exact acknowledgement changed no current blocker");
        }
        clock()?;
        if DatabaseV11::capture(&row.issuer)? != row.database {
            bail!("Original ACK database replaced before commit");
        }
        advance(&transaction, &row.target)?;
        clock()?;
        transaction.commit()?;
        Ok(changed)
    }
}
