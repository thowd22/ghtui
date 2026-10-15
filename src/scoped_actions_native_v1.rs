//! Native catalogue Actions policy over the existing Actions model/rendering.
use crate::{
    actions_workspace::{ActionsAction, ActionsView, ActionsWorkspace},
    app::{Action, App, View},
    providers::runner::CancellationToken,
    scoped_actions_v1::*,
};
use std::sync::Arc;
struct SnapshotV1 {
    page: Arc<ScopedActionsPageV1>,
    workspace: Box<ActionsWorkspace>,
    bytes: usize,
}
pub struct ScopedActionsStateV1 {
    source: Arc<ScopedActionsSourceV1>,
    page: Arc<ScopedActionsPageV1>,
    retained_bytes: usize,
    back: Vec<SnapshotV1>,
    epoch: u64,
}
pub struct ScopedActionsOwnerV1 {
    work: Arc<ScopedActionsWorkV1>,
}
impl ScopedActionsOwnerV1 {
    pub(crate) fn new(work: Arc<ScopedActionsWorkV1>) -> Result<Arc<Self>, String> {
        work.check()?;
        Ok(Arc::new(Self { work }))
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
            && app.overlay.is_none()
            && !newer_foreign_intent(app)
            && app.scoped_actions_v1.as_ref().is_some_and(|state| {
                current(app, state)
                    && Arc::ptr_eq(&state.source, &self.work.source)
                    && Arc::ptr_eq(&state.page, &self.work.page)
                    && state.epoch == self.work.epoch
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
        && app.overlay.is_none()
        && !app.loading[View::Actions.index()]
        && !app.scoped_actions_cancel_v1
        && app.scoped_actions_owner_v1.is_none()
        && !newer_foreign_intent(app)
        && app
            .scoped_actions_work_v1
            .as_ref()
            .is_none_or(|current| Arc::ptr_eq(current, work))
        && app.scoped_actions_v1.as_ref().is_some_and(|state| {
            current(app, state)
                && Arc::ptr_eq(&state.source, &work.source)
                && Arc::ptr_eq(&state.page, &work.page)
                && state.epoch == work.epoch
        })
}
pub fn newer_foreign_intent(app: &App) -> bool {
    app.pr_native_diff_work_v1.is_some()
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
        || app
            .form
            .as_ref()
            .is_some_and(|form| form.preview_requested || form.submit_requested)
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
        source: state.source,
        page: state.page,
        retained_bytes: state.retained_bytes,
        back: vec![],
        epoch: 0,
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
}
fn explain(app: &mut App, reason: impl Into<String>) {
    app.dispatch(Action::Explain(reason.into()));
}
fn cancel_current(app: &mut App) {
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
    if let Some(workspace) = &mut app.actions {
        crate::actions_workspace::scoped_read_v1::prepare_policy(workspace, false);
    }
    app.hits.clear();
    app.status = "Original Actions read canceled • retained page unchanged • no mutation".into();
}
/// All keys AND hit actions arrive here before generic Actions/forms handling.
pub fn intercept(app: &mut App, action: &ActionsAction) -> bool {
    synchronize(app);
    let Some(state) = app.scoped_actions_v1.as_ref() else {
        return false;
    };
    if !current(app, state) {
        return false;
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
            state.retained_bytes = snapshot.bytes;
            state.epoch = state.epoch.wrapping_add(1);
            app.actions = Some(*snapshot.workspace);
            app.context = app.actions.as_ref().expect("retained").context();
            app.hits.clear();
            app.status = "Original Actions snapshot restored • no fresh request or mutation".into();
        } else {
            explain(app,"No earlier Actions snapshot; use the main view keys to leave. No query or mutation sent");
        }
        app.scoped_actions_v1 = Some(state);
        return true;
    }
    if let ActionsAction::ControlsPage(delta) = action {
        if let Some(w) = &mut app.actions {
            w.controls_page = w.controls_page.saturating_add_signed(*delta as isize);
        }
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
            state.retained_bytes = snapshot.bytes;
            state.epoch = state.epoch.wrapping_add(1);
            app.actions = Some(*snapshot.workspace);
            app.scoped_actions_v1 = Some(state);
            app.hits.clear();
            return true;
        }
    }
    state.epoch = state.epoch.wrapping_add(1);
    let work = ScopedActionsWorkV1::capture(state.page.clone(), action, state.epoch);
    app.scoped_actions_v1 = Some(state);
    match work {
        Ok(work) => {
            app.scoped_actions_work_v1 = Some(work);
            app.scoped_actions_cancel_v1 = false;
            if let Some(w) = &mut app.actions {
                crate::actions_workspace::scoped_read_v1::prepare_policy(w, true);
            }
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
        if let Some(w) = &mut app.actions {
            crate::actions_workspace::scoped_read_v1::prepare_policy(w, false);
        }
        return None;
    }
    Some(work)
}
pub fn submission_failed(app: &mut App, work: &Arc<ScopedActionsWorkV1>) {
    if app.scoped_actions_v1.as_ref().is_some_and(|state| {
        current(app, state) && Arc::ptr_eq(&state.page, &work.page) && state.epoch == work.epoch
    }) {
        if let Some(w) = &mut app.actions {
            crate::actions_workspace::scoped_read_v1::prepare_policy(w, false);
        }
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
    let mut state = app.scoped_actions_v1.take().expect("checked");
    if let Some(mut old) = app.actions.take() {
        crate::actions_workspace::scoped_read_v1::prepare_policy(&mut old, false);
        let mut retained = state
            .back
            .iter()
            .map(|s| s.bytes)
            .sum::<usize>()
            .saturating_add(state.retained_bytes);
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
        if state.retained_bytes <= BACK_LIMIT_V1 {
            state.back.push(SnapshotV1 {
                page: state.page.clone(),
                workspace: Box::new(old),
                bytes: state.retained_bytes,
            });
        }
    }
    state.page = prepared.page;
    state.retained_bytes = prepared.retained_bytes;
    app.context = prepared.workspace.context();
    app.actions = Some(prepared.workspace);
    app.scoped_actions_v1 = Some(state);
    app.loading[View::Actions.index()] = false;
    app.status =
        "Original scoped Actions target verified • bounded non-atomic read • no mutation".into();
    app.hits.clear();
    true
}
pub(crate) fn fail(app: &mut App, owner: &Arc<ScopedActionsOwnerV1>) -> bool {
    if !owner.owns(app) {
        return false;
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
    if app.scoped_actions_work_v1.is_some() || app.scoped_actions_owner_v1.is_some() {
        cancel_current(app);
    }
    app.hits.clear();
}
#[cfg(test)]
#[path = "scoped_actions_native_v1_tests.rs"]
mod tests;
