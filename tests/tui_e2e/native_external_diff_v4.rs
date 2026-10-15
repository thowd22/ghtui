//! Actual ghtui binary, real Git, real distinct child PTY, strict tool protocol.
//! Source-only declarations until Root runs the checked gate; not tool-compat
//! proof. The parent owns the established genuine comparison fixtures.
use super::*;
use crate::native_external_tools_v4::{ExternalToolsV4, PagerV4, RendererV4};
use crate::support::owned_pty_processes_v1::{process_identity, ProcessIdentity};
use std::ffi::OsStr;
const CHOOSER: &str = "Optional external diff • read-only";
const PANE: &str = "Optional external pager • host-owned read-only PTY";

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn configured() -> Native {
    let mut native = small_fixture();
    let old = git_bytes(
        &native.fixture,
        &["show", &format!("{}:code.rs", native.c)],
        None,
    );
    let new = git_bytes(
        &native.fixture,
        &["show", &format!("{}:code.rs", native.b)],
        None,
    );
    native.c = commit(
        &native.fixture,
        &tree(
            &native.fixture,
            &[
                (b"code.rs", &old, "100644"),
                (b"z-other.rs", b"outside old\n", "100644"),
            ],
        ),
        None,
        "External-common",
    );
    native.a = commit(
        &native.fixture,
        &tree(
            &native.fixture,
            &[
                (b"code.rs", b"REQUESTED A IS NOT OLD\n", "100644"),
                (b"z-other.rs", b"outside old\n", "100644"),
            ],
        ),
        Some(&native.c),
        "External-requested",
    );
    native.b = commit(
        &native.fixture,
        &tree(
            &native.fixture,
            &[
                (b"code.rs", &new, "100644"),
                (b"z-other.rs", b"outside new\n", "100644"),
            ],
        ),
        Some(&native.c),
        "External-target",
    );
    native
        .fixture
        .git(&["update-ref", "refs/heads/requested-A", &native.a]);
    native
        .fixture
        .git(&["update-ref", "refs/heads/target-B", &native.b]);
    native.before = state(&native.fixture);
    native
}
fn start(native: &Native, tools: &ExternalToolsV4, mouse: bool) -> Session {
    let trace = native.fixture.root.join("diff-sides-trace.jsonl");
    let mut env = vec![
        (OsStr::new("PATH"), tools.path()),
        (OsStr::new("GIT_TRACE2_EVENT"), trace.as_os_str()),
    ];
    if mouse {
        env.push((OsStr::new("NO_COLOR"), OsStr::new("1")));
    }
    Session::launch_binary_os_env(
        Path::new(env!("CARGO_BIN_EXE_ghtui")),
        &native.fixture,
        if mouse { 80 } else { 120 },
        if mouse { 24 } else { 40 },
        &env,
    )
}
fn action(session: &mut Session, mouse: bool, title: &str, label: &str, key: &[u8]) {
    let point = session.wait_for_owned_completed_action(title, label, &[]);
    if mouse {
        session.click(point.0, point.1);
    } else {
        session.send_key(key);
    }
}
fn native_ready(session: &mut Session, title: &str, text: &str) {
    session.wait_for_cursorless_condition(
        |frame| {
            let s = frame.contents();
            s.contains(title)
                && s.contains("diff ready")
                && s.contains(text)
                && !s.contains("NOT CURRENT")
                && !s.contains("Preparing native")
        },
        "actual current native view after external phase",
        Duration::from_secs(8),
    );
}
fn chooser(session: &mut Session, mouse: bool, title: &str, structural: bool) {
    action(session, mouse, title, "[E External]", b"E");
    session.wait_for_cursorless_condition(
        |frame| {
            crate::support::overlay_body(frame, CHOOSER).is_some_and(|body| {
                body.contains("WHOLE ORIGINAL CAPTURED COMPARISON")
                    && body.contains("Lifecycle ready")
            })
        },
        "actual original scope disclosure and mechanically clean launch",
        Duration::from_secs(8),
    );
    action(
        session,
        mouse,
        CHOOSER,
        if structural {
            "[t difftastic]"
        } else {
            "[d delta]"
        },
        if structural { b"t" } else { b"d" },
    );
    session.wait_for_cursorless_condition(
        |frame| {
            crate::support::overlay_body(frame, CHOOSER).is_some_and(|body| {
                body.contains(if structural {
                    "Selected: difftastic"
                } else {
                    "Selected: delta"
                })
            })
        },
        "completed explicit optional tool choice",
        Duration::from_secs(8),
    );
    action(session, mouse, CHOOSER, "[Enter Launch]", b"\r");
}
fn external(session: &mut Session, text: &str) {
    session.wait_for_cursorless_condition(
        |frame| {
            crate::support::overlay_body(frame, PANE).is_some_and(|body| {
                body.contains(text) && body.contains("[Esc Return]") && body.contains("[^C Cancel]")
            })
        },
        "actual bounded host pane with native Return/Cancel and genuine child cells",
        Duration::from_secs(8),
    );
}
fn gone(session: &mut Session, original: &ProcessIdentity, directory: &Path) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if let Some(current) = process_identity(original.pid).unwrap() {
            assert_eq!(
                (current.pid, current.uid, current.session, current.birth),
                (original.pid, original.uid, original.session, original.birth),
                "do not infer another reused PID is this child"
            );
            if current.zombie && current.parent == std::process::id() as i32 {
                let mut status = 0;
                assert_eq!(
                    unsafe { libc::waitpid(current.pid, &mut status, libc::WNOHANG) },
                    current.pid
                );
                assert!(libc::WIFEXITED(status) || libc::WIFSIGNALED(status));
            }
        }
        if process_identity(original.pid).unwrap().is_none() && !directory.exists() {
            return;
        }
        session.poll_output(Duration::from_millis(1));
        assert!(
            Instant::now() < deadline,
            "exact child identity and private capture directory must both be absent after cleanup"
        );
    }
}
fn child(session: &Session, row: &Value) -> ProcessIdentity {
    assert_eq!(row["stdin"], true);
    assert_eq!(row["stdout"], true);
    assert_eq!(row["stderr"], true);
    assert!(!row["initialTermios"].as_str().unwrap().trim().is_empty());
    assert!(!row["rawTermios"].as_str().unwrap().trim().is_empty());
    let flags = row["rawDescription"]
        .as_str()
        .unwrap()
        .split_whitespace()
        .collect::<Vec<_>>();
    assert!(flags.iter().any(|f| f.trim_end_matches(';') == "-icanon"));
    assert!(flags.iter().any(|f| f.trim_end_matches(';') == "-echo"));
    let pid = i32::try_from(row["pid"].as_i64().unwrap()).unwrap();
    let identity = process_identity(pid).unwrap().unwrap();
    assert_eq!(identity.parent, session.owned_application_pid());
    assert_eq!(identity.session, pid);
    assert_ne!(identity.session, unsafe {
        libc::getsid(session.owned_application_pid())
    });
    identity
}
fn no_host_handoff(session: &Session) {
    assert!(
        !session
            .transcript
            .windows(b"\x1b[?1049l".len())
            .any(|w| w == b"\x1b[?1049l"),
        "parent must genuinely retain its alternate terminal, not claim an unperformed suspend"
    );
    assert!(!session
        .transcript
        .windows(b"PRIVATE-EXTERNAL-TITLE".len())
        .any(|w| w == b"PRIVATE-EXTERNAL-TITLE"));
    assert!(!session
        .transcript
        .windows(b"PRIVATE-CLIPBOARD".len())
        .any(|w| w == b"PRIVATE-CLIPBOARD"));
}
fn actual_selected(session: &Session) -> String {
    session
        .completed_cursorless_screen()
        .unwrap()
        .contents()
        .lines()
        .find_map(|row| {
            row.strip_prefix("│>")
                .map(|row| row.trim_end_matches('│').trim().to_owned())
        })
        .expect("actual selected original native row, not a guessed caret or display coordinate")
}
fn original_patch(native: &Native) -> Vec<u8> {
    git_bytes(
        &native.fixture,
        &[
            "--no-replace-objects",
            "-c",
            "diff.autoRefreshIndex=false",
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--find-renames",
            &native.c,
            &native.b,
            "--",
        ],
        None,
    )
}
#[test]
fn native_external_diff_keyboard_saved_whole_delta_selected_c_b_structural_resize_return_and_cleanup(
) {
    saved(false);
}
#[test]
fn native_external_diff_mouse_saved_whole_delta_selected_c_b_structural_resize_return_and_cleanup()
{
    saved(true);
}
fn saved(mouse: bool) {
    let native = configured();
    let tools = ExternalToolsV4::install(&native.fixture, RendererV4::Ready, PagerV4::Hostile);
    let mut session = start(&native, &tools, mouse);
    open_comparison(&native, &mut session, mouse, true);
    choose(&mut session, mouse, "[v Native diff]", b"v");
    native_ready(&mut session, NATIVE_TITLE, "new_name");
    focus(&mut session, mouse, "let new_name = 2;");
    choose(&mut session, mouse, "[h Hunk]", b"h");
    native_ready(&mut session, NATIVE_TITLE, "Selected 2");
    if mouse {
        session.click_visible_action("Exact:");
    } else {
        session.send_key(b"/");
    }
    session.paste("new_name");
    session.send_key(b"\r");
    native_ready(&mut session, NATIVE_TITLE, "1 matches");
    let selected = actual_selected(&session);
    assert!(selected.contains("let new_name = 2;"));
    let host = session.actual_terminal_witness_v4().unwrap();
    assert!(host.is_raw());
    let before = starts(&native).len();
    chooser(&mut session, mouse, NATIVE_TITLE, false);
    external(&mut session, "EXTERNAL PAGER READY");
    let record = tools.wait_phase(&mut session, "delta", "body", 1).remove(0);
    assert_eq!(record["input"], hex(&original_patch(&native)));
    assert!(original_patch(&native)
        .windows(b"z-other.rs".len())
        .any(|w| w == b"z-other.rs"));
    let entered = tools.wait_phase(&mut session, "less", "pty", 1).remove(0);
    let identity = child(&session, &entered);
    let directory = PathBuf::from(entered["cwd"].as_str().unwrap());
    assert_eq!(record["args"][3], format!("--width={}", entered["columns"]));
    assert_eq!(session.actual_terminal_witness_v4().unwrap(), host);
    assert_eq!(
        starts(&native).len(),
        before,
        "delta borrows the already admitted original patch, no second Git backend"
    );
    let after = session.completed_cursorless_end();
    action(&mut session, mouse, PANE, "[j Down]", b"j");
    session.wait_for_new_cursorless_condition(
        after,
        |frame| frame.contents().contains("EXTERNAL KEY \"j\""),
        "actual child received shared keyboard/mouse Down",
        Duration::from_secs(8),
    );
    let records = tools.phase("less", "key").len();
    action(&mut session, mouse, PANE, "[/ Search]", b"/");
    session.paste("outside new");
    let until = Instant::now() + Duration::from_secs(8);
    while !session
        .visible_overlay_body(PANE)
        .is_some_and(|body| body.contains("Pager search: outside new"))
    {
        session.poll_output(Duration::from_millis(1));
        assert!(
            Instant::now() < until,
            "actual owned query editor with shown caret, not an old cursorless frame"
        );
    }
    if mouse {
        let point = session
            .locate("[Find]")
            .expect("unique Find inside actual owned query frame");
        session.click(point.0, point.1);
    } else {
        session.send_key(b"\r");
    }
    let expected = hex(b"/outside new\n");
    loop {
        let actual = tools
            .phase("less", "key")
            .into_iter()
            .skip(records)
            .map(|row| row["hex"].as_str().unwrap().to_owned())
            .collect::<String>();
        assert!(
            actual.len() <= expected.len(),
            "no hidden extra child input"
        );
        if actual.len() == expected.len() {
            assert_eq!(actual, expected);
            break;
        }
        session.poll_output(Duration::from_millis(1));
        assert!(
            Instant::now() < until,
            "actual fixed bounded pager search command missing"
        );
    }
    let width = if mouse { 120 } else { 80 };
    session.resize_owned_header(width, 24, PANE, &["[Esc Return]", "[^C Cancel]"]);
    let resized = tools
        .wait_phase(&mut session, "less", "resize", 1)
        .remove(0);
    external(
        &mut session,
        &format!("EXTERNAL SIZE {}x{}", resized["columns"], resized["rows"]),
    );
    assert_eq!(session.actual_terminal_witness_v4().unwrap(), host);
    no_host_handoff(&session);
    action(&mut session, mouse, PANE, "[Esc Return]", b"\x1b");
    native_ready(&mut session, NATIVE_TITLE, "Selected 2");
    native_ready(&mut session, NATIVE_TITLE, "new_name");
    assert_eq!(
        actual_selected(&session),
        selected,
        "Return must retain genuine original selection, not compensate with new navigation"
    );
    gone(&mut session, &identity, &directory);
    assert_eq!(session.actual_terminal_witness_v4().unwrap(), host);
    effects(&native, &session);
    chooser(&mut session, mouse, NATIVE_TITLE, true);
    external(&mut session, "EXTERNAL PAGER READY");
    let record = tools.wait_phase(&mut session, "difft", "body", 1).remove(0);
    let old = git_bytes(
        &native.fixture,
        &["show", &format!("{}:code.rs", native.c)],
        None,
    );
    let new = git_bytes(
        &native.fixture,
        &["show", &format!("{}:code.rs", native.b)],
        None,
    );
    assert_ne!(
        old,
        git_bytes(
            &native.fixture,
            &["show", &format!("{}:code.rs", native.a)],
            None
        )
    );
    assert_eq!(record["old"], hex(&old));
    assert_eq!(record["new"], hex(&new));
    let entered = tools.wait_phase(&mut session, "less", "pty", 2).remove(1);
    let identity = child(&session, &entered);
    let directory = PathBuf::from(entered["cwd"].as_str().unwrap());
    assert!(
        !Path::new(record["cwd"].as_str().unwrap()).exists(),
        "original full-side temporary inputs are actually deleted before pager publication"
    );
    action(&mut session, mouse, PANE, "[^C Cancel]", b"\x03");
    native_ready(&mut session, NATIVE_TITLE, "Selected 2");
    gone(&mut session, &identity, &directory);
    assert_eq!(session.actual_terminal_witness_v4().unwrap(), host);
    no_host_handoff(&session);
    effects(&native, &session);
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_screen("Native saved diff closed");
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn native_external_diff_keyboard_missing_exit_overflow_unpatched_and_pager_failures_keep_native() {
    failures(false);
}
#[test]
fn native_external_diff_mouse_missing_exit_overflow_unpatched_and_pager_failures_keep_native() {
    failures(true);
}
fn failures(mouse: bool) {
    for (renderer, pager, message, bodies, pagers) in [
        (RendererV4::Missing, PagerV4::Ready, "not installed", 0, 0),
        (RendererV4::ExitTwo, PagerV4::Ready, "exited2", 1, 0),
        (RendererV4::Overflow, PagerV4::Ready, "512KiB", 1, 0),
        (RendererV4::Ready, PagerV4::Unpatched, "less704+", 0, 0),
        (RendererV4::Ready, PagerV4::ExitTwo, "exited2", 1, 1),
        (
            RendererV4::Ready,
            PagerV4::OverboundEdit,
            "actual pane geometry",
            1,
            1,
        ),
    ] {
        let native = configured();
        let tools = ExternalToolsV4::install(&native.fixture, renderer, pager);
        let mut session = start(&native, &tools, mouse);
        open_comparison(&native, &mut session, mouse, true);
        choose(&mut session, mouse, "[v Native diff]", b"v");
        native_ready(&mut session, NATIVE_TITLE, "new_name");
        focus(&mut session, mouse, "let new_name = 2;");
        choose(&mut session, mouse, "[h Hunk]", b"h");
        native_ready(&mut session, NATIVE_TITLE, "Selected 2");
        let host = session.actual_terminal_witness_v4().unwrap();
        let children = starts(&native).len();
        chooser(&mut session, mouse, NATIVE_TITLE, false);
        session.wait_for_cursorless_condition(
            |frame| {
                crate::support::overlay_body(frame, "Action unavailable")
                    .is_some_and(|body| body.contains(message) && body.contains("[Esc Close]"))
            },
            "owned readable optional failure, never hidden in a clipped native status",
            Duration::from_secs(8),
        );
        action(
            &mut session,
            mouse,
            "Action unavailable",
            "[Esc Close]",
            b"\x1b",
        );
        session.wait_for_overlay_close("Action unavailable");
        native_ready(&mut session, NATIVE_TITLE, "Selected 2");
        assert_eq!(tools.phase("delta", "body").len(), bodies);
        assert_eq!(tools.phase("less", "pty").len(), pagers);
        assert_eq!(
            starts(&native).len(),
            children,
            "all optional delta failures must add zero Git operations"
        );
        for row in tools.phase("delta", "body") {
            assert_eq!(row["input"], hex(&original_patch(&native)));
            assert!(!Path::new(row["cwd"].as_str().unwrap()).exists());
        }
        for row in tools.phase("less", "pty") {
            assert!(!Path::new(row["cwd"].as_str().unwrap()).exists());
            let pid = i32::try_from(row["pid"].as_i64().unwrap()).unwrap();
            assert!(
                process_identity(pid).unwrap().is_none(),
                "actual pager identity must be gone before restored fallback"
            );
        }
        assert_eq!(session.actual_terminal_witness_v4().unwrap(), host);
        no_host_handoff(&session);
        assert!(!session.screen().contains("PRIVATE-CHILD-DIAGNOSTIC"));
        assert!(!session.screen().contains("PRIVATE-PAGER-DIAGNOSTIC"));
        effects(&native, &session);
        choose(&mut session, mouse, "[Esc Close]", b"\x1b");
        session.wait_for_screen("Native saved diff closed");
        session.quit_and_assert_restored_terminal(mouse);
    }
}

#[test]
fn native_external_diff_keyboard_actual_renderer_cancel_resize_never_renews_or_replays() {
    held(false);
}
#[test]
fn native_external_diff_mouse_actual_renderer_cancel_resize_never_renews_or_replays() {
    held(true);
}
fn held(mouse: bool) {
    let native = configured();
    let tools = ExternalToolsV4::install(&native.fixture, RendererV4::Hold, PagerV4::Ready);
    let mut session = start(&native, &tools, mouse);
    open_comparison(&native, &mut session, mouse, true);
    choose(&mut session, mouse, "[v Native diff]", b"v");
    native_ready(&mut session, NATIVE_TITLE, "new_name");
    let host = session.actual_terminal_witness_v4().unwrap();
    chooser(&mut session, mouse, NATIVE_TITLE, false);
    let row = tools.wait_phase(&mut session, "delta", "body", 1).remove(0);
    let pid = i32::try_from(row["pid"].as_i64().unwrap()).unwrap();
    let original = process_identity(pid).unwrap().unwrap();
    assert_eq!(
        original.session,
        unsafe { libc::getsid(session.owned_application_pid()) },
        "renderer stays on the original runner, not a fresh viewer lease"
    );
    let directory = PathBuf::from(row["cwd"].as_str().unwrap());
    session.resize_owned_header(40, 24, CHOOSER, &["[Enter Launch]", "[Esc Back]"]);
    session.wait_for_cursorless_condition(
        |frame| {
            crate::support::overlay_body(frame, CHOOSER).is_some_and(|body| {
                let visible = body.split_whitespace().collect::<Vec<_>>().join(" ");
                visible.contains("Resize cancelled the original preparation.")
                    && visible.contains("Launch is not retried.")
            })
        },
        "resize cancels real pending original renderer without relaunch",
        Duration::from_secs(8),
    );
    gone(&mut session, &original, &directory);
    tools.release();
    assert_eq!(tools.phase("delta", "body").len(), 1);
    assert_eq!(tools.phase("less", "pty").len(), 0);
    action(&mut session, mouse, CHOOSER, "[Esc Back]", b"\x1b");
    native_ready(&mut session, NATIVE_TITLE, "new_name");
    assert_eq!(session.actual_terminal_witness_v4().unwrap(), host);
    effects(&native, &session);
    no_host_handoff(&session);
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_screen("Native saved diff closed");
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn native_external_diff_keyboard_actual_staged_working_and_all_edits_full_side_truth_matrix() {
    mutable(false);
}
#[test]
fn native_external_diff_mouse_actual_staged_working_and_all_edits_full_side_truth_matrix() {
    mutable(true);
}
fn mutable(mouse: bool) {
    const LOCAL: &str = "Native local diff • observed snapshot";
    let native = configured();
    let tools = ExternalToolsV4::install(&native.fixture, RendererV4::Ready, PagerV4::Ready);
    let mut session = start(&native, &tools, mouse);
    session.wait_for_screen("Recent commits");
    session.resize(120, 40);
    session.wait_for_cursorless_condition(
        |frame| {
            frame.contents().contains("[e All edits]")
                && frame.contents().contains("Recent commits")
        },
        "actual wide repository controls before genuine mode entry",
        Duration::from_secs(8),
    );
    let host = session.actual_terminal_witness_v4().unwrap();
    for (index, label, key, old, new) in [
        (
            0,
            "[i Staged]",
            b"i".as_slice(),
            b"saved checkout tracked\n".as_slice(),
            b"user staged bytes\n".as_slice(),
        ),
        (
            1,
            "[w Working]",
            b"w".as_slice(),
            b"user staged bytes\n".as_slice(),
            b"user dirty bytes\n".as_slice(),
        ),
        (
            2,
            "[e All edits]",
            b"e".as_slice(),
            b"saved checkout tracked\n".as_slice(),
            b"user dirty bytes\n".as_slice(),
        ),
    ] {
        let after = session.completed_cursorless_end();
        choose(&mut session, mouse, label, key);
        session.wait_for_new_cursorless_condition(
            after,
            |frame| {
                let s = frame.contents();
                s.contains("[v Native diff]")
                    && s.contains(std::str::from_utf8(old).unwrap().trim())
                    && s.contains(std::str::from_utf8(new).unwrap().trim())
            },
            "actual independently loaded mutable comparison before Native",
            Duration::from_secs(8),
        );
        choose(&mut session, mouse, "[v Native diff]", b"v");
        native_ready(
            &mut session,
            LOCAL,
            std::str::from_utf8(new).unwrap().trim(),
        );
        assert!(
            session
                .completed_cursorless_screen()
                .unwrap()
                .contents()
                .contains("author unknown"),
            "do not attribute unsaved content to HEAD/config or a hosted user"
        );
        chooser(&mut session, mouse, LOCAL, true);
        external(&mut session, "EXTERNAL PAGER READY");
        let row = tools
            .wait_phase(&mut session, "difft", "body", index + 1)
            .remove(index);
        assert_eq!(row["old"], hex(old));
        assert_eq!(row["new"], hex(new));
        assert!(!Path::new(row["cwd"].as_str().unwrap()).exists());
        let entered = tools
            .wait_phase(&mut session, "less", "pty", index + 1)
            .remove(index);
        let identity = child(&session, &entered);
        let directory = PathBuf::from(entered["cwd"].as_str().unwrap());
        let key_records = tools.phase("less", "key").len();
        session.send_key(b"a");
        action(&mut session, mouse, PANE, "[g First]", b"g");
        external(&mut session, "EXTERNAL KEY \"g\"");
        let sent = tools.phase("less", "key");
        assert_eq!(sent.len(), key_records + 1);
        assert_eq!(
            sent.last().unwrap()["hex"],
            "67",
            "hidden native stage shortcut must not reach source, writer or external program"
        );
        action(&mut session, mouse, PANE, "[Esc Return]", b"\x1b");
        native_ready(
            &mut session,
            LOCAL,
            std::str::from_utf8(new).unwrap().trim(),
        );
        gone(&mut session, &identity, &directory);
        assert_eq!(session.actual_terminal_witness_v4().unwrap(), host);
        effects(&native, &session);
        no_host_handoff(&session);
        choose(&mut session, mouse, "[Esc Close]", b"\x1b");
        session.wait_for_screen("Native local diff closed");
    }
    assert_eq!(tools.phase("difft", "body").len(), 3);
    assert_eq!(tools.phase("less", "pty").len(), 3);
    effects(&native, &session);
    session.quit_and_assert_restored_terminal(mouse);
}
