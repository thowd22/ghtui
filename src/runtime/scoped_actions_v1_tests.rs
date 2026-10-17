//! Actual shared request/completion channel; no invented success/source proof.
use super::*;
use crate::{
    actions_workspace::ActionsAction as A, scoped_actions_native_v1 as native,
    scoped_actions_test_support_v1 as f, scoped_actions_v1::ScopedActionsRequestKindV1 as K,
};
use std::{sync::mpsc, time::Duration};
#[path = "scoped_actions_physical_slot_v65_tests.rs"]
mod physical_slot_v65;
fn queued(
    tail: Vec<crate::test_support::GhStep>,
    fault: Option<fn()>,
) -> (
    crate::test_support::Fixture,
    crate::providers::gh::GithubProvider,
    App,
    Vec<crate::test_support::GhStep>,
    Background,
    CancellationToken,
) {
    let (fixture, p, mut app, all) = f::setup(tail);
    assert!(native::intercept(
        &mut app,
        &A::OpenRun {
            repository: f::REPO.into(),
            id: 123,
            attempt: 2
        }
    ));
    let mut work = native::take_work(&mut app).unwrap();
    Arc::get_mut(&mut work).unwrap().after_read_fault_v1 = fault;
    let token = work.cancellation().clone();
    let mut background = Background::new();
    assert!(background.submit_scoped_actions_v1(&mut app, work, p.clone(), f::database(&fixture)));
    (fixture, p, app, all, background, token)
}
fn actual(background: &mut Background) -> Completion {
    let completion = background
        .completions
        .recv_timeout(Duration::from_secs(8))
        .expect("Actual original worker completion");
    assert!(completion
        .remote_scope_owner
        .as_ref()
        .is_some_and(|owner| owner.actions_v1.is_some()));
    completion
}
fn put(background: &Background, completion: Completion) {
    background
        .inline_completion_sender_v1
        .send(completion)
        .unwrap();
}
#[test]
fn scoped_actions_runtime_same_slot_genuine_success_installs_closed_attempt_once() {
    let (fixture, _, mut app, all, mut background, _) = queued(f::operation(K::Attempt, 42), None);
    let completion = actual(&mut background);
    assert!(completion.result.is_ok());
    put(&background, completion);
    assert!(background.poll(&mut app));
    assert_eq!(
        app.actions
            .as_ref()
            .unwrap()
            .run
            .as_ref()
            .unwrap()
            .attempt
            .run
            .id,
        123
    );
    assert!(background.current_scoped_actions_v1.is_none());
    assert!(!app.loading[View::Actions.index()]);
    assert!(!background.poll(&mut app));
    f::exact(&fixture, &all);
}
#[test]
fn scoped_actions_runtime_queued_newer_input_vetoes_old_success_before_all_effects() {
    let (fixture, _, mut app, all, mut background, old_token) =
        queued(f::operation(K::Attempt, 42), None);
    let completion = actual(&mut background);
    assert!(completion.result.is_ok());
    let original_owner = completion
        .remote_scope_owner
        .as_ref()
        .unwrap()
        .actions_v1
        .as_ref()
        .unwrap();
    assert!(Arc::ptr_eq(
        background.current_scoped_actions_v1.as_ref().unwrap(),
        original_owner
    ));
    assert!(background
        .current
        .get(&View::Actions.index())
        .unwrap()
        .1
        .same_instance(&old_token));
    assert!(!old_token.is_cancelled());
    native::resize(&mut app);
    assert!(old_token.is_cancelled());
    assert!(app.scoped_actions_cancel_v1);
    assert!(app.loading[View::Actions.index()]);
    assert!(app.scoped_actions_owner_v1.is_none());
    // Match the actual main-loop owned cancellation drain before the next
    // input. Clearing only the mechanical flag leaves loading/slot ownership
    // live and correctly prevents a new request from being captured.
    app.scoped_actions_cancel_v1 = false;
    assert!(background.cancel_scoped_actions_v1(&mut app));
    background.retire_stale_scoped_actions_v1(&mut app);
    assert!(!app.loading[View::Actions.index()]);
    assert!(background.current_scoped_actions_v1.is_none());
    assert!(background.current_scoped_actions_generation_v1.is_none());
    assert!(!background.current.contains_key(&View::Actions.index()));
    assert!(native::intercept(
        &mut app,
        &A::OpenRun {
            repository: f::REPO.into(),
            id: 123,
            attempt: 2
        }
    ));
    let fresh = app.scoped_actions_work_v1.as_ref().unwrap().clone();
    let fresh_token = fresh.cancellation().clone();
    assert!(!fresh_token.same_instance(&old_token));
    assert!(!fresh_token.is_cancelled());
    assert!(native::current_input(&app, &fresh));
    let fresh_source = fresh.source.clone();
    let fresh_page = fresh.page.clone();
    let source = app.remote_scope_source.as_ref().unwrap().clone();
    let scope = app.remote_scope.as_ref().unwrap().clone();
    let rows = app.actions.as_ref().unwrap().rows.as_ptr();
    let selection = (
        app.actions.as_ref().unwrap().selected,
        app.actions.as_ref().unwrap().top,
        app.actions.as_ref().unwrap().view,
    );
    let status = app.status.clone();
    let context = app.context.clone();
    let loading = app.loading;
    let generation = background.generation;
    let slots = background
        .current
        .iter()
        .map(|(slot, (generation, _))| (*slot, *generation))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert!(
        slots.is_empty(),
        "The real owned drain removed the only physical slot; fresh input is queued, not submitted"
    );
    let sql = f::sql_state(&fixture);
    let git = f::git_state(&fixture);
    f::exact(&fixture, &all);
    put(&background, completion);
    assert!(!background.poll(&mut app));
    assert!(Arc::ptr_eq(
        app.scoped_actions_work_v1.as_ref().unwrap(),
        &fresh
    ));
    let current = app.scoped_actions_work_v1.as_ref().unwrap();
    assert!(Arc::ptr_eq(&current.source, &fresh_source));
    assert!(Arc::ptr_eq(&current.page, &fresh_page));
    assert!(current.cancellation().same_instance(&fresh_token));
    assert!(!fresh_token.is_cancelled());
    assert!(old_token.is_cancelled());
    assert!(native::current_input(&app, current));
    assert!(Arc::ptr_eq(
        app.remote_scope_source.as_ref().unwrap(),
        &source
    ));
    assert!(Arc::ptr_eq(app.remote_scope.as_ref().unwrap(), &scope));
    assert_eq!(app.actions.as_ref().unwrap().rows.as_ptr(), rows);
    assert_eq!(
        (
            app.actions.as_ref().unwrap().selected,
            app.actions.as_ref().unwrap().top,
            app.actions.as_ref().unwrap().view,
        ),
        selection
    );
    assert_eq!(app.status, status);
    assert_eq!(app.context, context);
    assert_eq!(app.loading, loading);
    assert_eq!(background.generation, generation);
    assert_eq!(
        background
            .current
            .iter()
            .map(|(slot, (generation, _))| (*slot, *generation))
            .collect::<std::collections::BTreeMap<_, _>>(),
        slots
    );
    assert!(!background.current.contains_key(&View::Actions.index()));
    assert!(background.current_scoped_actions_v1.is_none());
    assert!(background.current_scoped_actions_generation_v1.is_none());
    assert!(app.scoped_actions_owner_v1.is_none());
    assert!(app.actions.as_ref().unwrap().run.is_none());
    assert_eq!(f::sql_state(&fixture), sql);
    assert_eq!(f::git_state(&fixture), git);
    f::exact(&fixture, &all);
    // Retire the genuine queued successor after all late-result assertions;
    // it never acquired a physical slot or launched an additional child.
    native::resize(&mut app);
    assert!(fresh_token.is_cancelled());
    assert!(app.scoped_actions_work_v1.is_none());
    app.scoped_actions_cancel_v1 = false;
    assert!(!background.cancel_scoped_actions_v1(&mut app));
}
#[test]
fn scoped_actions_runtime_old_owner_never_retires_actual_newer_foreign_generation() {
    let (fixture, _, mut app, all, mut background, _) = queued(f::operation(K::Attempt, 42), None);
    let completion = actual(&mut background);
    let (started_tx, started_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    assert!(background.submit(&mut app, View::Actions, move |_| {
        started_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        Ok(PreparedView {
            status: "NEWER_FOREIGN_ACTIONS_GENERATION".into(),
            ..Default::default()
        })
    }));
    started_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    let generation = background.current.get(&View::Actions.index()).unwrap().0;
    let status = app.status.clone();
    put(&background, completion);
    assert!(!background.poll(&mut app));
    assert_eq!(
        background.current.get(&View::Actions.index()).unwrap().0,
        generation
    );
    assert_eq!(app.status, status);
    release_tx.send(()).unwrap();
    f::exact(&fixture, &all);
}
#[test]
fn scoped_actions_runtime_actual_error_after_scope_replacement_is_effect_free() {
    let mut op = f::operation(K::Attempt, 42);
    let mut run = f::run();
    run["run_attempt"] = serde_json::json!(3);
    op[3].stdout = run.to_string().into_bytes();
    let tail = op[..4].iter().chain(op[7..].iter()).cloned().collect();
    let (fixture, _, mut app, all, mut background, _) = queued(tail, None);
    let completion = actual(&mut background);
    assert!(completion.result.is_err());
    app.remote_scope = Some(Arc::new(
        app.remote_scope.as_ref().unwrap().as_ref().clone(),
    ));
    app.status = "NEWER_SCOPE_STATUS_CANARY".into();
    let sql = f::sql_state(&fixture);
    put(&background, completion);
    assert!(!background.poll(&mut app));
    assert_eq!(app.status, "NEWER_SCOPE_STATUS_CANARY");
    assert!(app.actions.is_some());
    assert_eq!(f::sql_state(&fixture), sql);
    f::exact(&fixture, &all);
    background.cancel_scoped_actions_v1(&mut app);
}
fn actual_panic() {
    panic!("ACTUAL_PRIVATE_ACTIONS_WORKER_PANIC");
}
#[test]
fn scoped_actions_runtime_actual_caught_worker_panic_cannot_clear_newer_view() {
    let mut tail = f::operation(K::Attempt, 42);
    tail.truncate(7);
    let (fixture, _, mut app, all, mut background, _) = queued(tail, Some(actual_panic));
    let completion = actual(&mut background);
    assert_eq!(
        completion.result.as_ref().err().map(String::as_str),
        Some("Background task failed unexpectedly; refresh to retry")
    );
    app.dispatch(crate::app::Action::View(View::Repository));
    let status = app.status.clone();
    let context = app.context.clone();
    let loading = app.loading;
    put(&background, completion);
    assert!(!background.poll(&mut app));
    assert_eq!(app.status, status);
    assert_eq!(app.context, context);
    assert_eq!(app.loading, loading);
    assert!(!app.status.contains("PRIVATE"));
    f::exact(&fixture, &all);
    background.cancel_scoped_actions_v1(&mut app);
}
#[test]
fn scoped_actions_runtime_mixed_actual_prepared_result_rejected_before_privacy_or_slot() {
    let (fixture, _, mut app, all, mut background, _) = queued(f::operation(K::Attempt, 42), None);
    let mut completion = actual(&mut background);
    let prepared = completion.result.as_mut().unwrap();
    prepared.clear_private = true;
    prepared.status = "FOREIGN_PRIVATE_STATUS".into();
    let generation = background.current.get(&View::Actions.index()).unwrap().0;
    let status = app.status.clone();
    put(&background, completion);
    assert!(!background.poll(&mut app));
    assert_eq!(
        background.current.get(&View::Actions.index()).unwrap().0,
        generation
    );
    assert_eq!(app.status, status);
    assert!(app.actions.is_some());
    f::exact(&fixture, &all);
    background.cancel_scoped_actions_v1(&mut app);
}
#[test]
fn scoped_actions_runtime_current_closing_numeric_uid_failure_hides_only_original_private_state() {
    let (fixture, _, mut app, all, mut background, _) = queued(f::operation(K::Attempt, 43), None);
    let sql = f::sql_state(&fixture);
    let git = f::git_state(&fixture);
    let completion = actual(&mut background);
    assert!(completion.result.is_err());
    put(&background, completion);
    assert!(background.poll(&mut app));
    assert!(app.actions.is_none());
    assert!(app.scoped_actions_v1.is_none());
    assert_eq!(f::sql_state(&fixture), sql);
    assert_eq!(f::git_state(&fixture), git);
    assert!(!app.status.contains("PRIVATE_EXACT"));
    f::exact(&fixture, &all);
}
#[test]
fn scoped_actions_runtime_cancel_retained_logical_token_after_slot_retirement_preserves_foreign_slot(
) {
    let (fixture, _, mut app, all, mut background, token) =
        queued(f::operation(K::Attempt, 42), None);
    let completion = actual(&mut background);
    assert!(completion.result.is_ok());
    // Emulate the documented physical-slot retirement boundary, keeping the
    // genuine completed producer and its original logical token.
    background.current.remove(&View::Actions.index());
    let (started_tx, started_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    assert!(background.submit(&mut app, View::Actions, move |_| {
        started_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        Ok(PreparedView::default())
    }));
    started_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    let generation = background.current.get(&View::Actions.index()).unwrap().0;
    assert!(native::intercept(&mut app, &A::Back));
    assert!(background.cancel_scoped_actions_v1(&mut app));
    assert!(token.is_cancelled());
    assert_eq!(
        background.current.get(&View::Actions.index()).unwrap().0,
        generation
    );
    assert!(app.actions.as_ref().unwrap().run.is_none());
    release_tx.send(()).unwrap();
    drop(completion);
    f::exact(&fixture, &all);
}
#[test]
fn scoped_actions_runtime_stale_canceled_expired_submit_preserves_genuine_live_foreign_job() {
    for refused in 0..3 {
        let (fixture, p, mut app, all) = f::setup(vec![]);
        assert!(native::intercept(
            &mut app,
            &A::OpenRun {
                repository: f::REPO.into(),
                id: 123,
                attempt: 2
            }
        ));
        let mut work = native::take_work(&mut app).unwrap();
        match refused {
            0 => {
                app.remote_scope = Some(Arc::new(
                    app.remote_scope.as_ref().unwrap().as_ref().clone(),
                ))
            }
            1 => work.cancellation().cancel(),
            _ => {
                Arc::get_mut(&mut work).unwrap().deadline =
                    std::time::Instant::now() - Duration::from_millis(1)
            }
        }
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let mut background = Background::new();
        assert!(background.submit(&mut app, View::Actions, move |_| {
            started_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
            Ok(PreparedView {
                status: "ACTUAL_NEWER_ACTIONS_JOB".into(),
                ..Default::default()
            })
        }));
        started_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        let (generation, token) = background
            .current
            .get(&View::Actions.index())
            .unwrap()
            .clone();
        let status = app.status.clone();
        let context = app.context.clone();
        let loading = app.loading;
        let source = app.remote_scope_source.as_ref().unwrap().clone();
        let scope = app.remote_scope.as_ref().unwrap().clone();
        let rows = app.actions.as_ref().unwrap().rows.as_ptr();
        let sql = f::sql_state(&fixture);
        let git = f::git_state(&fixture);
        assert!(!background.submit_scoped_actions_v1(&mut app, work, p, f::database(&fixture)));
        assert_eq!(
            background.current.get(&View::Actions.index()).unwrap().0,
            generation
        );
        assert!(!token.is_cancelled());
        assert_eq!(app.status, status);
        assert_eq!(app.context, context);
        assert_eq!(app.loading, loading);
        assert!(Arc::ptr_eq(
            app.remote_scope_source.as_ref().unwrap(),
            &source
        ));
        assert!(Arc::ptr_eq(app.remote_scope.as_ref().unwrap(), &scope));
        assert_eq!(app.actions.as_ref().unwrap().rows.as_ptr(), rows);
        assert!(app.scoped_actions_owner_v1.is_none());
        assert!(background.current_scoped_actions_v1.is_none());
        assert_eq!(f::sql_state(&fixture), sql);
        assert_eq!(f::git_state(&fixture), git);
        f::exact(&fixture, &all);
        release_tx.send(()).unwrap();
    }
}
