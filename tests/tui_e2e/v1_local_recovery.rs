//! Paused merge guidance: the Repository overview names the paused operation,
//! refuses Continue while conflicts remain, shows the working file with
//! markers, resolves by named version and continues from the banner rows.
use crate::{
    local,
    local_management::operation,
    support::{Fixture, Session},
};
use ghtui::local_actions::LocalCommand;
use std::{fs, process::Command};

const VERSION: &str =
    "Keep version (ours = current branch, theirs = incoming, base = common ancestor)";

fn paused_merge(fixture: &Fixture) -> (String, String) {
    fixture.init_git();
    fixture.commit_file("file.txt", b"base\n", "base");
    fixture.git(&["switch", "-c", "feature"]);
    fixture.commit_file("file.txt", b"theirs\n", "theirs");
    let theirs = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    fixture.git(&["switch", "main"]);
    fixture.commit_file("file.txt", b"ours\n", "ours");
    let ours = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    fs::write(fixture.repo().join("parked.txt"), b"parked\n").unwrap();
    fixture.git(&["stash", "push", "-u", "-m", "parked work"]);
    let merge = Command::new("git")
        .args(["merge", "feature"])
        .current_dir(fixture.repo())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(!merge.status.success(), "fixture merge must conflict");
    (ours, theirs)
}

fn activate(session: &mut Session, mouse: bool, label: &str, keys: &[u8]) {
    if mouse {
        session.click_visible_action(label);
    } else {
        session.send_key(keys);
    }
}

#[test]
fn v1_local_recovery_keyboard_paused_merge_banner_resolve_and_continue() {
    journey(false);
}
#[test]
fn v1_local_recovery_mouse_paused_merge_banner_resolve_and_continue() {
    journey(true);
}

fn journey(mouse: bool) {
    let fixture = Fixture::new();
    let (ours, theirs) = paused_merge(&fixture);
    let stash = fixture.git(&["rev-parse", "refs/stash"]);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");
    session.wait_for_screen("⚠ MERGE PAUSED • 1 unresolved conflict");
    session.wait_for_screen("Saved stashes: 1 • newest stash@{0}");
    session.wait_for_screen("[Abort merge]");

    // Continue from the banner row is refused while the conflict is unresolved.
    activate(&mut session, mouse, "[Continue merge]", b"\x1b[B\x1b[B\r");
    session.wait_for_screen("Resolve and stage 1 conflicted file first.");
    assert!(fixture.repo().join(".git/MERGE_HEAD").exists());

    // Inspect the conflict, including the actual working bytes with markers.
    activate(
        &mut session,
        mouse,
        "[conflicted] file.txt",
        &[b"\x1b[A".repeat(32), b"\r".to_vec()].concat(),
    );
    session.wait_for_screen("Conflict in file.txt");
    session.wait_for_screen("Working file now");
    session.wait_for_screen("<<<<<<< HEAD");

    operation(&mut session, mouse, LocalCommand::ResolveConflict);
    session.wait_for_screen(&format!("{VERSION}:"));
    if mouse {
        let (column, row) = session.locate(&format!("{VERSION}:")).unwrap();
        session.click(column, row + 1);
    }
    session.send_key(b"\x15");
    session.paste("theirs");
    local::preview(&mut session, mouse);
    session.wait_for_screen("--stage=3");
    local::confirm(&mut session, mouse, 1);
    session.wait_for_screen("Completed • Choose conflict version");
    assert_eq!(
        fs::read(fixture.repo().join("file.txt")).unwrap(),
        b"theirs\n"
    );
    assert!(fixture
        .git(&["ls-files", "--unmerged"])
        .contains("file.txt"));

    // Stage the resolved file from its overview row.
    if mouse {
        session.click_visible_action("[conflicted] file.txt");
        session.wait_for_screen("Conflict in file.txt");
    } else {
        session.send_key(&b"\x1b[A".repeat(32));
    }
    operation(&mut session, mouse, LocalCommand::StageFile);
    local::preview(&mut session, mouse);
    local::confirm(&mut session, mouse, 0);
    session.wait_for_screen("Completed • Stage selected file");
    assert_eq!(fixture.git(&["ls-files", "--unmerged"]), "");
    session.wait_for_screen("⚠ MERGE PAUSED • all conflicts resolved and staged");

    // Continue from the banner row at the compact size.
    session.resize(80, 24);
    session.wait_for_screen("[Continue merge]");
    activate(
        &mut session,
        mouse,
        "[Continue merge]",
        &[b"\x1b[A".repeat(32), b"\x1b[B\r".to_vec()].concat(),
    );
    session.wait_for_screen("LOCAL INTENT • Preview first");
    local::preview(&mut session, mouse);
    session.wait_for_screen("merge --continue");
    local::confirm(&mut session, mouse, 0);
    session.wait_for_screen("Completed • Continue operation");
    assert_eq!(fixture.git(&["rev-parse", "HEAD^1"]), format!("{ours}\n"));
    assert_eq!(fixture.git(&["rev-parse", "HEAD^2"]), format!("{theirs}\n"));
    assert!(!fixture.repo().join(".git/MERGE_HEAD").exists());
    assert_eq!(fixture.git(&["rev-parse", "refs/stash"]), stash);
    assert!(!session.screen().contains("MERGE PAUSED"));
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}
