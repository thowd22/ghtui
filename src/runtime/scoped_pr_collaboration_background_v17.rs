//! Exact-purpose admission on the SAME queue. All mixed/newer/source/slot
//! checks precede removal, status, loading, privacy and late panic effects.
use super::*;
use crate::scoped_pr_collaboration_native_v17 as native;
const VERIFYING_ORIGINAL_V70: &str =
    "Verifying original Conversation • Esc Cancel • nothing resent";
#[cfg(test)]
#[path = "scoped_pr_collaboration_close_v20_tests.rs"]
mod close_v20_tests;
#[cfg(test)]
#[path = "scoped_pr_collaboration_refresh_v19_tests.rs"]
mod refresh_v19_tests;
#[cfg(test)]
#[path = "scoped_pr_collaboration_refresh_retry_v20_tests.rs"]
mod retry_v20_tests;

fn only_collaboration(p: &PreparedView) -> bool {
    p.scoped_pr_collaboration_v17.is_some()
        && p.notifications_v63.is_none()
        && p.scoped_actions_v1.is_none()
        && p.scoped_threads_v1.is_none()
        && p.scoped_reviews_v2.is_none()
        && p.scoped_inline_v1.is_none()
        && p.original_stash_recovery_v1.is_none()
        && p.original_diff_result_v1.is_none()
        && p.related_pr_v1.is_none()
        && p.lines.is_empty()
        && p.context.is_none()
        && p.status.is_empty()
        && p.remote_repository.is_none()
        && p.workspace.is_none()
        && p.graph_reread_retention_v3.is_none()
        && p.original_tree_result_v1.is_none()
        && p.repository_source_identity.is_none()
        && p.github.is_none()
        && p.scoped_pr_tab.is_none()
        && p.scoped_pr_resource.is_none()
        && p.form.is_none()
        && p.search.is_none()
        && p.navigate_to.is_none()
        && !p.restore_search_input
        && p.actions.is_none()
        && p.pr_checks_origin.is_none()
        && p.pr_checks_route.is_none()
        && p.scoped_pr_checks.is_none()
        && p.pr_check_continuation_v2.is_none()
        && p.actions_inspection.is_none()
        && !p.clear_actions_inspection
        && !p.clear_private
        && !p.activate_if_untouched
        && p.pr_inspection.is_none()
        && !p.clear_pr_inspection
        && p.pr_pending_message.is_none()
        && p.verified_identity.is_none()
        && p.pr_reactions.is_none()
        && p.pr_reaction_display.is_none()
        && p.readonly_notice.is_none()
        && p.source_message.is_none()
        && p.source_context.is_none()
        && p.metrics.is_none()
        && !p.clear_metrics
        && p.github_origin.is_none()
        && !p.invalidate_metrics
        && !p.invalidate_pr_metadata
        && p.navigation_restore.is_none()
        && p.catalogue.is_none()
        && p.local_actor_result.is_none()
        && p.remote_scope.is_none()
        && p.remote_scope_source.is_none()
}
impl Background {
    /// Observe and put back the SAME completed worker message before a State
    /// Preview test deliberately changes its presentation. No result/owner
    /// construction or admission happens here; production has no such API.
    #[cfg(test)]
    pub(crate) fn observe_state_preview_completion_v99(
        &mut self,
        app: &App,
        owner: &Arc<native::ScopedPrCollaborationOwnerV17>,
    ) {
        assert_eq!(
            owner.work().phase(),
            native::ConversationPhaseKindV17::Preview
        );
        let form = app.form.as_ref().expect("Actual State Preview Form");
        assert!(crate::scoped_pr_collaboration_ui_v17::state_v68::state(form).is_some());
        assert!(form.fields.is_empty() && form.submitting);
        assert!(owner.owns(app));
        assert!(app
            .scoped_pr_collaboration_owner_v17
            .as_ref()
            .is_some_and(|actual| Arc::ptr_eq(actual, owner)));
        let (generation, token) = self
            .current
            .get(&View::Inbox.index())
            .expect("Actual current physical Inbox generation/token");
        assert!(token.same_instance(owner.cancellation()));
        assert!(self
            .current_collaboration_v17
            .as_ref()
            .is_some_and(|(g, actual)| *g == *generation && Arc::ptr_eq(actual, owner)));
        let completion = self
            .completions
            .recv_timeout(std::time::Duration::from_secs(8))
            .expect("SAME actual State Preview worker completion within original test bound");
        assert_eq!(completion.view, View::Inbox);
        assert_eq!(completion.generation, *generation);
        assert!(
            completion.catalogue_owner.is_none()
                && completion.catalogue_recovery.is_none()
                && completion.local_actor_owner.is_none()
                && completion.remote_repository_owner.is_none()
                && completion.local_navigation_owner.is_none()
                && completion.original_tree_owner_v1.is_none()
                && completion.original_diff_owner_v1.is_none()
                && completion.original_stash_recovery_owner_v1.is_none()
        );
        let remote = completion
            .remote_scope_owner
            .as_ref()
            .expect("Actual remote owner");
        assert!(remote
            .collaboration_v17
            .as_ref()
            .is_some_and(|actual| Arc::ptr_eq(actual, owner)));
        assert!(remote.owns(app));
        assert!(
            remote.notifications_v63.is_none()
                && remote.actions_v1.is_none()
                && remote.threads_v1.is_none()
                && remote.reviews_v2.is_none()
                && remote.inline_v1.is_none()
                && remote.pr_native_diff_v1.is_none()
                && remote.check_origin.is_none()
                && remote.continuation_v2.is_none()
        );
        let prepared = completion
            .result
            .as_ref()
            .expect("Actual closed worker result");
        assert!(only_collaboration(prepared));
        assert!(prepared
            .scoped_pr_collaboration_v17
            .as_ref()
            .is_some_and(|actual| actual.matches(owner)));
        self.inline_completion_sender_v1
            .send(completion)
            .expect("Requeue ORIGINAL unmodified actual completion on SAME channel");
    }
    pub fn submit_scoped_pr_collaboration_v17(
        &mut self,
        app: &mut App,
        work: Arc<native::ScopedPrCollaborationWorkV17>,
        provider: crate::providers::gh::GithubProvider,
        database: std::path::PathBuf,
        drafts: crate::drafts::DraftClient,
    ) -> bool {
        if !work.matches(app)
            || native::newer_intent(app)
            || native::retired_presentation(app)
            || app.scoped_pr_collaboration_cancel_v17
            || app.loading[View::Inbox.index()]
            || self.current.contains_key(&View::Inbox.index())
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
        let Some(requests) = self.requests.as_ref() else {
            return false;
        };
        let Ok(owner) = native::ScopedPrCollaborationOwnerV17::new(work, &provider) else {
            return false;
        };
        if owner.expired() || owner.capture_publication(&database, &drafts).is_err() {
            return false;
        }
        let Some(generation) = self.generation.checked_add(1) else {
            return false;
        };
        let remote = ScopedRemoteOwner {
            collaboration_v17: Some(owner.clone()),
            notifications_v63: None,
            original: match owner.work().refresh_request_v19() {
                Some(r) => ScopedRemoteOriginalV13::CollaborationRefreshV19(r.clone()),
                None => match owner.original() {
                    native::CollaborationWorkOriginV78::PrPage(original) => {
                        ScopedRemoteOriginalV13::CollaborationV17(Arc::clone(original))
                    }
                    native::CollaborationWorkOriginV78::StateInbox(source) => {
                        ScopedRemoteOriginalV13::StateSourceV80(Arc::clone(source))
                    }
                },
            },
            actions_v1: None,
            pr_native_diff_v1: None,
            threads_v1: None,
            reviews_v2: None,
            inline_v1: None,
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
            work: Box::new(move |actual_token| {
                if !actual_token.same_instance(worker.cancellation()) {
                    return Err("Original Conversation worker token changed".into());
                }
                Ok(PreparedView {
                    scoped_pr_collaboration_v17: Some(native::execute(
                        worker, &provider, &database, &drafts,
                    )),
                    ..Default::default()
                })
            }),
        };
        // Queue-full/disconnected/expired refusal does not cancel any slot or
        // move a page/Form/body. Install only after actual try_send acceptance.
        match requests.try_send(request) {
            Ok(()) => {
                self.generation = generation;
                self.current.insert(
                    View::Inbox.index(),
                    (generation, owner.cancellation().clone()),
                );
                self.current_collaboration_v17 = Some((generation, owner.clone()));
                let waiting_drafts = owner.waiting_open_drafts_v20();
                let state_source = matches!(
                    owner.original(),
                    native::CollaborationWorkOriginV78::StateInbox(_)
                );
                app.scoped_pr_collaboration_owner_v17 = Some(owner);
                app.loading[View::Inbox.index()] = true;
                if let Some(form) = &mut app.form {
                    if matches!(
                        form.purpose,
                        crate::forms::FormPurpose::PrCollaborationV17(_)
                    ) {
                        form.submitting = true;
                    }
                }
                app.status = if state_source {
                    "Reading actual State queue source • membership unavailable until identity closes".into()
                } else if waiting_drafts {
                    "Queued behind accepted local draft saves • original Conversation read".into()
                } else {
                    VERIFYING_ORIGINAL_V70.into()
                };
                true
            }
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => false,
        }
    }
    pub(super) fn poll_scoped_pr_collaboration_v17(
        &mut self,
        app: &mut App,
        completion: &mut Completion,
    ) -> Option<bool> {
        let Some(owner) = completion
            .remote_scope_owner
            .as_ref()
            .and_then(|r| r.collaboration_v17.clone())
        else {
            if completion
                .result
                .as_ref()
                .is_ok_and(|p| p.scoped_pr_collaboration_v17.is_some())
            {
                return Some(false); // no raw/unowned facet can escape
            }
            return None;
        };
        let remote = completion
            .remote_scope_owner
            .as_ref()
            .expect("actual owner above");
        if completion.view != View::Inbox
            || completion.catalogue_owner.is_some()
            || completion.catalogue_recovery.is_some()
            || completion.local_actor_owner.is_some()
            || completion.remote_repository_owner.is_some()
            || completion.local_navigation_owner.is_some()
            || completion.original_tree_owner_v1.is_some()
            || completion.original_diff_owner_v1.is_some()
            || completion.original_stash_recovery_owner_v1.is_some()
            || remote.notifications_v63.is_some()
            || remote.actions_v1.is_some()
            || remote.threads_v1.is_some()
            || remote.reviews_v2.is_some()
            || remote.inline_v1.is_some()
            || remote.pr_native_diff_v1.is_some()
            || remote.check_origin.is_some()
            || remote.continuation_v2.is_some()
            || native::pending_intent(app)
            || native::newer_intent(app)
            || !remote.owns(app)
            || app
                .scoped_pr_collaboration_owner_v17
                .as_ref()
                .is_none_or(|o| !Arc::ptr_eq(o, &owner))
            || self
                .current_collaboration_v17
                .as_ref()
                .is_none_or(|(g, o)| *g != completion.generation || !Arc::ptr_eq(o, &owner))
            || self.current.get(&View::Inbox.index()).is_none_or(|(g, t)| {
                *g != completion.generation || !t.same_instance(owner.cancellation())
            })
            || completion.result.as_ref().is_ok_and(|p| {
                !only_collaboration(p)
                    || p.scoped_pr_collaboration_v17
                        .as_ref()
                        .is_none_or(|r| !r.matches(&owner))
            })
        {
            return Some(false);
        }
        self.current.remove(&View::Inbox.index());
        self.current_collaboration_v17 = None;
        app.loading[View::Inbox.index()] = false;
        match &mut completion.result {
            Ok(p) if !owner.expired() => native::publish(
                app,
                &owner,
                p.scoped_pr_collaboration_v17
                    .take()
                    .expect("closed facet checked"),
            ),
            _ => native::failed(app, &owner),
        }
        if app
            .scoped_pr_collaboration_owner_v17
            .as_ref()
            .is_some_and(|o| Arc::ptr_eq(o, &owner))
        {
            app.scoped_pr_collaboration_owner_v17 = None;
        }
        Some(true)
    }
    pub fn retire_stale_scoped_pr_collaboration_v17(&mut self, app: &mut App) {
        crate::scoped_pr_collaboration_ui_v17::pure_close_v20::retire_invalid(app);
        crate::scoped_pr_collaboration_refresh_v19::return_v19::retire_stale(app);
        let Some((generation, owner)) = self.current_collaboration_v17.clone() else {
            let Some(old) = app.scoped_pr_collaboration_owner_v17.clone() else {
                return;
            };
            if matches!(old.original(),native::CollaborationWorkOriginV78::PrPage(original) if original.retire_account_mismatch(app))
            {
                native::drop_old_form(app, &old);
                app.scoped_pr_collaboration_owner_v17 = None;
                return;
            }
            if old.display_retired() {
                if !matches!(old.original(),native::CollaborationWorkOriginV78::PrPage(original) if original.same_presentation(app))
                {
                    app.scoped_pr_collaboration_owner_v17 = None;
                }
                return;
            }
            old.cancellation().cancel();
            native::retire_orphan(app, &old);
            if !old.display_retired() {
                app.scoped_pr_collaboration_owner_v17 = None;
            }
            return;
        };
        if owner.owns(app) && !owner.expired() {
            return;
        }
        if self
            .current
            .get(&View::Inbox.index())
            .is_none_or(|(g, t)| *g != generation || !t.same_instance(owner.cancellation()))
        {
            // Generic replacement owns its physical slot. Retire only this
            // logical owner; NEVER cancel/remove the replacement request.
            self.current_collaboration_v17 = None;
            owner.cancellation().cancel();
            native::retire_orphan(app, &owner);
            return;
        }
        let original = owner.work().matches(app);
        // Display cleanup only, after the exact physical-slot check. A caption
        // never grants ownership: the actual App owner, page and scope do.
        let stopped_metadata = owner.work().phase()
            == native::ConversationPhaseKindV17::MetadataSectionV51
            && app.view == View::Inbox
            && app
                .scoped_pr_collaboration_owner_v17
                .as_ref()
                .is_some_and(|old| Arc::ptr_eq(old, &owner))
            && matches!(owner.original(),native::CollaborationWorkOriginV78::PrPage(original) if original.same_presentation(app))
            && app.github.as_ref().is_some_and(|github| {
                github.scope().ok().as_ref() == Some(owner.original().scope())
            })
            && app.form.is_none()
            && app.overlay.is_none()
            && !native::pending_intent(app)
            && !native::newer_intent(app)
            && app.status == VERIFYING_ORIGINAL_V70;
        owner.cancellation().cancel();
        self.current.remove(&View::Inbox.index());
        self.current_collaboration_v17 = None;
        // The physical slot above is still exactly this generation/token.
        // Losing only its presentation (for example a genuine scroll) must
        // not leave the removed request marked loading. The mismatched/newer
        // physical-slot branch returns before this point and stays untouched.
        app.loading[View::Inbox.index()] = false;
        if stopped_metadata {
            app.status =
                "Original Metadata read stopped • read again explicitly • nothing resent".into();
        }
        if original && owner.work().refresh_request_v19().is_some() && !native::newer_intent(app) {
            native::cancel_refresh_v19(app, &owner);
        } else {
            native::retire_orphan(app, &owner);
        }
        if !owner.display_retired()
            && app
                .scoped_pr_collaboration_owner_v17
                .as_ref()
                .is_some_and(|o| Arc::ptr_eq(o, &owner))
        {
            app.scoped_pr_collaboration_owner_v17 = None;
        }
    }
    pub fn cancel_scoped_pr_collaboration_v17(&mut self, app: &mut App) -> bool {
        let Some((generation, owner)) = self.current_collaboration_v17.clone() else {
            return false;
        };
        let original = owner.work().refresh_request_v19().map_or_else(
            || match owner.original() {
                native::CollaborationWorkOriginV78::PrPage(original) => {
                    original.owns_presentation(app)
                }
                native::CollaborationWorkOriginV78::StateInbox(source) => source.owns(app),
            },
            |request| request.owns_cancel_v19(app),
        );
        if !original
            || native::newer_intent(app)
            || app
                .scoped_pr_collaboration_owner_v17
                .as_ref()
                .is_none_or(|o| !Arc::ptr_eq(o, &owner))
            || self
                .current
                .get(&View::Inbox.index())
                .is_none_or(|(g, t)| *g != generation || !t.same_instance(owner.cancellation()))
        {
            return false;
        }
        owner.cancellation().cancel();
        self.current.remove(&View::Inbox.index());
        self.current_collaboration_v17 = None;
        app.loading[View::Inbox.index()] = false;
        if owner.work().refresh_request_v19().is_some() {
            native::cancel_refresh_v19(app, &owner);
        } else if owner.work().phase() == native::ConversationPhaseKindV17::Confirm {
            native::retire_terminal(app, &owner);
        } else {
            native::drop_old_form(app, &owner);
        }
        app.scoped_pr_collaboration_owner_v17 = None;
        app.status =
            "Conversation canceled • original draft/journal retained • nothing resent".into();
        true
    }
}
