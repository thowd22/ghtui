//! UNRUN genuine provider/Core, SQLite, installed-Git and exact Form history.
use super::*;
use crate::{
    actions_workspace::ActionsCommand,
    forms::FormAction,
    scoped_actions_native_v1::publication_v3::{self as publication, control_tests as c},
    scoped_actions_publication_test_support_v3 as w, scoped_actions_test_support_v1 as f,
    test_support::{Fixture, GhStep},
};
use serde_json::json;
#[path = "scoped_actions_history_cas_v6_tests.rs"]
mod cas_v6;
#[path = "scoped_actions_workflow_remote_history_v9_tests.rs"]
mod workflow_remote_v9;

fn phase(run: Option<serde_json::Value>, uid: u64) -> Vec<GhStep> {
    let mut out = f::principal(42);
    out.push(f::get(&format!("repos/{}", f::REPO), f::repo()));
    if let Some(run) = run {
        out.push(f::get(&format!("repos/{}/actions/runs/123", f::REPO), run));
    } else {
        let mut missing = f::get(&format!("repos/{}/actions/runs/123", f::REPO), json!({}));
        missing.status = 1;
        missing.stdout.clear();
        missing.stderr = b"gh: Not Found (HTTP 404)".to_vec();
        out.push(missing);
    }
    out.push(f::get(&format!("repos/{}", f::REPO), f::repo()));
    out.extend(f::principal(uid));
    assert_eq!(out.len(), 7);
    out
}
fn list_phase() -> Vec<GhStep> {
    let mut out = f::principal(42);
    out.extend((0..2).map(|_| f::get(&format!("repos/{}", f::REPO), f::repo())));
    out.extend(f::principal(42));
    assert_eq!(out.len(), 6);
    out
}
fn accepted(
    tail: Vec<GhStep>,
) -> (
    Fixture,
    GithubProvider,
    App,
    Vec<GhStep>,
    ActionsPublicationReference,
) {
    let mut steps = c::confirm_phase(ActionsCommand::DeleteRun, None, 42);
    steps.extend(tail);
    let (fixture, provider, mut app, all) = c::setup(ActionsCommand::DeleteRun, true, steps);
    let (result, owner, _writer) = c::run_confirm(&mut app, &provider, &fixture);
    let publication::PreparedResultV3::Publication(prepared) = &result else {
        panic!("actual original result")
    };
    let attempt = prepared
        .attempted_for_test_v107()
        .expect("actual attempted original result");
    let attempt = attempt.result_for_test_v8().0;
    let crate::actions_controller::ActionsExecution::AcceptedPending { publication, .. } = attempt
    else {
        panic!("actual delete acceptance")
    };
    let reference = publication.clone();
    assert!(publication::publish(&mut app, &owner, result));
    app.scoped_actions_owner_v1 = None;
    w::paint(&mut app, 120, 40);
    (fixture, provider, app, all, reference)
}
fn inspect(
    app: &mut App,
    provider: &GithubProvider,
    fixture: &Fixture,
) -> Arc<ScopedActionsWorkV1> {
    let work = w::request(app, FormAction::Submit);
    assert!(work.journal_v5.is_some());
    assert!(work.publication_v3.is_none());
    let owner = ScopedActionsOwnerV1::new(work.clone()).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let prepared = publication::execute(&owner, provider, &f::database(fixture), None).unwrap();
    assert!(publication::publish(app, &owner, prepared));
    app.scoped_actions_owner_v1 = None;
    work
}
fn next(app: &mut App, provider: &GithubProvider, fixture: &Fixture, action: ActionsAction) {
    assert!(intercept(app, &action));
    let work = super::super::take_work(app).expect("actual history work");
    let owner = ScopedActionsOwnerV1::new(work).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let prepared = publication::execute(&owner, provider, &f::database(fixture), None).unwrap();
    assert!(publication::publish(app, &owner, prepared));
    app.scoped_actions_owner_v1 = None;
}
#[test]
fn scoped_actions_control_v5_deleted_run_view_original_reads_saved_numeric_id_without_live_run_factory(
) {
    let (fixture, provider, mut app, all, reference) = accepted(phase(None, 42));
    let git = f::git_state(&fixture);
    let work = inspect(&mut app, &provider, &fixture);
    assert!(!work.cancellation().is_cancelled());
    assert!(app.form.is_none());
    assert!(app.overlay.is_none());
    assert_eq!(
        app.actions.as_ref().unwrap().view,
        ActionsView::PublicationInspection
    );
    let view = app
        .scoped_actions_v1
        .as_ref()
        .unwrap()
        .journal_v5
        .as_ref()
        .unwrap();
    assert_eq!(
        view.snapshot.selected().unwrap().publication_reference(),
        reference
    );
    assert!(view.acceptance());
    assert!(view.observed.as_ref().unwrap().run().is_none());
    assert!(app
        .actions
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .any(|row| row.label.contains("NOT proof of deletion")));
    assert_eq!(
        c::saved(&fixture, &provider, &app, &reference).state,
        ActionsWriteState::AcceptedPending
    );
    assert_eq!(f::git_state(&fixture), git);
    w::exact(&fixture, &all);
}
#[test]
fn scoped_actions_control_v5_deleted_history_ack_is_separate_fresh_no_write_and_releases_only_exact_accepted_marker(
) {
    let mut tail = phase(None, 42);
    tail.extend(phase(None, 42));
    let (fixture, provider, mut app, all, reference) = accepted(tail);
    let git = f::git_state(&fixture);
    inspect(&mut app, &provider, &fixture);
    let before = c::children(&fixture);
    next(
        &mut app,
        &provider,
        &fixture,
        ActionsAction::PublicationAcknowledgeAcceptance(reference.attempt_id),
    );
    assert_eq!(c::children(&fixture), before + 7);
    let marker = c::saved(&fixture, &provider, &app, &reference);
    assert_eq!(marker.state, ActionsWriteState::Acknowledged);
    assert_eq!(
        marker.receipt.as_ref().unwrap().kind,
        crate::actions_write_journal::ActionsReceiptKind::RunDeletionAccepted
    );
    assert!(!app
        .scoped_actions_v1
        .as_ref()
        .unwrap()
        .journal_v5
        .as_ref()
        .unwrap()
        .acceptance());
    let count = c::children(&fixture);
    let sql = f::sql_state(&fixture);
    assert!(intercept(
        &mut app,
        &ActionsAction::PublicationAcknowledgeAcceptance(reference.attempt_id)
    ));
    assert!(app.scoped_actions_work_v1.is_none());
    assert_eq!(c::children(&fixture), count);
    assert_eq!(f::sql_state(&fixture), sql);
    assert_eq!(f::git_state(&fixture), git);
    w::exact(&fixture, &all);
}
#[test]
fn scoped_actions_control_v5_deleted_history_changed_observation_does_not_ack_or_retarget_saved_heads(
) {
    let mut tail = phase(None, 42);
    let mut run = w::running();
    run["head_sha"] = json!("b".repeat(40));
    tail.extend(phase(Some(run), 42));
    let (fixture, provider, mut app, all, reference) = accepted(tail);
    inspect(&mut app, &provider, &fixture);
    let sql = f::sql_state(&fixture);
    let git = f::git_state(&fixture);
    assert!(intercept(
        &mut app,
        &ActionsAction::PublicationAcknowledgeAcceptance(reference.attempt_id)
    ));
    let work = super::super::take_work(&mut app).unwrap();
    let owner = ScopedActionsOwnerV1::new(work).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    assert!(publication::execute(&owner, &provider, &f::database(&fixture), None).is_err());
    assert_eq!(f::sql_state(&fixture), sql);
    assert_eq!(f::git_state(&fixture), git);
    assert_eq!(
        c::saved(&fixture, &provider, &app, &reference)
            .context
            .head_sha,
        Some("a".repeat(40))
    );
    // Failure happens before closing repository/final UID, not silent fallback.
    w::exact(&fixture, &all[..all.len() - 3]);
}
#[test]
fn scoped_actions_control_v5_deleted_history_final_uid_change_leaves_exact_original_receipt_protected(
) {
    let (fixture, provider, mut app, all, reference) = accepted(phase(None, 43));
    let work = w::request(&mut app, FormAction::Submit);
    let owner = ScopedActionsOwnerV1::new(work).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let sql = f::sql_state(&fixture);
    let git = f::git_state(&fixture);
    assert!(publication::execute(&owner, &provider, &f::database(&fixture), None).is_err());
    assert_eq!(f::sql_state(&fixture), sql);
    assert_eq!(f::git_state(&fixture), git);
    assert_eq!(
        c::saved(&fixture, &provider, &app, &reference).state,
        ActionsWriteState::AcceptedPending
    );
    w::exact(&fixture, &all);
}
#[test]
fn scoped_actions_control_v5_deleted_history_back_preserves_real_source_without_gh_sql_or_write_permission(
) {
    let (fixture, provider, mut app, all, _) = accepted(phase(None, 42));
    let source = app.scoped_actions_v1.as_ref().unwrap().source.clone();
    let page = app.scoped_actions_v1.as_ref().unwrap().page.clone();
    inspect(&mut app, &provider, &fixture);
    let sql = f::sql_state(&fixture);
    let git = f::git_state(&fixture);
    let count = c::children(&fixture);
    assert!(intercept(&mut app, &ActionsAction::Back));
    let state = app.scoped_actions_v1.as_ref().unwrap();
    assert!(state.journal_v5.is_none());
    assert!(Arc::ptr_eq(&source, &state.source));
    assert!(Arc::ptr_eq(&page, &state.page));
    assert_eq!(f::sql_state(&fixture), sql);
    assert_eq!(f::git_state(&fixture), git);
    assert_eq!(c::children(&fixture), count);
    w::exact(&fixture, &all);
}
#[test]
fn scoped_actions_control_v5_deleted_history_relaunch_lists_actual_protected_reference_not_saved_live_target(
) {
    let mut tail = phase(None, 42);
    tail.extend(list_phase());
    let (fixture, provider, mut app, all, reference) = accepted(tail);
    inspect(&mut app, &provider, &fixture);
    next(
        &mut app,
        &provider,
        &fixture,
        ActionsAction::PublicationList,
    );
    let view = app
        .scoped_actions_v1
        .as_ref()
        .unwrap()
        .journal_v5
        .as_ref()
        .unwrap();
    assert_eq!(view.reference(reference.attempt_id), Some(reference));
    assert!(view.snapshot.selected().is_none());
    assert_eq!(app.actions.as_ref().unwrap().publication_rows.len(), 1);
    let count = c::children(&fixture);
    let sql = f::sql_state(&fixture);
    assert!(intercept(
        &mut app,
        &ActionsAction::Compose(ActionsCommand::DeleteRun)
    ));
    assert!(app.scoped_actions_work_v1.is_none());
    assert_eq!(c::children(&fixture), count);
    assert_eq!(f::sql_state(&fixture), sql);
    w::exact(&fixture, &all);
}
#[test]
fn scoped_actions_control_v5_deleted_history_old_queue_failure_never_mutates_new_genuine_form_source(
) {
    let (fixture, provider, mut app, all, _) = accepted(vec![]);
    let old = w::request(&mut app, FormAction::Submit);
    let (fresh_fixture, _fresh_provider, mut fresh, fresh_steps) =
        c::setup(ActionsCommand::DeleteRun, false, vec![]);
    let new_work = w::request(&mut fresh, FormAction::Preview);
    fresh.scoped_actions_work_v1 = Some(new_work.clone());
    let source = fresh.scoped_actions_v1.as_ref().unwrap().source.clone();
    let epoch = fresh.scoped_actions_v1.as_ref().unwrap().epoch;
    let status = fresh.status.clone();
    let hits = format!("{:?}", fresh.hits);
    let sql = f::sql_state(&fresh_fixture);
    let count = c::children(&fresh_fixture);
    assert!(submission_failed(&mut fresh, &old));
    assert!(old.cancellation().is_cancelled());
    assert!(!new_work.cancellation().is_cancelled());
    assert!(Arc::ptr_eq(
        fresh.scoped_actions_work_v1.as_ref().unwrap(),
        &new_work
    ));
    assert!(Arc::ptr_eq(
        &fresh.scoped_actions_v1.as_ref().unwrap().source,
        &source
    ));
    assert_eq!(fresh.scoped_actions_v1.as_ref().unwrap().epoch, epoch);
    assert_eq!(fresh.status, status);
    assert_eq!(format!("{:?}", fresh.hits), hits);
    assert_eq!(f::sql_state(&fresh_fixture), sql);
    assert_eq!(c::children(&fresh_fixture), count);
    w::exact(&fixture, &all);
    w::exact(&fresh_fixture, &fresh_steps);
    let _ = provider;
}
#[test]
fn scoped_actions_control_v5_deleted_history_late_success_cannot_replace_genuine_new_form_or_clear_its_hits(
) {
    let (fixture, provider, mut app, all, _) = accepted(phase(None, 42));
    let work = w::request(&mut app, FormAction::Submit);
    let owner = ScopedActionsOwnerV1::new(work).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let result = publication::execute(&owner, &provider, &f::database(&fixture), None).unwrap();
    let (fresh_fixture, _fresh_provider, mut fresh, fresh_steps) =
        c::setup(ActionsCommand::DeleteLogs, false, vec![]);
    let source = fresh.scoped_actions_v1.as_ref().unwrap().source.clone();
    let form_title = fresh.form.as_ref().unwrap().title.clone();
    let status = fresh.status.clone();
    let hits = format!("{:?}", fresh.hits);
    let sql = f::sql_state(&fresh_fixture);
    fresh.scoped_actions_owner_v1 = Some(owner.clone());
    assert!(!publication::publish(&mut fresh, &owner, result));
    assert_eq!(fresh.form.as_ref().unwrap().title, form_title);
    assert!(Arc::ptr_eq(
        &fresh.scoped_actions_v1.as_ref().unwrap().source,
        &source
    ));
    assert_eq!(fresh.status, status);
    assert_eq!(format!("{:?}", fresh.hits), hits);
    assert_eq!(f::sql_state(&fresh_fixture), sql);
    w::exact(&fixture, &all);
    w::exact(&fresh_fixture, &fresh_steps);
}
#[test]
fn scoped_actions_control_v5_history_back_cannot_cancel_foreign_actual_form_owner_or_actions_slot()
{
    let (fixture, provider, mut app, all, _) = accepted(phase(None, 42));
    inspect(&mut app, &provider, &fixture);
    let (other, other_provider, mut fresh, other_steps) =
        c::setup(ActionsCommand::ForceCancel, false, vec![]);
    let foreign = w::request(&mut fresh, FormAction::Preview);
    let owner = ScopedActionsOwnerV1::new(foreign.clone()).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    app.loading[View::Actions.index()] = true;
    let source = app.scoped_actions_v1.as_ref().unwrap().source.clone();
    let epoch = app.scoped_actions_v1.as_ref().unwrap().epoch;
    let status = app.status.clone();
    let hits = format!("{:?}", app.hits);
    let sql = f::sql_state(&fixture);
    let other_sql = f::sql_state(&other);
    assert!(intercept(&mut app, &ActionsAction::Back));
    assert!(Arc::ptr_eq(
        app.scoped_actions_owner_v1.as_ref().unwrap(),
        &owner
    ));
    assert!(!foreign.cancellation().is_cancelled());
    assert!(app.loading[View::Actions.index()]);
    assert!(!app.scoped_actions_cancel_v1);
    assert!(Arc::ptr_eq(
        &source,
        &app.scoped_actions_v1.as_ref().unwrap().source
    ));
    assert_eq!(app.scoped_actions_v1.as_ref().unwrap().epoch, epoch);
    assert_eq!(app.status, status);
    assert_eq!(format!("{:?}", app.hits), hits);
    assert_eq!(f::sql_state(&fixture), sql);
    assert_eq!(f::sql_state(&other), other_sql);
    w::exact(&fixture, &all);
    w::exact(&other, &other_steps);
    let _ = other_provider;
}
