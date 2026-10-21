//! Worker preparation for the shared Actions viewer from an original PR check.
//! The sealed provider read is retained separately; this is not write authority.
use crate::{
    actions_workspace::{ActionsRow, ActionsView, ActionsWorkspace, RunPage, ScopedRun},
    providers::gh::{
        Check, GithubError, GithubErrorKind, HostIdentity, PrCheckJobAttemptOrigin,
        PrCheckRunObservation, PrSummary,
    },
    theme::SemanticToken,
};
use std::sync::Arc;
#[path = "pr_checks_selected_actions_v1.rs"]
mod selected_v1;

#[derive(Clone)]
pub struct PreparedPrCheckActions {
    observation: Arc<PrCheckRunObservation>,
    workspace: ActionsWorkspace,
}
impl std::fmt::Debug for PreparedPrCheckActions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PreparedPrCheckActions(<bounded verified PR-to-Actions read>)")
    }
}
impl PreparedPrCheckActions {
    /// Worker-only: bounded rows are copied once into the existing viewer;
    /// rendering does no provider calls, SQLite, source parsing or proof work.
    pub fn new(observation: Arc<PrCheckRunObservation>) -> Result<Self, GithubError> {
        let link = observation.link().ok_or_else(|| GithubError {
            kind: GithubErrorKind::Unsupported,
            message: observation
                .unavailable()
                .map_or(
                    "Exact check/run/attempt/job correlation unavailable",
                    |reason| reason.label(),
                )
                .into(),
        })?;
        let repo = observation.repository().full_name.clone();
        let mut workspace =
            ActionsWorkspace::new(observation.identity().clone(), vec![repo.clone()]);
        workspace.view = ActionsView::Run;
        workspace.filter.head_sha = Some(observation.head_sha().into());
        workspace.filter.event = Some(link.attempt().run.event.clone());
        workspace.filter.workflow_id = Some(link.attempt().run.workflow_id);
        workspace.runs = vec![ScopedRun {
            repository: repo.clone(),
            run: link.attempt().run.clone(),
        }];
        workspace.coverage.loaded = 1;
        workspace.coverage.total_matches = observation.run_coverage().total_matches.unwrap_or(1);
        workspace.coverage.complete = false;
        workspace.coverage.reasons = vec!["Selected check-suite observation only; broader workflow/head catalogue is not enumerated".into()];
        workspace.run = Some(RunPage {
            repository: repo,
            attempt: link.attempt().as_ref().clone(),
            selected_job: Some(link.selected_job()),
            logs: None,
            logs_skipped_bytes: 0,
            annotations: Vec::new(),
        });
        workspace.fetched_at = Some(observation.observed_through());
        workspace.warnings.push(format!("Opened from PR #{} • CheckRun {} • suite {} • observed event {} • exact attempt {} • job {}",
            observation.pr_number(), observation.check_id(), observation.suite_id(), link.attempt().run.event,
            link.attempt().run.attempt, link.selected_job()));
        if let Some(source) = link.job_attempt_origin(link.selected_job()) {
            workspace.warnings.push(source.label().into());
        }
        for coverage in [observation.run_coverage(), observation.job_coverage()] {
            workspace.warnings.extend(coverage.reasons.iter().cloned());
        }
        workspace.warnings.push("Read-only correlation is not rerun/cancel permission. Shared Actions publication must prepare and confirm the fresh exact target; opening or going Back never releases a pending write.".into());
        workspace.prepare_rows();
        workspace.drop_failure_jumps();
        let provenance = match link.job_attempt_origin(link.selected_job()) {
            Some(PrCheckJobAttemptOrigin::JsonObserved) => "Attempt source: supplied job JSON",
            Some(PrCheckJobAttemptOrigin::ExactEndpointBound) => {
                "Attempt source: verified exact endpoint (JSON omitted it)"
            }
            None => "Attempt source: unavailable",
        };
        workspace.rows.insert(
            0,
            ActionsRow {
                cells: Default::default(),
                label: format!(
                    "PR #{} • CheckRun {} • suite {}",
                    observation.pr_number(),
                    observation.check_id(),
                    observation.suite_id()
                ),
                role: SemanticToken::Normal,
                actor: None,
                action: None,
            },
        );
        workspace.rows.insert(
            1,
            ActionsRow {
                cells: Default::default(),
                label: provenance.into(),
                role: SemanticToken::Muted,
                actor: None,
                action: None,
            },
        );
        Ok(Self {
            observation,
            workspace,
        })
    }
    pub fn observation(&self) -> &Arc<PrCheckRunObservation> {
        &self.observation
    }
    pub fn workspace(&self) -> &ActionsWorkspace {
        &self.workspace
    }
    /// Root retains the original PR Checks route/selection separately and
    /// checks this before publication or Back. No unverified branch fallback.
    pub fn matches_original(
        &self,
        identity: &HostIdentity,
        original: &PrSummary,
        check: &Check,
    ) -> bool {
        self.observation.matches_original(identity, original, check)
    }
    pub fn into_workspace(self) -> ActionsWorkspace {
        self.workspace
    }
}
