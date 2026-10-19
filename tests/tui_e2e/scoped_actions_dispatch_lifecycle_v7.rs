//! TASK54W7 actual restart and raw final-UID drift. All source declarations UNRUN.
use super::*;

pub(super) fn history(protocol: Protocol, selected: bool) -> Vec<GhStep> {
    let mut body = Vec::new();
    if selected {
        body.push(get_versioned(
            &format!("repos/{CURRENT}/actions/workflows/7"),
            workflow(),
            protocol,
        ));
        if protocol.detailed() {
            body.push(get_versioned(
                &format!("repos/{CURRENT}/actions/runs/124"),
                returned_run(),
                protocol,
            ));
        }
    }
    let result = resources_v2::resource(body, 42);
    assert_eq!(
        result.len(),
        6 + usize::from(selected) * (1 + usize::from(protocol.detailed()))
    );
    result
}
struct Stopped(ProcessIdentity);
impl Stopped {
    fn capture(s: &Session, child: &ProcessIdentity) -> Self {
        let leader = process_identity(s.owned_application_pid())
            .unwrap()
            .unwrap();
        assert!(child.pid > 0 && child.pid != leader.pid && !child.zombie);
        assert_eq!(
            (child.parent, child.uid, child.session),
            (leader.pid, leader.uid, leader.session)
        );
        assert!(child.birth >= leader.birth);
        assert_eq!(process_identity(child.pid).unwrap().as_ref(), Some(child));
        // SAFETY: exact current positive child PID, actual UID/SID/birth, no group signal.
        assert_eq!(unsafe { libc::kill(child.pid, libc::SIGSTOP) }, 0);
        Self(child.clone())
    }
}
impl Drop for Stopped {
    fn drop(&mut self) {
        if let Ok(Some(current)) = process_identity(self.0.pid) {
            if !current.zombie
                && (current.uid, current.session, current.birth)
                    == (self.0.uid, self.0.session, self.0.birth)
            {
                // SAFETY: resume only the original verified child, even on assertion unwind.
                assert_eq!(unsafe { libc::kill(current.pid, libc::SIGCONT) }, 0);
            }
        }
    }
}
fn rows(f: &Fixture) -> Vec<String> {
    let db = publication_v3::raw(f);
    let mut q=db.prepare("SELECT json_array(attempt_id,host,principal,target_json,resources_json,kind_json,context_json,options_json,fingerprint_sha256,state,claimed,receipt_json,created_at) FROM actions_write_attempts ORDER BY attempt_id").unwrap();
    let result = q
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    result
}
fn final_uid_row_drift(mouse: bool) {
    for column in ["receipt_json", "context_json", "options_json"] {
        let p = Protocol::Detailed;
        let mut tail = history(p, true);
        let mut ack = history(p, true);
        ack.last_mut().unwrap().delay_ms = 5000;
        tail.extend(ack);
        let (f, mut s, expected) = super::task54_w7_returned_run::prepare_confirmed(mouse, tail);
        let confirmed = expected.len() - 16;
        let git = git_state(&f);
        let protected = unrelated(&f);
        let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
        let tty = s.actual_terminal_witness_v4().unwrap();
        let after = s.completed_cells_end();
        action(&mut s, mouse, HISTORY, "[View original]");
        s.wait_for_command_count(&f, confirmed + 8);
        owned(
            &mut s,
            after,
            HISTORY,
            &["Actual returned run 124", "[F6 Acknowledge locally]"],
        );
        let original = metadata(&f);
        action(&mut s, mouse, HISTORY, "[F6 Acknowledge locally]");
        let child = entered(&mut s, &f, expected.len());
        let stopped = Stopped::capture(&s, &child);
        // Deliberately independently writable SQLite, not the read-only observer.
        let db = rusqlite::Connection::open(database(&f)).unwrap();
        let generations: Vec<(String, i64)> = {
            let mut q=db.prepare("SELECT resource_key,generation FROM actions_write_generations ORDER BY resource_key").unwrap();
            let values = q
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .map(Result::unwrap)
                .collect();
            values
        };
        assert!(matches!(
            column,
            "receipt_json" | "context_json" | "options_json"
        ));
        assert_eq!(
            db.execute(
                &format!(
                    "UPDATE actions_write_attempts SET {column}={column}||' ' WHERE attempt_id=1"
                ),
                []
            )
            .unwrap(),
            1
        );
        let expected_rows = rows(&f);
        let after = s.completed_cursorless_end();
        drop(stopped);
        s.wait_for_new_cursorless_condition(
            after,
            |frame| {
                crate::support::overlay_body(frame, HISTORY).is_none()
                    && frame.contents().contains("private")
                    && crate::support::main_footer_visible(frame)
            },
            "Actual unchanged-generation raw Dispatch row drift during final UID refuses ACK",
            Duration::from_secs(8),
        );
        gone(&mut s, &child);
        assert_eq!(metadata(&f).0, "accepted_pending");
        assert_eq!(
            rows(&f),
            expected_rows,
            "No raw byte rewritten or protected reference released"
        );
        let now: Vec<(String, i64)> = {
            let mut q=db.prepare("SELECT resource_key,generation FROM actions_write_generations ORDER BY resource_key").unwrap();
            let values = q
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .map(Result::unwrap)
                .collect();
            values
        };
        assert_eq!(now, generations);
        assert_eq!(metadata(&f).0, original.0);
        assert_eq!(unrelated(&f), protected);
        assert_eq!(git_state(&f), git);
        assert_eq!(s.actual_terminal_witness_v4().unwrap(), tty);
        s.quit_and_assert_restored_terminal(mouse);
        exact(&s, &f, &expected, 1);
        assert_eq!(
            fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
            config
        );
        assert_private_intents_unchanged(&f);
        assert_read_only_git(&f);
    }
}
#[test]
fn scoped_actions_dispatch_v7_task54_w7_keyboard_final_uid_raw_receipt_context_options_cas_no_ack()
{
    final_uid_row_drift(false);
}
#[test]
fn scoped_actions_dispatch_v7_task54_w7_mouse_final_uid_raw_receipt_context_options_cas_no_ack() {
    final_uid_row_drift(true);
}
fn fresh_catalogue(f: &Fixture, mouse: bool, offset: usize) -> Session {
    let mut s = launch(f, mouse);
    s.resize(120, 40);
    s.wait_for_main_frame();
    let prior = commands(&s, f);
    assert_eq!(prior.len(), offset);
    choose(&mut s, mouse, "[: Commands]", b":");
    overlay_ready(
        &mut s,
        "Commands · current context",
        &["Filter focused:", "[Enter Choose]"],
    );
    s.send_key(b"\x01");
    s.paste("Discover repositories");
    overlay_ready(
        &mut s,
        "Commands · current context",
        &["> Discover repositories"],
    );
    choose(&mut s, mouse, "> Discover repositories", b"\r");
    s.wait_for_overlay_close("Commands · current context");
    ready(&mut s, &[TITLE, "[s Source]", "[Esc Close]"]);
    assert_eq!(
        commands(&s, f),
        prior,
        "Restart/pure open cannot replay saved publication"
    );
    let after = s.completed_cells_end();
    discover(&mut s, mouse);
    s.wait_for_command_count(f, offset + 7);
    // The actual loaded Catalogue owns a focused filter caret. Show marks
    // complete cells just as Hide does, without claiming cursor coordinates.
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(2));
        if s.completed_cells_end() > after
            && s.completed_cells_screen().is_some_and(|frame| {
                crate::support::overlay_body(frame, TITLE).is_some_and(|body| {
                    body.contains(FIRST)
                        && body.contains("Repository ID: 10000")
                        && body.contains("Selected: no")
                        && body.contains("[/ Filter]")
                })
            })
        {
            break;
        }
        if Instant::now() >= until {
            s.save_artifacts();
            panic!(
                "No NEW completed cells: Actual new loaded Catalogue source before restart filtering\n{}",
                s.screen()
            );
        }
    }
    filter(&mut s, mouse, "repo0000", FIRST, "Repository ID: 10000");
    choose(&mut s, mouse, "[Open Actions]", b"A");
    s.wait_for_command_count(f, offset + 14);
    s.wait_for_overlay_close(TITLE);
    actions_ready(&mut s);
    s
}
fn visible_history(s: &mut Session, mouse: bool, after: usize, words: &[&str]) {
    let until = Instant::now() + Duration::from_secs(8);
    let mut previous_end = after;
    loop {
        s.poll_output(Duration::from_millis(1));
        let end = s.completed_cursorless_end();
        if end > previous_end {
            if let Some((body, size)) = s.completed_cursorless_screen().and_then(|frame| {
                crate::support::main_footer_visible(frame)
                    .then(|| crate::support::overlay_body(frame, "Actions • focused view ─"))
                    .flatten()
                    .filter(|body| {
                        // A strict child count can precede worker publication.
                        // Never send PgDn/Select to the retained busy list:
                        // that correctly cancels the still-pending read.
                        // The framed pane clips the context line after the
                        // view segment at 80 columns, so anchor on the view
                        // segment without the clipped scope that follows it.
                        frame
                            .contents()
                            .contains("/ Inspect original Actions publication")
                            && crate::support::locate_control(frame, "[b Cancel read]").is_none()
                            && !body.contains("Original current read pending")
                            && crate::support::locate_control(frame, "[Esc Cancel]").is_none()
                    })
                    .map(|body| (body, frame.size()))
            }) {
                if words.iter().all(|word| body.contains(word)) {
                    return;
                }
                previous_end = end;
                if mouse {
                    s.wheel(size.1 / 2, size.0.saturating_sub(5), true);
                } else {
                    s.send_key(b"\x1b[6~");
                }
                loop {
                    s.poll_output(Duration::from_millis(1));
                    if s.completed_cursorless_end() > end
                        && s.completed_cursorless_screen()
                            .and_then(|frame| {
                                crate::support::overlay_body(frame, "Actions • focused view ─")
                            })
                            .is_some_and(|new| new != body)
                    {
                        break;
                    }
                    assert!(
                        Instant::now() < until,
                        "One actual history scroll needs a NEW changed owned viewport\n{}",
                        s.screen()
                    );
                }
            }
        }
        assert!(
            Instant::now() < until,
            "FULL original Dispatch history facts {words:?} must be actually visible\n{}",
            s.screen()
        );
    }
}
fn restart(mouse: bool) {
    for p in [Protocol::Compatible, Protocol::Detailed] {
        let mut first = opening();
        first.extend(schema_phase(p));
        first.extend(schema_phase(p));
        first.extend(confirmation(p, 42));
        let offset = first.len();
        let mut expected = first.clone();
        expected.extend(actions_protocol());
        expected.extend(history(p, false));
        expected.extend(history(p, true));
        expected.extend(history(p, true));
        let f = configured_native(&expected);
        let git = git_state(&f);
        let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
        let mut s = start(&f, mouse);
        begin(&mut s, &f, mouse, p);
        inspect_schema(&mut s, &f, mouse);
        preview(&mut s, &f, mouse);
        let after = s.completed_cells_end();
        action(&mut s, mouse, INTENT, "[Confirm action]");
        s.wait_for_command_count(&f, offset);
        owned(&mut s, after, HISTORY, &["[View original]", "[Close]"]);
        let original = metadata(&f);
        assert_eq!(original.0, "accepted_pending");
        let protected = unrelated(&f);
        close(&mut s, mouse, HISTORY);
        s.quit_and_assert_restored_terminal(mouse);
        let mut resumed = fresh_catalogue(&f, mouse, offset);
        let after_discovery = unrelated(&f);
        let mut expected_sql = protected.clone();
        let (_, reservations) = expected_sql
            .iter_mut()
            .find(|(table, _)| table == "sync_reservations")
            .unwrap();
        assert_eq!(reservations.len(), 1);
        let row = &mut reservations[0];
        use rusqlite::types::Value as SqlValue;
        assert_eq!(row.len(), 5);
        assert_eq!(row[0], SqlValue::Text(HOST.into()));
        assert_eq!(row[1], SqlValue::Text("user-id:42".into()));
        assert!(
            matches!(&row[2],SqlValue::Text(source)if source.starts_with("repository-catalogue-source-v1:"))
        );
        assert_eq!(row[4], SqlValue::Integer(1));
        let SqlValue::Integer(generation) = row[3] else {
            panic!("Genuine actual reservation generation")
        };
        assert!(generation > 0);
        row[3] = SqlValue::Integer(generation.checked_add(1).unwrap());
        assert_eq!(
            after_discovery, expected_sql,
            "Only the genuine new Catalogue reservation advances once"
        );
        assert_eq!(metadata(&f), original);
        let tty = resumed.actual_terminal_witness_v4().unwrap();
        let after = resumed.completed_cursorless_end();
        resources_v2::action(&mut resumed, mouse, "[P Publications]", b"P");
        resumed.wait_for_command_count(&f, offset + 20);
        complete(&mut resumed, after, &["Original #1", "DispatchWorkflow"]);
        resumed.resize(80, 24);
        let after = resumed.completed_cursorless_end();
        selected_row(&mut resumed, mouse, "Original #1");
        let selected = history(p, true).len();
        resumed.wait_for_command_count(&f, offset + 20 + selected);
        visible_history(
            &mut resumed,
            mouse,
            after,
            &[
                "HISTORICAL Dispatch branch main",
                &format!("Saved definition commit {C}"),
                &format!("Chosen API {}", p.version()),
            ],
        );
        assert_eq!(metadata(&f), original);
        let after = resumed.completed_cursorless_end();
        if mouse {
            selected_row(
                &mut resumed,
                true,
                "Acknowledge inspected acceptance (effect unverified)",
            );
        } else {
            resumed.focus_completed_actions_control_v2(
                "Acknowledge inspected acceptance (effect unverified)",
            );
            resumed.send_key(b"\r");
        }
        resumed.wait_for_command_count(&f, expected.len());
        visible_history(
            &mut resumed,
            mouse,
            after,
            &[
                "Explicit inspected acceptance ACK committed locally",
                "effect UNVERIFIED • no remote request repeated • original receipt retained",
            ],
        );
        let now = metadata(&f);
        assert_eq!(now.0, "acknowledged");
        assert_eq!((now.1, now.2, now.3), (original.1, original.2, original.3));
        assert_eq!(unrelated(&f), after_discovery);
        assert_eq!(git_state(&f), git);
        assert_eq!(resumed.actual_terminal_witness_v4().unwrap(), tty);
        resumed.quit_and_assert_restored_terminal(mouse);
        exact(&resumed, &f, &expected, 1);
        assert_eq!(
            fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
            config
        );
        assert_private_intents_unchanged(&f);
        assert_read_only_git(&f);
    }
}
#[test]
fn scoped_actions_dispatch_v7_task54_w7_keyboard_real_restart_fresh_source_inspect_acceptance_ack_no_resend(
) {
    restart(false);
}
#[test]
fn scoped_actions_dispatch_v7_task54_w7_mouse_real_restart_fresh_source_inspect_acceptance_ack_no_resend(
) {
    restart(true);
}
