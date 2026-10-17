//! Closed original title/description metadata. Decoded history is not a
//! producer seed, source, Preview, permission, or approval to publish again.
use super::*;
use crate::{
    providers::gh::{OriginalMetadataTextV25, PrMetadataTextFieldV25, PrMetadataTextReceiptV25},
    scoped_pr_collaboration_source_v17::PrCollaborationSourceV17,
};
use review_operation_history_v3::{bounded_encode, DatabaseV3};
#[path = "pr_write_metadata_history_v25.rs"]
pub(crate) mod history_v25;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RecordedMetadataOriginV25 {
    version: u8,
    database: DatabaseV3,
    pub(crate) scope: Scope,
    pub(crate) repository_id: u64,
    repository_node: String,
    repository: String,
    pub(crate) pr_id: u64,
    pr_node: String,
    number: u64,
    head: String,
    base: String,
    pub(crate) field: PrMetadataTextFieldV25,
    before_sha256: String,
    requested_sha256: String,
    requester: String,
    author_uid: Option<u64>,
}
pub(crate) struct MetadataOriginSeedV25(RecordedMetadataOriginV25);
fn sha(s: &str) -> String {
    format!("{:x}", Sha256::digest(s.as_bytes()))
}
fn digest(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn positive(id: u64) -> bool {
    (1..=i64::MAX as u64).contains(&id)
}
pub(crate) fn draft_key_v25(
    scope: &Scope,
    parent: &crate::providers::gh::PrResourceParentV1,
    field: PrMetadataTextFieldV25,
) -> Result<String> {
    let bytes = serde_json::to_vec(&(
        scope,
        parent.repository_id(),
        parent.repository_node(),
        parent.database_id(),
        parent.node_id(),
        parent.number(),
        parent.head_sha(),
        parent.base_tip_sha(),
        field,
    ))?;
    if bytes.len() > 8192 {
        bail!("Metadata key exceeds bounded original identity");
    }
    Ok(format!("metadata-text-v25/{:x}", Sha256::digest(bytes)))
}
impl MetadataOriginSeedV25 {
    pub(crate) fn from_producer(
        source: &PrCollaborationSourceV17,
        actual: &OriginalMetadataTextV25,
        requested: &str,
        database: &Path,
    ) -> Result<Self> {
        actual
            .validate_live_source_v25()
            .map_err(anyhow::Error::msg)?;
        actual
            .prepare_requested(requested)
            .map_err(anyhow::Error::msg)?;
        let parent = actual.observed().parent();
        if source.opening().database() != database
            || !GithubProvider::same_thread_history_parent_v1(source.parent(), parent)
            || Scope::github(&actual.identity().host, actual.identity().account_id)?
                != *source.opening().scope()
            || actual.identity().login.len() > 1024
        {
            bail!("Actual metadata producer/source/principal differs");
        }
        let data = RecordedMetadataOriginV25 {
            version: 25,
            database: DatabaseV3::capture(database)?,
            scope: source.opening().scope().clone(),
            repository_id: parent.repository_id(),
            repository_node: parent.repository_node().into(),
            repository: parent.repository().into(),
            pr_id: parent.database_id(),
            pr_node: parent.node_id().into(),
            number: parent.number(),
            head: parent.head_sha().into(),
            base: parent.base_tip_sha().into(),
            field: actual.observed().field(),
            before_sha256: sha(actual.observed().value()),
            requested_sha256: sha(requested),
            requester: actual.identity().login.clone(),
            author_uid: parent.author_account_id(),
        };
        data.validate()?;
        bounded_encode(&data)?;
        Ok(Self(data))
    }
    pub(crate) fn write(&self, values: &BTreeMap<String, String>) -> Result<PrWriteIntent> {
        self.0.write(values)
    }
    pub(crate) fn draft_key(&self) -> Result<String> {
        self.0.draft_key()
    }
    pub(crate) fn matches_receipt(&self, receipt: &PrMetadataTextReceiptV25) -> bool {
        self.0.field == receipt.field()
            && self.0.pr_node == receipt.parent().node_id()
            && self.0.requested_sha256 == format!("{:x}", hex_digest(receipt.digest()))
    }
}
// Formatting an actual fixed digest never allocates or hashes a private body.
fn hex_digest(bytes: &[u8; 32]) -> DigestHexV25<'_> {
    DigestHexV25(bytes)
}
struct DigestHexV25<'a>(&'a [u8; 32]);
impl std::fmt::LowerHex for DigestHexV25<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for b in self.0 {
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}
impl RecordedMetadataOriginV25 {
    pub(crate) fn observed_requester(&self) -> &str {
        &self.requester
    }
    pub(crate) fn historical_base(&self) -> &str {
        &self.base
    }
    pub(crate) fn author_uid(&self) -> Option<u64> {
        self.author_uid
    }
    pub(crate) fn same_object(&self, parent: &crate::providers::gh::PrResourceParentV1) -> bool {
        self.repository_id == parent.repository_id()
            && self.repository_node == parent.repository_node()
            && self.pr_id == parent.database_id()
            && self.pr_node == parent.node_id()
            && self.number == parent.number()
            && self.repository == parent.repository()
    }
    pub(crate) fn matches_parent(&self, parent: &crate::providers::gh::PrResourceParentV1) -> bool {
        self.repository_id == parent.repository_id()
            && self.repository_node == parent.repository_node()
            && self.pr_id == parent.database_id()
            && self.pr_node == parent.node_id()
            && self.number == parent.number()
            && self.repository == parent.repository()
            && self.head == parent.head_sha()
            && self.base == parent.base_tip_sha()
    }
    pub(crate) fn matches_receipt(&self, receipt: &PrMetadataTextReceiptV25) -> bool {
        self.matches_parent(receipt.parent())
            && self.field == receipt.field()
            && self.requested_sha256 == format!("{:x}", hex_digest(receipt.digest()))
    }
    pub(crate) fn validate(&self) -> Result<()> {
        let uid = self
            .scope
            .login
            .strip_prefix("user-id:")
            .and_then(|s| s.parse::<u64>().ok());
        if self.version != 25
            || !uid.is_some_and(positive)
            || Scope::github(&self.scope.host, uid)? != self.scope
            || ![self.repository_id, self.pr_id, self.number]
                .into_iter()
                .all(positive)
            || !crate::search::model::valid_sha(&self.head)
            || !crate::search::model::valid_sha(&self.base)
            || !digest(&self.before_sha256)
            || !digest(&self.requested_sha256)
            || self.before_sha256 == self.requested_sha256
            || self.requester.is_empty()
            || self.requester.len() > 1024
            || self.requester.chars().any(char::is_control)
            || self.author_uid.is_some_and(|id| !positive(id))
        {
            bail!("Invalid closed metadata original");
        }
        validate_repository(&self.repository)?;
        for s in [&self.repository_node, &self.pr_node] {
            if s.len() > 1024 {
                bail!("Metadata node exceeds bound");
            }
            canonical_identifier(s)?;
        }
        bounded_encode(self)?;
        Ok(())
    }
    pub(crate) fn matches_database(&self, database: &Path) -> Result<bool> {
        Ok(self.database == DatabaseV3::capture(database)?)
    }
    pub(crate) fn draft_key(&self) -> Result<String> {
        self.validate()?;
        let bytes = serde_json::to_vec(&(
            &self.scope,
            self.repository_id,
            &self.repository_node,
            self.pr_id,
            &self.pr_node,
            self.number,
            &self.head,
            &self.base,
            self.field,
        ))?;
        Ok(format!("metadata-text-v25/{:x}", Sha256::digest(bytes)))
    }
    fn write(&self, values: &BTreeMap<String, String>) -> Result<PrWriteIntent> {
        self.validate()?;
        let value = values
            .get("body")
            .filter(|_| values.len() == 1)
            .context("Exact one metadata field required")?;
        self.field
            .validate_requested(value)
            .map_err(anyhow::Error::msg)?;
        if sha(value) != self.requested_sha256 {
            bail!("Requested metadata differs from actual prepared original");
        }
        let mut bound = values.clone();
        bound.insert("origin-v25".into(), sha(&bounded_encode(self)?));
        Ok(PrWriteIntent {
            command: self.field.command().into(),
            selected_resource: self.draft_key()?,
            observed_head: self.head.clone(),
            values: bound,
        })
    }
    pub(crate) fn validate_marker(
        &self,
        database: &Path,
        target: &PrWriteTarget,
        marker: &PrWriteMarker,
    ) -> Result<()> {
        self.validate()?;
        if !self.matches_database(database)?
            || target != &marker.target
            || target.scope() != &self.scope
            || target.repository() != self.repository
            || target.node_id() != self.pr_node
            || target.number() != self.number
            || marker.command != self.field.command()
            || marker.selected_resource != self.draft_key()?
            || marker.observed_head != self.head
        {
            bail!("Metadata origin/database/target/command/reference differs");
        }
        validate_shape(marker, marker.state == PrWriteState::Confirmed)?;
        if marker.state == PrWriteState::Confirmed {
            let expected = format!(
                "metadata-text-v25/{}/{}",
                self.field.key(),
                self.requested_sha256
            );
            if marker
                .confirmed
                .as_ref()
                .is_none_or(|r| r.resource_id != expected)
            {
                bail!("Metadata receipt does not confirm this exact requested value");
            }
        }
        Ok(())
    }
}
pub(crate) fn effect(receipt: &PrMetadataTextReceiptV25) -> PrConfirmedEffect {
    PrConfirmedEffect {
        kind: PrEffectKind::Metadata,
        resource_id: format!(
            "metadata-text-v25/{}/{:x}",
            receipt.field().key(),
            hex_digest(receipt.digest())
        ),
        node_id: Some(receipt.parent().node_id().into()),
    }
}
pub(crate) fn recognized(command: &str) -> bool {
    matches!(command, "metadata-title-v25" | "metadata-description-v25")
}
pub(crate) fn validate_shape(marker: &PrWriteMarker, confirmed: bool) -> Result<()> {
    let field = match marker.command.as_str() {
        "metadata-title-v25" => PrMetadataTextFieldV25::Title,
        "metadata-description-v25" => PrMetadataTextFieldV25::Description,
        _ => bail!("Not a closed metadata text purpose"),
    };
    if !marker
        .selected_resource
        .strip_prefix("metadata-text-v25/")
        .is_some_and(digest)
    {
        bail!("Known metadata command lacks exact original key digest");
    }
    if confirmed {
        let receipt = marker
            .confirmed
            .as_ref()
            .filter(|r| r.kind == PrEffectKind::Metadata && marker.state == PrWriteState::Confirmed)
            .context("Actual metadata receipt absent")?;
        let prefix = format!("metadata-text-v25/{}/", field.key());
        if !receipt
            .resource_id
            .strip_prefix(&prefix)
            .is_some_and(digest)
            || receipt.node_id.as_deref() != Some(marker.target.node_id())
        {
            bail!("Metadata field/receipt/node mismatch");
        }
    } else if !matches!(
        marker.state,
        PrWriteState::Pending | PrWriteState::Uncertain | PrWriteState::Acknowledged
    ) || marker.confirmed.is_some()
    {
        bail!("Possible metadata effect needs unresolved original or explicit local ACK");
    }
    Ok(())
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MetadataOriginRecordV25 {
    pub(crate) seed: RecordedMetadataOriginV25,
    pub(crate) reference: PrPublicationReference,
}
pub(super) fn migrate(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS pr_write_metadata_origins_v25(attempt_id INTEGER PRIMARY KEY REFERENCES pr_write_attempts(attempt_id) ON DELETE CASCADE,repository_id INTEGER NOT NULL,pr_id INTEGER NOT NULL,origin_json TEXT NOT NULL);CREATE TRIGGER IF NOT EXISTS pr_write_metadata_origins_cleanup_v25 AFTER DELETE ON pr_write_attempts BEGIN DELETE FROM pr_write_metadata_origins_v25 WHERE attempt_id=OLD.attempt_id; END;CREATE TRIGGER IF NOT EXISTS pr_write_metadata_origins_immutable_v25 BEFORE UPDATE ON pr_write_metadata_origins_v25 BEGIN SELECT RAISE(ABORT,'Immutable original metadata');END;")?;
    Ok(())
}
pub(super) fn insert(
    tx: &Transaction<'_>,
    seed: &MetadataOriginSeedV25,
    marker: &PrWriteMarker,
) -> Result<()> {
    let record = MetadataOriginRecordV25 {
        seed: seed.0.clone(),
        reference: marker.publication_reference(),
    };
    let encoded = bounded_encode(&record)?;
    tx.execute("INSERT INTO pr_write_metadata_origins_v25(attempt_id,repository_id,pr_id,origin_json) VALUES(?1,?2,?3,?4)",params![marker.attempt_id,seed.0.repository_id as i64,seed.0.pr_id as i64,encoded])?;
    Ok(())
}
impl PrWriteJournal {
    pub(crate) fn begin_metadata_text_v25(
        &mut self,
        target: &PrWriteTarget,
        intent: &PrWriteIntent,
        seed: &MetadataOriginSeedV25,
    ) -> Result<PrWritePermit> {
        let values = BTreeMap::from([(
            "body".into(),
            intent
                .values
                .get("body")
                .context("Actual metadata value missing")?
                .clone(),
        )]);
        if seed.write(&values)?.digest()? != intent.digest()?
            || !seed.0.matches_database(&self.path)?
        {
            bail!("Fresh metadata seed does not bind original publication request");
        }
        self.begin_with_observed_origin_v2(
            target,
            intent,
            Some(ObservedOriginV2::MetadataTextV25(seed)),
        )
    }
    pub(crate) fn metadata_origin_v25(
        &self,
        target: &PrWriteTarget,
        marker: &PrWriteMarker,
    ) -> Result<MetadataOriginRecordV25> {
        metadata_members_v48::require_text_null_v48(&self.connection, marker.attempt_id)?;
        let foreign:usize=self.connection.query_row("SELECT (SELECT count(*) FROM pr_write_conversation_origins_v17 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_review_operation_origins_v3 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_thread_origins_v1 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_thread_comment_origins_v2 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_thread_reaction_origins_v3 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_file_comment_origins_v75 WHERE attempt_id=?1)",[marker.attempt_id],|r|r.get(0))?;
        let base: usize = self.connection.query_row(
            "SELECT count(*) FROM pr_write_metadata_base_origins_v32 WHERE attempt_id=?1",
            [marker.attempt_id],
            |r| r.get(0),
        )?;
        if foreign != 0 || base != 0 {
            bail!("Mixed original purposes cannot expose metadata body or release a barrier");
        }
        let bytes:usize=self.connection.query_row("SELECT length(CAST(origin_json AS BLOB)) FROM pr_write_metadata_origins_v25 WHERE attempt_id=?1",[marker.attempt_id],|r|r.get(0))?;
        if bytes > 16 * 1024 {
            bail!("Original metadata exceeds16KiB before copy");
        }
        let (repo,pr,raw):(u64,u64,String)=self.connection.query_row("SELECT repository_id,pr_id,origin_json FROM pr_write_metadata_origins_v25 WHERE attempt_id=?1 AND typeof(repository_id)='integer' AND typeof(pr_id)='integer' AND typeof(origin_json)='text' AND length(CAST(origin_json AS BLOB))<=16384 AND typeof(base_key_v48)='null' AND typeof(action_tag_v48)='null' AND typeof(parent_ack_attempt_v48)='null'",[marker.attempt_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        let record: MetadataOriginRecordV25 = serde_json::from_str(&raw)?;
        record.seed.validate_marker(&self.path, target, marker)?;
        if record.reference != marker.publication_reference()
            || record.seed.repository_id != repo
            || record.seed.pr_id != pr
        {
            bail!("Metadata original scalar/reference differs");
        }
        Ok(record)
    }
}
