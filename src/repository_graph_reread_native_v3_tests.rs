//! SOURCE-ONLY TestBackend/input contracts, complementary to real PTYs.
use super::*;
use crate::{
    local_commit_actor_origin_v1_test_support::Real,
    workspace::{RepositoryMode, Workspace},
};
use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::{backend::TestBackend, Terminal};

fn app(real: &Real) -> App {
    App {
        view: View::Repository,
        workspace: Workspace::load(&real.root, RepositoryMode::Graph, &real.original, None)
            .unwrap(),
        refresh_requested: false,
        ..Default::default()
    }
}
fn key(app: &mut App, code: KeyCode) {
    assert!(event(
        app,
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    ));
}
#[test]
fn commit_graph_v3_preview_same_action_keys_click_focus_resize_scroll_and_back_are_zero_reads() {
    let real = Real::new("sha1");
    let mut app = app(&real);
    let source = app
        .workspace
        .as_ref()
        .unwrap()
        .graph_state_v1
        .as_ref()
        .unwrap()
        .clone();
    real.clear_trace();
    for width in [120, 80, 40] {
        dispatch(&mut app, GraphRereadActionV3::Open);
        let mut terminal = Terminal::new(TestBackend::new(width, 24)).unwrap();
        for focused in 0..4 {
            terminal
                .draw(|frame| {
                    assert!(draw(frame, &mut app, frame.area()));
                })
                .unwrap();
            let hits = app.graph_reread_dialog_v3.as_ref().unwrap().hits();
            assert_eq!(hits.len(), 4);
            for (index, hit) in hits.iter().enumerate() {
                assert!(matches!(hit.action, Action::GraphRereadV3(_)));
                let cell = &terminal.backend().buffer()[(hit.rect.x, hit.rect.y)];
                assert_eq!(
                    cell.modifier.contains(Modifier::UNDERLINED),
                    index == focused
                );
                assert!(hit.rect.right() <= width);
            }
            key(&mut app, KeyCode::Tab);
        }
        assert!(event(&mut app, Event::Resize(80, 24)));
        assert!(app
            .graph_reread_dialog_v3
            .as_ref()
            .unwrap()
            .hits()
            .is_empty());
        terminal
            .draw(|frame| {
                draw(frame, &mut app, frame.area());
            })
            .unwrap();
        let body = app.graph_reread_dialog_v3.as_ref().unwrap().body;
        assert!(event(
            &mut app,
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: body.x,
                row: body.y,
                modifiers: KeyModifiers::NONE
            })
        ));
        assert!(app.graph_reread_dialog_v3.as_ref().unwrap().scroll > 0 || width == 120);
        key(&mut app, KeyCode::PageUp);
        assert_eq!(app.graph_reread_dialog_v3.as_ref().unwrap().scroll, 0);
        let back = app.graph_reread_dialog_v3.as_ref().unwrap().hits()[1].rect;
        assert!(event(
            &mut app,
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: back.x,
                row: back.y,
                modifiers: KeyModifiers::NONE
            })
        ));
        assert!(app.graph_reread_dialog_v3.is_none());
        assert!(app.graph_reread_request_v3.is_none());
        assert!(app.overlay.is_none());
        assert!(Arc::ptr_eq(
            app.workspace
                .as_ref()
                .unwrap()
                .graph_state_v1
                .as_ref()
                .unwrap(),
            &source
        ));
    }
    assert!(real.children().is_empty());
}
#[test]
fn commit_graph_v3_preview_captures_unknown_input_no_clickthrough_and_retires_equal_text_new_owner()
{
    let real = Real::new("sha1");
    let mut app = app(&real);
    dispatch(&mut app, GraphRereadActionV3::Open);
    real.clear_trace();
    key(&mut app, KeyCode::Char('r'));
    key(&mut app, KeyCode::Char('g'));
    assert!(event(
        &mut app,
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE
        })
    ));
    assert!(app.graph_reread_dialog_v3.is_some());
    assert!(!app.refresh_requested && app.repository_action.is_none());
    assert!(real.children().is_empty());
    app.workspace =
        Workspace::load(&real.root, RepositoryMode::Graph, &real.original, None).unwrap();
    real.clear_trace();
    key(&mut app, KeyCode::Enter);
    assert!(app.overlay.is_none());
    assert!(app.graph_reread_request_v3.is_none());
    assert!(
        real.children().is_empty(),
        "Changed source refuses before any new read"
    );
}
#[test]
fn commit_graph_v3_confirm_is_only_explicit_prepared_request_and_source_change_cannot_take_it() {
    let real = Real::new("sha256");
    let mut app = app(&real);
    dispatch(&mut app, GraphRereadActionV3::Open);
    real.clear_trace();
    key(&mut app, KeyCode::Enter);
    assert!(app.overlay.is_none());
    assert!(app.graph_reread_request_v3.is_some());
    assert!(real.children().is_empty());
    app.workspace =
        Workspace::load(&real.root, RepositoryMode::Graph, &real.original, None).unwrap();
    real.clear_trace();
    assert!(take_request(&mut app).is_none());
    assert!(app.status.contains("owner/retention changed"));
    assert!(real.children().is_empty());
}
