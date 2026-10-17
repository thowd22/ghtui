//! Source-only causal coverage of retained own PR result history and bounded
//! metadata publication. Actual factories/SQLite/Git/strictGH, never fake proof.
use super::*;
use crate::actions_write_journal::{ActionsOwnedWorkflowHistoryV6, ActionsWriteJournal};
fn history() -> Vec<GhStep> {
    let mut steps = principal(42, "history-start");
    steps.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)));
    steps.push(wf("disabled_manually"));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)));
    steps.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    steps.extend(principal(42, "history-end"));
    assert_eq!(steps.len(), 9);
    steps
}
fn accepted(
    tail: Vec<GhStep>,
) -> (
    Fixture,
    GithubProvider,
    App,
    Vec<GhStep>,
    crate::drafts::DraftWriter,
) {
    let mut call = confirm(42);
    call[16] = wf("active");
    call.extend(tail);
    let (fix, p, mut app, all) = prepare(call);
    let (value, owner, drafts) = attempt(&mut app, &p, &fix);
    assert!(publication::publish(&mut app, &owner, value));
    app.scoped_actions_owner_v1 = None;
    form_helpers::paint(&mut app, 120, 40);
    (fix, p, app, all, drafts)
}
fn proof(app: &App) -> Arc<ActionsOwnedWorkflowHistoryV6> {
    crate::scoped_actions_workflow_ui_v6::capture(
        app.form.as_ref().unwrap(),
        crate::scoped_actions_workflow_ui_v6::WorkflowPhaseV6::Inspect,
    )
    .unwrap()
    .owned_history()
    .unwrap()
    .clone()
}
fn read(app: &mut App, p: &GithubProvider, fix: &Fixture, action: FormAction) {
    let work = form_helpers::request(app, action);
    let owner = native::ScopedActionsOwnerV1::new(work).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let prepared = publication::execute(&owner, p, &database(fix), None).unwrap();
    assert!(publication::publish(app, &owner, prepared));
    app.scoped_actions_owner_v1 = None;
    form_helpers::paint(app, 120, 40);
}
#[test]
fn scoped_actions_workflow_v6_corrective_actual_own_pr_result_invalidates_plain_guard_but_retained_read_opens(
) {
    let (fix, p, mut app, all, _drafts) = accepted(history());
    let source = app
        .scoped_actions_v1
        .as_ref()
        .unwrap()
        .source_for_test_v107()
        .clone();
    assert!(
        source
            .pr_origin_v2()
            .unwrap()
            .validate_local(&database(&fix))
            .is_err(),
        "Causal old path: SAME own journal changed plain Actions-page guard"
    );
    let actual = proof(&app);
    let journal = ActionsWriteJournal::open(&database(&fix)).unwrap();
    source
        .validate_owned_workflow_history_v6(&database(&fix), &journal, &actual)
        .unwrap();
    read(&mut app, &p, &fix, FormAction::Submit);
    assert!(app
        .form
        .as_ref()
        .unwrap()
        .plan_lines()
        .iter()
        .any(|line| line.contains("state disabled_manually")));
    form_helpers::exact(&fix, &all);
}
#[test]
fn scoped_actions_workflow_v6_corrective_ack_moves_only_actual_cas_successor_and_allows_fresh_reinspection(
) {
    let mut tail = history();
    tail.extend(history());
    tail.extend(history());
    let (fix, p, mut app, all, _drafts) = accepted(tail);
    let old = proof(&app);
    read(&mut app, &p, &fix, FormAction::Submit);
    read(&mut app, &p, &fix, FormAction::AcknowledgeActionsV3);
    let current = proof(&app);
    assert!(!Arc::ptr_eq(&old, &current));
    let journal = ActionsWriteJournal::open(&database(&fix)).unwrap();
    assert!(journal.validate_owned_workflow_history_v6(&old).is_err());
    journal
        .validate_owned_workflow_history_v6(&current)
        .unwrap();
    assert_eq!(form_helpers::result_sql(&fix)[0].0, "transition_observed");
    read(&mut app, &p, &fix, FormAction::Submit);
    form_helpers::exact(&fix, &all);
}
#[test]
fn scoped_actions_workflow_v6_corrective_current_pr_cache_drift_is_not_waived_by_own_row_proof() {
    let (fix, _p, app, all, _drafts) = accepted(vec![]);
    let actual = proof(&app);
    let journal = ActionsWriteJournal::open(&database(&fix)).unwrap();
    let source = app
        .scoped_actions_v1
        .as_ref()
        .unwrap()
        .source_for_test_v107();
    source
        .validate_owned_workflow_history_v6(&database(&fix), &journal, &actual)
        .unwrap();
    // The actual catalogue producer reserves a source but does not publish a
    // sync checkpoint. Advance its genuine existing reservation through the
    // SAME storage boundary; never insert a fictional checkpoint for this test.
    let original = source.original();
    let mut cache = crate::storage::Cache::open(&database(&fix)).unwrap();
    assert!(cache
        .sync_checkpoint(original.scope(), original.sync_source())
        .unwrap()
        .is_none());
    let old = cache
        .sync_reservation(original.scope(), original.sync_source())
        .unwrap()
        .expect("Actual original catalogue reservation");
    let next = cache
        .reserve_sync_generation(original.scope(), original.sync_source())
        .unwrap();
    assert_eq!(next, old + 1);
    assert_eq!(
        cache
            .sync_reservation(original.scope(), original.sync_source())
            .unwrap(),
        Some(next)
    );
    journal.validate_owned_workflow_history_v6(&actual).unwrap();
    assert!(source
        .validate_owned_workflow_history_v6(&database(&fix), &journal, &actual)
        .is_err());
    assert_eq!(form_helpers::result_sql(&fix)[0].0, "accepted_pending");
    form_helpers::exact(&fix, &all);
}
#[test]
fn scoped_actions_workflow_v6_corrective_same_generation_receipt_context_options_drift_refuses_read_and_ack(
) {
    for column in ["receipt_json", "context_json", "options_json"] {
        let (fix, _p, app, all, _drafts) = accepted(vec![]);
        let actual = proof(&app);
        let db = rusqlite::Connection::open(database(&fix)).unwrap();
        let sql = format!("UPDATE actions_write_attempts SET {column}={column}||' '");
        assert_eq!(db.execute(&sql, []).unwrap(), 1);
        let journal = ActionsWriteJournal::open(&database(&fix)).unwrap();
        assert!(
            journal.validate_owned_workflow_history_v6(&actual).is_err(),
            "Exact raw bytes, not decoded equality"
        );
        assert_eq!(form_helpers::result_sql(&fix)[0].0, "accepted_pending");
        form_helpers::exact(&fix, &all);
    }
}
#[test]
fn scoped_actions_workflow_v6_corrective_other_genuine_database_and_source_cannot_adopt_retained_result(
) {
    let (fix, _p, app, all, _drafts) = accepted(vec![]);
    let actual = proof(&app);
    let (other, _op, other_app, other_all, _other_drafts) = accepted(vec![]);
    let journal = ActionsWriteJournal::open(&database(&other)).unwrap();
    let source = other_app
        .scoped_actions_v1
        .as_ref()
        .unwrap()
        .source_for_test_v107();
    assert!(source
        .validate_owned_workflow_history_v6(&database(&other), &journal, &actual)
        .is_err());
    assert_eq!(form_helpers::result_sql(&other)[0].0, "accepted_pending");
    form_helpers::exact(&fix, &all);
    form_helpers::exact(&other, &other_all);
}
#[test]
fn scoped_actions_workflow_v6_corrective_actual_numeric_alias_generation_change_refuses_retained_read(
) {
    let (fix, _p, app, all, _drafts) = accepted(vec![]);
    let actual = proof(&app);
    let db = rusqlite::Connection::open(database(&fix)).unwrap();
    assert_eq!(db.execute("UPDATE actions_write_generations SET generation=generation+1 WHERE resource_key LIKE '%repository-id/%'",[]).unwrap(),1);
    let journal = ActionsWriteJournal::open(&database(&fix)).unwrap();
    assert!(journal.validate_owned_workflow_history_v6(&actual).is_err());
    assert_eq!(form_helpers::result_sql(&fix)[0].0, "accepted_pending");
    form_helpers::exact(&fix, &all);
}
#[test]
fn scoped_actions_workflow_v6_corrective_many_blank_canonical_yaml_never_expands_in_scoped_result()
{
    let mut yaml = "name: PRIVATE_PR_WORKFLOW\non: [push]\njobs: {}\n".to_owned();
    // Stay inside the existing source's strict5000-row display admission.
    // Publication must still NOT expand even these genuine admitted YAML rows.
    yaml.push_str(&"\n".repeat(4_000));
    let mut tail = bracket(vec![wf("active"), wf("active")]);
    tail.extend(bracket(vec![wf("active"), wf("active")]));
    tail.extend(confirm(42));
    let (fix, p, mut app, all) = setup_yaml(tail, Some(&yaml));
    assert!(
        app.actions
            .as_ref()
            .unwrap()
            .definition
            .as_ref()
            .unwrap()
            .yaml
            .lines()
            .count()
            > 4_000
    );
    open(&mut app, &p, &fix);
    form_helpers::preview(&mut app, &p, &fix);
    let (value, owner, _drafts) = attempt(&mut app, &p, &fix);
    let publication::PreparedResultV3::Workflow(prepared) = &value else {
        panic!("Actual workflow result")
    };
    let (execution, owned) = prepared.attempt_for_test_v6().unwrap();
    assert!(owned.is_some());
    let crate::actions_controller::ActionsExecution::Confirmed { outcome, .. } = execution else {
        panic!("Actual typed confirmed result")
    };
    assert_eq!(
        outcome.workspace.view,
        crate::actions_workspace::ActionsView::PublicationInspection
    );
    assert!(outcome.workspace.definition.is_none());
    assert!(outcome.workspace.rows.len() < 32);
    assert!(
        crate::actions_workspace::scoped_read_v1::retained_bytes(&outcome.workspace).unwrap()
            < 256 * 1024
    );
    assert!(publication::publish(&mut app, &owner, value));
    form_helpers::exact(&fix, &all);
}
#[test]
fn scoped_actions_workflow_v6_corrective_over_5000_yaml_rows_refuses_before_plan_or_publication() {
    let mut yaml = "name: PRIVATE_PR_WORKFLOW\non: [push]\njobs: {}\n".to_owned();
    yaml.push_str(&"\n".repeat(10_000));
    let (fix, p, app, all, owner) = definition_request(vec![], Some(&yaml));
    let before = sql_state(&fix);
    let git = git_state_resource(&fix);
    let source = app.remote_scope_source.as_ref().unwrap().clone();
    let result = publication::execute(&owner, &p, &database(&fix), None);
    let error = result
        .err()
        .expect("Genuine oversized Definition must fail closed");
    assert_eq!(
        error,
        "Workflow definition exceeds5000 display rows; no silent YAML truncation"
    );
    assert!(app.form.is_none() && app.scoped_actions_work_v1.is_none());
    assert!(app.actions.as_ref().unwrap().definition.is_none());
    assert!(Arc::ptr_eq(
        app.remote_scope_source.as_ref().unwrap(),
        &source
    ));
    assert_eq!(sql_state(&fix), before);
    assert_eq!(git_state_resource(&fix), git);
    let connection = rusqlite::Connection::open(database(&fix)).unwrap();
    let writes: i64 = connection
        .query_row("SELECT COUNT(*) FROM actions_write_attempts", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(
        writes, 0,
        "Source refusal grants no Plan/Confirm/publication"
    );
    exact(&fix, &all);
}
#[test]
fn scoped_actions_workflow_v6_corrective_failed_last_uid_never_releases_prepared_owned_history() {
    let (fix, p, mut app, all) = prepare(confirm(999));
    let (value, owner, _drafts) = attempt(&mut app, &p, &fix);
    let publication::PreparedResultV3::Workflow(prepared) = &value else {
        panic!("Actual workflow result")
    };
    assert!(prepared.attempt_for_test_v6().unwrap().1.is_none());
    assert!(publication::publish(&mut app, &owner, value));
    assert!(app.form.is_none() && app.github.is_none());
    assert_eq!(form_helpers::result_sql(&fix)[0].0, "confirmed");
    form_helpers::exact(&fix, &all);
}
