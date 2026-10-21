//! TASK-117: follow an Actions run while it runs.
//!
//! Opening a run that has not completed starts a bounded live watch: the run, its jobs and
//! their steps are re-read every 5 s (backing off to 30 s while nothing changes), and the
//! watch stops when the run completes, when the run is left, when `w` stops it, or after a
//! bounded number of checks. GitHub serves a job's log only once that job has completed, so
//! the live Logs page shows each finished job's log as soon as it can be read, and says on a
//! running job's section when its log will appear. `F Follow` keeps the newest line in view.
use super::*;
use std::collections::BTreeMap;

/// The first and the shortest live interval.
pub const LIVE_POLL_MIN_SECS_V117: u64 = 5;
/// The longest interval reached by backing off while nothing changes.
pub const LIVE_POLL_MAX_SECS_V117: u64 = 30;
/// A bound on checks for one opened run; `w` resumes watching a run that is still going.
pub const LIVE_POLL_MAX_TICKS_V117: u32 = 720;
/// At most this many finished job logs are read per update, so one tick stays bounded.
const LIVE_LOGS_PER_TICK_V117: usize = 8;
pub const FOLLOW_LABEL_V117: &str = "[F Follow]";
pub const FOLLOWING_LABEL_V117: &str = "[F Stop following]";

#[derive(Clone, Debug, Default)]
pub struct LiveRunV117 {
    /// Started automatically because the opened run is in progress. Stops when the run
    /// finishes, the run is left, `w` stops it, or the check bound is reached.
    pub armed: bool,
    pub interval_secs: u64,
    pub ticks: u32,
    signature: String,
    /// The run attempt the cached logs belong to.
    run_key: Option<(String, u64, u64)>,
    /// Readable logs of finished jobs, each read once.
    logs: BTreeMap<u64, String>,
    /// Why a finished job's log could not be read yet; it is retried on the next update.
    errors: BTreeMap<u64, String>,
    /// The Logs page shows a live log: of one job, or (None) of every job of the attempt.
    pub target: Option<Option<u64>>,
    /// Keep the newest log line selected and in view.
    pub follow: bool,
}

/// A run that moves by itself: queued for or running on a runner. A run `waiting` on a
/// deployment approval is not followed automatically (it waits for a person); `w` watches it.
pub fn runs_live_v117(status: &str) -> bool {
    matches!(status, "queued" | "in_progress" | "requested" | "pending")
}
/// Job and step states that, when unchanged between checks, let the interval back off.
fn signature(attempt: &RunAttempt) -> String {
    let mut out = format!("{}/{}", attempt.run.status, attempt.run.conclusion);
    for job in &attempt.jobs {
        out.push_str(&format!("|{}:{}/{}", job.id, job.status, job.conclusion));
        for step in &job.steps {
            out.push_str(&format!(
                ",{}:{}/{}",
                step.number, step.status, step.conclusion
            ));
        }
    }
    out
}

/// How long a job or step took, or for how long it has been running.
pub fn live_timing_v117(started: &str, completed: &str, now: u64) -> Option<String> {
    if let Some(took) = crate::ui_table::duration(started, completed) {
        return Some(took);
    }
    if !completed.is_empty() {
        return None;
    }
    let start = crate::github_workspace::timestamp_seconds(started)?;
    let seconds = now.checked_sub(start)?;
    Some(format!(
        "running {}",
        match seconds {
            0..60 => format!("{seconds}s"),
            60..3600 => format!("{}m {:02}s", seconds / 60, seconds % 60),
            _ => format!("{}h {:02}m", seconds / 3600, seconds % 3600 / 60),
        }
    ))
}

impl ActionsWorkspace {
    /// Seconds until the next watch check.
    pub fn poll_interval_secs(&self) -> u64 {
        if self.live.armed {
            self.live.interval_secs.max(LIVE_POLL_MIN_SECS_V117)
        } else {
            LIVE_POLL_MIN_SECS_V117
        }
    }
    /// The open run's cached logs belong to it; another run or attempt starts afresh.
    fn live_key_v117(&mut self) {
        let key = self.run.as_ref().map(|page| {
            (
                page.repository.clone(),
                page.attempt.run.id,
                page.attempt.run.attempt,
            )
        });
        if self.live.run_key != key {
            let follow = self.live.follow;
            self.live = LiveRunV117 {
                run_key: key,
                follow,
                ..LiveRunV117::default()
            };
        }
    }
    fn start_live_v117(&mut self) {
        self.live.armed = true;
        self.live.interval_secs = LIVE_POLL_MIN_SECS_V117;
        self.live.ticks = 0;
        if let Some(page) = &self.run {
            self.live.signature = signature(&page.attempt);
        }
        self.watching = true;
        self.relabel_watch_v117();
    }
    fn relabel_watch_v117(&mut self) {
        let label = self.watch_label();
        for control in &mut self.controls {
            if control.action == ActionsAction::Watch {
                control.label = label.into();
            }
        }
    }
    /// After a run opens: one that has not completed is followed live without a manual Watch.
    pub(super) fn arm_live_v117(&mut self) {
        self.live_key_v117();
        self.live.target = None;
        let running = self
            .run
            .as_ref()
            .is_some_and(|page| runs_live_v117(&page.attempt.run.status));
        if running {
            self.start_live_v117();
        } else if self.live.armed {
            self.live.armed = false;
            self.watching = false;
        }
    }
    /// One live check has re-read the run: back off while nothing changed, stop at the bound.
    pub(super) fn after_live_tick_v117(&mut self) {
        if !self.live.armed {
            return;
        }
        let Some(page) = &self.run else {
            return;
        };
        let now = signature(&page.attempt);
        let completed = !runs_live_v117(&page.attempt.run.status);
        self.live.ticks += 1;
        if now == self.live.signature {
            self.live.interval_secs = (self.live.interval_secs * 2)
                .clamp(LIVE_POLL_MIN_SECS_V117, LIVE_POLL_MAX_SECS_V117);
        } else {
            self.live.interval_secs = LIVE_POLL_MIN_SECS_V117;
            self.live.signature = now;
        }
        if completed {
            self.live.armed = false;
        } else if self.live.ticks >= LIVE_POLL_MAX_TICKS_V117 {
            self.live.armed = false;
            self.watching = false;
            self.watch_note = Some(format!(
                "Live updates paused after {LIVE_POLL_MAX_TICKS_V117} checks • w resumes"
            ));
        }
    }
    /// A run that completed by any route (a live check, or a confirmed cancel) is not
    /// polled again; nor is a page whose run was closed.
    pub(super) fn settle_live_v117(&mut self) {
        if self.live.armed
            && !self
                .run
                .as_ref()
                .is_some_and(|page| runs_live_v117(&page.attempt.run.status))
        {
            self.live.armed = false;
            self.watching = false;
        }
    }
    /// Pressing `w` to stop also ends the automatic live watch.
    pub(super) fn stop_live_v117(&mut self) {
        self.live.armed = false;
    }
    /// After any action: leaving the run (or its logs) stops the automatic live watch.
    pub(super) fn leave_live_v117(&mut self) {
        if self.view != ActionsView::Logs {
            self.live.target = None;
        }
        if self.live.armed
            && !matches!(
                self.view,
                ActionsView::Run | ActionsView::Jobs | ActionsView::Logs
            )
        {
            self.live.armed = false;
            self.watching = false;
            self.watch_note = Some("Live updates stopped: you left the run".into());
            self.relabel_watch_v117();
        }
    }
    /// The run is still in progress: show what is finished now, and keep updating.
    pub(super) fn open_live_log_v117(
        &mut self,
        gh: &GithubProvider,
        job: Option<u64>,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        let page = self
            .run
            .as_ref()
            .ok_or_else(|| plain_error("Open an exact run attempt first"))?;
        if let Some(id) = job {
            if !page.attempt.jobs.iter().any(|candidate| candidate.id == id) {
                return Err(plain_error("Job belongs to another attempt"));
            }
        }
        let running = runs_live_v117(&page.attempt.run.status);
        self.live_key_v117();
        self.live.target = Some(job);
        self.fetch_live_logs_v117(gh, cancel);
        if let Some(page) = &mut self.run {
            page.selected_job = job;
            page.logs_skipped_bytes = 0;
        }
        self.rebuild_live_log_v117();
        if running && !self.watching {
            self.start_live_v117();
        }
        self.view = ActionsView::Logs;
        self.selected = 0;
        self.top = 0;
        Ok(())
    }
    /// A watch check on the live Logs page: re-read the run, then any newly finished logs.
    pub(super) fn refresh_live_log_v117(
        &mut self,
        gh: &GithubProvider,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        if self.live.target.is_none() {
            return Ok(());
        }
        self.refresh_open_attempt(gh, cancel)?;
        if let Some(Some(id)) = self.live.target {
            if !self
                .run
                .as_ref()
                .is_some_and(|page| page.attempt.jobs.iter().any(|job| job.id == id))
            {
                // A new attempt replaced the job; show every job of the current attempt.
                self.live.target = Some(None);
            }
        }
        self.live_key_v117_keep_target();
        self.fetch_live_logs_v117(gh, cancel);
        self.rebuild_live_log_v117();
        Ok(())
    }
    fn live_key_v117_keep_target(&mut self) {
        let target = self.live.target;
        self.live_key_v117();
        self.live.target = target;
    }
    /// Reads the log of each target job that has finished and is not cached yet.
    fn fetch_live_logs_v117(&mut self, gh: &GithubProvider, cancel: &CancellationToken) {
        let (Some(target), Some(page)) = (self.live.target, &self.run) else {
            return;
        };
        let repository = page.repository.clone();
        let jobs = page
            .attempt
            .jobs
            .iter()
            .filter(|job| target.is_none_or(|id| id == job.id))
            .filter(|job| job.status == "completed" && !self.live.logs.contains_key(&job.id))
            .take(LIVE_LOGS_PER_TICK_V117)
            .cloned()
            .collect::<Vec<_>>();
        for job in jobs {
            if cancel.is_cancelled() {
                break;
            }
            match gh.job_log_v117(&repository, &job, cancel) {
                Ok(log) => {
                    let text = if log.skipped_bytes > 0 {
                        format!(
                            "▲ Truncated: first {} of this job's log skipped\n{}",
                            log_size_label(log.skipped_bytes),
                            log.text
                        )
                    } else {
                        log.text
                    };
                    self.live.logs.insert(job.id, text);
                    self.live.errors.remove(&job.id);
                }
                Err(error) => {
                    self.live.errors.insert(job.id, error.message);
                }
            }
        }
    }
    /// The live log text: each finished job's log, and for each running job its state,
    /// steps and when its log will appear.
    fn rebuild_live_log_v117(&mut self) {
        let Some(target) = self.live.target else {
            return;
        };
        let now = now();
        let Some(page) = &mut self.run else {
            return;
        };
        let mut jobs = page
            .attempt
            .jobs
            .iter()
            .filter(|job| target.is_none_or(|id| id == job.id))
            .collect::<Vec<_>>();
        // The newest output comes last: finished jobs in the order they finished, then the
        // jobs still running or queued (whose logs come next). Ties keep GitHub's order.
        jobs.sort_by_key(|job| {
            (
                job.status != "completed",
                (job.status == "completed").then(|| job.completed_at.clone()),
            )
        });
        let several = jobs.len() > 1;
        let mut text = String::new();
        if jobs.is_empty() {
            text.push_str("No jobs reported yet • they appear here as the run starts them\n");
        }
        for job in jobs {
            let finished = job.status == "completed";
            let timing = live_timing_v117(&job.started_at, &job.completed_at, now)
                .map(|timing| format!(" • {timing}"))
                .unwrap_or_default();
            if several || !finished {
                text.push_str(&format!(
                    "── {} • {}{timing} ──\n",
                    job.name,
                    outcome(&job.status, &job.conclusion)
                ));
            }
            if finished {
                match (self.live.logs.get(&job.id), self.live.errors.get(&job.id)) {
                    (Some(log), _) => text.push_str(log),
                    (None, Some(error)) => text.push_str(&format!(
                        "Log not readable yet: {error} • retried on the next update\n"
                    )),
                    (None, None) => text.push_str("Log loads on the next update\n"),
                }
                continue;
            }
            text.push_str(LIVE_LOG_PENDING_V117);
            text.push('\n');
            for step in &job.steps {
                text.push_str(&format!(
                    "  Step {} {} • {}{}\n",
                    step.number,
                    step.name,
                    outcome(&step.status, &step.conclusion),
                    live_timing_v117(&step.started_at, &step.completed_at, now)
                        .map(|timing| format!(" • {timing}"))
                        .unwrap_or_default()
                ));
            }
        }
        page.logs = Some(text);
    }
    /// `F`: keep the newest line in view, or stop doing so. Local; no request.
    pub fn toggle_follow_v117(&mut self) {
        self.live.follow = !self.live.follow;
        let label = self.follow_label_v117();
        for control in &mut self.controls {
            if control.action == ActionsAction::Follow {
                control.label = label.into();
            }
        }
        self.follow_newest_v117();
    }
    pub(super) fn follow_label_v117(&self) -> &'static str {
        if self.live.follow {
            FOLLOWING_LABEL_V117
        } else {
            FOLLOW_LABEL_V117
        }
    }
    /// With Follow on, a live log keeps its newest line selected (the table scrolls to it).
    pub(super) fn follow_newest_v117(&mut self) {
        if self.live.follow && self.view == ActionsView::Logs && self.live.target.is_some() {
            self.selected = self.rows.len().saturating_sub(1);
        }
    }
}

#[cfg(test)]
#[path = "live_run_v117_tests.rs"]
mod tests;
