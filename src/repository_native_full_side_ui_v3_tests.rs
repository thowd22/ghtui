use super::*;
use crate::{
    git::{Comparison, OriginalComparisonSourceV1},
    original_diff_sides_v1_test_support as f,
    providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
};
use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{backend::TestBackend, Terminal};
fn loaded(
    width: u16,
) -> (
    crate::local_commit_actor_origin_v1_test_support::Real,
    NativeDiffStateV1,
) {
    let (real, a, b, _) = f::fixture("sha256");
    let mut workspace = f::workspace(&real, Comparison::BranchChanges { base: a, head: b });
    f::source(&workspace).install(&mut workspace).unwrap();
    let mut state = NativeDiffStateV1::<OriginalComparisonSourceV1>::open_source(
        &workspace,
        width.saturating_sub(2).min(512),
    )
    .unwrap();
    state.file = state
        .source()
        .unwrap()
        .members()
        .iter()
        .position(|member| {
            member
                .path(crate::git::OriginalComparisonSideV1::New)
                .is_some_and(|path| path.0 == b"shared")
        })
        .unwrap();
    prepare(&mut state);
    state.open_full_side(DiffSide::New).unwrap();
    prepare(&mut state);
    (real, state)
}
fn prepare(state: &mut NativeDiffStateV1) {
    let request = state.request().unwrap();
    let op = LocalGitNavigationReadV1::begin(state.intent().origin(), Default::default()).unwrap();
    let prepared = request.prepare(&op).unwrap();
    op.seal().unwrap();
    assert!(state.accept(prepared));
}
fn screen(
    layout: &mut NativeDiffLayoutV1,
    state: &mut NativeDiffStateV1,
    width: u16,
    height: u16,
) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| layout.draw(frame, state, frame.area(), Theme::from_settings(0, false)))
        .unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}
#[test]
fn native_full_side_ui_all_visible_controls_keyboard_mouse_and_600_geometry() {
    for width in [40, 120, 600] {
        let (real, mut state) = loaded(width);
        let before = real.snapshot();
        real.clear_trace();
        let mut layout = NativeDiffLayoutV1::default();
        let text = screen(&mut layout, &mut state, width, 40);
        assert!(text.contains("FULL New file"));
        assert!(text.contains("Path: shared"));
        assert!(text.contains("TARGET new bytes"));
        let hits = layout.hits.clone();
        assert!(hits.iter().any(|hit| hit.action == A::BackToPatch));
        for hit in hits {
            assert!(hit.rect.right() < width.min(514));
            assert_eq!(
                layout.mouse(
                    MouseEventKind::Down(MouseButton::Left),
                    hit.rect.x,
                    hit.rect.y
                ),
                Some(hit.action)
            );
        }
        for (key, action) in [
            ('F', A::FullSide(DiffSide::Old)),
            ('V', A::FullSide(DiffSide::New)),
            ('b', A::BackToPatch),
        ] {
            assert_eq!(
                layout.key(KeyCode::Char(key), KeyModifiers::NONE),
                Some(action)
            );
        }
        assert!(real.children().is_empty());
        assert_eq!(real.snapshot(), before);
    }
}
#[test]
fn native_full_side_ui_pending_and_tiny_have_no_full_row_hits() {
    let (_real, mut state) = loaded(120);
    let mut layout = NativeDiffLayoutV1::default();
    screen(&mut layout, &mut state, 120, 40);
    assert!(layout
        .hits
        .iter()
        .any(|hit| matches!(hit.action, A::FullPoint { .. })));
    state.query.insert("TARGET").unwrap();
    state.request().unwrap();
    screen(&mut layout, &mut state, 120, 40);
    assert!(layout
        .hits
        .iter()
        .all(|hit| !matches!(hit.action, A::FullPoint { .. } | A::Point { .. })));
    for (width, height) in [(10, 5), (18, 12), (32, 20)] {
        let tiny = screen(&mut layout, &mut state, width, height);
        assert!(layout
            .hits
            .iter()
            .all(|hit| hit.action == A::Close || hit.action == A::BackToPatch));
        assert!(layout.hits.iter().any(|hit| hit.action == A::Close));
        assert!(!tiny.contains("TARGET new bytes"));
    }
}
#[test]
fn native_full_side_ui_options_capture_fullside_keys_but_query_retains_text_input() {
    let (_real, mut state) = loaded(40);
    let mut layout = NativeDiffLayoutV1::default();
    layout.controls = true;
    screen(&mut layout, &mut state, 40, 24);
    assert!(layout
        .hits
        .iter()
        .any(|hit| hit.action == A::FullSide(DiffSide::Old)));
    assert!(layout
        .hits
        .iter()
        .any(|hit| hit.action == A::FullSide(DiffSide::New)));
    assert!(layout.hits.iter().any(|hit| hit.action == A::BackToPatch));
    layout.query_focused = true;
    assert_eq!(
        layout.key(KeyCode::Char('F'), KeyModifiers::NONE),
        Some(A::Insert("F".into()))
    );
    assert_eq!(
        layout.key(KeyCode::Char('V'), KeyModifiers::NONE),
        Some(A::Insert("V".into()))
    );
    assert_eq!(
        layout.key(KeyCode::Char('b'), KeyModifiers::NONE),
        Some(A::Insert("b".into()))
    );
}
