//! Closed six-purpose impact. Reactions/subscriptions are genuinely EMPTY,
//! never an unknown-command bypass or an invented discussion-body mutation.
use crate::{
    pr_write_journal::{PrEffectKind, PrWriteMarker, PrWriteState},
    storage::SyncBindingKind,
};
pub(crate) fn categories(
    marker: &PrWriteMarker,
    confirmed: bool,
) -> Result<Option<Vec<SyncBindingKind>>, String> {
    let (kind, text) = match marker.command.as_str() {
        "conversation-create-v17" | "conversation-edit-v17" => (PrEffectKind::Comment, true),
        "conversation-delete-v17" => (PrEffectKind::CommentDeleted, true),
        "conversation-reaction-add-v17" => (PrEffectKind::Reaction, false),
        "conversation-reaction-remove-v17" => (PrEffectKind::ReactionRemoved, false),
        "conversation-subscription-v17" => (PrEffectKind::Subscription, false),
        command if command.starts_with("conversation-") => {
            return Err("Unknown Conversation purpose has no impact/empty bypass".into())
        }
        _ => return Ok(None),
    };
    if !marker
        .selected_resource
        .strip_prefix("conversation-v17/")
        .is_some_and(|d| {
            d.len() == 64
                && d.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
    {
        return Err(
            "Known Conversation purpose lacks its exact lowercase original target digest".into(),
        );
    }
    if confirmed {
        let receipt = marker
            .confirmed
            .as_ref()
            .filter(|r| marker.state == PrWriteState::Confirmed && r.kind == kind)
            .ok_or("Exact confirmed Conversation purpose/effect disagrees")?;
        if kind == PrEffectKind::Subscription {
            if !["subscribed", "unsubscribed", "ignored"].contains(&receipt.resource_id.as_str())
                || receipt.node_id.is_some()
            {
                return Err("Subscription receipt is not one explicit observed state".into());
            }
        } else if receipt.resource_id.starts_with('0')
            || receipt
                .resource_id
                .parse::<u64>()
                .ok()
                .is_none_or(|id| !(1..=i64::MAX as u64).contains(&id))
            || !receipt.resource_id.bytes().all(|b| b.is_ascii_digit())
            || receipt
                .node_id
                .as_ref()
                .is_none_or(|n| n.is_empty() || n.len() > 1024 || n.chars().any(char::is_control))
        {
            return Err(
                "Actual Conversation/reaction numeric and opaque receipt unavailable".into(),
            );
        }
    } else if !matches!(
        marker.state,
        PrWriteState::Pending | PrWriteState::Uncertain
    ) || marker.confirmed.is_some()
    {
        return Err("Possible Conversation impact needs exact Pending/Uncertain state".into());
    }
    Ok(Some(if text {
        vec![SyncBindingKind::ConversationComment]
    } else {
        vec![]
    }))
}
