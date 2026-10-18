//! Run/job/log focus and failure-jump behaviour of the shared Actions viewer.
use super::*;
use crate::actions_ui::keyboard_action;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::json;

fn job(id: u64, name: &str, conclusion: &str, failed_step: bool) -> Value {
    json!({"id":id,"run_id":9,"attempt":1,"head_sha":"a","name":name,"status":"completed",
    "conclusion":conclusion,"started_at":"","completed_at":"","runner_id":1,"runner_name":"r",
    "runner_group_name":"g","labels":["ubuntu"],"check_run_url":"",
    "steps":[
        {"number":1,"name":"Set up","status":"completed","conclusion":"success","started_at":"","completed_at":""},
        {"number":2,"name":"Test","status":"completed","conclusion":if failed_step {"failure"} else {"success"},"started_at":"","completed_at":""}
    ]})
}
fn workspace(jobs: Vec<Value>) -> ActionsWorkspace {
    let mut workspace = ActionsWorkspace::new(
        HostIdentity {
            host: "github.invalid".into(),
            login: "octocat".into(),
            account_id: Some(42),
            gh_version: "offline".into(),
        },
        vec!["example/repo".into()],
    );
    let actor = json!({"login":"alice","display_name":null,"avatar_url":null,"actor_type":"User"});
    let attempt: RunAttempt = serde_json::from_value(json!({
        "run":{"id":9,"workflow_id":7,"name":"CI","title":"t","head_sha":"a","head_branch":"main",
            "event":"push","status":"completed","conclusion":"failure","attempt":1,"actor":actor,
            "triggering_actor":actor,"created_at":"","updated_at":"","url":""},
        "jobs":jobs,"definition_note":"note"}))
    .unwrap();
    workspace.run = Some(RunPage {
        repository: "example/repo".into(),
        attempt,
        selected_job: None,
        logs: None,
        annotations: vec![],
    });
    workspace.view = ActionsView::Run;
    workspace.prepare_rows();
    workspace
}
fn selected_label(workspace: &ActionsWorkspace) -> &str {
    &workspace.rows[workspace.selected].label
}
fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn v1_actions_runs_open_focuses_first_failed_job_then_first_job() {
    let mut failed = workspace(vec![
        job(1, "lint", "success", false),
        job(2, "test", "failure", true),
    ]);
    failed.focus_job_row();
    assert!(selected_label(&failed).starts_with("Job 2 • test"));
    let mut green = workspace(vec![
        job(1, "lint", "success", false),
        job(2, "test", "success", false),
    ]);
    green.focus_job_row();
    assert!(selected_label(&green).starts_with("Job 1 • lint"));
    // Header rows stay reachable above the job.
    assert!(green.rows[0].label.starts_with("Run 9"));
}

#[test]
fn v1_actions_runs_opened_job_stays_selected_in_jobs_and_back_to_run() {
    let mut workspace = workspace(vec![
        job(1, "lint", "success", false),
        job(2, "test", "success", false),
    ]);
    workspace.run.as_mut().unwrap().selected_job = Some(2);
    workspace.view = ActionsView::Jobs;
    workspace.prepare_rows();
    workspace.focus_job_row();
    assert!(selected_label(&workspace).starts_with("Job 2 • test"));
    workspace.view = ActionsView::Run;
    workspace.prepare_rows();
    workspace.focus_job_row();
    assert!(selected_label(&workspace).starts_with("Job 2 • test"));
}

#[test]
fn v1_actions_runs_enter_on_information_row_stays_in_actions() {
    let mut workspace = workspace(vec![job(1, "lint", "success", false)]);
    workspace.select(2); // run actor row: nothing to open
    assert!(workspace.rows[2].action.is_none());
    assert_eq!(
        keyboard_action(key(KeyCode::Enter), &workspace),
        Some(ActionsAction::Select(2))
    );
}

#[test]
fn v1_actions_runs_failure_jumps_walk_failed_jobs_and_steps_by_key_and_control() {
    let mut workspace = workspace(vec![
        job(1, "lint", "failure", false),
        job(2, "ok", "success", false),
        job(3, "test", "failure", true),
    ]);
    workspace.run.as_mut().unwrap().selected_job = Some(3);
    workspace.view = ActionsView::Jobs;
    workspace.prepare_rows();
    workspace.select(0);
    let next = |w: &ActionsWorkspace| keyboard_action(key(KeyCode::Char('>')), w);
    let Some(ActionsAction::Select(first)) = next(&workspace) else {
        panic!("next failure");
    };
    workspace.select(first);
    assert!(selected_label(&workspace).starts_with("Job 1 • lint"));
    let Some(ActionsAction::Select(second)) = next(&workspace) else {
        panic!("next failure");
    };
    workspace.select(second);
    assert!(selected_label(&workspace).starts_with("Job 3 • test"));
    // The clickable control carries the same target as the key.
    let control = workspace
        .controls
        .iter()
        .find(|control| control.label == NEXT_FAILURE)
        .unwrap();
    assert_eq!(Some(control.action.clone()), next(&workspace));
    let Some(ActionsAction::Select(step)) = next(&workspace) else {
        panic!("failed step");
    };
    workspace.select(step);
    assert!(selected_label(&workspace).contains("Step 2 • Test • completed / failure"));
    assert!(
        matches!(next(&workspace), Some(ActionsAction::Unavailable(reason)) if reason.contains("none later"))
    );
    assert_eq!(
        keyboard_action(key(KeyCode::Char('<')), &workspace),
        Some(ActionsAction::Select(second))
    );
}

#[test]
fn v1_actions_runs_step_rows_show_durations_or_reported_times() {
    assert_eq!(
        step_timing("2026-10-09T05:40:48Z", "2026-10-09T05:43:17Z"),
        "2m 29s"
    );
    assert_eq!(
        step_timing("2026-10-09T05:40:48Z", "2026-10-09T05:40:48Z"),
        "0s"
    );
    assert_eq!(
        step_timing("2026-10-09T05:00:00Z", "2026-10-09T06:05:00Z"),
        "1h 05m"
    );
    assert_eq!(
        step_timing("2026-10-09T05:40:48Z", ""),
        "started 2026-10-09T05:40:48Z"
    );
    assert_eq!(step_timing("", ""), "not started");
    assert_eq!(step_timing("soon", "later"), "soon → later");
}

#[test]
fn v1_actions_runs_logs_mark_error_lines_as_failure_jump_targets() {
    let mut workspace = workspace(vec![job(1, "lint", "failure", true)]);
    workspace.run.as_mut().unwrap().logs =
        Some("setup\n##[error]Process completed with exit code 1.\ntail\n".into());
    workspace.view = ActionsView::Logs;
    workspace.prepare_rows();
    workspace.select(0);
    assert_eq!(workspace.rows[1].role, SemanticToken::Failure);
    assert_eq!(
        keyboard_action(key(KeyCode::Char('>')), &workspace),
        Some(ActionsAction::Select(1))
    );
    // A green run without failures offers no failure controls.
    let green = workspace_green();
    assert!(!green.controls.iter().any(|c| c.label == NEXT_FAILURE));
    assert_eq!(keyboard_action(key(KeyCode::Char('>')), &green), None);
}
fn workspace_green() -> ActionsWorkspace {
    workspace(vec![job(1, "lint", "success", false)])
}
