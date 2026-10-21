//! UNDECLARED child of scoped_pr_resources_v1_tests. Genuine Source7/PR13/
//! Checks10 precede every bridge. No opaque admission/raw receipt is fabricated.
use super::*;
#[path = "scoped_actions_history_pr_v5_tests.rs"]
mod actions_history_v5;
#[path = "scoped_actions_rerun_pr_v4_tests.rs"]
mod actions_rerun_v4;
#[path = "scoped_actions_pr_origin_v2_tests.rs"]
mod actions_resources_v2;
#[path = "scoped_actions_workflow_pr_v6_tests.rs"]
mod actions_workflow_v6;
#[path = "scoped_pr_check_choices_v1_tests.rs"]
mod choices;
#[path = "scoped_pr_checks_pending_v1_tests.rs"]
mod pending;
use crate::{
    actions_workspace::{ActionsAction, ActionsCommand},
    app::View,
    pr_checks_native::{self, scoped::*},
    runtime::{Background, PreparedView},
};
use std::time::{Duration, Instant};

fn bridge_repo(name: &str) -> Value {
    let mut value = repo(410, name);
    value["url"] = json!(format!("https://{HOST}/api/v3/repos/{name}"));
    value
}
fn actual_check(name: &str) -> Value {
    let mut value = check_value(600);
    value["url"] = json!(format!("https://{HOST}/api/v3/repos/{name}/check-runs/600"));
    value
}
fn actual_run(name: &str) -> Value {
    json!({"id":123,"node_id":"WR_123","workflow_id":7,"check_suite_id":901,"run_attempt":2,
        "head_sha":"a".repeat(40),"head_branch":"feature/scoped-read","name":"Exact CI",
        "display_title":"SCOPED_RUN_TITLE","event":"pull_request","status":"completed","conclusion":"failure",
        "url":format!("https://{HOST}/api/v3/repos/{name}/actions/runs/123"),
        "html_url":format!("https://{HOST}/{name}/actions/runs/123"),"repository":bridge_repo(name),
        "actor":{"id":51,"login":"alice","type":"User"},"triggering_actor":{"id":42,"login":"octocat","type":"User"}})
}
fn actual_job(name: &str) -> Value {
    json!({"id":501,"node_id":"J_501","run_id":123,"head_sha":"a".repeat(40),"name":"test (stable)",
        "status":"completed","conclusion":"failure","url":format!("https://{HOST}/api/v3/repos/{name}/actions/jobs/501"),
        "check_run_url":format!("https://{HOST}/api/v3/repos/{name}/check-runs/600"),
        "steps":[{"number":1,"name":"Compile exact source","status":"completed","conclusion":"success"},
            {"number":2,"name":"Test exact source","status":"completed","conclusion":"failure"}]})
}
fn bridge_user(uid: u64) -> GhStep {
    get("user", json!({"id":uid,"login":"same-login","type":"User"}))
}
fn bridge_steps(name: &str) -> Vec<GhStep> {
    let mut op = principal(42, "bridge-current");
    op.push(get(&format!("repos/{REPO}"), repo(410, name)));
    op.extend([
        bridge_user(42),
        get(&format!("repos/{name}"), bridge_repo(name)),
        get(&format!("repos/{name}/pulls/7"), resource_parent(name)),
        get(&format!("repos/{name}/check-runs/600"), actual_check(name)),
        get(
            &format!(
                "repos/{name}/actions/runs?check_suite_id=901&head_sha={}&per_page=100&page=1",
                "a".repeat(40)
            ),
            json!({"total_count":1,"workflow_runs":[actual_run(name)]}),
        ),
        get(
            &format!("repos/{name}/actions/runs/123/attempts/2"),
            actual_run(name),
        ),
        get(
            &format!("repos/{name}/actions/runs/123/attempts/2/jobs?per_page=100&page=1"),
            json!({"total_count":1,"jobs":[actual_job(name)]}),
        ),
        get(&format!("repos/{name}/actions/runs/123"), actual_run(name)),
        get(&format!("repos/{name}/check-runs/600"), actual_check(name)),
        get(&format!("repos/{name}/pulls/7"), resource_parent(name)),
        get(&format!("repos/{name}"), bridge_repo(name)),
        bridge_user(42),
        get(&format!("repos/{REPO}"), repo(410, name)),
    ]);
    op.extend(principal(42, "bridge-final-login"));
    assert_eq!(op.len(), 18);
    op
}
fn log_steps(body: &str) -> Vec<GhStep> {
    let mut op = principal(42, "log-current");
    op.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    op.extend([
        bridge_user(42),
        get(&format!("repos/{REPO}"), bridge_repo(REPO)),
        get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)),
        get(&format!("repos/{REPO}/actions/runs/123"), actual_run(REPO)),
        GhStep::read(
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
            body,
        ),
        get(&format!("repos/{REPO}/actions/runs/123"), actual_run(REPO)),
        get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)),
        get(&format!("repos/{REPO}"), bridge_repo(REPO)),
        bridge_user(42),
        get(&format!("repos/{REPO}"), repo(410, REPO)),
    ]);
    op.extend(principal(42, "log-final-login"));
    assert_eq!(op.len(), 15);
    op
}
fn whole(op: &[GhStep]) -> Vec<GhStep> {
    let mut steps = script(&resource_steps(PrResourceKindV1::Checks, REPO));
    steps.extend_from_slice(op);
    steps
}
fn app_checked(fixture: &Fixture) -> App {
    let (source, active, mut github) = opened(fixture);
    drop(crate::actions_write_journal::ActionsWriteJournal::open(&database(fixture)).unwrap());
    let result = run_resource(
        fixture,
        source.clone(),
        active.clone(),
        &mut github,
        PrResourceKindV1::Checks,
        &Default::default(),
    )
    .unwrap();
    install_resource_if_current(&mut github, result).unwrap();
    let selected = github
        .pr
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .enumerate()
        .find(|(i, _)| {
            github
                .scoped_pr_resource
                .as_ref()
                .unwrap()
                .check(*i)
                .is_some_and(|check| check.id == 600)
        })
        .unwrap()
        .0;
    App {
        view: View::Inbox,
        github: Some(github),
        remote_scope: Some(active),
        remote_scope_source: Some(source),
        inbox_list: crate::ui_table::ListState {
            selected,
            top: selected.saturating_sub(2),
        },
        ..Default::default()
    }
}
fn input(app: &mut App) -> ScopedPrCheckInputV1 {
    let action = scoped_open_action(app);
    assert_eq!(
        action,
        Action::PrChecks(pr_checks_native::PrChecksUiAction::Open)
    );
    app.dispatch(action);
    take_open_v1(app).unwrap()
}
fn load_bridge(
    fixture: &Fixture,
    app: &mut App,
    limits: &ScopedPrCheckReadLimitsV1,
) -> PreparedView {
    load_bridge_using(fixture, app, limits, &provider(fixture))
}
fn load_bridge_using(
    fixture: &Fixture,
    app: &mut App,
    limits: &ScopedPrCheckReadLimitsV1,
    retained_provider: &GithubProvider,
) -> PreparedView {
    load_v1(
        input(app),
        retained_provider,
        &database(fixture),
        limits,
        &CancellationToken::default(),
    )
    .unwrap()
}
fn install(app: &mut App, prepared: PreparedView) {
    assert!(!prepared.clear_private);
    install_v1(app, *prepared.scoped_pr_checks.unwrap()).unwrap();
}
fn read_only(
    fixture: &Fixture,
    before: &[String],
    git_before: &(String, String, Vec<u8>, Vec<u8>),
    steps: &[GhStep],
) {
    effects(fixture, before, git_before, steps);
    let count: u64 = sql(fixture)
        .query_row("SELECT count(*) FROM actions_write_attempts", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 0);
}
fn failure(op: Vec<GhStep>, limits: ScopedPrCheckReadLimitsV1) {
    let steps = whole(&op);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let mut app = app_checked(&fixture);
    let before = sql_state(&fixture);
    let git_before = git_state_resource(&fixture);
    let rows = app
        .github
        .as_ref()
        .unwrap()
        .pr
        .as_ref()
        .unwrap()
        .rows
        .as_ptr();
    let result = load_bridge(&fixture, &mut app, &limits);
    assert!(result.clear_private);
    assert!(result.scoped_pr_checks.is_none());
    assert!(!result.status.contains(DRAFT));
    assert!(!result.status.contains("PRIVATE_DIAGNOSTIC"));
    // A worker failure requests private hiding; it does not take, clone or
    // mutate the foreground page before the exact-owner publisher applies it.
    assert_eq!(
        app.github
            .as_ref()
            .unwrap()
            .pr
            .as_ref()
            .unwrap()
            .rows
            .as_ptr(),
        rows
    );
    read_only(&fixture, &before, &git_before, &steps);
}

#[test]
fn scoped_check_raw_original_repo_database_replacement_refuses_before_sql() {
    let mut op = bridge_steps(REPO);
    op[4] = get(&format!("repos/{REPO}"), bridge_repo(REPO));
    let mut raw = bridge_repo(REPO);
    raw["id"] = json!(411);
    op[4] = get(&format!("repos/{REPO}"), raw);
    op.truncate(5);
    failure(op, Default::default());
}
#[test]
fn scoped_check_raw_original_repo_node_owner_namespace_refuses_before_sql() {
    for field in ["node", "owner"] {
        let mut op = bridge_steps(REPO);
        let mut raw = bridge_repo(REPO);
        if field == "node" {
            raw["node_id"] = json!("RECREATED_NODE");
        } else {
            raw["owner"]["id"] = json!(72);
        }
        op[4] = get(&format!("repos/{REPO}"), raw);
        op.truncate(5);
        failure(op, Default::default());
    }
}
#[test]
fn scoped_check_raw_positive_pr_database_cannot_alias_issue_search_id() {
    let mut op = bridge_steps(REPO);
    let mut raw = resource_parent(REPO);
    raw["id"] = json!(2007);
    op[5] = get(&format!("repos/{REPO}/pulls/7"), raw);
    op.truncate(6);
    failure(op, Default::default());
}
#[test]
fn scoped_check_raw_optional_head_repository_replacement_and_disappearance_refuse() {
    for head in [
        Value::Null,
        json!({"id":992,"node_id":"R_992","full_name":"fork/source"}),
    ] {
        let mut op = bridge_steps(REPO);
        let mut raw = resource_parent(REPO);
        raw["head"]["repo"] = head;
        op[5] = get(&format!("repos/{REPO}/pulls/7"), raw);
        op.truncate(6);
        failure(op, Default::default());
    }
}
#[test]
fn scoped_check_raw_known_check_node_and_suite_contradiction_stop_before_sql() {
    for field in ["node", "suite"] {
        let mut op = bridge_steps(REPO);
        let mut raw = actual_check(REPO);
        if field == "node" {
            raw["node_id"] = json!("NEW_CHECK_NODE");
        } else {
            raw["check_suite"]["id"] = json!(902);
        }
        op[6] = get(&format!("repos/{REPO}/check-runs/600"), raw);
        op.truncate(7);
        failure(op, Default::default());
    }
}
#[test]
fn scoped_check_raw_head_base_ref_lifecycle_drift_never_retargets() {
    for field in ["head", "base", "ref", "state"] {
        let mut op = bridge_steps(REPO);
        let mut raw = resource_parent(REPO);
        match field {
            "head" => raw["head"]["sha"] = json!("c".repeat(40)),
            "base" => raw["base"]["sha"] = json!("c".repeat(40)),
            "ref" => raw["head"]["ref"] = json!("another"),
            _ => raw["state"] = json!("closed"),
        }
        op[5] = get(&format!("repos/{REPO}/pulls/7"), raw);
        op.truncate(6);
        failure(op, Default::default());
    }
}
#[test]
fn scoped_check_raw_original_full_job_attempt_omission_is_not_null_or_default_one() {
    for attempt in [Value::Null, json!(1), json!("2")] {
        let mut op = bridge_steps(REPO);
        let mut raw = actual_job(REPO);
        raw["run_attempt"] = attempt;
        op[9] = get(
            &format!("repos/{REPO}/actions/runs/123/attempts/2/jobs?per_page=100&page=1"),
            json!({"total_count":1,"jobs":[raw]}),
        );
        op.truncate(10);
        failure(op, Default::default());
    }
}
#[test]
fn scoped_check_raw_closing_pr_source_change_discards_prepared_run() {
    let mut op = bridge_steps(REPO);
    let mut raw = resource_parent(REPO);
    raw["head"]["repo"]["node_id"] = json!("FORK_REPLACED");
    op[12] = get(&format!("repos/{REPO}/pulls/7"), raw);
    op.truncate(13);
    failure(op, Default::default());
}
#[test]
fn scoped_check_source18_and_fresh_logs15_exact_actor_provenance_sql_git_and_whole_set() {
    let mut op = bridge_steps(REPO);
    op.extend(log_steps("line1\nPRIVATE_LOG_VALID_CANARY\nline3\n"));
    let steps = whole(&op);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let mut app = app_checked(&fixture);
    let before = sql_state(&fixture);
    let git_before = git_state_resource(&fixture);
    let source = app.remote_scope_source.clone().unwrap();
    let active = app.remote_scope.clone().unwrap();
    let inbox = app.github.as_ref().unwrap().inbox.pull_requests.as_ptr();
    let selected = app.inbox_list.selected;
    let retained_provider = provider(&fixture);
    let value = load_bridge_using(&fixture, &mut app, &Default::default(), &retained_provider);
    install(&mut app, value);
    assert_eq!(app.view, View::Actions);
    assert_eq!(
        app.github.as_ref().unwrap().inbox.pull_requests.as_ptr(),
        inbox
    );
    assert!(Arc::ptr_eq(
        app.remote_scope_source.as_ref().unwrap(),
        &source
    ));
    assert!(Arc::ptr_eq(app.remote_scope.as_ref().unwrap(), &active));
    let route = app.pr_checks_route.clone().unwrap();
    let actions = app.actions.take().unwrap();
    assert_eq!(actions.run.as_ref().unwrap().attempt.run.attempt, 2);
    assert_eq!(actions.run.as_ref().unwrap().selected_job, Some(501));
    assert!(actions
        .run
        .as_ref()
        .unwrap()
        .attempt
        .definition_note
        .contains("verified exact endpoint"));
    assert!(actions.rows.iter().any(|row| row
        .actor
        .as_ref()
        .is_some_and(|actor| actor.login == "alice")));
    let job = apply_v1(
        route.clone(),
        actions,
        ActionsAction::OpenJob(501),
        &retained_provider,
        &database(&fixture),
        &Default::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    install(&mut app, job);
    assert_eq!(
        app.actions.as_ref().unwrap().view,
        crate::actions_workspace::ActionsView::Jobs
    );
    let logs = apply_v1(
        route,
        app.actions.take().unwrap(),
        ActionsAction::Logs(Some(501)),
        &retained_provider,
        &database(&fixture),
        &Default::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    install(&mut app, logs);
    assert!(app
        .actions
        .as_ref()
        .unwrap()
        .run
        .as_ref()
        .unwrap()
        .logs
        .as_ref()
        .unwrap()
        .contains("PRIVATE_LOG_VALID_CANARY"));
    assert_eq!(
        app.actions.as_ref().unwrap().identity.login,
        "log-final-login"
    );
    app.dispatch(Action::Actions(ActionsAction::Back));
    assert_eq!(app.view, View::Inbox);
    assert_eq!(app.inbox_list.selected, selected);
    assert!(app.pr_checks_route.is_none());
    assert!(app.actions.is_none());
    assert_eq!(
        app.github.as_ref().unwrap().pr.as_ref().unwrap().tab,
        PrTab::Checks
    );
    read_only(&fixture, &before, &git_before, &steps);
}
#[test]
fn scoped_check_same_uid_login_rename_is_closed_presentation_not_reused_uid() {
    for name in [REPO, "example/renamed"] {
        let steps = whole(&bridge_steps(name));
        let fixture = Fixture::new();
        fixture.scripted_gh(&steps);
        let mut app = app_checked(&fixture);
        let before = sql_state(&fixture);
        let git_before = git_state_resource(&fixture);
        let value = load_bridge(&fixture, &mut app, &Default::default());
        install(&mut app, value);
        assert_eq!(app.github.as_ref().unwrap().identity.account_id, Some(42));
        assert_eq!(
            app.github.as_ref().unwrap().identity.login,
            "bridge-final-login"
        );
        assert_eq!(
            app.github
                .as_ref()
                .unwrap()
                .selected
                .as_ref()
                .unwrap()
                .repository,
            REPO
        );
        assert_eq!(
            app.github
                .as_ref()
                .unwrap()
                .pr
                .as_ref()
                .unwrap()
                .details
                .summary
                .pr
                .repository,
            name
        );
        assert_eq!(app.actions.as_ref().unwrap().repositories, vec![name]);
        read_only(&fixture, &before, &git_before, &steps);
    }
    // Genuine raw merged state is closed+merged=true, while its displayed origin
    // stays merged. No DTO-only cast may authorize this normalization.
    let mut steps = whole(&bridge_steps(REPO));
    for step in &mut steps {
        if step
            .args
            .get(1)
            .is_some_and(|endpoint| endpoint.ends_with("/pulls/7"))
        {
            let mut raw: Value = serde_json::from_slice(&step.stdout).unwrap();
            raw["state"] = json!("closed");
            raw["merged"] = json!(true);
            step.stdout = serde_json::to_vec(&raw).unwrap();
        }
    }
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let mut app = app_checked(&fixture);
    let before = sql_state(&fixture);
    let git_before = git_state_resource(&fixture);
    let value = load_bridge(&fixture, &mut app, &Default::default());
    install(&mut app, value);
    assert_eq!(
        app.github
            .as_ref()
            .unwrap()
            .pr
            .as_ref()
            .unwrap()
            .details
            .summary
            .pr
            .state,
        "merged"
    );
    assert!(pr_checks_native::owns_origin(
        &app,
        app.pr_checks_origin.as_ref().unwrap()
    ));
    read_only(&fixture, &before, &git_before, &steps);
}
#[test]
fn scoped_check_final_reused_uid_and_closing_auth_loss_hide_all_private_results() {
    for auth in [false, true] {
        let mut op = bridge_steps(REPO);
        op[17] = if auth {
            let mut s = bridge_user(42);
            s.status = 1;
            s.stdout.clear();
            s.stderr = b"gh: Requires authentication (HTTP 401) PRIVATE_DIAGNOSTIC".to_vec();
            s
        } else {
            bridge_user(43)
        };
        failure(op, Default::default());
    }
}
#[test]
fn scoped_check_own_closing_repository_recreated_or_permission_unknown_hides() {
    for recreated in [false, true] {
        let mut op = bridge_steps(REPO);
        let mut raw = repo(410, REPO);
        if recreated {
            raw["id"] = json!(999);
        } else {
            raw["permissions"]["pull"] = Value::Null;
        }
        op[15] = get(&format!("repos/{REPO}"), raw);
        op.truncate(16);
        failure(op, Default::default());
    }
}
#[test]
fn scoped_check_exhausted_empty_run_is_honest_unavailable_not_actions_zero_or_fence() {
    let mut op = bridge_steps(REPO);
    op[7] = get(
        &format!(
            "repos/{REPO}/actions/runs?check_suite_id=901&head_sha={}&per_page=100&page=1",
            "a".repeat(40)
        ),
        json!({"total_count":0,"workflow_runs":[]}),
    );
    op.drain(8..11);
    assert_eq!(op.len(), 15);
    let steps = whole(&op);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let mut app = app_checked(&fixture);
    let before = sql_state(&fixture);
    let git_before = git_state_resource(&fixture);
    let result = load_bridge(&fixture, &mut app, &Default::default());
    install(&mut app, result);
    assert_eq!(app.view, View::Inbox);
    assert!(app.actions.is_none());
    assert!(matches!(
        app.remote_scope
            .as_ref()
            .unwrap()
            .consumer(&app.github.as_ref().unwrap().identity),
        RemoteScopeConsumerV1::Explicit(_)
    ));
    assert!(app.notice.contains("No Actions run observed"));
    read_only(&fixture, &before, &git_before, &steps);
}
#[test]
fn scoped_check_partial_single_seen_run_cannot_prove_automatic_uniqueness() {
    let mut op = bridge_steps(REPO);
    op[7] = get(
        &format!(
            "repos/{REPO}/actions/runs?check_suite_id=901&head_sha={}&per_page=100&page=1",
            "a".repeat(40)
        ),
        json!({"total_count":2,"workflow_runs":[actual_run(REPO)]}),
    );
    let steps = whole(&op);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let mut app = app_checked(&fixture);
    let before = sql_state(&fixture);
    let git_before = git_state_resource(&fixture);
    let result = load_bridge(&fixture, &mut app, &Default::default());
    install(&mut app, result);
    assert!(app.actions.is_none());
    assert_eq!(app.view, View::Inbox);
    assert!(app.notice.contains("partial"));
    read_only(&fixture, &before, &git_before, &steps);
}
#[test]
fn scoped_check_ambiguous_suite_retains_original_context_without_arbitrary_run() {
    let mut op = bridge_steps(REPO);
    let mut other = actual_run(REPO);
    other["id"] = json!(124);
    other["node_id"] = json!("WR_124");
    other["url"] = json!(format!(
        "https://{HOST}/api/v3/repos/{REPO}/actions/runs/124"
    ));
    other["html_url"] = json!(format!("https://{HOST}/{REPO}/actions/runs/124"));
    op[7] = get(
        &format!(
            "repos/{REPO}/actions/runs?check_suite_id=901&head_sha={}&per_page=100&page=1",
            "a".repeat(40)
        ),
        json!({"total_count":2,"workflow_runs":[actual_run(REPO),other]}),
    );
    op.drain(8..11);
    let steps = whole(&op);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let mut app = app_checked(&fixture);
    let before = sql_state(&fixture);
    let git_before = git_state_resource(&fixture);
    let result = load_bridge(&fixture, &mut app, &Default::default());
    install(&mut app, result);
    assert!(app.actions.is_none());
    assert!(app.notice.contains("More than one run"));
    read_only(&fixture, &before, &git_before, &steps);
}
#[test]
fn scoped_check_log_fresh_parent_head_repository_replacement_stops_before_capture() {
    let mut logs = log_steps("PRIVATE_LOG_LOST_CANARY");
    let mut raw = resource_parent(REPO);
    raw["head"]["repo"]["id"] = json!(993);
    logs[5] = get(&format!("repos/{REPO}/pulls/7"), raw);
    // IdentityChanged exits directly: no run/log, fallback auth, outer closing
    // or private diagnostic after contradictory genuine source metadata.
    logs.truncate(6);
    let mut op = bridge_steps(REPO);
    op.extend(logs);
    let steps = whole(&op);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let mut app = app_checked(&fixture);
    let before = sql_state(&fixture);
    let git_before = git_state_resource(&fixture);
    let retained_provider = provider(&fixture);
    let result = load_bridge_using(&fixture, &mut app, &Default::default(), &retained_provider);
    install(&mut app, result);
    let result = apply_v1(
        app.pr_checks_route.clone().unwrap(),
        app.actions.take().unwrap(),
        ActionsAction::Logs(Some(501)),
        &retained_provider,
        &database(&fixture),
        &Default::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(result.clear_private);
    assert!(!result.status.contains("PRIVATE_LOG_LOST_CANARY"));
    read_only(&fixture, &before, &git_before, &steps);
}
#[test]
fn scoped_check_logs_capture_failure_closes_inner_then_hides_without_default_probe() {
    let mut logs = log_steps("PRIVATE_LOG_LOST_CANARY");
    logs[7].status = 1;
    logs[7].stdout.clear();
    logs[7].stderr = b"gh: internal read failed PRIVATE_DIAGNOSTIC".to_vec();
    logs.truncate(12);
    let mut op = bridge_steps(REPO);
    op.extend(logs);
    let steps = whole(&op);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let mut app = app_checked(&fixture);
    let before = sql_state(&fixture);
    let git_before = git_state_resource(&fixture);
    let retained_provider = provider(&fixture);
    let result = load_bridge_using(&fixture, &mut app, &Default::default(), &retained_provider);
    install(&mut app, result);
    let result = apply_v1(
        app.pr_checks_route.clone().unwrap(),
        app.actions.take().unwrap(),
        ActionsAction::Logs(Some(501)),
        &retained_provider,
        &database(&fixture),
        &Default::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(result.clear_private);
    assert!(result.scoped_pr_checks.is_none());
    read_only(&fixture, &before, &git_before, &steps);
}
#[test]
fn scoped_check_logs_foreign_actual_runner_refuses_before_inner_private_reads() {
    let mut op = bridge_steps(REPO);
    op.extend(
        log_steps("FOREIGN_RUNNER_LOG_MUST_NOT_BE_READ")
            .into_iter()
            .take(3),
    );
    let steps = whole(&op);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let mut app = app_checked(&fixture);
    let before = sql_state(&fixture);
    let git_before = git_state_resource(&fixture);
    let original_provider = provider(&fixture);
    let value = load_bridge_using(&fixture, &mut app, &Default::default(), &original_provider);
    install(&mut app, value);
    let foreign_provider = provider(&fixture);
    let result = apply_v1(
        app.pr_checks_route.clone().unwrap(),
        app.actions.take().unwrap(),
        ActionsAction::Logs(Some(501)),
        &foreign_provider,
        &database(&fixture),
        &Default::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(result.clear_private);
    assert!(result.scoped_pr_checks.is_none());
    assert!(!result.status.contains("FOREIGN_RUNNER_LOG"));
    read_only(&fixture, &before, &git_before, &steps);
}

#[test]
fn scoped_check_queued_draft_and_recovery_actions_stopped_before_sql_or_child() {
    let steps = whole(&bridge_steps(REPO));
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let mut app = app_checked(&fixture);
    let before = sql_state(&fixture);
    let git_before = git_state_resource(&fixture);
    let result = load_bridge(&fixture, &mut app, &Default::default());
    install(&mut app, result);
    for action in [
        ActionsAction::Compose(ActionsCommand::RerunAll),
        ActionsAction::Compose(ActionsCommand::SearchLogs),
        ActionsAction::PublicationList,
        ActionsAction::Reconcile,
    ] {
        app.dispatch(Action::Close);
        app.dispatch(Action::Actions(action));
        assert!(app.actions_action.is_none());
        assert!(!app.actions.as_ref().unwrap().watching);
    }
    read_only(&fixture, &before, &git_before, &steps);
}
#[test]
fn scoped_check_cancelled_or_expired_source_never_launches_private_phase() {
    for expired in [false, true] {
        let prefix = whole(&[]);
        let fixture = Fixture::new();
        fixture.scripted_gh(&prefix);
        let mut app = app_checked(&fixture);
        let before = sql_state(&fixture);
        let git_before = git_state_resource(&fixture);
        let captured = input(&mut app);
        let cancel = CancellationToken::default();
        let provider = if expired {
            let provider = provider(&fixture)
                .with_scoped_read_window(Instant::now() + Duration::from_millis(1), 128 * 1024)
                .unwrap();
            std::thread::sleep(Duration::from_millis(5));
            provider
        } else {
            cancel.cancel();
            provider(&fixture)
        };
        let result = load_v1(
            captured,
            &provider,
            &database(&fixture),
            &Default::default(),
            &cancel,
        )
        .unwrap();
        assert!(result.clear_private);
        read_only(&fixture, &before, &git_before, &prefix);
    }
    // Failed/limited capture reserves its full64-byte selected cap and cannot
    // launch the next PR/private guard. Multibyte raw diagnostics are not free.
    let mut op = bridge_steps(REPO);
    op[4].stderr = "\u{1b}[31m界🦀".repeat(100).into_bytes();
    op.truncate(5);
    let mut limits = ScopedPrCheckReadLimitsV1::default();
    limits.bridge.max_document_bytes = 64;
    limits.bridge.max_row_bytes = 32;
    failure(op, limits);
}
#[test]
fn scoped_check_original_source_sql_cas_change_before_final_principal_does_not_rebase() {
    let mut op = bridge_steps(REPO);
    op.truncate(16);
    op[15].delay_ms = 250;
    let steps = whole(&op);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let mut app = app_checked(&fixture);
    let source = app.remote_scope_source.clone().unwrap();
    let key = source.saved_view_key().to_owned();
    let path = database(&fixture);
    let journal = fixture.root.join("gh-fixture/commands.jsonl");
    let git_before = git_state_resource(&fixture);
    let concurrent = std::thread::spawn(move || {
        wait_count(&journal, 48);
        let db = rusqlite::Connection::open(path).unwrap();
        assert_eq!(
            db.execute(
                "INSERT INTO saved_views(host,login,name,payload) VALUES(?1,'user-id:42',?2,?3) ON CONFLICT(host,login,name) DO UPDATE SET payload=excluded.payload",
                rusqlite::params![HOST, key, "{\"concurrent\":\"PRIVATE_SQL_CANARY\"}"],
            )
            .unwrap(),
            1,
            "The genuine originally-unsaved source must actually change before closing"
        );
    });
    let result = load_bridge(&fixture, &mut app, &Default::default());
    concurrent.join().unwrap();
    assert!(result.clear_private);
    assert!(!result.status.contains("PRIVATE_SQL_CANARY"));
    exact(&fixture, &steps);
    assert_eq!(git_state_resource(&fixture), git_before);
    let bytes: String = sql(&fixture)
        .query_row(
            "SELECT payload FROM saved_views WHERE name=?1",
            [source.saved_view_key()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(bytes, "{\"concurrent\":\"PRIVATE_SQL_CANARY\"}");
    let attempts:u64=sql(&fixture).query_row("SELECT (SELECT count(*) FROM actions_write_attempts)+(SELECT count(*) FROM pr_write_attempts)",[],|row|row.get(0)).unwrap();
    assert_eq!(attempts, 0);
}

#[test]
fn scoped_check_runtime_success_owner_replaced_before_effects_drops_private_models() {
    runtime_stale(0);
}
#[test]
fn scoped_check_runtime_error_owner_replaced_before_effects_drops_diagnostics() {
    runtime_stale(1);
}
#[test]
fn scoped_check_runtime_panic_owner_replaced_before_effects_drops_diagnostics() {
    runtime_stale(2);
}
#[test]
fn scoped_check_runtime_back_during_queued_logs_cancels_actions_not_cached_inbox() {
    runtime_stale(3);
}
fn runtime_stale(mode: u8) {
    let steps = whole(&bridge_steps(REPO));
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let mut app = app_checked(&fixture);
    let before = sql_state(&fixture);
    let git_before = git_state_resource(&fixture);
    let result = load_bridge(&fixture, &mut app, &Default::default());
    install(&mut app, result);
    let origin = app.pr_checks_origin.clone().unwrap();
    let mut background = Background::new();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    assert!(
        background.submit_scoped_pr_check_v1(&mut app, View::Actions, origin, move |_| {
            ready_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            match mode {
                1 => Err("PRIVATE_STALE_ERROR_CANARY".into()),
                2 => panic!("PRIVATE_STALE_PANIC_CANARY"),
                _ => Ok(PreparedView {
                    status: "PRIVATE_STALE_SUCCESS_CANARY".into(),
                    context: Some("PRIVATE_STALE_CONTEXT_CANARY".into()),
                    ..Default::default()
                }),
            }
        })
    );
    ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    if mode == 3 {
        app.dispatch(Action::Actions(ActionsAction::Back));
        background.cancel(View::Actions);
    } else {
        app.remote_scope = None;
    }
    let expected_status = app.status.clone();
    let expected_view = app.view;
    release_tx.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        background.poll(&mut app);
        std::thread::sleep(Duration::from_millis(5));
        if !app.loading[View::Actions.index()] {
            break;
        }
    }
    assert_eq!(app.status, expected_status);
    assert_eq!(app.view, expected_view);
    assert!(!app.context.contains("PRIVATE_STALE"));
    if mode == 3 {
        assert!(app.github.as_ref().unwrap().pr.is_some());
        assert!(app.pr_checks_route.is_none());
    }
    read_only(&fixture, &before, &git_before, &steps);
}
