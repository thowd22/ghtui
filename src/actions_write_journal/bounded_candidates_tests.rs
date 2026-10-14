use super::*;

#[test]
fn actions_candidates_unlisted_oversized_original_refuses_before_new_marker() {
    let dir = directory();
    let mut journal = ActionsWriteJournal::open(&dir.path().join("cache.sqlite3")).unwrap();
    let first = target(run("example/first"));
    let second = target(run("example/second"));
    let value = intent(ActionsWriteKind::Cancel, b"never persisted");
    let older = journal.begin(&first, &value).unwrap();
    let latest = journal.begin(&second, &value).unwrap();
    journal
        .connection
        .execute(
            "UPDATE actions_write_attempts SET options_json=?1 WHERE attempt_id=?2",
            params!["x".repeat(131_073), older.marker().attempt_id],
        )
        .unwrap();
    let page = journal.list_unresolved(first.scope(), 1).unwrap();
    assert_eq!(page.markers[0].attempt_id, latest.marker().attempt_id);
    assert_eq!(page.total, 2);
    assert!(!page.complete);
    assert!(journal
        .blocking(&first, ActionsWriteKind::Cancel)
        .unwrap_err()
        .to_string()
        .contains("metadata budget"));
    assert!(journal
        .begin(&first, &value)
        .unwrap_err()
        .to_string()
        .contains("metadata budget"));
    assert_eq!(
        journal
            .blocking(&second, ActionsWriteKind::ForceCancel)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        journal
            .connection
            .query_row("SELECT COUNT(*) FROM actions_write_attempts", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
}

#[test]
fn actions_candidates_closed_fingerprint_corruption_refuses_read_and_begin() {
    let dir = directory();
    let mut journal = ActionsWriteJournal::open(&dir.path().join("cache.sqlite3")).unwrap();
    let scope = target(run("example/repo"));
    let value = intent(ActionsWriteKind::Cancel, b"");
    let permit = journal.begin(&scope, &value).unwrap();
    journal
        .connection
        .execute(
            "UPDATE actions_write_attempts SET kind_json='\"force_cancel\"' WHERE attempt_id=?1",
            [permit.marker().attempt_id],
        )
        .unwrap();
    assert!(journal
        .blocking(&scope, ActionsWriteKind::Cancel)
        .unwrap_err()
        .to_string()
        .contains("fingerprint"));
    assert!(journal
        .begin(&scope, &value)
        .unwrap_err()
        .to_string()
        .contains("fingerprint"));
    assert_eq!(
        journal
            .connection
            .query_row("SELECT COUNT(*) FROM actions_write_attempts", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        journal
            .connection
            .query_row(
                "SELECT state,claimed FROM actions_write_attempts",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            )
            .unwrap(),
        ("pending".into(), 0)
    );
}

#[test]
fn actions_candidates_overflow_is_explicit_read_refusal_not_a_global_lease_claim() {
    let dir = directory();
    let mut journal = ActionsWriteJournal::open(&dir.path().join("cache.sqlite3")).unwrap();
    let scope = target(run("example/crowded"));
    let value = intent(ActionsWriteKind::Cancel, b"");
    let original = journal.begin(&scope, &value).unwrap();
    let transaction = journal.connection.unchecked_transaction().unwrap();
    for _ in 0..256 {
        transaction.execute("INSERT INTO actions_write_attempts(host,principal,target_json,resources_json,kind_json,context_json,options_json,fingerprint_sha256,state,created_at) SELECT host,principal,target_json,resources_json,kind_json,context_json,options_json,fingerprint_sha256,state,created_at FROM actions_write_attempts WHERE attempt_id=?1",[original.marker().attempt_id]).unwrap();
        let id = transaction.last_insert_rowid();
        transaction.execute("INSERT INTO actions_write_resources(host,principal,resource_key,attempt_id) VALUES(?1,?2,?3,?4)",params![scope.scope().host,scope.scope().login,scope.resource_keys()[0],id]).unwrap();
    }
    transaction.commit().unwrap();
    let page = journal.list_unresolved(scope.scope(), 256).unwrap();
    assert_eq!(page.total, 257);
    assert_eq!(page.markers.len(), 256);
    assert!(!page.complete);
    let error = journal
        .blocking(&scope, ActionsWriteKind::ForceCancel)
        .unwrap_err()
        .to_string();
    assert!(error.contains("candidate read budget exceeded"));
    assert!(error.contains("no complete overlap decision"));
    assert!(journal.begin(&scope, &value).is_err());
    let other = target(run("example/independent"));
    assert!(journal
        .blocking(&other, ActionsWriteKind::Cancel)
        .unwrap()
        .is_empty());
    let separate = journal.begin(&other, &value).unwrap();
    assert!(separate.marker().attempt_id > original.marker().attempt_id);
    assert_eq!(
        journal
            .connection
            .query_row("SELECT COUNT(*) FROM actions_write_attempts", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        258
    );
}

#[test]
fn actions_candidates_keep_accepted_deployment_narrow_and_unknown_broad() {
    let dir = directory();
    let mut journal = ActionsWriteJournal::open(&dir.path().join("cache.sqlite3")).unwrap();
    let approved = deployment(vec![10], 3);
    let approve = intent(
        ActionsWriteKind::ApproveDeployment,
        b"not persisted explanation",
    );
    let permit = journal.begin(&approved, &approve).unwrap();
    journal.claim(&permit, &approved, &approve).unwrap();
    journal
        .mark_accepted_pending(&permit, &receipt(ActionsReceiptKind::DeploymentAccepted))
        .unwrap();
    let cancellation = target(run("example/repo"));
    assert!(journal
        .blocking(&cancellation, ActionsWriteKind::Cancel)
        .unwrap()
        .is_empty());
    assert_eq!(
        journal
            .blocking(
                &deployment(vec![10, 11], 3),
                ActionsWriteKind::RejectDeployment
            )
            .unwrap()
            .len(),
        1
    );
    assert!(journal
        .blocking(&deployment(vec![11], 3), ActionsWriteKind::RejectDeployment)
        .unwrap()
        .is_empty());
    assert!(journal
        .blocking(&deployment(vec![10], 4), ActionsWriteKind::RejectDeployment)
        .unwrap()
        .is_empty());
    let unknown = deployment(vec![11], 3);
    let other = journal.begin(&unknown, &approve).unwrap();
    journal.claim(&other, &unknown, &approve).unwrap();
    journal.mark_uncertain(&other).unwrap();
    let blockers = journal
        .blocking(&cancellation, ActionsWriteKind::Cancel)
        .unwrap();
    assert_eq!(blockers.len(), 1);
    assert_eq!(blockers[0].state, ActionsWriteState::Uncertain);
    let foreign =
        ActionsWriteTarget::new(&provider(), &identity(Some(43)), &run("example/repo")).unwrap();
    assert!(journal
        .blocking(&foreign, ActionsWriteKind::Cancel)
        .unwrap()
        .is_empty());
    assert!(journal
        .blocking(&target(run("example/other")), ActionsWriteKind::Cancel)
        .unwrap()
        .is_empty());
}
