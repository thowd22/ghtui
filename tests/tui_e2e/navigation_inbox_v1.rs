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
    // At 120 columns the list shares the screen with the preview (TASK-92), so the queue
    // summary ends in an ellipsis after the counts that fit.
    session.wait_for_screen("Queue: All • All 3 · Created by me 2 · Assigned to me 2 · Needs my");
    session.wait_for_screen("Created by me 2 · Assigned to me 2 · Needs my re…");
    journal(&mut session, &fixture, 8);
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
    journal(&mut session, &fixture, 13);
    choose(&mut session, mouse, "[b Back / Cancel]", b"b");
    session.wait_for_overlay_close(NOTIFICATIONS);
    session.wait_for_screen("Queue: Needs my review");
    journal(&mut session, &fixture, 13);
    session.quit_and_assert_restored_terminal(mouse);
}

/// TASK-90: the loaded Inbox has no bracketed toolbar rows. Queues are tabs, the rest are
/// key-bar tokens whose click and key run the same action, compacted at 80 columns.
fn footer_journey(mouse: bool) {
    let fixture = configured();
    let mut steps = initial(false);
    steps.extend(notifications_read());
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    refresh(&mut session, mouse);
    session.wait_for_screen("Improve auth");
    journal(&mut session, &fixture, 8);
    for control in [
        "[a All]",
        "[c Created by me]",
        "[m Assigned to me]",
        "[v Needs my review]",
        "[Enter Open]",
        "[r Refresh]",
        "[F8 Notifications]",
        "[x Clear cache]",
        "[? Help]",
        "[: Commands]",
        "[q Quit]",
    ] {
        session.wait_for_control(control);
    }
    let screen = session.screen();
    for retired in ["[x Clear cache]", "[F8 Notifications]", "[c Created by me]"] {
        assert!(
            !screen.contains(retired),
            "{retired} toolbar row remains\n{screen}"
        );
    }
    session.choose_control(mouse, "[c Created by me]");
    session.wait_for_screen("Queue: Created by me • All 3");
    session.resize_owned_footer(
        80,
        24,
        &crate::support::view_title("Inbox"),
        &["[F8 Notifications]", "[: Cmds]"],
    );
    session.wait_for_control("[v Needs my review]");
    session.choose_control(mouse, "[F8 Notifications]");
    session.wait_for_screen(NOTIFICATIONS);
    session.wait_for_screen("No notifications observed in this exact query.");
    journal(&mut session, &fixture, 13);
    choose(&mut session, mouse, "[b Back / Cancel]", b"b");
    session.wait_for_overlay_close(NOTIFICATIONS);
    session.wait_for_screen("Queue: Created by me");
    journal(&mut session, &fixture, 13);
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn design_footer_v90_inbox_keyboard_tabs_tokens_and_compact_resize() {
    footer_journey(false)
}
#[test]
fn design_footer_v90_inbox_mouse_tabs_tokens_and_compact_resize() {
    footer_journey(true)
}

#[test]
fn v1_navigation_inbox_keyboard_prompt_counts_help_and_palette_notifications() {
    journey(false)
}
#[test]
fn v1_navigation_inbox_mouse_prompt_counts_help_and_palette_notifications() {
    journey(true)
}

/// The screen line holding every part, with its row.
fn table_line(session: &Session, parts: &[&str]) -> Option<(u16, String)> {
    session
        .screen()
        .lines()
        .enumerate()
        .find(|(_, line)| parts.iter().all(|part| line.contains(part)))
        .map(|(y, line)| (y as u16, line.to_owned()))
}

fn wait_line(session: &mut Session, parts: &[&str]) -> (u16, String) {
    session
        .try_wait_for_condition(
            |screen| {
                screen
                    .lines()
                    .any(|line| parts.iter().all(|part| line.contains(part)))
            },
            &format!("one line holding {parts:?}"),
            std::time::Duration::from_secs(8),
        )
        .unwrap();
    table_line(session, parts).unwrap()
}

/// Display column of `needle` in `line`.
fn cell_x(line: &str, needle: &str) -> u16 {
    line[..line.find(needle).unwrap()].chars().count() as u16
}

/// TASK-91: the Inbox queue is a table under column titles: one row per PR, the `▌` bar on
/// the selected row, the author as chip + login (no `@`), a right-aligned relative age and
/// the queues. Keyboard Down/Up move the bar; Enter and a click anywhere on the row open that
/// exact PR, whose Commits tab is a table with the short SHA and the full SHA in its details.
fn table_journey(mouse: bool) {
    let fixture = configured();
    let mut steps = initial(false);
    steps.extend(pages().into_iter().take(7));
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 140, 42);
    session.wait_for_screen("ghtui");
    refresh(&mut session, mouse);
    session.wait_for_screen("Improve auth");
    journal(&mut session, &fixture, 8);
    let title = crate::support::view_title("Inbox");
    let (header_y, header) = wait_line(&mut session, &["PR", "TITLE", "AUTHOR", "AGE", "QUEUE"]);
    let (alpha_y, alpha) = wait_line(
        &mut session,
        &["org/alpha#7", "Improve auth", "alice", "Created, Assigned"],
    );
    let (beta_y, beta) = wait_line(
        &mut session,
        &["org/beta#7", "Repair parser", "alice", "Assigned, Review"],
    );
    let (gamma_y, gamma) = wait_line(&mut session, &["org/gamma#9", "Update docs", "Created"]);
    let mut rows = [alpha_y, beta_y, gamma_y];
    rows.sort();
    // Comfortable density (the default, TASK-96): each PR is its row over a muted detail line.
    assert_eq!(
        rows,
        [header_y + 1, header_y + 3, header_y + 5],
        "{}",
        session.screen()
    );
    for row in rows {
        let detail = session
            .screen()
            .lines()
            .nth(usize::from(row) + 1)
            .unwrap_or_default()
            .to_owned();
        assert!(
            detail.contains("Open · updated"),
            "{detail}\n{}",
            session.screen()
        );
    }
    let age_end = cell_x(&header, "AGE") as usize + 3;
    for row in [&alpha, &beta, &gamma] {
        assert_eq!(cell_x(row, "org/"), cell_x(&header, "PR"), "{row}");
        assert!(!row.contains("@alice") && !row.contains("> "), "{row}");
        let cells = row.chars().collect::<Vec<_>>();
        assert_eq!(
            cells[age_end - 1],
            'd',
            "age right-aligned under AGE: {row}"
        );
        assert!(cells.get(age_end).is_none_or(|cell| *cell == ' '), "{row}");
    }
    // The old pre-table two-line item format is gone.
    assert!(!session.screen().contains("review/checks: open PR to load"));
    let first = session.wait_for_selected_row(&title, "Improve auth");
    assert!(first.starts_with("org/alpha#7"), "{first}");
    if mouse {
        // The whole row is the target: a click on its age cell opens it.
        session.click(age_end as u16 - 2, alpha_y);
    } else {
        session.send_key(b"\x1b[B");
        session.wait_for_selected_row(&title, "Repair parser");
        session.send_key(b"\x1b[A");
        session.wait_for_selected_row(&title, "Improve auth");
        session.send_key(b"\r");
    }
    session.wait_for_screen("1 comments");
    journal(&mut session, &fixture, 15);
    choose(&mut session, mouse, "[h Commits]", b"h");
    let (sha_header_y, sha_header) = wait_line(&mut session, &["SHA", "MESSAGE", "AUTHOR", "AGE"]);
    let (commit_y, commit) = wait_line(&mut session, &["aaaaaaa", "Fix auth boundary", "alice"]);
    assert_eq!(commit_y, sha_header_y + 1);
    assert_eq!(cell_x(&commit, "aaaaaaa"), cell_x(&sha_header, "SHA"));
    assert!(
        !commit.contains(&"a".repeat(40)) && !commit.contains("@alice"),
        "{commit}"
    );
    let (sha_y, _) = wait_line(&mut session, &[&format!("Commit {}", "a".repeat(40))]);
    let (parents_y, _) = wait_line(&mut session, &[&format!("Parents: {}", "b".repeat(40))]);
    assert_eq!(parents_y, sha_y + 1);
    session.wait_for_screen("@build-bot [bot]");
    journal(&mut session, &fixture, 15);
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn design_table_v91_inbox_keyboard_table_rows_bar_and_commits_table() {
    table_journey(false)
}
#[test]
fn design_table_v91_inbox_mouse_table_rows_bar_and_commits_table() {
    table_journey(true)
}

/// The preview pane's columns (from its left border) on every screen line, when the screen
/// paints a `Preview` pane beside the list (TASK-92).
fn preview_pane(screen: &str) -> Option<Vec<String>> {
    let x = screen.lines().find_map(|line| {
        let chars = line.chars().collect::<Vec<_>>();
        let x = chars
            .iter()
            .enumerate()
            .skip(1)
            .find(|(_, cell)| **cell == '╭')
            .map(|(x, _)| x)?;
        chars[x..]
            .iter()
            .collect::<String>()
            .contains("Preview")
            .then_some(x)
    })?;
    Some(
        screen
            .lines()
            .map(|line| line.chars().skip(x).collect())
            .collect(),
    )
}

/// Waits until the preview pane holds every part (each on some line) and returns its lines.
fn wait_preview(session: &mut Session, parts: &[&str]) -> Vec<String> {
    session
        .try_wait_for_condition(
            |screen| {
                preview_pane(screen).is_some_and(|lines| {
                    parts
                        .iter()
                        .all(|part| lines.iter().any(|line| line.contains(part)))
                })
            },
            &format!("a preview pane holding {parts:?}"),
            std::time::Duration::from_secs(8),
        )
        .unwrap();
    preview_pane(&session.screen()).unwrap()
}

/// The list pane's top border (left of the preview), which carries the list's focus marker.
fn list_title(session: &Session) -> String {
    session
        .screen()
        .lines()
        .find(|line| line.contains("Inbox • focused view"))
        .map(|line| line.split('╮').next().unwrap_or_default().to_owned())
        .unwrap_or_default()
}

/// TASK-92: at 140 columns the Inbox queue shows the selected PR's cached summary in a
/// preview pane beside the list; moving the cursor re-presents it with no gh command. Tab and
/// a click focus the preview (its `▶` marker moves there, the list's leaves); Shift-Tab
/// returns. Below 120 columns the preview goes and Enter drills down. Enter in the focused
/// preview (keyboard) or its `Enter Open` border token (mouse) opens the same PR.
fn split_journey(mouse: bool) {
    let fixture = configured();
    let mut steps = initial(false);
    steps.extend(pages().into_iter().take(7));
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 140, 42);
    session.wait_for_screen("ghtui");
    refresh(&mut session, mouse);
    session.wait_for_screen("Improve auth");
    journal(&mut session, &fixture, 8);
    let title = crate::support::view_title("Inbox");
    session.wait_for_selected_row(&title, "Improve auth");
    let preview = wait_preview(
        &mut session,
        &[
            "Preview",
            "Enter Open",
            "Improve auth",
            "org/alpha#7",
            "OPEN",
            "AL alice",
            "2026-10-01T00:00:00Z",
            "2026-10-12T14:00:00Z",
            "https://github.invalid/org/alpha/pull/7",
            "DESCRIPTION",
            "A useful change",
            "Second paragraph",
        ],
    );
    assert!(!preview.iter().any(|line| line.contains("▶ Preview")));
    assert!(
        list_title(&session).contains("▶ Inbox"),
        "{}",
        session.screen()
    );
    let top = session
        .screen()
        .lines()
        .position(|line| line.contains("╭─ Preview") || line.contains("─ Preview"))
        .unwrap() as u16;
    let preview_x = session
        .screen()
        .lines()
        .nth(usize::from(top))
        .unwrap()
        .chars()
        .skip(1)
        .position(|cell| cell == '╭')
        .unwrap() as u16
        + 1;
    assert_eq!(preview_x, 84, "{}", session.screen());
    if mouse {
        // A click on the preview body focuses that pane and reads nothing.
        session.click(preview_x + 12, top + 6);
        wait_preview(&mut session, &["▶ Preview", "Improve auth"]);
        assert!(!list_title(&session).contains('▶'), "{}", session.screen());
        // The wheel over the preview belongs to the preview: the list cursor stays, and a
        // summary that fits does not scroll away.
        session.wheel(preview_x + 12, top + 6, true);
        session.wait_for_selected_row(&title, "Improve auth");
        wait_preview(
            &mut session,
            &["▶ Preview", "Improve auth", "Second paragraph"],
        );
    } else {
        session.send_key(b"\x1b[B");
        session.wait_for_selected_row(&title, "Repair parser");
        let beta = wait_preview(&mut session, &["Repair parser", "org/beta#7"]);
        assert!(!beta.iter().any(|line| line.contains("Improve auth")));
        session.send_key(b"\x1b[A");
        session.wait_for_selected_row(&title, "Improve auth");
        wait_preview(&mut session, &["Improve auth", "org/alpha#7"]);
        session.send_key(b"\t");
        wait_preview(&mut session, &["▶ Preview", "Improve auth"]);
        assert!(!list_title(&session).contains('▶'), "{}", session.screen());
        session.send_key(b"\x1b[Z");
        session
            .try_wait_for_condition(
                |screen| screen.contains("▶ Inbox") && !screen.contains("▶ Preview"),
                "the list pane focused again",
                std::time::Duration::from_secs(8),
            )
            .unwrap();
    }
    // Presenting cached summaries ran no gh command.
    journal(&mut session, &fixture, 8);

    // Below 120 columns the list keeps the whole pane: drill-down instead of a preview.
    session.resize(80, 24);
    session
        .try_wait_for_condition(
            |screen| preview_pane(screen).is_none() && screen.contains("org/alpha#7"),
            "the preview gone at 80x24",
            std::time::Duration::from_secs(8),
        )
        .unwrap();
    assert!(!session.screen().contains("A useful change"));
    session.wait_for_selected_row(&title, "Improve auth");
    session.resize(140, 42);
    wait_preview(&mut session, &["Improve auth", "Enter Open"]);
    if mouse {
        let (x, y) = session.locate("Enter Open").expect("the Open token");
        session.click(x + 1, y);
    } else {
        session.send_key(b"\t");
        wait_preview(&mut session, &["▶ Preview"]);
        session.send_key(b"\r");
    }
    session.wait_for_screen("1 comments");
    journal(&mut session, &fixture, 15);
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn design_split_v92_inbox_keyboard_preview_beside_list_and_drill_down_at_80() {
    split_journey(false)
}
#[test]
fn design_split_v92_inbox_mouse_preview_beside_list_and_drill_down_at_80() {
    split_journey(true)
}
