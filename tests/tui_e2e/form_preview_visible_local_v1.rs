//! DECLARED/UNRUN. Real generic Local Form, actual installed Git and PTY.
use super::*;
use std::{fs, time::Duration};
fn marker(session: &Session) -> Option<String> {
    let text = session.completed_cursorless_screen()?.contents();
    let start = text.find("VISIBLE_ROW_")?;
    Some(text[start..].chars().take(15).collect())
}
fn step(session: &mut Session, mouse: bool) {
    let before = session.completed_cursorless_end();
    let old = session.completed_cursorless_screen().unwrap().contents();
    if mouse {
        session.wheel(30, 12, true)
    } else {
        session.send_key(b"\x1b[6~")
    }
    session.wait_for_new_cursorless_condition(
        before,
        |frame| {
            let text = frame.contents();
            text != old && text.contains("READ-ONLY PREVIEW") && text.contains("[Confirm action]")
        },
        "actual new generic prepared Preview scroll frame",
        Duration::from_secs(8),
    );
}
fn state(f: &Fixture) -> Vec<Vec<u8>> {
    vec![
        f.git(&["rev-parse", "HEAD"]).into_bytes(),
        f.git(&["show-ref"]).into_bytes(),
        f.git(&["--no-optional-locks", "status", "--porcelain=v2", "-z"])
            .into_bytes(),
        fs::read(f.repo().join(".git/index")).unwrap(),
        fs::read(f.repo().join(".git/config")).unwrap(),
        fs::read(f.repo().join("visible.txt")).unwrap(),
        fs::read(f.repo().join("dirty.txt")).unwrap(),
    ]
}
fn journey(mouse: bool) {
    let f = Fixture::new();
    f.init_git();
    f.commit_file(
        "visible.txt",
        b"original\n",
        "Original saved preview source",
    );
    let content = (0..220)
        .map(|row| {
            format!(
                "VISIBLE_ROW_{row:03} {} 界e\u{301}\n",
                "0123456789".repeat(12)
            )
        })
        .collect::<String>();
    fs::write(f.repo().join("visible.txt"), content.as_bytes()).unwrap();
    f.git(&["add", "visible.txt"]);
    fs::write(
        f.repo().join("dirty.txt"),
        b"UNTRACKED_DATA_NEVER_COMMITTED\0\xff",
    )
    .unwrap();
    let before = state(&f);
    let mut session = Session::launch(&f, 120, 40);
    session.wait_for_screen("Recent commits");
    operation(&mut session, mouse, "Commit staged changes", 4);
    session.paste("Bounded preview only • never Confirm");
    preview(&mut session, mouse);
    session.wait_for_screen("Exact staged content for the next commit:");
    for _ in 0..18 {
        step(&mut session, mouse);
    }
    let original = marker(&session).expect("genuine deep original patch row visible");
    for (width, height) in [(80, 24), (120, 40), (80, 24), (120, 40)] {
        let old = session.completed_cursorless_end();
        session.resize(width, height);
        session.wait_for_new_cursorless_condition(
            old,
            |frame| {
                frame.contents().contains(&original)
                    && frame.contents().contains("READ-ONLY PREVIEW")
                    && frame.contents().contains("[Confirm action]")
            },
            "original display-byte anchor survives actual64/88 resize without reparsing Git",
            Duration::from_secs(8),
        );
    }
    let old = session.completed_cursorless_end();
    session.resize(40, 24);
    session.wait_for_new_cursorless_condition(
        old,
        |frame| {
            let t = frame.contents();
            t.contains("resize required")
                && t.contains("[Ctrl+P Edit fields]")
                && t.contains("[Ctrl+Enter Confirm unavailable]")
                && t.contains("[Esc Close]")
        },
        "tiny prepared Preview exposes same disabled Confirm and accessible Edit/Close",
        Duration::from_secs(8),
    );
    let old = session.completed_cursorless_end();
    if mouse {
        session.click_visible_action("[Ctrl+Enter Confirm unavailable]")
    } else {
        session.send_key(b"\x1b[13;5u")
    }
    session.wait_for_new_cursorless_condition(
        old,
        |frame| frame.contents().contains("Confirm refused"),
        "actual hidden Confirm remains disabled after input",
        Duration::from_secs(8),
    );
    assert_eq!(state(&f), before);
    assert!(session.inspect_command_journal(&f).is_empty());
    if mouse {
        session.click_visible_action("[Ctrl+P Edit fields]")
    } else {
        session.send_key(b"\x10")
    }
    session.resize(120, 40);
    session.wait_for_screen("LOCAL INTENT • Preview first");
    session.wait_for_screen("Bounded preview only");
    close(&mut session, mouse);
    session.try_wait_for_condition(|_|{
        rusqlite::Connection::open_with_flags(f.root.join("config/ghtui/cache.sqlite3"),rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).and_then(|db|db.query_row(
            "SELECT body FROM drafts WHERE resource LIKE 'local/%' ORDER BY updated_at DESC LIMIT 1",[],|row|row.get::<_,String>(0)))
            .is_ok_and(|body|body.contains("Bounded preview only"))
    },"actual asynchronous raw local draft saved without a Git mutation",Duration::from_secs(8)).unwrap();
    let db = rusqlite::Connection::open_with_flags(
        f.root.join("config/ghtui/cache.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let encoded:String=db.query_row("SELECT body FROM drafts WHERE resource LIKE 'local:%' OR resource LIKE 'local/%' ORDER BY updated_at DESC LIMIT 1",[],|row|row.get(0)).unwrap();
    assert!(encoded.contains("Bounded preview only"));
    assert_eq!(state(&f), before);
    assert!(session.inspect_command_journal(&f).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn form_preview_visible_keyboard_local_deep_scroll_resize_hidden_confirm_edit_and_git_sql_tty() {
    journey(false)
}
#[test]
fn form_preview_visible_mouse_local_deep_scroll_resize_hidden_confirm_edit_and_git_sql_tty() {
    journey(true)
}
