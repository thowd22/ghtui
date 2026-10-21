//! TASK-122: the command palette uses the shared fuzzy finder. Typing an
//! abbreviation ranks live, shows "N matching of M", highlights the matched
//! characters of each result, and Up/Down/Enter or a click run the result.
//! Keyboard and mouse; the repository, refs, index and saved settings stay as
//! they were and no gh command runs.
use crate::support::{Fixture, Session};
use std::{
    fs,
    time::{Duration, Instant},
};

const PALETTE: &str = "Commands · current context";
const SETTINGS: &str = "Settings • local preferences";

/// Underlined (highlighted) chars of the screen row `y` from column `x`, `len` cells.
fn marked(session: &Session, y: u16, x: u16, len: u16) -> Vec<(usize, String)> {
    let screen = session.completed_cells_screen().unwrap();
    (0..len)
        .filter_map(|offset| {
            let cell = screen.cell(y, x + offset)?;
            cell.underline()
                .then(|| (usize::from(offset), cell.contents()))
        })
        .collect()
}

fn count_caption(screen: &str) -> Option<(usize, usize)> {
    let line = screen.lines().find(|line| line.contains(" matching of "))?;
    let at = line.find(" matching of ")?;
    let shown: usize = line[..at]
        .rsplit(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()?;
    let all: usize = line[at + 13..]
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()?;
    Some((shown, all))
}

fn journey(mouse: bool) {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[]); // Reject every unexpected gh command.
    fixture.init_git();
    fixture.commit_file("file.txt", b"saved bytes\n", "initial saved version");
    let config = fixture.root.join("config/ghtui/config.toml");
    let before = (
        fixture.git(&["rev-parse", "HEAD"]),
        fixture.git(&["show-ref"]),
        fixture.git(&["status", "--porcelain=v2"]),
        fs::read(fixture.repo().join(".git/index")).unwrap(),
        fs::read(&config).ok(),
    );
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");
    session.wait_for_settled_footer();
    session.choose(mouse, "[: Commands]", b":");
    session.wait_for_overlay_body(PALETTE);
    session.wait_for_screen("Filter focused:");
    session
        .try_wait_for_condition(
            |screen| count_caption(screen).is_some(),
            "palette count",
            Duration::from_secs(8),
        )
        .unwrap();
    let (_, all) = count_caption(&session.screen()).unwrap();

    // An abbreviation, not a substring: the shared finder matches it live.
    session.paste("opn sttngs");
    session
        .try_wait_for_condition(
            |screen| {
                screen.contains("Open settings")
                    && count_caption(screen)
                        .is_some_and(|(shown, total)| shown >= 1 && shown < all && total == all)
            },
            "fuzzy palette results with a count",
            Duration::from_secs(8),
        )
        .unwrap_or_else(|error| panic!("{error}"));
    // The matched characters of the result are highlighted, the others are not.
    let deadline = Instant::now() + Duration::from_secs(8);
    let (x, y, marks) = loop {
        session.poll_output(Duration::from_millis(10));
        let found = session
            .completed_cells_screen()
            .and_then(|screen| crate::support::locate_screen(screen, "Open settings"));
        if let Some((x, y)) = found {
            let marks = marked(&session, y, x, 13);
            if !marks.is_empty() {
                break (x, y, marks);
            }
        }
        assert!(
            Instant::now() < deadline,
            "highlighted result row\n{}",
            session.screen()
        );
    };
    let spelled: String = marks.iter().map(|(_, c)| c.as_str()).collect();
    assert!(
        spelled.to_lowercase().chars().all(|c| "opnstg".contains(c)),
        "only typed letters are highlighted: {spelled:?}"
    );
    assert!(
        marks.iter().all(|(offset, _)| ![2, 6, 9].contains(offset)),
        "e/e/i are not typed letters: {marks:?}"
    );
    assert_eq!(marks[0].0, 0, "the O of Open is a match: {marks:?}");

    if mouse {
        // A click on the result runs it, like Enter on the selected row.
        session.click(x, y);
    } else {
        // Down walks the selection to the result, then Enter runs it.
        for _ in 0..12 {
            if session.screen().contains("> Open settings") {
                break;
            }
            let previous = session.screen();
            session.send_key(b"\x1b[B");
            session
                .try_wait_for_condition(
                    |screen| screen != previous,
                    "selection moved",
                    Duration::from_secs(4),
                )
                .unwrap();
        }
        assert!(session.screen().contains("> Open settings"));
        session.send_key(b"\r");
    }
    session.wait_for_overlay_close(PALETTE);
    session.wait_for_screen(SETTINGS);
    if mouse {
        session.close_overlay_by_mouse(SETTINGS);
    } else {
        session.close_overlay_by_key(SETTINGS);
    }

    // Esc on the palette closes it without running anything.
    session.choose(mouse, "[: Commands]", b":");
    session.wait_for_overlay_body(PALETTE);
    session.paste("zzqqxx");
    session.wait_for_screen("No matching commands");
    if mouse {
        session.close_overlay_by_mouse(PALETTE);
    } else {
        session.close_overlay_by_key(PALETTE);
    }
    assert!(!session.screen().contains(SETTINGS));

    let after = (
        fixture.git(&["rev-parse", "HEAD"]),
        fixture.git(&["show-ref"]),
        fixture.git(&["status", "--porcelain=v2"]),
        fs::read(fixture.repo().join(".git/index")).unwrap(),
        fs::read(&config).ok(),
    );
    assert_eq!(after, before, "the palette changed nothing");
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn fuzzy_finder_v122_palette_keyboard_abbreviation_highlight_count_enter_esc() {
    journey(false);
}

#[test]
fn fuzzy_finder_v122_palette_mouse_abbreviation_highlight_count_click_close() {
    journey(true);
}
