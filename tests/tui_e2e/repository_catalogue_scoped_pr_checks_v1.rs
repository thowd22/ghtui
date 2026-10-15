//! UNREGISTERED child of repository_catalogue_scoped_pr_resources_v1.
//! Four actual-binary key/mouse declarations. No executed/native PASS claim.
use super::*;
#[path = "repository_catalogue_scoped_pr_check_choices_v1.rs"]
mod choices;
#[path = "scoped_check_first_selection_v1.rs"]
mod first_selection;
#[path = "repository_catalogue_scoped_pr_checks_pending_v1.rs"]
mod pending;

fn current_repo() -> Value {
    let mut raw = repository(0);
    raw["url"] = json!(format!("https://{HOST}/api/v3/repos/{FIRST}"));
    raw
}
fn selected_check() -> Value {
    json!({"id":600,"node_id":"CHECK_600","head_sha":"a".repeat(40),"name":"Exact observed build", "status":"completed","conclusion":"failure",
        "url":format!("https://{HOST}/api/v3/repos/{FIRST}/check-runs/600"),"check_suite":{"id":900},
        "details_url":"https://foreign.invalid/unrelated/repo/actions/runs/999"})
}
fn observed_run() -> Value {
    json!({"id":123,"node_id":"RUN_123","workflow_id":7,"check_suite_id":900,"run_attempt":2,
        "head_sha":"a".repeat(40),"head_branch":"feature/scoped-open","name":"Exact CI", "display_title":"Selected exact CI",
        "event":"pull_request","status":"completed","conclusion":"failure",
        "url":format!("https://{HOST}/api/v3/repos/{FIRST}/actions/runs/123"),"html_url":format!("https://{HOST}/{FIRST}/actions/runs/123"),
        "repository":current_repo(),"actor":{"id":51,"node_id":"USER_51","login":"alice","type":"User"},
        "triggering_actor":{"id":42,"node_id":"USER_42","login":"octocat","type":"User"}})
}
fn observed_job() -> Value {
    json!({"id":501,"node_id":"JOB_501","run_id":123,"head_sha":"a".repeat(40),"name":"test (stable)","status":"completed","conclusion":"failure",
        "url":format!("https://{HOST}/api/v3/repos/{FIRST}/actions/jobs/501"),"check_run_url":format!("https://{HOST}/api/v3/repos/{FIRST}/check-runs/600"),
        "steps":[{"number":1,"name":"Compile exact source","status":"completed","conclusion":"success"},
            {"number":2,"name":"Test exact source","status":"completed","conclusion":"failure"}]})
}
fn who(uid: u64) -> GhStep {
    get("user", json!({"id":uid,"login":"octocat","type":"User"}))
}
fn bridge() -> Vec<GhStep> {
    let mut op = principal(42);
    op.push(get(&format!("repos/{FIRST}"), repository(0)));
    op.extend([
        who(42),
        get(&format!("repos/{FIRST}"), current_repo()),
        get(&format!("repos/{FIRST}/pulls/7"), parent_resource()),
        get(&format!("repos/{FIRST}/check-runs/600"), selected_check()),
        get(
            &format!(
                "repos/{FIRST}/actions/runs?check_suite_id=900&head_sha={}&per_page=100&page=1",
                "a".repeat(40)
            ),
            json!({"total_count":1,"workflow_runs":[observed_run()]}),
        ),
        get(
            &format!("repos/{FIRST}/actions/runs/123/attempts/2"),
            observed_run(),
        ),
        get(
            &format!("repos/{FIRST}/actions/runs/123/attempts/2/jobs?per_page=100&page=1"),
            json!({"total_count":1,"jobs":[observed_job()]}),
        ),
        get(&format!("repos/{FIRST}/actions/runs/123"), observed_run()),
        get(&format!("repos/{FIRST}/check-runs/600"), selected_check()),
        get(&format!("repos/{FIRST}/pulls/7"), parent_resource()),
        get(&format!("repos/{FIRST}"), current_repo()),
        who(42),
        get(&format!("repos/{FIRST}"), repository(0)),
    ]);
    op.extend(principal(42));
    assert_eq!(op.len(), 18);
    op
}
pub(super) fn bridge_for_head(head: &str) -> Vec<GhStep> {
    let mut steps = bridge();
    for step in &mut steps {
        for arg in &mut step.args {
            *arg = arg.replace(&"a".repeat(40), head);
        }
        step.stdout = String::from_utf8(std::mem::take(&mut step.stdout))
            .unwrap()
            .replace(&"a".repeat(40), head)
            .into_bytes();
    }
    steps
}
fn logs() -> Vec<GhStep> {
    let body = std::iter::once("EXACT_MATCHED_JOB_LOG_CANARY • 界🦀".to_owned())
        .chain((0..40).map(|n| format!("Scoped exact log line {n:02}")))
        .collect::<Vec<_>>()
        .join("\n");
    let mut op = principal(42);
    op.push(get(&format!("repos/{FIRST}"), repository(0)));
    op.extend([
        who(42),
        get(&format!("repos/{FIRST}"), current_repo()),
        get(&format!("repos/{FIRST}/pulls/7"), parent_resource()),
        get(&format!("repos/{FIRST}/actions/runs/123"), observed_run()),
        GhStep::read(
            &[
                "run",
                "view",
                "123",
                "--repo",
                &format!("{HOST}/{FIRST}"),
                "--attempt",
                "2",
                "--log",
                "--job",
                "501",
            ],
            &body,
        ),
        get(&format!("repos/{FIRST}/actions/runs/123"), observed_run()),
        get(&format!("repos/{FIRST}/pulls/7"), parent_resource()),
        get(&format!("repos/{FIRST}"), current_repo()),
        who(42),
        get(&format!("repos/{FIRST}"), repository(0)),
    ]);
    op.extend(principal(42));
    assert_eq!(op.len(), 15);
    op
}
pub(super) fn logs_for_head(head: &str) -> Vec<GhStep> {
    let mut steps = logs();
    for step in &mut steps {
        for arg in &mut step.args {
            *arg = arg.replace(&"a".repeat(40), head);
        }
        step.stdout = String::from_utf8(std::mem::take(&mut step.stdout))
            .unwrap()
            .replace(&"a".repeat(40), head)
            .into_bytes();
    }
    steps
}
fn steps(logs: Vec<GhStep>) -> Vec<GhStep> {
    let mut steps = initial();
    steps.extend(resource_operation(true));
    steps.extend(bridge());
    steps.extend(logs);
    steps
}
fn open_run(session: &mut Session, fixture: &Fixture, mouse: bool) {
    selected(session, fixture, mouse);
    choose(session, mouse, "[k Checks]", b"k");
    session.wait_for_command_count(fixture, 41);
    ready(
        session,
        &[
            "Required rules unavailable",
            "Exact observed build",
            "ci-actor",
            "[Enter Open check]",
        ],
    );
    select_observed_row(session, mouse, "Exact observed build");
    if mouse {
        session.click_visible_action("[Enter Open check]");
    } else {
        session.focus_visible_action("[Enter Open check]");
        session.send_key(b"\r");
    }
    session.wait_for_command_count(fixture, 59);
    ready(
        session,
        &[
            "Run 123 • attempt 2",
            "Job 501",
            "verified exact endpoint",
            "alice",
            &"a".repeat(40),
        ],
    );
    assert!(!session.screen().contains(DETAIL_BODY));
    assert!(!session.screen().contains(PRIVATE_DRAFT));
    assert!(!session.screen().contains("run 999"));
}
pub(super) fn select_observed_row(session: &mut Session, mouse: bool, label: &str) {
    select_observed_row_in(session, mouse, "Inbox • focused view", label);
}
pub(super) fn select_observed_row_in(session: &mut Session, mouse: bool, title: &str, label: &str) {
    // Keep the former20x20ms navigation budget. A ready repaint is not an
    // acknowledgement of a Down key: inspect the completed selected row,
    // move once in the actual required direction, then await its result.
    session.select_completed_row_with_timeout(mouse, title, label, Duration::from_millis(400));
    assert!(
        crate::support::completed_navigation_controls_v1::progress_selection_in(
            session.completed_cursorless_screen().unwrap(),
            title,
            label,
        )
        .is_some_and(|position| position.selected_row == Some(position.target.1)),
        "selected the genuine completed check row, not a notice/status"
    );
}
pub(super) fn control(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    if !mouse {
        session.send_key(key);
        return;
    }
    fn page(screen: &str) -> Option<(usize, usize)> {
        let line = screen
            .lines()
            .find(|line| line.contains("• [] change control page"))?;
        let fraction = line.split('•').next()?.split_whitespace().last()?;
        let (current, total) = fraction.split_once('/')?;
        Some((current.parse().ok()?, total.parse().ok()?))
    }
    // A previous action can leave the toolbar on any page. Walk back to the
    // beginning, then forward, acknowledging each actual rendered transition
    // before sending another click. Never saturate the queue on a stale frame.
    let mut forward = false;
    for _ in 0..64 {
        if session.screen().contains(label) {
            session.click_visible_action(label);
            return;
        }
        let (current, total) = page(&session.screen()).expect("Actual control page counter");
        forward |= current == 1;
        if forward && current == total {
            break;
        }
        let expected = if forward { current + 1 } else { current - 1 };
        session.click_visible_action(if forward {
            "[Controls →]"
        } else {
            "[← Controls]"
        });
        session
            .try_wait_for_condition(
                |screen| page(screen) == Some((expected, total)),
                "One acknowledged control-page transition",
                Duration::from_secs(3),
            )
            .unwrap();
    }
    session.save_artifacts();
    panic!("Scoped shared control not visible: {label}");
}
fn close_notice(session: &mut Session, mouse: bool) {
    if mouse {
        session.close_overlay_by_mouse("Action unavailable");
    } else {
        session.close_overlay_by_key("Action unavailable");
    }
}
fn read_only_control(session: &mut Session, fixture: &Fixture, mouse: bool) {
    control(session, mouse, "[Runs unavailable:", b"R");
    overlay_ready(
        session,
        "Action unavailable",
        &["scoped PR-check", "nothing sent", "[Esc Close]"],
    );
    let body = session.wait_for_overlay_body("Action unavailable");
    assert!(visible_words(&body).contains("original"));
    assert_eq!(commands(session, fixture).len(), 59);
    close_notice(session, mouse);
    ready(session, &["Run 123 • attempt 2"]);
}
fn happy_bridge(mouse: bool) {
    let expected = steps(logs());
    assert_eq!(expected.len(), 74);
    let fixture = resource_configured(&expected);
    let git_before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    open_run(&mut session, &fixture, mouse);
    let before = sql_state(&fixture);
    let trace = git_commands(&fixture);
    read_only_control(&mut session, &fixture, mouse);
    if mouse {
        session.click_visible_action("Job 501");
    } else {
        session.focus_visible_action("Job 501");
        session.send_key(b"\r");
    }
    let deadline = Instant::now() + Duration::from_secs(8);
    while !(session.screen().contains("Step 2") && session.screen().contains("Test exact source")) {
        assert!(
            Instant::now() < deadline,
            "Exact job steps never became visible"
        );
        if mouse {
            let (column, row) = session
                .locate("Step 1")
                .or_else(|| session.locate("Job 501"))
                .or_else(|| session.locate("Run 123"))
                .expect("Navigate the actual prepared job body");
            session.wheel(column, row, true);
        } else {
            session.send_key(b"\x1b[6~");
        }
        session.poll_output(Duration::from_millis(20));
    }
    ready(&mut session, &["Step 2", "Test exact source"]);
    assert_eq!(commands(&session, &fixture).len(), 59);
    control(&mut session, mouse, "[l Logs]", b"l");
    session.wait_for_command_count(&fixture, 74);
    ready(
        &mut session,
        &["EXACT_MATCHED_JOB_LOG_CANARY", "Scoped exact log line 00"],
    );
    // Scoped search is a pure captured-snapshot editor, never legacy Form/SQL.
    choices::continuations_v2::query_for_job(
        &mut session,
        mouse,
        501,
        false,
        "Scoped exact log line 00",
    );
    ready(&mut session, &["1 matches in captured job 501 attempt 2"]);
    assert_eq!(sql_state(&fixture), before);
    assert_eq!(commands(&session, &fixture).len(), 74);
    session.resize(if mouse { 120 } else { 80 }, if mouse { 40 } else { 24 });
    session.wait_for_main_frame();
    for _ in 0..45 {
        if session.screen().contains("Scoped exact log line 30") {
            break;
        }
        let before_scroll = session
            .screen()
            .lines()
            .filter(|line| line.contains("Scoped exact log line"))
            .collect::<Vec<_>>()
            .join("\n");
        if mouse {
            let (column, row) = session.locate("Scoped exact log line").unwrap();
            session.wheel(column, row, true);
        } else {
            session.send_key(b"\x1b[B");
        }
        session
            .try_wait_for_condition(
                |screen| {
                    screen
                        .lines()
                        .filter(|line| line.contains("Scoped exact log line"))
                        .collect::<Vec<_>>()
                        .join("\n")
                        != before_scroll
                },
                "One actual loaded-log selection/scroll transition",
                Duration::from_secs(3),
            )
            .unwrap();
    }
    ready(&mut session, &["Scoped exact log line 30"]);
    assert_eq!(commands(&session, &fixture).len(), 74);
    control(&mut session, mouse, "[b Back to PR checks]", b"b");
    ready(
        &mut session,
        &[
            "Exact observed build",
            "Required rules unavailable",
            "[b Back]",
        ],
    );
    assert!(!session.screen().contains("EXACT_MATCHED_JOB_LOG_CANARY"));
    assert!(!session.screen().contains("Run 123 • attempt 2"));
    choose(&mut session, mouse, "[b Back]", b"b");
    ready(&mut session, &[ROW_TITLE, "Cached • fetched"]);
    unchanged(&session, &fixture, &before, &expected);
    assert_eq!(git_state(&fixture), git_before);
    assert_eq!(git_commands(&fixture), trace);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    session.quit_and_assert_restored_terminal(mouse);
    exact_reads(&session, &fixture, &expected);
    no_attempts(&fixture);
}
fn lost_log(mouse: bool) {
    let mut log = logs();
    log[11] = who(43);
    log.truncate(12);
    let expected = steps(log);
    assert_eq!(expected.len(), 71);
    let fixture = resource_configured(&expected);
    let git_before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    open_run(&mut session, &fixture, mouse);
    let before = sql_state(&fixture);
    let trace = git_commands(&fixture);
    control(&mut session, mouse, "[l Logs]", b"l");
    session.wait_for_command_count(&fixture, 71);
    session
        .try_wait_for_condition(
            |screen| {
                visible_words(screen).contains("Original repository scope is fenced")
                    && !screen.contains("EXACT_MATCHED_JOB_LOG_CANARY")
                    && !screen.contains(DETAIL_BODY)
                    && !screen.contains("Run 123 • attempt 2")
                    && !screen.contains(PRIVATE_DRAFT)
            },
            "Fresh inner closing UID43 hides captured logs and all original private pages",
            Duration::from_secs(8),
        )
        .unwrap();
    choose(&mut session, mouse, "1 Inbox", b"1");
    choose(&mut session, mouse, "[r Refresh]", b"r");
    session.resize(if mouse { 120 } else { 80 }, if mouse { 40 } else { 24 });
    session.poll_output(Duration::from_millis(100));
    assert_eq!(commands(&session, &fixture).len(), 71);
    assert_eq!(sql_state(&fixture), before);
    no_attempts(&fixture);
    choose(&mut session, mouse, "2 Repository", b"2");
    ready(&mut session, &["Recent commits"]);
    assert_read_only_git(&fixture);
    assert_eq!(git_state(&fixture), git_before);
    assert_eq!(git_commands(&fixture), trace);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    session.quit_and_assert_restored_terminal(mouse);
    exact_reads(&session, &fixture, &expected);
    assert_eq!(sql_state(&fixture), before);
}
#[test]
fn repository_catalogue_scoped_check_actions_keyboard_exact_bridge_and_matched_logs() {
    happy_bridge(false);
}
#[test]
fn repository_catalogue_scoped_check_actions_mouse_exact_bridge_and_matched_logs() {
    happy_bridge(true);
}
#[test]
fn repository_catalogue_scoped_check_actions_keyboard_late_log_scope_loss_no_fallback() {
    lost_log(false);
}
#[test]
fn repository_catalogue_scoped_check_actions_mouse_late_log_scope_loss_no_fallback() {
    lost_log(true);
}
