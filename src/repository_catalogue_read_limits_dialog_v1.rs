//! Local limits dialog; fixed small editor/geometry work only.
//! No provider/SQL/source restoration occurs in this editor.
use crate::{
    editor::{display_line, TextBuffer},
    repository_catalogue_read_limits_v1::{
        CatalogueDiscoveryBudgetV1, DiscoveryLimitFieldV1, DISCOVERY_FIELD_BYTES,
        DISCOVERY_FIELD_COUNT,
    },
    theme::{SemanticToken, Theme},
};
use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{
    layout::{Position, Rect},
    style::Modifier,
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

pub const DISCOVERY_LIMITS_TITLE: &str = "Repository discovery limits";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiscoveryLimitsPurposeV1 {
    ApplyOnly,
    ReRead,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiscoveryLimitsActionV1 {
    Field(usize),
    EditorClick {
        field: usize,
        column: usize,
        extend: bool,
    },
    Preset(usize),
    Scroll(i32),
    Apply,
    ReRead,
    Back,
}

/// Small local draft, not a serialized budget/source proof. Parent routes the
/// returned Apply/ReRead/Back through ONE shared action identity for both inputs.
pub struct CatalogueDiscoveryLimitsDialogV1 {
    fields: [TextBuffer; DISCOVERY_FIELD_COUNT],
    focus: usize,
    purpose: DiscoveryLimitsPurposeV1,
    top: usize,
    view_rows: usize,
    manual_scroll: bool,
    hits: Vec<(Rect, DiscoveryLimitsActionV1)>,
    editors: [Rect; DISCOVERY_FIELD_COUNT],
    body: Rect,
    error: Option<&'static str>,
}
impl CatalogueDiscoveryLimitsDialogV1 {
    pub fn new(budget: CatalogueDiscoveryBudgetV1, purpose: DiscoveryLimitsPurposeV1) -> Self {
        Self {
            fields: budget
                .fields()
                .map(|text| TextBuffer::new(text).expect("small ASCII bounded numeric preset")),
            focus: 0,
            purpose,
            top: 0,
            view_rows: 1,
            manual_scroll: false,
            hits: vec![],
            editors: [Rect::default(); DISCOVERY_FIELD_COUNT],
            body: Rect::default(),
            error: None,
        }
    }
    pub fn purpose(&self) -> DiscoveryLimitsPurposeV1 {
        self.purpose
    }
    pub fn budget(&mut self) -> Result<CatalogueDiscoveryBudgetV1, &'static str> {
        let result = CatalogueDiscoveryBudgetV1::from_fields(std::array::from_fn(|i| {
            self.fields[i].text.as_str()
        }));
        self.error = result.as_ref().err().copied();
        result
    }
    pub fn clear_layout(&mut self) {
        self.hits.clear();
        self.editors.fill(Rect::default());
        self.body = Rect::default();
    }
    pub fn apply_local(&mut self, action: &DiscoveryLimitsActionV1) {
        match action {
            DiscoveryLimitsActionV1::Field(index) if *index < DISCOVERY_FIELD_COUNT => {
                self.focus = *index;
                self.manual_scroll = false;
            }
            DiscoveryLimitsActionV1::EditorClick {
                field,
                column,
                extend,
            } if *field < DISCOVERY_FIELD_COUNT => {
                self.focus = *field;
                self.manual_scroll = false;
                self.fields[*field].click(0, *column, *extend);
            }
            DiscoveryLimitsActionV1::Preset(index) => {
                match CatalogueDiscoveryBudgetV1::preset(*index) {
                    Ok(preset) => {
                        self.fields = preset
                            .fields()
                            .map(|text| TextBuffer::new(text).expect("fixed small preset"));
                        self.error = None;
                        self.focus = DISCOVERY_FIELD_COUNT + index;
                        self.manual_scroll = false;
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            DiscoveryLimitsActionV1::Scroll(delta) => {
                self.top = self.top.saturating_add_signed(*delta as isize).min(12);
                self.manual_scroll = true;
            }
            _ => {}
        }
        self.clear_layout();
    }
    fn edit(&mut self, text: &str) {
        let Some(field) = self.fields.get_mut(self.focus) else {
            return;
        };
        let replaced = field.selection().map_or(0, |range| range.len());
        if text.is_empty()
            || text.len() > DISCOVERY_FIELD_BYTES
            || !text.bytes().all(|byte| byte.is_ascii_digit())
            || field
                .text
                .len()
                .saturating_sub(replaced)
                .saturating_add(text.len())
                > DISCOVERY_FIELD_BYTES
        {
            self.error =
                Some("Numeric field input rejected before copy; use at most 6 ASCII digits");
            return;
        }
        if field.insert(text).is_err() {
            self.error = Some("Bounded field editor unavailable; no request sent");
        } else {
            self.error = None;
        }
        self.manual_scroll = false;
        self.clear_layout();
    }
    pub fn paste(&mut self, text: &str) {
        self.edit(text);
    }
    pub fn key(&mut self, event: KeyEvent) -> Option<DiscoveryLimitsActionV1> {
        if event.kind == KeyEventKind::Release {
            return None;
        }
        let control = event.modifiers.contains(KeyModifiers::CONTROL);
        let shift = event.modifiers.contains(KeyModifiers::SHIFT);
        let count = if self.purpose == DiscoveryLimitsPurposeV1::ReRead {
            11
        } else {
            10
        };
        match event.code {
            KeyCode::Esc => return Some(DiscoveryLimitsActionV1::Back),
            KeyCode::Tab | KeyCode::BackTab => {
                let backwards = shift || event.code == KeyCode::BackTab;
                self.focus = (self.focus + if backwards { count - 1 } else { 1 }) % count;
                self.manual_scroll = false;
            }
            KeyCode::PageUp => {
                return Some(DiscoveryLimitsActionV1::Scroll(-(self.view_rows as i32)))
            }
            KeyCode::PageDown => {
                return Some(DiscoveryLimitsActionV1::Scroll(self.view_rows as i32))
            }
            KeyCode::F(5) if self.purpose == DiscoveryLimitsPurposeV1::ReRead => {
                return Some(DiscoveryLimitsActionV1::ReRead);
            }
            KeyCode::Char(ch @ '1'..='3') if self.focus >= DISCOVERY_FIELD_COUNT && !control => {
                return Some(DiscoveryLimitsActionV1::Preset(ch as usize - '1' as usize));
            }
            KeyCode::Enter => {
                return Some(match self.focus {
                    0..=4 => {
                        self.focus = (self.focus + 1) % DISCOVERY_FIELD_COUNT;
                        self.manual_scroll = false;
                        self.clear_layout();
                        return None;
                    }
                    5..=7 => DiscoveryLimitsActionV1::Preset(self.focus - 5),
                    8 => DiscoveryLimitsActionV1::Apply,
                    9 if self.purpose == DiscoveryLimitsPurposeV1::ReRead => {
                        DiscoveryLimitsActionV1::ReRead
                    }
                    _ => DiscoveryLimitsActionV1::Back,
                });
            }
            _ if self.focus < DISCOVERY_FIELD_COUNT => {
                let field = &mut self.fields[self.focus];
                match event.code {
                    KeyCode::Char('a') if control => field.select_all(),
                    KeyCode::Char('u') if control => field.clear(),
                    KeyCode::Left => field.horizontal(false, shift),
                    KeyCode::Right => field.horizontal(true, shift),
                    KeyCode::Home => field.line_edge(false, true, shift),
                    KeyCode::End => field.line_edge(true, true, shift),
                    KeyCode::Backspace => field.delete_backward(),
                    KeyCode::Delete => field.delete_forward(),
                    KeyCode::Char(ch)
                        if !control && !event.modifiers.contains(KeyModifiers::ALT) =>
                    {
                        self.edit(&ch.to_string());
                        return None;
                    }
                    _ => return None,
                }
                self.error = None;
                self.manual_scroll = false;
            }
            _ => return None,
        }
        self.clear_layout();
        None
    }
    pub fn mouse(&self, event: MouseEvent) -> Option<DiscoveryLimitsActionV1> {
        let position = Position::new(event.column, event.row);
        if matches!(
            event.kind,
            MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left)
        ) {
            if let Some(field) = self.editors.iter().position(|rect| rect.contains(position)) {
                return Some(DiscoveryLimitsActionV1::EditorClick {
                    field,
                    column: usize::from(event.column - self.editors[field].x),
                    extend: event.modifiers.contains(KeyModifiers::SHIFT)
                        || event.kind == MouseEventKind::Drag(MouseButton::Left),
                });
            }
            if event.kind == MouseEventKind::Down(MouseButton::Left) {
                return self
                    .hits
                    .iter()
                    .find(|(rect, _)| rect.contains(position))
                    .map(|(_, action)| action.clone());
            }
            return None;
        }
        if self.body.contains(position) {
            return match event.kind {
                MouseEventKind::ScrollUp => Some(DiscoveryLimitsActionV1::Scroll(-3)),
                MouseEventKind::ScrollDown => Some(DiscoveryLimitsActionV1::Scroll(3)),
                _ => None,
            };
        }
        None
    }
    fn button(
        &mut self,
        frame: &mut Frame,
        rect: Rect,
        label: &str,
        action: DiscoveryLimitsActionV1,
        active: bool,
        theme: Theme,
    ) {
        if rect.is_empty() {
            return;
        }
        frame.render_widget(
            Paragraph::new(label).style(
                theme
                    .style(if active {
                        SemanticToken::Focus
                    } else {
                        SemanticToken::Normal
                    })
                    .add_modifier(if active {
                        Modifier::UNDERLINED
                    } else {
                        Modifier::empty()
                    }),
            ),
            rect,
        );
        self.hits.push((rect, action));
    }
    pub fn draw(&mut self, frame: &mut Frame, area: Rect, theme: Theme) {
        self.clear_layout();
        frame.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .title(DISCOVERY_LIMITS_TITLE)
            .style(theme.base());
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if inner.is_empty() {
            return;
        }
        let mut controls = vec![(
            "[Enter Apply]",
            DiscoveryLimitsActionV1::Apply,
            self.focus == 8,
        )];
        if self.purpose == DiscoveryLimitsPurposeV1::ReRead {
            controls.push((
                "[F5 Re-read]",
                DiscoveryLimitsActionV1::ReRead,
                self.focus == 9,
            ));
        }
        controls.extend([
            (
                "[Esc Back]",
                DiscoveryLimitsActionV1::Back,
                self.focus
                    == if self.purpose == DiscoveryLimitsPurposeV1::ReRead {
                        10
                    } else {
                        9
                    },
            ),
            (
                "[PgUp]",
                DiscoveryLimitsActionV1::Scroll(-(self.view_rows as i32)),
                false,
            ),
            (
                "[PgDn]",
                DiscoveryLimitsActionV1::Scroll(self.view_rows as i32),
                false,
            ),
        ]);
        let mut footer_rows = 1u16;
        let mut occupied = 0u16;
        for (label, _, _) in &controls {
            let width = (label.len() as u16).min(inner.width);
            if occupied != 0 && occupied.saturating_add(width) > inner.width {
                footer_rows += 1;
                occupied = 0;
            }
            occupied = occupied.saturating_add(width + 1);
        }
        footer_rows = footer_rows.min(inner.height);
        let error_rows = u16::from(self.error.is_some() && inner.height > footer_rows + 1);
        self.body = Rect::new(
            inner.x,
            inner.y,
            inner.width,
            inner.height.saturating_sub(footer_rows + error_rows),
        );
        self.view_rows = usize::from(self.body.height).max(1);
        let total = 13usize;
        self.top = self.top.min(total.saturating_sub(self.view_rows));
        if !self.manual_scroll && self.focus < 8 {
            let row = if self.focus < 5 {
                3 + self.focus
            } else {
                8 + self.focus - 5
            };
            if row < self.top {
                self.top = row;
            } else if row >= self.top + self.view_rows {
                self.top = row + 1 - self.view_rows;
            }
        }
        for offset in 0..usize::from(self.body.height) {
            let logical = self.top + offset;
            if logical >= total {
                break;
            }
            let rect = Rect::new(self.body.x, self.body.y + offset as u16, self.body.width, 1);
            match logical {
                3..=7 => {
                    let index = logical - 3;
                    let label = DiscoveryLimitFieldV1::ALL[index].label();
                    let caption = (label.len() as u16).min(rect.width);
                    frame.render_widget(
                        Paragraph::new(label).style(theme.style(if self.focus == index {
                            SemanticToken::Focus
                        } else {
                            SemanticToken::Normal
                        })),
                        Rect::new(rect.x, rect.y, caption, 1),
                    );
                    let editor = Rect::new(rect.x + caption, rect.y, rect.width - caption, 1);
                    self.editors[index] = editor;
                    self.fields[index].ensure_visible(usize::from(editor.width), 1);
                    frame.render_widget(
                        Paragraph::new(display_line(
                            &self.fields[index].text,
                            self.fields[index].left_column,
                            usize::from(editor.width),
                        )),
                        editor,
                    );
                    self.hits
                        .push((rect, DiscoveryLimitsActionV1::Field(index)));
                    if self.focus == index && editor.width > 0 {
                        let column = self.fields[index]
                            .position()
                            .1
                            .saturating_sub(self.fields[index].left_column)
                            .min(usize::from(editor.width - 1));
                        frame
                            .set_cursor_position(Position::new(editor.x + column as u16, editor.y));
                    }
                }
                8..=10 => {
                    let index = logical - 8;
                    let label = [
                        "[1 Standard: 20 pages / 2000 rows]",
                        "[2 Larger: 100 pages / 10000 rows]",
                        "[3 Maximum: 1000 pages / 100000 rows]",
                    ][index];
                    self.button(
                        frame,
                        rect,
                        label,
                        DiscoveryLimitsActionV1::Preset(index),
                        self.focus == 5 + index,
                        theme,
                    );
                }
                _ => {
                    let label = match logical {
                        0 => "Bounds only: Apply does not send a request",
                        1 => "Fixed page 100; each raw row at most 128 KiB",
                        2 => "Re-read starts at page 1; no stale row merge",
                        11 => "Saved sets/pins and unresolved intent retained",
                        _ => "No activation/clone; re-activate fresh source explicitly",
                    };
                    frame.render_widget(
                        Paragraph::new(label).style(theme.style(SemanticToken::Muted)),
                        rect,
                    );
                }
            }
        }
        if error_rows > 0 {
            frame.render_widget(
                Paragraph::new("Invalid limits • no request sent")
                    .style(theme.style(SemanticToken::Failure)),
                Rect::new(inner.x, self.body.bottom(), inner.width, 1),
            );
        }
        let mut x = inner.x;
        let mut y = inner.bottom().saturating_sub(footer_rows);
        for (label, mut action, active) in controls {
            let width = (label.len() as u16).min(inner.width);
            if x > inner.x && x.saturating_add(width) > inner.right() {
                x = inner.x;
                y += 1;
            }
            if y >= inner.bottom() {
                break;
            }
            if let DiscoveryLimitsActionV1::Scroll(delta) = &mut action {
                *delta = (self.view_rows as i32) * delta.signum();
            }
            self.button(
                frame,
                Rect::new(x, y, width, 1),
                label,
                action,
                active,
                theme,
            );
            x += width + 1;
        }
    }
}

#[cfg(test)]
#[path = "repository_catalogue_read_limits_dialog_v1_tests.rs"]
mod tests;
