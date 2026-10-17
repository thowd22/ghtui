//! Real-key/mouse one-attempt publication declarations. No backend-only or
//! snapshot evidence substitutes for Root's future actual-binary execution.
use super::*;
const ID: u64 = 9007199254740993;
const ALL_CONTROLS: &[&str] = &[
    "[b Back / Cancel]",
    "[q Quit]",
    "[h History]",
    "[Enter Details]",
    "[r Refresh]",
    "[a Include read]",
    "[p Participating]",
    "[m Mark read]",
    "[i Ignore]",
    "[u Unignore]",
    "[U Mark unread unavailable]",
    "[C Confirm]",
    "[I Inspect latest]",
    "[L Local ACK]",
    "[W Watch / Stop]",
];
#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Read,
    Ignore,
    Unignore,
}
fn subscription(ignored: bool) -> Value {
    json!({"subscribed":!ignored,"ignored":ignored,"reason":null,"created_at":"2026-10-15T18:00:00Z","url":format!("https://api.github.com/notifications/threads/{ID}/subscription"),"thread_url":format!("https://api.github.com/notifications/threads/{ID}")})
}
fn pre(action: Action) -> Vec<GhStep> {
    let mut steps = principal(42);
    steps.push(get(&format!("repos/{REPO}"), repository()));
    steps.push(get(&format!("notifications/threads/{ID}"), row(ID)));
    if action != Action::Read {
        steps.push(get(
            &format!("notifications/threads/{ID}/subscription"),
            subscription(action == Action::Unignore),
        ));
    }
    steps.push(get(&format!("notifications/threads/{ID}"), row(ID)));
    if action != Action::Read {
        steps.push(get(
            &format!("notifications/threads/{ID}/subscription"),
            subscription(action == Action::Unignore),
        ));
    }
    steps.push(get(&format!("repos/{REPO}"), repository()));
    steps.extend(principal(42));
    steps
}
fn post(action: Action) -> Vec<GhStep> {
    let endpoint = if action == Action::Read {
        format!("notifications/threads/{ID}")
    } else {
        format!("notifications/threads/{ID}/subscription")
    };
    let mut args = vec![
        "api",
        endpoint.as_str(),
        "--hostname",
        HOST,
        "--method",
        if action == Action::Read {
            "PATCH"
        } else {
            "PUT"
        },
        "--include",
    ];
    let raw = if action == Action::Read {
        "HTTP/2.0 205 Reset Content\nContent-Length: 0\r\n\r\n".into()
    } else {
        args.extend(["--input", "-"]);
        format!(
            "HTTP/2.0 200 OK\n\r\n{}",
            subscription(action == Action::Ignore)
        )
    };
    let mut write = GhStep::read(&args, &raw);
    if action != Action::Read {
        write.stdin = serde_json::to_vec(&json!({"ignored":action==Action::Ignore})).unwrap();
    }
    let mut read = row(ID);
    read["unread"] = json!(false);
    read["last_read_at"] = json!("2026-10-15T18:01:00Z");
    let mut repo = repository();
    repo["url"] = json!(format!("https://api.github.com/repos/{REPO}"));
    vec![
        write,
        get(
            &endpoint,
            if action == Action::Read {
                read
            } else {
                subscription(action == Action::Ignore)
            },
        ),
        get("user", json!({"id":42,"login":"octocat"})),
        get(&format!("repos/{REPO}"), repo),
        get("user", json!({"id":42,"login":"octocat"})),
    ]
}
fn control(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    let end = Instant::now() + Duration::from_secs(8);
    let needs_enabled = matches!(label, "[C Confirm]" | "[L Local ACK]");
    let enabled = |frame: &vt100::Screen, point: (u16, u16)| {
        !needs_enabled
            || frame
                .cell(point.1, point.0)
                .is_some_and(|cell| cell.underline())
    };
    if mouse {
        loop {
            session.poll_output(Duration::from_millis(1));
            if let Some(position) = session
                .completed_cells_screen()
                .and_then(|frame| {
                    owned_focus_v1(
                        frame,
                        TITLE,
                        label,
                        FocusAttributeV1::Inverse,
                        ALL_CONTROLS,
                        &[],
                    )
                    .filter(|focus| enabled(frame, focus.target))
                })
                .map(|focus| focus.target)
            {
                session.click(position.0, position.1);
                return;
            }
            assert!(
                Instant::now() < end,
                "Current completed notification control missing: {label}"
            );
        }
    } else if key == b"\r" {
        session.focus_completed_owned_action_v1(
            TITLE,
            label,
            FocusAttributeV1::Inverse,
            ALL_CONTROLS,
            &[],
        );
        if needs_enabled {
            loop {
                assert!(
                    Instant::now() < end,
                    "Exact current focused notification control is not enabled: {label}"
                );
                session.poll_output(Duration::from_millis(1));
                if session.completed_cells_screen().is_some_and(|frame| {
                    owned_focus_v1(
                        frame,
                        TITLE,
                        label,
                        FocusAttributeV1::Inverse,
                        ALL_CONTROLS,
                        &[],
                    )
                    .is_some_and(|focus| focus.target_focused && enabled(frame, focus.target))
                }) {
                    break;
                }
            }
        }
        session.send_key(b"\r");
    } else {
        session.send_key(key);
    }
}
fn exact_writes(session: &Session, f: &Fixture, steps: &[GhStep], writes: usize) {
    let journal = session
        .inspect_command_journal(f)
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(journal.len(), steps.len());
    let mut count = 0;
    for (index, (actual, expected)) in journal.iter().zip(steps).enumerate() {
        assert_eq!(actual["step"], index);
        assert_eq!(actual["matched"], true);
        assert_eq!(actual["args"], json!(expected.args));
        let expected_stdin = expected
            .stdin
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(actual["stdin_hex"], expected_stdin);
        count += usize::from(
            expected
                .args
                .iter()
                .any(|arg| matches!(arg.as_str(), "PATCH" | "PUT" | "POST" | "DELETE")),
        );
        assert!(!expected
            .args
            .iter()
            .any(|arg| arg == "--paginate" || arg == "--slurp" || arg == "graphql"));
    }
    assert_eq!(count, writes);
}
fn independently_unchanged(f: &Fixture, before: Witness, action: &str, state: &str) {
    let mut after = witness(f);
    let mut before = before;
    let old = before
        .sql
        .remove("notification_write_attempts_v63")
        .unwrap();
    assert!(old.is_empty());
    let actual = after.sql.remove("notification_write_attempts_v63").unwrap();
    assert_eq!(actual.len(), 1);
    assert_eq!(after, before);
    let connection =
        Connection::open_with_flags(database(f), OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let value:(String,u64,String,u64,String,String,String,Option<String>,u64)=connection.query_row("SELECT host,account_id,thread_id,repository_id,repository_node,action,state,receipt_json,generation FROM notification_write_attempts_v63",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?,r.get(8)?))).unwrap();
    assert_eq!(
        (
            value.0.as_str(),
            value.1,
            value.2.as_str(),
            value.3,
            value.4.as_str(),
            value.5.as_str(),
            value.6.as_str(),
            value.8
        ),
        (
            HOST,
            42,
            ID.to_string().as_str(),
            91000,
            "ACTUAL_NOTIFICATION_REPOSITORY_NODE",
            action,
            state,
            1
        )
    );
    if state == "confirmed" {
        let receipt: Value = serde_json::from_str(value.7.as_ref().unwrap()).unwrap();
        assert_eq!(receipt["account_id"], 42);
        assert_eq!(receipt["thread"], ID.to_string());
        assert_eq!(receipt["repository_id"], 91000);
        assert_eq!(receipt["requested"], action);
    } else {
        assert!(value.7.is_none());
    }
}
fn prepared(session: &mut Session, mouse: bool, action: Action) {
    let after = session.completed_cells_end();
    let (label, key, needle) = match action {
        Action::Read => (
            "[m Mark read]",
            b"m".as_slice(),
            "Mark this exact notification read",
        ),
        Action::Ignore => (
            "[i Ignore]",
            b"i".as_slice(),
            "Ignore this exact notification thread",
        ),
        Action::Unignore => (
            "[u Unignore]",
            b"u".as_slice(),
            "Stop ignoring this exact notification thread",
        ),
    };
    control(session, mouse, label, key);
    ready(session, after, needle);
    session.resize_owned_header(
        120,
        40,
        TITLE,
        &["[b Back / Cancel]", "[q Quit]", "[C Confirm]"],
    );
    ready(session, 0, "Explicit Confirm starts a new original phase");
    let after = session.completed_cells_end();
    session.paste("DO_NOT_EDIT_OR_PUBLISH_FROM_READONLY_PREVIEW");
    session.wait_for_new_cursorless_condition(
        after,
        |frame| {
            let text = frame.contents();
            text.contains(needle)
                && text.contains("[C Confirm]")
                && !text.contains("DO_NOT_EDIT_OR_PUBLISH")
        },
        "Actual read-only Preview ignores paste without changing exact approval",
        Duration::from_secs(8),
    );
    session.capture_completed_terminal_v1("notifications-real-publication-preview-120");
}
fn publish(mouse: bool, action: Action) {
    let mut steps = empty_inbox_steps("octocat");
    steps.extend(phase(false, false, false, false));
    steps.extend(pre(action));
    steps.extend(pre(action));
    steps.extend(post(action));
    let f = fixture(&steps);
    let mut session = start(&f);
    let before = witness(&f);
    let after = open(&mut session, mouse);
    ready(&mut session, after, PRIVATE);
    let initial = session.inspect_command_journal(&f);
    control(&mut session, mouse, "[U Mark unread unavailable]", b"U");
    ready(&mut session, 0, "bulk PUT is not an inverse");
    assert_eq!(session.inspect_command_journal(&f), initial);
    prepared(&mut session, mouse, action);
    assert_eq!(witness(&f), before);
    let after = session.completed_cells_end();
    control(&mut session, mouse, "[C Confirm]", b"\r");
    session.wait_for_new_cursorless_condition(
        after,
        |frame| {
            let text = frame.contents();
            !text.contains(PRIVATE)
                && !text.contains(TITLE)
                && text.contains("Notification state observed confirmed")
        },
        "Actual typed notification state confirmed and exact old private snapshot retired",
        Duration::from_secs(8),
    );
    exact_writes(&session, &f, &steps, 1);
    independently_unchanged(
        &f,
        before,
        match action {
            Action::Read => "mark_read",
            Action::Ignore => "ignore",
            Action::Unignore => "unignore",
        },
        "confirmed",
    );
    assert_sql_generation(&f, &[1]);
    session.quit_and_assert_restored_terminal(mouse);
}
fn cancel_before_invocation(mouse: bool) {
    let mut steps = empty_inbox_steps("octocat");
    steps.extend(phase(false, false, false, false));
    steps.extend(pre(Action::Read));
    let mut held = pre(Action::Read);
    held[4].delay_ms = 6000;
    held.truncate(5);
    steps.extend(held);
    let f = fixture(&steps);
    let mut session = start(&f);
    let before = witness(&f);
    let after = open(&mut session, mouse);
    ready(&mut session, after, PRIVATE);
    prepared(&mut session, mouse, Action::Read);
    control(&mut session, mouse, "[C Confirm]", b"\r");
    session.wait_for_command_count(&f, steps.len());
    let journal = session.inspect_command_journal(&f);
    let last: Value = serde_json::from_str(journal.lines().last().unwrap()).unwrap();
    let child = process_identity(i32::try_from(last["pid"].as_u64().unwrap()).unwrap())
        .unwrap()
        .expect("actual held original closing thread alive before Cancel");
    let application_pid = u32::try_from(session.owned_application_pid())
        .expect("owned application PID must be positive and fit u32");
    let owner = OwnedPtySession::capture(application_pid).unwrap();
    assert!(owner.owns(&child));
    assert!(!child.zombie);
    let end = session.completed_cells_end();
    control(&mut session, mouse, "[b Back / Cancel]", b"b");
    session.wait_for_new_cursorless_condition(end,|frame|{let text=frame.contents();!text.contains(TITLE)&&!text.contains(PRIVATE)&&text.contains("Notification attempt closed")},"Exact uncertain write presentation closes without restoring private old body or claiming no effects",Duration::from_secs(8));
    child_gone(&mut session, &child);
    exact_writes(&session, &f, &steps, 0);
    independently_unchanged(&f, before, "mark_read", "pending");
    assert_sql_generation(&f, &[1]);
    session.quit_and_assert_restored_terminal(mouse);
}
fn immutable_attempt(f: &Fixture) -> Vec<SqlValue> {
    let connection =
        Connection::open_with_flags(database(f), OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    connection.query_row("SELECT id,host,account_id,thread_id,repository_id,repository_node,subject_kind,subject_key,action,requested,observed_updated_at,original_source_key,original_generation,intent_digest,receipt_json,origin_json,created_at,generation FROM notification_write_attempts_v63",[],|row|(0..18).map(|i|row.get(i)).collect()).unwrap()
}
fn unknown_restart_inspect_ack(mouse: bool) {
    let mut steps = empty_inbox_steps("octocat");
    steps.extend(phase(false, false, false, false));
    steps.extend(pre(Action::Read));
    steps.extend(pre(Action::Read));
    let mut bad = post(Action::Read).remove(0);
    bad.stdout = b"HTTP/2.0 200 OK\nContent-Length: 0\r\n\r\n".to_vec();
    steps.push(bad);
    let first_count = steps.len();
    assert_eq!(first_count, 31);
    steps.extend(empty_inbox_steps("octocat"));
    steps.extend(phase(false, false, false, false));
    steps.extend(not_modified_phase());
    steps.extend(pre(Action::Read));
    steps.extend(pre(Action::Read));
    let f = fixture(&steps);
    let mut session = start(&f);
    let before = witness(&f);
    let after = open(&mut session, mouse);
    ready(&mut session, after, PRIVATE);
    prepared(&mut session, mouse, Action::Read);
    let after = session.completed_cells_end();
    control(&mut session, mouse, "[C Confirm]", b"\r");
    session.wait_for_new_cursorless_condition(
        after,
        |frame| {
            let text = frame.contents();
            text.contains("Notification outcome uncertain")
                && !text.contains(PRIVATE)
                && !text.contains(TITLE)
        },
        "Actual malformed205 acknowledgement remains uncertain and old private body is erased",
        Duration::from_secs(8),
    );
    exact_writes(&session, &f, &steps[..first_count], 1);
    independently_unchanged(&f, before, "mark_read", "uncertain");
    let original = immutable_attempt(&f);
    let first_journal = session.inspect_command_journal(&f);
    session.quit_and_assert_restored_terminal(mouse);
    let mut session = start(&f);
    let before = witness(&f);
    assert_eq!(immutable_attempt(&f), original);
    let after = open(&mut session, mouse);
    ready(&mut session, after, PRIVATE);
    let after = session.completed_cells_end();
    control(&mut session, mouse, "[h History]", b"h");
    ready(&mut session, after, "Uncertain");
    assert_eq!(
        witness(&f),
        before,
        "Fresh history reads never restore/acknowledge/delete a protected row"
    );
    let after = session.completed_cells_end();
    control(&mut session, mouse, "[I Inspect latest]", b"I");
    ready(&mut session, after, "Current observed state: Unread");
    assert_eq!(
        witness(&f),
        before,
        "Fresh server inspection is still read-only"
    );
    let before_resize = session.completed_cursorless_end();
    session.resize_owned_header(
        120,
        40,
        TITLE,
        &["[b Back / Cancel]", "[q Quit]", "[L Local ACK]"],
    );
    let sentence = "Explicit LOCAL acknowledgement only releases this exact unresolved local barrier. It never resends, restores a draft, deletes history or claims a remote effect.";
    assert!(sentence.is_ascii());
    let split = usize::from(ghtui::prepared_form_text_v1::FORM_COLUMNS_V1[1]);
    let expected_rows = [&sentence[..split], &sentence[split..]];
    session.wait_for_new_cursorless_condition(
        before_resize,
        |frame| {
            frame.size() == (40, 120)
                && crate::support::owned_resize_controls_ready_v1(
                    frame,
                    TITLE,
                    &["[b Back / Cancel]", "[q Quit]", "[L Local ACK]"],
                    crate::support::ResizeToolbarV1::Header,
                )
                && crate::support::overlay_body(frame, TITLE).is_some_and(|body| {
                    body.contains("Current observed state: Unread")
                        && body.lines().collect::<Vec<_>>().windows(2).any(|rows| {
                            rows.iter().zip(expected_rows).all(|(actual, expected)| {
                                actual
                                    .strip_prefix(expected)
                                    .is_some_and(|padding| padding.bytes().all(|b| b == b' '))
                            })
                        })
                })
        },
        "Same NEW completed Inspect frame retains full exact no-resend sentence across adjacent prepared rows",
        Duration::from_secs(8),
    );
    session.capture_completed_terminal_v1("notifications-real-restart-inspect-before-local-ack");
    let after = session.completed_cells_end();
    control(&mut session, mouse, "[L Local ACK]", b"\r");
    ready(
        &mut session,
        after,
        "Exact local notification barrier acknowledged",
    );
    assert_eq!(
        immutable_attempt(&f),
        original,
        "Local ACK never rewrites body/reference/receipt/original target/history"
    );
    let connection =
        Connection::open_with_flags(database(&f), OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let state: String = connection
        .query_row(
            "SELECT state FROM notification_write_attempts_v63",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(state, "acknowledged");
    let mut after_witness = witness(&f);
    let mut before_witness = before;
    after_witness.sql.remove("notification_write_attempts_v63");
    before_witness.sql.remove("notification_write_attempts_v63");
    assert_eq!(after_witness, before_witness);
    assert_sql_generation(&f, &[3]);
    let actual = session.inspect_command_journal(&f);
    assert!(actual.starts_with(&first_journal));
    exact_writes(&session, &f, &steps, 1);
    let end = session.completed_cells_end();
    session.send_key(b"L");
    session.send_key(b"C");
    session.wait_for_new_cursorless_condition(
        end,
        |frame| {
            frame
                .contents()
                .contains("Exact local notification barrier acknowledged")
        },
        "Repeated disabled controls cannot replay publication or ACK",
        Duration::from_secs(8),
    );
    assert_eq!(session.inspect_command_journal(&f), actual);
    quit(&mut session, mouse);
}
fn watch_stop(mouse: bool) {
    let mut steps = empty_inbox_steps("octocat");
    steps.extend(phase(false, false, false, false));
    let f = fixture(&steps);
    let mut session = start(&f);
    let before = witness(&f);
    let after = open(&mut session, mouse);
    ready(&mut session, after, PRIVATE);
    let original = session.inspect_command_journal(&f);
    let after = session.completed_cells_end();
    control(&mut session, mouse, "[W Watch / Stop]", b"W");
    ready(&mut session, after, "Finite read-only watch");
    let after = session.completed_cells_end();
    control(&mut session, mouse, "[r Refresh]", b"r");
    ready(&mut session, after, "Watch server cooldown still applies");
    assert_eq!(session.inspect_command_journal(&f), original);
    let after = session.completed_cells_end();
    control(&mut session, mouse, "[W Watch / Stop]", b"W");
    ready(&mut session, after, "Finite notification watch stopped");
    exact_writes(&session, &f, &steps, 0);
    assert_eq!(witness(&f), before);
    assert_sql_generation(&f, &[1]);
    quit(&mut session, mouse);
}
#[test]
fn notifications_v63_keyboard_mark_read_exact205_one_post_readback_effects() {
    publish(false, Action::Read)
}
#[test]
fn notifications_v63_mouse_mark_read_exact205_one_post_readback_effects() {
    publish(true, Action::Read)
}
#[test]
fn notifications_v63_keyboard_ignore_thread_not_pr_subscription() {
    publish(false, Action::Ignore)
}
#[test]
fn notifications_v63_mouse_ignore_thread_not_pr_subscription() {
    publish(true, Action::Ignore)
}
#[test]
fn notifications_v63_keyboard_unignore_thread_not_pr_subscription() {
    publish(false, Action::Unignore)
}
#[test]
fn notifications_v63_mouse_unignore_thread_not_pr_subscription() {
    publish(true, Action::Unignore)
}
#[test]
fn notifications_v63_keyboard_held_closing_thread_cancel_pending_no_write() {
    cancel_before_invocation(false)
}
#[test]
fn notifications_v63_mouse_held_closing_thread_cancel_pending_no_write() {
    cancel_before_invocation(true)
}
#[test]
fn notifications_v63_keyboard_real_unknown_restart_fresh_inspect_explicit_local_ack_no_resend() {
    unknown_restart_inspect_ack(false)
}
#[test]
fn notifications_v63_mouse_real_unknown_restart_fresh_inspect_explicit_local_ack_no_resend() {
    unknown_restart_inspect_ack(true)
}
#[test]
fn notifications_v63_keyboard_finite_watch_server_cooldown_stop_no_extra_child() {
    watch_stop(false)
}
#[test]
fn notifications_v63_mouse_finite_watch_server_cooldown_stop_no_extra_child() {
    watch_stop(true)
}
