//! Specialized source/ticket gate on the SAME existing Inbox queue and slot.
use super::*;
use crate::scoped_pr_observed_tabs_v1::resources::immutable_images::reviews_v2::native_v2 as reviews;
pub(super) fn only_reviews(p: &PreparedView) -> bool {
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
    scoped_reviews_v2.is_some()
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
    pub fn retire_stale_scoped_reviews_v2(&mut self, app: &mut App) {
        let Some(owner) = self.current_reviews_owner_v2.clone() else {
            if let Some(old) = app.scoped_reviews_owner_v2.take() {
                old.cancellation().cancel();
                if old.work().phase() == reviews::ScopedReviewsPhaseV2::Confirm {
                    reviews::retire_orphaned_confirm_presentation(app, &old);
                    return;
                }
                let original_context = old.work().origin().owns(app);
                if let Some(form) = &mut app.form {
                    if original_context {
                        old.work().retire_form_busy_v3(form);
                    }
                }
            }
            return;
        };
        if owner.work().matches(app) && !reviews::newer_intent(app) {
            return;
        }
        let Some(generation) = self.current_reviews_generation_v2 else {
            return;
        };
        if self
            .current
            .get(&View::Inbox.index())
            .is_none_or(|(current, _)| *current != generation)
        {
            return;
        }
        owner.cancellation().cancel();
        self.current.remove(&View::Inbox.index());
        self.current_reviews_owner_v2 = None;
        self.current_reviews_generation_v2 = None;
        if owner.work().origin().owns(app) {
            if let Some(form) = &mut app.form {
                owner.work().retire_form_busy_v3(form);
            }
        }
        if owner.work().phase() == reviews::ScopedReviewsPhaseV2::Confirm
            && owner.original_private_source_is_current(app)
        {
            reviews::retire_private(app, self);
            app.status = "Original review presentation retired • outcome may be unknown • draft/journal preserved • nothing resent; refresh explicitly".into();
            return;
        }
        if app
            .scoped_reviews_owner_v2
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, &owner))
        {
            app.scoped_reviews_owner_v2 = None;
        }
        app.loading[View::Inbox.index()] = false;
    }
    pub fn submit_scoped_reviews_v2(
        &mut self,
        app: &mut App,
        work: Arc<reviews::ScopedReviewsWorkV2>,
        provider: crate::providers::gh::GithubProvider,
        database: std::path::PathBuf,
        drafts: crate::drafts::DraftClient,
    ) -> bool {
        if work.expired() || !work.matches(app) || reviews::newer_intent(app) {
            return false;
        }
        let original_work = work.clone();
        let owner = match reviews::ScopedReviewsOwnerV2::new(work, &database, &drafts) {
            Ok(owner) => owner,
            Err(reason) => {
                if !original_work.expired() {
                    app.status = reason;
                }
                return false;
            }
        };
        if owner.expired() {
            return false;
        }
        if owner.work().origin().active().is_some_and(|scope| {
            crate::remote_scope_gate_v1::stop_before_fallback(Some(scope)).is_some()
        }) {
            return false;
        }
        let original = if let Some(summary) = owner
            .work()
            .origin()
            .summary()
            .and_then(|intent| intent.personal_summary_v15())
        {
            ScopedRemoteOriginalV13::PersonalSummary(summary.clone())
        } else if let Some((scope, source, navigation)) = owner.remote_owners() {
            ScopedRemoteOriginalV13::Read {
                scope,
                source,
                navigation,
            }
        } else {
            return false;
        };
        self.cancel(View::Inbox);
        self.generation += 1;
        let generation = self.generation;
        app.scoped_reviews_owner_v2 = Some(owner.clone());
        let remote = ScopedRemoteOwner {
            collaboration_v17: None,
            notifications_v63: None,
            actions_v1: None,
            pr_native_diff_v1: None,
            threads_v1: None,
            original,
            check_origin: None,
            continuation_v2: None,
            reviews_v2: Some(owner.clone()),
            inline_v1: None,
        };
        if !remote.owns(app) {
            app.scoped_reviews_owner_v2 = None;
            return false;
        }
        let worker_owner = owner.clone();
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
            work: Box::new(move |original_same_token| {
                if !original_same_token.same_instance(worker_owner.cancellation()) {
                    return Err("Original review queue token changed; no private result".into());
                }
                Ok(PreparedView {
                    scoped_reviews_v2: Some(reviews::execute(
                        worker_owner,
                        &provider,
                        &database,
                        &drafts,
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
                self.current_reviews_owner_v2 = Some(owner.clone());
                self.current_reviews_generation_v2 = Some(generation);
                app.loading[View::Inbox.index()] = true;
                if let Some(form) = &mut app.form {
                    if owner.work().request().is_some()
                        || owner.work().operation_request_v3().is_some()
                    {
                        form.submitting = true;
                    }
                }
                app.status = format!(
                    "Original review {:?} pending • Esc cancels original operation",
                    owner.work().phase()
                );
                true
            }
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                owner.cancellation().cancel();
                app.scoped_reviews_owner_v2 = None;
                app.loading[View::Inbox.index()] = false;
                app.status = "Reviews queue full • nothing submitted • retry explicitly".into();
                false
            }
        }
    }
    pub fn cancel_scoped_reviews_v2(&mut self, app: &mut App) -> bool {
        let Some(owner) = self.current_reviews_owner_v2.clone() else {
            return false;
        };
        let Some(generation) = self.current_reviews_generation_v2 else {
            return false;
        };
        if app
            .scoped_reviews_owner_v2
            .as_ref()
            .is_none_or(|current| !Arc::ptr_eq(current, &owner))
            || self
                .current
                .get(&View::Inbox.index())
                .is_none_or(|(current, _)| *current != generation)
        {
            return false;
        }
        owner.cancellation().cancel();
        self.current.remove(&View::Inbox.index());
        self.current_reviews_owner_v2 = None;
        self.current_reviews_generation_v2 = None;
        if (owner.work().matches(app) && !reviews::newer_intent(app))
            || (owner.work().phase() == reviews::ScopedReviewsPhaseV2::Confirm
                && owner.original_private_source_is_current(app))
        {
            reviews::retire_private(app, self);
            app.status="Original review operation canceled • draft/journal preserved • nothing resent; refresh explicitly".into();
        } else {
            // Cancel only the exact old owned job, never fence/clear a newer
            // chosen source, model, queued intent or unrelated live slot.
            if owner.work().origin().owns(app) {
                if let Some(form) = &mut app.form {
                    owner.work().retire_form_busy_v3(form);
                }
            }
            app.scoped_reviews_owner_v2 = None;
            app.loading[View::Inbox.index()] = false;
        }
        true
    }
}
