//! Explicit new Conversation actions. These local lineage descriptions are NOT
//! source/capability, restored approval, invocation absence, retry or body replay.
//! Only a genuine live original read issues Base; decoded history never can.
use super::*;
use crate::{
    pr_actions::PrCommand,
    scoped_pr_collaboration_source_v17::{
        initial_v17::InitialReadV17, ConversationOpenObservationV17,
    },
};
use conversation_history_v17::{ConversationActionV17, ConversationOriginSeedV17};
#[path = "pr_write_action_lineage_kernel_v52.rs"]
pub(in crate::pr_write_journal) mod kernel_v52;
use review_operation_history_v3::{bounded_encode, DatabaseV3};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::pr_write_journal) struct ActionObservationV18 {
    scope: Scope,
    repository_id: u64,
    repository_node: String,
    repository: String,
    pr_id: u64,
    pr_node: String,
    number: u64,
    head: String,
    base: String,
    base_key: String,
    action: ConversationActionV17,
    selected: Option<(u64, String)>,
}
#[derive(Clone)]
pub(crate) struct ConversationActionBaseV18 {
    database: DatabaseV3,
    observation: ActionObservationV18,
    target: PrWriteTarget,
}
impl ConversationActionBaseV18 {
    /// No tuple/history constructor. The borrowed read exists ONLY inside the
    /// original queued Core closure, before parent/SQL/final-principal checks.
    pub(crate) fn from_open_read(
        read: &InitialReadV17<'_>,
        command: PrCommand,
        observed: &ConversationOpenObservationV17,
    ) -> Result<Self> {
        let action = ConversationActionV17::from_command(command)?;
        let selected = match observed {
            ConversationOpenObservationV17::New if action == ConversationActionV17::Create => None,
            ConversationOpenObservationV17::Subscription(value)
                if action == ConversationActionV17::Subscription =>
            {
                if !GithubProvider::same_thread_history_parent_v1(read.parent, value.parent()) {
                    bail!("Current subscription parent differs from the actual read");
                }
                None
            }
            ConversationOpenObservationV17::Comment(value)
                if matches!(
                    action,
                    ConversationActionV17::Edit | ConversationActionV17::Delete
                ) =>
            {
                value.require(command)?;
                if !GithubProvider::same_thread_history_parent_v1(read.parent, value.parent()) {
                    bail!("Current selected comment parent differs from the actual read");
                }
                Some((value.observed().id, value.observed().node_id.clone()))
            }
            ConversationOpenObservationV17::Reactions(value)
                if matches!(
                    action,
                    ConversationActionV17::AddReaction | ConversationActionV17::RemoveReaction
                ) =>
            {
                value.original().require(command)?;
                if !GithubProvider::same_thread_history_parent_v1(
                    read.parent,
                    value.original().parent(),
                ) || action == ConversationActionV17::RemoveReaction && !value.complete()
                {
                    bail!("Actual complete current-owned reaction catalogue required");
                }
                Some((
                    value.original().observed().id,
                    value.original().observed().node_id.clone(),
                ))
            }
            _ => bail!("Current action and genuine observed purpose differ"),
        };
        let scope = Scope::github(&read.identity.host, read.identity.account_id)?;
        let target = PrWriteTarget::new(read.identity, read.parent.summary())?;
        let base_key = conversation_history_v17::draft_key_observed_v18(
            &scope,
            read.parent,
            action,
            selected.as_ref().map(|(id, node)| (*id, node.as_str())),
        )?;
        let parent = read.parent;
        let observation = ActionObservationV18 {
            scope,
            repository_id: parent.repository_id(),
            repository_node: parent.repository_node().into(),
            repository: parent.repository().into(),
            pr_id: parent.database_id(),
            pr_node: parent.node_id().into(),
            number: parent.number(),
            head: parent.head_sha().into(),
            base: parent.base_tip_sha().into(),
            base_key,
            action,
            selected,
        };
        bounded_encode(&observation)?;
        Ok(Self {
            database: DatabaseV3::capture(read.original_database_v18())?,
            observation,
            target,
        })
    }
    pub(crate) fn key(&self) -> &str {
        &self.observation.base_key
    }
    pub(crate) fn scope(&self) -> &Scope {
        &self.observation.scope
    }
    pub(crate) fn target(&self) -> &PrWriteTarget {
        &self.target
    }
    fn matches_seed(&self, seed: &ConversationOriginSeedV17) -> Result<()> {
        matches_observation(&self.observation, seed)
    }
}
fn matches_observation(
    observed: &ActionObservationV18,
    seed: &ConversationOriginSeedV17,
) -> Result<()> {
    seed.validate()?;
    if seed.form_key()? != observed.base_key
        || seed.scope != observed.scope
        || seed.repository_id != observed.repository_id
        || seed.repository_node != observed.repository_node
        || seed.repository != observed.repository
        || seed.pr_id != observed.pr_id
        || seed.pr_node != observed.pr_node
        || seed.number != observed.number
        || seed.head != observed.head
        || seed.base != observed.base
        || seed.action != observed.action
        || seed.selected.as_ref().map(|s| (s.id, s.node.as_str()))
            != observed
                .selected
                .as_ref()
                .map(|(id, node)| (*id, node.as_str()))
    {
        bail!("Actual fresh producer differs from the original new-action base");
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::pr_write_journal) enum TerminalKindV18 {
    Acknowledged,
    Confirmed,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::pr_write_journal) struct TerminalV18 {
    pub(in crate::pr_write_journal) reference: PrPublicationReference,
    pub(in crate::pr_write_journal) kind: TerminalKindV18,
    pub(in crate::pr_write_journal) receipt: Option<PrConfirmedEffect>,
    pub(in crate::pr_write_journal) created_at: u64,
    pub(in crate::pr_write_journal) raw_attempt_sha256: String,
    pub(in crate::pr_write_journal) raw_origin_sha256: String,
}
/// Non-Clone inner snapshot; Arc cloning grants no new clock or SQL permission.
pub(crate) struct ConversationTerminalRevisionV18 {
    database: DatabaseV3,
    base: ConversationActionBaseV18,
    generation: i64,
    terminal: TerminalV18,
    key: String,
    parent_ack: Option<i64>,
    root_ack: Option<TerminalV18>,
    protected_key: String,
    protected_fingerprint: Option<String>,
}
impl ConversationTerminalRevisionV18 {
    pub(crate) fn key(&self) -> &str {
        &self.key
    }
    pub(crate) fn reference(&self) -> &PrPublicationReference {
        &self.terminal.reference
    }
    pub(crate) fn label(&self) -> &'static str {
        match self.terminal.kind {
            TerminalKindV18::Acknowledged => {
                "Explicit local acknowledgement; outcome may remain unknown"
            }
            TerminalKindV18::Confirmed if self.protected_fingerprint.is_some() => {
                "Confirmed original receipt; retained protected draft (NOT acknowledged)"
            }
            TerminalKindV18::Confirmed => {
                "Confirmed original receipt; original draft is absent (NOT acknowledged)"
            }
        }
    }
}
fn next_key(observation: &ActionObservationV18, terminal: &TerminalV18) -> Result<String> {
    let encoded = bounded_encode(&(
        "ghtui-explicit-new-conversation-action-v18",
        &observation.base_key,
        observation.action,
        &terminal.reference,
    ))?;
    Ok(format!(
        "{}/new-v18/{:x}",
        observation.base_key,
        Sha256::digest(encoded.as_bytes())
    ))
}
fn bind_revision(
    observation: &ActionObservationV18,
    terminal: &TerminalV18,
    key: &str,
    write: &mut PrWriteIntent,
) -> Result<()> {
    if write.command != observation.action.command()
        || write.observed_head != observation.head
        || next_key(observation, terminal)? != key
    {
        bail!("Exact terminal/new-action write binding differs");
    }
    write.values.insert(
        "explicit-new-conversation-v18".into(),
        format!(
            "{:x}",
            Sha256::digest(
                bounded_encode(&(
                    "ghtui-explicit-new-conversation-write-v18",
                    observation,
                    &terminal.reference,
                    key,
                ))?
                .as_bytes()
            )
        ),
    );
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConversationActionOriginSeedV18 {
    version: u8,
    current: ConversationOriginSeedV17,
    database: DatabaseV3,
    observation: ActionObservationV18,
    generation: i64,
    terminal: TerminalV18,
    key: String,
    parent_ack: Option<i64>,
    root_ack: Option<TerminalV18>,
    protected_key: String,
    protected_fingerprint: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConversationActionOriginRecordV18 {
    seed: ConversationActionOriginSeedV18,
    reference: PrPublicationReference,
}
impl ConversationActionOriginSeedV18 {
    pub(crate) fn from_revision(
        current: &ConversationOriginSeedV17,
        base: &ConversationActionBaseV18,
        revision: &Arc<ConversationTerminalRevisionV18>,
    ) -> Result<Self> {
        base.matches_seed(current)?;
        if revision.base.observation != base.observation || revision.database != base.database {
            bail!("Actual new-action Base/revision differ");
        }
        let seed = Self {
            version: 18,
            current: current.clone(),
            database: revision.database.clone(),
            observation: base.observation.clone(),
            generation: revision.generation,
            terminal: revision.terminal.clone(),
            key: revision.key.clone(),
            parent_ack: revision.parent_ack,
            root_ack: revision.root_ack.clone(),
            protected_key: revision.protected_key.clone(),
            protected_fingerprint: revision.protected_fingerprint.clone(),
        };
        bounded_encode(&seed)?;
        Ok(seed)
    }
    pub(crate) fn write(&self, values: &BTreeMap<String, String>) -> Result<PrWriteIntent> {
        let mut write = self.current.write(values)?;
        bind_revision(&self.observation, &self.terminal, &self.key, &mut write)?;
        Ok(write)
    }
}

/// Closed historical union. Decoding never issues a Base or a Revision.
/// Untagged SERIALIZATION preserves the old V17 bytes; decoding below checks
/// an explicit version rather than trying alternate historical shapes.
#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub(crate) enum ObservedConversationOriginV18 {
    Legacy17(Box<conversation_history_v17::ConversationOriginRecordV17>),
    Action18(Box<ConversationActionOriginRecordV18>),
}
/// Descriptive nullable SQL columns, never an issued Base/Revision or authority.
pub(super) struct ConversationLineageIndexV18<'a> {
    pub base: Option<&'a str>,
    pub action: Option<&'a str>,
    pub parent_ack: Option<i64>,
}
impl ObservedConversationOriginV18 {
    pub(crate) fn seed(&self) -> &ConversationOriginSeedV17 {
        match self {
            Self::Legacy17(v) => &v.seed,
            Self::Action18(v) => &v.seed.current,
        }
    }
    pub(crate) fn reference(&self) -> &PrPublicationReference {
        match self {
            Self::Legacy17(v) => &v.reference,
            Self::Action18(v) => &v.reference,
        }
    }
    pub(crate) fn key(&self) -> Result<String> {
        match self {
            Self::Legacy17(v) => v.seed.form_key(),
            Self::Action18(v) => Ok(v.seed.key.clone()),
        }
    }
    /// Allocation debit only, not a draft-key/source getter. Native terminal
    /// capture calls this AFTER actual decode/validation and BEFORE key().
    pub(super) fn terminal_key_copy_debit_v88(&self) -> Result<usize> {
        match self {
            Self::Legacy17(_) => Ok(256),
            Self::Action18(record) => {
                let key = &record.seed.key;
                if key.is_empty() || key.len() > 256 || key.capacity() > 16 * 1024 {
                    bail!("Actual validated Conversation action key exceeds borrowed bounds");
                }
                // The already decoded origin key remains live while key()
                // clones it. Charge its ACTUAL capacity plus a fixed256-byte
                // new-key allowance; caller checks actual new capacity too.
                key.capacity()
                    .checked_add(256)
                    .context("Native Conversation key-copy bound overflow")
            }
        }
    }
    pub(crate) fn write(&self, values: &BTreeMap<String, String>) -> Result<PrWriteIntent> {
        match self {
            Self::Legacy17(v) => v.seed.write(values),
            Self::Action18(v) => v.seed.write(values),
        }
    }
    pub(super) fn decode(
        raw: &str,
        database: &Path,
        target: &PrWriteTarget,
        marker: &PrWriteMarker,
        repo: u64,
        pr: u64,
        index: ConversationLineageIndexV18<'_>,
    ) -> Result<Self> {
        let ConversationLineageIndexV18 {
            base,
            action,
            parent_ack,
        } = index;
        if raw.len() > 16384 {
            bail!("Conversation origin exceeds16KiB");
        }
        let value: serde_json::Value = serde_json::from_str(raw)?;
        let record = match value
            .get("seed")
            .and_then(|v| v.get("version"))
            .and_then(|v| v.as_u64())
        {
            Some(17) if base.is_none() && action.is_none() && parent_ack.is_none() => {
                Self::Legacy17(Box::new(serde_json::from_str(raw)?))
            }
            Some(18) => Self::Action18(Box::new(serde_json::from_str(raw)?)),
            _ => bail!("Unknown/malformed original Conversation lineage version/index"),
        };
        let seed = record.seed();
        seed.validate_marker(database, target, marker)?;
        if seed.repository_id != repo
            || seed.pr_id != pr
            || record.reference() != &marker.publication_reference()
        {
            bail!("Exact Conversation origin/marker identities differ");
        }
        if let Self::Action18(record) = &record {
            record.seed.validate_shape(database)?;
            if base != Some(record.seed.observation.base_key.as_str())
                || action != Some(record.seed.current.action.command())
                || parent_ack != record.seed.parent_ack
                || record.seed.terminal.reference.attempt_id >= marker.attempt_id
            {
                bail!("Conversation lineage index/order differs");
            }
        }
        Ok(record)
    }
}
impl ConversationActionOriginSeedV18 {
    fn validate_shape(&self, database: &Path) -> Result<()> {
        self.current.validate()?;
        let o = &self.observation;
        if self.version != 18
            || self.database != DatabaseV3::capture(database)?
            || !self.current.matches_database(database)?
            || self.current.form_key()? != o.base_key
            || self.current.scope != o.scope
            || self.current.repository_id != o.repository_id
            || self.current.repository_node != o.repository_node
            || self.current.repository != o.repository
            || self.current.pr_id != o.pr_id
            || self.current.pr_node != o.pr_node
            || self.current.number != o.number
            || self.current.head != o.head
            || self.current.base != o.base
            || self.current.action != o.action
            || self
                .current
                .selected
                .as_ref()
                .map(|s| (s.id, s.node.as_str()))
                != o.selected.as_ref().map(|(id, n)| (*id, n.as_str()))
            || next_key(o, &self.terminal)? != self.key
            || self.key.len() > 256
            || self.protected_key.len() > 256
            || self.generation < 0
            || self.terminal.reference.attempt_id <= 0
            || self
                .parent_ack
                .is_some_and(|id| id <= 0 || id > self.terminal.reference.attempt_id)
            || self.parent_ack != self.root_ack.as_ref().map(|t| t.reference.attempt_id)
            || self.root_ack.as_ref().is_some_and(|t| {
                t.kind != TerminalKindV18::Acknowledged
                    || t.receipt.is_some()
                    || !digest(&t.raw_attempt_sha256)
                    || !digest(&t.raw_origin_sha256)
            })
            || !digest(&self.terminal.raw_attempt_sha256)
            || !digest(&self.terminal.raw_origin_sha256)
            || self
                .protected_fingerprint
                .as_ref()
                .is_some_and(|s| !digest(s))
        {
            bail!("Closed Conversation lineage shape/database differs");
        }
        match self.terminal.kind {
            TerminalKindV18::Acknowledged if self.terminal.receipt.is_none() => {}
            TerminalKindV18::Confirmed if self.terminal.receipt.is_some() => {}
            _ => bail!("ACK and actual durable Confirmed bases must remain distinct"),
        }
        bounded_encode(self)?;
        Ok(())
    }
}
fn digest(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn action_tag(action: ConversationActionV17) -> &'static str {
    action.command()
}
pub(super) fn migrate(tx: &Transaction<'_>) -> Result<()> {
    let columns = {
        let mut q = tx.prepare("PRAGMA table_info(pr_write_conversation_origins_v17)")?;
        let rows = q
            .query_map([], |r| r.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    for (name, kind) in [
        ("base_key_v18", "TEXT"),
        ("action_tag_v18", "TEXT"),
        ("parent_ack_attempt_v18", "INTEGER"),
    ] {
        if !columns.iter().any(|c| c == name) {
            tx.execute_batch(&format!(
                "ALTER TABLE pr_write_conversation_origins_v17 ADD COLUMN {name} {kind}"
            ))?;
        }
    }
    tx.execute_batch("CREATE INDEX IF NOT EXISTS pr_write_conversation_lineage_v18 ON pr_write_conversation_origins_v17(base_key_v18,action_tag_v18,attempt_id DESC)")?;
    Ok(())
}
pub(super) fn insert(
    tx: &Transaction<'_>,
    seed: &ConversationActionOriginSeedV18,
    marker: &PrWriteMarker,
) -> Result<()> {
    let record = ConversationActionOriginRecordV18 {
        seed: seed.clone(),
        reference: marker.publication_reference(),
    };
    let json = bounded_encode(&record)?;
    tx.execute("INSERT INTO pr_write_conversation_origins_v17(attempt_id,repository_id,pr_id,origin_json,base_key_v18,action_tag_v18,parent_ack_attempt_v18) VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![marker.attempt_id,seed.current.repository_id as i64,seed.current.pr_id as i64,json,seed.observation.base_key,action_tag(seed.current.action),seed.parent_ack])?;
    Ok(())
}
struct StoredV18 {
    marker: PrWriteMarker,
    origin: ObservedConversationOriginV18,
    attempt_hash: String,
    origin_hash: String,
}
const ATTEMPT_BYTES:&str="length(CAST(host AS BLOB))+length(CAST(principal AS BLOB))+length(CAST(pr_node AS BLOB))+length(CAST(repository AS BLOB))+length(CAST(command AS BLOB))+length(CAST(selected_resource AS BLOB))+length(CAST(observed_head AS BLOB))+length(CAST(payload_sha256 AS BLOB))+length(CAST(state AS BLOB))+COALESCE(length(CAST(confirmed_json AS BLOB)),0)";
fn stored(
    connection: &Connection,
    path: &Path,
    target: &PrWriteTarget,
    id: i64,
) -> Result<StoredV18> {
    let lengths:(usize,usize,usize)=connection.query_row("SELECT length(CAST(origin_json AS BLOB)),COALESCE(length(CAST(base_key_v18 AS BLOB)),0),COALESCE(length(CAST(action_tag_v18 AS BLOB)),0) FROM pr_write_conversation_origins_v17 WHERE attempt_id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
    if lengths.0 > 16384 || lengths.1 > 256 || lengths.2 > 128 {
        bail!("Bounded original terminal metadata refused before materialization");
    }
    let (marker, attempt_hash) = kernel_v52::stored_attempt_v52(connection, target, id)?;
    let (repo,pr,json,base,action,ack):(u64,u64,String,Option<String>,Option<String>,Option<i64>)=connection.query_row("SELECT repository_id,pr_id,origin_json,base_key_v18,action_tag_v18,parent_ack_attempt_v18 FROM pr_write_conversation_origins_v17 WHERE attempt_id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?)))?;
    let origin_hash = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&(repo, pr, &json, &base, &action, ack))?)
    );
    let origin = ObservedConversationOriginV18::decode(
        &json,
        path,
        target,
        &marker,
        repo,
        pr,
        ConversationLineageIndexV18 {
            base: base.as_deref(),
            action: action.as_deref(),
            parent_ack: ack,
        },
    )?;
    Ok(StoredV18 {
        marker,
        origin,
        attempt_hash,
        origin_hash,
    })
}
fn terminal(value: &StoredV18) -> Result<TerminalV18> {
    let terminal =
        kernel_v52::terminal_marker_v52(&value.marker, &value.attempt_hash, &value.origin_hash)?;
    // The common closed effect validator checks exact numeric/node/state shape;
    // selected-resource effects additionally bind the original selected IDs.
    let seed = value.origin.seed();
    if let Some(receipt) = &value.marker.confirmed {
        let selected = match seed.action {
            ConversationActionV17::Edit | ConversationActionV17::Delete => {
                seed.selected.as_ref().map(|s| (s.id, s.node.as_str()))
            }
            ConversationActionV17::RemoveReaction => {
                seed.reaction.as_ref().map(|(id, n)| (*id, n.as_str()))
            }
            _ => None,
        };
        if selected.is_some_and(|(id, node)| {
            receipt.resource_id != id.to_string() || receipt.node_id.as_deref() != Some(node)
        }) {
            bail!("Terminal receipt selected original ID/node differs");
        }
    }
    Ok(terminal)
}
fn protected(
    connection: &Connection,
    scope: &Scope,
    key: &str,
    stored: &StoredV18,
) -> Result<Option<String>> {
    kernel_v52::protected_row_v52(
        connection,
        scope,
        key,
        kernel_v52::OriginRefV52::Conversation(stored),
    )
}

fn latest(connection: &Connection, observed: &ActionObservationV18) -> Result<Option<i64>> {
    // The index is descriptive, not target authority. Select the actual newest
    // indexed row FIRST, including a foreign/dangling row, and let stored()
    // validate its complete target and origin. Filtering the target (or an
    // inner join) here could silently revive an older valid terminal instead.
    Ok(connection.query_row("SELECT attempt_id FROM pr_write_conversation_origins_v17 WHERE base_key_v18=?1 AND action_tag_v18=?2 ORDER BY attempt_id DESC LIMIT 1",params![observed.base_key,action_tag(observed.action)],|r|r.get(0)).optional()?)
}
fn validate_terminal(
    connection: &Connection,
    path: &Path,
    observed: &ActionObservationV18,
    target: &PrWriteTarget,
    expected: &TerminalV18,
    root_ack: Option<&TerminalV18>,
) -> Result<StoredV18> {
    kernel_v52::validate_conversation_terminal_v52(
        connection, path, observed, target, expected, root_ack,
    )
}

impl PrWriteJournal {
    /// At most latest + its immediate terminal + root ACK (three witnesses).
    /// Only a genuine open-read Base enters; history never calls this factory.
    pub(crate) fn conversation_action_revision_v18(
        &self,
        base: &ConversationActionBaseV18,
    ) -> Result<Option<Arc<ConversationTerminalRevisionV18>>> {
        kernel_v52::capture_revision_v52(self, kernel_v52::FamilyV52::Conversation(base))?
            .map(|captured| {
                Ok(Arc::new(ConversationTerminalRevisionV18 {
                    database: captured.database,
                    base: base.clone(),
                    generation: captured.generation,
                    terminal: captured.terminal,
                    key: captured.key,
                    parent_ack: captured.parent_ack,
                    root_ack: captured.root_ack,
                    protected_key: captured.protected_key,
                    protected_fingerprint: captured.protected_fingerprint,
                }))
            })
            .transpose()
    }
    pub(crate) fn validate_conversation_revision_v18(
        &self,
        base: &ConversationActionBaseV18,
        revision: &ConversationTerminalRevisionV18,
    ) -> Result<()> {
        let actual = self
            .conversation_action_revision_v18(base)?
            .context("Original terminal disappeared")?;
        if actual.database != revision.database
            || actual.base.observation != revision.base.observation
            || actual.generation != revision.generation
            || actual.terminal != revision.terminal
            || actual.key != revision.key
            || actual.parent_ack != revision.parent_ack
            || actual.root_ack != revision.root_ack
            || actual.protected_key != revision.protected_key
            || actual.protected_fingerprint != revision.protected_fingerprint
        {
            bail!("Exact explicit New-action basis changed");
        }
        Ok(())
    }
    pub(crate) fn begin_conversation_action_v18(
        &mut self,
        target: &PrWriteTarget,
        intent: &PrWriteIntent,
        seed: &ConversationActionOriginSeedV18,
    ) -> Result<PrWritePermit> {
        seed.validate_shape(&self.path)?;
        let values = intent
            .values
            .iter()
            .filter(|(k, _)| !matches!(k.as_str(), "origin-v17" | "explicit-new-conversation-v18"))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        if seed.current.scope != target.scope
            || seed.current.pr_node != target.pr_node_id
            || seed.current.repository != target.repository
            || seed.current.number != target.number
            || seed.write(&values)?.digest()? != intent.digest()?
        {
            bail!("Exact V18 durable current payload differs");
        }
        self.begin_with_observed_origin_v2(
            target,
            intent,
            Some(ObservedOriginV2::ConversationActionV18(seed)),
        )
    }
    pub(crate) fn validate_owned_conversation_action_v18(
        &self,
        seed: &ConversationActionOriginSeedV18,
        reference: &PrPublicationReference,
    ) -> Result<()> {
        seed.validate_shape(&self.path)?;
        // A comparison-only journal target is NOT an issued Base/Revision.
        let target = PrWriteTarget::for_source(
            &seed.current.scope,
            &seed.current.repository,
            &seed.current.pr_node,
            seed.current.number,
        )?;
        let own = stored(&self.connection, &self.path, &target, reference.attempt_id)?;
        let ObservedConversationOriginV18::Action18(record) = &own.origin else {
            bail!("Owned current origin is not a V18 lineage");
        };
        if own.marker.publication_reference() != *reference
            || bounded_encode(&record.seed)? != bounded_encode(seed)?
            || latest(&self.connection, &seed.observation)? != Some(reference.attempt_id)
        {
            bail!("Exact current owned lineage differs");
        }
        let parent = validate_terminal(
            &self.connection,
            &self.path,
            &seed.observation,
            &target,
            &seed.terminal,
            seed.root_ack.as_ref(),
        )?;
        if parent.origin.key()? != seed.protected_key
            || protected(
                &self.connection,
                &seed.current.scope,
                &seed.protected_key,
                &parent,
            )? != seed.protected_fingerprint
        {
            bail!("Protected ancestor body moved");
        }
        Ok(())
    }
}
fn generation_in(connection: &Connection, target: &PrWriteTarget) -> Result<i64> {
    generation(connection, target)
}
pub(super) fn validate_begin(
    tx: &Transaction<'_>,
    path: &Path,
    target: &PrWriteTarget,
    seed: &ConversationActionOriginSeedV18,
) -> Result<()> {
    seed.validate_shape(path)?;
    let expected = kernel_v52::CapturedV52 {
        database: seed.database.clone(),
        generation: seed.generation,
        terminal: seed.terminal.clone(),
        key: seed.key.clone(),
        parent_ack: seed.parent_ack,
        root_ack: seed.root_ack.clone(),
        protected_key: seed.protected_key.clone(),
        protected_fingerprint: seed.protected_fingerprint.clone(),
    };
    kernel_v52::validate_begin_v52(
        tx,
        path,
        kernel_v52::CompareV52::Conversation(&seed.observation),
        target,
        &expected,
    )
}
