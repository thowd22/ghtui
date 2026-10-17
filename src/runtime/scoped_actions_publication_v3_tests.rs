//! Actual SAME queue/channel, source and generic held job. SOURCE ONLY/UNRUN.
use super::*;
use crate::{
    forms::FormAction, scoped_actions_publication_test_support_v3 as w,
    scoped_actions_test_support_v1 as f,
};
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};
fn actual(background: &Background) -> Completion {
    background
        .completions
        .recv_timeout(Duration::from_secs(8))
        .expect("actual worker completion")
}
fn put(background: &Background, value: Completion) {
    background.inline_completion_sender_v1.send(value).unwrap();
}
fn queued() -> (
    crate::test_support::Fixture,
    App,
    Vec<crate::test_support::GhStep>,
    Background,
) {
    queued_outcome(0)
}
fn actual_fault() {
    panic!("actual publication preparation fault after real source close");
}
fn queued_outcome(
    outcome: u8,
) -> (
    crate::test_support::Fixture,
    App,
    Vec<crate::test_support::GhStep>,
    Background,
) {
    let mut tail = w::read_phase(w::running(), 42);
    if outcome == 1 {
        tail[3].status = 1;
        tail[3].stderr = b"PRIVATE_ORIGINAL_READ_FAILURE".to_vec();
        tail.truncate(4);
    }
    let (fix, p, mut app, all) = w::setup(tail);
    let mut work = w::open_work(&mut app);
    if outcome == 2 {
        Arc::get_mut(&mut work).unwrap().after_read_fault_v1 = Some(actual_fault);
    }
    let mut background = Background::new();
    assert!(background.submit_scoped_actions_v1(&mut app, work, p, f::database(&fix)));
    (fix, app, all, background)
}
#[test]
fn scoped_actions_publication_v3_runtime_genuine_open_uses_same_actions_slot_and_one_typed_result()
{
    let (fix, mut app, all, mut background) = queued();
    let value = actual(&background);
    assert!(matches!(
        &**value
            .result
            .as_ref()
            .unwrap()
            .scoped_actions_v1
            .as_ref()
            .unwrap(),
        native::publication_v3::PreparedResultV3::Publication(_)
    ));
    assert!(background.current.contains_key(&View::Actions.index()));
    assert_eq!(background.current.len(), 1);
    put(&background, value);
    assert!(background.poll(&mut app));
    assert!(app.form.is_some());
    assert!(background.current.is_empty());
    assert!(!background.poll(&mut app));
    w::exact(&fix, &all);
}
#[test]
fn scoped_actions_publication_v3_runtime_stale_cancelled_expired_work_cannot_touch_actual_held_generic_job(
) {
    for mode in 0..3 {
        let (fix, p, mut app, all) = w::setup(vec![]);
        let mut work = w::open_work(&mut app);
        match mode {
            0 => native::resize(&mut app),
            1 => work.cancellation().cancel(),
            _ => {
                Arc::get_mut(&mut work).unwrap().deadline =
                    Instant::now() - Duration::from_millis(1)
            }
        }
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let mut background = Background::new();
        assert!(background.submit(&mut app, View::Actions, move |_| {
            entered_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
            Ok(PreparedView::default())
        }));
        entered_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        let (generation, live) = background.current[&View::Actions.index()].clone();
        let status = app.status.clone();
        let context = app.context.clone();
        let loading = app.loading;
        let rows = app.actions.as_ref().unwrap().rows.as_ptr();
        let source = app.remote_scope_source.as_ref().unwrap().clone();
        let sql = f::sql_state(&fix);
        let git = f::git_state(&fix);
        let drafts = crate::drafts::DraftWriter::new(f::database(&fix));
        assert!(!background.submit_scoped_actions_publication_v3(
            &mut app,
            work,
            p,
            f::database(&fix),
            drafts.client()
        ));
        assert_eq!(background.current[&View::Actions.index()].0, generation);
        assert!(!live.is_cancelled());
        assert_eq!(app.status, status);
        assert_eq!(app.context, context);
        assert_eq!(app.loading, loading);
        assert_eq!(app.actions.as_ref().unwrap().rows.as_ptr(), rows);
        assert!(Arc::ptr_eq(
            app.remote_scope_source.as_ref().unwrap(),
            &source
        ));
        assert!(app.scoped_actions_owner_v1.is_none());
        assert_eq!(f::sql_state(&fix), sql);
        assert_eq!(f::git_state(&fix), git);
        w::exact(&fix, &all);
        release_tx.send(()).unwrap();
    }
}
#[test]
fn scoped_actions_publication_v3_runtime_queued_success_error_and_caught_panic_cannot_touch_newer_overlay_or_generation(
) {
    for outcome in 0..3 {
        let (fix, mut app, all, mut background) = queued_outcome(outcome);
        let value = actual(&background);
        assert_eq!(value.result.is_ok(), outcome == 0);
        // Genuine new user event, not a fabricated source/result or frame.
        app.dispatch(crate::app::Action::Explain("ACTUAL_NEWER_OVERLAY".into()));
        let status = app.status.clone();
        let loading = app.loading;
        let generation = background.current[&View::Actions.index()].0;
        let source = app.remote_scope_source.as_ref().unwrap().clone();
        let sql = f::sql_state(&fix);
        put(&background, value);
        assert!(!background.poll(&mut app));
        assert_eq!(app.status, status);
        assert_eq!(app.loading, loading);
        assert_eq!(background.current[&View::Actions.index()].0, generation);
        assert!(Arc::ptr_eq(
            app.remote_scope_source.as_ref().unwrap(),
            &source
        ));
        assert!(app.form.is_none());
        assert_eq!(f::sql_state(&fix), sql);
        w::exact(&fix, &all);
    }
}
#[test]
fn scoped_actions_publication_v3_runtime_real_confirm_result_late_newer_form_never_replaces_private_newer_ui(
) {
    let mut tail = w::read_phase(w::running(), 42);
    tail.extend(w::read_phase(w::running(), 42));
    tail.extend(w::confirm_phase(w::running(), false, 42));
    let (fix, p, mut app, all) = w::setup(tail);
    w::open(&mut app, &p, &fix);
    w::preview(&mut app, &p, &fix);
    let work = w::request(&mut app, FormAction::Submit);
    let drafts = crate::drafts::DraftWriter::new(f::database(&fix));
    let mut background = Background::new();
    assert!(background.submit_scoped_actions_publication_v3(
        &mut app,
        work,
        p,
        f::database(&fix),
        drafts.client()
    ));
    let value = actual(&background);
    assert!(value.result.is_ok());
    assert_eq!(w::result_sql(&fix)[0].0, "accepted_pending");
    app.dispatch(crate::app::Action::Explain(
        "ACTUAL_NEWER_EXPLANATION".into(),
    ));
    let status = app.status.clone();
    let overlay = app.overlay;
    let generation = background.current[&View::Actions.index()].0;
    let rows = app.actions.as_ref().unwrap().rows.as_ptr();
    put(&background, value);
    assert!(!background.poll(&mut app));
    assert_eq!(app.overlay, overlay);
    assert_eq!(app.status, status);
    assert_eq!(background.current[&View::Actions.index()].0, generation);
    assert_eq!(app.actions.as_ref().unwrap().rows.as_ptr(), rows);
    assert_eq!(w::result_sql(&fix)[0].0, "accepted_pending");
    w::exact(&fix, &all);
}
#[test]
fn scoped_actions_publication_v3_runtime_mixed_form_result_rejected_before_slot_and_privacy_effects(
) {
    let (fix, mut app, all, mut background) = queued();
    let mut value = actual(&background);
    let prepared = value.result.as_mut().unwrap();
    prepared.clear_private = true;
    prepared.status = "FOREIGN_RESULT".into();
    let source = app.remote_scope_source.as_ref().unwrap().clone();
    let status = app.status.clone();
    let loading = app.loading;
    let generation = background.current[&View::Actions.index()].0;
    put(&background, value);
    assert!(!background.poll(&mut app));
    assert_eq!(app.status, status);
    assert_eq!(app.loading, loading);
    assert_eq!(background.current[&View::Actions.index()].0, generation);
    assert!(Arc::ptr_eq(
        app.remote_scope_source.as_ref().unwrap(),
        &source
    ));
    assert!(app.form.is_none());
    w::exact(&fix, &all);
}
#[test]
fn scoped_actions_publication_v3_runtime_resize_cancels_exact_preview_purpose_and_late_result_cannot_repaint(
) {
    let mut tail = w::read_phase(w::running(), 42);
    tail.extend(w::read_phase(w::running(), 42));
    let (fix, p, mut app, all) = w::setup(tail);
    w::open(&mut app, &p, &fix);
    let work = w::request(&mut app, FormAction::Preview);
    let token = work.cancellation().clone();
    let mut background = Background::new();
    assert!(background.submit_scoped_actions_v1(&mut app, work, p, f::database(&fix)));
    let value = actual(&background);
    assert!(value.result.is_ok());
    app.handle_event(crossterm::event::Event::Resize(80, 24));
    assert!(token.is_cancelled());
    assert!(app.scoped_actions_cancel_v1);
    let status = app.status.clone();
    let generation = background.current[&View::Actions.index()].0;
    put(&background, value);
    assert!(!background.poll(&mut app));
    assert_eq!(app.status, status);
    assert_eq!(background.current[&View::Actions.index()].0, generation);
    assert!(!app.form.as_ref().unwrap().plan_ready());
    assert!(w::result_sql(&fix).is_empty());
    w::exact(&fix, &all);
}
