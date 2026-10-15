//! Reviewer-request STRUCTURED freshness only. Never a body/label-filter fence
//! or generic PrMetadataReceipt decode; membership is not notification proof.
use crate::{
    pr_write_journal::{
        PrEffectKind, PrPublicationReference, PrWriteJournal, PrWriteMarker, PrWriteState,
        PrWriteTarget,
    },
    providers::gh::{HostIdentity, PrResourceParentV1},
    storage::SyncBindingKind,
};
use std::path::{Path, PathBuf};
pub(crate) fn categories(
    marker: &PrWriteMarker,
    confirmed: bool,
) -> Result<Option<Vec<SyncBindingKind>>, String> {
    if marker.command != "review-rerequest-v3" {
        return Ok(None);
    }
    marker
        .selected_resource
        .strip_prefix("review-request-v3/")
        .filter(|digest| {
            digest.len() == 64
                && digest
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
        .ok_or("Re-request lacks exact canonical original selected-target digest")?;
    if confirmed {
        let actual = marker
            .confirmed
            .as_ref()
            .filter(|actual| {
                marker.state == PrWriteState::Confirmed && actual.kind == PrEffectKind::Metadata
            })
            .ok_or("Re-request needs its exact actual confirmed structured effect")?;
        let (kind, id) = actual
            .resource_id
            .strip_prefix("reviewer-request/")
            .and_then(|value| value.split_once('/'))
            .ok_or("Re-request structured receipt has no exact actual kind/numeric member")?;
        if !matches!(kind, "User" | "Bot" | "Team")
            || id.starts_with('0')
            || !id.bytes().all(|b| b.is_ascii_digit())
            || id
                .parse::<u64>()
                .ok()
                .is_none_or(|id| id == 0 || id > i64::MAX as u64)
            || actual.node_id.as_deref().is_none_or(|node| {
                node.is_empty() || node.len() > 1024 || node.chars().any(char::is_control)
            })
        {
            return Err(
                "Re-request effect lacks actual observed numeric+opaque reviewer identity".into(),
            );
        }
    } else if !matches!(
        marker.state,
        PrWriteState::Pending | PrWriteState::Uncertain
    ) || marker.confirmed.is_some()
    {
        return Err("Possible structured re-request needs unresolved original marker; no invocation inferred".into());
    }
    Ok(Some(Vec::new())) // CLOSED validated EMPTY text impact, never generic None.
}
pub struct ReviewRequestStructuredImpactV3 {
    database: PathBuf,
    target: PrWriteTarget,
    source_epoch: i64,
    reference: PrPublicationReference,
    repository_id: u64,
    pr_id: u64,
}
impl ReviewRequestStructuredImpactV3 {
    pub fn target(&self) -> &PrWriteTarget {
        &self.target
    }
    pub fn source_epoch(&self) -> i64 {
        self.source_epoch
    }
    pub fn repository_id(&self) -> u64 {
        self.repository_id
    }
    pub fn pr_id(&self) -> u64 {
        self.pr_id
    }
    pub fn notice(&self) -> &'static str {
        "Original requested-reviewer/metadata/Dashboard evidence requires fresh replacement; notification delivery unknown. Discussion bytes and label-filter evidence are not claimed changed."
    }
    pub fn validate(&self) -> Result<(), String> {
        let journal = PrWriteJournal::open(&self.database).map_err(|error| error.to_string())?;
        if journal
            .source_generation(&self.target)
            .map_err(|error| error.to_string())?
            != self.source_epoch
            || journal
                .original_publication(&self.target, &self.reference)
                .map_err(|error| error.to_string())?
                .is_none()
        {
            return Err(
                "Original structured re-request source/ref changed; no stale evidence may publish"
                    .into(),
            );
        }
        Ok(())
    }
}
/// Source/SQL/principal closure remains the caller's original boundary. This
/// validates the SAME exact journal-origin/receipt/epoch and returns retirement
/// instructions ONLY; it performs ZERO body/label/queue-membership fences.
pub fn structured_impact(
    database: &Path,
    identity: &HostIdentity,
    parent: &PrResourceParentV1,
    reference: &PrPublicationReference,
    expected_epoch: i64,
    confirmed: bool,
) -> Result<ReviewRequestStructuredImpactV3, String> {
    let target =
        PrWriteTarget::new(identity, parent.summary()).map_err(|error| error.to_string())?;
    let journal = PrWriteJournal::open(database).map_err(|error| error.to_string())?;
    let marker = journal
        .original_publication(&target, reference)
        .map_err(|error| error.to_string())?
        .ok_or("Exact original re-request publication missing")?;
    categories(&marker, confirmed)?.ok_or("Not a closed re-request purpose")?;
    let origin = journal
        .review_operation_original_v3(&target, reference)
        .map_err(|error| error.to_string())?
        .ok_or(
            "Original actual reviewer identity metadata missing; no guessed structured impact",
        )?;
    let original = origin.seed.metadata();
    let write = original.write("")?;
    if original.command() != "review-rerequest-v3"
        || original.scope() != target.scope()
        || original.repository() != target.repository()
        || original.pr_node() != target.node_id()
        || original.number() != target.number()
        || original.repository_id() != parent.repository_id()
        || original.repository_node() != parent.repository_node()
        || original.pr_id() != parent.database_id()
        || original.head() != parent.head_sha()
        || original.base() != parent.base_tip_sha()
        || write.digest().map_err(|error| error.to_string())? != marker.payload_sha256
        || journal
            .source_generation(&target)
            .map_err(|error| error.to_string())?
            != expected_epoch
    {
        return Err("Re-request effect/original target/numeric scope/source epoch mismatch".into());
    }
    if confirmed
        && !original.matches_reviewer_effect(
            marker
                .confirmed
                .as_ref()
                .ok_or("Original confirmed reviewer effect missing")?,
        )
    {
        return Err("Re-request receipt differs from actual original User/Bot/Team numeric+opaque selection".into());
    }
    let result = ReviewRequestStructuredImpactV3 {
        database: journal.database_path().into(),
        target,
        source_epoch: expected_epoch,
        reference: reference.clone(),
        repository_id: original.repository_id(),
        pr_id: original.pr_id(),
    };
    result.validate()?;
    Ok(result)
}
