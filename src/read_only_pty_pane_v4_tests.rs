use super::*;
use crate::providers::owned_pty_viewer_v4::{
    tests::{fixture, wait_exit, wait_frame},
    DisplayPermitV4, PreparedPagerCaptureV4, ViewerAdmissionV4,
};
use ratatui::{backend::TestBackend, Terminal};

#[test]
fn native_external_diff_pane_all_visible_controls_have_shared_actions_and_real_hit_geometry() {
    for (width, height) in [(40, 24), (80, 24), (120, 40), (600, 40)] {
        let area = Rect::new(0, 0, width, height);
        let layout = layout(area);
        assert!(ReadOnlyPtyPaneV4::geometry(area, 1).is_some());
        assert_eq!(layout.hits.len(), 12);
        for hit in &layout.hits {
            assert!(
                hit.rect.x > 0
                    && hit.rect.y > 0
                    && hit.rect.right() < width
                    && hit.rect.bottom() < height
            );
            assert!(!label(&hit.action).is_empty());
        }
        assert!(layout
            .hits
            .iter()
            .any(|hit| hit.action == PtyPaneActionV4::Return));
        assert!(layout
            .hits
            .iter()
            .any(|hit| hit.action == PtyPaneActionV4::Cancel));
        assert!(layout.body.width <= width - 2);
    }
    assert!(ReadOnlyPtyPaneV4::geometry(Rect::new(0, 0, 40, 8), 0).is_none());
    let tiny = layout(Rect::new(0, 0, 40, 8));
    assert!(tiny
        .hits
        .iter()
        .any(|hit| hit.action == PtyPaneActionV4::Return));
    assert!(tiny
        .hits
        .iter()
        .any(|hit| hit.action == PtyPaneActionV4::Cancel));
}
#[test]
fn native_external_diff_pane_untrusted_headers_cannot_be_terminal_effects_or_unbounded_metadata() {
    let safe = safe_header(Arc::from("source\x1b]52;c;secret\x07\u{202e}"));
    assert!(!safe.contains('\x1b'));
    assert!(!safe.chars().any(char::is_control));
    assert!(!safe.contains('\u{202e}'));
    let huge = safe_header(Arc::from("x".repeat(8193)));
    assert!(huge.len() < 128);
    assert!(huge.contains("bounded"));
    let original = Style::default()
        .fg(Color::Rgb(240, 0, 0))
        .bg(Color::Indexed(200))
        .add_modifier(Modifier::BOLD);
    let plain = cell_style(original, ColorMode::NoColor);
    assert_eq!(plain.fg, Some(Color::Reset));
    assert_eq!(plain.bg, Some(Color::Reset));
    assert!(plain.add_modifier.contains(Modifier::BOLD));
    let low = cell_style(original, ColorMode::LowColor);
    assert!(matches!(low.fg, Some(Color::Indexed(0..=15))));
    assert!(matches!(low.bg, Some(Color::Indexed(0..=15))));
}
#[test]
fn native_external_diff_pane_real_cells_keyboard_mouse_return_and_tiny_resize_capture() {
    for mouse in [false, true] {
        let fixture = fixture("");
        let capture = PreparedPagerCaptureV4::from_renderer(b"pane-original").unwrap();
        let admission = ViewerAdmissionV4::new();
        let area = Rect::new(0, 0, 80, 24);
        let geometry = ReadOnlyPtyPaneV4::geometry(area, 0).unwrap();
        let handle = PtyViewerHandleV4::launch(
            capture,
            fixture.pager(),
            DisplayPermitV4::launch(),
            geometry,
            &admission,
        )
        .unwrap();
        wait_frame(&handle, "READY pane-original");
        let mut pane = ReadOnlyPtyPaneV4::new(
            handle,
            geometry,
            Arc::from("Original saved C → B"),
            Arc::from("delta • WHOLE captured comparison • read-only"),
        );
        pane.resized(area);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| pane.draw(frame, area, Theme::from_settings(0, true)))
            .unwrap();
        let cell_epoch = pane.geometry;
        pane.resized(Rect::new(0, 0, 40, 8));
        assert!(pane.frame.is_none());
        assert_ne!(pane.epoch, cell_epoch.epoch());
        terminal.resize(Rect::new(0, 0, 40, 8)).unwrap();
        terminal
            .draw(|frame| pane.draw(frame, Rect::new(0, 0, 40, 8), Theme::from_settings(0, true)))
            .unwrap();
        let actual: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(
            actual.contains("Resize"),
            "tiny real painted pane must explain why input is disabled"
        );
        assert_eq!(
            pane.layout.hits.len(),
            2,
            "only genuine host Return/Cancel hits survive tiny geometry"
        );
        assert!(pane.event(&Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Char('a'),
            KeyModifiers::NONE
        ))));
        assert!(
            !pane.returned,
            "native Stage must not receive a behind-pane key"
        );
        if mouse {
            let hit = pane
                .layout
                .hits
                .iter()
                .find(|hit| hit.action == PtyPaneActionV4::Return)
                .unwrap()
                .rect;
            pane.event(&Event::Mouse(crossterm::event::MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: hit.x,
                row: hit.y,
                modifiers: KeyModifiers::NONE,
            }));
        } else {
            pane.event(&Event::Key(crossterm::event::KeyEvent::new(
                KeyCode::Esc,
                KeyModifiers::NONE,
            )));
        }
        assert!(pane.returned);
        assert!(pane.stopping);
        assert!(pane.frame.is_none());
        let exit = wait_exit(&mut pane.handle);
        assert!(exit.cleanup_proven);
        assert!(!admission.blocked());
    }
}
