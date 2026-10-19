//! TASK-90 key bar on the actual executable: the Dashboard's controls and the Global tail are
//! one bottom row of `key label` tokens, clickable and bound to the same keys, compacted at
//! 80x24, with disabled controls leaving the bar while their keys explain why.
use super::*;

const PANE: &str = "Dashboard • focused view";
const WIDE: [&str; 13] = [
    "[r Refresh]",
    "[i Details]",
    "[b Back]",
    "[7 7 days]",
    "[0 30 days]",
    "[R Repositories]",
    "[L Layout]",
    "[S Save]",
    "[O Open saved]",
    "[D Defaults]",
    "[? Help]",
    "[: Commands]",
    "[q Quit]",
];
const NARROW: &[&str] = &[
    "[r Refresh]",
    "[i Details]",
    "[b Back]",
    "[7 7 days]",
    "[0 30 days]",
    "[? Help]",
    "[: Cmds]",
    "[q Quit]",
];

fn last_row(session: &Session) -> String {
    session
        .screen()
        .lines()
        .last()
        .unwrap_or_default()
        .to_owned()
}

#[test]
fn design_footer_v90_keyboard_dashboard_key_bar_and_global_tail() {
    key_bar(false);
}
#[test]
fn design_footer_v90_mouse_dashboard_key_bar_and_global_tail() {
    key_bar(true);
}
fn key_bar(mouse: bool) {
    let fixture = configured(false);
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let mut steps = refresh(&[APP]);
    steps.extend(window(&[APP]));
    let mut slow = refresh(&[APP]);
    slow[2].delay_ms = 2500;
    steps.extend(slow);
    assert_eq!(steps.len(), 20);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 140, 42);
    start(&mut session, mouse);
    journal(&mut session, &fixture, 7);
    cards_ready(&mut session, 1, 1);
    // 140 columns: every Dashboard control and the Global tail fit in the actual last row.
    for label in WIDE {
        session.wait_for_control(label);
    }
    let footer = last_row(&session);
    assert!(footer.starts_with(" r Refresh  i Details  b Back  7 7 days"));
    assert!(footer.trim_end().ends_with("? Help  : Commands  q Quit"));
    assert!(
        !session.screen().contains("[r Refresh]"),
        "no bracketed toolbar remains"
    );
    assert!(
        !session.screen().contains("Tab buttons"),
        "the static hint is gone"
    );
    // Same window action by click or key, with the same read effect.
    let clock = as_of(&mut session);
    session.choose_control(mouse, "[0 30 days]");
    journal(&mut session, &fixture, 13);
    assert_window(&mut session, 30, clock);
    // While a refresh runs every view control leaves the bar for Cancel; a disabled key
    // explains itself instead of doing nothing.
    session.choose_control(mouse, "[r Refresh]");
    session.wait_for_control("[Esc Cancel]");
    session.wait_for_command_count(&fixture, 10);
    for label in ["[r Refresh]", "[L Layout]", "[D Defaults]"] {
        session.assert_control_absent(label);
    }
    session.press_disabled_key(b"L", "Dashboard is preparing");
    session.choose(mouse, "[Esc Close]", b"\x1b");
    session.wait_for_overlay_close("Action unavailable");
    journal(&mut session, &fixture, 20);
    cards_ready(&mut session, 1, 1);
    session.wait_for_control("[r Refresh]");
    session.assert_control_absent("[Esc Cancel]");
    // 80x24: compact labels, lower-priority controls move to Commands and Help.
    session.resize_owned_footer(80, 24, PANE, NARROW);
    for label in ["[D Defaults]", "[O Open saved]", "[S Save]", "[L Layout]"] {
        session.assert_control_absent(label);
    }
    // `: Cmds` is the compact Commands token, so the locator still finds Commands; the full
    // label is gone from the bar.
    assert!(!last_row(&session).contains(": Commands"));
    assert!(last_row(&session)
        .trim_end()
        .ends_with("? Help  : Cmds  q Quit"));
    // A dropped control still answers its key.
    session.send_key(b"L");
    session.wait_for_screen("Layout only");
    session.wait_for_control("[Space Show/hide]");
    session.choose_control(mouse, "[b Back]");
    session.wait_for_control("[i Details]");
    // Global tail: Help and Commands open by click or key and close back to the key bar.
    session.choose_control(mouse, "[? Help]");
    session.wait_for_screen("Help • keyboard and mouse");
    session.choose(mouse, "[Esc Close]", b"\x1b");
    session.wait_for_overlay_close("Help • keyboard and mouse");
    session.wait_for_main_frame();
    session.choose_control(mouse, "[: Cmds]");
    session.wait_for_screen("Commands · current context");
    session.send_key(b"\x1b");
    session.wait_for_overlay_close("Commands · current context");
    session.wait_for_main_frame();
    journal(&mut session, &fixture, 20);
    metadata_only(&fixture);
    preserve(&fixture, &head, &index, &status);
    session.quit_and_assert_restored_terminal(mouse);
    journal(&mut session, &fixture, 20);
}
