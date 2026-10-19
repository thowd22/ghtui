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
    // Untracked files start collapsed to a count in the Files panel; u shows them.
    session.wait_for_screen("untracked: 1 hidden · u shows them");
    press(&mut session, mouse, "[u Show untracked]", b"u");
    session.wait_for_screen("untracked: 1 shown · u hides them");
    // Repeated file selection keeps one selection note, not one per activation.
    if mouse {
        session.click_visible_action("new.txt");
        session.wait_for_screen("Selected working file: new.txt");
        session.click_visible_action("new.txt");
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
    session.wait_for_control("[+ Stage]");
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
        // The key bar keeps the direct staging keys at 80 columns.
        session.wait_for_control("[- Unstage]");
        session.click_visible_action("+first");
        session.click_visible_action("[h Hunk]");
        session.wait_for_screen("Selected hunk");
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
    session.wait_for_row(&["Untracked", "untracked", "new.txt"]);
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

/// The Overview as panels (TASK-93): untracked noise collapsed to a count and a toggle,
/// stage, unstage and commit from the Files panel through their preview and confirmation,
/// and a read-only cursor that never changes the working tree or index. 140x42 shows the
/// preview beside the panels; 80x24 stacks them.
/// Clicks the leftmost bordered occurrence of `text`: a Files panel row, never the preview
/// headline (right) or the status line (no border).
fn click_row(session: &mut Session, text: &str) {
    session.wait_for_screen(text);
    let screen = session.screen();
    let (row, column) = screen
        .lines()
        .enumerate()
        .filter_map(|(y, line)| {
            let byte = line.find(text)?;
            line[..byte]
                .contains('│')
                .then(|| (y, line[..byte].chars().count()))
        })
        .min_by_key(|(_, x)| *x)
        .unwrap_or_else(|| panic!("no panel row with {text:?}\n{screen}"));
    session.click(column as u16, row as u16);
}

fn panels_journey(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("file.txt", b"saved\n", "Saved version");
    std::fs::write(fixture.repo().join("file.txt"), b"saved\nedited\n").unwrap();
    for name in ["new.txt", "noise-1.log", "noise-2.log"] {
        std::fs::write(fixture.repo().join(name), format!("{name}\n")).unwrap();
    }
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let status = fixture.git(&["status", "--porcelain=v1", "--untracked-files=all"]);
    let index = std::fs::read(fixture.repo().join(".git/index")).unwrap();
    let mut session = Session::launch(&fixture, 140, 42);
    session.wait_for_screen("Recent commits — saved versions");
    session.wait_for_screen("Files · 4 changed");
    session.wait_for_screen("untracked: 3 hidden · u shows them");
    assert!(!session.screen().contains("noise-1.log"));
    // The preview pane sits beside the panels at 140 columns; the branch line has nothing to
    // open, so its preview has no Open token yet.
    session.wait_for_screen("─ Details ─");

    // Browsing: select the modified file; the preview names it; nothing is written.
    if mouse {
        click_row(&mut session, "file.txt");
    } else {
        session.send_key(b"\x1b[B\r");
    }
    session.wait_for_screen("Selected working file: file.txt");
    session.wait_for_selected_cells("Files ·", &["Modified", "unstaged", "file.txt"]);
    session.wait_for_screen("Worktree  changed");
    session.wait_for_screen("─ Enter Open ─");
    session.wait_for_screen("Enter opens the working diff");
    // Down skips the collapsed untracked files and reaches the commits panel.
    session.send_key(b"\x1b[B");
    session.wait_for_selected_cells(
        "Recent commits — saved versions",
        &[&head[..7], "Saved version"],
    );
    session.wait_for_screen(&format!("Commit    {}", head.trim()));
    assert_eq!(
        fixture.git(&["status", "--porcelain=v1", "--untracked-files=all"]),
        status
    );
    assert_eq!(
        std::fs::read(fixture.repo().join(".git/index")).unwrap(),
        index
    );

    // Show the untracked files, then stage one of them.
    press(&mut session, mouse, "[u Show untracked]", b"u");
    session.wait_for_screen("untracked: 3 shown · u hides them");
    session.wait_for_screen("noise-1.log");
    if mouse {
        click_row(&mut session, "new.txt");
        session.wait_for_screen("Selected working file: new.txt");
        session.click_visible_action("[+ Stage]");
    } else {
        // Branch row, file.txt, then new.txt.
        session.send_key(&b"\x1b[A".repeat(16));
        session.send_key(b"\x1b[B");
        session.wait_for_selected_cells("Files ·", &["Untracked", "new.txt"]);
        session.send_key(b"+");
    }
    session.wait_for_screen("New untracked file: new.txt");
    assert_eq!(fixture.git(&["ls-files", "--", "new.txt"]), "");
    confirm(
        &mut session,
        mouse,
        "Completed • Stage selected file • new.txt",
    );
    assert_eq!(fixture.git(&["show", ":new.txt"]), "new.txt\n");
    session.wait_for_row(&["Added", "staged", "new.txt"]);

    // Unstage it again from its row.
    if mouse {
        click_row(&mut session, "new.txt");
        session.wait_for_screen("Selected working file: new.txt");
        session.click_visible_action("[- Unstage]");
    } else {
        session.send_key(&b"\x1b[A".repeat(16));
        session.send_key(b"\x1b[B");
        session.wait_for_selected_cells("Files ·", &["Added", "new.txt"]);
        session.send_key(b"-");
    }
    confirm(&mut session, mouse, "Completed • Unstage selected file");
    assert_eq!(fixture.git(&["ls-files", "--", "new.txt"]), "");
    assert_eq!(
        std::fs::read(fixture.repo().join("new.txt")).unwrap(),
        b"new.txt\n"
    );

    // Narrow: the panels stack, the preview leaves; stage file.txt and commit it.
    session.resize(80, 24);
    session.wait_for_screen("Recent commits — saved versions");
    assert!(session.locate("Enter Open").is_none());
    if mouse {
        click_row(&mut session, "file.txt");
        session.wait_for_screen("Selected working file: file.txt");
        session.click_visible_action("[+ Stage]");
    } else {
        session.send_key(&b"\x1b[A".repeat(16));
        session.wait_for_selected_cells("Files ·", &["Modified", "file.txt"]);
        session.send_key(b"+");
    }
    confirm(
        &mut session,
        mouse,
        "Completed • Stage selected file • file.txt",
    );
    assert_eq!(fixture.git(&["show", ":file.txt"]), "saved\nedited\n");
    press(&mut session, mouse, "[K Commit]", b"K");
    session.wait_for_screen("LOCAL INTENT • Preview first");
    session.paste("Panels commit");
    session.wait_for_screen("Panels commit");
    press(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("+edited");
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    confirm(&mut session, mouse, "Completed • Commit staged changes");
    assert_eq!(
        fixture
            .git(&["show", "--format=%s", "--no-patch", "HEAD"])
            .trim(),
        "Panels commit"
    );
    assert_eq!(
        fixture
            .git(&["show", "--format=%P", "--no-patch", "HEAD"])
            .trim(),
        head.trim()
    );
    assert_eq!(fixture.git(&["show", "HEAD:file.txt"]), "saved\nedited\n");
    session.wait_for_row(&["Panels commit"]);
    // The untracked files were never added.
    assert_eq!(
        fixture.git(&["ls-files", "--", "noise-1.log", "new.txt"]),
        ""
    );
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn design_panels_v93_keyboard_stage_unstage_commit_and_read_only_browsing() {
    panels_journey(false)
}

#[test]
fn design_panels_v93_mouse_stage_unstage_commit_and_read_only_browsing() {
    panels_journey(true)
}
