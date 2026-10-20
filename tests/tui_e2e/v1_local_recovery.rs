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
        "conflicted  file.txt",
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
        session.click_visible_action("conflicted  file.txt");
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

#[test]
fn v1_local_recovery_keyboard_conflicted_stash_apply_banner() {
    stash_conflict(false);
}
#[test]
fn v1_local_recovery_mouse_conflicted_stash_apply_banner() {
    stash_conflict(true);
}

fn stash_conflict(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("file.txt", b"base\n", "base");
    fs::write(fixture.repo().join("file.txt"), b"stashed\n").unwrap();
    fixture.git(&["stash", "push", "-m", "parked edit"]);
    let stash = fixture.git(&["rev-parse", "refs/stash"]);
    fixture.commit_file("file.txt", b"committed\n", "committed");
    let apply = Command::new("git")
        .args(["stash", "apply"])
        .current_dir(fixture.repo())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(!apply.status.success(), "fixture stash apply must conflict");
    let mut session = Session::launch(&fixture, 80, 24);
    session.wait_for_screen("Recent commits");
    session.wait_for_screen("⚠ 1 unresolved conflict without a paused operation");
    activate(&mut session, mouse, "conflicted  file.txt", b"\x1b[B\r");
    session.wait_for_screen("Conflict in file.txt");
    session.wait_for_screen("Ours — current branch content");
    assert_eq!(fixture.git(&["rev-parse", "refs/stash"]), stash);
    assert!(fixture
        .git(&["ls-files", "--unmerged"])
        .contains("file.txt"));
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn v1_local_recovery_keyboard_amended_away_commit_rescue_branch() {
    rescue(false);
}
#[test]
fn v1_local_recovery_mouse_amended_away_commit_rescue_branch() {
    rescue(true);
}

fn rescue(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("file.txt", b"base\n", "base");
    fixture.commit_file("file.txt", b"draft\n", "draft work");
    let draft = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    fs::write(fixture.repo().join("file.txt"), b"polished\n").unwrap();
    fixture.git(&["commit", "-q", "--amend", "-am", "polished work"]);
    let head = fixture.git(&["rev-parse", "HEAD"]);
    fs::write(fixture.repo().join("file.txt"), b"important dirty work\n").unwrap();
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");
    session.wait_for_screen("Left behind by amend/reset/rebase");
    let row = format!("↺ {} draft work • before amend", &draft[..8]);
    session.wait_for_screen(&row);
    if mouse {
        session.click_visible_action(&row);
    } else {
        // The first Down focuses the dirty file row; then two recent commits
        // precede the left-behind commit.
        session.send_key(b"\x1b[B");
        session.send_key(&b"\x1b[A".repeat(32));
        for _ in 0..3 {
            session.send_key(b"\x1b[B");
        }
        session.send_key(b"\r");
    }
    // The History detail shows the full SHA beside its `SHA` label (or on its own row).
    session.wait_for_screen(&draft);

    session.resize(80, 24);
    operation(&mut session, mouse, LocalCommand::RecoverBranch);
    session.wait_for_screen("New name:");
    if mouse {
        let (column, line) = session.locate("New name:").unwrap();
        session.click(column, line + 1);
    }
    session.send_key(b"\x15");
    session.paste("rescue-draft");
    session.wait_for_screen("rescue-draft");
    local::preview(&mut session, mouse);
    session.wait_for_screen(&draft[..12]);
    local::confirm(&mut session, mouse, 2);
    session.wait_for_screen("Completed • Create rescue branch");
    assert_eq!(
        fixture.git(&["rev-parse", "rescue-draft"]),
        format!("{draft}\n")
    );
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.git(&["branch", "--show-current"]), "main\n");
    assert_eq!(
        fs::read(fixture.repo().join("file.txt")).unwrap(),
        b"important dirty work\n"
    );
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}
