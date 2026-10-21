//! TASK-117: an Actions run in progress updates by itself. A strict fake gh run moves
//! queued -> in_progress -> completed with no manual Refresh or Watch: live job and step
//! status with running times, a finished job's log while another job still runs (read by
//! REST, since `gh run view --log` refuses a run in progress), the running job's note on
//! when its log appears, Follow keeping the newest line selected, and polling that stops
//! once the run completes. Keyboard and mouse; reads only; no Git change.
use super::*;
use std::time::{Duration, Instant};

const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn run(status: &str, conclusion: Value) -> Value {
    let mut run = raw_run();
    run["status"] = json!(status);
    run["conclusion"] = conclusion;
    run
}
fn step(number: u64, name: &str, status: &str, conclusion: Value, done: bool) -> Value {
    json!({"number":number,"name":name,"status":status,"conclusion":conclusion,
        "started_at":(status != "queued").then_some("2026-10-13T12:00:10Z"),
        "completed_at":done.then_some("2026-10-13T12:00:40Z")})
}
fn job(id: u64, name: &str, status: &str, conclusion: Value, steps: Vec<Value>) -> Value {
    json!({"id":id,"run_id":123,"run_attempt":2,"head_sha":SHA,"name":name,"status":status,
        "conclusion":conclusion,"runner_name":"hosted","labels":["ubuntu-latest"],
        "started_at":(status != "queued").then_some("2026-10-13T12:00:00Z"),
        // lint (502) finishes before the test job, so its log comes first.
        "completed_at":(status == "completed")
            .then_some(if id == 502 { "2026-10-13T12:00:50Z" } else { "2026-10-13T12:01:05Z" }),
        "check_run_url":format!("https://github.invalid/api/v3/repos/example/repo/check-runs/{id}"),
        "steps":steps})
}
fn attempt(run: &Value, jobs: Vec<Value>) -> Vec<GhStep> {
    vec![
        read(
            "repos/example/repo/actions/runs/123/attempts/2",
            run.clone(),
            false,
        ),
        read(
            "repos/example/repo/actions/runs/123/attempts/2/jobs?per_page=100",
            json!([{ "jobs": jobs }]),
            true,
        ),
    ]
}
/// One live check: the account, then the opened attempt and its jobs with their steps.
fn tick(run: &Value, jobs: Vec<Value>) -> Vec<GhStep> {
    let mut steps = current_account();
    steps.extend(attempt(run, jobs));
    steps
}
fn job_log(id: u64, text: &str) -> GhStep {
    let mut step = read(
        &format!("repos/example/repo/actions/jobs/{id}/logs"),
        json!(null),
        false,
    );
    step.stdout = text.as_bytes().to_vec();
    step
}
fn wait_long(session: &mut Session, text: &str) {
    if let Err(error) = session.try_wait_for(text, Duration::from_secs(15)) {
        session.save_artifacts();
        panic!("{error}; artifacts: {}", session.artifact_dir.display());
    }
}
/// A live update arrives within the backoff interval, so selection waits allow 15 s.
fn wait_selected_long(session: &mut Session, text: &str) {
    let title = crate::support::view_title("Actions");
    let deadline = Instant::now() + Duration::from_secs(15);
    while !session
        .selected_row(&title)
        .is_some_and(|row| row.contains(text))
    {
        if Instant::now() >= deadline {
            session.save_artifacts();
            panic!(
                "selected row {text:?} not painted (selected {:?}); artifacts: {}",
                session.selected_row(&title),
                session.artifact_dir.display()
            );
        }
        session.poll_output(Duration::from_millis(50));
    }
}

#[test]
fn v1_actions_live_run_keyboard_updates_status_and_logs_until_completed() {
    journey(false)
}
#[test]
fn v1_actions_live_run_mouse_updates_status_and_logs_until_completed() {
    journey(true)
}

fn journey(mouse: bool) {
    let fixture = fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let status = fixture.git(&["status", "--porcelain=v2"]);
    let queued = run("queued", Value::Null);
    let running = run("in_progress", Value::Null);
    let finished = run("completed", json!("success"));
    let mut steps = initial();
    steps[2].stdout =
        serde_json::to_vec(&json!({"total_count":1,"workflow_runs":[queued.clone()]})).unwrap();
    // Opened while queued: both jobs wait for a runner.
    steps.extend(attempt(
        &queued,
        vec![
            job(501, "matrix test linux", "queued", Value::Null, vec![]),
            job(502, "lint", "queued", Value::Null, vec![]),
        ],
    ));
    let opened = steps.len();
    // First live check: in progress; lint has finished, the test job is on its second step.
    steps.extend(tick(
        &running,
        vec![
            job(
                501,
                "matrix test linux",
                "in_progress",
                Value::Null,
                vec![
                    step(1, "Set up job", "completed", json!("success"), true),
                    step(2, "Run tests", "in_progress", Value::Null, false),
                ],
            ),
            job(
                502,
                "lint",
                "completed",
                json!("success"),
                vec![step(1, "Run lint", "completed", json!("success"), true)],
            ),
        ],
    ));
    let first_tick = steps.len();
    // Logs while the run is in progress: the finished lint log, by REST.
    steps.push(job_log(
        502,
        "2026-10-13T12:00:01.0000000Z ##[group]Run lint\n2026-10-13T12:00:02.0000000Z LINT CLEAN MARKER\n2026-10-13T12:00:03.0000000Z ##[endgroup]\n",
    ));
    let logs_opened = steps.len();
    // Second live check: the run completed; the test job's log is read as soon as it can be.
    steps.extend(tick(
        &finished,
        vec![
            job(
                501,
                "matrix test linux",
                "completed",
                json!("success"),
                vec![
                    step(1, "Set up job", "completed", json!("success"), true),
                    step(2, "Run tests", "completed", json!("success"), true),
                ],
            ),
            job(
                502,
                "lint",
                "completed",
                json!("success"),
                vec![step(1, "Run lint", "completed", json!("success"), true)],
            ),
        ],
    ));
    let mut test_log = String::new();
    for line in 0..60 {
        test_log.push_str(&format!(
            "2026-10-13T12:00:{:02}.0000000Z test output line {line:02}\n",
            line % 60
        ));
    }
    test_log.push_str("2026-10-13T12:01:00.0000000Z NEWEST TEST LINE MARKER\n");
    steps.push(job_log(501, &test_log));
    let finished_count = steps.len();
    fixture.scripted_gh(&steps);

    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "Actions", b"4");
    session.wait_for_screen(&run_cell(123, 2));
    if mouse {
        session.click_visible_action(&run_cell(123, 2));
    } else {
        session.send_key(b"\r");
    }
    session.wait_for_screen("Run 123 • attempt 2 • queued");
    // No Watch was pressed: the queued run is followed live, and Stop is on the bar.
    session.wait_for_screen("Live: updates every 5s until the run finishes");
    session.wait_for_control("[w Stop watch]");
    wait_row(&mut session, &["matrix test linux", "queued", "501"]);
    journal(&mut session, &fixture, opened);

    // queued -> in_progress without a manual refresh: job states and running times.
    wait_long(&mut session, "Run 123 • attempt 2 • in_progress");
    wait_row(
        &mut session,
        &["matrix test linux", "in progress", "running", "501"],
    );
    wait_row(&mut session, &["lint", "success", "50s", "502"]);
    journal(&mut session, &fixture, first_tick);

    // Logs of the whole run while it runs: the run row first, then Logs.
    if mouse {
        session.click_visible_action("Exact commit");
        wait_selected(&mut session, "Exact commit");
    } else {
        session.send_key(b"\x1b[H");
        wait_selected(&mut session, "Run 123 • attempt 2");
    }
    choose(&mut session, mouse, "[l Logs]", b"l");
    session.wait_for_screen("LINT CLEAN MARKER");
    session.wait_for_screen("── lint • success • 50s ──");
    session.wait_for_screen("── matrix test linux • in progress • running");
    session.wait_for_screen("Log appears when this job finishes");
    session.wait_for_screen("  Step 2 Run tests • in progress • running");
    session.wait_for_control("[F Follow]");
    session.wait_for_control("[w Stop watch]");
    assert!(!session.screen().contains("NEWEST TEST LINE MARKER"));
    journal(&mut session, &fixture, logs_opened);
    // Follow keeps the newest line selected as the log grows.
    choose(&mut session, mouse, "[F Follow]", b"F");
    session.wait_for_control("[F Stop following]");
    wait_selected(&mut session, "Step 2 Run tests");

    // in_progress -> completed: the finished job's log appears by itself, followed.
    wait_long(
        &mut session,
        "Watch ended: run 123 attempt 2 finished • success",
    );
    wait_selected_long(&mut session, "NEWEST TEST LINE MARKER");
    assert!(!session
        .screen()
        .contains("Log appears when this job finishes"));
    let rows = journal(&mut session, &fixture, finished_count);
    // A completed run is not polled again.
    session.poll_output(Duration::from_secs(7));
    journal(&mut session, &fixture, finished_count);
    session.resize(80, 24);
    wait_selected(&mut session, "NEWEST TEST LINE MARKER");
    if !mouse {
        // Followed to the end; the top of the log now heads the test job as finished.
        session.send_key(b"\x1b[H");
        session.wait_for_screen("── matrix test linux • success • 1m 05s ──");
    }

    // Reads only: no write method, and never `gh run view --log` on a run in progress.
    assert!(rows.iter().all(
        |row| row["args"]
            .as_array()
            .is_some_and(|args| !args.iter().any(|arg| matches!(
                arg.as_str(),
                Some("POST" | "PUT" | "PATCH" | "DELETE" | "--log")
            )))
    ));
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.git(&["status", "--porcelain=v2"]), status);
    session.quit_and_assert_restored_terminal(mouse);
}
