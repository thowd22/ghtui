//! ONE moved V18 <=3-unique-row lineage algorithm. Closed actual producer
//! families only; this module has no callback/query/trait/history-to-Base API.
use super::*;
use crate::pr_write_journal::issue_link_origin_v87::{
    lineage_v87::{IssueLinkActionBaseV87, StoredIssueLinkV87},
    RecordedIssueLinkV87,
};
use crate::pr_write_journal::metadata_members_v48::lineage_v52::{
    MembersActionBaseV48, StoredMembersV52,
};
use crate::pr_write_journal::state_origin_v68::lineage_v68::{StateActionBaseV68, StoredStateV68};

pub(in crate::pr_write_journal) enum FamilyV52<'a> {
    Conversation(&'a ConversationActionBaseV18),
    Members(&'a MembersActionBaseV48),
    State(&'a StateActionBaseV68),
    IssueLinksV87(&'a IssueLinkActionBaseV87),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::pr_write_journal) struct CapturedV52 {
    pub database: DatabaseV3,
    pub generation: i64,
    pub terminal: TerminalV18,
    pub key: String,
    pub parent_ack: Option<i64>,
    pub root_ack: Option<TerminalV18>,
    pub protected_key: String,
    pub protected_fingerprint: Option<String>,
}
// Comparison descriptions are NOT issued Bases. They can validate an already
// typed seed during atomic begin/owned checks, never enter capture_revision.
#[derive(Clone, Copy)]
pub(in crate::pr_write_journal) enum CompareV52<'a> {
    Conversation(&'a ActionObservationV18),
    Members(&'a crate::pr_write_journal::metadata_members_v48::RecordedMetadataMembersV48),
    State(&'a crate::pr_write_journal::state_origin_v68::RecordedStateOriginV68),
    IssueLinksV87(&'a RecordedIssueLinkV87),
}
enum StoredV52 {
    Conversation(Box<StoredV18>),
    Members(Box<StoredMembersV52>),
    State(Box<StoredStateV68>),
    IssueLinksV87(Box<StoredIssueLinkV87>),
}
/// Fixed-size relocation scratch only. Existing bounded strings/Vecs MOVE into
/// the boxes; their allocations are not copied or given a renewed allowance.
/// At most three unique stored terminals, two phase seeds and one disposition
/// coexist. For each we charge BOTH the old stack payload and requested padded
/// heap layout plus its Box pointer, before allocating. This fits the existing
/// 64KiB producer/SQL fixed-overlap term; raw origin16KiB/marker8KiB and the
/// three-row lineage limits remain independent and unchanged. This is a
/// requested-layout proof, not an allocator-header or process-RSS claim.
pub(in crate::pr_write_journal) fn reserve_origin_box_moves_v79() -> Result<usize> {
    fn relocation<T>() -> Option<usize> {
        std::alloc::Layout::new::<T>()
            .pad_to_align()
            .size()
            .checked_mul(2)?
            .checked_add(std::mem::size_of::<Box<T>>())
    }
    let stored = relocation::<StoredV18>()
        .zip(relocation::<StoredMembersV52>())
        .map(|(conversation, members)| conversation.max(members))
        .zip(relocation::<StoredStateV68>())
        .map(|(old, state)| old.max(state))
        .zip(relocation::<StoredIssueLinkV87>())
        .map(|(old, issue)| old.max(issue));
    let seeds = relocation::<metadata_origin_v25::MetadataOriginSeedV25>()
        .zip(relocation::<
            metadata_members_v48::MetadataMembersOriginSeedV48,
        >())
        .map(|(text, members)| text.max(members))
        .zip(relocation::<state_origin_v68::StateOriginSeedV68>())
        .map(|(old, state)| old.max(state))
        .zip(relocation::<issue_link_origin_v87::IssueLinkOriginSeedV87>())
        .map(|(old, issue)| old.max(issue));
    stored
        .and_then(|bytes| bytes.checked_mul(3))
        .zip(seeds.and_then(|bytes| bytes.checked_mul(2)))
        .and_then(|(stored, seeds)| stored.checked_add(seeds))
        .zip(relocation::<
            metadata_members_v48::MetadataMembersOriginRecordV48,
        >())
        .and_then(|(other, members)| {
            relocation::<state_origin_v68::StateOriginRecordV68>()
                .map(|state| (other, members.max(state)))
        })
        .and_then(|(other, disposition)| other.checked_add(disposition))
        .filter(|bytes| *bytes <= 64 * 1024)
        .context("Closed original box relocation exceeds fixed64KiB overlap")
}
pub(super) enum OriginRefV52<'a> {
    Conversation(&'a StoredV18),
    Members(&'a StoredMembersV52),
    State(&'a StoredStateV68),
    IssueLinksV87(&'a StoredIssueLinkV87),
}
impl OriginRefV52<'_> {
    fn marker(&self) -> &PrWriteMarker {
        match self {
            Self::Conversation(v) => &v.marker,
            Self::Members(v) => &v.marker,
            Self::State(v) => &v.marker,
            Self::IssueLinksV87(v) => &v.marker,
        }
    }
    fn write(&self, values: &BTreeMap<String, String>) -> Result<PrWriteIntent> {
        match self {
            Self::Conversation(v) => v.origin.write(values),
            Self::Members(v) => v.record.seed.write(values),
            Self::State(v) => v.record.seed.write(values),
            Self::IssueLinksV87(v) => v.record.seed.write(values),
        }
    }
}
impl FamilyV52<'_> {
    fn database(&self) -> &DatabaseV3 {
        match self {
            Self::Conversation(v) => &v.database,
            Self::Members(v) => v.database_v52(),
            Self::State(v) => v.database_v68(),
            Self::IssueLinksV87(v) => v.database_v87(),
        }
    }
    fn target(&self) -> &PrWriteTarget {
        match self {
            Self::Conversation(v) => v.target(),
            Self::Members(v) => v.target_v52(),
            Self::State(v) => v.target(),
            Self::IssueLinksV87(v) => v.target_v87(),
        }
    }
    fn scope(&self) -> &Scope {
        self.target().scope()
    }
    fn compare(&self) -> CompareV52<'_> {
        match self {
            Self::Conversation(v) => CompareV52::Conversation(&v.observation),
            Self::Members(v) => CompareV52::Members(v.observed_v52()),
            Self::State(v) => CompareV52::State(v.observed_v68()),
            Self::IssueLinksV87(v) => CompareV52::IssueLinksV87(v.observed_v87()),
        }
    }
    fn root_key(&self) -> &str {
        match self {
            Self::Conversation(v) => v.key(),
            Self::Members(v) => v.current_draft_v52(),
            Self::State(v) => v.key(),
            Self::IssueLinksV87(v) => v.current_draft_v87(),
        }
    }
    fn candidate(&self, connection: &Connection) -> Result<Option<i64>> {
        match self {
            Self::Conversation(base) => {
                let selector = format!(
                    "conversation-v17/{}",
                    base.key()
                        .strip_prefix("conversation-draft-v17/")
                        .context("Actual original draft namespace differs")?
                );
                Ok(connection.query_row("SELECT attempt_id FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND pr_node=?3 AND command=?4 AND selected_resource=?5 ORDER BY attempt_id DESC LIMIT 1",params![base.scope().host,base.scope().login,base.target.pr_node_id,base.observation.action.command(),selector],|r|r.get(0)).optional()?)
            }
            Self::Members(_) | Self::State(_) | Self::IssueLinksV87(_) => {
                self.compare().latest(connection)
            }
        }
    }
}
impl CompareV52<'_> {
    pub(in crate::pr_write_journal) fn latest(
        &self,
        connection: &Connection,
    ) -> Result<Option<i64>> {
        match self {
            Self::Conversation(observed)=>super::latest(connection,observed),
            Self::Members(observed)=>Ok(connection.query_row("SELECT attempt_id FROM pr_write_metadata_origins_v25 WHERE base_key_v48=?1 AND action_tag_v48=?2 ORDER BY attempt_id DESC LIMIT 1",params![observed.base_key(),observed.command()?.key()],|r|r.get(0)).optional()?),
            Self::State(observed)=>crate::pr_write_journal::state_origin_v68::lineage_v68::latest_state_v68(connection,observed),
            Self::IssueLinksV87(observed)=>Ok(connection.query_row("SELECT attempt_id FROM pr_write_issue_link_origins_v87 WHERE base_key=?1 AND action_tag=?2 ORDER BY attempt_id DESC LIMIT 1",params![observed.base_key_v87(),observed.command_v87()?.command()],|r|r.get(0)).optional()?),
        }
    }
    fn stored(
        &self,
        connection: &Connection,
        path: &Path,
        target: &PrWriteTarget,
        id: i64,
    ) -> Result<StoredV52> {
        reserve_origin_box_moves_v79()?;
        let value = match self {
            Self::Conversation(_) => {
                StoredV52::Conversation(Box::new(super::stored(connection, path, target, id)?))
            }
            Self::Members(_) => StoredV52::Members(Box::new(StoredMembersV52::load(
                connection, path, target, id,
            )?)),
            Self::State(_) => StoredV52::State(Box::new(StoredStateV68::load(
                connection, path, target, id,
            )?)),
            Self::IssueLinksV87(_) => StoredV52::IssueLinksV87(Box::new(StoredIssueLinkV87::load(
                connection, path, target, id,
            )?)),
        };
        match (self, &value) {
            (Self::Conversation(observed), StoredV52::Conversation(value)) => {
                matches_observation(observed, value.origin.seed())?
            }
            (Self::Members(observed), StoredV52::Members(value)) => {
                observed.matches_stable_v52(&value.record.seed)?
            }
            (Self::State(observed), StoredV52::State(value)) => {
                observed.matches_stable_v68(&value.record.seed)?
            }
            (Self::IssueLinksV87(observed), StoredV52::IssueLinksV87(value)) => {
                observed.matches_stable_v87(&value.record.seed)?
            }
            _ => bail!("Mixed closed lineage family"),
        }
        Ok(value)
    }
    fn next_key(&self, terminal: &TerminalV18) -> Result<String> {
        match self {
            Self::Conversation(observed) => super::next_key(observed, terminal),
            Self::State(observed) => {
                crate::pr_write_journal::state_origin_v68::lineage_v68::next_key_v68(
                    observed, terminal,
                )
            }
            Self::Members(observed) => {
                let encoded = bounded_encode(&(
                    "ghtui-explicit-new-metadata-members-v48",
                    observed.base_key(),
                    observed.command()?.key(),
                    &terminal.reference,
                ))?;
                Ok(format!(
                    "metadata-members-v48/{:x}",
                    Sha256::digest(encoded.as_bytes())
                ))
            }
            Self::IssueLinksV87(observed) => observed.next_key_v87(terminal),
        }
    }
}
impl StoredV52 {
    fn origin_ref(&self) -> OriginRefV52<'_> {
        match self {
            Self::Conversation(v) => OriginRefV52::Conversation(v),
            Self::Members(v) => OriginRefV52::Members(v),
            Self::State(v) => OriginRefV52::State(v),
            Self::IssueLinksV87(v) => OriginRefV52::IssueLinksV87(v),
        }
    }
    fn marker(&self) -> &PrWriteMarker {
        match self {
            Self::Conversation(v) => &v.marker,
            Self::Members(v) => &v.marker,
            Self::State(v) => &v.marker,
            Self::IssueLinksV87(v) => &v.marker,
        }
    }
    fn terminal(&self) -> Result<TerminalV18> {
        match self {
            Self::Conversation(v) => super::terminal(v),
            Self::Members(v) => v.terminal(),
            Self::State(v) => v.terminal(),
            Self::IssueLinksV87(v) => v.terminal(),
        }
    }
    fn key(&self) -> Result<String> {
        match self {
            Self::Conversation(v) => v.origin.key(),
            Self::Members(v) => Ok(v.record.seed.draft_key_v52().into()),
            Self::State(v) => v.record.seed.draft_key(),
            Self::IssueLinksV87(v) => Ok(v.record.seed.draft_key_v87().into()),
        }
    }
    fn ancestor(&self) -> Option<(&TerminalV18, Option<&TerminalV18>)> {
        match self {
            Self::Conversation(v) => match &v.origin {
                ObservedConversationOriginV18::Legacy17(_) => None,
                ObservedConversationOriginV18::Action18(v) => {
                    Some((&v.seed.terminal, v.seed.root_ack.as_ref()))
                }
            },
            Self::Members(v) => v
                .record
                .seed
                .lineage_v52()
                .map(|v| (&v.terminal, v.root_ack.as_ref())),
            Self::State(v) => v
                .record
                .seed
                .lineage_v68()
                .map(|v| (&v.terminal, v.root_ack.as_ref())),
            Self::IssueLinksV87(v) => v
                .record
                .seed
                .lineage_v87()
                .map(|v| (&v.terminal, v.root_ack.as_ref())),
        }
    }
    fn legacy(&self) -> bool {
        matches!(self,Self::Conversation(v) if matches!(v.origin,ObservedConversationOriginV18::Legacy17(_)))
    }
}
pub(super) fn protected_row_v52(
    connection: &Connection,
    scope: &Scope,
    key: &str,
    stored: OriginRefV52<'_>,
) -> Result<Option<String>> {
    let bytes:Option<usize>=connection.query_row("SELECT length(CAST(body AS BLOB)) FROM drafts WHERE host=?1 AND login=?2 AND resource=?3",params![scope.host,scope.login,key],|r|r.get(0)).optional()?;
    if bytes.is_some_and(|n| n > 128 * 1024) {
        bail!("Protected Conversation envelope exceeds bound");
    }
    let raw: Option<String> = connection
        .query_row(
            "SELECT body FROM drafts WHERE host=?1 AND login=?2 AND resource=?3",
            params![scope.host, scope.login, key],
            |r| r.get(0),
        )
        .optional()?;
    raw.map(|raw| {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Envelope {
            version: u8,
            values: BTreeMap<String, String>,
            publication: Option<PrPublicationReference>,
        }
        let value: Envelope = serde_json::from_str(&raw)?;
        if value.publication.as_ref() != Some(&stored.marker().publication_reference())
            || value.version != 17
            || stored.write(&value.values)?.digest()? != stored.marker().payload_sha256
        {
            bail!("Protected old body/ref/fingerprint differs; never overwrite");
        }
        Ok(format!("{:x}", Sha256::digest(raw.as_bytes())))
    })
    .transpose()
}
fn protected_v52(
    connection: &Connection,
    scope: &Scope,
    key: &str,
    stored: &StoredV52,
) -> Result<Option<String>> {
    protected_row_v52(connection, scope, key, stored.origin_ref())
}
fn validate_terminal_v52(
    connection: &Connection,
    path: &Path,
    compare: CompareV52<'_>,
    target: &PrWriteTarget,
    expected: &TerminalV18,
    root_ack: Option<&TerminalV18>,
) -> Result<StoredV52> {
    let current = compare.stored(connection, path, target, expected.reference.attempt_id)?;
    if current.terminal()? != *expected {
        bail!("Exact terminal raw row/ref/receipt/created/origin changed");
    }
    if let Some(anchor) = root_ack {
        let id = anchor.reference.attempt_id;
        if id > expected.reference.attempt_id {
            bail!("Non-monotonic ACK lineage");
        }
        if id != expected.reference.attempt_id {
            let root = compare.stored(connection, path, target, id)?;
            if root.terminal()? != *anchor || anchor.kind != TerminalKindV18::Acknowledged {
                bail!("Exact original ACK anchor unavailable");
            }
        } else if expected != anchor || expected.kind != TerminalKindV18::Acknowledged {
            bail!("Confirmed is not ACK");
        }
    }
    Ok(current)
}
pub(in crate::pr_write_journal) fn capture_revision_v52(
    journal: &PrWriteJournal,
    family: FamilyV52<'_>,
) -> Result<Option<CapturedV52>> {
    if DatabaseV3::capture(&journal.path)? != *family.database()
        || journal.blocking(family.target())?.is_some()
    {
        bail!("Current original DB or global blocker differs");
    }
    let generation = journal.source_generation(family.target())?;
    let tx = journal.connection.unchecked_transaction()?;
    let compare = family.compare();
    let indexed = compare.latest(&tx)?;
    let candidate = family.candidate(&tx)?;
    let id = if let Some(id) = indexed {
        Some(id)
    } else {
        let bytes:Option<usize>=tx.query_row("SELECT length(CAST(body AS BLOB)) FROM drafts WHERE host=?1 AND login=?2 AND resource=?3",params![family.scope().host,family.scope().login,family.root_key()],|r|r.get(0)).optional()?;
        if bytes.is_some_and(|n| n > 128 * 1024) {
            bail!("Original root draft exceeds bound");
        }
        let raw: Option<String> = tx
            .query_row(
                "SELECT body FROM drafts WHERE host=?1 AND login=?2 AND resource=?3",
                params![family.scope().host, family.scope().login, family.root_key()],
                |r| r.get(0),
            )
            .optional()?;
        let from_draft = raw
            .map(|raw| -> Result<Option<i64>> {
                let value: serde_json::Value = serde_json::from_str(&raw)?;
                let reference: Option<PrPublicationReference> = serde_json::from_value(
                    value
                        .get("publication")
                        .cloned()
                        .context("Original publication field absent")?,
                )?;
                Ok(reference.map(|r| r.attempt_id))
            })
            .transpose()?
            .flatten();
        if matches!(family, FamilyV52::Members(_) | FamilyV52::IssueLinksV87(_))
            && from_draft.is_some()
        {
            bail!("Known member protected root has no exact index; no reconstruction/fallback");
        }
        if matches!(family, FamilyV52::State(_)) && from_draft.is_some() {
            bail!("Known State protected root has no exact index; no reconstruction/fallback");
        }
        from_draft
    };
    let Some(id) = id else {
        if let Some(id) = candidate {
            let observed = compare.stored(&tx, &journal.path, family.target(), id)?;
            observed.terminal()?;
            if !observed.legacy() {
                bail!("Unindexed new-action origin cannot become a fresh root");
            }
        }
        if generation != generation_in(&tx, family.target())? {
            bail!("Original generation moved");
        }
        tx.commit()?;
        if DatabaseV3::capture(&journal.path)? != *family.database() {
            bail!("Original DB replaced");
        }
        return Ok(None);
    };
    if candidate != Some(id) {
        bail!("The original terminal is no longer the latest exact action");
    }
    let current = compare.stored(&tx, &journal.path, family.target(), id)?;
    let terminal = current.terminal()?;
    let protected_key = current.key()?;
    let root_ack = if let Some((ancestor, root)) = current.ancestor() {
        validate_terminal_v52(&tx, &journal.path, compare, family.target(), ancestor, root)?;
        root.cloned()
            .or_else(|| (terminal.kind == TerminalKindV18::Acknowledged).then(|| terminal.clone()))
    } else {
        (terminal.kind == TerminalKindV18::Acknowledged).then(|| terminal.clone())
    };
    let parent_ack = root_ack.as_ref().map(|t| t.reference.attempt_id);
    let protected_fingerprint = protected_v52(&tx, family.scope(), &protected_key, &current)?;
    let key = compare.next_key(&terminal)?;
    let next_bytes:Option<usize>=tx.query_row("SELECT length(CAST(body AS BLOB)) FROM drafts WHERE host=?1 AND login=?2 AND resource=?3",params![family.scope().host,family.scope().login,&key],|r|r.get(0)).optional()?;
    if next_bytes.is_some_and(|n| n > 131072) {
        bail!("Successor draft exceeds bounded envelope");
    }
    let next: Option<String> = tx
        .query_row(
            "SELECT body FROM drafts WHERE host=?1 AND login=?2 AND resource=?3",
            params![family.scope().host, family.scope().login, &key],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(next) = next {
        let value: serde_json::Value = serde_json::from_str(&next)?;
        if value.get("publication") != Some(&serde_json::Value::Null) {
            bail!("Unindexed protected successor is not an explicit New action");
        }
    }
    if generation != generation_in(&tx, family.target())? {
        bail!("Generation moved during lineage capture");
    }
    tx.commit()?;
    if DatabaseV3::capture(&journal.path)? != *family.database() {
        bail!("Original database replaced");
    }
    Ok(Some(CapturedV52 {
        database: family.database().clone(),
        generation,
        terminal,
        key,
        parent_ack,
        root_ack,
        protected_key,
        protected_fingerprint,
    }))
}
pub(in crate::pr_write_journal) fn validate_begin_v52(
    tx: &Transaction<'_>,
    path: &Path,
    compare: CompareV52<'_>,
    target: &PrWriteTarget,
    expected: &CapturedV52,
) -> Result<()> {
    if DatabaseV3::capture(path)? != expected.database
        || generation_in(tx, target)? != expected.generation
        || blocking_row(tx, target)?.is_some()
    {
        bail!("New-action current generation/global blocker differs");
    }
    let parent = validate_terminal_v52(
        tx,
        path,
        compare,
        target,
        &expected.terminal,
        expected.root_ack.as_ref(),
    )?;
    let latest = compare.latest(tx)?;
    if !((parent.legacy() && latest.is_none()) || latest == Some(parent.marker().attempt_id)) {
        bail!("Superseded/foreign terminal cannot initialize a new action");
    }
    if parent.key()? != expected.protected_key
        || protected_v52(tx, target.scope(), &expected.protected_key, &parent)?
            != expected.protected_fingerprint
    {
        bail!("Original protected ancestor moved; never clear/replay");
    }
    Ok(())
}
pub(in crate::pr_write_journal) fn validate_owned_basis_v52(
    connection: &Connection,
    path: &Path,
    compare: CompareV52<'_>,
    target: &PrWriteTarget,
    expected: &CapturedV52,
    own_attempt: i64,
) -> Result<()> {
    if DatabaseV3::capture(path)? != expected.database
        || compare.latest(connection)? != Some(own_attempt)
    {
        bail!("Owned exact lineage/database superseded");
    }
    let parent = validate_terminal_v52(
        connection,
        path,
        compare,
        target,
        &expected.terminal,
        expected.root_ack.as_ref(),
    )?;
    if parent.key()? != expected.protected_key
        || protected_v52(connection, target.scope(), &expected.protected_key, &parent)?
            != expected.protected_fingerprint
    {
        bail!("Protected exact ancestor changed before owned invocation");
    }
    Ok(())
}
pub(super) fn validate_conversation_terminal_v52(
    connection: &Connection,
    path: &Path,
    observed: &ActionObservationV18,
    target: &PrWriteTarget,
    expected: &TerminalV18,
    root: Option<&TerminalV18>,
) -> Result<StoredV18> {
    match validate_terminal_v52(
        connection,
        path,
        CompareV52::Conversation(observed),
        target,
        expected,
        root,
    )? {
        StoredV52::Conversation(v) => Ok(*v),
        StoredV52::Members(_) | StoredV52::State(_) | StoredV52::IssueLinksV87(_) => {
            bail!("Mixed terminal family")
        }
    }
}

pub(in crate::pr_write_journal) fn stored_attempt_v52(
    connection: &Connection,
    target: &PrWriteTarget,
    id: i64,
) -> Result<(PrWriteMarker, String)> {
    let bytes: usize = connection.query_row(
        &format!("SELECT {ATTEMPT_BYTES} FROM pr_write_attempts WHERE attempt_id=?1"),
        [id],
        |r| r.get(0),
    )?;
    if bytes > 8192 {
        bail!("Bounded original terminal metadata refused before materialization");
    }
    let (host,principal,node,raw):(String,String,String,MarkerRow)=connection.query_row("SELECT host,principal,pr_node,attempt_id,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at FROM pr_write_attempts WHERE attempt_id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,(r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?,r.get(8)?,r.get(9)?,r.get(10)?,r.get(11)?,r.get(12)?))))?;
    if host != target.scope.host
        || principal != target.scope.login
        || node != target.pr_node_id
        || raw.1 != target.repository
        || raw.2 != target.number
    {
        bail!("Foreign terminal action target");
    }
    let attempt_hash = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&(&host, &principal, &node, &raw))?)
    );
    if let Some(receipt) = &raw.8 {
        #[derive(Deserialize, Serialize)]
        #[serde(deny_unknown_fields)]
        struct ExactReceipt {
            kind: PrEffectKind,
            resource_id: String,
            node_id: Option<String>,
        }
        let parsed: ExactReceipt = serde_json::from_str(receipt)?;
        if serde_json::to_value(parsed)? != serde_json::from_str::<serde_json::Value>(receipt)? {
            bail!("Exact terminal receipt shape differs");
        }
    }
    let marker = decode_marker(target, raw)?;
    Ok((marker, attempt_hash))
}
pub(in crate::pr_write_journal) fn terminal_marker_v52(
    marker: &PrWriteMarker,
    attempt_hash: &str,
    origin_hash: &str,
) -> Result<TerminalV18> {
    let kind = match marker.state {
        PrWriteState::Acknowledged if marker.confirmed.is_none() => TerminalKindV18::Acknowledged,
        PrWriteState::Confirmed if marker.confirmed.is_some() => TerminalKindV18::Confirmed,
        _ => bail!("Pending/Uncertain/known effect IDs are NOT terminal New-action bases"),
    };
    Ok(TerminalV18 {
        reference: marker.publication_reference(),
        kind,
        receipt: marker.confirmed.clone(),
        created_at: marker.created_at,
        raw_attempt_sha256: attempt_hash.into(),
        raw_origin_sha256: origin_hash.into(),
    })
}
