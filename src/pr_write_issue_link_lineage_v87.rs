//! Manual Issue originals enter the SAME bounded V52 terminal walk. No saved
//! row, ACK or receipt constructs an actual current action Base.
use super::*;
use crate::pr_write_journal::conversation_action_lineage_v18::{
    kernel_v52::{self, CapturedV52, CompareV52, FamilyV52},
    TerminalKindV18, TerminalV18,
};
use crate::scoped_pr_collaboration_source_v17::issue_links_phase_v87::IssueLinkPhaseV87;

#[derive(Clone)]
pub(crate) struct IssueLinkActionBaseV87 {
    current: RecordedIssueLinkV87,
    target: PrWriteTarget,
}
pub(crate) struct IssueLinkTerminalRevisionV87 {
    base: IssueLinkActionBaseV87,
    captured: CapturedV52,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct IssueLinkLineageV87 {
    pub(in crate::pr_write_journal) captured: CapturedV52,
    current_head_bound_draft: String,
}
impl IssueLinkActionBaseV87 {
    pub(crate) fn from_producer(
        seed: &IssueLinkOriginSeedV87,
        phase: &IssueLinkPhaseV87<'_>,
        path: &Path,
    ) -> Result<Self> {
        seed.0.validate()?;
        let facts = phase
            .actual()
            .validated_origin_facts_v87(phase.plan())
            .map_err(anyhow::Error::msg)?;
        if std::time::Instant::now() >= phase.deadline()
            || seed.0.database != DatabaseV3::capture(path)?
            || seed.0.lineage.is_some()
            || seed.0.plan_sha256 != facts.plan_digest
            || seed.0.command != facts.command.command()
            || seed.0.pr_id != facts.parent.database_id()
            || seed.0.pr_node != facts.parent.node_id()
            || seed.0.head != facts.parent.head_sha()
            || seed.0.base != facts.parent.base_tip_sha()
            || seed.0.repository_id != facts.parent.repository_id()
            || seed.0.issue_id != facts.issue_id
            || seed.0.issue_node != facts.issue_node
            || seed.0.issue_repository_id != facts.issue_repository_id
            || seed.0.body_null != facts.body_null
            || seed.0.body_sha256 != facts.body_digest
        {
            bail!("Actual live Issue phase and root seed required for action Base");
        }
        let target = PrWriteTarget::for_source(
            &seed.0.scope,
            &seed.0.repository,
            &seed.0.pr_node,
            seed.0.number,
        )?;
        Ok(Self {
            current: seed.0.clone(),
            target,
        })
    }
    pub(in crate::pr_write_journal) fn database_v87(&self) -> &DatabaseV3 {
        &self.current.database
    }
    pub(in crate::pr_write_journal) fn target_v87(&self) -> &PrWriteTarget {
        &self.target
    }
    pub(in crate::pr_write_journal) fn observed_v87(&self) -> &RecordedIssueLinkV87 {
        &self.current
    }
    pub(in crate::pr_write_journal) fn current_draft_v87(&self) -> &str {
        &self.current.draft_key
    }
}
impl IssueLinkTerminalRevisionV87 {
    pub(crate) fn reference(&self) -> &PrPublicationReference {
        &self.captured.terminal.reference
    }
    pub(crate) fn label(&self) -> &'static str {
        match self.captured.terminal.kind {
            TerminalKindV18::Acknowledged => {
                "Explicit local acknowledgement; Issue-link outcome may remain unknown"
            }
            TerminalKindV18::Confirmed if self.captured.protected_fingerprint.is_some() => {
                "Confirmed manual Issue receipt; protected draft retained (NOT acknowledged)"
            }
            TerminalKindV18::Confirmed => {
                "Confirmed manual Issue receipt; original draft absent (NOT acknowledged)"
            }
        }
    }
}
impl RecordedIssueLinkV87 {
    pub(in crate::pr_write_journal) fn base_key_v87(&self) -> &str {
        &self.base_key
    }
    pub(in crate::pr_write_journal) fn command_v87(&self) -> Result<IssueLinkCommandV87> {
        command(&self.command)
    }
    pub(in crate::pr_write_journal) fn draft_key_v87(&self) -> &str {
        &self.draft_key
    }
    pub(in crate::pr_write_journal) fn lineage_v87(&self) -> Option<&CapturedV52> {
        self.lineage.as_ref().map(|v| &v.captured)
    }
    pub(in crate::pr_write_journal) fn next_key_v87(
        &self,
        terminal: &TerminalV18,
    ) -> Result<String> {
        let raw = bounded_encode(&(
            "ghtui-explicit-new-manual-issue-v87",
            &self.base_key,
            &self.command,
            &terminal.reference,
        ))?;
        Ok(format!("issue-link-draft-v87/{}", digest(raw.as_bytes())))
    }
    pub(in crate::pr_write_journal) fn matches_stable_v87(&self, other: &Self) -> Result<()> {
        self.validate()?;
        other.validate()?;
        if self.database != other.database
            || self.scope != other.scope
            || self.repository_id != other.repository_id
            || self.repository_node != other.repository_node
            || self.repository != other.repository
            || self.pr_id != other.pr_id
            || self.pr_node != other.pr_node
            || self.number != other.number
            || self.issue_repository_id != other.issue_repository_id
            || self.issue_repository_node != other.issue_repository_node
            || self.issue_repository != other.issue_repository
            || self.issue_id != other.issue_id
            || self.issue_node != other.issue_node
            || self.issue_number != other.issue_number
            || self.command != other.command
            || self.base_key != other.base_key
        {
            bail!("Stable manual Issue action/database differs; mutable H/B and sets are not history authority");
        }
        Ok(())
    }
}
pub(super) fn validate_record_v87(current: &RecordedIssueLinkV87) -> Result<()> {
    let line = current
        .lineage
        .as_ref()
        .context("Exact Issue lineage missing")?;
    let c = &line.captured;
    if c.database != current.database
        || c.generation < 0
        || c.key != current.draft_key
        || c.key.len() > 128
        || c.protected_key.len() > 256
        || line.current_head_bound_draft != current.root_draft_key()?
        || c.terminal.reference.attempt_id <= 0
        || c.parent_ack != current.parent_ack_attempt
        || c.parent_ack != c.root_ack.as_ref().map(|t| t.reference.attempt_id)
        || c.parent_ack
            .is_some_and(|id| id <= 0 || id > c.terminal.reference.attempt_id)
        || !canonical_digest(&c.terminal.raw_attempt_sha256)
        || !canonical_digest(&c.terminal.raw_origin_sha256)
        || c.protected_fingerprint
            .as_ref()
            .is_some_and(|s| !canonical_digest(s))
        || c.key != current.next_key_v87(&c.terminal)?
    {
        bail!("Malformed exact Issue terminal lineage/current draft/database");
    }
    if let Some(root) = &c.root_ack {
        if root.kind != TerminalKindV18::Acknowledged
            || root.receipt.is_some()
            || !canonical_digest(&root.raw_attempt_sha256)
            || !canonical_digest(&root.raw_origin_sha256)
        {
            bail!("Issue root ACK is not an exact acknowledged terminal");
        }
    }
    match (c.terminal.kind, &c.terminal.receipt) {
        (TerminalKindV18::Acknowledged, None) | (TerminalKindV18::Confirmed, Some(_)) => {}
        _ => bail!("Unknown ACK must not become a Confirmed Issue receipt"),
    }
    bounded_encode(line)?;
    Ok(())
}
impl IssueLinkOriginSeedV87 {
    pub(crate) fn from_revision_v87(
        current: Self,
        base: &IssueLinkActionBaseV87,
        revision: &Arc<IssueLinkTerminalRevisionV87>,
    ) -> Result<Self> {
        current.0.matches_stable_v87(&base.current)?;
        base.current.matches_stable_v87(&revision.base.current)?;
        if current.0.head != base.current.head
            || current.0.base != base.current.base
            || base.current.head != revision.base.current.head
            || base.current.base != revision.base.current.base
        {
            bail!("Fresh Issue Base and current H/B differ before binding New");
        }
        let mut record = current.0;
        let current_head_bound_draft = record.draft_key.clone();
        record.draft_key = revision.captured.key.clone();
        record.parent_ack_attempt = revision.captured.parent_ack;
        record.lineage = Some(IssueLinkLineageV87 {
            captured: revision.captured.clone(),
            current_head_bound_draft,
        });
        record.validate()?;
        Ok(Self(record))
    }
}
pub(in crate::pr_write_journal) struct StoredIssueLinkV87 {
    pub marker: PrWriteMarker,
    pub record: IssueLinkOriginRecordV87,
    attempt_hash: String,
    origin_hash: String,
}
impl StoredIssueLinkV87 {
    pub(in crate::pr_write_journal) fn load(
        connection: &Connection,
        path: &Path,
        target: &PrWriteTarget,
        id: i64,
    ) -> Result<Self> {
        let (marker, attempt_hash) = kernel_v52::stored_attempt_v52(connection, target, id)?;
        let record = super::load_record(connection, path, target, &marker)?;
        // load_record proved raw scalar types/byte limits and every index.
        let raw:(String,String,String,u64,u64,String,String,String,Option<i64>)=connection.query_row("SELECT host,principal,issue_node,issue_id,issue_repository_id,origin_json,base_key,action_tag,parent_ack_attempt FROM pr_write_issue_link_origins_v87 WHERE attempt_id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?,r.get(8)?)))?;
        let origin_hash = digest(bounded_encode(&raw)?.as_bytes());
        if record
            .seed
            .lineage_v87()
            .is_some_and(|line| line.terminal.reference.attempt_id >= marker.attempt_id)
        {
            bail!("Nonmonotonic manual Issue terminal lineage");
        }
        Ok(Self {
            marker,
            record,
            attempt_hash,
            origin_hash,
        })
    }
    pub(in crate::pr_write_journal) fn terminal(&self) -> Result<TerminalV18> {
        kernel_v52::terminal_marker_v52(&self.marker, &self.attempt_hash, &self.origin_hash)
    }
}
impl PrWriteJournal {
    pub(crate) fn issue_link_action_revision_v87(
        &self,
        base: &IssueLinkActionBaseV87,
    ) -> Result<Option<Arc<IssueLinkTerminalRevisionV87>>> {
        Ok(
            kernel_v52::capture_revision_v52(self, FamilyV52::IssueLinksV87(base))?.map(
                |captured| {
                    Arc::new(IssueLinkTerminalRevisionV87 {
                        base: base.clone(),
                        captured,
                    })
                },
            ),
        )
    }
    pub(crate) fn validate_issue_link_revision_v87(
        &self,
        base: &IssueLinkActionBaseV87,
        revision: &IssueLinkTerminalRevisionV87,
    ) -> Result<()> {
        base.current.matches_stable_v87(&revision.base.current)?;
        if base.current.head != revision.base.current.head
            || base.current.base != revision.base.current.base
            || self
                .issue_link_action_revision_v87(base)?
                .as_ref()
                .is_none_or(|v| v.captured != revision.captured)
        {
            bail!("Actual current Issue terminal revision changed");
        }
        Ok(())
    }
    pub(crate) fn validate_owned_issue_link_v87(
        &self,
        seed: &IssueLinkOriginSeedV87,
        reference: &PrPublicationReference,
    ) -> Result<()> {
        seed.0.validate()?;
        let target = PrWriteTarget::for_source(
            &seed.0.scope,
            &seed.0.repository,
            &seed.0.pr_node,
            seed.0.number,
        )?;
        let marker = self
            .original_publication(&target, reference)?
            .context("Owned manual Issue attempt unavailable")?;
        let own = self.issue_link_origin_v87(&target, &marker)?;
        if bounded_encode(&own.seed)? != bounded_encode(&seed.0)? {
            bail!("Owned manual Issue original changed");
        }
        if let Some(lineage) = &seed.0.lineage {
            kernel_v52::validate_owned_basis_v52(
                &self.connection,
                &self.path,
                CompareV52::IssueLinksV87(&seed.0),
                &target,
                &lineage.captured,
                reference.attempt_id,
            )?;
        }
        Ok(())
    }
}
