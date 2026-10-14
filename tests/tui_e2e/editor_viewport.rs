//! SOURCE-ONLY real-binary editor regression; unregistered and UNRUN. Exercises
//! the already active Local CreateBranch field, never a direct event handler.
//! No Preview/Confirm: the only allowed write is this exact local draft.
use crate::{
    local, local_management,
    support::{Fixture, Session},
};
use ghtui::local_actions::LocalCommand;
use rusqlite::{Connection, OptionalExtension};
use serde_json::Value;
use std::{
    fs,
    os::unix::ffi::OsStrExt,
    time::{Duration, Instant},
};

#[derive(Debug, PartialEq, Eq)]
struct GitState {
    head: String,
    refs: String,
    index: Vec<u8>,
    status: String,
    dirty: Vec<u8>,
    staged: Vec<u8>,
    untracked: Vec<u8>,
}
fn git_state(fixture: &Fixture) -> GitState {
    GitState {
        head: fixture.git(&["rev-parse", "HEAD"]),
        refs: fixture.git(&["show-ref"]),
        index: fs::read(fixture.repo().join(".git/index")).unwrap(),
        status: fixture.git(&["status", "--porcelain=v2", "-z"]),
        dirty: fs::read(fixture.repo().join("dirty界.txt")).unwrap(),
        staged: fs::read(fixture.repo().join("staged.txt")).unwrap(),
        untracked: fs::read(fixture.repo().join("scratch.txt")).unwrap(),
    }
}
fn draft_resource(fixture: &Fixture) -> String {
    let directory = fs::canonicalize(fixture.repo()).unwrap();
    let encoded = directory
        .as_os_str()
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("local/{encoded}/operation/createbranch")
}
fn draft(fixture: &Fixture) -> Option<Value> {
    let sql = Connection::open_with_flags(
        fixture.root.join("config/ghtui/cache.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let body: Option<String> = sql
        .query_row(
            "SELECT body FROM drafts WHERE host='local' AND login='workspace' AND resource=?1",
            [draft_resource(fixture)],
            |row| row.get(0),
        )
        .optional()
        .unwrap();
    body.map(|body| serde_json::from_str(&body).unwrap())
}
fn wait_draft(session: &mut Session, fixture: &Fixture, expected: &str, head: &str) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if let Some(body) = draft(fixture) {
            if body["values"]["name"].as_str() == Some(expected) {
                assert_eq!(body["version"], 1);
                assert_eq!(body["values"]["revision"], head);
                assert_eq!(body["values"].as_object().unwrap().len(), 2);
                assert_eq!(body["selections"], serde_json::json!({}));
                assert_eq!(body.as_object().unwrap().len(), 3);
                return;
            }
        }
        if Instant::now() >= deadline {
            session.save_artifacts();
            panic!(
                "exact raw draft never committed; artifacts: {}",
                session.artifact_dir.display()
            );
        }
        // Consume real output while waiting on the actual asynchronous SQL
        // acknowledgement. This is not a sleep-based readiness guess.
        session.poll_output(Duration::from_millis(10));
    }
}
fn name_field(session: &mut Session) -> (u16, u16) {
    session.wait_for_screen("New name:");
    let (column, row) = session.locate("New name:").unwrap();
    (column, row + 1)
}

/// Decode the real cursor escape output independently of Session's screen
/// helper. `epoch` starts before each actual PTY resize; the new full redraw
/// therefore uses this parser's dimensions, rather than replaying old geometry.
fn wait_caret(
    session: &mut Session,
    epoch: usize,
    columns: u16,
    row: u16,
    column: u16,
    field_x: u16,
    first: &str,
) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let mut parser = vt100::Parser::new(24, columns, 0);
        parser.process(&session.transcript[epoch..]);
        let screen = parser.screen();
        if screen.cursor_position() == (row, column)
            && screen
                .cell(row, field_x)
                .is_some_and(|cell| cell.contents() == first)
            && session.screen().contains("LOCAL INTENT • Preview first")
        {
            return;
        }
        if Instant::now() >= deadline {
            session.save_artifacts();
            panic!(
                "painted first grapheme and real caret disagree: wanted ({row},{column}), got {:?}; screen {}\nartifacts: {}",
                screen.cursor_position(),
                session.screen(),
                session.artifact_dir.display()
            );
        }
        session.poll_output(Duration::from_millis(10));
    }
}

#[test]
fn mouse_keyboard_editor_viewport_keyboard_wide_combining_caret_and_exact_draft() {
    journey(false);
}
#[test]
fn mouse_keyboard_editor_viewport_mouse_wide_combining_click_and_exact_draft() {
    journey(true);
}
fn journey(mouse: bool) {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[]); // Strictly reject every unexpected gh child.
    fixture.init_git();
    fixture.commit_file(
        "dirty界.txt",
        b"original saved bytes\n",
        "Unicode editor origin",
    );
    fixture.commit_file("staged.txt", b"saved stage bytes\n", "Stage origin");
    fs::write(
        fixture.repo().join("dirty界.txt"),
        b"untouched dirty bytes\n",
    )
    .unwrap();
    fs::write(
        fixture.repo().join("staged.txt"),
        b"untouched staged bytes\n",
    )
    .unwrap();
    fixture.git(&["add", "--", "staged.txt"]);
    fs::write(fixture.repo().join("scratch.txt"), b"untouched scratch\n").unwrap();
    let before = git_state(&fixture);
    let head = before.head.trim();
    let mut session = Session::launch(&fixture, 80, 24);
    session.wait_for_screen("Recent commits");
    local_management::operation(&mut session, mouse, LocalCommand::CreateBranch);
    session.wait_for_screen("[Keep draft / Close]");
    let (field_x, row) = name_field(&mut session);
    let raw = "界".repeat(64);
    session.paste(&raw);
    // Actual form: centered76-cell block, horizontal margins2+2 =>72 cells.
    // Origin58 means35 whole glyphs and a caret at70, NOT unaligned origin57.
    wait_caret(&mut session, 0, 80, row, field_x + 70, field_x, "界");
    wait_draft(&mut session, &fixture, &raw, head);
    assert!(session.inspect_command_journal(&fixture).is_empty());
    assert_eq!(git_state(&fixture), before);

    // At70 columns the same ACTIVE form has a62-cell field, matching the
    // inventory filter's80-column geometry without fabricating a picker source.
    let narrow_epoch = session.transcript.len();
    session.resize(70, 24);
    let (field_x, row) = name_field(&mut session);
    wait_caret(
        &mut session,
        narrow_epoch,
        70,
        row,
        field_x + 60,
        field_x,
        "界",
    );
    if mouse {
        session.click(field_x, row);
    } else {
        // Origin68 is exactly34 actual graphemes from the start.
        // End→30 Left events reaches that exact original byte boundary102.
        session.send_key(&b"\x1b[D".repeat(30));
    }
    wait_caret(&mut session, narrow_epoch, 70, row, field_x, field_x, "界");
    session.paste("Xe\u{301}");
    wait_caret(
        &mut session,
        narrow_epoch,
        70,
        row,
        field_x + 2,
        field_x,
        "X",
    );
    let inserted = format!("{}Xe\u{301}{}", "界".repeat(34), "界".repeat(30));
    wait_draft(&mut session, &fixture, &inserted, head);
    session.send_key(b"\x7f"); // Delete ONE complete combining grapheme.
    wait_caret(
        &mut session,
        narrow_epoch,
        70,
        row,
        field_x + 1,
        field_x,
        "X",
    );
    let expected = format!("{}X{}", "界".repeat(34), "界".repeat(30));
    wait_draft(&mut session, &fixture, &expected, head);
    session.resize(120, 40);
    session.wait_for_screen("[Keep draft / Close]");
    session.wait_for_screen("New name:");
    session.resize(80, 24);
    session.wait_for_screen("[Keep draft / Close]");
    local::close(&mut session, mouse);
    wait_draft(&mut session, &fixture, &expected, head);
    assert_eq!(git_state(&fixture), before);
    assert!(session.inspect_command_journal(&fixture).is_empty());

    // Reopen the ACTUAL persisted form: no editor approval or Git mutation is
    // restored. A named raw draft is not a choice, preview or publication.
    local_management::operation(&mut session, mouse, LocalCommand::CreateBranch);
    session
        .try_wait_for_condition(
            |screen| {
                screen.contains("New name:")
                    && screen.contains("LOCAL INTENT • Preview first")
                    && screen.contains("[Keep draft / Close]")
                    && !screen.contains("READ-ONLY PREVIEW • Confirm action")
            },
            "restored raw local draft without preview approval",
            Duration::from_secs(8),
        )
        .unwrap();
    // Restoring a fresh TextBuffer at80 gives origin58: five whole wide
    // graphemes, the exact inserted byte, then all30 remaining graphemes.
    session.wait_for_screen(&format!("{}X{}", "界".repeat(5), "界".repeat(30)));
    assert_eq!(draft(&fixture).unwrap()["values"]["name"], expected);
    local::close(&mut session, mouse);
    assert_eq!(git_state(&fixture), before);
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}
