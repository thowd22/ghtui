//! SOURCE-ONLY opaque five-second lease/capture contracts. No native result.
use super::*;
use ghtui::providers::gh::{
    MetricsPrObservation, MetricsReadLimits, MetricsRepository, METRICS_SCOPE_RECEIPT_MAX_AGE,
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
fn extra() -> MetricsRepository {
    MetricsRepository {
        id: 199,
        node_id: "R_199".into(),
        full_name: "example/empty".into(),
        ..canonical()
    }
}
fn pull(number: u64) -> Value {
    json!({"id":100+number,"node_id":format!("PR_{number}"),"number":number,"title":"Original","state":"open","draft":false,"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-12T00:00:00Z","merged_at":null,"closed_at":null,"merged":false,"head":{"sha":"a".repeat(40)},"base":{"repo":repo()},"user":{"id":51,"login":"alice"},"requested_reviewers":[],"requested_teams":[]})
}
fn get(endpoint: &str, value: Value) -> GhStep {
    step(api_args(endpoint), value)
}
fn user(id: u64) -> GhStep {
    get("user", json!({"id":id,"login":"octocat"}))
}
fn canonical_reads(number: u64) -> Vec<GhStep> {
    vec![
        get("repos/example/repo", repo()),
        get(&format!("repos/example/repo/pulls/{number}"), pull(number)),
        get("repos/example/repo", repo()),
    ]
}
fn five() -> Vec<GhStep> {
    let mut reads = vec![user(42)];
    reads.extend(canonical_reads(1));
    reads.push(user(42));
    reads
}
fn prepare(fixture: &Fixture, reads: Vec<GhStep>) -> (GithubProvider, Vec<MetricsPrObservation>) {
    let mut steps = vec![
        get("repos/example/repo", repo()),
        get(
            "repos/example/repo/pulls?state=all&sort=created&direction=desc&per_page=100&page=1",
            json!([pull(1), pull(2)]),
        ),
        get("repos/example/repo", repo()),
    ];
    steps.extend(reads);
    fixture.scripted_gh(&steps);
    let gh = provider(fixture);
    let parents = gh
        .repository_pr_inventory(
            &current(),
            "example/repo",
            None,
            &Default::default(),
            &CancellationToken::default(),
        )
        .unwrap()
        .prs;
    (gh, parents)
}
fn assert_reads(fixture: &Fixture, count: usize) {
    let rows = journal(fixture);
    assert_eq!(rows.len(), count);
    assert!(rows
        .iter()
        .all(|row| row["matched"] == true && row["args"][5] == "GET" && row["stdin_hex"] == ""));
}

#[test]
fn metrics_scope_receipts_exact_five_canonical_reads_make_only_short_lived_current_proof() {
    let fixture = Fixture::new();
    let reads = five();
    let bytes: usize = reads.iter().map(|step| step.stdout.len()).sum();
    let (gh, parents) = prepare(&fixture, reads);
    let receipt = gh
        .metric_scope_receipt(
            &current(),
            &parents[0],
            &Default::default(),
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(receipt.identity().account_id, Some(42));
    assert_eq!(receipt.parent(), &parents[0]);
    assert!(receipt.observed_interval().1 >= receipt.observed_interval().0);
    assert_eq!(receipt.accounting().requests(), 5);
    assert_eq!(receipt.accounting().requested_pages(), 0);
    assert_eq!(receipt.accounting().fetched_pages(), 0);
    assert_eq!(receipt.accounting().raw_rows(), 0);
    assert_eq!(receipt.accounting().successful_stdout_bytes(), bytes);
    assert_eq!(receipt.accounting().successful_diagnostic_bytes(), 0);
    assert_eq!(receipt.accounting().failed_reserved_bytes(), 0);
    assert_eq!(receipt.accounting().charged_bytes(), bytes);
    receipt
        .validate_for(
            &current(),
            &parents[0],
            METRICS_SCOPE_RECEIPT_MAX_AGE,
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(!format!("{receipt:?}").contains("PR_1"));
    assert!(!format!("{receipt:?}").contains("octocat"));
    assert_reads(&fixture, 8);
}

#[test]
fn metrics_scope_receipts_batch_all_original_headers_are_exact_and_counters_charged_once() {
    let fixture = Fixture::new();
    let mut reads = vec![user(42)];
    reads.extend(canonical_reads(1));
    reads.extend(canonical_reads(2));
    reads.push(get(
        "repos/example/empty",
        json!({"id":199,"node_id":"R_199","full_name":"example/empty"}),
    ));
    reads.push(user(42));
    let bytes: usize = reads.iter().map(|step| step.stdout.len()).sum();
    let (gh, parents) = prepare(&fixture, reads);
    let batch = gh
        .metric_scope_receipts_with_repositories(
            &current(),
            &parents,
            &[extra()],
            &Default::default(),
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(batch.accounting().requests(), 9);
    assert_eq!(batch.accounting().charged_bytes(), bytes);
    assert_eq!(batch.receipts().len(), 2);
    assert_eq!(batch.checked_repositories().len(), 2);
    batch
        .validate_repositories(
            &current(),
            &[canonical(), extra()],
            METRICS_SCOPE_RECEIPT_MAX_AGE,
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(batch
        .validate_repositories(
            &current(),
            &[canonical()],
            METRICS_SCOPE_RECEIPT_MAX_AGE,
            &CancellationToken::default()
        )
        .is_err());
    for (parent, receipt) in parents.iter().zip(batch.receipts()) {
        receipt
            .validate_for(
                &current(),
                parent,
                METRICS_SCOPE_RECEIPT_MAX_AGE,
                &CancellationToken::default(),
            )
            .unwrap();
        assert_eq!(receipt.accounting(), batch.accounting());
    }
    let mut changed = extra();
    changed.id = 200;
    assert!(batch
        .validate_repositories(
            &current(),
            &[canonical(), changed],
            METRICS_SCOPE_RECEIPT_MAX_AGE,
            &CancellationToken::default()
        )
        .is_err());
    assert_reads(&fixture, 12);
}

#[test]
fn metrics_scope_receipts_genuine_headers_only_are_not_fabricated_empty_parent_proofs() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[
        user(42),
        get(
            "repos/example/empty",
            json!({"id":199,"node_id":"R_199","full_name":"example/empty"}),
        ),
        user(42),
    ]);
    let batch = provider(&fixture)
        .metric_scope_receipts_with_repositories(
            &current(),
            &[],
            &[extra()],
            &Default::default(),
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(batch.receipts().is_empty());
    assert_eq!(batch.accounting().requests(), 3);
    assert_eq!(batch.checked_repositories(), &[extra()]);
    batch
        .validate_repositories(
            &current(),
            &[extra()],
            METRICS_SCOPE_RECEIPT_MAX_AGE,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_reads(&fixture, 3);
}

#[test]
fn metrics_scope_receipts_lease_cannot_be_retargeted_extended_or_reused_after_cancel() {
    let fixture = Fixture::new();
    let (gh, parents) = prepare(&fixture, five());
    let token = CancellationToken::default();
    let receipt = gh
        .metric_scope_receipt(&current(), &parents[0], &Default::default(), &token)
        .unwrap();
    for age in [Duration::ZERO, Duration::from_secs(6)] {
        assert!(receipt
            .validate_for(&current(), &parents[0], age, &token)
            .is_err());
    }
    assert!(receipt
        .validate_for(
            &HostIdentity {
                account_id: Some(43),
                ..current()
            },
            &parents[0],
            METRICS_SCOPE_RECEIPT_MAX_AGE,
            &token
        )
        .is_err());
    assert!(receipt
        .validate_for(
            &current(),
            &parents[1],
            METRICS_SCOPE_RECEIPT_MAX_AGE,
            &token
        )
        .is_err());
    let mut changed = parents[0].clone();
    changed.head_sha = Some("b".repeat(40));
    assert!(receipt
        .validate_for(&current(), &changed, METRICS_SCOPE_RECEIPT_MAX_AGE, &token)
        .is_err());
    assert!(receipt
        .validate_for(&current(), &parents[0], Duration::from_nanos(1), &token)
        .is_err());
    token.cancel();
    assert!(receipt
        .validate_for(
            &current(),
            &parents[0],
            METRICS_SCOPE_RECEIPT_MAX_AGE,
            &token
        )
        .is_err());
    assert_reads(&fixture, 8);
}

#[test]
fn metrics_scope_receipts_invalid_targets_duplicates_and_limits_never_launch_transport() {
    let fixture = Fixture::new();
    let (gh, parents) = prepare(&fixture, vec![]);
    let token = CancellationToken::default();
    let limits = MetricsReadLimits::default();
    let mut variants = vec![];
    for head in [None, Some("a".repeat(7)), Some("z".repeat(40))] {
        let mut p = parents[0].clone();
        p.head_sha = head;
        variants.push(p);
    }
    let mut p = parents[0].clone();
    p.id = 0;
    variants.push(p);
    let mut p = parents[0].clone();
    p.repository.id = 0;
    variants.push(p);
    let mut p = parents[0].clone();
    p.node_id = "".into();
    variants.push(p);
    for p in variants {
        assert!(gh
            .metric_scope_receipt(&current(), &p, &limits, &token)
            .is_err());
    }
    assert!(gh
        .metric_scope_receipts(
            &current(),
            &[parents[0].clone(), parents[0].clone()],
            &limits,
            &token
        )
        .is_err());
    assert!(gh
        .metric_scope_receipts_with_repositories(
            &current(),
            &parents[..1],
            &[canonical()],
            &limits,
            &token
        )
        .is_err());
    assert!(gh
        .metric_scope_receipts(&current(), &[], &limits, &token)
        .is_err());
    assert!(gh
        .metric_scope_receipt(
            &HostIdentity {
                account_id: Some(0),
                ..current()
            },
            &parents[0],
            &limits,
            &token
        )
        .is_err());
    assert!(gh
        .metric_scope_receipt(
            &current(),
            &parents[0],
            &MetricsReadLimits {
                max_response_bytes: 0,
                ..limits.clone()
            },
            &token
        )
        .is_err());
    token.cancel();
    assert!(gh
        .metric_scope_receipt(&current(), &parents[0], &limits, &token)
        .is_err());
    assert_reads(&fixture, 3);
}

#[test]
fn metrics_scope_receipts_current_repo_parent_head_and_uid_replacements_abort_exactly() {
    for at in [0usize, 1, 2, 3, 4] {
        let fixture = Fixture::new();
        let mut reads = five();
        let mut raw: Value = serde_json::from_slice(&reads[at].stdout).unwrap();
        if at == 0 || at == 4 {
            raw["id"] = json!(43);
        } else if at == 2 {
            raw["head"]["sha"] = json!("b".repeat(40));
        } else {
            raw["id"] = json!(199);
            raw["node_id"] = json!("R_recreated");
        }
        reads[at].stdout = serde_json::to_vec(&raw).unwrap();
        reads.truncate(at + 1);
        let (gh, parents) = prepare(&fixture, reads);
        let err = gh
            .metric_scope_receipt(
                &current(),
                &parents[0],
                &Default::default(),
                &CancellationToken::default(),
            )
            .unwrap_err();
        assert!(matches!(
            err.kind,
            GithubErrorKind::IdentityChanged | GithubErrorKind::StaleHead
        ));
        assert_reads(&fixture, 3 + at + 1);
    }
}

#[test]
fn metrics_scope_receipts_control_only_success_stderr_is_charged_before_later_requests() {
    let fixture = Fixture::new();
    let mut reads = five();
    let diagnostics = b"\x1b[31m\x1b[0m\x01\x02".to_vec();
    reads[0].stderr = diagnostics.clone();
    let stdout: usize = reads.iter().map(|step| step.stdout.len()).sum();
    let (gh, parents) = prepare(&fixture, reads);
    let receipt = gh
        .metric_scope_receipt(
            &current(),
            &parents[0],
            &Default::default(),
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(receipt.accounting().successful_stdout_bytes(), stdout);
    assert_eq!(
        receipt.accounting().successful_diagnostic_bytes(),
        diagnostics.len()
    );
    assert_eq!(
        receipt.accounting().charged_bytes(),
        stdout + diagnostics.len()
    );
    assert_reads(&fixture, 8);
    let fixture = Fixture::new();
    let mut reads = five();
    reads[0].stderr = "\x1b[31m\x1b[0m".repeat(20).into_bytes();
    let cap = reads[0].stdout.len() + reads[0].stderr.len();
    reads.truncate(1);
    let (gh, parents) = prepare(&fixture, reads);
    assert!(gh
        .metric_scope_receipt(
            &current(),
            &parents[0],
            &MetricsReadLimits {
                max_response_bytes: cap,
                ..Default::default()
            },
            &CancellationToken::default()
        )
        .is_err());
    assert_reads(&fixture, 4);
}

#[test]
fn metrics_scope_receipts_failed_dualstream_capture_and_deadline_never_issue_or_fetch_later() {
    for limited in [false, true] {
        let fixture = Fixture::new();
        let mut failed = user(42);
        failed.status = 1;
        failed.stdout = "é".repeat(1000).into_bytes();
        failed.stderr = format!("HTTP 502 {}", "界".repeat(1000)).into_bytes();
        let (gh, parents) = prepare(&fixture, vec![failed]);
        let limits = MetricsReadLimits {
            max_response_bytes: if limited { 1024 } else { 16 * 1024 * 1024 },
            ..Default::default()
        };
        assert!(gh
            .metric_scope_receipt(
                &current(),
                &parents[0],
                &limits,
                &CancellationToken::default()
            )
            .is_err());
        assert_reads(&fixture, 4);
    }
    let fixture = Fixture::new();
    let mut delayed = user(42);
    delayed.delay_ms = 1000;
    let (gh, parents) = prepare(&fixture, vec![delayed]);
    assert!(gh
        .metric_scope_receipt(
            &current(),
            &parents[0],
            &MetricsReadLimits {
                max_duration: Duration::from_millis(250),
                ..Default::default()
            },
            &CancellationToken::default()
        )
        .is_err());
    assert_reads(&fixture, 4);
}

#[test]
fn metrics_scope_receipts_common_collection_failure_reserves_full_shared_cap_before_closing_repo() {
    for enough in [false, true] {
        let fixture = Fixture::new();
        let mut failed = get(
            "repos/example/repo/pulls?state=all&sort=created&direction=desc&per_page=100&page=1",
            Value::Null,
        );
        failed.status = 1;
        failed.stdout = "é".repeat(1000).into_bytes();
        failed.stderr = format!("HTTP 502 {}", "界".repeat(1000)).into_bytes();
        let mut reads = vec![get("repos/example/repo", repo()), failed];
        if enough {
            reads.push(get("repos/example/repo", repo()));
        }
        fixture.scripted_gh(&reads);
        let limits = MetricsReadLimits {
            max_response_bytes: if enough { 16 * 1024 * 1024 } else { 16 * 1024 },
            ..Default::default()
        };
        let result = provider(&fixture).repository_pr_inventory(
            &current(),
            "example/repo",
            None,
            &limits,
            &CancellationToken::default(),
        );
        if enough {
            let source = result.unwrap();
            assert!(source.prs.is_empty());
            assert!(!source.coverage.complete_empty);
            assert!(matches!(
                source.coverage.status,
                ghtui::providers::gh::MetricsReadStatus::Unavailable(_)
            ));
            assert_reads(&fixture, 3);
        } else {
            assert!(result.is_err());
            assert_reads(&fixture, 2);
        }
    }
}

#[test]
fn metrics_scope_receipts_slow_final_identity_does_not_renew_original_canonical_clock() {
    let fixture = Fixture::new();
    let mut reads = five();
    reads[4].delay_ms = 5100;
    let (gh, parents) = prepare(&fixture, reads);
    let err = gh
        .metric_scope_receipt(
            &current(),
            &parents[0],
            &MetricsReadLimits {
                max_duration: Duration::from_secs(8),
                ..Default::default()
            },
            &CancellationToken::default(),
        )
        .unwrap_err();
    assert!(matches!(err.kind, GithubErrorKind::Transport));
    assert!(err.message.contains("lease"));
    assert_reads(&fixture, 8);
}

#[test]
fn metrics_scope_receipts_inflight_cancellation_closes_without_remaining_canonical_calls() {
    let fixture = Fixture::new();
    let mut delayed = user(42);
    delayed.delay_ms = 1000;
    let (gh, parents) = prepare(&fixture, vec![delayed]);
    let token = CancellationToken::default();
    let worker_token = token.clone();
    let worker = thread::spawn(move || {
        gh.metric_scope_receipt(&current(), &parents[0], &Default::default(), &worker_token)
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    while journal(&fixture).len() < 4 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(journal(&fixture).len(), 4);
    token.cancel();
    assert!(worker.join().unwrap().is_err());
    assert_reads(&fixture, 4);
}
