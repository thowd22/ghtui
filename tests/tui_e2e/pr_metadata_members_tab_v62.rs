//! Real binary Metadata -> all ten Members commands. Every named journey
//! executes both actual Personal and Catalogue entry, strict gh argv/stdin,
//! independently queried origin/receipt/context/draft SQL, Git and PTY cleanup.
use super::*;
#[path = "pr_metadata_members_negative_v71.rs"]
mod negative_v71;
#[path = "pr_metadata_quick_v1.rs"]
mod quick_v1;
fn context_four(value: &Value) -> Vec<GhStep> {
    vec![
        get(&format!("repos/{FIRST}/pulls/7"), value.clone()),
        get(&format!("repos/{FIRST}/issues/7"), issue(value)),
        get(&format!("repos/{FIRST}"), repo()),
        get(&format!("repos/{FIRST}/pulls/7"), value.clone()),
    ]
}
fn section() -> Vec<GhStep> {
    let mut calls = principal(42);
    calls.push(get(&format!("repos/{FIRST}"), repo()));
    calls.extend(context_four(&raw()));
    calls.push(get(&format!("repos/{FIRST}"), repo()));
    calls.extend(principal(42));
    assert_eq!(calls.len(), 10);
    calls
}
fn metadata_post(command: Cmd) -> Vec<GhStep> {
    let mut calls = post(command);
    calls.splice(17..17, context_four(&after(command)));
    assert_eq!(calls.len(), 22);
    calls
}
fn metadata_manual(command: Cmd) -> Vec<GhStep> {
    let mut calls = manual_phase(command);
    calls.splice(12..12, context_four(&after(command)));
    assert_eq!(calls.len(), 19);
    calls
}
fn sql(f: &Fixture) -> rusqlite::Connection {
    super::super::super::raw(f)
}
fn metadata_frame(s: &mut Session, before: usize) {
    s.wait_for_new_cursorless_condition(
        before,
        |frame| {
            let text = frame.contents();
            crate::support::main_footer_visible(frame)
                && crate::support::controls::control_painted(frame, "[u Metadata]")
                && text.contains(&format!("https://{HOST}/{FIRST}/pull/7"))
                && text.contains("Your repository permissions:")
                && !text.contains("[Confirm action]")
        },
        "NEW actual Metadata source/context/controls, not Summary fallback",
        Duration::from_secs(8),
    );
}
fn enter_metadata(s: &mut Session, f: &Fixture, mouse: bool, end: usize) {
    let original = s.completed_cells_screen().unwrap().size();
    let before = s.completed_cursorless_end();
    let ready = |frame: &vt100::Screen| {
        frame.size() == (40, 120)
            && crate::support::main_footer_visible(frame)
            && frame.contents().contains(&"a".repeat(40))
            && frame.contents().contains(&format!("{FIRST}#7"))
            && crate::support::controls::control_painted(frame, "[u Metadata]")
            && !frame.contents().contains("[Esc Cancel]")
            && !frame.contents().contains("[Confirm action]")
            && !frame.contents().contains("Verifying original Conversation")
    };
    if original != (40, 120) {
        s.resize(120, 40);
        s.wait_for_new_cursorless_condition(
            before,
            ready,
            "NEW full PR/head original at120 before one Metadata input",
            Duration::from_secs(8),
        );
    } else {
        // Existing launch is120x40 for keyboard. resize() intentionally emits
        // no event for unchanged geometry; begin/restart already witnessed its
        // genuine completed owned entry, which must still satisfy full facts.
        s.wait_for_cursorless_condition(
            ready,
            "Actual full PR/head original ready before one Metadata input",
            Duration::from_secs(8),
        );
    }
    let before = s.completed_cursorless_end();
    choose(s, mouse, "[u Metadata]", b"u");
    s.wait_for_command_count(f, end);
    metadata_frame(s, before);
    if original != (40, 120) {
        let before = s.completed_cursorless_end();
        s.resize(original.1, original.0);
        metadata_frame(s, before);
    }
}
fn begin_metadata(
    steps: &[GhStep],
    personal: bool,
    mouse: bool,
    initial: usize,
    section_end: usize,
) -> (Fixture, Session, GitState, Vec<u8>, Vec<String>) {
    let (f, mut s, git, config, stable) = begin(steps, personal, mouse, initial);
    enter_metadata(&mut s, &f, mouse, section_end);
    exact(&s, &f, &steps[..section_end]);
    assert_eq!(
        sql(&f)
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                .get::<_, usize>(0))
            .unwrap(),
        0
    );
    (f, s, git, config, stable)
}
fn prepared_steps(personal: bool, command: Cmd) -> (Vec<GhStep>, usize, usize, usize, usize) {
    let mut steps = initial_steps(personal);
    let initial = steps.len();
    steps.extend(section());
    let section_end = steps.len();
    steps.extend(phase(command, raw(), candidate(command), current()));
    let opened = steps.len();
    steps.extend(selected_phase(command));
    let prepared = steps.len();
    (steps, initial, section_end, opened, prepared)
}
fn preview_from_metadata(
    s: &mut Session,
    f: &Fixture,
    mouse: bool,
    command: Cmd,
    opened: usize,
    prepared: usize,
    steps: &[GhStep],
) {
    open_members(s, f, mouse, command, opened);
    select(s, f, mouse, command, &steps[..opened]);
    prepare(s, f, mouse, command, prepared);
}
fn back_from_metadata(s: &mut Session, mouse: bool) {
    let before = s.completed_cursorless_end();
    choose(s, mouse, "[b Back]", b"b");
    s.wait_for_new_cursorless_condition(
        before,
        |frame| {
            crate::support::main_footer_visible(frame)
                && !crate::support::controls::control_painted(frame, "[b Back]")
                && !frame.contents().contains("[Confirm action]")
        },
        "Pure actual same-family cached Back with no GH",
        Duration::from_secs(8),
    );
}
fn happy_metadata(mouse: bool, command: Cmd) {
    for personal in [false, true] {
        let (mut steps, initial, section_end, opened, prepared) = prepared_steps(personal, command);
        steps.extend(selected_phase(command));
        steps.extend(metadata_post(command));
        let (f, mut s, git, config, stable) =
            begin_metadata(&steps, personal, mouse, initial, section_end);
        preview_from_metadata(&mut s, &f, mouse, command, opened, prepared, &steps);
        if command == Cmd::AddLabel {
            resize_rejects_hidden_confirm_v191(&mut s, &f, mouse, &steps[..prepared]);
        }
        resize_members(&mut s, command, 120, 40, PREVIEW_CONTROLS);
        resize_members(&mut s, command, 80, 24, PREVIEW_CONTROLS);
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
            1
        );
        s.capture_completed_terminal_v1("metadata-members-v62-actual-fresh-Metadata");
        back_from_metadata(&mut s, mouse);
        finish(s, &f, mouse, &steps, &git, &config, &stable);
    }
}
macro_rules! happy_pair {
    ($key:ident,$mouse:ident,$command:ident) => {
        #[test]
        fn $key() {
            happy_metadata(false, Cmd::$command)
        }
        #[test]
        fn $mouse() {
            happy_metadata(true, Cmd::$command)
        }
    };
}
happy_pair!(
    pr_metadata_members_tab_v62_add_label_keyboard,
    pr_metadata_members_tab_v62_add_label_mouse,
    AddLabel
);
happy_pair!(
    pr_metadata_members_tab_v62_remove_label_keyboard,
    pr_metadata_members_tab_v62_remove_label_mouse,
    RemoveLabel
);
happy_pair!(
    pr_metadata_members_tab_v62_add_assignee_keyboard,
    pr_metadata_members_tab_v62_add_assignee_mouse,
    AddAssignee
);
happy_pair!(
    pr_metadata_members_tab_v62_remove_assignee_keyboard,
    pr_metadata_members_tab_v62_remove_assignee_mouse,
    RemoveAssignee
);
happy_pair!(
    pr_metadata_members_tab_v62_add_user_reviewer_keyboard,
    pr_metadata_members_tab_v62_add_user_reviewer_mouse,
    AddReviewer
);
happy_pair!(
    pr_metadata_members_tab_v62_remove_user_reviewer_keyboard,
    pr_metadata_members_tab_v62_remove_user_reviewer_mouse,
    RemoveReviewer
);
happy_pair!(
    pr_metadata_members_tab_v62_add_team_keyboard,
    pr_metadata_members_tab_v62_add_team_mouse,
    AddTeam
);
happy_pair!(
    pr_metadata_members_tab_v62_remove_team_keyboard,
    pr_metadata_members_tab_v62_remove_team_mouse,
    RemoveTeam
);
happy_pair!(
    pr_metadata_members_tab_v62_set_milestone_keyboard,
    pr_metadata_members_tab_v62_set_milestone_mouse,
    SetMilestone
);
happy_pair!(
    pr_metadata_members_tab_v62_clear_milestone_keyboard,
    pr_metadata_members_tab_v62_clear_milestone_mouse,
    ClearMilestone
);

fn failed_context_refresh(mouse: bool) {
    for personal in [false, true] {
        let command = Cmd::AddLabel;
        let (mut steps, initial, section_end, opened, prepared) = prepared_steps(personal, command);
        steps.extend(selected_phase(command));
        let mut paid = metadata_post(command);
        let mut changed = after(command);
        changed["body"] = json!("NEVER_ADOPT_CONTRADICTORY_METADATA_CONTEXT");
        paid[20] = get(&format!("repos/{FIRST}/pulls/7"), changed);
        paid.truncate(21);
        steps.extend(paid);
        let published = steps.len();
        steps.extend(metadata_manual(command));
        let (f, mut s, git, config, stable) =
            begin_metadata(&steps, personal, mouse, initial, section_end);
        preview_from_metadata(&mut s, &f, mouse, command, opened, prepared, &steps);
        let before = s.completed_cells_end();
        control(
            &mut s,
            mouse,
            &members_title(command),
            "[Confirm action]",
            PREVIEW_CONTROLS,
        );
        s.wait_for_command_count(&f, published);
        form_frame(
            &mut s,
            before,
            RETURN,
            &["DURABLY CONFIRMED", RETURN_CONTROLS[0], RETURN_CONTROLS[1]],
        );
        assert!(!s
            .screen()
            .contains("NEVER_ADOPT_CONTRADICTORY_METADATA_CONTEXT"));
        assert_eq!(state(&f), "confirmed");
        let origin_before = origin(&f);
        let draft_before = original_draft(&f);
        let marker_before: Value = serde_json::from_str(&super::super::super::marker(&f)).unwrap();
        exact(&s, &f, &steps[..published]);
        let before = s.completed_cursorless_end();
        control(&mut s, mouse, RETURN, RETURN_CONTROLS[0], RETURN_CONTROLS);
        s.wait_for_command_count(&f, steps.len());
        metadata_frame(&mut s, before);
        assert_eq!(origin(&f), origin_before);
        assert_eq!(original_draft(&f), draft_before);
        assert_eq!(
            serde_json::from_str::<Value>(&super::super::super::marker(&f)).unwrap(),
            marker_before
        );
        let stored: String = sql(&f)
            .query_row(
                "SELECT payload FROM records WHERE kind='pr-metadata-context-v48'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&stored).unwrap()["issue_metadata"],
            issue(&after(command))
        );
        assert_eq!(
            commands(&s, &f)
                .iter()
                .filter(|r| r["stdin_hex"]
                    .as_str()
                    .is_some_and(|s| s.contains("6d75746174696f6e")))
                .count(),
            1
        );
        back_from_metadata(&mut s, mouse);
        finish(s, &f, mouse, &steps, &git, &config, &stable);
    }
}
fn account_metadata(mouse: bool) {
    for personal in [false, true] {
        let command = Cmd::RemoveLabel;
        let mut steps = initial_steps(personal);
        let initial = steps.len();
        steps.extend(section());
        let section_end = steps.len();
        steps.extend(phase(command, raw(), candidate(command), current()));
        let opened = steps.len();
        steps.extend(principal(43));
        let (f, mut s, git, config, stable) =
            begin_metadata(&steps, personal, mouse, initial, section_end);
        open_members(&mut s, &f, mouse, command, opened);
        select(&mut s, &f, mouse, command, &steps[..opened]);
        let before_sql = all_sql(&f);
        let before = s.completed_cursorless_end();
        control(
            &mut s,
            mouse,
            &members_title(command),
            "[Preview]",
            SELECT_CONTROLS,
        );
        s.wait_for_command_count(&f, steps.len());
        s.wait_for_new_cursorless_condition(
            before,
            |frame| {
                crate::support::main_footer_visible(frame)
                    && !frame.contents().contains(&members_title(command))
                    && !frame.contents().contains(DETAIL_BODY)
                    && !frame.contents().contains("[Choose member]")
            },
            "Actual new numeric account hides exact Metadata Members private Form",
            Duration::from_secs(8),
        );
        no_attempt(&f);
        assert_eq!(all_sql(&f), before_sql);
        finish(s, &f, mouse, &steps, &git, &config, &stable);
    }
}
fn denied_metadata(mouse: bool) {
    for personal in [false, true] {
        let command = Cmd::AddReviewer;
        let mut steps = initial_steps(personal);
        let initial = steps.len();
        steps.extend(section());
        let section_end = steps.len();
        let mut calls = phase(command, raw(), candidate(command), current());
        for i in [2, 8] {
            let mut denied = repo();
            denied["permissions"] =
                json!({"pull":true,"push":false,"maintain":false,"admin":false});
            calls[i].stdout = serde_json::to_vec(&denied).unwrap();
        }
        steps.extend(calls);
        let (f, mut s, git, config, stable) =
            begin_metadata(&steps, personal, mouse, initial, section_end);
        open_members(&mut s, &f, mouse, command, steps.len());
        let before_sql = all_sql(&f);
        let before = s.completed_cells_end();
        control(
            &mut s,
            mouse,
            &members_title(command),
            "[Choose member]",
            SELECT_CONTROLS,
        );
        form_frame(
            &mut s,
            before,
            picker_title(command),
            &["new-reviewer", "[Esc Back]"],
        );
        let body = crate::support::overlay_body(
            s.completed_cells_screen().unwrap(),
            picker_title(command),
        )
        .unwrap();
        assert!(body.contains("mutation permission") || body.contains("browse only"));
        let before = s.completed_cells_end();
        picker_control(&mut s, mouse, command, "[Esc Back]");
        form_frame(&mut s, before, &members_title(command), SELECT_CONTROLS);
        no_attempt(&f);
        assert_eq!(all_sql(&f), before_sql);
        close_members(&mut s, mouse, command, false);
        back_from_metadata(&mut s, mouse);
        finish(s, &f, mouse, &steps, &git, &config, &stable);
    }
}
fn invalid_metadata(mouse: bool) {
    for personal in [false, true] {
        let command = Cmd::AddAssignee;
        let (steps, initial, section_end, opened, prepared) = prepared_steps(personal, command);
        let (f, mut s, git, config, stable) =
            begin_metadata(&steps, personal, mouse, initial, section_end);
        open_members(&mut s, &f, mouse, command, opened);
        let before_sql = all_sql(&f);
        let before = s.completed_cells_end();
        control(
            &mut s,
            mouse,
            &members_title(command),
            "[Preview]",
            SELECT_CONTROLS,
        );
        form_frame(
            &mut s,
            before,
            &members_title(command),
            &[
                "Choose an actual indexed member before Preview",
                "[Choose member]",
                "[Preview]",
            ],
        );
        assert_eq!(all_sql(&f), before_sql);
        no_attempt(&f);
        exact(&s, &f, &steps[..opened]);
        select(&mut s, &f, mouse, command, &steps[..opened]);
        prepare(&mut s, &f, mouse, command, prepared);
        let before_sql = all_sql(&f);
        resize_members(&mut s, command, 120, 40, PREVIEW_CONTROLS);
        resize_members(&mut s, command, 80, 24, PREVIEW_CONTROLS);
        close_members(&mut s, mouse, command, true);
        assert_eq!(all_sql(&f), before_sql);
        no_attempt(&f);
        back_from_metadata(&mut s, mouse);
        finish(s, &f, mouse, &steps, &git, &config, &stable);
    }
}
macro_rules! negative_pair {
    ($key:ident,$mouse:ident,$journey:ident) => {
        #[test]
        fn $key() {
            $journey(false)
        }
        #[test]
        fn $mouse() {
            $journey(true)
        }
    };
}
negative_pair!(
    pr_metadata_members_tab_v62_failed_context_new19_refresh_keyboard,
    pr_metadata_members_tab_v62_failed_context_new19_refresh_mouse,
    failed_context_refresh
);
negative_pair!(
    pr_metadata_members_tab_v62_numeric_account_loss_keyboard,
    pr_metadata_members_tab_v62_numeric_account_loss_mouse,
    account_metadata
);
negative_pair!(
    pr_metadata_members_tab_v62_current_denied_browse_only_keyboard,
    pr_metadata_members_tab_v62_current_denied_browse_only_mouse,
    denied_metadata
);
negative_pair!(
    pr_metadata_members_tab_v62_missing_selection_then_saved_preview_close_keyboard,
    pr_metadata_members_tab_v62_missing_selection_then_saved_preview_close_mouse,
    invalid_metadata
);

fn restart_metadata(
    f: &Fixture,
    mouse: bool,
    personal: bool,
    loaded: usize,
    section_end: usize,
    steps: &[GhStep],
) -> Session {
    let mut s = restart(f, mouse, personal, loaded, &steps[..loaded]);
    enter_metadata(&mut s, f, mouse, section_end);
    exact(&s, f, &steps[..section_end]);
    s
}
fn restart_ack_metadata(mouse: bool) {
    for personal in [false, true] {
        let command = Cmd::AddLabel;
        let (mut steps, initial, section_end, opened, prepared) = prepared_steps(personal, command);
        steps.extend(selected_phase(command));
        let mut failed = metadata_post(command);
        failed[1].status = 1;
        failed[1].stdout.clear();
        failed[1].stderr = b"Actual first readback unavailable; no receipt inferred".to_vec();
        failed.truncate(2);
        steps.extend(failed);
        let uncertain = steps.len();
        steps.extend(initial_steps(personal));
        let restarted = steps.len();
        steps.extend(section());
        let restarted_metadata = steps.len();
        steps.extend(history_phase(None));
        let listed = steps.len();
        steps.extend(history_phase(Some(command)));
        let inspected = steps.len();
        steps.extend(initial_steps(personal));
        let reopened = steps.len();
        steps.extend(section());
        let reopened_metadata = steps.len();
        steps.extend(phase(command, raw(), candidate(command), current()));
        let second_open = steps.len();
        steps.extend(selected_phase(command));
        let noticed = steps.len();
        steps.extend(selected_phase(command));
        let new_open = steps.len();
        steps.extend(selected_phase(command));
        let second_preview = steps.len();
        steps.extend(selected_phase(command));
        steps.extend(metadata_post(command));
        let (f, mut s, git, config, stable) =
            begin_metadata(&steps, personal, mouse, initial, section_end);
        preview_from_metadata(&mut s, &f, mouse, command, opened, prepared, &steps);
        let before = s.completed_cursorless_end();
        control(
            &mut s,
            mouse,
            &members_title(command),
            "[Confirm action]",
            PREVIEW_CONTROLS,
        );
        s.wait_for_command_count(&f, uncertain);
        terminal(&mut s, before, "Unknown outcome");
        assert_eq!(state(&f), "uncertain");
        let old_origin = origin(&f);
        let old_draft = original_draft(&f);
        exact(&s, &f, &steps[..uncertain]);
        s.quit_and_assert_restored_terminal(mouse);
        let mut s = restart_metadata(&f, mouse, personal, restarted, restarted_metadata, &steps);
        open_history(&mut s, &f, mouse, listed);
        inspect(&mut s, &f, mouse, inspected);
        let expected = expected_ack(&all_sql(&f));
        let before = s.completed_cursorless_end();
        control(
            &mut s,
            mouse,
            &history_title(),
            "[Acknowledge local]",
            HISTORY_FOOTER,
        );
        terminal(&mut s, before, "Local ACK");
        assert_eq!(all_sql(&f), expected);
        assert_eq!(origin(&f), old_origin);
        assert_eq!(original_draft(&f), old_draft);
        exact(&s, &f, &steps[..inspected]);
        s.quit_and_assert_restored_terminal(mouse);
        let mut s = restart_metadata(&f, mouse, personal, reopened, reopened_metadata, &steps);
        open_members(&mut s, &f, mouse, command, second_open);
        select_with_expected(&mut s, &f, mouse, command, &steps[..second_open], 1, 1);
        let before_drafts = all_sql(&f).get("drafts").cloned();
        let before = s.completed_cells_end();
        control(
            &mut s,
            mouse,
            &members_title(command),
            "[Preview]",
            SELECT_CONTROLS,
        );
        wait_restart_commands(&mut s, &f, noticed, "terminal notice Preview");
        form_frame(
            &mut s,
            before,
            &members_title(command),
            &["NEEDS NEW ACTION", "[n New action]", "[Inspect history]"],
        );
        assert_eq!(all_sql(&f).get("drafts").cloned(), before_drafts);
        assert_eq!(origin(&f), old_origin);
        assert_eq!(original_draft(&f), old_draft);
        let before = s.completed_cells_end();
        control(
            &mut s,
            mouse,
            &members_title(command),
            "[n New action]",
            NEW_CONTROLS,
        );
        wait_restart_commands(&mut s, &f, new_open, "explicit New action");
        form_frame(&mut s, before, &members_title(command), SELECT_CONTROLS);
        prepare_with_expected(&mut s, &f, mouse, command, second_preview, 1);
        let before = s.completed_cursorless_end();
        control(
            &mut s,
            mouse,
            &members_title(command),
            "[Confirm action]",
            PREVIEW_CONTROLS,
        );
        wait_restart_commands(&mut s, &f, steps.len(), "new lineage Confirm");
        metadata_frame(&mut s, before);
        assert_eq!(origin(&f), old_origin);
        assert_eq!(original_draft(&f), old_draft);
        let rows=sql(&f).prepare("SELECT a.state,a.selected_resource,o.base_key_v48,o.parent_ack_attempt_v48 FROM pr_write_attempts a JOIN pr_write_metadata_origins_v25 o USING(attempt_id) ORDER BY a.attempt_id").unwrap().query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,Option<i64>>(3)?))).unwrap().map(Result::unwrap).collect::<Vec<_>>();
        assert_eq!(rows.len(), 2);
        assert_eq!(
            (&rows[0].0, &rows[1].0),
            (&"acknowledged".to_owned(), &"confirmed".to_owned())
        );
        assert_eq!(rows[0].2, rows[1].2);
        assert_ne!(rows[0].1, rows[1].1);
        assert_eq!(rows[1].3, Some(1));
        assert_eq!(
            commands(&s, &f)
                .iter()
                .filter(|r| r["stdin_hex"]
                    .as_str()
                    .is_some_and(|s| s.contains("6d75746174696f6e")))
                .count(),
            2
        );
        back_from_metadata(&mut s, mouse);
        finish(s, &f, mouse, &steps, &git, &config, &stable);
    }
}
fn wait_restart_commands(s: &mut Session, f: &Fixture, count: usize, phase: &str) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        let journal = s.inspect_command_journal(f);
        if journal.lines().count() >= count {
            return;
        }
        if Instant::now() >= until {
            s.capture_completed_terminal_v1("members-restart-v235-command-boundary-failure");
            panic!(
                "Actual {phase}: expected {count} commands, got {}\n{}\n{}",
                journal.lines().count(),
                journal.lines().rev().take(4).collect::<Vec<_>>().join("\n"),
                s.screen()
            );
        }
        s.poll_output(Duration::from_millis(2));
    }
}
fn cancel_metadata(mouse: bool) {
    for personal in [false, true] {
        let command = Cmd::AddLabel;
        let (mut steps, initial, section_end, opened, prepared) = prepared_steps(personal, command);
        steps.extend(selected_phase(command));
        let paid = metadata_post(command);
        steps.push(paid[0].clone());
        let held_index = steps.len();
        let mut held = paid[1].clone();
        held.delay_ms = 30_000;
        steps.push(held);
        let (f, mut s, git, config, stable) =
            begin_metadata(&steps, personal, mouse, initial, section_end);
        preview_from_metadata(&mut s, &f, mouse, command, opened, prepared, &steps);
        control(
            &mut s,
            mouse,
            &members_title(command),
            "[Confirm action]",
            PREVIEW_CONTROLS,
        );
        s.wait_for_command_count(&f, steps.len());
        let log = commands(&s, &f);
        let pid = i32::try_from(log[held_index]["pid"].as_u64().unwrap()).unwrap();
        let original = process_identity(pid).unwrap().unwrap();
        let owned =
            OwnedPtySession::capture(u32::try_from(s.owned_application_pid()).unwrap()).unwrap();
        assert!(owned.owns(&original));
        assert!(!original.zombie);
        let old_origin = origin(&f);
        let old_draft = original_draft(&f);
        let before = all_sql(&f);
        let expected = {
            let mut expected = before.clone();
            let (columns, rows) = expected.get_mut("pr_write_attempts").unwrap();
            let state = columns.iter().position(|s| s == "state").unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(
                rows[0][state],
                rusqlite::types::Value::Text("pending".into())
            );
            rows[0][state] = rusqlite::types::Value::Text("uncertain".into());
            // mark_uncertain is a durable terminal transition and advances
            // precisely this original PR journal scope once in the same tx.
            let (columns, rows) = expected.get_mut("pr_write_scopes").unwrap();
            let generation = columns.iter().position(|s| s == "generation").unwrap();
            let principal = columns.iter().position(|s| s == "principal").unwrap();
            let node = columns.iter().position(|s| s == "pr_node").unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(
                rows[0][principal],
                rusqlite::types::Value::Text("user-id:42".into())
            );
            assert_eq!(
                rows[0][node],
                rusqlite::types::Value::Text("GENUINE_CANONICAL_PR_NODE_1007".into())
            );
            assert_eq!(rows[0][generation], rusqlite::types::Value::Integer(2));
            rows[0][generation] = rusqlite::types::Value::Integer(3);
            expected
        };
        // Busy Form has only EscCancel; keyboard Esc is the real shortcut and
        // never asks a footer-only Tab helper to cross an editor field.
        if mouse {
            control(
                &mut s,
                true,
                &members_title(command),
                "[Esc Cancel]",
                &["[Esc Cancel]"],
            );
        } else {
            s.wait_for_owned_completed_action(&members_title(command), "[Esc Cancel]", &[]);
            s.send_key(b"\x1b");
        }
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            s.poll_output(Duration::from_millis(2));
            let gone = match process_identity(pid) {
                Ok(None) => true,
                Ok(Some(now)) => {
                    // An exited child is briefly a zombie until the app reaps
                    // it; only one that outlives the deadline is a leak.
                    assert!(
                        !now.zombie || Instant::now() < until,
                        "Canceled exact child must be reaped"
                    );
                    assert_eq!(
                        (now.uid, now.birth, now.session),
                        (original.uid, original.birth, original.session)
                    );
                    false
                }
                Err(e) => panic!("Exact owned child observation: {e}"),
            };
            if gone && state(&f) == "uncertain" {
                break;
            }
            assert!(
                Instant::now() < until,
                "Exact Metadata Members child cancellation and durable uncertainty"
            );
        }
        assert_eq!(all_sql(&f), expected);
        assert_eq!(origin(&f), old_origin);
        assert_eq!(original_draft(&f), old_draft);
        s.wait_for_cursorless_condition(
            |frame| {
                crate::support::main_footer_visible(frame)
                    && !frame.contents().contains(&members_title(command))
                    && !frame.contents().contains(DETAIL_BODY)
            },
            "Cancelled exact private Metadata underlay retired",
            Duration::from_secs(8),
        );
        finish(s, &f, mouse, &steps, &git, &config, &stable);
    }
}
negative_pair!(
    pr_metadata_members_tab_v62_restart_history_ack_new_fresh_metadata_keyboard,
    pr_metadata_members_tab_v62_restart_history_ack_new_fresh_metadata_mouse,
    restart_ack_metadata
);
negative_pair!(
    pr_metadata_members_tab_v62_held_readback_cancel_exact_child_keyboard,
    pr_metadata_members_tab_v62_held_readback_cancel_exact_child_mouse,
    cancel_metadata
);
