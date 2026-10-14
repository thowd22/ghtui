use super::*;
use crate::app::View;
use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::{backend::TestBackend, Terminal};

fn draw(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| crate::ui::draw(frame, app)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn key(app: &mut App, code: KeyCode) {
    app.handle_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
}
fn click(app: &mut App, rect: Rect) {
    app.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: rect.x,
        row: rect.y,
        modifiers: KeyModifiers::NONE,
    }));
}

#[test]
fn navigation_ui_palette_key_and_mouse_choose_same_original_action_after_closing_widget() {
    for mouse in [false, true] {
        let mut app = App::default();
        app.dispatch(Action::Palette);
        app.handle_event(Event::Paste("Open Repository".into()));
        let screen = draw(&mut app, 80, 24);
        assert!(screen.contains("Commands · current context"));
        assert!(screen.contains("> Open Repository"));
        if mouse {
            let rect = app
                .hits
                .iter()
                .find(|hit| {
                    matches!(
                        &hit.action,
                        Action::PaletteInput(PaletteAction::Activate(1))
                    )
                })
                .unwrap()
                .rect;
            click(&mut app, rect);
        } else {
            key(&mut app, KeyCode::Enter);
        }
        assert_eq!(app.view, View::Repository);
        assert!(app.repository_load_requested);
        assert_eq!(app.overlay, None);
        assert!(app.command_palette.is_none() && app.command_palette_context.is_none());
        assert!(app.hits.is_empty());
        assert!(app.local_command.is_none() && app.pr_action.is_none() && app.form.is_none());
    }
}

#[test]
fn navigation_ui_palette_disabled_context_keeps_explanation_not_missing_action_or_form() {
    let mut app = App {
        view: View::Repository,
        ..Default::default()
    };
    app.dispatch(Action::Palette);
    app.handle_event(Event::Paste("Stage selected lines".into()));
    let screen = draw(&mut app, 120, 40);
    assert!(screen.contains("Stage selected lines"));
    assert!(screen.contains("explanation"));
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.overlay, Some(Overlay::Notice));
    assert!(app.notice.contains("exact local Git repository"));
    assert!(app.command_palette.is_none());
    assert!(app.local_command.is_none() && app.form.is_none());
}

#[test]
fn navigation_ui_palette_resize_clears_old_mouse_geometry_before_any_repaint() {
    let mut app = App::default();
    app.dispatch(Action::Palette);
    draw(&mut app, 120, 40);
    let rect = app
        .hits
        .iter()
        .find(|hit| {
            matches!(
                &hit.action,
                Action::PaletteInput(PaletteAction::Activate(1))
            )
        })
        .unwrap()
        .rect;
    app.handle_event(Event::Resize(80, 24));
    assert!(app.hits.is_empty());
    click(&mut app, rect);
    assert_eq!(app.view, View::Inbox);
    assert_eq!(app.overlay, Some(Overlay::Palette));
    assert!(!app.repository_load_requested);
    assert!(draw(&mut app, 80, 24).contains("Commands · current context"));
}

#[test]
fn navigation_ui_changed_epoch_or_view_refuses_owned_palette_action_without_rebinding() {
    for change_view in [false, true] {
        let mut app = App::default();
        app.dispatch(Action::Palette);
        if change_view {
            app.view = View::Search;
        } else {
            app.command_source_epoch += 1;
        }
        app.dispatch(Action::PaletteInput(PaletteAction::Activate(1)));
        assert_eq!(app.overlay, Some(Overlay::Notice));
        assert!(app.notice.contains("source changed"));
        assert!(!app.repository_load_requested);
        assert!(app.command_palette.is_none());
    }
}

#[test]
fn navigation_ui_retirement_drops_old_payloads_and_exhaustion_never_reuses_context() {
    let mut app = App::default();
    app.dispatch(Action::Palette);
    let old = app.command_palette_context.unwrap();
    app.retire_command_context();
    assert_eq!(app.overlay, None);
    assert!(app.command_palette.is_none());
    app.dispatch(Action::PaletteInput(PaletteAction::Activate(1)));
    assert_eq!(app.view, View::Inbox);
    assert!(!old.is_current(app.view, app.command_source_epoch));
    app.command_source_epoch = u64::MAX;
    app.dispatch(Action::Palette);
    assert!(app.command_context_exhausted);
    assert_eq!(app.command_source_epoch, u64::MAX);
    assert_eq!(app.overlay, Some(Overlay::Notice));
    assert!(app.notice.contains("restart ghtui"));
    assert!(app.command_palette.is_none());
}

#[test]
fn navigation_ui_palette_captures_background_keys_mouse_and_oversized_atomic_paste() {
    let mut app = App::default();
    draw(&mut app, 80, 24);
    let background = app
        .hits
        .iter()
        .find(|hit| hit.action == Action::View(View::Search))
        .unwrap()
        .rect;
    app.dispatch(Action::Palette);
    draw(&mut app, 80, 24);
    click(&mut app, background);
    key(&mut app, KeyCode::Char('3'));
    assert_eq!(app.view, View::Inbox);
    assert_eq!(app.command_palette.as_ref().unwrap().query(), "3");
    app.handle_event(Event::Paste("界".repeat(512)));
    assert_eq!(app.command_palette.as_ref().unwrap().query(), "3");
    assert!(
        app.github_action.is_none() && app.actions_action.is_none() && app.search_action.is_none()
    );
}

#[test]
fn navigation_ui_help_all_views_scroll_close_and_ignore_hidden_view_controls() {
    for view in View::ALL {
        let mut app = App {
            view,
            no_color: true,
            ..Default::default()
        };
        draw(&mut app, 80, 24);
        let hidden = app
            .hits
            .iter()
            .find(|hit| hit.action == Action::View(View::Search))
            .unwrap()
            .rect;
        app.dispatch(Action::Help);
        let first = draw(&mut app, 80, 24);
        assert!(first.contains(&format!("{} • help for this view", view.label())));
        click(&mut app, hidden);
        key(&mut app, KeyCode::Char('3'));
        assert_eq!(app.view, view);
        key(&mut app, KeyCode::End);
        let last = draw(&mut app, 80, 24);
        assert!(last.contains("authentication") && last.contains("gh."));
        assert!(app.context_help_scroll > 0);
        key(&mut app, KeyCode::Home);
        assert!(draw(&mut app, 120, 40).contains(&format!("{} • help for this view", view.label())));
        let close = app
            .hits
            .iter()
            .find(|hit| hit.action == Action::Close)
            .unwrap()
            .rect;
        click(&mut app, close);
        assert_eq!(app.overlay, None);
        assert!(app.context_help.is_none());
    }
}

#[test]
fn navigation_ui_help_keyboard_mouse_page_scroll_and_tab_enter_share_actions() {
    let mut key_app = App {
        view: View::Repository,
        ..Default::default()
    };
    let mut mouse_app = App {
        view: View::Repository,
        ..Default::default()
    };
    for app in [&mut key_app, &mut mouse_app] {
        app.dispatch(Action::Help);
        draw(app, 80, 24);
    }
    key(&mut key_app, KeyCode::PageDown);
    let next = mouse_app
        .hits
        .iter()
        .find(|hit| matches!(hit.action,Action::HelpScroll(delta) if delta>0))
        .unwrap()
        .rect;
    click(&mut mouse_app, next);
    assert_eq!(key_app.context_help_scroll, mouse_app.context_help_scroll);
    assert!(key_app.context_help_scroll > 0);
    draw(&mut key_app, 80, 24);
    key(&mut key_app, KeyCode::Tab);
    key(&mut key_app, KeyCode::Tab);
    key(&mut key_app, KeyCode::Enter);
    assert_eq!(key_app.overlay, None);
    app_close(&mut mouse_app);
    assert_eq!(mouse_app.overlay, key_app.overlay);
}
fn app_close(app: &mut App) {
    key(app, KeyCode::Esc);
}

#[test]
fn navigation_ui_emergency_quit_is_retained_for_both_new_overlays() {
    for action in [Action::Help, Action::Palette] {
        let mut app = App::default();
        app.dispatch(action);
        app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
        )));
        assert!(app.quit);
        assert!(app.local_command.is_none() && app.pr_action.is_none());
    }
}
