//! One read-only topmost overlay. Every control shares keyboard/mouse action.
use super::*;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    layout::Rect,
    style::Modifier,
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
pub const TITLE: &str = "Thread publication history • inspect only";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryActionV1 {
    Select(usize),
    Move(i32),
    Scroll(i32),
    Horizontal(i32),
    Home,
    Open,
    Refresh,
    CommentsV2,
    ReactionsV3,
    Back,
    Quit,
}
pub struct ThreadHistoryBrowserV1 {
    pub comment_history_v2: Option<comments_v2::ui::CommentHistoryBrowserV2>,
    pub reaction_history_v3: Option<reactions_v3::ui::ReactionHistoryBrowserV3>,
    pub intent: Arc<ThreadHistoryIntentV1>,
    pub listed: Option<Arc<PreparedThreadHistoryV1>>,
    pub opened: Option<Arc<PreparedThreadHistoryV1>>,
    pub selected: usize,
    pub scroll: usize,
    pub column: usize,
    pub message: &'static str,
    pub(crate) hits: Vec<(Rect, HistoryActionV1)>,
}
/// Idle invalidation drops only THIS old presentation. A genuinely newer
/// source/form/job is not cleared or canceled merely because history became
/// stale. Publication/read owners still use their existing exact slot guards.
pub fn retire_invalid(app: &mut App) -> bool {
    let Some(old) = app
        .scoped_thread_history_v1
        .as_ref()
        .filter(|view| !view.intent.owns(app))
        .map(|view| view.intent.clone())
    else {
        return false;
    };
    if crate::remote_scope_gate_v1::retire_original_account_mismatch(app, &old.source, &old.active)
    {
        return true;
    }
    let consumed = app.overlay == Some(Overlay::PrThreadHistoryV1);
    if app.scoped_threads_work_v1.as_ref().is_some_and(|work| {
        work.history_intent()
            .is_some_and(|intent| Arc::ptr_eq(intent, &old))
    }) {
        if let Some(work) = app.scoped_threads_work_v1.take() {
            work.cancellation().cancel();
        }
    }
    if app.scoped_threads_owner_v1.as_ref().is_some_and(|owner| {
        owner
            .history_intent()
            .is_some_and(|intent| Arc::ptr_eq(intent, &old))
    }) {
        app.scoped_threads_cancel_v1 = true;
    }
    app.scoped_thread_history_v1 = None;
    if app.overlay == Some(Overlay::PrThreadHistoryV1) {
        app.overlay = None;
        app.hits.clear();
    }
    consumed
}
impl ThreadHistoryBrowserV1 {
    pub fn pending(intent: Arc<ThreadHistoryIntentV1>) -> Self {
        Self{comment_history_v2:None,reaction_history_v3:None,intent,listed:None,opened:None,selected:0,scroll:0,column:0,message:"Fresh original Summary/account/source read pending • Esc cancels • nothing published",hits:vec![]}
    }
}
pub fn dispatch(app: &mut App, action: HistoryActionV1) {
    if action == HistoryActionV1::Quit {
        native_v1::close(app);
        app.quit = true;
        return;
    }
    if action == HistoryActionV1::Back {
        if !app.loading[View::Inbox.index()]
            && app.scoped_threads_work_v1.is_none()
            && app
                .scoped_thread_history_v1
                .as_ref()
                .is_some_and(|view| view.opened.is_some())
        {
            if let Some(view) = &mut app.scoped_thread_history_v1 {
                view.opened = None;
                view.scroll = 0;
                view.column = 0;
                view.hits.clear();
            }
        } else {
            native_v1::close(app);
        }
        return;
    }
    if app.loading[View::Inbox.index()] || app.scoped_threads_work_v1.is_some() {
        return;
    }
    if action == HistoryActionV1::Open {
        native_v1::open_history_record(app);
        return;
    }
    if action == HistoryActionV1::Refresh {
        native_v1::refresh_history(app);
        return;
    }
    if action == HistoryActionV1::CommentsV2 {
        native_v1::open_comment_history_v2(app);
        return;
    }
    if action == HistoryActionV1::ReactionsV3 {
        native_v1::open_reaction_history_v3(app);
        return;
    }
    let Some(view) = &mut app.scoped_thread_history_v1 else {
        return;
    };
    match action {
        HistoryActionV1::Select(index) => {
            if view.opened.is_none()
                && view
                    .listed
                    .as_ref()
                    .is_some_and(|data| index < data.labels.len())
            {
                view.selected = index;
                view.column = 0;
            }
        }
        HistoryActionV1::Move(delta) => {
            if view.opened.is_none() {
                let len = view.listed.as_ref().map_or(0, |data| data.labels.len());
                view.selected = view
                    .selected
                    .saturating_add_signed(delta as isize)
                    .min(len.saturating_sub(1));
                view.scroll = view.selected;
                view.column = 0;
            } else {
                view.scroll = view.scroll.saturating_add_signed(delta as isize);
            }
        }
        HistoryActionV1::Scroll(delta) => {
            if view.opened.is_some() {
                view.scroll = view.scroll.saturating_add_signed(delta as isize);
            } else {
                let len = view.listed.as_ref().map_or(0, |data| data.labels.len());
                view.selected = view
                    .selected
                    .saturating_add_signed(delta as isize)
                    .min(len.saturating_sub(1));
                view.scroll = view.selected;
            }
        }
        HistoryActionV1::Horizontal(delta) => {
            view.column = view
                .column
                .saturating_add_signed(delta as isize)
                .min(512 * 1024)
        }
        HistoryActionV1::Home => {
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
    if retire_invalid(app) {
        return true;
    }
    if app.overlay != Some(Overlay::PrThreadHistoryV1) {
        return false;
    }
    if app
        .scoped_thread_history_v1
        .as_ref()
        .is_some_and(|view| view.reaction_history_v3.is_some())
    {
        return reactions_v3::ui::event(app, event);
    }
    if app
        .scoped_thread_history_v1
        .as_ref()
        .is_some_and(|view| view.comment_history_v2.is_some())
    {
        return comments_v2::ui::event(app, event);
    }
    let action = match event {
        Event::Resize(_, _) => {
            if let Some(view) = &mut app.scoped_thread_history_v1 {
                view.hits.clear();
            }
            None
        }
        Event::Key(key) if key.kind != KeyEventKind::Release => {
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                Some(HistoryActionV1::Quit)
            } else {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('b') => Some(HistoryActionV1::Back),
                    KeyCode::Char('q') => Some(HistoryActionV1::Quit),
                    KeyCode::Enter => Some(HistoryActionV1::Open),
                    KeyCode::Char('r') => Some(HistoryActionV1::Refresh),
                    KeyCode::Char('c') => Some(HistoryActionV1::CommentsV2),
                    KeyCode::Char('R') => Some(HistoryActionV1::ReactionsV3),
                    KeyCode::Up | KeyCode::Char('k') => Some(HistoryActionV1::Move(-1)),
                    KeyCode::Down | KeyCode::Char('j') => Some(HistoryActionV1::Move(1)),
                    KeyCode::PageUp => Some(HistoryActionV1::Scroll(-10)),
                    KeyCode::PageDown => Some(HistoryActionV1::Scroll(10)),
                    KeyCode::Left => Some(HistoryActionV1::Horizontal(-24)),
                    KeyCode::Right => Some(HistoryActionV1::Horizontal(24)),
                    KeyCode::Home => Some(HistoryActionV1::Home),
                    _ => None,
                }
            }
        }
        Event::Mouse(mouse) => match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                app.scoped_thread_history_v1.as_ref().and_then(|view| {
                    view.hits
                        .iter()
                        .find(|(rect, _)| rect.contains((mouse.column, mouse.row).into()))
                        .map(|(_, action)| *action)
                })
            }
            MouseEventKind::ScrollDown => Some(HistoryActionV1::Scroll(3)),
            MouseEventKind::ScrollUp => Some(HistoryActionV1::Scroll(-3)),
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
    retire_invalid(app);
    if app
        .scoped_thread_history_v1
        .as_ref()
        .is_some_and(|view| view.reaction_history_v3.is_some())
    {
        reactions_v3::ui::draw(frame, app, area);
        return;
    }
    if app
        .scoped_thread_history_v1
        .as_ref()
        .is_some_and(|view| view.comment_history_v2.is_some())
    {
        comments_v2::ui::draw(frame, app, area);
        return;
    }
    app.hits.clear();
    let theme = app.theme();
    let valid = app
        .scoped_thread_history_v1
        .as_ref()
        .is_some_and(|view| view.intent.owns(app));
    let busy = app.loading[View::Inbox.index()] || app.scoped_threads_work_v1.is_some();
    let Some(view) = &mut app.scoped_thread_history_v1 else {
        return;
    };
    view.hits.clear();
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(TITLE)
        .style(theme.style(SemanticToken::Normal));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if !valid {
        return;
    }
    if inner.width < 10 || inner.height < 4 {
        return;
    }
    let mut x = inner.x;
    let mut y = inner.y;
    let controls = [
        ("[Enter Open]", HistoryActionV1::Open),
        ("[r Refresh]", HistoryActionV1::Refresh),
        ("[c Comment history]", HistoryActionV1::CommentsV2),
        ("[R Reaction history]", HistoryActionV1::ReactionsV3),
        ("[PgUp Previous]", HistoryActionV1::Scroll(-10)),
        ("[PgDn Next]", HistoryActionV1::Scroll(10)),
        ("[Home Top]", HistoryActionV1::Home),
        ("[← Left]", HistoryActionV1::Horizontal(-24)),
        ("[→ Right]", HistoryActionV1::Horizontal(24)),
        ("[Esc Back]", HistoryActionV1::Back),
        ("[q Quit]", HistoryActionV1::Quit),
    ];
    for (label, action) in controls {
        let width = ratatui::text::Line::from(label).width() as u16;
        if x > inner.x && x + width > inner.right() {
            x = inner.x;
            y += 1;
        }
        if y >= inner.bottom().saturating_sub(2) {
            break;
        }
        let enabled = matches!(action, HistoryActionV1::Back | HistoryActionV1::Quit)
            || (!busy
                && (action != HistoryActionV1::Open
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
                    "Recorded intent account:"
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
        for line in &data.header {
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
    if let Some(data) = data {
        let detail = view.opened.is_some();
        let lines = if detail { &data.detail } else { &data.labels };
        view.scroll = view.scroll.min(lines.len().saturating_sub(1));
        for (row, line) in lines
            .iter()
            .enumerate()
            .skip(view.scroll)
            .take(body.height as usize)
        {
            let rect = Rect::new(body.x, body.y + (row - view.scroll) as u16, body.width, 1);
            let selected = !detail && row == view.selected;
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
                data.roles[row]
            });
            if selected {
                style = style.add_modifier(Modifier::REVERSED);
            }
            frame.render_widget(Paragraph::new(label).style(style), rect);
            if !detail && !busy {
                view.hits.push((rect, HistoryActionV1::Select(row)));
            }
        }
        if lines.is_empty() {
            frame.render_widget(
                Paragraph::new("No local thread attempts observed in this exact current scope")
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
            "Inspect only • no resend / restore / discard / acknowledgement"
        })
        .style(theme.style(SemanticToken::Muted)),
        Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
    );
}
