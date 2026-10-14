use super::*;
use crate::actions_write_journal::{ActionsWriteJournal, ActionsWritePermit};
use crate::test_support as support;
use serde_json::json;
use std::fs;
use support::{Fixture, GhStep};

fn step(endpoint: &str, method: &str, output: Value, page: bool) -> GhStep {
    let mut args = [
        "api",
        endpoint,
        "--hostname",
        "github.invalid",
        "--method",
        method,
    ]
    .map(str::to_owned)
    .to_vec();
    if page {
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
fn identity() -> HostIdentity {
    HostIdentity {
        host: "github.invalid".into(),
        login: "octocat".into(),
        account_id: Some(42),
        gh_version: "gh version 2.101.0".into(),
    }
}
fn account() -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        step("user", "GET", json!({"login":"octocat","id":42}), false),
    ]
}
fn provider(fixture: &Fixture) -> GithubProvider {
    GithubProvider::new("github.invalid")
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"))
}
fn plan(
    operation: ActionsOperation,
    command: ActionsCommand,
    target: ActionsTarget,
) -> ActionsPlan {
    ActionsPlan {
        operation,
        command,
        target: Some(target),
        identity: identity(),
        preview: vec![],
        workspace: ActionsWorkspace::new(identity(), vec!["example/repo".into()]),
    }
}
fn journal(fixture: &Fixture) -> Vec<Value> {
    fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn begin(
    fixture: &Fixture,
    plan: &ActionsPlan,
    provider: &GithubProvider,
) -> (ActionsWriteJournal, ActionsWritePermit) {
    let descriptor = plan.publication_descriptor(provider).unwrap().unwrap();
    let mut journal = ActionsWriteJournal::open(&fixture.root.join("actions.sqlite3")).unwrap();
    let permit = journal
        .begin(&descriptor.target, &descriptor.intent)
        .unwrap();
    journal
        .claim(&permit, &descriptor.target, &descriptor.intent)
        .unwrap();
    (journal, permit)
}
fn saved_state(fixture: &Fixture) -> (String, Option<String>) {
    let connection = rusqlite::Connection::open(fixture.root.join("actions.sqlite3")).unwrap();
    connection
        .query_row(
            "SELECT state,receipt_json FROM actions_write_attempts",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap()
}

#[test]
fn actions_cache_publication_records_verified_effect_before_optional_usage_failure() {
    let fixture = Fixture::new();
    let cache = ActionCache {
        id: 9,
        key: "linux-build".into(),
        ref_name: "refs/heads/main".into(),
        version: "v1".into(),
        size_in_bytes: 400,
        created_at: "created".into(),
        last_accessed_at: "used".into(),
    };
    let cache_json = json!({"id":9,"key":"linux-build","ref":"refs/heads/main","version":"v1","size_in_bytes":400,"created_at":"created","last_accessed_at":"used"});
    let mut steps = account();
    steps.extend([
        step("repos/example/repo/actions/caches?per_page=100&key=linux-build&ref=refs%2Fheads%2Fmain","GET",json!([{"actions_caches":[cache_json]}]),true),
        step("repos/example/repo/actions/caches/9","DELETE",Value::Null,false),
        step("repos/example/repo/actions/caches?per_page=100","GET",json!([{"actions_caches":[]}]),true),
    ]);
    let mut unavailable = step(
        "repos/example/repo/actions/cache/usage",
        "GET",
        Value::Null,
        false,
    );
    unavailable.status = 1;
    unavailable.stderr = b"offline usage unavailable".to_vec();
    steps.push(unavailable);
    fixture.scripted_gh(&steps);
    let provider = provider(&fixture);
    let plan = plan(
        ActionsOperation::DeleteCache(cache),
        ActionsCommand::DeleteCache,
        ActionsTarget::Cache {
            repository: "example/repo".into(),
            id: 9,
        },
    );
    let (mut durable, permit) = begin(&fixture, &plan, &provider);
    let published = plan
        .publish(&provider, true, &CancellationToken::default())
        .unwrap();
    assert_eq!(
        journal(&fixture).len(),
        5,
        "optional usage must not run before receipt persistence"
    );
    let ActionsDisposition::VerifiedEffect { receipt } = &published.disposition else {
        panic!("Exact deletion must have a verified receipt");
    };
    assert_eq!(receipt.kind, ActionsReceiptKind::CacheDeleted);
    assert_eq!(receipt.resource_id, Some(9));
    durable.mark_confirmed(&permit, receipt).unwrap();
    assert_eq!(saved_state(&fixture).0, "confirmed");
    let refreshed = published.refresh_checked(&provider, &CancellationToken::default());
    assert!(refreshed.refresh_failed);
    let outcome = refreshed.outcome;
    assert!(outcome.workspace.cache_usage.is_none());
    assert!(outcome
        .workspace
        .warnings
        .iter()
        .any(|warning| warning.contains("usage refresh is unavailable")));
    assert_eq!(saved_state(&fixture).0, "confirmed");
    assert_eq!(journal(&fixture).len(), 6);
    assert_eq!(
        journal(&fixture)
            .iter()
            .filter(|row| row["args"][5] == "DELETE")
            .count(),
        1
    );
    assert!(journal(&fixture).iter().all(|row| row["matched"] == true));
}

fn deployment_fixture() -> (Fixture, ActionsPlan) {
    let fixture = Fixture::new();
    let run = WorkflowRun {
        id: 123,
        workflow_id: 7,
        name: "CI".into(),
        title: "Fixture".into(),
        head_sha: "a".repeat(40),
        head_branch: "feature".into(),
        event: "push".into(),
        status: "waiting".into(),
        conclusion: String::new(),
        attempt: 2,
        actor: Actor::default(),
        triggering_actor: Actor::default(),
        created_at: String::new(),
        updated_at: String::new(),
        url: String::new(),
    };
    let run_json = json!({"id":123,"workflow_id":7,"name":"CI","head_sha":"a".repeat(40),"head_branch":"feature","event":"push","status":"waiting","conclusion":null,"run_attempt":2});
    let mut steps = account();
    steps.extend([
        step("repos/example/repo/actions/runs/123","GET",run_json,false),
        step("repos/example/repo/actions/runs/123/pending_deployments","GET",json!([{"environment":{"id":9,"name":"production"},"current_user_can_approve":true,"reviewers":[],"wait_timer":0,"wait_timer_started_at":null}]),false),
    ]);
    let mut publish = step(
        "repos/example/repo/actions/runs/123/pending_deployments",
        "POST",
        json!([]),
        false,
    );
    publish.args.extend(["--input".into(), "-".into()]);
    publish.stdin =
        serde_json::to_vec(&json!({"environment_ids":[9],"state":"approved","comment":"reviewed"}))
            .unwrap();
    steps.push(publish);
    steps.push(step(
        "repos/example/repo/actions/runs/123/pending_deployments",
        "GET",
        json!([]),
        false,
    ));
    for endpoint in [
        "repos/example/repo/actions/runs/123",
        "repos/example/repo/actions/runs/123/approvals",
    ] {
        let mut failed = step(endpoint, "GET", Value::Null, false);
        failed.status = 1;
        failed.stderr = b"offline follow-up unavailable".to_vec();
        steps.push(failed);
    }
    fixture.scripted_gh(&steps);
    let target = ActionsTarget::DeploymentReview {
        repository: "example/repo".into(),
        run_id: 123,
        attempt: 2,
        head_sha: "a".repeat(40),
        environment_ids: vec![9],
    };
    let plan = plan(
        ActionsOperation::Deployment {
            run,
            review: DeploymentReview {
                environment_ids: vec![9],
                decision: DeploymentDecision::Approved,
                comment: "reviewed".into(),
            },
        },
        ActionsCommand::ApproveDeployment,
        target,
    );
    (fixture, plan)
}

#[test]
fn actions_approvals_publication_records_acceptance_before_optional_run_history_failures() {
    let (fixture, plan) = deployment_fixture();
    let provider = provider(&fixture);
    let (mut durable, permit) = begin(&fixture, &plan, &provider);
    let published = plan
        .publish(&provider, true, &CancellationToken::default())
        .unwrap();
    assert_eq!(
        journal(&fixture).len(),
        6,
        "run/history are optional after typed acceptance"
    );
    let ActionsDisposition::Accepted { receipt, .. } = &published.disposition else {
        panic!("Gate absence cannot prove correlated completed decision");
    };
    assert_eq!(receipt.kind, ActionsReceiptKind::DeploymentAccepted);
    assert_eq!(receipt.observed_attempt, Some(2));
    durable.mark_accepted_pending(&permit, receipt).unwrap();
    assert_eq!(saved_state(&fixture).0, "accepted_pending");
    let refreshed = published.refresh_checked(&provider, &CancellationToken::default());
    assert!(refreshed.refresh_failed);
    let outcome = refreshed.outcome;
    assert!(outcome
        .workspace
        .warnings
        .iter()
        .any(|warning| warning.contains("run refresh unavailable")));
    assert!(outcome.workspace.approval_history_error.is_some());
    assert_eq!(saved_state(&fixture).0, "accepted_pending");
    assert_eq!(journal(&fixture).len(), 8);
    assert_eq!(
        journal(&fixture)
            .iter()
            .filter(|row| row["args"][5] == "POST")
            .count(),
        1
    );
    assert!(journal(&fixture).iter().all(|row| row["matched"] == true));
}

#[test]
fn actions_approvals_publication_storage_failure_retains_pending_without_optional_reads_or_resend()
{
    let (fixture, plan) = deployment_fixture();
    let provider = provider(&fixture);
    let (mut durable, permit) = begin(&fixture, &plan, &provider);
    let published = plan
        .publish(&provider, true, &CancellationToken::default())
        .unwrap();
    let connection = rusqlite::Connection::open(fixture.root.join("actions.sqlite3")).unwrap();
    connection.execute_batch("CREATE TRIGGER receipt_failure BEFORE UPDATE OF receipt_json ON actions_write_attempts BEGIN SELECT RAISE(ABORT,'fixture storage failed'); END;").unwrap();
    let ActionsDisposition::Accepted { receipt, .. } = &published.disposition else {
        panic!("Accepted deployment receipt required");
    };
    assert!(durable.mark_accepted_pending(&permit, receipt).is_err());
    assert_eq!(saved_state(&fixture).0, "pending");
    assert!(saved_state(&fixture).1.is_none());
    assert_eq!(journal(&fixture).len(), 6);
    let descriptor = plan.publication_descriptor(&provider).unwrap().unwrap();
    assert!(!durable
        .blocking(&descriptor.target, descriptor.intent.kind)
        .unwrap()
        .is_empty());
    assert!(durable
        .begin(&descriptor.target, &descriptor.intent)
        .is_err());
    assert_eq!(
        journal(&fixture)
            .iter()
            .filter(|row| row["args"][5] == "POST")
            .count(),
        1
    );
}

#[test]
fn actions_run_control_publication_refresh_status_does_not_parse_existing_warnings() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[]);
    let provider = provider(&fixture);
    let mut workspace = ActionsWorkspace::new(identity(), vec!["example/repo".into()]);
    workspace
        .warnings
        .push("Earlier unrelated refresh was unavailable; this is not a new I/O failure".into());
    let published = ActionsPublication {
        message: "Exact effect has a typed receipt, never inferred from this string".into(),
        workspace,
        disposition: verified_receipt(ActionsReceiptKind::CacheDeleted, Some(9)),
        refresh: PostPublicationRefresh::None,
    };
    let refreshed = published.refresh_checked(&provider, &CancellationToken::default());
    assert!(!refreshed.refresh_failed);
    assert!(journal(&fixture).is_empty());
    assert!(refreshed
        .outcome
        .workspace
        .warnings
        .iter()
        .any(|warning| warning.contains("Earlier unrelated")));
}

#[test]
fn actions_settings_local_scope_publication_from_empty_catalogue_stays_confirmed_read_only() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[step(
        "orgs/example/actions/secrets?per_page=100",
        "GET",
        json!([{"secrets":[]}]),
        true,
    )]);
    let provider = provider(&fixture);
    let workspace = ActionsWorkspace::new(identity(), vec![]);
    assert!(workspace
        .resource_key(ActionsCommand::ChooseScope)
        .unwrap()
        .contains("settings-scope"));
    let values = BTreeMap::from([
        ("kind".into(), "organization".into()),
        ("name".into(), "example".into()),
    ]);
    let prepared = ActionsPlan::prepare(
        &workspace,
        ActionsCommand::ChooseScope,
        &values,
        &provider,
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(prepared
        .publication_descriptor(&provider)
        .unwrap()
        .is_none());
    assert!(prepared.preview.iter().any(|line| line == "name: example"));
    assert!(prepared
        .publish(&provider, false, &CancellationToken::default())
        .is_err());
    assert!(journal(&fixture).is_empty());
    let published = prepared
        .publish(&provider, true, &CancellationToken::default())
        .unwrap();
    assert!(matches!(
        published.disposition,
        ActionsDisposition::LocalOnly
    ));
    assert!(
        matches!(published.workspace.scope,ActionsScope::Organization {ref organization} if organization == "example")
    );
    assert!(published.workspace.repositories.is_empty());
    let refreshed = published.refresh_checked(&provider, &CancellationToken::default());
    assert!(!refreshed.refresh_failed);
    let rows = journal(&fixture);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["matched"], true);
    assert_eq!(rows[0]["args"][5], "GET");
    assert!(
        !fixture.root.join("actions.sqlite3").exists(),
        "Read-only scope selection never creates a publication marker"
    );
}
