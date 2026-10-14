use super::*;
use crate::{repository_catalogue_read_limits_test_support_v1 as support, test_support::Fixture};
use crossterm::event::KeyModifiers;
use ratatui::{backend::TestBackend, Terminal};

fn app_with(original: Arc<CatalogueIntentSource>) -> App {
    let mut app = App {
        settings_host: support::HOST.into(),
        view: View::Repository,
        context: "Independent local repository".into(),
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
fn query(modal: &mut CatalogueModal, text: &str) {
    if let Some(CatalogueRequest::Prepare(job)) = modal
        .workspace
        .as_mut()
        .unwrap()
        .apply(CatalogueInput::Paste(text.into()))
        .unwrap()
    {
        modal
            .workspace
            .as_mut()
            .unwrap()
            .accept(job.prepare(&CancellationToken::default()).unwrap())
            .unwrap();
    }
}
fn initial(fixture: &Fixture) -> Arc<CatalogueIntentSource> {
    support::load(fixture, &RepositoryCatalogueLimits::default())
}

#[test]
fn repo_catalog_source_transition_retry_genuine_fresh_source_keeps_selected_pin_query_not_old_permission(
) {
    let fixture = Fixture::new();
    let mut current = support::row(0);
    current["permissions"]["pull"] = serde_json::json!(false);
    let mut steps = support::collection(&[support::row(0)], 20, 42, 42);
    steps.extend(support::collection(&[current], 20, 42, 42));
    fixture.scripted_gh(&steps);
    let original = initial(&fixture);
    let mut app = app_with(original.clone());
    let modal = app.catalogue.as_mut().unwrap();
    modal
        .workspace
        .as_mut()
        .unwrap()
        .apply(CatalogueInput::Select)
        .unwrap();
    modal
        .workspace
        .as_mut()
        .unwrap()
        .apply(CatalogueInput::Pin)
        .unwrap();
    query(modal, "repo0000");
    retry(modal).unwrap();
    assert_eq!(modal.workspace.as_ref().unwrap().query(), "repo0000");
    assert_eq!(
        support::generation(&fixture, &original),
        1,
        "Queueing is no SQL/transport"
    );
    let work = take_request(&mut app).unwrap();
    let output = super::super::execute(
        work,
        &support::provider(&fixture),
        &support::database(&fixture),
        &CancellationToken::default(),
    )
    .unwrap();
    super::super::accept(&mut app, output.catalogue.unwrap());
    let modal = app.catalogue.as_ref().unwrap();
    let fresh = modal.guard.as_ref().unwrap();
    assert!(!Arc::ptr_eq(fresh.source(), original.source()));
    let workspace = modal.workspace.as_ref().unwrap();
    assert_eq!(workspace.query(), "repo0000");
    assert!(workspace.selected_ids().contains(&10000) && workspace.pins().contains(&10000));
    assert_eq!(
        fresh.source().repositories()[0].permissions.pull,
        Some(false)
    );
    assert_eq!(support::generation(&fixture, &original), 2);
    support::exact(&fixture, &steps);
}

#[test]
fn repo_catalog_source_transition_same_casefold_source_uses_original_cas_and_original_endpoint() {
    let fixture = Fixture::new();
    let steps = support::collection(&[support::row(0)], 20, 42, 42);
    fixture.scripted_gh(&steps);
    let original = initial(&fixture);
    let mut app = app_with(original.clone());
    source_submit(
        app.catalogue.as_mut().unwrap(),
        0,
        TextBuffer::new("Example").unwrap(),
        true,
    )
    .unwrap();
    let Some(CatalogueWork::Reload {
        original: queued,
        choices,
        ..
    }) = take_request(&mut app)
    else {
        panic!("Same declaration must NOT call unguarded Load");
    };
    assert!(Arc::ptr_eq(&queued, &original));
    assert!(Arc::ptr_eq(choices.original(), original.source()));
    assert_eq!(
        queued.source().source(),
        &RepositoryCatalogueSource::Organization("example".into())
    );
    assert_eq!(support::generation(&fixture, &original), 1);
    support::exact(&fixture, &steps);
}

#[test]
fn repo_catalog_source_transition_retry_actual_same_login_uid43_has_no_private_sql_reservation_or_old_choice_publish(
) {
    let fixture = Fixture::new();
    let mut steps = support::collection(&[support::row(0)], 20, 42, 42);
    steps.extend(support::principal(43));
    fixture.scripted_gh(&steps);
    let original = initial(&fixture);
    let mut app = app_with(original.clone());
    let modal = app.catalogue.as_mut().unwrap();
    modal
        .workspace
        .as_mut()
        .unwrap()
        .apply(CatalogueInput::Select)
        .unwrap();
    retry(modal).unwrap();
    let output = super::super::execute(
        take_request(&mut app).unwrap(),
        &support::provider(&fixture),
        &support::database(&fixture),
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(output.clear_private);
    assert!(matches!(
        output.catalogue.as_ref().unwrap().kind,
        CataloguePreparedKind::Hidden
    ));
    assert_eq!(support::generation(&fixture, &original), 1);
    let cache = Cache::open(&support::database(&fixture)).unwrap();
    cache
        .validate_repository_catalogue_intent_source(&original)
        .unwrap();
    let foreign = crate::storage::Scope::github(support::HOST, Some(43)).unwrap();
    assert!(cache.saved_views(&foreign).unwrap().is_empty());
    assert!(cache.draft(&foreign, "original").unwrap().is_none());
    super::super::privacy(&mut app);
    assert!(app.catalogue.as_ref().unwrap().workspace.is_none());
    assert!(!app.catalogue.as_ref().unwrap().source_transition.active());
    support::exact(&fixture, &steps);
}

#[test]
fn repo_catalog_source_transition_different_source_back_preserves_query_and_exact_pending_editor() {
    let fixture = Fixture::new();
    let steps = support::collection(&[support::row(0)], 20, 42, 42);
    fixture.scripted_gh(&steps);
    let original = initial(&fixture);
    let mut app = app_with(original.clone());
    let modal = app.catalogue.as_mut().unwrap();
    query(modal, "private-filter-only");
    modal.shell_focus = 1;
    let mut editor = TextBuffer::new("other-source").unwrap();
    editor.horizontal(false, true);
    let caret = editor.cursor;
    let selection = editor.selection();
    source_submit(modal, 0, editor, true).unwrap();
    assert!(modal.source_transition.active());
    assert!(modal.request.is_none());
    assert!(Arc::ptr_eq(modal.guard.as_ref().unwrap(), &original));
    dispatch(&mut app, CatalogueSourceChangeActionV1::Back);
    let modal = app.catalogue.as_ref().unwrap();
    assert!(!modal.source_transition.active());
    assert_eq!(modal.shell_focus, 1);
    assert_eq!(
        modal.workspace.as_ref().unwrap().query(),
        "private-filter-only"
    );
    let Some(CatalogueDialog::Source {
        kind: 0,
        editor,
        editing: true,
    }) = &modal.dialog
    else {
        panic!("Actual editor must return, not a source-name reconstruction");
    };
    assert_eq!(editor.text, "other-source");
    assert_eq!(editor.cursor, caret);
    assert_eq!(editor.selection(), selection);
    assert_eq!(support::generation(&fixture, &original), 1);
    support::exact(&fixture, &steps);
}

#[test]
fn repo_catalog_source_transition_explicit_discard_changes_only_local_choices_no_saved_drafts_or_scope_activation(
) {
    let fixture = Fixture::new();
    let steps = support::collection(&[support::row(0)], 20, 42, 42);
    fixture.scripted_gh(&steps);
    let original = initial(&fixture);
    let mut app = app_with(original.clone());
    let modal = app.catalogue.as_mut().unwrap();
    modal
        .workspace
        .as_mut()
        .unwrap()
        .apply(CatalogueInput::Select)
        .unwrap();
    modal
        .workspace
        .as_mut()
        .unwrap()
        .apply(CatalogueInput::Pin)
        .unwrap();
    source_submit(modal, 0, TextBuffer::new("other").unwrap(), true).unwrap();
    let original_owner = modal.owner.clone();
    dispatch(&mut app, CatalogueSourceChangeActionV1::DiscardDiscover);
    let modal = app.catalogue.as_ref().unwrap();
    assert!(modal.workspace.is_none() && modal.guard.is_none());
    assert!(!Arc::ptr_eq(&modal.owner, &original_owner));
    let work = take_request(&mut app).unwrap();
    let CatalogueWork::ChangeSource {
        original: preserved,
        source,
        ..
    } = &work
    else {
        panic!("Switch must carry genuine retirement origin");
    };
    assert!(Arc::ptr_eq(preserved, &original));
    assert_eq!(
        source,
        &RepositoryCatalogueSource::Organization("other".into())
    );
    let views = super::super::retire_for_work(&mut app, &work);
    assert!(views.contains(&View::Inbox) && views.contains(&View::Actions));
    assert!(app
        .remote_scope
        .as_ref()
        .unwrap()
        .blocked_intent()
        .is_some());
    assert!(app.remote_scope_source.is_none());
    assert_eq!(app.context, "Independent local repository");
    let cache = Cache::open(&support::database(&fixture)).unwrap();
    cache
        .validate_repository_catalogue_intent_source(&original)
        .unwrap();
    assert_eq!(
        cache
            .draft(original.scope(), "original")
            .unwrap()
            .as_deref(),
        Some("ORIGINAL_UID42_DRAFT")
    );
    assert_eq!(support::generation(&fixture, &original), 1);
    support::exact(&fixture, &steps);
}

#[test]
fn repo_catalog_source_transition_detached_pending_owner_cannot_redirect_new_same_looking_live_source(
) {
    let fixture = Fixture::new();
    let mut steps = support::collection(&[support::row(0)], 20, 42, 42);
    steps.extend(support::collection(&[support::row(0)], 20, 42, 42));
    fixture.scripted_gh(&steps);
    let first = initial(&fixture);
    let mut app = app_with(first);
    let modal = app.catalogue.as_mut().unwrap();
    query(modal, "repo0000");
    source_submit(modal, 0, TextBuffer::new("other").unwrap(), true).unwrap();
    let next = initial(&fixture);
    let (mut workspace, job) = CatalogueWorkspace::new(next.source().clone()).unwrap();
    workspace
        .accept(job.prepare(&CancellationToken::default()).unwrap())
        .unwrap();
    let modal = app.catalogue.as_mut().unwrap();
    modal.owner = Arc::new(CatalogueOwner {
        host: support::HOST.into(),
        expected: Some(next.source().identity().clone()),
    });
    modal.guard = Some(next.clone());
    modal.workspace = Some(workspace);
    modal.status = "NEW_CONTEXT_CANARY".into();
    dispatch(&mut app, CatalogueSourceChangeActionV1::DiscardDiscover);
    let modal = app.catalogue.as_ref().unwrap();
    assert!(Arc::ptr_eq(modal.guard.as_ref().unwrap(), &next));
    assert!(Arc::ptr_eq(
        modal.workspace.as_ref().unwrap().source().unwrap(),
        next.source()
    ));
    assert_eq!(modal.status, "NEW_CONTEXT_CANARY");
    assert!(modal.request.is_none());
    support::exact(&fixture, &steps);
}

#[test]
fn repo_catalog_source_transition_raw_invalid_or_unknown_source_refuses_before_transport_persistence_or_live_state_drop(
) {
    let fixture = Fixture::new();
    let steps = support::collection(&[support::row(0)], 20, 42, 42);
    fixture.scripted_gh(&steps);
    let original = initial(&fixture);
    let mut app = app_with(original.clone());
    for (kind, text) in [
        (0, ""),
        (0, "bad/name"),
        (0, "bad\nname"),
        (0, ".."),
        (3, "other"),
    ] {
        let modal = app.catalogue.as_mut().unwrap();
        assert!(source_submit(modal, kind, TextBuffer::new(text).unwrap(), true).is_err());
        assert!(Arc::ptr_eq(modal.guard.as_ref().unwrap(), &original));
        assert!(modal.workspace.is_some() && modal.request.is_none());
        assert!(!modal.source_transition.active());
    }
    assert_eq!(support::generation(&fixture, &original), 1);
    support::exact(&fixture, &steps);
}

#[test]
fn repo_catalog_source_transition_invalid_submit_keeps_actual_editor_selection_focus_and_original_source(
) {
    let fixture = Fixture::new();
    let steps = support::collection(&[support::row(0)], 20, 42, 42);
    fixture.scripted_gh(&steps);
    let original = initial(&fixture);
    let mut app = app_with(original.clone());
    let modal = app.catalogue.as_mut().unwrap();
    query(modal, "repo0000");
    let original_owner = modal.owner.clone();
    for (kind, text) in [(0, ""), (0, "bad/name"), (0, ".."), (3, "other")] {
        let mut editor = TextBuffer::new(text).unwrap();
        editor.horizontal(false, true);
        let cursor = editor.cursor;
        let selection = editor.selection();
        modal.shell_focus = 1;
        modal.dialog = Some(CatalogueDialog::Source {
            kind,
            editor,
            editing: true,
        });
        super::super::submit_dialog(modal).unwrap();
        let Some(CatalogueDialog::Source {
            kind: retained_kind,
            editor,
            editing: true,
        }) = &modal.dialog
        else {
            panic!("Invalid local input must retain the same editable Source dialogue");
        };
        assert_eq!(*retained_kind, kind);
        assert_eq!(editor.text, text);
        assert_eq!(editor.cursor, cursor);
        assert_eq!(editor.selection(), selection);
        assert_eq!(modal.shell_focus, 1);
        assert!(modal.status.starts_with("Invalid source"));
        assert!(Arc::ptr_eq(&modal.owner, &original_owner));
        assert!(Arc::ptr_eq(modal.guard.as_ref().unwrap(), &original));
        assert_eq!(modal.workspace.as_ref().unwrap().query(), "repo0000");
        assert!(modal.request.is_none() && !modal.busy);
        assert!(!modal.source_transition.active());
    }
    assert_eq!(support::generation(&fixture, &original), 1);
    support::exact(&fixture, &steps);
}

#[test]
fn repo_catalog_source_transition_owned_warning_hides_painted_avatar_rows_before_redraw() {
    let fixture = Fixture::new();
    let steps = support::collection(&[support::row(0)], 20, 42, 42);
    fixture.scripted_gh(&steps);
    let original = initial(&fixture);
    let mut app = app_with(original.clone());
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| {
            crate::repository_catalogue_dialogs::draw(
                frame,
                frame.area(),
                app.catalogue.as_mut().unwrap(),
            );
        })
        .unwrap();
    assert_eq!(
        crate::repository_catalogue_avatars_v1::request(&app)
            .unwrap()
            .requested_rows(),
        1,
        "Actual visible original row is eligible before the overlay"
    );
    let modal = app.catalogue.as_mut().unwrap();
    modal
        .workspace
        .as_mut()
        .unwrap()
        .apply(CatalogueInput::Select)
        .unwrap();
    source_submit(modal, 0, TextBuffer::new("other").unwrap(), true).unwrap();
    assert!(modal.source_transition.active());
    assert_eq!(
        crate::repository_catalogue_avatars_v1::request(&app)
            .unwrap()
            .requested_rows(),
        0,
        "Owned warning hides old painted rows even before next drawing"
    );
    dispatch(&mut app, CatalogueSourceChangeActionV1::Back);
    assert_eq!(
        crate::repository_catalogue_avatars_v1::request(&app)
            .unwrap()
            .requested_rows(),
        0,
        "Restored Source editor still captures the underlying image surface"
    );
    assert_eq!(support::generation(&fixture, &original), 1);
    support::exact(&fixture, &steps);
}

#[test]
fn repo_catalog_source_transition_fullscreen_key_mouse_actions_resize_and_hidden_clickthrough_are_equivalent(
) {
    let fixture = Fixture::new();
    let steps = support::collection(&[support::row(0)], 20, 42, 42);
    fixture.scripted_gh(&steps);
    let original = initial(&fixture);
    let mut app = app_with(original);
    let modal = app.catalogue.as_mut().unwrap();
    query(modal, "query-only-intent");
    source_submit(modal, 0, TextBuffer::new("other").unwrap(), true).unwrap();
    for width in [120, 80, 40] {
        let theme = app.theme();
        let mut terminal = Terminal::new(TestBackend::new(width, 24)).unwrap();
        terminal
            .draw(|frame| {
                app.catalogue
                    .as_mut()
                    .unwrap()
                    .source_transition
                    .draw(frame, frame.area(), theme)
            })
            .unwrap();
        let transition = &mut app.catalogue.as_mut().unwrap().source_transition;
        assert_eq!(
            transition.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            Some(CatalogueSourceChangeActionV1::Back)
        );
        assert_eq!(
            transition.key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE)),
            Some(CatalogueSourceChangeActionV1::DiscardDiscover)
        );
        for (_, action) in &transition.hits {
            assert!(matches!(
                action,
                CatalogueSourceChangeActionV1::Back
                    | CatalogueSourceChangeActionV1::DiscardDiscover
            ));
        }
        let rect = transition
            .hits
            .iter()
            .find(|(_, action)| *action == CatalogueSourceChangeActionV1::DiscardDiscover)
            .unwrap()
            .0;
        assert_eq!(
            transition.mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: rect.x,
                row: rect.y,
                modifiers: KeyModifiers::NONE
            }),
            Some(CatalogueSourceChangeActionV1::DiscardDiscover)
        );
        transition.clear_layout();
        assert!(transition
            .mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: rect.x,
                row: rect.y,
                modifiers: KeyModifiers::NONE
            })
            .is_none());
        assert!(transition
            .key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE))
            .is_none());
    }
    support::exact(&fixture, &steps);
}
