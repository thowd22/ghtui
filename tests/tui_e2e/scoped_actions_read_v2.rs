//! SOURCE ONLY / UNRUN. These are actual installed-binary PTY journeys, not
//! direct handlers or accepted snapshots. Root verifies nonzero selections.
use super::*;
const TEXT: &str = "first actual\nerror 界 é\nthird actual\nerror second\nlast\n";
fn log_steps(uid: u64) -> Vec<GhStep> {
    let mut steps = operation(ReadKind::Logs, uid, CURRENT);
    steps
        .iter_mut()
        .find(|step| step.args.first().is_some_and(|a| a == "run"))
        .unwrap()
        .stdout = TEXT.as_bytes().to_vec();
    steps
}
fn ctl(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    resources_v2::action(session, mouse, label, key);
}
fn modal(session: &mut Session, title: &str) -> String {
    session.wait_for_screen(title);
    session.wait_for_overlay_body(title)
}
fn field(session: &mut Session, mouse: bool, label: &str, value: &str, first: bool) {
    if mouse {
        let (x, y) = session
            .locate(label)
            .expect("actual owned editor field label");
        session.click(x + label.chars().count() as u16 + 1, y);
    } else if !first {
        session.send_key(b"\t");
    }
    session.paste(value);
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        if session
            .screen()
            .lines()
            .any(|row| row.contains(label) && row.contains(value))
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "actual owned field value acknowledgement {label}/{value}\n{}",
            session.screen()
        );
    }
}
fn apply(session: &mut Session, mouse: bool, search: bool) {
    if mouse {
        session.click_visible_action(if search {
            "[Search captured logs]"
        } else {
            "[Apply filters]"
        });
    } else {
        session.send_key(b"\r");
    }
}
fn capture_logs(session: &mut Session, fixture: &Fixture, mouse: bool) {
    open_run(session, fixture, mouse, 24, CURRENT);
    jobs(session, mouse);
    let after = session.completed_cursorless_end();
    ctl(session, mouse, "[l Logs]", b"l");
    session.wait_for_command_count(fixture, 37);
    complete(session, after, &["error 界 é", "error second"]);
}
fn search(session: &mut Session, mouse: bool, query: &str, regex: bool) {
    ctl(session, mouse, "[/ Search loaded logs]", b"/");
    modal(session, "Search actual captured Actions logs");
    // Search reopens the retained bounded query; replace it explicitly.
    if mouse {
        let (x, y) = session.locate("Query:").unwrap();
        session.click(x + 7, y);
    }
    session.send_key(b"\x01");
    session.paste(query);
    let expected = if regex { "Regex" } else { "Exact text" };
    if mouse {
        session.click_visible_action(&format!("[{expected}]"));
    } else {
        session.send_key(if regex { b"\t\t\r" } else { b"\t\r" });
    }
    session.wait_for_screen(if regex {
        "Mode: Regex"
    } else {
        "Mode: ExactText"
    });
    if mouse {
        apply(session, true, true);
    } else {
        // Focus remains the mode control; move to the actual Apply control.
        session.send_key(if regex { b"\t\r" } else { b"\t\t\r" });
    }
}
fn selected_log(session: &Session, label: &str) {
    let screen = session
        .completed_cursorless_screen()
        .expect("actual complete log selection")
        .contents();
    assert!(
        screen
            .lines()
            .any(|row| row.contains("> ") && row.contains(label)),
        "actual selected canonical captured-log row {label}\n{screen}"
    );
}
fn filters(mouse: bool) {
    let endpoint=format!("repos/{CURRENT}/actions/workflows/7/runs?per_page=100&actor=alice&branch=main&event=push&status=failure&head_sha={H}&created=2026-10-15&page=");
    let page = |number: u32| {
        resources_v2::resource(
            vec![get(
                &format!("{endpoint}{number}"),
                json!({"total_count":10000,"workflow_runs":[fresh_run(CURRENT)]}),
            )],
            42,
        )
    };
    let mut steps = actions_protocol();
    steps.extend(page(1));
    steps.extend(page(1));
    steps.extend(page(2));
    steps.extend(operation(ReadKind::Run, 42, CURRENT));
    let fixture = configured_native(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    let sql = sql(&fixture);
    session.resize(120, 40);
    session.wait_for_main_frame();
    ctl(&mut session, mouse, "[f Edit filters]", b"f");
    let body = modal(&mut session, "Original repository run filters");
    assert!(body.contains("Apply filters"));
    for (i, (label, value)) in [
        ("Actor:", "alice"),
        ("Branch:", "main"),
        ("Event:", "push"),
        ("Status:", "failure"),
        ("Workflow ID:", "7"),
        ("Head SHA:", H),
        ("Created:", "2026-10-15"),
    ]
    .iter()
    .enumerate()
    {
        field(&mut session, mouse, label, value, i == 0);
    }
    let after = session.completed_cursorless_end();
    apply(&mut session, mouse, false);
    session.wait_for_command_count(&fixture, 21);
    complete(
        &mut session,
        after,
        &["Run page 1", "/10000 reported", "status failure", "run 123"],
    );
    let after = session.completed_cursorless_end();
    ctl(&mut session, mouse, "[r Refresh]", b"r");
    session.wait_for_command_count(&fixture, 28);
    let refresh_started = after;
    complete(
        &mut session,
        after,
        &[
            "Run page 1",
            "status failure",
            "Original scoped Actions target verified",
        ],
    );
    let after = session.completed_cursorless_end();
    if mouse {
        session.click_ready_scoped_actions_control_v4(refresh_started, "[) Next runs]");
    } else {
        ctl(&mut session, false, "[) Next runs]", b")");
    }
    session.wait_for_command_count(&fixture, 35);
    complete(
        &mut session,
        after,
        &["Run page 2", "partial is not absence"],
    );
    open_run(&mut session, &fixture, mouse, 45, CURRENT);
    end(&mut session, &fixture, mouse, &steps, &git, &config, &sql);
}
fn match_navigation(mouse: bool) {
    let mut steps = actions_protocol();
    steps.extend(operation(ReadKind::Run, 42, CURRENT));
    steps.extend(log_steps(42));
    let fixture = configured_native(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    let sql = sql(&fixture);
    session.resize(120, 40);
    session.wait_for_main_frame();
    capture_logs(&mut session, &fixture, mouse);
    // Invalid editor kind is a real global shortcut, not a fabricated mouse
    // control. It explains the actual page limitation with no GH/SQL/Git read.
    let after = session.completed_cursorless_end();
    ctl(&mut session, mouse, "[w Finite Watch / Stop]", b"w");
    complete(&mut session, after, &["Finite Watch armed", "error 界 é"]);
    session.send_key(b"f");
    let body = modal(&mut session, "Action unavailable");
    assert!(body.contains("Open an actual repository Runs page"));
    assert!(!body.contains("Apply filters"));
    let after = session.completed_cursorless_end();
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_new_cursorless_condition(
        after,
        |screen| {
            let text = screen.contents();
            !text.contains("Action unavailable") && text.contains("error 界 é")
        },
        "unavailable editor closed to same actual job-log snapshot",
        Duration::from_secs(8),
    );
    assert_eq!(commands(&session, &fixture).len(), 37);
    // Explicitly rearm, then use the existing key/click Search entry. Its
    // valid editor must stop THIS Watch rather than renew its source/clock.
    let after = session.completed_cursorless_end();
    ctl(&mut session, mouse, "[w Finite Watch / Stop]", b"w");
    complete(&mut session, after, &["Finite Watch armed", "error 界 é"]);
    let after = session.completed_cursorless_end();
    search(&mut session, mouse, "error", false);
    complete(&mut session, after, &["2 matching lines", "error 界 é"]);
    let after = session.completed_cursorless_end();
    ctl(&mut session, mouse, "[> Next match]", b">");
    complete(&mut session, after, &["matching line 2"]);
    selected_log(&session, "error 界 é");
    let after = session.completed_cursorless_end();
    ctl(&mut session, mouse, "[< Previous match]", b"<");
    complete(&mut session, after, &["matching line 4"]);
    selected_log(&session, "error second");
    let after = session.completed_cursorless_end();
    search(&mut session, mouse, "界|second", true);
    complete(&mut session, after, &["2 matching lines"]);
    selected_log(&session, "error second");
    let after = session.completed_cursorless_end();
    ctl(&mut session, mouse, "[x Clear matches]", b"x");
    complete(&mut session, after, &["matches cleared", "error second"]);
    selected_log(&session, "error second");
    assert_eq!(commands(&session, &fixture).len(), 37);
    assert!(!session.screen().contains("Inline comment"));
    end(&mut session, &fixture, mouse, &steps, &git, &config, &sql);
}
fn invalid_query(mouse: bool) {
    let mut steps = actions_protocol();
    steps.extend(operation(ReadKind::Run, 42, CURRENT));
    steps.extend(log_steps(42));
    let fixture = configured_native(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    let sql = sql(&fixture);
    session.resize(120, 40);
    session.wait_for_main_frame();
    capture_logs(&mut session, &fixture, mouse);
    let after = session.completed_cursorless_end();
    search(&mut session, mouse, "error", false);
    complete(&mut session, after, &["2 matching lines"]);
    let after = session.completed_cursorless_end();
    search(&mut session, mouse, "[", true);
    complete(
        &mut session,
        after,
        &[
            "query unavailable",
            "previous matches unchanged",
            "error second",
        ],
    );
    let after = session.completed_cursorless_end();
    ctl(&mut session, mouse, "[> Next match]", b">");
    complete(&mut session, after, &["matching line 2"]);
    selected_log(&session, "error 界 é");
    let after = session.completed_cursorless_end();
    search(&mut session, mouse, "NOT_PRESENT_IN_ACTUAL_CAPTURE", false);
    complete(&mut session, after, &["0 matching lines", "error 界 é"]);
    selected_log(&session, "error 界 é");
    end(&mut session, &fixture, mouse, &steps, &git, &config, &sql);
}
fn tiny_editor(mouse: bool) {
    let steps = actions_protocol();
    let fixture = configured_native(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    let sql = sql(&fixture);
    ctl(&mut session, mouse, "[f Edit filters]", b"f");
    modal(&mut session, "Original repository run filters");
    session.resize_owned_single_action_modal_v2(
        40,
        10,
        "Resize for original Actions editor",
        "[Ctrl-C Quit]",
    );
    let body = modal(&mut session, "Resize for original Actions editor");
    assert!(!body.contains("Apply filters"));
    assert!(body.contains("Close editor"));
    // The old Apply geometry is not reused after a real Resize.
    if mouse {
        session.click(5, 12);
    } else {
        session.send_key(b"\r");
    }
    assert_eq!(commands(&session, &fixture).len(), 14);
    session.resize_owned_vertical_controls_v4(
        120,
        40,
        "Original repository run filters",
        12,
        &["[Apply filters]", "[Esc Close editor]", "[Ctrl-C Quit]"],
    );
    modal(&mut session, "Original repository run filters");
    session.capture_completed_terminal_v1("actions-filters");
    session.paste(&"X".repeat(1025));
    if mouse {
        session.click_visible_action("[Esc Close editor]");
    } else {
        session.send_key(b"\x1b");
    }
    session.wait_for_overlay_close("Original repository run filters");
    session.wait_for_main_frame();
    assert_eq!(commands(&session, &fixture).len(), 14);
    end(&mut session, &fixture, mouse, &steps, &git, &config, &sql);
}
fn watch_ticks(mouse: bool) {
    let mut steps = actions_protocol();
    steps.extend(operation(ReadKind::Run, 42, CURRENT));
    steps.extend(log_steps(42));
    steps.extend(log_steps(42));
    steps.extend(log_steps(42));
    let fixture = configured_native(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    let sql = sql(&fixture);
    session.resize(120, 40);
    session.wait_for_main_frame();
    capture_logs(&mut session, &fixture, mouse);
    let began = Instant::now();
    ctl(&mut session, mouse, "[w Finite Watch / Stop]", b"w");
    session.wait_for_screen("Finite Watch armed");
    session.wait_for_command_count(&fixture, 50);
    session.wait_for_cursorless_condition(
        |s| s.contents().contains("tick 1/12"),
        "actual first finite Watch group",
        Duration::from_secs(8),
    );
    assert!(began.elapsed() >= Duration::from_millis(4500));
    session.wait_for_command_count(&fixture, 63);
    session.wait_for_cursorless_condition(
        |s| s.contents().contains("tick 2/12"),
        "same original finite Watch second group",
        Duration::from_secs(8),
    );
    let after = session.completed_cursorless_end();
    ctl(&mut session, mouse, "[w Finite Watch / Stop]", b"w");
    complete(&mut session, after, &["Finite Watch stopped", "error 界 é"]);
    end(&mut session, &fixture, mouse, &steps, &git, &config, &sql);
}
fn watch_cancel(mouse: bool) {
    let mut held = log_steps(42);
    held[7].delay_ms = 60_000;
    held.truncate(8);
    let mut steps = actions_protocol();
    steps.extend(operation(ReadKind::Run, 42, CURRENT));
    steps.extend(log_steps(42));
    steps.extend(held);
    steps.extend(log_steps(42));
    let fixture = configured_native(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    let sql = sql(&fixture);
    session.resize(120, 40);
    session.wait_for_main_frame();
    capture_logs(&mut session, &fixture, mouse);
    ctl(&mut session, mouse, "[w Finite Watch / Stop]", b"w");
    let child = entered(&mut session, &fixture, 45);
    session.wait_for_cursorless_condition(
        |s| s.contents().contains("Cancel read"),
        "held original finite Watch Cancel",
        Duration::from_secs(8),
    );
    let after = session.completed_cursorless_end();
    ctl(&mut session, mouse, "[b Cancel read]", b"b");
    complete(&mut session, after, &["Finite Watch stopped", "error 界 é"]);
    gone(&mut session, &child);
    exact_native(&session, &fixture, &steps, 45);
    let began = Instant::now();
    ctl(&mut session, mouse, "[w Finite Watch / Stop]", b"w");
    session.wait_for_command_count(&fixture, 58);
    session.wait_for_cursorless_condition(
        |s| s.contents().contains("tick 1/12"),
        "explicitly rearmed original finite quota",
        Duration::from_secs(8),
    );
    assert!(began.elapsed() >= Duration::from_millis(4500));
    ctl(&mut session, mouse, "[w Finite Watch / Stop]", b"w");
    session.wait_for_screen("Finite Watch stopped");
    end(&mut session, &fixture, mouse, &steps, &git, &config, &sql);
}
fn watch_uid(mouse: bool) {
    let mut steps = actions_protocol();
    steps.extend(operation(ReadKind::Run, 42, CURRENT));
    steps.extend(log_steps(42));
    steps.extend(log_steps(43));
    let fixture = configured_native(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    let sql = sql(&fixture);
    session.resize(120, 40);
    session.wait_for_main_frame();
    capture_logs(&mut session, &fixture, mouse);
    let after = session.completed_cursorless_end();
    ctl(&mut session, mouse, "[w Finite Watch / Stop]", b"w");
    session.wait_for_command_count(&fixture, 50);
    complete(
        &mut session,
        after,
        &["Original Actions read unavailable", "private source hidden"],
    );
    assert!(!session.screen().contains("error second"));
    end(&mut session, &fixture, mouse, &steps, &git, &config, &sql);
}
fn watch_drift(mouse: bool) {
    for changed in ["run_attempt", "head_sha"] {
        let mut tick = log_steps(42);
        let mut run = fresh_run(CURRENT);
        run[changed] = if changed == "run_attempt" {
            json!(3)
        } else {
            json!("b".repeat(40))
        };
        tick[3].stdout = run.to_string().into_bytes();
        tick = tick[..4].iter().chain(tick[10..].iter()).cloned().collect();
        let mut steps = actions_protocol();
        steps.extend(operation(ReadKind::Run, 42, CURRENT));
        steps.extend(log_steps(42));
        steps.extend(tick);
        let fixture = configured_native(&steps);
        let git = git_state(&fixture);
        let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
        let mut session = start(&fixture, mouse);
        let sql = sql(&fixture);
        session.resize(120, 40);
        session.wait_for_main_frame();
        capture_logs(&mut session, &fixture, mouse);
        let after = session.completed_cursorless_end();
        ctl(&mut session, mouse, "[w Finite Watch / Stop]", b"w");
        session.wait_for_command_count(&fixture, 44);
        complete(
            &mut session,
            after,
            &["Original Actions read unavailable", "private source hidden"],
        );
        assert!(!session.screen().contains("error second"));
        assert!(!session.screen().contains("attempt 3"));
        end(&mut session, &fixture, mouse, &steps, &git, &config, &sql);
    }
}
#[test]
fn scoped_actions_read_v2_keyboard_filters_all_fields_refresh_partial_page_and_open() {
    filters(false);
}
#[test]
fn scoped_actions_read_v2_mouse_filters_all_fields_refresh_partial_page_and_open() {
    filters(true);
}
#[test]
fn scoped_actions_read_v2_keyboard_shared_log_search_regex_matches_and_clear() {
    match_navigation(false);
}
#[test]
fn scoped_actions_read_v2_mouse_shared_log_search_regex_matches_and_clear() {
    match_navigation(true);
}
#[test]
fn scoped_actions_read_v2_keyboard_invalid_regex_and_zero_match_keep_real_snapshot() {
    invalid_query(false);
}
#[test]
fn scoped_actions_read_v2_mouse_invalid_regex_and_zero_match_keep_real_snapshot() {
    invalid_query(true);
}
#[test]
fn scoped_actions_read_v2_keyboard_editor_tiny_resize_old_hit_and_paste_bounds() {
    tiny_editor(false);
}
#[test]
fn scoped_actions_read_v2_mouse_editor_tiny_resize_old_hit_and_paste_bounds() {
    tiny_editor(true);
}
#[test]
fn scoped_actions_read_v2_keyboard_finite_watch_two_real_ticks_and_stop() {
    watch_ticks(false);
}
#[test]
fn scoped_actions_read_v2_mouse_finite_watch_two_real_ticks_and_stop() {
    watch_ticks(true);
}
#[test]
fn scoped_actions_read_v2_keyboard_watch_held_child_cancel_gone_explicit_rearm() {
    watch_cancel(false);
}
#[test]
fn scoped_actions_read_v2_mouse_watch_held_child_cancel_gone_explicit_rearm() {
    watch_cancel(true);
}
#[test]
fn scoped_actions_read_v2_keyboard_watch_changed_uid_hides_private_capture() {
    watch_uid(false);
}
#[test]
fn scoped_actions_read_v2_mouse_watch_changed_uid_hides_private_capture() {
    watch_uid(true);
}
#[test]
fn scoped_actions_read_v2_keyboard_watch_never_adopts_new_attempt_or_head() {
    watch_drift(false);
}
#[test]
fn scoped_actions_read_v2_mouse_watch_never_adopts_new_attempt_or_head() {
    watch_drift(true);
}
