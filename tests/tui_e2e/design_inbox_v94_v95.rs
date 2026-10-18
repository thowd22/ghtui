//! TASK-94 and TASK-95 on a real Inbox: freshness sits on a dimmed section header while the
//! PARTIAL coverage label stays visible, and list rows carry two-cell initials chips whose hue is
//! stable per identity. Real cells from the compiled binary at 140x42 and 80x24.
use crate::support::{empty_inbox_steps, Fixture, GhStep, Session};
use ghtui::identity::{ActorIdentity, ActorKind, InitialsChip};
use ghtui::theme::CATPPUCCIN_MOCHA;
use serde_json::{json, Value};
use std::time::{Duration, Instant};
use vt100::Color;

const MAUVE: Color = Color::Rgb(0xcb, 0xa6, 0xf7);
const MUTED: Color = Color::Rgb(0xa6, 0xad, 0xc8);

fn rgb(hex: u32) -> Color {
    Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

fn raw_pr(number: u64, title: &str, login: &str) -> Value {
    json!({"node_id":format!("PR_DESIGN_{number}"),"id":700+number,"number":number,"title":title,"body":"Design fixture","repository_url":"https://api.github.com/repos/org/design","html_url":format!("https://github.com/org/design/pull/{number}"),"user":{"login":login,"type":"User","avatar_url":null},"state":"open","draft":false,"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-13T00:00:00Z"})
}

fn protocol() -> Vec<GhStep> {
    let mut steps = empty_inbox_steps("octocat");
    for (index, step) in steps.iter_mut().skip(4).enumerate() {
        // The first queue reports incomplete results, so coverage is PARTIAL.
        step.stdout = serde_json::to_vec(&json!({
            "total_count": 3,
            "incomplete_results": index == 0,
            "items": [
                raw_pr(1, "Chip first octocat row", "octocat"),
                raw_pr(2, "Chip hubot row", "hubot"),
                raw_pr(3, "Chip second octocat row", "octocat"),
            ]
        }))
        .unwrap();
    }
    steps
}

fn person(login: &str) -> ActorIdentity {
    ActorIdentity {
        host: Some("github.com".into()),
        login: Some(login.into()),
        name: String::new(),
        email: None,
        avatar_url: None,
        kind: ActorKind::User,
        git_role: None,
        host_associated: true,
    }
}

fn wait_cells(
    session: &mut Session,
    after: usize,
    label: &str,
    predicate: impl Fn(&vt100::Screen) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        if session.completed_cells_end() > after
            && session.completed_cells_screen().is_some_and(&predicate)
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "No completed frame: {label}\n{}",
            session.screen()
        );
    }
}

/// The section header row: `PULL REQUESTS` in the Inbox hue, freshness right-aligned and muted.
fn section_header(frame: &vt100::Screen) -> bool {
    let contents = frame.contents();
    let Some((row, line)) = contents
        .lines()
        .enumerate()
        .find(|(_, line)| line.contains("PULL REQUESTS"))
    else {
        return false;
    };
    let heading = line[..line.find("PULL REQUESTS").unwrap()].chars().count() as u16;
    let fresh = line[..line.find("Fresh • fetched").unwrap_or(0)]
        .chars()
        .count() as u16;
    let (_, cols) = frame.size();
    line.contains("Fresh • fetched")
        // Right-aligned against the pane's right border.
        && line.trim_end().ends_with("UTC│")
        && line.chars().count() == usize::from(cols)
        && frame
            .cell(row as u16, heading)
            .is_some_and(|cell| cell.fgcolor() == MAUVE && cell.bold())
        && frame
            .cell(row as u16, fresh)
            .is_some_and(|cell| cell.fgcolor() == MUTED)
        // Partial coverage keeps its label: never shown as zero or dropped.
        && contents.contains("3 unique PRs across")
        && contents.contains("PARTIAL")
}

/// Every author line opens with that author's filled initials chip in its identity hue.
fn chips(frame: &vt100::Screen) -> bool {
    let contents = frame.contents();
    let lane = |login: &str| rgb(CATPPUCCIN_MOCHA.lanes()[InitialsChip::slot(&person(login))]);
    let mut seen = [0, 0];
    for (row, line) in contents.lines().enumerate() {
        for (index, (login, initials)) in [("octocat", "OC"), ("hubot", "HU")].iter().enumerate() {
            let Some(at) = line.find(&format!("@{login} • ")) else {
                continue;
            };
            let column = line[..at].chars().count() as u16;
            let Some(chip) = column.checked_sub(4) else {
                return false;
            };
            let filled = initials.chars().enumerate().all(|(offset, letter)| {
                frame
                    .cell(row as u16, chip + offset as u16)
                    .is_some_and(|cell| {
                        cell.contents() == letter.to_string()
                            && cell.bgcolor() == lane(login)
                            && cell.bold()
                    })
            });
            // One blank cell of theme background separates the chip from the label.
            let gap = frame
                .cell(row as u16, chip + 2)
                .is_some_and(|cell| cell.bgcolor() != lane(login));
            if !filled || !gap {
                return false;
            }
            seen[index] += 1;
        }
    }
    seen == [2, 1] && !contents.contains("[OC]") && !contents.contains("[HU]")
}

fn journey(mouse: bool, size: (u16, u16), resized: (u16, u16)) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("readme", b"saved text\n", "Inbox design saved version");
    fixture.scripted_gh(&protocol());
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let mut session = Session::launch(&fixture, size.0, size.1);
    session.wait_for_screen("Recent commits");
    if mouse {
        let (column, row) = session.locate("1 Inbox").expect("Inbox tab is painted");
        session.click(column, row);
    } else {
        session.send_key(b"1");
    }
    session.wait_for_screen("Inbox • focused view");
    session.choose(mouse, "[r Refresh]", b"r");
    session.wait_for_screen("Chip hubot row");
    session.wait_for_command_count(&fixture, 7);
    wait_cells(&mut session, 0, "section header and chips", |frame| {
        section_header(frame) && chips(frame)
    });
    session.capture_completed_terminal_v1(if mouse {
        "design-inbox-v94-v95-mouse"
    } else {
        "design-inbox-v94-v95-keyboard"
    });
    // Moving the selection keeps every chip; the selected row is marked without color.
    let after = session.completed_cells_end();
    session.send_key(b"\x1b[B");
    wait_cells(&mut session, after, "selection keeps chips", |frame| {
        chips(frame)
            && frame
                .contents()
                .lines()
                .any(|line| line.contains("> org/design#") && line.contains("Chip"))
    });
    let after = session.completed_cells_end();
    session.resize(resized.0, resized.1);
    wait_cells(&mut session, after, "after resize", |frame| {
        frame.size() == (resized.1, resized.0) && section_header(frame) && chips(frame)
    });
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.git(&["status", "--porcelain=v1"]), "");
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn design_hierarchy_v94_keyboard_section_header_partial_coverage_140x42_then_80x24() {
    journey(false, (140, 42), (80, 24));
}

#[test]
fn design_hierarchy_v94_mouse_section_header_partial_coverage_80x24_then_140x42() {
    journey(true, (80, 24), (140, 42));
}

#[test]
fn design_chips_v95_keyboard_list_rows_use_identity_chips_140x42_then_80x24() {
    journey(false, (140, 42), (80, 24));
}

#[test]
fn design_chips_v95_mouse_list_rows_use_identity_chips_80x24_then_140x42() {
    journey(true, (80, 24), (140, 42));
}
