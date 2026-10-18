//! Real-binary Inbox navigation journey: discoverable empty state, F8 before
//! load, per-queue counts, accurate Help and running Notifications from the
//! command palette. Strict offline fake gh; keyboard and mouse variants.
use super::*;

const PALETTE: &str = "Commands · current context";
const NOTIFICATIONS: &str = "Notifications • observed";

fn notifications_read() -> Vec<GhStep> {
    let mut steps = identity("octocat");
    let mut page = api("notifications?all=false&participating=false&per_page=50&page=1");
    page.push("--include".into());
    steps.push(GhStep {
        args: page,
        stdin: vec![],
        stdout: b"HTTP/2 200 OK\nETag: \"snap\"\r\nX-Poll-Interval: 60\r\n\r\n[]".to_vec(),
        stderr: vec![],
        status: 0,
        delay_ms: 0,
    });
    steps.extend(identity("octocat"));
    steps
}

fn open_palette(session: &mut Session, mouse: bool) {
    choose(session, mouse, "[: Commands]", b":");
    session.wait_for_screen(PALETTE);
}

fn journey(mouse: bool) {
    let fixture = configured();
    let mut steps = initial(false);
    steps.extend(notifications_read());
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    // One clear prompt before anything is loaded.
    session.wait_for_screen("Press r (or click Refresh below) to load your PRs");

    // Notifications before the Inbox loads explain the prerequisite instead
    // of silently doing nothing (there is no clickable F8 control yet).
    session.send_key(b"\x1b[19~");
    session.wait_for_screen("Refresh the Inbox (r) first, then F8 opens notifications.");
    if !mouse {
        open_palette(&mut session, mouse);
        session.paste("notif");
        session.wait_for_screen("> Open notifications inbox [why unavailable]");
        session.send_key(b"\r");
        session.wait_for_screen("Action unavailable • explanation only");
        session.wait_for_screen("Notifications open from the loaded");
        session.send_key(b"\x1b");
        session.wait_for_overlay_close("Action unavailable • explanation only");
    }
    session.wait_for_command_count(&fixture, 0);

    refresh(&mut session, mouse);
    session.wait_for_screen("Improve auth");
    session.wait_for_screen(
        "Queue: All • All 3 · Created by me 2 · Assigned to me 2 · Needs my review 1",
    );
    journal(&mut session, &fixture, 7);
    choose(&mut session, mouse, "[v Needs my review]", b"v");
    session.wait_for_screen("Queue: Needs my review • All 3");
    session.wait_for_screen("Repair parser");

    // Help documents the Inbox keys and F8 with word-wrapped rows.
    choose(&mut session, mouse, "[? Help]", b"?");
    session.wait_for_screen("Inbox controls: Refresh loads your PRs.");
    session.wait_for_screen("F8 Notifications opens unread GitHub notifications");
    if mouse {
        session.close_overlay_by_mouse("Help • keyboard and mouse");
    } else {
        session.send_key(b"\x1b");
        session.wait_for_overlay_close("Help • keyboard and mouse");
    }

    // The palette lists runnable Inbox commands first, with their keys, and
    // runs Notifications through the same action as F8.
    open_palette(&mut session, mouse);
    session.wait_for_screen("Show Needs my review");
    let screen = session.screen();
    let filter = screen
        .lines()
        .find(|line| line.contains("Show Created by me"))
        .expect("queue command listed");
    assert!(
        filter.trim_end_matches(['│', ' ']).ends_with(" c"),
        "{filter}"
    );
    assert!(!screen.contains("Show All [why unavailable]"));
    session.paste("notif");
    session.wait_for_screen("> Open notifications inbox");
    let row = session
        .screen()
        .lines()
        .find(|line| line.contains("> Open notifications inbox"))
        .unwrap()
        .to_string();
    assert!(!row.contains("why unavailable"), "{row}");
    assert!(row.contains("F8"), "{row}");
    if mouse {
        session.click_visible_action("> Open notifications inbox");
    } else {
        session.send_key(b"\r");
    }
    session.wait_for_screen(NOTIFICATIONS);
    session.wait_for_screen("No notifications observed in this exact query.");
    journal(&mut session, &fixture, 12);
    choose(&mut session, mouse, "[b Back / Cancel]", b"b");
    session.wait_for_overlay_close(NOTIFICATIONS);
    session.wait_for_screen("Queue: Needs my review");
    journal(&mut session, &fixture, 12);
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn v1_navigation_inbox_keyboard_prompt_counts_help_and_palette_notifications() {
    journey(false)
}
#[test]
fn v1_navigation_inbox_mouse_prompt_counts_help_and_palette_notifications() {
    journey(true)
}
