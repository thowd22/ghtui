//! Real terminal input, completed-frame geometry and independent read-only effects.
use crate::support::{Fixture, Session};
use std::{fs, path::Path, time::Duration};

const TITLE: &str = "Help • keyboard and mouse";
const FIRST: &str = "Repository • help for this view";
const LAST: &str = "no-color mode.";
/// Help opens with the view's footer keys as titled key tables, before the prose.
const KEYS: &str = "Keys • ";

fn protected(f: &Fixture) -> Vec<Vec<u8>> {
    vec![
        f.git(&["rev-parse", "HEAD"]).into_bytes(),
        f.git(&["show-ref"]).into_bytes(),
        f.git(&["--no-optional-locks", "status", "--porcelain=v2", "-z"])
            .into_bytes(),
        fs::read(f.repo().join(".git/index")).unwrap(),
        fs::read(f.repo().join(".git/config")).unwrap(),
        fs::read(f.repo().join("saved.txt")).unwrap(),
        fs::read(f.repo().join("scratch界.txt")).unwrap(),
    ]
}

fn wait(session: &mut Session, after: usize, predicate: impl Fn(&vt100::Screen) -> bool) {
    session.wait_for_new_cursorless_condition(
        after,
        |frame| frame.contents().contains(TITLE) && predicate(frame),
        "new complete owned Help frame after actual input",
        Duration::from_secs(8),
    );
}

// Locate symbols in the ACTUAL completed terminal, not production geometry.
fn symbol(frame: &vt100::Screen, symbol: &str) -> (u16, u16) {
    let (rows, cols) = frame.size();
    let cells = (0..rows)
        .flat_map(|row| (0..cols).map(move |col| (col, row)))
        .filter(|(col, row)| {
            frame
                .cell(*row, *col)
                .is_some_and(|cell| cell.contents() == symbol)
        })
        .collect::<Vec<_>>();
    assert!(!cells.is_empty(), "observed scrollbar symbol {symbol}");
    assert!(cells.iter().all(|(col, _)| *col == cells[0].0));
    cells[0]
}

fn journey(mouse: bool) {
    let f = Fixture::new();
    f.scripted_gh(&[]); // Every GitHub child, including a read, is unexpected.
    f.init_git();
    f.commit_file(
        "saved.txt",
        b"saved scrollbar source\n",
        "Scrollbar saved version",
    );
    fs::write(f.repo().join("saved.txt"), b"working bytes retained\n").unwrap();
    fs::write(f.repo().join("scratch界.txt"), b"untracked\0\xff").unwrap();
    let before = protected(&f);
    let trace = f.root.join("help-git-trace.jsonl");
    let mut session = Session::launch_binary(
        Path::new(env!("CARGO_BIN_EXE_ghtui")),
        &f,
        120,
        40,
        &[
            ("GIT_TRACE2_EVENT", trace.to_str().unwrap()),
            ("NO_COLOR", "1"),
        ],
    );
    session.wait_for_screen("Recent commits");
    session.assert_focused_view("Repository");
    let git_before = fs::read(&trace).expect("actual application Git trace exists");
    assert!(!git_before.is_empty());
    let config_path = f.root.join("config/ghtui/config.toml");
    let config_before = fs::read(&config_path).ok();
    if mouse {
        session.click_visible_action("[? Help]");
    } else {
        session.send_key(b"?");
    }
    session.wait_for_cursorless_condition(
        |frame| {
            frame.contents().contains(TITLE)
                && frame.contents().contains(FIRST)
                && frame.contents().contains(KEYS)
                && frame.contents().contains('▲')
                && frame.contents().contains('▼')
        },
        "full initial Help body and actual scrollbar",
        Duration::from_secs(8),
    );
    session.capture_completed_terminal_v1("task4-help-scrollbar-120x40");

    for (cols, rows) in [(120, 40), (80, 24)] {
        if cols == 80 {
            let after = session.completed_cursorless_end();
            session.resize(cols, rows);
            wait(&mut session, after, |frame| {
                frame.size() == (rows, cols) && frame.contents().contains(FIRST)
            });
        }
        let after = session.completed_cursorless_end();
        if mouse {
            let down = symbol(session.completed_cursorless_screen().unwrap(), "▼");
            session.click(down.0, down.1);
        } else {
            session.send_key(b"\x1b[B");
        }
        // One line down: the title leaves and the first key table heads the body.
        wait(&mut session, after, |frame| {
            !frame.contents().contains(FIRST) && frame.contents().contains(KEYS)
        });
        let after = session.completed_cursorless_end();
        if mouse {
            let up = symbol(session.completed_cursorless_screen().unwrap(), "▲");
            session.click(up.0, up.1);
        } else {
            session.send_key(b"\x1b[A");
        }
        wait(&mut session, after, |frame| {
            frame.contents().contains(FIRST)
        });

        let after = session.completed_cursorless_end();
        if mouse {
            let down = symbol(session.completed_cursorless_screen().unwrap(), "▼");
            session.click(down.0, down.1 - 1); // Observed track below top thumb: one page.
        } else {
            session.send_key(b"\x1b[6~");
        }
        wait(&mut session, after, |frame| {
            !frame.contents().contains(FIRST)
        });
        let after = session.completed_cursorless_end();
        if mouse {
            let up = symbol(session.completed_cursorless_screen().unwrap(), "▲");
            session.click(up.0, up.1 + 1); // Observed track above moved thumb: one page.
        } else {
            session.send_key(b"\x1b[5~");
        }
        wait(&mut session, after, |frame| {
            frame.contents().contains(FIRST)
        });

        let after = session.completed_cursorless_end();
        if mouse {
            let thumb = symbol(session.completed_cursorless_screen().unwrap(), "█");
            let down = symbol(session.completed_cursorless_screen().unwrap(), "▼");
            session.drag(thumb, down);
        } else {
            session.send_key(b"\x1b[F");
        }
        wait(&mut session, after, |frame| {
            frame.contents().contains(LAST) && !frame.contents().contains(FIRST)
        });
        session.capture_completed_terminal_v1(if cols == 80 {
            "task4-help-scrollbar-tail-80x24"
        } else {
            "task4-help-scrollbar-tail-120x40"
        });
        let after = session.completed_cursorless_end();
        if mouse {
            let thumb = symbol(session.completed_cursorless_screen().unwrap(), "█");
            let up = symbol(session.completed_cursorless_screen().unwrap(), "▲");
            session.drag(thumb, up);
        } else {
            session.send_key(b"\x1b[H");
        }
        wait(&mut session, after, |frame| {
            frame.contents().contains(FIRST)
        });

        // Background tab input must not leave Help or queue a source read.
        let after = session.completed_cursorless_end();
        if mouse {
            session.click(35, 1);
        } else {
            session.send_key(b"3");
        }
        wait(&mut session, after, |frame| {
            frame.contents().contains(FIRST)
        });
        if mouse {
            session.click_visible_action("[Esc Close]");
        } else {
            session.send_key(b"\x1b");
        }
        session.wait_for_screen("Recent commits");
        session.assert_focused_view("Repository");
        if mouse {
            session.click_visible_action("[? Help]");
        } else {
            session.send_key(b"?");
        }
        session.wait_for_cursorless_condition(
            |frame| frame.contents().contains(TITLE) && frame.contents().contains(FIRST),
            "fresh Help owner starts at the first row",
            Duration::from_secs(8),
        );
    }
    assert_eq!(
        fs::read(&trace).unwrap(),
        git_before,
        "Help starts zero additional Git children"
    );
    assert_eq!(protected(&f), before);
    assert_eq!(fs::read(&config_path).ok(), config_before);
    assert!(session.inspect_command_journal(&f).is_empty());
    let db = rusqlite::Connection::open_with_flags(
        f.root.join("config/ghtui/cache.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM drafts", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    if mouse {
        session.click_visible_action("[Esc Close]");
    } else {
        session.send_key(b"\x1b");
    }
    session.wait_for_screen("Recent commits");
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn mouse_keyboard_scrollbar_help_keyboard() {
    journey(false);
}

#[test]
fn mouse_keyboard_scrollbar_help_mouse() {
    journey(true);
}
