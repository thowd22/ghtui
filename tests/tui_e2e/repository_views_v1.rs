//! Daily repository browsing: readable graph rows, tree Back to the opened
//! file, and plain diff file headings, with independent Git effect checks.
use crate::support::{Fixture, Session};
use std::{
    fs,
    time::{Duration, Instant},
};

fn choose(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    if mouse {
        session.click_visible_action(label);
    } else {
        session.send_key(key);
    }
}

fn select_row(session: &mut Session, mouse: bool, label: &str) {
    if mouse {
        session.click_visible_action(label);
        return;
    }
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        if let Some((column, row)) = session.locate(label) {
            if session.cell_inverted(row, column) {
                session.send_key(b"\r");
                return;
            }
        }
        assert!(
            Instant::now() < deadline,
            "Row never keyboard-selected: {label}\n{}",
            session.screen()
        );
        session.send_key(b"\x1b[B");
        session.poll_output(Duration::from_millis(50));
    }
}

fn wait_selected(session: &mut Session, label: &str) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(20));
        if let Some((column, row)) = session.locate(label) {
            if session.cell_inverted(row, column) {
                return;
            }
        }
        assert!(
            Instant::now() < deadline,
            "Row never reselected: {label}\n{}",
            session.screen()
        );
    }
}

#[test]
fn v1_repo_views_keyboard_graph_tree_back_and_plain_diff_headings() {
    journey(false);
}

#[test]
fn v1_repo_views_mouse_graph_tree_back_and_plain_diff_headings() {
    journey(true);
}

fn journey(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("notes.txt", b"first note\n", "First note");
    fixture.commit_file("src/main.rs", b"fn main() {}\n", "Add the main program");
    fixture.commit_file("notes.txt", b"second note\n", "Second note");
    let head = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    let seconds = fixture
        .git(&["log", "-1", "--format=%at"])
        .trim()
        .to_owned();
    let date = String::from_utf8(
        std::process::Command::new("date")
            .args(["-u", "-d", &format!("@{seconds}"), "+%F"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_owned();
    fs::write(fixture.repo().join("src/main.rs"), b"uncommitted edit\n").unwrap();
    let refs = fixture.git(&["show-ref"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2"]);

    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");
    // One name for a commit authored and committed by the same person.
    session.wait_for_screen(&format!("Second note • Fixture Author • {date}"));
    assert!(!session.screen().contains("• committer Fixture Author"));

    choose(&mut session, mouse, "[g Graph]", b"g");
    session.wait_for_screen("Visible commits 1–3 of 3 loaded (limit 10000)");
    session.wait_for_screen(&format!(
        "▶ selected {} Second note (HEAD, main) • Fixture Author",
        &head[..8]
    ));
    session.wait_for_screen("Add the main program • Fixture Author");
    assert!(
        !session.screen().contains("↳ parent"),
        "linear history uses one row per commit\n{}",
        session.screen()
    );

    choose(&mut session, mouse, "[t Tree]", b"t");
    session.wait_for_screen("Saved file tree");
    select_row(&mut session, mouse, "▸ src");
    session.wait_for_screen("▾ src");
    select_row(&mut session, mouse, "· main.rs");
    session.wait_for_screen("src/main.rs at saved version");
    session.wait_for_screen("fn main() {}");
    session.wait_for_screen("[u Back to tree]");
    // Keyboard uses Esc; the mouse clicks the visible Back control.
    choose(&mut session, mouse, "[u Back to tree]", b"\x1b");
    session.wait_for_screen("Saved file tree");
    session.wait_for_screen("▾ src");
    wait_selected(&mut session, "· main.rs");
    assert!(!session.screen().contains("uncommitted edit"));

    choose(&mut session, mouse, "[d Changes]", b"d");
    session.wait_for_screen("Saved commit changes");
    session.wait_for_screen("+second note");
    let screen = session.screen();
    assert!(screen.contains("│notes.txt "), "{screen}");
    assert!(!screen.contains("notes.txt → notes.txt"), "{screen}");
    assert!(!screen.contains("index "), "{screen}");

    assert_eq!(fixture.git(&["rev-parse", "HEAD"]).trim(), head);
    assert_eq!(fixture.git(&["show-ref"]), refs);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(fixture.git(&["status", "--porcelain=v2"]), status);
    assert_eq!(
        fs::read(fixture.repo().join("src/main.rs")).unwrap(),
        b"uncommitted edit\n"
    );
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}
