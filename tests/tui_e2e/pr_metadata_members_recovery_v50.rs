//! Real restart/ACK/New and Confirmed-only read recovery; no saved origin is
//! used as a source, permission or receipt factory. Every child is explicit.
use super::*;
#[path = "pr_metadata_members_tab_v62.rs"]
mod metadata_tab_v62;
const RETURN: &str = "Confirmed Conversation • read-only refresh";
const RETURN_CONTROLS: &[&str] = &["[r Refresh PR]", "[Esc Back to Inbox]"];
const NEW_CONTROLS: &[&str] = &["[n New action]", "[Inspect history]", "[Esc Close]"];
fn history_title() -> String {
    format!("Original member metadata history • {FIRST}#7")
}
fn history_phase(command: Option<Cmd>) -> Vec<GhStep> {
    let mut calls = principal(42);
    calls.push(get(&format!("repos/{FIRST}"), repo()));
    calls.push(get(&format!("repos/{FIRST}/pulls/7"), raw()));
    if let Some(command) = command {
        calls.push(get(&format!("repos/{FIRST}/issues/7"), issue(&raw())));
        calls.push(graphql(command, current(), false));
    }
    calls.push(get(&format!("repos/{FIRST}/pulls/7"), raw()));
    calls.push(get(&format!("repos/{FIRST}"), repo()));
    calls.extend(principal(42));
    assert_eq!(calls.len(), if command.is_some() { 10 } else { 8 });
    calls
}
fn manual_phase(command: Cmd) -> Vec<GhStep> {
    let mut calls = principal(42);
    calls.push(get(&format!("repos/{FIRST}"), repo()));
    calls.push(get(&format!("repos/{FIRST}/pulls/7"), after(command)));
    calls.push(get(
        &format!("repos/{FIRST}/issues/7"),
        issue(&after(command)),
    ));
    calls.push(graphql(command, current_after(command), false));
    calls.extend(post(command)[11..16].iter().cloned());
    calls.push(get(&format!("repos/{FIRST}/pulls/7"), after(command)));
    calls.push(get(&format!("repos/{FIRST}"), repo()));
    calls.extend(principal(42));
    assert_eq!(
        calls.len(),
        15,
        "SAME Core6 + raw/issue/GQL/five lists/closing raw9"
    );
    calls
}
fn origin(f: &Fixture) -> String {
    super::super::raw(f).query_row("SELECT json_array(attempt_id,repository_id,pr_id,origin_json,base_key_v48,action_tag_v48,parent_ack_attempt_v48) FROM pr_write_metadata_origins_v25 WHERE attempt_id=1",[],|row|row.get(0)).unwrap()
}
fn original_draft(f: &Fixture) -> (String, String) {
    super::super::raw(f).query_row("SELECT resource,body FROM drafts WHERE resource=(SELECT selected_resource FROM pr_write_attempts WHERE attempt_id=1)",[],|row|Ok((row.get(0)?,row.get(1)?))).unwrap()
}
fn state(f: &Fixture) -> String {
    super::super::raw(f)
        .query_row(
            "SELECT state FROM pr_write_attempts WHERE attempt_id=1",
            [],
            |row| row.get(0),
        )
        .unwrap()
}
fn restart(f: &Fixture, mouse: bool, personal: bool, end: usize, expected: &[GhStep]) -> Session {
    let mut s = launch(f, mouse);
    enter(&mut s, f, personal, mouse, end, false, expected);
    if personal {
        let frame = s.completed_cursorless_end();
        choose(&mut s, mouse, "[c Conversation]", b"c");
        s.wait_for_new_cursorless_condition(
            frame,
            |screen| {
                // The selected tab is `c Conversation` (`c Conv` below 100 columns); the
                // page is the Conversation body, which starts with the PR description.
                crate::support::main_footer_visible(screen)
                    && crate::support::controls::control_painted(screen, "[c Conversation]")
                    && screen.contents().contains("PR description")
            },
            "Restart genuinely entered original Conversation",
            Duration::from_secs(8),
        );
    }
    s
}
fn open_history(s: &mut Session, f: &Fixture, mouse: bool, end: usize) {
    let frame = s.completed_cells_end();
    crate::pr_actions::menu(s, mouse, "Inspect member metadata history");
    wait_history_commands_v239(s, f, end, "List exact protected history");
    form_frame(
        s,
        frame,
        &history_title(),
        &[
            "ORIGINAL MEMBER METADATA HISTORY",
            "[Inspect current]",
            "[Acknowledge local]",
        ],
    );
}
fn inspect(s: &mut Session, f: &Fixture, mouse: bool, end: usize) {
    let frame = s.completed_cells_end();
    control(
        s,
        mouse,
        &history_title(),
        "[Inspect current]",
        HISTORY_FOOTER,
    );
    wait_history_commands_v239(s, f, end, "Inspect exact protected history");
    form_frame(
        s,
        frame,
        &history_title(),
        &["Fresh actual", "[Acknowledge local]", "[Esc Close]"],
    );
    s.capture_completed_terminal_v1("metadata-members-fresh-protected-history-inspection");
}
fn wait_history_commands_v239(s: &mut Session, f: &Fixture, end: usize, phase: &str) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        let journal = s.inspect_command_journal(f);
        if journal.lines().count() >= end {
            return;
        }
        if Instant::now() >= until {
            s.capture_completed_terminal_v1("members-history-v239-command-boundary-failure");
            panic!(
                "Actual {phase}: expected {end} commands, got {}\n{}\n{}",
                journal.lines().count(),
                journal.lines().rev().take(4).collect::<Vec<_>>().join("\n"),
                s.screen()
            );
        }
        s.poll_output(Duration::from_millis(2));
    }
}
// A typed full raw snapshot retains column names, SQL NULL and byte types.
// Expected ACK changes are derived BEFORE the action, never poststate-adopted.
type RawSql = std::collections::BTreeMap<String, (Vec<String>, Vec<Vec<rusqlite::types::Value>>)>;
fn all_sql(f: &Fixture) -> RawSql {
    let db = super::super::raw(f);
    let names=db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap().query_map([],|r|r.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>();
    let mut result = RawSql::new();
    for table in names {
        let mut statement = db
            .prepare(&format!("SELECT * FROM \"{}\"", table.replace('"', "\"\"")))
            .unwrap();
        let columns = statement
            .column_names()
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        let mut rows = statement
            .query_map([], |r| {
                (0..columns.len())
                    .map(|i| r.get(i))
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        rows.sort_by_key(|row| format!("{row:?}"));
        result.insert(table, (columns, rows));
    }
    result
}
fn expected_ack(before: &RawSql) -> RawSql {
    use rusqlite::types::Value as Sql;
    let mut expected = before.clone();
    let (columns, rows) = expected.get_mut("pr_write_attempts").unwrap();
    let state = columns.iter().position(|s| s == "state").unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0][state], Sql::Text("uncertain".into()));
    rows[0][state] = Sql::Text("acknowledged".into());
    let (columns, rows) = expected.get_mut("pr_write_scopes").unwrap();
    let generation = columns.iter().position(|s| s == "generation").unwrap();
    let node = columns.iter().position(|s| s == "pr_node").unwrap();
    let mut changed = 0;
    for row in rows {
        if row[node] == Sql::Text("GENUINE_CANONICAL_PR_NODE_1007".into()) {
            let Sql::Integer(n) = &mut row[generation] else {
                panic!("Actual integer generation required")
            };
            *n += 1;
            changed += 1;
        }
    }
    assert_eq!(changed, 1);
    expected
}
fn uncertain_steps(personal: bool, command: Cmd) -> (Vec<GhStep>, usize, usize, usize) {
    let mut steps = initial_steps(personal);
    let initial = steps.len();
    steps.extend(phase(command, raw(), candidate(command), current()));
    let opened = steps.len();
    steps.extend(selected_phase(command));
    let prepared = steps.len();
    steps.extend(selected_phase(command));
    let mut paid = post(command);
    paid[1].status = 1;
    paid[1].stdout.clear();
    paid[1].stderr = b"actual mutation readback unavailable; never infer receipt".to_vec();
    paid.truncate(2);
    steps.extend(paid);
    (steps, initial, opened, prepared)
}
fn invoke_uncertain(
    s: &mut Session,
    f: &Fixture,
    mouse: bool,
    command: Cmd,
    opened: usize,
    prepared: usize,
    steps: &[GhStep],
) {
    let end = prepared + selected_phase(command).len() + 2;
    open_members(s, f, mouse, command, opened);
    select(s, f, mouse, command, &steps[..opened]);
    prepare(s, f, mouse, command, prepared);
    let before = s.completed_cursorless_end();
    control(
        s,
        mouse,
        &members_title(command),
        "[Confirm action]",
        PREVIEW_CONTROLS,
    );
    s.wait_for_command_count(f, end);
    terminal(s, before, "Unknown outcome");
    assert_eq!(state(f), "uncertain");
    let (_, body) = original_draft(f);
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["values"]["body"], selected_value(command));
    assert!(body["publication"].is_object());
    assert!(body.get("approval").is_none());
    exact(s, f, &steps[..end]);
}
fn restart_ack_new(mouse: bool) {
    restart_ack_new_chain_v57(mouse, false)
}
fn restart_ack_new_chain_v57(mouse: bool, third: bool) {
    for personal in [false, true] {
        let command = Cmd::AddLabel;
        let (mut steps, initial, opened, prepared) = uncertain_steps(personal, command);
        steps.extend(initial_steps(personal));
        let restarted = steps.len();
        steps.extend(history_phase(None));
        let listed = steps.len();
        steps.extend(history_phase(Some(command)));
        let inspected = steps.len();
        steps.extend(initial_steps(personal));
        let reopened = steps.len();
        steps.extend(phase(command, raw(), candidate(command), current()));
        let second_open = steps.len();
        steps.extend(selected_phase(command));
        let noticed = steps.len();
        steps.extend(selected_phase(command));
        let new_open = steps.len();
        steps.extend(selected_phase(command));
        let second_preview = steps.len();
        steps.extend(selected_phase(command));
        steps.extend(post(command));
        let second_confirmed = steps.len();
        let third_stages = if third {
            // A genuine restarted current read observes the selected label
            // absent again. It is current input, not replay of saved rows.
            steps.extend(initial_steps(personal));
            let restarted = steps.len();
            steps.extend(phase(command, raw(), candidate(command), current()));
            let opened = steps.len();
            steps.extend(selected_phase(command));
            let noticed = steps.len();
            steps.extend(selected_phase(command));
            let new_opened = steps.len();
            steps.extend(selected_phase(command));
            let previewed = steps.len();
            steps.extend(selected_phase(command));
            steps.extend(post(command));
            Some((restarted, opened, noticed, new_opened, previewed))
        } else {
            None
        };
        let (f, mut s, git, config, stable) = begin(&steps, personal, mouse, initial);
        invoke_uncertain(&mut s, &f, mouse, command, opened, prepared, &steps);
        let old_body = original_draft(&f);
        let old_origin = origin(&f);
        s.quit_and_assert_restored_terminal(mouse);
        let mut s = restart(&f, mouse, personal, restarted, &steps[..restarted]);
        open_history(&mut s, &f, mouse, listed);
        assert_eq!(original_draft(&f), old_body);
        assert_eq!(origin(&f), old_origin);
        inspect(&mut s, &f, mouse, inspected);
        let expected = expected_ack(&all_sql(&f));
        let frame = s.completed_cursorless_end();
        control(
            &mut s,
            mouse,
            &history_title(),
            "[Acknowledge local]",
            HISTORY_FOOTER,
        );
        terminal(&mut s, frame, "Local ACK");
        assert_eq!(
            all_sql(&f),
            expected,
            "Only exact ACK state plus one actual generation may change"
        );
        assert_eq!(original_draft(&f), old_body);
        assert_eq!(origin(&f), old_origin);
        exact(&s, &f, &steps[..inspected]);
        s.quit_and_assert_restored_terminal(mouse);
        let mut s = restart(&f, mouse, personal, reopened, &steps[..reopened]);
        open_members(&mut s, &f, mouse, command, second_open);
        select_with_expected(&mut s, &f, mouse, command, &steps[..second_open], 1, 1);
        let unchanged = all_sql(&f);
        let frame = s.completed_cells_end();
        control(
            &mut s,
            mouse,
            &members_title(command),
            "[Preview]",
            SELECT_CONTROLS,
        );
        s.wait_for_command_count(&f, noticed);
        form_frame(
            &mut s,
            frame,
            &members_title(command),
            &["NEEDS NEW ACTION", "[n New action]", "[Inspect history]"],
        );
        // Personal's fresh Core may reserve its same actual row; the old
        // protected facts are exact, and no new draft/attempt was written.
        assert_eq!(original_draft(&f), old_body);
        assert_eq!(origin(&f), old_origin);
        let db = super::super::raw(&f);
        assert_eq!(
            db.query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                .get::<_, usize>(0))
                .unwrap(),
            1
        );
        assert_eq!(all_sql(&f).get("drafts"), unchanged.get("drafts"));
        let frame = s.completed_cells_end();
        control(
            &mut s,
            mouse,
            &members_title(command),
            "[n New action]",
            NEW_CONTROLS,
        );
        s.wait_for_command_count(&f, new_open);
        form_frame(&mut s, frame, &members_title(command), SELECT_CONTROLS);
        assert_eq!(original_draft(&f), old_body);
        assert_eq!(origin(&f), old_origin);
        prepare_with_expected(&mut s, &f, mouse, command, second_preview, 1);
        control(
            &mut s,
            mouse,
            &members_title(command),
            "[Confirm action]",
            PREVIEW_CONTROLS,
        );
        s.wait_for_command_count(&f, second_confirmed);
        refreshed_v19::wait_fresh_page_v19(&mut s);
        assert_eq!(original_draft(&f), old_body);
        assert_eq!(origin(&f), old_origin);
        let rows=db.prepare("SELECT a.state,a.selected_resource,o.base_key_v48,o.parent_ack_attempt_v48 FROM pr_write_attempts a JOIN pr_write_metadata_origins_v25 o USING(attempt_id) ORDER BY a.attempt_id").unwrap().query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,Option<i64>>(3)?))).unwrap().map(Result::unwrap).collect::<Vec<_>>();
        assert_eq!(rows.len(), 2);
        assert_eq!(
            (&rows[0].0, &rows[1].0),
            (&"acknowledged".to_owned(), &"confirmed".to_owned())
        );
        assert_eq!(rows[0].2, rows[1].2);
        assert_ne!(rows[0].1, rows[1].1);
        assert_eq!(rows[1].3, Some(1));
        exact(&s, &f, &steps[..second_confirmed]);
        if let Some((restarted, opened, noticed, new_opened, previewed)) = third_stages {
            let second_raw: String = db.query_row("SELECT json_array(a.attempt_id,a.host,a.principal,a.pr_node,a.repository,a.pr_number,a.command,a.selected_resource,a.observed_head,a.payload_sha256,a.state,a.confirmed_json,a.created_at,o.repository_id,o.pr_id,o.origin_json,o.base_key_v48,o.action_tag_v48,o.parent_ack_attempt_v48) FROM pr_write_attempts a JOIN pr_write_metadata_origins_v25 o USING(attempt_id) WHERE a.attempt_id=2",[],|r|r.get(0)).unwrap();
            let reference = db.query_row("SELECT attempt_id,pr_node,observed_head,payload_sha256 FROM pr_write_attempts WHERE attempt_id=2",[],|r|Ok(ghtui::pr_write_journal::PrPublicationReference {attempt_id:r.get(0)?,pr_node_id:r.get(1)?,observed_head:r.get(2)?,payload_sha256:r.get(3)?})).unwrap();
            use sha2::{Digest, Sha256};
            let expected_key = format!(
                "metadata-members-v48/{:x}",
                Sha256::digest(
                    serde_json::to_vec(&(
                        "ghtui-explicit-new-metadata-members-v48",
                        &rows[1].2,
                        command.key(),
                        &reference
                    ))
                    .unwrap()
                )
            );
            assert_ne!(expected_key, rows[0].1);
            assert_ne!(expected_key, rows[1].1);
            s.quit_and_assert_restored_terminal(mouse);
            s = restart(&f, mouse, personal, restarted, &steps[..restarted]);
            open_members(&mut s, &f, mouse, command, opened);
            select_with_expected(&mut s, &f, mouse, command, &steps[..opened], 2, 1);
            let before_new = all_sql(&f);
            let frame = s.completed_cells_end();
            control(
                &mut s,
                mouse,
                &members_title(command),
                "[Preview]",
                SELECT_CONTROLS,
            );
            s.wait_for_command_count(&f, noticed);
            form_frame(
                &mut s,
                frame,
                &members_title(command),
                &[
                    "NEEDS NEW ACTION",
                    "Confirmed original receipt",
                    "[n New action]",
                    "[Inspect history]",
                ],
            );
            assert_eq!(all_sql(&f).get("drafts"), before_new.get("drafts"));
            assert_eq!(original_draft(&f), old_body);
            assert_eq!(origin(&f), old_origin);
            let frame = s.completed_cells_end();
            control(
                &mut s,
                mouse,
                &members_title(command),
                "[n New action]",
                NEW_CONTROLS,
            );
            s.wait_for_command_count(&f, new_opened);
            form_frame(&mut s, frame, &members_title(command), SELECT_CONTROLS);
            assert_eq!(original_draft(&f), old_body);
            assert_eq!(origin(&f), old_origin);
            prepare_with_expected(&mut s, &f, mouse, command, previewed, 2);
            control(
                &mut s,
                mouse,
                &members_title(command),
                "[Confirm action]",
                PREVIEW_CONTROLS,
            );
            s.wait_for_command_count(&f, steps.len());
            refreshed_v19::wait_fresh_page_v19(&mut s);
            assert_eq!(original_draft(&f), old_body);
            assert_eq!(origin(&f), old_origin);
            assert_eq!(db.query_row("SELECT json_array(a.attempt_id,a.host,a.principal,a.pr_node,a.repository,a.pr_number,a.command,a.selected_resource,a.observed_head,a.payload_sha256,a.state,a.confirmed_json,a.created_at,o.repository_id,o.pr_id,o.origin_json,o.base_key_v48,o.action_tag_v48,o.parent_ack_attempt_v48) FROM pr_write_attempts a JOIN pr_write_metadata_origins_v25 o USING(attempt_id) WHERE a.attempt_id=2",[],|r|r.get::<_,String>(0)).unwrap(),second_raw);
            let rows=db.prepare("SELECT a.state,a.selected_resource,o.base_key_v48,o.parent_ack_attempt_v48 FROM pr_write_attempts a JOIN pr_write_metadata_origins_v25 o USING(attempt_id) ORDER BY a.attempt_id").unwrap().query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,Option<i64>>(3)?))).unwrap().map(Result::unwrap).collect::<Vec<_>>();
            assert_eq!(rows.len(), 3);
            assert_eq!(
                (&rows[0].0, &rows[1].0, &rows[2].0),
                (
                    &"acknowledged".to_owned(),
                    &"confirmed".to_owned(),
                    &"confirmed".to_owned()
                )
            );
            assert_eq!(rows[2].1, expected_key);
            assert_eq!(rows[0].2, rows[2].2);
            assert_eq!(
                rows[2].3,
                Some(1),
                "Root ACK remains distinct from immediate Confirmed terminal #2"
            );
            assert!(
                db.query_row(
                    "SELECT count(*) FROM pr_write_attempts WHERE state IN('pending','uncertain')",
                    [],
                    |r| r.get::<_, usize>(0)
                )
                .unwrap()
                    == 0
            );
            assert!(
                db.query_row(
                    "SELECT count(*) FROM drafts WHERE resource=?1",
                    [&expected_key],
                    |r| r.get::<_, usize>(0)
                )
                .unwrap()
                    == 0
            );
            assert_eq!(
                commands(&s, &f)
                    .iter()
                    .filter(|r| r["stdin_hex"]
                        .as_str()
                        .unwrap_or_default()
                        .contains("6d75746174696f6e"))
                    .count(),
                3
            );
            s.capture_completed_terminal_v1("metadata-members-v57-three-terminal-chain-fresh-page");
        }
        finish(s, &f, mouse, &steps, &git, &config, &stable);
    }
}
fn cleanup_manual(mouse: bool) {
    for personal in [false, true] {
        let command = if personal {
            Cmd::RemoveReviewer
        } else {
            Cmd::AddLabel
        };
        let mut steps = initial_steps(personal);
        let initial = steps.len();
        steps.extend(phase(command, raw(), candidate(command), current()));
        let opened = steps.len();
        steps.extend(selected_phase(command));
        let prepared = steps.len();
        steps.extend(selected_phase(command));
        // Cleanup follows actual final UID closure even when DELETE fails.
        steps.extend(post(command));
        let published = steps.len();
        steps.extend(manual_phase(command));
        let (f, mut s, git, config, stable) = begin(&steps, personal, mouse, initial);
        open_members(&mut s, &f, mouse, command, opened);
        select(&mut s, &f, mouse, command, &steps[..opened]);
        prepare(&mut s, &f, mouse, command, prepared);
        writable(&f).execute_batch("CREATE TRIGGER members_cleanup_v50 BEFORE DELETE ON drafts WHEN OLD.resource LIKE 'metadata-members-v48/%' BEGIN SELECT RAISE(ABORT,'real Members cleanup unavailable'); END;").unwrap();
        let frame = s.completed_cells_end();
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
            frame,
            RETURN,
            &["DURABLY CONFIRMED", "[r Refresh PR]", "[Esc Back to Inbox]"],
        );
        assert_eq!(state(&f), "confirmed");
        let draft = original_draft(&f);
        let origin = origin(&f);
        let terminal = all_sql(&f).get("pr_write_attempts").unwrap().clone();
        writable(&f)
            .execute_batch("DROP TRIGGER members_cleanup_v50")
            .unwrap();
        s.capture_completed_terminal_v1("metadata-members-durable-confirmed-manual-return");
        let before = s.completed_cursorless_end();
        control(&mut s, mouse, RETURN, "[r Refresh PR]", RETURN_CONTROLS);
        s.wait_for_command_count(&f, steps.len());
        s.wait_for_new_cursorless_condition(
            before,
            |screen| {
                crate::support::main_footer_visible(screen)
                    && screen
                        .contents()
                        .contains("Fresh original PR/search observation")
                    && crate::support::controls::control_painted(screen, "[b Back]")
                    && !screen.contents().contains(RETURN)
            },
            "Actual15-child member read installs genuine same-family page",
            Duration::from_secs(8),
        );
        assert_eq!(state(&f), "confirmed");
        assert_eq!(original_draft(&f), draft);
        assert_eq!(self::origin(&f), origin);
        assert_eq!(all_sql(&f).get("pr_write_attempts"), Some(&terminal));
        assert_eq!(
            super::super::raw(&f)
                .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                    .get::<_, usize>(0))
                .unwrap(),
            1
        );
        finish(s, &f, mouse, &steps, &git, &config, &stable);
    }
}
fn held_uid_cas(mouse: bool) {
    for personal in [false, true] {
        let command = Cmd::AddAssignee;
        let (mut steps, initial, opened, prepared) = uncertain_steps(personal, command);
        steps.extend(initial_steps(personal));
        let restarted = steps.len();
        steps.extend(history_phase(None));
        let listed = steps.len();
        let mut held = history_phase(Some(command));
        held.last_mut().unwrap().delay_ms = 2000;
        steps.extend(held);
        let last = steps.len();
        let (f, mut s, git, config, stable) = begin(&steps, personal, mouse, initial);
        invoke_uncertain(&mut s, &f, mouse, command, opened, prepared, &steps);
        let body = original_draft(&f);
        let origin = origin(&f);
        s.quit_and_assert_restored_terminal(mouse);
        let mut s = restart(&f, mouse, personal, restarted, &steps[..restarted]);
        open_history(&mut s, &f, mouse, listed);
        let frame = s.completed_cells_end();
        let owned = OwnedPtySession::capture(s.owned_application_pid() as u32).unwrap();
        control(
            &mut s,
            mouse,
            &history_title(),
            "[Inspect current]",
            HISTORY_FOOTER,
        );
        s.wait_for_command_count(&f, last);
        let until = Instant::now() + Duration::from_secs(5);
        let held = loop {
            let rows = fs::read_to_string(f.root.join("gh-fixture/commands.jsonl")).unwrap();
            let row: Value = serde_json::from_str(rows.lines().last().unwrap()).unwrap();
            let pid = i32::try_from(row["pid"].as_u64().unwrap()).unwrap();
            if let Some(identity) = process_identity(pid).unwrap() {
                assert!(owned.owns(&identity));
                assert!(!identity.zombie);
                assert_eq!(unsafe { libc::kill(pid, libc::SIGSTOP) }, 0);
                break (pid, identity);
            }
            assert!(
                Instant::now() < until,
                "Actual held finalUID child must be owned"
            );
            s.poll_output(Duration::from_millis(1));
        };
        writable(&f)
            .execute(
                "UPDATE pr_write_attempts SET created_at=created_at+1 WHERE attempt_id=1",
                [],
            )
            .unwrap();
        let expected = all_sql(&f);
        let current = process_identity(held.0).unwrap().unwrap();
        assert!(owned.owns(&current));
        assert!(!current.zombie);
        assert_eq!(
            (current.uid, current.birth, current.session),
            (held.1.uid, held.1.birth, held.1.session)
        );
        assert_eq!(unsafe { libc::kill(held.0, libc::SIGCONT) }, 0);
        // Final UID is last private read. Its previously captured raw row is
        // retained, not reparsed afterwards. Explicit ACK must reject the
        // exact changed raw row before reconciliation/atomic release.
        form_frame(
            &mut s,
            frame,
            &history_title(),
            &["Fresh actual", "[Acknowledge local]", "[Esc Close]"],
        );
        let frame = s.completed_cursorless_end();
        control(
            &mut s,
            mouse,
            &history_title(),
            "[Acknowledge local]",
            HISTORY_FOOTER,
        );
        s.wait_for_new_cursorless_condition(
            frame,
            |screen| {
                crate::support::main_footer_visible(screen)
                    && !screen.contents().contains(&history_title())
            },
            "Exact held-finalUID raw row prevents ACK and retires stale inspection",
            Duration::from_secs(8),
        );
        assert_eq!(all_sql(&f), expected);
        assert_eq!(state(&f), "uncertain");
        assert_eq!(original_draft(&f), body);
        assert_eq!(self::origin(&f), origin);
        finish(s, &f, mouse, &steps, &git, &config, &stable);
    }
}
macro_rules! recovery_native {
    ($key:ident,$mouse:ident,$helper:ident) => {
        #[test]
        fn $key() {
            $helper(false)
        }
        #[test]
        fn $mouse() {
            $helper(true)
        }
    };
}
recovery_native!(
    pr_metadata_members_v50_restart_inspect_ack_explicit_new_keyboard,
    pr_metadata_members_v50_restart_inspect_ack_explicit_new_mouse,
    restart_ack_new
);
fn three_terminal_chain_v57(mouse: bool) {
    restart_ack_new_chain_v57(mouse, true)
}
recovery_native!(
    pr_metadata_members_v57_keyboard_restart_three_terminal_new_chain_exact_root_and_confirmed_parent,
    pr_metadata_members_v57_mouse_restart_three_terminal_new_chain_exact_root_and_confirmed_parent,
    three_terminal_chain_v57
);
recovery_native!(
    pr_metadata_members_v50_confirmed_cleanup_manual15_keyboard,
    pr_metadata_members_v50_confirmed_cleanup_manual15_mouse,
    cleanup_manual
);
recovery_native!(
    pr_metadata_members_v50_held_final_uid_raw_cas_keyboard,
    pr_metadata_members_v50_held_final_uid_raw_cas_mouse,
    held_uid_cas
);
