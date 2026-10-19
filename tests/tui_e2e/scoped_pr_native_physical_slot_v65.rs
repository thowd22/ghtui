//! Ordinary real-binary routing after genuine Native pure Back. This does NOT
//! claim deterministic reproduction of the pre-main poll/take_work interval.
use super::*;
// At 80 columns the focused frame title left-ellipsizes the source label
// ("…al/current @ working files"); the coverage row still names the exact
// local source and its exclusions in full at every supported width.
const LOCAL_SOURCE: &str = "local/current working files: ignores excluded";
const LOCAL_SOURCE_TITLE: &str = "current @ working files";

fn all_sql(fixture: &Fixture) -> Vec<String> {
    let database = rusqlite::Connection::open_with_flags(
        database(fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let names = database
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    let mut all = Vec::new();
    for name in names {
        let mut statement = database
            .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
            .unwrap();
        let count = statement.column_count();
        let mut rows = statement
            .query_map([], |r| {
                let columns = (0..count)
                    .map(|i| r.get::<_, rusqlite::types::Value>(i))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(format!("{name}:{columns:?}"))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        rows.sort();
        all.extend(rows);
    }
    all
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
    choose(session, mouse, "[Enter Search]", b"\r");
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

fn reopen_catalogue(session: &mut Session, fixture: &Fixture, mouse: bool, steps: &[GhStep]) {
    // The Catalogue-only helper excludes the genuine pr diff read from this
    // full Files/Images/Native protocol. Keep it unchanged; validate every
    // actual ordinal, exact argv and empty stdin against the original 58 here.
    assert_eq!(steps.len(), 58);
    exact_reads(session, fixture, steps);
    for step in steps {
        assert!(
            step.args == ["--version"]
                || (step.args.first().is_some_and(|arg| arg == "api")
                    && step.args.windows(2).any(|args| args == ["--method", "GET"]))
                || step.args
                    == [
                        "pr",
                        "diff",
                        "7",
                        "--repo",
                        &format!("{HOST}/{FIRST}"),
                        "--color",
                        "never"
                    ]
        );
    }
    choose(session, mouse, "[: Commands]", b":");
    overlay_ready(
        session,
        "Commands · current context",
        &["Filter focused:", "[Enter Choose]"],
    );
    session.send_key(b"\x01");
    session.paste("Discover repositories");
    overlay_ready(
        session,
        "Commands · current context",
        &["> Discover repositories"],
    );
    choose(session, mouse, "> Discover repositories", b"\r");
    session.wait_for_overlay_close("Commands · current context");
    ready(session, &[TITLE, "[s Source]", "[Esc Close]"]);
    exact_reads(session, fixture, steps);
}

fn resubmitted_search_complete(
    session: &mut Session,
    fixture: &Fixture,
    after: usize,
    original_git_trace: &str,
) {
    let directory = fixture.repo().canonicalize().unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        let trace = fs::read_to_string(fixture.root.join("catalogue-git-trace.jsonl")).unwrap();
        let new_reads = trace.strip_prefix(original_git_trace).unwrap();
        // Read only COMPLETE real Trace2 records while the child may still
        // append. This exact installed-Git corpus read cannot be supplied by
        // a pre-Submit redraw of the unchanged query/old hit.
        let records = new_reads
            .split_inclusive('\n')
            .filter(|row| row.ends_with('\n'))
            .map(|row| serde_json::from_str::<Value>(row).unwrap())
            .collect::<Vec<_>>();
        let actual_corpus_read = records.iter().any(|start| {
            start["event"] == "start"
                && start["argv"].as_array().is_some_and(|args| {
                    args.iter().any(|arg| arg == "for-each-ref")
                        && args.iter().any(|arg| {
                            arg == "--format=%(refname)%00%(objectname)%00%(upstream)%00%(HEAD)%00"
                        })
                })
                && records.iter().any(|row| {
                    row["sid"] == start["sid"]
                        && row["event"] == "def_repo"
                        && row["worktree"].as_str() == directory.to_str()
                })
                && records.iter().any(|row| {
                    row["sid"] == start["sid"] && row["event"] == "exit" && row["code"] == 0
                })
        });
        if actual_corpus_read
            && session.completed_cells_end() > after
            && session.completed_cells_screen().is_some_and(|frame| {
                let text = frame.contents();
                crate::support::main_footer_visible(frame)
                    && text.lines().any(|row| {
                        row.contains("Search • focused view") && row.contains(LOCAL_SOURCE_TITLE)
                    })
                    && text.contains("Search complete • exact source and exclusions shown")
                    && text.contains(LOCAL_SOURCE)
                    && text.contains("1 results • Exact")
                    && text.contains("dirty界.txt:1")
                    && !text.contains("[Esc Cancel]")
                    && text
                        .lines()
                        .any(|row| row.contains("[/ Query]") && row.contains("dirty checkout"))
            })
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Actual post-Submit installed-Git corpus read and NEW completed Search result required before Discovery\n{}",
            session.screen()
        );
    }
}

fn routing(mouse: bool) {
    let (fixture, h, steps) = fixture_and_steps(|_| {});
    let git = readonly_git(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    preselect_local_search(&mut session, &fixture, mouse);
    files_selected(&mut session, &fixture, mouse);
    choose(&mut session, mouse, "[v Native diff]", b"v");
    session.wait_for_command_count(&fixture, 58);
    nready(&mut session, "new-no-newline");
    let sql = all_sql(&fixture);
    let resource_sql = sql_state(&fixture);
    let trace = commands(&session, &fixture);
    assert_eq!(trace.len(), 58);
    session.resize_owned_modal(80, 24, NATIVE, &["[Esc Close]", "[o Options]"]);
    nready(&mut session, "new-no-newline");
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
    nready(&mut session, "new-no-newline");
    assert_eq!(
        commands(&session, &fixture),
        trace,
        "Retained images reopen is pure"
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
    assert_eq!(all_sql(&fixture), sql);
    let before = session.completed_cursorless_end();
    choose(&mut session, mouse, "2 Repository", b"2");
    session.wait_for_new_cursorless_condition(
        before,
        |frame| {
            frame.contents().contains("Repository • focused view")
                && crate::support::main_footer_visible(frame)
        },
        "NEW genuine Repository view after Native Back",
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
                && !frame.contents().contains("[Esc Cancel]")
                && crate::support::main_footer_visible(frame)
        },
        "Actual local Repository read completes after pure Native Back",
        Duration::from_secs(8),
    );
    assert_eq!(commands(&session, &fixture), trace);
    assert_eq!(all_sql(&fixture), sql);
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
    choose(&mut session, mouse, "[/ Query]", b"/");
    session.send_key(b"\x01");
    session.paste("dirty checkout");
    let query_trace = fs::read_to_string(fixture.root.join("catalogue-git-trace.jsonl")).unwrap();
    let before = session.completed_cells_end();
    choose(&mut session, mouse, "[Enter Search]", b"\r");
    // The retained hit was already visible. Wait for this actual query's
    // completed worker/status before opening a palette that completion would
    // correctly replace. This is not a permission or result-source shortcut.
    resubmitted_search_complete(&mut session, &fixture, before, &query_trace);
    let resized_after = session.completed_cells_end();
    session.resize(if mouse { 120 } else { 80 }, if mouse { 40 } else { 24 });
    let resized = search_frame(
        &mut session,
        resized_after,
        &[
            LOCAL_SOURCE,
            "1 results • Exact",
            "dirty界.txt:1",
            "Search complete • exact source and exclusions shown",
        ],
    );
    assert!(!resized.contains("[Esc Cancel]"));
    assert_eq!(commands(&session, &fixture), trace);
    assert_eq!(all_sql(&fixture), sql);
    reopen_catalogue(&mut session, &fixture, mouse, &steps);
    session.resize_owned_modal(120, 40, TITLE, &["[q Quit]", "[Esc Close]"]);
    ready(&mut session, &[TITLE, "[s Source]", "[Esc Close]"]);
    assert_eq!(commands(&session, &fixture), trace);
    assert_eq!(all_sql(&fixture), sql);
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_overlay_close(TITLE);
    session.wait_for_main_frame();
    session.assert_focused_view("Search");
    session.wait_for_screen("dirty界.txt:1");
    let inbox_after = session.completed_cursorless_end();
    choose(&mut session, mouse, "1 Inbox", b"1");
    // These toolbar labels arrive before the actual Files body and its final
    // cursorless completion. Never combine that partial parser frame with the
    // previously completed Search frame when checking the retained original H.
    session.wait_for_new_cursorless_condition(
        inbox_after,
        |frame| {
            crate::support::main_footer_visible(frame)
                && crate::support::overlay_body(frame, "Inbox • focused view").is_some_and(|body| {
                    body.contains("[f Files]")
                        && body.contains("[v Native diff]")
                        && body.contains("Files: all 1 loaded")
                        && body.contains("File src/界.rs")
                        && body.contains("new-no-newline")
                        && body
                            .chars()
                            .filter(|character| !character.is_whitespace())
                            .collect::<String>()
                            .contains(&h.h)
                })
        },
        "NEW completed exact cached original Inbox Files/H after pure Discovery Back",
        Duration::from_secs(8),
    );
    // Cached original Files is not a new provider proof or write grant.
    let screen = session.completed_cursorless_screen().unwrap().contents();
    let compact = screen
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>();
    assert!(
        compact.contains(&h.h),
        "Exact original reviewed H remains visible after cached Back"
    );
    assert_eq!(commands(&session, &fixture), trace);
    assert_eq!(all_sql(&fixture), sql);
    assert_eq!(readonly_git(&fixture), git);
    assert_read_only_git(&fixture);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    session.capture_completed_terminal_v1("native-physical-slot-normal-routing-v65");
    session.quit_and_assert_restored_terminal(mouse);
    unchanged(&session, &fixture, &resource_sql, &steps);
    assert_eq!(all_sql(&fixture), sql);
    assert_eq!(readonly_git(&fixture), git);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    assert_read_only_git(&fixture);
}

#[test]
fn scoped_pr_native_diff_physical_slot_v65_keyboard_native_back_repository_search_discovery_normal_routing(
) {
    routing(false);
}
#[test]
fn scoped_pr_native_diff_physical_slot_v65_mouse_native_back_repository_search_discovery_normal_routing(
) {
    routing(true);
}
