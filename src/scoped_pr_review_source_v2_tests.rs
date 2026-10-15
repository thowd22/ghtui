//! Genuine source/UID/Files/image producers; saved anchors remain metadata.
use super::*;
#[path = "scoped_pr_review_avatars_v7_tests.rs"]
mod avatars_v7;
#[path = "scoped_pr_review_local_batch_v10_tests.rs"]
mod local_batch_v10;
#[path = "scoped_pr_review_new_batch_v12_tests.rs"]
mod new_batch_v12;
#[path = "scoped_pr_review_operations_consumer_v3_tests.rs"]
mod operations_v3;
#[path = "scoped_pr_review_preview_v2_tests.rs"]
mod preview_v2;
#[path = "scoped_pr_review_runtime_v2_tests.rs"]
mod runtime_v2;
#[path = "scoped_pr_review_ui_v2_tests.rs"]
mod ui_v2;
use images::reviews_v2::{
    ReviewDraftCommentV2, ReviewDraftValuesV2, ScopedReviewAnchorIntentV2, ScopedReviewDraftV2,
    ScopedReviewIntentV2, ScopedReviewSelectionV2,
};

fn owner(s: &mut Setup) -> (App, Arc<ClosedPrFileImagesV1>) {
    let result = read(s, &Default::default(), &Default::default()).unwrap();
    let proof = result.proof.clone();
    let mut app = App {
        view: crate::app::View::Inbox,
        // Test color capability is explicit; the shell's NO_COLOR does not
        // decide whether an avatar-enabled contract exercises real pixels.
        no_color: false,
        remote_scope_source: Some(s.source.clone()),
        remote_scope: Some(s.active.clone()),
        github: Some(s.workspace.clone()),
        ..Default::default()
    };
    let (viewer, display_limited) = images::install_images(&mut app, result).unwrap();
    assert!(!display_limited);
    app.pr_file_images = viewer;
    s.workspace = app.github.as_ref().unwrap().clone();
    (app, proof)
}
fn open_steps(h: &history::GitImagesV1, rows: Value) -> Vec<GhStep> {
    let mut steps = opening_steps(h);
    steps.push(catalogue_step(rows));
    steps.extend(closing_steps(h));
    steps
}
fn open(
    s: &Setup,
    intent: Arc<ScopedReviewIntentV2>,
) -> Result<Arc<ScopedReviewDraftV2>, images::reviews_v2::ScopedReviewFailureV2> {
    ScopedReviewDraftV2::open(
        intent,
        &provider(&s.fixture),
        &database(&s.fixture),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
}

#[test]
fn pending_reviews_v2_source_capture_and_actual_canonical_anchor_are_pure_exact_allocation_navigation(
) {
    let mut s = inline_setup(|_, _| {});
    let (mut app, proof) = owner(&mut s);
    let before = commands(&s.fixture).len();
    let intent = ScopedReviewIntentV2::capture(&app).unwrap();
    assert!(intent.owns(&app));
    let range = proof.canonical_range(PrImageSideV1::Old, 1, 2).unwrap();
    let anchor = ScopedReviewAnchorIntentV2::capture(&range, &app).unwrap();
    assert!(anchor.owns(&app));
    let metadata = anchor
        .prepare_metadata(
            &intent,
            &Default::default(),
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
    assert_eq!(metadata.server_path(), history::PATH);
    assert_eq!(metadata.head(), s.h.h);
    assert_eq!(metadata.side(), crate::providers::gh::DiffSide::Left);
    assert_eq!(metadata.first(), 1);
    assert_eq!(metadata.last(), 2);
    let key = intent.draft_key().unwrap();
    assert!(key.starts_with("review-draft/"));
    assert_eq!(key.len(), 77);
    assert_eq!(
        key,
        ScopedReviewIntentV2::capture(&app)
            .unwrap()
            .draft_key()
            .unwrap()
    );
    app.github.as_mut().unwrap().pr.as_mut().unwrap().tab =
        crate::github_workspace::PrTab::Conversation;
    assert!(!intent.owns(&app));
    assert!(!anchor.owns(&app));
    assert!(ScopedReviewIntentV2::capture(&app).is_err());
    assert!(ScopedReviewAnchorIntentV2::capture(&range, &app).is_err());
    app.github.as_mut().unwrap().pr.as_mut().unwrap().tab = crate::github_workspace::PrTab::Files;
    assert!(intent.owns(&app));
    assert!(anchor.owns(&app));
    app.remote_scope_source = None;
    assert!(!intent.owns(&app));
    assert!(!anchor.owns(&app));
    assert!(ScopedReviewIntentV2::capture(&app).is_err());
    assert!(ScopedReviewAnchorIntentV2::capture(&range, &app).is_err());
    assert_eq!(commands(&s.fixture).len(), before);
    unchanged_git_and_exact_script(&s);
}

#[test]
fn pending_reviews_v2_source_open_restores_old_head_metadata_as_text_only_after_actual_closing_proof(
) {
    let mut s = inline_setup(|h, steps| steps.extend(open_steps(h, json!([]))));
    let (app, proof) = owner(&mut s);
    let intent = ScopedReviewIntentV2::capture(&app).unwrap();
    let anchor = ScopedReviewAnchorIntentV2::capture(
        &proof.canonical_range(PrImageSideV1::Old, 1, 2).unwrap(),
        &app,
    )
    .unwrap();
    let comment = ReviewDraftCommentV2::prepare(
        &anchor,
        &intent,
        Arc::from("exact retained pending text"),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let values =
        ReviewDraftValuesV2::replace(Arc::from("restored summary"), vec![comment].into()).unwrap();
    let mut raw: Value = serde_json::from_str(&values.encoded_values().unwrap()).unwrap();
    raw["comments"][0]["anchor"]["head"] = json!(s.h.m);
    let encoded = json!({"version":3,"values":raw,"publication":null}).to_string();
    let key = intent.draft_key().unwrap();
    Cache::open(&database(&s.fixture))
        .unwrap()
        .put_draft(intent.scope(), &key, &encoded)
        .unwrap();
    s.sql_before = sql_state(&s.fixture);
    let before = commands(&s.fixture).len();
    let draft = open(&s, intent).unwrap();
    assert_eq!(commands(&s.fixture).len() - before, 9);
    assert_eq!(draft.identity().account_id, Some(42));
    assert_eq!(draft.identity().login, "reviewer-active");
    assert_eq!(draft.values().summary(), "restored summary");
    assert_eq!(
        draft.values().comments()[0].body(),
        "exact retained pending text"
    );
    assert_eq!(draft.values().comments()[0].anchor().head(), s.h.m);
    assert_ne!(
        draft.values().comments()[0].anchor().head(),
        draft.intent().parent().head_sha()
    );
    assert!(draft.publication().is_none());
    assert!(draft.catalogue().complete());
    assert!(draft.catalogue().rows().is_empty());
    assert_eq!(
        Cache::open(&database(&s.fixture))
            .unwrap()
            .draft(draft.intent().scope(), draft.key())
            .unwrap()
            .as_deref(),
        Some(encoded.as_str())
    );
    unchanged_git_and_exact_script(&s);
}

#[test]
fn pending_reviews_v2_source_selected_actual_other_numeric_author_never_infers_own_pending_from_login(
) {
    let mut s = inline_setup(|h, steps| {
        let mut actual = review(h, "PENDING", "foreign server pending", 43);
        actual["user"]["login"] = json!("reviewer-active");
        steps.extend(open_steps(h, json!([actual.clone()])));
        steps.extend(opening_steps(h));
        steps.extend(snapshot_steps(h, &actual));
        steps.extend(closing_steps(h));
    });
    let (app, _) = owner(&mut s);
    let draft = open(&s, ScopedReviewIntentV2::capture(&app).unwrap()).unwrap();
    let before = commands(&s.fixture).len();
    let selected = ScopedReviewSelectionV2::read(
        &draft,
        0,
        &provider(&s.fixture),
        &database(&s.fixture),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    assert_eq!(commands(&s.fixture).len() - before, 11);
    assert_eq!(selected.identity().account_id, Some(42));
    assert_eq!(selected.snapshot().review().author_account_id(), Some(43));
    assert_eq!(
        selected.snapshot().review().author().login,
        "reviewer-active"
    );
    assert!(!selected.snapshot().review().is_observed_own_pending());
    assert!(selected.snapshot().complete());
    assert!(selected
        .snapshot()
        .comments()
        .iter()
        .all(|comment| comment.author_account_id() == Some(43)));
    unchanged_git_and_exact_script(&s);
}

#[test]
fn pending_reviews_v2_source_values_preflight_rejects_encoding_expansion_and_many_comments_before_any_transport(
) {
    let mut s = inline_setup(|_, _| {});
    let (app, proof) = owner(&mut s);
    let intent = ScopedReviewIntentV2::capture(&app).unwrap();
    let anchor = ScopedReviewAnchorIntentV2::capture(
        &proof.canonical_range(PrImageSideV1::New, 1, 2).unwrap(),
        &app,
    )
    .unwrap();
    let comment = ReviewDraftCommentV2::prepare(
        &anchor,
        &intent,
        Arc::from("one actual original comment"),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let before = commands(&s.fixture).len();
    assert!(
        ReviewDraftValuesV2::replace(Arc::from("\u{1}".repeat(65_536)), Arc::from([])).is_err()
    );
    assert!(ReviewDraftValuesV2::replace(Arc::from("summary"), vec![comment; 33].into()).is_err());
    assert!(ReviewDraftValuesV2::decode_values(
        r#"{"summary":"text","comments":[],"approved":true}"#
    )
    .is_err());
    assert_eq!(commands(&s.fixture).len(), before);
    unchanged_git_and_exact_script(&s);
}

#[test]
fn pending_reviews_v2_source_malformed_saved_approval_refuses_before_catalogue_and_retains_exact_draft(
) {
    let mut s = inline_setup(|h, steps| steps.extend(opening_steps(h)));
    let (app, _) = owner(&mut s);
    let intent = ScopedReviewIntentV2::capture(&app).unwrap();
    let key = intent.draft_key().unwrap();
    let body = r#"{"version":3,"values":{"summary":"KEEP_PRIVATE_TEXT","comments":[],"approved":true},"publication":null}"#;
    Cache::open(&database(&s.fixture))
        .unwrap()
        .put_draft(intent.scope(), &key, body)
        .unwrap();
    s.sql_before = sql_state(&s.fixture);
    let before = commands(&s.fixture).len();
    let error = open(&s, intent.clone()).unwrap_err();
    assert_eq!(commands(&s.fixture).len() - before, 4);
    assert!(!format!("{error:?}").contains("KEEP_PRIVATE_TEXT"));
    assert_eq!(
        Cache::open(&database(&s.fixture))
            .unwrap()
            .draft(intent.scope(), &key)
            .unwrap()
            .as_deref(),
        Some(body)
    );
    unchanged_git_and_exact_script(&s);
}

#[test]
fn pending_reviews_v2_source_account_loss_or_actual_clear_hides_private_and_never_reads_catalogue()
{
    for account_loss in [true, false] {
        let mut s = inline_setup(|h, steps| {
            if account_loss {
                steps.extend(principal(43, "reviewer-active"));
            } else {
                steps.extend(opening_steps(h));
            }
        });
        let (app, _) = owner(&mut s);
        let intent = ScopedReviewIntentV2::capture(&app).unwrap();
        let key = intent.draft_key().unwrap();
        Cache::open(&database(&s.fixture))
            .unwrap()
            .put_draft(intent.scope(), &key, "retained malformed private draft")
            .unwrap();
        if !account_loss {
            Cache::open(&database(&s.fixture))
                .unwrap()
                .clear_records(intent.scope())
                .unwrap();
        }
        s.sql_before = sql_state(&s.fixture);
        let before = commands(&s.fixture).len();
        let error = open(&s, intent.clone()).unwrap_err();
        assert!(error.hide_private);
        assert_eq!(
            commands(&s.fixture).len() - before,
            if account_loss { 2 } else { 4 }
        );
        assert_eq!(
            Cache::open(&database(&s.fixture))
                .unwrap()
                .draft(intent.scope(), &key)
                .unwrap()
                .as_deref(),
            Some("retained malformed private draft")
        );
        unchanged_git_and_exact_script(&s);
    }
}

#[test]
fn pending_reviews_v2_source_cancellation_and_original_expiry_return_no_private_result_or_extra_child(
) {
    let mut s = inline_setup(|_, _| {});
    let (app, _) = owner(&mut s);
    let intent = ScopedReviewIntentV2::capture(&app).unwrap();
    let before = commands(&s.fixture).len();
    let cancelled = CancellationToken::default();
    cancelled.cancel();
    for (cancel, deadline) in [
        (cancelled, Instant::now() + Duration::from_secs(30)),
        (
            CancellationToken::default(),
            Instant::now() - Duration::from_secs(1),
        ),
    ] {
        assert!(ScopedReviewDraftV2::open(
            intent.clone(),
            &provider(&s.fixture),
            &database(&s.fixture),
            &Default::default(),
            &Default::default(),
            &Default::default(),
            &cancel,
            deadline
        )
        .is_err());
    }
    assert_eq!(commands(&s.fixture).len(), before);
    unchanged_git_and_exact_script(&s);
}
