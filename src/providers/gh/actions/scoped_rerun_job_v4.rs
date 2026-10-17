//! Bounded selected-job precondition INSIDE the same original Confirm body.
//! No source constructor, second job grammar, refreshed clock or write permit.
use super::{
    attempt_jobs_v1::ExactAttemptJobs,
    scoped_read_v1::{bind_scoped_actions_run_raw_v3, ScopedActionsRunRawPhaseV3},
    *,
};
use std::{sync::Arc, time::Instant};
#[cfg(test)]
#[path = "scoped_rerun_job_v4_tests.rs"]
mod tests;

fn phase(cancel: &CancellationToken, deadline: Instant) -> GithubResult<()> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        return Err(error(
            GithubErrorKind::Transport,
            "Original selected-job confirmation cancelled/expired; no renewed read group",
        ));
    }
    Ok(())
}
fn refused() -> GithubError {
    error(GithubErrorKind::StaleHead,
        "Actual original selected job/attempt membership changed or is unavailable within bounded coverage")
}
pub(super) fn verify(
    measured: &GithubProvider,
    repo: &str,
    run: &Arc<ScopedActionsRunTargetV1>,
    selected: &Arc<ScopedActionsJobTargetV1>,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<()> {
    phase(cancel, deadline)?;
    // Fresh snapshots have a new Arc: the captured member's parent must retain
    // the SAME actual producer/full pin, not pointer-equal a new HTTP result.
    if !run.original_writer_v3(measured)
        || !selected.run().original_writer_v3(measured)
        || !selected.run().same_watch_pin_v2(run)
        || selected.id() == 0
    {
        return Err(refused());
    }
    let direct = measured.api(
        &format!("repos/{repo}/actions/jobs/{}", selected.id()),
        "GET",
        None,
        None,
        false,
        cancel,
    )?;
    phase(cancel, deadline)?;
    // Explicit invalid/null is NOT genuine omission. Never default to one.
    if direct
        .get("run_attempt")
        .is_some_and(|value| value.as_u64().filter(|n| *n > 0) != Some(run.run().attempt))
    {
        return Err(refused());
    }
    let endpoint = ExactAttemptJobs::read_with_raw_guard_v4(
        measured,
        repo,
        run.run().id,
        run.run().attempt,
        cancel,
        |raw| {
            bind_scoped_actions_run_raw_v3(
                raw,
                repo,
                run,
                ScopedActionsRunRawPhaseV3::UnchangedPrewrite,
            )
            .map(|_| ())
        },
    )?;
    phase(cancel, deadline)?;
    let (actual, _) = endpoint.bind_job(&direct)?;
    if actual.id != selected.id()
        || selected
            .node_v2()
            .is_some_and(|node| direct["node_id"].as_str() != Some(node))
        || actual.check_run_url != selected.check_url_v4()
    {
        return Err(refused());
    }
    drop(direct);
    let mut seen = BTreeSet::new();
    let mut total = None;
    for page in 1..=2 {
        phase(cancel, deadline)?;
        let raw = measured.api(
            &format!("{}&page={page}", endpoint.jobs_endpoint),
            "GET",
            None,
            None,
            false,
            cancel,
        )?;
        phase(cancel, deadline)?;
        let members = raw
            .get("jobs")
            .and_then(Value::as_array)
            .ok_or_else(refused)?;
        let count = raw
            .get("total_count")
            .and_then(Value::as_u64)
            .filter(|n| *n <= i64::MAX as u64)
            .ok_or_else(refused)?;
        if members.len() > 100
            || count < members.len() as u64
            || total.is_some_and(|old| old != count)
        {
            return Err(refused());
        }
        total = Some(count);
        let mut found = false;
        for member in members {
            phase(cancel, deadline)?;
            let (job, _) = endpoint.bind_job(member)?;
            if !seen.insert(job.id) {
                return Err(refused());
            }
            if job.id == selected.id() {
                if selected
                    .node_v2()
                    .is_some_and(|node| member["node_id"].as_str() != Some(node))
                    || job.check_run_url != actual.check_run_url
                    || job.head_sha != actual.head_sha
                    || job.run_id != actual.run_id
                    || job.attempt != actual.attempt
                {
                    return Err(refused());
                }
                found = true;
            }
        }
        if seen.len() as u64 > count {
            return Err(refused());
        }
        // Positive exact membership is sufficient; do not pretend the bounded
        // page is a complete catalogue or require all popular-repo history.
        if found {
            phase(cancel, deadline)?;
            return Ok(());
        }
        if members.len() < 100 {
            break;
        }
    }
    phase(cancel, deadline)?;
    Err(refused()) // not positively observed, never complete-absence evidence
}
