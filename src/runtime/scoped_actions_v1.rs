//! Exact logical owner/generation on SAME existing Actions queue/slot.
use super::*;
use crate::scoped_actions_native_v1 as native;
pub(super) fn only_actions(p: &PreparedView) -> bool {
    if p.notifications_v63.is_some() {
        return false;
    }
    let PreparedView {
        scoped_pr_collaboration_v17,
        notifications_v63: _,
        scoped_actions_v1,
        scoped_threads_v1,
        scoped_reviews_v2,
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
        refresh_authored_queue,
        navigation_restore,
        catalogue,
        local_actor_result,
        remote_scope,
        remote_scope_source,
        remote_repository,
    } = p;
    scoped_actions_v1.is_some()
        && scoped_reviews_v2.is_none()
        && scoped_threads_v1.is_none()
        && scoped_inline_v1.is_none()
        && scoped_pr_collaboration_v17.is_none()
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
        && !refresh_authored_queue
        && navigation_restore.is_none()
        && catalogue.is_none()
        && local_actor_result.is_none()
        && remote_scope.is_none()
        && remote_scope_source.is_none()
        && remote_repository.is_none()
}

impl Background {
    pub fn submit_scoped_actions_v1(
        &mut self,
        app: &mut App,
        work: Arc<crate::scoped_actions_v1::ScopedActionsWorkV1>,
        provider: crate::providers::gh::GithubProvider,
        database: std::path::PathBuf,
    ) -> bool {
        self.submit_scoped_actions_impl_v3(app, work, provider, database, None)
    }
    pub fn submit_scoped_actions_publication_v3(
        &mut self,
        app: &mut App,
        work: Arc<crate::scoped_actions_v1::ScopedActionsWorkV1>,
        provider: crate::providers::gh::GithubProvider,
        database: std::path::PathBuf,
        drafts: crate::drafts::DraftClient,
    ) -> bool {
        self.submit_scoped_actions_impl_v3(app, work, provider, database, Some(drafts))
    }
    fn submit_scoped_actions_impl_v3(
        &mut self,
        app: &mut App,
        work: Arc<crate::scoped_actions_v1::ScopedActionsWorkV1>,
        provider: crate::providers::gh::GithubProvider,
        database: std::path::PathBuf,
        drafts: Option<crate::drafts::DraftClient>,
    ) -> bool {
        if work.check().is_err() || !native::current_input(app, &work) {
            // Cancel only this rejected captured token. Never touch another
            // live job, logical owner, generation or visible/private state.
            work.cancellation().cancel();
            return false;
        }
        let owner = match drafts.as_ref().map_or_else(
            || native::ScopedActionsOwnerV1::new(work.clone()),
            |drafts| {
                native::ScopedActionsOwnerV1::new_publication_v3(work.clone(), &database, drafts)
            },
        ) {
            Ok(owner) => owner,
            Err(_) => {
                work.cancellation().cancel();
                return false;
            }
        };
        let (scope, source) = owner.remote_owners();
        if native::publication_v3::workflow_edit_v9::prepare_cutoff(
            &owner,
            &provider,
            &database,
            drafts.as_ref(),
        )
        .is_err()
        {
            work.cancellation().cancel();
            return false;
        }
        self.cancel_scoped_actions_v1(app);
        self.cancel(View::Actions);
        self.generation += 1;
        let generation = self.generation;
        app.scoped_actions_owner_v1 = Some(owner.clone());
        let remote = ScopedRemoteOwner {
            collaboration_v17: None,
            notifications_v63: None,
            actions_v1: Some(owner.clone()),
            original: ScopedRemoteOriginalV13::catalogue(scope, source, None),
            check_origin: None,
            continuation_v2: None,
            pr_native_diff_v1: None,
            threads_v1: None,
            inline_v1: None,
            reviews_v2: None,
        };
        if !remote.owns(app) {
            app.scoped_actions_owner_v1 = None;
            native::submission_failed(app, &work);
            return false;
        }
        let worker = owner.clone();
        let request = Request {
            view: View::Actions,
            generation,
            cancellation: owner.cancellation().clone(),
            original_stash_recovery_owner_v1: None,
            catalogue_owner: None,
            catalogue_recovery: None,
            remote_scope_owner: Some(remote),
            local_actor_owner: None,
            remote_repository_owner: None,
            local_navigation_owner: None,
            original_tree_owner_v1: None,
            original_diff_owner_v1: None,
            work: Box::new(move |same_token| {
                if !same_token.same_instance(worker.cancellation()) {
                    return Err("Original Actions token differs".into());
                }
                native::publication_v3::execute(&worker, &provider, &database, drafts.as_ref()).map(
                    |prepared| PreparedView {
                        scoped_actions_v1: Some(Box::new(prepared)),
                        ..Default::default()
                    },
                )
            }),
        };
        match self
            .requests
            .as_ref()
            .expect("active existing queue")
            .try_send(request)
        {
            Ok(()) => {
                self.current.insert(
                    View::Actions.index(),
                    (generation, owner.cancellation().clone()),
                );
                self.current_scoped_actions_v1 = Some(owner);
                self.current_scoped_actions_generation_v1 = Some(generation);
                app.loading[View::Actions.index()] = true;
                app.status =
                    "Original scoped Actions read pending • Back cancels • no mutation".into();
                true
            }
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                app.scoped_actions_owner_v1 = None;
                native::submission_failed(app, &work);
                app.loading[View::Actions.index()] = false;
                false
            }
        }
    }
    pub fn cancel_scoped_actions_v1(&mut self, app: &mut App) -> bool {
        let (Some(owner), Some(generation)) = (
            self.current_scoped_actions_v1.take(),
            self.current_scoped_actions_generation_v1.take(),
        ) else {
            return false;
        };
        owner.cancellation().cancel();
        let same_slot = self
            .current
            .get(&View::Actions.index())
            .is_some_and(|(g, _)| *g == generation);
        if same_slot {
            self.current.remove(&View::Actions.index());
        }
        if app
            .scoped_actions_owner_v1
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, &owner))
        {
            app.scoped_actions_owner_v1 = None;
        }
        if same_slot || !self.current.contains_key(&View::Actions.index()) {
            app.loading[View::Actions.index()] = false;
        }
        true
    }
    pub fn retire_stale_scoped_actions_v1(&mut self, app: &mut App) {
        if self
            .current_scoped_actions_v1
            .as_ref()
            .is_some_and(|owner| !owner.owns(app))
        {
            self.cancel_scoped_actions_v1(app);
        }
    }
    pub(super) fn poll_scoped_actions_v1(
        &mut self,
        app: &mut App,
        c: &mut Completion,
    ) -> Option<bool> {
        let owner = c
            .remote_scope_owner
            .as_ref()
            .and_then(|remote| remote.actions_v1.as_ref())
            .cloned();
        let Some(owner) = owner else {
            if c.result
                .as_ref()
                .is_ok_and(|p| p.scoped_actions_v1.is_some())
                || c.view == View::Actions
                    && self.current_scoped_actions_generation_v1 == Some(c.generation)
            {
                return Some(false);
            }
            return None;
        };
        let remote = c.remote_scope_owner.as_ref().expect("original owner");
        if c.view != View::Actions
            || c.catalogue_owner.is_some()
            || c.catalogue_recovery.is_some()
            || c.original_stash_recovery_owner_v1.is_some()
            || c.local_actor_owner.is_some()
            || c.remote_repository_owner.is_some()
            || c.local_navigation_owner.is_some()
            || c.original_tree_owner_v1.is_some()
            || c.original_diff_owner_v1.is_some()
            || remote.original.navigation().is_some()
            || remote.check_origin.is_some()
            || remote.continuation_v2.is_some()
            || remote.pr_native_diff_v1.is_some()
            || remote.inline_v1.is_some()
            || remote.threads_v1.is_some()
            || remote.reviews_v2.is_some()
            || self.current_scoped_actions_generation_v1 != Some(c.generation)
            || self
                .current_scoped_actions_v1
                .as_ref()
                .is_none_or(|current| !Arc::ptr_eq(current, &owner))
            || self
                .current
                .get(&View::Actions.index())
                .is_none_or(|(g, _)| *g != c.generation)
            || !remote.owns(app)
            || c.result.as_ref().is_ok_and(|p| {
                !only_actions(p)
                    || p.scoped_actions_v1
                        .as_ref()
                        .is_none_or(|p| !p.matches(owner.work()))
            })
        {
            return Some(false);
        }
        // Exact owner/source/kind/current-pending gates precede ALL slot/UI/
        // privacy/loading effects including actual Err/caught panic.
        let result = std::mem::replace(
            &mut c.result,
            Err("Consumed original Actions completion".into()),
        );
        let admitted = if owner.work().check().is_err() {
            native::fail(app, &owner)
        } else {
            match result {
                Ok(mut p) => native::publication_v3::publish(
                    app,
                    &owner,
                    *p.scoped_actions_v1.take().expect("closed checked result"),
                ),
                Err(_) => native::fail(app, &owner),
            }
        };
        if admitted {
            self.current.remove(&View::Actions.index());
            self.current_scoped_actions_v1 = None;
            self.current_scoped_actions_generation_v1 = None;
            if app
                .scoped_actions_owner_v1
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &owner))
            {
                app.scoped_actions_owner_v1 = None;
            }
            app.loading[View::Actions.index()] = false;
        }
        Some(admitted)
    }
}
#[cfg(test)]
#[path = "scoped_actions_publication_v3_tests.rs"]
mod publication_tests_v3;
#[cfg(test)]
#[path = "scoped_actions_read_v2_tests.rs"]
mod read_tests_v2;
#[cfg(test)]
#[path = "scoped_actions_v1_tests.rs"]
mod tests;
