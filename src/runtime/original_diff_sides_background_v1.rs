//! Specialized publication on the EXISTING Background and exact queued owner.
//! UNAPPLIED registration/match hooks; no separate transport/queue.
use super::*;
use crate::repository_diff_sides_worker_v1::{OriginalDiffWorkV1, OriginalDiffWorkerOwnerV1};
impl Background {
    pub fn submit_original_diff_sides_v1(
        &mut self,
        app: &mut App,
        work: Arc<OriginalDiffWorkV1>,
    ) -> bool {
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
            original_diff_owner_v1: Some(owner),
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
                self.current
                    .insert(View::Repository.index(), (generation, cancellation));
                app.loading[View::Repository.index()] = true;
                app.status = "Reading original comparison side… • Esc Cancel".into();
                true
            }
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                app.loading[View::Repository.index()] = false;
                app.status = "Original comparison queue full • close/retry explicitly".into();
                false
            }
        }
    }
}
pub(super) fn original_diff_prepared_only_v1(prepared: &PreparedView) -> bool {
    prepared.original_diff_result_v1.is_some()
        && prepared.original_tree_result_v1.is_none()
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
    if app.view != View::Repository
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
            match result.publish(workspace, &mut app.original_diff_picker_v1) {
                Ok(Some(focus)) => {
                    app.selected = focus.min(workspace.display_rows_v1().len().saturating_sub(1));
                    app.scroll = app.selected.saturating_sub(2);
                    app.repository_selection = None;
                    app.repository_content_focused = true;
                    app.status =
                        "Original comparison side ready • read-only; checkout preserved".into();
                }
                Ok(None) => {
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
