//! TASK54W7 actual final-UID boundary declarations; UNCOMPILED/UNRUN.
//! Real producer/channel children and independently writable SQLite, not DTO authority.
use super::*;
use crate::owned_pty_processes_test_support as process;
use std::{
    fs,
    time::{Duration, Instant},
};

struct HeldUid(process::ProcessIdentity);
impl HeldUid {
    fn stop(pid: i32) -> Self {
        let parent = process::process_identity(std::process::id() as i32)
            .unwrap()
            .unwrap();
        let child = process::process_identity(pid)
            .unwrap()
            .expect("Actual final UID child");
        assert!(pid > 0 && pid != parent.pid && !child.zombie);
        assert_eq!(child.parent, parent.pid);
        assert_eq!(child.uid, parent.uid);
        assert_eq!(child.session, parent.session);
        assert_eq!(
            process::process_identity(pid).unwrap().as_ref(),
            Some(&child)
        );
        // SAFETY: only this positive PID after actual UID/birth/SID identity check.
        assert_eq!(unsafe { libc::kill(pid, libc::SIGSTOP) }, 0);
        Self(child)
    }
}
impl Drop for HeldUid {
    fn drop(&mut self) {
        if let Ok(Some(now)) = process::process_identity(self.0.pid) {
            if !now.zombie
                && (now.pid, now.uid, now.birth, now.session)
                    == (self.0.pid, self.0.uid, self.0.birth, self.0.session)
            {
                // SAFETY: always resume only the same original stopped child.
                assert_eq!(unsafe { libc::kill(now.pid, libc::SIGCONT) }, 0);
            }
        }
    }
}

fn protected_rows(f: &Fixture) -> Vec<String> {
    let db = rusqlite::Connection::open(f::database(f)).unwrap();
    let mut q = db.prepare("SELECT json_array(attempt_id,host,principal,target_json,resources_json,kind_json,context_json,options_json,fingerprint_sha256,state,claimed,receipt_json,created_at) FROM actions_write_attempts ORDER BY attempt_id").unwrap();
    let mut rows = q
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    let mut q = db.prepare("SELECT json_array(host,principal,resource_key,generation) FROM actions_write_generations ORDER BY host,principal,resource_key").unwrap();
    rows.extend(
        q.query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .map(Result::unwrap),
    );
    rows
}

// fake_gh writes one LF-terminated record with append/write_all. A reader can
// observe that append before its final LF. Only complete records are evidence;
// malformed LF-terminated JSON remains a hard error, never an ignored row.
fn complete_command_rows_v183(bytes: &[u8]) -> serde_json::Result<Vec<Value>> {
    let Some(end) = bytes.iter().rposition(|byte| *byte == b'\n') else {
        return Ok(Vec::new());
    };
    bytes[..=end]
        .split_inclusive(|byte| *byte == b'\n')
        .map(serde_json::from_slice)
        .collect()
}

#[test]
fn scoped_actions_dispatch_v7_task54_w7_complete_journal_prefix_refuses_malformed_complete_record()
{
    assert!(complete_command_rows_v183(b"").unwrap().is_empty());
    for unfinished in [
        b"{\"args\":[".as_slice(),
        b"{\"step\":0}".as_slice(),
        &[0xe7, 0x95],
    ] {
        assert!(complete_command_rows_v183(unfinished).unwrap().is_empty());
    }
    let first =
        b"{\"step\":0,\"args\":[\"--version\"],\"stdin_hex\":\"\",\"pid\":123,\"matched\":true}\n";
    let second = b"{\"step\":1,\"args\":[\"api\",\"user\",\"--hostname\",\"github.invalid\",\"--method\",\"GET\"],\"stdin_hex\":\"\",\"pid\":124,\"matched\":true}\n";
    let expected_first = serde_json::from_slice::<Value>(first).unwrap();
    let expected_second = serde_json::from_slice::<Value>(second).unwrap();
    assert_eq!(
        complete_command_rows_v183(first).unwrap(),
        vec![expected_first.clone()]
    );
    let mut complete_partial = first.to_vec();
    complete_partial.extend_from_slice(&second[..73]);
    assert_eq!(
        complete_command_rows_v183(&complete_partial).unwrap(),
        vec![expected_first.clone()]
    );
    let mut complete = first.to_vec();
    complete.extend_from_slice(second);
    assert_eq!(
        complete_command_rows_v183(&complete).unwrap(),
        vec![expected_first, expected_second]
    );
    for malformed in [
        b"{\"args\":[\n".as_slice(),
        b"not JSON\n".as_slice(),
        b"\n".as_slice(),
    ] {
        assert!(complete_command_rows_v183(malformed).is_err());
        let mut after_valid = first.to_vec();
        after_valid.extend_from_slice(malformed);
        after_valid.extend_from_slice(b"{\"unfinished\":");
        assert!(
            complete_command_rows_v183(&after_valid).is_err(),
            "A later unfinished append cannot conceal a malformed complete record"
        );
    }
}

fn held_ack(change: Option<&str>, cancel: bool, abort: bool) {
    let protocol = DispatchProtocolV7::Detailed2026Run200;
    let mut tail = confirm_phase(protocol, 42);
    tail.extend(followthrough::history_phase(protocol, 42));
    let mut ack = followthrough::history_phase(protocol, 42);
    ack.last_mut().unwrap().delay_ms = 5000;
    tail.extend(ack);
    let (f, p, mut app, all) = prepared(protocol, tail);
    let _drafts = confirm(&mut app, &p, &f);
    let original_source = app.scoped_actions_v1.as_ref().unwrap().source.clone();
    let work = w::request(&mut app, FormAction::Submit);
    install_work(&mut app, &p, &f, work);
    let before = f::sql_state(&f);
    let git = f::git_state(&f);
    let original = metadata(&f);
    assert_eq!(original.0, "accepted_pending");
    let work = w::request(&mut app, FormAction::AcknowledgeActionsAcceptanceV5);
    let owner = native::ScopedActionsOwnerV1::new(work).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let worker_owner = owner.clone();
    let worker_provider = p.clone();
    let database = f::database(&f);
    let worker = std::thread::spawn(move || {
        super::super::super::execute(&worker_owner, &worker_provider, &database, None)
    });
    let until = Instant::now() + Duration::from_secs(5);
    let pid = loop {
        let bytes = fs::read(f.root.join("gh-fixture/commands.jsonl")).unwrap();
        let rows = complete_command_rows_v183(&bytes)
            .expect("Malformed COMPLETE fake-gh journal record is a hard failure");
        if rows.len() == all.len() {
            let last = rows.last().unwrap();
            assert_eq!(
                last["args"],
                json!(["api", "user", "--hostname", f::HOST, "--method", "GET"])
            );
            assert_eq!(last["matched"], true);
            break i32::try_from(last["pid"].as_i64().unwrap()).unwrap();
        }
        assert!(
            Instant::now() < until,
            "Actual Dispatch ACK final UID never entered: {}",
            String::from_utf8_lossy(&bytes)
        );
        std::thread::yield_now();
    };
    let held = HeldUid::stop(pid);
    let db = rusqlite::Connection::open(f::database(&f)).unwrap();
    if change == Some("provider_generation") {
        p.native_external_runner_v4().mark_ambiguous(&format!(
            "{}/repository-id/{}/actions/workflow/7",
            f::HOST,
            original_source.reference().id
        ));
    } else if change == Some("alias_generation") {
        assert!(
            db.execute(
                "UPDATE actions_write_generations SET generation=generation+1",
                []
            )
            .unwrap()
                > 0
        );
    } else if let Some(column) = change {
        assert!(matches!(
            column,
            "context_json" | "options_json" | "receipt_json"
        ));
        // Same parsed JSON and no generation bump: only a full raw-row CAS
        // can detect this original metadata-byte change during final UID.
        assert_eq!(
            db.execute(
                &format!("UPDATE actions_write_attempts SET {column}={column}||' '"),
                []
            )
            .unwrap(),
            1
        );
    }
    if abort {
        db.execute_batch("CREATE TRIGGER refuse_dispatch_ack BEFORE UPDATE OF state ON actions_write_attempts WHEN NEW.state='acknowledged' BEGIN SELECT RAISE(ABORT,'original-dispatch-row-protected'); END;").unwrap();
    }
    let edited = protected_rows(&f);
    let edited_metadata = metadata(&f);
    drop(db);
    if cancel {
        owner.cancellation().cancel();
    }
    drop(held);
    let result = worker.join().unwrap();
    assert!(
        process::process_identity(pid).unwrap().is_none(),
        "Original final UID child must actually be gone, not a zombie/kill0 waiver"
    );
    if change.is_some() || cancel || abort {
        assert!(result.is_err());
        assert_eq!(protected_rows(&f), edited);
        assert_eq!(metadata(&f), edited_metadata);
        assert_eq!(metadata(&f).0, "accepted_pending");
    } else {
        assert!(super::super::super::publish(
            &mut app,
            &owner,
            result.unwrap()
        ));
        assert_eq!(metadata(&f).0, "acknowledged");
        assert_eq!(
            (metadata(&f).1, metadata(&f).2, metadata(&f).3),
            (original.1, original.2, original.3)
        );
        assert!(!app.form.as_ref().unwrap().plan_ready());
        assert!(Arc::ptr_eq(
            &app.scoped_actions_v1.as_ref().unwrap().source,
            &original_source
        ));
    }
    assert_eq!(f::sql_state(&f), before);
    assert_eq!(f::git_state(&f), git);
    exact(&f, &all);
}

#[test]
fn scoped_actions_dispatch_v7_task54_w7_actual_held_final_uid_receipt_bytes_refuse_ack() {
    held_ack(Some("receipt_json"), false, false);
}
#[test]
fn scoped_actions_dispatch_v7_task54_w7_actual_held_final_uid_context_bytes_refuse_ack() {
    held_ack(Some("context_json"), false, false);
}
#[test]
fn scoped_actions_dispatch_v7_task54_w7_actual_held_final_uid_options_bytes_refuse_ack() {
    held_ack(Some("options_json"), false, false);
}
#[test]
fn scoped_actions_dispatch_v7_task54_w7_actual_held_final_uid_unchanged_ack_succeeds() {
    held_ack(None, false, false);
}
#[test]
fn scoped_actions_dispatch_v7_task54_w7_actual_held_final_uid_cancel_retains_protection() {
    held_ack(None, true, false);
}
#[test]
fn scoped_actions_dispatch_v7_task54_w7_actual_held_final_uid_alias_generation_retains_protection()
{
    held_ack(Some("alias_generation"), false, false);
}
#[test]
fn scoped_actions_dispatch_v7_task54_w7_actual_held_final_uid_provider_generation_retains_protection(
) {
    held_ack(Some("provider_generation"), false, false);
}
#[test]
fn scoped_actions_dispatch_v7_task54_w7_actual_runner_observation_sql_abort_does_not_release_protected_row(
) {
    held_ack(None, false, true);
}
