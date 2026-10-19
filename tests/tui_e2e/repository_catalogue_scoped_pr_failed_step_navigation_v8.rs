//! Actual binary, original closed Source/PR/Checks and independent effects.
use super::*;

fn step_rows(known: bool) -> Value {
    let mut rows = vec![
        json!({"number":1,"name":"Compile exact source","status":"completed","conclusion":"success"}),
        json!({"number":0,"name":"Unknown reported number","status":"completed","conclusion":"failure"}),
        json!({"number":18,"name":"Unknown conclusion","status":"completed","conclusion":null}),
        json!({"number":19,"name":"Unknown completion","status":null,"conclusion":"failure"}),
        json!({"number":20,"name":"Still running","status":"in_progress","conclusion":"failure"}),
    ];
    if known {
        rows.insert(1,json!({"number":2,"name":"Test exact source","status":"completed","conclusion":"failure"}));
        rows.push(json!({"number":17,"name":"Sparse reported failure 界🦀","status":"completed","conclusion":"failure"}));
    }
    json!(rows)
}
/// A step row's JOB cell starts `N name`; this prefix stays whole even at 40 columns.
fn step_cell(number: u64) -> &'static str {
    match number {
        0 => "0 Unknown rep",
        2 => "2 Test exact",
        17 => "17 Sparse rep",
        _ => unreachable!("no step {number} in this fixture"),
    }
}
fn original(known: bool) -> Vec<GhStep> {
    let mut op = bridge();
    let mut member = observed_job();
    member["steps"] = step_rows(known);
    op[9] = get(
        &format!("repos/{FIRST}/actions/runs/123/attempts/2/jobs?per_page=100&page=1"),
        json!({"total_count":1,"jobs":[member]}),
    );
    let mut all = initial();
    all.extend(resource_operation(true));
    all.extend(op);
    assert_eq!(all.len(), 59);
    all
}
fn open_job(session: &mut Session, fixture: &Fixture, mouse: bool) {
    let previous = session.completed_cursorless_end();
    if mouse {
        session.click_visible_action("test (stable)")
    } else {
        session.focus_visible_action("test (stable)");
        session.send_key(b"\r");
    }
    completed_action(session, previous, "Jobs and steps");
    ready(
        session,
        &["[> Next failed step]", "[< Previous failed step]"],
    );
    assert_eq!(commands(session, fixture).len(), 59);
}
fn navigate(session: &mut Session, mouse: bool, next: bool, number: u64) {
    let previous = session.completed_cursorless_end();
    control(
        session,
        mouse,
        if next {
            "[> Next failed step]"
        } else {
            "[< Previous failed step]"
        },
        if next { b">" } else { b"<" },
    );
    completed_action(session, previous, &format!("Observed failed step {number}"));
    let step = step_cell(number);
    ready(session, &[step]);
    assert!(
        session
            .screen()
            .lines()
            .any(|line| line.contains('▌') && line.contains(step)),
        "Actual selected failed row\n{}",
        session.screen()
    );
}
fn journey(mouse: bool, known: bool, lost: bool) {
    let mut expected = original(known);
    if lost {
        let mut refresh = selected_read(123);
        refresh[14] = who(43);
        refresh.truncate(15);
        expected.extend(refresh);
    } else if known {
        expected.extend(logs());
    }
    let fixture = resource_configured(&expected);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    open_run(&mut session, &fixture, mouse);
    open_job(&mut session, &fixture, mouse);
    let before = sql_state(&fixture);
    let trace = git_commands(&fixture);
    if lost {
        let previous = session.completed_cursorless_end();
        control(&mut session, mouse, "[r Refresh]", b"r");
        session.wait_for_command_count(&fixture, 74);
        completed_action(&mut session, previous, "private results hidden");
        assert!(!session.screen().contains("[> Next failed step]"));
        assert!(!session.screen().contains(step_cell(2)));
        session.send_key(b">");
        session.poll_output(Duration::from_millis(30));
        assert_eq!(commands(&session, &fixture).len(), 74);
        assert!(!session.screen().contains("Sparse reported failure"));
    } else if known {
        for (next, number) in [(true, 2), (true, 17), (true, 2), (false, 17)] {
            navigate(&mut session, mouse, next, number);
            assert_eq!(commands(&session, &fixture).len(), 59);
            assert_eq!(sql_state(&fixture), before);
        }
        for width in [40, 80, 120] {
            let previous = session.completed_cursorless_end();
            session.resize(width, 40);
            completed_action(&mut session, previous, "[< Previous failed step]");
            ready(
                &mut session,
                &["[> Next failed step]", "[< Previous failed step]"],
            );
            navigate(&mut session, mouse, true, 2);
            navigate(&mut session, mouse, false, 17);
            assert_eq!(commands(&session, &fixture).len(), 59);
        }
        assert!(!session.screen().contains("selective step capture"));
        let previous = session.completed_cursorless_end();
        if mouse {
            session.click_visible_action(step_cell(17))
        } else {
            session.send_key(b"\r")
        }
        session.wait_for_command_count(&fixture, 74);
        completed_action(&mut session, previous, "EXACT_MATCHED_JOB_LOG_CANARY");
        // Sparse reported number17 is NEVER a transport locator: only the
        // already-admitted whole job501 log bracket was executed.
        let journal = commands(&session, &fixture);
        assert_eq!(
            journal.iter().filter(|row| row["args"][0] == "run").count(),
            1
        );
        assert!(journal.iter().all(|row| !row["args"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_str().is_some_and(|s| s.contains("steps/")))));
    } else {
        for width in [40, 80, 120] {
            let previous = session.completed_cursorless_end();
            session.resize(width, 40);
            completed_action(&mut session, previous, "[< Previous failed step]");
            let previous = session.completed_cursorless_end();
            control(&mut session, mouse, "[> Next failed step]", b">");
            overlay_ready(
                &mut session,
                "Action unavailable",
                &[
                    "No positively observed completed/failure step",
                    "Missing/unknown status or number is not failure",
                    "[Esc Close]",
                ],
            );
            assert!(session.completed_cursorless_end() > previous);
            assert!(!session.screen().contains("Observed failed step 0"));
            assert!(!session.screen().contains("Observed failed step 18"));
            let previous = session.completed_cursorless_end();
            close_notice(&mut session, mouse);
            completed_action(&mut session, previous, "[> Next failed step]");
            ready(&mut session, &[step_cell(0), "[> Next failed step]"]);
            assert_eq!(commands(&session, &fixture).len(), 59);
        }
    }
    finish_read(
        &mut session,
        &fixture,
        mouse,
        &expected,
        ReadOnlyEvidence {
            sql: &before,
            git: &git,
            config: &config,
            trace: &trace,
        },
    );
}

#[test]
fn failed_step_v8_keyboard_original_sparse_failure_wrap_resize_whole_job_logs_only() {
    journey(false, true, false)
}
#[test]
fn failed_step_v8_mouse_original_sparse_failure_wrap_resize_whole_job_logs_only() {
    journey(true, true, false)
}
#[test]
fn failed_step_v8_keyboard_nullable_unknown_noncompleted_zero_disabled_explain() {
    journey(false, false, false)
}
#[test]
fn failed_step_v8_mouse_nullable_unknown_noncompleted_zero_disabled_explain() {
    journey(true, false, false)
}
#[test]
fn failed_step_v8_keyboard_original_access_loss_retires_controls_no_lookup() {
    journey(false, true, true)
}
#[test]
fn failed_step_v8_mouse_original_access_loss_retires_controls_no_lookup() {
    journey(true, true, true)
}
