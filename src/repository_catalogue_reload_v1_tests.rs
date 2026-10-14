//! Strict real-child/original SQLite re-read contracts.
use super::*;
use crate::{
    repository_catalogue_read_limits_test_support_v1 as support,
    repository_catalogue_read_limits_v1::CatalogueDiscoveryBudgetV1,
    repository_catalogue_save_v1::save_catalogue_intent,
    test_support::{Fixture, GhStep},
};
use serde_json::{json, Value};
use std::{
    fs, thread,
    time::{Duration, Instant},
};

fn wait(fixture: &Fixture, count: usize) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
            .unwrap_or_default()
            .lines()
            .count()
            == count
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Actual original reread child barrier not reached"
        );
        thread::yield_now();
    }
}
fn payload(fixture: &Fixture, original: &CatalogueIntentSource) -> Option<String> {
    use rusqlite::OptionalExtension;
    rusqlite::Connection::open_with_flags(
        support::database(fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap()
    .query_row(
        "SELECT payload FROM saved_views WHERE host=?1 AND login=?2 AND name=?3",
        rusqlite::params![
            original.scope().host,
            original.scope().login,
            original.saved_view_key()
        ],
        |row| row.get(0),
    )
    .optional()
    .unwrap()
}
fn edit_payload(fixture: &Fixture, original: &CatalogueIntentSource) {
    let mut candidate = original.intent().clone();
    candidate
        .set(original.source(), "CONCURRENT_USER_EDIT", &[10000])
        .unwrap();
    let bytes = serde_json::to_string(&candidate.to_value().unwrap()).unwrap();
    rusqlite::Connection::open(support::database(fixture)).unwrap().execute("INSERT INTO saved_views(host,login,name,payload) VALUES(?1,?2,?3,?4) ON CONFLICT(host,login,name) DO UPDATE SET payload=excluded.payload",
        rusqlite::params![original.scope().host,original.scope().login,original.saved_view_key(),bytes]).unwrap();
}
fn safe(failure: &CatalogueSaveFailure) {
    let message = failure.to_string();
    for canary in [
        "PRIVATE_REREAD_DIAGNOSTIC",
        "ORIGINAL_UID42_DRAFT",
        "CONCURRENT_USER_EDIT",
        "Fresh observed repository",
    ] {
        assert!(!message.contains(canary));
    }
    assert!(
        !failure.original_intent_committed,
        "Re-read reserves source only; no saved intent committed"
    );
}

#[test]
fn repo_catalog_read_limits_reload_beyond2000_fresh_page1_keeps_exact_durable_missing_refs_and_pins(
) {
    let old_rows = (0..2000).map(support::row).collect::<Vec<_>>();
    let new_rows = (0..2051)
        .filter(|index| *index != 777)
        .map(support::row)
        .collect::<Vec<_>>();
    let mut steps = support::collection(&old_rows, 20, 42, 42);
    steps.extend(support::principal(42));
    steps.extend(support::principal(42));
    steps.extend(support::collection(&new_rows, 100, 42, 42));
    assert_eq!(steps.len(), 26 + 4 + 27);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let old = support::load(&fixture, &CatalogueDiscoveryBudgetV1::default().limits());
    assert_eq!(old.source().repositories().len(), 2000);
    assert!(!old.source().coverage().membership_complete);
    let mut candidate = old.intent().clone();
    candidate
        .set(old.source(), "originals", &[10000, 10777])
        .unwrap();
    candidate.pin(old.source(), 10000, true).unwrap();
    let mut cache = Cache::open(&support::database(&fixture)).unwrap();
    let old = Arc::new(
        save_catalogue_intent(
            &support::provider(&fixture),
            &mut cache,
            &old,
            candidate,
            Duration::from_secs(30),
            &CancellationToken::default(),
        )
        .unwrap(),
    );
    let before = payload(&fixture, &old).unwrap();
    let value = old.intent().to_value().unwrap();
    let fresh = reload_catalogue_intent_source(
        &support::provider(&fixture),
        &mut cache,
        &old,
        &CatalogueDiscoveryBudgetV1::preset(1).unwrap().limits(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(fresh.source().repositories().len(), 2050);
    assert!(fresh.source().coverage().membership_complete);
    assert_eq!(fresh.source().coverage().fetched_pages, 21);
    assert_eq!(fresh.source().coverage().raw_rows, 2050);
    assert!(fresh
        .source()
        .repositories()
        .iter()
        .any(|row| row.id == 12049));
    assert!(!fresh
        .source()
        .repositories()
        .iter()
        .any(|row| row.id == 10777));
    assert_eq!(fresh.intent().to_value().unwrap(), value);
    assert_eq!(payload(&fixture, &fresh).as_deref(), Some(before.as_str()));
    assert_eq!(
        fresh.intent().sets()["originals"][&10777].last_name,
        "example/repo0777"
    );
    assert_eq!(
        fresh.intent().sets()["originals"][&10777]
            .node_id
            .as_deref(),
        Some("R_10777")
    );
    assert!(fresh.intent().pins().contains_key(&10000));
    assert_eq!(support::generation(&fixture, &fresh), 2);
    assert!(cache
        .validate_repository_catalogue_intent_source(&old)
        .is_err());
    cache
        .validate_repository_catalogue_intent_source(&fresh)
        .unwrap();
    assert_eq!(
        cache.draft(fresh.scope(), "original").unwrap().as_deref(),
        Some("ORIGINAL_UID42_DRAFT")
    );
    assert!(cache
        .list(&Scope::github(support::HOST, Some(43)).unwrap(), None)
        .unwrap()
        .is_empty());
    support::exact(&fixture, &steps);
}

#[test]
fn repo_catalog_read_limits_reload_concurrent_payload_or_generation_change_is_not_adopted_or_reserved(
) {
    for change in 0..2 {
        let old_rows = [support::row(0)];
        let mut steps = support::collection(&old_rows, 20, 42, 42);
        steps.extend(support::principal(42));
        let mut owner = support::get("orgs/example", support::owner());
        owner.delay_ms = 300;
        steps.push(owner);
        let fixture = Fixture::new();
        fixture.scripted_gh(&steps);
        let old = support::load(&fixture, &CatalogueDiscoveryBudgetV1::default().limits());
        let provider = support::provider(&fixture);
        let database = support::database(&fixture);
        let original = old.clone();
        let worker = thread::spawn(move || {
            let mut cache = Cache::open(&database).unwrap();
            reload_catalogue_intent_source(
                &provider,
                &mut cache,
                &original,
                &CatalogueDiscoveryBudgetV1::preset(1).unwrap().limits(),
                &CancellationToken::default(),
            )
        });
        wait(&fixture, 10);
        if change == 0 {
            edit_payload(&fixture, &old);
        } else {
            rusqlite::Connection::open(support::database(&fixture)).unwrap().execute("UPDATE sync_reservations SET generation=generation+1 WHERE host=?1 AND login=?2 AND source=?3",rusqlite::params![old.scope().host,old.scope().login,old.sync_source()]).unwrap();
        }
        let expected_payload = payload(&fixture, &old);
        let expected_generation = support::generation(&fixture, &old);
        let failure = worker.join().unwrap().unwrap_err();
        safe(&failure);
        assert!(matches!(failure.reason, CatalogueSaveReason::Boundary(_)));
        assert_eq!(
            support::generation(&fixture, &old),
            expected_generation,
            "No new reread reservation after stale original CAS"
        );
        assert_eq!(
            payload(&fixture, &old),
            expected_payload,
            "Concurrent original user edit not overwritten/adopted"
        );
        support::exact(&fixture, &steps);
    }
}

#[test]
fn repo_catalog_read_limits_reload_foreign_issuer_pre_uid43_and_changed_owner_stop_before_reserve_pages(
) {
    for mode in 0..5 {
        let rows = [support::row(0)];
        let mut steps = support::collection(&rows, 20, 42, 42);
        steps.extend(support::principal(if mode == 1 { 43 } else { 42 }));
        if mode != 1 {
            let mut owner = support::owner();
            match mode {
                2 => owner["id"] = json!(72),
                3 => owner["node_id"] = Value::Null,
                4 => owner["type"] = json!("User"),
                _ => {}
            }
            steps.push(support::get("orgs/example", owner));
        }
        let fixture = Fixture::new();
        fixture.scripted_gh(&steps);
        let old = support::load(&fixture, &CatalogueDiscoveryBudgetV1::default().limits());
        let path = if mode == 0 {
            fixture.root.join("foreign.sqlite3")
        } else {
            support::database(&fixture)
        };
        let mut cache = Cache::open(&path).unwrap();
        let before = payload(&fixture, &old);
        let failure = reload_catalogue_intent_source(
            &support::provider(&fixture),
            &mut cache,
            &old,
            &CatalogueDiscoveryBudgetV1::preset(1).unwrap().limits(),
            &CancellationToken::default(),
        )
        .unwrap_err();
        safe(&failure);
        assert_eq!(support::generation(&fixture, &old), 1);
        assert_eq!(payload(&fixture, &old), before);
        if mode == 0 {
            let count: i64 = rusqlite::Connection::open(path)
                .unwrap()
                .query_row("SELECT count(*) FROM sync_reservations", [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 0);
        }
        assert_eq!(steps.len(), if mode == 1 { 9 } else { 10 });
        support::exact(&fixture, &steps);
    }
}

#[test]
fn repo_catalog_read_limits_reload_partial_failure_keeps_only_new_rows_and_actual_coverage_not_old_append(
) {
    let old_rows = [support::row(0), support::row(1), support::row(2)];
    let mut steps = support::collection(&old_rows, 20, 42, 42);
    steps.extend(support::principal(42));
    steps.push(support::get("orgs/example", support::owner()));
    let fresh = (100..200).map(support::row).collect::<Vec<_>>();
    steps.push(support::get(
        "orgs/example/repos?type=all&sort=full_name&direction=asc&per_page=100&page=1",
        json!(fresh),
    ));
    let mut failure = GhStep::read(
        &[
            "api",
            "orgs/example/repos?type=all&sort=full_name&direction=asc&per_page=100&page=2",
            "--hostname",
            support::HOST,
            "--method",
            "GET",
        ],
        "",
    );
    failure.status = 1;
    failure.stderr = b"HTTP 403 PRIVATE_REREAD_DIAGNOSTIC".to_vec();
    steps.push(failure);
    steps.push(support::get("orgs/example", support::owner()));
    steps.extend(support::principal(42));
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let original = support::load(&fixture, &CatalogueDiscoveryBudgetV1::default().limits());
    let mut cache = Cache::open(&support::database(&fixture)).unwrap();
    let next = reload_catalogue_intent_source(
        &support::provider(&fixture),
        &mut cache,
        &original,
        &CatalogueDiscoveryBudgetV1::preset(1).unwrap().limits(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(next.source().repositories().len(), 100);
    assert!(next
        .source()
        .repositories()
        .iter()
        .all(|row| row.id >= 10100));
    assert_eq!(next.source().coverage().requested_pages, 2);
    assert_eq!(next.source().coverage().fetched_pages, 1);
    assert!(!next.source().coverage().membership_complete);
    assert!(next.source().coverage().failure.is_some());
    assert!(!next.source().coverage().complete_empty);
    assert!(next.source().accounting().failed_reserved_bytes() > 0);
    assert_eq!(support::generation(&fixture, &next), 2);
    assert_eq!(steps.len(), 15);
    support::exact(&fixture, &steps);
}

#[test]
fn repo_catalog_read_limits_reload_final_uid43_never_binds_new_source_or_copies_original_intent() {
    let rows = [support::row(0)];
    let mut steps = support::collection(&rows, 20, 42, 42);
    steps.extend(support::collection(&rows, 100, 42, 43));
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let original = support::load(&fixture, &CatalogueDiscoveryBudgetV1::default().limits());
    let mut cache = Cache::open(&support::database(&fixture)).unwrap();
    let before = payload(&fixture, &original);
    let failure = reload_catalogue_intent_source(
        &support::provider(&fixture),
        &mut cache,
        &original,
        &CatalogueDiscoveryBudgetV1::preset(1).unwrap().limits(),
        &CancellationToken::default(),
    )
    .unwrap_err();
    safe(&failure);
    assert!(matches!(
        failure.reason,
        CatalogueSaveReason::Principal(GithubErrorKind::IdentityChanged)
    ));
    assert_eq!(
        support::generation(&fixture, &original),
        2,
        "Deliberate original42 reservation precedes final loss; NO rollback claim"
    );
    assert_eq!(payload(&fixture, &original), before);
    let other = Scope::github(support::HOST, Some(43)).unwrap();
    assert!(cache.list(&other, None).unwrap().is_empty());
    assert!(cache.draft(&other, "original").unwrap().is_none());
    assert_eq!(steps.len(), 14);
    support::exact(&fixture, &steps);
}

#[test]
fn repo_catalog_read_limits_reload_original_deadline_and_live_cancel_prevent_closing_and_next_page()
{
    for cancel_live in [false, true] {
        let rows = [support::row(0)];
        let mut steps = support::collection(&rows, 20, 42, 42);
        steps.extend(support::principal(42));
        steps.push(support::get("orgs/example", support::owner()));
        let mut page = support::get(
            "orgs/example/repos?type=all&sort=full_name&direction=asc&per_page=100&page=1",
            json!([support::row(0)]),
        );
        page.delay_ms = 1000;
        steps.push(page);
        let fixture = Fixture::new();
        fixture.scripted_gh(&steps);
        let original = support::load(&fixture, &CatalogueDiscoveryBudgetV1::default().limits());
        let provider = support::provider(&fixture);
        let database = support::database(&fixture);
        let old = original.clone();
        let token = CancellationToken::default();
        let child_token = token.clone();
        let worker = thread::spawn(move || {
            let mut cache = Cache::open(&database).unwrap();
            let mut limits = CatalogueDiscoveryBudgetV1::preset(1).unwrap().limits();
            if !cancel_live {
                limits.max_duration = Duration::from_millis(160);
            }
            reload_catalogue_intent_source(&provider, &mut cache, &old, &limits, &child_token)
        });
        wait(&fixture, 11);
        if cancel_live {
            token.cancel();
        }
        let failure = worker.join().unwrap().unwrap_err();
        safe(&failure);
        assert_eq!(support::generation(&fixture, &original), 2);
        assert!(payload(&fixture, &original).is_none());
        assert_eq!(steps.len(), 11);
        support::exact(&fixture, &steps);
    }
}

#[test]
fn repo_catalog_read_limits_reload_concurrent_payload_after_reservation_refuses_before_final_owner_user(
) {
    let rows = [support::row(0)];
    let mut steps = support::collection(&rows, 20, 42, 42);
    steps.extend(support::principal(42));
    steps.push(support::get("orgs/example", support::owner()));
    let mut page = support::get(
        "orgs/example/repos?type=all&sort=full_name&direction=asc&per_page=100&page=1",
        json!([support::row(0)]),
    );
    page.delay_ms = 300;
    steps.push(page);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let original = support::load(&fixture, &CatalogueDiscoveryBudgetV1::default().limits());
    let provider = support::provider(&fixture);
    let database = support::database(&fixture);
    let old = original.clone();
    let worker = thread::spawn(move || {
        let mut cache = Cache::open(&database).unwrap();
        reload_catalogue_intent_source(
            &provider,
            &mut cache,
            &old,
            &CatalogueDiscoveryBudgetV1::preset(1).unwrap().limits(),
            &CancellationToken::default(),
        )
    });
    wait(&fixture, 11);
    assert_eq!(support::generation(&fixture, &original), 2);
    edit_payload(&fixture, &original);
    let concurrent = payload(&fixture, &original);
    let failure = worker.join().unwrap().unwrap_err();
    safe(&failure);
    assert!(matches!(
        failure.reason,
        CatalogueSaveReason::Boundary(CatalogueIntentBoundaryError::SavedIntentChanged)
    ));
    assert_eq!(payload(&fixture, &original), concurrent);
    assert_eq!(support::generation(&fixture, &original), 2);
    support::exact(&fixture, &steps);
}

#[test]
fn repo_catalog_read_limits_reload_owner403_and_pre_cancel_have_no_new_reservation_or_intent_reads()
{
    for cancel_pre in [false, true] {
        let rows = [support::row(0)];
        let mut steps = support::collection(&rows, 20, 42, 42);
        if !cancel_pre {
            steps.extend(support::principal(42));
            let mut owner = support::get("orgs/example", Value::Null);
            owner.status = 1;
            owner.stderr = b"HTTP 403 PRIVATE_REREAD_DIAGNOSTIC".to_vec();
            steps.push(owner);
        }
        let fixture = Fixture::new();
        fixture.scripted_gh(&steps);
        let original = support::load(&fixture, &CatalogueDiscoveryBudgetV1::default().limits());
        let mut cache = Cache::open(&support::database(&fixture)).unwrap();
        let token = CancellationToken::default();
        if cancel_pre {
            token.cancel();
        }
        let failure = reload_catalogue_intent_source(
            &support::provider(&fixture),
            &mut cache,
            &original,
            &CatalogueDiscoveryBudgetV1::preset(1).unwrap().limits(),
            &token,
        )
        .unwrap_err();
        safe(&failure);
        assert_eq!(support::generation(&fixture, &original), 1);
        assert!(payload(&fixture, &original).is_none());
        support::exact(&fixture, &steps);
    }
}
