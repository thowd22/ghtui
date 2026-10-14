use super::*;

#[test]
fn actions_original_bounded_read_rejects_oversized_or_changed_metadata_without_transport() {
    let dir = directory();
    let mut journal = ActionsWriteJournal::open(&dir.path().join("cache.sqlite3")).unwrap();
    let scope = target(run("example/repo"));
    let value = intent(ActionsWriteKind::Cancel, b"never persisted content");
    let permit = journal.begin(&scope, &value).unwrap();
    let reference = permit.marker().publication_reference();
    let original = journal.restored_publication(&scope, &reference).unwrap();
    assert_eq!(original.kind, ActionsWriteKind::Cancel);
    journal
        .connection
        .execute(
            "UPDATE actions_write_attempts SET context_json=?1 WHERE attempt_id=?2",
            params!["x".repeat(131_073), reference.attempt_id],
        )
        .unwrap();
    for result in [
        journal.restored_publication(&scope, &reference),
        journal.original_publication(&provider(), &identity(Some(42)), &reference),
    ] {
        assert!(result.unwrap_err().to_string().contains("metadata budget"));
    }
    assert!(journal
        .prepare_inspection(
            &provider(),
            &identity(Some(42)),
            reference.attempt_id,
            &CancellationToken::default()
        )
        .unwrap_err()
        .to_string()
        .contains("metadata budget"));
    journal.connection.execute("UPDATE actions_write_attempts SET context_json=?1,kind_json='\"force_cancel\"' WHERE attempt_id=?2",params![serde_json::to_string(&value.context).unwrap(),reference.attempt_id]).unwrap();
    assert!(journal
        .original_publication(&provider(), &identity(Some(42)), &reference)
        .unwrap_err()
        .to_string()
        .contains("fingerprint"));
    assert!(journal
        .restored_publication(&scope, &reference)
        .unwrap_err()
        .to_string()
        .contains("fingerprint"));
    assert!(journal
        .prepare_inspection(
            &provider(),
            &identity(Some(42)),
            reference.attempt_id,
            &CancellationToken::default()
        )
        .unwrap_err()
        .to_string()
        .contains("fingerprint"));
    // The intentionally nonexistent transport program proves refusal happens
    // before a provider read; this helper neither clears nor publishes markers.
    assert_eq!(
        journal
            .connection
            .query_row(
                "SELECT state FROM actions_write_attempts WHERE attempt_id=?1",
                [reference.attempt_id],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        "pending"
    );
}

#[test]
fn actions_original_bounded_read_hides_other_account_and_preserves_confirmed_receipt() {
    let dir = directory();
    let path = dir.path().join("cache.sqlite3");
    let mut journal = ActionsWriteJournal::open(&path).unwrap();
    let scope = target(run("example/repo"));
    let value = intent(ActionsWriteKind::Cancel, b"");
    let permit = journal.begin(&scope, &value).unwrap();
    journal.claim(&permit, &scope, &value).unwrap();
    journal
        .mark_confirmed(&permit, &receipt(ActionsReceiptKind::RunTransition))
        .unwrap();
    let reference = permit.marker().publication_reference();
    drop(journal);
    let journal = ActionsWriteJournal::open(&path).unwrap();
    let original = journal
        .original_publication(&provider(), &identity(Some(42)), &reference)
        .unwrap();
    assert_eq!(original.state, ActionsWriteState::Confirmed);
    assert_eq!(
        original.receipt,
        Some(receipt(ActionsReceiptKind::RunTransition))
    );
    assert!(journal
        .original_publication(&provider(), &identity(Some(43)), &reference)
        .is_err());
    let other =
        ActionsWriteTarget::new(&provider(), &identity(Some(43)), &run("example/repo")).unwrap();
    assert!(journal.restored_publication(&other, &reference).is_err());
    assert!(journal
        .prepare_inspection(
            &provider(),
            &identity(Some(42)),
            reference.attempt_id,
            &CancellationToken::default()
        )
        .unwrap_err()
        .to_string()
        .contains("no longer"));
    assert!(journal
        .blocking(&scope, ActionsWriteKind::Cancel)
        .unwrap()
        .is_empty());
}
