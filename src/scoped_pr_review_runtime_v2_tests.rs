//! Genuine closed producer and real Background work/completion queue. These
//! exercise current/stale success, error/panic and mixed facets before effects.
use super::*;
use crate::{
    app::{Overlay, View},
    drafts::DraftWriter,
    runtime::{scoped_reviews_test_support_v2 as channel, Background},
};
use images::reviews_v2::native_v2 as native;
#[path = "scoped_pr_review_lifecycle_v6_tests.rs"]
mod lifecycle_v6;
fn queue(s: &mut Setup) -> (App, Background, DraftWriter) {
    let (mut app, _) = owner(s);
    native::open(&mut app, false);
    let work = app.scoped_reviews_work_v2.take().unwrap();
    let writer = DraftWriter::new(database(&s.fixture));
    let mut bg = Background::new();
    assert!(bg.submit_scoped_reviews_v2(
        &mut app,
        work,
        provider(&s.fixture),
        database(&s.fixture),
        writer.client()
    ));
    (app, bg, writer)
}
fn complete(bg: &mut Background, app: &mut App) {
    let end = Instant::now() + Duration::from_secs(10);
    while !bg.poll(app) {
        assert!(
            Instant::now() < end,
            "actual original review work never completed"
        );
        std::thread::yield_now();
    }
}
#[test]
fn pending_reviews_v2_runtime_actual_queue_closed_browser_only_exact_original_owner_and_route() {
    let mut s = inline_setup(|h, steps| steps.extend(open_steps(h, json!([]))));
    let (mut app, mut bg, _writer) = queue(&mut s);
    let owner = app.scoped_reviews_owner_v2.clone().unwrap();
    assert!(!owner.expired());
    assert!(owner.deadline().duration_since(Instant::now()) <= Duration::from_secs(30));
    complete(&mut bg, &mut app);
    assert_eq!(app.overlay, Some(Overlay::ScopedReviewsV2));
    assert!(app.scoped_reviews_v2.as_ref().unwrap().owns(&app));
    assert!(app.scoped_reviews_owner_v2.is_none());
    assert!(channel::generation(&bg).is_none());
    assert!(!app.loading[View::Inbox.index()]);
    unchanged_git_and_exact_script(&s);
}

#[test]
fn pending_reviews_v2_retired_actual_browser_does_not_swallow_first_quit_or_close_unrelated_help() {
    use crossterm::event::{
        Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };
    for mode in 0..3 {
        let mut s = inline_setup(|h, steps| steps.extend(open_steps(h, json!([]))));
        let (mut app, mut bg, _writer) = queue(&mut s);
        complete(&mut bg, &mut app);
        assert_eq!(app.overlay, Some(Overlay::ScopedReviewsV2));
        assert!(app.scoped_reviews_v2.as_ref().unwrap().owns(&app));
        let before = commands(&s.fixture).len();
        let sql_before = sql_state(&s.fixture);
        if mode == 2 {
            app.overlay = Some(Overlay::Help);
        }
        native::retire_private(&mut app, &mut bg);
        assert!(app.scoped_reviews_v2.is_none());
        assert!(app.pr_file_images.is_none());
        assert!(app.github.is_none());
        assert!(app.hits.is_empty());
        if mode == 2 {
            assert_eq!(app.overlay, Some(Overlay::Help));
            assert!(!app.quit);
        } else {
            assert!(
                app.overlay.is_none(),
                "retired model must not own the next event"
            );
            if mode == 0 {
                app.handle_event(Event::Key(KeyEvent::new(
                    KeyCode::Char('q'),
                    KeyModifiers::NONE,
                )));
            } else {
                let mut terminal =
                    ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
                terminal
                    .draw(|frame| crate::ui::draw(frame, &mut app))
                    .unwrap();
                let quit = app
                    .hits
                    .iter()
                    .find(|hit| hit.action == crate::app::Action::Quit)
                    .expect("genuine visible Quit hit after private retirement")
                    .rect;
                app.handle_event(Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: quit.x,
                    row: quit.y,
                    modifiers: KeyModifiers::NONE,
                }));
            }
            assert!(
                app.quit,
                "one genuine Quit must exit, not dismiss an empty modal"
            );
        }
        assert_eq!(commands(&s.fixture).len(), before);
        assert_eq!(sql_state(&s.fixture), sql_before);
        unchanged_git_and_exact_script(&s);
    }
}
#[test]
fn pending_reviews_v2_runtime_mixed_or_missing_original_owner_never_removes_slot_or_applies_privacy_status(
) {
    for fault in [0, 1, 4] {
        let mut s = inline_setup(|h, steps| steps.extend(open_steps(h, json!([]))));
        let (mut app, mut bg, _writer) = queue(&mut s);
        let generation = channel::generation(&bg);
        let owner = app.scoped_reviews_owner_v2.clone().unwrap();
        channel::fault_next(&mut bg, fault);
        app.status = "CURRENT ORIGINAL REVIEW STATUS".into();
        assert!(!bg.poll(&mut app));
        assert_eq!(channel::generation(&bg), generation);
        assert!(Arc::ptr_eq(
            app.scoped_reviews_owner_v2.as_ref().unwrap(),
            &owner
        ));
        assert!(app.loading[View::Inbox.index()]);
        assert_eq!(app.status, "CURRENT ORIGINAL REVIEW STATUS");
        assert!(app.github.is_some());
        assert!(app.scoped_reviews_v2.is_none());
        assert!(bg.cancel_scoped_reviews_v2(&mut app));
        assert!(owner.cancellation().is_cancelled());
        unchanged_git_and_exact_script(&s);
    }
}
#[test]
fn pending_reviews_v2_runtime_newer_intent_or_lost_source_vetoes_actual_success_before_any_private_effect(
) {
    for source_lost in [false, true] {
        let mut s = inline_setup(|h, steps| steps.extend(open_steps(h, json!([]))));
        let (mut app, mut bg, _writer) = queue(&mut s);
        let generation = channel::generation(&bg);
        channel::fault_next(&mut bg, 3);
        if source_lost {
            app.remote_scope_source = None;
        } else {
            app.scoped_reviews_cancel_v2 = true;
        }
        app.status = "UNCHANGED NEWER INTENT STATUS".into();
        assert!(!bg.poll(&mut app));
        assert_eq!(channel::generation(&bg), generation);
        assert!(app.scoped_reviews_v2.is_none());
        assert_eq!(app.status, "UNCHANGED NEWER INTENT STATUS");
        app.scoped_reviews_cancel_v2 = false;
        assert!(bg.cancel_scoped_reviews_v2(&mut app));
        unchanged_git_and_exact_script(&s);
    }
}
#[test]
fn pending_reviews_v2_runtime_actual_caught_panic_is_owned_before_failure_and_cannot_clear_newer_scope(
) {
    for current in [true, false] {
        let mut s = inline_setup(|_, _| {});
        let (mut app, _) = owner(&mut s);
        native::open(&mut app, false);
        let work = native::actual_worker_panic_for_test(app.scoped_reviews_work_v2.take().unwrap());
        let mut bg = Background::new();
        let writer = DraftWriter::new(database(&s.fixture));
        assert!(bg.submit_scoped_reviews_v2(
            &mut app,
            work,
            provider(&s.fixture),
            database(&s.fixture),
            writer.client()
        ));
        channel::fault_next(&mut bg, 2);
        if !current {
            app.remote_scope_source = None;
            app.status = "GENUINE NEWER SCOPE STATUS".into();
        }
        if current {
            assert!(bg.poll(&mut app));
            assert!(app.github.is_none());
            assert!(!app.status.contains("PRIVATE_POISON"));
            assert!(channel::generation(&bg).is_none());
        } else {
            assert!(!bg.poll(&mut app));
            assert_eq!(app.status, "GENUINE NEWER SCOPE STATUS");
            assert!(channel::generation(&bg).is_some());
            assert!(bg.cancel_scoped_reviews_v2(&mut app));
        }
        unchanged_git_and_exact_script(&s);
    }
}

#[test]
fn pending_reviews_v2_idle_original_numeric_account_loss_hides_browser_form_images_and_consumes_event(
) {
    for form_surface in [false, true] {
        let mut s = inline_setup(|h, steps| steps.extend(open_steps(h, json!([]))));
        let (mut app, _) = owner(&mut s);
        let intent = ScopedReviewIntentV2::capture(&app).unwrap();
        let draft = open(&s, intent).unwrap();
        if form_surface {
            app.form = Some(
                images::reviews_v2::ui_v2::form_from_draft(
                    draft,
                    None,
                    images::reviews_v2::ReviewRequestedActionV2::CreatePending,
                    None,
                    &Default::default(),
                    Instant::now() + Duration::from_secs(30),
                )
                .unwrap(),
            );
            app.overlay = Some(Overlay::Form);
            app.form_dismissed = false;
        } else {
            app.scoped_reviews_v2 = Some(
                images::reviews_v2::browser_v2::ReviewBrowserV2::prepare(
                    draft,
                    &Default::default(),
                    Instant::now() + Duration::from_secs(30),
                )
                .unwrap(),
            );
            app.overlay = Some(Overlay::ScopedReviewsV2);
        }
        app.workspace = crate::workspace::Workspace::load(
            &s.fixture.repo(),
            crate::workspace::RepositoryMode::Graph,
            "HEAD",
            None,
        )
        .unwrap();
        assert!(app.pr_file_images.is_some());
        assert!(app.workspace.is_some());
        let local = app
            .workspace
            .as_ref()
            .map(|workspace| workspace.directory.clone());
        let before = commands(&s.fixture).len();
        app.github.as_mut().unwrap().identity.account_id = Some(43); // same login is NOT original UID42
        assert!(native::retire_invalid(&mut app));
        assert!(app.github.is_none());
        assert!(app.pr_file_images.is_none());
        assert!(app.scoped_reviews_v2.is_none());
        assert!(app.form.is_none());
        assert!(app.avatar_states.is_empty());
        assert!(app.hits.is_empty());
        assert_eq!(
            app.workspace
                .as_ref()
                .map(|workspace| workspace.directory.clone()),
            local
        );
        assert_eq!(commands(&s.fixture).len(), before);
        unchanged_git_and_exact_script(&s);
    }
}

#[test]
fn pending_reviews_v2_idle_stale_browser_cannot_retire_independently_newer_genuine_source_or_owner()
{
    let mut old = inline_setup(|h, steps| steps.extend(open_steps(h, json!([]))));
    let (old_app, _) = owner(&mut old);
    let draft = open(&old, ScopedReviewIntentV2::capture(&old_app).unwrap()).unwrap();
    let stale = images::reviews_v2::browser_v2::ReviewBrowserV2::prepare(
        draft,
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let mut fresh = inline_setup(|_, _| {});
    let (mut app, _) = owner(&mut fresh);
    let source = app.remote_scope_source.clone().unwrap();
    let active = app.remote_scope.clone().unwrap();
    app.overlay = None;
    native::open(&mut app, false);
    let work = app.scoped_reviews_work_v2.take().unwrap();
    let writer = DraftWriter::new(database(&fresh.fixture));
    let current =
        native::ScopedReviewsOwnerV2::new(work, &database(&fresh.fixture), &writer.client())
            .unwrap();
    app.scoped_reviews_owner_v2 = Some(current.clone());
    app.scoped_reviews_v2 = Some(stale);
    app.overlay = Some(Overlay::ScopedReviewsV2);
    app.status = "INDEPENDENT NEW SOURCE STATUS".into();
    app.github.as_mut().unwrap().identity.account_id = Some(43);
    let old_count = commands(&old.fixture).len();
    let new_count = commands(&fresh.fixture).len();
    assert!(native::retire_invalid(&mut app));
    assert!(app.scoped_reviews_v2.is_none());
    assert!(app.github.is_some());
    assert!(app.pr_file_images.is_some());
    assert!(Arc::ptr_eq(
        app.remote_scope_source.as_ref().unwrap(),
        &source
    ));
    assert!(Arc::ptr_eq(app.remote_scope.as_ref().unwrap(), &active));
    assert!(Arc::ptr_eq(
        app.scoped_reviews_owner_v2.as_ref().unwrap(),
        &current
    ));
    assert!(!current.cancellation().is_cancelled());
    assert!(!app.scoped_reviews_cancel_v2);
    assert_eq!(app.status, "INDEPENDENT NEW SOURCE STATUS");
    assert_eq!(commands(&old.fixture).len(), old_count);
    assert_eq!(commands(&fresh.fixture).len(), new_count);
    unchanged_git_and_exact_script(&old);
    unchanged_git_and_exact_script(&fresh);
}
