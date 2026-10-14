//! NEW private child of navigation_state. Bounds-only retention preflight;
//! cannot authenticate a repository or read Git/SQL in event handlers.
use super::*;
use crate::repository_graph_v1::{restore_v2::GraphPositionLeaseV2, GraphActionV1};
use std::sync::Arc;

pub(crate) fn retain_graph_before_leave_v2(app: &mut App) -> Result<(), String> {
    if app.view != View::Repository || app.remote_repository.is_some() {
        return Ok(());
    }
    let Some(workspace) = app
        .workspace
        .as_ref()
        .filter(|w| w.graph_retention_visible_v2())
    else {
        return Ok(());
    };
    let visit = app.original_saved_visits.capture_graph_v2(workspace)?;
    app.original_saved_visits.remember_checked_v2(visit)
}
/// Called BEFORE before_action/intent clearing. Discard is deliberate and pure;
/// cap refusal preserves actual current Workspace/forms/local working state.
pub(crate) fn handle_retention_action_v2(app: &mut App, action: &Action) -> bool {
    if matches!(
        action,
        Action::RepositoryRow(RowAction::Graph(GraphActionV1::DiscardBack))
    ) {
        if app.view != View::Repository
            || app.remote_repository.is_some()
            || app
                .workspace
                .as_ref()
                .is_none_or(|workspace| !workspace.graph_visible_v1())
            || app.loading[View::Repository.index()]
            || app.navigation_restore_inflight
        {
            app.dispatch(Action::Explain("Discard graph Back belongs to the current idle local Graph; cancel the original read first".into()));
            return true;
        }
        app.abandon_navigation_restore();
        // This action refuses during an owned pending read. Any independently
        // retained pending lease stays charged until its actual last Drop.
        app.original_saved_visits.discard_graph_back_v2();
        app.status = "Earlier Graph Back snapshots discarded • current graph remains loaded".into();
        if let Ok(Some(accounting)) = app.original_saved_visits.graph_retention_accounting_v2() {
            app.status.push_str(&format!(
                " • Back {} /16MiB bytes, {} pending positions",
                accounting.bytes, accounting.positions
            ));
        }
        app.hits.clear();
        return true;
    }
    if app.overlay.is_some()
        || matches!(
            action,
            Action::Quit | Action::Cancel | Action::Close | Action::Explain(_)
        )
        || app.view != View::Repository
    {
        return false;
    }
    if let Err(reason) = retain_graph_before_leave_v2(app) {
        app.dispatch(Action::Explain(reason));
        return true;
    }
    false
}
pub(crate) fn graph_requested_position_v2(
    visit: &crate::navigation_local_source_v1::OriginalSavedVisitV1,
    bookmark: &NavigationBookmark,
) -> Result<Option<Arc<GraphPositionLeaseV2>>, String> {
    match (
        visit.graph_lease_v2(),
        bookmark.position().graph_v2.as_ref(),
    ) {
        (Some(_), Some(position)) => visit.graph_request_position_v2(position).map(Some),
        (None, None) => Ok(None),
        _ => Err("Original Graph snapshot unavailable • no body/source/HEAD fallback".into()),
    }
}
