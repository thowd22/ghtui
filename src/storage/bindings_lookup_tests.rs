use super::*;
use crate::search::{
    collaboration::{DiscussionTarget, ParentKind},
    model::DiscussionKind,
};
use serde_json::{json, Value};

fn canonical(kind: DiscussionKind, node: &str, number: u64) -> DiscussionTarget {
    DiscussionTarget {
        scope: scope(),
        repository_id: 9,
        repository: "example/repo".into(),
        parent_kind: if kind == DiscussionKind::IssueBody {
            ParentKind::Issue
        } else {
            ParentKind::PullRequest
        },
        parent_node: node.into(),
        number,
        category: kind,
    }
}
fn state(target: &DiscussionTarget, access: Value, complete: bool) -> Record {
    let mut record = Record::new(
        "collaboration-state-v1",
        target.source(),
        &target.repository,
        json!({"target":target,"access":access,"enumeration_complete":complete,"complete":complete,"reasons":[],"fetched_at":123,"head_sha":null,"suppressed_resources":[],"suppressed_threads":[]}),
    );
    record.complete = complete;
    record.fetched_at = 123;
    record
}
fn publish_state(
    cache: &mut Cache,
    target: &DiscussionTarget,
    access: Value,
    complete: bool,
) -> u64 {
    let generation = cache
        .reserve_resource_sync(&scope(), &target.source(), &target.binding().unwrap())
        .unwrap();
    let metadata = state(target, access, complete);
    let body = Record::new(
        "collaboration-document-v1",
        format!("document:{}", target.source()),
        &target.repository,
        json!({"body":"BODY_CANARY_NOT_VISIBILITY_METADATA"}),
    );
    assert!(cache
        .commit_reserved_batch(
            &scope(),
            ReservedSyncBatch {
                source: &target.source(),
                generation,
                records: &[metadata, body],
                complete,
                deletion_authoritative: true
            }
        )
        .unwrap());
    generation
}
fn selected_fence(target: &DiscussionTarget) -> SyncFence {
    let binding = target.binding().unwrap();
    SyncFence {
        namespace: binding.namespace,
        repository_id: Some(binding.repository_id),
        repository: binding.repository,
        resource: binding.resource,
        kinds: vec![binding.kind],
    }
}

#[test]
fn collaboration_index_binding_lookup_removed_visibility_is_not_unrelated_inactive_flag() {
    let (_dir, mut cache) = fixture();
    let pr = canonical(DiscussionKind::PullRequestBody, "PR_node_7", 7);
    let issue = canonical(DiscussionKind::IssueBody, "ISSUE_node_8", 8);
    publish_state(&mut cache, &pr, json!("Allowed"), true);
    let issue_generation = publish_state(&mut cache, &issue, json!("Allowed"), true);
    cache
        .fence_resource_sync(&scope(), &selected_fence(&pr))
        .unwrap();
    // Deliberately malformed document JSON would fail any full-frame reader.
    cache.connection.execute("UPDATE records SET payload='not_document_JSON_BODY_CANARY' WHERE kind='collaboration-document-v1'",[]).unwrap();
    let snapshot = cache
        .lookup_resource_bindings(
            &scope(),
            "EXAMPLE/REPO",
            None,
            &[SyncBindingKind::PullRequestBody, SyncBindingKind::IssueBody],
            10,
        )
        .unwrap();
    assert!(snapshot.complete());
    assert_eq!(snapshot.total_bindings(), 2);
    assert_eq!(snapshot.visible_none_count(), 1);
    let removed = snapshot
        .rows()
        .iter()
        .find(|row| row.source == pr.source())
        .unwrap();
    assert_eq!(removed.status.visible_generation, None);
    assert!(!removed.status.active);
    let unrelated = snapshot
        .rows()
        .iter()
        .find(|row| row.source == issue.source())
        .unwrap();
    assert!(
        !unrelated.status.active,
        "scope-wide read fence is not removal"
    );
    assert_eq!(unrelated.status.visible_generation, Some(issue_generation));
    assert_eq!(unrelated.checkpoint_generation, Some(issue_generation));
    assert!(unrelated.state.member);
    for row in snapshot.rows() {
        let payload = row.state.payload.as_ref().unwrap();
        assert!(!payload.contains("BODY_CANARY"));
        let metadata: Value = serde_json::from_str(payload).unwrap();
        assert_eq!(
            metadata["target"]["parent_node"],
            row.status.binding.resource
        );
        assert!(row.state.payload_sha256.is_some());
    }
    snapshot.validate(&cache).unwrap();
    let other = cache
        .lookup_resource_bindings(
            &Scope::github("github.invalid", Some(43)).unwrap(),
            "example/repo",
            None,
            &[SyncBindingKind::PullRequestBody],
            10,
        )
        .unwrap();
    assert!(other.complete());
    assert_eq!(other.total_bindings(), 0);
}

#[test]
fn collaboration_index_binding_lookup_partial_page_keeps_hidden_count_and_keyspace_proof() {
    let (_dir, mut cache) = fixture();
    let a = canonical(DiscussionKind::PullRequestBody, "PR_A", 1);
    let b = canonical(DiscussionKind::PullRequestBody, "PR_B", 2);
    publish_state(&mut cache, &a, json!("Allowed"), true);
    publish_state(&mut cache, &b, json!("Allowed"), true);
    let (visible, hidden) = if a.source() < b.source() {
        (&a, &b)
    } else {
        (&b, &a)
    };
    cache
        .fence_resource_sync(&scope(), &selected_fence(hidden))
        .unwrap();
    let snapshot = cache
        .lookup_resource_bindings(
            &scope(),
            "example/repo",
            None,
            &[SyncBindingKind::PullRequestBody],
            1,
        )
        .unwrap();
    assert!(!snapshot.complete());
    assert_eq!(snapshot.rows().len(), 1);
    assert_eq!(snapshot.rows()[0].source, visible.source());
    assert!(snapshot.rows()[0].status.visible_generation.is_some());
    assert_eq!(
        snapshot.visible_none_count(),
        1,
        "the removed source lies outside the page"
    );
    snapshot.validate(&cache).unwrap();
    let changed = state(
        hidden,
        json!({"Unavailable":"metadata changed outside returned page"}),
        false,
    );
    cache.connection.execute("UPDATE records SET payload=?1 WHERE host=?2 AND login=?3 AND kind='collaboration-state-v1' AND id=?4",params![serde_json::to_string(&changed.payload).unwrap(),scope().host,scope().login,hidden.source()]).unwrap();
    assert!(
        snapshot.validate(&cache).is_err(),
        "repo metadata epoch catches outside-page state changes with stable counts"
    );
    let fresh = cache
        .lookup_resource_bindings(
            &scope(),
            "example/repo",
            None,
            &[SyncBindingKind::PullRequestBody],
            1,
        )
        .unwrap();
    let c = canonical(DiscussionKind::PullRequestBody, "PR_NEW", 3);
    publish_state(&mut cache, &c, json!("Allowed"), true);
    assert!(
        fresh.validate(&cache).is_err(),
        "new matching keys cannot publish under the old page proof"
    );
}

#[test]
fn collaboration_index_binding_lookup_first_source_tombstone_is_retained_beside_fresh_authority() {
    let (_dir, mut cache) = fixture();
    let target = canonical(
        DiscussionKind::PullRequestBody,
        "OPAQUE_parent_without_binding",
        7,
    );
    let result = cache
        .fence_resource_sync_once(&scope(), &selected_fence(&target), 17)
        .unwrap();
    assert!(result.sources.is_empty());
    let nodes = vec![target.parent_node.clone()];
    let snapshot = cache
        .lookup_resource_bindings(
            &scope(),
            "example/repo",
            Some(&nodes),
            &[SyncBindingKind::PullRequestBody],
            2,
        )
        .unwrap();
    assert!(snapshot.complete());
    assert_eq!(snapshot.total_bindings(), 0);
    assert_eq!(snapshot.total_tombstones(), 1);
    assert_eq!(snapshot.tombstones()[0].mutation_ordinal, 17);
    assert_eq!(snapshot.tombstones()[0].resource, target.parent_node);
    let issue = cache
        .lookup_resource_bindings(
            &scope(),
            "example/repo",
            Some(&nodes),
            &[SyncBindingKind::IssueBody],
            2,
        )
        .unwrap();
    assert_eq!(
        issue.total_tombstones(),
        0,
        "PR body does not become an issue body by number"
    );
    let fresh_generation = publish_state(&mut cache, &target, json!("Allowed"), true);
    assert!(snapshot.validate(&cache).is_err());
    let fresh = cache
        .lookup_resource_bindings(
            &scope(),
            "example/repo",
            Some(&nodes),
            &[SyncBindingKind::PullRequestBody],
            2,
        )
        .unwrap();
    assert!(fresh.complete());
    assert_eq!(
        fresh.total_tombstones(),
        1,
        "keep receipt replay protection, not permanent denial"
    );
    assert_eq!(
        fresh.rows()[0].status.visible_generation,
        Some(fresh_generation)
    );
    assert_eq!(
        fresh.rows()[0].checkpoint_generation,
        Some(fresh_generation)
    );
    assert_eq!(
        serde_json::from_str::<Value>(fresh.rows()[0].state.payload.as_ref().unwrap()).unwrap()
            ["access"],
        "Allowed"
    );
    let capped = cache
        .lookup_resource_bindings(
            &scope(),
            "example/repo",
            Some(&nodes),
            &[SyncBindingKind::PullRequestBody],
            1,
        )
        .unwrap();
    assert!(!capped.complete());
    assert_eq!(capped.rows().len(), 1);
    assert!(capped.tombstones().is_empty());
    assert_eq!(capped.total_tombstones(), 1);
    capped.validate(&cache).unwrap();
    cache
        .connection
        .execute(
            "UPDATE sync_mutation_fences SET mutation_ordinal=18 WHERE host=?1 AND login=?2 AND repository=?3 AND resource=?4",
            params![scope().host, scope().login, target.repository, target.parent_node],
        )
        .unwrap();
    assert!(
        capped.validate(&cache).is_err(),
        "tombstone changes outside the shared row cap invalidate the keyspace proof"
    );
    let changed = cache
        .lookup_resource_bindings(
            &scope(),
            "example/repo",
            Some(&nodes),
            &[SyncBindingKind::PullRequestBody],
            1,
        )
        .unwrap();
    assert_eq!(capped.rows(), changed.rows());
    assert_eq!(capped.total_tombstones(), changed.total_tombstones());
}

#[test]
fn collaboration_index_binding_lookup_state_preflight_handles_unavailable_oversize_and_missing_without_bodies(
) {
    let (_dir, mut cache) = fixture();
    let target = canonical(DiscussionKind::PullRequestBody, "PR_private", 7);
    publish_state(&mut cache, &target, json!("Allowed"), true);
    let replacement = cache
        .reserve_resource_sync(&scope(), &target.source(), &target.binding().unwrap())
        .unwrap();
    let metadata = state(
        &target,
        json!({"Unavailable":"selected metadata budget"}),
        false,
    );
    assert!(cache
        .commit_reserved_batch(
            &scope(),
            ReservedSyncBatch {
                source: &target.source(),
                generation: replacement,
                records: std::slice::from_ref(&metadata),
                complete: false,
                deletion_authoritative: false
            }
        )
        .unwrap());
    cache.connection.execute("UPDATE records SET payload='broken_BODY_CANARY' WHERE kind='collaboration-document-v1'",[]).unwrap();
    let unavailable = cache
        .lookup_resource_bindings(
            &scope(),
            "example/repo",
            None,
            &[SyncBindingKind::PullRequestBody],
            1,
        )
        .unwrap();
    let row = &unavailable.rows()[0];
    assert_eq!(row.status.visible_generation, Some(replacement));
    assert_eq!(row.checkpoint_generation, Some(replacement));
    assert_eq!(row.state.record_complete, Some(false));
    assert!(
        serde_json::from_str::<Value>(row.state.payload.as_ref().unwrap()).unwrap()["access"]
            .get("Unavailable")
            .is_some()
    );
    let mut large = metadata.payload;
    large["reasons"] = json!(["🚧".repeat(40_000)]);
    let encoded = serde_json::to_string(&large).unwrap();
    assert!(encoded.chars().count() < 128 * 1024);
    assert!(encoded.len() > 128 * 1024);
    cache
        .connection
        .execute(
            "UPDATE records SET payload=?1 WHERE kind='collaboration-state-v1' AND id=?2",
            params![encoded, target.source()],
        )
        .unwrap();
    assert!(unavailable.validate(&cache).is_err());
    let oversized = cache
        .lookup_resource_bindings(
            &scope(),
            "example/repo",
            None,
            &[SyncBindingKind::PullRequestBody],
            1,
        )
        .unwrap();
    assert!(oversized.rows()[0].state.payload.is_none());
    assert!(oversized.rows()[0].state.payload_sha256.is_none());
    assert!(oversized.rows()[0].state.payload_bytes.unwrap() > 128 * 1024);
    cache
        .delete_record(&scope(), "collaboration-state-v1", &target.source())
        .unwrap();
    let missing = cache
        .lookup_resource_bindings(
            &scope(),
            "example/repo",
            None,
            &[SyncBindingKind::PullRequestBody],
            1,
        )
        .unwrap();
    assert!(missing.rows()[0].state.payload.is_none());
    assert!(!missing.rows()[0].state.member);
    assert_eq!(
        cache
            .connection
            .query_row(
                "SELECT COUNT(*) FROM records WHERE kind='collaboration-document-v1'",
                [],
                |row| row.get::<_, u64>(0)
            )
            .unwrap(),
        1
    );
}

#[test]
fn collaboration_index_binding_lookup_guards_scope_selector_issuer_and_atomic_epoch_failure() {
    let (_dir, mut cache) = fixture();
    for (nodes, kinds, cap) in [
        (None, vec![], 1),
        (None, vec![SyncBindingKind::PullRequestBody], 0),
        (None, vec![SyncBindingKind::PullRequestBody], 5001),
        (Some(vec![]), vec![SyncBindingKind::PullRequestBody], 1),
        (
            Some(vec!["x".repeat(1025)]),
            vec![SyncBindingKind::PullRequestBody],
            1,
        ),
        (
            None,
            vec![
                SyncBindingKind::PullRequestBody,
                SyncBindingKind::PullRequestBody,
            ],
            1,
        ),
    ] {
        assert!(cache
            .lookup_resource_bindings(&scope(), "example/repo", nodes.as_deref(), &kinds, cap)
            .is_err());
    }
    assert!(cache
        .lookup_resource_bindings(
            &Scope::new("github.invalid", "octocat").unwrap(),
            "example/repo",
            None,
            &[SyncBindingKind::PullRequestBody],
            1
        )
        .is_err());
    assert!(cache
        .lookup_resource_bindings(
            &scope(),
            "../repo",
            None,
            &[SyncBindingKind::PullRequestBody],
            1
        )
        .is_err());
    let empty = cache
        .lookup_resource_bindings(
            &scope(),
            "example/repo",
            None,
            &[SyncBindingKind::PullRequestBody],
            1,
        )
        .unwrap();
    let (_other_dir, other) = fixture();
    assert!(
        empty.validate(&other).is_err(),
        "identical empty metadata cannot transfer issuer proof"
    );
    let target = canonical(DiscussionKind::PullRequestBody, "PR_epoch", 7);
    let generation = publish_state(&mut cache, &target, json!("Allowed"), true);
    let before = cache
        .lookup_resource_bindings(
            &scope(),
            "example/repo",
            None,
            &[SyncBindingKind::PullRequestBody],
            2,
        )
        .unwrap();
    cache.connection.execute_batch("CREATE TRIGGER fail_binding_lookup_epoch BEFORE UPDATE OF generation ON sync_binding_lookup_epochs BEGIN SELECT RAISE(FAIL,'fixture epoch failure'); END;").unwrap();
    assert!(cache
        .fence_resource_sync_once(&scope(), &selected_fence(&target), 31)
        .is_err());
    assert_eq!(
        cache
            .binding_status(&scope(), &target.source())
            .unwrap()
            .unwrap()
            .visible_generation,
        Some(generation)
    );
    assert_eq!(
        cache
            .connection
            .query_row("SELECT COUNT(*) FROM sync_mutation_fences", [], |row| row
                .get::<_, u64>(
                0
            ))
            .unwrap(),
        0,
        "failed epoch bookkeeping rolls back receipt and visibility fence"
    );
    before.validate(&cache).unwrap();
}
