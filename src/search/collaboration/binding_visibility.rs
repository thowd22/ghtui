//! Canonical metadata authority for remotely discovered parent overlays.
use super::*;
use crate::storage::ResourceBindingMetadata;

#[derive(Debug)]
pub enum BindingMetadataVisibility {
    CurrentAllowed {
        target: DiscussionTarget,
        generation: u64,
        metadata_sha256: String,
    },
    Hidden {
        /// A validated prior target, never guessed from a hash/node prefix.
        target: Option<DiscussionTarget>,
        reason: String,
    },
}
fn hidden(target: Option<DiscussionTarget>, reason: &str) -> BindingMetadataVisibility {
    BindingMetadataVisibility::Hidden {
        target,
        reason: reason.into(),
    }
}
pub fn binding_metadata_visibility(
    scope: &Scope,
    row: &ResourceBindingMetadata,
) -> Result<BindingMetadataVisibility, QueryError> {
    positive_scope(scope)?;
    let Some(payload) = row.state.payload.as_ref() else {
        return Ok(hidden(
            None,
            "Canonical state metadata is missing or exceeds its explicit byte budget",
        ));
    };
    if payload.len() > 128 * 1024 {
        return Ok(hidden(
            None,
            "Canonical state metadata exceeds the decoder byte budget",
        ));
    }
    let digest = format!("{:x}", Sha256::digest(payload.as_bytes()));
    if row.state.payload_sha256.as_deref() != Some(&digest)
        || row.state.payload_bytes != Some(payload.len() as u64)
    {
        return Ok(hidden(
            None,
            "Canonical state metadata checksum/byte proof differs",
        ));
    }
    let state: State = match serde_json::from_str(payload) {
        Ok(state) => state,
        Err(_) => {
            return Ok(hidden(
                None,
                "Canonical state metadata is malformed; no parent identity is inferred",
            ))
        }
    };
    if state.target.validate().is_err()
        || state.target.scope != *scope
        || state.target.source() != row.source
        || state.target.binding().is_err()
        || state.target.binding().ok().as_ref() != Some(&row.status.binding)
        || row.state.record_repository.as_deref() != Some(&state.target.repository)
        || row.state.record_complete != Some(state.complete)
        || row.state.fetched_at != Some(state.fetched_at)
    {
        return Ok(hidden(
            None,
            "Canonical state target/source/binding/scope proof differs",
        ));
    }
    let target = state.target;
    let Some(generation) = row.status.visible_generation else {
        return Ok(hidden(
            Some(target),
            "This exact canonical source is visibility-fenced or removed",
        ));
    };
    if !row.state.member || row.checkpoint_generation != Some(generation) || generation == 0 {
        return Ok(hidden(
            Some(target),
            "Canonical metadata membership/checkpoint visibility is unverified",
        ));
    }
    if row.status.reservation_generation == 0
        || generation > row.status.reservation_generation
        || row
            .source_generation
            .is_none_or(|epoch| epoch < row.status.reservation_generation)
        || row.source_active.is_none()
    {
        return Ok(hidden(
            Some(target),
            "Canonical source reservation epoch is unverified",
        ));
    }
    if state.access != Access::Allowed {
        return Ok(hidden(
            Some(target),
            "Canonical source access is revoked or unavailable",
        ));
    }
    if !state.complete
        || !state.enumeration_complete
        || !state.suppressed_resources.is_empty()
        || !state.suppressed_threads.is_empty()
    {
        return Ok(hidden(
            Some(target),
            "Canonical source coverage is partial or carries known suppression",
        ));
    }
    // active=false alone is not removal: a scope-wide read fence leaves unrelated
    // previously authorized source visibility/checkpoint intact.
    Ok(BindingMetadataVisibility::CurrentAllowed {
        target,
        generation,
        metadata_sha256: digest,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{CanonicalBindingStateMetadata, SyncBindingStatus};
    fn fixture() -> (Scope, ResourceBindingMetadata) {
        let scope = Scope::github("github.invalid", Some(42)).unwrap();
        let target = DiscussionTarget {
            scope: scope.clone(),
            repository_id: 9,
            repository: "example/repo".into(),
            parent_kind: ParentKind::PullRequest,
            parent_node: "GenuineOpaqueNode".into(),
            number: 7,
            category: DiscussionKind::PullRequestBody,
        };
        let state = State {
            target: target.clone(),
            access: Access::Allowed,
            enumeration_complete: true,
            complete: true,
            reasons: vec![],
            fetched_at: 123,
            head_sha: None,
            suppressed_resources: BTreeSet::new(),
            suppressed_threads: BTreeSet::new(),
        };
        let payload = serde_json::to_string(&state).unwrap();
        (
            scope,
            ResourceBindingMetadata {
                source: target.source(),
                status: SyncBindingStatus {
                    binding: target.binding().unwrap(),
                    reservation_generation: 2,
                    visible_generation: Some(2),
                    active: false,
                },
                checkpoint_generation: Some(2),
                source_generation: Some(3),
                source_active: Some(false),
                state: CanonicalBindingStateMetadata {
                    payload_sha256: Some(format!("{:x}", Sha256::digest(payload.as_bytes()))),
                    payload_bytes: Some(payload.len() as u64),
                    payload: Some(payload),
                    record_repository: Some(target.repository),
                    record_complete: Some(true),
                    fetched_at: Some(123),
                    member: true,
                },
            },
        )
    }
    fn change(row: &mut ResourceBindingMetadata, update: impl FnOnce(&mut Value)) {
        let mut value: Value = serde_json::from_str(row.state.payload.as_ref().unwrap()).unwrap();
        update(&mut value);
        let payload = serde_json::to_string(&value).unwrap();
        row.state.payload_bytes = Some(payload.len() as u64);
        row.state.payload_sha256 = Some(format!("{:x}", Sha256::digest(payload.as_bytes())));
        row.state.payload = Some(payload);
    }
    #[test]
    fn collaboration_index_binding_metadata_authority_keeps_unrelated_inactive_visibility() {
        let (scope, row) = fixture();
        let BindingMetadataVisibility::CurrentAllowed {
            target,
            generation,
            metadata_sha256,
        } = binding_metadata_visibility(&scope, &row).unwrap()
        else {
            panic!("active=false alone cannot revoke explicit source visibility");
        };
        assert_eq!(target.number, 7);
        assert_eq!(target.parent_kind, ParentKind::PullRequest);
        assert_eq!(target.parent_node, "GenuineOpaqueNode");
        assert_eq!(generation, 2);
        assert_eq!(Some(metadata_sha256), row.state.payload_sha256);
        let mut removed = row;
        removed.status.visible_generation = None;
        assert!(
            matches!(binding_metadata_visibility(&scope,&removed).unwrap(),BindingMetadataVisibility::Hidden {target:Some(target),..} if target.number == 7)
        );
    }
    #[test]
    fn collaboration_index_binding_metadata_authority_rejects_scope_source_target_and_record_mismatch(
    ) {
        let (scope, row) = fixture();
        let other = Scope::github("github.invalid", Some(43)).unwrap();
        assert!(matches!(
            binding_metadata_visibility(&other, &row).unwrap(),
            BindingMetadataVisibility::Hidden { target: None, .. }
        ));
        for mode in ["source", "binding", "record", "parent", "zero"] {
            let mut changed = row.clone();
            match mode {
                "source" => changed.source = format!("collaboration-v1:{}", "0".repeat(64)),
                "binding" => changed.status.binding.repository_id = 10,
                "record" => changed.state.record_repository = Some("example/other".into()),
                "parent" => change(&mut changed, |value| {
                    value["target"]["parent_kind"] = serde_json::json!("Issue")
                }),
                "zero" => change(&mut changed, |value| {
                    value["target"]["number"] = serde_json::json!(0)
                }),
                _ => unreachable!(),
            }
            assert!(
                matches!(
                    binding_metadata_visibility(&scope, &changed).unwrap(),
                    BindingMetadataVisibility::Hidden { target: None, .. }
                ),
                "{mode}"
            );
        }
    }
    #[test]
    fn collaboration_index_binding_metadata_authority_never_admits_unavailable_partial_suppressed_or_unverified_generations(
    ) {
        let (scope, row) = fixture();
        for mode in [
            "unavailable",
            "revoked",
            "partial",
            "resource",
            "thread",
            "membership",
            "checkpoint",
            "epoch",
            "future_visible",
        ] {
            let mut changed = row.clone();
            match mode {
                "unavailable" => change(&mut changed, |value| {
                    value["access"] = serde_json::json!({"Unavailable":"budget"})
                }),
                "revoked" => change(&mut changed, |value| {
                    value["access"] = serde_json::json!({"Revoked":"access"})
                }),
                "partial" => change(&mut changed, |value| {
                    value["enumeration_complete"] = serde_json::json!(false)
                }),
                "resource" => change(&mut changed, |value| {
                    value["suppressed_resources"] = serde_json::json!(["deleted-node"])
                }),
                "thread" => change(&mut changed, |value| {
                    value["suppressed_threads"] = serde_json::json!(["deleted-thread"])
                }),
                "membership" => changed.state.member = false,
                "checkpoint" => changed.checkpoint_generation = Some(1),
                "epoch" => changed.source_generation = None,
                "future_visible" => {
                    changed.status.visible_generation = Some(3);
                    changed.checkpoint_generation = Some(3);
                }
                _ => unreachable!(),
            }
            assert!(
                matches!(
                    binding_metadata_visibility(&scope, &changed).unwrap(),
                    BindingMetadataVisibility::Hidden {
                        target: Some(_),
                        ..
                    }
                ),
                "{mode}"
            );
        }
    }
    #[test]
    fn collaboration_index_binding_metadata_authority_preflights_missing_oversized_malformed_or_bad_checksum(
    ) {
        let (scope, row) = fixture();
        for mode in ["missing", "oversized", "malformed", "checksum"] {
            let mut changed = row.clone();
            match mode {
                "missing" => changed.state.payload = None,
                "oversized" => changed.state.payload = Some("x".repeat(131_073)),
                "malformed" => {
                    let payload = "not canonical JSON".to_string();
                    changed.state.payload_bytes = Some(payload.len() as u64);
                    changed.state.payload_sha256 =
                        Some(format!("{:x}", Sha256::digest(payload.as_bytes())));
                    changed.state.payload = Some(payload);
                }
                "checksum" => changed.state.payload_sha256 = Some("0".repeat(64)),
                _ => unreachable!(),
            }
            assert!(
                matches!(
                    binding_metadata_visibility(&scope, &changed).unwrap(),
                    BindingMetadataVisibility::Hidden { target: None, .. }
                ),
                "{mode}"
            );
        }
    }
}
