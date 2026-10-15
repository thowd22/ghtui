//! Native Actions transport. Accepted asynchronous writes are not advertised as completed.
use super::pr::url_component;
use super::*;
mod attempt_jobs_v1;
mod scoped_loaded_logs_v2;
mod scoped_read_v1;
mod scoped_resources_v2;
mod scoped_run_catalogue_v2;
mod settings;
pub use scoped_loaded_logs_v2::ScopedActionsLogSnapshotV2;
pub use scoped_read_v1::*;
pub(crate) use scoped_resources_v2::artifact_model_bytes_v2;
pub use scoped_resources_v2::*;
pub use scoped_run_catalogue_v2::{
    ScopedRunQueryV2, ScopedRunWindowV2, SCOPED_RUN_MAX_PAGE_V2, SCOPED_RUN_PAGE_SIZE_V2,
};
pub use settings::*;
mod workflows;
pub(super) use workflows::workflow_from_json_v2;
pub use workflows::*;
mod archive_allocation_guard_v2;
mod artifacts;
pub use artifacts::*;
mod policies;
pub use policies::*;
mod operations;
pub use operations::*;
mod reconcile;
pub use reconcile::*;
mod approvals;
pub use approvals::*;
mod resource_keys;
pub use resource_keys::canonical_actions_resource_keys;
pub(crate) use resource_keys::{canonical_actions_host, canonical_github_resource};

/// Shared exact endpoint and canonical model parser for legacy slurped reads
/// and scoped single-page reads. No second run grammar or default identity.
fn run_catalog_endpoint_v2(repo: &str, filter: &RunFilter) -> GithubResult<String> {
    validate_repo(repo)?;
    if let Some(sha) = &filter.head_sha {
        validate_sha(sha)?;
    }
    if filter.workflow_id == Some(0) {
        return Err(error(
            GithubErrorKind::InvalidTarget,
            "Select a valid workflow ID",
        ));
    }
    let mut endpoint = match filter.workflow_id {
        Some(id) => format!("repos/{repo}/actions/workflows/{id}/runs?per_page=100"),
        None => format!("repos/{repo}/actions/runs?per_page=100"),
    };
    let params = [
        ("actor", &filter.actor),
        ("branch", &filter.branch),
        ("event", &filter.event),
        ("status", &filter.status),
        ("head_sha", &filter.head_sha),
        ("created", &filter.created),
    ];
    for (key, value) in params {
        if let Some(value) = value {
            nonempty_parameter(value)?;
            endpoint.push_str(&format!("&{key}={}", url_component(value)));
        }
    }
    Ok(endpoint)
}
fn parse_run_catalogue_v2(pages: &[Value], filter: &RunFilter) -> GithubResult<RunCatalog> {
    let filtered = [
        &filter.actor,
        &filter.branch,
        &filter.event,
        &filter.status,
        &filter.head_sha,
        &filter.created,
    ]
    .iter()
    .any(|field| field.is_some());
    let total = pages
        .first()
        .and_then(|page| page["total_count"].as_u64())
        .ok_or_else(|| {
            error(
                GithubErrorKind::Json,
                "Run catalog omitted its total_count coverage; no complete inventory is inferred",
            )
        })?;
    let mut runs = Vec::new();
    let mut ids = BTreeSet::new();
    for page in pages {
        let values = page["workflow_runs"].as_array().ok_or_else(|| {
            error(
                GithubErrorKind::Json,
                "Run catalog page has no workflow_runs",
            )
        })?;
        for value in values {
            let run = run_from_json(value);
            if run.id == 0
                || run.workflow_id == 0
                || run.attempt == 0
                || validate_sha(&run.head_sha).is_err()
            {
                return Err(error(
                    GithubErrorKind::Json,
                    "Run catalog contains an incomplete run identity",
                ));
            }
            if filter.workflow_id.is_some_and(|id| id != run.workflow_id)
                || filter
                    .branch
                    .as_ref()
                    .is_some_and(|branch| branch != &run.head_branch)
                || filter
                    .event
                    .as_ref()
                    .is_some_and(|event| event != &run.event)
                || filter
                    .actor
                    .as_ref()
                    .is_some_and(|actor| !actor.eq_ignore_ascii_case(&run.actor.login))
            {
                return Err(error(GithubErrorKind::StaleHead,"Run catalog returned a different workflow/ref/event/actor than the requested scope"));
            }
            if let Some(sha) = &filter.head_sha {
                if run.head_sha != *sha {
                    return Err(error(
                        GithubErrorKind::StaleHead,
                        "Run catalog returned a different commit than the requested SHA",
                    ));
                }
            }
            if ids.insert(run.id) {
                runs.push(run);
            }
        }
    }
    let mut reasons = Vec::new();
    if filtered && total > 1000 {
        reasons.push("GitHub limits filtered workflow-run searches to 1,000 results; narrow the date/ref filter".into());
    }
    if (runs.len() as u64) < total {
        reasons.push(format!("Loaded {} of {total} matching runs", runs.len()));
    }
    Ok(RunCatalog {
        coverage: Coverage {
            loaded: runs.len(),
            total_matches: total,
            complete: reasons.is_empty(),
            reasons,
        },
        runs,
    })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobStep {
    pub number: u64,
    pub name: String,
    pub status: String,
    pub conclusion: String,
    pub started_at: String,
    pub completed_at: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActionsJob {
    pub id: u64,
    pub run_id: u64,
    pub attempt: u64,
    pub head_sha: String,
    pub name: String,
    pub status: String,
    pub conclusion: String,
    pub started_at: String,
    pub completed_at: String,
    pub runner_id: u64,
    pub runner_name: String,
    pub runner_group_name: String,
    pub labels: Vec<String>,
    pub check_run_url: String,
    pub steps: Vec<JobStep>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunAttempt {
    pub run: WorkflowRun,
    pub jobs: Vec<ActionsJob>,
    pub definition_note: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckAnnotation {
    pub path: String,
    pub start_line: u64,
    pub end_line: u64,
    pub start_column: Option<u64>,
    pub end_column: Option<u64>,
    pub level: String,
    pub title: String,
    pub message: String,
    pub raw_details: String,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum RunMutation {
    RerunAll { debug: bool },
    RerunFailed { debug: bool },
    RerunJob { job_id: u64, debug: bool },
    Cancel,
    ForceCancel,
    DeleteRun,
    DeleteLogs,
    ApproveFork,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunMutationOutcome {
    pub action: RunMutation,
    pub run: Option<WorkflowRun>,
    pub accepted: bool,
    pub completed: bool,
    pub explanation: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RunFilter {
    pub workflow_id: Option<u64>,
    pub actor: Option<String>,
    pub branch: Option<String>,
    pub event: Option<String>,
    pub status: Option<String>,
    pub head_sha: Option<String>,
    pub created: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunCatalog {
    pub runs: Vec<WorkflowRun>,
    pub coverage: Coverage,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActionCache {
    pub id: u64,
    pub key: String,
    pub ref_name: String,
    pub version: String,
    pub size_in_bytes: u64,
    pub created_at: String,
    pub last_accessed_at: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CacheUsage {
    pub active_caches_count: u64,
    pub active_caches_size_in_bytes: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingDeployment {
    pub environment_id: u64,
    pub environment_name: String,
    pub wait_timer_minutes: u64,
    pub wait_timer_started_at: String,
    pub current_user_can_approve: bool,
    pub reviewers: Vec<Value>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DeploymentDecision {
    Approved,
    Rejected,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeploymentReview {
    pub environment_ids: Vec<u64>,
    pub decision: DeploymentDecision,
    pub comment: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeploymentReviewOutcome {
    pub decision: DeploymentDecision,
    pub environment_ids: Vec<u64>,
    pub deployments: Vec<Value>,
    pub remaining: Vec<PendingDeployment>,
    pub completed: bool,
}

pub(super) fn job_from_json(v: &Value) -> ActionsJob {
    ActionsJob {
        id: number(v, "id"),
        run_id: number(v, "run_id"),
        attempt: number(v, "run_attempt"),
        head_sha: text(v, "head_sha"),
        name: text(v, "name"),
        status: text(v, "status"),
        conclusion: text(v, "conclusion"),
        started_at: text(v, "started_at"),
        completed_at: text(v, "completed_at"),
        runner_id: number(v, "runner_id"),
        runner_name: text(v, "runner_name"),
        runner_group_name: text(v, "runner_group_name"),
        labels: v["labels"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        check_run_url: text(v, "check_run_url"),
        steps: v["steps"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|s| JobStep {
                number: number(s, "number"),
                name: text(s, "name"),
                status: text(s, "status"),
                conclusion: text(s, "conclusion"),
                started_at: text(s, "started_at"),
                completed_at: text(s, "completed_at"),
            })
            .collect(),
    }
}
fn cache_from_json(v: &Value) -> ActionCache {
    ActionCache {
        id: number(v, "id"),
        key: text(v, "key"),
        ref_name: text(v, "ref"),
        version: text(v, "version"),
        size_in_bytes: number(v, "size_in_bytes"),
        created_at: text(v, "created_at"),
        last_accessed_at: text(v, "last_accessed_at"),
    }
}
fn nonempty_parameter(value: &str) -> GithubResult<()> {
    if value.is_empty() || value.contains('\0') || value.chars().any(char::is_control) {
        Err(error(
            GithubErrorKind::InvalidTarget,
            "An explicit nonempty target is required",
        ))
    } else {
        Ok(())
    }
}
impl GithubProvider {
    fn actions_resource(&self, repo: &str, run: u64) -> String {
        self.canonical_resource(&format!("{}/{repo}/run/{run}", self.host))
    }
    fn actions_ambiguous(&self, resource: &str, message: &str) -> GithubError {
        self.runner
            .mark_ambiguous(&self.canonical_resource(resource));
        error(GithubErrorKind::AmbiguousWrite, message)
    }
    fn actions_identity(
        &self,
        expected: &HostIdentity,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        let current = self.identity(cancel)?;
        if !current.same_account(expected) {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "GitHub account changed; refresh the selected Actions scope before changing it",
            ));
        }
        Ok(())
    }
    pub fn run(
        &self,
        repo: &str,
        id: u64,
        cancel: &CancellationToken,
    ) -> GithubResult<WorkflowRun> {
        validate_pr(repo, id)?;
        let v = self.api(
            &format!("repos/{repo}/actions/runs/{id}"),
            "GET",
            None,
            None,
            false,
            cancel,
        )?;
        let run = run_from_json(&v);
        if run.id != id || run.attempt == 0 || validate_sha(&run.head_sha).is_err() {
            return Err(error(GithubErrorKind::Unsupported,"The host returned an incomplete or mismatched run identity; no action can be safely targeted"));
        }
        Ok(run)
    }
    fn guard_run(
        &self,
        identity: &HostIdentity,
        repo: &str,
        target: &WorkflowRun,
        cancel: &CancellationToken,
    ) -> GithubResult<WorkflowRun> {
        self.actions_identity(identity, cancel)?;
        let fresh = self.run(repo, target.id, cancel)?;
        if fresh.workflow_id != target.workflow_id
            || fresh.attempt != target.attempt
            || fresh.head_sha != target.head_sha
            || fresh.status != target.status
            || fresh.conclusion != target.conclusion
        {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Run state or attempt changed; refresh the exact run before confirming this action",
            ));
        }
        Ok(fresh)
    }
    pub fn run_catalog(
        &self,
        repo: &str,
        filter: &RunFilter,
        cancel: &CancellationToken,
    ) -> GithubResult<RunCatalog> {
        let endpoint = run_catalog_endpoint_v2(repo, filter)?;
        let pages = self.api(&endpoint, "GET", None, None, true, cancel)?;
        let pages = pages
            .as_array()
            .ok_or_else(|| error(GithubErrorKind::Json, "Run pagination returned no pages"))?;
        parse_run_catalogue_v2(pages, filter)
    }
    pub fn run_attempt(
        &self,
        repo: &str,
        id: u64,
        attempt: u64,
        cancel: &CancellationToken,
    ) -> GithubResult<RunAttempt> {
        attempt_jobs_v1::read_run_attempt_v1(self, repo, id, attempt, cancel)
    }
    pub fn job_annotations(
        &self,
        repo: &str,
        job: &ActionsJob,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<CheckAnnotation>> {
        validate_repo(repo)?;
        let prefix = format!(
            "https://{}/repos/{repo}/check-runs/",
            if self.host == "github.com" {
                "api.github.com".to_owned()
            } else {
                format!("{}/api/v3", self.host)
            }
        );
        let id = job
            .check_run_url
            .strip_prefix(&prefix)
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|n| *n > 0)
            .ok_or_else(|| {
                error(
                    GithubErrorKind::Unsupported,
                    "Job does not provide a check-run URL on the selected repository/host",
                )
            })?;
        Ok(self
            .api_list(
                &format!("repos/{repo}/check-runs/{id}/annotations?per_page=100"),
                None,
                cancel,
            )?
            .iter()
            .map(|v| CheckAnnotation {
                path: text(v, "path"),
                start_line: number(v, "start_line"),
                end_line: number(v, "end_line"),
                start_column: v["start_column"].as_u64(),
                end_column: v["end_column"].as_u64(),
                level: text(v, "annotation_level"),
                title: text(v, "title"),
                message: text(v, "message"),
                raw_details: text(v, "raw_details"),
            })
            .collect())
    }
    pub fn run_logs_at(
        &self,
        repo: &str,
        target: &WorkflowRun,
        job: Option<&ActionsJob>,
        cancel: &CancellationToken,
    ) -> GithubResult<String> {
        validate_pr(repo, target.id)?;
        if target.attempt == 0 {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Run logs need an exact attempt",
            ));
        }
        let mut args = vec![
            "run".into(),
            "view".into(),
            target.id.to_string(),
            "--repo".into(),
            format!("{}/{repo}", self.host),
            "--attempt".into(),
            target.attempt.to_string(),
            "--log".into(),
        ];
        if let Some(job) = job {
            if job.run_id != target.id
                || job.attempt != target.attempt
                || job.head_sha != target.head_sha
            {
                return Err(error(
                    GithubErrorKind::InvalidTarget,
                    "Job logs must belong to the selected run attempt and SHA",
                ));
            }
            args.extend(["--job".into(), job.id.to_string()]);
        }
        self.execute(args, None, None, cancel).map(|v| {
            super::super::runner::redact(
                &super::super::runner::sanitize_terminal(&String::from_utf8_lossy(&v)),
                &[],
            )
        })
    }
    pub fn mutate_run(
        &self,
        identity: &HostIdentity,
        repo: &str,
        target: &WorkflowRun,
        action: RunMutation,
        cancel: &CancellationToken,
    ) -> GithubResult<RunMutationOutcome> {
        let fresh = self.guard_run(identity, repo, target, cancel)?;
        let resource = self.actions_resource(repo, target.id);
        let base = format!("repos/{repo}/actions/runs/{}", target.id);
        let mut input = None;
        let (endpoint, method) = match action {
            RunMutation::RerunAll { debug } | RunMutation::RerunFailed { debug } => {
                if fresh.status != "completed" {
                    return Err(error(
                        GithubErrorKind::InvalidTarget,
                        "Only a completed run can be rerun",
                    ));
                }
                input = Some(json!({"enable_debug_logging":debug}));
                (
                    format!(
                        "{base}/{}",
                        if matches!(action, RunMutation::RerunFailed { .. }) {
                            "rerun-failed-jobs"
                        } else {
                            "rerun"
                        }
                    ),
                    "POST",
                )
            }
            RunMutation::RerunJob { job_id, debug } => {
                if fresh.status != "completed" || job_id == 0 {
                    return Err(error(
                        GithubErrorKind::InvalidTarget,
                        "Select a job in a completed run",
                    ));
                }
                let job = self.api(
                    &format!("repos/{repo}/actions/jobs/{job_id}"),
                    "GET",
                    None,
                    None,
                    false,
                    cancel,
                )?;
                if number(&job, "id") != job_id
                    || number(&job, "run_id") != fresh.id
                    || number(&job, "run_attempt") != fresh.attempt
                    || text(&job, "head_sha") != fresh.head_sha
                {
                    return Err(error(
                        GithubErrorKind::StaleHead,
                        "The job does not belong to the selected current run attempt",
                    ));
                }
                input = Some(json!({"enable_debug_logging":debug}));
                (format!("repos/{repo}/actions/jobs/{job_id}/rerun"), "POST")
            }
            RunMutation::Cancel | RunMutation::ForceCancel => {
                if fresh.status == "completed" {
                    return Err(error(
                        GithubErrorKind::InvalidTarget,
                        "The run already completed",
                    ));
                }
                (
                    format!(
                        "{base}/{}",
                        if matches!(action, RunMutation::ForceCancel) {
                            "force-cancel"
                        } else {
                            "cancel"
                        }
                    ),
                    "POST",
                )
            }
            RunMutation::DeleteRun => (base.clone(), "DELETE"),
            RunMutation::DeleteLogs => (format!("{base}/logs"), "DELETE"),
            RunMutation::ApproveFork => {
                if fresh.event != "pull_request"
                    || fresh.status != "action_required" && fresh.conclusion != "action_required"
                {
                    return Err(error(
                        GithubErrorKind::InvalidTarget,
                        "The selected fork run is not awaiting approval",
                    ));
                }
                (format!("{base}/approve"), "POST")
            }
        };
        self.api(
            &endpoint,
            method,
            input,
            Some(resource.clone()),
            false,
            cancel,
        )?;
        if matches!(action, RunMutation::DeleteRun) {
            let runs = self.after_write(&resource, self.runs(repo, None, cancel))?;
            if runs.iter().any(|r| r.id == target.id) {
                return Err(self.actions_ambiguous(
                    &resource,
                    "Run is still visible after deletion; reconcile without repeating DELETE",
                ));
            }
            return Ok(RunMutationOutcome {
                action,
                run: None,
                accepted: true,
                completed: true,
                explanation: "The selected run is absent from the refreshed catalog".into(),
            });
        }
        let observed = self.after_write(&resource, self.run(repo, target.id, cancel))?;
        let rerun = matches!(
            action,
            RunMutation::RerunAll { .. }
                | RunMutation::RerunFailed { .. }
                | RunMutation::RerunJob { .. }
        );
        if observed.head_sha != target.head_sha
            || observed.workflow_id != target.workflow_id
            || observed.attempt < target.attempt
            || !rerun && observed.attempt != target.attempt
        {
            return Err(self.actions_ambiguous(
                &resource,
                "Post-write run identity no longer matches the selected workflow/commit/original attempt; reconcile",
            ));
        }
        if !matches!(
            observed.status.as_str(),
            "queued"
                | "requested"
                | "pending"
                | "waiting"
                | "in_progress"
                | "completed"
                | "action_required"
        ) || observed.status == "completed" && observed.conclusion.is_empty()
        {
            return Err(self.actions_ambiguous(
                &resource,
                "Post-write run state is absent or unrecognized; no completed transition is inferred",
            ));
        }
        let completed = match action {
            RunMutation::RerunAll { .. }
            | RunMutation::RerunFailed { .. }
            | RunMutation::RerunJob { .. } => observed.attempt > target.attempt,
            RunMutation::Cancel | RunMutation::ForceCancel => {
                observed.status == "completed" && observed.conclusion == "cancelled"
            }
            RunMutation::ApproveFork => {
                observed.status != "action_required" && observed.conclusion != "action_required"
            }
            _ => false,
        };
        Ok(RunMutationOutcome {
            action,
            run: Some(observed),
            accepted: true,
            completed,
            explanation: if matches!(action, RunMutation::DeleteLogs) {
                "Log deletion accepted; the API does not expose a log-deleted field. A later log request may report unavailable; do not retry deletion automatically.".into()
            } else if completed {
                "The requested state transition was observed".into()
            } else {
                "GitHub accepted the request; the expected state transition has not yet been observed. Refresh or watch this exact run; do not issue the mutation again.".into()
            },
        })
    }
    pub fn action_caches(
        &self,
        repo: &str,
        key: Option<&str>,
        ref_name: Option<&str>,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<ActionCache>> {
        validate_repo(repo)?;
        let mut endpoint = format!("repos/{repo}/actions/caches?per_page=100");
        for (name, value) in [("key", key), ("ref", ref_name)] {
            if let Some(value) = value {
                nonempty_parameter(value)?;
                endpoint.push_str(&format!("&{name}={}", url_component(value)));
            }
        }
        Ok(self
            .api_list(&endpoint, Some("actions_caches"), cancel)?
            .iter()
            .map(cache_from_json)
            .collect())
    }
    pub fn cache_usage(&self, repo: &str, cancel: &CancellationToken) -> GithubResult<CacheUsage> {
        validate_repo(repo)?;
        let v = self.api(
            &format!("repos/{repo}/actions/cache/usage"),
            "GET",
            None,
            None,
            false,
            cancel,
        )?;
        Ok(CacheUsage {
            active_caches_count: number(&v, "active_caches_count"),
            active_caches_size_in_bytes: number(&v, "active_caches_size_in_bytes"),
        })
    }
    pub fn delete_cache(
        &self,
        identity: &HostIdentity,
        repo: &str,
        target: &ActionCache,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<ActionCache>> {
        validate_pr(repo, target.id)?;
        self.actions_identity(identity, cancel)?;
        let caches = self.action_caches(repo, Some(&target.key), Some(&target.ref_name), cancel)?;
        if caches.iter().find(|c| c.id == target.id) != Some(target) {
            return Err(error(GithubErrorKind::StaleHead,"Selected cache changed or disappeared; refresh its exact ID/key/ref before deletion"));
        }
        let resource = format!("{}/{repo}/cache/{}", self.host, target.id);
        self.api(
            &format!("repos/{repo}/actions/caches/{}", target.id),
            "DELETE",
            None,
            Some(resource.clone()),
            false,
            cancel,
        )?;
        let remaining =
            self.after_write(&resource, self.action_caches(repo, None, None, cancel))?;
        if remaining.iter().any(|c| c.id == target.id) {
            return Err(self.actions_ambiguous(
                &resource,
                "Cache is still visible after deletion; reconcile without repeating the write",
            ));
        }
        Ok(remaining)
    }
    pub fn pending_deployments(
        &self,
        repo: &str,
        run: u64,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<PendingDeployment>> {
        validate_pr(repo, run)?;
        let v = self.api(
            &format!("repos/{repo}/actions/runs/{run}/pending_deployments"),
            "GET",
            None,
            None,
            false,
            cancel,
        )?;
        let mut ids = BTreeSet::new();
        v.as_array()
            .ok_or_else(|| {
                error(
                    GithubErrorKind::Json,
                    "Pending deployments response is not a list",
                )
            })?
            .iter()
            .map(|v| {
                let id = number(&v["environment"], "id");
                if id == 0 || !ids.insert(id) {
                    return Err(error(
                        GithubErrorKind::Json,
                        "Pending environment has a missing or duplicate identity",
                    ));
                }
                Ok(PendingDeployment {
                    environment_id: id,
                    environment_name: text(&v["environment"], "name"),
                    wait_timer_minutes: number(v, "wait_timer"),
                    wait_timer_started_at: text(v, "wait_timer_started_at"),
                    current_user_can_approve: boolean(v, "current_user_can_approve"),
                    reviewers: v["reviewers"].as_array().cloned().unwrap_or_default(),
                })
            })
            .collect()
    }
    pub fn review_deployments(
        &self,
        identity: &HostIdentity,
        repo: &str,
        target: &WorkflowRun,
        review: &DeploymentReview,
        cancel: &CancellationToken,
    ) -> GithubResult<DeploymentReviewOutcome> {
        let environment_ids = &review.environment_ids;
        let decision = review.decision;
        let comment = &review.comment;
        if environment_ids.is_empty()
            || environment_ids.contains(&0)
            || environment_ids.iter().collect::<BTreeSet<_>>().len() != environment_ids.len()
            || comment.trim().is_empty()
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Choose distinct pending environments and enter a review comment",
            ));
        }
        self.guard_run(identity, repo, target, cancel)?;
        let gates = self.pending_deployments(repo, target.id, cancel)?;
        for id in environment_ids {
            let gate = gates
                .iter()
                .find(|g| g.environment_id == *id)
                .ok_or_else(|| {
                    error(
                        GithubErrorKind::StaleHead,
                        "A selected deployment gate is no longer pending",
                    )
                })?;
            if !gate.current_user_can_approve {
                return Err(error(GithubErrorKind::Permission,"GitHub reports that the current actor cannot review a selected environment (reviewer/self-approval restrictions apply)"));
            }
        }
        let resource = self.actions_resource(repo, target.id);
        let v = self.api(
            &format!(
                "repos/{repo}/actions/runs/{}/pending_deployments",
                target.id
            ),
            "POST",
            Some(json!({"environment_ids":environment_ids,"state":decision,"comment":comment})),
            Some(resource.clone()),
            false,
            cancel,
        )?;
        let deployments = v.as_array().cloned().ok_or_else(|| {
            self.actions_ambiguous(
                &resource,
                "Deployment review returned no identifiable results; reconcile pending gates",
            )
        })?;
        let remaining =
            self.after_write(&resource, self.pending_deployments(repo, target.id, cancel))?;
        let completed = !remaining
            .iter()
            .any(|g| environment_ids.contains(&g.environment_id));
        Ok(DeploymentReviewOutcome {
            decision,
            environment_ids: environment_ids.to_vec(),
            deployments,
            remaining,
            completed,
        })
    }
}
