//! Each later private log read gets fresh current original-source admission.
use super::*;

#[derive(Clone, Debug)]
pub struct PrCheckLogReadLimits {
    pub max_document_bytes: usize,
    pub max_log_bytes: usize,
    pub max_lines: usize,
    pub max_response_bytes: usize,
    pub max_duration: Duration,
}
impl Default for PrCheckLogReadLimits {
    fn default() -> Self {
        Self {
            max_document_bytes: 128 * 1024,
            max_log_bytes: 512 * 1024,
            max_lines: 5000,
            max_response_bytes: 2 * 1024 * 1024,
            max_duration: Duration::from_secs(30),
        }
    }
}
impl PrCheckLogReadLimits {
    pub(super) fn selected(&self) -> GithubResult<MetricsReadLimits> {
        if !(1..=128 * 1024).contains(&self.max_document_bytes)
            || !(1..=512 * 1024).contains(&self.max_log_bytes)
            || !(1..=10000).contains(&self.max_lines)
            || !(1..=2 * 1024 * 1024).contains(&self.max_response_bytes)
            || self.max_duration.is_zero()
            || self.max_duration > Duration::from_secs(30)
        {
            return Err(error(GithubErrorKind::InvalidTarget,"Check log reads need positive canonical/log/raw/prepared-line caps and one deadline"));
        }
        Ok(MetricsReadLimits {
            max_pages: 1,
            page_size: 1,
            max_rows: 1,
            max_document_bytes: self.max_document_bytes,
            max_response_bytes: self.max_response_bytes,
            max_duration: self.max_duration,
        })
    }
}
pub struct PrCheckLogsObservation {
    pub(super) captured_target: PrCheckCapturedTargetV2,
    origin: Arc<PrCheckRunObservation>,
    identity: HostIdentity,
    logs: String,
    limits: PrCheckLogReadLimits,
    requests: usize,
    stdout_bytes: usize,
    diagnostic_bytes: usize,
    failed_reserved_bytes: usize,
    observed_from: u64,
    observed_through: u64,
}
impl fmt::Debug for PrCheckLogsObservation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PrCheckLogsObservation(<guarded private logs; read only>)")
    }
}
impl PrCheckLogsObservation {
    pub(crate) fn workflow_owned_bytes_v9(&self) -> Option<usize> {
        use crate::providers::gh::workflow_capacity_v9 as b;
        b::sum([
            b::owner::<Self>(),
            self.origin.workflow_owned_bytes_v9()?,
            b::identity(&self.identity)?,
            self.logs.capacity(),
            self.captured_target.workflow_owned_bytes_v9()?,
        ])
    }
    pub fn origin(&self) -> &Arc<PrCheckRunObservation> {
        &self.origin
    }
    pub fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub fn logs(&self) -> &str {
        &self.logs
    }
    pub fn limits(&self) -> &PrCheckLogReadLimits {
        &self.limits
    }
    pub fn requests(&self) -> usize {
        self.requests
    }
    pub fn successful_stdout_bytes(&self) -> usize {
        self.stdout_bytes
    }
    pub fn successful_diagnostic_bytes(&self) -> usize {
        self.diagnostic_bytes
    }
    pub fn failed_reserved_bytes(&self) -> usize {
        self.failed_reserved_bytes
    }
    pub fn observed_from(&self) -> u64 {
        self.observed_from
    }
    pub fn observed_through(&self) -> u64 {
        self.observed_through
    }
}
fn current_link_run(
    provider: &GithubProvider,
    observed: &PrCheckRunObservation,
    raw: &Value,
) -> GithubResult<()> {
    let link = observed.link.as_ref().ok_or_else(|| {
        error(
            GithubErrorKind::InvalidTarget,
            "Original check/run/job correlation is unavailable",
        )
    })?;
    let proof = CheckProof {
        node: observed.check_node.clone(),
        suite: observed.suite_id,
    };
    let current = observed_run(
        provider,
        &observed.original,
        &observed.repository,
        &proof,
        raw,
    )?;
    let prior = ObservedRun {
        raw_node: link.run_node.clone(),
        run: link.attempt.run.clone(),
    };
    same_run(&prior, &current)
}
struct ClosedLogRead {
    identity: GithubResult<HostIdentity>,
    result: GithubResult<String>,
}
fn hidden_failure(failure: GithubError) -> GithubError {
    let message = format!(
        "PR check log context unavailable • prior private frames hidden ({:?})",
        failure.kind
    );
    GithubError {
        kind: failure.kind,
        message,
    }
}
fn phase(
    provider: &GithubProvider,
    origin: &PrCheckRunObservation,
    limits: &PrCheckLogReadLimits,
    budget: &mut MetricsBudget,
    cancel: &CancellationToken,
    guard: &mut impl FnMut(PrCheckLogReadPhase) -> GithubResult<()>,
    scoped: Option<&ScopedPrCheckBindingV1<'_>>,
) -> GithubResult<ClosedLogRead> {
    let link = origin.link.as_ref().ok_or_else(|| {
        error(
            GithubErrorKind::InvalidTarget,
            "Select a positively correlated check/run/actual attempt/job first",
        )
    })?;
    let who = principal(provider, budget, &origin.identity, cancel)?;
    let repo_path = format!("repos/{}", origin.original.repository);
    let parent_path = format!("{repo_path}/pulls/{}", origin.original.number);
    let run_path = format!("{repo_path}/actions/runs/{}", link.attempt.run.id);
    let raw = budget.read_document(provider, &repo_path, cancel)?;
    let repo = current_repo(provider, &origin.original, &raw)?;
    if let Some(binding) = scoped {
        binding.repository(&raw)?;
    }
    require_same_repo(&origin.repository, &repo)?;
    let raw = budget.read_document(provider, &parent_path, cancel)?;
    let parent = current_parent(provider, &origin.original, &repo, &raw)?;
    if let Some(binding) = scoped {
        binding.parent(provider, &raw)?;
    }
    if parent != origin.pr_database_id {
        return Err(error(
            GithubErrorKind::IdentityChanged,
            "Original check PR database identity changed before logs",
        ));
    }
    current_link_run(
        provider,
        origin,
        &budget.read_document(provider, &run_path, cancel)?,
    )?;
    budget.pr_check_guard(cancel, || guard(PrCheckLogReadPhase::BeforeCapture))?;
    let args = vec![
        "run".into(),
        "view".into(),
        link.attempt.run.id.to_string(),
        "--repo".into(),
        format!("{}/{}", provider.host, origin.original.repository),
        "--attempt".into(),
        link.attempt.run.attempt.to_string(),
        "--log".into(),
        "--job".into(),
        link.selected_job.to_string(),
    ];
    let result = budget.pr_check_log_capture(provider, args, limits.max_log_bytes, cancel);
    // Normal transport errors still close all four guards. Exhausted/cancelled
    // capture prevents these later requests through the same original budget.
    current_link_run(
        provider,
        origin,
        &budget.read_document(provider, &run_path, cancel)?,
    )?;
    let raw = budget.read_document(provider, &parent_path, cancel)?;
    let parent = current_parent(provider, &origin.original, &repo, &raw)?;
    if let Some(binding) = scoped {
        binding.parent(provider, &raw)?;
    }
    if parent != origin.pr_database_id {
        return Err(error(
            GithubErrorKind::IdentityChanged,
            "Original check PR database identity changed after logs",
        ));
    }
    let local = budget.pr_check_guard(cancel, || guard(PrCheckLogReadPhase::BeforeFinalProof));
    let raw = budget.read_document(provider, &repo_path, cancel)?;
    let final_repo = current_repo(provider, &origin.original, &raw)?;
    if let Some(binding) = scoped {
        binding.repository(&raw)?;
    }
    require_same_repo(&origin.repository, &final_repo)?;
    // The closing principal has now been attempted. Its failure, of any kind,
    // must hide captured data, not send an extra fallback authentication read.
    let identity = principal(provider, budget, &who, cancel);
    Ok(ClosedLogRead {
        identity,
        result: local.and(result),
    })
}
impl GithubProvider {
    /// Bridge-only guard user/repo/PR/run -> selected log -> run/PR/repo/user.
    /// Nine children on success; initial source proof is NOT current authority.
    pub fn pr_check_logs(
        &self,
        origin: Arc<PrCheckRunObservation>,
        limits: &PrCheckLogReadLimits,
        cancel: &CancellationToken,
    ) -> GithubResult<PrCheckLogsObservation> {
        self.pr_check_logs_with_guard(origin, limits, cancel, |_| Ok(()))
    }
    pub fn pr_check_logs_with_guard(
        &self,
        origin: Arc<PrCheckRunObservation>,
        limits: &PrCheckLogReadLimits,
        cancel: &CancellationToken,
        guard: impl FnMut(PrCheckLogReadPhase) -> GithubResult<()>,
    ) -> GithubResult<PrCheckLogsObservation> {
        self.pr_check_logs_impl_v1(origin, limits, cancel, guard, None)
    }
    pub(super) fn pr_check_logs_impl_v1(
        &self,
        origin: Arc<PrCheckRunObservation>,
        limits: &PrCheckLogReadLimits,
        cancel: &CancellationToken,
        mut guard: impl FnMut(PrCheckLogReadPhase) -> GithubResult<()>,
        scoped: Option<&ScopedPrCheckBindingV1<'_>>,
    ) -> GithubResult<PrCheckLogsObservation> {
        let selected = limits.selected()?;
        if self.host != origin.identity.host || !self.runner.same_instance(&origin.producer) {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Original check came from a different producer/host; log admission unavailable",
            ));
        }
        let mut budget = MetricsBudget::new(&selected)?;
        let closed = match phase(
            self,
            &origin,
            limits,
            &mut budget,
            cancel,
            &mut guard,
            scoped,
        ) {
            Ok(value) => value,
            Err(failure)
                if matches!(
                    failure.kind,
                    GithubErrorKind::IdentityChanged | GithubErrorKind::Authentication
                ) =>
            {
                return Err(hidden_failure(failure))
            }
            Err(failure) => {
                // Early target/transport failure does not make a private error
                // safe under an old principal. No retries or budget reset.
                principal(self, &mut budget, &origin.identity, cancel).map_err(|error_| {
                    if matches!(
                        error_.kind,
                        GithubErrorKind::IdentityChanged | GithubErrorKind::Authentication
                    ) {
                        hidden_failure(error_)
                    } else {
                        error(
                            GithubErrorKind::Transport,
                            "Check log scope/budget unavailable; prior private frames hidden",
                        )
                    }
                })?;
                return Err(hidden_failure(failure));
            }
        };
        let identity = closed.identity.map_err(hidden_failure)?;
        let logs = closed.result.map_err(hidden_failure)?;
        if logs.lines().count() > limits.max_lines {
            return Err(error(
                GithubErrorKind::Unsupported,
                "Captured logs exceed prepared-line budget; no complete empty log/view inferred",
            ));
        }
        Ok(PrCheckLogsObservation {
            captured_target: PrCheckCapturedTargetV2::matched(&origin)?,
            origin,
            identity,
            logs,
            limits: limits.clone(),
            requests: budget.requests,
            stdout_bytes: budget.bytes,
            diagnostic_bytes: budget.successful_diagnostic_bytes,
            failed_reserved_bytes: budget.failed_reserved_bytes,
            observed_from: budget.observed_from,
            observed_through: budget.observed_through()?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrCheckLogReadPhase {
    BeforeCapture,
    BeforeFinalProof,
}

/// Only the private existing bridge factory can forward a fully closed other-job capture.
pub(super) fn observation_from_closed_other_job_v1(
    origin: Arc<PrCheckRunObservation>,
    identity: HostIdentity,
    logs: String,
    limits: &PrCheckLogReadLimits,
    budget: &MetricsBudget,
    captured_target: PrCheckCapturedTargetV2,
) -> GithubResult<PrCheckLogsObservation> {
    Ok(PrCheckLogsObservation {
        captured_target,
        origin,
        identity,
        logs,
        limits: limits.clone(),
        requests: budget.requests,
        stdout_bytes: budget.bytes,
        diagnostic_bytes: budget.successful_diagnostic_bytes,
        failed_reserved_bytes: budget.failed_reserved_bytes,
        observed_from: budget.observed_from,
        observed_through: budget.observed_through()?,
    })
}
