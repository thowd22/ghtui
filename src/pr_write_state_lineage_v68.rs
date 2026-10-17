//! State is the third CLOSED family of the ONE <=3-row V52 algorithm.
//! Decoded records and receipts never construct a current Action Base.
use super::*;
use crate::pr_write_journal::conversation_action_lineage_v18::{
    kernel_v52::{self, CapturedV52, CompareV52, FamilyV52},
    TerminalKindV18, TerminalV18,
};
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct StateActionBaseV68 {
    current: RecordedStateOriginV68,
    target: PrWriteTarget,
    current_draft: String,
}
pub(crate) struct StateTerminalRevisionV68 {
    base: StateActionBaseV68,
    captured: CapturedV52,
}
impl StateActionBaseV68 {
    /// Only an actual live InitialRead plus its exact observed parent can
    /// issue this Base. This is not a history/known-ID constructor.
    pub(crate) fn from_open_read(
        read: &crate::scoped_pr_collaboration_source_v17::initial_v17::InitialReadV17<'_>,
        actual: &OriginalPrStateV68,
    ) -> Result<Self> {
        actual
            .validate_live_source_v68()
            .map_err(anyhow::Error::msg)?;
        actual.require().map_err(anyhow::Error::msg)?;
        if !Arc::ptr_eq(read.parent, actual.observed().parent())
            || read.identity.host != actual.identity().host
            || read.identity.account_id != actual.identity().account_id
            || read.identity.login != actual.identity().login
        {
            bail!("State Base requires the exact genuine current read and requester");
        }
        let current = record_from_actual_v68(read.original_database_v18(), actual)?;
        let target = PrWriteTarget::new(read.identity, read.parent.summary())?;
        let current_draft = current.draft_key()?;
        Ok(Self {
            current,
            target,
            current_draft,
        })
    }
    pub(crate) fn key(&self) -> &str {
        &self.current_draft
    }
    pub(crate) fn target(&self) -> &PrWriteTarget {
        &self.target
    }
    pub(in crate::pr_write_journal) fn database_v68(&self) -> &DatabaseV3 {
        &self.current.database
    }
    pub(in crate::pr_write_journal) fn observed_v68(&self) -> &RecordedStateOriginV68 {
        &self.current
    }
}
impl StateTerminalRevisionV68 {
    #[cfg(test)]
    pub(crate) fn key(&self) -> &str {
        &self.captured.key
    }
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
impl RecordedStateOriginV68 {
    pub(in crate::pr_write_journal) fn lineage_v68(&self) -> Option<&CapturedV52> {
        self.lineage_v68.as_ref()
    }
    pub(in crate::pr_write_journal) fn matches_stable_v68(&self, other: &Self) -> Result<()> {
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
            || self.command != other.command
            || self.stable_base_unchecked_v68()? != other.stable_base_unchecked_v68()?
        {
            bail!("Stable State target/command differs; mutable H/B never reset lineage");
        }
        Ok(())
    }
}
pub(in crate::pr_write_journal) fn next_key_v68(
    current: &RecordedStateOriginV68,
    terminal: &TerminalV18,
) -> Result<String> {
    Ok(format!(
        "state-v68/{}",
        sha(&(
            "ghtui-explicit-new-state-v68",
            current.stable_base_unchecked_v68()?,
            current.command,
            &terminal.reference,
        ))?
    ))
}
fn canonical_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(super) fn validate_record(current: &RecordedStateOriginV68) -> Result<()> {
    // Caller has checked the root fields. Do not recurse through validate().
    let c = current
        .lineage_v68
        .as_ref()
        .context("State lineage absent")?;
    if c.database != current.database
        || c.generation < 0
        || c.key.len() > 128
        || c.protected_key.len() > 256
        || c.key != next_key_v68(current, &c.terminal)?
        || c.terminal.reference.attempt_id <= 0
        || c.parent_ack != c.root_ack.as_ref().map(|r| r.reference.attempt_id)
        || c.parent_ack
            .is_some_and(|id| id <= 0 || id > c.terminal.reference.attempt_id)
        || !canonical_digest(&c.terminal.raw_attempt_sha256)
        || !canonical_digest(&c.terminal.raw_origin_sha256)
        || c.protected_fingerprint
            .as_ref()
            .is_some_and(|s| !canonical_digest(s))
    {
        bail!("Malformed exact State lineage/database/index/protected fingerprint");
    }
    if let Some(root) = &c.root_ack {
        if root.kind != TerminalKindV18::Acknowledged
            || root.receipt.is_some()
            || root.reference.attempt_id <= 0
            || !canonical_digest(&root.raw_attempt_sha256)
            || !canonical_digest(&root.raw_origin_sha256)
        {
            bail!("State root ACK must remain an exact acknowledged terminal");
        }
    }
    match (c.terminal.kind, &c.terminal.receipt) {
        (TerminalKindV18::Acknowledged, None) | (TerminalKindV18::Confirmed, Some(_)) => {}
        _ => bail!("State ACK and durable Confirmed are not interchangeable"),
    }
    bounded_encode(c)?;
    Ok(())
}
impl StateOriginSeedV68 {
    pub(crate) fn from_revision_v68(
        current: Self,
        base: &StateActionBaseV68,
        revision: &Arc<StateTerminalRevisionV68>,
    ) -> Result<Self> {
        current.0.matches_stable_v68(&base.current)?;
        base.current.matches_stable_v68(&revision.base.current)?;
        if current.0.lineage_v68.is_some()
            || current.0.head != base.current.head
            || current.0.base != base.current.base
            || base.current.head != revision.base.current.head
            || base.current.base != revision.base.current.base
        {
            bail!("State New requires the same freshly captured current H/B and unbound seed");
        }
        let mut record = current.0;
        record.lineage_v68 = Some(revision.captured.clone());
        record.validate()?;
        Ok(Self(record))
    }
}
/// Fixed index AND fixed target/command must name the same latest row. An
/// absent/malformed newer index never silently falls back to an older action.
pub(in crate::pr_write_journal) fn latest_state_v68(
    connection: &Connection,
    current: &RecordedStateOriginV68,
) -> Result<Option<i64>> {
    let indexed: Option<i64> = connection.query_row(
        "SELECT attempt_id FROM pr_write_state_origins_v68 WHERE base_key_v71=?1 AND action_tag_v71=?2 ORDER BY attempt_id DESC LIMIT 1",
        params![current.stable_base_unchecked_v68()?, current.command.key()], |r|r.get(0),
    ).optional()?;
    let candidate: Option<i64> = connection.query_row(
        "SELECT attempt_id FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND pr_node=?3 AND command=?4 ORDER BY attempt_id DESC LIMIT 1",
        params![current.scope.host,current.scope.login,current.pr_node,current.command.key()], |r|r.get(0),
    ).optional()?;
    if indexed != candidate || indexed.is_some_and(|id| id <= 0) {
        bail!("Latest State index differs from latest target/command; no older fallback");
    }
    Ok(indexed)
}
pub(in crate::pr_write_journal) struct StoredStateV68 {
    pub marker: PrWriteMarker,
    pub record: StateOriginRecordV68,
    attempt_hash: String,
    origin_hash: String,
}
impl StoredStateV68 {
    pub(in crate::pr_write_journal) fn load(
        connection: &Connection,
        path: &Path,
        target: &PrWriteTarget,
        id: i64,
    ) -> Result<Self> {
        let (marker, attempt_hash) = kernel_v52::stored_attempt_v52(connection, target, id)?;
        let (record, origin_hash) = read_origin_v68(connection, path, target, &marker)?;
        if record
            .seed
            .lineage_v68
            .as_ref()
            .is_some_and(|l| l.terminal.reference.attempt_id >= marker.attempt_id)
        {
            bail!("Nonmonotonic State terminal lineage");
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
pub(in crate::pr_write_journal) fn validate_begin(
    tx: &Transaction<'_>,
    path: &Path,
    target: &PrWriteTarget,
    seed: &StateOriginSeedV68,
) -> Result<()> {
    seed.0.validate()?;
    if seed.0.database != DatabaseV3::capture(path)?
        || target.scope() != &seed.0.scope
        || target.repository() != seed.0.repository
        || target.node_id() != seed.0.pr_node
        || target.number() != seed.0.number
    {
        bail!("Exact State target/file differs before atomic begin");
    }
    if let Some(lineage) = &seed.0.lineage_v68 {
        return kernel_v52::validate_begin_v52(
            tx,
            path,
            CompareV52::State(&seed.0),
            target,
            lineage,
        );
    }
    if latest_state_v68(tx, &seed.0)?.is_some() {
        bail!("Existing State action requires exact terminal descendant; never reset root");
    }
    Ok(())
}
pub(super) fn validate_owned_in_transaction_v68(
    connection: &Connection,
    path: &Path,
    seed: &StateOriginSeedV68,
    own: i64,
) -> Result<()> {
    let target = PrWriteTarget::for_source(
        &seed.0.scope,
        &seed.0.repository,
        &seed.0.pr_node,
        seed.0.number,
    )?;
    if latest_state_v68(connection, &seed.0)? != Some(own) {
        bail!("Owned State is not the exact latest target/command index");
    }
    if let Some(lineage) = &seed.0.lineage_v68 {
        kernel_v52::validate_owned_basis_v52(
            connection,
            path,
            CompareV52::State(&seed.0),
            &target,
            lineage,
            own,
        )?;
    }
    Ok(())
}
impl PrWriteJournal {
    pub(crate) fn state_action_revision_v68(
        &self,
        base: &StateActionBaseV68,
    ) -> Result<Option<Arc<StateTerminalRevisionV68>>> {
        Ok(
            kernel_v52::capture_revision_v52(self, FamilyV52::State(base))?.map(|captured| {
                Arc::new(StateTerminalRevisionV68 {
                    base: base.clone(),
                    captured,
                })
            }),
        )
    }
    pub(crate) fn validate_state_revision_v68(
        &self,
        base: &StateActionBaseV68,
        revision: &StateTerminalRevisionV68,
    ) -> Result<()> {
        base.current.matches_stable_v68(&revision.base.current)?;
        if base.current.head != revision.base.current.head
            || base.current.base != revision.base.current.base
            || self
                .state_action_revision_v68(base)?
                .as_ref()
                .is_none_or(|now| now.captured != revision.captured)
        {
            bail!("Exact current State terminal revision moved");
        }
        Ok(())
    }
    pub(crate) fn validate_owned_state_v68(
        &self,
        seed: &StateOriginSeedV68,
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
            .context("Owned State original unavailable")?;
        let own = self.state_origin_v68(&target, &marker)?;
        if bounded_encode(&own.seed)? != bounded_encode(&seed.0)? {
            bail!("Owned State original changed");
        }
        if let Some(lineage) = &seed.0.lineage_v68 {
            kernel_v52::validate_owned_basis_v52(
                &self.connection,
                &self.path,
                CompareV52::State(&seed.0),
                &target,
                lineage,
                reference.attempt_id,
            )?;
        }
        Ok(())
    }
}
