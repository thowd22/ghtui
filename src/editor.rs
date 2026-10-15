//! Bounded grapheme-aware editing shared by composers and native action forms.
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

pub const MAX_TEXT_BYTES: usize = 256 * 1024;
#[derive(Clone, Debug, Default)]
pub struct TextBuffer {
    pub text: String,
    pub cursor: usize,
    pub anchor: Option<usize>,
    pub top_line: usize,
    pub left_column: usize,
    preferred_column: Option<usize>,
    manual_scroll: bool,
}
impl TextBuffer {
    pub fn new(text: impl Into<String>) -> Result<Self, String> {
        let text = text.into();
        if text.len() > MAX_TEXT_BYTES {
            return Err("Text exceeds the 256 KiB editor limit".into());
        }
        let cursor = text.len();
        Ok(Self {
            text,
            cursor,
            ..Self::default()
        })
    }
    pub fn selection(&self) -> Option<Range<usize>> {
        self.anchor
            .filter(|anchor| *anchor != self.cursor)
            .map(|anchor| anchor.min(self.cursor)..anchor.max(self.cursor))
    }
    pub fn selected_text(&self) -> &str {
        self.selection().map_or("", |range| &self.text[range])
    }
    pub fn select_all(&mut self) {
        self.anchor = Some(0);
        self.cursor = self.text.len();
    }
    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.anchor = None;
        self.top_line = 0;
        self.left_column = 0;
        self.preferred_column = None;
        self.manual_scroll = false;
    }
    pub fn insert(&mut self, value: &str) -> Result<(), String> {
        let value = value
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .chars()
            .filter(|ch| !ch.is_control() || matches!(ch, '\n' | '\t'))
            .collect::<String>();
        let selection = self.selection().unwrap_or(self.cursor..self.cursor);
        if self.text.len() - selection.len() + value.len() > MAX_TEXT_BYTES {
            return Err("Text exceeds the 256 KiB editor limit".into());
        }
        self.text.replace_range(selection.clone(), &value);
        self.cursor = selection.start + value.len();
        self.anchor = None;
        self.preferred_column = None;
        self.manual_scroll = false;
        Ok(())
    }
    pub fn delete_backward(&mut self) {
        if self.selection().is_some() {
            let _ = self.insert("");
            return;
        }
        let previous = self.text[..self.cursor]
            .grapheme_indices(true)
            .next_back()
            .map(|(index, _)| index)
            .unwrap_or(0);
        self.text.replace_range(previous..self.cursor, "");
        self.cursor = previous;
        self.anchor = None;
        self.preferred_column = None;
    }
    pub fn delete_forward(&mut self) {
        if self.selection().is_some() {
            let _ = self.insert("");
            return;
        }
        if let Some(grapheme) = self.text[self.cursor..].graphemes(true).next() {
            self.text
                .replace_range(self.cursor..self.cursor + grapheme.len(), "");
        }
        self.anchor = None;
        self.preferred_column = None;
    }
    fn set_cursor(&mut self, cursor: usize, select: bool) {
        self.manual_scroll = false;
        if select {
            self.anchor.get_or_insert(self.cursor);
        } else {
            self.anchor = None;
        }
        self.cursor = cursor;
    }
    pub fn horizontal(&mut self, right: bool, select: bool) {
        if !select {
            if let Some(selection) = self.selection() {
                self.set_cursor(
                    if right {
                        selection.end
                    } else {
                        selection.start
                    },
                    false,
                );
                self.preferred_column = None;
                return;
            }
        }
        let next = if right {
            self.text[self.cursor..]
                .graphemes(true)
                .next()
                .map_or(self.cursor, |grapheme| self.cursor + grapheme.len())
        } else {
            self.text[..self.cursor]
                .grapheme_indices(true)
                .next_back()
                .map_or(0, |(index, _)| index)
        };
        self.set_cursor(next, select);
        self.preferred_column = None;
    }
    pub fn position(&self) -> (usize, usize) {
        let prefix = &self.text[..self.cursor];
        let row = prefix.bytes().filter(|byte| *byte == b'\n').count();
        (
            row,
            display_width(prefix.rsplit('\n').next().unwrap_or_default()),
        )
    }
    fn at_position(&self, row: usize, column: usize) -> usize {
        let mut start = 0;
        for (index, line) in self.text.split('\n').enumerate() {
            if index == row {
                let mut width = 0;
                for (offset, grapheme) in line.grapheme_indices(true) {
                    let next = width + grapheme_width(grapheme);
                    if next > column {
                        return start + offset;
                    }
                    width = next;
                }
                return start + line.len();
            }
            start += line.len() + 1;
        }
        self.text.len()
    }
    pub fn vertical(&mut self, down: bool, select: bool) {
        let (row, column) = self.position();
        let column = self.preferred_column.unwrap_or(column);
        let target = if down { row + 1 } else { row.saturating_sub(1) };
        self.set_cursor(self.at_position(target, column), select);
        self.preferred_column = Some(column);
    }
    pub fn line_edge(&mut self, end: bool, document: bool, select: bool) {
        let next = if document {
            if end {
                self.text.len()
            } else {
                0
            }
        } else if end {
            self.text[self.cursor..]
                .find('\n')
                .map_or(self.text.len(), |offset| self.cursor + offset)
        } else {
            self.text[..self.cursor]
                .rfind('\n')
                .map_or(0, |offset| offset + 1)
        };
        self.set_cursor(next, select);
        self.preferred_column = None;
    }
    pub fn click(&mut self, row: usize, column: usize, extend: bool) {
        let row = row + self.top_line;
        let line = self.text.split('\n').nth(row).unwrap_or_default();
        // Use the same first painted grapheme as display_line, including rows
        // with different wide/combining boundaries in a multiline viewport.
        let left = grapheme_column_at_or_after(line, self.left_column);
        self.set_cursor(self.at_position(row, column + left), extend);
        self.preferred_column = None;
    }
    pub fn ensure_visible(&mut self, width: usize, height: usize) {
        if self.manual_scroll {
            return;
        }
        let (row, column) = self.position();
        if row < self.top_line {
            self.top_line = row;
        } else if row >= self.top_line + height.max(1) {
            self.top_line = row + 1 - height.max(1);
        }
        // A narrow field may have scrolled horizontally to keep its caret in
        // view. Once the whole prefix up to that caret fits again, do not keep
        // hiding it after a resize. This uses the already measured position,
        // not an additional full-line scan, and preserves manual scrolling.
        if column < width.max(1) {
            self.left_column = 0;
        } else if column < self.left_column {
            self.left_column = column;
        } else if column >= self.left_column + width.max(1) {
            self.left_column = column + 1 - width.max(1);
        }
        let line = self.text.split('\n').nth(row).unwrap_or_default();
        self.left_column = grapheme_column_at_or_after(line, self.left_column);
    }
    pub fn scroll(&mut self, delta: isize) {
        self.manual_scroll = true;
        let max = self.text.bytes().filter(|byte| *byte == b'\n').count();
        self.top_line = self.top_line.saturating_add_signed(delta).min(max);
    }
}
// Display cells are not raw byte offsets. Mirror the first whole grapheme
// painted by display_line instead of starting inside a wide glyph or a tab.
fn grapheme_column_at_or_after(line: &str, requested: usize) -> usize {
    let mut column = 0;
    for grapheme in line.graphemes(true) {
        if column >= requested {
            return column;
        }
        column += grapheme_width(grapheme);
    }
    column
}
pub fn grapheme_width(grapheme: &str) -> usize {
    if grapheme == "\t" {
        4
    } else {
        ratatui::text::Line::from(grapheme).width()
    }
}
pub fn display_width(value: &str) -> usize {
    value.graphemes(true).map(grapheme_width).sum()
}
pub fn display_line(value: &str, left: usize, width: usize) -> String {
    let mut consumed = 0;
    let mut out = String::new();
    let mut shown = 0;
    for grapheme in value.graphemes(true) {
        let cells = grapheme_width(grapheme);
        if consumed >= left {
            if shown + cells > width {
                break;
            }
            if grapheme == "\t" {
                out.push_str("    ");
            } else {
                out.push_str(grapheme);
            }
            shown += cells;
        }
        consumed += cells;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn composer_graphemes_selection_multiline_paste_and_rejected_bounds() {
        let mut buffer = TextBuffer::new("café 👩🏽‍💻 e\u{301}").unwrap();
        buffer.delete_backward();
        assert_eq!(buffer.text, "café 👩🏽‍💻 ");
        buffer.delete_backward();
        buffer.delete_backward();
        assert_eq!(buffer.text, "café ");
        buffer.select_all();
        buffer.insert("第一行\r\nsecond\tline").unwrap();
        assert_eq!(buffer.text, "第一行\nsecond\tline");
        buffer.line_edge(false, true, false);
        buffer.horizontal(true, true);
        assert_eq!(buffer.selected_text(), "第");
        buffer.insert("X").unwrap();
        assert_eq!(buffer.text, "X一行\nsecond\tline");
        let previous = buffer.text.clone();
        assert!(buffer.insert(&"x".repeat(MAX_TEXT_BYTES)).is_err());
        assert_eq!(buffer.text, previous);
    }
    #[test]
    fn form_preview_visible_editor_growth_reveals_prefix_without_changing_text_cursor_or_selection()
    {
        for text in [
            "Bounded preview only • never Confirm",
            "Bounded 界e\u{301}\tpreview only",
        ] {
            let mut buffer = TextBuffer::new(text).unwrap();
            buffer.anchor = Some(0);
            let cursor = buffer.cursor;
            let selection = buffer.selection();
            buffer.ensure_visible(12, 1);
            assert!(buffer.left_column > 0);
            assert!(!display_line(text, buffer.left_column, 12).starts_with("Bounded"));
            buffer.ensure_visible(94, 1);
            assert_eq!(buffer.left_column, 0);
            assert_eq!(
                display_line(text, buffer.left_column, 94),
                text.replace('\t', "    ")
            );
            assert_eq!(buffer.text, text);
            assert_eq!(buffer.cursor, cursor);
            assert_eq!(buffer.selection(), selection);
            assert_eq!(buffer.top_line, 0);
            buffer.ensure_visible(12, 1);
            let manual_left = buffer.left_column;
            buffer.scroll(0);
            buffer.ensure_visible(94, 1);
            assert_eq!(
                buffer.left_column, manual_left,
                "explicit manual scroll remains authoritative"
            );
        }
    }

    #[test]
    fn composer_keyboard_and_mouse_use_identical_wide_cell_coordinates() {
        let mut buffer = TextBuffer::new("ab界d\nxy\nab界d").unwrap();
        buffer.line_edge(false, true, false);
        buffer.horizontal(true, false);
        buffer.horizontal(true, false);
        buffer.horizontal(true, false);
        assert_eq!(buffer.position(), (0, 4));
        buffer.vertical(true, false);
        assert_eq!(buffer.position(), (1, 2));
        buffer.vertical(true, false);
        assert_eq!(buffer.position(), (2, 4));
        buffer.click(0, 2, false);
        assert_eq!(buffer.text[buffer.cursor..].chars().next(), Some('界'));
        buffer.click(0, 4, true);
        assert_eq!(buffer.selected_text(), "界");
        buffer.ensure_visible(3, 1);
        assert_eq!(buffer.left_column, 2);
        assert_eq!(display_line("ab界d", 2, 3), "界d");
    }
}

#[cfg(test)]
#[path = "editor_viewport_tests.rs"]
mod viewport_tests;
