//! Closed native PR producer on the SAME Inbox request queue/result facet.
use super::*;
use crate::scoped_pr_native_diff_native_v1 as native;
pub(super) fn only_native(p: &PreparedView) -> bool {
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
        scoped_reviews_v2,
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
        refresh_authored_queue,
        navigation_restore,
        catalogue,
        local_actor_result,
        remote_scope,
        remote_scope_source,
        personal_readmission_v105,
        remote_repository,
    } = p;
    scoped_threads_v1.is_none()
        && scoped_reviews_v2.is_none()
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
        && scoped_pr_resource
            .as_ref()
            .is_some_and(native::resource_is_native)
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
        && remote_repository.is_none()
}

impl Background {
    pub fn submit_pr_native_diff_v1(
        &mut self,
        app: &mut App,
        work: Arc<native::PrNativeWorkV1>,
        provider: crate::providers::gh::GithubProvider,
        database: std::path::PathBuf,
    ) -> bool {
        let personal = work.bounded_origin_v63();
        if self.requests.is_none()
            || !work.matches(app)
            || native::newer_intent(app)
            || (personal
                && (self.current.contains_key(&View::Inbox.index())
                    || self.current_pr_native_diff_v1.is_some()
                    || self.current_personal_pr_entry_v13.is_some()
                    || self.current_threads_owner_v1.is_some()
                    || self.current_reviews_owner_v2.is_some()
                    || self.current_inline_owner_v1.is_some()
                    || self.current_scoped_actions_v1.is_some()
                    || app.loading[View::Inbox.index()]))
        {
            work.restore_not_submitted(app);
            return false;
        }
        let owner = match native::PrNativeOwnerV1::new_for_provider(work.clone(), &provider) {
            Ok(owner) => owner,
            Err(reason) => {
                work.restore_not_submitted(app);
                if !work.bounded_origin_v63() {
                    app.status = reason;
                }
                return false;
            }
        };
        let (scope, source, navigation) = owner.remote_owners();
        if crate::remote_scope_gate_v1::stop_before_fallback(Some(&scope)).is_some() {
            work.restore_not_submitted(app);
            return false;
        }
        let Some(generation) = self.generation.checked_add(1) else {
            return false;
        };
        if !personal {
            self.cancel(View::Inbox);
            self.generation = generation;
            app.pr_native_diff_owner_v1 = Some(owner.clone());
        }
        let remote = ScopedRemoteOwner {
            collaboration_v17: None,
            notifications_v63: None,
            actions_v1: None,
            reviews_v2: None,
            pr_native_diff_v1: Some(owner.clone()),
            threads_v1: None,
            inline_v1: None,
            original: ScopedRemoteOriginalV13::Read {
                scope,
                source,
                navigation,
            },
            check_origin: None,
            continuation_v2: None,
        };
        if !remote.original.owns(app) || (!personal && !remote.owns(app)) {
            if !personal {
                app.pr_native_diff_owner_v1 = None;
            }
            work.restore_not_submitted(app);
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
            work: Box::new(move |same_original_token| {
                if !same_original_token.same_instance(worker.cancellation()) {
                    return Err("Original Native request token changed".into());
                }
                let prepared = native::execute(worker, &provider, &database)?;
                Ok(PreparedView { scoped_pr_resource: Some(crate::scoped_pr_observed_tabs_v1::resources::native::PrResourceCompletionV1::Native(Box::new(prepared))), ..Default::default() })
            }),
        };
        match self
            .requests
            .as_ref()
            .expect("active queue")
            .try_send(request)
        {
            Ok(()) => {
                if personal {
                    // Same retained App page throughout queue admission. No
                    // fallible work remains before installing this exact owner.
                    self.generation = generation;
                    app.pr_native_diff_owner_v1 = Some(owner.clone());
                }
                self.current.insert(
                    View::Inbox.index(),
                    (generation, owner.cancellation().clone()),
                );
                self.current_pr_native_diff_v1 = Some((generation, owner));
                app.loading[View::Inbox.index()] = true;
                app.status = "Original native PR file pending • verification + preparation share one original operation".into();
                true
            }
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                owner.cancellation().cancel();
                work.restore_not_submitted(app);
                if !personal {
                    app.pr_native_diff_owner_v1 = None;
                    app.loading[View::Inbox.index()] = false;
                    app.status = "Native PR queue unavailable • request not submitted • no mutation • retry explicitly".into();
                }
                false
            }
        }
    }
    pub fn cancel_pr_native_diff_v1(&mut self, app: &mut App) -> bool {
        let Some((generation, owner)) = self.current_pr_native_diff_v1.take() else {
            return false;
        };
        owner.cancellation().cancel();
        // The retained logical token belongs to us even after an old result was
        // discarded. A newer foreign Inbox physical generation is never removed.
        let same_slot = self
            .current
            .get(&View::Inbox.index())
            .is_some_and(|(current, token)| {
                *current == generation && token.same_instance(owner.cancellation())
            });
        if same_slot {
            self.current.remove(&View::Inbox.index());
        }
        if app
            .pr_native_diff_owner_v1
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, &owner))
        {
            app.pr_native_diff_owner_v1 = None;
        }
        if same_slot || !self.current.contains_key(&View::Inbox.index()) {
            app.loading[View::Inbox.index()] = false;
        }
        true
    }
    pub fn retire_stale_pr_native_diff_v1(&mut self, app: &mut App) {
        if self
            .current_pr_native_diff_v1
            .as_ref()
            .is_some_and(|(_, owner)| {
                !owner.owns(app) || (owner.work_is_bounded_v63() && owner.expired())
            })
        {
            self.cancel_pr_native_diff_v1(app);
        }
    }
    /// Some(false) means a specialized or mixed result was REJECTED before any
    /// physical/logical/loading/status/privacy effect.
    pub(super) fn poll_pr_native_diff_v1(
        &mut self,
        app: &mut App,
        completion: &mut Completion,
    ) -> Option<bool> {
        let owner = completion
            .remote_scope_owner
            .as_ref()
            .and_then(|remote| remote.pr_native_diff_v1.as_ref())
            .cloned();
        let Some(owner) = owner else {
            if completion.result.as_ref().is_ok_and(|prepared| {
                prepared
                    .scoped_pr_resource
                    .as_ref()
                    .is_some_and(native::resource_is_native)
            }) || self
                .current_pr_native_diff_v1
                .as_ref()
                .is_some_and(|(generation, _)| {
                    *generation == completion.generation && completion.view == View::Inbox
                })
            {
                return Some(false);
            }
            return None;
        };
        let remote = completion
            .remote_scope_owner
            .as_ref()
            .expect("same original owner");
        if completion.view != View::Inbox || completion.catalogue_owner.is_some() || completion.catalogue_recovery.is_some()
            || completion.local_actor_owner.is_some() || completion.remote_repository_owner.is_some()
            || completion.local_navigation_owner.is_some() || completion.original_tree_owner_v1.is_some()
            || completion.original_diff_owner_v1.is_some() || completion.original_stash_recovery_owner_v1.is_some()
            || remote.threads_v1.is_some() || remote.inline_v1.is_some() || remote.check_origin.is_some() || remote.continuation_v2.is_some()
            || self.current.get(&View::Inbox.index()).is_none_or(|(generation, token)| *generation != completion.generation || !token.same_instance(owner.cancellation()))
            || self.current_pr_native_diff_v1.as_ref().is_none_or(|(generation, current)| *generation != completion.generation || !Arc::ptr_eq(current, &owner))
            || !remote.owns(app)
            || completion.result.as_ref().is_ok_and(|prepared| !only_native(prepared) || match prepared.scoped_pr_resource.as_ref() {
                Some(crate::scoped_pr_observed_tabs_v1::resources::native::PrResourceCompletionV1::Native(result)) => !result.matches(&owner),
                _ => true,
            }) { return Some(false); }
        self.current.remove(&View::Inbox.index());
        self.current_pr_native_diff_v1 = None;
        app.pr_native_diff_owner_v1 = None;
        let result = std::mem::replace(
            &mut completion.result,
            Err("consumed exact original native PR result".into()),
        );
        if owner.expired() {
            native::failure(app, self, &owner);
        } else {
            match result {
                Ok(mut prepared) => {
                    let Some(crate::scoped_pr_observed_tabs_v1::resources::native::PrResourceCompletionV1::Native(result)) = prepared.scoped_pr_resource.take() else { unreachable!("closed native result checked"); };
                    if !native::publish(app, *result) {
                        native::failure(app, self, &owner);
                    }
                }
                Err(_) => native::failure(app, self, &owner),
            }
        }
        app.loading[View::Inbox.index()] = false;
        Some(true)
    }
}

#[cfg(test)]
pub(crate) fn generation(background: &Background) -> Option<u64> {
    background
        .current
        .get(&View::Inbox.index())
        .map(|(generation, _)| *generation)
}
/// Observe/requeue only an ACTUAL worker completion. This test-only seam
/// creates no owner/source/result and cannot grant publication authority.
#[cfg(test)]
pub(crate) fn assert_actual_physical_slot_v65(
    background: &mut Background,
    app: &mut App,
    slot: usize,
    admitted: bool,
    successful_worker: bool,
) {
    let before = background.current.clone();
    let (generation, token) = before.get(&slot).expect("Actual queued physical slot");
    let pending = app
        .pr_native_diff_work_v1
        .as_ref()
        .expect("Actual captured native input")
        .clone();
    let completion = background
        .completions
        .recv_timeout(std::time::Duration::from_secs(8))
        .expect("Actual submitted worker must complete");
    let actual_slot = if completion.remote_repository_owner.is_some() {
        6
    } else if completion.catalogue_owner.is_some() {
        5
    } else {
        completion.view.index()
    };
    assert_eq!(actual_slot, slot);
    assert_eq!(completion.generation, *generation);
    assert_eq!(completion.result.is_ok(), successful_worker);
    assert!(completion
        .remote_scope_owner
        .as_ref()
        .is_none_or(|owner| owner.pr_native_diff_v1.is_none()));
    background
        .inline_completion_sender_v1
        .send(completion)
        .expect("Same actual completion channel");
    assert_eq!(background.poll(app), admitted);
    assert_eq!(
        background.current.len(),
        before.len() - usize::from(admitted)
    );
    for (original_slot, (original_generation, original_token)) in &before {
        if admitted && *original_slot == slot {
            assert!(!background.current.contains_key(original_slot));
        } else {
            let (current_generation, current_token) = &background.current[original_slot];
            assert_eq!(current_generation, original_generation);
            assert!(current_token.same_instance(original_token));
        }
    }
    assert!(!token.is_cancelled());
    assert!(Arc::ptr_eq(
        app.pr_native_diff_work_v1.as_ref().unwrap(),
        &pending
    ));
}
#[cfg(test)]
pub(crate) fn hold_actual_completion(background: &mut Background, outcome: u8, mixed: bool) {
    let mut completion = background
        .completions
        .recv_timeout(std::time::Duration::from_secs(8))
        .expect("Actual native PR worker completion");
    assert!(completion
        .remote_scope_owner
        .as_ref()
        .is_some_and(|owner| owner.pr_native_diff_v1.is_some()));
    if outcome == 0 {
        assert!(completion.result.is_ok());
    } else if outcome == 1 {
        assert_eq!(
            completion.result.as_ref().err().map(String::as_str),
            Some("Native diff file is outside observed canonical members"),
            "The actual source-bound preparation must reject the unobserved member"
        );
    } else {
        assert!(
            completion.result.is_err(),
            "Actual worker error/caught panic required"
        );
    }
    if mixed {
        let prepared = completion
            .result
            .as_mut()
            .expect("Actual successful native source first");
        prepared.status = "MIXED_PRIVATE_STATUS_MUST_NOT_PUBLISH".into();
        prepared.clear_private = true;
    }
    background
        .inline_completion_sender_v1
        .send(completion)
        .expect("SAME actual completion channel");
}
