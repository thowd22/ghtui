//! Normal cross-view routing regression, NOT a deterministic pre-main-tick
//! reproduction. There is no public pause between main poll and take_work.
use super::*;
// At 80 columns the focused frame title left-ellipsizes the source label
// ("…al/current @ working files"); the coverage row still names the exact
// local source and its exclusions in full at every supported width.
const LOCAL_SOURCE: &str = "local/current working files: ignores excluded";
const LOCAL_SOURCE_TITLE: &str = "current @ working files";
fn all_sql(fixture: &Fixture) -> Vec<String> {
    let db = rusqlite::Connection::open_with_flags(
        database(fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let names=db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap().query_map([],|r|r.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>();
    let mut out = Vec::new();
    for name in names {
        let mut statement = db
            .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
            .unwrap();
        let n = statement.column_count();
        let mut rows = statement
            .query_map([], |r| {
                let values = (0..n)
                    .map(|i| r.get::<_, rusqlite::types::Value>(i))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(format!("{name}:{values:?}"))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        rows.sort();
        out.extend(rows);
    }
    out
}
fn search_frame(session: &mut Session, after: usize, required: &[&str]) -> String {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(2));
        if session.completed_cells_end() > after {
            if let Some(frame) = session.completed_cells_screen() {
                let text = frame.contents();
                if text.lines().any(|row| {
                    row.contains("Search • focused view") && row.contains(LOCAL_SOURCE_TITLE)
                }) && crate::support::main_footer_visible(frame)
                    && required.iter().all(|word| text.contains(word))
                {
                    return text;
                }
            }
        }
        assert!(
            Instant::now() < deadline,
            "NEW completed actual local Search frame missing {required:?}\n{}",
            session.screen()
        );
    }
}
fn preselect_local_search(session: &mut Session, fixture: &Fixture, mouse: bool) {
    let sql = all_sql(fixture);
    let gh = session.inspect_command_journal(fixture);
    let git_trace = fs::read_to_string(fixture.root.join("catalogue-git-trace.jsonl")).unwrap();
    session.resize(120, 40);
    let before = session.completed_cells_end();
    choose(session, mouse, "3 Search", b"3");
    search_frame(
        session,
        before,
        &[
            "Selected local search corpus ready",
            "local/current @ working files",
        ],
    );
    let before = session.completed_cells_end();
    choose(session, mouse, "[c Corpus]", b"c");
    let directory = fixture.repo().canonicalize().unwrap();
    search_frame(
        session,
        before,
        &["[x] local/current", directory.to_str().unwrap()],
    );
    let before = session.completed_cells_end();
    choose(session, mouse, "[c Corpus]", b"c");
    search_frame(
        session,
        before,
        &["local/current @ working files", "Exact mode"],
    );
    choose(session, mouse, "[/ Query]", b"/");
    session.send_key(b"\x01");
    session.paste("dirty checkout");
    let before = session.completed_cells_end();
    session.begin_search_completion_v1(
        "dirty checkout",
        "local/current @ working files",
        "dirty界.txt:1",
    );
    choose(session, mouse, "[Enter Search]", b"\r");
    session.wait_search_completion_v1();
    let text = search_frame(
        session,
        before,
        &[
            "1 results • Exact",
            "dirty界.txt:1",
            "local/current @ working files",
        ],
    );
    assert!(text
        .lines()
        .any(|row| row.contains("[/ Query]") && row.contains("dirty checkout")));
    let actual_trace = fs::read_to_string(fixture.root.join("catalogue-git-trace.jsonl")).unwrap();
    let new_reads = actual_trace.strip_prefix(&git_trace).unwrap();
    assert!(new_reads
        .lines()
        .map(|row| serde_json::from_str::<Value>(row).unwrap())
        .any(|row| row["event"] == "def_repo" && row["worktree"].as_str() == directory.to_str()));
    assert_eq!(session.inspect_command_journal(fixture), gh);
    assert_eq!(all_sql(fixture), sql);
    let before = session.completed_cursorless_end();
    choose(session, mouse, "2 Repository", b"2");
    session.wait_for_new_cursorless_condition(
        before,
        |frame| {
            frame.contents().contains("Repository • focused view")
                && crate::support::main_footer_visible(frame)
        },
        "Return from genuine preselected local Search before any remote source",
        Duration::from_secs(8),
    );
    session.resize(if mouse { 80 } else { 120 }, if mouse { 24 } else { 40 });
    assert_eq!(session.inspect_command_journal(fixture), gh);
    assert_eq!(all_sql(fixture), sql);
    assert_read_only_git(fixture);
}
fn navigation(mouse: bool) {
    let fixture = configured(&actions_protocol());
    Cache::open(&database(&fixture))
        .unwrap()
        .save_view(
            &Scope::github(HOST, Some(42)).unwrap(),
            INTENT_CANARY,
            &original_intent(),
        )
        .unwrap();
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    // A tagged remote source may retain an already chosen independent local
    // corpus; it must not rebuild one from Config as a legacy fallback.
    preselect_local_search(&mut session, &fixture, mouse);
    open_catalogue(&mut session, &fixture, mouse, 0);
    discover(&mut session, mouse);
    session.wait_for_command_count(&fixture, 7);
    ready(&mut session, &[TITLE, FIRST, "Repository ID: 10000"]);
    filter(
        &mut session,
        mouse,
        "repo0000",
        FIRST,
        "Repository ID: 10000",
    );
    choose(&mut session, mouse, "[Open Actions]", b"A");
    session.wait_for_command_count(&fixture, 14);
    session.wait_for_overlay_close(TITLE);
    actions_ready(&mut session);
    assert_actions_commands(&session, &fixture);
    let sql = all_sql(&fixture);
    let trace = session.inspect_command_journal(&fixture);
    let before = session.completed_cursorless_end();
    choose(&mut session, mouse, "2 Repository", b"2");
    session.wait_for_new_cursorless_condition(
        before,
        |frame| {
            frame.contents().contains("Repository • focused view")
                && crate::support::main_footer_visible(frame)
        },
        "NEW actual Repository frame after Actions",
        Duration::from_secs(8),
    );
    session.assert_focused_view("Repository");
    session.wait_for_screen("dirty界.txt");
    let before = session.completed_cursorless_end();
    choose(&mut session, mouse, "[r Refresh]", b"r");
    session.wait_for_new_cursorless_condition(
        before,
        |frame| {
            frame.contents().contains("dirty界.txt")
                && crate::support::main_footer_visible(frame)
                && crate::support::locate_control(frame, "[Esc Cancel]").is_none()
        },
        "Actual local Repository refresh completes independently",
        Duration::from_secs(8),
    );
    assert_eq!(all_sql(&fixture), sql);
    assert_eq!(session.inspect_command_journal(&fixture), trace);
    let before = session.completed_cells_end();
    choose(&mut session, mouse, "3 Search", b"3");
    let retained = search_frame(
        &mut session,
        before,
        &[LOCAL_SOURCE, "1 results • Exact", "dirty界.txt:1"],
    );
    assert!(retained
        .lines()
        .any(|row| row.contains("[/ Query]") && row.contains("dirty checkout")));
    session.assert_focused_view("Search");
    choose(&mut session, mouse, "[/ Query]", b"/");
    session.send_key(b"\x01");
    session.paste("dirty checkout");
    // The Loading frame replaces the coverage row; only the title tail names
    // the source while this new request is in flight.
    session.begin_search_completion_v1("dirty checkout", LOCAL_SOURCE_TITLE, "dirty界.txt:1");
    choose(&mut session, mouse, "[Enter Search]", b"\r");
    session.wait_search_completion_v1();
    session.wait_for_screen("1 results • Exact");
    session.wait_for_screen(LOCAL_SOURCE);
    session.wait_for_screen("dirty界.txt:1");
    session.resize(if mouse { 120 } else { 80 }, if mouse { 40 } else { 24 });
    session.wait_for_screen("dirty界.txt:1");
    session.wait_for_screen(LOCAL_SOURCE);
    assert_eq!(all_sql(&fixture), sql);
    assert_eq!(session.inspect_command_journal(&fixture), trace);
    // Genuine hidden discovery owner opens/closes with no load request. It
    // cannot turn Actions' pending intent or a retained name into a read proof.
    open_catalogue(&mut session, &fixture, mouse, 14);
    session.resize_owned_modal(120, 40, TITLE, &["[q Quit]", "[Esc Close]"]);
    ready(&mut session, &[TITLE, "[s Source]", "[Esc Close]"]);
    assert_eq!(all_sql(&fixture), sql);
    assert_eq!(session.inspect_command_journal(&fixture), trace);
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_overlay_close(TITLE);
    session.wait_for_main_frame();
    session.assert_focused_view("Search");
    session.wait_for_screen("dirty界.txt:1");
    choose(&mut session, mouse, "4 Actions", b"4");
    actions_ready(&mut session);
    assert_actions_commands(&session, &fixture);
    assert_private_intents_unchanged(&fixture);
    assert_eq!(all_sql(&fixture), sql);
    assert_eq!(session.inspect_command_journal(&fixture), trace);
    assert_eq!(git_state(&fixture), git);
    assert_read_only_git(&fixture);
    session.capture_completed_terminal_v1("actions-physical-slot-normal-routing-v65");
    session.quit_and_assert_restored_terminal(mouse);
    assert_actions_commands(&session, &fixture);
    assert_eq!(all_sql(&fixture), sql);
    assert_eq!(git_state(&fixture), git);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    assert_private_intents_unchanged(&fixture);
    assert_read_only_git(&fixture);
}
#[test]
fn scoped_actions_physical_slot_v65_test_framework_search_completion_keyboard_actual_actions_repository_search_discovery_normal_routing(
) {
    navigation(false);
}
#[test]
fn scoped_actions_physical_slot_v65_test_framework_search_completion_mouse_actual_actions_repository_search_discovery_normal_routing(
) {
    navigation(true);
}
