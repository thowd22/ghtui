//! One-step Metadata-tab controls: the `l Add label`-style key-bar tokens and
//! their shortcut keys open the SAME member workflow as the `p` menu, then
//! choose/Preview/Confirm publish exactly one change through strict fake gh.
use super::*;

/// (wide label, compact label below 100 columns, key)
fn quick(command: Cmd) -> (&'static str, &'static str, &'static [u8]) {
    match command {
        Cmd::AddLabel => ("[l Add label]", "[l +Label]", b"l"),
        Cmd::RemoveLabel => ("[L Remove label]", "[L -Label]", b"L"),
        Cmd::AddAssignee => ("[a Assign]", "[a +Assignee]", b"a"),
        Cmd::RemoveAssignee => ("[A Unassign]", "[A -Assignee]", b"A"),
        Cmd::AddReviewer => ("[w Request review]", "[w +Reviewer]", b"w"),
        Cmd::RemoveReviewer => ("[W Remove reviewer]", "[W -Reviewer]", b"W"),
        Cmd::AddTeam => ("[g Request team]", "[g +Team]", b"g"),
        Cmd::RemoveTeam => ("[G Remove team]", "[G -Team]", b"G"),
        Cmd::SetMilestone => ("[m Set milestone]", "[m Milestone]", b"m"),
        Cmd::ClearMilestone => ("[M Clear milestone]", "[M -Milestone]", b"M"),
    }
}

/// The additions are key-bar tokens (label chosen by the current width); the removals leave
/// the bar first on narrow rows but keep their keys and their `p` menu entries. The chosen
/// one opens the same member workflow the `p` menu opens.
fn open_quick(s: &mut Session, f: &Fixture, mouse: bool, command: Cmd, end: usize) {
    let frame = s.completed_cells_screen().unwrap();
    let wide = frame.size().1 >= 100;
    for every in [
        Cmd::AddLabel,
        Cmd::AddReviewer,
        Cmd::AddAssignee,
        Cmd::SetMilestone,
    ] {
        let (long, compact, _) = quick(every);
        let label = if wide { long } else { compact };
        assert!(
            crate::support::controls::control_painted(frame, label),
            "{label} missing\n{}",
            frame.contents()
        );
    }
    let after = s.completed_cells_end();
    let (long, compact, key) = quick(command);
    choose(s, mouse, if wide { long } else { compact }, key);
    s.wait_for_command_count(f, end);
    form_frame(
        s,
        after,
        &members_title(command),
        selection_controls(command),
    );
}

fn quick_journey(mouse: bool, command: Cmd) {
    let personal = false;
    let (mut steps, initial, section_end, opened, prepared) = prepared_steps(personal, command);
    steps.extend(selected_phase(command));
    steps.extend(metadata_post(command));
    let (f, mut s, git, config, stable) =
        begin_metadata(&steps, personal, mouse, initial, section_end);
    assert!(s
        .screen()
        .contains("You can change labels, assignees, reviewers and the milestone"));
    open_quick(&mut s, &f, mouse, command, opened);
    select(&mut s, &f, mouse, command, &steps[..opened]);
    prepare(&mut s, &f, mouse, command, prepared);
    let before = s.completed_cursorless_end();
    control(
        &mut s,
        mouse,
        &members_title(command),
        "[Confirm action]",
        PREVIEW_CONTROLS,
    );
    s.wait_for_command_count(&f, steps.len());
    metadata_frame(&mut s, before);
    receipt(&f, command);
    assert_eq!(
        commands(&s, &f)
            .iter()
            .filter(|row| row["stdin_hex"]
                .as_str()
                .is_some_and(|s| s.contains("6d75746174696f6e")))
            .count(),
        1,
        "exactly one GraphQL mutation"
    );
    s.capture_completed_terminal_v1("metadata-quick-v1-after-confirm");
    back_from_metadata(&mut s, mouse);
    finish(s, &f, mouse, &steps, &git, &config, &stable);
}

/// Esc closes the opened workflow without any write; the tab stays usable.
fn quick_cancel(mouse: bool) {
    let command = Cmd::AddLabel;
    let (steps, initial, section_end, opened, _) = prepared_steps(false, command);
    let steps = steps[..opened].to_vec();
    let (f, mut s, git, config, stable) =
        begin_metadata(&steps, false, mouse, initial, section_end);
    open_quick(&mut s, &f, mouse, command, opened);
    let before = s.completed_cursorless_end();
    if mouse {
        s.click_visible_action("[Esc Close]");
    } else {
        s.send_key(b"\x1b");
    }
    metadata_frame(&mut s, before);
    assert!(crate::support::controls::control_painted(
        s.completed_cells_screen().unwrap(),
        "[l Add label]"
    ));
    exact(&s, &f, &steps);
    assert_eq!(
        sql(&f)
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                .get::<_, usize>(0))
            .unwrap(),
        0
    );
    back_from_metadata(&mut s, mouse);
    finish(s, &f, mouse, &steps, &git, &config, &stable);
}

#[test]
fn v1_pr_metadata_quick_add_label_keyboard() {
    quick_journey(false, Cmd::AddLabel)
}
#[test]
fn v1_pr_metadata_quick_add_label_mouse() {
    quick_journey(true, Cmd::AddLabel)
}
#[test]
fn v1_pr_metadata_quick_request_review_keyboard() {
    quick_journey(false, Cmd::AddReviewer)
}
#[test]
fn v1_pr_metadata_quick_request_review_mouse() {
    quick_journey(true, Cmd::AddReviewer)
}
#[test]
fn v1_pr_metadata_quick_set_milestone_keyboard() {
    quick_journey(false, Cmd::SetMilestone)
}
#[test]
fn v1_pr_metadata_quick_set_milestone_mouse() {
    quick_journey(true, Cmd::SetMilestone)
}
#[test]
fn v1_pr_metadata_quick_cancel_keyboard() {
    quick_cancel(false)
}
#[test]
fn v1_pr_metadata_quick_cancel_mouse() {
    quick_cancel(true)
}

/// The PR actions menu jumps to its first/last entry (Home/End); the mouse
/// reaches the same rows with the visible paging controls. Read-only.
fn menu_ends(mouse: bool) {
    let (steps, initial, section_end, _, _) = prepared_steps(false, Cmd::AddLabel);
    let steps = steps[..section_end].to_vec();
    let (f, mut s, git, config, stable) =
        begin_metadata(&steps, false, mouse, initial, section_end);
    choose(&mut s, mouse, "[p PR actions]", b"p");
    s.wait_for_screen("> Add conversation comment");
    if mouse {
        s.click_visible_action("[PgDn More]");
    } else {
        s.send_key(b"\x1b[F");
    }
    s.try_wait_for_condition(
        |screen| {
            !screen.contains("> Add conversation comment")
                && crate::support::selected_pr_menu_row(screen).is_some()
        },
        "menu moved away from the first entry",
        Duration::from_secs(8),
    )
    .unwrap();
    if mouse {
        s.click_visible_action("[PgUp Previous]");
    } else {
        s.send_key(b"\x1b[H");
    }
    s.wait_for_screen("> Add conversation comment");
    s.send_key(b"\x1b");
    s.try_wait_for_condition(
        |screen| !screen.contains("[PgDn More]"),
        "menu closed",
        Duration::from_secs(8),
    )
    .unwrap();
    exact(&s, &f, &steps);
    back_from_metadata(&mut s, mouse);
    finish(s, &f, mouse, &steps, &git, &config, &stable);
}
#[test]
fn v1_pr_metadata_quick_menu_home_end_keyboard() {
    menu_ends(false)
}
#[test]
fn v1_pr_metadata_quick_menu_paging_mouse() {
    menu_ends(true)
}
