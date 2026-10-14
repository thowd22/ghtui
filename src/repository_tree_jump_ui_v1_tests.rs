use super::*;
use crate::{
    git::{original_tree_inventory_v1::tests::fixture_tree, OriginalTreeIntentV1},
    local_commit_actor_origin_v1_test_support::Real,
    providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
    repository_tree_jump_v1::{OriginalTreeFrameV1, OriginalTreeJumpEffectV1},
};
use ratatui::{backend::TestBackend, Terminal};
use std::sync::Arc;

fn state() -> (Real, OriginalTreeJumpV1) {
    let real = Real::new("sha256");
    let workspace = fixture_tree(
        &real,
        &[
            b"AlphaSharedVeryLongFileName.rs",
            b"AlphaSharedVeryLongFileNames.rs",
            b"wide-\xe6\x97\xa5\xe6\x9c\xac.rs",
        ],
    );
    let mut state = OriginalTreeJumpV1::new(OriginalTreeIntentV1::capture(&workspace).unwrap());
    let request = state.request().unwrap();
    let op =
        LocalGitNavigationReadV1::begin(request.intent().origin(), Default::default()).unwrap();
    let frame = OriginalTreeFrameV1::prepare(request, &op).unwrap();
    op.seal().unwrap();
    assert!(state.accept(frame));
    (real, state)
}
fn draw(
    state: &mut OriginalTreeJumpV1,
    layout: &mut OriginalTreeLayoutV1,
    width: u16,
    height: u16,
) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| layout.draw(frame, state, frame.area(), Theme::from_settings(2, true)))
        .unwrap();
    terminal
}
fn screen(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            out.push_str(buffer[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}
#[test]
fn original_tree_jump_widget_wide_narrow_semantic_labels_full_sha_hits_and_keyboard_equivalence() {
    let (real, mut state) = state();
    real.clear_trace();
    let mut layout = OriginalTreeLayoutV1::default();
    for (w, h) in [(120, 40), (80, 24)] {
        let terminal = draw(&mut state, &mut layout, w, h);
        let view = screen(&terminal);
        assert!(view.contains("Jump to saved path"));
        assert!(view.contains(state.intent_sha_v1()));
        assert!(view.contains("[Esc Close]"));
        let row = layout
            .hits()
            .iter()
            .find(|hit| matches!(hit.action, Action::Activate(_)))
            .unwrap();
        let mouse = layout
            .mouse(
                MouseEventKind::Down(MouseButton::Left),
                row.rect.x,
                row.rect.y,
            )
            .unwrap();
        state.apply(Action::FocusList).unwrap();
        let enter = layout
            .key(&state, KeyCode::Enter, KeyModifiers::NONE)
            .unwrap();
        let OriginalTreeJumpEffectV1::Open(key_ticket) = state.apply(enter).unwrap() else {
            panic!("Enter must open exact member")
        };
        let OriginalTreeJumpEffectV1::Open(mouse_ticket) = state.apply(mouse).unwrap() else {
            panic!("Mouse must open exact member")
        };
        assert_eq!(key_ticket, mouse_ticket);
    }
    assert!(real.children().is_empty());
}
#[test]
fn original_tree_jump_widget_resize_clears_stale_geometry_and_tiny_close_remains_clickable() {
    let (_, mut state) = state();
    let mut layout = OriginalTreeLayoutV1::default();
    draw(&mut state, &mut layout, 120, 40);
    let old = layout.hits().last().unwrap().rect;
    layout.clear();
    assert!(layout
        .mouse(MouseEventKind::Down(MouseButton::Left), old.x, old.y)
        .is_none());
    draw(&mut state, &mut layout, 20, 5);
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
fn original_tree_jump_widget_focus_query_caret_paste_bounds_and_no_overlay_fallthrough() {
    let (_, mut state) = state();
    let mut layout = OriginalTreeLayoutV1::default();
    draw(&mut state, &mut layout, 80, 24);
    assert_eq!(
        layout.key(&state, KeyCode::Esc, KeyModifiers::NONE),
        Some(Action::Close)
    );
    assert_eq!(
        layout.key(&state, KeyCode::Tab, KeyModifiers::NONE),
        Some(Action::FocusNext(false))
    );
    let text = layout
        .key(&state, KeyCode::Char('日'), KeyModifiers::NONE)
        .unwrap();
    state.apply(text).unwrap();
    assert_eq!(state.query.text, "日");
    assert!(state.apply(Action::Paste("z".repeat(257))).is_err());
    assert_eq!(state.query.text, "日");
    state.apply(Action::Clear).unwrap();
    state.apply(Action::Paste("日".repeat(80))).unwrap();
    draw(&mut state, &mut layout, 80, 24);
    assert!(state.query.left_column > 0);
    let old_left = state.query.left_column;
    let click = layout
        .mouse(
            MouseEventKind::Down(MouseButton::Left),
            layout.query.x,
            layout.query.y,
        )
        .unwrap();
    state.apply(click).unwrap();
    assert_eq!(
        display_width(&state.query.text[..state.query.cursor]),
        old_left
    );
}
#[test]
fn original_tree_jump_widget_hovered_wheel_and_keyboard_share_one_focus_action() {
    let (_, mut state) = state();
    let mut layout = OriginalTreeLayoutV1::default();
    draw(&mut state, &mut layout, 120, 40);
    state.apply(Action::FocusDetail).unwrap();
    let key = layout.key(&state, KeyCode::Down, KeyModifiers::NONE);
    let mouse = layout.mouse(MouseEventKind::ScrollDown, layout.detail.x, layout.detail.y);
    assert_eq!(key, mouse);
    assert_eq!(layout.mouse(MouseEventKind::ScrollDown, 0, 0), None);
    state.apply(Action::FocusList).unwrap();
    assert_eq!(
        layout.key(&state, KeyCode::Down, KeyModifiers::NONE),
        layout.mouse(MouseEventKind::ScrollDown, layout.list.x, layout.list.y)
    );
}
#[test]
fn original_tree_jump_widget_pending_query_has_no_old_clickable_members_or_choose_target() {
    let (_, mut state) = state();
    let mut layout = OriginalTreeLayoutV1::default();
    draw(&mut state, &mut layout, 120, 40);
    state.apply(Action::Insert("Alpha".into())).unwrap();
    let terminal = draw(&mut state, &mut layout, 80, 24);
    assert!(screen(&terminal).contains("Original source/filter pending"));
    assert!(!layout
        .hits()
        .iter()
        .any(|hit| matches!(hit.action, Action::Activate(_))));
    assert!(state.apply(Action::Choose).is_err());
}

#[test]
fn original_tree_jump_widget_rejected_input_is_visible_and_keeps_actual_frame_query_and_zero_git() {
    let (real, mut state) = state();
    let original = state.frame().unwrap().clone();
    let before = real.snapshot();
    real.clear_trace();
    assert!(state.apply(Action::Paste("x".repeat(257))).is_err());
    assert!(state.query.text.is_empty());
    assert!(state.pending().is_none());
    assert!(Arc::ptr_eq(state.frame().unwrap(), &original));
    let mut layout = OriginalTreeLayoutV1::default();
    for (width, height) in [(120, 40), (80, 24)] {
        let terminal = draw(&mut state, &mut layout, width, height);
        let visible = screen(&terminal);
        assert!(visible.contains("limited to256 raw bytes"));
        assert!(visible.contains("rejected input"));
        assert!(layout
            .hits()
            .iter()
            .any(|hit| matches!(hit.action, Action::Activate(_))));
    }
    assert!(real.children().is_empty());
    assert_eq!(real.snapshot(), before);
}

#[test]
fn original_tree_jump_widget_footer_focus_and_enter_share_actual_click_action() {
    let (real, mut state) = state();
    real.clear_trace();
    let mut layout = OriginalTreeLayoutV1::default();
    for (focus, action) in [
        (Focus::Choose, Action::Choose),
        (Focus::Previous, Action::Page(-1)),
        (Focus::Next, Action::Page(1)),
        (Focus::Close, Action::Close),
    ] {
        state.focus = focus;
        for (width, height) in [(120, 40), (80, 24)] {
            let terminal = draw(&mut state, &mut layout, width, height);
            let hit = layout
                .hits()
                .iter()
                .find(|hit| hit.action == action)
                .unwrap();
            assert!(terminal.backend().buffer()[(hit.rect.x, hit.rect.y)]
                .modifier
                .contains(Modifier::UNDERLINED));
            assert_eq!(
                layout.key(&state, KeyCode::Enter, KeyModifiers::NONE),
                Some(action.clone())
            );
            assert_eq!(
                layout.mouse(
                    MouseEventKind::Down(MouseButton::Left),
                    hit.rect.x,
                    hit.rect.y
                ),
                Some(action.clone())
            );
        }
    }
    state.focus = Focus::Close;
    let terminal = draw(&mut state, &mut layout, 20, 5);
    let hit = layout
        .hits()
        .iter()
        .find(|hit| hit.action == Action::Close)
        .unwrap();
    assert!(terminal.backend().buffer()[(hit.rect.x, hit.rect.y)]
        .modifier
        .contains(Modifier::UNDERLINED));
    assert_eq!(
        layout.key(&state, KeyCode::Enter, KeyModifiers::NONE),
        Some(Action::Close)
    );
    assert!(real.children().is_empty());
}
