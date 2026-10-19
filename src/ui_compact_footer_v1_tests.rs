//! Main key bar contracts on the actual frame: the Global tail, compact labels, Cancel while
//! loading, and footer hits registered last with the same action by click and key. The
//! compiled binary's resize/quit path is covered by the PTY journeys.
use super::*;
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{backend::TestBackend, Terminal};

#[test]
fn design_footer_v90_main_footer_keeps_global_tail_compacts_and_registers_hits_last() {
    for width in [40u16, 80, 140] {
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
                let controls = crate::footer::main_controls(&app);
                let placed = crate::footer::layout(&controls, width);
                let expected = placed
                    .iter()
                    .map(|p| controls[p.index].token(p.compact))
                    .collect::<Vec<_>>();
                assert_eq!(
                    footer
                        .split("  ")
                        .map(str::trim)
                        .filter(|t| !t.is_empty())
                        .collect::<Vec<_>>(),
                    expected,
                    "painted tokens follow footer::layout at {width}"
                );
                for token in ["? Help", "q Quit"] {
                    assert!(expected.iter().any(|t| t == token), "{token} at {width}");
                }
                let commands = if width < crate::footer::COMPACT_WIDTH {
                    ": Cmds"
                } else {
                    ": Commands"
                };
                assert!(expected.iter().any(|t| t == commands));
                assert_eq!(expected.iter().any(|t| t == "Esc Cancel"), pending);
                assert_eq!(
                    expected.iter().any(|t| t == "s Settings"),
                    width >= 80,
                    "transitional Settings leaves first on narrow rows"
                );
                assert!(
                    !footer.contains('['),
                    "the key bar has no bracketed buttons"
                );
                // The footer is the last block of hits, all on the actual last row, in order.
                let first = app.hits.len() - placed.len();
                assert!(app.hits[..first].iter().all(|hit| hit.rect.y != 23));
                for (hit, p) in app.hits[first..].iter().zip(&placed) {
                    assert_eq!(hit.rect, Rect::new(p.x, 23, p.width, 1));
                    assert_eq!(hit.action, controls[p.index].action);
                    assert!(hit.rect.right() <= width);
                }
                assert_eq!(
                    app.hits[first..]
                        .iter()
                        .any(|hit| hit.action == Action::Cancel),
                    pending
                );
                let quit = app.hits[first..]
                    .iter()
                    .find(|hit| hit.action == Action::Quit)
                    .unwrap()
                    .rect;
                if mouse {
                    app.handle_event(Event::Mouse(MouseEvent {
                        kind: MouseEventKind::Down(MouseButton::Left),
                        column: quit.right() - 1,
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

#[test]
fn design_footer_v90_main_footer_help_and_commands_click_like_their_keys() {
    for (width, height) in [(140u16, 42u16), (80, 24)] {
        for (label, key, overlay) in [
            ("Help", KeyCode::Char('?'), Overlay::Help),
            ("Cmds", KeyCode::Char(':'), Overlay::Palette),
            ("Commands", KeyCode::Char(':'), Overlay::Palette),
        ] {
            if (label == "Cmds") != (width < crate::footer::COMPACT_WIDTH) {
                continue;
            }
            for mouse in [false, true] {
                let mut app = App::default();
                let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                terminal.draw(|frame| draw(frame, &mut app)).unwrap();
                let row = (0..width)
                    .map(|col| terminal.backend().buffer()[(col, height - 1)].symbol())
                    .collect::<String>();
                let column = row.find(label).unwrap() as u16;
                if mouse {
                    app.handle_event(Event::Mouse(MouseEvent {
                        kind: MouseEventKind::Down(MouseButton::Left),
                        column,
                        row: height - 1,
                        modifiers: KeyModifiers::NONE,
                    }));
                } else {
                    app.handle_event(Event::Key(KeyEvent::new(key, KeyModifiers::NONE)));
                }
                assert_eq!(
                    app.overlay,
                    Some(overlay),
                    "{label} at {width}, mouse={mouse}"
                );
            }
        }
    }
}
