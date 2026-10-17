//! SOURCE ONLY / UNRUN: exact genuine source/Form/request cancellation fences.
//! Independently newer instances are produced by the real read/Form factories;
//! retained old owners model a delayed physical completion, not a fabricated DTO.
use super::*;
use crate::{
    forms::FormAction, scoped_actions_publication_test_support_v3 as w,
    scoped_actions_test_support_v1 as f,
};

fn prepared(
    next: Vec<crate::test_support::GhStep>,
) -> (
    crate::test_support::Fixture,
    crate::providers::gh::GithubProvider,
    App,
    Vec<crate::test_support::GhStep>,
) {
    let mut steps = w::read_phase(w::running(), 42);
    steps.extend(w::read_phase(w::running(), 42));
    steps.extend(next);
    let (fixture, provider, mut app, all) = w::setup(steps);
    w::open(&mut app, &provider, &fixture);
    w::preview(&mut app, &provider, &fixture);
    (fixture, provider, app, all)
}

fn old_confirm(
    app: &mut App,
    fixture: &crate::test_support::Fixture,
) -> (Arc<ScopedActionsOwnerV1>, crate::drafts::DraftWriter) {
    let work = w::request(app, FormAction::Submit);
    let drafts = crate::drafts::DraftWriter::new(f::database(fixture));
    let owner =
        ScopedActionsOwnerV1::new_publication_v3(work, &f::database(fixture), &drafts.client())
            .unwrap();
    assert!(owner
        .work
        .publication_v3
        .as_ref()
        .unwrap()
        .confirm()
        .is_some());
    app.scoped_actions_owner_v1 = Some(owner.clone());
    (owner, drafts)
}

fn preview_work(app: &mut App) -> Arc<ScopedActionsWorkV1> {
    if app.form.as_ref().unwrap().preview {
        // The shared Preview control first returns from the displayed Preview
        // to the intent. Observe that genuine paint before requesting another
        // Preview; a local Back is not a queued provider operation.
        app.form.as_mut().unwrap().dispatch(&FormAction::Preview);
        assert!(!app.form.as_ref().unwrap().preview);
        assert!(!app.form.as_ref().unwrap().preview_requested);
        w::paint(app, 120, 40);
    }
    w::request(app, FormAction::Preview)
}

fn newer_request(app: &mut App) -> Arc<ScopedActionsWorkV1> {
    let work = preview_work(app);
    assert!(app.form.as_ref().unwrap().submitting);
    assert!(work
        .publication_v3
        .as_ref()
        .unwrap()
        .request()
        .unwrap()
        .matches_form(app.form.as_ref().unwrap()));
    app.scoped_actions_work_v1 = Some(work.clone());
    work
}

/// No event/body parse/SQL/gh occurs in this witness. The NEW request remains
/// applicable to its genuine Form while the independently old owner stays live.
fn assert_newer_untouched(
    app: &App,
    owner: &Arc<ScopedActionsOwnerV1>,
    newer: &Arc<ScopedActionsWorkV1>,
    status: &str,
    hits: &str,
    loading: bool,
    epoch: u64,
) {
    assert!(Arc::ptr_eq(
        app.scoped_actions_owner_v1.as_ref().unwrap(),
        owner
    ));
    assert!(Arc::ptr_eq(
        app.scoped_actions_work_v1.as_ref().unwrap(),
        newer
    ));
    assert!(!owner.cancellation().is_cancelled());
    assert!(!newer.cancellation().is_cancelled());
    assert!(!app.scoped_actions_cancel_v1);
    assert_eq!(app.overlay, Some(crate::app::Overlay::Form));
    assert_eq!(app.status, status);
    assert_eq!(format!("{:?}", app.hits), hits);
    assert_eq!(app.loading[View::Actions.index()], loading);
    let state = app.scoped_actions_v1.as_ref().unwrap();
    assert_eq!(state.epoch, epoch);
    assert!(Arc::ptr_eq(&state.source, &newer.source));
    assert!(Arc::ptr_eq(&state.page, &newer.page));
    let form = app.form.as_ref().unwrap();
    assert!(form.submitting);
    assert!(newer
        .publication_v3
        .as_ref()
        .unwrap()
        .request()
        .unwrap()
        .matches_form(form));
}

#[test]
fn scoped_actions_publication_v3_current_confirm_close_cancels_only_exact_genuine_request() {
    let (fixture, _provider, mut app, all) = prepared(vec![]);
    let sql = f::sql_state(&fixture);
    let git = f::git_state(&fixture);
    let (owner, _drafts) = old_confirm(&mut app, &fixture);
    assert_eq!(applicable_cancel_v3(&app), Some(true));
    app.dispatch(Action::Form(FormAction::Close));
    assert!(owner.cancellation().is_cancelled());
    assert!(app.form.is_none());
    assert!(app.scoped_actions_v1.is_none());
    assert_eq!(app.overlay, None);
    assert!(app.status.contains("logically canceled"));
    assert!(
        w::result_sql(&fixture).is_empty(),
        "queued only: no invocation/marker"
    );
    assert_eq!(f::sql_state(&fixture), sql);
    assert_eq!(f::git_state(&fixture), git);
    w::exact(&fixture, &all);
}

#[test]
fn scoped_actions_publication_v3_current_preview_resize_keeps_exact_original_presentation() {
    let (fixture, _provider, mut app, all) = prepared(vec![]);
    let sql = f::sql_state(&fixture);
    let git = f::git_state(&fixture);
    let work = preview_work(&mut app);
    let page = app.scoped_actions_v1.as_ref().unwrap().page.clone();
    let epoch = app.scoped_actions_v1.as_ref().unwrap().epoch;
    let owner = ScopedActionsOwnerV1::new(work.clone()).unwrap();
    app.scoped_actions_owner_v1 = Some(owner);
    assert_eq!(applicable_cancel_v3(&app), Some(false));
    super::super::resize(&mut app);
    assert!(work.cancellation().is_cancelled());
    assert!(app.form.is_some());
    assert_eq!(app.overlay, Some(crate::app::Overlay::Form));
    let state = app.scoped_actions_v1.as_ref().unwrap();
    assert!(Arc::ptr_eq(&page, &state.page));
    assert_eq!(state.epoch, epoch.wrapping_add(1));
    assert!(!app.form.as_ref().unwrap().submitting);
    assert_eq!(f::sql_state(&fixture), sql);
    assert_eq!(f::git_state(&fixture), git);
    w::exact(&fixture, &all);
}

#[test]
fn scoped_actions_publication_v3_initial_open_resize_retires_only_current_original_input() {
    let (fixture, _provider, mut app, all) = w::setup(vec![]);
    let sql = f::sql_state(&fixture);
    let git = f::git_state(&fixture);
    let work = w::open_work(&mut app);
    let page = app.scoped_actions_v1.as_ref().unwrap().page.clone();
    let epoch = app.scoped_actions_v1.as_ref().unwrap().epoch;
    let owner = ScopedActionsOwnerV1::new(work.clone()).unwrap();
    app.scoped_actions_owner_v1 = Some(owner);
    assert!(current_retirement_work_v3(&app).is_some());
    assert!(app.overlay.is_none());
    super::super::resize(&mut app);
    assert!(work.cancellation().is_cancelled());
    assert!(app.form.is_none());
    assert!(app.overlay.is_none());
    let state = app.scoped_actions_v1.as_ref().unwrap();
    assert!(Arc::ptr_eq(&page, &state.page));
    assert_eq!(state.epoch, epoch.wrapping_add(1));
    assert_eq!(f::sql_state(&fixture), sql);
    assert_eq!(f::git_state(&fixture), git);
    w::exact(&fixture, &all);
}

#[test]
fn scoped_actions_publication_v3_old_confirm_cannot_close_or_resize_genuine_new_form_ticket() {
    let (fixture, provider, mut app, all) = prepared(w::read_phase(w::running(), 42));
    let (old, _drafts) = old_confirm(&mut app, &fixture);
    let old_page = old.work.page.clone();
    // Fresh Open uses the actual original source worker and shared Form factory.
    // Restore the delayed OLD owner only after the NEW source-bound work exists.
    app.scoped_actions_owner_v1 = None;
    app.form = None;
    app.overlay = None;
    w::open(&mut app, &provider, &fixture);
    let newer = newer_request(&mut app);
    assert!(Arc::ptr_eq(&old_page, &newer.page));
    assert!(!old
        .work
        .publication_v3
        .as_ref()
        .unwrap()
        .request()
        .unwrap()
        .matches_form(app.form.as_ref().unwrap()));
    app.scoped_actions_owner_v1 = Some(old.clone());
    let sql = f::sql_state(&fixture);
    let git = f::git_state(&fixture);
    let status = app.status.clone();
    let hits = format!("{:?}", app.hits);
    let loading = app.loading[View::Actions.index()];
    let epoch = app.scoped_actions_v1.as_ref().unwrap().epoch;
    assert_eq!(applicable_cancel_v3(&app), None);
    super::super::resize(&mut app);
    assert_newer_untouched(&app, &old, &newer, &status, &hits, loading, epoch);
    app.dispatch(Action::Form(FormAction::Close));
    assert_newer_untouched(&app, &old, &newer, &status, &hits, loading, epoch);
    assert_eq!(f::sql_state(&fixture), sql);
    assert_eq!(f::git_state(&fixture), git);
    w::exact(&fixture, &all);
}

#[test]
fn scoped_actions_publication_v3_old_confirm_cannot_retire_independently_new_genuine_source_form() {
    let (old_fixture, _provider, mut old_app, old_all) = prepared(vec![]);
    let (old, _drafts) = old_confirm(&mut old_app, &old_fixture);
    let (fixture, _provider, mut app, all) = prepared(vec![]);
    let newer = newer_request(&mut app);
    assert!(!Arc::ptr_eq(&old.work.source, &newer.source));
    assert!(!Arc::ptr_eq(&old.work.page, &newer.page));
    app.scoped_actions_owner_v1 = Some(old.clone());
    let sql = f::sql_state(&fixture);
    let old_sql = f::sql_state(&old_fixture);
    let git = f::git_state(&fixture);
    let old_git = f::git_state(&old_fixture);
    let status = app.status.clone();
    let hits = format!("{:?}", app.hits);
    let loading = app.loading[View::Actions.index()];
    let epoch = app.scoped_actions_v1.as_ref().unwrap().epoch;
    assert_eq!(applicable_cancel_v3(&app), None);
    super::super::resize(&mut app);
    assert_newer_untouched(&app, &old, &newer, &status, &hits, loading, epoch);
    app.dispatch(Action::Form(FormAction::Close));
    assert_newer_untouched(&app, &old, &newer, &status, &hits, loading, epoch);
    assert_eq!(f::sql_state(&fixture), sql);
    assert_eq!(f::sql_state(&old_fixture), old_sql);
    assert_eq!(f::git_state(&fixture), git);
    assert_eq!(f::git_state(&old_fixture), old_git);
    w::exact(&fixture, &all);
    w::exact(&old_fixture, &old_all);
}

#[test]
fn scoped_actions_publication_v3_newer_repository_intent_prevents_old_confirm_cancel_effects() {
    let (fixture, _provider, mut app, all) = prepared(vec![]);
    let (old, _drafts) = old_confirm(&mut app, &fixture);
    let sql = f::sql_state(&fixture);
    let git = f::git_state(&fixture);
    let page = app.scoped_actions_v1.as_ref().unwrap().page.clone();
    let epoch = app.scoped_actions_v1.as_ref().unwrap().epoch;
    let status = app.status.clone();
    let hits = format!("{:?}", app.hits);
    app.repository_load_requested = true;
    assert_eq!(applicable_cancel_v3(&app), None);
    super::super::resize(&mut app);
    app.dispatch(Action::Form(FormAction::Close));
    assert!(Arc::ptr_eq(
        app.scoped_actions_owner_v1.as_ref().unwrap(),
        &old
    ));
    assert!(!old.cancellation().is_cancelled());
    assert!(!app.scoped_actions_cancel_v1);
    assert!(app.form.as_ref().unwrap().submitting);
    assert_eq!(app.status, status);
    assert_eq!(format!("{:?}", app.hits), hits);
    assert_eq!(app.scoped_actions_v1.as_ref().unwrap().epoch, epoch);
    assert!(Arc::ptr_eq(
        &app.scoped_actions_v1.as_ref().unwrap().page,
        &page
    ));
    assert_eq!(f::sql_state(&fixture), sql);
    assert_eq!(f::git_state(&fixture), git);
    w::exact(&fixture, &all);
}
