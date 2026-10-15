//! Runtime scoped-owner contracts for actual original observed PR sections.
//! Real catalogue/session/detail factories, channel barriers and independent
//! SQL/Git effects exercise the existing publisher, including caught panic.
use super::*;
use crate::{
    app::{Action, View},
    github_workspace::{GithubAction, PrKey},
    providers::gh::{Actor, PullRequest, Queue},
    scoped_pr_observed_tabs_v1::{prepare, take_input, ObservedPrTabV1},
};

fn pr_summary() -> Value {
    json!({"id":1007,"node_id":"PR_1007","number":7,"title":"Observed runtime PR",
      "body":"PRIVATE_CURRENT_PR_BODY","html_url":format!("https://{HOST}/example/repo/pull/7"),
      "repository_url":format!("https://{HOST}/repos/example/repo"),"state":"open","draft":false,
      "user":{"id":51,"node_id":"USER_51","login":"pr-author","type":"User"},
      "head":{"sha":"a".repeat(40),"ref":"feature","repo":{"id":991,"node_id":"R_991","full_name":"fork/source"}},
      "base":{"sha":"b".repeat(40),"ref":"main","repo":{"id":410,"node_id":"R_410","full_name":"example/repo"}}})
}
fn paged(endpoint: &str, rows: Value) -> GhStep {
    GhStep::read(
        &[
            "api",
            endpoint,
            "--hostname",
            HOST,
            "--method",
            "GET",
            "--paginate",
            "--slurp",
        ],
        &json!([rows]).to_string(),
    )
}
fn detail_setup() -> (
    Fixture,
    Arc<CatalogueIntentSource>,
    Arc<ActiveRemoteScopeV1>,
    App,
    Vec<GhStep>,
) {
    let fixture = Fixture::new();
    git(&fixture, &["init", "-q"]);
    git(&fixture, &["config", "user.name", "Fixture"]);
    git(
        &fixture,
        &["config", "user.email", "fixture@example.invalid"],
    );
    std::fs::write(fixture.repo().join("tracked"), b"original\n").unwrap();
    git(&fixture, &["add", "tracked"]);
    git(&fixture, &["commit", "-qm", "Initial"]);
    std::fs::write(
        fixture.repo().join("tracked"),
        b"private uncommitted bytes\n",
    )
    .unwrap();
    let mut steps = source_steps();
    steps.extend(principal("octocat"));
    steps.push(get("repos/example/repo", repository()));
    steps.push(get("repos/example/repo/pulls/7", pr_summary()));
    for (endpoint, rows) in [
        (
            "repos/example/repo/issues/7/comments?per_page=100",
            json!([{"id":301,"node_id":"COMMENT_301","body":"Observed runtime comment","user":{"id":52,"login":"comment-author","type":"User"}}]),
        ),
        ("repos/example/repo/pulls/7/commits?per_page=100", json!([])),
        ("repos/example/repo/pulls/7/files?per_page=100", json!([])),
        ("repos/example/repo/pulls/7/reviews?per_page=100", json!([])),
        (
            "repos/example/repo/issues/7/timeline?per_page=100",
            json!([]),
        ),
    ] {
        steps.push(paged(endpoint, rows));
    }
    steps.push(get("repos/example/repo/pulls/7", pr_summary()));
    steps.push(get("repos/example/repo", repository()));
    steps.extend(principal("renamed-current-login"));
    assert_eq!(steps.len(), 20);
    fixture.scripted_gh(&steps);
    let original = load(&fixture);
    let active = scope(&original);
    let mut previous = GithubWorkspace::empty(
        original.source().identity().clone(),
        vec!["example/repo".into()],
        Default::default(),
    );
    // Previously observed navigation intent; search Issue node is deliberately
    // NOT the canonical PR node. The actual PR13 factory supplies authority.
    previous.inbox.pull_requests.push(PullRequest {
        host: HOST.into(),
        repository: "example/repo".into(),
        number: 7,
        id: "SEARCH_ISSUE_2007".into(),
        title: "Cached previous row".into(),
        body: "PRIVATE_CACHED_INBOX".into(),
        url: format!("https://{HOST}/example/repo/pull/7"),
        author: Actor::default(),
        state: "open".into(),
        draft: false,
        created_at: String::new(),
        updated_at: String::new(),
        queues: [Queue::Authored].into_iter().collect(),
    });
    previous.select_index(0);
    let selected = previous.selected.clone().unwrap();
    let result = crate::scoped_pr_read_v1::open(
        original.clone(),
        active.clone(),
        previous,
        &selected,
        &provider(&fixture),
        &database(&fixture),
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(
        result.workspace.pr.as_ref().unwrap().details.summary.pr.id,
        "PR_1007"
    );
    assert!(result.workspace.scoped_pr_navigation.is_some());
    drop(crate::pr_write_journal::PrWriteJournal::open(&database(&fixture)).unwrap());
    drop(crate::actions_write_journal::ActionsWriteJournal::open(&database(&fixture)).unwrap());
    let app = App {
        view: View::Inbox,
        github: Some(result.workspace),
        remote_scope: Some(active.clone()),
        remote_scope_source: Some(original.clone()),
        status: "ADMITTED_ORIGINAL_OWNER".into(),
        ..Default::default()
    };
    (fixture, original, active, app, steps)
}
fn git(fixture: &Fixture, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(fixture.repo())
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
fn effects(fixture: &Fixture) -> (String, Vec<u8>, Vec<u8>, String, Vec<String>) {
    let cache = Cache::open(&database(fixture)).unwrap();
    let sql = rusqlite::Connection::open(database(fixture)).unwrap();
    let rows=[
        "SELECT json_array(host,login,resource,body) FROM drafts ORDER BY host,login,resource",
        "SELECT json_array(host,login,name,payload) FROM saved_views ORDER BY host,login,name",
        "SELECT json_array(host,login,source,generation,complete) FROM checkpoints ORDER BY host,login,source",
        "SELECT json_array(host,login,kind,id,payload) FROM records ORDER BY host,login,kind,id",
        "SELECT json_array(attempt_id,state) FROM pr_write_attempts ORDER BY attempt_id",
        "SELECT json_array(attempt_id,state) FROM actions_write_attempts ORDER BY attempt_id",
    ].into_iter().flat_map(|query|sql.prepare(query).unwrap().query_map([],|row|row.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>()).collect();
    for table in ["pr_write_attempts", "actions_write_attempts"] {
        assert_eq!(
            sql.query_row(&format!("SELECT count(*) FROM {table}"), [], |row| row
                .get::<_, u64>(0))
                .unwrap(),
            0,
            "pure sections never claim/ACK/publish"
        );
    }
    assert_eq!(
        cache
            .draft(&Scope::github(HOST, Some(42)).unwrap(), "original")
            .unwrap()
            .as_deref(),
        Some("PRIVATE_ORIGINAL_DRAFT")
    );
    (
        git(fixture, &["rev-parse", "HEAD"]),
        std::fs::read(fixture.repo().join(".git/index")).unwrap(),
        std::fs::read(fixture.repo().join("tracked")).unwrap(),
        git(fixture, &["status", "--porcelain=v1"]),
        rows,
    )
}
fn submit_tab(
    background: &mut Background,
    app: &mut App,
    original: Arc<CatalogueIntentSource>,
    active: Arc<ActiveRemoteScopeV1>,
    work: impl FnOnce(CancellationToken) -> Result<PreparedView, String> + Send + 'static,
) -> bool {
    let navigation = app
        .github
        .as_ref()
        .unwrap()
        .scoped_pr_navigation
        .clone()
        .unwrap();
    background.submit_scoped_pr_tab(app, active, original, navigation, work)
}

#[test]
fn pending_reviews_v2_scoped_principal_actual_worker_retains_numeric_rename_but_not_foreign_missing_or_untagged_identity(
) {
    for (account_id, tagged, retained) in [
        (Some(42), true, true),
        (Some(43), true, false),
        (None, true, false),
        (Some(42), false, false),
    ] {
        let (fixture, original, active, mut app, mut steps) = detail_setup();
        steps.push(GhStep::read(
            &["--version"],
            "gh version 2.99.0 (2026-10-01)\n",
        ));
        steps.push(get(
            "user",
            json!({"id": account_id, "login":"actual-renamed-current-account"}),
        ));
        fixture.scripted_gh(&steps);
        let before = effects(&fixture);
        let original_nav = app
            .github
            .as_ref()
            .unwrap()
            .scoped_pr_navigation
            .clone()
            .unwrap();
        let p = provider(&fixture);
        let work = move |cancel: CancellationToken| {
            let identity = p.identity(&cancel).map_err(|error| error.message)?;
            Ok(PreparedView {
                verified_identity: Some(identity),
                status: "Actual numeric-principal read completed".into(),
                ..Default::default()
            })
        };
        let mut background = Background::new();
        assert!(if tagged {
            submit_tab(
                &mut background,
                &mut app,
                original.clone(),
                active.clone(),
                work,
            )
        } else {
            background.submit(&mut app, View::Inbox, work)
        });
        poll(&mut background, &mut app);
        assert_eq!(app.github.is_some(), retained);
        if retained {
            let github = app.github.as_ref().unwrap();
            assert_eq!(github.identity.account_id, Some(42));
            assert_eq!(github.identity.login, "actual-renamed-current-account");
            assert!(Arc::ptr_eq(
                github.scoped_pr_navigation.as_ref().unwrap(),
                &original_nav
            ));
            assert!(github.pr.is_some());
        }
        assert_eq!(app.status, "Actual numeric-principal read completed");
        if account_id == Some(42) {
            assert!(Arc::ptr_eq(
                app.remote_scope_source.as_ref().unwrap(),
                &original
            ));
            assert!(Arc::ptr_eq(app.remote_scope.as_ref().unwrap(), &active));
        } else {
            assert!(app.remote_scope_source.is_none());
            assert!(!Arc::ptr_eq(app.remote_scope.as_ref().unwrap(), &active));
        }
        assert_eq!(effects(&fixture), before);
        exact(&fixture, &steps);
    }
}

#[test]
fn pending_reviews_v2_scoped_principal_continuity_requires_actual_original_navigation_source_host_and_positive_uid(
) {
    let (fixture, original, active, mut app, steps) = detail_setup();
    let owner = ScopedRemoteOwner {
        actions_v1: None,
        scope: active.clone(),
        source: original.clone(),
        navigation: app.github.as_ref().unwrap().scoped_pr_navigation.clone(),
        check_origin: None,
        continuation_v2: None,
        pr_native_diff_v1: None,
        threads_v1: None,
        reviews_v2: None,
        inline_v1: None,
    };
    let mut identity = app.github.as_ref().unwrap().identity.clone();
    identity.login = "renamed-observation-not-permission".into();
    let before = effects(&fixture);
    assert!(owner.same_original_pr_principal(&app, &identity));
    for id in [None, Some(0), Some(43)] {
        let mut wrong = identity.clone();
        wrong.account_id = id;
        assert!(!owner.same_original_pr_principal(&app, &wrong));
    }
    let mut wrong = identity.clone();
    wrong.host = "other.github.invalid".into();
    assert!(!owner.same_original_pr_principal(&app, &wrong));
    app.remote_scope_source = None;
    assert!(!owner.same_original_pr_principal(&app, &identity));
    app.remote_scope_source = Some(original);
    app.remote_scope = None;
    assert!(!owner.same_original_pr_principal(&app, &identity));
    app.remote_scope = Some(active);
    app.github.as_mut().unwrap().scoped_pr_navigation = None;
    assert!(!owner.same_original_pr_principal(&app, &identity));
    assert_eq!(effects(&fixture), before);
    exact(&fixture, &steps);
}

#[test]
fn scoped_pr_observed_tabs_runtime_navigation_pointer_retirement_rejects_success_error_and_panic_before_all_effects(
) {
    for outcome in 0..3 {
        let (fixture, original, active, mut app, steps) = detail_setup();
        let before = effects(&fixture);
        let input =
            take_input(app.github.as_mut().unwrap(), ObservedPrTabV1::Conversation).unwrap();
        let mut background = Background::new();
        let (started, running) = mpsc::sync_channel(1);
        let (release, waiting) = mpsc::sync_channel(1);
        let producer_source = original.clone();
        let producer_active = active.clone();
        assert!(submit_tab(
            &mut background,
            &mut app,
            original.clone(),
            active.clone(),
            move |cancel| {
                started.send(()).unwrap();
                waiting.recv_timeout(Duration::from_secs(3)).unwrap();
                match outcome {
                    0 => Ok(PreparedView {
                        scoped_pr_tab: Some(
                            prepare(producer_source, producer_active, input, &cancel).unwrap(),
                        ),
                        clear_private: true,
                        status: "PRIVATE_STALE_SUCCESS".into(),
                        verified_identity: Some(crate::providers::gh::HostIdentity {
                            host: HOST.into(),
                            login: "same-login".into(),
                            account_id: Some(43),
                            gh_version: "not authority".into(),
                        }),
                        ..Default::default()
                    }),
                    1 => Err("PRIVATE_STALE_ERROR".into()),
                    _ => panic!("PRIVATE_STALE_PANIC"),
                }
            }
        ));
        running.recv_timeout(Duration::from_secs(3)).unwrap();
        let previous = app
            .github
            .as_ref()
            .unwrap()
            .scoped_pr_navigation
            .as_ref()
            .unwrap();
        let replacement = Arc::new(previous.as_ref().clone());
        assert!(!Arc::ptr_eq(previous, &replacement));
        app.github.as_mut().unwrap().scoped_pr_navigation = Some(replacement.clone());
        app.status = "NEW_NAVIGATION_OWNER_STATUS".into();
        app.content[0] = vec!["NEW_OWNER_FRAME".into()];
        release.send(()).unwrap();
        poll(&mut background, &mut app);
        assert_eq!(app.status, "NEW_NAVIGATION_OWNER_STATUS");
        assert_eq!(app.content[0], ["NEW_OWNER_FRAME"]);
        assert!(app.github.as_ref().unwrap().pr.is_none());
        assert_eq!(app.github.as_ref().unwrap().identity.account_id, Some(42));
        assert!(Arc::ptr_eq(
            app.github
                .as_ref()
                .unwrap()
                .scoped_pr_navigation
                .as_ref()
                .unwrap(),
            &replacement
        ));
        assert!(Arc::ptr_eq(app.remote_scope.as_ref().unwrap(), &active));
        assert!(Arc::ptr_eq(
            app.remote_scope_source.as_ref().unwrap(),
            &original
        ));
        assert_eq!(effects(&fixture), before);
        exact(&fixture, &steps);
    }
}

#[test]
fn scoped_pr_observed_tabs_runtime_back_cancels_generation_and_queued_page_cannot_reopen_cached_inbox(
) {
    let (fixture, original, active, mut app, steps) = detail_setup();
    let before = effects(&fixture);
    let input = take_input(app.github.as_mut().unwrap(), ObservedPrTabV1::Timeline).unwrap();
    let mut background = Background::new();
    let (started, running) = mpsc::sync_channel(1);
    let (release, waiting) = mpsc::sync_channel(1);
    let producer_source = original.clone();
    let producer_active = active.clone();
    assert!(submit_tab(
        &mut background,
        &mut app,
        original.clone(),
        active.clone(),
        move |_cancel| {
            started.send(()).unwrap();
            waiting.recv_timeout(Duration::from_secs(3)).unwrap();
            // Deliberately complete with a separate uncancelled pure token: the
            // actual Back generation+pointer fence, not producer cooperation, wins.
            let page = prepare(
                producer_source,
                producer_active,
                input,
                &CancellationToken::default(),
            )
            .unwrap();
            Ok(PreparedView {
                scoped_pr_tab: Some(page),
                status: "LATE_PAGE_MUST_NOT_REOPEN".into(),
                ..Default::default()
            })
        }
    ));
    running.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(app.loading[View::Inbox.index()]);
    app.dispatch(Action::Github(GithubAction::Back));
    assert!(app.scoped_pr_tab_cancel_requested);
    assert!(app.status.contains("Previous cached Inbox • no new reads"));
    assert!(!app.status.contains("Loading Inbox"));
    assert!(!app
        .github
        .as_ref()
        .unwrap()
        .freshness
        .label()
        .contains("refreshing"));
    background.cancel(View::Inbox);
    app.scoped_pr_tab_cancel_requested = false;
    assert!(background.current.is_empty());
    assert!(!app.loading[View::Inbox.index()]);
    assert!(app.github.as_ref().unwrap().scoped_pr_navigation.is_none());
    let cached = app.github.as_ref().unwrap().inbox.pull_requests.as_ptr();
    assert!(
        background.submit(&mut app, View::Inbox, |_| Ok(PreparedView {
            status: "CACHED_BACK_STILL_CURRENT".into(),
            ..Default::default()
        }))
    );
    release.send(()).unwrap();
    poll(&mut background, &mut app);
    drop(background);
    assert_eq!(app.status, "CACHED_BACK_STILL_CURRENT");
    assert!(app.github.as_ref().unwrap().pr.is_none());
    assert_eq!(
        app.github.as_ref().unwrap().inbox.pull_requests.as_ptr(),
        cached
    );
    assert_eq!(
        app.github.as_ref().unwrap().selected.as_ref().unwrap(),
        &PrKey {
            host: HOST.into(),
            repository: "example/repo".into(),
            number: 7
        }
    );
    assert!(Arc::ptr_eq(app.remote_scope.as_ref().unwrap(), &active));
    assert!(Arc::ptr_eq(
        app.remote_scope_source.as_ref().unwrap(),
        &original
    ));
    assert_eq!(effects(&fixture), before);
    exact(&fixture, &steps);
}

#[test]
fn scoped_pr_observed_tabs_runtime_page_only_publish_retains_cached_inbox_allocation_and_exact_owner(
) {
    let (fixture, original, active, mut app, steps) = detail_setup();
    let before = effects(&fixture);
    let cached = app.github.as_ref().unwrap().inbox.pull_requests.as_ptr();
    let navigation = app
        .github
        .as_ref()
        .unwrap()
        .scoped_pr_navigation
        .clone()
        .unwrap();
    let input = take_input(app.github.as_mut().unwrap(), ObservedPrTabV1::Conversation).unwrap();
    assert_eq!(
        app.github.as_ref().unwrap().inbox.pull_requests.as_ptr(),
        cached
    );
    let mut background = Background::new();
    let producer_source = original.clone();
    let producer_active = active.clone();
    assert!(submit_tab(
        &mut background,
        &mut app,
        original.clone(),
        active.clone(),
        move |cancel| Ok(PreparedView {
            scoped_pr_tab: Some(prepare(producer_source, producer_active, input, &cancel).unwrap()),
            status: "Observed conversation • no new reads".into(),
            ..Default::default()
        })
    ));
    poll(&mut background, &mut app);
    assert_eq!(
        app.github.as_ref().unwrap().pr.as_ref().unwrap().tab,
        crate::github_workspace::PrTab::Conversation
    );
    assert_eq!(
        app.github.as_ref().unwrap().inbox.pull_requests.as_ptr(),
        cached
    );
    assert!(Arc::ptr_eq(
        app.github
            .as_ref()
            .unwrap()
            .scoped_pr_navigation
            .as_ref()
            .unwrap(),
        &navigation
    ));
    assert_eq!(app.status, "Observed conversation • no new reads");
    assert_eq!(effects(&fixture), before);
    exact(&fixture, &steps);
}

#[test]
fn scoped_pr_observed_tabs_runtime_empty_unresolved_and_fenced_queued_open_or_tab_never_starts_worker_or_auth_but_back_is_pure(
) {
    for mode in 0..3 {
        let (fixture, original, _live, mut app, steps) = detail_setup();
        let before = effects(&fixture);
        let blocked = match mode {
            0 => ActiveRemoteScopeV1::explicit_selection(original.source(), &[]).unwrap(),
            1 => {
                // A saved navigation reference remains unavailable when not
                // observed. This decodes INTENT, never a fake Read/access row.
                let mut wire = original.intent().to_value().unwrap();
                wire["sets"] = json!({"unresolved":{"411":{"id":411,"node_id":null,"last_name":"example/unseen"}}});
                let intent = crate::repository_catalogue_intent_v1::CatalogueIntentV1::from_value(
                    &wire,
                    original.scope(),
                    original.intent().source(),
                )
                .unwrap();
                ActiveRemoteScopeV1::explicitly_activate_saved_set(
                    &intent,
                    original.source(),
                    "unresolved",
                )
                .unwrap()
            }
            _ => ActiveRemoteScopeV1::unavailable(
                crate::active_remote_scope_v1::RemoteScopeFenceV1::PrivacyFence,
            ),
        };
        assert!(crate::remote_scope_gate_v1::stop_before_fallback(Some(&blocked)).is_some());
        let blocked = Arc::new(blocked);
        app.remote_scope = Some(blocked.clone());
        let cached = app.github.as_ref().unwrap().inbox.pull_requests.as_ptr();
        let navigation = app
            .github
            .as_ref()
            .unwrap()
            .scoped_pr_navigation
            .clone()
            .unwrap();
        let ran = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut background = Background::new();
        let flag = ran.clone();
        assert!(!background.submit_scoped_remote(
            &mut app,
            View::Inbox,
            blocked.clone(),
            original.clone(),
            move |_| {
                flag.store(true, std::sync::atomic::Ordering::SeqCst);
                Err("BLOCKED_OPEN_MUST_NOT_RUN_AUTH_OR_SQL".into())
            }
        ));
        let flag = ran.clone();
        assert!(!background.submit_scoped_pr_tab(
            &mut app,
            blocked.clone(),
            original.clone(),
            navigation,
            move |_| {
                flag.store(true, std::sync::atomic::Ordering::SeqCst);
                Err("BLOCKED_TAB_MUST_NOT_RUN".into())
            }
        ));
        assert!(!ran.load(std::sync::atomic::Ordering::SeqCst));
        assert!(background.current.is_empty());
        assert_eq!(app.status, "ADMITTED_ORIGINAL_OWNER");
        assert!(!app.loading[View::Inbox.index()]);
        app.dispatch(Action::Github(GithubAction::Back));
        assert!(app.github.as_ref().unwrap().pr.is_none());
        assert!(app.github.as_ref().unwrap().scoped_pr_navigation.is_none());
        assert_eq!(
            app.github.as_ref().unwrap().inbox.pull_requests.as_ptr(),
            cached
        );
        assert!(Arc::ptr_eq(app.remote_scope.as_ref().unwrap(), &blocked));
        assert!(Arc::ptr_eq(
            app.remote_scope_source.as_ref().unwrap(),
            &original
        ));
        assert_eq!(effects(&fixture), before);
        exact(&fixture, &steps);
    }
}
