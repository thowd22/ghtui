use super::*;

#[path = "discussion_metadata_budget_tests.rs"]
mod budget_tests;
use crate::{
    pr_write_journal::{PrConfirmedEffect, PrWriteIntent, PrWriteState},
    search::{
        collaboration::{DiscussionTarget, ParentKind},
        model::DiscussionKind,
    },
    storage::Record,
};
use rusqlite::Connection;
use serde_json::json;
use std::{fs, os::unix::fs::PermissionsExt};

fn fixture() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let database = dir.path().join("cache.sqlite3");
    (dir, database)
}
fn target(category: DiscussionKind) -> DiscussionTarget {
    DiscussionTarget {
        scope: Scope::github("github.invalid", Some(42)).unwrap(),
        repository_id: 9,
        repository: "example/repo".into(),
        parent_kind: ParentKind::PullRequest,
        parent_node: "PR_guard_7".into(),
        number: 7,
        category,
    }
}
fn cached(database: &Path, target: &DiscussionTarget) -> (Cache, u64) {
    let mut cache = Cache::open(database).unwrap();
    let source = target.source();
    let token = cache
        .reserve_resource_sync(&target.scope, &source, &target.binding().unwrap())
        .unwrap();
    assert!(cache
        .commit_reserved_sync(
            &target.scope,
            &source,
            token,
            &[Record::new(
                "collaboration-document-v1",
                &source,
                &target.repository,
                json!({"body":"recoverable old private bytes"})
            )],
            true
        )
        .unwrap());
    (cache, token)
}
fn journal_intent() -> PrWriteIntent {
    PrWriteIntent {
        command: "new-comment".into(),
        selected_resource: "pr".into(),
        observed_head: "a".repeat(40),
        values: BTreeMap::from([("body".into(), "submitted private text".into())]),
    }
}
fn confirmed(database: &Path, target: &DiscussionTarget) -> PrPublicationReference {
    let key = PrWriteTarget::for_source(
        &target.scope,
        &target.repository,
        &target.parent_node,
        target.number,
    )
    .unwrap();
    let intent = journal_intent();
    let mut journal = PrWriteJournal::open(database).unwrap();
    let permit = journal.begin(&key, &intent).unwrap();
    journal.claim(&permit, &key, &intent).unwrap();
    journal
        .mark_confirmed(
            &permit,
            &PrConfirmedEffect {
                kind: PrEffectKind::Comment,
                resource_id: "10".into(),
                node_id: Some("IC_10".into()),
            },
        )
        .unwrap();
    permit.marker().publication_reference()
}

#[test]
fn collaboration_index_read_guard_recovers_confirmed_sql_failure_without_body_copy_or_resend() {
    let (_dir, database) = fixture();
    let target = target(DiscussionKind::ConversationComment);
    let (mut cache, token) = cached(&database, &target);
    let receipt = confirmed(&database, &target);
    let sql = Connection::open(&database).unwrap();
    sql.execute_batch("UPDATE records SET payload='unparseable retained record bytes'; DROP INDEX pr_write_source_receipts; CREATE TRIGGER fail_visibility BEFORE UPDATE OF visible_generation ON sync_resource_bindings BEGIN SELECT RAISE(ABORT,'fixture visibility recovery failure'); END;").unwrap();
    assert!(
        guard_discussion_reads(&database, &target.scope, std::slice::from_ref(&target))
            .unwrap_err()
            .contains("private metadata remains hidden")
    );
    assert_eq!(
        cache
            .binding_status(&target.scope, &target.source())
            .unwrap()
            .unwrap()
            .visible_generation,
        Some(token)
    );
    sql.execute_batch("DROP TRIGGER fail_visibility;").unwrap();
    let guard =
        guard_discussion_reads(&database, &target.scope, std::slice::from_ref(&target)).unwrap();
    assert_eq!(guard.recovered_sources, vec![target.source()]);
    assert!(guard.unavailable_sources.is_empty());
    let plan=sql.query_row("EXPLAIN QUERY PLAN SELECT attempt_id FROM pr_write_attempts WHERE host='github.invalid' AND principal='user-id:42' AND pr_node='PR_guard_7' AND state='confirmed' ORDER BY attempt_id DESC LIMIT 1",[],|row|row.get::<_,String>(3)).unwrap();
    assert!(
        plan.contains("pr_write_source_receipts"),
        "existing journals gain the exact positive-principal/PR/state receipt lookup index: {plan}"
    );
    assert_eq!(
        cache
            .binding_status(&target.scope, &target.source())
            .unwrap()
            .unwrap()
            .visible_generation,
        None
    );
    assert_eq!(
        sql.query_row("SELECT payload FROM records", [], |row| row
            .get::<_, String>(0))
            .unwrap(),
        "unparseable retained record bytes",
        "guard does not parse or erase recoverable source payloads"
    );
    validate_discussion_read_guard(&database, &guard).unwrap();
    let fresh = cache
        .reserve_resource_sync(&target.scope, &target.source(), &target.binding().unwrap())
        .unwrap();
    assert!(cache
        .commit_reserved_sync(
            &target.scope,
            &target.source(),
            fresh,
            &[Record::new(
                "collaboration-document-v1",
                target.source(),
                "example/repo",
                json!({"body":"new verified hydration"})
            )],
            true
        )
        .unwrap());
    let repeated =
        guard_discussion_reads(&database, &target.scope, std::slice::from_ref(&target)).unwrap();
    assert!(repeated.recovered_sources.is_empty());
    assert_eq!(
        cache
            .binding_status(&target.scope, &target.source())
            .unwrap()
            .unwrap()
            .visible_generation,
        Some(fresh),
        "latest processed receipt cannot hide newer explicit hydration"
    );
    assert_eq!(
        sql.query_row(
            "SELECT state,attempt_id FROM pr_write_attempts",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        )
        .unwrap(),
        ("confirmed".into(), receipt.attempt_id)
    );
}

#[test]
fn collaboration_index_read_guard_pending_label_is_not_invocation_confirmation_or_acknowledgement()
{
    let (_dir, database) = fixture();
    let comments = target(DiscussionKind::ConversationComment);
    let reviews = target(DiscussionKind::Review);
    let (cache, _) = cached(&database, &comments);
    let (_, review_generation) = cached(&database, &reviews);
    let key = PrWriteTarget::for_source(
        &comments.scope,
        &comments.repository,
        &comments.parent_node,
        comments.number,
    )
    .unwrap();
    let mut journal = PrWriteJournal::open(&database).unwrap();
    let permit = journal.begin(&key, &journal_intent()).unwrap();
    let guard = guard_discussion_reads(
        &database,
        &comments.scope,
        &[comments.clone(), reviews.clone()],
    )
    .unwrap();
    assert_eq!(guard.unavailable_sources.len(), 1);
    let notice = &guard.unavailable_sources[&comments.source()];
    assert!(notice.contains("Pending") && notice.contains("proves neither invocation nor success"));
    assert_eq!(
        cache
            .binding_status(&comments.scope, &comments.source())
            .unwrap()
            .unwrap()
            .visible_generation,
        None
    );
    assert_eq!(
        cache
            .binding_status(&reviews.scope, &reviews.source())
            .unwrap()
            .unwrap()
            .visible_generation,
        Some(review_generation)
    );
    assert_eq!(
        journal.blocking(&key).unwrap().unwrap().state,
        PrWriteState::Pending
    );
    assert!(permit.marker().confirmed.is_none());
    assert_eq!(
        Connection::open(&database)
            .unwrap()
            .query_row("SELECT COUNT(*) FROM sync_mutation_fences", [], |row| row
                .get::<_, u64>(
                0
            ))
            .unwrap(),
        0,
        "a pending read fence does not consume genuine receipt authority"
    );
}

#[test]
fn collaboration_index_read_guard_is_exact_positive_account_parent_kind_repo_number_and_category() {
    let (_dir, database) = fixture();
    let selected = target(DiscussionKind::ConversationComment);
    let (cache, token) = cached(&database, &selected);
    confirmed(&database, &selected);
    let mut other = selected.clone();
    other.scope = Scope::github("github.invalid", Some(43)).unwrap();
    let other_guard =
        guard_discussion_reads(&database, &other.scope, std::slice::from_ref(&other)).unwrap();
    assert!(other_guard.recovered_sources.is_empty());
    assert_eq!(
        cache
            .binding_status(&selected.scope, &selected.source())
            .unwrap()
            .unwrap()
            .visible_generation,
        Some(token)
    );
    assert!(guard_discussion_reads(&database, &selected.scope, &[other]).is_err());
    for changed in ["repo", "number"] {
        let mut wrong = selected.clone();
        match changed {
            "repo" => wrong.repository = "other/repo".into(),
            _ => wrong.number = 8,
        };
        assert!(
            guard_discussion_reads(&database, &selected.scope, &[wrong]).is_err(),
            "{changed}"
        );
    }
    let mut issue = selected.clone();
    issue.parent_kind = ParentKind::Issue;
    issue.category = DiscussionKind::IssueBody;
    let issue_guard =
        guard_discussion_reads(&database, &issue.scope, std::slice::from_ref(&issue)).unwrap();
    assert!(issue_guard.recovered_sources.is_empty(),"same repository/number/opaque node never silently changes an Issue source into PR receipt authority");
    assert_eq!(
        cache
            .binding_status(&selected.scope, &selected.source())
            .unwrap()
            .unwrap()
            .visible_generation,
        Some(token)
    );
    let review = target(DiscussionKind::Review);
    assert!(
        guard_discussion_reads(&database, &review.scope, std::slice::from_ref(&review))
            .unwrap()
            .recovered_sources
            .is_empty()
    );
    let mut invalid = selected.clone();
    invalid.scope = Scope::new("github.invalid", "octocat").unwrap();
    assert!(
        guard_discussion_reads(&database, &invalid.scope, std::slice::from_ref(&invalid)).is_err()
    );
}

#[test]
fn collaboration_index_read_guard_generation_and_database_proof_reject_late_publication() {
    let (dir, database) = fixture();
    let selected = target(DiscussionKind::ConversationComment);
    cached(&database, &selected);
    let guard = guard_discussion_reads(&database, &selected.scope, std::slice::from_ref(&selected))
        .unwrap();
    validate_discussion_read_guard(&database, &guard).unwrap();
    let other = dir.path().join("other.sqlite3");
    Cache::open(&other).unwrap();
    assert!(validate_discussion_read_guard(&other, &guard)
        .unwrap_err()
        .contains("another database"));
    let key = PrWriteTarget::for_source(
        &selected.scope,
        &selected.repository,
        &selected.parent_node,
        selected.number,
    )
    .unwrap();
    PrWriteJournal::open(&database)
        .unwrap()
        .begin(&key, &journal_intent())
        .unwrap();
    assert!(validate_discussion_read_guard(&database, &guard)
        .unwrap_err()
        .contains("publication changed"));
    let fresh = guard_discussion_reads(&database, &selected.scope, std::slice::from_ref(&selected))
        .unwrap();
    assert!(fresh.unavailable_sources.contains_key(&selected.source()));
}

#[test]
fn collaboration_index_read_guard_bounds_metadata_before_parse_and_refuses_unbounded_input() {
    let (_dir, database) = fixture();
    let selected = target(DiscussionKind::ConversationComment);
    let (cache, token) = cached(&database, &selected);
    confirmed(&database, &selected);
    Connection::open(&database)
        .unwrap()
        .execute_batch(
            "UPDATE pr_write_attempts SET confirmed_json=printf('%10000s','not a valid receipt');",
        )
        .unwrap();
    assert!(
        guard_discussion_reads(&database, &selected.scope, std::slice::from_ref(&selected))
            .unwrap_err()
            .contains("8 KiB")
    );
    assert_eq!(
        cache
            .binding_status(&selected.scope, &selected.source())
            .unwrap()
            .unwrap()
            .visible_generation,
        Some(token)
    );
    assert!(
        guard_discussion_reads(&database, &selected.scope, &vec![selected.clone(); 257])
            .unwrap_err()
            .contains("256")
    );
}
