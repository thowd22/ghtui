//! Actual binary/keys/mouse. Genuine Git-built M/H trees/blobs; exact offline
//! gh protocol, independent full Git/SQL/config and real terminal restoration.
use super::*;
#[path = "file_comments_native_return_v75.rs"]
mod file_comments_v75;
#[path = "scoped_pr_native_inline_v1.rs"]
mod inline_entry;
#[path = "native_external_pr_diff_v4.rs"]
mod native_external_pr_diff_v4;
#[path = "scoped_pr_native_physical_slot_v65.rs"]
mod physical_slot_v65;
#[path = "scoped_pr_native_threads_v3.rs"]
mod threads_v3;
use crate::support::owned_pty_processes_v1::{process_identity, OwnedPtySession};
const NATIVE: &str = "Native PR diff";
fn nready(session: &mut Session, text: &str) {
    session.wait_for_cursorless_condition(
        |frame| {
            let s = frame.contents();
            s.contains(NATIVE)
                && s.contains(text)
                && s.contains("[o Options]")
                && !s.contains("NOT CURRENT")
                && !s.contains("previous preview (not current)")
                && !s.contains("preparing")
        },
        "completed current genuine native PR patch",
        Duration::from_secs(8),
    );
}
fn ack(session: &mut Session, after: usize, predicate: impl Fn(&str) -> bool, label: &str) {
    session.wait_for_new_cursorless_condition(
        after,
        |frame| predicate(&frame.contents()),
        label,
        Duration::from_secs(8),
    );
}
fn act(session: &mut Session, mouse: bool, title: &str, label: &str, key: &[u8]) {
    let action = label;
    // Patch/full-file panes intentionally repeat some controls at the top and
    // bottom. Keep the observer's unique owned-box rule: identify the genuine
    // footer with its adjacent controls instead of guessing between duplicates.
    let label = if title == NATIVE {
        match label {
            "[o Options]" => {
                if session
                    .completed_cursorless_screen()
                    .is_some_and(|frame| frame.contents().contains("FULL "))
                {
                    "[o Options] [Esc Close]"
                } else {
                    "[o Options] [q Quit]"
                }
            }
            "[Esc Close]" => "[Esc Close] [o Options] [q Quit]",
            "[b Back patch]" => "[b Back patch] [o Options]",
            _ => label,
        }
    } else {
        label
    };
    // The shorter text also occurs in the ready status. The actual border
    // title has a bullet after "diff", including its bounded-width variant.
    let owner = if title == NATIVE {
        "Native PR diff •"
    } else {
        title
    };
    let point = session.wait_for_owned_completed_action(owner, label, &[]);
    let before = session.completed_cursorless_end();
    if mouse {
        session.click(point.0, point.1);
    } else {
        session.send_key(key);
    }
    if title == NATIVE && action == "[o Options]" {
        ack(
            session,
            before,
            |screen| screen.contains("[o Back to diff]") || screen.contains("[o Back to file]"),
            "actual Options page replaces the previous body before its next input",
        );
    } else if title == NATIVE && action == "[o Back to diff]" {
        ack(
            session,
            before,
            |screen| screen.contains("[o Options]") && !screen.contains("[o Back to diff]"),
            "actual patch page replaces Options before its next input",
        );
    }
}
fn selected(screen: &str, text: &str) -> bool {
    screen
        .lines()
        .any(|line| line.starts_with("│>") && line.contains(text))
}
fn no_intermediate_images(session: &Session, mouse: bool) {
    let mut parser =
        vt100::Parser::new(if mouse { 24 } else { 40 }, if mouse { 80 } else { 120 }, 0);
    let mut frames = crate::support::cursorless_frames::CursorlessFrames::default();
    let mut observed = 0;
    for (index, byte) in session.transcript.iter().enumerate() {
        frames.process(&mut parser, std::slice::from_ref(byte), index);
        if frames.end > observed {
            observed = frames.end;
            assert!(!frames.last.as_ref().unwrap().contents().contains("Immutable file images"),"actual completed draw must never publish an intermediate image overlay during direct Files-v");
        }
    }
    assert!(
        observed > 0,
        "requires actual completed VT draws, not empty transcript"
    );
}
fn focus(session: &mut Session, mouse: bool, text: &str) {
    let before = session.completed_cursorless_end();
    if mouse {
        let point =
            crate::support::locate_screen(session.completed_cursorless_screen().unwrap(), text)
                .unwrap();
        session.click(point.0, point.1);
        ack(
            session,
            before,
            |s| selected(s, text),
            "actual selected remote canonical member",
        );
        return;
    }
    let path = session
        .completed_cursorless_screen()
        .unwrap()
        .contents()
        .lines()
        .find_map(|line| {
            line.split_once("File 1/1: ")
                .map(|(_, rest)| rest.trim_end_matches('│').trim().to_owned())
        })
        .unwrap();
    session.send_key(b"\x1b[H");
    ack(
        session,
        before,
        |s| selected(s, &path),
        "original Home header after key",
    );
    for _ in 0..24 {
        let screen = session.completed_cursorless_screen().unwrap().contents();
        if selected(&screen, text) {
            return;
        }
        let rows = screen.lines().collect::<Vec<_>>();
        let at = rows.iter().position(|row| row.starts_with("│>")).unwrap();
        let next = rows
            .get(at + 1)
            .unwrap()
            .trim_start_matches('│')
            .trim_end_matches('│')
            .trim()
            .to_owned();
        assert!(!next.is_empty() && !next.starts_with('['));
        let before = session.completed_cursorless_end();
        session.send_key(b"\x1b[B");
        ack(
            session,
            before,
            |s| selected(s, &next),
            "exact predicted next canonical row",
        );
    }
    panic!("actual canonical changed member not reached");
}
fn readonly_git(fixture: &Fixture) -> GitState {
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let state = GitState {
        head: fixture.git(&["rev-parse", "HEAD"]),
        refs: fixture.git(&["show-ref"]),
        index: index.clone(),
        status: fixture.git(&["--no-optional-locks", "status", "--porcelain=v2", "-z"]),
        staged_diff: fixture.git(&[
            "-c",
            "diff.autoRefreshIndex=false",
            "diff",
            "--cached",
            "--binary",
            "--no-ext-diff",
        ]),
        working_diff: fixture.git(&[
            "-c",
            "diff.autoRefreshIndex=false",
            "diff",
            "--binary",
            "--no-ext-diff",
        ]),
        dirty: fs::read(fixture.repo().join("dirty界.txt")).unwrap(),
        staged: fs::read(fixture.repo().join("staged.txt")).unwrap(),
        untracked: fs::read(fixture.repo().join("scratch.txt")).unwrap(),
    };
    assert_eq!(
        fs::read(fixture.repo().join(".git/index")).unwrap(),
        index,
        "independent observer is itself read-only"
    );
    state
}
fn full(session: &mut Session, side: &str, text: &str) {
    session.wait_for_cursorless_condition(
        |frame| {
            let s = frame.contents();
            s.contains(NATIVE)
                && s.contains(&format!("FULL {side} file"))
                && s.contains("Full-file context ready")
                && s.contains(text)
                && s.contains("[b Back patch]")
        },
        "exact verified PR full-file bytes",
        Duration::from_secs(8),
    );
}
fn source(session: &mut Session, mouse: bool, h: &history::GitImagesV1) {
    act(session, mouse, NATIVE, "[I Source]", b"I");
    let mut seen = String::new();
    let title = "Native PR source • read-only";
    for page in 0..6 {
        let body = session.wait_for_overlay_body(title);
        seen.push_str(&body);
        let compact = seen
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .collect::<String>();
        if compact.contains(&h.m)
            && compact.contains(&h.h)
            && compact.contains(&h.b)
            && compact.contains("user-id:42")
            && seen.contains("Patch digest:")
        {
            break;
        }
        assert!(
            page < 5,
            "actual source body must expose all original identities"
        );
        let before = session.completed_cursorless_end();
        if mouse {
            let point = session.wait_for_owned_completed_action(title, "[Esc Close]", &[]);
            session.wheel(point.0, point.1.saturating_sub(2), true);
        } else {
            session.send_key(b"\x1b[6~");
        }
        session.wait_for_new_cursorless_condition(
            before,
            |frame| crate::support::overlay_body(frame, title).is_some_and(|next| next != body),
            "actual owned Source body moves, not repaint",
            Duration::from_secs(8),
        );
    }
    let before = session.completed_cursorless_end();
    session.send_key(b"FVb");
    ack(
        session,
        before,
        |screen| {
            screen.contains(title)
                && !screen.contains("FULL Old file")
                && !screen.contains("FULL New file")
        },
        "Source owns completed input frame and captures underlay keys",
    );
    let before = session.completed_cursorless_end();
    act(session, mouse, title, "[Esc Close]", b"\x1b");
    ack(
        session,
        before,
        |screen| screen.contains(NATIVE) && !screen.contains(title),
        "Source overlay closes through its owned control",
    );
}
#[test]
fn scoped_pr_native_diff_keyboard_direct_files_ranges_full_context_source_back_readonly() {
    happy(false);
}
#[test]
fn scoped_pr_native_diff_mouse_direct_files_drag_full_context_source_back_readonly() {
    happy(true);
}
fn happy(mouse: bool) {
    let old = (1..=100)
        .map(|n| format!("line{n:03} unchanged\n"))
        .collect::<String>();
    let new = old.replace(
        "line050 unchanged",
        &format!("line050 CHANGED {} WRAP_END", "WRAP_WITNESS ".repeat(12)),
    );
    let (fixture, h, steps) = fixture_with_bytes(old.as_bytes(), new.as_bytes(), |_| {});
    let git = readonly_git(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    files_selected(&mut session, &fixture, mouse);
    let sql = sql_state(&fixture);
    let trace = git_commands(&fixture);
    choose(&mut session, mouse, "[v Native diff]", b"v");
    session.wait_for_command_count(&fixture, 58);
    nready(&mut session, "line050 CHANGED");
    no_intermediate_images(&session, mouse);
    assert!(session.screen().contains("one verified selected file"));
    session.resize_owned_modal(120, 40, NATIVE, &["[Esc Close]", "[o Options]"]);
    nready(&mut session, "line050 CHANGED");
    focus(&mut session, mouse, "line050 unchanged");
    if mouse {
        let first = crate::support::locate_screen(
            session.completed_cursorless_screen().unwrap(),
            "line050 unchanged",
        )
        .unwrap();
        let last = crate::support::locate_screen(
            session.completed_cursorless_screen().unwrap(),
            "line050 CHANGED",
        )
        .unwrap();
        session.drag(first, last);
    } else {
        session.send_key(b" ");
        nready(&mut session, "Selected 1");
        session.send_key(b"\x1b[1;2B");
    }
    nready(&mut session, "Selected 2");
    act(&mut session, mouse, NATIVE, "[o Options]", b"o");
    act(&mut session, mouse, NATIVE, "[l Layout]", b"l");
    act(&mut session, mouse, NATIVE, "[o Back to diff]", b"o");
    session.wait_for_cursorless_condition(
        |frame| {
            let screen = frame.contents();
            screen.contains("Native PR diff ready")
                && screen.contains("Selected 2")
                && !screen.contains("NOT CURRENT")
                && screen.lines().any(|line| {
                    line.contains("line050 unchanged") && line.contains("line050 CHANGED")
                })
        },
        "actual shared split projection pairs original Old/New on one visible row",
        Duration::from_secs(8),
    );
    assert!(
        !session
            .completed_cursorless_screen()
            .unwrap()
            .contents()
            .contains("WRAP_END"),
        "unwrapped split clips the genuine long New line"
    );
    act(&mut session, mouse, NATIVE, "[o Options]", b"o");
    act(&mut session, mouse, NATIVE, "[w Wrap]", b"w");
    act(&mut session, mouse, NATIVE, "[o Back to diff]", b"o");
    nready(&mut session, "WRAP_END");
    assert!(
        session
            .completed_cursorless_screen()
            .unwrap()
            .contents()
            .contains("Selected 2"),
        "wrapping reveals genuine long-line suffix without changing canonical selection"
    );
    act(&mut session, mouse, NATIVE, "[o Options]", b"o");
    act(&mut session, mouse, NATIVE, "[a Stage disabled]", b"a");
    session.wait_for_cursorless_condition(
        |frame| {
            frame
                .contents()
                .contains("no local index/worktree authority")
        },
        "disabled remote Stage explains without queue/write",
        Duration::from_secs(8),
    );
    act(&mut session, mouse, NATIVE, "[o Back to diff]", b"o");
    nready(&mut session, "Selected 2");
    source(&mut session, mouse, &h);
    nready(&mut session, "Selected 2");
    act(&mut session, mouse, NATIVE, "[o Options]", b"o");
    act(&mut session, mouse, NATIVE, "[F Full Old]", b"F");
    full(&mut session, "Old", "line001 unchanged");
    act(&mut session, mouse, NATIVE, "[o Options]", b"o");
    act(&mut session, mouse, NATIVE, "[V Full New]", b"V");
    full(&mut session, "New", "line001 unchanged");
    if mouse {
        let point =
            crate::support::locate_screen(session.completed_cursorless_screen().unwrap(), "Exact:")
                .unwrap();
        session.click(point.0, point.1);
    } else {
        session.send_key(b"/");
    }
    session.paste("line090");
    session.send_key(b"\r");
    session.wait_for_cursorless_condition(
        |frame| {
            let s = frame.contents();
            s.contains("FULL New file")
                && s.contains("1 matching lines")
                && s.contains("Full-file context ready")
                && !s.contains("NOT CURRENT")
        },
        "new query finishes on original full-file source before match action",
        Duration::from_secs(8),
    );
    act(&mut session, mouse, NATIVE, "[n Match]", b"n");
    full(&mut session, "New", "line090 unchanged");
    session.resize_owned_modal(40, 24, NATIVE, &["[Esc Close]", "[o Options]"]);
    full(&mut session, "New", "line090 unchanged");
    session.resize_owned_modal(600, 24, NATIVE, &["[Esc Close]", "[o Options]"]);
    full(&mut session, "New", "line090 unchanged");
    act(&mut session, mouse, NATIVE, "[b Back patch]", b"b");
    nready(&mut session, "Selected 2");
    assert!(
        session.screen().contains("line090"),
        "current query persists but does not redefine original range"
    );
    act(&mut session, mouse, NATIVE, "[Esc Close]", b"\x1b");
    overlay_ready(
        &mut session,
        "Immutable file images",
        &["[v Native diff]", "[R Range]"],
    );
    act(
        &mut session,
        mouse,
        "Immutable file images",
        "[v Native diff]",
        b"v",
    );
    nready(&mut session, "line050 CHANGED");
    assert_eq!(
        commands(&session, &fixture).len(),
        58,
        "retained image entry/projections add zero requests"
    );
    act(&mut session, mouse, NATIVE, "[Esc Close]", b"\x1b");
    act(
        &mut session,
        mouse,
        "Immutable file images",
        "[Esc Close]",
        b"\x1b",
    );
    ready(&mut session, &["[f Files]", "[v Native diff]"]);
    unchanged(&session, &fixture, &sql, &steps);
    assert_eq!(git_commands(&fixture), trace);
    assert_eq!(readonly_git(&fixture), git);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn scoped_pr_native_diff_keyboard_final_same_login_uid_change_no_private_or_native_publication() {
    refusal(true, false);
}
#[test]
fn scoped_pr_native_diff_mouse_final_same_login_uid_change_no_private_or_native_publication() {
    refusal(true, true);
}
#[test]
fn scoped_pr_native_diff_keyboard_closing_head_change_no_private_or_native_publication() {
    refusal(false, false);
}
#[test]
fn scoped_pr_native_diff_mouse_closing_head_change_no_private_or_native_publication() {
    refusal(false, true);
}
fn refusal(uid: bool, mouse: bool) {
    let (fixture, _, steps) = fixture_and_steps(|image| {
        if uid {
            image[16] = get("user", json!({"id":43,"login":"octocat"}));
        } else {
            let index = image
                .iter()
                .rposition(|step| {
                    step.args
                        .get(1)
                        .is_some_and(|value| value.ends_with("/pulls/7"))
                })
                .unwrap();
            let mut value: Value = serde_json::from_slice(&image[index].stdout).unwrap();
            value["head"]["sha"] = json!("f".repeat(40));
            image[index].stdout = value.to_string().into_bytes();
            image.truncate(index + 1);
        }
    });
    let git = readonly_git(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    files_selected(&mut session, &fixture, mouse);
    let sql = sql_state(&fixture);
    let trace = git_commands(&fixture);
    choose(&mut session, mouse, "[v Native diff]", b"v");
    session.wait_for_command_count(&fixture, steps.len());
    session.wait_for_cursorless_condition(
        |frame| {
            let s = visible_words(&frame.contents());
            s.contains("Original repository scope is fenced") && !s.contains(NATIVE)
        },
        "original account/head source refusal before native publication",
        Duration::from_secs(8),
    );
    for private in [
        "old-no-newline",
        "new-no-newline",
        PRIVATE_DRAFT,
        DETAIL_BODY,
        ROW_TITLE,
    ] {
        assert!(!session.screen().contains(private));
    }
    unchanged(&session, &fixture, &sql, &steps);
    assert_eq!(git_commands(&fixture), trace);
    assert_eq!(readonly_git(&fixture), git);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn scoped_pr_native_diff_keyboard_original_live_read_back_reaps_then_fresh_direct_reopen() {
    cancel(false);
}
#[test]
fn scoped_pr_native_diff_mouse_original_live_read_back_reaps_then_fresh_direct_reopen() {
    cancel(true);
}
fn cancel(mouse: bool) {
    let (fixture, h, mut steps) = fixture_and_steps(|image| {
        image[16].delay_ms = 30_000;
    });
    let mut again = initial()[18..].to_vec();
    again.extend(resource_operation(false));
    let mut images = steps[41..].to_vec();
    images[16].delay_ms = 0;
    again.extend(images);
    bind_steps(&mut again, &h);
    steps.extend(again);
    fixture.scripted_gh(&steps);
    let git = readonly_git(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    files_selected(&mut session, &fixture, mouse);
    let sql = sql_state(&fixture);
    let trace = git_commands(&fixture);
    choose(&mut session, mouse, "[v Native diff]", b"v");
    session.wait_for_command_count(&fixture, 58);
    let rows = commands(&session, &fixture);
    let pid = rows[57]["pid"]
        .as_i64()
        .filter(|pid| *pid > 0 && *pid <= i64::from(i32::MAX))
        .unwrap() as i32;
    let original = process_identity(pid)
        .unwrap()
        .expect("actual final principal child still held before close");
    let owner = OwnedPtySession::capture(session.owned_application_pid() as u32).unwrap();
    assert!(owner.owns(&original));
    assert!(!original.zombie);
    choose(&mut session, mouse, "[b Back]", b"b");
    session.wait_for_cursorless_condition(
        |frame| {
            let s = frame.contents();
            s.contains(ROW_TITLE) && !s.contains(NATIVE)
        },
        "original Back retires pending native opening",
        Duration::from_secs(8),
    );
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let Some(current) = process_identity(pid).unwrap() else {
            break;
        };
        assert_eq!(
            (current.uid, current.birth, current.session),
            (original.uid, original.birth, original.session)
        );
        assert!(owner.owns(&current));
        if current.zombie && current.parent == std::process::id() as i32 {
            let mut status = 0;
            assert_eq!(
                unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) },
                pid
            );
            assert!(process_identity(pid).unwrap().is_none());
            break;
        }
        session.poll_output(Duration::from_millis(1));
        assert!(
            Instant::now() < deadline,
            "exact owned final-principal child must be gone, not merely kill0"
        );
    }
    choose(&mut session, mouse, ROW_TITLE, b"\r");
    session.wait_for_command_count(&fixture, 71);
    ready(&mut session, &[DETAIL_TITLE, "[f Files]"]);
    choose(&mut session, mouse, "[f Files]", b"f");
    session.wait_for_command_count(&fixture, 81);
    session.wait_for_cursorless_condition(
        |frame| {
            let screen = frame.contents();
            screen.contains("Files: all")
                && screen.contains("Old side: current base-branch tip")
                && crate::support::controls::text_has_control(&screen, "[N Next file]")
        },
        "second Files producer publishes its actual completed original page",
        Duration::from_secs(8),
    );
    let before = session.completed_cursorless_end();
    act(
        &mut session,
        mouse,
        "Inbox • focused view",
        "[N Next file]",
        b"N",
    );
    ack(
        &mut session,
        before,
        |screen| screen.contains("> File src/界.rs"),
        "second original Files selection completes before native reopen",
    );
    act(
        &mut session,
        mouse,
        "Inbox • focused view",
        "[v Native diff]",
        b"v",
    );
    session.wait_for_command_count(&fixture, 98);
    nready(&mut session, "new-no-newline");
    assert_eq!(commands(&session, &fixture).len(), 98);
    unchanged(&session, &fixture, &sql, &steps);
    assert_eq!(git_commands(&fixture), trace);
    assert_eq!(readonly_git(&fixture), git);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    session.quit_and_assert_restored_terminal(mouse);
}
