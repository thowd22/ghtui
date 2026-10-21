//! TASK-117: a finished job's log while its run is still in progress.
//!
//! `gh run view --log` refuses a run (or job) that is still in progress, and GitHub's REST
//! API serves a job's log only after that job completes. So a live run reads each job's log
//! by REST (`GET repos/{repo}/actions/jobs/{id}/logs`) as soon as that job has completed,
//! with memory bounded like every other log (only the tail of an oversized log is kept).
use super::*;
use crate::providers::runner::{redact, sanitize_terminal};

/// The plain reason a running job has no log yet; shown on the job's own log section.
pub const LIVE_LOG_PENDING_V117: &str =
    "Log appears when this job finishes • GitHub serves a job's log only after it completes";

impl GithubProvider {
    /// One COMPLETED job's log by REST. A job that has not completed is refused locally,
    /// without a request: GitHub would answer with nothing.
    pub fn job_log_v117(
        &self,
        repo: &str,
        job: &ActionsJob,
        cancel: &CancellationToken,
    ) -> GithubResult<RunLogTail> {
        validate_repo(repo)?;
        if job.id == 0 {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Job logs need an exact job id",
            ));
        }
        if job.status != "completed" {
            return Err(error(GithubErrorKind::InvalidTarget, LIVE_LOG_PENDING_V117));
        }
        let args = vec![
            "api".into(),
            format!("repos/{repo}/actions/jobs/{}/logs", job.id),
            "--hostname".into(),
            self.host.clone(),
            "--method".into(),
            "GET".into(),
        ];
        let (raw, skipped) = self.execute_tail(args, cancel)?;
        Ok(job_log_from_raw_v117(&raw, skipped))
    }
}

/// Readable text of one job's raw REST log (`<timestamp> text` lines): timestamps are
/// dropped, `##[group]Name` becomes a `── Name ──` header and `##[endgroup]` disappears.
/// When bytes were skipped the first kept line is partial, so it is dropped and counted.
pub fn job_log_from_raw_v117(raw: &[u8], skipped_bytes: u64) -> RunLogTail {
    let (raw, skipped_bytes) = if skipped_bytes == 0 {
        (raw, 0)
    } else {
        let cut = raw
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(raw.len(), |newline| newline + 1);
        (&raw[cut..], skipped_bytes + cut as u64)
    };
    let raw = String::from_utf8_lossy(raw);
    let mut out = String::with_capacity(raw.len() / 2);
    for line in raw.lines() {
        let text = strip_log_timestamp(line.trim_end_matches('\r'));
        if text.starts_with("##[endgroup]") {
            continue;
        }
        if let Some(group) = text.strip_prefix("##[group]") {
            out.push_str(&format!("── {group} ──"));
        } else {
            out.push_str(text);
        }
        out.push('\n');
    }
    RunLogTail {
        text: redact(&sanitize_terminal(&out), &[]),
        skipped_bytes,
    }
}

#[cfg(test)]
#[path = "live_job_log_v117_tests.rs"]
mod tests;
