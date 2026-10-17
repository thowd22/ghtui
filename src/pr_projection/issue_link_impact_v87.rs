//! Manual development links mutate structured relationships, not discussion
//! text. Empty impact requires the exact native original, never a legacy tag.
use crate::{
    pr_write_journal::{PrEffectKind, PrWriteJournal, PrWriteMarker, PrWriteState, PrWriteTarget},
    storage::SyncBindingKind,
};
pub(crate) fn categories(
    marker: &PrWriteMarker,
    confirmed: bool,
) -> Result<Option<Vec<SyncBindingKind>>, String> {
    let known = matches!(
        marker.command.as_str(),
        "issue-link-v87" | "issue-unlink-manual-v87"
    );
    let key = marker
        .selected_resource
        .strip_prefix("issue-link-draft-v87/");
    if !known && key.is_none() {
        return Ok(None);
    }
    if !known
        || key.is_none_or(|key| {
            key.len() != 64
                || !key
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
    {
        return Err("Malformed known manual Issue purpose; no legacy impact fallback".into());
    }
    if confirmed {
        let effect = marker
            .confirmed
            .as_ref()
            .filter(|effect| {
                marker.state == PrWriteState::Confirmed && effect.kind == PrEffectKind::Metadata
            })
            .ok_or("Manual Issue exact Confirmed receipt unavailable")?;
        if effect
            .resource_id
            .parse::<u64>()
            .ok()
            .is_none_or(|n| n == 0 || n > i64::MAX as u64 || n.to_string() != effect.resource_id)
            || effect.node_id.as_deref().is_none_or(|node| {
                node.is_empty() || node.len() > 1024 || node.chars().any(char::is_control)
            })
        {
            return Err(
                "Manual Issue receipt requires exact selected numeric and node identity".into(),
            );
        }
    } else if !matches!(
        marker.state,
        PrWriteState::Pending | PrWriteState::Uncertain
    ) || marker.confirmed.is_some()
    {
        return Err("Manual Issue unresolved impact requires exact unresolved marker".into());
    }
    Ok(Some(Vec::new()))
}
pub(crate) fn validate_original(
    marker: &PrWriteMarker,
    journal: &PrWriteJournal,
    target: &PrWriteTarget,
) -> Result<(), String> {
    if categories(marker, marker.state == PrWriteState::Confirmed)?.is_none() {
        return Ok(());
    }
    journal
        .issue_link_origin_v87(target, marker)
        .map_err(|e| e.to_string())?;
    Ok(())
}
