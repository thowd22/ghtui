use super::*;
use crate::{
    git::Comparison, original_diff_sides_v1_test_support as support,
    repository_diff_sides_v1::OriginalDiffEffectV1,
};
use ratatui::{backend::TestBackend, Terminal};
fn fixture() -> (
    crate::local_commit_actor_origin_v1_test_support::Real,
    OriginalDiffPickerV1,
) {
    let (real, a, b, _) = support::fixture("sha256");
    let workspace = support::workspace(&real, Comparison::BranchChanges { base: a, head: b });
    let picker = support::state(&workspace);
    (real, picker)
}
fn draw(
    picker: &mut OriginalDiffPickerV1,
    layout: &mut OriginalDiffLayoutV1,
    w: u16,
    h: u16,
) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal
        .draw(|frame| layout.draw(frame, picker, frame.area(), Theme::from_settings(2, true)))
        .unwrap();
    terminal
}
fn screen(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            let cell = &buffer[(x, y)];
            if x > 0
                && buffer[(x - 1, y)]
                    .symbol()
                    .chars()
                    .any(|ch| ratatui::text::Span::raw(ch.to_string()).width() > 1)
                && cell.symbol() == " "
            {
                continue;
            }
            out.push_str(cell.symbol());
        }
        out.push('\n');
    }
    out
}
#[test]
fn original_diff_sides_widget_full_sha_narrow_wide_readable_old_new_no_color_and_same_actions() {
    let (real, mut picker) = fixture();
    real.clear_trace();
    let mut layout = OriginalDiffLayoutV1::default();
    for (w, h) in [(80, 24), (120, 40)] {
        let terminal = draw(&mut picker, &mut layout, w, h);
        let text = screen(&terminal);
        assert!(text.contains(picker.intent().head()));
        assert!(text.contains(picker.intent().content_base()));
        assert!(text.contains("[O Open old]"));
        assert!(text.contains("[N Open new]"));
        let hit = layout
            .hits()
            .iter()
            .find(|hit| hit.action == Action::OpenSide(Side::New))
            .unwrap();
        let mouse = layout
            .mouse(
                MouseEventKind::Down(MouseButton::Left),
                hit.rect.x,
                hit.rect.y,
            )
            .unwrap();
        picker.apply(Action::Focus(Focus::List)).unwrap();
        let key = layout
            .key(&picker, KeyCode::Char('N'), KeyModifiers::NONE)
            .unwrap();
        assert_eq!(mouse, key);
        assert!(matches!(
            picker.apply(key),
            Ok(OriginalDiffEffectV1::Side(_))
        ));
    }
    assert!(real.children().is_empty());
}
#[test]
fn original_diff_sides_widget_resize_stale_hits_tiny_close_and_hovered_wheel_parity() {
    let (_, mut picker) = fixture();
    let mut layout = OriginalDiffLayoutV1::default();
    draw(&mut picker, &mut layout, 120, 40);
    picker.apply(Action::Focus(Focus::Detail)).unwrap();
    assert_eq!(
        layout.key(&picker, KeyCode::Down, KeyModifiers::NONE),
        layout.mouse(MouseEventKind::ScrollDown, layout.detail.x, layout.detail.y)
    );
    assert_eq!(layout.mouse(MouseEventKind::ScrollDown, 0, 0), None);
    let rect = layout.hits().last().unwrap().rect;
    layout.clear();
    assert!(layout
        .mouse(MouseEventKind::Down(MouseButton::Left), rect.x, rect.y)
        .is_none());
    draw(&mut picker, &mut layout, 20, 5);
    let close = layout
        .hits()
        .iter()
        .find(|hit| hit.action == Action::Close)
        .unwrap();
    assert_eq!(
        layout.mouse(
            MouseEventKind::Down(MouseButton::Left),
            close.rect.x,
            close.rect.y
        ),
        Some(Action::Close)
    );
}
#[test]
fn original_diff_sides_widget_query_focus_capture_caret_bounds_and_no_stale_members() {
    let (_, mut picker) = fixture();
    let mut layout = OriginalDiffLayoutV1::default();
    draw(&mut picker, &mut layout, 80, 24);
    assert_eq!(
        layout.key(&picker, KeyCode::Char('N'), KeyModifiers::NONE),
        Some(Action::Insert("N".into()))
    );
    picker.apply(Action::Paste("日".repeat(80))).unwrap();
    draw(&mut picker, &mut layout, 80, 24);
    assert!(picker.query.left_column > 0);
    let before = picker.query.text.clone();
    assert!(picker.apply(Action::Paste("x".repeat(257))).is_err());
    assert_eq!(picker.query.text, before);
    assert!(!layout
        .hits()
        .iter()
        .any(|hit| matches!(hit.action, Action::Select(_))));
    assert!(picker.apply(Action::OpenSide(Side::New)).is_err());
    let click = layout
        .mouse(
            MouseEventKind::Down(MouseButton::Left),
            layout.query.x,
            layout.query.y,
        )
        .unwrap();
    picker.apply(click).unwrap();
    assert!(picker.query.cursor < picker.query.text.len());
}
#[test]
fn original_diff_sides_widget_row_focus_is_not_implied_side_or_activation() {
    let (_, mut picker) = fixture();
    let mut layout = OriginalDiffLayoutV1::default();
    draw(&mut picker, &mut layout, 120, 40);
    let hit = layout
        .hits()
        .iter()
        .find(|hit| matches!(hit.action, Action::Select(_)))
        .unwrap();
    let action = layout
        .mouse(
            MouseEventKind::Down(MouseButton::Left),
            hit.rect.x,
            hit.rect.y,
        )
        .unwrap();
    assert!(matches!(
        picker.apply(action),
        Ok(OriginalDiffEffectV1::None)
    ));
    assert_eq!(picker.focus, Focus::List);
    assert_eq!(
        layout.key(&picker, KeyCode::Enter, KeyModifiers::NONE),
        Some(Action::OpenSide(Side::New))
    );
}
