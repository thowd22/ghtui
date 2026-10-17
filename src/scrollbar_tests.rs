use super::*;
use ratatui::{backend::TestBackend, Terminal};

const DOWN: MouseEventKind = MouseEventKind::Down(MouseButton::Left);
const DRAG: MouseEventKind = MouseEventKind::Drag(MouseButton::Left);
const UP: MouseEventKind = MouseEventKind::Up(MouseButton::Left);

#[test]
fn mouse_keyboard_scrollbar_one_hidden_row_keeps_drag_travel_and_both_exact_endpoints() {
    let mut bar = Scrollbar::default();
    let area = Rect::new(79, 3, 1, 12);
    bar.layout(area, 11, 10, 0);
    assert_eq!(
        bar.thumb_height, 9,
        "one track cell must remain for the hidden row"
    );
    assert_eq!(bar.thumb_top, 4);
    bar.mouse(DOWN, 79, 4);
    assert_eq!(bar.mouse(DRAG, 79, 14), Some(ScrollRequest::To(1)));
    bar.layout(area, 11, 10, 1);
    assert_eq!(bar.thumb_top, 5);
    bar.mouse(UP, 79, 14);
    bar.mouse(DOWN, 79, 5);
    assert_eq!(bar.mouse(DRAG, 79, 3), Some(ScrollRequest::To(0)));
}

#[test]
fn mouse_keyboard_scrollbar_arrows_and_track_emit_exact_line_and_page_requests() {
    let mut bar = Scrollbar::default();
    bar.layout(Rect::new(79, 3, 1, 12), 100, 10, 45);
    assert_eq!(bar.thumb_height, 1);
    assert_eq!(bar.thumb_top, 8);
    for (row, request) in [
        (3, ScrollRequest::By(-1)),
        (14, ScrollRequest::By(1)),
        (4, ScrollRequest::By(-10)),
        (13, ScrollRequest::By(10)),
    ] {
        assert_eq!(bar.mouse(DOWN, 79, row), Some(request));
        assert_eq!(
            bar.mouse(UP, 79, row),
            None,
            "release never duplicates a click"
        );
    }
    assert_eq!(
        bar.mouse(MouseEventKind::ScrollDown, 79, 8),
        Some(ScrollRequest::By(3))
    );
    assert_eq!(
        bar.mouse(MouseEventKind::ScrollUp, 79, 8),
        Some(ScrollRequest::By(-3))
    );
    assert_eq!(bar.mouse(DOWN, 78, 3), None);
    assert_eq!(bar.mouse(MouseEventKind::ScrollDown, 78, 8), None);
    assert_eq!(
        bar.mouse(MouseEventKind::Down(MouseButton::Right), 79, 8),
        None
    );
}

#[test]
fn mouse_keyboard_scrollbar_drag_survives_identical_redraws_and_clamps_outside_release() {
    let area = Rect::new(79, 3, 1, 12);
    let mut bar = Scrollbar::default();
    bar.layout(area, 100, 10, 0);
    assert_eq!(bar.mouse(DOWN, 79, 4), None);
    bar.layout(area, 100, 10, 0);
    assert_eq!(bar.mouse(DRAG, 20, 50), Some(ScrollRequest::To(90)));
    bar.layout(area, 100, 10, 90);
    assert_eq!(bar.mouse(DRAG, 0, 0), Some(ScrollRequest::To(0)));
    assert_eq!(bar.mouse(UP, 0, 0), None);
    assert_eq!(
        bar.mouse(DRAG, 79, 13),
        None,
        "outside release ends capture"
    );
}

#[test]
fn mouse_keyboard_scrollbar_resize_source_shrink_and_explicit_retirement_drop_drag() {
    let area = Rect::new(79, 3, 1, 12);
    for change in 0..5 {
        let mut bar = Scrollbar::default();
        bar.layout(area, 100, 10, 0);
        bar.mouse(DOWN, 79, 4);
        match change {
            0 => bar.layout(Rect::new(119, 3, 1, 12), 100, 10, 0),
            1 => bar.layout(area, 101, 10, 0),
            2 => bar.layout(area, 100, 11, 0),
            3 => bar.clear(),
            _ => bar.cancel_drag(),
        }
        assert_eq!(bar.mouse(DRAG, 79, 13), None);
    }
    for (area, total, visible) in [
        (area, 10, 10),
        (area, 0, 10),
        (area, 100, 0),
        (Rect::new(79, 3, 1, 2), 100, 10),
        (Rect::new(79, 3, 0, 12), 100, 10),
    ] {
        let mut bar = Scrollbar::default();
        bar.layout(area, total, visible, 0);
        assert!(bar.area.is_empty());
        assert_eq!(bar.mouse(DOWN, 79, 3), None);
    }
}

#[test]
fn mouse_keyboard_scrollbar_extreme_row_count_has_exact_endpoints_without_overflow() {
    let mut bar = Scrollbar::default();
    let area = Rect::new(0, 0, 1, 24);
    let max = usize::MAX - 28;
    bar.layout(area, usize::MAX, 28, usize::MAX);
    assert_eq!(bar.position, max);
    assert_eq!(bar.thumb_height, 1);
    assert_eq!(bar.thumb_top, 22);
    bar.mouse(DOWN, 0, 22);
    assert_eq!(bar.mouse(DRAG, 0, 0), Some(ScrollRequest::To(0)));
    assert_eq!(bar.mouse(DRAG, 0, u16::MAX), Some(ScrollRequest::To(max)));
}

#[test]
fn mouse_keyboard_scrollbar_no_color_symbols_and_hidden_all_fit_preserve_text_cells() {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let mut bar = Scrollbar::default();
    terminal
        .draw(|frame| {
            frame.render_widget(
                Paragraph::new("unchanged visible text"),
                Rect::new(1, 3, 70, 1),
            );
            bar.draw(
                frame,
                Rect::new(79, 3, 1, 12),
                100,
                10,
                0,
                Theme::from_settings(0, true),
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(79, 3)].symbol(), "▲");
    assert_eq!(buffer[(79, 4)].symbol(), "█");
    assert_eq!(buffer[(79, 14)].symbol(), "▼");
    assert_eq!(buffer[(1, 3)].symbol(), "u");
    terminal
        .draw(|frame| {
            frame.render_widget(Paragraph::new("│"), Rect::new(79, 3, 1, 1));
            bar.draw(
                frame,
                Rect::new(79, 3, 1, 12),
                10,
                10,
                0,
                Theme::from_settings(0, true),
            );
        })
        .unwrap();
    assert_eq!(terminal.backend().buffer()[(79, 3)].symbol(), "│");
    assert_eq!(bar.mouse(DOWN, 79, 3), None);
}
