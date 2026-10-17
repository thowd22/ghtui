//! UNRUN declarations. Actual closed installed-Git/Core/run factories and real
//! private SQLite. Persisted negative metadata is NOT source/write authority.
use super::*;
use crate::{
    actions_workspace::{ActionsCommand, ActionsPlan, ActionsPublicationDescriptor},
    scoped_actions_publication_test_support_v3 as w, scoped_actions_test_support_v1 as f,
};

fn actual() -> (
    crate::test_support::Fixture,
    GithubProvider,
    crate::app::App,
    Vec<crate::test_support::GhStep>,
    ActionsPublicationDescriptor,
    ActionsPublicationDescriptor,
) {
    let (fixture, provider, app, all) = w::setup(vec![]);
    let state = app.scoped_actions_v1.as_ref().unwrap();
    let run = state.page_for_test_v107().run().unwrap();
    let descriptor = |command, action| {
        ActionsPlan::prepare(
            app.actions.as_ref().unwrap(),
            command,
            &Default::default(),
            &provider,
            &CancellationToken::default(),
        )
        .unwrap()
        .scoped_run_descriptor_v4(&provider, state.source_for_test_v107(), run, None, action)
        .unwrap()
    };
    let cancel = descriptor(ActionsCommand::Cancel, RunMutation::Cancel);
    let force = descriptor(ActionsCommand::ForceCancel, RunMutation::ForceCancel);
    (fixture, provider, app, all, cancel, force)
}
fn accepted(
    journal: &mut ActionsWriteJournal,
    cancel: &ActionsPublicationDescriptor,
) -> ActionsPublicationReference {
    let permit = journal.begin(&cancel.target, &cancel.intent).unwrap();
    journal
        .claim(&permit, &cancel.target, &cancel.intent)
        .unwrap();
    journal
        .mark_accepted_pending(
            &permit,
            &ActionsReceipt {
                kind: ActionsReceiptKind::RunTransition,
                resource_id: Some(123),
                observed_attempt: Some(2),
                observed_run_id: None,
                original_dispatch_run_v7: None,
                workflow_remote_v9: None,
            },
        )
        .unwrap();
    permit.marker().publication_reference()
}
fn proof(
    journal: &ActionsWriteJournal,
    provider: &GithubProvider,
    app: &crate::app::App,
    force: &ActionsPublicationDescriptor,
) -> Result<Option<Arc<ScopedForceEscalationV5>>> {
    let state = app.scoped_actions_v1.as_ref().unwrap();
    journal.prepare_force_escalation_v5(
        provider,
        state.source_for_test_v107(),
        state.page_for_test_v107().run().unwrap(),
        &force.target,
        &force.intent,
    )
}
fn attach(force: &mut ActionsPublicationDescriptor, proof: &ScopedForceEscalationV5) {
    force.intent.context.force_escalation_v5 = Some(proof.metadata());
    force.target = ActionsWriteTarget::from_original_context_v3(
        force.target.scope(),
        force.target.target(),
        &force.intent.context,
    )
    .unwrap();
}
fn sql(journal: &ActionsWriteJournal) -> (i64, i64, i64) {
    journal.connection.query_row("SELECT (SELECT COUNT(*) FROM actions_write_attempts),COALESCE((SELECT SUM(generation) FROM actions_write_generations),0),COALESCE((SELECT SUM(claimed) FROM actions_write_attempts),0)", [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).unwrap()
}
#[test]
fn scoped_actions_control_v5_actual_accepted_cancel_escalates_once_without_rewriting_old_result() {
    let (fixture, provider, app, all, cancel, mut force) = actual();
    let git = f::git_state(&fixture);
    let mut journal = ActionsWriteJournal::open(&f::database(&fixture)).unwrap();
    let reference = accepted(&mut journal, &cancel);
    let original_row = |journal: &ActionsWriteJournal| {
        journal.connection.query_row(
            "SELECT json_array(attempt_id,host,principal,target_json,resources_json,kind_json,context_json,options_json,fingerprint_sha256,state,receipt_json,claimed,created_at) FROM actions_write_attempts WHERE attempt_id=?1",
            [reference.attempt_id], |row| row.get::<_, String>(0),
        ).unwrap()
    };
    let original_raw = original_row(&journal);
    let original = journal
        .restored_publication(&cancel.target, &reference)
        .unwrap();
    let retained = proof(&journal, &provider, &app, &force).unwrap().unwrap();
    assert!(retained.matches(
        app.scoped_actions_v1
            .as_ref()
            .unwrap()
            .source_for_test_v107(),
        app.scoped_actions_v1
            .as_ref()
            .unwrap()
            .page_for_test_v107()
            .run()
            .unwrap()
    ));
    attach(&mut force, &retained);
    let permit = journal
        .begin_scoped_force_v5(&force.target, &force.intent, retained.clone())
        .unwrap();
    assert_eq!(sql(&journal).0, 2);
    journal
        .claim(&permit, &force.target, &force.intent)
        .unwrap();
    let stamp = journal.capture_claim_stamp_v3(&permit).unwrap();
    journal.validate_claim_stamp_v3(&stamp).unwrap();
    // Both claim AND finish hold their existing Immediate transaction. The
    // old Cancel is checked inside that same snapshot, never a nested BEGIN.
    let receipt = ActionsReceipt {
        kind: ActionsReceiptKind::RunTransition,
        resource_id: Some(123),
        observed_attempt: Some(2),
        observed_run_id: None,
        original_dispatch_run_v7: None,
        workflow_remote_v9: None,
    };
    journal.mark_accepted_pending(&permit, &receipt).unwrap();
    journal
        .validate_result_stamp_v3(&stamp, ActionsWriteState::AcceptedPending, Some(&receipt))
        .unwrap();
    assert_eq!(
        original_row(&journal),
        original_raw,
        "Every raw old Cancel column remains unchanged through Force claim/finish"
    );
    let before = sql(&journal);
    assert!(journal
        .begin_scoped_force_v5(&force.target, &force.intent, retained)
        .is_err());
    assert_eq!(sql(&journal), before);
    let old = journal
        .restored_publication(&cancel.target, &reference)
        .unwrap();
    assert!(same_original(&old, &original));
    assert_eq!(old.state, ActionsWriteState::AcceptedPending);
    assert_eq!(f::git_state(&fixture), git);
    w::exact(&fixture, &all);
}
#[test]
fn scoped_actions_control_v5_deserialized_escalation_metadata_is_never_begin_authority() {
    let (fixture, provider, app, all, cancel, mut force) = actual();
    let mut journal = ActionsWriteJournal::open(&f::database(&fixture)).unwrap();
    accepted(&mut journal, &cancel);
    let retained = proof(&journal, &provider, &app, &force).unwrap().unwrap();
    attach(&mut force, &retained);
    force.intent.context =
        serde_json::from_slice(&serde_json::to_vec(&force.intent.context).unwrap()).unwrap();
    let before = sql(&journal);
    assert!(journal.begin(&force.target, &force.intent).is_err());
    assert_eq!(sql(&journal), before);
    w::exact(&fixture, &all);
}
#[test]
fn scoped_actions_control_v5_pending_or_uncertain_normal_cancel_never_qualifies_for_force_exception(
) {
    for uncertain in [false, true] {
        let (fixture, provider, app, all, cancel, force) = actual();
        let mut journal = ActionsWriteJournal::open(&f::database(&fixture)).unwrap();
        let permit = journal.begin(&cancel.target, &cancel.intent).unwrap();
        if uncertain {
            journal
                .claim(&permit, &cancel.target, &cancel.intent)
                .unwrap();
            journal.mark_uncertain(&permit).unwrap();
        }
        let before = sql(&journal);
        assert!(proof(&journal, &provider, &app, &force).is_err());
        assert_eq!(sql(&journal), before);
        w::exact(&fixture, &all);
    }
}
#[test]
fn scoped_actions_control_v5_repository_source_cannot_escalate_saved_pr_context_even_same_run() {
    let (fixture, provider, app, all, mut cancel, force) = actual();
    cancel.intent.context.original_pr_v3 = Some(ActionsOriginalPrContextV3 {
        id: 900,
        node: "PR_900".into(),
        number: 9,
        head_sha: cancel.intent.context.head_sha.clone().unwrap(),
        base_sha: "b".repeat(40),
    });
    // Real accepted SQL marker with descriptive PR metadata; this deliberately
    // does NOT manufacture a positive PR source from the metadata.
    let mut journal = ActionsWriteJournal::open(&f::database(&fixture)).unwrap();
    accepted(&mut journal, &cancel);
    let before = sql(&journal);
    assert!(proof(&journal, &provider, &app, &force).is_err());
    assert_eq!(sql(&journal), before);
    w::exact(&fixture, &all);
}
#[test]
fn scoped_actions_control_v5_generation_change_between_prepare_and_begin_inserts_nothing() {
    let (fixture, provider, app, all, cancel, mut force) = actual();
    let mut journal = ActionsWriteJournal::open(&f::database(&fixture)).unwrap();
    accepted(&mut journal, &cancel);
    let retained = proof(&journal, &provider, &app, &force).unwrap().unwrap();
    attach(&mut force, &retained);
    assert_eq!(
        journal
            .connection
            .execute(
                "UPDATE actions_write_generations SET generation=generation+1",
                []
            )
            .unwrap(),
        force.target.resources.len()
    );
    let before = sql(&journal);
    assert!(journal
        .begin_scoped_force_v5(&force.target, &force.intent, retained)
        .is_err());
    assert_eq!(sql(&journal), before);
    w::exact(&fixture, &all);
}
#[test]
fn scoped_actions_control_v5_old_cancel_mutation_before_claim_never_claims_new_force() {
    let (fixture, provider, app, all, cancel, mut force) = actual();
    let mut journal = ActionsWriteJournal::open(&f::database(&fixture)).unwrap();
    let old = accepted(&mut journal, &cancel);
    let retained = proof(&journal, &provider, &app, &force).unwrap().unwrap();
    attach(&mut force, &retained);
    let permit = journal
        .begin_scoped_force_v5(&force.target, &force.intent, retained)
        .unwrap();
    assert_eq!(
        journal
            .connection
            .execute(
                "UPDATE actions_write_attempts SET state='acknowledged' WHERE attempt_id=?1",
                [old.attempt_id]
            )
            .unwrap(),
        1
    );
    let before = sql(&journal);
    assert!(journal
        .claim(&permit, &force.target, &force.intent)
        .is_err());
    assert_eq!(sql(&journal), before);
    assert!(!permit.used.load(Ordering::Acquire));
    w::exact(&fixture, &all);
}
#[test]
fn scoped_actions_control_v5_old_cancel_receipt_change_is_rejected_again_at_actual_claim_stamp() {
    let (fixture, provider, app, all, cancel, mut force) = actual();
    let mut journal = ActionsWriteJournal::open(&f::database(&fixture)).unwrap();
    let old = accepted(&mut journal, &cancel);
    let retained = proof(&journal, &provider, &app, &force).unwrap().unwrap();
    attach(&mut force, &retained);
    let permit = journal
        .begin_scoped_force_v5(&force.target, &force.intent, retained)
        .unwrap();
    journal
        .claim(&permit, &force.target, &force.intent)
        .unwrap();
    let stamp = journal.capture_claim_stamp_v3(&permit).unwrap();
    assert_eq!(journal.connection.execute("UPDATE actions_write_attempts SET receipt_json=json_set(receipt_json,'$.observed_attempt',3) WHERE attempt_id=?1", [old.attempt_id]).unwrap(), 1);
    let before = sql(&journal);
    assert!(journal.validate_claim_stamp_v3(&stamp).is_err());
    assert_eq!(sql(&journal), before);
    w::exact(&fixture, &all);
}
#[test]
fn scoped_actions_control_v5_no_old_blocker_needs_no_exception_and_other_kinds_cannot_capture_one()
{
    let (fixture, provider, app, all, cancel, force) = actual();
    let journal = ActionsWriteJournal::open(&f::database(&fixture)).unwrap();
    let before = sql(&journal);
    assert!(proof(&journal, &provider, &app, &force).unwrap().is_none());
    assert!(proof(&journal, &provider, &app, &cancel).is_err());
    assert_eq!(sql(&journal), before);
    w::exact(&fixture, &all);
}
#[test]
fn scoped_actions_control_v5_other_action_cannot_describe_force_metadata() {
    let (fixture, provider, app, all, cancel, mut force) = actual();
    let mut journal = ActionsWriteJournal::open(&f::database(&fixture)).unwrap();
    accepted(&mut journal, &cancel);
    let retained = proof(&journal, &provider, &app, &force).unwrap().unwrap();
    attach(&mut force, &retained);
    for kind in [
        ActionsWriteKind::Cancel,
        ActionsWriteKind::DeleteRun,
        ActionsWriteKind::DeleteLogs,
        ActionsWriteKind::RerunAll,
    ] {
        force.intent.kind = kind;
        assert!(force.intent.fingerprint(&force.target).is_err());
    }
    w::exact(&fixture, &all);
}
#[test]
fn scoped_actions_control_v5_none_metadata_serializes_exact_original_context_and_fingerprint() {
    let (fixture, _provider, _app, all, cancel, _force) = actual();
    let value = serde_json::to_value(&cancel.intent.context).unwrap();
    assert!(value.get("force_escalation_v5").is_none());
    let old = serde_json::json!({"run_id":cancel.intent.context.run_id,"attempt":cancel.intent.context.attempt,
        "head_sha":cancel.intent.context.head_sha,"workflow_commit":null,"job_id":null,
        "original_repository_v3":cancel.intent.context.original_repository_v3,
        "original_run_v3":cancel.intent.context.original_run_v3});
    let restored: ActionsImmutableContext = serde_json::from_value(old).unwrap();
    let old_intent = ActionsWriteIntent::new(
        cancel.intent.kind,
        restored,
        cancel.intent.options.clone(),
        b"legacy-independent-binding",
    )
    .unwrap();
    assert_eq!(
        serde_json::to_vec(&old_intent.context).unwrap(),
        serde_json::to_vec(&cancel.intent.context).unwrap()
    );
    assert_eq!(
        old_intent.fingerprint(&cancel.target).unwrap(),
        cancel.intent.fingerprint(&cancel.target).unwrap()
    );
    w::exact(&fixture, &all);
}
