//! DECLARED/UNRUN. Real SQLite migration/transactions and exact private files;
//! synthetic metadata here tests storage only, never claims a provider receipt.
use super::*;
use std::os::unix::fs::PermissionsExt;
fn setup() -> (
    tempfile::TempDir,
    PrWriteJournal,
    PrWriteTarget,
    ThreadOriginSeedV1,
) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let journal = PrWriteJournal::open(&dir.path().join("cache.sqlite")).unwrap();
    let target = PrWriteTarget::for_source(
        &Scope::github("github.invalid", Some(42)).unwrap(),
        "example/repo",
        "PR_exact",
        7,
    )
    .unwrap();
    let mut seed = ThreadOriginSeedV1 {
        version: 1,
        database: DatabaseV1::capture(&journal.path).unwrap(),
        scope: target.scope.clone(),
        repository_id: 410,
        repository_node: "R_exact".into(),
        repository: target.repository.clone(),
        pr_id: 1007,
        pr_node: target.pr_node_id.clone(),
        number: 7,
        head: "a".repeat(40),
        base: "b".repeat(40),
        thread_node: "THREAD_exact".into(),
        command: "thread-reply".into(),
        observed_resolved: Some(false),
        root: Some(ThreadRootHistoryV1 {
            node: "ROOT_exact".into(),
            rest_id: 9009,
            commit: "c".repeat(40),
            path: "src/界.rs".into(),
        }),
        draft_key: String::new(),
        observed_login: Some("observed-old-name".into()),
    };
    seed.draft_key = seed.expected_draft_key().unwrap();
    (dir, journal, target, seed)
}
fn counts(journal: &PrWriteJournal) -> (i64, i64, i64) {
    journal.connection.query_row("SELECT (SELECT count(*) FROM pr_write_attempts),(SELECT count(*) FROM pr_write_thread_origins_v1),(SELECT COALESCE(sum(generation),0) FROM pr_write_scopes)",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap()
}
#[test]
fn review_threads_v1_history_sql_atomic_metadata_and_attempt_share_one_generation_and_no_second_body(
) {
    let (_dir, mut journal, target, seed) = setup();
    let write = seed.write("PRIVATE_ONLY_IN_EXISTING_DRAFT").unwrap();
    let permit = journal.begin_thread_v1(&target, &write, &seed).unwrap();
    assert_eq!(counts(&journal), (1, 1, 1));
    let encoded: String = journal
        .connection
        .query_row(
            "SELECT origin_json FROM pr_write_thread_origins_v1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!encoded.contains("PRIVATE_ONLY_IN_EXISTING_DRAFT"));
    let record: ThreadOriginRecordV1 = serde_json::from_str(&encoded).unwrap();
    assert_eq!(record.reference, permit.marker().publication_reference());
    assert_eq!(record.seed, seed);
    let snapshot = journal
        .thread_history_v1(&target, 410, "R_exact", 1007)
        .unwrap();
    assert_eq!(snapshot.generation, 1);
    assert_eq!(snapshot.rows[0].marker.state, PrWriteState::Pending);
    journal
        .validate_thread_history_v1(&target, 410, "R_exact", 1007, &snapshot)
        .unwrap();
    assert_eq!(counts(&journal), (1, 1, 1));
    assert!(journal.begin_thread_v1(&target, &write, &seed).is_err());
    assert_eq!(counts(&journal), (1, 1, 1));
}
#[test]
fn review_threads_v1_history_sql_metadata_failure_rolls_back_attempt_generation_and_permit() {
    let (_dir, mut journal, target, seed) = setup();
    journal.connection.execute_batch("CREATE TRIGGER refuse_thread_origin BEFORE INSERT ON pr_write_thread_origins_v1 BEGIN SELECT RAISE(ABORT,'controlled origin failure'); END;").unwrap();
    assert!(journal
        .begin_thread_v1(&target, &seed.write("exact reply").unwrap(), &seed)
        .is_err());
    assert_eq!(counts(&journal), (0, 0, 0));
    assert!(journal.blocking(&target).unwrap().is_none());
    journal
        .connection
        .execute_batch("DROP TRIGGER refuse_thread_origin;")
        .unwrap();
    let permit = journal
        .begin_thread_v1(&target, &seed.write("exact reply").unwrap(), &seed)
        .unwrap();
    assert_eq!(permit.marker().state, PrWriteState::Pending);
    assert_eq!(counts(&journal), (1, 1, 1));
}
#[test]
fn review_threads_v1_history_sql_migration_keeps_protected_legacy_unsupported_and_cleanup_follows_attempt(
) {
    let (dir, mut journal, target, seed) = setup();
    let write = seed.write("legacy private body must not open").unwrap();
    let permit = journal.begin(&target, &write).unwrap();
    let reference = permit.marker().publication_reference();
    drop(journal);
    let raw = Connection::open(dir.path().join("cache.sqlite")).unwrap();
    raw.execute_batch("DROP TRIGGER pr_write_thread_origins_immutable_v1;DROP TRIGGER pr_write_thread_origins_cleanup_v1;DROP TABLE pr_write_thread_origins_v1;").unwrap();
    drop(raw);
    let journal = PrWriteJournal::open(&dir.path().join("cache.sqlite")).unwrap();
    let snapshot = journal
        .thread_history_v1(&target, 410, "R_exact", 1007)
        .unwrap();
    assert_eq!(snapshot.rows[0].marker.publication_reference(), reference);
    assert!(snapshot.rows[0].origin.is_none());
    assert_eq!(counts(&journal), (1, 0, 1));
    drop(journal);
    let mut journal = PrWriteJournal::open(&dir.path().join("cache.sqlite")).unwrap();
    assert_eq!(counts(&journal), (1, 0, 1));
    journal
        .connection
        .execute("DELETE FROM pr_write_attempts", [])
        .unwrap();
    let permit = journal.begin_thread_v1(&target, &write, &seed).unwrap();
    assert!(journal
        .connection
        .execute("UPDATE pr_write_thread_origins_v1 SET origin_json='{}'", [])
        .is_err());
    journal
        .connection
        .execute(
            "DELETE FROM pr_write_attempts WHERE attempt_id=?1",
            [permit.marker().attempt_id],
        )
        .unwrap();
    assert_eq!(counts(&journal), (0, 0, 2));
}
#[test]
fn review_threads_v1_history_sql_same_path_replaced_db_foreign_uid_repo_pr_and_fingerprint_refuse()
{
    let (_dir, mut journal, target, seed) = setup();
    let write = seed.write("private").unwrap();
    journal.begin_thread_v1(&target, &write, &seed).unwrap();
    assert!(journal
        .thread_history_v1(&target, 411, "R_exact", 1007)
        .is_err());
    assert!(journal
        .thread_history_v1(&target, 410, "R_recreated_same_numeric_id", 1007)
        .is_err());
    assert!(journal
        .thread_history_v1(&target, 410, "R_exact", 1008)
        .is_err());
    let foreign = PrWriteTarget::for_source(
        &Scope::github("github.invalid", Some(43)).unwrap(),
        "example/repo",
        "PR_exact",
        7,
    )
    .unwrap();
    assert!(journal
        .thread_history_v1(&foreign, 410, "R_exact", 1007)
        .unwrap()
        .rows
        .is_empty());
    let mut changed = write.clone();
    changed.values.insert("root-id".into(), "9010".into());
    let before = counts(&journal);
    assert!(journal.begin_thread_v1(&target, &changed, &seed).is_err());
    assert_eq!(counts(&journal), before);
    let snapshot = journal
        .thread_history_v1(&target, 410, "R_exact", 1007)
        .unwrap();
    journal
        .connection
        .execute(
            "UPDATE pr_write_attempts SET payload_sha256=?1",
            ["f".repeat(64)],
        )
        .unwrap();
    assert!(journal
        .validate_thread_history_v1(&target, 410, "R_exact", 1007, &snapshot)
        .is_err());
    let replacement = journal.path.with_file_name("replacement.sqlite");
    drop(Cache::open(&replacement).unwrap());
    std::fs::rename(&replacement, &journal.path).unwrap();
    assert!(journal
        .thread_history_v1(&target, 410, "R_exact", 1007)
        .is_err());
}
#[test]
fn review_threads_v1_history_sql_utf8_record_and_unknown_fields_are_bounded_before_materialization()
{
    let (_dir, mut journal, target, mut seed) = setup();
    seed.observed_login = Some("\u{1}".repeat(4096));
    let write = seed.write("private").unwrap();
    assert!(journal.begin_thread_v1(&target, &write, &seed).is_err());
    assert_eq!(counts(&journal), (0, 0, 0));
    seed.observed_login = Some("actor".into());
    journal
        .begin_thread_v1(&target, &seed.write("private").unwrap(), &seed)
        .unwrap();
    journal
        .connection
        .execute_batch("DROP TRIGGER pr_write_thread_origins_immutable_v1;")
        .unwrap();
    journal
        .connection
        .execute(
            "UPDATE pr_write_thread_origins_v1 SET origin_json=?1",
            ["界".repeat(6000)],
        )
        .unwrap();
    assert!(journal
        .thread_history_v1(&target, 410, "R_exact", 1007)
        .is_err());
    let refvalue = journal
        .connection
        .query_row("SELECT attempt_id FROM pr_write_attempts", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap();
    let record = ThreadOriginRecordV1 {
        seed,
        reference: PrPublicationReference {
            attempt_id: refvalue,
            pr_node_id: target.node_id().into(),
            observed_head: "a".repeat(40),
            payload_sha256: "f".repeat(64),
        },
    };
    let mut value = serde_json::to_value(record).unwrap();
    value["unexpected_permission"] = serde_json::json!(true);
    journal
        .connection
        .execute(
            "UPDATE pr_write_thread_origins_v1 SET origin_json=?1",
            [value.to_string()],
        )
        .unwrap();
    assert!(journal
        .thread_history_v1(&target, 410, "R_exact", 1007)
        .is_err());
}
#[test]
fn review_threads_v1_history_sql_snapshot_requires_exact_state_generation_origin_and_latest_membership(
) {
    let (_dir, mut journal, target, seed) = setup();
    let write = seed.write("private").unwrap();
    let permit = journal.begin_thread_v1(&target, &write, &seed).unwrap();
    let first = journal
        .thread_history_v1(&target, 410, "R_exact", 1007)
        .unwrap();
    journal.mark_uncertain(&permit).unwrap();
    assert!(journal
        .validate_thread_history_v1(&target, 410, "R_exact", 1007, &first)
        .is_err());
    let current = journal
        .thread_history_v1(&target, 410, "R_exact", 1007)
        .unwrap();
    assert_eq!(current.rows[0].marker.state, PrWriteState::Uncertain);
    journal
        .validate_thread_history_v1(&target, 410, "R_exact", 1007, &current)
        .unwrap();
    assert_eq!(
        current.rows[0]
            .origin
            .as_ref()
            .unwrap()
            .seed
            .root
            .as_ref()
            .unwrap()
            .commit,
        "c".repeat(40)
    );
    assert_ne!(
        current.rows[0].origin.as_ref().unwrap().seed.head,
        "c".repeat(40)
    );
    assert!(journal.begin(&target, &write).is_err());
}
#[test]
fn review_threads_v1_history_sql_latest_rows_are_bounded_with_explicit_unknown_total_not_false_full_enumeration(
) {
    let (_dir, mut journal, target, seed) = setup();
    for index in 0..65 {
        let write = seed.write(&format!("old exact body {index}")).unwrap();
        let permit = journal.begin_thread_v1(&target, &write, &seed).unwrap();
        journal.claim(&permit, &target, &write).unwrap();
        journal
            .mark_confirmed(
                &permit,
                &PrConfirmedEffect {
                    kind: PrEffectKind::Comment,
                    resource_id: (9000 + index).to_string(),
                    node_id: Some(format!("REPLY_{index}")),
                },
            )
            .unwrap();
    }
    let snapshot = journal
        .thread_history_v1(&target, 410, "R_exact", 1007)
        .unwrap();
    assert_eq!(snapshot.rows.len(), THREAD_HISTORY_ROWS_V1);
    assert_eq!(snapshot.total, None);
    assert_eq!(snapshot.rows.first().unwrap().marker.attempt_id, 65);
    assert_eq!(snapshot.rows.last().unwrap().marker.attempt_id, 2);
    assert_eq!(counts(&journal), (65, 65, 195));
    journal
        .validate_thread_history_v1(&target, 410, "R_exact", 1007, &snapshot)
        .unwrap();
    assert_eq!(counts(&journal), (65, 65, 195));
}
