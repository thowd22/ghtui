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
    let title = lines.get(usize::from(top)).copied().unwrap_or_default();
    let active = TABS
        .iter()
        .find(|tab| tab.ends_with(view))
        .and_then(|tab| column_of(header, tab));
    header.starts_with(" ghtui ")
        && TABS.iter().all(|tab| header.contains(tab))
        && (header.contains("[Back]") || lines.get(1).is_some_and(|row| row.contains("[Back]")))
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
                .any(|line| line.contains("Repository • focused view") && line.contains("Local "))
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
