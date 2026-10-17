//! One CLOSED local Clear on the existing Inbox queue. No provider/Core,
//! source renewal, name fallback, new App facet or arbitrary SQL callback.
use super::*;
use crate::{
    active_remote_scope_v1::ActiveRemoteScopeV1,
    github_workspace::{Freshness, GithubAction, InboxFilter, PrKey},
    storage::{Cache, CatalogueIntentSource, Scope},
};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub(super) struct CatalogueClearOwnerV5 {
    active: Arc<ActiveRemoteScopeV1>,
    source: Arc<CatalogueIntentSource>,
    database: PathBuf,
    generation: u64,
    rows: (usize, usize),
    selection: Option<PrKey>,
    filter: InboxFilter,
    position: (usize, usize, usize, bool),
    cancel: CancellationToken,
    deadline: Instant,
    #[cfg(test)]
    after_sql_v5: Option<fn() -> Result<(), String>>,
}
fn newer(app: &App, own_clear: bool) -> bool {
    app.github_action
        .as_ref()
        .is_some_and(|a| !own_clear || *a != GithubAction::ClearCache)
        || app.refresh_requested
        || app.pr_action.is_some()
        || app.cancel_requested
        || app.scoped_pr_tab_cancel_requested
        || app.personal_pr_entry_work_v13.is_some()
        || app.personal_pr_entry_cancel_v13
        || app.scoped_pr_collaboration_work_v17.is_some()
        || app.scoped_pr_collaboration_cancel_v17
        || app.notifications_work_v63.is_some()
        || app.notifications_owner_v63.is_some()
        || app.notifications_cancel_v63
        || app.scoped_inline_work_v1.is_some()
        || app.scoped_inline_cancel_v1
        || app.scoped_threads_work_v1.is_some()
        || app.scoped_threads_cancel_v1
        || app.scoped_reviews_work_v2.is_some()
        || app.scoped_reviews_cancel_v2
        || app.pr_native_diff_work_v1.is_some()
        || app.pr_native_diff_cancel_v1
        || app.scoped_actions_work_v1.is_some()
        || app.scoped_actions_cancel_v1
}
impl CatalogueClearOwnerV5 {
    fn capture(app: &App, database: &Path) -> Option<Arc<Self>> {
        if app.view != View::Inbox
            || app.overlay.is_some()
            || app.form.is_some()
            || app.personal_pr_source_v13.is_some()
            || app.notification_pr_source_v63.is_some()
            || app.loading[View::Inbox.index()]
            || newer(app, true)
        {
            return None;
        }
        let active = app.remote_scope.as_ref()?;
        let source = app.remote_scope_source.as_ref()?;
        if crate::remote_scope_gate_v1::stop_before_fallback(Some(active)).is_some() {
            return None;
        }
        let g = app.github.as_ref()?;
        if !matches!(
            active.consumer(&g.identity),
            crate::active_remote_scope_v1::RemoteScopeConsumerV1::Explicit(_)
        ) {
            return None;
        }
        if g.pr.is_some()
            || g.scoped_pr_navigation.is_some()
            || g.scoped_pr_resource.is_some()
            || g.scope().ok().as_ref() != Some(source.scope())
            || g.identity.account_id.is_none_or(|id| id == 0)
            || database.as_os_str().len() > 4096
            || g.selected
                .as_ref()
                .is_some_and(|k| k.host.len() + k.repository.len() > 1024)
        {
            return None;
        }
        Some(Arc::new(Self {
            active: Arc::clone(active),
            source: Arc::clone(source),
            database: database.into(),
            generation: g.generation,
            rows: (
                g.inbox.pull_requests.as_ptr() as usize,
                g.inbox.pull_requests.len(),
            ),
            selection: g.selected.clone(),
            filter: g.filter,
            position: (
                app.selected,
                app.scroll,
                app.focus,
                app.github_content_focused,
            ),
            cancel: CancellationToken::default(),
            deadline: Instant::now() + Duration::from_secs(30),
            #[cfg(test)]
            after_sql_v5: None,
        }))
    }
    pub(super) fn scope(&self) -> &Scope {
        self.source.scope()
    }
    fn same_presentation(&self, app: &App) -> bool {
        self.matches_presentation_v5(app, false)
    }
    fn matches_presentation_v5(&self, app: &App, own_clear: bool) -> bool {
        app.view == View::Inbox
            && app.overlay.is_none()
            && app.form.is_none()
            && app.personal_pr_source_v13.is_none()
            && app.notification_pr_source_v63.is_none()
            && !newer(app, own_clear)
            && app
                .remote_scope
                .as_ref()
                .is_some_and(|a| Arc::ptr_eq(a, &self.active))
            && app
                .remote_scope_source
                .as_ref()
                .is_some_and(|s| Arc::ptr_eq(s, &self.source))
            && (
                app.selected,
                app.scroll,
                app.focus,
                app.github_content_focused,
            ) == self.position
            && app.github.as_ref().is_some_and(|g| {
                g.pr.is_none()
                    && g.scoped_pr_navigation.is_none()
                    && g.scoped_pr_resource.is_none()
                    && g.generation == self.generation
                    && g.filter == self.filter
                    && g.selected == self.selection
                    && (
                        g.inbox.pull_requests.as_ptr() as usize,
                        g.inbox.pull_requests.len(),
                    ) == self.rows
            })
    }
    pub(super) fn owns(&self, app: &App) -> bool {
        self.same_presentation(app)
            && app
                .github
                .as_ref()
                .is_some_and(|g| g.scope().ok().as_ref() == Some(self.scope()))
    }
    fn live(&self) -> bool {
        !self.cancel.is_cancelled() && Instant::now() < self.deadline
    }
    fn execute(&self) -> Result<PreparedView, String> {
        if !self.live() {
            return Err("Original local Clear cancelled/expired • no deletion committed".into());
        }
        Cache::clear_original_catalogue_records_v5(
            &self.source,
            &self.database,
            &self.cancel,
            self.deadline,
        )
        .map_err(|_| "Original local Clear boundary changed/failed • no deletion committed")?;
        #[cfg(test)]
        if let Some(fault) = self.after_sql_v5 {
            fault()?;
        }
        // Only the exact runtime owner may consume this EMPTY success. No
        // Workspace/body is cloned/moved from App and no source is installed.
        Ok(PreparedView::default())
    }
}
fn only_empty(p: &PreparedView) -> bool {
    // Exhaustive on purpose: an additive result facet MUST cause a compiler
    // review here, never silently become acceptable to a local-only erasure.
    let PreparedView {
        scoped_pr_collaboration_v17: None,
        notifications_v63: None,
        scoped_actions_v1: None,
        scoped_threads_v1: None,
        scoped_reviews_v2: None,
        scoped_inline_v1: None,
        original_stash_recovery_v1: None,
        original_diff_result_v1: None,
        related_pr_v1: None,
        lines,
        context: None,
        status,
        remote_repository: None,
        workspace: None,
        graph_reread_retention_v3: None,
        original_tree_result_v1: None,
        repository_source_identity: None,
        github: None,
        scoped_pr_tab: None,
        scoped_pr_resource: None,
        form: None,
        search: None,
        navigate_to: None,
        restore_search_input: false,
        actions: None,
        pr_checks_origin: None,
        pr_checks_route: None,
        scoped_pr_checks: None,
        pr_check_continuation_v2: None,
        actions_inspection: None,
        clear_actions_inspection: false,
        clear_private: false,
        activate_if_untouched: false,
        pr_inspection: None,
        clear_pr_inspection: false,
        pr_pending_message: None,
        verified_identity: None,
        pr_reactions: None,
        pr_reaction_display: None,
        readonly_notice: None,
        source_message: None,
        source_context: None,
        metrics: None,
        clear_metrics: false,
        github_origin: None,
        invalidate_metrics: false,
        invalidate_pr_metadata: false,
        navigation_restore: None,
        catalogue: None,
        local_actor_result: None,
        remote_scope: None,
        remote_scope_source: None,
    } = p
    else {
        return false;
    };
    lines.is_empty() && status.is_empty()
}
impl Background {
    /// ONLY Main's explicit loaded-Catalogue Clear branch calls this. Initial
    /// capture/refusal never cancels or takes an old physical/logical request.
    pub fn submit_catalogue_clear_v5(&mut self, app: &mut App, database: &Path) -> bool {
        if app.github_action != Some(GithubAction::ClearCache) {
            return false;
        }
        let Some(owner) = CatalogueClearOwnerV5::capture(app, database) else {
            return false;
        };
        self.submit_captured_catalogue_clear_v5(app, owner)
    }
    fn submit_captured_catalogue_clear_v5(
        &mut self,
        app: &mut App,
        owner: Arc<CatalogueClearOwnerV5>,
    ) -> bool {
        // Only a freshly captured actual owner reaches this private seam.
        // The test-only fault hook does not bypass any admission predicate.
        if app.github_action != Some(GithubAction::ClearCache)
            || !owner.matches_presentation_v5(app, true)
            || app
                .github
                .as_ref()
                .is_none_or(|g| g.scope().ok().as_ref() != Some(owner.scope()))
            || app.loading[View::Inbox.index()]
        {
            return false;
        }
        if self.current.contains_key(&View::Inbox.index())
            || self.current_collaboration_v17.is_some()
            || self.current_notifications_v63.is_some()
            || self.current_personal_pr_entry_v13.is_some()
            || self.current_threads_owner_v1.is_some()
            || self.current_reviews_owner_v2.is_some()
            || self.current_inline_owner_v1.is_some()
            || self.current_pr_native_diff_v1.is_some()
        {
            return false;
        }
        let Some(generation) = self.generation.checked_add(1) else {
            return false;
        };
        let remote = ScopedRemoteOwner {
            collaboration_v17: None,
            notifications_v63: None,
            actions_v1: None,
            pr_native_diff_v1: None,
            threads_v1: None,
            reviews_v2: None,
            inline_v1: None,
            original: ScopedRemoteOriginalV13::CatalogueClearV5(Arc::clone(&owner)),
            check_origin: None,
            continuation_v2: None,
        };
        let worker = Arc::clone(&owner);
        let request = Request {
            original_stash_recovery_owner_v1: None,
            view: View::Inbox,
            generation,
            cancellation: owner.cancel.clone(),
            catalogue_owner: None,
            catalogue_recovery: None,
            remote_scope_owner: Some(remote),
            local_actor_owner: None,
            remote_repository_owner: None,
            local_navigation_owner: None,
            original_tree_owner_v1: None,
            original_diff_owner_v1: None,
            work: Box::new(move |actual| {
                if !actual.same_instance(&worker.cancel) {
                    return Err("Original local Clear token differs".into());
                }
                worker.execute()
            }),
        };
        if !owner.live() {
            return false;
        }
        let Some(requests) = self.requests.as_ref() else {
            return false;
        };
        match requests.try_send(request) {
            Ok(()) => {
                self.generation = generation;
                self.current
                    .insert(View::Inbox.index(), (generation, owner.cancel.clone()));
                app.github_action = None;
                app.loading[View::Inbox.index()] = true;
                app.status =
                    "Clearing original account cache • drafts/history kept • Esc Cancel".into();
                true
            }
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => false,
        }
    }
    pub(super) fn poll_catalogue_clear_v5(
        &mut self,
        app: &mut App,
        completion: &mut Completion,
    ) -> Option<bool> {
        let remote = completion.remote_scope_owner.as_ref()?;
        let ScopedRemoteOriginalV13::CatalogueClearV5(owner) = &remote.original else {
            return None;
        };
        if completion.view != View::Inbox
            || completion.catalogue_owner.is_some()
            || completion.catalogue_recovery.is_some()
            || completion.local_actor_owner.is_some()
            || completion.remote_repository_owner.is_some()
            || completion.local_navigation_owner.is_some()
            || completion.original_tree_owner_v1.is_some()
            || completion.original_diff_owner_v1.is_some()
            || completion.original_stash_recovery_owner_v1.is_some()
            || remote.collaboration_v17.is_some()
            || remote.notifications_v63.is_some()
            || remote.actions_v1.is_some()
            || remote.pr_native_diff_v1.is_some()
            || remote.threads_v1.is_some()
            || remote.reviews_v2.is_some()
            || remote.inline_v1.is_some()
            || remote.check_origin.is_some()
            || remote.continuation_v2.is_some()
            || completion.result.as_ref().is_ok_and(|p| !only_empty(p))
        {
            return Some(false);
        }
        // Reject ALL mixed purposes/facets BEFORE even privacy cleanup.
        // Exact old account loss retains privacy precedence, even if a generic
        // successor owns the physical slot. New pages/overlays never match.
        if owner.same_presentation(app)
            && app
                .github
                .as_ref()
                .is_some_and(|g| g.scope().ok().as_ref() != Some(owner.scope()))
        {
            let own_slot = self
                .current
                .get(&View::Inbox.index())
                .is_some_and(|(g, t)| {
                    *g == completion.generation && t.same_instance(&owner.cancel)
                });
            app.github = None;
            app.context = "Original account changed • previous Inbox hidden".into();
            app.content[View::Inbox.index()] =
                vec!["Original account changed • private cached Inbox hidden".into()];
            app.avatar_states.clear();
            app.hits.clear();
            if own_slot {
                self.current.remove(&View::Inbox.index());
                app.loading[View::Inbox.index()] = false;
                app.status =
                    "Original account changed • previous Inbox hidden • Clear not retargeted"
                        .into();
            }
            return Some(true);
        }
        if !remote.owns(app)
            || self
                .current
                .get(&View::Inbox.index())
                .is_none_or(|(g, t)| *g != completion.generation || !t.same_instance(&owner.cancel))
        {
            return Some(false);
        }
        self.current.remove(&View::Inbox.index());
        app.loading[View::Inbox.index()] = false;
        if completion.result.is_ok() {
            // Commit may have linearized before a late cancel; report the real
            // completed deletion, never infer that cancellation undid SQL.
            let g = app
                .github
                .as_mut()
                .expect("exact current numeric Inbox above");
            g.inbox.pull_requests.clear();
            g.inbox.coverage = Default::default();
            g.selected = None;
            g.pr = None;
            g.scoped_pr_navigation = None;
            g.scoped_pr_resource = None;
            g.personal_pr_page_ticket_v13 = None;
            g.needs_refresh = false;
            g.freshness = Freshness::Empty;
            app.selected = 0;
            app.scroll = 0;
            app.focus = 0;
            app.context = g.context();
            app.hits.clear();
            app.avatar_states.clear();
            app.status =
                "Original account cache cleared • drafts and write history retained".into();
        } else {
            app.status =
                "Original local Clear unavailable • drafts/history kept • inspect cache".into();
        }
        Some(true)
    }
}

#[cfg(test)]
#[path = "catalogue_clear_v5_tests.rs"]
mod tests;
