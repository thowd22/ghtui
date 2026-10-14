//! Bounded prepared Graph spans and shared action mapping, no graph scan/I/O.
use crate::{
    app::Action,
    repository_graph_v1::{GraphActionV1, GraphViewV1},
    theme::Theme,
    workspace::{RowAction, Workspace},
};
use crossterm::event::KeyCode;
#[path = "repository_graph_navigation_v2.rs"]
pub(crate) mod navigation_v2;
use ratatui::text::{Line, Span};
use ratatui::{layout::Rect, widgets::Paragraph, Frame};
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
        KeyCode::Char('N') => Some(GraphActionV1::More),
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
pub fn controls(workspace: &Workspace) -> Vec<(&'static str, Action)> {
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
    let controls = if workspace.graph_view_v1 == GraphViewV1::Timeline {
        vec![
            (
                "[< Newer loaded]",
                GraphActionV1::Window(crate::repository_graph_v1::GraphWindowActionV2::Newer),
            ),
            (
                "[> Older loaded]",
                GraphActionV1::Window(crate::repository_graph_v1::GraphWindowActionV2::Older),
            ),
            (
                "[Home First loaded]",
                GraphActionV1::Window(crate::repository_graph_v1::GraphWindowActionV2::First),
            ),
            (
                "[End Last loaded]",
                GraphActionV1::Window(crate::repository_graph_v1::GraphWindowActionV2::Last),
            ),
            ("[x Discard graph Back]", GraphActionV1::DiscardBack),
            ("[N More history]", GraphActionV1::More),
            ("[P Parents]", GraphActionV1::Parents),
            ("[D Details]", GraphActionV1::Details),
            ("[l Legend]", GraphActionV1::Legend),
            ("[← Graph]", GraphActionV1::Horizontal(-8)),
            ("[Graph →]", GraphActionV1::Horizontal(8)),
        ]
    } else {
        vec![
            ("[u Graph back]", GraphActionV1::Back),
            ("[← Graph]", GraphActionV1::Horizontal(-8)),
            ("[Graph →]", GraphActionV1::Horizontal(8)),
        ]
    };
    let mut prepared: Vec<_> = controls
        .into_iter()
        .map(|(label, action)| {
            let typed = workspace.graph_action_reason_v1(&action).map_or_else(
                || Action::RepositoryRow(RowAction::Graph(action)),
                Action::Explain,
            );
            (label, typed)
        })
        .collect();
    prepared.push((
        "[R Re-read original]",
        Action::GraphRereadV3(crate::repository_graph_reread_native_v3::GraphRereadActionV3::Open),
    ));
    prepared
}
#[derive(Clone, Copy, Debug)]
pub struct GraphControlFocusV2 {
    pub hit: usize,
    pub content_focused: bool,
}
#[cfg(test)]
#[path = "repository_graph_focus_v2_tests.rs"]
mod focus_v2_tests;

pub fn draw_controls(
    frame: &mut Frame,
    workspace: &Workspace,
    hits: &mut Vec<crate::app::HitRegion>,
    area: Rect,
    theme: Theme,
    busy: bool,
    focus: GraphControlFocusV2,
) -> u16 {
    let mut x = area.x;
    let mut row = 0u16;
    let controls = controls(workspace);
    if controls.is_empty() || area.width == 0 || area.height == 0 {
        return 0;
    }
    for (label, action) in controls {
        let width = Line::from(label).width() as u16;
        if x != area.x && x.saturating_add(width) > area.right() {
            x = area.x;
            row += 1;
        }
        if row >= area.height || width > area.width {
            break;
        }
        let action = if busy {
            Action::Explain("Wait for the original graph read, or cancel it first".into())
        } else {
            action
        };
        let rect = Rect::new(x, area.y + row, width, 1);
        let disabled = matches!(action, Action::Explain(_));
        let focused = !focus.content_focused && focus.hit == hits.len();
        let mut style = theme.style(if disabled {
            crate::theme::SemanticToken::Muted
        } else {
            crate::theme::SemanticToken::Reference
        });
        if focused {
            style = style.add_modifier(ratatui::style::Modifier::UNDERLINED);
        }
        frame.render_widget(Paragraph::new(label).style(style), rect);
        hits.push(crate::app::HitRegion { rect, action });
        x = rect.right().saturating_add(1);
    }
    let horizontal = format!("Graph horizontal: {} cells", workspace.graph_horizontal_v1);
    let status_width = Line::from(horizontal.as_str()).width() as u16;
    if x.saturating_add(status_width) <= area.right() || row + 1 < area.height {
        let status_x = if x.saturating_add(status_width) <= area.right() {
            x
        } else {
            row += 1;
            area.x
        };
        frame.render_widget(
            Paragraph::new(horizontal).style(theme.style(crate::theme::SemanticToken::Muted)),
            Rect::new(status_x, area.y + row, area.right() - status_x, 1),
        );
    }
    row + 1
}
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
