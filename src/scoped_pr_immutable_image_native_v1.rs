//! Minimal consumer of the SAME page-only runtime publisher; no second backend.
use super::*;
use crate::app::{Action, App, Overlay, View};
use crossterm::event::{Event, KeyEventKind};

/// Every event and draw checks the actual original owner BEFORE private effects.
/// A receipt or retained image is never renewed access for a different source.
pub fn retire_invalid(app: &mut App) -> bool {
    if app
        .pr_file_images
        .as_ref()
        .is_some_and(|view| !view.owns(app))
    {
        app.pr_file_images = None;
        if app.overlay == Some(Overlay::PrFileImages) {
            app.overlay = None;
        }
        app.hits.clear();
        return true;
    }
    false
}
pub fn action(app: &mut App, action: ui::PrImageUiActionV1) {
    if retire_invalid(app) {
        return;
    }
    if app.scoped_inline_owner_v1.is_some() {
        match action {
            ui::PrImageUiActionV1::Close | ui::PrImageUiActionV1::Back => {
                app.scoped_inline_cancel_v1 = true;
            }
            ui::PrImageUiActionV1::Quit => app.dispatch(Action::Quit),
            _ => {
                app.status =
                    "Original inline read pending • Esc Close cancels original operation".into();
            }
        }
        return;
    }
    if action == ui::PrImageUiActionV1::Comment {
        super::inline_v1::native_v1::open(app);
        return;
    }
    let Some(view) = app.pr_file_images.as_mut() else {
        return;
    };
    if let Some(action) = view.apply(action) {
        match action {
            ui::PrImageUiActionV1::Quit => app.dispatch(Action::Quit),
            ui::PrImageUiActionV1::Back => {
                app.pr_file_images = None;
                app.overlay = None;
                app.dispatch(Action::Github(crate::github_workspace::GithubAction::Back));
            }
            ui::PrImageUiActionV1::Close => {
                app.pr_file_images = None;
                app.overlay = None;
                app.hits.clear();
            }
            _ => {}
        }
    }
}
/// Captures ALL overlay events, including unrecognized keys, paste and resize.
pub fn event(app: &mut App, event: Event) {
    if retire_invalid(app) {
        return;
    }
    let Some(view) = app.pr_file_images.as_mut() else {
        app.overlay = None;
        return;
    };
    let action = match event {
        Event::Key(key) if key.kind != KeyEventKind::Release => ui::key(view, key),
        Event::Mouse(mouse) => ui::mouse(view, mouse),
        Event::Resize(_, _) => {
            view.clear_layout();
            None
        }
        _ => None,
    };
    if let Some(input) = action {
        self::action(app, input);
    }
}
pub fn draw(frame: &mut ratatui::Frame, app: &mut App, area: ratatui::layout::Rect) {
    if retire_invalid(app) {
        return;
    }
    let theme = app.theme();
    if let Some(view) = app.pr_file_images.as_mut() {
        frame.render_widget(ratatui::widgets::Clear, area);
        ui::draw(frame, area, view, theme);
        if app.scoped_inline_owner_v1.is_some() && area.height >= 3 {
            frame.render_widget(ratatui::widgets::Paragraph::new("Inline read pending • [Esc Close] cancels original operation • draft preserved").style(theme.style(crate::theme::SemanticToken::Pending)),ratatui::layout::Rect::new(area.x+1,area.bottom()-3,area.width.saturating_sub(2),1));
        }
    }
}
/// Called only after Runtime's existing original owner + operation generation
/// checks, before any success/status/identity/overlay effect. A later Form or
/// other workspace remains selected: completion cannot steal its overlay.
pub fn install(
    app: &mut App,
    result: PrImmutableImageResultV1,
) -> Result<bool, ObservedPrTabFailureV1> {
    let (viewer, limited) = install_images(app, result)?;
    if !limited && app.view == View::Inbox && app.overlay.is_none() {
        app.pr_file_images = viewer;
        app.overlay = Some(Overlay::PrFileImages);
        app.hits.clear();
    }
    Ok(limited)
}
