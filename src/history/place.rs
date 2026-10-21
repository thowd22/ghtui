//! The reader's place in History across a continuation page (TASK-109).
//!
//! Scrolling past the last loaded commit reads the next page. The table that comes back
//! keeps the row the person scrolled to: the commit under the `▌` bar and the commit at the
//! top of the viewport are found again by SHA in the new window, so neither PageDown/Down
//! nor the wheel lands back on the newest commit.

use crate::{
    app::App,
    workspace::{RowAction, Workspace},
};

/// Where the History table stood when its next page was asked for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HistoryPlaceV109 {
    /// The commit under the bar.
    pub bar: Option<String>,
    /// The first commit at or below the viewport's top, and how many rows below the top it
    /// sat (a non-commit row such as the window summary can be the top row).
    pub top: Option<(String, usize)>,
    /// Commits loaded before the page read; only a larger walk adopts this place.
    pub loaded: usize,
}

fn commit_at(workspace: &Workspace, index: usize) -> Option<String> {
    match workspace.display_rows_v1().get(index)?.action.as_ref()? {
        RowAction::Commit(id) => Some(id.clone()),
        _ => None,
    }
}

/// The History table's place in `app` now, when History's commit table is shown.
pub fn capture(app: &App) -> Option<HistoryPlaceV109> {
    let workspace = app.workspace.as_ref()?;
    if !workspace.graph_visible_v1() {
        return None;
    }
    let loaded = workspace.graph_state_v1.as_ref()?.loaded();
    let top = app.repository_list.top;
    let rows = workspace.display_rows_v1().len();
    let top = (top..rows).find_map(|index| Some((commit_at(workspace, index)?, index - top)));
    Some(HistoryPlaceV109 {
        bar: commit_at(workspace, app.repository_list.selected),
        top,
        loaded,
    })
}

/// Puts the bar and the viewport back on the commits `place` names, in the newly installed
/// `workspace`. A commit the new window does not show leaves that half as it is.
pub fn restore(app: &mut App, workspace: &Workspace, place: &HistoryPlaceV109) -> bool {
    if !workspace.graph_visible_v1()
        || workspace
            .graph_state_v1
            .as_ref()
            .is_none_or(|state| state.loaded() <= place.loaded)
    {
        return false;
    }
    let row = |id: &String| workspace.graph_action_index_v1(&RowAction::Commit(id.clone()));
    let mut kept = false;
    if let Some(index) = place.bar.as_ref().and_then(row) {
        app.repository_list.selected = index;
        kept = true;
    }
    if let Some(index) = place
        .top
        .as_ref()
        .and_then(|(id, offset)| row(id)?.checked_sub(*offset))
    {
        app.repository_list.top = index;
        kept = true;
    }
    kept
}

#[cfg(test)]
#[path = "place_tests.rs"]
mod tests;
