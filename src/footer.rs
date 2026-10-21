//! The context key bar: one bottom row of `key label` tokens, the key in the view accent and
//! the label muted, ending with the Global tail `? Help  : Commands  q Quit`. Each view declares
//! its controls as [`KeyControl`]s; [`layout`] fits them by priority, [`draw`] paints them and
//! registers their click targets last, and [`resolve_key`] maps a key press to the same action.
//! Disabled controls leave the bar, but their key still answers with the reason.
use crate::app::{Action, App, HitRegion};
use crate::theme::{Role, Theme};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

/// Below this width every control with a compact label uses it.
pub const COMPACT_WIDTH: u16 = 100;
/// Pinned controls are never dropped for width; the Global tail uses it.
pub const PINNED: u8 = 0;
/// Columns before the first token.
const LEAD: u16 = 1;
/// Tokens are separated by two spaces, so `l Logs` reads as one token.
const GAP: u16 = 2;

/// Where a control belongs. Mode controls are view sub-modes and go to the tab strip
/// ([`crate::ui_chrome::tab_strip`]), never to the footer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlGroup {
    /// Moves within the view: Back, paging, panes.
    Navigate,
    /// Acts on the selection or the view: Refresh, Details, Save.
    Act,
    /// Selects a sub-mode: Runs/Workflows, Summary/Files.
    Mode,
    /// Help, Commands, Quit and Cancel.
    Global,
}

/// One keyboard control. `key` is the painted key token (`r`, `Esc`, `F10`, `Space`); the same
/// string drives [`resolve_key`], so what the bar shows is what the key does.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyControl<A = Action> {
    pub key: String,
    pub label: String,
    /// A shorter label used below [`COMPACT_WIDTH`].
    pub compact: Option<String>,
    pub action: A,
    /// Lower is more important. [`PINNED`] never leaves the bar; ties drop rightmost first.
    pub priority: u8,
    pub group: ControlGroup,
    /// Why the action is unavailable. A disabled control is not painted; its key explains.
    pub disabled: Option<String>,
    /// The active choice of a set, such as the current window or tab.
    pub selected: bool,
}

impl<A> KeyControl<A> {
    /// An enabled Act control with a middle priority.
    pub fn new(key: impl Into<String>, label: impl Into<String>, action: A) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            compact: None,
            action,
            priority: 50,
            group: ControlGroup::Act,
            disabled: None,
            selected: false,
        }
    }
    pub fn with_compact(mut self, compact: impl Into<String>) -> Self {
        self.compact = Some(compact.into());
        self
    }
    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }
    pub fn in_group(mut self, group: ControlGroup) -> Self {
        self.group = group;
        self
    }
    /// Disables the control when `reason` is set; `None` leaves it as it was.
    pub fn disabled_by(mut self, reason: Option<String>) -> Self {
        if reason.is_some() {
            self.disabled = reason;
        }
        self
    }
    pub fn with_selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
    pub fn label_text(&self, compact: bool) -> &str {
        match (&self.compact, compact) {
            (Some(short), true) => short,
            _ => &self.label,
        }
    }
    /// The painted token, `key label`, or the label alone for an unbound key.
    pub fn token(&self, compact: bool) -> String {
        if self.key.is_empty() {
            self.label_text(compact).to_owned()
        } else {
            format!("{} {}", self.key, self.label_text(compact))
        }
    }
    fn shown_in_footer(&self) -> bool {
        self.group != ControlGroup::Mode && self.disabled.is_none()
    }
}

/// One placed token: `controls[index]` at column offset `x` from the bar's left edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placed {
    pub index: usize,
    pub x: u16,
    pub width: u16,
    pub compact: bool,
}

fn token_width<A>(control: &KeyControl<A>, compact: bool) -> u16 {
    crate::editor::display_width(&control.token(compact)).min(usize::from(u16::MAX)) as u16
}

/// Fits the enabled, non-Mode controls into `width` columns, in declaration order. Below
/// [`COMPACT_WIDTH`] compact labels are used. While the row is too wide, the control with the
/// highest priority number leaves first (the rightmost on ties); pinned controls stay, and a
/// pinned token that cannot fit whole is left out rather than cut.
pub fn layout<A>(controls: &[KeyControl<A>], width: u16) -> Vec<Placed> {
    let compact = width < COMPACT_WIDTH;
    let mut shown = controls
        .iter()
        .enumerate()
        .filter(|(_, control)| control.shown_in_footer())
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let total = |shown: &[usize]| {
        u32::from(LEAD)
            + shown
                .iter()
                .map(|index| u32::from(token_width(&controls[*index], compact)))
                .sum::<u32>()
            + u32::from(GAP) * shown.len().saturating_sub(1) as u32
    };
    while total(&shown) > u32::from(width) {
        let Some(position) = shown
            .iter()
            .enumerate()
            .filter(|(_, index)| controls[**index].priority > PINNED)
            .max_by_key(|(position, index)| (controls[**index].priority, *position))
            .map(|(position, _)| position)
        else {
            break;
        };
        shown.remove(position);
    }
    let mut placed = Vec::with_capacity(shown.len());
    let mut x = LEAD;
    for index in shown {
        let token = token_width(&controls[index], compact);
        if u32::from(x) + u32::from(token) > u32::from(width) {
            continue;
        }
        placed.push(Placed {
            index,
            x,
            width: token,
            compact,
        });
        x = x.saturating_add(token).saturating_add(GAP);
    }
    placed
}

/// Paints the bar into `area` and returns each token's click target and action, in placement
/// order. `focus` is a placement position: that token is underlined (focus never relies on color).
pub fn render<A: Clone>(
    frame: &mut Frame,
    area: Rect,
    theme: Theme,
    controls: &[KeyControl<A>],
    focus: Option<usize>,
) -> Vec<(Rect, A)> {
    render_bar(frame, area, theme, controls, focus, false)
}

/// [`render`]; `inactive` paints every token muted, keys without their accent, for a bar that an open dialog has taken
/// the keys and clicks from.
fn render_bar<A: Clone>(
    frame: &mut Frame,
    area: Rect,
    theme: Theme,
    controls: &[KeyControl<A>],
    focus: Option<usize>,
    inactive: bool,
) -> Vec<(Rect, A)> {
    if area.is_empty() {
        return Vec::new();
    }
    let row = Rect::new(area.x, area.y, area.width, 1);
    frame.render_widget(Paragraph::new("").style(theme.base()), row);
    let mut targets = Vec::new();
    for (position, placed) in layout(controls, row.width).into_iter().enumerate() {
        let control = &controls[placed.index];
        let rect = Rect::new(row.x + placed.x, row.y, placed.width, 1);
        let mut key = theme.role(Role::Accent).add_modifier(Modifier::BOLD);
        let mut label = if control.selected {
            theme.role(Role::Text).add_modifier(Modifier::BOLD)
        } else {
            theme.role(Role::TextMuted)
        };
        if inactive {
            key = theme.role(Role::TextMuted);
            label = theme.role(Role::TextMuted);
        } else if focus == Some(position) {
            key = key.add_modifier(Modifier::UNDERLINED);
            label = label.add_modifier(Modifier::UNDERLINED);
        }
        let text = crate::app::safe_text(control.label_text(placed.compact));
        let spans = if control.key.is_empty() {
            vec![Span::styled(text, label)]
        } else {
            vec![
                Span::styled(crate::app::safe_text(&control.key), key),
                Span::styled(format!(" {text}"), label),
            ]
        };
        frame.render_widget(Paragraph::new(Line::from(spans)), rect);
        targets.push((rect, control.action.clone()));
    }
    targets
}

/// Paints the main key bar and pushes its hits after every other hit, so the footer stays last
/// in registration (and Tab) order.
pub fn draw(frame: &mut Frame, app: &mut App, area: Rect, controls: &[KeyControl]) {
    let first = app.hits.len();
    // While a view's content owns Enter, no bar token may look focused.
    let content_owns_enter = app.overlay.is_none()
        && match app.view {
            crate::app::View::Inbox => app.github_content_focused,
            crate::app::View::Repository => app.repository_content_focused,
            crate::app::View::Actions => app.actions_content_focused,
            crate::app::View::Dashboard => app.metrics_content_focused,
            crate::app::View::Search => app.search_content_focused,
        };
    let focus = app.focus.checked_sub(first).filter(|_| !content_owns_enter);
    // An open dialog (Help, Commands, a form…) owns every key and click until it closes, so
    // the bar under it is painted inactive (no key accent). The image viewer keeps the bar live.
    let inactive = app
        .overlay
        .is_some_and(|overlay| overlay != crate::app::Overlay::PrFileImages);
    let targets = render_bar(frame, area, app.theme(), controls, focus, inactive);
    app.hits.extend(
        targets
            .into_iter()
            .map(|(rect, action)| HitRegion { rect, action }),
    );
}

/// What a key press resolves to: the control's action, or the reason it is disabled.
#[derive(Clone, Debug, PartialEq)]
pub enum Resolved<A> {
    Act(A),
    Explain(String),
}

/// Whether `event` is the painted `key` token. Single characters match with or without Shift
/// (uppercase letters arrive shifted); Ctrl and Alt never match a plain key.
pub fn key_matches(key: &str, event: &KeyEvent) -> bool {
    if event.kind != KeyEventKind::Press {
        return false;
    }
    if let Some(letter) = key.strip_prefix("Ctrl-") {
        let mut letters = letter.chars();
        return match (letters.next(), letters.next()) {
            (Some(letter), None) => {
                event.modifiers.contains(KeyModifiers::CONTROL)
                    && matches!(event.code, KeyCode::Char(c) if c.eq_ignore_ascii_case(&letter))
            }
            _ => false,
        };
    }
    if event
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    {
        return false;
    }
    let named = match key.to_ascii_lowercase().as_str() {
        "esc" => Some(KeyCode::Esc),
        "enter" => Some(KeyCode::Enter),
        "tab" => Some(KeyCode::Tab),
        "space" => Some(KeyCode::Char(' ')),
        "backspace" => Some(KeyCode::Backspace),
        "del" | "delete" => Some(KeyCode::Delete),
        "pgup" => Some(KeyCode::PageUp),
        "pgdn" => Some(KeyCode::PageDown),
        "home" => Some(KeyCode::Home),
        "end" => Some(KeyCode::End),
        "up" | "↑" => Some(KeyCode::Up),
        "down" | "↓" => Some(KeyCode::Down),
        "left" | "←" => Some(KeyCode::Left),
        "right" | "→" => Some(KeyCode::Right),
        name => name
            .strip_prefix('f')
            .and_then(|number| number.parse::<u8>().ok())
            .filter(|number| (1..=24).contains(number))
            .map(KeyCode::F),
    };
    if let Some(code) = named {
        return event.code == code;
    }
    let mut characters = key.chars();
    match (characters.next(), characters.next()) {
        (Some(character), None) => event.code == KeyCode::Char(character),
        _ => false,
    }
}

/// The control bound to `event`, including Mode and disabled ones. A disabled key resolves to
/// its reason, never to nothing.
pub fn resolve<A: Clone>(controls: &[KeyControl<A>], event: &KeyEvent) -> Option<Resolved<A>> {
    let control = controls
        .iter()
        .find(|control| !control.key.is_empty() && key_matches(&control.key, event))?;
    Some(match &control.disabled {
        Some(reason) => Resolved::Explain(reason.clone()),
        None => Resolved::Act(control.action.clone()),
    })
}

/// [`resolve`] for app actions: a disabled key becomes `Action::Explain(reason)`.
pub fn resolve_key(controls: &[KeyControl], event: KeyEvent) -> Option<Action> {
    resolve(controls, &event).map(|resolved| match resolved {
        Resolved::Act(action) => action,
        Resolved::Explain(reason) => Action::Explain(reason),
    })
}

/// The Global tail every main frame ends with: `? Help  : Commands  q Quit`, preceded by
/// `Esc Cancel` while the current view is loading. Keys follow the configured bindings.
pub fn global_tail(app: &App) -> Vec<KeyControl> {
    let mut tail = Vec::new();
    // TASK-120: while mouse capture is released the bar says so and how to restore it.
    tail.extend(crate::clipboard::footer_control(app));
    if app.loading[app.view.index()] || app.navigation_restore_inflight {
        tail.push(global("Esc", "Cancel", Action::Cancel));
    }
    tail.push(global(app.shortcut("help"), "Help", Action::Help));
    tail.push(global(app.shortcut("palette"), "Commands", Action::Palette).with_compact("Cmds"));
    tail.push(global(app.shortcut("quit"), "Quit", Action::Quit));
    tail
}

fn global(key: &str, label: &str, action: Action) -> KeyControl {
    KeyControl::new(key, label, action)
        .with_priority(PINNED)
        .in_group(ControlGroup::Global)
}

/// Keys for views whose toolbars are not converted to the key bar yet: Refresh and Settings
/// stay one press or click away. They leave before any pinned Global control when the row is
/// narrow (Settings first).
pub fn transitional_keys(app: &App) -> Vec<KeyControl> {
    vec![
        KeyControl::new(app.shortcut("refresh"), "Refresh", Action::Refresh)
            .with_priority(60)
            .in_group(ControlGroup::Global),
        KeyControl::new(app.shortcut("settings"), "Settings", Action::Settings)
            .with_priority(70)
            .in_group(ControlGroup::Global),
    ]
}

/// Whether the Inbox/PR detail view draws its own content and so owns the key bar.
fn inbox_adapter(app: &App) -> bool {
    app.overlay.is_none()
        && app.github.is_some()
        && !crate::scoped_pr_collaboration_native_v17::retired_presentation(app)
}

/// The controls of the main frame's key bar: the focused view's keys, then the Global tail.
pub fn main_controls(app: &App) -> Vec<KeyControl> {
    let mut controls = match (app.view, &app.metrics) {
        (crate::app::View::Dashboard, Some(metrics)) if app.overlay.is_none() => {
            crate::metrics_ui::footer_keys(metrics)
        }
        (crate::app::View::Actions, _) if app.overlay.is_none() && app.actions.is_some() => {
            crate::actions_ui::footer_keys(app.actions.as_ref().unwrap())
        }
        (crate::app::View::Inbox, _) if inbox_adapter(app) => crate::github_ui::footer_keys(app),
        (crate::app::View::Repository, _)
            if app.overlay.is_none()
                && (app.workspace.is_some() || app.core_worktree_v118.is_some())
                && app.remote_repository.is_none() =>
        {
            crate::repository_ui::footer_keys(app)
        }
        // Search has no Refresh of its own; the global Refresh and Settings keys stay last.
        (crate::app::View::Search, _) if app.overlay.is_none() => {
            let mut keys = crate::search_ui::footer_keys(app);
            keys.extend(transitional_keys(app));
            keys
        }
        _ => transitional_keys(app),
    };
    // TASK-120: `y Copy` where the focused item has a natural value (lowest priority).
    if app.overlay.is_none()
        && matches!(
            app.view,
            crate::app::View::Inbox | crate::app::View::Actions | crate::app::View::Repository
        )
    {
        let copy = crate::clipboard::copy_control(app, &controls);
        controls.extend(copy);
    }
    controls.extend(global_tail(app));
    controls
}

#[cfg(test)]
#[path = "footer_tests.rs"]
mod tests;
