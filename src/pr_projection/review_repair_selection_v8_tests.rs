//! Genuine SQLite causal declarations. No transport mutation or publication
//! authority is synthesized. Root alone executes these new tests.
use super::*;
use crate::{
    pr_projection::{guard_discussion_reads, validate_discussion_read_guard},
    pr_write_journal::{PrConfirmedEffect, PrEffectKind, PrWriteIntent},
    search::{
        collaboration::{DiscussionTarget, ParentKind},
        model::DiscussionKind,
    },
    storage::Record,
};
use rusqlite::{params, Connection};
use serde_json::json;
use std::{collections::BTreeMap, os::unix::fs::PermissionsExt};
fn fixture() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let path = dir.path().join("cache.sqlite3");
    (dir, path)
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
fn key(t: &DiscussionTarget) -> PrWriteTarget {
    PrWriteTarget::for_source(&t.scope, &t.repository, &t.parent_node, t.number).unwrap()
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
                json!({"body":"ORIGINAL_UNCHANGED_PRIVATE_BODY"})
            )],
            true
        )
        .unwrap());
    generation
}
fn visible(path: &Path, t: &DiscussionTarget) -> Option<u64> {
    Cache::open(path)
        .unwrap()
        .binding_status(&t.scope, &t.source())
        .unwrap()
        .unwrap()
        .visible_generation
}
fn confirmed(
    path: &Path,
    t: &DiscussionTarget,
    command: &str,
    selection: String,
    kind: PrEffectKind,
    id: &str,
) -> i64 {
    let mut journal = PrWriteJournal::open(path).unwrap();
    let key = key(t);
    let intent = PrWriteIntent {
        command: command.into(),
        selected_resource: selection,
        observed_head: "a".repeat(40),
        values: BTreeMap::from([("body".into(), "ACTUAL_ORIGINAL_INTENT".into())]),
    };
    let permit = journal.begin(&key, &intent).unwrap();
    journal.claim(&permit, &key, &intent).unwrap();
    journal
        .mark_confirmed(
            &permit,
            &PrConfirmedEffect {
                kind,
                resource_id: id.into(),
                node_id: Some(format!("NODE_{id}")),
            },
        )
        .unwrap();
    permit.marker().attempt_id
}
fn histories(path: &Path, t: &DiscussionTarget, inline_later: bool) -> (i64, i64) {
    let old = confirmed(
        path,
        t,
        "review-submit-v2",
        "review/810".into(),
        PrEffectKind::Review,
        "810",
    );
    let new = if inline_later {
        confirmed(
            path,
            t,
            "inline-comment",
            format!("inline/{}", "b".repeat(64)),
            PrEffectKind::Comment,
            "901",
        )
    } else {
        confirmed(
            path,
            t,
            "review-update-pending-v2",
            "review/810".into(),
            PrEffectKind::Review,
            "810",
        )
    };
    assert!(new > old);
    (old, new)
}
fn ordinals(path: &Path) -> Vec<(String, i64)> {
    let db = Connection::open(path).unwrap();
    let mut statement = db
        .prepare("SELECT kind,mutation_ordinal FROM sync_mutation_fences ORDER BY kind")
        .unwrap();
    statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn pending_reviews_v2_composite_v8_mixed_latest_real_histories_refresh_in_both_source_orders_without_false_current_equality(
) {
    for inline_later in [false, true] {
        for reverse in [false, true] {
            let (_dir, path) = fixture();
            let reviews = target(DiscussionKind::Review);
            let inline = target(DiscussionKind::InlineComment);
            hydrate(&path, &reviews);
            hydrate(&path, &inline);
            let neighbour = target(DiscussionKind::ConversationComment);
            let neighbour_generation = hydrate(&path, &neighbour);
            let (old, new) = histories(&path, &reviews, inline_later);
            let journal = PrWriteJournal::open(&path).unwrap();
            let a = journal
                .latest_source_receipt(&key(&reviews), SyncBindingKind::Review)
                .unwrap()
                .unwrap();
            let b = journal
                .latest_source_receipt(&key(&reviews), SyncBindingKind::InlineComment)
                .unwrap()
                .unwrap();
            assert_ne!(a.attempt_id, b.attempt_id);
            drop(journal);
            let order = if reverse {
                vec![inline.clone(), reviews.clone()]
            } else {
                vec![reviews.clone(), inline.clone()]
            };
            let guard = guard_discussion_reads(&path, &reviews.scope, &order).unwrap();
            assert!(guard
                .status()
                .iter()
                .any(|message| message.contains("historical composite")
                    && message.contains("newer single-category")));
            assert!(guard.blocked_sources().is_empty());
            assert_eq!(visible(&path, &reviews), None);
            assert_eq!(visible(&path, &inline), None);
            assert_eq!(visible(&path, &neighbour), Some(neighbour_generation));
            let fences = ordinals(&path);
            assert_eq!(fences.len(), 2);
            assert!(fences
                .iter()
                .any(|(kind, id)| kind == "review" && *id == if inline_later { old } else { new }));
            assert!(fences.iter().any(|(kind, id)| kind == "inline_comment"
                && *id == if inline_later { new } else { old }));
            validate_discussion_read_guard(&path, &guard).unwrap();
            let db = Connection::open(&path).unwrap();
            assert_eq!(
                db.query_row(
                    "SELECT count(*) FROM pr_write_attempts WHERE state='confirmed'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
                2
            );
        }
    }
}

#[test]
fn pending_reviews_v2_composite_v8_newer_already_fenced_then_fresh_hydration_survives_historical_repair_and_replay(
) {
    for inline_later in [false, true] {
        let (_dir, path) = fixture();
        let reviews = target(DiscussionKind::Review);
        let inline = target(DiscussionKind::InlineComment);
        hydrate(&path, &reviews);
        hydrate(&path, &inline);
        histories(&path, &reviews, inline_later);
        let (latest, older) = if inline_later {
            (&inline, &reviews)
        } else {
            (&reviews, &inline)
        };
        guard_discussion_reads(&path, &reviews.scope, std::slice::from_ref(latest)).unwrap();
        let fresh_latest = hydrate(&path, latest);
        let first =
            guard_discussion_reads(&path, &reviews.scope, std::slice::from_ref(older)).unwrap();
        assert!(first
            .status()
            .iter()
            .any(|line| line.contains("historical composite")));
        assert_eq!(
            visible(&path, latest),
            Some(fresh_latest),
            "old Both cannot invalidate the already newer fenced category"
        );
        assert_eq!(visible(&path, older), None);
        let fresh_older = hydrate(&path, older);
        let before = ordinals(&path);
        let replay =
            guard_discussion_reads(&path, &reviews.scope, &[older.clone(), latest.clone()])
                .unwrap();
        assert!(replay.recovered_sources().is_empty());
        assert_eq!(visible(&path, older), Some(fresh_older));
        assert_eq!(visible(&path, latest), Some(fresh_latest));
        assert_eq!(ordinals(&path), before);
    }
}

#[test]
fn pending_reviews_v2_composite_v8_actual_reference_change_without_generation_change_refuses_final_private_admission(
) {
    let (_dir, path) = fixture();
    let reviews = target(DiscussionKind::Review);
    let inline = target(DiscussionKind::InlineComment);
    hydrate(&path, &reviews);
    hydrate(&path, &inline);
    let (old, _) = histories(&path, &reviews, false);
    let guard =
        guard_discussion_reads(&path, &reviews.scope, std::slice::from_ref(&inline)).unwrap();
    let journal = PrWriteJournal::open(&path).unwrap();
    let generation = journal.source_generation(&key(&reviews)).unwrap();
    let db = Connection::open(&path).unwrap();
    assert_eq!(
        db.execute(
            "UPDATE pr_write_attempts SET payload_sha256=?1 WHERE attempt_id=?2",
            params!["c".repeat(64), old]
        )
        .unwrap(),
        1
    );
    assert_eq!(
        journal.source_generation(&key(&reviews)).unwrap(),
        generation,
        "causal control: old generic generation-only guard cannot detect this exact-ref change"
    );
    assert!(validate_discussion_read_guard(&path, &guard).is_err());
    assert_eq!(
        db.query_row("SELECT count(*) FROM pr_write_attempts", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
}

#[test]
fn pending_reviews_v2_composite_v8_second_atomic_effect_failure_stays_hidden_then_ordinary_fresh_repair_is_idempotent(
) {
    let (_dir, path) = fixture();
    let reviews = target(DiscussionKind::Review);
    let inline = target(DiscussionKind::InlineComment);
    hydrate(&path, &reviews);
    hydrate(&path, &inline);
    let (old, new) = histories(&path, &reviews, false);
    let db = Connection::open(&path).unwrap();
    db.execute_batch(&format!("CREATE TRIGGER refuse_newer_effect BEFORE INSERT ON sync_mutation_fences WHEN NEW.kind='review' AND NEW.mutation_ordinal={new} BEGIN SELECT RAISE(ABORT,'actual newer single effect refusal'); END;")).unwrap();
    assert!(guard_discussion_reads(&path, &reviews.scope, std::slice::from_ref(&inline)).is_err());
    assert_eq!(visible(&path, &reviews), None);
    assert_eq!(visible(&path, &inline), None);
    assert!(
        ordinals(&path).iter().all(|(_, id)| *id == old),
        "original Both committed atomically, not one fabricated split category"
    );
    db.execute_batch("DROP TRIGGER refuse_newer_effect;")
        .unwrap();
    guard_discussion_reads(&path, &reviews.scope, std::slice::from_ref(&inline)).unwrap();
    let a = hydrate(&path, &reviews);
    let b = hydrate(&path, &inline);
    let before = ordinals(&path);
    guard_discussion_reads(&path, &reviews.scope, &[reviews.clone(), inline.clone()]).unwrap();
    assert_eq!(visible(&path, &reviews), Some(a));
    assert_eq!(visible(&path, &inline), Some(b));
    assert_eq!(ordinals(&path), before);
    assert_eq!(before.len(), 2);
}

#[test]
fn pending_reviews_v2_composite_v8_actual_selected_ref_changes_between_atomic_effects_prevent_success_and_keep_whole_original_fence(
) {
    let (_dir, path) = fixture();
    let reviews = target(DiscussionKind::Review);
    let inline = target(DiscussionKind::InlineComment);
    hydrate(&path, &reviews);
    hydrate(&path, &inline);
    let (old, new) = histories(&path, &reviews, false);
    let db = Connection::open(&path).unwrap();
    db.execute_batch(&format!("CREATE TRIGGER move_selected_receipt AFTER INSERT ON sync_mutation_fences WHEN NEW.kind='inline_comment' AND NEW.mutation_ordinal={old} BEGIN UPDATE pr_write_attempts SET payload_sha256='{}' WHERE attempt_id={new}; END;","d".repeat(64))).unwrap();
    assert!(guard_discussion_reads(&path, &reviews.scope, std::slice::from_ref(&inline)).is_err());
    assert_eq!(visible(&path, &reviews), None);
    assert_eq!(visible(&path, &inline), None);
    let fences = ordinals(&path);
    assert_eq!(fences.len(), 2);
    assert!(fences.iter().all(|(_, id)| *id == old));
    assert_eq!(
        db.query_row(
            "SELECT payload_sha256 FROM pr_write_attempts WHERE attempt_id=?1",
            [new],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "d".repeat(64)
    );
}

#[test]
fn pending_reviews_v2_composite_v8_actual_same_path_database_replacement_and_malformed_superseder_refuse_without_recovery_guess(
) {
    for malformed in [false, true] {
        let (_dir, path) = fixture();
        let reviews = target(DiscussionKind::Review);
        let inline = target(DiscussionKind::InlineComment);
        hydrate(&path, &reviews);
        hydrate(&path, &inline);
        let (_, new) = histories(&path, &reviews, false);
        if malformed {
            let db = Connection::open(&path).unwrap();
            db.execute(
                "UPDATE pr_write_attempts SET selected_resource='review/0810' WHERE attempt_id=?1",
                [new],
            )
            .unwrap();
            assert!(
                guard_discussion_reads(&path, &reviews.scope, std::slice::from_ref(&inline))
                    .is_err()
            );
            assert!(ordinals(&path).is_empty());
        } else {
            let guard =
                guard_discussion_reads(&path, &reviews.scope, std::slice::from_ref(&inline))
                    .unwrap();
            let original = fs::symlink_metadata(&path).unwrap();
            let retained = path.with_file_name("original-retained.sqlite3");
            fs::rename(&path, &retained).unwrap();
            fs::copy(&retained, &path).unwrap();
            assert_ne!(original.ino(), fs::symlink_metadata(&path).unwrap().ino());
            assert!(validate_discussion_read_guard(&path, &guard).is_err());
            assert!(retained.exists());
        }
    }
}
