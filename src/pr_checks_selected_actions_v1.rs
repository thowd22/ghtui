//! Source-only child of shared Actions preparation, not a second viewer.
use super::*;
use crate::providers::gh::PrCheckSelectionOriginV1;

impl PreparedPrCheckActions {
    pub(crate) fn new_selected_v1(
        observation: Arc<PrCheckRunObservation>,
    ) -> Result<Self, GithubError> {
        let selected = observation
            .selected_run_v1()
            .filter(|value| value.selection() != PrCheckSelectionOriginV1::ObservedOnly)
            .ok_or_else(|| GithubError {
                kind: GithubErrorKind::InvalidTarget,
                message: "Genuine explicit selected-run read required".into(),
            })?;
        let repo = observation.repository().full_name.clone();
        let mut workspace =
            ActionsWorkspace::new(observation.identity().clone(), vec![repo.clone()]);
        workspace.view = ActionsView::Run;
        workspace.filter.head_sha = Some(observation.head_sha().into());
        workspace.filter.event = Some(selected.attempt().run.event.clone());
        workspace.filter.workflow_id = Some(selected.attempt().run.workflow_id);
        workspace.runs = vec![ScopedRun {
            repository: repo.clone(),
            run: selected.attempt().run.clone(),
        }];
        workspace.coverage.loaded = 1;
        workspace.coverage.total_matches = 1;
        workspace.coverage.complete = false;
        workspace.coverage.reasons = vec![
            "Explicit positive run only; broader suite/workflow catalogue is not complete".into(),
        ];
        workspace.run = Some(RunPage {
            repository: repo,
            attempt: selected.attempt().as_ref().clone(),
            selected_job: selected.selected_job(),
            logs: None,
            annotations: vec![],
        });
        workspace.fetched_at = Some(observation.observed_through());
        workspace.warnings.push("Suite-associated run explicitly selected. No first-job/CheckRun mapping or mutation permission inferred; observed steps may be partial.".into());
        workspace
            .warnings
            .extend(observation.job_coverage().reasons.iter().cloned());
        workspace.prepare_rows();
        Ok(Self {
            observation,
            workspace,
        })
    }
}
