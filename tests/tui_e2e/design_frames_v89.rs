//! TASK-89: one merged header row (logo chip, view tabs, Back), rounded pane frames carrying the
//! scope context in their top border, and the focused pane's accent border plus `▶` marker.
//! Real cells are read from the compiled binary at 140x42 and 80x24, by keyboard and by mouse.
use crate::support::{Fixture, Session};
use std::time::{Duration, Instant};
use vt100::Color;

const MAUVE: Color = Color::Rgb(0xcb, 0xa6, 0xf7);
const PEACH: Color = Color::Rgb(0xfa, 0xb3, 0x87);
const BORDER: Color = Color::Rgb(0x45, 0x47, 0x5a);
const TABS: [&str; 5] = [
    "1 Inbox",
    "2 Repository",
    "3 Search",
    "4 Actions",
    "5 Dashboard",
];

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

fn column_of(line: &str, text: &str) -> Option<u16> {
    line.find(text).map(|at| line[..at].chars().count() as u16)
}

/// The content pane's top row: the first row under the header that opens with `╭`.
fn pane_top(frame: &vt100::Screen) -> Option<u16> {
    (1..3).find(|row| {
        frame
            .cell(*row, 0)
            .is_some_and(|cell| cell.contents() == "╭")
    })
}

/// The whole chrome contract for one completed frame.
fn chrome(frame: &vt100::Screen, view: &str, hue: Color, focused: bool) -> bool {
    let (rows, cols) = frame.size();
    let contents = frame.contents();
    let lines = contents.lines().collect::<Vec<_>>();
    let Some(top) = pane_top(frame) else {
        return false;
    };
    let header = lines.first().copied().unwrap_or_default();
    // The content pane's own top border: up to its first `╮` (a Preview or Details pane may
    // follow it on the same row from 120 columns).
    let title = lines
        .get(usize::from(top))
        .copied()
        .unwrap_or_default()
        .split_inclusive('╮')
        .next()
        .unwrap_or_default();
    let active = TABS
        .iter()
        .find(|tab| tab.ends_with(view))
        .and_then(|tab| column_of(header, tab));
    // One header row at 80 columns and wider: Back is a key-bar style token, never bracketed.
    header.starts_with(" ghtui ")
        && top == 1
        && TABS.iter().all(|tab| header.contains(tab))
        && header.contains("5 Dashboard  Alt-← Back")
        && !header.contains('[')
        // Logo and active tab are filled with the view hue; text stays bold.
        && frame
            .cell(0, 1)
            .is_some_and(|cell| cell.bgcolor() == hue && cell.bold())
        && active.is_some_and(|column| {
            frame
                .cell(0, column)
                .is_some_and(|cell| cell.bgcolor() == hue)
        })
        // Inactive tabs are not filled.
        && TABS
            .iter()
            .filter(|tab| !tab.ends_with(view))
            .filter_map(|tab| column_of(header, tab))
            .all(|column| frame.cell(0, column).is_some_and(|cell| cell.bgcolor() != hue))
        && title.contains(&format!("{view} • focused view ─"))
        && title.contains('▶') == focused
        && title.trim_end().ends_with(" ─╮")
        && frame
            .cell(top + 1, 0)
            .is_some_and(|cell| cell.contents() == "│" && cell.fgcolor() == if focused { hue } else { BORDER })
        && frame
            .cell(rows - 3, 0)
            .is_some_and(|cell| cell.contents() == "╰")
        && frame
            .cell(rows - 3, cols - 1)
            .is_some_and(|cell| cell.contents() == "╯")
}

fn choose_tab(session: &mut Session, mouse: bool, tab: &str) {
    if mouse {
        let (column, row) = session.locate(tab).expect("tab is painted");
        session.click(column, row);
    } else {
        session.send_key(&tab.as_bytes()[..1]);
    }
}

fn journey(mouse: bool, size: (u16, u16), resized: (u16, u16)) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("readme", b"saved text\n", "Frame saved version");
    std::fs::write(fixture.repo().join("readme"), b"local work to preserve\n").unwrap();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let status = fixture.git(&["status", "--porcelain=v1"]);
    let mut session = Session::launch(&fixture, size.0, size.1);
    session.wait_for_screen("Recent commits");

    let after = session.completed_cells_end();
    choose_tab(&mut session, mouse, "2 Repository");
    wait_cells(&mut session, after, "focused Repository frame", |frame| {
        chrome(frame, "Repository", PEACH, true)
            && frame
                .contents()
                .lines()
                .any(|line| line.contains("Repository • focused view") && line.contains(" / main "))
    });

    // Tab moves focus into the header: the marker and the accent border leave the pane.
    let after = session.completed_cells_end();
    session.send_key(b"\t");
    wait_cells(&mut session, after, "unfocused Repository frame", |frame| {
        chrome(frame, "Repository", PEACH, false)
    });

    let after = session.completed_cells_end();
    choose_tab(&mut session, mouse, "1 Inbox");
    wait_cells(&mut session, after, "focused Inbox frame", |frame| {
        chrome(frame, "Inbox", MAUVE, true)
    });
    session.capture_completed_terminal_v1(if mouse {
        "design-frames-v89-inbox-mouse"
    } else {
        "design-frames-v89-inbox-keyboard"
    });

    let after = session.completed_cells_end();
    session.resize(resized.0, resized.1);
    wait_cells(&mut session, after, "Inbox frame after resize", |frame| {
        frame.size() == (resized.1, resized.0) && chrome(frame, "Inbox", MAUVE, true)
    });
    let after = session.completed_cells_end();
    choose_tab(&mut session, mouse, "2 Repository");
    wait_cells(
        &mut session,
        after,
        "Repository frame after resize",
        |frame| chrome(frame, "Repository", PEACH, true),
    );

    // Browsing chrome never touches the working tree or GitHub.
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.git(&["status", "--porcelain=v1"]), status);
    assert_eq!(
        std::fs::read(fixture.repo().join("readme")).unwrap(),
        b"local work to preserve\n"
    );
    assert!(
        session.inspect_command_journal(&fixture).is_empty(),
        "frames must not run any GitHub command"
    );
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn design_frames_v89_keyboard_merged_header_rounded_focus_frames_140x42_then_80x24() {
    journey(false, (140, 42), (80, 24));
}

#[test]
fn design_frames_v89_mouse_merged_header_rounded_focus_frames_80x24_then_140x42() {
    journey(true, (80, 24), (140, 42));
}

/// The header row and the row under it (the pane's top border when the header is one row).
fn rows(frame: &vt100::Screen) -> (String, String) {
    let contents = frame.contents();
    let mut lines = contents.lines();
    let header = lines.next().unwrap_or_default().to_owned();
    let top = lines.next().unwrap_or_default().to_owned();
    (header, top)
}

/// Back and Source identity are one-row key-bar style tokens (`Alt-← Back` in the header,
/// `I Source` closing the Repository pane border); each is clicked or pressed with the same
/// effect, at 80x24 and 140x42, without touching the working tree.
fn header_journey(mouse: bool, size: (u16, u16)) {
    const SOURCE_TITLE: &str = "Repository source identity • read-only";
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("readme", b"saved text\n", "Header saved version");
    std::fs::write(fixture.repo().join("readme"), b"local work to preserve\n").unwrap();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let status = fixture.git(&["status", "--porcelain=v1"]);
    let mut session = Session::launch(&fixture, size.0, size.1);
    session.wait_for_screen("Recent commits");
    // The Repository pane spells the token out from 100 columns; beside the 140-column
    // Details pane it is narrower than that and keeps the short form.
    let source = "I Source";
    let pane_top = |top: &str| {
        top.split_inclusive('╮')
            .next()
            .unwrap_or_default()
            .to_owned()
    };
    let source_closes = |top: &str| {
        let pane = pane_top(top);
        pane.ends_with(&format!("─ {source} ─╮"))
            || pane.ends_with(&format!("─ {source} identity ─╮"))
    };

    let after = session.completed_cells_end();
    choose_tab(&mut session, mouse, "2 Repository");
    wait_cells(
        &mut session,
        after,
        "one-row header with Source token",
        |frame| {
            let (header, top) = rows(frame);
            chrome(frame, "Repository", PEACH, true)
                && !header.contains("Source")
                && source_closes(&top)
                // The scope keeps the branch; a long temporary path drops to `repo / main`.
                && top.contains(" / main ")
        },
    );
    session.capture_completed_terminal_v1(if mouse {
        "design-header-v89-repository-mouse"
    } else {
        "design-header-v89-repository-keyboard"
    });

    let after = session.completed_cells_end();
    if mouse {
        let (column, row) = session.locate("I Source").expect("Source token is painted");
        assert_eq!(row, 1, "Source identity sits in the pane's top border");
        session.click(column + 2, row);
    } else {
        session.send_key(b"I");
    }
    wait_cells(
        &mut session,
        after,
        "Source identity explanation",
        |frame| frame.contents().contains(SOURCE_TITLE),
    );
    let after = session.completed_cells_end();
    if mouse {
        session.click_visible_action("[Esc Close]");
    } else {
        session.send_key(b"\x1b");
    }
    wait_cells(&mut session, after, "explanation closed", |frame| {
        let (header, top) = rows(frame);
        !frame.contents().contains(SOURCE_TITLE)
            && header.contains("Alt-← Back")
            && top.contains(source)
    });

    // Leave for Search, then Back returns to the same Repository page.
    let after = session.completed_cells_end();
    choose_tab(&mut session, mouse, "3 Search");
    wait_cells(&mut session, after, "Search with one-row Back", |frame| {
        let (header, top) = rows(frame);
        // Back waits for the Search corpus read, so the journey does too.
        top.contains("Search • focused view")
            && frame
                .contents()
                .contains("Selected local search corpus ready")
            && header.contains("5 Dashboard  Alt-← Back")
            && !header.contains('[')
            // A crumb names the earlier page when the row has room; it never wraps.
            && (size.0 < 100 || header.contains("Alt-← Back  Repository • "))
    });
    let after = session.completed_cells_end();
    if mouse {
        let (column, row) = session.locate("Alt-← Back").expect("Back token is painted");
        assert_eq!(row, 0);
        session.click(column + 1, row);
    } else {
        session.send_key(b"\x1b[1;3D");
    }
    wait_cells(&mut session, after, "Back restored Repository", |frame| {
        let (_, top) = rows(frame);
        top.contains("Repository • focused view") && top.contains(source)
    });

    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.git(&["status", "--porcelain=v1"]), status);
    assert_eq!(
        std::fs::read(fixture.repo().join("readme")).unwrap(),
        b"local work to preserve\n"
    );
    assert!(
        session.inspect_command_journal(&fixture).is_empty(),
        "header navigation must not run any GitHub command"
    );
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn design_header_v89_keyboard_one_row_back_and_source_tokens_80x24() {
    header_journey(false, (80, 24));
}

#[test]
fn design_header_v89_mouse_one_row_back_and_source_tokens_80x24() {
    header_journey(true, (80, 24));
}

#[test]
fn design_header_v89_keyboard_one_row_back_and_source_tokens_140x42() {
    header_journey(false, (140, 42));
}

#[test]
fn design_header_v89_mouse_one_row_back_and_source_tokens_140x42() {
    header_journey(true, (140, 42));
}
