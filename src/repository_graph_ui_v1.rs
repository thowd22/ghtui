//! Bounded prepared Graph spans and shared action mapping, no graph scan/I/O.
use crate::{
    app::Action,
    footer::{ControlGroup, KeyControl},
    repository_graph_v1::{GraphActionV1, GraphViewV1},
    theme::Theme,
    workspace::{RowAction, Workspace},
};
use crossterm::event::KeyCode;
#[path = "repository_graph_navigation_v2.rs"]
pub(crate) mod navigation_v2;
use ratatui::text::{Line, Span};
use unicode_segmentation::UnicodeSegmentation;

pub fn key(workspace: &Workspace, key: KeyCode) -> Option<GraphActionV1> {
    if let Some(action) = navigation_v2::window_key_v2(workspace, key) {
        return Some(action);
    }
    if workspace.graph_state_v1.is_none()
        || workspace.actor_profile.is_some()
        || workspace
            .file_provenance
            .as_ref()
            .is_some_and(|state| state.view != crate::workspace::ProvenanceView::Graph)
        || (!workspace.graph_visible_v1() && workspace.graph_view_v1 != GraphViewV1::Comparison)
    {
        return None;
    }
    match key {
        // `n`/`N` step through History matches (TASK-99.3); More loads with `L` or by
        // moving past the last loaded commit.
        KeyCode::Char('L') => Some(GraphActionV1::More),
        KeyCode::Char('P') => Some(GraphActionV1::Parents),
        KeyCode::Char('D') => Some(GraphActionV1::Details),
        KeyCode::Char('l') => Some(GraphActionV1::Legend),
        KeyCode::Char('u') if workspace.graph_view_v1 != GraphViewV1::Timeline => {
            Some(GraphActionV1::Back)
        }
        KeyCode::Left => Some(GraphActionV1::Horizontal(-8)),
        KeyCode::Right => Some(GraphActionV1::Horizontal(8)),
        _ => None,
    }
}
/// Why every Graph key waits while the original graph is read.
pub const BUSY: &str = "Wait for the original graph read, or cancel it first";

/// The Graph key-bar controls in display order. They exist only while a graph projection is
/// visible; an unavailable action keeps its key and explains itself instead of being painted.
pub fn controls(workspace: &Workspace, busy: bool) -> Vec<KeyControl> {
    use crate::repository_graph_v1::GraphWindowActionV2 as W;
    if workspace.graph_state_v1.is_none()
        || workspace.actor_profile.is_some()
        || workspace
            .file_provenance
            .as_ref()
            .is_some_and(|state| state.view != crate::workspace::ProvenanceView::Graph)
        || (!workspace.graph_visible_v1() && workspace.graph_view_v1 != GraphViewV1::Comparison)
    {
        return Vec::new();
    }
    let controls: Vec<(&str, &str, Option<&str>, GraphActionV1, u8)> =
        if workspace.graph_view_v1 == GraphViewV1::Timeline {
            vec![
                ("L", "Load more", Some("More"), GraphActionV1::More, 14),
                ("D", "Details", None, GraphActionV1::Details, 15),
                ("P", "Parents", None, GraphActionV1::Parents, 16),
                (
                    "<",
                    "Newer loaded",
                    Some("Newer"),
                    GraphActionV1::Window(W::Newer),
                    18,
                ),
                (
                    ">",
                    "Older loaded",
                    Some("Older"),
                    GraphActionV1::Window(W::Older),
                    19,
                ),
                (
                    "Home",
                    "First loaded",
                    Some("First"),
                    GraphActionV1::Window(W::First),
                    20,
                ),
                (
                    "End",
                    "Last loaded",
                    Some("Last"),
                    GraphActionV1::Window(W::Last),
                    21,
                ),
                (
                    "x",
                    "Discard graph Back",
                    Some("Discard"),
                    GraphActionV1::DiscardBack,
                    30,
                ),
                ("l", "Legend", None, GraphActionV1::Legend, 34),
                ("←", "Pan left", None, GraphActionV1::Horizontal(-8), 45),
                ("→", "Pan right", None, GraphActionV1::Horizontal(8), 46),
            ]
        } else {
            vec![
                ("u", "Graph back", Some("Back"), GraphActionV1::Back, 5),
                ("←", "Pan left", None, GraphActionV1::Horizontal(-8), 45),
                ("→", "Pan right", None, GraphActionV1::Horizontal(8), 46),
            ]
        };
    let busy = busy.then(|| BUSY.to_string());
    let mut prepared: Vec<_> = controls
        .into_iter()
        .map(|(key, label, compact, action, priority)| {
            let reason = busy
                .clone()
                .or_else(|| workspace.graph_action_reason_v1(&action));
            let group = if matches!(action, GraphActionV1::Back) {
                ControlGroup::Navigate
            } else {
                ControlGroup::Act
            };
            let control =
                KeyControl::new(key, label, Action::RepositoryRow(RowAction::Graph(action)))
                    .with_priority(priority)
                    .in_group(group)
                    .disabled_by(reason);
            match compact {
                Some(compact) => control.with_compact(compact),
                None => control,
            }
        })
        .collect();
    prepared.push(
        KeyControl::new(
            "R",
            "Re-read original",
            Action::GraphRereadV3(
                crate::repository_graph_reread_native_v3::GraphRereadActionV3::Open,
            ),
        )
        .with_compact("Re-read")
        .with_priority(35)
        .disabled_by(busy),
    );
    prepared
}

/// The muted graph pan position shown beside the tab strip, or on its own row when narrow.
pub fn horizontal_status(workspace: &Workspace) -> String {
    format!("Graph horizontal: {} cells", workspace.graph_horizontal_v1)
}
#[cfg(test)]
#[path = "repository_graph_focus_v2_tests.rs"]
mod focus_v2_tests;

pub fn line(
    workspace: &Workspace,
    index: usize,
    theme: Theme,
    width: usize,
) -> Option<Line<'static>> {
    let paint = workspace.graph_paint_v1(index)?;
    let fallback = paint
        .is_empty()
        .then(|| {
            workspace
                .display_rows_v1()
                .get(index)
                .map(|row| (row.label.as_str(), None))
        })
        .flatten();
    let mut spans = Vec::new();
    let mut skipped = 0usize;
    let mut drawn = 0usize;
    // Pan graph lanes, not their semantic headers or another view's actions.
    // The stored offset still belongs to the Timeline position for Back.
    let horizontal = if workspace.graph_view_v1 == GraphViewV1::Timeline
        && paint.iter().any(|part| part.color.is_some())
    {
        workspace.graph_horizontal_v1
    } else {
        0
    };
    for (part, color) in paint
        .iter()
        .map(|part| (part.text.as_str(), part.color))
        .chain(fallback)
    {
        let mut text = String::new();
        for glyph in part.graphemes(true) {
            let cells = crate::editor::grapheme_width(glyph);
            if color.is_some() && skipped < horizontal {
                skipped += cells;
                continue;
            }
            if drawn + cells > width {
                break;
            }
            text.push_str(glyph);
            drawn += cells;
        }
        if !text.is_empty() {
            spans.push(if let Some(color) = color {
                Span::styled(text, theme.graph(color))
            } else {
                Span::raw(text)
            });
        }
        if drawn >= width {
            break;
        }
    }
    Some(Line::from(spans))
}
#[cfg(test)]
#[path = "repository_graph_ui_v1_tests.rs"]
mod tests;
