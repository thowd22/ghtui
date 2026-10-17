//! Actual binary/strict GH/real SQLite and dirty Git. The expected typed result
//! is chosen by the fixture protocol, never inferred from a status substring.
use super::*;

fn receipt(fixture: &Fixture, expected: &str, enabled: bool) {
    marker(fixture, expected);
    let db = durable::raw(fixture);
    let (count, kind, raw): (i64, String, String) = db
        .query_row(
            "SELECT count(*),kind_json,receipt_json FROM actions_write_attempts",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(
        serde_json::from_str::<Value>(&kind).unwrap(),
        json!(if enabled {
            "enable_workflow"
        } else {
            "disable_workflow"
        })
    );
    let value: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(value["kind"], "workflow_state");
    assert_eq!(value["resource_id"], 7);
    assert_eq!(value["observed_attempt"], Value::Null);
}
// This typed outcome has TWO actual footer rows. The shared durable helper
// intentionally describes Forms with all declared controls on the last row;
// do not weaken that contract to make Info controls appear on that row.
fn resize_outcome(session: &mut Session, columns: u16, rows: u16) {
    let after = session.completed_cells_end();
    session.resize(columns, rows);
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        if session.completed_cells_end() > after
            && session.completed_cells_screen().is_some_and(|frame| {
                frame.size() == (rows, columns)
                    && crate::support::main_footer_visible(frame)
                    && crate::support::overlay_body(frame, HISTORY).is_some_and(|body| {
                        let mut footer = body.lines().rev();
                        footer.next().is_some_and(|row| {
                            row.trim() == "[View original] [F4 Acknowledge locally] [Keep draft / Close]"
                        }) && footer.next().is_some_and(|row| {
                            row.split_whitespace().collect::<Vec<_>>().join(" ")
                                == "Tab: buttons • Esc: close [F7 Info↑] [F8 Info↓]"
                        })
                    })
            })
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "NEW resized exact owned outcome requires both complete footer rows\n{}",
            session.screen()
        );
    }
}
fn outcome_controls(s: &mut Session, f: &Fixture, mouse: bool, expected: &str) {
    let sql_before = sql(f);
    let trace = commands(s, f);
    for (columns, rows) in [(80, 24), (120, 40)] {
        resize_outcome(s, columns, rows);
        let body = s
            .completed_cells_screen()
            .and_then(|frame| crate::support::overlay_body(frame, HISTORY))
            .unwrap();
        assert!(prose_v120::adjacent_literal(&body, expected));
        assert!(body.contains("Tab: buttons • Esc: close"));
        assert!(
            !body.contains("Ctrl-A: select all")
                && !body.contains("Ctrl-P: preview")
                && !body.contains("Esc: keep draft")
        );
        assert!(body.contains("[F4 Acknowledge locally]"));
        assert!(body.contains("account #42"));
        assert!(body.contains("@octocat"));
        assert!(!body.contains("Some("));
        assert!(!body.contains("None"));
        assert!(!body.contains("[F7 Error↑]"));
        let before = s.completed_cells_end();
        form_action(s, mouse, HISTORY, "[F8 Info↓]");
        cells(s, before, HISTORY, &["[F7 Info↑]", "[F8 Info↓]"]);
        let before = s.completed_cells_end();
        form_action(s, mouse, HISTORY, "[F7 Info↑]");
        cells(s, before, HISTORY, &[expected, "[F7 Info↑]"]);
        assert_eq!(sql(f), sql_before);
        assert_eq!(commands(s, f), trace);
    }
}
fn journey(mouse: bool) {
    for (case, enabled, confirmed) in [(0, false, true), (1, true, true), (2, false, false)] {
        let before_state = if enabled {
            "disabled_inactivity"
        } else {
            "active"
        };
        let after_state = if !confirmed || enabled {
            "active"
        } else {
            "disabled_manually"
        };
        let mut steps = opening(before_state);
        steps.extend(phase(before_state));
        steps.extend(attempt(before_state, after_state, enabled, 42));
        steps.extend(history(true, after_state));
        assert_eq!(steps.len(), 68);
        let fixture = configured_native(&steps);
        if !confirmed {
            let path = fixture.root.join("config/ghtui/config.toml");
            let mut config = ghtui::config::Config::load(&path).unwrap();
            config.theme = ghtui::config::ThemeName::NoColor;
            config.save(&path).unwrap();
        }
        let git = git_state(&fixture);
        let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
        let mut session = start(&fixture, mouse);
        initial(&mut session, &fixture, mouse, before_state, enabled);
        let before = unrelated(&fixture);
        preview(&mut session, &fixture, mouse, enabled);
        let end = session.completed_cells_end();
        form_action(&mut session, mouse, &title(enabled), "[Confirm action]");
        session.wait_for_command_count(&fixture, 61);
        let expected = if confirmed {
            "Confirmed"
        } else {
            "Accepted; still waiting"
        };
        cells(
            &mut session,
            end,
            HISTORY,
            &[expected, "[View original]", "[F7 Info↑]"],
        );
        let state = if confirmed {
            "confirmed"
        } else {
            "accepted_pending"
        };
        receipt(&fixture, state, enabled);
        outcome_controls(&mut session, &fixture, mouse, expected);
        resize_outcome(&mut session, 80, 24);
        if case == 0 {
            session.capture_completed_terminal_v1("novice-workflow-confirmed80");
        }
        if case == 2 {
            session.capture_completed_terminal_v1("novice-workflow-pending80-no-color");
        }
        let end = session.completed_cells_end();
        form_action(&mut session, mouse, HISTORY, "[View original]");
        session.wait_for_command_count(&fixture, 68);
        cells(
            &mut session,
            end,
            HISTORY,
            &[
                "Saved workflow change #1",
                "workflow #7",
                "Actual current workflow 7",
                "[F4 Acknowledge locally]",
            ],
        );
        durable::resize_form(
            &mut session,
            120,
            40,
            HISTORY,
            &[
                "[View original]",
                "[F4 Acknowledge locally]",
                "[Keep draft / Close]",
            ],
        );
        let body = session
            .completed_cells_screen()
            .and_then(|frame| crate::support::overlay_body(frame, HISTORY))
            .unwrap();
        assert!(prose_v120::adjacent_literal(
            &body,
            &format!("Saved workflow: #7 • repository {CURRENT}")
        ));
        assert!(prose_v120::adjacent_literal(&body,"Recorded result: workflow state • workflow #7 • Run attempt: not applicable to workflow state"));
        assert!(!body.contains("Workflow {"));
        assert!(!body.contains("Some("));
        assert!(!body.contains("None"));
        if case == 1 {
            session.capture_completed_terminal_v1("novice-workflow-saved-result120");
        }
        // Inspect is read-only, not ACK, retry, or a transition attribution.
        receipt(&fixture, state, enabled);
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
        receipt(&fixture, state, enabled);
    }
}

#[test]
fn novice_usability_workflow_v25_keyboard_confirmed_pending_actor_history_and_same_effects() {
    journey(false);
}
#[test]
fn novice_usability_workflow_v25_mouse_confirmed_pending_actor_history_and_same_effects() {
    journey(true);
}
