//! ONE real input from the noninteractive original Checks header must select
//! genuine Check600, not metadata/Status700. No movement loop or timing retry.
use super::*;
fn first_input(mouse: bool) {
    let expected = steps(vec![]);
    assert_eq!(expected.len(), 59);
    let fixture = resource_configured(&expected);
    let git_before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    selected(&mut session, &fixture, mouse);
    choose(&mut session, mouse, "[k Checks]", b"k");
    session.wait_for_command_count(&fixture, 41);
    session.wait_for_cursorless_condition(|frame|{let text=frame.contents();text.contains("Required rules unavailable")&&text.contains("Check#600")&&text.contains("Status#700")&&crate::support::controls::control_painted(frame, "[Enter Open check]")},"Actual completed original Checks notice+genuine Check600+unrelated Status700 before first input",Duration::from_secs(8));
    let original = crate::support::completed_navigation_controls_v1::progress_selection(
        session.completed_cursorless_screen().unwrap(),
        "Exact observed build",
    )
    .unwrap();
    assert_eq!(
        original.selected_row, None,
        "Header is not a selectable check"
    );
    let sql_before = sql_state(&fixture);
    let trace = git_commands(&fixture);
    let before = session.completed_cursorless_end();
    if mouse {
        session.click(original.target.0, original.target.1);
    } else {
        session.send_key(b"\x1b[B");
    }
    session.wait_for_new_cursorless_condition(before,|frame|crate::support::completed_navigation_controls_v1::progress_selection(frame,"Exact observed build").is_some_and(|selection|selection.selected_row==Some(selection.target.1)),"Exactly ONE Down/click must visibly select genuine original Check600, not a notice or Status700",Duration::from_millis(400));
    assert_eq!(
        commands(&session, &fixture).len(),
        41,
        "Selection itself performs zero API work"
    );
    if mouse {
        session.click_visible_action("[Enter Open check]");
    } else {
        session.send_key(b"\r");
    }
    session.wait_for_command_count(&fixture, 59);
    ready(
        &mut session,
        &["Run 123 • attempt 2", "Job 501", &"a".repeat(40), "alice"],
    );
    assert!(!session.screen().contains(DETAIL_BODY));
    assert!(!session.screen().contains(PRIVATE_DRAFT));
    control(&mut session, mouse, "[b Back to PR checks]", b"b");
    ready(&mut session, &["Exact observed build", "Status#700"]);
    assert_eq!(commands(&session, &fixture).len(), 59);
    unchanged(&session, &fixture, &sql_before, &expected);
    assert_eq!(git_state(&fixture), git_before);
    assert_eq!(git_commands(&fixture), trace);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    exact_reads(&session, &fixture, &expected);
    no_attempts(&fixture);
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn completed_frame_actions_scoped_checks_v1_keyboard_first_down_genuine600_then_exact_enter59() {
    first_input(false)
}
#[test]
fn completed_frame_actions_scoped_checks_v1_mouse_first_click_genuine600_then_exact_enter59() {
    first_input(true)
}
