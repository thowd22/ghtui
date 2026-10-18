//! Fast daily staging: + / - stage or unstage the hunk under the cursor, an
//! explicitly selected line, or a changed file, with the read-only preview
//! already open; Ctrl-S / [Confirm action] is still the only write. The view
//! stays in the diff being worked through. Effects are read back from Git.
use crate::support::{Fixture, Session};

fn lines(values: &[String]) -> String {
    values.iter().map(|line| format!("{line}\n")).collect()
}

fn press(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    if mouse {
        session.click_visible_action(label)
    } else {
        session.send_key(key)
    }
}

fn confirm(session: &mut Session, mouse: bool, completed: &str) {
    session.wait_for_screen("READ-ONLY PREVIEW • Confirm action");
    press(session, mouse, "[Confirm action]", b"\x13");
    session.wait_for_screen(completed);
}

fn journey(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    let saved: Vec<String> = (1..=20).map(|index| format!("l{index}")).collect();
    fixture.commit_file("file.txt", lines(&saved).as_bytes(), "Saved version");
    let mut working = saved.clone();
    working[1] = "first".into();
    working[18] = "second".into();
    std::fs::write(fixture.repo().join("file.txt"), lines(&working)).unwrap();
    std::fs::write(fixture.repo().join("new.txt"), b"brand new\n").unwrap();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");

    // Nothing is staged yet: Commit explains instead of opening a form.
    press(&mut session, mouse, "[K Commit]", b"K");
    session.wait_for_screen("Nothing is staged yet");
    assert!(!session.screen().contains("LOCAL INTENT"));
    // Repeated file selection keeps one selection note, not one per activation.
    if mouse {
        session.click_visible_action("new.txt");
        session.wait_for_screen("Selected working file: new.txt");
        session.click_visible_action("Untracked [untracked] new.txt");
    } else {
        session.send_key(b"\x1b[B\x1b[B\r");
        session.wait_for_screen("Selected working file: new.txt");
        session.send_key(b"\r");
    }
    session.poll_output(std::time::Duration::from_millis(300));
    assert_eq!(
        session.screen().matches("Selected working file:").count(),
        1
    );

    // Working diff: stage the whole first hunk.
    press(&mut session, mouse, "[w Working]", b"w");
    session.wait_for_screen("+first");
    session.wait_for_screen("+second");
    session.wait_for_screen("[+ Stage]");
    if mouse {
        session.click_visible_action("+first");
        session.click_visible_action("[h Hunk]");
        session.wait_for_screen("Selected hunk");
        session.click_visible_action("[+ Stage]");
    } else {
        // The cursor starts on the comparison title: nothing is guessed.
        session.send_key(b"+");
        session.wait_for_screen("Select a changed file, diff line or hunk first");
        assert!(!session.screen().contains("Stage selected lines"));
        // Cursor on a changed line with no explicit range stages its hunk.
        session.send_key(b"\x1b[B\x1b[B+");
    }
    session.wait_for_screen("Stage selected lines");
    session.wait_for_screen("Ctrl-S: confirm");
    assert_eq!(fixture.git(&["show", ":file.txt"]), lines(&saved));
    confirm(
        &mut session,
        mouse,
        "Completed • Stage selected lines • 2 lines in file.txt",
    );
    let mut index = saved.clone();
    index[1] = "first".into();
    assert_eq!(fixture.git(&["show", ":file.txt"]), lines(&index));
    // Still in Working diff; only the second change remains unstaged.
    session.wait_for_screen("Unstaged edits compared to the next commit");
    session.wait_for_screen("+second");
    assert!(!session.screen().contains("+first"));

    // Stage exactly one selected line: the addition, not its paired removal.
    if mouse {
        session.click_visible_action("+second");
        session.wait_for_screen("Selected change line");
        session.click_visible_action("[+ Stage]");
    } else {
        session.send_key(b"w");
        session.wait_for_screen("+second");
        // Title, then context l16, l17, l18, removal l19, addition second.
        session.send_key(b"\x1b[B\x1b[B\x1b[B\x1b[B\x1b[B\r");
        session.wait_for_screen("Selected change line");
        session.send_key(b"+");
    }
    confirm(
        &mut session,
        mouse,
        "Completed • Stage selected lines • 1 line in file.txt",
    );
    index.insert(19, "second".into());
    assert_eq!(fixture.git(&["show", ":file.txt"]), lines(&index));

    // Unstage the first hunk again from Staged diff, at narrow width.
    session.resize(80, 24);
    press(&mut session, mouse, "[i Staged]", b"i");
    session.wait_for_screen("Staged for the next commit");
    session.wait_for_screen("+first");
    if mouse {
        // The direct controls only paint when they fit; the hunk control and
        // the Local operations route stay clickable.
        assert!(!session.screen().contains("[- Unstage]"));
        session.click_visible_action("+first");
        session.click_visible_action("[h Hunk]");
        session.wait_for_screen("Selected hunk");
        session.resize(120, 40);
        session.click_visible_action("[- Unstage]");
    } else {
        session.send_key(b"\x1b[B-");
    }
    confirm(
        &mut session,
        mouse,
        "Completed • Unstage selected lines • 2 lines in file.txt",
    );
    index[1] = "l2".into();
    assert_eq!(fixture.git(&["show", ":file.txt"]), lines(&index));
    session.wait_for_screen("Staged for the next commit");
    session.resize(120, 40);

    // Overview: stage the new file directly.
    press(&mut session, mouse, "[o Overview]", b"o");
    session.wait_for_screen("Untracked [untracked] new.txt");
    if mouse {
        session.click_visible_action("new.txt");
        session.wait_for_screen("Selected working file:");
        session.click_visible_action("[+ Stage]");
    } else {
        session.send_key(b"\x1b[B\x1b[B+");
    }
    session.wait_for_screen("New untracked file: new.txt");
    session.wait_for_screen("    1 brand new");
    assert_eq!(fixture.git(&["ls-files", "--", "new.txt"]), "");
    confirm(
        &mut session,
        mouse,
        "Completed • Stage selected file • new.txt",
    );
    assert_eq!(fixture.git(&["show", ":new.txt"]), "brand new\n");

    // Commit what is staged.
    press(&mut session, mouse, "[K Commit]", b"K");
    session.wait_for_screen("LOCAL INTENT • Preview first");
    session.paste("Quick staged commit");
    session.wait_for_screen("Quick staged commit");
    press(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("+brand new");
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    confirm(&mut session, mouse, "Completed • Commit staged changes");
    assert_eq!(
        fixture
            .git(&["show", "--format=%P", "--no-patch", "HEAD"])
            .trim(),
        head.trim()
    );
    assert_eq!(
        fixture
            .git(&["show", "--format=%s", "--no-patch", "HEAD"])
            .trim(),
        "Quick staged commit"
    );
    assert_eq!(fixture.git(&["show", "HEAD:file.txt"]), lines(&index));
    assert_eq!(fixture.git(&["show", "HEAD:new.txt"]), "brand new\n");
    // The working file was never rewritten.
    assert_eq!(
        std::fs::read(fixture.repo().join("file.txt")).unwrap(),
        lines(&working).as_bytes()
    );
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn v1_local_staging_keyboard_hunk_line_file_and_commit_with_direct_keys() {
    journey(false)
}

#[test]
fn v1_local_staging_mouse_hunk_line_file_and_commit_with_direct_controls() {
    journey(true)
}
