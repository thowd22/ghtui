//! Single test-only genuine Source/access/Actions factory fixture; no DTO capabilities.
use crate::{
    active_remote_scope_v1::ActiveRemoteScopeV1,
    app::{App, View},
    providers::{gh::*, runner::CancellationToken},
    repository_catalogue_intent_v1::IntentReference,
    repository_catalogue_save_v1::load_catalogue_intent_source,
    scoped_actions_native_v1 as native,
    scoped_actions_v1::*,
    storage::{Cache, Scope},
    test_support::{Fixture, GhStep},
};
use serde_json::{json, Value};
use std::{fs, path::PathBuf, sync::Arc};
pub(crate) const HOST: &str = "github.invalid";
pub(crate) const REPO: &str = "example/repo0000";
pub(crate) fn get(path: &str, value: Value) -> GhStep {
    GhStep::read(
        &["api", path, "--hostname", HOST, "--method", "GET"],
        &value.to_string(),
    )
}
pub(crate) fn list(path: &str, value: Value) -> GhStep {
    let mut step = get(path, json!([value]));
    step.args.extend(["--paginate".into(), "--slurp".into()]);
    step
}
pub(crate) fn principal(uid: u64) -> Vec<GhStep> {
    crate::repository_catalogue_read_limits_test_support_v1::principal(uid)
}
pub(crate) fn repo() -> Value {
    crate::repository_catalogue_read_limits_test_support_v1::row(0)
}
pub(crate) fn run() -> Value {
    json!({"id":123,"node_id":"WR_123","workflow_id":7,"check_suite_id":901,"name":"Source CI","display_title":"ACTUAL_CATALOGUE_RUN",
        "head_sha":"a".repeat(40),"head_branch":"main","event":"push","status":"completed","conclusion":"failure","run_attempt":2,
        "created_at":"2026-10-15T00:00:00Z","updated_at":"2026-10-15T00:01:00Z",
        "actor":{"id":51,"login":"actual-ci-actor","type":"User"},"triggering_actor":{"id":42,"login":"octocat","type":"User"},"repository":repo()})
}
pub(crate) fn job() -> Value {
    json!({"id":501,"node_id":"JOB_501","run_id":123,"head_sha":"a".repeat(40),"name":"test (stable, linux)","status":"completed","conclusion":"failure",
        "url":format!("https://{HOST}/api/v3/repos/{REPO}/actions/jobs/501"),
        "check_run_url":format!("https://{HOST}/api/v3/repos/{REPO}/check-runs/600"),
        "steps":[{"number":2,"name":"Actual failed step","status":"completed","conclusion":"failure"}]})
}
pub(crate) fn check() -> Value {
    json!({"id":600,"node_id":"CHECK_600","head_sha":"a".repeat(40),"check_suite":{"id":901},
    "url":format!("https://{HOST}/api/v3/repos/{REPO}/check-runs/600")})
}
pub(crate) fn annotations() -> Value {
    json!([{"path":"src/界.rs","start_line":7,"end_line":9,"annotation_level":"failure","title":"Actual diagnostic","message":"PRIVATE_CATALOGUE_ANNOTATION","raw_details":"metadata only"}])
}
pub(crate) fn initial_steps() -> Vec<GhStep> {
    let mut steps =
        crate::repository_catalogue_read_limits_test_support_v1::collection(&[repo()], 1, 42, 42);
    steps.extend(principal(42));
    steps.push(get(&format!("repos/{REPO}"), repo()));
    steps.push(list(
        &format!("repos/{REPO}/actions/runs?per_page=100"),
        json!({"total_count":1,"workflow_runs":[run()]}),
    ));
    steps.push(get(&format!("repos/{REPO}"), repo()));
    steps.extend(principal(42));
    assert_eq!(steps.len(), 14);
    steps
}
pub(crate) fn operation(kind: ScopedActionsRequestKindV1, final_uid: u64) -> Vec<GhStep> {
    let mut steps = principal(42);
    steps.push(get(&format!("repos/{REPO}"), repo()));
    if kind == ScopedActionsRequestKindV1::Catalogue {
        steps.push(list(
            &format!("repos/{REPO}/actions/runs?per_page=100"),
            json!({"total_count":1,"workflow_runs":[run()]}),
        ));
    } else {
        steps.push(get(&format!("repos/{REPO}/actions/runs/123"), run()));
        steps.push(get(
            &format!("repos/{REPO}/actions/runs/123/attempts/2"),
            run(),
        ));
        steps.push(list(
            &format!("repos/{REPO}/actions/runs/123/attempts/2/jobs?per_page=100"),
            json!({"total_count":1,"jobs":[job()]}),
        ));
        if matches!(
            kind,
            ScopedActionsRequestKindV1::Logs | ScopedActionsRequestKindV1::Annotations
        ) {
            steps.push(get(&format!("repos/{REPO}/actions/jobs/501"), job()));
            if kind == ScopedActionsRequestKindV1::Logs {
                steps.push(GhStep::read(
                    &[
                        "run",
                        "view",
                        "123",
                        "--repo",
                        &format!("{HOST}/{REPO}"),
                        "--attempt",
                        "2",
                        "--log",
                        "--job",
                        "501",
                    ],
                    "PRIVATE_EXACT_JOB_501_LOG\nactual failed step\n",
                ));
            } else {
                steps.push(get(&format!("repos/{REPO}/check-runs/600"), check()));
                steps.push(list(
                    &format!("repos/{REPO}/check-runs/600/annotations?per_page=100"),
                    annotations(),
                ));
                steps.push(get(&format!("repos/{REPO}/check-runs/600"), check()));
            }
            steps.push(get(&format!("repos/{REPO}/actions/jobs/501"), job()));
        }
        steps.push(get(&format!("repos/{REPO}/actions/runs/123"), run()));
    }
    steps.push(get(&format!("repos/{REPO}"), repo()));
    steps.extend(principal(final_uid));
    steps
}
pub(crate) fn database(f: &Fixture) -> PathBuf {
    f.root.join("cache.sqlite3")
}
pub(crate) fn provider(f: &Fixture) -> GithubProvider {
    GithubProvider::new(HOST)
        .unwrap()
        .with_program(f.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", f.root.join("gh-fixture"))
}
pub(crate) fn setup(mut tail: Vec<GhStep>) -> (Fixture, GithubProvider, App, Vec<GhStep>) {
    let f = Fixture::new();
    let mut all = initial_steps();
    all.append(&mut tail);
    f.scripted_gh(&all);
    git(&f, &["init", "-b", "main"]);
    git(&f, &["config", "user.name", "Actions fixture"]);
    git(&f, &["config", "user.email", "actions@example.invalid"]);
    fs::write(f.repo().join("tracked"), b"saved original\n").unwrap();
    git(&f, &["add", "tracked"]);
    git(&f, &["commit", "-m", "original Actions fixture"]);
    fs::write(f.repo().join("tracked"), b"LOCAL_DIRTY_ACTIONS_CANARY\n").unwrap();
    let p = provider(&f);
    let mut cache = Cache::open(&database(&f)).unwrap();
    cache
        .put_draft(
            &Scope::github(HOST, Some(42)).unwrap(),
            "keep-actions",
            "ORIGINAL_ACTIONS_DRAFT",
        )
        .unwrap();
    let source = Arc::new(
        load_catalogue_intent_source(
            &p,
            &mut cache,
            None,
            &RepositoryCatalogueSource::Organization("example".into()),
            &Default::default(),
            &Default::default(),
        )
        .unwrap(),
    );
    let raw = &source.source().repositories()[0];
    let reference = IntentReference {
        id: raw.id,
        node_id: raw.node_id.clone(),
        last_name: raw.full_name.clone(),
    };
    let token = CancellationToken::default();
    let mut session = p
        .begin_repository_catalogue_access(source.source(), &Default::default(), &token)
        .unwrap();
    session.verify(&reference, &token).unwrap();
    let window = session
        .reserve_external_reads(1, 1024 * 1024, &token)
        .unwrap();
    let bounded = p
        .clone()
        .with_scoped_read_window(window.deadline(), window.max_output_bytes())
        .unwrap();
    let read = bounded
        .scoped_actions_catalogue_v1(
            session.observed_original_v1(reference.id).unwrap(),
            &Default::default(),
            &token,
        )
        .unwrap();
    let seed = prepare_initial_read(read, session.identity(), &token).unwrap();
    session.verify(&reference, &token).unwrap();
    let closed = session
        .close_with_guard(&token, || {
            cache
                .validate_repository_catalogue_intent_source(&source)
                .map_err(|_| GithubError {
                    kind: GithubErrorKind::IdentityChanged,
                    message: "Original test SQL changed".into(),
                })
        })
        .unwrap();
    let mut active =
        ActiveRemoteScopeV1::explicit_selection(source.source(), &[reference.id]).unwrap();
    active
        .observe_closed_access(source.source(), &closed)
        .unwrap();
    let initial = initial(
        source,
        Arc::new(active),
        reference,
        seed,
        &closed,
        &p,
        &token,
    )
    .unwrap();
    let mut app = App {
        settings_host: HOST.into(),
        view: View::Actions,
        ..Default::default()
    };
    native::install_initial(&mut app, initial);
    // View installation asks the real event loop to retire the old PR-check
    // continuation, even when none existed. Match main's empty-cancel drain
    // before the next input; never drop a live watch, request or owner here.
    assert!(app.pr_check_continuation_cancel_v2);
    assert!(app.pr_check_watch_v2.is_none());
    assert!(app.pr_check_continuation_request_v2.is_none());
    assert!(app.pr_check_continuation_owner_v2.is_none());
    app.pr_check_continuation_cancel_v2 = false;
    (f, p, app, all)
}
pub(crate) fn read(
    app: &mut App,
    p: &GithubProvider,
    f: &Fixture,
    action: crate::actions_workspace::ActionsAction,
) -> Result<PreparedScopedActionsV1, String> {
    assert!(native::intercept(app, &action));
    let work = native::take_work(app).expect("Actual captured original Actions work");
    crate::scoped_actions_v1::execute(work, p, &database(f))
}
pub(crate) fn install(app: &mut App, prepared: PreparedScopedActionsV1) {
    let owner = native::ScopedActionsOwnerV1::new(prepared.work.clone()).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    assert!(native::publish(app, &owner, prepared));
    app.scoped_actions_owner_v1 = None;
}
pub(crate) fn opened(tail: Vec<GhStep>) -> (Fixture, GithubProvider, App, Vec<GhStep>) {
    let mut op = operation(ScopedActionsRequestKindV1::Attempt, 42);
    op.extend(tail);
    let (f, p, mut app, all) = setup(op);
    let value = read(
        &mut app,
        &p,
        &f,
        crate::actions_workspace::ActionsAction::OpenRun {
            repository: REPO.into(),
            id: 123,
            attempt: 2,
        },
    )
    .unwrap();
    install(&mut app, value);
    (f, p, app, all)
}
pub(crate) fn exact(f: &Fixture, steps: &[GhStep]) {
    let rows = fs::read_to_string(f.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        rows.len(),
        steps.len(),
        "Exact GH children, no background/default fallback"
    );
    for (actual, expected) in rows.iter().zip(steps) {
        assert_eq!(actual["args"], json!(expected.args));
        assert_eq!(actual["matched"], true);
        assert_eq!(actual["stdin_hex"], "");
        assert!(!expected
            .args
            .iter()
            .any(|v| matches!(v.as_str(), "POST" | "PATCH" | "PUT" | "DELETE" | "graphql")));
    }
    assert_eq!(
        Cache::open(&database(f))
            .unwrap()
            .draft(&Scope::github(HOST, Some(42)).unwrap(), "keep-actions")
            .unwrap()
            .as_deref(),
        Some("ORIGINAL_ACTIONS_DRAFT")
    );
}
pub(crate) fn sql_state(f: &Fixture) -> Vec<(String, String, String)> {
    let db = rusqlite::Connection::open_with_flags(
        database(f),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let mut statement = db
        .prepare("SELECT host,login,body FROM drafts ORDER BY host,login,resource")
        .unwrap();
    let rows = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    rows
}
pub(crate) fn git(f: &Fixture, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(["-c", "diff.autoRefreshIndex=false"])
        .args(args)
        .current_dir(f.repo())
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
pub(crate) fn git_state(f: &Fixture) -> (String, String, Vec<u8>, Vec<u8>, Vec<u8>) {
    (
        git(f, &["rev-parse", "HEAD"]),
        git(f, &["status", "--porcelain=v1"]),
        fs::read(f.repo().join(".git/index")).unwrap(),
        fs::read(f.repo().join("tracked")).unwrap(),
        fs::read(f.repo().join(".git/config")).unwrap(),
    )
}
