//! Source-only Background lane using ONE actual token and pre-queue clock.
//! Request/Completion field/accept-before-effect hooks remain unapplied.
use super::*;
use crate::navigation_local_owner_v1::OriginalLocalNavigationOwnerV1;

#[cfg(test)]
#[path = "original_local_queued_intent_v1_tests.rs"]
mod queued_intent_tests;

impl Background {
    pub fn submit_original_local_v1(
        &mut self,
        app: &mut App,
        visit: Option<Arc<crate::navigation_local_source_v1::OriginalSavedVisitV1>>,
        work: impl FnOnce(Arc<OriginalLocalNavigationOwnerV1>) -> Result<PreparedView, String>
            + Send
            + 'static,
    ) -> bool {
        let cancellation = CancellationToken::default();
        let owner = match app
            .workspace
            .as_ref()
            .ok_or_else(|| "Original local source is unavailable".to_owned())
            .and_then(|workspace| match visit.as_ref() {
                Some(visit) => OriginalLocalNavigationOwnerV1::begin_visit(
                    workspace,
                    visit,
                    cancellation.clone(),
                ),
                None => OriginalLocalNavigationOwnerV1::begin(workspace, cancellation.clone()),
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
            local_navigation_owner: Some(owner.clone()),
            original_tree_owner_v1: None,
            work: Box::new(move |_same_request_token| {
                worker_owner.operation().check()?;
                let result = work(worker_owner.clone());
                // Seal on both paths before any prepared model is returned.
                // Cancellation/timeout may make seal fail; no late success.
                let sealed = worker_owner.operation().seal();
                match (result, sealed) {
                    (Ok(prepared), Ok(_)) if original_local_prepared_only_v1(&prepared) => {
                        Ok(prepared)
                    }
                    (Ok(_), Ok(_)) => Err(
                        "Original local worker returned a foreign prepared kind; nothing admitted"
                            .into(),
                    ),
                    (Err(reason), _) => Err(reason),
                    (Ok(_), Err(reason)) => Err(reason),
                }
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
                self.current_original_local_owner = Some(owner);
                app.loading[View::Repository.index()] = true;
                if app.view == View::Repository {
                    app.status = "Reading original local source… • Esc Cancel".into();
                }
                true
            }
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                app.loading[View::Repository.index()] = false;
                app.status =
                    "Background queue is full • explicit Refresh to retry original source".into();
                false
            }
        }
    }
}

pub(super) fn original_local_prepared_only_v1(prepared: &PreparedView) -> bool {
    prepared.workspace.is_some()
        && prepared
            .graph_reread_retention_v3
            .as_ref()
            .is_none_or(|retention| {
                prepared
                    .workspace
                    .as_ref()
                    .is_some_and(|workspace| retention.matches_workspace(workspace))
            })
        && (prepared.graph_reread_retention_v3.is_none() || prepared.navigation_restore.is_none())
        && prepared.original_tree_result_v1.is_none()
        && prepared.lines.is_empty()
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
}

#[cfg(test)]
#[path = "local_saved_navigation_v1_tests.rs"]
mod tests;
