//! Real-binary local mutations; effects are independently read from Git objects/index/worktree.
use crate::support::{Fixture, Session};
#[path = "form_preview_visible_local_v1.rs"]
mod form_preview_visible_v1;
#[path = "local_confirm_shortcut_v1.rs"]
mod local_confirm_shortcut_v1;
#[path = "pr_create_v1.rs"]
mod pr_create_v1;
#[path = "stale_worktree_v1.rs"]
mod stale_worktree_v1;

#[test]
fn local_workspace_keyboard_explicit_github_repo_keeps_local_dirty_and_unborn_checkouts_accessible()
{
    explicit_github_local_workspace(false)
}

#[test]
fn local_workspace_mouse_explicit_github_repo_keeps_local_dirty_and_unborn_checkouts_accessible() {
    explicit_github_local_workspace(true)
}

fn explicit_github_local_workspace(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file(
        "nested/local.txt",
        b"saved local content\n",
        "Local saved version",
    );
    fixture.commit_file("staged.txt", b"before staging\n", "Another local version");
    std::fs::write(
        fixture.repo().join("nested/local.txt"),
        b"dirty local content\n",
    )
    .unwrap();
    std::fs::write(fixture.repo().join("staged.txt"), b"staged local content\n").unwrap();
    fixture.git(&["add", "staged.txt"]);
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = std::fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let refs = fixture.git(&["show-ref"]);
    let mut session = Session::launch_from_with_args(
        &fixture,
        &fixture.repo().join("nested"),
        120,
        40,
        &["--host", "github.invalid", "--repo", "different/remote"],
    );
    session.wait_for_screen("different/remote");
    session.assert_focused_view("Inbox");
    if mouse {
        session.click_visible_action("Repository");
    } else {
        session.send_key(b"2");
    }
    session.wait_for_screen("Branch: main");
    session.assert_focused_view("Repository");
    session.wait_for_screen("Local saved version");
    session.wait_for_cursorless_condition(
        |frame| {
            let text = frame.contents();
            text.contains("Local saved version") && text.contains("Branch: main")
        },
        "completed original local history screenshot frame",
        std::time::Duration::from_secs(8),
    );
    session.capture_completed_terminal_v1("git-history");
    if mouse {
        session.click_visible_action("[w Working]");
    } else {
        session.send_key(b"w");
    }
    session.wait_for_screen("dirty local content");
    session.resize(80, 24);
    if mouse {
        session.click_visible_action("[i Staged]");
    } else {
        session.send_key(b"i");
    }
    session.wait_for_screen("staged local content");
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.git(&["show-ref"]), refs);
    assert_eq!(fixture.git(&["status", "--porcelain=v2", "-z"]), status);
    assert_eq!(
        std::fs::read(fixture.repo().join(".git/index")).unwrap(),
        index
    );
    assert_eq!(
        std::fs::read(fixture.repo().join("nested/local.txt")).unwrap(),
        b"dirty local content\n"
    );
    assert_eq!(
        std::fs::read(fixture.repo().join("staged.txt")).unwrap(),
        b"staged local content\n"
    );
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);

    let unborn = Fixture::new();
    unborn.init_git();
    std::fs::create_dir(unborn.repo().join("nested")).unwrap();
    std::fs::write(unborn.repo().join("first.txt"), b"first staged content\n").unwrap();
    unborn.git(&["add", "first.txt"]);
    std::fs::write(unborn.repo().join("untracked.txt"), b"untracked content\n").unwrap();
    let index = std::fs::read(unborn.repo().join(".git/index")).unwrap();
    let status = unborn.git(&["status", "--porcelain=v2", "-z"]);
    let mut session = Session::launch_from_with_args(
        &unborn,
        &unborn.repo().join("nested"),
        120,
        40,
        &["--repo", "different/remote"],
    );
    session.wait_for_screen("different/remote");
    session.assert_focused_view("Inbox");
    if mouse {
        session.click_visible_action("Repository");
    } else {
        session.send_key(b"2");
    }
    session.wait_for_screen("Branch: main");
    session.wait_for_screen("saved version: none yet");
    session.resize(80, 24);
    if mouse {
        session.click_visible_action("[C Compare]");
    } else {
        session.send_key(b"C");
    }
    session.wait_for_screen("Action unavailable");
    session.wait_for_screen("no saved commits yet");
    assert!(!session.screen().contains("READ-ONLY COMPARISON"));
    // The explanation captures input; underlying navigation cannot start a GitHub read.
    if mouse {
        session.click_visible_action("Inbox");
        session.click_visible_action("[Esc Close]");
    } else {
        session.send_key(b"1");
        session.close_overlay_by_key("Action unavailable");
    }
    session.assert_focused_view("Repository");
    if mouse {
        session.click_visible_action("[i Staged]");
    } else {
        session.send_key(b"i");
    }
    session.wait_for_screen("first staged content");
    assert_eq!(unborn.git(&["symbolic-ref", "HEAD"]), "refs/heads/main\n");
    assert_eq!(unborn.git(&["status", "--porcelain=v2", "-z"]), status);
    assert_eq!(
        std::fs::read(unborn.repo().join(".git/index")).unwrap(),
        index
    );
    assert_eq!(
        std::fs::read(unborn.repo().join("first.txt")).unwrap(),
        b"first staged content\n"
    );
    assert_eq!(
        std::fs::read(unborn.repo().join("untracked.txt")).unwrap(),
        b"untracked content\n"
    );
    assert!(!unborn.repo().join(".git/refs/heads/main").exists());
    assert!(session.inspect_command_journal(&unborn).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}

pub(crate) fn select_changed(session: &mut Session, mouse: bool, name: &str) {
    if mouse {
        session.click_visible_action(name);
        session.wait_for_screen("Selected working file:");
    } else {
        session.send_key(b"\x1b[B");
    }
}
pub(crate) fn operation(session: &mut Session, mouse: bool, label: &str, index: usize) {
    if mouse {
        session.click_visible_action("[m Local operations]")
    } else {
        session.send_key(b"m")
    }
    session.wait_for_screen("Local operations • explicit preview");
    if mouse {
        session.click_visible_action(label)
    } else {
        let mut keys = b"\x1b[B".repeat(index);
        keys.push(b'\r');
        session.send_key(&keys)
    }
    session.wait_for_screen("LOCAL INTENT • Preview first");
}
pub(crate) fn preview(session: &mut Session, mouse: bool) {
    if mouse {
        session.click_visible_action("[Preview]")
    } else {
        session.send_key(b"\x10")
    };
    session.wait_for_screen("READ-ONLY PREVIEW • Confirm action");
    session.wait_for_screen("Read-only preview ready");
}
pub(crate) fn confirm(session: &mut Session, mouse: bool, fields: usize) {
    if mouse {
        session.click_visible_action("[Confirm action]")
    } else {
        let mut keys = b"\t".repeat(fields + 2);
        keys.push(b'\r');
        session.send_key(&keys)
    }
}
pub(crate) fn close(session: &mut Session, mouse: bool) {
    if mouse {
        session.click_visible_action("[Keep draft / Close]")
    } else {
        session.send_key(b"\x1b")
    };
    session.wait_for_screen("Composer closed • draft kept locally");
}

#[test]
fn staging_keyboard_exact_file_preview_cancel_stage_and_unstage() {
    staging(false)
}
#[test]
fn staging_mouse_exact_file_preview_cancel_stage_and_unstage() {
    staging(true)
}
fn staging(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("with space.txt", b"old\n", "First saved version");
    std::fs::write(fixture.repo().join("with space.txt"), b"new\n").unwrap();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");
    select_changed(&mut session, mouse, "with space.txt");
    operation(&mut session, mouse, "Stage selected file", 0);
    assert_eq!(fixture.git(&["show", ":with space.txt"]), "old\n");
    close(&mut session, mouse);
    assert_eq!(fixture.git(&["show", ":with space.txt"]), "old\n");
    operation(&mut session, mouse, "Stage selected file", 0);
    preview(&mut session, mouse);
    session.wait_for_screen("Exact file: with space.txt");
    assert_eq!(fixture.git(&["show", ":with space.txt"]), "old\n");
    confirm(&mut session, mouse, 0);
    session.wait_for_screen("Completed • Stage selected file");
    assert_eq!(fixture.git(&["show", ":with space.txt"]), "new\n");
    assert_eq!(
        std::fs::read(fixture.repo().join("with space.txt")).unwrap(),
        b"new\n"
    );
    session.resize(80, 24);
    operation(&mut session, mouse, "Unstage selected file", 1);
    preview(&mut session, mouse);
    assert_eq!(fixture.git(&["show", ":with space.txt"]), "new\n");
    confirm(&mut session, mouse, 0);
    session.wait_for_screen("Completed • Unstage selected file");
    assert_eq!(fixture.git(&["show", ":with space.txt"]), "old\n");
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn local_commit_keyboard_stage_message_hook_failure_and_amend_cancel() {
    commit(false)
}
#[test]
fn local_commit_mouse_stage_message_hook_failure_and_amend_cancel() {
    commit(true)
}

#[test]
fn staging_keyboard_canonical_range_and_hunk_produce_exact_index_content() {
    partial_staging(false)
}
#[test]
fn staging_mouse_canonical_drag_range_and_hunk_produce_exact_index_content() {
    partial_staging(true)
}
fn partial_staging(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    let original = b"top\nkeep\nbottom\nmore\n";
    let working = b"top\nalpha\nbeta\nkeep\nbottom\nmore\ngamma\n";
    fixture.commit_file("file.txt", original, "Saved version");
    std::fs::write(fixture.repo().join("file.txt"), working).unwrap();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");
    if mouse {
        session.click_visible_action("[w Working]")
    } else {
        session.send_key(b"w")
    };
    session.wait_for_screen("+alpha");
    // Both canonical endpoints must arrive before deriving drag geometry; a
    // streamed terminal frame can contain alpha before the following beta row.
    session.wait_for_screen("+beta");
    if mouse {
        let start = session.locate("+alpha").unwrap();
        let end = session.locate("+beta").unwrap();
        session.drag(start, end);
    } else {
        // Canonical working comparison has context top, then added alpha/beta.
        session.send_key(b"\x1b[B\x1b[B\r\x1b[1;2B");
    }
    session.wait_for_screen("Selected 2 canonical lines");
    operation(&mut session, mouse, "Stage selected lines", 2);
    preview(&mut session, mouse);
    session.wait_for_screen("+alpha");
    session.wait_for_screen("+beta");
    assert_eq!(
        fixture.git(&["show", ":file.txt"]),
        String::from_utf8_lossy(original)
    );
    confirm(&mut session, mouse, 0);
    session.wait_for_screen("Completed • Stage selected lines");
    assert_eq!(
        fixture.git(&["show", ":file.txt"]),
        "top\nalpha\nbeta\nkeep\nbottom\nmore\n"
    );
    assert_eq!(
        std::fs::read(fixture.repo().join("file.txt")).unwrap(),
        working
    );
    if mouse {
        session.click_visible_action("[i Staged]")
    } else {
        session.send_key(b"i")
    };
    session.wait_for_screen("+alpha");
    if mouse {
        session.click_visible_action("+beta")
    } else {
        session.send_key(b"\x1b[B\x1b[B\x1b[B\r")
    };
    operation(&mut session, mouse, "Unstage selected lines", 3);
    preview(&mut session, mouse);
    confirm(&mut session, mouse, 0);
    session.wait_for_screen("Completed • Unstage selected lines");
    assert_eq!(
        fixture.git(&["show", ":file.txt"]),
        "top\nalpha\nkeep\nbottom\nmore\n"
    );
    session.resize(80, 24);
    if mouse {
        session.click_visible_action("[w Working]")
    } else {
        session.send_key(b"w")
    };
    session.wait_for_screen("+beta");
    if mouse {
        session.click_visible_action("+beta");
        session.click_visible_action("[h Hunk]")
    } else {
        session.send_key(b"\x1b[B\x1b[B\x1b[B\rh")
    };
    session.wait_for_screen("Selected hunk");
    operation(&mut session, mouse, "Stage selected lines", 2);
    preview(&mut session, mouse);
    confirm(&mut session, mouse, 0);
    session.wait_for_screen("Completed • Stage selected lines");
    assert_eq!(
        fixture.git(&["show", ":file.txt"]),
        String::from_utf8_lossy(working)
    );
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(
        std::fs::read(fixture.repo().join("file.txt")).unwrap(),
        working
    );
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}
fn commit(mouse: bool) {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("a.txt", b"old\n", "First saved version");
    fixture.commit_file("z.txt", b"keep\n", "Other file");
    let head = fixture.git(&["rev-parse", "HEAD"]);
    std::fs::write(fixture.repo().join("a.txt"), b"new\n").unwrap();
    std::fs::write(fixture.repo().join("z.txt"), b"unstaged work to preserve\n").unwrap();
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");
    select_changed(&mut session, mouse, "a.txt");
    operation(&mut session, mouse, "Stage selected file", 0);
    preview(&mut session, mouse);
    confirm(&mut session, mouse, 0);
    session.wait_for_screen("Completed • Stage selected file");
    operation(&mut session, mouse, "Commit staged changes", 4);
    session.paste("Résumé commit\n\nA detailed body");
    session.wait_for_screen("Résumé commit");
    preview(&mut session, mouse);
    session.wait_for_screen("Exact staged content for the next commit:");
    session.wait_for_screen("+new");
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    confirm(&mut session, mouse, 1);
    session.wait_for_screen("Completed • Commit staged changes");
    let committed = fixture.git(&["rev-parse", "HEAD"]);
    assert_ne!(committed, head);
    assert_eq!(
        fixture
            .git(&["show", "--format=%P", "--no-patch", "HEAD"])
            .trim(),
        head.trim()
    );
    assert_eq!(fixture.git(&["show", "HEAD:a.txt"]), "new\n");
    assert_eq!(fixture.git(&["show", "HEAD:z.txt"]), "keep\n");
    assert_eq!(
        std::fs::read(fixture.repo().join("z.txt")).unwrap(),
        b"unstaged work to preserve\n"
    );
    assert!(fixture
        .git(&["show", "--format=%B", "--no-patch", "HEAD"])
        .contains("A detailed body"));
    operation(&mut session, mouse, "Amend latest commit", 5);
    session.paste("Cancelled amend");
    close(&mut session, mouse);
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), committed);
    std::fs::write(fixture.repo().join("a.txt"), b"third\n").unwrap();
    fixture.git(&["add", "a.txt"]);
    let hook = fixture.repo().join(".git/hooks/pre-commit");
    std::fs::write(
        &hook,
        "#!/bin/sh\necho 'FIXTURE HOOK REJECTED COMMIT' >&2\nexit 1\n",
    )
    .unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    session.resize(80, 24);
    if mouse {
        session.click_visible_action("[r Refresh]")
    } else {
        session.send_key(b"r")
    };
    session.wait_for_screen("Repository loaded • read-only exploration");
    operation(&mut session, mouse, "Commit staged changes", 4);
    session.paste("Retain rejected draft");
    preview(&mut session, mouse);
    confirm(&mut session, mouse, 1);
    session.wait_for_screen("FIXTURE HOOK REJECTED");
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), committed);
    assert_eq!(fixture.git(&["show", ":a.txt"]), "third\n");
    assert_eq!(
        std::fs::read(fixture.repo().join("z.txt")).unwrap(),
        b"unstaged work to preserve\n"
    );
    close(&mut session, mouse);
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}
