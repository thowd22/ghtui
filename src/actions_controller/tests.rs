use super::*;
use crate::test_support as support;
use crate::{
    actions_workspace::{ActionsCommand, ActionsWorkspace, RunPage},
    actions_write_journal::{
        ActionsImmutableContext, ActionsReceiptKind, ActionsWriteIntent, ActionsWriteState,
        ActionsWriteTarget,
    },
    providers::gh::{ActionsScope, ActionsTarget, Actor, RunAttempt, WorkflowRun},
    storage::Cache,
};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::{cell::RefCell, collections::BTreeMap, fs, process::Command};
use support::{Fixture, GhStep};
#[path = "local_only_tests.rs"]
mod local_only_tests;

fn identity() -> HostIdentity {
    HostIdentity {
        host: "github.invalid".into(),
        login: "octocat".into(),
        account_id: Some(42),
        gh_version: "2.101.0".into(),
    }
}
fn provider(fixture: &Fixture) -> GithubProvider {
    GithubProvider::new("github.invalid")
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"))
}
fn read(endpoint: &str, value: Value) -> GhStep {
    GhStep {
        args: [
            "api",
            endpoint,
            "--hostname",
            "github.invalid",
            "--method",
            "GET",
        ]
        .map(str::to_owned)
        .to_vec(),
        stdin: vec![],
        stdout: serde_json::to_vec(&value).unwrap(),
        stderr: vec![],
        status: 0,
        delay_ms: 0,
    }
}
fn account() -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        read("user", json!({"id":42,"login":"octocat"})),
    ]
}
fn raw_run(status: &str, conclusion: &str) -> Value {
    json!({"id":123,"workflow_id":7,"name":"CI","display_title":"Original run","head_sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","head_branch":"feature","event":"pull_request","status":status,"conclusion":conclusion,"run_attempt":3,"actor":{"id":51,"login":"alice","type":"User"},"triggering_actor":{"id":42,"login":"octocat","type":"User"}})
}
fn workspace() -> ActionsWorkspace {
    let mut workspace = ActionsWorkspace::new(identity(), vec!["example/repo".into()]);
    workspace.run = Some(RunPage {
        repository: "example/repo".into(),
        attempt: RunAttempt {
            run: WorkflowRun {
                id: 123,
                workflow_id: 7,
                name: "CI".into(),
                title: "Original run".into(),
                head_sha: "a".repeat(40),
                head_branch: "feature".into(),
                event: "pull_request".into(),
                status: "in_progress".into(),
                conclusion: String::new(),
                attempt: 3,
                actor: Actor {
                    login: "alice".into(),
                    actor_type: "User".into(),
                    ..Default::default()
                },
                triggering_actor: Actor {
                    login: "octocat".into(),
                    actor_type: "User".into(),
                    ..Default::default()
                },
                created_at: String::new(),
                updated_at: String::new(),
                url: String::new(),
            },
            jobs: vec![],
            definition_note: "exact original attempt".into(),
        },
        selected_job: None,
        logs: None,
        annotations: vec![],
    });
    workspace
}
fn plan(fixture: &Fixture) -> ActionsPlan {
    ActionsPlan::prepare(
        &workspace(),
        ActionsCommand::Cancel,
        &BTreeMap::new(),
        &provider(fixture),
        &CancellationToken::default(),
    )
    .unwrap()
}
fn mutation(failed: bool, observed_status: &str, observed_conclusion: &str) -> Vec<GhStep> {
    let mut steps = account(); // Controller fresh principal before marker.
    steps.extend(account()); // Provider exact pre-write guard.
    steps.push(read(
        "repos/example/repo/actions/runs/123",
        raw_run("in_progress", ""),
    ));
    let mut write = read("repos/example/repo/actions/runs/123/cancel", Value::Null);
    write.args[5] = "POST".into();
    if failed {
        write.status = 1;
        write.stderr = b"fixture response interrupted after actual invocation".to_vec();
    }
    steps.push(write);
    if !failed {
        steps.push(read(
            "repos/example/repo/actions/runs/123",
            raw_run(observed_status, observed_conclusion),
        ));
    }
    steps.extend(account()); // Controller final current account after durable result.
    steps
}
fn commands(fixture: &Fixture) -> Vec<Value> {
    fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn assert_commands(fixture: &Fixture, count: usize, writes: usize) {
    let rows = commands(fixture);
    assert_eq!(rows.len(), count);
    assert!(rows.iter().all(|row| row["matched"] == true));
    assert_eq!(
        rows.iter().filter(|row| row["args"][5] == "POST").count(),
        writes
    );
}
fn git(fixture: &Fixture, args: &[&str]) -> Vec<u8> {
    let output = Command::new("git")
        .args(args)
        .current_dir(fixture.repo())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}
fn git_fixture(fixture: &Fixture) -> Vec<u8> {
    git(fixture, &["init", "-b", "main"]);
    git(fixture, &["config", "user.name", "Fixture"]);
    git(
        fixture,
        &["config", "user.email", "fixture@example.invalid"],
    );
    git(fixture, &["commit", "--allow-empty", "-m", "saved version"]);
    fs::write(fixture.repo().join("dirty.txt"), "preserve untracked bytes").unwrap();
    git(fixture, &["status", "--porcelain=v1", "-z"])
}

#[test]
fn actions_run_control_controller_metadata_flush_failure_blocks_before_claim_or_post() {
    let fixture = Fixture::new();
    let git_before = git_fixture(&fixture);
    fixture.scripted_gh(&account());
    let database = fixture.root.join("cache.sqlite3");
    let mut journal = ActionsWriteJournal::open(&database).unwrap();
    let plan = plan(&fixture);
    let result = execute(
        &provider(&fixture),
        &mut journal,
        &plan,
        true,
        None,
        |marker| {
            let row: (i64, String, u64) = Connection::open(&database)
                .unwrap()
                .query_row(
                    "SELECT attempt_id,state,claimed FROM actions_write_attempts",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .unwrap();
            assert_eq!(row, (marker.attempt_id, "pending".into(), 0));
            Err("fixture ordered metadata failure".into())
        },
        &CancellationToken::default(),
    );
    assert!(
        matches!(result,ActionsExecution::Refused{error,..} if error.contains("no effect was sent"))
    );
    assert_commands(&fixture, 2, 0);
    assert_eq!(
        Connection::open(&database)
            .unwrap()
            .query_row(
                "SELECT state,claimed FROM actions_write_attempts",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
            )
            .unwrap(),
        ("pending".into(), 0)
    );
    assert_eq!(
        git(&fixture, &["status", "--porcelain=v1", "-z"]),
        git_before
    );
    assert_eq!(
        fs::read_to_string(fixture.repo().join("dirty.txt")).unwrap(),
        "preserve untracked bytes"
    );
}

#[test]
fn actions_run_control_controller_unknown_restart_preserves_original_before_changed_fields() {
    let fixture = Fixture::new();
    let mut steps = mutation(true, "", "");
    steps.extend(account());
    steps.extend(account());
    fixture.scripted_gh(&steps);
    let database = fixture.root.join("cache.sqlite3");
    let mut journal = ActionsWriteJournal::open(&database).unwrap();
    let plan = plan(&fixture);
    let saved = RefCell::new(None);
    let outcome = execute(
        &provider(&fixture),
        &mut journal,
        &plan,
        true,
        None,
        |marker| {
            saved.replace(Some(marker.publication_reference()));
            Ok(())
        },
        &CancellationToken::default(),
    );
    assert!(matches!(
        outcome,
        ActionsExecution::Unknown {
            private_visible: true,
            ..
        }
    ));
    assert_commands(&fixture, 8, 1);
    drop(journal);
    Cache::open(&database)
        .unwrap()
        .clear_records(&crate::storage::Scope::github("github.invalid", Some(42)).unwrap())
        .unwrap();
    let mut reopened = ActionsWriteJournal::open(&database).unwrap();
    let mut edited = plan.clone();
    edited.command = ActionsCommand::SetSecret;
    edited.preview = vec!["changed fields must not imply a resend".into()];
    let restored = execute(
        &provider(&fixture),
        &mut reopened,
        &edited,
        true,
        saved.borrow().as_ref(),
        |_| panic!("original restoration must not reserve or flush another attempt"),
        &CancellationToken::default(),
    );
    let ActionsExecution::RestoredOriginal { marker } = restored else {
        panic!("{restored:?}")
    };
    assert_eq!(marker.kind, ActionsWriteKind::Cancel);
    assert_eq!(marker.state, ActionsWriteState::Uncertain);
    assert_eq!(marker.context.attempt, Some(3));
    assert!(marker.receipt.is_none());
    assert_commands(&fixture, 12, 1);
    assert_eq!(
        Connection::open(&database)
            .unwrap()
            .query_row("SELECT COUNT(*) FROM actions_write_attempts", [], |row| row
                .get::<_, u64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn actions_run_control_controller_confirmed_receipt_survives_failed_final_identity_without_resend()
{
    let fixture = Fixture::new();
    let mut steps = mutation(false, "completed", "cancelled");
    let last = steps.last_mut().unwrap();
    last.status = 1;
    last.stdout.clear();
    last.stderr = b"fixture auth expired after receipt".to_vec();
    fixture.scripted_gh(&steps);
    let database = fixture.root.join("cache.sqlite3");
    let mut journal = ActionsWriteJournal::open(&database).unwrap();
    let plan = plan(&fixture);
    let outcome = execute(
        &provider(&fixture),
        &mut journal,
        &plan,
        true,
        None,
        |_| Ok(()),
        &CancellationToken::default(),
    );
    assert!(matches!(
        outcome,
        ActionsExecution::EffectConfirmedButRefreshFailed {
            private_visible: false,
            outcome: None,
            ..
        }
    ));
    let row: (String, Value) = Connection::open(&database)
        .unwrap()
        .query_row(
            "SELECT state,receipt_json FROM actions_write_attempts",
            [],
            |row| {
                let text: String = row.get(1)?;
                Ok((row.get(0)?, serde_json::from_str(&text).unwrap()))
            },
        )
        .unwrap();
    assert_eq!(row.0, "confirmed");
    assert_eq!(row.1["resource_id"], 123);
    assert_commands(&fixture, 9, 1);
}

#[test]
fn actions_run_control_controller_receipt_sql_failure_retains_claimed_barrier_not_fake_success() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&mutation(false, "completed", "cancelled"));
    let database = fixture.root.join("cache.sqlite3");
    let mut journal = ActionsWriteJournal::open(&database).unwrap();
    Connection::open(&database).unwrap().execute_batch("CREATE TRIGGER fail_receipt BEFORE UPDATE OF state ON actions_write_attempts WHEN NEW.state='confirmed' BEGIN SELECT RAISE(ABORT,'fixture durable receipt failure'); END;").unwrap();
    let outcome = execute(
        &provider(&fixture),
        &mut journal,
        &plan(&fixture),
        true,
        None,
        |_| Ok(()),
        &CancellationToken::default(),
    );
    assert!(matches!(
        outcome,
        ActionsExecution::ReceiptPersistenceFailed {
            disposition: ActionsDisposition::VerifiedEffect { .. },
            private_visible: true,
            ..
        }
    ));
    assert_eq!(
        Connection::open(&database)
            .unwrap()
            .query_row(
                "SELECT state,claimed,receipt_json IS NULL FROM actions_write_attempts",
                [],
                |row| Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, u64>(1)?,
                    row.get::<_, bool>(2)?
                ))
            )
            .unwrap(),
        ("pending".into(), 1, true)
    );
    assert_commands(&fixture, 9, 1);
}

#[test]
fn actions_run_control_controller_accepted_pending_is_durable_not_confirmed_cancellation() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&mutation(false, "in_progress", ""));
    let database = fixture.root.join("cache.sqlite3");
    let mut journal = ActionsWriteJournal::open(&database).unwrap();
    let result = execute(
        &provider(&fixture),
        &mut journal,
        &plan(&fixture),
        true,
        None,
        |_| Ok(()),
        &CancellationToken::default(),
    );
    assert!(matches!(result, ActionsExecution::AcceptedPending { .. }));
    assert_commands(&fixture, 9, 1);
    let row: (String, u64) = Connection::open(&database)
        .unwrap()
        .query_row(
            "SELECT state,claimed FROM actions_write_attempts",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(row, ("accepted_pending".into(), 1));
    drop(journal);
    let reopened = ActionsWriteJournal::open(&database).unwrap();
    let target = ActionsWriteTarget::new(
        &provider(&fixture),
        &identity(),
        &ActionsTarget::Run {
            repository: "example/repo".into(),
            id: 123,
        },
    )
    .unwrap();
    assert_eq!(
        reopened
            .blocking(&target, ActionsWriteKind::Cancel)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn actions_secrets_controller_original_confirmed_lookup_needs_no_new_secret_body_or_head() {
    let fixture = Fixture::new();
    let mut steps = account();
    steps.extend(account());
    fixture.scripted_gh(&steps);
    let database = fixture.root.join("cache.sqlite3");
    let mut journal = ActionsWriteJournal::open(&database).unwrap();
    let target = ActionsWriteTarget::new(
        &provider(&fixture),
        &identity(),
        &ActionsTarget::Secret {
            scope: ActionsScope::Repository {
                repository: "example/repo".into(),
            },
            name: "DEPLOY_TOKEN".into(),
        },
    )
    .unwrap();
    let intent = ActionsWriteIntent::new(
        ActionsWriteKind::SetSecret,
        ActionsImmutableContext::default(),
        Default::default(),
        b"private-original-secret",
    )
    .unwrap();
    let permit = journal.begin(&target, &intent).unwrap();
    journal.claim(&permit, &target, &intent).unwrap();
    journal
        .mark_confirmed(
            &permit,
            &ActionsReceipt {
                kind: ActionsReceiptKind::SecretMetadata,
                resource_id: None,
                observed_attempt: None,
                observed_run_id: None,
                original_dispatch_run_v7: None,
                workflow_remote_v9: None,
            },
        )
        .unwrap();
    let reference = permit.marker().publication_reference();
    let result = restore_original(
        &provider(&fixture),
        &journal,
        &identity(),
        &reference,
        &CancellationToken::default(),
    );
    let ActionsExecution::RestoredOriginal { marker } = result else {
        panic!("{result:?}")
    };
    assert_eq!(marker.kind, ActionsWriteKind::SetSecret);
    assert_eq!(marker.state, ActionsWriteState::Confirmed);
    assert_commands(&fixture, 4, 0);
    assert!(!format!("{marker:?}").contains("private-original-secret"));
    Connection::open(&database)
        .unwrap()
        .execute(
            "UPDATE actions_write_attempts SET kind_json=?1",
            ["\"delete_secret\""],
        )
        .unwrap();
    assert!(
        journal
            .original_publication(&provider(&fixture), &identity(), &reference)
            .is_err(),
        "changed closed target/action metadata cannot reuse the old original fingerprint"
    );
}
