//! Real personal Inbox transport, borrowed timeline preparation and both inputs.
use super::*;
#[path = "../support/timeline_actor_observed_fixture_v1.rs"]
pub(crate) mod observed;
use std::{fs, time::Duration};

#[derive(Debug, PartialEq, Eq)]
struct GitStateV1 {
    head: String,
    refs: String,
    index: Vec<u8>,
    staged: String,
    dirty: String,
    tracked: Vec<u8>,
    untracked: Vec<u8>,
}
fn git_state(fixture: &Fixture) -> GitStateV1 {
    GitStateV1 {
        head: fixture.git(&["rev-parse", "HEAD"]),
        refs: fixture.git(&["show-ref", "--head"]),
        index: fs::read(fixture.repo().join(".git/index")).unwrap(),
        staged: fixture.git(&["diff", "--cached", "--binary"]),
        dirty: fixture.git(&["diff", "--binary"]),
        tracked: fs::read(fixture.repo().join("tracked")).unwrap(),
        untracked: fs::read(fixture.repo().join("scratch")).unwrap(),
    }
}
fn sql(fixture: &Fixture) -> Vec<String> {
    let db = rusqlite::Connection::open_with_flags(
        fixture.root.join("config/ghtui/cache.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    [
        "SELECT json_array(host,login,resource,body) FROM drafts ORDER BY host,login,resource",
        "SELECT json_array(host,login,kind,id,payload) FROM records ORDER BY host,login,kind,id",
        "SELECT json_array(host,login,name,payload) FROM saved_views ORDER BY host,login,name",
        "SELECT json_array(attempt_id,state) FROM pr_write_attempts ORDER BY attempt_id",
        "SELECT json_array(attempt_id,state) FROM actions_write_attempts ORDER BY attempt_id",
    ]
    .iter()
    .flat_map(|q| {
        db.prepare(q)
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>()
    })
    .collect()
}
fn git_commands(fixture: &Fixture) -> Vec<Value> {
    fs::read_to_string(fixture.root.join("timeline-git-trace.jsonl"))
        .expect("Actual Git instrumentation is not optional zero-effect evidence")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|record| record["event"] == "cmd_name")
        .collect()
}
fn journey(mouse: bool) {
    let fixture = configured();
    observed::forbid_http(&fixture);
    fixture.init_git();
    fixture.commit_file("tracked", b"saved\n", "Original timeline canary");
    fs::write(fixture.repo().join("tracked"), b"staged\n").unwrap();
    fixture.git(&["add", "tracked"]);
    fs::write(fixture.repo().join("tracked"), b"dirty\n").unwrap();
    fs::write(fixture.repo().join("scratch"), b"untracked\n").unwrap();
    let original_git = git_state(&fixture);
    let config_path = fixture.root.join("config/ghtui/config.toml");
    let config = fs::read(&config_path).unwrap();
    let database = fixture.root.join("config/ghtui/cache.sqlite3");
    drop(ghtui::pr_write_journal::PrWriteJournal::open(&database).unwrap());
    drop(ghtui::actions_write_journal::ActionsWriteJournal::open(&database).unwrap());
    let mut steps = initial(false);
    let mut details = pages();
    details[5].stdout = json!([observed::events()]).to_string().into_bytes();
    // This pure Summary->Timeline journey invokes exactly the seven original
    // detail reads, not lazy Metadata/Files/Checks tabs that it never opens.
    details.truncate(7);
    steps.extend(details);
    fixture.scripted_gh(&steps);
    let trace_path = fixture.root.join("timeline-git-trace.jsonl");
    let mut session = Session::launch_binary(
        std::path::Path::new(env!("CARGO_BIN_EXE_ghtui")),
        &fixture,
        120,
        40,
        &[("GIT_TRACE2_EVENT", trace_path.to_str().unwrap())],
    );
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "1 Inbox", b"1");
    session.assert_focused_view("Inbox");
    refresh(&mut session, mouse);
    session.wait_for_screen("3 unique PRs across 5 queue matches");
    choose(&mut session, mouse, "Improve auth", b"\r");
    session.wait_for_cursorless_condition(
        |f| f.contents().contains("Head aaaaaaaaa") && f.contents().contains("[t Timeline]"),
        "Completed exact original personal PR",
        Duration::from_secs(8),
    );
    journal(&mut session, &fixture, steps.len());
    let original_sql = sql(&fixture);
    let commands = session.inspect_command_journal(&fixture);
    let original_git_commands = git_commands(&fixture);
    assert!(!original_git_commands.is_empty());
    assert!(original_git_commands.iter().all(|r| !matches!(
        r["name"].as_str(),
        Some(
            "add"
                | "reset"
                | "commit"
                | "checkout"
                | "switch"
                | "update-ref"
                | "fetch"
                | "clone"
                | "pull"
                | "push"
                | "merge"
                | "rebase"
                | "stash"
                | "rm"
        )
    )));
    choose(&mut session, mouse, "[t Timeline]", b"t");
    session.wait_for_cursorless_condition(
        |f| {
            let s = f.contents();
            observed::EXPECTED.iter().all(|e| s.contains(e))
                && observed::ABSENT.iter().all(|e| !s.contains(e))
        },
        "Completed distinct timeline author/generator/review date and honest unknown actors",
        Duration::from_secs(8),
    );
    session.resize(80, 24);
    session.wait_for_cursorless_condition(
        |f| f.contents().contains("comment author") && f.contents().contains("[t Timeline]"),
        "Narrow timeline keeps semantic author role and clickable tab",
        Duration::from_secs(8),
    );
    session.resize(120, 40);
    choose(&mut session, mouse, "[b Back]", b"b");
    session.wait_for_cursorless_condition(
        |f| {
            f.contents().contains("3 unique PRs across 5 queue matches")
                && !f.contents().contains("Exact timeline comment body")
        },
        "Pure Back clears original timeline body",
        Duration::from_secs(8),
    );
    assert_eq!(session.inspect_command_journal(&fixture), commands);
    assert_eq!(git_commands(&fixture), original_git_commands);
    assert_eq!(sql(&fixture), original_sql);
    assert_eq!(git_state(&fixture), original_git);
    assert_eq!(fs::read(&config_path).unwrap(), config);
    observed::assert_http_zero(&fixture);
    session.quit_and_assert_restored_terminal(mouse);
    journal(&mut session, &fixture, steps.len());
    assert_eq!(sql(&fixture), original_sql);
    assert_eq!(git_state(&fixture), original_git);
    observed::assert_http_zero(&fixture);
}
#[test]
fn timeline_actor_v1_legacy_keyboard_observed_author_generator_date_unknown_and_pure_back() {
    journey(false)
}
#[test]
fn timeline_actor_v1_legacy_mouse_observed_author_generator_date_unknown_and_pure_back() {
    journey(true)
}
