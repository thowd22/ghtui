//! Strict bounded hydration contracts, independent of caller cache/index projections.
use super::*;
use ghtui::providers::gh::{
    DiscussionParent, DiscussionParentKind, DiscussionRecordKind, HydrationLimits, Permissions,
    Repository, DISCUSSION_THREADS_QUERY,
};

fn repository() -> Repository {
    Repository {
        host: "github.invalid".into(),
        id: 410,
        full_name: "example/repo".into(),
        description: String::new(),
        default_branch: "main".into(),
        private: true,
        archived: false,
        permissions: Permissions::default(),
    }
}
fn repo_value(id: u64) -> Value {
    json!({"id":id,"full_name":"example/repo","private":true})
}
fn parent_pr() -> Value {
    let mut value = pr(7, "example/repo");
    value["id"] = json!(700);
    value["url"] = json!("https://github.invalid/repos/example/repo/pulls/7");
    value["base"]["repo"] = json!({"id":410,"full_name":"example/repo"});
    value["labels"] = json!([{"name":"bug"}]);
    value["created_at"] = json!("2026-10-10T12:00:00Z");
    value
}
fn parent_issue() -> Value {
    json!({"id":800,"node_id":"ISSUE_8","number":8,"title":"Issue 8","body":"issue-only phrase","state":"open","user":null,"labels":[],"url":"https://github.invalid/repos/example/repo/issues/8","repository_url":"https://github.invalid/repos/example/repo","html_url":"https://github.invalid/example/repo/issues/8","created_at":"2026-10-10T12:00:00Z","updated_at":"2026-10-12T00:00:00Z"})
}
fn expected_issue() -> DiscussionParent {
    DiscussionParent {
        host: "github.invalid".into(),
        repository_id: 410,
        repository: "example/repo".into(),
        node_id: "ISSUE_8".into(),
        numeric_id: Some(800),
        number: 8,
        kind: DiscussionParentKind::Issue,
        title: "Issue 8".into(),
        body: "issue-only phrase".into(),
        body_complete: true,
        state: "open".into(),
        author: None,
        labels: Some(vec![]),
        created_at: Some("2026-10-10T12:00:00Z".into()),
        updated_at: Some("2026-10-12T00:00:00Z".into()),
        url: "https://github.invalid/example/repo/issues/8".into(),
        head_sha: None,
    }
}
fn comment(id: u64, issue: bool) -> Value {
    let number = if issue { 8 } else { 7 };
    let html_kind = if issue { "issues" } else { "pull" };
    json!({"id":id,"node_id":format!("COMMENT_{id}"),"body":format!("comment phrase {id}"),"user":null,"created_at":"2026-10-11T12:00:00Z","updated_at":"2026-10-12T12:00:00Z","issue_url":format!("https://github.invalid/repos/example/repo/issues/{number}"),"html_url":format!("https://github.invalid/example/repo/{html_kind}/{number}#issuecomment-{id}")})
}
fn pr_steps(parent: Value, list: Vec<GhStep>, fresh: Value, after_account: u64) -> Vec<GhStep> {
    let mut steps = account_steps(42);
    steps.push(step(api_args("repos/example/repo/pulls/7"), parent));
    steps.extend(list);
    steps.push(step(api_args("repos/example/repo/pulls/7"), fresh));
    steps.extend(account_steps(after_account));
    steps
}
fn assert_reads(fixture: &Fixture, count: usize) {
    let rows = journal(fixture);
    assert_eq!(rows.len(), count);
    assert!(rows.iter().all(|row| row["matched"] == true));
    assert!(!rows.iter().any(|row| row["args"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| arg == "PUT" || arg == "PATCH" || arg == "DELETE")));
    for row in rows.iter().filter(|row| row["args"][1] == "graphql") {
        let hex = row["stdin_hex"].as_str().unwrap();
        let bytes = hex
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect::<Vec<_>>();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        assert!(
            body["query"].as_str().unwrap().starts_with("query("),
            "A read-only GraphQL POST is not a mutation"
        );
    }
}

#[test]
fn collaboration_index_provider_paged_comments_keep_actual_parent_id_labels_dates_and_deleted_actor(
) {
    let fixture = Fixture::new();
    fixture.scripted_gh(&pr_steps(
        parent_pr(),
        vec![
            step(
                api_args("repos/example/repo/issues/7/comments?per_page=1&page=1"),
                json!([comment(11, false)]),
            ),
            step(
                api_args("repos/example/repo/issues/7/comments?per_page=1&page=2"),
                json!([]),
            ),
        ],
        parent_pr(),
        42,
    ));
    let snapshot = provider(&fixture)
        .pr_discussion_comments(
            &verified_identity(),
            &repository(),
            &target(),
            &HydrationLimits {
                page_size: 1,
                ..Default::default()
            },
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(snapshot.parent.numeric_id, Some(700));
    assert_ne!(snapshot.parent.numeric_id, Some(snapshot.parent.number));
    assert_eq!(snapshot.parent.labels, Some(vec!["bug".into()]));
    assert!(snapshot.parent.body_complete);
    assert_eq!(snapshot.records.len(), 1);
    assert_eq!(snapshot.records[0].numeric_id, Some(11));
    assert!(snapshot.records[0].author.is_none());
    assert_eq!(
        snapshot.records[0].updated_at.as_deref(),
        Some("2026-10-12T12:00:00Z")
    );
    assert!(snapshot.coverage.enumeration_complete && snapshot.coverage.complete);
    assert_eq!(snapshot.coverage.pages, 2);
    assert_reads(&fixture, 8);
}

#[test]
fn collaboration_index_provider_budgets_exclude_observed_nodes_without_authorizing_old_text() {
    for oversized in [false, true] {
        let fixture = Fixture::new();
        let mut parent = parent_pr();
        let mut first = comment(11, false);
        if oversized {
            parent["body"] = json!("parent".repeat(1000));
            first["body"] = json!("oversized".repeat(1000));
        }
        fixture.scripted_gh(&pr_steps(
            parent.clone(),
            vec![step(
                api_args("repos/example/repo/issues/7/comments?per_page=3&page=1"),
                json!([first, comment(12, false), comment(13, false)]),
            )],
            parent,
            42,
        ));
        let limits = HydrationLimits {
            max_documents: 1,
            max_document_bytes: 1400,
            page_size: 3,
            ..Default::default()
        };
        let snapshot = provider(&fixture)
            .pr_discussion_comments(
                &verified_identity(),
                &repository(),
                &target(),
                &limits,
                &CancellationToken::default(),
            )
            .unwrap();
        assert!(!snapshot.coverage.enumeration_complete && !snapshot.coverage.complete);
        assert_eq!(snapshot.coverage.next_page, Some(1));
        if oversized {
            assert!(snapshot.records.is_empty());
            assert!(!snapshot.parent.body_complete && snapshot.parent.body.is_empty());
            assert_eq!(
                snapshot.coverage.excluded_nodes,
                vec![
                    "PR_example/repo_7",
                    "COMMENT_11",
                    "COMMENT_12",
                    "COMMENT_13"
                ]
            );
        } else {
            assert_eq!(snapshot.records.len(), 1);
            assert_eq!(
                snapshot.coverage.excluded_nodes,
                vec!["COMMENT_12", "COMMENT_13"]
            );
        }
        assert_reads(&fixture, 7);
    }
    let fixture = Fixture::new();
    fixture.scripted_gh(&pr_steps(
        parent_pr(),
        vec![step(
            api_args("repos/example/repo/issues/7/comments?per_page=1&page=1"),
            json!([comment(11, false)]),
        )],
        parent_pr(),
        42,
    ));
    let snapshot = provider(&fixture)
        .pr_discussion_comments(
            &verified_identity(),
            &repository(),
            &target(),
            &HydrationLimits {
                max_pages: 1,
                page_size: 1,
                ..Default::default()
            },
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(!snapshot.coverage.enumeration_complete);
    assert_eq!(snapshot.coverage.next_page, Some(2));
    assert!(
        snapshot.coverage.excluded_nodes.is_empty(),
        "Unknown future pages are not observed exclusions"
    );
    assert_reads(&fixture, 7);
}

#[test]
fn collaboration_index_provider_pending_reviews_do_not_invent_submission_dates_or_actor_types() {
    let fixture = Fixture::new();
    let review = json!({"id":12,"node_id":"REVIEW_12","body":"review-only phrase","state":"PENDING","commit_id":"cccccccccccccccccccccccccccccccccccccccc","user":{"login":"future","type":"FutureActor"},"pull_request_url":"https://github.invalid/repos/example/repo/pulls/7","html_url":"https://github.invalid/example/repo/pull/7#pullrequestreview-12"});
    fixture.scripted_gh(&pr_steps(
        parent_pr(),
        vec![step(
            api_args("repos/example/repo/pulls/7/reviews?per_page=50&page=1"),
            json!([review]),
        )],
        parent_pr(),
        42,
    ));
    let snapshot = provider(&fixture)
        .pr_discussion_reviews(
            &verified_identity(),
            &repository(),
            &target(),
            &HydrationLimits::default(),
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(snapshot.records[0].kind, DiscussionRecordKind::Review);
    assert_eq!(snapshot.records[0].state.as_deref(), Some("PENDING"));
    assert_eq!(
        snapshot.records[0].commit_sha.as_deref(),
        Some("cccccccccccccccccccccccccccccccccccccccc")
    );
    assert_ne!(snapshot.records[0].commit_sha, snapshot.parent.head_sha);
    assert!(snapshot.records[0].created_at.is_none() && snapshot.records[0].submitted_at.is_none());
    assert_eq!(
        snapshot.records[0].author.as_ref().unwrap().actor_type,
        "FutureActor"
    );
    assert_reads(&fixture, 7);
}

#[test]
fn collaboration_index_provider_same_sha_parent_replacement_and_same_login_reassignment_fail_closed(
) {
    for changed_node in [false, true] {
        let fixture = Fixture::new();
        let mut fresh = parent_pr();
        if changed_node {
            fresh["node_id"] = json!("DIFFERENT_PR");
        }
        fixture.scripted_gh(&pr_steps(
            parent_pr(),
            vec![step(
                api_args("repos/example/repo/issues/7/comments?per_page=50&page=1"),
                json!([]),
            )],
            fresh,
            if changed_node { 42 } else { 43 },
        ));
        let failure = provider(&fixture)
            .pr_discussion_comments(
                &verified_identity(),
                &repository(),
                &target(),
                &HydrationLimits::default(),
                &CancellationToken::default(),
            )
            .unwrap_err();
        if changed_node {
            assert!(matches!(failure.kind, GithubErrorKind::StaleHead));
            assert_reads(&fixture, 5);
        } else {
            assert!(matches!(failure.kind, GithubErrorKind::IdentityChanged));
            assert_reads(&fixture, 7);
        }
    }
    let fixture = Fixture::new();
    fixture.scripted_gh(&[]);
    assert!(provider(&fixture)
        .pr_discussion_comments(
            &identity(),
            &repository(),
            &target(),
            &HydrationLimits::default(),
            &CancellationToken::default()
        )
        .is_err());
    assert!(journal(&fixture).is_empty());
}

#[test]
fn collaboration_index_provider_issue_preflight_precedes_reserved_parent_hydration() {
    let fixture = Fixture::new();
    let mut steps = account_steps(42);
    steps.extend([
        step(api_args("repos/example/repo"), repo_value(410)),
        step(api_args("repos/example/repo/issues/8"), parent_issue()),
        step(api_args("repos/example/repo"), repo_value(410)),
    ]);
    steps.extend(account_steps(42));
    steps.extend(account_steps(42));
    steps.extend([
        step(api_args("repos/example/repo"), repo_value(410)),
        step(api_args("repos/example/repo/issues/8"), parent_issue()),
        step(
            api_args("repos/example/repo/issues/8/comments?per_page=50&page=1"),
            json!([comment(21, true)]),
        ),
        step(api_args("repos/example/repo/issues/8"), parent_issue()),
        step(api_args("repos/example/repo"), repo_value(410)),
    ]);
    steps.extend(account_steps(42));
    fixture.scripted_gh(&steps);
    let provider = provider(&fixture);
    let parent = provider
        .issue_parent(
            &verified_identity(),
            &repository(),
            8,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(parent.node_id, "ISSUE_8");
    assert_eq!(parent.numeric_id, Some(800));
    assert_reads(&fixture, 7);
    let snapshot = provider
        .issue_details(
            &verified_identity(),
            &repository(),
            &parent,
            &HydrationLimits::default(),
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(snapshot.parent.kind, DiscussionParentKind::Issue);
    assert_eq!(snapshot.records[0].numeric_id, Some(21));
    assert!(snapshot.parent.author.is_none());
    assert!(snapshot.coverage.enumeration_complete);
    assert_reads(&fixture, 16);
}

#[test]
fn collaboration_index_provider_issue_kind_replacement_and_repository_recreation_never_mix_sources()
{
    for mode in [0, 1, 2] {
        let fixture = Fixture::new();
        let mut observed = parent_issue();
        if mode == 0 {
            observed["pull_request"] =
                json!({"url":"https://github.invalid/repos/example/repo/pulls/8"});
        }
        if mode == 1 {
            observed["node_id"] = json!("REPLACEMENT_ISSUE");
        }
        let mut steps = account_steps(42);
        steps.extend([
            step(api_args("repos/example/repo"), repo_value(410)),
            step(api_args("repos/example/repo/issues/8"), observed),
        ]);
        if mode == 2 {
            steps.extend([
                step(
                    api_args("repos/example/repo/issues/8/comments?per_page=50&page=1"),
                    json!([]),
                ),
                step(api_args("repos/example/repo/issues/8"), parent_issue()),
                step(api_args("repos/example/repo"), repo_value(999)),
            ]);
        }
        fixture.scripted_gh(&steps);
        assert!(matches!(
            provider(&fixture)
                .issue_details(
                    &verified_identity(),
                    &repository(),
                    &expected_issue(),
                    &HydrationLimits::default(),
                    &CancellationToken::default()
                )
                .unwrap_err()
                .kind,
            GithubErrorKind::InvalidTarget
        ));
        assert_reads(&fixture, if mode == 2 { 7 } else { 4 });
    }
}

fn thread(id: &str, has_more_comments: bool) -> Value {
    json!({"id":id,"path":"src/lib.rs","diffSide":"RIGHT","line":7,"startLine":null,"isResolved":false,"isOutdated":false,"viewerCanResolve":true,"viewerCanUnresolve":false,"pullRequest":{"id":"PR_example/repo_7"},"repository":{"nameWithOwner":"example/repo"},"comments":{"totalCount":if has_more_comments{2}else{1},"nodes":[{"id":format!("INLINE_{id}"),"fullDatabaseId":"18446744073709551616","body":"inline-only phrase","createdAt":"2026-10-11T12:00:00Z","updatedAt":"2026-10-12T12:00:00Z","url":"https://github.invalid/example/repo/pull/7#discussion_r99","author":{"__typename":"Bot","login":"ci-bot","avatarUrl":"https://avatars.invalid/99"},"commit":{"oid":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},"pullRequest":{"id":"PR_example/repo_7"}}],"pageInfo":{"hasNextPage":has_more_comments,"endCursor":"INNER_CURSOR"}}})
}
fn thread_page(nodes: Vec<Value>, total: u64, more: bool, cursor: &str) -> Value {
    json!({"data":{"node":{"__typename":"PullRequest","id":"PR_example/repo_7","number":7,"headRefOid":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","baseRefOid":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","updatedAt":"2026-10-12T00:00:00Z","repository":{"nameWithOwner":"example/repo"},"reviewThreads":{"totalCount":total,"nodes":nodes,"pageInfo":{"hasNextPage":more,"endCursor":cursor}}}}})
}
fn gql(cursor: Option<&str>, first: usize, inner: usize, out: Value) -> GhStep {
    write_step(
        "graphql",
        "POST",
        Some(
            json!({"query":DISCUSSION_THREADS_QUERY,"variables":{"id":"PR_example/repo_7","cursor":cursor,"first":first,"commentsFirst":inner}}),
        ),
        out,
    )
}

#[test]
fn collaboration_index_provider_outer_threads_complete_does_not_claim_inner_comment_completeness() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&pr_steps(
        parent_pr(),
        vec![gql(
            None,
            50,
            20,
            thread_page(vec![thread("THREAD_1", true)], 1, false, "OUTER_END"),
        )],
        parent_pr(),
        42,
    ));
    let snapshot = provider(&fixture)
        .discussion_threads(
            &verified_identity(),
            &repository(),
            &target(),
            &HydrationLimits::default(),
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(snapshot.coverage.enumeration_complete && !snapshot.coverage.complete);
    assert!(!snapshot.comment_coverage["THREAD_1"].enumeration_complete);
    assert!(!snapshot.threads[0].comments_complete);
    assert_eq!(
        snapshot.threads[0].comments[0]["fullDatabaseId"],
        "18446744073709551616"
    );
    assert_eq!(
        snapshot.threads[0].comments[0]["author"]["__typename"],
        "Bot"
    );
    assert_eq!(
        snapshot.threads[0].comments[0]["updatedAt"],
        "2026-10-12T12:00:00Z"
    );
    assert_reads(&fixture, 7);
}

#[test]
fn collaboration_index_provider_outer_page_budget_preserves_complete_group_evidence_and_cursor() {
    for capped in [false, true] {
        let fixture = Fixture::new();
        let mut reads = vec![gql(
            None,
            1,
            1,
            thread_page(vec![thread("THREAD_1", false)], 2, true, "CURSOR_1"),
        )];
        if !capped {
            reads.push(gql(
                Some("CURSOR_1"),
                1,
                1,
                thread_page(vec![thread("THREAD_2", false)], 2, false, "CURSOR_2"),
            ));
        }
        fixture.scripted_gh(&pr_steps(parent_pr(), reads, parent_pr(), 42));
        let snapshot = provider(&fixture)
            .discussion_threads(
                &verified_identity(),
                &repository(),
                &target(),
                &HydrationLimits {
                    page_size: 1,
                    thread_comment_page_size: 1,
                    max_pages: if capped { 1 } else { 2 },
                    ..Default::default()
                },
                &CancellationToken::default(),
            )
            .unwrap();
        assert!(snapshot.comment_coverage["THREAD_1"].enumeration_complete);
        assert_eq!(snapshot.coverage.enumeration_complete, !capped);
        assert_eq!(
            snapshot.coverage.next_cursor.as_deref(),
            if capped { Some("CURSOR_1") } else { None }
        );
        assert_eq!(snapshot.threads.len(), if capped { 1 } else { 2 });
        assert_reads(&fixture, if capped { 7 } else { 8 });
    }
}

#[test]
fn collaboration_index_provider_thread_budget_excludes_every_observed_unadmitted_group() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&pr_steps(
        parent_pr(),
        vec![gql(
            None,
            2,
            20,
            thread_page(
                vec![thread("THREAD_1", false), thread("THREAD_2", false)],
                2,
                false,
                "END",
            ),
        )],
        parent_pr(),
        42,
    ));
    let snapshot = provider(&fixture)
        .discussion_threads(
            &verified_identity(),
            &repository(),
            &target(),
            &HydrationLimits {
                page_size: 2,
                max_documents: 1,
                ..Default::default()
            },
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(!snapshot.coverage.enumeration_complete);
    assert!(snapshot.threads.is_empty());
    assert_eq!(
        snapshot.coverage.excluded_nodes,
        vec!["THREAD_1", "THREAD_2"]
    );
    assert_reads(&fixture, 7);
}

#[test]
fn collaboration_index_provider_foreign_graphql_parent_and_nonadvancing_cursor_are_not_partial_success(
) {
    for repeated_cursor in [false, true] {
        let fixture = Fixture::new();
        let mut first = thread_page(vec![thread("THREAD_1", false)], 3, true, "REPEATED");
        if !repeated_cursor {
            first["data"]["node"]["id"] = json!("OTHER_PR");
        }
        let mut reads = vec![gql(None, 1, 1, first)];
        if repeated_cursor {
            reads.push(gql(
                Some("REPEATED"),
                1,
                1,
                thread_page(vec![thread("THREAD_2", false)], 3, true, "REPEATED"),
            ));
        }
        let mut steps = account_steps(42);
        steps.push(step(api_args("repos/example/repo/pulls/7"), parent_pr()));
        steps.extend(reads);
        fixture.scripted_gh(&steps);
        let failure = provider(&fixture)
            .discussion_threads(
                &verified_identity(),
                &repository(),
                &target(),
                &HydrationLimits {
                    page_size: 1,
                    thread_comment_page_size: 1,
                    ..Default::default()
                },
                &CancellationToken::default(),
            )
            .unwrap_err();
        if repeated_cursor {
            assert!(matches!(failure.kind, GithubErrorKind::Json));
            assert_reads(&fixture, 5);
        } else {
            assert!(matches!(failure.kind, GithubErrorKind::StaleHead));
            assert_reads(&fixture, 4);
        }
    }
}
