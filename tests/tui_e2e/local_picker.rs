//! Real keyboard/mouse inventory selection: no labels are converted into command targets.
use crate::{
    local, local_management,
    support::{Fixture, Session},
};
use ghtui::local_actions::LocalCommand;
use std::{fs, os::unix::ffi::OsStrExt, process::Command, time::Duration};

#[test]
fn local_picker_keyboard_cancel_nomatch_stale_ref_immutable_stash_and_raw_worktree() {
    journey(false);
}

#[test]
fn local_picker_mouse_cancel_nomatch_stale_ref_immutable_stash_and_raw_worktree() {
    journey(true);
}

fn open(session: &mut Session, mouse: bool, field: &str) {
    if mouse {
        session.click_visible_action("[F4 Choose]");
    } else {
        session.send_key(b"\x1bOS");
    }
    session.wait_for_screen(&format!("Choose {field} • local inventory"));
}

fn filter(session: &mut Session, mouse: bool, query: &str) {
    if mouse {
        session.click_visible_action("[Ctrl-U Clear]");
    } else {
        session.send_key(b"\x15");
    }
    session.paste(query);
    session.wait_for_screen(&format!("Filter: {query}"));
}

fn choose(session: &mut Session, mouse: bool, field: &str, label: &str) {
    session.wait_for_screen(label);
    if mouse {
        session.click_visible_action(label);
    } else {
        session.send_key(b"\r");
    }
    let title = format!("Choose {field} • local inventory");
    session
        .try_wait_for_condition(
            |screen| !screen.contains(&title),
            "chosen picker to close",
            Duration::from_secs(8),
        )
        .unwrap();
}

fn preview_request(session: &mut Session, mouse: bool) {
    if mouse {
        session.click_visible_action("[Preview]");
    } else {
        session.send_key(b"\x10");
    }
}

fn journey(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("file.txt", b"saved bytes\n", "initial saved version");
    fixture.git(&["branch", "review"]);
    fixture.git(&["branch", "linked"]);
    fixture.git(&["branch", "linked-valid"]);
    fs::write(fixture.repo().join("file.txt"), b"stash bytes\n").unwrap();
    fixture.git(&["stash", "push", "-m", "picker stash"]);
    let stash = fixture.git(&["rev-parse", "refs/stash"]);
    let raw = fixture
        .root
        .join(crate::support::exact_worktree_component());
    let unicode = fixture.root.join("linked-�");
    for (path, branch) in [(&raw, "linked"), (&unicode, "linked-valid")] {
        let result = Command::new("git")
            .arg("-C")
            .arg(fixture.repo())
            .args(["worktree", "add", "--"])
            .arg(path)
            .arg(branch)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    fs::write(
        unicode.join("file.txt"),
        b"valid Unicode worktree must survive\n",
    )
    .unwrap();
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");
    local_management::operation(&mut session, mouse, LocalCommand::SwitchBranch);
    open(&mut session, mouse, "name");
    if mouse {
        session.close_overlay_by_mouse("Choose name • local inventory");
    } else {
        session.close_overlay_by_key("Choose name • local inventory");
    }
    assert_eq!(fixture.git(&["branch", "--show-current"]), "main\n");
    open(&mut session, mouse, "name");
    filter(&mut session, mouse, "no-target-xyz");
    session.wait_for_screen("No matching targets. Scope was not broadened.");
    assert_eq!(fixture.git(&["branch", "--show-current"]), "main\n");
    filter(&mut session, mouse, "review");
    choose(&mut session, mouse, "name", "refs/heads/review");
    session
        .try_wait_for_condition(
            |screen| {
                screen
                    .lines()
                    .skip_while(|line| !line.contains("Existing name:"))
                    .nth(1)
                    .is_some_and(|line| line.contains("review"))
            },
            "chosen branch value in actual field",
            Duration::from_secs(8),
        )
        .unwrap();
    fixture.commit_file(
        "another.txt",
        b"external saved bytes\n",
        "external ref update",
    );
    fixture.git(&["branch", "-f", "review", "HEAD"]);
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    preview_request(&mut session, mouse);
    session.wait_for_screen("Selected reference changed");
    assert_eq!(fixture.git(&["branch", "--show-current"]), "main\n");
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    open(&mut session, mouse, "name");
    filter(&mut session, mouse, "review");
    choose(&mut session, mouse, "name", "refs/heads/review");
    local::preview(&mut session, mouse);
    local::confirm(&mut session, mouse, 1);
    session.wait_for_screen("Completed • Switch branch");
    assert_eq!(fixture.git(&["branch", "--show-current"]), "review\n");
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    local_management::operation(&mut session, mouse, LocalCommand::StashApply);
    open(&mut session, mouse, "id");
    filter(&mut session, mouse, "picker stash");
    choose(&mut session, mouse, "id", "stash@{0}");
    local::preview(&mut session, mouse);
    session.wait_for_screen(stash.trim());
    local::confirm(&mut session, mouse, 1);
    session.wait_for_screen(&format!("Completed • {}", LocalCommand::StashApply.label()));
    assert_eq!(
        fs::read(fixture.repo().join("file.txt")).unwrap(),
        b"stash bytes\n"
    );
    assert_eq!(fixture.git(&["rev-parse", "refs/stash"]), stash);
    session.resize(80, 24);
    local_management::operation(&mut session, mouse, LocalCommand::RemoveWorktree);
    open(&mut session, mouse, "path");
    let raw_label = crate::support::exact_worktree_label();
    filter(&mut session, mouse, raw_label);
    let exact_path_label = ghtui::git::RawPath::new(raw.as_os_str().as_bytes().to_vec()).display();
    choose(&mut session, mouse, "path", &exact_path_label);
    local::preview(&mut session, mouse);
    session.wait_for_screen("Exact worktree directory:");
    session.wait_for_screen(raw_label);
    assert!(raw.exists());
    assert_eq!(
        fs::read(unicode.join("file.txt")).unwrap(),
        b"valid Unicode worktree must survive\n"
    );
    local::confirm(&mut session, mouse, 1);
    session.wait_for_screen("Completed • Remove linked worktree");
    assert!(!raw.exists());
    assert_eq!(
        fs::read(unicode.join("file.txt")).unwrap(),
        b"valid Unicode worktree must survive\n"
    );
    assert_eq!(
        fs::read(fixture.repo().join("file.txt")).unwrap(),
        b"stash bytes\n"
    );
    assert_eq!(
        fs::read(fixture.repo().join("another.txt")).unwrap(),
        b"external saved bytes\n"
    );
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.git(&["branch", "--show-current"]), "review\n");
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}
