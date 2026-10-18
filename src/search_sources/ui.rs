//! Layout-derived hits and local editing only. Source work is dispatched to a background worker.
use super::{SourceAction, SourceKind};
use crate::{
    app::{safe_text, Action, App, HitRegion, Overlay, View},
    theme::SemanticToken,
};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    layout::{Margin, Rect},
    style::Modifier,
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

pub fn dispatch(app: &mut App, action: SourceAction) {
    match action {
        SourceAction::Open => {
            app.source_controls
                .choose_kind(app.source_controls.kind, app.search.as_ref());
            app.source_controls.editing = None;
            app.source_controls.message =
                "Load reads gh/Git and writes only local cache. No remote mutation or clone."
                    .into();
            app.overlay = Some(Overlay::SearchSources);
            app.focus = 0;
            app.search_controls.editing = None;
        }
        SourceAction::Kind(kind) => {
            if !app.loading[View::Search.index()] {
                app.source_controls.choose_kind(kind, app.search.as_ref());
            }
        }
        SourceAction::Field(index) => {
            if !app.loading[View::Search.index()] && index < 5 {
                if index == 4 && app.source_controls.kind == SourceKind::Commits {
                    app.source_controls.message="Local commit messages use installed Git; no API paging or gh authentication.".into();
                } else {
                    app.source_controls.editing = Some(index);
                }
            }
        }
        SourceAction::Load | SourceAction::Remove => {
            app.source_controls.editing = None;
            if app.loading[View::Search.index()] {
                app.source_controls.message =
                    "Wait for this source read, or Cancel. No duplicate request was started."
                        .into();
            } else {
                match app.source_controls.request() {
                    Ok(_) => app.source_action = Some(action),
                    Err(error) => app.source_controls.message = error,
                }
            }
        }
        SourceAction::Close => {
            if app.loading[View::Search.index()] {
                app.cancel_requested = true;
            }
            app.overlay = None;
            app.source_controls.editing = None;
        }
        SourceAction::ContextScroll(delta) => {
            let maximum = app.source_context.as_ref().map_or(0, |context| {
                context
                    .rows
                    .len()
                    .saturating_sub(app.source_context_visible)
            });
            app.source_context_scroll = app
                .source_context_scroll
                .saturating_add_signed(delta as isize)
                .min(maximum);
        }
        SourceAction::ContextClose => {
            app.source_context = None;
            app.source_context_scroll = 0;
            app.overlay = None;
        }
    }
}
pub fn handle_event(app: &mut App, event: &Event) {
    if app.overlay == Some(Overlay::SearchContext) {
        handle_context_event(app, event);
        return;
    }
    match event {
        Event::Key(key) if key.kind != KeyEventKind::Release => {
            let control = key.modifiers.contains(KeyModifiers::CONTROL);
            let shift = key.modifiers.contains(KeyModifiers::SHIFT);
            if control && key.code == KeyCode::Char('c') {
                app.dispatch(Action::Quit);
                return;
            }
            if key.code == KeyCode::Esc {
                if app.source_controls.editing.take().is_none() {
                    dispatch(app, SourceAction::Close);
                }
                return;
            }
            if matches!(key.code, KeyCode::Tab | KeyCode::BackTab) {
                app.source_controls.editing = None;
                app.focus = (app.focus
                    + if key.code == KeyCode::Tab {
                        1
                    } else {
                        app.hits.len().saturating_sub(1)
                    })
                    % app.hits.len().max(1);
                return;
            }
            if let Some(index) = app.source_controls.editing {
                if key.code == KeyCode::Enter {
                    app.source_controls.editing = None;
                    return;
                }
                let field = &mut app.source_controls.fields[index];
                match key.code {
                    KeyCode::Char('a') if control => field.select_all(),
                    KeyCode::Char('u') if control => field.clear(),
                    KeyCode::Char(character)
                        if !control && !key.modifiers.contains(KeyModifiers::ALT) =>
                    {
                        let _ = field.insert(&character.to_string());
                    }
                    KeyCode::Left => field.horizontal(false, shift),
                    KeyCode::Right => field.horizontal(true, shift),
                    KeyCode::Home => field.line_edge(false, control, shift),
                    KeyCode::End => field.line_edge(true, control, shift),
                    KeyCode::Backspace => field.delete_backward(),
                    KeyCode::Delete => field.delete_forward(),
                    _ => {}
                }
                return;
            }
            let action = match key.code {
                KeyCode::Char('p') => Some(SourceAction::Kind(SourceKind::PullRequest)),
                KeyCode::Char('i') => Some(SourceAction::Kind(SourceKind::Issue)),
                KeyCode::Char('g') => Some(SourceAction::Kind(SourceKind::Commits)),
                KeyCode::Char('l') => Some(SourceAction::Load),
                KeyCode::Char('d') => Some(SourceAction::Remove),
                KeyCode::Char('r') => Some(SourceAction::Field(0)),
                KeyCode::Char('v') => Some(SourceAction::Field(1)),
                KeyCode::Enter => app.hits.get(app.focus).and_then(|hit| {
                    if let Action::SearchSource(action) = &hit.action {
                        Some(action.clone())
                    } else {
                        None
                    }
                }),
                _ => None,
            };
            if let Some(action) = action {
                dispatch(app, action);
            }
        }
        Event::Paste(text) => {
            if let Some(index) = app.source_controls.editing {
                let _ = app.source_controls.fields[index]
                    .insert(&text.replace(['\n', '\r', '\t'], " "));
            }
        }
        Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
            if let Some((focus, hit)) = app
                .hits
                .iter()
                .enumerate()
                .find(|(_, hit)| hit.rect.contains((mouse.column, mouse.row).into()))
            {
                let action = hit.action.clone();
                app.focus = focus;
                app.dispatch(action);
                if let Some(index) = app.source_controls.editing {
                    if let Some((_, rect)) = app
                        .source_controls
                        .field_rects
                        .iter()
                        .find(|(field, _)| *field == index)
                    {
                        if rect.contains((mouse.column, mouse.row).into()) {
                            let left = app.source_controls.fields[index].left_column;
                            app.source_controls.fields[index].click(
                                0,
                                left + mouse.column.saturating_sub(rect.x) as usize,
                                false,
                            );
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

fn handle_context_event(app: &mut App, event: &Event) {
    let action = match event {
        Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.dispatch(Action::Quit);
                return;
            }
            KeyCode::Esc => Some(SourceAction::ContextClose),
            KeyCode::Down | KeyCode::Char('j') => Some(SourceAction::ContextScroll(1)),
            KeyCode::Up | KeyCode::Char('k') => Some(SourceAction::ContextScroll(-1)),
            KeyCode::PageDown => Some(SourceAction::ContextScroll(
                app.source_context_visible as i32,
            )),
            KeyCode::PageUp => Some(SourceAction::ContextScroll(
                -(app.source_context_visible as i32),
            )),
            KeyCode::Tab | KeyCode::BackTab => {
                app.focus = (app.focus
                    + if key.code == KeyCode::Tab {
                        1
                    } else {
                        app.hits.len().saturating_sub(1)
                    })
                    % app.hits.len().max(1);
                return;
            }
            KeyCode::Enter => app.hits.get(app.focus).and_then(|hit| {
                if let Action::SearchSource(action) = &hit.action {
                    Some(action.clone())
                } else {
                    None
                }
            }),
            _ => None,
        },
        Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => app
            .hits
            .iter()
            .enumerate()
            .find(|(_, hit)| hit.rect.contains((mouse.column, mouse.row).into()))
            .and_then(|(focus, hit)| {
                app.focus = focus;
                if let Action::SearchSource(action) = &hit.action {
                    Some(action.clone())
                } else {
                    None
                }
            }),
        Event::Mouse(mouse)
            if matches!(
                mouse.kind,
                MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
            ) =>
        {
            Some(SourceAction::ContextScroll(
                if mouse.kind == MouseEventKind::ScrollUp {
                    -3
                } else {
                    3
                },
            ))
        }
        _ => None,
    };
    if let Some(action) = action {
        dispatch(app, action);
    }
}

pub fn draw_context(frame: &mut Frame, app: &mut App, area: Rect) {
    app.hits.clear();
    let width = area.width.saturating_sub(4).min(76);
    let height = area.height.saturating_sub(2).min(34);
    let rect = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .title(" Exact search source • read-only ")
            .style(app.theme().base()),
        rect,
    );
    let inner = rect.inner(Margin::new(2, 1));
    let theme = app.theme();
    let Some(context) = &app.source_context else {
        return;
    };
    let actor_rows = (context.actors.len().min(2) * 2) as u16;
    if inner.width >= 68 && inner.height > actor_rows + 3 {
        for (index, actor) in context.actors.iter().take(2).enumerate() {
            let image = app.avatar_states.get(actor).and_then(|state| {
                if let crate::identity::AvatarState::Ready(image) = state {
                    Some(image)
                } else {
                    None
                }
            });
            frame.render_widget(
                crate::identity::ActorWidget {
                    identity: actor,
                    image: image.map(std::sync::Arc::as_ref),
                    theme,
                },
                Rect::new(inner.x, inner.y + index as u16 * 2, inner.width, 2),
            );
        }
        let body = Rect::new(
            inner.x,
            inner.y + actor_rows,
            68,
            inner.height - actor_rows - 2,
        );
        app.source_context_visible = body.height as usize;
        app.source_context_scroll = app.source_context_scroll.min(
            context
                .rows
                .len()
                .saturating_sub(app.source_context_visible),
        );
        let rows = context
            .rows
            .iter()
            .skip(app.source_context_scroll)
            .take(app.source_context_visible)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        frame.render_widget(Paragraph::new(rows).style(theme.base()), body);
    } else {
        frame.render_widget(
            Paragraph::new(
                "Resize to at least 76×14 for exact source identity and body. Use Close to return.",
            ),
            inner,
        );
    }
    let y = inner.bottom().saturating_sub(1);
    for (x, width, label, action) in [
        (
            0,
            20,
            "[PgUp Previous]",
            SourceAction::ContextScroll(-(app.source_context_visible as i32)),
        ),
        (
            21,
            17,
            "[PgDn More]",
            SourceAction::ContextScroll(app.source_context_visible as i32),
        ),
        (39, 14, "[Esc Close]", SourceAction::ContextClose),
    ] {
        let left = inner.x + x.min(inner.width);
        button(
            frame,
            app,
            Rect::new(left, y, width.min(inner.right().saturating_sub(left)), 1),
            label,
            action,
            false,
        );
    }
}
fn button(
    frame: &mut Frame,
    app: &mut App,
    rect: Rect,
    label: impl Into<String>,
    action: SourceAction,
    active: bool,
) {
    if rect.is_empty() {
        return;
    }
    let mut style = app.theme().style(if active {
        SemanticToken::Selection
    } else {
        SemanticToken::Muted
    });
    if app.focus == app.hits.len() {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    frame.render_widget(Paragraph::new(label.into()).style(style), rect);
    app.hits.push(HitRegion {
        rect,
        action: Action::SearchSource(action),
    });
}
pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    app.hits.clear();
    app.source_controls.field_rects.clear();
    let width = area.width.saturating_sub(4).min(100);
    let height = area.height.saturating_sub(2).min(21);
    let rect = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .title(" Search sources • explicit read-only hydration ")
            .style(app.theme().base()),
        rect,
    );
    let inner = rect.inner(Margin::new(2, 1));
    if inner.width < 60 || inner.height < 17 {
        frame.render_widget(
            Paragraph::new(
                "Resize to at least 68×21 to edit source controls. No source work was started.",
            ),
            inner,
        );
        button(
            frame,
            app,
            Rect::new(
                inner.x,
                inner.bottom().saturating_sub(1),
                inner.width.min(14),
                1,
            ),
            "[Esc Close]",
            SourceAction::Close,
            false,
        );
        return;
    }
    for (x, width, label, kind) in [
        (0, 12, "[p PR]", SourceKind::PullRequest),
        (12, 13, "[i Issue]", SourceKind::Issue),
        (25, 31, "[g Local commit messages]", SourceKind::Commits),
    ] {
        button(
            frame,
            app,
            Rect::new(inner.x + x, inner.y, width, 1),
            label,
            SourceAction::Kind(kind),
            app.source_controls.kind == kind,
        );
    }
    let operator = app
        .search
        .as_ref()
        .and_then(|search| search.remote_identity.as_ref())
        .map_or_else(
            || "Local sources need no GitHub login; gh verifies remote sources.".into(),
            |identity| {
                format!(
                    "Operator @{} • account #{} • {}",
                    identity.login,
                    identity.account_id.unwrap_or(0),
                    identity.host
                )
            },
        );
    frame.render_widget(
        Paragraph::new(safe_text(&operator)),
        Rect::new(inner.x, inner.y + 1, inner.width, 1),
    );
    let labels = [
        "[r Repository]",
        if app.source_controls.kind == SourceKind::Commits {
            "[v Saved revision]"
        } else {
            "[v PR/issue number]"
        },
        "Max documents/commits",
        "Max bytes/category",
        "Max API pages",
    ];
    for (index, label) in labels.into_iter().enumerate() {
        let row = Rect::new(inner.x, inner.y + 3 + index as u16, inner.width, 1);
        let field_rect = Rect::new(row.x + 23, row.y, row.width - 23, 1);
        let value = if index == 4 && app.source_controls.kind == SourceKind::Commits {
            "n/a • local Git only".into()
        } else {
            let editor = &mut app.source_controls.fields[index];
            editor.ensure_visible(field_rect.width as usize, 1);
            crate::editor::display_line(
                &safe_text(&editor.text),
                editor.left_column,
                field_rect.width as usize,
            )
        };
        button(
            frame,
            app,
            row,
            format!("{label:<23}{value}"),
            SourceAction::Field(index),
            app.source_controls.editing == Some(index),
        );
        app.source_controls.field_rects.push((index, field_rect));
        if app.source_controls.editing == Some(index) {
            let editor = &app.source_controls.fields[index];
            let column = editor
                .position()
                .1
                .saturating_sub(editor.left_column)
                .min(field_rect.width.saturating_sub(1) as usize);
            frame.set_cursor_position((field_rect.x + column as u16, field_rect.y));
        }
    }
    let counts = app.search.as_ref().map_or((0, 0), |search| {
        (
            search.metadata_sources.discussions.len(),
            search.metadata_sources.commits.len(),
        )
    });
    let note=format!("Selected: {} discussion categories, {} commit histories.\nPR loads body, general comments, reviews and inline threads.\nCaps apply per fetch category and to combined searchable sources.\n{}",counts.0,counts.1,if app.loading[View::Search.index()] {"Loading source… Cancel is safe; partial coverage is never called full history."}else{&app.source_controls.message});
    frame.render_widget(
        Paragraph::new(safe_text(&note)).wrap(ratatui::widgets::Wrap { trim: false }),
        Rect::new(
            inner.x,
            inner.y + 9,
            inner.width,
            inner.height.saturating_sub(11),
        ),
    );
    let y = inner.bottom() - 1;
    for (x, width, label, action) in [
        (0, 17, "[l Load source]", SourceAction::Load),
        (18, 20, "[d Remove source]", SourceAction::Remove),
        (39, 14, "[Esc Close]", SourceAction::Close),
    ] {
        button(
            frame,
            app,
            Rect::new(inner.x + x, y, width, 1),
            label,
            action,
            false,
        );
    }
}
