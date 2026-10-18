//! Source-only prepared original-tree picker, no I/O/matching/body cloning.
use crate::{
    editor::{display_line, display_width},
    repository_tree_jump_v1::{
        OriginalTreeFocusV1 as Focus, OriginalTreeJumpActionV1 as Action, OriginalTreeJumpV1,
    },
    theme::{SemanticToken, Theme},
};
use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    layout::Rect,
    style::Modifier,
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

#[derive(Clone, Debug)]
pub struct OriginalTreeHitV1 {
    pub rect: Rect,
    pub action: Action,
}
/// Layout-only controls over worker-prepared exact directory breadcrumbs.
/// The caller reserves returned rows BEFORE repository body/hit offsets.
pub fn draw_breadcrumbs(
    frame: &mut Frame,
    projection: &crate::workspace::PreparedOriginalTreeNavigationV1,
    hits: &mut Vec<crate::app::HitRegion>,
    area: Rect,
    theme: Theme,
    focus: usize,
) -> u16 {
    let mut x = area.x;
    let mut row = 0u16;
    let mut displayed = 0;
    for crumb in projection.breadcrumbs() {
        let width = crumb.width().min(area.width as usize) as u16;
        if x != area.x && x.saturating_add(width) > area.right() {
            x = area.x;
            row += 1;
        }
        if row >= area.height || width == 0 {
            break;
        }
        let rect = Rect::new(x, area.y + row, width, 1);
        let mut style = theme.style(SemanticToken::Reference);
        if hits.len() == focus {
            style = style.add_modifier(Modifier::UNDERLINED);
        }
        frame.render_widget(Paragraph::new(crumb.label()).style(style), rect);
        hits.push(crate::app::HitRegion {
            rect,
            action: crate::app::Action::RepositoryRow(crate::workspace::RowAction::Tree(
                crumb.action().clone(),
            )),
        });
        displayed += 1;
        x = x.saturating_add(width + 1);
    }
    if displayed < projection.breadcrumbs().len() && row + 1 < area.height {
        row += 1;
        frame.render_widget(
            Paragraph::new("More original folders: use Jump; clipped names retain raw targets")
                .style(theme.style(SemanticToken::Muted)),
            Rect::new(area.x, area.y + row, area.width, 1),
        );
    }
    if displayed == 0 {
        0
    } else {
        (row + 1).min(area.height)
    }
}
#[derive(Default)]
pub struct OriginalTreeLayoutV1 {
    hits: Vec<OriginalTreeHitV1>,
    area: Rect,
    list: Rect,
    detail: Rect,
    query: Rect,
}
impl OriginalTreeLayoutV1 {
    pub fn clear(&mut self) {
        self.hits.clear();
        self.area = Rect::default();
        self.list = Rect::default();
        self.detail = Rect::default();
        self.query = Rect::default();
    }
    pub fn hits(&self) -> &[OriginalTreeHitV1] {
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
        state: &OriginalTreeJumpV1,
        code: KeyCode,
        mods: KeyModifiers,
    ) -> Option<Action> {
        let control = mods.contains(KeyModifiers::CONTROL);
        let shift = mods.contains(KeyModifiers::SHIFT);
        if code == KeyCode::Esc {
            return Some(Action::Close);
        }
        if code == KeyCode::Tab {
            return Some(Action::FocusNext(false));
        }
        if code == KeyCode::BackTab {
            return Some(Action::FocusNext(true));
        }
        if state.focus == Focus::Query {
            return match code {
                KeyCode::Char('a') if control => Some(Action::SelectAll),
                KeyCode::Char('u') if control => Some(Action::Clear),
                KeyCode::Char(c) if !control && !mods.contains(KeyModifiers::ALT) => {
                    Some(Action::Insert(c.to_string()))
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
                KeyCode::Enter | KeyCode::Down => Some(Action::FocusList),
                _ => None,
            };
        }
        if code == KeyCode::Enter {
            return Some(match state.focus {
                Focus::Previous => Action::Page(-1),
                Focus::Next => Action::Page(1),
                Focus::Close => Action::Close,
                Focus::Detail => Action::FocusDetail,
                _ => Action::Choose,
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
            KeyCode::Char('/') => Some(Action::FocusQuery),
            KeyCode::Char('P') => Some(Action::Page(-1)),
            KeyCode::Char('N') => Some(Action::Page(1)),
            _ => None,
        }
    }
    pub fn draw(
        &mut self,
        frame: &mut Frame,
        state: &mut OriginalTreeJumpV1,
        area: Rect,
        theme: Theme,
    ) {
        self.clear();
        self.area = area;
        frame.render_widget(Clear, area);
        frame.render_widget(
            Block::default()
                .borders(Borders::ALL)
                .border_type(ratatui::widgets::BorderType::Rounded)
                .title("Jump to saved path • original source")
                .style(theme.base()),
            area,
        );
        let inner = area.inner(ratatui::layout::Margin::new(1, 1));
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
                theme.style(SemanticToken::Focus)
            } else {
                theme.style(SemanticToken::Reference)
            }),
            close,
        );
        self.hits.push(OriginalTreeHitV1 {
            rect: close,
            action: Action::Close,
        });
        if inner.height < 8 || inner.width < 30 {
            return;
        }
        frame.render_widget(
            Paragraph::new(format!("Saved: {}", state.intent_sha_v1()))
                .style(theme.style(SemanticToken::Reference)),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
        frame.render_widget(
            Paragraph::new(
                "Full-path fuzzy navigation • local saved objects • no checkout/network",
            )
            .style(theme.style(SemanticToken::Muted)),
            Rect::new(inner.x, inner.y + 1, inner.width, 1),
        );
        frame.render_widget(
            Paragraph::new(state.status.as_str())
                .wrap(ratatui::widgets::Wrap { trim: false })
                .style(theme.style(SemanticToken::Reference)),
            Rect::new(inner.x, inner.y + 2, inner.width, 2),
        );
        frame.render_widget(
            Paragraph::new("Filter:").style(if state.focus == Focus::Query {
                theme
                    .style(SemanticToken::Focus)
                    .add_modifier(Modifier::UNDERLINED)
            } else {
                theme.base()
            }),
            Rect::new(inner.x, inner.y + 4, 8, 1),
        );
        self.query = Rect::new(inner.x + 8, inner.y + 4, inner.width.saturating_sub(8), 1);
        self.hits.push(OriginalTreeHitV1 {
            rect: Rect::new(inner.x, inner.y + 4, 8, 1),
            action: Action::FocusQuery,
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
        let content = Rect::new(
            inner.x,
            inner.y + 5,
            inner.width,
            inner.height.saturating_sub(6),
        );
        if inner.width >= 100 {
            let left = inner.width.saturating_sub(78).min(40);
            self.list = Rect::new(content.x, content.y, left, content.height);
            self.detail = Rect::new(
                content.x + left,
                content.y,
                inner.width - left,
                content.height,
            );
        } else {
            let list = content.height.div_ceil(2);
            self.list = Rect::new(content.x, content.y, inner.width, list);
            self.detail = Rect::new(
                content.x,
                content.y + list,
                inner.width,
                content.height - list,
            );
        }
        if state.selected < state.list_scroll {
            state.list_scroll = state.selected;
        } else if state.selected >= state.list_scroll + self.list.height as usize {
            state.list_scroll = state.selected + 1 - self.list.height.max(1) as usize;
        }
        if let Some(source) = &state.frame {
            for (offset, row) in source
                .matches()
                .iter()
                .enumerate()
                .skip(state.list_scroll)
                .take(self.list.height as usize)
            {
                let rect = Rect::new(
                    self.list.x,
                    self.list.y + (offset - state.list_scroll) as u16,
                    self.list.width,
                    1,
                );
                frame.render_widget(
                    Paragraph::new(row.label()).style(if offset == state.selected {
                        theme.selected(SemanticToken::Reference)
                    } else {
                        theme.base()
                    }),
                    rect,
                );
                if let Some(ticket) = source.ticket(offset) {
                    self.hits.push(OriginalTreeHitV1 {
                        rect,
                        action: Action::Activate(ticket),
                    });
                }
            }
            if source.matches().is_empty() {
                frame.render_widget(
                    Paragraph::new(
                        "No known matching path; partial tree leaves unseen paths unknown",
                    )
                    .wrap(ratatui::widgets::Wrap { trim: false })
                    .style(theme.style(SemanticToken::Muted)),
                    self.list,
                );
            }
            if let Some(selected) = source.matches().get(state.selected) {
                for (visible, line) in selected
                    .detail()
                    .iter()
                    .skip(state.detail_scroll)
                    .take(self.detail.height as usize)
                    .enumerate()
                {
                    frame.render_widget(
                        Paragraph::new(line.as_str()).style(theme.style(SemanticToken::Reference)),
                        Rect::new(
                            self.detail.x,
                            self.detail.y + visible as u16,
                            self.detail.width,
                            1,
                        ),
                    );
                }
            }
        } else {
            frame.render_widget(
                Paragraph::new("Original source/filter pending • no old rows may be opened")
                    .wrap(ratatui::widgets::Wrap { trim: false })
                    .style(theme.style(SemanticToken::Muted)),
                self.list,
            );
        }
        self.hits.push(OriginalTreeHitV1 {
            rect: self.detail,
            action: Action::FocusDetail,
        });
        let mut x = inner.x;
        for (text, action, focus) in [
            ("[Enter Open]", Action::Choose, Focus::Choose),
            ("[P Previous100]", Action::Page(-1), Focus::Previous),
            ("[N Next100]", Action::Page(1), Focus::Next),
        ] {
            let width = (text.len() as u16).min(close.x.saturating_sub(x + 1));
            if width == 0 {
                break;
            }
            let rect = Rect::new(x, close.y, width, 1);
            frame.render_widget(
                Paragraph::new(text).style(if state.focus == focus {
                    theme
                        .style(SemanticToken::Focus)
                        .add_modifier(Modifier::UNDERLINED)
                } else {
                    theme.style(SemanticToken::Reference)
                }),
                rect,
            );
            self.hits.push(OriginalTreeHitV1 { rect, action });
            x += width + 1;
        }
    }
}

#[cfg(test)]
#[path = "repository_tree_jump_ui_v1_tests.rs"]
mod tests;
