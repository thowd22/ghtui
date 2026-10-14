//! Small prepared clone-progress snapshots. No subprocess or filesystem work during drawing.
use crate::git::RawPath;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CorpusPhase {
    #[default]
    Idle,
    Validating,
    Transferring,
    Verifying,
    Published,
    Retained,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CorpusProgress {
    pub phase: CorpusPhase,
    pub target: Option<RawPath>,
    pub observed_disk_bytes: u64,
    pub observed_entries: usize,
    pub observation_complete: bool,
    pub elapsed_millis: u64,
    pub saved_commit: Option<String>,
    pub recovery_directory: Option<RawPath>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ProgressHandle(Arc<Mutex<CorpusProgress>>);

impl ProgressHandle {
    /// Return prepared bounded state. Never fetch, scan or decode in a renderer.
    pub fn snapshot(&self) -> CorpusProgress {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    pub(crate) fn update(&self, update: impl FnOnce(&mut CorpusProgress)) {
        update(&mut self.0.lock().unwrap_or_else(|p| p.into_inner()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_corpus_prepared_progress_keeps_exact_raw_target_across_worker_updates() {
        let progress = ProgressHandle::default();
        assert_eq!(progress.snapshot().phase, CorpusPhase::Idle);
        let target = RawPath::new(b"/private/managed/odd-\xff-path".to_vec());
        let expected = target.clone();
        let worker_handle = progress.clone();
        std::thread::spawn(move || {
            worker_handle.update(|state| {
                state.phase = CorpusPhase::Transferring;
                state.target = Some(target);
                state.observed_disk_bytes = 4096;
                state.observed_entries = 17;
                state.observation_complete = true;
                state.elapsed_millis = 123;
            });
            worker_handle.update(|state| {
                state.phase = CorpusPhase::Published;
                state.saved_commit = Some("a".repeat(40));
            });
        })
        .join()
        .unwrap();
        let snapshot = progress.snapshot();
        assert_eq!(snapshot.phase, CorpusPhase::Published);
        assert_eq!(snapshot.target, Some(expected));
        assert_eq!(snapshot.observed_disk_bytes, 4096);
        assert_eq!(snapshot.observed_entries, 17);
        assert!(snapshot.observation_complete);
        assert_eq!(snapshot.elapsed_millis, 123);
        assert_eq!(
            snapshot.saved_commit.as_deref(),
            Some("a".repeat(40).as_str())
        );
        assert_eq!(snapshot.recovery_directory, None);
        assert_eq!(snapshot.error, None);
    }
}
