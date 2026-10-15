//! Source-only genuine producer / strict offline gh declarations. No fixture
//! constructs OriginalThreadCommentV2, and these are NOT execution evidence.
use super::*;
#[path = "scoped_pr_thread_comment_controller_v2_tests.rs"]
mod controller_v2;
#[path = "scoped_pr_thread_historical_provider_v5_tests.rs"]
mod historical_v5;
#[path = "scoped_pr_thread_reactions_v3_tests.rs"]
mod reactions_v3;
#[path = "scoped_pr_thread_comment_guard_v2_tests.rs"]
mod root_source_guards;
use crate::providers::gh::{
    OriginalThreadCommentV2, ThreadCommentMutationV2, ThreadCommentReceiptV2,
    THREAD_COMMENT_ABSENCE_QUERY_V2, THREAD_SELECTED_COMMENT_QUERY_V2,
};

fn selected_comment() -> Value {
    let mut raw = selected_node(false, true)["comments"]["nodes"][0].clone();
    raw["__typename"] = json!("PullRequestReviewComment");
    raw["pullRequest"]["baseRefOid"] = json!("b".repeat(40));
    raw["state"] = json!("SUBMITTED");
    raw["outdated"] = json!(true);
    raw["pullRequestReview"] =
        json!({"id":"ACTUAL_SUBMITTED_REVIEW_600","fullDatabaseId":"600","state":"COMMENTED"});
    raw["viewerCanUpdate"] = json!(true);
    raw["viewerCanDelete"] = json!(true);
    raw["viewerCanReact"] = json!(true);
    raw["viewerDidAuthor"] = json!(false);
    raw
}
fn rest_comment(body: &str, updated: &str) -> Value {
    json!({"id":9009,"node_id":"COMMENT_ACTUAL_9","pull_request_review_id":600,
        "pull_request_url":format!("https://{HOST}/repos/{REPO}/pulls/7"),
        "user":{"id":51,"login":"renamed-observed-author","type":"User","avatar_url":null},
        "body":body,"created_at":"2026-10-01T00:00:00Z","updated_at":updated,
        "path":"src/old.rs","commit_id":"c".repeat(40),"original_commit_id":"d".repeat(40)})
}
fn observation(raw: Value, rest: Value) -> Vec<GhStep> {
    let mut steps = write_guard(42, "renamed-same-numeric-viewer");
    steps.push(graphql_step(
        THREAD_SELECTED_COMMENT_QUERY_V2,
        json!({"id":"COMMENT_ACTUAL_9"}),
        json!({"data":{"node":raw}}),
    ));
    steps.push(get(&format!("repos/{REPO}/pulls/comments/9009"), rest));
    steps
}
fn observe(
    a: &AttemptV2<'_>,
) -> Result<OriginalThreadCommentV2, crate::providers::gh::GithubError> {
    a.measured.original_thread_comment_v2(
        a.identity,
        a.actual.clone(),
        "COMMENT_ACTUAL_9",
        9009,
        a.cancel,
        a.deadline,
    )
}
fn exact_protocol(real: &Real, writes: usize) {
    assert_eq!(sql_state(&real.fixture), real.sql);
    assert_eq!(git_state_resource(&real.fixture), real.git);
    let rows = fs::read_to_string(real.fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|row| serde_json::from_str::<Value>(row).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), real.steps.len());
    let mut actual_writes = 0;
    for (row, step) in rows.iter().zip(&real.steps) {
        assert_eq!(row["matched"], true);
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(
            row["stdin_hex"],
            step.stdin
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        if step
            .args
            .iter()
            .any(|arg| matches!(arg.as_str(), "PATCH" | "DELETE"))
        {
            actual_writes += 1;
            assert_eq!(step.args[1], format!("repos/{REPO}/pulls/comments/9009"));
            if step.args.iter().any(|arg| arg == "DELETE") {
                assert!(step.args.iter().any(|arg| arg == "--include"));
            }
        } else if step.args.iter().any(|arg| arg == "POST") {
            assert_eq!(step.args[1], "graphql");
            let value: Value = serde_json::from_slice(&step.stdin).unwrap();
            assert!(value["query"].as_str().unwrap().starts_with("query("));
        }
    }
    assert_eq!(actual_writes, writes);
}
fn current_rest() -> Value {
    rest_comment(BODY, "2026-10-02T00:00:00Z")
}
fn mutation_guard() -> Vec<GhStep> {
    let mut steps = write_guard(42, "renamed-viewer");
    steps.push(get(
        &format!("repos/{REPO}/pulls/comments/9009"),
        current_rest(),
    ));
    steps
}
fn two_observations() -> Vec<GhStep> {
    let mut steps = opening_steps(selected_node(false, true));
    steps.extend(observation(selected_comment(), current_rest()));
    steps.extend(observation(selected_comment(), current_rest()));
    steps
}
fn delete_ack(output: &str) -> GhStep {
    GhStep::read(
        &[
            "api",
            &format!("repos/{REPO}/pulls/comments/9009"),
            "--hostname",
            HOST,
            "--method",
            "DELETE",
            "--include",
        ],
        output,
    )
}
fn absent(value: Value) -> GhStep {
    graphql_step(
        THREAD_COMMENT_ABSENCE_QUERY_V2,
        json!({"id":"COMMENT_ACTUAL_9"}),
        value,
    )
}
fn visibility_steps() -> Vec<GhStep> {
    vec![
        get("user", json!({"id":42,"login":"fresh-current"})),
        get(&format!("repos/{REPO}"), repo(410, REPO)),
        get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)),
        get("user", json!({"id":42,"login":"fresh-final"})),
    ]
}

#[test]
fn review_threads_v1_comment_actual_factory_keeps_author_distinct_from_moderator_and_c_distinct_h()
{
    let mut steps = opening_steps(selected_node(false, true));
    steps.extend(observation(selected_comment(), current_rest()));
    steps.extend(close_steps());
    let real = real(steps);
    with_original_limits(&real, 20, |a| {
        let actual = observe(&a).unwrap();
        assert_eq!(actual.principal(), 42);
        assert_eq!(actual.author_account_id(), Some(51));
        assert_eq!(actual.viewer_did_author(), Some(false));
        assert_eq!(actual.viewer_can_update(), Some(true));
        assert_eq!(actual.viewer_can_delete(), Some(true));
        assert_eq!(actual.viewer_can_react(), Some(true));
        assert_eq!(
            actual.comment().commit_sha.as_deref(),
            Some("c".repeat(40).as_str())
        );
        assert_eq!(actual.original().parent().head_sha(), "a".repeat(40));
        assert_eq!(actual.comment().body.as_deref(), Some(BODY));
        assert_eq!(actual.review_id(), 600);
        assert_eq!(actual.outdated(), Some(true));
        (*a.close)().unwrap();
        assert_eq!(a.captures.consumed_requests(), 0);
    });
    exact_protocol(&real, 0);
}
#[test]
fn review_threads_v1_comment_factory_rejects_wrong_node_id_base_repo_author_and_graphql_errors_beside_data(
) {
    for field in [
        "node", "rest_id", "base", "repo", "author", "errors", "pending",
    ] {
        let mut raw = selected_comment();
        let mut value = json!({"data":{"node":raw.clone()}});
        match field {
            "node" => raw["id"] = json!("WRONG_COMMENT_NODE"),
            "rest_id" => raw["fullDatabaseId"] = json!("9008"),
            "base" => raw["pullRequest"]["baseRefOid"] = json!("e".repeat(40)),
            "repo" => raw["repository"]["databaseId"] = json!(999),
            "author" => raw["author"]["databaseId"] = json!(52),
            "pending" => raw["state"] = json!("PENDING"),
            _ => value["errors"] = json!([{"message":"permission denied beside data"}]),
        }
        value["data"]["node"] = raw;
        let mut steps = opening_steps(selected_node(false, true));
        steps.extend(write_guard(42, "same-viewer"));
        steps.push(graphql_step(
            THREAD_SELECTED_COMMENT_QUERY_V2,
            json!({"id":"COMMENT_ACTUAL_9"}),
            value,
        ));
        let real = real(steps);
        with_original_limits(&real, 20, |a| {
            assert!(observe(&a).is_err(), "{field}");
            assert_eq!(a.captures.consumed_requests(), 0);
        });
        exact_protocol(&real, 0);
    }
}
#[test]
fn review_threads_v1_comment_factory_numeric_account_loss_refuses_before_private_comment_reads() {
    let mut steps = opening_steps(selected_node(false, true));
    steps.push(get("user", json!({"id":43,"login":"octocat"})));
    let real = real(steps);
    with_original_limits(&real, 20, |a| {
        assert!(observe(&a).is_err());
        assert_eq!(a.captures.consumed_requests(), 0);
    });
    exact_protocol(&real, 0);
}
#[test]
fn review_threads_v1_comment_factory_rejects_stale_body_time_original_commit_path_and_rest_parent()
{
    for field in ["body", "time", "commit", "path", "parent", "author"] {
        let mut rest = current_rest();
        match field {
            "body" => rest["body"] = json!("changed elsewhere"),
            "time" => rest["updated_at"] = json!("2026-10-03T00:00:00Z"),
            "commit" => rest["commit_id"] = json!("a".repeat(40)),
            "path" => rest["path"] = json!("src/new.rs"),
            "author" => rest["user"]["id"] = json!(52),
            _ => rest["pull_request_url"] = json!(format!("https://{HOST}/repos/{REPO}/pulls/8")),
        }
        let mut steps = opening_steps(selected_node(false, true));
        steps.extend(observation(selected_comment(), rest));
        let real = real(steps);
        with_original_limits(&real, 20, |a| {
            assert!(observe(&a).is_err(), "{field}");
        });
        exact_protocol(&real, 0);
    }
}
#[test]
fn review_threads_v1_comment_edit_uses_actual_fresh_capability_and_one_patch_exact_readback_visibility(
) {
    let edited = "edited exact submitted body 界\nsecond line";
    let mut steps = two_observations();
    steps.extend(mutation_guard());
    steps.extend(close_steps());
    let mut patch = GhStep::read(
        &[
            "api",
            &format!("repos/{REPO}/pulls/comments/9009"),
            "--hostname",
            HOST,
            "--method",
            "PATCH",
            "--input",
            "-",
        ],
        &rest_comment(edited, "2026-10-03T00:00:00Z").to_string(),
    );
    patch.stdin = serde_json::to_vec(&json!({"body":edited})).unwrap();
    steps.push(patch);
    steps.push(get(
        &format!("repos/{REPO}/pulls/comments/9009"),
        rest_comment(edited, "2026-10-03T00:00:00Z"),
    ));
    steps.extend(visibility_steps());
    let real = real(steps);
    with_original_limits(&real, 20, |mut a| {
        let expected = observe(&a).unwrap();
        let fresh = observe(&a).unwrap();
        let result = a
            .provider
            .mutate_thread_comment_guarded_v2(
                a.identity,
                &expected,
                &fresh,
                a.measured,
                a.captures,
                ThreadCommentMutationV2::Edit(edited),
                a.cancel,
                a.deadline,
                &mut *a.close,
            )
            .unwrap();
        let ThreadCommentReceiptV2::Edited(receipt) = result else {
            panic!("actual edit receipt required")
        };
        assert_eq!(receipt.comment().id, 9009);
        assert_eq!(receipt.comment().body, edited);
        assert_eq!(receipt.comment().author.login, "renamed-observed-author");
        a.visibility().unwrap();
        assert_eq!(a.captures.consumed_requests(), 6);
    });
    exact_protocol(&real, 1);
}
#[test]
fn review_threads_v1_comment_delete_requires_exact_204_and_node_null_then_same_original_visibility()
{
    let mut steps = two_observations();
    steps.extend(mutation_guard());
    steps.extend(close_steps());
    steps.push(delete_ack("HTTP/2.0 204 No Content\nContent-Length: 0\n\n"));
    steps.push(absent(json!({"data":{"node":null}})));
    steps.extend(visibility_steps());
    let real = real(steps);
    with_original_limits(&real, 20, |mut a| {
        let expected = observe(&a).unwrap();
        let fresh = observe(&a).unwrap();
        let result = a
            .provider
            .mutate_thread_comment_guarded_v2(
                a.identity,
                &expected,
                &fresh,
                a.measured,
                a.captures,
                ThreadCommentMutationV2::Delete,
                a.cancel,
                a.deadline,
                &mut *a.close,
            )
            .unwrap();
        let ThreadCommentReceiptV2::Deleted(receipt) = result else {
            panic!("closed exact delete receipt required")
        };
        assert_eq!(receipt.comment_id(), 9009);
        assert_eq!(receipt.comment_node(), "COMMENT_ACTUAL_9");
        a.visibility().unwrap();
        assert_eq!(a.captures.consumed_requests(), 6);
    });
    exact_protocol(&real, 1);
}
#[test]
fn review_threads_v1_comment_delete_204_alone_null_errors_missing_or_remaining_node_stays_unknown_no_resend(
) {
    for value in [
        json!({"data":{"node":null},"errors":[{"message":"denied"}]}),
        json!({"data":{}}),
        json!({"data":{"node":{"id":"COMMENT_ACTUAL_9"}}}),
    ] {
        let mut steps = two_observations();
        steps.extend(mutation_guard());
        steps.extend(close_steps());
        steps.push(delete_ack("HTTP/1.1 204 No Content\r\n\r\n"));
        steps.push(absent(value));
        let real = real(steps);
        with_original_limits(&real, 20, |a| {
            let expected = observe(&a).unwrap();
            let fresh = observe(&a).unwrap();
            assert!(matches!(
                a.provider.mutate_thread_comment_guarded_v2(
                    a.identity,
                    &expected,
                    &fresh,
                    a.measured,
                    a.captures,
                    ThreadCommentMutationV2::Delete,
                    a.cancel,
                    a.deadline,
                    &mut *a.close
                ),
                Err(ThreadPublicationFailureV1::WriteMayHaveStarted(_))
            ));
            assert_eq!(a.captures.consumed_requests(), 2);
        });
        exact_protocol(&real, 1);
    }
}
#[test]
fn review_threads_v1_comment_delete_malformed_or_404_ack_stops_without_null_query_or_retry() {
    for output in [
        "HTTP/2.0 200 OK\n\n",
        "HTTP/2.0 404 Not Found\n\n",
        "HTTP/2.0 204 No Content\n\nnull",
    ] {
        let mut steps = two_observations();
        steps.extend(mutation_guard());
        steps.extend(close_steps());
        steps.push(delete_ack(output));
        let real = real(steps);
        with_original_limits(&real, 20, |a| {
            let expected = observe(&a).unwrap();
            let fresh = observe(&a).unwrap();
            assert!(matches!(
                a.provider.mutate_thread_comment_guarded_v2(
                    a.identity,
                    &expected,
                    &fresh,
                    a.measured,
                    a.captures,
                    ThreadCommentMutationV2::Delete,
                    a.cancel,
                    a.deadline,
                    &mut *a.close
                ),
                Err(ThreadPublicationFailureV1::WriteMayHaveStarted(_))
            ));
            assert_eq!(a.captures.consumed_requests(), 1);
        });
        exact_protocol(&real, 1);
    }
}
#[test]
fn review_threads_v1_comment_false_unknown_capability_and_expired_or_foreign_token_do_not_start_write(
) {
    for capability in [Some(false), None] {
        let mut raw = selected_comment();
        raw["viewerCanUpdate"] = json!(capability);
        raw["viewerCanDelete"] = json!(capability);
        let mut steps = opening_steps(selected_node(false, true));
        steps.extend(observation(raw, current_rest()));
        let real = real(steps);
        with_original_limits(&real, 20, |a| {
            let actual = observe(&a).unwrap();
            for mutation in [
                ThreadCommentMutationV2::Edit("changed"),
                ThreadCommentMutationV2::Delete,
            ] {
                assert!(matches!(
                    a.provider.mutate_thread_comment_guarded_v2(
                        a.identity,
                        &actual,
                        &actual,
                        a.measured,
                        a.captures,
                        mutation,
                        a.cancel,
                        a.deadline,
                        &mut *a.close
                    ),
                    Err(ThreadPublicationFailureV1::NotStarted(_))
                ));
            }
            let foreign = CancellationToken::default();
            assert!(a
                .measured
                .original_thread_comment_v2(
                    a.identity,
                    a.actual.clone(),
                    "COMMENT_ACTUAL_9",
                    9009,
                    &foreign,
                    a.deadline
                )
                .is_err());
            assert!(a
                .measured
                .original_thread_comment_v2(
                    a.identity,
                    a.actual.clone(),
                    "COMMENT_ACTUAL_9",
                    9009,
                    a.cancel,
                    Instant::now()
                )
                .is_err());
            a.cancel.cancel();
            assert!(observe(&a).is_err());
            assert_eq!(a.captures.consumed_requests(), 0);
        });
        exact_protocol(&real, 0);
    }
}
#[test]
fn review_threads_v1_comment_failed_original_close_activates_no_postwrite_slot() {
    let mut steps = two_observations();
    steps.extend(mutation_guard());
    let real = real(steps);
    with_original_limits(&real, 20, |a| {
        let expected = observe(&a).unwrap();
        let fresh = observe(&a).unwrap();
        assert!(matches!(
            a.provider.mutate_thread_comment_guarded_v2(
                a.identity,
                &expected,
                &fresh,
                a.measured,
                a.captures,
                ThreadCommentMutationV2::Delete,
                a.cancel,
                a.deadline,
                || Err(missing_original_fixture())
            ),
            Err(ThreadPublicationFailureV1::NotStarted(_))
        ));
        assert_eq!(a.captures.consumed_requests(), 0);
        assert!(a.closed.borrow().is_none());
    });
    exact_protocol(&real, 0);
}
#[test]
fn review_threads_v1_comment_nonroot_first_root_last_uses_actual_numeric_reply_parent_not_row_order(
) {
    let mut node = selected_node(false, true);
    let root = node["comments"]["nodes"][0].clone();
    let mut child = root.clone();
    child["id"] = json!("COMMENT_ACTUAL_8");
    child["fullDatabaseId"] = json!("9008");
    child["body"] = json!("actual selected nonroot");
    child["replyTo"] = json!({"id":"COMMENT_ACTUAL_9"});
    node["comments"]["nodes"] = json!([child, root]);
    node["comments"]["totalCount"] = json!(2);
    let mut raw = selected_comment();
    raw["id"] = json!("COMMENT_ACTUAL_8");
    raw["fullDatabaseId"] = json!("9008");
    raw["body"] = json!("actual selected nonroot");
    raw["replyTo"] = json!({"id":"COMMENT_ACTUAL_9","fullDatabaseId":"9009"});
    let mut rest = rest_comment("actual selected nonroot", "2026-10-02T00:00:00Z");
    rest["id"] = json!(9008);
    rest["node_id"] = json!("COMMENT_ACTUAL_8");
    rest["in_reply_to_id"] = json!(9009);
    let mut steps = opening_steps(node);
    steps.extend(write_guard(42, "current"));
    steps.push(graphql_step(
        THREAD_SELECTED_COMMENT_QUERY_V2,
        json!({"id":"COMMENT_ACTUAL_8"}),
        json!({"data":{"node":raw}}),
    ));
    steps.push(get(&format!("repos/{REPO}/pulls/comments/9008"), rest));
    steps.extend(close_steps());
    let real = real(steps);
    with_original_limits(&real, 20, |a| {
        let actual = a
            .measured
            .original_thread_comment_v2(
                a.identity,
                a.actual.clone(),
                "COMMENT_ACTUAL_8",
                9008,
                a.cancel,
                a.deadline,
            )
            .unwrap();
        assert_eq!(actual.comment().node_id, "COMMENT_ACTUAL_8");
        assert_eq!(
            actual.original().original_reply_root().unwrap().database_id,
            Some(9009)
        );
        assert_eq!(
            actual.comment().reply_to,
            crate::providers::gh::MetricsThreadReplyTo::Reply("COMMENT_ACTUAL_9".into())
        );
        (*a.close)().unwrap();
    });
    exact_protocol(&real, 0);
}
#[test]
fn review_threads_v1_comment_actual_sibling_producer_arc_and_original_token_cannot_be_adopted() {
    let original_real = real(opening_steps(selected_node(false, true)));
    let (original, token) = with_original(&original_real, |a| (a.actual.clone(), a.cancel.clone()));
    exact_protocol(&original_real, 0);
    let sibling_real = real(opening_steps(selected_node(false, true)));
    with_original(&sibling_real, |a| {
        assert!(a
            .measured
            .original_thread_comment_v2(
                a.identity,
                original,
                "COMMENT_ACTUAL_9",
                9009,
                &token,
                a.deadline
            )
            .is_err());
        assert_eq!(a.captures.consumed_requests(), 0);
    });
    exact_protocol(&sibling_real, 0);
}
#[test]
fn review_threads_v1_comment_actual_confirmed_delete_then_final_uid_loss_is_not_private_visibility()
{
    let mut steps = two_observations();
    steps.extend(mutation_guard());
    steps.extend(close_steps());
    steps.push(delete_ack("HTTP/2.0 204 No Content\n\n"));
    steps.push(absent(json!({"data":{"node":null}})));
    let mut final_steps = visibility_steps();
    final_steps[3] = get("user", json!({"id":43,"login":"octocat"}));
    steps.extend(final_steps);
    let real = real(steps);
    with_original_limits(&real, 20, |mut a| {
        let expected = observe(&a).unwrap();
        let fresh = observe(&a).unwrap();
        assert!(matches!(
            a.provider.mutate_thread_comment_guarded_v2(
                a.identity,
                &expected,
                &fresh,
                a.measured,
                a.captures,
                ThreadCommentMutationV2::Delete,
                a.cancel,
                a.deadline,
                &mut *a.close
            ),
            Ok(ThreadCommentReceiptV2::Deleted(_))
        ));
        assert!(a.visibility().is_err());
        assert_eq!(a.captures.consumed_requests(), 6);
    });
    exact_protocol(&real, 1);
}
#[test]
fn review_threads_v1_comment_actual_large_review_id_uses_full_database_id_without_narrowing_or_decoding(
) {
    let id = 9_007_199_254_740_993u64;
    let mut raw = selected_comment();
    raw["pullRequestReview"]["fullDatabaseId"] = json!(id.to_string());
    let mut rest = current_rest();
    rest["pull_request_review_id"] = json!(id);
    let mut steps = opening_steps(selected_node(false, true));
    steps.extend(observation(raw, rest));
    steps.extend(close_steps());
    let real = real(steps);
    with_original_limits(&real, 20, |a| {
        let actual = observe(&a).unwrap();
        assert_eq!(actual.review_id(), id);
        (*a.close)().unwrap();
    });
    exact_protocol(&real, 0);
}
#[test]
fn review_threads_v1_comment_dedicated_coordinates_require_explicit_nullable_positive_exact_observations(
) {
    for field in ["line", "startLine", "originalLine", "originalStartLine"] {
        for value in [
            json!(0),
            json!(-1),
            json!("7"),
            json!(i32::MAX as u64 + 1),
            json!(8),
        ] {
            let mut raw = selected_comment();
            raw[field] = value;
            let mut steps = opening_steps(selected_node(false, true));
            steps.extend(write_guard(42, "current"));
            steps.push(graphql_step(
                THREAD_SELECTED_COMMENT_QUERY_V2,
                json!({"id":"COMMENT_ACTUAL_9"}),
                json!({"data":{"node":raw}}),
            ));
            let real = real(steps);
            with_original_limits(&real, 20, |a| {
                assert!(observe(&a).is_err(), "{field}");
            });
            exact_protocol(&real, 0);
        }
    }
    let mut raw = selected_comment();
    raw.as_object_mut().unwrap().remove("line");
    let mut steps = opening_steps(selected_node(false, true));
    steps.extend(write_guard(42, "current"));
    steps.push(graphql_step(
        THREAD_SELECTED_COMMENT_QUERY_V2,
        json!({"id":"COMMENT_ACTUAL_9"}),
        json!({"data":{"node":raw}}),
    ));
    let real = real(steps);
    with_original_limits(&real, 20, |a| {
        assert!(observe(&a).is_err());
    });
    exact_protocol(&real, 0);
}
#[test]
fn review_threads_v1_comment_actual_root_rest_parent_requires_absent_or_explicit_null_not_malformed(
) {
    for value in [
        json!("not-a-number"),
        json!(0),
        json!(-1),
        json!({}),
        json!(true),
    ] {
        let mut rest = current_rest();
        rest["in_reply_to_id"] = value;
        let mut steps = opening_steps(selected_node(false, true));
        steps.extend(observation(selected_comment(), rest));
        let real = real(steps);
        with_original_limits(&real, 20, |a| {
            assert!(observe(&a).is_err());
        });
        exact_protocol(&real, 0);
    }
}
