//! One owned nested read-only historical chooser/viewer. No source reads,
//! full-body wrapping or publication/range constructors in draw or events.
use super::*;
use crate::prepared_form_text_v1::FormPreviewViewportV1;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    layout::Rect,
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
pub const TITLE: &str = "Historical comment source • placement UNKNOWN";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActionV5 {
    Choose(Role),
    Scroll(i32),
    Home,
    Back,
    Quit,
}
pub struct HistoricalBrowserV5 {
    pub(crate) selection: Arc<HistoricalSelectionV5>,
    pub(crate) role: Option<Role>,
    pub(crate) prepared: Option<Arc<ClosedThreadHistoricalContextV5>>,
    top: usize,
    viewport: FormPreviewViewportV1,
    hits: Vec<(Rect, ActionV5)>,
    body: Rect,
    pub(crate) message: &'static str,
}
pub fn open(app: &mut App) {
    if native_v1::newer_intent(app)
        || app.loading[View::Inbox.index()]
        || app.scoped_threads_owner_v1.is_some()
    {
        return;
    }
    match HistoricalSelectionV5::capture(app) {
        Ok(selection) => {
            let Some(view) = app
                .scoped_threads_v1
                .as_mut()
                .and_then(|view| view.comments_v2.as_mut())
            else {
                return;
            };
            view.historical_v5=Some(HistoricalBrowserV5{selection,role:None,prepared:None,top:0,viewport:Default::default(),hits:vec![],body:Rect::default(),message:"Choose exact observed commit role • path is a candidate, review placement UNKNOWN"});
            app.hits.clear();
        }
        Err(reason) => app.status = reason,
    }
}
fn selection(app: &App) -> Option<Arc<HistoricalSelectionV5>> {
    app.scoped_threads_v1
        .as_ref()
        .and_then(|view| view.comments_v2.as_ref())
        .and_then(|view| view.historical_v5.as_ref())
        .map(|view| view.selection.clone())
}
pub fn event(app: &mut App, event: &Event) -> bool {
    let Some(original) = selection(app) else {
        return false;
    };
    if !original.owns(app) {
        native_v1::close_historical_v5(app, &original);
        return true;
    }
    let view = app
        .scoped_threads_v1
        .as_ref()
        .and_then(|view| view.comments_v2.as_ref())
        .and_then(|view| view.historical_v5.as_ref())
        .expect("same original nested view");
    let action = match event {
        Event::Resize(..) => {
            if let Some(view) = app
                .scoped_threads_v1
                .as_mut()
                .and_then(|view| view.comments_v2.as_mut())
                .and_then(|view| view.historical_v5.as_mut())
            {
                view.hits.clear();
                view.body = Rect::default();
            }
            None
        }
        Event::Key(key) if key.kind != KeyEventKind::Release => {
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                Some(ActionV5::Quit)
            } else {
                match key.code {
                    KeyCode::Char('c') => Some(ActionV5::Choose(Role::Associated)),
                    KeyCode::Char('o') => Some(ActionV5::Choose(Role::Original)),
                    KeyCode::Down | KeyCode::Char('j') => Some(ActionV5::Scroll(1)),
                    KeyCode::Up | KeyCode::Char('k') => Some(ActionV5::Scroll(-1)),
                    KeyCode::PageDown => Some(ActionV5::Scroll(10)),
                    KeyCode::PageUp => Some(ActionV5::Scroll(-10)),
                    KeyCode::Home => Some(ActionV5::Home),
                    KeyCode::Char('b') | KeyCode::Esc => Some(ActionV5::Back),
                    KeyCode::Char('q') => Some(ActionV5::Quit),
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
                Some(ActionV5::Scroll(3))
            }
            MouseEventKind::ScrollUp if view.body.contains((mouse.column, mouse.row).into()) => {
                Some(ActionV5::Scroll(-3))
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
fn dispatch(app: &mut App, action: ActionV5) {
    let Some(original) = selection(app).filter(|original| original.owns(app)) else {
        return;
    };
    if matches!(action, ActionV5::Back | ActionV5::Quit) {
        native_v1::close_historical_v5(app, &original);
        if action == ActionV5::Quit {
            native_v1::close(app);
            app.quit = true;
        }
        return;
    }
    let busy = app.loading[View::Inbox.index()]
        || app.scoped_threads_owner_v1.is_some()
        || native_v1::newer_intent(app);
    if busy {
        return;
    }
    if let ActionV5::Choose(role) = action {
        native_v1::open_historical_v5(app, role);
        return;
    }
    let Some(view) = app
        .scoped_threads_v1
        .as_mut()
        .and_then(|view| view.comments_v2.as_mut())
        .and_then(|view| view.historical_v5.as_mut())
    else {
        return;
    };
    if view.prepared.is_none() {
        return;
    }
    match action {
        ActionV5::Scroll(delta) => view.top = view.top.saturating_add_signed(delta as isize),
        ActionV5::Home => view.top = 0,
        _ => {}
    }
    view.hits.clear();
}
pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) -> bool {
    let Some(original) = selection(app) else {
        return false;
    };
    let valid = original.owns(app);
    let theme = app.theme();
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .title(TITLE)
            .border_style(theme.style(SemanticToken::Focus)),
        area,
    );
    if !valid {
        native_v1::close_historical_v5(app, &original);
        return true;
    }
    let busy = app.loading[View::Inbox.index()]
        || app.scoped_threads_work_v1.is_some()
        || app.scoped_threads_owner_v1.is_some();
    let avatars = &app.avatar_states;
    let Some(view) = app
        .scoped_threads_v1
        .as_mut()
        .and_then(|view| view.comments_v2.as_mut())
        .and_then(|view| view.historical_v5.as_mut())
    else {
        return true;
    };
    view.hits.clear();
    view.body = Rect::default();
    let inner = area.inner(ratatui::layout::Margin::new(1, 1));
    let mut x = inner.x;
    let mut y = inner.y;
    let controls = [
        ("[c Associated C]", ActionV5::Choose(Role::Associated)),
        ("[o Original C0]", ActionV5::Choose(Role::Original)),
        ("[PgUp Previous]", ActionV5::Scroll(-10)),
        ("[PgDn Next]", ActionV5::Scroll(10)),
        ("[Home Top]", ActionV5::Home),
        ("[Esc Back]", ActionV5::Back),
        ("[q Quit]", ActionV5::Quit),
    ];
    for (label, action) in controls {
        let width = ratatui::text::Line::from(label).width() as u16;
        if x > inner.x && x.saturating_add(width) > inner.right() {
            x = inner.x;
            y = y.saturating_add(1);
        }
        if y >= inner.bottom() {
            break;
        }
        let enabled = match action {
            ActionV5::Choose(role) => !busy && view.selection.role_available(role),
            ActionV5::Scroll(_) | ActionV5::Home => !busy && view.prepared.is_some(),
            _ => true,
        };
        let rect = Rect::new(x, y, width.min(inner.right().saturating_sub(x)), 1);
        frame.render_widget(
            Paragraph::new(label).style(theme.style(if enabled {
                SemanticToken::Focus
            } else {
                SemanticToken::Muted
            })),
            rect,
        );
        view.hits.push((rect, action));
        x = x.saturating_add(width + 1);
    }
    y = y.saturating_add(1);
    if y < inner.bottom() {
        frame.render_widget(
            Paragraph::new(view.message).style(theme.style(SemanticToken::Pending)),
            Rect::new(inner.x, y, inner.width, 1),
        );
        y = y.saturating_add(1);
    }
    if inner.width < 68 || inner.bottom().saturating_sub(y) < 3 {
        if y < inner.bottom() {
            frame.render_widget(Paragraph::new("Resize to80x18 to inspect read-only historical content; Back remains available").style(theme.style(SemanticToken::Pending)),Rect::new(inner.x,y,inner.width,inner.bottom()-y));
        }
        return true;
    }
    let body = Rect::new(
        inner.x,
        y,
        inner.width,
        inner
            .bottom()
            .saturating_sub(y)
            .min(crate::prepared_form_text_v1::FORM_VISIBLE_ROWS_V1),
    );
    view.body = body;
    if let Some(prepared) = &view.prepared {
        if !Arc::ptr_eq(&prepared.intent.selection, &original)
            || view.role != Some(prepared.intent.role)
        {
            return true;
        }
        let Ok(window) = prepared.document.window(
            &mut view.viewport,
            body.width.saturating_sub(4),
            body.height,
            view.top,
        ) else {
            return true;
        };
        view.top = window.top();
        let image = match avatars.get(&prepared.actor) {
            Some(crate::identity::AvatarState::Ready(image)) => Some(image.as_ref()),
            _ => None,
        };
        frame.render_widget(
            crate::identity::AvatarWidget {
                identity: &prepared.actor,
                image,
                theme,
            },
            Rect::new(body.x, body.y, 4, 1),
        );
        for (offset, row) in window.rows().enumerate() {
            frame.render_widget(
                Paragraph::new(row.text()).style(theme.base()),
                Rect::new(
                    body.x + 4,
                    body.y + offset as u16,
                    body.width.saturating_sub(4),
                    1,
                ),
            );
        }
    } else {
        let text = if busy {
            "Loading exact historical commit/tree/blob under original source and clock • Esc Back cancels"
        } else {
            "Explicitly choose Associated C or Original C0. The observed path is tested exactly; no old rename, LEFT side or original line is guessed."
        };
        frame.render_widget(
            Paragraph::new(text).style(theme.style(SemanticToken::Muted)),
            body,
        );
    }
    true
}
