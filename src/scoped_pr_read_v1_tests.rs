//! Strict subprocess/SQLite/real Git contracts for selected scoped PR reads.
use super::*;
#[path = "scoped_pr_observed_tabs_v1_tests.rs"]
mod observed_tabs;
#[path = "scoped_pr_observed_tabs_cap_view_v1_tests.rs"]
mod observed_tabs_cap_view;
#[path = "scoped_pr_read_v1_raw_node_tests.rs"]
mod raw_node_tests;
#[path = "scoped_pr_resources_v1_tests.rs"]
mod resources;
use crate::{
    app::{Action, App},
    github_workspace::{GithubAction, InboxFilter},
    providers::gh::{
        Actor, Coverage, Inbox, PullRequest, Queue, RepositoryCatalogueLimits,
        RepositoryCatalogueSource,
    },
    repository_catalogue_save_v1::load_catalogue_intent_source,
    test_support::{Fixture, GhStep},
};
use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    thread,
    time::{Duration, Instant},
};

const HOST: &str = "github.invalid";
const REPO: &str = "example/one";
const DRAFT: &str = "PRIVATE_ORIGINAL_DRAFT_SCOPED_PR";
const OLD: &str = "PRIVATE_PREVIOUS_INBOX_SCOPED_PR";
const FRESH: &str = "Fresh canonical detail body • 界🦀";
fn get(endpoint: &str, value: Value) -> GhStep {
    GhStep::read(
        &["api", endpoint, "--hostname", HOST, "--method", "GET"],
        &value.to_string(),
    )
}
fn list(endpoint: &str, rows: Value) -> GhStep {
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
fn principal(uid: u64, login: &str) -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.99.0 (2026-10-01)\n"),
        get("user", json!({"id":uid,"login":login})),
    ]
}
fn owner() -> Value {
    json!({"id":71,"node_id":"OWNER_71","login":"example","type":"Organization"})
}
fn repo(id: u64, name: &str) -> Value {
    json!({"id":id,"node_id":format!("R_{id}"),"full_name":name,"owner":owner(),"permissions":{"pull":true,"push":false}})
}
fn source_steps() -> Vec<GhStep> {
    let mut steps = principal(42, "octocat");
    steps.push(get("orgs/example", owner()));
    steps.push(get(
        "orgs/example/repos?type=all&sort=full_name&direction=asc&per_page=100&page=1",
        json!([repo(410, REPO), repo(411, "example/two")]),
    ));
    steps.push(get("orgs/example", owner()));
    steps.extend(principal(42, "octocat"));
    steps
}
fn summary(name: &str) -> Value {
    json!({"id":1007,"node_id":"GENUINE_PR_NODE_1007","number":7,"title":"Canonical REST PR details","body":FRESH,
        "html_url":format!("https://{HOST}/{name}/pull/7"),"repository_url":format!("https://{HOST}/repos/{name}"),
        "user":{"id":51,"node_id":"USER_51","login":"actual-pr-author","type":"User"},"state":"open","draft":false,
        "created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-14T00:00:00Z","merged":false,
        "head":{"sha":"a".repeat(40),"ref":"feature/scoped-read","repo":{"id":991,"node_id":"R_991","full_name":"fork/source"}},
        "base":{"sha":"b".repeat(40),"ref":"main","repo":{"id":410,"node_id":"R_410","full_name":name}},
        "additions":3,"deletions":1,"changed_files":1,"mergeable":null,"mergeable_state":"unknown"})
}
fn detail_steps(name: &str) -> Vec<GhStep> {
    vec![
        get(&format!("repos/{name}/pulls/7"), summary(name)),
        list(
            &format!("repos/{name}/issues/7/comments?per_page=100"),
            json!([{"id":301,"node_id":"COMMENT_301","body":"Actual conversation","user":{"id":52,"login":"comment-author","type":"User"},"created_at":"2026-10-02T00:00:00Z"}]),
        ),
        list(
            &format!("repos/{name}/pulls/7/commits?per_page=100"),
            json!([{"sha":"c".repeat(40),"commit":{"message":"Genuine observed commit","author":{"name":"Commit author"}}}]),
        ),
        list(
            &format!("repos/{name}/pulls/7/files?per_page=100"),
            json!([{"filename":"src/界.rs","status":"modified","additions":3,"deletions":1,"patch":"@@ -1 +1 @@\n-old\n+new"}]),
        ),
        list(
            &format!("repos/{name}/pulls/7/reviews?per_page=100"),
            json!([{"id":601,"node_id":"REVIEW_601","state":"COMMENTED","body":"Review text","user":{"id":53,"login":"review-author","type":"User"}}]),
        ),
        list(
            &format!("repos/{name}/issues/7/timeline?per_page=100"),
            json!([{"id":801,"event":"review_requested","created_at":"2026-10-02T00:00:00Z"}]),
        ),
        get(&format!("repos/{name}/pulls/7"), summary(name)),
    ]
}
fn operation(name: &str, final_uid: u64) -> Vec<GhStep> {
    let mut steps = principal(42, "octocat");
    steps.push(get(&format!("repos/{REPO}"), repo(410, name)));
    steps.extend(detail_steps(name));
    steps.push(get(&format!("repos/{REPO}"), repo(410, name)));
    steps.extend(principal(final_uid, "renamed-same-principal"));
    steps
}
fn database(fixture: &Fixture) -> std::path::PathBuf {
    fixture.root.join("cache.sqlite3")
}
fn git(fixture: &Fixture, arguments: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(arguments)
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
fn provider(fixture: &Fixture) -> GithubProvider {
    GithubProvider::new(HOST)
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"))
}
fn load(fixture: &Fixture) -> Arc<CatalogueIntentSource> {
    let mut cache = Cache::open(&database(fixture)).unwrap();
    cache
        .put_draft(
            &Scope::github(HOST, Some(42)).unwrap(),
            "original-review",
            DRAFT,
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
fn previous(original: &CatalogueIntentSource) -> GithubWorkspace {
    let mut workspace = GithubWorkspace::empty(
        original.source().identity().clone(),
        vec![REPO.into(), "example/two".into()],
        Default::default(),
    );
    let row = PullRequest {
        host: HOST.into(),
        repository: REPO.into(),
        number: 7,
        id: "SEARCH_ISSUE_NODE_2007_NOT_PR_NODE".into(),
        title: "Selected previous search observation".into(),
        body: OLD.into(),
        url: format!("https://{HOST}/{REPO}/pull/7"),
        author: Actor::default(),
        state: "open".into(),
        draft: false,
        created_at: String::new(),
        updated_at: String::new(),
        queues: [Queue::Authored].into_iter().collect(),
    };
    workspace.selected = Some(PrKey::from(&row));
    workspace.inbox = Inbox {
        identity: workspace.identity.clone(),
        pull_requests: vec![row],
        coverage: Coverage {
            loaded: 1,
            total_matches: 1,
            complete: true,
            reasons: vec![],
        },
    };
    workspace.filter = InboxFilter::Authored;
    workspace.freshness = Freshness::Fresh { fetched_at: 991 };
    workspace
}
fn active(original: &CatalogueIntentSource) -> Arc<ActiveRemoteScopeV1> {
    Arc::new(ActiveRemoteScopeV1::explicit_selection(original.source(), &[410, 411]).unwrap())
}
fn sql(fixture: &Fixture) -> Connection {
    Connection::open_with_flags(database(fixture), OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap()
}
fn sql_state(fixture: &Fixture) -> Vec<String> {
    let db = sql(fixture);
    ["SELECT json_array(host,login,resource,body) FROM drafts ORDER BY host,login,resource",
     "SELECT json_array(host,login,name,payload) FROM saved_views ORDER BY host,login,name",
     "SELECT json_array(host,login,source,generation,complete) FROM checkpoints ORDER BY host,login,source",
     "SELECT json_array(host,login,kind,id,payload) FROM records ORDER BY host,login,kind,id"]
        .iter().flat_map(|query| db.prepare(query).unwrap().query_map([],|row|row.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>()).collect()
}
fn exact(fixture: &Fixture, steps: &[GhStep]) {
    let rows = fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), steps.len());
    for (row, step) in rows.iter().zip(steps) {
        assert_eq!(row["matched"], true);
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(row["stdin_hex"], "");
        assert!(!step
            .args
            .iter()
            .any(|arg| matches!(arg.as_str(), "POST" | "PATCH" | "DELETE" | "graphql")));
    }
}
fn safe(error: &ScopedPrReadFailureV1) {
    for canary in [
        DRAFT,
        OLD,
        FRESH,
        "PRIVATE_DIAGNOSTIC",
        "PRIVATE_SQL_CANARY",
    ] {
        assert!(!error.to_string().contains(canary));
    }
}
fn call(
    fixture: &Fixture,
    original: Arc<CatalogueIntentSource>,
    scope: Arc<ActiveRemoteScopeV1>,
    workspace: GithubWorkspace,
) -> Result<ScopedPrReadResultV1, ScopedPrReadFailureV1> {
    let key = workspace.selected.clone().unwrap();
    open(
        original,
        scope,
        workspace,
        &key,
        &provider(fixture),
        &database(fixture),
        &CancellationToken::default(),
    )
}

#[test]
fn scoped_pr_read_happy13_canonical_node_not_search_identity_whole_scope_sql_git_unchanged() {
    let fixture = Fixture::new();
    let mut steps = source_steps();
    steps.extend(operation(REPO, 42));
    fixture.scripted_gh(&steps);
    let original = load(&fixture);
    let scope = active(&original);
    let before = sql_state(&fixture);
    git(&fixture, &["init", "--initial-branch=main"]);
    git(&fixture, &["config", "user.name", "Scoped PR fixture"]);
    git(
        &fixture,
        &["config", "user.email", "scoped-pr@example.invalid"],
    );
    fs::write(fixture.repo().join("dirty界.txt"), b"original\n").unwrap();
    git(&fixture, &["add", "--", "dirty界.txt"]);
    git(
        &fixture,
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "Scoped PR baseline",
        ],
    );
    fs::write(fixture.repo().join("dirty界.txt"), b"untouched dirty\n").unwrap();
    let head = git(&fixture, &["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let before_git = git(&fixture, &["status", "--porcelain=v2", "-z"]);
    let result = call(
        &fixture,
        original.clone(),
        scope.clone(),
        previous(&original),
    )
    .unwrap();
    assert_eq!(steps.len(), 20);
    exact(&fixture, &steps);
    assert_eq!(result.reserved_detail_children, 7);
    assert_eq!(result.reserved_detail_capture_bytes, 7 * 1024 * 1024);
    assert!(Arc::ptr_eq(&result.original_active, &scope));
    assert!(Arc::ptr_eq(&result.original_source, &original));
    let RemoteScopeConsumerV1::Explicit(refs) =
        result.original_active.consumer(&result.closed_identity)
    else {
        panic!("unchanged full scope expected")
    };
    assert_eq!(refs.references().len(), 2);
    assert_eq!(refs.references()[1].original().id, 411);
    assert_eq!(result.workspace.repositories, [REPO, "example/two"]);
    assert!(matches!(
        result.workspace.freshness,
        Freshness::Cached { fetched_at: 991 }
    ));
    assert!(!result.workspace.inbox.coverage.complete);
    assert_eq!(
        result.workspace.inbox.coverage.reasons,
        [PREVIOUS_INBOX_NOTICE]
    );
    assert_eq!(result.workspace.inbox.pull_requests[0].body, OLD);
    let page = result.workspace.pr.unwrap();
    assert_eq!(page.details.summary.pr.id, "GENUINE_PR_NODE_1007");
    assert_eq!(page.details.summary.pr.body, FRESH);
    assert_eq!(page.details.summary.pr.author.login, "actual-pr-author");
    assert_eq!(page.tab, PrTab::Summary);
    assert!(
        page.context.is_none()
            && page.checks.is_none()
            && page.review_progress.is_none()
            && page.diff.is_none()
    );
    assert_eq!(page.details.comments.len(), 1);
    assert_eq!(page.details.commits.len(), 1);
    assert_eq!(page.details.files[0].path, "src/界.rs");
    assert_eq!(sql_state(&fixture), before);
    assert_eq!(git(&fixture, &["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(
        git(&fixture, &["status", "--porcelain=v2", "-z"]),
        before_git
    );
    assert_eq!(
        fs::read(fixture.repo().join("dirty界.txt")).unwrap(),
        b"untouched dirty\n"
    );
}

#[test]
fn scoped_pr_read_same_id_rename_fresh_page_pure_back_original_cached_selection_no_queued_write() {
    let fixture = Fixture::new();
    let mut steps = source_steps();
    steps.extend(operation("example/renamed", 42));
    fixture.scripted_gh(&steps);
    let original = load(&fixture);
    let scope = active(&original);
    let before = sql_state(&fixture);
    let result = call(
        &fixture,
        original.clone(),
        scope.clone(),
        previous(&original),
    )
    .unwrap();
    assert_eq!(result.selected_original.last_name, REPO);
    assert_eq!(result.selected_canonical.repository, "example/renamed");
    assert_eq!(result.closed_identity.login, "renamed-same-principal");
    assert_eq!(result.workspace.identity.login, "renamed-same-principal");
    assert_eq!(result.workspace.selected.as_ref().unwrap().repository, REPO);
    let mut app = App {
        github: Some(result.workspace),
        remote_scope: Some(scope.clone()),
        remote_scope_source: Some(original.clone()),
        ..Default::default()
    };
    app.dispatch(Action::Github(GithubAction::Back));
    let back = app.github.as_ref().unwrap();
    assert!(back.pr.is_none());
    assert_eq!(back.selected_index(), 0);
    assert_eq!(back.selected.as_ref().unwrap().repository, REPO);
    assert!(matches!(
        back.freshness,
        Freshness::Cached { fetched_at: 991 }
    ));
    assert!(!back.inbox.coverage.complete);
    assert!(
        app.github_action.is_none()
            && app.pr_action.is_none()
            && app.pr_pending_message.is_empty()
            && app.form.is_none()
    );
    assert!(Arc::ptr_eq(app.remote_scope.as_ref().unwrap(), &scope));
    assert!(Arc::ptr_eq(
        app.remote_scope_source.as_ref().unwrap(),
        &original
    ));
    assert_eq!(sql_state(&fixture), before);
    exact(&fixture, &steps);
}

#[test]
fn scoped_pr_read_exact_selected_member_only_default_empty_fenced_foreign_or_unselected_no_query_sql(
) {
    for kind in 0..7 {
        let fixture = Fixture::new();
        let mut steps = source_steps();
        steps.extend(principal(42, "octocat"));
        fixture.scripted_gh(&steps);
        let original = load(&fixture);
        let mut workspace = previous(&original);
        let mut scope = active(&original);
        let mut key = workspace.selected.clone().unwrap();
        match kind {
            0 => {
                scope = Arc::new(
                    ActiveRemoteScopeV1::default_personal(original.source().identity()).unwrap(),
                )
            }
            1 => {
                scope = Arc::new(
                    ActiveRemoteScopeV1::explicit_selection(original.source(), &[]).unwrap(),
                )
            }
            2 => {
                scope = Arc::new(ActiveRemoteScopeV1::unavailable(
                    crate::active_remote_scope_v1::RemoteScopeFenceV1::PrivacyFence,
                ))
            }
            3 => key.number = 99,
            4 => workspace.inbox.identity.account_id = Some(43),
            5 => workspace.filter = InboxFilter::Assigned,
            _ => {
                key.repository = "foreign/private".into();
                workspace.selected = Some(key.clone());
                workspace.inbox.pull_requests[0].repository = key.repository.clone();
            }
        }
        let cold = fixture.root.join("unopened/cache.sqlite3");
        let error = open(
            original,
            scope,
            workspace,
            &key,
            &provider(&fixture),
            &cold,
            &CancellationToken::default(),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            ScopedPrReadFailureV1::ScopeUnavailable
                | ScopedPrReadFailureV1::SelectedOriginalUnavailable
        ));
        safe(&error);
        assert!(!cold.parent().unwrap().exists());
        exact(&fixture, &steps);
    }
}

#[test]
fn scoped_pr_read_first_repo_unknown_denied_recreated_or_missing_nodes_closes_without_private_sql()
{
    for kind in 0..6 {
        let fixture = Fixture::new();
        let mut steps = source_steps();
        steps.extend(principal(42, "octocat"));
        let mut value = repo(410, REPO);
        match kind {
            0 => value["permissions"]["pull"] = Value::Null,
            1 => value["node_id"] = Value::Null,
            2 => value["id"] = json!(999),
            3 => value["owner"]["id"] = json!(999),
            4 => value["owner"]["node_id"] = Value::Null,
            _ => {}
        }
        let mut read = get(&format!("repos/{REPO}"), value);
        if kind == 5 {
            read.status = 1;
            read.stdout.clear();
            read.stderr = b"gh: PRIVATE_DIAGNOSTIC (HTTP 403)".to_vec();
        }
        steps.push(read);
        steps.extend(principal(42, "octocat"));
        fixture.scripted_gh(&steps);
        let original = load(&fixture);
        fs::set_permissions(database(&fixture), fs::Permissions::from_mode(0o644)).unwrap();
        let before = sql_state(&fixture);
        let error = call(
            &fixture,
            original.clone(),
            active(&original),
            previous(&original),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            ScopedPrReadFailureV1::CurrentRepositoryUnavailable
        ));
        safe(&error);
        assert_eq!(steps.len(), 12);
        exact(&fixture, &steps);
        assert_eq!(
            fs::metadata(database(&fixture))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o644,
            "no Cache open/repair after missing initial access"
        );
        assert_eq!(sql_state(&fixture), before);
    }
}

#[test]
fn scoped_pr_read_pr_node_head_or_base_moving_refuses_after_detail7_before_closing_sql() {
    for kind in 0..3 {
        let fixture = Fixture::new();
        let mut steps = source_steps();
        let mut op = operation(REPO, 42);
        op.truncate(10);
        let mut moved = summary(REPO);
        match kind {
            0 => moved["node_id"] = json!("REPLACED_PR_NODE"),
            1 => moved["head"]["sha"] = json!("d".repeat(40)),
            _ => moved["base"]["sha"] = json!("e".repeat(40)),
        };
        op[9] = get(&format!("repos/{REPO}/pulls/7"), moved);
        steps.extend(op);
        fixture.scripted_gh(&steps);
        let original = load(&fixture);
        let before = sql_state(&fixture);
        let error = call(
            &fixture,
            original.clone(),
            active(&original),
            previous(&original),
        )
        .unwrap_err();
        assert!(matches!(error, ScopedPrReadFailureV1::DetailsUnavailable));
        safe(&error);
        exact(&fixture, &steps);
        assert_eq!(sql_state(&fixture), before);
    }
}

#[test]
fn scoped_pr_read_short_sha_control_node_or_missing_ref_never_prepares_canonical_page() {
    for kind in 0..3 {
        let fixture = Fixture::new();
        let mut steps = source_steps();
        let mut op = operation(REPO, 42);
        op.truncate(10);
        let mut unknown = summary(REPO);
        match kind {
            0 => unknown["head"]["sha"] = json!("a".repeat(7)),
            1 => unknown["node_id"] = json!("PRIVATE_DIAGNOSTIC\nNODE"),
            _ => unknown["base"]["ref"] = Value::Null,
        };
        op[3] = get(&format!("repos/{REPO}/pulls/7"), unknown.clone());
        op[9] = get(&format!("repos/{REPO}/pulls/7"), unknown);
        steps.extend(op);
        fixture.scripted_gh(&steps);
        let original = load(&fixture);
        let before = sql_state(&fixture);
        let error = call(
            &fixture,
            original.clone(),
            active(&original),
            previous(&original),
        )
        .unwrap_err();
        assert!(matches!(error, ScopedPrReadFailureV1::CanonicalChanged));
        safe(&error);
        exact(&fixture, &steps);
        assert_eq!(sql_state(&fixture), before);
    }
}

#[test]
fn scoped_pr_read_closing_repository_changed_name_permission_owner_or_id_hides_all_details() {
    for kind in 0..4 {
        let fixture = Fixture::new();
        let mut steps = source_steps();
        let mut op = operation(REPO, 42);
        op.truncate(11);
        let mut changed = repo(410, REPO);
        match kind {
            0 => changed["full_name"] = json!("example/moved-after-details"),
            1 => changed["permissions"]["pull"] = Value::Null,
            2 => changed["owner"]["id"] = json!(999),
            _ => changed["id"] = json!(999),
        };
        op[10] = get(&format!("repos/{REPO}"), changed);
        steps.extend(op);
        fixture.scripted_gh(&steps);
        let original = load(&fixture);
        let before = sql_state(&fixture);
        let error = call(
            &fixture,
            original.clone(),
            active(&original),
            previous(&original),
        )
        .unwrap_err();
        assert!(matches!(error, ScopedPrReadFailureV1::CanonicalChanged));
        safe(&error);
        exact(&fixture, &steps);
        assert_eq!(sql_state(&fixture), before);
    }
}

#[test]
fn scoped_pr_read_pre_and_final_uid_drift_or_auth_failure_never_returns_private_model_or_43_sql() {
    for kind in 0..3 {
        let fixture = Fixture::new();
        let mut steps = source_steps();
        match kind {
            0 => steps.extend(principal(43, "octocat")),
            1 => steps.extend(operation(REPO, 43)),
            _ => {
                let mut op = operation(REPO, 42);
                op[12].stdout.clear();
                op[12].stderr = b"gh: PRIVATE_DIAGNOSTIC (HTTP 401)".to_vec();
                op[12].status = 1;
                steps.extend(op);
            }
        }
        fixture.scripted_gh(&steps);
        let original = load(&fixture);
        let before = sql_state(&fixture);
        let error = call(
            &fixture,
            original.clone(),
            active(&original),
            previous(&original),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            ScopedPrReadFailureV1::OriginalPrincipal(_)
                | ScopedPrReadFailureV1::ClosingPrincipal(_)
        ));
        safe(&error);
        exact(&fixture, &steps);
        assert_eq!(sql_state(&fixture), before);
        let count:u64=sql(&fixture).query_row("SELECT (SELECT count(*) FROM records WHERE login='user-id:43')+(SELECT count(*) FROM drafts WHERE login='user-id:43')+(SELECT count(*) FROM saved_views WHERE login='user-id:43')",[],|row|row.get(0)).unwrap();
        assert_eq!(count, 0);
    }
}

#[test]
fn scoped_pr_read_original_sql_cas_is_not_rebased_no_final_auth_or_model_after_actual_concurrent_edit(
) {
    let fixture = Fixture::new();
    let mut steps = source_steps();
    let mut op = operation(REPO, 42);
    op.truncate(11);
    op[10].delay_ms = 250;
    steps.extend(op);
    fixture.scripted_gh(&steps);
    let original = load(&fixture);
    let scope = active(&original);
    let path = database(&fixture);
    let key = original.saved_view_key().to_owned();
    let journal = fixture.root.join("gh-fixture/commands.jsonl");
    let concurrent = thread::spawn(move || {
        wait_count(&journal, 18);
        let db = Connection::open(path).unwrap();
        db.execute("INSERT INTO saved_views(host,login,name,payload) VALUES(?1,'user-id:42',?2,?3) ON CONFLICT(host,login,name) DO UPDATE SET payload=excluded.payload",rusqlite::params![HOST,key,"{\"concurrent\":\"PRIVATE_SQL_CANARY\"}"]).unwrap();
    });
    let error = call(
        &fixture,
        original.clone(),
        scope.clone(),
        previous(&original),
    )
    .unwrap_err();
    concurrent.join().unwrap();
    assert!(matches!(
        error,
        ScopedPrReadFailureV1::OriginalBoundary(CatalogueIntentBoundaryError::SavedIntentChanged)
    ));
    safe(&error);
    exact(&fixture, &steps);
    let bytes: String = sql(&fixture)
        .query_row(
            "SELECT payload FROM saved_views WHERE name=?1",
            [original.saved_view_key()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(bytes, "{\"concurrent\":\"PRIVATE_SQL_CANARY\"}");
    let RemoteScopeConsumerV1::Explicit(refs) = scope.consumer(original.source().identity()) else {
        panic!("original intent must remain explicit")
    };
    assert_eq!(refs.references().len(), 2);
}

fn wait_count(path: &Path, count: usize) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if fs::read_to_string(path)
            .ok()
            .is_some_and(|text| text.lines().count() >= count)
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "actual delayed child never launched"
        );
        thread::sleep(Duration::from_millis(2));
    }
}
#[test]
fn scoped_pr_read_reservation_failed_capture_cancellation_and_same_deadline_stop_before_next_child()
{
    for kind in 0..4 {
        let fixture = Fixture::new();
        let mut steps = source_steps();
        if kind == 0 {
            steps.extend(principal(42, "octocat"));
        } else {
            let mut op = operation(REPO, 42);
            op.truncate(4);
            match kind {
                1 => op[3].stdout = "🦀".repeat(300000).into_bytes(),
                _ => op[3].delay_ms = 2000,
            };
            steps.extend(op);
        }
        fixture.scripted_gh(&steps);
        let original = load(&fixture);
        let workspace = previous(&original);
        let key = workspace.selected.clone().unwrap();
        let before = sql_state(&fixture);
        let cancel = CancellationToken::default();
        let mut limits = RepositoryCatalogueAccessLimits::default();
        limits.transport.max_response_bytes = WHOLE_CAPTURE;
        if kind == 0 {
            limits.transport.max_response_bytes = 1024 * 1024;
        }
        if kind == 3 {
            limits.transport.max_duration = Duration::from_millis(1000);
        }
        let mut cancelled = None;
        if kind == 2 {
            let journal = fixture.root.join("gh-fixture/commands.jsonl");
            let token = cancel.clone();
            cancelled = Some(thread::spawn(move || {
                wait_count(&journal, 11);
                token.cancel();
            }));
        }
        let error = open_with_limits(
            original.clone(),
            active(&original),
            workspace,
            &key,
            &provider(&fixture),
            &database(&fixture),
            &limits,
            &cancel,
        )
        .unwrap_err();
        if let Some(thread) = cancelled {
            thread.join().unwrap();
        }
        assert!(matches!(
            error,
            ScopedPrReadFailureV1::CaptureUnavailable | ScopedPrReadFailureV1::DetailsUnavailable
        ));
        safe(&error);
        exact(&fixture, &steps);
        assert_eq!(sql_state(&fixture), before);
    }
}
