//! NEW child of Graph UI. Tiny position/range guards, no topology/body scan.
use super::*;
use crate::repository_graph_v1::projection_v2::GraphWindowActionV2;

pub(crate) fn window_key_v2(workspace: &Workspace, key: KeyCode) -> Option<GraphActionV1> {
    if !workspace.graph_visible_v1() || workspace.graph_view_v1 != GraphViewV1::Timeline {
        return None;
    }
    let action = match key {
        KeyCode::Char('<') => GraphWindowActionV2::Newer,
        KeyCode::Char('>') => GraphWindowActionV2::Older,
        KeyCode::Home => GraphWindowActionV2::First,
        KeyCode::End => GraphWindowActionV2::Last,
        KeyCode::Char('x') => return Some(GraphActionV1::DiscardBack),
        _ => return None,
    };
    Some(GraphActionV1::Window(action))
}
/// Existing row scroll remains within a projection. Pg keys/arrows/wheel only
/// cross at a genuine currently painted boundary; no click-through overlays.
pub(crate) fn boundary_scroll_v2(app: &crate::app::App, delta: i32) -> Option<GraphActionV1> {
    if app.view != crate::app::View::Repository
        || app.remote_repository.is_some()
        || app.overlay.is_some()
        || app.loading[crate::app::View::Repository.index()]
        || app.repository_visible_rows == 0
    {
        return None;
    }
    let workspace = app.workspace.as_ref()?;
    if !workspace.graph_visible_v1() || workspace.graph_view_v1 != GraphViewV1::Timeline {
        return None;
    }
    let end = workspace
        .display_rows_v1()
        .len()
        .saturating_sub(app.repository_visible_rows);
    let direction = if delta > 0 && app.repository_list.top >= end {
        GraphWindowActionV2::Older
    } else if delta < 0 && app.repository_list.top == 0 {
        GraphWindowActionV2::Newer
    } else {
        return None;
    };
    let action = GraphActionV1::Window(direction);
    // At outermost loaded boundary ordinary harmless scroll semantics remain;
    // explicit visible disabled buttons still explain the same closed reason.
    workspace
        .graph_action_reason_v1(&action)
        .is_none()
        .then_some(action)
}
