//! SOURCE-ONLY runtime child. Root registration/Cargo still required; UNRUN.
//! Genuine catalogue factories supply sources. Channel barriers exercise actual
//! Request/Completion ownership, independently of generation cancellation.
use super::*;
#[path = "scoped_pr_observed_tabs_runtime_v1_tests.rs"]
mod observed_tabs;
use crate::{
    active_remote_scope_v1::ActiveRemoteScopeV1,
    github_workspace::GithubWorkspace,
    providers::gh::{GithubProvider, RepositoryCatalogueLimits, RepositoryCatalogueSource},
    repository_catalogue_save_v1::load_catalogue_intent_source,
    storage::{Cache, CatalogueIntentSource, Scope},
    test_support::{Fixture, GhStep},
};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

const HOST: &str = "github.invalid";
fn get(endpoint: &str, value: Value) -> GhStep {
    GhStep::read(
        &["api", endpoint, "--hostname", HOST, "--method", "GET"],
        &value.to_string(),
    )
}
fn principal(login: &str) -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.99.0 (2026-10-01)\n"),
        get("user", json!({"id":42,"login":login})),
    ]
}
fn owner() -> Value {
    json!({"id":71,"node_id":"OWNER_71","login":"example","type":"Organization"})
}
fn repository() -> Value {
    json!({"id":410,"node_id":"R_410","full_name":"example/repo","owner":owner(),
      "description":"Actual original catalogue member","private":true,"fork":false,
      "archived":false,"default_branch":"main","language":"Rust","permissions":{"pull":true}})
}
fn source_steps() -> Vec<GhStep> {
    let mut steps = principal("octocat");
    steps.push(get("orgs/example", owner()));
    steps.push(get(
        "orgs/example/repos?type=all&sort=full_name&direction=asc&per_page=100&page=1",
        json!([repository()]),
    ));
    steps.push(get("orgs/example", owner()));
    steps.extend(principal("octocat"));
    assert_eq!(steps.len(), 7);
    steps
}
fn provider(fixture: &Fixture) -> GithubProvider {
    GithubProvider::new(HOST)
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"))
}
fn database(fixture: &Fixture) -> std::path::PathBuf {
    fixture.root.join("cache.sqlite3")
}
fn load(fixture: &Fixture) -> Arc<CatalogueIntentSource> {
    let mut cache = Cache::open(&database(fixture)).unwrap();
    cache
        .put_draft(
            &Scope::github(HOST, Some(42)).unwrap(),
            "original",
            "PRIVATE_ORIGINAL_DRAFT",
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
fn scope(source: &CatalogueIntentSource) -> Arc<ActiveRemoteScopeV1> {
    Arc::new(ActiveRemoteScopeV1::explicit_selection(source.source(), &[410]).unwrap())
}
fn app(source: Arc<CatalogueIntentSource>, active: Arc<ActiveRemoteScopeV1>) -> App {
    App {
        view: View::Inbox,
        settings_host: HOST.into(),
        github: Some(GithubWorkspace::empty(
            source.source().identity().clone(),
            vec!["example/repo".into()],
            Default::default(),
        )),
        remote_scope: Some(active),
        remote_scope_source: Some(source),
        context: "CURRENT_PRIVATE_OWNER".into(),
        status: "CURRENT_PRIVATE_STATUS".into(),
        ..App::default()
    }
}
fn poll(background: &mut Background, app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while background.current.contains_key(&View::Inbox.index()) {
        background.poll(app);
        assert!(
            Instant::now() < deadline,
            "Real scoped worker completion did not arrive"
        );
        thread::yield_now();
    }
}
fn exact(fixture: &Fixture, expected: &[GhStep]) {
    let rows = std::fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), expected.len());
    for (actual, expected) in rows.iter().zip(expected) {
        assert_eq!(actual["matched"], true);
        assert_eq!(actual["args"], json!(expected.args));
        assert_eq!(actual["stdin_hex"], "");
    }
}

#[test]
fn repo_catalog_scoped_runtime_rejects_unowned_pair_before_queue_status_or_work() {
    let fixture = Fixture::new();
    let steps = source_steps();
    fixture.scripted_gh(&steps);
    let original = load(&fixture);
    let active = scope(&original);
    let mut app = app(original.clone(), active.clone());
    let equal_text_different_owner = Arc::new(active.as_ref().clone());
    assert!(!Arc::ptr_eq(&active, &equal_text_different_owner));
    let mut background = Background::new();
    let ran = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let observed = ran.clone();
    assert!(!background.submit_scoped_remote(
        &mut app,
        View::Inbox,
        equal_text_different_owner,
        original,
        move |_| {
            observed.store(true, std::sync::atomic::Ordering::SeqCst);
            Err("UNOWNED_WORK_MUST_NOT_START".into())
        }
    ));
    assert!(!ran.load(std::sync::atomic::Ordering::SeqCst));
    assert!(background.current.is_empty());
    assert_eq!(app.status, "CURRENT_PRIVATE_STATUS");
    assert_eq!(app.context, "CURRENT_PRIVATE_OWNER");
    assert_eq!(app.loading, [false; 5]);
    exact(&fixture, &steps);
}

#[test]
fn repo_catalog_scoped_runtime_equal_scope_owner_retired_success_error_panic_are_effect_free() {
    for outcome in 0..3 {
        let fixture = Fixture::new();
        let steps = source_steps();
        fixture.scripted_gh(&steps);
        let original = load(&fixture);
        let active = scope(&original);
        let mut app = app(original.clone(), active.clone());
        let mut background = Background::new();
        let (started, running) = mpsc::sync_channel(1);
        let (release, waiting) = mpsc::sync_channel(1);
        assert!(background.submit_scoped_remote(
            &mut app,
            View::Inbox,
            active.clone(),
            original.clone(),
            move |_| {
                started.send(()).unwrap();
                waiting.recv_timeout(Duration::from_secs(3)).unwrap();
                match outcome {
                    0 => Ok(PreparedView {
                        verified_identity: Some(crate::providers::gh::HostIdentity {
                            host: HOST.into(),
                            login: "octocat".into(),
                            account_id: Some(43),
                            gh_version: "probe, not API authority".into(),
                        }),
                        clear_private: true,
                        status: "PRIVATE_STALE_SUCCESS".into(),
                        lines: vec!["PRIVATE_STALE_FRAME".into()],
                        ..Default::default()
                    }),
                    1 => Err("PRIVATE_STALE_ERROR".into()),
                    _ => panic!("PRIVATE_STALE_PANIC"),
                }
            }
        ));
        running.recv_timeout(Duration::from_secs(3)).unwrap();
        // No cancel/resubmit: the exact pointer fence itself must reject this
        // completion, even though the genuine immutable scope text is equal.
        let replacement = Arc::new(active.as_ref().clone());
        app.remote_scope = Some(replacement.clone());
        app.status = "NEW_OWNER_STATUS".into();
        app.content[View::Inbox.index()] = vec!["NEW_OWNER_VISIBLE_FRAME".into()];
        let before = Cache::open(&database(&fixture))
            .unwrap()
            .list(original.scope(), None)
            .unwrap();
        release.send(()).unwrap();
        poll(&mut background, &mut app);
        assert!(Arc::ptr_eq(
            app.remote_scope.as_ref().unwrap(),
            &replacement
        ));
        assert!(Arc::ptr_eq(
            app.remote_scope_source.as_ref().unwrap(),
            &original
        ));
        assert_eq!(app.github.as_ref().unwrap().identity.account_id, Some(42));
        assert_eq!(app.status, "NEW_OWNER_STATUS");
        assert_eq!(app.context, "CURRENT_PRIVATE_OWNER");
        assert_eq!(
            app.content[View::Inbox.index()],
            ["NEW_OWNER_VISIBLE_FRAME"]
        );
        let cache = Cache::open(&database(&fixture)).unwrap();
        assert_eq!(cache.list(original.scope(), None).unwrap(), before);
        assert_eq!(
            cache
                .draft(original.scope(), "original")
                .unwrap()
                .as_deref(),
            Some("PRIVATE_ORIGINAL_DRAFT")
        );
        exact(&fixture, &steps);
    }
}

#[test]
fn repo_catalog_scoped_runtime_source_owner_replacement_rejects_same_scope_private_effects() {
    let fixture = Fixture::new();
    let mut steps = source_steps();
    steps.extend(source_steps());
    fixture.scripted_gh(&steps);
    let original = load(&fixture);
    let active = scope(&original);
    let replacement = load(&fixture); // Second genuine read, NOT cloned/raw authority.
    assert_eq!(original.scope(), replacement.scope());
    assert_eq!(
        original.source().repositories()[0].id,
        replacement.source().repositories()[0].id
    );
    assert!(!Arc::ptr_eq(&original, &replacement));
    let mut app = app(original.clone(), active.clone());
    let mut background = Background::new();
    let (started, running) = mpsc::sync_channel(1);
    let (release, waiting) = mpsc::sync_channel(1);
    assert!(background.submit_scoped_remote(
        &mut app,
        View::Inbox,
        active.clone(),
        original.clone(),
        move |_| {
            started.send(()).unwrap();
            waiting.recv_timeout(Duration::from_secs(3)).unwrap();
            Ok(PreparedView {
                clear_private: true,
                status: "PRIVATE_SOURCE_OLD_FAILURE".into(),
                ..Default::default()
            })
        }
    ));
    running.recv_timeout(Duration::from_secs(3)).unwrap();
    app.remote_scope_source = Some(replacement.clone());
    app.status = "NEW_SOURCE_STATUS".into();
    release.send(()).unwrap();
    poll(&mut background, &mut app);
    assert!(Arc::ptr_eq(app.remote_scope.as_ref().unwrap(), &active));
    assert!(Arc::ptr_eq(
        app.remote_scope_source.as_ref().unwrap(),
        &replacement
    ));
    assert_eq!(app.github.as_ref().unwrap().identity.account_id, Some(42));
    assert_eq!(app.status, "NEW_SOURCE_STATUS");
    assert_eq!(app.context, "CURRENT_PRIVATE_OWNER");
    Cache::open(&database(&fixture))
        .unwrap()
        .validate_repository_catalogue_intent_source(&replacement)
        .unwrap();
    exact(&fixture, &steps);
}

#[test]
fn repo_catalog_scoped_runtime_true_owned_hidden_failure_fences_without_erasing_original_sql() {
    let fixture = Fixture::new();
    let steps = source_steps();
    fixture.scripted_gh(&steps);
    let original = load(&fixture);
    let active = scope(&original);
    let mut app = app(original.clone(), active.clone());
    let mut background = Background::new();
    assert!(background.submit_scoped_remote(
        &mut app,
        View::Inbox,
        active,
        original.clone(),
        |_| {
            Ok(PreparedView {
                clear_private: true,
                status: "OWNED_PRIVATE_ACCESS_UNAVAILABLE".into(),
                ..Default::default()
            })
        }
    ));
    poll(&mut background, &mut app);
    assert!(app.github.is_none());
    assert!(app.remote_scope_source.is_none());
    assert!(matches!(
        app.remote_scope
            .as_ref()
            .unwrap()
            .consumer(original.source().identity()),
        crate::active_remote_scope_v1::RemoteScopeConsumerV1::Fenced(_)
    ));
    assert_eq!(app.status, "OWNED_PRIVATE_ACCESS_UNAVAILABLE");
    let cache = Cache::open(&database(&fixture)).unwrap();
    assert_eq!(
        cache
            .draft(original.scope(), "original")
            .unwrap()
            .as_deref(),
        Some("PRIVATE_ORIGINAL_DRAFT")
    );
    assert!(cache
        .list(&Scope::github(HOST, Some(43)).unwrap(), None)
        .unwrap()
        .is_empty());
    exact(&fixture, &steps);
}
