//! Immutable manual-Issue originals and the SECONDARY Issue barrier. Saved
//! rows describe an attempt; only a live original producer creates a seed.
//! The PR attempt table, its thirteen columns and all old serializers stay
//! unchanged. Both begin and claim validate this barrier in THEIR Immediate.
use super::*;
use crate::{
    pr_issue_links_v87::IssueLinkCommandV87,
    providers::gh::{IssueLinkPlanV87, OriginalIssueLinksV87},
    scoped_pr_collaboration_source_v17::PrCollaborationSourceV17,
};
use review_operation_history_v3::{bounded_encode, DatabaseV3};
use std::sync::Arc;

const ORIGIN_CAP_V87: usize = 16 * 1024;
const FIXED_OVERLAP_V87: usize = 64 * 1024;
#[path = "pr_write_issue_link_history_v87.rs"]
pub(crate) mod history_v87;
#[path = "pr_write_issue_link_lineage_v87.rs"]
pub(crate) mod lineage_v87;

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn canonical_digest(raw: &str) -> bool {
    raw.len() == 64
        && raw
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn positive(value: u64) -> bool {
    (1..=i64::MAX as u64).contains(&value)
}
pub(super) fn recognized(command: &str) -> bool {
    matches!(command, "issue-link-v87" | "issue-unlink-manual-v87")
}
fn command(raw: &str) -> Result<IssueLinkCommandV87> {
    match raw {
        "issue-link-v87" => Ok(IssueLinkCommandV87::LinkIssue),
        "issue-unlink-manual-v87" => Ok(IssueLinkCommandV87::UnlinkManualIssue),
        _ => bail!("Not an exact manual Issue-link purpose"),
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RecordedIssueLinkV87 {
    version: u8,
    database: DatabaseV3,
    scope: Scope,
    repository_id: u64,
    repository_node: String,
    repository: String,
    pr_id: u64,
    pr_node: String,
    number: u64,
    issue_repository_id: u64,
    issue_repository_node: String,
    issue_repository: String,
    issue_id: u64,
    issue_node: String,
    issue_number: u64,
    command: String,
    head: String,
    base: String,
    body_null: bool,
    body_sha256: String,
    plan_sha256: String,
    base_key: String,
    draft_key: String,
    parent_ack_attempt: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    lineage: Option<lineage_v87::IssueLinkLineageV87>,
    // Captured from this actual database, not a caller-provided counter. Claim
    // excludes its own just-inserted row and compares this same witness again.
    issue_latest: Option<i64>,
}
/// No serde or saved-row constructor can issue a live seed.
pub(crate) struct IssueLinkOriginSeedV87(RecordedIssueLinkV87);
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IssueLinkOriginRecordV87 {
    pub(crate) seed: RecordedIssueLinkV87,
    pub(crate) reference: PrPublicationReference,
}

impl RecordedIssueLinkV87 {
    pub(in crate::pr_write_journal) fn matches_manual_terminal_v94(
        &self,
        command: IssueLinkCommandV87,
        parent: &crate::providers::gh::PrResourceParentV1,
    ) -> bool {
        self.command == command.command() && self.same_object(parent)
    }
    pub(in crate::pr_write_journal) fn manual_plan_digest_v94(&self) -> &str {
        &self.plan_sha256
    }
    // Closed native-terminal read constraint only; not an eligible choice or
    // a general origin getter. No body, seed, permission or source is returned.
    pub(in crate::pr_write_journal) fn selected_issue_v94(
        &self,
    ) -> (&str, u64, &str, &str, u64, u64) {
        (
            &self.issue_repository,
            self.issue_repository_id,
            &self.issue_repository_node,
            &self.issue_node,
            self.issue_id,
            self.issue_number,
        )
    }
}

impl IssueLinkOriginSeedV87 {
    pub(crate) fn from_producer(
        source: &PrCollaborationSourceV17,
        actual: &Arc<OriginalIssueLinksV87>,
        plan: &IssueLinkPlanV87,
        database: &Path,
    ) -> Result<Self> {
        if actual
            .retained_bound_v87()
            .checked_add(FIXED_OVERLAP_V87)
            .is_none_or(|bytes| bytes > 4 * 1024 * 1024)
        {
            bail!("Actual live Issue model has no fixed origin/SQL overlap before allocation");
        }
        let facts = actual
            .validated_origin_facts_v87(plan)
            .map_err(anyhow::Error::msg)?;
        let parent = facts.parent;
        let scope = Scope::github(&facts.identity.host, facts.identity.account_id)?;
        if source.opening().database() != database
            || source.opening().scope() != &scope
            || !source.opening().admits_issue_links_v87(facts.command)
            || !GithubProvider::same_thread_history_parent_v1(source.parent(), parent)
        {
            bail!("Actual Issue producer/closed source/database/purpose differs");
        }
        // Requested-layout/escaped-copy proof BEFORE the first origin String.
        // Each dynamic producer scalar is bounded by the strict raw decoder.
        // The fixed 64KiB SQL/origin overlap includes two record moves, all
        // escaped scalar serializations and the at-most16KiB encoded record.
        let scalar_bytes = [
            facts.identity.host.as_str(),
            scope.login.as_str(),
            parent.repository_node(),
            parent.repository(),
            parent.node_id(),
            facts.issue_node,
            facts.issue_repository_node,
            facts.issue_repository,
            parent.head_sha(),
            parent.base_tip_sha(),
            facts.body_digest,
            facts.plan_digest,
        ]
        .into_iter()
        .try_fold(0usize, |sum, value| {
            if value.len() > 1024 {
                return None;
            }
            sum.checked_add(value.len())
        })
        .context("Issue origin scalar bounds exceed preclone capacity")?;
        let overlap = scalar_bytes
            .checked_mul(8)
            .and_then(|n| n.checked_add(2 * std::mem::size_of::<RecordedIssueLinkV87>()))
            .and_then(|n| n.checked_add(ORIGIN_CAP_V87))
            .filter(|n| *n <= FIXED_OVERLAP_V87)
            .context("Issue origin fixed overlap exceeds64KiB before clone")?;
        if actual
            .retained_bound_v87()
            .checked_add(overlap)
            .is_none_or(|bytes| bytes > 4 * 1024 * 1024)
        {
            bail!("Live original plus origin copies exceeds combined4MiB before clone");
        }
        let mut record = RecordedIssueLinkV87 {
            version: 87,
            database: DatabaseV3::capture(database)?,
            scope,
            repository_id: parent.repository_id(),
            repository_node: parent.repository_node().into(),
            repository: parent.repository().into(),
            pr_id: parent.database_id(),
            pr_node: parent.node_id().into(),
            number: parent.number(),
            issue_repository_id: facts.issue_repository_id,
            issue_repository_node: facts.issue_repository_node.into(),
            issue_repository: facts.issue_repository.into(),
            issue_id: facts.issue_id,
            issue_node: facts.issue_node.into(),
            issue_number: facts.issue_number,
            command: facts.command.command().into(),
            head: parent.head_sha().into(),
            base: parent.base_tip_sha().into(),
            body_null: facts.body_null,
            body_sha256: facts.body_digest.into(),
            plan_sha256: facts.plan_digest.into(),
            base_key: String::new(),
            draft_key: String::new(),
            parent_ack_attempt: None,
            lineage: None,
            issue_latest: None,
        };
        record.base_key = record.stable_key()?;
        record.draft_key = record.root_draft_key()?;
        // SQL is worker-only and BEFORE original Core final UID. This captures
        // the selected Issue's latest real attempt across PRs, never authority
        // from a list row or an inferred scalar. No row is written here.
        let journal = PrWriteJournal::open(database)?;
        record.issue_latest = selected_latest(&journal.connection, &journal.path, &record, None)?;
        require_secondary_unblocked(&journal.connection, &journal.path, &record, None)?;
        record.validate()?;
        Ok(Self(record))
    }
    pub(crate) fn values(&self) -> Result<BTreeMap<String, String>> {
        self.0.values()
    }
    pub(crate) fn draft_key(&self) -> &str {
        &self.0.draft_key
    }
    pub(crate) fn write(&self, values: &BTreeMap<String, String>) -> Result<PrWriteIntent> {
        self.0.write(values)
    }
    pub(crate) fn matches_receipt(&self, receipt: &PrConfirmedEffect) -> Result<bool> {
        self.0.validate()?;
        Ok(*receipt == self.0.expected_effect())
    }
}
impl RecordedIssueLinkV87 {
    fn same_object(&self, parent: &crate::providers::gh::PrResourceParentV1) -> bool {
        self.repository_id == parent.repository_id()
            && self.repository_node == parent.repository_node()
            && self.repository == parent.repository()
            && self.pr_id == parent.database_id()
            && self.pr_node == parent.node_id()
            && self.number == parent.number()
    }
    pub(in crate::pr_write_journal) fn native_terminal_fields_v87(
        &self,
        actual: &Arc<OriginalIssueLinksV87>,
        plan: &IssueLinkPlanV87,
        effect: &PrConfirmedEffect,
    ) -> Result<(String, u64, u64)> {
        self.validate()?;
        if self.plan_sha256 != plan.digest()
            || self.command != plan.command().command()
            || self.pr_id != actual.parent().database_id()
            || self.pr_node != actual.parent().node_id()
            || self.repository_id != actual.parent().repository_id()
            || self.repository_node != actual.parent().repository_node()
            || self.repository != actual.parent().repository()
            || self.number != actual.parent().number()
            || self.head != actual.parent().head_sha()
            || self.base != actual.parent().base_tip_sha()
            || *effect != self.expected_effect()
        {
            bail!("Actual Issue original differs from typed native receipt/parent/plan");
        }
        Ok((
            self.issue_node.clone(),
            self.issue_id,
            self.issue_repository_id,
        ))
    }
    fn stable_key(&self) -> Result<String> {
        let raw = bounded_encode(&(
            "ghtui-manual-issue-base-v87",
            &self.scope,
            self.repository_id,
            &self.repository_node,
            &self.repository,
            self.pr_id,
            &self.pr_node,
            self.number,
            self.issue_repository_id,
            &self.issue_repository_node,
            &self.issue_repository,
            self.issue_id,
            &self.issue_node,
            self.issue_number,
            &self.command,
        ))?;
        Ok(format!("issue-link-base-v87/{}", digest(raw.as_bytes())))
    }
    fn root_draft_key(&self) -> Result<String> {
        let raw = bounded_encode(&(
            "ghtui-manual-issue-draft-v87",
            &self.base_key,
            &self.head,
            &self.base,
        ))?;
        Ok(format!("issue-link-draft-v87/{}", digest(raw.as_bytes())))
    }
    fn values(&self) -> Result<BTreeMap<String, String>> {
        // A selected Issue intent, not a permission or a hidden body keyword.
        let body = bounded_encode(&(
            87u8,
            &self.command,
            self.issue_repository_id,
            &self.issue_repository,
            self.issue_id,
            &self.issue_node,
            self.issue_number,
        ))?;
        Ok(BTreeMap::from([("body".into(), body)]))
    }
    fn expected_effect(&self) -> PrConfirmedEffect {
        PrConfirmedEffect {
            kind: PrEffectKind::Metadata,
            resource_id: self.issue_id.to_string(),
            node_id: Some(self.issue_node.clone()),
        }
    }
    fn validate(&self) -> Result<()> {
        let uid = self
            .scope
            .login
            .strip_prefix("user-id:")
            .and_then(|s| s.parse::<u64>().ok());
        if self.version != 87
            || uid.is_none_or(|id| !positive(id))
            || Scope::github(&self.scope.host, uid)? != self.scope
            || ![
                self.repository_id,
                self.pr_id,
                self.number,
                self.issue_repository_id,
                self.issue_id,
                self.issue_number,
            ]
            .into_iter()
            .all(positive)
            || !crate::search::model::valid_sha(&self.head)
            || !crate::search::model::valid_sha(&self.base)
            || ![&self.body_sha256, &self.plan_sha256]
                .into_iter()
                .all(|s| canonical_digest(s))
            || self.issue_latest.is_some_and(|id| id <= 0)
            || self.parent_ack_attempt.is_some_and(|id| id <= 0)
        {
            bail!("Malformed immutable manual Issue origin");
        }
        command(&self.command)?;
        validate_repository(&self.repository)?;
        validate_repository(&self.issue_repository)?;
        for value in [
            &self.repository_node,
            &self.pr_node,
            &self.issue_repository_node,
            &self.issue_node,
        ] {
            if value.len() > 1024 {
                bail!("Issue original identity exceeds bound");
            }
            canonical_identifier(value)?;
        }
        if self.base_key != self.stable_key()? {
            bail!("Issue stable selected identity differs");
        }
        if self.lineage.is_some() {
            lineage_v87::validate_record_v87(self)?;
        } else if self.draft_key != self.root_draft_key()? || self.parent_ack_attempt.is_some() {
            bail!("Issue root key differs; explicit New requires actual terminal lineage");
        }
        bounded_encode(self)?;
        Ok(())
    }
    pub(in crate::pr_write_journal) fn write(
        &self,
        values: &BTreeMap<String, String>,
    ) -> Result<PrWriteIntent> {
        self.validate()?;
        if values.len() != 1 || values != &self.values()? {
            bail!("Accepted Issue envelope differs from actual selected Issue");
        }
        let mut bound = values.clone();
        bound.insert(
            "origin-issue-v87".into(),
            digest(bounded_encode(self)?.as_bytes()),
        );
        bound.insert("requested-plan-v87".into(), self.plan_sha256.clone());
        Ok(PrWriteIntent {
            command: self.command.clone(),
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
        if self.write(&self.values()?)?.digest()? != marker.payload_sha256
            || self.database != DatabaseV3::capture(path)?
            || target != &marker.target
            || target.scope() != &self.scope
            || target.repository() != self.repository
            || target.node_id() != self.pr_node
            || target.number() != self.number
            || marker.command != self.command
            || marker.selected_resource != self.draft_key
            || marker.observed_head != self.head
        {
            bail!("Exact Issue origin/database/PR/reference differs");
        }
        match marker.state {
            PrWriteState::Confirmed
                if marker.confirmed.as_ref() == Some(&self.expected_effect()) => {}
            PrWriteState::Pending | PrWriteState::Uncertain | PrWriteState::Acknowledged
                if marker.confirmed.is_none() => {}
            _ => bail!("Issue state does not have its exact actual receipt"),
        }
        Ok(())
    }
}

pub(super) fn migrate(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS pr_write_issue_link_origins_v87(
        attempt_id INTEGER PRIMARY KEY REFERENCES pr_write_attempts(attempt_id) ON DELETE CASCADE,
        host TEXT NOT NULL,principal TEXT NOT NULL,issue_node TEXT NOT NULL,
        issue_id INTEGER NOT NULL,issue_repository_id INTEGER NOT NULL,
        origin_json TEXT NOT NULL,base_key TEXT NOT NULL,action_tag TEXT NOT NULL,
        parent_ack_attempt INTEGER NULL);
        CREATE INDEX IF NOT EXISTS pr_write_issue_link_selected_v87 ON pr_write_issue_link_origins_v87(host,principal,issue_node,attempt_id DESC);
        CREATE INDEX IF NOT EXISTS pr_write_issue_link_lineage_v87 ON pr_write_issue_link_origins_v87(base_key,action_tag,attempt_id DESC);
        CREATE TRIGGER IF NOT EXISTS pr_write_issue_link_cleanup_v87 AFTER DELETE ON pr_write_attempts BEGIN DELETE FROM pr_write_issue_link_origins_v87 WHERE attempt_id=OLD.attempt_id;END;
        CREATE TRIGGER IF NOT EXISTS pr_write_issue_link_immutable_v87 BEFORE UPDATE ON pr_write_issue_link_origins_v87 BEGIN SELECT RAISE(ABORT,'Immutable manual Issue original');END;")?;
    Ok(())
}
pub(super) fn insert(
    tx: &Transaction<'_>,
    seed: &IssueLinkOriginSeedV87,
    marker: &PrWriteMarker,
) -> Result<()> {
    let record = IssueLinkOriginRecordV87 {
        seed: seed.0.clone(),
        reference: marker.publication_reference(),
    };
    let raw = bounded_encode(&record)?;
    tx.execute("INSERT INTO pr_write_issue_link_origins_v87(attempt_id,host,principal,issue_node,issue_id,issue_repository_id,origin_json,base_key,action_tag,parent_ack_attempt) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        params![marker.attempt_id,seed.0.scope.host,seed.0.scope.login,seed.0.issue_node,
            seed.0.issue_id as i64,seed.0.issue_repository_id as i64,raw,seed.0.base_key,seed.0.command,seed.0.parent_ack_attempt])?;
    Ok(())
}

fn require_secondary_unblocked(
    connection: &Connection,
    path: &Path,
    current: &RecordedIssueLinkV87,
    own: Option<i64>,
) -> Result<()> {
    // Missing/malformed known pending origins cannot masquerade as unrelated
    // issues. This is a scalar predicate BEFORE any historical JSON is copied.
    let malformed:bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM pr_write_attempts a LEFT JOIN pr_write_issue_link_origins_v87 o ON o.attempt_id=a.attempt_id WHERE a.host=?1 AND a.principal=?2 AND a.command IN('issue-link-v87','issue-unlink-manual-v87') AND a.state IN('pending','uncertain') AND (?3 IS NULL OR a.attempt_id<>?3) AND (o.attempt_id IS NULL OR typeof(o.host)<>'text' OR typeof(o.principal)<>'text' OR typeof(o.issue_node)<>'text' OR length(CAST(o.issue_node AS BLOB)) NOT BETWEEN 1 AND 1024 OR typeof(o.issue_id)<>'integer' OR o.issue_id<=0 OR typeof(o.issue_repository_id)<>'integer' OR o.issue_repository_id<=0 OR typeof(o.origin_json)<>'text' OR length(CAST(o.origin_json AS BLOB)) NOT BETWEEN 1 AND 16384 OR o.host<>a.host OR o.principal<>a.principal OR o.action_tag<>a.command))",params![current.scope.host,current.scope.login,own],|r|r.get(0))?;
    if malformed {
        bail!("Known unresolved Issue original missing or malformed; no fallback");
    }
    // At most64 unresolved known-purpose rows are materialized, one bounded
    // record at a time. A corrupt JSON/index/foreign-origin row cannot hide
    // behind another Issue's index and thereby be treated as unrelated.
    let mut query=connection.prepare("SELECT attempt_id FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND command IN('issue-link-v87','issue-unlink-manual-v87') AND state IN('pending','uncertain') AND (?3 IS NULL OR attempt_id<>?3) ORDER BY attempt_id LIMIT 65")?;
    let ids = query
        .query_map(params![current.scope.host, current.scope.login, own], |r| {
            r.get::<_, i64>(0)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if ids.len() > 64 {
        bail!("Known unresolved Issue scope exceeds bounded64-row guard; inspect first");
    }
    for id in ids {
        let target = bounded_target(connection, &current.scope, id)?;
        let raw=connection.query_row("SELECT attempt_id,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at FROM pr_write_attempts WHERE attempt_id=?1",[id],read_marker)?;
        let marker = decode_marker(&target, raw)?;
        load_record(connection, path, &target, &marker)?;
    }
    let blocked:bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM pr_write_issue_link_origins_v87 o JOIN pr_write_attempts a ON a.attempt_id=o.attempt_id WHERE o.host=?1 AND o.principal=?2 AND o.issue_node=?3 AND a.state IN('pending','uncertain') AND (?4 IS NULL OR a.attempt_id<>?4))",params![current.scope.host,current.scope.login,current.issue_node,own],|r|r.get(0))?;
    if blocked {
        bail!("Selected Issue has an unresolved publication on a PR; inspect before a new write");
    }
    Ok(())
}
fn selected_latest(
    connection: &Connection,
    path: &Path,
    current: &RecordedIssueLinkV87,
    own: Option<i64>,
) -> Result<Option<i64>> {
    let id:Option<i64> = connection.query_row("SELECT MAX(attempt_id) FROM pr_write_issue_link_origins_v87 WHERE host=?1 AND principal=?2 AND issue_node=?3 AND (?4 IS NULL OR attempt_id<>?4)",params![current.scope.host,current.scope.login,current.issue_node,own],|r|r.get(0))?;
    if let Some(id) = id {
        let target = bounded_target(connection, &current.scope, id)?;
        let raw=connection.query_row("SELECT attempt_id,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at FROM pr_write_attempts WHERE attempt_id=?1",[id],read_marker)?;
        let marker = decode_marker(&target, raw)?;
        let record = load_record(connection, path, &target, &marker)?;
        if record.seed.issue_id != current.issue_id
            || record.seed.issue_node != current.issue_node
            || record.seed.issue_repository_id != current.issue_repository_id
            || record.seed.issue_repository_node != current.issue_repository_node
            || record.seed.issue_repository != current.issue_repository
        {
            bail!("Newest selected Issue index differs from its exact canonical identity");
        }
    }
    Ok(id)
}
fn bounded_target(connection: &Connection, scope: &Scope, id: i64) -> Result<PrWriteTarget> {
    let valid:bool = connection.query_row("SELECT typeof(pr_node)='text' AND length(CAST(pr_node AS BLOB)) BETWEEN 1 AND 1024 AND typeof(repository)='text' AND length(CAST(repository AS BLOB)) BETWEEN 1 AND 256 AND typeof(pr_number)='integer' AND pr_number>0 AND typeof(command)='text' AND command IN('issue-link-v87','issue-unlink-manual-v87') AND (length(CAST(repository AS BLOB))+length(CAST(command AS BLOB))+length(CAST(selected_resource AS BLOB))+length(CAST(observed_head AS BLOB))+length(CAST(payload_sha256 AS BLOB))+length(CAST(COALESCE(confirmed_json,'') AS BLOB)))<=8192 FROM pr_write_attempts WHERE attempt_id=?1 AND host=?2 AND principal=?3",params![id,scope.host,scope.login],|r|r.get(0))?;
    if !valid {
        bail!("Known selected Issue attempt malformed before copy; no fallback");
    }
    let (node, repo, number): (String, String, u64) = connection.query_row(
        "SELECT pr_node,repository,pr_number FROM pr_write_attempts WHERE attempt_id=?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    PrWriteTarget::for_source(scope, &repo, &node, number)
}
pub(super) fn validate_begin(
    tx: &Transaction<'_>,
    path: &Path,
    target: &PrWriteTarget,
    seed: &IssueLinkOriginSeedV87,
) -> Result<()> {
    seed.0.validate()?;
    if seed.0.database != DatabaseV3::capture(path)?
        || target.scope() != &seed.0.scope
        || target.node_id() != seed.0.pr_node
        || target.repository() != seed.0.repository
        || target.number() != seed.0.number
    {
        bail!("Actual Issue original differs before begin");
    }
    require_secondary_unblocked(tx, path, &seed.0, None)?;
    if selected_latest(tx, path, &seed.0, None)? != seed.0.issue_latest {
        bail!("Selected Issue latest attempt changed before begin; fresh preview required");
    }
    if let Some(lineage) = &seed.0.lineage {
        return conversation_action_lineage_v18::kernel_v52::validate_begin_v52(
            tx,
            path,
            conversation_action_lineage_v18::kernel_v52::CompareV52::IssueLinksV87(&seed.0),
            target,
            &lineage.captured,
        );
    }
    let prior:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM pr_write_issue_link_origins_v87 WHERE base_key=?1 AND action_tag=?2)",params![seed.0.base_key,seed.0.command],|r|r.get(0))?;
    if prior {
        bail!("Existing Issue action requires exact explicit New lineage, not a root reset");
    }
    Ok(())
}
pub(super) fn validate_claim(
    tx: &Transaction<'_>,
    path: &Path,
    target: &PrWriteTarget,
    marker: &PrWriteMarker,
) -> Result<()> {
    if !recognized(&marker.command) {
        let graft: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM pr_write_issue_link_origins_v87 WHERE attempt_id=?1)",
            [marker.attempt_id],
            |r| r.get(0),
        )?;
        if graft {
            bail!("Foreign Issue origin grafted onto another purpose");
        }
        return Ok(());
    }
    let record = load_record(tx, path, target, marker)?;
    require_secondary_unblocked(tx, path, &record.seed, Some(marker.attempt_id))?;
    if selected_latest(tx, path, &record.seed, Some(marker.attempt_id))? != record.seed.issue_latest
    {
        bail!("Selected Issue latest attempt changed after begin; no mutation admitted");
    }
    Ok(())
}
pub(super) fn require_absent(connection: &Connection, attempt: i64) -> Result<()> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM pr_write_issue_link_origins_v87 WHERE attempt_id=?1)",
        [attempt],
        |r| r.get(0),
    )?;
    if exists {
        bail!("Opposite manual Issue origin is not this native purpose");
    }
    Ok(())
}
pub(super) fn load_record(
    connection: &Connection,
    path: &Path,
    target: &PrWriteTarget,
    marker: &PrWriteMarker,
) -> Result<IssueLinkOriginRecordV87> {
    let valid:bool=connection.query_row("SELECT typeof(host)='text' AND typeof(principal)='text' AND typeof(issue_node)='text' AND length(CAST(issue_node AS BLOB)) BETWEEN 1 AND 1024 AND typeof(issue_id)='integer' AND issue_id>0 AND typeof(issue_repository_id)='integer' AND issue_repository_id>0 AND typeof(origin_json)='text' AND length(CAST(origin_json AS BLOB)) BETWEEN 1 AND 16384 AND typeof(base_key)='text' AND length(CAST(base_key AS BLOB)) BETWEEN 1 AND 128 AND typeof(action_tag)='text' AND length(CAST(action_tag AS BLOB)) BETWEEN 1 AND 64 AND (typeof(parent_ack_attempt)='null' OR (typeof(parent_ack_attempt)='integer' AND parent_ack_attempt>0)) FROM pr_write_issue_link_origins_v87 WHERE attempt_id=?1",[marker.attempt_id],|r|r.get(0))?;
    if !valid {
        bail!("Missing/malformed known manual Issue original before copy");
    }
    let (host,principal,node,id,repo,raw,base,action,ack):(String,String,String,u64,u64,String,String,String,Option<i64>)=connection.query_row("SELECT host,principal,issue_node,issue_id,issue_repository_id,origin_json,base_key,action_tag,parent_ack_attempt FROM pr_write_issue_link_origins_v87 WHERE attempt_id=?1",[marker.attempt_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?,r.get(8)?)))?;
    let record: IssueLinkOriginRecordV87 = serde_json::from_str(&raw)?;
    record.seed.validate_marker(path, target, marker)?;
    if record.reference != marker.publication_reference()
        || record.seed.scope.host != host
        || record.seed.scope.login != principal
        || record.seed.issue_node != node
        || record.seed.issue_id != id
        || record.seed.issue_repository_id != repo
        || record.seed.base_key != base
        || record.seed.command != action
        || record.seed.parent_ack_attempt != ack
    {
        bail!("Manual Issue origin raw indices/reference differ");
    }
    let foreign:bool=connection.query_row("SELECT EXISTS(SELECT 1 FROM pr_write_conversation_origins_v17 WHERE attempt_id=?1) OR EXISTS(SELECT 1 FROM pr_write_metadata_origins_v25 WHERE attempt_id=?1) OR EXISTS(SELECT 1 FROM pr_write_metadata_base_origins_v32 WHERE attempt_id=?1) OR EXISTS(SELECT 1 FROM pr_write_review_operation_origins_v3 WHERE attempt_id=?1) OR EXISTS(SELECT 1 FROM pr_write_thread_origins_v1 WHERE attempt_id=?1) OR EXISTS(SELECT 1 FROM pr_write_thread_comment_origins_v2 WHERE attempt_id=?1) OR EXISTS(SELECT 1 FROM pr_write_thread_reaction_origins_v3 WHERE attempt_id=?1) OR EXISTS(SELECT 1 FROM pr_write_file_comment_origins_v75 WHERE attempt_id=?1) OR EXISTS(SELECT 1 FROM pr_write_state_origins_v68 WHERE attempt_id=?1) OR EXISTS(SELECT 1 FROM pr_write_state_dispositions_v68 WHERE attempt_id=?1)",[marker.attempt_id],|r|r.get(0))?;
    if foreign {
        bail!("Opposite origin graft cannot expose an Issue link history or ACK");
    }
    Ok(record)
}
impl PrWriteJournal {
    pub(crate) fn begin_issue_link_v87(
        &mut self,
        target: &PrWriteTarget,
        write: &PrWriteIntent,
        seed: &IssueLinkOriginSeedV87,
    ) -> Result<PrWritePermit> {
        if seed.write(&seed.values()?)?.digest()? != write.digest()?
            || seed.0.database != DatabaseV3::capture(&self.path)?
        {
            bail!("Issue seed differs from exact producer publication");
        }
        self.begin_with_observed_origin_v2(
            target,
            write,
            Some(ObservedOriginV2::IssueLinkV87(seed)),
        )
    }
    pub(crate) fn issue_link_origin_v87(
        &self,
        target: &PrWriteTarget,
        marker: &PrWriteMarker,
    ) -> Result<IssueLinkOriginRecordV87> {
        load_record(&self.connection, &self.path, target, marker)
    }
}
