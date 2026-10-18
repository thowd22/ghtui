//! Pure owned overlay. All visible keys/clicks share PrCheckChoiceUiV1; no
//! form/draft/SQL/transport/JSON/sort/whole-model cloning during input/draw.
use super::*;
use crossterm::event::{Event, KeyCode, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

pub fn event(app: &mut App, event: Event) -> bool {
    if app.overlay != Some(crate::app::Overlay::PrCheckChoices) {
        return false;
    }
    if app
        .pr_check_choice_picker
        .as_ref()
        .is_none_or(|picker| !picker.owns(app))
    {
        retire(app);
        return true;
    }
    let action = match event {
        Event::Resize(_, _) => {
            if let Some(picker) = app.pr_check_choice_picker.as_mut() {
                picker.hits.clear();
            }
            None
        }
        Event::Key(key) if key.kind == crossterm::event::KeyEventKind::Press => {
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                Some(PrCheckChoiceUiV1::Quit)
            } else {
                match key.code {
                    KeyCode::Char('q') => Some(PrCheckChoiceUiV1::Quit),
                    KeyCode::Esc => Some(PrCheckChoiceUiV1::Close),
                    KeyCode::Up | KeyCode::Char('k') => Some(PrCheckChoiceUiV1::Scroll(-1)),
                    KeyCode::Down | KeyCode::Char('j') => Some(PrCheckChoiceUiV1::Scroll(1)),
                    KeyCode::PageUp => Some(PrCheckChoiceUiV1::Scroll(-10)),
                    KeyCode::PageDown => Some(PrCheckChoiceUiV1::Scroll(10)),
                    KeyCode::Tab | KeyCode::BackTab => {
                        let picker = app.pr_check_choice_picker.as_mut().unwrap();
                        picker.focus =
                            (picker.focus + if key.code == KeyCode::Tab { 1 } else { 2 }) % 3;
                        None
                    }
                    KeyCode::Enter => {
                        Some(match app.pr_check_choice_picker.as_ref().unwrap().focus {
                            0 => PrCheckChoiceUiV1::Confirm,
                            1 => PrCheckChoiceUiV1::Close,
                            _ => PrCheckChoiceUiV1::Quit,
                        })
                    }
                    _ => None,
                }
            }
        }
        Event::Mouse(mouse) => match mouse.kind {
            MouseEventKind::ScrollDown => Some(PrCheckChoiceUiV1::Scroll(3)),
            MouseEventKind::ScrollUp => Some(PrCheckChoiceUiV1::Scroll(-3)),
            MouseEventKind::Down(MouseButton::Left) => app
                .pr_check_choice_picker
                .as_ref()
                .unwrap()
                .hits
                .iter()
                .find(|(rect, _)| rect.contains((mouse.column, mouse.row).into()))
                .map(|(_, action)| *action),
            _ => None,
        },
        _ => None,
    };
    if let Some(action) = action {
        dispatch(app, action);
    }
    true // Capture every event, including paste/unrecognized keys. No leaks.
}
pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    if app.overlay != Some(crate::app::Overlay::PrCheckChoices) {
        return;
    }
    let theme = app.theme();
    let Some(picker) = app
        .pr_check_choice_picker
        .as_ref()
        .filter(|picker| picker.owns(app))
    else {
        return;
    };
    let prepared = picker.prepared.clone();
    let header = &prepared.header;
    let avatars = &app.avatar_states;
    let rect = Rect::new(area.x, area.y, area.width, area.height);
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .title(" Observed PR check candidates • read only ")
            .border_style(theme.style(crate::theme::SemanticToken::Focus)),
        rect,
    );
    let inside = rect.inner(ratatui::layout::Margin::new(1, 1));
    if inside.width == 0 || inside.height == 0 {
        return;
    }
    let picker = app.pr_check_choice_picker.as_mut().unwrap();
    picker.hits.clear();
    frame.render_widget(
        Paragraph::new(header.as_str())
            .wrap(Wrap { trim: false })
            .style(theme.style(crate::theme::SemanticToken::Pending)),
        Rect::new(inside.x, inside.y, inside.width, inside.height.min(3)),
    );
    let y = inside.y.saturating_add(3);
    let height = inside.bottom().saturating_sub(y.saturating_add(2)) as usize;
    if picker.selected < picker.top {
        picker.top = picker.selected;
    } else if height > 0 && picker.selected >= picker.top.saturating_add(height) {
        picker.top = picker.selected + 1 - height;
    }
    for (offset, (index, row)) in picker
        .prepared
        .rows
        .iter()
        .enumerate()
        .skip(picker.top)
        .take(height)
        .enumerate()
    {
        let rect = Rect::new(inside.x, y + offset as u16, inside.width, 1);
        let role = if matches!(row.item, Item::Run(_)) {
            crate::theme::SemanticToken::Actor
        } else {
            crate::theme::SemanticToken::Normal
        };
        let mut text_rect = rect;
        if let Some(identity) = row.actor.as_ref() {
            let width = 4.min(rect.width);
            let image = match avatars.get(identity) {
                Some(crate::identity::AvatarState::Ready(image)) => Some(image.as_ref()),
                _ => None,
            };
            frame.render_widget(
                crate::identity::AvatarWidget {
                    identity,
                    image,
                    theme,
                },
                Rect::new(rect.x, rect.y, width, 1),
            );
            text_rect = Rect::new(rect.x + width, rect.y, rect.width - width, 1);
        }
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw(if picker.selected == index { "> " } else { "  " }),
                Span::raw(row.label.as_str()),
            ]))
            .style(if picker.selected == index {
                theme.selected(role)
            } else {
                theme.style(role)
            }),
            text_rect,
        );
        picker.hits.push((rect, PrCheckChoiceUiV1::Select(index)));
    }
    let mut x = inside.x;
    let y = inside.bottom().saturating_sub(1);
    for (index, label, action) in [
        (0, "[Enter Choose]", PrCheckChoiceUiV1::Confirm),
        (1, "[Esc Close]", PrCheckChoiceUiV1::Close),
        (2, "[q Quit]", PrCheckChoiceUiV1::Quit),
    ] {
        let width = (label.len() as u16).min(inside.right().saturating_sub(x));
        if width == 0 {
            break;
        }
        let rect = Rect::new(x, y, width, 1);
        let mut style = theme
            .style(crate::theme::SemanticToken::Focus)
            .remove_modifier(Modifier::UNDERLINED);
        if picker.focus == index {
            style = style.add_modifier(Modifier::UNDERLINED);
        }
        frame.render_widget(Paragraph::new(label).style(style), rect);
        picker.hits.push((rect, action));
        x = x.saturating_add(width + 1);
    }
}
/// Same launch action in Inbox/Actions. Prepared row launch never restores a
/// private draft. Return the remaining body area, avoiding control overlap.
pub fn draw_launch(frame: &mut Frame, app: &mut App, area: Rect) -> Rect {
    if !has_candidates(app) || pending(app) || area.width == 0 || area.height == 0 {
        return area;
    }
    // The ordinary focused-view renderer may have painted an old placeholder
    // underneath us. Retire the entire row, not only the short clickable label.
    frame.render_widget(Clear, Rect::new(area.x, area.y, area.width, 1));
    let rect = Rect::new(area.x, area.y, area.width.min(14), 1);
    let mut style = app
        .theme()
        .style(crate::theme::SemanticToken::Focus)
        .remove_modifier(Modifier::UNDERLINED);
    let content_focused = match app.view {
        View::Inbox => app.github_content_focused,
        View::Actions => app.actions_content_focused,
        _ => true,
    };
    if !content_focused && app.focus == app.hits.len() {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    frame.render_widget(Paragraph::new("[C Candidates]").style(style), rect);
    app.hits.push(crate::app::HitRegion {
        rect,
        action: Action::PrCheckChoice(PrCheckChoiceUiV1::Open),
    });
    Rect::new(area.x, area.y + 1, area.width, area.height - 1)
}
