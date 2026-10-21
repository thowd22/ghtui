//! One genuine Personal opening on the existing Inbox channel. The original
//! non-Clone App page is never in Request/Completion and needs no Drop repair.
use super::*;
use crate::personal_pr_entry_v13 as native;
use crate::scoped_pr_observed_tabs_v1::resources::native::PrResourceCompletionV1;

fn only_personal(prepared: &PreparedView) -> bool {
    if prepared.notifications_v63.is_some() {
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
        original_diff_result_v1,
        related_pr_v1,
        lines,
        context,
        status,
        remote_repository,
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
        personal_readmission_v105,
    } = prepared;
    scoped_actions_v1.is_none()
        && scoped_threads_v1.is_none()
        && scoped_reviews_v2.is_none()
        && scoped_inline_v1.is_none()
        && scoped_pr_collaboration_v17.is_none()
        && original_stash_recovery_v1.is_none()
        && original_diff_result_v1.is_none()
        && related_pr_v1.is_none()
        && lines.is_empty()
        && context.is_none()
        && status.is_empty()
        && remote_repository.is_none()
        && workspace.is_none()
        && graph_reread_retention_v3.is_none()
        && original_tree_result_v1.is_none()
        && repository_source_identity.is_none()
        && github.is_none()
        && scoped_pr_tab.is_none()
        && matches!(
            scoped_pr_resource,
            Some(PrResourceCompletionV1::PersonalOpen(_))
        )
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
        && personal_readmission_v105.is_none()
}

#[cfg(test)]
#[path = "personal_pr_entry_v13_tests.rs"]
mod tests;
impl Background {
    pub fn submit_personal_pr_entry_v13(
        &mut self,
        app: &mut App,
        work: Arc<native::PersonalPrEntryWorkV13>,
        provider: crate::providers::gh::GithubProvider,
    ) -> bool {
        // Refuse BEFORE cancel, generation, owner, queue, loading or status.
        // Existing foreign slots are not replaced to open a Personal page.
        if work.expired()
            || !work.matches_capture(app)
            || app.personal_pr_entry_cancel_v13
            || self.current.contains_key(&View::Inbox.index())
            || self.current_personal_pr_entry_v13.is_some()
            || self.current_threads_owner_v1.is_some()
            || self.current_reviews_owner_v2.is_some()
            || self.current_inline_owner_v1.is_some()
            || self.current_pr_native_diff_v1.is_some()
            || app.loading[View::Inbox.index()]
        {
            work.cancellation().cancel();
            return false;
        }
        let Ok(owner) = native::PersonalPrEntryOwnerV13::new(app, work.clone()) else {
            return false;
        };
        let Some(generation) = self.generation.checked_add(1) else {
            return false;
        };
        let remote = ScopedRemoteOwner {
            collaboration_v17: None,
            notifications_v63: None,
            original: ScopedRemoteOriginalV13::OpeningPersonal(owner.clone()),
            actions_v1: None,
            pr_native_diff_v1: None,
            threads_v1: None,
            reviews_v2: None,
            inline_v1: None,
            check_origin: None,
            continuation_v2: None,
        };
        let worker = owner.clone();
        let request = Request {
            view: View::Inbox,
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
                    return Err("Original personal queue token changed".into());
                }
                let result = native::execute(worker.clone(), &provider);
                Ok(PreparedView {
                    scoped_pr_resource: Some(PrResourceCompletionV1::PersonalOpen(Box::new(
                        native::PersonalPrEntryResultV13 {
                            owner: worker,
                            result,
                        },
                    ))),
                    ..Default::default()
                })
            }),
        };
        let Some(queue) = self.requests.as_ref() else {
            return false;
        };
        match queue.try_send(request) {
            Ok(()) => {
                // The single foreground consumer cannot change App between
                // its final check and this nonfallible accepted installation.
                self.generation = generation;
                self.current.insert(
                    View::Inbox.index(),
                    (generation, owner.cancellation().clone()),
                );
                self.current_personal_pr_entry_v13 = Some((generation, owner.clone()));
                app.personal_pr_entry_owner_v13 = Some(owner);
                app.loading[View::Inbox.index()] = true;
                app.status =
                    "Verifying personal PR Files • original page retained • Esc Cancel".into();
                true
            }
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                work.cancellation().cancel();
                // No old-page, global generation, slot/loading/status effects.
                false
            }
        }
    }
    pub fn cancel_personal_pr_entry_v13(&mut self, app: &mut App) -> bool {
        let Some((generation, owner)) = self.current_personal_pr_entry_v13.take() else {
            // Generic cancellation can remove our logical fence before Main
            // drains the old App marker. Never touch a replacement slot.
            if let Some(owner) = app.personal_pr_entry_owner_v13.take() {
                owner.cancellation().cancel();
            }
            return false;
        };
        let same_slot = self
            .current
            .get(&View::Inbox.index())
            .is_some_and(|(g, t)| *g == generation && t.same_instance(owner.cancellation()));
        let same_app = app
            .personal_pr_entry_owner_v13
            .as_ref()
            .is_some_and(|v| Arc::ptr_eq(v, &owner));
        owner.cancellation().cancel();
        if same_slot {
            self.current.remove(&View::Inbox.index());
        }
        if same_app {
            app.personal_pr_entry_owner_v13 = None;
        }
        if same_slot {
            app.loading[View::Inbox.index()] = false;
            if owner.same_original_page(app) {
                app.status = "Personal Files cancelled • original cached page retained".into();
            }
        }
        true
    }
    pub fn retire_stale_personal_pr_entry_v13(&mut self, app: &mut App) {
        if self.current_personal_pr_entry_v13.is_none() {
            if let Some(owner) = app.personal_pr_entry_owner_v13.clone() {
                owner.retire_account_mismatch(app);
                if app
                    .personal_pr_entry_owner_v13
                    .as_ref()
                    .is_some_and(|v| Arc::ptr_eq(v, &owner))
                {
                    app.personal_pr_entry_owner_v13 = None;
                }
                owner.cancellation().cancel();
            }
            return;
        }
        if let Some((_, owner)) = &self.current_personal_pr_entry_v13 {
            // Privacy precedence is a genuine original-page/account check,
            // independent of whether our old physical slot still exists.
            owner.retire_account_mismatch(app);
        }
        if self
            .current_personal_pr_entry_v13
            .as_ref()
            .is_some_and(|(_, o)| !o.owns(app) || o.expired())
        {
            self.cancel_personal_pr_entry_v13(app);
        }
    }
    pub(super) fn poll_personal_pr_entry_v13(
        &mut self,
        app: &mut App,
        c: &mut Completion,
    ) -> Option<bool> {
        let owner = c
            .remote_scope_owner
            .as_ref()
            .and_then(|r| r.original.opening_personal())
            .cloned();
        let Some(owner) = owner else {
            if c.result.as_ref().is_ok_and(|p| {
                matches!(
                    p.scoped_pr_resource,
                    Some(PrResourceCompletionV1::PersonalOpen(_))
                )
            }) {
                return Some(false);
            }
            return None;
        };
        let remote = c
            .remote_scope_owner
            .as_ref()
            .expect("closed personal owner");
        // Mixed/stale/canceled results have no privacy/status/slot effects.
        if c.view != View::Inbox
            || c.catalogue_owner.is_some()
            || c.catalogue_recovery.is_some()
            || c.local_actor_owner.is_some()
            || c.remote_repository_owner.is_some()
            || c.local_navigation_owner.is_some()
            || c.original_tree_owner_v1.is_some()
            || c.original_diff_owner_v1.is_some()
            || c.original_stash_recovery_owner_v1.is_some()
            || remote.actions_v1.is_some()
            || remote.pr_native_diff_v1.is_some()
            || remote.threads_v1.is_some()
            || remote.reviews_v2.is_some()
            || remote.inline_v1.is_some()
            || remote.check_origin.is_some()
            || remote.continuation_v2.is_some()
            || owner.cancellation().is_cancelled()
            || native::newer_intent(app)
            || self
                .current_personal_pr_entry_v13
                .as_ref()
                .is_none_or(|(g, o)| *g != c.generation || !Arc::ptr_eq(o, &owner))
            || c.result.as_ref().is_ok_and(|p| {
                !only_personal(p)
                    || match &p.scoped_pr_resource {
                        Some(PrResourceCompletionV1::PersonalOpen(v)) => !v.matches(&owner),
                        _ => true,
                    }
            })
        {
            return Some(false);
        }
        if owner.retire_account_mismatch(app) {
            self.cancel_personal_pr_entry_v13(app);
            return Some(true);
        }
        if !remote.owns(app)
            || self
                .current
                .get(&View::Inbox.index())
                .is_none_or(|(g, t)| *g != c.generation || !t.same_instance(owner.cancellation()))
        {
            return Some(false);
        }
        self.current.remove(&View::Inbox.index());
        self.current_personal_pr_entry_v13 = None;
        let result = std::mem::replace(
            &mut c.result,
            Err("consumed original Personal result".into()),
        );
        if owner.expired() {
            native::terminal_failure(app, &owner, native::expired_failure());
        } else {
            match result {
                Ok(mut p) => {
                    let Some(PrResourceCompletionV1::PersonalOpen(v)) = p.scoped_pr_resource.take()
                    else {
                        unreachable!("closed result checked")
                    };
                    match v.result {
                        Ok(value) => {
                            native::publish(app, &owner, value);
                        }
                        Err(error) => native::terminal_failure(app, &owner, error),
                    }
                }
                Err(_) => native::terminal_failure(app, &owner, native::expired_failure()),
            }
        }
        Some(true)
    }
}
