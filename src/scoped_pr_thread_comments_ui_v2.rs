//! Nested actual comment selection. No IO, API call, authority, dynamic body
//! formatting or full workspace clone occurs in paint/event handling.
use super::*;
use crate::pr_write_journal::thread_comment_history_v2::CommentActionV2;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    layout::Rect,
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
pub const TITLE: &str = "Selected thread comments";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommentUiActionV2 {
    Historical,
    CurrentCode,
    Select(usize),
    Move(i32),
    Scroll(i32),
    Horizontal(i32),
    Home,
    Toggle,
    Edit,
    Delete,
    Reactions,
    Back,
    Quit,
}
pub struct ThreadCommentsBrowserV2 {
    pub(crate) historical_v5: Option<historical_v5::ui::HistoricalBrowserV5>,
    pub intent: Arc<ThreadCommentsIntentV2>,
    pub prepared: Option<Arc<PreparedThreadCommentsV2>>,
    pub selected: usize,
    pub reactions_v3: Option<reactions_v3::ui::ThreadReactionsBrowserV3>,
    scroll: usize,
    column: usize,
    expanded: bool,
    pub message: &'static str,
    hits: Vec<(Rect, CommentUiActionV2)>,
    body: Rect,
}
impl ThreadCommentsBrowserV2 {
    pub fn pending(intent: Arc<ThreadCommentsIntentV2>) -> Self {
        Self {
            historical_v5: None,
            intent,
            prepared: None,
            selected: 0,
            reactions_v3: None,
            scroll: 0,
            column: 0,
            expanded: true,
            message: "Exact selected-thread comment read pending • Back cancels • no writes",
            hits: vec![],
            body: Rect::default(),
        }
    }
}
fn busy(app: &App) -> bool {
    app.loading[View::Inbox.index()]
        || app.scoped_threads_work_v1.is_some()
        || app.scoped_threads_owner_v1.is_some()
}
fn owns(app: &App) -> bool {
    app.overlay == Some(Overlay::PrThreadsV1)
        && app
            .scoped_threads_v1
            .as_ref()
            .and_then(|view| view.comments_v2.as_ref())
            .is_some_and(|view| {
                view.intent.owns(app)
                    && view.prepared.as_ref().is_none_or(|prepared| {
                        Arc::ptr_eq(&prepared.intent, &view.intent)
                            && prepared.actual.thread().node_id == view.intent.thread_node()
                    })
            })
}
pub fn event(app: &mut App, event: &Event) -> bool {
    if app
        .scoped_threads_v1
        .as_ref()
        .and_then(|view| view.comments_v2.as_ref())
        .is_some_and(|view| view.reactions_v3.is_some())
    {
        return reactions_v3::ui::event(app, event);
    }
    if super::super::ui_v1::retire_invalid(app) {
        return true;
    }
    if !owns(app) {
        if let Some(view) = app
            .scoped_threads_v1
            .as_mut()
            .and_then(|view| view.comments_v2.as_mut())
        {
            view.hits.clear();
            view.body = Rect::default();
        }
        return true;
    }
    if historical_v5::ui::event(app, event) {
        return true;
    }
    let Some(view) = app
        .scoped_threads_v1
        .as_ref()
        .and_then(|view| view.comments_v2.as_ref())
    else {
        return false;
    };
    let action = match event {
        Event::Resize(..) => {
            if let Some(view) = app
                .scoped_threads_v1
                .as_mut()
                .and_then(|view| view.comments_v2.as_mut())
            {
                view.hits.clear();
                view.body = Rect::default();
            }
            None
        }
        Event::Key(key) if key.kind != KeyEventKind::Release => {
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                Some(CommentUiActionV2::Quit)
            } else {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('b') => Some(CommentUiActionV2::Back),
                    KeyCode::Char('q') => Some(CommentUiActionV2::Quit),
                    KeyCode::Up | KeyCode::Char('k') => Some(CommentUiActionV2::Move(-1)),
                    KeyCode::Down | KeyCode::Char('j') => Some(CommentUiActionV2::Move(1)),
                    KeyCode::PageUp => Some(CommentUiActionV2::Scroll(-10)),
                    KeyCode::PageDown => Some(CommentUiActionV2::Scroll(10)),
                    KeyCode::Left => Some(CommentUiActionV2::Horizontal(-24)),
                    KeyCode::Right => Some(CommentUiActionV2::Horizontal(24)),
                    KeyCode::Home => Some(CommentUiActionV2::Home),
                    KeyCode::Enter => Some(CommentUiActionV2::Toggle),
                    KeyCode::Char('e') => Some(CommentUiActionV2::Edit),
                    KeyCode::Char('d') => Some(CommentUiActionV2::Delete),
                    KeyCode::Char('R') => Some(CommentUiActionV2::Reactions),
                    KeyCode::Char('g') => Some(CommentUiActionV2::CurrentCode),
                    KeyCode::Char('h') => Some(CommentUiActionV2::Historical),
                    _ => None,
                }
            }
        }
        Event::Mouse(mouse) => match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => view
                .hits
                .iter()
                .find(|(rect, _)| rect.contains((mouse.column, mouse.row).into()))
                .map(|(_, action)| *action),
            MouseEventKind::ScrollDown if view.body.contains((mouse.column, mouse.row).into()) => {
                Some(CommentUiActionV2::Scroll(3))
            }
            MouseEventKind::ScrollUp if view.body.contains((mouse.column, mouse.row).into()) => {
                Some(CommentUiActionV2::Scroll(-3))
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
pub(crate) fn dispatch(app: &mut App, action: CommentUiActionV2) {
    if matches!(action, CommentUiActionV2::Back | CommentUiActionV2::Quit) {
        native_v1::close_comments_v2(app);
        if action == CommentUiActionV2::Quit {
            native_v1::close(app);
            app.quit = true;
        }
        return;
    }
    if busy(app) {
        return;
    }
    if action == CommentUiActionV2::Reactions {
        native_v1::open_reactions_v3(app);
        return;
    }
    if action == CommentUiActionV2::CurrentCode {
        native_v1::open_current_code_v4(app);
        return;
    }
    if action == CommentUiActionV2::Historical {
        historical_v5::ui::open(app);
        return;
    }
    if matches!(action, CommentUiActionV2::Edit | CommentUiActionV2::Delete) {
        native_v1::open_comment_publication_v2(
            app,
            if action == CommentUiActionV2::Edit {
                CommentActionV2::Edit
            } else {
                CommentActionV2::Delete
            },
        );
        return;
    }
    let Some(view) = app
        .scoped_threads_v1
        .as_mut()
        .and_then(|view| view.comments_v2.as_mut())
    else {
        return;
    };
    let count = view
        .prepared
        .as_ref()
        .map_or(0, |prepared| prepared.rows.len());
    match action {
        CommentUiActionV2::Historical => unreachable!("handled before mutable view"),
        CommentUiActionV2::Select(index) if index < count => {
            view.selected = index;
            view.scroll = 0;
        }
        CommentUiActionV2::Move(delta) => {
            view.selected = view
                .selected
                .saturating_add_signed(delta as isize)
                .min(count.saturating_sub(1));
            view.scroll = 0;
        }
        CommentUiActionV2::Scroll(delta) => {
            view.scroll = view.scroll.saturating_add_signed(delta as isize)
        }
        CommentUiActionV2::Horizontal(delta) => {
            view.column = view
                .column
                .saturating_add_signed(delta as isize)
                .min(FIELD_CAP)
        }
        CommentUiActionV2::Home => {
            view.scroll = 0;
            view.column = 0;
        }
        CommentUiActionV2::Toggle => {
            view.expanded = !view.expanded;
            view.scroll = 0;
        }
        _ => {}
    }
    view.hits.clear();
}
pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    if app
        .scoped_threads_v1
        .as_ref()
        .and_then(|view| view.comments_v2.as_ref())
        .is_some_and(|view| view.reactions_v3.is_some())
    {
        reactions_v3::ui::draw(frame, app, area);
        return;
    }
    if super::super::ui_v1::retire_invalid(app) {
        frame.render_widget(Clear, area);
        return;
    }
    if historical_v5::ui::draw(frame, app, area) {
        return;
    }
    let valid = owns(app);
    let theme = app.theme();
    let is_busy = busy(app);
    let avatars = &app.avatar_states;
    let Some(browser) = app.scoped_threads_v1.as_mut() else {
        return;
    };
    let notice = &browser.code_notice_v4;
    let Some(view) = browser.comments_v2.as_mut() else {
        return;
    };
    view.hits.clear();
    view.body = Rect::default();
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .title(TITLE)
            .border_style(theme.style(SemanticToken::Focus)),
        area,
    );
    if !valid {
        return;
    }
    let inside = area.inner(ratatui::layout::Margin::new(1, 1));
    if inside.width == 0 || inside.height == 0 {
        return;
    }
    let controls = [
        ("[Enter Details]", CommentUiActionV2::Toggle),
        ("[e Edit]", CommentUiActionV2::Edit),
        ("[d Delete]", CommentUiActionV2::Delete),
        ("[R Reactions]", CommentUiActionV2::Reactions),
        ("[g Current code]", CommentUiActionV2::CurrentCode),
        ("[h Historical source]", CommentUiActionV2::Historical),
        ("[↑ Previous]", CommentUiActionV2::Move(-1)),
        ("[↓ Next]", CommentUiActionV2::Move(1)),
        ("[PgUp]", CommentUiActionV2::Scroll(-10)),
        ("[PgDn]", CommentUiActionV2::Scroll(10)),
        ("[← Left]", CommentUiActionV2::Horizontal(-24)),
        ("[→ Right]", CommentUiActionV2::Horizontal(24)),
        ("[Home Top]", CommentUiActionV2::Home),
        ("[Esc Back]", CommentUiActionV2::Back),
        ("[q Quit]", CommentUiActionV2::Quit),
    ];
    let mut x = inside.x;
    let mut y = inside.y;
    for (label, action) in controls {
        let width = ratatui::text::Line::from(label).width() as u16;
        if x > inside.x && x.saturating_add(width) > inside.right() {
            x = inside.x;
            y += 1;
        }
        if y >= inside.bottom().saturating_sub(1) {
            break;
        }
        let enabled = matches!(action, CommentUiActionV2::Back | CommentUiActionV2::Quit)
            || (!is_busy
                && view
                    .prepared
                    .as_ref()
                    .is_some_and(|prepared| !prepared.rows.is_empty()));
        let rect = Rect::new(x, y, width.min(inside.right().saturating_sub(x)), 1);
        frame.render_widget(
            Paragraph::new(label).style(theme.style(if enabled {
                SemanticToken::Reference
            } else {
                SemanticToken::Muted
            })),
            rect,
        );
        if enabled {
            view.hits.push((rect, action));
        }
        x = x.saturating_add(width + 1);
    }
    y += 1;
    if y >= inside.bottom() {
        return;
    }
    if let Some(notice) = notice {
        frame.render_widget(
            Paragraph::new(notice.as_str()).style(theme.style(SemanticToken::Pending)),
            Rect::new(inside.x, y, inside.width, 1),
        );
        y += 1;
    }
    let Some(prepared) = &view.prepared else {
        frame.render_widget(
            Paragraph::new(view.message).style(theme.style(SemanticToken::Pending)),
            Rect::new(inside.x, y, inside.width, inside.bottom() - y),
        );
        return;
    };
    for line in prepared.header.iter().take(2) {
        if y >= inside.bottom() {
            return;
        }
        frame.render_widget(
            Paragraph::new(line.as_str())
                .scroll((0, view.column as u16))
                .style(theme.style(SemanticToken::Pending)),
            Rect::new(inside.x, y, inside.width, 1),
        );
        y += 1;
    }
    view.selected = view.selected.min(prepared.rows.len().saturating_sub(1));
    for (index, row) in prepared
        .rows
        .iter()
        .enumerate()
        .skip(view.selected.saturating_sub(1))
        .take(3)
    {
        if y >= inside.bottom() {
            return;
        }
        let rect = Rect::new(inside.x, y, inside.width, 1);
        let image = match avatars.get(&row.actor) {
            Some(crate::identity::AvatarState::Ready(image)) => Some(image.as_ref()),
            _ => None,
        };
        let avatar_width = rect.width.min(4);
        frame.render_widget(
            crate::identity::AvatarWidget {
                identity: &row.actor,
                image,
                theme,
            },
            Rect::new(rect.x, rect.y, avatar_width, 1),
        );
        frame.render_widget(
            Paragraph::new(format!(
                "{}{}",
                if index == view.selected { "> " } else { "  " },
                row.heading
            ))
            .style(if index == view.selected {
                theme.selected(SemanticToken::Reference)
            } else {
                theme.style(SemanticToken::Reference)
            }),
            Rect::new(rect.x + avatar_width, rect.y, rect.width - avatar_width, 1),
        );
        if !is_busy {
            view.hits.push((rect, CommentUiActionV2::Select(index)));
        }
        y += 1;
    }
    view.body = Rect::new(inside.x, y, inside.width, inside.bottom().saturating_sub(y));
    if let Some(row) = prepared.rows.get(view.selected).filter(|_| view.expanded) {
        view.scroll = view
            .scroll
            .min(row.lines.len().saturating_sub(view.body.height as usize));
        for (offset, line) in row
            .lines
            .iter()
            .skip(view.scroll)
            .take(view.body.height as usize)
            .enumerate()
        {
            frame.render_widget(
                Paragraph::new(line.as_str())
                    .scroll((0, view.column as u16))
                    .style(theme.style(SemanticToken::Reference)),
                Rect::new(view.body.x, view.body.y + offset as u16, view.body.width, 1),
            );
        }
    }
}
