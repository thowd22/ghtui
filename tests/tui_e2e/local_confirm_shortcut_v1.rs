//! Ctrl-S is the keyboard equivalent of [Confirm action]: same preview-first
//! semantics, no Tab-order guessing. Effects are read back from real Git.
use super::{operation, preview, select_changed};
use crate::support::{Fixture, Session};

fn confirm_shortcut(session: &mut Session, mouse: bool) {
    if mouse {
        session.click_visible_action("[Confirm action]")
    } else {
        session.send_key(b"\x13")
    }
}

fn journey(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("a.txt", b"old\n", "First saved version");
    let head = fixture.git(&["rev-parse", "HEAD"]);
    std::fs::write(fixture.repo().join("a.txt"), b"new\n").unwrap();
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");
    select_changed(&mut session, mouse, "a.txt");
    operation(&mut session, mouse, "Stage selected file", 0);
    session.wait_for_screen("Ctrl-S: confirm");
    preview(&mut session, mouse);
    confirm_shortcut(&mut session, mouse);
    session.wait_for_screen("Completed • Stage selected file");
    assert_eq!(fixture.git(&["show", ":a.txt"]), "new\n");

    operation(&mut session, mouse, "Commit staged changes", 4);
    session.paste("Shortcut commit");
    session.wait_for_screen("Shortcut commit");
    if !mouse {
        // Confirm before any preview is refused, exactly like the button.
        session.send_key(b"\x13");
        session.poll_output(std::time::Duration::from_millis(300));
        assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
        assert!(!session
            .screen()
            .contains("Completed • Commit staged changes"));
    }
    preview(&mut session, mouse);
    session.wait_for_screen("+new");
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    session.resize(80, 24);
    confirm_shortcut(&mut session, mouse);
    session.wait_for_screen("Completed • Commit staged changes");
    assert_eq!(
        fixture
            .git(&["show", "--format=%P", "--no-patch", "HEAD"])
            .trim(),
        head.trim()
    );
    assert_eq!(
        fixture
            .git(&["show", "--format=%s", "--no-patch", "HEAD"])
            .trim(),
        "Shortcut commit"
    );
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn local_confirm_shortcut_keyboard_ctrl_s_stages_and_commits_after_preview() {
    journey(false)
}
#[test]
fn local_confirm_shortcut_mouse_button_stages_and_commits_after_preview() {
    journey(true)
}
