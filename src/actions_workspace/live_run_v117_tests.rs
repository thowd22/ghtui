//! TASK-117: live run state, backoff, leaving, the live log text and Follow.
use super::*;
use crate::actions_ui::footer_keys;
use crate::app::Action;
use crate::footer::resolve_key;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::json;

fn job(id: u64, name: &str, status: &str, conclusion: &str) -> Value {
    json!({"id":id,"run_id":9,"attempt":1,"head_sha":"a","name":name,"status":status,
    "conclusion":conclusion,"started_at":"2026-10-13T12:00:00Z",
    "completed_at":if status == "completed" {"2026-10-13T12:01:05Z"} else {""},
    "runner_id":1,"runner_name":"r","runner_group_name":"g","labels":["ubuntu"],"check_run_url":"",
    "steps":[
        {"number":1,"name":"Set up","status":"completed","conclusion":"success",
         "started_at":"2026-10-13T12:00:00Z","completed_at":"2026-10-13T12:00:03Z"},
        {"number":2,"name":"Build","status":status,"conclusion":conclusion,
         "started_at":"2026-10-13T12:00:03Z","completed_at":""}
    ]})
}
fn attempt(status: &str, jobs: Vec<Value>) -> RunAttempt {
    let actor = json!({"login":"alice","display_name":null,"avatar_url":null,"actor_type":"User"});
    serde_json::from_value(json!({
        "run":{"id":9,"workflow_id":7,"name":"CI","title":"t","head_sha":"a","head_branch":"main",
            "event":"push","status":status,"conclusion":"","attempt":1,"actor":actor,
            "triggering_actor":actor,"created_at":"","updated_at":"","url":""},
        "jobs":jobs,"definition_note":"note"}))
    .unwrap()
}
fn workspace(status: &str, jobs: Vec<Value>) -> ActionsWorkspace {
    let mut workspace = ActionsWorkspace::new(
        HostIdentity {
            host: "github.invalid".into(),
            login: "octocat".into(),
            account_id: Some(42),
            gh_version: "offline".into(),
        },
        vec!["example/repo".into()],
    );
    workspace.run = Some(RunPage {
        repository: "example/repo".into(),
        attempt: attempt(status, jobs),
        selected_job: None,
        logs: None,
        logs_skipped_bytes: 0,
        annotations: vec![],
    });
    workspace.view = ActionsView::Run;
    workspace.arm_live_v117();
    workspace.prepare_rows();
    workspace
}

#[test]
fn live_run_v117_in_progress_run_arms_and_completed_run_does_not() {
    let live = workspace("in_progress", vec![job(1, "build", "in_progress", "")]);
    assert!(live.live.armed && live.watching && live.watch_applies());
    assert_eq!(live.poll_interval_secs(), LIVE_POLL_MIN_SECS_V117);
    assert!(live
        .controls
        .iter()
        .any(|control| control.label == "[w Stop watch]"));
    let done = workspace("completed", vec![job(1, "build", "completed", "success")]);
    assert!(!done.live.armed && !done.watching);
}

#[test]
fn live_run_v117_backs_off_while_unchanged_resets_on_change_and_stops_when_done() {
    let mut live = workspace("in_progress", vec![job(1, "build", "in_progress", "")]);
    live.after_live_tick_v117();
    assert_eq!(live.poll_interval_secs(), 10);
    live.after_live_tick_v117();
    assert_eq!(live.poll_interval_secs(), 20);
    live.after_live_tick_v117();
    live.after_live_tick_v117();
    assert_eq!(live.poll_interval_secs(), LIVE_POLL_MAX_SECS_V117);
    // A step finishing is a change: back to the shortest interval.
    live.run.as_mut().unwrap().attempt = attempt(
        "in_progress",
        vec![
            job(1, "build", "completed", "success"),
            job(2, "test", "queued", ""),
        ],
    );
    live.after_live_tick_v117();
    assert_eq!(live.poll_interval_secs(), LIVE_POLL_MIN_SECS_V117);
    live.run.as_mut().unwrap().attempt =
        attempt("completed", vec![job(1, "build", "completed", "success")]);
    let run = live.run.as_ref().unwrap().attempt.run.clone();
    live.observe_watched_run(&run);
    live.after_live_tick_v117();
    assert!(!live.live.armed && !live.watching);
}

#[test]
fn live_run_v117_checks_are_bounded() {
    let mut live = workspace("in_progress", vec![job(1, "build", "in_progress", "")]);
    for _ in 0..live_run_v117::LIVE_POLL_MAX_TICKS_V117 {
        live.after_live_tick_v117();
    }
    assert!(!live.live.armed && !live.watching);
    assert!(live
        .watch_note
        .as_deref()
        .is_some_and(|note| note.contains("w resumes")));
}

#[test]
fn live_run_v117_leaving_the_run_stops_the_live_watch() {
    let mut live = workspace("in_progress", vec![job(1, "build", "in_progress", "")]);
    live.view = ActionsView::Jobs;
    live.leave_live_v117();
    assert!(live.live.armed && live.watching);
    live.view = ActionsView::Runs;
    live.leave_live_v117();
    assert!(!live.live.armed && !live.watching);
    assert_eq!(
        live.watch_note.as_deref(),
        Some("Live updates stopped: you left the run")
    );
    // Pressing w on a live run also ends the automatic watch.
    let mut live = workspace("in_progress", vec![job(1, "build", "in_progress", "")]);
    live.toggle_watch().unwrap();
    assert!(!live.live.armed && !live.watching);
}

#[test]
fn live_run_v117_live_log_shows_finished_logs_and_explains_running_jobs() {
    let mut live = workspace(
        "in_progress",
        vec![
            job(1, "build", "completed", "success"),
            job(2, "test", "in_progress", ""),
        ],
    );
    live.live.target = Some(None);
    live.live.logs.insert(1, "compiled ok\n".into());
    live.rebuild_live_log_v117();
    live.view = ActionsView::Logs;
    live.prepare_rows();
    let lines = live
        .rows
        .iter()
        .map(|row| row.label.as_str())
        .collect::<Vec<_>>();
    assert_eq!(lines[0], "── build • success • 1m 05s ──");
    assert_eq!(lines[1], "compiled ok");
    assert!(lines[2].starts_with("── test • in progress • running "));
    assert_eq!(lines[3], crate::providers::gh::LIVE_LOG_PENDING_V117);
    assert_eq!(lines[4], "  Step 1 Set up • success • 3s");
    assert!(lines[5].starts_with("  Step 2 Build • in progress • running "));
    // A live log page offers Follow and the watch's Stop.
    for label in [FOLLOW_LABEL_V117, "[w Stop watch]"] {
        assert!(live.controls.iter().any(|control| control.label == label));
    }
    assert!(live.watch_applies());
}

#[test]
fn live_run_v117_follow_by_key_keeps_the_newest_line_selected() {
    let mut live = workspace(
        "in_progress",
        vec![
            job(1, "build", "completed", "success"),
            job(2, "test", "in_progress", ""),
        ],
    );
    live.live.target = Some(None);
    live.live.logs.insert(1, "a\nb\nc\n".into());
    live.rebuild_live_log_v117();
    live.view = ActionsView::Logs;
    live.prepare_rows();
    assert_eq!(live.selected, 0);
    let action = match resolve_key(
        &footer_keys(&live),
        KeyEvent::new(KeyCode::Char('F'), KeyModifiers::NONE),
    ) {
        Some(Action::Actions(action)) => action,
        other => panic!("F resolves to {other:?}"),
    };
    assert_eq!(action, ActionsAction::Follow);
    live.toggle_follow_v117();
    assert_eq!(live.selected, live.rows.len() - 1);
    assert!(live
        .controls
        .iter()
        .any(|control| control.label == FOLLOWING_LABEL_V117));
    // A new finished log arrives: the selection moves to the new last line.
    live.live.logs.insert(2, "d\ne\n".into());
    live.run.as_mut().unwrap().attempt.jobs[1] =
        serde_json::from_value(job(2, "test", "completed", "success")).unwrap();
    live.rebuild_live_log_v117();
    live.prepare_rows();
    assert_eq!(live.rows[live.selected].label, "e");
}

#[test]
fn live_run_v117_timing_reports_elapsed_for_running_work() {
    let start = crate::github_workspace::timestamp_seconds("2026-10-13T12:00:00Z").unwrap();
    assert_eq!(
        live_timing_v117("2026-10-13T12:00:00Z", "", start + 75).as_deref(),
        Some("running 1m 15s")
    );
    assert_eq!(
        live_timing_v117("2026-10-13T12:00:00Z", "2026-10-13T12:00:09Z", start + 999).as_deref(),
        Some("9s")
    );
    assert_eq!(live_timing_v117("", "", start), None);
}
