//! W9 is a closed purpose in the SAME Actions owner/work/result/physical slot.
//! Local steps never borrow the remote publisher's approval or draft fence.
use super::*;
use crate::{
    drafts::{DraftClient, ReadCutoffFailureV20},
    providers::gh::GithubProvider,
    scoped_actions_workflow_edit_ui_v9::{
        self as edit_ui, ClosedWorkflowFormV9, WorkflowEditPhaseV9, WorkflowEditRequestV9,
        WorkflowEditStepV9, WorkflowLocalPreviewV9, WorkflowLocalReceiptV9,
    },
    scoped_actions_workflow_edit_v9::{self as edit, draft_cutoff_v9::WorkflowEditReadCutoffV9, *},
};
#[path = "scoped_actions_workflow_edit_native_presentation_v9.rs"]
mod native_presentation_v9;
pub(crate) use native_presentation_v9::WorkflowNativePresentationV9;

pub(crate) enum WorkflowEditWorkV9 {
    Open {
        intent: Arc<WorkflowEditIntentV9>,
        returning: Option<Arc<ClosedWorkflowFormV9>>,
        presentation: Arc<WorkflowNativePresentationV9>,
    },
    Form {
        request: Arc<WorkflowEditRequestV9>,
        confirm: Option<WorkflowEditConfirmV9>,
    },
}
pub(crate) enum WorkflowEditConfirmV9 {
    Worktree(Arc<WorkflowWorktreeConfirmV9>),
    Save(Arc<WorkflowSaveConfirmV9>),
    Stage(Arc<WorkflowStageConfirmV9>),
    Commit(Arc<WorkflowCommitConfirmV9>),
    Remote(Arc<WorkflowRemoteConfirmV9>),
}
impl WorkflowEditWorkV9 {
    fn matches_native_work_v9(&self, work: &ScopedActionsWorkV1) -> bool {
        match self {
            Self::Open { presentation, .. } => presentation.matches_work(work),
            Self::Form { request, .. } => request.matches_native_work_v9(work),
        }
    }
    pub(crate) fn intent(&self) -> &Arc<WorkflowEditIntentV9> {
        match self {
            Self::Open { intent, .. } => intent,
            Self::Form { request, .. } => request.intent(),
        }
    }
    pub(crate) fn request(&self) -> Option<&Arc<WorkflowEditRequestV9>> {
        match self {
            Self::Form { request, .. } => Some(request),
            _ => None,
        }
    }
    pub(crate) fn matches_form(&self, form: &Form) -> bool {
        self.request().is_some_and(|r| r.matches_form(form))
    }
    pub(crate) fn overlay_current(&self, app: &App) -> bool {
        match self {
            Self::Open {
                returning,
                presentation,
                ..
            } => {
                app.overlay.is_none()
                    && presentation.matches_open_app(app)
                    && returning.as_ref().is_none_or(|old| {
                        app.form_dismissed
                            && app.form.as_ref().is_some_and(|form| old.matches(form))
                    })
            }
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
pub struct PreparedWorkflowEditV9 {
    work: Arc<ScopedActionsWorkV1>,
    publication: Arc<WorkflowEditWorkV9>,
    outcome: WorkflowEditOutcomeV9,
}
enum WorkflowEditOutcomeV9 {
    Open(Box<Form>),
    Preview(WorkflowLocalPreviewV9),
    Receipt(WorkflowLocalReceiptV9),
    RemoteAttempt(Box<crate::actions_controller::WorkflowRemoteExecutionV9>),
    DraftReadFailed(WorkflowDraftReadFailureV9),
}
enum WorkflowDraftReadFailureV9 {
    Unavailable,
    RestoreExactRetained,
    RetireInvalidRetained,
}
impl PreparedWorkflowEditV9 {
    pub(crate) fn matches(&self, work: &Arc<ScopedActionsWorkV1>) -> bool {
        Arc::ptr_eq(&self.work,work)&&self.publication.matches_native_work_v9(work)&&work.kind==ScopedActionsRequestKindV1::Publication&&work.journal_v5.is_none()
            &&work.publication_v3.as_deref().is_some_and(|p|matches!(p,PublicationWorkV3::WorkflowEdit(current) if Arc::ptr_eq(current,&self.publication)))
            &&Arc::ptr_eq(&work.source,self.publication.intent().source())&&Arc::ptr_eq(&work.page,self.publication.intent().page())
            &&match(&*self.publication,&self.outcome){
                (WorkflowEditWorkV9::Open{intent,..},WorkflowEditOutcomeV9::Open(form))=>matches!(&form.purpose,FormPurpose::ScopedWorkflowEditV9(state) if Arc::ptr_eq(state.intent(),intent)),
                (WorkflowEditWorkV9::Open{..},WorkflowEditOutcomeV9::DraftReadFailed(_))=>true,
                (WorkflowEditWorkV9::Form{request,confirm:None},WorkflowEditOutcomeV9::Preview(_))=>request.phase()==WorkflowEditPhaseV9::Preview,
                (WorkflowEditWorkV9::Form{request,confirm:Some(WorkflowEditConfirmV9::Worktree(_)|WorkflowEditConfirmV9::Save(_)|WorkflowEditConfirmV9::Stage(_)|WorkflowEditConfirmV9::Commit(_))},WorkflowEditOutcomeV9::Receipt(_))=>request.phase()==WorkflowEditPhaseV9::Confirm,
                (WorkflowEditWorkV9::Form{request,confirm:Some(WorkflowEditConfirmV9::Remote(confirm))},WorkflowEditOutcomeV9::RemoteAttempt(result))=>request.phase()==WorkflowEditPhaseV9::Confirm && result.matches_preview_v9(confirm.preview()),
                _=>false,
            }
    }
}
pub(crate) fn source_current(app: &App, intent: &WorkflowEditIntentV9) -> bool {
    app.view == View::Actions
        && app.scoped_actions_v1.as_ref().is_some_and(|state| {
            current(app, state)
                && Arc::ptr_eq(&state.source, intent.source())
                && Arc::ptr_eq(&state.page, intent.page())
        })
}
pub(crate) fn intercept(app: &mut App, action: &ActionsAction) -> bool {
    if *action != ActionsAction::EditWorkflowLocalV9 {
        return false;
    }
    let captured = (|| -> Result<Arc<ScopedActionsWorkV1>, String> {
        if app.view != View::Actions
            || app.overlay.is_some()
            || app.loading[View::Actions.index()]
            || app.scoped_actions_work_v1.is_some()
            || app.scoped_actions_owner_v1.is_some()
            || app.scoped_actions_cancel_v1
            || newer_foreign_intent(app)
        {
            return Err("Another original intent is current".into());
        }
        let state = app
            .scoped_actions_v1
            .as_ref()
            .filter(|state| current(app, state))
            .ok_or("Actual original Workflow Definition required")?;
        let epoch = state.epoch.wrapping_add(1);
        let presentation = WorkflowNativePresentationV9::capture(app, epoch)?;
        let intent = WorkflowEditIntentV9::capture(state.page.clone(), epoch)?;
        let returning = match app.form.as_ref().filter(|form| {
            app.form_dismissed && matches!(form.purpose, FormPurpose::ScopedWorkflowEditV9(_))
        }) {
            Some(form) => Some(
                ClosedWorkflowFormV9::capture(form, &intent)
                    .ok_or("Exact retained workflow Form unavailable; no uncharged reopening")?,
            ),
            None => None,
        };
        capture_work(
            state.page.clone(),
            epoch,
            PublicationWorkV3::WorkflowEdit(Arc::new(WorkflowEditWorkV9::Open {
                intent,
                returning,
                presentation,
            })),
        )
    })();
    match captured{Ok(work)=>{
        let state=app.scoped_actions_v1.as_mut().expect("pure exact W9 source preflight");
        if let Some(watch)=state.watch_v2.take(){watch.stop();}state.epoch=work.epoch;state.editor_v2=None;
        app.scoped_actions_work_v1=Some(work);app.hits.clear();app.status="Opening exact workflow local editor • read only • original queue clock • no Git/remote write".into();
    },Err(_)=>explain(app,"Inspect the actual original workflow Definition/current source first; local mapping is not write permission and no operation was queued")};
    true
}
pub(crate) fn capture_form_work(app: &mut App) -> bool {
    if app.view != View::Actions
        || app.overlay != Some(crate::app::Overlay::Form)
        || app.form_dismissed
    {
        return false;
    }
    let Some(form) = app
        .form
        .as_ref()
        .filter(|f| matches!(f.purpose, FormPurpose::ScopedWorkflowEditV9(_)))
    else {
        return false;
    };
    if let Some(ack) = edit_ui::history_requested_v9(form) {
        return crate::scoped_actions_native_v1::journal_v5::capture_workflow_form_v9(app, ack);
    }
    let phase = if form.submit_requested {
        WorkflowEditPhaseV9::Confirm
    } else if form.preview_requested {
        WorkflowEditPhaseV9::Preview
    } else {
        return false;
    };
    let FormPurpose::ScopedWorkflowEditV9(state) = &form.purpose else {
        return false;
    };
    if app.loading[View::Actions.index()]
        || app.scoped_actions_work_v1.is_some()
        || app.scoped_actions_owner_v1.is_some()
        || app.scoped_actions_cancel_v1
        || newer_foreign_without_form_v3(app)
        || !source_current(app, state.intent())
    {
        return false;
    }
    let captured = (|| -> Result<Arc<ScopedActionsWorkV1>, String> {
        let state = app
            .scoped_actions_v1
            .as_ref()
            .ok_or("Original W9 page unavailable")?;
        let presentation = WorkflowNativePresentationV9::capture(app, state.epoch.wrapping_add(1))?;
        let request = edit_ui::capture_native_v9(form, phase, presentation)?;
        let mut work = capture_work(
            state.page.clone(),
            state.epoch.wrapping_add(1),
            PublicationWorkV3::WorkflowEdit(Arc::new(WorkflowEditWorkV9::Form {
                request: request.clone(),
                confirm: None,
            })),
        )?;
        if phase == WorkflowEditPhaseV9::Confirm {
            let confirm = match request
                .preview()
                .ok_or("Exact local-step Preview missing")?
            {
                WorkflowLocalPreviewV9::Worktree(_) => WorkflowEditConfirmV9::Worktree(
                    WorkflowWorktreeConfirmV9::capture_for_form_v9(
                        &request,
                        work.cancellation(),
                        work.deadline(),
                    )
                    .map_err(|_| "Original isolation Confirm unavailable")?,
                ),
                WorkflowLocalPreviewV9::Save(_) => WorkflowEditConfirmV9::Save(
                    WorkflowSaveConfirmV9::capture_for_form_v9(
                        &request,
                        work.cancellation(),
                        work.deadline(),
                    )
                    .map_err(|_| "Original Save Confirm unavailable")?,
                ),
                WorkflowLocalPreviewV9::Stage(_) => WorkflowEditConfirmV9::Stage(
                    WorkflowStageConfirmV9::capture_for_form_v9(
                        &request,
                        work.cancellation(),
                        work.deadline(),
                    )
                    .map_err(|_| "Original Stage Confirm unavailable")?,
                ),
                WorkflowLocalPreviewV9::Commit(_) => WorkflowEditConfirmV9::Commit(
                    WorkflowCommitConfirmV9::capture_for_form_v9(
                        &request,
                        work.cancellation(),
                        work.deadline(),
                    )
                    .map_err(|_| "Original Commit Confirm unavailable")?,
                ),
                WorkflowLocalPreviewV9::Remote(_) => WorkflowEditConfirmV9::Remote(
                    WorkflowRemoteConfirmV9::capture_for_form_v9(
                        &request,
                        work.cancellation(),
                        work.deadline(),
                    )
                    .map_err(|_| "Original exact remote Confirm unavailable")?,
                ),
            };
            Arc::get_mut(&mut work)
                .ok_or("New original W9 work aliased")?
                .publication_v3 = Some(Arc::new(PublicationWorkV3::WorkflowEdit(Arc::new(
                WorkflowEditWorkV9::Form {
                    request,
                    confirm: Some(confirm),
                },
            ))));
        }
        Ok(work)
    })();
    match captured {
        Ok(work) => {
            let form = app.form.as_mut().expect("actual captured W9 Form");
            form.preview_requested = false;
            form.submit_requested = false;
            form.submitting = true;
            let state = app.scoped_actions_v1.as_mut().expect("actual W9 source");
            if let Some(watch) = state.watch_v2.take() {
                watch.stop();
            }
            state.epoch = work.epoch;
            app.scoped_actions_work_v1 = Some(work);
            app.hits.clear();
            app.status =
                "Preparing exact original workflow step • no automatic retry or renewed approval"
                    .into();
        }
        Err(_) => {
            if let Some(form) = app.form.as_mut() {
                form.preview_requested = false;
                form.submit_requested = false;
                form.error="Original exact workflow step unavailable; inspect current fields/receipt/source. No operation queued.".into();
            }
        }
    }
    true
}
/// Called only after SAME pure runtime admission and BEFORE slot cancellation,
/// generation/loading/owner effects or try_send. Queue failure drops own ticket.
pub(crate) fn prepare_cutoff(
    owner: &Arc<ScopedActionsOwnerV1>,
    provider: &GithubProvider,
    database: &Path,
    drafts: Option<&DraftClient>,
) -> Result<(), String> {
    let Some(PublicationWorkV3::WorkflowEdit(publication)) = owner.work().publication_v3.as_deref()
    else {
        return Ok(());
    };
    if let WorkflowEditWorkV9::Form {
        confirm: Some(WorkflowEditConfirmV9::Remote(_)),
        ..
    } = &**publication
    {
        let effective = provider
            .workflow_edit_queued_deadline_v9(owner.deadline(), owner.cancellation())
            .map_err(|_| "Original remote issuer clock unavailable")?;
        let fence = drafts
            .ok_or("Original workflow draft client unavailable")?
            .capture_workflow_edit_publication_v9(
                database,
                publication.intent(),
                owner.cancellation(),
                effective,
            )
            .map_err(|_| "Original workflow publication fence unavailable")?;
        let mut slot = owner
            .publication_v3
            .lock()
            .map_err(|_| "Original workflow publication fence unavailable")?;
        if slot.is_some() {
            return Err("Original workflow publication fence occupied".into());
        }
        *slot = Some(fence);
        return Ok(());
    }
    if !matches!(&**publication, WorkflowEditWorkV9::Open { .. }) {
        return Ok(());
    }
    let effective = provider
        .workflow_edit_queued_deadline_v9(owner.deadline(), owner.cancellation())
        .map_err(|_| "Original W9 issuer clock unavailable")?;
    let cutoff = WorkflowEditReadCutoffV9::capture(
        drafts.ok_or("Original W9 draft client unavailable")?,
        database,
        owner.cancellation(),
        effective,
    )
    .map_err(|_| "Original W9 draft read cutoff unavailable")?;
    let mut slot = owner
        .workflow_edit_read_v9
        .lock()
        .map_err(|_| "Original W9 cutoff unavailable")?;
    if slot.is_some() {
        return Err("Original W9 cutoff already captured".into());
    }
    *slot = Some(cutoff);
    Ok(())
}
pub(crate) fn execute(
    owner: &Arc<ScopedActionsOwnerV1>,
    publication: &Arc<WorkflowEditWorkV9>,
    provider: &GithubProvider,
    database: &Path,
    drafts: Option<&DraftClient>,
) -> Result<PreparedWorkflowEditV9, String> {
    let work = owner.work();
    work.check()?;
    if work.kind != ScopedActionsRequestKindV1::Publication
        || !publication.matches_native_work_v9(work)
        || !Arc::ptr_eq(&work.source, publication.intent().source())
        || !Arc::ptr_eq(&work.page, publication.intent().page())
    {
        return Err("Original W9 source/page/purpose differs".into());
    }
    let outcome = match &**publication {
        WorkflowEditWorkV9::Open {
            intent,
            returning,
            presentation,
        } => {
            if intent.epoch() != work.epoch {
                return Err("Original W9 opener epoch differs".into());
            }
            let drafts = drafts.ok_or("Original W9 draft client unavailable")?;
            let slot = owner
                .workflow_edit_read_v9
                .lock()
                .map_err(|_| "Original W9 cutoff unavailable")?;
            let cutoff = slot.as_ref().ok_or("Original prequeue W9 cutoff missing")?;
            match cutoff.wait(drafts, work.cancellation()) {
                Err(failure) => {
                    // Typed SaveFailed ONLY. Validate the SAME already-issued
                    // local/file/Clear witness once; no Core/GH/source factory.
                    let outcome = match (failure, returning.as_ref()) {
                        (ReadCutoffFailureV20::SaveFailed, Some(old)) => {
                            match old.validate_boundary(database, work.cancellation()) {
                                Ok(()) => WorkflowDraftReadFailureV9::RestoreExactRetained,
                                Err(_) => WorkflowDraftReadFailureV9::RetireInvalidRetained,
                            }
                        }
                        _ => WorkflowDraftReadFailureV9::Unavailable,
                    };
                    WorkflowEditOutcomeV9::DraftReadFailed(outcome)
                }
                Ok(()) => {
                    let (opened, seed) = edit::prepare_local_held_native_v9(
                        intent.clone(),
                        returning.as_deref(),
                        presentation,
                        provider,
                        database,
                        work.cancellation(),
                        work.deadline(),
                    )
                    .map_err(|_| "Original W9 local source/draft unavailable")?;
                    cutoff.check(drafts, work.cancellation()).map_err(|_| {
                        "Original W9 accepted draft ledger changed before publication"
                    })?;
                    WorkflowEditOutcomeV9::Open(seed.attach(opened)?)
                }
            }
        }
        WorkflowEditWorkV9::Form {
            request,
            confirm: None,
        } => {
            if request.phase() != WorkflowEditPhaseV9::Preview {
                return Err("Exact W9 Preview purpose required".into());
            }
            let preview=match request.step(){
                WorkflowEditStepV9::Isolate=>WorkflowLocalPreviewV9::Worktree(WorkflowWorktreePreviewV9::prepare_for_form_v9(request,provider,database,work.cancellation(),work.deadline()).map_err(|_|"Original isolated-worktree Preview unavailable")?),
                WorkflowEditStepV9::Save=>WorkflowLocalPreviewV9::Save(WorkflowSavePreviewV9::prepare_for_form_v9(request,provider,database,work.cancellation(),work.deadline()).map_err(|_|"Original exact Save Preview unavailable")?),
                WorkflowEditStepV9::Stage|WorkflowEditStepV9::Commit=>{
                    let step=if request.step()==WorkflowEditStepV9::Stage{crate::git::workflow_edit_v9::WorkflowLocalStepV9::Stage}else{crate::git::workflow_edit_v9::WorkflowLocalStepV9::Commit};
                    let preview=WorkflowLocalPlanPreviewV9::prepare_for_form_v9(request,provider,database,work.cancellation(),work.deadline()).map_err(|_|"Original exact Git LocalPlan Preview unavailable")?;
                    if step==crate::git::workflow_edit_v9::WorkflowLocalStepV9::Stage{WorkflowLocalPreviewV9::Stage(preview)}else{WorkflowLocalPreviewV9::Commit(preview)}
                }
                WorkflowEditStepV9::PublishBranch|WorkflowEditStepV9::CreatePr=>WorkflowLocalPreviewV9::Remote(WorkflowRemotePreviewV9::prepare_for_form_v9(request,provider,database,work.cancellation(),work.deadline()).map_err(|_|"Original remote source/member/ref/transport Preview unavailable; no write or fallback")?),
            };
            // Request includes current raw fields, prior Preview and actual
            // opened projection. Charge simultaneous old+new material before
            // accepting this result; no source/provider cap is increased.
            if request
                .owned_bytes()
                .checked_add(preview.retained_bytes())
                .is_none_or(|n| n > 16 * 1024 * 1024)
            {
                return Err("Workflow prepared material exceeds original peak bound".into());
            }
            WorkflowEditOutcomeV9::Preview(preview)
        }
        WorkflowEditWorkV9::Form {
            request,
            confirm: Some(confirm),
        } => {
            if request.phase() != WorkflowEditPhaseV9::Confirm {
                return Err("Original explicit W9 Confirm required".into());
            }
            if let WorkflowEditConfirmV9::Remote(confirm) = confirm {
                if !confirm.cancellation().same_instance(work.cancellation())
                    || confirm.deadline() != work.deadline()
                {
                    return Err("Original remote Confirm token/queued deadline differs".into());
                }
                let fence = owner
                    .publication_v3
                    .lock()
                    .map_err(|_| "Original remote fence unavailable")?
                    .take()
                    .ok_or("Original remote fence missing")?;
                let result=confirm.execute(provider,database,drafts.ok_or("Original workflow draft client unavailable")?,fence).map_err(|_|"Original remote workflow attempt unavailable; protected marker must be inspected before another attempt")?;
                work.check()?;
                return Ok(PreparedWorkflowEditV9 {
                    work: work.clone(),
                    publication: publication.clone(),
                    outcome: WorkflowEditOutcomeV9::RemoteAttempt(Box::new(result)),
                });
            }
            let receipt = match confirm {
                WorkflowEditConfirmV9::Worktree(c) => match c
                    .execute(provider, database)
                    .map_err(|_| "Original isolation attempt unavailable; inspect actual state")?
                {
                    WorkflowWorktreeOutcomeV9::Isolated(r) => WorkflowLocalReceiptV9::Isolated(r),
                    WorkflowWorktreeOutcomeV9::InspectionRequired => {
                        WorkflowLocalReceiptV9::InspectionRequired
                    }
                },
                WorkflowEditConfirmV9::Save(c) => match c
                    .execute(provider, database)
                    .map_err(|_| "Original Save attempt unavailable; inspect actual state")?
                {
                    WorkflowSaveOutcomeV9::Saved(r) => WorkflowLocalReceiptV9::Saved(r),
                    WorkflowSaveOutcomeV9::NotSaved | WorkflowSaveOutcomeV9::InspectionRequired => {
                        WorkflowLocalReceiptV9::InspectionRequired
                    }
                },
                WorkflowEditConfirmV9::Stage(c) => match c
                    .execute(provider, database)
                    .map_err(|_| "Original Stage attempt unavailable; inspect actual state")?
                {
                    WorkflowStageOutcomeV9::Staged(r) => WorkflowLocalReceiptV9::Staged(r),
                    WorkflowStageOutcomeV9::InspectionRequired => {
                        WorkflowLocalReceiptV9::InspectionRequired
                    }
                },
                WorkflowEditConfirmV9::Commit(c) => match c
                    .execute(provider, database)
                    .map_err(|_| "Original Commit attempt unavailable; inspect actual state")?
                {
                    WorkflowCommitOutcomeV9::Committed(r) => WorkflowLocalReceiptV9::Committed(r),
                    WorkflowCommitOutcomeV9::InspectionRequired => {
                        WorkflowLocalReceiptV9::InspectionRequired
                    }
                },
                WorkflowEditConfirmV9::Remote(_) => {
                    return Err("Original remote attempt cannot use a local-step receipt".into())
                }
            };
            WorkflowEditOutcomeV9::Receipt(receipt)
        }
    };
    work.check()?;
    Ok(PreparedWorkflowEditV9 {
        work: work.clone(),
        publication: publication.clone(),
        outcome,
    })
}
pub(crate) fn publish(
    app: &mut App,
    owner: &Arc<ScopedActionsOwnerV1>,
    prepared: PreparedWorkflowEditV9,
) -> bool {
    if !owner.owns(app) || !prepared.matches(owner.work()) || owner.work().check().is_err() {
        return false;
    }
    if matches!(&*prepared.publication, WorkflowEditWorkV9::Open { .. })
        && owner
            .workflow_edit_read_v9
            .lock()
            .ok()
            .is_none_or(|slot| slot.as_ref().is_none_or(|cutoff| !cutoff.current()))
    {
        return false;
    }
    match prepared.outcome {
        WorkflowEditOutcomeV9::Open(form) => {
            app.form = Some(*form);
            app.overlay = Some(crate::app::Overlay::Form);
            app.form_dismissed = false;
            app.status="Original local workflow editor ready • editing/opening is read only • Preview each exact step".into();
        }
        WorkflowEditOutcomeV9::DraftReadFailed(outcome) => {
            if let WorkflowEditWorkV9::Open {
                returning: Some(old),
                ..
            } = &*prepared.publication
            {
                // Repeat exact admission immediately before ANY body/status
                // effects. An independently newer genuine Form is untouched.
                if app.overlay.is_some()
                    || !app.form_dismissed
                    || app.form.as_ref().is_none_or(|form| !old.matches(form))
                {
                    return false;
                }
                match outcome {
                    WorkflowDraftReadFailureV9::RestoreExactRetained => {
                        app.overlay = Some(crate::app::Overlay::Form);
                        app.form_dismissed = false;
                        if let Some(form) = app.form.as_mut() {
                            form.error="Actual accepted workflow draft save failed; exact unsaved editable body retained. New explicit input may retry autosave; nothing was replayed.".into();
                        }
                    }
                    WorkflowDraftReadFailureV9::RetireInvalidRetained => {
                        app.form = None;
                    }
                    WorkflowDraftReadFailureV9::Unavailable => {}
                }
            }
            app.status="Original workflow draft read unavailable • accepted is not durable • no Core/GH/open/replay".into();
        }
        WorkflowEditOutcomeV9::Preview(preview) => {
            let Some(request) = prepared.publication.request() else {
                return false;
            };
            let Some(form) = app.form.as_mut() else {
                return false;
            };
            if !edit_ui::install_preview(form, request, preview) {
                return false;
            }
            app.status="Inspect complete exact local-step Preview; explicit Confirm only • no automatic retry".into();
        }
        WorkflowEditOutcomeV9::Receipt(receipt) => {
            let Some(request) = prepared.publication.request() else {
                return false;
            };
            let Some(form) = app.form.as_mut() else {
                return false;
            };
            if !edit_ui::install_receipt(form, request, receipt) {
                return false;
            }
            app.status="Original local step finished • inspect actual receipt/files/Git • remote publication not inferred".into();
        }
        WorkflowEditOutcomeV9::RemoteAttempt(result) => {
            if let Some(failure) = result.cleanup_failure_v9() {
                // The SAME applicable owner/source/Form gate remains inside
                // fail. This fixed local-ack status neither displays private
                // material nor fabricates an account-loss diagnosis.
                if !super::fail(app, owner) {
                    return false;
                }
                app.status = failure.to_string();
                return true;
            }
            if !result.private_visible_v9() {
                return super::fail(app, owner);
            }
            let Some(request) = prepared.publication.request() else {
                return false;
            };
            let Some(form) = app.form.as_mut() else {
                return false;
            };
            if !edit_ui::install_remote_result_v9(form, request, *result) {
                return false;
            }
            app.status="Actual original workflow remote receipt observed and durably retained; no automatic resend or inferred workflow success".into();
        }
    }
    app.hits.clear();
    true
}
pub(crate) fn observe_cleanup_wait_v9(app: &mut App) {
    let Some(owner) = app.scoped_actions_owner_v1.as_ref() else {
        return;
    };
    if !owner.owns(app) || owner.work().check().is_err() {
        return;
    }
    let Some(PublicationWorkV3::WorkflowEdit(publication)) = owner.work().publication_v3.as_deref()
    else {
        return;
    };
    let WorkflowEditWorkV9::Form {
        request,
        confirm: Some(WorkflowEditConfirmV9::Remote(confirm)),
    } = &**publication
    else {
        return;
    };
    if request.phase() == WorkflowEditPhaseV9::Confirm
        && publication.matches_native_work_v9(owner.work())
        && publication.overlay_current(app)
        && app
            .form
            .as_ref()
            .is_some_and(|form| form.submitting && request.matches_form(form))
        && confirm.cleanup_waiting_v9()
        && app.status != "Cleanup accepted • ACK pending • NEVER resend"
    {
        // Observation only, no IO, slot transition, approval or source update.
        app.status = "Cleanup accepted • ACK pending • NEVER resend".into();
    }
}
pub(crate) fn retire_pending(app: &mut App) -> bool {
    let Some(work) = current_retirement_work_v3(app).cloned() else {
        return false;
    };
    let Some(PublicationWorkV3::WorkflowEdit(publication)) = work.publication_v3.as_deref() else {
        return false;
    };
    if !publication.matches_native_work_v9(&work)
        || !Arc::ptr_eq(publication.intent().source(), &work.source)
        || !Arc::ptr_eq(publication.intent().page(), &work.page)
    {
        return true;
    }
    if app.overlay.is_none() && matches!(&**publication, WorkflowEditWorkV9::Open { .. }) {
        cancel_current(app);
        return true;
    }
    let Some(request) = publication.request().cloned() else {
        return true;
    };
    if app.overlay != Some(crate::app::Overlay::Form)
        || app.form_dismissed
        || app
            .form
            .as_ref()
            .is_none_or(|form| !form.submitting || !request.matches_retirement_form_v9(form))
    {
        return true;
    }
    let may_have_effect = matches!(
        &**publication,
        WorkflowEditWorkV9::Form {
            confirm: Some(_),
            ..
        }
    );
    let accepted_without_ack = matches!(
        &**publication,
        WorkflowEditWorkV9::Form {
            confirm: Some(WorkflowEditConfirmV9::Remote(confirm)),
            ..
        } if confirm.cleanup_waiting_v9()
    );
    if let Some(form) = app.form.as_mut() {
        edit_ui::retire_for_cancel(form, &request, may_have_effect);
    }
    cancel_current(app);
    app.status = if accepted_without_ack {
        crate::drafts::WorkflowCleanupFailureV9::Unknown.to_string()
    } else if may_have_effect {
        "Original local attempt logically canceled • effects may exist • inspect; no retry/rollback"
            .into()
    } else {
        "Original local preparation canceled • exact private text/source retained • no renewed purpose".into()
    };
    true
}
