//! SOURCE ONLY: unregistered real-binary journeys. No direct handler calls.
use super::*;
use ghtui::{
    actions_write_journal::{
        ActionsImmutableContext, ActionsSafeOptions, ActionsWriteIntent, ActionsWriteJournal,
        ActionsWriteKind, ActionsWriteTarget,
    },
    providers::gh::ActionsTarget,
    storage::Scope,
};
use std::{fs, path::Path};

#[test]
fn actions_run_view_keyboard_optional_attempt_open_refresh_logs_retains_pending_original() {
    journey(false);
}
#[test]
fn actions_run_view_mouse_optional_attempt_open_refresh_logs_retains_pending_original() {
    journey(true);
}
fn original_rows(database: &Path) -> Vec<(String, i64, Option<String>, String)> {
    let connection =
        rusqlite::Connection::open_with_flags(database, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let mut statement = connection.prepare("SELECT state,claimed,receipt_json,context_json FROM actions_write_attempts ORDER BY attempt_id").unwrap();
    let rows = statement
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    rows
}
fn generations(database: &Path) -> Vec<(String, String, String, i64)> {
    let connection =
        rusqlite::Connection::open_with_flags(database, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let mut statement = connection.prepare("SELECT host,principal,resource_key,generation FROM actions_write_generations ORDER BY host,principal,resource_key").unwrap();
    let rows = statement
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    rows
}
fn omitted_job(name: &str) -> Value {
    json!({"id":501,"run_id":123,"head_sha":"a".repeat(40),"name":name,"status":"completed","conclusion":"failure","runner_name":"hosted","labels":["ubuntu-latest"],"check_run_url":"https://github.invalid/api/v3/repos/example/repo/check-runs/600","html_url":"https://foreign.invalid/actions/runs/999/attempts/1","steps":[{"number":3,"name":"Run tests","status":"completed","conclusion":"failure"}]})
}
fn open_missing(name: &str) -> Vec<GhStep> {
    publication_support::open_attempt(&raw_run(), vec![omitted_job(name)])
}
fn journey(mouse: bool) {
    let fixture = fixture();
    fs::write(
        fixture.repo().join("unchanged.txt"),
        b"dirty original retained\n",
    )
    .unwrap();
    fs::write(
        fixture.repo().join("staged.txt"),
        b"staged original retained\n",
    )
    .unwrap();
    fixture.git(&["add", "staged.txt"]);
    fs::write(fixture.repo().join("untracked.txt"), "untracked α\n").unwrap();
    let original_head = fixture.git(&["rev-parse", "HEAD"]);
    let original_status = fixture.git(&["status", "--porcelain"]);
    let original_index = fixture.git(&["ls-files", "--stage"]);
    let database = fixture.root.join("config/ghtui/cache.sqlite3");
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    let target = ActionsWriteTarget::from_scope(
        &scope,
        &ActionsTarget::Run {
            repository: "example/repo".into(),
            id: 123,
        },
    )
    .unwrap();
    let intent = ActionsWriteIntent::new(
        ActionsWriteKind::Cancel,
        ActionsImmutableContext {
            run_id: Some(123),
            attempt: Some(2),
            head_sha: Some("a".repeat(40)),
            ..Default::default()
        },
        ActionsSafeOptions::default(),
        b"",
    )
    .unwrap();
    let mut publication = ActionsWriteJournal::open(&database).unwrap();
    let pending = publication.begin(&target, &intent).unwrap();
    assert_eq!(pending.marker().attempt_id, 1);
    drop(publication);
    let before_rows = original_rows(&database);
    assert_eq!(before_rows[0].0, "pending");
    assert_eq!(before_rows[0].1, 0);
    assert!(before_rows[0].2.is_none());
    let before_generations = generations(&database);
    let mut steps = initial(); // Existing Actions startup identity2 + catalogue1.
    steps.extend(open_missing("initial optional attempt two")); // Exact two reads.
    steps.extend(current_account()); // Existing explicit Refresh principal2.
    steps.extend(open_missing("fresh optional attempt two")); // Same exact two.
    steps.push(GhStep::read(
        &[
            "run",
            "view",
            "123",
            "--repo",
            "github.invalid/example/repo",
            "--attempt",
            "2",
            "--log",
            "--job",
            "501",
        ],
        "actual attempt two job log\nline two\nretained final line\n",
    ));
    assert_eq!(steps.len(), 10);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "Actions", b"4");
    session.wait_for_screen("example/repo run 123 attempt 2");
    if mouse {
        session.click_visible_action("example/repo run 123");
    } else {
        session.send_key(b"\r");
    }
    session.wait_for_screen("initial optional attempt two");
    session.wait_for_screen("Attempt source: verified exact endpoint");
    session.wait_for_screen("@alice");
    journal(&mut session, &fixture, 5);
    choose(&mut session, mouse, "[r Refresh]", b"r");
    session.wait_for_screen("fresh optional attempt two");
    journal(&mut session, &fixture, 9);
    session.wait_for_screen("Attempt source: verified exact endpoint");
    session.resize(80, 24);
    session.wait_for_screen("Attempt source: verified exact endpoint");
    command(&mut session, mouse, "Rerun all jobs");
    session.wait_for_screen("Action unavailable");
    session.wait_for_screen("Original attempt #1");
    assert!(!session.screen().contains("ACTIONS INTENT"));
    journal(&mut session, &fixture, 9);
    if mouse {
        session.click_visible_action("[Esc Close]");
    } else {
        session.send_key(b"\x1b");
    }
    session
        .try_wait_for_condition(
            |screen| {
                !screen.contains("Action unavailable")
                    && screen.contains("Attempt source: verified exact endpoint")
            },
            "exact original run frame after disabled notice",
            std::time::Duration::from_secs(8),
        )
        .unwrap();
    session.resize(120, 40);
    session.wait_for_screen("fresh optional attempt two");
    if mouse {
        session.click_visible_action("Job 501");
    } else {
        session.send_key(b"\x1b[H");
        for _ in 0..5 {
            session.send_key(b"\x1b[B");
        }
        session.send_key(b"\r");
    }
    session.wait_for_screen("Step 3");
    choose(&mut session, mouse, "[l Logs]", b"l");
    session.wait_for_screen("actual attempt two job log");
    let rows = journal(&mut session, &fixture, 10);
    assert_eq!(
        rows[9]["args"],
        json!([
            "run",
            "view",
            "123",
            "--repo",
            "github.invalid/example/repo",
            "--attempt",
            "2",
            "--log",
            "--job",
            "501"
        ])
    );
    assert!(rows.iter().all(|row| row["args"][0] == "--version"
        || row["args"][0] == "run"
        || row["args"][5] == "GET"));
    assert!(rows.iter().all(|row| row["stdin_hex"] == ""));
    assert_eq!(original_rows(&database), before_rows);
    assert_eq!(generations(&database), before_generations);
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), original_head);
    assert_eq!(fixture.git(&["status", "--porcelain"]), original_status);
    assert_eq!(fixture.git(&["ls-files", "--stage"]), original_index);
    assert_eq!(
        fs::read(fixture.repo().join("unchanged.txt")).unwrap(),
        b"dirty original retained\n"
    );
    assert_eq!(
        fs::read(fixture.repo().join("staged.txt")).unwrap(),
        b"staged original retained\n"
    );
    assert_eq!(
        fs::read(fixture.repo().join("untracked.txt")).unwrap(),
        "untracked α\n".as_bytes()
    );
    session.quit_and_assert_restored_terminal(mouse);
}
