//! Owned nested reaction actor viewport. Prepared labels only; no I/O or
//! capability/username-derived writes in draw, hit geometry or key handling.
use super::*;
use crate::providers::gh::ThreadReactionContentV3 as Content;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    layout::Rect,
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
pub const TITLE: &str = "Selected comment reactions";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReactionUiV3 {
    Select(usize),
    Move(i32),
    Scroll(i32),
    Horizontal(i32),
    Top,
    Details,
    Add(Content),
    Remove,
    Back,
    Quit,
}
pub struct ThreadReactionsBrowserV3 {
    pub intent: Arc<ThreadReactionsIntentV3>,
    pub prepared: Option<Arc<PreparedThreadReactionsV3>>,
    pub selected: usize,
    scroll: usize,
    column: usize,
    expanded: bool,
    hits: Vec<(Rect, ReactionUiV3)>,
    body: Rect,
    pub message: &'static str,
}
impl ThreadReactionsBrowserV3 {
    pub fn pending(intent: Arc<ThreadReactionsIntentV3>) -> Self {
        Self {
            intent,
            prepared: None,
            selected: 0,
            scroll: 0,
            column: 0,
            expanded: true,
            hits: vec![],
            body: Rect::default(),
            message: "Original reaction actors pending • no writes • Back cancels",
        }
    }
}
fn view(app: &App) -> Option<&ThreadReactionsBrowserV3> {
    app.scoped_threads_v1
        .as_ref()?
        .comments_v2
        .as_ref()?
        .reactions_v3
        .as_ref()
}
fn view_mut(app: &mut App) -> Option<&mut ThreadReactionsBrowserV3> {
    app.scoped_threads_v1
        .as_mut()?
        .comments_v2
        .as_mut()?
        .reactions_v3
        .as_mut()
}
#[cfg(test)]
pub(crate) fn control_hits_for_test(app: &App) -> Vec<(Rect, ReactionUiV3)> {
    view(app).map_or_else(Vec::new, |view| view.hits.clone())
}
fn owns(app: &App) -> bool {
    app.overlay == Some(Overlay::PrThreadsV1)
        && view(app).is_some_and(|view| {
            view.intent.owns(app)
                && view.prepared.as_ref().is_none_or(|prepared| {
                    Arc::ptr_eq(&prepared.intent, &view.intent)
                        && prepared.actual.comment().comment().node_id
                            == view.intent.comment().node_id
                })
        })
}
fn busy(app: &App) -> bool {
    app.loading[View::Inbox.index()]
        || app.scoped_threads_work_v1.is_some()
        || app.scoped_threads_owner_v1.is_some()
}
pub fn event(app: &mut App, event: &Event) -> bool {
    if super::super::super::ui_v1::retire_invalid(app) {
        return true;
    }
    if !owns(app) {
        if let Some(view) = view_mut(app) {
            view.hits.clear();
            view.body = Rect::default();
        }
        return true;
    }
    let Some(view) = view(app) else {
        return false;
    };
    let action = match event {
        Event::Resize(..) => {
            if let Some(view) = view_mut(app) {
                view.hits.clear();
                view.body = Rect::default();
            }
            None
        }
        Event::Key(key) if key.kind != KeyEventKind::Release => {
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                Some(ReactionUiV3::Quit)
            } else {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('b') => Some(ReactionUiV3::Back),
                    KeyCode::Char('q') => Some(ReactionUiV3::Quit),
                    KeyCode::Up | KeyCode::Char('k') => Some(ReactionUiV3::Move(-1)),
                    KeyCode::Down | KeyCode::Char('j') => Some(ReactionUiV3::Move(1)),
                    KeyCode::PageUp => Some(ReactionUiV3::Scroll(-10)),
                    KeyCode::PageDown => Some(ReactionUiV3::Scroll(10)),
                    KeyCode::Left => Some(ReactionUiV3::Horizontal(-24)),
                    KeyCode::Right => Some(ReactionUiV3::Horizontal(24)),
                    KeyCode::Home => Some(ReactionUiV3::Top),
                    KeyCode::Enter => Some(ReactionUiV3::Details),
                    KeyCode::Char('r') => Some(ReactionUiV3::Remove),
                    KeyCode::Char(number @ '1'..='8') => Some(ReactionUiV3::Add(
                        Content::ALL[(number as u8 - b'1') as usize],
                    )),
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
                Some(ReactionUiV3::Scroll(3))
            }
            MouseEventKind::ScrollUp if view.body.contains((mouse.column, mouse.row).into()) => {
                Some(ReactionUiV3::Scroll(-3))
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
pub(crate) fn dispatch(app: &mut App, action: ReactionUiV3) {
    if matches!(action, ReactionUiV3::Back | ReactionUiV3::Quit) {
        native_v1::close_reactions_v3(app);
        if action == ReactionUiV3::Quit {
            native_v1::close(app);
            app.quit = true;
        }
        return;
    }
    if busy(app) {
        return;
    }
    if !owns(app) {
        return;
    }
    match action {
        ReactionUiV3::Add(content) => {
            native_v1::open_reaction_publication_v3(app, ReactionActionV3::Add(content));
            return;
        }
        ReactionUiV3::Remove => {
            let selected = view(app)
                .and_then(|view| {
                    view.prepared
                        .as_ref()
                        .and_then(|prepared| prepared.actual.rows().get(view.selected))
                })
                .map(|row| row.id());
            if let Some(id) = selected {
                native_v1::open_reaction_publication_v3(app, ReactionActionV3::Remove(id));
            }
            return;
        }
        _ => {}
    }
    let Some(view) = view_mut(app) else {
        return;
    };
    let count = view
        .prepared
        .as_ref()
        .map_or(0, |prepared| prepared.rows.len());
    match action {
        ReactionUiV3::Select(index) if index < count => {
            view.selected = index;
            view.scroll = 0;
        }
        ReactionUiV3::Move(delta) => {
            view.selected = view
                .selected
                .saturating_add_signed(delta as isize)
                .min(count.saturating_sub(1));
            view.scroll = 0;
        }
        ReactionUiV3::Scroll(delta) => {
            view.scroll = view.scroll.saturating_add_signed(delta as isize)
        }
        ReactionUiV3::Horizontal(delta) => {
            view.column = view
                .column
                .saturating_add_signed(delta as isize)
                .min(16 * 1024)
        }
        ReactionUiV3::Top => {
            view.scroll = 0;
            view.column = 0;
        }
        ReactionUiV3::Details => {
            view.expanded = !view.expanded;
            view.scroll = 0;
        }
        _ => {}
    }
    view.hits.clear();
}
pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    if super::super::super::ui_v1::retire_invalid(app) {
        frame.render_widget(Clear, area);
        return;
    }
    let valid = owns(app);
    let busy = busy(app);
    let theme = app.theme();
    let avatars = &app.avatar_states;
    let Some(view) = app
        .scoped_threads_v1
        .as_mut()
        .and_then(|view| view.comments_v2.as_mut())
        .and_then(|view| view.reactions_v3.as_mut())
    else {
        return;
    };
    view.hits.clear();
    view.body = Rect::default();
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .title(TITLE)
            .border_style(theme.style(SemanticToken::Focus)),
        area,
    );
    if !valid {
        return;
    }
    let inside = area.inner(ratatui::layout::Margin::new(1, 1));
    let mut x = inside.x;
    let mut y = inside.y;
    let controls = [
        ("[1 +1]", ReactionUiV3::Add(Content::ThumbsUp)),
        ("[2 -1]", ReactionUiV3::Add(Content::ThumbsDown)),
        ("[3 laugh]", ReactionUiV3::Add(Content::Laugh)),
        ("[4 confused]", ReactionUiV3::Add(Content::Confused)),
        ("[5 heart]", ReactionUiV3::Add(Content::Heart)),
        ("[6 hooray]", ReactionUiV3::Add(Content::Hooray)),
        ("[7 rocket]", ReactionUiV3::Add(Content::Rocket)),
        ("[8 eyes]", ReactionUiV3::Add(Content::Eyes)),
        ("[r Remove mine]", ReactionUiV3::Remove),
        ("[Enter Details]", ReactionUiV3::Details),
        ("[↑ Previous]", ReactionUiV3::Move(-1)),
        ("[↓ Next]", ReactionUiV3::Move(1)),
        ("[PgUp]", ReactionUiV3::Scroll(-10)),
        ("[PgDn]", ReactionUiV3::Scroll(10)),
        ("[← Left]", ReactionUiV3::Horizontal(-24)),
        ("[→ Right]", ReactionUiV3::Horizontal(24)),
        ("[Home Top]", ReactionUiV3::Top),
        ("[Esc Back]", ReactionUiV3::Back),
        ("[q Quit]", ReactionUiV3::Quit),
    ];
    for (label, action) in controls {
        let width = ratatui::text::Line::from(label).width() as u16;
        if x > inside.x && x.saturating_add(width) > inside.right() {
            x = inside.x;
            y += 1;
        }
        if y >= inside.bottom().saturating_sub(1) {
            break;
        }
        let enabled = matches!(action, ReactionUiV3::Back | ReactionUiV3::Quit)
            || !busy
                && view.prepared.as_ref().is_some_and(|prepared| match action {
                    ReactionUiV3::Add(_) => true,
                    ReactionUiV3::Remove => prepared
                        .rows
                        .get(view.selected)
                        .is_some_and(|row| row.owned),
                    _ => !prepared.rows.is_empty(),
                });
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
    let Some(prepared) = &view.prepared else {
        frame.render_widget(
            Paragraph::new(view.message).style(theme.style(SemanticToken::Pending)),
            Rect::new(inside.x, y, inside.width, inside.bottom() - y),
        );
        return;
    };
    for line in prepared.header.iter() {
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
    if prepared.rows.is_empty() && y < inside.bottom() {
        frame.render_widget(Paragraph::new("No reactions observed in the complete/partial coverage stated above; add requires a fresh explicit Preview."),Rect::new(inside.x,y,inside.width,1));
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
        let avatar_width = rect.width.min(4);
        let image = match avatars.get(&row.actor) {
            Some(crate::identity::AvatarState::Ready(image)) => Some(image.as_ref()),
            _ => None,
        };
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
            .scroll((0, view.column as u16))
            .style(if index == view.selected {
                theme.selected(SemanticToken::Reference)
            } else {
                theme.style(SemanticToken::Reference)
            }),
            Rect::new(rect.x + avatar_width, rect.y, rect.width - avatar_width, 1),
        );
        if !busy {
            view.hits.push((rect, ReactionUiV3::Select(index)));
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
                Paragraph::new(line.as_str()).scroll((0, view.column as u16)),
                Rect::new(view.body.x, view.body.y + offset as u16, view.body.width, 1),
            );
        }
    }
}
