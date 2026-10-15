//! Additive use of the EXISTING two-worker Background queue and generation.
//! Parent Request/Completion/PreparedView hooks remain unapplied.
use super::*;
use crate::repository_tree_worker_v1::{OriginalTreeWorkV1, OriginalTreeWorkerOwnerV1};
impl Background {
    pub fn submit_original_tree_v1(
        &mut self,
        app: &mut App,
        work: Arc<OriginalTreeWorkV1>,
    ) -> bool {
        if app
            .workspace
            .as_ref()
            .is_none_or(|workspace| !work.owns(workspace, app.original_tree_jump_v1.as_ref()))
            || !crate::repository_tree_viewport_native_v2::work_current(app, &work)
        {
            return false;
        }
        let cancellation = CancellationToken::default();
        let owner = match app
            .workspace
            .as_ref()
            .ok_or_else(|| "Original tree source unavailable".to_owned())
            .and_then(|workspace| {
                OriginalTreeWorkerOwnerV1::begin(
                    workspace,
                    app.original_tree_jump_v1.as_ref(),
                    work.clone(),
                    cancellation.clone(),
                )
            }) {
            Ok(owner) => owner,
            Err(reason) => {
                crate::repository_tree_viewport_native_v2::reject(app, &work, None, &reason);
                app.status = reason;
                return false;
            }
        };
        if !work.metadata_v2() {
            app.retire_command_context();
        }
        self.cancel(View::Repository);
        self.generation += 1;
        let generation = self.generation;
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
            remote_repository_owner: None,
            local_navigation_owner: None,
            original_tree_owner_v1: Some(owner.clone()),
            original_diff_owner_v1: None,
            work: Box::new(move |_| {
                worker_owner.prepare().map(|result| PreparedView {
                    original_tree_result_v1: Some(result),
                    ..Default::default()
                })
            }),
        };
        match self
            .requests
            .as_ref()
            .expect("active worker queue")
            .try_send(request)
        {
            Ok(()) => {
                self.current_original_tree_owner_v1 = Some(owner);
                self.current_original_tree_generation_v2 = Some(generation);
                self.current
                    .insert(View::Repository.index(), (generation, cancellation));
                app.loading[View::Repository.index()] = !work.metadata_v2();
                if !work.metadata_v2() {
                    app.status = "Reading original saved tree… • Esc Cancel".into();
                }
                true
            }
            Err(failure) => {
                app.loading[View::Repository.index()] = false;
                let reason = match failure {
                    TrySendError::Full(_) => "Original tree queue full",
                    TrySendError::Disconnected(_) => "Original tree worker queue disconnected",
                };
                crate::repository_tree_viewport_native_v2::reject(app, &work, Some(&owner), reason);
                app.status = reason.into();
                false
            }
        }
    }
    pub fn cancel_original_tree_metadata_v2(&mut self, work: &Arc<OriginalTreeWorkV1>) -> bool {
        if work.metadata_v2()
            && self
                .current_original_tree_owner_v1
                .as_ref()
                .is_some_and(|owner| Arc::ptr_eq(owner.work(), work))
        {
            self.cancel_current_original_tree_v2()
        } else {
            false
        }
    }
    pub fn cancel_current_original_tree_v2(&mut self) -> bool {
        let Some(owner) = &self.current_original_tree_owner_v1 else {
            return false;
        };
        let Some(generation) = self.current_original_tree_generation_v2 else {
            return false;
        };
        if self
            .current
            .get(&View::Repository.index())
            .is_some_and(|(current, _)| *current != generation)
        {
            return false; // Another live Repository job must remain untouched.
        }
        // poll may already have consumed a completed-but-vetoed physical slot.
        // Close still owns the original logical token/loading retirement.
        owner.cancellation().cancel();
        self.cancel(View::Repository);
        true
    }
}
/// Must run before generic loading/status/private/identity/model effects.
pub(super) fn original_tree_prepared_only_v1(prepared: &PreparedView) -> bool {
    if prepared.scoped_actions_v1.is_some() {
        return false;
    }
    if prepared.scoped_threads_v1.is_some()
        || prepared.scoped_inline_v1.is_some()
        || prepared.scoped_reviews_v2.is_some()
    {
        return false;
    }
    if prepared.original_stash_recovery_v1.is_some() {
        return false;
    }
    prepared.original_tree_result_v1.is_some()
        && prepared.related_pr_v1.is_none()
        && prepared.original_diff_result_v1.is_none()
        && prepared.workspace.is_none()
        && prepared.graph_reread_retention_v3.is_none()
        && prepared.lines.is_empty()
        && prepared.context.is_none()
        && prepared.status.is_empty()
        && prepared.repository_source_identity.is_none()
        && prepared.github.is_none()
        && prepared.actions.is_none()
        && prepared.search.is_none()
        && prepared.form.is_none()
        && prepared.metrics.is_none()
        && prepared.catalogue.is_none()
        && prepared.scoped_pr_tab.is_none()
        && prepared.scoped_pr_resource.is_none()
        && prepared.verified_identity.is_none()
        && prepared.local_actor_result.is_none()
        && prepared.remote_repository.is_none()
        && prepared.remote_scope.is_none()
        && prepared.remote_scope_source.is_none()
        && prepared.pr_checks_origin.is_none()
        && prepared.pr_checks_route.is_none()
        && prepared.scoped_pr_checks.is_none()
        && prepared.pr_check_continuation_v2.is_none()
        && prepared.actions_inspection.is_none()
        && prepared.pr_inspection.is_none()
        && prepared.pr_reactions.is_none()
        && prepared.pr_reaction_display.is_none()
        && prepared.github_origin.is_none()
        && prepared.source_context.is_none()
        && prepared.source_message.is_none()
        && prepared.readonly_notice.is_none()
        && prepared.pr_pending_message.is_none()
        && !prepared.clear_private
        && !prepared.clear_metrics
        && !prepared.invalidate_metrics
        && !prepared.invalidate_pr_metadata
        && !prepared.clear_actions_inspection
        && !prepared.clear_pr_inspection
        && !prepared.restore_search_input
        && !prepared.activate_if_untouched
        && prepared.navigate_to.is_none()
        && prepared.navigation_restore.is_none()
}

/// Event-thread publisher: only moves/installs already prepared Arcs. No
/// source preparation, filesystem/SQL, matcher, body clone or provider here.
pub(super) fn publish_original_tree_v1(
    app: &mut App,
    owner: &Arc<OriginalTreeWorkerOwnerV1>,
    result: Result<PreparedView, String>,
) -> bool {
    if app.repository_load_requested
        || app.pr_native_diff_work_v1.is_some()
        || app.pr_native_diff_cancel_v1
        || app.scoped_inline_work_v1.is_some()
        || app.scoped_reviews_work_v2.is_some()
        || app.scoped_reviews_cancel_v2
        || app.scoped_inline_cancel_v1
        || app.repository_action.is_some()
        || app.original_tree_work_v1.is_some()
        || app.original_tree_cancel_v1
        || app.original_diff_work_v1.is_some()
        || app.original_diff_cancel_v1
        || app.related_pr_requested_v1
        || app.original_stash_recovery_request_v1.is_some()
        || app.original_stash_recovery_cancel_requested_v1
        || app.related_pr_cancel_v1
        || app.cancel_requested
        || (app.refresh_requested && app.view == View::Repository)
        || !crate::repository_tree_viewport_native_v2::work_current(app, owner.work())
        || app.view != View::Repository
        || app.remote_repository.is_some()
        || app.workspace.as_ref().is_none_or(|workspace| {
            !owner.can_admit(
                workspace,
                app.original_tree_jump_v1.as_ref(),
                result.is_ok(),
            )
        })
    {
        return false;
    }
    if let Ok(prepared) = &result {
        if !original_tree_prepared_only_v1(prepared)
            || prepared
                .original_tree_result_v1
                .as_ref()
                .is_none_or(|result| !result.matches(owner))
        {
            return false;
        }
    }
    // All owner/kind checks precede loading/status/principal/privacy/hits.
    let queue_member_after_success = result.is_ok() && !owner.work().metadata_v2();
    if !owner.work().metadata_v2() {
        app.retire_command_context();
    }
    app.loading[View::Repository.index()] = false;
    app.original_tree_layout_v1.clear();
    app.hits.clear();
    match result {
        Ok(mut prepared) => {
            let result = prepared
                .original_tree_result_v1
                .take()
                .expect("typed result checked");
            let workspace = app.workspace.as_mut().expect("original source checked");
            match result.publish_v2(workspace, &mut app.original_tree_jump_v1) {
                Ok(crate::repository_tree_worker_v1::TreePublishEffectV2::Navigation(focus)) => {
                    app.selected = focus.min(workspace.display_rows_v1().len().saturating_sub(1));
                    app.scroll = app.selected.saturating_sub(2);
                    app.repository_selection = None;
                    app.repository_content_focused = true;
                    app.status =
                        "Original saved tree navigation ready • read-only; checkout preserved"
                            .into();
                }
                Ok(crate::repository_tree_worker_v1::TreePublishEffectV2::Query) => {
                    if let Some(jump) = &mut app.original_tree_jump_v1 {
                        if matches!(
                            jump.focus,
                            crate::repository_tree_jump_v1::OriginalTreeFocusV1::Choose
                                | crate::repository_tree_jump_v1::OriginalTreeFocusV1::Previous
                                | crate::repository_tree_jump_v1::OriginalTreeFocusV1::Next
                        ) {
                            jump.focus = crate::repository_tree_jump_v1::OriginalTreeFocusV1::List;
                            app.focus = 1;
                        }
                    }
                    app.status =
                        "Original saved-path matches ready • bounded observed inventory".into();
                }
                Ok(crate::repository_tree_worker_v1::TreePublishEffectV2::MemberDetail(detail)) => {
                    app.original_tree_viewport_v2.cache.admit(
                        detail,
                        app.original_tree_jump_v1
                            .as_ref()
                            .expect("current member checked"),
                    );
                    app.original_tree_viewport_v2.pending = None;
                }
                Ok(crate::repository_tree_worker_v1::TreePublishEffectV2::FolderDetail(detail)) => {
                    app.original_tree_viewport_v2.folder = Some(detail);
                    app.original_tree_viewport_v2.pending = None;
                }
                Err(reason) => app.status = format!("Error: {reason}"),
            }
        }
        Err(reason) => {
            crate::repository_tree_viewport_native_v2::reject(
                app,
                owner.work(),
                Some(owner),
                &reason,
            );
            if let Some(jump) = app.original_tree_jump_v1.as_mut() {
                owner.failed(jump, &reason);
            }
            app.status = if reason.len() <= 4096 {
                format!(
                    "Error: {}",
                    crate::providers::runner::sanitize_terminal(&reason)
                )
            } else {
                "Error: Original tree read unavailable; oversized diagnostic omitted".into()
            };
        }
    }
    if queue_member_after_success {
        crate::repository_tree_viewport_native_v2::queue_member(app);
    }
    true
}

#[cfg(test)]
#[path = "original_tree_before_poll_v2_tests.rs"]
mod before_poll_v2_tests;
