//! UNREGISTERED child of frozen scoped checks E2E. Eight independent real
//! binaries/key+mouse declarations; all positive choices originate Source7.
use super::*;
#[path = "repository_catalogue_scoped_pr_check_continuations_v2.rs"]
pub(super) mod continuations_v2;

const CHOICE_TITLE: &str = "Observed PR check candidates";
const OTHER_LOG: &str = "OTHER_EXACT_ATTEMPT_JOB_LOG_CANARY";
fn chosen_run(id: u64) -> Value {
    let mut row = observed_run();
    row["id"] = json!(id);
    row["node_id"] = json!(format!("RUN_{id}"));
    row["display_title"] = json!(format!("Explicit genuine run {id}"));
    row["url"] = json!(format!(
        "https://{HOST}/api/v3/repos/{FIRST}/actions/runs/{id}"
    ));
    row["html_url"] = json!(format!("https://{HOST}/{FIRST}/actions/runs/{id}"));
    row
}
fn chosen_job(run: u64, id: u64) -> Value {
    let mut row = observed_job();
    row["id"] = json!(id);
    row["node_id"] = json!(format!("JOB_{id}"));
    row["run_id"] = json!(run);
    row["name"] = json!(format!("Exact member {id} 界🦀"));
    row["url"] = json!(format!(
        "https://{HOST}/api/v3/repos/{FIRST}/actions/jobs/{id}"
    ));
    if id != 501 {
        row["check_run_url"] = json!(format!(
            "https://{HOST}/api/v3/repos/{FIRST}/check-runs/601"
        ));
    }
    row
}
fn member_rows(run: u64) -> Vec<Value> {
    vec![chosen_job(run, 501), chosen_job(run, 502)]
}
fn candidate_initial(ambiguous: bool) -> Vec<GhStep> {
    let mut op = bridge();
    op[7] = get(
        &format!(
            "repos/{FIRST}/actions/runs?check_suite_id=900&head_sha={}&per_page=100&page=1",
            "a".repeat(40)
        ),
        json!({"total_count":2,"workflow_runs":if ambiguous {vec![chosen_run(123),chosen_run(124)]}else{vec![chosen_run(123)]}}),
    );
    if ambiguous {
        op.drain(8..11);
    } else {
        op[8] = get(
            &format!("repos/{FIRST}/actions/runs/123/attempts/2"),
            chosen_run(123),
        );
        op[9] = get(
            &format!("repos/{FIRST}/actions/runs/123/attempts/2/jobs?per_page=100&page=1"),
            json!({"total_count":2,"jobs":member_rows(123)}),
        );
        op[10] = get(&format!("repos/{FIRST}/actions/runs/123"), chosen_run(123));
    }
    op
}
fn selected_read(run: u64) -> Vec<GhStep> {
    let mut op = bridge();
    op[7] = get(
        &format!("repos/{FIRST}/actions/runs/{run}"),
        chosen_run(run),
    );
    op[8] = get(
        &format!("repos/{FIRST}/actions/runs/{run}/attempts/2"),
        chosen_run(run),
    );
    op[9] = get(
        &format!("repos/{FIRST}/actions/runs/{run}/attempts/2/jobs?per_page=100&page=1"),
        json!({"total_count":2,"jobs":member_rows(run)}),
    );
    op[10] = get(
        &format!("repos/{FIRST}/actions/runs/{run}"),
        chosen_run(run),
    );
    assert_eq!(op.len(), 18);
    op
}
fn other_read(run: u64, missing: bool, lost: bool) -> Vec<GhStep> {
    let body = std::iter::once(format!("{OTHER_LOG} • 界🦀"))
        .chain((0..45).map(|n| format!("Other selected log line {n:02}")))
        .collect::<Vec<_>>()
        .join("\n");
    let mut op = principal(42);
    op.push(get(&format!("repos/{FIRST}"), repository(0)));
    op.extend([
        who(42),get(&format!("repos/{FIRST}"),current_repo()),get(&format!("repos/{FIRST}/pulls/7"),parent_resource()),
        get(&format!("repos/{FIRST}/actions/runs/{run}"),chosen_run(run)),get(&format!("repos/{FIRST}/actions/runs/{run}/attempts/2"),chosen_run(run)),
        get(&format!("repos/{FIRST}/actions/runs/{run}/attempts/2/jobs?per_page=100&page=1"),json!({"total_count":if missing{1}else{2},"jobs":if missing{vec![chosen_job(run,501)]}else{member_rows(run)}})),
        GhStep::read(&["run","view",&run.to_string(),"--repo",&format!("{HOST}/{FIRST}"),"--attempt","2","--log","--job","502"],&body),
        get(&format!("repos/{FIRST}/actions/jobs/502"),chosen_job(run,502)),get(&format!("repos/{FIRST}/actions/runs/{run}"),chosen_run(run)),
        get(&format!("repos/{FIRST}/pulls/7"),parent_resource()),get(&format!("repos/{FIRST}"),current_repo()),who(if lost{43}else{42}),get(&format!("repos/{FIRST}"),repository(0)),]);
    op.extend(principal(42));
    assert_eq!(op.len(), 18);
    if lost {
        op.truncate(15);
    } else if missing {
        op.drain(9..11);
    }
    op
}
fn script_choices(ambiguous: bool, missing: bool, lost: bool) -> Vec<GhStep> {
    let run = if ambiguous { 124 } else { 123 };
    let mut op = initial();
    op.extend(resource_operation(true));
    op.extend(candidate_initial(ambiguous));
    op.extend(selected_read(run));
    op.extend(other_read(run, missing, lost));
    op
}
fn candidate_source(session: &mut Session, fixture: &Fixture, mouse: bool, ambiguous: bool) {
    selected(session, fixture, mouse);
    choose(session, mouse, "[k Checks]", b"k");
    session.wait_for_command_count(fixture, 41);
    ready(
        session,
        &[
            "Exact observed build",
            "Required rules unavailable",
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
    session.wait_for_command_count(fixture, if ambiguous { 56 } else { 59 });
    overlay_ready(
        session,
        "Action unavailable",
        &[
            if ambiguous {
                "More than one run"
            } else {
                "partial"
            },
            "[Esc Close]",
        ],
    );
    close_notice(session, mouse);
    ready(session, &["[C Candidates]", "Exact observed build"]);
    assert!(!session.screen().contains("Run 123 • attempt 2"));
    assert!(!session.screen().contains(DETAIL_BODY));
    assert!(!session.screen().contains(PRIVATE_DRAFT));
}
fn completed_back_to_checks(session: &mut Session, mouse: bool) {
    let previous = session.completed_cursorless_end();
    control(session, mouse, "[b Back to PR checks]", b"b");
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(5));
        if session.completed_cursorless_end() > previous
            && session.completed_cursorless_screen().is_some_and(|frame| {
                let text = frame.contents();
                text.contains("[f Files]")
                    && text.contains("[k Checks]")
                    && text.contains("Exact observed build")
                    && text.contains("Required rules unavailable")
                    && !text.contains("[C Candidates]")
                    && !text.contains("[Esc Cancel]")
            })
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "No new completed cached Checks Back frame\n{}",
            session.screen()
        );
    }
}
fn pick(
    session: &mut Session,
    fixture: &Fixture,
    mouse: bool,
    index: usize,
    expected_count: usize,
    ambiguous: bool,
) {
    choose(session, mouse, "[C Candidates]", b"C");
    overlay_ready(
        session,
        CHOICE_TITLE,
        &[
            "SEEN",
            if ambiguous { "Run 124" } else { "Run 123" },
            "[Enter Choose]",
            "[Esc Close]",
            "[q Quit]",
        ],
    );
    let before = commands(session, fixture).len();
    session.resize(if mouse { 80 } else { 120 }, if mouse { 24 } else { 40 });
    overlay_ready(session, CHOICE_TITLE, &["[Enter Choose]", "[Esc Close]"]);
    // Close/reopen captures the modal without firing an underlying Refresh or
    // generic Compose. No private draft/candidate fetch occurs on either path.
    if mouse {
        session.click_visible_action("[Esc Close]");
    } else {
        session.send_key(b"\x1b");
    }
    session.wait_for_overlay_close(CHOICE_TITLE);
    assert_eq!(commands(session, fixture).len(), before);
    choose(session, mouse, "[C Candidates]", b"C");
    overlay_ready(session, CHOICE_TITLE, &["[Enter Choose]"]);
    if mouse {
        session.click_visible_action(if ambiguous {
            "Run 124 •"
        } else {
            "Run 123 •"
        });
    } else {
        for _ in 0..index {
            session.send_key(b"\x1b[B");
        }
    }
    if mouse {
        session.click_visible_action("[Enter Choose]");
    } else {
        session.send_key(b"\r");
    }
    session.wait_for_overlay_close(CHOICE_TITLE);
    session.wait_for_command_count(fixture, expected_count);
    ready(
        session,
        &[
            if ambiguous {
                "Run 124 • attempt 2"
            } else {
                "Run 123 • attempt 2"
            },
            "verified exact endpoint",
            "[C Candidates]",
        ],
    );
    // At 80x24 the second actual job is below the body viewport. Navigate the
    // body independently of the fixed toolbar before asserting/opening it.
    for _ in 0..12 {
        if session.screen().contains("Job 502") {
            break;
        }
        if mouse {
            let (column, row) = session
                .locate("Job 501")
                .or_else(|| session.locate("Run "))
                .unwrap();
            session.wheel(column, row, true);
        } else {
            session.send_key(b"\x1b[B");
        }
        session.poll_output(Duration::from_millis(20));
    }
    ready(session, &["Job 502", "[C Candidates]"]);
}
fn another_job(session: &mut Session, fixture: &Fixture, mouse: bool, count: usize) {
    if mouse {
        session.click_visible_action("Job 502");
    } else {
        session.focus_visible_action("Job 502");
        session.send_key(b"\r");
    }
    let deadline = Instant::now() + Duration::from_secs(8);
    while !(session.screen().contains("Step 2") && session.screen().contains("Test exact source")) {
        assert!(Instant::now() < deadline, "Chosen job steps became visible");
        if mouse {
            let (column, row) = session
                .locate("Step 1")
                .or_else(|| session.locate("Job 502"))
                .or_else(|| session.locate("Run "))
                .expect("Navigate the actual chosen-job body");
            session.wheel(column, row, true);
        } else {
            session.send_key(b"\x1b[B");
        }
        session.poll_output(Duration::from_millis(20));
    }
    ready(session, &["Step 2", "Test exact source"]);
    assert_eq!(commands(session, fixture).len(), count);
    // The shared viewer stays read-only even after an explicit choice: refusal
    // must precede every generic private-draft/journal or mutation route.
    control(session, mouse, "[Runs unavailable:", b"R");
    overlay_ready(
        session,
        "Action unavailable",
        &["nothing sent", "[Esc Close]"],
    );
    close_notice(session, mouse);
    assert_eq!(commands(session, fixture).len(), count);
    control(session, mouse, "[l Logs]", b"l");
}
fn chooser_journey(mouse: bool, ambiguous: bool, missing: bool, lost: bool) {
    let expected = script_choices(ambiguous, missing, lost);
    let fixture = resource_configured(&expected);
    let git_before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    candidate_source(&mut session, &fixture, mouse, ambiguous);
    let before = sql_state(&fixture);
    let trace = git_commands(&fixture);
    let selected_count = if ambiguous { 74 } else { 77 };
    pick(
        &mut session,
        &fixture,
        mouse,
        usize::from(ambiguous),
        selected_count,
        ambiguous,
    );
    assert_eq!(sql_state(&fixture), before);
    another_job(&mut session, &fixture, mouse, selected_count);
    session.wait_for_command_count(&fixture, expected.len());
    if lost {
        session
            .try_wait_for_condition(
                |screen| {
                    visible_words(screen).contains("Original repository scope is fenced")
                        && !screen.contains(OTHER_LOG)
                        && !screen.contains(DETAIL_BODY)
                        && !screen.contains(PRIVATE_DRAFT)
                        && !screen.contains("Run 123 • attempt 2")
                },
                "Closing UID43 hides other-job captured logs without default Actions fallback",
                Duration::from_secs(8),
            )
            .unwrap();
        choose(&mut session, mouse, "1 Inbox", b"1");
        choose(&mut session, mouse, "[r Refresh]", b"r");
        session.poll_output(Duration::from_millis(100));
        assert_eq!(commands(&session, &fixture).len(), expected.len());
    } else if missing {
        overlay_ready(&mut session, "Action unavailable", &["[Esc Close]"]);
        let explanation = session
            .wait_for_overlay_body("Action unavailable")
            .split_whitespace()
            .collect::<String>();
        assert!(explanation.contains("ChosenjobnotpresentintheexhaustedAPI-visibleattemptlist;nohistoricaldeletioninferred"),
            "Actual member-unavailable explanation across terminal wrapping: {explanation}");
        close_notice(&mut session, mouse);
        ready(&mut session, &["Run 123 • attempt 2"]);
        assert!(!session.screen().contains(OTHER_LOG));
        assert!(!session.screen().contains("logs: 0"));
        assert_eq!(commands(&session, &fixture).len(), 93);
        completed_back_to_checks(&mut session, mouse);
        ready(
            &mut session,
            &["Exact observed build", "Required rules unavailable"],
        );
    } else {
        ready(&mut session, &[OTHER_LOG, "Other selected log line 00"]);
        assert!(!session.screen().contains("EXACT_MATCHED_JOB_LOG_CANARY"));
        assert!(!session.screen().contains(PRIVATE_DRAFT));
        for _ in 0..50 {
            if session.screen().contains("Other selected log line 30") {
                break;
            }
            let before_scroll = session
                .screen()
                .lines()
                .filter(|line| line.contains("Other selected log line"))
                .collect::<Vec<_>>()
                .join("\n");
            if mouse {
                let (x, y) = session.locate("Other selected log line").unwrap();
                session.wheel(x, y, true);
            } else {
                session.send_key(b"\x1b[B");
            }
            session
                .try_wait_for_condition(
                    |screen| {
                        screen
                            .lines()
                            .filter(|line| line.contains("Other selected log line"))
                            .collect::<Vec<_>>()
                            .join("\n")
                            != before_scroll
                    },
                    "One acknowledged genuine other-job log selection/scroll",
                    Duration::from_secs(3),
                )
                .unwrap();
        }
        ready(&mut session, &["Other selected log line 30"]);
        assert_eq!(commands(&session, &fixture).len(), expected.len());
        continuations_v2::query_for_job(&mut session, mouse, 502, false, OTHER_LOG);
        ready(&mut session, &["1 matches in captured job 502 attempt 2"]);
        assert_eq!(sql_state(&fixture), before);
        completed_back_to_checks(&mut session, mouse);
        ready(
            &mut session,
            &["Exact observed build", "Required rules unavailable"],
        );
        assert!(!session.screen().contains(OTHER_LOG));
    }
    session.resize(if mouse { 120 } else { 80 }, if mouse { 40 } else { 24 });
    assert_eq!(sql_state(&fixture), before);
    assert_eq!(commands(&session, &fixture).len(), expected.len());
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
fn repository_catalogue_check_choices_keyboard_partial_positive_and_other_logs() {
    chooser_journey(false, false, false, false);
}
#[test]
fn repository_catalogue_check_choices_mouse_partial_positive_and_other_logs() {
    chooser_journey(true, false, false, false);
}
#[test]
fn repository_catalogue_check_choices_keyboard_ambiguous_second_not_default_first() {
    chooser_journey(false, true, false, false);
}
#[test]
fn repository_catalogue_check_choices_mouse_ambiguous_second_not_default_first() {
    chooser_journey(true, true, false, false);
}
#[test]
fn repository_catalogue_check_choices_keyboard_missing_member_no_false_empty() {
    chooser_journey(false, false, true, false);
}
#[test]
fn repository_catalogue_check_choices_mouse_missing_member_no_false_empty() {
    chooser_journey(true, false, true, false);
}
#[test]
fn repository_catalogue_check_choices_keyboard_late_uid_reuse_hides_log() {
    chooser_journey(false, false, false, true);
}
#[test]
fn repository_catalogue_check_choices_mouse_late_uid_reuse_hides_log() {
    chooser_journey(true, false, false, true);
}
