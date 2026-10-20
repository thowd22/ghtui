//! TASK-93: the Repository viewers that own every key while open (native diff, saved-folder
//! detail, saved-path picker and changed-files picker) render inside the content pane, under
//! the shell header and above the key bar, instead of over the whole terminal.
//!
//! The viewer keeps its own keys and click targets. The shell around it stays usable: a click
//! on a view tab or breadcrumb acts as it does anywhere else, and the header's Back, the key
//! bar's `Esc Back` and the configured Back key all send the viewer its own Esc, so every way
//! back has the same effect as pressing Esc. While a write plan or an external viewer is open
//! it keeps every click (as before), so a tab cannot retire it underneath.
use crate::{
    app::{Action, App, View},
    footer::{ControlGroup, KeyControl, PINNED},
    navigation_trail::NavigationAction,
};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    widgets::Block,
    Frame,
};

/// What a shell control around an open viewer does.
#[derive(Clone, Debug)]
pub enum ChromeActionV93 {
    /// The viewer's own Esc.
    Back,
    /// A shell action (view tab, breadcrumb, Quit).
    Act(Action),
}

/// The pane the viewer was last drawn into and the shell's click targets around it. Empty
/// whenever the last frame did not embed a viewer.
#[derive(Clone, Debug, Default)]
pub struct TakeoverChromeV93 {
    pub pane: Option<Rect>,
    pub hits: Vec<(Rect, ChromeActionV93)>,
}

/// Whether a Repository viewer that owns the screen is open.
pub fn open(app: &App) -> bool {
    app.view == View::Repository
        && app.overlay.is_none()
        && app.remote_repository.is_none()
        && (app.native_diff_v1.is_some()
            || app.native_mutable_diff_v2.is_some()
            || app.native_mutable_plan_v2.is_some()
            || app.original_tree_viewport_v2.folder_intent.is_some()
            || app.original_tree_jump_v1.is_some()
            || app.original_diff_picker_v1.is_some())
}

/// A write plan or an external viewer keeps every input until it ends.
fn exclusive(app: &App) -> bool {
    app.native_mutable_plan_v2.is_some()
        || app
            .native_diff_v1
            .as_ref()
            .is_some_and(|state| state.external_v4.is_some())
        || app
            .native_mutable_diff_v2
            .as_ref()
            .is_some_and(|state| state.external_v4.is_some())
}

/// The key bar under an open viewer: Back (the viewer's Esc) and Quit (Ctrl-C, which every
/// viewer maps to Quit).
pub fn controls() -> Vec<KeyControl<ChromeActionV93>> {
    vec![
        KeyControl::new("Esc", "Back", ChromeActionV93::Back)
            .with_priority(PINNED)
            .in_group(ControlGroup::Navigate),
        KeyControl::new("Ctrl-C", "Quit", ChromeActionV93::Act(Action::Quit))
            .with_priority(PINNED)
            .in_group(ControlGroup::Global),
    ]
}

/// The pane rect under the header and above the key bar.
pub fn pane(app: &App, area: Rect) -> [Rect; 3] {
    let header = crate::ui_chrome::header_rows(app, area.width);
    let rows = Layout::vertical([
        Constraint::Length(header),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(area);
    [rows[0], rows[1], rows[2]]
}

/// Draws an open Repository viewer inside the shell. Viewers of other views keep the whole
/// area. Returns whether a viewer was drawn.
pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) -> bool {
    app.takeover_v93 = TakeoverChromeV93::default();
    if app.view != View::Repository {
        return super::draw_takeover(frame, app, area);
    }
    let [header, pane, footer] = pane(app, area);
    if !super::draw_takeover(frame, app, pane) {
        return false;
    }
    let theme = app.theme();
    frame.render_widget(Block::default().style(theme.base()), header);
    // The viewer owns focus; nothing in the shell may look focused.
    let focus = app.focus;
    app.focus = usize::MAX;
    let first = app.hits.len();
    crate::ui_chrome::draw_header(frame, app, header);
    let header_hits = app.hits.split_off(first);
    app.focus = focus;
    let exclusive = exclusive(app);
    let mut hits = header_hits
        .into_iter()
        .filter_map(|hit| {
            let action = match hit.action {
                Action::Navigation(NavigationAction::Back) => ChromeActionV93::Back,
                Action::View(_) | Action::Navigation(NavigationAction::Breadcrumb(_))
                    if !exclusive =>
                {
                    ChromeActionV93::Act(hit.action)
                }
                _ => return None,
            };
            Some((hit.rect, action))
        })
        .collect::<Vec<_>>();
    hits.extend(crate::footer::render(
        frame,
        footer,
        theme,
        &controls(),
        None,
    ));
    app.takeover_v93 = TakeoverChromeV93 {
        pane: Some(pane),
        hits,
    };
    true
}

fn back_key(app: &App, key: &KeyEvent) -> bool {
    let binding = app.shortcut("back");
    // Only a modified binding: a plain key may be text the viewer is editing.
    binding.contains('+')
        && crate::app::pressed_shortcut(key.code, key.modifiers).as_deref() == Some(binding)
}

fn back(app: &mut App) {
    app.takeover_v93 = TakeoverChromeV93::default();
    app.handle_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
}

/// Routes a click on the shell around an open viewer, and the Back key, before the viewer
/// captures the event. Everything else stays the viewer's.
pub fn event(app: &mut App, event: &Event) -> bool {
    let Some(pane) = app.takeover_v93.pane else {
        return false;
    };
    if !open(app) {
        app.takeover_v93 = TakeoverChromeV93::default();
        return false;
    }
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press && back_key(app, key) => {
            back(app);
            true
        }
        Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
            let position = (mouse.column, mouse.row).into();
            if pane.contains(position) {
                return false;
            }
            let Some(action) = app
                .takeover_v93
                .hits
                .iter()
                .rev()
                .find(|(rect, _)| rect.contains(position))
                .map(|(_, action)| action.clone())
            else {
                return false;
            };
            match action {
                ChromeActionV93::Back => back(app),
                ChromeActionV93::Act(action) => {
                    app.takeover_v93 = TakeoverChromeV93::default();
                    app.dispatch(action);
                }
            }
            true
        }
        _ => false,
    }
}
