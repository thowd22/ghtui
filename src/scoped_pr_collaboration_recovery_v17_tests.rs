//! Genuine denied invocation -> original atomic journal origin/body -> same
//! Core inspector. Exact final-UID and actual BEGIN IMMEDIATE lock intervals.
use super::*;
use crate::owned_pty_processes_test_support as process;
use crate::{
    personal_pr_entry_v13::tests::collaboration_v17::protocol::{
        phase,
        publication::{exact_protocol, execute, prepared, receipts},
    },
    personal_pr_entry_v13::tests::{self as fixture, Setup},
    pr_actions::PrCommand,
    pr_collaboration_origin_v17::PrCollaborationOpenIntentV17,
    providers::gh::ConversationRecoveryCurrentV17,
    scoped_pr_collaboration_source_v17::with_history_read_v17,
    scoped_pr_collaboration_ui_v17 as ui,
    scoped_pr_observed_tabs_v1::resources::ResourceLocalGuard,
};
use conversation_history_v17::ConversationRecoveryWitnessV17;
use std::sync::{mpsc, Arc};

fn seeded(
    held: bool,
) -> (
    Setup,
    Arc<PrCollaborationOpenIntentV17>,
    PrWriteTarget,
    PrPublicationReference,
    String,
) {
    seeded_with_tail(held, Vec::new())
}
fn seeded_with_tail(
    held: bool,
    extra: Vec<crate::test_support::GhStep>,
) -> (
    Setup,
    Arc<PrCollaborationOpenIntentV17>,
    PrWriteTarget,
    PrPublicationReference,
    String,
) {
    let command = PrCommand::NewComment;
    let mut tail = phase(command, false, Some(true));
    let mut denied = receipts(command).remove(0);
    denied.status = 1;
    denied.stdout.clear();
    denied.stderr = b"HTTP 403: explicit uncertain fixture".to_vec();
    tail.push(denied);
    let mut inspect = phase(command, false, Some(true));
    inspect.insert(
        4,
        fixture::get(
            &format!(
                "repos/{}/issues/7/comments?per_page=100&page=1",
                fixture::REPO
            ),
            serde_json::json!([]),
        ),
    );
    if held {
        inspect.last_mut().unwrap().delay_ms = 2_000;
    }
    tail.extend(inspect);
    tail.extend(extra);
    let (s, form, preview) = prepared(command, tail);
    let request = ui::request(&form).unwrap();
    let original = request.intent().source.opening().clone();
    let target = PrWriteTarget::new(
        request.intent().source.identity(),
        request.intent().source.parent().summary(),
    )
    .unwrap();
    let outcome = execute(&s, &form, &preview);
    assert!(matches!(
        outcome.state,
        crate::scoped_pr_collaboration_plan_v17::ConversationPublicationStateV17::Uncertain
    ));
    let body = Cache::open(&fixture::database(&s.fixture))
        .unwrap()
        .draft(target.scope(), &form.resource())
        .unwrap()
        .unwrap();
    (s, original, target, outcome.reference, body)
}
#[path = "scoped_pr_collaboration_lineage_v18_tests.rs"]
mod lineage_v18;
fn inspection(
    original: Arc<PrCollaborationOpenIntentV17>,
    api: &GithubProvider,
    path: &Path,
    reference: &PrPublicationReference,
    cancel: &CancellationToken,
    deadline: std::time::Instant,
) -> (
    ConversationRecoveryWitnessV17,
    ConversationRecoveryCurrentV17,
) {
    let bounded = api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    let ((witness, observed, _, _, _, _), _) = with_history_read_v17(
        original,
        &bounded,
        path,
        cancel,
        deadline,
        |read| {
            let target = PrWriteTarget::new(read.identity, read.parent.summary()).unwrap();
            let journal = PrWriteJournal::open(path).unwrap();
            let snapshot = journal
                .conversation_history_v17(&target, read.parent)
                .unwrap();
            ResourceLocalGuard::validate_conversation_history_v17(
                path,
                read.boundary,
                &target,
                read.parent,
                &snapshot,
            )?;
            let witness = journal
                .prepare_conversation_inspection_v17(&target, reference)
                .unwrap();
            let observed = read.inspect(witness.origin.seed())?;
            // Display is prepared BEFORE original final principal too.
            crate::prepared_form_text_v1::PreparedFormTextV1::prepare(
                observed.lines(),
                cancel,
                deadline,
            )
            .unwrap();
            Ok((
                witness,
                observed,
                target,
                read.parent.clone(),
                snapshot,
                read.identity.clone(),
            ))
        },
        |(witness, observed, target, parent, snapshot, identity), boundary| {
            ResourceLocalGuard::validate_conversation_history_v17(
                path, boundary, target, parent, snapshot,
            )?;
            PrWriteJournal::open(path)
                .unwrap()
                .validate_conversation_inspection_v17(witness)
                .unwrap();
            api.validate_conversation_inspection_v17(observed, identity, parent, cancel, deadline)
        },
    )
    .unwrap();
    (witness, observed)
}
struct Stopped(process::ProcessIdentity);
impl Stopped {
    fn hold(s: &Setup) -> Self {
        let index = s.steps.iter().position(|v| v.delay_ms == 2_000).unwrap();
        let until = std::time::Instant::now() + Duration::from_secs(6);
        loop {
            let journal = std::fs::read_to_string(s.fixture.root.join("gh-fixture/commands.jsonl"))
                .unwrap_or_default();
            if let Some(line) = journal.lines().nth(index) {
                let row: serde_json::Value = serde_json::from_str(line).unwrap();
                assert_eq!(row["matched"], true);
                let pid = i32::try_from(row["pid"].as_u64().unwrap()).unwrap();
                let now = process::process_identity(pid).unwrap().unwrap();
                assert_eq!(now.uid, unsafe { libc::geteuid() });
                assert_eq!(now.session, unsafe { libc::getsid(0) });
                assert!(!now.zombie);
                // SAFETY: positive strict-fixture PID freshly matched by UID,
                // birth/session below and never a group/name-based target.
                assert_eq!(unsafe { libc::kill(pid, libc::SIGSTOP) }, 0);
                return Self(now);
            }
            assert!(std::time::Instant::now() < until);
            std::thread::yield_now();
        }
    }
}
impl Drop for Stopped {
    fn drop(&mut self) {
        if let Ok(Some(now)) = process::process_identity(self.0.pid) {
            if (now.uid, now.birth, now.session) == (self.0.uid, self.0.birth, self.0.session)
                && !now.zombie
            {
                // SAFETY: same freshly revalidated exact child only.
                unsafe {
                    libc::kill(now.pid, libc::SIGCONT);
                }
            }
        }
    }
}
fn drift(connection: &Connection, which: u8) {
    match which {
        0 => {}
        1 => {
            assert_eq!(
                connection
                    .execute("UPDATE pr_write_attempts SET confirmed_json='{}'", [])
                    .unwrap(),
                1
            );
        }
        2 => {
            assert_eq!(
                connection
                    .execute("UPDATE pr_write_attempts SET created_at=created_at+1", [])
                    .unwrap(),
                1
            );
        }
        3 => {
            assert_eq!(
                connection
                    .execute("UPDATE pr_write_attempts SET pr_number=pr_number+1", [])
                    .unwrap(),
                1
            );
        }
        4 => {
            connection.execute_batch("DROP TRIGGER pr_write_conversation_origins_immutable_v17; UPDATE pr_write_conversation_origins_v17 SET origin_json=json_set(origin_json,'$.seed.observed_requester','EXTERNAL_RAW_ORIGIN');").unwrap();
        }
        _ => unreachable!(),
    }
}
fn retained(s: &Setup, target: &PrWriteTarget, body: &str, success: bool) {
    let connection = Connection::open_with_flags(
        fixture::database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                .get::<_, usize>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        connection
            .query_row("SELECT state FROM pr_write_attempts", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        if success { "acknowledged" } else { "uncertain" }
    );
    let stored: String = connection
        .query_row(
            "SELECT body FROM drafts WHERE resource LIKE 'conversation-draft-v17/%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stored, body);
    for uid in [42, 43] {
        assert_eq!(
            Cache::open(&fixture::database(&s.fixture))
                .unwrap()
                .draft(&Scope::github("github.invalid", Some(uid)).unwrap(), "kept")
                .unwrap()
                .unwrap(),
            format!("PRIVATE_{uid}")
        );
    }
    assert_eq!(target.scope().login, "user-id:42");
    exact_protocol(s);
}
#[test]
fn conversation_comments_v17_exact_raw_ack_held_final_uid_unchanged_and_receipt_created_target_origin_drift(
) {
    for which in 0..=6 {
        let (s, original, target, reference, body) = seeded(true);
        let generation = PrWriteJournal::open(&fixture::database(&s.fixture))
            .unwrap()
            .source_generation(&target)
            .unwrap();
        let cancel = CancellationToken::default();
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        let (witness, observed) = std::thread::scope(|scope| {
            let worker = scope.spawn(|| {
                inspection(
                    original,
                    &s.api,
                    &fixture::database(&s.fixture),
                    &reference,
                    &cancel,
                    deadline,
                )
            });
            let held = Stopped::hold(&s);
            let external = Connection::open_with_flags(
                fixture::database(&s.fixture),
                rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
            )
            .unwrap();
            drift(&external, if which <= 4 { which } else { 0 });
            drop(held);
            worker.join().unwrap()
        });
        if which == 5 {
            cancel.cancel();
        }
        let deadline = if which == 6 {
            std::time::Instant::now() - Duration::from_millis(1)
        } else {
            deadline
        };
        let mut journal = PrWriteJournal::open(&fixture::database(&s.fixture)).unwrap();
        let result = journal
            .acknowledge_conversation_inspection_v17(&s.api, witness, observed, &cancel, deadline);
        if which == 0 {
            assert_eq!(result.unwrap(), 1);
        } else {
            assert!(result.is_err());
        }
        retained(&s, &target, &body, which == 0);
        assert_eq!(
            journal.source_generation(&target).unwrap(),
            generation + i64::from(which == 0)
        );
    }
}

// SQLite's ACTUAL statement trace observes BEGIN IMMEDIATE after all preceding
// exact-row checks. The independent held writer makes this a causal lock-wait
// test, not a timed assumption or a production retry/observation hook.
unsafe extern "C" fn trace_begin(
    mask: std::os::raw::c_uint,
    context: *mut std::os::raw::c_void,
    _stmt: *mut std::os::raw::c_void,
    sql: *mut std::os::raw::c_void,
) -> std::os::raw::c_int {
    if mask == rusqlite::ffi::SQLITE_TRACE_STMT && !sql.is_null() {
        // SAFETY: SQLite provides a NUL-terminated SQL statement for STMT and
        // context remains the Box<SyncSender> alive through connection drop.
        let statement = unsafe { std::ffi::CStr::from_ptr(sql.cast()) }.to_string_lossy();
        if statement.trim().eq_ignore_ascii_case("BEGIN IMMEDIATE") {
            let sender = unsafe { &*context.cast::<mpsc::SyncSender<()>>() };
            let _ = sender.try_send(());
        }
    }
    0
}
#[test]
fn conversation_comments_v17_exact_raw_ack_actual_immediate_lockwait_drift_refuses_and_unchanged_commits(
) {
    for which in [0, 1, 2, 4] {
        let (s, original, target, reference, body) = seeded(false);
        let generation = PrWriteJournal::open(&fixture::database(&s.fixture))
            .unwrap()
            .source_generation(&target)
            .unwrap();
        let cancel = CancellationToken::default();
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        let (witness, observed) = inspection(
            original,
            &s.api,
            &fixture::database(&s.fixture),
            &reference,
            &cancel,
            deadline,
        );
        let mut journal = PrWriteJournal::open(&fixture::database(&s.fixture)).unwrap();
        let external = Connection::open_with_flags(
            fixture::database(&s.fixture),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
        )
        .unwrap();
        external.execute_batch("BEGIN IMMEDIATE").unwrap();
        let (sent, seen) = mpsc::sync_channel(1);
        let mut context = Box::new(sent);
        // SAFETY: test-only read observation of this exact Connection. The
        // boxed sender outlives the scoped worker and connection destruction.
        assert_eq!(
            unsafe {
                rusqlite::ffi::sqlite3_trace_v2(
                    journal.connection.handle(),
                    rusqlite::ffi::SQLITE_TRACE_STMT,
                    Some(trace_begin),
                    (&mut *context as *mut mpsc::SyncSender<()>).cast(),
                )
            },
            rusqlite::ffi::SQLITE_OK
        );
        let api = &s.api;
        let token = &cancel;
        let result = std::thread::scope(|scope| {
            let worker = scope.spawn(move || {
                journal.acknowledge_conversation_inspection_v17(
                    api, witness, observed, token, deadline,
                )
            });
            seen.recv_timeout(Duration::from_secs(5))
                .expect("Actual ACK reached BEGIN IMMEDIATE while independent writer holds lock");
            drift(&external, which);
            external.execute_batch("COMMIT").unwrap();
            worker.join().unwrap()
        });
        if which == 0 {
            assert_eq!(result.unwrap(), 1);
        } else {
            assert!(result.is_err());
        }
        retained(&s, &target, &body, which == 0);
        assert_eq!(
            PrWriteJournal::open(&fixture::database(&s.fixture))
                .unwrap()
                .source_generation(&target)
                .unwrap(),
            generation + i64::from(which == 0)
        );
    }
}
