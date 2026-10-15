//! Strict real gh transport: an anchored publication needs its complete receipt.
use super::*;

fn inline() -> InlineComment {
    InlineComment {
        path: "renamed.rs".into(),
        side: DiffSide::Left,
        line: 2,
        start_line: Some(1),
        start_side: Some(DiffSide::Left),
        body: "Please fix both original old lines".into(),
    }
}
fn receipt() -> Value {
    let comment = inline();
    json!({"id":50,"node_id":"PRRC_50","pull_request_review_id":80,
        "pull_request_url":"https://api.github.invalid/repos/example/repo/pulls/7",
        "path":comment.path,"side":"LEFT","line":2,"start_line":1,"start_side":"LEFT",
        "original_line":2,"original_start_line":1,"commit_id":target().head_sha,
        "original_commit_id":target().head_sha,"body":comment.body,
        "user":{"id":42,"login":"octocat","type":"User"}})
}
fn protocol(created: Value, observed: Value, patch: &str) -> Vec<GhStep> {
    let comment = inline();
    let mut steps = account_steps(42);
    steps.extend([
        step(
            api_args("repos/example/repo/pulls/7"),
            pr(7, "example/repo"),
        ),
        page_step(
            "repos/example/repo/pulls/7/files?per_page=100",
            json!([[{
            "filename":"renamed.rs","previous_filename":"old.rs","patch":patch}]]),
        ),
        step(
            api_args("repos/example/repo/pulls/7"),
            pr(7, "example/repo"),
        ),
        write_step(
            "repos/example/repo/pulls/7/comments",
            "POST",
            Some(json!({
            "path":comment.path,"side":"LEFT","line":2,"start_line":1,"start_side":"LEFT",
            "commit_id":target().head_sha,"body":comment.body})),
            created,
        ),
        step(api_args("repos/example/repo/pulls/comments/50"), observed),
    ]);
    steps
}
const PATCH: &str = "@@ -1,2 +1,2 @@\n-old one\n-old two\n+new one\n+new two";

#[test]
fn inline_comments_context_endpoint_needs_right_side_not_left_context() {
    let patch = "@@ -1,3 +1,3 @@\n context\n-old\n+new\n final context";
    let mut comment = inline();
    comment.line = 1;
    comment.start_line = None;
    comment.start_side = None;
    assert!(comment.validate_patch(patch).is_err());
    comment.side = DiffSide::Right;
    assert!(comment.validate_patch(patch).is_ok());
    comment.side = DiffSide::Left;
    comment.line = 2;
    assert!(comment.validate_patch(patch).is_ok());
    comment.line = 3;
    comment.start_line = Some(2);
    comment.start_side = Some(DiffSide::Left);
    assert!(comment.validate_patch(patch).is_err());
}

#[test]
fn inline_comments_guarded_source_refusal_is_not_started_and_never_posts() {
    use ghtui::providers::gh::{GithubError, InlinePublicationFailureV1};
    let fixture = Fixture::new();
    fixture.scripted_gh(&protocol(receipt(), receipt(), PATCH));
    let result = provider(&fixture).create_inline_comment_guarded_v1(
        &verified_identity(),
        &target(),
        &inline(),
        &CancellationToken::default(),
        Instant::now() + Duration::from_secs(5),
        || {
            assert_eq!(journal(&fixture).len(), 5);
            Err(GithubError {
                kind: GithubErrorKind::IdentityChanged,
                message: "Original source changed".into(),
            })
        },
    );
    assert!(matches!(
        result,
        Err(InlinePublicationFailureV1::NotStarted(_))
    ));
    assert_eq!(journal(&fixture).len(), 5);
    assert!(journal(&fixture).iter().all(|call| call["matched"] == true));
}

#[test]
fn inline_comments_guarded_callback_cancellation_or_elapsed_deadline_prevents_post() {
    use ghtui::providers::gh::InlinePublicationFailureV1;
    for cancel_after_check in [false, true] {
        let fixture = Fixture::new();
        fixture.scripted_gh(&protocol(receipt(), receipt(), PATCH));
        let cancel = CancellationToken::default();
        let deadline = Instant::now() + Duration::from_secs(1);
        let result = provider(&fixture).create_inline_comment_guarded_v1(
            &verified_identity(),
            &target(),
            &inline(),
            &cancel,
            deadline,
            || {
                assert_eq!(journal(&fixture).len(), 5);
                if cancel_after_check {
                    cancel.cancel();
                } else {
                    thread::sleep(
                        deadline.saturating_duration_since(Instant::now())
                            + Duration::from_millis(1),
                    );
                }
                Ok(())
            },
        );
        assert!(matches!(
            result,
            Err(InlinePublicationFailureV1::NotStarted(_))
        ));
        assert_eq!(journal(&fixture).len(), 5);
    }
}

#[test]
fn inline_comments_guarded_receipt_failure_after_post_is_never_not_started() {
    use ghtui::providers::gh::InlinePublicationFailureV1;
    let fixture = Fixture::new();
    let mut wrong = receipt();
    wrong["user"]["id"] = json!(43);
    fixture.scripted_gh(&protocol(wrong, receipt(), PATCH));
    let result = provider(&fixture).create_inline_comment_guarded_v1(
        &verified_identity(),
        &target(),
        &inline(),
        &CancellationToken::default(),
        Instant::now() + Duration::from_secs(5),
        || Ok(()),
    );
    assert!(matches!(
        result,
        Err(InlinePublicationFailureV1::WriteMayHaveStarted(_))
    ));
    assert_eq!(journal(&fixture).len(), 6);
}

#[test]
fn inline_comments_original_deadline_cannot_be_extended_by_later_timeout_setting() {
    let fixture = Fixture::new();
    let mut delayed = protocol(receipt(), receipt(), PATCH);
    delayed[0].delay_ms = 2000;
    fixture.scripted_gh(&delayed);
    let deadline = Instant::now() + Duration::from_millis(50);
    let provider = provider(&fixture)
        .with_scoped_read_window(deadline, 128 * 1024)
        .unwrap()
        .with_timeout(Duration::from_secs(30));
    let result = provider.inline_comment(
        &verified_identity(),
        &target(),
        &inline(),
        &CancellationToken::default(),
    );
    assert!(result.is_err());
    assert_eq!(journal(&fixture).len(), 1);
    assert!(Instant::now() < deadline + Duration::from_secs(1));
    thread::sleep(Duration::from_millis(10));
    assert!(provider
        .inline_comment(
            &verified_identity(),
            &target(),
            &inline(),
            &CancellationToken::default()
        )
        .is_err());
    assert_eq!(journal(&fixture).len(), 1);
}

#[test]
fn inline_comments_receipt_exact_author_parent_side_range_and_single_post() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&protocol(receipt(), receipt(), PATCH));
    let result = provider(&fixture)
        .inline_comment(
            &verified_identity(),
            &target(),
            &inline(),
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(result["node_id"], "PRRC_50");
    assert_eq!(result["user"]["id"], 42);
    assert_eq!(result["start_side"], "LEFT");
    let calls = journal(&fixture);
    assert_eq!(calls.len(), 7);
    assert_eq!(
        calls
            .iter()
            .filter(|call| call["args"]
                .as_array()
                .unwrap()
                .iter()
                .any(|arg| arg == "POST"))
            .count(),
        1
    );
    assert!(calls.iter().all(|call| call["matched"] == true));
}

fn corrupt(value: &mut Value, kind: usize) {
    match kind {
        0 => value["node_id"] = json!(""),
        1 => value["user"]["id"] = json!(43),
        2 => value["side"] = json!("RIGHT"),
        3 => value["start_line"] = json!(2),
        4 => value["start_side"] = json!("RIGHT"),
        5 => value["pull_request_url"] = json!("https://other.invalid/repos/example/repo/pulls/7"),
        6 => value["id"] = json!(51),
        7 => value["in_reply_to_id"] = json!(999),
        _ => value["user"]["login"] = json!("different-login"),
    }
}
#[test]
fn inline_comments_receipt_wrong_creation_cannot_be_laundered_by_valid_readback() {
    for kind in [0, 1, 2, 3, 4, 5, 7, 8] {
        let fixture = Fixture::new();
        let mut wrong = receipt();
        corrupt(&mut wrong, kind);
        fixture.scripted_gh(&protocol(wrong, receipt(), PATCH));
        let result = provider(&fixture).inline_comment(
            &verified_identity(),
            &target(),
            &inline(),
            &CancellationToken::default(),
        );
        assert!(
            matches!(result, Err(ref error) if matches!(error.kind, GithubErrorKind::AmbiguousWrite)),
            "Incorrect create field {kind} was accepted: {result:?}"
        );
        let calls = journal(&fixture);
        assert_eq!(calls.len(), 6, "Refuse the invalid creation before its GET");
        assert!(calls.iter().all(|call| call["matched"] == true));
    }
}
#[test]
fn inline_comments_receipt_wrong_readback_never_confirms_another_actor_node_side_or_parent() {
    for kind in 0..9 {
        let fixture = Fixture::new();
        let mut wrong = receipt();
        corrupt(&mut wrong, kind);
        fixture.scripted_gh(&protocol(receipt(), wrong, PATCH));
        let result = provider(&fixture).inline_comment(
            &verified_identity(),
            &target(),
            &inline(),
            &CancellationToken::default(),
        );
        assert!(
            matches!(result, Err(ref error) if matches!(error.kind, GithubErrorKind::AmbiguousWrite)),
            "Incorrect observed field {kind} was accepted: {result:?}"
        );
        let calls = journal(&fixture);
        assert_eq!(calls.len(), 7);
        assert!(calls.iter().all(|call| call["matched"] == true));
    }
}
#[test]
fn inline_comments_canonical_range_rejects_truncated_hunks_and_adjacent_distinct_hunks() {
    let comment = inline();
    assert!(
        comment
            .validate_patch("@@ -1,2 +1,2 @@\n-old one\n-old two\n+new one")
            .is_err(),
        "A present old-side range cannot waive missing new-side patch bytes"
    );
    assert!(
        comment
            .validate_patch("@@ -1 +1 @@\n-old one\n+new one\n@@ -2 +2 @@\n-old two\n+new two")
            .is_err(),
        "Adjacent coordinates in separate hunks are not one canonical review range"
    );
    assert!(comment.validate_patch(PATCH).is_ok());
}
#[test]
fn inline_comments_patch_refusal_precedes_publication_and_preserves_no_write_effects() {
    let fixture = Fixture::new();
    let mut steps = protocol(
        receipt(),
        receipt(),
        "@@ -1,2 +1,2 @@\n-old one\n-old two\n+new one",
    );
    steps.truncate(4);
    fixture.scripted_gh(&steps);
    let error = provider(&fixture)
        .inline_comment(
            &verified_identity(),
            &target(),
            &inline(),
            &CancellationToken::default(),
        )
        .unwrap_err();
    assert!(matches!(
        error.kind,
        GithubErrorKind::InvalidTarget | GithubErrorKind::Json
    ));
    let calls = journal(&fixture);
    assert_eq!(calls.len(), 4);
    assert!(calls.iter().all(|call| call["matched"] == true));
    assert!(calls.iter().all(|call| !call["args"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| arg == "POST")));
}

#[test]
fn inline_comments_overflowing_canonical_coordinates_return_error_not_panic_or_wrapped_anchor() {
    let result = std::panic::catch_unwind(|| {
        inline().validate_patch("@@ -4294967295,4 +1 @@\n-a\n-b\n-c\n-d\n+new")
    });
    assert!(
        result.is_ok(),
        "Malformed hunk coordinates must not panic the actual canonical parser"
    );
    assert!(
        result.unwrap().is_err(),
        "Overflow must not manufacture wrapped line1/2 authority"
    );
    let mut maximum = inline();
    maximum.line = u64::from(u32::MAX);
    maximum.start_line = None;
    maximum.start_side = None;
    maximum.side = DiffSide::Right;
    assert!(
        maximum
            .validate_patch("@@ -4294967295 +4294967295 @@\n last representable line")
            .is_ok(),
        "The final representable line stays valid without overflowing its next-position counter"
    );
}

#[test]
fn inline_comments_typed_receipt_retains_validated_actor_path_side_head_and_exact_body() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&protocol(receipt(), receipt(), PATCH));
    let result = provider(&fixture)
        .create_inline_comment_v1(
            &verified_identity(),
            &target(),
            &inline(),
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(result.comment().id, 50);
    assert_eq!(result.comment().node_id, "PRRC_50");
    assert_eq!(result.comment().author.login, "octocat");
    assert_eq!(result.comment().body, inline().body);
    assert_eq!(result.account_id(), 42);
    assert_eq!(result.path(), "renamed.rs");
    assert_eq!(result.side(), DiffSide::Left);
    assert_eq!((result.start_line(), result.line()), (Some(1), 2));
    assert_eq!(result.observed_head(), target().head_sha);
    assert_eq!(journal(&fixture).len(), 7);
}
