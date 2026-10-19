//! UNRUN actual process restart and original accepted-Cancel/Force separation.
use super::*;
use crate::repository_catalogue_native_v1::actions_v1::scoped_actions_v1::selected_row;
fn history_facts_visible_v135(
    session: &mut Session,
    mouse: bool,
    after: usize,
    facts: &[&str],
    acknowledged: bool,
) {
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut scrolled_after = None;
    loop {
        session.poll_output(Duration::from_millis(1));
        let end = session.completed_cursorless_end();
        if end > after && scrolled_after.is_none_or(|previous| end > previous) {
            if let Some((frame, body)) = session.completed_cursorless_screen().and_then(|frame| {
                (crate::support::main_footer_visible(frame)
                    && frame.contents().contains("Inspect original Actions")
                    && !frame.contents().contains("[Esc Cancel]"))
                .then(|| crate::support::overlay_body(frame, "Actions • focused view"))
                .flatten()
                .filter(|_| {
                    !acknowledged
                        || (session.completed_actions_page().is_some()
                            && session
                                .completed_actions_control("Acknowledge inspected state")
                                .is_none())
                })
                .map(|body| (frame, body))
            }) {
                if facts.iter().all(|fact| body.contains(fact)) {
                    return;
                }
                let size = frame.size();
                let previous = body;
                scrolled_after = Some(end);
                if mouse {
                    session.wheel(size.1 / 2, size.0.saturating_sub(5), true);
                } else {
                    session.send_key(b"\x1b[6~");
                }
                // One real selection/viewport input until a NEW complete
                // owned body actually changes. No sleep/retry/hidden model.
                loop {
                    session.poll_output(Duration::from_millis(1));
                    if session.completed_cursorless_end() > end
                        && session
                            .completed_cursorless_screen()
                            .and_then(|frame| {
                                crate::support::overlay_body(frame, "Actions • focused view")
                            })
                            .is_some_and(|body| body != previous)
                    {
                        break;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "ONE Actions history scroll needs NEW changed owned frame\n{}",
                        session.screen()
                    );
                }
            }
        }
        assert!(
            Instant::now() < deadline,
            "FULL original facts {facts:?} must actually coexist in owned viewport\n{}",
            session.screen()
        );
    }
}
fn generic_phase(v: Option<Value>) -> Vec<GhStep> {
    let mut out = principal(42);
    out.push(get(&format!("repos/{FIRST}"), repository(0)));
    out.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    if let Some(v) = v {
        out.push(get(&format!("repos/{FIRST}/actions/runs/123"), v));
    }
    out.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    out.push(get(&format!("repos/{FIRST}"), repository(0)));
    out.extend(principal(42));
    out
}
fn restart(mouse: bool) {
    for unknown in [false, true] {
        let c = C::RerunAll;
        let mut first = start_steps(c);
        if unknown {
            let at = first
                .iter()
                .position(|s| s.args.iter().any(|a| a == "POST"))
                .unwrap();
            first[at].status = 1;
            first[at].stderr = b"PRIVATE_UNKNOWN_POST".to_vec();
            first.truncate(at + 1);
        }
        let current = if unknown {
            m(2, "completed", "failure")
        } else {
            m(3, "queued", "")
        };
        let mut expected = first.clone();
        expected.extend(initial());
        expected.extend(generic_phase(None));
        expected.extend(generic_phase(Some(current.clone())));
        expected.extend(hist(Some(current), 42));
        let f = resource_configured(&expected);
        let git = git_state(&f);
        let cfg = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
        let mut s = launch(&f, mouse);
        open_preview(&mut s, &f, mouse, c, 0);
        let after = s.completed_cells_end();
        act(&mut s, mouse, &title(c), "[Confirm action]");
        s.wait_for_command_count(&f, first.len());
        if unknown {
            s.wait_for_new_cursorless_condition(
                after,
                |fr| {
                    fr.contents().contains("private")
                        && !fr.contents().contains(&ht(c))
                        && crate::support::main_footer_visible(fr)
                },
                "Actual unknown protected request has no restored live proof",
                Duration::from_secs(8),
            );
        } else {
            complete_cells(&mut s, after, &ht(c), &["[View original]"]);
            close(&mut s, mouse, c);
        }
        let original = journal_rows(&f);
        assert_eq!(
            state(&f),
            if unknown {
                "uncertain"
            } else {
                "accepted_pending"
            }
        );
        s.quit_and_assert_restored_terminal(mouse);
        let mut resumed = launch(&f, mouse);
        selected_offset_v120(&mut resumed, &f, mouse, first.len());
        resumed.wait_for_command_count(&f, first.len() + 31);
        let sql = unrelated(&f);
        let trace = git_commands(&f);
        let after = resumed.completed_cursorless_end();
        choose(&mut resumed, mouse, "[F8 Actions history]", b"\x1b[19~");
        resumed.wait_for_command_count(&f, first.len() + 39);
        resumed.wait_for_new_cursorless_condition(
            after,
            |fr| fr.contents().contains("Original #1") && fr.contents().contains("RerunAll"),
            "Fresh actual Summary history, no saved source permit",
            Duration::from_secs(8),
        );
        let before_inspection = resumed.completed_cursorless_end();
        selected_row(&mut resumed, mouse, "Original #1");
        resumed.wait_for_command_count(&f, first.len() + 48);
        history_facts_visible_v135(
            &mut resumed,
            mouse,
            before_inspection,
            &["Actual current readable run 123", "HISTORICAL"],
            false,
        );
        assert_eq!(journal_rows(&f), original);
        let before_ack = resumed.completed_cursorless_end();
        if mouse {
            control(&mut resumed, true, "Acknowledge", b"");
        } else {
            resumed.focus_completed_actions_control_v2("Acknowledge");
            resumed.send_key(b"\r");
        }
        resumed.wait_for_command_count(&f, expected.len());
        history_facts_visible_v135(
            &mut resumed,
            mouse,
            before_ack,
            &["ACK committed locally"],
            true,
        );
        assert_eq!(
            state(&f),
            if unknown {
                "acknowledged"
            } else {
                "transition_observed"
            }
        );
        assert_eq!(unrelated(&f), sql);
        assert_eq!(git_commands(&f), trace);
        assert_eq!(git_state(&f), git);
        resumed.quit_and_assert_restored_terminal(mouse);
        exact_v8(&resumed, &f, &expected, 1);
        assert_eq!(
            fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
            cfg
        );
    }
}
fn escalation(mouse: bool) {
    let first = start_steps(C::Cancel);
    let second = start_steps(C::ForceCancel);
    let mut expected = first.clone();
    expected.extend(second.clone());
    expected.extend(hist(Some(m(2, "completed", "cancelled")), 42));
    expected.extend(hist(Some(m(2, "completed", "cancelled")), 42));
    let f = resource_configured(&expected);
    let git = git_state(&f);
    let cfg = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
    let mut s = launch(&f, mouse);
    begin(&mut s, &f, mouse, C::Cancel, first.len());
    let old = journal_rows(&f)[0].clone();
    close(&mut s, mouse, C::Cancel);
    s.quit_and_assert_restored_terminal(mouse);
    let mut resumed = launch(&f, mouse);
    begin_offset(
        &mut resumed,
        &f,
        mouse,
        C::ForceCancel,
        first.len() + second.len(),
        first.len(),
    );
    let sql = unrelated(&f);
    let after = resumed.completed_cells_end();
    act(&mut resumed, mouse, &ht(C::ForceCancel), "[View original]");
    resumed.wait_for_command_count(&f, first.len() + second.len() + 10);
    owned_v120::visible(
        &mut resumed,
        mouse,
        after,
        &ht(C::ForceCancel),
        &["Later original run transition observed"],
    );
    let after = resumed.completed_cells_end();
    act(&mut resumed, mouse, &ht(C::ForceCancel), "[F4 Acknowledge]");
    resumed.wait_for_command_count(&f, expected.len());
    owned_v120::visible(
        &mut resumed,
        mouse,
        after,
        &ht(C::ForceCancel),
        &["acknowledged locally"],
    );
    let rows = journal_rows(&f);
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0], old,
        "Force read/ACK MUST NOT rewrite/autoACK accepted Cancel"
    );
    assert_eq!(state(&f), "transition_observed");
    assert_eq!(unrelated(&f), sql);
    assert_eq!(git_state(&f), git);
    close(&mut resumed, mouse, C::ForceCancel);
    resumed.quit_and_assert_restored_terminal(mouse);
    exact_v8(&resumed, &f, &expected, 2);
    assert_eq!(
        fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
        cfg
    );
}
#[test]
fn scoped_actions_retained_history_v8_keyboard_actual_restart_pending_and_accepted_ack() {
    restart(false);
}
#[test]
fn scoped_actions_retained_history_v8_mouse_actual_restart_pending_and_accepted_ack() {
    restart(true);
}
#[test]
fn scoped_actions_retained_history_v8_keyboard_force_ack_keeps_old_accepted_cancel() {
    escalation(false);
}
#[test]
fn scoped_actions_retained_history_v8_mouse_force_ack_keeps_old_accepted_cancel() {
    escalation(true);
}
