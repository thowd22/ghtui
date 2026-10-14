//! SOURCE-ONLY projection tests; activate with provider health at coordinated lease.
use super::*;
use crate::{
    metrics::{
        DashboardEvidence, EventCollection, Health, PrCohortKind, PrEvidence, PrState, UtcWindow,
    },
    providers::gh::{
        HostIdentity, MetricsActor, MetricsCheckResult, MetricsCommitStatusState,
        MetricsHealthConclusion, MetricsHealthStatus, MetricsPrState,
    },
};

fn identity(id: u64) -> HostIdentity {
    HostIdentity {
        host: "github.invalid".into(),
        login: "viewer".into(),
        account_id: Some(id),
        gh_version: "2.99.0".into(),
    }
}
fn scope() -> MetricsScope {
    MetricsScope::new(&identity(42), &["org/app".into(), "org/other".into()]).unwrap()
}
fn repo() -> MetricsRepository {
    MetricsRepository {
        host: "github.invalid".into(),
        id: 50,
        node_id: "R_Genuine".into(),
        full_name: "org/app".into(),
    }
}
fn metrics_actor(id: Option<u64>, login: &str) -> MetricsActor {
    MetricsActor {
        account_id: id,
        actor: Actor {
            login: login.into(),
            display_name: Some("Actual actor".into()),
            avatar_url: Some("https://avatars.invalid/actual".into()),
            actor_type: "User".into(),
        },
    }
}
fn parent() -> MetricsPrObservation {
    MetricsPrObservation {
        repository: repo(),
        id: 700,
        node_id: "PR_Genuine".into(),
        number: 7,
        title: "Original".into(),
        author: metrics_actor(Some(10), "author"),
        state: MetricsPrState::Open,
        draft: None,
        created_at: None,
        merged_at: None,
        closed_at: None,
        merged: None,
        head_sha: Some("a".repeat(40)),
        requested_users: None,
        requested_teams: None,
        observed_updated_at: None,
        reasons: vec!["Nullable original metadata".into()],
    }
}
fn eligible() -> EvidenceCoverage {
    EvidenceCoverage {
        state: CoverageState::Partial(vec!["Inventory metadata partial".into()]),
        complete_empty: false,
        fetched_at: Some(99),
    }
}
fn coverage(loaded: usize) -> MetricsReadCoverage {
    MetricsReadCoverage {
        enumeration_complete: true,
        metric_evidence_complete: true,
        complete_empty: loaded == 0,
        requested_pages: 1,
        fetched_pages: 1,
        fetched_rows: loaded,
        admitted_rows: loaded,
        excluded_rows: 0,
        response_bytes: 500,
        reasons: vec![],
        status: MetricsReadStatus::Complete,
    }
}
fn check(id: u64, kind: &str) -> MetricsCheckObservation {
    MetricsCheckObservation {
        id,
        node_id: Some(format!("{kind}_{id}")),
        name: Some("Actual source name".into()),
        head_sha: "a".repeat(40),
        result: if kind == "check" {
            MetricsCheckResult::CheckRun {
                status: MetricsHealthStatus::Completed,
                conclusion: Some(MetricsHealthConclusion::Failure),
            }
        } else {
            MetricsCheckResult::CommitStatus {
                state: MetricsCommitStatusState::Success,
            }
        },
        app_id: if kind == "check" { Some(90) } else { None },
        actor: Some(metrics_actor(Some(11), "ci-actor")),
        started_at: None,
        completed_at: None,
        created_at: None,
        url: None,
        current_context_unique: true,
        reasons: vec![],
    }
}
fn checks() -> ExactPrCheckHealth {
    ExactPrCheckHealth {
        identity: identity(42),
        parent: parent(),
        check_runs: vec![check(201, "check")],
        commit_statuses: vec![check(301, "status")],
        check_run_coverage: coverage(1),
        commit_status_coverage: coverage(1),
        required_rules: MetricsRequiredRules::Unavailable {
            reason: "Required rule source unavailable".into(),
        },
        source_limitations: vec![],
        observed_from: 100,
        observed_through: 101,
        requests: 8,
        raw_rows: 2,
        collection_pages: 2,
        aggregate_response_bytes: 1800,
        successful_diagnostic_bytes: 0,
        failed_reserved_bytes: 0,
    }
}
fn run(id: u64) -> MetricsRunObservation {
    MetricsRunObservation {
        repository: repo(),
        id,
        node_id: format!("RUN_{id}"),
        workflow_id: 80,
        attempt: 3,
        head_sha: "b".repeat(40),
        name: Some("Actual workflow".into()),
        title: Some("Actual run title".into()),
        event: Some("push".into()),
        head_branch: Some("main".into()),
        status: MetricsHealthStatus::InProgress,
        conclusion: None,
        actor: Some(metrics_actor(Some(12), "original-author")),
        triggering_actor: Some(metrics_actor(Some(13), "actual-rerunner")),
        created_at: None,
        started_at: None,
        observed_updated_at: None,
        url: None,
        reasons: vec!["Run created time unavailable".into()],
    }
}
fn runs() -> RepositoryRunHealth {
    let mut source = coverage(1);
    source.metric_evidence_complete = false;
    source.status = MetricsReadStatus::Partial;
    source.reasons = vec!["Created time unavailable".into()];
    RepositoryRunHealth {
        identity: identity(42),
        repository: repo(),
        runs: vec![run(401)],
        coverage: source,
        source_limitations: vec![],
        observed_from: 100,
        observed_through: 101,
        requests: 5,
        raw_rows: 1,
        collection_pages: 1,
        aggregate_response_bytes: 1300,
        successful_diagnostic_bytes: 0,
        failed_reserved_bytes: 0,
    }
}
fn project_checks(sources: &[ExactPrCheckHealth]) -> Result<CheckHealthProjection, String> {
    project_check_health(&scope(), &[parent()], &eligible(), sources)
}
fn project_runs(sources: &[RepositoryRunHealth]) -> Result<RunHealthProjection, String> {
    project_run_health(&scope(), &[repo()], &eligible(), sources)
}

#[test]
fn metrics_health_projection_check_targets_retain_genuine_parent_ids_head_actor_and_required_gaps()
{
    let projected = project_checks(&[checks()]).unwrap();
    assert_eq!(projected.eligible_parents, 1);
    assert_eq!(projected.loaded_parents.len(), 1);
    assert_eq!(projected.checks.len(), 2);
    let MetricTarget::Check {
        pr,
        head_sha,
        check_id,
        kind,
    } = &projected.checks[0].target
    else {
        panic!("Wrong drill type")
    };
    assert_eq!(pr.repository_id, Some(50));
    assert_eq!(pr.database_id, Some(700));
    assert_eq!(pr.node_id.as_deref(), Some("PR_Genuine"));
    assert!(pr.source_record_id.is_none());
    assert_eq!(head_sha, &"a".repeat(40));
    assert_eq!((*check_id, kind.as_str()), (201, "check"));
    assert_eq!(projected.checks[0].health, Health::Failure);
    assert_eq!(projected.checks[1].health, Health::Success);
    assert_eq!(
        projected.provenance[0]
            .observed
            .actor
            .as_ref()
            .unwrap()
            .account_id,
        Some(11)
    );
    assert_eq!(projected.provenance[0].observed.app_id, Some(90));
    let CoverageState::Partial(reasons) = &projected.coverage.state else {
        panic!("Missing source gaps")
    };
    assert!(reasons.iter().any(|reason| reason.contains("Required")));
    assert!(reasons.iter().any(|reason| reason.contains("1,000")));
    assert!(!projected.coverage.complete_empty);
}

#[test]
fn metrics_health_projection_success_diagnostics_have_separate_honest_raw_capture_budget() {
    let mut source = checks();
    source.successful_diagnostic_bytes = 17;
    let projected = project_checks(&[source]).unwrap();
    assert_eq!(projected.sources[0].successful_response_bytes, 1800);
    assert_eq!(projected.sources[0].successful_diagnostic_bytes, 17);
    assert_eq!(projected.sources[0].failed_reserved_bytes, 0);
    assert_eq!(projected.checks[0].health, Health::Failure);
    let mut source = runs();
    source.successful_diagnostic_bytes = MAX_METADATA;
    assert!(project_runs(&[source]).is_err());
}
#[test]
fn metrics_health_projection_missing_or_repeated_context_is_unknown_not_healthy_or_duplicated_alias(
) {
    let mut source = checks();
    source.commit_statuses[0].current_context_unique = false;
    source.commit_statuses[0].name = None;
    source.commit_statuses[0].actor = None;
    let projected = project_checks(&[source]).unwrap();
    assert_eq!(projected.checks[1].health, Health::Unknown);
    assert!(projected.checks[1].name.contains("name unavailable"));
    assert!(projected.provenance[1].observed.actor.is_none());
    assert_eq!(projected.checks[0].health, Health::Failure);
    assert!(matches!(
        projected.coverage.state,
        CoverageState::Partial(_)
    ));
}
#[test]
fn metrics_health_projection_unavailable_endpoint_retains_positive_facts_with_partial_not_unavailable(
) {
    let mut source = checks();
    source.commit_statuses.clear();
    source.commit_status_coverage = coverage(0);
    source.commit_status_coverage.enumeration_complete = false;
    source.commit_status_coverage.complete_empty = false;
    source.commit_status_coverage.metric_evidence_complete = false;
    source.commit_status_coverage.fetched_pages = 0;
    source.commit_status_coverage.response_bytes = 0;
    source.commit_status_coverage.status =
        MetricsReadStatus::Unavailable(crate::providers::gh::GithubError {
            kind: crate::providers::gh::GithubErrorKind::Permission,
            message: "Checks-only permission gap".into(),
        });
    source.raw_rows = 1;
    source.failed_reserved_bytes = 4 * 1024 * 1024;
    let projected = project_checks(&[source]).unwrap();
    assert_eq!(projected.checks.len(), 1);
    assert_eq!(projected.checks[0].health, Health::Failure);
    assert!(matches!(
        projected.coverage.state,
        CoverageState::Partial(_)
    ));
    assert!(!projected.sources[0].commit_statuses.complete_empty);
    assert_eq!(projected.sources[0].failed_reserved_bytes, 4 * 1024 * 1024);
}
#[test]
fn metrics_health_projection_observed_empty_detail_is_preserved_but_global_unknown_never_becomes_zero(
) {
    let mut source = checks();
    source.check_runs.clear();
    source.commit_statuses.clear();
    source.check_run_coverage = coverage(0);
    source.commit_status_coverage = coverage(0);
    source.raw_rows = 0;
    let projected = project_checks(&[source]).unwrap();
    assert!(projected.checks.is_empty());
    assert!(
        projected.sources[0].check_runs.complete_empty
            && projected.sources[0].commit_statuses.complete_empty
    );
    assert!(matches!(
        projected.coverage.state,
        CoverageState::Partial(_)
    ));
    assert!(!projected.coverage.complete_empty);
    let absent = project_checks(&[]).unwrap();
    assert!(matches!(
        absent.coverage.state,
        CoverageState::Unavailable(_)
    ));
    assert!(!absent.coverage.complete_empty);
    assert_eq!(absent.eligible_parents, 1);
    assert!(absent.loaded_parents.is_empty());
}
#[test]
fn metrics_health_projection_explicit_eligibility_missing_parents_and_repositories_are_not_full_denominators(
) {
    let mut second = parent();
    second.id = 701;
    second.node_id = "PR_Second".into();
    second.number = 8;
    let source = checks();
    let projected =
        project_check_health(&scope(), &[parent(), second], &eligible(), &[source]).unwrap();
    assert_eq!(projected.eligible_parents, 2);
    assert_eq!(projected.loaded_parents.len(), 1);
    let CoverageState::Partial(reasons) = projected.coverage.state else {
        panic!("Missing parent gap")
    };
    assert!(reasons.iter().any(|reason| reason.contains("#8")));
    let mut other = repo();
    other.id = 51;
    other.node_id = "R_Second".into();
    other.full_name = "org/other".into();
    let projected = project_run_health(&scope(), &[repo(), other], &eligible(), &[runs()]).unwrap();
    assert_eq!(projected.eligible_repositories, 2);
    assert_eq!(projected.loaded_repositories.len(), 1);
    let CoverageState::Partial(reasons) = projected.coverage.state else {
        panic!("Missing repo gap")
    };
    assert!(reasons.iter().any(|reason| reason.contains("org/other")));
}
#[test]
fn metrics_health_projection_current_run_attempt_uses_actual_trigger_actor_not_original_actor_fallback(
) {
    let projected = project_runs(&[runs()]).unwrap();
    assert_eq!(projected.runs[0].actor.account_id, Some(13));
    assert_eq!(projected.runs[0].actor.actor.login, "actual-rerunner");
    let MetricTarget::Run {
        repository_id,
        run_id,
        attempt,
        workflow_id,
        head_sha,
        ..
    } = &projected.runs[0].target
    else {
        panic!("Wrong run target")
    };
    assert_eq!(
        (*repository_id, *run_id, *attempt, *workflow_id),
        (Some(50), 401, 3, 80)
    );
    assert_eq!(head_sha, &"b".repeat(40));
    assert_eq!(projected.runs[0].observed_at, 101);
    assert!(projected.runs[0].created_at.is_none());
    assert_eq!(projected.runs[0].health, Health::Pending);
    let mut missing = runs();
    missing.runs[0].triggering_actor = None;
    let projected = project_runs(&[missing]).unwrap();
    assert!(
        projected.runs[0].actor.account_id.is_none()
            && projected.runs[0].actor.actor.login.is_empty()
    );
    assert_eq!(
        projected.provenance[0]
            .observed
            .actor
            .as_ref()
            .unwrap()
            .account_id,
        Some(12)
    );
}
#[test]
fn metrics_health_projection_same_login_other_uid_replaced_parent_or_full_head_refuses() {
    for variation in 0..6 {
        let mut source = checks();
        match variation {
            0 => source.identity = identity(43),
            1 => source.parent.repository.id = 51,
            2 => source.parent.id = 701,
            3 => source.parent.node_id = "PR_replaced".into(),
            4 => source.parent.head_sha = Some("c".repeat(40)),
            _ => source.check_runs[0].head_sha = "a".repeat(7),
        };
        assert!(project_checks(&[source]).is_err());
    }
    let mut source = runs();
    source.identity = identity(43);
    assert!(project_runs(&[source]).is_err());
    let mut source = runs();
    source.repository.node_id = "R_replaced".into();
    assert!(project_runs(&[source]).is_err());
}
#[test]
fn metrics_health_projection_forged_empty_counters_namespace_and_duplicate_identity_refuse() {
    for variation in 0..6 {
        let mut source = checks();
        match variation {
            0 => source.check_run_coverage.complete_empty = true,
            1 => source.check_run_coverage.admitted_rows = 0,
            2 => source.raw_rows = 0,
            3 => source.requests = 7,
            4 => {
                source.check_runs[0].result = MetricsCheckResult::CommitStatus {
                    state: MetricsCommitStatusState::Success,
                }
            }
            _ => source.commit_statuses[0].node_id = source.check_runs[0].node_id.clone(),
        };
        assert!(project_checks(&[source]).is_err());
    }
    assert!(project_checks(&[checks(), checks()]).is_err());
    let mut source = runs();
    source.runs.push(source.runs[0].clone());
    source.coverage = coverage(2);
    source.raw_rows = 2;
    assert!(project_runs(&[source]).is_err());
}
#[test]
fn metrics_health_projection_limits_utf8_reasons_and_failed_capture_budget_preflight_before_copy() {
    let mut source = checks();
    source.source_limitations = vec!["x".repeat(4097)];
    assert!(project_checks(&[source]).is_err());
    let mut source = checks();
    source.check_runs[0].actor.as_mut().unwrap().actor.login = "é".repeat(2049);
    assert!(project_checks(&[source]).is_err());
    let mut source = checks();
    source.failed_reserved_bytes = MAX_METADATA;
    assert!(project_checks(&[source]).is_err());
    let mut source = runs();
    source.runs[0].title = Some("x".repeat(4097));
    assert!(project_runs(&[source]).is_err());
    let mut source = runs();
    source.source_limitations = vec!["bounded".into(); 129];
    assert!(project_runs(&[source]).is_err());
    let mut source = runs();
    source.observed_through = 99;
    assert!(project_runs(&[source]).is_err());
}
#[test]
fn metrics_health_projection_real_dashboard_admits_partial_known_checks_but_missing_run_dates_are_unknown(
) {
    let projected = project_checks(&[checks()]).unwrap();
    let run = project_runs(&[runs()]).unwrap();
    let original = parent();
    let target = super::parent(&scope(), &original).unwrap();
    let evidence = DashboardEvidence {
        scope: scope(),
        cohort: PrCohortKind::SelectedRepositoryInventory,
        prs: vec![PrEvidence {
            target,
            cohort_member: true,
            title: original.title,
            author: actor(Some(&original.author)).unwrap(),
            state: PrState::Open,
            draft: None,
            head_sha: original.head_sha,
            created_at: None,
            review_requested_now: None,
            timeline: EventCollection::unavailable(),
            reviews: EventCollection::unavailable(),
        }],
        pr_coverage: eligible(),
        checks: projected.checks,
        check_coverage: projected.coverage,
        runs: run.runs,
        run_coverage: run.coverage,
    };
    let snapshot =
        crate::metrics::dashboard(&evidence, 101, UtcWindow::new(90, 101).unwrap()).unwrap();
    let checks = snapshot
        .cards
        .iter()
        .find(|card| card.kind == crate::metrics::MetricKind::Checks)
        .unwrap();
    assert_eq!(checks.contributors.len(), 2);
    assert!(matches!(
        checks.value,
        crate::metrics::MetricValue::HealthCounts {
            failure: 1,
            success: 1,
            ..
        }
    ));
    let runs = snapshot
        .cards
        .iter()
        .find(|card| card.kind == crate::metrics::MetricKind::Workflows)
        .unwrap();
    assert_eq!(runs.unknown, 1);
    assert_eq!(runs.contributors[0].actors[0].account_id, Some(13));
}
#[test]
fn metrics_health_projection_real_provider_clocks_readonly_journal_and_git_do_not_change_during_projection(
) {
    use crate::{
        providers::{gh::GithubProvider, runner::CancellationToken},
        test_support::{Fixture, GhStep},
    };
    use serde_json::json;
    use std::{fs, process::Command};
    let fixture = Fixture::new();
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .args(args)
            .current_dir(fixture.repo())
            .output()
            .unwrap();
        assert!(output.status.success());
        output.stdout
    };
    git(&["init", "--initial-branch=main"]);
    git(&["config", "user.email", "fixture@example.invalid"]);
    git(&["config", "user.name", "Fixture"]);
    fs::write(fixture.repo().join("original"), b"original\n").unwrap();
    git(&["add", "original"]);
    git(&["commit", "-m", "baseline"]);
    fs::write(fixture.repo().join("dirty"), b"untracked\n").unwrap();
    let before_head = git(&["rev-parse", "HEAD"]);
    let before_status = git(&["status", "--porcelain=v1", "-z"]);
    let raw_repo = json!({"id":50,"node_id":"R_Genuine","full_name":"org/app"});
    let raw_run = json!({"id":401,"node_id":"RUN_401","workflow_id":80,"run_attempt":3,"head_sha":"b".repeat(40),"name":"CI","display_title":"Actual run","event":"push","head_branch":"main","status":"completed","conclusion":"success","created_at":"2026-10-11T00:00:00Z","run_started_at":"2026-10-12T00:00:00Z","updated_at":"2026-10-13T00:00:00Z","actor":{"id":12,"login":"original"},"triggering_actor":{"id":13,"login":"actual-rerunner"},"repository":raw_repo});
    let get = |endpoint: &str, value: serde_json::Value| GhStep {
        args: vec![
            "api".into(),
            endpoint.into(),
            "--hostname".into(),
            "github.invalid".into(),
            "--method".into(),
            "GET".into(),
        ],
        stdin: vec![],
        stdout: serde_json::to_vec(&value).unwrap(),
        stderr: vec![],
        status: 0,
        delay_ms: 0,
    };
    fixture.scripted_gh(&[
        get("user", json!({"id":42,"login":"viewer"})),
        get("repos/org/app", raw_repo.clone()),
        get(
            "repos/org/app/actions/runs?per_page=100&page=1",
            json!({"total_count":1,"workflow_runs":[raw_run]}),
        ),
        get("repos/org/app", raw_repo),
        get("user", json!({"id":42,"login":"viewer"})),
    ]);
    let gh = GithubProvider::new("github.invalid")
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"));
    let source = gh
        .metrics_repository_run_health(
            &identity(42),
            &repo(),
            &Default::default(),
            &CancellationToken::default(),
        )
        .unwrap();
    let journal = fixture.root.join("gh-fixture/commands.jsonl");
    let before_commands = fs::read(&journal).unwrap();
    let projected = project_runs(&[source]).unwrap();
    assert_eq!(
        projected.runs[0].created_at,
        crate::metrics::utc_seconds("2026-10-11T00:00:00Z")
    );
    assert_eq!(
        projected.provenance[0]
            .observed
            .started_at
            .as_ref()
            .unwrap()
            .raw(),
        "2026-10-12T00:00:00Z"
    );
    assert_eq!(
        projected.provenance[0]
            .observed
            .observed_updated_at
            .as_ref()
            .unwrap()
            .raw(),
        "2026-10-13T00:00:00Z"
    );
    assert_eq!(projected.runs[0].actor.account_id, Some(13));
    assert_eq!(fs::read(&journal).unwrap(), before_commands);
    let rows = String::from_utf8(before_commands)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 5);
    assert!(rows
        .iter()
        .all(|row| row["matched"] == true && row["args"][5] == "GET" && row["stdin_hex"] == ""));
    assert_eq!(git(&["rev-parse", "HEAD"]), before_head);
    assert_eq!(git(&["status", "--porcelain=v1", "-z"]), before_status);
    assert_eq!(
        fs::read(fixture.repo().join("dirty")).unwrap(),
        b"untracked\n"
    );
}
