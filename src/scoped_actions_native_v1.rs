//! Native catalogue Actions policy over the existing Actions model/rendering.
use crate::scoped_actions_read_v2::{ActionsLoadedMatchesV2, ActionsReadWatchV2};
use crate::{
    actions_workspace::{ActionsAction, ActionsView, ActionsWorkspace},
    app::{Action, App, View},
    providers::runner::CancellationToken,
    scoped_actions_v1::*,
};
use std::sync::Arc;
#[path = "scoped_actions_journal_v5.rs"]
pub mod journal_v5;
#[path = "scoped_actions_publication_native_v3.rs"]
pub mod publication_v3;
#[path = "scoped_actions_read_ui_v2.rs"]
pub mod read_ui_v2;
struct SnapshotV1 {
    page: Arc<ScopedActionsPageV1>,
    workspace: Box<ActionsWorkspace>,
    bytes: usize,
    matches_v2: Option<Arc<ActionsLoadedMatchesV2>>,
}
pub struct ScopedActionsStateV1 {
    dispatch_run_v7: Option<publication_v3::dispatch_v7::run_read_v7::DispatchRunReturnV7>,
    journal_v5: Option<Arc<journal_v5::ClosedJournalViewV5>>,
    journal_back_v5: Option<journal_v5::JournalBackV5>,
    source: Arc<ScopedActionsSourceV1>,
    page: Arc<ScopedActionsPageV1>,
    retained_bytes: usize,
    back: Vec<SnapshotV1>,
    epoch: u64,
    entry_page_v2: std::sync::Weak<ScopedActionsPageV1>,
    matches_v2: Option<Arc<ActionsLoadedMatchesV2>>,
    watch_v2: Option<Arc<ActionsReadWatchV2>>,
    editor_v2: Option<read_ui_v2::ReadEditorV2>,
}
pub struct ScopedActionsOwnerV1 {
    work: Arc<ScopedActionsWorkV1>,
    publication_v3: std::sync::Mutex<Option<crate::drafts::DraftPublicationFenceV2>>,
    workflow_edit_read_v9: std::sync::Mutex<
        Option<crate::scoped_actions_workflow_edit_v9::draft_cutoff_v9::WorkflowEditReadCutoffV9>,
    >,
}
#[cfg(test)]
impl ScopedActionsStateV1 {
    pub(crate) fn source_for_test_v107(&self) -> &Arc<ScopedActionsSourceV1> {
        &self.source
    }
    pub(crate) fn page_for_test_v107(&self) -> &Arc<ScopedActionsPageV1> {
        &self.page
    }
}
impl ScopedActionsOwnerV1 {
    pub(crate) fn new(work: Arc<ScopedActionsWorkV1>) -> Result<Arc<Self>, String> {
        work.check()?;
        Ok(Arc::new(Self {
            work,
            publication_v3: std::sync::Mutex::new(None),
            workflow_edit_read_v9: std::sync::Mutex::new(None),
        }))
    }
    pub fn cancellation(&self) -> &CancellationToken {
        self.work.cancellation()
    }
    pub fn deadline(&self) -> std::time::Instant {
        self.work.deadline()
    }
    pub(crate) fn work(&self) -> &Arc<ScopedActionsWorkV1> {
        &self.work
    }
    pub(crate) fn remote_owners(
        &self,
    ) -> (
        Arc<crate::active_remote_scope_v1::ActiveRemoteScopeV1>,
        Arc<crate::storage::CatalogueIntentSource>,
    ) {
        (
            self.work.source.scope().clone(),
            self.work.source.original().clone(),
        )
    }
    pub fn owns(&self, app: &App) -> bool {
        app.view == View::Actions
            && publication_v3::overlay_current(app, &self.work)
            && !publication_v3::foreign_intent(app, &self.work)
            && app.scoped_actions_v1.as_ref().is_some_and(|state| {
                current(app, state)
                    && Arc::ptr_eq(&state.source, &self.work.source)
                    && Arc::ptr_eq(&state.page, &self.work.page)
                    && state.epoch == self.work.epoch
                    && self.work.watch_v2.as_ref().is_none_or(|watch| {
                        state
                            .watch_v2
                            .as_ref()
                            .is_some_and(|current| Arc::ptr_eq(current, watch))
                            && watch.matches(&state.page)
                    })
            })
            && app
                .scoped_actions_work_v1
                .as_ref()
                .is_none_or(|work| Arc::ptr_eq(work, &self.work))
            && !app.scoped_actions_cancel_v1
            && app
                .scoped_actions_owner_v1
                .as_ref()
                .is_some_and(|owner| Arc::ptr_eq(&owner.work, &self.work))
    }
}
fn current(app: &App, state: &ScopedActionsStateV1) -> bool {
    app.remote_repository.is_none()
        && state
            .source
            .pr_origin_v2()
            .is_none_or(|origin| origin.current(app))
        && state
            .source
            .history_pr_v5
            .as_ref()
            .is_none_or(|origin| origin.current(app))
        && app.github.as_ref().is_none_or(|github| {
            github.scope().ok().as_ref() == Some(state.source.original().scope())
        })
        && app
            .remote_scope_source
            .as_ref()
            .is_some_and(|source| Arc::ptr_eq(source, state.source.original()))
        && app
            .remote_scope
            .as_ref()
            .is_some_and(|scope| Arc::ptr_eq(scope, state.source.scope()))
        && app.actions.as_ref().is_some_and(|w| {
            crate::storage::Scope::github(&w.identity.host, w.identity.account_id)
                .ok()
                .as_ref()
                == Some(state.source.original().scope())
        })
}
/// Pure pre-submission input admission, BEFORE touching any existing physical
/// slot/logical owner/generation/status/loading. A live unrelated Actions job
/// cannot be canceled by an older captured read, including expired/canceled
/// work whose source/page is otherwise still retained.
pub(crate) fn current_input(app: &App, work: &Arc<ScopedActionsWorkV1>) -> bool {
    app.view == View::Actions
        && publication_v3::overlay_current(app, work)
        && !app.loading[View::Actions.index()]
        && !app.scoped_actions_cancel_v1
        && app.scoped_actions_owner_v1.is_none()
        && !publication_v3::foreign_intent(app, work)
        && app
            .scoped_actions_work_v1
            .as_ref()
            .is_none_or(|current| Arc::ptr_eq(current, work))
        && app.scoped_actions_v1.as_ref().is_some_and(|state| {
            current(app, state)
                && Arc::ptr_eq(&state.source, &work.source)
                && Arc::ptr_eq(&state.page, &work.page)
                && state.epoch == work.epoch
                && work.watch_v2.as_ref().is_none_or(|watch| {
                    state
                        .watch_v2
                        .as_ref()
                        .is_some_and(|current| Arc::ptr_eq(current, watch))
                        && watch.active()
                        && watch.matches(&state.page)
                })
        })
}
pub fn newer_foreign_intent(app: &App) -> bool {
    newer_foreign_without_form_v3(app)
        || app
            .form
            .as_ref()
            .is_some_and(|form| form.preview_requested || form.submit_requested)
}
fn newer_foreign_without_form_v3(app: &App) -> bool {
    app.notifications_work_v63.is_some()
        || app.notifications_owner_v63.is_some()
        || app.notifications_cancel_v63
        || app.scoped_pr_collaboration_work_v17.is_some()
        || app.scoped_pr_collaboration_owner_v17.is_some()
        || app.scoped_pr_collaboration_cancel_v17
        || app.personal_pr_entry_work_v13.is_some()
        || app.personal_pr_entry_owner_v13.is_some()
        || app.personal_pr_entry_cancel_v13
        || app.pr_native_diff_work_v1.is_some()
        || app.pr_native_diff_cancel_v1
        || app.pr_native_diff_owner_v1.is_some()
        || app.scoped_threads_owner_v1.is_some()
        || app.scoped_reviews_owner_v2.is_some()
        || app.scoped_inline_owner_v1.is_some()
        || app.pr_check_continuation_owner_v2.is_some()
        || app.scoped_threads_work_v1.is_some()
        || app.scoped_threads_cancel_v1
        || app.scoped_reviews_work_v2.is_some()
        || app.scoped_reviews_cancel_v2
        || app.scoped_inline_work_v1.is_some()
        || app.scoped_inline_cancel_v1
        || app.original_tree_work_v1.is_some()
        || app.original_tree_cancel_v1
        || app.original_diff_work_v1.is_some()
        || app.original_diff_cancel_v1
        || app.original_stash_recovery_request_v1.is_some()
        || app.original_stash_recovery_cancel_requested_v1
        || app.graph_reread_request_v3.is_some()
        || app.related_pr_requested_v1
        || app.related_pr_cancel_v1
        || app.repository_load_requested
        || app.repository_action.is_some()
        || app.navigation_restore.is_some()
        || app.navigation_restore_inflight
        || app.local_actor_requested
        || app.local_actor_cancel_requested
        || app.github_action.is_some()
        || app.pr_action.is_some()
        || app.pr_checks_request
        || app.pr_check_choice_request.is_some()
        || app.pr_check_continuation_request_v2.is_some()
        || app.pr_check_continuation_cancel_v2
        || app.pr_checks_cancel_actions
        || app.scoped_pr_tab_cancel_requested
        || app.remote_repository_cancel_requested
        || app.catalogue_cancel_requested
        || app.actions_action.is_some()
        || app.actions_load_requested
        || app.search_action.is_some()
        || app.search_load_requested
        || app.metrics_action.is_some()
        || app.metrics_load_requested
        || app.source_action.is_some()
        || app.corpus_requested
        || app.refresh_requested
        || app.cancel_requested
        || app.quit
        || app
            .catalogue
            .as_ref()
            .is_some_and(|modal| modal.request.is_some())
}
pub fn install_initial(app: &mut App, state: InitialScopedActionsV1) {
    app.remote_scope = Some(state.source.scope().clone());
    app.remote_scope_source = Some(state.source.original().clone());
    app.context = state.workspace.context();
    app.actions = Some(state.workspace);
    // Preserve the application's ordinary focused-view/focus retirement
    // semantics. This does not request a legacy load because the genuine
    // worker-prepared Actions workspace is already installed.
    app.dispatch(Action::View(View::Actions));
    app.scoped_actions_v1 = Some(ScopedActionsStateV1 {
        dispatch_run_v7: None,
        journal_v5: None,
        journal_back_v5: None,
        source: state.source,
        entry_page_v2: Arc::downgrade(&state.page),
        page: state.page,
        retained_bytes: state.retained_bytes,
        back: vec![],
        epoch: 0,
        matches_v2: None,
        watch_v2: None,
        editor_v2: None,
    });
    app.scoped_actions_work_v1 = None;
    app.scoped_actions_owner_v1 = None;
    app.scoped_actions_cancel_v1 = false;
    app.actions_action = None;
    app.actions_load_requested = false;
    app.view = View::Actions;
    app.overlay = None;
    app.actions_content_focused = true;
    app.loading[View::Actions.index()] = false;
    app.hits.clear();
    app.status="Actions • focused view • original numeric repository verified • retained catalogue • non-atomic observed read".into();
}
pub fn retire(app: &mut App) {
    if let Some(state) = &mut app.scoped_actions_v1 {
        if let Some(watch) = state.watch_v2.take() {
            watch.stop();
        }
        state.editor_v2 = None;
    }
    if app.overlay == Some(crate::app::Overlay::ScopedActionsReadV2) {
        app.overlay = None;
    }
    if let Some(work) = app.scoped_actions_work_v1.take() {
        work.cancellation().cancel();
    }
    if let Some(owner) = app.scoped_actions_owner_v1.take() {
        owner.cancellation().cancel();
    }
    app.scoped_actions_v1 = None;
    app.scoped_actions_cancel_v1 = true;
}
pub fn synchronize(app: &mut App) {
    if app
        .scoped_actions_v1
        .as_ref()
        .is_some_and(|state| !current(app, state))
    {
        retire(app);
    }
    publication_v3::workflow_edit_v9::observe_cleanup_wait_v9(app);
}
fn explain(app: &mut App, reason: impl Into<String>) {
    app.dispatch(Action::Explain(reason.into()));
}
fn prepare_policy(app: &mut App, pending: bool) {
    let source = app
        .scoped_actions_v1
        .as_ref()
        .map(|state| state.source.clone());
    if let (Some(source), Some(workspace)) = (source, app.actions.as_mut()) {
        if app
            .scoped_actions_v1
            .as_ref()
            .is_some_and(|state| state.dispatch_run_v7.is_some())
        {
            publication_v3::dispatch_v7::run_read_v7::prepare_policy(workspace, pending);
        } else if app
            .scoped_actions_v1
            .as_ref()
            .is_some_and(|state| state.journal_v5.is_some() || state.source.history_pr_v5.is_some())
        {
            journal_v5::prepare_policy(workspace, pending);
        } else {
            source.prepare_policy(workspace, pending);
        }
    }
}
fn cancel_current(app: &mut App) {
    if let Some(state) = &mut app.scoped_actions_v1 {
        if let Some(watch) = state.watch_v2.take() {
            watch.stop();
        }
    }
    if let Some(workspace) = &mut app.actions {
        workspace.watching = false;
    }
    if let Some(work) = app.scoped_actions_work_v1.take() {
        work.cancellation().cancel();
    }
    if let Some(owner) = app.scoped_actions_owner_v1.take() {
        owner.cancellation().cancel();
    }
    app.scoped_actions_cancel_v1 = true;
    if let Some(state) = &mut app.scoped_actions_v1 {
        state.epoch = state.epoch.wrapping_add(1);
    }
    prepare_policy(app, false);
    app.hits.clear();
    app.status = "Original Actions read canceled • retained page unchanged • no mutation".into();
}
/// All keys AND hit actions arrive here before generic Actions/forms handling.
pub fn intercept(app: &mut App, action: &ActionsAction) -> bool {
    synchronize(app);
    if publication_v3::workflow_edit_v9::intercept(app, action) {
        return true;
    }
    if publication_v3::dispatch_v7::run_read_v7::intercept(app, action) {
        return true;
    }
    if journal_v5::intercept(app, action) {
        return true;
    }
    if publication_v3::intercept(app, action) {
        return true;
    }
    if app.scoped_actions_v1.is_none()
        && app
            .pr_checks_route
            .as_ref()
            .is_some_and(|route| route.scoped_v1())
        && crate::pr_checks_native::scoped::actions_origin_v2::run_command_entry_v4(action)
    {
        // Pure admission MUST precede source seeding, cancellation, epoch,
        // hits/status/loading or queue effects. A generic live Actions slot
        // and independently newer source/intent must remain untouched.
        let eligible = app.view == View::Actions
            && app.overlay.is_none()
            && !app.loading[View::Actions.index()]
            && app.scoped_actions_work_v1.is_none()
            && app.scoped_actions_owner_v1.is_none()
            && !app.scoped_actions_cancel_v1
            && !newer_foreign_intent(app);
        if !eligible {
            return true;
        }
        let ActionsAction::Compose(command) = action else {
            unreachable!("closed run entry");
        };
        let prepared =
            crate::pr_checks_native::scoped::actions_origin_v2::ScopedPrActionsOriginV2::capture(
                app,
            )
            .and_then(|origin| {
                retained_pr_route_v2(
                    origin,
                    app.actions
                        .as_ref()
                        .ok_or("Original Actions workspace unavailable")?,
                )
            })
            .and_then(|(source, page, retained_bytes)| {
                let work = publication_v3::prepare_initial_run_v4(page.clone(), *command)?;
                Ok((source, page, retained_bytes, work))
            });
        // Refusal deliberately does not replace another overlay/status/hits.
        if let Ok((source, page, retained_bytes, work)) = prepared {
            app.scoped_actions_v1 = Some(ScopedActionsStateV1 {
                dispatch_run_v7: None,
                journal_v5: None,
                journal_back_v5: None,
                source,
                entry_page_v2: Arc::downgrade(&page),
                page,
                retained_bytes,
                back: vec![],
                epoch: work.epoch,
                matches_v2: None,
                watch_v2: None,
                editor_v2: None,
            });
            app.scoped_actions_work_v1 = Some(work);
            app.hits.clear();
            app.status = format!(
                "Opening original {} intent • read only • original30s includes queue • no POST",
                command.label()
            );
        }
        return true;
    }
    if app.scoped_actions_v1.is_none()
        && app
            .pr_checks_route
            .as_ref()
            .is_some_and(|route| route.scoped_v1())
        && crate::pr_checks_native::scoped::actions_origin_v2::resource_entry_action(action)
    {
        if app.view != View::Actions
            || app.overlay.is_some()
            || app.loading[View::Actions.index()]
            || newer_foreign_intent(app)
        {
            explain(app,"Original PR Actions resource entry is not current; no request, fallback or cancellation");
            return true;
        }
        let seed =
            crate::pr_checks_native::scoped::actions_origin_v2::ScopedPrActionsOriginV2::capture(
                app,
            )
            .and_then(|origin| {
                retained_pr_route_v2(
                    origin,
                    app.actions
                        .as_ref()
                        .ok_or("Original Actions workspace unavailable")?,
                )
            });
        match seed {
            Ok((source, page, retained_bytes)) => {
                app.scoped_actions_v1 = Some(ScopedActionsStateV1 {
                    dispatch_run_v7: None,
                    journal_v5: None,
                    journal_back_v5: None,
                    source,
                    entry_page_v2: Arc::downgrade(&page),
                    page,
                    retained_bytes,
                    back: vec![],
                    epoch: 0,
                    matches_v2: None,
                    watch_v2: None,
                    editor_v2: None,
                })
            }
            Err(reason) => {
                explain(app, reason);
                return true;
            }
        }
    }
    let Some(state) = app.scoped_actions_v1.as_ref() else {
        return false;
    };
    if !current(app, state) {
        return false;
    }
    if read_ui_v2::intercept(app, action) {
        return true;
    }
    if let ActionsAction::Unavailable(reason) = action {
        explain(app, reason.clone());
        return true;
    }
    if matches!(action, ActionsAction::Back) {
        if app.scoped_actions_work_v1.is_some() || app.scoped_actions_owner_v1.is_some() {
            cancel_current(app);
            return true;
        }
        let mut state = app.scoped_actions_v1.take().expect("current");
        if let Some(snapshot) = state.back.pop() {
            state.page = snapshot.page;
            state.matches_v2 = snapshot.matches_v2;
            state.retained_bytes = snapshot.bytes;
            state.epoch = state.epoch.wrapping_add(1);
            app.actions = Some(*snapshot.workspace);
            app.context = app.actions.as_ref().expect("retained").context();
            app.hits.clear();
            app.status = "Original Actions snapshot restored • no fresh request or mutation".into();
            if state.source.pr_origin_v2().is_some()
                && state
                    .entry_page_v2
                    .upgrade()
                    .is_some_and(|entry| Arc::ptr_eq(&state.page, &entry))
            {
                // Restore the actual untouched PR route and its workspace as
                // one unit. Its old log indices must not control A2 rows.
                if let Some(watch) = state.watch_v2.take() {
                    watch.stop();
                }
                return true;
            }
        } else if state.source.pr_origin_v2().is_some() {
            crate::pr_checks_native::dispatch(app, crate::pr_checks_native::PrChecksUiAction::Back);
            return true;
        } else {
            explain(app,"No earlier Actions snapshot; use the main view keys to leave. No query or mutation sent");
        }
        app.scoped_actions_v1 = Some(state);
        return true;
    }
    if let ActionsAction::Select(index) = action {
        let changed = app
            .actions
            .as_ref()
            .is_some_and(|w| w.selected != (*index).min(w.rows.len().saturating_sub(1)));
        if changed
            && (app.scoped_actions_work_v1.is_some() || app.scoped_actions_owner_v1.is_some())
        {
            cancel_current(app);
        }
        if let Some(w) = &mut app.actions {
            w.selected = (*index).min(w.rows.len().saturating_sub(1));
            if w.selected < w.top {
                w.top = w.selected;
            } else if w.selected >= w.top + app.actions_visible_rows.max(1) {
                w.top = w.selected + 1 - app.actions_visible_rows.max(1);
            }
        }
        app.actions_content_focused = true;
        return true;
    }
    if matches!(action, ActionsAction::Repository(0)) {
        return true;
    }
    if app.scoped_actions_work_v1.is_some() || app.scoped_actions_owner_v1.is_some() {
        explain(
            app,
            "Original scoped Actions read pending; Cancel read or wait • no other request sent",
        );
        return true;
    }
    if app.loading[View::Actions.index()] {
        explain(app,"Another Actions operation is still running; wait or cancel that operation before opening an original read • no request sent");
        return true;
    }
    let mut state = app.scoped_actions_v1.take().expect("current");
    // Cached Runs tab is a pure original-page return; absence needs explicit
    // fresh catalogue work, never a broader legacy repository list.
    if matches!(action, ActionsAction::View(ActionsView::Runs)) {
        if let Some(index) = state
            .back
            .iter()
            .rposition(|s| s.page.view() == ActionsView::Runs)
        {
            let snapshot = state.back.remove(index);
            state.back.truncate(index);
            state.page = snapshot.page;
            state.matches_v2 = snapshot.matches_v2;
            state.retained_bytes = snapshot.bytes;
            state.epoch = state.epoch.wrapping_add(1);
            app.actions = Some(*snapshot.workspace);
            app.scoped_actions_v1 = Some(state);
            app.hits.clear();
            return true;
        }
    }
    state.epoch = state.epoch.wrapping_add(1);
    if let Some(watch) = state.watch_v2.take() {
        watch.stop();
    }
    let work = ScopedActionsWorkV1::capture(state.page.clone(), action, state.epoch);
    app.scoped_actions_v1 = Some(state);
    match work {
        Ok(work) => {
            app.scoped_actions_work_v1 = Some(work);
            app.scoped_actions_cancel_v1 = false;
            prepare_policy(app, true);
            app.status="Original Actions read queued • 30s includes queue wait • Back cancels • no mutation".into();
            app.hits.clear();
        }
        Err(reason) => explain(
            app,
            format!(
                "{reason} • {}",
                crate::actions_workspace::scoped_read_v1::UNAVAILABLE
            ),
        ),
    }
    true
}
pub fn take_work(app: &mut App) -> Option<Arc<ScopedActionsWorkV1>> {
    let work = app.scoped_actions_work_v1.take()?;
    if app.scoped_actions_v1.as_ref().is_none_or(|state| {
        !current(app, state) || !Arc::ptr_eq(&state.page, &work.page) || state.epoch != work.epoch
    }) {
        work.cancellation().cancel();
        return None;
    }
    if work.check().is_err() {
        work.cancellation().cancel();
        app.status =
            "Queued original Actions read expired; retained snapshot, no request sent".into();
        prepare_policy(app, false);
        return None;
    }
    Some(work)
}
pub fn submission_failed(app: &mut App, work: &Arc<ScopedActionsWorkV1>) {
    if publication_v3::submission_failed(app, work) {
        return;
    }
    if app.scoped_actions_v1.as_ref().is_some_and(|state| {
        current(app, state) && Arc::ptr_eq(&state.page, &work.page) && state.epoch == work.epoch
    }) {
        prepare_policy(app, false);
        app.status =
            "Original Actions queue/provider unavailable • no read or mutation submitted".into();
        app.hits.clear();
    }
    work.cancellation().cancel();
}
pub(crate) fn publish(
    app: &mut App,
    owner: &Arc<ScopedActionsOwnerV1>,
    prepared: PreparedScopedActionsV1,
) -> bool {
    if !owner.owns(app) || !prepared.matches(owner.work()) || owner.work.check().is_err() {
        return false;
    }
    publish_admitted_v7(app, owner, prepared);
    true
}
/// Mechanical publication body shared by ordinary reads and the closed
/// returned-Dispatch read after their distinct exact-current owner admission.
fn publish_admitted_v7(
    app: &mut App,
    owner: &Arc<ScopedActionsOwnerV1>,
    prepared: PreparedScopedActionsV1,
) {
    let mut state = app.scoped_actions_v1.take().expect("checked");
    let transient_projection =
        owner.work.kind == ScopedActionsRequestKindV1::Search || owner.work.watch_v2.is_some();
    if let Some(mut old) = app.actions.take().filter(|_| !transient_projection) {
        if state.dispatch_run_v7.is_some() {
            publication_v3::dispatch_v7::run_read_v7::prepare_policy(&mut old, false)
        } else {
            state.source.prepare_policy(&mut old, false)
        };
        let mut retained = state
            .back
            .iter()
            .map(|s| s.bytes)
            .sum::<usize>()
            .saturating_add(state.retained_bytes)
            .saturating_add(
                state
                    .dispatch_run_v7
                    .as_ref()
                    .map_or(0, |view| view.retained),
            );
        while !state.back.is_empty()
            && (state.back.len() >= 3 || retained > BACK_LIMIT_V1.saturating_sub(4096))
        {
            // Prefer keeping the original catalogue anchor while evicting a
            // bounded intermediate page. Never exceed total retention to do so.
            let index = if state.back.len() > 1 && state.back[0].page.view() == ActionsView::Runs {
                1
            } else {
                0
            };
            let removed = state.back.remove(index);
            retained = retained.saturating_sub(removed.bytes);
        }
        if state.retained_bytes <= BACK_LIMIT_V1
            && (state.dispatch_run_v7.is_none() || retained <= BACK_LIMIT_V1.saturating_sub(4096))
        {
            state.back.push(SnapshotV1 {
                page: state.page.clone(),
                workspace: Box::new(old),
                bytes: state.retained_bytes,
                matches_v2: state.matches_v2.take(),
            });
        }
    }
    state.page = prepared.page;
    state.retained_bytes = prepared.retained_bytes;
    state.matches_v2 = prepared.matches_v2;
    if prepared
        .watch_progress_v2
        .is_some_and(|(ticks, _)| ticks >= 12)
    {
        if let Some(watch) = state.watch_v2.take() {
            watch.stop();
        }
    }
    app.context = prepared.workspace.context();
    app.actions = Some(prepared.workspace);
    if let Some(workspace) = &mut app.actions {
        workspace.watching = state.watch_v2.as_ref().is_some_and(|watch| watch.active());
    }
    let matched = state.matches_v2.as_ref().map(|matches| {
        (
            matches.indices().len(),
            matches.query() == crate::log_finder_v1::ERROR_PATTERN_V1,
            app.actions.as_ref().map_or(0, |w| w.selected + 1),
        )
    });
    app.scoped_actions_v1 = Some(state);
    app.loading[View::Actions.index()] = false;
    app.status = if let Some((ticks, bytes)) = prepared.watch_progress_v2 {
        format!("Original finite Watch • tick {ticks}/12 • {bytes}/33554432 captured bytes • original120s deadline • no mutation")
    } else if let Some((count, errors, line)) = matched {
        if errors {
            format!("First error at line {line} • {count} error lines in actual captured job • 0 external effects • n/N next/previous, x clear")
        } else if count > 0 {
            format!("Loaded-log search • line {line} • {count} matching lines in actual captured job • 0 external effects • n/N next/previous, x clear")
        } else {
            format!("Loaded-log search • {count} matching lines in actual captured job • 0 external effects • x clear")
        }
    } else {
        "Original scoped Actions target verified • bounded non-atomic read • no mutation".into()
    };
    app.hits.clear();
}
pub(crate) fn fail(app: &mut App, owner: &Arc<ScopedActionsOwnerV1>) -> bool {
    if !owner.owns(app) {
        return false;
    }
    if owner.work.journal_v5.is_some() {
        return journal_v5::fail(app, owner);
    }
    if owner.work.publication_v3.is_some() {
        return publication_v3::fail(app, owner);
    }
    if owner.work.kind == ScopedActionsRequestKindV1::Search {
        prepare_policy(app, false);
        app.loading[View::Actions.index()] = false;
        app.status = "Loaded-log query unavailable • retained snapshot and previous matches unchanged • no external effects".into();
        app.hits.clear();
        return true;
    }
    // Current exact original pair only. A newer source/Threads/Review request
    // was already rejected before this shared privacy retirement.
    crate::repository_catalogue_native::privacy(app);
    crate::repository_catalogue_native::retire_remote_views(app);
    app.loading[View::Actions.index()] = false;
    app.context = "Original Actions read unavailable • private source hidden".into();
    app.status =
        "Original Actions read unavailable • original saved intent/drafts preserved • no resend"
            .into();
    app.hits.clear();
    true
}
pub fn forward_refresh(app: &mut App) -> bool {
    if app.view == View::Actions && app.scoped_actions_v1.is_some() {
        app.refresh_requested = false;
        intercept(app, &ActionsAction::Refresh);
        true
    } else {
        false
    }
}
pub fn resize(app: &mut App) {
    if app.scoped_actions_v1.is_none() {
        return;
    }
    if publication_v3::resize(app) {
        return;
    }
    if app.scoped_actions_work_v1.is_some() || app.scoped_actions_owner_v1.is_some() {
        cancel_current(app);
    }
    if let Some(editor) = app
        .scoped_actions_v1
        .as_mut()
        .and_then(|state| state.editor_v2.as_mut())
    {
        editor.clear_geometry_v2();
    }
    app.hits.clear();
}
pub(crate) fn form_source_current_v3(
    app: &App,
    intent: &crate::scoped_actions_publication_v3::ScopedActionsIntentV3,
) -> bool {
    publication_v3::form_source_current(app, intent)
}
pub(crate) fn cancel_publication_v3(app: &mut App) {
    publication_v3::cancel(app);
}
#[cfg(test)]
#[path = "scoped_actions_read_v2_tests.rs"]
mod read_tests_v2;
#[cfg(test)]
#[path = "scoped_actions_native_v1_tests.rs"]
mod tests;
