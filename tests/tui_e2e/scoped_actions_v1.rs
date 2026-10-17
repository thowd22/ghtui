//! Actual binary, real Git/SQLite, strict offline GH. ALL declarations UNRUN.
use super::*;
#[path = "scoped_actions_control_v5.rs"]
mod control_v5;
#[path = "scoped_actions_dispatch_v7.rs"]
mod dispatch_v7;
#[path = "scoped_actions_publication_v3.rs"]
mod publication_v3;
#[path = "scoped_actions_read_v2.rs"]
mod read_v2;
#[path = "scoped_actions_rerun_v4.rs"]
mod rerun_v4;
#[path = "scoped_actions_resources_v2.rs"]
mod resources_v2;
#[path = "scoped_actions_workflow_edit_v9.rs"]
mod task54_workflow_edit_v9;
#[path = "scoped_actions_workflow_v6.rs"]
mod workflow_v6;
use crate::support::owned_pty_processes_v1::{process_identity, ProcessIdentity};
const H: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
fn fresh_run(name: &str) -> Value {
    let mut value = run(123, "CI", "alice", "User");
    value["node_id"] = json!("WR_123");
    value["check_suite_id"] = json!(901);
    let mut repo = current_repository();
    repo["full_name"] = json!(name);
    value["repository"] = repo;
    value
}
fn fresh_job(name: &str) -> Value {
    json!({"id":501,"node_id":"JOB_501","run_id":123,"head_sha":H,"name":"Actual matrix stable linux",
        "status":"completed","conclusion":"failure",
        "url":format!("https://{HOST}/api/v3/repos/{name}/actions/jobs/501"),
        "check_run_url":format!("https://{HOST}/api/v3/repos/{name}/check-runs/600"),
        "steps":[{"number":2,"name":"Actual failed source step","status":"completed","conclusion":"failure"}]})
}
fn fresh_check(name: &str) -> Value {
    json!({"id":600,"node_id":"CHECK_600","head_sha":H,"check_suite":{"id":901},
        "url":format!("https://{HOST}/api/v3/repos/{name}/check-runs/600")})
}
fn paged(path: &str, value: Value) -> GhStep {
    let mut step = get(path, json!([value]));
    step.args.extend(["--paginate".into(), "--slurp".into()]);
    step
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum ReadKind {
    Run,
    Logs,
    Annotations,
}
fn operation(kind: ReadKind, uid: u64, name: &str) -> Vec<GhStep> {
    let mut current = current_repository();
    current["full_name"] = json!(name);
    let mut steps = principal(42);
    steps.push(get(&format!("repos/{FIRST}"), current.clone()));
    steps.push(get(
        &format!("repos/{name}/actions/runs/123"),
        fresh_run(name),
    ));
    steps.push(get(
        &format!("repos/{name}/actions/runs/123/attempts/2"),
        fresh_run(name),
    ));
    steps.push(paged(
        &format!("repos/{name}/actions/runs/123/attempts/2/jobs?per_page=100"),
        json!({"total_count":1,"jobs":[fresh_job(name)]}),
    ));
    if kind != ReadKind::Run {
        steps.push(get(
            &format!("repos/{name}/actions/jobs/501"),
            fresh_job(name),
        ));
        if kind == ReadKind::Logs {
            steps.push(GhStep::read(
                &[
                    "run",
                    "view",
                    "123",
                    "--repo",
                    &format!("{HOST}/{name}"),
                    "--attempt",
                    "2",
                    "--log",
                    "--job",
                    "501",
                ],
                "PRIVATE_SCOPED_ACTIONS_JOB_501_LOG\nActual failed source step\nthird log line\n",
            ));
        } else {
            steps.push(get(
                &format!("repos/{name}/check-runs/600"),
                fresh_check(name),
            ));
            steps.push(paged(&format!("repos/{name}/check-runs/600/annotations?per_page=100"),
                json!([{"path":"src/界.rs","start_line":7,"end_line":9,"annotation_level":"failure",
                    "title":"Actual reported annotation","message":"PRIVATE_SCOPED_ACTIONS_ANNOTATION","raw_details":"reported metadata, not Git coordinates"}])));
            steps.push(get(
                &format!("repos/{name}/check-runs/600"),
                fresh_check(name),
            ));
        }
        steps.push(get(
            &format!("repos/{name}/actions/jobs/501"),
            fresh_job(name),
        ));
    }
    steps.push(get(
        &format!("repos/{name}/actions/runs/123"),
        fresh_run(name),
    ));
    steps.push(get(&format!("repos/{FIRST}"), current));
    steps.extend(principal(uid));
    assert_eq!(
        steps.len(),
        match kind {
            ReadKind::Run => 10,
            ReadKind::Logs => 13,
            ReadKind::Annotations => 15,
        }
    );
    steps
}
fn exact_native(session: &Session, fixture: &Fixture, steps: &[GhStep], consumed: usize) {
    let rows = commands(session, fixture);
    assert_eq!(
        rows.len(),
        consumed,
        "No hidden refresh, default/name fallback or resend"
    );
    for (actual, expected) in rows.iter().zip(&steps[..consumed]) {
        assert_eq!(actual["args"], json!(expected.args));
        assert_eq!(actual["matched"], true);
        assert_eq!(actual["stdin_hex"], "");
        assert!(!expected.args.iter().any(|arg| matches!(
            arg.as_str(),
            "POST" | "PATCH" | "PUT" | "DELETE" | "graphql"
        )));
    }
}
fn sql(fixture: &Fixture) -> Vec<(String, Vec<Vec<rusqlite::types::Value>>)> {
    let db = rusqlite::Connection::open_with_flags(
        database(fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let names=db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap()
        .query_map([],|r|r.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>();
    names
        .into_iter()
        .map(|name| {
            let quoted = name.replace('"', "\"\"");
            let mut statement = db
                .prepare(&format!("SELECT * FROM \"{quoted}\" ORDER BY rowid"))
                .unwrap();
            let count = statement.column_count();
            let rows = statement
                .query_map([], |row| {
                    (0..count)
                        .map(|index| row.get(index))
                        .collect::<rusqlite::Result<Vec<rusqlite::types::Value>>>()
                })
                .unwrap()
                .map(Result::unwrap)
                .collect();
            (name, rows)
        })
        .collect()
}
fn complete(session: &mut Session, after: usize, required: &[&str]) {
    session.wait_for_new_cursorless_condition(
        after,
        |screen| {
            let text = screen.contents();
            required.iter().all(|value| text.contains(value))
        },
        &format!("Original completed Actions frame {required:?}"),
        Duration::from_secs(8),
    );
}
fn start(fixture: &Fixture, mouse: bool) -> Session {
    let mut session = launch(fixture, mouse);
    let tab = session.locate("4 Actions").unwrap();
    open_catalogue(&mut session, fixture, mouse, 0);
    discover_with_captured_actions_tab(&mut session, fixture, mouse, tab);
    filter(
        &mut session,
        mouse,
        "repo0000",
        FIRST,
        "Repository ID: 10000",
    );
    choose(&mut session, mouse, "[Open Actions]", b"A");
    session.wait_for_command_count(fixture, 14);
    session.wait_for_overlay_close(TITLE);
    actions_ready(&mut session);
    session
}
fn configured_native(steps: &[GhStep]) -> Fixture {
    let fixture = configured(steps);
    Cache::open(&database(&fixture))
        .unwrap()
        .save_view(
            &Scope::github(HOST, Some(42)).unwrap(),
            INTENT_CANARY,
            &original_intent(),
        )
        .unwrap();
    fixture
}
fn open_run(session: &mut Session, fixture: &Fixture, mouse: bool, count: usize, name: &str) {
    let after = session.completed_cursorless_end();
    if mouse {
        session.click_visible_action("run 123 attempt 2");
    } else {
        selected_row(session, false, "run 123 attempt 2");
    }
    session.wait_for_command_count(fixture, count);
    complete(session, after, &["Run 123", "attempt 2", H, name, "alice"]);
}
pub(crate) fn selected_row(session: &mut Session, mouse: bool, label: &str) {
    if mouse {
        session.click_visible_action(label);
        return;
    }
    let before = session.completed_cursorless_end();
    session.send_key(b"\x1b[H");
    session.wait_for_new_cursorless_condition(
        before,
        |s| s.contents().lines().any(|line| line.contains("> ")),
        "Actual Actions Home selected row",
        Duration::from_secs(8),
    );
    for _ in 0..32 {
        let selected = session
            .completed_cursorless_screen()
            .unwrap()
            .contents()
            .lines()
            .find(|line| line.contains("> "))
            .expect("Actual Actions selected marker")
            .to_owned();
        if selected.contains(label) {
            session.send_key(b"\r");
            return;
        }
        let after = session.completed_cursorless_end();
        session.send_key(b"\x1b[B");
        session.wait_for_new_cursorless_condition(
            after,
            |s| {
                s.contents()
                    .lines()
                    .any(|line| line.contains("> ") && line != selected)
            },
            "Actual next Actions selected member",
            Duration::from_secs(8),
        );
    }
    panic!("Independent actual Actions member not reached: {label}");
}
fn jobs(session: &mut Session, mouse: bool) {
    let before = session.completed_cursorless_end();
    choose(session, mouse, "[j Jobs]", b"j");
    let point = session.wait_for_scoped_actions_jobs_row_v2(before, "Job 501", H);
    let before = session.completed_cursorless_end();
    if mouse {
        session.click(point.0, point.1);
    } else {
        selected_row(session, false, "Job 501");
    }
    complete(
        session,
        before,
        &["Step 2", "Actual failed source step", "[N Annotations]"],
    );
}
fn end(
    session: &mut Session,
    fixture: &Fixture,
    mouse: bool,
    steps: &[GhStep],
    before: &GitState,
    config: &[u8],
    sql_before: &[(String, Vec<Vec<rusqlite::types::Value>>)],
) {
    assert_eq!(sql(fixture), sql_before);
    assert_private_intents_unchanged(fixture);
    assert_eq!(git_state(fixture), *before);
    assert_read_only_git(fixture);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    session.quit_and_assert_restored_terminal(mouse);
    exact_native(session, fixture, steps, steps.len());
    assert_eq!(sql(fixture), sql_before);
    assert_eq!(git_state(fixture), *before);
    assert_private_intents_unchanged(fixture);
    assert_read_only_git(fixture);
}
fn happy(mouse: bool) {
    let mut steps = actions_protocol();
    steps.extend(operation(ReadKind::Run, 42, CURRENT));
    steps.extend(operation(ReadKind::Logs, 42, CURRENT));
    steps.extend(operation(ReadKind::Annotations, 42, CURRENT));
    steps.extend(operation(ReadKind::Annotations, 42, CURRENT));
    let fixture = configured_native(&steps);
    let before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    let sql_before = sql(&fixture);
    open_run(&mut session, &fixture, mouse, 24, CURRENT);
    session.capture_completed_terminal_v1("actions-run");
    jobs(&mut session, mouse);
    session.capture_completed_terminal_v1("actions-jobs");
    let mark = session.completed_cursorless_end();
    choose(&mut session, mouse, "[l Logs]", b"l");
    session.wait_for_command_count(&fixture, 37);
    complete(
        &mut session,
        mark,
        &["PRIVATE_SCOPED_ACTIONS_JOB_501_LOG", CURRENT],
    );
    session.capture_completed_terminal_v1("actions-logs");
    let mark = session.completed_cursorless_end();
    choose(&mut session, mouse, "[N Annotations]", b"N");
    session.wait_for_command_count(&fixture, 52);
    complete(
        &mut session,
        mark,
        &[
            "Actual reported annotation",
            "PRIVATE_SCOPED_ACTIONS_ANNOTATION",
        ],
    );
    assert!(
        !session.screen().contains("Open local file"),
        "Reported annotation is not canonical source authority"
    );
    session.resize(if mouse { 120 } else { 80 }, if mouse { 40 } else { 24 });
    let mark = session.completed_cursorless_end();
    choose(&mut session, mouse, "[r Refresh]", b"r");
    session.wait_for_command_count(&fixture, 67);
    complete(
        &mut session,
        mark,
        &["PRIVATE_SCOPED_ACTIONS_ANNOTATION", CURRENT],
    );
    let count = commands(&session, &fixture).len();
    let mark = session.completed_cursorless_end();
    choose(&mut session, mouse, "[b Back]", b"b");
    complete(&mut session, mark, &["PRIVATE_SCOPED_ACTIONS_ANNOTATION"]);
    let mark = session.completed_cursorless_end();
    choose(&mut session, mouse, "[b Back]", b"b");
    complete(&mut session, mark, &["PRIVATE_SCOPED_ACTIONS_JOB_501_LOG"]);
    let mark = session.completed_cursorless_end();
    choose(&mut session, mouse, "[Runs]", b"R");
    complete(
        &mut session,
        mark,
        &["2 runs of 2 reported", "run 123 attempt 2", "@alice"],
    );
    assert_eq!(
        commands(&session, &fixture).len(),
        count,
        "Cached Back/Runs never renews access"
    );
    end(
        &mut session,
        &fixture,
        mouse,
        &steps,
        &before,
        &config,
        &sql_before,
    );
}
fn renamed(mouse: bool) {
    let name = "example/renamed-ci";
    let mut steps = actions_protocol();
    steps.extend(operation(ReadKind::Run, 42, name));
    let fixture = configured_native(&steps);
    let before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    let sql_before = sql(&fixture);
    open_run(&mut session, &fixture, mouse, 24, name);
    assert!(
        !session.screen().contains(CURRENT),
        "Same ID has separately observed current spelling"
    );
    jobs(&mut session, mouse);
    session.resize(if mouse { 120 } else { 80 }, if mouse { 40 } else { 24 });
    session.wait_for_cursorless_condition(
        |s| s.contents().contains("Step 2") && s.contents().contains(name),
        "Same actual job/renamed repository after resize",
        Duration::from_secs(8),
    );
    assert!(session.screen().contains("failure"));
    end(
        &mut session,
        &fixture,
        mouse,
        &steps,
        &before,
        &config,
        &sql_before,
    );
}
fn entered(session: &mut Session, fixture: &Fixture, count: usize) -> ProcessIdentity {
    session.wait_for_command_count(fixture, count);
    let rows = commands(session, fixture);
    let pid = i32::try_from(rows.last().unwrap()["pid"].as_i64().unwrap()).unwrap();
    let child = process_identity(pid)
        .unwrap()
        .expect("Actual held GH child");
    let leader = process_identity(session.owned_application_pid())
        .unwrap()
        .unwrap();
    assert_eq!(child.parent, leader.pid);
    assert_eq!(child.session, leader.session);
    assert_eq!(child.uid, leader.uid);
    assert!(child.birth >= leader.birth);
    assert!(!child.zombie);
    child
}
fn gone(session: &mut Session, child: &ProcessIdentity) {
    let deadline = Instant::now() + Duration::from_secs(4);
    while let Some(current) = process_identity(child.pid).unwrap() {
        assert_eq!(current.birth, child.birth);
        assert_eq!(current.uid, child.uid);
        assert_eq!(current.session, child.session);
        assert!(
            Instant::now() < deadline,
            "Actual owned GH child must be reaped, not kill0/zombie acceptance"
        );
        session.poll_output(Duration::from_millis(10));
    }
}
fn cancel(mouse: bool) {
    let mut held = operation(ReadKind::Logs, 42, CURRENT);
    held[7].delay_ms = 60_000;
    held.truncate(8);
    let mut steps = actions_protocol();
    steps.extend(operation(ReadKind::Run, 42, CURRENT));
    steps.extend(held);
    steps.extend(operation(ReadKind::Logs, 42, CURRENT));
    let fixture = configured_native(&steps);
    let before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    let sql_before = sql(&fixture);
    open_run(&mut session, &fixture, mouse, 24, CURRENT);
    jobs(&mut session, mouse);
    choose(&mut session, mouse, "[l Logs]", b"l");
    let child = entered(&mut session, &fixture, 32);
    session.wait_for_cursorless_condition(
        |s| s.contents().contains("[b Cancel read]"),
        "Owned pending read Cancel",
        Duration::from_secs(8),
    );
    let after = session.completed_cursorless_end();
    choose(&mut session, mouse, "[b Cancel read]", b"b");
    complete(&mut session, after, &["read canceled", "Step 2"]);
    gone(&mut session, &child);
    exact_native(&session, &fixture, &steps, 32);
    let after = session.completed_cursorless_end();
    choose(&mut session, mouse, "[l Logs]", b"l");
    session.wait_for_command_count(&fixture, 45);
    complete(&mut session, after, &["PRIVATE_SCOPED_ACTIONS_JOB_501_LOG"]);
    end(
        &mut session,
        &fixture,
        mouse,
        &steps,
        &before,
        &config,
        &sql_before,
    );
}
fn uid_loss(mouse: bool) {
    let mut steps = actions_protocol();
    steps.extend(operation(ReadKind::Run, 43, CURRENT));
    let fixture = configured_native(&steps);
    let before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    let sql_before = sql(&fixture);
    let after = session.completed_cursorless_end();
    if mouse {
        session.click_visible_action("run 123 attempt 2");
    } else {
        selected_row(&mut session, false, "run 123 attempt 2");
    }
    session.wait_for_command_count(&fixture, 24);
    complete(
        &mut session,
        after,
        &["Original Actions read unavailable", "private source hidden"],
    );
    assert!(!session.screen().contains("run 123 attempt 2"));
    assert!(!session.screen().contains("alice"));
    end(
        &mut session,
        &fixture,
        mouse,
        &steps,
        &before,
        &config,
        &sql_before,
    );
}
fn drift(mouse: bool) {
    for kind in 0..4 {
        let mut op = operation(
            if kind == 3 {
                ReadKind::Annotations
            } else {
                ReadKind::Run
            },
            42,
            CURRENT,
        );
        let mut steps = actions_protocol();
        if kind == 3 {
            steps.extend(operation(ReadKind::Run, 42, CURRENT));
        }
        if kind < 2 {
            let mut raw = fresh_run(CURRENT);
            raw[if kind == 0 { "run_attempt" } else { "head_sha" }] = if kind == 0 {
                json!(3)
            } else {
                json!("b".repeat(40))
            };
            op[3].stdout = raw.to_string().into_bytes();
            op = op[..4].iter().chain(op[7..].iter()).cloned().collect();
        } else if kind == 2 {
            let mut raw = fresh_job(CURRENT);
            raw["run_id"] = json!(999);
            op[5].stdout = json!([{"jobs":[raw]}]).to_string().into_bytes();
            op = op[..6].iter().chain(op[7..].iter()).cloned().collect();
        } else {
            let mut raw = fresh_check(CURRENT);
            raw["check_suite"]["id"] = json!(902);
            op[7].stdout = raw.to_string().into_bytes();
            op = op[..8].iter().chain(op[12..].iter()).cloned().collect();
        }
        steps.extend(op);
        let fixture = configured_native(&steps);
        let before = git_state(&fixture);
        let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
        let mut session = start(&fixture, mouse);
        let sql_before = sql(&fixture);
        if kind == 3 {
            open_run(&mut session, &fixture, mouse, 24, CURRENT);
            jobs(&mut session, mouse);
        }
        let after = session.completed_cursorless_end();
        if kind == 3 {
            choose(&mut session, mouse, "[N Annotations]", b"N");
        } else if mouse {
            session.click_visible_action("run 123 attempt 2");
        } else {
            selected_row(&mut session, false, "run 123 attempt 2");
        }
        session.wait_for_command_count(&fixture, steps.len());
        complete(
            &mut session,
            after,
            &["Original Actions read unavailable", "private source hidden"],
        );
        assert!(!session
            .screen()
            .contains("PRIVATE_SCOPED_ACTIONS_ANNOTATION"));
        end(
            &mut session,
            &fixture,
            mouse,
            &steps,
            &before,
            &config,
            &sql_before,
        );
    }
}
fn disabled_write(session: &mut Session, mouse: bool) {
    let after = session.completed_cursorless_end();
    if mouse {
        session.activate_completed_actions_control(true, "Approve fork run", b"");
    } else {
        session.focus_completed_actions_control_v2("Approve fork run");
        session.send_key(b"\r");
    }
    session.wait_for_new_cursorless_condition(
        after,
        |s| {
            // The real narrow modal hard-wraps the warning mid-word. Require
            // BOTH literal observations inside that same completed modal,
            // not a contiguous phrase borrowed from its covered parent.
            ["Approve fork run:", "current-ID adapter;", "[Esc Close]"]
                .iter()
                .all(|label| {
                    crate::support::completed_navigation_controls_v1::owned_action_position(
                        s,
                        "Action unavailable • explanation only",
                        label,
                        &[],
                    )
                    .is_some()
                })
        },
        "Owned disabled run mutation explanation",
        Duration::from_secs(8),
    );
    let after = session.completed_cursorless_end();
    choose(session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_new_cursorless_condition(
        after,
        |s| !s.contents().contains("Action unavailable") && s.contents().contains("Run 123"),
        "Closed unavailable explanation back to original run",
        Duration::from_secs(8),
    );
    // The disabled control lives on a later page. Return through real arrows
    // before the next Jobs/Refresh click; keyboard shortcuts must not conceal
    // an off-page mouse target.
    session.first_completed_actions_page(mouse);
}
fn failures(mouse: bool) {
    for failure in 0..3 {
        let mut op = operation(
            if failure == 2 {
                ReadKind::Run
            } else {
                ReadKind::Logs
            },
            42,
            CURRENT,
        );
        let mut steps = actions_protocol();
        steps.extend(operation(ReadKind::Run, 42, CURRENT));
        if failure == 2 {
            op[3].delay_ms = 60_000;
            op.truncate(4);
        } else {
            if failure == 0 {
                op[7].stdout = vec![b'X'; 512 * 1024 + 1];
            } else {
                op[7].status = 1;
                op[7].stdout.clear();
                op[7].stderr = b"PRIVATE_SCOPED_ACTIONS_FAILURE_DIAGNOSTIC".to_vec();
            }
            op = op[..8].iter().chain(op[10..].iter()).cloned().collect();
        }
        steps.extend(op);
        let fixture = configured_native(&steps);
        let before = git_state(&fixture);
        let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
        let mut session = start(&fixture, mouse);
        let sql_before = sql(&fixture);
        open_run(&mut session, &fixture, mouse, 24, CURRENT);
        // Actual key-focus/Enter OR mouse on the disabled write control.
        let count = commands(&session, &fixture).len();
        disabled_write(&mut session, mouse);
        assert_eq!(commands(&session, &fixture).len(), count);
        assert_eq!(sql(&fixture), sql_before);
        if failure < 2 {
            jobs(&mut session, mouse);
        }
        let after = session.completed_cursorless_end();
        if failure == 2 {
            choose(&mut session, mouse, "[r Refresh]", b"r");
        } else {
            choose(&mut session, mouse, "[l Logs]", b"l");
        }
        let held = if failure == 2 {
            Some(entered(&mut session, &fixture, 28))
        } else {
            None
        };
        session.wait_for_command_count(&fixture, steps.len());
        session.wait_for_new_cursorless_condition(
            after,
            |s| {
                s.contents().contains("Original Actions read unavailable")
                    && s.contents().contains("private source hidden")
            },
            "Bounded original read failure hides private view",
            Duration::from_secs(if failure == 2 { 35 } else { 8 }),
        );
        if let Some(child) = held {
            gone(&mut session, &child);
        }
        assert!(!session
            .screen()
            .contains("PRIVATE_SCOPED_ACTIONS_FAILURE_DIAGNOSTIC"));
        end(
            &mut session,
            &fixture,
            mouse,
            &steps,
            &before,
            &config,
            &sql_before,
        );
    }
}
#[test]
fn scoped_actions_keyboard_catalogue_run_job_logs_annotations_pinned_refresh_back() {
    happy(false);
}
#[test]
fn scoped_actions_mouse_catalogue_run_job_logs_annotations_pinned_refresh_back() {
    happy(true);
}
#[test]
fn scoped_actions_keyboard_same_id_rename_optional_a_matrix_and_resize() {
    renamed(false);
}
#[test]
fn scoped_actions_mouse_same_id_rename_optional_a_matrix_and_resize() {
    renamed(true);
}
#[test]
fn scoped_actions_keyboard_pending_cancel_owned_child_absent_no_late_effect() {
    cancel(false);
}
#[test]
fn scoped_actions_mouse_pending_cancel_owned_child_absent_no_late_effect() {
    cancel(true);
}
#[test]
fn scoped_actions_keyboard_closing_uid_replacement_hides_original_only() {
    uid_loss(false);
}
#[test]
fn scoped_actions_mouse_closing_uid_replacement_hides_original_only() {
    uid_loss(true);
}
#[test]
fn scoped_actions_keyboard_attempt_head_member_check_drift_refusal() {
    drift(false);
}
#[test]
fn scoped_actions_mouse_attempt_head_member_check_drift_refusal() {
    drift(true);
}
#[test]
fn scoped_actions_keyboard_pool_timeout_failed_child_and_disabled_writes() {
    failures(false);
}
#[test]
fn scoped_actions_mouse_pool_timeout_failed_child_and_disabled_writes() {
    failures(true);
}
