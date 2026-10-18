//! "Comment line" straight from the PR Files diff: select a diff line, press
//! `i` (or click it), extend the range in the opened images, then `P` alone
//! verifies the range and opens the same inline composer. Strict offline gh.
use super::*;

fn selected_row_contains(screen: &str, text: &str) -> bool {
    screen
        .lines()
        .any(|line| line.starts_with("│>") && line.contains(text))
}

/// Keyboard: move the Files cursor down until the given diff row is selected.
fn select_by_keyboard(session: &mut Session, text: &str) {
    for _ in 0..24 {
        if session
            .try_wait_for_condition(
                |screen| selected_row_contains(screen, text),
                "selected diff row",
                Duration::from_millis(300),
            )
            .is_ok()
        {
            return;
        }
        session.send_key(b"\x1b[B");
    }
    panic!("Diff row {text:?} never selected\n{}", session.screen());
}

fn select_by_mouse(session: &mut Session, text: &str) {
    session.wait_for_screen(text);
    let (column, row) = session.locate(text).unwrap();
    session.click(column, row);
    session
        .try_wait_for_condition(
            |screen| selected_row_contains(screen, text),
            "clicked diff row selected",
            Duration::from_secs(5),
        )
        .unwrap();
}

fn line_journey(mouse: bool) {
    // Keyboard comments RIGHT from the added line; mouse LEFT from the removed one.
    let side = if mouse { "LEFT" } else { "RIGHT" };
    let (fixture, h, mut steps) = fixture_and_steps(|_| {});
    let fresh_images = steps[steps.len() - 17..].to_vec();
    steps.extend(source_phase(&h, true));
    steps.extend(fresh_images);
    let mut confirm = confirm_steps(&h, side);
    confirm[7].delay_ms = 0;
    steps.extend(confirm);
    fixture.scripted_gh(&steps);
    let before_git = git_state(&fixture);
    let before_drafts = stable_drafts(&fixture);
    let mut session = launch(&fixture, mouse);
    files_selected(&mut session, &fixture, mouse);
    ready(&mut session, &["[i Comment line]"]);
    // A file row is not a line: the control explains instead of guessing.
    choose(&mut session, mouse, "[i Comment line]", b"i");
    ready(
        &mut session,
        &["Select a changed or context line in the diff"],
    );
    assert_eq!(commands(&session, &fixture).len(), 41);
    let row = if mouse {
        "-old-no-newline"
    } else {
        "+new-no-newline"
    };
    if mouse {
        select_by_mouse(&mut session, row);
    } else {
        select_by_keyboard(&mut session, row);
    }
    choose(&mut session, mouse, "[i Comment line]", b"i");
    session.wait_for_command_count(&fixture, 58);
    overlay_ready(
        &mut session,
        "Immutable file images",
        &[&format!("P Comment on {side} line 2"), "[P Comment]"],
    );
    if mouse {
        let last = session.locate("L      2").unwrap();
        let first = session.locate("L      1").unwrap();
        session.drag(last, first);
        ready(&mut session, &["Selection LEFT 1–2"]);
    } else {
        session.send_key(b"\x1b[1;2A");
        ready(&mut session, &["Selection RIGHT 1–2"]);
    }
    // P verifies the extended selection itself; no separate R is required.
    choose(&mut session, mouse, "[P Comment]", b"P");
    session.wait_for_command_count(&fixture, 66);
    editing_ready(&mut session, "");
    if mouse {
        session.click_owned_form_field("PR INTENT • Preview first", "Body");
    }
    session.paste(BODY);
    editing_ready(&mut session, BODY);
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_command_count(&fixture, 83);
    ready(
        &mut session,
        &[
            "READ-ONLY PREVIEW • Confirm action",
            &format!("{side} lines 1–2"),
            "[Confirm action]",
        ],
    );
    let before = session.completed_cursorless_end();
    crate::local::confirm(&mut session, mouse, 1);
    session.wait_for_command_count(&fixture, steps.len());
    session.wait_for_new_cursorless_condition(
        before,
        |frame| frame.contents().contains("Comment posted"),
        "Inline comment posted from the diff line",
        Duration::from_secs(8),
    );
    let state: (String, String) = raw(&fixture)
        .query_row("SELECT command,state FROM pr_write_attempts", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap();
    assert_eq!(state, ("inline-comment".into(), "confirmed".into()));
    assert_eq!(stable_drafts(&fixture), before_drafts);
    assert_eq!(git_state(&fixture), before_git);
    exact_commands(&session, &fixture, &steps, 1);
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn v1_pr_threads_keyboard_diff_line_i_extend_p_posts_right_range_once() {
    line_journey(false)
}

#[test]
fn v1_pr_threads_mouse_diff_line_click_drag_p_posts_left_range_once() {
    line_journey(true)
}
