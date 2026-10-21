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
    text::Span,
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

/// Search results (TASK-121), grouped by file: the short path or `#number title` (only on a
/// file's first result) and where the match is. Comfortable density paints each result's one
/// highlighted snippet on the full-width line below it; compact keeps it in a MATCH column.
const RESULT_COLUMNS: [Column; 2] = [
    Column::new("RESULT", 10).flex(1).priority(0),
    Column::new("LINE", 4).priority(0).right(),
];
/// The same with the repository, when the results span more than one; it leaves first.
const RESULT_COLUMNS_REPOSITORY: [Column; 3] = [
    Column::new("RESULT", 10).flex(1).priority(0),
    Column::new("LINE", 4).priority(0).right(),
    Column::new("REPOSITORY", 8).priority(2),
];
/// Compact density: one line per result with its snippet in MATCH (which leaves before LINE).
const COMPACT_COLUMNS: [Column; 3] = [
    // Its natural width: paths are already short and titles cut (see `TITLE`).
    Column::new("RESULT", 10).priority(0),
    Column::new("LINE", 4).priority(0).right(),
    Column::new("MATCH", 12).flex(3).priority(1),
];
const COMPACT_COLUMNS_REPOSITORY: [Column; 4] = [
    Column::new("RESULT", 10).priority(0),
    Column::new("LINE", 4).priority(0).right(),
    Column::new("MATCH", 12).flex(3).priority(1),
    Column::new("REPOSITORY", 8).priority(2),
];
/// Typing waits this long for the next key before the live fuzzy search runs.
pub const LIVE_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(250);
/// A result path wider than this keeps its file name and drops leading directories.
const SHORT_PATH: usize = 36;
/// A PR, issue or commit title is cut to this many cells, so the snippet keeps its room.
const TITLE: usize = 40;
/// Context kept before a snippet's first highlight; the rest of the lead becomes `…`.
const SNIPPET_LEAD: usize = 12;

/// `…/dir/file.rs` for a long path: the file name always stays.
fn short_path(path: &str, width: usize) -> String {
    if crate::editor::display_width(path) <= width {
        return path.to_owned();
    }
    let parts = path.split('/').collect::<Vec<_>>();
    let mut kept = String::new();
    for part in parts.iter().rev() {
        let next = if kept.is_empty() {
            (*part).to_owned()
        } else {
            format!("{part}/{kept}")
        };
        if !kept.is_empty() && crate::editor::display_width(&next) + 2 > width {
            break;
        }
        kept = next;
    }
    format!("…/{kept}")
}

/// The snippet from a little before its first highlight, so the match is what a narrow
/// column shows; the highlights move with it.
fn windowed_snippet(
    snippet: &str,
    highlights: &[std::ops::Range<usize>],
) -> (String, Vec<std::ops::Range<usize>>) {
    let first = highlights.first().map_or(0, |range| range.start);
    let lead = snippet[..first.min(snippet.len())].chars().count();
    let trimmed = snippet.trim_start();
    let blank = snippet.len() - trimmed.len();
    let (cut, prefix) = if lead > SNIPPET_LEAD {
        let skip = lead - SNIPPET_LEAD;
        let at = snippet
            .char_indices()
            .nth(skip)
            .map_or(snippet.len(), |(at, _)| at);
        (at.max(blank.min(first)), "…")
    } else {
        (blank.min(first), "")
    };
    if !snippet.is_char_boundary(cut) {
        return (snippet.to_owned(), highlights.to_vec());
    }
    let text = format!("{prefix}{}", &snippet[cut..]);
    let shift = |at: usize| at - cut + prefix.len();
    let ranges = highlights
        .iter()
        .filter(|range| range.start >= cut)
        .map(|range| shift(range.start)..shift(range.end))
        .collect();
    (text, ranges)
}

/// The snippet with its highlighted ranges, cut to `width` cells with an ellipsis. Text outside
/// the highlights takes `plain` (muted, like the Inbox detail line under its row).
fn snippet_spans(
    snippet: &str,
    highlights: &[std::ops::Range<usize>],
    width: usize,
    emphasis: ratatui::style::Style,
    plain: ratatui::style::Style,
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
            plain
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
    /// Empty on a file's later lines: the group's first line names it.
    title: String,
    repository: String,
    line: String,
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
    /// The saved search last restored or saved here; Pin, Rename and Delete act on it.
    pub saved_target: Option<String>,
    /// TASK-121: the optional mode, source, revision and name controls are shown.
    pub options: bool,
    /// When the live fuzzy search runs for the typed query (debounced).
    pub live_due: Option<std::time::Instant>,
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
            saved_target: None,
            options: false,
            live_due: None,
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

/// The query modes, drawn as the first tabs of the Options row (TASK-121: Fuzzy is the default;
/// the others are optional).
pub fn mode_tabs(app: &App) -> Vec<KeyControl> {
    let mode = search_mode(app);
    [
        ("F", "Fuzzy", SearchMode::Fuzzy),
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
    // `f` on a file result opens its History in the local repository (TASK-99.3); a result
    // that is not a file of this repository explains why.
    let history = if loading {
        Err(LOADING_OPEN.to_owned())
    } else if !has_result {
        Err(NO_RESULT.to_owned())
    } else {
        crate::history::entry::search_target(app)
    };
    let history_reason = history.as_ref().err().cloned();
    keys.push(
        KeyControl::new("f", "File history", crate::history::entry::action(history))
            .with_compact("History")
            .with_priority(65)
            .disabled_by(history_reason),
    );
    keys.push(KeyControl::new("M", "Repositories", Action::Corpus).with_priority(15));
    // The pane's `[O Options]` button is always painted; the bar shows it only when wide,
    // and it leaves before Refresh.
    keys.push(
        KeyControl::new("O", "Options", Action::Search(SearchAction::Options))
            .with_priority(62)
            .with_selected(app.search_controls.options),
    );
    keys.push(
        KeyControl::new("S", "Save query", Action::Search(SearchAction::Save)).with_priority(35),
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
        Options => {
            app.search_controls.options = !app.search_controls.options;
            app.search_controls.editing = None;
        }
        BlendPreferences => app.search_action = Some(action),
        Query => app.search_controls.editing = Some(SearchField::Query),
        // Both inputs live under Options, so editing one shows them.
        Revision => {
            app.search_controls.options = true;
            app.search_controls.editing = Some(SearchField::Revision);
        }
        Name => {
            app.search_controls.options = true;
            app.search_controls.editing = Some(SearchField::Name);
        }
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
        PinSaved(_) | RenameSaved(_) | DeleteSaved(_)
            if app.loading[crate::app::View::Search.index()] =>
        {
            app.status = "Search is still loading • wait before changing saved queries".into();
        }
        PinSaved(_) | RenameSaved(_) | DeleteSaved(_) => {
            app.search_controls.editing = None;
            app.search_controls.saved_target = match &action {
                RenameSaved(_) => Some(app.search_controls.name.text.trim().to_owned()),
                DeleteSaved(_) => None,
                _ => app.search_controls.saved_target.clone(),
            };
            app.search_action = Some(action);
        }
        Submit | Saved(_) => {
            if let Saved(name) = &action {
                // The restored query becomes the target of Pin, Rename and Delete.
                app.search_controls.saved_target = Some(name.clone());
                app.search_controls.name =
                    crate::editor::TextBuffer::new(name.as_str()).unwrap_or_default();
            }
            // An old hit count is not the result of the newly entered query.
            app.search_controls.live_due = None;
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
            if action == Save {
                app.search_controls.saved_target = Some(app.search_controls.name.text.clone());
            }
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
                    // TASK-121: the live results already answer this exact query, so Enter
                    // moves to them (the next Enter opens the selected one) instead of
                    // searching again.
                    if field == SearchField::Query && live_results_current(app) {
                        app.search_controls.live_due = None;
                        app.search_content_focused = true;
                        app.status =
                            "Results are current • ↑↓ choose a result • Enter or a click opens it"
                                .into();
                        return true;
                    }
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
                // In the fuzzy box ↑↓ choose a result while typing, like a finder.
                if field == SearchField::Query
                    && matches!(key.code, KeyCode::Up | KeyCode::Down)
                    && !control
                {
                    move_selection(app, if key.code == KeyCode::Down { 1 } else { -1 });
                    // Choosing a result keeps the box: typing goes on.
                    app.search_controls.editing = Some(field);
                    return true;
                }
                // `/` focuses the box; typed again into an empty box it is not text.
                if field == SearchField::Query
                    && key.code == KeyCode::Char('/')
                    && app.search_controls.query.text.is_empty()
                {
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
                    schedule_live(app);
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
                    schedule_live(app);
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
        KeyCode::Char('P' | 'R' | 'X') => {
            let Some(target) = saved_target(app) else {
                app.dispatch(Action::Explain(NO_SAVED_TARGET.into()));
                return true;
            };
            Some(match key.code {
                KeyCode::Char('P') => SearchAction::PinSaved(target),
                KeyCode::Char('R') => SearchAction::RenameSaved(target),
                _ => SearchAction::DeleteSaved(target),
            })
        }
        KeyCode::Enter if app.search_controls.corpus => app
            .search
            .as_ref()
            .and_then(|search| {
                search
                    .available_selections
                    .get(app.search_controls.corpus_index)
            })
            .map(|selection| SearchAction::ToggleRepository(selection.repository.clone())),
        // TASK-121: Enter on the focused results opens the selected one.
        KeyCode::Enter if app.search_content_focused && selected_result_ready(app) => {
            Some(SearchAction::Open(app.search_controls.selected))
        }
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
/// Whether the live fuzzy search may run by itself for the typed query: Fuzzy mode over this
/// account-free local scope only. Searches that must verify a GitHub account or private
/// source access first (loaded conversations, a remote scope) run when Enter asks.
pub fn live_allowed(app: &App) -> bool {
    app.view == crate::app::View::Search
        && crate::remote_scope_gate_v1::legacy_entry(app.remote_scope.as_deref()).is_ok()
        && app.search.as_ref().is_some_and(|search| {
            search.mode == SearchMode::Fuzzy
                && search.scope.host == "local"
                && search.metadata_sources.discussions.is_empty()
                && !search.blend_policy.enabled
        })
}

fn schedule_live(app: &mut App) {
    app.search_controls.live_due =
        live_allowed(app).then(|| std::time::Instant::now() + LIVE_DEBOUNCE);
}

/// The shown page answers exactly the typed query (a fuzzy page for older text is stale).
fn page_current(app: &App) -> bool {
    app.search.as_ref().is_some_and(|search| {
        search.page.is_some()
            && (search.mode != SearchMode::Fuzzy || search.input == app.search_controls.query.text)
    })
}

fn live_results_current(app: &App) -> bool {
    search_mode(app) == SearchMode::Fuzzy
        && !app.loading[crate::app::View::Search.index()]
        && app.search_controls.live_due.is_none()
        && page_current(app)
}

fn selected_result_ready(app: &App) -> bool {
    !app.search_controls.corpus
        && !app.loading[crate::app::View::Search.index()]
        && page_current(app)
        && app
            .search
            .as_ref()
            .and_then(|search| search.page.as_ref())
            .is_some_and(|page| page.hits.get(app.search_controls.selected).is_some())
}

/// TASK-121: runs the debounced live fuzzy search once typing pauses. Called once per frame by
/// the runtime loop; it never searches while a search is still loading (typing cancels that
/// one) and never for text the shown page already answers.
pub fn tick_live(app: &mut App) {
    let Some(due) = app.search_controls.live_due else {
        return;
    };
    if app.loading[crate::app::View::Search.index()] || std::time::Instant::now() < due {
        return;
    }
    app.search_controls.live_due = None;
    if !live_allowed(app) || app.search_action.is_some() {
        return;
    }
    let text = app.search_controls.query.text.clone();
    let Some(search) = &mut app.search else {
        return;
    };
    if text.trim().is_empty() {
        search.page = None;
        search.preview = None;
        app.status = "Type to search • results update as you type".into();
        return;
    }
    if search.page.is_some() && search.input == text {
        return;
    }
    search.page = None;
    search.preview = None;
    search.remote_items.clear();
    search.blend_evidence = None;
    app.search_controls.selected = 0;
    app.search_controls.top = 0;
    app.search_controls.preview_scroll = 0;
    // The query box keeps the keyboard: typing goes on while this search runs.
    app.search_action = Some(SearchAction::Submit);
}

const NO_SAVED_TARGET: &str = "Choose a saved query first (u or click it under Saved), or save one with S • Pin, Rename and Delete act on it";

/// The saved search Pin, Rename and Delete act on, when it still exists.
fn saved_target(app: &App) -> Option<String> {
    let target = app.search_controls.saved_target.as_ref()?;
    app.search
        .as_ref()?
        .saved
        .iter()
        .any(|saved| &saved.name == target)
        .then(|| target.clone())
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
pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    frame.render_widget(Clear, area);
    app.search_controls.fields.clear();
    app.search_controls.preview_rect = Rect::default();
    let saved = app.search.as_ref().map_or_else(Vec::new, |search| {
        search
            .saved
            .iter()
            .map(|saved| (saved.name.clone(), saved.pinned))
            .collect::<Vec<_>>()
    });
    let target = saved_target(app);
    let saved_rows = u16::from(!saved.is_empty());
    let options = app.search_controls.options;
    let option_rows = if options { 3 } else { 0 };
    if area.height < 3 + option_rows + saved_rows {
        frame.render_widget(Paragraph::new("Resize terminal for search controls"), area);
        return;
    }
    // TASK-121: the query box first, then one line with the Options control and what is being
    // searched. Options opens the mode and source tabs and the revision and name inputs; saved
    // queries follow when there are any. Every other Search action is a key-bar token.
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(option_rows),
        Constraint::Length(saved_rows),
        Constraint::Min(1),
    ])
    .split(area);
    let mode = search_mode(app);
    field(
        frame,
        app,
        rows[0],
        "[/ Query] ",
        SearchField::Query,
        SearchAction::Query,
    );
    let toggle = if options {
        "[O Hide options]"
    } else {
        "[O Options]"
    };
    let toggle_width = (toggle.len() as u16).min(rows[1].width);
    button(
        frame,
        app,
        Rect::new(rows[1].x, rows[1].y, toggle_width, 1),
        toggle,
        SearchAction::Options,
        options,
    );
    let source = if worktree_source(app) {
        "working files".to_owned()
    } else {
        format!("saved version {}", app.search_controls.revision.text.trim())
    };
    let hint = match mode {
        SearchMode::Fuzzy if live_allowed(app) => "type to search; ↑↓ choose; Enter opens",
        SearchMode::Fuzzy => "type, then Enter searches; Enter again opens",
        SearchMode::GitHub => "Enter searches GitHub",
        _ => "Enter searches",
    };
    let summary = if mode == SearchMode::GitHub {
        format!(" {mode:?} • {hint}")
    } else {
        format!(" {mode:?} • {source} • {hint}")
    };
    if rows[1].width > toggle_width {
        let width = rows[1].width - toggle_width;
        frame.render_widget(
            Paragraph::new(crate::ui_chrome::fit(&summary, usize::from(width)))
                .style(app.theme().role(crate::theme::Role::TextMuted)),
            Rect::new(rows[1].x + toggle_width, rows[1].y, width, 1),
        );
    }
    if options {
        // The mode tabs, then the source tabs on their own line, so both fit 80 columns.
        let tabs = Rect::new(rows[2].x, rows[2].y, rows[2].width, 1);
        crate::ui_chrome::tab_strip(frame, app, tabs, &mode_tabs(app));
        let sources = Rect::new(rows[2].x, rows[2].y + 1, rows[2].width, 1);
        crate::ui_chrome::tab_strip(frame, app, sources, &source_tabs(app));
        let inputs = Rect::new(rows[2].x, rows[2].y + 2, rows[2].width, 1);
        let revision_width = inputs.width / 2;
        field(
            frame,
            app,
            Rect::new(inputs.x, inputs.y, revision_width.saturating_sub(1), 1),
            "[d Revision] ",
            SearchField::Revision,
            SearchAction::Revision,
        );
        field(
            frame,
            app,
            Rect::new(
                inputs.x + revision_width,
                inputs.y,
                inputs.width - revision_width,
                1,
            ),
            "[z Name] ",
            SearchField::Name,
            SearchAction::Name,
        );
    }
    if saved_rows > 0 {
        let lead = "Saved ";
        let lead_width = (lead.len() as u16).min(rows[3].width);
        frame.render_widget(
            Paragraph::new(lead).style(app.theme().role(crate::theme::Role::TextMuted)),
            Rect::new(rows[3].x, rows[3].y, lead_width, 1),
        );
        let mut x = rows[3].x + lead_width;
        // The chosen saved query's own actions come first, so they are never cut by a long list.
        if let Some(target) = &target {
            let pinned = saved.iter().any(|(name, pinned)| name == target && *pinned);
            for (label, action) in [
                (
                    if pinned { "[P Unpin]" } else { "[P Pin]" },
                    SearchAction::PinSaved(target.clone()),
                ),
                ("[R Rename]", SearchAction::RenameSaved(target.clone())),
                ("[X Delete]", SearchAction::DeleteSaved(target.clone())),
            ] {
                let width = label.len() as u16;
                if x.saturating_add(width) > rows[3].right() {
                    break;
                }
                button(
                    frame,
                    app,
                    Rect::new(x, rows[3].y, width, 1),
                    label,
                    action,
                    false,
                );
                x += width + 1;
            }
        }
        for (index, (name, pinned)) in saved.into_iter().enumerate() {
            let label = format!(
                "[{} {}{}]",
                if index == 0 { "u" } else { "saved" },
                safe_text(&name),
                if pinned { " (pinned)" } else { "" }
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
    // A fuzzy page for older text does not answer the typed query (TASK-121): it waits for
    // the live search instead of showing stale matches.
    let page = search.page.as_ref().filter(|_| page_current(app));
    let Some(page) = page else {
        let notes = if mode == SearchMode::GitHub {
            // Local-corpus warnings do not describe the remote scope.
            format!("Types: type:file (default) type:pr type:issue • repo:owner/name or org:name searches outside {}",
                if search.remote_repositories.is_empty() { "the default scope".into() } else { search.remote_repositories.join(", ") })
        } else {
            search.warnings.join("\n")
        };
        let typed = app.search_controls.query.text.trim();
        let lead = if mode == SearchMode::Fuzzy && !typed.is_empty() {
            format!("Searching for “{}”…", safe_text(typed))
        } else if mode == SearchMode::Fuzzy {
            "Fuzzy mode • type to search file names, code and loaded conversations; results update as you type.".into()
        } else {
            format!("{mode:?} mode • enter a query, then Search.")
        };
        frame.render_widget(Paragraph::new(format!("{lead}\n{}\nFilters: repo: org: set: type: path: language: author: state: after: before: rev:\n{}",search.context(),notes)).wrap(ratatui::widgets::Wrap { trim: false }),body);
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
    // Each summary line ends in `…` when it is cut, instead of stopping mid-word at the border.
    let width = usize::from(body.width);
    let mut lines = vec![
        format!(
            "{} results • {:?} • searched source snapshot",
            page.hits.len(),
            page.query.mode
        ),
        summary,
    ];
    if !problems.is_empty() {
        lines.push(safe_text(&problems));
    }
    frame.render_widget(
        Paragraph::new(
            lines
                .iter()
                .map(|line| crate::ui_chrome::fit(line, width))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
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
    // TASK-121: one line per result, grouped by file (the path only on a file's first line):
    // short path or `#number title`, line:column, then one highlighted snippet.
    let repositories = page
        .hits
        .iter()
        .map(|hit| hit.document.key.repository.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let compact = crate::ui_chrome::density().is_compact();
    let lines_per_hit = if compact { 1 } else { 2 };
    let table = Table {
        columns: match (compact, repositories > 1) {
            (false, false) => &RESULT_COLUMNS,
            (false, true) => &RESULT_COLUMNS_REPOSITORY,
            (true, false) => &COMPACT_COLUMNS,
            (true, true) => &COMPACT_COLUMNS_REPOSITORY,
        },
        header: true,
        focused: app.search_content_focused,
        glyphs: false,
        empty: Some("No results in the searched snapshot"),
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
    let top = app.search_controls.top;
    let hits = page
        .hits
        .iter()
        .enumerate()
        .skip(top)
        .take(app.search_controls.visible)
        .map(|(index, hit)| {
            let continued = index > top
                && std::sync::Arc::ptr_eq(&page.hits[index - 1].document, &hit.document);
            let file_name = hit
                .matched_fields
                .first()
                .is_some_and(|field| field == crate::search::scan::FUZZY_FILE_NAME);
            let (snippet, highlights) = windowed_snippet(&hit.snippet, &hit.highlights);
            RenderHit {
                index,
                title: if continued {
                    String::new()
                } else {
                    match hit.destination() {
                        // PR and issue rows name their number like GitHub does.
                        crate::search::model::Destination::PullRequest { number, .. }
                        | crate::search::model::Destination::Issue { number, .. }
                            if number > 0 =>
                        {
                            crate::ui_chrome::fit(
                                &format!("#{number} {}", hit.document.title),
                                TITLE,
                            )
                        }
                        crate::search::model::Destination::LocalFile { .. } => {
                            short_path(&hit.document.title, SHORT_PATH)
                        }
                        _ => crate::ui_chrome::fit(&hit.document.title, TITLE),
                    }
                },
                repository: hit.document.key.repository.clone(),
                line: if file_name {
                    "name".into()
                } else if hit.line == 0 {
                    "fragment".into()
                } else {
                    format!("{}:{}", hit.line, hit.column)
                },
                snippet,
                highlights,
            }
        })
        .collect::<Vec<_>>();
    let theme = app.theme();
    let emphasis = theme
        .style(SemanticToken::Focus)
        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
    let plain = theme.role(crate::theme::Role::TextMuted);
    let rows = hits
        .iter()
        .flat_map(|hit| {
            let action = Some(SearchAction::Preview(hit.index));
            let mut cells = vec![
                safe_text(&hit.title).into(),
                if hit.line.contains(':') {
                    hit.line.clone().into()
                } else {
                    Cell::styled(hit.line.clone(), Role::TextMuted)
                },
            ];
            if compact {
                let spans =
                    snippet_spans(&hit.snippet, &hit.highlights, usize::MAX, emphasis, plain)
                        .into_iter()
                        .map(|span| (span.content.into_owned(), span.style))
                        .collect::<Vec<_>>();
                cells.push(Cell::spans(spans));
            }
            cells.push(Cell::styled(safe_text(&hit.repository), Role::TextMuted));
            let mut rows = vec![TableRow::cells(cells).with_action(action.clone())];
            if !compact {
                // The snippet is painted below with its highlights, across the list.
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
                emphasis,
                plain,
            );
            frame.buffer_mut().set_line(
                snippet_x,
                snippet.rect.y,
                &ratatui::text::Line::from(spans),
                width,
            );
            if selected == Some(position) {
                // The selected result's snippet shares its row's Selection background.
                frame
                    .buffer_mut()
                    .set_style(snippet.rect, crate::ui_table::selection_style(theme));
            }
        }
        // One click target per result (its row and snippet line): a click selects it, and a
        // click on the selected one opens it, like Enter.
        app.hits.push(HitRegion {
            rect,
            action: Action::Search(if selected == Some(position) {
                SearchAction::Open(hit.index)
            } else {
                SearchAction::Preview(hit.index)
            }),
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
            freshness: preview.freshness.clone(),
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
        // When this result's content was read (UTC), beside the scroll buttons.
        let fresh_x = 26.min(inner.width);
        frame.render_widget(
            Paragraph::new(crate::ui_chrome::fit(
                &preview.freshness,
                usize::from(inner.width - fresh_x),
            ))
            .style(app.theme().role(crate::theme::Role::TextMuted)),
            Rect::new(
                inner.x + fresh_x,
                inner.y + 2 + actor_rows,
                inner.width - fresh_x,
                1.min(inner.height.saturating_sub(2 + actor_rows)),
            ),
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
                "O Options",
                "S Save query",
                "H Sources",
                "B Blend",
                "r Refresh",
                "s Settings",
                "? Help",
                ": Commands",
                "q Quit"
            ]
        );
        // Options leaves before Refresh; the pane keeps its `[O Options]` button.
        assert_eq!(
            placed_tokens(&app, 120),
            [
                "Enter Search",
                "c Corpus",
                "M Repositories",
                "S Save query",
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
            (
                KeyCode::Char('F'),
                Action::Search(SearchAction::Mode(SearchMode::Fuzzy)),
            ),
            (KeyCode::Char('O'), Action::Search(SearchAction::Options)),
            (KeyCode::Char('M'), Action::Corpus),
            (KeyCode::Char('S'), Action::Search(SearchAction::Save)),
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

    fn draw_rows(app: &mut App, width: u16, height: u16) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| {
                app.hits.clear();
                draw(frame, app, Rect::new(0, 0, width, height - 1));
                let controls = crate::footer::main_controls(app);
                crate::footer::draw(frame, app, Rect::new(0, height - 1, width, 1), &controls);
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn design_footer_v90_search_pane_drops_toolbar_rows_and_registers_tab_and_footer_hits() {
        for (width, height) in [(140, 42), (80, 24)] {
            let mut app = search_app();
            // TASK-121: the query box, then one Options line; the tabs wait under Options.
            let rows = draw_rows(&mut app, width, height);
            assert!(rows[0].starts_with("[/ Query] "), "{}", rows[0]);
            assert!(
                rows[1].starts_with("[O Options] Exact • working files • Enter searches"),
                "{}",
                rows[1]
            );
            assert!(
                rows[2].starts_with("Exact mode • enter a query, then Search."),
                "{}",
                rows[2]
            );
            let screen = rows.join("\n");
            for gone in [
                "[Enter Search]",
                "[B Blend controls]",
                "[M Repositories]",
                "[c Corpus]",
                "[o Open result]",
                "x Regex",
                "[d Revision]",
            ] {
                assert!(!screen.contains(gone), "{gone}\n{screen}");
            }
            assert!(
                rows[height as usize - 1].starts_with(" Enter Search  c Corpus  M Repositories  ")
            );
            let options = app
                .hits
                .iter()
                .find(|hit| hit.action == Action::Search(SearchAction::Options))
                .unwrap();
            assert_eq!(options.rect.y, 1);
            // Options shows the mode and source tabs and the revision and name inputs.
            app.dispatch(options.action.clone());
            let rows = draw_rows(&mut app, width, height);
            assert!(rows[1].starts_with("[O Hide options]"), "{}", rows[1]);
            assert!(
                rows[2].starts_with(" F Fuzzy   e Exact   x Regex   n Ranked   h GitHub "),
                "{}",
                rows[2]
            );
            assert!(
                rows[3].starts_with(" w Working files   v Saved version "),
                "{}",
                rows[3]
            );
            assert!(rows[4].starts_with("[d Revision] ") && rows[4].contains("[z Name] "));
            assert!(rows[5].starts_with("Exact mode • enter a query, then Search."));
            // Tabs click to the same action their key resolves to; footer hits come last.
            let ranked = app
                .hits
                .iter()
                .find(|hit| hit.action == Action::Search(SearchAction::Mode(SearchMode::Ranked)))
                .unwrap();
            assert_eq!(ranked.rect.y, 2);
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

    fn file_hit(
        document: &std::sync::Arc<crate::search::model::Document>,
        line: usize,
        snippet: &str,
        word: &str,
    ) -> crate::search::model::SearchHit {
        let at = snippet.find(word).unwrap();
        crate::search::model::SearchHit {
            document: std::sync::Arc::clone(document),
            line,
            column: snippet[..at].chars().count() + 1,
            highlights: std::iter::once(at..at + word.len()).collect(),
            snippet: snippet.into(),
            score: 1.0,
            matched_fields: vec![crate::search::scan::FUZZY_TEXT.into()],
        }
    }

    fn document(path: &str) -> std::sync::Arc<crate::search::model::Document> {
        use crate::search::model::{Destination, Document, DocumentKey, ResultKind, SourceKind};
        std::sync::Arc::new(Document {
            key: DocumentKey {
                scope: Scope::new("local", "workspace").unwrap(),
                repository: "local/current".into(),
                revision: "worktree".into(),
                kind: ResultKind::File,
                path: None,
                resource: path.into(),
            },
            source: SourceKind::Worktree,
            title: path.into(),
            body: String::new(),
            metadata: Default::default(),
            destination: Destination::LocalFile {
                directory: "/tmp/example".into(),
                path: crate::git::RawPath::new(path.as_bytes()),
                revision: None,
                line: 1,
                column: 1,
            },
        })
    }

    #[test]
    fn search_fuzzy_v121_results_group_by_file_with_short_path_line_and_snippet() {
        for (width, height) in [(140, 42), (80, 24)] {
            let mut app = search_app();
            app.search.as_mut().unwrap().mode = SearchMode::Fuzzy;
            app.search_controls.query = TextBuffer::new("needle").unwrap();
            let long = document("deeply/nested/directory/structure/for/the/source/main.rs");
            let short = document("b.txt");
            let page = crate::search::scan::SearchPage {
                query: crate::search::Query::parse("needle", SearchMode::Fuzzy).unwrap(),
                hits: vec![
                    file_hit(&long, 3, "    let needle = 1;", "needle"),
                    file_hit(&long, 9, "needle again", "needle"),
                    file_hit(&short, 1, "beta needle", "needle"),
                ],
                coverage: crate::search::model::Coverage {
                    complete: true,
                    ..Default::default()
                },
            };
            let search = app.search.as_mut().unwrap();
            search.input = "needle".into();
            search.page = Some(page);
            app.search_content_focused = true;
            let rows = draw_rows(&mut app, width, height);
            let screen = rows.join("\n");
            assert!(screen.contains("3 results • Fuzzy"), "{screen}");
            let header = rows.iter().position(|row| row.contains("RESULT")).unwrap();
            assert!(rows[header].contains("LINE"));
            assert!(
                !rows[header].contains("REPOSITORY"),
                "one repository: {}",
                rows[header]
            );
            // The long path keeps its file name; the file's second result leaves it blank.
            // Each snippet (leading blanks dropped) is the line below its result.
            let first = &rows[header + 1];
            assert!(first.contains("…/") && first.contains("main.rs"), "{first}");
            assert!(first.contains("3:9"), "{first}");
            assert!(rows[header + 2].contains("let needle = 1;"));
            let second = &rows[header + 3];
            assert!(
                !second.contains("main.rs") && second.contains("9:1"),
                "{second}"
            );
            assert!(rows[header + 4].contains("needle again"));
            let third = &rows[header + 5];
            assert!(third.contains("b.txt") && third.contains("1:6"), "{third}");
            assert!(rows[header + 6].contains("beta needle"));
            // A click on a result selects it; on the selected one it opens it, like Enter.
            let targets = app
                .hits
                .iter()
                .filter(|hit| hit.rect.y > header as u16 && hit.rect.y <= header as u16 + 6)
                .map(|hit| hit.action.clone())
                .collect::<Vec<_>>();
            assert_eq!(
                targets,
                [
                    Action::Search(SearchAction::Open(0)),
                    Action::Search(SearchAction::Preview(1)),
                    Action::Search(SearchAction::Preview(2)),
                ]
            );
            assert!(handle_event(&mut app, &Event::Key(key(KeyCode::Enter))));
            assert_eq!(app.search_action, Some(SearchAction::Open(0)));
            // Text typed after the search hides the stale page until the live search runs.
            app.search_action = None;
            app.search_controls.editing = Some(SearchField::Query);
            assert!(handle_event(&mut app, &Event::Key(key(KeyCode::Char('s')))));
            assert!(app.search_controls.live_due.is_some());
            let rows = draw_rows(&mut app, width, height);
            assert!(rows.join("\n").contains("Searching for “needles”…"));
            app.search_controls.live_due = Some(std::time::Instant::now());
            tick_live(&mut app);
            assert_eq!(app.search_action, Some(SearchAction::Submit));
            assert_eq!(app.search_controls.editing, Some(SearchField::Query));
        }
    }

    #[test]
    fn search_fuzzy_v121_live_search_only_for_account_free_local_fuzzy_scope() {
        let mut app = search_app();
        app.search_controls.editing = Some(SearchField::Query);
        // Exact keeps Enter-to-search.
        handle_event(&mut app, &Event::Paste("abc".into()));
        assert!(app.search_controls.live_due.is_none());
        app.search.as_mut().unwrap().mode = SearchMode::Fuzzy;
        handle_event(&mut app, &Event::Paste("d".into()));
        assert!(app.search_controls.live_due.is_some());
        // A typed `/` into an empty box only focuses it.
        app.search_controls.query = TextBuffer::default();
        handle_event(&mut app, &Event::Key(key(KeyCode::Char('/'))));
        assert!(app.search_controls.query.text.is_empty());
        // A GitHub account scope is verified first, so Enter searches it.
        app.search.as_mut().unwrap().scope = Scope::github("github.com", Some(7)).unwrap();
        handle_event(&mut app, &Event::Paste("x".into()));
        assert!(app.search_controls.live_due.is_none());
        // An empty query clears instead of searching.
        let mut app = search_app();
        app.search.as_mut().unwrap().mode = SearchMode::Fuzzy;
        app.search_controls.live_due = Some(std::time::Instant::now());
        tick_live(&mut app);
        assert_eq!(app.search_action, None);
    }

    #[test]
    fn search_fuzzy_v121_short_path_and_snippet_window() {
        assert_eq!(short_path("a/b.rs", 36), "a/b.rs");
        let cut = short_path("one/two/three/four/five/six/seven/eight/file.rs", 20);
        assert!(
            cut.starts_with("…/") && cut.ends_with("eight/file.rs"),
            "{cut}"
        );
        assert_eq!(
            short_path(&"x".repeat(50), 10),
            format!("…/{}", "x".repeat(50))
        );
        let snippet = format!("{}needle tail", "a".repeat(40));
        let (text, ranges) = windowed_snippet(&snippet, std::slice::from_ref(&(40..46)));
        assert!(text.starts_with('…'));
        assert_eq!(&text[ranges[0].clone()], "needle");
        let (text, ranges) = windowed_snippet("    let needle", std::slice::from_ref(&(8..14)));
        assert_eq!(text, "let needle");
        assert_eq!(&text[ranges[0].clone()], "needle");
    }
}
