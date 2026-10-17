//! Exact native member originals in the SAME Metadata origin table. Recorded
//! observations/digests are inspect-only; only a genuine current phase issues
//! a seed. No saved ID, body, ACK or deserialization issues write authority.
use super::*;
use crate::{
    pr_metadata_actions::{PrMetadataCommand, PrMetadataPlan, PrMetadataReceipt},
    providers::gh::{OriginalMetadataMembersV48, PrResourceParentV1},
    scoped_pr_collaboration_source_v17::PrCollaborationSourceV17,
};
use review_operation_history_v3::{bounded_encode, DatabaseV3};
#[path = "pr_write_metadata_members_history_v48.rs"]
pub(crate) mod history_v48;
#[path = "pr_write_metadata_members_lineage_v52.rs"]
pub(crate) mod lineage_v52;

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn canonical_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn positive(value: u64) -> bool {
    (1..=i64::MAX as u64).contains(&value)
}
fn command(tag: &str) -> Result<PrMetadataCommand> {
    PrMetadataCommand::ALL
        .into_iter()
        .find(|command| command.key() == tag)
        .context("Not one of the ten closed metadata member purposes")
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RecordedMetadataMembersV48 {
    version: u8,
    database: DatabaseV3,
    pub(crate) scope: Scope,
    pub(crate) repository_id: u64,
    repository_node: String,
    repository: String,
    pub(crate) pr_id: u64,
    pr_node: String,
    number: u64,
    issue_id: u64,
    issue_node: String,
    head: String,
    base: String,
    head_ref_sha256: String,
    base_ref_sha256: String,
    action: String,
    selected_id: u64,
    selected_node: String,
    selected_namespace_sha256: String,
    base_key: String,
    draft_key: String,
    parent_ack_attempt: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    lineage_v52: Option<lineage_v52::MembersLineageV52>,
    before_sha256: String,
    after_sha256: String,
    expected_effect: PrConfirmedEffect,
    requested_plan_sha256: String,
    values_sha256: String,
}
/// No serde/DTO constructor: created ONLY while the actual original member
/// observation is live and the exact typed plan has been freshly validated.
pub(crate) struct MetadataMembersOriginSeedV48(RecordedMetadataMembersV48);
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MetadataMembersOriginRecordV48 {
    pub(crate) seed: RecordedMetadataMembersV48,
    pub(crate) reference: PrPublicationReference,
}
pub(crate) enum MetadataOriginDispositionV48 {
    LegacyOriginless,
    TextV25,
    MembersV48(Box<MetadataMembersOriginRecordV48>),
}
/// Pure fixed-layout accounting, not an original/permission constructor.
pub(crate) fn reserve_origin_box_moves_v79() -> Result<usize> {
    conversation_action_lineage_v18::kernel_v52::reserve_origin_box_moves_v79()
}
impl MetadataMembersOriginRecordV48 {
    /// Accepted historical envelope/ref only; never a source, selection,
    /// approval or retry. Stored values remain inspect-only.
    pub(crate) fn protected_values_v48(
        &self,
        path: &Path,
        marker: &PrWriteMarker,
    ) -> Result<BTreeMap<String, String>> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Saved {
            version: u8,
            values: BTreeMap<String, String>,
            publication: Option<PrPublicationReference>,
        }
        let encoded = Cache::open(path)?
            .bounded_draft(marker.target.scope(), &marker.selected_resource, 128 * 1024)?
            .context("Exact protected metadata member envelope unavailable")?;
        let saved: Saved = serde_json::from_str(&encoded)?;
        if saved.version != 17
            || saved.publication.as_ref() != Some(&self.reference)
            || self.reference != marker.publication_reference()
            || self.seed.write(&saved.values)?.digest()? != marker.payload_sha256
        {
            bail!("Protected metadata member envelope/ref/body differs from exact original");
        }
        Ok(saved.values)
    }
}

impl MetadataMembersOriginSeedV48 {
    pub(crate) fn from_producer(
        source: &PrCollaborationSourceV17,
        actual: &OriginalMetadataMembersV48,
        plan: &PrMetadataPlan,
        values: &BTreeMap<String, String>,
        database: &Path,
    ) -> Result<Self> {
        actual.require_mutation_v48().map_err(anyhow::Error::msg)?;
        plan.validate_original_members_v48(actual)
            .map_err(anyhow::Error::msg)?;
        let parent = actual.parent();
        if source.opening().database() != database
            || !GithubProvider::same_thread_history_parent_v1(source.parent(), parent)
            || Scope::github(&actual.identity().host, actual.identity().account_id)?
                != *source.opening().scope()
            || plan.target().scope() != source.opening().scope()
            || values.len() != 1
            || values.get("body").is_none_or(|body| body.len() > 65_536)
        {
            bail!("Actual metadata member producer/source/selected envelope differs");
        }
        let selected = plan
            .selected()
            .context("Actual selected member unavailable, including Clear milestone")?;
        let scope = source.opening().scope().clone();
        let selected_namespace_sha256 = actual
            .selected_namespace_sha256_v48(plan)
            .map_err(anyhow::Error::msg)?;
        // Stable identity explicitly excludes head/base, mutable sets, updated
        // times, body, display row, source allocation/generation and clocks.
        let (base_key, draft_key) = actual.action_keys_v48(plan).map_err(anyhow::Error::msg)?;
        let record = RecordedMetadataMembersV48 {
            version: 48,
            database: DatabaseV3::capture(database)?,
            scope,
            repository_id: parent.repository_id(),
            repository_node: parent.repository_node().into(),
            repository: parent.repository().into(),
            pr_id: parent.database_id(),
            pr_node: parent.node_id().into(),
            number: parent.number(),
            issue_id: plan.issue_id(),
            issue_node: plan.issue_node().into(),
            head: parent.head_sha().into(),
            base: parent.base_tip_sha().into(),
            head_ref_sha256: digest(plan.original().head_ref.as_bytes()),
            base_ref_sha256: digest(plan.original().base_ref.as_bytes()),
            action: plan.command().key().into(),
            selected_id: selected.id(),
            selected_node: selected.node().into(),
            selected_namespace_sha256,
            base_key,
            draft_key,
            parent_ack_attempt: None,
            lineage_v52: None,
            before_sha256: plan.intent().values["before_sha256"].clone(),
            after_sha256: plan.intent().values["after_sha256"].clone(),
            expected_effect: plan.receipt().map_err(anyhow::Error::msg)?.effect(),
            requested_plan_sha256: plan.intent().digest()?,
            values_sha256: digest(bounded_encode(values)?.as_bytes()),
        };
        record.validate()?;
        Ok(Self(record))
    }
    pub(crate) fn draft_key(&self) -> &str {
        &self.0.draft_key
    }
    #[cfg(test)]
    pub(crate) fn base_key(&self) -> &str {
        &self.0.base_key
    }
    pub(crate) fn write(&self, values: &BTreeMap<String, String>) -> Result<PrWriteIntent> {
        self.0.write(values)
    }
    pub(crate) fn matches_receipt(&self, effect: &PrConfirmedEffect) -> Result<bool> {
        self.0.validate()?;
        Ok(self.0.expected_effect == *effect)
    }
}
impl RecordedMetadataMembersV48 {
    pub(super) fn sql_identity_v48(&self) -> (u64, u64, &str, &str, Option<i64>) {
        (
            self.repository_id,
            self.pr_id,
            &self.base_key,
            &self.action,
            self.parent_ack_attempt,
        )
    }
    pub(crate) fn matches_effect_v48(&self, effect: &PrConfirmedEffect) -> Result<bool> {
        self.validate()?;
        Ok(self.expected_effect == *effect)
    }
    pub(crate) fn command(&self) -> Result<PrMetadataCommand> {
        command(&self.action)
    }
    pub(crate) fn base_key(&self) -> &str {
        &self.base_key
    }
    #[cfg(test)]
    pub(crate) fn parent_ack_attempt(&self) -> Option<i64> {
        self.parent_ack_attempt
    }
    pub(crate) fn same_object(&self, parent: &PrResourceParentV1) -> bool {
        self.repository_id == parent.repository_id()
            && self.repository_node == parent.repository_node()
            && self.repository == parent.repository()
            && self.pr_id == parent.database_id()
            && self.pr_node == parent.node_id()
            && self.number == parent.number()
    }
    pub(crate) fn matches_parent(&self, parent: &PrResourceParentV1) -> bool {
        self.same_object(parent)
            && self.head == parent.head_sha()
            && self.base == parent.base_tip_sha()
    }
    pub(crate) fn validate(&self) -> Result<()> {
        let uid = self
            .scope
            .login
            .strip_prefix("user-id:")
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|id| positive(*id));
        if self.version != 48
            || uid.is_none()
            || Scope::github(&self.scope.host, uid)? != self.scope
            || ![
                self.repository_id,
                self.pr_id,
                self.number,
                self.issue_id,
                self.selected_id,
            ]
            .into_iter()
            .all(positive)
            || !crate::search::model::valid_sha(&self.head)
            || !crate::search::model::valid_sha(&self.base)
            || self.parent_ack_attempt.is_some_and(|id| id <= 0)
            || ![
                &self.selected_namespace_sha256,
                &self.head_ref_sha256,
                &self.base_ref_sha256,
                &self.before_sha256,
                &self.after_sha256,
                &self.requested_plan_sha256,
                &self.values_sha256,
            ]
            .into_iter()
            .all(|value| canonical_digest(value))
            || self.before_sha256 == self.after_sha256
            || !self
                .base_key
                .strip_prefix("metadata-members-base-v48/")
                .is_some_and(canonical_digest)
            || !self
                .draft_key
                .strip_prefix("metadata-members-v48/")
                .is_some_and(canonical_digest)
        {
            bail!("Malformed exact original Metadata Members record");
        }
        validate_repository(&self.repository)?;
        for node in [
            &self.repository_node,
            &self.pr_node,
            &self.issue_node,
            &self.selected_node,
        ] {
            if node.len() > 512 {
                bail!("Metadata member node bound exceeded");
            }
            canonical_identifier(node)?;
        }
        let decoded =
            PrMetadataReceipt::decode(&self.expected_effect).map_err(anyhow::Error::msg)?;
        let (pr_id, issue_id, before, after) = decoded.original_binding_v48();
        if decoded.command() != command(&self.action)?
            || decoded.selected_id() != Some(self.selected_id)
            || decoded.repository_id() != self.repository_id
            || pr_id != self.pr_id
            || issue_id != self.issue_id
            || before != self.before_sha256
            || after != self.after_sha256
            || self.expected_effect.node_id.as_deref() != Some(self.pr_node.as_str())
        {
            bail!("Metadata member expected effect differs from exact selected purpose");
        }
        let stable = bounded_encode(&(
            "ghtui-metadata-members-base-v48",
            &self.scope,
            self.repository_id,
            &self.repository_node,
            self.pr_id,
            &self.pr_node,
            self.number,
            self.issue_id,
            &self.issue_node,
            &self.action,
            self.selected_id,
            &self.selected_node,
            &self.selected_namespace_sha256,
        ))?;
        if self.base_key != format!("metadata-members-base-v48/{}", digest(stable.as_bytes())) {
            bail!("Member stable base differs from its exact immutable selected identity");
        }
        if self.lineage_v52.is_none() {
            if self.parent_ack_attempt.is_some() {
                bail!("Root Members original cannot invent an ACK ancestor");
            }
            let draft = bounded_encode(&(
                "ghtui-metadata-members-draft-v48",
                &self.base_key,
                &self.head,
                &self.base,
            ))?;
            if self.draft_key != format!("metadata-members-v48/{}", digest(draft.as_bytes())) {
                bail!("Root member draft differs from actual head/base bound selected identity");
            }
        } else {
            lineage_v52::validate_record_v52(self)?;
        }
        bounded_encode(self)?;
        Ok(())
    }
    pub(in crate::pr_write_journal) fn write(
        &self,
        values: &BTreeMap<String, String>,
    ) -> Result<PrWriteIntent> {
        self.validate()?;
        if values.len() != 1
            || values.get("body").is_none_or(|body| body.len() > 65_536)
            || digest(bounded_encode(values)?.as_bytes()) != self.values_sha256
        {
            bail!("Metadata member accepted values differ from exact original");
        }
        let mut bound = values.clone();
        bound.insert(
            "origin-members-v48".into(),
            digest(bounded_encode(self)?.as_bytes()),
        );
        bound.insert(
            "requested-plan-v48".into(),
            self.requested_plan_sha256.clone(),
        );
        Ok(PrWriteIntent {
            command: self.action.clone(),
            selected_resource: self.draft_key.clone(),
            observed_head: self.head.clone(),
            values: bound,
        })
    }
    pub(crate) fn validate_marker(
        &self,
        path: &Path,
        target: &PrWriteTarget,
        marker: &PrWriteMarker,
    ) -> Result<()> {
        self.validate()?;
        if self.database != DatabaseV3::capture(path)?
            || target != &marker.target
            || target.scope() != &self.scope
            || target.repository() != self.repository
            || target.node_id() != self.pr_node
            || target.number() != self.number
            || marker.command != self.action
            || marker.selected_resource != self.draft_key
            || marker.observed_head != self.head
        {
            bail!("Exact metadata member origin/database/target/reference differs");
        }
        match marker.state {
            PrWriteState::Confirmed if marker.confirmed.as_ref() == Some(&self.expected_effect) => {
            }
            PrWriteState::Pending | PrWriteState::Uncertain | PrWriteState::Acknowledged
                if marker.confirmed.is_none() => {}
            _ => bail!("Metadata member actual receipt/state differs; no invented effect"),
        }
        Ok(())
    }
}

pub(super) fn migrate(tx: &Transaction<'_>) -> Result<()> {
    let columns = {
        let mut query = tx.prepare("PRAGMA table_info(pr_write_metadata_origins_v25)")?;
        let columns = query
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        columns
    };
    for (column, statement) in [
        ("base_key_v48", "ALTER TABLE pr_write_metadata_origins_v25 ADD COLUMN base_key_v48 TEXT NULL"),
        ("action_tag_v48", "ALTER TABLE pr_write_metadata_origins_v25 ADD COLUMN action_tag_v48 TEXT NULL"),
        ("parent_ack_attempt_v48", "ALTER TABLE pr_write_metadata_origins_v25 ADD COLUMN parent_ack_attempt_v48 INTEGER NULL"),
    ] {
        if !columns.iter().any(|present| present == column) { tx.execute_batch(statement)?; }
    }
    tx.execute_batch("CREATE INDEX IF NOT EXISTS pr_write_metadata_members_lineage_v48 ON pr_write_metadata_origins_v25(base_key_v48,action_tag_v48,attempt_id DESC)")?;
    Ok(())
}
pub(super) fn insert(
    tx: &Transaction<'_>,
    seed: &MetadataMembersOriginSeedV48,
    marker: &PrWriteMarker,
) -> Result<()> {
    let encoded = bounded_encode(&MetadataMembersOriginRecordV48 {
        seed: seed.0.clone(),
        reference: marker.publication_reference(),
    })?;
    tx.execute("INSERT INTO pr_write_metadata_origins_v25(attempt_id,repository_id,pr_id,origin_json,base_key_v48,action_tag_v48,parent_ack_attempt_v48) VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![marker.attempt_id, seed.0.repository_id as i64, seed.0.pr_id as i64, encoded, seed.0.base_key, seed.0.action, seed.0.parent_ack_attempt])?;
    Ok(())
}
pub(super) fn validate_begin(
    tx: &Transaction<'_>,
    path: &Path,
    target: &PrWriteTarget,
    seed: &MetadataMembersOriginSeedV48,
) -> Result<()> {
    seed.0.validate()?;
    if target.scope() != &seed.0.scope
        || target.node_id() != seed.0.pr_node
        || target.repository() != seed.0.repository
        || target.number() != seed.0.number
    {
        bail!("Member original target differs before atomic begin");
    }
    let latest: Option<i64> = tx.query_row("SELECT MAX(attempt_id) FROM pr_write_metadata_origins_v25 WHERE base_key_v48=?1 AND action_tag_v48=?2",
        params![seed.0.base_key, seed.0.action], |row| row.get(0))?;
    if let Some(lineage) = &seed.0.lineage_v52 {
        return conversation_action_lineage_v18::kernel_v52::validate_begin_v52(
            tx,
            path,
            conversation_action_lineage_v18::kernel_v52::CompareV52::Members(&seed.0),
            target,
            &lineage.captured,
        );
    }
    if latest.is_some() || seed.0.parent_ack_attempt.is_some() {
        bail!(
            "Existing member action requires its exact explicit terminal descendant; no root reset"
        );
    }
    Ok(())
}
pub(super) fn require_text_null_v48(connection: &Connection, attempt: i64) -> Result<()> {
    let exact: bool = connection.query_row("SELECT typeof(repository_id)='integer' AND typeof(pr_id)='integer' AND typeof(origin_json)='text' AND typeof(base_key_v48)='null' AND typeof(action_tag_v48)='null' AND typeof(parent_ack_attempt_v48)='null' FROM pr_write_metadata_origins_v25 WHERE attempt_id=?1", [attempt], |row| row.get(0))?;
    if !exact {
        bail!("Text original has foreign member indices or invalid scalar types");
    }
    Ok(())
}
impl PrWriteJournal {
    pub(crate) fn metadata_origin_disposition_v48(
        &self,
        target: &PrWriteTarget,
        marker: &PrWriteMarker,
    ) -> Result<MetadataOriginDispositionV48> {
        let count: usize = self.connection.query_row(
            "SELECT count(*) FROM pr_write_metadata_origins_v25 WHERE attempt_id=?1",
            [marker.attempt_id],
            |row| row.get(0),
        )?;
        match count {
            0 if marker
                .selected_resource
                .starts_with("metadata-members-v48/")
                || metadata_origin_v25::recognized(&marker.command) =>
            {
                bail!("Known native metadata original missing; no legacy fallback")
            }
            0 => Ok(MetadataOriginDispositionV48::LegacyOriginless),
            1 if metadata_origin_v25::recognized(&marker.command) => {
                self.metadata_origin_v25(target, marker)?;
                Ok(MetadataOriginDispositionV48::TextV25)
            }
            1 => {
                reserve_origin_box_moves_v79()?;
                Ok(MetadataOriginDispositionV48::MembersV48(Box::new(
                    self.metadata_members_origin_v48(target, marker)?,
                )))
            }
            _ => bail!("Mixed exact metadata original rows"),
        }
    }
    pub(crate) fn begin_metadata_members_v48(
        &mut self,
        target: &PrWriteTarget,
        write: &PrWriteIntent,
        seed: &MetadataMembersOriginSeedV48,
    ) -> Result<PrWritePermit> {
        let values = BTreeMap::from([(
            "body".into(),
            write
                .values
                .get("body")
                .context("Actual accepted member envelope missing")?
                .clone(),
        )]);
        if seed.write(&values)?.digest()? != write.digest()?
            || seed.0.database != DatabaseV3::capture(&self.path)?
        {
            bail!("Current member seed differs from exact publication request");
        }
        self.begin_with_observed_origin_v2(
            target,
            write,
            Some(ObservedOriginV2::MetadataMembersV48(seed)),
        )
    }
    pub(crate) fn metadata_members_origin_v48(
        &self,
        target: &PrWriteTarget,
        marker: &PrWriteMarker,
    ) -> Result<MetadataMembersOriginRecordV48> {
        load_record_v52(&self.connection, &self.path, target, marker)
    }
}
pub(super) fn load_record_v52(
    connection: &Connection,
    path: &Path,
    target: &PrWriteTarget,
    marker: &PrWriteMarker,
) -> Result<MetadataMembersOriginRecordV48> {
    let foreign: usize = connection.query_row("SELECT (SELECT count(*) FROM pr_write_conversation_origins_v17 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_metadata_base_origins_v32 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_review_operation_origins_v3 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_thread_origins_v1 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_thread_comment_origins_v2 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_thread_reaction_origins_v3 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_file_comment_origins_v75 WHERE attempt_id=?1)", [marker.attempt_id], |row| row.get(0))?;
    if foreign != 0 {
        bail!("Mixed purpose cannot expose a member body or ACK");
    }
    let (valid, bytes): (bool, usize) = connection.query_row("SELECT typeof(repository_id)='integer' AND typeof(pr_id)='integer' AND typeof(origin_json)='text' AND typeof(base_key_v48)='text' AND length(CAST(base_key_v48 AS BLOB)) BETWEEN 1 AND 128 AND typeof(action_tag_v48)='text' AND length(CAST(action_tag_v48 AS BLOB)) BETWEEN 1 AND 64 AND (typeof(parent_ack_attempt_v48)='null' OR (typeof(parent_ack_attempt_v48)='integer' AND parent_ack_attempt_v48>0)), length(CAST(origin_json AS BLOB)) FROM pr_write_metadata_origins_v25 WHERE attempt_id=?1", [marker.attempt_id], |row| Ok((row.get(0)?,row.get(1)?)))?;
    if !valid || bytes > 16 * 1024 {
        bail!("Invalid/bounded member original types before copy");
    }
    let (repo, pr, raw, base, action, parent): (u64,u64,String,String,String,Option<i64>) = connection.query_row("SELECT repository_id,pr_id,origin_json,base_key_v48,action_tag_v48,parent_ack_attempt_v48 FROM pr_write_metadata_origins_v25 WHERE attempt_id=?1 AND typeof(repository_id)='integer' AND typeof(pr_id)='integer' AND typeof(origin_json)='text' AND length(CAST(origin_json AS BLOB))<=16384 AND typeof(base_key_v48)='text' AND length(CAST(base_key_v48 AS BLOB)) BETWEEN 1 AND 128 AND typeof(action_tag_v48)='text' AND length(CAST(action_tag_v48 AS BLOB)) BETWEEN 1 AND 64 AND (typeof(parent_ack_attempt_v48)='null' OR (typeof(parent_ack_attempt_v48)='integer' AND parent_ack_attempt_v48>0))", [marker.attempt_id], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?)))?;
    let record: MetadataMembersOriginRecordV48 = serde_json::from_str(&raw)?;
    record.seed.validate_marker(path, target, marker)?;
    if record.reference != marker.publication_reference()
        || record.seed.repository_id != repo
        || record.seed.pr_id != pr
        || record.seed.base_key != base
        || record.seed.action != action
        || record.seed.parent_ack_attempt != parent
    {
        bail!("Exact member original raw lineage/reference differs");
    }
    Ok(record)
}
