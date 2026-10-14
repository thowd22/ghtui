//! SOURCE ONLY, undeclared. Read-only precondition inside the existing writer.
//! No new publisher/permit, inferred attempt1, raw JSON mutation or URL proof.
use super::{attempt_jobs_v1::ExactAttemptJobs, *};
use std::time::Instant;

fn before(
    provider: &GithubProvider,
    deadline: Instant,
    cancel: &CancellationToken,
) -> GithubResult<GithubProvider> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() || cancel.is_cancelled() {
        return Err(error(
            GithubErrorKind::Transport,
            "Original selected-job membership guard deadline/cancellation reached; nothing sent",
        ));
    }
    Ok(provider
        .clone()
        .with_timeout(provider.timeout.min(remaining)))
}
fn same_guarded_run(expected: &WorkflowRun, actual: &WorkflowRun) -> GithubResult<()> {
    if actual.id != expected.id
        || actual.workflow_id != expected.workflow_id
        || actual.attempt != expected.attempt
        || actual.head_sha != expected.head_sha
        || actual.status != expected.status
        || actual.conclusion != expected.conclusion
    {
        return Err(error(
            GithubErrorKind::StaleHead,
            "Actual run changed while verifying selected-job attempt membership; nothing sent",
        ));
    }
    Ok(())
}

/// Called ONLY after the existing guard_run's actual principal/latest-run read
/// and completed-run/positive-job preconditions. Caller passes that fresh run,
/// never a user-constructed RunAttempt or opaque read-as-write permission.
pub(super) fn validate_selected_job_rerun_v1(
    provider: &GithubProvider,
    identity: &HostIdentity,
    repo: &str,
    fresh: &WorkflowRun,
    job_id: u64,
    cancel: &CancellationToken,
) -> GithubResult<()> {
    let raw = provider.api(
        &format!("repos/{repo}/actions/jobs/{job_id}"),
        "GET",
        None,
        None,
        false,
        cancel,
    )?;
    if job_id == 0
        || number(&raw, "id") != job_id
        || number(&raw, "run_id") != fresh.id
        || text(&raw, "head_sha") != fresh.head_sha
    {
        return Err(error(
            GithubErrorKind::StaleHead,
            "The selected job does not belong to the actual guarded run and commit",
        ));
    }
    if let Some(supplied) = raw.get("run_attempt") {
        if supplied.as_u64().filter(|value| *value > 0) != Some(fresh.attempt) {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Supplied job attempt is invalid or differs; omission fallback is not allowed",
            ));
        }
        // Preserve existing supplied-field happy argv/count/timeout path.
        return Ok(());
    }

    // ONLY the added read group is bounded here. Existing initial guard/direct
    // GET, later POST/readback/controller and their raw captures are not claimed
    // to share this clock. Each added child recomputes remaining original time.
    let deadline = Instant::now() + provider.timeout.min(Duration::from_secs(30));
    let observed = ExactAttemptJobs::read(
        &before(provider, deadline, cancel)?,
        repo,
        fresh.id,
        fresh.attempt,
        cancel,
    )?;
    same_guarded_run(fresh, observed.run())?;
    let raw_jobs = before(provider, deadline, cancel)?.api_list(
        observed.jobs_endpoint(),
        Some("jobs"),
        cancel,
    )?;
    let mut members = raw_jobs
        .iter()
        .filter(|job| job.get("id").and_then(Value::as_u64) == Some(job_id));
    let selected = members.next().ok_or_else(|| {
        error(
            GithubErrorKind::StaleHead,
            "Selected job not positively observed in actual exact-attempt membership; no absence or permission inferred",
        )
    })?;
    if members.next().is_some() {
        return Err(error(
            GithubErrorKind::StaleHead,
            "Duplicate selected job observations cannot authorize a rerun",
        ));
    }
    let selected = observed.bound_job(selected)?;
    if selected.id != job_id || selected.run_id != fresh.id || selected.attempt != fresh.attempt {
        return Err(error(
            GithubErrorKind::StaleHead,
            "Exact-attempt parser did not bind the original selected job",
        ));
    }
    // Additional read delay cannot retain the original identity/run observation
    // as current write authority. Same existing account policy, fresh children,
    // no new auth/version floor; server targeting is still not atomic.
    let current = provider.identity_before(deadline, cancel)?;
    if !current.same_account(identity) {
        return Err(error(
            GithubErrorKind::IdentityChanged,
            "Actions account changed while reading selected-job membership; nothing sent",
        ));
    }
    let closing = before(provider, deadline, cancel)?.run(repo, fresh.id, cancel)?;
    same_guarded_run(fresh, &closing)?;
    before(provider, deadline, cancel)?;
    Ok(())
}
