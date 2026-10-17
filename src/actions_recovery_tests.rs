use super::*;
use crate::{
    actions_write_journal::{
        ActionsImmutableContext, ActionsReceipt, ActionsReceiptKind, ActionsWriteIntent,
        ActionsWriteKind,
    },
    providers::gh::{ActionsTarget, HostIdentity},
    test_support::{Fixture, GhStep},
};
use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Value};
use std::path::Path;

fn workspace() -> ActionsWorkspace {
    ActionsWorkspace::new(
        HostIdentity {
            host: "github.invalid".into(),
            login: "octocat".into(),
            account_id: Some(42),
            gh_version: "offline".into(),
        },
        vec!["example/repo".into()],
    )
}
fn provider(fixture: &Fixture) -> GithubProvider {
    GithubProvider::new("github.invalid")
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"))
}
fn read(endpoint: &str, output: Value, paginated: bool) -> GhStep {
    let mut args = [
        "api",
        endpoint,
        "--hostname",
        "github.invalid",
        "--method",
        "GET",
    ]
    .map(str::to_owned)
    .to_vec();
    if paginated {
        args.extend(["--paginate".into(), "--slurp".into()]);
    }
    GhStep {
        args,
        stdin: vec![],
        stdout: serde_json::to_vec(&output).unwrap(),
        stderr: vec![],
        status: 0,
        delay_ms: 0,
    }
}
fn account(id: u64) -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        read("user", json!({"id":id,"login":"octocat"}), false),
    ]
}
fn run(status: &str, conclusion: &str) -> Value {
    json!({"id":123,"workflow_id":7,"name":"CI","display_title":"Current observed run",
        "head_sha":"a".repeat(40),"head_branch":"feature","event":"pull_request",
        "status":status,"conclusion":conclusion,"run_attempt":3,
        "actor":{"id":51,"login":"alice","type":"User"},"triggering_actor":{"id":42,"login":"octocat","type":"User"}})
}
fn catalogue(current: Value) -> GhStep {
    read(
        "repos/example/repo/actions/runs?per_page=100",
        json!({"total_count":1,"workflow_runs":[current]}),
        false,
    )
}
fn inspection(current: Value) -> Vec<GhStep> {
    let mut steps = account(42);
    steps.push(catalogue(current));
    steps.extend(account(42));
    steps
}
fn assert_reads(fixture: &Fixture, count: usize) {
    let recorded = std::fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|row| serde_json::from_str::<Value>(row).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(recorded.len(), count);
    assert!(recorded.iter().all(|row| row["matched"] == true));
    assert!(recorded
        .iter()
        .all(|row| row["args"][5].is_null() || row["args"][5] == "GET"));
}
fn original(
    journal: &mut ActionsWriteJournal,
    provider: &GithubProvider,
    repository: &str,
    state: ActionsWriteState,
) -> (i64, ActionsWriteTarget) {
    let target = ActionsWriteTarget::new(
        provider,
        &workspace().identity,
        &ActionsTarget::Run {
            repository: repository.into(),
            id: 123,
        },
    )
    .unwrap();
    let intent = ActionsWriteIntent::new(
        ActionsWriteKind::Cancel,
        ActionsImmutableContext {
            run_id: Some(123),
            attempt: Some(3),
            head_sha: Some("a".repeat(40)),
            ..Default::default()
        },
        Default::default(),
        b"never persist private input",
    )
    .unwrap();
    let permit = journal.begin(&target, &intent).unwrap();
    if state != ActionsWriteState::Pending {
        journal.claim(&permit, &target, &intent).unwrap();
        match state {
            ActionsWriteState::Uncertain => journal.mark_uncertain(&permit).unwrap(),
            ActionsWriteState::AcceptedPending => journal
                .mark_accepted_pending(
                    &permit,
                    &ActionsReceipt {
                        kind: ActionsReceiptKind::RunTransition,
                        resource_id: Some(123),
                        observed_attempt: Some(3),
                        observed_run_id: None,
                        original_dispatch_run_v7: None,
                        workflow_remote_v9: None,
                    },
                )
                .unwrap(),
            _ => panic!("Fixture requires an explicitly unresolved original"),
        }
    }
    (permit.marker().attempt_id, target)
}
fn saved(database: &Path, id: i64) -> (String, Value, Option<Value>) {
    let connection =
        Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let row:(String,String,Option<String>,String,String) = connection.query_row("SELECT state,context_json,receipt_json,principal,kind_json FROM actions_write_attempts WHERE attempt_id=?1",[id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?))).unwrap();
    assert_eq!(row.3, "user-id:42");
    assert_eq!(
        serde_json::from_str::<Value>(&row.4).unwrap(),
        json!("cancel")
    );
    let context: Value = serde_json::from_str(&row.1).unwrap();
    assert_eq!(context["run_id"], 123);
    assert_eq!(context["attempt"], 3);
    assert_eq!(context["head_sha"], "a".repeat(40));
    (
        row.0,
        context,
        row.2.map(|text| serde_json::from_str(&text).unwrap()),
    )
}

#[test]
fn actions_run_control_recovery_list_is_principal_scoped_and_preserves_original_sql_context() {
    let fixture = Fixture::new();
    let provider = provider(&fixture);
    let database = fixture.root.join("state.sqlite3");
    let mut journal = ActionsWriteJournal::open(&database).unwrap();
    let (first, _) = original(
        &mut journal,
        &provider,
        "example/repo",
        ActionsWriteState::Pending,
    );
    let (second, _) = original(
        &mut journal,
        &provider,
        "example/other",
        ActionsWriteState::Uncertain,
    );
    let mut other_identity = workspace().identity;
    other_identity.account_id = Some(43);
    let other = ActionsWriteTarget::new(
        &provider,
        &other_identity,
        &ActionsTarget::Run {
            repository: "example/private".into(),
            id: 123,
        },
    )
    .unwrap();
    let value = ActionsWriteIntent::new(
        ActionsWriteKind::Cancel,
        ActionsImmutableContext {
            run_id: Some(123),
            attempt: Some(3),
            head_sha: Some("a".repeat(40)),
            ..Default::default()
        },
        Default::default(),
        b"",
    )
    .unwrap();
    journal.begin(&other, &value).unwrap();
    drop(journal);
    let mut steps = account(42);
    steps.extend(account(42));
    fixture.scripted_gh(&steps);
    let prepared = list(
        workspace(),
        &provider,
        &database,
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(!prepared.clear_private);
    assert!(prepared.clear_actions_inspection);
    assert!(prepared.actions_inspection.is_none());
    let page = prepared.actions.unwrap();
    assert_eq!(page.view, ActionsView::Publications);
    assert_eq!(page.publication_total, Some(2));
    assert!(page.publication_complete);
    assert_eq!(
        page.publication_rows
            .iter()
            .map(|row| row.attempt_id)
            .collect::<Vec<_>>(),
        vec![second, first]
    );
    assert!(page.publication_rows[0]
        .details
        .iter()
        .any(|line| line.contains("example/other")));
    assert!(!page
        .publication_rows
        .iter()
        .flat_map(|row| row.details.iter())
        .any(|line| line.contains("example/private")));
    assert_eq!(saved(&database, first).0, "pending");
    assert_eq!(saved(&database, second).0, "uncertain");
    assert_reads(&fixture, 4);
}

#[test]
fn actions_run_control_recovery_waiting_accepted_request_cannot_ack_or_resend() {
    let fixture = Fixture::new();
    let provider = provider(&fixture);
    let database = fixture.root.join("state.sqlite3");
    let mut journal = ActionsWriteJournal::open(&database).unwrap();
    let (id, target) = original(
        &mut journal,
        &provider,
        "example/repo",
        ActionsWriteState::AcceptedPending,
    );
    drop(journal);
    let mut steps = inspection(run("in_progress", ""));
    steps.extend(account(42)); // Fresh-account refusal presentation, no ACK.
    fixture.scripted_gh(&steps);
    let inspected = inspect(
        workspace(),
        id,
        &provider,
        &database,
        &CancellationToken::default(),
    )
    .unwrap();
    let page = inspected
        .actions
        .as_ref()
        .unwrap()
        .publication_inspection
        .as_ref()
        .unwrap();
    assert_eq!(page.attempt_id, id);
    assert!(page.acknowledgement.is_none());
    assert!(page.lines.iter().any(|line| line.contains("STILL WAITING")));
    assert_eq!(saved(&database, id).0, "accepted_pending");
    let refused = acknowledge(
        inspected.actions.unwrap(),
        RecoveryAcknowledgement::ObservedState(id),
        inspected.actions_inspection,
        &provider,
        &database,
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(refused.clear_actions_inspection);
    assert!(refused.status.contains("still awaits"));
    assert_eq!(saved(&database, id).0, "accepted_pending");
    let journal = ActionsWriteJournal::open(&database).unwrap();
    assert_eq!(
        journal
            .blocking(&target, ActionsWriteKind::Cancel)
            .unwrap()
            .len(),
        1
    );
    assert_reads(&fixture, 7);
}

#[test]
fn actions_run_control_recovery_different_completion_is_explicit_transition_not_confirmation() {
    let fixture = Fixture::new();
    let provider = provider(&fixture);
    let database = fixture.root.join("state.sqlite3");
    let mut journal = ActionsWriteJournal::open(&database).unwrap();
    let (id, target) = original(
        &mut journal,
        &provider,
        "example/repo",
        ActionsWriteState::AcceptedPending,
    );
    drop(journal);
    let observed = run("completed", "success");
    let mut steps = inspection(observed.clone());
    steps.extend(account(42));
    steps.push(catalogue(observed));
    steps.extend(account(42));
    fixture.scripted_gh(&steps);
    let inspected = inspect(
        workspace(),
        id,
        &provider,
        &database,
        &CancellationToken::default(),
    )
    .unwrap();
    let page = inspected
        .actions
        .as_ref()
        .unwrap()
        .publication_inspection
        .as_ref()
        .unwrap();
    assert_eq!(
        page.acknowledgement,
        Some(ActionsPublicationAcknowledgement::ObservedState)
    );
    assert!(page
        .lines
        .iter()
        .any(|line| line.contains("not attributed")));
    let original = saved(&database, id);
    assert_eq!(
        original.0, "accepted_pending",
        "reading a completion never acknowledges it"
    );
    let result = acknowledge(
        inspected.actions.unwrap(),
        RecoveryAcknowledgement::ObservedState(id),
        inspected.actions_inspection,
        &provider,
        &database,
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(result.clear_actions_inspection);
    assert!(result.status.contains("no cause/completion inferred"));
    let after = saved(&database, id);
    assert_eq!(after.0, "transition_observed");
    assert_ne!(after.0, "confirmed");
    assert_eq!(after.1, original.1);
    assert_eq!(
        after.2, original.2,
        "keep original acceptance rather than invent cancellation causality"
    );
    assert!(ActionsWriteJournal::open(&database)
        .unwrap()
        .blocking(&target, ActionsWriteKind::Cancel)
        .unwrap()
        .is_empty());
    assert_reads(&fixture, 10);
}

#[test]
fn actions_run_control_recovery_mismatched_and_reused_attempt_proofs_release_nothing() {
    let fixture = Fixture::new();
    let provider = provider(&fixture);
    let database = fixture.root.join("state.sqlite3");
    let mut journal = ActionsWriteJournal::open(&database).unwrap();
    let (first, _) = original(
        &mut journal,
        &provider,
        "example/repo",
        ActionsWriteState::Pending,
    );
    let (second, _) = original(
        &mut journal,
        &provider,
        "example/other",
        ActionsWriteState::Pending,
    );
    drop(journal);
    let mut steps = inspection(run("in_progress", ""));
    steps.extend(account(42));
    steps.extend(account(42));
    fixture.scripted_gh(&steps);
    let inspected = inspect(
        workspace(),
        first,
        &provider,
        &database,
        &CancellationToken::default(),
    )
    .unwrap();
    let slot = inspected.actions_inspection.unwrap();
    let workspace = inspected.actions.unwrap();
    let mismatch = acknowledge(
        workspace.clone(),
        RecoveryAcknowledgement::ObservedState(second),
        Some(slot.clone()),
        &provider,
        &database,
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(mismatch.status.contains("different original attempt"));
    assert!(mismatch.clear_actions_inspection);
    assert!(slot.lock().unwrap().is_none());
    let reused = acknowledge(
        workspace,
        RecoveryAcknowledgement::ObservedState(first),
        Some(slot),
        &provider,
        &database,
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(reused.status.contains("no reusable acknowledgement proof"));
    assert!(reused.clear_actions_inspection);
    assert_eq!(saved(&database, first).0, "pending");
    assert_eq!(saved(&database, second).0, "pending");
    assert_reads(&fixture, 9);
}

#[test]
fn actions_run_control_recovery_list_hides_same_login_account_drift_or_final_auth_failure() {
    for mode in ["before", "after", "final401"] {
        let fixture = Fixture::new();
        let provider = provider(&fixture);
        let database = fixture.root.join("state.sqlite3");
        let mut journal = ActionsWriteJournal::open(&database).unwrap();
        let (id, _) = original(
            &mut journal,
            &provider,
            "example/repo",
            ActionsWriteState::Pending,
        );
        drop(journal);
        let mut steps = account(if mode == "before" { 43 } else { 42 });
        if mode != "before" {
            let mut after = account(if mode == "after" { 43 } else { 42 });
            if mode == "final401" {
                after[1].status = 1;
                after[1].stdout.clear();
                after[1].stderr = b"HTTP 401 private-original-diagnostic".to_vec();
            }
            steps.extend(after);
        }
        fixture.scripted_gh(&steps);
        let hidden = list(
            workspace(),
            &provider,
            &database,
            &CancellationToken::default(),
        )
        .unwrap();
        assert!(hidden.clear_private);
        assert!(hidden.clear_actions_inspection);
        assert!(hidden.actions.is_none());
        assert!(hidden.actions_inspection.is_none());
        assert!(!hidden.status.contains("private-original-diagnostic"));
        assert_eq!(saved(&database, id).0, "pending");
        assert_reads(&fixture, if mode == "before" { 2 } else { 4 });
    }
}
