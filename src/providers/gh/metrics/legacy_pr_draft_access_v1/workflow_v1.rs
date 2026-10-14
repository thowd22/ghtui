//! SOURCE ONLY child of legacy access. Activation forwards standalone reads
//! here too: one source parser/five-read policy, with a shared mutable ledger.
use super::*;
use crate::providers::gh::metrics::legacy_pr_workflow_v1::{
    LegacyPrDraftWorkflowBudget, LegacyPrWorkflowAccounting,
};

pub(super) struct DraftReadArguments<'a> {
    pub identity: &'a HostIdentity,
    pub original: Original,
    pub limits: &'a LegacyPrDraftAccessLimits,
    pub window: &'a LegacyPrDraftReadWindow,
    pub prior: Option<&'a LegacyPrDraftAccess>,
    pub cancel: &'a CancellationToken,
}
impl GithubProvider {
    pub fn legacy_pr_draft_access_budgeted_v1(
        &self,
        identity: &HostIdentity,
        original: &PrSummary,
        workflow: &mut LegacyPrDraftWorkflowBudget,
        cancel: &CancellationToken,
    ) -> GithubResult<LegacyPrDraftAccess> {
        let original = Original::new(self, identity, original)?;
        let scope =
            crate::storage::Scope::github(&identity.host, identity.account_id).map_err(|_| {
                error(
                    GithubErrorKind::InvalidTarget,
                    "Whole original numeric scope missing",
                )
            })?;
        let limits = workflow.limits();
        workflow.enter(0, self, &scope, cancel)?;
        let result = {
            let (window, budget) = workflow.parts();
            self.read_legacy_using_budget_v1(
                DraftReadArguments {
                    identity,
                    original,
                    limits: &limits,
                    window,
                    prior: None,
                    cancel,
                },
                budget,
            )
        };
        workflow.finish(0, result.is_ok());
        result
    }
    pub fn revalidate_legacy_pr_draft_access_budgeted_v1(
        &self,
        prior: &LegacyPrDraftAccess,
        identity: &HostIdentity,
        workflow: &mut LegacyPrDraftWorkflowBudget,
        cancel: &CancellationToken,
    ) -> GithubResult<LegacyPrDraftAccess> {
        let scope =
            crate::storage::Scope::github(&identity.host, identity.account_id).map_err(|_| {
                error(
                    GithubErrorKind::InvalidTarget,
                    "Whole original numeric scope missing",
                )
            })?;
        let window = workflow.window();
        window.remaining(cancel)?;
        if !Arc::ptr_eq(&prior.window, &window.issuer)
            || self.host != prior.original.host
            || !self.runner.same_instance(&prior.producer)
            || !same_principal(&prior.identity, identity)
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Whole original closing cannot rebase provider/window/principal",
            ));
        }
        let limits = workflow.limits();
        workflow.enter(3, self, &scope, cancel)?;
        let result = {
            let (window, budget) = workflow.parts();
            self.read_legacy_using_budget_v1(
                DraftReadArguments {
                    identity,
                    original: prior.original.clone(),
                    limits: &limits,
                    window,
                    prior: Some(prior),
                    cancel,
                },
                budget,
            )
        };
        workflow.finish(3, result.is_ok());
        result
    }
    pub(super) fn read_legacy_using_budget_v1(
        &self,
        args: DraftReadArguments<'_>,
        budget: &mut MetricsBudget,
    ) -> GithubResult<LegacyPrDraftAccess> {
        let DraftReadArguments {
            identity,
            original,
            limits,
            window,
            prior,
            cancel,
        } = args;
        limits.selected(window, cancel)?;
        let before = LegacyPrWorkflowAccounting::snapshot(budget);
        let group_started = Instant::now();
        let mut read = |endpoint: &str| {
            let group_remaining = limits.max_duration.saturating_sub(group_started.elapsed());
            let remaining = window.remaining(cancel)?.min(group_remaining);
            if remaining.is_zero() {
                return Err(error(
                    GithubErrorKind::Transport,
                    "Original access group expired inside the shared workflow",
                ));
            }
            let provider = self.clone().with_timeout(self.timeout.min(remaining));
            let value = budget.read_document(&provider, endpoint, cancel)?;
            window.remaining(cancel)?;
            Ok::<_, GithubError>(value)
        };
        let first = user_observation(self, identity, &read("user")?)?;
        let endpoint = format!("repos/{}", original.repository);
        let repo = repo_observation(self, &original.repository, &read(&endpoint)?)?;
        if let Some(prior) = prior {
            require_same_repo(&prior.repository, &repo)?;
        }
        let (pr_database_id, head_repository) = parent_observation(
            self,
            &original,
            &repo,
            &read(&format!("{endpoint}/pulls/{}", original.number))?,
        )?;
        if prior.is_some_and(|prior| {
            prior.pr_database_id != pr_database_id || prior.head_repository != head_repository
        }) {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Observed original PR database/head-source identity changed; no draft retarget",
            ));
        }
        let closing = repo_observation(self, &original.repository, &read(&endpoint)?)?;
        require_same_repo(&repo, &closing)?;
        let identity = user_observation(self, &first, &read("user")?)?;
        window.remaining(cancel)?;
        let after = LegacyPrWorkflowAccounting::snapshot(budget);
        let delta = |left: usize, right: usize| {
            left.checked_sub(right).ok_or_else(|| {
                error(
                    GithubErrorKind::Transport,
                    "Shared workflow capture counters moved backwards",
                )
            })
        };
        let accounting = LegacyPrDraftAccessAccounting {
            requests: delta(after.requests(), before.requests())?,
            stdout: delta(
                after.successful_stdout_bytes(),
                before.successful_stdout_bytes(),
            )?,
            diagnostics: delta(
                after.successful_diagnostic_bytes(),
                before.successful_diagnostic_bytes(),
            )?,
            failed_reserved: delta(
                after.failed_reserved_bytes(),
                before.failed_reserved_bytes(),
            )?,
        };
        if accounting.requests != GROUP_REQUESTS
            || accounting.charged_bytes() > limits.max_response_bytes
        {
            return Err(error(
                GithubErrorKind::Transport,
                "Original canonical closing/capture group incomplete inside shared workflow",
            ));
        }
        Ok(LegacyPrDraftAccess {
            producer: self.runner.clone(),
            window: window.issuer.clone(),
            verified_at: Instant::now(),
            identity,
            original,
            repository: closing,
            pr_database_id,
            head_repository,
            limits: *limits,
            accounting,
        })
    }
}
