use super::*;
use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::{backend::TestBackend, Terminal};

fn draw(app: &mut App, width: u16, height: u16) {
    Terminal::new(TestBackend::new(width, height))
        .unwrap()
        .draw(|frame| crate::ui::draw(frame, app))
        .unwrap();
}

fn wheel(app: &mut App, column: u16, row: u16, down: bool) {
    app.handle_event(Event::Mouse(MouseEvent {
        kind: if down {
            MouseEventKind::ScrollDown
        } else {
            MouseEventKind::ScrollUp
        },
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }));
}

#[test]
fn mouse_keyboard_hover_routing_main_footer_header_and_body_use_current_pane_only() {
    for (width, height) in [(80, 24), (120, 40)] {
        for view in [View::Inbox, View::Repository] {
            let mut mouse = App {
                view,
                ..Default::default()
            };
            mouse.content[view.index()] = (0..80).map(|n| format!("row {n:02}")).collect();
            draw(&mut mouse, width, height);
            wheel(&mut mouse, 5, height - 1, true);
            assert_eq!(mouse.scroll, 0, "footer wheel must not scroll {view:?}");
            // The merged header (one or two rows) ends where the pane's rounded
            // top border begins; every header row is off-pane.
            let header = crate::ui_chrome::header_rows(&mouse, width);
            assert!(header >= 1);
            for row in 0..header {
                wheel(&mut mouse, 5, row, true);
                assert_eq!(mouse.scroll, 0, "header row {row} is not the content pane");
            }
            wheel(&mut mouse, width, 10, true);
            assert_eq!(mouse.scroll, 0, "outside the last painted column");
            wheel(&mut mouse, 5, 5, true);
            assert_eq!(mouse.scroll, 3);
            let mut keyboard = App {
                view,
                ..Default::default()
            };
            keyboard.content[view.index()] = mouse.content[view.index()].clone();
            draw(&mut keyboard, width, height);
            for _ in 0..3 {
                keyboard.handle_event(Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)));
            }
            assert_eq!(mouse.scroll, keyboard.scroll);
            mouse.scroll = 0;
            // The breadcrumb title is painted in the pane's own top border row.
            wheel(&mut mouse, 5, header, true);
            assert_eq!(
                mouse.scroll, 3,
                "the pane's own painted title belongs to that pane"
            );
            assert!(mouse.github_action.is_none() && mouse.local_command.is_none());
            assert!(mouse.actions_action.is_none() && !mouse.refresh_requested);
        }
    }
}

#[test]
fn mouse_keyboard_hover_routing_popup_capture_and_resize_retire_old_wheel_geometry() {
    let mut app = App::default();
    app.content[0] = (0..80).map(|n| format!("row {n}")).collect();
    draw(&mut app, 120, 40);
    app.view = View::Repository;
    wheel(&mut app, 5, 10, true);
    assert_eq!(
        app.scroll, 0,
        "a different view cannot reuse the old pane rectangle"
    );
    app.view = View::Inbox;
    app.handle_event(Event::Resize(80, 24));
    wheel(&mut app, 5, 10, true);
    assert_eq!(
        app.scroll, 0,
        "no wheel dispatch before the resized pane is painted"
    );
    draw(&mut app, 80, 24);
    wheel(&mut app, 5, 10, true);
    assert_eq!(app.scroll, 3);
    app.dispatch(Action::Explain(
        (0..80).map(|n| format!("notice {n}\n")).collect(),
    ));
    draw(&mut app, 80, 24);
    wheel(&mut app, 5, 23, true);
    assert_eq!(
        app.notice_scroll, 0,
        "background footer cannot scroll an owned notice"
    );
    wheel(&mut app, 10, 10, true);
    assert_eq!(app.notice_scroll, 3);
    assert_eq!(
        app.scroll, 3,
        "overlay wheel never reaches its underlying content"
    );
    app.dispatch(Action::Close);
    draw(&mut app, 80, 24);
    app.overlay = Some(Overlay::LocalActions);
    draw(&mut app, 80, 24);
    wheel(&mut app, 5, 23, true);
    assert_eq!(
        app.local_action_index, 0,
        "background does not move context menu selection"
    );
    wheel(&mut app, 10, 10, true);
    assert_eq!(app.local_action_index, 3);
    assert!(app.local_command.is_none() && app.form.is_none());
}
