//! SOURCE ONLY / UNRUN: original retained PR result→history/ACK and genuine
//! many-blank YAML without a native full-YAML result expansion.
use super::*;
fn history() -> Vec<GhStep> {
    let mut out = principal(42);
    out.push(get(&format!("repos/{FIRST}"), repository(0)));
    out.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    out.push(wf("disabled_manually"));
    out.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    out.push(get(&format!("repos/{FIRST}"), repository(0)));
    out.extend(principal(42));
    assert_eq!(out.len(), 9);
    out
}
fn run(mouse: bool, large_yaml: bool) {
    let cases: Vec<Option<&str>> = if large_yaml {
        vec![None]
    } else {
        vec![
            None,
            Some("cache"),
            Some("receipt_json"),
            Some("context_json"),
            Some("options_json"),
        ]
    };
    for drift in cases {
        let mut expected = steps(vec![]);
        expected.extend(list());
        let mut actual_definition = definition();
        if large_yaml {
            let raw = actual_definition
                .iter_mut()
                .find(|step| {
                    step.args
                        .iter()
                        .any(|arg| arg == "Accept: application/vnd.github.raw+json")
                })
                .unwrap();
            let mut yaml = "name: PRIVATE_PR_WORKFLOW\non: [push]\njobs: {}\n".to_owned();
            yaml.push_str(&"\n".repeat(4_000));
            raw.stdout = yaml.into_bytes();
        }
        expected.extend(actual_definition);
        expected.extend(phase());
        expected.extend(phase());
        let mut actual_confirm = confirm(42);
        if !large_yaml {
            actual_confirm[16] = wf("active");
        }
        expected.extend(actual_confirm);
        assert_eq!(expected.len(), 128);
        if drift.is_none() {
            expected.extend(history());
            if !large_yaml {
                expected.extend(history());
                expected.extend(history());
            }
        }
        let fixture = resource_configured(&expected);
        let git = git_state(&fixture);
        let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
        let mut session = launch(&fixture, mouse);
        open_run(&mut session, &fixture, mouse);
        let trace = git_commands(&fixture);
        open_original_form(&mut session, &fixture, mouse);
        let after = session.completed_cells_end();
        form_action(&mut session, mouse, FORM, "[Confirm action]");
        session.wait_for_command_count(&fixture, 128);
        cells(
            &mut session,
            after,
            HISTORY,
            &[
                if large_yaml {
                    "same workflow state observed"
                } else {
                    "NOT observed"
                },
                "[View original]",
            ],
        );
        let db = raw(&fixture);
        if let Some(column) = drift {
            if column == "cache" {
                // Advance the actual producer's existing reservation through
                // Cache, not a fictional checkpoint which was never loaded.
                let scope = Scope::github(HOST, Some(42)).unwrap();
                let sources: Vec<String> = db.prepare(
                    "SELECT source FROM sync_reservations WHERE host=?1 AND login=?2 AND source LIKE 'repository-catalogue-source-v1:%'"
                ).unwrap().query_map([&scope.host, &scope.login], |row| row.get(0))
                    .unwrap().map(Result::unwrap).collect();
                assert_eq!(sources.len(), 1, "Exactly the actual Catalogue producer");
                let mut cache = Cache::open(&database(&fixture)).unwrap();
                assert!(cache
                    .sync_checkpoint(&scope, &sources[0])
                    .unwrap()
                    .is_none());
                let old = cache
                    .sync_reservation(&scope, &sources[0])
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    cache.reserve_sync_generation(&scope, &sources[0]).unwrap(),
                    old + 1
                );
            } else {
                // Only this deliberate external-drift seam is writable;
                // every independent observation remains the read-only db.
                let writer = rusqlite::Connection::open(database(&fixture)).unwrap();
                assert_eq!(
                    writer
                        .execute(
                            &format!("UPDATE actions_write_attempts SET {column}={column}||' '"),
                            []
                        )
                        .unwrap(),
                    1
                );
            }
        }
        // Observe AFTER the deliberate external edit; app must preserve it and
        // all independently unrelated rows, not repair or silently acknowledge.
        let before = unrelated(&fixture);
        let after = session.completed_cells_end();
        form_action(&mut session, mouse, HISTORY, "[View original]");
        if drift.is_some() {
            session.wait_for_new_cursorless_condition(after,|frame|{
                let text=frame.contents();text.contains("private source hidden")
                    && !text.contains(HISTORY)&&!text.contains(DETAIL_BODY)
                    && crate::support::main_footer_visible(frame)},
                "Actual own-row/cache drift refuses history and fences only original private source",Duration::from_secs(8));
            assert_eq!(
                db.query_row("SELECT state FROM actions_write_attempts", [], |r| r
                    .get::<_, String>(0))
                    .unwrap(),
                "accepted_pending"
            );
        } else {
            session.wait_for_command_count(&fixture, 137);
            owned_v120::visible(
                &mut session,
                mouse,
                after,
                HISTORY,
                &[
                    "Actual current workflow 7",
                    "state disabled_manually",
                    "not causation",
                ],
            );
            if !large_yaml {
                let after = session.completed_cells_end();
                form_action(&mut session, mouse, HISTORY, "[F4 Acknowledge locally]");
                session.wait_for_command_count(&fixture, 146);
                owned_v120::visible(
                    &mut session,
                    mouse,
                    after,
                    HISTORY,
                    &["acknowledged locally", "no PUT/resend"],
                );
                assert_eq!(
                    db.query_row("SELECT state FROM actions_write_attempts", [], |r| r
                        .get::<_, String>(0))
                        .unwrap(),
                    "transition_observed"
                );
                let after = session.completed_cells_end();
                form_action(&mut session, mouse, HISTORY, "[View original]");
                session.wait_for_command_count(&fixture, 155);
                owned_v120::visible(
                    &mut session,
                    mouse,
                    after,
                    HISTORY,
                    &[
                        "Actual current workflow 7",
                        "state disabled_manually",
                        "not causation",
                    ],
                );
            } else {
                assert_eq!(
                    db.query_row("SELECT state FROM actions_write_attempts", [], |r| r
                        .get::<_, String>(0))
                        .unwrap(),
                    "confirmed"
                );
                session.capture_completed_terminal_v1("workflow-many-blank-yaml-bounded-result");
            }
            let after = session.completed_cells_end();
            form_action(&mut session, mouse, HISTORY, "[Keep draft / Close]");
            session.wait_for_new_cursorless_condition(
                after,
                |frame| {
                    !frame.contents().contains(HISTORY)
                        && crate::support::main_footer_visible(frame)
                },
                "Actual retained PR workflow history Close",
                Duration::from_secs(8),
            );
        }
        assert_eq!(git_state(&fixture), git);
        assert_eq!(git_commands(&fixture), trace);
        assert_eq!(unrelated(&fixture), before);
        assert_eq!(
            fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
            config
        );
        session.quit_and_assert_restored_terminal(mouse);
        let actual = commands(&session, &fixture);
        assert_eq!(actual.len(), expected.len());
        for (actual, expected) in actual.iter().zip(&expected) {
            assert_eq!(actual["args"], json!(expected.args));
            assert_eq!(actual["matched"], true);
            assert_eq!(actual["stdin_hex"], "");
        }
        assert_eq!(
            actual
                .iter()
                .filter(|row| row["args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|arg| arg == "PUT"))
                .count(),
            1
        );
        assert!(!actual.iter().any(|row| row["args"]
            .as_array()
            .unwrap()
            .iter()
            .any(|arg| matches!(arg.as_str(), Some("POST" | "PATCH" | "DELETE" | "graphql")))));
        assert_eq!(git_state(&fixture), git);
        assert_eq!(unrelated(&fixture), before);
    }
}
#[test]
fn scoped_actions_workflow_v6_keyboard_retained_pr_inspect_ack_and_newer_sql_drift() {
    run(false, false)
}
#[test]
fn scoped_actions_workflow_v6_mouse_retained_pr_inspect_ack_and_newer_sql_drift() {
    run(true, false)
}
#[test]
fn scoped_actions_workflow_v6_keyboard_many_blank_yaml_confirms_bounded_metadata_and_inspects() {
    run(false, true)
}
#[test]
fn scoped_actions_workflow_v6_mouse_many_blank_yaml_confirms_bounded_metadata_and_inspects() {
    run(true, true)
}

fn over_rows_refusal_v120(mouse: bool) {
    let mut expected = steps(vec![]);
    expected.extend(list());
    let mut actual = definition();
    let raw_step = actual
        .iter_mut()
        .find(|step| {
            step.args
                .iter()
                .any(|arg| arg == "Accept: application/vnd.github.raw+json")
        })
        .unwrap();
    raw_step.stdout = format!(
        "name: PRIVATE_PR_WORKFLOW\non: [push]\njobs: {{}}\n{}",
        "\n".repeat(10_000)
    )
    .into_bytes();
    expected.extend(actual);
    let fixture = resource_configured(&expected);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    open_run(&mut session, &fixture, mouse);
    let after = session.completed_cursorless_end();
    control(&mut session, mouse, "Workflows", b"W");
    session.wait_for_command_count(&fixture, 72);
    session.wait_for_new_cursorless_condition(
        after,
        |frame| {
            frame.contents().contains("PRIVATE_PR_WORKFLOW")
                && !frame.contents().contains("read pending")
        },
        "Actual original Workflows before over-row definition",
        Duration::from_secs(8),
    );
    let sql = unrelated(&fixture);
    let trace = git_commands(&fixture);
    let after = session.completed_cursorless_end();
    original_workflow_row(&mut session, mouse);
    session.wait_for_command_count(&fixture, 86);
    session.wait_for_new_cursorless_condition(
        after,
        |frame| {
            let body = frame.contents();
            body.contains("scope is fenced")
                && body.contains("private pages hidden")
                && !body.contains("PRIVATE_PR_WORKFLOW")
                && !body.contains(FORM)
                && !body.contains("[Confirm action]")
                && crate::support::main_footer_visible(frame)
        },
        "Actual >5000 definition rows refuse before a Plan, not a hidden truncated Preview",
        Duration::from_secs(8),
    );
    assert_eq!(
        raw(&fixture)
            .query_row("SELECT COUNT(*) FROM actions_write_attempts", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(unrelated(&fixture), sql);
    assert_eq!(git_commands(&fixture), trace);
    assert_eq!(git_state(&fixture), git);
    session.quit_and_assert_restored_terminal(mouse);
    let rows = commands(&session, &fixture);
    assert_eq!(rows.len(), expected.len());
    for (row, expected) in rows.iter().zip(&expected) {
        assert_eq!(row["args"], json!(expected.args));
        assert_eq!(row["matched"], true);
        assert_eq!(row["stdin_hex"], "");
        assert!(!row["args"].as_array().unwrap().iter().any(|arg| matches!(
            arg.as_str(),
            Some("POST" | "PUT" | "PATCH" | "DELETE" | "graphql")
        )));
    }
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    assert_eq!(git_state(&fixture), git);
}
#[test]
fn scoped_actions_workflow_v6_keyboard_over5000_yaml_rows_refuse_before_plan_or_publication() {
    over_rows_refusal_v120(false)
}
#[test]
fn scoped_actions_workflow_v6_mouse_over5000_yaml_rows_refuse_before_plan_or_publication() {
    over_rows_refusal_v120(true)
}
