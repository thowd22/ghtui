use super::*;
use crate::{
    scoped_actions_test_support_v1 as f, scoped_actions_v1::ScopedActionsRequestKindV1 as K,
};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{backend::TestBackend, Terminal};
fn render(app: &mut App, width: u16, height: u16) -> crate::actions_ui::ActionsRender {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let mut result = None;
    let theme = app.theme();
    let avatars = Default::default();
    terminal
        .draw(|frame| {
            let area = frame.area();
            result = Some(crate::actions_ui::draw(
                frame,
                app.actions.as_mut().unwrap(),
                area,
                theme,
                &avatars,
            ));
        })
        .unwrap();
    result.unwrap()
}
#[test]
fn scoped_actions_native_real_view_retirement_refuses_input_until_empty_runtime_cancel_is_drained()
{
    let (fixture, _, mut app, all) = f::setup(vec![]);
    let sql = f::sql_state(&fixture);
    let git = f::git_state(&fixture);
    let source = app.remote_scope_source.as_ref().unwrap().clone();
    assert!(!app.pr_check_continuation_cancel_v2);
    app.dispatch(Action::View(View::Actions));
    assert!(app.pr_check_continuation_cancel_v2);
    assert!(app.pr_check_watch_v2.is_none());
    assert!(app.pr_check_continuation_request_v2.is_none());
    assert!(app.pr_check_continuation_owner_v2.is_none());
    assert!(intercept(
        &mut app,
        &ActionsAction::OpenRun {
            repository: f::REPO.into(),
            id: 123,
            attempt: 2,
        },
    ));
    let work = take_work(&mut app).unwrap();
    assert!(!current_input(&app, &work));
    assert!(!work.cancellation().is_cancelled());
    // This is the same signal consumption as the real event loop. There is
    // no previous physical/logical continuation to cancel in this fixture.
    app.pr_check_continuation_cancel_v2 = false;
    assert!(current_input(&app, &work));
    assert!(!work.cancellation().is_cancelled());
    assert!(Arc::ptr_eq(
        app.remote_scope_source.as_ref().unwrap(),
        &source
    ));
    assert_eq!(f::sql_state(&fixture), sql);
    assert_eq!(f::git_state(&fixture), git);
    f::exact(&fixture, &all);
}
#[test]
fn scoped_actions_native_key_and_actual_hit_open_identical_observed_target() {
    let (fixture, _, mut app, all) = f::setup(vec![]);
    let keyboard = crate::actions_ui::keyboard_action(
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        app.actions.as_ref().unwrap(),
    )
    .unwrap();
    let hits = render(&mut app, 120, 40);
    let click = hits
        .hits
        .iter()
        .find(|hit| {
            matches!(
                hit.action,
                ActionsAction::OpenRun {
                    id: 123,
                    attempt: 2,
                    ..
                }
            )
        })
        .unwrap()
        .action
        .clone();
    assert_eq!(keyboard, click);
    assert!(intercept(&mut app, &click));
    assert_eq!(
        app.scoped_actions_work_v1
            .as_ref()
            .unwrap()
            .run
            .as_ref()
            .unwrap()
            .run()
            .id,
        123
    );
    f::exact(&fixture, &all);
}
#[test]
fn scoped_actions_native_disabled_keys_and_hits_cannot_open_forms_or_write_drafts() {
    for mouse in [false, true] {
        let (fixture, _, mut app, all) = f::setup(vec![]);
        let sql = f::sql_state(&fixture);
        let action = if mouse {
            let mut found = None;
            for page in 0..64 {
                app.actions.as_mut().unwrap().controls_page = page;
                let hits = render(&mut app, 80, 24);
                found = hits.hits.into_iter().find_map(|hit| match hit.action {
                    ActionsAction::Unavailable(reason)
                        if reason.contains("no legacy/name fallback") =>
                    {
                        Some(ActionsAction::Unavailable(reason))
                    }
                    _ => None,
                });
                if found.is_some() {
                    break;
                }
            }
            found.expect("Actual disabled source-origin control hit")
        } else {
            crate::actions_ui::keyboard_action(
                KeyEvent::new(KeyCode::Char('W'), KeyModifiers::NONE),
                app.actions.as_ref().unwrap(),
            )
            .unwrap()
        };
        assert!(intercept(&mut app, &action));
        assert!(app.form.is_none());
        assert!(app.scoped_actions_work_v1.is_none());
        assert_eq!(f::sql_state(&fixture), sql);
        f::exact(&fixture, &all);
    }
}
#[test]
fn scoped_actions_native_pure_selection_keeps_actual_row_allocations_without_rebuild() {
    let (fixture, _, mut app, all) = f::setup(vec![]);
    let pointer = app.actions.as_ref().unwrap().rows.as_ptr();
    let body = app.actions.as_ref().unwrap().rows[0].label.as_ptr();
    assert!(intercept(&mut app, &ActionsAction::Select(0)));
    assert_eq!(app.actions.as_ref().unwrap().rows.as_ptr(), pointer);
    assert_eq!(app.actions.as_ref().unwrap().rows[0].label.as_ptr(), body);
    assert!(app.scoped_actions_work_v1.is_none());
    f::exact(&fixture, &all);
}
#[test]
fn scoped_actions_native_back_moves_original_workspace_and_restores_exact_viewport() {
    let (fixture, p, mut app, all) = f::setup(f::operation(K::Attempt, 42));
    app.actions.as_mut().unwrap().controls_page = 1;
    let pointer = app.actions.as_ref().unwrap().rows[0].label.as_ptr();
    let value = f::read(
        &mut app,
        &p,
        &fixture,
        ActionsAction::OpenRun {
            repository: f::REPO.into(),
            id: 123,
            attempt: 2,
        },
    )
    .unwrap();
    f::install(&mut app, value);
    assert!(intercept(&mut app, &ActionsAction::Back));
    assert_eq!(app.actions.as_ref().unwrap().view, ActionsView::Runs);
    assert_eq!(app.actions.as_ref().unwrap().controls_page, 1);
    assert_eq!(
        app.actions.as_ref().unwrap().rows[0].label.as_ptr(),
        pointer
    );
    f::exact(&fixture, &all);
}
#[test]
fn scoped_actions_native_resize_cancels_exact_queued_work_and_retains_original_page() {
    let (fixture, _, mut app, all) = f::setup(vec![]);
    assert!(intercept(
        &mut app,
        &ActionsAction::OpenRun {
            repository: f::REPO.into(),
            id: 123,
            attempt: 2
        }
    ));
    let work = app.scoped_actions_work_v1.as_ref().unwrap().clone();
    app.handle_event(Event::Resize(80, 24));
    assert!(work.cancellation().is_cancelled());
    assert!(app.scoped_actions_work_v1.is_none());
    assert_eq!(app.actions.as_ref().unwrap().view, ActionsView::Runs);
    f::exact(&fixture, &all);
}
#[test]
fn scoped_actions_native_cancel_read_is_same_back_semantics_no_api_or_storage() {
    let (fixture, _, mut app, all) = f::setup(vec![]);
    let sql = f::sql_state(&fixture);
    let git = f::git_state(&fixture);
    assert!(intercept(
        &mut app,
        &ActionsAction::OpenRun {
            repository: f::REPO.into(),
            id: 123,
            attempt: 2
        }
    ));
    let token = app
        .scoped_actions_work_v1
        .as_ref()
        .unwrap()
        .cancellation()
        .clone();
    assert!(app
        .actions
        .as_ref()
        .unwrap()
        .controls
        .iter()
        .any(|control| control.label.contains("Cancel read")));
    assert!(intercept(&mut app, &ActionsAction::Back));
    assert!(token.is_cancelled());
    assert!(app.scoped_actions_work_v1.is_none());
    assert!(app.form.is_none());
    assert_eq!(f::sql_state(&fixture), sql);
    assert_eq!(f::git_state(&fixture), git);
    f::exact(&fixture, &all);
}
#[test]
fn scoped_actions_native_original_source_scope_replacement_retires_pending_before_effects() {
    let (fixture, _, mut app, all) = f::setup(vec![]);
    assert!(intercept(
        &mut app,
        &ActionsAction::OpenRun {
            repository: f::REPO.into(),
            id: 123,
            attempt: 2
        }
    ));
    let token = app
        .scoped_actions_work_v1
        .as_ref()
        .unwrap()
        .cancellation()
        .clone();
    app.remote_scope = Some(Arc::new(
        app.remote_scope.as_ref().unwrap().as_ref().clone(),
    ));
    synchronize(&mut app);
    assert!(app.scoped_actions_v1.is_none());
    assert!(token.is_cancelled());
    assert!(app.scoped_actions_work_v1.is_none());
    f::exact(&fixture, &all);
}
#[test]
fn scoped_actions_native_actor_failure_color_and_paged_controls_survive_narrow_resize() {
    let (fixture, _, mut app, all) = f::setup(vec![]);
    assert_eq!(
        app.actions.as_ref().unwrap().rows[0]
            .actor
            .as_ref()
            .unwrap()
            .login,
        "actual-ci-actor"
    );
    assert_eq!(
        app.actions.as_ref().unwrap().rows[0].role,
        crate::theme::SemanticToken::Failure
    );
    for (width, height) in [(80, 24), (120, 40), (80, 24)] {
        let output = render(&mut app, width, height);
        assert!(output.visible_rows > 0);
        assert!(output
            .hits
            .iter()
            .all(|hit| hit.rect.right() <= width && hit.rect.bottom() <= height));
        assert!(output
            .hits
            .iter()
            .any(|hit| matches!(hit.action, ActionsAction::Refresh)));
    }
    f::exact(&fixture, &all);
}
