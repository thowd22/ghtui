//! SOURCE ONLY / UNRUN. Sixteen actual-binary keyboard/mouse journeys, real
//! dirty Git, strict offline GH, independent SQLite/config/TTY/process effects.
//! No handler call, fake Source/owner, permissive fixture or accepted snapshot.
use super::*;
const FORM: &str = "Cancel original Actions run";
const HISTORY: &str = "Original Cancel publication";
fn running() -> Value {
    let mut value = fresh_run(CURRENT);
    value["status"] = json!("in_progress");
    value["conclusion"] = Value::Null;
    value
}
fn cancelled() -> Value {
    let mut value = running();
    value["status"] = json!("completed");
    value["conclusion"] = json!("cancelled");
    value
}
fn run_open() -> Vec<GhStep> {
    let mut steps = operation(ReadKind::Run, 42, CURRENT);
    for step in &mut steps {
        if step.args.get(1).is_some_and(|path| {
            path.ends_with("/actions/runs/123") || path.ends_with("/actions/runs/123/attempts/2")
        }) {
            step.stdout = running().to_string().into_bytes();
        }
    }
    steps
}
fn phase(value: Value, uid: u64) -> Vec<GhStep> {
    let mut steps = principal(42);
    steps.push(get(&format!("repos/{FIRST}"), current_repository()));
    steps.extend((0..2).map(|_| get(&format!("repos/{CURRENT}/actions/runs/123"), value.clone())));
    steps.push(get(&format!("repos/{FIRST}"), current_repository()));
    steps.extend(principal(uid));
    assert_eq!(steps.len(), 8);
    steps
}
fn post(fails: bool) -> GhStep {
    let mut step = GhStep::read(
        &[
            "api",
            &format!("repos/{CURRENT}/actions/runs/123/cancel"),
            "--hostname",
            HOST,
            "--method",
            "POST",
        ],
        "{}",
    );
    if fails {
        step.status = 1;
        step.stderr = b"PRIVATE_CANCEL_FAILURE_NEVER_FOR_TTY".to_vec();
    }
    step
}
fn confirm_steps(receipt: Value, fails: bool, uid: u64) -> Vec<GhStep> {
    let mut steps = principal(42);
    steps.push(get(&format!("repos/{FIRST}"), current_repository()));
    steps.push(get(&format!("repos/{CURRENT}/actions/runs/123"), running()));
    steps.extend(principal(42));
    steps.push(get(&format!("repos/{CURRENT}/actions/runs/123"), running()));
    steps.push(get(&format!("repos/{CURRENT}/actions/runs/123"), running()));
    steps.push(get(&format!("repos/{FIRST}"), current_repository()));
    steps.extend(principal(42));
    steps.push(post(fails));
    if !fails {
        steps.push(get(&format!("repos/{CURRENT}/actions/runs/123"), receipt));
        steps.push(get("user", json!({"login":"octocat","id":uid})));
    }
    assert_eq!(steps.len(), if fails { 12 } else { 14 });
    steps
}
fn opening() -> Vec<GhStep> {
    let mut steps = actions_protocol();
    steps.extend(run_open());
    steps.extend(phase(running(), 42));
    steps
}
fn ready(session: &mut Session, after: usize, words: &[&str]) {
    session.wait_for_new_cursorless_condition(
        after,
        |frame| {
            let title = if frame.contents().contains(HISTORY) {
                HISTORY
            } else {
                FORM
            };
            crate::support::overlay_body(frame, title)
                .is_some_and(|body| words.iter().all(|word| body.contains(word)))
        },
        &format!("actual owned original Cancel frame {words:?}"),
        Duration::from_secs(8),
    );
}
fn title(session: &Session) -> &'static str {
    if session
        .completed_cursorless_screen()
        .unwrap()
        .contents()
        .contains(HISTORY)
    {
        HISTORY
    } else {
        FORM
    }
}
fn control(session: &mut Session, mouse: bool, label: &str) {
    let title = title(session);
    let history = label == "[View original]"
        || label == "[F4 Acknowledge]"
        || session
            .completed_cursorless_screen()
            .unwrap()
            .contents()
            .contains("[View original]");
    let mut controls = if history {
        vec![
            "[View original]",
            "[F4 Acknowledge]",
            "[Keep draft / Close]",
        ]
    } else {
        vec!["[Preview]", "[Confirm action]", "[Keep draft / Close]"]
    };
    if session
        .completed_cursorless_screen()
        .unwrap()
        .contents()
        .contains("[F7 Error↑]")
    {
        controls.extend(["[F7 Error↑]", "[F8 Error↓]"]);
    }
    if mouse {
        let point = session.wait_for_owned_completed_action(title, label, &[]);
        session.click(point.0, point.1);
    } else {
        session.focus_completed_owned_action_v1(
            title,
            label,
            crate::support::completed_navigation_controls_v1::FocusAttributeV1::Inverse,
            &controls,
            &[],
        );
        session.send_key(b"\r");
    }
}
// These shared Forms are centered inside the main view, not full-screen
// modals. Require SIGWINCH plus the actual global footer, then a NEW completed
// frame with the complete ordered controls on THIS bounded Form's last row.
pub(super) fn resize_form(
    session: &mut Session,
    cols: u16,
    rows: u16,
    title: &str,
    controls: &[&str],
) {
    let after = session.completed_cursorless_end();
    session.resize(cols, rows);
    let expected = controls.join(" ");
    session.wait_for_new_cursorless_condition(
        after,
        |frame| {
            frame.size() == (rows, cols)
                && crate::support::overlay_body(frame, title).is_some_and(|body| {
                    body.lines()
                        .last()
                        .is_some_and(|row| row.trim() == expected)
                })
        },
        "Actual resized centered Form owns its complete ordered bottom controls",
        Duration::from_secs(8),
    );
}
fn open_form(session: &mut Session, fixture: &Fixture, mouse: bool, count: usize) {
    let after = session.completed_cursorless_end();
    resources_v2::action(session, mouse, "[C Cancel / history]", b"C");
    session.wait_for_command_count(fixture, count);
    ready(
        session,
        after,
        &[
            "ORIGINAL CANCEL INTENT",
            "Cancel run 123 attempt 2",
            "[Preview]",
            "[Confirm action]",
            "[Keep draft / Close]",
        ],
    );
}
fn initial(session: &mut Session, fixture: &Fixture, mouse: bool) {
    session.resize(120, 40);
    session.wait_for_main_frame();
    open_run(session, fixture, mouse, 24, CURRENT);
    session.wait_for_cursorless_condition(
        |frame| {
            frame
                .contents()
                .contains("Original scoped Actions target verified")
                && !frame.contents().contains("read pending")
        },
        "Actual source-bound Run publication ready before C control paging",
        Duration::from_secs(8),
    );
    open_form(session, fixture, mouse, 32);
}
fn preview(session: &mut Session, fixture: &Fixture, mouse: bool, count: usize) {
    let after = session.completed_cursorless_end();
    control(session, mouse, "[Preview]");
    session.wait_for_command_count(fixture, count);
    ready(session, after, &["CANCEL PREVIEW", "[Confirm action]", H]);
}
pub(super) fn raw(fixture: &Fixture) -> rusqlite::Connection {
    rusqlite::Connection::open_with_flags(
        database(fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap()
}
pub(super) fn attempts(fixture: &Fixture) -> Vec<(String, bool, Option<String>, String, String)> {
    let db = raw(fixture);
    let mut query=db.prepare("SELECT state,claimed,receipt_json,context_json,resources_json FROM actions_write_attempts ORDER BY attempt_id").unwrap();
    let rows = query
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })
        .unwrap();
    rows.map(Result::unwrap).collect()
}
pub(super) fn unrelated_sql(fixture: &Fixture) -> Vec<(String, Vec<Vec<rusqlite::types::Value>>)> {
    let mut rows = sql(fixture);
    rows.retain(|(name, _)| !name.starts_with("actions_write_") && name != "drafts");
    let db = raw(fixture);
    let mut query=db.prepare("SELECT host,login,resource,body FROM drafts WHERE resource NOT LIKE 'scoped-actions-v3/%' ORDER BY host,login,resource").unwrap();
    let values = query
        .query_map([], |row| {
            (0..4)
                .map(|column| row.get(column))
                .collect::<rusqlite::Result<Vec<rusqlite::types::Value>>>()
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    rows.push(("unrelated_drafts".into(), values));
    rows
}
fn exact_write(session: &Session, fixture: &Fixture, steps: &[GhStep], count: usize, posts: usize) {
    let rows = commands(session, fixture);
    assert_eq!(
        rows.len(),
        count,
        "Exact measured+prepaid original children; no silent retry/default read"
    );
    for (actual, expected) in rows.iter().zip(&steps[..count]) {
        assert_eq!(actual["args"], json!(expected.args));
        assert_eq!(actual["matched"], true);
        assert_eq!(actual["stdin_hex"], "");
    }
    assert_eq!(
        rows.iter()
            .filter(|row| row["args"]
                .as_array()
                .unwrap()
                .iter()
                .any(|arg| arg == "POST"))
            .count(),
        posts
    );
    assert!(!rows.iter().any(|row| row["args"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| matches!(arg.as_str(), Some("PUT" | "PATCH" | "DELETE" | "graphql")))));
}
pub(super) fn marker(fixture: &Fixture, state: &str, receipt: bool) {
    let rows = attempts(fixture);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, state);
    assert!(rows[0].1);
    assert_eq!(rows[0].2.is_some(), receipt);
    let context: Value = serde_json::from_str(&rows[0].3).unwrap();
    assert_eq!(context["run_id"], 123);
    assert_eq!(context["attempt"], 2);
    assert_eq!(context["head_sha"], H);
    assert_eq!(context["original_repository_v3"]["repository_id"], 10000);
    assert_eq!(
        context["original_repository_v3"]["repository_node"],
        "REPO_catalogue_10000"
    );
    assert_eq!(context["original_repository_v3"]["original_name"], FIRST);
    assert_eq!(context["original_run_v3"]["run_node"], "WR_123");
    assert_eq!(context["original_run_v3"]["workflow_id"], 7);
    assert_eq!(context["original_run_v3"]["check_suite_id"], 901);
    assert!(context["original_pr_v3"].is_null());
    let keys: Vec<String> = serde_json::from_str(&rows[0].4).unwrap();
    assert_eq!(
        keys,
        [
            format!("{HOST}/{CURRENT}/run/123"),
            format!("{HOST}/{FIRST}/run/123"),
            format!("{HOST}/repository-id/10000/actions/run/123")
        ]
    );
}
fn close_if_open(session: &mut Session, mouse: bool) {
    let text = session.completed_cursorless_screen().unwrap().contents();
    if text.contains(FORM) || text.contains(HISTORY) {
        let after = session.completed_cursorless_end();
        control(session, mouse, "[Keep draft / Close]");
        session.wait_for_new_cursorless_condition(
            after,
            |frame| !frame.contents().contains(FORM) && !frame.contents().contains(HISTORY),
            "Actual original Form closed with no resend",
            Duration::from_secs(8),
        );
    }
}
#[allow(clippy::too_many_arguments)] // independent SQL/Git/config/TTY and exact transport witnesses
fn finish(
    session: &mut Session,
    fixture: &Fixture,
    mouse: bool,
    steps: &[GhStep],
    posts: usize,
    git: &GitState,
    config: &[u8],
    sql_before: &[(String, Vec<Vec<rusqlite::types::Value>>)],
) {
    close_if_open(session, mouse);
    assert_eq!(git_state(fixture), *git);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    assert_eq!(unrelated_sql(fixture), sql_before);
    assert_private_intents_unchanged(fixture);
    assert_read_only_git(fixture);
    session.quit_and_assert_restored_terminal(mouse);
    exact_write(session, fixture, steps, steps.len(), posts);
    assert_eq!(git_state(fixture), *git);
    assert_eq!(unrelated_sql(fixture), sql_before);
}
fn success(mouse: bool) {
    let mut steps = opening();
    steps.extend(phase(running(), 42));
    steps.extend(confirm_steps(cancelled(), false, 42));
    let fixture = configured_native(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    initial(&mut session, &fixture, mouse);
    session.capture_completed_terminal_v1("actions-cancel-intent");
    let before = unrelated_sql(&fixture);
    preview(&mut session, &fixture, mouse, 40);
    session.capture_completed_terminal_v1("actions-cancel-preview");
    let after = session.completed_cursorless_end();
    control(&mut session, mouse, "[Confirm action]");
    session.wait_for_command_count(&fixture, 54);
    ready(
        &mut session,
        after,
        &[
            "ORIGINAL PROTECTED RESULT",
            "confirmed",
            "[View original]",
            "[F4 Acknowledge]",
        ],
    );
    marker(&fixture, "confirmed", true);
    resize_form(
        &mut session,
        80,
        24,
        HISTORY,
        &[
            "[View original]",
            "[F4 Acknowledge]",
            "[Keep draft / Close]",
        ],
    );
    session.capture_completed_terminal_v1("actions-cancel-history");
    let db = raw(&fixture);
    let drafts: u64 = db
        .query_row(
            "SELECT COUNT(*) FROM drafts WHERE resource LIKE 'scoped-actions-v3/%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(drafts, 0, "Only exact confirmed draft was removed");
    drop(db);
    // Old Preview cannot be invoked again: only history actions remain.
    let after = session.completed_cursorless_end();
    if mouse {
        let p = session.wait_for_owned_completed_action(HISTORY, "[F4 Acknowledge]", &[]);
        session.click(p.0, p.1);
    } else {
        session.send_key(b"\x1bOS");
    }
    ready(
        &mut session,
        after,
        &["Inspect a current original result", "[View original]"],
    );
    exact_write(&session, &fixture, &steps, 54, 1);
    finish(
        &mut session,
        &fixture,
        mouse,
        &steps,
        1,
        &git,
        &config,
        &before,
    );
}
fn accepted(mouse: bool) {
    let mut steps = opening();
    steps.extend(phase(running(), 42));
    steps.extend(confirm_steps(running(), false, 42));
    steps.extend(phase(running(), 42));
    steps.extend(phase(cancelled(), 42));
    steps.extend(phase(cancelled(), 42));
    let fixture = configured_native(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    initial(&mut session, &fixture, mouse);
    let before = unrelated_sql(&fixture);
    preview(&mut session, &fixture, mouse, 40);
    let after = session.completed_cursorless_end();
    control(&mut session, mouse, "[Confirm action]");
    session.wait_for_command_count(&fixture, 54);
    ready(
        &mut session,
        after,
        &["completion NOT confirmed", "[View original]"],
    );
    marker(&fixture, "accepted_pending", true);
    let after = session.completed_cursorless_end();
    control(&mut session, mouse, "[View original]");
    session.wait_for_command_count(&fixture, 62);
    ready(&mut session, after, &["still pending", "[F4 Acknowledge]"]);
    let snapshot = attempts(&fixture);
    let after = session.completed_cursorless_end();
    control(&mut session, mouse, "[F4 Acknowledge]");
    ready(&mut session, after, &["Inspect a current original result"]);
    assert_eq!(attempts(&fixture), snapshot);
    exact_write(&session, &fixture, &steps, 62, 1);
    let after = session.completed_cursorless_end();
    control(&mut session, mouse, "[View original]");
    session.wait_for_command_count(&fixture, 70);
    ready(
        &mut session,
        after,
        &["transition observed", "[F4 Acknowledge]"],
    );
    let after = session.completed_cursorless_end();
    control(&mut session, mouse, "[F4 Acknowledge]");
    session.wait_for_command_count(&fixture, 78);
    session.wait_for_new_cursorless_condition(
        after,
        |frame| {
            crate::support::overlay_body(frame, HISTORY).is_some_and(|body| {
                !body.contains("[Esc Cancel]")
                    && body.lines().last().is_some_and(|row| {
                        row.trim()
                            == "[View original] [F4 Acknowledge] [Keep draft / Close]"
                    })
                    && crate::owned_form_prose_v120::adjacent_literal(
                        &body,
                        "Explicit original observation acknowledged locally • no causal claim • no remote mutation/retry",
                    )
            })
        },
        "Entire local acknowledgement on ONE completed owned nonbusy result; no replay",
        Duration::from_secs(8),
    );
    marker(&fixture, "transition_observed", true);
    finish(
        &mut session,
        &fixture,
        mouse,
        &steps,
        1,
        &git,
        &config,
        &before,
    );
}
fn unknown_restart(mouse: bool) {
    let mut steps = opening();
    steps.extend(phase(running(), 42));
    steps.extend(confirm_steps(running(), true, 42));
    steps.extend(actions_protocol());
    steps.extend(run_open());
    steps.extend(phase(running(), 42));
    steps.extend(phase(running(), 42));
    let fixture = configured_native(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    initial(&mut session, &fixture, mouse);
    preview(&mut session, &fixture, mouse, 40);
    let after = session.completed_cursorless_end();
    control(&mut session, mouse, "[Confirm action]");
    session.wait_for_command_count(&fixture, 52);
    complete(&mut session, after, &["private source hidden"]);
    marker(&fixture, "uncertain", false);
    assert!(!session.screen().contains("PRIVATE_CANCEL_FAILURE"));
    let snapshot = attempts(&fixture);
    session.terminate_abruptly(); // no claim SIGKILL restored its terminal.
    let mut session = launch(&fixture, mouse);
    // SAME genuine command/source entry, but do not call the older zero-write
    // helper: this journal already contains the ONE actual Cancel POST.
    choose(&mut session, mouse, "[: Commands]", b":");
    overlay_ready(
        &mut session,
        "Commands · current context",
        &["Filter focused:", "[Enter Choose]"],
    );
    session.send_key(b"\x01");
    session.paste("Discover repositories");
    overlay_ready(
        &mut session,
        "Commands · current context",
        &["> Discover repositories"],
    );
    choose(&mut session, mouse, "> Discover repositories", b"\r");
    session.wait_for_overlay_close("Commands · current context");
    session.wait_for_screen(TITLE);
    exact_write(&session, &fixture, &steps, 52, 1);
    discover(&mut session, mouse);
    session.wait_for_command_count(&fixture, 59);
    session.wait_for_screen("Repository ID: 10000");
    filter(
        &mut session,
        mouse,
        "repo0000",
        FIRST,
        "Repository ID: 10000",
    );
    choose(&mut session, mouse, "[Open Actions]", b"A");
    session.wait_for_command_count(&fixture, 66);
    session.wait_for_overlay_close(TITLE);
    actions_ready(&mut session);
    session.resize(120, 40);
    session.wait_for_main_frame();
    open_run(&mut session, &fixture, mouse, 76, CURRENT);
    let after = session.completed_cursorless_end();
    resources_v2::action(&mut session, mouse, "[C Cancel / history]", b"C");
    session.wait_for_command_count(&fixture, 84);
    ready(
        &mut session,
        after,
        &["Outcome unknown", "[View original]", "[F4 Acknowledge]"],
    );
    assert_eq!(
        attempts(&fixture),
        snapshot,
        "Restart never sends/resets original attempt"
    );
    let before = unrelated_sql(&fixture);
    let after = session.completed_cursorless_end();
    control(&mut session, mouse, "[F4 Acknowledge]");
    session.wait_for_command_count(&fixture, 92);
    ready(&mut session, after, &["acknowledged", "no remote mutation"]);
    marker(&fixture, "acknowledged", false);
    finish(
        &mut session,
        &fixture,
        mouse,
        &steps,
        1,
        &git,
        &config,
        &before,
    );
}
fn identity_or_receipt_loss(mouse: bool) {
    for receipt_drift in [false, true] {
        let mut tail = confirm_steps(running(), false, 42);
        if receipt_drift {
            let mut wrong = running();
            wrong["node_id"] = json!("OTHER_ACTUAL_RUN_NODE");
            tail[12].stdout = wrong.to_string().into_bytes();
            tail.truncate(13);
        } else {
            tail[10] = get("user", json!({"id":43,"login":"new-principal"}));
            tail.truncate(11);
        }
        let mut steps = opening();
        steps.extend(phase(running(), 42));
        steps.extend(tail);
        let fixture = configured_native(&steps);
        let git = git_state(&fixture);
        let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
        let mut session = start(&fixture, mouse);
        initial(&mut session, &fixture, mouse);
        let before = unrelated_sql(&fixture);
        preview(&mut session, &fixture, mouse, 40);
        let after = session.completed_cursorless_end();
        control(&mut session, mouse, "[Confirm action]");
        session.wait_for_command_count(&fixture, steps.len());
        complete(&mut session, after, &["private source hidden"]);
        marker(
            &fixture,
            if receipt_drift {
                "uncertain"
            } else {
                "pending"
            },
            false,
        );
        assert!(!session.screen().contains(H));
        assert!(!session.screen().contains("PRIVATE_CANCEL"));
        finish(
            &mut session,
            &fixture,
            mouse,
            &steps,
            usize::from(receipt_drift),
            &git,
            &config,
            &before,
        );
    }
}
fn changed_sql(mouse: bool) {
    let mut steps = opening();
    steps.extend(phase(running(), 42));
    let fixture = configured_native(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    initial(&mut session, &fixture, mouse);
    preview(&mut session, &fixture, mouse, 40);
    let db = raw(&fixture);
    let sources:Vec<String>=db.prepare("SELECT source FROM sync_reservations WHERE host=?1 AND login='user-id:42' AND source LIKE 'repository-catalogue-source-v1:%'").unwrap().query_map([HOST],|row|row.get(0)).unwrap().map(Result::unwrap).collect();
    assert_eq!(sources.len(), 1);
    drop(db);
    Cache::open(&database(&fixture))
        .unwrap()
        .reserve_sync_generation(&Scope::github(HOST, Some(42)).unwrap(), &sources[0])
        .unwrap();
    let before = unrelated_sql(&fixture);
    let after = session.completed_cursorless_end();
    control(&mut session, mouse, "[Confirm action]");
    complete(&mut session, after, &["private source hidden"]);
    assert!(attempts(&fixture).is_empty());
    exact_write(&session, &fixture, &steps, 40, 0);
    finish(
        &mut session,
        &fixture,
        mouse,
        &steps,
        0,
        &git,
        &config,
        &before,
    );
}
fn cancel_held(mouse: bool) {
    for after_post in [false, true] {
        let mut tail = confirm_steps(running(), false, 42);
        let held = if after_post { 12 } else { 7 };
        tail[held].delay_ms = 60_000;
        tail.truncate(held + 1);
        let mut steps = opening();
        steps.extend(phase(running(), 42));
        steps.extend(tail);
        let fixture = configured_native(&steps);
        let git = git_state(&fixture);
        let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
        let mut session = start(&fixture, mouse);
        initial(&mut session, &fixture, mouse);
        let before = unrelated_sql(&fixture);
        preview(&mut session, &fixture, mouse, 40);
        control(&mut session, mouse, "[Confirm action]");
        let child = entered(&mut session, &fixture, steps.len());
        marker(&fixture, "pending", false);
        let point = session.wait_for_owned_completed_action(FORM, "[Esc Cancel]", &[]);
        let after = session.completed_cursorless_end();
        if mouse {
            session.click(point.0, point.1);
        } else {
            session.send_key(b"\x1b");
        }
        session.wait_for_new_cursorless_condition(
            after,
            |frame| {
                let text = frame.contents();
                text.contains("scope is fenced")
                    && text.contains("private pages hidden")
                    && !text.contains(FORM)
                    && !text.contains(HISTORY)
                    && !text.contains(H)
                    && !text.contains("@alice")
                    && crate::support::main_footer_visible(frame)
            },
            "Actual owned Cancel hides private source and leaves a usable fenced main view",
            Duration::from_secs(8),
        );
        gone(&mut session, &child);
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            let actual = attempts(&fixture);
            if actual[0].0 == if after_post { "uncertain" } else { "pending" } {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Actual owned writer result protected after cancellation: {actual:?}"
            );
            session.poll_output(Duration::from_millis(10));
        }
        marker(
            &fixture,
            if after_post { "uncertain" } else { "pending" },
            false,
        );
        finish(
            &mut session,
            &fixture,
            mouse,
            &steps,
            usize::from(after_post),
            &git,
            &config,
            &before,
        );
    }
}
fn tiny_and_back(mouse: bool) {
    let mut steps = opening();
    steps.extend(phase(running(), 42));
    steps.extend(phase(running(), 42));
    let fixture = configured_native(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    initial(&mut session, &fixture, mouse);
    let before = unrelated_sql(&fixture);
    preview(&mut session, &fixture, mouse, 40);
    let old_confirm = session.wait_for_owned_completed_action(FORM, "[Confirm action]", &[]);
    if !mouse {
        session.focus_completed_owned_action_v1(
            FORM,
            "[Confirm action]",
            crate::support::completed_navigation_controls_v1::FocusAttributeV1::Inverse,
            &["[Preview]", "[Confirm action]", "[Keep draft / Close]"],
            &[],
        );
    }
    session.resize_owned_single_action_modal_v2(
        40,
        10,
        "Original Actions • resize required",
        "[Esc Close]",
    );
    assert!(!session.screen().contains("[Confirm action]"));
    let after = session.completed_cursorless_end();
    if mouse {
        session.click(old_confirm.0, old_confirm.1);
    } else {
        session.send_key(b"\r");
    }
    complete(
        &mut session,
        after,
        if mouse {
            &["resize required", "Actions disabled"]
        } else {
            &["resize required", "Fresh useful exact original Preview"]
        },
    );
    exact_write(&session, &fixture, &steps, 40, 0);
    assert!(attempts(&fixture).is_empty());
    resize_form(
        &mut session,
        120,
        40,
        FORM,
        &["[Preview]", "[Confirm action]", "[Keep draft / Close]"],
    );
    let after = session.completed_cursorless_end();
    control(&mut session, mouse, "[Keep draft / Close]");
    complete(&mut session, after, &["Composer closed"]);
    let after = session.completed_cursorless_end();
    resources_v2::action(&mut session, mouse, "[C Cancel / history]", b"C");
    session.wait_for_command_count(&fixture, 48);
    ready(
        &mut session,
        after,
        &["ORIGINAL CANCEL INTENT", "[Preview]"],
    );
    assert!(attempts(&fixture).is_empty());
    finish(
        &mut session,
        &fixture,
        mouse,
        &steps,
        0,
        &git,
        &config,
        &before,
    );
}
fn pending_view_change(mouse: bool) {
    let mut held = phase(running(), 42);
    held[3].delay_ms = 60_000;
    held.truncate(4);
    let mut steps = opening();
    steps.extend(held);
    steps.extend(phase(running(), 42));
    let fixture = configured_native(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    initial(&mut session, &fixture, mouse);
    let before = unrelated_sql(&fixture);
    control(&mut session, mouse, "[Preview]");
    let child = entered(&mut session, &fixture, 36);
    let point = session.wait_for_owned_completed_action(FORM, "[Esc Cancel]", &[]);
    let after = session.completed_cursorless_end();
    if mouse {
        session.click(point.0, point.1);
    } else {
        session.send_key(b"\x1b");
    }
    ready(&mut session, after, &["[Keep draft / Close]"]);
    gone(&mut session, &child);
    close_if_open(&mut session, mouse);
    // The old held Preview was genuinely cancelled/reaped. Open a NEW actual
    // Form from the retained source and resize/Close it before changing views;
    // the old result must not hide/cancel/reopen this fresh private original.
    open_form(&mut session, &fixture, mouse, 44);
    resize_form(
        &mut session,
        80,
        24,
        FORM,
        &["[Preview]", "[Confirm action]", "[Keep draft / Close]"],
    );
    assert!(session.screen().contains("ORIGINAL CANCEL INTENT"));
    assert!(!session.screen().contains("private source hidden"));
    exact_write(&session, &fixture, &steps, 44, 0);
    let after = session.completed_cursorless_end();
    control(&mut session, mouse, "[Keep draft / Close]");
    complete(&mut session, after, &["Composer closed"]);
    assert!(attempts(&fixture).is_empty());
    choose(&mut session, mouse, "2 Repository", b"2");
    session.wait_for_main_frame();
    session.assert_focused_view("Repository");
    assert!(!session.screen().contains("CANCEL PREVIEW"));
    assert!(attempts(&fixture).is_empty());
    finish(
        &mut session,
        &fixture,
        mouse,
        &steps,
        0,
        &git,
        &config,
        &before,
    );
}
#[test]
fn scoped_actions_publication_v3_keyboard_actual_confirmed_cancel_receipt_and_exact_draft_cleanup()
{
    success(false)
}
#[test]
fn scoped_actions_publication_v3_mouse_actual_confirmed_cancel_receipt_and_exact_draft_cleanup() {
    success(true)
}
#[test]
fn scoped_actions_publication_v3_keyboard_accepted_then_fresh_transition_explicit_ack_no_retry() {
    accepted(false)
}
#[test]
fn scoped_actions_publication_v3_mouse_accepted_then_fresh_transition_explicit_ack_no_retry() {
    accepted(true)
}
#[test]
fn scoped_actions_publication_v3_keyboard_unknown_post_restart_original_history_and_explicit_ack() {
    unknown_restart(false)
}
#[test]
fn scoped_actions_publication_v3_mouse_unknown_post_restart_original_history_and_explicit_ack() {
    unknown_restart(true)
}
#[test]
fn scoped_actions_publication_v3_keyboard_final_uid_or_raw_receipt_drift_hides_private_preserves_marker(
) {
    identity_or_receipt_loss(false)
}
#[test]
fn scoped_actions_publication_v3_mouse_final_uid_or_raw_receipt_drift_hides_private_preserves_marker(
) {
    identity_or_receipt_loss(true)
}
#[test]
fn scoped_actions_publication_v3_keyboard_actual_sql_change_before_confirm_zero_children_zero_attempts(
) {
    changed_sql(false)
}
#[test]
fn scoped_actions_publication_v3_mouse_actual_sql_change_before_confirm_zero_children_zero_attempts(
) {
    changed_sql(true)
}
#[test]
fn scoped_actions_publication_v3_keyboard_owned_cancel_before_or_after_post_exact_child_gone() {
    cancel_held(false)
}
#[test]
fn scoped_actions_publication_v3_mouse_owned_cancel_before_or_after_post_exact_child_gone() {
    cancel_held(true)
}
#[test]
fn scoped_actions_publication_v3_keyboard_tiny_confirm_refusal_pure_back_new_open_no_old_preview() {
    tiny_and_back(false)
}
#[test]
fn scoped_actions_publication_v3_mouse_tiny_confirm_refusal_pure_back_new_open_no_old_preview() {
    tiny_and_back(true)
}
#[test]
fn scoped_actions_publication_v3_keyboard_pending_preview_cancel_new_view_no_late_private_repaint()
{
    pending_view_change(false)
}
#[test]
fn scoped_actions_publication_v3_mouse_pending_preview_cancel_new_view_no_late_private_repaint() {
    pending_view_change(true)
}
