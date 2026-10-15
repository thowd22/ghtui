//! Independent regressions for the dedicated selected-node/REST bridge.
//! Both sources come through the real bounded producer and strict fake-gh.
use super::*;

fn selected_with_review_bigint() -> Value {
    let mut raw = selected_comment();
    if raw["pullRequestReview"].get("fullDatabaseId").is_none() {
        raw["pullRequestReview"]["fullDatabaseId"] = raw["pullRequestReview"]["databaseId"].clone();
    }
    raw
}

#[test]
fn review_threads_v1_comment_root_regression_full_width_review_id_is_the_actual_rest_parent() {
    const REVIEW_ID: u64 = 9_007_199_254_740_993;
    let mut raw = selected_with_review_bigint();
    raw["pullRequestReview"]
        .as_object_mut()
        .unwrap()
        .remove("databaseId");
    raw["pullRequestReview"]["fullDatabaseId"] = json!(REVIEW_ID.to_string());
    let mut rest = current_rest();
    rest["pull_request_review_id"] = json!(REVIEW_ID);
    let mut steps = opening_steps(selected_node(false, true));
    steps.extend(observation(raw, rest));
    steps.extend(close_steps());
    let real = real(steps);
    with_original_limits(&real, 20, |a| {
        let selected = observe(&a)
            .expect("actual full-width review identity must not require deprecated Int databaseId");
        assert_eq!(selected.review_id(), REVIEW_ID);
        assert_eq!(selected.comment().database_id, Some(9009));
        (*a.close)().unwrap();
        assert_eq!(a.captures.consumed_requests(), 0);
    });
    exact_protocol(&real, 0);
}

#[test]
fn review_threads_v1_comment_root_regression_dedicated_position_refuses_before_rest_bridge() {
    for field in ["line", "startLine", "originalLine", "originalStartLine"] {
        for value in [
            None,
            Some(json!(0)),
            Some(json!(-1)),
            Some(json!("7")),
            Some(json!(true)),
            Some(json!(u64::MAX)),
            Some(json!(999)),
        ] {
            let mut raw = selected_with_review_bigint();
            if let Some(value) = value {
                raw[field] = value;
            } else {
                raw.as_object_mut().unwrap().remove(field);
            }
            let mut steps = opening_steps(selected_node(false, true));
            steps.extend(write_guard(42, "renamed-same-numeric-viewer"));
            steps.push(graphql_step(
                THREAD_SELECTED_COMMENT_QUERY_V2,
                json!({"id":"COMMENT_ACTUAL_9"}),
                json!({"data":{"node":raw}}),
            ));
            let real = real(steps);
            with_original_limits(&real, 20, |a| {
                let failure = observe(&a)
                    .expect_err("contradictory/malformed selected position must fail closed");
                assert!(
                    matches!(
                        failure.kind,
                        GithubErrorKind::StaleHead
                            | GithubErrorKind::Json
                            | GithubErrorKind::IdentityChanged
                            | GithubErrorKind::Unsupported
                    ),
                    "{field}: must reject source before any unplanned REST read: {failure:?}"
                );
                assert_eq!(a.captures.consumed_requests(), 0);
            });
            exact_protocol(&real, 0);
        }
    }
}

#[test]
fn review_threads_v1_comment_root_regression_malformed_root_parent_is_not_absent() {
    for value in [
        json!("9009"),
        json!(false),
        json!({"id":9009}),
        json!([]),
        json!(0),
        json!(-1),
        json!(9009),
    ] {
        let mut rest = current_rest();
        rest["in_reply_to_id"] = value;
        let mut steps = opening_steps(selected_node(false, true));
        steps.extend(observation(selected_with_review_bigint(), rest));
        let real = real(steps);
        with_original_limits(&real, 20, |a| {
            let failure = observe(&a)
                .expect_err("a root cannot adopt a non-null malformed or numeric reply parent");
            assert!(
                matches!(failure.kind, GithubErrorKind::IdentityChanged),
                "{failure:?}"
            );
            assert_eq!(a.captures.consumed_requests(), 0);
        });
        exact_protocol(&real, 0);
    }
}
