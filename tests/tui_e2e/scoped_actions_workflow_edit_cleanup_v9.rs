//! Actual binary / real TLS Push / canonical PR / same paid last UID. No saved
//! row, test DTO, timing assumption or direct handler can mint a cleanup seal.
use super::*;
use rusqlite::{params, types::Value as SqlValue, Connection, OpenFlags};
const KEY: &str = "scoped-actions-v9/repository/10000/workflow/7/local-edit";
type SqlState = Vec<(String, Vec<Vec<SqlValue>>)>;
#[derive(Clone, Copy, Debug)]
enum Change {
    None,
    Receipt,
    Timestamp,
    Clear,
    File,
    SqlAbort,
    Cancel,
    Database,
    AcceptedCancel,
}

struct HeldCleanupUid(ProcessIdentity);
impl HeldCleanupUid {
    fn stop(session: &Session, child: ProcessIdentity) -> Self {
        let app = process_identity(session.owned_application_pid())
            .unwrap()
            .unwrap();
        assert!(child.pid > 0 && child.pid != app.pid && !child.zombie);
        assert_eq!(
            (child.parent, child.uid, child.session),
            (app.pid, app.uid, app.session)
        );
        assert!(child.birth >= app.birth);
        assert_eq!(process_identity(child.pid).unwrap().as_ref(), Some(&child));
        // SAFETY: exact positive UID/birth/session-owned actual last-GH child.
        assert_eq!(unsafe { libc::kill(child.pid, libc::SIGSTOP) }, 0);
        Self(child)
    }
}
impl Drop for HeldCleanupUid {
    fn drop(&mut self) {
        if let Ok(Some(now)) = process_identity(self.0.pid) {
            if !now.zombie
                && (now.uid, now.session, now.birth) == (self.0.uid, self.0.session, self.0.birth)
            {
                // SAFETY: resume only that same still-owned stopped child.
                assert_eq!(unsafe { libc::kill(now.pid, libc::SIGCONT) }, 0);
            }
        }
    }
}
fn raw(f: &Fixture) -> Connection {
    Connection::open_with_flags(database(f), OpenFlags::SQLITE_OPEN_READ_WRITE).unwrap()
}
fn protected_draft(f: &Fixture, attempt: i64) -> Vec<SqlValue> {
    let db = raw(f);
    let row=db.query_row("SELECT host,login,resource,body,updated_at FROM drafts WHERE host=?1 AND login='user-id:42' AND resource=?2",params![REMOTE_HOST,KEY],|row|(0..5).map(|i|row.get(i)).collect::<rusqlite::Result<Vec<SqlValue>>>()).unwrap();
    let SqlValue::Text(body) = &row[3] else {
        panic!("Actual acknowledged reference is TEXT")
    };
    let decoded: Value = serde_json::from_str(body).unwrap();
    assert_eq!(decoded["version"], 9);
    assert_eq!(decoded["purpose"], "scoped-actions-workflow-remote-v9");
    assert_eq!(decoded["actions_publication"]["attempt_id"], attempt);
    assert!(matches!(row[4],SqlValue::Integer(n) if n>0));
    for private in [TITLE, PR_BODY, "PRIVATE_NATIVE_W9_EXACT_EDIT"] {
        assert!(!body.contains(private));
    }
    row
}
fn without_exact_draft(mut state: SqlState, row: &[SqlValue]) -> SqlState {
    let drafts = &mut state
        .iter_mut()
        .find(|(name, _)| name == "drafts")
        .unwrap()
        .1;
    let before = drafts.len();
    drafts.retain(|candidate| candidate != row);
    assert_eq!(
        drafts.len() + 1,
        before,
        "Exactly ONE actual ACK row expected for conditional cleanup"
    );
    state
}
fn failure_ready(s: &mut Session, after: usize, cleanup: bool) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > after
            && s.completed_cells_screen().is_some_and(|frame| {
                let text = frame.contents();
                crate::support::main_footer_visible(frame)
                    && crate::support::overlay_body(frame, FORM).is_none()
                    && if cleanup {
                        text.contains("Cleanup NOT applied")
                            && text.contains("receipt protected")
                            && text.contains("NEVER resend")
                    } else {
                        text.contains("Original Actions operation unavailable")
                            && text.contains("private source hidden")
                    }
                    && ![
                        TITLE,
                        PR_BODY,
                        "PRIVATE_NATIVE_W9_EXACT_EDIT",
                        "[Confirm step]",
                    ]
                    .iter()
                    .any(|word| text.contains(word))
            })
        {
            return;
        }
        assert!(Instant::now()<until,"NEW exact owned safe cleanup outcome, no hidden private result or renewed approval\n{}",s.screen());
    }
}
fn accepted_wait(s: &mut Session, after: usize) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > after
            && s.completed_cells_screen().is_some_and(|frame| {
                frame
                    .contents()
                    .contains("Cleanup accepted • ACK pending • NEVER resend")
                    && crate::support::overlay_body(frame, FORM).is_some()
                    && crate::support::completed_navigation_controls_v1::owned_action_position(
                        frame,
                        FORM,
                        "[Esc Cancel]",
                        &[],
                    )
                    .is_some()
            })
        {
            return;
        }
        assert!(
            Instant::now() < until,
            "Actual accepted SAME-owned cleanup wait, not last-GH timing\n{}",
            s.screen()
        );
    }
}
fn canceled_ready(s: &mut Session, after: usize, retained_pr_history: bool) {
    if retained_pr_history {
        remote_cells(
            s,
            after,
            &["[F4 View]", "[F6 ACK local]", "[Keep draft / Close]"],
        );
        let current = body(s).unwrap();
        assert!(!current.contains("[Confirm step]"));
        assert!(!current.contains("[Preview next step]"));
    } else {
        cells(s, after, &["[Keep draft / Close]"]);
    }
}
fn journey(mouse: bool, pr: bool, uid: u64, change: Change) {
    let mut r = remote_fixture();
    let original = git_state(&r.fixture);
    let config = fs::read(r.fixture.root.join("config/ghtui/config.toml")).unwrap();
    let git_config = fs::read(r.fixture.repo().join(".git/config")).unwrap();
    let mut s = start(&r.fixture, mouse);
    let (root, d) = actual_local_commit(&r, &mut s, mouse);
    let post_d = git_state(&r.fixture);
    assert_eq!(post_d.head, original.head);
    assert_eq!(post_d.index, original.index);
    assert_eq!(post_d.status, original.status);
    assert_eq!(post_d.staged_diff, original.staged_diff);
    assert_eq!(post_d.working_diff, original.working_diff);
    assert_eq!(post_d.dirty, original.dirty);
    assert_eq!(post_d.staged, original.staged);
    assert_eq!(post_d.untracked, original.untracked);
    let mut expected = original.refs.lines().map(str::to_owned).collect::<Vec<_>>();
    expected.push(format!("{d} refs/heads/{BRANCH}"));
    expected.sort();
    let mut actual = post_d.refs.lines().map(str::to_owned).collect::<Vec<_>>();
    actual.sort();
    assert_eq!(actual, expected);
    r.steps.extend(remote_reads(&r.c, &d, false));
    if pr {
        r.steps.extend(remote_confirm(&r.c, &d, false));
        r.steps.extend(remote_reads(&r.c, &d, true));
    }
    let mut closing = remote_confirm(&r.c, &d, pr);
    let final_uid = closing.last_mut().unwrap();
    assert_eq!(
        final_uid.args,
        vec!["api", "user", "--hostname", REMOTE_HOST, "--method", "GET"]
    );
    final_uid.stdout = serde_json::to_vec(&json!({"id":uid,"login":"octocat"})).unwrap();
    // The real SIGSTOP below, not this startup delay, owns the held boundary.
    // After SIGCONT the ordinary successful child must fit unchanged gone4s.
    final_uid.delay_ms = 1000;
    r.steps.extend(closing);
    r.fixture.scripted_gh(&r.steps);
    let mut count = 109;
    preview(&mut s, &r, mouse, &mut count, false, &d);
    if pr {
        confirm(&mut s, &r, mouse, &mut count, false);
        preview(&mut s, &r, mouse, &mut count, true, &d);
    }
    let after = s.completed_cells_end();
    action(&mut s, mouse, "[Confirm step]");
    count += if pr { 18 } else { 16 };
    let child = entered(&mut s, &r.fixture, count);
    let held = HeldCleanupUid::stop(&s, child.clone());
    let attempt = if pr { 2 } else { 1 };
    let protected = protected_draft(&r.fixture, attempt);
    let rows = journal(&r.fixture);
    assert_eq!(rows.len(), attempt as usize);
    assert!(rows.iter().all(|row| row.1 == "confirmed"));
    positive_aliases(&r, attempt as usize);
    assert_eq!(
        bare_git(&r.tls, &["rev-parse", "refs/heads/workflow/edit"]).trim(),
        d
    );
    assert_eq!(
        bare_git(&r.tls, &["rev-parse", "refs/heads/main"]).trim(),
        r.c
    );
    assert_eq!(
        commands(&s, &r.fixture).len(),
        count,
        "Paid final UID is the exact last child, not a prewrite UID"
    );
    let before = sql(&r.fixture);
    let mut expected_file = EDIT.as_bytes().to_vec();
    let mut replaced_database = None;
    match change {
        Change::Receipt => {
            assert_eq!(raw(&r.fixture).execute("UPDATE actions_write_attempts SET receipt_json=receipt_json||' ' WHERE attempt_id=?1",[attempt]).unwrap(),1);
        }
        Change::Timestamp => {
            assert_eq!(raw(&r.fixture).execute("UPDATE drafts SET updated_at=updated_at+1 WHERE host=?1 AND login='user-id:42' AND resource=?2",params![REMOTE_HOST,KEY]).unwrap(),1);
        }
        Change::Clear => {
            Cache::open(&database(&r.fixture))
                .unwrap()
                .clear_scope(&Scope::github(REMOTE_HOST, Some(42)).unwrap(), false)
                .unwrap();
        }
        Change::File => {
            expected_file = b"PRIVATE_NEWER_WORKFLOW_MUST_REMAIN\n".to_vec();
            fs::write(root.join(".github/workflows/ci.yml"), &expected_file).unwrap();
        }
        Change::SqlAbort => {
            raw(&r.fixture).execute_batch("CREATE TRIGGER w9_cleanup_test_abort BEFORE DELETE ON drafts WHEN OLD.resource='scoped-actions-v9/repository/10000/workflow/7/local-edit' BEGIN SELECT RAISE(ABORT,'PRIVATE_SQL_DIAGNOSTIC_NOT_DISPLAYED'); END;").unwrap();
        }
        Change::Database => {
            let database = database(&r.fixture);
            let old = database.with_file_name("actual-original-workflow-cleanup.sqlite3");
            fs::rename(&database, &old).unwrap();
            fs::copy(&old, &database).unwrap();
            // Both are independently real files with exactly mirrored rows.
            // The replacement cannot substitute for original issuer identity.
            let bytes = fs::read(&old).unwrap();
            assert_eq!(fs::read(&database).unwrap(), bytes);
            replaced_database = Some((old, bytes));
        }
        Change::None | Change::Cancel | Change::AcceptedCancel => {}
    }
    let expected_sql = sql(&r.fixture);
    assert_eq!(
        protected_draft(&r.fixture, attempt)[3],
        protected[3],
        "No cleanup while exact last UID is stopped"
    );
    let canceled = matches!(change, Change::Cancel | Change::AcceptedCancel);
    if matches!(change, Change::Cancel) {
        // Actual Cancel, not a timeout or fabricated completion. We make NO
        // claim that this precedes/follows publication queue acceptance.
        let point = s.wait_for_owned_completed_action(FORM, "[Esc Cancel]", &[]);
        if mouse {
            s.click(point.0, point.1);
        } else {
            s.send_key(b"\x1b");
        }
    }
    // Acquire only AFTER the real reference Save, remote receipt, complete
    // pre-UID raw preparation, and while the actual paid final UID is held.
    // Locking earlier would merely block Save and prove no cleanup acceptance.
    let lock = matches!(change, Change::AcceptedCancel).then(|| {
        let db = raw(&r.fixture);
        db.execute_batch("BEGIN IMMEDIATE").unwrap();
        db
    });
    let before_release = s.completed_cells_end();
    drop(held);
    gone(&mut s, &child);
    if let Some(lock) = lock {
        accepted_wait(&mut s, before_release);
        assert_eq!(sql(&r.fixture), expected_sql);
        assert_eq!(protected_draft(&r.fixture, attempt), protected);
        let current = s.completed_cells_end();
        let point = s.wait_for_owned_completed_action(FORM, "[Esc Cancel]", &[]);
        if mouse {
            s.click(point.0, point.1);
        } else {
            s.send_key(b"\x1b");
        }
        canceled_ready(&mut s, current, pr);
        assert!(s
            .completed_cells_screen()
            .unwrap()
            .contents()
            .contains("Cleanup ACK UNKNOWN"));
        assert_eq!(sql(&r.fixture), expected_sql);
        // The genuine accepted worker can now settle its refusal. No ACK or
        // operational Branch can be inferred from this caller's cancellation.
        lock.execute_batch("ROLLBACK").unwrap();
    }
    let removed = uid == 42 && matches!(change, Change::None);
    if removed {
        remote_cells(
            &mut s,
            after,
            &[if pr {
                "Actual canonical PR #91 observed after HTTP201"
            } else {
                "Exact newly published branch D observed"
            }],
        );
        assert_eq!(
            sql(&r.fixture),
            without_exact_draft(before, &protected),
            "Only the exact ACK body+timestamp row is removed after UID42"
        );
        if pr {
            let stable = sql(&r.fixture);
            let after = s.completed_cells_end();
            s.send_key(b"\x10");
            remote_cells(
                &mut s,
                after,
                &["read only", "never renew Preview or resend"],
            );
            assert_eq!(sql(&r.fixture), stable);
        }
        close(&mut s, mouse);
    } else if canceled {
        canceled_ready(&mut s, after, pr);
        if pr {
            let count_before = commands(&s, &r.fixture).len();
            let current = s.completed_cells_end();
            s.send_key(b"\x10");
            remote_cells(&mut s, current, &["read only", "[F6 ACK local]"]);
            assert_eq!(commands(&s, &r.fixture).len(), count_before);
        } else {
            let frame = s.completed_cells_screen().unwrap();
            let (x, y) = crate::support::completed_navigation_controls_v1::owned_action_position(
                frame,
                FORM,
                "[Confirm step]",
                &[],
            )
            .unwrap();
            assert!(
                frame.cell(y, x).is_some_and(|cell| !cell.underline()),
                "Canceled attempt cannot renew approval"
            );
        }
        assert_eq!(
            sql(&r.fixture),
            expected_sql,
            "Canceled original cannot clean draft or mint a next step"
        );
        close(&mut s, mouse);
    } else {
        failure_ready(&mut s, after, uid == 42);
        assert_eq!(
            sql(&r.fixture),
            expected_sql,
            "Every deliberate raw/source/file preimage is preserved, never silently ACKed"
        );
        assert_eq!(protected_draft(&r.fixture, attempt)[3], protected[3]);
    }
    let stable = sql(&r.fixture);
    assert_eq!(git_state(&r.fixture), post_d);
    assert_eq!(
        fs::read(root.join(".github/workflows/ci.yml")).unwrap(),
        expected_file
    );
    assert_eq!(
        fs::read(r.fixture.repo().join(".git/config")).unwrap(),
        git_config
    );
    assert_eq!(
        fs::read(r.fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    assert_eq!(
        r.fixture
            .git(&["-C", root.to_str().unwrap(), "rev-parse", "HEAD"])
            .trim(),
        d
    );
    exact_remote(&s, &r.fixture, &r.steps);
    s.capture_completed_terminal_v1("task54-w9-final-uid-exact-cleanup");
    s.quit_and_assert_restored_terminal(mouse);
    assert_eq!(sql(&r.fixture), stable);
    if let Some((old, bytes)) = replaced_database {
        assert_eq!(
            fs::read(old).unwrap(),
            bytes,
            "Original renamed database also unchanged"
        );
    }
    assert_eq!(git_state(&r.fixture), post_d);
    exact_remote(&s, &r.fixture, &r.steps);
    let posts = commands(&s, &r.fixture)
        .into_iter()
        .filter(|row| {
            row["args"]
                .as_array()
                .unwrap()
                .iter()
                .any(|arg| arg == "POST")
        })
        .count();
    assert_eq!(
        posts,
        usize::from(pr),
        "One CreatePR maximum, never a cleanup retry or second write"
    );
    cleanup(r.tls);
}
#[test]
fn keyboard_task54_workflow_edit_v9_cleanup_branch_holds_exact_draft_until_uid42() {
    journey(false, false, 42, Change::None)
}
#[test]
fn mouse_task54_workflow_edit_v9_cleanup_branch_holds_exact_draft_until_uid42() {
    journey(true, false, 42, Change::None)
}
#[test]
fn keyboard_task54_workflow_edit_v9_cleanup_branch_uid43_keeps_receipt_and_draft() {
    journey(false, false, 43, Change::None)
}
#[test]
fn mouse_task54_workflow_edit_v9_cleanup_branch_uid43_keeps_receipt_and_draft() {
    journey(true, false, 43, Change::None)
}
#[test]
fn keyboard_task54_workflow_edit_v9_cleanup_pr_holds_exact_draft_until_uid42() {
    journey(false, true, 42, Change::None)
}
#[test]
fn mouse_task54_workflow_edit_v9_cleanup_pr_holds_exact_draft_until_uid42() {
    journey(true, true, 42, Change::None)
}
#[test]
fn keyboard_task54_workflow_edit_v9_cleanup_pr_uid43_keeps_receipt_and_draft() {
    journey(false, true, 43, Change::None)
}
#[test]
fn mouse_task54_workflow_edit_v9_cleanup_pr_uid43_keeps_receipt_and_draft() {
    journey(true, true, 43, Change::None)
}
#[test]
fn keyboard_task54_workflow_edit_v9_cleanup_exact_raw_timestamp_clear_and_file_cas() {
    for change in [
        Change::Receipt,
        Change::Timestamp,
        Change::Clear,
        Change::File,
    ] {
        journey(false, false, 42, change)
    }
}
#[test]
fn mouse_task54_workflow_edit_v9_cleanup_exact_raw_timestamp_clear_and_file_cas() {
    for change in [
        Change::Receipt,
        Change::Timestamp,
        Change::Clear,
        Change::File,
    ] {
        journey(true, false, 42, change)
    }
}
#[test]
fn keyboard_task54_workflow_edit_v9_cleanup_sql_abort_and_cancel_do_not_grant_branch() {
    for change in [Change::SqlAbort, Change::Cancel] {
        journey(false, false, 42, change)
    }
}
#[test]
fn mouse_task54_workflow_edit_v9_cleanup_sql_abort_and_cancel_do_not_grant_branch() {
    for change in [Change::SqlAbort, Change::Cancel] {
        journey(true, false, 42, change)
    }
}
#[test]
fn keyboard_task54_workflow_edit_v9_cleanup_actual_accepted_no_ack_cancel_is_unknown() {
    for pr in [false, true] {
        journey(false, pr, 42, Change::AcceptedCancel)
    }
}
#[test]
fn mouse_task54_workflow_edit_v9_cleanup_actual_accepted_no_ack_cancel_is_unknown() {
    for pr in [false, true] {
        journey(true, pr, 42, Change::AcceptedCancel)
    }
}
#[test]
fn keyboard_task54_workflow_edit_v9_cleanup_mirrored_database_replacement_preserves_both() {
    journey(false, false, 42, Change::Database)
}
#[test]
fn mouse_task54_workflow_edit_v9_cleanup_mirrored_database_replacement_preserves_both() {
    journey(true, false, 42, Change::Database)
}
