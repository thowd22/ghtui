//! UNRUN. Actual original factories and SAME controller/SQLite/draft ACK. Fake
//! gh is strict offline transport, not fabricated source/receipt authority.
use super::*;
use crate::{
    actions_write_journal::{ActionsReceiptKind, ActionsWriteJournal},
    forms::FormAction,
    providers::gh::*,
    scoped_actions_publication_test_support_v3 as w, scoped_actions_rerun_test_support_v4 as r,
    scoped_actions_test_support_v1 as f,
    test_support::{Fixture, GhStep},
};
use serde_json::{json, Value};

fn ack(command: ActionsCommand) -> GhStep {
    let (path, method, status) = match command {
        ActionsCommand::ForceCancel => ("/force-cancel", "POST", 202),
        ActionsCommand::DeleteRun => ("", "DELETE", 204),
        ActionsCommand::DeleteLogs => ("/logs", "DELETE", 204),
        _ => panic!("closed actual new control"),
    };
    GhStep::read(
        &[
            "api",
            &format!("repos/{}/actions/runs/123{path}", f::REPO),
            "--hostname",
            f::HOST,
            "--method",
            method,
            "--include",
        ],
        &format!("HTTP/2.0 {status} Accepted\nContent-Length: 0\r\n\r\n"),
    )
}
pub(in crate::scoped_actions_native_v1) fn confirm_phase(
    command: ActionsCommand,
    receipt: Option<Value>,
    uid: u64,
) -> Vec<GhStep> {
    let mut steps = w::confirm_phase(w::running(), false, uid);
    steps[11] = ack(command);
    if let Some(receipt) = receipt {
        steps[12].stdout = receipt.to_string().into_bytes();
    } else {
        steps[12].status = 1;
        steps[12].stdout.clear();
        steps[12].stderr = b"gh: Not Found (HTTP 404)".to_vec();
    }
    if command == ActionsCommand::DeleteRun {
        steps.insert(13, f::get(&format!("repos/{}", f::REPO), f::repo()));
    }
    assert_eq!(
        steps.len(),
        if command == ActionsCommand::DeleteRun {
            15
        } else {
            14
        }
    );
    steps
}
pub(in crate::scoped_actions_native_v1) fn setup(
    command: ActionsCommand,
    preview: bool,
    tail: Vec<GhStep>,
) -> (Fixture, GithubProvider, App, Vec<GhStep>) {
    let mut steps = w::read_phase(w::running(), 42);
    if preview {
        steps.extend(w::read_phase(w::running(), 42));
    }
    steps.extend(tail);
    let (fixture, provider, mut app, all) = w::setup(steps);
    r::open(&mut app, &provider, &fixture, command);
    if command == ActionsCommand::ForceCancel {
        app.form
            .as_mut()
            .unwrap()
            .dispatch(&FormAction::Choice(0, 1));
        w::paint(&mut app, 120, 40);
    }
    if preview {
        w::preview(&mut app, &provider, &fixture);
    }
    (fixture, provider, app, all)
}
pub(in crate::scoped_actions_native_v1) fn run_confirm(
    app: &mut App,
    provider: &GithubProvider,
    fixture: &Fixture,
) -> (
    PreparedResultV3,
    Arc<ScopedActionsOwnerV1>,
    crate::drafts::DraftWriter,
) {
    let work = w::request(app, FormAction::Submit);
    let writer = crate::drafts::DraftWriter::new(f::database(fixture));
    let owner =
        ScopedActionsOwnerV1::new_publication_v3(work, &f::database(fixture), &writer.client())
            .unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let result = execute(
        &owner,
        provider,
        &f::database(fixture),
        Some(&writer.client()),
    )
    .unwrap();
    (result, owner, writer)
}
fn attempt(result: &PreparedResultV3) -> &ActionsExecution {
    let PreparedResultV3::Publication(prepared) = result else {
        panic!("actual publication result")
    };
    let PublicationOutcomeV3::Attempt(result) = &prepared.outcome else {
        panic!("actual attempted result")
    };
    result.result_for_test_v8().0
}
pub(in crate::scoped_actions_native_v1) fn saved(
    fixture: &Fixture,
    provider: &GithubProvider,
    app: &App,
    reference: &crate::actions_write_journal::ActionsPublicationReference,
) -> crate::actions_write_journal::ActionsWriteMarker {
    ActionsWriteJournal::open(&f::database(fixture))
        .unwrap()
        .original_publication(provider, &app.actions.as_ref().unwrap().identity, reference)
        .unwrap()
}
pub(in crate::scoped_actions_native_v1) fn children(fixture: &Fixture) -> usize {
    std::fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .count()
}
#[test]
fn scoped_actions_control_v5_force_requires_explicit_actual_choice_before_any_preview_gh() {
    let (fixture, provider, mut app, all) = setup(ActionsCommand::ForceCancel, false, vec![]);
    app.form
        .as_mut()
        .unwrap()
        .dispatch(&FormAction::Choice(0, 0));
    w::paint(&mut app, 120, 40);
    let count = children(&fixture);
    let sql = f::sql_state(&fixture);
    let git = f::git_state(&fixture);
    let source = app.scoped_actions_v1.as_ref().unwrap().source.clone();
    app.form.as_mut().unwrap().dispatch(&FormAction::Preview);
    assert!(capture_form_work(&mut app));
    assert!(app.scoped_actions_work_v1.is_none());
    assert!(app
        .form
        .as_ref()
        .unwrap()
        .error
        .contains("Choose force_ack=true"));
    assert!(Arc::ptr_eq(
        &source,
        &app.scoped_actions_v1.as_ref().unwrap().source
    ));
    assert_eq!(app.overlay, Some(crate::app::Overlay::Form));
    assert_eq!(children(&fixture), count);
    assert_eq!(f::sql_state(&fixture), sql);
    assert_eq!(f::git_state(&fixture), git);
    assert!(w::result_sql(&fixture).is_empty());
    w::exact(&fixture, &all);
    let _ = provider;
}
#[test]
fn scoped_actions_control_v5_force202_same_actual_active_run_records_acceptance_not_completion() {
    let (fixture, provider, mut app, all) = setup(
        ActionsCommand::ForceCancel,
        true,
        confirm_phase(ActionsCommand::ForceCancel, Some(w::running()), 42),
    );
    let git = f::git_state(&fixture);
    let (result, _owner, _writer) = run_confirm(&mut app, &provider, &fixture);
    let ActionsExecution::AcceptedPending {
        publication,
        receipt,
        ..
    } = attempt(&result)
    else {
        panic!("exact actual force acceptance")
    };
    assert_eq!(receipt.kind, ActionsReceiptKind::RunTransition);
    let marker = saved(&fixture, &provider, &app, publication);
    assert_eq!(
        marker.kind,
        crate::actions_write_journal::ActionsWriteKind::ForceCancel
    );
    assert_eq!(
        marker.state,
        crate::actions_write_journal::ActionsWriteState::AcceptedPending
    );
    assert!(marker.context.force_escalation_v5.is_none());
    assert_eq!(f::git_state(&fixture), git);
    w::exact(&fixture, &all);
}
#[test]
fn scoped_actions_control_v5_force202_exact_cancelled_observation_is_distinct_from_acceptance() {
    let (fixture, provider, mut app, all) = setup(
        ActionsCommand::ForceCancel,
        true,
        confirm_phase(ActionsCommand::ForceCancel, Some(w::cancelled()), 42),
    );
    let (result, _owner, _writer) = run_confirm(&mut app, &provider, &fixture);
    let ActionsExecution::Confirmed { publication, .. } = attempt(&result) else {
        panic!("actual same-run cancelled observation")
    };
    assert_eq!(
        saved(&fixture, &provider, &app, publication).state,
        crate::actions_write_journal::ActionsWriteState::Confirmed
    );
    w::exact(&fixture, &all);
}
#[test]
fn scoped_actions_control_v5_logs204_never_claims_that_run_metadata_proves_log_deletion() {
    let (fixture, provider, mut app, all) = setup(
        ActionsCommand::DeleteLogs,
        true,
        confirm_phase(ActionsCommand::DeleteLogs, Some(w::running()), 42),
    );
    let git = f::git_state(&fixture);
    let (result, _owner, _writer) = run_confirm(&mut app, &provider, &fixture);
    let ActionsExecution::AcceptedPending {
        publication,
        receipt,
        ..
    } = attempt(&result)
    else {
        panic!("actual logs acceptance only")
    };
    assert_eq!(receipt.kind, ActionsReceiptKind::LogsDeleted);
    assert_eq!(
        saved(&fixture, &provider, &app, publication).state,
        crate::actions_write_journal::ActionsWriteState::AcceptedPending
    );
    assert_eq!(f::git_state(&fixture), git);
    w::exact(&fixture, &all);
}
#[test]
fn scoped_actions_control_v5_delete204_actual404_plus_fresh_repository_is_acceptance_not_deletion_proof(
) {
    let (fixture, provider, mut app, all) = setup(
        ActionsCommand::DeleteRun,
        true,
        confirm_phase(ActionsCommand::DeleteRun, None, 42),
    );
    let git = f::git_state(&fixture);
    let (result, _owner, _writer) = run_confirm(&mut app, &provider, &fixture);
    let ActionsExecution::AcceptedPending {
        publication,
        receipt,
        observation,
        ..
    } = attempt(&result)
    else {
        panic!("typed deletion acceptance")
    };
    assert_eq!(receipt.kind, ActionsReceiptKind::RunDeletionAccepted);
    assert!(matches!(
        observation,
        crate::actions_workspace::ActionsAcceptedObservation::RunDeletionAcceptance {
            availability: ScopedDeletedRunAvailabilityV5::Unavailable,
            run_id: 123,
            requested_attempt: 2,
            ..
        }
    ));
    assert_eq!(
        saved(&fixture, &provider, &app, publication).state,
        crate::actions_write_journal::ActionsWriteState::AcceptedPending
    );
    assert_eq!(f::git_state(&fixture), git);
    w::exact(&fixture, &all);
}
#[test]
fn scoped_actions_control_v5_delete204_actual_still_visible_run_is_honest_accepted_present() {
    let (fixture, provider, mut app, all) = setup(
        ActionsCommand::DeleteRun,
        true,
        confirm_phase(ActionsCommand::DeleteRun, Some(w::running()), 42),
    );
    let (result, _owner, _writer) = run_confirm(&mut app, &provider, &fixture);
    assert!(matches!(
        attempt(&result),
        ActionsExecution::AcceptedPending {
            observation:
                crate::actions_workspace::ActionsAcceptedObservation::RunDeletionAcceptance {
                    availability: ScopedDeletedRunAvailabilityV5::Present,
                    ..
                },
            ..
        }
    ));
    w::exact(&fixture, &all);
}
#[test]
fn scoped_actions_control_v5_delete403_is_unknown_and_never_consumes_a_fresh_repository_slot() {
    let mut tail = confirm_phase(ActionsCommand::DeleteRun, None, 42);
    tail.truncate(13);
    tail[12].stderr = b"gh: Resource not accessible (HTTP 403)".to_vec();
    let (fixture, provider, mut app, all) = setup(ActionsCommand::DeleteRun, true, tail);
    let (result, _owner, _writer) = run_confirm(&mut app, &provider, &fixture);
    let ActionsExecution::Unknown {
        publication,
        private_visible,
        ..
    } = attempt(&result)
    else {
        panic!("unknown postwrite outcome")
    };
    assert!(!private_visible);
    let marker = saved(&fixture, &provider, &app, publication);
    assert_eq!(
        marker.state,
        crate::actions_write_journal::ActionsWriteState::Uncertain
    );
    assert!(marker.receipt.is_none());
    w::exact(&fixture, &all);
}
#[test]
fn scoped_actions_control_v5_delete_current_repository_drift_never_publishes_private_acceptance() {
    let mut tail = confirm_phase(ActionsCommand::DeleteRun, None, 42);
    let mut other = f::repo();
    other["id"] = json!(10001);
    tail[13].stdout = other.to_string().into_bytes();
    tail.truncate(14);
    let (fixture, provider, mut app, all) = setup(ActionsCommand::DeleteRun, true, tail);
    let (result, _owner, _writer) = run_confirm(&mut app, &provider, &fixture);
    assert!(matches!(
        attempt(&result),
        ActionsExecution::Unknown {
            private_visible: false,
            ..
        }
    ));
    w::exact(&fixture, &all);
}
#[test]
fn scoped_actions_control_v5_force_explicit_escalation_keeps_old_cancel_accepted_and_new_exact_reference(
) {
    let mut tail = w::read_phase(w::running(), 42);
    tail.extend(w::read_phase(w::running(), 42));
    tail.extend(confirm_phase(
        ActionsCommand::ForceCancel,
        Some(w::running()),
        42,
    ));
    let (fixture, provider, mut app, all) = w::setup(tail);
    let state = app.scoped_actions_v1.as_ref().unwrap();
    let descriptor = crate::actions_workspace::ActionsPlan::prepare(
        app.actions.as_ref().unwrap(),
        ActionsCommand::Cancel,
        &Default::default(),
        &provider,
        &CancellationToken::default(),
    )
    .unwrap()
    .scoped_cancel_descriptor_v3(&provider, &state.source, state.page.run().unwrap())
    .unwrap();
    let mut journal = ActionsWriteJournal::open(&f::database(&fixture)).unwrap();
    let permit = journal
        .begin(&descriptor.target, &descriptor.intent)
        .unwrap();
    journal
        .claim(&permit, &descriptor.target, &descriptor.intent)
        .unwrap();
    let receipt = crate::actions_write_journal::ActionsReceipt {
        kind: ActionsReceiptKind::RunTransition,
        resource_id: Some(123),
        observed_attempt: Some(2),
        observed_run_id: None,
        original_dispatch_run_v7: None,
        workflow_remote_v9: None,
    };
    journal.mark_accepted_pending(&permit, &receipt).unwrap();
    let old = permit.marker().publication_reference();
    drop(journal);
    r::open(&mut app, &provider, &fixture, ActionsCommand::ForceCancel);
    assert!(app
        .form
        .as_ref()
        .unwrap()
        .plan_lines()
        .iter()
        .any(|line| line.contains("old Cancel remains unchanged")));
    app.form
        .as_mut()
        .unwrap()
        .dispatch(&FormAction::Choice(0, 1));
    w::paint(&mut app, 120, 40);
    w::preview(&mut app, &provider, &fixture);
    let (result, _owner, _writer) = run_confirm(&mut app, &provider, &fixture);
    let ActionsExecution::AcceptedPending { publication, .. } = attempt(&result) else {
        panic!("actual explicit escalation")
    };
    let new = saved(&fixture, &provider, &app, publication);
    assert_eq!(
        new.context.force_escalation_v5.as_ref().unwrap().cancel,
        old
    );
    let old_marker = saved(&fixture, &provider, &app, &old);
    assert_eq!(
        old_marker.state,
        crate::actions_write_journal::ActionsWriteState::AcceptedPending
    );
    assert_eq!(old_marker.receipt.as_ref(), Some(&receipt));
    assert_ne!(publication.attempt_id, old.attempt_id);
    w::exact(&fixture, &all);
}
#[test]
fn scoped_actions_control_v5_logs_f4_cannot_release_acceptance_but_explicit_f6_uses_same_guarded_commit(
) {
    let mut tail = confirm_phase(ActionsCommand::DeleteLogs, Some(w::running()), 42);
    tail.extend(w::read_phase(w::running(), 42));
    tail.extend(w::read_phase(w::running(), 42));
    let (fixture, provider, mut app, all) = setup(ActionsCommand::DeleteLogs, true, tail);
    let git = f::git_state(&fixture);
    let (result, owner, _writer) = run_confirm(&mut app, &provider, &fixture);
    let ActionsExecution::AcceptedPending { publication, .. } = attempt(&result) else {
        panic!("actual accepted logs")
    };
    let reference = publication.clone();
    assert!(publish(&mut app, &owner, result));
    app.scoped_actions_owner_v1 = None;
    w::paint(&mut app, 120, 40);
    let work = w::request(&mut app, FormAction::Submit);
    let owner = ScopedActionsOwnerV1::new(work).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let prepared = execute(&owner, &provider, &f::database(&fixture), None).unwrap();
    assert!(publish(&mut app, &owner, prepared));
    app.scoped_actions_owner_v1 = None;
    w::paint(&mut app, 120, 40);
    assert!(ui::acceptance_ready_v5(app.form.as_ref().unwrap()));
    assert!(!ui::acknowledgement_ready(app.form.as_ref().unwrap()));
    let count = children(&fixture);
    app.form
        .as_mut()
        .unwrap()
        .dispatch(&FormAction::AcknowledgeActionsV3);
    assert!(!capture_form_work(&mut app));
    assert_eq!(children(&fixture), count);
    let work = w::request(&mut app, FormAction::AcknowledgeActionsAcceptanceV5);
    let owner = ScopedActionsOwnerV1::new(work).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let prepared = execute(&owner, &provider, &f::database(&fixture), None).unwrap();
    assert!(publish(&mut app, &owner, prepared));
    assert_eq!(
        saved(&fixture, &provider, &app, &reference).state,
        crate::actions_write_journal::ActionsWriteState::Acknowledged
    );
    assert_eq!(f::git_state(&fixture), git);
    w::exact(&fixture, &all);
}
fn newer_form_is_untouched(command: ActionsCommand) {
    let (old_fixture, _provider, mut old_app, old_steps) = setup(command, true, vec![]);
    let work = w::request(&mut old_app, FormAction::Submit);
    let writer = crate::drafts::DraftWriter::new(f::database(&old_fixture));
    let old = ScopedActionsOwnerV1::new_publication_v3(
        work,
        &f::database(&old_fixture),
        &writer.client(),
    )
    .unwrap();
    let (fixture, _provider, mut app, all) = setup(command, false, vec![]);
    let new_work = w::request(&mut app, FormAction::Preview);
    app.scoped_actions_work_v1 = Some(new_work.clone());
    app.scoped_actions_owner_v1 = Some(old.clone());
    let source = app.scoped_actions_v1.as_ref().unwrap().source.clone();
    let page = app.scoped_actions_v1.as_ref().unwrap().page.clone();
    let epoch = app.scoped_actions_v1.as_ref().unwrap().epoch;
    let status = app.status.clone();
    let loading = app.loading;
    let hits = format!("{:?}", app.hits);
    let sql = f::sql_state(&fixture);
    let git = f::git_state(&fixture);
    let old_sql = f::sql_state(&old_fixture);
    let old_git = f::git_state(&old_fixture);
    assert_eq!(applicable_cancel_v3(&app), None);
    super::super::resize(&mut app);
    app.dispatch(Action::Form(FormAction::Close));
    assert!(Arc::ptr_eq(
        app.scoped_actions_owner_v1.as_ref().unwrap(),
        &old
    ));
    assert!(Arc::ptr_eq(
        app.scoped_actions_work_v1.as_ref().unwrap(),
        &new_work
    ));
    assert!(!old.cancellation().is_cancelled());
    assert!(!new_work.cancellation().is_cancelled());
    assert!(!app.scoped_actions_cancel_v1);
    let state = app.scoped_actions_v1.as_ref().unwrap();
    assert!(Arc::ptr_eq(&source, &state.source));
    assert!(Arc::ptr_eq(&page, &state.page));
    assert_eq!(epoch, state.epoch);
    assert_eq!(app.status, status);
    assert_eq!(app.loading, loading);
    assert_eq!(format!("{:?}", app.hits), hits);
    assert!(new_work
        .publication_v3
        .as_ref()
        .unwrap()
        .request()
        .unwrap()
        .matches_form(app.form.as_ref().unwrap()));
    assert_eq!(f::sql_state(&fixture), sql);
    assert_eq!(f::git_state(&fixture), git);
    assert_eq!(f::sql_state(&old_fixture), old_sql);
    assert_eq!(f::git_state(&old_fixture), old_git);
    w::exact(&fixture, &all);
    w::exact(&old_fixture, &old_steps);
}
#[test]
fn scoped_actions_control_v5_old_force_confirm_cannot_close_resize_genuinely_newer_form_source() {
    newer_form_is_untouched(ActionsCommand::ForceCancel);
}
#[test]
fn scoped_actions_control_v5_old_delete_run_confirm_cannot_close_resize_genuinely_newer_form_source(
) {
    newer_form_is_untouched(ActionsCommand::DeleteRun);
}
#[test]
fn scoped_actions_control_v5_old_delete_logs_confirm_cannot_close_resize_genuinely_newer_form_source(
) {
    newer_form_is_untouched(ActionsCommand::DeleteLogs);
}
#[test]
fn scoped_actions_control_v5_actual_force_choice_edit_invalidates_exact_preview_without_gh() {
    let (fixture, _provider, mut app, all) = setup(ActionsCommand::ForceCancel, true, vec![]);
    assert!(app.form.as_ref().unwrap().plan_ready());
    let count = children(&fixture);
    let sql = f::sql_state(&fixture);
    let git = f::git_state(&fixture);
    app.form
        .as_mut()
        .unwrap()
        .dispatch(&FormAction::Choice(0, 0));
    w::paint(&mut app, 120, 40);
    assert!(!app.form.as_ref().unwrap().plan_ready());
    app.form.as_mut().unwrap().dispatch(&FormAction::Submit);
    assert!(!capture_form_work(&mut app));
    assert!(app.scoped_actions_work_v1.is_none());
    assert_eq!(children(&fixture), count);
    assert_eq!(f::sql_state(&fixture), sql);
    assert_eq!(f::git_state(&fixture), git);
    w::exact(&fixture, &all);
}
#[test]
fn scoped_actions_control_v5_actual_all_three_tiny_previews_have_no_confirm_hit_or_hidden_approval()
{
    for command in [
        ActionsCommand::ForceCancel,
        ActionsCommand::DeleteRun,
        ActionsCommand::DeleteLogs,
    ] {
        let (fixture, _provider, mut app, all) = setup(command, true, vec![]);
        let count = children(&fixture);
        let sql = f::sql_state(&fixture);
        let git = f::git_state(&fixture);
        app.form
            .as_mut()
            .unwrap()
            .clear_preview_display_geometry_v1();
        let screen = w::paint(&mut app, 40, 10);
        assert!(screen.to_ascii_lowercase().contains("resize"));
        assert!(!app
            .hits
            .iter()
            .any(|hit| hit.action == Action::Form(FormAction::Submit)));
        app.form.as_mut().unwrap().dispatch(&FormAction::Submit);
        assert!(!capture_form_work(&mut app));
        assert_eq!(children(&fixture), count);
        assert_eq!(f::sql_state(&fixture), sql);
        assert_eq!(f::git_state(&fixture), git);
        w::exact(&fixture, &all);
    }
}
