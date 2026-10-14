use super::*;
use crate::{runtime::Background, storage::Scope, test_support::Fixture};
use std::{sync::mpsc, thread, time::Instant};

fn shell() -> App {
    let mut app = App {
        settings_host: "github.invalid".into(),
        view: View::Repository,
        focus: 3,
        ..Default::default()
    };
    app.context = "Original local repository".into();
    dispatch(&mut app, CatalogueUiAction::Open);
    assert_eq!(app.overlay, Some(Overlay::RepositoryCatalogue));
    app
}
fn poll(background: &mut Background, app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while background.catalogue_pending() {
        background.poll(app);
        assert!(
            Instant::now() < deadline,
            "Real bounded worker failed to complete"
        );
        thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn repo_catalog_native_boundary_open_source_and_close_never_queue_or_retarget_local() {
    let mut app = shell();
    assert_eq!(app.loading, [false; 5]);
    assert!(take_request(&mut app).is_none());
    dispatch(&mut app, CatalogueUiAction::Source);
    dispatch(&mut app, CatalogueUiAction::Kind(1));
    assert!(matches!(
        app.catalogue.as_ref().unwrap().dialog,
        Some(CatalogueDialog::Source { kind: 1, .. })
    ));
    assert!(take_request(&mut app).is_none());
    dispatch(&mut app, CatalogueUiAction::DialogClose);
    dispatch(&mut app, CatalogueUiAction::Close);
    assert_eq!(app.view, View::Repository);
    assert_eq!(app.context, "Original local repository");
    assert_eq!(app.focus, 3);
    assert!(app.catalogue.is_none());
    assert!(app.catalogue_cancel_requested);
}
#[test]
fn repo_catalog_native_boundary_busy_close_is_same_action_and_preserves_focus() {
    let mut app = shell();
    let expected = [
        app.github_content_focused,
        app.repository_content_focused,
        app.actions_content_focused,
        app.metrics_content_focused,
    ];
    app.catalogue.as_mut().unwrap().busy = true;
    dispatch(&mut app, CatalogueUiAction::Source);
    assert!(app.catalogue.as_ref().unwrap().dialog.is_none());
    assert!(app
        .catalogue
        .as_ref()
        .unwrap()
        .status
        .contains("no action resent"));
    dispatch(&mut app, CatalogueUiAction::Close);
    assert_eq!(app.focus, 3);
    assert_eq!(
        [
            app.github_content_focused,
            app.repository_content_focused,
            app.actions_content_focused,
            app.metrics_content_focused
        ],
        expected
    );
    assert!(take_request(&mut app).is_none());
}
#[test]
fn repo_catalog_native_boundary_stale_success_failure_and_panic_never_clear_new_source() {
    for outcome in 0..3 {
        let mut app = shell();
        app.content[View::Inbox.index()] =
            vec!["Original private inbox remains independently owned".into()];
        let owner = app.catalogue.as_ref().unwrap().owner.clone();
        let mut background = Background::new();
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        assert!(background.submit_catalogue(&mut app, owner, move |_| {
            started_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
            match outcome {
                0 => Ok(PreparedView {
                    clear_private: true,
                    ..Default::default()
                }),
                1 => Err("PRIVATE_STALE_FAILURE_CANARY".into()),
                _ => panic!("PRIVATE_STALE_PANIC_CANARY"),
            }
        }));
        started_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        // Change actual owned source without submitting/cancelling the old
        // generation: this independently exercises the owner fence itself.
        dispatch(&mut app, CatalogueUiAction::Source);
        dispatch(&mut app, CatalogueUiAction::Kind(1));
        dispatch(&mut app, CatalogueUiAction::Submit);
        let replacement = app.catalogue.as_ref().unwrap().owner.clone();
        assert!(app.catalogue.as_ref().unwrap().busy);
        release_tx.send(()).unwrap();
        poll(&mut background, &mut app);
        assert!(owns(&app, &replacement));
        assert!(!app.catalogue.as_ref().unwrap().fenced);
        assert_eq!(
            app.content[View::Inbox.index()],
            vec!["Original private inbox remains independently owned"]
        );
        assert!(!app.status.contains("PRIVATE_STALE"));
        assert!(matches!(
            take_request(&mut app),
            Some(CatalogueWork::Load { .. })
        ));
    }
}
#[test]
fn repo_catalog_native_boundary_dedicated_lane_does_not_cancel_local_worker() {
    let mut app = shell();
    let owner = app.catalogue.as_ref().unwrap().owner.clone();
    let mut background = Background::new();
    let (started_tx, started_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    background.submit(&mut app, View::Repository, move |_| {
        started_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        Ok(PreparedView {
            lines: vec!["Independent local completion".into()],
            status: "Local completion".into(),
            ..Default::default()
        })
    });
    started_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    let prepared_owner = owner.clone();
    assert!(
        background.submit_catalogue(&mut app, owner, move |_| Ok(prepared(
            prepared_owner,
            CataloguePreparedKind::Message("Local prepared inventory explanation".into())
        )))
    );
    poll(&mut background, &mut app);
    assert!(app.loading[View::Repository.index()]);
    assert!(matches!(
        app.catalogue.as_ref().unwrap().dialog,
        Some(CatalogueDialog::Message { .. })
    ));
    release_tx.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while app.loading[View::Repository.index()] {
        background.poll(&mut app);
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        app.content[View::Repository.index()],
        vec!["Independent local completion"]
    );
    assert_eq!(app.overlay, Some(Overlay::RepositoryCatalogue));
}
#[test]
fn repo_catalog_native_boundary_current_failure_hides_private_ui_not_original_sql() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[]);
    let database = fixture.root.join("private.sqlite3");
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    let cache = Cache::open(&database).unwrap();
    cache
        .put_draft(&scope, "keep-original", "Original draft bytes")
        .unwrap();
    cache
        .save_view(
            &scope,
            "keep-set",
            &serde_json::json!({"original_id":10001}),
        )
        .unwrap();
    drop(cache);
    let mut app = shell();
    app.content[View::Inbox.index()] = vec!["PRIVATE_CURRENT_INBOX_CANARY".into()];
    let owner = app.catalogue.as_ref().unwrap().owner.clone();
    let mut background = Background::new();
    assert!(background.submit_catalogue(&mut app, owner, |_| Err(
        "PRIVATE_PROVIDER_OR_SQL_ERROR_CANARY".into()
    )));
    poll(&mut background, &mut app);
    assert!(app.catalogue.as_ref().unwrap().fenced);
    assert!(app.catalogue.as_ref().unwrap().guard.is_none());
    assert!(!app.content[View::Inbox.index()]
        .join("\n")
        .contains("PRIVATE_CURRENT"));
    assert!(!app.status.contains("PRIVATE_PROVIDER"));
    let cache = Cache::open(&database).unwrap();
    assert_eq!(
        cache.draft(&scope, "keep-original").unwrap().as_deref(),
        Some("Original draft bytes")
    );
    assert_eq!(
        cache.saved_view(&scope, "keep-set").unwrap(),
        Some(serde_json::json!({"original_id":10001}))
    );
    assert!(cache
        .saved_views(&Scope::github("github.invalid", Some(43)).unwrap())
        .unwrap()
        .is_empty());
    assert!(!fixture.root.join("gh-fixture/commands.jsonl").exists());
}

#[test]
fn repo_catalog_native_boundary_reopening_accessible_source_keeps_explicit_kind() {
    let mut app = shell();
    app.catalogue.as_mut().unwrap().source =
        Some(RepositoryCatalogueSource::AuthenticatedAccessible);
    dispatch(&mut app, CatalogueUiAction::Source);
    assert!(matches!(
        app.catalogue.as_ref().unwrap().dialog,
        Some(CatalogueDialog::Source {
            kind: 1,
            editing: false,
            ..
        })
    ));
    assert!(take_request(&mut app).is_none());
}

#[test]
fn repo_catalog_native_boundary_owned_hidden_preserves_shell_after_inherited_remote_form() {
    let fixture = Fixture::new();
    let mut app = shell();
    // An inherited presentation form is not a catalogue/access proof. The
    // actual bounded worker failure must remove it without closing this owner.
    app.form = Some(
        crate::forms::Form::corpus(crate::corpus_ui::CorpusIntent {
            root: fixture.root.join("corpus"),
            identity: HostIdentity {
                host: "github.invalid".into(),
                account_id: Some(42),
                login: "old-presentation".into(),
                gh_version: "old-presentation".into(),
            },
            allowed_repositories: vec![],
            current_selections: vec![],
            catalog: vec![],
            recoveries: vec![],
            orphans: vec![],
            choices: Default::default(),
            plan: None,
            progress: Default::default(),
        })
        .unwrap(),
    );
    assert_eq!(app.form.as_ref().unwrap().view(), View::Search);
    let owner = app.catalogue.as_ref().unwrap().owner.clone();
    let produced = owner.clone();
    let mut background = Background::new();
    assert!(background.submit_catalogue(&mut app, owner, move |_| Ok(hidden(produced))));
    poll(&mut background, &mut app);
    assert!(app.form.is_none());
    assert_eq!(app.overlay, Some(Overlay::RepositoryCatalogue));
    assert!(app.catalogue.as_ref().unwrap().fenced);
    assert!(app
        .remote_scope
        .as_ref()
        .unwrap()
        .blocked_intent()
        .is_some());
    assert!(!fixture.root.join("gh-fixture/commands.jsonl").exists());
}
