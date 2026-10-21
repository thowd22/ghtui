use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;

fn buffer(lines: &[&str]) -> Buffer {
    let width = lines
        .iter()
        .map(|line| unicode_width(line))
        .max()
        .unwrap_or(0);
    let mut buffer = Buffer::empty(Rect::new(0, 0, width, lines.len() as u16));
    for (y, line) in lines.iter().enumerate() {
        buffer.set_string(0, y as u16, line, ratatui::style::Style::default());
    }
    buffer
}

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> Event {
    Event::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

#[test]
fn text_selection_spans_run_in_reading_order_inside_bounds() {
    // Forward and backward drags select the same cells.
    let forward = spans((3, 1), (2, 3), (1, 8));
    assert_eq!(forward, spans((2, 3), (3, 1), (1, 8)));
    assert_eq!(
        forward,
        vec![
            RowSpan {
                y: 1,
                from: 3,
                to: 8
            },
            RowSpan {
                y: 2,
                from: 1,
                to: 8
            },
            RowSpan {
                y: 3,
                from: 1,
                to: 2
            },
        ]
    );
    // One row, columns clamped into the pane.
    assert_eq!(
        spans((0, 4), (20, 4), (2, 9)),
        vec![RowSpan {
            y: 4,
            from: 2,
            to: 9
        }]
    );
    // A backward drag on one row.
    assert_eq!(
        spans((7, 0), (4, 0), (0, 9)),
        vec![RowSpan {
            y: 0,
            from: 4,
            to: 7
        }]
    );
}

#[test]
fn text_selection_pane_bounds_stop_at_vertical_borders() {
    let frame = buffer(&["╭ left ──╮╭ right ─╮", "│ abc    ││ def    │"]);
    assert_eq!(pane_bounds(&frame, (3, 1)), (1, 8));
    assert_eq!(pane_bounds(&frame, (13, 1)), (11, 18));
    let plain = buffer(&["no borders here"]);
    assert_eq!(pane_bounds(&plain, (4, 0)), (0, 14));
}

#[test]
fn text_selection_text_trims_rows_and_keeps_wide_characters_once() {
    let frame = buffer(&["│ alpha beta   │", "│ 日本 gamma   │", "│              │"]);
    let bounds = pane_bounds(&frame, (2, 0));
    let selected = text(&frame, &spans((2, 0), (14, 2), bounds));
    assert_eq!(selected, "alpha beta\n 日本 gamma");
    let word = text(&frame, &spans((8, 0), (11, 0), bounds));
    assert_eq!(word, "beta");
}

#[test]
fn text_selection_drag_selects_and_release_copies_with_osc52() {
    let mut app = App::default();
    app.copy_v120
        .selection
        .remember_frame(&buffer(&["│ 0123456789abcdef │", "│ second line      │"]));
    // The press reaches the app; the drag is the selection's; release copies.
    assert!(!event(
        &mut app,
        &mouse(MouseEventKind::Down(MouseButton::Left), 2, 0)
    ));
    assert!(event(
        &mut app,
        &mouse(MouseEventKind::Drag(MouseButton::Left), 8, 1)
    ));
    assert!(app.copy_v120.selection.active());
    assert!(event(
        &mut app,
        &mouse(MouseEventKind::Drag(MouseButton::Left), 7, 1)
    ));
    assert!(!event(
        &mut app,
        &mouse(MouseEventKind::Up(MouseButton::Left), 7, 1)
    ));
    let copied = "0123456789abcdef\n second";
    assert!(
        app.status
            .starts_with("Copied selection (24 chars) via terminal OSC 52"),
        "{}",
        app.status
    );
    assert_eq!(
        app.copy_v120.pending_osc52.as_deref(),
        Some(crate::clipboard::osc52_sequence(copied).unwrap().as_slice())
    );
    // The highlight stays until the next key.
    let mut painted = buffer(&["│ 0123456789abcdef │", "│ second line      │"]);
    app.copy_v120.selection.paint(&mut painted);
    let reversed = |x: u16, y: u16| {
        painted
            .cell(Position::new(x, y))
            .unwrap()
            .modifier
            .contains(Modifier::REVERSED)
    };
    assert!(reversed(2, 0) && reversed(7, 1) && !reversed(8, 1) && !reversed(0, 0));
    event(
        &mut app,
        &Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
    );
    assert!(!app.copy_v120.selection.active());
}

#[test]
fn text_selection_leaves_scrollbars_borders_and_fields_to_the_app() {
    let mut app = App::default();
    app.copy_v120.selection.remember_frame(&buffer(&[
        "│ text here  ▲",
        "│ more text  █",
        "│ end        ▼",
    ]));
    // A scrollbar thumb drag stays the app's.
    assert!(!event(
        &mut app,
        &mouse(MouseEventKind::Down(MouseButton::Left), 13, 1)
    ));
    assert!(!event(
        &mut app,
        &mouse(MouseEventKind::Drag(MouseButton::Left), 13, 2)
    ));
    assert!(!app.copy_v120.selection.active());
    assert!(!event(
        &mut app,
        &mouse(MouseEventKind::Up(MouseButton::Left), 13, 2)
    ));
    assert!(app.copy_v120.pending_osc52.is_none());
    // So does a drag from a pane border.
    assert!(!event(
        &mut app,
        &mouse(MouseEventKind::Down(MouseButton::Left), 0, 0)
    ));
    assert!(!event(
        &mut app,
        &mouse(MouseEventKind::Drag(MouseButton::Left), 5, 1)
    ));
    // And a drag that starts inside a search field.
    app.search_controls.fields =
        vec![(crate::search_ui::SearchField::Query, Rect::new(2, 0, 8, 1))];
    assert!(!event(
        &mut app,
        &mouse(MouseEventKind::Down(MouseButton::Left), 3, 0)
    ));
    assert!(!event(
        &mut app,
        &mouse(MouseEventKind::Drag(MouseButton::Left), 6, 0)
    ));
    // A click without a drag copies nothing.
    app.search_controls.fields.clear();
    assert!(!event(
        &mut app,
        &mouse(MouseEventKind::Down(MouseButton::Left), 3, 0)
    ));
    assert!(!event(
        &mut app,
        &mouse(MouseEventKind::Up(MouseButton::Left), 3, 0)
    ));
    assert!(app.copy_v120.pending_osc52.is_none());
}
