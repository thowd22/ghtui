//! Input acknowledgements use a real Help round trip, never a readiness sleep.
use crate::support::{controls::view_title, Fixture, Session};
use std::{fs, path::Path, time::Duration};

const HELP: &str = "Help • keyboard and mouse";
const MENU: &str = "Local operations • explicit preview and confirmation";
fn state(f: &Fixture) -> Vec<Vec<u8>> {
    vec![
        f.git(&["rev-parse", "HEAD"]).into_bytes(),
        f.git(&["show-ref"]).into_bytes(),
        f.git(&["--no-optional-locks", "status", "--porcelain=v2", "-z"])
            .into_bytes(),
        fs::read(f.repo().join(".git/index")).unwrap(),
        fs::read(f.repo().join(".git/config")).unwrap(),
        fs::read(f.repo().join("history.txt")).unwrap(),
        fs::read(f.repo().join("scratch.txt")).unwrap(),
    ]
}

fn ready(session: &mut Session) -> String {
    session.wait_for_cursorless_condition(
        |frame| {
            !frame.contents().contains(HELP)
                && frame.contents().contains(&view_title("Repository"))
                && frame.contents().contains("[? Help]")
        },
        "complete repository frame",
        Duration::from_secs(8),
    );
    session.completed_cursorless_screen().unwrap().contents()
}

fn acknowledge(session: &mut Session, mouse: bool) -> String {
    // The later Help event can only be handled after the queued wheel. Return
    // to the same pane without changing its scroll to observe the wheel effect.
    if mouse {
        session.click_visible_action("[? Help]");
    } else {
        session.send_key(b"?");
    }
    session.wait_for_cursorless_condition(
        |frame| {
            frame.contents().contains(HELP)
                && frame.contents().contains("Repository • help for this view")
                && frame.contents().contains("[Esc Close]")
        },
        "Help acknowledges all preceding terminal input",
        Duration::from_secs(8),
    );
    let after = session.completed_cursorless_end();
    if mouse {
        session.click_visible_action("[Esc Close]");
    } else {
        session.send_key(b"\x1b");
    }
    session.wait_for_new_cursorless_condition(
        after,
        |frame| !frame.contents().contains(HELP) && frame.contents().contains("[? Help]"),
        "new underlying frame after acknowledged wheel",
        Duration::from_secs(8),
    );
    ready(session)
}

fn popup_journey(session: &mut Session, mouse: bool, height: u16) {
    if mouse {
        session.click_visible_action("[m Local operations]");
    } else {
        session.send_key(b"m");
    }
    session.wait_for_cursorless_condition(
        |frame| {
            frame.contents().contains(MENU) && frame.contents().contains("> Stage selected file")
        },
        "actual original local menu selection",
        Duration::from_secs(8),
    );
    // Down acknowledges the earlier wheel without resetting the menu. A buggy
    // outside +3 followed by Down can never pass the expected +1 selection.
    session.wheel(5, height - 1, true);
    session.send_key(b"\x1b[B");
    session.wait_for_cursorless_condition(
        |frame| {
            frame.contents().contains(MENU) && frame.contents().contains("> Unstage selected file")
        },
        "off-popup wheel ignored before the acknowledged first selection step",
        Duration::from_secs(8),
    );
    if mouse {
        let (x, y) = session
            .locate("> Unstage selected file")
            .expect("observed selected popup row");
        session.wheel(x, y, true);
        session.send_key(b"\x1b[A"); // +3 then -1 reaches the same row as two Down keys.
    } else {
        session.send_key(b"\x1b[B\x1b[B");
    }
    session.wait_for_cursorless_condition(
        |frame| {
            frame.contents().contains(MENU) && frame.contents().contains("> Unstage selected lines")
        },
        "hovered popup wheel and keyboard reach the exact same selection",
        Duration::from_secs(8),
    );
    let after = session.completed_cursorless_end();
    if mouse {
        session.click_visible_action("[Esc Close]");
    } else {
        session.send_key(b"\x1b");
    }
    session.wait_for_new_cursorless_condition(
        after,
        |frame| !frame.contents().contains(MENU) && frame.contents().contains("[? Help]"),
        "popup closes without running any local command",
        Duration::from_secs(8),
    );
}

fn journey(mouse: bool) {
    let f = Fixture::new();
    f.scripted_gh(&[]);
    f.init_git();
    for version in 0..40 {
        f.commit_file(
            "history.txt",
            format!("saved version {version}\n").as_bytes(),
            &format!("HOVER_SAVED_{version:02}"),
        );
    }
    fs::write(f.repo().join("history.txt"), b"working bytes preserved\n").unwrap();
    fs::write(f.repo().join("scratch.txt"), b"untracked\0\xff").unwrap();
    let before = state(&f);
    let trace = f.root.join("hover-git-trace.jsonl");
    let mut session = Session::launch_binary(
        Path::new(env!("CARGO_BIN_EXE_ghtui")),
        &f,
        120,
        40,
        &[
            ("NO_COLOR", "1"),
            ("GIT_TRACE2_EVENT", trace.to_str().unwrap()),
        ],
    );
    session.wait_for_screen("Recent commits");
    session.assert_focused_view("Repository");
    let git_before = fs::read(&trace).expect("observed startup Git trace");
    assert!(!git_before.is_empty());
    let config_path = f.root.join("config/ghtui/config.toml");
    let config_before = fs::read(&config_path).ok();
    for (width, height) in [(120, 40), (80, 24)] {
        if width == 80 {
            session.resize(width, height);
            session.wait_for_cursorless_condition(
                |frame| frame.size() == (height, width) && frame.contents().contains("[? Help]"),
                "resized actual pane",
                Duration::from_secs(8),
            );
        }
        let original = ready(&mut session);
        // The pane title sits in its rounded top border directly below the
        // merged header. The footer, the first header row and the header row
        // next to the border are all outside the content pane.
        let (_, title_row) = session
            .locate(&view_title("Repository"))
            .expect("observed repository pane title");
        assert!(title_row > 0, "the merged header precedes the content pane");
        let mut points = vec![(5, height - 1), (5, 0)];
        if title_row > 1 {
            points.push((5, title_row - 1));
        }
        for point in points {
            session.wheel(point.0, point.1, true);
            let after = acknowledge(&mut session, mouse);
            assert_eq!(
                after, original,
                "an acknowledged off-pane wheel changed the repository screen"
            );
        }
        if mouse {
            for _ in 0..10 {
                session.wheel(10, 12, true);
                acknowledge(&mut session, mouse);
            }
        } else {
            for _ in 0..3 {
                session.send_key(b"\x1b[6~");
                acknowledge(&mut session, mouse);
            }
        }
        assert_ne!(
            ready(&mut session),
            original,
            "hovered wheel and keyboard paging reach later rows"
        );
        if mouse {
            for _ in 0..10 {
                session.wheel(10, 12, false);
                acknowledge(&mut session, mouse);
            }
        } else {
            for _ in 0..3 {
                session.send_key(b"\x1b[5~");
                acknowledge(&mut session, mouse);
            }
        }
        assert_eq!(
            ready(&mut session),
            original,
            "both input routes restore the exact initial pane"
        );
        popup_journey(&mut session, mouse, height);
    }
    assert_eq!(state(&f), before);
    assert_eq!(
        fs::read(&trace).unwrap(),
        git_before,
        "scrolling and Help launch no additional Git"
    );
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
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn hover_routing_keyboard_footer_header_help_barrier_paging_resize_and_readonly() {
    journey(false);
}

#[test]
fn hover_routing_mouse_footer_header_help_barrier_hovered_wheel_resize_and_readonly() {
    journey(true);
}
