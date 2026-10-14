//! SOURCE-ONLY strict gh + real file-backed SQLite contracts. UNREGISTERED and
//! UNRUN until Root's included-source/Cargo activation window.
use super::*;
use crate::storage::Scope;
use crate::test_support as support;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    thread,
};
use support::{Fixture, GhStep};

fn get(endpoint: &str, value: Value) -> GhStep {
    GhStep::read(
        &[
            "api",
            endpoint,
            "--hostname",
            "github.invalid",
            "--method",
            "GET",
        ],
        &value.to_string(),
    )
}
fn principal(uid: u64, login: &str) -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.99.0 (2026-10-01)\n"),
        get("user", json!({"id":uid,"login":login})),
    ]
}
fn catalogue(owner_id: u64, node: &str) -> Vec<GhStep> {
    let owner = json!({"id":owner_id,"node_id":node,"login":"example","type":"Organization"});
    let mut steps = principal(42, "octocat");
    steps.push(get("orgs/example", owner.clone()));
    steps.push(get("orgs/example/repos?type=all&sort=full_name&direction=asc&per_page=100&page=1",
        json!([{"id":101,"node_id":"R_original_101","full_name":"example/private-original","owner":owner,"private":true,"permissions":{"pull":true}}])));
    steps.push(get("orgs/example", owner));
    steps.extend(principal(42, "octocat"));
    steps
}
fn provider(fixture: &Fixture) -> GithubProvider {
    GithubProvider::new("github.invalid")
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"))
}
fn database(fixture: &Fixture) -> PathBuf {
    fixture.root.join("cache.sqlite3")
}
fn load(fixture: &Fixture, cache: &mut Cache) -> CatalogueIntentSource {
    load_catalogue_intent_source(
        &provider(fixture),
        cache,
        None,
        &RepositoryCatalogueSource::Organization("example".into()),
        &RepositoryCatalogueLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap()
}
fn candidate(source: &CatalogueIntentSource) -> CatalogueIntentV1 {
    let mut intent = source.intent().clone();
    intent.set(source.source(), "focused", &[101]).unwrap();
    intent.pin(source.source(), 101, true).unwrap();
    intent
}
fn scope(uid: u64) -> Scope {
    Scope::github("github.invalid", Some(uid)).unwrap()
}
fn saved(cache: &Cache, source: &CatalogueIntentSource) -> Option<Value> {
    cache
        .saved_view_bounded(
            source.scope(),
            source.saved_view_key(),
            crate::repository_catalogue_intent_v1::INTENT_MAX_BYTES,
        )
        .unwrap()
}
fn commands(fixture: &Fixture, count: usize) {
    let rows = journal(&fixture.root.join("gh-fixture"));
    assert_eq!(rows.len(), count);
    assert!(rows
        .iter()
        .all(|row| row["matched"] == true && row["stdin_hex"] == ""));
    assert!(rows
        .iter()
        .all(|row| row["args"][0] == "--version"
            || (row["args"][0] == "api" && row["args"][5] == "GET")));
    assert!(!rows.iter().any(
        |row| row["args"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value == "--paginate"
                || value == "--slurp"
                || value == "POST"
                || value == "PATCH"
                || value == "DELETE")
    ));
}
fn journal(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path.join("commands.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}
fn wait_step(path: &Path, step: usize) {
    let until = Instant::now() + Duration::from_secs(3);
    while !journal(path).iter().any(|row| row["step"] == step) {
        assert!(
            Instant::now() < until,
            "Actual command journal step{step} was not reached"
        );
        thread::yield_now();
    }
}
fn assert_boundary(error: CatalogueSaveFailure, expected: CatalogueIntentBoundaryError) {
    assert!(
        matches!(error.reason, CatalogueSaveReason::Boundary(reason) if reason == expected),
        "{error:?}"
    );
    assert!(!error.original_intent_committed);
    assert!(!error.to_string().contains("private-original"));
}

#[test]
fn repo_catalog_save_genuine_source_four_read_rename_and_exact_original_intent() {
    let fixture = Fixture::new();
    let mut steps = catalogue(71, "OWNER_save_71");
    steps.extend(principal(42, "renamed-login"));
    steps.extend(principal(42, "another-rename"));
    fixture.scripted_gh(&steps);
    let mut cache = Cache::open(&database(&fixture)).unwrap();
    cache
        .put_draft(
            &scope(42),
            "original-review",
            "Draft must remain original42",
        )
        .unwrap();
    let source = load(&fixture, &mut cache);
    let generation = cache
        .sync_reservation(source.scope(), source.sync_source())
        .unwrap();
    let updated = save_catalogue_intent(
        &provider(&fixture),
        &mut cache,
        &source,
        candidate(&source),
        Duration::from_secs(5),
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(Arc::ptr_eq(source.source(), updated.source()));
    assert_eq!(
        cache
            .sync_reservation(source.scope(), source.sync_source())
            .unwrap(),
        generation,
        "Save MUST NOT reserve/rebase a new source generation"
    );
    let value = saved(&cache, &source).unwrap();
    assert_eq!(value["sets"]["focused"]["101"]["node_id"], "R_original_101");
    assert_eq!(
        value["sets"]["focused"]["101"]["last_name"],
        "example/private-original"
    );
    assert_eq!(value["source"]["owner_id"], 71);
    assert_eq!(value["scope"]["login"], "user-id:42");
    assert_eq!(updated.intent().to_value().unwrap(), value);
    assert_eq!(
        cache
            .draft(&scope(42), "original-review")
            .unwrap()
            .as_deref(),
        Some("Draft must remain original42")
    );
    assert!(cache
        .saved_view_bounded(&scope(43), source.saved_view_key(), 128 * 1024)
        .unwrap()
        .is_none());
    commands(&fixture, 11);
}

#[test]
fn repo_catalog_save_final_reused_login_uid_switch_hides_ack_preserves_original_commit() {
    let fixture = Fixture::new();
    let mut steps = catalogue(71, "OWNER_save_71");
    steps.extend(principal(42, "octocat"));
    steps.extend(principal(43, "octocat"));
    fixture.scripted_gh(&steps);
    let mut cache = Cache::open(&database(&fixture)).unwrap();
    cache.put_draft(&scope(42), "review", "original42").unwrap();
    let source = load(&fixture, &mut cache);
    let error = save_catalogue_intent(
        &provider(&fixture),
        &mut cache,
        &source,
        candidate(&source),
        Duration::from_secs(5),
        &CancellationToken::default(),
    )
    .unwrap_err();
    assert!(matches!(error.reason, CatalogueSaveReason::IdentityChanged));
    assert!(error.original_intent_committed, "No returned acknowledgement/model, but original local write must not be resent or copied to43");
    assert_eq!(
        saved(&cache, &source).unwrap()["sets"]["focused"]["101"]["id"],
        101
    );
    assert_eq!(
        cache.draft(&scope(42), "review").unwrap().as_deref(),
        Some("original42")
    );
    assert!(cache
        .saved_view_bounded(&scope(43), source.saved_view_key(), 128 * 1024)
        .unwrap()
        .is_none());
    assert!(cache.draft(&scope(43), "review").unwrap().is_none());
    commands(&fixture, 11);
}

#[test]
fn repo_catalog_save_pre_principal_switch_never_writes_or_guesses_login_scope() {
    for uid in [0, 43] {
        let fixture = Fixture::new();
        let mut steps = catalogue(71, "OWNER_save_71");
        steps.extend(principal(uid, "octocat"));
        fixture.scripted_gh(&steps);
        let mut cache = Cache::open(&database(&fixture)).unwrap();
        let source = load(&fixture, &mut cache);
        let error = save_catalogue_intent(
            &provider(&fixture),
            &mut cache,
            &source,
            candidate(&source),
            Duration::from_secs(5),
            &CancellationToken::default(),
        )
        .unwrap_err();
        assert!(matches!(error.reason, CatalogueSaveReason::IdentityChanged));
        assert!(!error.original_intent_committed);
        assert!(saved(&cache, &source).is_none());
        let count: u64 = Connection::open(database(&fixture))
            .unwrap()
            .query_row("SELECT COUNT(*) FROM saved_views", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
        commands(&fixture, 9);
    }
}

#[test]
fn repo_catalog_save_final_authentication_failure_preserves_intent_without_private_diagnostic() {
    let fixture = Fixture::new();
    let mut steps = catalogue(71, "OWNER_save_71");
    steps.extend(principal(42, "octocat"));
    let mut closing = principal(42, "octocat");
    closing[1].status = 1;
    closing[1].stdout.clear();
    closing[1].stderr = b"gh: private-original token-user diagnostic (HTTP 401)\n".to_vec();
    steps.extend(closing);
    fixture.scripted_gh(&steps);
    let mut cache = Cache::open(&database(&fixture)).unwrap();
    let source = load(&fixture, &mut cache);
    let error = save_catalogue_intent(
        &provider(&fixture),
        &mut cache,
        &source,
        candidate(&source),
        Duration::from_secs(5),
        &CancellationToken::default(),
    )
    .unwrap_err();
    assert!(matches!(
        error.reason,
        CatalogueSaveReason::Principal(GithubErrorKind::Authentication)
    ));
    assert!(error.original_intent_committed);
    assert!(!format!("{error:?} {error}").contains("private-original"));
    assert!(saved(&cache, &source).is_some());
    commands(&fixture, 11);
}

#[test]
fn repo_catalog_save_real_concurrent_clear_revoke_or_reservation_cannot_rebase_source() {
    for mutation in 0..3 {
        let fixture = Fixture::new();
        let mut steps = catalogue(71, "OWNER_save_71");
        steps.extend(principal(42, "octocat"));
        steps.extend(principal(42, "octocat"));
        steps.extend(principal(42, "octocat"));
        fixture.scripted_gh(&steps);
        let mut cache = Cache::open(&database(&fixture)).unwrap();
        let original = load(&fixture, &mut cache);
        let source = save_catalogue_intent(
            &provider(&fixture),
            &mut cache,
            &original,
            candidate(&original),
            Duration::from_secs(5),
            &CancellationToken::default(),
        )
        .unwrap();
        let before = saved(&cache, &source);
        let mut second = Cache::open(&database(&fixture)).unwrap();
        match mutation {
            0 => second.clear_records(source.scope()).unwrap(),
            1 => {
                second
                    .revoke_repository(source.scope(), "example/private-original")
                    .unwrap();
            }
            _ => {
                second
                    .reserve_sync_generation(source.scope(), source.sync_source())
                    .unwrap();
            }
        }
        assert_boundary(
            save_catalogue_intent(
                &provider(&fixture),
                &mut cache,
                &source,
                candidate(&source),
                Duration::from_secs(5),
                &CancellationToken::default(),
            )
            .unwrap_err(),
            CatalogueIntentBoundaryError::OriginalSource,
        );
        assert_eq!(saved(&cache, &source), before);
        commands(&fixture, 13);
    }
}

#[test]
fn repo_catalog_save_original_exact_checkpoint_tuple_change_refuses_even_same_reservation() {
    let fixture = Fixture::new();
    let mut steps = catalogue(71, "OWNER_save_71");
    steps.extend(principal(42, "octocat"));
    fixture.scripted_gh(&steps);
    let mut cache = Cache::open(&database(&fixture)).unwrap();
    let source = load(&fixture, &mut cache);
    let generation = cache
        .sync_reservation(source.scope(), source.sync_source())
        .unwrap()
        .unwrap();
    let second = Connection::open(database(&fixture)).unwrap();
    second
        .execute(
            "INSERT INTO checkpoints VALUES(?1,?2,?3,?4,1234,0)",
            params![
                source.scope().host,
                source.scope().login,
                source.sync_source(),
                generation
            ],
        )
        .unwrap();
    assert_boundary(
        save_catalogue_intent(
            &provider(&fixture),
            &mut cache,
            &source,
            candidate(&source),
            Duration::from_secs(5),
            &CancellationToken::default(),
        )
        .unwrap_err(),
        CatalogueIntentBoundaryError::OriginalSource,
    );
    assert!(saved(&cache, &source).is_none());
    commands(&fixture, 9);
}

#[test]
fn repo_catalog_save_exact_saved_view_compare_and_swap_preserves_concurrent_user_edit() {
    let fixture = Fixture::new();
    let mut steps = catalogue(71, "OWNER_save_71");
    steps.extend(principal(42, "octocat"));
    fixture.scripted_gh(&steps);
    let mut cache = Cache::open(&database(&fixture)).unwrap();
    let source = load(&fixture, &mut cache);
    let mut user_edit = source.intent().clone();
    user_edit
        .set(source.source(), "concurrent-edit", &[101])
        .unwrap();
    let value = user_edit.to_value().unwrap();
    Cache::open(&database(&fixture))
        .unwrap()
        .save_view(source.scope(), source.saved_view_key(), &value)
        .unwrap();
    assert_boundary(
        save_catalogue_intent(
            &provider(&fixture),
            &mut cache,
            &source,
            candidate(&source),
            Duration::from_secs(5),
            &CancellationToken::default(),
        )
        .unwrap_err(),
        CatalogueIntentBoundaryError::SavedIntentChanged,
    );
    assert_eq!(saved(&cache, &source), Some(value));
    commands(&fixture, 9);
}

#[test]
fn repo_catalog_save_exact_key_only_ignores_unbounded_foreign_payloads_without_scanning() {
    let fixture = Fixture::new();
    let mut steps = catalogue(71, "OWNER_save_71");
    steps.extend(principal(42, "octocat"));
    steps.extend(principal(42, "octocat"));
    fixture.scripted_gh(&steps);
    let mut cache = Cache::open(&database(&fixture)).unwrap();
    let sql = Connection::open(database(&fixture)).unwrap();
    sql.execute("INSERT INTO saved_views VALUES('github.invalid','user-id:42','not-catalogue',CAST(zeroblob(4194304) AS TEXT))", []).unwrap();
    sql.execute("INSERT INTO saved_views VALUES('github.invalid','user-id:43','foreign','private invalid JSON must never parse')", []).unwrap();
    let source = load(&fixture, &mut cache);
    save_catalogue_intent(
        &provider(&fixture),
        &mut cache,
        &source,
        candidate(&source),
        Duration::from_secs(5),
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(saved(&cache, &source).is_some());
    assert_eq!(
        sql.query_row(
            "SELECT length(CAST(payload AS BLOB)) FROM saved_views WHERE name='not-catalogue'",
            [],
            |row| row.get::<_, u64>(0)
        )
        .unwrap(),
        4194304
    );
    commands(&fixture, 11);
}

#[test]
fn repo_catalog_save_oversized_exact_payload_and_invalid_wire_never_parse_default_or_bump_epoch() {
    for oversized in [false, true] {
        let fixture = Fixture::new();
        let mut steps = catalogue(71, "OWNER_save_71");
        steps.extend(catalogue(71, "OWNER_save_71").into_iter().take(3));
        fixture.scripted_gh(&steps);
        let mut cache = Cache::open(&database(&fixture)).unwrap();
        let source = load(&fixture, &mut cache);
        let before = cache
            .sync_reservation(source.scope(), source.sync_source())
            .unwrap();
        let sql = Connection::open(database(&fixture)).unwrap();
        let raw = if oversized {
            "界".repeat(128 * 1024)
        } else {
            "private-invalid-wire".into()
        };
        sql.execute(
            "INSERT INTO saved_views VALUES(?1,?2,?3,?4)",
            params![
                source.scope().host,
                source.scope().login,
                source.saved_view_key(),
                raw
            ],
        )
        .unwrap();
        let error = load_catalogue_intent_source(
            &provider(&fixture),
            &mut cache,
            None,
            &RepositoryCatalogueSource::Organization("example".into()),
            &RepositoryCatalogueLimits::default(),
            &CancellationToken::default(),
        )
        .unwrap_err();
        assert_boundary(error, CatalogueIntentBoundaryError::SavedIntentInvalid);
        assert_eq!(
            cache
                .sync_reservation(source.scope(), source.sync_source())
                .unwrap(),
            before,
            "Failed reservation must roll back, not rebase original source"
        );
        assert_eq!(
            sql.query_row(
                "SELECT length(CAST(payload AS BLOB)) FROM saved_views WHERE name=?1",
                [source.saved_view_key()],
                |row| row.get::<_, usize>(0)
            )
            .unwrap(),
            raw.len()
        );
        commands(&fixture, 10);
    }
}

#[test]
fn repo_catalog_save_identical_copied_database_or_replaced_inode_never_reissues_original_token() {
    for replaced in [false, true] {
        let fixture = Fixture::new();
        let mut steps = catalogue(71, "OWNER_save_71");
        steps.extend(principal(42, "octocat"));
        fixture.scripted_gh(&steps);
        let mut cache = Cache::open(&database(&fixture)).unwrap();
        let source = load(&fixture, &mut cache);
        let next = if replaced {
            let previous = fixture.root.join("previous-original.sqlite3");
            std::fs::rename(database(&fixture), &previous).unwrap();
            std::fs::copy(&previous, database(&fixture)).unwrap();
            database(&fixture)
        } else {
            let next = fixture.root.join("identical-copy.sqlite3");
            std::fs::copy(database(&fixture), &next).unwrap();
            next
        };
        let mut other = Cache::open(&next).unwrap();
        assert_eq!(
            other
                .sync_reservation(source.scope(), source.sync_source())
                .unwrap(),
            cache
                .sync_reservation(source.scope(), source.sync_source())
                .unwrap()
        );
        assert_boundary(
            save_catalogue_intent(
                &provider(&fixture),
                &mut other,
                &source,
                candidate(&source),
                Duration::from_secs(5),
                &CancellationToken::default(),
            )
            .unwrap_err(),
            CatalogueIntentBoundaryError::OriginalSource,
        );
        assert!(saved(&other, &source).is_none());
        commands(&fixture, 9);
    }
}

#[test]
fn repo_catalog_save_same_original_file_reopen_retains_cas_without_new_reservation() {
    let fixture = Fixture::new();
    let mut steps = catalogue(71, "OWNER_save_71");
    steps.extend(principal(42, "renamed-login"));
    steps.extend(principal(42, "renamed-login"));
    fixture.scripted_gh(&steps);
    let mut cache = Cache::open(&database(&fixture)).unwrap();
    let source = load(&fixture, &mut cache);
    let generation = cache
        .sync_reservation(source.scope(), source.sync_source())
        .unwrap();
    drop(cache);
    let mut reopened = Cache::open(&database(&fixture)).unwrap();
    save_catalogue_intent(
        &provider(&fixture),
        &mut reopened,
        &source,
        candidate(&source),
        Duration::from_secs(5),
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(
        reopened
            .sync_reservation(source.scope(), source.sync_source())
            .unwrap(),
        generation
    );
    assert!(saved(&reopened, &source).is_some());
    commands(&fixture, 11);
}

#[test]
fn repo_catalog_save_forged_new_reference_cannot_use_name_node_or_numeric_alias() {
    for mutation in 0..4 {
        let fixture = Fixture::new();
        let mut steps = catalogue(71, "OWNER_save_71");
        steps.extend(principal(42, "octocat"));
        fixture.scripted_gh(&steps);
        let mut cache = Cache::open(&database(&fixture)).unwrap();
        let source = load(&fixture, &mut cache);
        let mut value = candidate(&source).to_value().unwrap();
        match mutation {
            0 => {
                value["sets"]["focused"] = json!({"102":{"id":102,"node_id":"R_unobserved_102","last_name":"example/private-original"}});
            }
            1 => {
                value["sets"]["focused"]["101"]["node_id"] = Value::Null;
                value["pins"] = json!({});
            }
            2 => {
                value["sets"]["focused"]["101"]["last_name"] = json!("example/PRIVATE-ORIGINAL");
            }
            _ => {
                value["sets"]["focused"]["101"]["node_id"] = json!("101");
                value["pins"] = json!({});
            }
        }
        let candidate =
            CatalogueIntentV1::from_value(&value, source.scope(), source.intent().source())
                .unwrap();
        assert_boundary(
            save_catalogue_intent(
                &provider(&fixture),
                &mut cache,
                &source,
                candidate,
                Duration::from_secs(5),
                &CancellationToken::default(),
            )
            .unwrap_err(),
            CatalogueIntentBoundaryError::CandidateInvalid,
        );
        assert!(saved(&cache, &source).is_none());
        commands(&fixture, 9);
    }
}

#[test]
fn repo_catalog_save_unchanged_unavailable_originals_survive_new_known_selection() {
    let fixture = Fixture::new();
    let mut steps = catalogue(71, "OWNER_save_71");
    steps.extend(catalogue(71, "OWNER_save_71"));
    steps.extend(principal(42, "octocat"));
    steps.extend(principal(42, "octocat"));
    fixture.scripted_gh(&steps);
    let mut cache = Cache::open(&database(&fixture)).unwrap();
    let old_source = load(&fixture, &mut cache);
    let mut original = old_source.intent().to_value().unwrap();
    original["sets"]["unavailable-old"] = json!({"555":{"id":555,"node_id":"R_original_deleted_555","last_name":"example/no-longer-observed"}});
    cache
        .save_view(old_source.scope(), old_source.saved_view_key(), &original)
        .unwrap();
    let source = load(&fixture, &mut cache); // Explicit fresh source, not Save rebase.
    let updated = save_catalogue_intent(
        &provider(&fixture),
        &mut cache,
        &source,
        candidate(&source),
        Duration::from_secs(5),
        &CancellationToken::default(),
    )
    .unwrap();
    let value = saved(&cache, &source).unwrap();
    assert_eq!(
        value["sets"]["unavailable-old"],
        original["sets"]["unavailable-old"]
    );
    assert_eq!(updated.intent().sets()["focused"][&101].id, 101);
    assert!(matches!(
        updated
            .intent()
            .resolve(
                updated.source(),
                &updated.intent().sets()["unavailable-old"][&555]
            )
            .unwrap(),
        crate::repository_catalogue_intent_v1::IntentResolution::Unavailable(_)
    ));
    commands(&fixture, 18);
}

#[test]
fn repo_catalog_save_write_and_epoch_check_share_transaction_with_rollback_no_payload_leak() {
    for epoch_trigger in [false, true] {
        let fixture = Fixture::new();
        let mut steps = catalogue(71, "OWNER_save_71");
        steps.extend(principal(42, "octocat"));
        fixture.scripted_gh(&steps);
        let mut cache = Cache::open(&database(&fixture)).unwrap();
        cache
            .put_draft(&scope(42), "original", "private draft intact")
            .unwrap();
        let source = load(&fixture, &mut cache);
        let sql = Connection::open(database(&fixture)).unwrap();
        let before = cache
            .sync_reservation(source.scope(), source.sync_source())
            .unwrap();
        if epoch_trigger {
            sql.execute_batch("CREATE TRIGGER change_original_epoch AFTER INSERT ON saved_views BEGIN UPDATE sync_reservations SET generation=generation+1,active=0 WHERE host=NEW.host AND login=NEW.login; END;").unwrap();
        } else {
            sql.execute_batch("CREATE TRIGGER refuse_private_write BEFORE INSERT ON saved_views BEGIN SELECT RAISE(ABORT,'private-original payload must not leak'); END;").unwrap();
        }
        let expected = if epoch_trigger {
            CatalogueIntentBoundaryError::OriginalSource
        } else {
            CatalogueIntentBoundaryError::Database
        };
        let error = save_catalogue_intent(
            &provider(&fixture),
            &mut cache,
            &source,
            candidate(&source),
            Duration::from_secs(5),
            &CancellationToken::default(),
        )
        .unwrap_err();
        assert!(!format!("{error:?} {error}").contains("private-original"));
        assert_boundary(error, expected);
        assert_eq!(
            cache
                .sync_reservation(source.scope(), source.sync_source())
                .unwrap(),
            before,
            "Original epoch mutation AND write roll back together"
        );
        assert!(saved(&cache, &source).is_none());
        assert_eq!(
            cache.draft(&scope(42), "original").unwrap().as_deref(),
            Some("private draft intact")
        );
        commands(&fixture, 9);
    }
}

#[test]
fn repo_catalog_save_no_sql_after_final_proof_and_no_returned_receipt_renews_authority() {
    let fixture = Fixture::new();
    let mut steps = catalogue(71, "OWNER_save_71");
    steps.extend(principal(42, "octocat"));
    let mut closing = principal(42, "octocat");
    closing[1].delay_ms = 250;
    steps.extend(closing);
    steps.extend(principal(42, "octocat"));
    fixture.scripted_gh(&steps);
    let mut cache = Cache::open(&database(&fixture)).unwrap();
    let source = load(&fixture, &mut cache);
    let path = database(&fixture);
    let journal_path = fixture.root.join("gh-fixture");
    let view_key = source.saved_view_key().to_owned();
    let during_closing_proof = thread::spawn(move || {
        wait_step(&journal_path, 10); // Actual final user child has started.
        let sql = Connection::open(path).unwrap();
        assert_eq!(
            sql.query_row(
                "SELECT COUNT(*) FROM saved_views WHERE name=?1",
                [view_key],
                |row| row.get::<_, u64>(0)
            )
            .unwrap(),
            1,
            "Write must already precede closing account child"
        );
        sql.execute_batch("DROP TABLE sync_reservations;").unwrap();
    });
    let updated = save_catalogue_intent(
        &provider(&fixture),
        &mut cache,
        &source,
        candidate(&source),
        Duration::from_secs(5),
        &CancellationToken::default(),
    )
    .unwrap();
    during_closing_proof.join().unwrap();
    assert_eq!(
        saved(&cache, &source).unwrap(),
        updated.intent().to_value().unwrap()
    );
    assert_boundary(
        save_catalogue_intent(
            &provider(&fixture),
            &mut cache,
            &updated,
            candidate(&updated),
            Duration::from_secs(5),
            &CancellationToken::default(),
        )
        .unwrap_err(),
        CatalogueIntentBoundaryError::Database,
    );
    commands(&fixture, 13);
}

#[test]
fn repo_catalog_save_original_owner_resource_or_source_kind_never_retargets_old_intent() {
    let fixture = Fixture::new();
    let mut steps = catalogue(71, "OWNER_save_71");
    steps.extend(catalogue(72, "OWNER_recreated_72"));
    steps.extend(principal(42, "octocat"));
    fixture.scripted_gh(&steps);
    let mut cache = Cache::open(&database(&fixture)).unwrap();
    let original = load(&fixture, &mut cache);
    let replacement = load(&fixture, &mut cache);
    assert_ne!(original.saved_view_key(), replacement.saved_view_key());
    assert_ne!(original.sync_source(), replacement.sync_source());
    assert_boundary(
        save_catalogue_intent(
            &provider(&fixture),
            &mut cache,
            &replacement,
            candidate(&original),
            Duration::from_secs(5),
            &CancellationToken::default(),
        )
        .unwrap_err(),
        CatalogueIntentBoundaryError::CandidateInvalid,
    );
    assert!(saved(&cache, &original).is_none() && saved(&cache, &replacement).is_none());
    commands(&fixture, 16);
}

#[test]
fn repo_catalog_save_cancelled_or_invalid_original_deadline_sends_no_gh_or_sql_write() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&catalogue(71, "OWNER_save_71"));
    let mut cache = Cache::open(&database(&fixture)).unwrap();
    let source = load(&fixture, &mut cache);
    for duration in [Duration::ZERO, Duration::from_secs(301)] {
        let error = save_catalogue_intent(
            &provider(&fixture),
            &mut cache,
            &source,
            candidate(&source),
            duration,
            &CancellationToken::default(),
        )
        .unwrap_err();
        assert!(matches!(error.reason, CatalogueSaveReason::InvalidDeadline));
        assert!(!error.original_intent_committed);
    }
    let cancel = CancellationToken::default();
    cancel.cancel();
    assert!(save_catalogue_intent(
        &provider(&fixture),
        &mut cache,
        &source,
        candidate(&source),
        Duration::from_secs(5),
        &cancel
    )
    .is_err());
    assert!(saved(&cache, &source).is_none());
    commands(&fixture, 7);
}

#[test]
fn repo_catalog_save_actual_inflight_principal_child_concurrent_sql_fence_wins() {
    for mutation in 0..3 {
        let fixture = Fixture::new();
        let mut steps = catalogue(71, "OWNER_save_71");
        let mut pre = principal(42, "octocat");
        pre[1].delay_ms = 250;
        steps.extend(pre);
        fixture.scripted_gh(&steps);
        let mut cache = Cache::open(&database(&fixture)).unwrap();
        let source = load(&fixture, &mut cache);
        let path = database(&fixture);
        let journal_path = fixture.root.join("gh-fixture");
        let original_scope = source.scope().clone();
        let original_key = source.sync_source().to_owned();
        let concurrent = thread::spawn(move || {
            wait_step(&journal_path, 8); // Real PRE user child is in flight.
            let mut second = Cache::open(&path).unwrap();
            match mutation {
                0 => second.clear_records(&original_scope).unwrap(),
                1 => {
                    second
                        .revoke_repository(&original_scope, "example/private-original")
                        .unwrap();
                }
                _ => {
                    second
                        .reserve_sync_generation(&original_scope, &original_key)
                        .unwrap();
                }
            }
        });
        let error = save_catalogue_intent(
            &provider(&fixture),
            &mut cache,
            &source,
            candidate(&source),
            Duration::from_secs(5),
            &CancellationToken::default(),
        )
        .unwrap_err();
        concurrent.join().unwrap();
        assert_boundary(error, CatalogueIntentBoundaryError::OriginalSource);
        assert!(saved(&cache, &source).is_none());
        commands(&fixture, 9);
    }
}

#[test]
fn repo_catalog_save_factory_original_reservation_clear_during_page_refuses_before_final_proof() {
    let fixture = Fixture::new();
    let mut steps = catalogue(71, "OWNER_save_71");
    steps.truncate(4);
    steps[3].delay_ms = 250;
    fixture.scripted_gh(&steps);
    let mut cache = Cache::open(&database(&fixture)).unwrap();
    cache
        .put_draft(&scope(42), "original", "Must not clear user draft")
        .unwrap();
    let path = database(&fixture);
    let journal_path = fixture.root.join("gh-fixture");
    let concurrent = thread::spawn(move || {
        wait_step(&journal_path, 3); // Actual page child, AFTER genuine reservation.
        Cache::open(&path)
            .unwrap()
            .clear_records(&scope(42))
            .unwrap();
    });
    let error = load_catalogue_intent_source(
        &provider(&fixture),
        &mut cache,
        None,
        &RepositoryCatalogueSource::Organization("example".into()),
        &RepositoryCatalogueLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap_err();
    concurrent.join().unwrap();
    assert_boundary(error, CatalogueIntentBoundaryError::OriginalSource);
    assert_eq!(
        cache.draft(&scope(42), "original").unwrap().as_deref(),
        Some("Must not clear user draft")
    );
    let sql = Connection::open(database(&fixture)).unwrap();
    assert_eq!(
        sql.query_row(
            "SELECT COUNT(*) FROM sync_reservations WHERE active=1",
            [],
            |row| row.get::<_, u64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        sql.query_row("SELECT COUNT(*) FROM saved_views", [], |row| row
            .get::<_, u64>(0))
            .unwrap(),
        0
    );
    commands(&fixture, 4);
}
