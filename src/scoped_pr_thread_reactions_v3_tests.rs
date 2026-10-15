//! Genuine selected producer, strict commands, bounded original accounting.
//! Source declarations only; Root alone executes all verification.
use super::*;
use crate::providers::gh::{
    OriginalThreadReactionsV3, ThreadReactionContentV3 as Content,
    ThreadReactionMutationV3 as Mutation, ThreadReactionReceiptV3 as Receipt,
    THREAD_REACTIONS_POSTWRITE_CAPS_V3,
};

fn row(id: u64, uid: Option<u64>, content: &str) -> Value {
    json!({"id":id,"node_id":format!("ACTUAL_REACTION_NODE_{id}"),"content":content,
        "user":uid.map(|uid|json!({"id":uid,"login":"same-login-not-ownership","type":"User","avatar_url":null})),
        "created_at":"2026-10-03T00:00:00Z"})
}
fn endpoint(page: usize) -> String {
    format!("repos/{REPO}/pulls/comments/9009/reactions?per_page=100&page={page}")
}
fn list_steps(pages: Vec<Value>) -> Vec<GhStep> {
    let mut steps = write_guard(42, "renamed-numeric-viewer");
    steps.push(get(
        &format!("repos/{REPO}/pulls/comments/9009"),
        current_rest(),
    ));
    steps.extend(
        pages
            .into_iter()
            .enumerate()
            .map(|(i, value)| get(&endpoint(i + 1), value)),
    );
    steps
}
fn original_steps() -> Vec<GhStep> {
    let mut steps = opening_steps(selected_node(false, true));
    steps.extend(observation(selected_comment(), current_rest()));
    steps
}
fn reaction_original(a: &AttemptV2<'_>) -> Arc<OriginalThreadCommentV2> {
    Arc::new(observe(a).unwrap())
}
fn actors(
    a: &AttemptV2<'_>,
    comment: Arc<OriginalThreadCommentV2>,
) -> Result<OriginalThreadReactionsV3, GithubError> {
    a.measured
        .original_thread_reactions_v3(a.identity, comment, a.cancel, a.deadline)
}
fn page100(offset: u64) -> Value {
    json!((offset..offset + 100)
        .map(|id| row(id, Some(id + 1000), "eyes"))
        .collect::<Vec<_>>())
}
fn http(add: bool, status: u16, body: Option<&Value>, id: u64) -> GhStep {
    let mut args = vec![
        "api".to_owned(),
        format!(
            "repos/{REPO}/pulls/comments/9009/reactions{}",
            if add { String::new() } else { format!("/{id}") }
        ),
        "--hostname".into(),
        HOST.into(),
        "--method".into(),
        if add { "POST".into() } else { "DELETE".into() },
        "--include".into(),
    ];
    if add {
        args.extend(["--input".into(), "-".into()]);
    }
    let data = body.map_or_else(String::new, |body| serde_json::to_string(body).unwrap());
    // Exact pinned gh mixed status LF/header CRLF, not a uniform fake HTTP line.
    let output = format!(
        "HTTP/2.0 {status} Observed\nContent-Length: {}\r\n\r\n{data}",
        data.len()
    );
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    let mut step = GhStep::read(&refs, &output);
    if add {
        step.stdin = serde_json::to_vec(&json!({"content":"heart"})).unwrap();
    }
    step
}
fn protocol(real: &Real, writes: usize) {
    assert_eq!(sql_state(&real.fixture), real.sql);
    assert_eq!(git_state_resource(&real.fixture), real.git);
    let rows: Vec<Value> = fs::read_to_string(real.fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), real.steps.len());
    let mut actual = 0;
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
        if step.args.iter().any(|arg| arg == "POST" || arg == "DELETE")
            && step.args[1].contains("/reactions")
        {
            actual += 1;
            assert!(step.args.iter().any(|arg| arg == "--include"));
            assert!(
                step.args[1].starts_with(&format!("repos/{REPO}/pulls/comments/9009/reactions"))
            );
        } else if step.args.iter().any(|arg| arg == "POST") {
            assert_eq!(step.args[1], "graphql");
            assert!(
                serde_json::from_slice::<Value>(&step.stdin).unwrap()["query"]
                    .as_str()
                    .unwrap()
                    .starts_with("query(")
            );
        }
    }
    assert_eq!(actual, writes);
}
#[test]
fn review_threads_v3_reaction_factory_actual_actors_unknown_deleted_and_independent_comment_c() {
    let mut steps = original_steps();
    steps.extend(list_steps(vec![json!([
        row(7001, Some(42), "heart"),
        row(7002, Some(99), "eyes"),
        row(7003, None, "rocket")
    ])]));
    steps.extend(close_steps());
    let real = real(steps);
    with_original_caps_v3(&real, 24, &THREAD_REACTIONS_POSTWRITE_CAPS_V3, |a| {
        let comment = reaction_original(&a);
        let actual = actors(&a, comment).unwrap();
        assert_eq!(actual.rows().len(), 3);
        assert!(actual.complete());
        assert_eq!(actual.pages(), 1);
        assert_eq!(actual.selected(7001).unwrap().account_id(), Some(42));
        assert_eq!(actual.selected(7002).unwrap().account_id(), Some(99));
        assert!(actual.selected(7003).unwrap().actor().is_none());
        assert_eq!(actual.comment().author_account_id(), Some(51));
        assert_eq!(
            actual.comment().comment().commit_sha.as_deref(),
            Some("c".repeat(40).as_str())
        );
        assert_eq!(
            actual.comment().original().parent().head_sha(),
            "a".repeat(40)
        );
        (*a.close)().unwrap();
        assert_eq!(a.captures.consumed_requests(), 0);
    });
    protocol(&real, 0);
}
#[test]
fn review_threads_v3_reaction_factory_partial_four_pages_is_not_absence_and_duplicate_refuses() {
    for duplicate in [false, true] {
        let mut pages = vec![page100(1), page100(101), page100(201), page100(301)];
        if duplicate {
            pages[1][0]["id"] = json!(1);
            pages.truncate(2);
        }
        let mut steps = original_steps();
        steps.extend(list_steps(pages));
        if !duplicate {
            steps.extend(close_steps());
        }
        let real = real(steps);
        with_original_caps_v3(&real, 28, &THREAD_REACTIONS_POSTWRITE_CAPS_V3, |a| {
            let result = actors(&a, reaction_original(&a));
            if duplicate {
                assert!(result.is_err());
            } else {
                let actual = result.unwrap();
                assert_eq!(actual.rows().len(), 400);
                assert_eq!(actual.pages(), 4);
                assert!(!actual.complete());
                (*a.close)().unwrap();
            }
            assert_eq!(a.captures.consumed_requests(), 0);
        });
        protocol(&real, 0);
    }
}
#[test]
fn review_threads_v3_reaction_factory_rejects_malformed_actor_id_content_time_and_account_loss() {
    for field in [
        "id",
        "node",
        "content",
        "time",
        "actor",
        "duplicate-node",
        "account",
    ] {
        let mut value = row(7001, Some(42), "heart");
        match field {
            "id" => value["id"] = json!(0),
            "node" => value["node_id"] = json!("\u{1b}[2J"),
            "content" => value["content"] = json!("unknown"),
            "time" => value["created_at"] = json!("invalid"),
            "actor" => value["user"]["id"] = json!(0),
            _ => {}
        }
        let mut steps = original_steps();
        if field == "account" {
            steps.push(get("user", json!({"id":43,"login":"octocat"})));
        } else {
            let page = if field == "duplicate-node" {
                let mut other = row(7002, Some(77), "eyes");
                other["node_id"] = value["node_id"].clone();
                json!([value, other])
            } else {
                json!([value])
            };
            steps.extend(list_steps(vec![page]));
        }
        let real = real(steps);
        with_original_caps_v3(&real, 24, &THREAD_REACTIONS_POSTWRITE_CAPS_V3, |a| {
            assert!(actors(&a, reaction_original(&a)).is_err(), "{field}");
            assert_eq!(a.captures.consumed_requests(), 0);
        });
        protocol(&real, 0);
    }
}
#[test]
fn review_threads_v3_reaction_add_existing200_and_created201_require_actual_id_actor_readback_same_cursor(
) {
    for status in [200, 201] {
        let actual = row(7001, Some(42), "heart");
        let mut steps = original_steps();
        steps.extend(list_steps(vec![json!([])]));
        steps.extend(list_steps(vec![json!([])]));
        steps.extend(mutation_guard());
        steps.extend(close_steps());
        steps.push(http(true, status, Some(&actual), 0));
        steps.push(get(&endpoint(1), json!([actual])));
        steps.extend(visibility_steps());
        let real = real(steps);
        with_original_caps_v3(&real, 40, &THREAD_REACTIONS_POSTWRITE_CAPS_V3, |mut a| {
            let comment = reaction_original(&a);
            let expected = actors(&a, comment.clone()).unwrap();
            let fresh = actors(&a, comment).unwrap();
            let receipt = a
                .provider
                .mutate_thread_reaction_guarded_v3(
                    a.identity,
                    &expected,
                    &fresh,
                    a.measured,
                    a.captures,
                    Mutation::Add(Content::Heart),
                    a.cancel,
                    a.deadline,
                    &mut *a.close,
                )
                .unwrap();
            let Receipt::Added(receipt) = receipt else {
                panic!("closed add receipt")
            };
            assert_eq!(receipt.reaction().id(), 7001);
            assert_eq!(receipt.reaction().account_id(), Some(42));
            assert_eq!(receipt.already_existed(), status == 200);
            assert!(receipt.readback_complete());
            assert_eq!(a.captures.reserved_requests(), 9);
            assert_eq!(a.captures.consumed_requests(), 2);
            a.visibility().unwrap();
            assert_eq!(a.captures.consumed_requests(), 6);
        });
        protocol(&real, 1);
    }
}
#[test]
fn review_threads_v3_reaction_remove_requires_actual_numeric_owned_id_exact204_and_complete_absence(
) {
    let actual = row(7001, Some(42), "heart");
    let mut steps = original_steps();
    steps.extend(list_steps(vec![json!([actual.clone()])]));
    steps.extend(list_steps(vec![json!([actual])]));
    steps.extend(mutation_guard());
    steps.extend(close_steps());
    steps.push(http(false, 204, None, 7001));
    steps.push(get(&endpoint(1), json!([row(7002, Some(99), "heart")])));
    steps.extend(visibility_steps());
    let real = real(steps);
    with_original_caps_v3(&real, 40, &THREAD_REACTIONS_POSTWRITE_CAPS_V3, |mut a| {
        let comment = reaction_original(&a);
        let expected = actors(&a, comment.clone()).unwrap();
        let fresh = actors(&a, comment).unwrap();
        let receipt = a
            .provider
            .mutate_thread_reaction_guarded_v3(
                a.identity,
                &expected,
                &fresh,
                a.measured,
                a.captures,
                Mutation::Remove(7001),
                a.cancel,
                a.deadline,
                &mut *a.close,
            )
            .unwrap();
        let Receipt::Removed(receipt) = receipt else {
            panic!("closed removal")
        };
        assert_eq!(receipt.reaction_id(), 7001);
        assert_eq!(receipt.reaction_node(), "ACTUAL_REACTION_NODE_7001");
        assert_eq!(receipt.account_id(), 42);
        assert_eq!(receipt.comment_id(), 9009);
        assert_eq!(a.captures.consumed_requests(), 2);
        a.visibility().unwrap();
        assert_eq!(a.captures.consumed_requests(), 6);
    });
    protocol(&real, 1);
}
#[test]
fn review_threads_v3_reaction_remove_foreign_same_login_and_unknown_permission_refuses_before_close_or_post(
) {
    for foreign in [false, true] {
        let mut raw = selected_comment();
        if !foreign {
            raw["viewerCanReact"] = Value::Null;
        }
        let mut steps = opening_steps(selected_node(false, true));
        steps.extend(observation(raw, current_rest()));
        let own = row(7001, Some(if foreign { 99 } else { 42 }), "heart");
        steps.extend(list_steps(vec![json!([own.clone()])]));
        steps.extend(list_steps(vec![json!([own])]));
        let real = real(steps);
        with_original_caps_v3(&real, 32, &THREAD_REACTIONS_POSTWRITE_CAPS_V3, |a| {
            let comment = reaction_original(&a);
            let expected = actors(&a, comment.clone()).unwrap();
            let fresh = actors(&a, comment).unwrap();
            let closed = std::cell::Cell::new(false);
            let result = a.provider.mutate_thread_reaction_guarded_v3(
                a.identity,
                &expected,
                &fresh,
                a.measured,
                a.captures,
                Mutation::Remove(7001),
                a.cancel,
                a.deadline,
                || {
                    closed.set(true);
                    Ok(())
                },
            );
            assert!(matches!(
                result,
                Err(ThreadPublicationFailureV1::NotStarted(_))
            ));
            assert!(!closed.get());
            assert_eq!(a.captures.consumed_requests(), 0);
        });
        protocol(&real, 0);
    }
}
#[test]
fn review_threads_v3_reaction_malformed_or_partial_postwrite_readback_is_uncertain_not_retry() {
    for mode in [
        "wrong-actor",
        "wrong-id",
        "partial-remove",
        "still-present",
        "error",
    ] {
        let actual = row(7001, Some(42), "heart");
        let add = mode.starts_with("wrong");
        let mut steps = original_steps();
        steps.extend(list_steps(vec![json!([actual.clone()])]));
        steps.extend(list_steps(vec![json!([actual.clone()])]));
        steps.extend(mutation_guard());
        steps.extend(close_steps());
        steps.push(http(
            add,
            if add { 201 } else { 204 },
            add.then_some(&actual),
            7001,
        ));
        match mode {
            "wrong-actor" => steps.push(get(&endpoint(1), json!([row(7001, Some(99), "heart")]))),
            "wrong-id" => steps.push(get(&endpoint(1), json!([row(7002, Some(42), "heart")]))),
            "partial-remove" => {
                for page in 1..=4 {
                    steps.push(get(&endpoint(page), page100(10000 + page as u64 * 100)));
                }
            }
            "still-present" => steps.push(get(&endpoint(1), json!([actual]))),
            _ => {
                let mut failure = get(&endpoint(1), json!({"message":"forbidden"}));
                failure.status = 1;
                steps.push(failure);
            }
        }
        let real = real(steps);
        with_original_caps_v3(&real, 40, &THREAD_REACTIONS_POSTWRITE_CAPS_V3, |a| {
            let comment = reaction_original(&a);
            let expected = actors(&a, comment.clone()).unwrap();
            let fresh = actors(&a, comment).unwrap();
            let mutation = if add {
                Mutation::Add(Content::Heart)
            } else {
                Mutation::Remove(7001)
            };
            assert!(matches!(
                a.provider.mutate_thread_reaction_guarded_v3(
                    a.identity,
                    &expected,
                    &fresh,
                    a.measured,
                    a.captures,
                    mutation,
                    a.cancel,
                    a.deadline,
                    &mut *a.close
                ),
                Err(ThreadPublicationFailureV1::WriteMayHaveStarted(_))
            ));
            assert_eq!(
                a.captures.consumed_requests(),
                if mode == "partial-remove" { 5 } else { 2 }
            );
        });
        protocol(&real, 1);
    }
}
#[test]
fn review_threads_v3_reaction_failed_close_and_cancelled_original_token_cannot_post() {
    for cancelled in [false, true] {
        let mut steps = original_steps();
        steps.extend(list_steps(vec![json!([])]));
        steps.extend(list_steps(vec![json!([])]));
        if !cancelled {
            steps.extend(mutation_guard());
        }
        let real = real(steps);
        with_original_caps_v3(&real, 32, &THREAD_REACTIONS_POSTWRITE_CAPS_V3, |a| {
            let comment = reaction_original(&a);
            let expected = actors(&a, comment.clone()).unwrap();
            let fresh = actors(&a, comment).unwrap();
            if cancelled {
                a.cancel.cancel();
            }
            assert!(matches!(
                a.provider.mutate_thread_reaction_guarded_v3(
                    a.identity,
                    &expected,
                    &fresh,
                    a.measured,
                    a.captures,
                    Mutation::Add(Content::Heart),
                    a.cancel,
                    a.deadline,
                    || Err(missing_original_fixture())
                ),
                Err(ThreadPublicationFailureV1::NotStarted(_))
            ));
            assert_eq!(a.captures.consumed_requests(), 0);
        });
        protocol(&real, 0);
    }
}
