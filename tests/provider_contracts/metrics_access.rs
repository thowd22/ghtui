//! Staged exact1/3 current access/drill contracts. Register at Root source window.
use super::*;
use ghtui::providers::gh::{MetricsPrObservation, MetricsReadLimits, MetricsRepository};

fn current() -> HostIdentity {
    HostIdentity {
        account_id: Some(42),
        ..identity()
    }
}
fn canonical() -> MetricsRepository {
    MetricsRepository {
        host: "github.invalid".into(),
        id: 99,
        node_id: "R_99".into(),
        full_name: "example/repo".into(),
    }
}
fn repo() -> Value {
    json!({"id":99,"node_id":"R_99","full_name":"example/repo","url":"https://github.invalid/api/v3/repos/example/repo"})
}
fn parent() -> Value {
    json!({"id":101,"node_id":"PR_1","number":1,"title":"Original PR","state":"open","draft":false,"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-12T00:00:00Z","merged_at":null,"closed_at":null,"merged":false,"head":{"sha":"a".repeat(40)},"base":{"repo":repo()},"user":{"id":51,"login":"alice"},"requested_reviewers":[],"requested_teams":[]})
}
fn get(endpoint: &str, value: Value) -> GhStep {
    step(api_args(endpoint), value)
}
fn initial(parent: Value) -> Vec<GhStep> {
    vec![
        get("repos/example/repo", repo()),
        get(
            "repos/example/repo/pulls?state=all&sort=created&direction=desc&per_page=100&page=1",
            json!([parent]),
        ),
        get("repos/example/repo", repo()),
    ]
}
fn load(gh: &GithubProvider) -> MetricsPrObservation {
    gh.repository_pr_inventory(
        &current(),
        "example/repo",
        None,
        &Default::default(),
        &CancellationToken::default(),
    )
    .unwrap()
    .prs
    .remove(0)
}
fn reads(fixture: &Fixture, count: usize) {
    let rows = journal(fixture);
    assert_eq!(rows.len(), count);
    assert!(rows
        .iter()
        .all(|row| row["matched"] == true && row["args"][5] == "GET" && row["stdin_hex"] == ""));
    assert!(rows.iter().all(|row| row["args"][1] != "user")); // Root owns identity brackets.
}

#[test]
fn metrics_access_repository_is_one_exact_bounded_get_without_discovery_or_history() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[get("repos/example/repo", repo())]);
    assert_eq!(
        provider(&fixture)
            .metrics_repository_identity(
                &current(),
                &canonical(),
                &Default::default(),
                &CancellationToken::default()
            )
            .unwrap(),
        canonical()
    );
    reads(&fixture, 1);
}
#[test]
fn metrics_access_pr_is_exact_three_reads_and_nullable_metadata_is_not_invented() {
    let fixture = Fixture::new();
    let mut raw = parent();
    raw["created_at"] = Value::Null;
    raw["draft"] = Value::Null;
    raw.as_object_mut().unwrap().remove("requested_reviewers");
    let mut steps = initial(raw.clone());
    steps.extend([
        get("repos/example/repo", repo()),
        get("repos/example/repo/pulls/1", raw),
        get("repos/example/repo", repo()),
    ]);
    fixture.scripted_gh(&steps);
    let gh = provider(&fixture);
    let original = load(&gh);
    let fresh = gh
        .metrics_pr_identity(
            &current(),
            &original,
            &Default::default(),
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(fresh, original);
    assert!(fresh.created_at.is_none() && fresh.draft.is_none() && fresh.requested_users.is_none());
    assert!(fresh.observed_updated_at.is_some());
    assert_eq!(fresh.id, 101);
    assert_eq!(fresh.node_id, "PR_1");
    reads(&fixture, 6); // Three explicit preparatory inventory reads + exact3 drill reads.
    let endpoints = journal(&fixture)
        .into_iter()
        .skip(3)
        .map(|row| row["args"][1].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        endpoints,
        [
            "repos/example/repo",
            "repos/example/repo/pulls/1",
            "repos/example/repo"
        ]
    );
}
#[test]
fn metrics_access_invalid_principal_positive_ids_full_sha_and_limits_stop_before_transport() {
    let fixture = Fixture::new();
    let gh = provider(&fixture);
    for account_id in [None, Some(0)] {
        assert!(gh
            .metrics_repository_identity(
                &HostIdentity {
                    account_id,
                    ..current()
                },
                &canonical(),
                &Default::default(),
                &CancellationToken::default()
            )
            .is_err());
    }
    for variation in 0..4 {
        let mut expected = canonical();
        match variation {
            0 => expected.id = 0,
            1 => expected.node_id.clear(),
            2 => expected.host = "other.invalid".into(),
            _ => expected.full_name = "EXAMPLE/REPO".into(),
        };
        assert!(gh
            .metrics_repository_identity(
                &current(),
                &expected,
                &Default::default(),
                &CancellationToken::default()
            )
            .is_err());
    }
    assert!(gh
        .metrics_repository_identity(
            &current(),
            &canonical(),
            &MetricsReadLimits {
                max_response_bytes: 0,
                ..Default::default()
            },
            &CancellationToken::default()
        )
        .is_err());
    let cancelled = CancellationToken::default();
    cancelled.cancel();
    assert!(gh
        .metrics_repository_identity(&current(), &canonical(), &Default::default(), &cancelled)
        .is_err());
    assert!(journal(&fixture).is_empty());
    fixture.scripted_gh(&initial(parent()));
    let original = load(&gh);
    for variation in 0..6 {
        let mut invalid = original.clone();
        match variation {
            0 => invalid.id = 0,
            1 => invalid.number = 0,
            2 => invalid.node_id.clear(),
            3 => invalid.head_sha = Some("a".repeat(7)),
            4 => invalid.head_sha = None,
            _ => invalid.repository.node_id = "bad\nnode".into(),
        };
        assert!(gh
            .metrics_pr_identity(
                &current(),
                &invalid,
                &Default::default(),
                &CancellationToken::default()
            )
            .is_err());
    }
    assert!(gh
        .metrics_pr_identity(&current(), &original, &Default::default(), &cancelled)
        .is_err());
    reads(&fixture, 3);
}
#[test]
fn metrics_access_same_slug_recreation_wrong_canonical_name_and_nonpositive_response_refuse() {
    for variation in 0..4 {
        let fixture = Fixture::new();
        let mut raw = repo();
        match variation {
            0 => raw["id"] = json!(100),
            1 => raw["node_id"] = json!("R_recreated"),
            2 => raw["full_name"] = json!("other/repo"),
            _ => raw["id"] = json!(0),
        };
        fixture.scripted_gh(&[get("repos/example/repo", raw)]);
        let error = provider(&fixture)
            .metrics_repository_identity(
                &current(),
                &canonical(),
                &Default::default(),
                &CancellationToken::default(),
            )
            .unwrap_err();
        if variation == 3 {
            assert!(matches!(error.kind, GithubErrorKind::Json));
        } else {
            assert!(matches!(error.kind, GithubErrorKind::IdentityChanged));
        }
        reads(&fixture, 1);
    }
}
#[test]
fn metrics_access_pr_head_parent_lifecycle_request_and_freshness_drift_stop_before_final_get() {
    for variation in 0..9 {
        let fixture = Fixture::new();
        let mut changed = parent();
        match variation {
            0 => changed["head"]["sha"] = json!("a".repeat(7)),
            1 => changed["id"] = json!(102),
            2 => changed["node_id"] = json!("PR_recreated"),
            3 => {
                changed["state"] = json!("closed");
                changed["closed_at"] = json!("2026-10-13T00:00:00Z");
            }
            4 => changed["draft"] = json!(true),
            5 => changed["updated_at"] = json!("2026-10-13T00:00:00Z"),
            6 => changed["requested_reviewers"] = json!([{"id":42,"login":"viewer"}]),
            7 => changed["created_at"] = json!("2026-10-02T00:00:00Z"),
            _ => changed["base"]["repo"]["node_id"] = json!("R_replaced"),
        }
        let mut steps = initial(parent());
        steps.extend([
            get("repos/example/repo", repo()),
            get("repos/example/repo/pulls/1", changed),
        ]);
        fixture.scripted_gh(&steps);
        let gh = provider(&fixture);
        let original = load(&gh);
        let error = gh
            .metrics_pr_identity(
                &current(),
                &original,
                &Default::default(),
                &CancellationToken::default(),
            )
            .unwrap_err();
        if matches!(variation, 1 | 2 | 8) {
            assert!(matches!(error.kind, GithubErrorKind::IdentityChanged));
        } else {
            assert!(matches!(error.kind, GithubErrorKind::StaleHead));
        }
        reads(&fixture, 5);
    }
    let fixture = Fixture::new();
    let mut replaced = repo();
    replaced["id"] = json!(100);
    replaced["node_id"] = json!("R_replaced");
    let mut steps = initial(parent());
    steps.extend([
        get("repos/example/repo", repo()),
        get("repos/example/repo/pulls/1", parent()),
        get("repos/example/repo", replaced),
    ]);
    fixture.scripted_gh(&steps);
    let gh = provider(&fixture);
    let original = load(&gh);
    assert!(matches!(
        gh.metrics_pr_identity(
            &current(),
            &original,
            &Default::default(),
            &CancellationToken::default()
        )
        .unwrap_err()
        .kind,
        GithubErrorKind::IdentityChanged
    ));
    reads(&fixture, 6);
}
#[test]
fn metrics_access_bytes_and_permission_failures_have_exact_bounded_request_counts() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[get("repos/example/repo", repo())]);
    assert!(provider(&fixture)
        .metrics_repository_identity(
            &current(),
            &canonical(),
            &MetricsReadLimits {
                max_response_bytes: 1,
                ..Default::default()
            },
            &CancellationToken::default()
        )
        .is_err());
    reads(&fixture, 1);
    let fixture = Fixture::new();
    let mut denied = get("repos/example/repo", repo());
    denied.status = 1;
    denied.stderr = b"HTTP 403: current repository access denied".to_vec();
    fixture.scripted_gh(&[denied]);
    assert!(matches!(
        provider(&fixture)
            .metrics_repository_identity(
                &current(),
                &canonical(),
                &Default::default(),
                &CancellationToken::default()
            )
            .unwrap_err()
            .kind,
        GithubErrorKind::Permission
    ));
    reads(&fixture, 1);
    let fixture = Fixture::new();
    let proof = [
        get("repos/example/repo", repo()),
        get("repos/example/repo/pulls/1", parent()),
        get("repos/example/repo", repo()),
    ];
    let cap = proof.iter().map(|step| step.stdout.len()).sum::<usize>() - 1;
    let mut steps = initial(parent());
    steps.extend(proof);
    fixture.scripted_gh(&steps);
    let gh = provider(&fixture);
    let original = load(&gh);
    assert!(gh
        .metrics_pr_identity(
            &current(),
            &original,
            &MetricsReadLimits {
                max_response_bytes: cap,
                ..Default::default()
            },
            &CancellationToken::default()
        )
        .is_err());
    reads(&fixture, 6);
}
