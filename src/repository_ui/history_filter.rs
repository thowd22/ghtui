//! The History filter row (TASK-99.3): `/ query  path: src/ ✕  author: ada ✕  all branches`
//! inside the History pane, `N matches · n next` in its border, and the search, match, hide
//! and scope keys. Every chip is removable by its `✕` and by Backspace on an empty query, and
//! every key is also a clickable footer token with the same action. Drawing reads no Git;
//! applying a chip or a scope runs on the worker through `GraphActionV1::Filter`.
use crate::{
    app::{Action, App, HitRegion, View},
    footer::{ControlGroup, KeyControl},
    git::HistoryRefKindV1 as Kind,
    history::filter::{
        HistoryChipV993 as Chip, HistoryFilterSpecV993 as Spec, HistoryScopeV993 as Scope,
        MAX_QUERY,
    },
    repository_graph_v1::{GraphActionV1, GraphViewV1},
    theme::{Role, Theme},
    workspace::{RowAction, Workspace},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistoryFilterActionV993 {
    /// Focus the query (`/`, or a click on it).
    Edit,
    /// The next (`n`) or previous (`N`) matching loaded commit, wrapping.
    Next,
    Previous,
    /// Hide non-matching commits, or show them dimmed again (`h`).
    ToggleHide,
    /// All branches ↔ the selected commit's branch (`B`, or a click on the scope chip).
    Scope,
    /// Remove one chip (its `✕`, or Backspace on an empty query for the last one).
    Remove(Chip),
    /// Remove the text and every chip (`Esc`).
    Clear,
}
use HistoryFilterActionV993 as A;

/// Placeholder of an empty, unfocused query.
pub const PLACEHOLDER: &str = "search message · author · SHA · author: · path: · branch:";
/// Placeholder of an empty, unfocused query beside chips.
pub const SHORT_PLACEHOLDER: &str = "search";

fn timeline(workspace: &Workspace) -> bool {
    workspace.graph_visible_v1()
        && workspace.graph_view_v1 == GraphViewV1::Timeline
        && workspace.graph_display_v1.is_some()
}

/// The History timeline is the Repository body and no overlay owns input.
pub fn active(app: &App) -> bool {
    app.view == View::Repository
        && app.overlay.is_none()
        && app.workspace.as_ref().is_some_and(timeline)
}

/// The query has keyboard focus.
pub fn editing(app: &App) -> bool {
    app.history_filter_input_v993.is_some() && active(app)
}

fn spec(app: &App) -> Spec {
    app.workspace
        .as_ref()
        .map(|workspace| workspace.history_filter_v993.spec.clone())
        .unwrap_or_default()
}

/// Why `action` cannot run now; None when it can.
pub fn reason(app: &App, action: &HistoryFilterActionV993) -> Option<String> {
    let Some(workspace) = app
        .workspace
        .as_ref()
        .filter(|workspace| timeline(workspace))
    else {
        return Some("Open History (g) first".into());
    };
    if app.loading[View::Repository.index()] {
        return Some(crate::repository_graph_ui_v1::BUSY.into());
    }
    let filter = &workspace.history_filter_v993;
    match action {
        A::Next | A::Previous if !filter.spec.matching() => {
            Some("Search first: / then text, author:NAME or path:PATH".into())
        }
        A::Next | A::Previous if filter.count() == 0 => {
            Some("No loaded commit matches • L loads more history".into())
        }
        A::ToggleHide if !filter.spec.matching() => {
            Some("Search or add a chip before hiding the commits that do not match".into())
        }
        A::Clear if filter.spec.is_empty() => Some("No search or chip to clear".into()),
        A::Remove(chip) if !filter.spec.order.contains(chip) => {
            Some("That chip is not applied".into())
        }
        _ => None,
    }
}

/// Footer tokens of the History filter, most important first. Esc only while something
/// filters (and nothing loads), so it never shadows Cancel.
pub fn controls(app: &App) -> Vec<KeyControl> {
    if !app.workspace.as_ref().is_some_and(timeline) {
        return Vec::new();
    }
    let spec = spec(app);
    let control = |key: &str, label: &str, compact: &str, action: A, priority: u8| {
        KeyControl::new(key, label, Action::HistoryFilterV993(action.clone()))
            .with_compact(compact)
            .with_priority(priority)
            .disabled_by(reason(app, &action))
    };
    let mut controls = vec![
        control("/", "Search", "Search", A::Edit, 3),
        control("n", "Next match", "Next", A::Next, 4),
        control("N", "Previous match", "Prev", A::Previous, 13),
        // `b` is the Branch mode tab; the scope takes Shift-B.
        control(
            "B",
            if spec.scope == Scope::All {
                "Branch scope"
            } else {
                "All branches"
            },
            if spec.scope == Scope::All {
                "Scope"
            } else {
                "All"
            },
            A::Scope,
            10,
        )
        .in_group(ControlGroup::Navigate),
        control(
            "h",
            if spec.hide {
                "Show non-matches"
            } else {
                "Hide non-matches"
            },
            if spec.hide { "Show" } else { "Hide" },
            A::ToggleHide,
            12,
        ),
    ];
    if !spec.is_empty() && !app.loading[View::Repository.index()] {
        controls.insert(1, control("Esc", "Clear filter", "Clear", A::Clear, 2));
    }
    controls
}

fn apply(app: &mut App, spec: Spec) {
    app.dispatch(Action::RepositoryRow(RowAction::Graph(
        GraphActionV1::Filter(spec),
    )));
}

/// The scope `B` picks: back to every branch, or the selected commit's branch (a local
/// branch first, then a remote branch, else the checked-out HEAD).
fn next_scope(app: &App) -> Scope {
    let Some(workspace) = app.workspace.as_ref() else {
        return Scope::All;
    };
    if workspace.history_filter_v993.spec.scope != Scope::All {
        return Scope::All;
    }
    let chips = cursor_id(app)
        .and_then(|id| workspace.graph_state_v1.as_ref()?.observed(&id))
        .map(|observed| observed.chips().to_vec())
        .unwrap_or_default();
    [Kind::Local, Kind::Remote]
        .into_iter()
        .find_map(|kind| chips.iter().find(|chip| chip.kind == kind))
        .map_or(Scope::Current, |chip| Scope::Branch(chip.name.clone()))
}

/// The commit under the `▌` bar, else the graph selection.
fn cursor_id(app: &App) -> Option<String> {
    let workspace = app.workspace.as_ref()?;
    match workspace
        .display_rows_v1()
        .get(app.repository_list.selected)
        .and_then(|row| row.action.as_ref())
    {
        Some(RowAction::Commit(id)) => Some(id.clone()),
        _ => Some(workspace.revision.clone()),
    }
}

pub(crate) fn dispatch(app: &mut App, action: HistoryFilterActionV993) {
    if let Some(reason) = reason(app, &action) {
        app.dispatch(Action::Explain(reason));
        return;
    }
    app.repository_content_focused = true;
    app.user_navigated = true;
    if matches!(action, A::Next | A::Previous | A::ToggleHide | A::Scope) {
        // Acting elsewhere ends typing; the text already applied stays.
        app.history_filter_input_v993 = None;
    }
    match action {
        A::Edit => {
            app.history_filter_input_v993 = Some(spec(app).text);
            app.status =
                "Search History • text, author:NAME, path:PATH or branch:NAME • Enter applies • Esc closes"
                    .into();
        }
        A::Next | A::Previous => step(app, action == A::Next),
        A::ToggleHide => {
            let mut spec = spec(app);
            spec.hide = !spec.hide;
            apply(app, spec);
        }
        A::Scope => {
            let mut spec = spec(app);
            spec.set_scope(next_scope(app));
            apply(app, spec);
        }
        A::Remove(chip) => {
            let mut spec = spec(app);
            spec.remove(chip);
            apply(app, spec);
        }
        A::Clear => {
            app.history_filter_input_v993 = None;
            apply(app, Spec::default());
        }
    }
}

/// Moves the bar to the next or previous match. A match in the painted window moves the bar
/// only; one in another loaded window re-centres the window on it (no Git read).
fn step(app: &mut App, forward: bool) {
    let Some(workspace) = app.workspace.as_ref() else {
        return;
    };
    let Some(state) = workspace.graph_state_v1.clone() else {
        return;
    };
    let filter = &workspace.history_filter_v993;
    let from = cursor_id(app)
        .and_then(|id| state.loaded_member_v2(&id))
        .map_or(0, |member| member.node as usize);
    let Some(target) = filter.step(&state, from, forward) else {
        return;
    };
    let Some(commit) = state
        .observed_at_v993(target)
        .map(|row| row.commit().clone())
    else {
        return;
    };
    let flags = filter.flags(&state).unwrap_or_default();
    let ordinal = flags[..=target].iter().filter(|flag| **flag).count();
    let wrapped = if forward {
        target <= from
    } else {
        target >= from
    };
    let status = format!(
        "Match {ordinal} of {}{} • {} {}",
        filter.count(),
        if wrapped && target != from {
            " • wrapped"
        } else {
            ""
        },
        crate::ui_table::short_sha(&commit.id),
        crate::app::safe_text(&commit.subject),
    );
    match workspace.graph_action_index_v1(&RowAction::Commit(commit.id.clone())) {
        Some(row) => {
            app.repository_list.selected = row;
            let visible = app.repository_visible_rows.max(1);
            // A match off screen lands mid-view with context on both sides, not on an edge.
            if row < app.repository_list.top || row >= app.repository_list.top + visible {
                app.repository_list.top = row.saturating_sub(visible / 2);
            }
            app.status = status;
        }
        None => {
            app.dispatch(Action::RepositoryRow(RowAction::Commit(commit.id)));
            app.status = status;
        }
    }
}

/// Keys while the query has focus. Returns true when the key was consumed.
pub(crate) fn key(app: &mut App, key: KeyEvent) -> bool {
    if app.history_filter_input_v993.is_none() {
        return false;
    }
    if !active(app) {
        app.history_filter_input_v993 = None;
        return false;
    }
    if key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    {
        return false;
    }
    match key.code {
        KeyCode::Char(character) => {
            let draft = app.history_filter_input_v993.as_mut().unwrap();
            if draft.len() + character.len_utf8() <= MAX_QUERY && !character.is_control() {
                draft.push(character);
            }
            live(app);
        }
        KeyCode::Backspace => {
            let draft = app.history_filter_input_v993.as_mut().unwrap();
            if draft.pop().is_some() {
                live(app);
            } else if let Some(chip) = spec(app).last_chip() {
                dispatch(app, A::Remove(chip));
            }
        }
        KeyCode::Enter => {
            let draft = app.history_filter_input_v993.take().unwrap_or_default();
            let next = spec(app).with_query(&draft);
            if app.loading[View::Repository.index()] {
                app.dispatch(Action::Explain(crate::repository_graph_ui_v1::BUSY.into()));
                return true;
            }
            apply(app, next);
        }
        KeyCode::Esc => {
            app.history_filter_input_v993 = None;
            app.status = "Search closed • n/N step through matches • Esc clears the filter".into();
        }
        // Moving or tabbing leaves the query; the key then acts as usual.
        _ => {
            app.history_filter_input_v993 = None;
            return false;
        }
    }
    true
}

/// Highlight mode matches while typing: the text applies to the loaded commits at once. Hide
/// mode waits for Enter, since it rebuilds the rows.
fn live(app: &mut App) {
    if app.loading[View::Repository.index()] {
        return;
    }
    let draft = app.history_filter_input_v993.clone().unwrap_or_default();
    let Some(workspace) = app.workspace.as_mut() else {
        return;
    };
    let Some(state) = workspace.graph_state_v1.clone() else {
        return;
    };
    if workspace.history_filter_v993.spec.hide || draft.contains(':') {
        return;
    }
    let position = workspace.graph_position_v2().ok();
    let filter = &mut workspace.history_filter_v993;
    if filter.spec.is_empty() && filter.saved.is_none() {
        filter.saved = position.map(|position| crate::history::filter::SavedHistoryV993 {
            state: state.clone(),
            position,
        });
    }
    filter.spec.text = draft.trim().to_owned();
    filter.sync(&state);
}

/// The scope chip's label: `all branches`, `branch: main`, or the checked-out branch.
pub fn scope_label(workspace: &Workspace) -> String {
    match &workspace.history_filter_v993.spec.scope {
        Scope::All => "all branches".into(),
        Scope::Current => format!(
            "branch: {}",
            workspace.status.branch.as_deref().unwrap_or("HEAD")
        ),
        Scope::Branch(name) => format!("branch: {name}"),
    }
}

/// The breadcrumb after `History ›`.
pub fn title_scope(workspace: &Workspace) -> String {
    match &workspace.history_filter_v993.spec.scope {
        Scope::All => crate::repository_ui::history_table::SCOPE.into(),
        _ => scope_label(workspace)
            .trim_start_matches("branch: ")
            .to_owned(),
    }
}

/// The filter row is shown while the query has focus or anything filters.
pub fn shown(app: &App) -> bool {
    editing(app)
        || app
            .workspace
            .as_ref()
            .is_some_and(|workspace| !workspace.history_filter_v993.spec.is_empty())
}

/// Draws the filter row at `area` (one line) and registers its click targets.
pub(crate) fn draw_row(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(workspace) = app.workspace.as_ref() else {
        return;
    };
    let theme = app.theme();
    let spec = &workspace.history_filter_v993.spec;
    let base = theme.role(Role::Text);
    let muted = theme.role(Role::TextMuted);
    let accent = theme.role(Role::Accent).add_modifier(Modifier::BOLD);
    let width = usize::from(area.width.saturating_sub(1));
    let mut pieces: Vec<(String, Style, Option<Action>)> = Vec::new();
    pieces.push((
        "/ ".into(),
        accent,
        Some(Action::HistoryFilterV993(A::Edit)),
    ));
    let query = match &app.history_filter_input_v993 {
        Some(draft) => Some((
            format!("{}▏", crate::app::safe_text(draft)),
            base.add_modifier(Modifier::BOLD),
        )),
        None => spec.text().map(|text| {
            (
                crate::app::safe_text(text),
                base.add_modifier(Modifier::BOLD),
            )
        }),
    };
    // An empty query keeps a muted prompt: the full hint alone, a short one beside chips (so
    // `/` never floats before a gap).
    let query = query.or_else(|| {
        Some((
            if spec.order.is_empty() && spec.scope == Scope::All {
                PLACEHOLDER
            } else {
                SHORT_PLACEHOLDER
            }
            .to_owned(),
            muted,
        ))
    });
    if let Some((text, style)) = query {
        pieces.push((
            crate::ui_chrome::fit(&text, (width / 2).max(8)),
            style,
            Some(Action::HistoryFilterV993(A::Edit)),
        ));
    }
    let chip_style = theme.soft_chip(Role::Info);
    let remove = theme.role(Role::Accent).add_modifier(Modifier::BOLD);
    let chip = |pieces: &mut Vec<(String, Style, Option<Action>)>,
                label: String,
                click: A,
                removable: Option<Chip>| {
        pieces.push(("  ".into(), base, None));
        pieces.push((
            format!(" {} ", crate::ui_chrome::fit(&label, 40)),
            chip_style,
            Some(Action::HistoryFilterV993(click)),
        ));
        if let Some(chip) = removable {
            pieces.push((
                "✕".into(),
                remove,
                Some(Action::HistoryFilterV993(A::Remove(chip))),
            ));
        }
    };
    for kind in &spec.order {
        match kind {
            Chip::Path => {
                if let Some(path) = &spec.path {
                    chip(
                        &mut pieces,
                        format!(
                            "path: {}",
                            crate::app::safe_text(&String::from_utf8_lossy(&path.0))
                        ),
                        A::Edit,
                        Some(Chip::Path),
                    );
                }
            }
            Chip::Author => {
                if let Some(author) = &spec.author {
                    chip(
                        &mut pieces,
                        format!("author: {}", crate::app::safe_text(author)),
                        A::Edit,
                        Some(Chip::Author),
                    );
                }
            }
            Chip::Scope => {}
        }
    }
    chip(
        &mut pieces,
        scope_label(workspace),
        A::Scope,
        (spec.scope != Scope::All).then_some(Chip::Scope),
    );
    let hint = if spec.matching() {
        if spec.hide {
            "non-matches hidden · h show"
        } else {
            "non-matches dimmed · h hide"
        }
    } else {
        "B scope · Esc clears"
    };
    pieces.push(("  ".into(), base, None));
    // While something matches, the hint is also the click target of `h`.
    pieces.push((
        hint.into(),
        muted,
        spec.matching()
            .then_some(Action::HistoryFilterV993(A::ToggleHide)),
    ));
    let mut x = area.x + 1;
    let right = area.right();
    let mut spans = Vec::new();
    let mut hits = Vec::new();
    for (text, style, action) in pieces {
        let cells = crate::editor::display_width(&text) as u16;
        if x + cells > right {
            // Whole pieces only, so a chip never loses its ✕.
            break;
        }
        if let Some(action) = action {
            hits.push(HitRegion {
                rect: Rect::new(x, area.y, cells, 1),
                action,
            });
        }
        spans.push(Span::styled(text, style));
        x += cells;
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(theme.base()),
        Rect::new(area.x + 1, area.y, area.width.saturating_sub(1), 1),
    );
    app.hits.extend(hits);
}

/// `2 matches · n next` drawn into the pane's top rule, right-aligned; click steps to the next
/// match.
pub(crate) fn draw_count(frame: &mut Frame, app: &mut App, rule: Rect) {
    let Some(workspace) = app.workspace.as_ref() else {
        return;
    };
    let filter = &workspace.history_filter_v993;
    if !filter.spec.matching() || rule.width < 30 {
        return;
    }
    let theme: Theme = app.theme();
    let count = filter.count();
    let capped = if filter.chip_capped() { "+" } else { "" };
    let text = match count {
        0 => " no matches loaded · L more ".to_owned(),
        1 => format!(" 1{capped} match · n next "),
        count => format!(" {count}{capped} matches · n next "),
    };
    let cells = crate::editor::display_width(&text) as u16;
    if cells + 24 > rule.width {
        return;
    }
    let area = Rect::new(rule.right() - cells - 2, rule.y, cells, 1);
    frame.render_widget(
        Paragraph::new(text).style(theme.role(Role::Accent).add_modifier(Modifier::BOLD)),
        area,
    );
    if count > 0 {
        app.hits.push(HitRegion {
            rect: area,
            action: Action::HistoryFilterV993(A::Next),
        });
    }
}

#[cfg(test)]
#[path = "history_filter_tests.rs"]
mod history_filter_v993_ui_tests;
