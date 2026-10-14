//! Shared typed key/mouse remote browse controls; no transport, SQL, parse or
//! full source clone during draw. Actual original index is retained in row hit.
use super::*;
use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{
    layout::{Position, Rect},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

pub const REMOTE_REPOSITORY_TITLE_V1: &str = "Remote repository • saved versions, no checkout";
impl RemoteRepositoryWorkspaceV1 {
    pub fn key(&self, key: KeyEvent) -> Option<RemoteBrowseActionV1> {
        if key.kind == KeyEventKind::Release {
            return None;
        }
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        if self.focus == 2 {
            match key.code {
                KeyCode::Enter => {
                    return [
                        RemoteBrowseActionV1::Open,
                        RemoteBrowseActionV1::Up,
                        RemoteBrowseActionV1::Refresh,
                        RemoteBrowseActionV1::Branches,
                        RemoteBrowseActionV1::Tags,
                        RemoteBrowseActionV1::CommitFocus,
                        RemoteBrowseActionV1::MappedLocal,
                        RemoteBrowseActionV1::Details,
                        RemoteBrowseActionV1::Back,
                    ]
                    .get(self.toolbar)
                    .cloned()
                }
                KeyCode::Left => return Some(RemoteBrowseActionV1::FocusNext(true)),
                KeyCode::Right => return Some(RemoteBrowseActionV1::FocusNext(false)),
                _ => {}
            }
        }
        if self.focus == 1 {
            return Some(match key.code {
                KeyCode::Esc => RemoteBrowseActionV1::Back,
                KeyCode::Tab => RemoteBrowseActionV1::FocusNext(false),
                KeyCode::BackTab => RemoteBrowseActionV1::FocusNext(true),
                KeyCode::Enter => RemoteBrowseActionV1::Commit,
                KeyCode::Char('u') if control => RemoteBrowseActionV1::CommitClear,
                KeyCode::Char('a') if control => RemoteBrowseActionV1::CommitSelectAll,
                KeyCode::Char(c) if !control => RemoteBrowseActionV1::CommitInsert(c.to_string()),
                KeyCode::Backspace => RemoteBrowseActionV1::CommitBackspace,
                KeyCode::Left => RemoteBrowseActionV1::CommitLeft,
                KeyCode::Right => RemoteBrowseActionV1::CommitRight,
                _ => return None,
            });
        }
        Some(match key.code {
            KeyCode::Esc | KeyCode::Char('b') => RemoteBrowseActionV1::Back,
            KeyCode::Enter => RemoteBrowseActionV1::Open,
            KeyCode::Up => RemoteBrowseActionV1::Move(-1),
            KeyCode::Down => RemoteBrowseActionV1::Move(1),
            KeyCode::Home => RemoteBrowseActionV1::Row(0),
            KeyCode::End => RemoteBrowseActionV1::Row(self.rows.len().saturating_sub(1)),
            KeyCode::PageUp => RemoteBrowseActionV1::Page(-10),
            KeyCode::PageDown => RemoteBrowseActionV1::Page(10),
            KeyCode::Char('u') => RemoteBrowseActionV1::Up,
            KeyCode::Char('r') => RemoteBrowseActionV1::Refresh,
            KeyCode::Char('B') => RemoteBrowseActionV1::Branches,
            KeyCode::Char('T') => RemoteBrowseActionV1::Tags,
            KeyCode::Char('C') => RemoteBrowseActionV1::CommitFocus,
            KeyCode::Char('L') => RemoteBrowseActionV1::MappedLocal,
            KeyCode::Char('D') => RemoteBrowseActionV1::Details,
            KeyCode::Tab => RemoteBrowseActionV1::FocusNext(false),
            KeyCode::BackTab => RemoteBrowseActionV1::FocusNext(true),
            _ => return None,
        })
    }
    pub fn mouse(&self, event: MouseEvent) -> Option<RemoteBrowseActionV1> {
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => self.hits.iter().rev().find_map(|(r, a)| {
                r.contains(Position::new(event.column, event.row))
                    .then(|| a.clone())
            }),
            MouseEventKind::ScrollDown
                if self
                    .list_area
                    .contains(Position::new(event.column, event.row)) =>
            {
                Some(RemoteBrowseActionV1::Move(3))
            }
            MouseEventKind::ScrollUp
                if self
                    .list_area
                    .contains(Position::new(event.column, event.row)) =>
            {
                Some(RemoteBrowseActionV1::Move(-3))
            }
            _ => None,
        }
    }
    pub fn draw(&mut self, frame: &mut Frame, area: Rect, theme: crate::theme::Theme) {
        self.clear_layout();
        frame.render_widget(Clear, area);
        let shell = Block::default()
            .borders(Borders::ALL)
            .title(REMOTE_REPOSITORY_TITLE_V1)
            .style(theme.base());
        let inner = shell.inner(area);
        frame.render_widget(shell, area);
        if inner.height < 4 {
            return;
        }
        let header_rows = (self.header.len() as u16)
            .min(5)
            .min(inner.height.saturating_sub(4));
        for (i, line) in self.header.iter().take(header_rows as usize).enumerate() {
            frame.render_widget(
                Paragraph::new(line.as_str()).style(theme.style(SemanticToken::Reference)),
                Rect::new(inner.x, inner.y + i as u16, inner.width, 1),
            );
        }
        let controls = 3.min(inner.height.saturating_sub(header_rows));
        self.list_area = Rect::new(
            inner.x,
            inner.y + header_rows,
            inner.width,
            inner.height.saturating_sub(header_rows + controls),
        );
        let visible = self.list_area.height as usize;
        self.scroll = self.scroll.min(self.selected);
        if self.selected >= self.scroll + visible {
            self.scroll = self.selected.saturating_sub(visible.saturating_sub(1));
        }
        if self.rows.is_empty() {
            frame.render_widget(Paragraph::new("No observed rows • not an unavailable source or verified absence outside coverage"),self.list_area);
        }
        for (offset, row) in self.rows.iter().skip(self.scroll).take(visible).enumerate() {
            let index = self.scroll + offset;
            let rect = Rect::new(
                self.list_area.x,
                self.list_area.y + offset as u16,
                self.list_area.width,
                1,
            );
            frame.render_widget(
                Paragraph::new(format!(
                    "{} {}",
                    if index == self.selected { ">" } else { " " },
                    row.label
                ))
                .style(if index == self.selected {
                    theme.selected(row.role)
                } else {
                    theme.style(row.role)
                }),
                rect,
            );
            self.hits.push((rect, RemoteBrowseActionV1::Row(index)));
        }
        let y = inner.bottom().saturating_sub(controls);
        // Controls stay visible at40columns, share exact action IDs and do not
        // reuse local mutation controls. Full effect is in static Help/status.
        let bars: [&[(&str, RemoteBrowseActionV1)]; 3] = [
            &[
                ("[Enter Open]", RemoteBrowseActionV1::Open),
                ("[u Up]", RemoteBrowseActionV1::Up),
                ("[r Refresh]", RemoteBrowseActionV1::Refresh),
            ],
            &[
                ("[B Branches]", RemoteBrowseActionV1::Branches),
                ("[T Tags]", RemoteBrowseActionV1::Tags),
                ("[C Commit]", RemoteBrowseActionV1::CommitFocus),
            ],
            &[
                ("[L Local]", RemoteBrowseActionV1::MappedLocal),
                ("[D Details]", RemoteBrowseActionV1::Details),
                ("[Esc Back]", RemoteBrowseActionV1::Back),
            ],
        ];
        let mut ordinal = 0;
        for (row, bar) in bars.iter().take(controls as usize).enumerate() {
            let mut x = inner.x;
            for (label, action) in *bar {
                let width = (label.len() as u16).min(inner.right().saturating_sub(x));
                if width == 0 {
                    break;
                }
                let rect = Rect::new(x, y + row as u16, width, 1);
                frame.render_widget(
                    Paragraph::new(*label).style(if self.focus == 2 && self.toolbar == ordinal {
                        theme.selected(SemanticToken::Focus)
                    } else {
                        theme.style(SemanticToken::Focus)
                    }),
                    rect,
                );
                self.hits.push((rect, action.clone()));
                ordinal += 1;
                x = x.saturating_add(width + 1);
            }
        }
        // Exact commit editor is a separate owned modal in native coordinator,
        // not a hidden field underneath these controls.
    }
}
