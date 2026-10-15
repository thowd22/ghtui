//! Worker-prepared source policy over the ONE existing Actions engine.
use super::*;
use crate::scoped_actions_v1::PREPARED_LIMIT_V1;
use std::mem::size_of;
pub(crate) const UNAVAILABLE: &str = "This catalogue-source Actions operation needs its own current-ID adapter; no legacy/name fallback, draft, permission or write was inferred";

pub(crate) fn prepare_policy(workspace: &mut ActionsWorkspace, pending: bool) {
    if workspace.run.is_some() {
        for (label, view) in [
            ("[t Run]", ActionsView::Run),
            ("[j Jobs]", ActionsView::Jobs),
        ] {
            if !workspace
                .controls
                .iter()
                .any(|control| control.action == ActionsAction::View(view))
            {
                workspace.controls.push(ActionsControl {
                    label: label.into(),
                    action: ActionsAction::View(view),
                    disabled_reason: None,
                });
            }
        }
    }
    for control in &mut workspace.controls {
        if matches!(control.action, ActionsAction::Back) {
            control.label = "[b Back]".into();
        }
        let allowed = matches!(
            control.action,
            ActionsAction::ScopedReadV2(_)
                | ActionsAction::Refresh
                | ActionsAction::Back
                | ActionsAction::View(ActionsView::Runs | ActionsView::Run | ActionsView::Jobs)
                | ActionsAction::OpenJob(_)
                | ActionsAction::Logs(Some(_))
                | ActionsAction::Annotations(_)
        );
        if !allowed {
            control.disabled_reason = Some(UNAVAILABLE.into());
        } else {
            control.disabled_reason = None;
        }
    }
    if !workspace
        .controls
        .iter()
        .any(|c| matches!(c.action, ActionsAction::Back))
    {
        workspace.controls.push(ActionsControl {
            label: "[b Back]".into(),
            action: ActionsAction::Back,
            disabled_reason: None,
        });
    }
    crate::actions_workspace::scoped_resources_v2::prepare_policy(workspace);
    use crate::scoped_actions_native_v1::read_ui_v2::ReadUiActionV2 as Ui;
    for (label, action, available) in [
        (
            "[f Edit filters]",
            Ui::OpenFilters,
            workspace.view == ActionsView::Runs,
        ),
        (
            "[/ Search loaded logs]",
            Ui::OpenSearch,
            workspace.view == ActionsView::Logs,
        ),
        (
            "[> Next match]",
            Ui::Next(true),
            workspace.view == ActionsView::Logs,
        ),
        (
            "[< Previous match]",
            Ui::Next(false),
            workspace.view == ActionsView::Logs,
        ),
        (
            "[x Clear matches]",
            Ui::Clear,
            workspace.view == ActionsView::Logs,
        ),
        (
            "[w Finite Watch / Stop]",
            Ui::Watch,
            workspace.run.is_some()
                && matches!(
                    workspace.view,
                    ActionsView::Run | ActionsView::Jobs | ActionsView::Logs
                ),
        ),
    ] {
        if available
            && !workspace.controls.iter().any(|control| {
                control.action
                    == ActionsAction::ScopedReadV2(
                        crate::scoped_actions_resources_v2::ScopedActionsReadActionV2::Ui(action),
                    )
            })
        {
            workspace.controls.push(ActionsControl {
                label: label.into(),
                action: ActionsAction::ScopedReadV2(
                    crate::scoped_actions_resources_v2::ScopedActionsReadActionV2::Ui(action),
                ),
                disabled_reason: None,
            });
        }
    }
    if pending {
        for control in &mut workspace.controls {
            if matches!(control.action, ActionsAction::Back) {
                control.label = "[b Cancel read]".into();
                control.disabled_reason = None;
            } else {
                control.disabled_reason =
                    Some("Original current read pending; Cancel or wait".into());
            }
        }
    }
    workspace.controls.sort_by_key(|control| {
        !matches!(
            control.action,
            ActionsAction::ScopedReadV2(_)
                | ActionsAction::Refresh
                | ActionsAction::Back
                | ActionsAction::View(ActionsView::Runs | ActionsView::Run | ActionsView::Jobs)
                | ActionsAction::Logs(Some(_))
                | ActionsAction::Annotations(_)
        )
    });
}
/// Actual observed page/query controls, prepared before original final proof.
pub(crate) fn prepare_catalogue_window_v2(
    workspace: &mut ActionsWorkspace,
    window: &Arc<ScopedRunWindowV2>,
) {
    use crate::scoped_actions_resources_v2::ScopedActionsReadActionV2 as Read;
    for (label, action, available) in [
        (
            "[( Previous runs]",
            Read::RunPage(-1),
            window.previous_query().is_some(),
        ),
        (
            "[) Next runs]",
            Read::RunPage(1),
            window.next_query().is_some(),
        ),
        ("[A All statuses]", Read::AllRunStatuses, true),
        ("[F Failed runs]", Read::FailedRuns, true),
        ("[I Active runs]", Read::ActiveRuns, true),
    ] {
        if available {
            workspace.controls.push(ActionsControl {
                label: label.into(),
                action: ActionsAction::ScopedReadV2(action),
                disabled_reason: None,
            });
        }
    }
    workspace.controls.sort_by_key(|control| {
        !matches!(
            control.action,
            ActionsAction::ScopedReadV2(_)
                | ActionsAction::Refresh
                | ActionsAction::Back
                | ActionsAction::View(ActionsView::Runs)
        )
    });
    workspace.warnings.push(format!(
        "Run page {} • status {} • {}/{} reported • partial is not absence",
        window.query().page(),
        window.query().filter().status.as_deref().unwrap_or("all"),
        window.loaded(),
        window.reported()
    ));
}
pub(crate) fn prepare_labels(workspace: &mut ActionsWorkspace) {
    if workspace.view == ActionsView::Annotations {
        if let Some(page) = &workspace.run {
            // Worker-only: retain all admitted message/detail lines, never
            // interpret API positions as native file/range coordinates.
            workspace.rows = Vec::new();
            for note in &page.annotations {
                let location = if note.start_line > 0 && note.end_line >= note.start_line {
                    format!("reported lines {}–{}", note.start_line, note.end_line)
                } else {
                    "reported lines unavailable".into()
                };
                workspace.rows.push(ActionsRow{label:format!("{} • {location} • {} • {} • API metadata, not canonical Git/review coordinates",if note.path.is_empty(){"Path unavailable"}else{&note.path},note.level,note.title),
                        role:if note.level=="failure"{SemanticToken::Failure}else{SemanticToken::Pending},actor:None,action:None});
                for line in note.message.split('\n') {
                    workspace
                        .rows
                        .push(ActionsRow::text(format!("Message: {line}")));
                }
                for line in note.raw_details.split('\n') {
                    workspace
                        .rows
                        .push(ActionsRow::text(format!("Details: {line}")));
                }
            }
        }
    }
}
/// Worker-only focus on the actual selected job member. Starting its visible
/// body at that row keeps its steps reachable even when all factual run-header
/// rows do not also fit a narrow terminal. This never derives a job from text.
pub(crate) fn focus_selected_job(workspace: &mut ActionsWorkspace) {
    if workspace.view != ActionsView::Jobs {
        return;
    }
    let Some(id) = workspace.run.as_ref().and_then(|page| page.selected_job) else {
        return;
    };
    if let Some(index) = workspace
        .rows
        .iter()
        .position(|row| row.action == Some(ActionsAction::OpenJob(id)))
    {
        workspace.selected = index;
        workspace.top = index;
    }
}
fn strings(values: &[&String]) -> usize {
    values
        .iter()
        .map(|v| v.capacity())
        .fold(0, usize::saturating_add)
}
fn actor(value: &Actor) -> usize {
    value.login.capacity()
        + value.actor_type.capacity()
        + value.display_name.as_ref().map_or(0, String::capacity)
        + value.avatar_url.as_ref().map_or(0, String::capacity)
}
pub(crate) fn run_owned_bytes(run: &WorkflowRun) -> usize {
    strings(&[
        &run.name,
        &run.title,
        &run.head_sha,
        &run.head_branch,
        &run.event,
        &run.status,
        &run.conclusion,
        &run.created_at,
        &run.updated_at,
        &run.url,
    ])
    .saturating_add(actor(&run.actor))
    .saturating_add(actor(&run.triggering_actor))
}
pub(crate) fn attempt_owned_bytes(value: &RunAttempt) -> usize {
    let mut bytes = size_of::<RunAttempt>()
        + value.definition_note.capacity()
        + run_owned_bytes(&value.run)
        + value.jobs.capacity() * size_of::<ActionsJob>();
    for job in &value.jobs {
        bytes = bytes.saturating_add(strings(&[
            &job.head_sha,
            &job.name,
            &job.status,
            &job.conclusion,
            &job.started_at,
            &job.completed_at,
            &job.runner_name,
            &job.runner_group_name,
            &job.check_run_url,
        ]));
        bytes = bytes.saturating_add(job.labels.capacity() * size_of::<String>());
        for label in &job.labels {
            bytes = bytes.saturating_add(label.capacity());
        }
        bytes = bytes.saturating_add(job.steps.capacity() * size_of::<JobStep>());
        for step in &job.steps {
            bytes = bytes.saturating_add(strings(&[
                &step.name,
                &step.status,
                &step.conclusion,
                &step.started_at,
                &step.completed_at,
            ]));
        }
    }
    bytes
}
pub(crate) fn preflight_projection(
    attempt: &RunAttempt,
    body: Option<&ScopedActionsReadBodyV1>,
) -> Result<(), String> {
    let steps = attempt
        .jobs
        .iter()
        .fold(0usize, |sum, job| sum.saturating_add(job.steps.len()));
    let mut bound = attempt_owned_bytes(attempt)
        .saturating_mul(4)
        .saturating_add(attempt.jobs.len().saturating_add(steps).saturating_mul(256))
        .saturating_add(4096);
    match body {
        Some(ScopedActionsReadBodyV1::Logs(text)) => {
            bound = bound
                .saturating_add(text.owned_bytes().saturating_mul(4))
                .saturating_add(text.text().lines().count().saturating_mul(192));
        }
        Some(ScopedActionsReadBodyV1::Annotations(rows)) => {
            bound =
                bound.saturating_add(rows.capacity().saturating_mul(size_of::<CheckAnnotation>()));
            let mut display_rows = 0usize;
            for row in rows {
                display_rows = display_rows.saturating_add(
                    1 + row.message.split('\n').count() + row.raw_details.split('\n').count(),
                );
                bound = bound.saturating_add(
                    strings(&[
                        &row.path,
                        &row.level,
                        &row.title,
                        &row.message,
                        &row.raw_details,
                    ])
                    .saturating_mul(4)
                    .saturating_add(512),
                );
            }
            if display_rows > 5000 {
                return Err("Annotation diagnostic display exceeds5000lines; no hidden truncation or complete claim".into());
            }
            bound = bound.saturating_add(display_rows.saturating_mul(256));
        }
        _ => {}
    }
    if bound > PREPARED_LIMIT_V1 {
        Err(
            "Actions owned model/projection preflight exceeds8MiB; no body clone/row allocation"
                .into(),
        )
    } else {
        Ok(())
    }
}
/// Newly owned readonly page allocations, not RSS/transitive borrowed source.
pub(crate) fn retained_bytes(w: &ActionsWorkspace) -> Result<usize, String> {
    if !matches!(
        w.view,
        ActionsView::Runs
            | ActionsView::Run
            | ActionsView::Jobs
            | ActionsView::Logs
            | ActionsView::Annotations
            | ActionsView::Artifacts
            | ActionsView::Artifact
            | ActionsView::Workflows
            | ActionsView::Workflow
            | ActionsView::Secrets
            | ActionsView::Variables
            | ActionsView::Policies
            | ActionsView::Environments
            | ActionsView::Runners
            | ActionsView::Caches
    ) {
        return Err("Unsupported scoped Actions prepared page".into());
    }
    let mut bytes = size_of::<ActionsWorkspace>();
    let mut add = |n: usize| {
        bytes = bytes.saturating_add(n);
    };
    add(w.identity.host.capacity()
        + w.identity.login.capacity()
        + w.identity.gh_version.capacity());
    add(w.repositories.capacity() * size_of::<String>());
    for repo in &w.repositories {
        add(repo.capacity());
    }
    if let ActionsScope::Repository { repository } = &w.scope {
        add(repository.capacity());
    }
    add(w.runs.capacity() * size_of::<ScopedRun>());
    for item in &w.runs {
        add(item.repository.capacity());
        add(run_owned_bytes(&item.run));
    }
    add(w.rows.capacity() * size_of::<ActionsRow>());
    for row in &w.rows {
        add(row.label.capacity());
        if let Some(a) = &row.actor {
            add(actor(a));
        }
        if let Some(ActionsAction::OpenRun { repository, .. }) = &row.action {
            add(repository.capacity());
        }
    }
    add(w.controls.capacity() * size_of::<ActionsControl>());
    for control in &w.controls {
        add(control.label.capacity());
        if let Some(reason) = &control.disabled_reason {
            add(reason.capacity());
        }
    }
    add(w.warnings.capacity() * size_of::<String>());
    for warning in &w.warnings {
        add(warning.capacity());
    }
    add(w.coverage.reasons.capacity() * size_of::<String>());
    for reason in &w.coverage.reasons {
        add(reason.capacity());
    }
    if let Some(page) = &w.run {
        add(page.repository.capacity() + page.attempt.definition_note.capacity());
        add(run_owned_bytes(&page.attempt.run));
        add(page.attempt.jobs.capacity() * size_of::<ActionsJob>());
        for job in &page.attempt.jobs {
            add(strings(&[
                &job.head_sha,
                &job.name,
                &job.status,
                &job.conclusion,
                &job.started_at,
                &job.completed_at,
                &job.runner_name,
                &job.runner_group_name,
                &job.check_run_url,
            ]));
            add(job.labels.capacity() * size_of::<String>());
            for label in &job.labels {
                add(label.capacity());
            }
            add(job.steps.capacity() * size_of::<JobStep>());
            for step in &job.steps {
                add(strings(&[
                    &step.name,
                    &step.status,
                    &step.conclusion,
                    &step.started_at,
                    &step.completed_at,
                ]));
            }
        }
        if let Some(logs) = &page.logs {
            add(logs.capacity());
        }
        add(page.annotations.capacity() * size_of::<CheckAnnotation>());
        for row in &page.annotations {
            add(strings(&[
                &row.path,
                &row.level,
                &row.title,
                &row.message,
                &row.raw_details,
            ]));
        }
    }
    bytes = bytes.saturating_add(super::scoped_resources_v2::workspace_bytes(w));
    if bytes > PREPARED_LIMIT_V1 {
        Err("Scoped Actions prepared content exceeds8MiB; no incomplete page admitted".into())
    } else {
        Ok(bytes)
    }
}
