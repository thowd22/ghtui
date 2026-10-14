//! SOURCE-ONLY strict current thread evidence contracts. Registration is a later lease.
use super::*;
use ghtui::providers::gh::{
    MetricsPrObservation, MetricsReadStatus, MetricsThreadLimits, MetricsThreadReplyTo,
    METRICS_REVIEW_THREADS_QUERY, METRICS_THREAD_COMMENTS_QUERY,
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
fn parent() -> Value {
    json!({"id":101,"node_id":"PR_1","number":1,"title":"Original PR","state":"open","draft":false,"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-12T00:00:00Z","merged_at":null,"closed_at":null,"merged":false,"head":{"sha":"a".repeat(40)},"base":{"repo":repo()},"user":{"id":51,"login":"alice"},"requested_reviewers":[],"requested_teams":[]})
}
fn graph_repo() -> Value {
    json!({"id":"R_99","databaseId":99,"nameWithOwner":"example/repo"})
}
fn graph_parent() -> Value {
    json!({"__typename":"PullRequest","id":"PR_1","fullDatabaseId":"101","number":1,"headRefOid":"a".repeat(40),"repository":graph_repo()})
}
fn actor(id: u64, login: &str) -> Value {
    json!({"__typename":"User","databaseId":id,"login":login,"name":login,"avatarUrl":format!("https://avatars.invalid/{id}")})
}
fn connection(rows: Vec<Value>, total: u64, cursor: Option<&str>) -> Value {
    json!({"nodes":rows,"totalCount":total,"pageInfo":{"hasNextPage":cursor.is_some(),"endCursor":cursor}})
}
fn comment(id: &str, root: bool) -> Value {
    json!({"id":id,"fullDatabaseId":if root {u64::MAX.to_string()} else {(u64::MAX-1).to_string()},"body":"Raw terminal bytes \u{1b}[31m are data","createdAt":"2026-10-02T00:00:00Z","updatedAt":"2026-10-03T00:00:00Z","path":"old\tname\nfile.rs","line":20,"startLine":19,"originalLine":7,"originalStartLine":6,"originalCommit":{"oid":"b".repeat(40)},"commit":{"oid":"c".repeat(40)},"replyTo":if root {Value::Null} else {json!({"id":"C_root"})},"author":actor(if root {51} else {52},if root {"alice"} else {"bob"}),"pullRequest":graph_parent(),"repository":graph_repo()})
}
fn thread(id: &str, comments: Value) -> Value {
    json!({"__typename":"PullRequestReviewThread","id":id,"isResolved":false,"isOutdated":true,"path":"new/path.rs","diffSide":"RIGHT","startDiffSide":"RIGHT","line":20,"startLine":19,"originalLine":7,"originalStartLine":6,"viewerCanReply":true,"viewerCanResolve":true,"viewerCanUnresolve":false,"resolvedBy":null,"pullRequest":graph_parent(),"repository":graph_repo(),"comments":comments})
}
fn outer(rows: Vec<Value>, total: u64, cursor: Option<&str>) -> Value {
    let mut parent = graph_parent();
    parent["reviewThreads"] = connection(rows, total, cursor);
    json!({"data":{"node":parent}})
}
fn get(endpoint: &str, value: Value) -> GhStep {
    step(api_args(endpoint), value)
}
fn prefix() -> Vec<GhStep> {
    vec![
        get("user", json!({"id":42,"login":"octocat"})),
        get("repos/example/repo", repo()),
        get("repos/example/repo/pulls/1", parent()),
    ]
}
fn suffix() -> Vec<GhStep> {
    vec![
        get("repos/example/repo", repo()),
        get("repos/example/repo/pulls/1", parent()),
        get("user", json!({"id":42,"login":"octocat"})),
    ]
}
fn outer_step(
    value: Value,
    limits: &MetricsThreadLimits,
    after: Option<&str>,
    include: bool,
) -> GhStep {
    graphql_step(
        METRICS_REVIEW_THREADS_QUERY,
        json!({"id":"PR_1","first":limits.thread_page_size,"after":after,"commentsFirst":limits.comment_page_size,"includeComments":include}),
        value,
    )
}
fn initial() -> Vec<GhStep> {
    vec![
        get("repos/example/repo", repo()),
        get(
            "repos/example/repo/pulls?state=all&sort=created&direction=desc&per_page=100&page=1",
            json!([parent()]),
        ),
        get("repos/example/repo", repo()),
    ]
}
fn prepare(fixture: &Fixture, reads: Vec<GhStep>) -> (GithubProvider, MetricsPrObservation) {
    let mut steps = initial();
    steps.extend(reads);
    fixture.scripted_gh(&steps);
    let provider = provider(fixture);
    let parent = provider
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
    (provider, parent)
}
fn complete(value: Value, limits: &MetricsThreadLimits) -> Vec<GhStep> {
    let mut steps = prefix();
    steps.push(outer_step(value, limits, None, true));
    steps.extend(suffix());
    steps
}
fn assert_reads(fixture: &Fixture, count: usize) {
    let records = journal(fixture);
    assert_eq!(records.len(), count);
    assert!(records.iter().all(|record| record["matched"] == true));
    for record in records {
        if record["args"][1] == "graphql" {
            assert_eq!(record["args"][5], "POST");
            assert_eq!(record["args"][6], "--input");
            let hex = record["stdin_hex"].as_str().unwrap();
            let bytes = (0..hex.len())
                .step_by(2)
                .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap())
                .collect::<Vec<_>>();
            let body: Value = serde_json::from_slice(&bytes).unwrap();
            assert!(body["query"].as_str().unwrap().starts_with("query("));
            assert!(!body["query"].as_str().unwrap().contains("mutation"));
            assert!((1..=100).contains(&body["variables"]["first"].as_u64().unwrap()));
        } else {
            assert_eq!(record["args"][5], "GET");
            assert_eq!(record["stdin_hex"], "");
        }
    }
}

#[test]
fn metrics_threads_provider_exact_seven_reads_keep_original_root_and_actor_ids_not_array_order() {
    let fixture = Fixture::new();
    let limits = MetricsThreadLimits::default();
    let (provider, parent) = prepare(
        &fixture,
        complete(
            outer(
                vec![thread(
                    "T_1",
                    connection(
                        vec![comment("C_reply", false), comment("C_root", true)],
                        2,
                        None,
                    ),
                )],
                1,
                None,
            ),
            &limits,
        ),
    );
    let result = provider
        .metrics_review_threads(&current(), &parent, &limits, &CancellationToken::default())
        .unwrap();
    assert_eq!(result.requests, 7);
    assert_eq!(result.graphql_pages, 1);
    assert_eq!(result.raw_comments, 2);
    assert!(
        result.state_coverage.enumeration_complete
            && result.state_coverage.metric_evidence_complete
    );
    let thread = &result.threads[0];
    let original = thread.original_comment.as_ref().unwrap();
    assert_eq!(original.comment_node_id, "C_root");
    assert_eq!(original.original_line, Some(7));
    assert_eq!(original.original_start_line, Some(6));
    assert_eq!(
        original.original_commit_sha.as_deref(),
        Some("b".repeat(40).as_str())
    );
    assert_eq!(
        original.observed_path.as_deref(),
        Some("old\tname\nfile.rs")
    );
    assert_eq!(
        thread.placement.observed_path.as_deref(),
        Some("new/path.rs")
    );
    assert_eq!(
        thread.comments[0].author.as_ref().unwrap().account_id,
        Some(52)
    );
    assert_eq!(
        thread.comments[1].author.as_ref().unwrap().account_id,
        Some(51)
    );
    assert_eq!(thread.comments[1].database_id, Some(u64::MAX));
    assert!(thread.comments[0].body.as_ref().unwrap().contains('\u{1b}'));
    assert!(
        !result
            .provenance_coverage
            .original_tree_verification_available
    );
    assert!(result.provenance_coverage.original_candidates_complete);
    assert_eq!(result.parent, parent);
    assert_reads(&fixture, 10);
}

#[test]
fn metrics_threads_provider_control_only_diagnostic_capture_is_not_sanitized_away_from_budget() {
    let fixture = Fixture::new();
    let limits = MetricsThreadLimits::default();
    let mut calls = complete(outer(vec![], 0, None), &limits);
    let diagnostic = b"\x1b[31m\x1b[0m\x01\x02".to_vec();
    calls[0].stderr = diagnostic.clone();
    let bytes: usize = calls.iter().map(|step| step.stdout.len()).sum();
    let (gh, parent) = prepare(&fixture, calls);
    let source = gh
        .metrics_review_threads(&current(), &parent, &limits, &CancellationToken::default())
        .unwrap();
    assert_eq!(source.aggregate_response_bytes, bytes);
    assert_eq!(source.successful_diagnostic_bytes, diagnostic.len());
    assert_eq!(source.failed_reserved_bytes, 0);
    assert!(source.state_coverage.complete_empty);
    assert_reads(&fixture, 10);

    let fixture = Fixture::new();
    let mut first = prefix().remove(0);
    first.stderr = "\x1b[31m\x1b[0m".repeat(20).into_bytes();
    let cap = first.stdout.len() + first.stderr.len();
    let (gh, parent) = prepare(&fixture, vec![first]);
    assert!(gh
        .metrics_review_threads(
            &current(),
            &parent,
            &MetricsThreadLimits {
                max_response_bytes: cap,
                ..limits
            },
            &CancellationToken::default()
        )
        .is_err());
    assert_reads(&fixture, 4);
}
#[test]
fn metrics_threads_provider_nullable_state_actor_anchor_and_reply_fields_never_become_false_or_original(
) {
    let fixture = Fixture::new();
    let limits = MetricsThreadLimits::default();
    let mut raw_comment = comment("C_unknown", true);
    for field in [
        "originalLine",
        "originalStartLine",
        "originalCommit",
        "author",
        "body",
    ] {
        raw_comment[field] = Value::Null;
    }
    raw_comment.as_object_mut().unwrap().remove("replyTo");
    let mut raw = thread("T_unknown", connection(vec![raw_comment], 1, None));
    raw["isResolved"] = Value::Null;
    raw.as_object_mut().unwrap().remove("isOutdated");
    let (provider, parent) = prepare(&fixture, complete(outer(vec![raw], 1, None), &limits));
    let result = provider
        .metrics_review_threads(&current(), &parent, &limits, &CancellationToken::default())
        .unwrap();
    assert!(result.state_coverage.enumeration_complete);
    assert!(!result.state_coverage.metric_evidence_complete);
    let thread = &result.threads[0];
    assert_eq!((thread.resolved, thread.outdated), (None, None));
    assert!(thread.original_comment.is_none());
    assert!(matches!(
        thread.comments[0].reply_to,
        MetricsThreadReplyTo::Unknown
    ));
    assert!(
        thread.comments[0].original_line.is_none()
            && thread.comments[0].original_commit_sha.is_none()
    );
    assert!(
        thread.comments[0].author.is_none()
            && thread.comments[0].body.is_none()
            && thread.resolved_by.is_none()
    );
    assert!(
        !result.provenance_coverage.actors_complete
            && !result.provenance_coverage.content_complete
            && !result.provenance_coverage.original_candidates_complete
    );
    assert_reads(&fixture, 10);
}
#[test]
fn metrics_threads_provider_actual_empty_outer_and_empty_nested_proofs_are_distinct() {
    for nested in [false, true] {
        let fixture = Fixture::new();
        let limits = MetricsThreadLimits::default();
        let value = if nested {
            outer(
                vec![thread("T_empty", connection(vec![], 0, None))],
                1,
                None,
            )
        } else {
            outer(vec![], 0, None)
        };
        let (provider, parent) = prepare(&fixture, complete(value, &limits));
        let result = provider
            .metrics_review_threads(&current(), &parent, &limits, &CancellationToken::default())
            .unwrap();
        assert!(
            result.state_coverage.enumeration_complete
                && result.state_coverage.metric_evidence_complete
        );
        assert_eq!(result.state_coverage.complete_empty, !nested);
        if nested {
            assert!(result.threads[0].comment_coverage.complete_empty);
            assert!(result.threads[0].original_comment.is_none());
        }
        assert_reads(&fixture, 10);
    }
}
#[test]
fn metrics_threads_provider_outer_and_nested_caps_do_not_trade_state_for_fake_zero_comments() {
    for variation in 0..3 {
        let fixture = Fixture::new();
        let limits = MetricsThreadLimits {
            thread_page_size: if variation == 2 { 2 } else { 1 },
            max_threads: if variation == 0 { 1 } else { 10 },
            comment_page_size: 1,
            max_comments: if variation == 2 { 1 } else { 10 },
            max_comment_pages_per_thread: 1,
            ..Default::default()
        };
        let mut rows = vec![thread(
            "T_1",
            connection(vec![comment("C_root", true)], 2, Some("nextComment")),
        )];
        if variation == 2 {
            rows[0].as_object_mut().unwrap().remove("comments");
            let mut row = thread("T_2", Value::Null);
            row.as_object_mut().unwrap().remove("comments");
            rows.push(row);
        }
        let mut reads = prefix();
        reads.push(outer_step(
            outer(
                rows,
                2,
                if variation == 0 {
                    Some("nextThread")
                } else {
                    None
                },
            ),
            &limits,
            None,
            variation != 2,
        ));
        reads.extend(suffix());
        let (provider, parent) = prepare(&fixture, reads);
        let result = provider
            .metrics_review_threads(&current(), &parent, &limits, &CancellationToken::default())
            .unwrap();
        assert_eq!(result.graphql_pages, 1);
        assert_eq!(result.threads.len(), if variation == 2 { 2 } else { 1 });
        assert_eq!(result.state_coverage.enumeration_complete, variation == 2);
        assert!(!result.threads[0].comment_coverage.enumeration_complete);
        assert!(!result.threads[0].comment_coverage.complete_empty);
        assert!(!result.provenance_coverage.comment_enumeration_complete);
        if variation == 2 {
            assert_eq!(result.raw_comments, 0);
            assert!(result.threads[0].comments.is_empty());
        }
        assert_reads(&fixture, 10);
    }
}
#[test]
fn metrics_threads_provider_nested_cursor_fetches_real_original_comment_without_current_commit_substitution(
) {
    let fixture = Fixture::new();
    let limits = MetricsThreadLimits {
        thread_page_size: 1,
        comment_page_size: 1,
        ..Default::default()
    };
    let mut reads = prefix();
    reads.push(outer_step(
        outer(
            vec![thread(
                "T_1",
                connection(vec![comment("C_reply", false)], 2, Some("cursor1")),
            )],
            1,
            None,
        ),
        &limits,
        None,
        true,
    ));
    reads.push(graphql_step(
        METRICS_THREAD_COMMENTS_QUERY,
        json!({"id":"T_1","first":1,"after":"cursor1"}),
        json!({"data":{"node":thread("T_1",connection(vec![comment("C_root",true)],2,None))}}),
    ));
    reads.extend(suffix());
    let (provider, parent) = prepare(&fixture, reads);
    let result = provider
        .metrics_review_threads(&current(), &parent, &limits, &CancellationToken::default())
        .unwrap();
    assert_eq!(result.requests, 8);
    assert_eq!(result.graphql_pages, 2);
    assert!(result.threads[0].comment_coverage.enumeration_complete);
    assert_eq!(result.threads[0].comment_coverage.fetched_rows, 2);
    assert_eq!(
        result.threads[0]
            .original_comment
            .as_ref()
            .unwrap()
            .comment_node_id,
        "C_root"
    );
    assert_reads(&fixture, 11);
}
#[test]
fn metrics_threads_provider_canonical_repo_parent_head_and_uid_drift_refuse_private_evidence() {
    for variation in 0..8 {
        let fixture = Fixture::new();
        let limits = MetricsThreadLimits::default();
        let mut reads = complete(outer(vec![], 0, None), &limits);
        let at = match variation {
            0 => 0,
            1 => 1,
            2 | 3 => 2,
            4 => 4,
            5 | 6 => 5,
            _ => 6,
        };
        let mut raw = match at {
            0 | 6 => json!({"id":42,"login":"octocat"}),
            1 | 4 => repo(),
            _ => parent(),
        };
        match variation {
            0 | 7 => raw["id"] = json!(43),
            1 | 4 => raw["id"] = json!(100),
            2 | 5 => raw["node_id"] = json!("PR_replacement"),
            _ => raw["head"]["sha"] = json!("d".repeat(40)),
        };
        reads[at].stdout = serde_json::to_vec(&raw).unwrap();
        reads.truncate(at + 1);
        let (provider, parent) = prepare(&fixture, reads);
        let err = provider
            .metrics_review_threads(&current(), &parent, &limits, &CancellationToken::default())
            .unwrap_err();
        assert!(matches!(
            err.kind,
            GithubErrorKind::IdentityChanged | GithubErrorKind::StaleHead
        ));
        assert_reads(&fixture, 3 + at + 1);
    }
}
#[test]
fn metrics_threads_provider_graphql_parent_kind_repository_and_comment_identity_are_exact() {
    for variation in 0..6 {
        let fixture = Fixture::new();
        let limits = MetricsThreadLimits::default();
        let mut value = outer(
            vec![thread(
                "T_1",
                connection(vec![comment("C_root", true)], 1, None),
            )],
            1,
            None,
        );
        match variation {
            0 => value["data"]["node"]["__typename"] = json!("Issue"),
            1 => value["data"]["node"]["fullDatabaseId"] = json!("102"),
            2 => value["data"]["node"]["headRefOid"] = json!("a".repeat(7)),
            3 => {
                value["data"]["node"]["reviewThreads"]["nodes"][0]["repository"]["databaseId"] =
                    json!(100)
            }
            4 => {
                value["data"]["node"]["reviewThreads"]["nodes"][0]["comments"]["nodes"][0]
                    ["pullRequest"]["id"] = json!("PR_other")
            }
            _ => {
                value["data"]["node"]["reviewThreads"]["nodes"][0]["comments"]["nodes"][0]["id"] =
                    json!("")
            }
        };
        let mut reads = prefix();
        reads.push(outer_step(value, &limits, None, true));
        let (provider, parent) = prepare(&fixture, reads);
        assert!(matches!(
            provider
                .metrics_review_threads(&current(), &parent, &limits, &CancellationToken::default())
                .unwrap_err()
                .kind,
            GithubErrorKind::IdentityChanged | GithubErrorKind::StaleHead | GithubErrorKind::Json
        ));
        assert_reads(&fixture, 7);
    }
}
#[test]
fn metrics_threads_provider_duplicate_nodes_and_contradictory_totals_cannot_prove_complete() {
    for variation in 0..4 {
        let fixture = Fixture::new();
        let limits = MetricsThreadLimits::default();
        let mut rows = vec![thread(
            "T_1",
            connection(vec![comment("C_root", true)], 1, None),
        )];
        match variation {
            0 => rows.push(rows[0].clone()),
            1 => {
                rows[0]["comments"] = connection(
                    vec![comment("C_root", true), comment("C_root", true)],
                    2,
                    None,
                )
            }
            2 => {
                rows[0]["comments"] = connection(
                    vec![comment("C_root", true), comment("C_otherroot", true)],
                    2,
                    None,
                )
            }
            _ => {}
        };
        let total = if variation == 0 {
            2
        } else if variation == 3 {
            2
        } else {
            1
        };
        let (provider, parent) = prepare(&fixture, complete(outer(rows, total, None), &limits));
        let result = provider.metrics_review_threads(
            &current(),
            &parent,
            &limits,
            &CancellationToken::default(),
        );
        if variation == 3 {
            let snapshot = result.unwrap();
            assert!(
                !snapshot.state_coverage.enumeration_complete
                    && !snapshot.state_coverage.metric_evidence_complete
            );
            assert_reads(&fixture, 10);
        } else {
            assert!(matches!(
                result.unwrap_err().kind,
                GithubErrorKind::IdentityChanged
            ));
            assert_reads(&fixture, 7);
        }
    }
}
#[test]
fn metrics_threads_provider_document_cap_excludes_content_but_preserves_actual_outer_states() {
    let fixture = Fixture::new();
    let limits = MetricsThreadLimits {
        max_document_bytes: 1024,
        ..Default::default()
    };
    let mut raw = comment("C_large", true);
    raw["body"] = json!("s".repeat(2000));
    let (provider, parent) = prepare(
        &fixture,
        complete(
            outer(vec![thread("T_1", connection(vec![raw], 1, None))], 1, None),
            &limits,
        ),
    );
    let snapshot = provider
        .metrics_review_threads(&current(), &parent, &limits, &CancellationToken::default())
        .unwrap();
    assert!(
        snapshot.state_coverage.enumeration_complete
            && snapshot.state_coverage.metric_evidence_complete
    );
    assert_eq!(
        (snapshot.threads[0].resolved, snapshot.threads[0].outdated),
        (Some(false), Some(true))
    );
    assert_eq!(snapshot.threads[0].comment_coverage.excluded_rows, 1);
    assert!(!snapshot.threads[0].comment_coverage.complete_empty);
    assert!(snapshot.threads[0].comments.is_empty());
    assert!(!snapshot.provenance_coverage.content_complete);
    assert_reads(&fixture, 10);
}
#[test]
fn metrics_threads_provider_invalid_principal_full_head_limits_cancel_and_bytes_stop_before_more_reads(
) {
    let fixture = Fixture::new();
    let (provider, parent) = prepare(&fixture, vec![]);
    for uid in [None, Some(0)] {
        assert!(provider
            .metrics_review_threads(
                &HostIdentity {
                    account_id: uid,
                    ..current()
                },
                &parent,
                &Default::default(),
                &CancellationToken::default()
            )
            .is_err());
    }
    for head in [
        None,
        Some("a".repeat(7)),
        Some("a".repeat(33)),
        Some("z".repeat(40)),
    ] {
        let mut invalid = parent.clone();
        invalid.head_sha = head;
        assert!(provider
            .metrics_review_threads(
                &current(),
                &invalid,
                &Default::default(),
                &CancellationToken::default()
            )
            .is_err());
    }
    assert!(provider
        .metrics_review_threads(
            &current(),
            &parent,
            &MetricsThreadLimits {
                max_graphql_pages: 0,
                ..Default::default()
            },
            &CancellationToken::default()
        )
        .is_err());
    let cancelled = CancellationToken::default();
    cancelled.cancel();
    assert!(provider
        .metrics_review_threads(&current(), &parent, &Default::default(), &cancelled)
        .is_err());
    assert_reads(&fixture, 3);
    let fixture = Fixture::new();
    let (provider, parent) = prepare(
        &fixture,
        vec![get("user", json!({"id":42,"login":"octocat"}))],
    );
    assert!(provider
        .metrics_review_threads(
            &current(),
            &parent,
            &MetricsThreadLimits {
                max_response_bytes: 1,
                ..Default::default()
            },
            &CancellationToken::default()
        )
        .is_err());
    assert_reads(&fixture, 4);
}
#[test]
fn metrics_threads_provider_graphql_errors_and_nonutf8_json_are_unavailable_not_complete_empty() {
    for variation in 0..3 {
        let fixture = Fixture::new();
        let limits = MetricsThreadLimits::default();
        let mut request = outer_step(outer(vec![], 0, None), &limits, None, true);
        match variation {
            0 => {
                request.stdout = serde_json::to_vec(
                    &json!({"errors":[{"message":"Unavailable"}],"data":{"node":null}}),
                )
                .unwrap()
            }
            1 => request.stdout = vec![b'{', b'"', 0xff, b'"', b'}'],
            _ => {
                request.status = 1;
                request.stderr = b"HTTP 403: inaccessible repository".to_vec();
            }
        }
        let mut reads = prefix();
        reads.push(request);
        if variation != 2 {
            reads.extend(suffix());
        }
        let (provider, parent) = prepare(&fixture, reads);
        let result = provider.metrics_review_threads(
            &current(),
            &parent,
            &limits,
            &CancellationToken::default(),
        );
        if variation == 2 {
            assert!(matches!(
                result.unwrap_err().kind,
                GithubErrorKind::Permission
            ));
            assert_reads(&fixture, 7);
        } else {
            let snapshot = result.unwrap();
            assert!(matches!(
                snapshot.state_coverage.status,
                MetricsReadStatus::Unavailable(_)
            ));
            assert!(
                !snapshot.state_coverage.enumeration_complete
                    && !snapshot.state_coverage.complete_empty
            );
            assert_reads(&fixture, 10);
        }
    }
}
#[test]
fn metrics_threads_provider_ranges_paths_bigints_and_deleted_actors_remain_nullable_candidates() {
    for variation in 0..9 {
        let fixture = Fixture::new();
        let limits = MetricsThreadLimits::default();
        let mut raw = comment("C_root", true);
        match variation {
            0 => {
                raw["originalCommit"]["oid"] = json!("a".repeat(7));
                raw["originalLine"] = json!(-1);
            }
            1 => {
                raw["originalStartLine"] = json!(8);
                raw["originalLine"] = json!(7);
            }
            2 => raw["path"] = json!("../escape"),
            3 => {
                raw["fullDatabaseId"] = json!("18446744073709551616");
                raw["author"] = Value::Null;
            }
            4 => {
                raw["fullDatabaseId"] = json!("0001");
                raw["originalCommit"]["oid"] = json!("b".repeat(64));
            }
            5 => raw["path"] = json!("/absolute"),
            6 => raw["path"] = json!("src//file"),
            7 => raw["path"] = json!("bad\0path"),
            _ => raw["path"] = json!("x".repeat(4097)),
        };
        let mut thread = thread("T_1", connection(vec![raw], 1, None));
        thread["isResolved"] = json!(true);
        thread["resolvedBy"] = Value::Null;
        let (provider, parent) = prepare(&fixture, complete(outer(vec![thread], 1, None), &limits));
        let snapshot = provider
            .metrics_review_threads(&current(), &parent, &limits, &CancellationToken::default())
            .unwrap();
        assert!(
            snapshot.state_coverage.enumeration_complete
                && snapshot.state_coverage.metric_evidence_complete
        );
        assert!(!snapshot.provenance_coverage.actors_complete);
        let comment = &snapshot.threads[0].comments[0];
        match variation {
            0 => assert!(comment.original_commit_sha.is_none() && comment.original_line.is_none()),
            1 => assert!(comment.original_line.is_none() && comment.original_start_line.is_none()),
            2 => assert!(comment.observed_path.is_none()),
            3 => assert!(comment.database_id.is_none() && comment.author.is_none()),
            4 => {
                assert!(comment.database_id.is_none());
                assert_eq!(comment.original_commit_sha.as_ref().unwrap().len(), 64);
            }
            _ => assert!(comment.observed_path.is_none()),
        };
        assert!(snapshot.threads[0].resolved_by.is_none());
        assert!(
            !snapshot
                .provenance_coverage
                .original_tree_verification_available
        );
        assert_reads(&fixture, 10);
    }
}

#[test]
fn metrics_threads_provider_outer_cursor_paging_keeps_exact_scope_and_distinct_real_nodes() {
    let fixture = Fixture::new();
    let limits = MetricsThreadLimits {
        thread_page_size: 1,
        comment_page_size: 1,
        ..Default::default()
    };
    let mut second_comment = comment("C_root2", true);
    second_comment["fullDatabaseId"] = json!(201);
    let mut reads = prefix();
    reads.push(outer_step(
        outer(
            vec![thread(
                "T_1",
                connection(vec![comment("C_root", true)], 1, None),
            )],
            2,
            Some("outer1"),
        ),
        &limits,
        None,
        true,
    ));
    reads.push(outer_step(
        outer(
            vec![thread("T_2", connection(vec![second_comment], 1, None))],
            2,
            None,
        ),
        &limits,
        Some("outer1"),
        true,
    ));
    reads.extend(suffix());
    let (provider, parent) = prepare(&fixture, reads);
    let snapshot = provider
        .metrics_review_threads(&current(), &parent, &limits, &CancellationToken::default())
        .unwrap();
    assert_eq!(snapshot.requests, 8);
    assert_eq!(snapshot.state_coverage.fetched_pages, 2);
    assert!(
        snapshot.state_coverage.enumeration_complete
            && snapshot.state_coverage.metric_evidence_complete
    );
    assert_eq!(
        snapshot
            .threads
            .iter()
            .map(|thread| thread.node_id.as_str())
            .collect::<Vec<_>>(),
        ["T_1", "T_2"]
    );
    assert!(snapshot
        .threads
        .iter()
        .all(|thread| thread.parent_node_id == "PR_1"
            && thread.comment_coverage.enumeration_complete));
    assert_reads(&fixture, 11);
}

#[test]
fn metrics_threads_provider_moving_nested_state_totals_and_reused_cursor_refuse_without_extra_read()
{
    for variation in 0..4 {
        let fixture = Fixture::new();
        let limits = MetricsThreadLimits {
            thread_page_size: 1,
            comment_page_size: 1,
            ..Default::default()
        };
        let mut fresh = thread("T_1", connection(vec![comment("C_root", true)], 2, None));
        match variation {
            0 => fresh["isResolved"] = json!(true),
            1 => fresh["comments"]["totalCount"] = json!(3),
            2 => fresh["comments"]["pageInfo"] = json!({"hasNextPage":true,"endCursor":"cursor1"}),
            _ => fresh["pullRequest"]["headRefOid"] = json!("d".repeat(40)),
        }
        let mut reads = prefix();
        reads.push(outer_step(
            outer(
                vec![thread(
                    "T_1",
                    connection(vec![comment("C_reply", false)], 2, Some("cursor1")),
                )],
                1,
                None,
            ),
            &limits,
            None,
            true,
        ));
        reads.push(graphql_step(
            METRICS_THREAD_COMMENTS_QUERY,
            json!({"id":"T_1","first":1,"after":"cursor1"}),
            json!({"data":{"node":fresh}}),
        ));
        let (provider, parent) = prepare(&fixture, reads);
        let error = provider
            .metrics_review_threads(&current(), &parent, &limits, &CancellationToken::default())
            .unwrap_err();
        assert!(matches!(
            error.kind,
            GithubErrorKind::StaleHead | GithubErrorKind::Json
        ));
        assert_reads(&fixture, 8);
    }
}

#[test]
fn metrics_threads_provider_raw_page_and_positive_database_node_collisions_do_not_hide_in_caps() {
    for variation in 0..3 {
        let fixture = Fixture::new();
        let limits = MetricsThreadLimits {
            thread_page_size: 1,
            comment_page_size: if variation == 0 { 1 } else { 2 },
            max_graphql_pages: if variation == 2 { 1 } else { 10 },
            ..Default::default()
        };
        let mut second = comment("C_reply", false);
        if variation == 1 {
            second["fullDatabaseId"] = json!(u64::MAX.to_string());
        }
        let comments = if variation == 2 {
            connection(vec![comment("C_root", true)], 2, Some("commentNext"))
        } else {
            connection(vec![comment("C_root", true), second], 2, None)
        };
        let mut reads = prefix();
        reads.push(outer_step(
            outer(vec![thread("T_1", comments)], 1, None),
            &limits,
            None,
            true,
        ));
        if variation == 2 {
            reads.extend(suffix());
        }
        let (provider, parent) = prepare(&fixture, reads);
        let result = provider.metrics_review_threads(
            &current(),
            &parent,
            &limits,
            &CancellationToken::default(),
        );
        if variation == 2 {
            let snapshot = result.unwrap();
            assert!(
                snapshot.state_coverage.enumeration_complete
                    && snapshot.state_coverage.metric_evidence_complete
            );
            assert!(!snapshot.threads[0].comment_coverage.enumeration_complete);
            assert_eq!(snapshot.graphql_pages, 1);
            assert_reads(&fixture, 10);
        } else {
            assert!(matches!(
                result.unwrap_err().kind,
                GithubErrorKind::Json | GithubErrorKind::IdentityChanged
            ));
            assert_reads(&fixture, 7);
        }
    }
}

#[test]
fn metrics_threads_provider_timeout_budget_never_publishes_unbracketed_old_state() {
    let fixture = Fixture::new();
    let limits = MetricsThreadLimits {
        max_duration: Duration::from_millis(250),
        ..Default::default()
    };
    let mut slow = get("user", json!({"id":42,"login":"octocat"}));
    slow.delay_ms = 1_000;
    let (provider, parent) = prepare(&fixture, vec![slow]);
    assert!(provider
        .metrics_review_threads(&current(), &parent, &limits, &CancellationToken::default())
        .is_err());
    assert_reads(&fixture, 4);
}

#[test]
fn metrics_threads_provider_failed_nested_dual_capture_reservation_preserves_state_only_if_final_proof_fits(
) {
    for exhausted in [false, true] {
        let fixture = Fixture::new();
        let mut limits = MetricsThreadLimits {
            thread_page_size: 1,
            comment_page_size: 1,
            ..Default::default()
        };
        let initial_page = outer_step(
            outer(
                vec![thread(
                    "T_1",
                    connection(vec![comment("C_reply", false)], 2, Some("cursor1")),
                )],
                1,
                None,
            ),
            &limits,
            None,
            true,
        );
        let prefix_bytes = prefix().iter().map(|step| step.stdout.len()).sum::<usize>()
            + initial_page.stdout.len();
        if exhausted {
            limits.max_response_bytes = prefix_bytes + 4096;
        }
        let mut failed = graphql_step(
            METRICS_THREAD_COMMENTS_QUERY,
            json!({"id":"T_1","first":1,"after":"cursor1"}),
            Value::Null,
        );
        failed.status = 1;
        failed.stdout = "é".repeat(1000).into_bytes();
        failed.stderr = format!("HTTP 502: {}", "界".repeat(1000)).into_bytes();
        let mut steps = prefix();
        steps.push(initial_page);
        steps.push(failed);
        let successful_bytes =
            prefix_bytes + suffix().iter().map(|step| step.stdout.len()).sum::<usize>();
        steps.extend(suffix());
        let (provider, parent) = prepare(&fixture, steps);
        let result = provider.metrics_review_threads(
            &current(),
            &parent,
            &limits,
            &CancellationToken::default(),
        );
        if exhausted {
            assert!(result.is_err());
            assert_reads(&fixture, 8);
        } else {
            let source = result.unwrap();
            assert_eq!(source.failed_reserved_bytes, 4 * 1024 * 1024);
            assert_eq!(source.aggregate_response_bytes, successful_bytes);
            assert!(
                source.state_coverage.enumeration_complete
                    && source.state_coverage.metric_evidence_complete
            );
            assert!(!source.threads[0].comment_coverage.enumeration_complete);
            assert!(source.threads[0].original_comment.is_none());
            assert_reads(&fixture, 11);
        }
    }
}
