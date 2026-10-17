//! Nested, read-only inspector. Its Summary observation never becomes a
//! comment producer or publication approval; Back does no provider work.
use super::*;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    layout::Rect,
    style::Modifier,
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
pub const TITLE: &str = "FILE comment publication history • inspect only";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileHistoryActionV75 {
    Select(usize),
    Move(i32),
    Scroll(i32),
    Horizontal(i32),
    Home,
    Open,
    Refresh,
    Back,
    Quit,
}
pub struct FileCommentHistoryBrowserV75 {
    pub intent: Arc<ThreadHistoryIntentV1>,
    pub listed: Option<Arc<PreparedFileCommentHistoryV75>>,
    pub opened: Option<Arc<PreparedFileCommentHistoryV75>>,
    pub selected: usize,
    pub scroll: usize,
    pub column: usize,
    pub message: &'static str,
    hits: Vec<(Rect, FileHistoryActionV75)>,
    body: Rect,
}
impl FileCommentHistoryBrowserV75 {
    pub fn pending(intent: Arc<ThreadHistoryIntentV1>) -> Self {
        Self {
            intent,
            listed: None,
            opened: None,
            selected: 0,
            scroll: 0,
            column: 0,
            message: "Fresh current Summary/account/source read pending • no writes",
            hits: vec![],
            body: Rect::default(),
        }
    }
}
fn view(app: &App) -> Option<&FileCommentHistoryBrowserV75> {
    app.scoped_thread_history_v1
        .as_ref()?
        .file_history_v75
        .as_ref()
}
fn view_mut(app: &mut App) -> Option<&mut FileCommentHistoryBrowserV75> {
    app.scoped_thread_history_v1
        .as_mut()?
        .file_history_v75
        .as_mut()
}
pub fn dispatch(app: &mut App, action: FileHistoryActionV75) {
    if action == FileHistoryActionV75::Quit {
        native_v1::close(app);
        app.quit = true;
        return;
    }
    let busy = app.loading[View::Inbox.index()] || app.scoped_threads_work_v1.is_some();
    if action == FileHistoryActionV75::Back {
        if !busy && view(app).is_some_and(|view| view.opened.is_some()) {
            if let Some(view) = view_mut(app) {
                view.opened = None;
                view.scroll = 0;
                view.column = 0;
                view.hits.clear();
            }
        } else {
            native_v1::close_file_history_v75(app);
        }
        return;
    }
    if busy {
        return;
    }
    if action == FileHistoryActionV75::Open {
        native_v1::open_file_history_record_v75(app);
        return;
    }
    if action == FileHistoryActionV75::Refresh {
        native_v1::refresh_file_history_v75(app);
        return;
    }
    let Some(view) = view_mut(app) else {
        return;
    };
    let len = view.listed.as_ref().map_or(0, |data| data.labels.len());
    match action {
        FileHistoryActionV75::Select(index) if view.opened.is_none() && index < len => {
            view.selected = index;
            view.column = 0;
        }
        FileHistoryActionV75::Move(delta) | FileHistoryActionV75::Scroll(delta) => {
            if view.opened.is_some() {
                view.scroll = view.scroll.saturating_add_signed(delta as isize);
            } else {
                view.selected = view
                    .selected
                    .saturating_add_signed(delta as isize)
                    .min(len.saturating_sub(1));
                view.scroll = view.selected;
                view.column = 0;
            }
        }
        FileHistoryActionV75::Horizontal(delta) => {
            view.column = view
                .column
                .saturating_add_signed(delta as isize)
                .min(1024 * 1024)
        }
        FileHistoryActionV75::Home => {
            view.scroll = 0;
            view.column = 0;
            if view.opened.is_none() {
                view.selected = 0;
            }
        }
        _ => {}
    }
    view.hits.clear();
}
pub fn event(app: &mut App, event: &Event) -> bool {
    if super::super::ui::retire_invalid(app) {
        return true;
    }
    if app.overlay != Some(Overlay::PrThreadHistoryV1) || view(app).is_none() {
        return false;
    }
    let action = match event {
        Event::Resize(_, _) => {
            if let Some(view) = view_mut(app) {
                view.hits.clear();
                view.body = Rect::default();
            }
            None
        }
        Event::Key(key) if key.kind != KeyEventKind::Release => {
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                Some(FileHistoryActionV75::Quit)
            } else {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('b') => Some(FileHistoryActionV75::Back),
                    KeyCode::Char('q') => Some(FileHistoryActionV75::Quit),
                    KeyCode::Enter => Some(FileHistoryActionV75::Open),
                    KeyCode::Char('r') => Some(FileHistoryActionV75::Refresh),
                    KeyCode::Up | KeyCode::Char('k') => Some(FileHistoryActionV75::Move(-1)),
                    KeyCode::Down | KeyCode::Char('j') => Some(FileHistoryActionV75::Move(1)),
                    KeyCode::PageUp => Some(FileHistoryActionV75::Scroll(-10)),
                    KeyCode::PageDown => Some(FileHistoryActionV75::Scroll(10)),
                    KeyCode::Left => Some(FileHistoryActionV75::Horizontal(-24)),
                    KeyCode::Right => Some(FileHistoryActionV75::Horizontal(24)),
                    KeyCode::Home => Some(FileHistoryActionV75::Home),
                    _ => None,
                }
            }
        }
        Event::Mouse(mouse) => match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => view(app).and_then(|view| {
                view.hits
                    .iter()
                    .find(|(rect, _)| rect.contains((mouse.column, mouse.row).into()))
                    .map(|(_, action)| *action)
            }),
            MouseEventKind::ScrollDown
                if view(app)
                    .is_some_and(|view| view.body.contains((mouse.column, mouse.row).into())) =>
            {
                Some(FileHistoryActionV75::Scroll(3))
            }
            MouseEventKind::ScrollUp
                if view(app)
                    .is_some_and(|view| view.body.contains((mouse.column, mouse.row).into())) =>
            {
                Some(FileHistoryActionV75::Scroll(-3))
            }
            _ => None,
        },
        _ => None,
    };
    if let Some(action) = action {
        dispatch(app, action);
    }
    true
}
pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    if super::super::ui::retire_invalid(app) {
        return;
    }
    app.hits.clear();
    let theme = app.theme();
    let valid = view(app).is_some_and(|view| view.intent.owns(app));
    let busy = app.loading[View::Inbox.index()] || app.scoped_threads_work_v1.is_some();
    let Some(view) = view_mut(app) else {
        return;
    };
    view.hits.clear();
    view.body = Rect::default();
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(TITLE)
        .style(theme.style(SemanticToken::Normal));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if !valid || inner.width < 10 || inner.height < 4 {
        return;
    }
    let (mut x, mut y) = (inner.x, inner.y);
    for (label, action) in [
        ("[Enter Open]", FileHistoryActionV75::Open),
        ("[r Refresh]", FileHistoryActionV75::Refresh),
        ("[PgUp Previous]", FileHistoryActionV75::Scroll(-10)),
        ("[PgDn Next]", FileHistoryActionV75::Scroll(10)),
        ("[Home Top]", FileHistoryActionV75::Home),
        ("[← Left]", FileHistoryActionV75::Horizontal(-24)),
        ("[→ Right]", FileHistoryActionV75::Horizontal(24)),
        ("[Esc Back]", FileHistoryActionV75::Back),
        ("[q Quit]", FileHistoryActionV75::Quit),
    ] {
        let width = ratatui::text::Line::from(label).width() as u16;
        if x > inner.x && x + width > inner.right() {
            x = inner.x;
            y += 1;
        }
        if y >= inner.bottom().saturating_sub(2) {
            break;
        }
        let enabled = matches!(
            action,
            FileHistoryActionV75::Back | FileHistoryActionV75::Quit
        ) || (!busy
            && (action != FileHistoryActionV75::Open
                || view.opened.is_none()
                    && view
                        .listed
                        .as_ref()
                        .is_some_and(|data| !data.labels.is_empty())));
        let rect = Rect::new(x, y, width.min(inner.width), 1);
        frame.render_widget(
            Paragraph::new(label).style(theme.style(if enabled {
                SemanticToken::Focus
            } else {
                SemanticToken::Muted
            })),
            rect,
        );
        if enabled {
            view.hits.push((rect, action));
        }
        x += width + 1;
    }
    y += 1;
    let data = view.opened.as_ref().or(view.listed.as_ref());
    if let Some(data) = data {
        let avatar = Rect::new(inner.x, y, 4.min(inner.width), 1);
        frame.render_widget(
            crate::identity::AvatarWidget {
                identity: &data.actor,
                image: None,
                theme,
            },
            avatar,
        );
        frame.render_widget(
            Paragraph::new(format!(
                "{} {}",
                if view.opened.is_some() {
                    "Recorded creation account:"
                } else {
                    "Current inspecting account:"
                },
                data.actor.label()
            ))
            .style(theme.style(SemanticToken::Actor)),
            Rect::new(
                avatar.right(),
                y,
                inner.width.saturating_sub(avatar.width),
                1,
            ),
        );
        y += 1;
        for line in data.header.iter() {
            for line in crate::ui::notice_lines(line, inner.width) {
                if y >= inner.bottom().saturating_sub(2) {
                    break;
                }
                frame.render_widget(
                    Paragraph::new(line).style(theme.style(SemanticToken::Muted)),
                    Rect::new(inner.x, y, inner.width, 1),
                );
                y += 1;
            }
        }
    }
    let body = Rect::new(
        inner.x,
        y,
        inner.width,
        inner.bottom().saturating_sub(y + 1),
    );
    view.body = body;
    if let Some(data) = data {
        let detail = view.opened.is_some();
        let lines = if detail { &data.detail } else { &data.labels };
        view.scroll = view.scroll.min(lines.len().saturating_sub(1));
        for (index, line) in lines
            .iter()
            .enumerate()
            .skip(view.scroll)
            .take(body.height as usize)
        {
            let rect = Rect::new(body.x, body.y + (index - view.scroll) as u16, body.width, 1);
            let selected = !detail && index == view.selected;
            let offset = line
                .char_indices()
                .nth(view.column)
                .map_or(line.len(), |(offset, _)| offset);
            let label = if detail {
                std::borrow::Cow::Borrowed(&line[offset..])
            } else {
                std::borrow::Cow::Owned(format!(
                    "{} {}",
                    if selected { ">" } else { " " },
                    &line[offset..]
                ))
            };
            let mut style = theme.style(if selected {
                SemanticToken::Focus
            } else if detail {
                SemanticToken::Normal
            } else {
                data.roles[index]
            });
            if selected {
                style = style.add_modifier(Modifier::REVERSED);
            }
            frame.render_widget(Paragraph::new(label).style(style), rect);
            if !detail && !busy {
                view.hits.push((rect, FileHistoryActionV75::Select(index)));
            }
        }
        if lines.is_empty() {
            frame.render_widget(
                Paragraph::new(
                    "No protected comment/reaction attempts observed in this exact scope",
                )
                .style(theme.style(SemanticToken::Muted)),
                body,
            );
        }
    } else {
        frame.render_widget(
            Paragraph::new(view.message).style(theme.style(SemanticToken::Pending)),
            body,
        );
    }
    frame.render_widget(
        Paragraph::new(if busy {
            "Exact original history read pending • Back cancels • no writes"
        } else {
            "INSPECT ONLY • no resend / restore / discard / acknowledgement"
        })
        .style(theme.style(SemanticToken::Muted)),
        Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
    );
}
