//! Native prepared command discovery and public contextual Help. All provider,
//! Git, SQL and filesystem work remains in existing guarded workers.
use crate::{
    app::{Action, App, HitRegion, Overlay},
    command_palette::{CommandPalette, PaletteAction, PaletteOutcome},
    context_help::{ContextHelp, HelpLineKind},
    navigation_catalogue::PreparedCatalogue,
    theme::SemanticToken,
};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    layout::{Margin, Rect},
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

impl App {
    /// Retire owned actions, not only geometry. This epoch only refuses stale
    /// discovery; it is never fresh repository/identity/mutation authority.
    pub fn retire_command_context(&mut self) {
        self.help_scrollbar.clear();
        self.command_palette = None;
        self.command_palette_context = None;
        if self.overlay == Some(Overlay::Palette) {
            self.overlay = None;
            self.hits.clear();
        }
        match self.command_source_epoch.checked_add(1) {
            Some(epoch) => self.command_source_epoch = epoch,
            None => self.command_context_exhausted = true,
        }
    }
}
pub fn open_palette(app: &mut App) {
    app.retire_command_context();
    app.clear_capability_inspector();
    app.context_help = None;
    app.context_help_area = Rect::default();
    app.help_scrollbar.clear();
    app.hits.clear();
    app.palette_index = 0;
    if app.command_context_exhausted {
        app.dispatch(Action::Explain("Command context exhausted; restart ghtui before preparing new contextual actions. No old target was reused.".into()));
        return;
    }
    let prepared =
        PreparedCatalogue::prepare(app, app.command_source_epoch).and_then(|catalogue| {
            CommandPalette::new(catalogue.entries).and_then(|mut palette| {
                for (id, key) in crate::navigation_catalogue::shortcut_hints(app) {
                    palette.set_shortcut(&id, &key);
                }
                if app.original_tree_viewport_v2.width.saturating_sub(6) >= 20 {
                    palette.prepare_width_v2(
                        app.original_tree_viewport_v2
                            .width
                            .saturating_sub(6)
                            .min(100),
                    )?;
                }
                Ok((catalogue.context, palette))
            })
        });
    match prepared {
        Ok((context, palette)) => {
            app.command_palette_context = Some(context);
            app.command_palette = Some(palette);
            app.overlay = Some(Overlay::Palette);
        }
        Err(error) => app.dispatch(Action::Explain(error)),
    }
}
pub fn palette_input(app: &mut App, input: PaletteAction) {
    if app.overlay != Some(Overlay::Palette) {
        return;
    }
    if app.command_context_exhausted
        || !app
            .command_palette_context
            .is_some_and(|context| context.is_current(app.view, app.command_source_epoch))
    {
        app.retire_command_context();
        app.dispatch(Action::Explain(
            "Command source changed; reopen Commands and choose the current exact target.".into(),
        ));
        return;
    }
    let result = app
        .command_palette
        .as_mut()
        .map_or(PaletteOutcome::Closed, |palette| palette.dispatch(input));
    if let Some(palette) = &mut app.command_palette {
        if app.original_tree_viewport_v2.width.saturating_sub(6) >= 20 {
            if let Err(reason) = palette.prepare_width_v2(
                app.original_tree_viewport_v2
                    .width
                    .saturating_sub(6)
                    .min(100),
            ) {
                app.status = reason;
            }
        }
    }
    match result {
        PaletteOutcome::Chosen { action, .. } => {
            // Many existing contextual actions do not close their caller menu.
            // Consume/discard this widget before ordinary guarded dispatch.
            app.command_palette = None;
            app.command_palette_context = None;
            app.overlay = None;
            app.hits.clear();
            app.dispatch(action);
        }
        PaletteOutcome::Closed => {
            app.retire_command_context();
            app.hits.clear();
        }
        PaletteOutcome::Changed | PaletteOutcome::Rejected(_) | PaletteOutcome::Ignored => {
            app.hits.clear()
        }
    }
}
pub fn open_help(app: &mut App) {
    app.context_help_scroll = 0;
    app.context_help_area = Rect::default();
    app.help_scrollbar.clear();
    app.hits.clear();
    match ContextHelp::prepare(app.view, &app.bindings) {
        Ok(help) => {
            app.context_help = Some(help);
            app.overlay = Some(Overlay::Help);
            app.focus = 0;
        }
        Err(error) => app.dispatch(Action::Explain(error)),
    }
}
pub fn event(app: &mut App, event: Event) {
    if let Event::Key(key) = &event {
        if key.kind != KeyEventKind::Release
            && key.modifiers.contains(KeyModifiers::CONTROL)
            && key.code == KeyCode::Char('c')
        {
            app.dispatch(Action::Quit);
            return;
        }
    }
    if app.overlay == Some(Overlay::Palette) {
        let action = match event {
            Event::Key(key) if key.kind != KeyEventKind::Release => app
                .command_palette
                .as_ref()
                .and_then(|palette| palette.key(key.code, key.modifiers)),
            Event::Paste(text) => Some(PaletteAction::Paste(text)),
            Event::Mouse(mouse) => app
                .command_palette
                .as_ref()
                .and_then(|palette| palette.mouse(mouse.kind, mouse.column, mouse.row)),
            Event::Resize(_, _) => {
                app.hits.clear();
                if let Some(palette) = &mut app.command_palette {
                    palette.clear_layout();
                    if app.original_tree_viewport_v2.width.saturating_sub(6) >= 20 {
                        if let Err(reason) = palette.prepare_width_v2(
                            app.original_tree_viewport_v2
                                .width
                                .saturating_sub(6)
                                .min(100),
                        ) {
                            app.status = reason;
                        }
                    }
                }
                None
            }
            _ => None,
        };
        if let Some(action) = action {
            app.dispatch(Action::PaletteInput(action));
        }
        return;
    }
    if app.overlay != Some(Overlay::Help) {
        return;
    }
    if matches!(event, Event::Key(_) | Event::Paste(_)) {
        app.help_scrollbar.cancel_drag();
    }
    let action = match event {
        Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
            KeyCode::Esc | KeyCode::Char('?') => Some(Action::Close),
            KeyCode::Up | KeyCode::Char('k') => Some(Action::HelpScroll(-1)),
            KeyCode::Down | KeyCode::Char('j') => Some(Action::HelpScroll(1)),
            KeyCode::PageUp => Some(Action::HelpScroll(-(app.context_help_rows as i32))),
            KeyCode::PageDown => Some(Action::HelpScroll(app.context_help_rows as i32)),
            KeyCode::Home => Some(Action::HelpScroll(-4096)),
            KeyCode::End => Some(Action::HelpScroll(4096)),
            KeyCode::Tab => {
                app.focus = (app.focus + 1) % app.hits.len().max(1);
                None
            }
            KeyCode::BackTab => {
                app.focus = (app.focus + app.hits.len().saturating_sub(1)) % app.hits.len().max(1);
                None
            }
            KeyCode::Enter => app.hits.get(app.focus).map(|hit| hit.action.clone()),
            _ => None,
        },
        Event::Mouse(mouse) => {
            if let Some(request) = app
                .help_scrollbar
                .mouse(mouse.kind, mouse.column, mouse.row)
            {
                let delta = match request {
                    crate::scrollbar::ScrollRequest::By(delta) => delta,
                    crate::scrollbar::ScrollRequest::To(target) => (target as i128
                        - app.context_help_scroll as i128)
                        .clamp(i128::from(i32::MIN), i128::from(i32::MAX))
                        as i32,
                };
                Some(Action::HelpScroll(delta))
            } else {
                match mouse.kind {
                    MouseEventKind::Down(MouseButton::Left) => app
                        .hits
                        .iter()
                        .enumerate()
                        .rev()
                        .find(|(_, hit)| hit.rect.contains((mouse.column, mouse.row).into()))
                        .map(|(index, hit)| {
                            app.focus = index;
                            hit.action.clone()
                        }),
                    MouseEventKind::ScrollDown | MouseEventKind::ScrollUp
                        if app
                            .context_help_area
                            .contains((mouse.column, mouse.row).into()) =>
                    {
                        Some(Action::HelpScroll(
                            if mouse.kind == MouseEventKind::ScrollDown {
                                3
                            } else {
                                -3
                            },
                        ))
                    }
                    _ => None,
                }
            }
        }
        Event::Resize(_, _) => {
            app.hits.clear();
            app.context_help_area = Rect::default();
            app.help_scrollbar.clear();
            None
        }
        _ => None,
    };
    if let Some(action) = action {
        app.dispatch(action);
    }
}
pub fn draw_help(frame: &mut Frame, app: &mut App, area: Rect) {
    let theme = app.theme();
    let width = area.width.saturating_sub(4).min(100);
    let height = area.height.saturating_sub(4).min(30);
    let rect = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .title("Help • keyboard and mouse")
            .style(
                theme
                    .style(SemanticToken::Focus)
                    .remove_modifier(ratatui::style::Modifier::UNDERLINED),
            ),
        rect,
    );
    let inner = rect.inner(Margin::new(2, 1));
    let body = Rect::new(
        inner.x,
        inner.y,
        inner.width,
        inner.height.saturating_sub(2),
    );
    app.context_help_area = body;
    app.context_help_rows = usize::from(body.height).max(1);
    let mut lines = Vec::<Line<'static>>::new();
    if let Some(help) = &app.context_help {
        for row in help.lines() {
            let role = match row.kind {
                HelpLineKind::Heading => SemanticToken::Focus,
                HelpLineKind::Shortcut => SemanticToken::Reference,
                HelpLineKind::Guidance => SemanticToken::Normal,
                HelpLineKind::Limitation => SemanticToken::Muted,
            };
            lines.extend(
                help_wrap_v1(&row.text, body.width)
                    .into_iter()
                    .map(|line| Line::from(line).style(theme.style(role))),
            );
        }
    }
    app.context_help_scroll = app
        .context_help_scroll
        .min(lines.len().saturating_sub(app.context_help_rows));
    app.help_scrollbar.draw(
        frame,
        Rect::new(rect.right().saturating_sub(1), body.y, 1, body.height),
        lines.len(),
        body.height,
        app.context_help_scroll,
        theme,
    );
    frame.render_widget(
        Paragraph::new(lines).scroll((app.context_help_scroll as u16, 0)),
        body,
    );
    let narrow = inner.width < 58;
    for (offset, label, action) in [
        (
            0,
            "[PgUp Previous]",
            Action::HelpScroll(-(app.context_help_rows as i32)),
        ),
        (
            20,
            "[PgDn More]",
            Action::HelpScroll(app.context_help_rows as i32),
        ),
        (40, "[Esc Close]", Action::Close),
    ] {
        let close = action == Action::Close;
        let (offset, y) = if narrow && close {
            (0, inner.bottom().saturating_sub(1))
        } else if narrow {
            (offset, inner.bottom().saturating_sub(2))
        } else {
            (offset, inner.bottom().saturating_sub(1))
        };
        let button = Rect::new(
            inner.x + offset.min(inner.width),
            y,
            crate::editor::display_width(label).min(usize::from(inner.width.saturating_sub(offset)))
                as u16,
            1,
        );
        if !button.is_empty() {
            frame.render_widget(
                Paragraph::new(label).style(theme.style(if app.focus == app.hits.len() {
                    SemanticToken::Focus
                } else {
                    SemanticToken::Normal
                })),
                button,
            );
            app.hits.push(HitRegion {
                rect: button,
                action,
            });
        }
    }
}

#[cfg(test)]
#[path = "navigation_ui_tests.rs"]
mod tests;

/// Word-wrapped help rows. Words wider than the row fall back to the shared
/// grapheme wrapping so nothing is clipped.
pub(crate) fn help_wrap_v1(text: &str, width: u16) -> Vec<String> {
    use crate::editor::display_width;
    let limit = usize::from(width.max(1));
    let mut rows = Vec::new();
    let mut row = String::new();
    for word in text.split(' ').filter(|word| !word.is_empty()) {
        let needed = display_width(word) + usize::from(!row.is_empty());
        if !row.is_empty() && display_width(&row) + needed > limit {
            rows.push(std::mem::take(&mut row));
        }
        if row.is_empty() && display_width(word) > limit {
            let mut pieces = crate::ui::notice_lines(word, width)
                .into_iter()
                .map(|line| line.to_string())
                .collect::<Vec<_>>();
            row = pieces.pop().unwrap_or_default();
            rows.extend(pieces);
            continue;
        }
        if !row.is_empty() {
            row.push(' ');
        }
        row.push_str(word);
    }
    if !row.is_empty() || rows.is_empty() {
        rows.push(row);
    }
    rows
}
