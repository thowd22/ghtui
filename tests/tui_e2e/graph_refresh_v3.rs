//! NEW UNREGISTERED child of graph_compact_v2. Source-only declarations:
//! genuine ordinary10000 history/all50actual pages, no censored known-page proof.
use super::*;

fn older400(native: &NativeGraph, session: &mut Session, mouse: bool) -> String {
    for start in [200, 400] {
        choose_bar_or_palette(session, mouse, "[> Older loaded]", b">");
        visible_page(session, 10_000, start);
    }
    visible_page(session, 10_000, 400);
    let selected = native
        .fixture
        .git(&["rev-parse", "HEAD~400"])
        .trim()
        .to_owned();
    session.wait_for_screen(&format!("▶ selected {}", &selected[..8]));
    choose_bar_or_palette(session, mouse, "[→ Pan right]", b"\x1b[C");
    session.wait_for_screen("Graph horizontal: 8 cells");
    selected
}
fn exact_recheck(native: &NativeGraph, before: usize, expected: usize) {
    let current = trace(native);
    let reads = &current[before..];
    assert_eq!(
        reads.len(),
        expected,
        "Original bounded status/exact member/shallow closure, not rebuild50pages"
    );
    assert!(!reads
        .iter()
        .any(|child| child["argv"].as_array().is_some_and(|argv| argv
            .iter()
            .any(|arg| arg == "log" || arg == "rev-list" || arg == "show-ref"))));
    assert!(reads.iter().any(|child| child["argv"]
        .as_array()
        .is_some_and(|argv| argv.iter().any(|arg| arg == "cat-file"))));
}
fn refresh_full(mouse: bool) {
    let mut native = ordinary10000();
    let mut session = launch(&native, mouse);
    load_all(&native, &mut session, mouse);
    let selected = older400(&native, &mut session, mouse);
    fs::write(
        native.fixture.repo().join("readme"),
        b"DIRTY GRAPH REFRESH V3 CANARY\n",
    )
    .unwrap();
    native.baseline = raw_state(&native.fixture.repo());
    let before = trace(&native).len();
    choose_bar_or_palette(&mut session, mouse, "[r Refresh]", b"r");
    session.wait_for_screen("Original Graph snapshot refreshed");
    visible_page(&mut session, 10_000, 400);
    session.wait_for_screen(&format!("▶ selected {}", &selected[..8]));
    session.wait_for_screen("Graph horizontal: 8 cells");
    exact_recheck(&native, before, 10);
    session.resize(80, 24);
    session.wait_for_main_frame();
    session.wait_for_control("[P Parents]");
    choose(&mut session, mouse, "[P Parents]", b"P");
    session.wait_for_screen("Ordered original parents");
    session.wait_for_screen(&format!(
        "Parent 1: {}",
        native.fixture.git(&["rev-parse", "HEAD~401"]).trim()
    ));
    choose(&mut session, mouse, "[u Graph back]", b"u");
    visible_page(&mut session, 10_000, 400);
    choose(&mut session, mouse, "[w Working]", b"w");
    session.wait_for_screen("DIRTY GRAPH REFRESH V3 CANARY");
    choose(&mut session, mouse, "[Back]", b"\x1b[1;3D");
    visible_page(&mut session, 10_000, 400);
    session.wait_for_screen(&format!("▶ selected {}", &selected[..8]));
    effects(&native, &session, None);
    session.quit_and_assert_restored_terminal(mouse);
}
fn deliberate_reread(mouse: bool) {
    let native = ordinary10000();
    let mut session = launch(&native, mouse);
    load_all(&native, &mut session, mouse);
    let selected = older400(&native, &mut session, mouse);
    let before = trace(&native).len();
    choose_bar_or_palette(&mut session, mouse, "[R Re-read original]", b"R");
    session.wait_for_screen("Re-read original Graph • first page");
    session.wait_for_screen("[Enter Re-read]");
    session.wait_for_screen("[Esc Back]");
    session.resize_owned_modal(
        80,
        24,
        "Re-read original Graph • first page",
        &["[PgUp]", "[PgDn]"],
    );
    if mouse {
        session.wheel(30, 12, true);
    } else {
        session.send_key(b"\x1b[6~");
    }
    session.wait_for_screen("No checkout, fetch, Git mutation");
    // Unknown/stale background-key input remains captured by this preview.
    session.send_key(b"rg");
    unchanged_calls(&native, &mut session, before);
    choose(&mut session, mouse, "[Esc Back]", b"\x1b");
    session.wait_for_overlay_close("Re-read original Graph • first page");
    session.wait_for_main_frame();
    visible_page(&mut session, 10_000, 400);
    session.wait_for_screen(&format!("▶ selected {}", &selected[..8]));
    unchanged_calls(&native, &mut session, before);
    session.resize(120, 40);
    session.wait_for_main_frame();
    choose_bar_or_palette(&mut session, mouse, "[R Re-read original]", b"R");
    session.wait_for_screen("Re-read original Graph • first page");
    if mouse {
        session.click_visible_action("[Enter Re-read]");
    } else {
        session.send_key(b"\r");
    }
    session.wait_for_overlay_close("Re-read original Graph • first page");
    session.wait_for_screen("Original Graph re-read • first200 loaded");
    ready_loaded(&mut session, 200, 0);
    session.wait_for_screen(&format!("▶ selected {}", &native.tip[..8]));
    session.wait_for_screen("Graph horizontal: 0 cells");
    let actual = trace(&native);
    let reads = &actual[before..];
    assert_eq!(
        reads.len(),
        20,
        "One captured request: current status and oldmember10 + genuinefirstpage9 + epoch1"
    );
    assert_eq!(
        reads
            .iter()
            .filter(|child| child["argv"]
                .as_array()
                .is_some_and(|argv| argv.iter().any(|arg| arg == "log")))
            .count(),
        1
    );
    assert!(!reads.iter().any(|child| child["argv"]
        .as_array()
        .is_some_and(|argv| argv.iter().any(|arg| arg == "--skip=200"))));
    let back = trace(&native).len();
    choose(&mut session, mouse, "[Back]", b"\x1b[1;3D");
    visible_page(&mut session, 10_000, 400);
    session.wait_for_screen(&format!("▶ selected {}", &selected[..8]));
    session.wait_for_screen("Graph horizontal: 8 cells");
    exact_recheck(&native, back, 10);
    effects(&native, &session, None);
    session.quit_and_assert_restored_terminal(mouse);
}
fn pending_refresh(mouse: bool, replace: bool) {
    let native = ordinary10000();
    let pid_file = held_restore(&native);
    let mut session = launch(&native, mouse);
    load_all(&native, &mut session, mouse);
    let selected = older400(&native, &mut session, mouse);
    fs::write(
        native.fixture.root.join("arm-restore"),
        b"hold actual fresh snapshot member\n",
    )
    .unwrap();
    let before = trace(&native).len();
    choose_bar_or_palette(&mut session, mouse, "[r Refresh]", b"r");
    until(
        &mut session,
        || {
            fs::read_to_string(&pid_file)
                .is_ok_and(|raw| raw.trim().parse::<i32>().is_ok_and(|pid| pid > 0))
        },
        "Actual fresh Refresh member child must enter",
    );
    let pid = fs::read_to_string(pid_file)
        .unwrap()
        .trim()
        .parse::<i32>()
        .unwrap();
    assert_eq!(unsafe { libc::kill(pid, 0) }, 0);
    session.wait_for_control("[Esc Cancel]");
    let mut original = None;
    if replace {
        let retired = native.fixture.root.join("retired-original-graph-v3");
        fs::rename(native.fixture.repo(), &retired).unwrap();
        fs::create_dir(native.fixture.repo()).unwrap();
        native.fixture.init_git();
        native.fixture.commit_file(
            "readme",
            b"REPLACEMENT PRIVATE GRAPH CANARY\n",
            "REPLACEMENT PRIVATE GRAPH MUST NEVER LOAD",
        );
        original = Some(retired);
        fs::write(
            native.fixture.root.join("release-restore"),
            b"release original child after inode replacement\n",
        )
        .unwrap();
        session.wait_for_screen("Error:");
        assert!(!session.screen().contains("REPLACEMENT PRIVATE GRAPH"));
    } else {
        choose(&mut session, mouse, "[Esc Cancel]", b"\x1b");
        session.wait_for_screen("Cancelled");
    }
    until(
        &mut session,
        || unsafe { libc::kill(pid, 0) } != 0,
        "Original Refresh process group must be gone",
    );
    session.wait_for_main_frame();
    visible_page(&mut session, 10_000, 400);
    session.wait_for_screen(&format!("▶ selected {}", &selected[..8]));
    assert!(!session
        .screen()
        .contains("Original Graph snapshot refreshed"));
    let current = trace(&native);
    assert!(!current[before..].iter().any(|child| child["argv"]
        .as_array()
        .is_some_and(|argv| argv
            .iter()
            .any(|arg| arg == "log" || arg == "rev-list" || arg == "show-ref"))));
    effects(&native, &session, original.as_deref());
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn commit_graph_v3_native_keyboard_full10000_snapshot_refresh_dirty_and_exact_back() {
    refresh_full(false)
}
#[test]
fn commit_graph_v3_native_mouse_full10000_snapshot_refresh_dirty_and_exact_back() {
    refresh_full(true)
}
#[test]
fn commit_graph_v3_native_keyboard_full10000_deliberate_first200_reread_preview_back_and_retained_old(
) {
    deliberate_reread(false)
}
#[test]
fn commit_graph_v3_native_mouse_full10000_deliberate_first200_reread_preview_back_and_retained_old()
{
    deliberate_reread(true)
}
#[test]
fn commit_graph_v3_native_keyboard_full10000_held_original_refresh_cancel() {
    pending_refresh(false, false)
}
#[test]
fn commit_graph_v3_native_mouse_full10000_held_original_refresh_cancel() {
    pending_refresh(true, false)
}
#[test]
fn commit_graph_v3_native_keyboard_full10000_held_original_refresh_source_replaced() {
    pending_refresh(false, true)
}
#[test]
fn commit_graph_v3_native_mouse_full10000_held_original_refresh_source_replaced() {
    pending_refresh(true, true)
}
