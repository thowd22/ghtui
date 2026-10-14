//! Local navigation of genuinely loaded, structured failed steps. Reported
//! numbers are display data only: never a selective-log transport locator.
use super::*;

fn page(app: &App) -> Option<(&ActionsWorkspace, &crate::providers::gh::RunAttempt)> {
    if app.view != crate::app::View::Actions
        || app.overlay.is_some()
        || app.loading[crate::app::View::Actions.index()]
        || app.actions_action.is_some()
        || app.actions_load_requested
        || app.pr_check_continuation_request_v2.is_some()
    {
        return None;
    }
    let route = app.pr_checks_route.as_ref()?;
    if !route.scoped_v1() || !owns_origin(app, &route.origin) {
        return None;
    }
    let workspace = app.actions.as_ref()?;
    let actual = workspace.run.as_ref()?;
    let expected = route
        .observation
        .selected_run_v1()
        .map(|r| r.attempt())
        .or_else(|| route.observation.link().map(|r| r.attempt()))?;
    if workspace.view != ActionsView::Jobs
        || actual.repository != route.observation.repository().full_name
        || workspace.identity.host != route.observation.identity().host
        || workspace.identity.account_id != route.observation.identity().account_id
        || workspace.identity.account_id.is_none_or(|id| id == 0)
        || actual.attempt.run.id != expected.run.id
        || actual.attempt.run.attempt != expected.run.attempt
        || actual.attempt.run.head_sha != expected.run.head_sha
        || actual.attempt.jobs.len() != expected.jobs.len()
    {
        return None;
    }
    for (known, shown) in expected.jobs.iter().zip(&actual.attempt.jobs) {
        if known.id != shown.id
            || known.run_id != shown.run_id
            || known.attempt != shown.attempt
            || known.head_sha != shown.head_sha
            || known.steps.len() != shown.steps.len()
            || known.steps.iter().zip(&shown.steps).any(|(a, b)| {
                a.number != b.number
                    || a.name != b.name
                    || a.status != b.status
                    || a.conclusion != b.conclusion
            })
        {
            return None;
        }
    }
    Some((workspace, expected))
}
#[derive(Clone, Copy)]
struct FailedStep {
    row: usize,
    number: u64,
    job: u64,
    attempt: u64,
}
fn target(app: &App, next: bool) -> Option<FailedStep> {
    let (workspace, attempt) = page(app)?;
    let selected = workspace.run.as_ref().and_then(|p| p.selected_job);
    let mut row = 5usize; // SAME prepared run identity/actor/provenance header.
    let mut first = None;
    let mut last = None;
    let mut directional = None;
    for job in &attempt.jobs {
        row += 1;
        if selected == Some(job.id) {
            for step in &job.steps {
                if step.number > 0
                    && step.status == "completed"
                    && step.conclusion == "failure"
                    && workspace
                        .rows
                        .get(row)
                        .is_some_and(|r| r.action == Some(ActionsAction::Logs(Some(job.id))))
                {
                    let value = FailedStep {
                        row,
                        number: step.number,
                        job: job.id,
                        attempt: attempt.run.attempt,
                    };
                    first.get_or_insert(value);
                    last = Some(value);
                    if next && row > workspace.selected && directional.is_none()
                        || !next && row < workspace.selected
                    {
                        directional = Some(value);
                    }
                }
                row += 1;
            }
        }
    }
    directional.or(if next { first } else { last })
}
pub(super) fn available(app: &App) -> bool {
    target(app, true).is_some()
}
pub(super) fn navigate(app: &mut App, next: bool) {
    let target = target(app, next);
    let Some(workspace) = app.actions.as_mut() else {
        return;
    };
    let Some(FailedStep {
        row,
        number,
        job,
        attempt,
    }) = target
    else {
        app.dispatch(Action::Explain("No positively observed completed/failure step in the current original job. Missing/unknown status or number is not failure; no lookup or selective-log request was made.".into()));
        return;
    };
    workspace.selected = row;
    workspace.top = row.saturating_sub(app.actions_visible_rows / 2);
    app.actions_content_focused = true;
    app.status = format!("Observed failed step {number} • job {job} • original attempt {attempt} • Enter opens whole pinned job log, not selective step logs");
}
