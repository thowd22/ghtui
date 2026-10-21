//! TASK-104: truthful local draft cleanup after a CONFIRMED publication.
//!
//! A cleanup Remove that the ordered draft queue accepted but did not
//! acknowledge in time proves nothing about the stored body: completion still
//! owns it and may delete it later. That outcome is reported as "cleanup
//! unconfirmed", never as "draft retained" or "draft deleted". Those two words
//! are used only for an actual ACK, or for a fresh inspection taken when no
//! accepted cleanup can still be running.
use super::{DraftClient, Message};
use crate::storage::{Cache, Scope};
use std::{
    path::Path,
    sync::mpsc::{self, RecvTimeoutError, TrySendError},
    time::{Duration, Instant},
};

/// The longest a confirmed publication waits for its cleanup ACK. Longer than
/// one SQLite lock wait (`busy_v1::BUSY_BUDGET`), so an ordinary lock failure
/// is reported (and inspected) rather than turned into an unknown outcome.
pub const CONFIRMED_CLEANUP_ACK_WAIT: Duration = Duration::from_secs(3);

pub const DRAFT_DELETED: &str = "draft deleted";
pub const DRAFT_RETAINED: &str = "draft retained";
pub const CLEANUP_UNCONFIRMED: &str = "cleanup unconfirmed";

/// What is actually known about the stored draft after a confirmed write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DraftCleanupOutcome {
    /// The ordered queue acknowledged the DELETE, or a fresh read with no
    /// cleanup in flight found no stored row.
    Deleted,
    /// No cleanup is in flight and a fresh read still finds the stored row.
    Retained(String),
    /// Accepted into the ordered queue without an ACK (late, lost or the
    /// worker stopped), or the fresh read failed. Retention is not proven.
    Unconfirmed(String),
}

impl DraftCleanupOutcome {
    /// Short status caption; the three states never share a leading phrase.
    pub fn caption(&self) -> String {
        match self {
            Self::Deleted => DRAFT_DELETED.into(),
            Self::Retained(reason) => format!("{DRAFT_RETAINED} ({reason})"),
            Self::Unconfirmed(reason) => {
                format!("{CLEANUP_UNCONFIRMED} ({reason}) • inspect before reuse; do not resend")
            }
        }
    }
}

/// Fresh read used only when no accepted cleanup can still change the row.
fn inspect(database: &Path, scope: &Scope, resource: &str, reason: String) -> DraftCleanupOutcome {
    match Cache::open(database).and_then(|cache| cache.draft(scope, resource)) {
        Ok(Some(_)) => DraftCleanupOutcome::Retained(reason),
        Ok(None) => DraftCleanupOutcome::Deleted,
        Err(error) => {
            DraftCleanupOutcome::Unconfirmed(format!("{reason}; inspection failed: {error}"))
        }
    }
}

impl DraftClient {
    /// Remove the exact composer draft after a confirmed remote effect, waiting
    /// at most `wait` for the ordered queue's ACK. Never retries or resends.
    pub fn remove_confirmed_publication_v104(
        &self,
        database: &Path,
        scope: Scope,
        resource: String,
        wait: Duration,
    ) -> DraftCleanupOutcome {
        let deadline = Instant::now() + wait;
        let activity = match self.producer.begin_legacy() {
            Ok(activity) => activity,
            Err(error) => {
                return inspect(
                    database,
                    &scope,
                    &resource,
                    format!("cleanup not queued: {error}"),
                )
            }
        };
        let (sender, reply) = mpsc::sync_channel(1);
        let mut message = Message::Remove(scope.clone(), resource.clone(), sender, Some(activity));
        loop {
            match self.sender.try_send(message) {
                Ok(()) => break,
                Err(TrySendError::Full(original)) if Instant::now() < deadline => {
                    message = original;
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(TrySendError::Full(original) | TrySendError::Disconnected(original)) => {
                    // Never accepted: dropping it releases its activity here.
                    drop(original);
                    return inspect(
                        database,
                        &scope,
                        &resource,
                        "cleanup not queued: draft writer busy or closed".into(),
                    );
                }
            }
        }
        // Accepted: from here on only an actual ACK may claim an outcome.
        match reply.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(Ok(())) => DraftCleanupOutcome::Deleted,
            // The worker finished this message, so nothing accepted is still
            // running for it; a fresh read is a real observation.
            Ok(Err(error)) => inspect(
                database,
                &scope,
                &resource,
                format!("cleanup failed: {error}"),
            ),
            Err(RecvTimeoutError::Timeout) => DraftCleanupOutcome::Unconfirmed(format!(
                "no local ACK within {}s",
                wait.as_secs_f32()
            )),
            Err(RecvTimeoutError::Disconnected) => DraftCleanupOutcome::Unconfirmed(
                "draft writer stopped after accepting cleanup".into(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drafts::DraftWriter;

    fn fixture() -> (tempfile::TempDir, std::path::PathBuf, Scope) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("private/cache.sqlite3");
        let scope = Scope::github("github.invalid", Some(42)).unwrap();
        let cache = Cache::open(&path).unwrap();
        cache
            .put_draft(&scope, "composer/comment", "café body")
            .unwrap();
        (directory, path, scope)
    }

    #[test]
    fn confirmed_cleanup_v104_ack_reports_draft_deleted() {
        let (_directory, path, scope) = fixture();
        let writer = DraftWriter::new(path.clone());
        let outcome = writer.client().remove_confirmed_publication_v104(
            &path,
            scope.clone(),
            "composer/comment".into(),
            CONFIRMED_CLEANUP_ACK_WAIT,
        );
        assert_eq!(outcome, DraftCleanupOutcome::Deleted);
        assert_eq!(outcome.caption(), "draft deleted");
        drop(writer);
        assert!(Cache::open(&path)
            .unwrap()
            .draft(&scope, "composer/comment")
            .unwrap()
            .is_none());
    }

    #[test]
    fn confirmed_cleanup_v104_failed_delete_is_inspected_as_retained() {
        let (_directory, path, scope) = fixture();
        rusqlite::Connection::open(&path)
            .unwrap()
            .execute_batch(
                "CREATE TRIGGER refuse_cleanup BEFORE DELETE ON drafts
                 BEGIN SELECT RAISE(ABORT,'fixture refused cleanup'); END;",
            )
            .unwrap();
        let writer = DraftWriter::new(path.clone());
        let outcome = writer.client().remove_confirmed_publication_v104(
            &path,
            scope.clone(),
            "composer/comment".into(),
            CONFIRMED_CLEANUP_ACK_WAIT,
        );
        assert!(
            matches!(&outcome, DraftCleanupOutcome::Retained(reason) if reason.contains("fixture refused cleanup")),
            "{outcome:?}"
        );
        assert!(outcome.caption().starts_with("draft retained"));
        drop(writer);
        assert_eq!(
            Cache::open(&path)
                .unwrap()
                .draft(&scope, "composer/comment")
                .unwrap()
                .as_deref(),
            Some("café body")
        );
    }

    #[test]
    fn confirmed_cleanup_v104_closed_writer_never_queues_and_inspects_retained() {
        let (_directory, path, scope) = fixture();
        let writer = DraftWriter::new(path.clone());
        let client = writer.client();
        drop(writer);
        let outcome = client.remove_confirmed_publication_v104(
            &path,
            scope.clone(),
            "composer/comment".into(),
            CONFIRMED_CLEANUP_ACK_WAIT,
        );
        assert!(
            matches!(&outcome, DraftCleanupOutcome::Retained(reason) if reason.starts_with("cleanup not queued")),
            "{outcome:?}"
        );
        assert!(Cache::open(&path)
            .unwrap()
            .draft(&scope, "composer/comment")
            .unwrap()
            .is_some());
    }

    /// The accepted Remove is held behind a real SQLite write lock past the
    /// ACK bound. The row is still stored when the outcome is reported, and
    /// the late completion deletes it afterwards: neither "retained" nor
    /// "deleted" would have been true, so only "cleanup unconfirmed" is.
    #[test]
    fn confirmed_cleanup_v104_late_ack_is_unconfirmed_and_late_completion_still_deletes() {
        let (_directory, path, scope) = fixture();
        let writer = DraftWriter::new(path.clone());
        let client = writer.client();
        // Let the worker open its cache before the lock is taken.
        client
            .save_and_wait(scope.clone(), "other".into(), "other".into())
            .unwrap();
        let mut blocker = rusqlite::Connection::open(&path).unwrap();
        let transaction = blocker
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let outcome = client.remove_confirmed_publication_v104(
            &path,
            scope.clone(),
            "composer/comment".into(),
            Duration::from_millis(100),
        );
        assert!(
            matches!(&outcome, DraftCleanupOutcome::Unconfirmed(reason) if reason.starts_with("no local ACK")),
            "{outcome:?}"
        );
        assert!(outcome.caption().starts_with("cleanup unconfirmed"));
        assert!(!outcome.caption().contains(DRAFT_RETAINED));
        assert!(!outcome.caption().contains(DRAFT_DELETED));
        let stored: Option<String> = transaction
            .query_row(
                "SELECT body FROM drafts WHERE host=?1 AND login=?2 AND resource='composer/comment'",
                rusqlite::params![scope.host, scope.login],
                |row| row.get(0),
            )
            .ok();
        assert_eq!(
            stored.as_deref(),
            Some("café body"),
            "still stored when reported"
        );
        transaction.rollback().unwrap();
        // Joining the worker waits for the accepted late completion.
        drop(writer);
        assert!(Cache::open(&path)
            .unwrap()
            .draft(&scope, "composer/comment")
            .unwrap()
            .is_none());
    }
}
