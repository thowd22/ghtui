//! TASK-120: drag over the painted frame to select its text, as in a terminal.
//!
//! A left press is always delivered to the app (a click keeps selecting rows and pressing
//! controls). When the pointer then drags, the drag becomes a text selection unless the press
//! landed on something that owns dragging: a text field, a scrollbar or pane border, a diff
//! line range, or a full-screen viewer with its own drag. The selection runs in reading order
//! inside the pane under the press, is painted reversed, and is copied on release through
//! [`crate::clipboard::copy`].
use crate::app::{Action, App, View};
use crossterm::event::{Event, KeyEventKind, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{buffer::Buffer, layout::Position, style::Modifier};
use unicode_segmentation::UnicodeSegmentation;

/// A cell on screen: column `x`, row `y`.
pub type Cell = (u16, u16);

/// One selected row: columns `from..=to`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowSpan {
    pub y: u16,
    pub from: u16,
    pub to: u16,
}

#[derive(Default)]
pub struct TextSelection {
    press: Option<Cell>,
    anchor: Option<Cell>,
    cursor: Cell,
    /// The pane's inner columns (left, right) found at the press.
    bounds: (u16, u16),
    dragging: bool,
    /// The last frame as drawn, before any selection paint.
    frame: Option<Buffer>,
}

impl TextSelection {
    pub fn clear(&mut self) {
        self.press = None;
        self.anchor = None;
        self.dragging = false;
    }
    pub fn active(&self) -> bool {
        self.anchor.is_some()
    }
    pub fn spans(&self) -> Vec<RowSpan> {
        self.anchor
            .map(|anchor| spans(anchor, self.cursor, self.bounds))
            .unwrap_or_default()
    }
    /// Keep the frame the user sees; the selection's text is read from it.
    pub fn remember_frame(&mut self, buffer: &Buffer) {
        match &mut self.frame {
            Some(frame) if frame.area == buffer.area => frame.clone_from(buffer),
            frame => *frame = Some(buffer.clone()),
        }
    }
    /// Paint the selection reversed over a drawn frame.
    pub fn paint(&self, buffer: &mut Buffer) {
        for span in self.spans() {
            for x in span.from..=span.to {
                if let Some(cell) = buffer.cell_mut(Position::new(x, span.y)) {
                    let style = cell.style();
                    let reversed = style.add_modifier.contains(Modifier::REVERSED);
                    cell.set_style(if reversed {
                        style.remove_modifier(Modifier::REVERSED)
                    } else {
                        style.add_modifier(Modifier::REVERSED)
                    });
                }
            }
        }
    }
}

/// The rows from `anchor` to `cursor` in reading order, kept inside `bounds` (inclusive).
pub fn spans(anchor: Cell, cursor: Cell, bounds: (u16, u16)) -> Vec<RowSpan> {
    let (left, right) = bounds;
    if left > right {
        return Vec::new();
    }
    let clamp = |(x, y): Cell| (x.clamp(left, right), y);
    let (a, c) = (clamp(anchor), clamp(cursor));
    let (start, end) = if (a.1, a.0) <= (c.1, c.0) {
        (a, c)
    } else {
        (c, a)
    };
    (start.1..=end.1)
        .map(|y| RowSpan {
            y,
            from: if y == start.1 { start.0 } else { left },
            to: if y == end.1 { end.0 } else { right },
        })
        .collect()
}

/// Glyphs that draw pane borders and scrollbars. A drag that starts on one is not a text
/// selection, and a vertical one bounds the selection's pane.
fn is_vertical_edge(symbol: &str) -> bool {
    matches!(
        symbol,
        "│" | "┃" | "║" | "╭" | "╮" | "╰" | "╯" | "├" | "┤" | "┌" | "┐" | "└" | "┘"
    )
}
fn is_chrome(symbol: &str) -> bool {
    is_vertical_edge(symbol)
        || matches!(symbol, "─" | "━" | "═" | "▲" | "▼" | "█" | "┬" | "┴" | "┼")
}

/// The inner columns of the pane holding `at`: between the nearest vertical edges on its row.
pub fn pane_bounds(buffer: &Buffer, at: Cell) -> (u16, u16) {
    let area = buffer.area;
    let symbol = |x: u16| {
        buffer
            .cell(Position::new(x, at.1))
            .map(|cell| cell.symbol().to_owned())
            .unwrap_or_default()
    };
    let left = (area.x..at.0)
        .rev()
        .find(|x| is_vertical_edge(&symbol(*x)))
        .map_or(area.x, |x| x + 1);
    let right = (at.0 + 1..area.right())
        .find(|x| is_vertical_edge(&symbol(*x)))
        .map_or(area.right().saturating_sub(1), |x| x - 1);
    (left, right.max(left))
}

/// The text of `spans` in `buffer`: each row's cells in order (a wide character once),
/// trailing blanks dropped, rows joined by newlines.
pub fn text(buffer: &Buffer, spans: &[RowSpan]) -> String {
    let mut rows = Vec::with_capacity(spans.len());
    for span in spans {
        let mut row = String::new();
        let mut x = span.from;
        while x <= span.to {
            let Some(cell) = buffer.cell(Position::new(x, span.y)) else {
                break;
            };
            let symbol = cell.symbol();
            row.push_str(symbol);
            let width = unicode_width(symbol);
            x = x.saturating_add(width.max(1));
            if x == u16::MAX {
                break;
            }
        }
        rows.push(row.trim_end().to_owned());
    }
    while rows.last().is_some_and(String::is_empty) {
        rows.pop();
    }
    rows.join("\n")
}

fn unicode_width(symbol: &str) -> u16 {
    symbol
        .graphemes(true)
        .map(|grapheme| crate::editor::grapheme_width(grapheme) as u16)
        .sum()
}

/// Whether a drag from `at` belongs to the app rather than to text selection.
fn owned_by_app(app: &App, frame: Option<&Buffer>, at: Cell) -> bool {
    let point = Position::new(at.0, at.1);
    if frame
        .and_then(|frame| frame.cell(point))
        .is_some_and(|cell| is_chrome(cell.symbol()))
    {
        return true;
    }
    // Full-screen viewers and dialogs with their own drag (line ranges, panning, editors).
    if app.native_diff_v1.is_some()
        || app.native_mutable_diff_v2.is_some()
        || app.pr_native_diff_v1.is_some()
        || app.pr_file_images.is_some()
        || app.catalogue.is_some()
        || app.overlay == Some(crate::app::Overlay::ScopedActionsReadV2)
    {
        return true;
    }
    if app
        .form
        .as_ref()
        .is_some_and(|form| form.field_rects.iter().any(|rect| rect.contains(point)))
    {
        return true;
    }
    if app
        .search_controls
        .fields
        .iter()
        .any(|(_, rect)| rect.contains(point))
    {
        return true;
    }
    // Repository diff rows extend a line range by dragging.
    app.view == View::Repository
        && app.hits.iter().any(|hit| {
            hit.rect.contains(point)
                && matches!(
                    hit.action,
                    Action::RepositoryRow(crate::workspace::RowAction::DiffLine(_))
                )
        })
}

/// Selection input. Returns true when the event was used here and must not reach the app.
pub fn event(app: &mut App, event: &Event) -> bool {
    match event {
        Event::Mouse(mouse) => self::mouse(app, mouse),
        Event::Key(key) if key.kind != KeyEventKind::Release => {
            app.copy_v120.selection.clear();
            false
        }
        Event::Resize(_, _) => {
            app.copy_v120.selection.clear();
            false
        }
        _ => false,
    }
}

fn mouse(app: &mut App, mouse: &MouseEvent) -> bool {
    let at = (mouse.column, mouse.row);
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let owned = owned_by_app(app, app.copy_v120.selection.frame.as_ref(), at);
            let selection = &mut app.copy_v120.selection;
            selection.clear();
            selection.press = (!owned).then_some(at);
            false
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            let selection = &mut app.copy_v120.selection;
            if selection.dragging {
                selection.cursor = at;
                return true;
            }
            let Some(press) = selection.press else {
                return false;
            };
            if press == at {
                return false;
            }
            selection.bounds = selection
                .frame
                .as_ref()
                .map_or((0, u16::MAX - 1), |frame| pane_bounds(frame, press));
            selection.anchor = Some(press);
            selection.cursor = at;
            selection.dragging = true;
            true
        }
        MouseEventKind::Up(MouseButton::Left) => {
            let selection = &mut app.copy_v120.selection;
            selection.press = None;
            if !selection.dragging {
                return false;
            }
            selection.dragging = false;
            selection.cursor = at;
            let copied = selection
                .frame
                .as_ref()
                .map(|frame| text(frame, &selection.spans()))
                .unwrap_or_default();
            if copied.trim().is_empty() {
                app.status = "Nothing copied • the selection holds only blank cells".into();
            } else {
                crate::clipboard::copy(app, "selection", &copied);
            }
            // The release still ends any app-owned press (a scrollbar grab, for one).
            false
        }
        _ => false,
    }
}

#[cfg(test)]
#[path = "text_selection_tests.rs"]
mod tests;
