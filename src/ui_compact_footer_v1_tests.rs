//! Compact main-footer contracts; actual 40-column Graph PTYs verify the
//! compiled application resize/quit path independently of these widgets.
use super::*;
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{backend::TestBackend, Terminal};

#[test]
fn navigation_compact_footer_preserves_visible_quit_commands_and_owned_cancel_hits() {
    for width in [40, 80, 120] {
        for pending in [false, true] {
            for mouse in [false, true] {
                let mut app = App {
                    view: View::Repository,
                    ..Default::default()
                };
                app.loading[View::Repository.index()] = pending;
                let mut terminal = Terminal::new(TestBackend::new(width, 24)).unwrap();
                terminal.draw(|frame| draw(frame, &mut app)).unwrap();
                let footer = (0..width)
                    .map(|col| terminal.backend().buffer()[(col, 23)].symbol())
                    .collect::<String>();
                assert!(footer.contains("Help"));
                assert!(footer.contains("Quit"));
                assert!(footer.contains(if width < 80 { "Cmds" } else { "Commands" }));
                assert_eq!(footer.contains("Cancel"), pending);
                let controls = app
                    .hits
                    .iter()
                    .filter(|hit| hit.rect.y == 23)
                    .collect::<Vec<_>>();
                assert!(controls
                    .iter()
                    .all(|hit| hit.rect.width > 0 && hit.rect.right() <= width));
                assert!(controls.iter().any(|hit| hit.action == Action::Palette));
                assert_eq!(
                    controls.iter().any(|hit| hit.action == Action::Cancel),
                    pending
                );
                let quit = controls
                    .iter()
                    .find(|hit| hit.action == Action::Quit)
                    .unwrap()
                    .rect;
                if mouse {
                    app.handle_event(Event::Mouse(MouseEvent {
                        kind: MouseEventKind::Down(MouseButton::Left),
                        column: quit.x,
                        row: quit.y,
                        modifiers: KeyModifiers::NONE,
                    }));
                } else {
                    app.handle_event(Event::Key(KeyEvent::new(
                        KeyCode::Char('q'),
                        KeyModifiers::NONE,
                    )));
                }
                assert!(
                    app.quit,
                    "Same visible Quit action at {width}, pending={pending}, mouse={mouse}"
                );
            }
        }
    }
}
