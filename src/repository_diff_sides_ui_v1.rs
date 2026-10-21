//! Layout-only changed-file picker. No matcher/provider/SQL/read or body clone.
use crate::{
    editor::{display_line, display_width},
    git::OriginalComparisonSideV1 as Side,
    repository_diff_sides_v1::{
        OriginalDiffActionV1 as Action, OriginalDiffFocusV1 as Focus, OriginalDiffPickerV1,
    },
    theme::{SemanticToken, Theme},
};
use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::style::Modifier;
use ratatui::{
    layout::{Margin, Rect},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

#[derive(Clone, Debug)]
pub struct OriginalDiffHitV1 {
    pub rect: Rect,
    pub action: Action,
}
#[derive(Default)]
pub struct OriginalDiffLayoutV1 {
    hits: Vec<OriginalDiffHitV1>,
    list: Rect,
    detail: Rect,
    query: Rect,
}
impl OriginalDiffLayoutV1 {
    pub fn clear(&mut self) {
        self.hits.clear();
        self.list = Rect::default();
        self.detail = Rect::default();
        self.query = Rect::default();
    }
    pub fn hits(&self) -> &[OriginalDiffHitV1] {
        &self.hits
    }
    pub fn mouse(&self, kind: MouseEventKind, column: u16, row: u16) -> Option<Action> {
        let point = (column, row).into();
        match kind {
            MouseEventKind::Down(MouseButton::Left) if self.query.contains(point) => Some(
                Action::QueryClick(column.saturating_sub(self.query.x) as usize),
            ),
            MouseEventKind::Down(MouseButton::Left) => self
                .hits
                .iter()
                .rev()
                .find(|hit| hit.rect.contains(point))
                .map(|hit| hit.action.clone()),
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp
                if self.detail.contains(point) =>
            {
                Some(Action::DetailScroll(
                    if kind == MouseEventKind::ScrollDown {
                        1
                    } else {
                        -1
                    },
                ))
            }
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp if self.list.contains(point) => {
                Some(Action::Move(if kind == MouseEventKind::ScrollDown {
                    1
                } else {
                    -1
                }))
            }
            _ => None,
        }
    }
    pub fn key(
        &self,
        state: &OriginalDiffPickerV1,
        code: KeyCode,
        mods: KeyModifiers,
    ) -> Option<Action> {
        let control = mods.contains(KeyModifiers::CONTROL);
        let shift = mods.contains(KeyModifiers::SHIFT);
        if code == KeyCode::Esc {
            return Some(Action::Close);
        }
        if control && code == KeyCode::Char('c') {
            return Some(Action::Quit);
        }
        if code == KeyCode::Tab {
            return Some(Action::FocusNext(false));
        }
        if code == KeyCode::BackTab {
            return Some(Action::FocusNext(true));
        }
        if state.focus != Focus::Query && code == KeyCode::Char('q') {
            return Some(Action::Quit);
        }
        if state.focus == Focus::Query {
            return match code {
                KeyCode::Char('a') if control => Some(Action::SelectAll),
                KeyCode::Char('u') if control => Some(Action::Clear),
                KeyCode::Char(value) if !control && !mods.contains(KeyModifiers::ALT) => {
                    Some(Action::Insert(value.to_string()))
                }
                KeyCode::Backspace => Some(Action::Backspace),
                KeyCode::Delete => Some(Action::Delete),
                KeyCode::Left | KeyCode::Right => Some(Action::Cursor {
                    right: code == KeyCode::Right,
                    select: shift,
                }),
                KeyCode::Home | KeyCode::End => Some(Action::Edge {
                    end: code == KeyCode::End,
                    select: shift,
                }),
                KeyCode::Enter | KeyCode::Down => Some(Action::Focus(Focus::List)),
                _ => None,
            };
        }
        if code == KeyCode::Enter {
            return Some(match state.focus {
                Focus::Old => Action::OpenSide(Side::Old),
                Focus::New | Focus::List => Action::OpenSide(Side::New),
                Focus::Previous => Action::Page(-1),
                Focus::Next => Action::Page(1),
                Focus::Close => Action::Close,
                Focus::Detail => Action::Focus(Focus::Detail),
                Focus::Query => Action::Focus(Focus::List),
            });
        }
        if state.focus == Focus::Detail {
            return match code {
                KeyCode::Up | KeyCode::Char('k') => Some(Action::DetailScroll(-1)),
                KeyCode::Down | KeyCode::Char('j') => Some(Action::DetailScroll(1)),
                KeyCode::PageUp => {
                    Some(Action::DetailScroll(-i32::from(self.detail.height.max(1))))
                }
                KeyCode::PageDown => {
                    Some(Action::DetailScroll(i32::from(self.detail.height.max(1))))
                }
                KeyCode::Home => Some(Action::DetailScroll(i32::MIN)),
                KeyCode::End => Some(Action::DetailScroll(i32::MAX)),
                KeyCode::Char('/') => Some(Action::Focus(Focus::Query)),
                KeyCode::Char('O') => Some(Action::OpenSide(Side::Old)),
                KeyCode::Char('N') => Some(Action::OpenSide(Side::New)),
                _ => None,
            };
        }
        match code {
            KeyCode::Up | KeyCode::Char('k') => Some(Action::Move(-1)),
            KeyCode::Down | KeyCode::Char('j') => Some(Action::Move(1)),
            KeyCode::PageUp => Some(Action::Move(-i32::from(self.list.height.max(1)))),
            KeyCode::PageDown => Some(Action::Move(i32::from(self.list.height.max(1)))),
            KeyCode::Home => Some(Action::First),
            KeyCode::End => Some(Action::Last),
            KeyCode::Char('/') => Some(Action::Focus(Focus::Query)),
            KeyCode::Char('O') => Some(Action::OpenSide(Side::Old)),
            KeyCode::Char('N') => Some(Action::OpenSide(Side::New)),
            KeyCode::Char('[') => Some(Action::Page(-1)),
            KeyCode::Char(']') => Some(Action::Page(1)),
            _ => None,
        }
    }
    pub fn draw(
        &mut self,
        frame: &mut Frame,
        state: &mut OriginalDiffPickerV1,
        area: Rect,
        theme: Theme,
    ) {
        self.clear();
        frame.render_widget(Clear, area);
        frame.render_widget(
            Block::default()
                .borders(Borders::ALL)
                .border_type(ratatui::widgets::BorderType::Rounded)
                .title("Compare saved file sides • original comparison")
                .style(theme.base()),
            area,
        );
        let inner = area.inner(Margin::new(1, 1));
        if inner.is_empty() {
            return;
        }
        let close = Rect::new(
            inner.right().saturating_sub(11).max(inner.x),
            inner.bottom().saturating_sub(1),
            inner.width.min(11),
            1,
        );
        frame.render_widget(
            Paragraph::new("[Esc Close]").style(if state.focus == Focus::Close {
                theme
                    .style(SemanticToken::Reference)
                    .add_modifier(Modifier::UNDERLINED)
            } else {
                theme.style(SemanticToken::Reference)
            }),
            close,
        );
        self.hits.push(OriginalDiffHitV1 {
            rect: close,
            action: Action::Close,
        });
        if inner.height < 10 || inner.width < 35 {
            return;
        }
        frame.render_widget(
            Paragraph::new(format!("Old: {}", state.intent().content_base()))
                .style(theme.style(SemanticToken::Removed)),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
        frame.render_widget(
            Paragraph::new(format!("New: {}", state.intent().head()))
                .style(theme.style(SemanticToken::Added)),
            Rect::new(inner.x, inner.y + 1, inner.width, 1),
        );
        frame.render_widget(
            Paragraph::new(state.status.as_str())
                .wrap(ratatui::widgets::Wrap { trim: false })
                .style(theme.style(SemanticToken::Reference)),
            Rect::new(inner.x, inner.y + 2, inner.width, 2),
        );
        let caption = Rect::new(inner.x, inner.y + 4, 8, 1);
        self.query = Rect::new(inner.x + 8, inner.y + 4, inner.width - 8, 1);
        frame.render_widget(
            Paragraph::new("Filter:").style(if state.focus == Focus::Query {
                theme.style(SemanticToken::Focus)
            } else {
                theme.base()
            }),
            caption,
        );
        self.hits.push(OriginalDiffHitV1 {
            rect: caption,
            action: Action::Focus(Focus::Query),
        });
        state.query.ensure_visible(self.query.width as usize, 1);
        frame.render_widget(
            Paragraph::new(display_line(
                &state.query.text,
                state.query.left_column,
                self.query.width as usize,
            ))
            .style(theme.base()),
            self.query,
        );
        if state.focus == Focus::Query {
            let caret = display_width(&state.query.text[..state.query.cursor])
                .saturating_sub(state.query.left_column)
                .min(self.query.width.saturating_sub(1) as usize);
            frame.set_cursor_position((self.query.x + caret as u16, self.query.y));
        }
        let content = Rect::new(inner.x, inner.y + 5, inner.width, inner.height - 6);
        if inner.width >= 100 {
            let left = inner.width - 74;
            self.list = Rect::new(content.x, content.y, left, content.height);
            self.detail = Rect::new(content.x + left, content.y, 74, content.height);
        } else {
            let list = content.height.min(5);
            self.list = Rect::new(content.x, content.y, content.width, list);
            self.detail = Rect::new(
                content.x,
                content.y + list,
                content.width,
                content.height - list,
            );
        }
        if state.selected < state.list_scroll {
            state.list_scroll = state.selected;
        } else if state.selected >= state.list_scroll + self.list.height as usize {
            state.list_scroll = state.selected + 1 - self.list.height.max(1) as usize;
        }
        let mut finder = Some(state.query.text.trim())
            .filter(|query| !query.is_empty())
            .map(crate::fuzzy_finder::FuzzyQueryV1::new);
        let mark = crate::fuzzy_finder::mark_style_v1(theme);
        if let Some(source) = state.frame() {
            for visible in
                state.list_scroll..(state.list_scroll + self.list.height as usize).min(source.len())
            {
                let Some(member) = source.member(visible) else {
                    continue;
                };
                let rect = Rect::new(
                    self.list.x,
                    self.list.y + (visible - state.list_scroll) as u16,
                    self.list.width,
                    1,
                );
                let style = if visible == state.selected {
                    theme.selected(SemanticToken::Reference)
                } else {
                    theme.base()
                };
                // Matched characters carry the shared finder's highlight (TASK-122).
                let positions = finder
                    .as_mut()
                    .and_then(|finder| finder.positions(member.label()))
                    .unwrap_or_default();
                frame.render_widget(
                    Paragraph::new(ratatui::text::Line::from(
                        crate::fuzzy_finder::highlight_spans_v1(
                            member.label(),
                            &positions,
                            0,
                            usize::from(rect.width),
                            style,
                            mark,
                        ),
                    ))
                    .style(style),
                    rect,
                );
                if let Some(ticket) = source.ticket(visible, Side::New) {
                    self.hits.push(OriginalDiffHitV1 {
                        rect,
                        action: Action::Select(ticket),
                    });
                }
            }
            if source.is_empty() {
                frame.render_widget(
                    Paragraph::new(
                        "No known matching changed file; unseen/partial members remain unknown",
                    )
                    .wrap(ratatui::widgets::Wrap { trim: false })
                    .style(theme.style(SemanticToken::Muted)),
                    self.list,
                );
            }
            if let Some(member) = source.member(state.selected) {
                for (offset, line) in member
                    .detail()
                    .iter()
                    .skip(state.detail_scroll)
                    .take(self.detail.height as usize)
                    .enumerate()
                {
                    frame.render_widget(
                        Paragraph::new(line.as_str()).style(if state.focus == Focus::Detail {
                            theme
                                .style(SemanticToken::Reference)
                                .add_modifier(Modifier::UNDERLINED)
                        } else {
                            theme.style(SemanticToken::Reference)
                        }),
                        Rect::new(
                            self.detail.x,
                            self.detail.y + offset as u16,
                            self.detail.width,
                            1,
                        ),
                    );
                }
            }
        }
        self.hits.push(OriginalDiffHitV1 {
            rect: self.detail,
            action: Action::Focus(Focus::Detail),
        });
        let controls = [
            ("[O Open old]", Action::OpenSide(Side::Old)),
            ("[N Open new]", Action::OpenSide(Side::New)),
            ("[‹ Page]", Action::Page(-1)),
            ("[Page ›]", Action::Page(1)),
            ("[q Quit]", Action::Quit),
        ];
        let mut x = inner.x;
        for (label, action) in controls {
            let width = display_width(label) as u16;
            if x + width > close.x {
                break;
            }
            let rect = Rect::new(x, close.y, width, 1);
            let focused = match &action {
                Action::OpenSide(Side::Old) => state.focus == Focus::Old,
                Action::OpenSide(Side::New) => state.focus == Focus::New,
                Action::Page(-1) => state.focus == Focus::Previous,
                Action::Page(1) => state.focus == Focus::Next,
                _ => false,
            };
            frame.render_widget(
                Paragraph::new(label).style(if focused {
                    theme
                        .style(SemanticToken::Reference)
                        .add_modifier(Modifier::UNDERLINED)
                } else {
                    theme.style(SemanticToken::Reference)
                }),
                rect,
            );
            self.hits.push(OriginalDiffHitV1 { rect, action });
            x += width + 1;
        }
    }
}

#[cfg(test)]
#[path = "repository_diff_sides_ui_v1_tests.rs"]
mod tests;
