//! Same existing Inbox queue: exact original source/navigation/ticket before effects.
use super::*;
use crate::scoped_pr_observed_tabs_v1::resources::threads_v1::native_v1 as threads;
pub(super) fn only_threads(p: &PreparedView) -> bool {
    let PreparedView {
        scoped_threads_v1,
        scoped_inline_v1,
        original_stash_recovery_v1,
        related_pr_v1,
        original_diff_result_v1,
        lines,
        context,
        status,
        workspace,
        graph_reread_retention_v3,
        original_tree_result_v1,
        repository_source_identity,
        github,
        scoped_pr_tab,
        scoped_pr_resource,
        form,
        search,
        navigate_to,
        restore_search_input,
        actions,
        pr_checks_origin,
        pr_checks_route,
        scoped_pr_checks,
        pr_check_continuation_v2,
        actions_inspection,
        clear_actions_inspection,
        clear_private,
        activate_if_untouched,
        pr_inspection,
        clear_pr_inspection,
        pr_pending_message,
        verified_identity,
        pr_reactions,
        pr_reaction_display,
        readonly_notice,
        source_message,
        source_context,
        metrics,
        clear_metrics,
        github_origin,
        invalidate_metrics,
        invalidate_pr_metadata,
        navigation_restore,
        catalogue,
        local_actor_result,
        remote_scope,
        remote_scope_source,
        remote_repository,
    } = p;
    scoped_threads_v1.is_some()
        && scoped_inline_v1.is_none()
        && original_stash_recovery_v1.is_none()
        && related_pr_v1.is_none()
        && original_diff_result_v1.is_none()
        && lines.is_empty()
        && context.is_none()
        && status.is_empty()
        && workspace.is_none()
        && graph_reread_retention_v3.is_none()
        && original_tree_result_v1.is_none()
        && repository_source_identity.is_none()
        && github.is_none()
        && scoped_pr_tab.is_none()
        && scoped_pr_resource.is_none()
        && form.is_none()
        && search.is_none()
        && navigate_to.is_none()
        && !restore_search_input
        && actions.is_none()
        && pr_checks_origin.is_none()
        && pr_checks_route.is_none()
        && scoped_pr_checks.is_none()
        && pr_check_continuation_v2.is_none()
        && actions_inspection.is_none()
        && !clear_actions_inspection
        && !clear_private
        && !activate_if_untouched
        && pr_inspection.is_none()
        && !clear_pr_inspection
        && pr_pending_message.is_none()
        && verified_identity.is_none()
        && pr_reactions.is_none()
        && pr_reaction_display.is_none()
        && readonly_notice.is_none()
        && source_message.is_none()
        && source_context.is_none()
        && metrics.is_none()
        && !clear_metrics
        && github_origin.is_none()
        && !invalidate_metrics
        && !invalidate_pr_metadata
        && navigation_restore.is_none()
        && catalogue.is_none()
        && local_actor_result.is_none()
        && remote_scope.is_none()
        && remote_scope_source.is_none()
        && remote_repository.is_none()
}

impl Background {
    pub fn submit_scoped_threads_v1(
        &mut self,
        app: &mut App,
        owner: Arc<threads::ScopedThreadsOwnerV1>,
        provider: crate::providers::gh::GithubProvider,
        database: std::path::PathBuf,
    ) -> bool {
        self.submit_scoped_thread_operation_v1(app, owner, provider, database, None)
    }
    pub fn submit_scoped_threads_with_drafts_v1(
        &mut self,
        app: &mut App,
        owner: Arc<threads::ScopedThreadsOwnerV1>,
        provider: crate::providers::gh::GithubProvider,
        database: std::path::PathBuf,
        drafts: crate::drafts::DraftClient,
    ) -> bool {
        self.submit_scoped_thread_operation_v1(app, owner, provider, database, Some(drafts))
    }
    fn submit_scoped_thread_operation_v1(
        &mut self,
        app: &mut App,
        owner: Arc<threads::ScopedThreadsOwnerV1>,
        provider: crate::providers::gh::GithubProvider,
        database: std::path::PathBuf,
        drafts: Option<crate::drafts::DraftClient>,
    ) -> bool {
        // Owner/token/deadline were captured by actual input BEFORE this queue.
        if !owner.presentation_matches(app) || threads::newer_intent(app) {
            owner.release_busy_presentation(app);
            return false;
        }
        if owner.expired() {
            owner.cancellation().cancel();
            owner.rejected(
                app,
                "Original queued thread operation expired • nothing started • reopen explicitly",
            );
            return false;
        }
        let intent = &owner.intent;
        if crate::remote_scope_gate_v1::stop_before_fallback(Some(&intent.active)).is_some() {
            return false;
        }
        self.cancel(View::Inbox);
        self.generation += 1;
        let generation = self.generation;
        app.scoped_threads_owner_v1 = Some(owner.clone());
        let remote = ScopedRemoteOwner {
            pr_native_diff_v1: None,
            scope: intent.active.clone(),
            source: intent.source.clone(),
            navigation: Some(intent.navigation.clone()),
            check_origin: None,
            continuation_v2: None,
            inline_v1: None,
            threads_v1: Some(owner.clone()),
        };
        if !remote.owns(app) {
            app.scoped_threads_owner_v1 = None;
            return false;
        }
        let worker = owner.clone();
        let request = Request {
            original_stash_recovery_owner_v1: None,
            view: View::Inbox,
            generation,
            cancellation: owner.cancellation().clone(),
            catalogue_owner: None,
            catalogue_recovery: None,
            remote_scope_owner: Some(remote),
            local_actor_owner: None,
            remote_repository_owner: None,
            local_navigation_owner: None,
            original_tree_owner_v1: None,
            original_diff_owner_v1: None,
            work: Box::new(move |_same_original_token| {
                Ok(PreparedView {
                    scoped_threads_v1: Some(threads::execute_with_drafts(
                        worker,
                        &provider,
                        &database,
                        drafts.as_ref(),
                    )),
                    ..Default::default()
                })
            }),
        };
        match self
            .requests
            .as_ref()
            .expect("active queue")
            .try_send(request)
        {
            Ok(()) => {
                self.current.insert(
                    View::Inbox.index(),
                    (generation, owner.cancellation().clone()),
                );
                self.current_threads_owner_v1 = Some(owner.clone());
                self.current_threads_generation_v1 = Some(generation);
                app.loading[View::Inbox.index()] = true;
                if owner.phase() == threads::ScopedThreadPhaseV1::Preview
                    || owner.phase() == threads::ScopedThreadPhaseV1::Confirm
                {
                    if let Some(form) = &mut app.form {
                        form.submitting = true;
                    }
                }
                app.status = format!(
                    "Original thread {:?} pending • Esc cancels exact operation",
                    owner.phase()
                );
                true
            }
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                owner.cancellation().cancel();
                app.scoped_threads_owner_v1 = None;
                app.loading[View::Inbox.index()] = false;
                owner.rejected(
                    app,
                    "Thread queue full • nothing started • Back and open explicitly",
                );
                false
            }
        }
    }
    /// Some consumes/rejects the completion; None leaves the ordinary value
    /// unchanged. This runs BEFORE every existing purpose and generic effect.
    pub(super) fn poll_threads_completion_v1(
        &mut self,
        app: &mut App,
        completion: &mut Completion,
        slot: usize,
    ) -> Option<bool> {
        if app.scoped_threads_work_v1.is_some() || app.scoped_threads_cancel_v1 {
            return Some(false);
        }
        let Some(owner) = completion
            .remote_scope_owner
            .as_ref()
            .and_then(|x| x.threads_v1.clone())
        else {
            if completion
                .result
                .as_ref()
                .is_ok_and(|x| x.scoped_threads_v1.is_some())
                || (slot == View::Inbox.index() && self.current_threads_owner_v1.is_some())
            {
                return Some(false);
            }
            return None;
        };
        let remote = completion
            .remote_scope_owner
            .as_ref()
            .expect("same original owner");
        if slot != View::Inbox.index()
            || completion.view != View::Inbox
            || completion.catalogue_owner.is_some()
            || completion.catalogue_recovery.is_some()
            || completion.local_actor_owner.is_some()
            || completion.local_navigation_owner.is_some()
            || completion.remote_repository_owner.is_some()
            || completion.original_tree_owner_v1.is_some()
            || completion.original_diff_owner_v1.is_some()
            || completion.original_stash_recovery_owner_v1.is_some()
            || remote.pr_native_diff_v1.is_some()
            || remote.inline_v1.is_some()
            || remote.check_origin.is_some()
            || remote.continuation_v2.is_some()
            || self.current_threads_generation_v1 != Some(completion.generation)
            || self
                .current_threads_owner_v1
                .as_ref()
                .is_none_or(|x| !Arc::ptr_eq(x, &owner))
            || !remote.owns(app)
            || completion.result.as_ref().is_ok_and(|p| {
                !only_threads(p)
                    || p.scoped_threads_v1
                        .as_ref()
                        .is_none_or(|x| !x.matches(&owner))
            })
        {
            return Some(false);
        }
        self.current.remove(&slot);
        self.current_threads_owner_v1 = None;
        self.current_threads_generation_v1 = None;
        match completion.result.as_mut() {
            Ok(prepared)
                if !owner.expired()
                    && !threads::needs_private_retirement(
                        prepared
                            .scoped_threads_v1
                            .as_ref()
                            .expect("checked closed facet"),
                    ) =>
            {
                threads::publish(
                    app,
                    self,
                    &owner,
                    prepared
                        .scoped_threads_v1
                        .take()
                        .expect("checked closed facet"),
                );
            }
            _ => threads::fail(app, self),
        }
        Some(true)
    }
    pub fn retire_stale_scoped_threads_v1(&mut self, app: &mut App) {
        let Some(owner) = self.current_threads_owner_v1.clone() else {
            if let Some(old) = app
                .scoped_threads_owner_v1
                .as_ref()
                .filter(|owner| owner.expired() || !owner.presentation_matches(app))
                .cloned()
            {
                old.cancellation().cancel();
                if old.phase() == threads::ScopedThreadPhaseV1::Confirm
                    && old.original_private_source_is_current(app)
                {
                    threads::fail(app, self);
                    app.status = "Original thread presentation retired • outcome may be unknown • protection retained; refresh explicitly".into();
                    return;
                }
                old.release_busy_presentation(app);
                app.scoped_threads_owner_v1 = None;
                if app
                    .scoped_threads_v1
                    .as_ref()
                    .is_some_and(|view| Arc::ptr_eq(&view.intent, &old.intent))
                {
                    app.scoped_threads_v1 = None;
                    if app.overlay == Some(crate::app::Overlay::PrThreadsV1) {
                        app.overlay = None;
                    }
                    app.hits.clear();
                }
            }
            return;
        };
        if owner.presentation_matches(app) {
            return;
        }
        owner.cancellation().cancel();
        if self.current_threads_generation_v1.is_some()
            && self.current.get(&View::Inbox.index()).map(|x| x.0)
                == self.current_threads_generation_v1
        {
            self.current.remove(&View::Inbox.index());
            app.loading[View::Inbox.index()] = false;
        }
        self.current_threads_owner_v1 = None;
        self.current_threads_generation_v1 = None;
        if owner.phase() == threads::ScopedThreadPhaseV1::Confirm
            && owner.original_private_source_is_current(app)
        {
            threads::fail(app, self);
            app.status = "Original thread presentation retired • possible write protected • nothing resent; refresh explicitly".into();
            return;
        }
        owner.release_busy_presentation(app);
        if app
            .scoped_threads_owner_v1
            .as_ref()
            .is_some_and(|x| Arc::ptr_eq(x, &owner))
        {
            app.scoped_threads_owner_v1 = None;
        }
        if app
            .scoped_threads_v1
            .as_ref()
            .is_some_and(|x| Arc::ptr_eq(&x.intent, &owner.intent))
        {
            app.scoped_threads_v1 = None;
            if app.overlay == Some(crate::app::Overlay::PrThreadsV1) {
                app.overlay = None;
            }
            app.hits.clear();
        }
    }
    pub fn cancel_scoped_threads_v1(&mut self, app: &mut App) -> bool {
        let Some(owner) = self.current_threads_owner_v1.clone() else {
            return false;
        };
        let Some(generation) = self.current_threads_generation_v1 else {
            return false;
        };
        if app
            .scoped_threads_owner_v1
            .as_ref()
            .is_none_or(|x| !Arc::ptr_eq(x, &owner))
            || self.current.get(&View::Inbox.index()).map(|x| x.0) != Some(generation)
        {
            return false;
        }
        owner.cancellation().cancel();
        self.current.remove(&View::Inbox.index());
        self.current_threads_owner_v1 = None;
        self.current_threads_generation_v1 = None;
        app.scoped_threads_owner_v1 = None;
        app.loading[View::Inbox.index()] = false;
        if owner.phase() == threads::ScopedThreadPhaseV1::Confirm
            && owner.original_private_source_is_current(app)
        {
            threads::fail(app, self);
            app.status = "Original thread publication canceled • outcome may be unknown • draft/journal retained • nothing resent; refresh explicitly".into();
            return true;
        }
        owner.release_busy_presentation(app);
        app.status =
            "Original thread read canceled • previous Files observation preserved • no writes"
                .into();
        true
    }
}
