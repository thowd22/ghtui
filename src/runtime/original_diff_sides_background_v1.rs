//! Specialized publication on the EXISTING Background and exact queued owner.
//! No separate transport/queue; stale owners cannot retire a newer shared slot.
use super::*;
use crate::repository_diff_sides_worker_v1::{OriginalDiffWorkV1, OriginalDiffWorkerOwnerV1};
impl Background {
    pub fn submit_original_diff_sides_v1(
        &mut self,
        app: &mut App,
        work: Arc<OriginalDiffWorkV1>,
    ) -> bool {
        if let OriginalDiffWorkV1::Native(request) = work.as_ref() {
            if app.native_diff_v1.as_ref().is_none_or(|state| {
                !state.accepts(request)
                    || app
                        .workspace
                        .as_ref()
                        .is_none_or(|workspace| !state.owns(workspace))
            }) {
                return false;
            }
        }
        if let OriginalDiffWorkV1::MutableNative(request) = work.as_ref() {
            if app.native_mutable_diff_v2.as_ref().is_none_or(|state| {
                !state.accepts(request)
                    || app
                        .workspace
                        .as_ref()
                        .is_none_or(|workspace| !state.owns(workspace))
            }) {
                return false;
            }
        }
        if app
            .workspace
            .as_ref()
            .is_none_or(|workspace| !work.owns(workspace, app.original_diff_picker_v1.as_ref()))
        {
            return false;
        }
        let retained_work = work.clone();
        let cancellation = CancellationToken::default();
        let owner = match app
            .workspace
            .as_ref()
            .ok_or_else(|| "Original comparison source unavailable".to_owned())
            .and_then(|workspace| {
                OriginalDiffWorkerOwnerV1::begin(
                    workspace,
                    app.original_diff_picker_v1.as_ref(),
                    work,
                    cancellation.clone(),
                )
            }) {
            Ok(owner) => owner,
            Err(reason) => {
                if let (OriginalDiffWorkV1::Query(request), Some(picker)) =
                    (retained_work.as_ref(), app.original_diff_picker_v1.as_mut())
                {
                    picker.fail(request, &reason);
                }
                app.status = reason;
                return false;
            }
        };
        app.retire_command_context();
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
            original_tree_owner_v1: None,
            original_diff_owner_v1: Some(owner.clone()),
            work: Box::new(move |_| {
                worker_owner.prepare().map(|result| PreparedView {
                    original_diff_result_v1: Some(result),
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
                self.current_original_diff_owner_v1 = Some(owner.clone());
                self.current_original_diff_generation_v1 = Some(generation);
                self.current
                    .insert(View::Repository.index(), (generation, cancellation));
                app.loading[View::Repository.index()] = true;
                app.status = "Reading original comparison side… • Esc Cancel".into();
                true
            }
            Err(failure) => {
                let reason = match failure {
                    TrySendError::Full(_) => {
                        "Original comparison queue full; close and reopen explicitly"
                    }
                    TrySendError::Disconnected(_) => {
                        "Original comparison worker queue disconnected; close explicitly"
                    }
                };
                if let Some(picker) = app.original_diff_picker_v1.as_mut() {
                    owner.failed(picker, reason);
                }
                app.loading[View::Repository.index()] = false;
                app.status = reason.into();
                false
            }
        }
    }
    pub fn cancel_current_original_diff_v1(&mut self) -> bool {
        let (Some(owner), Some(generation)) = (
            &self.current_original_diff_owner_v1,
            self.current_original_diff_generation_v1,
        ) else {
            return false;
        };
        if self
            .current
            .get(&View::Repository.index())
            .is_some_and(|(actual, _)| *actual != generation)
        {
            return false;
        }
        // Poll can already have consumed a completed slot before newer Close
        // vetoed publication. The exact logical owner still owns its loading
        // state, but must not cancel a newer unrelated repository generation.
        owner.cancellation().cancel();
        self.current_original_diff_owner_v1 = None;
        self.current_original_diff_generation_v1 = None;
        if self.current.contains_key(&View::Repository.index()) {
            self.cancel(View::Repository);
        }
        true
    }
}

#[cfg(test)]
#[path = "original_diff_before_poll_v1_tests.rs"]
mod before_poll_v1_tests;
#[cfg(test)]
#[path = "native_diff_before_poll_v1_tests.rs"]
mod native_diff_before_poll_v1_tests;
#[cfg(test)]
#[path = "native_mutable_diff_before_poll_v2_tests.rs"]
mod native_mutable_diff_before_poll_v2_tests;
pub(super) fn original_diff_prepared_only_v1(prepared: &PreparedView) -> bool {
    prepared.original_diff_result_v1.is_some()
        && prepared.original_stash_recovery_v1.is_none()
        && prepared.related_pr_v1.is_none()
        && prepared.original_tree_result_v1.is_none()
        && prepared.graph_reread_retention_v3.is_none()
        && prepared.pr_check_continuation_v2.is_none()
        && prepared.workspace.is_none()
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
pub(super) fn publish_original_diff_sides_v1(
    app: &mut App,
    owner: &Arc<OriginalDiffWorkerOwnerV1>,
    result: Result<PreparedView, String>,
) -> bool {
    if app.repository_load_requested
        || app.related_pr_requested_v1
        || app.original_stash_recovery_request_v1.is_some()
        || app.original_stash_recovery_cancel_requested_v1
        || app.related_pr_cancel_v1
        || app.related_pr_v1.is_some()
        || app.repository_action.is_some()
        || app.original_diff_work_v1.is_some()
        || app.original_diff_cancel_v1
        || app.original_tree_work_v1.is_some()
        || app.original_tree_cancel_v1
        || app.graph_reread_request_v3.is_some()
        || app.navigation_restore.is_some()
        || app.navigation_restore_inflight
        || app.cancel_requested
        || (app.refresh_requested && app.view == View::Repository)
        || app.view != View::Repository
        || app.remote_repository.is_some()
        || app.workspace.as_ref().is_none_or(|workspace| {
            !owner.can_admit(
                workspace,
                app.original_diff_picker_v1.as_ref(),
                result.is_ok(),
            )
        })
    {
        return false;
    }
    if let Ok(prepared) = &result {
        if !original_diff_prepared_only_v1(prepared)
            || prepared
                .original_diff_result_v1
                .as_ref()
                .is_none_or(|result| !result.matches(owner))
        {
            return false;
        }
    }
    if let OriginalDiffWorkV1::Native(request) = owner.work().as_ref() {
        if app.native_mutable_diff_v2.is_some()
            || app.native_diff_v1.as_ref().is_none_or(|state| {
                !state.accepts(request)
                    || app
                        .workspace
                        .as_ref()
                        .is_none_or(|workspace| !state.owns(workspace))
            })
        {
            return false;
        }
        // Exact closed Native work is a distinct result branch under the SAME
        // B owner/token/ledger. No legacy picker/Workspace publication occurs.
        let status = match result {
            Ok(mut prepared) => {
                let result = prepared
                    .original_diff_result_v1
                    .take()
                    .expect("typed result checked");
                let source_identity = result.source_identity_v1().clone();
                let workspace = app
                    .workspace
                    .as_ref()
                    .expect("original native source checked");
                let state = app
                    .native_diff_v1
                    .as_mut()
                    .expect("exact native request checked");
                if result.publish_native_v1(workspace, state).is_err() {
                    return false;
                }
                app.repository_source_identity = Some(source_identity);
                state.status.clone()
            }
            Err(reason) => {
                let state = app
                    .native_diff_v1
                    .as_mut()
                    .expect("exact native request checked");
                if !state.fail(request, &reason) {
                    return false;
                }
                state.status.clone()
            }
        };
        app.retire_command_context();
        app.loading[View::Repository.index()] = false;
        app.native_diff_layout_v1.clear();
        app.hits.clear();
        app.status = status;
        return true;
    }
    if let OriginalDiffWorkV1::MutableNative(request) = owner.work().as_ref() {
        if app.native_diff_v1.is_some()
            || app.native_mutable_diff_v2.as_ref().is_none_or(|state| {
                !state.accepts(request)
                    || app
                        .workspace
                        .as_ref()
                        .is_none_or(|workspace| !state.owns(workspace))
            })
        {
            return false;
        }
        // Exact closed Native work is a distinct result branch under the SAME
        // B owner/token/ledger. No legacy picker/Workspace publication occurs.
        let status = match result {
            Ok(mut prepared) => {
                let result = prepared
                    .original_diff_result_v1
                    .take()
                    .expect("typed result checked");
                let source_identity = result.source_identity_v1().clone();
                let workspace = app
                    .workspace
                    .as_ref()
                    .expect("original native source checked");
                let state = app
                    .native_mutable_diff_v2
                    .as_mut()
                    .expect("exact native request checked");
                if result.publish_native_mutable_v2(workspace, state).is_err() {
                    return false;
                }
                app.repository_source_identity = Some(source_identity);
                state.status.clone()
            }
            Err(reason) => {
                let state = app
                    .native_mutable_diff_v2
                    .as_mut()
                    .expect("exact native request checked");
                if !state.fail(request, &reason) {
                    return false;
                }
                state.status.clone()
            }
        };
        app.retire_command_context();
        app.loading[View::Repository.index()] = false;
        app.native_mutable_diff_layout_v2.clear();
        app.hits.clear();
        app.status = status;
        return true;
    }
    if app.native_diff_v1.is_some() || app.native_mutable_diff_v2.is_some() {
        return false; // A newly capturing Native purpose supersedes the picker.
    }
    // No global/private/loading/hit effect precedes exact owner/result proof.
    app.retire_command_context();
    app.loading[View::Repository.index()] = false;
    app.original_diff_layout_v1.clear();
    app.hits.clear();
    match result {
        Ok(mut prepared) => {
            let result = prepared
                .original_diff_result_v1
                .take()
                .expect("typed result checked");
            let workspace = app.workspace.as_mut().expect("original source checked");
            let source_identity = result.source_identity_v1().clone();
            match result.publish(workspace, &mut app.original_diff_picker_v1) {
                Ok(Some(focus)) => {
                    app.repository_source_identity = Some(source_identity);
                    app.selected = focus.min(workspace.display_rows_v1().len().saturating_sub(1));
                    app.scroll = app.selected.saturating_sub(2);
                    app.repository_selection = None;
                    app.repository_content_focused = true;
                    app.status = if workspace.original_diff_side_v1.as_ref().is_some_and(|side|side.visible_for(workspace)) {"Original comparison side ready • read-only; checkout preserved"} else {"Original saved comparison restored • actual Old/New content; checkout preserved"}.into();
                }
                Ok(None) => {
                    app.repository_source_identity = Some(source_identity);
                    app.status =
                        "Original changed-file matches ready • bounded canonical members".into()
                }
                Err(reason) => app.status = format!("Error: {reason}"),
            }
        }
        Err(reason) => {
            if let Some(picker) = app.original_diff_picker_v1.as_mut() {
                owner.failed(picker, &reason);
            }
            app.status = if reason.len() <= 4096 {
                format!(
                    "Error: {}",
                    crate::providers::runner::sanitize_terminal(&reason)
                )
            } else {
                "Error: Original comparison unavailable; diagnostic omitted".into()
            };
        }
    }
    true
}
