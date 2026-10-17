//! Workflow variants on the SAME original Actions Work/owner/result/queue.
use super::*;
#[cfg(test)]
#[path = "scoped_actions_workflow_v6_tests.rs"]
mod tests;
use crate::{
    scoped_actions_workflow_publication_v6::{
        self as workflow, ScopedWorkflowIntentV6, WorkflowConfirmV6, WorkflowPreviewV6,
    },
    scoped_actions_workflow_ui_v6::{self as workflow_ui, WorkflowPhaseV6, WorkflowRequestV6},
};
pub(crate) enum WorkflowWorkV6 {
    Open(Arc<ScopedWorkflowIntentV6>),
    Form {
        request: Arc<WorkflowRequestV6>,
        confirm: Option<Arc<WorkflowConfirmV6>>,
    },
}
impl WorkflowWorkV6 {
    pub(crate) fn intent(&self) -> &Arc<ScopedWorkflowIntentV6> {
        match self {
            Self::Open(intent) => intent,
            Self::Form { request, .. } => request.opened().intent(),
        }
    }
    pub(crate) fn request(&self) -> Option<&Arc<WorkflowRequestV6>> {
        match self {
            Self::Form { request, .. } => Some(request),
            _ => None,
        }
    }
    pub(crate) fn confirm(&self) -> Option<&Arc<WorkflowConfirmV6>> {
        match self {
            Self::Form { confirm, .. } => confirm.as_ref(),
            _ => None,
        }
    }
    pub(crate) fn matches_form(&self, form: &Form) -> bool {
        self.request()
            .is_some_and(|request| request.matches_form(form))
    }
    pub(crate) fn overlay_current(&self, app: &App) -> bool {
        match self {
            Self::Open(_) => app.overlay.is_none(),
            Self::Form { request, .. } => {
                app.overlay == Some(crate::app::Overlay::Form)
                    && !app.form_dismissed
                    && app
                        .form
                        .as_ref()
                        .is_some_and(|form| request.matches_form(form))
            }
        }
    }
}
pub struct PreparedWorkflowV6 {
    work: Arc<ScopedActionsWorkV1>,
    publication: Arc<WorkflowWorkV6>,
    outcome: WorkflowOutcomeV6,
}
enum WorkflowOutcomeV6 {
    Open(Box<Form>),
    Preview(Arc<WorkflowPreviewV6>),
    Attempt(Box<crate::actions_controller::WorkflowExecutionV6>),
    History(Arc<workflow::WorkflowHistoryV6>),
}
impl PreparedWorkflowV6 {
    #[cfg(test)]
    pub(crate) fn attempt_for_test_v6(
        &self,
    ) -> Option<(
        &ActionsExecution,
        Option<&Arc<crate::actions_write_journal::ActionsOwnedWorkflowHistoryV6>>,
    )> {
        match &self.outcome {
            WorkflowOutcomeV6::Attempt(result) => Some(result.result_for_test_v6()),
            _ => None,
        }
    }
    pub(crate) fn matches(&self, work: &Arc<ScopedActionsWorkV1>) -> bool {
        Arc::ptr_eq(&self.work,work)&&work.kind==ScopedActionsRequestKindV1::Publication&&work.journal_v5.is_none()
            &&work.publication_v3.as_deref().is_some_and(|p|matches!(p,PublicationWorkV3::Workflow(current)if Arc::ptr_eq(current,&self.publication)))
            &&Arc::ptr_eq(&work.source,self.publication.intent().source())&&Arc::ptr_eq(&work.page,self.publication.intent().page())
            &&match(&*self.publication,&self.outcome){
                (WorkflowWorkV6::Open(intent),WorkflowOutcomeV6::Open(form))=>matches!(&form.purpose,FormPurpose::ScopedActionsWorkflowV6(state)if Arc::ptr_eq(state.opened().intent(),intent)),
                (WorkflowWorkV6::Form{request,confirm:None},WorkflowOutcomeV6::Preview(preview))=>request.phase()==WorkflowPhaseV6::Preview&&Arc::ptr_eq(request.opened(),preview.opened()),
                (WorkflowWorkV6::Form{request,confirm:Some(confirm)},WorkflowOutcomeV6::Attempt(result))=>result.matches_history_v6()&&request.phase()==WorkflowPhaseV6::Confirm&&request.preview().is_some_and(|p|Arc::ptr_eq(p,confirm.preview())),
                (WorkflowWorkV6::Form{request,confirm:None},WorkflowOutcomeV6::History(history))=>matches!(request.phase(),WorkflowPhaseV6::Inspect|WorkflowPhaseV6::Acknowledge)&&request.reference()==Some(history.reference()),
                _=>false,
            }
    }
}
pub(crate) fn form_source_current(app: &App, intent: &ScopedWorkflowIntentV6) -> bool {
    app.view == View::Actions
        && app.scoped_actions_v1.as_ref().is_some_and(|state| {
            current(app, state)
                && Arc::ptr_eq(&state.source, intent.source())
                && Arc::ptr_eq(&state.page, intent.page())
        })
}
pub(crate) fn intercept(app: &mut App, action: &ActionsAction) -> bool {
    let ActionsAction::Compose(
        command @ (ActionsCommand::EnableWorkflow | ActionsCommand::DisableWorkflow),
    ) = action
    else {
        return false;
    };
    if app.scoped_actions_v1.is_none() {
        return false;
    }
    let allowed = app.view == View::Actions
        && app.overlay.is_none()
        && !app.loading[View::Actions.index()]
        && app.scoped_actions_work_v1.is_none()
        && app.scoped_actions_owner_v1.is_none()
        && !app.scoped_actions_cancel_v1
        && !newer_foreign_intent(app);
    let captured = app
        .scoped_actions_v1
        .as_ref()
        .filter(|state| allowed && current(app, state))
        .ok_or_else(|| "Original current workflow page required".to_owned())
        .and_then(|state| {
            let epoch = state.epoch.wrapping_add(1);
            let intent = ScopedWorkflowIntentV6::capture(
                state.page.clone(),
                epoch,
                *command == ActionsCommand::EnableWorkflow,
            )?;
            capture_work(
                state.page.clone(),
                epoch,
                PublicationWorkV3::Workflow(Arc::new(WorkflowWorkV6::Open(intent))),
            )
        });
    match captured{Ok(work)=>{
        let state=app.scoped_actions_v1.as_mut().expect("pure actual workflow preflight");
        if let Some(watch)=state.watch_v2.take(){watch.stop();}state.epoch=work.epoch;state.editor_v2=None;
        app.scoped_actions_work_v1=Some(work);app.hits.clear();app.status="Opening original whole-workflow intent • read only • no PUT".into();
    },Err(_)=>explain(app,"Inspect an actual supported workflow Definition/current source; no workflow work or cancellation occurred")};
    true
}
pub(crate) fn capture_form_work(app: &mut App) -> bool {
    if app.overlay != Some(crate::app::Overlay::Form)
        || app.view != View::Actions
        || app.form_dismissed
    {
        return false;
    }
    let Some(form) = app.form.as_ref() else {
        return false;
    };
    let FormPurpose::ScopedActionsWorkflowV6(state) = &form.purpose else {
        return false;
    };
    let phase = if state.ack_requested {
        WorkflowPhaseV6::Acknowledge
    } else if state.inspect_requested {
        WorkflowPhaseV6::Inspect
    } else if form.submit_requested {
        WorkflowPhaseV6::Confirm
    } else if form.preview_requested {
        WorkflowPhaseV6::Preview
    } else {
        return false;
    };
    if app.loading[View::Actions.index()]
        || app.scoped_actions_owner_v1.is_some()
        || app.scoped_actions_work_v1.is_some()
        || app.scoped_actions_cancel_v1
        || newer_foreign_without_form_v3(app)
        || !form_source_current(app, state.opened().intent())
    {
        return false;
    }
    let captured = (|| -> Result<Arc<ScopedActionsWorkV1>, String> {
        let request = workflow_ui::capture(form, phase)?;
        let state = app
            .scoped_actions_v1
            .as_ref()
            .ok_or("Original workflow source missing")?;
        let mut work = capture_work(
            state.page.clone(),
            state.epoch.wrapping_add(1),
            PublicationWorkV3::Workflow(Arc::new(WorkflowWorkV6::Form {
                request: request.clone(),
                confirm: None,
            })),
        )?;
        if phase == WorkflowPhaseV6::Confirm {
            let confirm = WorkflowConfirmV6::capture(
                request
                    .preview()
                    .ok_or("Actual workflow Preview missing")?
                    .clone(),
                work.cancellation(),
                work.deadline(),
            )?;
            Arc::get_mut(&mut work)
                .ok_or("New workflow work aliased")?
                .publication_v3 = Some(Arc::new(PublicationWorkV3::Workflow(Arc::new(
                WorkflowWorkV6::Form {
                    request,
                    confirm: Some(confirm),
                },
            ))));
        }
        Ok(work)
    })();
    match captured {
        Ok(work) => {
            let form = app.form.as_mut().expect("actual captured workflow Form");
            form.preview_requested = false;
            form.submit_requested = false;
            form.submitting = true;
            if let FormPurpose::ScopedActionsWorkflowV6(state) = &mut form.purpose {
                state.inspect_requested = false;
                state.ack_requested = false;
            }
            let state = app
                .scoped_actions_v1
                .as_mut()
                .expect("actual current workflow");
            if let Some(watch) = state.watch_v2.take() {
                watch.stop();
            }
            state.epoch = work.epoch;
            app.scoped_actions_work_v1 = Some(work);
            app.hits.clear();
            app.status = match phase {
                WorkflowPhaseV6::Confirm => {
                    "Explicit whole-workflow Confirm queued • effects may occur • no retry"
                }
                WorkflowPhaseV6::Preview => "Preparing exact workflow Preview • no PUT",
                WorkflowPhaseV6::Inspect => "Inspecting original protected workflow • no resend",
                WorkflowPhaseV6::Acknowledge => {
                    "Original observed workflow ACK queued • local only"
                }
            }
            .into();
        }
        Err(reason) => {
            let form = app.form.as_mut().expect("actual workflow Form");
            form.preview_requested = false;
            form.submit_requested = false;
            if let FormPurpose::ScopedActionsWorkflowV6(state) = &mut form.purpose {
                state.inspect_requested = false;
                state.ack_requested = false;
            }
            form.error = reason;
        }
    }
    true
}
pub(crate) fn execute(
    owner: &Arc<ScopedActionsOwnerV1>,
    publication: &Arc<WorkflowWorkV6>,
    provider: &crate::providers::gh::GithubProvider,
    database: &Path,
    drafts: Option<&DraftClient>,
) -> Result<PreparedWorkflowV6, String> {
    let work = owner.work();
    work.check()?;
    if work.kind != ScopedActionsRequestKindV1::Publication
        || !Arc::ptr_eq(&work.source, publication.intent().source())
        || !Arc::ptr_eq(&work.page, publication.intent().page())
        || (publication.request().is_none() && publication.intent().epoch() != work.epoch)
    {
        return Err("Original workflow work/source/page differs".into());
    }
    let outcome = match &**publication {
        WorkflowWorkV6::Open(intent) => WorkflowOutcomeV6::Open(Box::new(workflow_ui::form(
            workflow::open(
                intent.clone(),
                provider,
                database,
                work.cancellation(),
                work.deadline(),
            )
            .map_err(|_| "Original workflow Open unavailable")?,
        )?)),
        WorkflowWorkV6::Form { request, confirm } => match request.phase() {
            WorkflowPhaseV6::Preview if confirm.is_none() => WorkflowOutcomeV6::Preview(
                workflow::prepare_preview(
                    request.opened().clone(),
                    provider,
                    database,
                    work.cancellation(),
                    work.deadline(),
                )
                .map_err(|_| "Original workflow Preview unavailable")?,
            ),
            WorkflowPhaseV6::Confirm => {
                let confirm = confirm.as_ref().ok_or("Actual workflow Confirm missing")?;
                if !confirm.cancellation().same_instance(work.cancellation())
                    || confirm.deadline() != work.deadline()
                {
                    return Err("Workflow original attempt token/deadline differs".into());
                }
                let fence = owner
                    .publication_v3
                    .lock()
                    .map_err(|_| "Workflow draft cutoff unavailable")?
                    .take()
                    .ok_or("Workflow draft cutoff already consumed")?;
                WorkflowOutcomeV6::Attempt(Box::new(
                    crate::actions_controller::execute_scoped_workflow_v6(
                        confirm,
                        provider,
                        database,
                        drafts.ok_or("Original draft writer unavailable")?,
                        fence,
                    ),
                ))
            }
            WorkflowPhaseV6::Inspect | WorkflowPhaseV6::Acknowledge if confirm.is_none() => {
                WorkflowOutcomeV6::History(
                    workflow::inspect(
                        request.opened(),
                        request
                            .reference()
                            .ok_or("Original workflow reference missing")?,
                        request
                            .owned_history()
                            .ok_or("Actual own workflow history fence missing")?,
                        request.history(),
                        request.phase() == WorkflowPhaseV6::Acknowledge,
                        provider,
                        database,
                        work.cancellation(),
                        work.deadline(),
                    )
                    .map_err(|_| "Original workflow inspection/ACK unavailable")?,
                )
            }
            _ => return Err("Original workflow result/request phase differs".into()),
        },
    };
    #[cfg(test)]
    if let Some(fault) = work.after_read_fault_v1 {
        fault();
    }
    work.check()?;
    Ok(PreparedWorkflowV6 {
        work: work.clone(),
        publication: publication.clone(),
        outcome,
    })
}
pub(crate) fn publish(
    app: &mut App,
    owner: &Arc<ScopedActionsOwnerV1>,
    prepared: PreparedWorkflowV6,
) -> bool {
    if !owner.owns(app) || !prepared.matches(owner.work()) || owner.work().check().is_err() {
        return false;
    }
    match prepared.outcome {
        WorkflowOutcomeV6::Open(form) => {
            app.form = Some(*form);
            app.form_dismissed = false;
            app.overlay = Some(crate::app::Overlay::Form);
            app.status =
                "Original whole-workflow intent ready • Preview then explicit Confirm • no PUT"
                    .into();
        }
        WorkflowOutcomeV6::Preview(preview) => {
            let request = prepared
                .publication
                .request()
                .expect("typed workflow Preview");
            if !app
                .form
                .as_mut()
                .is_some_and(|form| workflow_ui::install_preview(form, request, preview))
            {
                return false;
            }
            app.status="Exact whole-workflow Preview ready • nothing sent • explicit Confirm is NEW bounded attempt".into();
        }
        WorkflowOutcomeV6::History(history) => {
            let request = prepared
                .publication
                .request()
                .expect("typed workflow history");
            if !app
                .form
                .as_mut()
                .is_some_and(|form| workflow_ui::install_history(form, request, history))
            {
                return false;
            }
            app.status =
                "Original workflow inspected/acknowledged • no mutation/retry or causal claim"
                    .into();
        }
        WorkflowOutcomeV6::Attempt(result) => {
            let (execution, history) = result.into_parts();
            // Pure display annotation from the SAME actual typed result. No
            // message/HTTP parsing, authority, ACK or source state is inferred.
            let feedback = match &execution {
                ActionsExecution::Confirmed { .. } => {
                    Some(workflow_ui::WorkflowOutcomeFeedbackV25::Confirmed)
                }
                ActionsExecution::AcceptedPending { .. } => {
                    Some(workflow_ui::WorkflowOutcomeFeedbackV25::AcceptedPending)
                }
                _ => None,
            };
            let (reference,message,visible)=match execution{
                ActionsExecution::Confirmed{publication,..}=>(Some(publication),workflow_ui::WorkflowOutcomeFeedbackV25::Confirmed.message().into(),true),
                ActionsExecution::AcceptedPending{publication,..}=>(Some(publication),workflow_ui::WorkflowOutcomeFeedbackV25::AcceptedPending.message().into(),true),
                ActionsExecution::Unknown{publication,private_visible,..}|ActionsExecution::ReceiptPersistenceFailed{publication,private_visible,..}|ActionsExecution::EffectConfirmedButRefreshFailed{publication,private_visible,..}|ActionsExecution::AcceptedButRefreshFailed{publication,private_visible,..}=>(Some(publication),"Original workflow outcome uncertain or unavailable • exact protected result retained • no retry".into(),private_visible),
                ActionsExecution::Refused{private_visible,..}=>(None,"Original workflow refused before invocation • protected draft/marker retained • no retry".into(),private_visible),
                _=>(None,"Original workflow result unavailable • no retry".into(),false),
            };
            if !visible {
                return super::fail(app, owner);
            }
            super::retired_underlay(app);
            // A completed/uncertain write retires the prior workflow-state
            // catalogue and its Back snapshots. Before an attempt, pure Back
            // retains them; after it, no old active/disabled row is revived.
            if let Some(state) = app.scoped_actions_v1.as_mut() {
                state.back.clear();
            }
            if let Some(workspace) = app.actions.as_mut() {
                if let Some(control) = workspace
                    .controls
                    .iter_mut()
                    .find(|control| control.action == ActionsAction::Refresh)
                {
                    control.label = "[r Refresh actual workflow]".into();
                }
            }
            let request = prepared
                .publication
                .request()
                .expect("typed workflow Confirm");
            let Some(form) = app.form.as_mut() else {
                return false;
            };
            if let Some(reference) = reference {
                if !workflow_ui::install_attempt(
                    form, request, reference, message, feedback, history,
                ) {
                    return false;
                }
            } else {
                request.retire_busy(form);
                form.error = message;
            }
            app.status="Original workflow attempt finished • acceptance/observed state only • durable protection retained".into();
        }
    }
    app.hits.clear();
    true
}
