//! SOURCE ONLY / UNRUN: genuine producers, SAME durable writer and independent
//! installed Git/SQL/strict GH. These do not replace native mouse/key journeys.
use super::*;
use crate::{
    actions_write_journal::ActionsWriteJournal,
    forms::FormAction,
    scoped_actions_publication_test_support_v3 as w, scoped_actions_test_support_v1 as f,
    storage::{Cache, Scope},
};
use serde_json::Value;
use std::time::Instant;

fn phases() -> Vec<crate::test_support::GhStep> {
    let mut steps = w::read_phase(w::running(), 42);
    steps.extend(w::read_phase(w::running(), 42));
    steps
}
fn prepared(
    tail: Vec<crate::test_support::GhStep>,
) -> (
    crate::test_support::Fixture,
    crate::providers::gh::GithubProvider,
    App,
    Vec<crate::test_support::GhStep>,
) {
    let mut steps = phases();
    steps.extend(tail);
    let (fixture, provider, mut app, all) = w::setup(steps);
    w::open(&mut app, &provider, &fixture);
    w::preview(&mut app, &provider, &fixture);
    (fixture, provider, app, all)
}
fn confirm(
    app: &mut App,
    provider: &crate::providers::gh::GithubProvider,
    fixture: &crate::test_support::Fixture,
) -> (
    PreparedResultV3,
    Arc<ScopedActionsOwnerV1>,
    crate::drafts::DraftWriter,
) {
    let work = w::request(app, FormAction::Submit);
    assert!(work.publication_v3.as_ref().unwrap().confirm().is_some());
    let draft = crate::drafts::DraftWriter::new(f::database(fixture));
    let owner = ScopedActionsOwnerV1::new_publication_v3(
        work.clone(),
        &f::database(fixture),
        &draft.client(),
    )
    .unwrap();
    assert!(owner.cancellation().same_instance(work.cancellation()));
    assert_eq!(owner.deadline(), work.deadline());
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let value = execute(
        &owner,
        provider,
        &f::database(fixture),
        Some(&draft.client()),
    )
    .unwrap();
    (value, owner, draft)
}
fn original_descriptor(
    app: &App,
    provider: &crate::providers::gh::GithubProvider,
) -> crate::actions_workspace::ActionsPublicationDescriptor {
    let state = app.scoped_actions_v1.as_ref().unwrap();
    crate::actions_workspace::ActionsPlan::prepare(
        app.actions.as_ref().unwrap(),
        ActionsCommand::Cancel,
        &Default::default(),
        provider,
        &CancellationToken::default(),
    )
    .unwrap()
    .scoped_cancel_descriptor_v3(provider, &state.source, state.page.run().unwrap())
    .unwrap()
}
fn original_pending(
    app: &App,
    provider: &crate::providers::gh::GithubProvider,
    fixture: &crate::test_support::Fixture,
) -> crate::actions_write_journal::ActionsPublicationReference {
    let descriptor = original_descriptor(app, provider);
    let mut journal = ActionsWriteJournal::open(&f::database(fixture)).unwrap();
    journal
        .begin(&descriptor.target, &descriptor.intent)
        .unwrap()
        .marker()
        .publication_reference()
}
#[test]
fn scoped_actions_publication_v3_original_form_preview_are_actual_source_read_only_and_bounded() {
    let (fixture, provider, mut app, all) = w::setup(phases());
    let git = f::git_state(&fixture);
    let drafts = f::sql_state(&fixture);
    w::open(&mut app, &provider, &fixture);
    let before = app.scoped_actions_v1.as_ref().unwrap().page.clone();
    let opened = match &app.form.as_ref().unwrap().purpose {
        FormPurpose::ScopedActionsV3(state) => state.opened().clone(),
        _ => panic!("Actual specialized Form"),
    };
    assert!(Arc::ptr_eq(opened.intent().page(), &before));
    assert_eq!(opened.intent().original().run().id, 123);
    assert!(!app.form.as_ref().unwrap().plan_ready());
    assert!(w::result_sql(&fixture).is_empty());
    w::preview(&mut app, &provider, &fixture);
    let form = app.form.as_ref().unwrap();
    assert!(form.plan_lines().iter().any(|line| line.contains("Cancel")));
    assert!(form.plan_ready());
    assert_eq!(form.scope(), Scope::github(f::HOST, Some(42)).unwrap());
    assert!(form
        .resource()
        .contains("repository/10000/run/123/attempt/2/head/"));
    assert!(
        form.preview_display_v1
            .document
            .as_ref()
            .unwrap()
            .retained_bytes()
            <= 16 * 1024 * 1024
    );
    assert!(w::result_sql(&fixture).is_empty());
    assert_eq!(f::sql_state(&fixture), drafts);
    assert_eq!(f::git_state(&fixture), git);
    w::exact(&fixture, &all);
}
#[test]
fn scoped_actions_publication_v3_hidden_geometry_never_grants_confirm_and_resize_keeps_preview_without_renewal(
) {
    let (fixture, _provider, mut app, all) = prepared(vec![]);
    let sql = f::sql_state(&fixture);
    let git = f::git_state(&fixture);
    let before = match &app.form.as_ref().unwrap().purpose {
        FormPurpose::ScopedActionsV3(state) => state.prepared_preview().cloned(),
        _ => panic!("Actual Form"),
    };
    app.form
        .as_mut()
        .unwrap()
        .clear_preview_display_geometry_v1();
    let tiny = w::paint(&mut app, 40, 10);
    assert!(tiny.to_ascii_lowercase().contains("resize"));
    assert!(
        tiny.contains("Actions disabled"),
        "Tiny guidance must be legible, not clipped"
    );
    assert!(!app
        .hits
        .iter()
        .any(|hit| hit.action == Action::Form(FormAction::Submit)));
    app.form.as_mut().unwrap().dispatch(&FormAction::Submit);
    assert!(!capture_form_work(&mut app));
    assert!(app.scoped_actions_work_v1.is_none());
    assert!(app.form.as_ref().unwrap().error.contains("Preview"));
    w::paint(&mut app, 120, 40);
    let FormPurpose::ScopedActionsV3(state) = &app.form.as_ref().unwrap().purpose else {
        panic!("Actual Form")
    };
    assert!(Arc::ptr_eq(
        state.prepared_preview().unwrap(),
        before.as_ref().unwrap()
    ));
    assert!(state.ready());
    assert!(w::result_sql(&fixture).is_empty());
    assert_eq!(f::sql_state(&fixture), sql);
    assert_eq!(f::git_state(&fixture), git);
    w::exact(&fixture, &all);
}
#[test]
fn scoped_actions_publication_v3_same_confirm_records_real_cancelled_receipt_once_and_only_removes_exact_draft(
) {
    let (fixture, provider, mut app, all) = prepared(w::confirm_phase(w::cancelled(), false, 42));
    let git = f::git_state(&fixture);
    let (result, owner, _draft) = confirm(&mut app, &provider, &fixture);
    let rows = w::result_sql(&fixture);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "confirmed");
    assert!(rows[0].1);
    let receipt: Value = serde_json::from_str(rows[0].2.as_ref().unwrap()).unwrap();
    assert_eq!(receipt["resource_id"], 123);
    assert!(publish(&mut app, &owner, result));
    app.scoped_actions_owner_v1 = None;
    let form = app.form.as_ref().unwrap();
    assert!(form.actions_publication.is_some());
    assert!(!form.plan_ready());
    let key = form.resource();
    assert!(Cache::open(&f::database(&fixture))
        .unwrap()
        .draft(&form.scope(), &key)
        .unwrap()
        .is_none());
    assert_eq!(
        Cache::open(&f::database(&fixture))
            .unwrap()
            .draft(&form.scope(), "keep-actions")
            .unwrap()
            .as_deref(),
        Some("ORIGINAL_ACTIONS_DRAFT")
    );
    assert!(
        app.actions.as_ref().unwrap().rows.is_empty(),
        "Original body retired after actual attempt"
    );
    let controls = [
        FormAction::Submit,
        FormAction::AcknowledgeActionsV3,
        FormAction::Close,
        FormAction::ErrorScroll(-3),
        FormAction::ErrorScroll(3),
    ];
    let mut visited = std::collections::BTreeSet::new();
    for _ in 0..controls.len() {
        assert!(app
            .form
            .as_mut()
            .unwrap()
            .key(
                crossterm::event::KeyCode::Tab,
                crossterm::event::KeyModifiers::NONE,
            )
            .is_none());
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                crate::forms::draw(frame, &mut app, area);
            })
            .unwrap();
        let focus = app.form.as_ref().unwrap().focus;
        assert!(
            visited.insert(focus),
            "Each actual Tab reaches a distinct visible control"
        );
        let hit = app
            .hits
            .iter()
            .find(|hit| hit.action == Action::Form(controls[focus].clone()))
            .unwrap();
        assert!(
            terminal.backend().buffer()[(hit.rect.x, hit.rect.y)]
                .modifier
                .contains(ratatui::style::Modifier::REVERSED),
            "Every keyboard-reachable control, including error scrolling, paints focus"
        );
        assert!(
            !app.hits
                .iter()
                .any(|hit| hit.action == Action::Form(FormAction::NewIntent)),
            "Inspect-only original has no misleading new-writer escape"
        );
    }
    assert_eq!(visited.len(), controls.len());
    let before = w::result_sql(&fixture);
    app.form.as_mut().unwrap().dispatch(&FormAction::Preview);
    assert!(!capture_form_work(&mut app));
    assert_eq!(w::result_sql(&fixture), before);
    assert_eq!(f::git_state(&fixture), git);
    w::exact(&fixture, &all);
}
#[test]
fn scoped_actions_publication_v3_accepted_is_not_completed_and_still_pending_inspection_cannot_ack_or_retry(
) {
    let mut steps = w::confirm_phase(w::running(), false, 42);
    steps.extend(w::read_phase(w::running(), 42));
    let (fixture, provider, mut app, all) = prepared(steps);
    let (result, owner, _draft) = confirm(&mut app, &provider, &fixture);
    assert!(publish(&mut app, &owner, result));
    app.scoped_actions_owner_v1 = None;
    w::paint(&mut app, 120, 40);
    assert_eq!(w::result_sql(&fixture)[0].0, "accepted_pending");
    assert!(app.form.as_ref().unwrap().error.contains("NOT confirmed"));
    let work = w::request(&mut app, FormAction::Submit);
    let owner = ScopedActionsOwnerV1::new(work).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let result = execute(&owner, &provider, &f::database(&fixture), None).unwrap();
    assert!(publish(&mut app, &owner, result));
    app.scoped_actions_owner_v1 = None;
    w::paint(&mut app, 120, 40);
    let sql = w::result_sql(&fixture);
    assert!(app
        .form
        .as_ref()
        .unwrap()
        .plan_lines()
        .iter()
        .any(|line| line.contains("still pending")));
    app.form
        .as_mut()
        .unwrap()
        .dispatch(&FormAction::AcknowledgeActionsV3);
    assert!(!capture_form_work(&mut app));
    assert_eq!(w::result_sql(&fixture), sql);
    assert!(app.form.as_ref().unwrap().error.contains("Inspect"));
    w::exact(&fixture, &all);
}
#[test]
fn scoped_actions_publication_v3_failed_post_has_one_uncertain_marker_no_receipt_and_reopens_only_original_history(
) {
    let mut steps = w::confirm_phase(w::running(), true, 42);
    steps.extend(w::read_phase(w::running(), 42));
    let (fixture, provider, mut app, all) = prepared(steps);
    let opened = match &app.form.as_ref().unwrap().purpose {
        FormPurpose::ScopedActionsV3(state) => state.opened().clone(),
        _ => panic!("Actual Form"),
    };
    let git = f::git_state(&fixture);
    let (result, owner, _draft) = confirm(&mut app, &provider, &fixture);
    let rows = w::result_sql(&fixture);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "uncertain");
    assert!(rows[0].2.is_none());
    assert!(publish(&mut app, &owner, result));
    assert!(app.form.is_none());
    assert!(app.actions.is_none());
    assert_eq!(
        app.overlay, None,
        "Retired private Form cannot invisibly capture input"
    );
    assert!(app.form_dismissed);
    app.handle_event(crossterm::event::Event::Key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('q'),
            crossterm::event::KeyModifiers::NONE,
        ),
    ));
    assert!(
        app.quit,
        "Global keyboard Quit remains usable after private retirement"
    );
    let reopened = source::open(
        opened.intent().clone(),
        &provider,
        &f::database(&fixture),
        &CancellationToken::default(),
        Instant::now() + std::time::Duration::from_secs(30),
    )
    .unwrap();
    assert!(reopened.history().is_some());
    let form = ui::form(reopened).unwrap();
    assert!(form.actions_publication.is_some());
    assert!(!form.plan_ready());
    assert_eq!(w::result_sql(&fixture), rows);
    assert_eq!(f::git_state(&fixture), git);
    w::exact(&fixture, &all);
}
#[test]
fn scoped_actions_publication_v3_crash_before_draft_ack_recovers_bounded_original_marker_without_minting_source(
) {
    let (fixture, provider, mut app, all) = w::setup(w::read_phase(w::running(), 42));
    let reference = original_pending(&app, &provider, &fixture);
    let before = w::result_sql(&fixture);
    let git = f::git_state(&fixture);
    w::open(&mut app, &provider, &fixture);
    assert_eq!(
        app.form.as_ref().unwrap().actions_publication.as_ref(),
        Some(&reference)
    );
    assert!(!app.form.as_ref().unwrap().plan_ready());
    assert!(app
        .form
        .as_ref()
        .unwrap()
        .plan_lines()
        .iter()
        .any(|line| line.contains("Pending invocation")));
    assert_eq!(w::result_sql(&fixture), before);
    assert_eq!(f::git_state(&fixture), git);
    w::exact(&fixture, &all);
}
#[test]
fn scoped_actions_publication_v3_explicit_fresh_unknown_result_ack_uses_same_journal_commit_without_any_post(
) {
    let mut steps = w::read_phase(w::running(), 42);
    steps.extend(w::read_phase(w::running(), 42));
    let (fixture, provider, mut app, all) = w::setup(steps);
    original_pending(&app, &provider, &fixture);
    w::open(&mut app, &provider, &fixture);
    w::paint(&mut app, 120, 40);
    let before = w::result_sql(&fixture);
    assert_eq!(before[0].0, "pending");
    let work = w::request(&mut app, FormAction::AcknowledgeActionsV3);
    let owner = ScopedActionsOwnerV1::new(work).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let value = execute(&owner, &provider, &f::database(&fixture), None).unwrap();
    assert!(publish(&mut app, &owner, value));
    app.scoped_actions_owner_v1 = None;
    assert_eq!(w::result_sql(&fixture)[0].0, "acknowledged");
    assert!(w::result_sql(&fixture)[0].2.is_none());
    assert!(!app.form.as_ref().unwrap().plan_ready());
    assert!(app
        .form
        .as_ref()
        .unwrap()
        .plan_lines()
        .iter()
        .any(|line| line.contains("no remote mutation")));
    w::exact(&fixture, &all);
}
#[test]
fn scoped_actions_publication_v3_actual_source_sql_change_before_confirm_creates_no_attempt_and_no_gh_child(
) {
    let (fixture, provider, mut app, all) = prepared(vec![]);
    let git = f::git_state(&fixture);
    let work = w::request(&mut app, FormAction::Submit);
    let drafts = crate::drafts::DraftWriter::new(f::database(&fixture));
    let owner =
        ScopedActionsOwnerV1::new_publication_v3(work, &f::database(&fixture), &drafts.client())
            .unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let actual_source = app.scoped_actions_v1.as_ref().unwrap().source.original();
    Cache::open(&f::database(&fixture))
        .unwrap()
        .reserve_sync_generation(actual_source.scope(), actual_source.sync_source())
        .unwrap();
    let result = execute(
        &owner,
        &provider,
        &f::database(&fixture),
        Some(&drafts.client()),
    )
    .unwrap();
    assert!(
        matches!(&result,PreparedResultV3::Publication(prepared)if matches!(&prepared.outcome,PublicationOutcomeV3::Attempt(result) if matches!(result.result_for_test_v8().0, ActionsExecution::Refused{..})))
    );
    assert!(w::result_sql(&fixture).is_empty());
    assert_eq!(f::git_state(&fixture), git);
    w::exact(&fixture, &all);
}
