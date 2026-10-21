//! TASK-107: Actions logs larger than the bounded capture show their LAST part
//! (memory holds at most the runner's output limit), and Export streams the
//! whole log from gh to disk without holding it in memory.
use super::*;
use crate::providers::runner::{redact, sanitize_terminal};
use std::io::{BufRead, Seek, Write};

/// Disk bound for a streamed full-log export; memory stays at one line plus a chunk.
pub const LOG_EXPORT_DISK_LIMIT_V107: usize = 2 * 1024 * 1024 * 1024;
const LOG_EXPORT_TIMEOUT_V107: Duration = Duration::from_secs(600);
const EXPORT_CHUNK: usize = 64 * 1024;

/// The readable tail of a run log and how many raw bytes came before it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunLogTail {
    pub text: String,
    /// Raw `gh run view --log` bytes not kept (0 = the whole log is in `text`).
    pub skipped_bytes: u64,
}

/// Turns a kept raw tail into readable text. When bytes were skipped, the first
/// kept line is partial, so it is dropped too and counted as skipped.
pub fn run_log_tail_from_raw(raw: &[u8], skipped_bytes: u64) -> RunLogTail {
    let (raw, skipped_bytes) = if skipped_bytes == 0 {
        (raw, 0)
    } else {
        let cut = raw
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(raw.len(), |newline| newline + 1);
        (&raw[cut..], skipped_bytes + cut as u64)
    };
    RunLogTail {
        text: redact(
            &sanitize_terminal(&readable_run_log(&String::from_utf8_lossy(raw))),
            &[],
        ),
        skipped_bytes,
    }
}

/// Byte count in the largest unit that keeps it readable, e.g. `16.0 MiB`.
pub fn log_size_label(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Streams raw `gh run view --log` output into readable, sanitized text, one line
/// at a time; returns the number of lines written.
pub fn stream_readable_run_log(
    raw: &mut (impl BufRead + Seek),
    out: &mut impl Write,
) -> std::io::Result<u64> {
    // First pass: is this more than one job? (Only that decides header style.)
    let mut first_job: Option<Vec<u8>> = None;
    let mut multi_job = false;
    let mut line = Vec::new();
    while !multi_job && raw.read_until(b'\n', &mut line)? > 0 {
        if let Some(tab) = line.iter().position(|byte| *byte == b'\t') {
            match &first_job {
                None => first_job = Some(line[..tab].to_vec()),
                Some(job) => multi_job = job[..] != line[..tab],
            }
        }
        line.clear();
    }
    raw.rewind()?;
    let mut readable = ReadableRunLog::new(multi_job);
    let mut chunk = String::with_capacity(EXPORT_CHUNK + 1024);
    let mut lines = 0u64;
    let flush = |chunk: &mut String, out: &mut dyn Write| -> std::io::Result<()> {
        out.write_all(redact(&sanitize_terminal(chunk), &[]).as_bytes())?;
        chunk.clear();
        Ok(())
    };
    while raw.read_until(b'\n', &mut line)? > 0 {
        let mut end = line.len();
        if line[..end].ends_with(b"\n") {
            end -= 1;
            if line[..end].ends_with(b"\r") {
                end -= 1;
            }
        }
        let before = chunk.len();
        readable.push(&String::from_utf8_lossy(&line[..end]), &mut chunk);
        lines += chunk[before..].matches('\n').count() as u64;
        line.clear();
        if chunk.len() >= EXPORT_CHUNK {
            flush(&mut chunk, &mut *out)?;
        }
    }
    flush(&mut chunk, &mut *out)?;
    Ok(lines)
}

impl GithubProvider {
    /// The run log with memory bounded by the runner's output limit: a log over
    /// the limit keeps only its last part, and says how much came before it.
    pub fn run_logs_tail_at(
        &self,
        repo: &str,
        target: &WorkflowRun,
        job: Option<&ActionsJob>,
        cancel: &CancellationToken,
    ) -> GithubResult<RunLogTail> {
        let args = self.run_log_args(repo, target, job)?;
        let (raw, skipped) = self.execute_tail(args, cancel)?;
        Ok(run_log_tail_from_raw(&raw, skipped))
    }
    /// Re-reads the WHOLE log from gh straight into an anonymous temporary file,
    /// then writes its readable form to a new `destination` (never overwriting).
    /// Returns the number of lines written.
    pub fn export_run_log_at(
        &self,
        repo: &str,
        target: &WorkflowRun,
        job: Option<&ActionsJob>,
        destination: &std::path::Path,
        cancel: &CancellationToken,
    ) -> GithubResult<u64> {
        let args = self.run_log_args(repo, target, job)?;
        let local = |error: std::io::Error| {
            error_v107(format!("Log export failed: {error}; nothing was written"))
        };
        let parent = destination
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .ok_or_else(|| error_v107("Choose a destination inside an existing directory"))?;
        let raw = tempfile::tempfile().map_err(local)?;
        self.execute_to_file(
            args,
            raw.try_clone().map_err(local)?,
            LOG_EXPORT_DISK_LIMIT_V107,
            LOG_EXPORT_TIMEOUT_V107,
            cancel,
        )?;
        let mut reader = std::io::BufReader::new(raw);
        reader.rewind().map_err(local)?;
        let mut staging = tempfile::NamedTempFile::new_in(parent).map_err(local)?;
        let lines = {
            let mut writer = std::io::BufWriter::new(staging.as_file_mut());
            let lines = stream_readable_run_log(&mut reader, &mut writer).map_err(local)?;
            writer.flush().map_err(local)?;
            lines
        };
        staging.as_file().sync_all().map_err(local)?;
        staging.persist_noclobber(destination).map_err(|_| {
            error_v107("Log export destination already exists or cannot be created; choose a new explicit path")
        })?;
        Ok(lines)
    }
}
fn error_v107(message: impl Into<String>) -> GithubError {
    GithubError {
        kind: GithubErrorKind::InvalidTarget,
        message: message.into(),
    }
}

#[cfg(test)]
#[path = "log_tail_v107_tests.rs"]
mod tests;
