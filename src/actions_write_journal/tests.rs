use super::*;
use crate::providers::gh::ActionsScope;
use crate::test_support as support;
use serde_json::{json, Value};
use std::{fs, os::unix::fs::PermissionsExt};
use support::{Fixture, GhStep};
#[path = "acceptance_tests.rs"]
mod acceptance_tests;
#[path = "availability_tests.rs"]
mod availability_tests;
#[path = "bounded_candidates_tests.rs"]
mod bounded_candidates_tests;
#[path = "bounded_original_tests.rs"]
mod bounded_original_tests;
#[path = "scope_target_tests.rs"]
mod scope_target_tests;

fn directory() -> tempfile::TempDir {
    tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap()
}
fn provider() -> GithubProvider {
    // Canonical resource resolution is pure: a transport attempt would fail.
    GithubProvider::new("github.invalid")
        .unwrap()
        .with_program("/ghtui-fixture/no-network-program")
}
fn identity(account: Option<u64>) -> HostIdentity {
    HostIdentity {
        host: "github.invalid".into(),
        login: "octocat".into(),
        account_id: account,
        gh_version: "gh version 2.101.0".into(),
    }
}
fn run(repository: &str) -> ActionsTarget {
    ActionsTarget::Run {
        repository: repository.into(),
        id: 123,
    }
}
fn target(value: ActionsTarget) -> ActionsWriteTarget {
    ActionsWriteTarget::new(&provider(), &identity(Some(42)), &value).unwrap()
}
fn intent(kind: ActionsWriteKind, payload: &[u8]) -> ActionsWriteIntent {
    ActionsWriteIntent::new(
        kind,
        ActionsImmutableContext {
            run_id: Some(123),
            attempt: Some(3),
            head_sha: Some("a".repeat(40)),
            ..Default::default()
        },
        ActionsSafeOptions::default(),
        payload,
    )
    .unwrap()
}
fn deployment(ids: Vec<u64>, attempt: u64) -> ActionsWriteTarget {
    target(ActionsTarget::DeploymentReview {
        repository: "example/repo".into(),
        run_id: 123,
        attempt,
        head_sha: "a".repeat(40),
        environment_ids: ids,
    })
}
fn receipt(kind: ActionsReceiptKind) -> ActionsReceipt {
    ActionsReceipt {
        kind,
        resource_id: Some(123),
        observed_attempt: Some(3),
        observed_run_id: None,
    }
}

#[test]
fn actions_run_control_journal_positive_principal_exact_kind_and_pure_keys() {
    let gh = provider();
    for account in [None, Some(0)] {
        assert!(ActionsWriteTarget::new(&gh, &identity(account), &run("example/repo")).is_err());
    }
    let mut other = identity(Some(42));
    other.host = "other.invalid".into();
    assert!(ActionsWriteTarget::new(&gh, &other, &run("example/repo")).is_err());
    assert!(ActionsWriteTarget::new(
        &gh,
        &identity(Some(42)),
        &ActionsTarget::Run {
            repository: "example/repo".into(),
            id: 0
        }
    )
    .is_err());
    let scope = target(run("Example/Repo"));
    assert_eq!(
        scope.resource_keys(),
        &["github.invalid/example/repo/run/123"]
    );
    assert_eq!(scope.scope().login, "user-id:42");
    assert!(intent(ActionsWriteKind::SetSecret, b"value")
        .fingerprint(&scope)
        .is_err());
    assert!(ActionsWriteIntent::new(
        ActionsWriteKind::Cancel,
        ActionsImmutableContext {
            head_sha: Some("body, not SHA".into()),
            ..Default::default()
        },
        Default::default(),
        b""
    )
    .is_err());
}

#[test]
fn actions_run_control_journal_settings_ids_association_kind_and_dispatch_correlation_are_exact() {
    let dir = directory();
    let path = dir.path().join("cache.sqlite3");
    let runner = target(ActionsTarget::Runner {
        scope: ActionsScope::Repository {
            repository: "example/repo".into(),
        },
        id: 88,
    });
    let value = ActionsWriteIntent::new(
        ActionsWriteKind::RunnerLabels,
        Default::default(),
        Default::default(),
        b"safe transient labels",
    )
    .unwrap();
    let mut journal = ActionsWriteJournal::open(&path).unwrap();
    let permit = journal.begin(&runner, &value).unwrap();
    journal.claim(&permit, &runner, &value).unwrap();
    let mut effect = ActionsReceipt {
        kind: ActionsReceiptKind::SettingsMetadata,
        resource_id: Some(99),
        observed_attempt: None,
        observed_run_id: None,
    };
    assert!(journal.mark_confirmed(&permit, &effect).is_err());
    assert_eq!(
        journal
            .blocking(&runner, ActionsWriteKind::RemoveRunner)
            .unwrap()
            .len(),
        1
    );
    effect.resource_id = Some(88);
    journal.mark_confirmed(&permit, &effect).unwrap();
    let membership = target(ActionsTarget::GroupAssociations {
        organization: "example".into(),
        id: 9,
        kind: GroupAssociation::Repositories,
    });
    let wrong_kind = ActionsWriteIntent::new(
        ActionsWriteKind::GroupRunners,
        Default::default(),
        Default::default(),
        b"membership",
    )
    .unwrap();
    assert!(wrong_kind.fingerprint(&membership).is_err());
    let workflow = target(ActionsTarget::Workflow {
        repository: "example/repo".into(),
        id: 7,
    });
    let dispatch = ActionsWriteIntent::new(
        ActionsWriteKind::DispatchWorkflow,
        ActionsImmutableContext {
            workflow_commit: Some("a".repeat(40)),
            ..Default::default()
        },
        Default::default(),
        b"dispatch payload",
    )
    .unwrap();
    let permit = journal.begin(&workflow, &dispatch).unwrap();
    journal.claim(&permit, &workflow, &dispatch).unwrap();
    let receipt = ActionsReceipt {
        kind: ActionsReceiptKind::Dispatch,
        resource_id: Some(7),
        observed_attempt: Some(1),
        observed_run_id: Some(123),
    };
    journal.mark_accepted_pending(&permit, &receipt).unwrap();
    let marker = journal
        .blocking(&workflow, ActionsWriteKind::DispatchWorkflow)
        .unwrap()
        .remove(0);
    assert_eq!(marker.state, ActionsWriteState::AcceptedPending);
    assert_eq!(marker.receipt.unwrap().observed_run_id, Some(123));
    let invalid = ActionsReceipt {
        kind: ActionsReceiptKind::SettingsMetadata,
        ..receipt
    };
    assert!(
        validate_receipt(&invalid).is_err(),
        "a dispatch launch is not a settings or completion receipt"
    );
}
#[test]
fn actions_run_control_journal_pending_survives_restart_alias_and_actual_cache_clear() {
    let dir = directory();
    let path = dir.path().join("cache.sqlite3");
    let scope = target(run("example/repo"));
    let original = intent(ActionsWriteKind::Cancel, b"cancel");
    let mut journal = ActionsWriteJournal::open(&path).unwrap();
    let permit = journal.begin(&scope, &original).unwrap();
    let id = permit.marker().attempt_id;
    drop(permit);
    drop(journal);
    Cache::open(&path)
        .unwrap()
        .clear_records(scope.scope())
        .unwrap();
    let mut reopened = ActionsWriteJournal::open(&path).unwrap();
    let alias = target(run("EXAMPLE/REPO"));
    let observed = reopened
        .blocking(&alias, ActionsWriteKind::DeleteRun)
        .unwrap();
    assert_eq!(observed.len(), 1);
    assert_eq!(observed[0].attempt_id, id);
    assert_eq!(observed[0].state, ActionsWriteState::Pending);
    assert!(reopened
        .begin(
            &alias,
            &intent(ActionsWriteKind::DeleteRun, b"different action")
        )
        .is_err());
    assert_eq!(generations(&reopened.connection, &scope).unwrap(), vec![1]);
}
#[test]
fn actions_run_control_journal_single_use_changed_payload_and_database_isolation() {
    let dir = directory();
    let scope = target(run("example/repo"));
    let one = intent(ActionsWriteKind::Cancel, b"first body");
    let edited = intent(ActionsWriteKind::Cancel, b"edited body");
    assert_eq!(
        one.fingerprint(&scope).unwrap(),
        edited.fingerprint(&scope).unwrap(),
        "persisted fingerprint excludes content"
    );
    let mut source = ActionsWriteJournal::open(&dir.path().join("source.sqlite3")).unwrap();
    let mut other = ActionsWriteJournal::open(&dir.path().join("other.sqlite3")).unwrap();
    let permit = source.begin(&scope, &one).unwrap();
    assert!(other.claim(&permit, &scope, &one).is_err());
    assert!(source.claim(&permit, &scope, &edited).is_err());
    source.claim(&permit, &scope, &one).unwrap();
    assert!(source.claim(&permit, &scope, &one).is_err());
    assert_eq!(generations(&source.connection, &scope).unwrap(), vec![2]);
    assert!(other
        .blocking(&scope, ActionsWriteKind::Cancel)
        .unwrap()
        .is_empty());
    assert_eq!(
        source.blocking(&scope, ActionsWriteKind::Cancel).unwrap()[0].state,
        ActionsWriteState::Pending
    );
}

#[test]
fn actions_run_control_journal_claim_transaction_failure_cannot_be_mislabeled_confirmed() {
    let dir = directory();
    let scope = target(run("example/repo"));
    let value = intent(ActionsWriteKind::Cancel, b"private publication");
    let mut journal = ActionsWriteJournal::open(&dir.path().join("cache.sqlite3")).unwrap();
    let permit = journal.begin(&scope, &value).unwrap();
    journal.connection.execute_batch("CREATE TRIGGER fail_generation BEFORE UPDATE ON actions_write_generations BEGIN SELECT RAISE(FAIL,'fixture generation failure'); END;").unwrap();
    assert!(journal.claim(&permit, &scope, &value).is_err());
    journal
        .connection
        .execute_batch("DROP TRIGGER fail_generation;")
        .unwrap();
    assert!(journal
        .mark_confirmed(&permit, &receipt(ActionsReceiptKind::RunTransition))
        .is_err());
    assert!(
        journal.claim(&permit, &scope, &value).is_err(),
        "a transaction-failed in-memory permit remains poisoned"
    );
    assert_eq!(
        journal
            .connection
            .query_row("SELECT claimed FROM actions_write_attempts", [], |row| row
                .get::<_, i64>(
                0
            ))
            .unwrap(),
        0
    );
    assert_eq!(
        journal.blocking(&scope, ActionsWriteKind::Cancel).unwrap()[0].state,
        ActionsWriteState::Pending
    );
    assert_eq!(generations(&journal.connection, &scope).unwrap(), vec![1]);
}
#[test]
fn actions_run_control_journal_sql_failures_never_issue_permit_or_release_protection() {
    let dir = directory();
    let path = dir.path().join("cache.sqlite3");
    let scope = target(run("example/repo"));
    let value = intent(ActionsWriteKind::Cancel, b"publish");
    let mut journal = ActionsWriteJournal::open(&path).unwrap();
    journal.connection.execute_batch("CREATE TRIGGER fail_begin BEFORE INSERT ON actions_write_attempts BEGIN SELECT RAISE(FAIL,'fixture storage failure'); END;").unwrap();
    assert!(journal.begin(&scope, &value).is_err());
    assert_eq!(generations(&journal.connection, &scope).unwrap(), vec![0]);
    assert!(journal
        .blocking(&scope, ActionsWriteKind::Cancel)
        .unwrap()
        .is_empty());
    journal
        .connection
        .execute_batch("DROP TRIGGER fail_begin;")
        .unwrap();
    let permit = journal.begin(&scope, &value).unwrap();
    journal.connection.execute_batch("CREATE TRIGGER fail_claim BEFORE UPDATE OF claimed ON actions_write_attempts BEGIN SELECT RAISE(FAIL,'fixture claim failure'); END;").unwrap();
    assert!(journal.claim(&permit, &scope, &value).is_err());
    assert_eq!(generations(&journal.connection, &scope).unwrap(), vec![1]);
    journal
        .connection
        .execute_batch("DROP TRIGGER fail_claim;")
        .unwrap();
    journal.claim(&permit, &scope, &value).unwrap();
    journal.connection.execute_batch("CREATE TRIGGER fail_receipt BEFORE UPDATE OF state ON actions_write_attempts BEGIN SELECT RAISE(FAIL,'fixture receipt failure'); END;").unwrap();
    assert!(journal
        .mark_confirmed(&permit, &receipt(ActionsReceiptKind::RunTransition))
        .is_err());
    drop(journal);
    let reopened = ActionsWriteJournal::open(&path).unwrap();
    assert_eq!(
        reopened.blocking(&scope, ActionsWriteKind::Cancel).unwrap()[0].state,
        ActionsWriteState::Pending
    );
    assert_eq!(generations(&reopened.connection, &scope).unwrap(), vec![2]);
}
#[test]
fn actions_run_control_journal_secret_content_length_and_raw_hash_never_persist_or_debug() {
    let dir = directory();
    let path = dir.path().join("cache.sqlite3");
    let scope = target(ActionsTarget::Secret {
        scope: ActionsScope::Repository {
            repository: "example/repo".into(),
        },
        name: "DEPLOY_TOKEN".into(),
    });
    let secret = b"very-private-low-entropy-value-not-a-token";
    let value = ActionsWriteIntent::new(
        ActionsWriteKind::SetSecret,
        Default::default(),
        Default::default(),
        secret,
    )
    .unwrap();
    let another = ActionsWriteIntent::new(
        ActionsWriteKind::SetSecret,
        Default::default(),
        Default::default(),
        b"different length",
    )
    .unwrap();
    assert_eq!(
        value.fingerprint(&scope).unwrap(),
        another.fingerprint(&scope).unwrap()
    );
    let raw_hash = format!("{:x}", Sha256::digest(secret));
    let mut journal = ActionsWriteJournal::open(&path).unwrap();
    let permit = journal.begin(&scope, &value).unwrap();
    for display in [format!("{value:?}"), format!("{permit:?}")] {
        assert!(!display.contains(std::str::from_utf8(secret).unwrap()));
        assert!(!display.contains(&raw_hash));
    }
    let saved:(String,String,String,String,String,String)=journal.connection.query_row("SELECT target_json,kind_json,context_json,options_json,fingerprint_sha256,COALESCE(receipt_json,'') FROM actions_write_attempts",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?))).unwrap();
    let text = format!("{saved:?}");
    assert!(!text.contains(std::str::from_utf8(secret).unwrap()));
    assert!(!text.contains(&raw_hash));
    assert!(!text.contains("transient_binding"));
    assert!(!text.contains("payload_length"));
    drop(journal);
    for entry in fs::read_dir(dir.path()).unwrap() {
        let bytes = fs::read(entry.unwrap().path()).unwrap();
        assert!(!bytes.windows(secret.len()).any(|window| window == secret));
        assert!(!bytes
            .windows(raw_hash.len())
            .any(|window| window == raw_hash.as_bytes()));
    }
}
#[test]
fn actions_approvals_journal_accepted_pending_survives_restart_blocks_only_original_gate_decisions()
{
    let dir = directory();
    let path = dir.path().join("cache.sqlite3");
    let gate = deployment(vec![9], 3);
    let approval = intent(
        ActionsWriteKind::ApproveDeployment,
        b"a private explanation",
    );
    let mut journal = ActionsWriteJournal::open(&path).unwrap();
    let permit = journal.begin(&gate, &approval).unwrap();
    assert!(
        journal
            .mark_accepted_pending(&permit, &receipt(ActionsReceiptKind::DeploymentAccepted))
            .is_err(),
        "acceptance cannot predate authorization"
    );
    journal.claim(&permit, &gate, &approval).unwrap();
    journal
        .mark_accepted_pending(&permit, &receipt(ActionsReceiptKind::DeploymentAccepted))
        .unwrap();
    let id = permit.marker().attempt_id;
    drop(permit);
    drop(journal);
    let reopened = ActionsWriteJournal::open(&path).unwrap();
    for kind in [
        ActionsWriteKind::ApproveDeployment,
        ActionsWriteKind::RejectDeployment,
    ] {
        let markers = reopened
            .blocking(&deployment(vec![9, 10], 3), kind)
            .unwrap();
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].state, ActionsWriteState::AcceptedPending);
        assert_eq!(markers[0].attempt_id, id);
    }
    assert!(reopened
        .blocking(&target(run("example/repo")), ActionsWriteKind::Cancel)
        .unwrap()
        .is_empty());
    assert!(reopened
        .blocking(
            &deployment(vec![10], 3),
            ActionsWriteKind::ApproveDeployment
        )
        .unwrap()
        .is_empty());
    assert!(reopened
        .blocking(&deployment(vec![9], 4), ActionsWriteKind::ApproveDeployment)
        .unwrap()
        .is_empty());
    let marker = read_attempt(&reopened.connection, gate.scope(), id)
        .unwrap()
        .unwrap();
    assert_eq!(
        marker.receipt.unwrap().kind,
        ActionsReceiptKind::DeploymentAccepted,
        "gate still pending is acceptance, never requested-decision completion"
    );
}
#[test]
fn actions_run_control_journal_restored_original_reference_prevents_resend_after_field_edits() {
    let dir = directory();
    let scope = target(run("example/repo"));
    let mut journal = ActionsWriteJournal::open(&dir.path().join("cache.sqlite3")).unwrap();
    let value = intent(ActionsWriteKind::Cancel, b"original comment");
    let permit = journal.begin(&scope, &value).unwrap();
    let reference = permit.marker().publication_reference();
    journal.claim(&permit, &scope, &value).unwrap();
    assert!(journal
        .mark_confirmed(&permit, &receipt(ActionsReceiptKind::RunDeleted))
        .is_err());
    journal
        .mark_confirmed(&permit, &receipt(ActionsReceiptKind::RunTransition))
        .unwrap();
    let edited = intent(ActionsWriteKind::Cancel, b"different restored text");
    assert_eq!(
        journal
            .restored_publication(&scope, &reference)
            .unwrap()
            .state,
        ActionsWriteState::Confirmed
    );
    let fresh = journal.begin(&scope, &edited).unwrap();
    assert_ne!(
        fresh.marker().attempt_id,
        reference.attempt_id,
        "fresh deliberate action is not a restored resend"
    );
    let other =
        ActionsWriteTarget::new(&provider(), &identity(Some(43)), &run("example/repo")).unwrap();
    assert!(journal.restored_publication(&other, &reference).is_err());
}
#[test]
fn actions_run_control_journal_multi_resource_intersection_serializes_runner_effects() {
    let dir = directory();
    let path = dir.path().join("cache.sqlite3");
    let runner = target(ActionsTarget::Runner {
        scope: ActionsScope::Repository {
            repository: "example/repo".into(),
        },
        id: 88,
    });
    assert_eq!(
        runner.resources.len(),
        2,
        "runner and label keys remain the provider authority"
    );
    let value = ActionsWriteIntent::new(
        ActionsWriteKind::RunnerLabels,
        Default::default(),
        Default::default(),
        b"label content",
    )
    .unwrap();
    let mut first = ActionsWriteJournal::open(&path).unwrap();
    let second = ActionsWriteJournal::open(&path).unwrap();
    let permit = first.begin(&runner, &value).unwrap();
    assert_eq!(
        second
            .blocking(&runner, ActionsWriteKind::RemoveRunner)
            .unwrap()
            .len(),
        1
    );
    for key in &runner.resources {
        let intersecting = ActionsWriteTarget {
            scope: runner.scope.clone(),
            target: runner.target.clone(),
            resources: vec![key.clone()],
        };
        assert_eq!(
            second
                .blocking(&intersecting, ActionsWriteKind::RemoveRunner)
                .unwrap()[0]
                .attempt_id,
            permit.marker().attempt_id
        );
    }
    assert_eq!(
        generations(&second.connection, &runner).unwrap(),
        vec![1, 1]
    );
}

fn gh(fixture: &Fixture, runner: crate::providers::runner::CommandRunner) -> GithubProvider {
    GithubProvider::new("github.invalid")
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"))
        .with_runner(runner)
}
fn read(endpoint: &str, value: Value, paged: bool) -> GhStep {
    let mut args = vec![
        "api".into(),
        endpoint.into(),
        "--hostname".into(),
        "github.invalid".into(),
        "--method".into(),
        "GET".into(),
    ];
    if paged {
        args.extend(["--paginate".into(), "--slurp".into()]);
    }
    GhStep {
        args,
        stdin: vec![],
        stdout: serde_json::to_vec(&value).unwrap(),
        stderr: vec![],
        status: 0,
        delay_ms: 0,
    }
}
fn account(id: u64) -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        read("user", json!({"login":"octocat","id":id}), false),
    ]
}
fn run_value(attempt: u64) -> Value {
    json!({"id":123,"workflow_id":42,"name":"CI","head_sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","head_branch":"feature","event":"pull_request","status":"waiting","conclusion":"","run_attempt":attempt,"actor":{"login":"alice","id":55,"type":"User"},"triggering_actor":{"login":"octocat","id":42,"type":"User"}})
}
fn run_inspection() -> Vec<GhStep> {
    run_inspection_value(Some(run_value(3)))
}
fn run_inspection_value(value: Option<Value>) -> Vec<GhStep> {
    let mut steps = account(42);
    steps.push(read(
        "repos/example/repo/actions/runs?per_page=100",
        json!([{"workflow_runs":value.into_iter().collect::<Vec<_>>()}]),
        true,
    ));
    steps.extend(account(42));
    steps
}
fn deployment_inspection(pending: bool, attempt: u64) -> Vec<GhStep> {
    let mut steps = account(42);
    steps.push(read(
        "repos/example/repo/actions/runs/123",
        run_value(attempt),
        false,
    ));
    steps.push(read(
        "repos/example/repo/actions/runs/123/pending_deployments",
        if pending {
            json!([{"environment":{"id":9,"name":"production"},"current_user_can_approve":true}])
        } else {
            json!([])
        },
        false,
    ));
    steps.push(read(
        "repos/example/repo/actions/runs/123/approvals",
        json!([]),
        false,
    ));
    steps.push(read(
        "repos/example/repo/actions/runs/123",
        run_value(attempt),
        false,
    ));
    steps.extend(account(42));
    steps
}
fn commands(fixture: &Fixture) -> Vec<Value> {
    fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn assert_only_reads(fixture: &Fixture, count: usize) {
    let journal = commands(fixture);
    assert_eq!(journal.len(), count);
    for row in journal {
        assert_eq!(row["matched"], true);
        let args = row["args"].as_array().unwrap();
        assert!(!args
            .iter()
            .any(|arg| matches!(arg.as_str(), Some("POST" | "PUT" | "PATCH" | "DELETE"))));
    }
}
#[test]
fn actions_run_control_journal_fresh_opaque_inspection_then_explicit_ack_not_reading_unlocks() {
    let fixture = Fixture::new();
    let path = fixture.repo().join("cache.sqlite3");
    let mut steps = run_inspection();
    steps.extend(account(42));
    fixture.scripted_gh(&steps);
    let runner = crate::providers::runner::CommandRunner::default();
    let client = gh(&fixture, runner.clone());
    let scope =
        ActionsWriteTarget::new(&client, &identity(Some(42)), &run("example/repo")).unwrap();
    let value = intent(ActionsWriteKind::Cancel, b"private publication");
    let mut journal = ActionsWriteJournal::open(&path).unwrap();
    let permit = journal.begin(&scope, &value).unwrap();
    journal.claim(&permit, &scope, &value).unwrap();
    journal.mark_uncertain(&permit).unwrap();
    runner.mark_ambiguous(&scope.resources[0]);
    let snapshot = journal
        .prepare_inspection(
            &client,
            &identity(Some(42)),
            permit.marker().attempt_id,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(snapshot.observation().identity().account_id, Some(42));
    assert_eq!(snapshot.observation().state()["id"], 123);
    assert_eq!(
        journal
            .blocking(&scope, ActionsWriteKind::Cancel)
            .unwrap()
            .len(),
        1
    );
    assert_only_reads(&fixture, 5);
    journal
        .acknowledge(
            &client,
            snapshot,
            &identity(Some(42)),
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(journal
        .blocking(&scope, ActionsWriteKind::Cancel)
        .unwrap()
        .is_empty());
    assert_eq!(
        read_attempt(
            &journal.connection,
            scope.scope(),
            permit.marker().attempt_id
        )
        .unwrap()
        .unwrap()
        .state,
        ActionsWriteState::Acknowledged
    );
    assert_only_reads(&fixture, 7);
}
#[test]
fn actions_run_control_journal_ack_sql_failure_keeps_restart_block_after_provider_ack() {
    let fixture = Fixture::new();
    let path = fixture.repo().join("cache.sqlite3");
    let mut steps = run_inspection();
    steps.extend(account(42));
    fixture.scripted_gh(&steps);
    let runner = crate::providers::runner::CommandRunner::default();
    let client = gh(&fixture, runner.clone());
    let scope =
        ActionsWriteTarget::new(&client, &identity(Some(42)), &run("example/repo")).unwrap();
    let value = intent(ActionsWriteKind::Cancel, b"private publication");
    let mut journal = ActionsWriteJournal::open(&path).unwrap();
    let permit = journal.begin(&scope, &value).unwrap();
    journal.claim(&permit, &scope, &value).unwrap();
    journal.mark_uncertain(&permit).unwrap();
    runner.mark_ambiguous(&scope.resources[0]);
    let snapshot = journal
        .prepare_inspection(
            &client,
            &identity(Some(42)),
            permit.marker().attempt_id,
            &CancellationToken::default(),
        )
        .unwrap();
    journal.connection.execute_batch("CREATE TRIGGER fail_ack BEFORE UPDATE OF state ON actions_write_attempts BEGIN SELECT RAISE(FAIL,'fixture acknowledgement storage failure'); END;").unwrap();
    assert!(journal
        .acknowledge(
            &client,
            snapshot,
            &identity(Some(42)),
            &CancellationToken::default()
        )
        .is_err());
    drop(journal);
    let mut reopened = ActionsWriteJournal::open(&path).unwrap();
    assert_eq!(
        reopened.blocking(&scope, ActionsWriteKind::Cancel).unwrap()[0].state,
        ActionsWriteState::Uncertain
    );
    assert!(reopened.begin(&scope, &value).is_err());
    assert_only_reads(&fixture, 7);
}
#[test]
fn actions_run_control_journal_old_local_provider_database_and_account_ack_stays_blocked() {
    for failure in ["local", "provider", "database", "account"] {
        let fixture = Fixture::new();
        let path = fixture.repo().join("cache.sqlite3");
        let mut steps = run_inspection();
        if matches!(failure, "provider" | "account") {
            steps.extend(account(if failure == "account" { 43 } else { 42 }));
        }
        fixture.scripted_gh(&steps);
        let runner = crate::providers::runner::CommandRunner::default();
        let client = gh(&fixture, runner.clone());
        let scope =
            ActionsWriteTarget::new(&client, &identity(Some(42)), &run("example/repo")).unwrap();
        let value = intent(ActionsWriteKind::Cancel, b"publication");
        let mut journal = ActionsWriteJournal::open(&path).unwrap();
        let permit = journal.begin(&scope, &value).unwrap();
        journal.claim(&permit, &scope, &value).unwrap();
        journal.mark_uncertain(&permit).unwrap();
        runner.mark_ambiguous(&scope.resources[0]);
        let snapshot = journal
            .prepare_inspection(
                &client,
                &identity(Some(42)),
                permit.marker().attempt_id,
                &CancellationToken::default(),
            )
            .unwrap();
        match failure {
            "local" => journal.mark_uncertain(&permit).unwrap(),
            "provider" => runner.mark_ambiguous(&scope.resources[0]),
            _ => {}
        }
        let result = if failure == "database" {
            ActionsWriteJournal::open(&fixture.repo().join("other.sqlite3"))
                .unwrap()
                .acknowledge(
                    &client,
                    snapshot,
                    &identity(Some(42)),
                    &CancellationToken::default(),
                )
        } else {
            journal.acknowledge(
                &client,
                snapshot,
                &identity(Some(42)),
                &CancellationToken::default(),
            )
        };
        assert!(
            result.is_err(),
            "{failure} must not acknowledge stale/private state"
        );
        assert_eq!(
            journal.blocking(&scope, ActionsWriteKind::Cancel).unwrap()[0].state,
            ActionsWriteState::Uncertain
        );
        assert_only_reads(
            &fixture,
            if matches!(failure, "provider" | "account") {
                7
            } else {
                5
            },
        );
    }
}
#[test]
fn actions_approvals_journal_still_pending_cannot_ack_then_explicit_gate_transition_is_not_decision_proof(
) {
    let fixture = Fixture::new();
    let path = fixture.repo().join("cache.sqlite3");
    let mut steps = deployment_inspection(true, 3);
    steps.extend(deployment_inspection(false, 3));
    steps.extend(account(42));
    fixture.scripted_gh(&steps);
    let client = gh(&fixture, Default::default());
    let scope = ActionsWriteTarget::new(
        &client,
        &identity(Some(42)),
        deployment(vec![9], 3).target(),
    )
    .unwrap();
    let value = intent(
        ActionsWriteKind::ApproveDeployment,
        b"private decision explanation",
    );
    let mut journal = ActionsWriteJournal::open(&path).unwrap();
    let permit = journal.begin(&scope, &value).unwrap();
    journal.claim(&permit, &scope, &value).unwrap();
    journal
        .mark_accepted_pending(&permit, &receipt(ActionsReceiptKind::DeploymentAccepted))
        .unwrap();
    let pending = journal
        .prepare_inspection(
            &client,
            &identity(Some(42)),
            permit.marker().attempt_id,
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(matches!(
        pending.observation().deployment_transition().unwrap(),
        DeploymentTransition::StillPending { .. }
    ));
    assert_eq!(
        pending.disposition().unwrap(),
        ActionsInspectionDisposition::AcceptedStillPending
    );
    assert!(!pending.disposition().unwrap().can_acknowledge());
    assert!(journal
        .acknowledge(
            &client,
            pending,
            &identity(Some(42)),
            &CancellationToken::default()
        )
        .unwrap_err()
        .to_string()
        .contains("still has pending"));
    assert_eq!(
        journal
            .blocking(&scope, ActionsWriteKind::RejectDeployment)
            .unwrap()
            .len(),
        1
    );
    assert_only_reads(&fixture, 8);
    let transitioned = journal
        .prepare_inspection(
            &client,
            &identity(Some(42)),
            permit.marker().attempt_id,
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(matches!(
        transitioned.observation().deployment_transition().unwrap(),
        DeploymentTransition::GatesNoLongerPending { .. }
    ));
    assert_eq!(
        transitioned.disposition().unwrap(),
        ActionsInspectionDisposition::TransitionObserved
    );
    assert!(transitioned.disposition().unwrap().can_acknowledge());
    assert!(!transitioned
        .disposition()
        .unwrap()
        .can_acknowledge_inspected_acceptance());
    assert_eq!(
        journal
            .blocking(&scope, ActionsWriteKind::ApproveDeployment)
            .unwrap()
            .len(),
        1,
        "merely observing gates absent is not acknowledgement"
    );
    journal
        .acknowledge(
            &client,
            transitioned,
            &identity(Some(42)),
            &CancellationToken::default(),
        )
        .unwrap();
    let marker = read_attempt(
        &journal.connection,
        scope.scope(),
        permit.marker().attempt_id,
    )
    .unwrap()
    .unwrap();
    assert_eq!(marker.state, ActionsWriteState::TransitionObserved);
    assert_ne!(marker.state, ActionsWriteState::Confirmed);
    assert_eq!(
        marker.receipt.unwrap().kind,
        ActionsReceiptKind::DeploymentAccepted
    );
    assert_only_reads(&fixture, 18);
}

#[test]
fn actions_approvals_journal_original_attempt_advance_is_observed_not_requested_decision_confirmed()
{
    let fixture = Fixture::new();
    let path = fixture.repo().join("cache.sqlite3");
    let mut steps = deployment_inspection(true, 4);
    steps.extend(account(42));
    fixture.scripted_gh(&steps);
    let client = gh(&fixture, Default::default());
    let scope = ActionsWriteTarget::new(
        &client,
        &identity(Some(42)),
        deployment(vec![9], 3).target(),
    )
    .unwrap();
    let value = intent(ActionsWriteKind::ApproveDeployment, b"private explanation");
    let mut journal = ActionsWriteJournal::open(&path).unwrap();
    let permit = journal.begin(&scope, &value).unwrap();
    journal.claim(&permit, &scope, &value).unwrap();
    journal
        .mark_accepted_pending(&permit, &receipt(ActionsReceiptKind::DeploymentAccepted))
        .unwrap();
    let snapshot = journal
        .prepare_inspection(
            &client,
            &identity(Some(42)),
            permit.marker().attempt_id,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(
        snapshot.observation().deployment_transition().unwrap(),
        DeploymentTransition::OriginalAttemptAdvanced {
            requested_attempt: 3,
            current_attempt: 4
        }
    );
    assert_eq!(
        journal
            .blocking(&scope, ActionsWriteKind::ApproveDeployment)
            .unwrap()
            .len(),
        1
    );
    journal
        .acknowledge(
            &client,
            snapshot,
            &identity(Some(42)),
            &CancellationToken::default(),
        )
        .unwrap();
    let marker = read_attempt(
        &journal.connection,
        scope.scope(),
        permit.marker().attempt_id,
    )
    .unwrap()
    .unwrap();
    assert_eq!(marker.state, ActionsWriteState::TransitionObserved);
    assert_eq!(marker.context.attempt, Some(3));
    assert_eq!(
        marker.receipt.unwrap().kind,
        ActionsReceiptKind::DeploymentAccepted
    );
    assert_only_reads(&fixture, 10);
}

#[test]
fn actions_run_control_journal_accepted_cancel_still_pending_then_different_completion_is_observed_not_confirmed(
) {
    let fixture = Fixture::new();
    let database = fixture.repo().join("cache.sqlite3");
    let mut finished = run_value(3);
    finished["status"] = json!("completed");
    finished["conclusion"] = json!("success");
    let mut steps = run_inspection();
    steps.extend(run_inspection_value(Some(finished)));
    steps.extend(account(42));
    fixture.scripted_gh(&steps);
    let client = gh(&fixture, Default::default());
    let target =
        ActionsWriteTarget::new(&client, &identity(Some(42)), &run("example/repo")).unwrap();
    let value = intent(ActionsWriteKind::Cancel, b"requested cancellation");
    let mut journal = ActionsWriteJournal::open(&database).unwrap();
    let permit = journal.begin(&target, &value).unwrap();
    journal.claim(&permit, &target, &value).unwrap();
    journal
        .mark_accepted_pending(&permit, &receipt(ActionsReceiptKind::RunTransition))
        .unwrap();
    let id = permit.marker().attempt_id;
    drop(journal);
    let mut restarted = ActionsWriteJournal::open(&database).unwrap();
    let pending = restarted
        .prepare_inspection(
            &client,
            &identity(Some(42)),
            id,
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(restarted
        .acknowledge(
            &client,
            pending,
            &identity(Some(42)),
            &CancellationToken::default()
        )
        .unwrap_err()
        .to_string()
        .contains("still awaits"));
    assert_eq!(
        restarted
            .blocking(&target, ActionsWriteKind::Cancel)
            .unwrap()
            .len(),
        1
    );
    assert_only_reads(&fixture, 5);
    let completed = restarted
        .prepare_inspection(
            &client,
            &identity(Some(42)),
            id,
            &CancellationToken::default(),
        )
        .unwrap();
    restarted
        .acknowledge(
            &client,
            completed,
            &identity(Some(42)),
            &CancellationToken::default(),
        )
        .unwrap();
    let marker = read_attempt(&restarted.connection, target.scope(), id)
        .unwrap()
        .unwrap();
    assert_eq!(marker.state, ActionsWriteState::TransitionObserved);
    assert_ne!(
        marker.state,
        ActionsWriteState::Confirmed,
        "success completed while cancelling does not establish requested cancellation"
    );
    assert_eq!(marker.context.attempt, Some(3));
    assert_eq!(
        marker.receipt.unwrap().kind,
        ActionsReceiptKind::RunTransition
    );
    assert!(restarted
        .blocking(&target, ActionsWriteKind::Cancel)
        .unwrap()
        .is_empty());
    assert_only_reads(&fixture, 12);
}

#[test]
fn actions_run_control_journal_accepted_rerun_requires_later_original_attempt_and_explicit_ack() {
    let fixture = Fixture::new();
    let database = fixture.repo().join("cache.sqlite3");
    let mut original = run_value(3);
    original["status"] = json!("completed");
    original["conclusion"] = json!("failure");
    let mut advanced = run_value(4);
    advanced["status"] = json!("in_progress");
    let mut steps = run_inspection_value(Some(original));
    steps.extend(run_inspection_value(Some(advanced)));
    steps.extend(account(42));
    fixture.scripted_gh(&steps);
    let client = gh(&fixture, Default::default());
    let target =
        ActionsWriteTarget::new(&client, &identity(Some(42)), &run("example/repo")).unwrap();
    let value = intent(ActionsWriteKind::RerunFailed, b"rerun failed jobs");
    let mut journal = ActionsWriteJournal::open(&database).unwrap();
    let permit = journal.begin(&target, &value).unwrap();
    journal.claim(&permit, &target, &value).unwrap();
    journal
        .mark_accepted_pending(&permit, &receipt(ActionsReceiptKind::RunTransition))
        .unwrap();
    let id = permit.marker().attempt_id;
    let stale = journal
        .prepare_inspection(
            &client,
            &identity(Some(42)),
            id,
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(journal
        .acknowledge(
            &client,
            stale,
            &identity(Some(42)),
            &CancellationToken::default()
        )
        .unwrap_err()
        .to_string()
        .contains("still awaits"));
    let fresh = journal
        .prepare_inspection(
            &client,
            &identity(Some(42)),
            id,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(
        journal
            .blocking(&target, ActionsWriteKind::RerunFailed)
            .unwrap()
            .len(),
        1,
        "ordinary inspection cannot retire an accepted lease"
    );
    journal
        .acknowledge(
            &client,
            fresh,
            &identity(Some(42)),
            &CancellationToken::default(),
        )
        .unwrap();
    let marker = read_attempt(&journal.connection, target.scope(), id)
        .unwrap()
        .unwrap();
    assert_eq!(marker.state, ActionsWriteState::TransitionObserved);
    assert_eq!(
        marker.context.attempt,
        Some(3),
        "original request context is preserved even when new attempt4 is observed"
    );
    assert_eq!(marker.receipt.unwrap().observed_attempt, Some(3));
    assert_only_reads(&fixture, 12);
}

#[test]
fn actions_run_control_journal_bounded_run_absence_and_logs_metadata_cannot_retire_accepted_effect()
{
    for (kind, run, reason) in [
        (ActionsWriteKind::Cancel, None, "observed catalogue"),
        (
            ActionsWriteKind::DeleteLogs,
            Some(run_value(3)),
            "metadata cannot prove logs",
        ),
    ] {
        let fixture = Fixture::new();
        let database = fixture.repo().join("cache.sqlite3");
        fixture.scripted_gh(&run_inspection_value(run));
        let client = gh(&fixture, Default::default());
        let target = ActionsWriteTarget::new(
            &client,
            &identity(Some(42)),
            &ActionsTarget::Run {
                repository: "example/repo".into(),
                id: 123,
            },
        )
        .unwrap();
        let value = intent(kind, b"accepted unresolved effect");
        let mut journal = ActionsWriteJournal::open(&database).unwrap();
        let permit = journal.begin(&target, &value).unwrap();
        journal.claim(&permit, &target, &value).unwrap();
        let effect = receipt(if kind == ActionsWriteKind::DeleteLogs {
            ActionsReceiptKind::LogsDeleted
        } else {
            ActionsReceiptKind::RunTransition
        });
        journal.mark_accepted_pending(&permit, &effect).unwrap();
        let observation = journal
            .prepare_inspection(
                &client,
                &identity(Some(42)),
                permit.marker().attempt_id,
                &CancellationToken::default(),
            )
            .unwrap();
        assert!(journal
            .acknowledge(
                &client,
                observation,
                &identity(Some(42)),
                &CancellationToken::default()
            )
            .unwrap_err()
            .to_string()
            .contains(reason));
        assert_eq!(
            journal.blocking(&target, kind).unwrap()[0].state,
            ActionsWriteState::AcceptedPending
        );
        assert_only_reads(&fixture, 5);
    }
}
