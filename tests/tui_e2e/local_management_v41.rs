//! TASK-41/44 journeys on the real binary: view and drop an exact stash, see worktree dirty
//! state and open another worktree read-only; bisect dirty refusal, structured candidate
//! progress, skip, an interrupted search resumed from its banner rows and the no-candidate
//! stop. Every Git effect is checked independently of the screen.
use crate::{
    local,
    local_management::{fill, finish, launch, operation, refresh, run},
    support::{Fixture, Session},
};
use ghtui::local_actions::LocalCommand;
use std::{fs, path::Path, process::Command, time::Duration};

/// Mouse clicks the row's own text; keyboard moves the `▌` bar onto it and presses Enter.
fn activate_row(session: &mut Session, mouse: bool, text: &str) {
    session.wait_for_screen(text);
    if mouse {
        let (column, row) = session.locate(text).unwrap();
        session.click(column, row);
        return;
    }
    session.send_key(&b"\x1b[A".repeat(48));
    for _ in 0..64 {
        session.poll_output(Duration::from_millis(120));
        if session
            .screen()
            .lines()
            .any(|line| line.contains(text) && line.contains('▌'))
        {
            session.send_key(b"\r");
            return;
        }
        session.send_key(b"\x1b[B");
    }
    panic!(
        "row {text:?} never took the selection\n{}",
        session.screen()
    );
}

fn git_in(directory: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "{args:?}");
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn stash_worktrees_keyboard_view_drop_stash_and_open_dirty_worktree() {
    stash_view_and_worktree(false);
}
#[test]
fn stash_worktrees_mouse_view_drop_stash_and_open_dirty_worktree() {
    stash_view_and_worktree(true);
}
fn stash_view_and_worktree(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("file.txt", b"saved\n", "saved");
    let head = fixture.git(&["rev-parse", "HEAD"]);
    fixture.git(&["branch", "linked"]);
    let linked = fixture.root.join("linked-worktree");
    fixture.git(&["worktree", "add", linked.to_str().unwrap(), "linked"]);
    fs::write(linked.join("file.txt"), b"linked dirty work\n").unwrap();
    fs::write(fixture.repo().join("file.txt"), b"tracked work\n").unwrap();
    fs::write(fixture.repo().join("new.txt"), b"untracked work\n").unwrap();
    let mut session = launch(&fixture);
    session.wait_for_screen("⎇ linked worktree • dirty: uncommitted changes");
    run(
        &mut session,
        mouse,
        LocalCommand::StashSave,
        &[
            ("Stash description", "view me"),
            ("Include untracked files?", "true"),
        ],
    );
    let id = fixture.git(&["rev-parse", "refs/stash"]).trim().to_string();
    assert!(!fixture.repo().join("new.txt").exists());

    // View the exact stash's contents in the read-only preview, then close without effect.
    operation(&mut session, mouse, LocalCommand::StashDrop);
    fill(&mut session, mouse, &[("Exact stash commit SHA", &id)]);
    local::preview(&mut session, mouse);
    session.wait_for_screen(&format!("Exact stash commit: {id}"));
    session.wait_for_screen("Stash contents (read-only view of this exact stash):");
    session.wait_for_screen("+tracked work");
    session.wait_for_screen("Untracked files saved in this stash: 1");
    session.wait_for_screen("untracked new.txt");
    local::close(&mut session, mouse);
    assert_eq!(fixture.git(&["rev-parse", "refs/stash"]), format!("{id}\n"));

    // Drop the exact stash after confirmation; working files are untouched.
    run(
        &mut session,
        mouse,
        LocalCommand::StashDrop,
        &[("Exact stash commit SHA", &id)],
    );
    assert_eq!(fixture.git(&["stash", "list"]), "");
    assert_eq!(
        fs::read(fixture.repo().join("file.txt")).unwrap(),
        b"saved\n"
    );

    // Open the dirty linked worktree read-only from its Overview row.
    refresh(&mut session, mouse);
    activate_row(
        &mut session,
        mouse,
        "⎇ linked worktree • dirty: uncommitted changes",
    );
    session.wait_for_screen("Repository loaded • read-only exploration");
    session.wait_for_screen("⎇ main worktree • clean • Enter: open");
    session.wait_for_screen("file.txt");
    assert_eq!(
        fs::read(linked.join("file.txt")).unwrap(),
        b"linked dirty work\n"
    );
    assert_eq!(git_in(&linked, &["status", "--porcelain"]), " M file.txt\n");
    assert_eq!(git_in(&linked, &["branch", "--show-current"]), "linked\n");
    // And back to the main worktree.
    activate_row(&mut session, mouse, "⎇ main worktree • clean • Enter: open");
    session.wait_for_screen("⎇ linked worktree • dirty: uncommitted changes");
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    assert_eq!(git_in(&linked, &["status", "--porcelain"]), " M file.txt\n");
    finish(&mut session, &fixture, mouse);
}

#[test]
fn guided_bisect_keyboard_dirty_refusal_skip_interrupted_and_no_candidate() {
    bisect_recovery(false);
}
#[test]
fn guided_bisect_mouse_dirty_refusal_skip_interrupted_and_no_candidate() {
    bisect_recovery(true);
}
fn bisect_recovery(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    let mut commits = vec![];
    for index in 0..5 {
        fixture.commit_file(
            "version.txt",
            format!("version {index}\n").as_bytes(),
            &format!("version {index}"),
        );
        commits.push(fixture.git(&["rev-parse", "HEAD"]).trim().to_string());
    }
    let start = fixture.git(&["rev-parse", "HEAD"]);
    let bounds = [
        ("Known good saved version", commits[0].as_str()),
        ("Known bad saved version", commits[4].as_str()),
    ];
    fs::write(fixture.repo().join("version.txt"), b"dirty work\n").unwrap();
    let mut session = launch(&fixture);
    operation(&mut session, mouse, LocalCommand::BisectStart);
    fill(&mut session, mouse, &bounds);
    if mouse {
        session.click_visible_action("[Preview]");
    } else {
        session.send_key(b"\x10");
    }
    session.wait_for_screen("Save or stash local changes");
    assert!(!fixture.repo().join(".git/BISECT_LOG").exists());
    assert_eq!(
        fs::read(fixture.repo().join("version.txt")).unwrap(),
        b"dirty work\n"
    );
    local::close(&mut session, mouse);
    fs::write(fixture.repo().join("version.txt"), b"version 4\n").unwrap();
    refresh(&mut session, mouse);

    run(&mut session, mouse, LocalCommand::BisectStart, &bounds);
    let candidate = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    session.wait_for_screen(&format!("Candidate: {candidate}"));
    session.wait_for_screen("left to test after this");
    run(&mut session, mouse, LocalCommand::BisectSkip, &[]);
    assert!(fixture.git(&["bisect", "log"]).contains("git bisect skip"));
    let candidate = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    session.wait_for_screen(&format!("Candidate: {candidate}"));
    finish(&mut session, &fixture, mouse);

    // Interrupted: a new session resumes from the banner rows Git's bisect state keeps.
    let mut session = launch(&fixture);
    session.wait_for_screen("Bug search in progress");
    session.wait_for_screen("[End bug search]");
    let mut stopped = false;
    for _ in 0..4 {
        activate_row(&mut session, mouse, "[Skip untestable version]");
        session.wait_for_screen("LOCAL INTENT • Preview first");
        local::preview(&mut session, mouse);
        local::confirm(&mut session, mouse, 0);
        if session
            .try_wait_for("No feasible candidate", Duration::from_secs(4))
            .is_ok()
        {
            stopped = true;
            break;
        }
        session.wait_for_screen("Completed • Skip untestable version");
    }
    assert!(
        stopped,
        "skipping every candidate must stop\n{}",
        session.screen()
    );
    assert!(fixture.repo().join(".git/BISECT_LOG").exists());
    local::close(&mut session, mouse);
    refresh(&mut session, mouse);
    session.resize(80, 24);
    run(&mut session, mouse, LocalCommand::BisectReset, &[]);
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), start);
    assert_eq!(fixture.git(&["branch", "--show-current"]), "main\n");
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    assert!(!fixture.repo().join(".git/BISECT_LOG").exists());
    finish(&mut session, &fixture, mouse);
}
