//! NEW UNREGISTERED child of graph_paging_v1.rs. Real binary, real10000 objects,
//! all50actual200-record pages, both input paths. Declarations are UNRUN.
use super::*;
#[path = "graph_refresh_v3.rs"]
mod refresh_v3;
#[path = "../support/graph_compact_selection_v1.rs"]
mod selection_v1;

fn ordinary10000() -> NativeGraph {
    let mut native = fixture("ordinary");
    let blob = native
        .fixture
        .git(&["rev-parse", "HEAD:readme"])
        .trim()
        .to_owned();
    let mut input = Vec::new();
    for number in 1..=10_000 {
        let subject = format!("Ordinary graph version {:05}\n", number - 1);
        input.extend_from_slice(format!("commit refs/heads/main\nmark :{number}\nauthor Graph Writer <writer@example.invalid> {} +0000\ncommitter Graph Recorder <record@example.invalid> {} +0000\ndata {}\n{subject}",
            1_700_000_000+number,1_700_000_000+number,subject.len()).as_bytes());
        if number > 1 {
            input.extend_from_slice(format!("from :{}\n", number - 1).as_bytes());
        } else {
            input.extend_from_slice(format!("deleteall\nM 100644 {blob} readme\n").as_bytes());
        }
        input.push(b'\n');
    }
    object(
        &native.fixture,
        "git",
        &["fast-import", "--quiet", "--force"],
        &input,
    );
    native.tip = native.fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    assert_eq!(
        native.fixture.git(&["rev-list", "--count", "HEAD"]).trim(),
        "10000"
    );
    native.baseline = raw_state(&native.fixture.repo());
    native.sql = sql(&native.fixture);
    native
}
fn ready_loaded(session: &mut Session, loaded: usize, start: usize) {
    let count = format!("of {loaded} loaded (limit 10000)");
    let range = format!(
        "Visible commits {}–{}",
        start + 1,
        (start + 200).min(loaded)
    );
    session
        .wait_for_cursorless_condition(
            |frame| {
                let screen = frame.contents();
                screen.contains(&count)
                    && screen.contains(&range)
                    && screen.contains("[N More history]")
                    && !screen.contains("[Esc Cancel]")
            },
            &format!("Exact current loaded count/range and complete nonpending frame: loaded={loaded}, start={start}"),
            Duration::from_secs(8),
        );
}
fn load_all(native: &NativeGraph, session: &mut Session, mouse: bool) {
    graph(native, session, mouse, 200);
    ready_loaded(session, 200, 0);
    for page in 2..=50 {
        let before = trace(native).len();
        choose(session, mouse, "[N More history]", b"N");
        ready_loaded(session, page * 200, 0);
        let current = trace(native);
        let children = &current[before..];
        assert_eq!(children.len(), 6, "Each explicit More owns one actual prequeue30s/128-child operation; no old-clock renewal");
        assert_eq!(
            children
                .iter()
                .filter(|child| child["argv"]
                    .as_array()
                    .is_some_and(|args| args.iter().any(|arg| arg == "log")))
                .count(),
            1
        );
        let skip = format!("--skip={}", (page - 1) * 200);
        assert!(children.iter().any(|child| child["argv"]
            .as_array()
            .is_some_and(|args| args.iter().any(|arg| arg.as_str() == Some(skip.as_str())))));
    }
    // This is FULL actual loaded history, not a truncated/censored known page.
    session.wait_for_screen("of 10000 loaded (limit 10000)");
    body_to(
        session,
        mouse,
        "All ancestors in this original saved walk are loaded",
    );
    // Home is a real pure projection request even on this same first range;
    // it restores visible first-member position after inspecting the footer.
    choose(session, mouse, "[Home First loaded]", b"\x1b[H");
    // The same old range/count can be painted before this pure request ends.
    // Require its newly owned completion status, not a transient busy frame.
    session.wait_for_cursorless_condition(
        |frame| {
            frame
                .contents()
                .contains("First loaded graph projection ready • no Git reads")
        },
        "This explicit First projection completed through the owned publisher",
        Duration::from_secs(8),
    );
    ready_loaded(session, 10_000, 0);
}

fn select_loaded_row(session: &mut Session, mouse: bool, label: &str, actual_saved_sha: &str) {
    if mouse {
        select_row(session, true, label);
        return;
    }
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        let frame = session
            .completed_cursorless_screen()
            .expect("actual completed Graph frame");
        let batch = selection_v1::first_page_down_batch(frame, label, actual_saved_sha)
            .unwrap_or_else(|reason| {
                panic!(
                    "Refused graph navigation plan: {reason}\n{}",
                    session.screen()
                )
            });
        let Some(batch) = batch else {
            session.send_key(b"\r");
            return;
        };
        let previous = session.completed_cursorless_end();
        // Preserve every real commit AND parent-connector action, with at most
        //16 genuine Down events per bounded batch. Labels are navigation hints;
        // independent full SHA/read completion below remains object authority.
        session.send_key(&batch.bytes());
        loop {
            session.poll_output(Duration::from_millis(1));
            if session.completed_cursorless_end() > previous
                && session.completed_cursorless_screen().is_some_and(|frame| {
                    selection_v1::selected_subject(frame)
                        .is_ok_and(|subject| subject == batch.expected_subject)
                        && !frame.contents().contains("[Esc Cancel]")
                })
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Actual NEW graph frame did not finish the exact bounded batch toward {label}\n{}",
                session.screen()
            );
        }
        assert!(
            Instant::now() < deadline,
            "Exact graph row never became keyboard-selected: {label}\n{}",
            session.screen()
        );
    }
}
fn visible_page(session: &mut Session, loaded: usize, start: usize) {
    session.wait_for_cursorless_condition(
        |frame| {
            let screen = frame.contents();
            screen.contains(&format!(
                "Visible commits {}–{}",
                start + 1,
                (start + 200).min(loaded)
            )) && screen.contains(&format!("of {loaded} loaded (limit 10000)"))
                && !screen.contains("[Esc Cancel]")
        },
        "Actual newer/older projection has completed",
        Duration::from_secs(8),
    );
}
fn full_back(mouse: bool) {
    let native = ordinary10000();
    let mut session = launch(&native, mouse);
    load_all(&native, &mut session, mouse);
    let full = trace(&native).len();
    choose(&mut session, mouse, "[> Older loaded]", b">");
    visible_page(&mut session, 10_000, 200);
    choose(&mut session, mouse, "[> Older loaded]", b">");
    visible_page(&mut session, 10_000, 400);
    let selected = native
        .fixture
        .git(&["rev-parse", "HEAD~400"])
        .trim()
        .to_owned();
    session.wait_for_screen(&format!("▶ selected {}", &selected[..8]));
    choose(&mut session, mouse, "[Graph →]", b"\x1b[C");
    session.wait_for_screen("Graph horizontal: 8 cells");
    if mouse {
        choose(&mut session, true, "[P Parents]", b"P");
    } else {
        session.focus_visible_action("[P Parents]");
        session.send_key(b"\r");
    }
    session.wait_for_screen(&format!(
        "Parent 1: {}",
        native.fixture.git(&["rev-parse", "HEAD~401"]).trim()
    ));
    unchanged_calls(&native, &mut session, full);
    choose(&mut session, mouse, "[u Graph back]", b"u");
    visible_page(&mut session, 10_000, 400);
    choose(&mut session, mouse, "[End Last loaded]", b"\x1b[F");
    visible_page(&mut session, 10_000, 9800);
    unchanged_calls(&native, &mut session, full);
    choose(&mut session, mouse, "[Home First loaded]", b"\x1b[H");
    visible_page(&mut session, 10_000, 0);
    // Select actual last row on first projection; its actual parent lives on
    // another projection, proving cross-page global membership not row index.
    if mouse {
        body_to(&mut session, true, "Ordinary graph version 09800");
    }
    select_loaded_row(
        &mut session,
        mouse,
        "Ordinary graph version 09800",
        &native.tip,
    );
    let last_first_page = native
        .fixture
        .git(&["rev-parse", "HEAD~199"])
        .trim()
        .to_owned();
    session.wait_for_cursorless_condition(
        |frame| {
            let screen = frame.contents();
            screen.contains(&format!("Saved: {last_first_page}"))
                && screen.contains(&format!("▶ selected {}", &last_first_page[..8]))
                && !screen.contains("[Esc Cancel]")
        },
        "Exact last first-page member has completed selection before opening Parents",
        Duration::from_secs(8),
    );
    choose(&mut session, mouse, "[P Parents]", b"P");
    session.wait_for_screen("[Compare parent 1]");
    select_row(&mut session, mouse, "Parent 1:");
    visible_page(&mut session, 10_000, 200);
    session.wait_for_screen("Ordinary graph version 09799");
    unchanged_calls(&native, &mut session, full);
    // Return to exact earlier member/range, then leave via an actual Git tree
    // operation. Back must restore full shared pages, not rebuildpage1/HEAD.
    choose(&mut session, mouse, "[> Older loaded]", b">");
    visible_page(&mut session, 10_000, 400);
    session.wait_for_screen(&format!("▶ selected {}", &selected[..8]));
    choose(&mut session, mouse, "[t Tree]", b"t");
    session.wait_for_screen("Saved file tree");
    let before_back = trace(&native).len();
    choose(&mut session, mouse, "[Back]", b"\x1b[1;3D");
    visible_page(&mut session, 10_000, 400);
    session.wait_for_screen(&format!("▶ selected {}", &selected[..8]));
    session.wait_for_screen("Graph horizontal: 8 cells");
    let restore = &trace(&native)[before_back..];
    assert!(!restore.is_empty());
    assert!(
        restore.len() <= 16,
        "Fresh original status/member/closure only, not50pages/128children"
    );
    assert!(!restore
        .iter()
        .any(|row| row["argv"].as_array().is_some_and(|argv| argv
            .iter()
            .any(|arg| arg == "log" || arg == "rev-list" || arg == "show-ref"))));
    session.resize(80, 24);
    session.wait_for_screen("[P Parents]");
    choose(&mut session, mouse, "[P Parents]", b"P");
    session.wait_for_screen("Ordered original parents");
    choose(&mut session, mouse, "[u Graph back]", b"u");
    session.wait_for_screen("[> Older loaded]");
    session.resize(120, 40);
    visible_page(&mut session, 10_000, 400);
    effects(&native, &session, None);
    session.quit_and_assert_restored_terminal(mouse);
}
fn discard(mouse: bool) {
    let native = ordinary10000();
    let mut session = launch(&native, mouse);
    graph(&native, &mut session, mouse, 200);
    ready_loaded(&mut session, 200, 0);
    choose(&mut session, mouse, "[N More history]", b"N");
    ready_loaded(&mut session, 400, 0);
    let prior = trace(&native).len();
    let old_sql = sql(&native.fixture);
    choose(&mut session, mouse, "[x Discard graph Back]", b"x");
    session
        .wait_for_screen("Earlier Graph Back snapshots discarded • current graph remains loaded");
    unchanged_calls(&native, &mut session, prior);
    assert_eq!(sql(&native.fixture), old_sql);
    ready_loaded(&mut session, 400, 0);
    // The previous200-source bookmark must be honestly unavailable, no legacy
    // graph reconstruction/source decode/HEAD fallback despite real objects.
    choose(&mut session, mouse, "[Back]", b"\x1b[1;3D");
    session.wait_for_screen("Original Graph snapshot unavailable");
    unchanged_calls(&native, &mut session, prior);
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_overlay_close("Original Graph snapshot unavailable");
    session.wait_for_main_frame();
    ready_loaded(&mut session, 400, 0);
    effects(&native, &session, None);
    session.quit_and_assert_restored_terminal(mouse);
}
fn held_restore(native: &NativeGraph) -> PathBuf {
    let output = Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let installed = String::from_utf8(output.stdout).unwrap().trim().to_owned();
    let root = native.fixture.root.display();
    let script=format!("#!/bin/sh\ncase \" $* \" in *' --batch-check '*) if [ -f '{root}/arm-restore' ]; then '{installed}' \"$@\" > '{root}/restore-output' || exit $?; printf '%s\\n' \"$$\" > '{root}/restore-pid'; while [ ! -f '{root}/release-restore' ]; do /bin/sleep 0.02; done; /bin/cat '{root}/restore-output'; else exec '{installed}' \"$@\"; fi;; *) exec '{installed}' \"$@\";; esac\n");
    let path = native.fixture.root.join("bin/git");
    fs::write(&path, script).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    native.fixture.root.join("restore-pid")
}
fn restore_cancel(mouse: bool) {
    let native = ordinary10000();
    let pid_file = held_restore(&native);
    let mut session = launch(&native, mouse);
    load_all(&native, &mut session, mouse);
    choose(&mut session, mouse, "[t Tree]", b"t");
    session.wait_for_screen("Saved file tree");
    fs::write(
        native.fixture.root.join("arm-restore"),
        b"hold exact original Back read\n",
    )
    .unwrap();
    choose(&mut session, mouse, "[Back]", b"\x1b[1;3D");
    until(
        &mut session,
        || {
            fs::read_to_string(&pid_file)
                .is_ok_and(|raw| raw.trim().parse::<i32>().is_ok_and(|pid| pid > 0))
        },
        "Actual retained Back child must enter",
    );
    let pid = fs::read_to_string(pid_file)
        .unwrap()
        .trim()
        .parse::<i32>()
        .unwrap();
    assert_eq!(unsafe { libc::kill(pid, 0) }, 0);
    session.wait_for_screen("[Esc Cancel]");
    choose(&mut session, mouse, "[Esc Cancel]", b"\x1b");
    until(
        &mut session,
        || unsafe { libc::kill(pid, 0) } != 0,
        "Original Back process group must terminate",
    );
    session.wait_for_screen("Cancelled");
    session.wait_for_main_frame();
    session.wait_for_screen("Saved file tree");
    assert!(!session.screen().contains("of 10000 loaded"));
    effects(&native, &session, None);
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn commit_graph_v2_native_keyboard_full10000_all50pages_original_back() {
    full_back(false)
}
#[test]
fn commit_graph_v2_native_mouse_full10000_all50pages_original_back() {
    full_back(true)
}
#[test]
fn commit_graph_v2_native_keyboard_discard_only_graph_pages_no_fallback() {
    discard(false)
}
#[test]
fn commit_graph_v2_native_mouse_discard_only_graph_pages_no_fallback() {
    discard(true)
}
#[test]
fn commit_graph_v2_native_keyboard_real_pending_back_cancel_preserves_tree() {
    restore_cancel(false)
}
#[test]
fn commit_graph_v2_native_mouse_real_pending_back_cancel_preserves_tree() {
    restore_cancel(true)
}
