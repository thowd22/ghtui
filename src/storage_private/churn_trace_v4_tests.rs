//! Diagnostic-only controlled actual SQLite transactions, not product retries.
use super::*;
use crate::{
    drafts::DraftWriter,
    storage::{Cache, Scope},
};
use serde_json::{json, Value};
use std::{
    cell::{Cell, RefCell},
    sync::mpsc,
    thread,
    time::Duration,
};

enum Command {
    Begin(usize),
    Commit,
    Stop,
}
struct Producer {
    commands: mpsc::SyncSender<Command>,
    replies: mpsc::Receiver<Value>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Producer {
    fn new(path: PathBuf) -> Self {
        let (commands, receiver) = mpsc::sync_channel(1);
        let (reply, replies) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            let sql = rusqlite::Connection::open(&path).unwrap();
            sql.busy_timeout(Duration::ZERO).unwrap();
            let journal = target_paths(&path)[3].clone();
            assert_eq!(
                sql.query_row("PRAGMA journal_mode=DELETE", [], |row| row
                    .get::<_, String>(0))
                    .unwrap(),
                "delete"
            );
            while let Ok(command) = receiver.recv() {
                match command {
                    Command::Begin(round) => {
                        sql.execute_batch("BEGIN IMMEDIATE;").unwrap();
                        let body = format!(
                            "genuine original draft transaction {round} {}",
                            "x".repeat(16_384)
                        );
                        assert_eq!(sql.execute("UPDATE drafts SET body=?1 WHERE host='github.invalid' AND login='user-id:42' AND resource='corpus-original'", [body]).unwrap(), 1);
                        let metadata = fs::symlink_metadata(&journal).unwrap();
                        assert!(metadata.is_file() && metadata.len() > 0 && metadata.nlink() == 1);
                        reply.send(json!({"event":"sqlite_begin","round":round,"inode":metadata.ino(),"links":metadata.nlink(),"bytes":metadata.len()})).unwrap();
                    }
                    Command::Commit => {
                        sql.execute_batch("COMMIT;").unwrap();
                        assert!(
                            !journal.exists(),
                            "only actual SQLite may remove its journal"
                        );
                        reply
                            .send(json!({"event":"sqlite_commit","journal_absent":true}))
                            .unwrap();
                    }
                    Command::Stop => break,
                }
            }
        });
        Self {
            commands,
            replies,
            worker: Some(worker),
        }
    }
    fn call(&self, command: Command) -> Value {
        self.commands.send(command).unwrap();
        self.replies.recv_timeout(Duration::from_secs(3)).unwrap()
    }
}
impl Drop for Producer {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Stop);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}

#[test]
fn cache_storage_churn_trace_v4_actual_sqlite_commit_before_optional_open_exhausts_exact_four() {
    let directory = named_race_v2_tests::directory();
    let path = directory.path().join("cache.sqlite");
    let journal = target_paths(&path)[3].clone();
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    let cache = Cache::open(&path).unwrap();
    cache
        .put_draft(&scope, "corpus-original", "original draft")
        .unwrap();
    drop(cache);
    let original_main = fs::symlink_metadata(&path).unwrap();
    let producer = Producer::new(path.clone());
    let pass = Cell::new(0usize);
    let trace = RefCell::new(Vec::<Value>::new());
    let result = prepare_with_named_metadata_v2(
        &path,
        |name, file| {
            let metadata = file.metadata().unwrap();
            trace.borrow_mut().push(json!({"event":"opened","pass":pass.get(),"name":name.file_name().unwrap().to_str().unwrap(),"inode":metadata.ino(),"links":metadata.nlink()}));
            if name == path {
                trace.borrow_mut().push(producer.call(Command::Commit));
                assert!(!journal.exists());
            }
        },
        |name, phase| {
            if name == path && phase == NamedPhaseV2::Initial {
                pass.set(pass.get() + 1);
                trace
                    .borrow_mut()
                    .push(producer.call(Command::Begin(pass.get())));
            }
            let result = fs::symlink_metadata(name);
            trace.borrow_mut().push(match &result {
                Ok(metadata) => json!({"event":"named","pass":pass.get(),"phase":format!("{phase:?}"),"name":name.file_name().unwrap().to_str().unwrap(),"inode":metadata.ino(),"links":metadata.nlink(),"uid":metadata.uid()}),
                Err(error) => json!({"event":"named","pass":pass.get(),"phase":format!("{phase:?}"),"name":name.file_name().unwrap().to_str().unwrap(),"error":format!("{:?}", error.kind())}),
            });
            result
        },
    );
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("bounded validation"));
    assert_eq!(pass.get(), 4);
    let trace = trace.into_inner();
    assert_eq!(
        trace
            .iter()
            .filter(|row| row["event"] == "sqlite_begin")
            .count(),
        4
    );
    assert_eq!(
        trace
            .iter()
            .filter(|row| row["event"] == "sqlite_commit")
            .count(),
        4
    );
    assert_eq!(
        trace.iter().filter(|row| row["event"] == "opened").count(),
        4
    );
    assert!(trace
        .iter()
        .filter(|row| row["event"] == "opened")
        .all(|row| row["name"] == "cache.sqlite"));
    assert!(trace
        .iter()
        .filter(|row| row["event"] == "named")
        .all(|row| row["phase"] == "Initial"));
    assert!(trace
        .iter()
        .filter(|row| row["event"] == "named" && row["name"] == "cache.sqlite-journal")
        .all(|row| row["links"] == 1));
    let main = fs::symlink_metadata(&path).unwrap();
    assert_eq!(
        (main.dev(), main.ino()),
        (original_main.dev(), original_main.ino())
    );
    drop(producer);
    let sql = rusqlite::Connection::open(&path).unwrap();
    let final_body: String = sql
        .query_row(
            "SELECT body FROM drafts WHERE resource='corpus-original'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(final_body.starts_with("genuine original draft transaction 4 "));
    println!(
        "ACTUAL_SQLITE_CHURN_TRACE {}",
        serde_json::to_string(&trace).unwrap()
    );
}

#[test]
fn cache_storage_churn_trace_v4_existing_ordered_draft_queue_closes_before_fresh_preparation() {
    let directory = named_race_v2_tests::directory();
    let path = directory.path().join("cache.sqlite");
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    let mut writer = DraftWriter::new(path.clone());
    writer
        .client()
        .save_and_wait(scope.clone(), "corpus-original".into(), "ready".into())
        .unwrap();
    let original_main = fs::symlink_metadata(&path).unwrap();
    let hold = rusqlite::Connection::open(&path).unwrap();
    hold.execute_batch("BEGIN IMMEDIATE; UPDATE drafts SET body='held original transaction' WHERE resource='corpus-original';").unwrap();
    assert!(fs::metadata(&target_paths(&path)[3]).unwrap().len() > 0);
    for round in 0..24 {
        writer
            .save(
                scope.clone(),
                "corpus-original".into(),
                format!("queued revision {round} {}", "x".repeat(16_384)),
            )
            .unwrap();
    }
    let client = writer.client();
    let pending = writer.take_pending();
    assert_eq!(
        pending.len(),
        1,
        "actual blocked writer must overflow its eight-slot queue"
    );
    hold.execute_batch("COMMIT;").unwrap();
    drop(hold);
    client.save_pending(pending).unwrap();
    client
        .save_and_wait(
            scope.clone(),
            "corpus-original".into(),
            "explicit original preview revision".into(),
        )
        .unwrap();
    let phases = RefCell::new(Vec::new());
    let held = prepare_with_named_metadata_v2(
        &path,
        |_, _| {},
        |name, phase| {
            phases
                .borrow_mut()
                .push((name.file_name().unwrap().to_owned(), phase));
            fs::symlink_metadata(name)
        },
    )
    .unwrap();
    assert_eq!(held.len(), 1);
    assert_eq!(
        phases
            .into_inner()
            .iter()
            .filter(|(name, phase)| name == "cache.sqlite" && *phase == NamedPhaseV2::Initial)
            .count(),
        1
    );
    let main = fs::symlink_metadata(&path).unwrap();
    assert_eq!(
        (main.dev(), main.ino()),
        (original_main.dev(), original_main.ino())
    );
    assert_eq!(
        Cache::open(&path)
            .unwrap()
            .draft(&scope, "corpus-original")
            .unwrap()
            .as_deref(),
        Some("explicit original preview revision")
    );
    assert!(!target_paths(&path)[3].exists());
    drop(writer);
    println!("ACTUAL_ORDERED_DRAFT_BARRIER_ONE_ORIGINAL_PREPARATION");
}
