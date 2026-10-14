//! Source-only strict bounded remote discovery contracts; register at activation.
use super::*;
use ghtui::providers::gh::{RemoteCanonicalIdentity, RemoteSearchLimits, RemoteSearchStatus};

fn current() -> HostIdentity {
    HostIdentity {
        account_id: Some(42),
        ..identity()
    }
}
fn limits(
    kind: RemoteSearchKind,
    pages: usize,
    results: usize,
    bytes: usize,
) -> RemoteSearchLimits {
    RemoteSearchLimits {
        selected_kinds: vec![kind],
        max_pages: pages,
        max_results: results,
        max_response_bytes: bytes,
        max_duration: Duration::from_secs(10),
    }
}
fn raw_pr(n: u64) -> Value {
    let mut item = pr(n, "example/repo");
    item["repository_url"] = json!("https://github.invalid/api/v3/repos/example/repo");
    item["id"] = json!(n + 1000);
    item["repository"] = json!({"id":99,"full_name":"example/repo"});
    item["pull_request"] =
        json!({"url":format!("https://api.github.invalid/repos/example/repo/pulls/{n}")});
    item
}
fn page(
    kind: RemoteSearchKind,
    page: usize,
    per_page: usize,
    total: u64,
    items: Vec<Value>,
    incomplete: bool,
) -> GhStep {
    let endpoint = match kind {
        RemoteSearchKind::Code => "search/code",
        RemoteSearchKind::Repositories => "search/repositories",
        _ => "search/issues",
    };
    let q = match kind {
        RemoteSearchKind::PullRequests => "needle repo:example/repo is:pr",
        RemoteSearchKind::Issues => "needle repo:example/repo is:issue",
        _ => "needle repo:example/repo",
    };
    let mut args = api_args(endpoint);
    args.extend([
        "--header".into(),
        "Accept: application/vnd.github.text-match+json".into(),
        "-f".into(),
        format!("q={q}"),
        "-f".into(),
        format!("per_page={per_page}"),
        "-f".into(),
        format!("page={page}"),
    ]);
    step(
        args,
        json!({"total_count":total,"items":items,"incomplete_results":incomplete}),
    )
}
fn search(
    fixture: &Fixture,
    kind: RemoteSearchKind,
    limits: &RemoteSearchLimits,
) -> ghtui::providers::gh::GithubResult<ghtui::providers::gh::BoundedRemoteSearchResult> {
    provider(fixture).remote_search_bounded(
        &current(),
        &Query::parse("needle", SearchMode::GitHub).unwrap(),
        kind,
        &["example/repo".into()],
        limits,
        &CancellationToken::default(),
    )
}
#[test]
fn collaboration_index_remote_bounded_limits_scope_and_excluded_kinds_refuse_before_transport() {
    let fixture = Fixture::new();
    let kind = RemoteSearchKind::PullRequests;
    let valid = limits(kind, 2, 105, 4096);
    for invalid in [
        RemoteSearchLimits {
            max_pages: 0,
            ..valid.clone()
        },
        RemoteSearchLimits {
            max_pages: 11,
            ..valid.clone()
        },
        RemoteSearchLimits {
            max_results: 0,
            ..valid.clone()
        },
        RemoteSearchLimits {
            max_results: 1001,
            ..valid.clone()
        },
        RemoteSearchLimits {
            max_response_bytes: 0,
            ..valid.clone()
        },
        RemoteSearchLimits {
            max_response_bytes: 16 * 1024 * 1024 + 1,
            ..valid.clone()
        },
        RemoteSearchLimits {
            selected_kinds: vec![RemoteSearchKind::Code],
            ..valid.clone()
        },
        RemoteSearchLimits {
            selected_kinds: vec![kind, kind],
            ..valid.clone()
        },
        RemoteSearchLimits {
            max_duration: Duration::ZERO,
            ..valid.clone()
        },
    ] {
        assert!(search(&fixture, kind, &invalid).is_err());
    }
    for account_id in [None, Some(0)] {
        let account = HostIdentity {
            account_id,
            ..current()
        };
        assert!(provider(&fixture)
            .remote_search_bounded(
                &account,
                &Query::parse("needle", SearchMode::GitHub).unwrap(),
                kind,
                &["example/repo".into()],
                &valid,
                &CancellationToken::default()
            )
            .is_err());
    }
    for (input, mode, repos) in [
        ("needle", SearchMode::Regex, vec!["example/repo".into()]),
        (
            "needle repo:other/repo",
            SearchMode::GitHub,
            vec!["example/repo".into()],
        ),
        (
            "needle rev:main",
            SearchMode::GitHub,
            vec!["example/repo".into()],
        ),
        (
            "needle type:comment",
            SearchMode::GitHub,
            vec!["example/repo".into()],
        ),
        ("needle", SearchMode::GitHub, vec![]),
    ] {
        assert!(provider(&fixture)
            .remote_search_bounded(
                &current(),
                &Query::parse(input, mode).unwrap(),
                kind,
                &repos,
                &valid,
                &CancellationToken::default()
            )
            .is_err());
    }
    assert!(journal(&fixture).is_empty());
}
#[test]
fn collaboration_index_remote_bounded_result105_stops_before_second100_row_request() {
    let fixture = Fixture::new();
    let kind = RemoteSearchKind::PullRequests;
    let request = page(kind, 1, 100, 1001, (1..=100).map(raw_pr).collect(), false);
    let bytes = request.stdout.len();
    fixture.scripted_gh(&[request]);
    let policy = limits(kind, 10, 105, 1024 * 1024);
    let result = search(&fixture, kind, &policy).unwrap();
    assert_eq!(result.result.items.len(), 100);
    assert_eq!(result.provenance.len(), 100);
    assert_eq!(
        (
            result.requested_pages,
            result.fetched_pages,
            result.fetched_rows,
            result.response_bytes
        ),
        (1, 1, 100, bytes)
    );
    assert_eq!(
        result.original_query,
        Query::parse("needle", SearchMode::GitHub).unwrap()
    );
    assert_eq!(result.requested_repositories, vec!["example/repo"]);
    assert_eq!(result.reported_total, Some(1001));
    assert!(!result.result.coverage.complete);
    assert!(matches!(
        result.status,
        RemoteSearchStatus::Partial {
            failure: None,
            cancelled: false
        }
    ));
    assert!(result
        .result
        .coverage
        .reasons
        .iter()
        .any(|reason| reason.contains("before requesting")));
    for (item, proof) in result.result.items.iter().zip(&result.provenance) {
        assert!(proof.as_ref().unwrap().matches_item(&current(), item));
    }
    assert_eq!(journal(&fixture).len(), 1);
}
#[test]
fn collaboration_index_remote_bounded_page_cap_is_actual_and_moving_incomplete_counts_stay_partial()
{
    let fixture = Fixture::new();
    let kind = RemoteSearchKind::PullRequests;
    fixture.scripted_gh(&[
        page(kind, 1, 100, 999, (1..=100).map(raw_pr).collect(), false),
        page(kind, 2, 100, 1000, (101..=200).map(raw_pr).collect(), true),
    ]);
    let result = search(&fixture, kind, &limits(kind, 2, 1000, 1024 * 1024)).unwrap();
    assert_eq!(result.result.items.len(), 200);
    assert_eq!(result.provenance.len(), 200);
    assert_eq!(
        (
            result.requested_pages,
            result.fetched_pages,
            result.fetched_rows
        ),
        (2, 2, 200)
    );
    assert_eq!(result.reported_total, Some(999));
    assert!(!result.result.coverage.complete);
    assert!(result
        .result
        .coverage
        .reasons
        .iter()
        .any(|reason| reason.contains("changed while paging")));
    assert!(result
        .result
        .coverage
        .reasons
        .iter()
        .any(|reason| reason.contains("incomplete_results")));
    assert_eq!(journal(&fixture).len(), 2);
}
#[test]
fn collaboration_index_remote_bounded_small_result_cap_sets_request_width_and_rejects_extra_rows() {
    let fixture = Fixture::new();
    let kind = RemoteSearchKind::PullRequests;
    fixture.scripted_gh(&[page(kind, 1, 3, 10, (1..=3).map(raw_pr).collect(), false)]);
    let result = search(&fixture, kind, &limits(kind, 10, 3, 8192)).unwrap();
    assert_eq!(result.result.items.len(), 3);
    assert_eq!(result.requested_pages, 1);
    assert!(!result.result.coverage.complete);
    let malformed = Fixture::new();
    malformed.scripted_gh(&[page(kind, 1, 3, 4, (1..=4).map(raw_pr).collect(), false)]);
    assert!(matches!(
        search(&malformed, kind, &limits(kind, 10, 3, 8192))
            .unwrap_err()
            .kind,
        GithubErrorKind::Json
    ));
    assert_eq!(journal(&malformed).len(), 1);
}
#[test]
fn collaboration_index_remote_bounded_aggregate_byte_cap_kills_response_before_parse_or_next_page()
{
    let fixture = Fixture::new();
    let kind = RemoteSearchKind::PullRequests;
    let request = page(kind, 1, 100, 1000, (1..=100).map(raw_pr).collect(), false);
    let bytes = request.stdout.len();
    let second = page(kind, 2, 100, 1000, (101..=200).map(raw_pr).collect(), false);
    fixture.scripted_gh(&[request, second]);
    let result = search(&fixture, kind, &limits(kind, 10, 1000, bytes + 1)).unwrap();
    assert_eq!(result.result.items.len(), 100);
    assert_eq!(result.provenance.len(), 100);
    assert_eq!(
        (
            result.requested_pages,
            result.fetched_pages,
            result.fetched_rows,
            result.response_bytes
        ),
        (2, 1, 100, bytes)
    );
    assert!(
        matches!(result.status,RemoteSearchStatus::Partial{failure:Some(ref failure),cancelled:false}if matches!(failure.kind,GithubErrorKind::Unsupported))
    );
    assert_eq!(journal(&fixture).len(), 2);
    let first = Fixture::new();
    first.scripted_gh(&[page(
        kind,
        1,
        100,
        1000,
        (1..=100).map(raw_pr).collect(),
        false,
    )]);
    let unavailable = search(&first, kind, &limits(kind, 10, 1000, 1)).unwrap();
    assert!(unavailable.result.items.is_empty() && unavailable.provenance.is_empty());
    assert_eq!(unavailable.reported_total, None);
    assert_eq!(unavailable.fetched_pages, 0);
    assert!(!unavailable.result.coverage.complete);
    assert!(matches!(
        unavailable.status,
        RemoteSearchStatus::Unavailable { .. }
    ));
    assert_eq!(journal(&first).len(), 1);
}
#[test]
fn collaboration_index_remote_bounded_rate_failure_retains_only_prior_scope_and_auth_never_retains_private_pages(
) {
    for (stderr, fatal) in [
        ("HTTP 429: rate limit exceeded", false),
        ("offline connection reset", false),
        ("HTTP 401: unauthorized", true),
        ("HTTP 403: forbidden", true),
    ] {
        let fixture = Fixture::new();
        let kind = RemoteSearchKind::PullRequests;
        let mut failed = page(kind, 2, 100, 1000, vec![], false);
        failed.status = 1;
        failed.stderr = stderr.as_bytes().to_vec();
        fixture.scripted_gh(&[
            page(kind, 1, 100, 1000, (1..=100).map(raw_pr).collect(), false),
            failed,
        ]);
        let result = search(&fixture, kind, &limits(kind, 10, 1000, 1024 * 1024));
        if fatal {
            assert!(result.is_err());
        } else {
            let result = result.unwrap();
            assert_eq!(result.result.items.len(), 100);
            assert_eq!(result.requested_pages, 2);
            assert!(!result.result.coverage.complete);
            assert!(matches!(
                result.status,
                RemoteSearchStatus::Partial {
                    failure: Some(_),
                    cancelled: false
                }
            ));
        }
        assert_eq!(journal(&fixture).len(), 2);
    }
    let fixture = Fixture::new();
    let cancelled = CancellationToken::default();
    cancelled.cancel();
    let result = provider(&fixture)
        .remote_search_bounded(
            &current(),
            &Query::parse("needle", SearchMode::GitHub).unwrap(),
            RemoteSearchKind::Code,
            &["example/repo".into()],
            &limits(RemoteSearchKind::Code, 2, 100, 8192),
            &cancelled,
        )
        .unwrap();
    assert_eq!(result.requested_pages, 0);
    assert_eq!(result.reported_total, None);
    assert!(!result.result.coverage.complete);
    assert!(matches!(
        result.status,
        RemoteSearchStatus::Unavailable {
            cancelled: true,
            ..
        }
    ));
    assert!(journal(&fixture).is_empty());
}
#[test]
fn collaboration_index_remote_bounded_opaque_numeric_same_display_ids_remain_position_aligned_distinct(
) {
    let fixture = Fixture::new();
    let kind = RemoteSearchKind::PullRequests;
    let mut opaque = raw_pr(7);
    opaque["id"] = json!(700);
    opaque["node_id"] = json!("700");
    let mut numeric = raw_pr(8);
    numeric["id"] = json!(700);
    numeric.as_object_mut().unwrap().remove("node_id");
    fixture.scripted_gh(&[page(kind, 1, 100, 2, vec![opaque, numeric], false)]);
    let result = search(&fixture, kind, &limits(kind, 2, 100, 8192)).unwrap();
    assert_eq!(result.result.items.len(), 2);
    assert_eq!(result.result.items[0].id, result.result.items[1].id);
    assert!(
        matches!(result.provenance[0].as_ref().unwrap().identity(),RemoteCanonicalIdentity::OpaqueNode{node_id,..}if node_id=="700")
    );
    assert!(matches!(
        result.provenance[1].as_ref().unwrap().identity(),
        RemoteCanonicalIdentity::Numeric {
            database_id: 700,
            ..
        }
    ));
    assert_eq!(
        result.provenance[0].as_ref().unwrap().collaboration_node(),
        Some("700")
    );
    assert!(!result.provenance[0]
        .as_ref()
        .unwrap()
        .matches_item(&current(), &result.result.items[1]));
    assert!(!result.provenance[1]
        .as_ref()
        .unwrap()
        .matches_item(&current(), &result.result.items[0]));
    assert!(result.provenance[1]
        .as_ref()
        .unwrap()
        .collaboration_node()
        .is_none());
    assert!(result.result.coverage.complete);
    assert!(matches!(result.status, RemoteSearchStatus::Complete));
    assert_eq!(journal(&fixture).len(), 1);
}
#[test]
fn collaboration_index_remote_bounded_raw_blob_keys_preserve_case_and_conflicting_parent_metadata_fails_closed(
) {
    let fixture = Fixture::new();
    let kind = RemoteSearchKind::Code;
    let code = |path: &str| json!({"name":"Main.rs","path":path,"sha":"A".repeat(40),"repository":{"id":99,"full_name":"example/repo"}});
    fixture.scripted_gh(&[page(
        kind,
        1,
        100,
        2,
        vec![code("src/Main.rs"), code("src/main.rs")],
        false,
    )]);
    let result = search(&fixture, kind, &limits(kind, 2, 100, 8192)).unwrap();
    assert_eq!(result.result.items.len(), 2);
    assert!(
        matches!(result.provenance[0].as_ref().unwrap().identity(),RemoteCanonicalIdentity::CodeBlob{path,blob_sha,repository_id:Some(99)}if path=="src/Main.rs"&&blob_sha==&"a".repeat(40))
    );
    for change in [
        "id",
        "repository",
        "number",
        "repository_url",
        "repository_name",
        "repository_url_api_alias",
    ] {
        let fixture = Fixture::new();
        let original = raw_pr(7);
        let mut changed = original.clone();
        match change {
            "id" => changed["id"] = json!(2000),
            "repository" => changed["repository"]["id"] = json!(100),
            "repository_name" => changed["repository"]["full_name"] = json!("other/repo"),
            "repository_url_api_alias" => {
                changed["repository_url"] = json!("https://api.github.invalid/repos/example/repo")
            }
            "number" => {
                changed["number"] = json!(8);
                changed["html_url"] = json!("https://github.invalid/example/repo/pull/8");
            }
            _ => changed["repository_url"] = json!("https://evil.invalid/repos/example/repo"),
        }
        fixture.scripted_gh(&[page(
            RemoteSearchKind::PullRequests,
            1,
            100,
            2,
            vec![original, changed],
            false,
        )]);
        assert!(matches!(
            search(
                &fixture,
                RemoteSearchKind::PullRequests,
                &limits(RemoteSearchKind::PullRequests, 2, 100, 8192)
            )
            .unwrap_err()
            .kind,
            GithubErrorKind::IdentityChanged
        ));
        assert_eq!(journal(&fixture).len(), 1);
    }
    // Only the actual documented Cloud/residency API authority differs from
    // the selected web host. Generic GHES never guesses an api. alias.
    for (host, api_host) in [
        ("github.com", "api.github.com"),
        ("acme.ghe.com", "api.acme.ghe.com"),
    ] {
        let fixture = Fixture::new();
        let mut canonical = raw_pr(7);
        canonical["repository_url"] = json!(format!("https://{api_host}/repos/example/repo"));
        canonical["html_url"] = json!(format!("https://{host}/example/repo/pull/7"));
        let mut request = page(
            RemoteSearchKind::PullRequests,
            1,
            100,
            1,
            vec![canonical],
            false,
        );
        request.args[3] = host.into();
        fixture.scripted_gh(&[request]);
        let account = HostIdentity {
            host: host.into(),
            ..current()
        };
        let gh = GithubProvider::new(host)
            .unwrap()
            .with_program(fixture.root.join("bin/gh"))
            .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"));
        let result = gh
            .remote_search_bounded(
                &account,
                &Query::parse("needle", SearchMode::GitHub).unwrap(),
                RemoteSearchKind::PullRequests,
                &["example/repo".into()],
                &limits(RemoteSearchKind::PullRequests, 2, 100, 8192),
                &CancellationToken::default(),
            )
            .unwrap();
        assert!(matches!(result.status, RemoteSearchStatus::Complete));
        assert!(result.provenance[0]
            .as_ref()
            .unwrap()
            .matches_item(&account, &result.result.items[0]));
        assert_eq!(journal(&fixture).len(), 1);
    }
}
