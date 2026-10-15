//! Genuine native proof/Files members, one shared action, no new capabilities.
use super::*;
use crate::repository_native_diff_native_v1::NativeAppV2;
use crate::repository_native_diff_ui_v1::NativeDiffActionV1 as A;
use crate::scoped_pr_observed_tabs_v1::resources::threads_v1 as threads;
use crate::{
    git::OriginalComparisonSourceV1, repository_native_diff_content_v2::NativeDiffPurposeV2,
};

fn genuine_file_selection(app: &mut App) {
    let github = app.github.as_ref().unwrap();
    let view = github.scoped_pr_resource.as_ref().unwrap();
    app.selected = (0..github.pr.as_ref().unwrap().rows.len())
        .find(|row| matches!(view.row(*row), Some(PrResourceRowV1::FileList(0))))
        .unwrap();
}
#[test]
fn review_threads_v3_native_key_and_click_action_capture_original_files_and_pure_back_retains_state(
) {
    for mouse in [false, true] {
        let (s, mut app, mut background) = loaded(|_, _| {});
        genuine_file_selection(&mut app);
        select(&s, &mut app, &mut background, DiffSide::New);
        let state = app.pr_native_diff_v1.as_ref().unwrap();
        let frame = state.current_frame().unwrap().clone();
        let range = state.range().unwrap().clone();
        let source = state.source().unwrap().clone();
        let viewport = (state.selected, state.scroll, state.horizontal, state.right);
        let query = state.query.text.clone();
        let before = commands(&s.fixture).len();
        if mouse {
            use ratatui::{backend::TestBackend, Terminal};
            let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
            terminal
                .draw(|frame| {
                    let area = frame.area();
                    crate::repository_native_diff_native_v1::draw(frame, &mut app, area);
                })
                .unwrap();
            let hit = app
                .pr_native_diff_layout_v1
                .hits()
                .iter()
                .find(|hit| hit.action == A::Threads)
                .unwrap();
            let event = Event::Mouse(crossterm::event::MouseEvent {
                kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                column: hit.rect.x,
                row: hit.rect.y,
                modifiers: KeyModifiers::NONE,
            });
            assert!(crate::repository_native_diff_native_v1::capture(
                &mut app, &event
            ));
        } else {
            assert!(crate::repository_native_diff_native_v1::capture(
                &mut app,
                &Event::Key(KeyEvent::new(KeyCode::Char('T'), KeyModifiers::NONE))
            ));
        }
        let owner = app.scoped_threads_work_v1.as_ref().unwrap();
        assert!(owner.files_intent().unwrap().owns(&app));
        assert_eq!(
            owner
                .files_intent()
                .unwrap()
                .path()
                .expect("Actual Files origin retains its observed path")
                .as_bytes(),
            source
                .proof()
                .observed()
                .canonical_file_v1()
                .new_path
                .as_deref()
                .unwrap()
        );
        assert_eq!(app.overlay, Some(Overlay::PrThreadsV1));
        threads::native_v1::close(&mut app);
        let state = app.pr_native_diff_v1.as_ref().unwrap();
        assert!(Arc::ptr_eq(state.current_frame().unwrap(), &frame));
        assert!(Arc::ptr_eq(state.range().unwrap(), &range));
        assert!(Arc::ptr_eq(state.source().unwrap(), &source));
        assert_eq!(
            (state.selected, state.scroll, state.horizontal, state.right),
            viewport
        );
        assert_eq!(state.query.text, query);
        assert_eq!(commands(&s.fixture).len(), before);
        unchanged_git_and_exact_script(&s);
    }
}
#[test]
fn review_threads_v3_native_threads_refuses_overlay_pending_foreign_source_and_full_side_no_local_hook(
) {
    assert!(!OriginalComparisonSourceV1::supports_review_threads());
    for case in 0..5 {
        let (s, mut app, mut background) = loaded(|_, _| {});
        genuine_file_selection(&mut app);
        let before = commands(&s.fixture).len();
        match case {
            0 => app.overlay = Some(Overlay::Help),
            1 => app.refresh_requested = true,
            2 => {
                app.github.as_mut().unwrap().identity.account_id = Some(43);
            }
            3 => {
                let state = app.pr_native_diff_v1.as_mut().unwrap();
                state.option(NativeDiffOptionChangeV1::Layout).unwrap();
            }
            _ => {
                let state = app.pr_native_diff_v1.as_mut().unwrap();
                state.open_full_side(DiffSide::New).unwrap();
            }
        }
        <ScopedPrNativeSourceV1 as NativeAppV2>::threads(&mut app);
        assert!(app.scoped_threads_work_v1.is_none());
        assert_ne!(app.overlay, Some(Overlay::PrThreadsV1));
        assert_eq!(commands(&s.fixture).len(), before);
        // Consume only this genuine fixture's original work, never a new one.
        app.refresh_requested = false;
        app.pr_native_diff_work_v1 = None;
        let _ = &mut background;
        unchanged_git_and_exact_script(&s);
    }
}
