//! UNRUN genuine last-UID child suspension and concurrent real SQLite edits.
//! No fabricated source, receipt, clock, PID, parser frame or post-UID hook.
use super::*;
use crate::owned_pty_processes_test_support as process;
use std::time::{Duration, Instant};

pub(super) struct HeldFinalUid(process::ProcessIdentity);
impl HeldFinalUid {
    pub(super) fn stop(pid: i32) -> Self {
        let parent = process::process_identity(std::process::id() as i32)
            .unwrap()
            .unwrap();
        let child = process::process_identity(pid)
            .unwrap()
            .expect("actual last-UID child live");
        assert!(pid > 0 && pid != parent.pid && !child.zombie);
        assert_eq!(child.parent, parent.pid);
        assert_eq!(child.uid, parent.uid);
        assert_eq!(child.session, parent.session);
        assert!(child.birth >= parent.birth);
        assert_eq!(
            process::process_identity(pid).unwrap().as_ref(),
            Some(&child)
        );
        // SAFETY: exact positive original child identity checked immediately,
        // never a process name, group, foreign child or new subreaper.
        assert_eq!(unsafe { libc::kill(pid, libc::SIGSTOP) }, 0);
        Self(child)
    }
}
impl Drop for HeldFinalUid {
    fn drop(&mut self) {
        if let Ok(Some(now)) = process::process_identity(self.0.pid) {
            if !now.zombie
                && (now.pid, now.uid, now.session, now.birth)
                    == (self.0.pid, self.0.uid, self.0.session, self.0.birth)
            {
                // SAFETY: only the exact verified original stopped child.
                assert_eq!(unsafe { libc::kill(now.pid, libc::SIGCONT) }, 0);
            }
        }
    }
}
#[derive(Clone, Copy)]
enum Change {
    None,
    Receipt,
    Context,
    Options,
}
fn action_rows(fixture: &Fixture) -> (Vec<String>, Vec<String>) {
    let db = rusqlite::Connection::open(f::database(fixture)).unwrap();
    let rows = |sql: &str| {
        db.prepare(sql)
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    (rows("SELECT json_array(attempt_id,host,principal,target_json,resources_json,kind_json,context_json,options_json,fingerprint_sha256,state,claimed,receipt_json,created_at) FROM actions_write_attempts ORDER BY attempt_id"),
     rows("SELECT json_array(host,principal,resource_key,generation) FROM actions_write_generations ORDER BY host,principal,resource_key"))
}
fn race(change: Change) {
    let mut closing = phase(None, 42);
    closing.last_mut().unwrap().delay_ms = 5_000;
    let mut tail = phase(None, 42);
    tail.extend(closing);
    let (fixture, provider, mut app, all, reference) = accepted(tail);
    inspect(&mut app, &provider, &fixture);
    let source = app.scoped_actions_v1.as_ref().unwrap().source.clone();
    let git = f::git_state(&fixture);
    let unrelated = f::sql_state(&fixture);
    let before = c::children(&fixture);
    assert!(intercept(
        &mut app,
        &ActionsAction::PublicationAcknowledgeAcceptance(reference.attempt_id)
    ));
    let work = super::super::super::take_work(&mut app).unwrap();
    let owner = ScopedActionsOwnerV1::new(work).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let worker_owner = owner.clone();
    let worker_provider = provider.clone();
    let database = f::database(&fixture);
    let worker = std::thread::spawn(move || {
        publication::execute(&worker_owner, &worker_provider, &database, None)
    });
    let until = Instant::now() + Duration::from_secs(5);
    let pid = loop {
        let journal =
            std::fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
        let records = journal
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .collect::<Vec<_>>();
        if records.len() == before + 7 {
            let last = records.last().unwrap();
            assert_eq!(
                last["args"],
                json!(["api", "user", "--hostname", f::HOST, "--method", "GET"])
            );
            break i32::try_from(last["pid"].as_i64().unwrap()).unwrap();
        }
        assert!(Instant::now() < until, "actual final UID never entered");
        std::thread::yield_now();
    };
    let held = HeldFinalUid::stop(pid);
    let db = rusqlite::Connection::open(f::database(&fixture)).unwrap();
    let (receipt,context,options):(String,String,String)=db.query_row(
        "SELECT receipt_json,context_json,options_json FROM actions_write_attempts WHERE attempt_id=?1",[reference.attempt_id],
        |row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
    let (column, mut value) = match change {
        Change::None => ("", json!(null)),
        Change::Receipt => (
            "receipt_json",
            serde_json::from_str::<serde_json::Value>(&receipt).unwrap(),
        ),
        Change::Context => (
            "context_json",
            serde_json::from_str::<serde_json::Value>(&context).unwrap(),
        ),
        Change::Options => (
            "options_json",
            serde_json::from_str::<serde_json::Value>(&options).unwrap(),
        ),
    };
    match change {
        Change::None => {}
        Change::Receipt => value["observed_attempt"] = json!(3),
        Change::Context => value["head_sha"] = json!("b".repeat(40)),
        Change::Options => value["debug_logging"] = json!(true),
    }
    if !matches!(change, Change::None) {
        let sql = match column {
            "receipt_json" => {
                "UPDATE actions_write_attempts SET receipt_json=?1 WHERE attempt_id=?2"
            }
            "context_json" => {
                "UPDATE actions_write_attempts SET context_json=?1 WHERE attempt_id=?2"
            }
            "options_json" => {
                "UPDATE actions_write_attempts SET options_json=?1 WHERE attempt_id=?2"
            }
            _ => unreachable!(),
        };
        assert_eq!(
            db.execute(
                sql,
                rusqlite::params![value.to_string(), reference.attempt_id]
            )
            .unwrap(),
            1
        );
    }
    drop(db);
    let edited = action_rows(&fixture);
    drop(held);
    let result = worker.join().unwrap();
    assert!(
        process::process_identity(pid).unwrap().is_none(),
        "actual final UID child reaped, not a zombie/signal0 waiver"
    );
    if matches!(change, Change::None) {
        assert!(result.is_ok());
        assert_eq!(
            c::saved(&fixture, &provider, &app, &reference).state,
            ActionsWriteState::Acknowledged
        );
    } else {
        assert!(
            result.is_err(),
            "same-generation changed original metadata must not be acknowledged"
        );
        assert_eq!(
            action_rows(&fixture),
            edited,
            "no ACK, gen advance, repair, resend or adopted metadata"
        );
        let db = rusqlite::Connection::open(f::database(&fixture)).unwrap();
        assert_eq!(
            db.query_row(
                "SELECT state FROM actions_write_attempts WHERE attempt_id=?1",
                [reference.attempt_id],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
            "accepted_pending"
        );
    }
    assert!(Arc::ptr_eq(
        &source,
        &app.scoped_actions_v1.as_ref().unwrap().source
    ));
    assert_eq!(f::git_state(&fixture), git);
    assert_eq!(f::sql_state(&fixture), unrelated);
    assert_eq!(c::children(&fixture), before + 7);
    w::exact(&fixture, &all);
}
#[test]
fn scoped_actions_history_cas_v6_actual_last_uid_receipt_edit_without_generation_stays_protected() {
    race(Change::Receipt)
}
#[test]
fn scoped_actions_history_cas_v6_actual_last_uid_context_edit_without_generation_stays_protected() {
    race(Change::Context)
}
#[test]
fn scoped_actions_history_cas_v6_actual_last_uid_options_edit_without_generation_stays_protected() {
    race(Change::Options)
}
#[test]
fn scoped_actions_history_cas_v6_actual_last_uid_unchanged_original_still_acknowledges() {
    race(Change::None)
}
