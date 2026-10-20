//! Same Inbox slot/queue. Exact original owner, token, generation, result and
//! newer intent are checked BEFORE every slot/loading/error/privacy effect.
use super::*;
use crate::{notifications_native_v63 as native, notifications_v63::NotificationWorkV63};

#[cfg(test)]
#[path = "notifications_admission_v63_tests.rs"]
mod admission_tests;

pub(super) fn only_notifications_v63(p: &PreparedView) -> bool {
    let PreparedView {
        notifications_v63,
        scoped_pr_collaboration_v17,
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
    } = p;
    notifications_v63.is_some()
        && scoped_pr_collaboration_v17.is_none()
        && scoped_actions_v1.is_none()
        && scoped_threads_v1.is_none()
        && scoped_reviews_v2.is_none()
        && scoped_inline_v1.is_none()
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
}
impl Background {
    pub fn tick_notifications_v63(&mut self, app: &mut App) {
        native::tick(app);
    }
    #[cfg(test)]
    pub(crate) fn notification_completions_observed_v63(&self) -> usize {
        self.notifications_completion_observed_v63
    }
    pub fn submit_notifications_v63(
        &mut self,
        app: &mut App,
        owner: Arc<NotificationWorkV63>,
        provider: crate::providers::gh::GithubProvider,
        database: std::path::PathBuf,
    ) -> bool {
        if !owner.matches(app)
            || owner.expired()
            || native::newer_intent(app)
            || app.loading[View::Inbox.index()]
            || self.current.contains_key(&View::Inbox.index())
            || self.current_notifications_v63.is_some()
            || self.current_collaboration_v17.is_some()
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
            notifications_v63: Some(owner.clone()),
            collaboration_v17: None,
            actions_v1: None,
            pr_native_diff_v1: None,
            threads_v1: None,
            reviews_v2: None,
            inline_v1: None,
            original: ScopedRemoteOriginalV13::Notifications(owner.original().clone()),
            check_origin: None,
            continuation_v2: None,
        };
        if !remote.owns(app) {
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
            work: Box::new(move |same_token| {
                if !same_token.same_instance(worker.cancellation()) {
                    return Err("Original notification token changed".into());
                }
                #[cfg(test)]
                if worker.actual_worker_fault_v63() == 1 {
                    panic!("Controlled actual notification Background worker panic");
                }
                #[cfg(test)]
                let mixed = worker.actual_worker_fault_v63() == 2;
                let prepared = PreparedView {
                    notifications_v63: Some(crate::notifications_v63::execute(
                        worker, &provider, &database,
                    )),
                    ..Default::default()
                };
                #[cfg(test)]
                let prepared = if mixed {
                    PreparedView {
                        lines: vec!["MIXED FACET MUST NEVER PUBLISH".into()],
                        ..prepared
                    }
                } else {
                    prepared
                };
                Ok(prepared)
            }),
        };
        // Pure final admission check over the SAME original token/clock after
        // bounded request construction. No refusal cancels another live slot,
        // advances generation or hides an existing private presentation.
        if owner.expired()
            || !owner.matches(app)
            || native::newer_intent(app)
            || self.requests.is_none()
        {
            return false;
        }
        match self
            .requests
            .as_ref()
            .expect("active queue")
            .try_send(request)
        {
            Ok(()) => {
                self.generation = generation;
                self.current.insert(
                    View::Inbox.index(),
                    (generation, owner.cancellation().clone()),
                );
                self.current_notifications_v63 = Some((generation, owner.clone()));
                app.notifications_owner_v63 = Some(owner);
                app.loading[View::Inbox.index()] = true;
                app.status =
                    "Notification read queued • original clock includes queue time • Esc cancels"
                        .into();
                true
            }
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => false,
        }
    }
    pub(super) fn poll_notifications_completion_v63(
        &mut self,
        app: &mut App,
        completion: &mut Completion,
        slot: usize,
    ) -> Option<bool> {
        let owner = completion
            .remote_scope_owner
            .as_ref()
            .and_then(|remote| remote.notifications_v63.clone());
        let typed = completion
            .result
            .as_ref()
            .is_ok_and(|p| p.notifications_v63.is_some());
        // A declared closed purpose may restrict a completion, never supply
        // its missing nested owner or authorize generic physical-slot effects.
        let declared_notifications = completion
            .remote_scope_owner
            .as_ref()
            .is_some_and(|remote| {
                matches!(&remote.original, ScopedRemoteOriginalV13::Notifications(_))
            });
        // A genuine pending Inbox read cannot suppress Repository/Search or
        // another physical slot. A forged notification owner/facet on those
        // slots still refuses rather than falling through to generic effects.
        if slot != View::Inbox.index() || completion.view != View::Inbox {
            return (owner.is_some() || typed || declared_notifications).then_some(false);
        }
        let same_logical_completion =
            self.current_notifications_v63
                .as_ref()
                .is_some_and(|(generation, _)| {
                    *generation == completion.generation
                        && self.current.get(&slot).map(|entry| entry.0) == Some(*generation)
                });
        // Pending notification input still precedes every old Inbox effect.
        // A cancellation raised while retiring an OLD presentation cannot own
        // a genuinely newer generic Inbox generation. Main drains cancel flags
        // before retirement, so that orphan flag may coexist with its result.
        if app.notifications_work_v63.is_some()
            || (app.notifications_cancel_v63
                && (owner.is_some()
                    || typed
                    || declared_notifications
                    || same_logical_completion
                    || app.notifications_owner_v63.is_some()))
        {
            return Some(false);
        }
        let Some(owner) = owner else {
            if typed
                || declared_notifications
                || same_logical_completion
                || app.notifications_owner_v63.is_some()
            {
                return Some(false);
            }
            return None;
        };
        #[cfg(test)]
        {
            self.notifications_completion_observed_v63 = self
                .notifications_completion_observed_v63
                .checked_add(1)
                .expect("bounded genuine completion observer");
        }
        let remote = completion
            .remote_scope_owner
            .as_ref()
            .expect("actual nested owner");
        if slot != View::Inbox.index()
            || self.current.get(&slot).map(|value| value.0) != Some(completion.generation)
            || completion.view != View::Inbox
            || completion.catalogue_owner.is_some()
            || completion.catalogue_recovery.is_some()
            || completion.local_actor_owner.is_some()
            || completion.remote_repository_owner.is_some()
            || completion.local_navigation_owner.is_some()
            || completion.original_tree_owner_v1.is_some()
            || completion.original_diff_owner_v1.is_some()
            || completion.original_stash_recovery_owner_v1.is_some()
            || remote.collaboration_v17.is_some()
            || remote.actions_v1.is_some()
            || remote.pr_native_diff_v1.is_some()
            || remote.threads_v1.is_some()
            || remote.reviews_v2.is_some()
            || remote.inline_v1.is_some()
            || remote.check_origin.is_some()
            || remote.continuation_v2.is_some()
            || self
                .current_notifications_v63
                .as_ref()
                .is_none_or(|(generation, current)| {
                    *generation != completion.generation || !Arc::ptr_eq(current, &owner)
                })
            || app
                .notifications_owner_v63
                .as_ref()
                .is_none_or(|current| !Arc::ptr_eq(current, &owner))
            || !remote.owns(app)
            || native::newer_intent(app)
            || completion.result.as_ref().is_ok_and(|p| {
                !only_notifications_v63(p)
                    || p.notifications_v63
                        .as_ref()
                        .is_none_or(|p| !p.matches(&owner))
            })
        {
            return Some(false);
        }
        self.current.remove(&slot);
        self.current_notifications_v63 = None;
        app.loading[View::Inbox.index()] = false;
        match completion.result.as_mut() {
            Ok(prepared) if !owner.expired() => native::publish(
                app,
                &owner,
                prepared
                    .notifications_v63
                    .take()
                    .expect("checked exact result"),
            ),
            _ => native::failed(app, &owner),
        }
        Some(true)
    }
    pub fn cancel_notifications_v63(&mut self, app: &mut App) -> bool {
        let Some((generation, owner)) = self.current_notifications_v63.clone() else {
            return false;
        };
        // Cancellation of this actual original token is always safe. Visible
        // effects/slot removal require BOTH logical and physical ownership.
        owner.cancellation().cancel();
        if self.current.get(&View::Inbox.index()).map(|x| x.0) != Some(generation)
            || app
                .notifications_owner_v63
                .as_ref()
                .is_none_or(|current| !Arc::ptr_eq(current, &owner))
        {
            return false;
        }
        self.current.remove(&View::Inbox.index());
        self.current_notifications_v63 = None;
        app.notifications_owner_v63 = None;
        app.loading[View::Inbox.index()] = false;
        if owner.matches(app) {
            app.status = if owner.read_phase_v63()
                == crate::notifications_v63::NotificationReadPhaseV63::Publish
            {
                "Notification attempt canceled • outcome unconfirmed; inspect protected history"
            } else {
                "Notification read canceled • refresh explicitly"
            }
            .into();
        }
        true
    }
    pub fn retire_stale_notifications_v63(&mut self, app: &mut App) {
        native::retire_invalid(app);
        let Some((generation, owner)) = self.current_notifications_v63.clone() else {
            if let Some(owner) = app.notifications_owner_v63.clone() {
                owner.cancellation().cancel();
                let old_browser = owner.matches(app);
                if app
                    .notifications_owner_v63
                    .as_ref()
                    .is_some_and(|current| Arc::ptr_eq(current, &owner))
                {
                    app.notifications_owner_v63 = None;
                }
                if old_browser {
                    app.notifications_v63 = None;
                    if app.overlay == Some(crate::app::Overlay::NotificationsV63) {
                        app.overlay = None;
                        app.hits.clear();
                    }
                }
            }
            return;
        };
        if owner.matches(app)
            && !owner.expired()
            && !native::newer_intent(app)
            && self.current.get(&View::Inbox.index()).map(|entry| entry.0) == Some(generation)
        {
            return;
        }
        owner.cancellation().cancel();
        if self.current.get(&View::Inbox.index()).map(|x| x.0) == Some(generation) {
            self.current.remove(&View::Inbox.index());
            app.loading[View::Inbox.index()] = false;
        }
        self.current_notifications_v63 = None;
        if app
            .notifications_owner_v63
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, &owner))
        {
            app.notifications_owner_v63 = None;
        }
        // No stale error/status/privacy/focus effect and no replacement token
        // cancellation. The UI's exact source mismatch gate owns body hiding.
    }
}
