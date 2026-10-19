//! New-file staging discloses actual bytes before a write and leaves other new files alone.
use crate::{
    local,
    support::{Fixture, Session},
};
use std::fs;

#[test]
fn staging_keyboard_new_file_content_cancel_stage_unstage_and_preserve_other_new_files() {
    journey(false);
}

#[test]
fn staging_mouse_new_file_content_cancel_stage_unstage_and_preserve_other_new_files() {
    journey(true);
}

/// The Files panel collapses new files to a count: show them through the panel's untracked row
/// (mouse) or `u`, then put the selection bar on the exact new file (its click target, or Up
/// to the panel's Branch row and Down onto the file).
fn select_new_file(session: &mut Session, mouse: bool, name: &str) {
    session.wait_for_screen("untracked: 2 hidden");
    if mouse {
        session.click_visible_action("untracked: 2 hidden");
    } else {
        session.send_key(b"u");
    }
    session.wait_for_screen("Showing 2 untracked files");
    if mouse {
        local::select_changed(session, mouse, name);
        return;
    }
    let selected = |screen: &str| {
        screen
            .lines()
            .any(|line| line.contains("▌ ") && line.contains(name))
    };
    // Up to the Branch row at the top of the Files panel, then Down onto the new file.
    for _ in 0..6 {
        if session
            .try_wait_for_condition(
                |screen| screen.lines().any(|line| line.contains("▌ Branch: main")),
                "Selection bar on the Branch row",
                std::time::Duration::from_secs(1),
            )
            .is_ok()
        {
            break;
        }
        session.send_key(b"\x1b[A");
    }
    for _ in 0..6 {
        if session
            .try_wait_for_condition(
                selected,
                "Selection bar on the exact new file row",
                std::time::Duration::from_secs(1),
            )
            .is_ok()
        {
            return;
        }
        session.send_key(b"\x1b[B");
    }
    panic!(
        "Selection bar never reached the exact new file row\n{}",
        session.screen()
    );
}

fn journey(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("saved.txt", b"saved bytes\n", "saved version");
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let name = "new 日本語.txt";
    fs::write(fixture.repo().join(name), b"new file bytes\nsecond line\n").unwrap();
    fs::write(fixture.repo().join("unrelated.bin"), b"unrelated\0bytes").unwrap();
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");
    select_new_file(&mut session, mouse, name);
    local::operation(&mut session, mouse, "Stage selected file", 0);
    local::preview(&mut session, mouse);
    session.wait_for_screen("New untracked file:");
    session.wait_for_screen("new file bytes");
    session.wait_for_screen("second line");
    assert_eq!(fixture.git(&["ls-files", "--", name]), "");
    local::close(&mut session, mouse);
    assert_eq!(fixture.git(&["ls-files", "--", name]), "");
    local::operation(&mut session, mouse, "Stage selected file", 0);
    local::preview(&mut session, mouse);
    local::confirm(&mut session, mouse, 0);
    session.wait_for_screen("Completed • Stage selected file");
    assert_eq!(
        fixture.git(&["show", &format!(":{name}")]),
        "new file bytes\nsecond line\n"
    );
    assert_eq!(fixture.git(&["ls-files", "--", "unrelated.bin"]), "");
    session.resize(80, 24);
    local::operation(&mut session, mouse, "Unstage selected file", 1);
    local::preview(&mut session, mouse);
    local::confirm(&mut session, mouse, 0);
    session.wait_for_screen("Completed • Unstage selected file");
    assert_eq!(fixture.git(&["ls-files", "--", name]), "");
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(
        fs::read(fixture.repo().join(name)).unwrap(),
        b"new file bytes\nsecond line\n"
    );
    assert_eq!(
        fs::read(fixture.repo().join("unrelated.bin")).unwrap(),
        b"unrelated\0bytes"
    );
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}
