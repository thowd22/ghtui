//! Closed pending-comment effects retire BOTH review membership and inline
//! text as ONE original atomic set. No unknown-purpose category inference.
use crate::{
    pr_write_journal::{PrEffectKind, PrWriteJournal, PrWriteMarker, PrWriteState, PrWriteTarget},
    storage::SyncBindingKind,
};
pub(crate) fn categories(
    marker: &PrWriteMarker,
    confirmed: bool,
) -> Result<Option<Vec<SyncBindingKind>>, String> {
    let effect = match marker.command.as_str() {
        "review-pending-comment-add-v3" | "review-pending-comment-edit-v3" => PrEffectKind::Comment,
        "review-pending-comment-delete-v3" => PrEffectKind::CommentDeleted,
        _ => return Ok(None),
    };
    marker
        .selected_resource
        .strip_prefix("pending-comment-v3/")
        .filter(|digest| {
            digest.len() == 64
                && digest
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
        .ok_or("Pending-comment purpose lacks canonical exact original observation digest")?;
    if confirmed {
        let actual = marker
            .confirmed
            .as_ref()
            .filter(|actual| marker.state == PrWriteState::Confirmed && actual.kind == effect)
            .ok_or("Pending-comment exact state/typed effect mismatch")?;
        if actual.resource_id.starts_with('0')
            || !actual.resource_id.bytes().all(|b| b.is_ascii_digit())
            || actual
                .resource_id
                .parse::<u64>()
                .ok()
                .is_none_or(|id| id == 0 || id > i64::MAX as u64)
            || actual.node_id.as_deref().is_none_or(|node| {
                node.is_empty() || node.len() > 1024 || node.chars().any(char::is_control)
            })
        {
            return Err("Actual pending-comment receipt requires positive full-width numeric/opaque identity".into());
        }
    } else if !matches!(
        marker.state,
        PrWriteState::Pending | PrWriteState::Uncertain
    ) || marker.confirmed.is_some()
    {
        return Err("Possible pending-comment effect requires exact unresolved marker, not inferred invocation".into());
    }
    Ok(Some(vec![
        SyncBindingKind::Review,
        SyncBindingKind::InlineComment,
    ]))
}
/// Only this NEW purpose needs its own actual atomic origin. Existing closed
/// Review/Thread/Comment validators keep their unchanged protocols.
pub(crate) fn validate_original(
    marker: &PrWriteMarker,
    journal: &PrWriteJournal,
    target: &PrWriteTarget,
) -> Result<(), String> {
    super::issue_link_impact_v87::validate_original(marker, journal, target)?;
    super::metadata_text_impact_v25::validate_original(marker, journal, target)?;
    super::metadata_members_impact_v48::validate_original(marker, journal, target)?;
    super::metadata_base_impact_v32::validate_original(marker, journal, target)?;
    let confirmed = marker.state == PrWriteState::Confirmed;
    if categories(marker, confirmed)?.is_none() {
        return Ok(());
    }
    let origin = journal
        .review_observed_origin_v12(target, &marker.publication_reference())
        .map_err(|error| error.to_string())?
        .ok_or("Known pending-comment purpose lacks its actual atomic original observation")?;
    let original = origin
        .metadata()
        .ok_or("Known V3 pending-comment purpose lacks its closed original metadata")?;
    if original.command() != marker.command
        || original.scope() != target.scope()
        || original.repository() != target.repository()
        || original.pr_node() != target.node_id()
        || original.number() != target.number()
    {
        return Err(
            "Original pending-comment impact differs from exact account/repository/PR/purpose"
                .into(),
        );
    }
    if confirmed
        && !original.matches_pending_effect(
            marker
                .confirmed
                .as_ref()
                .ok_or("Actual pending-comment receipt missing")?,
        )
    {
        return Err(
            "Original actual selected pending comment differs from its typed receipt".into(),
        );
    }
    Ok(())
}
