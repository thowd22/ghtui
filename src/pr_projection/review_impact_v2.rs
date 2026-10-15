//! Closed pending-review impact protocol. Not an invocation/success parser and
//! never a general command allowlist: unrelated commands retain their router.
use crate::{
    pr_write_journal::{PrEffectKind, PrWriteMarker, PrWriteState},
    storage::SyncBindingKind,
};

fn canonical_id(value: &str) -> Result<u64, String> {
    let id = value
        .parse::<u64>()
        .ok()
        .filter(|id| *id > 0 && *id <= i64::MAX as u64)
        .filter(|id| id.to_string() == value)
        .ok_or("Review selection/receipt has no canonical positive actual ID")?;
    Ok(id)
}

/// None means another purpose, not acceptance. Owned malformed protocol always
/// errors, including Pending/Uncertain; no possible effect is a receipt claim.
pub(super) fn categories(
    marker: &PrWriteMarker,
    confirmed: bool,
) -> Result<Option<Vec<SyncBindingKind>>, String> {
    if let Some(categories) = super::review_operation_impact_v3::categories(marker, confirmed)? {
        return Ok(Some(categories));
    }
    let (both, create, discard) = match marker.command.as_str() {
        "review-create-pending-v2" => (true, true, false),
        "review-submit-v2" => (true, false, false),
        "review-discard-v2" => (true, false, true),
        "review-update-pending-v2" | "review-dismiss-v2" => (false, false, false),
        _ => return Ok(None),
    };
    let selected = if create {
        marker
            .selected_resource
            .strip_prefix("review-draft/")
            .filter(|digest| {
                digest.len() == 64
                    && digest
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            })
            .ok_or("Pending batch lacks its exact original body-independent draft SHA256")?;
        None
    } else {
        Some(canonical_id(
            marker
                .selected_resource
                .strip_prefix("review/")
                .ok_or("Pending review selection is not an exact review/<ID> resource")?,
        )?)
    };
    if confirmed {
        let effect = marker
            .confirmed
            .as_ref()
            .filter(|_| marker.state == PrWriteState::Confirmed)
            .ok_or("Pending-review impact needs its genuine confirmed journal marker")?;
        let expected = if discard {
            PrEffectKind::ReviewDiscarded
        } else {
            PrEffectKind::Review
        };
        let actual = canonical_id(&effect.resource_id)?;
        if effect.kind != expected
            || selected.is_some_and(|id| id != actual)
            || effect.node_id.as_deref().is_none_or(|node| {
                node.is_empty() || node.len() > 1024 || node.chars().any(char::is_control)
            })
        {
            return Err(
                "Review receipt differs from the exact selected ID/node/typed effect".into(),
            );
        }
    } else if !matches!(
        marker.state,
        PrWriteState::Pending | PrWriteState::Uncertain
    ) || marker.confirmed.is_some()
    {
        return Err("Possible pending-review impact requires an unresolved marker, not inferred confirmation".into());
    }
    Ok(Some(if both {
        vec![SyncBindingKind::Review, SyncBindingKind::InlineComment]
    } else {
        vec![SyncBindingKind::Review]
    }))
}

/// Accept only this helper's exact known closed set. A generic contains() would
/// accidentally admit future multi-category effects to this source protocol.
pub(super) fn matches_source(
    marker: &PrWriteMarker,
    confirmed: bool,
    kind: SyncBindingKind,
    actual: &[SyncBindingKind],
) -> Result<bool, String> {
    Ok(match categories(marker, confirmed)? {
        Some(expected) if expected.as_slice() == actual => match expected.as_slice() {
            [SyncBindingKind::Review, SyncBindingKind::InlineComment] => matches!(
                kind,
                SyncBindingKind::Review | SyncBindingKind::InlineComment
            ),
            [SyncBindingKind::Review] => kind == SyncBindingKind::Review,
            _ => false,
        },
        Some(_) => false,
        None => actual == [kind],
    })
}

#[cfg(test)]
#[path = "review_impact_v2_tests.rs"]
mod tests;
