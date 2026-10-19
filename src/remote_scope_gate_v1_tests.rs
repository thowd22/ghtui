//! SOURCE-ONLY consumer contracts. Actual catalogue factories/strict gh;
//! intent/old UI DTOs never construct current repository-access authority.
use super::*;
use crate::{
    active_remote_scope_v1::RemoteScopeFenceV1,
    forms::Form,
    local_actions::{LocalCommand, LocalContext},
    providers::{
        gh::{
            GithubProvider, RepositoryCatalogueLimits, RepositoryCatalogueRead,
            RepositoryCatalogueSource,
        },
        runner::CancellationToken,
    },
    repository_catalogue_intent_v1::CatalogueIntentV1,
    storage::{Cache, Record, Scope},
    test_support::{Fixture, GhStep},
    workspace::{RepositoryMode, Workspace},
};
use serde_json::{json, Value};
use std::{fs, process::Command, sync::Arc};

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
fn repo(id: u64, name: &str) -> Value {
    json!({"id":id,"node_id":format!("R_{id}"),"full_name":name,
      "owner":{"id":71,"node_id":"OWNER_71","login":"example","type":"Organization"},
      "permissions":{"pull":true}})
}
fn actual(uid: u64, login: &str, rows: Vec<Value>) -> (Fixture, RepositoryCatalogueRead) {
    let fixture = Fixture::new();
    let account = || {
        vec![
            GhStep::read(&["--version"], "gh version 2.99.0 (2026-10-01)\n"),
            get("user", json!({"id":uid,"login":login})),
        ]
    };
    let owner = json!({"id":71,"node_id":"OWNER_71","login":"example","type":"Organization"});
    let mut steps = account();
    steps.push(get("orgs/example", owner.clone()));
    steps.push(get(
        "orgs/example/repos?type=all&sort=full_name&direction=asc&per_page=100&page=1",
        json!(rows),
    ));
    steps.push(get("orgs/example", owner));
    steps.extend(account());
    fixture.scripted_gh(&steps);
    let provider = GithubProvider::new("github.invalid")
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"));
    let read = provider
        .repository_catalogue_read(
            None,
            &RepositoryCatalogueSource::Organization("example".into()),
            &RepositoryCatalogueLimits::default(),
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(read.accounting().requests(), 7);
    exact(&fixture, &steps);
    (fixture, read)
}
fn exact(fixture: &Fixture, steps: &[GhStep]) {
    let rows = journal(fixture);
    assert_eq!(rows.len(), steps.len());
    for (row, step) in rows.iter().zip(steps) {
        assert_eq!(row["matched"], true);
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(row["stdin_hex"], "");
    }
}
fn journal(fixture: &Fixture) -> Vec<Value> {
    fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn unresolved() -> (Fixture, ActiveRemoteScopeV1) {
    let (_original_fixture, original) = actual(
        42,
        "same-login",
        vec![repo(410, "example/one"), repo(411, "example/two")],
    );
    let mut saved = CatalogueIntentV1::new(original.context()).unwrap();
    saved.set(&original, "both", &[410, 411]).unwrap();
    let (fixture, current) = actual(42, "same-login", vec![repo(410, "example/one")]);
    let active =
        ActiveRemoteScopeV1::explicitly_activate_saved_set(&saved, &current, "both").unwrap();
    assert!(matches!(
        active.consumer(current.identity()),
        RemoteScopeConsumerV1::Unresolved(_)
    ));
    (fixture, active)
}
fn pending(app: &mut App) {
    app.github_action = Some(crate::github_workspace::GithubAction::Back);
    app.pr_checks_request = true;
    app.actions_action = Some(crate::actions_workspace::ActionsAction::Refresh);
    app.actions_load_requested = true;
    app.search_action = Some(crate::search::workspace::SearchAction::Saved(
        "private-saved".into(),
    ));
    app.search_load_requested = true;
    app.corpus_requested = true;
    app.source_action = Some(crate::search_sources::SourceAction::Load);
    app.metrics_action = Some(crate::metrics::MetricsAction::Refresh);
    app.metrics_load_requested = true;
    app.refresh_requested = true;
    app.notice = "PRIVATE_NOTICE_CANARY".into();
    for view in [View::Inbox, View::Actions, View::Search, View::Dashboard] {
        app.loading[view.index()] = true;
        app.content[view.index()] = vec!["PRIVATE_COUNT_SNIPPET_CANARY".into()];
    }
}
fn assert_retired(app: &App) {
    assert!(app.github.is_none() && app.github_action.is_none());
    assert!(app.pr_action.is_none() && !app.pr_checks_request);
    assert!(app.actions.is_none() && app.actions_action.is_none() && !app.actions_load_requested);
    assert!(app.search.is_none() && app.search_action.is_none() && !app.search_load_requested);
    assert!(app.source_action.is_none() && app.source_context.is_none() && !app.corpus_requested);
    assert!(app.metrics.is_none() && app.metrics_action.is_none() && !app.metrics_load_requested);
    assert!(app.notice.is_empty() && app.hits.is_empty());
    for view in [View::Inbox, View::Actions, View::Search, View::Dashboard] {
        assert!(!app.loading[view.index()]);
        assert!(!app.content[view.index()].join("\n").contains("PRIVATE_"));
        assert!(!app.content[view.index()].join("\n").contains("0 records"));
    }
}

#[test]
fn repo_catalog_scope_consumers_empty_unresolved_and_fenced_never_allow_legacy_fallback() {
    let (fixture, read) = actual(42, "same-login", vec![]);
    let empty = ActiveRemoteScopeV1::explicit_selection(&read, &[]).unwrap();
    let (missing_fixture, missing) = unresolved();
    let fenced = ActiveRemoteScopeV1::unavailable(RemoteScopeFenceV1::PrivacyFence);
    for active in [empty, missing, fenced] {
        let stop = stop_before_fallback(Some(&active)).unwrap();
        for consumer in RemoteConsumerV1::ALL {
            let mut app = App {
                view: View::Actions,
                remote_scope: Some(Arc::new(active.clone())),
                ..Default::default()
            };
            pending(&mut app);
            let original = app.remote_scope.as_ref().unwrap().clone();
            let mut background = Background::new();
            assert!(refuse_legacy_request(&mut app, &mut background, consumer));
            assert_retired(&app);
            assert!(!app.refresh_requested);
            assert!(Arc::ptr_eq(app.remote_scope.as_ref().unwrap(), &original));
            assert_eq!(
                stop_before_fallback(app.remote_scope.as_deref()),
                Some(stop)
            );
            assert!(app.status.contains(consumer.label()));
            assert!(app.status.contains("no fallback") || app.status.contains("fallback or query"));
            app.notice = "Read-only scope explanation • no authority or request".into();
            app.overlay = Some(Overlay::Notice);
            assert!(!enforce_stopped_intent(&mut app, &mut background));
            assert_eq!(app.overlay, Some(Overlay::Notice));
            assert!(app.notice.contains("Read-only scope explanation"));
        }
    }
    assert_eq!(journal(&fixture).len(), 7);
    assert_eq!(journal(&missing_fixture).len(), 7);
}

#[test]
fn repo_catalog_scope_consumers_none_startup_does_not_reset_requests_or_local_intent() {
    let mut app = App {
        view: View::Repository,
        context: "Local independent original".into(),
        ..Default::default()
    };
    pending(&mut app);
    let before = app.content.clone();
    let mut background = Background::new();
    assert!(!enforce_stopped_intent(&mut app, &mut background));
    for consumer in RemoteConsumerV1::ALL {
        assert!(!refuse_legacy_request(&mut app, &mut background, consumer));
    }
    assert!(app.remote_scope.is_none());
    assert!(app.refresh_requested && app.corpus_requested && app.actions_load_requested);
    assert!(
        app.search_action.is_some() && app.source_action.is_some() && app.metrics_action.is_some()
    );
    assert_eq!(app.content, before);
    assert_eq!(app.context, "Local independent original");
}

#[test]
fn repo_catalog_scope_consumers_observed_nonempty_requires_current_ids_not_saved_name_vec() {
    let (fixture, read) = actual(42, "same-login", vec![repo(410, "example/one")]);
    let active = ActiveRemoteScopeV1::explicit_selection(&read, &[410]).unwrap();
    assert_eq!(stop_before_fallback(Some(&active)), None);
    let CurrentNavigationV1::ExplicitRequiresCurrentAccess(refs) =
        after_current_identity(Some(&active), read.identity())
    else {
        panic!("actual observed refs must stay typed navigation intent");
    };
    assert_eq!(refs.references().len(), 1);
    assert_eq!(refs.references()[0].original().id, 410);
    assert_eq!(
        refs.references()[0].original().node_id.as_deref(),
        Some("R_410")
    );
    assert_eq!(
        refs.references()[0].observed().unwrap().last_name,
        "example/one"
    );
    assert_eq!(
        legacy_entry(Some(&active)),
        Err(ScopeEntryStopV1::ScopedRouteRequired)
    );
    // Legitimate deliberate personal navigation is also tagged and needs an
    // original numeric guard; it is not an error recovery/default-reset path.
    let personal = ActiveRemoteScopeV1::default_personal(read.identity()).unwrap();
    assert!(matches!(
        after_current_identity(Some(&personal), read.identity()),
        CurrentNavigationV1::DefaultPersonalIntent
    ));
    assert_eq!(
        legacy_entry(Some(&personal)),
        Err(ScopeEntryStopV1::ScopedRouteRequired)
    );
    assert_eq!(journal(&fixture).len(), 7);
}

#[test]
fn repo_catalog_scope_consumers_actual_uid_switch_and_rename_are_distinct() {
    let (_original, read) = actual(42, "same-login", vec![repo(410, "example/one")]);
    let active = ActiveRemoteScopeV1::explicit_selection(&read, &[410]).unwrap();
    let (renamed_fixture, renamed) = actual(42, "new-login", vec![repo(410, "example/one")]);
    assert!(matches!(
        after_current_identity(Some(&active), renamed.identity()),
        CurrentNavigationV1::ExplicitRequiresCurrentAccess(_)
    ));
    let (reused_fixture, reused) = actual(43, "same-login", vec![repo(410, "example/one")]);
    assert!(matches!(
        after_current_identity(Some(&active), reused.identity()),
        CurrentNavigationV1::Stopped(ScopeEntryStopV1::Intent(RemoteScopeStopV1::Fenced(
            RemoteScopeFenceV1::PrincipalChanged
        )))
    ));
    assert!(
        matches!(
            active.consumer(read.identity()),
            RemoteScopeConsumerV1::Explicit(_)
        ),
        "a refusal never mutates original into default/new account"
    );
    assert_eq!(journal(&renamed_fixture).len(), 7);
    assert_eq!(journal(&reused_fixture).len(), 7);
}

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
#[test]
fn repo_catalog_scope_consumers_retirement_preserves_real_local_git_form_and_sql_drafts() {
    let (fixture, read) = actual(42, "same-login", vec![]);
    git(&fixture, &["init", "--initial-branch=main"]);
    git(&fixture, &["config", "user.name", "Scope fixture"]);
    git(&fixture, &["config", "user.email", "scope@example.invalid"]);
    fs::write(fixture.repo().join("file.txt"), b"saved\n").unwrap();
    git(&fixture, &["add", "--", "file.txt"]);
    git(&fixture, &["commit", "-m", "Original local scope"]);
    fs::write(fixture.repo().join("file.txt"), b"untouched dirty bytes\n").unwrap();
    let head = git(&fixture, &["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = git(&fixture, &["status", "--porcelain=v2", "-z"]);
    let workspace = Workspace::load(&fixture.repo(), RepositoryMode::Overview, "HEAD", None)
        .unwrap()
        .unwrap();
    let mut form = Form::local(
        LocalCommand::CreateBranch,
        LocalContext::from_workspace(&workspace, None, None),
    )
    .unwrap();
    form.fields[0]
        .editor
        .insert("untouched-local-draft")
        .unwrap();
    let local_draft = form.draft();
    let database = fixture.root.join("cache.sqlite3");
    let cache = Cache::open(&database).unwrap();
    let remote = Scope::github("github.invalid", Some(42)).unwrap();
    cache
        .put_draft(&remote, "original/receipt-draft", "PRIVATE_DRAFT_PRESERVED")
        .unwrap();
    cache
        .put(
            &remote,
            &Record::new(
                "private-fixture",
                "original",
                "example/one",
                json!({"body":"PRIVATE_BODY_PRESERVED"}),
            ),
        )
        .unwrap();
    let active = Arc::new(ActiveRemoteScopeV1::explicit_selection(&read, &[]).unwrap());
    let mut app = App {
        view: View::Repository,
        workspace: Some(workspace),
        form: Some(form),
        context: "Local original breadcrumb".into(),
        remote_scope: Some(active.clone()),
        ..Default::default()
    };
    pending(&mut app);
    app.loading[View::Repository.index()] = true;
    app.inbox_list.selected = 81;
    app.inbox_list.top = 53;
    app.focus = 7;
    let mut background = Background::new();
    assert!(enforce_stopped_intent(&mut app, &mut background));
    assert_retired(&app);
    assert!(app.workspace.is_some() && app.loading[View::Repository.index()]);
    assert_eq!(
        (app.inbox_list.selected, app.inbox_list.top, app.focus),
        (81, 53, 7)
    );
    assert_eq!(app.form.as_ref().unwrap().draft(), local_draft);
    assert_eq!(app.context, "Local original breadcrumb");
    assert!(
        app.refresh_requested,
        "do not consume a local refresh request"
    );
    assert!(Arc::ptr_eq(app.remote_scope.as_ref().unwrap(), &active));
    assert_eq!(
        cache
            .draft(&remote, "original/receipt-draft")
            .unwrap()
            .as_deref(),
        Some("PRIVATE_DRAFT_PRESERVED")
    );
    assert_eq!(
        cache
            .get(&remote, "private-fixture", "original")
            .unwrap()
            .unwrap()
            .payload["body"],
        "PRIVATE_BODY_PRESERVED"
    );
    assert_eq!(git(&fixture, &["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(git(&fixture, &["status", "--porcelain=v2", "-z"]), status);
    assert_eq!(
        fs::read(fixture.repo().join("file.txt")).unwrap(),
        b"untouched dirty bytes\n"
    );
    assert_eq!(journal(&fixture).len(), 7);
}

#[test]
fn repo_catalog_immutable_images_fenced_scroll_cannot_hide_actual_narrow_explanation() {
    use ratatui::{backend::TestBackend, Terminal};
    let (fixture, read) = actual(42, "same-login", vec![repo(410, "example/one")]);
    let mut active = ActiveRemoteScopeV1::explicit_selection(&read, &[410]).unwrap();
    active.fence(RemoteScopeFenceV1::PrivacyFence);
    let database = fixture.root.join("visibility.sqlite3");
    let cache = Cache::open(&database).unwrap();
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    cache
        .put_draft(&scope, "original", "PRIVATE_DRAFT_RETAINED")
        .unwrap();
    let mut app = App {
        view: View::Inbox,
        inbox_list: crate::ui_table::ListState {
            selected: 41,
            top: 35,
        },
        focus: 7,
        github_content_focused: true,
        remote_scope: Some(Arc::new(active)),
        ..Default::default()
    };
    let mut background = Background::new();
    assert!(enforce_stopped_intent(&mut app, &mut background));
    assert_eq!(
        (app.inbox_list.selected, app.inbox_list.top, app.focus),
        (0, 0, 0)
    );
    assert!(!app.github_content_focused);
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|frame| crate::ui::draw(frame, &mut app))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let screen = (0..24)
        .map(|y| (0..80).map(|x| buffer[(x, y)].symbol()).collect::<String>())
        .collect::<Vec<_>>();
    let words = screen
        .iter()
        .map(|line| line.trim().trim_matches('│').trim())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(words.contains("Original repository scope is fenced • private pages hidden • no fallback or query sent"), "{words}");
    assert!(!words.contains("PRIVATE_DRAFT_RETAINED"));
    assert_eq!(
        cache.draft(&scope, "original").unwrap().as_deref(),
        Some("PRIVATE_DRAFT_RETAINED")
    );
    assert_eq!(journal(&fixture).len(), 7);
}

#[test]
fn repo_catalog_scope_consumers_unsupported_legacy_keeps_live_catalogue_owner_and_tag() {
    let (fixture, read) = actual(42, "same-login", vec![repo(410, "example/one")]);
    let active = Arc::new(ActiveRemoteScopeV1::explicit_selection(&read, &[410]).unwrap());
    let mut app = App {
        settings_host: "github.invalid".into(),
        view: View::Actions,
        remote_scope: Some(active.clone()),
        ..Default::default()
    };
    crate::repository_catalogue_native::dispatch(
        &mut app,
        crate::repository_catalogue_native::CatalogueUiAction::Open,
    );
    let original_owner = app.catalogue.as_ref().unwrap().owner.clone();
    let overlay = app.overlay;
    let mut background = Background::new();
    assert!(refuse_legacy_request(
        &mut app,
        &mut background,
        RemoteConsumerV1::Actions
    ));
    assert!(Arc::ptr_eq(
        &app.catalogue.as_ref().unwrap().owner,
        &original_owner
    ));
    assert_eq!(app.overlay, overlay);
    assert!(!app.catalogue.as_ref().unwrap().fenced);
    assert!(Arc::ptr_eq(app.remote_scope.as_ref().unwrap(), &active));
    assert!(matches!(
        active.consumer(read.identity()),
        RemoteScopeConsumerV1::Explicit(_)
    ));
    assert_eq!(journal(&fixture).len(), 7);
}

#[test]
fn repo_catalog_scope_consumers_stale_saved_search_and_watch_requests_are_removed_together() {
    let (fixture, read) = actual(42, "same-login", vec![]);
    let active = Arc::new(ActiveRemoteScopeV1::explicit_selection(&read, &[]).unwrap());
    let mut app = App {
        view: View::Search,
        remote_scope: Some(active.clone()),
        ..Default::default()
    };
    pending(&mut app);
    let mut actions = crate::actions_workspace::ActionsWorkspace::new(
        read.identity().clone(),
        vec!["example/old-private".into()],
    );
    actions.watching = true;
    app.actions = Some(actions);
    let mut search = crate::search::workspace::SearchWorkspace::new(
        Scope::github("github.invalid", Some(42)).unwrap(),
        vec![],
    );
    search.remote_identity = Some(read.identity().clone());
    search.remote_repositories = vec!["example/old-private".into()];
    search.input = "PRIVATE_SAVED_QUERY_CANARY".into();
    app.search = Some(search);
    let mut background = Background::new();
    assert!(enforce_stopped_intent(&mut app, &mut background));
    assert_retired(&app);
    assert!(!app.refresh_requested);
    assert!(app.remote_scope.is_some());
    assert_eq!(
        stop_before_fallback(app.remote_scope.as_deref()),
        Some(ScopeEntryStopV1::Intent(RemoteScopeStopV1::ExplicitEmpty))
    );
    assert_eq!(journal(&fixture).len(), 7);
}

#[test]
fn repo_catalog_scope_consumers_existing_working_search_is_preserved_without_config_rebuild() {
    let (fixture, read) = actual(42, "same-login", vec![]);
    git(&fixture, &["init", "--initial-branch=main"]);
    fs::write(
        fixture.repo().join("selected.txt"),
        b"working needle original\n",
    )
    .unwrap();
    let database = fixture.root.join("cache.sqlite3");
    let cache = Cache::open(&database).unwrap();
    let selection = crate::search::corpus::RepositorySelection {
        repository: "local/selected".into(),
        directory: fixture.repo(),
        source: crate::search::corpus::LocalSource::Worktree,
        sets: vec![],
    };
    let mut search = crate::search::workspace::SearchWorkspace::new(
        Scope::new("local", "workspace").unwrap(),
        vec![selection],
    );
    search
        .search_cached("needle", &cache, &CancellationToken::default())
        .unwrap();
    assert_eq!(search.page.as_ref().unwrap().hits.len(), 1);
    let original_id = search.page.as_ref().unwrap().hits[0].document.key.clone();
    let original_body = search.page.as_ref().unwrap().hits[0].document.body.clone();
    search.remote_repositories = vec!["example/not-authority".into()];
    let active = Arc::new(ActiveRemoteScopeV1::explicit_selection(&read, &[]).unwrap());
    let mut app = App {
        view: View::Search,
        remote_scope: Some(active.clone()),
        search: Some(search),
        search_action: Some(crate::search::workspace::SearchAction::Submit),
        context: "Working local original".into(),
        ..Default::default()
    };
    app.loading[View::Search.index()] = true;
    let mut background = Background::new();
    assert!(enforce_stopped_intent(&mut app, &mut background));
    assert!(!refuse_legacy_request(
        &mut app,
        &mut background,
        RemoteConsumerV1::Search
    ));
    let retained = app.search.as_ref().unwrap();
    assert_eq!(retained.selections.len(), 1);
    assert_eq!(retained.selections[0].directory, fixture.repo());
    assert_eq!(retained.selections[0].repository, "local/selected");
    assert_eq!(
        retained.page.as_ref().unwrap().hits[0].document.key,
        original_id
    );
    assert_eq!(
        retained.page.as_ref().unwrap().hits[0].document.body,
        original_body
    );
    assert!(retained.remote_repositories.is_empty());
    assert!(app.search_action.is_some() && app.loading[View::Search.index()]);
    assert_eq!(app.context, "Working local original");
    // A Saved query can change provider/source/scope; it is NOT the retained
    // Working source exception and cannot queue a fallback/SQL restore.
    app.search_action = Some(crate::search::workspace::SearchAction::Saved(
        "outside".into(),
    ));
    assert!(refuse_legacy_request(
        &mut app,
        &mut background,
        RemoteConsumerV1::Search
    ));
    assert!(app.search_action.is_none());
    assert!(
        app.search.is_some(),
        "rejecting remote intent must not erase existing local results"
    );
    assert!(Arc::ptr_eq(app.remote_scope.as_ref().unwrap(), &active));
    assert_eq!(
        fs::read(fixture.repo().join("selected.txt")).unwrap(),
        b"working needle original\n"
    );
    assert_eq!(journal(&fixture).len(), 7);
}

#[test]
fn origin_default_personal_scope_does_not_block_repository_views_but_still_guards_inbox_refresh() {
    let (fixture, read) = actual(42, "same-login", vec![repo(410, "example/one")]);
    let personal = ActiveRemoteScopeV1::default_personal(read.identity()).unwrap();
    assert!(personal.is_default_personal());
    let mut background = crate::runtime::Background::new();
    for consumer in [
        RemoteConsumerV1::Actions,
        RemoteConsumerV1::Search,
        RemoteConsumerV1::Dashboard,
    ] {
        let mut app = App {
            remote_scope: Some(Arc::new(personal.clone())),
            ..App::default()
        };
        assert!(!refuse_legacy_request(&mut app, &mut background, consumer));
        assert!(
            app.remote_scope.is_some(),
            "{consumer:?} kept the personal scope"
        );
    }
    let mut app = App {
        remote_scope: Some(Arc::new(personal)),
        ..App::default()
    };
    assert!(refuse_legacy_request(
        &mut app,
        &mut background,
        RemoteConsumerV1::MyPrs
    ));
    let explicit = ActiveRemoteScopeV1::explicit_selection(&read, &[410]).unwrap();
    assert!(!explicit.is_default_personal());
    let mut app = App {
        remote_scope: Some(Arc::new(explicit)),
        ..App::default()
    };
    assert!(refuse_legacy_request(
        &mut app,
        &mut background,
        RemoteConsumerV1::Actions
    ));
    assert_eq!(journal(&fixture).len(), 7, "gating sends no GitHub request");
}
