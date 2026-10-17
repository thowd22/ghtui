//! Closed publication variants on the SAME existing Actions Work/result/owner.
//! No new queue, scope issuer, mutable DTO source or foreground body clone.
use super::*;
use crate::{
    actions_controller::ActionsExecution,
    actions_workspace::{ActionsCommand, ActionsControl},
    drafts::DraftClient,
    forms::{Form, FormPurpose},
    scoped_actions_publication_ui_v3::{
        self as ui, ComposerPhaseV3, ScopedActionsComposerRequestV3,
    },
    scoped_actions_publication_v3::{
        self as source, ScopedActionsConfirmV3, ScopedActionsHistoryV3, ScopedActionsIntentV3,
        ScopedActionsPreviewV3,
    },
};
use std::path::Path;
#[cfg(test)]
#[path = "scoped_actions_publication_cancel_v3_tests.rs"]
mod cancel_tests;
#[cfg(test)]
#[path = "scoped_actions_control_v5_tests.rs"]
pub(super) mod control_tests;
#[path = "scoped_actions_dispatch_native_v7.rs"]
pub(crate) mod dispatch_v7;
#[cfg(test)]
#[path = "scoped_actions_rerun_v4_tests.rs"]
mod rerun_tests;
#[cfg(test)]
#[path = "scoped_actions_publication_v3_tests.rs"]
mod tests;
#[path = "scoped_actions_workflow_edit_native_v9.rs"]
pub(crate) mod workflow_edit_v9;
#[path = "scoped_actions_workflow_native_v6.rs"]
pub(crate) mod workflow_v6;

pub(crate) enum PublicationWorkV3 {
    WorkflowEdit(Arc<workflow_edit_v9::WorkflowEditWorkV9>),
    Dispatch(Arc<dispatch_v7::DispatchWorkV7>),
    Workflow(Arc<workflow_v6::WorkflowWorkV6>),
    Open(Arc<ScopedActionsIntentV3>),
    Form {
        request: Arc<ScopedActionsComposerRequestV3>,
        confirm: Option<Arc<ScopedActionsConfirmV3>>,
    },
}
impl PublicationWorkV3 {
    fn intent(&self) -> Option<&Arc<ScopedActionsIntentV3>> {
        match self {
            Self::Open(intent) => Some(intent),
            Self::Form { request, .. } => Some(request.opened().intent()),
            Self::Workflow(_) | Self::Dispatch(_) | Self::WorkflowEdit(_) => None,
        }
    }
    fn source(&self) -> &Arc<ScopedActionsSourceV1> {
        match self {
            Self::WorkflowEdit(work) => work.intent().source(),
            Self::Dispatch(work) => work.intent().source(),
            Self::Workflow(work) => work.intent().source(),
            _ => self.intent().expect("closed Run variant").source(),
        }
    }
    fn page(&self) -> &Arc<ScopedActionsPageV1> {
        match self {
            Self::WorkflowEdit(work) => work.intent().page(),
            Self::Dispatch(work) => work.page(),
            Self::Workflow(work) => work.intent().page(),
            _ => self.intent().expect("closed Run variant").page(),
        }
    }
    fn matches_form(&self, form: &Form) -> bool {
        match self {
            Self::WorkflowEdit(work) => work.matches_form(form),
            Self::Dispatch(work) => work.matches_form(form),
            Self::Workflow(work) => work.request().is_some_and(|r| r.matches_form(form)),
            _ => self.request().is_some_and(|r| r.matches_form(form)),
        }
    }
    fn confirm_origin(&self) -> Option<(&crate::storage::Scope, &str)> {
        match self {
            Self::WorkflowEdit(_) => None, // Local steps never consume remote publication approval.
            Self::Dispatch(work) => work.confirm().map(|confirm| {
                let intent = confirm.preview().opened().intent();
                (intent.source().original().scope(), intent.key())
            }),
            Self::Workflow(work) => work.confirm().map(|c| {
                let intent = c.preview().opened().intent();
                (intent.source().original().scope(), intent.key())
            }),
            _ => self.confirm().map(|c| {
                let intent = c.preview().opened().intent();
                (intent.source().original().scope(), intent.key())
            }),
        }
    }
    fn request(&self) -> Option<&Arc<ScopedActionsComposerRequestV3>> {
        match self {
            Self::Form { request, .. } => Some(request),
            _ => None,
        }
    }
    fn confirm(&self) -> Option<&Arc<ScopedActionsConfirmV3>> {
        match self {
            Self::Form { confirm, .. } => confirm.as_ref(),
            _ => None,
        }
    }
}
pub enum PreparedResultV3 {
    WorkflowEdit(Box<workflow_edit_v9::PreparedWorkflowEditV9>),
    Dispatch(Box<dispatch_v7::PreparedDispatchV7>),
    Workflow(Box<workflow_v6::PreparedWorkflowV6>),
    Read(Box<PreparedScopedActionsV1>),
    Publication(PreparedPublicationV3),
    Journal(Box<super::journal_v5::PreparedJournalV5>),
}
pub struct PreparedPublicationV3 {
    work: Arc<ScopedActionsWorkV1>,
    publication: Arc<PublicationWorkV3>,
    outcome: PublicationOutcomeV3,
}
enum PublicationOutcomeV3 {
    Open(Box<Form>),
    Preview(Arc<ScopedActionsPreviewV3>),
    Attempt(Box<crate::actions_controller::RunExecutionV8>),
    History(Arc<ScopedActionsHistoryV3>),
}
#[cfg(test)]
impl PreparedPublicationV3 {
    pub(crate) fn attempted_for_test_v107(
        &self,
    ) -> Option<&crate::actions_controller::RunExecutionV8> {
        match &self.outcome {
            PublicationOutcomeV3::Attempt(attempt) => Some(attempt),
            _ => None,
        }
    }
}
impl PreparedResultV3 {
    pub(crate) fn matches(&self, work: &Arc<ScopedActionsWorkV1>) -> bool {
        match self {
            Self::WorkflowEdit(prepared) => prepared.matches(work),
            Self::Dispatch(prepared) => prepared.matches(work),
            Self::Workflow(prepared) => prepared.matches(work),
            Self::Read(read) => {
                work.journal_v5.is_none() && work.publication_v3.is_none() && read.matches(work)
            }
            Self::Journal(prepared) => work.publication_v3.is_none() && prepared.matches(work),
            Self::Publication(prepared) => {
                work.journal_v5.is_none()
                    && Arc::ptr_eq(&prepared.work, work)
                    && work.kind == ScopedActionsRequestKindV1::Publication
                    && work
                        .publication_v3
                        .as_ref()
                        .is_some_and(|current| Arc::ptr_eq(current, &prepared.publication))
                    && match (&*prepared.publication, &prepared.outcome) {
                        (PublicationWorkV3::Open(intent), PublicationOutcomeV3::Open(form)) => {
                            matches!(&form.purpose,FormPurpose::ScopedActionsV3(state)if Arc::ptr_eq(state.opened().intent(),intent))
                        }
                        (
                            PublicationWorkV3::Form {
                                request,
                                confirm: None,
                            },
                            PublicationOutcomeV3::Preview(preview),
                        ) => {
                            request.phase() == ComposerPhaseV3::Preview
                                && Arc::ptr_eq(request.opened(), preview.opened())
                                && request.options_v4() == preview.options()
                        }
                        (
                            PublicationWorkV3::Form {
                                request,
                                confirm: Some(confirm),
                            },
                            PublicationOutcomeV3::Attempt(result),
                        ) => {
                            result.matches_history_v8()
                                && request.phase() == ComposerPhaseV3::Confirm
                                && request
                                    .preview()
                                    .is_some_and(|preview| Arc::ptr_eq(preview, confirm.preview()))
                        }
                        (
                            PublicationWorkV3::Form {
                                request,
                                confirm: None,
                            },
                            PublicationOutcomeV3::History(history),
                        ) => {
                            matches!(
                                request.phase(),
                                ComposerPhaseV3::Inspect
                                    | ComposerPhaseV3::Acknowledge
                                    | ComposerPhaseV3::AcknowledgeAcceptanceV5
                            ) && request.reference() == Some(&history.reference())
                        }
                        _ => false,
                    }
            }
        }
    }
}
pub(crate) fn overlay_current(app: &App, work: &Arc<ScopedActionsWorkV1>) -> bool {
    if let Some(current) = super::journal_v5::overlay_current(app, work) {
        return current;
    }
    match work.publication_v3.as_deref() {
        Some(PublicationWorkV3::WorkflowEdit(work)) => work.overlay_current(app),
        Some(PublicationWorkV3::Dispatch(work)) => work.overlay_current(app),
        Some(PublicationWorkV3::Workflow(work)) => work.overlay_current(app),
        None | Some(PublicationWorkV3::Open(_)) => app.overlay.is_none(),
        Some(PublicationWorkV3::Form { request, .. }) => {
            app.overlay == Some(crate::app::Overlay::Form)
                && !app.form_dismissed
                && app
                    .form
                    .as_ref()
                    .is_some_and(|form| request.matches_form(form))
        }
    }
}
pub(crate) fn foreign_intent(app: &App, work: &Arc<ScopedActionsWorkV1>) -> bool {
    newer_foreign_without_form_v3(app)
        || app.form.as_ref().is_some_and(|form| {
            (form.preview_requested || form.submit_requested)
                && !work
                    .publication_v3
                    .as_ref()
                    .is_some_and(|publication| publication.matches_form(form))
        })
}
pub(crate) fn form_source_current(app: &App, intent: &ScopedActionsIntentV3) -> bool {
    app.view == View::Actions
        && app.scoped_actions_v1.as_ref().is_some_and(|state| {
            current(app, state)
                && Arc::ptr_eq(&state.source, intent.source())
                && Arc::ptr_eq(&state.page, intent.page())
        })
}
fn capture_work(
    page: Arc<ScopedActionsPageV1>,
    epoch: u64,
    publication: PublicationWorkV3,
) -> Result<Arc<ScopedActionsWorkV1>, String> {
    let mut work = ScopedActionsWorkV1::capture(page, &ActionsAction::Refresh, epoch)?;
    let inner = Arc::get_mut(&mut work).ok_or("New original work unexpectedly aliased")?;
    inner.kind = ScopedActionsRequestKindV1::Publication;
    inner.publication_v3 = Some(Arc::new(publication));
    inner.query_v2 = None;
    inner.resource_v2 = None;
    inner.search_v2 = None;
    Ok(work)
}
/// Initial PR run entry: capture the SAME original prequeue purpose before
/// installing its retained source/page. No App effects or provider reads here.
pub(super) fn prepare_initial_run_v4(
    page: Arc<ScopedActionsPageV1>,
    command: ActionsCommand,
) -> Result<Arc<ScopedActionsWorkV1>, String> {
    let intent = ScopedActionsIntentV3::capture_command_v4(page.clone(), 1, command)?;
    capture_work(page, 1, PublicationWorkV3::Open(intent))
}
pub(crate) fn intercept(app: &mut App, action: &ActionsAction) -> bool {
    if dispatch_v7::intercept(app, action) {
        return true;
    }
    if workflow_v6::intercept(app, action) {
        return true;
    }
    let ActionsAction::Compose(command) = action else {
        return false;
    };
    if !matches!(
        command,
        ActionsCommand::Cancel
            | ActionsCommand::ForceCancel
            | ActionsCommand::DeleteRun
            | ActionsCommand::DeleteLogs
            | ActionsCommand::RerunAll
            | ActionsCommand::RerunFailed
            | ActionsCommand::RerunJob
    ) || app.scoped_actions_v1.is_none()
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
    let requested = app
        .scoped_actions_v1
        .as_ref()
        .filter(|state| allowed && current(app, state))
        .ok_or_else(|| "Original current Actions run required".to_owned())
        .and_then(|state| {
            let epoch = state.epoch.wrapping_add(1);
            let intent = if *command == ActionsCommand::Cancel {
                ScopedActionsIntentV3::capture(state.page.clone(), epoch)
            } else {
                ScopedActionsIntentV3::capture_command_v4(state.page.clone(), epoch, *command)
            }?;
            capture_work(state.page.clone(), epoch, PublicationWorkV3::Open(intent))
        });
    match requested {
        Ok(work)=>{
            let state=app.scoped_actions_v1.as_mut().expect("pure actual current preflight");
            if let Some(watch)=state.watch_v2.take(){watch.stop();}
            state.epoch=work.epoch;state.editor_v2=None;
            app.scoped_actions_work_v1=Some(work);app.hits.clear();
            app.status=if *command==ActionsCommand::Cancel {"Opening original Cancel intent • read only • original30s includes queue • no POST".into()}
                else{format!("Opening original {} intent • read only • original30s includes queue • no POST",command.label())};
        }
        Err(_)=>explain(app,"Original Cancel source/page is unavailable or another intent is current; no work or cancellation"),
    }
    true
}
/// Explicit Preview/Confirm/Inspect/F4 only. Validate actual Form/source/flags
/// BEFORE consuming flags, stopping Watch, epoch or any other visible effect.
pub fn capture_form_work(app: &mut App) -> bool {
    if workflow_edit_v9::capture_form_work(app) {
        return true;
    }
    if dispatch_v7::capture_form_work(app) {
        return true;
    }
    if workflow_v6::capture_form_work(app) {
        return true;
    }
    if app.overlay != Some(crate::app::Overlay::Form)
        || app.view != View::Actions
        || app.form_dismissed
    {
        return false;
    }
    let Some(form) = app.form.as_ref() else {
        return false;
    };
    let FormPurpose::ScopedActionsV3(state) = &form.purpose else {
        return false;
    };
    let phase = if state.acceptance_requested_v5 {
        Some(ComposerPhaseV3::AcknowledgeAcceptanceV5)
    } else if state.acknowledge_requested {
        Some(ComposerPhaseV3::Acknowledge)
    } else if state.inspection_requested {
        Some(ComposerPhaseV3::Inspect)
    } else if form.submit_requested {
        Some(ComposerPhaseV3::Confirm)
    } else if form.preview_requested {
        Some(ComposerPhaseV3::Preview)
    } else {
        None
    };
    let Some(phase) = phase else { return false };
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
        let request = ui::capture(form, phase)?;
        if phase == ComposerPhaseV3::Inspect
            && request.opened().intent().command() == ActionsCommand::DeleteRun
            && (request.owned_history_v8().is_none()
                || request.opened().intent().source().pr_origin_v2().is_none())
        {
            // Preserve the existing numeric-ID repository history surface.
            // Only the retained PR-origin Form needs its actual own-result
            // witness to substitute the Actions portion of the old PR guard.
            return super::journal_v5::capture_deleted_form(app, request);
        }
        let state = app
            .scoped_actions_v1
            .as_ref()
            .ok_or("Original Actions unavailable")?;
        let epoch = state.epoch.wrapping_add(1);
        let mut work = capture_work(
            state.page.clone(),
            epoch,
            PublicationWorkV3::Form {
                request: request.clone(),
                confirm: None,
            },
        )?;
        if phase == ComposerPhaseV3::Confirm {
            let confirm = ScopedActionsConfirmV3::capture(
                request
                    .preview()
                    .ok_or("Fresh original Preview missing")?
                    .clone(),
                work.cancellation(),
                work.deadline(),
            )?;
            let inner = Arc::get_mut(&mut work).ok_or("New work unexpectedly aliased")?;
            inner.publication_v3 = Some(Arc::new(PublicationWorkV3::Form {
                request,
                confirm: Some(confirm),
            }));
        }
        Ok(work)
    })();
    match captured {
        Ok(work) => {
            let form = app.form.as_mut().expect("actual captured Form");
            form.preview_requested = false;
            form.submit_requested = false;
            form.submitting = true;
            if let FormPurpose::ScopedActionsV3(state) = &mut form.purpose {
                state.inspection_requested = false;
                state.acknowledge_requested = false;
                state.acceptance_requested_v5 = false;
            }
            let state = app
                .scoped_actions_v1
                .as_mut()
                .expect("actual current source");
            if let Some(watch) = state.watch_v2.take() {
                watch.stop();
            }
            state.epoch = work.epoch;
            app.scoped_actions_work_v1 = Some(work);
            app.hits.clear();
            let command = app.form.as_ref().and_then(|form| {
                if let FormPurpose::ScopedActionsV3(state) = &form.purpose {
                    Some(state.opened().intent().command())
                } else {
                    None
                }
            });
            let label = command.map_or("Actions", |command| {
                if command == ActionsCommand::Cancel {
                    "Cancel"
                } else {
                    command.label()
                }
            });
            app.status=match phase{ComposerPhaseV3::Confirm=>format!("Explicit original {label} attempt queued • may have effects after invocation • no retry"),ComposerPhaseV3::Preview=>format!("Preparing exact original {label} Preview • no remote mutation"),ComposerPhaseV3::Inspect=>"Inspecting original protected result • no resend".into(),ComposerPhaseV3::Acknowledge=>"Explicit original result ACK queued • no remote mutation".into(),ComposerPhaseV3::AcknowledgeAcceptanceV5=>"Explicit inspected acceptance ACK queued • effect unverified • no remote mutation".into()};
        }
        Err(reason) => {
            let form = app.form.as_mut().expect("actual own Form");
            form.preview_requested = false;
            form.submit_requested = false;
            if let FormPurpose::ScopedActionsV3(state) = &mut form.purpose {
                state.inspection_requested = false;
                state.acknowledge_requested = false;
                state.acceptance_requested_v5 = false;
            }
            form.error = reason;
        }
    }
    true
}
impl ScopedActionsOwnerV1 {
    pub(crate) fn new_publication_v3(
        work: Arc<ScopedActionsWorkV1>,
        database: &Path,
        drafts: &DraftClient,
    ) -> Result<Arc<Self>, String> {
        work.check()?;
        let fence = if let Some((scope, key)) = work
            .publication_v3
            .as_ref()
            .and_then(|work| work.confirm_origin())
        {
            Some(
                drafts
                    .capture_publication_v2(
                        database,
                        scope,
                        key,
                        work.cancellation(),
                        work.deadline(),
                    )
                    .map_err(|_| "Original draft publication cutoff unavailable")?,
            )
        } else {
            None
        };
        Ok(Arc::new(Self {
            work,
            publication_v3: std::sync::Mutex::new(fence),
            workflow_edit_read_v9: std::sync::Mutex::new(None),
        }))
    }
}
pub(crate) fn execute(
    owner: &Arc<ScopedActionsOwnerV1>,
    provider: &crate::providers::gh::GithubProvider,
    database: &Path,
    drafts: Option<&DraftClient>,
) -> Result<PreparedResultV3, String> {
    let work = owner.work();
    work.check()?;
    if work.journal_v5.is_some() {
        return super::journal_v5::execute(work, provider, database)
            .map(|value| PreparedResultV3::Journal(Box::new(value)));
    }
    let Some(publication) = &work.publication_v3 else {
        return crate::scoped_actions_v1::execute(work.clone(), provider, database)
            .map(Box::new)
            .map(PreparedResultV3::Read);
    };
    if let PublicationWorkV3::WorkflowEdit(publication) = &**publication {
        return workflow_edit_v9::execute(owner, publication, provider, database, drafts)
            .map(Box::new)
            .map(PreparedResultV3::WorkflowEdit);
    }
    if let PublicationWorkV3::Dispatch(publication) = &**publication {
        return dispatch_v7::execute(owner, publication, provider, database, drafts)
            .map(Box::new)
            .map(PreparedResultV3::Dispatch);
    }
    if let PublicationWorkV3::Workflow(publication) = &**publication {
        return workflow_v6::execute(owner, publication, provider, database, drafts)
            .map(Box::new)
            .map(PreparedResultV3::Workflow);
    }
    if work.kind != ScopedActionsRequestKindV1::Publication
        || !Arc::ptr_eq(&work.source, publication.source())
        || !Arc::ptr_eq(&work.page, publication.page())
        || matches!(&**publication,PublicationWorkV3::Open(intent) if intent.epoch()!=work.epoch)
    {
        return Err("Original typed Actions publication context differs".into());
    }
    let outcome = match &**publication {
        PublicationWorkV3::WorkflowEdit(_) => {
            return Err("Workflow edit has its SAME closed typed worker; no legacy fallback".into())
        }
        PublicationWorkV3::Dispatch(_) => {
            return Err("Dispatch purpose must use SAME closed worker".into())
        }
        PublicationWorkV3::Workflow(_) => {
            return Err("Workflow has its closed typed worker branch".into())
        }
        PublicationWorkV3::Open(intent) => PublicationOutcomeV3::Open(Box::new(ui::form(
            source::open(
                intent.clone(),
                provider,
                database,
                work.cancellation(),
                work.deadline(),
            )
            .map_err(|_| "Original Cancel open unavailable")?,
        )?)),
        PublicationWorkV3::Form { request, confirm } => match request.phase() {
            ComposerPhaseV3::Preview if confirm.is_none() => PublicationOutcomeV3::Preview(
                (if request.opened().intent().command() == ActionsCommand::Cancel {
                    source::preview(
                        request.opened().clone(),
                        provider,
                        database,
                        work.cancellation(),
                        work.deadline(),
                    )
                } else {
                    source::preview_with_options_v4(
                        request.opened().clone(),
                        request.options_v4(),
                        provider,
                        database,
                        work.cancellation(),
                        work.deadline(),
                    )
                })
                .map_err(|_| "Original Cancel Preview unavailable")?,
            ),
            ComposerPhaseV3::Confirm => {
                let confirm = confirm
                    .as_ref()
                    .ok_or("Original explicit Confirm missing")?;
                if !confirm.cancellation().same_instance(work.cancellation())
                    || confirm.deadline() != work.deadline()
                {
                    return Err("Original Confirm token/deadline differs".into());
                }
                let fence = owner
                    .publication_v3
                    .lock()
                    .map_err(|_| "Original draft cutoff unavailable")?
                    .take()
                    .ok_or("Original explicit Confirm cutoff already consumed")?;
                PublicationOutcomeV3::Attempt(Box::new(
                    crate::actions_controller::execute_scoped_run_v3(
                        confirm,
                        provider,
                        database,
                        drafts.ok_or("Original Draft writer unavailable")?,
                        fence,
                    ),
                ))
            }
            ComposerPhaseV3::Inspect
            | ComposerPhaseV3::Acknowledge
            | ComposerPhaseV3::AcknowledgeAcceptanceV5
                if confirm.is_none() =>
            {
                PublicationOutcomeV3::History(
                    source::inspect_with_acceptance_v5(
                        request.opened(),
                        request.reference().ok_or("Original reference missing")?,
                        request.history(),
                        request.phase() != ComposerPhaseV3::Inspect,
                        request.phase() == ComposerPhaseV3::AcknowledgeAcceptanceV5,
                        provider,
                        database,
                        work.cancellation(),
                        work.deadline(),
                        request.owned_history_v8(),
                    )
                    .map_err(|_| "Original result inspection/ACK unavailable")?,
                )
            }
            _ => return Err("Original Actions phase/result differs".into()),
        },
    };
    #[cfg(test)]
    if let Some(fault) = work.after_read_fault_v1 {
        fault();
    }
    work.check()?;
    Ok(PreparedResultV3::Publication(PreparedPublicationV3 {
        work: work.clone(),
        publication: publication.clone(),
        outcome,
    }))
}
fn retired_underlay(app: &mut App) {
    if let Some(state) = &mut app.scoped_actions_v1 {
        if let Some(watch) = state.watch_v2.take() {
            watch.stop();
        }
        state.matches_v2 = None;
    }
    if let Some(workspace) = &mut app.actions {
        workspace.rows.clear();
        workspace.selected = 0;
        workspace.top = 0;
        workspace.watching = false;
        workspace.controls = vec![
            ActionsControl {
                label: "[r Refresh actual run]".into(),
                action: ActionsAction::Refresh,
                disabled_reason: None,
            },
            ActionsControl {
                label: "[b Back]".into(),
                action: ActionsAction::Back,
                disabled_reason: None,
            },
        ];
    }
}
pub(crate) fn publish(
    app: &mut App,
    owner: &Arc<ScopedActionsOwnerV1>,
    prepared: PreparedResultV3,
) -> bool {
    if !owner.owns(app) || !prepared.matches(owner.work()) || owner.work.check().is_err() {
        return false;
    }
    let prepared = match prepared {
        PreparedResultV3::WorkflowEdit(prepared) => {
            return workflow_edit_v9::publish(app, owner, *prepared)
        }
        PreparedResultV3::Dispatch(prepared) => return dispatch_v7::publish(app, owner, *prepared),
        PreparedResultV3::Workflow(prepared) => return workflow_v6::publish(app, owner, *prepared),
        PreparedResultV3::Read(read) => return super::publish(app, owner, *read),
        PreparedResultV3::Journal(prepared) => {
            return super::journal_v5::publish(app, owner, *prepared)
        }
        PreparedResultV3::Publication(prepared) => prepared,
    };
    match prepared.outcome {
        PublicationOutcomeV3::Open(form) => {
            let command = prepared
                .publication
                .intent()
                .expect("typed actual Run outcome")
                .command();
            app.form = Some(*form);
            app.form_dismissed = false;
            app.overlay = Some(crate::app::Overlay::Form);
            app.status = if command == ActionsCommand::Cancel {
                "Original Cancel form ready • no write • Preview then explicit Confirm".into()
            } else if matches!(
                command,
                ActionsCommand::RerunAll | ActionsCommand::RerunFailed | ActionsCommand::RerunJob
            ) {
                "Original rerun form ready • exact debug choice • Preview then explicit Confirm"
                    .into()
            } else {
                format!("Original {} intent ready • no write • inspect exact target/effect, Preview then explicit Confirm",command.label())
            };
        }
        PublicationOutcomeV3::Preview(preview) => {
            let request = prepared
                .publication
                .request()
                .expect("typed checked Preview");
            if !app
                .form
                .as_mut()
                .is_some_and(|form| ui::install_preview(form, request, preview))
            {
                return false;
            }
            app.status="Exact original run Preview ready • nothing sent • explicit Confirm is a NEW bounded attempt".into();
        }
        PublicationOutcomeV3::History(history) => {
            let request = prepared
                .publication
                .request()
                .expect("typed checked history");
            if !app
                .form
                .as_mut()
                .is_some_and(|form| ui::install_history(form, request, history))
            {
                return false;
            }
            app.status="Original result freshly inspected/acknowledged • nothing resent • no inferred cancellation cause".into();
        }
        PublicationOutcomeV3::Attempt(result) => {
            let (result, owned_history_v8) = result.into_parts();
            let command = prepared
                .publication
                .intent()
                .expect("typed actual Run outcome")
                .command();
            let rerun = matches!(
                command,
                ActionsCommand::RerunAll | ActionsCommand::RerunFailed | ActionsCommand::RerunJob
            );
            let label = command.label();
            let outcome_label = if rerun {
                "rerun"
            } else if command == ActionsCommand::Cancel {
                "Cancel"
            } else {
                label
            };
            let (reference,message,visible)=match result{
                ActionsExecution::Confirmed{publication,..}=>(Some(publication),if command==ActionsCommand::Cancel {"Original cancellation confirmed and durably recorded • no retry".into()}else{format!("Original {label} typed observed receipt durably recorded • no retry")},true),
                ActionsExecution::AcceptedPending{publication,..}=>(Some(publication),if rerun {"GitHub accepted rerun • workflow completion NOT confirmed • inspect observed attempt transition; do not resend".into()}else if command==ActionsCommand::DeleteRun {"GitHub accepted whole-run deletion (HTTP204) • deletion/causality NOT confirmed •404 means unavailable • inspect/ACK acceptance only; never resend".into()}else if command==ActionsCommand::DeleteLogs {"GitHub accepted whole-run log deletion (HTTP204) • effect UNVERIFIED • inspect then separate F6 ACK acceptance only; never resend".into()}else{format!("GitHub accepted {label} • completion NOT confirmed • inspect original result; do not resend")},true),
                ActionsExecution::Unknown{publication,private_visible,..}|ActionsExecution::ReceiptPersistenceFailed{publication,private_visible,..}=>(Some(publication),format!("Original {outcome_label} outcome unknown • exact protected marker retained • no automatic retry"),private_visible),
                ActionsExecution::EffectConfirmedButRefreshFailed{publication,private_visible,..}=>(Some(publication),"Original typed cancellation receipt retained • view unavailable • no resend".into(),private_visible),
                ActionsExecution::AcceptedButRefreshFailed{publication,private_visible,..}=>(Some(publication),"Original accepted receipt retained • completion NOT confirmed • no resend".into(),private_visible),
                ActionsExecution::Refused{private_visible,..}=>(None,format!("Original {outcome_label} was refused before invocation; any exact protected draft/marker remains • no retry"),private_visible),
                _=>(None,"Original Cancel result unavailable • protection retained • no retry".into(),false),
            };
            if !visible {
                return fail(app, owner);
            }
            retired_underlay(app);
            let request = prepared
                .publication
                .request()
                .expect("typed checked Confirm");
            let Some(form) = app.form.as_mut() else {
                return false;
            };
            if let Some(reference) = reference {
                if !ui::install_attempt(form, request, reference, message, owned_history_v8) {
                    return false;
                }
            } else {
                request.retire_busy(form);
                form.error = message;
            }
            app.status = if rerun {
                "Original rerun attempt finished • acceptance/observed transition only • durable protection retained".into()
            } else if command == ActionsCommand::Cancel {
                "Original Cancel attempt finished • durable result protection retained • actual run requires refresh".into()
            } else {
                format!("Original {label} attempt finished • typed acceptance/observed state only • durable protection retained")
            };
        }
    }
    app.hits.clear();
    true
}
pub(crate) fn submission_failed(app: &mut App, work: &Arc<ScopedActionsWorkV1>) -> bool {
    if super::journal_v5::submission_failed(app, work) {
        return true;
    }
    let Some(publication) = &work.publication_v3 else {
        return false;
    };
    if !overlay_current(app, work)
        || foreign_intent(app, work)
        || app.scoped_actions_v1.as_ref().is_none_or(|state| {
            !current(app, state)
                || state.epoch != work.epoch
                || !Arc::ptr_eq(&state.page, &work.page)
        })
    {
        work.cancellation().cancel();
        return true;
    }
    if let (Some(request), Some(form)) = (publication.request(), app.form.as_mut()) {
        request.retire_busy(form);
        form.error="Original Actions queue/draft cutoff unavailable; nothing submitted • inspect any protected history before another intent".into();
    }
    if let (PublicationWorkV3::Workflow(publication), Some(form)) =
        (&**publication, app.form.as_mut())
    {
        if let Some(request) = publication.request() {
            request.retire_busy(form);
            form.error = "Original workflow submission unavailable • no fallback or retry".into();
        }
    }
    if let (PublicationWorkV3::Dispatch(publication), Some(form)) =
        (&**publication, app.form.as_mut())
    {
        if let Some(request) = publication.request() {
            request.retire_busy(form);
            form.error = "Original Dispatch submission unavailable • no fallback or retry".into();
        }
    }
    if let (PublicationWorkV3::WorkflowEdit(publication), Some(form)) =
        (&**publication, app.form.as_mut())
    {
        if let Some(request) = publication.request() {
            request.retire_busy(form);
            form.error="Original workflow local step could not enter its original queue; exact draft retained, no operation/retry".into();
        }
    }
    work.cancellation().cancel();
    app.status = "Original Actions submission unavailable • no fallback or retry".into();
    app.hits.clear();
    true
}
pub(crate) fn fail(app: &mut App, owner: &Arc<ScopedActionsOwnerV1>) -> bool {
    if !owner.owns(app) {
        return false;
    }
    // Remote retirement removes this Form before returning. Remember its
    // actual ownership now so an invisible modal cannot keep consuming input
    // after the private payload has gone. A newer Form fails owner.owns above.
    let retire_form = app.overlay == Some(crate::app::Overlay::Form)
        && app.form.as_ref().is_some_and(ui::is_original);
    crate::repository_catalogue_native::privacy(app);
    crate::repository_catalogue_native::retire_remote_views(app);
    if retire_form {
        app.form = None;
        app.overlay = None;
        app.form_dismissed = true;
    }
    app.status="Original Actions operation unavailable • private source hidden • protected results preserved; nothing resent".into();
    app.hits.clear();
    true
}
/// Close/Resize are input retirement, not publication admission. They may
/// cancel an already cancelled/expired SAME request, but cannot adopt another
/// original Form merely because its enum variant is the same. In particular,
/// do not consult `is_original` as an identity or infer Confirm from a foreign
/// old owner. Every retained/queued token must be this exact current Form work
/// before ANY token, slot, source, privacy or presentation effect.
fn current_retirement_work_v3(app: &App) -> Option<&Arc<ScopedActionsWorkV1>> {
    if app.view != View::Actions || newer_foreign_without_form_v3(app) {
        return None;
    }
    let state = app.scoped_actions_v1.as_ref()?;
    if !current(app, state) {
        return None;
    }
    let work = match (
        app.scoped_actions_work_v1.as_ref(),
        app.scoped_actions_owner_v1.as_ref(),
    ) {
        (Some(work), Some(owner)) if Arc::ptr_eq(work, &owner.work) => work,
        (Some(_), Some(_)) | (None, None) => return None,
        (Some(work), None) => work,
        (None, Some(owner)) => &owner.work,
    };
    ((work.publication_v3.is_some() || work.journal_v5.is_some())
        && state.epoch == work.epoch
        && Arc::ptr_eq(&state.source, &work.source)
        && Arc::ptr_eq(&state.page, &work.page))
    .then_some(work)
}
fn applicable_cancel_v3(app: &App) -> Option<bool> {
    if app.overlay != Some(crate::app::Overlay::Form) || app.form_dismissed {
        return None;
    }
    let work = current_retirement_work_v3(app)?;
    let form = app.form.as_ref()?;
    if let Some(request) = super::journal_v5::form_request(work) {
        return request.matches_form(form).then_some(false);
    }
    if let Some(request) = super::journal_v5::workflow_form_request_v9(work) {
        return (request.matches(form)
            && Arc::ptr_eq(request.intent().source(), &work.source)
            && Arc::ptr_eq(request.intent().page(), &work.page))
        .then_some(false);
    }
    let publication = work.publication_v3.as_ref()?;
    if let PublicationWorkV3::Dispatch(publication) = &**publication {
        return (publication.matches_form(form)
            && Arc::ptr_eq(&work.source, publication.intent().source())
            && Arc::ptr_eq(&work.page, publication.page()))
        .then_some(publication.confirm().is_some());
    }
    if let PublicationWorkV3::Workflow(publication) = &**publication {
        return (publication.matches_form(form)
            && Arc::ptr_eq(&work.source, publication.intent().source())
            && Arc::ptr_eq(&work.page, publication.intent().page()))
        .then_some(publication.confirm().is_some());
    }
    let request = publication.request()?;
    let intent = request.opened().intent();
    (request.matches_form(form)
        && Arc::ptr_eq(&work.source, intent.source())
        && Arc::ptr_eq(&work.page, intent.page()))
    .then_some(publication.confirm().is_some())
}
pub(crate) fn cancel(app: &mut App) {
    if workflow_edit_v9::retire_pending(app) {
        return;
    }
    let Some(confirm) = applicable_cancel_v3(app) else {
        return;
    };
    if let Some(work) = app.scoped_actions_work_v1.take() {
        work.cancellation().cancel();
    }
    if let Some(owner) = &app.scoped_actions_owner_v1 {
        owner.cancellation().cancel();
    }
    app.scoped_actions_cancel_v1 = true;
    if confirm {
        crate::repository_catalogue_native::privacy(app);
        crate::repository_catalogue_native::retire_remote_views(app);
        app.form = None;
        app.overlay = None;
        app.status="Original Cancel attempt logically canceled • effects may exist • protected reference retained; no retry".into();
    } else {
        if let Some(state) = &mut app.scoped_actions_v1 {
            state.epoch = state.epoch.wrapping_add(1);
        }
        if let Some(form) = &mut app.form {
            form.submitting = false;
            form.invalidate_plan();
        }
        app.status="Original Actions preparation canceled • retained source • no mutation or renewed purpose".into();
    }
    app.hits.clear();
}
pub(crate) fn resize(app: &mut App) -> bool {
    if workflow_edit_v9::retire_pending(app) {
        return true;
    }
    if dispatch_v7::run_read_v7::resize_pending(app) {
        return true;
    }
    let pending = app.scoped_actions_owner_v1.as_ref().is_some_and(|owner| {
        owner.work.publication_v3.is_some() || owner.work.journal_v5.is_some()
    }) || app
        .scoped_actions_work_v1
        .as_ref()
        .is_some_and(|work| work.publication_v3.is_some() || work.journal_v5.is_some());
    if pending {
        // A nonmatching publication is NOT a generic read that the caller may
        // retire as a fallback. Claim this routing branch even on pure refusal
        // so old work cannot cancel a newer Form/source through cancel_current.
        let original_open = app.overlay.is_none()
            && current_retirement_work_v3(app).is_some_and(|work| {
                if work.journal_v5.is_some() {
                    return super::journal_v5::form_request(work).is_none()
                        && super::journal_v5::workflow_form_request_v9(work).is_none();
                }
                if let Some(PublicationWorkV3::Workflow(publication)) =
                    work.publication_v3.as_deref()
                {
                    return publication.request().is_none()
                        && publication.intent().epoch() == work.epoch
                        && Arc::ptr_eq(publication.intent().source(), &work.source)
                        && Arc::ptr_eq(publication.intent().page(), &work.page);
                }
                if let Some(PublicationWorkV3::Dispatch(publication)) =
                    work.publication_v3.as_deref()
                {
                    return publication.request().is_none()
                        && publication.intent().epoch() == work.epoch
                        && Arc::ptr_eq(publication.intent().source(), &work.source)
                        && Arc::ptr_eq(publication.intent().page(), &work.page);
                }
                matches!(work.publication_v3.as_deref(),Some(PublicationWorkV3::Open(intent))
                    if intent.epoch()==work.epoch
                        &&Arc::ptr_eq(intent.source(),&work.source)
                        &&Arc::ptr_eq(intent.page(),&work.page))
            });
        if original_open {
            // Initial Open has no Form ticket yet. Preserve its existing
            // resize cancellation only after the closed original input gate.
            cancel_current(app);
        } else {
            cancel(app);
        }
        true
    } else {
        false
    }
}
