//! SOURCE ONLY / UNRUN. Actual private SQLite claimed-state/generation/file
//! effects; pure durable metadata fixtures are NOT source or write permits.
use super::*;
use std::{fs, os::unix::fs::PermissionsExt};

#[test]
fn scoped_actions_publication_v3_scoped_generations_are_not_adopted_between_begin_claim_or_finish()
{
    for after_claim in [false, true] {
        let temp = directory();
        let (mut journal, target, intent) = setup(&temp.path().join("cache.sqlite3"));
        let permit = journal.begin(&target, &intent).unwrap();
        if after_claim {
            journal.claim(&permit, &target, &intent).unwrap();
        }
        assert_eq!(journal.connection.execute("UPDATE actions_write_generations SET generation=generation+1 WHERE resource_key=?1", [&target.resources[0]]).unwrap(),1);
        let before = state(&journal);
        if after_claim {
            let receipt = ActionsReceipt {
                kind: ActionsReceiptKind::RunTransition,
                resource_id: Some(123),
                observed_attempt: Some(2),
                observed_run_id: None,
                original_dispatch_run_v7: None,
                workflow_remote_v9: None,
            };
            assert!(journal.mark_confirmed(&permit, &receipt).is_err());
            assert!(journal.capture_claim_stamp_v3(&permit).is_err());
        } else {
            assert!(journal.claim(&permit, &target, &intent).is_err());
            assert!(!permit.used.load(Ordering::Acquire));
        }
        assert_eq!(
            state(&journal),
            before,
            "No adopted generation, claim or fabricated receipt"
        );
        let marker = journal
            .restored_publication(&target, &permit.marker().publication_reference())
            .unwrap();
        assert_eq!(marker.state, ActionsWriteState::Pending);
        assert!(marker.receipt.is_none());
    }
}

fn directory() -> tempfile::TempDir {
    tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap()
}
fn setup(path: &Path) -> (ActionsWriteJournal, ActionsWriteTarget, ActionsWriteIntent) {
    let context = ActionsImmutableContext {
        run_id: Some(123),
        attempt: Some(2),
        head_sha: Some("a".repeat(40)),
        original_repository_v3: Some(ActionsOriginalRepositoryContextV3 {
            repository_id: 10000,
            repository_node: "REPO_10000".into(),
            original_name: "example/original".into(),
        }),
        original_run_v3: Some(ActionsOriginalRunContextV3 {
            run_node: "RUN_123".into(),
            workflow_id: 7,
            check_suite_id: Some(901),
        }),
        ..Default::default()
    };
    let target = ActionsWriteTarget::from_original_context_v3(
        &Scope::github("github.invalid", Some(42)).unwrap(),
        &ActionsTarget::Run {
            repository: "example/current".into(),
            id: 123,
        },
        &context,
    )
    .unwrap();
    let intent = ActionsWriteIntent::new(
        ActionsWriteKind::Cancel,
        context,
        Default::default(),
        b"actual private binding",
    )
    .unwrap();
    (ActionsWriteJournal::open(path).unwrap(), target, intent)
}
fn state(journal: &ActionsWriteJournal) -> (i64, i64, i64) {
    journal.connection.query_row("SELECT (SELECT COUNT(*) FROM actions_write_attempts),COALESCE((SELECT SUM(generation) FROM actions_write_generations),0),(SELECT SUM(claimed) FROM actions_write_attempts)",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap()
}
#[test]
fn scoped_actions_publication_v3_claim_stamp_requires_actual_same_claimed_permit() {
    let temp = directory();
    let (mut journal, target, intent) = setup(&temp.path().join("cache.sqlite3"));
    let permit = journal.begin(&target, &intent).unwrap();
    let before = state(&journal);
    assert!(journal.capture_claim_stamp_v3(&permit).is_err());
    assert_eq!(state(&journal), before);
    journal.claim(&permit, &target, &intent).unwrap();
    let stamp = journal.capture_claim_stamp_v3(&permit).unwrap();
    let claimed = state(&journal);
    journal.validate_claim_stamp_v3(&stamp).unwrap();
    assert_eq!(state(&journal), claimed);
    assert_eq!(
        claimed,
        (1, 6, 1),
        "Three exact old/current/numeric aliases, two generations each"
    );
}
#[test]
fn scoped_actions_publication_v3_claim_stamp_refuses_actual_resource_generation_drift() {
    let temp = directory();
    let (mut journal, target, intent) = setup(&temp.path().join("cache.sqlite3"));
    let permit = journal.begin(&target, &intent).unwrap();
    journal.claim(&permit, &target, &intent).unwrap();
    let stamp = journal.capture_claim_stamp_v3(&permit).unwrap();
    let changed = journal
        .connection
        .execute(
            "UPDATE actions_write_generations SET generation=generation+1 WHERE resource_key=?1",
            [&target.resources[1]],
        )
        .unwrap();
    assert_eq!(changed, 1);
    let before = state(&journal);
    assert!(journal.validate_claim_stamp_v3(&stamp).is_err());
    assert_eq!(state(&journal), before);
}
#[test]
fn scoped_actions_publication_v3_claim_stamp_refuses_claim_state_payload_and_target_edits() {
    for sql in [
        "UPDATE actions_write_attempts SET claimed=0",
        "UPDATE actions_write_attempts SET state='uncertain'",
        "UPDATE actions_write_attempts SET context_json=json_set(context_json,'$.attempt',3)",
        "UPDATE actions_write_attempts SET target_json=json_set(target_json,'$.Run.repository','Example/Current')",
    ] {
        let temp=directory();let (mut journal,target,intent)=setup(&temp.path().join("cache.sqlite3"));
        let permit=journal.begin(&target,&intent).unwrap();journal.claim(&permit,&target,&intent).unwrap();
        let stamp=journal.capture_claim_stamp_v3(&permit).unwrap();
        assert_eq!(journal.connection.execute(sql,[]).unwrap(),1);let before=state(&journal);
        assert!(journal.validate_claim_stamp_v3(&stamp).is_err(),"{sql}");assert_eq!(state(&journal),before);
    }
}
#[test]
fn scoped_actions_publication_v3_claim_stamp_refuses_actual_replaced_path_before_reading_new_db() {
    let temp = directory();
    let path = temp.path().join("cache.sqlite3");
    let (mut journal, target, intent) = setup(&path);
    let permit = journal.begin(&target, &intent).unwrap();
    journal.claim(&permit, &target, &intent).unwrap();
    let stamp = journal.capture_claim_stamp_v3(&permit).unwrap();
    fs::rename(&path, temp.path().join("held-original.sqlite3")).unwrap();
    fs::write(&path, b"NOT_A_SQLITE_DATABASE").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let bytes = fs::read(&path).unwrap();
    assert!(journal.validate_claim_stamp_v3(&stamp).is_err());
    assert_eq!(
        fs::read(&path).unwrap(),
        bytes,
        "No new path parsed/repaired/adopted by validation"
    );
}
#[test]
fn scoped_actions_publication_v3_claim_stamp_cannot_adopt_another_journal_or_old_none_permit() {
    let temp = directory();
    let (mut journal, target, intent) = setup(&temp.path().join("first.sqlite3"));
    let (other, _, _) = setup(&temp.path().join("other.sqlite3"));
    let permit = journal.begin(&target, &intent).unwrap();
    journal.claim(&permit, &target, &intent).unwrap();
    assert!(other.capture_claim_stamp_v3(&permit).is_err());
    let stamp = journal.capture_claim_stamp_v3(&permit).unwrap();
    assert!(other.validate_claim_stamp_v3(&stamp).is_err());
    let old = ActionsImmutableContext {
        run_id: Some(124),
        attempt: Some(2),
        head_sha: Some("a".repeat(40)),
        ..Default::default()
    };
    let old_target = ActionsWriteTarget::from_scope(
        target.scope(),
        &ActionsTarget::Run {
            repository: "example/legacy".into(),
            id: 124,
        },
    )
    .unwrap();
    let old_intent =
        ActionsWriteIntent::new(ActionsWriteKind::Cancel, old, Default::default(), b"legacy")
            .unwrap();
    let old_permit = journal.begin(&old_target, &old_intent).unwrap();
    journal
        .claim(&old_permit, &old_target, &old_intent)
        .unwrap();
    assert!(
        journal.capture_claim_stamp_v3(&old_permit).is_err(),
        "Legacy None behavior is not promoted into scoped source proof"
    );
}
