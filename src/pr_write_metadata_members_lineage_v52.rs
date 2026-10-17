//! Producer-only Members base/revision around the ONE moved V18 kernel.
//! Serialized lineage facts are descriptive; deserialization issues no Base.
use super::*;
use crate::pr_write_journal::conversation_action_lineage_v18::{
    kernel_v52::{self, CapturedV52, CompareV52, FamilyV52},
    TerminalKindV18, TerminalV18,
};
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct MembersActionBaseV48 {
    current: RecordedMetadataMembersV48,
    target: PrWriteTarget,
}
pub(crate) struct MembersTerminalRevisionV48 {
    base: MembersActionBaseV48,
    captured: CapturedV52,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MembersLineageV52 {
    pub(in crate::pr_write_journal) captured: CapturedV52,
    current_head_bound_draft: String,
}
impl MembersActionBaseV48 {
    /// A genuine current nonSerde seed is required. Historical records cannot
    /// call this factory and no arbitrary target/source tuple is accepted.
    pub(crate) fn from_producer(
        seed: &MetadataMembersOriginSeedV48,
        phase:&crate::scoped_pr_collaboration_source_v17::metadata_members_phase_v48::MetadataMembersPhaseV48<'_>,
        plan: &PrMetadataPlan,
        path: &Path,
    ) -> Result<Self> {
        seed.0.validate()?;
        let actual = phase.actual();
        actual.require_mutation_v48().map_err(anyhow::Error::msg)?;
        plan.validate_original_members_v48(actual)
            .map_err(anyhow::Error::msg)?;
        let (base, draft) = actual.action_keys_v48(plan).map_err(anyhow::Error::msg)?;
        if std::time::Instant::now() >= phase.deadline()
            || !seed.0.matches_parent(actual.parent())
            || seed.0.command()? != plan.command()
            || seed.0.base_key != base
            || seed.0.draft_key != draft
            || seed.0.requested_plan_sha256 != plan.intent().digest()?
            || seed.0.expected_effect != plan.receipt().map_err(anyhow::Error::msg)?.effect()
            || seed.0.lineage_v52.is_some()
            || seed.0.database != DatabaseV3::capture(path)?
        {
            bail!("Actual fresh root seed/database required for Members Base");
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
    pub(in crate::pr_write_journal) fn database_v52(&self) -> &DatabaseV3 {
        &self.current.database
    }
    pub(in crate::pr_write_journal) fn target_v52(&self) -> &PrWriteTarget {
        &self.target
    }
    pub(in crate::pr_write_journal) fn observed_v52(&self) -> &RecordedMetadataMembersV48 {
        &self.current
    }
    pub(in crate::pr_write_journal) fn current_draft_v52(&self) -> &str {
        &self.current.draft_key
    }
}
impl MembersTerminalRevisionV48 {
    pub(crate) fn reference(&self) -> &PrPublicationReference {
        &self.captured.terminal.reference
    }
    pub(crate) fn label(&self) -> &'static str {
        match self.captured.terminal.kind {
            TerminalKindV18::Acknowledged => {
                "Explicit local acknowledgement; outcome may remain unknown"
            }
            TerminalKindV18::Confirmed if self.captured.protected_fingerprint.is_some() => {
                "Confirmed original receipt; retained protected draft (NOT acknowledged)"
            }
            TerminalKindV18::Confirmed => {
                "Confirmed original receipt; original draft absent (NOT acknowledged)"
            }
        }
    }
}
impl RecordedMetadataMembersV48 {
    pub(in crate::pr_write_journal) fn draft_key_v52(&self) -> &str {
        &self.draft_key
    }
    pub(in crate::pr_write_journal) fn lineage_v52(&self) -> Option<&CapturedV52> {
        self.lineage_v52.as_ref().map(|v| &v.captured)
    }
    pub(in crate::pr_write_journal) fn matches_stable_v52(&self, other: &Self) -> Result<()> {
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
            || self.issue_id != other.issue_id
            || self.issue_node != other.issue_node
            || self.action != other.action
            || self.selected_id != other.selected_id
            || self.selected_node != other.selected_node
            || self.selected_namespace_sha256 != other.selected_namespace_sha256
            || self.base_key != other.base_key
        {
            bail!("Exact stable Members action identity differs; current H/B never grant history authority");
        }
        Ok(())
    }
}
pub(super) fn validate_record_v52(current: &RecordedMetadataMembersV48) -> Result<()> {
    let line = current
        .lineage_v52
        .as_ref()
        .context("Exact Members lineage missing")?;
    let c = &line.captured;
    let root_draft = bounded_encode(&(
        "ghtui-metadata-members-draft-v48",
        &current.base_key,
        &current.head,
        &current.base,
    ))?;
    if c.database != current.database
        || c.generation < 0
        || c.key != current.draft_key
        || c.key.len() > 128
        || c.protected_key.len() > 256
        || line.current_head_bound_draft
            != format!("metadata-members-v48/{}", digest(root_draft.as_bytes()))
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
    {
        bail!("Malformed exact Members lineage/database/current head-bound draft");
    }
    if let Some(root) = &c.root_ack {
        if root.kind != TerminalKindV18::Acknowledged
            || root.receipt.is_some()
            || !canonical_digest(&root.raw_attempt_sha256)
            || !canonical_digest(&root.raw_origin_sha256)
        {
            bail!("Members root ACK is not an exact acknowledged terminal");
        }
    }
    match (c.terminal.kind, &c.terminal.receipt) {
        (TerminalKindV18::Acknowledged, None) | (TerminalKindV18::Confirmed, Some(_)) => {}
        _ => bail!("Members ACK and durable Confirmed must remain separate"),
    }
    // The stable identity is already checked by current.validate BEFORE this
    // helper; hashing here is comparison only and must not recursively validate.
    let encoded = bounded_encode(&(
        "ghtui-explicit-new-metadata-members-v48",
        current.base_key(),
        current.command()?.key(),
        &c.terminal.reference,
    ))?;
    if c.key != format!("metadata-members-v48/{}", digest(encoded.as_bytes())) {
        bail!("Members new key differs from exact terminal reference");
    }
    bounded_encode(line)?;
    Ok(())
}
impl MetadataMembersOriginSeedV48 {
    pub(crate) fn from_revision_v52(
        current: Self,
        base: &MembersActionBaseV48,
        revision: &Arc<MembersTerminalRevisionV48>,
    ) -> Result<Self> {
        current.0.matches_stable_v52(&base.current)?;
        base.current.matches_stable_v52(&revision.base.current)?;
        if current.0.head != base.current.head
            || current.0.base != base.current.base
            || base.current.head != revision.base.current.head
            || base.current.base != revision.base.current.base
        {
            bail!("Actual fresh Members Base/current H/B differs before binding New action");
        }
        let mut record = current.0;
        let current_head_bound_draft = record.draft_key.clone();
        record.draft_key = revision.captured.key.clone();
        record.parent_ack_attempt = revision.captured.parent_ack;
        record.lineage_v52 = Some(MembersLineageV52 {
            captured: revision.captured.clone(),
            current_head_bound_draft,
        });
        record.validate()?;
        Ok(Self(record))
    }
}
pub(in crate::pr_write_journal) struct StoredMembersV52 {
    pub marker: PrWriteMarker,
    pub record: MetadataMembersOriginRecordV48,
    attempt_hash: String,
    origin_hash: String,
}
impl StoredMembersV52 {
    pub(in crate::pr_write_journal) fn load(
        connection: &Connection,
        path: &Path,
        target: &PrWriteTarget,
        id: i64,
    ) -> Result<Self> {
        let lengths:(usize,usize,usize)=connection.query_row("SELECT length(CAST(origin_json AS BLOB)),COALESCE(length(CAST(base_key_v48 AS BLOB)),0),COALESCE(length(CAST(action_tag_v48 AS BLOB)),0) FROM pr_write_metadata_origins_v25 WHERE attempt_id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        if lengths.0 > 16384 || lengths.1 > 128 || lengths.2 > 64 {
            bail!("Member terminal metadata refused before copy");
        }
        let (marker, attempt_hash) = kernel_v52::stored_attempt_v52(connection, target, id)?;
        let record = super::load_record_v52(connection, path, target, &marker)?;
        let(repo,pr,json,base,action,ack):(u64,u64,String,String,String,Option<i64>)=connection.query_row("SELECT repository_id,pr_id,origin_json,base_key_v48,action_tag_v48,parent_ack_attempt_v48 FROM pr_write_metadata_origins_v25 WHERE attempt_id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?)))?;
        let origin_hash = digest(&serde_json::to_vec(&(
            repo, pr, &json, &base, &action, ack,
        ))?);
        if record
            .seed
            .lineage_v52()
            .is_some_and(|line| line.terminal.reference.attempt_id >= marker.attempt_id)
        {
            bail!("Nonmonotonic Members terminal lineage");
        }
        Ok(Self {
            marker,
            record,
            attempt_hash,
            origin_hash,
        })
    }
    pub(in crate::pr_write_journal) fn terminal(&self) -> Result<TerminalV18> {
        // load_record already checked SAME typed command/selected/issue/PR and
        // exact expected effect. No arbitrary known ID becomes Confirmed.
        kernel_v52::terminal_marker_v52(&self.marker, &self.attempt_hash, &self.origin_hash)
    }
}
impl PrWriteJournal {
    pub(crate) fn validate_owned_members_v52(
        &self,
        seed: &MetadataMembersOriginSeedV48,
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
            .context("Owned member original unavailable")?;
        let own = self.metadata_members_origin_v48(&target, &marker)?;
        if bounded_encode(&own.seed)? != bounded_encode(&seed.0)? {
            bail!("Owned member current original changed");
        }
        if let Some(lineage) = &seed.0.lineage_v52 {
            kernel_v52::validate_owned_basis_v52(
                &self.connection,
                &self.path,
                CompareV52::Members(&seed.0),
                &target,
                &lineage.captured,
                reference.attempt_id,
            )?;
        }
        Ok(())
    }
    pub(crate) fn members_action_revision_v52(
        &self,
        base: &MembersActionBaseV48,
    ) -> Result<Option<Arc<MembersTerminalRevisionV48>>> {
        Ok(
            kernel_v52::capture_revision_v52(self, FamilyV52::Members(base))?.map(|captured| {
                Arc::new(MembersTerminalRevisionV48 {
                    base: base.clone(),
                    captured,
                })
            }),
        )
    }
    pub(crate) fn validate_members_revision_v52(
        &self,
        base: &MembersActionBaseV48,
        revision: &MembersTerminalRevisionV48,
    ) -> Result<()> {
        base.current.matches_stable_v52(&revision.base.current)?;
        if base.current.head != revision.base.current.head
            || base.current.base != revision.base.current.base
            || self
                .members_action_revision_v52(base)?
                .as_ref()
                .is_none_or(|current| current.captured != revision.captured)
        {
            bail!("Exact current Members terminal revision moved");
        }
        Ok(())
    }
}
