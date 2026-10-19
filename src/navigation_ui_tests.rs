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
fn mouse_keyboard_scrollbar_help_arrows_pages_and_drag_match_keyboard_without_work() {
    let mut keyboard = App {
        view: View::Repository,
        ..Default::default()
    };
    let mut mouse = App {
        view: View::Repository,
        ..Default::default()
    };
    for app in [&mut keyboard, &mut mouse] {
        app.dispatch(Action::Help);
        draw(app, 80, 24);
    }
    // Help's existing border, not a column taken away from its wrapped text.
    let arrow = Rect::new(77, mouse.context_help_area.bottom() - 1, 1, 1);
    click(&mut mouse, arrow);
    key(&mut keyboard, KeyCode::Down);
    assert_eq!(mouse.context_help_scroll, 1);
    assert_eq!(mouse.context_help_scroll, keyboard.context_help_scroll);
    assert_eq!(draw(&mut mouse, 80, 24), draw(&mut keyboard, 80, 24));
    key(&mut mouse, KeyCode::Home);
    key(&mut keyboard, KeyCode::Home);
    draw(&mut mouse, 80, 24);
    click(&mut mouse, Rect::new(77, arrow.y - 1, 1, 1));
    key(&mut keyboard, KeyCode::PageDown);
    assert_eq!(mouse.context_help_scroll, keyboard.context_help_scroll);
    assert_eq!(draw(&mut mouse, 80, 24), draw(&mut keyboard, 80, 24));
    key(&mut mouse, KeyCode::Home);
    draw(&mut mouse, 80, 24);
    let thumb_y = mouse.context_help_area.y + 1;
    click(&mut mouse, Rect::new(77, thumb_y, 1, 1));
    mouse.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 0,
        row: 23,
        modifiers: KeyModifiers::NONE,
    }));
    key(&mut keyboard, KeyCode::End);
    assert_eq!(draw(&mut mouse, 80, 24), draw(&mut keyboard, 80, 24));
    assert!(mouse.context_help_scroll > 0);
    for app in [&mouse, &keyboard] {
        assert!(app.github_action.is_none() && app.actions_action.is_none());
        assert!(app.local_command.is_none() && app.form.is_none());
        assert!(!app.refresh_requested && !app.repository_load_requested);
    }
}

#[test]
fn mouse_keyboard_scrollbar_help_resize_close_reopen_and_keyboard_cancel_old_drag() {
    for retire in 0..3 {
        let mut app = App::default();
        app.dispatch(Action::Help);
        draw(&mut app, 80, 24);
        let thumb_y = app.context_help_area.y + 1;
        click(&mut app, Rect::new(77, thumb_y, 1, 1));
        match retire {
            0 => app.handle_event(Event::Resize(120, 40)),
            1 => {
                app.dispatch(Action::Close);
                app.dispatch(Action::Help);
            }
            _ => key(&mut app, KeyCode::Home),
        }
        app.handle_event(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Left),
            column: 77,
            row: 20,
            modifiers: KeyModifiers::NONE,
        }));
        assert_eq!(
            app.context_help_scroll, 0,
            "retired drag cannot scroll new geometry"
        );
        assert_eq!(app.view, View::Inbox);
        assert_eq!(app.overlay, Some(Overlay::Help));
    }
    let mut app = App::default();
    app.dispatch(Action::HelpScroll(10));
    assert_eq!(
        app.context_help_scroll, 0,
        "closed Help ignores a stale scroll action"
    );
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

#[test]
fn v1_navigation_help_wrap_keeps_words_whole_and_splits_only_overlong_words() {
    let rows = help_wrap_v1("Enter or click opens the selected PR quickly", 16);
    assert_eq!(
        rows,
        ["Enter or click", "opens the", "selected PR", "quickly"]
    );
    assert!(rows
        .iter()
        .all(|row| crate::editor::display_width(row) <= 16));
    let long = help_wrap_v1("see abcdefghijklmnopqrstuvwxyz end", 10);
    assert_eq!(long, ["see", "abcdefghij", "klmnopqrst", "uvwxyz end"]);
    assert_eq!(help_wrap_v1("", 10), [""]);
}

#[test]
fn v1_navigation_inbox_queue_summary_counts_and_empty_queue_points_to_populated_queue() {
    use crate::github_workspace::{empty_queue_hint_v1, queue_summary_v1, InboxFilter};
    assert_eq!(
        queue_summary_v1("Needs my review", [3, 2, 2, 0]),
        "Queue: Needs my review • All 3 · Created by me 2 · Assigned to me 2 · Needs my review 0"
    );
    assert_eq!(
        empty_queue_hint_v1(InboxFilter::ReviewRequested, [3, 2, 2, 0]),
        "No pull requests in this queue / scope. All has 3: press a or click All above."
    );
    assert_eq!(
        empty_queue_hint_v1(InboxFilter::All, [0, 0, 0, 0]),
        "No pull requests in this queue / scope. Press r to refresh."
    );
    assert_eq!(
        empty_queue_hint_v1(InboxFilter::Assigned, [0, 0, 0, 0]),
        "No pull requests in this queue / scope. Press r to refresh."
    );
}

#[test]
fn v1_navigation_inbox_f8_before_load_explains_refresh_instead_of_doing_nothing() {
    let mut app = App::default();
    assert_eq!(app.view, View::Inbox);
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::F(8))));
    assert!(
        app.status.contains("Refresh the Inbox (r) first"),
        "{}",
        app.status
    );
    assert!(app.overlay.is_none());
    assert!(app.notifications_v63.is_none());
}

#[test]
fn v1_navigation_inbox_mouse_palette_explains_notifications_before_load() {
    let mut app = App::default();
    let screen = draw(&mut app, 120, 40);
    let lines = screen.lines().collect::<Vec<_>>();
    let row = lines
        .iter()
        .rposition(|line| line.contains(": Commands"))
        .unwrap();
    let column = screen.lines().nth(row).unwrap().find(": Commands").unwrap();
    click(&mut app, Rect::new(column as u16, row as u16, 1, 1));
    assert_eq!(app.overlay, Some(Overlay::Palette), "{}", app.status);
    app.handle_event(Event::Paste("notif".into()));
    let screen = draw(&mut app, 120, 40);
    assert!(
        screen.contains("> Open notifications inbox [why unavailable]"),
        "{screen}"
    );
    let rect = app
        .hits
        .iter()
        .find(|hit| {
            matches!(
                &hit.action,
                Action::PaletteInput(PaletteAction::Activate(_))
            )
        })
        .unwrap()
        .rect;
    click(&mut app, rect);
    assert_eq!(app.overlay, Some(Overlay::Notice));
    assert!(
        app.notice.contains("Refresh the Inbox (r) first"),
        "{}",
        app.notice
    );
    assert!(app.notifications_v63.is_none());
}

#[test]
fn design_help_v97_key_rows_align_one_column_accent_key_and_ellipsis_label() {
    let theme = crate::theme::Theme::new(
        crate::config::ThemeName::CatppuccinMocha,
        crate::theme::ColorMode::TrueColor,
    );
    let rows = [("r", "Refresh"), ("Enter", "Open"), ("F10", "Reviews")]
        .map(|(key, label)| help_key_row_v97(theme, key, label, false, 5, 40));
    let text = |line: &Line| {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    };
    assert_eq!(text(&rows[0]), "  r      Refresh");
    assert_eq!(text(&rows[1]), "  Enter  Open");
    assert_eq!(text(&rows[2]), "  F10    Reviews");
    // The key is the accent span; the label starts in the same column on every row.
    for row in &rows {
        assert_eq!(
            row.spans[1].style,
            theme
                .style(SemanticToken::Accent)
                .add_modifier(Modifier::BOLD)
        );
        assert_eq!(text(row).find(row.spans[3].content.as_ref()), Some(9));
    }
    let disabled = help_key_row_v97(theme, "x", "Clear cache · unavailable", true, 5, 40);
    assert_eq!(disabled.spans[3].style, theme.style(SemanticToken::Muted));
    // A long label ends in an ellipsis within the row.
    let cut = help_key_row_v97(theme, "p", &"PR actions ".repeat(8), false, 5, 30);
    assert_eq!(crate::editor::display_width(&text(&cut)), 30);
    assert!(text(&cut).ends_with('…'));
}

#[test]
fn design_help_v97_help_overlay_paints_view_keys_before_the_prose_at_80_and_140() {
    for (width, height) in [(140, 42), (80, 24)] {
        let mut app = App::default();
        let footer = crate::footer::main_controls(&app);
        key(&mut app, KeyCode::Char('?'));
        assert_eq!(app.overlay, Some(Overlay::Help));
        let screen = draw(&mut app, width, height);
        let lines = screen.lines().collect::<Vec<_>>();
        let heading = lines
            .iter()
            .position(|line| line.contains("Keys • Global"))
            .unwrap_or_else(|| panic!("{screen}"));
        // Every footer key of this frame is a Help row with its label in one shared column.
        let mut columns = std::collections::BTreeSet::new();
        for control in footer.iter().filter(|control| !control.key.is_empty()) {
            let row = lines[heading..]
                .iter()
                .find(|line| {
                    line.split_whitespace()
                        .collect::<Vec<_>>()
                        .windows(2)
                        .any(|pair| {
                            pair[0].ends_with(control.key.as_str()) && pair[1] == control.label
                        })
                })
                .unwrap_or_else(|| panic!("{} {}\n{screen}", control.key, control.label));
            columns.insert(row.find(control.label.as_str()).unwrap());
        }
        assert_eq!(columns.len(), 1, "{screen}");
        assert!(
            !screen.contains("Personal queues and PRs") || {
                let prose = lines
                    .iter()
                    .position(|line| line.contains("Personal queues and PRs"))
                    .unwrap();
                prose > heading
            }
        );
    }
}
