use super::*;

#[path = "bindings_lookup_tests.rs"]
mod lookup_tests;
use std::{fs, os::unix::fs::PermissionsExt};

fn fixture() -> (tempfile::TempDir, Cache) {
    let dir = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let cache = Cache::open(&dir.path().join("cache.sqlite3")).unwrap();
    (dir, cache)
}
fn scope() -> Scope {
    Scope::github("github.invalid", Some(42)).unwrap()
}
fn source(value: u64) -> String {
    format!("collaboration-v1:{value:064x}")
}
fn binding(category: SyncBindingKind) -> SyncBinding {
    SyncBinding {
        namespace: SyncNamespace::Collaboration,
        repository_id: 9,
        repository: "example/repo".into(),
        resource: "PR_exact_7".into(),
        kind: category,
    }
}
fn fence(categories: Vec<SyncBindingKind>) -> SyncFence {
    SyncFence {
        namespace: SyncNamespace::Collaboration,
        repository_id: Some(9),
        repository: "EXAMPLE/REPO".into(),
        resource: "PR_exact_7".into(),
        kinds: categories,
    }
}
fn publish(cache: &mut Cache, key: &str, generation: u64, body: &str) -> bool {
    let record = Record::new(
        "collaboration-document-v1",
        key,
        "example/repo",
        serde_json::json!({"body":body}),
    );
    cache
        .commit_reserved_sync(&scope(), key, generation, &[record], true)
        .unwrap()
}
#[test]
fn collaboration_index_binding_first_inflight_is_fenced_before_any_body_or_checkpoint() {
    let (_dir, mut cache) = fixture();
    let key = source(1);
    let token = cache
        .reserve_resource_sync(
            &scope(),
            &key,
            &binding(SyncBindingKind::ConversationComment),
        )
        .unwrap();
    assert!(cache.sync_checkpoint(&scope(), &key).unwrap().is_none());
    assert_eq!(
        cache
            .binding_status(&scope(), &key)
            .unwrap()
            .unwrap()
            .visible_generation,
        None
    );
    let result = cache
        .fence_resource_sync(&scope(), &fence(vec![SyncBindingKind::ConversationComment]))
        .unwrap();
    assert_eq!(result.sources, vec![key.clone()]);
    assert_eq!(result.active_reservations_fenced, 1);
    assert!(!publish(&mut cache, &key, token, "old private text"));
    assert!(cache
        .get(&scope(), "collaboration-document-v1", &key)
        .unwrap()
        .is_none());
}
#[test]
fn collaboration_index_binding_rereserve_after_mutation_never_reactivates_old_visible_body() {
    let (_dir, mut cache) = fixture();
    let key = source(2);
    let bound = binding(SyncBindingKind::ConversationComment);
    let first = cache.reserve_resource_sync(&scope(), &key, &bound).unwrap();
    assert!(publish(&mut cache, &key, first, "original body"));
    assert_eq!(
        cache
            .binding_status(&scope(), &key)
            .unwrap()
            .unwrap()
            .visible_generation,
        Some(first)
    );
    cache
        .fence_resource_sync(&scope(), &fence(vec![SyncBindingKind::ConversationComment]))
        .unwrap();
    let replacement = cache.reserve_resource_sync(&scope(), &key, &bound).unwrap();
    assert_eq!(
        cache
            .binding_status(&scope(), &key)
            .unwrap()
            .unwrap()
            .visible_generation,
        None
    );
    assert_eq!(
        cache
            .get(&scope(), "collaboration-document-v1", &key)
            .unwrap()
            .unwrap()
            .payload["body"],
        "original body",
        "old bytes preserved, not searchable authority"
    );
    assert!(!publish(&mut cache, &key, first, "stale completion"));
    assert!(publish(&mut cache, &key, replacement, "fresh edited body"));
    assert_eq!(
        cache
            .binding_status(&scope(), &key)
            .unwrap()
            .unwrap()
            .visible_generation,
        Some(replacement)
    );
}
#[test]
fn collaboration_index_binding_exact_category_visibility_and_scopewide_derived_fence() {
    let (_dir, mut cache) = fixture();
    let comments = source(3);
    let reviews = source(4);
    let a = cache
        .reserve_resource_sync(
            &scope(),
            &comments,
            &binding(SyncBindingKind::ConversationComment),
        )
        .unwrap();
    assert!(publish(&mut cache, &comments, a, "comment body"));
    let b = cache
        .reserve_resource_sync(&scope(), &reviews, &binding(SyncBindingKind::Review))
        .unwrap();
    assert!(publish(&mut cache, &reviews, b, "review body"));
    let rank = cache
        .reserve_sync_generation(&scope(), "search-index:saved-context")
        .unwrap();
    let other = Scope::github("github.invalid", Some(43)).unwrap();
    let other_rank = cache
        .reserve_sync_generation(&other, "search-index:saved-context")
        .unwrap();
    let result = cache
        .fence_resource_sync(&scope(), &fence(vec![SyncBindingKind::ConversationComment]))
        .unwrap();
    assert_eq!(result.sources, vec![comments.clone()]);
    assert_eq!(
        cache
            .binding_status(&scope(), &comments)
            .unwrap()
            .unwrap()
            .visible_generation,
        None
    );
    assert_eq!(
        cache
            .binding_status(&scope(), &reviews)
            .unwrap()
            .unwrap()
            .visible_generation,
        Some(b)
    );
    assert!(!cache
        .commit_reserved_sync(&scope(), "search-index:saved-context", rank, &[], true)
        .unwrap());
    assert!(cache
        .commit_reserved_sync(&other, "search-index:saved-context", other_rank, &[], true)
        .unwrap());
}
#[test]
fn collaboration_index_binding_canonical_principal_repo_node_category_checks_do_not_guess() {
    let (_dir, mut cache) = fixture();
    let key = source(5);
    let bound = binding(SyncBindingKind::InlineComment);
    assert!(cache
        .reserve_resource_sync(
            &Scope::new("github.invalid", "octocat").unwrap(),
            &key,
            &bound
        )
        .is_err());
    let mut invalid = bound.clone();
    invalid.repository_id = 0;
    assert!(cache
        .reserve_resource_sync(&scope(), &key, &invalid)
        .is_err());
    assert!(cache
        .reserve_resource_sync(&scope(), "not-a-source", &bound)
        .is_err());
    let current = cache.reserve_resource_sync(&scope(), &key, &bound).unwrap();
    let mut changed = bound.clone();
    changed.resource = "PR_other".into();
    assert!(cache
        .reserve_resource_sync(&scope(), &key, &changed)
        .is_err());
    let mut wrong = fence(vec![SyncBindingKind::InlineComment]);
    wrong.repository_id = Some(99);
    assert!(cache
        .fence_resource_sync(&scope(), &wrong)
        .unwrap()
        .sources
        .is_empty());
    assert_eq!(
        cache
            .binding_status(&scope(), &key)
            .unwrap()
            .unwrap()
            .reservation_generation,
        current
    );
    assert!(
        cache.sync_reservation(&scope(), &key).unwrap().is_none(),
        "scope read fence remains conservative even with no matching text visibility"
    );
}
#[test]
fn collaboration_index_binding_same_name_recreated_repo_all_original_node_ids_bounded() {
    let (_dir, mut cache) = fixture();
    for (id, key) in [(9, source(6)), (99, source(7))] {
        let mut bound = binding(SyncBindingKind::Review);
        bound.repository_id = id;
        cache.reserve_resource_sync(&scope(), &key, &bound).unwrap();
    }
    let mut original = fence(vec![SyncBindingKind::Review]);
    original.repository_id = None;
    assert_eq!(
        cache
            .fence_resource_sync(&scope(), &original)
            .unwrap()
            .sources
            .len(),
        2
    );
    for id in 100..133 {
        let mut bound = binding(SyncBindingKind::Review);
        bound.repository_id = id;
        cache
            .reserve_resource_sync(&scope(), &source(id), &bound)
            .unwrap();
    }
    let active = cache.sync_reservation(&scope(), &source(132)).unwrap();
    assert!(cache
        .fence_resource_sync(&scope(), &original)
        .unwrap_err()
        .to_string()
        .contains("32"));
    assert_eq!(
        cache.sync_reservation(&scope(), &source(132)).unwrap(),
        active,
        "cap failure performs no guessed partial invalidation"
    );
}
#[test]
fn collaboration_index_binding_atomic_sql_failure_preserves_current_visibility_and_pending_reads() {
    let (_dir, mut cache) = fixture();
    let key = source(8);
    let bound = binding(SyncBindingKind::ConversationComment);
    let first = cache.reserve_resource_sync(&scope(), &key, &bound).unwrap();
    assert!(publish(&mut cache, &key, first, "verified cached body"));
    let slow = cache.reserve_resource_sync(&scope(), &key, &bound).unwrap();
    cache.connection.execute_batch("CREATE TRIGGER fail_fence BEFORE UPDATE OF visible_generation ON sync_resource_bindings BEGIN SELECT RAISE(FAIL,'fixture atomic fence failure'); END;").unwrap();
    assert!(cache
        .fence_resource_sync(&scope(), &fence(vec![SyncBindingKind::ConversationComment]))
        .is_err());
    assert_eq!(cache.sync_reservation(&scope(), &key).unwrap(), Some(slow));
    assert_eq!(
        cache
            .binding_status(&scope(), &key)
            .unwrap()
            .unwrap()
            .visible_generation,
        Some(first)
    );
    assert_eq!(
        cache
            .get(&scope(), "collaboration-document-v1", &key)
            .unwrap()
            .unwrap()
            .payload["body"],
        "verified cached body"
    );
}
#[test]
fn collaboration_index_binding_clear_and_revoke_preserve_registry_without_body_resurrection() {
    let (dir, mut cache) = fixture();
    let key = source(9);
    let bound = binding(SyncBindingKind::PullRequestBody);
    let first = cache.reserve_resource_sync(&scope(), &key, &bound).unwrap();
    assert!(publish(&mut cache, &key, first, "body before clear"));
    cache
        .put_draft(&scope(), "pr/draft", "private draft")
        .unwrap();
    cache.clear_records(&scope()).unwrap();
    assert_eq!(
        cache
            .binding_status(&scope(), &key)
            .unwrap()
            .unwrap()
            .visible_generation,
        None
    );
    assert_eq!(
        cache.draft(&scope(), "pr/draft").unwrap().as_deref(),
        Some("private draft")
    );
    drop(cache);
    let mut restarted = Cache::open(&dir.path().join("cache.sqlite3")).unwrap();
    let next = restarted
        .reserve_resource_sync(&scope(), &key, &bound)
        .unwrap();
    assert_eq!(
        restarted
            .binding_status(&scope(), &key)
            .unwrap()
            .unwrap()
            .visible_generation,
        None
    );
    assert!(publish(
        &mut restarted,
        &key,
        next,
        "fresh after explicit hydration"
    ));
    restarted
        .revoke_repository(&scope(), "example/repo")
        .unwrap();
    assert_eq!(
        restarted
            .binding_status(&scope(), &key)
            .unwrap()
            .unwrap()
            .visible_generation,
        None
    );
}

#[test]
fn collaboration_index_binding_late_or_replayed_receipt_never_hides_new_hydration() {
    let (_dir, mut cache) = fixture();
    let key = source(10);
    let bound = binding(SyncBindingKind::ConversationComment);
    let initial = cache.reserve_resource_sync(&scope(), &key, &bound).unwrap();
    assert!(publish(&mut cache, &key, initial, "original body"));
    let selector = fence(vec![SyncBindingKind::ConversationComment]);
    cache
        .fence_resource_sync_once(&scope(), &selector, 20)
        .unwrap();
    let fresh = cache.reserve_resource_sync(&scope(), &key, &bound).unwrap();
    assert!(publish(&mut cache, &key, fresh, "new confirmed body"));
    let pending_rank = cache
        .reserve_sync_generation(&scope(), "search-index:new-context")
        .unwrap();
    for old in [10, 20] {
        let result = cache
            .fence_resource_sync_once(&scope(), &selector, old)
            .unwrap();
        assert!(result.sources.is_empty());
        assert_eq!(result.active_reservations_fenced, 0);
        assert_eq!(
            cache
                .binding_status(&scope(), &key)
                .unwrap()
                .unwrap()
                .visible_generation,
            Some(fresh)
        );
        assert_eq!(
            cache
                .sync_reservation(&scope(), "search-index:new-context")
                .unwrap(),
            Some(pending_rank)
        );
    }
    cache
        .fence_resource_sync_once(&scope(), &selector, 21)
        .unwrap();
    assert_eq!(
        cache
            .binding_status(&scope(), &key)
            .unwrap()
            .unwrap()
            .visible_generation,
        None
    );
    assert!(cache
        .sync_reservation(&scope(), "search-index:new-context")
        .unwrap()
        .is_none());
}

#[test]
fn collaboration_index_binding_schema3_migration_preserves_source_intent_and_principal_isolation() {
    let (dir, mut cache) = fixture();
    let account = scope();
    let other = Scope::github("github.invalid", Some(43)).unwrap();
    cache
        .put_draft(&account, "pr/review", "saved review body")
        .unwrap();
    cache
        .save_view(
            &account,
            "search:discussion",
            &serde_json::json!({"query":"review-only"}),
        )
        .unwrap();
    let key = source(22);
    let token = cache.reserve_sync_generation(&account, &key).unwrap();
    let old = Record::new(
        "collaboration-document-v1",
        "existing",
        "example/repo",
        serde_json::json!({"body":"previous source data"}),
    );
    assert!(cache
        .commit_reserved_sync(&account, &key, token, std::slice::from_ref(&old), true)
        .unwrap());
    cache.connection.execute_batch("DROP TABLE sync_resource_bindings; DROP TABLE sync_mutation_fences; PRAGMA user_version=3;").unwrap();
    drop(cache);
    let mut migrated = Cache::open(&dir.path().join("cache.sqlite3")).unwrap();
    assert_eq!(
        migrated
            .connection
            .pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))
            .unwrap(),
        4
    );
    assert_eq!(
        migrated.draft(&account, "pr/review").unwrap().as_deref(),
        Some("saved review body")
    );
    assert_eq!(
        migrated.saved_view(&account, "search:discussion").unwrap(),
        Some(serde_json::json!({"query":"review-only"}))
    );
    assert_eq!(
        migrated
            .sync_frame(&account, &key)
            .unwrap()
            .unwrap()
            .records,
        vec![old]
    );
    assert!(
        migrated.binding_status(&account, &key).unwrap().is_none(),
        "historical rows do not gain resource authorization from migration"
    );
    let fresh = migrated
        .reserve_resource_sync(&account, &key, &binding(SyncBindingKind::Review))
        .unwrap();
    assert!(fresh > token);
    assert_eq!(
        migrated
            .binding_status(&account, &key)
            .unwrap()
            .unwrap()
            .visible_generation,
        None
    );
    assert!(migrated.binding_status(&other, &key).unwrap().is_none());
    assert!(publish(&mut migrated, &key, fresh, "fresh verified review"));
    assert_eq!(
        migrated
            .binding_status(&account, &key)
            .unwrap()
            .unwrap()
            .visible_generation,
        Some(fresh)
    );
}

#[test]
fn collaboration_index_binding_receipt_ordinal_transaction_failure_is_recoverable_not_acknowledged()
{
    let (_dir, mut cache) = fixture();
    let key = source(23);
    let bound = binding(SyncBindingKind::Review);
    let token = cache.reserve_resource_sync(&scope(), &key, &bound).unwrap();
    assert!(publish(&mut cache, &key, token, "visible prior review"));
    cache.connection.execute_batch("CREATE TRIGGER fail_receipt_fence BEFORE UPDATE OF visible_generation ON sync_resource_bindings BEGIN SELECT RAISE(ABORT,'fixture receipt fence failure'); END;").unwrap();
    let selector = fence(vec![SyncBindingKind::Review]);
    assert!(cache
        .fence_resource_sync_once(&scope(), &selector, 40)
        .is_err());
    assert_eq!(
        cache
            .connection
            .query_row("SELECT COUNT(*) FROM sync_mutation_fences", [], |row| row
                .get::<_, u64>(
                0
            ))
            .unwrap(),
        0,
        "receipt ordinal rolls back with failed visibility fence"
    );
    assert_eq!(
        cache
            .binding_status(&scope(), &key)
            .unwrap()
            .unwrap()
            .visible_generation,
        Some(token)
    );
    cache
        .connection
        .execute_batch("DROP TRIGGER fail_receipt_fence;")
        .unwrap();
    let retried = cache
        .fence_resource_sync_once(&scope(), &selector, 40)
        .unwrap();
    assert_eq!(retried.sources, vec![key.clone()]);
    assert_eq!(
        cache
            .binding_status(&scope(), &key)
            .unwrap()
            .unwrap()
            .visible_generation,
        None
    );
    let current = cache.reserve_resource_sync(&scope(), &key, &bound).unwrap();
    assert!(publish(
        &mut cache,
        &key,
        current,
        "fresh review after explicit hydration"
    ));
    assert!(cache
        .fence_resource_sync_once(&scope(), &selector, 40)
        .unwrap()
        .sources
        .is_empty());
    assert_eq!(
        cache
            .binding_status(&scope(), &key)
            .unwrap()
            .unwrap()
            .visible_generation,
        Some(current)
    );
}

#[test]
fn collaboration_index_binding_ordinary_publication_cannot_authorize_a_reserved_resource() {
    let (_dir, mut cache) = fixture();
    let key = source(24);
    let bound = binding(SyncBindingKind::ConversationComment);
    let token = cache.reserve_resource_sync(&scope(), &key, &bound).unwrap();
    let ordinary = Record::new(
        "collaboration-document-v1",
        "unverified",
        "example/repo",
        serde_json::json!({"body":"ordinary source bytes"}),
    );
    assert!(cache
        .commit_sync(&scope(), &key, token, &[ordinary], true)
        .unwrap());
    assert_eq!(
        cache
            .binding_status(&scope(), &key)
            .unwrap()
            .unwrap()
            .visible_generation,
        None,
        "ordinary source generation is not current reserved-fetch authority"
    );
    let fresh = cache.reserve_resource_sync(&scope(), &key, &bound).unwrap();
    assert!(publish(&mut cache, &key, fresh, "fresh reserved body"));
    assert_eq!(
        cache
            .binding_status(&scope(), &key)
            .unwrap()
            .unwrap()
            .visible_generation,
        Some(fresh)
    );
}

#[test]
fn collaboration_index_binding_reserved_publication_rolls_back_if_binding_authority_changed() {
    let (_dir, mut cache) = fixture();
    let key = source(25);
    let bound = binding(SyncBindingKind::ConversationComment);
    let first = cache.reserve_resource_sync(&scope(), &key, &bound).unwrap();
    assert!(publish(&mut cache, &key, first, "original allowed body"));
    let next = cache.reserve_resource_sync(&scope(), &key, &bound).unwrap();
    cache
        .connection
        .execute(
            "UPDATE sync_resource_bindings SET active=0 WHERE source=?1",
            params![key],
        )
        .unwrap();
    let replacement = Record::new(
        "collaboration-document-v1",
        &key,
        "example/repo",
        serde_json::json!({"body":"must not publish"}),
    );
    assert!(cache
        .commit_reserved_sync(&scope(), &key, next, &[replacement], true)
        .unwrap_err()
        .to_string()
        .contains("invalidated"));
    assert_eq!(
        cache.sync_checkpoint(&scope(), &key).unwrap(),
        Some((first, true))
    );
    assert_eq!(
        cache
            .get(&scope(), "collaboration-document-v1", &key)
            .unwrap()
            .unwrap()
            .payload["body"],
        "original allowed body"
    );
    assert_eq!(
        cache
            .binding_status(&scope(), &key)
            .unwrap()
            .unwrap()
            .visible_generation,
        Some(first)
    );
}
