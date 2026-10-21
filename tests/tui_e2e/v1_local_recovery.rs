//! Paused merge guidance: the Repository overview names the paused operation,
//! refuses Continue while conflicts remain, shows the working file with
//! markers, resolves by named version and continues from the banner rows.
use crate::{
    local,
    local_management::{fill, operation, refresh},
    local_management_v41::activate_row,
    support::{Fixture, Session},
};

/// The Overview entry that shows the commits the reflogs can recover (TASK-119).
const RECOVERY_ENTRY: &str = "left behind by amend, reset, rebase or branch moves";
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
    session.wait_for_screen("1 stash · newest stash@{0}");
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
    // TASK-119: the recovery list sits behind its own entry below Recent commits.
    activate_row(&mut session, mouse, RECOVERY_ENTRY);
    session.wait_for_screen("From local reflogs (HEAD and branches)");
    let row = format!("↺ {} draft work • before amend", &draft[..8]);
    activate_row(&mut session, mouse, &row);
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

fn fixture_git_fails(fixture: &Fixture, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(fixture.repo())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(!output.status.success(), "fixture {args:?} must stop");
}

/// Selects the nth (0-based) actionable Overview row by keyboard, or clicks
/// the row label by mouse. `open` also presses Enter on the keyboard path.
fn select_row(session: &mut Session, mouse: bool, label: &str, index: usize, open: bool) {
    if mouse {
        session.click_visible_action(label);
    } else {
        // The first Down focuses the rows pane; Up then reaches the first row.
        session.send_key(b"\x1b[B");
        session.send_key(&b"\x1b[A".repeat(32));
        for _ in 0..index {
            session.send_key(b"\x1b[B");
        }
        if open {
            session.send_key(b"\r");
        }
    }
}

fn choose_version(session: &mut Session, mouse: bool, version: &str, stage: &str) {
    operation(session, mouse, LocalCommand::ResolveConflict);
    session.wait_for_screen(&format!("{VERSION}:"));
    if mouse {
        let (column, row) = session.locate(&format!("{VERSION}:")).unwrap();
        session.click(column, row + 1);
    }
    session.send_key(b"\x15");
    session.paste(version);
    local::preview(session, mouse);
    session.wait_for_screen(&format!("--stage={stage}"));
    local::confirm(session, mouse, 1);
    session.wait_for_screen("Completed • Choose conflict version");
}

fn stage_selected(session: &mut Session, mouse: bool) {
    operation(session, mouse, LocalCommand::StageFile);
    local::preview(session, mouse);
    local::confirm(session, mouse, 0);
    session.wait_for_screen("Completed • Stage selected file");
}

#[test]
fn v1_local_recovery_keyboard_rename_delete_and_file_directory_conflicts() {
    structural(false);
}
#[test]
fn v1_local_recovery_mouse_rename_delete_and_file_directory_conflicts() {
    structural(true);
}

fn structural(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("old.txt", b"a\nb\nc\nd\n", "base old");
    fixture.commit_file("thing", b"x\n", "base thing");
    fixture.git(&["switch", "-c", "feature"]);
    fixture.git(&["mv", "old.txt", "new.txt"]);
    fixture.git(&["rm", "-q", "thing"]);
    fixture.commit_file("thing/inner", b"y\n", "feature: rename and directory");
    let theirs = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    fixture.git(&["switch", "main"]);
    fixture.git(&["rm", "-q", "old.txt"]);
    fixture.commit_file("thing", b"changed\n", "main: delete and change");
    let ours = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    fixture_git_fails(&fixture, &["merge", "feature"]);

    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");
    session.wait_for_screen("⚠ MERGE PAUSED • 2 unresolved conflicts");
    // Overview rows in status order: thing, thing/inner, new.txt, thing~HEAD.
    select_row(&mut session, mouse, "conflicted  new.txt", 2, true);
    session.wait_for_screen("Conflict in new.txt");
    session.wait_for_screen("Delete conflict: deleted on our side");
    session.wait_for_screen("No ours version");

    // Keep the renamed file: their version, then stage it.
    choose_version(&mut session, mouse, "theirs", "3");
    assert_eq!(
        fs::read(fixture.repo().join("new.txt")).unwrap(),
        b"a\nb\nc\nd\n"
    );
    select_row(&mut session, mouse, "conflicted  new.txt", 2, false);
    if mouse {
        session.wait_for_screen("Conflict in new.txt");
    }
    stage_selected(&mut session, mouse);
    assert!(!fixture.git(&["ls-files", "--unmerged"]).contains("new.txt"));
    session.wait_for_screen("⚠ MERGE PAUSED • 1 unresolved conflict");

    // The file Git moved aside for the directory: now new.txt, thing, thing/inner, thing~HEAD.
    select_row(&mut session, mouse, "conflicted  thing~HEAD", 3, true);
    session.wait_for_screen("Conflict in thing~HEAD");
    session.wait_for_screen("File/directory conflict: a directory now occupies thing");
    session.wait_for_screen("No theirs version");
    choose_version(&mut session, mouse, "ours", "2");
    assert_eq!(
        fs::read(fixture.repo().join("thing~HEAD")).unwrap(),
        b"changed\n"
    );
    select_row(&mut session, mouse, "conflicted  thing~HEAD", 3, false);
    if mouse {
        session.wait_for_screen("Conflict in thing~HEAD");
    }
    stage_selected(&mut session, mouse);
    assert_eq!(fixture.git(&["ls-files", "--unmerged"]), "");
    session.wait_for_screen("⚠ MERGE PAUSED • all conflicts resolved and staged");

    operation(&mut session, mouse, LocalCommand::Continue);
    local::preview(&mut session, mouse);
    session.wait_for_screen("merge --continue");
    local::confirm(&mut session, mouse, 0);
    session.wait_for_screen("Completed • Continue operation");
    assert_eq!(fixture.git(&["rev-parse", "HEAD^1"]), format!("{ours}\n"));
    assert_eq!(fixture.git(&["rev-parse", "HEAD^2"]), format!("{theirs}\n"));
    assert_eq!(
        fixture.git(&["ls-tree", "-r", "--name-only", "HEAD"]),
        "new.txt\nthing/inner\nthing~HEAD\n"
    );
    assert!(!fixture.repo().join(".git/MERGE_HEAD").exists());
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn v1_local_recovery_keyboard_merge_preview_refuses_change_made_while_open() {
    race(false);
}
#[test]
fn v1_local_recovery_mouse_merge_preview_refuses_change_made_while_open() {
    race(true);
}

fn race(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("notes.txt", b"saved notes\n", "base");
    fixture.git(&["switch", "-c", "feature"]);
    fixture.commit_file("feature.txt", b"feature\n", "feature work");
    let feature = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    fixture.git(&["switch", "main"]);
    fixture.commit_file("main.txt", b"main\n", "main work");
    let head = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();

    let mut session = Session::launch(&fixture, 80, 24);
    session.wait_for_screen("Recent commits");
    operation(&mut session, mouse, LocalCommand::Merge);
    fill(&mut session, mouse, &[("Saved version", &feature)]);
    local::preview(&mut session, mouse);
    // Another tool edits a tracked file while the preview is on screen.
    fs::write(fixture.repo().join("notes.txt"), b"edited while open\n").unwrap();
    local::confirm(&mut session, mouse, 1);
    session.wait_for_screen("Repository changed since the preview");
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), format!("{head}\n"));
    assert!(!fixture.repo().join(".git/MERGE_HEAD").exists());
    assert_eq!(
        fs::read(fixture.repo().join("notes.txt")).unwrap(),
        b"edited while open\n"
    );
    local::close(&mut session, mouse);

    // A fresh preview of the new, dirty state is refused explicitly.
    refresh(&mut session, mouse);
    session.wait_for_screen("notes.txt");
    operation(&mut session, mouse, LocalCommand::Merge);
    fill(&mut session, mouse, &[("Saved version", &feature)]);
    if mouse {
        session.click_visible_action("[Preview]");
    } else {
        session.send_key(b"\x10");
    }
    session.wait_for_screen("Save or stash local changes before changing history");
    assert!(!session.screen().contains("READ-ONLY PREVIEW"));
    local::close(&mut session, mouse);
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), format!("{head}\n"));
    assert!(!fixture.repo().join(".git/MERGE_HEAD").exists());
    assert_eq!(
        fs::read(fixture.repo().join("notes.txt")).unwrap(),
        b"edited while open\n"
    );
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn v1_local_recovery_keyboard_pruned_and_branch_moved_reflog_rows() {
    reflog_limits(false);
}
#[test]
fn v1_local_recovery_mouse_pruned_and_branch_moved_reflog_rows() {
    reflog_limits(true);
}

fn reflog_limits(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("file.txt", b"base\n", "base");
    fixture.commit_file("file.txt", b"draft\n", "draft work");
    let draft = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    fs::write(fixture.repo().join("file.txt"), b"polished\n").unwrap();
    fixture.git(&["commit", "-q", "--amend", "-am", "polished work"]);
    fixture.git(&["switch", "-c", "feature"]);
    fixture.commit_file("feature.txt", b"feature\n", "feature tip");
    let tip = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    fixture.git(&["switch", "main"]);
    fixture.git(&["branch", "-f", "feature", "main"]);
    fs::remove_file(
        fixture
            .repo()
            .join(".git/objects")
            .join(&draft[..2])
            .join(&draft[2..]),
    )
    .unwrap();
    let head = fixture.git(&["rev-parse", "HEAD"]);

    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");
    activate_row(&mut session, mouse, RECOVERY_ENTRY);
    session.wait_for_screen("Pruned commits are gone and cannot be recovered");
    session.wait_for_screen(&format!("✗ {} draft work • before amend", &draft[..8]));
    session.wait_for_screen("pruned: commit is gone, cannot recover");
    let row = format!(
        "↺ {} feature tip • before branch move • feature@{{0}}",
        &tip[..8]
    );
    // The pruned row has no action: the keyboard skips it.
    activate_row(&mut session, mouse, &row);
    session.wait_for_screen(&tip);

    session.resize(80, 24);
    operation(&mut session, mouse, LocalCommand::RecoverBranch);
    session.wait_for_screen("New name:");
    if mouse {
        let (column, line) = session.locate("New name:").unwrap();
        session.click(column, line + 1);
    }
    session.send_key(b"\x15");
    session.paste("rescue-feature");
    session.wait_for_screen("rescue-feature");
    local::preview(&mut session, mouse);
    session.wait_for_screen(&tip[..12]);
    local::confirm(&mut session, mouse, 2);
    session.wait_for_screen("Completed • Create rescue branch");
    assert_eq!(
        fixture.git(&["rev-parse", "rescue-feature"]),
        format!("{tip}\n")
    );
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.git(&["branch", "--show-current"]), "main\n");
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}

const BLOCK: &str = "Conflict block number (Block N)";
const KEEP: &str = "Keep (ours, theirs, both, base, edited)";
const TEXT: &str = "Edited lines (keep = edited)";

#[test]
fn v1_local_recovery_keyboard_resolve_conflict_blocks_one_at_a_time() {
    conflict_blocks(false);
}
#[test]
fn v1_local_recovery_mouse_resolve_conflict_blocks_one_at_a_time() {
    conflict_blocks(true);
}

/// TASK-42 AC2: two conflict blocks in one file are resolved separately, one
/// keeping theirs and one with lines typed in ghtui. The index stays unmerged
/// until the explicit Stage, and the merge then continues with those bytes.
fn conflict_blocks(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    let body = |first: &str, last: &str| {
        format!("{first}\nkeep one\nkeep two\nkeep three\nkeep four\nkeep five\n{last}\n")
    };
    fixture.commit_file("file.txt", body("one", "two").as_bytes(), "base");
    fixture.git(&["switch", "-c", "feature"]);
    fixture.commit_file(
        "file.txt",
        body("one theirs", "two theirs").as_bytes(),
        "theirs",
    );
    let theirs = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    fixture.git(&["switch", "main"]);
    fixture.commit_file("file.txt", body("one ours", "two ours").as_bytes(), "ours");
    let ours = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    fixture_git_fails(&fixture, &["merge", "feature"]);
    let file = fixture.repo().join("file.txt");
    let unmerged = fixture.git(&["ls-files", "--unmerged"]);

    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");
    session.wait_for_screen("⚠ MERGE PAUSED • 1 unresolved conflict");
    select_row(&mut session, mouse, "conflicted  file.txt", 0, true);
    session.wait_for_screen("Conflict in file.txt");
    session.wait_for_screen("Conflict blocks: 2 • m → Resolve one conflict block");
    session.wait_for_screen("Block 1 of 2 (lines 1–5): ours 1 line • theirs 1 line");
    session.wait_for_screen("Block 2 of 2 (lines 11–15): ours 1 line • theirs 1 line");

    // Block 2 keeps the incoming lines; block 1 stays as it is.
    operation(&mut session, mouse, LocalCommand::ResolveConflictBlock);
    fill(&mut session, mouse, &[(BLOCK, "2"), (KEEP, "theirs")]);
    local::preview(&mut session, mouse);
    session.wait_for_screen("Git effect: block 2 → theirs in file.txt");
    session.wait_for_screen("result │ two theirs");
    session.wait_for_screen("1 conflict block remain unresolved after this.");
    assert!(fs::read_to_string(&file).unwrap().contains("two ours"));
    local::confirm(&mut session, mouse, 3);
    session.wait_for_screen("Completed • Resolve one conflict block");
    let after = fs::read_to_string(&file).unwrap();
    assert!(
        after.starts_with("<<<<<<< HEAD\none ours\n=======\none theirs\n>>>>>>> feature\n"),
        "{after}"
    );
    assert!(after.ends_with("keep five\ntwo theirs\n"), "{after}");
    assert_eq!(fixture.git(&["ls-files", "--unmerged"]), unmerged);

    // The compact layout shows the one remaining block; type its lines.
    session.resize(80, 24);
    select_row(&mut session, mouse, "conflicted  file.txt", 0, true);
    session.wait_for_screen("Conflict blocks: 1");
    session.wait_for_screen("Block 1 of 1 (lines 1–5)");
    operation(&mut session, mouse, LocalCommand::ResolveConflictBlock);
    fill(
        &mut session,
        mouse,
        &[(BLOCK, "1"), (KEEP, "edited"), (TEXT, "one merged")],
    );
    local::preview(&mut session, mouse);
    session.wait_for_screen("Git effect: block 1 → edited text in file.txt");
    local::confirm(&mut session, mouse, 3);
    session.wait_for_screen("Completed • Resolve one conflict block");
    let resolved = body("one merged", "two theirs");
    assert_eq!(fs::read_to_string(&file).unwrap(), resolved);
    assert_eq!(fixture.git(&["ls-files", "--unmerged"]), unmerged);

    select_row(&mut session, mouse, "conflicted  file.txt", 0, true);
    session.wait_for_screen("Conflict blocks: none left");
    stage_selected(&mut session, mouse);
    assert_eq!(fixture.git(&["ls-files", "--unmerged"]), "");
    assert_eq!(fixture.git(&["show", ":file.txt"]), resolved);
    session.wait_for_screen("⚠ MERGE PAUSED • all conflicts resolved and staged");

    operation(&mut session, mouse, LocalCommand::Continue);
    local::preview(&mut session, mouse);
    session.wait_for_screen("merge --continue");
    local::confirm(&mut session, mouse, 0);
    session.wait_for_screen("Completed • Continue operation");
    assert_eq!(fixture.git(&["rev-parse", "HEAD^1"]), format!("{ours}\n"));
    assert_eq!(fixture.git(&["rev-parse", "HEAD^2"]), format!("{theirs}\n"));
    assert_eq!(fixture.git(&["show", "HEAD:file.txt"]), resolved);
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}
