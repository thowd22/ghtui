//! Real SQL composite source fences; no remote effect is inferred from markers.
use super::*;
use crate::{
    pr_projection::guard_discussion_reads,
    pr_write_journal::{PrConfirmedEffect, PrWriteIntent, PrWriteJournal, PrWriteTarget},
    search::{
        collaboration::{DiscussionTarget, ParentKind},
        model::DiscussionKind,
    },
    storage::{Cache, Record, Scope},
};
use rusqlite::{params, Connection};
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

fn fixture() -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let path = directory.path().join("cache.sqlite3");
    (directory, path)
}
fn target(kind: DiscussionKind) -> DiscussionTarget {
    DiscussionTarget {
        scope: Scope::github("github.invalid", Some(42)).unwrap(),
        repository_id: 410,
        repository: "example/repo".into(),
        parent_kind: ParentKind::PullRequest,
        parent_node: "PR_exact7".into(),
        number: 7,
        category: kind,
    }
}
fn key(target: &DiscussionTarget) -> PrWriteTarget {
    PrWriteTarget::for_source(
        &target.scope,
        &target.repository,
        &target.parent_node,
        target.number,
    )
    .unwrap()
}
fn hydrate(path: &Path, target: &DiscussionTarget) -> u64 {
    let mut cache = Cache::open(path).unwrap();
    let source = target.source();
    let generation = cache
        .reserve_resource_sync(&target.scope, &source, &target.binding().unwrap())
        .unwrap();
    assert!(cache
        .commit_reserved_sync(
            &target.scope,
            &source,
            generation,
            &[Record::new(
                "collaboration-document-v1",
                &source,
                &target.repository,
                json!({"body":"RETAINED_PRIVATE_BYTES"})
            )],
            true
        )
        .unwrap());
    generation
}
fn visible(path: &Path, target: &DiscussionTarget) -> Option<u64> {
    Cache::open(path)
        .unwrap()
        .binding_status(&target.scope, &target.source())
        .unwrap()
        .unwrap()
        .visible_generation
}
fn intent(command: &str, selected: &str) -> PrWriteIntent {
    PrWriteIntent {
        command: command.into(),
        selected_resource: selected.into(),
        observed_head: "a".repeat(40),
        values: BTreeMap::from([("body".into(), "ORIGINAL_UNCHANGED_PAYLOAD".into())]),
    }
}
fn selected(command: &str) -> String {
    if command == "review-create-pending-v2" {
        format!("review-draft/{}", "a".repeat(64))
    } else {
        "review/810".into()
    }
}
fn confirmed(
    path: &Path,
    target: &DiscussionTarget,
    command: &str,
    selection: &str,
    kind: PrEffectKind,
) -> i64 {
    let mut journal = PrWriteJournal::open(path).unwrap();
    let key = key(target);
    let intent = intent(command, selection);
    let permit = journal.begin(&key, &intent).unwrap();
    journal.claim(&permit, &key, &intent).unwrap();
    journal
        .mark_confirmed(
            &permit,
            &PrConfirmedEffect {
                kind,
                resource_id: "810".into(),
                node_id: Some("ACTUAL_REVIEW_810".into()),
            },
        )
        .unwrap();
    permit.marker().attempt_id
}
fn ordinals(path: &Path) -> Vec<(String, i64)> {
    let connection = Connection::open(path).unwrap();
    let mut statement = connection
        .prepare("SELECT kind,mutation_ordinal FROM sync_mutation_fences ORDER BY kind")
        .unwrap();
    statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn pending_reviews_v2_composite_crash_recovery_both_directions_is_one_atomic_fence_and_replay_preserves_newer_hydration(
) {
    for command in [
        "review-create-pending-v2",
        "review-submit-v2",
        "review-discard-v2",
    ] {
        for reverse in [false, true] {
            let (_directory, path) = fixture();
            let reviews = target(DiscussionKind::Review);
            let inline = target(DiscussionKind::InlineComment);
            let neighbour = target(DiscussionKind::ConversationComment);
            hydrate(&path, &reviews);
            hydrate(&path, &inline);
            let neighbour_generation = hydrate(&path, &neighbour);
            let mut cache = Cache::open(&path).unwrap();
            cache
                .reserve_sync_generation(&reviews.scope, "search-index:sentinel")
                .unwrap();
            let sql = Connection::open(&path).unwrap();
            sql.execute_batch("CREATE TABLE review_fence_audit(n INTEGER); CREATE TRIGGER review_one_fence AFTER UPDATE OF generation ON sync_reservations WHEN NEW.source='search-index:sentinel' BEGIN INSERT INTO review_fence_audit VALUES(1); END;").unwrap();
            let attempt = confirmed(
                &path,
                &reviews,
                command,
                &selected(command),
                if command == "review-discard-v2" {
                    PrEffectKind::ReviewDiscarded
                } else {
                    PrEffectKind::Review
                },
            );
            let journal = PrWriteJournal::open(&path).unwrap();
            for category in [SyncBindingKind::Review, SyncBindingKind::InlineComment] {
                assert_eq!(
                    journal
                        .latest_source_receipt(&key(&reviews), category)
                        .unwrap()
                        .unwrap()
                        .attempt_id,
                    attempt
                );
            }
            let pair = if reverse {
                vec![inline.clone(), reviews.clone()]
            } else {
                vec![reviews.clone(), inline.clone()]
            };
            let guard = guard_discussion_reads(&path, &reviews.scope, &pair).unwrap();
            assert_eq!(guard.recovered_sources().len(), 2);
            assert!(guard.blocked_sources().is_empty());
            assert_eq!(visible(&path, &reviews), None);
            assert_eq!(visible(&path, &inline), None);
            assert_eq!(visible(&path, &neighbour), Some(neighbour_generation));
            let fences = ordinals(&path);
            assert_eq!(fences.len(), 2);
            assert!(fences.iter().all(|(_, ordinal)| *ordinal == attempt));
            assert_eq!(
                sql.query_row("SELECT COUNT(*) FROM review_fence_audit", [], |row| row
                    .get::<_, u64>(0))
                    .unwrap(),
                1
            );
            assert_eq!(
                sql.query_row(
                    "SELECT COUNT(*) FROM records WHERE payload LIKE '%RETAINED_PRIVATE_BYTES%'",
                    [],
                    |row| row.get::<_, u64>(0)
                )
                .unwrap(),
                3
            );
            let fresh_review = hydrate(&path, &reviews);
            let fresh_inline = hydrate(&path, &inline);
            let replay = guard_discussion_reads(
                &path,
                &reviews.scope,
                &pair.into_iter().rev().collect::<Vec<_>>(),
            )
            .unwrap();
            assert!(replay.recovered_sources().is_empty());
            assert_eq!(visible(&path, &reviews), Some(fresh_review));
            assert_eq!(visible(&path, &inline), Some(fresh_inline));
            assert_eq!(
                sql.query_row("SELECT COUNT(*) FROM review_fence_audit", [], |row| row
                    .get::<_, u64>(0))
                    .unwrap(),
                1
            );
            assert_eq!(
                sql.query_row(
                    "SELECT COUNT(*) FROM pr_write_attempts WHERE state='confirmed'",
                    [],
                    |row| row.get::<_, u64>(0)
                )
                .unwrap(),
                1
            );
        }
    }
}

#[test]
fn pending_reviews_v2_unresolved_batch_hides_both_without_consuming_confirmation_or_claiming_invocation(
) {
    for uncertain in [false, true] {
        let (_directory, path) = fixture();
        let reviews = target(DiscussionKind::Review);
        let inline = target(DiscussionKind::InlineComment);
        hydrate(&path, &reviews);
        hydrate(&path, &inline);
        let mut journal = PrWriteJournal::open(&path).unwrap();
        let key = key(&reviews);
        let intent = intent(
            "review-create-pending-v2",
            &selected("review-create-pending-v2"),
        );
        let permit = journal.begin(&key, &intent).unwrap();
        if uncertain {
            journal.claim(&permit, &key, &intent).unwrap();
            journal.mark_uncertain(&permit).unwrap();
        }
        let guard =
            guard_discussion_reads(&path, &reviews.scope, std::slice::from_ref(&reviews)).unwrap();
        let notice = &guard.blocked_sources()[&reviews.source()];
        assert!(notice.contains("proves neither invocation nor success"));
        assert_eq!(visible(&path, &reviews), None);
        assert_eq!(visible(&path, &inline), None);
        assert!(ordinals(&path).is_empty());
        assert!(journal
            .source_blocking(&key)
            .unwrap()
            .unwrap()
            .confirmed
            .is_none());
        let review_generation = hydrate(&path, &reviews);
        let inline_generation = hydrate(&path, &inline);
        if !uncertain {
            journal.claim(&permit, &key, &intent).unwrap();
        }
        journal
            .mark_confirmed(
                &permit,
                &PrConfirmedEffect {
                    kind: PrEffectKind::Review,
                    resource_id: "810".into(),
                    node_id: Some("ACTUAL_REVIEW_810".into()),
                },
            )
            .unwrap();
        assert_eq!(visible(&path, &reviews), Some(review_generation));
        assert_eq!(visible(&path, &inline), Some(inline_generation));
        guard_discussion_reads(&path, &inline.scope, std::slice::from_ref(&inline)).unwrap();
        assert_eq!(visible(&path, &reviews), None);
        assert_eq!(visible(&path, &inline), None);
        assert_eq!(ordinals(&path).len(), 2);
    }
}

#[test]
fn pending_reviews_v2_summary_or_dismissal_is_review_only_not_an_inline_body_change() {
    for command in ["review-update-pending-v2", "review-dismiss-v2"] {
        let (_directory, path) = fixture();
        let reviews = target(DiscussionKind::Review);
        let inline = target(DiscussionKind::InlineComment);
        hydrate(&path, &reviews);
        let original_inline = hydrate(&path, &inline);
        confirmed(&path, &reviews, command, "review/810", PrEffectKind::Review);
        let journal = PrWriteJournal::open(&path).unwrap();
        assert!(journal
            .latest_source_receipt(&key(&reviews), SyncBindingKind::InlineComment)
            .unwrap()
            .is_none());
        guard_discussion_reads(&path, &reviews.scope, &[inline.clone(), reviews.clone()]).unwrap();
        assert_eq!(visible(&path, &reviews), None);
        assert_eq!(visible(&path, &inline), Some(original_inline));
        assert_eq!(ordinals(&path).len(), 1);
    }
}

#[test]
fn pending_reviews_v2_owned_malformed_newest_selector_or_effect_never_revives_older_valid_receipt()
{
    for (command, selection, kind) in [
        ("review-submit-v2", "review/0810", PrEffectKind::Review),
        ("review-submit-v2", "review/811", PrEffectKind::Review),
        ("review-discard-v2", "review/810", PrEffectKind::Review),
        (
            "review-create-pending-v2",
            "review-draft/not-a-digest",
            PrEffectKind::Review,
        ),
        (
            "review-create-pending-v2",
            "review-draft/AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            PrEffectKind::Comment,
        ),
    ] {
        let (_directory, path) = fixture();
        let reviews = target(DiscussionKind::Review);
        let inline = target(DiscussionKind::InlineComment);
        let review_generation = hydrate(&path, &reviews);
        let inline_generation = hydrate(&path, &inline);
        confirmed(
            &path,
            &reviews,
            "review-submit-v2",
            "review/810",
            PrEffectKind::Review,
        );
        let bad = confirmed(&path, &reviews, command, selection, kind);
        let journal = PrWriteJournal::open(&path).unwrap();
        for category in [SyncBindingKind::Review, SyncBindingKind::InlineComment] {
            assert_eq!(
                journal
                    .latest_source_receipt(&key(&reviews), category)
                    .unwrap()
                    .unwrap()
                    .attempt_id,
                bad
            );
        }
        assert!(
            guard_discussion_reads(&path, &reviews.scope, &[reviews.clone(), inline.clone()])
                .is_err()
        );
        assert_eq!(visible(&path, &reviews), Some(review_generation));
        assert_eq!(visible(&path, &inline), Some(inline_generation));
        assert!(
            ordinals(&path).is_empty(),
            "failed guard publishes no body and guesses no repair"
        );
    }
}

#[test]
fn pending_reviews_v2_second_category_sql_failure_rolls_back_whole_composite_then_fresh_guard_repairs_both(
) {
    let (_directory, path) = fixture();
    let reviews = target(DiscussionKind::Review);
    let inline = target(DiscussionKind::InlineComment);
    let review_generation = hydrate(&path, &reviews);
    let inline_generation = hydrate(&path, &inline);
    confirmed(
        &path,
        &reviews,
        "review-submit-v2",
        "review/810",
        PrEffectKind::Review,
    );
    let sql = Connection::open(&path).unwrap();
    sql.execute_batch("CREATE TRIGGER refuse_second_review_category BEFORE INSERT ON sync_mutation_fences WHEN NEW.kind='inline_comment' BEGIN SELECT RAISE(ABORT,'actual second category refusal'); END;").unwrap();
    assert!(guard_discussion_reads(&path, &reviews.scope, std::slice::from_ref(&reviews)).is_err());
    assert_eq!(visible(&path, &reviews), Some(review_generation));
    assert_eq!(visible(&path, &inline), Some(inline_generation));
    assert!(ordinals(&path).is_empty());
    sql.execute_batch("DROP TRIGGER refuse_second_review_category;")
        .unwrap();
    guard_discussion_reads(&path, &reviews.scope, std::slice::from_ref(&inline)).unwrap();
    assert_eq!(visible(&path, &reviews), None);
    assert_eq!(visible(&path, &inline), None);
    assert_eq!(ordinals(&path).len(), 2);
}

#[test]
fn pending_reviews_v2_composite_receipt_cannot_cross_numeric_scope_repo_node_or_pr_number() {
    let (_directory, path) = fixture();
    let reviews = target(DiscussionKind::Review);
    let inline = target(DiscussionKind::InlineComment);
    let review_generation = hydrate(&path, &reviews);
    let inline_generation = hydrate(&path, &inline);
    confirmed(
        &path,
        &reviews,
        "review-submit-v2",
        "review/810",
        PrEffectKind::Review,
    );
    let mut foreign = inline.clone();
    foreign.scope = Scope::github("github.invalid", Some(43)).unwrap();
    let foreign_generation = hydrate(&path, &foreign);
    let guard =
        guard_discussion_reads(&path, &foreign.scope, std::slice::from_ref(&foreign)).unwrap();
    assert!(guard.recovered_sources().is_empty());
    assert_eq!(visible(&path, &foreign), Some(foreign_generation));
    assert!(guard_discussion_reads(&path, &reviews.scope, &[foreign]).is_err());
    for change in ["repo", "number"] {
        let mut wrong = inline.clone();
        if change == "repo" {
            wrong.repository = "other/repo".into();
        } else {
            wrong.number = 8;
        }
        assert!(guard_discussion_reads(&path, &reviews.scope, &[wrong]).is_err());
    }
    let mut other_node = inline.clone();
    other_node.parent_node = "PR_other".into();
    let other_generation = hydrate(&path, &other_node);
    assert!(
        guard_discussion_reads(&path, &other_node.scope, std::slice::from_ref(&other_node))
            .unwrap()
            .recovered_sources()
            .is_empty()
    );
    assert_eq!(visible(&path, &other_node), Some(other_generation));
    assert_eq!(visible(&path, &reviews), Some(review_generation));
    assert_eq!(visible(&path, &inline), Some(inline_generation));
    assert!(ordinals(&path).is_empty());
    let sql = Connection::open(&path).unwrap();
    assert_eq!(
        sql.query_row(
            "SELECT COUNT(*) FROM pr_write_attempts WHERE principal=?1 AND state='confirmed'",
            params!["user-id:42"],
            |row| row.get::<_, u64>(0)
        )
        .unwrap(),
        1
    );
}
