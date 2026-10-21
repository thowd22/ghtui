//! Child of graph_paging_v1.rs: the real binary over a real 10,000-commit walk, all fifty
//! 200-commit pages loaded with `L`, History's `showing a–b · < > page` windows, the drill-down's
//! parent link across windows and Back, by keyboard and mouse.
use super::*;
#[path = "graph_refresh_v3.rs"]
mod refresh_v3;

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
    // History walks every branch and tag (TASK-99.1): the setup tag still names the
    // replaced root, which would make this an unrelated 10,001st commit.
    native.fixture.git(&["tag", "-d", "actual-graph-name"]);
    assert_eq!(
        native.fixture.git(&["rev-list", "--count", "HEAD"]).trim(),
        "10000"
    );
    native.baseline = raw_state(&native.fixture.repo());
    native.sql = sql(&native.fixture);
    native
}
fn ready_loaded(session: &mut Session, loaded: usize, start: usize) {
    // `scroll for more` leaves the status once the whole walk (10000 here) is loaded.
    wait_status(session, loaded, start, loaded < 10_000);
}
fn load_all(native: &NativeGraph, session: &mut Session, mouse: bool) {
    graph(native, session, mouse, 200);
    ready_loaded(session, 200, 0);
    for page in 2..=50 {
        let before = trace(native).len();
        choose_bar_or_palette(session, mouse, "[L Load more]", b"L");
        ready_loaded(session, page * 200, 0);
        let children = &graph_children(native, before);
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
    // This is FULL actual loaded history, not a truncated/censored known page: the last
    // window ends with the completed walk's boundary row.
    ready_loaded(session, 10_000, 0);
    choose_bar_or_palette(session, mouse, "[End Last loaded]", b"\x1b[F");
    ready_loaded(session, 10_000, 9800);
    body_to(session, mouse, "End of history • every commit is loaded");
    // Home is a real pure projection request even on this same first range;
    // it restores visible first-member position after inspecting the footer.
    choose_bar_or_palette(session, mouse, "[Home First loaded]", b"\x1b[H");
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

/// The ordinal of the ordinary commit under the `▌` bar; the tip's message is cut by its ref
/// chips, so it counts as 9999.
fn selected_ordinal(frame: &vt100::Screen) -> Option<usize> {
    let row = selected_history_row(frame)?;
    match row.message.strip_prefix("Ordinary graph version ") {
        Some(digits) if digits.len() == 5 => digits.parse().ok(),
        _ => row.message.contains('…').then_some(9_999),
    }
}
/// Moves the `▌` bar down the first window to the ordinary commit `label` (`sha`): Down in
/// batches of at most 8, each waiting for its frame, or a click on the row's SHA after
/// scrolling it into view.
fn select_loaded_row(session: &mut Session, mouse: bool, label: &str, sha: &str) {
    if mouse {
        body_to(session, true, label);
        session.click_history_row(sha);
        session.wait_for_history_selected(sha);
        return;
    }
    let target: usize = label
        .trim_start_matches("Ordinary graph version ")
        .parse()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        session.poll_output(Duration::from_millis(1));
        let frame = session
            .completed_cursorless_screen()
            .expect("a completed History frame");
        let current = selected_ordinal(frame)
            .unwrap_or_else(|| panic!("no ordinary commit under the bar\n{}", session.screen()));
        assert!(current >= target, "Down would move away from {label}");
        if current == target {
            break;
        }
        let step = (current - target).min(8);
        let expected = current - step;
        session.send_key(&b"\x1b[B".repeat(step));
        session.wait_for_cursorless_condition(
            |frame| {
                selected_ordinal(frame) == Some(expected)
                    && !frame.contents().contains("Esc Cancel")
            },
            &format!("the bar on ordinary commit {expected:05}"),
            deadline.saturating_duration_since(Instant::now()),
        );
    }
    session.wait_for_history_selected(sha);
}
fn visible_page(session: &mut Session, loaded: usize, start: usize) {
    wait_status(session, loaded, start, loaded < 10_000);
}
fn full_back(mouse: bool) {
    let native = ordinary10000();
    let mut session = launch(&native, mouse);
    load_all(&native, &mut session, mouse);
    let full = trace(&native).len();
    choose_bar_or_palette(&mut session, mouse, "[> Older loaded]", b">");
    visible_page(&mut session, 10_000, 200);
    choose_bar_or_palette(&mut session, mouse, "[> Older loaded]", b">");
    visible_page(&mut session, 10_000, 400);
    let selected = native
        .fixture
        .git(&["rev-parse", "HEAD~400"])
        .trim()
        .to_owned();
    session.wait_for_history_selected(&selected);
    choose_bar_or_palette(&mut session, mouse, "[→ Pan right]", b"\x1b[C");
    session.wait_for_screen("History pan: 8 cells");
    open_parents(&mut session, mouse, &selected);
    session.wait_for_screen(&format!(
        "Parent    {}",
        &native.fixture.git(&["rev-parse", "HEAD~401"]).trim()[..7]
    ));
    unchanged_calls(&native, &mut session, full);
    graph_back(&mut session, mouse);
    visible_page(&mut session, 10_000, 400);
    session.wait_for_history_selected(&selected);
    choose_bar_or_palette(&mut session, mouse, "[End Last loaded]", b"\x1b[F");
    visible_page(&mut session, 10_000, 9800);
    unchanged_calls(&native, &mut session, full);
    choose_bar_or_palette(&mut session, mouse, "[Home First loaded]", b"\x1b[H");
    visible_page(&mut session, 10_000, 0);
    // Select the last row of the first window; its parent lives in the next window, proving
    // global membership rather than a row index.
    let last_first_page = native
        .fixture
        .git(&["rev-parse", "HEAD~199"])
        .trim()
        .to_owned();
    select_loaded_row(
        &mut session,
        mouse,
        "Ordinary graph version 09800",
        &last_first_page,
    );
    open_parents(&mut session, mouse, &last_first_page);
    let parent = native
        .fixture
        .git(&["rev-parse", "HEAD~200"])
        .trim()
        .to_owned();
    follow_parent(&mut session, mouse, &parent);
    session.wait_for_screen(&format!("Commit {}", &parent[..7]));
    graph_back(&mut session, mouse);
    visible_page(&mut session, 10_000, 200);
    session.wait_for_history_selected(&parent);
    session.wait_for_screen("Ordinary graph version 09799");
    unchanged_calls(&native, &mut session, full);
    // Return to exact earlier member/range, then leave via an actual Git tree
    // operation. Back must restore full shared pages, not rebuildpage1/HEAD.
    choose_bar_or_palette(&mut session, mouse, "[> Older loaded]", b">");
    visible_page(&mut session, 10_000, 400);
    session.wait_for_history_selected(&selected);
    session.wait_for_settled_footer();
    choose(&mut session, mouse, "[t Tree]", b"t");
    session.wait_for_screen("Saved file tree");
    session.wait_for_screen(&format!("Viewed: {selected}"));
    let before_back = trace(&native).len();
    session.wait_for_settled_footer();
    choose(&mut session, mouse, "Alt-← Back", b"\x1b[1;3D");
    visible_page(&mut session, 10_000, 400);
    session.wait_for_history_selected(&selected);
    session.wait_for_screen("History pan: 8 cells");
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
    session.wait_for_main_frame();
    session.wait_for_history_ready();
    open_parents(&mut session, mouse, &selected);
    graph_back(&mut session, mouse);
    session.resize(120, 40);
    visible_page(&mut session, 10_000, 400);
    session.wait_for_history_selected(&selected);
    effects(&native, &session, None);
    session.quit_and_assert_restored_terminal(mouse);
}
fn discard(mouse: bool) {
    let native = ordinary10000();
    let mut session = launch(&native, mouse);
    graph(&native, &mut session, mouse, 200);
    ready_loaded(&mut session, 200, 0);
    choose_bar_or_palette(&mut session, mouse, "[L Load more]", b"L");
    ready_loaded(&mut session, 400, 0);
    let prior = trace(&native).len();
    let old_sql = sql(&native.fixture);
    choose_bar_or_palette(&mut session, mouse, "[x Discard graph Back]", b"x");
    session
        .wait_for_screen("Earlier Graph Back snapshots discarded • current graph remains loaded");
    unchanged_calls(&native, &mut session, prior);
    assert_eq!(sql(&native.fixture), old_sql);
    ready_loaded(&mut session, 400, 0);
    // The previous200-source bookmark must be honestly unavailable, no legacy
    // graph reconstruction/source decode/HEAD fallback despite real objects.
    session.wait_for_settled_footer();
    choose(&mut session, mouse, "Alt-← Back", b"\x1b[1;3D");
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
    session.wait_for_settled_footer();
    choose(&mut session, mouse, "[t Tree]", b"t");
    session.wait_for_screen("Saved file tree");
    fs::write(
        native.fixture.root.join("arm-restore"),
        b"hold exact original Back read\n",
    )
    .unwrap();
    choose(&mut session, mouse, "Alt-← Back", b"\x1b[1;3D");
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
    session.wait_for_control("[Esc Cancel]");
    choose(&mut session, mouse, "[Esc Cancel]", b"\x1b");
    until(
        &mut session,
        || unsafe { libc::kill(pid, 0) } != 0,
        "Original Back process group must terminate",
    );
    session.wait_for_screen("Cancelled");
    session.wait_for_main_frame();
    session.wait_for_screen("Saved file tree");
    assert!(!session.screen().contains("10000 commits loaded"));
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
