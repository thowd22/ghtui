//! Actual local project/subdirectory discovery with unchanged source/index/remotes.
use crate::support::{Fixture, Session};
use std::{path::Path, process::Command};
#[test]
fn project_git_init_keyboard_real_project_and_subdirectory_are_read_only() {
    project(false)
}
#[test]
fn project_git_init_mouse_real_project_and_subdirectory_are_read_only() {
    project(true)
}
fn project(mouse: bool) {
    let fixture = Fixture::new();
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = std::fs::read(directory.join("Cargo.toml")).unwrap();
    let index = std::fs::read(directory.join(".git/index")).ok();
    let remotes = || {
        let output = Command::new("git")
            .arg("-C")
            .arg(directory)
            .args(["remote", "-v"])
            .output()
            .unwrap();
        assert!(output.status.success());
        output.stdout
    };
    let before = remotes();
    let status = ghtui::git::GitRepository::discover(directory)
        .unwrap()
        .unwrap()
        .status()
        .unwrap();
    for start in [directory.to_path_buf(), directory.join("src")] {
        let mut session = Session::launch_from(&fixture, &start, 120, 40);
        session.wait_for_screen("Local Git workspace ready");
        session.wait_for_screen(directory.to_str().unwrap());
        if mouse {
            session.click_visible_action("[t Tree]")
        } else {
            session.send_key(b"t")
        };
        session.wait_for_screen("Repository loaded • read-only exploration");
        session.resize(80, 24);
        if mouse {
            session.click_visible_action("[o Overview]")
        } else {
            session.send_key(b"o")
        };
        session.wait_for_screen(&format!(
            "Branch: {}",
            status.branch.as_deref().unwrap_or("detached HEAD")
        ));
        if status.head.is_none() {
            session.wait_for_screen("saved version: none yet");
        }
        assert!(session.inspect_command_journal(&fixture).is_empty());
        session.quit_and_assert_restored_terminal(mouse);
    }
    assert_eq!(std::fs::read(directory.join("Cargo.toml")).unwrap(), source);
    assert_eq!(std::fs::read(directory.join(".git/index")).ok(), index);
    assert_eq!(remotes(), before);
}
