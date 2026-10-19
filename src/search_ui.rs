//! Local input and prepared rendering only: no subprocess, disk or network I/O.
use crate::{
    app::{safe_text, Action, App, HitRegion},
    editor::TextBuffer,
    footer::{ControlGroup, KeyControl},
    search::{corpus::LocalSource, workspace::SearchAction, SearchMode},
    theme::{Role, SemanticToken},
    ui_table::{draw_table, Cell, Column, ListState, Table, TableRow},
};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

/// Search results: the identifying title first, then where the match is, then the repository.
const RESULT_COLUMNS: [Column; 3] = [
    Column::new("RESULT", 12).flex(3).priority(0),
    Column::new("LINE", 4).priority(1).right(),
    // Fixed and least important: it leaves before a result title is cut.
    Column::new("REPOSITORY", 8).priority(2),
];

/// The snippet with its highlighted ranges, cut to `width` cells with an ellipsis.
fn snippet_spans(
    snippet: &str,
    highlights: &[std::ops::Range<usize>],
    width: usize,
    emphasis: ratatui::style::Style,
) -> Vec<Span<'static>> {
    use unicode_segmentation::UnicodeSegmentation;
    let mut pieces = Vec::new();
    let mut at = 0;
    for range in highlights {
        if range.start >= at
            && range.end <= snippet.len()
            && snippet.is_char_boundary(range.start)
            && snippet.is_char_boundary(range.end)
        {
            pieces.push((safe_text(&snippet[at..range.start]), false));
            pieces.push((safe_text(&snippet[range.clone()]), true));
            at = range.end;
        }
    }
    pieces.push((safe_text(&snippet[at..]), false));
    let total = pieces
        .iter()
        .map(|(text, _)| crate::editor::display_width(text))
        .sum::<usize>();
    let style = |emphasized: bool| {
        if emphasized {
            emphasis
        } else {
            ratatui::style::Style::default()
        }
    };
    if total <= width {
        return pieces
            .into_iter()
            .map(|(text, emphasized)| Span::styled(text, style(emphasized)))
            .collect();
    }
    let mut budget = width.saturating_sub(1);
    let mut spans = Vec::new();
    for (text, emphasized) in pieces {
        let mut kept = String::new();
        for grapheme in text.graphemes(true) {
            let cells = crate::editor::grapheme_width(grapheme);
            if cells > budget {
                budget = 0;
                break;
            }
            budget -= cells;
            kept.push_str(grapheme);
        }
        let cut = budget == 0;
        spans.push(Span::styled(kept, style(emphasized)));
        if cut {
            if width > 0 {
                spans.push(Span::styled("…", style(emphasized)));
            }
            break;
        }
    }
    spans
}

struct RenderHit {
    index: usize,
    title: String,
    repository: String,
    line: usize,
    column: usize,
    snippet: String,
    highlights: Vec<std::ops::Range<usize>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchField {
    Query,
    Revision,
    Name,
}
#[derive(Clone, Debug)]
pub struct SearchControls {
    pub query: TextBuffer,
    pub revision: TextBuffer,
    pub name: TextBuffer,
    pub editing: Option<SearchField>,
    pub fields: Vec<(SearchField, Rect)>,
    pub corpus: bool,
    pub selected: usize,
    pub top: usize,
    pub visible: usize,
    pub preview_scroll: usize,
    pub corpus_index: usize,
    pub preview_rect: Rect,
}
impl Default for SearchControls {
    fn default() -> Self {
        Self {
            query: Default::default(),
            revision: TextBuffer::new("HEAD").unwrap(),
            name: TextBuffer::new("My search").unwrap(),
            editing: None,
            fields: vec![],
            corpus: false,
            selected: 0,
            top: 0,
            visible: 1,
            preview_scroll: 0,
            corpus_index: 0,
            preview_rect: Rect::default(),
        }
    }
}
impl SearchControls {
    fn editor(&mut self, field: SearchField) -> &mut TextBuffer {
        match field {
            SearchField::Query => &mut self.query,
            SearchField::Revision => &mut self.revision,
            SearchField::Name => &mut self.name,
        }
    }
}

/// Why the corpus pane cannot change GitHub discovery scope.
const GITHUB_CORPUS: &str = "GitHub repository scope comes from --repo, the selected saved repository set, the launch checkout's origin, or explicit mapped repositories; a typed repo:owner/name or org:name qualifier replaces it for that query. Provider indexing limits apply. Use Exact, Regex or Ranked to select local hidden/ignored files; remote scope is never broadened automatically.";
/// Why GitHub mode has no working-file or saved-version source.
const GITHUB_SOURCE: &str = "GitHub discovery searches its indexed default-branch data. Choose Exact, Regex or Ranked for local working files or saved revisions; no remote revision or regex semantics are substituted.";
const LOADING_OPEN: &str = "Search is still loading • wait for this query before opening a result";
const NO_RESULT: &str = "Select a result from the exact current query/source first. Saved query intent is not a result or access permit.";
const NO_CHECKOUT: &str = "No existing checkout is listed • map one through M Repositories; nothing is cloned or fetched automatically.";

fn search_mode(app: &App) -> SearchMode {
    app.search
        .as_ref()
        .map_or(SearchMode::Exact, |search| search.mode)
}

/// Whether every selected repository searches its working files rather than a saved version.
fn worktree_source(app: &App) -> bool {
    app.search.as_ref().is_none_or(|search| {
        search
            .selections
            .iter()
            .all(|selection| selection.source == LocalSource::Worktree)
    })
}

/// The query modes, drawn as the first tabs of the Search pane.
pub fn mode_tabs(app: &App) -> Vec<KeyControl> {
    let mode = search_mode(app);
    [
        ("e", "Exact", SearchMode::Exact),
        ("x", "Regex", SearchMode::Regex),
        ("n", "Ranked", SearchMode::Ranked),
        ("h", "GitHub", SearchMode::GitHub),
    ]
    .into_iter()
    .map(|(key, label, value)| {
        KeyControl::new(key, label, Action::Search(SearchAction::Mode(value)))
            .in_group(ControlGroup::Mode)
            .with_selected(mode == value)
    })
    .collect()
}

/// The local source choice, drawn after the mode tabs. GitHub discovery has no local source,
/// so both tabs dim there and explain why.
pub fn source_tabs(app: &App) -> Vec<KeyControl> {
    let remote = search_mode(app) == SearchMode::GitHub;
    let worktree = worktree_source(app);
    let reason = remote.then(|| GITHUB_SOURCE.to_owned());
    vec![
        KeyControl::new("w", "Working files", Action::Search(SearchAction::Worktree))
            .in_group(ControlGroup::Mode)
            .with_selected(worktree && !remote)
            .disabled_by(reason.clone()),
        KeyControl::new(
            "v",
            "Saved version",
            Action::Search(SearchAction::SavedVersion),
        )
        .in_group(ControlGroup::Mode)
        .with_selected(!worktree && !remote)
        .disabled_by(reason),
    ]
}

/// The Search key bar, most important first, after its Mode tabs (which [`crate::footer`]
/// leaves to the tab strip). `/ Query`, `d Revision` and `z Name` stay labelled input fields
/// and selection keys stay in [`handle_event`]. At 80 columns the bar keeps Search, Open,
/// Repositories and Sources (Corpus instead of Open before a result); the rest stay on their
/// keys, in Help and in the palette.
pub fn footer_keys(app: &App) -> Vec<KeyControl> {
    let corpus = app.search_controls.corpus;
    let remote = search_mode(app) == SearchMode::GitHub;
    let loading = app.loading[crate::app::View::Search.index()];
    let mut keys = mode_tabs(app);
    keys.extend(source_tabs(app));
    if corpus {
        let flag = |value: bool| if value { "on" } else { "off" };
        let limits = app.search.as_ref().map(|search| &search.corpus_limits);
        let hidden = limits.is_some_and(|limits| limits.include_hidden);
        let ignored = limits.is_some_and(|limits| limits.include_ignored);
        let current = app.search.as_ref().and_then(|search| {
            search
                .available_selections
                .get(app.search_controls.corpus_index)
                .map(|selection| selection.repository.clone())
        });
        keys.push(
            KeyControl::new(
                "Enter",
                "Toggle",
                Action::Search(SearchAction::ToggleRepository(
                    current.clone().unwrap_or_default(),
                )),
            )
            .with_priority(5)
            .disabled_by(current.is_none().then(|| NO_CHECKOUT.to_owned())),
        );
        keys.push(
            KeyControl::new(
                "b",
                format!("Hidden files: {}", flag(hidden)),
                Action::Search(SearchAction::ToggleHidden),
            )
            .with_compact(format!("Hidden: {}", flag(hidden)))
            .with_priority(10)
            .with_selected(hidden),
        );
        keys.push(
            KeyControl::new(
                "i",
                format!("Ignored files: {}", flag(ignored)),
                Action::Search(SearchAction::ToggleIgnored),
            )
            .with_compact(format!("Ignored: {}", flag(ignored)))
            .with_priority(12)
            .with_selected(ignored),
        );
    } else {
        keys.push(
            KeyControl::new("Enter", "Search", Action::Search(SearchAction::Submit))
                .with_priority(5),
        );
    }
    let has_result = app
        .search
        .as_ref()
        .and_then(|search| search.page.as_ref())
        .is_some_and(|page| page.hits.get(app.search_controls.selected).is_some());
    keys.push(
        KeyControl::new(
            "o",
            "Open result",
            Action::Search(SearchAction::Open(app.search_controls.selected)),
        )
        .with_priority(10)
        .disabled_by(if loading {
            Some(LOADING_OPEN.to_owned())
        } else {
            (!has_result).then(|| NO_RESULT.to_owned())
        }),
    );
    // While the corpus pane is open, `c` is how it closes, so it outranks everything else.
    keys.push(
        KeyControl::new("c", "Corpus", Action::Search(SearchAction::Corpus))
            .with_priority(if corpus { 8 } else { 20 })
            .with_selected(corpus)
            .disabled_by((remote && !corpus).then(|| GITHUB_CORPUS.to_owned())),
    );
    keys.push(KeyControl::new("M", "Repositories", Action::Corpus).with_priority(15));
    keys.push(
        KeyControl::new("f", "Save query", Action::Search(SearchAction::Save)).with_priority(35),
    );
    keys.push(
        KeyControl::new(
            "H",
            "Sources",
            Action::SearchSource(crate::search_sources::SourceAction::Open),
        )
        .with_priority(18),
    );
    keys.push(
        KeyControl::new("B", "Blend", Action::Search(SearchAction::BlendControls))
            .with_priority(45),
    );
    if !corpus {
        let enabled = app
            .search
            .as_ref()
            .is_some_and(|search| search.blend_policy.enabled);
        let state = if enabled { "on" } else { "off" };
        keys.push(
            KeyControl::new(
                "b",
                format!("Blend GitHub: {state}"),
                Action::SearchBlend(crate::search::fusion::ui::BlendAction::Toggle),
            )
            .with_compact(format!("GitHub: {state}"))
            .with_priority(55)
            .with_selected(enabled)
            .disabled_by(crate::search_blend::toggle_disabled(app)),
        );
    }
    keys
}

pub fn dispatch(app: &mut App, action: SearchAction) {
    use SearchAction::*;
    match &action {
        BlendControls => crate::search_blend::open(app),
        BlendPreferences => app.search_action = Some(action),
        Query => app.search_controls.editing = Some(SearchField::Query),
        Revision => app.search_controls.editing = Some(SearchField::Revision),
        Name => app.search_controls.editing = Some(SearchField::Name),
        Corpus => {
            // Closing an open corpus pane is always allowed; only opening it is local-only.
            if !app.search_controls.corpus && search_mode(app) == SearchMode::GitHub {
                app.dispatch(Action::Explain(GITHUB_CORPUS.into()));
                return;
            }
            app.search_controls.corpus = !app.search_controls.corpus;
            app.search_controls.editing = None;
        }
        Preview(index) => {
            if app.loading[crate::app::View::Search.index()] {
                app.status =
                    "Search is still loading • wait for this query before previewing results"
                        .into();
                return;
            }
            if let Some(search) = &mut app.search {
                match search.preview(*index) {
                    Ok(()) => {
                        app.search_controls.selected = *index;
                        app.search_controls.preview_scroll = 0;
                        app.search_controls.editing = None;
                    }
                    Err(error) => app.status = error.to_string(),
                }
            }
        }
        Mode(mode) => {
            if let Some(search) = &mut app.search {
                search.set_mode(*mode);
            }
            reset_results(app);
            if *mode == SearchMode::GitHub || app.search.is_none() {
                app.search_action = Some(action.clone());
            }
        }
        Worktree | SavedVersion => {
            if app
                .search
                .as_ref()
                .is_some_and(|search| search.mode == SearchMode::GitHub)
            {
                app.dispatch(Action::Explain(GITHUB_SOURCE.into()));
                return;
            }
            if let Some(search) = &mut app.search {
                search.set_source(if action == Worktree {
                    LocalSource::Worktree
                } else {
                    LocalSource::Revision(app.search_controls.revision.text.trim().into())
                });
                app.context = search.context();
            }
            reset_results(app);
        }
        ToggleHidden | ToggleIgnored => {
            if let Some(search) = &mut app.search {
                if action == ToggleHidden {
                    search.corpus_limits.include_hidden = !search.corpus_limits.include_hidden;
                } else {
                    search.corpus_limits.include_ignored = !search.corpus_limits.include_ignored;
                }
                search.page = None;
                search.preview = None;
            }
            reset_results(app);
        }
        ToggleRepository(repository) => {
            if let Some(search) = &mut app.search {
                if search
                    .selections
                    .iter()
                    .any(|selected| &selected.repository == repository)
                {
                    search
                        .selections
                        .retain(|selected| &selected.repository != repository);
                } else if let Some(selection) = search
                    .available_selections
                    .iter()
                    .find(|selection| &selection.repository == repository)
                    .cloned()
                {
                    search.selections.push(selection);
                }
                search.page = None;
                search.preview = None;
                app.context = search.context();
            }
            reset_results(app);
        }
        Open(_) if app.loading[crate::app::View::Search.index()] => {
            app.status = LOADING_OPEN.into();
        }
        Submit | Saved(_) => {
            // An old hit count is not the result of the newly entered query.
            if let Some(search) = &mut app.search {
                search.page = None;
                search.preview = None;
                search.remote_items.clear();
                search.blend_evidence = None;
            }
            app.hits.clear();
            app.search_controls.editing = None;
            app.search_action = Some(action);
        }
        Open(_) | Save => {
            app.search_controls.editing = None;
            app.search_action = Some(action);
        }
        PreviewScroll(delta) => {
            let maximum = app
                .search
                .as_ref()
                .and_then(|search| search.preview.as_ref())
                .map_or(0, |preview| preview.lines.len().saturating_sub(1));
            app.search_controls.preview_scroll = app
                .search_controls
                .preview_scroll
                .saturating_add_signed(*delta as isize)
                .min(maximum);
        }
    }
}
fn reset_results(app: &mut App) {
    if let Some(search) = &mut app.search {
        search.blend_evidence = None;
    }
    if app.loading[crate::app::View::Search.index()] {
        app.cancel_requested = true;
    }
    app.search_controls.selected = 0;
    app.search_controls.top = 0;
    app.search_controls.preview_scroll = 0;
    app.search_controls.editing = None;
    app.status = "Search scope changed • Submit to search this exact scope".into();
}

pub fn handle_event(app: &mut App, event: &Event) -> bool {
    if let Some(field) = app.search_controls.editing {
        match event {
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                let control = key.modifiers.contains(KeyModifiers::CONTROL);
                let shift = key.modifiers.contains(KeyModifiers::SHIFT);
                if control && key.code == KeyCode::Char('c') {
                    app.quit = true;
                    return true;
                }
                if key.code == KeyCode::Esc {
                    app.search_controls.editing = None;
                    return true;
                }
                if key.code == KeyCode::Enter {
                    app.search_controls.editing = None;
                    app.dispatch(Action::Search(match field {
                        SearchField::Query => SearchAction::Submit,
                        SearchField::Revision => SearchAction::SavedVersion,
                        SearchField::Name => SearchAction::Save,
                    }));
                    return true;
                }
                if matches!(key.code, KeyCode::Tab | KeyCode::BackTab) {
                    app.search_controls.editing = None;
                    app.search_content_focused = false;
                    app.focus = (app.focus
                        + if key.code == KeyCode::Tab {
                            1
                        } else {
                            app.hits.len().saturating_sub(1)
                        })
                        % app.hits.len().max(1);
                    return true;
                }
                let before = app.search_controls.editor(field).text.clone();
                let editor = app.search_controls.editor(field);
                match key.code {
                    KeyCode::Char('a') if control => editor.select_all(),
                    KeyCode::Char('u') if control => editor.clear(),
                    KeyCode::Char(character)
                        if !control && !key.modifiers.contains(KeyModifiers::ALT) =>
                    {
                        let _ = editor.insert(&character.to_string());
                    }
                    KeyCode::Left => editor.horizontal(false, shift),
                    KeyCode::Right => editor.horizontal(true, shift),
                    KeyCode::Home => editor.line_edge(false, control, shift),
                    KeyCode::End => editor.line_edge(true, control, shift),
                    KeyCode::Backspace => editor.delete_backward(),
                    KeyCode::Delete => editor.delete_forward(),
                    _ => {}
                }
                if field == SearchField::Query && app.search_controls.query.text != before {
                    if let Some(search) = &mut app.search {
                        search.page = None;
                        search.preview = None;
                    }
                    if app.loading[crate::app::View::Search.index()] {
                        app.cancel_requested = true;
                    }
                }
                return true;
            }
            Event::Paste(text) => {
                let text = text.replace(['\r', '\n', '\t'], " ");
                if let Err(error) = app.search_controls.editor(field).insert(&text) {
                    app.status = error;
                }
                if field == SearchField::Query {
                    if let Some(search) = &mut app.search {
                        search.page = None;
                        search.preview = None;
                    }
                    if app.loading[crate::app::View::Search.index()] {
                        app.cancel_requested = true;
                    }
                }
                return true;
            }
            _ => {}
        }
    }
    if let Event::Mouse(mouse) = event {
        if matches!(
            mouse.kind,
            MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left)
        ) {
            if let Some((field, rect)) = app
                .search_controls
                .fields
                .iter()
                .find(|(_, rect)| rect.contains((mouse.column, mouse.row).into()))
                .copied()
            {
                app.search_controls.editing = Some(field);
                app.search_controls.editor(field).click(
                    0,
                    mouse.column.saturating_sub(rect.x) as usize,
                    matches!(mouse.kind, MouseEventKind::Drag(_)),
                );
                return true;
            }
        }
        if matches!(
            mouse.kind,
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp
        ) {
            if app
                .search_controls
                .preview_rect
                .contains((mouse.column, mouse.row).into())
            {
                app.dispatch(Action::Search(SearchAction::PreviewScroll(
                    if mouse.kind == MouseEventKind::ScrollDown {
                        3
                    } else {
                        -3
                    },
                )));
                return true;
            }
            move_selection(
                app,
                if mouse.kind == MouseEventKind::ScrollDown {
                    3
                } else {
                    -3
                },
            );
            return true;
        }
    }
    let Event::Key(key) = event else {
        return false;
    };
    if key.kind == KeyEventKind::Release {
        return true;
    }
    // Every painted control (key bar and tabs) answers through its one declaration, and a
    // disabled one explains itself. Enter keeps activating the focused control or row.
    if key.code != KeyCode::Enter {
        if let Some(action) = crate::footer::resolve_key(&footer_keys(app), *key) {
            app.dispatch(action);
            return true;
        }
    }
    let action = match key.code {
        KeyCode::Down if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(SearchAction::PreviewScroll(3))
        }
        KeyCode::Up if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(SearchAction::PreviewScroll(-3))
        }
        KeyCode::Char('/') => Some(SearchAction::Query),
        KeyCode::Char('d') => Some(SearchAction::Revision),
        KeyCode::Char('z') => Some(SearchAction::Name),
        KeyCode::Char('u') => app
            .search
            .as_ref()
            .and_then(|search| search.saved.first())
            .map(|saved| SearchAction::Saved(saved.name.clone())),
        KeyCode::Char('p') => Some(SearchAction::Preview(app.search_controls.selected)),
        KeyCode::Enter if app.search_controls.corpus => app
            .search
            .as_ref()
            .and_then(|search| {
                search
                    .available_selections
                    .get(app.search_controls.corpus_index)
            })
            .map(|selection| SearchAction::ToggleRepository(selection.repository.clone())),
        KeyCode::Enter => app
            .hits
            .get(app.focus)
            .map(|hit| hit.action.clone())
            .and_then(|action| {
                if let Action::Search(action) = action {
                    Some(action)
                } else {
                    None
                }
            }),
        KeyCode::Down | KeyCode::Up | KeyCode::PageDown | KeyCode::PageUp => {
            let delta = match key.code {
                KeyCode::Down => 1,
                KeyCode::Up => -1,
                KeyCode::PageDown => 10,
                _ => -10,
            };
            move_selection(app, delta);
            return true;
        }
        _ => None,
    };
    if let Some(action) = action {
        app.dispatch(Action::Search(action));
        true
    } else {
        false
    }
}
fn move_selection(app: &mut App, delta: isize) {
    if app.search_controls.corpus {
        let count = app
            .search
            .as_ref()
            .map_or(0, |search| search.available_selections.len());
        app.search_controls.corpus_index = app
            .search_controls
            .corpus_index
            .saturating_add_signed(delta)
            .min(count.saturating_sub(1));
        return;
    }
    let length = app
        .search
        .as_ref()
        .and_then(|search| search.page.as_ref())
        .map_or(0, |page| page.hits.len());
    if length == 0 {
        return;
    }
    let next = app
        .search_controls
        .selected
        .saturating_add_signed(delta)
        .min(length - 1);
    app.search_controls.top = if next < app.search_controls.top {
        next
    } else if next >= app.search_controls.top + app.search_controls.visible {
        next + 1 - app.search_controls.visible.max(1)
    } else {
        app.search_controls.top
    };
    app.dispatch(Action::Search(SearchAction::Preview(next)));
}

fn button(
    frame: &mut Frame,
    app: &mut App,
    rect: Rect,
    label: impl Into<String>,
    action: SearchAction,
    active: bool,
) {
    let style = if active || app.focus == app.hits.len() {
        app.theme()
            .style(SemanticToken::Focus)
            .add_modifier(Modifier::BOLD)
    } else {
        app.theme().base()
    };
    frame.render_widget(Paragraph::new(label.into()).style(style), rect);
    if rect.width > 0 && rect.height > 0 {
        app.hits.push(HitRegion {
            rect,
            action: Action::Search(action),
        });
    }
}
fn field(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    label: &str,
    kind: SearchField,
    action: SearchAction,
) {
    let label_width = (label.len() as u16).min(area.width);
    button(
        frame,
        app,
        Rect::new(area.x, area.y, label_width, 1),
        label,
        action,
        app.search_controls.editing == Some(kind),
    );
    let rect = Rect::new(
        area.x + label_width,
        area.y,
        area.width.saturating_sub(label_width),
        1,
    );
    app.search_controls.fields.push((kind, rect));
    let theme = app.theme();
    let editor = app.search_controls.editor(kind);
    editor.ensure_visible(rect.width as usize, 1);
    let text =
        crate::forms::editor_line(editor, &editor.text, 0, rect.width as usize, theme, false);
    frame.render_widget(
        Paragraph::new(text).style(
            theme
                .style(SemanticToken::Focus)
                .add_modifier(Modifier::UNDERLINED),
        ),
        rect,
    );
    if app.search_controls.editing == Some(kind) && rect.width > 0 {
        let (row, column) = app.search_controls.editor(kind).position();
        let left = app.search_controls.editor(kind).left_column;
        if row == 0 {
            frame.set_cursor_position((
                rect.x
                    + column
                        .saturating_sub(left)
                        .min(rect.width.saturating_sub(1) as usize) as u16,
                rect.y,
            ));
        }
    }
}
/// Display width of a tab strip drawn by [`crate::ui_chrome::tab_strip`]: ` key label ` per
/// tab plus one gap column after each.
fn tab_strip_width(controls: &[KeyControl]) -> u16 {
    controls
        .iter()
        .map(|control| {
            crate::editor::display_width(&format!(" {} {} ", control.key, control.label)) + 1
        })
        .sum::<usize>()
        .min(usize::from(u16::MAX)) as u16
}
pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    frame.render_widget(Clear, area);
    app.search_controls.fields.clear();
    app.search_controls.preview_rect = Rect::default();
    let saved = app.search.as_ref().map_or_else(Vec::new, |search| {
        search
            .saved
            .iter()
            .map(|saved| saved.name.clone())
            .collect::<Vec<_>>()
    });
    let saved_rows = u16::from(!saved.is_empty());
    if area.height < 4 + saved_rows {
        frame.render_widget(Paragraph::new("Resize terminal for search controls"), area);
        return;
    }
    // Tabs, the query, revision and name inputs, then saved queries when there are any. Every
    // other Search action is a key-bar token (see `footer_keys`).
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(saved_rows),
        Constraint::Min(1),
    ])
    .split(area);
    let mode = search_mode(app);
    let modes = mode_tabs(app);
    crate::ui_chrome::tab_strip(frame, app, rows[0], &modes);
    // The source tabs follow in the last gap column, behind a divider.
    let divider = rows[0].x + tab_strip_width(&modes).saturating_sub(1);
    if divider < rows[0].right() {
        frame.render_widget(
            Paragraph::new("│").style(app.theme().role(crate::theme::Role::Border)),
            Rect::new(divider, rows[0].y, 1, 1),
        );
        crate::ui_chrome::tab_strip(
            frame,
            app,
            Rect::new(divider + 1, rows[0].y, rows[0].right() - divider - 1, 1),
            &source_tabs(app),
        );
    }
    field(
        frame,
        app,
        rows[1],
        "[/ Query] ",
        SearchField::Query,
        SearchAction::Query,
    );
    let revision_width = rows[2].width / 2;
    field(
        frame,
        app,
        Rect::new(rows[2].x, rows[2].y, revision_width.saturating_sub(1), 1),
        "[d Revision] ",
        SearchField::Revision,
        SearchAction::Revision,
    );
    field(
        frame,
        app,
        Rect::new(
            rows[2].x + revision_width,
            rows[2].y,
            rows[2].width - revision_width,
            1,
        ),
        "[z Name] ",
        SearchField::Name,
        SearchAction::Name,
    );
    if saved_rows > 0 {
        let lead = "Saved ";
        let lead_width = (lead.len() as u16).min(rows[3].width);
        frame.render_widget(
            Paragraph::new(lead).style(app.theme().role(crate::theme::Role::TextMuted)),
            Rect::new(rows[3].x, rows[3].y, lead_width, 1),
        );
        let mut x = rows[3].x + lead_width;
        for (index, name) in saved.into_iter().enumerate() {
            let label = format!(
                "[{} {}]",
                if index == 0 { "u" } else { "saved" },
                safe_text(&name)
            );
            let width = crate::editor::display_width(&label).min(usize::from(u16::MAX)) as u16;
            // A saved query is shown whole or not at all, so its click target is never cut.
            if x.saturating_add(width) > rows[3].right() {
                break;
            }
            button(
                frame,
                app,
                Rect::new(x, rows[3].y, width, 1),
                label,
                SearchAction::Saved(name),
                false,
            );
            x += width + 1;
        }
    }
    let body = rows[4];
    if app.search_controls.corpus {
        draw_corpus(frame, app, body);
        return;
    }
    let Some(search) = &app.search else {
        let message = crate::remote_scope_gate_v1::legacy_entry(app.remote_scope.as_deref())
            .err()
            .map_or(
                "Preparing selected local corpus… • no clone or fetch",
                |reason| reason.label(),
            );
        frame.render_widget(
            Paragraph::new(message).wrap(ratatui::widgets::Wrap { trim: false }),
            body,
        );
        return;
    };
    let Some(page) = &search.page else {
        let notes = if mode == SearchMode::GitHub {
            // Local-corpus warnings do not describe the remote scope.
            format!("Types: type:file (default) type:pr type:issue • repo:owner/name or org:name searches outside {}",
                if search.remote_repositories.is_empty() { "the default scope".into() } else { search.remote_repositories.join(", ") })
        } else {
            search.warnings.join("\n")
        };
        frame.render_widget(Paragraph::new(format!("{:?} mode • enter a query, then Search.\n{}\nFilters: repo: org: set: type: path: language: author: state: after: before: rev:\n{}",mode,search.context(),notes)).wrap(ratatui::widgets::Wrap { trim: false }),body);
        return;
    };
    let summary = page.coverage.summary();
    let problems = page
        .coverage
        .reasons
        .iter()
        .chain(page.coverage.unavailable.iter())
        .cloned()
        .collect::<Vec<_>>()
        .join("; ");
    let source_rows = search
        .blend_evidence
        .as_ref()
        .map_or(0, |evidence| evidence.headlines.len().min(3)) as u16;
    let header_rows = if problems.is_empty() { 2 } else { 3 };
    frame.render_widget(
        Paragraph::new(format!(
            "{} results • {:?} • searched source snapshot\n{}{}",
            page.hits.len(),
            page.query.mode,
            summary,
            if problems.is_empty() {
                String::new()
            } else {
                format!("\n{}", safe_text(&problems))
            }
        )),
        Rect::new(body.x, body.y, body.width, header_rows.min(body.height)),
    );
    if let Some(evidence) = &search.blend_evidence {
        for (offset, (headline, token)) in evidence
            .headlines
            .iter()
            .take(source_rows as usize)
            .enumerate()
        {
            let y = body.y + header_rows + offset as u16;
            if y < body.bottom() {
                frame.render_widget(
                    Paragraph::new(headline.as_str()).style(app.theme().style(*token)),
                    Rect::new(body.x, y, body.width, 1),
                );
            }
        }
    }
    let header_rows = header_rows + source_rows;
    let available = Rect::new(
        body.x,
        body.y + header_rows.min(body.height),
        body.width,
        body.height.saturating_sub(header_rows),
    );
    // The shared list plus preview threshold (TASK-92); `available` is inside the pane border.
    let side_by_side = available.width + 2 >= crate::ui_chrome::SPLIT_MIN_WIDTH;
    let panels = if side_by_side {
        // One blank column keeps the results' ellipses off the preview's rule.
        Layout::horizontal([Constraint::Percentage(48), Constraint::Percentage(52)])
            .spacing(1)
            .split(available)
    } else {
        Layout::vertical([
            Constraint::Length((available.height / 2).max(2)),
            Constraint::Min(1),
        ])
        .split(available)
    };
    // One result is a table row (title, line:column, repository) over its snippet line.
    let table = Table {
        columns: &RESULT_COLUMNS,
        header: true,
        focused: app.search_content_focused,
        glyphs: false,
        empty: Some("No results in the searched snapshot"),
    };
    // Comfortable: a result is its table row over its snippet line. Compact (TASK-96): one
    // line per result; the snippet stays in the preview.
    let lines_per_hit = if crate::ui_chrome::density().is_compact() {
        1
    } else {
        2
    };
    app.search_controls.visible =
        (panels[0].height.saturating_sub(1) as usize / lines_per_hit).max(1);
    if let Some(last) = page.hits.len().checked_sub(1) {
        // Keep the selected result visible after a resize.
        let mut state = ListState {
            selected: app.search_controls.selected.min(last),
            top: app.search_controls.top.min(last),
        };
        state.ensure_visible(app.search_controls.visible);
        app.search_controls.top = state.top;
    }
    // Copy only small visible labels/snippets, never entire source documents.
    let hits = page
        .hits
        .iter()
        .enumerate()
        .skip(app.search_controls.top)
        .take(app.search_controls.visible)
        .map(|(index, hit)| RenderHit {
            index,
            title: match hit.destination() {
                // Discovery rows name their PR/issue number like GitHub does.
                crate::search::model::Destination::PullRequest { number, .. }
                | crate::search::model::Destination::Issue { number, .. }
                    if hit.document.source == crate::search::model::SourceKind::GitHub
                        && number > 0 =>
                {
                    format!("#{number} {}", hit.document.title)
                }
                _ => hit.document.title.clone(),
            },
            repository: hit.document.key.repository.clone(),
            line: hit.line,
            column: hit.column,
            snippet: hit.snippet.clone(),
            highlights: hit.highlights.clone(),
        })
        .collect::<Vec<_>>();
    let rows = hits
        .iter()
        .flat_map(|hit| {
            let action = Some(SearchAction::Preview(hit.index));
            let mut rows = vec![TableRow::cells(vec![
                safe_text(&hit.title).into(),
                if hit.line == 0 {
                    Cell::styled("fragment", Role::TextMuted)
                } else {
                    format!("{}:{}", hit.line, hit.column).into()
                },
                Cell::styled(safe_text(&hit.repository), Role::TextMuted),
            ])
            .with_action(action.clone())];
            if lines_per_hit == 2 {
                // The snippet is painted below with its highlights.
                rows.push(TableRow::text(String::new()).with_action(action));
            }
            rows
        })
        .collect::<Vec<_>>();
    let selected = hits
        .iter()
        .position(|hit| hit.index == app.search_controls.selected);
    let mut state = ListState {
        selected: selected.unwrap_or_default() * lines_per_hit,
        top: 0,
    };
    let theme = app.theme();
    let rendered = draw_table(frame, panels[0], &table, &rows, &mut state, theme);
    let snippet_x = panels[0].x + crate::ui_table::lead(false).min(panels[0].width);
    for pair in rendered.hits.chunks(lines_per_hit) {
        let position = pair[0].index / lines_per_hit;
        let hit = &hits[position];
        let mut rect = pair[0].rect;
        if let Some(snippet) = pair.get(1) {
            rect.height += snippet.rect.height;
            let width = snippet.rect.right().saturating_sub(snippet_x);
            let spans = snippet_spans(
                &hit.snippet,
                &hit.highlights,
                usize::from(width),
                theme
                    .style(SemanticToken::Focus)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            );
            frame
                .buffer_mut()
                .set_line(snippet_x, snippet.rect.y, &Line::from(spans), width);
            if selected == Some(position) {
                // The selected result's snippet shares its row's Selection background.
                frame
                    .buffer_mut()
                    .set_style(snippet.rect, crate::ui_table::selection_style(theme));
            }
        }
        // One click target per result: its title row and snippet line together.
        app.hits.push(HitRegion {
            rect,
            action: Action::Search(SearchAction::Preview(hit.index)),
        });
    }
    let remote_actor = app.search.as_ref().and_then(|search| {
        search.remote_identity.as_ref().and_then(|identity| {
            search
                .result_actors(app.search_controls.selected)
                .ok()
                .and_then(|actors| actors.into_iter().next())
                .map(|(role, actor)| (identity.host.clone(), role, actor))
        })
    });
    let actor_rows = u16::from(remote_actor.is_some());
    let preview = app
        .search
        .as_ref()
        .and_then(|search| search.preview.as_ref())
        .map(|preview| crate::search::workspace::SearchPreview {
            title: preview.title.clone(),
            context: preview.context.clone(),
            source: preview.source,
            matched_fields: preview.matched_fields.clone(),
            destination: preview.destination.clone(),
            lines: preview
                .lines
                .iter()
                .skip(app.search_controls.preview_scroll)
                .take(panels[1].height.saturating_sub(4 + actor_rows) as usize)
                .cloned()
                .collect(),
        });
    if let Some(preview) = preview {
        app.search_controls.preview_rect = panels[1];
        // Beside the list a left rule separates the preview; below it (drill-down widths) a
        // top rule does, so no stray `│` doubles the pane border.
        let (borders, pad) = if side_by_side {
            (Borders::LEFT, 2)
        } else {
            (Borders::TOP, 1)
        };
        frame.render_widget(
            Block::default()
                .borders(borders)
                .border_style(app.theme().role(crate::theme::Role::Border))
                .title(crate::ui_chrome::fit(
                    &format!(
                        "{} Preview {} ",
                        if side_by_side { "" } else { "─" },
                        safe_text(&preview.title)
                    ),
                    usize::from(panels[1].width.saturating_sub(2)),
                )),
            panels[1],
        );
        let inner = Rect::new(
            panels[1].x + pad.min(panels[1].width),
            panels[1].y,
            panels[1].width.saturating_sub(pad + 1),
            panels[1].height,
        );
        if let Some((host, role, actor)) = remote_actor {
            let label = if role.starts_with("repository owner") {
                "Owner (not author): "
            } else {
                "Author: "
            };
            let width = (label.len() as u16).min(inner.width);
            frame.render_widget(
                Paragraph::new(label).style(app.theme().style(SemanticToken::Actor)),
                Rect::new(
                    inner.x,
                    inner.y + 1,
                    width,
                    1.min(inner.height.saturating_sub(1)),
                ),
            );
            let identity = crate::identity::ActorIdentity::github(&host, &actor);
            let image = match app.avatar_states.get(&identity) {
                Some(crate::identity::AvatarState::Ready(image)) => Some(image.as_ref()),
                _ => None,
            };
            frame.render_widget(
                crate::identity::ActorWidget {
                    identity: &identity,
                    image,
                    theme: app.theme(),
                },
                Rect::new(
                    inner.x + width,
                    inner.y + 1,
                    inner.width.saturating_sub(width),
                    1.min(inner.height.saturating_sub(1)),
                ),
            );
        }
        frame.render_widget(
            Paragraph::new(preview.source.label()),
            Rect::new(
                inner.x,
                inner.y + 1 + actor_rows,
                inner.width,
                1.min(inner.height.saturating_sub(1 + actor_rows)),
            ),
        );
        // Opening the result is the key bar's `o Open result`; the preview keeps its scroll.
        let more = Rect::new(
            inner.x,
            inner.y + 2 + actor_rows,
            12.min(inner.width),
            1.min(inner.height.saturating_sub(2 + actor_rows)),
        );
        button(
            frame,
            app,
            more,
            "[Ctrl↓ More]",
            SearchAction::PreviewScroll(3),
            false,
        );
        let back = Rect::new(
            inner.x + 13.min(inner.width),
            inner.y + 2 + actor_rows,
            12.min(inner.width.saturating_sub(13)),
            1.min(inner.height.saturating_sub(2 + actor_rows)),
        );
        button(
            frame,
            app,
            back,
            "[Ctrl↑ Back]",
            SearchAction::PreviewScroll(-3),
            false,
        );
        // A fused contribution is short, structured prepared metadata. Keep
        // each contributing rank visible rather than clipping the last source
        // behind a long lexical explanation in the half-width preview.
        let why = if let Some(rrf) = preview
            .matched_fields
            .iter()
            .find(|field| field.starts_with("RRF k=60:"))
        {
            rrf.split(" + ")
                .enumerate()
                .map(|(index, field)| {
                    if index == 0 {
                        format!("Why: {field}")
                    } else {
                        format!("  + {field}")
                    }
                })
                .collect::<Vec<_>>()
        } else {
            vec![format!(
                "Why: {}",
                if preview.matched_fields.is_empty() {
                    "literal text match".into()
                } else {
                    preview.matched_fields.join(", ")
                }
            )]
        };
        let why_height = (why.len() as u16).min(inner.height.saturating_sub(3 + actor_rows));
        frame.render_widget(
            Paragraph::new(why.join("\n")).style(app.theme().style(SemanticToken::Muted)),
            Rect::new(inner.x, inner.y + 3 + actor_rows, inner.width, why_height),
        );
        for (offset, line) in preview
            .lines
            .iter()
            .take(inner.height.saturating_sub(3 + actor_rows + why_height) as usize)
            .enumerate()
        {
            frame.render_widget(
                Paragraph::new(format!(
                    "{} {:>5} {}",
                    if line.selected { ">" } else { " " },
                    if line.number == 0 {
                        "part".into()
                    } else {
                        line.number.to_string()
                    },
                    safe_text(&line.text)
                ))
                .style(if line.selected {
                    app.theme()
                        .style(SemanticToken::Focus)
                        .add_modifier(Modifier::BOLD)
                } else {
                    app.theme().base()
                }),
                Rect::new(
                    inner.x,
                    inner.y + 3 + actor_rows + why_height + offset as u16,
                    inner.width,
                    1,
                ),
            );
        }
    }
}
fn draw_corpus(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(search) = &app.search else {
        return;
    };
    let hidden = search.corpus_limits.include_hidden;
    let ignored = search.corpus_limits.include_ignored;
    let selections = search.available_selections.clone();
    let selected = search
        .selections
        .iter()
        .map(|selection| selection.repository.clone())
        .collect::<Vec<_>>();
    // The b/i toggles are key-bar tokens; this line keeps their state readable when the
    // bar is narrow.
    let flag = |value: bool| if value { "on" } else { "off" };
    frame.render_widget(
        Paragraph::new(format!(
            "Selected existing checkouts only • no automatic clone/fetch • Hidden files: {} • Ignored files: {}",
            flag(hidden),
            flag(ignored)
        ))
        .style(app.theme().role(crate::theme::Role::TextMuted)),
        Rect::new(area.x, area.y, area.width, 1.min(area.height)),
    );
    let visible = area.height.saturating_sub(1) as usize;
    let first = app
        .search_controls
        .corpus_index
        .saturating_sub(visible.saturating_sub(1));
    for (offset, (index, selection)) in selections
        .iter()
        .enumerate()
        .skip(first)
        .take(visible)
        .enumerate()
    {
        button(
            frame,
            app,
            Rect::new(area.x, area.y + 1 + offset as u16, area.width, 1),
            format!(
                "{} [{}] {} • {}",
                if index == app.search_controls.corpus_index {
                    ">"
                } else {
                    " "
                },
                if selected.contains(&selection.repository) {
                    "x"
                } else {
                    " "
                },
                selection.repository,
                selection.directory.display()
            ),
            SearchAction::ToggleRepository(selection.repository.clone()),
            index == app.search_controls.corpus_index,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        search::{corpus::RepositorySelection, workspace::SearchWorkspace},
        storage::Scope,
    };
    use crossterm::event::KeyEvent;
    use ratatui::{backend::TestBackend, Terminal};
    #[test]
    fn query_modes_input_keeps_unicode_literals_and_shared_scope_actions() {
        let mut app = App {
            view: crate::app::View::Search,
            search: Some(SearchWorkspace::new(
                Scope::new("local", "workspace").unwrap(),
                vec![RepositorySelection {
                    repository: "local/current".into(),
                    directory: "/tmp/example".into(),
                    source: LocalSource::Worktree,
                    sets: vec![],
                }],
            )),
            ..App::default()
        };
        app.dispatch(Action::Search(SearchAction::Query));
        assert!(handle_event(&mut app, &Event::Paste("q 日本語 👩🏽‍💻".into())));
        assert!(!app.quit);
        handle_event(
            &mut app,
            &Event::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
        );
        assert_eq!(app.search_controls.query.text, "q 日本語 ");
        handle_event(
            &mut app,
            &Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        );
        assert_eq!(app.search_action, Some(SearchAction::Submit));
        app.dispatch(Action::Search(SearchAction::Mode(SearchMode::Regex)));
        app.dispatch(Action::Search(SearchAction::SavedVersion));
        assert_eq!(app.search.as_ref().unwrap().mode, SearchMode::Regex);
        assert_eq!(
            app.search.as_ref().unwrap().selections[0].source,
            LocalSource::Revision("HEAD".into())
        );
        app.dispatch(Action::Search(SearchAction::ToggleRepository(
            "local/current".into(),
        )));
        assert!(app.search.as_ref().unwrap().selections.is_empty());
        assert_eq!(app.search.as_ref().unwrap().available_selections.len(), 1);
    }

    fn search_app() -> App {
        App {
            view: crate::app::View::Search,
            search: Some(SearchWorkspace::new(
                Scope::new("local", "workspace").unwrap(),
                vec![RepositorySelection {
                    repository: "local/current".into(),
                    directory: "/tmp/example".into(),
                    source: LocalSource::Worktree,
                    sets: vec![],
                }],
            )),
            ..App::default()
        }
    }
    fn placed_tokens(app: &App, width: u16) -> Vec<String> {
        let controls = crate::footer::main_controls(app);
        crate::footer::layout(&controls, width)
            .into_iter()
            .map(|placed| controls[placed.index].token(placed.compact))
            .collect()
    }
    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn design_help_v97_search_help_and_commands_cover_every_footer_key() {
        let mut app = search_app();
        assert!(crate::navigation_ui::assert_help_and_palette_cover_footer_v97(&app, "Search") > 5);
        app.search_controls.corpus = true;
        assert!(
            crate::navigation_ui::assert_help_and_palette_cover_footer_v97(&app, "Search corpus")
                > 3
        );
    }

    #[test]
    fn design_footer_v90_search_bar_keeps_priorities_at_140_and_80_columns() {
        let app = search_app();
        assert_eq!(
            placed_tokens(&app, 140),
            [
                "Enter Search",
                "c Corpus",
                "M Repositories",
                "f Save query",
                "H Sources",
                "B Blend",
                "r Refresh",
                "s Settings",
                "? Help",
                ": Commands",
                "q Quit"
            ]
        );
        assert_eq!(
            placed_tokens(&app, 120),
            [
                "Enter Search",
                "c Corpus",
                "M Repositories",
                "f Save query",
                "H Sources",
                "B Blend",
                "r Refresh",
                "? Help",
                ": Commands",
                "q Quit"
            ]
        );
        // Mode and source tabs never enter the bar; Open (no result yet) and the Exact-mode
        // Blend toggle are disabled, so they leave it too.
        assert_eq!(
            placed_tokens(&app, 80),
            [
                "Enter Search",
                "c Corpus",
                "M Repositories",
                "H Sources",
                "? Help",
                ": Cmds",
                "q Quit"
            ]
        );
    }

    #[test]
    fn design_footer_v90_search_keys_resolve_like_their_tokens_and_disabled_keys_explain() {
        let mut app = search_app();
        let keys = footer_keys(&app);
        for (code, expected) in [
            (
                KeyCode::Char('n'),
                Action::Search(SearchAction::Mode(SearchMode::Ranked)),
            ),
            (KeyCode::Char('w'), Action::Search(SearchAction::Worktree)),
            (
                KeyCode::Char('v'),
                Action::Search(SearchAction::SavedVersion),
            ),
            (KeyCode::Char('c'), Action::Search(SearchAction::Corpus)),
            (KeyCode::Char('M'), Action::Corpus),
            (KeyCode::Char('f'), Action::Search(SearchAction::Save)),
            (
                KeyCode::Char('H'),
                Action::SearchSource(crate::search_sources::SourceAction::Open),
            ),
            (
                KeyCode::Char('B'),
                Action::Search(SearchAction::BlendControls),
            ),
            (KeyCode::Enter, Action::Search(SearchAction::Submit)),
        ] {
            assert_eq!(crate::footer::resolve_key(&keys, key(code)), Some(expected));
        }
        assert_eq!(
            crate::footer::resolve_key(&keys, key(KeyCode::Char('o'))),
            Some(Action::Explain(NO_RESULT.into()))
        );
        assert!(matches!(
            crate::footer::resolve_key(&keys, key(KeyCode::Char('b'))),
            Some(Action::Explain(reason)) if reason.contains("Ranked-only")
        ));
        // GitHub discovery has no local source or corpus: the tabs dim and explain.
        app.dispatch(Action::Search(SearchAction::Mode(SearchMode::GitHub)));
        let keys = footer_keys(&app);
        for code in [KeyCode::Char('w'), KeyCode::Char('v')] {
            assert_eq!(
                crate::footer::resolve_key(&keys, key(code)),
                Some(Action::Explain(GITHUB_SOURCE.into()))
            );
        }
        assert_eq!(
            crate::footer::resolve_key(&keys, key(KeyCode::Char('c'))),
            Some(Action::Explain(GITHUB_CORPUS.into()))
        );
        assert!(!placed_tokens(&app, 140).contains(&"c Corpus".to_owned()));
    }

    #[test]
    fn design_footer_v90_search_corpus_keys_change_with_the_pane_and_handle_event_dispatches() {
        let mut app = search_app();
        assert!(handle_event(&mut app, &Event::Key(key(KeyCode::Char('c')))));
        assert!(app.search_controls.corpus);
        assert_eq!(
            &placed_tokens(&app, 140)[..4],
            [
                "Enter Toggle",
                "b Hidden files: off",
                "i Ignored files: off",
                "c Corpus"
            ]
        );
        assert!(placed_tokens(&app, 80).contains(&"b Hidden: off".to_owned()));
        assert!(handle_event(&mut app, &Event::Key(key(KeyCode::Char('b')))));
        assert!(app.search.as_ref().unwrap().corpus_limits.include_hidden);
        assert!(placed_tokens(&app, 140).contains(&"b Hidden files: on".to_owned()));
        assert!(handle_event(&mut app, &Event::Key(key(KeyCode::Char('i')))));
        assert!(app.search.as_ref().unwrap().corpus_limits.include_ignored);
        assert!(handle_event(&mut app, &Event::Key(key(KeyCode::Enter))));
        assert!(app.search.as_ref().unwrap().selections.is_empty());
        assert!(handle_event(&mut app, &Event::Key(key(KeyCode::Char('c')))));
        assert!(!app.search_controls.corpus);
        assert!(handle_event(&mut app, &Event::Key(key(KeyCode::Char('M')))));
        assert!(app.corpus_requested);
        // Ctrl never triggers a plain-letter control.
        app.corpus_requested = false;
        handle_event(
            &mut app,
            &Event::Key(KeyEvent::new(KeyCode::Char('M'), KeyModifiers::CONTROL)),
        );
        assert!(!app.corpus_requested);
    }

    #[test]
    fn design_footer_v90_search_pane_drops_toolbar_rows_and_registers_tab_and_footer_hits() {
        for (width, height) in [(140, 42), (80, 24)] {
            let mut app = search_app();
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|frame| {
                    draw(frame, &mut app, Rect::new(0, 0, width, height - 1));
                    let controls = crate::footer::main_controls(&app);
                    crate::footer::draw(
                        frame,
                        &mut app,
                        Rect::new(0, height - 1, width, 1),
                        &controls,
                    );
                })
                .unwrap();
            let buffer = terminal.backend().buffer();
            let row = |y: u16| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            };
            assert!(
                row(0).starts_with(
                    " e Exact   x Regex   n Ranked   h GitHub │ w Working files   v Saved version "
                ),
                "{}",
                row(0)
            );
            assert!(row(1).starts_with("[/ Query] "));
            assert!(row(2).starts_with("[d Revision] ") && row(2).contains("[z Name] "));
            assert!(
                row(3).starts_with("Exact mode • enter a query, then Search."),
                "{}",
                row(3)
            );
            let screen = (0..height).map(row).collect::<Vec<_>>().join("\n");
            for gone in [
                "[Enter Search]",
                "[B Blend controls]",
                "[M Repositories]",
                "[c Corpus]",
                "[o Open result]",
            ] {
                assert!(!screen.contains(gone), "{gone}\n{screen}");
            }
            assert!(row(height - 1).starts_with(" Enter Search  c Corpus  M Repositories  "));
            // Tabs click to the same action their key resolves to; footer hits come last.
            let ranked = app
                .hits
                .iter()
                .find(|hit| hit.action == Action::Search(SearchAction::Mode(SearchMode::Ranked)))
                .unwrap();
            assert_eq!(ranked.rect.y, 0);
            let last = app.hits.last().unwrap();
            assert_eq!((last.rect.y, &last.action), (height - 1, &Action::Quit));
            let corpus = app
                .hits
                .iter()
                .find(|hit| hit.action == Action::Search(SearchAction::Corpus))
                .unwrap();
            assert_eq!(corpus.rect.y, height - 1);
        }
    }
}
