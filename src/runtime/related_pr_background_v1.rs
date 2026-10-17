use super::*;
impl Background {
    /// Close owns only this retained operation/generation, including a completed
    /// result vetoed before publication. Never cancel a different Repo job.
    pub fn cancel_current_related_pr_v1(&mut self) -> bool {
        let Some(generation) = self.current_related_pr_generation_v1 else {
            return false;
        };
        if self.current_related_pr_v1.is_none()
            || self
                .current
                .get(&View::Repository.index())
                .is_some_and(|(current, _)| *current != generation)
        {
            return false;
        }
        self.current_related_pr_v1
            .as_ref()
            .expect("checked owner")
            .cancellation()
            .cancel();
        self.cancel(View::Repository);
        true
    }
    pub(super) fn fail_current_related_pr_v1(
        &mut self,
        app: &mut App,
        owner: &Arc<crate::repository_related_pr_v1::RelatedPrOwnerV1>,
    ) {
        if !owner.owns(app) {
            return;
        }
        // Claim and erase the owned modal first: privacy retires the original
        // profile/choice, so it must never run before the owner check above.
        if !crate::repository_related_pr_v1::fail(app, owner) {
            return;
        }
        for view in [View::Inbox, View::Actions, View::Search, View::Dashboard] {
            if view != View::Search
                || !crate::remote_scope_gate_v1::independent_local_search(app.search.as_ref())
            {
                self.cancel(view);
            }
        }
        crate::repository_catalogue_native::privacy(app);
        crate::repository_catalogue_native::retire_remote_views(app);
        app.loading[View::Repository.index()] = false;
    }
    pub fn submit_related_pr_v1(
        &mut self,
        app: &mut App,
        work: impl FnOnce(
                Arc<crate::repository_related_pr_v1::RelatedPrOwnerV1>,
            ) -> Result<PreparedView, String>
            + Send
            + 'static,
    ) -> bool {
        self.cancel(View::Repository);
        let cancellation = CancellationToken::default();
        let owner = match crate::repository_related_pr_v1::begin(app, cancellation.clone()) {
            Ok(owner) => owner,
            Err(reason) => {
                app.status = reason;
                return false;
            }
        };
        let Some(generation) = self.generation.checked_add(1) else {
            crate::repository_related_pr_v1::fail(app, &owner);
            return false;
        };
        self.generation = generation;
        let worker_owner = owner.clone();
        let request = Request {
            original_stash_recovery_owner_v1: None,
            view: View::Repository,
            generation,
            cancellation: cancellation.clone(),
            catalogue_owner: None,
            catalogue_recovery: None,
            remote_scope_owner: None,
            local_actor_owner: None,
            local_navigation_owner: None,
            original_tree_owner_v1: None,
            original_diff_owner_v1: None,
            remote_repository_owner: None,
            work: Box::new(move |_| work(worker_owner)),
        };
        match self
            .requests
            .as_ref()
            .expect("active worker queue")
            .try_send(request)
        {
            Ok(()) => {
                self.current
                    .insert(View::Repository.index(), (generation, cancellation));
                self.current_related_pr_v1 = Some(owner);
                self.current_related_pr_generation_v1 = Some(generation);
                app.loading[View::Repository.index()] = true;
                true
            }
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                crate::repository_related_pr_v1::fail(app, &owner);
                owner.cancellation().cancel();
                app.loading[View::Repository.index()] = false;
                app.status =
                    "Related PR queue full • zero work submitted • discover explicitly".into();
                false
            }
        }
    }
}
pub(super) fn only_related(p: &PreparedView) -> bool {
    if p.scoped_actions_v1.is_some() {
        return false;
    }
    if p.notifications_v63.is_some() {
        return false;
    }
    let PreparedView {
        scoped_pr_collaboration_v17,
        notifications_v63: _,
        scoped_actions_v1: _,
        scoped_threads_v1,
        scoped_inline_v1,
        scoped_reviews_v2,
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
    related_pr_v1.is_some()
        && scoped_threads_v1.is_none()
        && scoped_inline_v1.is_none()
        && scoped_pr_collaboration_v17.is_none()
        && scoped_reviews_v2.is_none()
        && original_stash_recovery_v1.is_none()
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

#[cfg(test)]
#[path = "related_pr_background_v1_tests.rs"]
mod tests;
