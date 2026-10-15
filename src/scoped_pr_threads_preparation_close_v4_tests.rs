//! Actual source-produced body/preparation boundaries. A Clear or original
//! token cancel at this boundary is real, not a substituted worker response.
//! Declarations only / UNRUN until Root executes the serialized gates.
use super::*;

type ClearSqlV5 = Vec<(String, String, Vec<Vec<rusqlite::types::Value>>)>;

/// Read-only prediction from the BEFORE database. Never calls Clear, writes
/// SQL, or accepts any part of the resulting database as an expected baseline.
/// Every table/column is retained, including drafts, receipts, foreign scopes,
/// lookup epochs and mutation tombstones which the older four-query witness
/// did not cover.
fn clear_sql_v5(fixture: &Fixture, scope: &Scope, predict_clear: bool) -> ClearSqlV5 {
    let db = sql(fixture);
    let tables: Vec<(String, String)> = db
        .prepare("SELECT name,sql FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    tables.into_iter().map(|(table, schema)| {
        let quoted = format!("\"{}\"", table.replace('"', "\"\""));
        let columns: Vec<String> = db.prepare(&format!("PRAGMA table_info({quoted})")).unwrap()
            .query_map([], |row| row.get(1)).unwrap().map(Result::unwrap).collect();
        let target = "t.host=?1 AND t.login=?2";
        let expressions = columns.iter().map(|column| {
            let name = format!("t.\"{}\"", column.replace('"', "\"\""));
            if !predict_clear { return name; }
            match (table.as_str(), column.as_str()) {
                ("sync_reservations", "generation") => format!("CASE WHEN {target} THEN CASE WHEN {name}<9223372036854775807 THEN {name}+1 ELSE {name} END ELSE {name} END"),
                ("sync_reservations" | "sync_resource_bindings", "active") => format!("CASE WHEN {target} THEN 0 ELSE {name} END"),
                ("sync_resource_bindings", "visible_generation") => format!("CASE WHEN {target} THEN NULL ELSE {name} END"),
                ("sync_binding_lookup_epochs", "generation") => format!("{name} + CASE WHEN {target} THEN COALESCE((SELECT SUM(
                    2 + 2*(SELECT COUNT(*) FROM sync_reservations r WHERE r.host=b.host AND r.login=b.login AND r.source=b.source)
                    + (SELECT COUNT(*) FROM checkpoints c WHERE c.host=b.host AND c.login=b.login AND c.source=b.source)
                    + (SELECT COUNT(*) FROM records r WHERE r.host=b.host AND r.login=b.login AND r.kind='collaboration-state-v1' AND r.id=b.source)
                    + (SELECT COUNT(*) FROM checkpoint_records m WHERE m.host=b.host AND m.login=b.login AND m.kind='collaboration-state-v1' AND m.id=b.source)
                  ) FROM sync_resource_bindings b WHERE b.host=t.host AND b.login=t.login AND b.namespace=t.namespace AND b.repository=t.repository),0) ELSE 0 END"),
                _ => name,
            }
        }).collect::<Vec<_>>().join(",");
        // Clear deletes the actual scope's source records/checkpoints and the
        // corresponding FK membership rows. It does not delete any other data.
        let filter = if predict_clear && matches!(table.as_str(), "records" | "checkpoints" | "checkpoint_records") {
            format!("NOT ({target})")
        } else { "?1 IS NOT NULL AND ?2 IS NOT NULL".into() };
        let order = (1..=columns.len()).map(|n| n.to_string()).collect::<Vec<_>>().join(",");
        let mut statement = db.prepare(&format!("SELECT {expressions} FROM {quoted} t WHERE {filter} ORDER BY {order}")).unwrap();
        let rows = statement.query_map(rusqlite::params![scope.host,scope.login], |row| {
            (0..columns.len()).map(|index| row.get::<_,rusqlite::types::Value>(index)).collect::<rusqlite::Result<Vec<_>>>()
        }).unwrap().map(Result::unwrap).collect();
        (table,schema,rows)
    }).collect()
}

fn seed_clear_witnesses_v5(real: &Real, scope: &Scope) {
    use crate::storage::{Record, SyncBinding, SyncBindingKind, SyncNamespace};
    let foreign = Scope::github(HOST, Some(97)).unwrap();
    let mut cache = Cache::open(&database(&real.fixture)).unwrap();
    for (index, owner) in [scope, &foreign].into_iter().enumerate() {
        let source = format!(
            "collaboration-v1:{}",
            if index == 0 { "b" } else { "c" }.repeat(64)
        );
        let resource = format!("CLEAR_TEST_OPAQUE_PARENT_{index}");
        let generation = cache
            .reserve_resource_sync(
                owner,
                &source,
                &SyncBinding {
                    namespace: SyncNamespace::Collaboration,
                    repository_id: 410,
                    repository: REPO.into(),
                    resource: resource.clone(),
                    kind: SyncBindingKind::InlineComment,
                },
            )
            .unwrap();
        assert!(cache
            .commit_reserved_sync(
                owner,
                &source,
                generation,
                &[Record::new(
                    "collaboration-state-v1",
                    &source,
                    REPO,
                    json!({"clear_witness":index})
                )],
                true
            )
            .unwrap());
        cache
            .put_draft(owner, "clear-retained-draft", "PRIVATE_CLEAR_DRAFT_WITNESS")
            .unwrap();
        cache
            .save_view(owner, "clear-retained-preference", &json!({"scope":index}))
            .unwrap();
        let target =
            crate::pr_write_journal::PrWriteTarget::for_source(owner, REPO, &resource, 7).unwrap();
        let intent = crate::pr_write_journal::PrWriteIntent {
            command: "thread-clear-test-observation".into(),
            selected_resource: resource.clone(),
            observed_head: "a".repeat(40),
            values: Default::default(),
        };
        crate::pr_write_journal::PrWriteJournal::open(&database(&real.fixture))
            .unwrap()
            .begin(&target, &intent)
            .unwrap();
    }
    // Nonzero tombstones are independent fixtures, not a call to the real
    // fence (which would invalidate the source before the preparation hook).
    let db = Connection::open(database(&real.fixture)).unwrap();
    for owner in [scope, &foreign] {
        db.execute("INSERT INTO sync_mutation_fences VALUES(?1,?2,'collaboration',?3,'CLEAR_TEST_PRIOR_TOMBSTONE','inline_comment',7)",rusqlite::params![owner.host,owner.login,REPO]).unwrap();
    }
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM pr_write_attempts", [], |row| row
            .get::<_, u64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM sync_mutation_fences", [], |row| row
            .get::<_, u64>(
            0
        ))
        .unwrap(),
        2
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM sync_resource_bindings WHERE visible_generation IS NOT NULL",
            [],
            |row| row.get::<_, u64>(0)
        )
        .unwrap(),
        2
    );
}

fn clear_protocol_intact_v5(real: &Real) {
    assert_eq!(git_state_resource(&real.fixture), real.git);
    let rows: Vec<Value> = fs::read_to_string(real.fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|row| serde_json::from_str(row).unwrap())
        .collect();
    assert_eq!(rows.len(), real.steps.len());
    for (row, step) in rows.iter().zip(&real.steps) {
        assert_eq!(row["matched"], true);
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(
            row["stdin_hex"],
            step.stdin
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        if step.args.iter().any(|arg| arg == "POST") {
            assert_eq!(step.args[1], "graphql");
            let body: Value = serde_json::from_slice(&step.stdin).unwrap();
            assert!(body["query"].as_str().unwrap().starts_with("query("));
            assert!(!body["query"].as_str().unwrap().contains("mutation"));
        }
    }
}
#[test]
fn review_threads_v4_original_browser_clear_after_real_preparation_refuses_private_return() {
    // Genuine inner metrics read+its final guards really complete (13 children).
    // Once Clear occurs, not even the final outer raw-parent reads may start.
    // Offer the unchanged full original17 children: under the old ordering the
    // real Clear would run after all17 and private rows would return. The
    // corrected operation must stop at the exact first13 actual children.
    let mut real = real(thread_steps(Some("LINE")));
    let original = threads::ThreadBrowseIntentV1::capture(&real.app).unwrap();
    let pointer = Arc::as_ptr(&original);
    let scope = original.scope().clone();
    seed_clear_witnesses_v5(&real, &scope);
    let expected = clear_sql_v5(&real.fixture, &scope, true);
    assert_ne!(expected, clear_sql_v5(&real.fixture, &scope, false));
    let config = real.fixture.root.join("config/ghtui/clear-test.toml");
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    fs::write(&config, b"# private Clear fixture configuration\n").unwrap();
    let config_before = fs::read(&config).unwrap();
    let git_config = fs::read(real.fixture.repo().join(".git/config")).unwrap();
    let path = database(&real.fixture);
    let called = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = called.clone();
    let cleared_scope = scope.clone();
    let original = threads::after_preparation_for_test(
        original,
        Arc::new(move || {
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Cache::open(&path)
                .unwrap()
                .clear_records(&cleared_scope)
                .unwrap();
        }),
    );
    assert_eq!(Arc::as_ptr(&original), pointer);
    assert!(matches!(
        threads::load(
            original,
            &provider(&real.fixture),
            &database(&real.fixture),
            &Default::default(),
            Instant::now() + Duration::from_secs(30)
        ),
        Err(threads::ThreadBrowseFailureV1::Source)
    ));
    assert_eq!(called.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(clear_sql_v5(&real.fixture,&scope,false),expected,"Only the independently predicted scoped Clear/cascade/lookup-trigger effects are allowed; drafts, receipts, foreign rows and tombstones must be exact");
    assert_eq!(fs::read(config).unwrap(), config_before);
    assert_eq!(
        fs::read(real.fixture.repo().join(".git/config")).unwrap(),
        git_config
    );
    real.steps.truncate(real.steps.len() - 4);
    clear_protocol_intact_v5(&real);
}
#[test]
fn review_threads_v4_original_browser_canceled_same_token_after_preparation_cannot_close_or_publish(
) {
    let mut real = real(thread_steps(Some("LINE")));
    let token = crate::providers::runner::CancellationToken::default();
    let actual = token.clone();
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let called = calls.clone();
    let intent = threads::after_preparation_for_test(
        threads::ThreadBrowseIntentV1::capture(&real.app).unwrap(),
        Arc::new(move || {
            called.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            actual.cancel();
        }),
    );
    assert!(matches!(
        threads::load(
            intent,
            &provider(&real.fixture),
            &database(&real.fixture),
            &token,
            Instant::now() + Duration::from_secs(30)
        ),
        Err(threads::ThreadBrowseFailureV1::Source)
    ));
    assert!(token.is_cancelled());
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    real.steps.truncate(real.steps.len() - 4);
    intact(&real);
}
#[test]
fn review_threads_v4_original_browser_prepare_before_closing_retains_actual_final_actor_and_protocol(
) {
    let mut steps = thread_steps(Some("LINE"));
    steps[16] = get(
        "user",
        json!({"id":42,"login":"actual-final-renamed-viewer"}),
    );
    let real = real(steps);
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let called = calls.clone();
    let intent = threads::after_preparation_for_test(
        threads::ThreadBrowseIntentV1::capture(&real.app).unwrap(),
        Arc::new(move || {
            called.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }),
    );
    let observed = threads::load(
        intent.clone(),
        &provider(&real.fixture),
        &database(&real.fixture),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    assert!(Arc::ptr_eq(&observed.intent, &intent));
    assert_eq!(observed.identity.login, "actual-final-renamed-viewer");
    assert_eq!(observed.identity.account_id, Some(42));
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(observed
        .groups
        .iter()
        .flat_map(|group| group.lines.iter())
        .any(|line| line.text.contains("Original private thread body")));
    intact(&real);
}
