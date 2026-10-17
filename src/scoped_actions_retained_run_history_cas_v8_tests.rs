//! UNRUN genuine final-UID child hold + independent real SQLite race.
use super::*;
use crate::owned_pty_processes_test_support as process;
struct Held(process::ProcessIdentity);
impl Held {
    fn stop(pid: i32) -> Self {
        let parent = process::process_identity(std::process::id() as i32)
            .unwrap()
            .unwrap();
        let child = process::process_identity(pid)
            .unwrap()
            .expect("actual last UID live");
        assert!(pid > 0 && pid != parent.pid && !child.zombie);
        assert_eq!(child.parent, parent.pid);
        assert_eq!(child.uid, parent.uid);
        assert_eq!(child.session, parent.session);
        assert_eq!(
            process::process_identity(pid).unwrap().as_ref(),
            Some(&child)
        );
        // SAFETY: exact positive actual child UID/birth/SID identity immediately verified.
        assert_eq!(unsafe { libc::kill(pid, libc::SIGSTOP) }, 0);
        Self(child)
    }
}
impl Drop for Held {
    fn drop(&mut self) {
        if let Ok(Some(now)) = process::process_identity(self.0.pid) {
            if !now.zombie
                && (now.pid, now.uid, now.session, now.birth)
                    == (self.0.pid, self.0.uid, self.0.session, self.0.birth)
            {
                // SAFETY: resume ONLY exact captured original stopped child.
                assert_eq!(unsafe { libc::kill(now.pid, libc::SIGCONT) }, 0);
            }
        }
    }
}
fn rows(f: &Fixture) -> Vec<String> {
    let db = rusqlite::Connection::open(database(f)).unwrap();
    let mut q=db.prepare("SELECT json_array(attempt_id,state,receipt_json,context_json,options_json,fingerprint_sha256,claimed) FROM actions_write_attempts ORDER BY attempt_id").unwrap();
    let mut v: Vec<String> = q
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let mut generations = db.prepare("SELECT json_array(host,principal,resource_key,generation) FROM actions_write_generations ORDER BY host,principal,resource_key").unwrap();
    v.extend(
        generations
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(Result::unwrap),
    );
    v
}
fn race(change: Option<&str>, cancel: bool, abort: bool) {
    let mut ack = history(Some(model(3, "queued", "")), 42);
    ack.last_mut().unwrap().delay_ms = 5000;
    let mut tail = history(Some(model(3, "queued", "")), 42);
    tail.extend(ack);
    let (f, p, mut app, all, _writer) = accepted(ActionsCommand::RerunAll, tail);
    let actual_preview = crate::scoped_actions_rerun_test_support_v4::preview(&app);
    read(&mut app, &p, &f, FormAction::Submit).unwrap();
    let original = proof(&app);
    let git = git_state_resource(&f);
    let sql = sql_state(&f);
    let before = std::fs::read_to_string(f.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .count();
    let owner = read_work(&mut app, FormAction::AcknowledgeActionsV3);
    let wp = p.clone();
    let wo = owner.clone();
    let dbpath = database(&f);
    let worker = std::thread::spawn(move || publication::execute(&wo, &wp, &dbpath, None));
    let until = Instant::now() + Duration::from_secs(5);
    let pid = loop {
        let text = std::fs::read_to_string(f.root.join("gh-fixture/commands.jsonl")).unwrap();
        let records: Vec<Value> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        if records.len() == before + 10 {
            let last = records.last().unwrap();
            assert_eq!(
                last["args"],
                json!(["api", "user", "--hostname", HOST, "--method", "GET"])
            );
            break i32::try_from(last["pid"].as_i64().unwrap()).unwrap();
        }
        assert!(Instant::now() < until, "actual last UID never entered");
        std::thread::yield_now();
    };
    let held = Held::stop(pid);
    let db = rusqlite::Connection::open(database(&f)).unwrap();
    if change == Some("provider_generation") {
        p.native_external_runner_v4()
            .mark_ambiguous(&format!("{HOST}/repository-id/410/actions/run/123"));
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
        let raw: String = db
            .query_row(
                &format!("SELECT {column} FROM actions_write_attempts"),
                [],
                |r| r.get(0),
            )
            .unwrap();
        let mut value: Value = serde_json::from_str(&raw).unwrap();
        match column {
            "receipt_json" => value["observed_attempt"] = json!(4),
            "context_json" => value["head_sha"] = json!("b".repeat(40)),
            _ => value["debug_logging"] = json!(true),
        }
        assert_eq!(
            db.execute(
                &format!("UPDATE actions_write_attempts SET {column}=?1"),
                [value.to_string()]
            )
            .unwrap(),
            1
        );
    }
    if abort {
        db.execute_batch("CREATE TRIGGER refuse_owned_run_ack BEFORE UPDATE OF state ON actions_write_attempts WHEN NEW.state='transition_observed' BEGIN SELECT RAISE(ABORT,'original-row-protected'); END;").unwrap();
    }
    let edited = rows(&f);
    drop(db);
    if cancel {
        owner.cancellation().cancel();
    }
    drop(held);
    let result = worker.join().unwrap();
    assert!(
        process::process_identity(pid).unwrap().is_none(),
        "actual child reaped, no zombie/kill0 waiver"
    );
    if change.is_some() || cancel || abort {
        assert!(result.is_err());
        assert_eq!(rows(&f), edited);
        assert_eq!(status(&f), "accepted_pending");
        if abort {
            let intent = actual_preview.opened().intent();
            let descriptor = actual_preview
                .plan()
                .scoped_run_descriptor_v4(
                    &p,
                    intent.source(),
                    actual_preview.target(),
                    intent.selected_job(),
                    intent.action(actual_preview.options()),
                )
                .unwrap();
            assert!(ActionsWriteJournal::open(&database(&f)).unwrap().begin(&descriptor.target,&descriptor.intent).is_err(),
                "Runner reconciliation cannot bypass unchanged durable original barrier after local CAS failure");
            assert_eq!(rows(&f), edited);
        }
    } else {
        let value = result.unwrap();
        assert!(publication::publish(&mut app, &owner, value));
        assert_eq!(status(&f), "transition_observed");
        assert!(ActionsWriteJournal::open(&database(&f))
            .unwrap()
            .validate_owned_run_history_v8(&original)
            .is_err());
    }
    assert_eq!(git_state_resource(&f), git);
    assert_eq!(sql_state(&f), sql);
    exact(&f, &all);
}
#[test]
fn scoped_actions_retained_history_v8_held_final_uid_receipt_bytes_cannot_ack() {
    race(Some("receipt_json"), false, false);
}
#[test]
fn scoped_actions_retained_history_v8_held_final_uid_context_bytes_cannot_ack() {
    race(Some("context_json"), false, false);
}
#[test]
fn scoped_actions_retained_history_v8_held_final_uid_options_bytes_cannot_ack() {
    race(Some("options_json"), false, false);
}
#[test]
fn scoped_actions_retained_history_v8_held_final_uid_unchanged_ack_succeeds() {
    race(None, false, false);
}
#[test]
fn scoped_actions_retained_history_v8_held_final_uid_cancel_cannot_release() {
    race(None, true, false);
}
#[test]
fn scoped_actions_retained_history_v8_held_final_uid_alias_generation_cannot_release() {
    race(Some("alias_generation"), false, false);
}
#[test]
fn scoped_actions_retained_history_v8_held_final_uid_actual_provider_generation_cannot_release() {
    race(Some("provider_generation"), false, false);
}
#[test]
fn scoped_actions_retained_history_v8_runner_reconciled_but_sql_abort_retains_protected_row() {
    race(None, false, true);
}
