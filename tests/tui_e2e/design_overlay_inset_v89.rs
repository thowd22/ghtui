//! TASK-89 follow-up: Help and Commands draw inset inside the content pane instead of on its
//! top border (`╭─╭─`), and entering Search focuses its content (`▶`) like the other views while
//! query typing keeps working. Real cells from the compiled binary at 80x24 and 140x42, by
//! keyboard and by mouse.
use crate::support::{Fixture, Session};
use std::time::{Duration, Instant};

const HELP: &str = "Help • keyboard and mouse";
const COMMANDS: &str = "Commands · current context";

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

fn symbol(frame: &vt100::Screen, row: u16, column: u16) -> String {
    frame
        .cell(row, column)
        .map(|cell| cell.contents().to_string())
        .unwrap_or_default()
}

/// The content pane's top row: the first row under the header that opens with `╭`.
fn pane_top(frame: &vt100::Screen) -> Option<u16> {
    (1..3).find(|row| symbol(frame, *row, 0) == "╭")
}

/// The overlay titled `title` is drawn wholly inside the content pane: the pane keeps its own
/// title and side/bottom borders, and the overlay frame starts below the pane's top border and
/// at least two columns in, closing above the pane's bottom border.
fn inset_overlay(frame: &vt100::Screen, title: &str) -> bool {
    let (rows, _) = frame.size();
    let contents = frame.contents();
    let lines = contents.lines().collect::<Vec<_>>();
    let Some(top) = pane_top(frame) else {
        return false;
    };
    let pane_title = lines.get(usize::from(top)).copied().unwrap_or_default();
    let Some(overlay_row) = (top + 1..rows).find(|row| {
        lines
            .get(usize::from(*row))
            .is_some_and(|line| line.contains(&format!("╭─ ▶ {title}")))
    }) else {
        return false;
    };
    let line = lines[usize::from(overlay_row)];
    let Some(left) = line.chars().position(|c| c == '╭').map(|at| at as u16) else {
        return false;
    };
    let bottom = (overlay_row + 1..rows).find(|row| symbol(frame, *row, left) == "╰");
    pane_title.contains("focused view")
        && !pane_title.contains("╭─╭")
        && !pane_title.contains(title)
        && symbol(frame, overlay_row, 0) == "│"
        && left >= 2
        && bottom.is_some_and(|bottom| bottom < rows - 3)
        && symbol(frame, rows - 3, 0) == "╰"
        && contents.contains("[Esc Close]")
}

fn search_marker(frame: &vt100::Screen) -> bool {
    let contents = frame.contents();
    pane_top(frame).is_some_and(|top| {
        contents
            .lines()
            .nth(usize::from(top))
            .is_some_and(|line| line.contains("▶ Search • focused view"))
    })
}

fn open(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    if mouse {
        let (column, row) = session.locate(label).expect("footer control is painted");
        session.click(column, row);
    } else {
        session.send_key(key);
    }
}

fn close(session: &mut Session, mouse: bool) {
    if mouse {
        let (column, row) = session.locate("[Esc Close]").expect("Close is painted");
        session.click(column, row);
    } else {
        session.send_key(b"\x1b");
    }
}

fn overlays(session: &mut Session, mouse: bool) {
    for (label, key, title) in [("[? Help]", b"?", HELP), ("[: Commands]", b":", COMMANDS)] {
        let after = session.completed_cells_end();
        open(session, mouse, label, key);
        wait_cells(session, after, title, |frame| inset_overlay(frame, title));
        let after = session.completed_cells_end();
        close(session, mouse);
        wait_cells(session, after, "overlay closed", |frame| {
            !frame.contents().contains(title)
        });
    }
}

fn journey(mouse: bool, size: (u16, u16), resized: (u16, u16)) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("readme", b"saved text\n", "Overlay saved version");
    std::fs::write(fixture.repo().join("readme"), b"local work to preserve\n").unwrap();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let status = fixture.git(&["status", "--porcelain=v1"]);
    let mut session = Session::launch(&fixture, size.0, size.1);
    session.wait_for_screen("Recent commits");

    overlays(&mut session, mouse);
    // Help stays inset across a resize while it is open.
    let after = session.completed_cells_end();
    open(&mut session, mouse, "[? Help]", b"?");
    wait_cells(&mut session, after, "Help before resize", |frame| {
        inset_overlay(frame, HELP)
    });
    let after = session.completed_cells_end();
    session.resize(resized.0, resized.1);
    wait_cells(&mut session, after, "Help after resize", |frame| {
        frame.size() == (resized.1, resized.0) && inset_overlay(frame, HELP)
    });
    let after = session.completed_cells_end();
    close(&mut session, mouse);
    wait_cells(&mut session, after, "Help closed", |frame| {
        !frame.contents().contains(HELP)
    });
    overlays(&mut session, mouse);
    assert!(
        session.inspect_command_journal(&fixture).is_empty(),
        "Help and Commands must not run any GitHub command"
    );

    // Entering Search focuses its content, like the other views.
    let after = session.completed_cells_end();
    if mouse {
        let (column, row) = session.locate("3 Search").expect("Search tab is painted");
        session.click(column, row);
    } else {
        session.send_key(b"3");
    }
    wait_cells(&mut session, after, "focused Search content", search_marker);
    // Query typing keeps its semantics and the content keeps focus while editing.
    let after = session.completed_cells_end();
    if mouse {
        let (column, row) = session.locate("[/ Query]").expect("Query field is painted");
        session.click(column, row);
    } else {
        session.send_key(b"/");
    }
    session.send_key(b"needle");
    wait_cells(&mut session, after, "typed Search query", |frame| {
        search_marker(frame) && frame.contents().contains("needle")
    });
    session.send_key(b"\x1b");

    // Read-only browsing never touches the working tree.
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.git(&["status", "--porcelain=v1"]), status);
    assert_eq!(
        std::fs::read(fixture.repo().join("readme")).unwrap(),
        b"local work to preserve\n"
    );
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn design_overlay_inset_v89_keyboard_help_commands_inset_and_search_focus_80x24_then_140x42() {
    journey(false, (80, 24), (140, 42));
}

#[test]
fn design_overlay_inset_v89_mouse_help_commands_inset_and_search_focus_140x42_then_80x24() {
    journey(true, (140, 42), (80, 24));
}
