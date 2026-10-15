#[path = "provider_contracts/actions_accepted_transitions.rs"]
mod actions_accepted_transitions;
#[path = "provider_contracts/actions_approval_recovery.rs"]
mod actions_approval_recovery;
#[path = "provider_contracts/actions_journal_proofs.rs"]
mod actions_journal_proofs;
#[path = "provider_contracts/actions_settings.rs"]
mod actions_settings;
#[path = "provider_contracts/actions_typed_receipts.rs"]
mod actions_typed_receipts;
#[path = "provider_contracts/capture_accounting.rs"]
mod capture_accounting;
#[path = "provider_contracts/collaboration_index.rs"]
mod collaboration_index;
#[path = "provider_contracts/inbox_exact_v1.rs"]
mod inbox_exact_v1;
#[path = "provider_contracts/inline_receipts_v1.rs"]
mod inline_receipts_v1;
#[path = "provider_contracts/local_commit_actor_v1.rs"]
mod local_commit_actor_v1;
#[path = "provider_contracts/metadata_actions.rs"]
mod metadata_actions;
#[path = "provider_contracts/metadata_branch.rs"]
mod metadata_branch;
#[path = "provider_contracts/metadata_catalogue.rs"]
mod metadata_catalogue;
#[path = "provider_contracts/metrics.rs"]
mod metrics;
#[path = "provider_contracts/metrics_access.rs"]
mod metrics_access;
#[path = "provider_contracts/pr_checks_bridge.rs"]
mod pr_checks_bridge;
#[path = "provider_contracts/principal_freshness.rs"]
mod principal_freshness;
#[path = "provider_contracts/read_access.rs"]
mod read_access;
#[path = "provider_contracts/remote_bounded.rs"]
mod remote_bounded;
#[path = "provider_contracts/repository_catalogue.rs"]
mod repository_catalogue;
#[path = "provider_contracts/repository_catalogue_access_v1.rs"]
mod repository_catalogue_access_v1;
#[path = "support/providers.rs"]
mod support;
use ghtui::providers::gh::{
    verify_blob_digest, ActionCache, ActionsArtifact, ActionsScope, ActionsTarget, ActionsVariable,
    Actor, AllowedActions, ArtifactArchive, ArtifactLimits, CommentKind, ConversationComment,
    CreatePr, CreateRunnerGroup, DeploymentBranchMode, DeploymentDecision, DeploymentPatternEdit,
    DeploymentPatternType, DeploymentReview, DiffSide, DispatchInputType, EnvironmentEdit,
    EnvironmentReviewer, GithubErrorKind, GithubProvider, GroupAssociation, GroupAssociationEdit,
    HostIdentity, InlineComment, IssueMetadataEdit, LockReason, PolicyEdit, PolicySection,
    PrSummary, PrTextEdit, PullRequest, Queue, Reaction, RemoteSearchItem, RemoteSearchKind,
    ReviewEvent, ReviewerSelection, RunFilter, RunMutation, RunnerGroup, RunnerGroupEdit,
    RunnerLabel, ScopeVisibility, SecretUpdate, SelfHostedRunner, SettingMembershipEdit,
    Subscription, VariableEdit, Workflow, WorkflowDefinition, WorkflowRun, PR_MERGE_CONTEXT_QUERY,
};
use ghtui::providers::runner::{
    redact, sanitize_terminal, CancellationToken, CommandRunner, CommandSpec, FailureKind,
};
use ghtui::search::{Query, SearchMode};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Write};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};
use support::{Fixture, GhStep};

fn runner_spec(fixture: &Fixture, args: &[&str]) -> CommandSpec {
    CommandSpec::new(fixture.root.join("bin/gh"))
        .args(args.iter().copied())
        .env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"))
}
fn provider(fixture: &Fixture) -> GithubProvider {
    GithubProvider::new("github.invalid")
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"))
}
fn journal(fixture: &Fixture) -> Vec<Value> {
    fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn identity() -> HostIdentity {
    HostIdentity {
        host: "github.invalid".into(),
        login: "octocat".into(),
        account_id: None,
        gh_version: "gh version 2.101.0".into(),
    }
}
fn api_args(endpoint: &str) -> Vec<String> {
    vec![
        "api".into(),
        endpoint.into(),
        "--hostname".into(),
        "github.invalid".into(),
        "--method".into(),
        "GET".into(),
    ]
}
fn step(args: Vec<String>, out: Value) -> GhStep {
    GhStep {
        args,
        stdout: serde_json::to_vec(&out).unwrap(),
        stdin: vec![],
        stderr: vec![],
        status: 0,
        delay_ms: 0,
    }
}
fn search_step(queue: &str, items: Vec<Value>, total: u64, incomplete: bool) -> GhStep {
    let mut args = api_args("search/issues");
    args.extend([
        "-f".into(),
        format!("q=is:pr is:open {queue}:octocat"),
        "-f".into(),
        "per_page=100".into(),
        "-f".into(),
        "page=1".into(),
    ]);
    step(
        args,
        json!({"total_count":total,"incomplete_results":incomplete,"items":items}),
    )
}
fn pr(n: u64, repo: &str) -> Value {
    json!({"node_id":format!("PR_{repo}_{n}"),"number":n,"title":format!("PR {n}"),"body":"details","repository_url":format!("https://api.github.invalid/repos/{repo}"),"html_url":format!("https://github.invalid/{repo}/pull/{n}"),"user":{"login":"alice","type":"User","avatar_url":"https://avatars.invalid/u/1"},"state":"open","draft":false,"updated_at":"2026-10-12T00:00:00Z","head":{"sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","ref":"feature"},"base":{"sha":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","ref":"main"}})
}

fn target() -> PrSummary {
    PrSummary {
        pr: PullRequest {
            host: "github.invalid".into(),
            id: "PR_example/repo_7".into(),
            repository: "example/repo".into(),
            number: 7,
            title: "PR 7".into(),
            body: "details".into(),
            url: "https://github.invalid/example/repo/pull/7".into(),
            author: Actor::default(),
            state: "open".into(),
            draft: false,
            created_at: String::new(),
            updated_at: "2026-10-12T00:00:00Z".into(),
            queues: Default::default(),
        },
        head_sha: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
        base_sha: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
        head_ref: "feature".into(),
        base_ref: "main".into(),
        merged: false,
        auto_merge: false,
        merge_commit_sha: None,
        mergeable: Some(true),
        mergeable_state: "clean".into(),
        additions: 0,
        deletions: 0,
        changed_files: 0,
    }
}
fn guard_steps() -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        step(api_args("user"), json!({"login":"octocat"})),
        step(
            api_args("repos/example/repo/pulls/7"),
            pr(7, "example/repo"),
        ),
    ]
}
fn write_step(endpoint: &str, method: &str, payload: Option<Value>, out: Value) -> GhStep {
    let mut args = api_args(endpoint);
    args[5] = method.into();
    let mut step = step(args, out);
    if let Some(payload) = payload {
        step.args.extend(["--input".into(), "-".into()]);
        step.stdin = serde_json::to_vec(&payload).unwrap();
    }
    step
}
fn page_step(endpoint: &str, pages: Value) -> GhStep {
    let mut args = api_args(endpoint);
    args.extend(["--paginate".into(), "--slurp".into()]);
    step(args, pages)
}
fn account_steps(account_id: u64) -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        step(api_args("user"), json!({"login":"octocat","id":account_id})),
    ]
}
fn verified_identity() -> HostIdentity {
    HostIdentity {
        account_id: Some(42),
        ..identity()
    }
}
fn empty_pr_detail_steps(summary: Value) -> Vec<GhStep> {
    vec![
        step(api_args("repos/example/repo/pulls/7"), summary.clone()),
        page_step(
            "repos/example/repo/issues/7/comments?per_page=100",
            json!([[]]),
        ),
        page_step(
            "repos/example/repo/pulls/7/commits?per_page=100",
            json!([[]]),
        ),
        page_step("repos/example/repo/pulls/7/files?per_page=100", json!([[]])),
        page_step(
            "repos/example/repo/pulls/7/reviews?per_page=100",
            json!([[]]),
        ),
        page_step(
            "repos/example/repo/issues/7/timeline?per_page=100",
            json!([[]]),
        ),
        step(api_args("repos/example/repo/pulls/7"), summary),
    ]
}
fn run_value() -> Value {
    json!({"id":123,"workflow_id":42,"name":"CI","head_sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","head_branch":"feature","event":"pull_request","status":"completed","conclusion":"failure","run_attempt":2,"actor":{"login":"alice","avatar_url":"https://avatars.invalid/1"},"triggering_actor":{"login":"octocat"}})
}
fn run_target() -> WorkflowRun {
    serde_json::from_value(json!({"id":123,"workflow_id":42,"name":"CI","title":"build","head_sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","head_branch":"feature","event":"pull_request","status":"completed","conclusion":"failure","attempt":2,"actor":{"login":"alice","display_name":null,"avatar_url":null,"actor_type":"User"},"triggering_actor":{"login":"octocat","display_name":null,"avatar_url":null,"actor_type":"User"},"created_at":"","updated_at":"","url":""})).unwrap()
}
fn run_guard(value: Value) -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        step(api_args("user"), json!({"login":"octocat"})),
        step(api_args("repos/example/repo/actions/runs/123"), value),
    ]
}

#[test]
fn pr_context_exposes_permissions_forks_metadata_and_requested_actors_without_hiding_staleness() {
    let fixture = Fixture::new();
    let mut raw = pr(7, "example/repo");
    raw["head"]["repo"] = json!({"id":9,"full_name":"alice/fork","private":true});
    raw["base"]["repo"] = json!({"id":8,"full_name":"example/repo"});
    raw["requested_reviewers"] =
        json!([{"login":"reviewer","avatar_url":"https://avatars.invalid/2"}]);
    raw["requested_teams"] = json!([{"id":3,"slug":"security","name":"Security"}]);
    raw["maintainer_can_modify"] = json!(true);
    fixture.scripted_gh(&[step(api_args("repos/example/repo/pulls/7"),raw),step(api_args("repos/example/repo/issues/7"),json!({"number":7,"labels":[{"name":"bug"}],"assignees":[{"login":"octocat"}]})),step(api_args("repos/example/repo"),json!({"id":8,"full_name":"example/repo","permissions":{"push":true,"admin":false},"default_branch":"main"})),step(api_args("repos/example/repo/pulls/7"),pr(7,"example/repo"))]);
    let context = provider(&fixture)
        .pr_context(&target(), &CancellationToken::default())
        .unwrap();
    assert!(context.repository.permissions.push);
    assert!(context.is_cross_repository);
    assert_eq!(context.head_repository.unwrap().full_name, "alice/fork");
    assert_eq!(context.requested_reviewers[0].label(), "@reviewer");
    assert_eq!(context.requested_teams[0].slug, "security");
    assert_eq!(context.issue_metadata["labels"][0]["name"], "bug");
    assert_eq!(context.source_urls.len(), 2);
    assert_eq!(journal(&fixture).len(), 4);
    let fixture = Fixture::new();
    let mut raw = pr(7, "example/repo");
    raw["base"]["sha"] = json!("cccccccccccccccccccccccccccccccccccccccc");
    fixture.scripted_gh(&[step(api_args("repos/example/repo/pulls/7"), raw)]);
    assert!(matches!(
        provider(&fixture)
            .pr_context(&target(), &CancellationToken::default())
            .unwrap_err()
            .kind,
        GithubErrorKind::StaleHead
    ));
    assert_eq!(journal(&fixture).len(), 1);
}
#[test]
fn actions_inventory_filter_encodes_refs_keeps_membership_and_exposes_search_caps() {
    let fixture = Fixture::new();
    let mut matching = run_value();
    matching["head_branch"] = json!("feature/fix");
    fixture.scripted_gh(&[page_step("repos/example/repo/actions/workflows/42/runs?per_page=100&branch=feature%2Ffix&event=pull_request",json!([{"total_count":1001,"workflow_runs":[matching]}]))]);
    let filter = RunFilter {
        workflow_id: Some(42),
        branch: Some("feature/fix".into()),
        event: Some("pull_request".into()),
        ..RunFilter::default()
    };
    let catalog = provider(&fixture)
        .run_catalog("example/repo", &filter, &CancellationToken::default())
        .unwrap();
    assert!(!catalog.coverage.complete);
    assert!(catalog.coverage.reasons.iter().any(|s| s.contains("1,000")));
    assert_eq!(catalog.runs[0].triggering_actor.label(), "@octocat");
    assert_eq!(catalog.runs[0].attempt, 2);
    assert!(journal(&fixture)[0]["matched"].as_bool().unwrap());
}
#[test]
fn actions_attempt_jobs_steps_annotations_and_logs_preserve_exact_context() {
    let fixture = Fixture::new();
    let job = json!({"id":501,"run_id":123,"run_attempt":2,"head_sha":run_target().head_sha,"name":"test (linux, 1.94)","status":"completed","conclusion":"failure","runner_name":"hosted","labels":["ubuntu-latest"],"check_run_url":"https://github.invalid/api/v3/repos/example/repo/check-runs/600","steps":[{"number":2,"name":"tests","status":"completed","conclusion":"failure"}]});
    fixture.scripted_gh(&[step(api_args("repos/example/repo/actions/runs/123/attempts/2"),run_value()),page_step("repos/example/repo/actions/runs/123/attempts/2/jobs?per_page=100",json!([{"jobs":[job]}])),page_step("repos/example/repo/check-runs/600/annotations?per_page=100",json!([[{"path":"src/lib.rs","start_line":4,"end_line":5,"annotation_level":"failure","message":"failed test","title":"tests"}]])),GhStep::read(&["run","view","123","--repo","github.invalid/example/repo","--attempt","2","--log","--job","501"],"\u{1b}[31mfailed\u{1b}[0m ghp_abc123\n")]);
    let client = provider(&fixture);
    let attempt = client
        .run_attempt("example/repo", 123, 2, &CancellationToken::default())
        .unwrap();
    assert_eq!(attempt.jobs[0].steps[0].number, 2);
    assert_eq!(attempt.jobs[0].name, "test (linux, 1.94)");
    assert!(attempt.definition_note.contains("not explicit"));
    let annotations = client
        .job_annotations(
            "example/repo",
            &attempt.jobs[0],
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(annotations[0].start_line, 4);
    assert_eq!(annotations[0].level, "failure");
    let logs = client
        .run_logs_at(
            "example/repo",
            &attempt.run,
            Some(&attempt.jobs[0]),
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(!logs.contains('\u{1b}'));
    assert!(!logs.contains("ghp_abc123"));
    assert!(logs.contains("failed"));
    assert_eq!(journal(&fixture).len(), 4);
}
#[test]
fn actions_attempt_rejects_cross_run_jobs_and_log_targets() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[step(api_args("repos/example/repo/actions/runs/123/attempts/2"),run_value()),page_step("repos/example/repo/actions/runs/123/attempts/2/jobs?per_page=100",json!([{"jobs":[{"id":501,"run_id":999,"run_attempt":2,"head_sha":run_target().head_sha}]}]))]);
    assert!(matches!(
        provider(&fixture)
            .run_attempt("example/repo", 123, 2, &CancellationToken::default())
            .unwrap_err()
            .kind,
        GithubErrorKind::Json
    ));
    assert_eq!(journal(&fixture).len(), 2);
}
#[test]
fn actions_run_control_rerun_debug_specific_job_uses_actual_job_id_and_reports_pending_acceptance()
{
    let fixture = Fixture::new();
    let mut steps = run_guard(run_value());
    steps.extend([
        step(
            api_args("repos/example/repo/actions/jobs/501"),
            json!({"id":501,"run_id":123,"run_attempt":2,"head_sha":run_target().head_sha}),
        ),
        write_step(
            "repos/example/repo/actions/jobs/501/rerun",
            "POST",
            Some(json!({"enable_debug_logging":true})),
            Value::Null,
        ),
        step(api_args("repos/example/repo/actions/runs/123"), run_value()),
    ]);
    fixture.scripted_gh(&steps);
    let outcome = provider(&fixture)
        .mutate_run(
            &identity(),
            "example/repo",
            &run_target(),
            RunMutation::RerunJob {
                job_id: 501,
                debug: true,
            },
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(outcome.accepted);
    assert!(!outcome.completed);
    assert!(outcome.explanation.contains("do not issue"));
    assert_eq!(journal(&fixture).len(), 6);
    assert_eq!(
        journal(&fixture)[4]["args"][1],
        "repos/example/repo/actions/jobs/501/rerun"
    );
}
#[test]
fn actions_run_controls_reject_stale_attempts_and_completed_cancel_before_write() {
    let fixture = Fixture::new();
    let mut stale = run_value();
    stale["run_attempt"] = json!(3);
    fixture.scripted_gh(&run_guard(stale));
    assert!(matches!(
        provider(&fixture)
            .mutate_run(
                &identity(),
                "example/repo",
                &run_target(),
                RunMutation::RerunAll { debug: false },
                &CancellationToken::default()
            )
            .unwrap_err()
            .kind,
        GithubErrorKind::StaleHead
    ));
    assert_eq!(journal(&fixture).len(), 3);
    let fixture = Fixture::new();
    fixture.scripted_gh(&run_guard(run_value()));
    assert!(matches!(
        provider(&fixture)
            .mutate_run(
                &identity(),
                "example/repo",
                &run_target(),
                RunMutation::Cancel,
                &CancellationToken::default()
            )
            .unwrap_err()
            .kind,
        GithubErrorKind::InvalidTarget
    ));
    assert_eq!(journal(&fixture).len(), 3);
}
#[test]
fn actions_run_control_cancel_and_delete_target_one_run_and_confirm_refreshed_effects() {
    let fixture = Fixture::new();
    let mut running = run_value();
    running["status"] = json!("in_progress");
    running["conclusion"] = Value::Null;
    let mut target = run_target();
    target.status = "in_progress".into();
    target.conclusion = String::new();
    let mut completed = run_value();
    completed["conclusion"] = json!("cancelled");
    let mut steps = run_guard(running);
    steps.extend([
        write_step(
            "repos/example/repo/actions/runs/123/force-cancel",
            "POST",
            None,
            Value::Null,
        ),
        step(api_args("repos/example/repo/actions/runs/123"), completed),
    ]);
    fixture.scripted_gh(&steps);
    assert!(
        provider(&fixture)
            .mutate_run(
                &identity(),
                "example/repo",
                &target,
                RunMutation::ForceCancel,
                &CancellationToken::default()
            )
            .unwrap()
            .completed
    );
    let fixture = Fixture::new();
    let mut steps = run_guard(run_value());
    steps.extend([
        write_step(
            "repos/example/repo/actions/runs/123",
            "DELETE",
            None,
            Value::Null,
        ),
        page_step(
            "repos/example/repo/actions/runs?per_page=100",
            json!([{"workflow_runs":[{"id":124,"workflow_id":42,"run_attempt":1,"head_sha":run_target().head_sha}]}]),
        ),
    ]);
    fixture.scripted_gh(&steps);
    let result = provider(&fixture)
        .mutate_run(
            &identity(),
            "example/repo",
            &run_target(),
            RunMutation::DeleteRun,
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(result.completed);
    assert!(result.run.is_none());
    assert_eq!(journal(&fixture).len(), 5);
}
#[test]
fn actions_cache_selected_delete_keeps_unrelated_entries_and_units() {
    let fixture = Fixture::new();
    let cache = ActionCache {
        id: 77,
        key: "linux/node".into(),
        ref_name: "refs/heads/main".into(),
        version: "v1".into(),
        size_in_bytes: 2048,
        created_at: "date".into(),
        last_accessed_at: "date2".into(),
    };
    let raw = json!({"id":77,"key":"linux/node","ref":"refs/heads/main","version":"v1","size_in_bytes":2048,"created_at":"date","last_accessed_at":"date2"});
    fixture.scripted_gh(&[step(api_args("repos/example/repo/actions/cache/usage"),json!({"active_caches_count":2,"active_caches_size_in_bytes":4096})),GhStep::read(&["--version"],"gh version 2.101.0\n"),step(api_args("user"),json!({"login":"octocat"})),page_step("repos/example/repo/actions/caches?per_page=100&key=linux%2Fnode&ref=refs%2Fheads%2Fmain",json!([{"actions_caches":[raw]}])),write_step("repos/example/repo/actions/caches/77","DELETE",None,Value::Null),page_step("repos/example/repo/actions/caches?per_page=100",json!([{"actions_caches":[{"id":78,"key":"other","size_in_bytes":2048}]}]))]);
    let client = provider(&fixture);
    assert_eq!(
        client
            .cache_usage("example/repo", &CancellationToken::default())
            .unwrap()
            .active_caches_size_in_bytes,
        4096
    );
    let remaining = client
        .delete_cache(
            &identity(),
            "example/repo",
            &cache,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(remaining[0].id, 78);
    assert_eq!(journal(&fixture).len(), 6);
}
#[test]
fn actions_approvals_deployment_review_checks_eligibility_exact_environment_ids_and_partial_state()
{
    let fixture = Fixture::new();
    let gate = json!({"environment":{"id":9,"name":"production"},"current_user_can_approve":true,"wait_timer":30,"reviewers":[{"type":"User","reviewer":{"login":"octocat"}}]});
    let mut steps = run_guard(run_value());
    steps.extend([
        step(
            api_args("repos/example/repo/actions/runs/123/pending_deployments"),
            json!([gate.clone()]),
        ),
        write_step(
            "repos/example/repo/actions/runs/123/pending_deployments",
            "POST",
            Some(json!({"environment_ids":[9],"state":"approved","comment":"ship"})),
            json!([{"id":44,"environment":"production","creator":{"login":"octocat"}}]),
        ),
        step(
            api_args("repos/example/repo/actions/runs/123/pending_deployments"),
            json!([gate]),
        ),
    ]);
    fixture.scripted_gh(&steps);
    let request = DeploymentReview {
        environment_ids: vec![9],
        decision: DeploymentDecision::Approved,
        comment: "ship".into(),
    };
    let result = provider(&fixture)
        .review_deployments(
            &identity(),
            "example/repo",
            &run_target(),
            &request,
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(!result.completed);
    assert_eq!(result.deployments[0]["creator"]["login"], "octocat");
    assert_eq!(result.remaining[0].wait_timer_minutes, 30);
    assert_eq!(journal(&fixture).len(), 6);
    let fixture = Fixture::new();
    let mut steps = run_guard(run_value());
    steps.push(step(
        api_args("repos/example/repo/actions/runs/123/pending_deployments"),
        json!([{"environment":{"id":9,"name":"production"},"current_user_can_approve":false}]),
    ));
    fixture.scripted_gh(&steps);
    assert!(matches!(
        provider(&fixture)
            .review_deployments(
                &identity(),
                "example/repo",
                &run_target(),
                &request,
                &CancellationToken::default()
            )
            .unwrap_err()
            .kind,
        GithubErrorKind::Permission
    ));
    assert_eq!(journal(&fixture).len(), 4);
}

#[test]
fn actions_approvals_history_preserves_exact_environment_ids_durable_actors_and_missing_host_fields(
) {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[step(api_args("repos/example/repo/actions/runs/123/approvals"),json!([
        {"state":"approved","comment":"ship","environments":[{"id":9,"name":"production"}],"user":{"id":42,"login":"octocat","type":"User"}},
        {"state":"future-state","comment":"unknown state preserved","environments":[{"id":10,"name":"staging"}],"user":null}
    ]))]);
    let history = provider(&fixture)
        .deployment_review_history("example/repo", 123, &CancellationToken::default())
        .unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].account_id, Some(42));
    assert_eq!(history[0].actor.login, "octocat");
    assert_eq!(history[0].environments[0].id, 9);
    assert_eq!(history[1].state, "future-state");
    assert_eq!(history[1].account_id, None);
    assert_eq!(history[1].actor.actor_type, "Unknown");
    assert_eq!(journal(&fixture).len(), 1);
    for invalid in [
        json!({}),
        json!([{"state":"approved","environments":[{"id":0}]}]),
        json!([{"state":"approved","environments":[{"id":9},{"id":9}]}]),
        json!([{"environments":[{"id":9}]}]),
    ] {
        let fixture = Fixture::new();
        fixture.scripted_gh(&[step(
            api_args("repos/example/repo/actions/runs/123/approvals"),
            invalid,
        )]);
        assert!(matches!(
            provider(&fixture)
                .deployment_review_history("example/repo", 123, &CancellationToken::default())
                .unwrap_err()
                .kind,
            GithubErrorKind::Json
        ));
        assert_eq!(journal(&fixture).len(), 1);
    }
}

#[test]
fn actions_approvals_stale_removed_duplicate_and_denied_environment_gates_never_publish() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[step(
        api_args("repos/example/repo/actions/runs/123/pending_deployments"),
        json!([
            {"environment":{"id":9,"name":"production"}},
            {"environment":{"id":9,"name":"same id twice"}}
        ]),
    )]);
    assert!(matches!(
        provider(&fixture)
            .pending_deployments("example/repo", 123, &CancellationToken::default())
            .unwrap_err()
            .kind,
        GithubErrorKind::Json
    ));
    assert_eq!(journal(&fixture).len(), 1);
    for eligible in [false, true] {
        let fixture = Fixture::new();
        let mut steps = run_guard(run_value());
        steps.push(step(api_args("repos/example/repo/actions/runs/123/pending_deployments"),json!([{"environment":{"id":9,"name":"production"},"current_user_can_approve":eligible}])));
        fixture.scripted_gh(&steps);
        let request = DeploymentReview {
            environment_ids: if eligible { vec![9, 10] } else { vec![9] },
            decision: DeploymentDecision::Approved,
            comment: "ship".into(),
        };
        let error = provider(&fixture)
            .review_deployments(
                &identity(),
                "example/repo",
                &run_target(),
                &request,
                &CancellationToken::default(),
            )
            .unwrap_err();
        if eligible {
            assert!(matches!(error.kind, GithubErrorKind::StaleHead));
        } else {
            assert!(matches!(error.kind, GithubErrorKind::Permission));
        }
        assert_eq!(journal(&fixture).len(), 4);
        assert!(journal(&fixture).iter().all(|row| row["matched"] == true));
    }
}

#[test]
fn actions_approvals_fork_control_requires_pull_request_and_observes_only_selected_run_transition()
{
    for event in ["pull_request", "push"] {
        let fixture = Fixture::new();
        let mut awaiting = run_value();
        awaiting["event"] = json!(event);
        awaiting["conclusion"] = json!("action_required");
        let mut target = run_target();
        target.event = event.into();
        target.conclusion = "action_required".into();
        let mut steps = run_guard(awaiting.clone());
        if event == "pull_request" {
            let mut queued = awaiting;
            queued["status"] = json!("queued");
            queued["conclusion"] = Value::Null;
            steps.extend([
                write_step(
                    "repos/example/repo/actions/runs/123/approve",
                    "POST",
                    None,
                    Value::Null,
                ),
                step(api_args("repos/example/repo/actions/runs/123"), queued),
            ]);
        }
        fixture.scripted_gh(&steps);
        let result = provider(&fixture).mutate_run(
            &identity(),
            "example/repo",
            &target,
            RunMutation::ApproveFork,
            &CancellationToken::default(),
        );
        if event == "pull_request" {
            let outcome = result.unwrap();
            assert!(outcome.accepted && outcome.completed);
            assert_eq!(outcome.run.unwrap().status, "queued");
            assert_eq!(journal(&fixture).len(), 5);
        } else {
            assert!(matches!(
                result.unwrap_err().kind,
                GithubErrorKind::InvalidTarget
            ));
            assert_eq!(journal(&fixture).len(), 3);
        }
        assert!(journal(&fixture).iter().all(|row| row["matched"] == true));
    }
}

fn workflow() -> Workflow {
    Workflow {
        id: 42,
        name: "deploy".into(),
        path: ".github/workflows/deploy.yml".into(),
        state: "active".into(),
    }
}
fn workflow_yaml() -> &'static str {
    "name: deploy\non:\n  workflow_dispatch:\n    inputs:\n      debug:\n        type: boolean\n        default: false\n      count:\n        type: number\n        default: 2\n      level:\n        type: choice\n        required: true\n        options: [info, warning]\n        default: info\n      target:\n        type: environment\n        required: true\njobs:\n  build:\n    strategy:\n      matrix:\n        os: [linux, mac]\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo hi\n  deploy:\n    needs: build\n    uses: example/shared/.github/workflows/deploy.yml@main\n"
}
#[test]
fn workflow_controls_yaml_real_parser_preserves_on_typed_defaults_dependencies_and_matrix() {
    let definition = WorkflowDefinition::parse(
        workflow(),
        "feature/fix".into(),
        run_target().head_sha,
        workflow_yaml().into(),
    )
    .unwrap();
    assert_eq!(definition.triggers, ["workflow_dispatch"]);
    assert_eq!(
        definition.dispatch_inputs[0].input_type,
        DispatchInputType::Boolean
    );
    assert_eq!(definition.jobs[1].needs, ["build"]);
    assert!(definition.jobs[1].reusable_workflow.is_some());
    assert_eq!(definition.jobs[0].matrix["os"], json!(["linux", "mac"]));
    let provided = BTreeMap::from([("target".into(), json!("production"))]);
    let payload = definition
        .validated_inputs(&provided, &["production".into()])
        .unwrap();
    assert_eq!(payload["debug"], json!(false));
    assert_eq!(payload["count"], json!(2));
    assert_eq!(payload["level"], "info");
    assert!(definition
        .validated_inputs(&provided, &["staging".into()])
        .is_err());
    let mut invalid = provided.clone();
    invalid.insert("debug".into(), json!("false"));
    assert!(definition
        .validated_inputs(&invalid, &["production".into()])
        .is_err());
    invalid.insert("debug".into(), json!(false));
    invalid.insert("level".into(), json!("unknown"));
    assert!(definition
        .validated_inputs(&invalid, &["production".into()])
        .is_err());
}
#[test]
fn workflow_controls_yaml_rejects_alias_bombs_depth_and_unknown_input_types() {
    let mut yaml = String::from("a: &a [x, x, x, x]\n");
    let mut previous = 'a';
    for next in 'b'..='n' {
        yaml.push_str(&format!(
            "{next}: &{next} [*{previous}, *{previous}, *{previous}, *{previous}]\n"
        ));
        previous = next;
    }
    let err = WorkflowDefinition::parse(workflow(), "main".into(), run_target().head_sha, yaml)
        .unwrap_err();
    assert!(err.message.contains("alias expansion"));
    let deep = format!("jobs: {}x{}", "[".repeat(70), "]".repeat(70));
    assert!(
        WorkflowDefinition::parse(workflow(), "main".into(), run_target().head_sha, deep)
            .unwrap_err()
            .message
            .contains("nesting")
    );
    assert!(WorkflowDefinition::parse(
        workflow(),
        "main".into(),
        run_target().head_sha,
        "on: { workflow_dispatch: { inputs: { foo: { type: mysterious } } } }".into()
    )
    .is_err());
}
#[test]
fn workflow_controls_definition_fetches_immutable_yaml_and_dispatch_confirms_returned_run() {
    let fixture = Fixture::new();
    let wf =
        json!({"id":42,"name":"deploy","path":".github/workflows/deploy.yml","state":"active"});
    let rawargs = vec![
        "api".into(),
        format!(
            "repos/example/repo/contents/.github/workflows/deploy.yml?ref={}",
            run_target().head_sha
        ),
        "--hostname".into(),
        "github.invalid".into(),
        "--method".into(),
        "GET".into(),
        "--header".into(),
        "Accept: application/vnd.github.raw+json".into(),
        "--allow-escape-sequences".into(),
    ];
    let mut dispatch_run = run_value();
    dispatch_run["event"] = json!("workflow_dispatch");
    let mut steps = vec![
        step(
            api_args("repos/example/repo/commits/feature%2Ffix"),
            json!({"sha":run_target().head_sha}),
        ),
        GhStep {
            args: rawargs,
            stdout: workflow_yaml().as_bytes().to_vec(),
            stdin: vec![],
            stderr: vec![],
            status: 0,
            delay_ms: 0,
        },
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        step(api_args("user"), json!({"login":"octocat"})),
        step(api_args("repos/example/repo/actions/workflows/42"), wf),
        step(
            api_args("repos/example/repo/commits/feature%2Ffix"),
            json!({"sha":run_target().head_sha}),
        ),
        page_step(
            "repos/example/repo/environments?per_page=100",
            json!([{"environments":[{"id":9,"name":"production"}]}]),
        ),
    ];
    steps.extend([write_step("repos/example/repo/actions/workflows/42/dispatches","POST",Some(json!({"ref":"feature/fix","inputs":{"debug":false,"count":2,"level":"info","target":"production"}})),json!({"workflow_run_id":123})),step(api_args("repos/example/repo/actions/runs/123"),dispatch_run)]);
    fixture.scripted_gh(&steps);
    let client = provider(&fixture);
    let definition = client
        .workflow_definition(
            "example/repo",
            &workflow(),
            "feature/fix",
            &CancellationToken::default(),
        )
        .unwrap();
    let outcome = client
        .dispatch_workflow(
            &identity(),
            "example/repo",
            &definition,
            &BTreeMap::from([("target".into(), json!("production"))]),
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(outcome.run.unwrap().id, 123);
    assert!(outcome.explanation.contains("confirmed"));
    assert_eq!(journal(&fixture).len(), 9);
}
#[test]
fn workflow_controls_enable_readback_and_stale_ref_block_dispatch() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        step(api_args("user"), json!({"login":"octocat"})),
        step(
            api_args("repos/example/repo/actions/workflows/42"),
            json!({"id":42,"name":"deploy","path":workflow().path,"state":"active"}),
        ),
        write_step(
            "repos/example/repo/actions/workflows/42/disable",
            "PUT",
            None,
            Value::Null,
        ),
        step(
            api_args("repos/example/repo/actions/workflows/42"),
            json!({"id":42,"name":"deploy","path":workflow().path,"state":"disabled_manually"}),
        ),
    ]);
    assert_eq!(
        provider(&fixture)
            .set_workflow_enabled(
                &identity(),
                "example/repo",
                &workflow(),
                false,
                &CancellationToken::default()
            )
            .unwrap()
            .state,
        "disabled_manually"
    );
    let fixture = Fixture::new();
    fixture.scripted_gh(&[
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        step(api_args("user"), json!({"login":"octocat"})),
        step(
            api_args("repos/example/repo/actions/workflows/42"),
            json!({"id":42,"name":"deploy","path":workflow().path,"state":"active"}),
        ),
        step(
            api_args("repos/example/repo/commits/main"),
            json!({"sha":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}),
        ),
    ]);
    let definition = WorkflowDefinition::parse(
        workflow(),
        "main".into(),
        run_target().head_sha,
        "on: workflow_dispatch\n".into(),
    )
    .unwrap();
    assert!(matches!(
        provider(&fixture)
            .dispatch_workflow(
                &identity(),
                "example/repo",
                &definition,
                &BTreeMap::new(),
                &CancellationToken::default()
            )
            .unwrap_err()
            .kind,
        GithubErrorKind::StaleHead
    ));
    assert_eq!(journal(&fixture).len(), 4);
}
#[test]
fn actions_secrets_vars_secret_set_masks_stdin_journal_and_metadata_never_contains_value() {
    let fixture = Fixture::new();
    let value = "synthetic secret, no persistence";
    let mut set = GhStep::read(
        &[
            "secret",
            "set",
            "TOKEN",
            "--app",
            "actions",
            "--repo",
            "github.invalid/example/repo",
            "--env",
            "prod/eu",
        ],
        value,
    );
    set.stdin = value.as_bytes().to_vec();
    let scope = ActionsScope::Environment {
        repository: "example/repo".into(),
        environment: "prod/eu".into(),
    };
    fixture.scripted_gh(&[GhStep::read(&["--version"],"gh version 2.101.0\n"),step(api_args("user"),json!({"login":"octocat"})),page_step("repos/example/repo/environments/prod%2Feu/secrets?per_page=100",json!([{"secrets":[]}])),set,page_step("repos/example/repo/environments/prod%2Feu/secrets?per_page=100",json!([{"secrets":[{"name":"TOKEN","created_at":"date","updated_at":"date","value":value}]}]))]);
    let result = provider(&fixture)
        .set_secret(
            &identity(),
            &scope,
            &SecretUpdate {
                name: "TOKEN".into(),
                observed: None,
                policy: None,
            },
            value,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(result.name, "TOKEN");
    assert!(!serde_json::to_string(&result).unwrap().contains(value));
    assert_eq!(journal(&fixture)[3]["stdin_hex"], "[REDACTED]");
    let journal_text = fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    assert!(!journal_text.contains(value));
    assert!(!journal_text.contains(
        &value
            .as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    ));
}
#[test]
fn actions_secrets_vars_secret_error_diagnostics_redact_transient_value_and_require_reconciliation()
{
    let fixture = Fixture::new();
    let secret = "not-a-github-token-but-still-secret";
    let mut set = GhStep::read(
        &[
            "secret",
            "set",
            "TOKEN",
            "--app",
            "actions",
            "--repo",
            "github.invalid/example/repo",
        ],
        secret,
    );
    set.stdin = secret.as_bytes().to_vec();
    set.stderr = format!("failure: {secret}").into_bytes();
    set.status = 1;
    fixture.scripted_gh(&[
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        step(api_args("user"), json!({"login":"octocat"})),
        page_step(
            "repos/example/repo/actions/secrets?per_page=100",
            json!([{"secrets":[]}]),
        ),
        set,
    ]);
    let scope = ActionsScope::Repository {
        repository: "example/repo".into(),
    };
    let error = provider(&fixture)
        .set_secret(
            &identity(),
            &scope,
            &SecretUpdate {
                name: "TOKEN".into(),
                observed: None,
                policy: None,
            },
            secret,
            &CancellationToken::default(),
        )
        .unwrap_err();
    assert!(matches!(error.kind, GithubErrorKind::AmbiguousWrite));
    assert!(!error.message.contains(secret));
    assert!(error.message.contains("[REDACTED]"));
    assert_eq!(journal(&fixture)[3]["stdin_hex"], "[REDACTED]");
}
#[test]
fn actions_secrets_vars_variables_environment_patch_roundtrip_preserves_scope_and_checks_staleness()
{
    let fixture = Fixture::new();
    let scope = ActionsScope::Environment {
        repository: "example/repo".into(),
        environment: "prod/eu".into(),
    };
    let observed = ActionsVariable {
        name: "LEVEL".into(),
        value: "info".into(),
        created_at: "date".into(),
        updated_at: "date".into(),
        visibility: None,
    };
    fixture.scripted_gh(&[
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        step(api_args("user"), json!({"login":"octocat"})),
        step(
            api_args("repos/example/repo/environments/prod%2Feu/variables/LEVEL"),
            json!(observed),
        ),
        write_step(
            "repos/example/repo/environments/prod%2Feu/variables/LEVEL",
            "PATCH",
            Some(json!({"name":"LEVEL","value":"warning"})),
            Value::Null,
        ),
        step(
            api_args("repos/example/repo/environments/prod%2Feu/variables/LEVEL"),
            json!({"name":"LEVEL","value":"warning","created_at":"date","updated_at":"date2"}),
        ),
    ]);
    let edit = VariableEdit {
        name: "LEVEL".into(),
        value: "warning".into(),
        visibility: None,
        selected_repository_ids: None,
    };
    assert_eq!(
        provider(&fixture)
            .set_variable(
                &identity(),
                &scope,
                Some(&observed),
                &edit,
                &CancellationToken::default()
            )
            .unwrap()
            .value,
        "warning"
    );
    assert_eq!(journal(&fixture).len(), 5);
    let fixture = Fixture::new();
    fixture.scripted_gh(&[
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        step(api_args("user"), json!({"login":"octocat"})),
        step(
            api_args("repos/example/repo/environments/prod%2Feu/variables/LEVEL"),
            json!({"name":"LEVEL","value":"someone else","created_at":"date","updated_at":"later"}),
        ),
    ]);
    assert!(matches!(
        provider(&fixture)
            .set_variable(
                &identity(),
                &scope,
                Some(&observed),
                &edit,
                &CancellationToken::default()
            )
            .unwrap_err()
            .kind,
        GithubErrorKind::StaleHead
    ));
    assert_eq!(journal(&fixture).len(), 3);
}
fn zip_files(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in files {
        writer
            .start_file(
                *name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}
#[test]
fn actions_artifacts_preview_export_and_safe_extract_verify_bytes_without_overwriting() {
    use sha2::{Digest, Sha256};
    let fixture = Fixture::new();
    let bytes = zip_files(&[("out/readme.txt", b"hello\n"), ("image.bin", &[0, 255, 1])]);
    let digest = format!("sha256:{:x}", Sha256::digest(&bytes));
    let archive =
        ArtifactArchive::inspect(bytes.clone(), Some(&digest), ArtifactLimits::default()).unwrap();
    assert!(archive.digest_verified);
    assert_eq!(archive.preview_text("out/readme.txt").unwrap(), "hello\n");
    assert!(archive.preview_text("image.bin").is_err());
    let export = fixture.root.join("output.zip");
    archive.export_zip(&export).unwrap();
    assert_eq!(fs::read(&export).unwrap(), bytes);
    assert!(archive.export_zip(&export).is_err());
    let destination = fixture.root.join("extracted");
    let files = archive
        .extract_new(&destination, &CancellationToken::default())
        .unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(
        fs::read(destination.join("out/readme.txt")).unwrap(),
        b"hello\n"
    );
    assert!(archive
        .extract_new(&destination, &CancellationToken::default())
        .is_err());
    assert!(!fixture.root.join("out").exists());
}

#[test]
fn actions_artifacts_complete_preview_checks_crc_at_exact_budget_and_bounds_partial_bytes() {
    let bytes = zip_files(&[("crc.txt", b"checkcrc")]);
    let limits = ArtifactLimits {
        preview_bytes: 8,
        ..ArtifactLimits::default()
    };
    let archive = ArtifactArchive::inspect(bytes.clone(), None, limits.clone()).unwrap();
    assert_eq!(archive.preview("crc.txt").unwrap(), b"checkcrc");
    let mut corrupted = bytes;
    let offset = corrupted
        .windows(8)
        .position(|bytes| bytes == b"checkcrc")
        .unwrap();
    corrupted[offset] ^= 1;
    let archive = ArtifactArchive::inspect(corrupted, None, limits).unwrap();
    assert!(archive.preview("crc.txt").is_err());
    let archive = ArtifactArchive::inspect(
        zip_files(&[("large.txt", b"abcdefghijkl")]),
        None,
        ArtifactLimits {
            preview_bytes: 8,
            ..ArtifactLimits::default()
        },
    )
    .unwrap();
    assert_eq!(archive.preview("large.txt").unwrap(), b"abcdefgh");
}
#[test]
fn actions_artifacts_reject_traversal_absolute_paths_symlinks_duplicates_and_budgets() {
    for name in [
        "../escape",
        "/absolute",
        "a/../escape",
        "C:\\escape",
        "a\\..\\escape",
    ] {
        assert!(
            ArtifactArchive::inspect(zip_files(&[(name, b"x")]), None, ArtifactLimits::default())
                .is_err(),
            "{name}"
        );
    }
    assert!(ArtifactArchive::inspect(
        zip_files(&[("readme", b"a"), ("README", b"b")]),
        None,
        ArtifactLimits::default()
    )
    .is_err());
    assert!(ArtifactArchive::inspect(
        zip_files(&[("folder", b"a"), ("folder/file", b"b")]),
        None,
        ArtifactLimits::default()
    )
    .is_err());
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .add_symlink(
            "link",
            "/tmp/escape",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
    assert!(ArtifactArchive::inspect(
        writer.finish().unwrap().into_inner(),
        None,
        ArtifactLimits::default()
    )
    .unwrap_err()
    .message
    .contains("link"));
    let limits = ArtifactLimits {
        entry_bytes: 2,
        ..ArtifactLimits::default()
    };
    assert!(ArtifactArchive::inspect(zip_files(&[("big", b"123")]), None, limits).is_err());
    assert!(ArtifactArchive::inspect(
        zip_files(&[("file", b"x")]),
        Some(&format!("sha256:{}", "0".repeat(64))),
        ArtifactLimits::default()
    )
    .is_err());
    let fixture = Fixture::new();
    let token = CancellationToken::default();
    token.cancel();
    let archive = ArtifactArchive::inspect(
        zip_files(&[("file", b"x")]),
        None,
        ArtifactLimits::default(),
    )
    .unwrap();
    assert!(archive
        .extract_new(&fixture.root.join("cancelled"), &token)
        .is_err());
    assert!(!fixture.root.join("cancelled").exists());
}
#[test]
fn raw_binary_transport_preserves_escape_bytes_and_has_safe_old_cli_flag_fallback() {
    let fixture = Fixture::new();
    let sha = "cccccccccccccccccccccccccccccccccccccccc";
    let mut args = api_args(&format!("repos/example/repo/git/blobs/{sha}"));
    args.extend([
        "--header".into(),
        "Accept: application/vnd.github.raw+json".into(),
    ]);
    let mut modern = args.clone();
    modern.push("--allow-escape-sequences".into());
    let mut unsupported = GhStep {
        args: modern,
        stdout: vec![],
        stdin: vec![],
        stderr: b"unknown flag: --allow-escape-sequences".to_vec(),
        status: 1,
        delay_ms: 0,
    };
    unsupported.delay_ms = 1;
    let raw = b"\0\xff\x1b[31mraw";
    fixture.scripted_gh(&[
        unsupported,
        GhStep {
            args,
            stdout: raw.to_vec(),
            stdin: vec![],
            stderr: vec![],
            status: 0,
            delay_ms: 0,
        },
    ]);
    assert_eq!(
        provider(&fixture)
            .remote_blob("example/repo", sha, &CancellationToken::default())
            .unwrap(),
        raw
    );
    assert_eq!(journal(&fixture).len(), 2);
    assert!(journal(&fixture).iter().all(|v| v["matched"] == true));
}
fn artifact_raw() -> Value {
    json!({"id":77,"name":"results","size_in_bytes":100,"expired":false,"created_at":"date","updated_at":"date","expires_at":"later","workflow_run":{"id":123,"head_sha":run_target().head_sha}})
}
#[test]
fn actions_artifacts_download_keeps_integrity_identity_and_delete_preserves_other_artifacts() {
    let fixture = Fixture::new();
    let bytes = zip_files(&[("result.txt", b"passed\n")]);
    let raw = artifact_raw();
    let target = ActionsArtifact {
        id: 77,
        name: "results".into(),
        run_id: 123,
        attempt: None,
        head_sha: run_target().head_sha,
        size_in_bytes: 100,
        expired: false,
        created_at: "date".into(),
        updated_at: "date".into(),
        expires_at: "later".into(),
        digest: None,
    };
    let mut args = api_args("repos/example/repo/actions/artifacts/77/zip");
    args.push("--allow-escape-sequences".into());
    fixture.scripted_gh(&[
        step(
            api_args("repos/example/repo/actions/artifacts/77"),
            raw.clone(),
        ),
        GhStep {
            args,
            stdout: bytes,
            stdin: vec![],
            stderr: vec![],
            status: 0,
            delay_ms: 0,
        },
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        step(api_args("user"), json!({"login":"octocat"})),
        step(api_args("repos/example/repo/actions/artifacts/77"), raw),
        write_step(
            "repos/example/repo/actions/artifacts/77",
            "DELETE",
            None,
            Value::Null,
        ),
        page_step(
            "repos/example/repo/actions/runs/123/artifacts?per_page=100",
            json!([{"artifacts":[{"id":78,"name":"other","workflow_run":{"id":123}}]}]),
        ),
    ]);
    let client = provider(&fixture);
    let archive = client
        .download_artifact(
            "example/repo",
            &target,
            ArtifactLimits::default(),
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(archive.preview_text("result.txt").unwrap(), "passed\n");
    assert!(!archive.digest_verified);
    let remaining = client
        .delete_artifact(
            &identity(),
            "example/repo",
            &target,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(remaining[0].id, 78);
    assert_eq!(remaining[0].attempt, None);
    assert_eq!(journal(&fixture).len(), 7);
}
#[test]
fn actions_policy_edit_previews_typed_keys_checks_concurrency_and_reads_back() {
    let fixture = Fixture::new();
    let scope = ActionsScope::Repository {
        repository: "example/repo".into(),
    };
    let before = json!({"enabled":true,"allowed_actions":"all","sha_pinning_required":false,"extra_host_field":42});
    let after = json!({"enabled":true,"allowed_actions":"selected","sha_pinning_required":true,"extra_host_field":42});
    fixture.scripted_gh(&[
        step(
            api_args("repos/example/repo/actions/permissions"),
            before.clone(),
        ),
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        step(api_args("user"), json!({"login":"octocat"})),
        step(api_args("repos/example/repo/actions/permissions"), before),
        write_step(
            "repos/example/repo/actions/permissions",
            "PUT",
            Some(json!({"enabled":true,"allowed_actions":"selected","sha_pinning_required":true})),
            Value::Null,
        ),
        step(api_args("repos/example/repo/actions/permissions"), after),
    ]);
    let client = provider(&fixture);
    let observed = client
        .actions_policy(
            &scope,
            PolicySection::Permissions,
            &CancellationToken::default(),
        )
        .unwrap();
    let result = client
        .edit_actions_policy(
            &identity(),
            &observed,
            &PolicyEdit::RepositoryPermissions {
                enabled: true,
                allowed_actions: AllowedActions::Selected,
                sha_pinning_required: true,
            },
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(result.value["extra_host_field"], 42);
    assert_eq!(result.value["allowed_actions"], "selected");
    assert_eq!(journal(&fixture).len(), 6);
}
#[test]
fn actions_successful_write_failed_readback_is_ambiguous_and_not_repeated() {
    let fixture = Fixture::new();
    let before = json!({"enabled":true,"allowed_actions":"all","sha_pinning_required":false});
    let mut denied = step(
        api_args("repos/example/repo/actions/permissions"),
        Value::Null,
    );
    denied.status = 1;
    denied.stderr = b"HTTP 403: readback permission denied".to_vec();
    fixture.scripted_gh(&[
        step(
            api_args("repos/example/repo/actions/permissions"),
            before.clone(),
        ),
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        step(api_args("user"), json!({"login":"octocat"})),
        step(
            api_args("repos/example/repo/actions/permissions"),
            before.clone(),
        ),
        write_step(
            "repos/example/repo/actions/permissions",
            "PUT",
            Some(json!({"enabled":false,"allowed_actions":"all","sha_pinning_required":false})),
            Value::Null,
        ),
        denied,
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        step(api_args("user"), json!({"login":"octocat"})),
        step(api_args("repos/example/repo/actions/permissions"), before),
    ]);
    let client = provider(&fixture);
    let scope = ActionsScope::Repository {
        repository: "example/repo".into(),
    };
    let observed = client
        .actions_policy(
            &scope,
            PolicySection::Permissions,
            &CancellationToken::default(),
        )
        .unwrap();
    let edit = PolicyEdit::RepositoryPermissions {
        enabled: false,
        allowed_actions: AllowedActions::All,
        sha_pinning_required: false,
    };
    let first = client
        .edit_actions_policy(&identity(), &observed, &edit, &CancellationToken::default())
        .unwrap_err();
    assert!(matches!(first.kind, GithubErrorKind::AmbiguousWrite));
    assert!(first.message.contains("may have succeeded"));
    let second = client
        .edit_actions_policy(&identity(), &observed, &edit, &CancellationToken::default())
        .unwrap_err();
    assert!(matches!(second.kind, GithubErrorKind::AmbiguousWrite));
    assert_eq!(
        journal(&fixture)
            .iter()
            .filter(|row| row["args"][5] == "PUT")
            .count(),
        1
    );
    assert_eq!(journal(&fixture).len(), 9);
}

#[test]
fn github_case_alias_cannot_bypass_uncertain_write_or_reconciliation_generation() {
    let fixture = Fixture::new();
    let workflow = Workflow {
        id: 7,
        name: "Build".into(),
        path: ".github/workflows/CI.yml".into(),
        state: "active".into(),
    };
    let value = serde_json::to_value(&workflow).unwrap();
    let mut steps = Vec::new();
    for repository in ["example/repo", "EXAMPLE/REPO"] {
        steps.extend([
            GhStep::read(&["--version"], "gh version 2.101.0\n"),
            step(api_args("user"), json!({"login":"octocat"})),
            step(
                api_args(&format!("repos/{repository}/actions/workflows/7")),
                value.clone(),
            ),
        ]);
        if repository == "example/repo" {
            let mut uncertain = write_step(
                "repos/example/repo/actions/workflows/7/disable",
                "PUT",
                None,
                Value::Null,
            );
            uncertain.status = 1;
            uncertain.stderr = b"connection closed after request; result unknown".to_vec();
            steps.push(uncertain);
        }
    }
    steps.extend([
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        step(api_args("user"), json!({"login":"octocat"})),
        page_step(
            "repos/EXAMPLE/REPO/actions/workflows?per_page=100",
            json!([{"workflows":[value]}]),
        ),
        page_step(
            "repos/EXAMPLE/REPO/actions/workflows/7/runs?per_page=100",
            json!([{"total_count":0,"workflow_runs":[]}]),
        ),
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        step(api_args("user"), json!({"login":"octocat"})),
        GhStep::read(&["after-ack"], "accepted"),
    ]);
    fixture.scripted_gh(&steps);
    let runner = CommandRunner::default();
    let gh = provider(&fixture).with_runner(runner.clone());
    for repository in ["example/repo", "EXAMPLE/REPO"] {
        assert!(matches!(
            gh.set_workflow_enabled(
                &identity(),
                repository,
                &workflow,
                false,
                &CancellationToken::default()
            )
            .unwrap_err()
            .kind,
            GithubErrorKind::AmbiguousWrite
        ));
    }
    assert_eq!(journal(&fixture).len(), 7);
    assert_eq!(
        journal(&fixture)
            .iter()
            .filter(|row| row["args"][5] == "PUT")
            .count(),
        1
    );
    let observation = gh
        .inspect_actions_target(
            &identity(),
            &ActionsTarget::Workflow {
                repository: "EXAMPLE/REPO".into(),
                id: 7,
            },
            &CancellationToken::default(),
        )
        .unwrap();
    gh.acknowledge_actions_observation(observation).unwrap();
    let key = "github.invalid/example/repo/workflow/7";
    let command = runner_spec(&fixture, &["after-ack"]).write(key);
    assert_eq!(
        runner
            .run(command, &CancellationToken::default())
            .unwrap()
            .stdout,
        b"accepted"
    );
    assert_eq!(journal(&fixture).len(), 14);
}
#[test]
fn actions_runner_labels_keep_read_only_labels_and_validate_exact_runner() {
    let fixture = Fixture::new();
    let scope = ActionsScope::Organization {
        organization: "example".into(),
    };
    let raw = json!({"id":9,"name":"builder","os":"linux","status":"online","busy":false,"labels":[{"id":1,"name":"self-hosted","type":"read-only"},{"id":2,"name":"old","type":"custom"}]});
    let after = json!({"id":9,"name":"builder","os":"linux","status":"online","busy":false,"labels":[{"id":1,"name":"self-hosted","type":"read-only"},{"id":3,"name":"gpu","type":"custom"}]});
    let runner = SelfHostedRunner {
        id: 9,
        name: "builder".into(),
        os: "linux".into(),
        status: "online".into(),
        busy: false,
        labels: vec![
            RunnerLabel {
                id: 1,
                name: "self-hosted".into(),
                label_type: "read-only".into(),
            },
            RunnerLabel {
                id: 2,
                name: "old".into(),
                label_type: "custom".into(),
            },
        ],
    };
    fixture.scripted_gh(&[
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        step(api_args("user"), json!({"login":"octocat"})),
        step(api_args("orgs/example/actions/runners/9"), raw),
        write_step(
            "orgs/example/actions/runners/9/labels",
            "PUT",
            Some(json!({"labels":["gpu"]})),
            json!({"labels":after["labels"]}),
        ),
        step(api_args("orgs/example/actions/runners/9"), after),
    ]);
    let fresh = provider(&fixture)
        .set_runner_labels(
            &identity(),
            &scope,
            &runner,
            &["gpu".into()],
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(fresh.labels[0].label_type, "read-only");
    assert_eq!(fresh.labels[1].name, "gpu");
    assert_eq!(journal(&fixture).len(), 5);
}
#[test]
fn actions_group_associations_replace_only_confirmed_exact_ids() {
    let fixture = Fixture::new();
    let group = RunnerGroup {
        id: 7,
        name: "build".into(),
        visibility: "selected".into(),
        default: false,
        inherited: false,
        allows_public_repositories: false,
        restricted_to_workflows: false,
        selected_workflows: vec![],
        workflow_restrictions_read_only: false,
    };
    fixture.scripted_gh(&[
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        step(api_args("user"), json!({"login":"octocat"})),
        step(
            api_args("orgs/example/actions/runner-groups/7"),
            json!(group),
        ),
        page_step(
            "orgs/example/actions/runner-groups/7/repositories?per_page=100",
            json!([{"repositories":[{"id":9}]}]),
        ),
        write_step(
            "orgs/example/actions/runner-groups/7/repositories",
            "PUT",
            Some(json!({"selected_repository_ids":[10,11]})),
            Value::Null,
        ),
        page_step(
            "orgs/example/actions/runner-groups/7/repositories?per_page=100",
            json!([{"repositories":[{"id":10},{"id":11}]}]),
        ),
    ]);
    let edit = GroupAssociationEdit {
        kind: GroupAssociation::Repositories,
        observed_ids: vec![9],
        selected_ids: vec![10, 11],
    };
    let result = provider(&fixture)
        .set_group_associations(
            &identity(),
            "example",
            &group,
            &edit,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(
        result
            .iter()
            .map(|v| v["id"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [10, 11]
    );
    assert_eq!(journal(&fixture).len(), 6);
}
#[test]
fn actions_environment_protection_edit_keeps_scope_and_typed_reviewers() {
    let fixture = Fixture::new();
    let before =
        json!({"id":9,"name":"prod/eu","protection_rules":[],"deployment_branch_policy":null});
    let after = json!({"id":9,"name":"prod/eu","protection_rules":[{"type":"wait_timer","wait_timer":30},{"type":"required_reviewers","prevent_self_review":true,"reviewers":[{"type":"User","reviewer":{"id":1,"login":"octocat"}}]}],"deployment_branch_policy":{"protected_branches":true,"custom_branch_policies":false}});
    fixture.scripted_gh(&[GhStep::read(&["--version"],"gh version 2.101.0\n"),step(api_args("user"),json!({"login":"octocat"})),step(api_args("repos/example/repo/environments/prod%2Feu"),before.clone()),write_step("repos/example/repo/environments/prod%2Feu","PUT",Some(json!({"wait_timer":30,"prevent_self_review":true,"reviewers":[{"type":"User","id":1}],"deployment_branch_policy":{"protected_branches":true,"custom_branch_policies":false}})),after.clone()),step(api_args("repos/example/repo/environments/prod%2Feu"),after)]);
    let edit = EnvironmentEdit {
        wait_timer: 30,
        prevent_self_review: true,
        reviewers: vec![EnvironmentReviewer {
            reviewer_type: "User".into(),
            id: 1,
        }],
        branch_mode: DeploymentBranchMode::Protected,
    };
    let result = provider(&fixture)
        .edit_environment(
            &identity(),
            "example/repo",
            &before,
            &edit,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(
        result["protection_rules"][1]["reviewers"][0]["reviewer"]["login"],
        "octocat"
    );
    assert_eq!(journal(&fixture).len(), 5);
}
fn remote_search_step(
    endpoint: &str,
    q: &str,
    page: u64,
    total: u64,
    items: Vec<Value>,
    incomplete: bool,
) -> GhStep {
    let mut args = api_args(endpoint);
    args.extend([
        "--header".into(),
        "Accept: application/vnd.github.text-match+json".into(),
        "-f".into(),
        format!("q={q}"),
        "-f".into(),
        "per_page=100".into(),
        "-f".into(),
        format!("page={page}"),
    ]);
    step(
        args,
        json!({"total_count":total,"items":items,"incomplete_results":incomplete}),
    )
}
#[test]
fn remote_search_rejects_regex_revision_comments_and_out_of_scope_without_requests() {
    let fixture = Fixture::new();
    let client = provider(&fixture);
    for (input, mode, kind) in [
        ("needle", SearchMode::Regex, RemoteSearchKind::Code),
        (
            "needle rev:main",
            SearchMode::GitHub,
            RemoteSearchKind::Code,
        ),
        (
            "needle type:comment",
            SearchMode::GitHub,
            RemoteSearchKind::PullRequests,
        ),
        (
            "needle repo:other/repo",
            SearchMode::GitHub,
            RemoteSearchKind::PullRequests,
        ),
    ] {
        let query = Query::parse(input, mode).unwrap();
        assert!(
            client
                .remote_search(
                    &identity(),
                    &query,
                    kind,
                    &["example/repo".into()],
                    &CancellationToken::default()
                )
                .is_err(),
            "{input}"
        );
    }
    assert!(journal(&fixture).is_empty());
}
#[test]
fn remote_pr_search_enforces_type_scope_match_provenance_and_partial_coverage() {
    let fixture = Fixture::new();
    let mut item = pr(7, "example/repo");
    item["pull_request"] = json!({"url":"https://api.github.invalid/repos/example/repo/pulls/7"});
    item["text_matches"] = json!([{"fragment":"needle in review","property":"body","object_type":"IssueComment","object_url":"https://api.github.invalid/repos/example/repo/issues/comments/9","matches":[{"indices":[0,6]}]}]);
    fixture.scripted_gh(&[remote_search_step(
        "search/issues",
        "needle repo:example/repo is:pr",
        1,
        1001,
        vec![item],
        true,
    )]);
    let query = Query::parse("needle type:pr", SearchMode::GitHub).unwrap();
    let results = provider(&fixture)
        .remote_search(
            &identity(),
            &query,
            RemoteSearchKind::PullRequests,
            &["example/repo".into()],
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(results.items[0].number, Some(7));
    assert_eq!(results.items[0].author.as_ref().unwrap().label(), "@alice");
    assert_eq!(results.items[0].text_matches[0].object_type, "IssueComment");
    assert_eq!(
        results.items[0].text_matches[0].server_match_indices,
        [[0, 6]]
    );
    assert!(!results.coverage.complete);
    assert!(results.coverage.reasons.iter().any(|r| r.contains("1,000")));
    assert!(results
        .coverage
        .reasons
        .iter()
        .any(|r| r.contains("incomplete_results")));
    assert!(results
        .provider_notes
        .iter()
        .any(|r| r.contains("exact comment destinations")));
    assert_eq!(journal(&fixture).len(), 1);
}
#[test]
fn remote_code_search_blob_identity_is_verified_at_an_immutable_commit_before_navigation() {
    let fixture = Fixture::new();
    let blob = "cccccccccccccccccccccccccccccccccccccccc";
    let item = json!({"name":"lib.rs","path":"src/lib.rs","sha":blob,"html_url":"https://github.invalid/example/repo/blob/main/src/lib.rs","repository":{"full_name":"example/repo","owner":{"login":"org-owner"}},"text_matches":[{"fragment":"fn needle() {}","property":"content","matches":[{"indices":[3,9]}]}]});
    fixture.scripted_gh(&[
        remote_search_step(
            "search/code",
            "needle repo:example/repo",
            1,
            1,
            vec![item],
            false,
        ),
        step(
            api_args("repos/example/repo"),
            json!({"id":8,"full_name":"example/repo","default_branch":"main"}),
        ),
        step(
            api_args("repos/example/repo/commits/main"),
            json!({"sha":run_target().head_sha}),
        ),
        step(
            api_args(&format!(
                "repos/example/repo/contents/src/lib.rs?ref={}",
                run_target().head_sha
            )),
            json!({"type":"file","path":"src/lib.rs","sha":blob}),
        ),
    ]);
    let client = provider(&fixture);
    let query = Query::parse("needle type:file", SearchMode::GitHub).unwrap();
    let results = client
        .remote_search(
            &identity(),
            &query,
            RemoteSearchKind::Code,
            &["example/repo".into()],
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(results.items[0].author.is_none());
    assert_eq!(
        results.items[0].repository_owner.as_ref().unwrap().label(),
        "@org-owner"
    );
    assert_eq!(results.items[0].blob_sha.as_deref(), Some(blob));
    let resolved = client
        .resolve_remote_code(&results.items[0], &CancellationToken::default())
        .unwrap();
    assert_eq!(resolved.commit_sha, run_target().head_sha);
    assert_ne!(resolved.commit_sha, resolved.blob_sha);
    assert_eq!(resolved.path, "src/lib.rs");
    assert_eq!(journal(&fixture).len(), 4);
}
#[test]
fn remote_code_stale_blob_does_not_show_a_different_file_version() {
    let fixture = Fixture::new();
    let item = json!({"name":"lib.rs","path":"src/lib.rs","sha":"cccccccccccccccccccccccccccccccccccccccc","repository":{"full_name":"example/repo"}});
    fixture.scripted_gh(&[remote_search_step("search/code","needle repo:example/repo",1,1,vec![item],false),step(api_args("repos/example/repo"),json!({"id":8,"full_name":"example/repo","default_branch":"main"})),step(api_args("repos/example/repo/commits/main"),json!({"sha":run_target().head_sha})),step(api_args(&format!("repos/example/repo/contents/src/lib.rs?ref={}",run_target().head_sha)),json!({"type":"file","path":"src/lib.rs","sha":"dddddddddddddddddddddddddddddddddddddddd"}))]);
    let client = provider(&fixture);
    let results = client
        .remote_search(
            &identity(),
            &Query::parse("needle", SearchMode::GitHub).unwrap(),
            RemoteSearchKind::Code,
            &["example/repo".into()],
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(matches!(
        client
            .resolve_remote_code(&results.items[0], &CancellationToken::default())
            .unwrap_err()
            .kind,
        GithubErrorKind::StaleHead
    ));
    assert_eq!(journal(&fixture).len(), 4);
}

fn real_git_blob(fixture: &Fixture, algorithm: &str, bytes: &[u8]) -> String {
    use std::process::{Command, Stdio};
    let git = || {
        let mut command = Command::new("git");
        command.current_dir(fixture.repo());
        for key in [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
            "GIT_OBJECT_DIRECTORY",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        ] {
            command.env_remove(key);
        }
        command
    };
    let output = git()
        .args(["init", "--quiet", &format!("--object-format={algorithm}")])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut child = git()
        .args(["hash-object", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(bytes).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
fn remote_code_item(blob: &str) -> RemoteSearchItem {
    RemoteSearchItem {
        host: "github.invalid".into(),
        repository: "example/repo".into(),
        kind: RemoteSearchKind::Code,
        id: format!("src/lib.rs:{blob}"),
        number: None,
        title: "lib.rs".into(),
        body: String::new(),
        url: "https://github.invalid/example/repo/blob/main/src/lib.rs".into(),
        state: String::new(),
        author: None,
        repository_owner: None,
        path: Some("src/lib.rs".into()),
        blob_sha: Some(blob.into()),
        created_at: String::new(),
        updated_at: String::new(),
        text_matches: vec![],
    }
}
fn remote_content_steps(blob: &str, content: Value) -> Vec<GhStep> {
    vec![
        step(
            api_args("repos/example/repo"),
            json!({"id":8,"full_name":"example/repo","default_branch":"main"}),
        ),
        step(
            api_args("repos/example/repo/commits/main"),
            json!({"sha":"a".repeat(blob.len())}),
        ),
        step(
            api_args(&format!(
                "repos/example/repo/contents/src/lib.rs?ref={}",
                "a".repeat(blob.len())
            )),
            content,
        ),
    ]
}
#[test]
fn remote_code_content_reuses_exact_lookup_and_matches_real_git_sha1_sha256_binary_blobs() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let bytes = b"fn fixture() {}\n\0\xff";
    for algorithm in ["sha1", "sha256"] {
        let fixture = Fixture::new();
        let blob = real_git_blob(&fixture, algorithm, bytes);
        let encoded = STANDARD.encode(bytes);
        let wrapped = format!("{}\r\n{}\n", &encoded[..8], &encoded[8..]);
        fixture.scripted_gh(&remote_content_steps(&blob,json!({"type":"file","path":"src/lib.rs","sha":blob,"size":bytes.len(),"encoding":"base64","content":wrapped})));
        let resolved = provider(&fixture)
            .resolve_remote_content(
                &remote_code_item(&blob),
                1024,
                &CancellationToken::default(),
            )
            .unwrap();
        assert_eq!(resolved.bytes, bytes);
        assert_eq!(resolved.file.blob_sha, blob);
        assert_eq!(resolved.file.commit_sha, "a".repeat(blob.len()));
        assert!(std::str::from_utf8(&resolved.bytes).is_err());
        verify_blob_digest(&resolved.bytes, &resolved.file.blob_sha).unwrap();
        assert_eq!(journal(&fixture).len(), 3);
        assert!(journal(&fixture)
            .iter()
            .all(|row| row["args"][0] == "api" && row["args"][5] == "GET"));
        assert!(!fixture.repo().join("src/lib.rs").exists());
    }
}
#[test]
fn remote_code_content_independent_digest_rejects_matching_metadata_but_different_bytes() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let fixture = Fixture::new();
    let blob = real_git_blob(&fixture, "sha1", b"original");
    fixture.scripted_gh(&remote_content_steps(&blob,json!({"type":"file","path":"src/lib.rs","sha":blob,"size":8,"encoding":"base64","content":STANDARD.encode(b"tampered")})));
    let error = provider(&fixture)
        .resolve_remote_content(
            &remote_code_item(&blob),
            1024,
            &CancellationToken::default(),
        )
        .unwrap_err();
    assert!(matches!(error.kind, GithubErrorKind::StaleHead));
    assert!(error.message.contains("bytes do not match"));
    assert_eq!(journal(&fixture).len(), 3);
}
#[test]
fn remote_code_content_budget_encoding_size_and_canonical_base64_fail_closed() {
    let blob = "c".repeat(40);
    for content in [
        json!({"size":2049,"encoding":"base64","content":""}),
        json!({"size":8,"encoding":"none","content":"","download_url":"https://untrusted.invalid/raw"}),
        json!({"encoding":"base64","content":"dGFtcGVyZWQ="}),
        json!({"size":1,"encoding":"base64","content":"dGFtcGVyZWQ="}),
        json!({"size":8,"encoding":"base64","content":"!!!!!!!!!!!!"}),
        json!({"size":8,"encoding":"base64","content":"dGFtcGVyZWQ "}),
    ] {
        let fixture = Fixture::new();
        let mut content = content;
        content["type"] = json!("file");
        content["path"] = json!("src/lib.rs");
        content["sha"] = json!(blob);
        fixture.scripted_gh(&remote_content_steps(&blob, content));
        assert!(provider(&fixture)
            .resolve_remote_content(
                &remote_code_item(&blob),
                2048,
                &CancellationToken::default()
            )
            .is_err());
        assert_eq!(journal(&fixture).len(), 3);
    }
}
#[test]
fn remote_code_content_invalid_budget_path_host_and_renamed_repository_do_not_follow_urls() {
    let fixture = Fixture::new();
    let client = provider(&fixture);
    let mut item = remote_code_item(&"c".repeat(40));
    for budget in [0, 2 * 1024 * 1024 + 1] {
        assert!(client
            .resolve_remote_content(&item, budget, &CancellationToken::default())
            .is_err());
    }
    for path in ["../outside", "/absolute", "src//file", "src/./file"] {
        item.path = Some(path.into());
        assert!(client
            .resolve_remote_content(&item, 1024, &CancellationToken::default())
            .is_err());
    }
    item.path = Some("src/lib.rs".into());
    item.host = "other.invalid".into();
    assert!(client
        .resolve_remote_content(&item, 1024, &CancellationToken::default())
        .is_err());
    assert!(journal(&fixture).is_empty());
    item.host = "github.invalid".into();
    fixture.scripted_gh(&[step(
        api_args("repos/example/repo"),
        json!({"id":8,"full_name":"new-owner/renamed","default_branch":"main"}),
    )]);
    assert!(matches!(
        client
            .resolve_remote_content(&item, 1024, &CancellationToken::default())
            .unwrap_err()
            .kind,
        GithubErrorKind::StaleHead
    ));
    assert_eq!(journal(&fixture).len(), 1);
}
#[test]
fn environment_deployment_patterns_create_delete_and_preserve_branch_tag_type() {
    let fixture = Fixture::new();
    let env = "prod/eu";
    let endpoint = "repos/example/repo/environments/prod%2Feu/deployment-branch-policies";
    let pattern = json!({"id":12,"name":"release/*","type":"tag"});
    fixture.scripted_gh(&[
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        step(api_args("user"), json!({"login":"octocat"})),
        step(
            api_args("repos/example/repo/environments/prod%2Feu"),
            json!({"id":9,"name":env,"deployment_branch_policy":{"custom_branch_policies":true}}),
        ),
        page_step(
            &format!("{endpoint}?per_page=100"),
            json!([{"branch_policies":[]}]),
        ),
        write_step(
            endpoint,
            "POST",
            Some(json!({"name":"release/*","type":"tag"})),
            pattern.clone(),
        ),
        step(api_args(&format!("{endpoint}/12")), pattern.clone()),
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        step(api_args("user"), json!({"login":"octocat"})),
        step(api_args(&format!("{endpoint}/12")), pattern),
        write_step(&format!("{endpoint}/12"), "DELETE", None, Value::Null),
        page_step(
            &format!("{endpoint}?per_page=100"),
            json!([{"branch_policies":[{"id":13,"name":"main","type":"branch"}]}]),
        ),
    ]);
    let client = provider(&fixture);
    let request = DeploymentPatternEdit {
        observed: None,
        name: "release/*".into(),
        pattern_type: DeploymentPatternType::Tag,
    };
    let created = client
        .set_deployment_pattern(
            &identity(),
            "example/repo",
            env,
            &request,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(created.pattern_type.as_deref(), Some("tag"));
    let remaining = client
        .delete_deployment_pattern(
            &identity(),
            "example/repo",
            env,
            &created,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(remaining[0].id, 13);
    assert_eq!(journal(&fixture).len(), 11);
}
#[test]
fn environment_custom_protection_uses_installed_app_and_exact_rule_readback() {
    let fixture = Fixture::new();
    let endpoint = "repos/example/repo/environments/production/deployment_protection_rules";
    let rule = json!({"id":4,"enabled":true,"app":{"id":5,"slug":"deployment-validator"}});
    fixture.scripted_gh(&[GhStep::read(&["--version"],"gh version 2.101.0\n"),step(api_args("user"),json!({"login":"octocat"})),page_step(&format!("{endpoint}/apps?per_page=100"),json!([[{"total_count":1},{"available_custom_deployment_protection_rule_integrations":[{"id":5,"slug":"deployment-validator"}]}]])),page_step(&format!("{endpoint}?per_page=100"),json!([{"custom_deployment_protection_rules":[]}])),write_step(endpoint,"POST",Some(json!({"integration_id":5})),rule.clone()),step(api_args(&format!("{endpoint}/4")),rule.clone()),GhStep::read(&["--version"],"gh version 2.101.0\n"),step(api_args("user"),json!({"login":"octocat"})),step(api_args(&format!("{endpoint}/4")),rule),write_step(&format!("{endpoint}/4"),"DELETE",None,Value::Null),page_step(&format!("{endpoint}?per_page=100"),json!([{"custom_deployment_protection_rules":[]}]))]);
    let client = provider(&fixture);
    let created = client
        .enable_custom_protection(
            &identity(),
            "example/repo",
            "production",
            5,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(created.app["slug"], "deployment-validator");
    assert!(client
        .disable_custom_protection(
            &identity(),
            "example/repo",
            "production",
            &created,
            &CancellationToken::default()
        )
        .unwrap()
        .is_empty());
    assert_eq!(journal(&fixture).len(), 11);
}
#[test]
fn actions_hosted_runner_inventory_is_read_only_and_reports_group_resources() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[page_step("orgs/example/actions/runner-groups/7/hosted-runners?per_page=100",json!([{"runners":[{"id":9,"name":"mac build","runner_group_id":7,"platform":"macos-arm64","status":"Ready","maximum_runners":2,"image":{"id":"macos-15"},"machine_size_details":{"cpu_cores":3,"memory_gb":7}}]}]))]);
    let runners = provider(&fixture)
        .hosted_runners("example", Some(7), &CancellationToken::default())
        .unwrap();
    assert_eq!(runners[0].platform, "macos-arm64");
    assert_eq!(runners[0].machine_size["memory_gb"], 7);
    assert_eq!(runners[0].maximum_runners, 2);
    assert_eq!(journal(&fixture).len(), 1);
    assert_eq!(journal(&fixture)[0]["args"][5], "GET");
}
#[test]
fn actions_runner_group_creation_and_deletion_verify_policy_and_memberships() {
    let fixture = Fixture::new();
    let group = json!({"id":7,"name":"build","visibility":"selected","default":false,"inherited":false,"allows_public_repositories":false,"restricted_to_workflows":true,"selected_workflows":["example/repo/.github/workflows/build.yml@refs/heads/main"]});
    let request = CreateRunnerGroup {
        settings: RunnerGroupEdit {
            name: "build".into(),
            visibility: ScopeVisibility::Selected,
            allows_public_repositories: false,
            restricted_to_workflows: true,
            selected_workflows: vec![
                "example/repo/.github/workflows/build.yml@refs/heads/main".into()
            ],
        },
        repository_ids: vec![8],
        runner_ids: vec![9],
    };
    fixture.scripted_gh(&[GhStep::read(&["--version"],"gh version 2.101.0\n"),step(api_args("user"),json!({"login":"octocat"})),page_step("orgs/example/actions/runner-groups?per_page=100",json!([{"runner_groups":[]}])),write_step("orgs/example/actions/runner-groups","POST",Some(json!({"name":"build","visibility":"selected","allows_public_repositories":false,"restricted_to_workflows":true,"selected_workflows":request.settings.selected_workflows,"selected_repository_ids":[8],"runners":[9]})),group.clone()),step(api_args("orgs/example/actions/runner-groups/7"),group.clone()),page_step("orgs/example/actions/runner-groups/7/runners?per_page=100",json!([{"runners":[{"id":9}]}])),page_step("orgs/example/actions/runner-groups/7/repositories?per_page=100",json!([{"repositories":[{"id":8}]}])),GhStep::read(&["--version"],"gh version 2.101.0\n"),step(api_args("user"),json!({"login":"octocat"})),step(api_args("orgs/example/actions/runner-groups/7"),group),write_step("orgs/example/actions/runner-groups/7","DELETE",None,Value::Null),page_step("orgs/example/actions/runner-groups?per_page=100",json!([{"runner_groups":[]}]))]);
    let client = provider(&fixture);
    let created = client
        .create_runner_group(
            &identity(),
            "example",
            &request,
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(created.restricted_to_workflows);
    assert!(client
        .delete_runner_group(
            &identity(),
            "example",
            &created,
            &CancellationToken::default()
        )
        .unwrap()
        .is_empty());
    assert_eq!(journal(&fixture).len(), 12);
}
#[test]
fn actions_usage_keeps_milliseconds_bytes_and_provider_gigabyte_units_distinct() {
    let fixture = Fixture::new();
    let scope = ActionsScope::Organization {
        organization: "example".into(),
    };
    fixture.scripted_gh(&[step(api_args("organizations/example/actions/cache/storage-limit"),json!({"max_cache_size_gb":150})),step(api_args("organizations/example/actions/cache/retention-limit"),json!({"max_cache_retention_days":80})),step(api_args("orgs/example/actions/cache/usage"),json!({"total_active_caches_count":2,"total_active_caches_size_in_bytes":4096})),page_step("orgs/example/actions/cache/usage-by-repository?per_page=100",json!([{"repository_cache_usages":[{"full_name":"example/repo","active_caches_size_in_bytes":4096,"active_caches_count":2}]}])),step(api_args("repos/example/repo/actions/runs/123/timing"),json!({"billable":{"UBUNTU":{"total_ms":1234}},"run_duration_ms":4567}))]);
    let client = provider(&fixture);
    let limit = client
        .cache_limits(&scope, &CancellationToken::default())
        .unwrap();
    assert_eq!(limit.max_cache_size_gb, 150);
    assert_eq!(limit.max_cache_retention_days, Some(80));
    let usage = client
        .organization_cache_usage("example", &CancellationToken::default())
        .unwrap();
    assert_eq!(usage.total_active_caches_size_in_bytes, 4096);
    assert_eq!(usage.repositories[0]["full_name"], "example/repo");
    let timing = client
        .workflow_usage("example/repo", 123, &CancellationToken::default())
        .unwrap();
    assert_eq!(timing.billable["UBUNTU"]["total_ms"], 1234);
    assert!(timing.limitation.contains("closing down"));
    assert!(timing.limitation.contains("not billed cost"));
    assert_eq!(journal(&fixture).len(), 5);
}
#[test]
fn actions_secrets_vars_secret_selected_memberships_change_without_any_secret_value_transport() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        step(api_args("user"), json!({"login":"octocat"})),
        step(
            api_args("orgs/example/actions/secrets/TOKEN"),
            json!({"name":"TOKEN","visibility":"selected"}),
        ),
        page_step(
            "orgs/example/actions/secrets/TOKEN/repositories?per_page=100",
            json!([{"repositories":[{"id":8,"full_name":"example/repo"}]}]),
        ),
        write_step(
            "orgs/example/actions/secrets/TOKEN/repositories",
            "PUT",
            Some(json!({"selected_repository_ids":[10]})),
            Value::Null,
        ),
        page_step(
            "orgs/example/actions/secrets/TOKEN/repositories?per_page=100",
            json!([{"repositories":[{"id":10,"full_name":"example/other"}]}]),
        ),
    ]);
    let client = provider(&fixture);
    let scope = ActionsScope::Organization {
        organization: "example".into(),
    };
    let result = client
        .edit_setting_memberships(
            &identity(),
            &scope,
            &SettingMembershipEdit {
                name: "TOKEN".into(),
                secret: true,
                observed_ids: vec![8],
                selected_ids: vec![10],
            },
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(result[0].full_name, "example/other");
    assert_eq!(journal(&fixture).len(), 6);
    let payload = String::from_utf8(
        journal(&fixture)[4]["stdin_hex"]
            .as_str()
            .unwrap()
            .as_bytes()
            .chunks_exact(2)
            .map(|bytes| u8::from_str_radix(std::str::from_utf8(bytes).unwrap(), 16).unwrap())
            .collect(),
    )
    .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&payload).unwrap(),
        json!({"selected_repository_ids":[10]})
    );
    assert!(!payload.contains("encrypted_value"));
}
fn observed_comment() -> ConversationComment {
    ConversationComment {
        id: 9,
        node_id: "IC9".into(),
        body: "old".into(),
        author: Actor {
            login: "octocat".into(),
            ..Actor::default()
        },
        created_at: String::new(),
        updated_at: "t0".into(),
        url: "https://github.invalid/example/repo/pull/7#issuecomment-9".into(),
    }
}
fn comment_value(body: &str, updated: &str) -> Value {
    json!({"id":9,"node_id":"IC9","body":body,"updated_at":updated,"user":{"login":"octocat"},"issue_url":"https://api.github.invalid/repos/example/repo/issues/7","html_url":"https://github.invalid/example/repo/pull/7#issuecomment-9"})
}

#[test]
fn process_runner_preserves_argv_stdin_without_shell_interpretation() {
    let fixture = Fixture::new();
    let input = b"body \"quotes\"\n$(touch /tmp/unwanted)";
    let mut request = GhStep::read(
        &["api", "; touch should-not-exist", "$(id)", "--input", "-"],
        "ok",
    );
    request.stdin = input.to_vec();
    fixture.scripted_gh(&[request]);
    let out = CommandRunner::default()
        .run(
            runner_spec(
                &fixture,
                &["api", "; touch should-not-exist", "$(id)", "--input", "-"],
            )
            .input(input.to_vec()),
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(out.stdout, b"ok");
    let records = journal(&fixture);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["args"][1], "; touch should-not-exist");
    assert_eq!(records[0]["matched"], true);
    assert!(!fixture.repo().join("should-not-exist").exists());
}
#[test]
fn process_runner_limits_output_and_cleans_timed_out_children() {
    let fixture = Fixture::new();
    let large = "x".repeat(128 * 1024);
    fixture.scripted_gh(&[GhStep::read(&["large"], &large)]);
    let mut spec = runner_spec(&fixture, &["large"]);
    spec.output_limit = 1024;
    let result = CommandRunner::default().run(spec, &CancellationToken::default());
    assert_eq!(result.unwrap_err().kind, FailureKind::OutputLimit);
    let fixture = Fixture::new();
    let mut slow = GhStep::read(&["slow"], "late");
    slow.delay_ms = 10_000;
    fixture.scripted_gh(&[slow]);
    let start = Instant::now();
    let result = CommandRunner::default().run(
        runner_spec(&fixture, &["slow"]).timeout(Duration::from_millis(100)),
        &CancellationToken::default(),
    );
    assert_eq!(result.unwrap_err().kind, FailureKind::Timeout);
    assert!(start.elapsed() < Duration::from_secs(2));
    assert_process_gone(journal(&fixture)[0]["pid"].as_u64().unwrap());
}
fn assert_process_gone(pid: u64) {
    #[cfg(unix)]
    {
        let result = unsafe { libc::kill(pid as i32, 0) };
        assert_eq!(result, -1, "fixture child {pid} still alive");
    }
}
#[test]
fn process_runner_cancellation_and_ambiguous_write_require_reconciliation() {
    let fixture = Fixture::new();
    let mut slow = GhStep::read(&["write"], "late");
    slow.delay_ms = 10_000;
    fixture.scripted_gh(&[slow, GhStep::read(&["write"], "done")]);
    let runner = CommandRunner::default();
    let cancel = CancellationToken::default();
    let worker_runner = runner.clone();
    let worker_cancel = cancel.clone();
    let spec = runner_spec(&fixture, &["write"]).write("host/repo/pr/1");
    let worker = thread::spawn(move || worker_runner.run(spec, &worker_cancel));
    let deadline = Instant::now() + Duration::from_secs(3);
    while journal(&fixture).is_empty() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    cancel.cancel();
    let failure = worker.join().unwrap().unwrap_err();
    assert_eq!(failure.kind, FailureKind::Cancelled);
    assert!(failure.outcome_unknown);
    assert_process_gone(journal(&fixture)[0]["pid"].as_u64().unwrap());
    let retry = runner
        .run(
            runner_spec(&fixture, &["write"]).write("host/repo/pr/1"),
            &CancellationToken::default(),
        )
        .unwrap_err();
    assert_eq!(retry.kind, FailureKind::ReconciliationRequired);
    assert_eq!(journal(&fixture).len(), 1);
    runner.mark_reconciled("host/repo/pr/1");
    assert_eq!(
        runner
            .run(
                runner_spec(&fixture, &["write"]).write("host/repo/pr/1"),
                &CancellationToken::default()
            )
            .unwrap()
            .stdout,
        b"done"
    );
    assert_eq!(journal(&fixture).len(), 2);
}
#[test]
fn process_runner_coalesces_identical_reads_and_serializes_writes() {
    let fixture = Fixture::new();
    let mut slow = GhStep::read(&["read"], "result");
    slow.delay_ms = 150;
    fixture.scripted_gh(&[slow]);
    let runner = CommandRunner::default();
    let barrier = Arc::new(Barrier::new(3));
    let children: Vec<_> = (0..2)
        .map(|_| {
            let runner = runner.clone();
            let barrier = barrier.clone();
            let spec = runner_spec(&fixture, &["read"]);
            thread::spawn(move || {
                barrier.wait();
                runner
                    .run(spec, &CancellationToken::default())
                    .unwrap()
                    .stdout
            })
        })
        .collect();
    barrier.wait();
    for child in children {
        assert_eq!(child.join().unwrap(), b"result");
    }
    assert_eq!(journal(&fixture).len(), 1);
    let fixture = Fixture::new();
    let mut slow = GhStep::read(&["write"], "one");
    slow.delay_ms = 150;
    fixture.scripted_gh(&[slow, GhStep::read(&["write"], "two")]);
    let runner = CommandRunner::default();
    let a = runner.clone();
    let spec = runner_spec(&fixture, &["write"]).write("same");
    let first = thread::spawn(move || a.run(spec, &CancellationToken::default()).unwrap());
    let deadline = Instant::now() + Duration::from_secs(3);
    while journal(&fixture).is_empty() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    let start = Instant::now();
    let second = runner
        .run(
            runner_spec(&fixture, &["write"]).write("same"),
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(start.elapsed() > Duration::from_millis(100));
    assert_eq!(first.join().unwrap().stdout, b"one");
    assert_eq!(second.stdout, b"two");
}
#[test]
fn process_runner_sanitizes_controls_and_redacts_tokens_in_failures() {
    assert_eq!(
        sanitize_terminal("hello\x1b[31mred\x1b[0m\x1b]52;c;clipboard\x07\u{202e}!"),
        "hellored!"
    );
    assert_eq!(
        redact("ghp_ABC github_pat_123 password", &["password".into()]),
        "[REDACTED] [REDACTED] [REDACTED]"
    );
    let fixture = Fixture::new();
    let mut bad = GhStep::read(&["error"], "");
    bad.status = 1;
    bad.stderr = b"\x1b[31mghp_TOPSECRET\x1b[0m".to_vec();
    fixture.scripted_gh(&[bad]);
    let error = CommandRunner::default()
        .run(
            runner_spec(&fixture, &["error"]),
            &CancellationToken::default(),
        )
        .unwrap_err();
    assert!(!error.message.contains("TOPSECRET"));
    assert!(!error.message.contains('\x1b'));
    assert_eq!(error.exit_code, Some(1));
}

#[test]
fn process_runner_independent_subscriber_survives_owner_cancellation() {
    let fixture = Fixture::new();
    let mut slow = GhStep::read(&["read"], "owner");
    slow.delay_ms = 10_000;
    fixture.scripted_gh(&[slow, GhStep::read(&["read"], "subscriber")]);
    let runner = CommandRunner::default();
    let owner_cancel = CancellationToken::default();
    let token = owner_cancel.clone();
    let owner_runner = runner.clone();
    let spec = runner_spec(&fixture, &["read"]);
    let owner = thread::spawn(move || owner_runner.run(spec, &token));
    let deadline = Instant::now() + Duration::from_secs(3);
    while journal(&fixture).is_empty() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    let barrier = Arc::new(Barrier::new(2));
    let ready = barrier.clone();
    let spec = runner_spec(&fixture, &["read"]);
    let subscriber = thread::spawn(move || {
        ready.wait();
        runner.run(spec, &CancellationToken::default())
    });
    barrier.wait();
    owner_cancel.cancel();
    assert_eq!(
        owner.join().unwrap().unwrap_err().kind,
        FailureKind::Cancelled
    );
    assert_eq!(subscriber.join().unwrap().unwrap().stdout, b"subscriber");
    assert_eq!(journal(&fixture).len(), 2);
}

#[test]
fn process_runner_redacts_declared_secret_outputs_and_preserves_raw_git_bytes() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[GhStep::read(&["secret"], "saved custom-secret-value")]);
    let mut spec = runner_spec(&fixture, &["secret"]);
    spec.sensitive_values.push("custom-secret-value".into());
    let output = CommandRunner::default()
        .run(spec, &CancellationToken::default())
        .unwrap();
    assert_eq!(output.stdout, b"saved [REDACTED]");
    assert!(!output.stdout_text().contains("custom-secret-value"));
    let fixture = Fixture::new();
    let raw = b"name\xff\0\x1b[31m";
    let mut request = GhStep::read(&["raw-git"], "");
    request.stdout = raw.to_vec();
    fixture.scripted_gh(&[request]);
    let output = CommandRunner::default()
        .run(
            runner_spec(&fixture, &["raw-git"]),
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(output.stdout, raw);
    assert!(!output.stdout_text().contains('\x1b'));
}
#[test]
fn gh_provider_resolves_host_identity_and_actionable_auth_errors() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[
        GhStep::read(&["--version"], "gh version 2.101.0 (2026-09-15)\n"),
        step(api_args("user"), json!({"login":"OctoCat","id":1})),
    ]);
    let id = provider(&fixture)
        .identity(&CancellationToken::default())
        .unwrap();
    assert_eq!(id.login, "octocat");
    assert_eq!(id.host, "github.invalid");
    assert_eq!(id.account_id, Some(1));
    assert!(journal(&fixture).iter().all(|v| v["matched"] == true));
    let fixture = Fixture::new();
    let mut denied = step(api_args("user"), Value::Null);
    denied.status = 1;
    denied.stderr = b"HTTP 401: requires gh auth login".to_vec();
    fixture.scripted_gh(&[GhStep::read(&["--version"], "gh version 2.101.0\n"), denied]);
    let error = provider(&fixture)
        .identity(&CancellationToken::default())
        .unwrap_err();
    assert!(matches!(error.kind, GithubErrorKind::Authentication));
    assert!(error.message.contains("gh auth login"));
    let error = GithubProvider::new("github.invalid")
        .unwrap()
        .with_program(fixture.root.join("missing-gh"))
        .identity(&CancellationToken::default())
        .unwrap_err();
    assert!(matches!(error.kind, GithubErrorKind::MissingGh));
    assert!(error.message.contains("local Git remains available"));
}
#[test]
fn pr_inbox_preserves_cross_repo_membership_and_explicit_partial_coverage() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[
        search_step("author", vec![pr(1, "a/repo"), pr(1, "b/repo")], 2, false),
        search_step("assignee", vec![pr(1, "a/repo")], 1, false),
        search_step("review-requested", vec![pr(2, "a/repo")], 1, true),
    ]);
    let inbox = provider(&fixture)
        .personal_inbox(&identity(), &[], &CancellationToken::default())
        .unwrap();
    assert_eq!(inbox.pull_requests.len(), 3);
    let shared = inbox
        .pull_requests
        .iter()
        .find(|p| p.repository == "a/repo" && p.number == 1)
        .unwrap();
    assert!(shared.queues.contains(&Queue::Authored));
    assert!(shared.queues.contains(&Queue::Assigned));
    assert!(!inbox.coverage.complete);
    assert_eq!(inbox.coverage.total_matches, 4);
    assert_eq!(shared.author.label(), "@alice");
    assert!(shared.author.avatar_url.is_some());
}
#[test]
fn gh_provider_paginates_organization_repositories_and_preserves_permissions() {
    let fixture = Fixture::new();
    let mut args = api_args("orgs/example/repos?per_page=100&type=all");
    args.extend(["--paginate".into(), "--slurp".into()]);
    fixture.scripted_gh(&[step(args,json!([[{"id":1,"full_name":"example/one","default_branch":"main","permissions":{"pull":true,"push":false}}],[{"id":2,"full_name":"example/two","private":true,"permissions":{"admin":true,"pull":true}}]]))]);
    let repos = provider(&fixture)
        .organization_repositories("example", &CancellationToken::default())
        .unwrap();
    assert_eq!(repos.len(), 2);
    assert_eq!(repos[1].full_name, "example/two");
    assert!(repos[1].permissions.admin);
    assert!(!repos[0].permissions.push);
    assert_eq!(repos[0].host, "github.invalid");
}
#[test]
fn pr_comments_validate_account_and_head_before_exact_json_write() {
    let fixture = Fixture::new();
    let mut summary = pr(7, "example/repo");
    summary["head"]["sha"] = json!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    let body = "multiline\n$(touch never) \"quoted\"";
    let mut write_args = api_args("repos/example/repo/issues/7/comments");
    write_args[5] = "POST".into();
    write_args.extend(["--input".into(), "-".into()]);
    let mut write = step(
        write_args,
        json!({"id":99,"node_id":"IC_99","body":body,"user":{"login":"octocat"},"html_url":"https://github.invalid/comment/99"}),
    );
    write.stdin = serde_json::to_vec(&json!({"body":body})).unwrap();
    fixture.scripted_gh(&[
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        step(api_args("user"), json!({"login":"octocat"})),
        step(api_args("repos/example/repo/pulls/7"), summary),
        write,
    ]);
    let comment = provider(&fixture)
        .comment(
            &identity(),
            "example/repo",
            7,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            body,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(comment.body, body);
    assert_eq!(comment.author.login, "octocat");
    let records = journal(&fixture);
    assert_eq!(records.len(), 4);
    assert!(records.iter().all(|r| r["matched"] == true));
    let fixture = Fixture::new();
    fixture.scripted_gh(&[
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        step(api_args("user"), json!({"login":"other-account"})),
    ]);
    let error = provider(&fixture)
        .comment(
            &identity(),
            "example/repo",
            7,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "draft",
            &CancellationToken::default(),
        )
        .unwrap_err();
    assert!(matches!(error.kind, GithubErrorKind::IdentityChanged));
    assert_eq!(journal(&fixture).len(), 2);
}
#[test]
fn gh_provider_stale_head_blocks_mutation_and_inventory_records_gaps() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        step(api_args("user"), json!({"login":"octocat"})),
        step(
            api_args("repos/example/repo/pulls/7"),
            pr(7, "example/repo"),
        ),
    ]);
    let error = provider(&fixture)
        .comment(
            &identity(),
            "example/repo",
            7,
            "cccccccccccccccccccccccccccccccccccccccc",
            "draft",
            &CancellationToken::default(),
        )
        .unwrap_err();
    assert!(matches!(error.kind, GithubErrorKind::StaleHead));
    assert_eq!(journal(&fixture).len(), 3);
    let inventory = provider(&fixture).capabilities();
    assert_eq!(inventory.len(), 33);
    assert!(inventory.iter().all(|c| !c.permission.is_empty()
        && !c.concurrency_guard.is_empty()
        && c.task.starts_with("TASK-")));
    let gap = inventory
        .iter()
        .find(|c| c.id == "actions-artifacts")
        .unwrap();
    assert!(!gap.implemented);
    assert!(gap.reason.contains("TASK-56"));
}
#[test]
fn actions_runs_keep_actor_attempt_and_commit_identity() {
    let fixture = Fixture::new();
    let sha = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let mut args = api_args(&format!(
        "repos/example/repo/actions/runs?per_page=100&head_sha={sha}"
    ));
    args.extend(["--paginate".into(), "--slurp".into()]);
    fixture.scripted_gh(&[step(args,json!([{"workflow_runs":[{"id":123,"workflow_id":42,"head_sha":sha,"head_branch":"feature","run_attempt":2,"actor":{"login":"alice"},"status":"completed","conclusion":"failure","name":"CI"}]}]))]);
    let runs = provider(&fixture)
        .runs("example/repo", Some(sha), &CancellationToken::default())
        .unwrap();
    assert_eq!(runs[0].id, 123);
    assert_eq!(runs[0].attempt, 2);
    assert_eq!(runs[0].head_sha, sha);
    assert_eq!(runs[0].actor.label(), "@alice");
    assert_eq!(runs[0].conclusion, "failure");
}

#[test]
fn pr_inbox_exposes_search_caps_and_empty_page_inconsistency() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[
        search_step("author", vec![], 1001, false),
        search_step("assignee", vec![], 0, false),
        search_step("review-requested", vec![], 0, false),
    ]);
    let inbox = provider(&fixture)
        .personal_inbox(&identity(), &[], &CancellationToken::default())
        .unwrap();
    assert!(!inbox.coverage.complete);
    assert_eq!(inbox.coverage.total_matches, 1001);
    assert!(inbox
        .coverage
        .reasons
        .iter()
        .any(|s| s.contains("at most 1000")));
    assert!(inbox
        .coverage
        .reasons
        .iter()
        .any(|s| s.contains("0 of 1001")));
}

#[test]
fn pr_details_loads_all_pages_and_keeps_canonical_files_and_actor_provenance() {
    let fixture = Fixture::new();
    let mut summary = pr(7, "example/repo");
    summary["changed_files"] = json!(2);
    let list = |endpoint: &str, pages: Value| {
        let mut args = api_args(endpoint);
        args.extend(["--paginate".into(), "--slurp".into()]);
        step(args, pages)
    };
    fixture.scripted_gh(&[
        step(api_args("repos/example/repo/pulls/7"),summary.clone()),
        list("repos/example/repo/issues/7/comments?per_page=100",json!([[{"id":1,"body":"first","user":{"login":"alice"}}],[{"id":2,"body":"second","user":{"login":"bob","type":"Bot"}}]])),
        list("repos/example/repo/pulls/7/commits?per_page=100",json!([[{"sha":"ccccccc","author":null,"committer":{"login":"verified-committer"},"commit":{"message":"change","author":{"name":"Local Author","date":"2026-10-12T00:00:00Z"},"committer":{"name":"Other Name"}},"parents":[{"sha":"bbbbbbb"}]}]])),
        list("repos/example/repo/pulls/7/files?per_page=100",json!([[{"filename":"new.rs","previous_filename":"old.rs","status":"renamed","additions":2,"deletions":1,"patch":"@@ -1 +1,2 @@\n-old\n+new\n+extra"}],[{"filename":"image.png","status":"added","additions":0,"deletions":0}]])),
        list("repos/example/repo/pulls/7/reviews?per_page=100",json!([[{"id":5,"state":"APPROVED"}]])),
        list("repos/example/repo/issues/7/timeline?per_page=100",json!([[{"event":"review_requested","actor":{"login":"alice"}}]])),
        step(api_args("repos/example/repo/pulls/7"),summary),
    ]);
    let details = provider(&fixture)
        .pr_details("example/repo", 7, &CancellationToken::default())
        .unwrap();
    assert_eq!(details.comments.len(), 2);
    assert_eq!(details.comments[1].author.actor_type, "Bot");
    assert!(details.commits[0].author.login.is_empty());
    assert_eq!(details.commits[0].author_name, "Local Author");
    assert_eq!(details.commits[0].committer.login, "verified-committer");
    assert_eq!(details.files[0].previous_path.as_deref(), Some("old.rs"));
    assert!(details.files[1].patch.is_none());
    assert_eq!(
        details.summary.head_sha,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    assert!(details.coverage.complete);
    assert_eq!(journal(&fixture).len(), 7);
}

#[test]
fn actions_run_details_use_exact_attempt_and_sanitize_logs() {
    let fixture = Fixture::new();
    let mut jobs = api_args("repos/example/repo/actions/runs/123/attempts/3/jobs?per_page=100");
    jobs.extend(["--paginate".into(), "--slurp".into()]);
    fixture.scripted_gh(&[
        step(api_args("repos/example/repo/actions/runs/123"),json!({"id":123,"run_attempt":3,"head_sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","actor":{"login":"octocat"}})),
        step(jobs,json!([{"jobs":[{"id":456,"name":"test (linux)","steps":[{"number":1,"name":"test","conclusion":"failure"}]}]}])),
        GhStep::read(&["run","view","123","--repo","github.invalid/example/repo","--log","--job","456"],"line\x1b]52;c;clipboard\x07\x1b[31m failed\x1b[0m\n"),
    ]);
    let gh = provider(&fixture);
    let details = gh
        .run_details("example/repo", 123, &CancellationToken::default())
        .unwrap();
    assert_eq!(details.run.attempt, 3);
    assert_eq!(details.jobs[0]["id"], 456);
    assert_eq!(details.jobs[0]["steps"][0]["conclusion"], "failure");
    assert_eq!(
        gh.run_logs(
            "example/repo",
            123,
            Some(456),
            &CancellationToken::default()
        )
        .unwrap(),
        "line failed\n"
    );
    assert_eq!(journal(&fixture).len(), 3);
}

#[test]
fn gh_provider_rejects_unsupported_versions_and_invalid_targets_without_requests() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[GhStep::read(&["--version"], "gh version 1.0.0\n")]);
    let error = provider(&fixture)
        .identity(&CancellationToken::default())
        .unwrap_err();
    assert!(matches!(error.kind, GithubErrorKind::Unsupported));
    assert_eq!(journal(&fixture).len(), 1);
    let fixture = Fixture::new();
    assert!(provider(&fixture)
        .repository("../repo", &CancellationToken::default())
        .is_err());
    assert!(provider(&fixture)
        .runs(
            "example/repo",
            Some("HEAD;payload"),
            &CancellationToken::default()
        )
        .is_err());
    assert!(GithubProvider::new("--hostname evil").is_err());
    assert!(journal(&fixture).is_empty());
}

#[test]
fn remote_tree_keeps_immutable_identity_truncation_and_raw_blob_bytes() {
    let fixture = Fixture::new();
    let sha = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let blob = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let mut blob_args = api_args(&format!("repos/example/repo/git/blobs/{blob}"));
    blob_args.extend([
        "--header".into(),
        "Accept: application/vnd.github.raw+json".into(),
        "--allow-escape-sequences".into(),
    ]);
    let raw = b"\x89PNG\0\xff\n";
    let mut blob_step = step(blob_args, Value::Null);
    blob_step.stdout = raw.to_vec();
    fixture.scripted_gh(&[
        step(api_args(&format!("repos/example/repo/git/trees/{sha}?recursive=1")),json!({"sha":sha,"truncated":true,"tree":[{"path":"old\nname.png","mode":"100644","type":"blob","sha":blob,"size":8},{"path":"vendor","mode":"160000","type":"commit","sha":"cccccccccccccccccccccccccccccccccccccccc"}]})),
        blob_step,
    ]);
    let gh = provider(&fixture);
    let tree = gh
        .remote_tree("example/repo", sha, true, &CancellationToken::default())
        .unwrap();
    assert_eq!(tree.sha, sha);
    assert!(tree.truncated);
    assert!(tree.recursive);
    assert_eq!(tree.entries[0].path, "old\nname.png");
    assert_eq!(tree.entries[1].object_type, "commit");
    assert_eq!(
        gh.remote_blob("example/repo", blob, &CancellationToken::default())
            .unwrap(),
        raw
    );
    assert!(
        fs::read_dir(fixture.repo()).unwrap().next().is_none(),
        "remote browsing changed local checkout"
    );
    assert_eq!(journal(&fixture).len(), 2);
}

#[test]
fn pr_diff_retains_expected_shas_and_rejects_changed_review_coordinates() {
    let fixture = Fixture::new();
    let mut changed = pr(7, "example/repo");
    changed["head"]["sha"] = json!("cccccccccccccccccccccccccccccccccccccccc");
    fixture.scripted_gh(&[
        step(
            api_args("repos/example/repo/pulls/7"),
            pr(7, "example/repo"),
        ),
        GhStep::read(
            &[
                "pr",
                "diff",
                "7",
                "--repo",
                "github.invalid/example/repo",
                "--color",
                "never",
            ],
            "diff --git a/file b/file\n@@ -1 +1 @@\n-old\n+new\n",
        ),
        step(api_args("repos/example/repo/pulls/7"), changed),
    ]);
    let gh = provider(&fixture);
    let target = gh
        .pr_summary("example/repo", 7, &CancellationToken::default())
        .unwrap();
    let error = gh
        .pr_diff_at(&target, &CancellationToken::default())
        .unwrap_err();
    assert!(matches!(error.kind, GithubErrorKind::StaleHead));
    assert!(error.message.contains("review lines"));
    assert_eq!(journal(&fixture).len(), 3);
}

#[test]
fn gh_provider_missing_required_host_fields_has_an_actionable_diagnostic() {
    let fixture = Fixture::new();
    let mut missing = pr(7, "example/repo");
    missing["head"] = json!({});
    fixture.scripted_gh(&[step(api_args("repos/example/repo/pulls/7"), missing)]);
    let error = provider(&fixture)
        .pr_summary("example/repo", 7, &CancellationToken::default())
        .unwrap_err();
    assert!(matches!(error.kind, GithubErrorKind::Unsupported));
    assert!(error.message.contains("base/head"));
    let fixture = Fixture::new();
    fixture.scripted_gh(&[step(
        api_args("repos/example/repo/actions/runs/123"),
        json!({"id":123,"status":"completed"}),
    )]);
    let error = provider(&fixture)
        .run_details("example/repo", 123, &CancellationToken::default())
        .unwrap_err();
    assert!(matches!(error.kind, GithubErrorKind::Unsupported));
    assert!(error.message.contains("attempt"));
    assert_eq!(journal(&fixture).len(), 1);
}

#[test]
fn process_runner_waiting_write_deadline_does_not_launch_a_duplicate() {
    let fixture = Fixture::new();
    let mut slow = GhStep::read(&["write"], "late");
    slow.delay_ms = 10_000;
    fixture.scripted_gh(&[slow]);
    let runner = CommandRunner::default();
    let owner_cancel = CancellationToken::default();
    let token = owner_cancel.clone();
    let worker = runner.clone();
    let spec = runner_spec(&fixture, &["write"]).write("deadline-resource");
    let owner = thread::spawn(move || worker.run(spec, &token));
    let deadline = Instant::now() + Duration::from_secs(3);
    while journal(&fixture).is_empty() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    let error = runner
        .run(
            runner_spec(&fixture, &["write"])
                .write("deadline-resource")
                .timeout(Duration::from_millis(40)),
            &CancellationToken::default(),
        )
        .unwrap_err();
    assert_eq!(error.kind, FailureKind::Timeout);
    assert!(!error.outcome_unknown);
    assert_eq!(journal(&fixture).len(), 1);
    owner_cancel.cancel();
    assert_eq!(
        owner.join().unwrap().unwrap_err().kind,
        FailureKind::Cancelled
    );
}

#[test]
fn pr_metadata_edits_only_selected_text_and_detects_concurrent_edits() {
    let fixture = Fixture::new();
    let mut changed = pr(7, "example/repo");
    changed["title"] = json!("New title");
    changed["body"] = json!("multiline\n\"body\"");
    let mut steps = guard_steps();
    steps.extend([
        write_step(
            "repos/example/repo/pulls/7",
            "PATCH",
            Some(json!({"title":"New title","body":"multiline\n\"body\""})),
            changed.clone(),
        ),
        step(api_args("repos/example/repo/pulls/7"), changed),
    ]);
    fixture.scripted_gh(&steps);
    let result = provider(&fixture)
        .edit_pr_text(
            &identity(),
            &target(),
            &PrTextEdit {
                title: Some("New title".into()),
                body: Some("multiline\n\"body\"".into()),
                base: None,
            },
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(result.pr.title, "New title");
    assert_eq!(result.base_ref, "main");
    assert_eq!(journal(&fixture).len(), 5);
    let fixture = Fixture::new();
    let mut steps = guard_steps();
    let mut concurrent = pr(7, "example/repo");
    concurrent["title"] = json!("Someone else's edit");
    steps[2] = step(api_args("repos/example/repo/pulls/7"), concurrent);
    fixture.scripted_gh(&steps);
    let error = provider(&fixture)
        .edit_pr_text(
            &identity(),
            &target(),
            &PrTextEdit {
                title: Some("overwrite".into()),
                ..PrTextEdit::default()
            },
            &CancellationToken::default(),
        )
        .unwrap_err();
    assert!(matches!(error.kind, GithubErrorKind::StaleHead));
    assert_eq!(journal(&fixture).len(), 3);
}

#[test]
fn pr_metadata_labels_assignees_and_explicit_milestone_removal_read_back() {
    let fixture = Fixture::new();
    let observed = json!({"labels":[{"name":"old"}],"assignees":[],"milestone":{"number":2}});
    let updated =
        json!({"labels":[{"name":"bug"}],"assignees":[{"login":"octocat"}],"milestone":null});
    let mut steps = guard_steps();
    steps.extend([
        step(api_args("repos/example/repo/issues/7"), observed.clone()),
        write_step(
            "repos/example/repo/issues/7",
            "PATCH",
            Some(json!({"labels":["bug"],"assignees":["octocat"],"milestone":null})),
            updated.clone(),
        ),
        step(api_args("repos/example/repo/issues/7"), updated),
    ]);
    fixture.scripted_gh(&steps);
    let edit = IssueMetadataEdit {
        labels: Some(vec!["bug".into()]),
        assignees: Some(vec!["octocat".into()]),
        milestone: Some(None),
    };
    let result = provider(&fixture)
        .edit_issue_metadata(
            &identity(),
            &target(),
            &edit,
            &observed,
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(result["milestone"].is_null());
    assert_eq!(result["labels"][0]["name"], "bug");
    assert_eq!(journal(&fixture).len(), 6);
}

#[test]
fn conversation_comments_edit_delete_use_owned_id_and_fresh_body() {
    let fixture = Fixture::new();
    let mut steps = guard_steps();
    steps.extend([
        step(
            api_args("repos/example/repo/issues/comments/9"),
            comment_value("old", "t0"),
        ),
        write_step(
            "repos/example/repo/issues/comments/9",
            "PATCH",
            Some(json!({"body":"new\ncomment"})),
            comment_value("new\ncomment", "t1"),
        ),
        step(
            api_args("repos/example/repo/issues/comments/9"),
            comment_value("new\ncomment", "t1"),
        ),
    ]);
    fixture.scripted_gh(&steps);
    let comment = provider(&fixture)
        .edit_comment(
            &identity(),
            &target(),
            &observed_comment(),
            CommentKind::Conversation,
            "new\ncomment",
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(comment.body, "new\ncomment");
    assert_eq!(comment.id, 9);
    let fixture = Fixture::new();
    let mut steps = guard_steps();
    steps.extend([
        step(
            api_args("repos/example/repo/issues/comments/9"),
            comment_value("old", "t0"),
        ),
        write_step(
            "repos/example/repo/issues/comments/9",
            "DELETE",
            None,
            Value::Null,
        ),
        page_step(
            "repos/example/repo/issues/7/comments?per_page=100",
            json!([[]]),
        ),
    ]);
    fixture.scripted_gh(&steps);
    provider(&fixture)
        .delete_comment(
            &identity(),
            &target(),
            &observed_comment(),
            CommentKind::Conversation,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(journal(&fixture).len(), 6);
    let fixture = Fixture::new();
    let mut steps = guard_steps();
    steps.push(step(
        api_args("repos/example/repo/issues/comments/9"),
        comment_value("concurrent", "t2"),
    ));
    fixture.scripted_gh(&steps);
    let error = provider(&fixture)
        .delete_comment(
            &identity(),
            &target(),
            &observed_comment(),
            CommentKind::Conversation,
            &CancellationToken::default(),
        )
        .unwrap_err();
    assert!(matches!(error.kind, GithubErrorKind::StaleHead));
    assert_eq!(journal(&fixture).len(), 4);
}

#[test]
fn inline_comments_deleted_multiline_range_uses_canonical_patch_and_sha() {
    let fixture = Fixture::new();
    let sha = target().head_sha;
    let body = "Please fix both old lines";
    let response = json!({"id":50,"node_id":"PRRC_50","pull_request_url":"https://api.github.invalid/repos/example/repo/pulls/7","user":{"id":42,"login":"octocat","type":"User"},"path":"renamed.rs","line":2,"start_line":1,"side":"LEFT","start_side":"LEFT","commit_id":sha,"body":body});
    let mut steps = guard_steps();
    steps[1] = step(api_args("user"), json!({"login":"octocat","id":42}));
    steps.extend([page_step("repos/example/repo/pulls/7/files?per_page=100",json!([[{"filename":"renamed.rs","previous_filename":"old.rs","patch":"@@ -1,2 +1,2 @@\n-old one\n-old two\n+new one\n+new two"}]])),write_step("repos/example/repo/pulls/7/comments","POST",Some(json!({"path":"renamed.rs","line":2,"start_line":1,"side":"LEFT","start_side":"LEFT","commit_id":sha,"body":body})),response.clone()),step(api_args("repos/example/repo/pulls/comments/50"),response)]);
    steps.insert(
        4,
        step(
            api_args("repos/example/repo/pulls/7"),
            pr(7, "example/repo"),
        ),
    );
    fixture.scripted_gh(&steps);
    let comment = InlineComment {
        path: "renamed.rs".into(),
        side: DiffSide::Left,
        line: 2,
        start_line: Some(1),
        start_side: Some(DiffSide::Left),
        body: body.into(),
    };
    let result = provider(&fixture)
        .inline_comment(
            &verified_identity(),
            &target(),
            &comment,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(result["commit_id"], sha);
    assert_eq!(result["side"], "LEFT");
    assert_eq!(journal(&fixture).len(), 7);
    let mut invalid = comment;
    invalid.line = 100;
    assert!(invalid
        .validate_patch("@@ -1,2 +1,2 @@\n-a\n-b\n+c\n+d")
        .is_err());
    invalid.line = 2;
    invalid.start_side = Some(DiffSide::Right);
    assert!(invalid.validate().is_err());
}

#[test]
fn pending_reviews_typed_creation_confirms_all_decisions_numeric_author_and_exact_review_readback()
{
    for (event, verb, state, body) in [
        (ReviewEvent::Approve, "APPROVE", "APPROVED", ""),
        (ReviewEvent::Comment, "COMMENT", "COMMENTED", "review notes"),
        (
            ReviewEvent::RequestChanges,
            "REQUEST_CHANGES",
            "CHANGES_REQUESTED",
            "please fix",
        ),
    ] {
        let fixture = Fixture::new();
        let value = json!({"id":80,"node_id":"R80","state":state,"body":body,"commit_id":target().head_sha,"user":{"id":42,"login":"octocat","type":"User"},"pull_request_url":"https://api.github.invalid/repos/example/repo/pulls/7"});
        let mut steps = account_steps(42);
        steps.extend([
            step(
                api_args("repos/example/repo/pulls/7"),
                pr(7, "example/repo"),
            ),
            write_step(
                "repos/example/repo/pulls/7/reviews",
                "POST",
                Some(json!({"event":verb,"body":body,"commit_id":target().head_sha})),
                value.clone(),
            ),
            step(api_args("repos/example/repo/pulls/7/reviews/80"), value),
        ]);
        fixture.scripted_gh(&steps);
        let result = provider(&fixture)
            .create_review(
                &verified_identity(),
                &target(),
                event,
                body,
                &CancellationToken::default(),
            )
            .unwrap();
        assert_eq!(result["id"], 80);
        assert_eq!(result["node_id"], "R80");
        assert_eq!(result["state"], state);
        assert_eq!(result["body"], body);
        assert_eq!(journal(&fixture).len(), 5);
        assert!(journal(&fixture).iter().all(|row| row["matched"] == true));
    }
    let fixture = Fixture::new();
    assert!(provider(&fixture)
        .create_review(
            &identity(),
            &target(),
            ReviewEvent::Approve,
            "",
            &CancellationToken::default()
        )
        .is_err());
    assert!(journal(&fixture).is_empty());
}

#[test]
fn pending_reviews_typed_creation_failed_or_incompatible_readback_blocks_duplicate_publication() {
    let valid = json!({"id":80,"node_id":"R80","state":"COMMENTED","body":"review notes","commit_id":target().head_sha,"user":{"id":42,"login":"octocat"},"pull_request_url":"https://api.github.invalid/repos/example/repo/pulls/7"});
    let mut variants = vec![];
    for (field, bad) in [
        ("id", json!(81)),
        ("node_id", json!("OTHER_NODE")),
        ("body", json!("changed review")),
        ("state", json!("DISMISSED")),
        ("commit_id", json!(target().base_sha)),
        ("user", json!({"id":43,"login":"octocat"})),
        (
            "pull_request_url",
            json!("https://api.github.invalid/repos/other/repo/pulls/7"),
        ),
    ] {
        let mut value = valid.clone();
        value[field] = bad;
        variants.push(step(
            api_args("repos/example/repo/pulls/7/reviews/80"),
            value,
        ));
    }
    let mut failure = step(
        api_args("repos/example/repo/pulls/7/reviews/80"),
        Value::Null,
    );
    failure.status = 1;
    failure.stderr = b"HTTP503 review readback unavailable".to_vec();
    variants.push(failure);
    for readback in variants {
        let fixture = Fixture::new();
        let runner = CommandRunner::default();
        let mut steps = account_steps(42);
        steps.extend([
            step(
                api_args("repos/example/repo/pulls/7"),
                pr(7, "example/repo"),
            ),
            write_step(
                "repos/example/repo/pulls/7/reviews",
                "POST",
                Some(
                    json!({"event":"COMMENT","body":"review notes","commit_id":target().head_sha}),
                ),
                valid.clone(),
            ),
            readback,
        ]);
        fixture.scripted_gh(&steps);
        assert!(matches!(
            provider(&fixture)
                .with_runner(runner.clone())
                .create_review(
                    &verified_identity(),
                    &target(),
                    ReviewEvent::Comment,
                    "review notes",
                    &CancellationToken::default()
                )
                .unwrap_err()
                .kind,
            GithubErrorKind::AmbiguousWrite
        ));
        assert_eq!(
            runner
                .run(
                    runner_spec(&fixture, &["must-not-repeat-review"])
                        .write("github.invalid/example/repo/pull/7"),
                    &CancellationToken::default()
                )
                .unwrap_err()
                .kind,
            FailureKind::ReconciliationRequired
        );
        assert_eq!(journal(&fixture).len(), 5);
        assert!(journal(&fixture).iter().all(|row| row["matched"] == true));
        assert_eq!(
            journal(&fixture)
                .iter()
                .filter(|row| row["args"][5] == "POST")
                .count(),
            1
        );
    }
}

#[test]
fn pending_reviews_typed_creation_rejects_invalid_post_receipt_without_any_readback_or_retry() {
    let valid = json!({"id":80,"node_id":"R80","state":"COMMENTED","body":"review notes","commit_id":target().head_sha,"user":{"id":42,"login":"octocat"},"pull_request_url":"https://api.github.invalid/repos/example/repo/pulls/7"});
    for (field, bad) in [
        ("id", json!(0)),
        ("node_id", json!("")),
        ("body", json!("wrong body")),
        ("user", json!({"id":43,"login":"octocat"})),
        (
            "pull_request_url",
            json!("https://attacker.invalid/repos/example/repo/pulls/7"),
        ),
    ] {
        let fixture = Fixture::new();
        let runner = CommandRunner::default();
        let mut value = valid.clone();
        value[field] = bad;
        let mut steps = account_steps(42);
        steps.extend([
            step(
                api_args("repos/example/repo/pulls/7"),
                pr(7, "example/repo"),
            ),
            write_step(
                "repos/example/repo/pulls/7/reviews",
                "POST",
                Some(
                    json!({"event":"COMMENT","body":"review notes","commit_id":target().head_sha}),
                ),
                value,
            ),
        ]);
        fixture.scripted_gh(&steps);
        assert!(matches!(
            provider(&fixture)
                .with_runner(runner.clone())
                .create_review(
                    &verified_identity(),
                    &target(),
                    ReviewEvent::Comment,
                    "review notes",
                    &CancellationToken::default()
                )
                .unwrap_err()
                .kind,
            GithubErrorKind::AmbiguousWrite
        ));
        assert_eq!(
            runner
                .run(
                    runner_spec(&fixture, &["must-not-repeat-review"])
                        .write("github.invalid/example/repo/pull/7"),
                    &CancellationToken::default()
                )
                .unwrap_err()
                .kind,
            FailureKind::ReconciliationRequired
        );
        assert_eq!(journal(&fixture).len(), 4);
        assert!(journal(&fixture).iter().all(|row| row["matched"] == true));
    }
}

#[test]
fn pending_reviews_create_without_event_and_submit_exact_owned_review() {
    let pending = json!({"id":81,"node_id":"PRR81","state":"PENDING","body":"draft","commit_id":target().head_sha,"user":{"login":"octocat"}});
    let fixture = Fixture::new();
    let mut steps = guard_steps();
    steps.extend([
        page_step(
            "repos/example/repo/pulls/7/reviews?per_page=100",
            json!([[]]),
        ),
        write_step(
            "repos/example/repo/pulls/7/reviews",
            "POST",
            Some(json!({"body":"draft","commit_id":target().head_sha,"comments":[]})),
            pending.clone(),
        ),
    ]);
    fixture.scripted_gh(&steps);
    let review = provider(&fixture)
        .create_pending_review(
            &identity(),
            &target(),
            "draft",
            &[],
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(review.state, "PENDING");
    assert_eq!(review.id, 81);
    assert_eq!(journal(&fixture).len(), 5);
    let fixture = Fixture::new();
    let mut submitted = pending.clone();
    submitted["state"] = json!("APPROVED");
    let mut steps = guard_steps();
    steps.extend([
        step(api_args("repos/example/repo/pulls/7/reviews/81"), pending),
        write_step(
            "repos/example/repo/pulls/7/reviews/81/events",
            "POST",
            Some(json!({"event":"APPROVE","body":"approved"})),
            submitted.clone(),
        ),
        step(api_args("repos/example/repo/pulls/7/reviews/81"), submitted),
    ]);
    fixture.scripted_gh(&steps);
    let review = provider(&fixture)
        .submit_pending_review(
            &identity(),
            &target(),
            81,
            ReviewEvent::Approve,
            "approved",
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(review.state, "APPROVED");
    assert_eq!(journal(&fixture).len(), 6);
}

#[test]
fn pr_metadata_malformed_write_response_requires_reconciliation_before_retry() {
    let fixture = Fixture::new();
    let mut steps = guard_steps();
    let mut malformed = write_step(
        "repos/example/repo/pulls/7",
        "PATCH",
        Some(json!({"title":"new"})),
        Value::Null,
    );
    malformed.stdout = b"invalid JSON".to_vec();
    steps.push(malformed);
    steps.extend(guard_steps());
    fixture.scripted_gh(&steps);
    let gh = provider(&fixture);
    let edit = PrTextEdit {
        title: Some("new".into()),
        ..PrTextEdit::default()
    };
    let first = gh
        .edit_pr_text(&identity(), &target(), &edit, &CancellationToken::default())
        .unwrap_err();
    assert!(matches!(first.kind, GithubErrorKind::AmbiguousWrite));
    let retry = gh
        .edit_pr_text(&identity(), &target(), &edit, &CancellationToken::default())
        .unwrap_err();
    assert!(matches!(retry.kind, GithubErrorKind::AmbiguousWrite));
    let records = journal(&fixture);
    assert_eq!(records.len(), 7);
    assert_eq!(
        records.iter().filter(|r| r["args"][5] == "PATCH").count(),
        1
    );
}

#[test]
fn pr_successful_write_failed_readback_blocks_duplicate_submission() {
    let fixture = Fixture::new();
    let mut steps = guard_steps();
    let mut readback = step(api_args("repos/example/repo/pulls/7"), Value::Null);
    readback.status = 1;
    readback.stderr = b"HTTP 503: temporarily unavailable".to_vec();
    steps.extend([
        write_step(
            "repos/example/repo/pulls/7",
            "PATCH",
            Some(json!({"title":"new"})),
            json!({"number":7}),
        ),
        readback,
    ]);
    steps.extend(guard_steps());
    fixture.scripted_gh(&steps);
    let gh = provider(&fixture);
    let edit = PrTextEdit {
        title: Some("new".into()),
        ..PrTextEdit::default()
    };
    assert!(matches!(
        gh.edit_pr_text(&identity(), &target(), &edit, &CancellationToken::default())
            .unwrap_err()
            .kind,
        GithubErrorKind::AmbiguousWrite
    ));
    assert!(matches!(
        gh.edit_pr_text(&identity(), &target(), &edit, &CancellationToken::default())
            .unwrap_err()
            .kind,
        GithubErrorKind::AmbiguousWrite
    ));
    let rows = journal(&fixture);
    assert_eq!(rows.len(), 8);
    assert_eq!(
        rows.iter().filter(|row| row["args"][5] == "PATCH").count(),
        1
    );
    assert!(rows.iter().all(|row| row["matched"] == true));
}

#[test]
fn reactions_reject_same_pr_number_in_different_repository_before_mutation() {
    let fixture = Fixture::new();
    let mut steps = guard_steps();
    steps.push(step(
        api_args("repos/example/repo/issues/comments/9"),
        json!({"id":9,"issue_url":"https://api.github.invalid/repos/other/repo/issues/7"}),
    ));
    fixture.scripted_gh(&steps);
    assert!(matches!(
        provider(&fixture)
            .react_comment(
                &identity(),
                &target(),
                9,
                CommentKind::Conversation,
                Reaction::ThumbsUp,
                &CancellationToken::default()
            )
            .unwrap_err()
            .kind,
        GithubErrorKind::InvalidTarget
    ));
    assert_eq!(journal(&fixture).len(), 4);
}

#[test]
fn conversation_comments_inspection_uses_explicit_kind_exact_parent_and_fresh_body() {
    for kind in [CommentKind::Conversation, CommentKind::Review] {
        let endpoint = if kind == CommentKind::Conversation {
            "repos/example/repo/issues/comments/9"
        } else {
            "repos/example/repo/pulls/comments/9"
        };
        let parent_key = if kind == CommentKind::Conversation {
            "issue_url"
        } else {
            "pull_request_url"
        };
        let parent_kind = if kind == CommentKind::Conversation {
            "issues"
        } else {
            "pulls"
        };
        let mut raw = comment_value("changed on server", "t1");
        raw[parent_key] = json!(format!(
            "https://github.invalid/api/v3/repos/EXAMPLE/REPO/{parent_kind}/7"
        ));
        raw["user"]["type"] = json!("Bot");
        let fixture = Fixture::new();
        fixture.scripted_gh(&[step(api_args(endpoint), raw.clone())]);
        let fresh = provider(&fixture)
            .inspect_comment(
                &target(),
                &observed_comment(),
                kind,
                &CancellationToken::default(),
            )
            .unwrap();
        assert_eq!(fresh.id, 9);
        assert_eq!(fresh.node_id, "IC9");
        assert_eq!(fresh.body, "changed on server");
        assert_eq!(fresh.updated_at, "t1");
        assert_eq!(fresh.author.actor_type, "Bot");
        assert_eq!(
            observed_comment().body,
            "old",
            "inspection must not overwrite retained intent"
        );
        assert_eq!(journal(&fixture).len(), 1);
        for (field, invalid) in [
            ("id", json!(10)),
            ("node_id", json!("ANOTHER_NODE")),
            (
                parent_key,
                json!(format!(
                    "https://api.github.invalid/repos/other/repo/{parent_kind}/7"
                )),
            ),
            (
                parent_key,
                json!(format!(
                    "https://attacker.invalid/repos/example/repo/{parent_kind}/7"
                )),
            ),
            (
                parent_key,
                json!(format!(
                    "https://api.github.invalid/repos/example/repo/{parent_kind}/70"
                )),
            ),
        ] {
            let fixture = Fixture::new();
            let mut invalid_raw = raw.clone();
            invalid_raw[field] = invalid;
            fixture.scripted_gh(&[step(api_args(endpoint), invalid_raw)]);
            assert!(matches!(
                provider(&fixture)
                    .inspect_comment(
                        &target(),
                        &observed_comment(),
                        kind,
                        &CancellationToken::default()
                    )
                    .unwrap_err()
                    .kind,
                GithubErrorKind::InvalidTarget
            ));
            assert_eq!(journal(&fixture).len(), 1);
        }
    }
    let fixture = Fixture::new();
    let mut selected = observed_comment();
    selected.node_id.clear();
    assert!(provider(&fixture)
        .inspect_comment(
            &target(),
            &selected,
            CommentKind::Conversation,
            &CancellationToken::default()
        )
        .is_err());
    assert!(journal(&fixture).is_empty());
}

#[test]
fn conversation_comments_create_binds_exact_pr_node_and_confirms_numeric_author_and_comment_readback(
) {
    let mut valid = comment_value("new comment", "t1");
    valid["user"]["id"] = json!(42);
    let mut cases = vec![(valid.clone(), valid.clone(), true)];
    for (field, bad) in [
        ("id", json!(0)),
        ("node_id", json!("")),
        ("body", json!("wrong text")),
        (
            "issue_url",
            json!("https://api.github.invalid/repos/other/repo/issues/7"),
        ),
        ("user", json!({"id":43,"login":"octocat"})),
    ] {
        let mut value = valid.clone();
        value[field] = bad;
        cases.push((value, Value::Null, false));
    }
    for (field, bad) in [
        ("node_id", json!("OTHER_NODE")),
        ("body", json!("another edit")),
        ("user", json!({"id":43,"login":"octocat"})),
    ] {
        let mut value = valid.clone();
        value[field] = bad;
        cases.push((valid.clone(), value, true));
    }
    for (created, readback, read) in cases {
        let fixture = Fixture::new();
        let runner = CommandRunner::default();
        let mut steps = account_steps(42);
        steps.extend([
            step(
                api_args("repos/example/repo/pulls/7"),
                pr(7, "example/repo"),
            ),
            write_step(
                "repos/example/repo/issues/7/comments",
                "POST",
                Some(json!({"body":"new comment"})),
                created.clone(),
            ),
        ]);
        if read {
            steps.push(step(
                api_args("repos/example/repo/issues/comments/9"),
                readback.clone(),
            ));
        }
        fixture.scripted_gh(&steps);
        let result = provider(&fixture)
            .with_runner(runner.clone())
            .create_comment(
                &verified_identity(),
                &target(),
                "new comment",
                &CancellationToken::default(),
            );
        if created == valid && readback == valid {
            let comment = result.unwrap();
            assert_eq!(comment.id, 9);
            assert_eq!(comment.node_id, "IC9");
            assert_eq!(comment.body, "new comment");
        } else {
            assert!(matches!(
                result.unwrap_err().kind,
                GithubErrorKind::AmbiguousWrite
            ));
            assert_eq!(
                runner
                    .run(
                        runner_spec(&fixture, &["must-not-republish"])
                            .write("github.invalid/example/repo/pull/7"),
                        &CancellationToken::default()
                    )
                    .unwrap_err()
                    .kind,
                FailureKind::ReconciliationRequired
            );
        }
        assert_eq!(journal(&fixture).len(), 4 + usize::from(read));
        assert!(journal(&fixture).iter().all(|row| row["matched"] == true));
        assert_eq!(
            journal(&fixture)
                .iter()
                .filter(|row| row["args"][5] == "POST")
                .count(),
            1
        );
    }
    let fixture = Fixture::new();
    let mut same_sha_other_node = pr(7, "example/repo");
    same_sha_other_node["node_id"] = json!("OTHER_PR_NODE");
    let mut steps = account_steps(42);
    steps.push(step(
        api_args("repos/example/repo/pulls/7"),
        same_sha_other_node,
    ));
    fixture.scripted_gh(&steps);
    assert!(matches!(
        provider(&fixture)
            .create_comment(
                &verified_identity(),
                &target(),
                "new comment",
                &CancellationToken::default()
            )
            .unwrap_err()
            .kind,
        GithubErrorKind::InvalidTarget
    ));
    assert_eq!(journal(&fixture).len(), 3);
}

#[test]
fn conversation_comments_shared_publication_guards_review_lifecycle_and_merge_exact_nodes_and_hosts(
) {
    for action in 0..3 {
        for changed_host in [false, true] {
            let fixture = Fixture::new();
            if !changed_host {
                fixture.scripted_gh(&guard_steps());
            }
            let mut selected = target();
            if changed_host {
                selected.pr.host = "other.invalid".into();
            } else {
                selected.pr.id = "ANOTHER_PR_WITH_SAME_SHA".into();
            }
            let provider = provider(&fixture);
            let cancellation = CancellationToken::default();
            let error = match action {
                0 => provider
                    .review(
                        &identity(),
                        &selected,
                        ReviewEvent::Comment,
                        "review",
                        &cancellation,
                    )
                    .map(|_| ())
                    .unwrap_err(),
                1 => provider
                    .lifecycle(
                        &identity(),
                        &selected,
                        ghtui::providers::gh::PrLifecycle::Close,
                        &cancellation,
                    )
                    .map(|_| ())
                    .unwrap_err(),
                _ => provider
                    .merge(
                        &identity(),
                        &selected,
                        ghtui::providers::gh::MergeMethod::Squash,
                        false,
                        &cancellation,
                    )
                    .map(|_| ())
                    .unwrap_err(),
            };
            assert!(matches!(error.kind, GithubErrorKind::InvalidTarget));
            assert_eq!(journal(&fixture).len(), if changed_host { 0 } else { 3 });
            assert!(journal(&fixture).iter().all(|row| row["matched"] == true));
        }
    }
}

#[test]
fn conversation_comments_reaction_catalogue_preserves_ids_accounts_unknown_actors_and_pages() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[
        step(api_args("repos/example/repo/issues/comments/9"), comment_value("old", "t0")),
        page_step("repos/example/repo/issues/comments/9/reactions?per_page=100", json!([
            [{"id":11,"node_id":"R11","content":"heart","user":{"id":42,"login":"octocat","type":"User"},"created_at":"t1"}],
            [{"id":12,"node_id":"R12","content":"future-reaction","user":null,"created_at":"t2"}]
        ])),
    ]);
    let reactions = provider(&fixture)
        .comment_reactions(
            &target(),
            &observed_comment(),
            CommentKind::Conversation,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(reactions.len(), 2);
    assert_eq!(reactions[0].account_id, Some(42));
    assert_eq!(reactions[0].actor.login, "octocat");
    assert_eq!(reactions[1].content, "future-reaction");
    assert_eq!(reactions[1].account_id, None);
    assert_eq!(reactions[1].actor.actor_type, "Unknown");
    assert!(reactions[1].actor.login.is_empty());
    assert_eq!(journal(&fixture).len(), 2);
    for pages in [
        json!([[{"id":0,"content":"heart"}]]),
        json!([[{"id":11,"content":"heart"}],[{"id":11,"content":"heart"}]]),
        json!([[{"id":11,"content":""}]]),
    ] {
        let fixture = Fixture::new();
        fixture.scripted_gh(&[
            step(
                api_args("repos/example/repo/issues/comments/9"),
                comment_value("old", "t0"),
            ),
            page_step(
                "repos/example/repo/issues/comments/9/reactions?per_page=100",
                pages,
            ),
        ]);
        assert!(matches!(
            provider(&fixture)
                .comment_reactions(
                    &target(),
                    &observed_comment(),
                    CommentKind::Conversation,
                    &CancellationToken::default()
                )
                .unwrap_err()
                .kind,
            GithubErrorKind::Json
        ));
        assert_eq!(journal(&fixture).len(), 2);
    }
}

#[test]
fn conversation_comments_reaction_readback_confirms_numeric_actor_and_exact_effect_or_blocks_retry()
{
    let valid = json!({"id":11,"content":"heart","user":{"id":42,"login":"octocat"}});
    let variants = [
        valid.clone(),
        json!({"id":12,"content":"heart","user":{"id":42,"login":"octocat"}}),
        json!({"id":11,"content":"eyes","user":{"id":42,"login":"octocat"}}),
        json!({"id":11,"content":"heart","user":{"id":43,"login":"octocat"}}),
        json!({"id":11,"content":"heart","user":{"id":42,"login":"another-user"}}),
        json!({"id":11,"content":"heart","user":{"login":"octocat"}}),
    ];
    let mut cases = variants
        .into_iter()
        .map(|readback| (valid.clone(), readback))
        .collect::<Vec<_>>();
    cases.extend([
        (
            json!({"id":0,"content":"heart","user":{"id":42,"login":"octocat"}}),
            valid.clone(),
        ),
        (
            json!({"id":11,"content":"heart","user":{"id":43,"login":"octocat"}}),
            valid.clone(),
        ),
        (Value::Null, valid.clone()),
    ]);
    for (created, readback) in cases {
        let fixture = Fixture::new();
        let runner = CommandRunner::default();
        let mut steps = account_steps(42);
        steps.extend([
            step(
                api_args("repos/example/repo/pulls/7"),
                pr(7, "example/repo"),
            ),
            step(
                api_args("repos/example/repo/issues/comments/9"),
                comment_value("old", "t0"),
            ),
            write_step(
                "repos/example/repo/issues/comments/9/reactions",
                "POST",
                Some(json!({"content":"heart"})),
                created.clone(),
            ),
            page_step(
                "repos/example/repo/issues/comments/9/reactions?per_page=100",
                json!([[readback.clone()]]),
            ),
        ]);
        fixture.scripted_gh(&steps);
        let result = provider(&fixture)
            .with_runner(runner.clone())
            .react_comment(
                &verified_identity(),
                &target(),
                9,
                CommentKind::Conversation,
                Reaction::Heart,
                &CancellationToken::default(),
            );
        if readback == valid && created == valid {
            assert_eq!(result.unwrap()[0]["id"], 11);
        } else {
            assert!(matches!(
                result.unwrap_err().kind,
                GithubErrorKind::AmbiguousWrite
            ));
            let error = runner
                .run(
                    runner_spec(&fixture, &["must-not-launch"])
                        .write("github.invalid/example/repo/pull/7"),
                    &CancellationToken::default(),
                )
                .unwrap_err();
            assert_eq!(error.kind, FailureKind::ReconciliationRequired);
        }
        let rows = journal(&fixture);
        assert_eq!(rows.len(), 6);
        assert!(rows.iter().all(|row| row["matched"] == true));
        assert_eq!(
            rows.iter().filter(|row| row["args"][5] == "POST").count(),
            1
        );
    }
}

#[test]
fn conversation_comments_ownership_does_not_trust_a_reassigned_username_or_changed_node() {
    let fixture = Fixture::new();
    let mut raw = comment_value("old", "t0");
    raw["user"]["id"] = json!(43);
    let mut steps = account_steps(42);
    steps.extend([
        step(api_args("repos/example/repo/pulls/7"),pr(7,"example/repo")),
        step(api_args("repos/example/repo/issues/comments/9"),raw),
        step(api_args("repos/example/repo"),json!({"id":8,"full_name":"example/repo","permissions":{"pull":true,"push":false,"maintain":false,"admin":false}})),
    ]);
    fixture.scripted_gh(&steps);
    assert!(matches!(
        provider(&fixture)
            .delete_comment(
                &verified_identity(),
                &target(),
                &observed_comment(),
                CommentKind::Conversation,
                &CancellationToken::default()
            )
            .unwrap_err()
            .kind,
        GithubErrorKind::Permission
    ));
    assert_eq!(journal(&fixture).len(), 5);
    let fixture = Fixture::new();
    let mut steps = guard_steps();
    let mut raw = comment_value("old", "t0");
    raw["node_id"] = json!("OTHER_NODE");
    steps.push(step(api_args("repos/example/repo/issues/comments/9"), raw));
    fixture.scripted_gh(&steps);
    assert!(matches!(
        provider(&fixture)
            .edit_comment(
                &identity(),
                &target(),
                &observed_comment(),
                CommentKind::Conversation,
                "new",
                &CancellationToken::default()
            )
            .unwrap_err()
            .kind,
        GithubErrorKind::InvalidTarget
    ));
    assert_eq!(journal(&fixture).len(), 4);
}

#[test]
fn conversation_comments_reaction_removal_verifies_stable_account_and_exact_id_absence() {
    for actor_id in [42, 43] {
        let fixture = Fixture::new();
        let mut steps = account_steps(42);
        steps.extend([
            step(
                api_args("repos/example/repo/pulls/7"),
                pr(7, "example/repo"),
            ),
            step(
                api_args("repos/example/repo/issues/comments/9"),
                comment_value("old", "t0"),
            ),
            page_step(
                "repos/example/repo/issues/comments/9/reactions?per_page=100",
                json!([[{"id":11,"content":"heart","user":{"id":actor_id,"login":"octocat"}}]]),
            ),
        ]);
        if actor_id == 42 {
            steps.extend([
                write_step(
                    "repos/example/repo/issues/comments/9/reactions/11",
                    "DELETE",
                    None,
                    Value::Null,
                ),
                page_step(
                    "repos/example/repo/issues/comments/9/reactions?per_page=100",
                    json!([[{"id":12,"content":"eyes","user":{"id":7,"login":"alice"}}]]),
                ),
            ]);
        }
        fixture.scripted_gh(&steps);
        let result = provider(&fixture).delete_reaction(
            &verified_identity(),
            &target(),
            9,
            CommentKind::Conversation,
            11,
            &CancellationToken::default(),
        );
        if actor_id == 42 {
            let remaining = result.unwrap();
            assert_eq!(remaining.len(), 1);
            assert_eq!(remaining[0]["id"], 12);
            assert_eq!(journal(&fixture).len(), 7);
        } else {
            assert!(matches!(
                result.unwrap_err().kind,
                GithubErrorKind::Permission
            ));
            assert_eq!(journal(&fixture).len(), 5);
        }
        assert!(journal(&fixture).iter().all(|row| row["matched"] == true));
        assert_eq!(
            journal(&fixture)
                .iter()
                .filter(|row| row["args"][5] == "DELETE")
                .count(),
            usize::from(actor_id == 42)
        );
    }
}

#[test]
fn conversation_comments_pr_inspection_preserves_changed_effects_and_requires_producer_explicit_ack(
) {
    let fixture = Fixture::new();
    let runner = CommandRunner::default();
    let resource = "github.invalid/example/repo/pull/7";
    runner.mark_ambiguous(resource);
    let mut fresh = pr(7, "example/repo");
    fresh["head"]["sha"] = json!("cccccccccccccccccccccccccccccccccccccccc");
    fresh["state"] = json!("closed");
    fresh["merged"] = json!(true);
    let mut steps = empty_pr_detail_steps(fresh.clone());
    steps.extend(account_steps(42));
    steps.extend(empty_pr_detail_steps(fresh));
    steps.extend(account_steps(42));
    steps.push(GhStep::read(&["verified-effect-write"], "done"));
    fixture.scripted_gh(&steps);
    let client = provider(&fixture).with_runner(runner.clone());
    let details = client
        .reconcile_pr("example/repo", 7, &CancellationToken::default())
        .unwrap();
    assert!(details.summary.merged);
    let blocked = || {
        runner
            .run(
                runner_spec(&fixture, &["verified-effect-write"]).write(resource),
                &CancellationToken::default(),
            )
            .unwrap_err()
    };
    assert_eq!(
        blocked().kind,
        FailureKind::ReconciliationRequired,
        "ordinary fresh details must not acknowledge"
    );
    let observation = client
        .inspect_pr_target(
            &verified_identity(),
            &target(),
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(observation.identity().account_id, Some(42));
    assert_eq!(observation.target().head_sha, target().head_sha);
    assert_eq!(
        observation.details().summary.head_sha,
        "cccccccccccccccccccccccccccccccccccccccc"
    );
    assert!(observation.details().summary.merged);
    assert_eq!(
        blocked().kind,
        FailureKind::ReconciliationRequired,
        "inspection alone must not acknowledge"
    );
    assert!(matches!(
        provider(&fixture)
            .acknowledge_pr_observation(observation.clone())
            .unwrap_err()
            .kind,
        GithubErrorKind::IdentityChanged
    ));
    assert_eq!(blocked().kind, FailureKind::ReconciliationRequired);
    client
        .clone()
        .acknowledge_pr_observation(observation.clone())
        .unwrap();
    assert_eq!(
        runner
            .run(
                runner_spec(&fixture, &["verified-effect-write"]).write(resource),
                &CancellationToken::default()
            )
            .unwrap()
            .stdout,
        b"done"
    );
    runner.mark_ambiguous(resource);
    assert!(matches!(
        client
            .acknowledge_pr_observation(observation)
            .unwrap_err()
            .kind,
        GithubErrorKind::StaleHead
    ));
    assert_eq!(
        blocked().kind,
        FailureKind::ReconciliationRequired,
        "old cloned observation must not clear newer uncertainty"
    );
    assert_eq!(journal(&fixture).len(), 19);
    assert!(journal(&fixture).iter().all(|row| row["matched"] == true));
}

#[test]
fn conversation_comments_pr_inspection_rejects_missing_reassigned_and_mid_read_accounts() {
    let fixture = Fixture::new();
    assert!(matches!(
        provider(&fixture)
            .inspect_pr_target(&identity(), &target(), &CancellationToken::default())
            .unwrap_err()
            .kind,
        GithubErrorKind::IdentityChanged
    ));
    assert!(journal(&fixture).is_empty());
    let fixture = Fixture::new();
    fixture.scripted_gh(&account_steps(43));
    assert!(matches!(
        provider(&fixture)
            .inspect_pr_target(
                &verified_identity(),
                &target(),
                &CancellationToken::default()
            )
            .unwrap_err()
            .kind,
        GithubErrorKind::IdentityChanged
    ));
    assert_eq!(journal(&fixture).len(), 2);
    let fixture = Fixture::new();
    let mut steps = account_steps(42);
    steps.extend(empty_pr_detail_steps(pr(7, "example/repo")));
    steps.extend(account_steps(43));
    fixture.scripted_gh(&steps);
    assert!(matches!(
        provider(&fixture)
            .inspect_pr_target(
                &verified_identity(),
                &target(),
                &CancellationToken::default()
            )
            .unwrap_err()
            .kind,
        GithubErrorKind::IdentityChanged
    ));
    assert_eq!(journal(&fixture).len(), 11);
    assert!(journal(&fixture).iter().all(|row| row["matched"] == true));
}

#[test]
fn conversation_comments_details_reject_changed_node_and_redirected_repository_with_matching_shas()
{
    let fixture = Fixture::new();
    let mut steps = empty_pr_detail_steps(pr(7, "example/repo"));
    let mut changed = pr(7, "example/repo");
    changed["node_id"] = json!("DIFFERENT_PR_SAME_SHAS");
    *steps.last_mut().unwrap() = step(api_args("repos/example/repo/pulls/7"), changed);
    fixture.scripted_gh(&steps);
    assert!(matches!(
        provider(&fixture)
            .pr_details("example/repo", 7, &CancellationToken::default())
            .unwrap_err()
            .kind,
        GithubErrorKind::StaleHead
    ));
    assert_eq!(journal(&fixture).len(), 7);
    for field in ["base", "repository_url"] {
        let fixture = Fixture::new();
        let mut raw = pr(7, "example/repo");
        if field == "base" {
            raw["base"]["repo"] = json!({"full_name":"renamed/repository"});
        } else {
            raw["repository_url"] = json!("https://api.github.invalid/repos/renamed/repository");
        }
        fixture.scripted_gh(&[step(api_args("repos/example/repo/pulls/7"), raw)]);
        assert!(matches!(
            provider(&fixture)
                .pr_summary("example/repo", 7, &CancellationToken::default())
                .unwrap_err()
                .kind,
            GithubErrorKind::InvalidTarget
        ));
        assert_eq!(journal(&fixture).len(), 1);
    }
}

fn graphql_step(query: &str, variables: Value, out: Value) -> GhStep {
    write_step(
        "graphql",
        "POST",
        Some(json!({"query":query,"variables":variables})),
        out,
    )
}
fn merge_context_value(queue: Value, auto: Value) -> Value {
    json!({"data":{"node":{"id":target().pr.id,"headRefOid":target().head_sha,"baseRefOid":target().base_sha,"mergeStateStatus":"CLEAN","reviewDecision":"APPROVED","viewerCanMergeAsAdmin":false,"viewerCanEnableAutoMerge":true,"viewerCanDisableAutoMerge":true,"autoMergeRequest":auto,"mergeQueueEntry":queue}}})
}
fn merge_context_step(queue: Value, auto: Value) -> GhStep {
    graphql_step(
        PR_MERGE_CONTEXT_QUERY,
        json!({"id":target().pr.id}),
        merge_context_value(queue, auto),
    )
}

#[test]
fn gh_identity_missing_or_nonpositive_ids_remain_explicitly_unverified() {
    for id in [Value::Null, json!(0), json!(-1), json!("42")] {
        let fixture = Fixture::new();
        fixture.scripted_gh(&[
            GhStep::read(&["--version"], "gh version 2.101.0\n"),
            step(api_args("user"), json!({"login":"octocat","id":id})),
        ]);
        assert_eq!(
            provider(&fixture)
                .identity(&CancellationToken::default())
                .unwrap()
                .account_id,
            None
        );
    }
    let old: HostIdentity = serde_json::from_value(
        json!({"host":"github.invalid","login":"octocat","gh_version":"2.101"}),
    )
    .unwrap();
    assert_eq!(old.account_id, None);
}

#[test]
fn reassigned_login_account_id_blocks_pr_and_actions_writes_before_target_reads() {
    let mut expected = identity();
    expected.account_id = Some(42);
    for actions in [false, true] {
        let fixture = Fixture::new();
        fixture.scripted_gh(&[
            GhStep::read(&["--version"], "gh version 2.101.0\n"),
            step(api_args("user"), json!({"login":"octocat","id":43})),
        ]);
        let gh = provider(&fixture);
        let result = if actions {
            gh.mutate_run(
                &expected,
                "example/repo",
                &run_target(),
                RunMutation::RerunAll { debug: false },
                &CancellationToken::default(),
            )
            .map(|_| ())
        } else {
            gh.comment(
                &expected,
                "example/repo",
                7,
                &target().head_sha,
                "hello",
                &CancellationToken::default(),
            )
            .map(|_| ())
        };
        assert!(matches!(
            result.unwrap_err().kind,
            GithubErrorKind::IdentityChanged
        ));
        assert_eq!(journal(&fixture).len(), 2);
        assert!(journal(&fixture).iter().all(|row| row["matched"] == true));
    }
}

#[test]
fn merge_queue_enqueue_confirms_entry_id_without_confusing_synthetic_commit_with_pr_head() {
    let fixture = Fixture::new();
    let queue = json!({"id":"MQ1","state":"QUEUED","position":1,"headCommit":{"oid":"cccccccccccccccccccccccccccccccccccccccc"},"baseCommit":{"oid":target().base_sha},"enqueuer":{"login":"octocat"},"jump":false});
    let mut steps = vec![merge_context_step(Value::Null, Value::Null)];
    steps.extend(guard_steps());
    steps.extend([
        graphql_step("mutation($input:EnqueuePullRequestInput!){enqueuePullRequest(input:$input){mergeQueueEntry{id}}}",json!({"input":{"pullRequestId":target().pr.id,"expectedHeadOid":target().head_sha,"jump":false}}),json!({"data":{"enqueuePullRequest":{"mergeQueueEntry":{"id":"MQ1"}}}})),
        merge_context_step(queue,Value::Null),
    ]);
    fixture.scripted_gh(&steps);
    let context = provider(&fixture)
        .enqueue_pr(&identity(), &target(), false, &CancellationToken::default())
        .unwrap();
    let entry = context.queue.unwrap();
    assert_eq!(entry.id, "MQ1");
    assert_ne!(entry.head_sha.as_deref(), Some(target().head_sha.as_str()));
    assert_eq!(journal(&fixture).len(), 6);
}

#[test]
fn merge_queue_dequeue_uses_pr_node_id_and_confirms_absence() {
    let fixture = Fixture::new();
    let queue = json!({"id":"MQ1","state":"QUEUED","position":1,"jump":false});
    let mut steps = vec![
        merge_context_step(queue.clone(), Value::Null),
        merge_context_step(queue, Value::Null),
    ];
    steps.extend(guard_steps());
    steps.extend([
        graphql_step("mutation($input:DequeuePullRequestInput!){dequeuePullRequest(input:$input){mergeQueueEntry{id}}}",json!({"input":{"id":target().pr.id}}),json!({"data":{"dequeuePullRequest":{"mergeQueueEntry":{"id":"MQ1"}}}})),
        merge_context_step(Value::Null,Value::Null),
    ]);
    fixture.scripted_gh(&steps);
    let gh = provider(&fixture);
    let observed = gh
        .merge_context(&target(), &CancellationToken::default())
        .unwrap()
        .queue
        .unwrap();
    assert!(gh
        .dequeue_pr(
            &identity(),
            &target(),
            &observed,
            &CancellationToken::default()
        )
        .unwrap()
        .queue
        .is_none());
    assert_eq!(journal(&fixture).len(), 7);
}

#[test]
fn auto_merge_disable_checks_explicit_viewer_permission_and_observed_state() {
    let auto = json!({"mergeMethod":"SQUASH","enabledBy":{"login":"octocat"}});
    let fixture = Fixture::new();
    let mut denied = merge_context_value(Value::Null, auto.clone());
    denied["data"]["node"]["viewerCanDisableAutoMerge"] = json!(false);
    fixture.scripted_gh(&[graphql_step(
        PR_MERGE_CONTEXT_QUERY,
        json!({"id":target().pr.id}),
        denied,
    )]);
    assert!(matches!(
        provider(&fixture)
            .disable_auto_merge(&identity(), &target(), &CancellationToken::default())
            .unwrap_err()
            .kind,
        GithubErrorKind::Permission
    ));
    assert_eq!(journal(&fixture).len(), 1);
    let fixture = Fixture::new();
    let mut steps = vec![merge_context_step(Value::Null, auto)];
    steps.extend(guard_steps());
    steps.extend([
        graphql_step("mutation($input:DisablePullRequestAutoMergeInput!){disablePullRequestAutoMerge(input:$input){pullRequest{id}}}",json!({"input":{"pullRequestId":target().pr.id}}),json!({"data":{"disablePullRequestAutoMerge":{"pullRequest":{"id":target().pr.id}}}})),
        merge_context_step(Value::Null,Value::Null),
    ]);
    fixture.scripted_gh(&steps);
    assert!(provider(&fixture)
        .disable_auto_merge(&identity(), &target(), &CancellationToken::default())
        .unwrap()
        .auto_merge_request
        .is_none());
    assert_eq!(journal(&fixture).len(), 6);
}

#[test]
fn actions_reconciliation_observes_effects_but_does_not_unlock_until_explicit_acknowledgement() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        step(api_args("user"), json!({"login":"octocat"})),
        page_step(
            "repos/example/repo/actions/runs?per_page=100",
            json!([{"workflow_runs":[run_value()]}]),
        ),
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        step(api_args("user"), json!({"login":"octocat"})),
        GhStep::read(&["after-ack"], "accepted"),
    ]);
    let runner = CommandRunner::default();
    let resource = "github.invalid/example/repo/run/123";
    runner.mark_ambiguous(resource);
    let gh = provider(&fixture).with_runner(runner.clone());
    let observed = gh
        .inspect_actions_target(
            &identity(),
            &ActionsTarget::Run {
                repository: "example/repo".into(),
                id: 123,
            },
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(observed.state()["id"], 123);
    let command = runner_spec(&fixture, &["after-ack"]).write(resource);
    assert!(matches!(
        runner
            .run(command.clone(), &CancellationToken::default())
            .unwrap_err()
            .kind,
        FailureKind::ReconciliationRequired
    ));
    assert_eq!(journal(&fixture).len(), 5);
    assert!(matches!(
        provider(&fixture)
            .acknowledge_actions_observation(observed.clone())
            .unwrap_err()
            .kind,
        GithubErrorKind::IdentityChanged
    ));
    assert!(matches!(
        runner
            .run(command.clone(), &CancellationToken::default())
            .unwrap_err()
            .kind,
        FailureKind::ReconciliationRequired
    ));
    assert_eq!(journal(&fixture).len(), 5);
    gh.acknowledge_actions_observation(observed).unwrap();
    assert_eq!(
        runner
            .run(command, &CancellationToken::default())
            .unwrap()
            .stdout,
        b"accepted"
    );
    assert_eq!(journal(&fixture).len(), 6);
}

#[test]
fn commit_checks_keep_latest_case_insensitive_status_and_pending_semantics() {
    let fixture = Fixture::new();
    let sha = target().head_sha;
    fixture.scripted_gh(&[
        page_step(&format!("repos/example/repo/commits/{sha}/check-runs?per_page=100"),json!([{"check_runs":[{"id":5,"name":"unit","head_sha":sha,"status":"completed","conclusion":"success"}]}])),
        page_step(&format!("repos/example/repo/commits/{sha}/statuses?per_page=100"),json!([[{"id":10,"context":"Deploy","state":"pending"},{"id":9,"context":"build","state":"success"},{"id":8,"context":"BUILD","state":"failure"}]])),
    ]);
    let checks = provider(&fixture)
        .checks("example/repo", &sha, &CancellationToken::default())
        .unwrap();
    assert_eq!(checks.len(), 3);
    assert_eq!(
        checks.iter().find(|c| c.name == "Deploy").unwrap().status,
        "pending"
    );
    assert_eq!(
        checks
            .iter()
            .find(|c| c.name == "Deploy")
            .unwrap()
            .conclusion,
        ""
    );
    assert_eq!(
        checks
            .iter()
            .find(|c| c.name == "build")
            .unwrap()
            .conclusion,
        "success"
    );
    assert!(!checks.iter().any(|c| c.conclusion == "failure"));
    let fixture = Fixture::new();
    fixture.scripted_gh(&[page_step(&format!("repos/example/repo/commits/{sha}/check-runs?per_page=100"),json!([{"check_runs":[{"id":5,"name":"unit","head_sha":target().base_sha,"status":"completed"}]}]))]);
    assert!(matches!(
        provider(&fixture)
            .checks("example/repo", &sha, &CancellationToken::default())
            .unwrap_err()
            .kind,
        GithubErrorKind::Json
    ));
    assert_eq!(journal(&fixture).len(), 1);
}

#[test]
fn actions_old_cloned_observation_cannot_acknowledge_newer_uncertain_write() {
    let fixture = Fixture::new();
    let mut newer = GhStep::read(&["newer-write"], "");
    newer.status = 1;
    newer.stderr = b"HTTP 503 after sending the request".to_vec();
    fixture.scripted_gh(&[
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        step(api_args("user"), json!({"login":"octocat"})),
        page_step(
            "repos/example/repo/actions/runs?per_page=100",
            json!([{"workflow_runs":[run_value()]}]),
        ),
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        step(api_args("user"), json!({"login":"octocat"})),
        newer,
    ]);
    let runner = CommandRunner::default();
    let resource = "github.invalid/example/repo/run/123";
    runner.mark_ambiguous(resource);
    let gh = provider(&fixture).with_runner(runner.clone());
    let observed = gh
        .inspect_actions_target(
            &identity(),
            &ActionsTarget::Run {
                repository: "example/repo".into(),
                id: 123,
            },
            &CancellationToken::default(),
        )
        .unwrap();
    let old = observed.clone();
    gh.acknowledge_actions_observation(observed).unwrap();
    let failure = runner
        .run(
            runner_spec(&fixture, &["newer-write"]).write(resource),
            &CancellationToken::default(),
        )
        .unwrap_err();
    assert!(failure.outcome_unknown);
    assert!(matches!(
        gh.acknowledge_actions_observation(old).unwrap_err().kind,
        GithubErrorKind::StaleHead
    ));
    assert!(matches!(
        runner
            .run(
                runner_spec(&fixture, &["must-not-retry"]).write(resource),
                &CancellationToken::default()
            )
            .unwrap_err()
            .kind,
        FailureKind::ReconciliationRequired
    ));
    assert_eq!(journal(&fixture).len(), 6);
    let unrelated = "github.invalid/example/repo/cache/9";
    runner.mark_ambiguous(unrelated);
    let versions = vec![
        (resource.into(), runner.resource_generation(resource)),
        (unrelated.into(), runner.resource_generation(unrelated)),
    ];
    runner.mark_ambiguous(unrelated);
    assert!(!runner.mark_reconciled_generations(&versions));
    assert!(matches!(
        runner
            .run(
                runner_spec(&fixture, &["must-not-retry"]).write(resource),
                &CancellationToken::default()
            )
            .unwrap_err()
            .kind,
        FailureKind::ReconciliationRequired
    ));
}

#[test]
fn actions_catalog_rejects_wrong_filter_membership_and_missing_coverage() {
    for (filter, endpoint) in [
        (
            RunFilter {
                workflow_id: Some(99),
                ..RunFilter::default()
            },
            "repos/example/repo/actions/workflows/99/runs?per_page=100",
        ),
        (
            RunFilter {
                branch: Some("other".into()),
                ..RunFilter::default()
            },
            "repos/example/repo/actions/runs?per_page=100&branch=other",
        ),
        (
            RunFilter {
                event: Some("push".into()),
                ..RunFilter::default()
            },
            "repos/example/repo/actions/runs?per_page=100&event=push",
        ),
        (
            RunFilter {
                actor: Some("other".into()),
                ..RunFilter::default()
            },
            "repos/example/repo/actions/runs?per_page=100&actor=other",
        ),
    ] {
        let fixture = Fixture::new();
        fixture.scripted_gh(&[page_step(
            endpoint,
            json!([{"total_count":1,"workflow_runs":[run_value()]}]),
        )]);
        assert!(matches!(
            provider(&fixture)
                .run_catalog("example/repo", &filter, &CancellationToken::default())
                .unwrap_err()
                .kind,
            GithubErrorKind::StaleHead
        ));
        assert_eq!(journal(&fixture).len(), 1);
    }
    let fixture = Fixture::new();
    fixture.scripted_gh(&[page_step(
        "repos/example/repo/actions/runs?per_page=100",
        json!([{"workflow_runs":[]}]),
    )]);
    assert!(matches!(
        provider(&fixture)
            .run_catalog(
                "example/repo",
                &RunFilter::default(),
                &CancellationToken::default()
            )
            .unwrap_err()
            .kind,
        GithubErrorKind::Json
    ));
}
const THREADS_QUERY:&str="query($id:ID!,$cursor:String){node(id:$id){... on PullRequest{reviewThreads(first:100,after:$cursor){nodes{id path diffSide line startLine isResolved isOutdated viewerCanResolve viewerCanUnresolve comments(first:100){nodes{id databaseId body author{login avatarUrl} commit{oid}} pageInfo{hasNextPage}}} pageInfo{hasNextPage endCursor}}}}}";
fn thread_page(resolved: bool) -> Value {
    json!({"data":{"node":{"reviewThreads":{"nodes":[{"id":"THREAD9","path":"file.rs","diffSide":"LEFT","line":null,"startLine":1,"isResolved":resolved,"isOutdated":true,"viewerCanResolve":true,"viewerCanUnresolve":true,"comments":{"nodes":[{"id":"COMMENT9","body":"older line","author":{"login":"alice"},"commit":{"oid":"cccccccccccccccccccccccccccccccccccccccc"}}],"pageInfo":{"hasNextPage":true}}}],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}})
}

#[test]
fn review_threads_resolve_exact_node_keep_outdated_provenance_and_partial_comments() {
    let fixture = Fixture::new();
    let vars = json!({"id":target().pr.id,"cursor":null});
    let mut steps = vec![graphql_step(
        THREADS_QUERY,
        vars.clone(),
        thread_page(false),
    )];
    steps.extend(guard_steps());
    steps.extend([graphql_step("mutation($input:ResolveReviewThreadInput!){resolveReviewThread(input:$input){thread{id isResolved}}}",json!({"input":{"threadId":"THREAD9"}}),json!({"data":{"resolveReviewThread":{"thread":{"id":"THREAD9","isResolved":true}}}})),graphql_step(THREADS_QUERY,vars,thread_page(true))]);
    fixture.scripted_gh(&steps);
    let thread = provider(&fixture)
        .resolve_thread(
            &identity(),
            &target(),
            "THREAD9",
            true,
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(thread.resolved);
    assert!(thread.outdated);
    assert_eq!(thread.line, None);
    assert!(!thread.comments_complete);
    assert_eq!(
        thread.comments[0]["commit"]["oid"],
        "cccccccccccccccccccccccccccccccccccccccc"
    );
    assert_eq!(journal(&fixture).len(), 6);
}

#[test]
fn viewed_files_graphql_keeps_canonical_path_and_pages_progress() {
    let query="query($id:ID!,$cursor:String){node(id:$id){... on PullRequest{files(first:100,after:$cursor){nodes{path viewerViewedState} pageInfo{hasNextPage endCursor}}}}}";
    let fixture = Fixture::new();
    let mut steps = guard_steps();
    steps.extend([graphql_step("mutation($input:MarkFileAsViewedInput!){markFileAsViewed(input:$input){pullRequest{id}}}",json!({"input":{"pullRequestId":target().pr.id,"path":"src/α file.rs"}}),json!({"data":{"markFileAsViewed":{"pullRequest":{"id":target().pr.id}}}})),graphql_step(query,json!({"id":target().pr.id,"cursor":null}),json!({"data":{"node":{"files":{"nodes":[{"path":"src/α file.rs","viewerViewedState":"VIEWED"}],"pageInfo":{"hasNextPage":true,"endCursor":"next"}}}}})),graphql_step(query,json!({"id":target().pr.id,"cursor":"next"}),json!({"data":{"node":{"files":{"nodes":[{"path":"other.rs","viewerViewedState":"UNVIEWED"}],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}}))]);
    fixture.scripted_gh(&steps);
    let files = provider(&fixture)
        .mark_viewed(
            &identity(),
            &target(),
            "src/α file.rs",
            true,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(files[0].state, "VIEWED");
    assert_eq!(files[0].path, "src/α file.rs");
    assert_eq!(files[1].state, "UNVIEWED");
    assert_eq!(journal(&fixture).len(), 6);
}

#[test]
fn conversation_comments_reactions_and_subscription_have_exact_actor_target() {
    let fixture = Fixture::new();
    let mut steps = guard_steps();
    steps.extend([
        step(
            api_args("repos/example/repo/issues/comments/9"),
            comment_value("old", "t0"),
        ),
        write_step(
            "repos/example/repo/issues/comments/9/reactions",
            "POST",
            Some(json!({"content":"heart"})),
            json!({"id":11,"content":"heart","user":{"login":"octocat"}}),
        ),
        page_step(
            "repos/example/repo/issues/comments/9/reactions?per_page=100",
            json!([[{"id":11,"content":"heart","user":{"login":"octocat"}}]]),
        ),
    ]);
    fixture.scripted_gh(&steps);
    let reactions = provider(&fixture)
        .react_comment(
            &identity(),
            &target(),
            9,
            CommentKind::Conversation,
            Reaction::Heart,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(reactions[0]["user"]["login"], "octocat");
    assert_eq!(reactions[0]["content"], "heart");
    let fixture = Fixture::new();
    let mut steps = guard_steps();
    steps.push(graphql_step("mutation($input:UpdateSubscriptionInput!){updateSubscription(input:$input){subscribable{id viewerSubscription}}}",json!({"input":{"subscribableId":target().pr.id,"state":"UNSUBSCRIBED"}}),json!({"data":{"updateSubscription":{"subscribable":{"id":target().pr.id,"viewerSubscription":"UNSUBSCRIBED"}}}})));
    fixture.scripted_gh(&steps);
    let value = provider(&fixture)
        .subscription(
            &identity(),
            &target(),
            Subscription::Unsubscribed,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(
        value["data"]["updateSubscription"]["subscribable"]["viewerSubscription"],
        "UNSUBSCRIBED"
    );
    assert_eq!(journal(&fixture).len(), 4);
}

#[test]
fn pending_reviews_discard_and_dismiss_inspect_current_review_state() {
    let pending = json!({"id":81,"state":"PENDING","body":"draft","commit_id":target().head_sha,"user":{"login":"octocat"}});
    let fixture = Fixture::new();
    let mut steps = guard_steps();
    steps.extend([
        step(api_args("repos/example/repo/pulls/7/reviews/81"), pending),
        write_step(
            "repos/example/repo/pulls/7/reviews/81",
            "DELETE",
            None,
            Value::Null,
        ),
        page_step(
            "repos/example/repo/pulls/7/reviews?per_page=100",
            json!([[]]),
        ),
    ]);
    fixture.scripted_gh(&steps);
    provider(&fixture)
        .discard_pending_review(&identity(), &target(), 81, &CancellationToken::default())
        .unwrap();
    assert_eq!(journal(&fixture).len(), 6);
    let approval = json!({"id":81,"state":"APPROVED","body":"approved","commit_id":target().head_sha,"user":{"login":"alice"}});
    let mut dismissed = approval.clone();
    dismissed["state"] = json!("DISMISSED");
    let fixture = Fixture::new();
    let mut steps = guard_steps();
    steps.extend([
        step(api_args("repos/example/repo/pulls/7/reviews/81"), approval),
        write_step(
            "repos/example/repo/pulls/7/reviews/81/dismissals",
            "PUT",
            Some(json!({"message":"Review no longer applies","event":"DISMISS"})),
            dismissed.clone(),
        ),
        step(api_args("repos/example/repo/pulls/7/reviews/81"), dismissed),
    ]);
    fixture.scripted_gh(&steps);
    let review = provider(&fixture)
        .dismiss_review(
            &identity(),
            &target(),
            81,
            "Review no longer applies",
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(review.state, "DISMISSED");
    assert_eq!(journal(&fixture).len(), 6);
}

#[test]
fn pr_lifecycle_lock_and_update_branch_use_reason_and_atomic_expected_head() {
    let fixture = Fixture::new();
    let mut steps = guard_steps();
    steps.extend([
        write_step(
            "repos/example/repo/issues/7/lock",
            "PUT",
            Some(json!({"lock_reason":"spam"})),
            Value::Null,
        ),
        step(
            api_args("repos/example/repo/issues/7"),
            json!({"locked":true,"active_lock_reason":"spam"}),
        ),
    ]);
    fixture.scripted_gh(&steps);
    let issue = provider(&fixture)
        .lock_pr(
            &identity(),
            &target(),
            Some(LockReason::Spam),
            true,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(issue["locked"], true);
    let fixture = Fixture::new();
    let mut changed = pr(7, "example/repo");
    changed["head"]["sha"] = json!("cccccccccccccccccccccccccccccccccccccccc");
    let mut steps = guard_steps();
    steps.extend([
        write_step(
            "repos/example/repo/pulls/7/update-branch",
            "PUT",
            Some(json!({"expected_head_sha":target().head_sha})),
            json!({"message":"Updating branch"}),
        ),
        step(api_args("repos/example/repo/pulls/7"), changed),
    ]);
    fixture.scripted_gh(&steps);
    let updated = provider(&fixture)
        .update_pr_branch(&identity(), &target(), &CancellationToken::default())
        .unwrap();
    assert_ne!(updated.head_sha, target().head_sha);
    assert_eq!(journal(&fixture).len(), 5);
}

#[test]
fn review_threads_graphql_errors_with_exit_zero_are_not_success_or_retryable() {
    let fixture = Fixture::new();
    let mut steps = guard_steps();
    steps.push(graphql_step("mutation($input:UpdateSubscriptionInput!){updateSubscription(input:$input){subscribable{id viewerSubscription}}}",json!({"input":{"subscribableId":target().pr.id,"state":"SUBSCRIBED"}}),json!({"errors":[{"message":"Missing permission"}],"data":null})));
    steps.extend(guard_steps());
    fixture.scripted_gh(&steps);
    let gh = provider(&fixture);
    let first = gh
        .subscription(
            &identity(),
            &target(),
            Subscription::Subscribed,
            &CancellationToken::default(),
        )
        .unwrap_err();
    assert!(matches!(first.kind, GithubErrorKind::AmbiguousWrite));
    let again = gh
        .subscription(
            &identity(),
            &target(),
            Subscription::Subscribed,
            &CancellationToken::default(),
        )
        .unwrap_err();
    assert!(matches!(again.kind, GithubErrorKind::AmbiguousWrite));
    assert_eq!(journal(&fixture).len(), 7);
}

fn creation_request() -> CreatePr {
    CreatePr {
        repository: "example/repo".into(),
        head_repository: "octocat/fork".into(),
        head: "feature/story".into(),
        base: "main".into(),
        title: "Proposal".into(),
        body: "body\n\"quoted\"".into(),
        draft: true,
        expected_head_sha: target().head_sha,
        expected_base_sha: target().base_sha,
    }
}
fn create_prefix() -> Vec<GhStep> {
    vec![GhStep::read(&["--version"],"gh version 2.101.0\n"),step(api_args("user"),json!({"login":"octocat"})),step(api_args("repos/octocat/fork/git/ref/heads/feature%2Fstory"),json!({"object":{"sha":target().head_sha}})),step(api_args("repos/example/repo/git/ref/heads/main"),json!({"object":{"sha":target().base_sha}})),page_step("repos/example/repo/pulls?state=open&head=octocat%3Afeature%2Fstory&base=main&per_page=100",json!([[]]))]
}

#[test]
fn create_pr_verifies_remote_fork_refs_prevents_duplicates_and_preserves_checkout() {
    let fixture = Fixture::new();
    let request = creation_request();
    let mut created = pr(8, "example/repo");
    created["title"] = json!(request.title);
    created["body"] = json!(request.body);
    created["draft"] = json!(true);
    let mut steps = create_prefix();
    steps.extend([write_step("repos/example/repo/pulls","POST",Some(json!({"head":"octocat:feature/story","head_repo":"fork","base":"main","title":request.title,"body":request.body,"draft":true})),created.clone()),step(api_args("repos/example/repo/pulls/8"),created)]);
    fixture.scripted_gh(&steps);
    let result = provider(&fixture)
        .create_pr(&identity(), &request, &CancellationToken::default())
        .unwrap();
    assert_eq!(result.pr.number, 8);
    assert!(result.pr.draft);
    assert_eq!(result.pr.title, "Proposal");
    assert!(fs::read_dir(fixture.repo()).unwrap().next().is_none());
    assert_eq!(journal(&fixture).len(), 7);
    let fixture = Fixture::new();
    let mut steps = create_prefix();
    steps[4] = page_step(
        "repos/example/repo/pulls?state=open&head=octocat%3Afeature%2Fstory&base=main&per_page=100",
        json!([[pr(8, "example/repo")]]),
    );
    fixture.scripted_gh(&steps);
    let error = provider(&fixture)
        .create_pr(
            &identity(),
            &creation_request(),
            &CancellationToken::default(),
        )
        .unwrap_err();
    assert!(matches!(error.kind, GithubErrorKind::InvalidTarget));
    assert!(error.message.contains("already exists"));
    assert_eq!(journal(&fixture).len(), 5);
}

#[test]
fn pending_reviews_add_multiline_thread_to_pending_review_without_publishing() {
    let fixture = Fixture::new();
    let pending = json!({"id":81,"node_id":"PRR81","state":"PENDING","body":"draft","commit_id":target().head_sha,"user":{"login":"octocat"}});
    let mut steps = guard_steps();
    steps.extend([
        step(api_args("repos/example/repo/pulls/7/reviews/81"), pending),
        page_step(
            "repos/example/repo/pulls/7/files?per_page=100",
            json!([[{"filename":"file.rs","patch":"@@ -1 +1,2 @@\n before\n+after"}]]),
        ),
    ]);
    steps.extend(guard_steps());
    steps.push(graphql_step("mutation($input:AddPullRequestReviewThreadInput!){addPullRequestReviewThread(input:$input){thread{id isResolved comments(first:1){nodes{id body}}}}}",json!({"input":{"pullRequestReviewId":"PRR81","path":"file.rs","side":"RIGHT","line":2,"startLine":1,"startSide":"RIGHT","body":"pending range"}}),json!({"data":{"addPullRequestReviewThread":{"thread":{"id":"THREAD_NEW","isResolved":false,"comments":{"nodes":[{"id":"COMMENT_NEW","body":"pending range"}]}}}}})));
    fixture.scripted_gh(&steps);
    let comment = InlineComment {
        path: "file.rs".into(),
        side: DiffSide::Right,
        line: 2,
        start_line: Some(1),
        start_side: Some(DiffSide::Right),
        body: "pending range".into(),
    };
    let value = provider(&fixture)
        .add_pending_comment(
            &identity(),
            &target(),
            81,
            &comment,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(
        value["data"]["addPullRequestReviewThread"]["thread"]["id"],
        "THREAD_NEW"
    );
    let records = journal(&fixture);
    assert_eq!(records.len(), 9);
    assert!(!records.iter().any(|r| r["args"][1]
        .as_str()
        .is_some_and(|s| s.ends_with("/events"))));
}

#[test]
fn pr_metadata_reviewers_and_teams_read_back_distinct_requested_resources() {
    let fixture = Fixture::new();
    let selection = ReviewerSelection {
        reviewers: vec!["alice".into()],
        team_reviewers: vec!["maintainers".into()],
    };
    let response = json!({"users":[{"login":"alice"}],"teams":[{"slug":"maintainers"}]});
    let mut steps = guard_steps();
    steps.extend([
        write_step(
            "repos/example/repo/pulls/7/requested_reviewers",
            "POST",
            Some(json!({"reviewers":["alice"],"team_reviewers":["maintainers"]})),
            response.clone(),
        ),
        step(
            api_args("repos/example/repo/pulls/7/requested_reviewers"),
            response,
        ),
    ]);
    fixture.scripted_gh(&steps);
    let result = provider(&fixture)
        .request_reviewers(
            &identity(),
            &target(),
            &selection,
            false,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(result["teams"][0]["slug"], "maintainers");
    assert_eq!(result["users"][0]["login"], "alice");
    assert_eq!(journal(&fixture).len(), 5);
}

#[test]
fn pr_lifecycle_revert_creates_a_new_canonical_pr_with_original_provenance() {
    let fixture = Fixture::new();
    let mut original = target();
    original.merged = true;
    original.pr.state = "merged".into();
    let mut merged = pr(7, "example/repo");
    merged["merged"] = json!(true);
    let mut steps = guard_steps();
    steps[2] = step(api_args("repos/example/repo/pulls/7"), merged);
    steps.extend([graphql_step("mutation($input:RevertPullRequestInput!){revertPullRequest(input:$input){revertPullRequest{id number url}}}",json!({"input":{"pullRequestId":original.pr.id,"title":"Revert PR 7","body":"Reverts example/repo#7","draft":true}}),json!({"data":{"revertPullRequest":{"revertPullRequest":{"id":"PR8","number":8,"url":"https://github.invalid/example/repo/pull/8"}}}})),step(api_args("repos/example/repo/pulls/8"),pr(8,"example/repo"))]);
    fixture.scripted_gh(&steps);
    let reverted = provider(&fixture)
        .revert_pr(
            &identity(),
            &original,
            "Revert PR 7",
            "Reverts example/repo#7",
            true,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(reverted.pr.number, 8);
    assert_eq!(journal(&fixture).len(), 5);
}

#[test]
fn pending_reviews_outdated_draft_can_be_discarded_but_cannot_be_submitted() {
    let pending = json!({"id":81,"state":"PENDING","body":"draft","commit_id":"cccccccccccccccccccccccccccccccccccccccc","user":{"login":"octocat"}});
    let fixture = Fixture::new();
    let mut steps = guard_steps();
    steps.push(step(
        api_args("repos/example/repo/pulls/7/reviews/81"),
        pending.clone(),
    ));
    fixture.scripted_gh(&steps);
    let error = provider(&fixture)
        .submit_pending_review(
            &identity(),
            &target(),
            81,
            ReviewEvent::Approve,
            "approved",
            &CancellationToken::default(),
        )
        .unwrap_err();
    assert!(matches!(error.kind, GithubErrorKind::StaleHead));
    assert_eq!(journal(&fixture).len(), 4);
    let fixture = Fixture::new();
    let mut steps = guard_steps();
    steps.extend([
        step(api_args("repos/example/repo/pulls/7/reviews/81"), pending),
        write_step(
            "repos/example/repo/pulls/7/reviews/81",
            "DELETE",
            None,
            Value::Null,
        ),
        page_step(
            "repos/example/repo/pulls/7/reviews?per_page=100",
            json!([[]]),
        ),
    ]);
    fixture.scripted_gh(&steps);
    provider(&fixture)
        .discard_pending_review(&identity(), &target(), 81, &CancellationToken::default())
        .unwrap();
    assert_eq!(journal(&fixture).len(), 6);
}

#[path = "provider_contracts/remote_repository_v1.rs"]
mod remote_repository_v1;
