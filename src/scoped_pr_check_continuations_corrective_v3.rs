//! SOURCE-ONLY child of the frozen continuation V2 module. The existing
//! Background generation/owner gate must call this before ANY generic effect.
//! No provider, SQL, local Graph/Tree preparation, body copy or second queue.
use super::*;

/// Closed prepared-kind whitelist. A continuation cannot smuggle a local
/// retained visit, another remote model, a form, a principal or generic flags.
/// Conversely the local Tree/Graph publishers must reject this result facet.
pub(crate) fn prepared_only(value: &PreparedView) -> bool {
    if value.original_stash_recovery_v1.is_some() {
        return false;
    }
    value.pr_check_continuation_v2.is_some()
        && value.scoped_inline_v1.is_none()
        && value.related_pr_v1.is_none()
        && value.original_tree_result_v1.is_none()
        && value.original_diff_result_v1.is_none()
        && value.graph_reread_retention_v3.is_none()
        && value.workspace.is_none()
        && value.repository_source_identity.is_none()
        && value.remote_repository.is_none()
        && value.lines.is_empty()
        && value.context.is_none()
        && value.status.is_empty()
        && value.github.is_none()
        && value.scoped_pr_tab.is_none()
        && value.scoped_pr_resource.is_none()
        && value.form.is_none()
        && value.search.is_none()
        && value.navigate_to.is_none()
        && !value.restore_search_input
        && value.actions.is_none()
        && value.pr_checks_origin.is_none()
        && value.pr_checks_route.is_none()
        && value.scoped_pr_checks.is_none()
        && value.actions_inspection.is_none()
        && !value.clear_actions_inspection
        && !value.clear_private
        && !value.activate_if_untouched
        && value.pr_inspection.is_none()
        && !value.clear_pr_inspection
        && value.pr_pending_message.is_none()
        && value.verified_identity.is_none()
        && value.pr_reactions.is_none()
        && value.pr_reaction_display.is_none()
        && value.readonly_notice.is_none()
        && value.source_message.is_none()
        && value.source_context.is_none()
        && value.metrics.is_none()
        && !value.clear_metrics
        && value.github_origin.is_none()
        && !value.invalidate_metrics
        && !value.invalidate_pr_metadata
        && value.navigation_restore.is_none()
        && value.catalogue.is_none()
        && value.local_actor_result.is_none()
        && value.remote_scope.is_none()
        && value.remote_scope_source.is_none()
}

fn outcome_matches(owner: &PrCheckContinuationOwnerV2, outcome: &OutcomeV2, app: &App) -> bool {
    match outcome {
        OutcomeV2::Remote(value) => {
            // The SAME opaque worker outcome owns its original resource. The
            // existing installer still validates the closed canonical rebind.
            // No private page is moved/cloned by a continuation completion.
            Arc::ptr_eq(&value.expected_origin, &owner.route.origin) && value.page.is_none()
        }
        OutcomeV2::Search(matches) => {
            owner
                .capture
                .as_ref()
                .is_some_and(|capture| matches.belongs_to(capture))
                && app
                    .actions
                    .as_ref()
                    .is_some_and(|workspace| loaded_capture_allowed(&owner.route, workspace))
        }
        OutcomeV2::SearchUnavailable => {
            owner.capture.is_some()
                && app
                    .actions
                    .as_ref()
                    .is_some_and(|workspace| loaded_capture_allowed(&owner.route, workspace))
        }
        OutcomeV2::Advanced(value) => Arc::ptr_eq(value.original(), &owner.route.observation),
        OutcomeV2::Hidden => true,
    }
}

/// A current failed proof must hide private frames NOW, not after the next
/// event-loop gate or a synthetic Close. The error/caught-panic payload is
/// never inspected, echoed, formatted or installed. Original SQL is untouched.
pub(super) fn hide_current(app: &mut App) {
    retire(app);
    crate::repository_catalogue_native::privacy(app);
    crate::repository_catalogue_native::retire_remote_views(app);
    if matches!(
        app.overlay,
        Some(
            crate::app::Overlay::PrCheckContinuationV2
                | crate::app::Overlay::PrCheckChoices
                | crate::app::Overlay::Notice
                | crate::app::Overlay::Form
                | crate::app::Overlay::PrActions
                | crate::app::Overlay::PrReactions
        )
    ) {
        app.overlay = None;
    }
    app.notice.clear();
    app.notice_scroll = 0;
    app.selected = 0;
    app.scroll = 0;
    app.context = "Original scoped read unavailable • private results hidden".into();
    app.refresh_requested = false;
    app.status = "Original scoped read unavailable • private results hidden; original intent/drafts retained".into();
}

/// The Background caller verifies its original generation and all Request
/// owner facets first. This independently verifies the SAME live ticket and
/// closed result before loading/status/context/private effects. Mixed and
/// ownerless results are simply dropped, including actual tentative leases.
pub fn publish(
    app: &mut App,
    owner: &Arc<PrCheckContinuationOwnerV2>,
    result: Result<PreparedView, String>,
) -> bool {
    if !owner.owns(app) {
        return false;
    }
    let mut prepared = match result {
        Err(_) => {
            hide_current(app);
            return true;
        }
        Ok(prepared) => prepared,
    };
    if !prepared_only(&prepared)
        || prepared
            .pr_check_continuation_v2
            .as_ref()
            .is_none_or(|value| {
                !Arc::ptr_eq(&value.owner, owner) || !outcome_matches(owner, &value.outcome, app)
            })
    {
        return false;
    }
    let value = *prepared
        .pr_check_continuation_v2
        .take()
        .expect("closed result checked");
    if matches!(&value.outcome, OutcomeV2::Hidden) {
        hide_current(app);
        return true;
    }
    app.retire_command_context();
    app.loading[View::Actions.index()] = false;
    // Existing install is still the sole prepared-Arc mover. An unexpected
    // current canonical mismatch cannot retain a partly modified private view.
    if install(app, value).is_err() {
        hide_current(app);
    }
    true
}

pub fn take_work(app: &mut App) -> Option<PrCheckContinuationWorkV2> {
    take_work_at(app, Instant::now())
}
/// Clear only the current pure-search ticket. A queued/running response cannot
/// put cleared highlights back; unrelated watch/remote/local owners survive.
pub(super) fn retire_pending_search(app: &mut App) {
    let Some(owner) = app.pr_check_continuation_owner_v2.as_ref() else {
        return;
    };
    if owner.capture.is_none() || !owner.owns(app) {
        return;
    }
    if app
        .pr_check_continuation_request_v2
        .as_ref()
        .is_some_and(|work| {
            Arc::ptr_eq(&work.owner, owner) && matches!(work.kind, WorkKindV2::Search { .. })
        })
    {
        app.pr_check_continuation_request_v2 = None;
    }
    app.pr_check_continuation_owner_v2 = None;
    app.pr_check_continuation_cancel_v2 = true;
    app.loading[View::Actions.index()] = false;
}
fn take_work_at(app: &mut App, now: Instant) -> Option<PrCheckContinuationWorkV2> {
    let work = app.pr_check_continuation_request_v2.take()?;
    if !work.owner.owns(app) {
        return None; // Never modify a newer owner or its private/local frame.
    }
    if now >= work.owner.deadline {
        stop(app);
        app.loading[View::Actions.index()] = false;
        app.status =
            "Original queued scoped read expired • previous snapshot retained; refresh explicitly"
                .into();
        app.hits.clear();
        return None;
    }
    Some(work)
}
#[cfg(test)]
pub(crate) fn take_work_at_for_test(
    app: &mut App,
    now: Instant,
) -> Option<PrCheckContinuationWorkV2> {
    take_work_at(app, now)
}
#[cfg(test)]
pub(crate) fn stop_current_quota_for_test(app: &App) {
    // Exercise an actual already-issued finite quota becoming unavailable
    // while its SAME ticket is live; no fake receipt/claim/allowance constructor.
    app.pr_check_watch_v2.as_ref().unwrap().quota.stop();
}

/// Called before the generic Refresh refusal/side effects. Footer/palette
/// Refresh is exactly the same admitted action as Actions keyboard `r`/mouse.
/// None/legacy routes still use their existing disabled explanation.
pub fn forward_global_refresh(app: &mut App) -> bool {
    if app.view != View::Actions || !app.pr_checks_route.as_ref().is_some_and(|r| r.scoped_v1()) {
        return false;
    }
    app.dispatch(Action::Actions(ActionsAction::Refresh));
    true
}
