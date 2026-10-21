use ghtui::storage::{Cache, Record, Scope};
use rusqlite::Connection;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Barrier,
    },
    thread,
    time::{Duration, Instant},
};

const SOURCE: &str =
    "2026-03-13 10:38:09 737ae4a34738ffa0c3ff7f9bb18df914dd1cad163f28fd6b6e114a344fe6d618";
const C_SHA256: &str = "9512509b1bccb7461f79bea8aad6280ae4699e925fa4804381b71f59e7efb0c5";

fn private_fixture() -> tempfile::TempDir {
    let fixture = tempfile::tempdir().unwrap();
    fs::set_permissions(fixture.path(), fs::Permissions::from_mode(0o700)).unwrap();
    fixture
}

#[test]
fn sqlite_dependency_actual_linked_runtime_and_unmodified_official_wal_fix() {
    assert_eq!(rusqlite::version(), "3.51.3");
    assert_eq!(rusqlite::version_number(), 3_051_003);
    let connection = Connection::open_in_memory().unwrap();
    let (version, source): (String, String) = connection
        .query_row("SELECT sqlite_version(), sqlite_source_id()", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap();
    assert_eq!(
        version, "3.51.3",
        "Actual linked C SQL function, not Cargo package/version text"
    );
    assert_eq!(source, SOURCE);
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let c =
        fs::read(root.join("vendor/upstream/rusqlite/libsqlite3-sys/sqlite3/sqlite3.c")).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&c)),
        C_SHA256,
        "Exact official 3.51.3 amalgamation bytes, not an old runtime with relabelled macros"
    );
    let text = std::str::from_utf8(&c).unwrap();
    assert!(
        text.contains("int bChg = memcmp(pLive->aSalt, pWal->hdr.aSalt, sizeof(pWal->hdr.aSalt));")
    );
    assert!(
        text.contains("if( 0==bChg ){\n        pInfo->nBackfillAttempted = mxSafeFrame;"),
        "Actual checkpoint salt-change guard precedes backfill publication"
    );
    let origin: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("vendor/upstream-sources.json")).unwrap())
            .unwrap();
    let sqlite = origin["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|source| source["directory"] == "vendor/upstream/rusqlite")
        .unwrap();
    assert_eq!(
        sqlite["revision"],
        "2a1790a69107cd03dae85d501dcbdb11c5b32ef3"
    );
    assert_eq!(sqlite["modified_files"], json!(["Cargo.toml"]));
    assert!(root.join("vendor/upstream/rusqlite/LICENSE").is_file());
}

#[test]
fn sqlite_dependency_fallible_unsigned_values_preserve_exact_boundaries_and_numeric_scope() {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("CREATE TABLE values_exact(value INTEGER NOT NULL)")
        .unwrap();
    for value in [0_u64, 42, i64::MAX as u64] {
        connection
            .execute("INSERT INTO values_exact VALUES (?1)", [value])
            .unwrap();
        let actual: u64 = connection
            .query_row(
                "SELECT value FROM values_exact ORDER BY rowid DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(actual, value);
    }
    for value in [i64::MAX as u64 + 1, u64::MAX] {
        assert!(connection
            .execute("INSERT INTO values_exact VALUES (?1)", [value])
            .is_err());
    }
    assert!(connection
        .query_row("SELECT -1", [], |row| row.get::<_, u64>(0))
        .is_err());
    let count: u64 = connection
        .query_row("SELECT count(*) FROM values_exact", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        count, 3,
        "Failed unsigned conversions cannot silently insert wrapped or saturated values"
    );
    let fixture = private_fixture();
    let cache = Cache::open(&fixture.path().join("cache.sqlite3")).unwrap();
    let maximum = Scope::github("github.invalid", Some(u64::MAX)).unwrap();
    let original = Scope::github("github.invalid", Some(42)).unwrap();
    cache
        .put_draft(&maximum, "same composer", "large numeric principal exact")
        .unwrap();
    cache
        .put_draft(&original, "same composer", "original42 exact")
        .unwrap();
    assert_eq!(maximum.login, "user-id:18446744073709551615");
    assert_eq!(
        cache.draft(&maximum, "same composer").unwrap().as_deref(),
        Some("large numeric principal exact")
    );
    assert_eq!(
        cache.draft(&original, "same composer").unwrap().as_deref(),
        Some("original42 exact")
    );
}

fn body(uid: u64, iteration: usize) -> String {
    format!(
        "private exact uid={uid} iteration={iteration} café 界\n{}",
        "committed WAL bytes ".repeat(32)
    )
}
struct WriterDone(Arc<AtomicUsize>);
impl Drop for WriterDone {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

#[test]
fn sqlite_dependency_real_concurrent_wal_checkpoint_drafts_and_records_reopen_isolated() {
    let fixture = private_fixture();
    let path = fixture.path().join("cache.sqlite3");
    let anchor = Cache::open(&path).unwrap();
    let first = Cache::open(&path).unwrap();
    let second = Cache::open(&path).unwrap();
    let checkpointer = Connection::open(&path).unwrap();
    checkpointer.busy_timeout(Duration::from_secs(2)).unwrap();
    let default_mode: String = checkpointer
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        default_mode, "delete",
        "Dependency update must not silently change production cache journal defaults"
    );
    let mode: String = checkpointer
        .query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
    let checkpoint: (i64, i64, i64) = checkpointer
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .unwrap();
    assert_eq!(
        checkpoint.0, 0,
        "First genuine complete checkpoint establishes resettable WAL state"
    );
    let barrier = Arc::new(Barrier::new(4));
    let remaining = Arc::new(AtomicUsize::new(2));
    // Both writers start every iteration together, so they contend for the write lock
    // each time while checkpoints run, but neither can starve the other for a whole
    // busy budget by re-taking the lock in a tight loop (TASK-102).
    let rounds = Arc::new(Barrier::new(2));
    let mut writers = Vec::new();
    for (uid, cache) in [(42, first), (43, second)] {
        let barrier = barrier.clone();
        let rounds = rounds.clone();
        let remaining = remaining.clone();
        writers.push(thread::spawn(move || {
            let _done = WriterDone(remaining);
            let scope = Scope::github("github.invalid", Some(uid)).unwrap();
            barrier.wait();
            // A failure is kept, not raised, so the other writer's rounds still complete.
            let mut failure = None;
            for iteration in 0..128 {
                rounds.wait();
                if failure.is_some() {
                    continue;
                }
                let body = body(uid, iteration);
                let written = cache
                    .put(
                        &scope,
                        &Record::new(
                            "private",
                            format!("record-{iteration:03}"),
                            "example/original",
                            json!({"uid":uid,"index":iteration,"body":body}),
                        ),
                    )
                    .and_then(|()| cache.put_draft(&scope, "composer/comment", &body));
                if let Err(error) = written {
                    failure = Some(format!("writer {uid} iteration {iteration}: {error:#}"));
                }
                thread::yield_now();
            }
            failure
        }));
    }
    let checkpoint_barrier = barrier.clone();
    let checkpoint_remaining = remaining.clone();
    let checker = thread::spawn(move || {
        checkpoint_barrier.wait();
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut calls = 0;
        let mut observed_log = false;
        while calls < 64 || checkpoint_remaining.load(Ordering::SeqCst) != 0 {
            let checkpoint: (i64, i64, i64) = checkpointer
                .query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })
                .unwrap();
            assert!(matches!(checkpoint.0, 0 | 1));
            observed_log |= checkpoint.1 > 0;
            calls += 1;
            assert!(
                Instant::now() < deadline,
                "Bounded actual concurrent checkpoint/writer journey"
            );
            thread::yield_now();
        }
        (checkpointer, calls, observed_log)
    });
    barrier.wait();
    let failures: Vec<String> = writers
        .into_iter()
        .filter_map(|writer| writer.join().unwrap())
        .collect();
    assert!(
        failures.is_empty(),
        "Concurrent writes failed: {failures:?}"
    );
    let (connection, calls, observed_log) = checker.join().unwrap();
    assert!(
        calls >= 64 && observed_log,
        "Checkpoints actually observed committed WAL frames, not only empty database calls"
    );
    let checkpoint: (i64, i64, i64) = connection
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .unwrap();
    assert_eq!(checkpoint, (0, 0, 0));
    let integrity: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .unwrap();
    assert_eq!(integrity, "ok");
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    drop(connection);
    drop(anchor);
    let reopened = Cache::open(&path).unwrap();
    for uid in [42, 43] {
        let scope = Scope::github("github.invalid", Some(uid)).unwrap();
        let records = reopened.list(&scope, Some("private")).unwrap();
        assert_eq!(records.len(), 128);
        for (iteration, record) in records.iter().enumerate() {
            assert_eq!(record.id, format!("record-{iteration:03}"));
            assert_eq!(
                record.payload,
                json!({"uid":uid,"index":iteration,"body":body(uid,iteration)})
            );
        }
        assert_eq!(
            reopened.draft(&scope, "composer/comment").unwrap(),
            Some(body(uid, 127))
        );
    }
    assert!(reopened
        .list(&Scope::github("github.invalid", Some(44)).unwrap(), None)
        .unwrap()
        .is_empty());
    let independent = Connection::open(&path).unwrap();
    let principals: Vec<(String, u64)> = independent
        .prepare("SELECT login,count(*) FROM records GROUP BY login ORDER BY login")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        principals,
        [("user-id:42".into(), 128), ("user-id:43".into(), 128)]
    );
}

#[test]
fn sqlite_dependency_unsafe_owned_cache_aliases_refuse_without_original_sql_changes() {
    let fixture = private_fixture();
    let path = fixture.path().join("cache.sqlite3");
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    let cache = Cache::open(&path).unwrap();
    cache
        .put_draft(&scope, "exact original", "retained original bytes café")
        .unwrap();
    drop(cache);
    let original = fs::read(&path).unwrap();
    let symbolic = fixture.path().join("symbolic.sqlite3");
    symlink(&path, &symbolic).unwrap();
    assert!(Cache::open(&symbolic).is_err());
    assert_eq!(fs::read(&path).unwrap(), original);
    let hard = fixture.path().join("hard.sqlite3");
    fs::hard_link(&path, &hard).unwrap();
    assert!(Cache::open(&hard).is_err());
    assert!(Cache::open(&path).is_err());
    assert_eq!(fs::read(&path).unwrap(), original);
    let directory = fixture.path().join("directory.sqlite3");
    fs::create_dir(&directory).unwrap();
    assert!(Cache::open(&directory).is_err());
}
