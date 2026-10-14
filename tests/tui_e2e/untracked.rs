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
    local::select_changed(&mut session, mouse, name);
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
