//! Actual binary/strict gh/real Git and SQL. Display is never operation proof.
use super::*;

fn body(s: &Session) -> String {
    let frame = s
        .completed_cells_screen()
        .expect("Actual completed Dispatch result");
    let body = crate::support::overlay_body(frame, HISTORY).expect("SAME owned Dispatch result");
    let labels = footer_labels(&body);
    assert!(labels.iter().all(|label| body.contains(label)));
    assert!(
        crate::support::completed_navigation_controls_v1::owned_focus_v1(
            frame,
            HISTORY,
            labels[0],
            crate::support::completed_navigation_controls_v1::FocusAttributeV1::Inverse,
            &labels,
            &[],
        )
        .is_some()
    );
    body
}
fn visible_proof(s: &mut Session, mouse: bool, literal: &str) {
    let until = Instant::now() + Duration::from_secs(8);
    for _ in 0..64 {
        s.poll_output(Duration::from_millis(1));
        let old = body(s);
        if crate::owned_form_prose_v120::adjacent_literal(&old, literal) {
            return;
        }
        let frame = s.completed_cells_screen().unwrap();
        let size = frame.size();
        let (x, y) = crate::support::locate_screen(frame, HISTORY).unwrap();
        let after = s.completed_cells_end();
        if mouse {
            s.wheel(x + 2, y + 5, true);
        } else {
            s.send_key(b"\x1b[B");
        }
        loop {
            s.poll_output(Duration::from_millis(1));
            if s.completed_cells_end() > after
                && s.completed_cells_screen()
                    .is_some_and(|frame| frame.size() == size)
                && body(s) != old
            {
                break;
            }
            assert!(
                Instant::now() < until,
                "ONE real proof scroll needs NEW same-owned changed body for {literal}\n{}",
                s.screen()
            );
        }
    }
    panic!("Full original proof was not reachable: {literal}");
}
fn enabled(s: &mut Session, label: &str) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        let _ = body(s);
        let frame = s.completed_cells_screen().unwrap();
        let point = crate::support::completed_navigation_controls_v1::owned_action_position(
            frame,
            HISTORY,
            label,
            &[],
        )
        .unwrap();
        if frame
            .cell(point.1, point.0)
            .is_some_and(|cell| cell.underline())
        {
            return;
        }
        assert!(
            Instant::now() < until,
            "Actual completed owned action must be enabled before {label}\n{}",
            s.screen()
        );
    }
}
fn journey(mouse: bool) {
    for p in [Protocol::Compatible, Protocol::Detailed] {
        let mut expected = opening();
        expected.extend(schema_phase(p));
        expected.extend(schema_phase(p));
        expected.extend(confirmation(p, 42));
        let confirmed = expected.len();
        let history = task54_w7_lifecycle::history(p, true);
        let inspected = confirmed + history.len();
        expected.extend(history.clone());
        expected.extend(history);
        let f = configured_native(&expected);
        let config_path = f.root.join("config/ghtui/config.toml");
        let mut config = ghtui::config::Config::load(&config_path).unwrap();
        config.theme = ghtui::config::ThemeName::NoColor;
        config.save(&config_path).unwrap();
        let config_bytes = fs::read(&config_path).unwrap();
        let git = git_state(&f);
        let mut s = start(&f, mouse);
        begin(&mut s, &f, mouse, p);
        let unrelated_before = unrelated(&f);
        inspect_schema(&mut s, &f, mouse);
        preview(&mut s, &f, mouse);
        let after = s.completed_cells_end();
        action(&mut s, mouse, INTENT, "[Confirm action]");
        s.wait_for_command_count(&f, confirmed);
        let run_label = if p.detailed() {
            "[F11 Open run]"
        } else {
            "[F11 Run unavailable]"
        };
        owned(
            &mut s,
            after,
            HISTORY,
            &[
                "Accepted; completion unknown",
                "[F6 Acknowledge locally]",
                run_label,
                "[Close]",
            ],
        );
        let original = metadata(&f);
        assert_eq!(original.0, "accepted_pending");
        let tty = s.actual_terminal_witness_v4().unwrap();
        for (width, height) in [(120, 40), (80, 20), (80, 24)] {
            resize_form(
                &mut s,
                width,
                height,
                HISTORY,
                &[
                    "[View original]",
                    "[F6 Acknowledge locally]",
                    run_label,
                    "[Close]",
                    "[F7 Info↑]",
                    "[F8 Info↓]",
                ],
            );
            let shown = body(&s);
            assert!(
                shown.find("Accepted; completion unknown").unwrap()
                    < shown.find("Original request").unwrap()
            );
            assert!(shown.contains("Tab: buttons • Esc: close"));
            assert!(
                !shown.contains("Ctrl-A: select all")
                    && !shown.contains("Ctrl-P: preview")
                    && !shown.contains("Esc: keep draft")
            );
            assert!(
                !shown.contains(SECRET)
                    && !shown.contains("NEVER_RUN_LOCALLY")
                    && !shown.contains("Some(")
                    && !shown.contains("None")
            );
            assert_eq!(metadata(&f), original);
            assert_eq!(unrelated(&f), unrelated_before);
            assert_eq!(commands(&s, &f).len(), confirmed);
        }
        for fact in [
            "Original request • one attempt per explicit Confirm".to_owned(),
            "numeric repository 10000 • workflow 7".to_owned(),
            "Definition path: .github/workflows/ci.yml".to_owned(),
            format!("Protocol: {}", p.version()),
            "POST literal branch name: main".to_owned(),
            format!("Observed chosen ref tip: {C} (commit)"),
            format!("Actual peeled definition commit: {C}"),
            "Omit does not suppress a server default.".to_owned(),
            "not an atomic ref/ABA guarantee.".to_owned(),
            "acceptance is not completion.".to_owned(),
        ] {
            if fact == "acceptance is not completion." && !p.detailed() {
                continue;
            }
            visible_proof(&mut s, mouse, &fact);
        }
        visible_proof(&mut s, mouse, "Close keeps its protected record.");
        visible_proof(
            &mut s,
            mouse,
            "Acknowledgement is local only, not workflow completion.",
        );
        assert_eq!(commands(&s, &f).len(), confirmed);
        assert_eq!(metadata(&f), original);
        if !p.detailed() {
            let frame = s.completed_cells_screen().unwrap();
            let point = crate::support::locate_screen(frame, run_label).unwrap();
            assert!(!frame.cell(point.1, point.0).unwrap().underline());
            let after = s.completed_cells_end();
            action(&mut s, mouse, HISTORY, run_label);
            owned(
                &mut s,
                after,
                HISTORY,
                &["No actual current POST200 returned-run proof"],
            );
            assert_eq!(commands(&s, &f).len(), confirmed);
            assert_eq!(metadata(&f), original);
        }
        let after = s.completed_cells_end();
        enabled(&mut s, "[View original]");
        action(&mut s, mouse, HISTORY, "[View original]");
        s.wait_for_command_count(&f, inspected);
        owned(
            &mut s,
            after,
            HISTORY,
            &[
                "Saved Dispatch #",
                "[F6 Acknowledge locally]",
                "Tab: buttons • Esc: close",
            ],
        );
        enabled(&mut s, "[View original]");
        if p.detailed() {
            visible_proof(&mut s, mouse, "Returned run #124");
        } else {
            visible_proof(
                &mut s,
                mouse,
                "Run ID not returned • acceptance only; no run or attempt invented",
            );
        }
        assert_eq!(metadata(&f), original);
        enabled(&mut s, "[F6 Acknowledge locally]");
        let after = s.completed_cells_end();
        action(&mut s, mouse, HISTORY, "[F6 Acknowledge locally]");
        s.wait_for_command_count(&f, expected.len());
        owned(
            &mut s,
            after,
            HISTORY,
            &["[F6 Acknowledge locally]", "Tab: buttons • Esc: close"],
        );
        enabled(&mut s, "[View original]");
        visible_proof(&mut s,mouse,"Explicit inspected Dispatch acceptance ACK • LOCAL ONLY • original receipt retained • no POST/retry");
        let acknowledged = metadata(&f);
        assert_eq!(acknowledged.0, "acknowledged");
        assert_eq!(
            (&acknowledged.1, &acknowledged.2, &acknowledged.3),
            (&original.1, &original.2, &original.3)
        );
        assert_eq!(unrelated(&f), unrelated_before);
        assert_eq!(git_state(&f), git);
        close(&mut s, mouse, HISTORY);
        assert_eq!(metadata(&f), acknowledged);
        assert_eq!(commands(&s, &f).len(), expected.len());
        assert_eq!(s.actual_terminal_witness_v4().unwrap(), tty);
        s.quit_and_assert_restored_terminal(mouse);
        exact(&s, &f, &expected, 1);
        assert_eq!(git_state(&f), git);
        assert_eq!(fs::read(&config_path).unwrap(), config_bytes);
        assert_private_intents_unchanged(&f);
        assert_read_only_git(&f);
    }
}

#[test]
fn scoped_actions_dispatch_v7_novice_usability_keyboard_original_result_proof_inspect_local_ack_no_resend(
) {
    journey(false);
}
#[test]
fn scoped_actions_dispatch_v7_novice_usability_mouse_original_result_proof_inspect_local_ack_no_resend(
) {
    journey(true);
}
