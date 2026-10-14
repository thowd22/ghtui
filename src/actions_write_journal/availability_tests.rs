use super::*;

#[test]
fn actions_unresolved_list_requires_positive_scope_and_exact_account() {
    let dir = directory();
    let mut journal = ActionsWriteJournal::open(&dir.path().join("cache.sqlite3")).unwrap();
    let scope = target(run("example/repo"));
    let value = intent(ActionsWriteKind::Cancel, b"not persisted");
    let permit = journal.begin(&scope, &value).unwrap();
    for invalid in [
        Scope::new("github.invalid", "octocat").unwrap(),
        Scope::new("github.invalid", "user-id:0").unwrap(),
        Scope::new("github.invalid", "user-id:042").unwrap(),
        Scope::new("GITHUB.INVALID", "user-id:42").unwrap(),
    ] {
        assert!(journal.list_unresolved(&invalid, 32).is_err());
    }
    for limit in [0, 257, usize::MAX] {
        assert!(journal.list_unresolved(scope.scope(), limit).is_err());
    }
    let page = journal.list_unresolved(scope.scope(), 1).unwrap();
    assert_eq!(page.total, 1);
    assert!(page.complete);
    assert_eq!(
        page.markers[0].publication_reference(),
        permit.marker().publication_reference()
    );
    let other_account = Scope::github("github.invalid", Some(43)).unwrap();
    let page = journal.list_unresolved(&other_account, 32).unwrap();
    assert_eq!(page.total, 0);
    assert!(page.complete);
    assert!(page.markers.is_empty());
    assert_eq!(
        journal
            .blocking(&scope, ActionsWriteKind::ForceCancel)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn actions_unresolved_list_bounds_latest_rows_and_omits_confirmed_receipts() {
    let dir = directory();
    let path = dir.path().join("cache.sqlite3");
    let mut journal = ActionsWriteJournal::open(&path).unwrap();
    let value = intent(ActionsWriteKind::Cancel, b"private body");
    let first = target(run("example/first"));
    let second = target(run("example/second"));
    let third = target(run("example/third"));
    let a = journal.begin(&first, &value).unwrap();
    let b = journal.begin(&second, &value).unwrap();
    journal.claim(&b, &second, &value).unwrap();
    journal.mark_uncertain(&b).unwrap();
    let c = journal.begin(&third, &value).unwrap();
    journal.claim(&c, &third, &value).unwrap();
    journal
        .mark_accepted_pending(&c, &receipt(ActionsReceiptKind::RunTransition))
        .unwrap();
    let page = journal.list_unresolved(first.scope(), 2).unwrap();
    assert_eq!(page.total, 3);
    assert!(!page.complete);
    assert_eq!(
        page.markers
            .iter()
            .map(|m| m.attempt_id)
            .collect::<Vec<_>>(),
        vec![c.marker().attempt_id, b.marker().attempt_id]
    );
    assert_eq!(page.markers[0].state, ActionsWriteState::AcceptedPending);
    assert_eq!(page.markers[1].state, ActionsWriteState::Uncertain);
    journal.claim(&a, &first, &value).unwrap();
    journal
        .mark_confirmed(&a, &receipt(ActionsReceiptKind::RunTransition))
        .unwrap();
    drop(journal);
    let reopened = ActionsWriteJournal::open(&path).unwrap();
    let page = reopened.list_unresolved(first.scope(), 256).unwrap();
    assert_eq!(page.total, 2);
    assert!(page.complete);
    assert_eq!(page.markers.len(), 2);
    assert!(reopened
        .blocking(&first, ActionsWriteKind::Cancel)
        .unwrap()
        .is_empty());
}

#[test]
fn actions_unresolved_list_rejects_oversized_corrupt_metadata_and_missing_receipts() {
    let dir = directory();
    let mut journal = ActionsWriteJournal::open(&dir.path().join("cache.sqlite3")).unwrap();
    let scope = target(run("example/repo"));
    let value = intent(ActionsWriteKind::Cancel, b"private body");
    let permit = journal.begin(&scope, &value).unwrap();
    let original: String = journal
        .connection
        .query_row(
            "SELECT options_json FROM actions_write_attempts WHERE attempt_id=?1",
            [permit.marker().attempt_id],
            |row| row.get(0),
        )
        .unwrap();
    journal
        .connection
        .execute(
            "UPDATE actions_write_attempts SET options_json=?1 WHERE attempt_id=?2",
            params!["x".repeat(131_073), permit.marker().attempt_id],
        )
        .unwrap();
    assert!(journal
        .list_unresolved(scope.scope(), 32)
        .unwrap_err()
        .to_string()
        .contains("metadata budget"));
    journal.connection.execute("UPDATE actions_write_attempts SET options_json=?1,kind_json='\"rerun_all\"' WHERE attempt_id=?2",params![original,permit.marker().attempt_id]).unwrap();
    assert!(journal
        .list_unresolved(scope.scope(), 32)
        .unwrap_err()
        .to_string()
        .contains("fingerprint"));
    journal.connection.execute("UPDATE actions_write_attempts SET kind_json='\"cancel\"',state='accepted_pending' WHERE attempt_id=?1",[permit.marker().attempt_id]).unwrap();
    assert!(journal
        .list_unresolved(scope.scope(), 32)
        .unwrap_err()
        .to_string()
        .contains("no typed receipt"));
    assert!(journal.begin(&scope, &value).is_err());
}

#[test]
fn actions_unresolved_blocks_target_is_account_resource_and_deployment_exact() {
    let dir = directory();
    let mut journal = ActionsWriteJournal::open(&dir.path().join("cache.sqlite3")).unwrap();
    let source = target(run("example/repo"));
    let value = intent(ActionsWriteKind::Cancel, b"");
    let permit = journal.begin(&source, &value).unwrap();
    let marker = permit.marker();
    assert!(marker.blocks_target(&source, ActionsWriteKind::ForceCancel));
    assert!(!marker.blocks_target(&target(run("example/other")), ActionsWriteKind::ForceCancel));
    let other =
        ActionsWriteTarget::new(&provider(), &identity(Some(43)), &run("example/repo")).unwrap();
    assert!(!marker.blocks_target(&other, ActionsWriteKind::Cancel));
    let selected = deployment(vec![10, 11], 3);
    let decision = intent(
        ActionsWriteKind::ApproveDeployment,
        b"not persisted comment",
    );
    let dir2 = directory();
    let mut journal = ActionsWriteJournal::open(&dir2.path().join("cache.sqlite3")).unwrap();
    let permit = journal.begin(&selected, &decision).unwrap();
    journal.claim(&permit, &selected, &decision).unwrap();
    journal
        .mark_accepted_pending(&permit, &receipt(ActionsReceiptKind::DeploymentAccepted))
        .unwrap();
    let marker = journal
        .list_unresolved(selected.scope(), 32)
        .unwrap()
        .markers
        .remove(0);
    assert!(marker.blocks_target(
        &deployment(vec![11, 12], 3),
        ActionsWriteKind::RejectDeployment
    ));
    assert!(!marker.blocks_target(
        &deployment(vec![12], 3),
        ActionsWriteKind::ApproveDeployment
    ));
    assert!(!marker.blocks_target(
        &deployment(vec![11], 4),
        ActionsWriteKind::ApproveDeployment
    ));
    assert!(!marker.blocks_target(&source, ActionsWriteKind::Cancel));
}
