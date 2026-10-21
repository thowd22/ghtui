//! Fair SQLite lock waits (TASK-102). SQLite's default busy handler backs off to 100 ms
//! sleeps, so under CPU load a waiting writer can lose every retry to a busier writer for
//! its whole budget and fail with "database is locked". This handler retries every
//! millisecond within the same 2 s budget, ending sooner while a [`BusyDeadline`] guard
//! for a caller's own deadline is alive on this thread.
use rusqlite::Connection;
use std::cell::Cell;
use std::time::{Duration, Instant};

/// The longest one lock wait may take, as with the former `busy_timeout(2 s)`.
pub(crate) const BUSY_BUDGET: Duration = Duration::from_secs(2);

thread_local! {
    static STARTED: Cell<Option<Instant>> = const { Cell::new(None) };
    static DEADLINE: Cell<Option<Instant>> = const { Cell::new(None) };
}

/// SQLite passes the number of earlier calls for the same lock event, so 0 starts a wait.
fn retry(attempt: i32) -> bool {
    let now = Instant::now();
    if attempt == 0 {
        STARTED.set(Some(now));
    }
    let started = STARTED.get().unwrap_or(now);
    let end = DEADLINE.get().map_or(started + BUSY_BUDGET, |deadline| {
        deadline.min(started + BUSY_BUDGET)
    });
    if now >= end {
        return false;
    }
    std::thread::sleep(Duration::from_millis(1).min(end - now));
    true
}

/// When a wait starting now must end: the full budget, or sooner while a
/// [`BusyDeadline`] guard is alive on this thread.
pub(crate) fn wait_end_from_now() -> Instant {
    let end = Instant::now() + BUSY_BUDGET;
    DEADLINE.get().map_or(end, |deadline| deadline.min(end))
}

/// Uses the fair handler for every lock wait on `connection`.
pub(crate) fn install(connection: &Connection) -> rusqlite::Result<()> {
    connection.busy_handler(Some(retry))
}

/// While alive, lock waits on this thread also end at its deadline. Dropping it restores
/// the previous bound, so a caller's short deadline never outlives its transaction.
#[must_use = "the deadline only holds while the guard is alive"]
pub(crate) struct BusyDeadline(Option<Instant>);

impl Drop for BusyDeadline {
    fn drop(&mut self) {
        DEADLINE.set(self.0);
    }
}

/// Installs the fair handler and bounds this thread's lock waits by `deadline`.
pub(crate) fn until(connection: &Connection, deadline: Instant) -> rusqlite::Result<BusyDeadline> {
    install(connection)?;
    let bound = DEADLINE.get().map_or(deadline, |outer| outer.min(deadline));
    Ok(BusyDeadline(DEADLINE.replace(Some(bound))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    fn locked_pair() -> (tempfile::TempDir, Connection, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("busy.sqlite3");
        let holder = Connection::open(&path).unwrap();
        holder.execute_batch("CREATE TABLE t(x INTEGER)").unwrap();
        let waiter = Connection::open(&path).unwrap();
        install(&waiter).unwrap();
        (dir, holder, waiter)
    }

    #[test]
    fn busy_v1_waits_for_a_lock_released_within_the_budget() {
        let (dir, holder, waiter) = locked_pair();
        holder.execute_batch("BEGIN IMMEDIATE").unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let release = barrier.clone();
        let handle = std::thread::spawn(move || {
            release.wait();
            std::thread::sleep(Duration::from_millis(150));
            holder.execute_batch("COMMIT").unwrap();
        });
        barrier.wait();
        let started = Instant::now();
        waiter.execute("INSERT INTO t VALUES (1)", []).unwrap();
        assert!(started.elapsed() >= Duration::from_millis(100));
        handle.join().unwrap();
        drop(dir);
    }

    #[test]
    fn busy_v1_gives_up_at_the_budget_and_a_guard_deadline_ends_sooner() {
        let (_dir, holder, waiter) = locked_pair();
        holder.execute_batch("BEGIN IMMEDIATE").unwrap();
        let started = Instant::now();
        {
            let _bound = until(&waiter, Instant::now() + Duration::from_millis(200)).unwrap();
            assert!(waiter.execute("INSERT INTO t VALUES (1)", []).is_err());
        }
        let bounded = started.elapsed();
        assert!(bounded >= Duration::from_millis(190) && bounded < Duration::from_secs(1));
        // The guard is gone, so the next wait uses the full budget again.
        let started = Instant::now();
        assert!(waiter.execute("INSERT INTO t VALUES (2)", []).is_err());
        assert!(started.elapsed() >= BUSY_BUDGET);
        holder.execute_batch("ROLLBACK").unwrap();
    }
}
