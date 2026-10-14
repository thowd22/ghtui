//! Undeclared finite volatile scheduling quota, NEVER permission/read authority.
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

pub const WATCH_TICKS_V2: usize = 12;
pub const WATCH_DURATION_V2: Duration = Duration::from_secs(120);
pub const WATCH_BYTES_V2: usize = 32 * 1024 * 1024;
pub const WATCH_GROUP_BYTES_V2: usize = 2 * (4096 + 64 * 1024) + 2 * 128 * 1024 + 2 * 1024 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrCheckWatchModeV2 {
    Status,
    SelectedJobLogs,
}
struct Quota {
    ticks: usize,
    charged: usize,
    in_flight: bool,
    next: Instant,
}
pub struct PrCheckWatchQuotaV2 {
    started: Instant,
    active: AtomicBool,
    quota: Mutex<Quota>,
}
impl std::fmt::Debug for PrCheckWatchQuotaV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PrCheckWatchQuotaV2(<finite quota, not access>)")
    }
}
impl PrCheckWatchQuotaV2 {
    pub(crate) fn new() -> Arc<Self> {
        let now = Instant::now();
        Arc::new(Self {
            started: now,
            active: AtomicBool::new(true),
            quota: Mutex::new(Quota {
                ticks: 0,
                charged: 0,
                in_flight: false,
                next: now + Duration::from_secs(5),
            }),
        })
    }
    pub fn active(&self) -> bool {
        self.active.load(Ordering::Acquire) && self.started.elapsed() < WATCH_DURATION_V2
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.started + WATCH_DURATION_V2
    }
    pub(crate) fn stop(&self) {
        self.active.store(false, Ordering::Release);
    }
    pub(crate) fn due(&self) -> bool {
        self.active()
            && self.quota.lock().is_ok_and(|q| {
                !q.in_flight
                    && q.ticks < WATCH_TICKS_V2
                    && Instant::now() >= q.next
                    && q.charged
                        .checked_add(WATCH_GROUP_BYTES_V2)
                        .is_some_and(|n| n <= WATCH_BYTES_V2)
            })
    }
    pub(crate) fn claim(self: &Arc<Self>) -> Result<PrCheckWatchClaimV2, String> {
        let mut q = self
            .quota
            .lock()
            .map_err(|_| "Watch quota unavailable".to_owned())?;
        if !self.active()
            || q.in_flight
            || q.ticks >= WATCH_TICKS_V2
            || q.charged
                .checked_add(WATCH_GROUP_BYTES_V2)
                .is_none_or(|n| n > WATCH_BYTES_V2)
        {
            self.stop();
            return Err("Finite watch quota/time exhausted; rearm explicitly".into());
        }
        q.in_flight = true;
        q.ticks += 1;
        q.charged += WATCH_GROUP_BYTES_V2;
        Ok(PrCheckWatchClaimV2 {
            quota: self.clone(),
            settled: false,
        })
    }
}
pub(crate) struct PrCheckWatchClaimV2 {
    quota: Arc<PrCheckWatchQuotaV2>,
    settled: bool,
}
impl PrCheckWatchClaimV2 {
    pub(crate) fn finish(mut self, actual: usize) -> Result<(usize, usize), String> {
        if actual > WATCH_GROUP_BYTES_V2 {
            self.quota.stop();
            return Err(
                "Actual watch group exceeded original selected quota; private result suppressed"
                    .into(),
            );
        }
        let mut q = self
            .quota
            .quota
            .lock()
            .map_err(|_| "Watch quota unavailable".to_owned())?;
        q.charged -= WATCH_GROUP_BYTES_V2 - actual;
        q.in_flight = false;
        q.next = Instant::now() + Duration::from_secs(5);
        self.settled = true;
        if !self.quota.active() {
            return Err("Watch stopped/expired; no completion displayed".into());
        }
        Ok((q.ticks, q.charged))
    }
}
impl Drop for PrCheckWatchClaimV2 {
    fn drop(&mut self) {
        if !self.settled {
            self.quota.stop();
            if let Ok(mut q) = self.quota.quota.lock() {
                q.in_flight = false;
            }
        }
    }
}
#[cfg(test)]
#[path = "pr_check_watch_quota_v2_tests.rs"]
mod tests;
