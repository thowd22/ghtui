//! State originals in the SAME PR journal. Recorded JSON/body/reference is
//! inspect-only; only a genuine current producer can issue StateOriginSeedV68.
use super::*;
use crate::{
    pr_state_model::PrStateCommand,
    providers::gh::{OriginalPrStateV68, PrResourceParentV1},
    scoped_pr_collaboration_source_v17::PrCollaborationSourceV17,
};
use review_operation_history_v3::{bounded_encode, DatabaseV3};
#[path = "pr_write_state_lineage_v68.rs"]
pub(crate) mod lineage_v68;
#[path = "pr_write_state_source_terminal_v80.rs"]
pub(crate) mod source_terminal_v80;

const ORIGIN_CAP: usize = 16 * 1024;
pub(super) const NON_STATE_ATTEMPT_SQL_V68: &str = "command NOT LIKE 'state-%' AND selected_resource NOT LIKE 'state-v68/%' AND NOT EXISTS(SELECT 1 FROM pr_write_state_origins_v68 AS s WHERE s.attempt_id=pr_write_attempts.attempt_id) AND NOT EXISTS(SELECT 1 FROM pr_write_state_dispositions_v68 AS d WHERE d.attempt_id=pr_write_attempts.attempt_id)";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RecordedStateOriginV68 {
    version: u8,
    database: DatabaseV3,
    pub(crate) scope: Scope,
    pub(crate) repository_id: u64,
    repository_node: String,
    repository: String,
    pub(crate) pr_id: u64,
    pr_node: String,
    number: u64,
    pub(crate) issue_id: u64,
    issue_node: String,
    head: String,
    base: String,
    head_ref: String,
    base_ref: String,
    before: String,
    draft: bool,
    command: PrStateCommand,
    requester: String,
    lineage_v68: Option<conversation_action_lineage_v18::kernel_v52::CapturedV52>,
}
pub(crate) struct StateOriginSeedV68(RecordedStateOriginV68);
#[path = "pr_write_state_cache_repair_v74.rs"]
mod cache_repair_v74;
pub(crate) use cache_repair_v74::{StateCacheRepairV74, StateRepairOriginalV74};
#[path = "pr_write_state_history_v74.rs"]
pub(crate) mod history_v74;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StateOriginRecordV68 {
    pub(crate) seed: RecordedStateOriginV68,
    pub(crate) reference: PrPublicationReference,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RecordedStateDispositionV68 {
    version: u8,
    command: PrStateCommand,
    reference: PrPublicationReference,
    repository_id: u64,
    pr_id: u64,
    issue_id: u64,
    old_state: String,
    new_state: String,
    head: String,
    base: String,
    head_ref: String,
    base_ref: String,
    draft: bool,
    merged: bool,
}
impl RecordedStateDispositionV68 {
    fn validate(&self, seed: &RecordedStateOriginV68, marker: &PrWriteMarker) -> Result<()> {
        if self.version != 68
            || self.command != seed.command
            || self.reference != marker.publication_reference()
            || self.repository_id != seed.repository_id
            || self.pr_id != seed.pr_id
            || self.issue_id != seed.issue_id
            || self.old_state != seed.before
            || self.new_state
                != match self.command {
                    PrStateCommand::Close => "closed",
                    PrStateCommand::Reopen => "open",
                    _ => bail!("Unsupported State receipt"),
                }
            || self.head != seed.head
            || self.base != seed.base
            || self.head_ref != seed.head_ref
            || self.base_ref != seed.base_ref
            || self.draft != seed.draft
            || self.merged
            || marker.state != PrWriteState::Confirmed
        {
            bail!("Recorded State receipt differs from exact original requested transition");
        }
        Ok(())
    }
}
fn positive(id: u64) -> bool {
    (1..=i64::MAX as u64).contains(&id)
}
fn sha(value: &impl Serialize) -> Result<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(bounded_encode(value)?.as_bytes())
    ))
}
/// Exact bounded raw origin + disposition hashing without constructing a
/// second escaped JSON String/Vec. The <=16KiB origin and <=8KiB disposition
/// have each already passed typed validation; their serialized string overlap
/// is bounded conservatively at64KiB before any digest bytes are accepted.
fn raw_state_hash_v68(value: &impl Serialize) -> Result<String> {
    struct Hash {
        digest: Sha256,
        bytes: usize,
    }
    impl std::io::Write for Hash {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.bytes = self
                .bytes
                .checked_add(bytes.len())
                .filter(|n| *n <= 64 * 1024)
                .ok_or_else(|| std::io::Error::other("Exact raw State witness exceeds64KiB"))?;
            self.digest.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut hash = Hash {
        digest: Sha256::new(),
        bytes: 0,
    };
    serde_json::to_writer(&mut hash, value)?;
    Ok(format!("{:x}", hash.digest.finalize()))
}
pub(crate) fn recognized(command: &str, selected: &str) -> bool {
    command.starts_with("state-") || selected.starts_with("state-v68/")
}
fn command_supported(command: PrStateCommand) -> bool {
    matches!(command, PrStateCommand::Close | PrStateCommand::Reopen)
}
pub(crate) fn state_draft_key_v68(
    scope: &Scope,
    parent: &PrResourceParentV1,
    command: PrStateCommand,
) -> Result<String> {
    if !command_supported(command) {
        bail!("State command has no native producer");
    }
    let stable = (
        scope,
        parent.repository_id(),
        parent.repository_node(),
        parent.database_id(),
        parent.node_id(),
        parent.number(),
        command,
    );
    // Head-bound draft storage is NOT the stable action lineage base. The
    // shared <=3 witness kernel must use stable_base_v68 below, never this key.
    Ok(format!(
        "state-v68/{}",
        sha(&(stable, parent.head_sha(), parent.base_tip_sha()))?
    ))
}
impl RecordedStateOriginV68 {
    pub(crate) fn same_object_v74(&self, parent: &PrResourceParentV1) -> bool {
        self.repository_id == parent.repository_id()
            && self.repository_node == parent.repository_node()
            && self.repository == parent.repository()
            && self.pr_id == parent.database_id()
            && self.pr_node == parent.node_id()
            && self.number == parent.number()
    }
    pub(crate) fn matches_actual_v68(&self, actual: &OriginalPrStateV68) -> bool {
        let p = actual.observed().parent();
        self.command == actual.observed().command()
            && self.repository_id == p.repository_id()
            && self.repository_node == p.repository_node()
            && self.repository == p.repository()
            && self.pr_id == p.database_id()
            && self.pr_node == p.node_id()
            && self.number == p.number()
            && self.issue_id == actual.observed().issue_id()
            && self.issue_node == actual.observed().issue_node()
            && self.head == p.head_sha()
            && self.base == p.base_tip_sha()
            && self.head_ref == p.summary().head_ref
            && self.base_ref == p.summary().base_ref
            && self.before == p.summary().pr.state
            && self.draft == p.summary().pr.draft
    }
    fn validate(&self) -> Result<()> {
        if self.version != 68
            || !command_supported(self.command)
            || !positive(self.repository_id)
            || !positive(self.pr_id)
            || !positive(self.issue_id)
            || !positive(self.number)
            || self.scope.host.len() > 256
            || self.requester.len() > 100
            || self.repository.len() > 256
            || self.requester.is_empty()
            || self.requester.chars().any(char::is_control)
            || !matches!(self.before.as_str(), "open" | "closed")
            || self.command == PrStateCommand::Close && self.before != "open"
            || self.command == PrStateCommand::Reopen && self.before != "closed"
        {
            bail!("Invalid closed original State identity/transition");
        }
        validate_repository(&self.repository)?;
        for value in [
            &self.repository_node,
            &self.pr_node,
            &self.issue_node,
            &self.head_ref,
            &self.base_ref,
        ] {
            if value.is_empty() || value.len() > 1024 || value.chars().any(char::is_control) {
                bail!("Invalid bounded State node/ref");
            }
        }
        for value in [&self.head, &self.base] {
            if !matches!(value.len(), 40 | 64) || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
                bail!("Original State head/base is not a full immutable Git identity");
            }
        }
        let account = self
            .scope
            .login
            .strip_prefix("user-id:")
            .and_then(|s| s.parse().ok())
            .filter(|id| positive(*id))
            .context("Positive State principal absent")?;
        if Scope::github(&self.scope.host, Some(account))? != self.scope {
            bail!("Noncanonical State principal scope");
        }
        bounded_encode(self)?;
        if self.lineage_v68.is_some() {
            lineage_v68::validate_record(self)?;
        }
        Ok(())
    }
    fn stable_base_unchecked_v68(&self) -> Result<String> {
        Ok(format!(
            "state-base-v68/{}",
            sha(&(
                &self.scope,
                self.repository_id,
                &self.repository_node,
                self.pr_id,
                &self.pr_node,
                self.number,
                self.command
            ))?
        ))
    }
    pub(crate) fn stable_base_v68(&self) -> Result<String> {
        self.validate()?;
        // No mutable H/B/state/current set/time/draft/read salt.
        self.stable_base_unchecked_v68()
    }
    pub(crate) fn draft_key(&self) -> Result<String> {
        self.validate()?;
        if let Some(lineage) = &self.lineage_v68 {
            return Ok(lineage.key.clone());
        }
        let stable = (
            &self.scope,
            self.repository_id,
            &self.repository_node,
            self.pr_id,
            &self.pr_node,
            self.number,
            self.command,
        );
        Ok(format!(
            "state-v68/{}",
            sha(&(stable, &self.head, &self.base))?
        ))
    }
    pub(crate) fn write(&self, values: &BTreeMap<String, String>) -> Result<PrWriteIntent> {
        self.validate()?;
        if !values.is_empty() {
            bail!("Close/Reopen is an exact bodyless draft purpose");
        }
        Ok(PrWriteIntent {
            command: self.command.key().into(),
            selected_resource: self.draft_key()?,
            observed_head: self.head.clone(),
            values: BTreeMap::from([("origin-v68".into(), sha(self)?)]),
        })
    }
    pub(crate) fn validate_marker(
        &self,
        database: &Path,
        target: &PrWriteTarget,
        marker: &PrWriteMarker,
    ) -> Result<()> {
        self.validate()?;
        if self.database != DatabaseV3::capture(database)?
            || target != &marker.target
            || target.scope() != &self.scope
            || target.repository() != self.repository
            || target.node_id() != self.pr_node
            || target.number() != self.number
            || marker.command != self.command.key()
            || marker.selected_resource != self.draft_key()?
            || marker.observed_head != self.head
            || marker.payload_sha256 != self.write(&BTreeMap::new())?.digest()?
        {
            bail!("State original/database/target/bodyless fingerprint differs");
        }
        if marker.state == PrWriteState::Confirmed {
            let effect = marker.confirmed.as_ref().context("State effect absent")?;
            if effect.kind != PrEffectKind::Lifecycle
                || effect.node_id.as_deref() != Some(self.pr_node.as_str())
                || effect.resource_id
                    != match self.command {
                        PrStateCommand::Close => "state-v68/close/closed",
                        PrStateCommand::Reopen => "state-v68/reopen/open",
                        _ => bail!("Unsupported State effect"),
                    }
            {
                bail!("State receipt does not bind requested original transition");
            }
        } else if marker.confirmed.is_some() {
            bail!("Unconfirmed State cannot carry a confirmed effect");
        }
        Ok(())
    }
}
impl StateOriginSeedV68 {
    pub(crate) fn draft_key_v68(&self) -> Result<String> {
        self.0.draft_key()
    }
    pub(crate) fn matches_receipt_v68(
        &self,
        receipt: &crate::providers::gh::StateReceiptV68,
    ) -> Result<bool> {
        self.0.validate()?;
        let transition = receipt.transition();
        let old = transition.old();
        let new = transition.parent();
        Ok(transition.command() == self.0.command
            && transition.issue_id() == self.0.issue_id
            && transition.issue_node() == self.0.issue_node
            && old.repository_id() == self.0.repository_id
            && old.repository_node() == self.0.repository_node
            && old.repository() == self.0.repository
            && old.database_id() == self.0.pr_id
            && old.node_id() == self.0.pr_node
            && old.number() == self.0.number
            && old.head_sha() == self.0.head
            && old.base_tip_sha() == self.0.base
            && old.summary().head_ref == self.0.head_ref
            && old.summary().base_ref == self.0.base_ref
            && old.summary().pr.state == self.0.before
            && old.summary().pr.draft == self.0.draft
            && !old.summary().merged
            && new.summary().pr.state
                == match self.0.command {
                    PrStateCommand::Close => "closed",
                    PrStateCommand::Reopen => "open",
                    _ => return Ok(false),
                }
            && new.repository_id() == self.0.repository_id
            && new.database_id() == self.0.pr_id
            && new.node_id() == self.0.pr_node
            && new.head_sha() == self.0.head
            && new.base_tip_sha() == self.0.base
            && new.summary().head_ref == self.0.head_ref
            && new.summary().base_ref == self.0.base_ref
            && new.summary().pr.draft == self.0.draft
            && !new.summary().merged)
    }
    pub(crate) fn from_producer(
        source: &PrCollaborationSourceV17,
        actual: &OriginalPrStateV68,
        database: &Path,
    ) -> Result<Self> {
        actual
            .validate_live_source_v68()
            .map_err(anyhow::Error::msg)?;
        actual.require().map_err(anyhow::Error::msg)?;
        let parent = actual.observed().parent();
        if !source
            .opening()
            .admits_state_v68(actual.observed().command())
            || source.opening().database() != database
            || !GithubProvider::same_thread_history_parent_v1(source.parent(), parent)
            || Scope::github(&actual.identity().host, actual.identity().account_id)?
                != *source.opening().scope()
        {
            bail!("State seed lacks exact genuine source/actual parent");
        }
        Ok(Self(record_from_actual_v68(database, actual)?))
    }
    pub(crate) fn write(&self, values: &BTreeMap<String, String>) -> Result<PrWriteIntent> {
        self.0.write(values)
    }
}
fn record_from_actual_v68(
    database: &Path,
    actual: &OriginalPrStateV68,
) -> Result<RecordedStateOriginV68> {
    actual
        .validate_live_source_v68()
        .map_err(anyhow::Error::msg)?;
    actual.require().map_err(anyhow::Error::msg)?;
    let parent = actual.observed().parent();
    let record = RecordedStateOriginV68 {
        version: 68,
        database: DatabaseV3::capture(database)?,
        scope: Scope::github(&actual.identity().host, actual.identity().account_id)?,
        repository_id: parent.repository_id(),
        repository_node: parent.repository_node().into(),
        repository: parent.repository().into(),
        pr_id: parent.database_id(),
        pr_node: parent.node_id().into(),
        number: parent.number(),
        issue_id: actual.observed().issue_id(),
        issue_node: actual.observed().issue_node().into(),
        head: parent.head_sha().into(),
        base: parent.base_tip_sha().into(),
        head_ref: parent.summary().head_ref.clone(),
        base_ref: parent.summary().base_ref.clone(),
        before: parent.summary().pr.state.clone(),
        draft: parent.summary().pr.draft,
        command: actual.observed().command(),
        requester: actual.identity().login.clone(),
        lineage_v68: None,
    };
    record.validate()?;
    Ok(record)
}

pub(super) fn migrate(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS pr_write_state_origins_v68(
      attempt_id INTEGER PRIMARY KEY REFERENCES pr_write_attempts(attempt_id) ON DELETE CASCADE,
      repository_id INTEGER NOT NULL CHECK(typeof(repository_id)='integer' AND repository_id>0),
      pr_id INTEGER NOT NULL CHECK(typeof(pr_id)='integer' AND pr_id>0),
      issue_id INTEGER NOT NULL CHECK(typeof(issue_id)='integer' AND issue_id>0),
      command TEXT NOT NULL CHECK(command IN('state-close','state-reopen')),
      base_key_v71 TEXT NOT NULL CHECK(typeof(base_key_v71)='text' AND length(CAST(base_key_v71 AS BLOB))<=256),
      action_tag_v71 TEXT NOT NULL CHECK(action_tag_v71 IN('state-close','state-reopen')),
      parent_ack_attempt_v71 INTEGER CHECK(parent_ack_attempt_v71 IS NULL OR(typeof(parent_ack_attempt_v71)='integer' AND parent_ack_attempt_v71>0)),
      origin_json TEXT NOT NULL CHECK(typeof(origin_json)='text' AND length(CAST(origin_json AS BLOB))<=16384));
      CREATE INDEX IF NOT EXISTS pr_write_state_lineage_v71 ON pr_write_state_origins_v68(base_key_v71,action_tag_v71,attempt_id DESC);
      CREATE TABLE IF NOT EXISTS pr_write_state_dispositions_v68(
      attempt_id INTEGER PRIMARY KEY REFERENCES pr_write_state_origins_v68(attempt_id) ON DELETE CASCADE,
      kind TEXT NOT NULL CHECK(kind IN('confirmed_effect','accepted_pending')),
      receipt_json TEXT NOT NULL CHECK(typeof(receipt_json)='text' AND length(CAST(receipt_json AS BLOB))<=8192));
      CREATE TRIGGER IF NOT EXISTS pr_write_state_origins_immutable_v68 BEFORE UPDATE ON pr_write_state_origins_v68
      BEGIN SELECT RAISE(ABORT,'Immutable original PR state'); END;
      CREATE TRIGGER IF NOT EXISTS pr_write_state_dispositions_immutable_v68 BEFORE UPDATE ON pr_write_state_dispositions_v68
      BEGIN SELECT RAISE(ABORT,'Immutable original PR state disposition'); END;
      CREATE TRIGGER IF NOT EXISTS pr_write_state_origins_cleanup_v68 AFTER DELETE ON pr_write_attempts
      BEGIN DELETE FROM pr_write_state_dispositions_v68 WHERE attempt_id=OLD.attempt_id;
      DELETE FROM pr_write_state_origins_v68 WHERE attempt_id=OLD.attempt_id; END;")?;
    Ok(())
}
pub(super) fn insert(
    tx: &Transaction<'_>,
    seed: &StateOriginSeedV68,
    marker: &PrWriteMarker,
) -> Result<()> {
    let record = StateOriginRecordV68 {
        seed: seed.0.clone(),
        reference: marker.publication_reference(),
    };
    let encoded = bounded_encode(&record)?;
    tx.execute("INSERT INTO pr_write_state_origins_v68(attempt_id,repository_id,pr_id,issue_id,command,base_key_v71,action_tag_v71,parent_ack_attempt_v71,origin_json) VALUES(?1,?2,?3,?4,?5,?6,?5,?7,?8)",
        params![marker.attempt_id,seed.0.repository_id as i64,seed.0.pr_id as i64,seed.0.issue_id as i64,seed.0.command.key(),seed.0.stable_base_v68()?,seed.0.lineage_v68.as_ref().and_then(|l|l.parent_ack),encoded])?;
    Ok(())
}
pub(super) fn reject_non_state_marker(
    connection: &Connection,
    marker: &PrWriteMarker,
) -> Result<()> {
    let exists:bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM pr_write_state_origins_v68 WHERE attempt_id=?1) OR EXISTS(SELECT 1 FROM pr_write_state_dispositions_v68 WHERE attempt_id=?1)",[marker.attempt_id],|r|r.get(0))?;
    if recognized(&marker.command, &marker.selected_resource) || exists {
        bail!("Known State original requires its exact typed State receipt/inspection; generic completion/ACK refused");
    }
    Ok(())
}
pub(super) fn reject_non_state_target(
    connection: &Connection,
    target: &PrWriteTarget,
) -> Result<()> {
    let exists:bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM pr_write_attempts AS a WHERE a.host=?1 AND a.principal=?2 AND a.pr_node=?3 AND a.state IN('pending','uncertain') AND (a.command LIKE 'state-%' OR a.selected_resource LIKE 'state-v68/%' OR EXISTS(SELECT 1 FROM pr_write_state_origins_v68 AS s WHERE s.attempt_id=a.attempt_id) OR EXISTS(SELECT 1 FROM pr_write_state_dispositions_v68 AS d WHERE d.attempt_id=a.attempt_id)))",
        params![target.scope.host,target.scope.login,target.pr_node_id],|r|r.get(0))?;
    if exists {
        bail!("Known State blocker cannot use generic/foreign-purpose ACK");
    }
    Ok(())
}
impl PrWriteJournal {
    /// Routing metadata ONLY from a genuine captured State opening. A true
    /// result selects fresh read-only history; it issues no target/source or
    /// permission and cannot turn malformed/foreign history into a fallback.
    pub(crate) fn state_open_requires_history_v75(
        &self,
        opening: &crate::pr_collaboration_origin_v17::PrCollaborationOpenIntentV17,
        command: PrStateCommand,
    ) -> Result<bool> {
        if self.path != opening.database() || !opening.admits_state_v68(command) {
            bail!("Exact current State opening required for history routing");
        }
        self.connection.query_row("SELECT EXISTS(SELECT 1 FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND pr_node=?3 AND state IN('pending','uncertain'))",params![opening.scope().host,opening.scope().login,opening.parent().node_id()],|r|r.get(0)).map_err(Into::into)
    }
    pub(crate) fn reject_non_state_cache_repair_v74(&self, marker: &PrWriteMarker) -> Result<()> {
        reject_non_state_marker(&self.connection, marker)
    }
    pub(crate) fn reject_non_state_cache_reference_v74(
        &self,
        target: &PrWriteTarget,
        reference: &PrPublicationReference,
    ) -> Result<()> {
        let state:bool=self.connection.query_row("SELECT EXISTS(SELECT 1 FROM pr_write_state_origins_v68 WHERE attempt_id=?1) OR EXISTS(SELECT 1 FROM pr_write_state_dispositions_v68 WHERE attempt_id=?1) OR EXISTS(SELECT 1 FROM pr_write_attempts a WHERE a.attempt_id=?1 AND a.host=?2 AND a.principal=?3 AND a.pr_node=?4 AND (a.command LIKE 'state-%' OR a.selected_resource LIKE 'state-v68/%'))",params![reference.attempt_id,target.scope.host,target.scope.login,target.pr_node_id],|r|r.get(0))?;
        if state {
            bail!("Known State cache repair requires its closed State witness before legacy receipt/body parsing");
        }
        Ok(())
    }
    /// State cannot enter ordinary mark_confirmed: the SAME Immediate commits
    /// its exact actual typed receipt AND journal effect, or neither.
    pub(crate) fn mark_state_confirmed_v68(
        &mut self,
        permit: &PrWritePermit,
        seed: &StateOriginSeedV68,
        receipt: &crate::providers::gh::StateReceiptV68,
        expected_generation: i64,
        cancel: &CancellationToken,
        deadline: std::time::Instant,
    ) -> Result<PrConfirmedEffect> {
        let deadline = deadline.min(receipt.publication_cutoff_v68());
        let path = &self.path;
        let check = || -> Result<()> {
            receipt
                .validate_commit_v68(cancel, deadline)
                .map_err(anyhow::Error::msg)?;
            if cancel.is_cancelled() || std::time::Instant::now() >= deadline {
                bail!("State receipt commit cancelled or expired");
            }
            if DatabaseV3::capture(path)? != seed.0.database {
                bail!("State receipt original database replaced");
            }
            Ok(())
        };
        check()?;
        if permit.issuer != self.path
            || !permit.claimed.load(Ordering::Acquire)
            || receipt.reference() != &permit.marker.publication_reference()
            || !seed.matches_receipt_v68(receipt)?
        {
            bail!(
                "Only the claimed original State permit and exact actual receipt may finish State"
            );
        }
        let original = &permit.marker;
        seed.0
            .validate_marker(&self.path, &original.target, original)?;
        let effect = receipt.effect();
        let mut confirmed = original.clone();
        confirmed.state = PrWriteState::Confirmed;
        confirmed.confirmed = Some(effect.clone());
        seed.0
            .validate_marker(&self.path, &original.target, &confirmed)?;
        let record = StateOriginRecordV68 {
            seed: seed.0.clone(),
            reference: original.publication_reference(),
        };
        let raw_origin = bounded_encode(&record)?;
        let transition = receipt.transition();
        let disposition = RecordedStateDispositionV68 {
            version: 68,
            command: seed.0.command,
            reference: original.publication_reference(),
            repository_id: seed.0.repository_id,
            pr_id: seed.0.pr_id,
            issue_id: seed.0.issue_id,
            old_state: seed.0.before.clone(),
            new_state: transition.parent().summary().pr.state.clone(),
            head: seed.0.head.clone(),
            base: seed.0.base.clone(),
            head_ref: seed.0.head_ref.clone(),
            base_ref: seed.0.base_ref.clone(),
            draft: seed.0.draft,
            merged: false,
        };
        disposition.validate(&seed.0, &confirmed)?;
        let raw_receipt = bounded_encode(&disposition)?;
        if raw_receipt.len() > 8192 {
            bail!("State disposition exceeds8KiB");
        }
        let fence_advance = cache_repair_v74::StateConfirmedFenceAdvanceV74::prepare(
            &self.connection,
            seed,
            receipt,
            &raw_receipt,
        )?;
        let raw_effect = serde_json::to_string(&effect)?;
        let base = seed.0.stable_base_v68()?;
        let parent = seed.0.lineage_v68.as_ref().and_then(|l| l.parent_ack);
        check()?;
        let _busy = crate::storage::busy_v1::until(&self.connection, deadline)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        check()?;
        if generation(&tx, &original.target)? != expected_generation {
            bail!("State receipt generation changed before typed commit");
        }
        let current = read_origin_v68(&tx, &self.path, &original.target, original)?.0;
        if bounded_encode(&current)? != raw_origin {
            bail!("State origin changed before typed receipt commit");
        }
        lineage_v68::validate_owned_in_transaction_v68(&tx, &self.path, seed, original.attempt_id)?;
        let changed=tx.execute("UPDATE pr_write_attempts SET state='confirmed',confirmed_json=?1 WHERE attempt_id=?2 AND host=?3 AND principal=?4 AND pr_node=?5 AND repository=?6 AND pr_number=?7 AND command=?8 AND selected_resource=?9 AND observed_head=?10 AND payload_sha256=?11 AND state='pending' AND confirmed_json IS NULL AND created_at=?12 AND EXISTS(SELECT 1 FROM pr_write_state_origins_v68 AS s WHERE s.attempt_id=pr_write_attempts.attempt_id AND typeof(s.repository_id)='integer' AND s.repository_id=?13 AND typeof(s.pr_id)='integer' AND s.pr_id=?14 AND typeof(s.issue_id)='integer' AND s.issue_id=?15 AND typeof(s.command)='text' AND CAST(s.command AS BLOB)=CAST(?8 AS BLOB) AND typeof(s.origin_json)='text' AND CAST(s.origin_json AS BLOB)=CAST(?16 AS BLOB) AND typeof(s.base_key_v71)='text' AND CAST(s.base_key_v71 AS BLOB)=CAST(?17 AS BLOB) AND typeof(s.action_tag_v71)='text' AND CAST(s.action_tag_v71 AS BLOB)=CAST(?8 AS BLOB) AND typeof(s.parent_ack_attempt_v71)=typeof(?18) AND s.parent_ack_attempt_v71 IS ?18) AND NOT EXISTS(SELECT 1 FROM pr_write_state_dispositions_v68 AS d WHERE d.attempt_id=pr_write_attempts.attempt_id) AND NOT EXISTS(SELECT 1 FROM pr_write_issue_link_origins_v87 AS i WHERE i.attempt_id=pr_write_attempts.attempt_id)",
            params![raw_effect,original.attempt_id,original.target.scope.host,original.target.scope.login,original.target.pr_node_id,
                original.target.repository,original.target.number,original.command,original.selected_resource,original.observed_head,
                original.payload_sha256,original.created_at,seed.0.repository_id,seed.0.pr_id,seed.0.issue_id,raw_origin,base,parent])?;
        if changed != 1 {
            bail!("Exact State pending row/origin/index changed; no half-confirmed effect");
        }
        tx.execute("INSERT INTO pr_write_state_dispositions_v68(attempt_id,kind,receipt_json) VALUES(?1,'confirmed_effect',?2)",params![original.attempt_id,raw_receipt])?;
        fence_advance.apply(&tx)?;
        advance(&tx, &original.target)?;
        check()?;
        tx.commit()?;
        Ok(effect)
    }
    pub(crate) fn begin_state_v68(
        &mut self,
        target: &PrWriteTarget,
        intent: &PrWriteIntent,
        seed: &StateOriginSeedV68,
    ) -> Result<PrWritePermit> {
        if seed.0.database != DatabaseV3::capture(&self.path)?
            || seed.0.scope != *target.scope()
            || seed.0.repository != target.repository()
            || seed.0.pr_node != target.node_id()
            || seed.0.number != target.number()
            || seed.write(&BTreeMap::new())?.digest()? != intent.digest()?
        {
            bail!("Actual State seed does not bind this original bodyless publication");
        }
        self.begin_with_observed_origin_v2(target, intent, Some(ObservedOriginV2::StateV68(seed)))
    }
    pub(crate) fn state_origin_v68(
        &self,
        target: &PrWriteTarget,
        marker: &PrWriteMarker,
    ) -> Result<StateOriginRecordV68> {
        read_origin_v68(&self.connection, &self.path, target, marker).map(|v| v.0)
    }
}
fn read_origin_v68(
    connection: &Connection,
    database: &Path,
    target: &PrWriteTarget,
    marker: &PrWriteMarker,
) -> Result<(StateOriginRecordV68, String)> {
    let foreign:usize=connection.query_row("SELECT (SELECT count(*) FROM pr_write_conversation_origins_v17 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_review_operation_origins_v3 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_thread_origins_v1 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_thread_comment_origins_v2 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_thread_reaction_origins_v3 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_file_comment_origins_v75 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_metadata_origins_v25 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_metadata_base_origins_v32 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_issue_link_origins_v87 WHERE attempt_id=?1)",[marker.attempt_id],|r|r.get(0))?;
    if foreign != 0 {
        bail!("State original has mixed foreign purposes");
    }
    let valid:bool=connection.query_row("SELECT typeof(repository_id)='integer' AND repository_id>0 AND typeof(pr_id)='integer' AND pr_id>0 AND typeof(issue_id)='integer' AND issue_id>0 AND typeof(command)='text' AND length(CAST(command AS BLOB))<=64 AND typeof(origin_json)='text' AND length(CAST(origin_json AS BLOB))<=?2 AND typeof(base_key_v71)='text' AND length(CAST(base_key_v71 AS BLOB))<=256 AND typeof(action_tag_v71)='text' AND length(CAST(action_tag_v71 AS BLOB))<=64 AND (typeof(parent_ack_attempt_v71)='null' OR(typeof(parent_ack_attempt_v71)='integer' AND parent_ack_attempt_v71>0)) FROM pr_write_state_origins_v68 WHERE attempt_id=?1",params![marker.attempt_id,ORIGIN_CAP as i64],|r|r.get(0))?;
    if !valid {
        bail!("State original scalar/types exceed pre-copy bound");
    }
    let (repository,pr,issue,command,raw,key,tag,parent):(u64,u64,u64,String,String,String,String,Option<i64>)=connection.query_row("SELECT repository_id,pr_id,issue_id,command,origin_json,base_key_v71,action_tag_v71,parent_ack_attempt_v71 FROM pr_write_state_origins_v68 WHERE attempt_id=?1",[marker.attempt_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?)))?;
    let record: StateOriginRecordV68 = serde_json::from_str(&raw)?;
    record.seed.validate_marker(database, target, marker)?;
    if record.reference != marker.publication_reference()
        || repository != record.seed.repository_id
        || pr != record.seed.pr_id
        || issue != record.seed.issue_id
        || command != record.seed.command.key()
        || key != record.seed.stable_base_v68()?
        || tag != record.seed.command.key()
        || parent != record.seed.lineage_v68.as_ref().and_then(|l| l.parent_ack)
    {
        bail!("State raw index/reference differs from closed original");
    }
    let disposition_bytes:Option<usize>=connection.query_row("SELECT length(CAST(receipt_json AS BLOB))+length(CAST(kind AS BLOB)) FROM pr_write_state_dispositions_v68 WHERE attempt_id=?1",[marker.attempt_id],|r|r.get(0)).optional()?;
    if disposition_bytes.is_some_and(|n| n > 8192 + 32) {
        bail!("State disposition exceeds pre-copy8KiB profile");
    }
    let disposition:Option<(String,String)>=connection.query_row("SELECT kind,receipt_json FROM pr_write_state_dispositions_v68 WHERE attempt_id=?1 AND typeof(kind)='text' AND typeof(receipt_json)='text' AND length(CAST(receipt_json AS BLOB))<=8192",[marker.attempt_id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    if disposition_bytes.is_some() != disposition.is_some() {
        bail!("State disposition raw type/absence differs");
    }
    match (&disposition, marker.state) {
        (Some((kind, receipt)), PrWriteState::Confirmed) if kind == "confirmed_effect" => {
            let receipt: RecordedStateDispositionV68 = serde_json::from_str(receipt)?;
            receipt.validate(&record.seed, marker)?;
        }
        (None, PrWriteState::Pending | PrWriteState::Uncertain | PrWriteState::Acknowledged) => {}
        _ => bail!("Unsupported acceptance/contradictory State disposition; no generic release"),
    }
    let raw_hash = raw_state_hash_v68(&(
        repository,
        pr,
        issue,
        command,
        raw,
        key,
        tag,
        parent,
        disposition,
    ))?;
    Ok((record, raw_hash))
}
