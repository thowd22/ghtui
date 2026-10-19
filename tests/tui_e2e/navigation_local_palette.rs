//! Actual compiled-binary navigation journeys, registered and verified with
//! keyboard/mouse input and independent real Git, gh, storage and TTY effects.
use crate::support::{Fixture, Session};
#[path = "repository_source_identity_native.rs"]
mod source_identity;
use serde_json::Value;
use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};

const PALETTE: &str = "Commands · current context"; // Actual prepared widget.
const HELP_OVERLAY: &str = "Help • keyboard and mouse"; // Confirmed Root header.
const REPOSITORY_HELP: &str = "Repository • help for this view"; // Actual producer.
const SEARCH_HELP: &str = "Search • help for this view"; // Actual producer.
                                                         // Global navigation, not a File back/toggle alias.
const GLOBAL_BACK: &str = "Alt-← Back";
const WORKING_CRUMB: &str = "Repository • Working diff";
const GLOBAL_BACK_KEY: &[u8] = b"\x1b[1;3D"; // Resolved default Alt+Left.

#[derive(Debug, PartialEq, Eq)]
struct GitState {
    head: String,
    refs: String,
    index: Vec<u8>,
    status: String,
    dirty: Vec<u8>,
    staged: Vec<u8>,
    untracked: Vec<u8>,
}
fn state(fixture: &Fixture) -> GitState {
    GitState {
        head: fixture.git(&["rev-parse", "HEAD"]),
        refs: fixture.git(&["show-ref"]),
        index: fs::read(fixture.repo().join(".git/index")).unwrap(),
        status: fixture.git(&["status", "--porcelain=v2", "-z"]),
        dirty: fs::read(fixture.repo().join("nested/日本語.txt")).unwrap(),
        staged: fs::read(fixture.repo().join("staged.txt")).unwrap(),
        untracked: fs::read(fixture.repo().join("scratch界.txt")).unwrap(),
    }
}
struct LocalFixture {
    fixture: Fixture,
    original: String,
    before: GitState,
}
fn fixture() -> LocalFixture {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[]); // Reject EVERY unexpected gh command.
    fixture.init_git();
    fixture.commit_file(
        "nested/日本語.txt",
        "saved navigation beacon α\nunchanged\n".as_bytes(),
        "Original navigation ancestry",
    );
    let original = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    fixture.commit_file(
        "staged.txt",
        b"saved stage baseline\n",
        "Later navigation version",
    );
    fixture.git(&["tag", "navigation-original", &original]);
    fs::write(
        fixture.repo().join("nested/日本語.txt"),
        "working navigation needle β\nunchanged\n".as_bytes(),
    )
    .unwrap();
    fs::write(
        fixture.repo().join("staged.txt"),
        b"staged navigation bytes\n",
    )
    .unwrap();
    fixture.git(&["add", "--", "staged.txt"]);
    fs::write(
        fixture.repo().join("scratch界.txt"),
        "untouched scratch content 🦀\n".as_bytes(),
    )
    .unwrap();
    let before = state(&fixture);
    assert!(
        before.status.contains("日本語.txt")
            && before.status.contains("staged.txt")
            && before.status.contains("scratch界.txt")
    );
    LocalFixture {
        fixture,
        original,
        before,
    }
}
fn launch(fixture: &Fixture, mouse: bool) -> Session {
    let trace = fixture.root.join("navigation-child-git-trace.jsonl");
    let trace = trace.to_str().unwrap();
    let env = if mouse {
        vec![("GIT_TRACE2_EVENT", trace), ("NO_COLOR", "1")]
    } else {
        vec![("GIT_TRACE2_EVENT", trace)]
    };
    Session::launch_binary(
        Path::new(env!("CARGO_BIN_EXE_ghtui")),
        fixture,
        120,
        40,
        &env,
    )
}
fn trace(fixture: &Fixture) -> Vec<Value> {
    fs::read_to_string(fixture.root.join("navigation-child-git-trace.jsonl"))
        .expect("Child Git trace must exist; missing instrumentation is not zero Git calls")
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn starts(fixture: &Fixture) -> usize {
    trace(fixture)
        .iter()
        .filter(|row| row["event"] == "start")
        .count()
}
fn assert_read_only(fixture: &Fixture) {
    let trace = trace(fixture);
    assert!(
        trace.iter().any(|row| row["event"] == "cmd_name"),
        "Actual child Git commands must be independently observed"
    );
    assert!(!trace
        .iter()
        .filter(|row| row["event"] == "cmd_name")
        .any(|row| matches!(
            row["name"].as_str(),
            Some(
                "add"
                    | "reset"
                    | "checkout"
                    | "switch"
                    | "commit"
                    | "merge"
                    | "rebase"
                    | "cherry-pick"
                    | "revert"
                    | "stash"
                    | "fetch"
                    | "push"
                    | "pull"
                    | "clone"
                    | "update-ref"
                    | "rm"
            )
        )));
}
fn choose(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    if mouse {
        session.click_visible_action(label);
    } else {
        session.send_key(key);
    }
}
fn body_ready(session: &mut Session, title: &str, required: &[&str]) -> String {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(10));
        if let Some(body) = session.visible_overlay_body(title) {
            let visible_words = body.split_whitespace().collect::<Vec<_>>().join(" ");
            if required.iter().all(|value| {
                visible_words.contains(&value.split_whitespace().collect::<Vec<_>>().join(" "))
            }) {
                return body;
            }
        }
        if Instant::now() >= deadline {
            session.save_artifacts();
            panic!(
                "Incomplete actual overlay {title:?}, required {required:?}, title cell {:?}, observed body {:?}\n{}",
                session.locate(title), session.visible_overlay_body(title), session.screen()
            );
        }
    }
}
fn open_palette(session: &mut Session, mouse: bool) {
    choose(session, mouse, "[: Commands]", b":");
    body_ready(
        session,
        PALETTE,
        &["Filter focused:", "[Enter Choose]", "[Esc Close]"],
    );
}
fn completed_notice(session: &mut Session, title: &str) {
    let previous = session.completed_cursorless_end();
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(5));
        if session.completed_cursorless_end() > previous
            && session.completed_cursorless_screen().is_some_and(|frame| {
                frame.contents().contains(title) && frame.contents().contains("[Esc Close]")
            })
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "No newer completed owned notice before Close: {title}\n{}",
            session.screen()
        );
    }
}
fn filter(session: &mut Session, mouse: bool, label: &str) {
    if mouse {
        session.click_visible_action("Filter");
    }
    session.send_key(b"\x01");
    session.paste(label);
    body_ready(session, PALETTE, &[label, "[Enter Choose]"]);
}
fn command(session: &mut Session, mouse: bool, label: &str) {
    open_palette(session, mouse);
    filter(session, mouse, label);
    activate_result(session, mouse, label);
    session.wait_for_overlay_close(PALETTE);
}
fn activate_result(session: &mut Session, mouse: bool, label: &str) {
    let selected = format!("> {label}");
    body_ready(session, PALETTE, &[&selected]);
    // The query field also contains label; click the actually rendered result
    // marker, not the first matching filter text or a direct handler.
    choose(session, mouse, &selected, b"\r");
}
fn selected_row(session: &mut Session, mouse: bool, label: &str) {
    if mouse {
        session.click_visible_action(label);
        return;
    }
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(10));
        if session
            .locate(label)
            .is_some_and(|(column, row)| session.cell_inverted(row, column))
        {
            session.send_key(b"\r");
            return;
        }
        if Instant::now() >= deadline {
            session.save_artifacts();
            panic!(
                "Exact Git row never selected: {label}\n{}",
                session.screen()
            );
        }
        session.send_key(b"\x1b[B");
    }
}
/// Help opens with the view's footer keys as titled key tables (`Keys • …`) before the prose.
/// Scroll its body a step at a time (Down, or the wheel over the body) until every `required`
/// prose fragment is in view together, waiting for each scrolled frame.
fn help_prose(session: &mut Session, mouse: bool, required: &[&str]) {
    let normalized = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
    for _ in 0..96 {
        let visible = session
            .visible_overlay_body(HELP_OVERLAY)
            .is_some_and(|body| {
                let body = normalized(&body);
                required
                    .iter()
                    .all(|value| body.contains(&normalized(value)))
            });
        if visible {
            return;
        }
        let after = session.completed_cursorless_end();
        if mouse {
            let (x, y) = session.locate(HELP_OVERLAY).expect("Help title border");
            session.wheel(x + 2, y + 2, true);
        } else {
            session.send_key(b"\x1b[B");
        }
        session.wait_for_new_cursorless_condition(
            after,
            |frame| frame.contents().contains(HELP_OVERLAY),
            "scrolled Help frame",
            Duration::from_secs(8),
        );
    }
    session.save_artifacts();
    panic!(
        "Help prose {required:?} never scrolled into view\n{}",
        session.screen()
    );
}
fn close_help(session: &mut Session, mouse: bool, title: &str) {
    if mouse {
        session.close_overlay_by_mouse(title);
    } else {
        session.close_overlay_by_key(title);
    }
}
fn check_effects(local: &LocalFixture, session: &Session) {
    assert_eq!(state(&local.fixture), local.before);
    assert!(
        session.inspect_command_journal(&local.fixture).is_empty(),
        "All local navigation must leave strict fake-gh journal0"
    );
    assert_read_only(&local.fixture);
}
fn check_local_storage_only(fixture: &Fixture) {
    let database = fixture.root.join("config/ghtui/cache.sqlite3");
    let connection =
        rusqlite::Connection::open_with_flags(database, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("Successful local Search creates the real scoped cache");
    for table in ["records", "saved_views"] {
        let count: u64 = connection
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE host <> 'local'"),
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 0,
            "Local navigation must not manufacture private GitHub {table}"
        );
    }
    let drafts: u64 = connection
        .query_row("SELECT COUNT(*) FROM drafts", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        drafts, 0,
        "Read-only navigation/Help must not prepare any mutation draft"
    );
}

#[test]
fn navigation_local_palette_keyboard_working_graph_context_help_search_back_and_breadcrumb_test_framework(
) {
    journey(false);
}
#[test]
fn navigation_local_palette_mouse_working_graph_context_help_search_back_and_breadcrumb_test_framework(
) {
    journey(true);
}
fn journey(mouse: bool) {
    let local = fixture();
    let mut session = launch(&local.fixture, mouse);
    session.wait_for_screen("Recent commits");
    session.assert_focused_view("Repository");
    assert!(starts(&local.fixture) > 0);
    let search_tab = session
        .locate("3 Search")
        .expect("Underlying Search tab before overlay");
    let count = starts(&local.fixture);
    open_palette(&mut session, mouse);
    // A background tab click must not change the real view or trigger a read.
    if mouse {
        session.click(search_tab.0, search_tab.1);
    }
    filter(&mut session, mouse, "Show Working diff");
    assert_eq!(
        starts(&local.fixture),
        count,
        "Filter/caret/overlay capture must not run Git"
    );
    assert!(session.inspect_command_journal(&local.fixture).is_empty());
    activate_result(&mut session, mouse, "Show Working diff");
    session.wait_for_overlay_close(PALETTE);
    session.wait_for_screen("working navigation needle β");
    session.wait_for_screen("Repository loaded • read-only exploration");
    command(&mut session, mouse, "Show Graph");
    session.wait_for_screen("loaded (limit 10000)");
    session.open_saved_graph_subject(mouse, "Original navigation ancestry", &local.original);
    session.wait_for_screen(&local.original);
    let selected = session.locate("Original navigation ancestry").unwrap();
    assert!(
        session.cell_inverted(selected.1, selected.0),
        "Actual responsible commit selection must remain visible"
    );
    let count = starts(&local.fixture);
    command(&mut session, mouse, "Help for this view");
    // The Repository key tables come first; the Git prose follows them.
    body_ready(&mut session, HELP_OVERLAY, &[REPOSITORY_HELP, "Keys • "]);
    help_prose(
        &mut session,
        mouse,
        &["Git in plain language", "Commit = a saved version"],
    );
    if mouse {
        session.click(search_tab.0, search_tab.1);
    } else {
        session.send_key(b"3");
    }
    session.resize(80, 24);
    body_ready(&mut session, HELP_OVERLAY, &[]);
    help_prose(&mut session, mouse, &["Git in plain language"]);
    assert_eq!(
        starts(&local.fixture),
        count,
        "Context Help/resize must not run Git"
    );
    close_help(&mut session, mouse, HELP_OVERLAY);
    session.assert_focused_view("Repository");
    session.wait_for_screen(&local.original);
    command(&mut session, mouse, "Open Search");
    session.wait_for_screen("Selected local search corpus ready");
    session.assert_focused_view("Search");
    choose(&mut session, mouse, "e Exact", b"e");
    choose(&mut session, mouse, "[/ Query]", b"/");
    session.send_key(b"\x01");
    session.paste("working navigation needle");
    choose(&mut session, mouse, "[Enter Search]", b"\r");
    session.wait_for_screen("1 results • Exact");
    session.wait_for_screen("日本語.txt:1:1");
    check_local_storage_only(&local.fixture);
    let count = starts(&local.fixture);
    command(&mut session, mouse, "Help for this view");
    body_ready(&mut session, HELP_OVERLAY, &[SEARCH_HELP, "Keys • "]);
    help_prose(
        &mut session,
        mouse,
        &["Choose the query's meaning", "Exact searches literal text"],
    );
    close_help(&mut session, mouse, HELP_OVERLAY);
    assert_eq!(starts(&local.fixture), count);
    session.wait_for_screen("1 results • Exact");
    // Global route: never replace with repository File back.
    let before_back = session.completed_cursorless_end();
    choose(&mut session, mouse, GLOBAL_BACK, GLOBAL_BACK_KEY);
    session.assert_focused_view("Repository");
    session.wait_for_screen("loaded (limit 10000)");
    session.wait_for_screen(&local.original);
    let selected = session.wait_for_saved_graph_return(
        before_back,
        &local.original,
        "Original navigation ancestry",
    );
    assert!(
        session
            .completed_cursorless_screen()
            .unwrap()
            .cell(selected.1, selected.0)
            .unwrap()
            .inverse(),
        "Back restores actual canonical commit, not old row index"
    );
    session.resize(120, 40);
    session.wait_for_screen(WORKING_CRUMB);
    // Original owner/epoch-bound crumb, not a generic Repository tab.
    if mouse {
        session.click_visible_action(WORKING_CRUMB);
    } else {
        session.focus_visible_action(WORKING_CRUMB);
        session.send_key(b"\r");
    }
    session.wait_for_screen("working navigation needle β");
    session.assert_focused_view("Repository");
    // The restored Working source is the later original checkout HEAD, not the
    // old Graph commit selected before leaving. Its full identity must be
    // readable independently of a clipped long-directory context breadcrumb.
    session.wait_for_screen(local.before.head.trim());
    check_effects(&local, &session);
    check_local_storage_only(&local.fixture);
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn navigation_local_palette_keyboard_disabled_explanation_no_match_capture_and_resize() {
    unavailable(false);
}
#[test]
fn navigation_local_palette_mouse_disabled_explanation_no_match_capture_and_resize() {
    unavailable(true);
}
fn unavailable(mouse: bool) {
    let local = fixture();
    let mut session = launch(&local.fixture, mouse);
    session.wait_for_screen("Recent commits");
    command(&mut session, mouse, "Show Graph");
    session.wait_for_screen("loaded (limit 10000)");
    session.open_saved_graph_subject(mouse, "Original navigation ancestry", &local.original);
    session.wait_for_screen(&local.original);
    let count = starts(&local.fixture);
    open_palette(&mut session, mouse);
    filter(&mut session, mouse, "unmatched-navigation-command-🦀");
    body_ready(&mut session, PALETTE, &["No matching commands"]);
    choose(&mut session, mouse, "[Enter Choose]", b"\r");
    body_ready(&mut session, PALETTE, &["No matching commands"]);
    assert_eq!(starts(&local.fixture), count);
    filter(&mut session, mouse, "Stage selected lines");
    body_ready(
        &mut session,
        PALETTE,
        &[
            "Stage selected lines",
            "[why unavailable]",
            "canonical added or removed",
        ],
    );
    session.resize(80, 24);
    body_ready(
        &mut session,
        PALETTE,
        &["Stage selected lines", "[why unavailable]"],
    );
    activate_result(&mut session, mouse, "Stage selected lines");
    session.wait_for_overlay_close(PALETTE);
    session.wait_for_screen("Action unavailable • explanation only");
    session.wait_for_screen("canonical added or removed");
    let search_tab = session.locate("3 Search").unwrap();
    if mouse {
        session.click(search_tab.0, search_tab.1);
    } else {
        session.send_key(b"3");
    }
    completed_notice(&mut session, "Action unavailable • explanation only");
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_overlay_close("Action unavailable • explanation only");
    session.assert_focused_view("Repository");
    session.wait_for_screen(&local.original);
    assert_eq!(
        starts(&local.fixture),
        count,
        "Disabled command selection must not execute Git or prepare a mutation"
    );
    assert!(
        !session.screen().contains("LOCAL INTENT")
            && !session.screen().contains("READ-ONLY PREVIEW")
    );
    check_effects(&local, &session);
    session.quit_and_assert_restored_terminal(mouse);
}

#[path = "navigation_local_intent.rs"]
mod intent;
