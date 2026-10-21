//! One in-process schema initializer per cache file (TASK-111).
//!
//! On a first launch the draft writer creates the whole cache schema in one write
//! transaction while the startup Inbox work opens the same file. A SQLite trace showed the
//! Inbox open queueing for that write lock through the busy handler, and the commit's syncs
//! were most of the span. Under heavy disk load the span outlasted the 2 s lock budget, so an
//! early Refresh failed with `database is locked`. Opens in this process now queue on this
//! gate instead: a later opener waits for the running initializer to finish, then finds the
//! schema current and needs no write lock at all. Other processes still meet SQLite's own
//! locking and lock budget.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, OnceLock, PoisonError};

/// Whether an opener holds the gate, and the condition its waiters sleep on.
#[derive(Default)]
struct Gate {
    held: Mutex<bool>,
    released: Condvar,
}

type Gates = Mutex<HashMap<PathBuf, Arc<Gate>>>;

fn gates() -> &'static Gates {
    static GATES: OnceLock<Gates> = OnceLock::new();
    GATES.get_or_init(Default::default)
}

/// Held while one opener checks, creates or migrates the schema of one cache file.
pub(crate) struct InitGate(Arc<Gate>);

impl Drop for InitGate {
    fn drop(&mut self) {
        *self.0.held.lock().unwrap_or_else(PoisonError::into_inner) = false;
        self.0.released.notify_one();
    }
}

fn key(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Waits until no other thread of this process is checking or initializing `path`, then
/// holds the gate until the returned value drops.
pub(crate) fn enter(path: &Path) -> InitGate {
    let gate = {
        let mut gates = gates().lock().unwrap_or_else(PoisonError::into_inner);
        // Forget gates nobody holds or waits on, so temporary files do not accumulate.
        gates.retain(|_, gate| Arc::strong_count(gate) > 1);
        gates.entry(key(path)).or_default().clone()
    };
    let mut held = gate.held.lock().unwrap_or_else(PoisonError::into_inner);
    while *held {
        held = gate
            .released
            .wait(held)
            .unwrap_or_else(PoisonError::into_inner);
    }
    *held = true;
    drop(held);
    InitGate(gate)
}

/// Waits for any initializer of `path` in this process to finish, without holding the gate.
pub(crate) fn wait(path: &Path) {
    drop(enter(path));
}

#[cfg(test)]
pub(crate) mod test_pause {
    //! Lets a test hold the first initializer of one file inside its write transaction.
    use std::collections::HashMap;
    use std::sync::{mpsc::SyncSender, Mutex, OnceLock};
    use std::time::Duration;

    type Pauses = Mutex<HashMap<String, (Duration, SyncSender<()>)>>;

    fn pauses() -> &'static Pauses {
        static PAUSES: OnceLock<Pauses> = OnceLock::new();
        PAUSES.get_or_init(Default::default)
    }

    /// The next initializer of the database at `path` (as SQLite reports it) signals
    /// `holding` once it holds the write lock, then keeps it for `hold`.
    pub(crate) fn pause_next(path: &str, hold: Duration, holding: SyncSender<()>) {
        pauses()
            .lock()
            .unwrap()
            .insert(path.to_owned(), (hold, holding));
    }

    pub(crate) fn reached(path: Option<&str>) {
        let Some(path) = path else { return };
        let pause = pauses().lock().unwrap().remove(path);
        if let Some((hold, holding)) = pause {
            let _ = holding.send(());
            std::thread::sleep(hold);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_pause;
    use crate::storage::{busy_v1, Cache, Scope};
    use std::time::{Duration, Instant};

    /// A first launch: the draft writer's open creates the schema while the startup Inbox
    /// work (and an early Refresh) opens the same file. The creator keeps its write
    /// transaction for longer than the whole lock budget, as a slow disk did in TASK-66.
    #[test]
    fn init_gate_v111_open_during_a_slow_first_schema_creation_waits_and_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        // SQLite reports the resolved path, so name the file the same way.
        let path = dir
            .path()
            .canonicalize()
            .unwrap()
            .join("private/cache.sqlite3");
        let hold = busy_v1::BUSY_BUDGET * 2 + Duration::from_millis(500);
        let (holding, held) = std::sync::mpsc::sync_channel(1);
        test_pause::pause_next(path.to_str().unwrap(), hold, holding);
        let startup_path = path.clone();
        let startup = std::thread::spawn(move || {
            Cache::open(&startup_path)
                .map(drop)
                .map_err(|error| error.to_string())
        });
        held.recv_timeout(Duration::from_secs(20))
            .expect("the startup open holds the schema write transaction");

        // The lock really is held past the budget: a plain writer gives up.
        let competing = rusqlite::Connection::open(&path).unwrap();
        busy_v1::install(&competing).unwrap();
        let started = Instant::now();
        let refused = competing.execute_batch("BEGIN IMMEDIATE").unwrap_err();
        assert!(
            refused.to_string().contains("database is locked"),
            "{refused}"
        );
        assert!(started.elapsed() >= busy_v1::BUSY_BUDGET);
        drop(competing);

        // The Refresh's open queues behind the creator and succeeds without a lock error.
        let started = Instant::now();
        let refresh = Cache::open(&path).expect("open waits for the schema creation");
        let waited = started.elapsed();
        assert!(waited >= Duration::from_millis(300), "{waited:?}");
        startup.join().unwrap().unwrap();

        // The queued connection then writes and reads the account cache normally.
        let scope = Scope::github("github.com", Some(42)).unwrap();
        refresh
            .put_draft(&scope, "review", "early refresh")
            .unwrap();
        let reopened = Cache::open(&path).unwrap();
        assert_eq!(
            reopened.draft(&scope, "review").unwrap().as_deref(),
            Some("early refresh")
        );
        let version: u32 = reopened
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 4);
    }
}
