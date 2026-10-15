//! SOURCE-ONLY strict provider contracts. All factories below perform genuine
//! original Files/source/account/parent reads; no receipt constructor exists.
use super::*;
#[path = "scoped_pr_thread_comment_v2_tests.rs"]
mod comment_v2;
#[path = "scoped_pr_thread_composer_v1_tests.rs"]
mod composer_v1;
#[path = "scoped_pr_thread_history_v1_tests.rs"]
mod history_v1;
use crate::providers::gh::{
    GithubError, GithubErrorKind, HostIdentity, OriginalThreadPublicationV1,
    ThreadPublicationFailureV1, METRICS_THREAD_COMMENTS_QUERY,
};

fn missing_original_fixture() -> GithubError {
    GithubError {
        kind: GithubErrorKind::InvalidTarget,
        message: "Exact original fixture source unavailable".into(),
    }
}

fn selected_node(resolved: bool, reply: bool) -> Value {
    let page = thread_page(Some("LINE"), false);
    let mut node = page["data"]["node"]["reviewThreads"]["nodes"][0].clone();
    node["__typename"] = json!("PullRequestReviewThread");
    node["isResolved"] = json!(resolved);
    node["viewerCanResolve"] = json!(!resolved);
    node["viewerCanUnresolve"] = json!(resolved);
    node["viewerCanReply"] = json!(reply);
    node["pullRequest"]["baseRefOid"] = json!("b".repeat(40));
    node
}
fn graphql_step(query: &str, variables: Value, output: Value) -> GhStep {
    let mut step = GhStep::read(
        &[
            "api",
            "graphql",
            "--hostname",
            HOST,
            "--method",
            "POST",
            "--input",
            "-",
        ],
        &output.to_string(),
    );
    step.stdin = serde_json::to_vec(&json!({"query":query,"variables":variables})).unwrap();
    step
}
fn selected_step(node: Value) -> GhStep {
    graphql_step(
        METRICS_THREAD_COMMENTS_QUERY,
        json!({"id":"THREAD_ACTUAL_9","first":100,"after":null}),
        json!({"data":{"node":node}}),
    )
}
fn observation_steps(node: Value) -> Vec<GhStep> {
    let mut steps = principal(42, "octocat");
    steps.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)));
    steps.extend(selected_bridge(node));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)));
    steps.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    steps.extend(principal(42, "octocat"));
    assert_eq!(steps.len(), 11);
    steps
}
fn selected_bridge(node: Value) -> Vec<GhStep> {
    vec![
        get(&format!("repos/{REPO}"), repo(410, REPO)),
        get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)),
        selected_step(node),
    ]
}
fn opening_steps(node: Value) -> Vec<GhStep> {
    observation_steps(node).into_iter().take(7).collect()
}
fn close_steps() -> Vec<GhStep> {
    let mut steps = vec![
        get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)),
        get(&format!("repos/{REPO}"), repo(410, REPO)),
    ];
    steps.extend(principal(42, "octocat"));
    steps
}
fn state_readback(node: Value) -> GhStep {
    graphql_step(
        crate::providers::gh::THREAD_RESOLUTION_STATE_QUERY_V2,
        json!({"id":"THREAD_ACTUAL_9"}),
        json!({"data":{"node":node}}),
    )
}
struct AttemptV2<'a> {
    provider: &'a GithubProvider,
    measured: &'a GithubProvider,
    actual: &'a Arc<OriginalThreadPublicationV1>,
    identity: &'a HostIdentity,
    captures: &'a mut crate::providers::gh::PostWriteCaptureAllowanceV2,
    cancel: &'a CancellationToken,
    deadline: Instant,
    close: &'a mut dyn FnMut() -> Result<(), crate::providers::gh::GithubError>,
    closed: &'a std::cell::RefCell<Option<crate::providers::gh::ClosedCatalogueAccess>>,
}
impl AttemptV2<'_> {
    fn resolve(
        &mut self,
        desired: bool,
    ) -> Result<crate::providers::gh::ThreadResolutionReceiptV1, ThreadPublicationFailureV1> {
        self.provider.resolve_thread_guarded_v1(
            self.identity,
            self.actual,
            self.measured,
            self.captures,
            desired,
            self.cancel,
            self.deadline,
            &mut *self.close,
        )
    }
    fn reply(
        &mut self,
        body: &str,
    ) -> Result<crate::providers::gh::ThreadReplyReceiptV1, ThreadPublicationFailureV1> {
        self.provider.reply_thread_guarded_v1(
            self.identity,
            self.actual,
            self.measured,
            self.captures,
            body,
            self.cancel,
            self.deadline,
            &mut *self.close,
        )
    }
    fn visibility(&mut self) -> Result<(), crate::providers::gh::GithubError> {
        let closed = self.closed.borrow();
        let current = closed
            .as_ref()
            .unwrap()
            .observation(self.actual.parent().repository_id())
            .unwrap();
        self.provider.close_thread_postwrite_visibility_v2(
            self.identity,
            self.actual,
            current,
            self.captures,
            self.cancel,
            self.deadline,
            || Ok(()),
        )
    }
}
fn with_original<T>(real: &Real, call: impl FnOnce(AttemptV2<'_>) -> T) -> T {
    with_original_limits(real, 10, call)
}
fn with_original_limits<T>(
    real: &Real,
    requests: usize,
    call: impl FnOnce(AttemptV2<'_>) -> T,
) -> T {
    let intent = threads::ThreadBrowseIntentV1::capture(&real.app).unwrap();
    assert!(intent.owns(&real.app));
    let observed = real
        .app
        .github
        .as_ref()
        .unwrap()
        .scoped_pr_resource
        .as_ref()
        .unwrap()
        .observed();
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let phase = provider(&real.fixture).with_catalogue_read_deadline(deadline);
    let mut session = phase
        .begin_repository_catalogue_access(intent.source.source(), &Default::default(), &cancel)
        .unwrap();
    let identity = session.identity().clone();
    let mut captures = session
        .reserve_postwrite_capture_v2(&crate::providers::gh::THREAD_POSTWRITE_CAPS_V2, 2, &cancel)
        .unwrap();
    let window = session
        .reserve_measured_external_reads(requests, 1024 * 1024, 8 * 1024 * 1024, 2, &cancel)
        .unwrap();
    let bounded = phase.clone().with_measured_read_window(window).unwrap();
    let current = session
        .verify(intent.navigation.original(), &cancel)
        .unwrap();
    let parent = bounded
        .pr_inline_original_parent_v1(current, observed, &Default::default(), &cancel)
        .unwrap();
    let local = ResourceLocalGuard::capture(
        &database(&real.fixture),
        &intent.source,
        &identity,
        parent.summary(),
    )
    .unwrap();
    let actual = Arc::new(
        bounded
            .original_thread_publication_v1(parent, "THREAD_ACTUAL_9", &cancel, deadline)
            .unwrap(),
    );
    let mut pending = Some(session);
    let closed = std::cell::RefCell::new(None);
    let mut close = || {
        let mut session = pending.take().ok_or_else(missing_original_fixture)?;
        let current = session
            .observation_v1(actual.parent().repository_id())
            .ok_or_else(missing_original_fixture)?;
        bounded.pr_inline_original_parent_v1(current, observed, &Default::default(), &cancel)?;
        session.verify(intent.navigation.original(), &cancel)?;
        *closed.borrow_mut() = Some(session.close_with_guard(&cancel, || {
            local.validate(&database(&real.fixture), &intent.source)
        })?);
        Ok(())
    };
    call(AttemptV2 {
        provider: &phase,
        measured: &bounded,
        actual: &actual,
        identity: &identity,
        captures: &mut captures,
        cancel: &cancel,
        deadline,
        close: &mut close,
        closed: &closed,
    })
}
fn write_guard(uid: u64, login: &str) -> Vec<GhStep> {
    vec![
        get("user", json!({"id":uid,"login":login})),
        get(&format!("repos/{REPO}"), repo(410, REPO)),
        get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)),
    ]
}
fn resolution_step(desired: bool, node: Value) -> GhStep {
    let (name, input) = if desired {
        ("resolveReviewThread", "ResolveReviewThreadInput")
    } else {
        ("unresolveReviewThread", "UnresolveReviewThreadInput")
    };
    let fields = crate::providers::gh::THREAD_RESOLUTION_MUTATION_FIELDS_V2;
    graphql_step(
        &format!("mutation($input:{input}!){{{name}(input:$input){{{fields}}}}}"),
        json!({"input":{"threadId":"THREAD_ACTUAL_9"}}),
        json!({"data":{name:{"thread":node}}}),
    )
}
fn protocol(real: &Real, mutations: usize) {
    assert_eq!(sql_state(&real.fixture), real.sql);
    assert_eq!(git_state_resource(&real.fixture), real.git);
    let rows: Vec<Value> = fs::read_to_string(real.fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|row| serde_json::from_str(row).unwrap())
        .collect();
    assert_eq!(rows.len(), real.steps.len());
    let mut writes = 0;
    for (row, step) in rows.iter().zip(&real.steps) {
        assert_eq!(row["matched"], true);
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(
            row["stdin_hex"],
            step.stdin
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        if step.args.iter().any(|arg| arg == "POST") {
            let body: Value = serde_json::from_slice(&step.stdin).unwrap();
            if step.args[1] == "graphql" {
                let query = body["query"].as_str().unwrap();
                if query.starts_with("mutation(") {
                    writes += 1;
                    assert_eq!(body["variables"]["input"]["threadId"], "THREAD_ACTUAL_9");
                } else {
                    assert!(query.starts_with("query("));
                }
            } else {
                assert_eq!(
                    step.args[1],
                    format!("repos/{REPO}/pulls/7/comments/9009/replies")
                );
                writes += 1;
            }
        }
    }
    assert_eq!(writes, mutations);
}
#[test]
fn review_threads_v1_guarded_resolve_reopen_exact_opaque_thread_parent_readback_and_numeric_actor()
{
    for desired in [true, false] {
        let mut final_node = selected_node(desired, true);
        if desired {
            final_node["resolvedBy"] = json!({"__typename":"User","databaseId":62,"login":"other-resolver","name":null,"avatarUrl":null});
        }
        let mut steps = opening_steps(selected_node(!desired, true));
        steps.extend(write_guard(42, "renamed-positive-account"));
        steps.extend(close_steps());
        steps.push(resolution_step(desired, final_node.clone()));
        steps.push(state_readback(final_node));
        let real = real(steps);
        with_original(&real, |mut attempt| {
            let receipt = attempt
                .resolve(desired)
                .unwrap_or_else(|_| panic!("exact original metered resolution required"));
            assert_eq!(receipt.thread_node(), "THREAD_ACTUAL_9");
            assert_eq!(receipt.resolved(), desired);
            if desired {
                assert_eq!(receipt.resolver().unwrap().account_id, Some(62));
            }
            assert_eq!(attempt.captures.consumed_requests(), 2);
        });
        protocol(&real, 1);
    }
}
fn reply_value(body: &str) -> Value {
    json!({"id":9100,"node_id":"CREATED_REPLY_9100","body":body,"user":{"id":42,"login":"renamed-positive-account","type":"User"},"pull_request_url":format!("https://{HOST}/api/v3/repos/{REPO}/pulls/7"),"commit_id":"c".repeat(40),"path":"src/old.rs","in_reply_to_id":9009,"created_at":"2026-10-15T00:00:00Z","updated_at":"2026-10-15T00:00:00Z"})
}
fn reply_step(body: &str, value: Value) -> GhStep {
    let mut step = GhStep::read(
        &[
            "api",
            &format!("repos/{REPO}/pulls/7/comments/9009/replies"),
            "--hostname",
            HOST,
            "--method",
            "POST",
            "--input",
            "-",
        ],
        &value.to_string(),
    );
    step.stdin = serde_json::to_vec(&json!({"body":body})).unwrap();
    step
}

#[test]
fn review_threads_v1_guarded_reply_uses_explicit_root_last_actual_rest_id_and_parent_commit_not_h()
{
    let mut node = selected_node(false, true);
    let root = node["comments"]["nodes"][0].clone();
    let mut reply = root.clone();
    reply["id"] = json!("EXISTING_REPLY");
    reply["fullDatabaseId"] = json!("9010");
    reply["replyTo"] = json!({"id":"COMMENT_ACTUAL_9"});
    node["comments"]["nodes"] = json!([reply, root]);
    node["comments"]["totalCount"] = json!(2);
    let body = "Actual bounded reply 界\nnot shell text";
    let created = reply_value(body);
    let mut steps = opening_steps(node);
    steps.extend(write_guard(42, "renamed-positive-account"));
    steps.extend(close_steps());
    steps.push(reply_step(body, created.clone()));
    steps.push(get(&format!("repos/{REPO}/pulls/comments/9100"), created));
    let real = real(steps);
    with_original(&real, |mut attempt| {
        assert_eq!(
            attempt.actual.original_reply_root().unwrap().database_id,
            Some(9009)
        );
        let receipt = attempt
            .reply(body)
            .unwrap_or_else(|_| panic!("actual metered root reply required"));
        assert_eq!(receipt.comment().id, 9100);
        assert_eq!(receipt.account_id(), 42);
        assert_eq!(receipt.original_comment_id(), 9009);
        assert_eq!(receipt.observed_parent_commit(), "c".repeat(40));
        assert_ne!(
            receipt.observed_parent_commit(),
            attempt.actual.parent().head_sha()
        );
        assert_eq!(attempt.captures.consumed_requests(), 2);
    });
    protocol(&real, 1);
}
#[test]
fn review_threads_v1_guarded_reply_wrong_readback_uid_root_commit_or_body_is_uncertain_one_invocation(
) {
    for field in ["uid", "root", "commit", "body", "node"] {
        let body = "Original reply body";
        let created = reply_value(body);
        let mut bad = created.clone();
        match field {
            "uid" => bad["user"]["id"] = json!(43),
            "root" => bad["in_reply_to_id"] = json!(9999),
            "commit" => bad["commit_id"] = json!("a".repeat(40)),
            "node" => bad["node_id"] = json!("DIFFERENT_NODE"),
            _ => bad["body"] = json!("Other body"),
        };
        let mut steps = opening_steps(selected_node(false, true));
        steps.extend(write_guard(42, "octocat"));
        steps.extend(close_steps());
        steps.push(reply_step(body, created));
        steps.push(get(&format!("repos/{REPO}/pulls/comments/9100"), bad));
        let real = real(steps);
        with_original(&real, |mut attempt| {
            assert!(matches!(
                attempt.reply(body),
                Err(ThreadPublicationFailureV1::WriteMayHaveStarted(_))
            ));
            assert_eq!(attempt.captures.consumed_requests(), 2);
        });
        protocol(&real, 1);
    }
}
#[test]
fn review_threads_v1_guarded_same_login_different_numeric_account_refuses_before_post() {
    let mut steps = opening_steps(selected_node(false, true));
    steps.push(get("user", json!({"id":43,"login":"octocat"})));
    let real = real(steps);
    with_original(&real, |mut attempt| {
        assert!(matches!(
            attempt.resolve(true),
            Err(ThreadPublicationFailureV1::NotStarted(_))
        ));
        assert_eq!(attempt.captures.consumed_requests(), 0);
        assert!(attempt.closed.borrow().is_none());
    });
    protocol(&real, 0);
}
#[test]
fn review_threads_v1_guarded_denied_expired_and_canceled_preview_starts_no_new_child() {
    let mut node = selected_node(false, false);
    node["viewerCanResolve"] = Value::Null;
    let real = real(opening_steps(node));
    with_original(&real, |mut attempt| {
        assert!(matches!(
            attempt.resolve(true),
            Err(ThreadPublicationFailureV1::NotStarted(_))
        ));
        assert!(matches!(
            attempt.reply("reply"),
            Err(ThreadPublicationFailureV1::NotStarted(_))
        ));
        assert!(matches!(
            attempt.provider.reply_thread_guarded_v1(
                attempt.identity,
                attempt.actual,
                attempt.measured,
                attempt.captures,
                "reply",
                attempt.cancel,
                Instant::now(),
                || panic!("expired phase cannot close")
            ),
            Err(ThreadPublicationFailureV1::NotStarted(_))
        ));
        attempt.cancel.cancel();
        assert!(matches!(
            attempt.resolve(true),
            Err(ThreadPublicationFailureV1::NotStarted(_))
        ));
        assert_eq!(attempt.captures.consumed_requests(), 0);
    });
    protocol(&real, 0);
}
#[test]
fn review_threads_v1_guarded_reply_missing_actual_root_id_never_decodes_opaque_node() {
    for id in [Value::Null, json!("18446744073709551616"), json!("0")] {
        let mut node = selected_node(false, true);
        node["comments"]["nodes"][0]["fullDatabaseId"] = id;
        let real = real(opening_steps(node));
        with_original(&real, |mut attempt| {
            assert!(attempt.actual.original_reply_root().is_err());
            assert!(matches!(
                attempt.reply("reply"),
                Err(ThreadPublicationFailureV1::NotStarted(_))
            ));
            assert_eq!(attempt.captures.consumed_requests(), 0);
        });
        protocol(&real, 0);
    }
}
#[test]
fn review_threads_v1_guarded_state_query_wrong_base_or_errors_beside_data_is_uncertain() {
    for malformed in ["wrong-base", "errors-plus-data", "missing-node"] {
        let final_node = selected_node(true, true);
        let mut back = json!({"data":{"node":final_node}});
        match malformed {
            "wrong-base" => {
                back["data"]["node"]["pullRequest"]["baseRefOid"] = json!("e".repeat(40))
            }
            "errors-plus-data" => back["errors"] = json!([{"message":"partial permission"}]),
            _ => back["data"]["node"] = Value::Null,
        };
        let mut steps = opening_steps(selected_node(false, true));
        steps.extend(write_guard(42, "octocat"));
        steps.extend(close_steps());
        steps.push(resolution_step(true, selected_node(true, true)));
        steps.push(graphql_step(
            crate::providers::gh::THREAD_RESOLUTION_STATE_QUERY_V2,
            json!({"id":"THREAD_ACTUAL_9"}),
            back,
        ));
        let real = real(steps);
        with_original(&real, |mut attempt| {
            assert!(matches!(
                attempt.resolve(true),
                Err(ThreadPublicationFailureV1::WriteMayHaveStarted(_))
            ));
            assert_eq!(attempt.captures.consumed_requests(), 2);
        });
        protocol(&real, 1);
    }
}
#[test]
fn review_threads_v1_guarded_failed_actual_close_and_foreign_original_token_cannot_post() {
    for mode in ["failed-close", "foreign-token", "foreign-runner"] {
        let mut steps = opening_steps(selected_node(false, true));
        if mode == "failed-close" {
            steps.extend(write_guard(42, "octocat"));
            steps.push(get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)));
            steps.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
            steps.extend(principal(43, "octocat"));
        }
        let real = real(steps);
        with_original(&real, |mut attempt| {
            let result = if mode == "failed-close" {
                attempt.resolve(true)
            } else if mode == "foreign-token" {
                let sibling = CancellationToken::default();
                attempt.provider.resolve_thread_guarded_v1(
                    attempt.identity,
                    attempt.actual,
                    attempt.measured,
                    attempt.captures,
                    true,
                    &sibling,
                    attempt.deadline,
                    || panic!("different original token"),
                )
            } else {
                let sibling = provider(&real.fixture);
                sibling.resolve_thread_guarded_v1(
                    attempt.identity,
                    attempt.actual,
                    attempt.measured,
                    attempt.captures,
                    true,
                    attempt.cancel,
                    attempt.deadline,
                    || panic!("foreign actual runner"),
                )
            };
            assert!(matches!(
                result,
                Err(ThreadPublicationFailureV1::NotStarted(_))
            ));
            assert_eq!(attempt.captures.consumed_requests(), 0);
            assert!(attempt.closed.borrow().is_none());
        });
        protocol(&real, 0);
    }
}
fn visibility_steps(final_uid: u64) -> Vec<GhStep> {
    vec![
        get("user", json!({"id":42,"login":"postwrite-opening"})),
        get(&format!("repos/{REPO}"), repo(410, REPO)),
        get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)),
        get("user", json!({"id":final_uid,"login":"same-login-final"})),
    ]
}
#[test]
fn review_threads_v1_guarded_postwrite_final_numeric_uid_and_all_six_fixed_children_are_independent(
) {
    for final_uid in [42, 43] {
        let final_node = selected_node(true, true);
        let mut steps = opening_steps(selected_node(false, true));
        steps.extend(write_guard(42, "octocat"));
        steps.extend(close_steps());
        steps.push(resolution_step(true, final_node.clone()));
        steps.push(state_readback(final_node));
        steps.extend(visibility_steps(final_uid));
        let real = real(steps);
        with_original(&real, |mut attempt| {
            assert!(attempt.resolve(true).is_ok());
            let visible = attempt.visibility();
            assert_eq!(visible.is_ok(), final_uid == 42);
            assert_eq!(attempt.captures.consumed_requests(), 6);
            let closed = attempt.closed.borrow();
            let closed = closed.as_ref().unwrap();
            assert_eq!(
                closed
                    .external_read_accounting()
                    .unwrap()
                    .children_started(),
                8
            );
            assert_eq!(
                closed.postwrite_capture_v2().unwrap().reserved_requests(),
                6
            );
            assert_eq!(
                closed.postwrite_capture_v2().unwrap().reserved_bytes(),
                crate::providers::gh::THREAD_POSTWRITE_CAPS_V2
                    .iter()
                    .sum::<usize>()
            );
        });
        protocol(&real, 1);
    }
}
#[test]
fn review_threads_v1_guarded_partial_comment_pages_cannot_claim_unique_root_or_start_reply() {
    let mut first = selected_node(false, true);
    first["comments"]["totalCount"] = json!(301);
    first["comments"]["pageInfo"] = json!({"hasNextPage":true,"endCursor":"next1"});
    let mut steps = opening_steps(first.clone());
    for (cursor, next, index) in [("next1", "next2", 2), ("next2", "next3", 3)] {
        let mut node = first.clone();
        node["comments"]["pageInfo"]["endCursor"] = json!(next);
        node["comments"]["nodes"][0]["id"] = json!(format!("ACTUAL_REPLY_{index}"));
        node["comments"]["nodes"][0]["fullDatabaseId"] = json!((9010 + index).to_string());
        node["comments"]["nodes"][0]["replyTo"] = json!({"id":"COMMENT_ACTUAL_9"});
        steps.push(graphql_step(
            METRICS_THREAD_COMMENTS_QUERY,
            json!({"id":"THREAD_ACTUAL_9","first":100,"after":cursor}),
            json!({"data":{"node":node}}),
        ));
    }
    let real = real(steps);
    with_original(&real, |mut attempt| {
        assert!(
            !attempt
                .actual
                .thread()
                .comment_coverage
                .enumeration_complete
        );
        assert_eq!(attempt.actual.thread().comments.len(), 3);
        assert!(attempt.actual.original_reply_root().is_err());
        assert!(matches!(
            attempt.reply("must not publish partial-root guess"),
            Err(ThreadPublicationFailureV1::NotStarted(_))
        ));
        assert_eq!(attempt.captures.consumed_requests(), 0);
    });
    protocol(&real, 0);
}
