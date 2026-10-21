//! Compiled-binary file provenance journeys with independent real Git effect checks. A file's
//! history is the History view with the file's `path:` chip (TASK-99); blame, the responsible
//! commit in History, its introduced diff and the original file keep their own views.
#[path = "graph_paging_v1.rs"]
mod graph_paging_v1;
#[path = "original_saved_navigation_v1.rs"]
mod original_saved_navigation_v1;
use crate::support::{history_fixture::history_filter_row, Fixture, Session};
use std::{
    fs,
    time::{Duration, Instant},
};
fn choose(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    if mouse {
        session.click_visible_action(label);
    } else {
        session.send_key(key);
    }
}
/// A key-bar control that leaves the bar by priority at some widths and states. The keyboard
/// presses its key; the mouse clicks the settled key-bar token when it is painted and otherwise
/// picks the same action explicitly in Commands, which lists every key-bar action.
fn choose_bar_or_palette(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    if !mouse {
        session.send_key(key);
        return;
    }
    session.wait_for_settled_footer();
    if session
        .try_wait_for_control(label, Duration::from_millis(300))
        .is_some()
    {
        session.click_visible_action(label);
        return;
    }
    // Commands names a control by its caption; a multi-character key is not part of it.
    let control = crate::support::Control::parse(label);
    match control.key.as_deref() {
        Some(key) if key.chars().count() > 1 => session.invoke_via_palette(true, &control.text),
        _ => session.invoke_via_palette(true, label),
    }
}
/// A disabled key-bar control is not painted; its key (or its Commands entry, for the mouse)
/// still answers with the visible reason and no effect.
fn explain_disabled(session: &mut Session, mouse: bool, label: &str, key: &[u8], reason: &str) {
    session.wait_for_settled_footer();
    session.assert_control_absent(label);
    if mouse {
        session.invoke_via_palette(true, label);
        session.wait_for_screen(reason);
    } else {
        session.press_disabled_key(key, reason);
    }
    session.wait_for_screen("Action unavailable");
}
fn select_row(session: &mut Session, mouse: bool, label: &str) {
    if mouse {
        session.click_visible_action(label);
        return;
    }
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        if let Some((column, row)) = session.locate(label) {
            if session.cell_inverted(row, column) {
                session.send_key(b"\r");
                return;
            }
        }
        assert!(
            Instant::now() < deadline,
            "Canonical row never keyboard-selected: {label}\n{}",
            session.screen()
        );
        session.send_key(b"\x1b[B");
        session.poll_output(Duration::from_millis(50));
    }
}
/// Moves History's `▌` bar to the commit `sha`: Down/Up presses, or a click on its SHA cell.
fn select_history(session: &mut Session, mouse: bool, sha: &str) {
    session.wait_for_history_ready();
    if mouse {
        session.click_history_row(sha);
    } else {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            session.poll_output(Duration::from_millis(30));
            let rows = session.history_rows();
            let selected = rows.iter().position(|row| row.selected);
            let wanted = rows.iter().position(|row| sha.starts_with(&row.sha));
            match (selected, wanted) {
                (Some(at), Some(wanted)) if at == wanted => break,
                (Some(at), Some(wanted)) if wanted < at => session.send_key(b"\x1b[A"),
                _ => session.send_key(b"\x1b[B"),
            }
            assert!(
                Instant::now() < deadline,
                "{sha} never reached by the keyboard\n{}",
                session.screen()
            );
            session.poll_output(Duration::from_millis(60));
        }
    }
    session.wait_for_history_selected(sha);
}
/// The file's History: the `path:` chip in the filter row, its match count in the border and
/// the bar on the file's latest change at or below the opened revision.
fn wait_path_history(session: &mut Session, path: &str, matches: usize, latest: &str) {
    let chip = format!("path: {path}");
    session.wait_for_cursorless_condition(
        |screen| {
            history_filter_row(screen).is_some_and(|row| {
                row.removable.iter().any(|(label, _)| *label == chip)
                    && row.matches == Some(matches)
            })
        },
        &format!("History with {chip} and {matches} matches"),
        Duration::from_secs(10),
    );
    session.wait_for_history_ready();
    session.wait_for_history_selected(latest);
    session.wait_for_settled_footer();
}
/// Global Back (Alt-← or the header's `Alt-← Back` token).
fn global_back(session: &mut Session, mouse: bool) {
    session.wait_for_settled_footer();
    choose(session, mouse, "Alt-← Back", b"\x1b[1;3D");
}
fn tree_file(session: &mut Session, mouse: bool, name: &str) {
    session.wait_for_settled_footer();
    choose(session, mouse, "[t Tree]", b"t");
    session.wait_for_screen("Saved file tree");
    select_row(session, mouse, name);
    session.wait_for_screen(&format!("{name} at saved version"));
}
fn provenance_row(session: &mut Session, mouse: bool, label: &str) {
    select_row(session, mouse, label);
    session.wait_for_screen("File/line provenance — exact responsible saved version");
}
fn blame_row(session: &mut Session, mouse: bool, text: &str) {
    // A file preview or introduced diff can retain the same text during a
    // transition. Require the actual prepared Blame row's actor separator.
    let label = format!("│ {text}");
    session.wait_for_screen(&label);
    if !mouse {
        session.send_key(b"\x1b[H");
    }
    provenance_row(session, mouse, &label);
}
fn scroll_diff_to(session: &mut Session, mouse: bool, label: &str) {
    // A narrow terminal need not show an entire patch and its source labels at
    // once. Navigate the actual body, retaining the earlier source assertions.
    let (column, row) = session
        .locate("first parent")
        .expect("the independently checked comparison source must be visible");
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        if session.locate(label).is_some() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Patch line never became visible through body navigation: {label}\n{}",
            session.screen()
        );
        if mouse {
            session.wheel(column, row, true);
        } else {
            session.send_key(b"\x1b[B");
        }
        session.poll_output(Duration::from_millis(50));
    }
}
#[test]
fn file_history_keyboard_old_revision_rename_blame_graph_diff_actor_roles_pages_and_back() {
    happy(false);
}
#[test]
fn file_history_mouse_old_revision_rename_blame_graph_diff_actor_roles_pages_and_back() {
    happy(true);
}
fn happy(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    let mut lines = (1..=145)
        .map(|line| format!("source line {line:03}"))
        .collect::<Vec<_>>();
    fixture.commit_file(
        "old name.txt",
        format!("{}\n", lines.join("\n")).as_bytes(),
        "Original complete saved file",
    );
    fixture.git(&["config", "user.name", "Distinct Integrator"]);
    fixture.git(&["config", "user.email", "integrator@example.invalid"]);
    lines[1] = "responsible second line".into();
    fs::write(
        fixture.repo().join("old name.txt"),
        format!("{}\n", lines.join("\n")),
    )
    .unwrap();
    fixture.git(&["add", "old name.txt"]);
    fixture.git(&[
        "commit",
        "--author=Line Writer <writer@example.invalid>",
        "-m",
        "Responsible source line change",
    ]);
    let responsible = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    for change in 1..=12 {
        lines[144] = format!("history edit {change:02}");
        fixture.commit_file(
            "old name.txt",
            format!("{}\n", lines.join("\n")).as_bytes(),
            &format!("Historical edit {change:02}"),
        );
    }
    fixture.git(&["mv", "old name.txt", "new name.txt"]);
    fixture.git(&["commit", "-m", "Chosen snapshot after rename"]);
    let chosen = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    lines[1] = "future source line outside chosen revision".into();
    fixture.commit_file(
        "new name.txt",
        format!("{}\n", lines.join("\n")).as_bytes(),
        "Future version must not rewrite chosen provenance",
    );
    let dirty = b"uncommitted user checkout bytes\n";
    fs::write(fixture.repo().join("new name.txt"), dirty).unwrap();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let refs = fixture.git(&["show-ref"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");
    session.wait_for_settled_footer();
    choose(&mut session, mouse, "[g History]", b"g");
    session.wait_for_history_ready();
    session.wait_for_screen("Read-only · 16 commits loaded");
    select_history(&mut session, mouse, &chosen);
    tree_file(&mut session, mouse, "new name.txt");
    session.wait_for_screen(&format!("Viewed: {chosen}"));
    session.wait_for_screen("responsible second line");
    assert!(!session.screen().contains("future source line outside"));
    // Saved-file line actions use exact immutable BlameAt coordinates too.
    provenance_row(&mut session, mouse, "responsible second line");
    session.wait_for_screen("Original line: 2 • chosen file line: 2");
    session.wait_for_screen("Original path: old name.txt");
    session.wait_for_screen(&responsible);
    choose(&mut session, mouse, "[u File back]", b"u");
    session.wait_for_screen("Blame lines 1–100 of 145");
    choose(&mut session, mouse, "[u File back]", b"u");
    session.wait_for_screen("new name.txt:1 / saved version");
    session.wait_for_screen(&chosen);
    // H (the mouse clicks `f File history`): History of every branch with the file's path
    // chip. The chip follows the rename (`git log --follow`), so all 16 commits match: the
    // 14 changes to `old name.txt`, the rename and the future version. The bar lands on the
    // chosen revision's own change, not the newer one above it.
    session.wait_for_settled_footer();
    choose(&mut session, mouse, "[f File history]", b"H");
    wait_path_history(&mut session, "new name.txt", 16, &chosen);
    let future = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    let rows = session.history_rows();
    let row = |sha: &str, what: &str| {
        rows.iter()
            .find(|row| sha.starts_with(&row.sha))
            .cloned()
            .unwrap_or_else(|| panic!("{what} row not visible\n{}", session.screen()))
    };
    let latest = row(&future, "newer");
    assert!(!latest.message.contains("was "), "{latest:?}");
    let rename = row(&chosen, "rename");
    assert!(
        rename.selected && rename.message.contains("renamed from"),
        "the selected rename row says where the file came from: {rename:?}"
    );
    let before = row(&responsible, "pre-rename");
    assert!(
        before.message.starts_with("was old name.txt"),
        "a pre-rename row leads with the old name: {before:?}"
    );
    session.wait_for_cursorless_condition(
        |screen| {
            screen
                .contents()
                .lines()
                .any(|line| line.contains("Path") && line.contains("old name.txt"))
        },
        "the detail's Path row names the old path",
        Duration::from_secs(8),
    );
    // The Esc filter clear is also offered while the chip is set.
    session.wait_for_control("[Esc Clear filter]");
    global_back(&mut session, mouse);
    session.wait_for_screen("new name.txt:1 / saved version");
    session.wait_for_screen(&chosen);
    session.wait_for_settled_footer();
    choose(&mut session, mouse, "[Y Blame]", b"Y");
    session.wait_for_screen("Blame lines 1–100 of 145");
    session.wait_for_screen("moved/copied line detection (-M/-C) is off");
    choose(&mut session, mouse, "[N Next]", b"N");
    session.wait_for_screen("Blame lines 101–145");
    session.resize(80, 24);
    choose(&mut session, mouse, "[P Previous]", b"P");
    session.wait_for_screen("Blame lines 1–100");
    blame_row(&mut session, mouse, "responsible second line");
    session.wait_for_screen("Original line: 2 • chosen file line: 2");
    session.wait_for_screen("Original path: old name.txt");
    session.wait_for_screen("Line Writer");
    session.wait_for_screen("Distinct Integrator");
    session.wait_for_screen(&responsible);
    choose_bar_or_palette(&mut session, mouse, "[a Author]", b"a");
    session.wait_for_screen("Actor profile — Git author");
    session.wait_for_screen("Line Writer");
    session.wait_for_screen("GitHub account association: unavailable");
    choose(&mut session, mouse, "[u File back]", b"u");
    session.wait_for_screen("Line blame");
    blame_row(&mut session, mouse, "responsible second line");
    // G opens the responsible commit in History; Back returns to the attributed line.
    choose(&mut session, mouse, "[G Commit in History]", b"G");
    session.wait_for_history_ready();
    session.wait_for_history_selected(&responsible);
    session.wait_for_screen("Responsible source line change");
    global_back(&mut session, mouse);
    session.wait_for_screen("Original line: 2 • chosen file line: 2");
    session.wait_for_screen(&responsible);
    choose(&mut session, mouse, "[u File back]", b"u");
    session.wait_for_screen("Blame lines 1–100");
    session.wait_for_screen(&chosen);
    blame_row(&mut session, mouse, "responsible second line");
    choose(&mut session, mouse, "[D Introduced diff]", b"D");
    session.wait_for_screen("Saved commit changes");
    session.wait_for_screen("first parent");
    session.wait_for_screen(&responsible);
    scroll_diff_to(&mut session, mouse, "+responsible second line");
    session.wait_for_screen("+responsible second line");
    choose(&mut session, mouse, "[u File back]", b"u");
    session.wait_for_screen("Blame lines 1–100");
    blame_row(&mut session, mouse, "responsible second line");
    choose(&mut session, mouse, "[F Original file]", b"F");
    session.wait_for_screen("Original immutable");
    session.wait_for_screen("old name.txt");
    session.wait_for_screen("responsible second line");
    assert_eq!(
        fixture
            .git(&["show", &format!("{responsible}:old name.txt")])
            .lines()
            .nth(1),
        Some("responsible second line")
    );
    assert_eq!(
        fixture
            .git(&[
                "blame",
                "--line-porcelain",
                "-L",
                "2,2",
                &chosen,
                "--",
                "new name.txt"
            ])
            .lines()
            .next()
            .unwrap()
            .split_whitespace()
            .next(),
        Some(responsible.as_str())
    );
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.git(&["show-ref"]), refs);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(fixture.git(&["status", "--porcelain=v2", "-z"]), status);
    assert_eq!(
        fs::read(fixture.repo().join("new name.txt")).unwrap(),
        dirty
    );
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn file_history_keyboard_binary_empty_lfs_limits_do_not_fabricate_provenance() {
    limits(false);
}
#[test]
fn file_history_mouse_binary_empty_lfs_limits_do_not_fabricate_provenance() {
    limits(true);
}
fn limits(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("binary", b"binary\0payload", "Saved binary");
    fixture.commit_file("empty", b"", "Saved empty text");
    fixture.commit_file("pointer",b"version https://git-lfs.github.com/spec/v1\noid sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nsize 5000\n","Saved LFS pointer");
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let trace = fixture.root.join("unsupported-content-git-trace.jsonl");
    let mut session = Session::launch_binary(
        std::path::Path::new(env!("CARGO_BIN_EXE_ghtui")),
        &fixture,
        120,
        40,
        &[("GIT_TRACE2_EVENT", trace.to_str().unwrap())],
    );
    let children = || {
        fs::read_to_string(&trace)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .filter(|event| event["event"] == "start")
            .count()
    };
    session.wait_for_screen("Recent commits");
    for (name, expected) in [
        ("binary", "binary/LFS"),
        ("empty", "Empty saved text"),
        ("pointer", "binary/LFS"),
    ] {
        tree_file(&mut session, mouse, name);
        choose(&mut session, mouse, "[Y Blame]", b"Y");
        session.wait_for_screen(expected);
        assert!(!session.screen().contains("Original line:"));
        assert!(!session.screen().contains("Blame lines"));
        for (label, key) in [
            ("[G Commit in History]", b"G".as_slice()),
            ("[D Introduced diff]", b"D".as_slice()),
            ("[F Original file]", b"F".as_slice()),
        ] {
            session.wait_for_screen(label);
            let before = children();
            assert!(before > 0, "The actual unsupported object was read");
            choose(&mut session, mouse, label, key);
            session.wait_for_screen("Select an attributed line before navigating its commit");
            assert_eq!(children(), before, "An explanation cannot read a target");
            assert!(!session.screen().contains("Original line:"));
            choose(&mut session, mouse, "[Esc Close]", b"\x1b");
            session.wait_for_overlay_close("Action unavailable • explanation only");
            session.wait_for_screen(expected);
        }
        // H (the mouse clicks `f File history`): History with the file's path chip and the bar
        // on the commit that saved it.
        let saved = fixture
            .git(&["log", "-1", "--format=%H", "HEAD", "--", name])
            .trim()
            .to_owned();
        session.wait_for_settled_footer();
        choose(&mut session, mouse, "[f File history]", b"H");
        wait_path_history(&mut session, name, 1, &saved);
        session.wait_for_screen("Read-only · 3 commits loaded");
        // Back to HEAD's row, so the next file's tree is HEAD's again.
        select_history(&mut session, mouse, head.trim());
        session.resize(80, 24);
        session.wait_for_main_frame();
        session.wait_for_history_ready();
    }
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(fixture.git(&["status", "--porcelain=v2", "-z"]), status);
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn file_history_keyboard_shallow_boundary_disabled_parent_diff_and_back_are_read_only() {
    shallow(false);
}
#[test]
fn file_history_mouse_shallow_boundary_disabled_parent_diff_and_back_are_read_only() {
    shallow(true);
}
fn shallow(mouse: bool) {
    let origin = Fixture::new();
    origin.init_git();
    origin.commit_file("file.txt", b"earlier original\n", "Older source version");
    origin.commit_file(
        "file.txt",
        b"shallow chosen line\n",
        "Shallow visible saved version",
    );
    let fixture = Fixture::new();
    fixture.git(&[
        "-c",
        "protocol.file.allow=always",
        "clone",
        "--no-local",
        "--depth=1",
        &format!("file://{}", origin.repo().display()),
        ".",
    ]);
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let refs = fixture.git(&["show-ref"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let shallow = fs::read(fixture.repo().join(".git/shallow")).unwrap();
    let bytes = fs::read(fixture.repo().join("file.txt")).unwrap();
    let mut session = Session::launch(&fixture, 80, 24);
    session.wait_for_screen("Recent commits");
    tree_file(&mut session, mouse, "file.txt");
    choose(&mut session, mouse, "[Y Blame]", b"Y");
    session.wait_for_screen("SHALLOW:");
    session.wait_for_screen("Blame lines 1–1 of 1");
    provenance_row(&mut session, mouse, "shallow chosen line");
    session.wait_for_screen("BOUNDARY:");
    session.wait_for_screen(head.trim());
    choose(&mut session, mouse, "[D Introduced diff]", b"D");
    session.wait_for_screen("Action unavailable");
    session.wait_for_screen("Shallow boundary:");
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_overlay_close("Action unavailable • explanation only");
    session.wait_for_screen("Original path: file.txt");
    choose(&mut session, mouse, "[G Commit in History]", b"G");
    session.wait_for_history_ready();
    session.wait_for_history_selected(head.trim());
    session.wait_for_screen("Read-only · 1 commit loaded");
    session.wait_for_screen("Shallow history boundary");
    // The message is cut to the 80-column table's MESSAGE column.
    session.wait_for_screen("Shallow visible saved v");
    assert!(!session.screen().contains("Older source version"));
    global_back(&mut session, mouse);
    session.wait_for_screen("Original path: file.txt");
    choose(&mut session, mouse, "[u File back]", b"u");
    session.wait_for_screen("Blame lines 1–1 of 1");
    session.wait_for_screen("shallow chosen line");
    if mouse {
        for _ in 0..3 {
            session.wheel(20, 14, false);
        }
    } else {
        session.send_key(b"\x1b[5~");
    }
    session.wait_for_screen("Line blame");
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.git(&["show-ref"]), refs);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(
        fs::read(fixture.repo().join(".git/shallow")).unwrap(),
        shallow
    );
    assert_eq!(fs::read(fixture.repo().join("file.txt")).unwrap(), bytes);
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}
