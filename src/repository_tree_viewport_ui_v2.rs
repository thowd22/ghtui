//! Geometry and borrowed prepared fields over the ONE original Tree controller.
use crate::{
    app::{Action, App, HitRegion},
    repository_tree_jump_v1::{OriginalTreeFocusV1 as Focus, OriginalTreeJumpActionV1 as Jump},
    repository_tree_viewport_v2::{TreeDetailFieldV2 as Field, TreeViewportActionV2 as Vp},
    theme::{SemanticToken, Theme},
};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    layout::Rect,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

fn button(
    frame: &mut Frame,
    app: &mut App,
    rect: Rect,
    label: &str,
    action: Action,
    selected: bool,
    theme: Theme,
) {
    if rect.is_empty() {
        return;
    }
    let focused = app
        .original_tree_jump_v1
        .as_ref()
        .is_some_and(|jump| jump.focus != Focus::Query)
        && app.focus == app.hits.len();
    frame.render_widget(
        Paragraph::new(label).style(crate::repository_tree_viewport_v2::focused_style_v2(
            theme,
            SemanticToken::Reference,
            selected,
            focused,
        )),
        rect,
    );
    app.hits.push(HitRegion { rect, action });
}
fn focused_action(action: &Action) -> Focus {
    match action {
        Action::OriginalTreeV1(Jump::FocusQuery | Jump::QueryClick(_)) => Focus::Query,
        Action::OriginalTreeV1(Jump::FocusDetail)
        | Action::OriginalTreeViewportV2(Vp::DetailField(_) | Vp::DetailScroll(_)) => Focus::Detail,
        Action::OriginalTreeV1(Jump::Page(-1)) => Focus::Previous,
        Action::OriginalTreeV1(Jump::Page(_)) => Focus::Next,
        Action::OriginalTreeV1(Jump::Close) => Focus::Close,
        Action::OriginalTreeV1(Jump::Choose)
        | Action::OriginalTreeViewportV2(Vp::Retry(_) | Vp::Commands) => Focus::Choose,
        _ => Focus::List,
    }
}
pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) -> bool {
    if app.original_tree_jump_v1.is_none() {
        return false;
    }
    let theme = app.theme();
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .title("Jump to saved path • original source")
            .style(theme.base()),
        area,
    );
    app.hits.clear();
    let inner = area.inner(ratatui::layout::Margin::new(1, 1));
    if inner.is_empty() {
        return true;
    }
    let close = Rect::new(
        inner.right().saturating_sub(11).max(inner.x),
        inner.bottom() - 1,
        inner.width.min(11),
        1,
    );
    if inner.height < 12 || inner.width < 30 {
        button(
            frame,
            app,
            close,
            "[Esc Close]",
            Action::OriginalTreeV1(Jump::Close),
            false,
            theme,
        );
        return true;
    }
    let jump = app.original_tree_jump_v1.as_ref().unwrap();
    let saved_rows = (jump.intent_sha_v1().len() as u16)
        .div_ceil(inner.width)
        .max(1);
    frame.render_widget(
        Paragraph::new("Saved version • original source")
            .style(theme.style(SemanticToken::Reference)),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    frame.render_widget(
        Paragraph::new(jump.intent_sha_v1())
            .wrap(Wrap { trim: false })
            .style(theme.style(SemanticToken::Reference)),
        Rect::new(inner.x, inner.y + 1, inner.width, saved_rows),
    );
    let status_y = inner.y + 1 + saved_rows;
    let failed = app.workspace.as_ref().is_some_and(|workspace| {
        app.original_tree_viewport_v2
            .failure
            .as_ref()
            .is_some_and(|failure| failure.is_current(workspace, jump))
    });
    if failed {
        let reason = jump
            .status
            .split(" • Retry is explicit")
            .next()
            .unwrap_or(jump.status.as_str());
        frame.render_widget(
            Paragraph::new(reason).style(theme.style(SemanticToken::Reference)),
            Rect::new(inner.x, status_y, inner.width, 1),
        );
        frame.render_widget(
            Paragraph::new("Retry is explicit; no read resent")
                .style(theme.style(SemanticToken::Reference)),
            Rect::new(inner.x, status_y + 1, inner.width, 1),
        );
    } else {
        frame.render_widget(
            Paragraph::new(jump.status.as_str())
                .wrap(Wrap { trim: false })
                .style(theme.style(SemanticToken::Reference)),
            Rect::new(inner.x, status_y, inner.width, 2),
        );
    }
    let query_y = status_y + 2;
    let filter = Rect::new(inner.x, query_y, 8, 1);
    let query = Rect::new(inner.x + 8, query_y, inner.width - 8, 1);
    let mut query_style = theme.base();
    if jump.focus == Focus::Query {
        query_style = query_style.add_modifier(ratatui::style::Modifier::UNDERLINED);
    }
    frame.render_widget(Paragraph::new("Filter:").style(query_style), filter);
    app.hits.push(HitRegion {
        rect: filter,
        action: Action::OriginalTreeV1(Jump::FocusQuery),
    });
    app.original_tree_viewport_v2.query_area = query;
    let jump = app.original_tree_jump_v1.as_mut().unwrap();
    jump.query.ensure_visible(usize::from(query.width), 1);
    frame.render_widget(
        Paragraph::new(crate::editor::display_line(
            &jump.query.text,
            jump.query.left_column,
            usize::from(query.width),
        )),
        query,
    );
    if jump.focus == Focus::Query {
        let caret = crate::editor::display_width(&jump.query.text[..jump.query.cursor])
            .saturating_sub(jump.query.left_column)
            .min(usize::from(query.width.saturating_sub(1)));
        frame.set_cursor_position((query.x + caret as u16, query.y));
    }
    let content = Rect::new(
        inner.x,
        query_y + 1,
        inner.width,
        inner.bottom().saturating_sub(query_y + 3),
    );
    let (list, detail) = if inner.width >= 100 {
        let left = inner.width.saturating_sub(78).min(40);
        (
            Rect::new(content.x, content.y, left, content.height),
            Rect::new(
                content.x + left,
                content.y,
                inner.width - left,
                content.height,
            ),
        )
    } else {
        let top = content.height.div_ceil(2);
        (
            Rect::new(content.x, content.y, content.width, top),
            Rect::new(
                content.x,
                content.y + top,
                content.width,
                content.height - top,
            ),
        )
    };
    app.original_tree_viewport_v2.list_area = list;
    app.original_tree_viewport_v2.detail_area = detail;
    let jump = app.original_tree_jump_v1.as_mut().unwrap();
    if jump.selected < jump.list_scroll {
        jump.list_scroll = jump.selected;
    } else if jump.selected >= jump.list_scroll + usize::from(list.height) {
        jump.list_scroll = jump.selected + 1 - usize::from(list.height.max(1));
    }
    if let Some(source) = jump.frame() {
        for (offset, row) in source
            .matches()
            .iter()
            .enumerate()
            .skip(jump.list_scroll)
            .take(usize::from(list.height))
        {
            let rect = Rect::new(
                list.x,
                list.y + (offset - jump.list_scroll) as u16,
                list.width,
                1,
            );
            let style = if offset == jump.selected {
                theme.selected(SemanticToken::Reference)
            } else {
                theme.base()
            };
            frame.render_widget(Paragraph::new(row.label()).style(style), rect);
            if let Some(ticket) = source.ticket(offset) {
                app.hits.push(HitRegion {
                    rect,
                    action: Action::OriginalTreeV1(Jump::Activate(ticket)),
                });
            }
        }
        if source.matches().is_empty() {
            frame.render_widget(
                Paragraph::new("No known matching path; partial tree leaves unseen paths unknown")
                    .wrap(Wrap { trim: false }),
                list,
            );
        }
    } else {
        frame.render_widget(
            Paragraph::new("Original source/filter pending • no old rows may be opened")
                .wrap(Wrap { trim: false }),
            list,
        );
    }
    let mut x = detail.x;
    for field in Field::ALL {
        let label = format!("[{}]", field.label());
        let rect = Rect::new(x, detail.y, label.len() as u16, 1);
        if rect.right() > detail.right() {
            break;
        }
        let selected = app.original_tree_viewport_v2.field == field;
        button(
            frame,
            app,
            rect,
            &label,
            Action::OriginalTreeViewportV2(Vp::DetailField(field)),
            selected,
            theme,
        );
        x = rect.right() + 1;
    }
    frame.render_widget(
        Paragraph::new(format!(
            "Detail: {}",
            app.original_tree_viewport_v2.field.label()
        )),
        Rect::new(detail.x, detail.y + 1, detail.width, 1),
    );
    let body = Rect::new(
        detail.x,
        detail.y + 2,
        detail.width,
        detail.height.saturating_sub(2),
    );
    let state = &app.original_tree_viewport_v2;
    let jump = app.original_tree_jump_v1.as_ref().unwrap();
    if let Some(prepared) = state.cache.ready(
        jump,
        crate::repository_tree_viewport_native_v2::member_width(state.width),
        state.field,
    ) {
        for (offset, row) in prepared
            .rows()
            .iter()
            .skip(jump.detail_scroll)
            .take(usize::from(body.height))
            .enumerate()
        {
            frame.render_widget(
                Paragraph::new(row.as_str()).style(theme.style(SemanticToken::Reference)),
                Rect::new(body.x, body.y + offset as u16, body.width, 1),
            );
        }
    } else {
        let text = if state.error.is_empty() {
            "Original member detail pending/unavailable • no old identity shown"
        } else {
            state.error.as_str()
        };
        frame.render_widget(
            Paragraph::new(text)
                .wrap(Wrap { trim: false })
                .style(theme.style(SemanticToken::Muted)),
            body,
        );
    }
    app.hits.push(HitRegion {
        rect: body,
        action: Action::OriginalTreeV1(Jump::FocusDetail),
    });
    let narrow = inner.width < 70;
    let failure = app
        .workspace
        .as_ref()
        .zip(app.original_tree_jump_v1.as_ref())
        .and_then(|(workspace, jump)| {
            app.original_tree_viewport_v2
                .failure
                .as_ref()
                .filter(|failure| failure.is_current(workspace, jump))
        })
        .cloned();
    let (open, open_action) = if let Some(failure) = failure {
        (
            "[Retry]",
            Action::OriginalTreeViewportV2(Vp::Retry(failure)),
        )
    } else {
        (
            if narrow { "[Open]" } else { "[Enter Open]" },
            Action::OriginalTreeV1(Jump::Choose),
        )
    };
    let controls = [
        (open, open_action),
        (
            if narrow { "[P ‹]" } else { "[P Previous100]" },
            Action::OriginalTreeV1(Jump::Page(-1)),
        ),
        (
            if narrow { "[N ›]" } else { "[N Next100]" },
            Action::OriginalTreeV1(Jump::Page(1)),
        ),
        (
            if narrow { "[Cmds]" } else { "[Commands]" },
            Action::OriginalTreeViewportV2(Vp::Commands),
        ),
        ("[Esc Close]", Action::OriginalTreeV1(Jump::Close)),
    ];
    let mut x = inner.x;
    for (label, action) in controls {
        let width = crate::editor::display_width(label) as u16;
        let rect = Rect::new(x, inner.bottom() - 1, width, 1);
        if rect.right() > inner.right() {
            continue;
        }
        button(frame, app, rect, label, action, false, theme);
        x = rect.right() + 1;
    }
    if let Some(action) = app.original_tree_viewport_v2.focus_restore.take() {
        if let Some(index) = app.hits.iter().position(|hit| hit.action == action) {
            app.focus = index;
            if let Some(jump) = &mut app.original_tree_jump_v1 {
                jump.focus = focused_action(&action);
            }
        }
    }
    true
}

pub fn event(app: &mut App, event: Event) -> bool {
    let Some(jump) = app.original_tree_jump_v1.as_ref() else {
        return false;
    };
    let action = match event {
        Event::Key(key) if key.kind != KeyEventKind::Release => {
            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                app.dispatch(Action::Quit);
                return true;
            }
            if key.code == KeyCode::Char('p') && key.modifiers.contains(KeyModifiers::CONTROL) {
                app.dispatch(Action::OriginalTreeViewportV2(Vp::Commands));
                return true;
            }
            match key.code {
                KeyCode::Tab | KeyCode::BackTab => {
                    app.focus = (app.focus
                        + if key.code == KeyCode::BackTab {
                            app.hits.len().saturating_sub(1)
                        } else {
                            1
                        })
                        % app.hits.len().max(1);
                    if let Some(jump) = &mut app.original_tree_jump_v1 {
                        jump.focus = app
                            .hits
                            .get(app.focus)
                            .map_or(Focus::List, |hit| focused_action(&hit.action));
                    }
                    app.hits.get(app.focus).and_then(|hit| match &hit.action {
                        Action::OriginalTreeV1(Jump::Activate(ticket)) => {
                            Some(Action::OriginalTreeV1(Jump::Select(ticket.clone())))
                        }
                        _ => None,
                    })
                }
                KeyCode::Enter if jump.focus == Focus::Query => {
                    Some(Action::OriginalTreeV1(Jump::FocusList))
                }
                KeyCode::Enter if jump.focus == Focus::List => {
                    Some(Action::OriginalTreeV1(Jump::Choose))
                }
                KeyCode::Enter => app
                    .hits
                    .get(app.focus)
                    .filter(|hit| {
                        !matches!(
                            &hit.action,
                            Action::OriginalTreeV1(Jump::Activate(_) | Jump::FocusQuery)
                        )
                    })
                    .map(|hit| hit.action.clone())
                    .or(Some(Action::OriginalTreeV1(Jump::Choose))),
                KeyCode::Left | KeyCode::Right if jump.focus == Focus::Detail => {
                    Some(Action::OriginalTreeViewportV2(Vp::DetailField(
                        app.original_tree_viewport_v2
                            .field
                            .next(key.code == KeyCode::Left),
                    )))
                }
                KeyCode::Up
                | KeyCode::Down
                | KeyCode::PageUp
                | KeyCode::PageDown
                | KeyCode::Home
                | KeyCode::End
                    if jump.focus == Focus::Detail =>
                {
                    Some(Action::OriginalTreeViewportV2(Vp::DetailScroll(
                        match key.code {
                            KeyCode::Up => -1,
                            KeyCode::Down => 1,
                            KeyCode::PageUp => -8,
                            KeyCode::PageDown => 8,
                            KeyCode::Home => i32::MIN,
                            _ => i32::MAX,
                        },
                    )))
                }
                KeyCode::PageUp | KeyCode::PageDown if jump.focus == Focus::List => {
                    Some(Action::OriginalTreeV1(Jump::Move(
                        i32::from(app.original_tree_viewport_v2.list_area.height.max(1))
                            * if key.code == KeyCode::PageUp { -1 } else { 1 },
                    )))
                }
                _ => app
                    .original_tree_layout_v1
                    .key(jump, key.code, key.modifiers)
                    .map(Action::OriginalTreeV1),
            }
        }
        Event::Paste(text) => Some(Action::OriginalTreeV1(Jump::Paste(text))),
        Event::Mouse(mouse) => match mouse.kind {
            MouseEventKind::Down(MouseButton::Left)
                if app
                    .original_tree_viewport_v2
                    .query_area
                    .contains((mouse.column, mouse.row).into()) =>
            {
                Some(Action::OriginalTreeV1(Jump::QueryClick(usize::from(
                    mouse.column - app.original_tree_viewport_v2.query_area.x,
                ))))
            }
            MouseEventKind::Down(MouseButton::Left) => app
                .hits
                .iter()
                .enumerate()
                .rev()
                .find(|(_, hit)| hit.rect.contains((mouse.column, mouse.row).into()))
                .map(|(index, hit)| {
                    app.focus = index;
                    if let Some(jump) = &mut app.original_tree_jump_v1 {
                        jump.focus = focused_action(&hit.action);
                    }
                    hit.action.clone()
                }),
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp
                if app
                    .original_tree_viewport_v2
                    .detail_area
                    .contains((mouse.column, mouse.row).into()) =>
            {
                Some(Action::OriginalTreeViewportV2(Vp::DetailScroll(
                    if mouse.kind == MouseEventKind::ScrollDown {
                        1
                    } else {
                        -1
                    },
                )))
            }
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp
                if app
                    .original_tree_viewport_v2
                    .list_area
                    .contains((mouse.column, mouse.row).into()) =>
            {
                Some(Action::OriginalTreeV1(Jump::Move(
                    if mouse.kind == MouseEventKind::ScrollDown {
                        1
                    } else {
                        -1
                    },
                )))
            }
            _ => None,
        },
        _ => None,
    };
    if let Some(action) = action {
        let focus_list = matches!(
            &action,
            Action::OriginalTreeV1(Jump::FocusList | Jump::Move(_) | Jump::First | Jump::Last)
        );
        app.dispatch(action);
        if focus_list {
            if let Some(jump) = app.original_tree_jump_v1.as_ref() {
                if let Some(ticket) = jump.frame().and_then(|frame| frame.ticket(jump.selected)) {
                    if let Some(index) = app.hits.iter().position(|hit| {
                        hit.action == Action::OriginalTreeV1(Jump::Activate(ticket.clone()))
                    }) {
                        app.focus = index;
                    }
                }
            }
        }
    }
    true
}
