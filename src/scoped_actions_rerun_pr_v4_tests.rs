//! SOURCE ONLY / UNRUN. Actual PR route factories, never fabricated source DTOs.
#[path = "scoped_actions_retained_run_history_v8_tests.rs"]
mod retained_history_v8;

#[test]
fn scoped_actions_rerun_v4_initial_pr_replaced_genuine_source_or_wrong_view_is_no_effect() {
    for changed_source in [false, true] {
        let (f, _p, mut app, steps) = setup(vec![]);
        let (other, _op, other_app, other_steps) = setup(vec![]);
        if changed_source {
            app.remote_scope_source = other_app.remote_scope_source.clone();
        } else {
            app.view = View::Repository;
        }
        let source = app.remote_scope_source.clone().unwrap();
        let route = app.pr_checks_route.clone().unwrap();
        let sql = sql_state(&f);
        let git = git_state_resource(&f);
        let other_sql = sql_state(&other);
        let other_git = git_state_resource(&other);
        let status = app.status.clone();
        let hits = format!("{:?}", app.hits);
        assert!(native::intercept(
            &mut app,
            &ActionsAction::Compose(ActionsCommand::RerunAll)
        ));
        unchanged(&app, &source, &route, &status, &hits);
        read_only(&f, &sql, &git, &steps);
        read_only(&other, &other_sql, &other_git, &other_steps);
    }
}

use super::*;
use crate::scoped_actions_native_v1 as native;
use crate::scoped_actions_native_v1::publication_v3 as publication;
use crate::storage::CatalogueIntentSource;

pub(super) fn setup(tail: Vec<GhStep>) -> (Fixture, GithubProvider, App, Vec<GhStep>) {
    let mut op = bridge_steps(REPO);
    op.extend(tail);
    let steps = whole(&op);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let mut app = app_checked(&fixture);
    let provider = provider(&fixture);
    let value = load_bridge_using(&fixture, &mut app, &Default::default(), &provider);
    install(&mut app, value);
    app.dispatch(Action::Close);
    // Exact empty signal consumed by the real main loop, not a guard waiver.
    assert!(app.pr_check_continuation_cancel_v2);
    assert!(app.pr_check_continuation_request_v2.is_none());
    assert!(app.pr_check_continuation_owner_v2.is_none());
    assert!(app.pr_check_watch_v2.is_none());
    assert!(app.pr_check_choice_request.is_none());
    assert!(!app.pr_checks_request);
    assert!(!app.pr_checks_cancel_actions);
    assert!(!app.actions_load_requested);
    assert!(app.actions_action.is_none());
    app.pr_check_continuation_cancel_v2 = false;
    assert!(app.scoped_actions_v1.is_none());
    (fixture, provider, app, steps)
}
pub(super) fn phase() -> Vec<GhStep> {
    let mut out = principal(42, "open-current");
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
        &format!("repos/{REPO}/actions/runs/123"),
        actual_run(REPO),
    ));
    out.push(get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)));
    out.push(get(
        &format!("repos/{REPO}/check-runs/600"),
        actual_check(REPO),
    ));
    out.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    out.extend(principal(42, "open-final"));
    assert_eq!(out.len(), 12);
    out
}
fn unchanged(
    app: &App,
    source: &Arc<CatalogueIntentSource>,
    route: &Arc<pr_checks_native::PrChecksRoute>,
    status: &str,
    hits: &str,
) {
    assert!(app.scoped_actions_v1.is_none());
    assert!(app.scoped_actions_work_v1.is_none());
    assert!(app.scoped_actions_owner_v1.is_none());
    assert!(!app.scoped_actions_cancel_v1);
    assert!(Arc::ptr_eq(
        app.remote_scope_source.as_ref().unwrap(),
        source
    ));
    assert!(Arc::ptr_eq(app.pr_checks_route.as_ref().unwrap(), route));
    assert_eq!(app.status, status);
    assert_eq!(format!("{:?}", app.hits), hits);
}
#[test]
fn scoped_actions_rerun_v4_initial_pr_opens_same_original_purpose_before_any_seed_effect() {
    let (f, p, mut app, steps) = setup(phase());
    let sql = sql_state(&f);
    let git = git_state_resource(&f);
    let route = app.pr_checks_route.clone().unwrap();
    let source = app.remote_scope_source.clone().unwrap();
    assert!(native::intercept(
        &mut app,
        &ActionsAction::Compose(ActionsCommand::RerunAll)
    ));
    let work = native::take_work(&mut app).expect("Actual direct initial PR command");
    assert_eq!(work.epoch, 1);
    assert_eq!(
        work.kind,
        crate::scoped_actions_v1::ScopedActionsRequestKindV1::Publication
    );
    assert!(Arc::ptr_eq(work.page.source().original(), &source));
    let owner = native::ScopedActionsOwnerV1::new(work).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let prepared = publication::execute(&owner, &p, &database(&f), None).unwrap();
    assert!(publication::publish(&mut app, &owner, prepared));
    assert!(Arc::ptr_eq(app.pr_checks_route.as_ref().unwrap(), &route));
    assert!(matches!(
        app.form.as_ref().unwrap().purpose,
        crate::forms::FormPurpose::ScopedActionsV3(_)
    ));
    assert_eq!(app.form.as_ref().unwrap().fields[0].key, "debug");
    read_only(&f, &sql, &git, &steps);
}
#[test]
fn scoped_actions_rerun_v4_initial_pr_controls_only_open_original_supported_command_not_permission()
{
    let (f, _p, app, steps) = setup(vec![]);
    let before_sql = sql_state(&f);
    let before_git = git_state_resource(&f);
    let w = app.actions.as_ref().unwrap();
    let page = w.run.as_ref().unwrap();
    assert_eq!(page.attempt.run.id, 123);
    assert_eq!(page.attempt.run.attempt, 2);
    assert_eq!(page.attempt.run.status, "completed");
    assert_eq!(page.selected_job, Some(501));
    const REASON: &str = "Additional scoped PR-check action needs its own original access/draft/publication admission; nothing sent.";
    // Independent literal inventory: a completed genuine matched member can
    // open these five intents, not gain publication permission from a name.
    let expected = [
        (ActionsCommand::ChooseScope, Some(REASON)),
        (ActionsCommand::RerunAll, None),
        (ActionsCommand::RerunFailed, None),
        (ActionsCommand::RerunJob, None),
        (ActionsCommand::Cancel, Some(REASON)),
        (ActionsCommand::ForceCancel, Some(REASON)),
        (ActionsCommand::DeleteRun, None),
        (ActionsCommand::DeleteLogs, None),
        (ActionsCommand::ApproveFork, Some(REASON)),
    ];
    let actual = w
        .controls
        .iter()
        .filter_map(|control| match control.action {
            ActionsAction::Compose(command) => Some(command),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        actual,
        expected
            .iter()
            .map(|(command, _)| *command)
            .collect::<Vec<_>>()
    );
    for (command, reason) in expected {
        let control = w
            .controls
            .iter()
            .find(|control| control.action == ActionsAction::Compose(command))
            .unwrap();
        assert_eq!(
            control.disabled_reason.as_deref(),
            reason,
            "Exact current {command:?} eligibility/reason"
        );
    }
    assert!(app.scoped_actions_work_v1.is_none());
    effects(&f, &before_sql, &before_git, &steps);
}
#[test]
fn scoped_actions_rerun_v4_initial_pr_invalid_selected_member_is_exact_zero_effect() {
    let (f, _p, mut app, steps) = setup(vec![]);
    app.actions
        .as_mut()
        .unwrap()
        .run
        .as_mut()
        .unwrap()
        .selected_job = Some(999);
    let sql = sql_state(&f);
    let git = git_state_resource(&f);
    let source = app.remote_scope_source.clone().unwrap();
    let route = app.pr_checks_route.clone().unwrap();
    let status = app.status.clone();
    let hits = format!("{:?}", app.hits);
    assert!(native::intercept(
        &mut app,
        &ActionsAction::Compose(ActionsCommand::RerunJob)
    ));
    unchanged(&app, &source, &route, &status, &hits);
    read_only(&f, &sql, &git, &steps);
}
#[test]
fn scoped_actions_rerun_v4_initial_pr_newer_intents_or_overlay_do_not_seed_or_cancel() {
    for choice in 0..3 {
        let (f, _p, mut app, steps) = setup(vec![]);
        match choice {
            0 => app.repository_load_requested = true,
            1 => app.scoped_threads_cancel_v1 = true,
            _ => app.overlay = Some(crate::app::Overlay::Help),
        }
        let sql = sql_state(&f);
        let git = git_state_resource(&f);
        let source = app.remote_scope_source.clone().unwrap();
        let route = app.pr_checks_route.clone().unwrap();
        let status = app.status.clone();
        let hits = format!("{:?}", app.hits);
        assert!(native::intercept(
            &mut app,
            &ActionsAction::Compose(ActionsCommand::RerunAll)
        ));
        unchanged(&app, &source, &route, &status, &hits);
        read_only(&f, &sql, &git, &steps);
    }
}
#[test]
fn scoped_actions_rerun_v4_initial_pr_actual_held_generic_actions_job_survives_refusal() {
    let (f, _p, mut app, steps) = setup(vec![]);
    let mut background = Background::new();
    let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(1);
    let (release_tx, release_rx) = std::sync::mpsc::sync_channel(1);
    assert!(background.submit(&mut app, View::Actions, move |cancel| {
        entered_tx.send(cancel.clone()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        Ok(PreparedView::default())
    }));
    let live = entered_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    let source = app.remote_scope_source.clone().unwrap();
    let route = app.pr_checks_route.clone().unwrap();
    let sql = sql_state(&f);
    let git = git_state_resource(&f);
    let status = app.status.clone();
    let hits = format!("{:?}", app.hits);
    let loading = app.loading;
    assert!(loading[View::Actions.index()]);
    assert!(native::intercept(
        &mut app,
        &ActionsAction::Compose(ActionsCommand::RerunAll)
    ));
    unchanged(&app, &source, &route, &status, &hits);
    assert_eq!(app.loading, loading);
    assert!(!live.is_cancelled());
    release_tx.send(()).unwrap();
    drop(background);
    read_only(&f, &sql, &git, &steps);
}
