//! SOURCE ONLY / UNRUN. Eight paired actual-binary journeys; actual installed
//! Git, strict gh, real journal/restart, exact PID cleanup and terminal restore.
use super::publication_v3 as durable;
use super::*;
use crate::owned_form_prose_v120 as prose_v120;
#[path = "novice_usability_workflow_v25.rs"]
mod novice_usability_v25;
const HISTORY: &str = "Original workflow publication";
fn workflow(state: &str) -> Value {
    json!({"id":7,"node_id":"WF_7","name":"ACTUAL_PRIVATE_WORKFLOW","path":".github/workflows/ci.yml","state":state})
}
fn wf(value: Value) -> GhStep {
    get(&format!("repos/{CURRENT}/actions/workflows/7"), value)
}
fn list(state: &str) -> Vec<GhStep> {
    resources_v2::resource(
        vec![paged(
            &format!("repos/{CURRENT}/actions/workflows?per_page=100"),
            json!({"total_count":1,"workflows":[workflow(state)]}),
        )],
        42,
    )
}
fn definition(state: &str) -> Vec<GhStep> {
    let sha = "c".repeat(40);
    resources_v2::resource(vec![wf(workflow(state)),
    get(&format!("repos/{CURRENT}/commits/main"),json!({"sha":sha})),GhStep::read(&["api",&format!("repos/{CURRENT}/contents/.github/workflows/ci.yml?ref={sha}"),"--hostname",HOST,"--method","GET","--header","Accept: application/vnd.github.raw+json","--allow-escape-sequences"],
    "name: ACTUAL_PRIVATE_WORKFLOW\non: [push]\njobs:\n  actual_job:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo NEVER_EXECUTED_WORKFLOW\n"),wf(workflow(state))],42)
}
fn phase(state: &str) -> Vec<GhStep> {
    let result = resources_v2::resource(vec![wf(workflow(state)), wf(workflow(state))], 42);
    assert_eq!(result.len(), 8);
    result
}
fn ack(enabled: bool) -> GhStep {
    GhStep::read(
        &[
            "api",
            &format!(
                "repos/{CURRENT}/actions/workflows/7/{}",
                if enabled { "enable" } else { "disable" }
            ),
            "--hostname",
            HOST,
            "--method",
            "PUT",
            "--include",
        ],
        "HTTP/2.0 204 No Content\nContent-Length: 0\r\n\r\n",
    )
}
fn attempt(before: &str, after: &str, enabled: bool, uid: u64) -> Vec<GhStep> {
    let mut out = principal(42);
    out.push(get(&format!("repos/{FIRST}"), current_repository()));
    out.push(wf(workflow(before)));
    out.extend(principal(42));
    out.extend([wf(workflow(before)), wf(workflow(before))]);
    out.push(get(&format!("repos/{FIRST}"), current_repository()));
    out.extend(principal(42));
    out.push(ack(enabled));
    out.push(wf(workflow(after)));
    out.push(get("user", json!({"id":uid,"login":"octocat"})));
    assert_eq!(out.len(), 14);
    out
}
fn history(selected: bool, state: &str) -> Vec<GhStep> {
    let out = resources_v2::resource(
        if selected {
            vec![wf(workflow(state))]
        } else {
            vec![]
        },
        42,
    );
    assert_eq!(out.len(), if selected { 7 } else { 6 });
    out
}
fn opening(state: &str) -> Vec<GhStep> {
    let mut out = actions_protocol();
    out.extend(list(state));
    out.extend(definition(state));
    out.extend(phase(state));
    assert_eq!(out.len(), 39);
    out
}
fn title(enabled: bool) -> String {
    format!(
        "{} workflow • original whole workflow",
        if enabled { "Enable" } else { "Disable" }
    )
}
fn cells(session: &mut Session, after: usize, title: &str, words: &[&str]) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        if session.completed_cells_end() > after
            && session
                .completed_cells_screen()
                .and_then(|frame| crate::support::overlay_body(frame, title))
                .is_some_and(|body| prose_v120::ready_workflow_body_v124(&body, words))
        {
            return;
        }
        assert!(
            Instant::now() < until,
            "NEW exact owned workflow frame {title} {words:?}\n{}",
            session.screen()
        );
    }
}
fn form_action(session: &mut Session, mouse: bool, title: &str, label: &str) {
    let body = session
        .completed_cells_screen()
        .and_then(|frame| crate::support::overlay_body(frame, title))
        .expect("Actual completed owned workflow Form");
    let mut controls = if body.contains("[View original]") {
        vec![
            "[View original]",
            "[F4 Acknowledge locally]",
            "[Keep draft / Close]",
        ]
    } else {
        vec![
            "[Preview]",
            "[Confirm action]",
            if body.contains("[Esc Cancel]") {
                "[Esc Cancel]"
            } else {
                "[Keep draft / Close]"
            },
        ]
    };
    if body.contains("[F7 Error↑]") {
        controls.extend(["[F7 Error↑]", "[F8 Error↓]"]);
    } else if body.contains("[F7 Info↑]") {
        controls.extend(["[F7 Info↑]", "[F8 Info↓]"]);
    }
    if mouse {
        let (x, y) = session.wait_for_owned_completed_action(title, label, &[]);
        session.click(x, y);
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
fn initial(session: &mut Session, fixture: &Fixture, mouse: bool, state: &str, enabled: bool) {
    session.resize(120, 40);
    session.wait_for_main_frame();
    let after = session.completed_cursorless_end();
    resources_v2::action(session, mouse, "Workflows", b"W");
    session.wait_for_command_count(fixture, 21);
    session.wait_for_new_cursorless_condition(
        after,
        |frame| {
            let text = frame.contents();
            text.contains("ACTUAL_PRIVATE_WORKFLOW")
                && text.contains(".github/workflows/ci.yml")
                && !text.contains("Requested ref main")
                && !text.contains("read pending")
        },
        "Actual completed workflow inventory, not old Definition geometry",
        Duration::from_secs(8),
    );
    let after = session.completed_cursorless_end();
    selected_row(session, mouse, "ACTUAL_PRIVATE_WORKFLOW");
    session.wait_for_command_count(fixture, 31);
    complete(
        session,
        after,
        &[
            "Requested ref main",
            "cccccccccccccccccccccccccccccccccccccccc",
        ],
    );
    let after = session.completed_cells_end();
    let command = if enabled {
        "Enable workflow"
    } else {
        "Disable workflow"
    };
    if mouse {
        resources_v2::action(session, true, command, b"");
    } else {
        session.focus_completed_actions_control_v2(command);
        session.send_key(b"\r");
    }
    session.wait_for_command_count(fixture, 39);
    cells(
        session,
        after,
        &title(enabled),
        &[
            "entire repository • all refs",
            "workflow 7",
            "[Preview]",
            "[Confirm action]",
            state,
        ],
    );
}
fn preview(session: &mut Session, fixture: &Fixture, mouse: bool, enabled: bool) {
    let after = session.completed_cells_end();
    form_action(session, mouse, &title(enabled), "[Preview]");
    session.wait_for_command_count(fixture, 47);
    cells(
        session,
        after,
        &title(enabled),
        &[
            "WHOLE WORKFLOW PREVIEW",
            "[Confirm action]",
            ".github/workflows/ci.yml",
        ],
    );
}
fn unrelated(fixture: &Fixture) -> Vec<(String, Vec<Vec<rusqlite::types::Value>>)> {
    let mut rows = sql(fixture);
    rows.retain(|(name, _)| !name.starts_with("actions_write_") && name != "drafts");
    let db = durable::raw(fixture);
    let mut query=db.prepare("SELECT host,login,resource,body FROM drafts WHERE resource NOT LIKE 'scoped-actions-v6/%' ORDER BY host,login,resource").unwrap();
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
fn exact(session: &Session, fixture: &Fixture, steps: &[GhStep], puts: usize) {
    let rows = commands(session, fixture);
    assert_eq!(rows.len(), steps.len());
    for (row, step) in rows.iter().zip(steps) {
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(row["matched"], true);
        assert_eq!(row["stdin_hex"], "");
    }
    assert_eq!(
        rows.iter()
            .filter(|row| row["args"]
                .as_array()
                .unwrap()
                .iter()
                .any(|arg| arg == "PUT"))
            .count(),
        puts
    );
    assert!(!rows.iter().any(|row| row["args"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| matches!(arg.as_str(), Some("POST" | "PATCH" | "DELETE" | "graphql")))));
}
fn marker(fixture: &Fixture, state: &str) {
    let db = durable::raw(fixture);
    let (actual,kind,context,options,resources):(String,String,String,String,String)=db.query_row("SELECT state,kind_json,context_json,options_json,resources_json FROM actions_write_attempts",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?))).unwrap();
    assert_eq!(actual, state);
    assert!(kind.contains("workflow"));
    let c: Value = serde_json::from_str(&context).unwrap();
    assert_eq!(c["original_workflow_v6"]["workflow_id"], 7);
    assert_eq!(c["original_workflow_v6"]["node"], "WF_7");
    assert!(c.get("original_run_v3").is_none());
    assert_eq!(c["run_id"], Value::Null);
    assert!(serde_json::from_str::<Value>(&options).unwrap()["enabled"].is_boolean());
    assert!(resources.contains("github.invalid/repository-id/10000/actions/workflow/7"));
}
fn close(session: &mut Session, mouse: bool, enabled: bool) {
    let text = session.completed_cells_screen().unwrap().contents();
    let current = if text.contains(HISTORY) {
        Some(HISTORY.to_owned())
    } else if text.contains(&title(enabled)) {
        Some(title(enabled))
    } else {
        None
    };
    if let Some(title) = current {
        let after = session.completed_cells_end();
        form_action(session, mouse, &title, "[Keep draft / Close]");
        session.wait_for_new_cursorless_condition(
            after,
            |frame| {
                !frame.contents().contains(&title) && crate::support::main_footer_visible(frame)
            },
            "Actual workflow Close returns usable main footer",
            Duration::from_secs(8),
        );
    }
}
#[allow(clippy::too_many_arguments)]
fn finish(
    session: &mut Session,
    fixture: &Fixture,
    mouse: bool,
    enabled: bool,
    steps: &[GhStep],
    puts: usize,
    git: &GitState,
    config: &[u8],
    before: &[(String, Vec<Vec<rusqlite::types::Value>>)],
) {
    close(session, mouse, enabled);
    assert_eq!(git_state(fixture), *git);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    assert_eq!(unrelated(fixture), before);
    assert_private_intents_unchanged(fixture);
    assert_read_only_git(fixture);
    session.quit_and_assert_restored_terminal(mouse);
    exact(session, fixture, steps, puts);
    assert_eq!(git_state(fixture), *git);
    assert_eq!(unrelated(fixture), before);
}
fn success(mouse: bool, enabled: bool) {
    let state = if enabled {
        "disabled_inactivity"
    } else {
        "active"
    };
    let wanted = if enabled {
        "active"
    } else {
        "disabled_manually"
    };
    let mut steps = opening(state);
    steps.extend(phase(state));
    steps.extend(attempt(state, wanted, enabled, 42));
    let fixture = configured_native(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    initial(&mut session, &fixture, mouse, state, enabled);
    let before = unrelated(&fixture);
    preview(&mut session, &fixture, mouse, enabled);
    durable::resize_form(
        &mut session,
        80,
        24,
        &title(enabled),
        &["[Preview]", "[Confirm action]", "[Keep draft / Close]"],
    );
    let after = session.completed_cells_end();
    form_action(&mut session, mouse, &title(enabled), "[Confirm action]");
    session.wait_for_command_count(&fixture, 61);
    cells(
        &mut session,
        after,
        HISTORY,
        &["same workflow state observed", "[View original]"],
    );
    marker(&fixture, "confirmed");
    session.capture_completed_terminal_v1(if enabled {
        "workflow-enable-confirmed80"
    } else {
        "workflow-disable-confirmed80"
    });
    finish(
        &mut session,
        &fixture,
        mouse,
        enabled,
        &steps,
        1,
        &git,
        &config,
        &before,
    );
}
fn pending(mouse: bool) {
    let mut steps = opening("active");
    steps.extend(phase("active"));
    steps.extend(attempt("active", "active", false, 42));
    steps.extend(history(true, "active"));
    steps.extend(history(true, "disabled_manually"));
    steps.extend(history(true, "disabled_manually"));
    let fixture = configured_native(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    initial(&mut session, &fixture, mouse, "active", false);
    let before = unrelated(&fixture);
    preview(&mut session, &fixture, mouse, false);
    let after = session.completed_cells_end();
    form_action(&mut session, mouse, &title(false), "[Confirm action]");
    session.wait_for_command_count(&fixture, 61);
    cells(
        &mut session,
        after,
        HISTORY,
        &["NOT observed", "[View original]"],
    );
    marker(&fixture, "accepted_pending");
    let after = session.completed_cells_end();
    form_action(&mut session, mouse, HISTORY, "[View original]");
    session.wait_for_command_count(&fixture, 68);
    cells(
        &mut session,
        after,
        HISTORY,
        &[
            "Actual current workflow 7",
            "state active",
            "[F4 Acknowledge locally]",
        ],
    );
    let count = commands(&session, &fixture).len();
    form_action(&mut session, mouse, HISTORY, "[F4 Acknowledge locally]");
    cells(
        &mut session,
        after,
        HISTORY,
        &["Inspect exact original workflow state before ACK"],
    );
    assert_eq!(commands(&session, &fixture).len(), count);
    let after = session.completed_cells_end();
    form_action(&mut session, mouse, HISTORY, "[View original]");
    session.wait_for_command_count(&fixture, 75);
    cells(&mut session, after, HISTORY, &["state disabled_manually"]);
    let after = session.completed_cells_end();
    form_action(&mut session, mouse, HISTORY, "[F4 Acknowledge locally]");
    session.wait_for_command_count(&fixture, 82);
    cells(
        &mut session,
        after,
        HISTORY,
        &["acknowledged locally", "no PUT/resend"],
    );
    marker(&fixture, "transition_observed");
    finish(
        &mut session,
        &fixture,
        mouse,
        false,
        &steps,
        1,
        &git,
        &config,
        &before,
    );
}
fn refusals(mouse: bool) {
    for kind in 0..3 {
        let mut tail = attempt("active", "disabled_manually", false, 42);
        let expected;
        if kind == 0 {
            let mut changed = workflow("active");
            changed["node_id"] = json!("RECREATED_WORKFLOW");
            tail[3] = wf(changed);
            tail.truncate(4);
            expected = None;
        } else if kind == 1 {
            tail[10] = get("user", json!({"id":999,"login":"octocat"}));
            tail.truncate(11);
            expected = Some("pending");
        } else {
            let mut changed = workflow("disabled_manually");
            changed["path"] = json!(".github/workflows/other.yml");
            tail[12] = wf(changed);
            tail.truncate(13);
            expected = Some("uncertain");
        }
        let mut steps = opening("active");
        steps.extend(phase("active"));
        steps.extend(tail);
        let fixture = configured_native(&steps);
        let git = git_state(&fixture);
        let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
        let mut session = start(&fixture, mouse);
        initial(&mut session, &fixture, mouse, "active", false);
        let before = unrelated(&fixture);
        preview(&mut session, &fixture, mouse, false);
        let after = session.completed_cursorless_end();
        form_action(&mut session, mouse, &title(false), "[Confirm action]");
        session.wait_for_command_count(&fixture, steps.len());
        complete(&mut session, after, &["private source hidden"]);
        if let Some(state) = expected {
            marker(&fixture, state);
        } else {
            let db = durable::raw(&fixture);
            assert_eq!(
                db.query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE name='actions_write_attempts'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                0
            );
        }
        assert!(!session.screen().contains("ACTUAL_PRIVATE_WORKFLOW"));
        finish(
            &mut session,
            &fixture,
            mouse,
            false,
            &steps,
            usize::from(kind == 2),
            &git,
            &config,
            &before,
        );
    }
}
fn held(mouse: bool) {
    for post in [false, true] {
        let at = if post { 12 } else { 8 };
        let mut tail = attempt("active", "disabled_manually", false, 42);
        tail[at].delay_ms = 60_000;
        tail.truncate(at + 1);
        let mut steps = opening("active");
        steps.extend(phase("active"));
        steps.extend(tail);
        let fixture = configured_native(&steps);
        let git = git_state(&fixture);
        let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
        let mut session = start(&fixture, mouse);
        initial(&mut session, &fixture, mouse, "active", false);
        let before = unrelated(&fixture);
        preview(&mut session, &fixture, mouse, false);
        form_action(&mut session, mouse, &title(false), "[Confirm action]");
        let child = entered(&mut session, &fixture, steps.len());
        let after = session.completed_cursorless_end();
        if mouse {
            let (x, y) =
                session.wait_for_owned_completed_action(&title(false), "[Esc Cancel]", &[]);
            session.click(x, y);
        } else {
            session.send_key(b"\x1b");
        }
        session.wait_for_new_cursorless_condition(
            after,
            |frame| {
                let text = frame.contents();
                text.contains("scope is fenced")
                    && text.contains("private pages hidden")
                    && !text.contains("ACTUAL_PRIVATE_WORKFLOW")
                    && crate::support::main_footer_visible(frame)
            },
            "Actual workflow invocation cancellation privacy fence",
            Duration::from_secs(8),
        );
        gone(&mut session, &child);
        let until = Instant::now() + Duration::from_secs(4);
        loop {
            let rows = durable::attempts(&fixture);
            if rows[0].0 == if post { "uncertain" } else { "pending" } {
                break;
            }
            assert!(
                Instant::now() < until,
                "Actual workflow cancellation must settle original journal"
            );
            session.poll_output(Duration::from_millis(10));
        }
        marker(&fixture, if post { "uncertain" } else { "pending" });
        finish(
            &mut session,
            &fixture,
            mouse,
            false,
            &steps,
            usize::from(post),
            &git,
            &config,
            &before,
        );
    }
}
fn committed_ack_visible_v135(session: &mut Session, mouse: bool, after: usize) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        let end = session.completed_cursorless_end();
        if end > after
            && session.completed_actions_page().is_some()
            && session
                .completed_actions_control("Acknowledge inspected state")
                .is_none()
        {
            if let Some((frame, body)) = session.completed_cursorless_screen().and_then(|frame| {
                (crate::support::main_footer_visible(frame)
                    && frame
                        .contents()
                        .contains("Inspect original Actions publication")
                    && !frame.contents().contains("[Esc Cancel]"))
                .then(|| crate::support::overlay_body(frame, "Actions • focused view"))
                .flatten()
                .filter(|body| !body.contains("[Acknowledge inspected state]"))
                .map(|body| (frame, body))
            }) {
                if body.contains("ACK committed locally") && body.contains("no PUT/retry") {
                    return;
                }
                let size = frame.size();
                if mouse {
                    session.wheel(size.1 / 2, size.0.saturating_sub(5), true);
                } else {
                    session.send_key(b"\x1b[6~");
                }
                loop {
                    session.poll_output(Duration::from_millis(1));
                    if session.completed_cursorless_end() > end
                        && session
                            .completed_cursorless_screen()
                            .and_then(|frame| {
                                crate::support::overlay_body(frame, "Actions • focused view")
                            })
                            .is_some_and(|current| current != body)
                    {
                        break;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "ONE post-ACK Actions scroll needs NEW changed owned body\n{}",
                        session.screen()
                    );
                }
            }
        }
        assert!(
            Instant::now() < deadline,
            "Both complete original ACK and no PUT/retry facts must be visible after actual publication\n{}",
            session.screen()
        );
    }
}
fn restart(mouse: bool) {
    let mut first = opening("active");
    first.extend(phase("active"));
    first.extend(attempt("active", "active", false, 42));
    let mut second = actions_protocol();
    second.extend(history(false, "disabled_manually"));
    second.extend(history(true, "disabled_manually"));
    second.extend(history(true, "disabled_manually"));
    let mut steps = first.clone();
    steps.extend(second);
    let fixture = configured_native(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    initial(&mut session, &fixture, mouse, "active", false);
    let before = unrelated(&fixture);
    preview(&mut session, &fixture, mouse, false);
    let after = session.completed_cells_end();
    form_action(&mut session, mouse, &title(false), "[Confirm action]");
    session.wait_for_command_count(&fixture, 61);
    cells(&mut session, after, HISTORY, &["NOT observed"]);
    marker(&fixture, "accepted_pending");
    close(&mut session, mouse, false);
    session.quit_and_assert_restored_terminal(mouse);
    assert_eq!(unrelated(&fixture), before);
    let mut resumed = restart_start_v120(&fixture, mouse, first.len());
    resumed.wait_for_command_count(&fixture, 75);
    // The user explicitly discovered the Catalogue a second time. Prove the
    // ONE genuine reservation advances, preserving its exact source and every
    // other independent row, before baselining subsequent read/ACK effects.
    let after_discovery = unrelated(&fixture);
    let mut expected_discovery = before.clone();
    let (_, reservations) = expected_discovery
        .iter_mut()
        .find(|(table, _)| table == "sync_reservations")
        .unwrap();
    assert_eq!(reservations.len(), 1);
    let row = &mut reservations[0];
    use rusqlite::types::Value as SqlValue;
    assert_eq!(row.len(), 5);
    assert_eq!(row[0], SqlValue::Text(HOST.into()));
    assert_eq!(row[1], SqlValue::Text("user-id:42".into()));
    assert!(matches!(&row[2], SqlValue::Text(source)
        if source.starts_with("repository-catalogue-source-v1:")));
    assert_eq!(row[4], SqlValue::Integer(1));
    let SqlValue::Integer(old_generation) = row[3] else {
        panic!("Actual existing Catalogue generation is an integer");
    };
    assert!(old_generation > 0);
    row[3] = SqlValue::Integer(old_generation.checked_add(1).unwrap());
    assert_eq!(
        after_discovery, expected_discovery,
        "Only the explicit second Catalogue producer reservation may advance once"
    );
    let after = resumed.completed_cursorless_end();
    resources_v2::action(&mut resumed, mouse, "[P Publications]", b"P");
    resumed.wait_for_command_count(&fixture, 81);
    complete(&mut resumed, after, &["Original #1", "DisableWorkflow"]);
    let after = resumed.completed_cursorless_end();
    selected_row(&mut resumed, mouse, "Original #1");
    resumed.wait_for_command_count(&fixture, 88);
    complete(
        &mut resumed,
        after,
        &["Actual current workflow 7", "state disabled_manually"],
    );
    let after = resumed.completed_cursorless_end();
    if mouse {
        resources_v2::action(&mut resumed, true, "Acknowledge", b"");
    } else {
        resumed.focus_completed_actions_control_v2("Acknowledge");
        resumed.send_key(b"\r");
    }
    resumed.wait_for_command_count(&fixture, 95);
    committed_ack_visible_v135(&mut resumed, mouse, after);
    marker(&fixture, "transition_observed");
    finish(
        &mut resumed,
        &fixture,
        mouse,
        false,
        &steps,
        1,
        &git,
        &config,
        &after_discovery,
    );
}
fn restart_start_v120(fixture: &Fixture, mouse: bool, offset: usize) -> Session {
    let mut session = launch(fixture, mouse);
    let prior = commands(&session, fixture);
    assert_eq!(prior.len(), offset);
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
    ready(&mut session, &[TITLE, "[s Source]", "[Esc Close]"]);
    // Prior writes remain in the same strict cumulative journal. Pure opening
    // must preserve every old record, not reclassify them as GET or reset it.
    assert_eq!(commands(&session, fixture), prior);
    discover(&mut session, mouse);
    session.wait_for_command_count(fixture, offset + 7);
    filter(
        &mut session,
        mouse,
        "repo0000",
        FIRST,
        "Repository ID: 10000",
    );
    choose(&mut session, mouse, "[Open Actions]", b"A");
    session.wait_for_command_count(fixture, offset + 14);
    session.wait_for_overlay_close(TITLE);
    actions_ready(&mut session);
    session
}
fn tiny(mouse: bool) {
    let mut steps = opening("active");
    steps.extend(phase("active"));
    let fixture = configured_native(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    initial(&mut session, &fixture, mouse, "active", false);
    let before = unrelated(&fixture);
    preview(&mut session, &fixture, mouse, false);
    let after = session.completed_cursorless_end();
    session.resize_owned_single_action_modal_v2(
        40,
        10,
        "Original Actions • resize required",
        "[Esc Close]",
    );
    session.wait_for_new_cursorless_condition(
        after,
        |frame| frame.size() == (10, 40) && frame.contents().contains("Actions disabled"),
        "Actual tiny workflow guidance, not hidden Confirm",
        Duration::from_secs(8),
    );
    assert!(!session.screen().contains("[Confirm action]"));
    if mouse {
        let (x, y) = session.wait_for_owned_completed_action(
            "Original Actions • resize required",
            "[Esc Close]",
            &[],
        );
        session.click(x, y);
    } else {
        session.send_key(b"\x1b");
    }
    session.resize(120, 40);
    session.wait_for_main_frame();
    assert_eq!(commands(&session, &fixture).len(), 47);
    finish(
        &mut session,
        &fixture,
        mouse,
        false,
        &steps,
        0,
        &git,
        &config,
        &before,
    );
}
struct Stopped(ProcessIdentity);
impl Stopped {
    fn capture(session: &Session, pid: i32) -> Self {
        let leader = process_identity(session.owned_application_pid())
            .unwrap()
            .unwrap();
        let child = process_identity(pid)
            .unwrap()
            .expect("Actual held UID child");
        assert_eq!(child.parent, leader.pid);
        assert_eq!(child.session, leader.session);
        assert_eq!(child.uid, leader.uid);
        assert!(child.birth >= leader.birth && !child.zombie);
        assert_eq!(process_identity(pid).unwrap().as_ref(), Some(&child));
        // SAFETY: the immediately revalidated positive PID is this fixture's
        // original UID/SID/birth child; this test never signals a process group.
        assert_eq!(unsafe { libc::kill(pid, libc::SIGSTOP) }, 0);
        Self(child)
    }
}
impl Drop for Stopped {
    fn drop(&mut self) {
        if let Ok(Some(current)) = process_identity(self.0.pid) {
            if !current.zombie
                && (current.uid, current.session, current.birth)
                    == (self.0.uid, self.0.session, self.0.birth)
            {
                // SAFETY: resume only the exact original UID/SID/birth child;
                // a recycled PID or unrelated descendant is never signaled.
                assert_eq!(unsafe { libc::kill(current.pid, libc::SIGCONT) }, 0);
            }
        }
    }
}
fn row_drift(mouse: bool) {
    for column in ["receipt_json", "context_json", "options_json"] {
        let mut last = history(true, "disabled_manually");
        last.last_mut().unwrap().delay_ms = 5000;
        let mut steps = opening("active");
        steps.extend(phase("active"));
        steps.extend(attempt("active", "active", false, 42));
        steps.extend(history(true, "disabled_manually"));
        steps.extend(last);
        let fixture = configured_native(&steps);
        let git = git_state(&fixture);
        let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
        let mut session = start(&fixture, mouse);
        initial(&mut session, &fixture, mouse, "active", false);
        let before = unrelated(&fixture);
        preview(&mut session, &fixture, mouse, false);
        let after = session.completed_cells_end();
        form_action(&mut session, mouse, &title(false), "[Confirm action]");
        session.wait_for_command_count(&fixture, 61);
        cells(&mut session, after, HISTORY, &["NOT observed"]);
        let after = session.completed_cells_end();
        form_action(&mut session, mouse, HISTORY, "[View original]");
        session.wait_for_command_count(&fixture, 68);
        cells(&mut session, after, HISTORY, &["state disabled_manually"]);
        form_action(&mut session, mouse, HISTORY, "[F4 Acknowledge locally]");
        let child = entered(&mut session, &fixture, 75);
        let stopped = Stopped::capture(&session, child.pid);
        let db = rusqlite::Connection::open(database(&fixture)).unwrap();
        let generations: Vec<(String, i64)> = {
            let mut q=db.prepare("SELECT resource_key,generation FROM actions_write_generations ORDER BY resource_key").unwrap();
            let rows = q
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .map(Result::unwrap)
                .collect();
            rows
        };
        let original: String = db
            .query_row(
                &format!("SELECT {column} FROM actions_write_attempts WHERE attempt_id=1"),
                [],
                |r| r.get(0),
            )
            .unwrap();
        let mut value: Value = serde_json::from_str(&original).unwrap();
        match column {
            "receipt_json" => value["resource_id"] = json!(8),
            "context_json" => value["original_workflow_v6"]["node"] = json!("CONCURRENT_NODE"),
            _ => value["enabled"] = json!(true),
        }
        let changed = value.to_string();
        db.execute(
            &format!("UPDATE actions_write_attempts SET {column}=?1 WHERE attempt_id=1"),
            [&changed],
        )
        .unwrap();
        drop(stopped);
        session.wait_for_cursorless_condition(
            |frame| {
                frame.contents().contains("private source hidden")
                    && crate::support::main_footer_visible(frame)
            },
            "Actual last-UID raw workflow-row drift is fenced, no release",
            Duration::from_secs(8),
        );
        // No cancellation was requested: the original 5s child is allowed to
        // finish, then its exact identity must genuinely disappear.
        gone(&mut session, &child);
        assert_eq!(
            db.query_row(
                "SELECT state FROM actions_write_attempts WHERE attempt_id=1",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "accepted_pending"
        );
        assert_eq!(
            db.query_row(
                &format!("SELECT {column} FROM actions_write_attempts WHERE attempt_id=1"),
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            changed
        );
        let after: Vec<(String, i64)> = {
            let mut q=db.prepare("SELECT resource_key,generation FROM actions_write_generations ORDER BY resource_key").unwrap();
            let rows = q
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .map(Result::unwrap)
                .collect();
            rows
        };
        assert_eq!(after, generations);
        finish(
            &mut session,
            &fixture,
            mouse,
            false,
            &steps,
            1,
            &git,
            &config,
            &before,
        );
    }
}

#[test]
fn scoped_actions_workflow_v6_keyboard_disable_exact_whole_workflow_effect() {
    success(false, false)
}
#[test]
fn scoped_actions_workflow_v6_mouse_disable_exact_whole_workflow_effect() {
    success(true, false)
}
#[test]
fn scoped_actions_workflow_v6_keyboard_enable_actual_disabled_resize_and_confirm() {
    success(false, true)
}
#[test]
fn scoped_actions_workflow_v6_mouse_enable_actual_disabled_resize_and_confirm() {
    success(true, true)
}
#[test]
fn scoped_actions_workflow_v6_keyboard_accepted_pending_inspect_transition_ack_no_resend() {
    pending(false)
}
#[test]
fn scoped_actions_workflow_v6_mouse_accepted_pending_inspect_transition_ack_no_resend() {
    pending(true)
}
#[test]
fn scoped_actions_workflow_v6_keyboard_recreated_path_uid_refusal_and_uncertainty() {
    refusals(false)
}
#[test]
fn scoped_actions_workflow_v6_mouse_recreated_path_uid_refusal_and_uncertainty() {
    refusals(true)
}
#[test]
fn scoped_actions_workflow_v6_keyboard_held_pre_and_post_invocation_cancel_exact_pid() {
    held(false)
}
#[test]
fn scoped_actions_workflow_v6_mouse_held_pre_and_post_invocation_cancel_exact_pid() {
    held(true)
}
#[test]
fn scoped_actions_workflow_v6_keyboard_real_restart_workflow_inspection_and_ack() {
    restart(false)
}
#[test]
fn scoped_actions_workflow_v6_mouse_real_restart_workflow_inspection_and_ack() {
    restart(true)
}
#[test]
fn scoped_actions_workflow_v6_keyboard_tiny_has_no_hidden_confirm_or_effect() {
    tiny(false)
}
#[test]
fn scoped_actions_workflow_v6_mouse_tiny_has_no_hidden_confirm_or_effect() {
    tiny(true)
}
#[test]
fn scoped_actions_workflow_v6_keyboard_last_uid_raw_receipt_context_options_drift_protected() {
    row_drift(false)
}
#[test]
fn scoped_actions_workflow_v6_mouse_last_uid_raw_receipt_context_options_drift_protected() {
    row_drift(true)
}
