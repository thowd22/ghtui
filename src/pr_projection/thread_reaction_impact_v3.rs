//! Reactions have no persisted discussion-text/structured-state projection in
//! this product. Empty impact is CLOSED and validated, not an unknown bypass.
//! The invoking controller still retires its original private presentation.
use crate::{
    pr_write_journal::{PrEffectKind, PrWriteMarker, PrWriteState},
    storage::SyncBindingKind,
};
pub(crate) fn categories(
    marker: &PrWriteMarker,
    confirmed: bool,
) -> Result<Option<Vec<SyncBindingKind>>, String> {
    let effect = match marker.command.as_str() {
        "thread-reaction-add-v3" => PrEffectKind::Reaction,
        "thread-reaction-remove-v3" => PrEffectKind::ReactionRemoved,
        command if command.starts_with("thread-reaction-") => {
            return Err(
                "Unknown versioned thread reaction purpose has no empty-impact bypass".into(),
            )
        }
        _ => return Ok(None),
    };
    let digest = marker
        .selected_resource
        .strip_prefix("thread-reaction-v3/")
        .filter(|value| {
            value.len() == 64
                && value
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
        .ok_or("Known reaction purpose lacks exact original lowercase target digest")?;
    debug_assert_eq!(digest.len(), 64);
    if confirmed {
        let receipt = marker
            .confirmed
            .as_ref()
            .filter(|receipt| marker.state == PrWriteState::Confirmed && receipt.kind == effect)
            .ok_or("Reaction receipt does not match exact confirmed purpose/state")?;
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
            return Err("Actual reaction numeric/opaque-node receipt unavailable".into());
        }
    } else if !matches!(
        marker.state,
        PrWriteState::Pending | PrWriteState::Uncertain
    ) || marker.confirmed.is_some()
    {
        return Err("Possible reaction impact requires exact Pending/Uncertain observation".into());
    }
    Ok(Some(Vec::new()))
}
