//! Undeclared child of shared selected-read implementation. No pinned relaxation.
use super::*;

pub struct PrCheckAttemptAdvanceV2 {
    original: Arc<PrCheckRunObservation>,
    latest: WorkflowRun,
    latest_node: String,
    latest_suite: u64,
    identity: HostIdentity,
    requests: usize,
    charged_bytes: usize,
}
impl fmt::Debug for PrCheckAttemptAdvanceV2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(
            "PrCheckAttemptAdvanceV2(<closed latest observation; original remains historical>)",
        )
    }
}
impl PrCheckAttemptAdvanceV2 {
    pub fn original(&self) -> &Arc<PrCheckRunObservation> {
        &self.original
    }
    pub fn latest(&self) -> &WorkflowRun {
        &self.latest
    }
    pub fn latest_node(&self) -> &str {
        &self.latest_node
    }
    pub fn latest_suite(&self) -> u64 {
        self.latest_suite
    }
    pub fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub fn requests(&self) -> usize {
        self.requests
    }
    pub fn charged_bytes(&self) -> usize {
        self.charged_bytes
    }
}
pub enum PrCheckRefreshReadV2 {
    Current(Box<PrCheckRunObservation>),
    AttemptAdvanced(Box<PrCheckAttemptAdvanceV2>),
}
pub(super) enum SelectionPolicyV2 {
    Pinned,
    RefreshExactCurrent,
}
fn latest_observed(
    provider: &GithubProvider,
    original: &PrCheckOriginal,
    repo: &MetricsRepository,
    wanted: &ObservedRun,
    raw: &Value,
) -> GithubResult<(ObservedRun, u64)> {
    let suite = positive_bounded(raw, "check_suite_id")?;
    let proof = CheckProof {
        node: String::new(),
        suite,
    };
    let observed = observed_run(provider, original, repo, &proof, raw)?;
    if observed.raw_node != wanted.raw_node
        || observed.run.id != wanted.run.id
        || observed.run.workflow_id != wanted.run.workflow_id
        || observed.run.event != wanted.run.event
        || observed.run.head_sha != wanted.run.head_sha
        || observed.run.attempt <= wanted.run.attempt
    {
        return Err(error(
            GithubErrorKind::StaleHead,
            "No genuine monotone exact-run advance; pinned attempt unchanged",
        ));
    }
    Ok((observed, suite))
}
#[allow(clippy::too_many_arguments)]
pub(super) fn close_advance_v2(
    provider: &GithubProvider,
    binding: &ScopedPrCheckBindingV1<'_>,
    original: &PrCheckOriginal,
    repo: &MetricsRepository,
    original_check: &CheckProof,
    original_parent: u64,
    wanted: &ObservedRun,
    opening: &Value,
    origin: &Arc<PrCheckRunObservation>,
    budget: &mut MetricsBudget,
    cancel: &CancellationToken,
    guard: &mut impl FnMut(PrCheckSelectedReadPhaseV1) -> GithubResult<()>,
) -> GithubResult<PrCheckAttemptAdvanceV2> {
    let (latest, suite) = latest_observed(provider, original, repo, wanted, opening)?;
    let base = format!("repos/{}", original.repository);
    let (closing, closing_suite) = latest_observed(
        provider,
        original,
        repo,
        wanted,
        &budget.read_document(
            provider,
            &format!("{base}/actions/runs/{}", wanted.run.id),
            cancel,
        )?,
    )?;
    same_run(&latest, &closing)?;
    if suite != closing_suite {
        return Err(error(
            GithubErrorKind::StaleHead,
            "Latest attempt suite changed during advance read",
        ));
    }
    let raw = budget.read_document(
        provider,
        &format!("{base}/check-runs/{}", original.check_id),
        cancel,
    )?;
    let check = check_proof(provider, original, &raw)?;
    binding.check(&raw)?;
    if &check != original_check {
        return Err(error(
            GithubErrorKind::StaleHead,
            "Original historical check changed during latest observation",
        ));
    }
    let raw = budget.read_document(
        provider,
        &format!("{base}/pulls/{}", original.number),
        cancel,
    )?;
    let parent = current_parent(provider, original, repo, &raw)?;
    binding.parent(provider, &raw)?;
    if parent != original_parent {
        return Err(error(
            GithubErrorKind::IdentityChanged,
            "Actual original PR changed during advance read",
        ));
    }
    let local = budget.pr_check_guard(cancel, || {
        guard(PrCheckSelectedReadPhaseV1::BeforeFinalProof)
    });
    let raw = budget.read_document(provider, &base, cancel)?;
    let final_repo = current_repo(provider, original, &raw)?;
    binding.repository(&raw)?;
    require_same_repo(repo, &final_repo)?;
    let who = principal(provider, budget, &origin.identity, cancel)?;
    local.map_err(|e| {
        error(
            e.kind,
            "Original advance visibility guard unavailable; private diagnostics hidden",
        )
    })?;
    budget.observed_through()?;
    Ok(PrCheckAttemptAdvanceV2 {
        original: origin.clone(),
        latest: closing.run,
        latest_node: closing.raw_node,
        latest_suite: suite,
        identity: who,
        requests: budget.requests,
        charged_bytes: budget
            .bytes
            .checked_add(budget.successful_diagnostic_bytes)
            .and_then(|n| n.checked_add(budget.failed_reserved_bytes))
            .ok_or_else(|| error(GithubErrorKind::Transport, "Advance accounting overflow"))?,
    })
}
