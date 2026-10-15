//! Actual confirmed SQLite record without terminal fence (not a real crash).
use super::*;
fn write(command: &str, target: String) -> PrWriteIntent {
    PrWriteIntent {
        command: command.into(),
        selected_resource: target,
        observed_head: "a".repeat(40),
        values: BTreeMap::from([("body".into(), "requested comment edit".into())]),
    }
}
fn receipt(command: &str) -> PrConfirmedEffect {
    PrConfirmedEffect {
        kind: if command == "thread-comment-edit-v2" {
            PrEffectKind::Comment
        } else {
            PrEffectKind::CommentDeleted
        },
        resource_id: "9009".into(),
        node_id: Some("ACTUAL_COMMENT_9009".into()),
    }
}
#[test]
fn review_threads_v2_comment_confirmed_crash_gap_recovers_exact_attempt_inline_only_once_foreign_review_untouched(
) {
    for command in ["thread-comment-edit-v2", "thread-comment-delete-v2"] {
        let (_dir, database) = fixture();
        let inline = thread_target(42, DiscussionKind::InlineComment);
        let review = thread_target(42, DiscussionKind::Review);
        let foreign = thread_target(43, DiscussionKind::InlineComment);
        let (mut cache, old) = cached(&database, &inline);
        let (_, review_generation) = cached(&database, &review);
        let (_, foreign_generation) = cached(&database, &foreign);
        let before = raw_records(&database);
        let reference = record_confirmed(
            &database,
            &inline,
            &write(command, format!("thread-comment-v2/{}", "a".repeat(64))),
            &receipt(command),
        );
        assert_eq!(ordinal(&database), None);
        assert_eq!(visible(&cache, &inline), Some(old));
        let guard = guard_discussion_reads(&database, &inline.scope, std::slice::from_ref(&inline))
            .unwrap();
        assert_eq!(guard.recovered_sources(), [inline.source()].as_slice());
        assert_eq!(ordinal(&database), Some(reference.attempt_id as u64));
        assert_eq!(visible(&cache, &inline), None);
        assert_eq!(visible(&cache, &review), Some(review_generation));
        assert_eq!(visible(&cache, &foreign), Some(foreign_generation));
        assert_eq!(raw_records(&database), before);
        let fresh = cache
            .reserve_resource_sync(&inline.scope, &inline.source(), &inline.binding().unwrap())
            .unwrap();
        assert!(cache
            .commit_reserved_sync(
                &inline.scope,
                &inline.source(),
                fresh,
                &[Record::new(
                    "collaboration-document-v1",
                    inline.source(),
                    &inline.repository,
                    json!({"body":"explicit later hydration","context":{"resolved":true}})
                )],
                true
            )
            .unwrap());
        let repeated =
            guard_discussion_reads(&database, &inline.scope, std::slice::from_ref(&inline))
                .unwrap();
        assert!(repeated.recovered_sources().is_empty());
        assert_eq!(ordinal(&database), Some(reference.attempt_id as u64));
        assert_eq!(visible(&cache, &inline), Some(fresh));
    }
}
#[test]
fn review_threads_v2_comment_malformed_latest_known_receipt_never_revives_older_valid_evidence() {
    for command in ["thread-comment-edit-v2", "thread-comment-delete-v2"] {
        let (_dir, database) = fixture();
        let inline = thread_target(42, DiscussionKind::InlineComment);
        let (_cache, _) = cached(&database, &inline);
        let older = record_confirmed(
            &database,
            &inline,
            &write(command, format!("thread-comment-v2/{}", "a".repeat(64))),
            &receipt(command),
        );
        let newer = record_confirmed(
            &database,
            &inline,
            &write(command, "thread-comment-v2/INVALID".into()),
            &receipt(command),
        );
        assert!(newer.attempt_id > older.attempt_id);
        let target = PrWriteTarget::for_source(
            &inline.scope,
            &inline.repository,
            &inline.parent_node,
            inline.number,
        )
        .unwrap();
        assert_eq!(
            PrWriteJournal::open(&database)
                .unwrap()
                .latest_source_receipt(&target, SyncBindingKind::InlineComment)
                .unwrap()
                .unwrap()
                .publication_reference(),
            newer
        );
        assert!(
            guard_discussion_reads(&database, &inline.scope, std::slice::from_ref(&inline))
                .is_err()
        );
        assert_eq!(ordinal(&database), None);
    }
}
