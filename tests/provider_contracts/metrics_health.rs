//! SOURCE-ONLY current health contracts. No native/full verification implied.
use super::*;
use ghtui::providers::gh::{
    MetricsPrObservation, MetricsReadLimits, MetricsReadStatus, MetricsRepository,
    MetricsRequiredRules,
};

fn current() -> HostIdentity {
    HostIdentity {
        account_id: Some(42),
        ..identity()
    }
}
fn repo() -> Value {
    json!({"id":99,"node_id":"R_99","full_name":"example/repo"})
}
fn canonical() -> MetricsRepository {
    MetricsRepository {
        host: "github.invalid".into(),
        id: 99,
        node_id: "R_99".into(),
        full_name: "example/repo".into(),
    }
}
fn parent() -> Value {
    json!({"id":101,"node_id":"PR_1","number":1,"title":"Original","state":"open","draft":false,"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-12T00:00:00Z","merged_at":null,"closed_at":null,"merged":false,"head":{"sha":"a".repeat(40)},"base":{"repo":repo()},"user":{"id":51,"login":"alice"},"requested_reviewers":[],"requested_teams":[]})
}
fn limits() -> MetricsReadLimits {
    MetricsReadLimits {
        page_size: 3,
        max_pages: 4,
        max_rows: 12,
        ..Default::default()
    }
}
fn check(id: u64, status: &str, conclusion: Value) -> Value {
    json!({"id":id,"node_id":format!("CHK_{id}"),"head_sha":"a".repeat(40),"name":format!("check{id}"),"status":status,"conclusion":conclusion,"started_at":"2026-10-12T00:00:00Z","completed_at":if status=="completed" {json!("2026-10-12T00:01:00Z")} else {Value::Null},"url":format!("https://github.invalid/api/v3/repos/example/repo/check-runs/{id}"),"app":{"id":71,"owner":{"id":61,"login":"ci-org","type":"Organization"}}})
}
fn status(id: u64, state: &str, context: Value) -> Value {
    json!({"id":id,"node_id":format!("STATUS_{id}"),"state":state,"context":context,"created_at":"2026-10-12T00:00:00Z","updated_at":"2026-10-12T00:00:00Z","url":format!("https://github.invalid/api/v3/repos/example/repo/statuses/{}","a".repeat(40)),"creator":{"id":62,"login":"builder","type":"Bot"}})
}
fn run(id: u64, attempt: u64, status: &str, conclusion: Value) -> Value {
    json!({"id":id,"node_id":format!("WFR_{id}"),"workflow_id":81,"run_attempt":attempt,"head_sha":"b".repeat(40),"name":"CI","display_title":format!("Run{id}"),"event":"push","head_branch":"main","status":status,"conclusion":conclusion,"created_at":"2026-10-11T00:00:00Z","run_started_at":"2026-10-12T00:00:00Z","updated_at":"2026-10-13T00:00:00Z","actor":{"id":63,"login":"author","type":"User"},"triggering_actor":{"id":64,"login":"rerunner","type":"User"},"repository":repo(),"url":format!("https://github.invalid/api/v3/repos/example/repo/actions/runs/{id}")})
}
fn get(endpoint: &str, value: Value) -> GhStep {
    step(api_args(endpoint), value)
}
fn user(id: u64) -> GhStep {
    get("user", json!({"id":id,"login":"octocat"}))
}
fn check_page(
    rows: Vec<Value>,
    total: Option<u64>,
    page: usize,
    limits: &MetricsReadLimits,
) -> GhStep {
    let mut raw = json!({"check_runs":rows});
    if let Some(total) = total {
        raw["total_count"] = json!(total);
    }
    get(
        &format!(
            "repos/example/repo/commits/{}/check-runs?filter=latest&per_page={}&page={page}",
            "a".repeat(40),
            limits.page_size
        ),
        raw,
    )
}
fn status_page(
    rows: Vec<Value>,
    total: Option<u64>,
    page: usize,
    limits: &MetricsReadLimits,
) -> GhStep {
    let mut raw = json!({"sha":"a".repeat(40),"repository":repo(),"statuses":rows});
    if let Some(total) = total {
        raw["total_count"] = json!(total);
    }
    get(
        &format!(
            "repos/example/repo/commits/{}/status?per_page={}&page={page}",
            "a".repeat(40),
            limits.page_size
        ),
        raw,
    )
}
fn run_page(
    rows: Vec<Value>,
    total: Option<u64>,
    page: usize,
    limits: &MetricsReadLimits,
) -> GhStep {
    let mut raw = json!({"workflow_runs":rows});
    if let Some(total) = total {
        raw["total_count"] = json!(total);
    }
    get(
        &format!(
            "repos/example/repo/actions/runs?per_page={}&page={page}",
            limits.page_size
        ),
        raw,
    )
}
fn check_prefix() -> Vec<GhStep> {
    vec![
        user(42),
        get("repos/example/repo", repo()),
        get("repos/example/repo/pulls/1", parent()),
    ]
}
fn check_suffix() -> Vec<GhStep> {
    vec![
        get("repos/example/repo", repo()),
        get("repos/example/repo/pulls/1", parent()),
        user(42),
    ]
}
fn complete_checks(checks: GhStep, statuses: GhStep) -> Vec<GhStep> {
    let mut steps = check_prefix();
    steps.extend([checks, statuses]);
    steps.extend(check_suffix());
    steps
}
fn complete_runs(page: GhStep) -> Vec<GhStep> {
    vec![
        user(42),
        get("repos/example/repo", repo()),
        page,
        get("repos/example/repo", repo()),
        user(42),
    ]
}
fn prepare(fixture: &Fixture, reads: Vec<GhStep>) -> (GithubProvider, MetricsPrObservation) {
    let mut steps = vec![
        get("repos/example/repo", repo()),
        get(
            "repos/example/repo/pulls?state=all&sort=created&direction=desc&per_page=100&page=1",
            json!([parent()]),
        ),
        get("repos/example/repo", repo()),
    ];
    steps.extend(reads);
    fixture.scripted_gh(&steps);
    let gh = provider(fixture);
    let parent = gh
        .repository_pr_inventory(
            &current(),
            "example/repo",
            None,
            &Default::default(),
            &CancellationToken::default(),
        )
        .unwrap()
        .prs
        .remove(0);
    (gh, parent)
}
fn reads(fixture: &Fixture, count: usize) {
    let rows = journal(fixture);
    assert_eq!(rows.len(), count);
    assert!(rows
        .iter()
        .all(|row| row["matched"] == true && row["args"][5] == "GET" && row["stdin_hex"] == ""));
    assert!(rows.iter().all(|row| !row["args"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| arg == "--paginate")));
}

#[test]
fn metrics_health_provider_checks_are_exact_eight_reads_with_separate_coverage_and_no_required_rules(
) {
    let fixture = Fixture::new();
    let limits = limits();
    let (gh, parent) = prepare(
        &fixture,
        complete_checks(
            check_page(
                vec![
                    check(201, "completed", json!("failure")),
                    check(202, "queued", Value::Null),
                ],
                Some(2),
                1,
                &limits,
            ),
            status_page(vec![status(301, "error", json!("ci"))], Some(1), 1, &limits),
        ),
    );
    let result = gh
        .metrics_pr_check_health(&current(), &parent, &limits, &CancellationToken::default())
        .unwrap();
    assert_eq!(result.requests, 8);
    assert_eq!(result.raw_rows, 3);
    assert_eq!(result.collection_pages, 2);
    assert!(
        result.check_run_coverage.enumeration_complete
            && result.check_run_coverage.metric_evidence_complete
    );
    assert!(
        result.commit_status_coverage.enumeration_complete
            && result.commit_status_coverage.metric_evidence_complete
    );
    assert!(matches!(
        result.required_rules,
        MetricsRequiredRules::Unavailable { .. }
    ));
    assert!(result
        .source_limitations
        .iter()
        .any(|reason| reason.contains("1,000")));
    assert_eq!(result.check_runs[0].id, 201);
    assert_eq!(result.check_runs[0].node_id.as_deref(), Some("CHK_201"));
    assert_eq!(result.check_runs[0].head_sha, "a".repeat(40));
    assert_eq!(
        result.check_runs[0].actor.as_ref().unwrap().account_id,
        Some(61)
    );
    assert_eq!(
        result.commit_statuses[0].actor.as_ref().unwrap().account_id,
        Some(62)
    );
    assert_eq!(
        result.commit_statuses[0].result.health_conclusion(),
        "failure"
    );
    assert_eq!(result.check_runs[1].result.health_status(), "queued");
    assert_eq!(result.check_runs[1].result.health_conclusion(), "");
    assert_eq!(result.parent, parent);
    reads(&fixture, 11);
}

#[test]
fn metrics_health_provider_raw_success_diagnostics_count_even_when_display_is_empty() {
    let fixture = Fixture::new();
    let limits = limits();
    let mut steps = complete_runs(run_page(vec![], Some(0), 1, &limits));
    let diagnostic = b"\x1b[31m\x1b[0m\x01\x02".to_vec();
    steps[0].stderr = diagnostic.clone();
    let bytes: usize = steps.iter().map(|step| step.stdout.len()).sum();
    fixture.scripted_gh(&steps);
    let result = provider(&fixture)
        .metrics_repository_run_health(
            &current(),
            &canonical(),
            &limits,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(result.aggregate_response_bytes, bytes);
    assert_eq!(result.successful_diagnostic_bytes, diagnostic.len());
    assert_eq!(result.failed_reserved_bytes, 0);
    assert!(result.coverage.complete_empty);
    reads(&fixture, 5);

    let fixture = Fixture::new();
    let mut first = user(42);
    first.stderr = "\x1b[31m\x1b[0m".repeat(20).into_bytes();
    let cap = first.stdout.len() + first.stderr.len();
    fixture.scripted_gh(&[first]);
    assert!(provider(&fixture)
        .metrics_repository_run_health(
            &current(),
            &canonical(),
            &MetricsReadLimits {
                max_response_bytes: cap,
                ..limits
            },
            &CancellationToken::default()
        )
        .is_err());
    reads(&fixture, 1);
}
#[test]
fn metrics_health_provider_run_catalogue_five_reads_keeps_actual_attempt_actors_and_clocks() {
    let fixture = Fixture::new();
    let limits = limits();
    fixture.scripted_gh(&complete_runs(run_page(
        vec![
            run(401, 3, "completed", json!("success")),
            run(402, 2, "in_progress", Value::Null),
        ],
        Some(2),
        1,
        &limits,
    )));
    let result = provider(&fixture)
        .metrics_repository_run_health(
            &current(),
            &canonical(),
            &limits,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(result.requests, 5);
    assert_eq!(result.collection_pages, 1);
    assert_eq!(result.raw_rows, 2);
    assert!(result.coverage.enumeration_complete && result.coverage.metric_evidence_complete);
    assert_eq!(result.runs[0].id, 401);
    assert_eq!(result.runs[0].attempt, 3);
    assert_eq!(result.runs[0].workflow_id, 81);
    assert_eq!(result.runs[0].actor.as_ref().unwrap().account_id, Some(63));
    assert_eq!(
        result.runs[0].triggering_actor.as_ref().unwrap().account_id,
        Some(64)
    );
    assert_eq!(
        result.runs[0].created_at.as_ref().unwrap().raw(),
        "2026-10-11T00:00:00Z"
    );
    assert_eq!(
        result.runs[0].started_at.as_ref().unwrap().raw(),
        "2026-10-12T00:00:00Z"
    );
    assert_eq!(
        result.runs[0].observed_updated_at.as_ref().unwrap().raw(),
        "2026-10-13T00:00:00Z"
    );
    assert!(result
        .source_limitations
        .iter()
        .any(|reason| reason.contains("not completion")));
    assert_eq!(result.runs[1].status.api_value(), Some("in_progress"));
    assert!(result.runs[1].conclusion.is_none());
    reads(&fixture, 5);
}
#[test]
fn metrics_health_provider_nullable_names_actors_and_conclusions_retain_positive_check_facts() {
    let fixture = Fixture::new();
    let limits = limits();
    let mut raw = check(201, "completed", Value::Null);
    raw["name"] = Value::Null;
    raw["app"] = Value::Null;
    raw["node_id"] = Value::Null;
    let mut context = status(301, "future_state", Value::Null);
    context["creator"] = Value::Null;
    let (gh, parent) = prepare(
        &fixture,
        complete_checks(
            check_page(vec![raw], Some(1), 1, &limits),
            status_page(vec![context], Some(1), 1, &limits),
        ),
    );
    let result = gh
        .metrics_pr_check_health(&current(), &parent, &limits, &CancellationToken::default())
        .unwrap();
    assert_eq!(result.check_runs[0].id, 201);
    assert!(
        result.check_runs[0].name.is_none()
            && result.check_runs[0].actor.is_none()
            && result.check_runs[0].node_id.is_none()
    );
    assert!(
        result.check_run_coverage.enumeration_complete
            && !result.check_run_coverage.metric_evidence_complete
    );
    assert_eq!(result.commit_statuses[0].id, 301);
    assert!(!result.commit_statuses[0].current_context_unique);
    assert!(result.commit_statuses[0].name.is_none() && result.commit_statuses[0].actor.is_none());
    assert_eq!(result.commit_statuses[0].result.health_status(), "unknown");
    assert!(!result.commit_status_coverage.metric_evidence_complete);
    reads(&fixture, 11);
}
#[test]
fn metrics_health_provider_only_actual_exhausted_empty_total_is_zero_not_missing_or_permission() {
    for variation in 0..3 {
        let fixture = Fixture::new();
        let limits = limits();
        let mut page = run_page(
            vec![],
            if variation == 1 { None } else { Some(0) },
            1,
            &limits,
        );
        if variation == 2 {
            page.status = 1;
            page.stderr = b"HTTP 403: Actions denied".to_vec();
        }
        fixture.scripted_gh(&complete_runs(page));
        let result = provider(&fixture)
            .metrics_repository_run_health(
                &current(),
                &canonical(),
                &limits,
                &CancellationToken::default(),
            )
            .unwrap();
        assert!(result.runs.is_empty());
        assert_eq!(result.coverage.complete_empty, variation == 0);
        assert_eq!(result.coverage.enumeration_complete, variation == 0);
        if variation == 2 {
            assert!(matches!(
                result.coverage.status,
                MetricsReadStatus::Unavailable(_)
            ));
        }
        reads(&fixture, 5);
    }
}
#[test]
fn metrics_health_provider_endpoint_permission_preserves_other_known_positive_source_without_certifying_empty(
) {
    let fixture = Fixture::new();
    let limits = limits();
    let mut statuses = status_page(vec![], Some(0), 1, &limits);
    statuses.status = 1;
    statuses.stderr = b"HTTP 404: status source unavailable".to_vec();
    let (gh, parent) = prepare(
        &fixture,
        complete_checks(
            check_page(
                vec![check(201, "completed", json!("failure"))],
                Some(1),
                1,
                &limits,
            ),
            statuses,
        ),
    );
    let result = gh
        .metrics_pr_check_health(&current(), &parent, &limits, &CancellationToken::default())
        .unwrap();
    assert_eq!(result.check_runs.len(), 1);
    assert_eq!(result.check_runs[0].result.health_conclusion(), "failure");
    assert!(
        result.commit_statuses.is_empty()
            && !result.commit_status_coverage.complete_empty
            && !result.commit_status_coverage.enumeration_complete
    );
    assert!(matches!(
        result.commit_status_coverage.status,
        MetricsReadStatus::Unavailable(_)
    ));
    reads(&fixture, 11);
}
#[test]
fn metrics_health_provider_shared_page_row_caps_stop_before_second_source_without_truncating_old_protocol(
) {
    let fixture = Fixture::new();
    let limits = MetricsReadLimits {
        page_size: 1,
        max_pages: 2,
        max_rows: 2,
        ..Default::default()
    };
    let mut read = check_prefix();
    read.extend([
        check_page(
            vec![check(201, "completed", json!("failure"))],
            Some(2),
            1,
            &limits,
        ),
        check_page(vec![check(202, "queued", Value::Null)], Some(2), 2, &limits),
    ]);
    read.extend(check_suffix());
    let (gh, parent) = prepare(&fixture, read);
    let result = gh
        .metrics_pr_check_health(&current(), &parent, &limits, &CancellationToken::default())
        .unwrap();
    assert_eq!(result.check_runs.len(), 2);
    assert_eq!(result.collection_pages, 2);
    assert_eq!(result.raw_rows, 2);
    assert!(!result.check_run_coverage.enumeration_complete);
    assert_eq!(result.commit_status_coverage.requested_pages, 0);
    assert!(
        !result.commit_status_coverage.complete_empty
            && !result.commit_status_coverage.enumeration_complete
    );
    assert!(journal(&fixture).iter().all(|row| !row["args"][1]
        .as_str()
        .unwrap()
        .ends_with("/status?per_page=1&page=1")));
    reads(&fixture, 11);
}
#[test]
fn metrics_health_provider_highest_observed_attempt_is_not_a_complete_current_or_historical_claim()
{
    let fixture = Fixture::new();
    let limits = MetricsReadLimits {
        page_size: 1,
        max_pages: 3,
        max_rows: 3,
        ..Default::default()
    };
    fixture.scripted_gh(&[
        user(42),
        get("repos/example/repo", repo()),
        run_page(
            vec![run(401, 1, "completed", json!("failure"))],
            Some(2),
            1,
            &limits,
        ),
        run_page(
            vec![run(401, 3, "in_progress", Value::Null)],
            Some(2),
            2,
            &limits,
        ),
        run_page(vec![], Some(2), 3, &limits),
        get("repos/example/repo", repo()),
        user(42),
    ]);
    let result = provider(&fixture)
        .metrics_repository_run_health(
            &current(),
            &canonical(),
            &limits,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(result.runs.len(), 1);
    assert_eq!(result.runs[0].attempt, 3);
    assert_eq!(result.runs[0].status.api_value(), Some("in_progress"));
    assert_eq!(result.raw_rows, 2);
    assert!(!result.coverage.enumeration_complete && !result.coverage.metric_evidence_complete);
    reads(&fixture, 7);
}
#[test]
fn metrics_health_provider_repeated_exact_contexts_stay_distinct_unknown_current_not_case_guessed()
{
    let fixture = Fixture::new();
    let limits = MetricsReadLimits {
        page_size: 5,
        max_rows: 10,
        ..limits()
    };
    let (gh, parent) = prepare(
        &fixture,
        complete_checks(
            check_page(vec![], Some(0), 1, &limits),
            status_page(
                vec![
                    status(301, "failure", json!("ci")),
                    status(302, "success", json!("ci")),
                    status(303, "success", json!("CI")),
                ],
                Some(3),
                1,
                &limits,
            ),
        ),
    );
    let result = gh
        .metrics_pr_check_health(&current(), &parent, &limits, &CancellationToken::default())
        .unwrap();
    assert_eq!(
        result
            .commit_statuses
            .iter()
            .map(|status| status.id)
            .collect::<Vec<_>>(),
        [301, 302, 303]
    );
    assert!(
        !result.commit_statuses[0].current_context_unique
            && !result.commit_statuses[1].current_context_unique
    );
    assert!(result.commit_statuses[2].current_context_unique);
    assert_eq!(result.commit_statuses[0].health_status(), "unknown");
    assert_eq!(result.commit_statuses[1].health_conclusion(), "");
    assert_eq!(result.commit_statuses[2].health_conclusion(), "success");
    assert!(!result.commit_status_coverage.metric_evidence_complete);
    reads(&fixture, 11);
}
#[test]
fn metrics_health_provider_positive_canonical_principal_parent_repo_and_head_brackets_reject_drift()
{
    for variation in 0..5 {
        let fixture = Fixture::new();
        let limits = limits();
        let mut read = complete_checks(
            check_page(vec![], Some(0), 1, &limits),
            status_page(vec![], Some(0), 1, &limits),
        );
        let at = match variation {
            0 => 0,
            1 => 1,
            2 => 2,
            3 => 5,
            _ => 7,
        };
        let mut raw = if at == 0 || at == 7 {
            json!({"id":43,"login":"octocat"})
        } else if at == 1 || at == 5 {
            let mut repo = repo();
            repo["id"] = json!(100);
            repo
        } else {
            let mut pr = parent();
            pr["head"]["sha"] = json!("c".repeat(40));
            pr
        };
        if variation == 1 {
            raw["node_id"] = json!("R_recreated");
        }
        read[at].stdout = serde_json::to_vec(&raw).unwrap();
        read.truncate(at + 1);
        let (gh, parent) = prepare(&fixture, read);
        assert!(matches!(
            gh.metrics_pr_check_health(&current(), &parent, &limits, &CancellationToken::default())
                .unwrap_err()
                .kind,
            GithubErrorKind::IdentityChanged | GithubErrorKind::StaleHead
        ));
        reads(&fixture, 3 + at + 1);
    }
}
#[test]
fn metrics_health_provider_check_and_status_cross_head_host_repo_are_not_attached_by_prefix() {
    for variation in 0..4 {
        let fixture = Fixture::new();
        let limits = limits();
        let mut check = check(201, "completed", json!("success"));
        let mut statuses = status_page(vec![], Some(0), 1, &limits);
        match variation {
            0 => check["head_sha"] = json!("c".repeat(40)),
            1 => {
                check["url"] =
                    json!("https://other.invalid/api/v3/repos/example/repo/check-runs/201")
            }
            2 => {
                let mut raw: Value = serde_json::from_slice(&statuses.stdout).unwrap();
                raw["sha"] = json!("c".repeat(40));
                statuses.stdout = serde_json::to_vec(&raw).unwrap();
            }
            _ => {
                let mut raw: Value = serde_json::from_slice(&statuses.stdout).unwrap();
                raw["repository"]["id"] = json!(100);
                statuses.stdout = serde_json::to_vec(&raw).unwrap();
            }
        }
        let mut read = check_prefix();
        read.push(check_page(vec![check], Some(1), 1, &limits));
        if variation >= 2 {
            read.push(statuses);
        }
        let (gh, parent) = prepare(&fixture, read);
        assert!(matches!(
            gh.metrics_pr_check_health(&current(), &parent, &limits, &CancellationToken::default())
                .unwrap_err()
                .kind,
            GithubErrorKind::IdentityChanged | GithubErrorKind::StaleHead
        ));
        reads(&fixture, if variation < 2 { 7 } else { 8 });
    }
}
#[test]
fn metrics_health_provider_run_positive_identity_and_raw_document_caps_are_honest_partial() {
    for variation in 0..5 {
        let fixture = Fixture::new();
        let mut limits = limits();
        let mut raw = run(401, 2, "completed", json!("success"));
        match variation {
            0 => raw["id"] = json!(0),
            1 => raw["run_attempt"] = json!(0),
            2 => raw["head_sha"] = json!("b".repeat(7)),
            3 => {
                raw["created_at"] = Value::Null;
                raw["triggering_actor"] = Value::Null;
            }
            _ => {
                limits.max_document_bytes = 1024;
                raw["display_title"] = json!("x".repeat(2000));
            }
        }
        fixture.scripted_gh(&complete_runs(run_page(vec![raw], Some(1), 1, &limits)));
        let result = provider(&fixture)
            .metrics_repository_run_health(
                &current(),
                &canonical(),
                &limits,
                &CancellationToken::default(),
            )
            .unwrap();
        assert!(!result.coverage.metric_evidence_complete);
        assert!(!result.coverage.complete_empty);
        if variation == 3 {
            assert_eq!(result.runs[0].id, 401);
            assert!(
                result.runs[0].created_at.is_none() && result.runs[0].triggering_actor.is_none()
            );
        } else {
            assert!(result.runs.is_empty());
            assert_eq!(result.coverage.excluded_rows, 1);
        }
        reads(&fixture, 5);
    }
}
#[test]
fn metrics_health_provider_nonpositive_scope_invalid_sha_cancel_and_bytes_stop_before_more_calls() {
    let fixture = Fixture::new();
    let gh = provider(&fixture);
    for uid in [None, Some(0)] {
        assert!(gh
            .metrics_repository_run_health(
                &HostIdentity {
                    account_id: uid,
                    ..current()
                },
                &canonical(),
                &limits(),
                &CancellationToken::default()
            )
            .is_err());
    }
    for variation in 0..3 {
        let mut repo = canonical();
        match variation {
            0 => repo.id = 0,
            1 => repo.node_id.clear(),
            _ => repo.host = "other.invalid".into(),
        };
        assert!(gh
            .metrics_repository_run_health(
                &current(),
                &repo,
                &limits(),
                &CancellationToken::default()
            )
            .is_err());
    }
    let cancelled = CancellationToken::default();
    cancelled.cancel();
    assert!(gh
        .metrics_repository_run_health(&current(), &canonical(), &limits(), &cancelled)
        .is_err());
    assert!(gh
        .metrics_repository_run_health(
            &current(),
            &canonical(),
            &MetricsReadLimits {
                max_pages: 0,
                ..limits()
            },
            &CancellationToken::default()
        )
        .is_err());
    reads(&fixture, 0);
    let fixture = Fixture::new();
    let (gh, parent) = prepare(&fixture, vec![]);
    for head in [
        None,
        Some("a".repeat(7)),
        Some("a".repeat(33)),
        Some("z".repeat(40)),
    ] {
        let mut invalid = parent.clone();
        invalid.head_sha = head;
        assert!(gh
            .metrics_pr_check_health(
                &current(),
                &invalid,
                &limits(),
                &CancellationToken::default()
            )
            .is_err());
    }
    reads(&fixture, 3);
    let fixture = Fixture::new();
    fixture.scripted_gh(&[user(42)]);
    assert!(provider(&fixture)
        .metrics_repository_run_health(
            &current(),
            &canonical(),
            &MetricsReadLimits {
                max_response_bytes: 1,
                ..limits()
            },
            &CancellationToken::default()
        )
        .is_err());
    reads(&fixture, 1);
}

#[test]
fn metrics_health_provider_run_final_uid_repo_and_actual_node_workflow_membership_refuse_drift() {
    for variation in 0..5 {
        let fixture = Fixture::new();
        let limits = limits();
        let mut raw = run(401, 2, "completed", json!("success"));
        if variation == 2 {
            raw["repository"]["id"] = json!(100);
        }
        if variation == 3 {
            raw["url"] = json!("https://other.invalid/repos/example/repo/actions/runs/401");
        }
        let mut second = run(402, 2, "completed", json!("success"));
        if variation == 4 {
            second["node_id"] = json!("WFR_401");
        }
        let mut calls = complete_runs(run_page(
            if variation == 4 {
                vec![raw, second]
            } else {
                vec![raw]
            },
            Some(if variation == 4 { 2 } else { 1 }),
            1,
            &limits,
        ));
        let count = match variation {
            0 => {
                calls[3].stdout = serde_json::to_vec(
                    &json!({"id":100,"node_id":"R_replaced","full_name":"example/repo"}),
                )
                .unwrap();
                4
            }
            1 => {
                calls[4] = user(43);
                5
            }
            _ => 3,
        };
        calls.truncate(count);
        fixture.scripted_gh(&calls);
        assert!(matches!(
            provider(&fixture)
                .metrics_repository_run_health(
                    &current(),
                    &canonical(),
                    &limits,
                    &CancellationToken::default()
                )
                .unwrap_err()
                .kind,
            GithubErrorKind::IdentityChanged
        ));
        reads(&fixture, count);
    }
}

#[test]
fn metrics_health_provider_later_malformed_page_keeps_known_facts_and_duplicate_opaque_nodes_refuse(
) {
    let fixture = Fixture::new();
    let limits = MetricsReadLimits {
        page_size: 1,
        max_pages: 3,
        max_rows: 3,
        ..Default::default()
    };
    let mut malformed = run_page(vec![], Some(2), 2, &limits);
    malformed.stdout = serde_json::to_vec(&json!({"total_count":2,"workflow_runs":null})).unwrap();
    fixture.scripted_gh(&[
        user(42),
        get("repos/example/repo", repo()),
        run_page(
            vec![run(401, 2, "completed", json!("failure"))],
            Some(2),
            1,
            &limits,
        ),
        malformed,
        get("repos/example/repo", repo()),
        user(42),
    ]);
    let result = provider(&fixture)
        .metrics_repository_run_health(
            &current(),
            &canonical(),
            &limits,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(result.runs[0].id, 401);
    assert_eq!(
        result.runs[0].conclusion.as_ref().unwrap().api_value(),
        "failure"
    );
    assert!(
        !result.coverage.enumeration_complete
            && !result.coverage.complete_empty
            && !result.coverage.metric_evidence_complete
    );
    reads(&fixture, 6);
    let fixture = Fixture::new();
    let limits = self::limits();
    let mut raw = status(301, "success", json!("ci"));
    raw["node_id"] = json!("CHK_201");
    let mut calls = complete_checks(
        check_page(
            vec![check(201, "completed", json!("failure"))],
            Some(1),
            1,
            &limits,
        ),
        status_page(vec![raw], Some(1), 1, &limits),
    );
    calls.truncate(5);
    let (gh, parent) = prepare(&fixture, calls);
    assert!(matches!(
        gh.metrics_pr_check_health(&current(), &parent, &limits, &CancellationToken::default())
            .unwrap_err()
            .kind,
        GithubErrorKind::IdentityChanged
    ));
    reads(&fixture, 8);
}

#[test]
fn metrics_health_provider_failed_multibyte_dual_capture_reserves_cap_and_cannot_launch_over_budget(
) {
    for exhausted in [false, true] {
        let fixture = Fixture::new();
        let mut limits = limits();
        let mut failed = run_page(vec![], Some(0), 1, &limits);
        failed.status = 1;
        failed.stdout = "é".repeat(1000).into_bytes();
        failed.stderr = format!("HTTP 502: {}", "界".repeat(1000)).into_bytes();
        let successful_bytes = user(42).stdout.len()
            + get("repos/example/repo", repo()).stdout.len()
            + get("repos/example/repo", repo()).stdout.len()
            + user(42).stdout.len();
        if exhausted {
            limits.max_response_bytes =
                user(42).stdout.len() + get("repos/example/repo", repo()).stdout.len() + 4096;
        }
        fixture.scripted_gh(&complete_runs(failed));
        let result = provider(&fixture).metrics_repository_run_health(
            &current(),
            &canonical(),
            &limits,
            &CancellationToken::default(),
        );
        if exhausted {
            assert!(result.is_err());
            reads(&fixture, 3);
        } else {
            let source = result.unwrap();
            assert_eq!(source.failed_reserved_bytes, 4 * 1024 * 1024);
            assert_eq!(source.aggregate_response_bytes, successful_bytes);
            assert!(matches!(
                source.coverage.status,
                MetricsReadStatus::Unavailable(_)
            ));
            assert!(!source.coverage.complete_empty);
            reads(&fixture, 5);
        }
    }
}

#[test]
fn metrics_health_provider_deadline_and_inflight_cancel_require_final_available_authority_budget() {
    for cancel_read in [false, true] {
        let fixture = Fixture::new();
        let mut slow = user(42);
        slow.delay_ms = 1000;
        fixture.scripted_gh(&[slow]);
        let gh = provider(&fixture);
        let token = CancellationToken::default();
        if cancel_read {
            let worker_token = token.clone();
            let worker = thread::spawn(move || {
                gh.metrics_repository_run_health(&current(), &canonical(), &limits(), &worker_token)
            });
            let deadline = Instant::now() + Duration::from_secs(2);
            while journal(&fixture).is_empty() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(5));
            }
            assert_eq!(journal(&fixture).len(), 1);
            token.cancel();
            assert!(worker.join().unwrap().is_err());
        } else {
            assert!(gh
                .metrics_repository_run_health(
                    &current(),
                    &canonical(),
                    &MetricsReadLimits {
                        max_duration: Duration::from_millis(250),
                        ..limits()
                    },
                    &token
                )
                .is_err());
        }
        reads(&fixture, 1);
    }
}
