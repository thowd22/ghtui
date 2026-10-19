//! Abrupt real-process recovery. Persisted facts, never screenshots, decide effect state.
use super::*;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Interrupted {
    Pending,
    Uncertain,
    Accepted,
    ReceiptSqlFailure,
}
impl Interrupted {
    fn state(self) -> &'static str {
        match self {
            Self::Pending | Self::ReceiptSqlFailure => "pending",
            Self::Uncertain => "uncertain",
            Self::Accepted => "accepted_pending",
        }
    }
}
#[test]
fn actions_restart_keyboard_sigkill_claimed_pending_original_and_no_resend() {
    journey(false, Interrupted::Pending);
}
#[test]
fn actions_restart_mouse_sigkill_claimed_pending_original_and_no_resend() {
    journey(true, Interrupted::Pending);
}
#[test]
fn actions_restart_keyboard_sigkill_uncertain_original_and_no_resend() {
    journey(false, Interrupted::Uncertain);
}
#[test]
fn actions_restart_mouse_sigkill_uncertain_original_and_no_resend() {
    journey(true, Interrupted::Uncertain);
}
#[test]
fn actions_restart_keyboard_sigkill_accepted_pending_requires_fresh_transition() {
    journey(false, Interrupted::Accepted);
}
#[test]
fn actions_restart_mouse_sigkill_accepted_pending_requires_fresh_transition() {
    journey(true, Interrupted::Accepted);
}
#[test]
fn actions_restart_keyboard_receipt_sql_failure_keeps_claimed_original_barrier() {
    journey(false, Interrupted::ReceiptSqlFailure);
}
#[test]
fn actions_restart_mouse_receipt_sql_failure_keeps_claimed_original_barrier() {
    journey(true, Interrupted::ReceiptSqlFailure);
}

fn sql(database: &Path) -> rusqlite::Connection {
    let connection = rusqlite::Connection::open(database).unwrap();
    connection
        .busy_timeout(std::time::Duration::from_secs(2))
        .unwrap();
    connection
}
fn assert_original(database: &Path, state: &str, accepted: bool) {
    let rows = publication_support::durable_rows(database);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].state, state);
    assert_eq!(rows[0].principal, "user-id:42");
    assert_eq!(rows[0].kind, json!("cancel"));
    assert_eq!(rows[0].context["run_id"], 123);
    assert_eq!(rows[0].context["attempt"], 2);
    assert_eq!(rows[0].context["head_sha"], "a".repeat(40));
    assert_eq!(rows[0].target["Run"]["id"], 123);
    assert_eq!(rows[0].receipt.is_some(), accepted);
    if accepted {
        assert_eq!(rows[0].receipt.as_ref().unwrap()["kind"], "run_transition");
        assert_eq!(rows[0].receipt.as_ref().unwrap()["observed_attempt"], 2);
    }
    let connection = sql(database);
    assert_eq!(
        connection
            .query_row(
                "SELECT claimed FROM actions_write_attempts WHERE attempt_id=1",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    let bodies: Vec<String> = connection
        .prepare("SELECT body FROM drafts WHERE host='github.invalid' AND login='user-id:42'")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(
        bodies.iter().any(|body| {
            let draft: Value = serde_json::from_str(body).unwrap();
            draft["version"] == 3
                && draft["purpose"] == "actions"
                && draft["actions_publication"]["attempt_id"] == 1
        }),
        "Original attempt reference was not durably saved before claim"
    );
}
fn open_run(session: &mut Session, mouse: bool) {
    session.wait_for_screen("ghtui");
    choose(session, mouse, "Actions", b"4");
    session.wait_for_screen(&run_cell(123, 2));
    if mouse {
        session.click_visible_action(&run_cell(123, 2));
    } else {
        session.send_key(b"\r");
    }
    session.wait_for_screen("Run 123 • attempt 2");
}
fn assert_one_cancel(rows: &[Value]) {
    let writes: Vec<_> = rows.iter().filter(|row| row["args"][5] == "POST").collect();
    assert_eq!(
        writes.len(),
        1,
        "Restart/inspect/ACK/View original must never resend"
    );
    assert_eq!(
        writes[0]["args"][1],
        "repos/example/repo/actions/runs/123/cancel"
    );
    assert_eq!(writes[0]["stdin_hex"], "");
    assert!(rows.iter().all(|row| row["matched"] == true));
}
fn wait_original_ready(session: &mut Session) {
    // The command journal records invocation before its response is consumed.
    // Independently fence the actual prepared, non-submitting form; old status
    // text alone would accept a stale frame and lose an input during the worker.
    session.poll_output(std::time::Duration::from_millis(80));
    session
        .try_wait_for_condition(
            |screen| {
                screen.contains("ORIGINAL ACTIONS ATTEMPT #1")
                    && screen.contains("[Keep draft / Close]")
                    && !screen.contains("[Esc Cancel]")
                    && !screen.contains("Loading Actions")
            },
            "Fresh original worker completed and visible controls are enabled",
            std::time::Duration::from_secs(8),
        )
        .unwrap();
}
fn journey(mouse: bool, interruption: Interrupted) {
    let fixture = fixture();
    let database = fixture.root.join("config/ghtui/cache.sqlite3");
    let unchanged_head = fixture.git(&["rev-parse", "HEAD"]);
    let unchanged_index = fixture.git(&["ls-files", "--stage"]);
    let queued = publication_support::run(2, "queued", None);
    let mut steps = publication_support::initial(queued.clone());
    steps.extend(publication_support::open_attempt(&queued, vec![]));
    steps.extend(compose_identity_steps());
    steps.extend(current_account());
    steps.extend(run_guard(queued.clone()));
    let mut invocation = write(
        "repos/example/repo/actions/runs/123/cancel",
        None,
        Value::Null,
    );
    if interruption == Interrupted::Pending {
        invocation.delay_ms = 20_000; // SIGKILL while the actual owned gh child is executing.
    } else if interruption == Interrupted::Uncertain {
        invocation.status = 1;
        invocation.stderr = b"fixture transport interrupted after request; result unknown".to_vec();
    }
    steps.push(invocation);
    if interruption != Interrupted::Pending {
        if interruption != Interrupted::Uncertain {
            let observed = if interruption == Interrupted::ReceiptSqlFailure {
                publication_support::run(2, "completed", Some("cancelled"))
            } else {
                queued.clone()
            };
            steps.push(read("repos/example/repo/actions/runs/123", observed, false));
        }
        steps.extend(current_account());
    }
    let interrupted_count = steps.len();
    steps.extend(publication_support::initial(queued.clone()));
    steps.extend(publication_support::open_attempt(&queued, vec![]));
    let restarted_count = steps.len();
    steps.extend(compose_identity_steps());
    steps.extend(publication_support::inspect_run(Some(queued.clone()), 42));
    let inspected_count = steps.len();
    let mut observed = queued;
    if interruption == Interrupted::Accepted {
        // A different later completion allows explicit retirement, not causal confirmation.
        observed = publication_support::run(2, "completed", Some("failure"));
        steps.extend(compose_identity_steps());
        steps.extend(publication_support::inspect_run(Some(observed.clone()), 42));
    }
    let eligible_count = steps.len();
    steps.extend(current_account());
    steps.push(publication_support::catalogue(vec![observed.clone()], 1));
    steps.extend(current_account());
    let acknowledged_count = steps.len();
    let reopened = observed;
    steps.push(publication_support::catalogue(vec![reopened.clone()], 1));
    steps.extend(publication_support::open_attempt(&reopened, vec![]));
    // OPEN original6, Preview original4, Submit/View original4. No new plan or write.
    steps.extend(current_account());
    steps.extend(compose_identity_steps());
    let restored_count = steps.len();
    steps.extend(compose_identity_steps());
    let preview_count = steps.len();
    steps.extend(compose_identity_steps());
    let final_count = steps.len();
    fixture.scripted_gh(&steps);
    let mut first = Session::launch(&fixture, 120, 40);
    open_run(&mut first, mouse);
    command(&mut first, mouse, "Cancel run");
    first.wait_for_screen("ACTIONS INTENT");
    choose(&mut first, mouse, "[Preview]", b"\x10");
    first.wait_for_screen("READ-ONLY PREVIEW");
    if interruption == Interrupted::ReceiptSqlFailure {
        sql(&database).execute_batch("CREATE TRIGGER reject_actions_receipt BEFORE UPDATE OF receipt_json ON actions_write_attempts WHEN NEW.receipt_json IS NOT NULL BEGIN SELECT RAISE(ABORT,'offline injected receipt save failure'); END;").unwrap();
    }
    confirm(&mut first, mouse, 0);
    if interruption == Interrupted::Uncertain {
        first.wait_for_screen("result unknown");
    }
    if interruption == Interrupted::Accepted {
        first.wait_for_screen("Request accepted; completion not confirmed");
    }
    if interruption == Interrupted::ReceiptSqlFailure {
        first.wait_for_screen("Actions result receipt unavailable");
    }
    let calls = journal(&mut first, &fixture, interrupted_count);
    assert_one_cancel(&calls);
    assert_original(
        &database,
        interruption.state(),
        interruption == Interrupted::Accepted,
    );
    first.terminate_abruptly();
    if interruption == Interrupted::Pending {
        let pid = calls.last().unwrap()["pid"].as_i64().unwrap();
        let pid = i32::try_from(pid).unwrap();
        assert!(pid > 0);
        // SAFETY: this is the exact still-delayed fixture child recorded for our only POST.
        let result = unsafe { libc::kill(pid, libc::SIGKILL) };
        assert!(result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH));
    }
    assert_original(
        &database,
        interruption.state(),
        interruption == Interrupted::Accepted,
    );
    let mut restarted = Session::launch(&fixture, 120, 40);
    open_run(&mut restarted, mouse);
    assert_one_cancel(&journal(&mut restarted, &fixture, restarted_count));
    command(&mut restarted, mouse, "Cancel run");
    restarted.wait_for_screen("Action unavailable");
    restarted.wait_for_screen("Original attempt #1");
    assert!(!restarted.screen().contains("ACTIONS INTENT"));
    choose(&mut restarted, mouse, "[Esc Close]", b"\x1b");
    restarted
        .try_wait_for_condition(
            |screen| !screen.contains("Action unavailable"),
            "Restart barrier explanation closed",
            std::time::Duration::from_secs(8),
        )
        .unwrap();
    inspect_original_publication(&mut restarted, mouse, 1);
    restarted.wait_for_screen("Original run: #123");
    restarted.wait_for_screen("attempt 2");
    assert_one_cancel(&journal(&mut restarted, &fixture, inspected_count));
    assert_original(
        &database,
        interruption.state(),
        interruption == Interrupted::Accepted,
    );
    if interruption == Interrupted::Accepted {
        restarted.wait_for_screen("ACCEPTED REQUEST STILL WAITING");
        assert!(restarted
            .completed_actions_control("Acknowledge inspected state")
            .is_none());
        restarted.resize(80, 24);
        restarted.wait_for_screen("Inspect ORIGINAL Actions attempt #1");
        restarted.resize(120, 40);
        inspect_original_publication(&mut restarted, mouse, 1);
        restarted.wait_for_screen("FRESH LATER TRANSITION");
        assert_one_cancel(&journal(&mut restarted, &fixture, eligible_count));
    }
    command(&mut restarted, mouse, "Acknowledge inspected state");
    restarted.wait_for_screen("Original affected state acknowledged");
    assert_one_cancel(&journal(&mut restarted, &fixture, acknowledged_count));
    let expected = if interruption == Interrupted::Accepted {
        "transition_observed"
    } else {
        "acknowledged"
    };
    assert_original(&database, expected, interruption == Interrupted::Accepted);
    choose(&mut restarted, mouse, "Runs", b"R");
    restarted.wait_for_screen(&run_cell(123, 2));
    if mouse {
        restarted.click_visible_action(&run_cell(123, 2));
    } else {
        restarted.send_key(b"\r");
    }
    restarted.wait_for_screen("Run 123 • attempt 2");
    // Completed accepted outcome no longer permits cancellation. A different
    // command on this same immutable draft still restores the original Cancel.
    command(
        &mut restarted,
        mouse,
        if interruption == Interrupted::Accepted {
            "Rerun all jobs"
        } else {
            "Cancel run"
        },
    );
    restarted.wait_for_screen("ORIGINAL ACTIONS ATTEMPT #1");
    restarted.wait_for_screen("Original run: #123");
    assert_one_cancel(&journal(&mut restarted, &fixture, restored_count));
    // OPEN restores the original already in preview. First Preview is a local
    // toggle; only explicitly opening it again launches the fresh worker read.
    choose(&mut restarted, mouse, "[Preview]", b"\x10");
    restarted
        .try_wait_for_condition(
            |screen| !screen.contains("ORIGINAL ACTIONS ATTEMPT #1"),
            "Original preview closed locally before fresh preview",
            std::time::Duration::from_secs(8),
        )
        .unwrap();
    assert_one_cancel(&journal(&mut restarted, &fixture, restored_count));
    choose(&mut restarted, mouse, "[Preview]", b"\x10");
    restarted.wait_for_screen("Original Actions attempt retained");
    assert_one_cancel(&journal(&mut restarted, &fixture, preview_count));
    wait_original_ready(&mut restarted);
    if mouse {
        restarted.click_visible_action("[View original]");
    } else {
        restarted.focus_visible_form_action("[View original]");
        restarted.send_key(b"\r");
    }
    restarted.wait_for_screen("Original Actions attempt retained");
    assert_one_cancel(&journal(&mut restarted, &fixture, final_count));
    wait_original_ready(&mut restarted);
    choose(&mut restarted, mouse, "[F9 New intent]", b"\x1b[20~");
    // ACTIONS INTENT is also visible in the preceding original-reference
    // frame. Wait for the genuine new local form: View original becomes
    // Confirm action, shifting the later Close hit by one terminal column.
    // A matched old heading is not acknowledgement that F9 was processed.
    restarted
        .try_wait_for_condition(
            |screen| {
                screen.contains("ACTIONS INTENT")
                    && screen.contains("New local draft only • original durable protection remains")
                    && screen.contains("[Confirm action]")
                    && screen.contains("[Keep draft / Close]")
                    && !screen.contains("[View original]")
                    && !screen.contains("ORIGINAL ACTIONS ATTEMPT #1")
            },
            "New local intent is rendered before locating its Close control",
            std::time::Duration::from_secs(8),
        )
        .unwrap_or_else(|error| {
            restarted.save_artifacts();
            panic!("{error}; artifacts: {}", restarted.artifact_dir.display());
        });
    close_form(&mut restarted, mouse);
    assert_one_cancel(&journal(&mut restarted, &fixture, final_count));
    let retained = publication_support::durable_rows(&database);
    assert_eq!(retained.len(), 1);
    assert_eq!(retained[0].state, expected);
    assert_eq!(retained[0].context["attempt"], 2);
    assert_eq!(
        retained[0].receipt.is_some(),
        interruption == Interrupted::Accepted
    );
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), unchanged_head);
    assert_eq!(fixture.git(&["ls-files", "--stage"]), unchanged_index);
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    restarted.resize(80, 24);
    restarted.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn actions_restart_keyboard_sql_view_failure_selection_keeps_remote_disabled_local_filter_available(
) {
    unavailable_selection(false);
}
#[test]
fn actions_restart_mouse_sql_view_failure_selection_keeps_remote_disabled_local_filter_available() {
    unavailable_selection(true);
}
fn unavailable_selection(mouse: bool) {
    use ghtui::{
        actions_write_journal::{
            ActionsImmutableContext, ActionsSafeOptions, ActionsWriteIntent, ActionsWriteJournal,
            ActionsWriteKind, ActionsWriteTarget,
        },
        providers::gh::ActionsTarget,
        storage::Scope,
    };
    let fixture = fixture();
    let database = fixture.root.join("config/ghtui/cache.sqlite3");
    let mut publication = ActionsWriteJournal::open(&database).unwrap();
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    let target = ActionsWriteTarget::from_scope(
        &scope,
        &ActionsTarget::Cache {
            repository: "example/repo".into(),
            id: 71,
        },
    )
    .unwrap();
    let intent = ActionsWriteIntent::new(
        ActionsWriteKind::DeleteCache,
        ActionsImmutableContext::default(),
        ActionsSafeOptions::default(),
        b"",
    )
    .unwrap();
    let permit = publication.begin(&target, &intent).unwrap();
    assert_eq!(permit.marker().attempt_id, 1);
    drop(publication);
    let original_head = fixture.git(&["rev-parse", "HEAD"]);
    let cache = |id, key| json!({"id":id,"key":key,"ref":"refs/heads/main","version":"v1","size_in_bytes":1024,"created_at":"2026-10-13T12:00:00Z","last_accessed_at":"2026-10-13T12:01:00Z"});
    let catalog = || {
        read(
            "repos/example/repo/actions/caches?per_page=100",
            json!([{"actions_caches":[cache(71,"protected-cache"),cache(72,"unrelated-cache")]}]),
            true,
        )
    };
    let usage = || {
        read(
            "repos/example/repo/actions/cache/usage",
            json!({"active_caches_count":2,"active_caches_size_in_bytes":2048}),
            false,
        )
    };
    let mut steps = initial();
    steps.extend([catalog(), usage()]);
    steps.extend(compose_identity_steps());
    let first_open_count = steps.len();
    steps.extend(current_account());
    steps.extend([catalog(), usage()]);
    steps.extend(current_account());
    let unavailable_count = steps.len();
    steps.extend(compose_identity_steps());
    let final_count = steps.len();
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "Actions", b"4");
    session.wait_for_screen(&run_cell(123, 2));
    choose(&mut session, mouse, "Caches", b"c");
    session.wait_for_screen("#71 • protected-cache");
    command(&mut session, mouse, "Filter caches");
    session.wait_for_screen("ACTIONS INTENT");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    journal(&mut session, &fixture, first_open_count);
    // Actual isolated SQLite SELECT failure, not a synthetic UI warning. The
    // original bytes remain in the renamed column and are restored for audit.
    sql(&database).execute_batch("ALTER TABLE actions_write_attempts RENAME COLUMN context_json TO unavailable_context_json;").unwrap();
    confirm(&mut session, mouse, 2);
    session.wait_for_screen("publication view unavailable");
    session.wait_for_screen("#72 • unrelated-cache");
    journal(&mut session, &fixture, unavailable_count);
    for (id, key) in [
        (72, "unrelated-cache"),
        (71, "protected-cache"),
        (72, "unrelated-cache"),
    ] {
        let label = format!("#{id} • {key}");
        if mouse {
            session.click_visible_action(&label);
        } else {
            session.send_key(b"\x1b[H");
            if id == 72 {
                session.send_key(b"\x1b[B");
            }
        }
        // Cache selection independently visible after the failed publication SQL view.
        wait_selected(&mut session, &label);
        let before_dialog = session.completed_cursorless_end();
        command(&mut session, mouse, "Delete selected cache");
        wait_unavailable_dialog(&mut session, before_dialog);
        assert!(!session.screen().contains("ACTIONS INTENT"));
        choose(&mut session, mouse, "[Esc Close]", b"\x1b");
        session
            .try_wait_for_condition(
                |screen| !screen.contains("Action unavailable"),
                "Unavailable view explanation closed",
                std::time::Duration::from_secs(8),
            )
            .unwrap();
        journal(&mut session, &fixture, unavailable_count);
    }
    // The local-only composer stays usable even though remote permission cannot
    // be established. Cancelling its preview has no storage or Git publication.
    command(&mut session, mouse, "Filter caches");
    session.wait_for_screen("ACTIONS INTENT");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    close_form(&mut session, mouse);
    let calls = journal(&mut session, &fixture, final_count);
    assert!(calls
        .iter()
        .all(|row| row["args"][0] == "--version" || row["args"][5] == "GET"));
    sql(&database).execute_batch("ALTER TABLE actions_write_attempts RENAME COLUMN unavailable_context_json TO context_json;").unwrap();
    let durable = publication_support::durable_rows(&database);
    assert_eq!(durable.len(), 1);
    assert_eq!(durable[0].state, "pending");
    assert_eq!(durable[0].target["Cache"]["id"], 71);
    assert!(durable[0].receipt.is_none());
    assert_eq!(
        sql(&database)
            .query_row(
                "SELECT claimed FROM actions_write_attempts WHERE attempt_id=1",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    session.resize(80, 24);
    if mouse {
        // Split genuine observed bytes immediately after the new dialog's
        // footer text, before its completed-frame boundary. No synthetic frame
        // or input is injected; require Close to wait for the actual full box.
        // The diff renderer skips a cell that already holds the same symbol
        // and theme style (blank cells now take the theme background), so the
        // space inside "[Esc Close]" need not be re-sent; "Close]" is the
        // label's contiguous tail.
        session.hold_observed_output_after_literal_v1("Close]");
    }
    let before_dialog = session.completed_cursorless_end();
    command(&mut session, mouse, "Delete selected cache");
    wait_unavailable_dialog(&mut session, before_dialog);
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    if mouse {
        let writes = session.finish_observed_output_hold_v1();
        assert!(writes.len() >= 2);
        let count = writes.len();
        assert!(
            writes[count - 1].completed_cells_end > writes[count - 2].completed_cells_end,
            "Close input must follow a NEW complete dialog frame, not its partial footer text"
        );
    }
    session
        .try_wait_for_condition(
            |screen| !screen.contains("Action unavailable"),
            "Narrow unavailable view explanation closed",
            std::time::Duration::from_secs(8),
        )
        .unwrap();
    journal(&mut session, &fixture, final_count);
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), original_head);
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    session.quit_and_assert_restored_terminal(mouse);
}

fn wait_unavailable_dialog(session: &mut Session, after: usize) {
    // A title or footer fragment is not a new completed overlay. In particular,
    // do not let button() page the catalogue or click a partially painted Close.
    session.wait_for_new_cursorless_condition(
        after,
        |frame| {
            crate::support::overlay_body(frame, "Action unavailable").is_some_and(|body| {
                body.contains("Delete selected cache:")
                    && body.contains("Publication view unavailable")
                    && body.contains("[Esc Close]")
            })
        },
        "Fresh owned unavailable explanation and Close finish painting before input",
        std::time::Duration::from_secs(8),
    );
}
