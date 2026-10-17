//! Actual POST200 returned Run -> SAME Actions read model and owner. A return
//! session is neither a new Source nor publication, search or watch authority.
use super::*;
use crate::{
    actions_write_journal::{ActionsOwnedDispatchHistoryV7, ActionsWriteJournal},
    scoped_actions_dispatch_publication_v7::OpenedDispatchV7,
};

pub(crate) struct DispatchRunSessionV7 {
    opened: Arc<OpenedDispatchV7>,
    owned: Arc<ActionsOwnedDispatchHistoryV7>,
    run: Arc<crate::providers::gh::ScopedActionsRunTargetV1>,
}
pub(crate) struct DispatchRunReturnV7 {
    pub(crate) session: Arc<DispatchRunSessionV7>,
    form: Box<Form>,
    workspace: Box<ActionsWorkspace>,
    page: Arc<ScopedActionsPageV1>,
    bytes: usize,
    /// Conservative <=8MiB previous worker material + actual current editor
    /// capacities (including cleared inputs) + exact original page charge.
    /// This is retention, not a new allocation or proof.
    pub(crate) retained: usize,
}
pub(crate) struct DispatchRunReadRequestV7 {
    session: Arc<DispatchRunSessionV7>,
    page: Arc<ScopedActionsPageV1>,
    opening: Option<Arc<DispatchRequestV7>>,
    run: Arc<crate::providers::gh::ScopedActionsRunTargetV1>,
    job: Option<Arc<crate::providers::gh::ScopedActionsJobTargetV1>>,
    kind: crate::providers::gh::ScopedActionsReadKindV1,
    view: crate::actions_workspace::ActionsView,
    return_bytes: usize,
}
impl DispatchRunReadRequestV7 {
    pub(crate) fn intent(&self) -> &Arc<ScopedDispatchIntentV7> {
        self.session.opened.intent()
    }
    pub(crate) fn page(&self) -> &Arc<ScopedActionsPageV1> {
        &self.page
    }
    pub(crate) fn opening(&self) -> Option<&Arc<DispatchRequestV7>> {
        self.opening.as_ref()
    }
    pub(crate) fn read_kind(&self) -> crate::providers::gh::ScopedActionsReadKindV1 {
        self.kind
    }
    pub(crate) fn validate(&self, database: &Path) -> crate::providers::gh::GithubResult<()> {
        let journal = ActionsWriteJournal::open(database).map_err(|_| failure())?;
        self.session
            .opened
            .validate_history(database, &journal, &self.session.owned)
    }
    pub(crate) fn matches_work(&self, work: &ScopedActionsWorkV1) -> bool {
        Arc::ptr_eq(&work.page, &self.page)
            && Arc::ptr_eq(&work.source, self.intent().source())
            && work
                .run
                .as_ref()
                .is_some_and(|run| Arc::ptr_eq(run, &self.run))
            && match (&work.job, &self.job) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
            && work.requested_view_v2 == Some(self.view)
            && work.query_v2.is_none()
            && work.resource_v2.is_none()
            && work.search_v2.is_none()
            && work.watch_v2.is_none()
            && work.journal_v5.is_none()
    }
    pub(crate) fn overlay_current(&self, app: &App) -> bool {
        if let Some(opening) = &self.opening {
            app.overlay == Some(crate::app::Overlay::Form)
                && !app.form_dismissed
                && app
                    .form
                    .as_ref()
                    .is_some_and(|form| opening.matches_form(form))
                && app
                    .scoped_actions_v1
                    .as_ref()
                    .is_some_and(|state| state.dispatch_run_v7.is_none())
        } else {
            app.overlay.is_none()
                && app.scoped_actions_v1.as_ref().is_some_and(|state| {
                    state
                        .dispatch_run_v7
                        .as_ref()
                        .is_some_and(|view| Arc::ptr_eq(&view.session, &self.session))
                })
        }
    }
}
fn failure() -> crate::providers::gh::GithubError {
    crate::providers::gh::GithubError {
        kind: crate::providers::gh::GithubErrorKind::IdentityChanged,
        message: "Original returned-run/source/history changed; no fallback or write permission"
            .into(),
    }
}
fn capture_work(request: Arc<DispatchRunReadRequestV7>, epoch: u64) -> Arc<ScopedActionsWorkV1> {
    Arc::new(ScopedActionsWorkV1 {
        source: request.intent().source().clone(),
        page: request.page.clone(),
        run: Some(request.run.clone()),
        job: request.job.clone(),
        kind: ScopedActionsRequestKindV1::Publication,
        publication_v3: Some(Arc::new(PublicationWorkV3::Dispatch(Arc::new(
            DispatchWorkV7::Read(request.clone()),
        )))),
        journal_v5: None,
        query_v2: None,
        resource_v2: None,
        search_v2: None,
        watch_v2: None,
        position_v2: None,
        requested_view_v2: Some(request.view),
        deadline: std::time::Instant::now() + std::time::Duration::from_secs(30),
        cancellation: crate::providers::runner::CancellationToken::default(),
        epoch,
        #[cfg(test)]
        after_read_fault_v1: None,
    })
}
pub(crate) fn capture_open(
    form: &Form,
    state: &super::super::ScopedActionsStateV1,
) -> Result<Arc<ScopedActionsWorkV1>, String> {
    let opening = dispatch_ui::capture(form, DispatchPhaseV7::OpenRun)?;
    let run = opening
        .created_run()
        .ok_or("No genuine POST200 returned run")?
        .clone();
    let owned = opening
        .owned_history()
        .ok_or("No actual Dispatch own-result fence")?
        .clone();
    if !owned.matches_returned_run_v7(&run)
        || !Arc::ptr_eq(&state.page, opening.opened().intent().page())
        || !Arc::ptr_eq(&state.source, opening.opened().intent().source())
        || state.dispatch_run_v7.is_some()
    {
        return Err("Original Dispatch/run/receipt/source differ; no read requested".into());
    }
    if form.fields.len() > 53 {
        return Err("Original Dispatch field schema exceeded".into());
    }
    let editor_bytes = dispatch::field_bytes(&form.fields)
        .saturating_add(form.title.capacity())
        .saturating_add(form.error.capacity());
    let return_bytes = state
        .retained_bytes
        .checked_add(crate::scoped_actions_v1::PREPARED_LIMIT_V1)
        .and_then(|bytes| bytes.checked_add(editor_bytes))
        .filter(|bytes| *bytes <= crate::scoped_actions_v1::BACK_LIMIT_V1)
        .ok_or("Original protected return exceeds bounded retention")?;
    let session = Arc::new(DispatchRunSessionV7 {
        opened: opening.opened().clone(),
        owned,
        run: run.clone(),
    });
    let request = Arc::new(DispatchRunReadRequestV7 {
        session,
        page: state.page.clone(),
        opening: Some(opening),
        run,
        job: None,
        kind: crate::providers::gh::ScopedActionsReadKindV1::Attempt,
        view: crate::actions_workspace::ActionsView::Run,
        return_bytes,
    });
    Ok(capture_work(request, state.epoch.wrapping_add(1)))
}
/// Read-only policy is prepared in the worker, before final SQL/principal.
/// Disabled controls retain the SAME key/click explanation; no legacy fallthrough.
pub(crate) fn prepare_policy(workspace: &mut ActionsWorkspace, pending: bool) {
    use crate::actions_workspace::ActionsView as V;
    crate::actions_workspace::scoped_read_v1::prepare_policy(workspace, pending);
    for control in &mut workspace.controls {
        let allowed = matches!(
            control.action,
            ActionsAction::Back
                | ActionsAction::Refresh
                | ActionsAction::View(V::Run | V::Jobs)
                | ActionsAction::OpenJob(_)
                | ActionsAction::Logs(Some(_))
                | ActionsAction::Annotations(_)
        );
        control.disabled_reason = if !allowed {
            Some("Returned Dispatch run is READ ONLY. Close back to protected receipt; use independently fresh normal source/discovery for writers, resources, Search or Watch. No old receipt grants permission.".into())
        } else if pending && !matches!(control.action, ActionsAction::Back) {
            Some("Wait or cancel this exact original returned-run read".into())
        } else {
            None
        };
        if matches!(control.action, ActionsAction::Back) {
            control.label = "[b Back to Dispatch receipt]".into();
        }
    }
}
pub(crate) fn intercept(app: &mut App, action: &ActionsAction) -> bool {
    let Some(state) = app
        .scoped_actions_v1
        .as_ref()
        .filter(|state| state.dispatch_run_v7.is_some())
    else {
        return false;
    };
    if app.view != View::Actions
        || app.overlay.is_some()
        || !super::super::current(app, state)
        || super::super::newer_foreign_intent(app)
    {
        return true;
    }
    let busy = app.scoped_actions_work_v1.is_some() || app.scoped_actions_owner_v1.is_some();
    if *action == ActionsAction::Back {
        if busy {
            let work = match (
                app.scoped_actions_work_v1.as_ref(),
                app.scoped_actions_owner_v1.as_ref(),
            ) {
                (Some(work), Some(owner)) if Arc::ptr_eq(work, owner.work()) => Some(work),
                (Some(_), Some(_)) => None,
                (Some(work), None) => Some(work),
                (None, Some(owner)) => Some(owner.work()),
                _ => None,
            };
            if work.is_some_and(|work| matches!(work.publication_v3.as_deref(),Some(PublicationWorkV3::Dispatch(d))if matches!(&**d,DispatchWorkV7::Read(r)if r.overlay_current(app)&&r.matches_work(work)))
                &&work.epoch==state.epoch&&Arc::ptr_eq(&work.page,&state.page)&&Arc::ptr_eq(&work.source,&state.source)) {
                super::super::cancel_current(app);
            }
        } else if !app.loading[View::Actions.index()] && !app.scoped_actions_cancel_v1 {
            let state = app
                .scoped_actions_v1
                .as_mut()
                .expect("actual checked returned-run state");
            let saved = state.dispatch_run_v7.take().expect("checked return");
            state.page = saved.page;
            state.retained_bytes = saved.bytes;
            state.matches_v2 = None;
            state.back.clear();
            state.epoch = state.epoch.wrapping_add(1);
            app.actions = Some(*saved.workspace);
            app.form = Some(*saved.form);
            app.overlay = Some(crate::app::Overlay::Form);
            app.form_dismissed = false;
            app.status="Original protected Dispatch receipt restored • pure Back • no resend, source renewal or guessed run".into();
            app.hits.clear();
        }
        return true;
    }
    if busy || app.loading[View::Actions.index()] || app.scoped_actions_cancel_v1 {
        return true;
    }
    let session = state
        .dispatch_run_v7
        .as_ref()
        .expect("checked return")
        .session
        .clone();
    let run = state
        .page
        .run()
        .filter(|run| run.same_watch_pin_v2(&session.run))
        .cloned();
    let Some(run) = run else { return true };
    use crate::actions_workspace::ActionsView as V;
    let (kind, view, job) = match action {
        ActionsAction::View(V::Run) => (
            crate::providers::gh::ScopedActionsReadKindV1::Attempt,
            V::Run,
            None,
        ),
        ActionsAction::View(V::Jobs) => (
            crate::providers::gh::ScopedActionsReadKindV1::Attempt,
            V::Jobs,
            None,
        ),
        ActionsAction::OpenJob(id) => {
            let Some(job) = state.page.job(*id) else {
                return true;
            };
            (
                crate::providers::gh::ScopedActionsReadKindV1::Attempt,
                V::Jobs,
                Some(job),
            )
        }
        ActionsAction::Logs(Some(id)) => {
            let Some(job) = state.page.job(*id) else {
                return true;
            };
            (
                crate::providers::gh::ScopedActionsReadKindV1::Logs,
                V::Logs,
                Some(job),
            )
        }
        ActionsAction::Annotations(id) => {
            let Some(job) = state.page.job(*id) else {
                return true;
            };
            (
                crate::providers::gh::ScopedActionsReadKindV1::Annotations,
                V::Annotations,
                Some(job),
            )
        }
        ActionsAction::Refresh => {
            let view = state.page.view();
            let job = state.page.selected_job_for_publication_v4();
            (
                if view == V::Logs {
                    crate::providers::gh::ScopedActionsReadKindV1::Logs
                } else if view == V::Annotations {
                    crate::providers::gh::ScopedActionsReadKindV1::Annotations
                } else {
                    crate::providers::gh::ScopedActionsReadKindV1::Attempt
                },
                view,
                job,
            )
        }
        ActionsAction::Select(_) | ActionsAction::ControlsPage(_) => return false,
        _ => {
            super::super::explain(app,"Returned Dispatch run is read only; resources/Search/Watch/Compose require independently fresh normal discovery. Back keeps the original protected receipt; nothing was requested.");
            return true;
        }
    };
    let request = Arc::new(DispatchRunReadRequestV7 {
        session,
        page: state.page.clone(),
        opening: None,
        run,
        job,
        kind,
        view,
        return_bytes: 0,
    });
    let work = capture_work(request, state.epoch.wrapping_add(1));
    let state = app.scoped_actions_v1.as_mut().expect("checked source");
    state.epoch = work.epoch;
    app.scoped_actions_work_v1 = Some(work);
    app.hits.clear();
    app.status =
        "Reading actual returned Dispatch run • same original source/receipt • no mutation".into();
    true
}
pub(crate) fn publish(
    app: &mut App,
    owner: &Arc<ScopedActionsOwnerV1>,
    request: &Arc<DispatchRunReadRequestV7>,
    prepared: crate::scoped_actions_v1::PreparedScopedActionsV1,
) -> bool {
    if !owner.owns(app)
        || !request.matches_work(owner.work())
        || !request.overlay_current(app)
        || !prepared.matches(owner.work())
        || owner.work().check().is_err()
    {
        return false;
    }
    if request.return_bytes > crate::scoped_actions_v1::BACK_LIMIT_V1 {
        return false;
    }
    if request.opening.is_some() {
        // Everything above was checked while the exact old Form/workspace still
        // existed. This is the one admitted publication; no intermediate source.
        let Some(mut form) = app.form.take() else {
            return false;
        };
        let Some(workspace) = app.actions.take() else {
            app.form = Some(form);
            return false;
        };
        form.submitting = false;
        form.submit_requested = false;
        form.preview_requested = false;
        let state = app.scoped_actions_v1.as_mut().expect("checked owner");
        state.dispatch_run_v7 = Some(DispatchRunReturnV7 {
            session: request.session.clone(),
            form: Box::new(form),
            workspace: Box::new(workspace),
            page: state.page.clone(),
            bytes: state.retained_bytes,
            retained: request.return_bytes,
        });
        state.back.clear();
        app.overlay = None;
        app.form_dismissed = false;
    }
    super::super::publish_admitted_v7(app, owner, prepared);
    app.status="Actual returned Dispatch run • Run/Jobs/Logs/Annotations read only • no completion/causal/write claim • b restores protected receipt".into();
    true
}
/// Resize retires only the same original read, even when already expired.
/// Foreign generation/work/Form/session/source refusal has ZERO App effects.
pub(crate) fn resize_pending(app: &mut App) -> bool {
    let read=app.scoped_actions_work_v1.as_ref().or_else(||app.scoped_actions_owner_v1.as_ref().map(|owner|owner.work()))
        .is_some_and(|work|matches!(work.publication_v3.as_deref(),Some(PublicationWorkV3::Dispatch(d))if matches!(&**d,DispatchWorkV7::Read(_))));
    if !read {
        return false;
    }
    let applicable=super::super::current_retirement_work_v3(app).is_some_and(|work|
        matches!(work.publication_v3.as_deref(),Some(PublicationWorkV3::Dispatch(d))if matches!(&**d,DispatchWorkV7::Read(request)if request.matches_work(work)&&request.overlay_current(app))));
    if applicable {
        // Initial F11 still owns the exact protected receipt Form. The generic
        // read cancellation below retires its token/page, not that Form's busy
        // flag. Retire only this actual matching presentation before cancellation;
        // an overlay-free continuation or a newer Form is never touched.
        let opening = super::super::current_retirement_work_v3(app).and_then(|work| {
            let PublicationWorkV3::Dispatch(dispatch) = work.publication_v3.as_deref()? else {
                return None;
            };
            let DispatchWorkV7::Read(request) = &**dispatch else {
                return None;
            };
            request.opening().cloned()
        });
        if let (Some(opening), Some(form)) = (opening, app.form.as_mut()) {
            opening.retire_busy(form);
        }
        super::super::cancel_current(app);
    }
    true
}
