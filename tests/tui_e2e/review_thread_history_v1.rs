//! Actual protected publication -> real binary restart -> fresh
//! Catalogue/MyPRs/Summary -> J inspect-only route, with no guessed old target.
use super::*;
#[path = "review_thread_history_interactions_v1.rs"]
mod interactions_v1;
pub(super) const HISTORY: &str = "Thread publication history • inspect only";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Protected {
    Pending,
    Uncertain,
    Confirmed,
}
pub(super) fn phase_history() -> Vec<GhStep> {
    let mut steps = principal(42);
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.extend(principal(42));
    assert_eq!(steps.len(), 8);
    steps
}
fn protected_sql(fixture: &Fixture) -> Vec<String> {
    let db = raw(fixture);
    [
        "SELECT json_array(host,login,resource,body) FROM drafts WHERE resource LIKE 'thread-draft/%' ORDER BY host,login,resource",
        "SELECT json_array(attempt_id,host,principal,pr_node,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at) FROM pr_write_attempts ORDER BY attempt_id",
        "SELECT json_array(host,principal,pr_node,generation) FROM pr_write_scopes ORDER BY host,principal,pr_node",
        "SELECT json_array(attempt_id,repository_id,pr_id,origin_json) FROM pr_write_thread_origins_v1 ORDER BY attempt_id",
    ].into_iter().flat_map(|query|db.prepare(query).unwrap().query_map([],|row|row.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>()).collect()
}
pub(super) fn history_ready(session: &mut Session, words: &[&str]) {
    session.wait_for_cursorless_condition(
        |frame| {
            crate::support::overlay_body(frame, HISTORY).is_some_and(|body| {
                body.contains("[Esc Back]")
                    && body.contains("[q Quit]")
                    && body.contains("Inspect only")
                    && words.iter().all(|word| body.contains(word))
            })
        },
        "Actual complete owned inspect-only history box/body/controls",
        Duration::from_secs(8),
    );
}
fn detail_contains(session: &mut Session, mouse: bool, target: &str) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let frame = session
            .completed_cursorless_screen()
            .expect("History has a completed cursorless frame");
        let body = crate::support::overlay_body(frame, HISTORY)
            .expect("Same owned complete history overlay");
        if body.contains(target) {
            return;
        }
        let previous = body.clone();
        let endpoint = session.completed_cursorless_end();
        choose(session, mouse, "[PgDn Next]", b"\x1b[6~");
        session.wait_for_new_cursorless_condition(
            endpoint,
            |frame| {
                crate::support::overlay_body(frame, HISTORY).is_some_and(|body| {
                    body.contains("[Esc Back]") && body.contains("Inspect only") && body != previous
                })
            },
            "One acknowledged actual owned detail viewport change before any further paging",
            deadline.saturating_duration_since(Instant::now()),
        );
        assert!(
            Instant::now() < deadline,
            "Original 8s detail budget; no blind scroll or timeout extension"
        );
    }
}
fn fresh_summary(session: &mut Session, fixture: &Fixture, mouse: bool, start: usize) {
    // This is a restart of the SAME strict journal, including its original
    // publication. The ordinary catalogue helper intentionally requires a
    // fresh read-only journal; do not weaken that helper or discard history.
    let original_commands = commands(session, fixture);
    assert_eq!(original_commands.len(), start);
    choose(session, mouse, "[: Commands]", b":");
    overlay_ready(
        session,
        "Commands · current context",
        &["Filter focused:", "[Enter Choose]"],
    );
    session.send_key(b"\x01");
    session.paste("Discover repositories");
    overlay_ready(
        session,
        "Commands · current context",
        &["> Discover repositories"],
    );
    choose(session, mouse, "> Discover repositories", b"\r");
    session.wait_for_overlay_close("Commands · current context");
    ready(session, &[TITLE, "[s Source]", "[Esc Close]"]);
    assert_eq!(commands(session, fixture), original_commands);
    discover(session, mouse);
    session.wait_for_command_count(fixture, start + 7);
    ready(session, &[TITLE, FIRST]);
    if !mouse {
        session.send_key(b"\t");
    }
    choose(session, mouse, "[Open My PRs]", b"\r");
    session.wait_for_overlay_close(TITLE);
    session.wait_for_command_count(fixture, start + 18);
    ready(session, &[ROW_TITLE]);
    choose(session, mouse, "1 Inbox", b"1");
    choose(session, mouse, ROW_TITLE, b"\r");
    session.wait_for_command_count(fixture, start + 31);
    ready(session, &[DETAIL_TITLE, "[J Thread history]", "[b Back]"]);
}
fn bind_fresh_head(steps: &mut [GhStep], head: &str) {
    for step in steps {
        let Ok(mut value) = serde_json::from_slice::<Value>(&step.stdout) else {
            continue;
        };
        if value["number"] == json!(7) && value.get("head").is_some() {
            value["head"]["sha"] = json!(head);
            step.stdout = value.to_string().into_bytes();
        }
    }
}
fn journey(
    mouse: bool,
    protected: Protected,
    legacy: bool,
    changed_head: bool,
    lose_account: bool,
) {
    journey_with_interactions(mouse, protected, legacy, changed_head, lose_account, false);
}
fn journey_with_interactions(
    mouse: bool,
    protected: Protected,
    legacy: bool,
    changed_head: bool,
    lose_account: bool,
    interactions: bool,
) {
    let operation = Operation::Reply;
    let mut write = confirm(operation);
    let held = match protected {
        Protected::Pending => 7,
        Protected::Uncertain => 15,
        Protected::Confirmed => 18,
    };
    write[held].delay_ms = 6000;
    write.truncate(held + 1);
    let mut steps = initial();
    steps.extend(resource_operation(false));
    steps.extend(browse(operation));
    steps.extend(observation(operation));
    steps.extend(observation(operation));
    steps.extend(write);
    let old_end = steps.len();
    let mut fresh = initial();
    let mut list = phase_history();
    let mut open = phase_history();
    if interactions {
        // A genuine original HistoryList child stays live while its disabled
        // control and owned resize geometry are exercised. Same existing
        // six-second held-child fixture; no fake frame or new provider seam.
        list[0].delay_ms = 6000;
    }
    if changed_head {
        let h = "d".repeat(40);
        bind_fresh_head(&mut fresh, &h);
        bind_fresh_head(&mut list, &h);
        bind_fresh_head(&mut open, &h);
    }
    steps.extend(fresh);
    steps.extend(list);
    steps.extend(open);
    if lose_account {
        steps.extend(principal(43));
    }
    let fixture = resource_configured(&steps);
    Cache::open(&database(&fixture))
        .unwrap()
        .put_draft(
            &Scope::github(HOST, Some(43)).unwrap(),
            "foreign-thread-unrelated-original",
            FOREIGN,
        )
        .unwrap();
    if protected == Protected::Confirmed {
        Connection::open(database(&fixture)).unwrap().execute_batch("CREATE TRIGGER refuse_thread_history_cleanup BEFORE DELETE ON drafts WHEN OLD.resource LIKE 'thread-draft/%' BEGIN SELECT RAISE(ABORT,'controlled confirmed cleanup failure'); END;").unwrap();
    }
    let git = git_state(&fixture);
    let sql = untouched_sql(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    prepared(&mut session, &fixture, mouse, operation);
    let before = session.completed_cursorless_end();
    form_control(&mut session, mouse, operation, "[Confirm action]");
    let child = held_child(&mut session, &fixture, 80 + held);
    let (encoded,state):(String,String)=raw(&fixture).query_row("SELECT d.body,a.state FROM drafts d JOIN pr_write_attempts a ON a.principal=d.login WHERE d.resource LIKE 'thread-draft/%'",[],|row|Ok((row.get(0)?,row.get(1)?))).unwrap();
    let original: Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(original["values"]["body"], REPLY);
    assert_eq!(
        state,
        if protected == Protected::Confirmed {
            "confirmed"
        } else {
            "pending"
        }
    );
    let origin: String = raw(&fixture)
        .query_row(
            "SELECT origin_json FROM pr_write_thread_origins_v1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let origin: Value = serde_json::from_str(&origin).unwrap();
    assert_eq!(origin["reference"], original["publication"]);
    assert_eq!(origin["seed"]["scope"]["login"], "user-id:42");
    assert_eq!(origin["seed"]["repository_id"], 10000);
    assert_eq!(origin["seed"]["pr_id"], 1007);
    assert_eq!(origin["seed"]["root"]["rest_id"], 9009);
    assert_eq!(origin["seed"]["root"]["commit"], "c".repeat(40));
    assert!(!origin.to_string().contains(REPLY));
    if protected != Protected::Confirmed {
        if mouse {
            let point =
                session.wait_for_owned_completed_action(&operation.title(), "[Esc Cancel]", &[]);
            session.click(point.0, point.1);
        } else {
            session.send_key(b"\x1b");
        }
    }
    session.wait_for_new_cursorless_condition(
        before,
        |frame| {
            let text = frame.contents();
            !text.contains(THREAD_BODY)
                && !text.contains("Original native reply 界")
                && !text.contains(&operation.title())
                && text.contains(if protected == Protected::Confirmed {
                    "Thread write observed; current source unavailable"
                } else {
                    "publication canceled"
                })
        },
        "Actual original private view retires after cancel or confirmed cleanup failure",
        Duration::from_secs(8),
    );
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut last_identity_error = None;
    loop {
        let state: String = raw(&fixture)
            .query_row("SELECT state FROM pr_write_attempts", [], |row| row.get(0))
            .unwrap();
        let expected = match protected {
            Protected::Pending => "pending",
            Protected::Uncertain => "uncertain",
            Protected::Confirmed => "confirmed",
        };
        let gone = match process_identity(child.pid) {
            Ok(None) => true,
            Ok(Some(now)) => {
                (now.birth, now.uid, now.session) != (child.birth, child.uid, child.session)
            }
            // A transient teardown error is NOT absence. Preserve the same
            // original combined state/reaping budget and require definite
            // absence or an independently different complete identity.
            Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {
                last_identity_error = Some(error.to_string());
                false
            }
            Err(error) => panic!("Exact original identity read failed: {error}"),
        };
        if state == expected && gone {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Exact original child reaped and durable state observed: {last_identity_error:?}"
        );
        session.poll_output(Duration::from_millis(1));
    }
    session.quit_and_assert_restored_terminal(mouse);
    if legacy {
        // A protected pre-origin v2 record is represented without inventing an
        // old target. Existing body/ref remain; the UI must NOT materialize it.
        Connection::open(database(&fixture))
            .unwrap()
            .execute("DELETE FROM pr_write_thread_origins_v1", [])
            .unwrap();
    }
    let protected_before = protected_sql(&fixture);
    let mut restarted = launch(&fixture, mouse);
    assert!(!restarted.screen().contains("Original native reply 界"));
    fresh_summary(&mut restarted, &fixture, mouse, old_end);
    // The restarted binary genuinely discovers its local checkout again.
    // Pin the no-Git-command history boundary AFTER that read-only startup
    // and fresh Summary route, retaining the original full trace as a prefix.
    let trace = git_commands(&fixture);
    choose(&mut restarted, mouse, "[J Thread history]", b"J");
    if interactions {
        let child = held_child(&mut restarted, &fixture, old_end + 31);
        interactions_v1::pending(&mut restarted, &fixture, mouse, old_end + 32, &child);
    }
    restarted.wait_for_command_count(&fixture, old_end + 39);
    history_ready(
        &mut restarted,
        &[if legacy {
            "protected legacy"
        } else {
            "exact origin recorded"
        }],
    );
    assert!(!restarted.screen().contains("Original native reply 界"));
    // Delay only delivery of genuine PTY bytes, not the application or its
    // provider. Paging must not run ahead of the completed opened-detail view.
    let detail_opened_after = restarted.completed_cursorless_end();
    restarted.hold_observed_output_after_next_write_v1(Duration::from_millis(200));
    choose(&mut restarted, mouse, "[Enter Open]", b"\r");
    restarted.wait_for_command_count(&fixture, old_end + 47);
    restarted.wait_for_new_cursorless_condition(
        detail_opened_after,
        |frame| {
            crate::support::overlay_body(frame, HISTORY).is_some_and(|body| {
                body.contains("Recorded intent account:")
                    && body.contains("Recorded attempt #")
                    && body.contains("Inspect only")
                    && !body.contains("Exact original history read pending")
            })
        },
        "Actual completed opened history detail before any paging input",
        Duration::from_secs(8),
    );
    if legacy {
        detail_contains(&mut restarted, mouse, "Protected legacy attempt");
        assert!(!restarted.screen().contains("Original native reply 界"));
    } else {
        detail_contains(&mut restarted, mouse, "Original native reply 界");
        assert!(!restarted.screen().contains(FOREIGN));
        if interactions {
            interactions_v1::detail(&mut restarted, &fixture, mouse, old_end + 47);
        }
    }
    let detail_inputs = restarted.finish_observed_output_hold_v1();
    assert_eq!(
        crate::support::receiver_delivery_gate_v1::maximum_unacknowledged(&detail_inputs),
        1,
        "Only one actual history input before a completed viewport acknowledgement: {detail_inputs:?}"
    );
    if changed_head {
        // The current-head header is also present in the scrolled old view.
        // Matching it alone cannot acknowledge Home Top. Hold delivery of the
        // genuine next output again so this boundary is exercised causally,
        // independently of the earlier Open/paging hold and its assertions.
        let current_head = "d".repeat(40);
        let previous =
            crate::support::overlay_body(restarted.completed_cursorless_screen().unwrap(), HISTORY)
                .unwrap();
        assert!(previous.contains(&current_head));
        assert!(previous.contains("Original native reply 界"));
        let top_after = restarted.completed_cells_end();
        restarted.hold_observed_output_after_next_write_v1(Duration::from_millis(200));
        choose(&mut restarted, mouse, "[Home Top]", b"\x1b[H");
        restarted.wait_for_new_cursorless_condition(
            top_after,
            |frame| {
                crate::support::overlay_body(frame, HISTORY).is_some_and(|body| {
                    body.contains(&current_head)
                        && body.contains("Recorded attempt #")
                        && body.contains("Recorded intent account:")
                        && body.contains("Inspect only")
                        && body.contains("[Esc Back]")
                        && body.contains("[q Quit]")
                })
            },
            "New completed owned Home Top viewport before any further history paging",
            Duration::from_secs(8),
        );
        detail_contains(&mut restarted, mouse, "Historical H");
        let top_inputs = restarted.finish_observed_output_hold_v1();
        assert_eq!(
            crate::support::receiver_delivery_gate_v1::maximum_unacknowledged(&top_inputs),
            1,
            "Home Top and further paging each need their own completed acknowledgement: {top_inputs:?}"
        );
        assert_ne!(origin["seed"]["head"], json!(current_head));
    }
    if lose_account {
        let before = restarted.completed_cursorless_end();
        choose(&mut restarted, mouse, "[r Refresh]", b"r");
        restarted.wait_for_command_count(&fixture, old_end + 49);
        restarted.wait_for_new_cursorless_condition(before,|frame|{let text=frame.contents();text.contains("Original thread source unavailable")&&!text.contains("Original native reply 界")&&!text.contains("Recorded intent account")&&!text.contains(HISTORY)&&!text.contains(FOREIGN)},"Previously visible protected body then changed numeric UID refuses and erases all original private history/actors",Duration::from_secs(8));
    } else {
        choose(&mut restarted, mouse, "[Esc Back]", b"\x1b");
        history_ready(
            &mut restarted,
            &[if legacy {
                "protected legacy"
            } else {
                "exact origin recorded"
            }],
        );
        assert!(!restarted.screen().contains("Original native reply 界"));
        let before = restarted.completed_cursorless_end();
        choose(&mut restarted, mouse, "[Esc Back]", b"\x1b");
        restarted.wait_for_new_cursorless_condition(
            before,
            |frame| {
                let text = frame.contents();
                !text.contains(HISTORY)
                    && !text.contains("Original native reply 界")
                    && text.contains("[J Thread history]")
                    && text.contains(DETAIL_TITLE)
            },
            "Exact original fresh Summary restored with protected body erased and no I/O",
            Duration::from_secs(8),
        );
    }
    assert_eq!(protected_sql(&fixture), protected_before);
    assert_eq!(untouched_sql(&fixture), sql);
    assert_eq!(git_state(&fixture), git);
    assert_eq!(git_commands(&fixture), trace);
    assert_read_only_git(&fixture);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    exact(
        &restarted,
        &fixture,
        &steps,
        usize::from(protected != Protected::Pending),
    );
    restarted.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn review_threads_v1_history_keyboard_pending_restart_exact_original_body_ref_no_resend() {
    journey(false, Protected::Pending, false, false, false)
}
#[test]
fn review_threads_v1_history_mouse_pending_restart_exact_original_body_ref_no_resend() {
    journey(true, Protected::Pending, false, false, false)
}
#[test]
fn review_threads_v1_history_keyboard_uncertain_restart_inspects_without_reconciliation() {
    journey(false, Protected::Uncertain, false, false, false)
}
#[test]
fn review_threads_v1_history_mouse_uncertain_restart_inspects_without_reconciliation() {
    journey(true, Protected::Uncertain, false, false, false)
}
#[test]
fn review_threads_v1_history_keyboard_confirmed_cleanup_failure_restart_inspects_receipt_and_retained_body(
) {
    journey(false, Protected::Confirmed, false, false, false)
}
#[test]
fn review_threads_v1_history_mouse_confirmed_cleanup_failure_restart_inspects_receipt_and_retained_body(
) {
    journey(true, Protected::Confirmed, false, false, false)
}
#[test]
fn review_threads_v1_history_keyboard_legacy_protected_record_has_no_guessed_origin_or_body_open() {
    journey(false, Protected::Pending, true, false, false)
}
#[test]
fn review_threads_v1_history_mouse_legacy_protected_record_has_no_guessed_origin_or_body_open() {
    journey(true, Protected::Pending, true, false, false)
}
#[test]
fn review_threads_v1_history_keyboard_historical_head_remains_distinct_from_fresh_current_head() {
    journey(false, Protected::Pending, false, true, false)
}
#[test]
fn review_threads_v1_history_mouse_historical_head_remains_distinct_from_fresh_current_head() {
    journey(true, Protected::Pending, false, true, false)
}
#[test]
fn review_threads_v1_history_keyboard_visible_protected_body_then_numeric_account_loss_erases_private_history(
) {
    journey(false, Protected::Pending, false, false, true)
}
#[test]
fn review_threads_v1_history_mouse_visible_protected_body_then_numeric_account_loss_erases_private_history(
) {
    journey(true, Protected::Pending, false, false, true)
}
fn atomic_failure(mouse: bool) {
    let operation = Operation::Reply;
    let mut steps = initial();
    steps.extend(resource_operation(false));
    steps.extend(browse(operation));
    steps.extend(observation(operation));
    steps.extend(observation(operation));
    steps.extend(confirm(operation).into_iter().take(7));
    let fixture = resource_configured(&steps);
    let git = git_state(&fixture);
    let sql = untouched_sql(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    prepared(&mut session, &fixture, mouse, operation);
    let trace = git_commands(&fixture);
    rusqlite::Connection::open(database(&fixture)).unwrap().execute_batch("CREATE TRIGGER refuse_native_thread_origin BEFORE INSERT ON pr_write_thread_origins_v1 BEGIN SELECT RAISE(ABORT,'controlled atomic origin failure'); END;").unwrap();
    let before = session.completed_cursorless_end();
    form_control(&mut session, mouse, operation, "[Confirm action]");
    session.wait_for_command_count(&fixture, 87);
    session.wait_for_new_cursorless_condition(before,|frame|{let text=frame.contents();text.contains("Original thread source unavailable")&&!text.contains(THREAD_BODY)&&!text.contains("Original native reply 界")&&!text.contains(&operation.title())},"Actual metadata INSERT failure rolls back SAME begin before ACK/claim/POST and hides private source",Duration::from_secs(8));
    let counts:(u64,u64,i64)=raw(&fixture).query_row("SELECT (SELECT count(*) FROM pr_write_attempts),(SELECT count(*) FROM pr_write_thread_origins_v1),(SELECT COALESCE(sum(generation),0) FROM pr_write_scopes)",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
    assert_eq!(counts, (0, 0, 0));
    let body: String = raw(&fixture)
        .query_row(
            "SELECT body FROM drafts WHERE resource LIKE 'thread-draft/%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["values"]["body"], REPLY);
    assert!(body["publication"].is_null());
    exact(&session, &fixture, &steps, 0);
    assert_eq!(untouched_sql(&fixture), sql);
    assert_eq!(git_state(&fixture), git);
    assert_eq!(git_commands(&fixture), trace);
    assert_read_only_git(&fixture);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn review_threads_v1_history_keyboard_atomic_origin_failure_rolls_back_begin_before_post() {
    atomic_failure(false)
}
#[test]
fn review_threads_v1_history_mouse_atomic_origin_failure_rolls_back_begin_before_post() {
    atomic_failure(true)
}
