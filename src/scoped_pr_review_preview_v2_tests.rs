//! Actual source/Preview/journal/SQL/ordinary POST+receipt protocols. These are
//! source declarations until Root executes the selected nonzero gate.
use super::*;
use crate::{
    drafts::DraftWriter,
    pr_write_journal::{PrWriteState, PrWriteTarget},
};
use images::reviews_v2::{
    ui_v2, ReviewRequestedActionV2, ScopedReviewPreviewV2, ScopedReviewPublicationStateV2,
    ScopedReviewRequestV2,
};

fn preview_steps(h: &history::GitImagesV1) -> Vec<GhStep> {
    open_steps(h, json!([]))
}
fn zero_comment_receipt(_h: &history::GitImagesV1, row: &Value) -> Vec<GhStep> {
    vec![
        get(&format!("repos/{REPO}/pulls/7/reviews/810"), row.clone()),
        get(
            &format!("repos/{REPO}/pulls/7/reviews/810/comments?per_page=100&page=1"),
            json!([]),
        ),
        get(&format!("repos/{REPO}/pulls/7/reviews/810"), row.clone()),
    ]
}
fn confirm_zero(h: &history::GitImagesV1) -> Vec<GhStep> {
    let actual = review(h, "PENDING", "pending summary", 42);
    let mut steps = opening_steps(h);
    steps.extend(guard_steps(h));
    steps.push(catalogue_step(json!([])));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), current_parent(h)));
    steps.extend(closing_steps(h));
    steps.push(write(
        &format!("repos/{REPO}/pulls/7/reviews"),
        "POST",
        Some(json!({"body":"pending summary","commit_id":h.h,"comments":[]})),
        actual.clone(),
    ));
    steps.extend(zero_comment_receipt(h, &actual));
    // Direct prepaid API children, not identity()/another version/auth bracket.
    steps.push(get("user", json!({"id":42,"login":"renamed-after-post"})));
    steps.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), current_parent(h)));
    steps.push(get(
        "user",
        json!({"id":42,"login":"closing-renamed-account"}),
    ));
    steps
}
fn prepared_summary(s: &mut Setup) -> (Arc<ScopedReviewDraftV2>, Arc<ScopedReviewPreviewV2>) {
    let (app, _) = owner(s);
    let draft = open(s, ScopedReviewIntentV2::capture(&app).unwrap()).unwrap();
    let values = Arc::new(
        ReviewDraftValuesV2::replace(Arc::from("pending summary"), Arc::from([])).unwrap(),
    );
    let request = ScopedReviewRequestV2::create(&draft, values);
    let preview = ScopedReviewPreviewV2::prepare(
        request,
        &provider(&s.fixture),
        &database(&s.fixture),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    (draft, preview)
}
fn execute(
    s: &Setup,
    preview: &Arc<ScopedReviewPreviewV2>,
    writer: &DraftWriter,
) -> images::reviews_v2::ScopedReviewPublicationV2 {
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let client = writer.client();
    let fence = client
        .capture_publication_v2(
            &database(&s.fixture),
            preview.request().intent().scope(),
            preview.draft_key(),
            &cancel,
            deadline,
        )
        .unwrap();
    preview
        .execute(
            &provider(&s.fixture),
            &database(&s.fixture),
            &client,
            fence,
            true,
            None,
            &Default::default(),
            &Default::default(),
            &Default::default(),
            &Default::default(),
            &cancel,
            deadline,
        )
        .unwrap()
}
fn marker(
    s: &Setup,
    preview: &ScopedReviewPreviewV2,
    result: &images::reviews_v2::ScopedReviewPublicationV2,
) -> crate::pr_write_journal::PrWriteMarker {
    let target = PrWriteTarget::for_source(
        preview.request().intent().scope(),
        REPO,
        preview.request().intent().parent().node_id(),
        7,
    )
    .unwrap();
    PrWriteJournal::open(&database(&s.fixture))
        .unwrap()
        .original_publication(&target, result.reference())
        .unwrap()
        .unwrap()
}
fn review_canaries(s: &Setup, key: &str, posts: usize) {
    assert_eq!(git_state_resource(&s.fixture), s.git_before);
    assert_eq!(
        fs::read(s.fixture.repo().join("LOCAL_DIRTY_CANARY")).unwrap(),
        b"LOCAL_WORKTREE_UNTOUCHED\0\xff\n"
    );
    let calls = commands(&s.fixture);
    assert_eq!(calls.len(), s.steps.len());
    for (actual, step) in calls.iter().zip(&s.steps) {
        assert_eq!(actual["matched"], true);
        assert_eq!(actual["args"], json!(step.args));
        assert_eq!(
            actual["stdin_hex"],
            step.stdin
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
    }
    assert_eq!(
        calls
            .iter()
            .filter(|call| call["args"]
                .as_array()
                .unwrap()
                .iter()
                .any(|arg| arg == "POST"))
            .count(),
        posts
    );
    let unrelated = |rows: Vec<String>| {
        rows.into_iter()
            .filter(|row| {
                let row: Value = serde_json::from_str(row).unwrap();
                !(row.as_array().is_some_and(|array| array.len() == 4)
                    && row[0] == HOST
                    && row[1] == "user-id:42"
                    && row[2] == key)
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        unrelated(sql_state(&s.fixture)),
        unrelated(s.sql_before.clone())
    );
    let foreign:i64=sql(&s.fixture).query_row("SELECT (SELECT count(*) FROM drafts WHERE login='user-id:43')+(SELECT count(*) FROM pr_write_attempts WHERE principal='user-id:43')",[],|row|row.get(0)).unwrap();
    assert_eq!(foreign, 0);
}
fn paint_form(s: &Setup, form: &mut crate::forms::Form, width: u16, height: u16) -> String {
    let mut app = App {
        view: crate::app::View::Inbox,
        remote_scope_source: Some(s.source.clone()),
        remote_scope: Some(s.active.clone()),
        github: Some(s.workspace.clone()),
        form: Some(form.clone()),
        overlay: Some(crate::app::Overlay::Form),
        ..Default::default()
    };
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| crate::forms::draw(frame, &mut app, frame.area()))
        .unwrap();
    let text = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    *form = app
        .form
        .take()
        .expect("actual source-bound form survives useful or resize-only draw");
    text
}

#[test]
fn pending_reviews_v2_preview_genuine_close_current_uid_summary_and_no_cas_claim() {
    let mut s = inline_setup(|h, steps| {
        steps.extend(open_steps(h, json!([])));
        steps.extend(preview_steps(h));
    });
    let before = commands(&s.fixture).len();
    let (draft, preview) = prepared_summary(&mut s);
    assert_eq!(commands(&s.fixture).len() - before, 17 + 9 + 9);
    assert_eq!(preview.identity().account_id, Some(42));
    assert_eq!(preview.identity().login, "reviewer-active");
    assert!(Arc::ptr_eq(preview.request().intent(), draft.intent()));
    assert_eq!(preview.draft_key(), draft.key());
    assert!(preview
        .lines()
        .iter()
        .any(|line| line.contains("not an expected-head CAS")
            || line.contains("Not an expected-head CAS")));
    assert!(preview.lines().iter().any(|line| line.contains(&s.h.h)));
    assert!(preview
        .lines()
        .iter()
        .any(|line| line.contains("pending summary")));
    unchanged_git_and_exact_script(&s);
}

#[test]
fn pending_reviews_v2_confirm_actual_pending_receipt_once_terminal_composite_source_retirement_and_safe_id_only(
) {
    let mut s = inline_setup(|h, steps| {
        steps.extend(open_steps(h, json!([])));
        steps.extend(preview_steps(h));
        steps.extend(confirm_zero(h));
    });
    let (_, preview) = prepared_summary(&mut s);
    let writer = DraftWriter::new(database(&s.fixture));
    let before = commands(&s.fixture).len();
    let result = execute(&s, &preview, &writer);
    assert_eq!(commands(&s.fixture).len() - before, 21);
    assert_eq!(
        result.state(),
        ScopedReviewPublicationStateV2::ConfirmedSnapshotRetired
    );
    assert_eq!(result.confirmed_review_id(), Some(810));
    assert!(result.source_retired());
    assert!(result.hide_private());
    assert!(!result.draft_retained());
    assert!(!format!("{result:?}").contains("pending summary"));
    let actual = marker(&s, &preview, &result);
    assert_eq!(actual.state, PrWriteState::Confirmed);
    assert_eq!(actual.command, "review-create-pending-v2");
    assert_eq!(actual.selected_resource, preview.draft_key());
    assert_eq!(actual.confirmed.unwrap().resource_id, "810");
    let cache = Cache::open(&database(&s.fixture)).unwrap();
    assert!(cache
        .draft(preview.request().intent().scope(), preview.draft_key())
        .unwrap()
        .is_none());
    assert!(cache
        .validate_repository_catalogue_intent_source(&s.source)
        .is_err());
    let before = commands(&s.fixture).len();
    let again = execute_with_fresh_virgin_fence(&s, &preview);
    assert!(again.is_err());
    assert_eq!(commands(&s.fixture).len(), before);
    review_canaries(&s, preview.draft_key(), 1);
}
fn execute_with_fresh_virgin_fence(
    s: &Setup,
    preview: &ScopedReviewPreviewV2,
) -> Result<images::reviews_v2::ScopedReviewPublicationV2, images::reviews_v2::ScopedReviewFailureV2>
{
    let writer = DraftWriter::new(database(&s.fixture));
    let client = writer.client();
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let fence = client
        .capture_publication_v2(
            &database(&s.fixture),
            preview.request().intent().scope(),
            preview.draft_key(),
            &cancel,
            deadline,
        )
        .unwrap();
    preview.execute(
        &provider(&s.fixture),
        &database(&s.fixture),
        &client,
        fence,
        true,
        None,
        &Default::default(),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        &cancel,
        deadline,
    )
}

#[test]
fn pending_reviews_v2_invoked_post_failure_keeps_original_reference_body_uncertain_and_never_automatically_resends(
) {
    let mut s = inline_setup(|h, steps| {
        steps.extend(open_steps(h, json!([])));
        steps.extend(preview_steps(h));
        let mut confirm = confirm_zero(h);
        confirm[13].status = 1;
        confirm[13].stdout.clear();
        confirm[13].stderr = b"PRIVATE_REVIEW_POST_DIAGNOSTIC".to_vec();
        confirm.truncate(14);
        steps.extend(confirm);
    });
    let (_, preview) = prepared_summary(&mut s);
    let writer = DraftWriter::new(database(&s.fixture));
    let result = execute(&s, &preview, &writer);
    assert_eq!(result.state(), ScopedReviewPublicationStateV2::Uncertain);
    assert!(result.confirmed_review_id().is_none());
    assert!(result.draft_retained());
    let actual = marker(&s, &preview, &result);
    assert_eq!(actual.state, PrWriteState::Uncertain);
    let body = Cache::open(&database(&s.fixture))
        .unwrap()
        .draft(preview.request().intent().scope(), preview.draft_key())
        .unwrap()
        .unwrap();
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["version"], 3);
    assert_eq!(body["values"]["summary"], "pending summary");
    assert_eq!(
        body["publication"],
        serde_json::to_value(result.reference()).unwrap()
    );
    assert!(body.get("approved").is_none());
    assert!(!format!("{result:?}").contains("PRIVATE_REVIEW"));
    review_canaries(&s, preview.draft_key(), 1);
}

#[test]
fn pending_reviews_v2_external_draft_replacement_after_claim_refuses_before_post_and_never_deletes_newer_body(
) {
    let mut s = inline_setup(|h, steps| {
        steps.extend(open_steps(h, json!([])));
        steps.extend(preview_steps(h));
        steps.extend(confirm_zero(h).into_iter().take(11));
    });
    let (_, preview) = prepared_summary(&mut s);
    rusqlite::Connection::open(database(&s.fixture)).unwrap().execute_batch("CREATE TRIGGER replace_review_on_claim AFTER UPDATE OF generation ON pr_write_scopes WHEN NEW.generation=2 BEGIN UPDATE drafts SET body='EXTERNAL_NEWER_DRAFT' WHERE host=NEW.host AND login=NEW.principal AND resource LIKE 'review-draft/%'; END;").unwrap();
    let writer = DraftWriter::new(database(&s.fixture));
    let result = execute(&s, &preview, &writer);
    assert_eq!(result.state(), ScopedReviewPublicationStateV2::NotStarted);
    assert!(result.confirmed_review_id().is_none());
    assert_eq!(marker(&s, &preview, &result).state, PrWriteState::Pending);
    assert_eq!(
        Cache::open(&database(&s.fixture))
            .unwrap()
            .draft(preview.request().intent().scope(), preview.draft_key())
            .unwrap()
            .as_deref(),
        Some("EXTERNAL_NEWER_DRAFT")
    );
    review_canaries(&s, preview.draft_key(), 0);
}

#[test]
fn pending_reviews_v2_external_draft_replacement_after_receipt_retains_confirmed_id_but_refuses_cleanup_and_private_receipt(
) {
    let mut s = inline_setup(|h, steps| {
        steps.extend(open_steps(h, json!([])));
        steps.extend(preview_steps(h));
        steps.extend(confirm_zero(h).into_iter().take(20));
    });
    let (_, preview) = prepared_summary(&mut s);
    rusqlite::Connection::open(database(&s.fixture)).unwrap().execute_batch("CREATE TRIGGER replace_review_after_confirmation AFTER UPDATE OF generation ON pr_write_scopes WHEN NEW.generation=3 BEGIN UPDATE drafts SET body='EXTERNAL_NEWER_CONFIRMED_DRAFT' WHERE host=NEW.host AND login=NEW.principal AND resource LIKE 'review-draft/%'; END;").unwrap();
    let writer = DraftWriter::new(database(&s.fixture));
    let result = execute(&s, &preview, &writer);
    assert_eq!(
        result.state(),
        ScopedReviewPublicationStateV2::ConfirmedButSourceUnavailable
    );
    assert_eq!(result.confirmed_review_id(), Some(810));
    assert!(result.hide_private());
    assert!(result.draft_retained());
    assert_eq!(marker(&s, &preview, &result).state, PrWriteState::Confirmed);
    assert_eq!(
        Cache::open(&database(&s.fixture))
            .unwrap()
            .draft(preview.request().intent().scope(), preview.draft_key())
            .unwrap()
            .as_deref(),
        Some("EXTERNAL_NEWER_CONFIRMED_DRAFT")
    );
    review_canaries(&s, preview.draft_key(), 1);
}

#[test]
fn pending_reviews_v2_selected_action_drafts_are_separate_from_batch_and_from_each_decision_after_fresh_source_close(
) {
    let mut s = inline_setup(|h, steps| {
        let row = review(h, "PENDING", "fresh current server summary", 42);
        steps.extend(open_steps(h, json!([row.clone()])));
        steps.extend(opening_steps(h));
        steps.extend(snapshot_steps(h, &row));
        steps.extend(closing_steps(h));
        steps.extend(open_steps(h, json!([row])));
    });
    let (app, proof) = owner(&mut s);
    let draft = open(&s, ScopedReviewIntentV2::capture(&app).unwrap()).unwrap();
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
    let key = selected
        .action_key(ReviewRequestedActionV2::UpdateSummary)
        .unwrap();
    let anchor = ScopedReviewAnchorIntentV2::capture(
        &proof.canonical_range(PrImageSideV1::Old, 1, 2).unwrap(),
        &app,
    )
    .unwrap();
    let comment = ReviewDraftCommentV2::prepare(
        &anchor,
        draft.intent(),
        Arc::from("ORIGINAL_BATCH_COMMENT"),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let values =
        ReviewDraftValuesV2::replace(Arc::from("ORIGINAL_BATCH_BODY"), vec![comment].into())
            .unwrap();
    let batch=json!({"version":3,"values":serde_json::from_str::<Value>(&values.encoded_values().unwrap()).unwrap(),"publication":null}).to_string();
    let action = r#"{"version":3,"values":{"summary":"RESTORED_ACTION_BODY","comments":[]},"publication":null}"#;
    let cache = Cache::open(&database(&s.fixture)).unwrap();
    cache
        .put_draft(draft.intent().scope(), draft.key(), &batch)
        .unwrap();
    cache
        .put_draft(draft.intent().scope(), &key, action)
        .unwrap();
    s.sql_before = sql_state(&s.fixture);
    let before = commands(&s.fixture).len();
    let actual = ScopedReviewDraftV2::open_selected(
        &selected,
        ReviewRequestedActionV2::UpdateSummary,
        &provider(&s.fixture),
        &database(&s.fixture),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    assert_eq!(commands(&s.fixture).len() - before, 9);
    let form = ui_v2::form_from_draft(
        actual,
        Some(selected.clone()),
        ReviewRequestedActionV2::UpdateSummary,
        None,
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    assert_eq!(form.fields[0].editor.text, "RESTORED_ACTION_BODY");
    assert_eq!(form.resource(), key);
    assert!(form.publication.is_none());
    assert!(!form.plan_ready());
    assert_ne!(
        selected
            .action_key(ReviewRequestedActionV2::Approve)
            .unwrap(),
        selected
            .action_key(ReviewRequestedActionV2::SubmitComment)
            .unwrap()
    );
    assert_ne!(
        selected
            .action_key(ReviewRequestedActionV2::RequestChanges)
            .unwrap(),
        key
    );
    assert_eq!(
        cache
            .draft(draft.intent().scope(), draft.key())
            .unwrap()
            .as_deref(),
        Some(batch.as_str())
    );
    let retained: Value = serde_json::from_str(&batch).unwrap();
    assert_eq!(
        retained["values"]["comments"][0]["body"],
        "ORIGINAL_BATCH_COMMENT"
    );
    assert_eq!(retained["values"]["comments"].as_array().unwrap().len(), 1);
    unchanged_git_and_exact_script(&s);
}

#[test]
fn pending_reviews_v2_shared_form_actual_ticket_edit_paste_and_editor_invalidate_preview_without_transport_or_restored_approval(
) {
    let mut s = inline_setup(|h, steps| {
        steps.extend(open_steps(h, json!([])));
        steps.extend(preview_steps(h));
    });
    let (app, _) = owner(&mut s);
    let draft = open(&s, ScopedReviewIntentV2::capture(&app).unwrap()).unwrap();
    let mut form = ui_v2::form_from_draft(
        draft.clone(),
        None,
        ReviewRequestedActionV2::CreatePending,
        None,
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    assert!(form.dirty);
    form.paste("pending summary");
    assert!(ui_v2::request(&form).is_err());
    assert!(paint_form(&s, &mut form, 120, 40).contains("Review summary"));
    let request = ui_v2::request(&form).unwrap();
    let preview = ScopedReviewPreviewV2::prepare(
        request.request().clone(),
        &provider(&s.fixture),
        &database(&s.fixture),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    assert!(ui_v2::accept_preview(&mut form, &request, preview));
    assert!(!form.plan_ready());
    assert!(form.preview);
    assert!(ui_v2::confirmed_preview(&form).is_err());
    assert!(paint_form(&s, &mut form, 120, 40).contains("pending summary"));
    assert!(form.plan_ready());
    assert!(ui_v2::confirmed_preview(&form).is_ok());
    let before_tiny = commands(&s.fixture).len();
    assert!(paint_form(&s, &mut form, 40, 12).contains("resize"));
    form.dispatch(&crate::forms::FormAction::Submit);
    assert!(!form.submit_requested);
    assert!(ui_v2::confirmed_preview(&form).is_err());
    assert_eq!(commands(&s.fixture).len(), before_tiny);
    paint_form(&s, &mut form, 120, 40);
    assert!(form.plan_ready());
    let before = commands(&s.fixture).len();
    ui_v2::replace_editor_result(&mut form, "pending summary").unwrap();
    assert!(!form.plan_ready());
    assert!(!request.matches_form(&form));
    let old = form.fields[0].editor.text.clone();
    assert!(ui_v2::replace_editor_result(&mut form, &"x".repeat(65_537)).is_err());
    assert_eq!(form.fields[0].editor.text, old);
    form.paste(&"x".repeat(65_537));
    assert_eq!(form.fields[0].editor.text, old);
    assert_eq!(commands(&s.fixture).len(), before);
    let encoded: Value = serde_json::from_str(&ui_v2::encoded_draft(&form).unwrap()).unwrap();
    assert_eq!(encoded["version"], 3);
    assert!(encoded.get("approved").is_none());
    let mut other = ui_v2::form_from_draft(
        draft,
        None,
        ReviewRequestedActionV2::CreatePending,
        None,
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    other.fields[0].editor = crate::editor::TextBuffer::new("pending summary").unwrap();
    assert!(!request.matches_form(&other));
    assert!(other.restore_draft(&encoded.to_string()).is_err());
    unchanged_git_and_exact_script(&s);
}
