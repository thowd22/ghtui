use super::*;
use crate::{
    local_commit_actor_origin_v1_test_support::Real,
    providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
    repository_diff_sides_worker_v1::OriginalDiffWorkerOwnerV1,
    repository_native_diff_v1::NativeDiffStateV1,
    workspace::{RepositoryMode, Workspace},
};
use crossterm::event::{KeyEvent, MouseEvent};
fn project(state: &mut NativeDiffStateV1<MutableDiffSourceV2>) {
    let request = state.request().unwrap();
    let op =
        LocalGitNavigationReadV1::begin(request.intent().origin(), Default::default()).unwrap();
    let prepared = request.prepare(&op).unwrap();
    op.seal().unwrap();
    assert!(state.accept(prepared));
}
fn ready(real: &Real) -> App {
    let workspace = Workspace::load(&real.root, RepositoryMode::Working, "HEAD", None)
        .unwrap()
        .unwrap();
    let mut state = NativeDiffStateV1::open_mutable(&workspace, 118).unwrap();
    project(&mut state);
    state.select_hunk(0).unwrap();
    project(&mut state);
    let mut app = App {
        view: View::Repository,
        workspace: Some(workspace),
        native_mutable_diff_v2: Some(state),
        refresh_requested: false,
        ..Default::default()
    };
    begin(&mut app);
    let work = app.original_diff_work_v1.take().unwrap();
    let owner = OriginalDiffWorkerOwnerV1::begin(
        app.workspace.as_ref().unwrap(),
        None,
        work,
        Default::default(),
    )
    .unwrap();
    owner
        .prepare()
        .unwrap()
        .publish_mutable_plan_v2(
            app.workspace.as_ref().unwrap(),
            app.native_mutable_diff_v2.as_ref().unwrap(),
            app.native_mutable_plan_v2.as_mut().unwrap(),
        )
        .unwrap();
    app
}
fn render(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| {
            assert!(draw(frame, app, frame.area()));
        })
        .unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .chunks(usize::from(width))
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}
fn key(app: &mut App, code: KeyCode) {
    assert!(capture(
        app,
        &Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    ));
}
#[test]
fn native_mutable_plan_actual_preview_geometry_keys_mouse_same_exact_purpose_and_closed_cancel() {
    for mouse in [false, true] {
        let real = Real::new("sha256");
        let mut app = ready(&real);
        let preview = match &app.native_mutable_plan_v2.as_ref().unwrap().phase {
            Phase::Preview(preview) => preview.clone(),
            _ => panic!("actual prepared preview"),
        };
        for width in [40, 80, 600] {
            let screen = render(&mut app, width, 24);
            assert!(
                screen.contains("Selected patch • confirm")
                    && screen.contains("[Enter Confirm]")
                    && screen.contains("Stage selected lines")
            );
            let plan = app.native_mutable_plan_v2.as_ref().unwrap();
            assert_eq!(plan.hits.len(), 6);
            assert!(plan.body.height >= 9);
            assert!(plan
                .hits
                .iter()
                .all(|(rect, _)| rect.right() < width.min(514) && rect.bottom() <= 23));
        }
        render(&mut app, 120, 40);
        real.clear_trace();
        let body = app.native_mutable_plan_v2.as_ref().unwrap().body;
        for (kind, expected) in [
            (MouseEventKind::ScrollRight, 8),
            (MouseEventKind::ScrollLeft, 0),
        ] {
            assert!(capture(
                &mut app,
                &Event::Mouse(MouseEvent {
                    kind,
                    column: body.x,
                    row: body.y,
                    modifiers: KeyModifiers::NONE
                })
            ));
            assert_eq!(
                app.native_mutable_plan_v2.as_ref().unwrap().horizontal,
                expected
            );
        }
        if mouse {
            let rect = app
                .native_mutable_plan_v2
                .as_ref()
                .unwrap()
                .hits
                .iter()
                .find(|(_, action)| *action == NativeMutablePlanActionV2::Confirm)
                .unwrap()
                .0;
            assert!(capture(
                &mut app,
                &Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: rect.x,
                    row: rect.y,
                    modifiers: KeyModifiers::NONE
                })
            ));
        } else {
            key(&mut app, KeyCode::Tab);
            key(&mut app, KeyCode::Tab);
            key(&mut app, KeyCode::Enter);
        }
        let work = app.original_diff_work_v1.take().unwrap();
        let OriginalDiffWorkV1::MutableConfirm(request) = work.as_ref() else {
            panic!("same closed explicit purpose")
        };
        assert!(Arc::ptr_eq(request.source(), preview.request().source()));
        assert!(Arc::ptr_eq(request.plan(), preview.plan()));
        assert!(
            real.children().is_empty(),
            "confirmation input does not execute or clone full source"
        );
        dispatch(&mut app, NativeMutablePlanActionV2::Close);
        assert!(app.original_diff_cancel_v1);
        assert!(app
            .native_mutable_diff_v2
            .as_ref()
            .unwrap()
            .current_frame()
            .is_none());
        let op =
            LocalGitNavigationReadV1::begin(request.source().intent().origin(), Default::default())
                .unwrap();
        assert!(request.prepare(&op).is_err());
        assert!(
            real.children().is_empty(),
            "closed queued purpose cannot be renewed even before binding"
        );
    }
}
#[test]
fn native_mutable_plan_tiny_hidden_preview_and_resize_epoch_cannot_confirm_by_key_mouse_or_action()
{
    let real = Real::new("sha1");
    let mut app = ready(&real);
    render(&mut app, 120, 40);
    let source = app
        .native_mutable_diff_v2
        .as_ref()
        .unwrap()
        .source()
        .unwrap()
        .clone();
    real.clear_trace();
    assert!(capture(&mut app, &Event::Resize(40, 8)));
    dispatch(&mut app, NativeMutablePlanActionV2::Confirm);
    assert!(
        app.original_diff_work_v1.is_none(),
        "input-time resize retires Confirm before draw"
    );
    let screen = render(&mut app, 40, 8);
    assert!(screen.contains("confirmation disabled"));
    assert!(!screen.contains("[Enter Confirm]"));
    assert!(app
        .native_mutable_plan_v2
        .as_ref()
        .unwrap()
        .hits
        .iter()
        .all(|(_, a)| matches!(
            a,
            NativeMutablePlanActionV2::Close
                | NativeMutablePlanActionV2::ExplainResize
                | NativeMutablePlanActionV2::Quit
        )));
    key(&mut app, KeyCode::Enter);
    assert!(
        app.overlay.is_none()
            && app
                .native_mutable_plan_v2
                .as_ref()
                .unwrap()
                .resize_explained,
        "disabled control explains inline within actual tiny geometry"
    );
    assert!(render(&mut app, 40, 8).contains("Confirm refused; resize to34x14"));
    assert!(capture(
        &mut app,
        &Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 16,
            row: 1,
            modifiers: KeyModifiers::NONE
        })
    ));
    dispatch(&mut app, NativeMutablePlanActionV2::Confirm);
    assert!(app.original_diff_work_v1.is_none());
    assert!(matches!(
        app.native_mutable_plan_v2.as_ref().unwrap().phase,
        Phase::Preview(_)
    ));
    assert!(real.children().is_empty());
    assert!(capture(&mut app, &Event::Resize(120, 40)));
    dispatch(&mut app, NativeMutablePlanActionV2::Confirm);
    assert!(app.original_diff_work_v1.is_none());
    render(&mut app, 120, 40);
    key(&mut app, KeyCode::Enter);
    assert!(
        matches!(app.original_diff_work_v1.as_deref(),Some(OriginalDiffWorkV1::MutableConfirm(request)) if Arc::ptr_eq(request.source(),&source))
    );
    assert!(real.children().is_empty());
}
#[test]
fn native_mutable_plan_preview_back_after_resize_reprepares_same_source_range_before_row_hits() {
    let real = Real::new("sha1");
    let mut app = ready(&real);
    let source = app
        .native_mutable_diff_v2
        .as_ref()
        .unwrap()
        .source()
        .unwrap()
        .clone();
    let changed = app
        .native_mutable_diff_v2
        .as_ref()
        .unwrap()
        .range()
        .unwrap()
        .changed()
        .to_vec();
    real.clear_trace();
    assert!(capture(&mut app, &Event::Resize(40, 24)));
    render(&mut app, 40, 24);
    dispatch(&mut app, NativeMutablePlanActionV2::Close);
    assert!(app.native_mutable_plan_v2.is_none());
    let state = app.native_mutable_diff_v2.as_ref().unwrap();
    assert_eq!(state.width, 38);
    assert!(state.current_frame().is_none() && state.pending().is_some());
    assert!(Arc::ptr_eq(state.source().unwrap(), &source));
    assert_eq!(state.range().unwrap().changed(), changed);
    assert!(
        real.children().is_empty(),
        "Back only queues bounded reflow"
    );
    let work = app.original_diff_work_v1.take().unwrap();
    let owner = OriginalDiffWorkerOwnerV1::begin(
        app.workspace.as_ref().unwrap(),
        None,
        work,
        Default::default(),
    )
    .unwrap();
    let prepared = owner.prepare().unwrap();
    prepared
        .publish_native_mutable_v2(
            app.workspace.as_ref().unwrap(),
            app.native_mutable_diff_v2.as_mut().unwrap(),
        )
        .unwrap();
    let state = app.native_mutable_diff_v2.as_ref().unwrap();
    assert_eq!(state.current_frame().unwrap().width(), 38);
    assert_eq!(state.range().unwrap().changed(), changed);
    assert!(Arc::ptr_eq(state.source().unwrap(), &source));
    assert_eq!(owner.accounting().unwrap().children_started(), 10);
}
