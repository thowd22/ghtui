//! Specialized source/ticket gate on the SAME existing Inbox queue and slot.
use super::*;
use crate::scoped_pr_observed_tabs_v1::resources::immutable_images::inline_v1::native_v1 as inline;
pub(super) fn only_inline(p: &PreparedView) -> bool {
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
    scoped_inline_v1.is_some()
        && scoped_pr_collaboration_v17.is_none()
        && scoped_threads_v1.is_none()
        && scoped_reviews_v2.is_none()
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
    pub fn retire_stale_scoped_inline_v1(&mut self, app: &mut App) {
        let Some(owner) = self.current_inline_owner_v1.clone() else {
            if let Some(old) = app.scoped_inline_owner_v1.take() {
                old.cancellation().cancel();
                if old.work().phase() == inline::ScopedInlinePhaseV1::Confirm {
                    inline::retire_orphaned_confirm_presentation_v14(app, &old);
                    return;
                }
                let original_context = old.work().intent().owns(app);
                if let (Some(request), Some(form)) = (old.work().request(), &mut app.form) {
                    if original_context {
                        request.retire_busy_presentation(form);
                    }
                }
            }
            return;
        };
        if owner.work().matches(app) {
            return;
        }
        let Some(generation) = self.current_inline_generation_v1 else {
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
        self.current_inline_owner_v1 = None;
        self.current_inline_generation_v1 = None;
        if owner.work().intent().owns(app) {
            if let (Some(request), Some(form)) = (owner.work().request(), &mut app.form) {
                request.retire_busy_presentation(form);
            }
        }
        if owner.work().phase() == inline::ScopedInlinePhaseV1::Confirm
            && owner.original_private_source_is_current(app)
        {
            inline::retire_private(app, self);
            app.status = "Original inline presentation retired • outcome may be unknown • draft/journal preserved • nothing resent; refresh explicitly".into();
            return;
        }
        if app
            .scoped_inline_owner_v1
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, &owner))
        {
            app.scoped_inline_owner_v1 = None;
        }
        app.loading[View::Inbox.index()] = false;
    }
    pub fn submit_scoped_inline_v1(
        &mut self,
        app: &mut App,
        work: Arc<inline::ScopedInlineWorkV1>,
        provider: crate::providers::gh::GithubProvider,
        database: std::path::PathBuf,
        drafts: crate::drafts::DraftClient,
    ) -> bool {
        if !work.matches(app) || inline::newer_intent(app) {
            return false;
        }
        let owner = match inline::ScopedInlineOwnerV1::new(work, &provider) {
            Ok(owner) => owner,
            Err(reason) => {
                app.status = reason;
                return false;
            }
        };
        let (scope, source, navigation) = owner.remote_owners();
        if crate::remote_scope_gate_v1::stop_before_fallback(Some(&scope)).is_some() {
            return false;
        }
        if owner.expired() || owner.capture_publication_v14(&database, &drafts).is_err() {
            return false;
        }
        self.cancel(View::Inbox);
        self.generation += 1;
        let generation = self.generation;
        app.scoped_inline_owner_v1 = Some(owner.clone());
        let remote = ScopedRemoteOwner {
            collaboration_v17: None,
            notifications_v63: None,
            actions_v1: None,
            threads_v1: None,
            original: ScopedRemoteOriginalV13::Read {
                scope,
                source,
                navigation,
            },
            check_origin: None,
            continuation_v2: None,
            pr_native_diff_v1: None,
            inline_v1: Some(owner.clone()),
            reviews_v2: None,
        };
        if !remote.owns(app) {
            app.scoped_inline_owner_v1 = None;
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
            work: Box::new(move |_original_same_token| {
                Ok(PreparedView {
                    scoped_inline_v1: Some(inline::execute(
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
                self.current_inline_owner_v1 = Some(owner.clone());
                self.current_inline_generation_v1 = Some(generation);
                app.loading[View::Inbox.index()] = true;
                if let Some(form) = &mut app.form {
                    if owner.work().request().is_some() {
                        form.submitting = true;
                    }
                }
                app.status = format!(
                    "Original inline {:?} pending • Esc cancels original operation",
                    owner.work().phase()
                );
                true
            }
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                owner.cancellation().cancel();
                app.scoped_inline_owner_v1 = None;
                app.loading[View::Inbox.index()] = false;
                app.status = "Inline queue full • nothing submitted • retry explicitly".into();
                false
            }
        }
    }
    pub fn cancel_scoped_inline_v1(&mut self, app: &mut App) -> bool {
        let Some(owner) = self.current_inline_owner_v1.clone() else {
            return false;
        };
        let Some(generation) = self.current_inline_generation_v1 else {
            return false;
        };
        if app
            .scoped_inline_owner_v1
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
        self.current_inline_owner_v1 = None;
        self.current_inline_generation_v1 = None;
        if let Some(origin) = owner.work().native_origin() {
            app.scoped_inline_owner_v1 = None;
            app.loading[View::Inbox.index()] = false;
            crate::scoped_pr_observed_tabs_v1::resources::immutable_images::inline_v1::native_entry_v1::return_native(app,origin);
            return true;
        }
        if (owner.work().matches(app) && !inline::newer_intent(app))
            || (owner.work().phase() == inline::ScopedInlinePhaseV1::Confirm
                && owner.original_private_source_is_current(app))
        {
            inline::retire_private(app, self);
            app.status="Original inline operation canceled • draft/journal preserved • nothing resent; refresh explicitly".into();
        } else {
            // Cancel only the exact old owned job, never fence/clear a newer
            // chosen source, model, queued intent or unrelated live slot.
            if owner.work().intent().owns(app) {
                if let (Some(request), Some(form)) = (owner.work().request(), &mut app.form) {
                    request.retire_busy_presentation(form);
                }
            }
            app.scoped_inline_owner_v1 = None;
            app.loading[View::Inbox.index()] = false;
        }
        true
    }
}
