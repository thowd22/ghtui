//! Genuine source-bound protected READ/ACK pages on the SAME Actions queue.
//! HistorySeed is not a run, catalogue, write target or permission. A closed
//! historical reference is read intent ONLY; old H/B never become current H/B.
use super::*;
use crate::{
    actions_workspace::{
        ActionsControl, ActionsPublicationAcknowledgement, ActionsPublicationInspectionPage,
    },
    actions_write_journal::{
        ActionsHistorySnapshotV5, ActionsPublicationReference, ActionsWriteJournal,
        ActionsWriteKind, ActionsWriteState,
    },
    providers::gh::*,
    scoped_actions_publication_ui_v3::ScopedActionsComposerRequestV3,
};
use std::path::Path;
#[cfg(test)]
#[path = "scoped_actions_journal_v5_tests.rs"]
mod tests;

enum RequestKindV5 {
    List,
    Inspect(ActionsPublicationReference),
    Acceptance(ActionsPublicationReference),
    WorkflowAckV6(ActionsPublicationReference),
    RunAckV8(ActionsPublicationReference),
    WorkflowRemoteAckV9(ActionsPublicationReference),
}
pub struct JournalRequestV5 {
    ticket: Arc<()>,
    page: Arc<ScopedActionsPageV1>,
    kind: RequestKindV5,
    previous: Option<Arc<ClosedJournalViewV5>>,
    form: Option<Arc<ScopedActionsComposerRequestV3>>,
    workflow_form_v9: Option<Arc<crate::scoped_actions_workflow_edit_ui_v9::WorkflowHistoryFormV9>>,
    held_workflow_v9: usize,
    workflow_partial_ack_v9: std::sync::atomic::AtomicBool,
}
pub struct ClosedJournalViewV5 {
    ticket: Arc<()>,
    snapshot: ActionsHistorySnapshotV5,
    observed: Option<HistoricalReadV6>,
    acknowledged: bool,
    run_inspection_v8: Option<crate::actions_write_journal::ActionsJournalInspection>,
    owned_workflow_remote_v9:
        Option<Arc<crate::actions_write_journal::ActionsOwnedWorkflowRemoteHistoryV9>>,
    bytes_v9: usize,
}
enum HistoricalReadV6 {
    Dispatch(Box<ScopedHistoricalDispatchReadV7>),
    Run(Box<ScopedHistoricalRunReadV5>),
    Workflow(Box<ScopedHistoricalWorkflowReadV6>),
    WorkflowRemoteV9(Box<ScopedHistoricalWorkflowRemoteReadV9>),
}
impl HistoricalReadV6 {
    fn same_observation(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Dispatch(a), Self::Dispatch(b)) => a.same_observation(b),
            (Self::Run(a), Self::Run(b)) => a.same_observation(b),
            (Self::Workflow(a), Self::Workflow(b)) => a.same_observation(b),
            (Self::WorkflowRemoteV9(a), Self::WorkflowRemoteV9(b)) => a.same_observation(b),
            _ => false,
        }
    }
    fn availability_label(&self) -> &'static str {
        match self {
            Self::Dispatch(read) => read.availability_label(),
            Self::Run(read) => read.availability_label(),
            Self::Workflow(read) => read.availability_label(),
            Self::WorkflowRemoteV9(read) => read.availability_label(),
        }
    }
    fn run(&self) -> Option<&WorkflowRun> {
        match self {
            Self::Dispatch(read) => read.run(),
            Self::Run(read) => read.run(),
            Self::Workflow(_) | Self::WorkflowRemoteV9(_) => None,
        }
    }
}
enum HistoryAckV6 {
    Dispatch(Box<crate::actions_write_journal::ActionsDispatchAckV7>),
    Run(Box<crate::actions_write_journal::ActionsHistoryAckV5>),
    Workflow(Box<crate::actions_write_journal::ActionsWorkflowAckV6>),
    ObservedRunV8(Box<crate::actions_write_journal::ActionsRunHistoryAckV8>),
    WorkflowRemoteV9(Box<crate::actions_write_journal::ActionsWorkflowRemoteAckV9>),
}
pub(super) struct JournalBackV5 {
    workspace: Box<ActionsWorkspace>,
    bytes: usize,
    matches: Option<Arc<ActionsLoadedMatchesV2>>,
}
pub struct PreparedJournalV5 {
    work: Arc<ScopedActionsWorkV1>,
    closed: Arc<ClosedJournalViewV5>,
    workspace: ActionsWorkspace,
    bytes: usize,
    workflow_document_v9: Option<Arc<crate::prepared_form_text_v1::PreparedFormTextV1>>,
}
fn safe() -> GithubError {
    GithubError {
        kind: GithubErrorKind::IdentityChanged,
        message: "Original Actions history source/observation unavailable; no fallback or retry"
            .into(),
    }
}
fn clock(work: &ScopedActionsWorkV1) -> GithubResult<()> {
    work.check().map(|_| ()).map_err(|_| safe())
}
fn validate_workflow_history_source_v9(
    source: &ScopedActionsSourceV1,
    request: &JournalRequestV5,
    journal: &ActionsWriteJournal,
    database: &Path,
) -> GithubResult<()> {
    if let Some(original) = &request.workflow_form_v9 {
        if request.reference() != Some(original.reference())
            || !std::ptr::eq(original.intent().source().as_ref(), source)
            || !Arc::ptr_eq(original.intent().page(), &request.page)
        {
            return Err(safe());
        }
        source.validate_owned_workflow_remote_history_v9(database, journal, original.proof())
    } else {
        source.validate_local(database)
    }
}
impl JournalRequestV5 {
    pub(crate) fn held_workflow_bytes_v9(&self) -> usize {
        self.held_workflow_v9
    }
    pub(crate) fn matches_page(&self, page: &Arc<ScopedActionsPageV1>) -> bool {
        Arc::ptr_eq(&self.page, page)
    }
    fn reference(&self) -> Option<&ActionsPublicationReference> {
        match &self.kind {
            RequestKindV5::List => None,
            RequestKindV5::Inspect(r)
            | RequestKindV5::Acceptance(r)
            | RequestKindV5::WorkflowAckV6(r)
            | RequestKindV5::RunAckV8(r)
            | RequestKindV5::WorkflowRemoteAckV9(r) => Some(r),
        }
    }
    fn accepts_overlay(&self, app: &App) -> bool {
        if let Some(original) = &self.workflow_form_v9 {
            return app.overlay == Some(crate::app::Overlay::Form)
                && !app.form_dismissed
                && app.form.as_ref().is_some_and(|form| original.matches(form));
        }
        if let Some(request) = &self.form {
            app.overlay == Some(crate::app::Overlay::Form)
                && !app.form_dismissed
                && app
                    .form
                    .as_ref()
                    .is_some_and(|form| request.matches_form(form))
        } else {
            app.overlay.is_none()
        }
    }
}
impl ClosedJournalViewV5 {
    pub(crate) fn retained_bytes_v9(&self) -> usize {
        self.bytes_v9
    }
    pub(crate) fn workflow_remote_ack_v9(&self) -> bool {
        !self.acknowledged&&self.snapshot.selected().is_some_and(|marker|
            matches!(&self.observed,Some(HistoricalReadV6::WorkflowRemoteV9(read))if crate::actions_write_journal::workflow_remote_ack_available_v9(marker,read)))
    }
    fn run_ack_v8(&self) -> bool {
        !self.acknowledged
            && self
                .run_inspection_v8
                .as_ref()
                .and_then(|i| i.disposition().ok())
                .is_some_and(|d| d.can_acknowledge())
    }
    fn workflow_ack(&self) -> bool {
        !self.acknowledged&&self.snapshot.selected().is_some_and(|marker|
        matches!(&self.observed,Some(HistoricalReadV6::Workflow(observed))if crate::actions_write_journal::workflow_ack_available_v6(marker,observed)))
    }
    fn reference(&self, id: i64) -> Option<ActionsPublicationReference> {
        self.snapshot.reference_for(id)
    }
    fn acceptance(&self) -> bool {
        !self.acknowledged
            && self.observed.is_some()
            && self.snapshot.selected().is_some_and(|marker| {
                marker.state == ActionsWriteState::AcceptedPending
                    && (matches!(
                        marker.kind,
                        ActionsWriteKind::DeleteRun | ActionsWriteKind::DeleteLogs
                    ) || (crate::actions_write_journal::dispatch_acceptance_available_v7(
                        marker,
                    ) && matches!(self.observed, Some(HistoricalReadV6::Dispatch(_)))))
            })
    }
}
impl PreparedJournalV5 {
    pub(crate) fn matches(&self, work: &Arc<ScopedActionsWorkV1>) -> bool {
        Arc::ptr_eq(&self.work, work)
            && work.kind == ScopedActionsRequestKindV1::Publication
            && work.publication_v3.is_none()
            && work.journal_v5.as_ref().is_some_and(|request| {
                Arc::ptr_eq(&request.ticket, &self.closed.ticket)
                    && request.matches_page(&work.page)
            })
    }
}
pub(super) fn overlay_current(app: &App, work: &ScopedActionsWorkV1) -> Option<bool> {
    work.journal_v5
        .as_ref()
        .map(|request| request.accepts_overlay(app))
}
pub(super) fn form_request(
    work: &ScopedActionsWorkV1,
) -> Option<&Arc<ScopedActionsComposerRequestV3>> {
    work.journal_v5
        .as_ref()
        .and_then(|request| request.form.as_ref())
}
pub(super) fn workflow_form_request_v9(
    work: &ScopedActionsWorkV1,
) -> Option<&Arc<crate::scoped_actions_workflow_edit_ui_v9::WorkflowHistoryFormV9>> {
    work.journal_v5
        .as_ref()
        .and_then(|request| request.workflow_form_v9.as_ref())
}
pub(super) fn held_presentation_v9(app: &App) -> Result<usize, String> {
    let state = app
        .scoped_actions_v1
        .as_ref()
        .ok_or("Actual history source unavailable")?;
    let current = app
        .actions
        .as_ref()
        .map(crate::actions_workspace::scoped_read_v1::retained_bytes)
        .transpose()
        .map_err(|_| "Actual history capacity overflow")?
        .unwrap_or(0)
        .checked_add(state.page.retained_bytes())
        .ok_or("Actual history capacity overflow")?;
    // Existing immutable retained envelopes include their prepared rows/model,
    // matches and closed journal snapshots. Shared Arc graphs are deliberately
    // conservatively counted again across distinct Back/Form presentations;
    // there is no unproved pointer-dedup or a fresh allocation grant.
    state
        .back
        .iter()
        .try_fold(current.max(state.retained_bytes), |n, entry| {
            n.checked_add(entry.bytes)
        })
        .and_then(|n| n.checked_add(state.journal_back_v5.as_ref().map_or(0, |back| back.bytes)))
        .ok_or_else(|| "Actual history retained capacity overflow".into())
}
pub(crate) fn capture_workflow_form_v9(app: &mut App, ack: bool) -> bool {
    // Pure original/current/busy/newer preflight BEFORE any Form error/flag,
    // cancellation, ticket clone, epoch, loading or queue effect.
    let applicable=app.scoped_actions_v1.as_ref().is_some_and(|state|current(app,state)
        &&app.form.as_ref().is_some_and(|form|matches!(&form.purpose,crate::forms::FormPurpose::ScopedWorkflowEditV9(edit)
            if Arc::ptr_eq(edit.intent().source(),&state.source)&&Arc::ptr_eq(edit.intent().page(),&state.page))))
        &&app.view==View::Actions&&app.overlay==Some(crate::app::Overlay::Form)&&!app.form_dismissed
        &&!app.loading[View::Actions.index()]&&app.scoped_actions_work_v1.is_none()&&app.scoped_actions_owner_v1.is_none()
        &&!app.scoped_actions_cancel_v1&&!super::newer_foreign_without_form_v3(app);
    if !applicable {
        return true;
    }
    let captured = (|| -> Result<Arc<ScopedActionsWorkV1>, String> {
        let state = app
            .scoped_actions_v1
            .as_ref()
            .filter(|state| current(app, state))
            .ok_or("Actual W9 source unavailable")?;
        let form = app.form.as_ref().ok_or("Actual W9 Form unavailable")?;
        if app.view != View::Actions
            || app.overlay != Some(crate::app::Overlay::Form)
            || app.form_dismissed
            || app.loading[View::Actions.index()]
            || app.scoped_actions_work_v1.is_some()
            || app.scoped_actions_owner_v1.is_some()
            || app.scoped_actions_cancel_v1
            || super::newer_foreign_without_form_v3(app)
        {
            return Err("Another original intent is current".into());
        }
        let form_held =
            crate::scoped_actions_workflow_edit_ui_v9::history_presentation_bound_v9(form)?;
        let held = held_presentation_v9(app)?
            .checked_add(form_held)
            .filter(|n| *n <= 8 * 1024 * 1024)
            .ok_or("Retained W9 history presentation exceeds8MiB")?;
        let origin = crate::scoped_actions_workflow_edit_ui_v9::capture_history_v9(form)?;
        if origin.held_bytes() != form_held {
            return Err("W9 Form capacity changed before capture".into());
        }
        if !Arc::ptr_eq(origin.intent().source(), &state.source)
            || !Arc::ptr_eq(origin.intent().page(), &state.page)
            || ack && !crate::scoped_actions_workflow_edit_ui_v9::history_ack_ready_v9(form)
        {
            return Err("Original W9 Form/source/inspection differs".into());
        }
        let request = Arc::new(JournalRequestV5 {
            ticket: Arc::new(()),
            page: state.page.clone(),
            kind: if ack {
                RequestKindV5::WorkflowRemoteAckV9(origin.reference().clone())
            } else {
                RequestKindV5::Inspect(origin.reference().clone())
            },
            previous: crate::scoped_actions_workflow_edit_ui_v9::history_previous_v9(form),
            form: None,
            workflow_form_v9: Some(origin),
            held_workflow_v9: held,
            workflow_partial_ack_v9: std::sync::atomic::AtomicBool::new(false),
        });
        ScopedActionsWorkV1::capture_journal_v5(
            state.page.clone(),
            state.epoch.wrapping_add(1),
            request,
        )
    })();
    match captured {
        Ok(work) => {
            let state = app
                .scoped_actions_v1
                .as_mut()
                .expect("Pure exact W9 READ origin preflight");
            if let Some(watch) = state.watch_v2.take() {
                watch.stop()
            }
            state.epoch = work.epoch;
            state.editor_v2 = None;
            if let Some(form) = app.form.as_mut() {
                form.submit_requested = false;
                form.preview_requested = false;
                form.submitting = true;
            }
            app.scoped_actions_work_v1 = Some(work);
            app.hits.clear();
            app.status="Reading exact original workflow remote history • effect UNVERIFIED • no mutation/retry/resend".into();
        }
        Err(_) => {
            if let Some(form) = app.form.as_mut() {
                crate::scoped_actions_workflow_edit_ui_v9::history_refused_v9(form);
            }
        }
    }
    true
}
pub(super) fn submission_failed(app: &mut App, work: &Arc<ScopedActionsWorkV1>) -> bool {
    let Some(request) = &work.journal_v5 else {
        return false;
    };
    // This is an input/queue failure, not permission to publish through an old
    // source. A newer Form, page, source, work or owner must stay untouched.
    let applicable = request.accepts_overlay(app)
        && !publication_v3::foreign_intent(app, work)
        && app.scoped_actions_v1.as_ref().is_some_and(|state| {
            current(app, state)
                && Arc::ptr_eq(&state.source, &work.source)
                && Arc::ptr_eq(&state.page, &work.page)
                && state.epoch == work.epoch
        })
        && app
            .scoped_actions_work_v1
            .as_ref()
            .is_none_or(|current| Arc::ptr_eq(current, work))
        && app
            .scoped_actions_owner_v1
            .as_ref()
            .is_none_or(|owner| Arc::ptr_eq(owner.work(), work));
    if applicable {
        if let (Some(original), Some(form)) = (&request.form, app.form.as_mut()) {
            original.retire_busy(form);
            form.error="Original history queue unavailable; protected receipt retained, no request repeated".into();
        }
        if let (Some(original), Some(form)) = (&request.workflow_form_v9, app.form.as_mut()) {
            original.retire_busy(form);
            form.error="Original W9 history queue unavailable; protected reference retained, no request repeated".into();
        }
        app.status =
            "Original Actions history submission unavailable • no fallback or retry".into();
        app.hits.clear();
    }
    work.cancellation().cancel();
    true
}
pub(super) fn fail(app: &mut App, owner: &Arc<ScopedActionsOwnerV1>) -> bool {
    if !owner.owns(app) {
        return false;
    }
    let owned_form = form_request(owner.work()).is_some_and(|request| {
        app.form
            .as_ref()
            .is_some_and(|form| request.matches_form(form))
    }) || workflow_form_request_v9(owner.work())
        .is_some_and(|request| app.form.as_ref().is_some_and(|form| request.matches(form)));
    crate::repository_catalogue_native::privacy(app);
    crate::repository_catalogue_native::retire_remote_views(app);
    if owned_form {
        app.form = None;
        app.form_dismissed = false;
        if app.overlay == Some(crate::app::Overlay::Form) {
            app.overlay = None;
        }
    }
    app.loading[View::Actions.index()] = false;
    app.hits.clear();
    app.status = if owner.work().journal_v5.as_ref().is_some_and(|r| {
        r.workflow_partial_ack_v9
            .load(std::sync::atomic::Ordering::Acquire)
    }) {
        "Local W9 ACK incomplete • runner observation acknowledged; durable original row remains protected • effect UNVERIFIED • no retry/resend".into()
    } else {
        "Original protected Actions history unavailable • private source hidden • original markers/receipts retained • no retry".into()
    };
    true
}
pub(super) fn prepare_policy(workspace: &mut ActionsWorkspace, pending: bool) {
    for control in &mut workspace.controls {
        let allowed = matches!(
            control.action,
            ActionsAction::Back
                | ActionsAction::Refresh
                | ActionsAction::PublicationList
                | ActionsAction::PublicationAcknowledgeAcceptance(_)
                | ActionsAction::PublicationAcknowledge(_)
        );
        control.disabled_reason = if !allowed {
            Some("Protected history is READ/ACK only; no saved target or permission".into())
        } else if pending && !matches!(control.action, ActionsAction::Back) {
            Some("Wait or cancel this exact original history read".into())
        } else {
            None
        };
    }
}
fn request(
    page: Arc<ScopedActionsPageV1>,
    kind: RequestKindV5,
    previous: Option<Arc<ClosedJournalViewV5>>,
    form: Option<Arc<ScopedActionsComposerRequestV3>>,
    held_workflow_v9: usize,
) -> Arc<JournalRequestV5> {
    Arc::new(JournalRequestV5 {
        ticket: Arc::new(()),
        page,
        kind,
        previous,
        form,
        workflow_form_v9: None,
        held_workflow_v9,
        workflow_partial_ack_v9: std::sync::atomic::AtomicBool::new(false),
    })
}
pub fn open_summary(app: &mut App) {
    if app.view != View::Inbox
        || app.overlay.is_some()
        || app.loading[View::Actions.index()]
        || app.scoped_actions_v1.is_some()
        || app.scoped_actions_work_v1.is_some()
        || app.scoped_actions_owner_v1.is_some()
        || app.scoped_actions_cancel_v1
        || newer_foreign_intent(app)
    {
        return;
    }
    let prepared = summary_history_seed_v5(app).and_then(|seed| {
        let original = request(
            seed.page.clone(),
            RequestKindV5::List,
            None,
            None,
            seed.retained_bytes,
        );
        let work = ScopedActionsWorkV1::capture_journal_v5(seed.page.clone(), 1, original)?;
        Ok((seed, work))
    });
    if let Ok((seed, work)) = prepared {
        app.scoped_actions_v1 = Some(ScopedActionsStateV1 {
            dispatch_run_v7: None,
            source: seed.source,
            page: seed.page.clone(),
            entry_page_v2: Arc::downgrade(&seed.page),
            retained_bytes: seed.retained_bytes,
            back: vec![],
            epoch: work.epoch,
            matches_v2: None,
            watch_v2: None,
            editor_v2: None,
            journal_v5: None,
            journal_back_v5: None,
        });
        app.actions = Some(seed.workspace);
        app.view = View::Actions;
        app.scoped_actions_work_v1 = Some(work);
        app.hits.clear();
        app.status="Opening original Actions publication history • actual Summary source • READ/ACK only • no run target or mutation".into();
    }
}
pub(super) fn intercept(app: &mut App, action: &ActionsAction) -> bool {
    let Some(state) = app.scoped_actions_v1.as_ref() else {
        return false;
    };
    let history_route = state.journal_v5.is_some() || state.source.history_pr_v5.is_some();
    let history_action = matches!(
        action,
        ActionsAction::PublicationList
            | ActionsAction::PublicationInspect(_)
            | ActionsAction::PublicationAcknowledgeAcceptance(_)
            | ActionsAction::PublicationAcknowledge(_)
    );
    if (history_route || history_action)
        && (app.view != View::Actions
            || app.overlay.is_some()
            || !current(app, state)
            || newer_foreign_intent(app))
    {
        return true;
    }
    let requested = match action {
        ActionsAction::PublicationList => Some(RequestKindV5::List),
        ActionsAction::PublicationInspect(id) => {
            let reference = state
                .journal_v5
                .as_ref()
                .and_then(|view| view.reference(*id));
            if let Some(reference) = reference {
                Some(RequestKindV5::Inspect(reference))
            } else {
                return true;
            }
        }
        ActionsAction::PublicationAcknowledge(id) => {
            let reference = state
                .journal_v5
                .as_ref()
                .filter(|view| {
                    view.workflow_ack() || view.run_ack_v8() || view.workflow_remote_ack_v9()
                })
                .and_then(|view| view.reference(*id));
            if let Some(reference) = reference {
                Some(
                    if state
                        .journal_v5
                        .as_ref()
                        .is_some_and(|v| v.workflow_remote_ack_v9())
                    {
                        RequestKindV5::WorkflowRemoteAckV9(reference)
                    } else if state.journal_v5.as_ref().is_some_and(|v| v.run_ack_v8()) {
                        RequestKindV5::RunAckV8(reference)
                    } else {
                        RequestKindV5::WorkflowAckV6(reference)
                    },
                )
            } else {
                return true;
            }
        }
        ActionsAction::PublicationAcknowledgeAcceptance(id) => {
            let reference = state
                .journal_v5
                .as_ref()
                .filter(|view| view.acceptance())
                .and_then(|view| view.reference(*id));
            if let Some(reference) = reference {
                Some(RequestKindV5::Acceptance(reference))
            } else {
                return true;
            }
        }
        ActionsAction::Refresh if state.journal_v5.is_some() => Some(
            state
                .journal_v5
                .as_ref()
                .and_then(|view| view.snapshot.selected())
                .map_or(RequestKindV5::List, |marker| {
                    RequestKindV5::Inspect(marker.publication_reference())
                }),
        ),
        ActionsAction::Back
            if state.journal_v5.is_some() || state.source.history_pr_v5.is_some() =>
        {
            if app.scoped_actions_work_v1.is_some() || app.scoped_actions_owner_v1.is_some() {
                let original = match (
                    app.scoped_actions_work_v1.as_ref(),
                    app.scoped_actions_owner_v1.as_ref(),
                ) {
                    (Some(work), Some(owner)) if Arc::ptr_eq(work, owner.work()) => Some(work),
                    (Some(_), Some(_)) => None,
                    (Some(work), None) => Some(work),
                    (None, Some(owner)) => Some(owner.work()),
                    (None, None) => None,
                };
                if original.is_some_and(|work| {
                    work.journal_v5.is_some()
                        && work.epoch == state.epoch
                        && Arc::ptr_eq(&work.source, &state.source)
                        && Arc::ptr_eq(&work.page, &state.page)
                        && overlay_current(app, work) == Some(true)
                        && !publication_v3::foreign_intent(app, work)
                }) {
                    cancel_current(app);
                }
                return true;
            }
            if app.loading[View::Actions.index()] {
                return true;
            }
            let mut state = app
                .scoped_actions_v1
                .take()
                .expect("actual checked history state");
            if let Some(back) = state.journal_back_v5.take() {
                app.actions = Some(*back.workspace);
                state.retained_bytes = back.bytes;
                state.matches_v2 = back.matches;
                state.journal_v5 = None;
                state.epoch = state.epoch.wrapping_add(1);
                app.scoped_actions_v1 = Some(state);
            } else if let Some(origin) = &state.source.history_pr_v5 {
                let (selected, scroll) = origin.return_position();
                app.view = View::Inbox;
                app.selected = selected;
                app.scroll = scroll;
                app.actions_content_focused = false;
            } else {
                state.journal_v5 = None;
                app.scoped_actions_v1 = Some(state);
            }
            app.hits.clear();
            app.status =
                "Original history Back • retained actual presentation • zero GH/SQL/remote effects"
                    .into();
            return true;
        }
        ActionsAction::Compose(_)
            if state.source.history_pr_v5.is_some() || state.journal_v5.is_some() =>
        {
            explain(app,"Protected history is READ/ACK only. Open the actual current repository/run for a new intent; no saved target or permission is inferred.");
            return true;
        }
        _ if state.source.history_pr_v5.is_some() || state.journal_v5.is_some() => {
            // Ordinary row selection/scrolling/control paging remain SAME native
            // Actions semantics; every other provider/mutation route is closed.
            return !matches!(
                action,
                ActionsAction::Select(_)
                    | ActionsAction::ControlsPage(_)
                    | ActionsAction::Unavailable(_)
            );
        }
        _ => None,
    };
    let Some(kind) = requested else { return false };
    let allowed = app.view == View::Actions
        && app.overlay.is_none()
        && !app.loading[View::Actions.index()]
        && app.scoped_actions_work_v1.is_none()
        && app.scoped_actions_owner_v1.is_none()
        && !app.scoped_actions_cancel_v1
        && !newer_foreign_intent(app)
        && current(app, state);
    if !allowed {
        return true;
    }
    let epoch = state.epoch.wrapping_add(1);
    let Ok(held) = held_presentation_v9(app) else {
        return true;
    };
    let original = request(
        state.page.clone(),
        kind,
        state.journal_v5.clone(),
        None,
        held,
    );
    if let Ok(work) = ScopedActionsWorkV1::capture_journal_v5(state.page.clone(), epoch, original) {
        let state = app
            .scoped_actions_v1
            .as_mut()
            .expect("pure actual preflight");
        if let Some(watch) = state.watch_v2.take() {
            watch.stop();
        }
        state.epoch = epoch;
        state.editor_v2 = None;
        app.scoped_actions_work_v1 = Some(work);
        app.hits.clear();
        app.status =
            "Reading exact original Actions history • original30s includes queue • no remote write"
                .into();
    }
    true
}
/// Genuine current protected Delete form may inspect its exact original ID
/// without reopening the possibly deleted run. Ticket/source/epoch remain exact.
pub(super) fn capture_deleted_form(
    app: &App,
    form_request: Arc<ScopedActionsComposerRequestV3>,
) -> Result<Arc<ScopedActionsWorkV1>, String> {
    let state = app
        .scoped_actions_v1
        .as_ref()
        .ok_or("Actual source unavailable")?;
    if !form_request.matches_form(
        app.form
            .as_ref()
            .ok_or("Actual original Form unavailable")?,
    ) || !publication_v3::form_source_current(app, form_request.opened().intent())
    {
        return Err("Original protected Form/source differs".into());
    }
    let reference = form_request
        .reference()
        .cloned()
        .ok_or("Original reference unavailable")?;
    let original = request(
        state.page.clone(),
        RequestKindV5::Inspect(reference),
        None,
        Some(form_request),
        0, // legacy Run Form path never enters the W9 profile
    );
    ScopedActionsWorkV1::capture_journal_v5(
        state.page.clone(),
        state.epoch.wrapping_add(1),
        original,
    )
}
pub(crate) fn execute(
    work: &Arc<ScopedActionsWorkV1>,
    provider: &GithubProvider,
    database: &Path,
) -> Result<PreparedJournalV5, String> {
    let result = (|| -> GithubResult<_> {
        clock(work)?;
        let request = work
            .journal_v5
            .as_ref()
            .filter(|request| request.matches_page(&work.page))
            .ok_or_else(safe)?;
        if work.publication_v3.is_some()
            || work.kind != ScopedActionsRequestKindV1::Publication
            || work.watch_v2.is_some()
            || work.resource_v2.is_some()
            || work.search_v2.is_some()
        {
            return Err(safe());
        }
        let source = &work.source;
        let mut journal = if request.workflow_form_v9.is_some() {
            let journal = ActionsWriteJournal::open(database).map_err(|_| safe())?;
            validate_workflow_history_source_v9(source, request, &journal, database)?;
            journal
        } else {
            source.validate_local(database)?;
            ActionsWriteJournal::open(database).map_err(|_| safe())?
        };
        let workflow_remote = journal
            .workflow_remote_history_profile_v9(source.original().scope(), request.reference())
            .map_err(|_| safe())?;
        let core_policy = if workflow_remote {
            Some(WorkflowCoreJsonV9::for_history_v9(provider, work)?)
        } else {
            None
        };
        let phase = provider
            .clone()
            .with_catalogue_read_deadline(work.deadline());
        let mut session = if let Some(policy) = &core_policy {
            phase.begin_workflow_edit_access_v9(
                source.original().source(),
                &crate::scoped_actions_publication_v3::access_limits(),
                policy,
                work.cancellation(),
            )?
        } else {
            phase.begin_repository_catalogue_access(
                source.original().source(),
                &crate::scoped_actions_publication_v3::access_limits(),
                work.cancellation(),
            )?
        };
        crate::scoped_actions_publication_v3::scope(source, session.identity())?;
        let window = session.reserve_measured_external_reads(
            if workflow_remote { 8 } else { 6 },
            if workflow_remote {
                256 * 1024
            } else {
                512 * 1024
            },
            1024 * 1024,
            2,
            work.cancellation(),
        )?;
        let effective = window.deadline();
        let measured = phase.clone().with_measured_read_window(window)?;
        session.verify(source.reference(), work.cancellation())?;
        let current = session
            .observed_original_v1(source.reference().id)
            .ok_or_else(safe)?;
        let json_heap = core_policy
            .as_ref()
            .map(|policy| policy.bind_measured_v9(&measured, work.cancellation()))
            .transpose()?;
        let _models_v9 = json_heap
            .as_ref()
            .map(|heap| heap.reserve_model_v9(1024 * 1024))
            .transpose()?;
        let parent = if let Some(origin) = &source.history_pr_v5 {
            Some(if let Some(heap) = &json_heap {
                origin.fresh_workflow_edit_history_v9(
                    &measured,
                    current,
                    heap,
                    work.cancellation(),
                )?
            } else {
                origin.fresh(&measured, current, work.cancellation())?
            })
        } else if let Some(origin) = source.pr_origin_v2() {
            Some(if let Some(heap) = &json_heap {
                origin.fresh_workflow_history_parent_v9(
                    &measured,
                    current,
                    heap,
                    work.cancellation(),
                )?
            } else {
                origin.fresh_history_parent_v5(&measured, current, work.cancellation())?
            })
        } else {
            None
        };
        let mut snapshot = journal
            .scoped_history_v5(
                &measured,
                session.identity(),
                current,
                parent.as_ref(),
                request.reference(),
            )
            .map_err(|_| safe())?;
        let mut observed = if request.reference().is_some() {
            Some(if workflow_remote {
                HistoricalReadV6::WorkflowRemoteV9(Box::new(
                    measured.scoped_actions_workflow_remote_history_v9(
                        current,
                        &snapshot.workflow_remote_key_v9().map_err(|_| safe())?,
                        json_heap.as_ref().ok_or_else(safe)?,
                        work.cancellation(),
                    )?,
                ))
            } else if snapshot
                .selected()
                .is_some_and(|marker| marker.context.original_dispatch_v7.is_some())
            {
                HistoricalReadV6::Dispatch(Box::new(
                    measured.scoped_actions_historical_dispatch_v7(
                        current,
                        &snapshot.dispatch_key_v7().map_err(|_| safe())?,
                        work.cancellation(),
                    )?,
                ))
            } else if snapshot
                .selected()
                .is_some_and(|marker| marker.context.original_workflow_v6.is_some())
            {
                HistoricalReadV6::Workflow(Box::new(
                    measured.scoped_actions_historical_workflow_v6(
                        current,
                        &snapshot.workflow_key_v6().map_err(|_| safe())?,
                        work.cancellation(),
                    )?,
                ))
            } else {
                HistoricalReadV6::Run(Box::new(measured.scoped_actions_historical_run_v5(
                    current,
                    &snapshot.key().map_err(|_| safe())?,
                    work.cancellation(),
                )?))
            })
        } else {
            None
        };
        let acceptance = matches!(request.kind, RequestKindV5::Acceptance(_));
        let workflow_ack = matches!(request.kind, RequestKindV5::WorkflowAckV6(_));
        let run_ack_v8 = matches!(request.kind, RequestKindV5::RunAckV8(_));
        let workflow_remote_ack = matches!(request.kind, RequestKindV5::WorkflowRemoteAckV9(_));
        let mut run_inspection_v8 = match (&observed, snapshot.selected()) {
            (Some(HistoricalReadV6::Run(read)), Some(marker))
                if read.run().is_some()
                    && matches!(
                        marker.state,
                        ActionsWriteState::Pending
                            | ActionsWriteState::Uncertain
                            | ActionsWriteState::AcceptedPending
                    )
                    && matches!(
                        marker.kind,
                        ActionsWriteKind::Cancel
                            | ActionsWriteKind::ForceCancel
                            | ActionsWriteKind::RerunAll
                            | ActionsWriteKind::RerunFailed
                            | ActionsWriteKind::RerunJob
                            | ActionsWriteKind::DeleteLogs
                    ) =>
            {
                Some(
                    journal
                        .prepare_run_history_inspection_v8(
                            &snapshot,
                            read,
                            &measured,
                            session.identity(),
                            current,
                            work.cancellation(),
                        )
                        .map_err(|_| safe())?,
                )
            }
            _ => None,
        };
        let run_ack_available_v8 = run_inspection_v8
            .as_ref()
            .and_then(|i| i.disposition().ok())
            .is_some_and(|d| d.can_acknowledge());
        if acceptance || workflow_ack || run_ack_v8 || workflow_remote_ack {
            let previous = request
                .previous
                .as_ref()
                .filter(|view| {
                    if workflow_remote_ack {
                        view.workflow_remote_ack_v9()
                    } else if run_ack_v8 {
                        view.run_ack_v8()
                    } else if workflow_ack {
                        view.workflow_ack()
                    } else {
                        view.acceptance()
                    }
                })
                .ok_or_else(safe)?;
            journal
                .validate_history_v5(&previous.snapshot)
                .map_err(|_| safe())?;
            if previous
                .snapshot
                .selected()
                .map(|marker| marker.publication_reference())
                != request.reference().cloned()
                || !previous
                    .observed
                    .as_ref()
                    .zip(observed.as_ref())
                    .is_some_and(|(a, b)| a.same_observation(b))
            {
                return Err(safe());
            }
            if run_ack_v8 {
                journal
                    .same_scoped_inspection_v3(
                        previous.run_inspection_v8.as_ref().ok_or_else(safe)?,
                        run_inspection_v8.as_ref().ok_or_else(safe)?,
                    )
                    .map_err(|_| safe())?;
            }
        }
        let mut workspace = ActionsWorkspace::new(
            session.identity().clone(),
            vec![current.repository().full_name.clone()],
        );
        workspace.view = if request.reference().is_some() {
            ActionsView::PublicationInspection
        } else {
            ActionsView::Publications
        };
        workspace.publication_total = Some(snapshot.total());
        workspace.publication_complete = snapshot.complete();
        workspace.recorded_workflow_history_v9 = snapshot.recorded_workflow_v9();
        if let Some(marker) = snapshot.selected() {
            let mut lines = crate::actions_routes::original_lines(marker);
            lines.pop();
            lines.push("Saved original run H/A and PR H/B above are HISTORICAL; this current read never retargets or resends them.".into());
            if let Some(parent) = &parent {
                lines.push(format!(
                    "CURRENT genuine PR #{} / numeric #{} / H {} / B {}",
                    parent.number(),
                    parent.database_id(),
                    parent.head_sha(),
                    parent.base_tip_sha()
                ));
            }
            if let Some(observed) = &observed {
                lines.push(observed.availability_label().into());
                if let HistoricalReadV6::Workflow(read) = observed {
                    if let Some(workflow) = read.workflow() {
                        lines.push(format!(
                            "Actual current workflow {} / state {}",
                            workflow.id, workflow.state
                        ));
                        lines.push(format!("Workflow path {} • no causal claim", workflow.path));
                    }
                }
                if let HistoricalReadV6::WorkflowRemoteV9(read) = observed {
                    lines.extend(read.factual_lines());
                    lines.push("Explicit local ACK releases only this inspected original row; effect UNVERIFIED, no retry/resend or source/permission reconstruction.".into());
                }
                if matches!(observed, HistoricalReadV6::Dispatch(_)) {
                    let key = snapshot.dispatch_key_v7().map_err(|_| safe())?;
                    lines.push(format!(
                        "HISTORICAL Dispatch {} {}",
                        key.context().namespace,
                        key.context().raw_name
                    ));
                    lines.push(format!("Saved ref tip {}", key.context().tip));
                    lines.push(format!("Saved definition commit {}", key.context().commit));
                    lines.push(format!(
                        "Chosen API {} • acceptance only",
                        key.context().protocol
                    ));
                    lines.push("No guessed run, completion or retry claim".into());
                }
                if let Some(run) = observed.run() {
                    lines.push(format!(
                        "Actual current readable run {} / A {} / H {} / {} / {} — read only",
                        run.id, run.attempt, run.head_sha, run.status, run.conclusion
                    ));
                }
            }
            if workflow_ack {
                lines.push("Explicit original workflow state ACK committed locally".into());
                lines.push("Exact receipt retained • no PUT/retry".into());
            }
            if run_ack_v8 {
                lines.push("Explicit original observed Run state ACK committed locally • no causality, completion or resend claim".into());
            }
            if workflow_remote_ack {
                lines.push("Explicit W9 observation ACK committed locally".into());
                lines.push("effect UNVERIFIED • original receipt retained if present".into());
                lines.push("no remote request repeated".into());
            }
            if acceptance {
                if matches!(&observed, Some(HistoricalReadV6::Dispatch(_))) {
                    lines.push("Explicit inspected acceptance ACK committed locally".into());
                    lines.push("effect UNVERIFIED • no remote request repeated • original receipt retained".into());
                } else {
                    lines.push("Explicit inspected acceptance ACK committed locally • effect UNVERIFIED • no remote request repeated • original receipt retained".into());
                }
            }
            workspace.publication_inspection = Some(ActionsPublicationInspectionPage {
                attempt_id: marker.attempt_id,
                heading: format!(
                    "Original {:?} publication #{} • current original-source inspection",
                    marker.kind, marker.attempt_id
                ),
                lines,
                acknowledgement: if (!workflow_remote_ack
                    && matches!(&observed,Some(HistoricalReadV6::WorkflowRemoteV9(read))if crate::actions_write_journal::workflow_remote_ack_available_v9(marker,read)))
                    || (!run_ack_v8 && run_ack_available_v8)
                    || (!workflow_ack
                        && matches!(&observed,Some(HistoricalReadV6::Workflow(read))if crate::actions_write_journal::workflow_ack_available_v6(marker,read)))
                {
                    Some(ActionsPublicationAcknowledgement::ObservedState)
                } else {
                    (!acceptance
                        && marker.state == ActionsWriteState::AcceptedPending
                        && (matches!(
                            marker.kind,
                            ActionsWriteKind::DeleteRun | ActionsWriteKind::DeleteLogs
                        ) || (crate::actions_write_journal::dispatch_acceptance_available_v7(
                            marker,
                        ) && matches!(&observed, Some(HistoricalReadV6::Dispatch(_))))))
                    .then_some(ActionsPublicationAcknowledgement::InspectedAcceptance)
                },
            });
        } else {
            workspace.publication_rows = snapshot.take_rows();
        }
        workspace.prepare_rows();
        workspace.controls.retain(|control| {
            matches!(
                control.action,
                ActionsAction::PublicationList
                    | ActionsAction::PublicationAcknowledgeAcceptance(_)
                    | ActionsAction::PublicationAcknowledge(_)
                    | ActionsAction::Back
            )
        });
        workspace.controls.push(ActionsControl {
            label: "[r Refresh history]".into(),
            action: ActionsAction::Refresh,
            disabled_reason: None,
        });
        if !workspace
            .controls
            .iter()
            .any(|control| control.action == ActionsAction::Back)
        {
            workspace.controls.push(ActionsControl {
                label: "[b Back]".into(),
                action: ActionsAction::Back,
                disabled_reason: None,
            });
        }
        workspace.warnings.push("History is a bounded non-atomic same-account inventory filtered by genuine numeric repository/PR identities; unseen rows remain UNKNOWN. No saved record grants a live run target.".into());
        let mut bytes = crate::actions_workspace::scoped_read_v1::retained_bytes(&workspace)
            .map_err(|_| safe())?
            .saturating_add(snapshot.owned_bytes())
            .saturating_add(256 * 1024);
        if bytes > PREPARED_LIMIT_V1 {
            return Err(safe());
        }
        let mut _document_charge_v9 = None;
        let workflow_document_v9 = if request.workflow_form_v9.is_some() {
            let heap = json_heap.as_ref().ok_or_else(safe)?;
            let lines = &workspace
                .publication_inspection
                .as_ref()
                .ok_or_else(safe)?
                .lines;
            let retained = PREPARED_LIMIT_V1
                .checked_sub(request.held_workflow_v9)
                .and_then(|n| n.checked_sub(bytes))
                .ok_or_else(safe)?;
            let peak = heap.remaining_v9()?;
            let mut charge = heap.reserve_model_v9(peak)?;
            let document =
                crate::prepared_form_text_v1::PreparedFormTextV1::prepare_with_remaining_v9(
                    lines,
                    work.cancellation(),
                    effective,
                    retained,
                    peak,
                )
                .map_err(|_| safe())?;
            charge.retain_actual_v9(document.retained_bytes())?;
            bytes = bytes
                .checked_add(document.retained_bytes())
                .ok_or_else(safe)?;
            _document_charge_v9 = Some(charge);
            Some(document)
        } else {
            None
        };
        if let Some(HistoricalReadV6::WorkflowRemoteV9(read)) = &observed {
            measured.close_workflow_remote_history_v9(
                read,
                current,
                &snapshot.workflow_remote_key_v9().map_err(|_| safe())?,
                json_heap.as_ref().ok_or_else(safe)?,
                work.cancellation(),
            )?;
        }
        if run_ack_v8 {
            let Some(HistoricalReadV6::Run(old)) = &observed else {
                return Err(safe());
            };
            let closing = measured.scoped_actions_historical_run_v5(
                current,
                &snapshot.key().map_err(|_| safe())?,
                work.cancellation(),
            )?;
            if !old.same_observation(&closing) {
                return Err(safe());
            }
        }
        let pending = if workflow_remote_ack {
            let Some(HistoricalReadV6::WorkflowRemoteV9(read)) = observed.take() else {
                return Err(safe());
            };
            Some(HistoryAckV6::WorkflowRemoteV9(Box::new(
                journal
                    .prepare_workflow_remote_ack_v9(
                        &snapshot,
                        *read,
                        &measured,
                        work.cancellation(),
                        request
                            .workflow_form_v9
                            .as_ref()
                            .map(|origin| origin.proof()),
                    )
                    .map_err(|_| safe())?,
            )))
        } else if run_ack_v8 {
            let Some(HistoricalReadV6::Run(read)) = observed.take() else {
                return Err(safe());
            };
            Some(HistoryAckV6::ObservedRunV8(Box::new(
                journal
                    .prepare_run_history_ack_v8(
                        &snapshot,
                        run_inspection_v8.take(),
                        *read,
                        &measured,
                        work.cancellation(),
                        false,
                        None,
                    )
                    .map_err(|_| safe())?,
            )))
        } else if acceptance {
            if let Some(HistoricalReadV6::Dispatch(_)) = &observed {
                let Some(HistoricalReadV6::Dispatch(read)) = observed.take() else {
                    return Err(safe());
                };
                Some(HistoryAckV6::Dispatch(Box::new(
                    journal
                        .prepare_dispatch_ack_v7(
                            &snapshot,
                            *read,
                            &measured,
                            work.cancellation(),
                            None,
                        )
                        .map_err(|_| safe())?,
                )))
            } else {
                let Some(HistoricalReadV6::Run(read)) = observed.take() else {
                    return Err(safe());
                };
                Some(HistoryAckV6::Run(Box::new(
                    journal
                        .prepare_history_acceptance_v5(
                            &snapshot,
                            *read,
                            &measured,
                            work.cancellation(),
                        )
                        .map_err(|_| safe())?,
                )))
            }
        } else if workflow_ack {
            let Some(HistoricalReadV6::Workflow(read)) = observed.take() else {
                return Err(safe());
            };
            Some(HistoryAckV6::Workflow(Box::new(
                journal
                    .prepare_workflow_ack_v6(&snapshot, *read, &measured, work.cancellation(), None)
                    .map_err(|_| safe())?,
            )))
        } else {
            None
        };
        bytes = bytes.saturating_add(pending.as_ref().map_or(0, |ack| match ack {
            HistoryAckV6::Dispatch(ack) => ack.retained_bytes(),
            HistoryAckV6::Run(ack) => ack.retained_row_fence_bytes_v6(),
            HistoryAckV6::Workflow(ack) => ack.retained_bytes(),
            HistoryAckV6::ObservedRunV8(ack) => ack.retained_bytes(),
            HistoryAckV6::WorkflowRemoteV9(ack) => ack.retained_bytes(),
        }));
        if bytes > PREPARED_LIMIT_V1 {
            return Err(safe());
        }
        let current = session
            .observed_original_v1(source.reference().id)
            .ok_or_else(safe)?;
        let closing = if let Some(origin) = &source.history_pr_v5 {
            Some(if let Some(heap) = &json_heap {
                origin.fresh_workflow_edit_history_v9(
                    &measured,
                    current,
                    heap,
                    work.cancellation(),
                )?
            } else {
                origin.fresh(&measured, current, work.cancellation())?
            })
        } else if let Some(origin) = source.pr_origin_v2() {
            Some(if let Some(heap) = &json_heap {
                origin.fresh_workflow_history_parent_v9(
                    &measured,
                    current,
                    heap,
                    work.cancellation(),
                )?
            } else {
                origin.fresh_history_parent_v5(&measured, current, work.cancellation())?
            })
        } else {
            None
        };
        if match (&parent, &closing) {
            (Some(a), Some(b)) => !GithubProvider::same_thread_history_parent_v1(a, b),
            (None, None) => false,
            _ => true,
        } {
            return Err(safe());
        }
        session.verify(source.reference(), work.cancellation())?;
        if workflow_remote
            && request
                .held_workflow_v9
                .checked_add(bytes)
                .is_none_or(|n| n > PREPARED_LIMIT_V1)
        {
            return Err(safe());
        }
        let mut owned_workflow_remote_v9 = request
            .workflow_form_v9
            .as_ref()
            .map(|origin| origin.proof().clone());
        let closed = session.close_with_guard(work.cancellation(), || {
            validate_workflow_history_source_v9(source, request, &journal, database)?;
            clock(work)?;
            journal.validate_history_v5(&snapshot).map_err(|_| safe())?;
            if let Some(ack) = &pending {
                match ack {
                    HistoryAckV6::Dispatch(ack) => journal.validate_dispatch_ack_v7(ack),
                    HistoryAckV6::Run(ack) => journal.validate_history_ack_v5(ack),
                    HistoryAckV6::Workflow(ack) => journal.validate_workflow_ack_v6(ack),
                    HistoryAckV6::ObservedRunV8(ack) => journal.validate_run_history_ack_v8(ack),
                    HistoryAckV6::WorkflowRemoteV9(ack) => {
                        journal.validate_workflow_remote_ack_v9(ack)
                    }
                }
                .map_err(|_| safe())?;
            }
            Ok(())
        })?;
        crate::scoped_actions_publication_v3::scope(source, closed.identity())?;
        if matches!(&observed, Some(HistoricalReadV6::WorkflowRemoteV9(read)) if !read.runner_barrier_current_v9())
        {
            return Err(safe());
        }
        if let Some(ack) = pending {
            match ack {
                HistoryAckV6::Dispatch(ack) => journal
                    .acknowledge_dispatch_v7(
                        &measured,
                        *ack,
                        closed.identity(),
                        work.cancellation(),
                    )
                    .map(|_| ()),
                HistoryAckV6::Run(ack) => journal.acknowledge_history_acceptance_v5(
                    &measured,
                    *ack,
                    closed.identity(),
                    work.cancellation(),
                ),
                HistoryAckV6::Workflow(ack) => journal
                    .acknowledge_workflow_v6(
                        &measured,
                        *ack,
                        closed.identity(),
                        work.cancellation(),
                    )
                    .map(|_| ()),
                HistoryAckV6::ObservedRunV8(ack) => journal
                    .acknowledge_run_history_v8(
                        &measured,
                        *ack,
                        closed.identity(),
                        work.cancellation(),
                    )
                    .map(|_| ()),
                HistoryAckV6::WorkflowRemoteV9(ack) => journal
                    .acknowledge_workflow_remote_v9(
                        &measured,
                        *ack,
                        closed.identity(),
                        work.cancellation(),
                        &request.workflow_partial_ack_v9,
                    )
                    .map(|successor| {
                        owned_workflow_remote_v9 = successor;
                    }),
            }
            .map_err(|_| safe())?;
        }
        // LAST gh was final UID. Only bounded identity replacement/moves and the
        // explicitly prepared SAME Immediate ACK commit follow, no body rebuild.
        bytes = bytes.saturating_sub(
            workspace.identity.host.capacity()
                + workspace.identity.login.capacity()
                + workspace.identity.gh_version.capacity(),
        );
        workspace.identity = closed.identity().clone();
        bytes = bytes.saturating_add(
            workspace.identity.host.capacity()
                + workspace.identity.login.capacity()
                + workspace.identity.gh_version.capacity(),
        );
        if bytes > PREPARED_LIMIT_V1 {
            return Err(safe());
        }
        if workflow_remote
            && request
                .held_workflow_v9
                .checked_add(bytes)
                .is_none_or(|n| n > PREPARED_LIMIT_V1)
        {
            return Err(safe());
        }
        clock(work)?;
        Ok(PreparedJournalV5 {
            work: work.clone(),
            closed: Arc::new(ClosedJournalViewV5 {
                ticket: request.ticket.clone(),
                snapshot,
                observed,
                acknowledged: acceptance || workflow_ack || run_ack_v8 || workflow_remote_ack,
                run_inspection_v8,
                owned_workflow_remote_v9,
                bytes_v9: bytes,
            }),
            workspace,
            bytes,
            workflow_document_v9,
        })
    })();
    result.map_err(|_|"Original Actions history unavailable/current scope changed; protected metadata retained, no fallback or retry".into())
}
pub(crate) fn publish(
    app: &mut App,
    owner: &Arc<ScopedActionsOwnerV1>,
    prepared: PreparedJournalV5,
) -> bool {
    if !owner.owns(app) || !prepared.matches(owner.work()) || owner.work.check().is_err() {
        return false;
    }
    if let Some(original) = prepared
        .work
        .journal_v5
        .as_ref()
        .and_then(|request| request.workflow_form_v9.as_ref())
    {
        let Some(document) = prepared.workflow_document_v9 else {
            return false;
        };
        let Some(form) = app.form.as_mut().filter(|form| original.matches(form)) else {
            return false;
        };
        let successor = prepared.closed.owned_workflow_remote_v9.clone();
        if !crate::scoped_actions_workflow_edit_ui_v9::install_history_v9(
            form,
            original,
            prepared.closed,
            document,
            successor,
        ) {
            return false;
        }
        app.loading[View::Actions.index()] = false;
        app.hits.clear();
        app.status="Original workflow remote history • READ/ACK only • effect UNVERIFIED • no retry/resend".into();
        return true;
    }
    let mut state = app
        .scoped_actions_v1
        .take()
        .expect("current original history owner");
    if state.journal_v5.is_none() && state.source.history_pr_v5.is_none() {
        if let Some(old) = app.actions.take() {
            state.journal_back_v5 = Some(JournalBackV5 {
                workspace: Box::new(old),
                bytes: state.retained_bytes,
                matches: state.matches_v2.take(),
            });
        }
    }
    let base = prepared
        .bytes
        .saturating_add(state.journal_back_v5.as_ref().map_or(0, |back| back.bytes));
    while !state.back.is_empty()
        && base.saturating_add(state.back.iter().map(|entry| entry.bytes).sum::<usize>())
            > BACK_LIMIT_V1
    {
        state.back.remove(0);
    }
    if prepared
        .work
        .journal_v5
        .as_ref()
        .is_some_and(|request| request.form.is_some())
    {
        app.form = None;
        app.form_dismissed = false;
        app.overlay = None;
    }
    state.retained_bytes = prepared.bytes;
    state.journal_v5 = Some(prepared.closed);
    state.matches_v2 = None;
    app.context = prepared.workspace.context();
    app.actions = Some(prepared.workspace);
    app.scoped_actions_v1 = Some(state);
    app.loading[View::Actions.index()] = false;
    app.actions_content_focused = true;
    app.hits.clear();
    app.status="Original Actions publication history • READ/ACK only • effect/causality unverified • no remote mutation or restored approval".into();
    true
}
