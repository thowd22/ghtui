use super::*;
use crate::{
    repository_catalogue_read_limits_test_support_v1 as support,
    storage::Scope,
    test_support::{Fixture, GhStep},
};
use crossterm::event::{KeyModifiers, MouseEventKind};
use ratatui::{backend::TestBackend, Terminal};
use serde_json::json;

fn app_with(original: Arc<CatalogueIntentSource>) -> App {
    let mut app = App {
        settings_host: support::HOST.into(),
        view: View::Repository,
        context: "Local repository must survive".into(),
        ..Default::default()
    };
    super::super::dispatch(&mut app, CatalogueUiAction::Open);
    let (mut workspace, job) = CatalogueWorkspace::new(original.source().clone()).unwrap();
    workspace.restore_observed_pins(original.intent()).unwrap();
    workspace
        .accept(job.prepare(&CancellationToken::default()).unwrap())
        .unwrap();
    let modal = app.catalogue.as_mut().unwrap();
    modal.guard = Some(original.clone());
    modal.workspace = Some(workspace);
    modal.source = Some(original.source().source().clone());
    modal.owner = Arc::new(CatalogueOwner {
        host: support::HOST.into(),
        expected: Some(original.source().identity().clone()),
    });
    app
}
fn loaded(fixture: &Fixture, steps: &[GhStep]) -> Arc<CatalogueIntentSource> {
    fixture.scripted_gh(steps);
    support::load(fixture, &RepositoryCatalogueLimits::default())
}

#[test]
fn repo_catalog_read_limits_native_local_apply_back_preserve_source_editor_and_zero_io() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[]);
    let mut app = App {
        settings_host: support::HOST.into(),
        ..Default::default()
    };
    super::super::dispatch(&mut app, CatalogueUiAction::Open);
    super::super::dispatch(&mut app, CatalogueUiAction::Source);
    let modal = app.catalogue.as_mut().unwrap();
    modal.dialog = Some(CatalogueDialog::Source {
        kind: 0,
        editor: TextBuffer::new("genuine-source-draft").unwrap(),
        editing: true,
    });
    dispatch(&mut app, DiscoveryNativeActionV1::Limits);
    dispatch(
        &mut app,
        DiscoveryNativeActionV1::Limit(DiscoveryLimitsActionV1::Preset(1)),
    );
    dispatch(
        &mut app,
        DiscoveryNativeActionV1::Limit(DiscoveryLimitsActionV1::Apply),
    );
    let modal = app.catalogue.as_ref().unwrap();
    assert_eq!(modal.discovery.budget.limits().max_rows, 10000);
    assert!(
        matches!(&modal.dialog, Some(CatalogueDialog::Source { editor, editing: true, .. })
        if editor.text == "genuine-source-draft")
    );
    assert!(modal.request.is_none());
    dispatch(&mut app, DiscoveryNativeActionV1::Limits);
    dispatch(
        &mut app,
        DiscoveryNativeActionV1::Limit(DiscoveryLimitsActionV1::Preset(2)),
    );
    dispatch(
        &mut app,
        DiscoveryNativeActionV1::Limit(DiscoveryLimitsActionV1::Back),
    );
    assert_eq!(
        app.catalogue
            .as_ref()
            .unwrap()
            .discovery
            .budget
            .limits()
            .max_rows,
        10000
    );
    assert!(!support::database(&fixture).exists());
    support::exact(&fixture, &[]);
}

#[test]
fn repo_catalog_read_limits_native_close_warning_key_mouse_retains_choices_until_explicit_discard()
{
    let fixture = Fixture::new();
    let steps = support::collection(&[support::row(0)], 20, 42, 42);
    let original = loaded(&fixture, &steps);
    let mut app = app_with(original.clone());
    app.catalogue
        .as_mut()
        .unwrap()
        .workspace
        .as_mut()
        .unwrap()
        .apply(CatalogueInput::Select)
        .unwrap();
    super::super::dispatch(&mut app, CatalogueUiAction::Close);
    assert!(app.catalogue.as_ref().unwrap().discovery.close_warning);
    assert_eq!(
        app.catalogue
            .as_ref()
            .unwrap()
            .workspace
            .as_ref()
            .unwrap()
            .selected_ids()
            .len(),
        1
    );
    dispatch(&mut app, DiscoveryNativeActionV1::CloseBack);
    assert_eq!(app.overlay, Some(Overlay::RepositoryCatalogue));
    assert!(!app.catalogue.as_ref().unwrap().discovery.close_warning);
    super::super::dispatch(&mut app, CatalogueUiAction::Close);
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let theme = app.theme();
    terminal
        .draw(|frame| {
            app.catalogue
                .as_mut()
                .unwrap()
                .discovery
                .draw(frame, frame.area(), theme)
        })
        .unwrap();
    let rect = app
        .catalogue
        .as_ref()
        .unwrap()
        .discovery
        .hits
        .iter()
        .find(|(_, action)| *action == DiscoveryNativeActionV1::DiscardAndClose)
        .unwrap()
        .0;
    let action = app
        .catalogue
        .as_mut()
        .unwrap()
        .discovery
        .mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: rect.x,
            row: rect.y,
            modifiers: KeyModifiers::NONE,
        })
        .unwrap();
    dispatch(&mut app, action);
    assert!(app.catalogue.is_none());
    assert_eq!(app.view, View::Repository);
    assert_eq!(app.context, "Local repository must survive");
    let cache = Cache::open(&support::database(&fixture)).unwrap();
    cache
        .validate_repository_catalogue_intent_source(&original)
        .unwrap();
    assert_eq!(support::generation(&fixture, &original), 1);
    support::exact(&fixture, &steps);
}

#[test]
fn repo_catalog_read_limits_native_closed_unavailable_hides_rows_retains_only_original_no_retry() {
    let fixture = Fixture::new();
    let mut steps = support::collection(&[support::row(0)], 20, 42, 42);
    let mut failed = support::collection(&[], 100, 42, 42);
    failed[3] = support::get(
        "orgs/example/repos?type=all&sort=full_name&direction=asc&per_page=100&page=1",
        json!([]),
    );
    failed[3].stdout.clear();
    failed[3].stderr = b"HTTP 403: original catalogue endpoint denied".to_vec();
    failed[3].status = 1;
    steps.extend(failed);
    let original = loaded(&fixture, &steps);
    let mut app = app_with(original.clone());
    let workspace = app.catalogue.as_mut().unwrap().workspace.as_mut().unwrap();
    workspace.apply(CatalogueInput::Select).unwrap();
    dispatch(&mut app, DiscoveryNativeActionV1::More);
    dispatch(
        &mut app,
        DiscoveryNativeActionV1::Limit(DiscoveryLimitsActionV1::ReRead),
    );
    let work = take_request(&mut app).unwrap();
    let original_owner = work.owner().clone();
    let retired = retire_for_work(&mut app, &work);
    assert!(retired.contains(&View::Inbox));
    let view = super::super::execute(
        work,
        &support::provider(&fixture),
        &support::database(&fixture),
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(!view.clear_private);
    assert_eq!(
        view.verified_identity.as_ref().unwrap().account_id,
        Some(42)
    );
    assert!(owns(&app, &original_owner));
    super::super::accept(&mut app, view.catalogue.unwrap());
    let modal = app.catalogue.as_ref().unwrap();
    assert!(modal.workspace.is_none() && modal.guard.is_none());
    assert!(modal.discovery.has_recovery());
    assert!(modal.discovery.recovery.as_ref().unwrap().same_original());
    let retained = modal.discovery.recovery.as_ref().unwrap();
    assert!(!Arc::ptr_eq(&retained.original, &retained.closed_current));
    let cache = Cache::open(&support::database(&fixture)).unwrap();
    assert!(cache
        .validate_repository_catalogue_intent_source(&retained.original)
        .is_err());
    cache
        .validate_repository_catalogue_intent_source(&retained.closed_current)
        .unwrap();
    assert_eq!(
        retained.closed_current.source().identity().account_id,
        Some(42)
    );
    assert!(retained.closed_current.source().repositories().is_empty());
    assert!(retained
        .closed_current
        .source()
        .coverage()
        .failure
        .is_some());
    assert_eq!(support::generation(&fixture, &original), 2);
    assert!(Cache::open(&support::database(&fixture))
        .unwrap()
        .validate_repository_catalogue_intent_source(&original)
        .is_err());
    super::super::dispatch(&mut app, CatalogueUiAction::Source);
    assert!(take_request(&mut app).is_none());
    super::super::dispatch(&mut app, CatalogueUiAction::Close);
    assert!(app.catalogue.as_ref().unwrap().discovery.has_recovery());
    dispatch(&mut app, DiscoveryNativeActionV1::CloseBack);
    super::super::privacy(&mut app);
    assert!(!app.catalogue.as_ref().unwrap().discovery.has_recovery());
    let foreign = Scope::github(support::HOST, Some(43)).unwrap();
    assert!(Cache::open(&support::database(&fixture))
        .unwrap()
        .draft(&foreign, "original")
        .unwrap()
        .is_none());
    support::exact(&fixture, &steps);
}

#[test]
fn repo_catalog_read_limits_native_resize_stale_hits_cannot_choose_or_start_reread() {
    let mut state = CatalogueDiscoveryNativeV1 {
        dialog: Some(CatalogueDiscoveryLimitsDialogV1::new(
            CatalogueDiscoveryBudgetV1::default(),
            DiscoveryLimitsPurposeV1::ReRead,
        )),
        ..Default::default()
    };
    for width in [120, 80, 40] {
        let mut terminal = Terminal::new(TestBackend::new(width, 24)).unwrap();
        terminal
            .draw(|frame| {
                state.draw(
                    frame,
                    frame.area(),
                    crate::theme::Theme::from_settings(0, true),
                )
            })
            .unwrap();
        state.clear_layout();
        assert!(state
            .mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 20,
                row: 22,
                modifiers: KeyModifiers::NONE
            })
            .is_none());
        assert!(state
            .key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .is_none());
        assert_eq!(
            state.dialog.as_ref().unwrap().purpose(),
            DiscoveryLimitsPurposeV1::ReRead
        );
    }
}

#[test]
fn repo_catalog_read_limits_native_original_window_refuses_after_genuine_frame_preparation() {
    let fixture = Fixture::new();
    let steps = support::collection(&[support::row(0)], 20, 42, 42);
    let original = loaded(&fixture, &steps);
    let (mut workspace, job) = CatalogueWorkspace::new(original.source().clone()).unwrap();
    let token = CancellationToken::default();
    workspace.accept(job.prepare(&token).unwrap()).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    check_original_admission(deadline, &token).unwrap();
    token.cancel();
    assert!(check_original_admission(deadline, &token).is_err());
    assert!(
        check_original_admission(std::time::Instant::now(), &CancellationToken::default()).is_err()
    );
    assert_eq!(workspace.source().unwrap().repositories().len(), 1);
    assert_eq!(support::generation(&fixture, &original), 1);
    support::exact(&fixture, &steps);
}
