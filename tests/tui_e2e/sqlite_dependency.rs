//! Real binary WAL cache/composer/restart with independent committed SQL,
//! numeric principal, strict gh, dirty Git/config and terminal effects.
use super::*;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    time::{Duration, Instant},
};

const RESOURCE: &str = "org/alpha/pull/7/composer/comment";
const FIRST: &str = "SQLite WAL draft café\nExact Unicode 👩🏽‍💻 and 界";
const SUFFIX: &str = "\nRestart retained exact committed bytes";

fn completed(session: &mut Session, expected: &[&str], absent: &[&str]) {
    session.wait_for_cursorless_condition(
        |frame| {
            let text = frame.contents();
            expected.iter().all(|value| text.contains(value))
                && absent.iter().all(|value| !text.contains(value))
        },
        "complete SQLite journey view/privacy frame",
        Duration::from_secs(8),
    );
}
fn draft(cache: &Cache, session: &mut Session, expected: &str) -> Value {
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        if let Some(raw) = cache.draft(&scope, RESOURCE).unwrap() {
            if let Ok(value) = serde_json::from_str::<Value>(&raw) {
                if value["values"]["body"].as_str() == Some(expected) {
                    assert_eq!(value["version"], 2);
                    assert!(
                        value["publication"].is_null(),
                        "Draft checkpoint is not a mutation receipt"
                    );
                    return value;
                }
            }
        }
        assert!(
            Instant::now() < until,
            "Actual DraftWriter must commit exact body bytes, not only paint saved status"
        );
        session.poll_output(Duration::from_millis(10));
    }
}
fn checkpoint(connection: &rusqlite::Connection, mode: &str) {
    let result: (i64, i64, i64) = connection
        .query_row(&format!("PRAGMA wal_checkpoint({mode})"), [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .unwrap();
    assert_eq!(
        result.0, 0,
        "Actual SQLite checkpoint must complete with no hidden busy retry"
    );
    if mode == "TRUNCATE" {
        assert_eq!(result, (0, 0, 0));
    }
    let source: String = connection
        .query_row("SELECT sqlite_source_id()", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        source,
        "2026-03-13 10:38:09 737ae4a34738ffa0c3ff7f9bb18df914dd1cad163f28fd6b6e114a344fe6d618"
    );
}
fn journal_exact(session: &mut Session, fixture: &Fixture, count: usize) {
    readonly_journal(session, fixture, count);
    let rows: Vec<Value> = session
        .inspect_command_journal(fixture)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(rows.iter().all(|row| row["stdin_hex"] == ""));
    assert!(rows.iter().all(
        |row| row["args"].as_array().unwrap().iter().all(|arg| !matches!(
            arg.as_str(),
            Some("graphql" | "POST" | "PUT" | "PATCH" | "DELETE")
        ))
    ));
}
fn journey(mouse: bool) {
    let fixture = configured();
    fixture.init_git();
    fixture.commit_file(
        "preserved.txt",
        b"saved checkout bytes\n",
        "SQLite fixture saved checkout",
    );
    fs::write(
        fixture.repo().join("preserved.txt"),
        b"dirty untouched checkout bytes\n",
    )
    .unwrap();
    fs::write(
        fixture.repo().join("staged.txt"),
        b"staged untouched bytes\n",
    )
    .unwrap();
    fixture.git(&["add", "--", "staged.txt"]);
    fs::write(
        fixture.repo().join("scratch.txt"),
        b"untracked untouched bytes\n",
    )
    .unwrap();
    let before_git = (
        fixture.git(&["rev-parse", "HEAD"]),
        fixture.git(&["show-ref", "--head"]),
        fs::read(fixture.repo().join(".git/index")).unwrap(),
        fixture.git(&["status", "--porcelain=v2", "-z"]),
    );
    let config_path = fixture.root.join("config/ghtui/config.toml");
    let before_config = fs::read(&config_path).unwrap();
    let path = fixture.root.join("config/ghtui/cache.sqlite3");
    let cache = Cache::open(&path).unwrap();
    let original = Scope::github("github.invalid", Some(42)).unwrap();
    let other = Scope::github("github.invalid", Some(43)).unwrap();
    cache
        .put_draft(&other, RESOURCE, "OTHER_NUMERIC_PRINCIPAL_DRAFT")
        .unwrap();
    let keeper = rusqlite::Connection::open(&path).unwrap();
    let mode: String = keeper
        .query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        mode, "wal",
        "Explicit legacy WAL fixture; production journal defaults are unchanged"
    );
    keeper.execute_batch("PRAGMA wal_autocheckpoint=0").unwrap();
    checkpoint(&keeper, "TRUNCATE");
    let mut steps = initial(false);
    steps.extend(pages().into_iter().take(7));
    steps.extend(initial(false));
    steps.extend(pages().into_iter().take(7));
    for _ in 0..2 {
        let mut principal = identity("octocat");
        principal[1].stdout = serde_json::to_vec(&json!({"id":43,"login":"octocat"})).unwrap();
        steps.extend(principal);
    }
    for queue in ["author", "assignee", "review-requested"] {
        steps.push(search("octocat", queue, vec![], false));
    }
    assert_eq!(steps.len(), 35);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(
        &fixture,
        if mouse { 80 } else { 120 },
        if mouse { 24 } else { 40 },
    );
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "1 Inbox", b"1");
    refresh(&mut session, mouse);
    completed(
        &mut session,
        &["Improve auth", "@octocat"],
        &["OTHER_NUMERIC_PRINCIPAL_DRAFT"],
    );
    choose(&mut session, mouse, "Improve auth", b"\r");
    completed(
        &mut session,
        &["1 comments", "[p PR actions]"],
        &["OTHER_NUMERIC_PRINCIPAL_DRAFT"],
    );
    open_comment(&mut session, mouse);
    session.paste(FIRST);
    session.wait_for_screen("Draft saved locally");
    draft(&cache, &mut session, FIRST);
    checkpoint(&keeper, "PASSIVE");
    for file in [
        path.clone(),
        std::path::PathBuf::from(format!("{}-wal", path.display())),
        std::path::PathBuf::from(format!("{}-shm", path.display())),
    ] {
        assert_eq!(
            fs::metadata(file).unwrap().permissions().mode() & 0o777,
            0o600,
            "Actual owned main and live WAL sidecars remain private"
        );
    }
    session.resize(if mouse { 120 } else { 80 }, if mouse { 40 } else { 24 });
    session.wait_for_screen("LOCAL DRAFT • Submit is the only publishing action");
    session.wait_for_screen("SQLite WAL draft café");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("PREVIEW • Nothing published");
    session.wait_for_screen("Exact Unicode");
    draft(&cache, &mut session, FIRST);
    journal_exact(&mut session, &fixture, 14);
    choose(&mut session, mouse, "[Keep draft / Close]", b"\x1b");
    completed(
        &mut session,
        &["[p PR actions]"],
        &[
            "[Keep draft / Close]",
            "LOCAL DRAFT",
            "OTHER_NUMERIC_PRINCIPAL_DRAFT",
        ],
    );
    checkpoint(&keeper, "TRUNCATE");
    session.quit_and_assert_restored_terminal(mouse);
    let actual: String = keeper
        .query_row(
            "SELECT body FROM drafts WHERE host=?1 AND login=?2 AND resource=?3",
            rusqlite::params![original.host, original.login, RESOURCE],
            |row| row.get(0),
        )
        .unwrap();
    let saved: Value = serde_json::from_str(&actual).unwrap();
    assert_eq!(saved["values"]["body"], FIRST);
    let mut restarted = Session::launch(&fixture, 80, 24);
    restarted.wait_for_screen("ghtui");
    choose(&mut restarted, mouse, "1 Inbox", b"1");
    refresh(&mut restarted, mouse);
    completed(
        &mut restarted,
        &["Fresh • fetched", "Improve auth"],
        &["OTHER_NUMERIC_PRINCIPAL_DRAFT"],
    );
    choose(&mut restarted, mouse, "Improve auth", b"\r");
    completed(&mut restarted, &["1 comments", "[p PR actions]"], &[]);
    open_comment(&mut restarted, mouse);
    restarted.wait_for_screen("SQLite WAL draft café");
    draft(&cache, &mut restarted, FIRST);
    restarted.send_key(b"\x1b[1;5F");
    restarted.paste(SUFFIX);
    let final_body = format!("{FIRST}{SUFFIX}");
    draft(&cache, &mut restarted, &final_body);
    choose(&mut restarted, mouse, "[Keep draft / Close]", b"\x1b");
    completed(
        &mut restarted,
        &["[p PR actions]"],
        &["[Keep draft / Close]", "LOCAL DRAFT"],
    );
    journal_exact(&mut restarted, &fixture, 28);
    checkpoint(&keeper, "TRUNCATE");
    restarted.quit_and_assert_restored_terminal(mouse);
    let original_records = cache.list(&original, None).unwrap();
    assert!(original_records.iter().any(|record| record.kind == "pr"));
    let mut reassigned = Session::launch(&fixture, 120, 40);
    reassigned.wait_for_screen("ghtui");
    choose(&mut reassigned, mouse, "1 Inbox", b"1");
    refresh(&mut reassigned, mouse);
    completed(
        &mut reassigned,
        &["Fresh • fetched", "No pull requests", "@octocat"],
        &[
            "Improve auth",
            "SQLite WAL draft café",
            "OTHER_NUMERIC_PRINCIPAL_DRAFT",
        ],
    );
    journal_exact(&mut reassigned, &fixture, 35);
    assert!(cache.list(&other, Some("pr")).unwrap().is_empty());
    assert_eq!(
        cache.list(&original, None).unwrap(),
        original_records,
        "Reassigned same username cannot clear or adopt another numeric principal's records"
    );
    let saved: Value =
        serde_json::from_str(&cache.draft(&original, RESOURCE).unwrap().unwrap()).unwrap();
    assert_eq!(saved["values"]["body"], final_body);
    assert!(saved["publication"].is_null());
    assert_eq!(
        cache.draft(&other, RESOURCE).unwrap().as_deref(),
        Some("OTHER_NUMERIC_PRINCIPAL_DRAFT")
    );
    checkpoint(&keeper, "TRUNCATE");
    assert_eq!(
        keeper
            .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    reassigned.quit_and_assert_restored_terminal(mouse);
    assert_eq!(fs::read(config_path).unwrap(), before_config);
    assert_eq!(
        (
            fixture.git(&["rev-parse", "HEAD"]),
            fixture.git(&["show-ref", "--head"]),
            fs::read(fixture.repo().join(".git/index")).unwrap(),
            fixture.git(&["status", "--porcelain=v2", "-z"])
        ),
        before_git
    );
    assert_eq!(
        fs::read(fixture.repo().join("preserved.txt")).unwrap(),
        b"dirty untouched checkout bytes\n"
    );
    assert_eq!(
        fs::read(fixture.repo().join("staged.txt")).unwrap(),
        b"staged untouched bytes\n"
    );
    assert_eq!(
        fs::read(fixture.repo().join("scratch.txt")).unwrap(),
        b"untracked untouched bytes\n"
    );
}
#[test]
fn sqlite_dependency_keyboard_real_wal_refresh_composer_checkpoint_restart_and_account_isolation() {
    journey(false);
}
#[test]
fn sqlite_dependency_mouse_real_wal_refresh_composer_checkpoint_restart_and_account_isolation() {
    journey(true);
}
