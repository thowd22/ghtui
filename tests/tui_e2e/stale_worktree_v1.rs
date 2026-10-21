//! A checkout whose core.worktree points at an unrelated directory (left by
//! another tool) explains the setting and its fix instead of a bare error.
use crate::support::{Fixture, Session};

fn journey(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("a.txt", b"saved\n", "First saved version");
    let elsewhere = tempfile::tempdir().unwrap();
    fixture.git(&[
        "config",
        "core.worktree",
        elsewhere.path().to_str().unwrap(),
    ]);
    let mut session = Session::launch(&fixture, 160, 40);
    session.wait_for_screen("ghtui");
    if mouse {
        session.click_visible_action("2 Repository");
    } else {
        session.send_key(b"2");
    }
    // TASK-118: plain words and choices; the unset command shows in its preview.
    session.wait_for_screen("This checkout's Git settings point at another folder");
    session.wait_for_screen(&format!("core.worktree = {}", elsewhere.path().display()));
    session.wait_for_screen("m  Local operations: unset core.worktree (preview first)");
    // Read-only: the setting and the checkout are left exactly as they were.
    assert_eq!(
        fixture.git(&["config", "core.worktree"]).trim_end(),
        elsewhere.path().to_str().unwrap()
    );
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn v1_stale_worktree_keyboard_explains_setting_and_fix() {
    journey(false)
}
#[test]
fn v1_stale_worktree_mouse_explains_setting_and_fix() {
    journey(true)
}
