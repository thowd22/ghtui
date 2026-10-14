//! SOURCE-ONLY shared editor contracts. Undeclared and UNRUN until Root applies
//! the reviewed viewport patch and registers this as an editor.rs test child.
use super::*;
use ratatui::{
    backend::TestBackend,
    layout::{Position, Rect},
    widgets::{Block, Borders, Paragraph},
    Terminal,
};

#[test]
fn mouse_keyboard_editor_viewport_wide_filter_80_columns_62_cells_exact_origin() {
    let raw = "界".repeat(64);
    let mut editor = TextBuffer::new(&raw).unwrap();
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let mut field = Rect::default();
    terminal
        .draw(|frame| {
            let area = Rect::new(0, 0, 80, 3);
            let block = Block::default().borders(Borders::ALL);
            let inner = block.inner(area);
            frame.render_widget(block, area);
            let caption = "Filter focused: ";
            let caption_width = display_width(caption) as u16;
            frame.render_widget(
                Paragraph::new(caption),
                Rect::new(inner.x, inner.y, caption_width, 1),
            );
            field = Rect::new(
                inner.x + caption_width,
                inner.y,
                inner.width - caption_width,
                1,
            );
            editor.ensure_visible(usize::from(field.width), 1);
            frame.render_widget(
                Paragraph::new(display_line(
                    &editor.text,
                    editor.left_column,
                    usize::from(field.width),
                )),
                field,
            );
            frame.set_cursor_position((
                field.x + (editor.position().1 - editor.left_column) as u16,
                field.y,
            ));
        })
        .unwrap();
    assert_eq!(field, Rect::new(17, 1, 62, 1));
    assert_eq!(editor.left_column, 68, "never start inside a wide glyph");
    assert_eq!(
        editor.cursor, 192,
        "caret remains an original UTF8 byte index"
    );
    assert_eq!(
        terminal.get_cursor_position().unwrap(),
        Position::new(77, 1)
    );
    let cells = terminal.backend().buffer();
    for column in (17..77).step_by(2) {
        assert_eq!(cells[(column, 1)].symbol(), "界");
    }
    assert_eq!(cells[(77, 1)].symbol(), " ");
    assert_eq!(display_line(&raw, editor.left_column, 62), "界".repeat(30));
    editor.click(0, 0, false);
    assert_eq!(editor.cursor, 34 * "界".len());
    assert_eq!(editor.position(), (0, 68));
    assert_eq!(editor.text, raw);
}

#[test]
fn mouse_keyboard_editor_viewport_combining_selection_preserves_exact_raw_bytes() {
    let raw = format!("{}e\u{301}", "界".repeat(64));
    let mut mouse = TextBuffer::new(&raw).unwrap();
    mouse.ensure_visible(62, 1);
    assert_eq!(mouse.left_column, 68);
    assert_eq!(mouse.position(), (0, 129));
    assert_eq!(
        display_line(&raw, 68, 62),
        format!("{}e\u{301}", "界".repeat(30))
    );
    mouse.click(0, 60, false);
    assert_eq!(mouse.cursor, 192);
    mouse.click(0, 61, true);
    assert_eq!(mouse.selection(), Some(192..195));
    assert_eq!(mouse.selected_text(), "e\u{301}");

    let mut keyboard = TextBuffer::new(&raw).unwrap();
    keyboard.horizontal(false, true);
    assert_eq!(keyboard.selection(), mouse.selection());
    mouse.insert("X").unwrap();
    keyboard.insert("X").unwrap();
    assert_eq!(mouse.text, format!("{}X", "界".repeat(64)));
    assert_eq!(keyboard.text, mouse.text);
    assert_eq!(keyboard.cursor, mouse.cursor);
    assert_eq!(mouse.anchor, None);
}

#[test]
fn mouse_keyboard_editor_viewport_zwj_and_tab_cells_share_visible_click_origin() {
    let wide = "界".repeat(64);
    let emoji = "👩🏽‍💻";
    let raw = format!("{wide}{emoji}\te\u{301}");
    assert_eq!(grapheme_width(emoji), 2);
    let mut editor = TextBuffer::new(&raw).unwrap();
    editor.ensure_visible(62, 1);
    assert_eq!(editor.left_column, 74);
    assert_eq!(editor.position(), (0, 135));
    editor.click(0, 54, false);
    assert_eq!(editor.cursor, wide.len());
    editor.click(0, 56, true);
    assert_eq!(editor.selected_text(), emoji);
    editor.click(0, 57, false);
    assert_eq!(editor.cursor, wide.len() + emoji.len());
    editor.click(0, 60, true);
    assert_eq!(editor.selected_text(), "\t");
    assert_eq!(editor.text, raw);
    assert_eq!(
        display_line(&raw, editor.left_column, 62),
        format!("{}{emoji}    e\u{301}", "界".repeat(27))
    );
}

#[test]
fn mouse_keyboard_editor_viewport_scrolled_multiline_click_uses_painted_row_boundary() {
    let first = "界".repeat(64);
    let raw = format!("{first}\n{}", "x".repeat(128));
    let mut editor = TextBuffer::new(&raw).unwrap();
    editor.ensure_visible(62, 1);
    assert_eq!((editor.top_line, editor.left_column), (1, 67));
    editor.scroll(-1);
    editor.ensure_visible(3, 1);
    assert_eq!((editor.top_line, editor.left_column), (0, 67));
    assert_eq!(display_line(&first, 67, 62), "界".repeat(30));
    editor.click(0, 0, false);
    assert_eq!(
        editor.cursor, 102,
        "first actually painted glyph, not hidden predecessor"
    );
    assert_eq!(editor.position(), (0, 68));
    editor.ensure_visible(62, 1);
    assert_eq!((editor.top_line, editor.left_column), (0, 68));
    editor.click(0, 2, true);
    assert_eq!(editor.selected_text(), "界");
    assert_eq!(editor.selection(), Some(102..105));
    assert_eq!(editor.text, raw);
}

#[test]
fn mouse_keyboard_editor_viewport_resize_and_narrow_bounds_preserve_keyboard_caret() {
    for (width, expected_left) in [
        (0, 128),
        (1, 128),
        (2, 128),
        (3, 126),
        (62, 68),
        (72, 58),
        (96, 34),
    ] {
        let raw = "界".repeat(64);
        let mut editor = TextBuffer::new(&raw).unwrap();
        editor.ensure_visible(width, 1);
        assert_eq!(editor.left_column, expected_left, "field width {width}");
        assert!(editor.position().1 - editor.left_column < width.max(1));
        assert_eq!(editor.cursor, 192);
        assert_eq!(editor.text, raw);
        editor.horizontal(false, false);
        assert_eq!(editor.cursor, 189);
        editor.horizontal(true, false);
        assert_eq!(editor.cursor, 192);
    }
    let mut ascii = TextBuffer::new("x".repeat(128)).unwrap();
    ascii.ensure_visible(62, 1);
    assert_eq!(
        ascii.left_column, 67,
        "do not round ordinary one-cell characters"
    );
    ascii.click(0, 0, false);
    assert_eq!(ascii.cursor, 67);
}
