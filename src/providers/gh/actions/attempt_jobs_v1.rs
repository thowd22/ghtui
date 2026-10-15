//! SOURCE ONLY, undeclared compatibility reader for the shared RunAttempt.
//! A missing optional job attempt is bound only by this actual exact-endpoint
//! read. This is not principal/repository access or publication authority.
use super::*;

const DEFINITION_NOTE: &str = "Matrix coordinates, reusable workflow call paths and dependency edges are not explicit in the jobs API; consult the exact-revision YAML. Expanded job names and runner labels are factual, not inferred dependencies.";

/// Private/non-Serde: only `read` constructs it, after the real attempt GET.
/// No caller may promote a DTO, URL or requested attempt into this evidence.
pub(super) struct ExactAttemptJobs {
    pub(super) run: WorkflowRun,
    pub(super) jobs_endpoint: String,
    pub(super) repository_id: Option<u64>,
    pub(super) repository_node: Option<String>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum JobAttemptOrigin {
    SuppliedAndMatched,
    ExactEndpointBound,
}
impl ExactAttemptJobs {
    pub(super) fn read(
        provider: &GithubProvider,
        repo: &str,
        id: u64,
        attempt: u64,
        cancel: &CancellationToken,
    ) -> GithubResult<Self> {
        // Preserve the existing target/SHA policy and argv. Tightening all
        // legacy read budgets/identity policies is a separate reviewed change.
        validate_pr(repo, id)?;
        if attempt == 0 {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Select a positive run attempt",
            ));
        }
        let endpoint = format!("repos/{repo}/actions/runs/{id}/attempts/{attempt}");
        let raw = provider.api(&endpoint, "GET", None, None, false, cancel)?;
        let run = run_from_json(&raw);
        if run.id != id || run.attempt != attempt || validate_sha(&run.head_sha).is_err() {
            return Err(error(
                GithubErrorKind::Json,
                "Attempt response does not match the selected run/attempt",
            ));
        }
        Ok(Self {
            run,
            jobs_endpoint: format!("{endpoint}/jobs?per_page=100"),
            repository_id: raw["repository"]["id"].as_u64().filter(|id| *id > 0),
            repository_node: raw["repository"]["node_id"]
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 256)
                .map(str::to_owned),
        })
    }
    pub(super) fn bind_job(&self, raw: &Value) -> GithubResult<(ActionsJob, JobAttemptOrigin)> {
        let origin = match raw.get("run_attempt") {
            None => JobAttemptOrigin::ExactEndpointBound,
            Some(value) => {
                let supplied = value
                    .as_u64()
                    .filter(|attempt| *attempt > 0)
                    .ok_or_else(|| {
                        error(
                            GithubErrorKind::Json,
                            "Supplied job attempt must be a positive integer; null is not omission",
                        )
                    })?;
                if supplied != self.run.attempt {
                    return Err(error(
                        GithubErrorKind::Json,
                        "Supplied job attempt differs from the observed exact-attempt endpoint",
                    ));
                }
                JobAttemptOrigin::SuppliedAndMatched
            }
        };
        let mut job = job_from_json(raw);
        if job.id == 0 || job.run_id != self.run.id || job.head_sha != self.run.head_sha {
            return Err(error(
                GithubErrorKind::Json,
                "Job response does not match the selected run/attempt/commit",
            ));
        }
        // This assigns typed contextual provenance, never mutates JSON or
        // supplies default1. All explicit values were checked before parsing.
        job.attempt = self.run.attempt;
        Ok((job, origin))
    }
}

/// One shared two-child legacy protocol. Exact `api_list --paginate --slurp`
/// behavior/capture limits are unchanged; no new auth, permissions or paging
/// completeness assertion is introduced by accepting the optional field.
pub(super) fn read_run_attempt_v1(
    provider: &GithubProvider,
    repo: &str,
    id: u64,
    attempt: u64,
    cancel: &CancellationToken,
) -> GithubResult<RunAttempt> {
    let observed = ExactAttemptJobs::read(provider, repo, id, attempt, cancel)?;
    let raw_jobs = provider.api_list(&observed.jobs_endpoint, Some("jobs"), cancel)?;
    let mut jobs = Vec::with_capacity(raw_jobs.len());
    let mut supplied = 0usize;
    let mut bound = 0usize;
    for raw in &raw_jobs {
        let (job, origin) = observed.bind_job(raw)?;
        match origin {
            JobAttemptOrigin::SuppliedAndMatched => supplied += 1,
            JobAttemptOrigin::ExactEndpointBound => bound += 1,
        }
        jobs.push(job);
    }
    let provenance = if jobs.is_empty() {
        "Attempt provenance: the exact-attempt response contained no observed jobs; no JSON attempt or job allocation is inferred.".to_owned()
    } else {
        format!("Attempt source: verified exact endpoint; {bound} job(s) omitted run_attempt, {supplied} job(s) supplied run_attempt and matched (run {} / attempt {}). No default attempt or URL-derived attempt is used.", observed.run.id, observed.run.attempt)
    };
    Ok(RunAttempt {
        run: observed.run,
        jobs,
        // Put provenance first: the existing one-line row clips long text. The
        // limitations remain intact instead of hiding the origin at its tail.
        definition_note: format!("{provenance} {DEFINITION_NOTE}"),
    })
}
