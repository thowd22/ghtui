//! SOURCE-ONLY actual Source7/PR13/Checks10/bridge18, same Actions continuation.
use super::*;
use crate::{scoped_actions_native_v1 as native, scoped_actions_v1 as actions};
fn artifact() -> Value {
    json!({"id":701,"node_id":"ART_701","name":"GENUINE_PR_ARTIFACT","expired":false,"workflow_run":{"id":123,"repository_id":410,"head_sha":"a".repeat(40)}})
}
fn resources(uid: u64) -> Vec<GhStep> {
    let mut out = principal(42, "actions-current");
    out.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    for path in ["before", "after"] {
        out.push(get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)));
        out.push(get(
            &format!("repos/{REPO}/check-runs/600"),
            actual_check(REPO),
        ));
        if path == "before" {
            out.push(get(
                &format!("repos/{REPO}/actions/runs/123"),
                actual_run(REPO),
            ));
            let mut step = get(
                &format!("repos/{REPO}/actions/runs/123/artifacts?per_page=100"),
                json!([{"total_count":1,"artifacts":[artifact()]}]),
            );
            step.args.extend(["--paginate".into(), "--slurp".into()]);
            out.push(step);
            out.push(get(
                &format!("repos/{REPO}/actions/runs/123"),
                actual_run(REPO),
            ));
        }
    }
    out.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    out.extend(principal(uid, "actions-final"));
    assert_eq!(out.len(), 13);
    out
}
fn setup_before_runtime_drain(tail: Vec<GhStep>) -> (Fixture, GithubProvider, App, Vec<GhStep>) {
    let mut op = bridge_steps(REPO);
    op.extend(tail);
    let steps = whole(&op);
    let f = Fixture::new();
    f.scripted_gh(&steps);
    let mut app = app_checked(&f);
    let p = provider(&f);
    let value = load_bridge_using(&f, &mut app, &Default::default(), &p);
    install(&mut app, value);
    app.dispatch(Action::Close);
    (f, p, app, steps)
}
fn drain_empty_continuation_cancel(app: &mut App) {
    // Opening the actual PR check retires the previous continuation. The real
    // main loop consumes this signal before another input; this direct-worker
    // fixture has no previous logical or physical continuation to cancel.
    assert!(app.pr_check_continuation_cancel_v2);
    assert!(app.pr_check_continuation_request_v2.is_none());
    assert!(app.pr_check_continuation_owner_v2.is_none());
    assert!(app.pr_check_watch_v2.is_none());
    assert!(app.pr_check_choice_request.is_none());
    assert!(!app.pr_checks_request);
    assert!(!app.pr_checks_cancel_actions);
    assert!(!app.actions_load_requested);
    assert!(app.actions_action.is_none());
    assert!(app.pr_checks_route.as_ref().unwrap().scoped_v1());
    assert!(app.remote_scope_source.is_some());
    app.pr_check_continuation_cancel_v2 = false;
}
fn setup(tail: Vec<GhStep>) -> (Fixture, GithubProvider, App, Vec<GhStep>) {
    let (fixture, provider, mut app, steps) = setup_before_runtime_drain(tail);
    drain_empty_continuation_cancel(&mut app);
    (fixture, provider, app, steps)
}
fn read(
    f: &Fixture,
    p: &GithubProvider,
    app: &mut App,
) -> Result<actions::PreparedScopedActionsV1, String> {
    assert!(native::intercept(app, &ActionsAction::Artifacts));
    let work = native::take_work(app).expect("Actual closed PR-origin Actions work");
    actions::execute(work, p, &database(f))
}
#[test]
fn scoped_actions_resources_v2_pr_retirement_signal_refuses_until_actual_empty_runtime_drain() {
    let (f, p, mut app, steps) = setup_before_runtime_drain(resources(42));
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    let commands = fs::read(f.root.join("gh-fixture/commands.jsonl")).unwrap();
    let source = app.remote_scope_source.as_ref().unwrap().clone();
    let route = app.pr_checks_route.as_ref().unwrap().clone();
    assert!(app.pr_check_continuation_cancel_v2);
    assert!(native::intercept(&mut app, &ActionsAction::Artifacts));
    assert!(app.scoped_actions_work_v1.is_none());
    assert!(app.scoped_actions_v1.is_none());
    assert_eq!(
        fs::read(f.root.join("gh-fixture/commands.jsonl")).unwrap(),
        commands
    );
    assert_eq!(sql_state(&f), before);
    assert_eq!(git_state_resource(&f), git);
    assert!(Arc::ptr_eq(
        app.remote_scope_source.as_ref().unwrap(),
        &source
    ));
    assert!(Arc::ptr_eq(app.pr_checks_route.as_ref().unwrap(), &route));
    app.dispatch(Action::Close);
    drain_empty_continuation_cancel(&mut app);
    let value = read(&f, &p, &mut app).unwrap();
    assert!(Arc::ptr_eq(value.page.source().original(), &source));
    assert_eq!(value.workspace.artifacts[0].id, 701);
    read_only(&f, &before, &git, &steps);
}
#[test]
fn scoped_actions_resources_v2_pr_entry_preserves_real_origin_head_run_principal_no_dto_source() {
    let (f, p, mut app, steps) = setup(resources(42));
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    let source = app.remote_scope_source.clone().unwrap();
    let route = app.pr_checks_route.clone().unwrap();
    let value = read(&f, &p, &mut app).unwrap();
    assert_eq!(value.page.run().unwrap().repository_id(), 410);
    assert_eq!(value.page.run().unwrap().run().head_sha, "a".repeat(40));
    assert_eq!(value.workspace.artifacts[0].id, 701);
    assert!(Arc::ptr_eq(value.page.source().original(), &source));
    assert!(Arc::ptr_eq(app.pr_checks_route.as_ref().unwrap(), &route));
    assert!(value
        .workspace
        .controls
        .iter()
        .filter(|c| matches!(c.action, ActionsAction::Compose(_)))
        .all(|c| c.disabled_reason.is_some()));
    read_only(&f, &before, &git, &steps);
}
#[test]
fn scoped_actions_resources_v2_pr_closing_head_replacement_discards_private_inventory() {
    let mut op = resources(42);
    let mut raw = resource_parent(REPO);
    raw["head"]["sha"] = json!("b".repeat(40));
    op[8] = get(&format!("repos/{REPO}/pulls/7"), raw);
    op.truncate(9);
    let (f, p, mut app, steps) = setup(op);
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    assert!(read(&f, &p, &mut app).is_err());
    assert!(app.actions.as_ref().unwrap().artifacts.is_empty());
    read_only(&f, &before, &git, &steps);
}
#[test]
fn scoped_actions_resources_v2_pr_same_login_numeric_uid_change_is_not_new_access() {
    let (f, p, mut app, steps) = setup(resources(43));
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    assert!(read(&f, &p, &mut app).is_err());
    assert!(app.actions.as_ref().unwrap().artifacts.is_empty());
    read_only(&f, &before, &git, &steps);
}
#[test]
fn scoped_actions_resources_v2_pr_back_restores_exact_original_checks_zero_children() {
    let (f, p, mut app, steps) = setup(resources(42));
    let source = app.remote_scope_source.clone().unwrap();
    let original = app.pr_checks_origin.clone().unwrap();
    let value = read(&f, &p, &mut app).unwrap();
    let owner = native::ScopedActionsOwnerV1::new(value.work.clone()).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    assert!(native::publish(&mut app, &owner, value));
    app.scoped_actions_owner_v1 = None;
    assert!(native::intercept(&mut app, &ActionsAction::Back));
    assert!(app.actions.as_ref().unwrap().artifacts.is_empty());
    assert!(Arc::ptr_eq(
        app.pr_checks_origin.as_ref().unwrap(),
        &original
    ));
    app.dispatch(Action::Actions(ActionsAction::Back));
    assert_eq!(app.view, View::Inbox);
    assert!(app.pr_checks_route.is_none());
    assert!(Arc::ptr_eq(
        app.remote_scope_source.as_ref().unwrap(),
        &source
    ));
    exact(&f, &steps);
}
#[test]
fn scoped_actions_resources_v2_pr_new_source_intent_rejects_late_resource_success() {
    let (f, p, mut app, steps) = setup(resources(42));
    let value = read(&f, &p, &mut app).unwrap();
    let owner = native::ScopedActionsOwnerV1::new(value.work.clone()).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    app.scoped_threads_cancel_v1 = true;
    let status = app.status.clone();
    let context = app.context.clone();
    assert!(!native::publish(&mut app, &owner, value));
    assert_eq!(app.status, status);
    assert_eq!(app.context, context);
    assert!(app.actions.as_ref().unwrap().artifacts.is_empty());
    exact(&f, &steps);
}
fn attempt_read_v2(logs: bool) -> Vec<GhStep> {
    let mut out = principal(42, "actions-current");
    out.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    out.push(get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)));
    out.push(get(
        &format!("repos/{REPO}/check-runs/600"),
        actual_check(REPO),
    ));
    out.push(get(
        &format!("repos/{REPO}/actions/runs/123"),
        actual_run(REPO),
    ));
    out.push(get(
        &format!("repos/{REPO}/actions/runs/123/attempts/2"),
        actual_run(REPO),
    ));
    let mut members = get(
        &format!("repos/{REPO}/actions/runs/123/attempts/2/jobs?per_page=100"),
        json!([{"total_count":1,"jobs":[actual_job(REPO)]}]),
    );
    members.args.extend(["--paginate".into(), "--slurp".into()]);
    out.push(members);
    if logs {
        out.push(get(
            &format!("repos/{REPO}/actions/jobs/501"),
            actual_job(REPO),
        ));
        out.push(GhStep::read(
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
            "NEW_ACTIONS_LOG_CAPTURE\nsecond actual\nNEW_ACTIONS_LOG_CAPTURE\n",
        ));
        out.push(get(
            &format!("repos/{REPO}/actions/jobs/501"),
            actual_job(REPO),
        ));
    }
    out.push(get(
        &format!("repos/{REPO}/actions/runs/123"),
        actual_run(REPO),
    ));
    out.push(get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)));
    out.push(get(
        &format!("repos/{REPO}/check-runs/600"),
        actual_check(REPO),
    ));
    out.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    out.extend(principal(42, "actions-final"));
    assert_eq!(out.len(), if logs { 17 } else { 14 });
    out
}
fn publish_read_v2(app: &mut App, prepared: actions::PreparedScopedActionsV1) {
    let owner = native::ScopedActionsOwnerV1::new(prepared.work.clone()).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    assert!(native::publish(app, &owner, prepared));
    app.scoped_actions_owner_v1 = None;
}
#[test]
fn scoped_actions_read_v2_pr_real_old_log_capture_cannot_control_new_actions_logs_and_back() {
    use crate::pr_checks_native::scoped::choices::continuations_v2::{
        self as next, LoadedLogSearchModeV2 as Mode,
    };
    let mut tail = log_steps("OLD_PR_CAPTURE_MATCH\nold second\n");
    tail.extend(resources(42));
    tail.extend(attempt_read_v2(false));
    tail.extend(attempt_read_v2(true));
    let (f, p, mut app, steps) = setup(tail);
    let before = sql_state(&f);
    let git = git_state_resource(&f);
    let old = apply_v1(
        app.pr_checks_route.clone().unwrap(),
        app.actions.take().unwrap(),
        ActionsAction::Logs(Some(501)),
        &p,
        &database(&f),
        &Default::default(),
        &Default::default(),
    )
    .unwrap();
    install(&mut app, old);
    let route = app.pr_checks_route.clone().unwrap();
    let capture = route.loaded_capture_v2().unwrap().clone();
    app.pr_check_log_matches_v2 = Some(
        next::LoadedLogMatchesV2::prepare(
            capture.clone(),
            "OLD_PR_CAPTURE_MATCH".into(),
            Mode::ExactText,
            Instant::now() + Duration::from_secs(30),
            &Default::default(),
        )
        .unwrap(),
    );
    let value = read(&f, &p, &mut app).unwrap();
    publish_read_v2(&mut app, value);
    assert!(native::intercept(
        &mut app,
        &ActionsAction::View(crate::actions_workspace::ActionsView::Jobs)
    ));
    let work = native::take_work(&mut app).unwrap();
    let value = actions::execute(work, &p, &database(&f)).unwrap();
    assert_eq!(
        value.workspace.view,
        crate::actions_workspace::ActionsView::Jobs
    );
    assert!(value
        .workspace
        .rows
        .iter()
        .any(|row| row.label.contains("Job 501")));
    publish_read_v2(&mut app, value);
    assert!(native::intercept(&mut app, &ActionsAction::Logs(Some(501))));
    let value = actions::execute(native::take_work(&mut app).unwrap(), &p, &database(&f)).unwrap();
    assert!(value.page.loaded_log_snapshot_v2().is_some());
    publish_read_v2(&mut app, value);
    let rows = app.actions.as_ref().unwrap().rows.as_ptr();
    let selected = app.actions.as_ref().unwrap().selected;
    let status = app.status.clone();
    next::ui::dispatch(&mut app, next::ui::PrCheckContinueUiV2::NextMatch);
    next::ui::dispatch(&mut app, next::ui::PrCheckContinueUiV2::ClearMatches);
    assert_eq!(app.actions.as_ref().unwrap().selected, selected);
    assert_eq!(app.actions.as_ref().unwrap().rows.as_ptr(), rows);
    assert_eq!(app.status, status);
    assert!(app
        .pr_check_log_matches_v2
        .as_ref()
        .unwrap()
        .belongs_to(&capture));
    for _ in 0..3 {
        app.dispatch(Action::Actions(ActionsAction::Back));
    }
    assert!(app.scoped_actions_v1.is_none());
    assert!(Arc::ptr_eq(app.pr_checks_route.as_ref().unwrap(), &route));
    assert!(app
        .actions
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .any(|row| row.label.contains("OLD_PR_CAPTURE_MATCH")));
    assert!(app
        .pr_check_log_matches_v2
        .as_ref()
        .unwrap()
        .belongs_to(&capture));
    read_only(&f, &before, &git, &steps);
}
