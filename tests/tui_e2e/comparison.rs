//! Immutable saved-commit comparisons, including both ordered merge parents.
#[path = "original_diff_sides_v1.rs"]
mod original_diff_sides_v1;
use crate::support::{Fixture, Session};
use std::{fs, time::Duration};

#[test]
fn compare_commits_keyboard_typed_endpoints_snapshot_proposed_swap_and_cancel_preserve_git() {
    endpoints_journey(false);
}

#[test]
fn compare_commits_mouse_typed_endpoints_snapshot_proposed_swap_and_cancel_preserve_git() {
    endpoints_journey(true);
}

fn comparison_form(session: &mut Session, mouse: bool) {
    mode(session, mouse, b"C", "[C Compare]");
    session.wait_for_screen("Compare two saved versions");
    session.wait_for_screen("READ-ONLY COMPARISON");
    assert!(!session.screen().contains("[Confirm action]"));
}

fn endpoint(session: &mut Session, mouse: bool, index: usize, reference: &str) {
    let field = if index == 0 { "base" } else { "head" };
    if mouse {
        let label = if index == 0 {
            "A: from (branch, tag or commit)"
        } else {
            "B: to (branch, tag or commit)"
        };
        session.wait_for_screen(label);
        let (_, row) = session.locate(label).unwrap();
        let line = session
            .screen()
            .lines()
            .nth(usize::from(row))
            .unwrap()
            .to_owned();
        let byte = line
            .find("[F4 Choose]")
            .expect("chooser exists on exact field row");
        session.click(line[..byte].chars().count() as u16, row);
    } else {
        if index == 1 {
            session.send_key(b"\x1b[17~"); // F6: next visible field, not a direct handler.
        }
        session.send_key(b"\x1bOS");
    }
    let title = format!("Choose {field} • saved comparison");
    session.wait_for_screen(&title);
    if mouse {
        session.click_visible_action("[Ctrl-U Clear]");
    } else {
        session.send_key(b"\x15");
    }
    session.paste(reference);
    let label = format!("{reference} •"); // The query lacks this delimiter; choose the actual row.
    session
        .try_wait_for_condition(
            |screen| {
                screen
                    .lines()
                    .skip_while(|line| !line.contains(&format!("Filter: {reference}")))
                    .nth(1)
                    .is_some_and(|line| line.contains(&label))
            },
            "filtered exact-reference option immediately below filter",
            Duration::from_secs(8),
        )
        .unwrap();
    session.wait_for_screen(&label);
    if mouse {
        session.click_visible_action(&label);
    } else {
        session.send_key(b"\r");
    }
    session
        .try_wait_for_condition(
            |screen| !screen.contains(&title),
            "exact endpoint picker closed",
            Duration::from_secs(8),
        )
        .unwrap();
}

fn comparison_preview(session: &mut Session, mouse: bool) {
    mode(session, mouse, b"\x10", "[Preview]");
    session.wait_for_screen("Read-only comparison preview ready");
    assert!(!session.screen().contains("[Confirm action]"));
}

fn comparison_open(session: &mut Session, mouse: bool, focused_field: usize) {
    if mouse {
        session.click_visible_action("[Open comparison]");
    } else {
        let mut tabs = b"\t".repeat(5 - focused_field);
        tabs.push(b'\r');
        session.send_key(&tabs);
    }
    session.wait_for_screen("Comparison opened • read-only");
    session
        .try_wait_for_condition(
            |screen| !screen.contains("Compare two saved versions"),
            "comparison form closed after read-only open",
            Duration::from_secs(8),
        )
        .unwrap();
}

fn see_comparison_row(session: &mut Session, mouse: bool, text: &str) {
    for _ in 0..8 {
        if session
            .try_wait_for_condition(
                |screen| screen.contains(text),
                text,
                Duration::from_millis(100),
            )
            .is_ok()
        {
            return;
        }
        if mouse {
            session.wheel(30, 14, true);
        } else {
            session.send_key(b"\x1b[6~");
        }
    }
    session.wait_for_screen(text);
}

fn endpoints_journey(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("root.txt", b"starting bytes\n", "root saved version");
    let root = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    fixture.git(&["switch", "-c", "feature"]);
    fixture.commit_file(
        "target.txt",
        b"target-only addition\n",
        "target saved version",
    );
    let target = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    fixture.git(&["switch", "main"]);
    fixture.commit_file("base.txt", b"base-only addition\n", "base saved version");
    let base = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    assert_eq!(
        fixture.git(&["diff", "--name-only", "main", "feature"]),
        "base.txt\ntarget.txt\n"
    );
    assert_eq!(
        fixture.git(&["diff", "--name-only", &root, "feature"]),
        "target.txt\n"
    );
    fs::write(
        fixture.repo().join("root.txt"),
        b"dirty work survives comparisons\n",
    )
    .unwrap();
    fs::write(
        fixture.repo().join("untracked.txt"),
        b"untracked work survives\n",
    )
    .unwrap();
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let refs = fixture.git(&["show-ref"]);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");
    comparison_form(&mut session, mouse);
    endpoint(&mut session, mouse, 0, "refs/heads/main");
    endpoint(&mut session, mouse, 1, "refs/heads/feature");
    comparison_preview(&mut session, mouse);
    session.wait_for_screen("Target is 1 commits ahead and 1 behind base");
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    if mouse {
        session.click_visible_action("[Keep draft / Close]");
    } else {
        session.send_key(b"\x1b");
    }
    session
        .try_wait_for_condition(
            |screen| !screen.contains("Compare two saved versions"),
            "cancelled comparison form closed",
            Duration::from_secs(8),
        )
        .unwrap();
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]).trim(), base);
    comparison_form(&mut session, mouse);
    session.wait_for_screen(&base);
    session.wait_for_screen(&target);
    comparison_preview(&mut session, mouse);
    comparison_open(&mut session, mouse, 0);
    session.wait_for_screen("Compare saved versions • read-only");
    see_comparison_row(&mut session, mouse, "2 changed files");
    see_comparison_row(&mut session, mouse, "-base-only addition");
    see_comparison_row(&mut session, mouse, "+target-only addition");
    session.resize(80, 24);
    comparison_form(&mut session, mouse);
    mode(&mut session, mouse, b"\x1b[20~", "[F9 Swap A/B]");
    comparison_preview(&mut session, mouse);
    comparison_open(&mut session, mouse, 0);
    see_comparison_row(&mut session, mouse, "+base-only addition");
    see_comparison_row(&mut session, mouse, "-target-only addition");
    comparison_form(&mut session, mouse);
    mode(&mut session, mouse, b"\x1b[20~", "[F9 Swap A/B]"); // Restore A/main → B/feature.
    if mouse {
        session.click_visible_action("[proposed]");
    } else {
        session.send_key(b"\x1b[17~\x1b[17~ "); // F6 twice, then the visible mode choice.
    }
    session.wait_for_screen("proposed");
    comparison_preview(&mut session, mouse);
    comparison_open(&mut session, mouse, 2);
    see_comparison_row(&mut session, mouse, "Proposed branch changes • read-only");
    see_comparison_row(&mut session, mouse, "1 changed files");
    see_comparison_row(&mut session, mouse, "+target-only addition");
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]).trim(), base);
    assert_eq!(fixture.git(&["show-ref"]), refs);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(
        fs::read(fixture.repo().join("root.txt")).unwrap(),
        b"dirty work survives comparisons\n"
    );
    assert_eq!(
        fs::read(fixture.repo().join("untracked.txt")).unwrap(),
        b"untracked work survives\n"
    );
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn commit_changes_keyboard_root_and_ordered_merge_parent_diffs_preserve_git() {
    journey(false);
}

#[test]
fn commit_changes_mouse_root_and_ordered_merge_parent_diffs_preserve_git() {
    journey(true);
}

fn mode(session: &mut Session, mouse: bool, key: &[u8], label: &str) {
    if mouse {
        session.click_visible_action(label);
    } else {
        session.send_key(key);
    }
}

fn journey(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file(
        "root.txt",
        b"first saved content\n",
        "Root comparison fixture",
    );
    let root = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    fixture.git(&["switch", "-c", "feature"]);
    fixture.commit_file(
        "feature.txt",
        b"feature-only line\n",
        "Feature saved version",
    );
    let feature = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    fixture.git(&["switch", "main"]);
    fixture.commit_file("main.txt", b"main-only line\n", "Main saved version");
    let main = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    fixture.git(&[
        "merge",
        "--no-ff",
        "feature",
        "-m",
        "Merge comparison fixture",
    ]);
    let merge = fixture.git(&["rev-parse", "HEAD"]);
    fs::write(fixture.repo().join("root.txt"), b"dirty bytes must stay\n").unwrap();
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let refs = fixture.git(&["show-ref"]);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");
    mode(&mut session, mouse, b"d", "[d Changes]");
    session.wait_for_screen("Saved commit changes");
    session.wait_for_screen("Compare to first parent (1):");
    session.wait_for_screen(&main);
    session.wait_for_screen("+feature-only line");
    assert!(!session.screen().contains("+main-only line"));
    if mouse {
        session.click_visible_action("Compare to parent 2:");
    } else {
        session.send_key(b"\x1b[B\x1b[B\r");
    }
    session.wait_for_screen("Compared parent 2");
    session.wait_for_screen(&feature);
    session.wait_for_screen("+main-only line");
    assert!(!session.screen().contains("+feature-only line"));
    session.resize(80, 24);
    mode(&mut session, mouse, b"g", "[g Graph]");
    session.open_saved_graph_subject(mouse, "Root comparison fixture", &root);
    // At 80 columns the one-row header keeps Back only; the crumb shows when there is room.
    session.resize(120, 40);
    session.wait_for_screen("Repository • Commit changes");
    session.resize(80, 24);
    mode(&mut session, mouse, b"D", "[D Details]");
    session.wait_for_screen("root commit:");
    mode(&mut session, mouse, b"d", "[d Changes]");
    session.wait_for_screen("Root commit — compared to an empty starting tree");
    session.wait_for_screen("+first saved content");
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), merge);
    assert_eq!(fixture.git(&["rev-parse", "main"]), merge);
    assert_eq!(
        fixture.git(&["rev-parse", "feature"]),
        format!("{feature}\n")
    );
    assert_eq!(fixture.git(&["rev-parse", &root]), format!("{root}\n"));
    assert_eq!(fixture.git(&["show-ref"]), refs);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(
        fs::read(fixture.repo().join("root.txt")).unwrap(),
        b"dirty bytes must stay\n"
    );
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}
