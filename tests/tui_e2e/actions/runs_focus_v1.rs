//! Opening a run lands on a job row, opened jobs keep their selection,
//! information rows keep Enter inside Actions, and `>`/`<` (or the matching
//! controls) walk failed jobs, steps and `##[error]` log lines.
use super::*;

const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn jobs_step() -> GhStep {
    let job = |id: u64, name: &str, conclusion: &str, steps: Value| {
        json!({"id":id,"run_id":123,"run_attempt":2,"head_sha":SHA,"name":name,"status":"completed",
            "conclusion":conclusion,"runner_name":"hosted","labels":["ubuntu-latest"],
            "check_run_url":format!("https://github.invalid/api/v3/repos/example/repo/check-runs/{id}"),
            "steps":steps})
    };
    read(
        "repos/example/repo/actions/runs/123/attempts/2/jobs?per_page=100",
        json!([{"jobs":[
            job(500, "lint", "success", json!([{"number":1,"name":"Lint","status":"completed","conclusion":"success"}])),
            job(501, "matrix test linux", "failure", json!([
                {"number":1,"name":"Set up job","status":"completed","conclusion":"success"},
                {"number":3,"name":"Run tests","status":"completed","conclusion":"failure","started_at":"2026-10-13T12:00:48Z","completed_at":"2026-10-13T12:03:17Z"}
            ]))
        ]}]),
        true,
    )
}

fn next_failure(session: &mut Session, mouse: bool) {
    if mouse {
        button(session, "[> Next failure]");
    } else {
        session.send_key(b">");
    }
}

#[test]
fn v1_actions_runs_keyboard_open_focuses_failed_job_steps_and_error_lines() {
    journey(false)
}
#[test]
fn v1_actions_runs_mouse_open_focuses_failed_job_steps_and_error_lines() {
    journey(true)
}

fn journey(mouse: bool) {
    let fixture = fixture();
    let mut steps = initial();
    steps.push(read(
        "repos/example/repo/actions/runs/123/attempts/2",
        raw_run(),
        false,
    ));
    steps.push(jobs_step());
    steps.push(GhStep::read(
        &[
            "run",
            "view",
            "123",
            "--repo",
            "github.invalid/example/repo",
            "--attempt",
            "2",
            "--log",
            "--job",
            "501",
        ],
        "matrix test linux\tUNKNOWN STEP\t\u{feff}2026-10-13T12:00:01.0000000Z Runner first line\n\
matrix test linux\tUNKNOWN STEP\t2026-10-13T12:00:02.0000000Z cargo test output\n\
matrix test linux\tUNKNOWN STEP\t2026-10-13T12:00:03.0000000Z ##[error]Process completed with exit code 101.\n\
matrix test linux\tUNKNOWN STEP\t2026-10-13T12:00:04.0000000Z Post job cleanup\n",
    ));
    // Back to the run re-reads the pinned attempt.
    steps.push(read(
        "repos/example/repo/actions/runs/123/attempts/2",
        raw_run(),
        false,
    ));
    steps.push(jobs_step());
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
    // Opening a run lands on its first failed job, not the run header.
    wait_row(&mut session, &["matrix test linux", "501"]);
    wait_selected(&mut session, "matrix test linux");
    journal(&mut session, &fixture, 5);

    // Information rows never route Enter/clicks out of Actions.
    if mouse {
        session.click_visible_action("triggering actor");
        wait_selected(&mut session, "triggering actor");
    } else {
        session.send_key(b"\x1b[H");
        wait_selected(&mut session, "Run 123 • attempt 2");
        session.send_key(b"\r");
        session.poll_output(std::time::Duration::from_millis(300));
        wait_selected(&mut session, "Run 123 • attempt 2");
    }
    assert!(!session.screen().contains("Inbox • focused view"));
    next_failure(&mut session, mouse);
    wait_selected(&mut session, "matrix test linux");

    // Opening the job keeps the selection on that job and shows its steps.
    if mouse {
        session.click_visible_action("matrix test linux");
    } else {
        session.send_key(b"\r");
    }
    wait_row(&mut session, &["3 Run tests", "failure", "2m 29s"]);
    wait_selected(&mut session, "matrix test linux");
    next_failure(&mut session, mouse);
    wait_selected(&mut session, "3 Run tests");

    // Logs: the byte-order-marked first line loses its timestamp; the next
    // failure jump lands on the runner's error line.
    choose(&mut session, mouse, "[l Logs]", b"l");
    session.wait_for_screen("Runner first line");
    assert!(!session.screen().contains("2026-10-13T12:00:01"));
    next_failure(&mut session, mouse);
    wait_selected(
        &mut session,
        "##[error]Process completed with exit code 101.",
    );
    journal(&mut session, &fixture, 6);

    // Back returns to the run with the opened job still selected.
    choose(&mut session, mouse, "[b Back]", b"b");
    wait_row(&mut session, &["lint", "success", "500"]);
    wait_selected(&mut session, "matrix test linux");
    session.resize(80, 24);
    wait_selected(&mut session, "matrix test linux");

    let records = journal(&mut session, &fixture, 8);
    assert!(!records
        .iter()
        .any(|row| row["args"].as_array().is_some_and(|args| args
            .iter()
            .any(|arg| matches!(arg.as_str(), Some("POST" | "PUT" | "PATCH" | "DELETE"))))));
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn v1_actions_runs_keyboard_logs_open_at_highlighted_job() {
    logs_at_highlighted_job(false)
}
#[test]
fn v1_actions_runs_mouse_logs_open_at_highlighted_job() {
    logs_at_highlighted_job(true)
}

/// Logs from the run page, before any job is opened, read the highlighted
/// (first failed) job's log rather than the whole run's.
fn logs_at_highlighted_job(mouse: bool) {
    let fixture = fixture();
    let mut steps = initial();
    steps.push(read(
        "repos/example/repo/actions/runs/123/attempts/2",
        raw_run(),
        false,
    ));
    steps.push(jobs_step());
    // The strict fixture only answers the exact job-scoped log read.
    steps.push(GhStep::read(
        &[
            "run",
            "view",
            "123",
            "--repo",
            "github.invalid/example/repo",
            "--attempt",
            "2",
            "--log",
            "--job",
            "501",
        ],
        "matrix test linux\tUNKNOWN STEP\t2026-10-13T12:00:01.0000000Z Failed job first line\n",
    ));
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
    wait_row(&mut session, &["matrix test linux", "501"]);
    wait_selected(&mut session, "matrix test linux");
    choose(&mut session, mouse, "[l Logs]", b"l");
    session.wait_for_screen("Failed job first line");
    let records = journal(&mut session, &fixture, 6);
    assert!(records.last().is_some_and(|row| row["args"]
        .as_array()
        .is_some_and(|args| args.iter().any(|arg| arg == "501"))));
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    session.quit_and_assert_restored_terminal(mouse);
}
