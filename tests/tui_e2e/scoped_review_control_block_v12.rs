//! Actual wrapped-modal framework journey, not an alias to a write scenario.
use super::*;
fn journey(mouse: bool) {
    let (fixture, h, mut steps) = fixture_and_steps(|_| {});
    steps.truncate(steps.len() - 17);
    steps.extend(source_open(
        &h,
        json!([row(&h, Effect::Summary, Failure::None, false)]),
    ));
    fixture.scripted_gh(&steps);
    foreign_draft(&fixture);
    let before_git = git_state(&fixture);
    let drafts = stable_drafts(&fixture);
    let configuration = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    files_selected(&mut session, &fixture, mouse);
    let before = session.completed_cursorless_end();
    choose(&mut session, mouse, "[F10 Reviews]", b"\x1b[21~");
    browser_ready(&mut session, before, "#810");
    let sql = sql_state(&fixture);
    let git = git_commands(&fixture);
    let count = commands(&session, &fixture).len();
    session.resize_owned_single_action_modal_v2(40, 14, "Scoped reviews", "[Esc Close]");
    choose(&mut session, false, "[F3 Edit pending]", b"\x1bOR");
    ready(&mut session, &["Resize to inspect reviews • disabled"]);
    assert_eq!(commands(&session, &fixture).len(), count);
    assert_eq!(sql_state(&fixture), sql);
    for (width, height) in [(40, 18), (80, 24), (120, 40)] {
        let before_resize = session.completed_cursorless_end();
        session.resize_owned_control_block_v3(
            width,
            height,
            "Scoped reviews",
            &REVIEW_CONTROLS_V12,
        );
        session.wait_for_new_cursorless_condition(
            before_resize,
            |frame| {
                let text = frame.contents();
                REVIEW_CONTROLS_V12
                    .iter()
                    .all(|control| text.contains(control))
                    && text.contains("#810")
                    && !text.contains("Resize to inspect reviews • disabled")
            },
            "All19 complete controls plus actual selectable review row in owned useful-size frame",
            Duration::from_secs(8),
        );
        session.capture_completed_terminal_v1("pr-reviews");
        if !mouse {
            session.focus_completed_owned_action_v1(
                "Scoped reviews",
                "[> Right]",
                crate::support::completed_navigation_controls_v1::FocusAttributeV1::Inverse,
                &REVIEW_CONTROLS_V12,
                &[],
            );
        }
        let before_move = session.completed_cursorless_end();
        if mouse {
            choose(&mut session, true, "[> Right]", b">");
        } else {
            session.send_key(b"\r");
        }
        session.wait_for_new_cursorless_condition(before_move,|frame|frame.contents().contains("Scoped reviews")
            &&!frame.contents().contains("#810")&&REVIEW_CONTROLS_V12.iter().all(|label|frame.contents().contains(label)),
            "Actual focused Enter/click scrolls the observed row horizontally without changing its source",Duration::from_secs(8));
        assert_eq!(
            commands(&session, &fixture).len(),
            count,
            "pure horizontal action cannot load or write"
        );
        assert_eq!(sql_state(&fixture), sql);
        assert_eq!(git_commands(&fixture), git);
        let before_move = session.completed_cursorless_end();
        choose(&mut session, mouse, "[< Left]", b"<");
        session.wait_for_new_cursorless_condition(
            before_move,
            |frame| {
                frame.contents().contains("Scoped reviews") && frame.contents().contains("#810")
            },
            "Pure opposite movement restores the actual same observed review row",
            Duration::from_secs(8),
        );
    }
    let before = session.completed_cursorless_end();
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_new_cursorless_condition(
        before,
        |frame| !frame.contents().contains("Scoped reviews") && frame.contents().contains("Inbox"),
        "Pure owned Close restores genuine Files without an extra provider call",
        Duration::from_secs(8),
    );
    exact_commands(&session, &fixture, &steps, 0);
    assert_eq!(sql_state(&fixture), sql);
    assert_eq!(stable_drafts(&fixture), drafts);
    assert_eq!(git_state(&fixture), before_git);
    assert_eq!(git_commands(&fixture), git);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        configuration
    );
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn test_framework_review_control_block_keyboard() {
    journey(false)
}
#[test]
fn test_framework_review_control_block_mouse() {
    journey(true)
}
