//! Actual gh/SQLite/Git session contracts and production-route regressions.
use super::*;
#[path = "original_repository_access_core_v13_tests.rs"]
mod original_core_v13;
#[path = "postwrite_capture_v2_tests.rs"]
mod postwrite_capture_v2_tests;
use crate::{
    github_workspace::GithubWorkspace,
    repository_catalogue_save_v1::load_catalogue_intent_source,
    storage::{Cache, CatalogueIntentSource, Scope},
    test_support::{Fixture, GhStep},
};
use serde_json::json;
use std::{
    fs,
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
    thread,
};

fn git(fixture: &Fixture, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(fixture.repo())
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

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
fn principal() -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.99.0 (2026-10-01)\n"),
        get("user", json!({"id":42,"login":"octocat"})),
    ]
}
fn owner() -> Value {
    json!({"id":71,"node_id":"OWNER_71","login":"example","type":"Organization"})
}
fn name(index: usize) -> String {
    format!("example/repo{index:04}")
}
fn repository(index: usize) -> Value {
    json!({"id":10000+index,"node_id":format!("R_{}",10000+index),"full_name":name(index),"owner":owner(),
  "description":"Actual retained source","private":true,"fork":false,"archived":false,"default_branch":"main","permissions":{"pull":true}})
}
fn source_steps(count: usize) -> Vec<GhStep> {
    let mut steps = principal();
    steps.push(get("orgs/example", owner()));
    steps.push(get(
        "orgs/example/repos?type=all&sort=full_name&direction=asc&per_page=100&page=1",
        json!((0..count).map(repository).collect::<Vec<_>>()),
    ));
    steps.push(get("orgs/example", owner()));
    steps.extend(principal());
    assert_eq!(steps.len(), 7);
    steps
}
fn provider(fixture: &Fixture) -> GithubProvider {
    GithubProvider::new("github.invalid")
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"))
}
fn database(fixture: &Fixture) -> std::path::PathBuf {
    fixture.root.join("cache.sqlite3")
}
fn original(fixture: &Fixture) -> Arc<CatalogueIntentSource> {
    let mut cache = Cache::open(&database(fixture)).unwrap();
    cache
        .put_draft(
            &Scope::github("github.invalid", Some(42)).unwrap(),
            "original",
            "PRIVATE_SOURCE_DRAFT",
        )
        .unwrap();
    Arc::new(
        load_catalogue_intent_source(
            &provider(fixture),
            &mut cache,
            None,
            &RepositoryCatalogueSource::Organization("example".into()),
            &RepositoryCatalogueLimits::default(),
            &CancellationToken::default(),
        )
        .unwrap(),
    )
}
fn reference(index: usize) -> IntentReference {
    IntentReference {
        id: 10000 + index as u64,
        node_id: Some(format!("R_{}", 10000 + index)),
        last_name: name(index),
    }
}
fn query(index: usize, qualifier: &str) -> GhStep {
    let repository = name(index);
    GhStep::read(&["api","search/issues","--hostname","github.invalid","--method","GET","-f",
      &format!("q=is:pr is:open {qualifier}:octocat repo:{repository}"),"-f","per_page=100","-f","page=1"],
      &json!({"total_count":1,"incomplete_results":false,"items":[{"id":30000+index,"node_id":format!("SEARCH_{index}_7"),"number":7,"title":format!("Measured real PR {index}"),"body":"Actual observed fragment",
        "repository_url":format!("https://github.invalid/api/v3/repos/{repository}"),"html_url":format!("https://github.invalid/{repository}/pull/7"),
        "pull_request":{"url":format!("https://github.invalid/api/v3/repos/{repository}/pulls/7"),"html_url":format!("https://github.invalid/{repository}/pull/7")},"state":"open","draft":false,
        "user":{"id":42,"login":"octocat","type":"User"}}]}).to_string())
}
fn external(count: usize) -> Vec<GhStep> {
    let mut steps = principal();
    for qualifier in ["author", "assignee", "review-requested"] {
        for index in 0..count {
            steps.push(query(index, qualifier));
        }
    }
    steps[0].stderr = b"\x1b[0m\x01\x02".to_vec();
    steps[2].stderr = b"\x1b[31m\x1b[0m".to_vec();
    steps
}
fn exact(fixture: &Fixture, steps: &[GhStep]) {
    let rows = fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), steps.len());
    for (row, step) in rows.iter().zip(steps) {
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(row["matched"], true);
        assert_eq!(row["stdin_hex"], "");
    }
}
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
            "Actual measured external child did not start"
        );
        thread::yield_now();
    }
}
fn limits() -> RepositoryCatalogueAccessLimits {
    let mut limits = RepositoryCatalogueAccessLimits::default();
    limits.transport.max_response_bytes = 64 * 1024 * 1024;
    limits
}

#[test]
fn repo_catalog_measured_session_three_and_sixtyfour_complete_scopes_count_every_actual_child() {
    for count in [3, 64] {
        let mut steps = source_steps(count);
        steps.extend(principal());
        for index in 0..count {
            steps.push(get(&format!("repos/{}", name(index)), repository(index)));
        }
        let external = external(count);
        steps.extend(external.clone());
        for index in 0..count {
            steps.push(get(&format!("repos/{}", name(index)), repository(index)));
        }
        steps.extend(principal());
        assert_eq!(steps.len(), 7 + 6 + 5 * count);
        let fixture = Fixture::new();
        fixture.scripted_gh(&steps);
        let original = original(&fixture);
        let provider = provider(&fixture);
        let cancel = CancellationToken::default();
        // Independent real Git state confirms browsing is not a checkout/clone.
        git(&fixture, &["init", "-q"]);
        git(&fixture, &["config", "user.name", "Measured fixture"]);
        git(
            &fixture,
            &["config", "user.email", "measured@example.invalid"],
        );
        fs::write(fixture.repo().join("local.txt"), b"saved\n").unwrap();
        git(&fixture, &["add", "--", "local.txt"]);
        git(&fixture, &["commit", "-qm", "Original Git"]);
        fs::write(fixture.repo().join("local.txt"), b"dirty unchanged\n").unwrap();
        let head = git(&fixture, &["rev-parse", "HEAD"]);
        let index = fs::read(fixture.repo().join(".git/index")).unwrap();
        let mut session = provider
            .begin_repository_catalogue_access(original.source(), &limits(), &cancel)
            .unwrap();
        let window = session
            .reserve_measured_external_reads(
                2 + 30 * count,
                1024 * 1024,
                32 * 1024 * 1024,
                2 * count,
                &cancel,
            )
            .unwrap();
        assert_eq!(window.capacity_bytes(), 32 * 1024 * 1024);
        assert_eq!(window.max_requests(), 2 + 30 * count);
        for index in 0..count {
            assert_eq!(
                session
                    .verify(&reference(index), &cancel)
                    .unwrap()
                    .repository()
                    .permissions
                    .pull,
                Some(true)
            );
        }
        let scoped = provider.clone().with_measured_read_window(window).unwrap();
        let workspace = GithubWorkspace::empty(
            session.identity().clone(),
            (0..count).map(name).collect(),
            Default::default(),
        )
        .refresh_exact_scope(original.scope(), &scoped, &database(&fixture), &cancel)
        .unwrap();
        assert_eq!(workspace.inbox.pull_requests.len(), count);
        assert_eq!(
            workspace.inbox.coverage.total_matches,
            u64::try_from(3 * count).unwrap()
        );
        assert!(workspace.inbox.coverage.complete);
        for index in 0..count {
            session.verify(&reference(index), &cancel).unwrap();
        }
        let closed = session
            .close_with_guard(&cancel, || {
                Cache::open(&database(&fixture))
                    .unwrap()
                    .validate_repository_catalogue_intent_source(&original)
                    .map_err(|_| error(GithubErrorKind::IdentityChanged, "Original SQL changed"))
            })
            .unwrap();
        let measured = closed.external_read_accounting().unwrap();
        assert!(measured.sealed());
        assert_eq!(measured.children_started(), 2 + 3 * count);
        assert_eq!(measured.requests_claimed(), 2 + 3 * count);
        assert_eq!(measured.successful_children(), 2 + 3 * count);
        assert_eq!(
            measured.stdout_bytes(),
            external.iter().map(|step| step.stdout.len()).sum::<usize>()
        );
        assert_eq!(
            measured.stderr_bytes(),
            external.iter().map(|step| step.stderr.len()).sum::<usize>()
        );
        assert_eq!(measured.failed_reserved_bytes(), 0);
        assert!(measured.charged_bytes() < 1024 * 1024);
        assert_eq!(
            closed.charged_bytes(),
            closed.accounting().charged_bytes() + measured.charged_bytes()
        );
        assert_eq!(closed.verified().len(), count);
        assert_eq!(closed.external_requests_reserved(), 2 + 30 * count);
        assert!(
            scoped.identity(&cancel).is_err(),
            "After closing proof every cloned external spec is permanently sealed"
        );
        // Independent SQL/Git oracle is test work, never producer SQL after proof.
        let cache = Cache::open(&database(&fixture)).unwrap();
        assert_eq!(
            cache
                .draft(original.scope(), "original")
                .unwrap()
                .as_deref(),
            Some("PRIVATE_SOURCE_DRAFT")
        );
        assert!(cache
            .list(&Scope::github("github.invalid", Some(43)).unwrap(), None)
            .unwrap()
            .is_empty());
        assert_eq!(git(&fixture, &["rev-parse", "HEAD"]), head);
        assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
        assert_eq!(
            fs::read(fixture.repo().join("local.txt")).unwrap(),
            b"dirty unchanged\n"
        );
        exact(&fixture, &steps);
    }
}

#[test]
fn repo_catalog_measured_actual_refresh_sixtyfour_preserves_whole_scope_and_bound_metadata() {
    let count = 64;
    let mut steps = source_steps(count);
    steps.extend(principal());
    for index in 0..count {
        steps.push(get(&format!("repos/{}", name(index)), repository(index)));
    }
    let external = external(count);
    steps.extend(external.clone());
    for index in 0..count {
        steps.push(get(&format!("repos/{}", name(index)), repository(index)));
    }
    steps.extend(principal());
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let original = original(&fixture);
    let original_intent = original.intent().to_value().unwrap();
    let ids = (0..count)
        .map(|index| 10000 + index as u64)
        .collect::<Vec<_>>();
    let active = Arc::new(
        crate::active_remote_scope_v1::ActiveRemoteScopeV1::explicit_selection(
            original.source(),
            &ids,
        )
        .unwrap(),
    );
    let result = crate::scoped_inbox_route_v1::refresh(
        original.clone(),
        active,
        &provider(&fixture),
        &database(&fixture),
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(steps.len(), 333);
    exact(&fixture, &steps);
    assert_eq!(result.workspace.inbox.pull_requests.len(), count);
    assert_eq!(result.workspace.inbox.coverage.loaded, count);
    assert_eq!(result.workspace.inbox.coverage.total_matches, 192);
    assert!(result.workspace.inbox.coverage.complete);
    assert_eq!(
        result.workspace.repositories,
        (0..count).map(name).collect::<Vec<_>>()
    );
    for index in 0..count {
        let row = result
            .workspace
            .inbox
            .pull_requests
            .iter()
            .find(|row| row.repository == name(index))
            .unwrap();
        assert_eq!(row.id, format!("SEARCH_{index}_7"));
        assert_eq!(row.number, 7);
        assert_eq!(row.queues.len(), 3);
    }
    let measured = &result.measured_legacy_read;
    assert!(measured.sealed());
    assert_eq!(measured.max_requests(), 1922);
    assert_eq!(measured.requests_claimed(), 194);
    assert_eq!(measured.children_started(), 194);
    assert_eq!(measured.successful_children(), 194);
    assert_eq!(
        measured.stdout_bytes(),
        external.iter().map(|step| step.stdout.len()).sum::<usize>()
    );
    assert_eq!(
        measured.stderr_bytes(),
        external.iter().map(|step| step.stderr.len()).sum::<usize>()
    );
    assert_eq!(measured.failed_reserved_bytes(), 0);
    assert_eq!(measured.in_flight_requests(), 0);
    assert!(measured.charged_bytes() < 1024 * 1024);
    let crate::active_remote_scope_v1::RemoteScopeConsumerV1::Explicit(current) =
        result.current_scope.consumer(&result.closed_identity)
    else {
        panic!("All original references must remain explicit, not a narrowed first member");
    };
    assert_eq!(current.references().len(), count);
    assert_eq!(
        current
            .references()
            .iter()
            .map(|reference| reference.original().id)
            .collect::<Vec<_>>(),
        ids
    );
    assert_eq!(original.intent().to_value().unwrap(), original_intent);
    let cache = Cache::open(&database(&fixture)).unwrap();
    cache
        .validate_repository_catalogue_intent_source(&original)
        .unwrap();
    assert_eq!(
        cache
            .draft(original.scope(), "original")
            .unwrap()
            .as_deref(),
        Some("PRIVATE_SOURCE_DRAFT")
    );
    assert!(cache
        .list(&Scope::github("github.invalid", Some(43)).unwrap(), None)
        .unwrap()
        .is_empty());
}

#[test]
fn repo_catalog_measured_session_parent_and_closing_allowance_refuse_before_any_external_or_repo() {
    let mut steps = source_steps(1);
    steps.extend(principal());
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let original = original(&fixture);
    let provider = provider(&fixture);
    let mut limits = limits();
    limits.transport.max_response_bytes = 32 * 1024;
    let mut session = provider
        .begin_repository_catalogue_access(
            original.source(),
            &limits,
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(session
        .reserve_measured_external_reads(
            32,
            1024 * 1024,
            32 * 1024 * 1024,
            2,
            &CancellationToken::default()
        )
        .is_err());
    assert!(session.core.measured_external.is_none());
    assert_eq!(session.core.repository_reads, 0);
    exact(&fixture, &steps);
    Cache::open(&database(&fixture))
        .unwrap()
        .validate_repository_catalogue_intent_source(&original)
        .unwrap();
}

#[test]
fn repo_catalog_measured_session_identity_and_failed_captures_share_pool_preserving_final_principal(
) {
    let mut steps = source_steps(1);
    steps.extend(principal());
    steps.push(get("repos/example/repo0000", repository(0)));
    let mut calls = principal();
    calls[0].stderr = b"\x1b[0m\x01\x02".to_vec();
    let successful_bytes = calls
        .iter()
        .map(|step| step.stdout.len() + step.stderr.len())
        .sum::<usize>();
    for _ in 0..2 {
        let mut failure = get("user", Value::Null);
        failure.status = 1;
        failure.stdout.clear();
        failure.stderr = b"\x01\x02\x03".to_vec();
        calls.push(failure);
    }
    steps.extend(calls);
    steps.extend(principal());
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let original = original(&fixture);
    let provider = provider(&fixture);
    let token = CancellationToken::default();
    let mut session = provider
        .begin_repository_catalogue_access(original.source(), &limits(), &token)
        .unwrap();
    session.verify(&reference(0), &token).unwrap();
    let window = session
        .reserve_measured_external_reads(8, 128, 256, 0, &token)
        .unwrap();
    let scoped = provider.clone().with_measured_read_window(window).unwrap();
    assert_eq!(scoped.identity(&token).unwrap().account_id, Some(42));
    for _ in 0..2 {
        assert!(scoped
            .api("user", "GET", None, None, false, &token)
            .is_err());
    }
    assert!(
        scoped.identity(&token).is_err(),
        "Exhausted pool cannot restart identity/version outside shared accounting"
    );
    let closed = session
        .close_with_guard(&token, || {
            Cache::open(&database(&fixture))
                .unwrap()
                .validate_repository_catalogue_intent_source(&original)
                .map_err(|_| error(GithubErrorKind::IdentityChanged, "Original SQL changed"))
        })
        .unwrap();
    let accounting = closed.external_read_accounting().unwrap();
    assert_eq!(accounting.children_started(), 4);
    assert_eq!(accounting.requests_claimed(), 4);
    assert_eq!(accounting.successful_children(), 2);
    assert_eq!(accounting.charged_bytes(), 256);
    assert_eq!(accounting.failed_reserved_bytes(), 256 - successful_bytes);
    assert_eq!(closed.identity().account_id, Some(42));
    exact(&fixture, &steps);
}

#[test]
fn repo_catalog_measured_session_extra_parent_get_refuses_before_guard_or_final_principal() {
    let mut steps = source_steps(1);
    steps.extend(principal());
    steps.push(get("repos/example/repo0000", repository(0)));
    steps.push(get("repos/example/repo0000", repository(0)));
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let original = original(&fixture);
    let provider = provider(&fixture);
    let token = CancellationToken::default();
    let mut session = provider
        .begin_repository_catalogue_access(original.source(), &limits(), &token)
        .unwrap();
    session.verify(&reference(0), &token).unwrap();
    let window = session
        .reserve_measured_external_reads(2, 1024, 64 * 1024, 1, &token)
        .unwrap();
    let scoped = provider.clone().with_measured_read_window(window).unwrap();
    session.verify(&reference(0), &token).unwrap();
    assert!(
        session.verify(&reference(0), &token).is_err(),
        "No undeclared parent capacity beyond requested original reads"
    );
    let called = AtomicBool::new(false);
    assert!(session
        .close_with_guard(&token, || {
            called.store(true, Ordering::SeqCst);
            Ok(())
        })
        .is_err());
    assert!(!called.load(Ordering::SeqCst));
    assert!(scoped
        .with_timeout(Duration::from_secs(100))
        .identity(&token)
        .is_err());
    exact(&fixture, &steps);
}

#[test]
fn repo_catalog_measured_session_identity_spends_original_clock_no_next_user_sql_or_final() {
    let mut steps = source_steps(1);
    steps.extend(principal());
    steps.push(get("repos/example/repo0000", repository(0)));
    let mut slow = principal().remove(0);
    slow.delay_ms = 1000;
    steps.push(slow);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let original = original(&fixture);
    let provider = provider(&fixture);
    let token = CancellationToken::default();
    let mut limits = limits();
    limits.transport.max_duration = Duration::from_millis(300);
    let mut session = provider
        .begin_repository_catalogue_access(original.source(), &limits, &token)
        .unwrap();
    session.verify(&reference(0), &token).unwrap();
    let deadline = session.deadline();
    let window = session
        .reserve_measured_external_reads(7, 1024 * 1024, 4 * 1024 * 1024, 0, &token)
        .unwrap();
    assert_eq!(window.deadline(), deadline);
    let scoped = provider
        .clone()
        .with_measured_read_window(window)
        .unwrap()
        .with_timeout(Duration::from_secs(30));
    let started = Instant::now();
    assert!(scoped.identity(&token).is_err());
    assert!(started.elapsed() < Duration::from_secs(3));
    let called = AtomicBool::new(false);
    assert!(session
        .close_with_guard(&token, || {
            called.store(true, Ordering::SeqCst);
            Ok(())
        })
        .is_err());
    assert!(!called.load(Ordering::SeqCst));
    exact(&fixture, &steps);
}

#[test]
fn repo_catalog_measured_session_cancelled_pool_has_no_next_identity_or_sql_callback() {
    let mut steps = source_steps(1);
    steps.extend(principal());
    steps.push(get("repos/example/repo0000", repository(0)));
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let original = original(&fixture);
    let provider = provider(&fixture);
    let token = CancellationToken::default();
    let mut session = provider
        .begin_repository_catalogue_access(original.source(), &limits(), &token)
        .unwrap();
    session.verify(&reference(0), &token).unwrap();
    let window = session
        .reserve_measured_external_reads(7, 1024 * 1024, 4 * 1024 * 1024, 0, &token)
        .unwrap();
    let scoped = provider.clone().with_measured_read_window(window).unwrap();
    token.cancel();
    assert!(scoped.identity(&CancellationToken::default()).is_err());
    let called = AtomicBool::new(false);
    assert!(session
        .close_with_guard(&CancellationToken::default(), || {
            called.store(true, Ordering::SeqCst);
            Ok(())
        })
        .is_err());
    assert!(!called.load(Ordering::SeqCst));
    exact(&fixture, &steps);
}

#[test]
fn repo_catalog_measured_session_live_clone_blocks_closing_guard_and_final_principal() {
    let mut steps = source_steps(1);
    steps.extend(principal());
    steps.push(get("repos/example/repo0000", repository(0)));
    let mut slow = principal().remove(0);
    slow.delay_ms = 300;
    steps.push(slow);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let original = original(&fixture);
    let provider = provider(&fixture);
    let token = CancellationToken::default();
    let mut session = provider
        .begin_repository_catalogue_access(original.source(), &limits(), &token)
        .unwrap();
    session.verify(&reference(0), &token).unwrap();
    let window = session
        .reserve_measured_external_reads(7, 1024 * 1024, 4 * 1024 * 1024, 0, &token)
        .unwrap();
    let scoped = provider.clone().with_measured_read_window(window).unwrap();
    let worker = thread::spawn(move || scoped.identity(&CancellationToken::default()));
    wait(&fixture, 11);
    let called = AtomicBool::new(false);
    assert!(session
        .close_with_guard(&token, || {
            called.store(true, Ordering::SeqCst);
            Ok(())
        })
        .is_err());
    assert!(!called.load(Ordering::SeqCst));
    assert!(
        worker.join().unwrap().is_err(),
        "Existing version child may finish but its following user cannot pass sealed pool"
    );
    exact(&fixture, &steps);
}

#[test]
fn repo_catalog_measured_session_early_drop_aborts_escaped_clone_and_reaps_real_child() {
    let mut steps = source_steps(1);
    steps.extend(principal());
    steps.push(get("repos/example/repo0000", repository(0)));
    let mut slow = principal().remove(0);
    slow.delay_ms = 1000;
    steps.push(slow);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let original = original(&fixture);
    let provider = provider(&fixture);
    let token = CancellationToken::default();
    let mut session = provider
        .begin_repository_catalogue_access(original.source(), &limits(), &token)
        .unwrap();
    session.verify(&reference(0), &token).unwrap();
    let window = session
        .reserve_measured_external_reads(7, 1024 * 1024, 4 * 1024 * 1024, 0, &token)
        .unwrap();
    let scoped = provider.clone().with_measured_read_window(window).unwrap();
    let later = scoped.clone();
    let worker = thread::spawn(move || scoped.identity(&CancellationToken::default()));
    wait(&fixture, 11);
    let rows = fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    let actual: Value = serde_json::from_str(rows.lines().last().unwrap()).unwrap();
    let pid = i32::try_from(actual["pid"].as_u64().unwrap()).unwrap();
    assert!(pid > 0);
    drop(session);
    assert!(worker.join().unwrap().is_err());
    // SAFETY: signal0 checks only the owned positive strict fixture child PID.
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ESRCH)
    );
    assert!(later
        .with_timeout(Duration::from_secs(100))
        .identity(&CancellationToken::default())
        .is_err());
    Cache::open(&database(&fixture))
        .unwrap()
        .validate_repository_catalogue_intent_source(&original)
        .unwrap();
    exact(&fixture, &steps); // No APIuser, original closingSQL/final2 or new scope.
}

#[test]
fn repo_catalog_measured_session_cancel_inside_original_sql_guard_stops_all_closing_children() {
    let mut steps = source_steps(1);
    steps.extend(principal());
    steps.push(get("repos/example/repo0000", repository(0)));
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let original = original(&fixture);
    let provider = provider(&fixture);
    let token = CancellationToken::default();
    let mut session = provider
        .begin_repository_catalogue_access(original.source(), &limits(), &token)
        .unwrap();
    session.verify(&reference(0), &token).unwrap();
    let window = session
        .reserve_measured_external_reads(7, 1024, 64 * 1024, 0, &token)
        .unwrap();
    let scoped = provider.clone().with_measured_read_window(window).unwrap();
    let guarded = AtomicBool::new(false);
    let failure = session
        .close_with_guard(&CancellationToken::default(), || {
            Cache::open(&database(&fixture))
                .unwrap()
                .validate_repository_catalogue_intent_source(&original)
                .unwrap();
            guarded.store(true, Ordering::SeqCst);
            token.cancel();
            Ok(())
        })
        .unwrap_err();
    assert!(matches!(failure.kind, GithubErrorKind::Transport));
    assert!(guarded.load(Ordering::SeqCst));
    assert!(scoped.identity(&CancellationToken::default()).is_err());
    assert_eq!(steps.len(), 10);
    exact(&fixture, &steps);
}

#[test]
fn repo_catalog_measured_session_original_cancel_during_closing_version_reaps_before_user() {
    let mut steps = source_steps(1);
    steps.extend(principal());
    steps.push(get("repos/example/repo0000", repository(0)));
    let mut closing_version = principal().remove(0);
    closing_version.delay_ms = 1000;
    steps.push(closing_version);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let original = original(&fixture);
    let provider = provider(&fixture);
    let token = CancellationToken::default();
    let mut session = provider
        .begin_repository_catalogue_access(original.source(), &limits(), &token)
        .unwrap();
    session.verify(&reference(0), &token).unwrap();
    let _window = session
        .reserve_measured_external_reads(7, 1024, 64 * 1024, 0, &token)
        .unwrap();
    let closing_token = CancellationToken::default();
    thread::scope(|workers| {
        let worker = workers.spawn(|| {
            session.close_with_guard(&closing_token, || {
                Cache::open(&database(&fixture))
                    .unwrap()
                    .validate_repository_catalogue_intent_source(&original)
                    .map_err(|_| error(GithubErrorKind::IdentityChanged, "Original SQL changed"))
            })
        });
        wait(&fixture, 11);
        let rows = fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
        let actual: Value = serde_json::from_str(rows.lines().last().unwrap()).unwrap();
        let pid = i32::try_from(actual["pid"].as_u64().unwrap()).unwrap();
        assert!(pid > 0);
        token.cancel();
        assert!(worker.join().unwrap().is_err());
        // SAFETY: checks only the owned positive real fixture child PID.
        assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH)
        );
    });
    assert_eq!(steps.len(), 11);
    exact(&fixture, &steps);
}

#[test]
fn repo_catalog_measured_legacy_session_cancel_inside_sql_guard_has_no_closing_child() {
    let mut steps = source_steps(1);
    steps.extend(principal());
    steps.push(get("repos/example/repo0000", repository(0)));
    let before_closing = steps.len();
    steps.extend(principal());
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let original = original(&fixture);
    let provider = provider(&fixture);
    let token = CancellationToken::default();
    let mut session = provider
        .begin_repository_catalogue_access(original.source(), &limits(), &token)
        .unwrap();
    session.verify(&reference(0), &token).unwrap();
    let _window = session.reserve_external_reads(7, 1024, &token).unwrap();
    let guarded = AtomicBool::new(false);
    let result = session.close_with_guard(&CancellationToken::default(), || {
        Cache::open(&database(&fixture))
            .unwrap()
            .validate_repository_catalogue_intent_source(&original)
            .unwrap();
        guarded.store(true, Ordering::SeqCst);
        token.cancel();
        Ok(())
    });
    assert!(guarded.load(Ordering::SeqCst));
    assert!(
        result.is_err(),
        "A fresh closing token cannot override the original producer's cancellation"
    );
    exact(&fixture, &steps[..before_closing]);
}

#[test]
fn repo_catalog_measured_legacy_original_cancel_reaps_closing_version_before_user() {
    let mut steps = source_steps(1);
    steps.extend(principal());
    steps.push(get("repos/example/repo0000", repository(0)));
    let mut closing = principal();
    closing[0].delay_ms = 1000;
    steps.extend(closing);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let original = original(&fixture);
    let provider = provider(&fixture);
    let token = CancellationToken::default();
    let mut session = provider
        .begin_repository_catalogue_access(original.source(), &limits(), &token)
        .unwrap();
    session.verify(&reference(0), &token).unwrap();
    let _window = session.reserve_external_reads(7, 1024, &token).unwrap();
    let closing_token = CancellationToken::default();
    thread::scope(|workers| {
        let worker = workers.spawn(|| {
            session.close_with_guard(&closing_token, || {
                Cache::open(&database(&fixture))
                    .unwrap()
                    .validate_repository_catalogue_intent_source(&original)
                    .map_err(|_| error(GithubErrorKind::IdentityChanged, "Original SQL changed"))
            })
        });
        wait(&fixture, 11);
        let pid = closing_pid(&fixture);
        token.cancel();
        assert!(
            worker.join().unwrap().is_err(),
            "Original cancellation must terminate the active closing version"
        );
        // SAFETY: signal0 checks only this strict fixture's observed positive PID.
        assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH)
        );
    });
    exact(&fixture, &steps[..11]);
}

fn closing_pid(fixture: &Fixture) -> i32 {
    let rows = fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    let actual: Value = serde_json::from_str(rows.lines().last().unwrap()).unwrap();
    let pid = i32::try_from(actual["pid"].as_u64().unwrap()).unwrap();
    assert!(pid > 0);
    pid
}
