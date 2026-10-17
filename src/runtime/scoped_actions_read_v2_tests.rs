//! SOURCE ONLY / UNRUN. Real worker channels and genuine captured originals.
use super::*;
use crate::{
    actions_workspace::ActionsAction as A,
    scoped_actions_native_v1::read_ui_v2::ReadUiActionV2 as U,
    scoped_actions_read_v2::LogSearchModeV2 as M,
    scoped_actions_resources_v2::ScopedActionsReadActionV2 as R,
    scoped_actions_test_support_v1 as f,
    scoped_actions_v1::{self as core, ScopedActionsRequestKindV1 as K},
};
use std::time::{Duration, Instant};
fn setup(
    tail: Vec<crate::test_support::GhStep>,
) -> (
    crate::test_support::Fixture,
    crate::providers::gh::GithubProvider,
    App,
    Vec<crate::test_support::GhStep>,
) {
    let mut op = f::operation(K::Logs, 42);
    op.extend(tail);
    let (fix, p, mut app, all) = f::opened(op);
    let value = f::read(&mut app, &p, &fix, A::Logs(Some(501))).unwrap();
    f::install(&mut app, value);
    (fix, p, app, all)
}
fn queue_search(app: &mut App, query: &str, mode: M) -> Arc<core::ScopedActionsWorkV1> {
    native::read_ui_v2::intercept(app, &A::ScopedReadV2(R::Ui(U::OpenSearch)));
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 24)).unwrap();
    terminal
        .draw(|frame| {
            let area = frame.area();
            native::read_ui_v2::draw(frame, app, area);
        })
        .unwrap();
    app.handle_event(crossterm::event::Event::Paste(query.into()));
    native::read_ui_v2::intercept(app, &A::ScopedReadV2(R::Ui(U::Mode(mode))));
    native::read_ui_v2::intercept(app, &A::ScopedReadV2(R::Ui(U::Apply)));
    native::take_work(app).expect("actual original editor captured Work")
}
fn watch_work(app: &mut App) -> Arc<core::ScopedActionsWorkV1> {
    native::read_ui_v2::intercept(app, &A::ScopedReadV2(R::Ui(U::Watch)));
    // Real quota/source, no fake observation/clock. Native cadence is exercised
    // separately by actual 5-second real-binary journeys.
    native::read_ui_v2::capture_watch_kernel_for_test_v2(app)
}
fn actual(background: &mut Background) -> Completion {
    background
        .completions
        .recv_timeout(Duration::from_secs(8))
        .expect("actual existing worker completion")
}
fn put(background: &Background, value: Completion) {
    background.inline_completion_sender_v1.send(value).unwrap();
}
fn submit(
    background: &mut Background,
    app: &mut App,
    work: Arc<core::ScopedActionsWorkV1>,
    p: &crate::providers::gh::GithubProvider,
    fix: &crate::test_support::Fixture,
) {
    assert!(background.submit_scoped_actions_v1(app, work, p.clone(), f::database(fix)));
}
#[test]
fn scoped_actions_read_v2_runtime_actual_search_uses_same_actions_slot_and_zero_gh() {
    let (fix, p, mut app, all) = setup(vec![]);
    let sql = f::sql_state(&fix);
    let work = queue_search(&mut app, "PRIVATE", M::ExactText);
    let token = work.cancellation().clone();
    let mut bg = Background::new();
    submit(&mut bg, &mut app, work, &p, &fix);
    let value = actual(&mut bg);
    assert_eq!(value.view, View::Actions);
    assert!(value
        .remote_scope_owner
        .as_ref()
        .unwrap()
        .actions_v1
        .is_some());
    assert!(
        matches!(&**value.result.as_ref().unwrap().scoped_actions_v1.as_ref().unwrap(),
        native::publication_v3::PreparedResultV3::Read(prepared) if prepared.matches_v2.is_some())
    );
    put(&bg, value);
    assert!(bg.poll(&mut app));
    assert!(!token.is_cancelled());
    assert!(app.status.contains("1 matching lines"));
    assert!(bg.current_scoped_actions_v1.is_none());
    assert_eq!(f::sql_state(&fix), sql);
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_read_v2_runtime_old_search_success_cannot_replace_newer_query_or_generation() {
    let (fix, p, mut app, all) = setup(vec![]);
    let work = queue_search(&mut app, "PRIVATE", M::ExactText);
    let mut bg = Background::new();
    submit(&mut bg, &mut app, work, &p, &fix);
    let value = actual(&mut bg);
    assert!(value.result.is_ok());
    native::resize(&mut app);
    bg.cancel_scoped_actions_v1(&mut app);
    app.scoped_actions_cancel_v1 = false;
    let fresh = queue_search(&mut app, "actual", M::ExactText);
    app.scoped_actions_work_v1 = Some(fresh.clone());
    let status = app.status.clone();
    let rows = app.actions.as_ref().unwrap().rows.as_ptr();
    put(&bg, value);
    assert!(!bg.poll(&mut app));
    assert!(Arc::ptr_eq(
        app.scoped_actions_work_v1.as_ref().unwrap(),
        &fresh
    ));
    assert_eq!(app.status, status);
    assert_eq!(app.actions.as_ref().unwrap().rows.as_ptr(), rows);
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_read_v2_runtime_real_regex_error_cannot_clear_newer_foreign_intent() {
    let (fix, p, mut app, all) = setup(vec![]);
    let work = queue_search(&mut app, "[", M::Regex);
    let mut bg = Background::new();
    submit(&mut bg, &mut app, work, &p, &fix);
    let value = actual(&mut bg);
    assert!(value.result.is_err());
    app.scoped_threads_cancel_v1 = true;
    app.status = "NEWER_THREAD_INTENT".into();
    let loading = app.loading;
    let generation = bg.generation;
    let rows = app.actions.as_ref().unwrap().rows.as_ptr();
    put(&bg, value);
    assert!(!bg.poll(&mut app));
    assert_eq!(app.status, "NEWER_THREAD_INTENT");
    assert_eq!(app.loading, loading);
    assert_eq!(bg.generation, generation);
    assert_eq!(app.actions.as_ref().unwrap().rows.as_ptr(), rows);
    bg.cancel_scoped_actions_v1(&mut app);
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_read_v2_runtime_real_regex_error_current_snapshot_is_not_privacy_erased() {
    let (fix, p, mut app, all) = setup(vec![]);
    let sql = f::sql_state(&fix);
    let rows = app.actions.as_ref().unwrap().rows.as_ptr();
    let work = queue_search(&mut app, "[", M::Regex);
    let mut bg = Background::new();
    submit(&mut bg, &mut app, work, &p, &fix);
    let value = actual(&mut bg);
    assert!(value.result.is_err());
    put(&bg, value);
    assert!(bg.poll(&mut app));
    assert_eq!(app.actions.as_ref().unwrap().rows.as_ptr(), rows);
    assert!(app.scoped_actions_v1.is_some());
    assert!(!app.loading[View::Actions.index()]);
    assert_eq!(f::sql_state(&fix), sql);
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_read_v2_runtime_mixed_prepared_facet_refuses_before_slot_or_highlights() {
    let (fix, p, mut app, all) = setup(vec![]);
    let work = queue_search(&mut app, "PRIVATE", M::ExactText);
    let mut bg = Background::new();
    submit(&mut bg, &mut app, work, &p, &fix);
    let mut value = actual(&mut bg);
    value.result.as_mut().unwrap().status = "FOREIGN_PRIVATE_PRESENTATION".into();
    let generation = bg.generation;
    let loading = app.loading;
    put(&bg, value);
    assert!(!bg.poll(&mut app));
    assert_eq!(bg.generation, generation);
    assert_eq!(app.loading, loading);
    assert!(!app.status.contains("FOREIGN_PRIVATE"));
    bg.cancel_scoped_actions_v1(&mut app);
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_read_v2_runtime_real_watch_group_keeps_original_token_and_finite_progress() {
    let (fix, p, mut app, all) = setup(f::operation(K::Logs, 42));
    let work = watch_work(&mut app);
    let deadline = work.deadline();
    let token = work.cancellation().clone();
    let mut bg = Background::new();
    submit(&mut bg, &mut app, work, &p, &fix);
    let value = actual(&mut bg);
    let native::publication_v3::PreparedResultV3::Read(prepared) = value
        .result
        .as_ref()
        .unwrap()
        .scoped_actions_v1
        .as_deref()
        .unwrap()
    else {
        panic!("A real watch read must not produce a publication result");
    };
    assert_eq!(prepared.watch_progress_v2.unwrap().0, 1);
    put(&bg, value);
    assert!(bg.poll(&mut app));
    assert!(!token.is_cancelled());
    assert!(app.actions.as_ref().unwrap().watching);
    assert!(Instant::now() < deadline);
    assert!(app.status.contains("tick 1/12"));
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_read_v2_runtime_stop_after_actual_queued_success_suppresses_all_publication() {
    let (fix, p, mut app, all) = setup(f::operation(K::Logs, 42));
    let work = watch_work(&mut app);
    let token = work.cancellation().clone();
    let mut bg = Background::new();
    submit(&mut bg, &mut app, work, &p, &fix);
    let value = actual(&mut bg);
    assert!(value.result.is_ok());
    native::intercept(&mut app, &A::Watch);
    let status = app.status.clone();
    let rows = app.actions.as_ref().unwrap().rows.as_ptr();
    assert!(token.is_cancelled());
    put(&bg, value);
    assert!(!bg.poll(&mut app));
    assert_eq!(app.status, status);
    assert_eq!(app.actions.as_ref().unwrap().rows.as_ptr(), rows);
    bg.cancel_scoped_actions_v1(&mut app);
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_read_v2_runtime_actual_watch_uid_error_cannot_clear_replaced_source() {
    let (fix, p, mut app, all) = setup(f::operation(K::Logs, 43));
    let work = watch_work(&mut app);
    let mut bg = Background::new();
    submit(&mut bg, &mut app, work, &p, &fix);
    let value = actual(&mut bg);
    assert!(value.result.is_err());
    app.remote_scope = None;
    app.status = "NEWER_SOURCE_CANARY".into();
    let sql = f::sql_state(&fix);
    let rows = app.actions.as_ref().unwrap().rows.as_ptr();
    put(&bg, value);
    assert!(!bg.poll(&mut app));
    assert_eq!(app.status, "NEWER_SOURCE_CANARY");
    assert_eq!(app.actions.as_ref().unwrap().rows.as_ptr(), rows);
    assert_eq!(f::sql_state(&fix), sql);
    bg.cancel_scoped_actions_v1(&mut app);
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_read_v2_runtime_current_failed_watch_quota_still_admits_exact_privacy_failure() {
    let (fix, p, mut app, all) = setup(f::operation(K::Logs, 43));
    let work = watch_work(&mut app);
    let mut bg = Background::new();
    submit(&mut bg, &mut app, work, &p, &fix);
    let value = actual(&mut bg);
    assert!(value.result.is_err());
    let sql = f::sql_state(&fix);
    let git = f::git_state(&fix);
    put(&bg, value);
    assert!(bg.poll(&mut app));
    assert!(app.actions.is_none());
    assert!(app.scoped_actions_v1.is_none());
    assert!(bg.current_scoped_actions_v1.is_none());
    assert_eq!(f::sql_state(&fix), sql);
    assert_eq!(f::git_state(&fix), git);
    f::exact(&fix, &all);
}
fn panic_after_real_read() {
    panic!("ACTUAL_PRIVATE_WATCH_PANIC");
}
#[test]
fn scoped_actions_read_v2_runtime_actual_caught_watch_panic_cannot_clear_newer_view() {
    let mut tail = f::operation(K::Logs, 42);
    tail.truncate(10);
    let (fix, p, mut app, all) = setup(tail);
    let mut work = watch_work(&mut app);
    Arc::get_mut(&mut work).unwrap().after_read_fault_v1 = Some(panic_after_real_read);
    let mut bg = Background::new();
    submit(&mut bg, &mut app, work, &p, &fix);
    let value = actual(&mut bg);
    assert!(value.result.is_err());
    app.view = View::Repository;
    app.status = "NEWER_LOCAL_VIEW".into();
    let loading = app.loading;
    put(&bg, value);
    assert!(!bg.poll(&mut app));
    assert_eq!(app.status, "NEWER_LOCAL_VIEW");
    assert_eq!(app.loading, loading);
    bg.cancel_scoped_actions_v1(&mut app);
    f::exact(&fix, &all);
}
#[test]
fn scoped_actions_read_v2_runtime_stopped_watch_submission_never_cancels_live_foreign_slot() {
    let (fix, p, mut app, all) = setup(vec![]);
    let work = watch_work(&mut app);
    native::intercept(&mut app, &A::Watch);
    let mut bg = Background::new();
    let (started_tx, started_rx) = std::sync::mpsc::sync_channel(1);
    let (release_tx, release_rx) = std::sync::mpsc::sync_channel(1);
    assert!(bg.submit(&mut app, View::Actions, move |_| {
        started_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        Ok(PreparedView::default())
    }));
    started_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    let (generation, token) = bg.current.get(&View::Actions.index()).unwrap().clone();
    let status = app.status.clone();
    let loading = app.loading;
    assert!(!bg.submit_scoped_actions_v1(&mut app, work, p, f::database(&fix)));
    assert!(!token.is_cancelled());
    assert_eq!(
        bg.current.get(&View::Actions.index()).unwrap().0,
        generation
    );
    assert_eq!(app.status, status);
    assert_eq!(app.loading, loading);
    release_tx.send(()).unwrap();
    f::exact(&fix, &all);
}
