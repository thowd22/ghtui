//! Every management journey drives the real binary and verifies separate Git effects.
use crate::{
    local,
    support::{Fixture, Session},
};
use ghtui::local_actions::LocalCommand;
use std::{fs, process::Command, time::Duration};

pub(crate) fn operation(session: &mut Session, mouse: bool, command: LocalCommand) {
    let index = LocalCommand::ALL
        .iter()
        .position(|(candidate, _)| *candidate == command)
        .unwrap();
    if !mouse {
        local::operation(session, false, command.label(), index);
        return;
    }
    local::open_local_operations(session, true);
    let mut selected = 0;
    let row_label = format!("  {}", command.label());
    let selected_label = format!("> {}", command.label());
    while !session.screen().contains(&row_label) && !session.screen().contains(&selected_label) {
        assert!(
            selected < LocalCommand::ALL.len() - 1,
            "operation did not become visible"
        );
        session.click_visible_action("[PgDn More]");
        selected = (selected + 8).min(LocalCommand::ALL.len() - 1);
        session.wait_for_screen(&format!("> {}", LocalCommand::ALL[selected].1));
    }
    session.click_visible_action(if session.screen().contains(&selected_label) {
        &selected_label
    } else {
        &row_label
    });
    session.wait_for_screen("LOCAL INTENT • Preview first");
}
pub(crate) fn fill(session: &mut Session, mouse: bool, fields: &[(&str, &str)]) {
    for (index, (label, value)) in fields.iter().enumerate() {
        if mouse {
            session.wait_for_screen(&format!("{label}:"));
            let (column, row) = session.locate(&format!("{label}:")).unwrap();
            session.click(column, row + 1);
        } else if index > 0 {
            session.send_key(b"\t");
        }
        session.send_key(b"\x15");
        session.paste(value);
        if *label == "Exact linked worktree directory" && !value.is_empty() {
            crate::long_local_identity_v1::verify_full_input(session, mouse, value);
        } else if !value.is_empty() {
            session.wait_for_screen(value);
        }
    }
    if !mouse && fields.len() > 1 {
        session.send_key(&b"\x1b[Z".repeat(fields.len() - 1));
    }
}
pub(crate) fn run(
    session: &mut Session,
    mouse: bool,
    command: LocalCommand,
    fields: &[(&str, &str)],
) {
    operation(session, mouse, command);
    fill(session, mouse, fields);
    local::preview(session, mouse);
    local::confirm(session, mouse, fields.len());
    session.wait_for_screen(&format!("Completed • {}", command.label()));
}
pub(crate) fn refresh(session: &mut Session, mouse: bool) {
    if mouse {
        // The Repository key bar drops `r Refresh` by priority on narrow rows; the command
        // palette then runs the same global Refresh by mouse.
        if session
            .try_wait_for_control("[r Refresh]", Duration::from_millis(500))
            .is_some()
        {
            session.click_visible_action("[r Refresh]");
        } else {
            session.invoke_via_palette(true, "[r Refresh]");
        }
    } else {
        session.send_key(b"r");
    }
    session.wait_for_screen("Repository loaded • read-only exploration");
}
pub(crate) fn launch(fixture: &Fixture) -> Session {
    let mut session = Session::launch(fixture, 120, 40);
    session.wait_for_screen("Recent commits");
    session
}
pub(crate) fn finish(session: &mut Session, fixture: &Fixture, mouse: bool) {
    assert!(session.inspect_command_journal(fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn branches_tags_keyboard_create_switch_rename_tag_upstream_and_cancel() {
    branches(false);
}
#[test]
fn branches_tags_mouse_create_switch_rename_tag_upstream_and_cancel() {
    branches(true);
}
fn branches(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("file.txt", b"saved\n", "saved");
    let head = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    fixture.git(&["switch", "-c", "collision"]);
    fixture.commit_file("file.txt", b"other branch content\n", "collision target");
    fixture.git(&["switch", "main"]);
    fixture.git(&["update-ref", "refs/remotes/origin/reviewed", &head]);
    fixture.git(&[
        "config",
        "remote.origin.url",
        "https://github.invalid/org/repo.git",
    ]);
    fixture.git(&[
        "config",
        "remote.origin.fetch",
        "+refs/heads/*:refs/remotes/origin/*",
    ]);
    let mut session = launch(&fixture);
    run(
        &mut session,
        mouse,
        LocalCommand::CreateBranch,
        &[("New name", "feature"), ("Saved version", &head)],
    );
    assert_eq!(fixture.git(&["rev-parse", "feature"]), format!("{head}\n"));
    run(
        &mut session,
        mouse,
        LocalCommand::SwitchBranch,
        &[("Existing name", "feature")],
    );
    run(
        &mut session,
        mouse,
        LocalCommand::RenameBranch,
        &[
            ("Existing branch", "feature"),
            ("New branch name", "reviewed"),
        ],
    );
    run(
        &mut session,
        mouse,
        LocalCommand::CreateTag,
        &[("New name", "v-native"), ("Saved version", &head)],
    );
    run(
        &mut session,
        mouse,
        LocalCommand::SetUpstream,
        &[
            ("Local branch", "reviewed"),
            ("Tracking branch (remote/name)", "origin/reviewed"),
        ],
    );
    assert_eq!(fixture.git(&["branch", "--show-current"]), "reviewed\n");
    assert_eq!(
        fixture.git(&["rev-parse", "--abbrev-ref", "@{upstream}"]),
        "origin/reviewed\n"
    );
    session.resize(80, 24);
    operation(&mut session, mouse, LocalCommand::DeleteTag);
    fill(&mut session, mouse, &[("Existing name", "v-native")]);
    local::preview(&mut session, mouse);
    local::close(&mut session, mouse);
    assert_eq!(fixture.git(&["rev-parse", "v-native"]), format!("{head}\n"));
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    fs::write(
        fixture.repo().join("file.txt"),
        b"dirty work must survive\n",
    )
    .unwrap();
    refresh(&mut session, mouse);
    operation(&mut session, mouse, LocalCommand::SwitchBranch);
    fill(&mut session, mouse, &[("Existing name", "collision")]);
    if mouse {
        session.click_visible_action("[Preview]");
    } else {
        session.send_key(b"\x10");
    }
    session.wait_for_screen("Save or stash local changes");
    assert_eq!(fixture.git(&["branch", "--show-current"]), "reviewed\n");
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), format!("{head}\n"));
    assert_eq!(
        fs::read(fixture.repo().join("file.txt")).unwrap(),
        b"dirty work must survive\n"
    );
    local::close(&mut session, mouse);
    finish(&mut session, &fixture, mouse);
}

#[test]
fn remote_git_keyboard_explicit_bare_push_fetch_and_effects() {
    remote(false);
}
#[test]
fn remote_git_mouse_explicit_bare_push_fetch_and_effects() {
    remote(true);
}
fn remote(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("file.txt", b"saved\n", "saved");
    let bare = fixture.root.join("remote.git");
    assert!(Command::new("git")
        .args(["init", "--bare"])
        .arg(&bare)
        .output()
        .unwrap()
        .status
        .success());
    fixture.git(&["remote", "add", "origin", bare.to_str().unwrap()]);
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let mut session = launch(&fixture);
    run(
        &mut session,
        mouse,
        LocalCommand::Push,
        &[
            ("Configured remote", "origin"),
            ("Local branch", "main"),
            ("Set tracking branch?", "true"),
        ],
    );
    let observed = Command::new("git")
        .arg("--git-dir")
        .arg(&bare)
        .args(["rev-parse", "refs/heads/main"])
        .output()
        .unwrap();
    assert!(observed.status.success());
    assert_eq!(String::from_utf8(observed.stdout).unwrap(), head);
    assert_eq!(
        fixture.git(&["rev-parse", "--abbrev-ref", "@{upstream}"]),
        "origin/main\n"
    );
    session.resize(80, 24);
    run(
        &mut session,
        mouse,
        LocalCommand::Fetch,
        &[("Configured remote", "origin")],
    );
    assert_eq!(fixture.git(&["rev-parse", "origin/main"]), head);
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    finish(&mut session, &fixture, mouse);
}

#[test]
fn stash_worktrees_keyboard_immutable_stash_apply_and_dirty_removal() {
    stash(false);
}
#[test]
fn stash_worktrees_mouse_immutable_stash_apply_and_dirty_removal() {
    stash(true);
}
fn stash(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("file.txt", b"saved\n", "saved");
    let head = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    fixture.git(&["branch", "linked"]);
    fs::write(fixture.repo().join("file.txt"), b"important work\n").unwrap();
    fs::write(fixture.repo().join("untracked.txt"), b"kept untracked\n").unwrap();
    let mut session = launch(&fixture);
    run(
        &mut session,
        mouse,
        LocalCommand::StashSave,
        &[
            ("Stash description", "native stash"),
            ("Include untracked files?", "false"),
        ],
    );
    let id = fixture.git(&["rev-parse", "refs/stash"]).trim().to_string();
    assert_eq!(
        fs::read(fixture.repo().join("file.txt")).unwrap(),
        b"saved\n"
    );
    run(
        &mut session,
        mouse,
        LocalCommand::StashApply,
        &[("Exact stash commit SHA", &id)],
    );
    assert_eq!(
        fs::read(fixture.repo().join("file.txt")).unwrap(),
        b"important work\n"
    );
    let linked = fixture.root.join("linked-worktree");
    run(
        &mut session,
        mouse,
        LocalCommand::AddWorktree,
        &[
            ("New worktree directory", linked.to_str().unwrap()),
            ("Existing branch", "linked"),
        ],
    );
    assert!(linked.join("file.txt").exists());
    fs::write(linked.join("file.txt"), b"linked work\n").unwrap();
    session.resize(80, 24);
    operation(&mut session, mouse, LocalCommand::RemoveWorktree);
    fill(
        &mut session,
        mouse,
        &[("Exact linked worktree directory", linked.to_str().unwrap())],
    );
    if mouse {
        session.click_visible_action("[Preview]");
    } else {
        session.send_key(b"\x10");
    }
    session.wait_for_screen("Linked worktree is dirty");
    assert_eq!(fs::read(linked.join("file.txt")).unwrap(), b"linked work\n");
    local::close(&mut session, mouse);
    fs::write(linked.join("file.txt"), b"saved\n").unwrap();
    let ignore_file = fixture.root.join("ignore-rules");
    fs::write(&ignore_file, b"ignored.env\n").unwrap();
    fixture.git(&["config", "core.excludesFile", ignore_file.to_str().unwrap()]);
    fs::write(
        linked.join("ignored.env"),
        b"important ignored configuration\n",
    )
    .unwrap();
    operation(&mut session, mouse, LocalCommand::RemoveWorktree);
    fill(
        &mut session,
        mouse,
        &[("Exact linked worktree directory", linked.to_str().unwrap())],
    );
    if mouse {
        session.click_visible_action("[Preview]");
    } else {
        session.send_key(b"\x10");
    }
    session.wait_for_screen("ignored local files");
    assert_eq!(
        fs::read(linked.join("ignored.env")).unwrap(),
        b"important ignored configuration\n"
    );
    assert!(linked.join(".git").exists());
    local::close(&mut session, mouse);
    fs::remove_file(linked.join("ignored.env")).unwrap();
    run(
        &mut session,
        mouse,
        LocalCommand::RemoveWorktree,
        &[("Exact linked worktree directory", linked.to_str().unwrap())],
    );
    assert!(!linked.exists());
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), format!("{head}\n"));
    assert_eq!(
        fs::read(fixture.repo().join("file.txt")).unwrap(),
        b"important work\n"
    );
    assert_eq!(
        fs::read(fixture.repo().join("untracked.txt")).unwrap(),
        b"kept untracked\n"
    );
    assert_eq!(fixture.git(&["rev-parse", "refs/stash"]), format!("{id}\n"));
    finish(&mut session, &fixture, mouse);
}

#[test]
fn git_sequencers_keyboard_conflict_version_stage_continue_and_parent_links() {
    sequencer(false);
}
#[test]
fn git_sequencers_mouse_conflict_version_stage_continue_and_parent_links() {
    sequencer(true);
}
fn sequencer(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("file.txt", b"base\n", "base");
    fixture.git(&["switch", "-c", "feature"]);
    fixture.commit_file("file.txt", b"theirs\n", "theirs");
    let theirs = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    fixture.git(&["switch", "main"]);
    fixture.commit_file("file.txt", b"ours\n", "ours");
    let ours = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    let mut session = launch(&fixture);
    operation(&mut session, mouse, LocalCommand::Merge);
    fill(&mut session, mouse, &[("Saved version", &theirs)]);
    local::preview(&mut session, mouse);
    local::confirm(&mut session, mouse, 1);
    session.wait_for_screen("CONFLICT: the operation stopped");
    session.wait_for_screen("[F8 Error↓]");
    // The plain-language summary precedes raw Git output; scroll to the recovery advice.
    for _ in 0..4 {
        if mouse {
            session.click_visible_action("[F8 Error↓]");
        } else {
            session.send_key(b"\x1b[19~");
        }
        if session
            .try_wait_for(
                "Refresh repository state",
                std::time::Duration::from_secs(2),
            )
            .is_ok()
        {
            break;
        }
    }
    session.wait_for_screen("Refresh repository state");
    assert!(fixture
        .git(&["ls-files", "--unmerged"])
        .contains("file.txt"));
    local::close(&mut session, mouse);
    refresh(&mut session, mouse);
    session.wait_for_screen("conflicted  file.txt");
    if mouse {
        session.click_visible_action("conflicted  file.txt");
        session.wait_for_screen("Conflict in file.txt");
    } else {
        session.send_key(b"\x1b[B");
    }
    run(
        &mut session,
        mouse,
        LocalCommand::ResolveConflict,
        &[(
            "Keep version (ours = current branch, theirs = incoming, base = common ancestor)",
            "ours",
        )],
    );
    assert_eq!(
        fs::read(fixture.repo().join("file.txt")).unwrap(),
        b"ours\n"
    );
    // Selection after refreshed content is explicit again, never inferred from labels.
    if mouse {
        session.click_visible_action("conflicted  file.txt");
        session.wait_for_screen("Conflict in file.txt");
    } else {
        session.send_key(&b"\x1b[A".repeat(32));
    }
    run(&mut session, mouse, LocalCommand::StageFile, &[]);
    assert_eq!(fixture.git(&["ls-files", "--unmerged"]), "");
    session.resize(80, 24);
    run(&mut session, mouse, LocalCommand::Continue, &[]);
    assert_eq!(fixture.git(&["rev-parse", "HEAD^1"]), format!("{ours}\n"));
    assert_eq!(fixture.git(&["rev-parse", "HEAD^2"]), format!("{theirs}\n"));
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    finish(&mut session, &fixture, mouse);
}

#[test]
fn reflog_recovery_keyboard_rescue_old_commit_and_cancel_reset() {
    recovery(false);
}
#[test]
fn reflog_recovery_mouse_rescue_old_commit_and_cancel_reset() {
    recovery(true);
}
fn recovery(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("file.txt", b"old\n", "old");
    let old = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    fixture.commit_file("file.txt", b"current\n", "current");
    let head = fixture.git(&["rev-parse", "HEAD"]);
    fs::write(fixture.repo().join("file.txt"), b"important dirty work\n").unwrap();
    let mut session = launch(&fixture);
    run(
        &mut session,
        mouse,
        LocalCommand::RecoverBranch,
        &[("New name", "rescue"), ("Saved version", &old)],
    );
    assert_eq!(fixture.git(&["rev-parse", "rescue"]), format!("{old}\n"));
    session.resize(80, 24);
    operation(&mut session, mouse, LocalCommand::Reset);
    fill(
        &mut session,
        mouse,
        &[("Saved version", &old), ("Reset effect", "hard")],
    );
    local::preview(&mut session, mouse);
    session.wait_for_screen("Warning:");
    local::close(&mut session, mouse);
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(
        fs::read(fixture.repo().join("file.txt")).unwrap(),
        b"important dirty work\n"
    );
    finish(&mut session, &fixture, mouse);
}

#[test]
fn guided_bisect_keyboard_classify_versions_find_culprit_and_restore() {
    bisect(false);
}
#[test]
fn guided_bisect_mouse_classify_versions_find_culprit_and_restore() {
    bisect(true);
}
fn bisect(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    let mut commits = vec![];
    for index in 0..6 {
        fixture.commit_file(
            "version.txt",
            format!("version {index}\n").as_bytes(),
            &format!("version {index}"),
        );
        commits.push(fixture.git(&["rev-parse", "HEAD"]).trim().to_string());
    }
    let start = fixture.git(&["rev-parse", "HEAD"]);
    let mut session = launch(&fixture);
    run(
        &mut session,
        mouse,
        LocalCommand::BisectStart,
        &[
            ("Known good saved version", &commits[0]),
            ("Known bad saved version", &commits[5]),
        ],
    );
    for _ in 0..8 {
        let id = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
        let index = commits
            .iter()
            .position(|candidate| *candidate == id)
            .unwrap();
        run(
            &mut session,
            mouse,
            if index >= 3 {
                LocalCommand::BisectBad
            } else {
                LocalCommand::BisectGood
            },
            &[],
        );
        if ghtui::git::bisect_reports_culprit(&fixture.git(&["bisect", "log"])) {
            break;
        }
    }
    let log = fixture.git(&["bisect", "log"]);
    assert!(
        ghtui::git::bisect_reports_culprit(&log)
            && log.contains(&format!(" commit: [{}]", commits[3])),
        "{log}"
    );
    assert_eq!(
        fixture.git(&["rev-parse", "HEAD"]),
        format!("{}\n", commits[3])
    );
    session.wait_for_screen("Culprit found:");
    if mouse {
        session.click_visible_action("[d Changes]");
    } else {
        session.send_key(b"d");
    }
    session.wait_for_screen("Saved commit changes");
    session.wait_for_screen("-version 2");
    session.wait_for_screen("+version 3");
    assert_eq!(
        fixture.git(&["rev-parse", "HEAD"]),
        format!("{}\n", commits[3])
    );
    session.resize(80, 24);
    run(&mut session, mouse, LocalCommand::BisectReset, &[]);
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), start);
    assert_eq!(fixture.git(&["branch", "--show-current"]), "main\n");
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    assert!(!fixture.repo().join(".git/BISECT_LOG").exists());
    finish(&mut session, &fixture, mouse);
}

/// Local main and the bare remote's main each gain one commit.
fn v1_local_branches_diverged(fixture: &Fixture) -> (std::path::PathBuf, String) {
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
    fixture.git(&["switch", "-c", "side"]);
    fixture.commit_file("remote.txt", b"remote side\n", "remote side");
    let remote_head = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    fixture.git(&["push", "origin", "side:main"]);
    fixture.git(&["switch", "main"]);
    fixture.git(&["branch", "-D", "side"]);
    fixture.commit_file("local.txt", b"local side\n", "local side");
    fixture.git(&["fetch", "origin"]);
    (bare, remote_head)
}
fn v1_bare_main(bare: &std::path::Path) -> String {
    let output = Command::new("git")
        .arg("--git-dir")
        .arg(bare)
        .args(["rev-parse", "refs/heads/main"])
        .output()
        .unwrap();
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}
fn v1_confirm(session: &mut Session, mouse: bool) {
    if mouse {
        session.click_visible_action("[Confirm action]");
    } else {
        session.send_key(b"\x13");
    }
}

#[test]
fn v1_local_branches_keyboard_diverged_sync_rejected_push_rebase_and_first_push() {
    v1_sync(false);
}
#[test]
fn v1_local_branches_mouse_diverged_sync_rejected_push_rebase_and_first_push() {
    v1_sync(true);
}
fn v1_sync(mouse: bool) {
    let fixture = Fixture::new();
    let (bare, remote_head) = v1_local_branches_diverged(&fixture);
    let local_head = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    let mut session = launch(&fixture);
    session.wait_for_screen("DIVERGED: pull with merge or rebase");

    // A normal push is predicted and then reported as rejected; the remote is unchanged.
    operation(&mut session, mouse, LocalCommand::Push);
    fill(
        &mut session,
        mouse,
        &[
            ("Configured remote", "origin"),
            ("Local branch", "main"),
            ("Set tracking branch?", "false"),
        ],
    );
    local::preview(&mut session, mouse);
    session.wait_for_screen("REMOTE AHEAD: a normal push will be rejected");
    v1_confirm(&mut session, mouse);
    session.wait_for_screen("Error: Push rejected: the remote branch has commits");
    assert_eq!(v1_bare_main(&bare), remote_head);
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]).trim(), local_head);
    local::close(&mut session, mouse);
    refresh(&mut session, mouse);

    // Rebase pull explains the divergence and replays the local commit on the remote one.
    operation(&mut session, mouse, LocalCommand::Pull);
    fill(
        &mut session,
        mouse,
        &[
            ("Configured remote", "origin"),
            ("Remote branch", "main"),
            ("Pull strategy", "rebase"),
        ],
    );
    local::preview(&mut session, mouse);
    session.wait_for_screen("DIVERGED: main and origin/main");
    session.wait_for_screen("Rebase replays your 1 local commit");
    v1_confirm(&mut session, mouse);
    session.wait_for_screen("Completed • Pull remote branch");
    assert_eq!(fixture.git(&["rev-parse", "HEAD~1"]).trim(), remote_head);
    session.wait_for_screen("1 commits to send, 0 commits to receive • push to share");
    run(
        &mut session,
        mouse,
        LocalCommand::Push,
        &[
            ("Configured remote", "origin"),
            ("Local branch", "main"),
            ("Set tracking branch?", "false"),
        ],
    );
    let pushed = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    assert_eq!(v1_bare_main(&bare), pushed);

    // An unmerged branch deletion shows its recovery SHA; cancelling keeps it.
    let topic = fixture
        .git(&[
            "commit-tree",
            "HEAD^{tree}",
            "-p",
            "HEAD",
            "-m",
            "topic only",
        ])
        .trim()
        .to_string();
    fixture.git(&["branch", "topic", &topic]);
    session.resize(80, 24);
    operation(&mut session, mouse, LocalCommand::DeleteBranch);
    fill(
        &mut session,
        mouse,
        &[
            ("Existing branch", "topic"),
            ("Delete unmerged history?", "false"),
        ],
    );
    local::preview(&mut session, mouse);
    session.wait_for_screen("UNMERGED: topic has");
    local::close(&mut session, mouse);
    assert_eq!(fixture.git(&["rev-parse", "topic"]).trim(), topic);

    // A new branch defaults to publishing with tracking on its first push.
    fixture.git(&["switch", "-c", "feature"]);
    refresh(&mut session, mouse);
    session.wait_for_screen("no upstream");
    operation(&mut session, mouse, LocalCommand::Push);
    session.wait_for_screen("feature");
    local::preview(&mut session, mouse);
    session.wait_for_screen("FIRST PUSH: creates branch feature");
    v1_confirm(&mut session, mouse);
    session.wait_for_screen("Completed • Push branch");
    assert_eq!(
        fixture.git(&["rev-parse", "--abbrev-ref", "@{upstream}"]),
        "origin/feature\n"
    );
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    finish(&mut session, &fixture, mouse);
}
