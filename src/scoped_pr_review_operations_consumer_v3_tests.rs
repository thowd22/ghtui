//! Genuine Files/review/modern-pending factories plus actual SQL; declarations
//! remain UNRUN. UI/handler contracts supplement, never replace native PTYs.
use super::*;
#[path = "scoped_pr_review_action_lineage_v12_tests.rs"]
mod action_lineage_v12;
#[path = "scoped_pr_review_operation_anchor_v9_tests.rs"]
mod anchor_v9;
#[path = "scoped_pr_review_recovery_v11_tests.rs"]
mod recovery_v11;
use super::super::pending_operations_v3::{graph_comment_v3, graph_review_v3, source_steps_v3};
use crate::{
    pr_write_journal::{PrConfirmedEffect, PrEffectKind, PrWriteTarget},
    search::{
        collaboration::{DiscussionTarget, ParentKind},
        model::DiscussionKind,
    },
    storage::{Record, SyncBindingKind},
};
use images::reviews_v2::operations_v3::{
    self as operations, ScopedReviewOperationIntentV3, ScopedReviewOperationSourceV3,
};

fn operation_fixture(reads: usize) -> (Setup, App, Arc<ScopedReviewOperationIntentV3>) {
    let mut s = inline_setup(|h, steps| {
        let row = review(h, "PENDING", "pending summary", 42);
        steps.extend(open_steps(h, json!([row.clone()])));
        steps.extend(opening_steps(h));
        steps.extend(snapshot_steps(h, &row));
        steps.extend(closing_steps(h));
        for _ in 0..reads {
            steps.extend(source_steps_v3(
                h,
                graph_review_v3(h),
                Some(graph_comment_v3(h)),
            ));
        }
    });
    let (app, _) = owner(&mut s);
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
    let intent = ScopedReviewOperationIntentV3::edit(selected, 0, false).unwrap();
    (s, app, intent)
}
fn operation_source(
    s: &Setup,
    intent: Arc<ScopedReviewOperationIntentV3>,
) -> Arc<ScopedReviewOperationSourceV3> {
    ScopedReviewOperationSourceV3::open(
        intent,
        &provider(&s.fixture),
        &database(&s.fixture),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap()
}
fn target(source: &ScopedReviewOperationSourceV3, category: DiscussionKind) -> DiscussionTarget {
    let original = source.intent().original();
    let parent = original.parent();
    DiscussionTarget {
        scope: original.scope().clone(),
        repository_id: parent.repository_id(),
        repository: parent.repository().into(),
        parent_kind: ParentKind::PullRequest,
        parent_node: parent.node_id().into(),
        number: parent.number(),
        category,
    }
}
fn hydrate(path: &std::path::Path, target: &DiscussionTarget) -> u64 {
    let mut cache = Cache::open(path).unwrap();
    let name = target.source();
    let generation = cache
        .reserve_resource_sync(&target.scope, &name, &target.binding().unwrap())
        .unwrap();
    assert!(cache
        .commit_reserved_sync(
            &target.scope,
            &name,
            generation,
            &[Record::new(
                "collaboration-document-v1",
                &name,
                &target.repository,
                json!({"body":"ORIGINAL retained discussion bytes"})
            )],
            true
        )
        .unwrap());
    generation
}
fn visible(path: &std::path::Path, target: &DiscussionTarget) -> Option<u64> {
    Cache::open(path)
        .unwrap()
        .binding_status(&target.scope, &target.source())
        .unwrap()
        .unwrap()
        .visible_generation
}

#[test]
fn pending_reviews_v3_consumer_original_action_draft_isolated_from_batch_and_restore_is_body_only_not_approval(
) {
    let (mut s, app, intent) = operation_fixture(1);
    let key = intent.draft_key().unwrap();
    let batch_key = ScopedReviewIntentV2::capture(&app)
        .unwrap()
        .draft_key()
        .unwrap();
    let cache = Cache::open(&database(&s.fixture)).unwrap();
    cache
        .put_draft(
            intent.original().scope(),
            &batch_key,
            "ORIGINAL batch comments/body/reference kept",
        )
        .unwrap();
    let encoded = operations::draft_v3::encode("Restored exact operation body 界", None).unwrap();
    cache
        .put_draft(intent.original().scope(), &key, &encoded)
        .unwrap();
    drop(cache);
    // Exact expected full draft/cache rows after ONLY these deliberate own
    // inserts, BEFORE opening/restoring the source under test. No poststate
    // adoption or exclusion of another private resource.
    s.sql_before = sql_state(&s.fixture);
    let source = operation_source(&s, intent.clone());
    assert_eq!(source.body(), "Restored exact operation body 界");
    assert!(source.restored_body());
    assert_eq!(source.identity().account_id, Some(42));
    let form = operations::ui_v3::form_from_source(
        source,
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    assert!(form.publication.is_none());
    assert!(form.restored_draft);
    assert!(form.dirty);
    assert!(!form.preview);
    assert!(!form.plan_ready());
    assert!(
        operations::ui_v3::request(&form).is_err(),
        "unpainted/restored text cannot be publication intent"
    );
    assert_eq!(
        Cache::open(&database(&s.fixture))
            .unwrap()
            .draft(intent.original().scope(), &batch_key)
            .unwrap()
            .as_deref(),
        Some("ORIGINAL batch comments/body/reference kept")
    );
    unchanged_git_and_exact_script(&s);
}

#[test]
fn pending_reviews_v3_consumer_exact_fresh_preview_prepares_document_under_original_clock_and_cancelled_phase_has_zero_children(
) {
    let (s, mut app, intent) = operation_fixture(2);
    let source = operation_source(&s, intent);
    let mut form = operations::ui_v3::form_from_source(
        source.clone(),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    form.fields[0].editor = crate::editor::TextBuffer::new("Fresh exact operation body").unwrap();
    app.form = Some(form);
    app.overlay = Some(crate::app::Overlay::Form);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| crate::forms::draw(frame, &mut app, frame.area()))
        .unwrap();
    let form_request = operations::ui_v3::request(app.form.as_ref().unwrap()).unwrap();
    let request = form_request.request().clone();
    let preview = operations::preview_v3::ReviewOperationPreviewV3::prepare(
        request.clone(),
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
    assert!(Arc::ptr_eq(preview.request(), &request));
    assert!(!Arc::ptr_eq(preview.source(), &source));
    assert_eq!(preview.source().body(), request.body());
    assert_eq!(preview.draft_key(), source.key());
    let prepared_doc = preview.source().display().clone();
    assert!(operations::ui_v3::accept_preview(
        app.form.as_mut().unwrap(),
        &form_request,
        preview
    ));
    assert!(Arc::ptr_eq(
        app.form
            .as_ref()
            .unwrap()
            .preview_display_v1
            .document
            .as_ref()
            .unwrap(),
        &prepared_doc
    ));
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
    assert!(text.contains("Fresh exact operation body"));
    assert!(text.contains("READ-ONLY PREVIEW"));
    assert!(text.contains("[Confirm action]"));
    assert!(app.form.as_ref().unwrap().plan_ready());
    assert!(Arc::ptr_eq(app.form.as_ref().unwrap().preview_display_v1.document.as_ref().unwrap(),&prepared_doc),"paint borrows SAME original prepared document, never reconstructs a display or provider clock");
    let before = commands(&s.fixture).len();
    let sql = sql_state(&s.fixture);
    let cancelled = CancellationToken::default();
    cancelled.cancel();
    assert!(operations::preview_v3::ReviewOperationPreviewV3::prepare(
        request,
        &provider(&s.fixture),
        &database(&s.fixture),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        &cancelled,
        Instant::now() + Duration::from_secs(30)
    )
    .is_err());
    assert_eq!(commands(&s.fixture).len(), before);
    assert_eq!(sql_state(&s.fixture), sql);
    unchanged_git_and_exact_script(&s);
}

#[test]
fn pending_reviews_v3_actual_origin_is_atomic_with_begin_and_crash_gap_read_recovers_both_categories_once_without_writes(
) {
    for reverse in [false, true] {
        let (mut s, _app, intent) = operation_fixture(1);
        let source = operation_source(&s, intent);
        let path = database(&s.fixture);
        let reviews = target(&source, DiscussionKind::Review);
        let inline = target(&source, DiscussionKind::InlineComment);
        let neighbour = target(&source, DiscussionKind::ConversationComment);
        assert_eq!(hydrate(&path, &reviews), 1);
        assert_eq!(hydrate(&path, &inline), 1);
        let kept = hydrate(&path, &neighbour);
        assert_eq!(kept, 1);
        // Seeded own rows are an explicit pre-mutation baseline, not changes
        // made by the original source read or the journal operation below.
        s.sql_before = sql_state(&s.fixture);
        let original_sql = s.sql_before.clone();
        let before = commands(&s.fixture).len();
        let key = PrWriteTarget::for_source(
            &reviews.scope,
            &reviews.repository,
            &reviews.parent_node,
            reviews.number,
        )
        .unwrap();
        let mut journal = PrWriteJournal::open(&path).unwrap();
        let write = source.prepared_write().unwrap();
        let permit = journal
            .begin_review_operation_v3(&key, write, source.origin_seed().unwrap())
            .unwrap();
        let reference = permit.marker().publication_reference();
        assert!(journal
            .review_operation_original_v3(&key, &reference)
            .unwrap()
            .is_some());
        journal.claim(&permit, &key, write).unwrap();
        journal
            .mark_confirmed(
                &permit,
                &PrConfirmedEffect {
                    kind: PrEffectKind::Comment,
                    resource_id: "910".into(),
                    node_id: Some("ACTUAL_COMMENT_910".into()),
                },
            )
            .unwrap();
        // Simulate only the precise confirmed→terminal-fence crash GAP in SQL;
        // no process crash or actual GitHub mutation is claimed here.
        for category in [SyncBindingKind::Review, SyncBindingKind::InlineComment] {
            assert_eq!(
                journal
                    .latest_source_receipt(&key, category)
                    .unwrap()
                    .unwrap()
                    .publication_reference(),
                reference
            );
        }
        drop(journal);
        let targets = if reverse {
            vec![inline.clone(), reviews.clone()]
        } else {
            vec![reviews.clone(), inline.clone()]
        };
        let guard =
            crate::pr_projection::guard_discussion_reads(&path, &reviews.scope, &targets).unwrap();
        assert_eq!(guard.recovered_sources().len(), 2);
        assert!(guard.blocked_sources().is_empty());
        assert_eq!(visible(&path, &reviews), None);
        assert_eq!(visible(&path, &inline), None);
        assert_eq!(visible(&path, &neighbour), Some(kept));
        let fresh_review = hydrate(&path, &reviews);
        let fresh_inline = hydrate(&path, &inline);
        assert_eq!(
            fresh_review, 3,
            "reserve1 -> exact once fence2 -> fresh hydration3"
        );
        assert_eq!(
            fresh_inline, 3,
            "both actual categories advance by the same explicit sequence"
        );
        crate::pr_projection::guard_discussion_reads(&path, &reviews.scope, &targets).unwrap();
        assert_eq!(visible(&path, &reviews), Some(fresh_review));
        assert_eq!(visible(&path, &inline), Some(fresh_inline));
        assert_eq!(commands(&s.fixture).len(), before);
        // Only the TWO exact intended checkpoint generations change1->3;
        // retained body rows, unrelated source, drafts and cache stay EXACT.
        // Build this independent expected value from pre-mutation rows, never
        // reading the post-operation SQL as an accepted baseline.
        let sources = [reviews.source(), inline.source()];
        let mut changed = 0;
        s.sql_before = original_sql
            .into_iter()
            .map(|row| {
                let mut value: Value = serde_json::from_str(&row).unwrap();
                if value.as_array().is_some_and(|fields| fields.len() == 5)
                    && value[2]
                        .as_str()
                        .is_some_and(|source| sources.iter().any(|expected| expected == source))
                    && value[3].as_u64() == Some(1)
                    && value[4].as_u64() == Some(1)
                {
                    value[3] = json!(3);
                    changed += 1;
                    serde_json::to_string(&value).unwrap()
                } else {
                    row
                }
            })
            .collect();
        assert_eq!(
            changed, 2,
            "only actual Review and Inline checkpoints are expected effects"
        );
        unchanged_git_and_exact_script(&s);
    }
}

#[test]
fn pending_reviews_v3_protected_history_exact_database_numeric_target_ref_generation_and_origin_never_restores_mutator(
) {
    let (s, _app, intent) = operation_fixture(1);
    let source = operation_source(&s, intent);
    let path = database(&s.fixture);
    let parent = source.intent().original().parent();
    let key = PrWriteTarget::new(source.identity(), parent.summary()).unwrap();
    let mut journal = PrWriteJournal::open(&path).unwrap();
    let permit = journal
        .begin_review_operation_v3(
            &key,
            source.prepared_write().unwrap(),
            source.origin_seed().unwrap(),
        )
        .unwrap();
    let reference = permit.marker().publication_reference();
    journal
        .claim(&permit, &key, source.prepared_write().unwrap())
        .unwrap();
    journal.mark_uncertain(&permit).unwrap();
    let history = journal
        .review_operation_history_v3(
            &key,
            parent.repository_id(),
            parent.repository_node(),
            parent.database_id(),
        )
        .unwrap();
    assert_eq!(history.rows.len(), 1);
    assert_eq!(history.rows[0].marker.publication_reference(), reference);
    assert_eq!(
        history.rows[0].marker.state,
        crate::pr_write_journal::PrWriteState::Uncertain
    );
    let Some(crate::pr_write_journal::review_action_lineage_v12::ReviewObservedOriginV12::Legacy(
        origin,
    )) = history.rows[0].origin.as_ref()
    else {
        panic!("Original V3 publication must retain its actual legacy origin variant");
    };
    assert_eq!(origin.seed.metadata().draft_key(), source.key());
    journal
        .validate_review_operation_history_v3(
            &key,
            parent.repository_id(),
            parent.repository_node(),
            parent.database_id(),
            &history,
        )
        .unwrap();
    let foreign = PrWriteTarget::for_source(
        &Scope::github(HOST, Some(43)).unwrap(),
        parent.repository(),
        parent.node_id(),
        parent.number(),
    )
    .unwrap();
    assert!(journal
        .review_operation_original_v3(&foreign, &reference)
        .unwrap()
        .is_none());
    assert!(journal
        .review_operation_history_v3(
            &key,
            parent.repository_id() + 1,
            parent.repository_node(),
            parent.database_id()
        )
        .is_err());
    assert!(
        journal
            .begin_review_operation_v3(
                &key,
                source.prepared_write().unwrap(),
                source.origin_seed().unwrap()
            )
            .is_err(),
        "uncertain history never authorizes resend"
    );
    unchanged_git_and_exact_script(&s);
}

fn actual_preview_work(
    s: &Setup,
    app: &mut App,
    intent: Arc<ScopedReviewOperationIntentV3>,
) -> Arc<images::reviews_v2::native_v2::ScopedReviewsWorkV2> {
    let source = operation_source(s, intent);
    app.form = Some(
        operations::ui_v3::form_from_source(
            source,
            &Default::default(),
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap(),
    );
    app.overlay = Some(crate::app::Overlay::Form);
    app.form_dismissed = false;
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| crate::forms::draw(frame, app, frame.area()))
        .unwrap();
    app.dispatch(crate::app::Action::Form(crate::forms::FormAction::Preview));
    images::reviews_v2::native_v2::capture_form(app);
    app.scoped_reviews_work_v2
        .take()
        .expect("actual useful original operation Preview intent")
}

#[test]
fn pending_reviews_v3_actual_background_operation_preview_marks_form_busy_and_accepts_only_original_request_result(
) {
    let (s, mut app, intent) = operation_fixture(2);
    let work = actual_preview_work(&s, &mut app, intent);
    let writer = crate::drafts::DraftWriter::new(database(&s.fixture));
    let mut bg = crate::runtime::Background::new();
    assert!(bg.submit_scoped_reviews_v2(
        &mut app,
        work,
        provider(&s.fixture),
        database(&s.fixture),
        writer.client()
    ));
    assert!(
        app.form.as_ref().unwrap().submitting,
        "new operation purpose must share real busy/cancel semantics, not editable in-flight text"
    );
    let until = Instant::now() + Duration::from_secs(10);
    while !bg.poll(&mut app) {
        assert!(Instant::now() < until);
        std::thread::yield_now();
    }
    let form = app.form.as_ref().unwrap();
    assert!(!form.submitting);
    assert!(form.preview);
    assert!(form.publication.is_none());
    assert!(matches!(
        form.purpose,
        crate::forms::FormPurpose::PrReviewOperationV3(_)
    ));
    assert!(app.scoped_reviews_owner_v2.is_none());
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| crate::forms::draw(frame, &mut app, frame.area()))
        .unwrap();
    assert!(app.form.as_ref().unwrap().plan_ready());
    let before = commands(&s.fixture).len();
    let sql = sql_state(&s.fixture);
    app.handle_event(crossterm::event::Event::Resize(40, 12));
    app.dispatch(crate::app::Action::Form(crate::forms::FormAction::Submit));
    assert!(!app.form.as_ref().unwrap().submit_requested);
    assert!(!app.form.as_ref().unwrap().plan_ready());
    assert!(
        app.form.as_ref().unwrap().preview,
        "resize restricts visibility, does not replace fresh source/Preview"
    );
    assert_eq!(commands(&s.fixture).len(), before);
    assert_eq!(sql_state(&s.fixture), sql);
    unchanged_git_and_exact_script(&s);
}

#[test]
fn pending_reviews_v3_actual_background_operation_mixed_success_or_caught_panic_cannot_touch_genuine_independently_newer_source(
) {
    use crate::runtime::scoped_reviews_test_support_v2 as channel;
    for panic in [false, true] {
        let (s, mut app, intent) = operation_fixture(if panic { 1 } else { 2 });
        let original = actual_preview_work(&s, &mut app, intent);
        let work = if panic {
            images::reviews_v2::native_v2::actual_worker_panic_for_test(original)
        } else {
            original
        };
        let writer = crate::drafts::DraftWriter::new(database(&s.fixture));
        let mut bg = crate::runtime::Background::new();
        assert!(bg.submit_scoped_reviews_v2(
            &mut app,
            work,
            provider(&s.fixture),
            database(&s.fixture),
            writer.client()
        ));
        channel::fault_next(&mut bg, if panic { 2 } else { 0 });
        let (next, next_app, _) = operation_fixture(0);
        let genuine = next_app.remote_scope_source.as_ref().unwrap().clone();
        // Preserve the ACTUAL original App owner for later owned cleanup;
        // never manufacture an old owner in the genuine successor App.
        let original_app = std::mem::replace(&mut app, next_app);
        app.status = "GENUINE NEWER SOURCE/status unaffected by old operation".into();
        let generation = channel::generation(&bg);
        let sql = sql_state(&next.fixture);
        let newer_nav = app
            .github
            .as_ref()
            .unwrap()
            .scoped_pr_navigation
            .as_ref()
            .unwrap()
            .clone();
        let newer_resource = app
            .github
            .as_ref()
            .unwrap()
            .scoped_pr_resource
            .as_ref()
            .unwrap()
            .clone();
        let newer_loading = app.loading;
        assert!(!bg.poll(&mut app));
        assert!(Arc::ptr_eq(
            app.remote_scope_source.as_ref().unwrap(),
            &genuine
        ));
        assert_eq!(
            app.status,
            "GENUINE NEWER SOURCE/status unaffected by old operation"
        );
        assert_eq!(channel::generation(&bg), generation);
        assert_eq!(sql_state(&next.fixture), sql);
        assert!(
            !bg.cancel_scoped_reviews_v2(&mut app),
            "successor has no old App owner: cancellation must refuse"
        );
        assert_eq!(channel::generation(&bg), generation);
        assert_eq!(app.loading, newer_loading);
        assert_eq!(
            app.status,
            "GENUINE NEWER SOURCE/status unaffected by old operation"
        );
        assert!(Arc::ptr_eq(
            app.remote_scope_source.as_ref().unwrap(),
            &genuine
        ));
        assert!(Arc::ptr_eq(
            app.github
                .as_ref()
                .unwrap()
                .scoped_pr_navigation
                .as_ref()
                .unwrap(),
            &newer_nav
        ));
        assert!(Arc::ptr_eq(
            app.github
                .as_ref()
                .unwrap()
                .scoped_pr_resource
                .as_ref()
                .unwrap(),
            &newer_resource
        ));
        let successor = std::mem::replace(&mut app, original_app);
        assert!(
            bg.cancel_scoped_reviews_v2(&mut app),
            "only original actual App may cancel/reap its owned worker"
        );
        app = successor;
        assert_eq!(channel::generation(&bg), None);
        assert_eq!(app.loading, newer_loading);
        assert_eq!(
            app.status,
            "GENUINE NEWER SOURCE/status unaffected by old operation"
        );
        assert!(Arc::ptr_eq(
            app.remote_scope_source.as_ref().unwrap(),
            &genuine
        ));
        assert!(Arc::ptr_eq(
            app.github
                .as_ref()
                .unwrap()
                .scoped_pr_navigation
                .as_ref()
                .unwrap(),
            &newer_nav
        ));
        assert!(Arc::ptr_eq(
            app.github
                .as_ref()
                .unwrap()
                .scoped_pr_resource
                .as_ref()
                .unwrap(),
            &newer_resource
        ));
        assert_eq!(sql_state(&next.fixture), sql);
        unchanged_git_and_exact_script(&s);
        unchanged_git_and_exact_script(&next);
    }
}
