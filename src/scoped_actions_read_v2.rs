//! Bounded retained log search and finite scheduling, never access authority.
pub(crate) use crate::pr_checks_native::scoped::choices::continuations_v2::LoadedLogSearchModeV2 as LogSearchModeV2;
use crate::{
    pr_checks_native::scoped::choices::continuations_v2::{
        prepare_loaded_log_indices_v3, LoadedLogSearchModeV2, PrCheckWatchClaimV2,
        PrCheckWatchQuotaV2,
    },
    providers::{gh::*, runner::CancellationToken},
    scoped_actions_v1::{ScopedActionsPageV1, ScopedActionsSourceV1},
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
pub struct ActionsLoadedMatchesV2 {
    snapshot: Arc<ScopedActionsLogSnapshotV2>,
    query: String,
    mode: LoadedLogSearchModeV2,
    indices: Vec<usize>,
}
impl ActionsLoadedMatchesV2 {
    pub(crate) fn prepare(
        snapshot: Arc<ScopedActionsLogSnapshotV2>,
        query: String,
        mode: LoadedLogSearchModeV2,
        deadline: Instant,
        cancel: &CancellationToken,
    ) -> Result<Arc<Self>, String> {
        let indices =
            prepare_loaded_log_indices_v3(snapshot.text(), &query, mode, deadline, cancel)?;
        if cancel.is_cancelled() || Instant::now() >= deadline {
            return Err("Original log search expired".into());
        }
        let prepared = Arc::new(Self {
            snapshot,
            query,
            mode,
            indices,
        });
        if cancel.is_cancelled() || Instant::now() >= deadline {
            return Err("Original log search expired".into());
        }
        Ok(prepared)
    }
    pub fn indices(&self) -> &[usize] {
        &self.indices
    }
    pub fn query(&self) -> &str {
        &self.query
    }
    pub fn mode(&self) -> LoadedLogSearchModeV2 {
        self.mode
    }
    pub fn snapshot(&self) -> &Arc<ScopedActionsLogSnapshotV2> {
        &self.snapshot
    }
    pub(crate) fn matches_page(&self, page: &ScopedActionsPageV1) -> bool {
        page.loaded_log_snapshot_v2()
            .is_some_and(|snapshot| Arc::ptr_eq(snapshot, &self.snapshot))
    }
    pub(crate) fn owned_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + 2 * std::mem::size_of::<usize>()
            + self.query.capacity()
            + self.indices.capacity() * std::mem::size_of::<usize>()
    }
}
/// No public constructor/serde. A query is bound only to this genuine retained
/// page's actual capture. It is not a renewed API/current-source lease.
pub(crate) struct ActionsLoadedQueryV2 {
    snapshot: Arc<ScopedActionsLogSnapshotV2>,
    query: String,
    mode: LoadedLogSearchModeV2,
}
impl ActionsLoadedQueryV2 {
    pub(crate) fn capture(
        page: &ScopedActionsPageV1,
        query: String,
        mode: LoadedLogSearchModeV2,
    ) -> Result<Arc<Self>, String> {
        if query.len() > 1024 || query.contains(['\n', '\r', '\0']) {
            return Err("Loaded-log query must be single-line and at most1024bytes".into());
        }
        Ok(Arc::new(Self {
            snapshot: page
                .loaded_log_snapshot_v2()
                .cloned()
                .ok_or("No actual retained job-log capture; no workspace String promoted")?,
            query,
            mode,
        }))
    }
    pub(crate) fn belongs_to(&self, page: &ScopedActionsPageV1) -> bool {
        page.loaded_log_snapshot_v2()
            .is_some_and(|snapshot| Arc::ptr_eq(snapshot, &self.snapshot))
    }
    pub(crate) fn prepare(
        &self,
        deadline: Instant,
        cancel: &CancellationToken,
    ) -> Result<Arc<ActionsLoadedMatchesV2>, String> {
        ActionsLoadedMatchesV2::prepare(
            self.snapshot.clone(),
            self.query.clone(),
            self.mode,
            deadline,
            cancel,
        )
    }
}
pub struct ActionsReadWatchV2 {
    source: Arc<ScopedActionsSourceV1>,
    run: Arc<ScopedActionsRunTargetV1>,
    job: Option<Arc<ScopedActionsJobTargetV1>>,
    quota: Arc<PrCheckWatchQuotaV2>,
    cancellation: CancellationToken,
}
impl ActionsReadWatchV2 {
    pub(crate) fn start(page: &ScopedActionsPageV1, logs: bool) -> Result<Arc<Self>, String> {
        let run = page
            .run()
            .cloned()
            .ok_or("Open an actual observed run before finite Watch")?;
        if run.node_v2().is_none() {
            return Err("Actual original run node unavailable; no watch identity guessed".into());
        }
        let job = if logs {
            let snapshot = page
                .loaded_log_snapshot_v2()
                .ok_or("Open actual selected-job logs before log Watch")?;
            if snapshot.job().node_v2().is_none() {
                return Err(
                    "Actual original job node unavailable; no watch identity guessed".into(),
                );
            }
            Some(snapshot.job().clone())
        } else {
            None
        };
        Ok(Arc::new(Self {
            source: page.source().clone(),
            run,
            job,
            quota: PrCheckWatchQuotaV2::new(),
            cancellation: CancellationToken::default(),
        }))
    }
    pub(crate) fn matches(&self, page: &ScopedActionsPageV1) -> bool {
        Arc::ptr_eq(&self.source, page.source())
            && page
                .run()
                .is_some_and(|run| run.same_watch_pin_v2(&self.run))
            && self.job.as_ref().is_none_or(|job| {
                page.job(job.id()).is_some_and(|current| {
                    current.node_v2() == job.node_v2() && current.run().same_watch_pin_v2(&self.run)
                })
            })
    }
    pub(crate) fn active(&self) -> bool {
        !self.cancellation.is_cancelled() && self.quota.active()
    }
    pub(crate) fn due(&self) -> bool {
        self.active() && self.quota.due()
    }
    pub(crate) fn stop(&self) {
        self.cancellation.cancel();
        self.quota.stop();
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.quota.deadline()
    }
    pub(crate) fn request_deadline(&self) -> Instant {
        (Instant::now() + Duration::from_secs(30)).min(self.deadline())
    }
    pub(crate) fn cancellation(&self) -> &CancellationToken {
        &self.cancellation
    }
    pub(crate) fn job(&self) -> Option<&Arc<ScopedActionsJobTargetV1>> {
        self.job.as_ref()
    }
    pub(crate) fn claim(&self, page: &ScopedActionsPageV1) -> Result<PrCheckWatchClaimV2, String> {
        if !self.active() || !self.matches(page) {
            return Err("Original finite Watch pin/source/token expired or changed".into());
        }
        self.quota.claim()
    }
}
