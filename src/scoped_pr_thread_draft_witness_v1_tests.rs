//! DECLARED/UNRUN actual SQLite + ordered-writer ACK contracts. Native held
//! provider journeys independently verify placement in the publication path.
use super::*;
use crate::{
    drafts::DraftWriter,
    pr_write_journal::{
        PrConfirmedEffect, PrEffectKind, PrWriteIntent, PrWriteJournal, PrWriteTarget,
    },
};
fn replaced(confirmed: bool) {
    let fixture = Fixture::new();
    let database = database(&fixture);
    let scope = Scope::github(HOST, Some(42)).unwrap();
    let key = format!("thread-draft/{}", "a".repeat(64));
    let target = PrWriteTarget::for_source(&scope, REPO, "PR_exact_sql", 7).unwrap();
    let intent = PrWriteIntent {
        command: "thread-reply".into(),
        selected_resource: format!("thread-reply/{}", "b".repeat(64)),
        observed_head: "a".repeat(40),
        values: std::collections::BTreeMap::from([(
            "body".into(),
            "original protected body".into(),
        )]),
    };
    let mut journal = PrWriteJournal::open(&database).unwrap();
    let permit = journal.begin(&target, &intent).unwrap();
    journal.claim(&permit, &target, &intent).unwrap();
    let reference = permit.marker().publication_reference();
    let original = publish::encode_draft("original protected body", Some(&reference)).unwrap();
    let mut writer = DraftWriter::new(database.clone());
    writer
        .save(scope.clone(), key.clone(), original.clone())
        .unwrap();
    let client = writer.client();
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(3);
    let fence = client
        .capture_publication_v2(&database, &scope, &key, &cancel, deadline)
        .unwrap();
    let ack = client
        .save_publication_and_wait_v2(fence, original.clone(), &cancel)
        .unwrap();
    assert!(publish::original_draft_matches_v1(
        &database, &scope, &key, &original, &cancel, deadline
    )
    .is_ok());
    if confirmed {
        journal
            .mark_confirmed(
                &permit,
                &PrConfirmedEffect {
                    kind: PrEffectKind::Comment,
                    resource_id: "901".into(),
                    node_id: Some("REPLY_901".into()),
                },
            )
            .unwrap();
    }
    let replacement =
        publish::encode_draft("EXTERNALLY_REPLACED_BODY_MUST_SURVIVE", Some(&reference)).unwrap();
    Cache::open(&database)
        .unwrap()
        .put_draft(&scope, &key, &replacement)
        .unwrap();
    // Actual external SQL leaves the SAME original producer/ACK current. This
    // proves why ACK-only would admit the boundary; no synthetic revision edit.
    ack.check_current(&client, &database, &scope, &key, &cancel)
        .unwrap();
    assert!(publish::original_draft_matches_v1(
        &database, &scope, &key, &original, &cancel, deadline
    )
    .is_err());
    assert_eq!(
        Cache::open(&database)
            .unwrap()
            .bounded_draft(&scope, &key, publish::DRAFT_CAP)
            .unwrap()
            .as_deref(),
        Some(replacement.as_str())
    );
    let marker = journal
        .original_publication(&target, &reference)
        .unwrap()
        .unwrap();
    assert_eq!(
        marker.state,
        if confirmed {
            crate::pr_write_journal::PrWriteState::Confirmed
        } else {
            crate::pr_write_journal::PrWriteState::Pending
        }
    );
    assert_eq!(marker.publication_reference(), reference);
    // This read-only boundary neither removes the replacement nor claims or
    // clears a marker. Native journeys prove zero/one actual POST respectively.
    let rows: u64 = rusqlite::Connection::open(&database)
        .unwrap()
        .query_row("SELECT count(*) FROM pr_write_attempts", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(rows, 1);
}
#[test]
fn review_threads_v1_external_same_database_draft_replacement_refuses_final_prepost_guard_with_current_ack(
) {
    replaced(false)
}
#[test]
fn review_threads_v1_external_same_database_draft_replacement_refuses_confirmed_precleanup_with_current_ack(
) {
    replaced(true)
}
#[test]
fn review_threads_v1_exact_encoded_draft_witness_refuses_missing_oversized_foreign_reference_and_cancel(
) {
    let fixture = Fixture::new();
    let database = database(&fixture);
    let scope = Scope::github(HOST, Some(42)).unwrap();
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(2);
    let original = publish::encode_draft("original", None).unwrap();
    let key = "thread-draft/exact";
    assert!(publish::original_draft_matches_v1(
        &database, &scope, key, &original, &cancel, deadline
    )
    .is_err());
    let cache = Cache::open(&database).unwrap();
    cache.put_draft(&scope, key, &"界".repeat(50_000)).unwrap();
    assert!(publish::original_draft_matches_v1(
        &database, &scope, key, &original, &cancel, deadline
    )
    .is_err());
    cache
        .put_draft(&Scope::github(HOST, Some(43)).unwrap(), key, &original)
        .unwrap();
    cache.remove_draft(&scope, key).unwrap();
    assert!(publish::original_draft_matches_v1(
        &database, &scope, key, &original, &cancel, deadline
    )
    .is_err());
    cache.put_draft(&scope, key, &original).unwrap();
    let wrong_reference = crate::pr_write_journal::PrPublicationReference {
        attempt_id: 91,
        pr_node_id: "FOREIGN_PR".into(),
        observed_head: "b".repeat(40),
        payload_sha256: "f".repeat(64),
    };
    cache
        .put_draft(
            &scope,
            key,
            &publish::encode_draft("original", Some(&wrong_reference)).unwrap(),
        )
        .unwrap();
    assert!(publish::original_draft_matches_v1(
        &database, &scope, key, &original, &cancel, deadline
    )
    .is_err());
    cache.put_draft(&scope, key, &original).unwrap();
    cancel.cancel();
    assert!(publish::original_draft_matches_v1(
        &database, &scope, key, &original, &cancel, deadline
    )
    .is_err());
    assert_eq!(
        cache.draft(&scope, key).unwrap().as_deref(),
        Some(original.as_str())
    );
}
