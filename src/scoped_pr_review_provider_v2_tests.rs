//! Actual typed provider routes over genuine original file-image factories.
#[path = "scoped_pr_review_pending_operations_v3_tests.rs"]
mod pending_operations_v3;
#[path = "scoped_pr_review_source_v2_tests.rs"]
mod source_v2;
use super::*;
use crate::providers::gh::{
    ObservedReviewStateV2, ReviewEvent, ReviewMutationV2, ReviewPublicationFailureV2,
    ReviewReadLimitsV2, ReviewReceiptV2,
};
use crate::scoped_pr_observed_tabs_v1::resources::ResourceLocalGuard;
use images::ClosedPrFileImagesV1;
use std::sync::atomic::{AtomicUsize, Ordering};

fn current_parent(h: &history::GitImagesV1) -> Value {
    let mut parent = resource_parent(REPO);
    h.bind_parent(&mut parent);
    parent
}
fn review(h: &history::GitImagesV1, state: &str, body: &str, uid: u64) -> Value {
    let mut value = json!({"id":810,"node_id":"ACTUAL_REVIEW_810","state":state,"body":body,
        "commit_id":h.h,"pull_request_url":format!("https://{HOST}/api/v3/repos/{REPO}/pulls/7"),
        "user":{"id":uid,"login":"renamed-reviewer","type":"User"}});
    if state != "PENDING" {
        value["submitted_at"] = json!("2026-10-15T00:00:00Z");
    }
    value
}
fn comments(h: &history::GitImagesV1) -> Vec<Value> {
    ["LEFT","RIGHT"].into_iter().enumerate().map(|(index,side)| json!({
        "id":910+index,"node_id":format!("ACTUAL_COMMENT_{}",910+index),"pull_request_review_id":810,
        "pull_request_url":format!("https://{HOST}/api/v3/repos/{REPO}/pulls/7"),
        "user":{"id":42,"login":"renamed-reviewer","type":"User"},
        "path":history::PATH,"side":side,"line":2,"start_line":1,"start_side":side,
        "body":format!("original {side} draft"),"commit_id":h.h,"in_reply_to_id":null,
    })).collect()
}
fn requested_comments() -> Vec<crate::providers::gh::InlineComment> {
    [
        crate::providers::gh::DiffSide::Left,
        crate::providers::gh::DiffSide::Right,
    ]
    .into_iter()
    .map(|side| crate::providers::gh::InlineComment {
        path: history::PATH.into(),
        side,
        line: 2,
        start_line: Some(1),
        start_side: Some(side),
        body: format!(
            "original {} draft",
            if side == crate::providers::gh::DiffSide::Left {
                "LEFT"
            } else {
                "RIGHT"
            }
        ),
    })
    .collect()
}
fn write(endpoint: &str, method: &str, payload: Option<Value>, response: Value) -> GhStep {
    let mut args = vec!["api", endpoint, "--hostname", HOST, "--method", method];
    if payload.is_some() {
        args.extend(["--input", "-"]);
    }
    let mut step = GhStep::read(&args, &response.to_string());
    if let Some(payload) = payload {
        step.stdin = serde_json::to_vec(&payload).unwrap();
    }
    step
}
fn catalogue_step(value: Value) -> GhStep {
    get(
        &format!("repos/{REPO}/pulls/7/reviews?per_page=100&page=1"),
        value,
    )
}
fn snapshot_steps(h: &history::GitImagesV1, row: &Value) -> Vec<GhStep> {
    let mut actual_comments = comments(h);
    for comment in &mut actual_comments {
        comment["user"] = row["user"].clone();
        comment["commit_id"] = row["commit_id"].clone();
    }
    vec![
        get(&format!("repos/{REPO}/pulls/7/reviews/810"), row.clone()),
        get(
            &format!("repos/{REPO}/pulls/7/reviews/810/comments?per_page=100&page=1"),
            json!(actual_comments),
        ),
        get(&format!("repos/{REPO}/pulls/7/reviews/810"), row.clone()),
    ]
}
fn guard_steps(h: &history::GitImagesV1) -> Vec<GhStep> {
    let mut result = principal(42, "reviewer-active");
    result.push(get(&format!("repos/{REPO}/pulls/7"), current_parent(h)));
    result
}
fn opening_steps(h: &history::GitImagesV1) -> Vec<GhStep> {
    let mut steps = principal(42, "reviewer-active");
    steps.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), current_parent(h)));
    steps
}
fn closing_steps(h: &history::GitImagesV1) -> Vec<GhStep> {
    let mut steps = vec![
        get(&format!("repos/{REPO}/pulls/7"), current_parent(h)),
        get(&format!("repos/{REPO}"), repo(410, REPO)),
    ];
    steps.extend(principal(42, "reviewer-active"));
    steps
}
fn select_steps(h: &history::GitImagesV1, row: &Value) -> Vec<GhStep> {
    let mut steps = opening_steps(h);
    steps.push(catalogue_step(json!([row.clone()])));
    steps.extend(snapshot_steps(h, row));
    steps.extend(closing_steps(h));
    steps
}
fn publish(
    s: &Setup,
    proof: &Arc<ClosedPrFileImagesV1>,
    mutation: ReviewMutationV2<'_>,
    limits: &ReviewReadLimitsV2,
    cancel: &CancellationToken,
    deadline: Instant,
    before: impl FnOnce() -> crate::providers::gh::GithubResult<()>,
) -> Result<ReviewReceiptV2, ReviewPublicationFailureV2> {
    let original = s.original_files.observed();
    let provider = provider(&s.fixture).with_catalogue_read_deadline(deadline);
    let database = s.fixture.root.join("cache.sqlite3");
    let mut session = provider
        .begin_repository_catalogue_access(proof.source.source(), &Default::default(), cancel)
        .map_err(ReviewPublicationFailureV2::NotStarted)?;
    let caps = vec![limits.max_document_bytes; limits.max_pages + 3];
    let mut captures = session
        .reserve_postwrite_capture_v2(&caps, 2, cancel)
        .map_err(ReviewPublicationFailureV2::NotStarted)?;
    let window = session
        .reserve_measured_external_reads(64, limits.max_document_bytes, 8 * 1024 * 1024, 2, cancel)
        .map_err(ReviewPublicationFailureV2::NotStarted)?;
    let measured = provider
        .clone()
        .with_measured_read_window(window)
        .map_err(ReviewPublicationFailureV2::NotStarted)?;
    session
        .verify(proof.navigation.original(), cancel)
        .map_err(ReviewPublicationFailureV2::NotStarted)?;
    let parent = measured
        .pr_inline_original_parent_v1(
            session
                .observation_v1(proof.navigation.original().id)
                .unwrap(),
            original,
            &Default::default(),
            cancel,
        )
        .map_err(ReviewPublicationFailureV2::NotStarted)?;
    let local = ResourceLocalGuard::capture(
        &database,
        &proof.source,
        session.identity(),
        parent.summary(),
    )
    .map_err(ReviewPublicationFailureV2::NotStarted)?;
    let identity = session.identity().clone();
    provider.mutate_review_guarded_v2(
        &measured,
        &mut captures,
        &identity,
        &parent,
        mutation,
        limits,
        cancel,
        deadline,
        || {
            before()?;
            measured.pr_inline_original_parent_v1(
                session
                    .observation_v1(proof.navigation.original().id)
                    .unwrap(),
                original,
                &Default::default(),
                cancel,
            )?;
            session.verify(proof.navigation.original(), cancel)?;
            session.close_with_guard(cancel, || local.validate(&database, &proof.source))?;
            Ok(())
        },
    )
}
fn create_steps(h: &history::GitImagesV1, response: Value, include_readback: bool) -> Vec<GhStep> {
    let mut result = opening_steps(h);
    result.extend(guard_steps(h));
    result.push(catalogue_step(json!([])));
    result.push(get(
        &format!("repos/{REPO}/pulls/7/files?per_page=100&page=1"),
        json!([h.file()]),
    ));
    result.push(get(&format!("repos/{REPO}/pulls/7"), current_parent(h)));
    result.extend(closing_steps(h));
    let payload = json!({"body":"pending summary","commit_id":h.h,"comments":[
        {"path":history::PATH,"side":"LEFT","line":2,"start_line":1,"start_side":"LEFT","body":"original LEFT draft"},
        {"path":history::PATH,"side":"RIGHT","line":2,"start_line":1,"start_side":"RIGHT","body":"original RIGHT draft"},
    ]});
    result.push(write(
        &format!("repos/{REPO}/pulls/7/reviews"),
        "POST",
        Some(payload),
        response.clone(),
    ));
    if include_readback {
        result.extend(snapshot_steps(h, &response));
    }
    result
}
fn select_pending(
    s: &Setup,
    proof: &Arc<ClosedPrFileImagesV1>,
) -> crate::providers::gh::ReviewSnapshotV2 {
    let original = s.original_files.observed();
    let p = provider(&s.fixture);
    let cancel = CancellationToken::default();
    let mut session = p
        .begin_repository_catalogue_access(proof.source.source(), &Default::default(), &cancel)
        .unwrap();
    let window = session
        .reserve_measured_external_reads(8, 1024 * 1024, 8 * 1024 * 1024, 2, &cancel)
        .unwrap();
    let measured = p.clone().with_measured_read_window(window).unwrap();
    session
        .verify(proof.navigation.original(), &cancel)
        .unwrap();
    let parent = measured
        .pr_inline_original_parent_v1(
            session
                .observation_v1(proof.navigation.original().id)
                .unwrap(),
            original,
            &Default::default(),
            &cancel,
        )
        .unwrap();
    let identity = session.identity().clone();
    let local = ResourceLocalGuard::capture(
        &s.fixture.root.join("cache.sqlite3"),
        &proof.source,
        &identity,
        parent.summary(),
    )
    .unwrap();
    let catalogue = measured
        .review_catalogue_v2(
            &identity,
            &parent,
            &Default::default(),
            &cancel,
            session.deadline(),
        )
        .unwrap();
    assert!(catalogue.complete());
    assert_eq!(catalogue.rows().len(), 1);
    let selected = measured
        .review_snapshot_v2(
            &identity,
            &parent,
            &catalogue.rows()[0],
            &Default::default(),
            &cancel,
            session.deadline(),
        )
        .unwrap();
    measured
        .pr_inline_original_parent_v1(
            session
                .observation_v1(proof.navigation.original().id)
                .unwrap(),
            original,
            &Default::default(),
            &cancel,
        )
        .unwrap();
    session
        .verify(proof.navigation.original(), &cancel)
        .unwrap();
    session
        .close_with_guard(&cancel, || {
            local.validate(&s.fixture.root.join("cache.sqlite3"), &proof.source)
        })
        .unwrap();
    selected
}

#[test]
fn pending_reviews_v2_create_event_omitted_two_canonical_comments_receipt_and_renamed_numeric_account(
) {
    let mut s = inline_setup(|h, steps| {
        steps.extend(create_steps(
            h,
            review(h, "PENDING", "pending summary", 42),
            true,
        ))
    });
    let result = read(&mut s, &Default::default(), &Default::default()).unwrap();
    let before = commands(&s.fixture).len();
    let callback = AtomicUsize::new(0);
    let comments = requested_comments();
    let receipt = publish(
        &s,
        &result.proof,
        ReviewMutationV2::CreatePending {
            body: "pending summary",
            comments: &comments,
        },
        &Default::default(),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
        || {
            callback.fetch_add(1, Ordering::SeqCst);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(callback.load(Ordering::SeqCst), 1);
    assert_eq!(commands(&s.fixture).len() - before, 18);
    assert_eq!(receipt.review_id(), 810);
    assert_eq!(receipt.review_node(), "ACTUAL_REVIEW_810");
    assert!(!receipt.discarded());
    let snapshot = receipt.snapshot().unwrap();
    assert!(snapshot.complete());
    assert_eq!(snapshot.review().author_account_id(), Some(42));
    assert_eq!(snapshot.review().author().login, "renamed-reviewer");
    assert_eq!(snapshot.review().submitted_at(), None);
    assert_eq!(snapshot.comments().len(), 2);
    assert_eq!(snapshot.comments()[0].review_id(), 810);
    assert_eq!(snapshot.comments()[0].path(), history::PATH);
    assert_eq!(snapshot.comments()[0].line(), Some(2));
    assert_eq!(snapshot.comments()[0].start_line(), Some(1));
    unchanged_git_and_exact_script(&s);
}

#[test]
fn pending_reviews_v2_all_three_decisions_confirm_exact_owned_review_without_login_self_inference()
{
    for (event, state, body) in [
        (ReviewEvent::Approve, "APPROVED", ""),
        (ReviewEvent::RequestChanges, "CHANGES_REQUESTED", "fix this"),
        (ReviewEvent::Comment, "COMMENTED", "observed feedback"),
    ] {
        let mut s = inline_setup(|h, steps| {
            let pending = review(h, "PENDING", "original pending", 42);
            steps.extend(select_steps(h, &pending));
            steps.extend(opening_steps(h));
            steps.extend(guard_steps(h));
            steps.extend(snapshot_steps(h, &pending));
            steps.push(get(&format!("repos/{REPO}/pulls/7"), current_parent(h)));
            steps.extend(closing_steps(h));
            let submitted = review(h, state, body, 42);
            steps.push(write(&format!("repos/{REPO}/pulls/7/reviews/810/events"),"POST",
                Some(json!({"event":match event {ReviewEvent::Approve=>"APPROVE",ReviewEvent::RequestChanges=>"REQUEST_CHANGES",ReviewEvent::Comment=>"COMMENT"},"body":body})),submitted.clone()));
            steps.extend(snapshot_steps(h, &submitted));
            // Same username can label the distinct immutable PR author ID51.
            for step in steps {
                if let Ok(mut value) = serde_json::from_slice::<Value>(&step.stdout) {
                    if value.get("head").is_some() && value.get("base").is_some() {
                        value["user"]["login"] = json!("reviewer-active");
                        step.stdout = value.to_string().into_bytes();
                    }
                }
            }
        });
        let result = read(&mut s, &Default::default(), &Default::default()).unwrap();
        let parent = result.proof.observed().parent();
        assert_eq!(parent.author_account_id(), Some(51));
        let observed = select_pending(&s, &result.proof);
        let before = commands(&s.fixture).len();
        let receipt = publish(
            &s,
            &result.proof,
            ReviewMutationV2::SubmitPending {
                observed: &observed,
                event,
                body,
            },
            &Default::default(),
            &Default::default(),
            Instant::now() + Duration::from_secs(30),
            || Ok(()),
        )
        .unwrap();
        assert_eq!(commands(&s.fixture).len() - before, 19);
        let snapshot = receipt.snapshot().unwrap();
        assert_ne!(snapshot.review().state(), &ObservedReviewStateV2::Pending);
        assert_eq!(snapshot.review().body(), body);
        assert_eq!(snapshot.review().author_account_id(), Some(42));
        assert_eq!(
            snapshot.review().submitted_at(),
            Some("2026-10-15T00:00:00Z")
        );
        assert_eq!(snapshot.comments().len(), 2);
        unchanged_git_and_exact_script(&s);
    }
}

#[test]
fn pending_reviews_v2_discard_requires_real_complete_absence_and_never_submits_old_commit() {
    let mut s = inline_setup(|h, steps| {
        let mut old = review(h, "PENDING", "old-head pending", 42);
        old["commit_id"] = json!(h.m);
        steps.extend(select_steps(h, &old));
        steps.extend(opening_steps(h));
        steps.extend(guard_steps(h));
        steps.extend(snapshot_steps(h, &old));
        steps.extend(opening_steps(h));
        steps.extend(guard_steps(h));
        steps.extend(snapshot_steps(h, &old));
        steps.push(get(&format!("repos/{REPO}/pulls/7"), current_parent(h)));
        steps.extend(closing_steps(h));
        steps.push(write(
            &format!("repos/{REPO}/pulls/7/reviews/810"),
            "DELETE",
            None,
            old,
        ));
        steps.push(catalogue_step(json!([])));
    });
    let result = read(&mut s, &Default::default(), &Default::default()).unwrap();
    let observed = select_pending(&s, &result.proof);
    assert_eq!(observed.review().commit(), s.h.m);
    let before = commands(&s.fixture).len();
    let refused = publish(
        &s,
        &result.proof,
        ReviewMutationV2::SubmitPending {
            observed: &observed,
            event: ReviewEvent::Comment,
            body: "old head feedback",
        },
        &Default::default(),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
        || panic!("old-head review must never invoke"),
    );
    assert!(matches!(
        refused,
        Err(ReviewPublicationFailureV2::NotStarted(_))
    ));
    assert_eq!(commands(&s.fixture).len() - before, 10);
    let before = commands(&s.fixture).len();
    let receipt = publish(
        &s,
        &result.proof,
        ReviewMutationV2::DiscardPending {
            observed: &observed,
        },
        &Default::default(),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
        || Ok(()),
    )
    .unwrap();
    assert_eq!(commands(&s.fixture).len() - before, 17);
    assert!(receipt.discarded());
    assert_eq!(receipt.review_id(), 810);
    assert!(receipt.snapshot().is_none());
    unchanged_git_and_exact_script(&s);
}

#[test]
fn pending_reviews_v2_bad_successful_post_has_no_receipt_readback_or_automatic_retry() {
    for variant in 0..4 {
        let mut s = inline_setup(|h, steps| {
            let mut invalid = review(h, "PENDING", "pending summary", 42);
            match variant {
                0 => invalid["user"]["id"] = json!(43),
                1 => invalid["body"] = json!("foreign body"),
                2 => invalid["commit_id"] = json!(h.m),
                _ => {
                    invalid["pull_request_url"] =
                        json!(format!("https://{HOST}/api/v3/repos/other/repo/pulls/7"))
                }
            }
            steps.extend(create_steps(h, invalid, false));
        });
        let result = read(&mut s, &Default::default(), &Default::default()).unwrap();
        let before = commands(&s.fixture).len();
        let comments = requested_comments();
        let outcome = publish(
            &s,
            &result.proof,
            ReviewMutationV2::CreatePending {
                body: "pending summary",
                comments: &comments,
            },
            &Default::default(),
            &Default::default(),
            Instant::now() + Duration::from_secs(30),
            || Ok(()),
        );
        assert!(matches!(
            outcome,
            Err(ReviewPublicationFailureV2::WriteMayHaveStarted(_))
        ));
        assert_eq!(commands(&s.fixture).len() - before, 15);
        unchanged_git_and_exact_script(&s);
    }
}

#[test]
fn pending_reviews_v2_encoded_body_expiry_and_cancel_refuse_before_extra_transport_or_callback() {
    let mut s = inline_setup(|h, steps| steps.extend(opening_steps(h)));
    let result = read(&mut s, &Default::default(), &Default::default()).unwrap();
    let before = commands(&s.fixture).len();
    let callback = AtomicUsize::new(0);
    let body = "\u{1}".repeat(65_536);
    let cancelled = CancellationToken::default();
    cancelled.cancel();
    for (body, cancel, deadline) in [
        (
            body.as_str(),
            CancellationToken::default(),
            Instant::now() + Duration::from_secs(30),
        ),
        (
            "ordinary draft",
            cancelled,
            Instant::now() + Duration::from_secs(30),
        ),
        (
            "ordinary draft",
            CancellationToken::default(),
            Instant::now() - Duration::from_secs(1),
        ),
    ] {
        let outcome = publish(
            &s,
            &result.proof,
            ReviewMutationV2::CreatePending {
                body,
                comments: &[],
            },
            &Default::default(),
            &cancel,
            deadline,
            || {
                callback.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        );
        assert!(matches!(
            outcome,
            Err(ReviewPublicationFailureV2::NotStarted(_))
        ));
    }
    assert_eq!(callback.load(Ordering::SeqCst), 0);
    assert_eq!(commands(&s.fixture).len() - before, 4);
    unchanged_git_and_exact_script(&s);
}

#[test]
fn pending_reviews_v2_incomplete_review_page_never_proves_pending_absence_or_invokes_write() {
    let mut s = inline_setup(|h, steps| {
        steps.extend(opening_steps(h));
        steps.extend(guard_steps(h));
        let one = review(h, "COMMENTED", "first observed", 51);
        let mut two = one.clone();
        two["id"] = json!(811);
        two["node_id"] = json!("ACTUAL_REVIEW_811");
        steps.push(catalogue_step(json!([one, two])));
    });
    let result = read(&mut s, &Default::default(), &Default::default()).unwrap();
    let before = commands(&s.fixture).len();
    let limits = crate::providers::gh::ReviewReadLimitsV2 {
        max_reviews: 1,
        ..Default::default()
    };
    let callback = AtomicUsize::new(0);
    let outcome = publish(
        &s,
        &result.proof,
        ReviewMutationV2::CreatePending {
            body: "local draft",
            comments: &[],
        },
        &limits,
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
        || {
            callback.fetch_add(1, Ordering::SeqCst);
            Ok(())
        },
    );
    assert!(matches!(
        outcome,
        Err(ReviewPublicationFailureV2::NotStarted(_))
    ));
    assert_eq!(commands(&s.fixture).len() - before, 8);
    assert_eq!(callback.load(Ordering::SeqCst), 0);
    unchanged_git_and_exact_script(&s);
}

#[test]
fn pending_reviews_v2_numeric_self_unknown_deleted_author_refuses_decisions_before_new_children() {
    for user in [
        json!({"id":42,"login":"completely-different-login","type":"User"}),
        json!({"id":0,"login":"reviewer-active","type":"User"}),
        json!({"id":null,"node_id":"USER_42","login":"reviewer-active","type":"User"}),
        Value::Null,
    ] {
        let mut s = inline_setup(|h, steps| {
            let pending = review(h, "PENDING", "original pending", 42);
            steps.extend(select_steps(h, &pending));
            steps.extend(opening_steps(h));
            steps.extend(opening_steps(h));
            for step in steps {
                if let Ok(mut value) = serde_json::from_slice::<Value>(&step.stdout) {
                    if value.get("head").is_some() && value.get("base").is_some() {
                        value["user"] = user.clone();
                        step.stdout = value.to_string().into_bytes();
                    }
                }
            }
        });
        let result = read(&mut s, &Default::default(), &Default::default()).unwrap();
        let observed = select_pending(&s, &result.proof);
        let before = commands(&s.fixture).len();
        for event in [ReviewEvent::Approve, ReviewEvent::RequestChanges] {
            let outcome = publish(
                &s,
                &result.proof,
                ReviewMutationV2::SubmitPending {
                    observed: &observed,
                    event,
                    body: "review decision",
                },
                &Default::default(),
                &Default::default(),
                Instant::now() + Duration::from_secs(30),
                || panic!("no proof callback for self/unknown author"),
            );
            let Err(ReviewPublicationFailureV2::NotStarted(error)) = outcome else {
                panic!("self/unknown author must fail before invocation");
            };
            assert!(matches!(
                error.kind,
                crate::providers::gh::GithubErrorKind::Permission
            ));
        }
        assert_eq!(commands(&s.fixture).len() - before, 8);
        unchanged_git_and_exact_script(&s);
    }
}

#[test]
fn pending_reviews_v2_final_original_closure_refusal_or_cancel_starts_no_post() {
    for cancel_in_callback in [false, true] {
        let mut s = inline_setup(|h, steps| {
            let mut request = create_steps(h, review(h, "PENDING", "pending summary", 42), false);
            assert_eq!(request.len(), 15);
            // The explicit original callback fails before closing reads or
            // POST, not after a synthetic successful source close.
            request.truncate(10);
            steps.extend(request);
        });
        let result = read(&mut s, &Default::default(), &Default::default()).unwrap();
        let before = commands(&s.fixture).len();
        let cancel = CancellationToken::default();
        let callback = AtomicUsize::new(0);
        let comments = requested_comments();
        let outcome = publish(
            &s,
            &result.proof,
            ReviewMutationV2::CreatePending {
                body: "pending summary",
                comments: &comments,
            },
            &Default::default(),
            &cancel,
            Instant::now() + Duration::from_secs(30),
            || {
                callback.fetch_add(1, Ordering::SeqCst);
                if cancel_in_callback {
                    cancel.cancel();
                    Ok(())
                } else {
                    Err(crate::providers::gh::GithubError {
                        kind: crate::providers::gh::GithubErrorKind::Permission,
                        message: "Actual original source/storage closing proof refused".into(),
                    })
                }
            },
        );
        assert!(matches!(
            outcome,
            Err(ReviewPublicationFailureV2::NotStarted(_))
        ));
        assert_eq!(callback.load(Ordering::SeqCst), 1);
        assert_eq!(commands(&s.fixture).len() - before, 10);
        unchanged_git_and_exact_script(&s);
    }
}

#[test]
fn pending_reviews_v2_denied_server_enforced_dismissal_preserves_foreign_author_and_is_uncertain() {
    let mut s = inline_setup(|h, steps| {
        let approved = review(h, "APPROVED", "foreign approval", 51);
        steps.extend(select_steps(h, &approved));
        steps.extend(opening_steps(h));
        steps.extend(guard_steps(h));
        steps.extend(snapshot_steps(h, &approved));
        steps.push(get(&format!("repos/{REPO}/pulls/7"), current_parent(h)));
        steps.extend(closing_steps(h));
        let mut denied = write(
            &format!("repos/{REPO}/pulls/7/reviews/810/dismissals"),
            "PUT",
            Some(json!({"message":"explicit current dismissal reason","event":"DISMISS"})),
            Value::Null,
        );
        denied.status = 1;
        denied.stderr =
            b"HTTP403: this caller is not permitted to dismiss protected-branch reviews".to_vec();
        steps.push(denied);
    });
    let result = read(&mut s, &Default::default(), &Default::default()).unwrap();
    let observed = select_pending(&s, &result.proof);
    assert_eq!(observed.review().author_account_id(), Some(51));
    let before = commands(&s.fixture).len();
    let outcome = publish(
        &s,
        &result.proof,
        ReviewMutationV2::DismissSubmitted {
            observed: &observed,
            reason: "explicit current dismissal reason",
        },
        &Default::default(),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
        || Ok(()),
    );
    assert!(matches!(
        outcome,
        Err(ReviewPublicationFailureV2::WriteMayHaveStarted(_))
    ));
    assert_eq!(commands(&s.fixture).len() - before, 16);
    assert_eq!(observed.review().author_account_id(), Some(51));
    assert_eq!(observed.review().body(), "foreign approval");
    unchanged_git_and_exact_script(&s);
}

#[test]
fn pending_reviews_v2_current_review_with_actual_old_commit_comment_refuses_submit_without_reanchoring(
) {
    let mut s = inline_setup(|h, steps| {
        let pending = review(h, "PENDING", "original pending", 42);
        let mut selection = select_steps(h, &pending);
        let mut fresh = opening_steps(h);
        fresh.extend(guard_steps(h));
        fresh.extend(snapshot_steps(h, &pending));
        for step in selection.iter_mut().chain(fresh.iter_mut()) {
            if let Ok(mut array) = serde_json::from_slice::<Value>(&step.stdout) {
                if let Some(rows) = array.as_array_mut().filter(|rows| {
                    rows.first()
                        .is_some_and(|row| row.get("pull_request_review_id").is_some())
                }) {
                    rows[0]["commit_id"] = json!(h.m);
                    step.stdout = array.to_string().into_bytes();
                }
            }
        }
        steps.extend(selection);
        steps.extend(fresh);
    });
    let result = read(&mut s, &Default::default(), &Default::default()).unwrap();
    let observed = select_pending(&s, &result.proof);
    assert_eq!(observed.review().commit(), s.h.h);
    assert_eq!(observed.comments()[0].commit(), s.h.m);
    let before = commands(&s.fixture).len();
    let outcome = publish(
        &s,
        &result.proof,
        ReviewMutationV2::SubmitPending {
            observed: &observed,
            event: ReviewEvent::Comment,
            body: "explicit decision",
        },
        &Default::default(),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
        || panic!("stale comment must never close for publication"),
    );
    let Err(ReviewPublicationFailureV2::NotStarted(error)) = outcome else {
        panic!("old actual comment commit must refuse before POST")
    };
    assert!(matches!(
        error.kind,
        crate::providers::gh::GithubErrorKind::StaleHead
    ));
    assert_eq!(commands(&s.fixture).len() - before, 10);
    assert_eq!(
        observed.comments()[0].commit(),
        s.h.m,
        "no auto coordinate or commit conversion"
    );
    unchanged_git_and_exact_script(&s);
}

#[test]
fn pending_reviews_v2_dismiss_without_actual_valid_same_review_submission_stays_readable_but_never_invokes(
) {
    for supplied in [
        Value::Null,
        json!(""),
        json!("not-an-observed-timestamp"),
        json!("2026-13-01T00:00:00Z"),
        json!("2026-10-15T25:00:00Z"),
    ] {
        let mut s = inline_setup(|h, steps| {
            let mut actual = review(h, "APPROVED", "ORIGINAL readable submitted-review body", 51);
            actual["submitted_at"] = supplied.clone();
            steps.extend(select_steps(h, &actual));
            steps.extend(opening_steps(h));
            steps.extend(guard_steps(h));
            steps.extend(snapshot_steps(h, &actual));
        });
        let original = read(&mut s, &Default::default(), &Default::default()).unwrap();
        let observed = select_pending(&s, &original.proof);
        assert_eq!(observed.review().state(), &ObservedReviewStateV2::Approved);
        assert_eq!(
            observed.review().body(),
            "ORIGINAL readable submitted-review body"
        );
        assert!(observed.complete());
        assert!(!observed.review().has_observed_submission_v2());
        assert_eq!(observed.review().submitted_at(), supplied.as_str());
        let before = commands(&s.fixture).len();
        let callback = AtomicUsize::new(0);
        let result = publish(
            &s,
            &original.proof,
            ReviewMutationV2::DismissSubmitted {
                observed: &observed,
                reason: "Explicit reason; server privilege UNKNOWN",
            },
            &Default::default(),
            &Default::default(),
            Instant::now() + Duration::from_secs(30),
            || {
                callback.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        );
        assert!(matches!(
            result,
            Err(ReviewPublicationFailureV2::NotStarted(_))
        ));
        assert_eq!(callback.load(Ordering::SeqCst), 0);
        assert_eq!(commands(&s.fixture).len() - before, 10);
        unchanged_git_and_exact_script(&s);
    }
}
