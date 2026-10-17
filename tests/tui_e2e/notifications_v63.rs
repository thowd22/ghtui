//! Actual-binary notification READ milestone; publication/follow/subscription
//! and protected-write recovery families remain separately REQUIRED.
use super::*;
use crate::support::{
    completed_navigation_controls_v1::{owned_focus_v1, FocusAttributeV1},
    owned_pty_processes_v1::{process_identity, OwnedPtySession, ProcessIdentity},
};
use rusqlite::{types::Value as SqlValue, Connection, OpenFlags};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    time::{Duration, Instant},
};
const TITLE: &str = "Notifications • observed";
const PRIVATE: &str = "ACTUAL_PRIVATE_NOTIF_TITLE";
const HOST: &str = "github.com";
const REPO: &str = "example/notifications";
const CONTROLS: &[&str] = &[
    "[b Back / Cancel]",
    "[q Quit]",
    "[h History]",
    "[Enter Details]",
    "[r Refresh]",
    "[a Include read]",
    "[p Participating]",
];
fn get(endpoint: &str, value: Value) -> GhStep {
    GhStep::read(
        &["api", endpoint, "--hostname", HOST, "--method", "GET"],
        &value.to_string(),
    )
}
fn principal(uid: u64) -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.99.0 (2026-10-01)\n"),
        get("user", json!({"id":uid,"login":"octocat"})),
    ]
}
fn repository() -> Value {
    json!({"id":91000,"node_id":"ACTUAL_NOTIFICATION_REPOSITORY_NODE","full_name":REPO,
    "owner":{"id":71,"node_id":"ACTUAL_OWNER_71","login":"example","type":"Organization"},"description":"Actual notification repository","private":true,
    "fork":false,"archived":false,"default_branch":"main","language":"Rust","permissions":{"pull":true,"push":false}})
}
fn row(id: u64) -> Value {
    let mut repo = repository();
    repo["url"] = json!(format!("https://api.github.com/repos/{REPO}"));
    json!({"id":id.to_string(),"unread":true,"reason":"review_requested","updated_at":"2026-10-15T18:00:00Z","last_read_at":null,
        "url":format!("https://api.github.com/notifications/threads/{id}"),"subscription_url":format!("https://api.github.com/notifications/threads/{id}/subscription"),
        "repository":repo,"subject":{"title":format!("{PRIVATE} 界"),"type":"PullRequest","url":format!("https://api.github.com/repos/{REPO}/pulls/7"),"latest_comment_url":format!("https://api.github.com/repos/{REPO}/issues/comments/99")}})
}
fn page(all: bool, participating: bool, number: usize, rows: Value, conditional: bool) -> GhStep {
    let endpoint =
        format!("notifications?all={all}&participating={participating}&per_page=50&page={number}");
    let mut args = vec![
        "api",
        endpoint.as_str(),
        "--hostname",
        HOST,
        "--method",
        "GET",
        "--include",
    ];
    if conditional {
        args.extend(["--header", "If-None-Match: \"snap\""]);
    }
    GhStep::read(
        &args,
        &format!("HTTP/2 200 OK\nETag: \"snap\"\r\nX-Poll-Interval: 60\r\n\r\n{rows}"),
    )
}
fn phase(all: bool, participating: bool, conditional: bool, empty: bool) -> Vec<GhStep> {
    let mut steps = principal(42);
    steps.push(page(
        all,
        participating,
        1,
        if empty {
            json!([])
        } else {
            json!([row(9007199254740993)])
        },
        conditional,
    ));
    if !empty {
        for _ in 0..2 {
            steps.push(get(&format!("repos/{REPO}"), repository()));
        }
    }
    steps.extend(principal(42));
    assert_eq!(steps.len(), if empty { 5 } else { 7 });
    steps
}
fn not_modified_phase() -> Vec<GhStep> {
    let mut steps = phase(false, false, true, false);
    steps[2].stdout=b"HTTP/2 304 Not Modified\nETag: \"snap\"\r\nContent-Length: 17\r\nX-Poll-Interval: 120\r\n\r\n".to_vec();
    steps[2].status = 1;
    steps[2].stderr = b"gh: HTTP 304\n".to_vec();
    steps
}
fn database(f: &Fixture) -> std::path::PathBuf {
    f.root.join("config/ghtui/cache.sqlite3")
}
#[derive(PartialEq, Eq, Debug)]
struct Witness {
    git: Vec<Vec<u8>>,
    config: Option<Vec<u8>>,
    sql: BTreeMap<String, Vec<Vec<String>>>,
}
fn sql(f: &Fixture) -> BTreeMap<String, Vec<Vec<String>>> {
    let connection =
        Connection::open_with_flags(database(f), OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let names=connection.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap()
        .query_map([],|r|r.get::<_,String>(0)).unwrap().map(|r|r.unwrap()).collect::<Vec<_>>();
    let mut out = BTreeMap::new();
    for name in names {
        assert!(name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'));
        let mut statement = connection
            .prepare(&format!("SELECT * FROM \"{name}\""))
            .unwrap();
        let columns = statement.column_count();
        let mut rows=statement.query_map([],|r|{let values=(0..columns).map(|i|r.get::<_,SqlValue>(i)).collect::<rusqlite::Result<Vec<_>>>()?;
            if name=="sync_reservations"&&matches!((&values[0],&values[1],&values[2]),(SqlValue::Text(host),SqlValue::Text(account),SqlValue::Text(source)) if host==HOST&&account=="user-id:42"&&source.starts_with("notifications-v63:")){return Ok(None);}
            Ok(Some(values.into_iter().map(|v|format!("{v:?}")).collect::<Vec<_>>()))
        }).unwrap().filter_map(|r|r.unwrap()).collect::<Vec<_>>();
        rows.sort();
        out.insert(name, rows);
    }
    out
}
fn witness(f: &Fixture) -> Witness {
    let mut git = vec![];
    for args in [
        &["rev-parse", "HEAD"][..],
        &["status", "--porcelain=v2", "-z", "--untracked-files=all"][..],
        &["diff", "--binary", "--no-ext-diff"][..],
        &["diff", "--cached", "--binary", "--no-ext-diff"][..],
    ] {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(f.repo())
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .output()
            .unwrap();
        assert!(output.status.success());
        git.push(output.stdout);
    }
    for path in [
        ".git/index",
        ".git/config",
        "dirty界.txt",
        "staged.txt",
        "scratch.txt",
    ] {
        git.push(fs::read(f.repo().join(path)).unwrap());
    }
    Witness {
        git,
        config: fs::read(f.root.join("config/ghtui/config.toml")).ok(),
        sql: sql(f),
    }
}
fn assert_sql_generation(f: &Fixture, expected: &[u64]) {
    use sha2::{Digest, Sha256};
    let connection =
        Connection::open_with_flags(database(f), OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let mut values=connection.prepare("SELECT source,generation,active FROM sync_reservations WHERE host='github.com' AND login='user-id:42' AND source LIKE 'notifications-v63:%' ORDER BY source").unwrap()
        .query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,u64>(1)?,r.get::<_,bool>(2)?))).unwrap().map(|r|r.unwrap()).collect::<Vec<_>>();
    values.sort();
    let queries = if expected.len() == 3 {
        vec![(false, false), (true, false), (true, true)]
    } else {
        vec![(false, false)]
    };
    assert_eq!(queries.len(), expected.len());
    let mut wanted = queries
        .into_iter()
        .zip(expected)
        .map(|((all, participating), generation)| {
            let mut hash = Sha256::new();
            let all = all.to_string();
            let participating = participating.to_string();
            for value in [
                "notifications-v63",
                HOST,
                "42",
                all.as_str(),
                participating.as_str(),
                "",
                "",
            ] {
                hash.update((value.len() as u64).to_be_bytes());
                hash.update(value.as_bytes());
            }
            (
                format!("notifications-v63:{:x}", hash.finalize()),
                *generation,
                true,
            )
        })
        .collect::<Vec<_>>();
    wanted.sort();
    assert_eq!(values, wanted);
}
fn fixture(steps: &[GhStep]) -> Fixture {
    let f = Fixture::new();
    f.init_git();
    f.commit_file("base.txt", b"BASE\n", "actual original commit");
    f.commit_file(
        "dirty界.txt",
        b"ORIGINAL_TRACKED_DIRTY\n",
        "actual tracked working file",
    );
    fs::write(f.repo().join("dirty界.txt"), b"UNCHANGED_WORKTREE\n").unwrap();
    fs::write(f.repo().join("staged.txt"), b"UNCHANGED_INDEX\n").unwrap();
    f.git(&["add", "--", "staged.txt"]);
    fs::write(f.repo().join("scratch.txt"), b"UNCHANGED_UNTRACKED\n").unwrap();
    let cache = ghtui::storage::Cache::open(&database(&f)).unwrap();
    let foreign = ghtui::storage::Scope::github(HOST, Some(99)).unwrap();
    cache
        .put_draft(&foreign, "foreign", "EXACT_FOREIGN_DRAFT")
        .unwrap();
    drop(cache);
    f.scripted_gh(steps);
    f
}
fn ready(session: &mut Session, after: usize, needle: &str) -> String {
    let end = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        if session.completed_cells_end() > after {
            // Freshness and owned body must come from the SAME completed
            // cells. The live parser may already be partway through a newer
            // draw, whose title/row can precede clearing the old Files tail.
            if let Some(body) = session
                .completed_cells_screen()
                .and_then(|frame| crate::support::overlay_body(frame, TITLE))
            {
                if body.contains(needle) && !body.contains("Verifying current account") {
                    return body;
                }
            }
        }
        assert!(
            Instant::now() < end,
            "No NEW completed owned notification frame containing {needle}\n{}",
            session.screen()
        );
    }
}
fn choose(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    choose_with_controls(session, mouse, label, key, CONTROLS);
}
fn choose_with_controls(
    session: &mut Session,
    mouse: bool,
    label: &str,
    key: &[u8],
    controls: &[&str],
) {
    if mouse {
        let end = Instant::now() + Duration::from_secs(8);
        loop {
            session.poll_output(Duration::from_millis(1));
            if let Some(point) = session
                .completed_cells_screen()
                .and_then(|frame| {
                    owned_focus_v1(
                        frame,
                        TITLE,
                        label,
                        FocusAttributeV1::Inverse,
                        controls,
                        &[],
                    )
                })
                .map(|focus| focus.target)
            {
                session.click(point.0, point.1);
                return;
            }
            assert!(
                Instant::now() < end,
                "No complete topmost current notification control {label}"
            );
        }
    } else if key == b"\r" {
        session.focus_completed_owned_action_v1(
            TITLE,
            label,
            FocusAttributeV1::Inverse,
            controls,
            &[],
        );
        session.send_key(b"\r");
    } else {
        session.send_key(key);
    }
}
fn open(session: &mut Session, mouse: bool) -> usize {
    let after = session.completed_cells_end();
    if mouse {
        let point = crate::support::locate_screen(
            session
                .completed_cells_screen()
                .expect("completed original Inbox"),
            "[F8 Notifications]",
        )
        .expect("actual completed Inbox notification control");
        session.click(point.0, point.1);
    } else {
        session.send_key(b"\x1b[19~");
    }
    after
}
fn exact(session: &Session, f: &Fixture, steps: &[GhStep]) {
    let rows = session
        .inspect_command_journal(f)
        .lines()
        .map(|s| serde_json::from_str::<Value>(s).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), steps.len());
    for (index, (row, expected)) in rows.iter().zip(steps).enumerate() {
        assert_eq!(row["step"], index);
        assert_eq!(row["matched"], true);
        assert_eq!(row["args"], json!(expected.args));
        assert_eq!(row["stdin_hex"], "");
        assert!(!expected.args.iter().any(|arg| matches!(
            arg.as_str(),
            "POST" | "PATCH" | "PUT" | "DELETE" | "--paginate" | "--slurp"
        )));
    }
}
fn start(f: &Fixture) -> Session {
    let mut session = Session::launch(f, 80, 24);
    session.wait_for_screen("Recent commits");
    session.assert_focused_view("Repository");
    session.send_key(b"1");
    session.wait_for_cursorless_condition(
        |frame| {
            let text = frame.contents();
            text.contains("Inbox • focused view")
                && text.contains("Use Refresh to load your authenticated GitHub inbox.")
                && text.contains("[r Refresh]")
        },
        "Actual completed idle Inbox before explicit startup Refresh",
        Duration::from_secs(8),
    );
    session.send_key(b"r");
    session.wait_for_command_count(f, 7);
    session.wait_for_cursorless_condition(
        |frame| {
            let text = frame.contents();
            text.contains("Fresh • fetched")
                && text.contains("No pull requests in this queue")
                && text.contains("[F8 Notifications]")
        },
        "Actual completed fresh original Inbox before notification input",
        Duration::from_secs(8),
    );
    session.wait_for_main_frame();
    session
}
fn quit(session: &mut Session, mouse: bool) {
    choose(session, mouse, "[q Quit]", b"q");
    session.assert_exit_and_restored(true);
}
fn main_back(session: &mut Session, mouse: bool) {
    let after = session.completed_cells_end();
    choose(session, mouse, "[b Back / Cancel]", b"b");
    session.wait_for_new_cursorless_condition(
        after,
        |frame| {
            let text = frame.contents();
            text.contains("[F8 Notifications]")
                && text.contains("No pull requests in this queue")
                && !text.contains(TITLE)
                && !text.contains(PRIVATE)
        },
        "Original completed Inbox restored and private notification rows erased",
        Duration::from_secs(8),
    );
}
fn read_details(mouse: bool) {
    let mut steps = empty_inbox_steps("octocat");
    steps.extend(phase(false, false, false, false));
    steps.extend(not_modified_phase());
    steps.extend(not_modified_phase());
    let f = fixture(&steps);
    let mut session = start(&f);
    let before = witness(&f);
    let after = open(&mut session, mouse);
    ready(&mut session, after, PRIVATE);
    session.capture_completed_terminal_v1("notifications-observed-80");
    session.resize_owned_header(120, 40, TITLE, CONTROLS);
    ready(&mut session, 0, PRIVATE);
    let after = session.completed_cells_end();
    choose(&mut session, mouse, "[Enter Details]", b"\r");
    ready(&mut session, after, "Acting user unavailable");
    session.capture_completed_terminal_v1("notifications-detail-120");
    let narrow_after = session.completed_cursorless_end();
    session.resize_owned_header(40, 18, TITLE, &CONTROLS[..2]);
    session.wait_for_new_cursorless_condition(
        narrow_after,
        |frame| {
            frame.size() == (18, 40)
                && crate::support::owned_resize_controls_ready_v1(
                    frame,
                    TITLE,
                    &CONTROLS[..2],
                    crate::support::ResizeToolbarV1::Header,
                )
                && crate::support::overlay_body(frame, TITLE).is_some_and(|body| {
                    CONTROLS.iter().all(|control| body.contains(control))
                        && body.contains("Widen to at least66")
                        && !body.contains("Verifying current account")
                })
        },
        "Same NEW completed40x18 notification detail and all original controls",
        Duration::from_secs(8),
    );
    let narrow = session
        .completed_cursorless_screen()
        .expect("Actual completed narrow notification detail frame");
    assert_eq!(narrow.size(), (18, 40));
    assert!(crate::support::owned_resize_controls_ready_v1(
        narrow,
        TITLE,
        &CONTROLS[..2],
        crate::support::ResizeToolbarV1::Header,
    ));
    let body = crate::support::overlay_body(narrow, TITLE)
        .expect("Same completed narrow notification title and full owned body");
    for control in CONTROLS {
        assert!(body.contains(control), "Missing narrow control: {control}");
    }
    assert!(body.contains("Widen to at least66"));
    assert!(!body.contains("Verifying current account"));
    session.resize_owned_header(120, 40, TITLE, CONTROLS);
    ready(&mut session, 0, "Acting user unavailable");
    choose(&mut session, mouse, "[b Back / Cancel]", b"b");
    ready(&mut session, 0, PRIVATE);
    let after = session.completed_cells_end();
    choose(&mut session, mouse, "[h History]", b"h");
    ready(&mut session, after, "No protected attempt recorded");
    assert!(session
        .visible_overlay_body(TITLE)
        .unwrap()
        .contains("inspect only"));
    assert_eq!(witness(&f), before);
    assert_sql_generation(&f, &[3]);
    choose(&mut session, mouse, "[b Back / Cancel]", b"b");
    ready(&mut session, 0, PRIVATE);
    main_back(&mut session, mouse);
    exact(&session, &f, &steps);
    assert_eq!(witness(&f), before);
    session.quit_and_assert_restored_terminal(mouse);
}
fn filters(mouse: bool) {
    let mut steps = empty_inbox_steps("octocat");
    steps.extend(phase(false, false, false, false));
    steps.extend(phase(true, false, false, false));
    steps.extend(phase(true, true, false, false));
    let f = fixture(&steps);
    let mut session = start(&f);
    let before = witness(&f);
    let after = open(&mut session, mouse);
    ready(&mut session, after, "Unread only");
    let after = session.completed_cells_end();
    choose(&mut session, mouse, "[a Include read]", b"a");
    ready(&mut session, after, "Including read");
    let after = session.completed_cells_end();
    choose(&mut session, mouse, "[p Participating]", b"p");
    ready(&mut session, after, "Participating only");
    assert_eq!(witness(&f), before);
    assert_sql_generation(&f, &[1, 1, 1]);
    exact(&session, &f, &steps);
    quit(&mut session, mouse);
}
fn empty_disabled(mouse: bool) {
    let mut steps = empty_inbox_steps("octocat");
    steps.extend(phase(false, false, false, true));
    let f = fixture(&steps);
    let mut session = start(&f);
    let before = witness(&f);
    let after = open(&mut session, mouse);
    ready(&mut session, after, "No notifications observed");
    let count = session.inspect_command_journal(&f).lines().count();
    choose(&mut session, mouse, "[Enter Details]", b"\r");
    session.poll_output(Duration::from_millis(50));
    assert_eq!(session.inspect_command_journal(&f).lines().count(), count);
    assert!(!session.screen().contains(PRIVATE));
    assert_eq!(witness(&f), before);
    assert_sql_generation(&f, &[1]);
    exact(&session, &f, &steps);
    quit(&mut session, mouse);
}
fn privacy(mouse: bool) {
    for changed in ["account", "repository"] {
        let mut steps = empty_inbox_steps("octocat");
        steps.extend(phase(false, false, false, false));
        if changed == "account" {
            steps.extend(principal(43));
        } else {
            let mut tail = not_modified_phase();
            let mut recreated = repository();
            recreated["id"] = json!(91001);
            recreated["node_id"] = json!("RECREATED_SAME_NAME");
            tail[3] = get(&format!("repos/{REPO}"), recreated);
            tail.truncate(4);
            steps.extend(tail);
        }
        let f = fixture(&steps);
        let mut session = start(&f);
        let before = witness(&f);
        let after = open(&mut session, mouse);
        ready(&mut session, after, PRIVATE);
        let after = session.completed_cells_end();
        choose(&mut session, mouse, "[r Refresh]", b"r");
        session.wait_for_new_cursorless_condition(after,|frame|{let text=frame.contents();!text.contains(PRIVATE)&&!text.contains(TITLE)&&text.contains("Private presentation retired")},"Exact original private notification and Inbox retired after actual account/repository loss",Duration::from_secs(8));
        exact(&session, &f, &steps);
        assert_eq!(witness(&f), before);
        assert_sql_generation(&f, if changed == "account" { &[1] } else { &[2] });
        session.quit_and_assert_restored_terminal(mouse);
    }
}
fn child_gone(session: &mut Session, original: &ProcessIdentity) {
    let end = Instant::now() + Duration::from_secs(8);
    loop {
        match process_identity(original.pid) {
            Ok(None) => return,
            Ok(Some(now))
                if (now.birth, now.uid, now.session)
                    != (original.birth, original.uid, original.session) =>
            {
                return
            }
            Ok(Some(_)) => {}
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {}
            Err(e) => panic!("Actual original child identity unavailable: {e}"),
        };
        assert!(Instant::now() < end, "Original exact held child not reaped");
        session.poll_output(Duration::from_millis(1));
    }
}
fn cancel(mouse: bool) {
    let mut steps = empty_inbox_steps("octocat");
    let mut held = phase(false, false, false, false);
    held[2].delay_ms = 6000;
    held.truncate(3);
    steps.extend(held);
    steps.extend(phase(false, false, false, false));
    let f = fixture(&steps);
    let mut session = start(&f);
    let before = witness(&f);
    let after = open(&mut session, mouse);
    session.wait_for_command_count(&f, 10);
    let rows = session
        .inspect_command_journal(&f)
        .lines()
        .map(|s| serde_json::from_str::<Value>(s).unwrap())
        .collect::<Vec<_>>();
    let pid = i32::try_from(rows[9]["pid"].as_u64().unwrap()).unwrap();
    let child = process_identity(pid)
        .unwrap()
        .expect("actual six-second held child live BEFORE Cancel");
    let application_pid = u32::try_from(session.owned_application_pid())
        .expect("owned application PID must be positive and fit u32");
    let owned = OwnedPtySession::capture(application_pid).unwrap();
    assert!(owned.owns(&child));
    assert!(!child.zombie);
    session.wait_for_new_cursorless_condition(
        after,
        |frame| frame.contents().contains("Verifying current account"),
        "Actual pending notification controls",
        Duration::from_secs(8),
    );
    main_back(&mut session, mouse);
    child_gone(&mut session, &child);
    assert_eq!(witness(&f), before);
    let after = open(&mut session, mouse);
    ready(&mut session, after, PRIVATE);
    assert_eq!(witness(&f), before);
    assert_sql_generation(&f, &[2]);
    exact(&session, &f, &steps);
    quit(&mut session, mouse);
}
fn partial(mouse: bool) {
    let mut steps = empty_inbox_steps("octocat");
    steps.extend(principal(42));
    for number in 1..=4 {
        steps.push(page(
            false,
            false,
            number,
            json!((0..50)
                .map(|i| row(100000 + number as u64 * 50 + i))
                .collect::<Vec<_>>()),
            false,
        ));
    }
    for _ in 0..2 {
        steps.push(get(&format!("repos/{REPO}"), repository()));
    }
    steps.extend(principal(42));
    let f = fixture(&steps);
    let mut session = start(&f);
    let before = witness(&f);
    let after = open(&mut session, mouse);
    ready(&mut session, after, "Partial query; absence unknown");
    session.resize_owned_header(120, 40, TITLE, CONTROLS);
    assert!(session
        .visible_overlay_body(TITLE)
        .unwrap()
        .contains("200 observed"));
    assert_sql_generation(&f, &[1]);
    exact(&session, &f, &steps);
    assert_eq!(witness(&f), before);
    quit(&mut session, mouse);
}
#[test]
fn notifications_v63_keyboard_read_details_history304_resize_and_pure_back() {
    read_details(false)
}
#[test]
fn notifications_v63_mouse_read_details_history304_resize_and_pure_back() {
    read_details(true)
}
#[test]
fn notifications_v63_keyboard_filters_exact_query_no_cross_query304() {
    filters(false)
}
#[test]
fn notifications_v63_mouse_filters_exact_query_no_cross_query304() {
    filters(true)
}
#[test]
fn notifications_v63_keyboard_complete_empty_disabled_details_zero_effects() {
    empty_disabled(false)
}
#[test]
fn notifications_v63_mouse_complete_empty_disabled_details_zero_effects() {
    empty_disabled(true)
}
#[test]
fn notifications_v63_keyboard_actual_account_recreated_repository_hide_old_private() {
    privacy(false)
}
#[test]
fn notifications_v63_mouse_actual_account_recreated_repository_hide_old_private() {
    privacy(true)
}
#[test]
fn notifications_v63_keyboard_owned_held_page_cancel_then_explicit_fresh_read() {
    cancel(false)
}
#[test]
fn notifications_v63_mouse_owned_held_page_cancel_then_explicit_fresh_read() {
    cancel(true)
}
#[test]
fn notifications_v63_keyboard_four_bounded_pages_honestly_partial() {
    partial(false)
}
#[test]
fn notifications_v63_mouse_four_bounded_pages_honestly_partial() {
    partial(true)
}
#[path = "notifications_pr_follow_v63.rs"]
mod pr_follow_v63;
#[path = "notifications_publication_v63.rs"]
mod publication_v63;
