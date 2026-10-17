//! Closed Dispatch variants on SAME Actions owner/work/result/slot and fences.
use super::*;
use crate::{
    providers::gh::GithubProvider,
    scoped_actions_dispatch_publication_v7::{
        self as dispatch, DispatchConfirmV7, DispatchPreviewV7, PreparedDispatchSchemaV7,
        ScopedDispatchIntentV7,
    },
    scoped_actions_dispatch_ui_v7::{self as dispatch_ui, DispatchPhaseV7, DispatchRequestV7},
};
#[path = "scoped_actions_dispatch_run_read_v7.rs"]
pub(crate) mod run_read_v7;
#[cfg(test)]
#[path = "scoped_actions_dispatch_v7_tests.rs"]
mod tests;
pub(crate) enum DispatchWorkV7 {
    Read(Arc<run_read_v7::DispatchRunReadRequestV7>),
    Open(Arc<ScopedDispatchIntentV7>),
    Form {
        request: Arc<DispatchRequestV7>,
        confirm: Option<Arc<DispatchConfirmV7>>,
    },
}
impl DispatchWorkV7 {
    pub(crate) fn intent(&self) -> &Arc<ScopedDispatchIntentV7> {
        match self {
            Self::Read(read) => read.intent(),
            Self::Open(intent) => intent,
            Self::Form { request, .. } => request.opened().intent(),
        }
    }
    pub(crate) fn request(&self) -> Option<&Arc<DispatchRequestV7>> {
        match self {
            Self::Read(read) => read.opening(),
            Self::Form { request, .. } => Some(request),
            _ => None,
        }
    }
    pub(crate) fn page(&self) -> &Arc<ScopedActionsPageV1> {
        match self {
            Self::Read(read) => read.page(),
            _ => self.intent().page(),
        }
    }
    pub(crate) fn confirm(&self) -> Option<&Arc<DispatchConfirmV7>> {
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
            Self::Read(read) => read.overlay_current(app),
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
pub struct PreparedDispatchV7 {
    work: Arc<ScopedActionsWorkV1>,
    publication: Arc<DispatchWorkV7>,
    outcome: DispatchOutcomeV7,
}
enum DispatchOutcomeV7 {
    Read(Box<crate::scoped_actions_v1::PreparedScopedActionsV1>),
    Open(Box<Form>),
    Schema(Box<PreparedDispatchSchemaV7>),
    Preview(Arc<DispatchPreviewV7>),
    Attempt(Box<crate::actions_controller::DispatchExecutionV7>),
    History(Arc<crate::scoped_actions_dispatch_history_v7::DispatchHistoryV7>),
}
impl PreparedDispatchV7 {
    pub(crate) fn matches(&self, work: &Arc<ScopedActionsWorkV1>) -> bool {
        Arc::ptr_eq(&self.work,work)&&work.kind==ScopedActionsRequestKindV1::Publication&&work.journal_v5.is_none()
            &&work.publication_v3.as_deref().is_some_and(|publication|matches!(publication,PublicationWorkV3::Dispatch(current)if Arc::ptr_eq(current,&self.publication)))
            &&Arc::ptr_eq(&work.source,self.publication.intent().source())&&Arc::ptr_eq(&work.page,self.publication.page())
            &&match(&*self.publication,&self.outcome){
                (DispatchWorkV7::Read(request),DispatchOutcomeV7::Read(prepared))=>request.matches_work(work)&&prepared.matches(work),
                (DispatchWorkV7::Open(intent),DispatchOutcomeV7::Open(form))=>matches!(&form.purpose,FormPurpose::ScopedActionsDispatchV7(state)if Arc::ptr_eq(state.opened().intent(),intent)),
                (DispatchWorkV7::Form{request,confirm:None},DispatchOutcomeV7::Schema(prepared))=>request.phase()==DispatchPhaseV7::Schema&&Arc::ptr_eq(request.opened(),prepared.opened())&&request.choice()==prepared.schema().choice()&&request.protocol()==prepared.schema().protocol(),
                (DispatchWorkV7::Form{request,confirm:None},DispatchOutcomeV7::Preview(preview))=>request.phase()==DispatchPhaseV7::Preview&&Arc::ptr_eq(request.opened(),preview.opened())&&request.inputs().is_some_and(|inputs|Arc::ptr_eq(inputs,preview.inputs())),
                (DispatchWorkV7::Form{request,confirm:Some(confirm)},DispatchOutcomeV7::Attempt(result))=>result.matches_history()&&request.phase()==DispatchPhaseV7::Confirm&&request.preview().is_some_and(|preview|Arc::ptr_eq(preview,confirm.preview())),
                (DispatchWorkV7::Form{request,confirm:None},DispatchOutcomeV7::History(history))=>matches!(request.phase(),DispatchPhaseV7::Inspect|DispatchPhaseV7::Acknowledge)&&request.reference()==Some(history.reference()),
                _=>false,
            }
    }
}
pub(crate) fn form_source_current(app: &App, intent: &ScopedDispatchIntentV7) -> bool {
    app.view == View::Actions
        && app.scoped_actions_v1.as_ref().is_some_and(|state| {
            current(app, state)
                && Arc::ptr_eq(&state.source, intent.source())
                && Arc::ptr_eq(&state.page, intent.page())
        })
}
pub(crate) fn intercept(app: &mut App, action: &ActionsAction) -> bool {
    if *action != ActionsAction::Compose(ActionsCommand::DispatchWorkflow)
        || app.scoped_actions_v1.is_none()
    {
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
        .ok_or_else(|| "Original current workflow required".to_owned())
        .and_then(|state| {
            let epoch = state.epoch.wrapping_add(1);
            let intent = ScopedDispatchIntentV7::capture(state.page.clone(), epoch)?;
            capture_work(
                state.page.clone(),
                epoch,
                PublicationWorkV3::Dispatch(Arc::new(DispatchWorkV7::Open(intent))),
            )
        });
    match captured{Ok(work)=>{let state=app.scoped_actions_v1.as_mut().expect("pure actual Dispatch preflight");if let Some(watch)=state.watch_v2.take(){watch.stop();}state.epoch=work.epoch;state.editor_v2=None;app.scoped_actions_work_v1=Some(work);app.hits.clear();app.status="Opening original workflow Dispatch • read only • no POST".into();},
        Err(_)=>explain(app,"Inspect actual active workflow Definition/current source first; no work, cancellation or dispatch occurred")};
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
    let FormPurpose::ScopedActionsDispatchV7(state) = &form.purpose else {
        return false;
    };
    let phase = if state.open_run_requested {
        DispatchPhaseV7::OpenRun
    } else if state.ack_requested {
        DispatchPhaseV7::Acknowledge
    } else if state.inspect_requested {
        DispatchPhaseV7::Inspect
    } else if form.submit_requested {
        DispatchPhaseV7::Confirm
    } else if form.preview_requested {
        state.next_preview_phase(&form.fields)
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
        let state = app
            .scoped_actions_v1
            .as_ref()
            .ok_or("Original Dispatch source missing")?;
        if phase == DispatchPhaseV7::OpenRun {
            return run_read_v7::capture_open(form, state);
        }
        let request = dispatch_ui::capture(form, phase)?;
        let mut work = capture_work(
            state.page.clone(),
            state.epoch.wrapping_add(1),
            PublicationWorkV3::Dispatch(Arc::new(DispatchWorkV7::Form {
                request: request.clone(),
                confirm: None,
            })),
        )?;
        if phase == DispatchPhaseV7::Confirm {
            let confirm = DispatchConfirmV7::capture(
                request
                    .preview()
                    .ok_or("Actual Dispatch Preview missing")?
                    .clone(),
                work.cancellation(),
                work.deadline(),
            )?;
            Arc::get_mut(&mut work)
                .ok_or("New Dispatch work aliased")?
                .publication_v3 = Some(Arc::new(PublicationWorkV3::Dispatch(Arc::new(
                DispatchWorkV7::Form {
                    request,
                    confirm: Some(confirm),
                },
            ))));
        }
        Ok(work)
    })();
    match captured {
        Ok(work) => {
            let form = app.form.as_mut().expect("actual captured Dispatch Form");
            form.preview_requested = false;
            form.submit_requested = false;
            form.submitting = true;
            if let FormPurpose::ScopedActionsDispatchV7(state) = &mut form.purpose {
                state.inspect_requested = false;
                state.ack_requested = false;
                state.open_run_requested = false;
            }
            let state = app
                .scoped_actions_v1
                .as_mut()
                .expect("actual original Dispatch source");
            if let Some(watch) = state.watch_v2.take() {
                watch.stop();
            }
            state.epoch = work.epoch;
            app.scoped_actions_work_v1 = Some(work);
            app.hits.clear();
            app.status = match phase {
                DispatchPhaseV7::OpenRun => {
                    "Reading actual POST200 returned Run • READ ONLY • no guessed204 target"
                }
                DispatchPhaseV7::Schema => {
                    "Reading actual exact-ref/version Dispatch schema • no POST"
                }
                DispatchPhaseV7::Preview => "Preparing exact typed Dispatch Preview • no POST",
                DispatchPhaseV7::Confirm => {
                    "Explicit Dispatch Confirm queued • effects may occur • no retry"
                }
                DispatchPhaseV7::Inspect => "Inspecting original protected Dispatch • no resend",
                DispatchPhaseV7::Acknowledge => {
                    "Explicit original Dispatch acceptance ACK queued • local only"
                }
            }
            .into();
        }
        Err(reason) => {
            let form = app.form.as_mut().expect("actual Dispatch Form");
            form.preview_requested = false;
            form.submit_requested = false;
            if let FormPurpose::ScopedActionsDispatchV7(state) = &mut form.purpose {
                state.inspect_requested = false;
                state.ack_requested = false;
                state.open_run_requested = false;
            }
            form.error = reason;
        }
    }
    true
}
pub(crate) fn execute(
    owner: &Arc<ScopedActionsOwnerV1>,
    publication: &Arc<DispatchWorkV7>,
    provider: &GithubProvider,
    database: &Path,
    drafts: Option<&DraftClient>,
) -> Result<PreparedDispatchV7, String> {
    let work = owner.work();
    work.check()?;
    if work.kind != ScopedActionsRequestKindV1::Publication
        || !Arc::ptr_eq(&work.source, publication.intent().source())
        || !Arc::ptr_eq(&work.page, publication.page())
        || (matches!(&**publication, DispatchWorkV7::Open(_))
            && publication.intent().epoch() != work.epoch)
    {
        return Err("Original Dispatch work/source/page differs".into());
    }
    let outcome = match &**publication {
        DispatchWorkV7::Read(request) => DispatchOutcomeV7::Read(Box::new(
            crate::scoped_actions_v1::execute_dispatch_read_v7(
                work.clone(),
                request,
                provider,
                database,
            )?,
        )),
        DispatchWorkV7::Open(intent) => DispatchOutcomeV7::Open(Box::new(dispatch_ui::form(
            dispatch::open(
                intent.clone(),
                provider,
                database,
                work.cancellation(),
                work.deadline(),
            )
            .map_err(|_| "Original Dispatch Open unavailable")?,
        )?)),
        DispatchWorkV7::Form { request, confirm } => match request.phase() {
            DispatchPhaseV7::Schema if confirm.is_none() => DispatchOutcomeV7::Schema(Box::new(
                dispatch::prepare_schema(
                    request.opened().clone(),
                    request.choice().clone(),
                    request.protocol(),
                    provider,
                    database,
                    work.cancellation(),
                    work.deadline(),
                )
                .map_err(|_| "Exact Dispatch schema/ref/version unavailable; no POST")?,
            )),
            DispatchPhaseV7::Preview if confirm.is_none() => DispatchOutcomeV7::Preview(
                dispatch::prepare_preview(
                    request,
                    provider,
                    database,
                    work.cancellation(),
                    work.deadline(),
                )
                .map_err(|_| "Exact Dispatch Preview unavailable; values hidden")?,
            ),
            DispatchPhaseV7::Confirm => {
                let confirm = confirm.as_ref().ok_or("Actual Dispatch Confirm missing")?;
                if !confirm.cancellation().same_instance(work.cancellation())
                    || confirm.deadline() != work.deadline()
                {
                    return Err("Dispatch original attempt clock/token differs".into());
                }
                let fence = owner
                    .publication_v3
                    .lock()
                    .map_err(|_| "Dispatch draft cutoff unavailable")?
                    .take()
                    .ok_or("Dispatch cutoff already consumed")?;
                DispatchOutcomeV7::Attempt(Box::new(
                    crate::actions_controller::execute_scoped_dispatch_v7(
                        confirm,
                        provider,
                        database,
                        drafts.ok_or("Original draft writer unavailable")?,
                        fence,
                    ),
                ))
            }
            DispatchPhaseV7::Inspect | DispatchPhaseV7::Acknowledge if confirm.is_none() => {
                DispatchOutcomeV7::History(
                    crate::scoped_actions_dispatch_history_v7::inspect(
                        request.opened(),
                        request
                            .reference()
                            .ok_or("Original Dispatch reference missing")?,
                        request
                            .owned_history()
                            .ok_or("Actual own Dispatch history fence missing")?,
                        request.history(),
                        request.phase() == DispatchPhaseV7::Acknowledge,
                        provider,
                        database,
                        work.cancellation(),
                        work.deadline(),
                    )
                    .map_err(|_| "Original Dispatch inspection/ACK unavailable")?,
                )
            }
            _ => return Err("Original Dispatch phase/result differs".into()),
        },
    };
    #[cfg(test)]
    if let Some(fault) = work.after_read_fault_v1 {
        fault();
    }
    work.check()?;
    Ok(PreparedDispatchV7 {
        work: work.clone(),
        publication: publication.clone(),
        outcome,
    })
}
pub(crate) fn publish(
    app: &mut App,
    owner: &Arc<ScopedActionsOwnerV1>,
    prepared: PreparedDispatchV7,
) -> bool {
    if !owner.owns(app) || !prepared.matches(owner.work()) || owner.work().check().is_err() {
        return false;
    }
    match prepared.outcome {
        DispatchOutcomeV7::Read(read) => {
            let DispatchWorkV7::Read(request) = &*prepared.publication else {
                return false;
            };
            return run_read_v7::publish(app, owner, request, *read);
        }
        DispatchOutcomeV7::Open(form) => {
            app.form = Some(*form);
            app.form_dismissed = false;
            app.overlay = Some(crate::app::Overlay::Form);
            app.status="Original Dispatch intent ready • inspect schema, Preview then explicit Confirm • no POST".into();
        }
        DispatchOutcomeV7::Schema(schema) => {
            let request = prepared.publication.request().expect("typed schema");
            if !app
                .form
                .as_mut()
                .is_some_and(|form| dispatch_ui::install_schema(form, request, *schema))
            {
                return false;
            }
            app.status =
                "Exact ref/protocol Dispatch schema ready • no POST • values are transient".into();
        }
        DispatchOutcomeV7::Preview(preview) => {
            let request = prepared.publication.request().expect("typed Preview");
            if !app
                .form
                .as_mut()
                .is_some_and(|form| dispatch_ui::install_preview(form, request, preview))
            {
                return false;
            }
            app.status="Exact Dispatch Preview ready • nothing sent • explicit Confirm is one NEW bounded attempt".into();
        }
        DispatchOutcomeV7::History(history) => {
            let request = prepared.publication.request().expect("typed history");
            if !app
                .form
                .as_mut()
                .is_some_and(|form| dispatch_ui::install_history(form, request, history))
            {
                return false;
            }
            app.status="Original Dispatch inspected/acceptance acknowledged • no completion/causal/retry claim".into();
        }
        DispatchOutcomeV7::Attempt(result) => {
            let (execution, history, run) = result.into_parts();
            let(reference,message,visible,feedback)=match execution{
                ActionsExecution::AcceptedPending{publication,..}=>{
                    let feedback=if run.is_some(){dispatch_ui::DispatchOutcomeFeedbackV33::AcceptedWithRun}else{dispatch_ui::DispatchOutcomeFeedbackV33::AcceptedWithoutRun};
                    (Some(publication),feedback.message().into(),true,Some(feedback))
                },
                ActionsExecution::Unknown{publication,private_visible,..}|ActionsExecution::ReceiptPersistenceFailed{publication,private_visible,..}|ActionsExecution::EffectConfirmedButRefreshFailed{publication,private_visible,..}|ActionsExecution::AcceptedButRefreshFailed{publication,private_visible,..}=>(Some(publication),"Original Dispatch outcome uncertain/unavailable • exact protected marker retained • no retry".into(),private_visible,None),
                ActionsExecution::Refused{private_visible,..}=>(None,"Original Dispatch refused before invocation • no retry/private diagnostics".into(),private_visible,None),
                _=>(None,"Original Dispatch unavailable • no completion or retry claim".into(),false,None),
            };
            if !visible {
                return super::fail(app, owner);
            }
            super::retired_underlay(app);
            if let Some(state) = app.scoped_actions_v1.as_mut() {
                state.back.clear();
            }
            let request = prepared.publication.request().expect("typed Confirm");
            let Some(form) = app.form.as_mut() else {
                return false;
            };
            if let Some(reference) = reference {
                if !dispatch_ui::install_attempt(
                    form, request, reference, message, history, run, feedback,
                ) {
                    return false;
                }
            } else {
                request.retire_busy(form);
                form.error = message;
            }
            app.status="Original Dispatch attempt finished • acceptance only • durable protection retained".into();
        }
    }
    app.loading[View::Actions.index()] = false;
    app.hits.clear();
    true
}
