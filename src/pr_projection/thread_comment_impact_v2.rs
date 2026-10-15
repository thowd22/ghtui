//! CLOSED selected submitted-comment freshness. This changes Inline bodies,
//! not conversation/review text; no unknown-purpose or malformed-known bypass.
use crate::{
    pr_write_journal::{PrEffectKind, PrWriteMarker, PrWriteState},
    storage::SyncBindingKind,
};
pub(crate) fn categories(
    marker: &PrWriteMarker,
    confirmed: bool,
) -> Result<Option<Vec<SyncBindingKind>>, String> {
    let expected = match marker.command.as_str() {
        "thread-comment-edit-v2" => PrEffectKind::Comment,
        "thread-comment-delete-v2" => PrEffectKind::CommentDeleted,
        _ => return Ok(None),
    };
    let digest = marker
        .selected_resource
        .strip_prefix("thread-comment-v2/")
        .filter(|digest| {
            digest.len() == 64
                && digest
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
        .ok_or("Known selected-comment purpose lacks its exact original lowercase target digest")?;
    debug_assert_eq!(digest.len(), 64);
    if confirmed {
        let receipt = marker
            .confirmed
            .as_ref()
            .filter(|receipt| marker.state == PrWriteState::Confirmed && receipt.kind == expected)
            .ok_or("Selected-comment effect must match exact confirmed state/purpose")?;
        if receipt.resource_id.starts_with('0')
            || !receipt.resource_id.bytes().all(|b| b.is_ascii_digit())
            || receipt
                .resource_id
                .parse::<u64>()
                .ok()
                .is_none_or(|id| id == 0 || id > i64::MAX as u64)
            || receipt.node_id.as_deref().is_none_or(|node| {
                node.is_empty() || node.len() > 512 || node.chars().any(char::is_control)
            })
        {
            return Err("Exact actual selected-comment numeric/node receipt unavailable".into());
        }
    } else if !matches!(
        marker.state,
        PrWriteState::Pending | PrWriteState::Uncertain
    ) || marker.confirmed.is_some()
    {
        return Err("Possible comment body freshness needs exact Pending/Uncertain, not invented confirmation".into());
    }
    Ok(Some(vec![SyncBindingKind::InlineComment]))
}
