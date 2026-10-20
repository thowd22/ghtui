//! Purpose-specific owner on the existing bounded repository worker queue.
use super::*;
use crate::repository_stash_recovery_v1::{
    OriginalStashRecoveryIntentV1, OriginalStashRecoveryOwnerV1,
};
#[cfg(test)]
#[path = "original_stash_recovery_before_poll_v1_tests.rs"]
mod tests;
impl Background {
    pub fn submit_stash_recovery_v1(
        &mut self,
        app: &mut App,
        intent: Arc<OriginalStashRecoveryIntentV1>,
    ) -> bool {
        if !intent.owns(app) {
            return false;
        }
        self.cancel(View::Repository);
        let cancellation = CancellationToken::default();
        let owner = match OriginalStashRecoveryOwnerV1::begin(app, intent, cancellation.clone()) {
            Ok(owner) => owner,
            Err(reason) => {
                app.status = reason;
                return false;
            }
        };
        let Some(generation) = self.generation.checked_add(1) else {
            crate::repository_stash_recovery_v1::fail(app, &owner);
            return false;
        };
        self.generation = generation;
        let worker = owner.clone();
        let request = Request {
            original_stash_recovery_owner_v1: Some(owner.clone()),
            view: View::Repository,
            generation,
            cancellation: cancellation.clone(),
            catalogue_owner: None,
            catalogue_recovery: None,
            remote_scope_owner: None,
            local_actor_owner: None,
            remote_repository_owner: None,
            local_navigation_owner: None,
            original_tree_owner_v1: None,
            original_diff_owner_v1: None,
            work: Box::new(move |_| {
                worker.prepare().map(|result| PreparedView {
                    original_stash_recovery_v1: Some(result),
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
                self.current_stash_recovery_v1 = Some((generation, owner));
                self.current
                    .insert(View::Repository.index(), (generation, cancellation));
                app.loading[View::Repository.index()] = true;
                true
            }
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                crate::repository_stash_recovery_v1::fail(app, &owner);
                owner.cancellation().cancel();
                app.loading[View::Repository.index()] = false;
                app.status =
                    "Stash inspection queue unavailable • zero observer work submitted".into();
                false
            }
        }
    }
    pub fn cancel_current_stash_recovery_v1(&mut self) -> bool {
        let Some((generation, owner)) = &self.current_stash_recovery_v1 else {
            return false;
        };
        if self
            .current
            .get(&View::Repository.index())
            .is_some_and(|(current, _)| current != generation)
        {
            return false;
        }
        owner.cancellation().cancel();
        self.cancel(View::Repository);
        true
    }
    pub(super) fn publish_stash_recovery_v1(
        &mut self,
        app: &mut App,
        completion: &mut Completion,
    ) -> Option<bool> {
        let Some(owner) = completion.original_stash_recovery_owner_v1.clone() else {
            return completion
                .result
                .as_ref()
                .is_ok_and(|p| p.original_stash_recovery_v1.is_some())
                .then_some(false);
        };
        if completion.view != View::Repository
            || completion.catalogue_owner.is_some()
            || completion.catalogue_recovery.is_some()
            || completion.remote_scope_owner.is_some()
            || completion.local_actor_owner.is_some()
            || completion.remote_repository_owner.is_some()
            || completion.local_navigation_owner.is_some()
            || completion.original_tree_owner_v1.is_some()
            || completion.original_diff_owner_v1.is_some()
            || self
                .current_stash_recovery_v1
                .as_ref()
                .is_none_or(|(generation, current)| {
                    *generation != completion.generation || !Arc::ptr_eq(current, &owner)
                })
            || !owner.owns(app)
            || completion.result.as_ref().is_ok_and(|p| {
                !only_stash(p)
                    || p.original_stash_recovery_v1
                        .as_ref()
                        .is_none_or(|result| !result.matches(&owner))
            })
        {
            return Some(false);
        }
        let result = std::mem::replace(
            &mut completion.result,
            Err("consumed original stash result".into()),
        );
        let admitted = match result {
            Ok(mut p) => crate::repository_stash_recovery_v1::accept(
                app,
                &owner,
                p.original_stash_recovery_v1.take().expect("checked result"),
            ),
            Err(_) => crate::repository_stash_recovery_v1::fail(app, &owner),
        };
        if admitted {
            self.current.remove(&View::Repository.index());
            self.current_stash_recovery_v1 = None;
            app.loading[View::Repository.index()] = false;
        }
        Some(admitted)
    }
}
pub(super) fn only_stash(p: &PreparedView) -> bool {
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
        original_stash_recovery_v1,
        scoped_inline_v1,
        scoped_reviews_v2,
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
    original_stash_recovery_v1.is_some()
        && scoped_threads_v1.is_none()
        && scoped_inline_v1.is_none()
        && scoped_pr_collaboration_v17.is_none()
        && scoped_reviews_v2.is_none()
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
