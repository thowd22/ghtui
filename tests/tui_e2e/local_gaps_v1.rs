//! Local Git gaps: launch inside a linked worktree with a paused cherry-pick, Branch changes
//! mode, a paused rebase aborted from its banner row, and guarded deletion, force-with-lease
//! and a conflicting pull. Every effect is read independently from Git.
use crate::{
    local,
    local_management::{fill, operation, refresh, run},
    support::{Fixture, Session},
};
use ghtui::local_actions::LocalCommand;
use std::{fs, path::Path, process::Command};

fn git_in(directory: &Path, args: &[&str]) -> (bool, String) {
    let output = Command::new("git")
        .args(args)
        .current_dir(directory)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_EDITOR", "true")
        .output()
        .unwrap();
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    )
}
fn ok_in(directory: &Path, args: &[&str]) -> String {
    let (success, stdout) = git_in(directory, args);
    assert!(success, "git {args:?} failed in {}", directory.display());
    stdout
}
fn activate(session: &mut Session, mouse: bool, label: &str, keys: &[u8]) {
    if mouse {
        session.click_visible_action(label);
    } else {
        session.send_key(keys);
    }
}

#[test]
fn local_workspace_keyboard_linked_worktree_cherry_pick_branch_mode_and_rebase_abort() {
    modes(false);
}
#[test]
fn local_workspace_mouse_linked_worktree_cherry_pick_branch_mode_and_rebase_abort() {
    modes(true);
}

fn modes(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("file.txt", b"base\n", "base");
    let linked = fixture.root.join("linked-worktree");
    fixture.git(&[
        "worktree",
        "add",
        "-b",
        "picker",
        linked.to_str().unwrap(),
        "main",
    ]);
    fs::write(linked.join("picker.txt"), b"picker branch line\n").unwrap();
    fs::write(linked.join("file.txt"), b"picker\n").unwrap();
    ok_in(&linked, &["add", "picker.txt", "file.txt"]);
    ok_in(&linked, &["commit", "-qm", "Picker work"]);
    fixture.commit_file("file.txt", b"main\n", "main edit");
    let main_edit = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    ok_in(&linked, &["branch", "--set-upstream-to=main"]);
    assert!(!git_in(&linked, &["cherry-pick", &main_edit]).0);
    let head = ok_in(&linked, &["rev-parse", "HEAD"]);
    let refs = fixture.git(&["show-ref"]);
    let status = ok_in(&linked, &["status", "--porcelain=v2", "-z"]);
    let index_path = ok_in(
        &linked,
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
    );
    let index = fs::read(index_path.trim()).unwrap();
    let main_status = fixture.git(&["status", "--porcelain=v2", "-z"]);

    // Launched from inside the linked worktree: its own branch and paused cherry-pick.
    let mut session = Session::launch_from(&fixture, &linked, 120, 40);
    session.wait_for_screen("Branch: picker");
    session.wait_for_screen("⚠ CHERRY-PICK PAUSED • 1 unresolved conflict");
    session.wait_for_screen("[Skip this commit]");
    session.wait_for_screen("[Abort cherry-pick]");
    activate(&mut session, mouse, "[b Branch]", b"b");
    session.wait_for_screen("Changes proposed by this branch");
    session.wait_for_screen("picker branch line");
    session.resize(80, 24);
    activate(&mut session, mouse, "[o Overview]", b"o");
    session.wait_for_screen("CHERRY-PICK PAUSED");
    assert_eq!(ok_in(&linked, &["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.git(&["show-ref"]), refs);
    assert_eq!(ok_in(&linked, &["status", "--porcelain=v2", "-z"]), status);
    assert_eq!(fs::read(index_path.trim()).unwrap(), index);
    assert_eq!(
        fixture.git(&["status", "--porcelain=v2", "-z"]),
        main_status
    );
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);

    // A paused rebase is named; Abort from its banner row (or m) restores the branch.
    let rebase = Fixture::new();
    rebase.init_git();
    rebase.commit_file("file.txt", b"base\n", "base");
    rebase.git(&["switch", "-c", "feature"]);
    rebase.commit_file("file.txt", b"feature\n", "feature edit");
    let feature = rebase.git(&["rev-parse", "HEAD"]);
    rebase.git(&["switch", "main"]);
    rebase.commit_file("file.txt", b"main\n", "main edit");
    rebase.git(&["switch", "feature"]);
    assert!(!git_in(&rebase.repo(), &["rebase", "main"]).0);
    // 120x40 keeps the Abort row inside the Files pane; at 80x24 it is below the fold.
    let mut session = Session::launch(&rebase, 120, 40);
    session.wait_for_screen("⚠ REBASE PAUSED • 1 unresolved conflict");
    if mouse {
        session.click_visible_action("[Abort rebase]");
        session.wait_for_screen("LOCAL INTENT • Preview first");
    } else {
        operation(&mut session, false, LocalCommand::Abort);
    }
    local::preview(&mut session, mouse);
    session.wait_for_screen("rebase --abort");
    assert!(rebase.repo().join(".git/rebase-merge").exists());
    local::confirm(&mut session, mouse, 0);
    session.wait_for_screen("Completed • Abort operation");
    assert!(!rebase.repo().join(".git/rebase-merge").exists());
    assert_eq!(rebase.git(&["branch", "--show-current"]), "feature\n");
    assert_eq!(rebase.git(&["rev-parse", "HEAD"]), feature);
    assert_eq!(
        fs::read(rebase.repo().join("file.txt")).unwrap(),
        b"feature\n"
    );
    assert!(session.inspect_command_journal(&rebase).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}

fn bare_main(bare: &Path) -> String {
    ok_in(bare, &["rev-parse", "refs/heads/main"])
        .trim()
        .to_string()
}

#[test]
fn local_management_keyboard_worktree_owned_delete_tag_lease_and_conflicting_pull() {
    guarded(false);
}
#[test]
fn local_management_mouse_worktree_owned_delete_tag_lease_and_conflicting_pull() {
    guarded(true);
}

fn guarded(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("file.txt", b"saved\n", "initial");
    let bare = fixture.root.join("remote.git");
    assert!(Command::new("git")
        .args(["init", "--bare", "--initial-branch=main"])
        .arg(&bare)
        .output()
        .unwrap()
        .status
        .success());
    fixture.git(&["remote", "add", "origin", bare.to_str().unwrap()]);
    fixture.git(&["push", "-u", "origin", "main"]);
    // The remote gains a commit this checkout replaces; local main gains its own.
    fixture.git(&["switch", "-c", "side"]);
    fixture.commit_file("remote.txt", b"remote side\n", "remote side");
    let remote_head = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    fixture.git(&["push", "origin", "side:main"]);
    fixture.git(&["switch", "main"]);
    fixture.git(&["branch", "-D", "side"]);
    fixture.commit_file("local.txt", b"local side\n", "local side");
    let local_head = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    fixture.git(&["fetch", "origin"]);
    fixture.git(&["branch", "done"]);
    fixture.git(&["tag", "v-old"]);
    let linked = fixture.root.join("linked-worktree");
    fixture.git(&["worktree", "add", "-b", "linked", linked.to_str().unwrap()]);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");

    // A branch owned by another worktree is explained and kept.
    operation(&mut session, mouse, LocalCommand::DeleteBranch);
    fill(
        &mut session,
        mouse,
        &[
            ("Existing branch", "linked"),
            ("Delete unmerged history?", "true"),
        ],
    );
    local::preview(&mut session, mouse);
    session.wait_for_screen("linked is checked out in linked worktree");
    local::confirm(&mut session, mouse, 2);
    session.wait_for_screen("Error: Branch is checked out in this or another worktree");
    assert_eq!(fixture.git(&["rev-parse", "linked"]).trim(), local_head);
    local::close(&mut session, mouse);
    refresh(&mut session, mouse);

    // Ordinary branch and tag deletions complete after exact confirmation.
    run(
        &mut session,
        mouse,
        LocalCommand::DeleteBranch,
        &[
            ("Existing branch", "done"),
            ("Delete unmerged history?", "false"),
        ],
    );
    assert!(
        !git_in(
            &fixture.repo(),
            &["rev-parse", "--verify", "-q", "refs/heads/done"]
        )
        .0
    );
    run(
        &mut session,
        mouse,
        LocalCommand::DeleteTag,
        &[("Existing name", "v-old")],
    );
    assert!(
        !git_in(
            &fixture.repo(),
            &["rev-parse", "--verify", "-q", "refs/tags/v-old"]
        )
        .0
    );

    // Force-with-lease refuses a wrong expected SHA, then replaces exactly the inspected one.
    session.resize(80, 24);
    let lease = [
        ("Configured remote", "origin"),
        ("Local branch", "main"),
        ("Exact expected remote commit SHA", local_head.as_str()),
    ];
    operation(&mut session, mouse, LocalCommand::ForcePushLease);
    fill(&mut session, mouse, &lease);
    if mouse {
        session.click_visible_action("[Preview]");
    } else {
        session.send_key(b"\x10");
    }
    session.wait_for_screen("Remote branch changed since it was inspected");
    assert_eq!(bare_main(&bare), remote_head);
    local::close(&mut session, mouse);
    operation(&mut session, mouse, LocalCommand::ForcePushLease);
    fill(
        &mut session,
        mouse,
        &[
            ("Configured remote", "origin"),
            ("Local branch", "main"),
            ("Exact expected remote commit SHA", remote_head.as_str()),
        ],
    );
    local::preview(&mut session, mouse);
    session.wait_for_screen("REPLACES remote history");
    assert_eq!(bare_main(&bare), remote_head);
    local::confirm(&mut session, mouse, 3);
    session.wait_for_screen("Completed • Force push with exact lease");
    assert_eq!(bare_main(&bare), local_head);

    // A conflicting merge pull stops, explains the paused merge and keeps local work.
    let other = fixture.root.join("other-clone");
    ok_in(
        &fixture.root,
        &[
            "clone",
            "-q",
            bare.to_str().unwrap(),
            other.to_str().unwrap(),
        ],
    );
    fs::write(other.join("file.txt"), b"remote conflicting edit\n").unwrap();
    ok_in(
        &other,
        &[
            "-c",
            "user.name=Other",
            "-c",
            "user.email=o@example.invalid",
            "commit",
            "-qam",
            "remote edit",
        ],
    );
    ok_in(&other, &["push", "-q", "origin", "main"]);
    let pushed = bare_main(&bare);
    fixture.commit_file("file.txt", b"local conflicting edit\n", "local edit");
    let local_edit = fixture.git(&["rev-parse", "HEAD"]);
    refresh(&mut session, mouse);
    operation(&mut session, mouse, LocalCommand::Pull);
    fill(
        &mut session,
        mouse,
        &[
            ("Configured remote", "origin"),
            ("Remote branch", "main"),
            ("Pull strategy", "merge"),
        ],
    );
    local::preview(&mut session, mouse);
    local::confirm(&mut session, mouse, 3);
    session.wait_for_screen("Error: CONFLICT: the operation stopped");
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), local_edit);
    assert!(fixture.repo().join(".git/MERGE_HEAD").exists());
    assert_eq!(bare_main(&bare), pushed);
    local::close(&mut session, mouse);
    refresh(&mut session, mouse);
    session.wait_for_screen("MERGE PAUSED");
    assert!(fs::read_to_string(fixture.repo().join("file.txt"))
        .unwrap()
        .contains("local conflicting edit"));
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}
