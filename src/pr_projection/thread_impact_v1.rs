//! Closed thread-publication selector. Inline binding carries both comment
//! content and persisted context.resolved; retiring it for a state write does
//! NOT claim that any comment body changed. No unknown-command passthrough.
use crate::{
    pr_write_journal::{PrEffectKind, PrWriteMarker, PrWriteState},
    storage::SyncBindingKind,
};

fn digest(selected: &str, prefix: &str) -> bool {
    selected.strip_prefix(prefix).is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}
fn node(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        !value.is_empty() && value.len() <= 512 && !value.chars().any(char::is_control)
    })
}
fn positive(value: &str) -> bool {
    !value.starts_with('0')
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && value
            .parse::<u64>()
            .ok()
            .is_some_and(|id| id > 0 && id <= i64::MAX as u64)
}
/// None means this is a DIFFERENT purpose and the existing closed router must
/// still decide/refuse it. Known thread commands never return None on error.
pub(crate) fn categories(
    marker: &PrWriteMarker,
    confirmed: bool,
) -> Result<Option<Vec<SyncBindingKind>>, String> {
    let expected = match marker.command.as_str() {
        "thread-reply" => {
            if !digest(&marker.selected_resource, "thread-reply/") {
                return Err("Thread reply lacks its exact original target SHA256".into());
            }
            PrEffectKind::Comment
        }
        "thread-resolve" | "thread-reopen" => {
            if !digest(&marker.selected_resource, "thread-state/") {
                return Err("Thread state write lacks its exact original target SHA256".into());
            }
            PrEffectKind::Thread
        }
        _ => return Ok(None),
    };
    if confirmed {
        let effect = marker.confirmed.as_ref().filter(|effect| marker.state == PrWriteState::Confirmed && effect.kind == expected && node(effect.node_id.as_deref())).ok_or("Thread confirmed selector requires its exact typed confirmed effect and actual opaque node")?;
        let matches = match marker.command.as_str() {
            "thread-reply" => positive(&effect.resource_id),
            "thread-resolve" => effect.resource_id == "resolved",
            "thread-reopen" => effect.resource_id == "unresolved",
            _ => false,
        };
        if !matches {
            return Err("Thread effect does not match this exact command".into());
        }
    } else if !matches!(
        marker.state,
        PrWriteState::Pending | PrWriteState::Uncertain
    ) || marker.confirmed.is_some()
    {
        return Err("Possible thread freshness retirement requires the exact pending/uncertain marker, not fabricated confirmation".into());
    }
    Ok(Some(vec![SyncBindingKind::InlineComment]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        pr_write_journal::{PrConfirmedEffect, PrWriteTarget},
        storage::Scope,
    };
    fn marker(command: &str) -> PrWriteMarker {
        PrWriteMarker {
            attempt_id: 1,
            target: PrWriteTarget::for_source(
                &Scope::github("github.invalid", Some(42)).unwrap(),
                "example/one",
                "PR_actual",
                7,
            )
            .unwrap(),
            command: command.into(),
            selected_resource: format!(
                "{}/{}",
                if command == "thread-reply" {
                    "thread-reply"
                } else {
                    "thread-state"
                },
                "a".repeat(64)
            ),
            observed_head: "b".repeat(40),
            payload_sha256: "c".repeat(64),
            state: PrWriteState::Pending,
            confirmed: None,
            created_at: 1,
        }
    }
    #[test]
    fn review_threads_v1_possible_reply_and_structured_state_require_exact_pending_shapes() {
        for command in ["thread-reply", "thread-resolve", "thread-reopen"] {
            let mut marker = marker(command);
            assert_eq!(
                categories(&marker, false).unwrap(),
                Some(vec![SyncBindingKind::InlineComment])
            );
            marker.state = PrWriteState::Uncertain;
            assert_eq!(
                categories(&marker, false).unwrap(),
                Some(vec![SyncBindingKind::InlineComment])
            );
            assert!(categories(&marker, true).is_err());
            marker.selected_resource.push('0');
            assert!(categories(&marker, false).is_err());
        }
    }
    #[test]
    fn review_threads_v1_confirmed_exact_effects_retire_only_original_inline_binding() {
        for (command, kind, resource) in [
            ("thread-reply", PrEffectKind::Comment, "9009"),
            ("thread-resolve", PrEffectKind::Thread, "resolved"),
            ("thread-reopen", PrEffectKind::Thread, "unresolved"),
        ] {
            let mut marker = marker(command);
            marker.state = PrWriteState::Confirmed;
            marker.confirmed = Some(PrConfirmedEffect {
                kind,
                resource_id: resource.into(),
                node_id: Some("ACTUAL_opaque_9".into()),
            });
            assert_eq!(
                categories(&marker, true).unwrap(),
                Some(vec![SyncBindingKind::InlineComment])
            );
            assert!(categories(&marker, false).is_err());
            marker.confirmed.as_mut().unwrap().kind = PrEffectKind::Review;
            assert!(categories(&marker, true).is_err());
        }
    }
    #[test]
    fn review_threads_v1_malformed_known_thread_selector_never_becomes_unknown_no_effect() {
        for selected in [
            "thread-state/",
            "thread-state/ABC",
            "thread-reply/../old",
            "thread-state/000",
            "thread-state/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaA",
        ] {
            let mut marker = marker("thread-resolve");
            marker.selected_resource = selected.into();
            assert!(categories(&marker, false).is_err());
        }
        let mut marker = marker("other-purpose");
        assert_eq!(categories(&marker, false).unwrap(), None);
        marker.command = "thread-reply".into();
        assert!(categories(&marker, false).is_err());
    }
    #[test]
    fn review_threads_v1_state_receipt_cannot_alias_opposite_transition_or_deleted_node() {
        let mut marker = marker("thread-resolve");
        marker.state = PrWriteState::Confirmed;
        marker.confirmed = Some(PrConfirmedEffect {
            kind: PrEffectKind::Thread,
            resource_id: "unresolved".into(),
            node_id: Some("NODE".into()),
        });
        assert!(categories(&marker, true).is_err());
        marker.confirmed.as_mut().unwrap().resource_id = "resolved".into();
        marker.confirmed.as_mut().unwrap().node_id = None;
        assert!(categories(&marker, true).is_err());
    }
}
