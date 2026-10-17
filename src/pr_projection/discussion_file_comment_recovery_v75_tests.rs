//! Real SQLite causal gaps, not a process-crash/live-GitHub claim. The
//! ordinary read guard must discover the SAME highest confirmed FILE attempt.
use super::*;
fn file_write(selected: &str) -> PrWriteIntent {
    PrWriteIntent {
        command: "file-comment-create-v75".into(),
        selected_resource: selected.into(),
        observed_head: "a".repeat(40),
        values: BTreeMap::from([
            ("body".into(), "actual file body".into()),
            ("subject-type".into(), "file".into()),
            ("path".into(), "src/界.rs".into()),
        ]),
    }
}
fn file_effect() -> PrConfirmedEffect {
    PrConfirmedEffect {
        kind: PrEffectKind::Comment,
        resource_id: "9181".into(),
        node_id: Some("ACTUAL_FILE_COMMENT_9181".into()),
    }
}
#[test]
fn file_comments_v75_actual_confirmed_gap_next_read_fences_inline_only_once_and_leaves_foreign_review_bodies(
) {
    let (_dir, database) = fixture();
    let inline = thread_target(42, DiscussionKind::InlineComment);
    let review = thread_target(42, DiscussionKind::Review);
    let foreign = thread_target(43, DiscussionKind::InlineComment);
    let (mut cache, old) = cached(&database, &inline);
    let (_, review_generation) = cached(&database, &review);
    let (_, foreign_generation) = cached(&database, &foreign);
    let before = raw_records(&database);
    let receipt = record_confirmed(
        &database,
        &inline,
        &file_write(&format!("file-comment-v75/{}", "a".repeat(64))),
        &file_effect(),
    );
    assert_eq!(ordinal(&database), None);
    assert_eq!(visible(&cache, &inline), Some(old));
    let target = PrWriteTarget::for_source(
        &inline.scope,
        &inline.repository,
        &inline.parent_node,
        inline.number,
    )
    .unwrap();
    let discovered = PrWriteJournal::open(&database)
        .unwrap()
        .latest_source_receipt(&target, SyncBindingKind::InlineComment)
        .unwrap()
        .unwrap();
    assert_eq!(discovered.publication_reference(), receipt);
    let guard =
        guard_discussion_reads(&database, &inline.scope, std::slice::from_ref(&inline)).unwrap();
    assert_eq!(guard.recovered_sources(), [inline.source()].as_slice());
    assert_eq!(ordinal(&database), Some(receipt.attempt_id as u64));
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
                json!({"body":"explicit new FILE hydration"})
            )],
            true
        )
        .unwrap());
    assert!(
        guard_discussion_reads(&database, &inline.scope, std::slice::from_ref(&inline))
            .unwrap()
            .recovered_sources()
            .is_empty()
    );
    assert_eq!(visible(&cache, &inline), Some(fresh));
    assert_eq!(ordinal(&database), Some(receipt.attempt_id as u64));
}
#[test]
fn file_comments_v75_newer_malformed_known_receipt_cannot_revive_older_confirmed_file_evidence() {
    for malformed in [
        "file-comment-v75/ABC",
        "thread-reply/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "unknown/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ] {
        let (_dir, database) = fixture();
        let inline = thread_target(42, DiscussionKind::InlineComment);
        let (cache, _) = cached(&database, &inline);
        let older = record_confirmed(
            &database,
            &inline,
            &file_write(&format!("file-comment-v75/{}", "a".repeat(64))),
            &file_effect(),
        );
        let newer = record_confirmed(&database, &inline, &file_write(malformed), &file_effect());
        assert!(newer.attempt_id > older.attempt_id);
        let target = PrWriteTarget::for_source(
            &inline.scope,
            &inline.repository,
            &inline.parent_node,
            inline.number,
        )
        .unwrap();
        let found = PrWriteJournal::open(&database)
            .unwrap()
            .latest_source_receipt(&target, SyncBindingKind::InlineComment)
            .unwrap()
            .unwrap();
        assert_eq!(found.publication_reference(), newer);
        assert!(
            guard_discussion_reads(&database, &inline.scope, std::slice::from_ref(&inline))
                .is_err()
        );
        assert_eq!(ordinal(&database), None);
        assert!(visible(&cache, &inline).is_some());
    }
}
