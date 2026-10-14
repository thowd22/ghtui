//! Read-only display of an immutable, worker-prepared source explanation.
//! Opening, input and draw perform no Git, gh, SQL or source resolution.
use crate::{
    app::{Action, App, HitRegion, Overlay, View},
    theme::SemanticToken,
};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    layout::{Margin, Rect},
    style::Modifier,
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use std::sync::Arc;

pub fn open(app: &mut App) {
    if app.view != View::Repository
        || app.loading[View::Repository.index()]
        || app.repository_load_requested
        || app.repository_action.is_some()
    {
        app.dispatch(Action::Explain(
            "Wait for the exact local repository read before viewing its source identity".into(),
        ));
        return;
    }
    let Some(source) = app.repository_source_identity.as_ref().map(Arc::clone) else {
        app.dispatch(Action::Explain("Repository source identity is unavailable for this prepared read; no saved version or actor basis is inferred".into()));
        return;
    };
    app.repository_source_overlay = Some(source);
    app.repository_source_scroll = 0;
    app.repository_source_area = Rect::default();
    app.repository_source_return_focus = Some((app.focus, app.repository_content_focused));
    app.hits.clear();
    app.focus = 0;
    app.overlay = Some(Overlay::RepositorySource);
}
/// Source replacement retires old details instead of silently rebinding them.
pub fn retire(app: &mut App) {
    app.repository_source_overlay = None;
    app.repository_source_return_focus = None;
    app.repository_source_area = Rect::default();
    app.repository_source_scroll = 0;
    if app.overlay == Some(Overlay::RepositorySource) {
        app.overlay = None;
    }
    app.hits.clear();
}
pub fn close(app: &mut App) {
    let focus = app.repository_source_return_focus.take();
    retire(app);
    if let Some((focus, content)) = focus {
        app.focus = focus;
        app.repository_content_focused = content;
    }
}
fn same_owner(app: &App) -> bool {
    app.view == View::Repository
        && app
            .repository_source_overlay
            .as_ref()
            .zip(app.repository_source_identity.as_ref())
            .is_some_and(|(open, current)| Arc::ptr_eq(open, current))
}
pub fn event(app: &mut App, event: Event) {
    if !same_owner(app) {
        retire(app);
        return;
    }
    let action = match event {
        Event::Key(key) if key.kind != KeyEventKind::Release => {
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                app.dispatch(Action::Quit);
                return;
            }
            match key.code {
                KeyCode::Esc | KeyCode::Char('I') => Some(Action::Close),
                KeyCode::Down | KeyCode::Char('j') => Some(Action::NoticeScroll(1)),
                KeyCode::Up | KeyCode::Char('k') => Some(Action::NoticeScroll(-1)),
                KeyCode::PageDown => Some(Action::NoticeScroll(app.repository_source_rows as i32)),
                KeyCode::PageUp => Some(Action::NoticeScroll(-(app.repository_source_rows as i32))),
                KeyCode::Home => {
                    app.repository_source_scroll = 0;
                    None
                }
                KeyCode::End => {
                    app.repository_source_scroll = 4096;
                    None
                }
                KeyCode::Tab => {
                    app.focus = (app.focus + 1) % app.hits.len().max(1);
                    None
                }
                KeyCode::BackTab => {
                    app.focus =
                        (app.focus + app.hits.len().saturating_sub(1)) % app.hits.len().max(1);
                    None
                }
                KeyCode::Enter => app.hits.get(app.focus).map(|hit| hit.action.clone()),
                _ => None,
            }
        }
        Event::Mouse(mouse) => match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => app
                .hits
                .iter()
                .enumerate()
                .rev()
                .find(|(_, hit)| hit.rect.contains((mouse.column, mouse.row).into()))
                .map(|(index, hit)| {
                    app.focus = index;
                    hit.action.clone()
                }),
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp
                if app
                    .repository_source_area
                    .contains((mouse.column, mouse.row).into()) =>
            {
                Some(Action::NoticeScroll(
                    if mouse.kind == MouseEventKind::ScrollDown {
                        3
                    } else {
                        -3
                    },
                ))
            }
            _ => None,
        },
        Event::Resize(_, _) => {
            app.hits.clear();
            app.repository_source_area = Rect::default();
            None
        }
        _ => None,
    };
    if let Some(action) = action {
        app.dispatch(action);
    }
}
pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    if !same_owner(app) {
        retire(app);
        return;
    }
    let source = app
        .repository_source_overlay
        .as_ref()
        .expect("checked source owner");
    let theme = app.theme();
    let width = area.width.saturating_sub(4).min(100);
    let height = area.height.saturating_sub(4).min(16);
    let rect = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .title("Repository source identity • read-only")
            .style(
                theme
                    .style(SemanticToken::Focus)
                    .remove_modifier(Modifier::UNDERLINED),
            ),
        rect,
    );
    let inner = rect.inner(Margin::new(2, 1));
    let body = Rect::new(
        inner.x,
        inner.y,
        inner.width,
        inner.height.saturating_sub(2),
    );
    app.repository_source_area = body;
    app.repository_source_rows = usize::from(body.height).max(1);
    let mut lines = Vec::<Line<'static>>::new();
    for row in source.details() {
        lines.extend(
            crate::ui::notice_lines(&row.text, body.width)
                .into_iter()
                .map(|line| line.style(theme.style(row.role))),
        );
    }
    app.repository_source_scroll = app
        .repository_source_scroll
        .min(lines.len().saturating_sub(usize::from(body.height)));
    frame.render_widget(
        Paragraph::new(lines).scroll((app.repository_source_scroll as u16, 0)),
        body,
    );
    for (offset, label, action) in [
        (
            0,
            "[PgUp Previous]",
            Action::NoticeScroll(-(app.repository_source_rows as i32)),
        ),
        (
            20,
            "[PgDn More]",
            Action::NoticeScroll(app.repository_source_rows as i32),
        ),
        (40, "[Esc Close]", Action::Close),
    ] {
        let narrow = inner.width < 58;
        let (offset, y) = if narrow && action == Action::Close {
            (0, inner.bottom().saturating_sub(1))
        } else if narrow {
            (offset, inner.bottom().saturating_sub(2))
        } else {
            (offset, inner.bottom().saturating_sub(1))
        };
        let width = crate::editor::display_width(label)
            .min(usize::from(inner.width.saturating_sub(offset))) as u16;
        let rect = Rect::new(inner.x + offset.min(inner.width), y, width, 1);
        if rect.is_empty() {
            continue;
        }
        frame.render_widget(
            Paragraph::new(label).style(
                theme
                    .style(if app.focus == app.hits.len() {
                        SemanticToken::Focus
                    } else {
                        SemanticToken::Normal
                    })
                    .remove_modifier(if app.focus == app.hits.len() {
                        Modifier::empty()
                    } else {
                        Modifier::UNDERLINED
                    }),
            ),
            rect,
        );
        app.hits.push(HitRegion { rect, action });
    }
}

#[cfg(test)]
#[path = "repository_source_ui_tests.rs"]
mod tests;
