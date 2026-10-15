//! V9-relative corrective declarations. Frozen49 remains immutable. No build
//! was run; Root can first execute the actual closed-result regression RED.
use super::*;
use images::{reviews_v2::native_v2 as native, ui::PrImageUiActionV1};

#[test]
fn pending_reviews_v3_actual_anchored_existing_pending_catalogue_publishes_browser_not_duplicate_create_form(
) {
    let mut s = inline_setup(|h, steps| {
        steps.extend(open_steps(
            h,
            json!([review(h, "PENDING", "pending summary", 42)]),
        ))
    });
    let (mut app, proof) = owner(&mut s);
    let viewer = app.pr_file_images.as_mut().unwrap();
    viewer.apply(PrImageUiActionV1::Old);
    viewer.apply(PrImageUiActionV1::Move {
        delta: 1,
        extend: true,
    });
    viewer.apply(PrImageUiActionV1::Range);
    assert_eq!(viewer.canonical_range().unwrap().start(), 1);
    assert_eq!(viewer.canonical_range().unwrap().end(), 2);
    app.overlay = Some(crate::app::Overlay::PrFileImages);
    let source = app.remote_scope_source.as_ref().unwrap().clone();
    let sql_before = sql_state(&s.fixture);
    native::open(&mut app, true);
    let work = app
        .scoped_reviews_work_v2
        .take()
        .expect("actual source/range anchored Review work");
    assert!(work.matches(&app));
    let writer = crate::drafts::DraftWriter::new(database(&s.fixture));
    let mut bg = crate::runtime::Background::new();
    assert!(bg.submit_scoped_reviews_v2(
        &mut app,
        work,
        provider(&s.fixture),
        database(&s.fixture),
        writer.client()
    ));
    let until = Instant::now() + Duration::from_secs(10);
    while !bg.poll(&mut app) {
        assert!(Instant::now() < until);
        std::thread::yield_now();
    }
    assert!(
        app.scoped_reviews_v2
            .as_ref()
            .is_some_and(|browser| browser.owns(&app)),
        "same genuine retained anchor must admit actual existing-PENDING browser result"
    );
    assert!(
        app.form.is_none(),
        "existing own PENDING is not a duplicate-create composer"
    );
    assert_eq!(app.overlay, Some(crate::app::Overlay::ScopedReviewsV2));
    assert!(Arc::ptr_eq(
        app.remote_scope_source.as_ref().unwrap(),
        &source
    ));
    assert!(proof.owns(&app));
    assert_eq!(sql_state(&s.fixture), sql_before);
    unchanged_git_and_exact_script(&s);
}

#[test]
fn pending_reviews_v3_expired_or_cancelled_original_operation_cannot_cancel_newer_actual_inbox_job_before_enqueue(
) {
    for cancelled in [false, true] {
        let (s, mut app, intent) = operation_fixture(1);
        let mut work = actual_preview_work(&s, &mut app, intent);
        native::expire_original_work_for_test(&mut work, cancelled);
        assert!(work.expired());
        let writer = crate::drafts::DraftWriter::new(database(&s.fixture));
        let mut bg = crate::runtime::Background::new();
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        bg.submit(&mut app, crate::app::View::Inbox, move |token| {
            entered_tx.send(token.clone()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            Ok(crate::runtime::PreparedView::default())
        });
        let token = entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(!token.is_cancelled());
        let generation = crate::runtime::scoped_reviews_test_support_v2::generation(&bg);
        app.status = "ACTUAL NEWER generic Inbox job/status".into();
        let loading = app.loading;
        let before = commands(&s.fixture).len();
        let sql = sql_state(&s.fixture);
        let original = app.remote_scope_source.as_ref().unwrap().clone();
        assert!(!bg.submit_scoped_reviews_v2(
            &mut app,
            work,
            provider(&s.fixture),
            database(&s.fixture),
            writer.client()
        ));
        assert!(!token.is_cancelled());
        assert_eq!(
            crate::runtime::scoped_reviews_test_support_v2::generation(&bg),
            generation
        );
        assert_eq!(app.loading, loading);
        assert_eq!(app.status, "ACTUAL NEWER generic Inbox job/status");
        assert!(Arc::ptr_eq(
            app.remote_scope_source.as_ref().unwrap(),
            &original
        ));
        assert!(app.scoped_reviews_owner_v2.is_none());
        assert_eq!(commands(&s.fixture).len(), before);
        assert_eq!(sql_state(&s.fixture), sql);
        release_tx.send(()).unwrap();
        unchanged_git_and_exact_script(&s);
    }
}
