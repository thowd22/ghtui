//! Closed actual Base selection. Recorded history cannot recreate eligibility,
//! an actual selected row, capability, source, Preview or resend permission.
use super::*;
use crate::{
    providers::gh::{BaseSelectionV32, OriginalMetadataBaseV32, PrMetadataBaseReceiptV32},
    scoped_pr_collaboration_source_v17::PrCollaborationSourceV17,
};
use review_operation_history_v3::{bounded_encode, DatabaseV3};
#[path = "pr_write_metadata_base_history_v32.rs"]
pub(crate) mod history_v32;
pub(crate) const COMMAND_V32: &str = "metadata-base-v32";
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RecordedBaseOriginV32 {
    version: u8,
    database: DatabaseV3,
    scope: Scope,
    repository_id: u64,
    repository_node: String,
    repository: String,
    pr_id: u64,
    pr_node: String,
    number: u64,
    head: String,
    base: String,
    base_ref: String,
    requested_ref: String,
    requested_tip: String,
    requester: String,
    author_uid: Option<u64>,
}
pub(crate) struct BaseOriginSeedV32(RecordedBaseOriginV32);
fn digest(v: &str) -> bool {
    v.len() == 64
        && v.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn sha(v: &str) -> String {
    format!("{:x}", Sha256::digest(v.as_bytes()))
}
fn selected_digest(name: &str, tip: &str) -> String {
    let mut h = Sha256::new();
    h.update(name.as_bytes());
    h.update([0]);
    h.update(tip.as_bytes());
    format!("{:x}", h.finalize())
}
fn branch(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 1024
        && v != "@"
        && v != "HEAD"
        && !v.starts_with('-')
        && !v.starts_with('/')
        && !v.ends_with('/')
        && !v.ends_with('.')
        && !v.contains("..")
        && !v.contains("@{")
        && !v
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || "~^:?*[\\".contains(c))
        && v.split('/')
            .all(|s| !s.is_empty() && !s.starts_with('.') && !s.ends_with(".lock"))
}
pub(crate) fn draft_key_v32(
    scope: &Scope,
    p: &crate::providers::gh::PrResourceParentV1,
) -> Result<String> {
    let b = serde_json::to_vec(&(
        scope,
        p.repository_id(),
        p.repository_node(),
        p.database_id(),
        p.node_id(),
        p.number(),
        p.head_sha(),
        p.base_tip_sha(),
        &p.summary().base_ref,
    ))?;
    if b.len() > 8192 {
        bail!("Base key exceeds bounded original identity");
    }
    Ok(format!("metadata-base-v32/{:x}", Sha256::digest(b)))
}
impl BaseOriginSeedV32 {
    pub(crate) fn from_producer(
        source: &PrCollaborationSourceV17,
        actual: &OriginalMetadataBaseV32,
        selected: &BaseSelectionV32,
        database: &Path,
    ) -> Result<Self> {
        actual
            .require_change(selected)
            .map_err(anyhow::Error::msg)?;
        let p = actual.parent();
        if source.opening().database() != database
            || !GithubProvider::same_thread_history_parent_v1(source.parent(), p)
            || Scope::github(&actual.identity().host, actual.identity().account_id)?
                != *source.opening().scope()
        {
            bail!("Actual Base producer/source/principal differs");
        }
        let data = RecordedBaseOriginV32 {
            version: 32,
            database: DatabaseV3::capture(database)?,
            scope: source.opening().scope().clone(),
            repository_id: p.repository_id(),
            repository_node: p.repository_node().into(),
            repository: p.repository().into(),
            pr_id: p.database_id(),
            pr_node: p.node_id().into(),
            number: p.number(),
            head: p.head_sha().into(),
            base: p.base_tip_sha().into(),
            base_ref: p.summary().base_ref.clone(),
            requested_ref: selected.name().into(),
            requested_tip: selected.tip().into(),
            requester: actual.identity().login.clone(),
            author_uid: p.author_account_id(),
        };
        data.validate()?;
        bounded_encode(&data)?;
        Ok(Self(data))
    }
    pub(crate) fn write(&self, v: &BTreeMap<String, String>) -> Result<PrWriteIntent> {
        self.0.write(v)
    }
    pub(crate) fn draft_key(&self) -> Result<String> {
        self.0.draft_key()
    }
    pub(crate) fn matches_receipt(&self, receipt: &PrMetadataBaseReceiptV32) -> bool {
        self.0.matches_receipt(receipt)
    }
}
impl RecordedBaseOriginV32 {
    pub(crate) fn requested_ref(&self) -> &str {
        &self.requested_ref
    }
    pub(crate) fn requested_tip(&self) -> &str {
        &self.requested_tip
    }
    pub(crate) fn historical_base(&self) -> &str {
        &self.base
    }
    pub(crate) fn historical_ref(&self) -> &str {
        &self.base_ref
    }
    pub(crate) fn same_object(&self, p: &crate::providers::gh::PrResourceParentV1) -> bool {
        self.repository_id == p.repository_id()
            && self.repository_node == p.repository_node()
            && self.repository == p.repository()
            && self.pr_id == p.database_id()
            && self.pr_node == p.node_id()
            && self.number == p.number()
    }
    pub(crate) fn matches_old(&self, p: &crate::providers::gh::PrResourceParentV1) -> bool {
        self.same_object(p)
            && self.head == p.head_sha()
            && self.base == p.base_tip_sha()
            && self.base_ref == p.summary().base_ref
            && self.author_uid == p.author_account_id()
    }
    pub(crate) fn matches_receipt(&self, r: &PrMetadataBaseReceiptV32) -> bool {
        self.matches_old(r.transition().old())
            && self.same_object(r.parent())
            && self.head == r.parent().head_sha()
            && self.requested_tip == r.parent().base_tip_sha()
            && self.requested_ref == r.parent().summary().base_ref
    }
    fn validate(&self) -> Result<()> {
        let uid = self
            .scope
            .login
            .strip_prefix("user-id:")
            .and_then(|s| s.parse::<u64>().ok());
        let positive = |id: u64| (1..=i64::MAX as u64).contains(&id);
        if self.version != 32
            || !uid.is_some_and(positive)
            || Scope::github(&self.scope.host, uid)? != self.scope
            || ![self.repository_id, self.pr_id, self.number]
                .into_iter()
                .all(positive)
            || ![&self.head, &self.base, &self.requested_tip]
                .into_iter()
                .all(|s| crate::search::model::valid_sha(s))
            || !branch(&self.base_ref)
            || !branch(&self.requested_ref)
            || self.base_ref == self.requested_ref
            || self.requester.is_empty()
            || self.requester.len() > 1024
            || self.requester.chars().any(char::is_control)
            || self.author_uid.is_some_and(|id| !positive(id))
        {
            bail!("Invalid closed Base original");
        }
        validate_repository(&self.repository)?;
        for s in [&self.repository_node, &self.pr_node] {
            if s.len() > 1024 {
                bail!("Base node exceeds bound");
            }
            canonical_identifier(s)?;
        }
        bounded_encode(self)?;
        Ok(())
    }
    fn matches_database(&self, database: &Path) -> Result<bool> {
        Ok(self.database == DatabaseV3::capture(database)?)
    }
    fn draft_key(&self) -> Result<String> {
        self.validate()?;
        Ok(format!(
            "metadata-base-v32/{:x}",
            Sha256::digest(serde_json::to_vec(&(
                &self.scope,
                self.repository_id,
                &self.repository_node,
                self.pr_id,
                &self.pr_node,
                self.number,
                &self.head,
                &self.base,
                &self.base_ref
            ))?)
        ))
    }
    fn write(&self, v: &BTreeMap<String, String>) -> Result<PrWriteIntent> {
        self.validate()?;
        if v.len() != 2
            || v.get("base").map(String::as_str) != Some(self.requested_ref.as_str())
            || v.get("tip").map(String::as_str) != Some(self.requested_tip.as_str())
        {
            bail!("Base request differs from actual selected member");
        }
        let mut bound = v.clone();
        bound.insert("origin-v32".into(), sha(&bounded_encode(self)?));
        Ok(PrWriteIntent {
            command: COMMAND_V32.into(),
            selected_resource: self.draft_key()?,
            observed_head: self.head.clone(),
            values: bound,
        })
    }
    fn validate_marker(
        &self,
        database: &Path,
        target: &PrWriteTarget,
        m: &PrWriteMarker,
    ) -> Result<()> {
        self.validate()?;
        let values = BTreeMap::from([
            ("base".into(), self.requested_ref.clone()),
            ("tip".into(), self.requested_tip.clone()),
        ]);
        if !self.matches_database(database)?
            || target != &m.target
            || target.scope() != &self.scope
            || target.repository() != self.repository
            || target.node_id() != self.pr_node
            || target.number() != self.number
            || m.command != COMMAND_V32
            || m.selected_resource != self.draft_key()?
            || m.observed_head != self.head
            || m.payload_sha256 != self.write(&values)?.digest()?
        {
            bail!("Base origin/database/target/command/reference differs");
        }
        validate_shape(m, m.state == PrWriteState::Confirmed)?;
        if m.state == PrWriteState::Confirmed
            && m.confirmed.as_ref().is_none_or(|e| {
                e.resource_id
                    != format!(
                        "metadata-base-v32/{}",
                        selected_digest(&self.requested_ref, &self.requested_tip)
                    )
            })
        {
            bail!("Base receipt differs from actual selected name/tip");
        }
        Ok(())
    }
}
pub(crate) fn effect(r: &PrMetadataBaseReceiptV32) -> PrConfirmedEffect {
    PrConfirmedEffect {
        kind: PrEffectKind::Metadata,
        resource_id: format!(
            "metadata-base-v32/{}",
            selected_digest(&r.parent().summary().base_ref, r.parent().base_tip_sha())
        ),
        node_id: Some(r.parent().node_id().into()),
    }
}
pub(crate) fn validate_shape(m: &PrWriteMarker, confirmed: bool) -> Result<()> {
    if m.command != COMMAND_V32
        || !m
            .selected_resource
            .strip_prefix("metadata-base-v32/")
            .is_some_and(digest)
    {
        bail!("Known Base purpose lacks exact original key digest");
    }
    if confirmed {
        let r = m
            .confirmed
            .as_ref()
            .filter(|r| r.kind == PrEffectKind::Metadata && m.state == PrWriteState::Confirmed)
            .context("Actual Base receipt absent")?;
        if !r
            .resource_id
            .strip_prefix("metadata-base-v32/")
            .is_some_and(digest)
            || r.node_id.as_deref() != Some(m.target.node_id())
        {
            bail!("Base receipt/node mismatch");
        }
    } else if !matches!(
        m.state,
        PrWriteState::Pending | PrWriteState::Uncertain | PrWriteState::Acknowledged
    ) || m.confirmed.is_some()
    {
        bail!("Possible Base effect requires unresolved original or explicit local ACK");
    }
    Ok(())
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BaseOriginRecordV32 {
    pub(crate) seed: RecordedBaseOriginV32,
    pub(crate) reference: PrPublicationReference,
}
impl BaseOriginRecordV32 {
    pub(super) fn matches_index_v32(&self, repository: u64, pr: u64) -> bool {
        self.seed.repository_id == repository && self.seed.pr_id == pr
    }
}
pub(super) fn migrate(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS pr_write_metadata_base_origins_v32(attempt_id INTEGER PRIMARY KEY REFERENCES pr_write_attempts(attempt_id) ON DELETE CASCADE,repository_id INTEGER NOT NULL,pr_id INTEGER NOT NULL,origin_json TEXT NOT NULL);CREATE TRIGGER IF NOT EXISTS pr_write_metadata_base_origins_cleanup_v32 AFTER DELETE ON pr_write_attempts BEGIN DELETE FROM pr_write_metadata_base_origins_v32 WHERE attempt_id=OLD.attempt_id; END;CREATE TRIGGER IF NOT EXISTS pr_write_metadata_base_origins_immutable_v32 BEFORE UPDATE ON pr_write_metadata_base_origins_v32 BEGIN SELECT RAISE(ABORT,'Immutable original Base');END;")?;
    Ok(())
}
pub(super) fn insert(
    tx: &Transaction<'_>,
    seed: &BaseOriginSeedV32,
    m: &PrWriteMarker,
) -> Result<()> {
    let data = bounded_encode(&BaseOriginRecordV32 {
        seed: seed.0.clone(),
        reference: m.publication_reference(),
    })?;
    tx.execute("INSERT INTO pr_write_metadata_base_origins_v32(attempt_id,repository_id,pr_id,origin_json) VALUES(?1,?2,?3,?4)",params![m.attempt_id,seed.0.repository_id as i64,seed.0.pr_id as i64,data])?;
    Ok(())
}
impl PrWriteJournal {
    pub(crate) fn begin_metadata_base_v32(
        &mut self,
        target: &PrWriteTarget,
        intent: &PrWriteIntent,
        seed: &BaseOriginSeedV32,
    ) -> Result<PrWritePermit> {
        let v = intent
            .values
            .iter()
            .filter(|(k, _)| matches!(k.as_str(), "base" | "tip"))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        if seed.write(&v)?.digest()? != intent.digest()? || !seed.0.matches_database(&self.path)? {
            bail!("Fresh Base seed differs from original publication");
        }
        self.begin_with_observed_origin_v2(
            target,
            intent,
            Some(ObservedOriginV2::MetadataBaseV32(seed)),
        )
    }
    pub(crate) fn metadata_base_origin_v32(
        &self,
        target: &PrWriteTarget,
        m: &PrWriteMarker,
    ) -> Result<BaseOriginRecordV32> {
        let foreign:usize=self.connection.query_row("SELECT (SELECT count(*) FROM pr_write_metadata_origins_v25 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_conversation_origins_v17 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_review_operation_origins_v3 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_thread_origins_v1 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_thread_comment_origins_v2 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_thread_reaction_origins_v3 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_file_comment_origins_v75 WHERE attempt_id=?1)",[m.attempt_id],|r|r.get(0))?;
        if foreign != 0 {
            bail!("Mixed original purposes cannot inspect or release Base");
        }
        let bytes:usize=self.connection.query_row("SELECT length(CAST(origin_json AS BLOB)) FROM pr_write_metadata_base_origins_v32 WHERE attempt_id=?1",[m.attempt_id],|r|r.get(0))
            .optional()?.context("Known Base publication lacks its exact original observation; private text remains hidden")?;
        if bytes > 16 * 1024 {
            bail!("Base original exceeds16KiB before copy");
        }
        let (repo,pr,raw):(u64,u64,String)=self.connection.query_row("SELECT repository_id,pr_id,origin_json FROM pr_write_metadata_base_origins_v32 WHERE attempt_id=?1",[m.attempt_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        let record: BaseOriginRecordV32 = serde_json::from_str(&raw)?;
        record.seed.validate_marker(&self.path, target, m)?;
        if record.reference != m.publication_reference()
            || record.seed.repository_id != repo
            || record.seed.pr_id != pr
        {
            bail!("Base original scalar/reference differs");
        }
        Ok(record)
    }
}
