//! TASK-118/119 on the real binary: the Overview's plain sections, the expandable worktree
//! stack over a main checkout with three linked worktrees, opening one in place and coming
//! back, and the stale `core.worktree` screen (open a linked worktree, preview, cancel, then
//! confirm the unset). Keyboard and mouse; every read-only path leaves refs, worktrees,
//! indexes and files exactly as they were, and the confirmed unset changes only that setting.
use crate::{
    local_management_v41::activate_row,
    support::{Fixture, Session},
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

fn git_in(directory: &Path, args: &[&str]) -> Vec<u8> {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

/// A main checkout and three linked worktrees under one folder: `two` has a changed file,
/// `three` is locked.
fn stack_fixture() -> (Fixture, Vec<PathBuf>) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("notes.txt", b"first\n", "First note");
    let mut linked = vec![];
    for name in ["one", "two", "three"] {
        fixture.git(&["branch", &format!("agent/{name}")]);
        let path = fixture.root.join("trees").join(name);
        fixture.git(&[
            "worktree",
            "add",
            "-q",
            path.to_str().unwrap(),
            &format!("agent/{name}"),
        ]);
        linked.push(path);
    }
    fixture.git(&["worktree", "lock", linked[2].to_str().unwrap()]);
    fs::write(linked[1].join("notes.txt"), b"two is busy\n").unwrap();
    (fixture, linked)
}

/// Refs, the worktree list, and each worktree's status, index bytes and files.
fn witness(repo: &Path, linked: &[PathBuf]) -> Vec<Vec<u8>> {
    let mut state = vec![
        git_in(linked[0].as_path(), &["for-each-ref"]),
        git_in(linked[0].as_path(), &["worktree", "list", "--porcelain"]),
    ];
    for tree in linked.iter().map(PathBuf::as_path).chain([repo]) {
        let index = git_in(
            tree,
            &["rev-parse", "--path-format=absolute", "--git-path", "index"],
        );
        let index = PathBuf::from(String::from_utf8(index).unwrap().trim());
        state.push(fs::read(index).unwrap());
        state.push(fs::read(tree.join("notes.txt")).unwrap());
    }
    for tree in linked {
        state.push(git_in(tree, &["status", "--porcelain"]));
    }
    state
}

fn wait_without(session: &mut Session, text: &str) {
    let text = text.to_owned();
    session
        .try_wait_for_condition(
            |screen| !screen.contains(&text),
            "text gone from the screen",
            Duration::from_secs(8),
        )
        .unwrap_or_else(|error| panic!("{error}\n{}", session.screen()));
}

fn stack_journey(mouse: bool) {
    let (fixture, linked) = stack_fixture();
    let head = String::from_utf8(git_in(&fixture.repo(), &["rev-parse", "HEAD"])).unwrap();
    let before = witness(&fixture.repo(), &linked);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");
    // TASK-119: plain sections; the full id and UTC time stay in the Branch section.
    for text in [
        "Branch: main · no upstream yet · Push publishes it",
        "[Push to publish this branch",
        "Last commit · ",
        " UTC · First note",
        &format!("  {}", head.trim()),
        "  author and committer Fixture Author",
        "Changes · clean",
        "Working files are clean",
        "Stashes",
        "No stashes",
        "▸ 4 worktrees: this checkout + 3 linked · 1 with changes",
    ] {
        session.wait_for_screen(text);
    }
    let screen = session.screen();
    for jargon in ["In use:", "Viewed:", "saved versions"] {
        assert!(!screen.contains(jargon), "{jargon}\n{screen}");
    }
    assert!(!screen.contains("…/two"), "collapsed stack\n{screen}");

    // Expand the stack and open the busy worktree in place.
    activate_row(&mut session, mouse, "4 worktrees: this checkout + 3 linked");
    session.wait_for_screen("▾ 4 worktrees");
    session.wait_for_screen("Linked worktrees are in");
    session.wait_for_screen("main checkout · you are here · clean");
    session.wait_for_screen("· clean · …/one");
    session.wait_for_screen("· clean · locked · …/three");
    activate_row(&mut session, mouse, "· 1 changed · …/two");
    session.wait_for_screen("Repository loaded • read-only exploration");
    session.wait_for_screen("Branch · Worktree: agent/two");
    session.wait_for_screen("◂ Back to ");
    session.wait_for_screen("you are here · 1 changed · …/two");
    session.wait_for_screen("Changes · 1 unstaged");
    session.wait_for_screen("notes.txt");
    assert_eq!(witness(&fixture.repo(), &linked), before);

    // The context and the way back stay visible at 80x24.
    session.resize(80, 24);
    session.wait_for_screen("Branch · Worktree: agent/two");
    session.wait_for_screen("◂ Back to ");
    // TASK-119: at 140x42 the sections and the stack stay whole and every row still acts.
    session.resize(140, 42);
    session.wait_for_screen("Branch · Worktree: agent/two");
    session.wait_for_screen("Changes · 1 unstaged");
    session.wait_for_screen("◂ Back to ");
    if mouse {
        session.click_visible_action("◂ Back to ");
    } else {
        session.send_key(b"B");
    }
    session.wait_for_screen("main checkout · you are here · clean");
    wait_without(&mut session, "Worktree: agent/two");
    session.wait_for_screen("Branch: main · no upstream yet");
    session.wait_for_screen("Changes · clean");
    session.wait_for_screen("No stashes");
    session.wait_for_screen("Recent commits");
    activate_row(&mut session, mouse, "· clean · …/one");
    session.wait_for_screen("Branch · Worktree: agent/one");
    session.wait_for_screen("you are here · clean · …/one");
    assert_eq!(witness(&fixture.repo(), &linked), before);
    session.resize(120, 40);
    session.wait_for_screen("◂ Back to ");
    if mouse {
        session.click_visible_action("◂ Back to ");
    } else {
        session.send_key(b"B");
    }
    session.wait_for_screen("main checkout · you are here · clean");
    wait_without(&mut session, "Worktree: agent/one");
    session.wait_for_screen("Branch: main · no upstream yet");
    assert_eq!(witness(&fixture.repo(), &linked), before);
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn worktree_stack_v118_keyboard_sections_expand_open_and_back() {
    stack_journey(false)
}
#[test]
fn worktree_stack_v118_mouse_sections_expand_open_and_back() {
    stack_journey(true)
}

fn choose(session: &mut Session, mouse: bool, control: &str, key: &[u8]) {
    session.wait_for_settled_footer();
    if mouse {
        session.click_visible_action(control);
    } else {
        session.send_key(key);
    }
}

fn mismatch_journey(mouse: bool) {
    let (fixture, linked) = stack_fixture();
    let elsewhere = fixture.root.join("elsewhere");
    fs::create_dir(&elsewhere).unwrap();
    fs::write(elsewhere.join("other.txt"), b"another tool's folder\n").unwrap();
    fixture.git(&["config", "core.worktree", elsewhere.to_str().unwrap()]);
    let before = witness(&fixture.repo(), &linked);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    if mouse {
        session.click_visible_action("2 Repository");
    } else {
        session.send_key(b"2");
    }
    // Plain words and choices instead of a dead end.
    for text in [
        "This checkout's Git settings point at another folder",
        "Git uses instead:",
        &format!("core.worktree = {}", elsewhere.display()),
        "O  Open the folder Git uses",
        "W  Show the linked worktrees of this repository (3)",
        "m  Local operations: unset core.worktree (preview first)",
    ] {
        session.wait_for_screen(text);
    }

    // The linked worktrees, read from the Git directory; open one and come back.
    choose(&mut session, mouse, "[W Linked worktrees]", b"W");
    session.wait_for_screen("⎇ agent/one");
    session.wait_for_screen("⎇ agent/three");
    if mouse {
        session.click_visible_action("⎇ agent/one");
    } else {
        // The W row keeps the selection; the first linked worktree is the next row.
        session.send_key(b"\x1b[B\r");
    }
    session.wait_for_screen("Branch · Worktree: agent/one");
    session.wait_for_screen("Recent commits");
    assert_eq!(witness(&fixture.repo(), &linked), before);
    if mouse {
        session.click_visible_action("◂ Back to ");
    } else {
        session.send_key(b"B");
    }
    session.wait_for_screen("Git uses instead:");

    // Unsetting needs its preview and a confirmation; Cancel changes nothing.
    choose(&mut session, mouse, "[m Local operations]", b"m");
    session.wait_for_screen("Preview: unset core.worktree");
    session.wait_for_screen("config --unset core.worktree");
    session.wait_for_screen("no files change");
    choose(&mut session, mouse, "[Esc Keep setting]", b"\x1b");
    session.wait_for_screen("Cancelled • core.worktree was not changed");
    assert_eq!(
        fixture.git(&["config", "core.worktree"]).trim(),
        elsewhere.to_str().unwrap()
    );
    assert_eq!(witness(&fixture.repo(), &linked), before);
    choose(&mut session, mouse, "[m Local operations]", b"m");
    session.wait_for_screen("Preview: unset core.worktree");
    choose(&mut session, mouse, "[y Confirm unset]", b"y");
    session.wait_for_screen("core.worktree unset • Repository now reads this checkout");
    session.wait_for_screen("Branch: main");
    session.wait_for_screen("Recent commits");
    // Exactly the one setting is gone; refs, worktrees, indexes and files are unchanged.
    let unset = Command::new("git")
        .arg("-C")
        .arg(fixture.repo())
        .args(["config", "--get", "core.worktree"])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(!unset.status.success() && unset.stdout.is_empty());
    assert_eq!(witness(&fixture.repo(), &linked), before);
    assert_eq!(
        fs::read(elsewhere.join("other.txt")).unwrap(),
        b"another tool's folder\n"
    );
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn worktree_stack_v118_keyboard_core_worktree_choices_preview_cancel_and_unset() {
    mismatch_journey(false)
}
#[test]
fn worktree_stack_v118_mouse_core_worktree_choices_preview_cancel_and_unset() {
    mismatch_journey(true)
}
