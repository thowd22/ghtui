//! Local input and prepared rendering only: no subprocess, disk or network I/O.
use crate::{
    app::{safe_text, Action, App, HitRegion},
    editor::TextBuffer,
    search::{corpus::LocalSource, workspace::SearchAction, SearchMode},
    theme::SemanticToken,
};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

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

pub fn dispatch(app: &mut App, action: SearchAction) {
    use SearchAction::*;
    match &action {
        BlendControls => crate::search_blend::open(app),
        BlendPreferences => app.search_action = Some(action),
        Query => app.search_controls.editing = Some(SearchField::Query),
        Revision => app.search_controls.editing = Some(SearchField::Revision),
        Name => app.search_controls.editing = Some(SearchField::Name),
        Corpus => {
            if app
                .search
                .as_ref()
                .is_some_and(|search| search.mode == SearchMode::GitHub)
            {
                app.dispatch(Action::Explain("GitHub repository scope comes from --repo, the selected saved repository set, or explicit mapped repositories. Provider indexing limits apply. Use Exact, Regex or Ranked to select local hidden/ignored files; remote scope is never broadened automatically.".into()));
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
                app.dispatch(Action::Explain("GitHub discovery searches its indexed default-branch data. Choose Exact, Regex or Ranked for local working files or saved revisions; no remote revision or regex semantics are substituted.".into()));
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
            app.status =
                "Search is still loading • wait for this query before opening a result".into();
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
    if key.code == KeyCode::Char('M') {
        app.dispatch(Action::Corpus);
        return true;
    }
    if key.code == KeyCode::Char('H') {
        app.dispatch(Action::SearchSource(
            crate::search_sources::SourceAction::Open,
        ));
        return true;
    }
    if key.code == KeyCode::Char('B') {
        app.dispatch(Action::Search(SearchAction::BlendControls));
        return true;
    }
    if key.code == KeyCode::Char('b') && !app.search_controls.corpus {
        app.dispatch(Action::SearchBlend(
            crate::search::fusion::ui::BlendAction::Toggle,
        ));
        return true;
    }
    let action = match key.code {
        KeyCode::Down if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(SearchAction::PreviewScroll(3))
        }
        KeyCode::Up if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(SearchAction::PreviewScroll(-3))
        }
        KeyCode::Char('/') => Some(SearchAction::Query),
        KeyCode::Char('e') => Some(SearchAction::Mode(SearchMode::Exact)),
        KeyCode::Char('x') => Some(SearchAction::Mode(SearchMode::Regex)),
        KeyCode::Char('n') => Some(SearchAction::Mode(SearchMode::Ranked)),
        KeyCode::Char('h') => Some(SearchAction::Mode(SearchMode::GitHub)),
        KeyCode::Char('w') => Some(SearchAction::Worktree),
        KeyCode::Char('v') => Some(SearchAction::SavedVersion),
        KeyCode::Char('d') => Some(SearchAction::Revision),
        KeyCode::Char('z') => Some(SearchAction::Name),
        KeyCode::Char('f') => Some(SearchAction::Save),
        KeyCode::Char('c') => Some(SearchAction::Corpus),
        KeyCode::Char('i') if app.search_controls.corpus => Some(SearchAction::ToggleIgnored),
        KeyCode::Char('b') if app.search_controls.corpus => Some(SearchAction::ToggleHidden),
        KeyCode::Char('u') => app
            .search
            .as_ref()
            .and_then(|search| search.saved.first())
            .map(|saved| SearchAction::Saved(saved.name.clone())),
        KeyCode::Char('p') => Some(SearchAction::Preview(app.search_controls.selected)),
        KeyCode::Char('o') => Some(SearchAction::Open(app.search_controls.selected)),
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
pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    frame.render_widget(Clear, area);
    app.search_controls.fields.clear();
    app.search_controls.preview_rect = Rect::default();
    if area.height < 7 {
        frame.render_widget(Paragraph::new("Resize terminal for search controls"), area);
        return;
    }
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
    ])
    .split(area);
    let mode = app
        .search
        .as_ref()
        .map_or(SearchMode::Exact, |search| search.mode);
    let mut x = rows[0].x;
    for (mode_value, label) in [
        (SearchMode::Exact, "[e Exact]"),
        (SearchMode::Regex, "[x Regex]"),
        (SearchMode::Ranked, "[n Ranked]"),
        (SearchMode::GitHub, "[h GitHub]"),
    ] {
        let width = (label.len() as u16 + 1).min(rows[0].right().saturating_sub(x));
        button(
            frame,
            app,
            Rect::new(x, rows[0].y, width, 1),
            label,
            SearchAction::Mode(mode_value),
            mode == mode_value,
        );
        x += width;
    }
    button(
        frame,
        app,
        Rect::new(x, rows[0].y, 14.min(rows[0].right().saturating_sub(x)), 1),
        "[Enter Search]",
        SearchAction::Submit,
        false,
    );
    // Keep saved-query names on their original row. The mode toolbar has room
    // for this control even at80 columns; adding it beside saved queries would
    // hide their full labels and mouse targets after restart.
    button(
        frame,
        app,
        Rect::new(
            x.saturating_add(15),
            rows[0].y,
            19.min(rows[0].right().saturating_sub(x.saturating_add(15))),
            1,
        ),
        "[B Blend controls]",
        SearchAction::BlendControls,
        false,
    );
    let worktree = app.search.as_ref().is_none_or(|search| {
        search
            .selections
            .iter()
            .all(|selection| selection.source == LocalSource::Worktree)
    });
    let remote = mode == SearchMode::GitHub;
    for (start, width, label, action, active) in [
        (
            0,
            19,
            if remote {
                "[w Indexed GitHub]"
            } else {
                "[w Working files]"
            },
            SearchAction::Worktree,
            worktree && !remote,
        ),
        (
            19,
            18,
            if remote {
                "[v Revision: n/a]"
            } else {
                "[v Saved version]"
            },
            SearchAction::SavedVersion,
            !worktree && !remote,
        ),
        (
            37,
            12,
            "[c Corpus]",
            SearchAction::Corpus,
            app.search_controls.corpus,
        ),
    ] {
        button(
            frame,
            app,
            Rect::new(
                rows[1].x + start.min(rows[1].width),
                rows[1].y,
                width.min(rows[1].width.saturating_sub(start)),
                1,
            ),
            label,
            action,
            active,
        );
    }
    let blend_label = format!(
        "[b Blend GitHub: {}]",
        if app
            .search
            .as_ref()
            .is_some_and(|search| search.blend_policy.enabled)
        {
            "on"
        } else {
            "off"
        }
    );
    let blend_rect = Rect::new(
        rows[1].x + 50.min(rows[1].width),
        rows[1].y,
        (blend_label.len() as u16).min(rows[1].width.saturating_sub(50)),
        1,
    );
    if !blend_rect.is_empty() {
        frame.render_widget(
            Paragraph::new(blend_label).style(if app.focus == app.hits.len() {
                app.theme().style(SemanticToken::Focus)
            } else {
                app.theme().base()
            }),
            blend_rect,
        );
        app.hits.push(HitRegion {
            rect: blend_rect,
            action: Action::SearchBlend(crate::search::fusion::ui::BlendAction::Toggle),
        });
    }
    field(
        frame,
        app,
        rows[2],
        "[/ Query] ",
        SearchField::Query,
        SearchAction::Query,
    );
    field(
        frame,
        app,
        rows[3],
        "[d Revision] ",
        SearchField::Revision,
        SearchAction::Revision,
    );
    let name_area = Rect::new(rows[4].x, rows[4].y, rows[4].width.saturating_sub(16), 1);
    field(
        frame,
        app,
        name_area,
        "[z Name] ",
        SearchField::Name,
        SearchAction::Name,
    );
    button(
        frame,
        app,
        Rect::new(name_area.right(), rows[4].y, 16.min(rows[4].width), 1),
        "[f Save query]",
        SearchAction::Save,
        false,
    );
    let saved = app.search.as_ref().map_or_else(Vec::new, |search| {
        search
            .saved
            .iter()
            .map(|saved| saved.name.clone())
            .collect::<Vec<_>>()
    });
    let corpus_button = Rect::new(rows[5].x, rows[5].y, 18.min(rows[5].width), 1);
    let focused = app.focus == app.hits.len();
    frame.render_widget(
        Paragraph::new("[M Repositories]").style(if focused {
            app.theme().style(SemanticToken::Focus)
        } else {
            app.theme().base()
        }),
        corpus_button,
    );
    if !corpus_button.is_empty() {
        app.hits.push(HitRegion {
            rect: corpus_button,
            action: Action::Corpus,
        });
    }
    let source_button = Rect::new(
        corpus_button.right(),
        rows[5].y,
        20.min(rows[5].right().saturating_sub(corpus_button.right())),
        1,
    );
    frame.render_widget(
        Paragraph::new("[H Search sources]").style(if app.focus == app.hits.len() {
            app.theme().style(SemanticToken::Focus)
        } else {
            app.theme().base()
        }),
        source_button,
    );
    if !source_button.is_empty() {
        app.hits.push(HitRegion {
            rect: source_button,
            action: Action::SearchSource(crate::search_sources::SourceAction::Open),
        });
    }
    let mut x = source_button.right();
    for (index, name) in saved.into_iter().enumerate() {
        let label = format!(
            "[{} {}] ",
            if index == 0 { "u" } else { "saved" },
            safe_text(&name)
        );
        let width = (ratatui::text::Line::from(label.as_str()).width() as u16)
            .min(rows[5].right().saturating_sub(x));
        button(
            frame,
            app,
            Rect::new(x, rows[5].y, width, 1),
            label,
            SearchAction::Saved(name),
            false,
        );
        x += width;
    }
    let body = rows[6];
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
        frame.render_widget(Paragraph::new(format!("{:?} mode • enter a query, then Search.\n{}\nFilters: repo: org: set: type: path: language: author: state: after: before: rev:\n{}",mode,search.context(),search.warnings.join("\n"))),body);
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
    let panels = if available.width >= 100 {
        Layout::horizontal([Constraint::Percentage(48), Constraint::Percentage(52)])
            .split(available)
    } else {
        Layout::vertical([
            Constraint::Length((available.height / 2).max(2)),
            Constraint::Min(1),
        ])
        .split(available)
    };
    app.search_controls.visible = (panels[0].height as usize / 2).max(1);
    // Copy only small visible labels/snippets, never entire source documents.
    let hits = page
        .hits
        .iter()
        .enumerate()
        .skip(app.search_controls.top)
        .take(app.search_controls.visible)
        .map(|(index, hit)| RenderHit {
            index,
            title: hit.document.title.clone(),
            repository: hit.document.key.repository.clone(),
            line: hit.line,
            column: hit.column,
            snippet: hit.snippet.clone(),
            highlights: hit.highlights.clone(),
        })
        .collect::<Vec<_>>();
    for (offset, hit) in hits.iter().enumerate() {
        let index = hit.index;
        let row = Rect::new(
            panels[0].x,
            panels[0].y + offset as u16 * 2,
            panels[0].width,
            2.min(
                panels[0]
                    .bottom()
                    .saturating_sub(panels[0].y + offset as u16 * 2),
            ),
        );
        let label = format!(
            "{} {}{} • {}",
            if index == app.search_controls.selected {
                ">"
            } else {
                " "
            },
            safe_text(&hit.title),
            if hit.line == 0 {
                " • provider fragment".into()
            } else {
                format!(":{}:{}", hit.line, hit.column)
            },
            hit.repository
        );
        let mut spans = Vec::new();
        let mut at = 0;
        for range in &hit.highlights {
            if range.start >= at
                && range.end <= hit.snippet.len()
                && hit.snippet.is_char_boundary(range.start)
                && hit.snippet.is_char_boundary(range.end)
            {
                spans.push(Span::raw(safe_text(&hit.snippet[at..range.start])));
                spans.push(Span::styled(
                    safe_text(&hit.snippet[range.clone()]),
                    app.theme()
                        .style(SemanticToken::Focus)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                ));
                at = range.end;
            }
        }
        spans.push(Span::raw(safe_text(&hit.snippet[at..])));
        frame.render_widget(
            Paragraph::new(vec![Line::from(label), Line::from(spans)]).style(
                if index == app.search_controls.selected {
                    app.theme().style(SemanticToken::Focus)
                } else {
                    app.theme().base()
                },
            ),
            row,
        );
        app.hits.push(HitRegion {
            rect: row,
            action: Action::Search(SearchAction::Preview(index)),
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
        frame.render_widget(
            Block::default()
                .borders(Borders::LEFT)
                .title(format!(" Preview {} ", safe_text(&preview.title))),
            panels[1],
        );
        let inner = panels[1].inner(ratatui::layout::Margin::new(1, 0));
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
        button(
            frame,
            app,
            Rect::new(
                inner.x,
                inner.y + 2 + actor_rows,
                16.min(inner.width),
                1.min(inner.height.saturating_sub(2 + actor_rows)),
            ),
            "[o Open result]",
            SearchAction::Open(app.search_controls.selected),
            false,
        );
        let more = Rect::new(
            inner.x + 17.min(inner.width),
            inner.y + 2 + actor_rows,
            20.min(inner.width.saturating_sub(17)),
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
            inner.x + 38.min(inner.width),
            inner.y + 2 + actor_rows,
            20.min(inner.width.saturating_sub(38)),
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
    button(
        frame,
        app,
        Rect::new(area.x, area.y, 25.min(area.width), 1),
        format!("[b Hidden files: {hidden}]"),
        SearchAction::ToggleHidden,
        hidden,
    );
    button(
        frame,
        app,
        Rect::new(
            area.x + 26.min(area.width),
            area.y,
            27.min(area.width.saturating_sub(26)),
            1,
        ),
        format!("[i Ignored files: {ignored}]"),
        SearchAction::ToggleIgnored,
        ignored,
    );
    frame.render_widget(
        Paragraph::new("Selected existing checkouts only • no automatic clone/fetch"),
        Rect::new(
            area.x,
            area.y + 1,
            area.width,
            1.min(area.height.saturating_sub(1)),
        ),
    );
    let visible = area.height.saturating_sub(2) as usize;
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
            Rect::new(area.x, area.y + 2 + offset as u16, area.width, 1),
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
}
