//! TASK-39/40: a detached HEAD refuses Pull and warns before switching away from commits on
//! no branch; a push cancelled while Git runs reports unchanged local refs and an UNKNOWN
//! remote outcome. Every effect is read independently from Git and the local bare remote.
use crate::{
    local,
    local_management::{fill, operation, pick_operation, refresh, run},
    support::{Fixture, Session},
};
use ghtui::local_actions::LocalCommand;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

const MENU: &str = "Local operations • explicit preview and confirmation";

fn bare_ref(bare: &Path, name: &str) -> Option<String> {
    let output = Command::new("git")
        .arg("--git-dir")
        .arg(bare)
        .args(["rev-parse", "--verify", "-q", name])
        .output()
        .unwrap();
    output
        .status
        .success()
        .then(|| String::from_utf8(output.stdout).unwrap().trim().to_string())
}

#[test]
fn remote_git_keyboard_detached_head_guards_and_cancelled_push_reports_effects() {
    journey(false);
}
#[test]
fn remote_git_mouse_detached_head_guards_and_cancelled_push_reports_effects() {
    journey(true);
}

fn journey(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("file.txt", b"saved\n", "initial");
    let base = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
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
    fixture.commit_file("publish.txt", b"to publish\n", "unpublished on main");
    let main = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    fixture.git(&["switch", "--detach"]);
    fixture.commit_file("detached.txt", b"detached work\n", "detached work");
    let detached = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    // The push hook records that Git is running, then waits for a release file the test
    // never creates (bounded), so only cancellation ends the push.
    let started = fixture.root.join("push-started");
    let hook = fixture.repo().join(".git/hooks/pre-push");
    fs::write(
        &hook,
        format!(
            "#!/bin/sh\n: > '{}'\ni=0\nwhile [ ! -e '{}' ] && [ $i -lt 600 ]; do sleep 0.1; i=$((i+1)); done\nexit 1\n",
            started.display(),
            fixture.root.join("push-release").display()
        ),
    )
    .unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();

    let mut session = Session::launch(&fixture, 80, 24);
    session.wait_for_screen("Branch: detached HEAD");

    // Pull is refused on a detached HEAD, in the menu and when chosen.
    pick_operation(&mut session, mouse, LocalCommand::Pull);
    session.wait_for_screen("[disabled: Detached HEAD: pull");
    if let Err(error) = session.try_wait_for_condition(
        |screen| screen.matches("Detached HEAD: pull").count() >= 2,
        "Pull refusal in the status line",
        Duration::from_secs(8),
    ) {
        panic!("{error}");
    }
    assert!(!session.screen().contains("LOCAL INTENT"));
    if mouse {
        session.close_overlay_by_mouse(MENU);
    } else {
        session.close_overlay_by_key(MENU);
    }
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]).trim(), detached);

    // Switching away warns about the commit on no branch; cancelling keeps HEAD detached.
    session.resize(120, 40);
    operation(&mut session, mouse, LocalCommand::SwitchBranch);
    fill(&mut session, mouse, &[("Existing name", "main")]);
    local::preview(&mut session, mouse);
    session.wait_for_screen("DETACHED HEAD: 1 commit here is on no branch or tag.");
    local::close(&mut session, mouse);
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]).trim(), detached);
    assert_eq!(fixture.git(&["branch", "--show-current"]), "");

    // Create branch here keeps the commit; the switch preview then says nothing is lost.
    run(
        &mut session,
        mouse,
        LocalCommand::CreateBranch,
        &[("New name", "kept"), ("Saved version", "HEAD")],
    );
    assert_eq!(fixture.git(&["rev-parse", "kept"]).trim(), detached);
    assert_eq!(fixture.git(&["branch", "--show-current"]), "");
    operation(&mut session, mouse, LocalCommand::SwitchBranch);
    fill(&mut session, mouse, &[("Existing name", "main")]);
    local::preview(&mut session, mouse);
    session.wait_for_screen("DETACHED HEAD: every commit here is on a branch or tag.");
    local::confirm(&mut session, mouse, 1);
    session.wait_for_screen("Completed • Switch branch");
    assert_eq!(fixture.git(&["branch", "--show-current"]), "main\n");
    assert_eq!(fixture.git(&["rev-parse", "kept"]).trim(), detached);

    // A push cancelled while Git runs: local refs unchanged, remote outcome UNKNOWN.
    let refs = fixture.git(&["show-ref"]);
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
    session.wait_for_screen("sends 1 commit;");
    local::confirm(&mut session, mouse, 3);
    let deadline = Instant::now() + Duration::from_secs(20);
    while !started.exists() {
        assert!(
            Instant::now() < deadline,
            "push never reached its hook\n{}",
            session.screen()
        );
        session.poll_output(Duration::from_millis(20));
    }
    session.wait_for_screen("[Esc Cancel]");
    if mouse {
        session.click_visible_action("[Esc Cancel]");
    } else {
        session.send_key(b"\x1b");
    }
    session.wait_for_screen("Cancelled: Git was stopped on request.");
    session.wait_for_screen("Local effects: no branch, tag or tracking ref changed.");
    session.wait_for_screen("Remote effect: UNKNOWN");
    assert_eq!(
        bare_ref(&bare, "refs/heads/main").as_deref(),
        Some(base.as_str())
    );
    assert_eq!(fixture.git(&["rev-parse", "main"]).trim(), main);
    assert_eq!(fixture.git(&["show-ref"]), refs);
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    local::close(&mut session, mouse);
    refresh(&mut session, mouse);
    session.wait_for_screen("Branch: main");
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}
