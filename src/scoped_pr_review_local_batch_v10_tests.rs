//! Genuine closed source/draft editors. These declarations are UNRUN; native
//! paired journeys independently exercise the real binary and durable SQL.
use super::*;
use crate::forms::{Form, FormAction, FormPurpose};
use crossterm::event::{KeyCode, KeyModifiers};
use images::reviews_v2::{ui_v2, ReviewRequestedActionV2};

fn setup(count: usize) -> (Setup, App, Arc<ScopedReviewDraftV2>) {
    let mut s = inline_setup(|h, steps| steps.extend(open_steps(h, json!([]))));
    let (app, proof) = owner(&mut s);
    let intent = ScopedReviewIntentV2::capture(&app).unwrap();
    let anchor = ScopedReviewAnchorIntentV2::capture(
        &proof.canonical_range(PrImageSideV1::Old, 1, 2).unwrap(),
        &app,
    )
    .unwrap();
    let mut comments = Vec::new();
    for index in 0..count {
        comments.push(
            ReviewDraftCommentV2::prepare(
                &anchor,
                &intent,
                format!("Original saved local body #{index} 界").into(),
                &Default::default(),
                Instant::now() + Duration::from_secs(30),
            )
            .unwrap(),
        );
    }
    let values =
        ReviewDraftValuesV2::replace(Arc::from("Original local summary"), comments.into()).unwrap();
    let encoded = format!(
        "{{\"version\":3,\"values\":{},\"publication\":null}}",
        values.encoded_values().unwrap()
    );
    Cache::open(&database(&s.fixture))
        .unwrap()
        .put_draft(intent.scope(), &intent.draft_key().unwrap(), &encoded)
        .unwrap();
    s.sql_before = sql_state(&s.fixture);
    let draft = open(&s, intent).unwrap();
    (s, app, draft)
}
fn form(draft: Arc<ScopedReviewDraftV2>) -> Form {
    ui_v2::form_from_draft(
        draft,
        None,
        ReviewRequestedActionV2::CreatePending,
        None,
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap()
}
fn paint(
    app: &mut App,
    form: &mut Form,
    width: u16,
    height: u16,
) -> (String, Vec<crate::app::HitRegion>) {
    app.form = Some(form.clone());
    app.overlay = Some(crate::app::Overlay::Form);
    app.hits.clear();
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| crate::forms::draw(frame, app, frame.area()))
        .unwrap();
    let text = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    *form = app
        .form
        .take()
        .expect("genuine original local form remains owned");
    (text, app.hits.clone())
}
#[test]
fn pending_reviews_v10_local_fields_edit_remove_retains_exact_anchor_and_unchanged_body_allocations(
) {
    let (s, mut app, draft) = setup(2);
    let mut form = form(draft.clone());
    let before = commands(&s.fixture).len();
    let sql = sql_state(&s.fixture);
    paint(&mut app, &mut form, 120, 40);
    let initial = ui_v2::request(&form).unwrap();
    assert_eq!(initial.request().values().comments().len(), 2);
    for (i, comment) in initial.request().values().comments().iter().enumerate() {
        assert!(std::ptr::eq(
            comment.anchor(),
            draft.values().comments()[i].anchor()
        ));
        assert_eq!(
            comment.body().as_ptr(),
            draft.values().comments()[i].body().as_ptr()
        );
    }
    form.dispatch(&FormAction::FieldScroll(1));
    paint(&mut app, &mut form, 80, 24);
    form.key(KeyCode::Char('a'), KeyModifiers::CONTROL);
    form.paste("Edited original local body 界");
    paint(&mut app, &mut form, 80, 24);
    let edited = ui_v2::request(&form).unwrap();
    assert!(!initial.matches_form(&form));
    assert_eq!(
        edited.request().values().comments()[0].body(),
        "Edited original local body 界"
    );
    assert!(std::ptr::eq(
        edited.request().values().comments()[0].anchor(),
        draft.values().comments()[0].anchor()
    ));
    form.prepare_preview_display_v1(
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    assert!(form.preview_display_v1.document.is_some());
    form.dispatch(&FormAction::FieldScroll(1));
    let (text, hits) = paint(&mut app, &mut form, 80, 24);
    assert!(text.contains("Saved local comment #2"));
    assert!(text.contains("[F12 Remove local comment]"));
    let hit = hits
        .iter()
        .find(|hit| hit.action == crate::app::Action::Form(FormAction::RemoveLocalReviewCommentV10))
        .unwrap();
    assert_eq!(hit.rect.width, "[F12 Remove local comment]".len() as u16);
    assert_eq!(
        form.key(KeyCode::F(12), KeyModifiers::NONE),
        Some(FormAction::RemoveLocalReviewCommentV10)
    );
    form.dispatch(&FormAction::RemoveLocalReviewCommentV10);
    assert!(
        form.preview_display_v1.document.is_none(),
        "SAME shared prepared Doc/input epoch retired after local range membership edit"
    );
    assert!(!edited.matches_form(&form));
    assert!(form.dirty);
    assert!(!form.preview);
    assert!(!form.plan_ready());
    paint(&mut app, &mut form, 80, 24);
    let kept = ui_v2::request(&form).unwrap();
    assert_eq!(kept.request().values().comments().len(), 1);
    assert_eq!(
        kept.request().values().comments()[0].body(),
        "Edited original local body 界"
    );
    assert!(std::ptr::eq(
        kept.request().values().comments()[0].anchor(),
        draft.values().comments()[0].anchor()
    ));
    let encoded: Value = serde_json::from_str(&ui_v2::encoded_draft(&form).unwrap()).unwrap();
    assert_eq!(encoded["values"]["comments"].as_array().unwrap().len(), 1);
    assert!(encoded["publication"].is_null());
    assert_eq!(commands(&s.fixture).len(), before);
    assert_eq!(sql_state(&s.fixture), sql);
    unchanged_git_and_exact_script(&s);
}
#[test]
fn pending_reviews_v10_local_removal_refuses_summary_busy_preview_protected_tiny_and_unpainted_selection(
) {
    let (s, mut app, draft) = setup(2);
    let pristine = form(draft);
    let before = commands(&s.fixture).len();
    let sql = sql_state(&s.fixture);
    for mode in 0..6 {
        let mut form = pristine.clone();
        form.dispatch(&FormAction::Field(1));
        paint(&mut app, &mut form, 80, 24);
        match mode {
            0 => form.dispatch(&FormAction::Field(0)),
            1 => form.submitting = true,
            2 => form.preview = true,
            3 => {
                form.publication = Some(crate::pr_write_journal::PrPublicationReference {
                    attempt_id: 1,
                    pr_node_id: draft_node(&form),
                    observed_head: "a".repeat(40),
                    payload_sha256: "a".repeat(64),
                })
            }
            4 => {
                let (text, hits) = paint(&mut app, &mut form, 40, 14);
                assert!(text.contains("resize required"));
                assert!(!hits.iter().any(|hit| hit.action
                    == crate::app::Action::Form(FormAction::RemoveLocalReviewCommentV10)));
            }
            _ => form.dispatch(&FormAction::Field(2)), // old painted #1 cannot remove newly selected #2
        }
        let bodies = form
            .fields
            .iter()
            .map(|field| field.editor.text.clone())
            .collect::<Vec<_>>();
        form.dispatch(&FormAction::RemoveLocalReviewCommentV10);
        assert_eq!(
            form.fields
                .iter()
                .map(|field| field.editor.text.clone())
                .collect::<Vec<_>>(),
            bodies,
            "no hidden/deferred local deletion mode{mode}"
        );
    }
    assert_eq!(commands(&s.fixture).len(), before);
    assert_eq!(sql_state(&s.fixture), sql);
    unchanged_git_and_exact_script(&s);
}
fn draft_node(form: &Form) -> String {
    let FormPurpose::PrReviewV2(state) = &form.purpose else {
        unreachable!()
    };
    state.intent().parent().node_id().into()
}
#[test]
fn pending_reviews_v10_blank_is_not_remove_and_encoded_preflight_refuses_before_replacement_body_allocations(
) {
    let (s, mut app, draft) = setup(2);
    let mut form = form(draft.clone());
    let before = commands(&s.fixture).len();
    form.dispatch(&FormAction::Field(1));
    paint(&mut app, &mut form, 80, 24);
    form.key(KeyCode::Char('a'), KeyModifiers::CONTROL);
    form.paste(" ");
    assert!(ui_v2::encoded_draft(&form).is_err());
    assert_eq!(form.fields.len(), 3);
    assert_eq!(draft.values().comments().len(), 2);
    form.fields[1].editor = crate::editor::TextBuffer::new("\u{1}".repeat(30_000)).unwrap();
    assert!(
        ui_v2::encoded_draft(&form).is_err(),
        "escaped JSON cap, not only raw input length"
    );
    assert_eq!(form.fields[1].editor.text.len(), 30_000);
    assert_eq!(
        draft.values().comments()[0].body(),
        "Original saved local body #0 界"
    );
    assert_eq!(commands(&s.fixture).len(), before);
    unchanged_git_and_exact_script(&s);
}
#[test]
fn pending_reviews_v10_all_32_retained_fields_reachable_keys_mouse_with_complete_action_at_80_and_120(
) {
    let (s, mut app, draft) = setup(32);
    let before = commands(&s.fixture).len();
    let mut form = form(draft);
    for (width, height) in [(80, 24), (120, 40)] {
        form.dispatch(&FormAction::Field(0));
        paint(&mut app, &mut form, width, height);
        for index in 0..32 {
            form.dispatch(&FormAction::FieldScroll(1));
            let (text, hits) = paint(&mut app, &mut form, width, height);
            assert_eq!(form.active_field, index + 1);
            assert!(text.contains(&format!("Saved local comment #{}:", index + 1)));
            assert!(hits.iter().any(|hit| hit.action
                == crate::app::Action::Form(FormAction::Field(index + 1))
                && !hit.rect.is_empty()));
            assert!(hits.iter().any(|hit| hit.action
                == crate::app::Action::Form(FormAction::RemoveLocalReviewCommentV10)));
            assert_eq!(
                form.key(KeyCode::F(12), KeyModifiers::NONE),
                Some(FormAction::RemoveLocalReviewCommentV10)
            );
        }
    }
    assert_eq!(commands(&s.fixture).len(), before);
    unchanged_git_and_exact_script(&s);
}
#[test]
fn pending_reviews_v10_cancelled_expired_worker_map_and_malformed_fields_fail_without_git_sql_or_authority(
) {
    let (s, mut app, draft) = setup(2);
    let before = commands(&s.fixture).len();
    let sql = sql_state(&s.fixture);
    for expired in [false, true] {
        let token = CancellationToken::default();
        if !expired {
            token.cancel();
        }
        assert!(ui_v2::form_from_draft(
            draft.clone(),
            None,
            ReviewRequestedActionV2::CreatePending,
            None,
            &token,
            if expired {
                Instant::now() - Duration::from_millis(1)
            } else {
                Instant::now() + Duration::from_secs(30)
            }
        )
        .is_err());
    }
    for malformed in [false, true] {
        let mut form = form(draft.clone());
        form.dispatch(&FormAction::Field(1));
        paint(&mut app, &mut form, 120, 40);
        if malformed {
            form.fields[1].key = "local-review-comment-FOREIGN".into();
        } else {
            form.fields.swap(1, 2);
        }
        let saved = form
            .fields
            .iter()
            .map(|field| (field.key.clone(), field.editor.text.clone()))
            .collect::<Vec<_>>();
        form.dispatch(&FormAction::RemoveLocalReviewCommentV10);
        assert!(ui_v2::request(&form).is_err());
        assert_eq!(
            form.fields
                .iter()
                .map(|field| (field.key.clone(), field.editor.text.clone()))
                .collect::<Vec<_>>(),
            saved
        );
    }
    assert_eq!(commands(&s.fixture).len(), before);
    assert_eq!(sql_state(&s.fixture), sql);
    unchanged_git_and_exact_script(&s);
}
#[test]
fn pending_reviews_v10_retired_actual_source_cannot_expose_or_remove_saved_local_fields_on_next_frame(
) {
    let (s, mut app, draft) = setup(2);
    let mut form = form(draft);
    form.dispatch(&FormAction::Field(1));
    paint(&mut app, &mut form, 80, 24);
    let before = commands(&s.fixture).len();
    let sql = sql_state(&s.fixture);
    app.form = Some(form);
    app.remote_scope_source = None;
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|frame| crate::forms::draw(frame, &mut app, frame.area()))
        .unwrap();
    assert!(app.form.is_none());
    assert!(
        !app.hits
            .iter()
            .any(|hit| hit.action
                == crate::app::Action::Form(FormAction::RemoveLocalReviewCommentV10))
    );
    assert_eq!(commands(&s.fixture).len(), before);
    assert_eq!(sql_state(&s.fixture), sql);
    unchanged_git_and_exact_script(&s);
}
#[test]
fn pending_reviews_v10_remove_last_local_comment_preserves_summary_and_never_acquires_server_delete_or_preview(
) {
    let (s, mut app, draft) = setup(1);
    let mut form = form(draft);
    let before = commands(&s.fixture).len();
    let sql = sql_state(&s.fixture);
    form.dispatch(&FormAction::Field(1));
    paint(&mut app, &mut form, 80, 24);
    form.dispatch(&FormAction::RemoveLocalReviewCommentV10);
    paint(&mut app, &mut form, 80, 24);
    let request = ui_v2::request(&form).unwrap();
    assert!(request.request().values().comments().is_empty());
    assert_eq!(
        request.request().values().summary(),
        "Original local summary"
    );
    assert_eq!(form.fields.len(), 1);
    assert!(!form.plan_ready());
    assert!(!form.preview);
    assert!(!form.submit_requested);
    assert!(form.publication.is_none());
    let saved = ui_v2::encoded_draft(&form).unwrap();
    form.dispatch(&FormAction::RemoveLocalReviewCommentV10);
    assert_eq!(ui_v2::encoded_draft(&form).unwrap(), saved);
    assert_eq!(commands(&s.fixture).len(), before);
    assert_eq!(sql_state(&s.fixture), sql);
    unchanged_git_and_exact_script(&s);
}
