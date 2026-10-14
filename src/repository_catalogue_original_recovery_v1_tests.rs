//! Source-only strict original-reservation recovery contracts; UNRUN.
use super::*;
use crate::{
    repository_catalogue_read_limits_test_support_v1 as support,
    repository_catalogue_reload_v1::reload_catalogue_intent_source,
    test_support::{Fixture, GhStep},
};
use crossterm::event::KeyModifiers;
use ratatui::{backend::TestBackend, Terminal};
use serde_json::json;
use std::{fs, thread, time::Duration};

fn unavailable() -> Vec<GhStep> {
    let mut steps = support::collection(&[], 20, 42, 42);
    steps[3].status = 1;
    steps[3].stdout.clear();
    steps[3].stderr = b"HTTP 403 PRIVATE_RECOVERY_DIAGNOSTIC".to_vec();
    steps
}
pub(crate) fn retained_app(fixture: &Fixture) -> (App, Arc<CatalogueReReadRecoveryV1>) {
    let original = support::load(fixture, &RepositoryCatalogueLimits::default());
    let (mut workspace, job) = CatalogueWorkspace::new(original.source().clone()).unwrap();
    workspace
        .accept(job.prepare(&CancellationToken::default()).unwrap())
        .unwrap();
    workspace.apply(CatalogueInput::Select).unwrap();
    workspace.apply(CatalogueInput::Pin).unwrap();
    if let Some(CatalogueRequest::Prepare(job)) = workspace
        .apply(CatalogueInput::Paste("repo0000".into()))
        .unwrap()
    {
        workspace
            .accept(job.prepare(&CancellationToken::default()).unwrap())
            .unwrap();
    }
    let choices = workspace.snapshot_transient_choices().unwrap();
    let mut cache = Cache::open(&support::database(fixture)).unwrap();
    let closed = Arc::new(
        reload_catalogue_intent_source(
            &support::provider(fixture),
            &mut cache,
            &original,
            &RepositoryCatalogueLimits::default(),
            &CancellationToken::default(),
        )
        .unwrap(),
    );
    assert!(closed.source().repositories().is_empty());
    assert!(closed.source().coverage().failure.is_some());
    let recovery = Arc::new(CatalogueReReadRecoveryV1 {
        original,
        closed_current: closed.clone(),
        choices,
    });
    assert!(recovery.same_original());
    let mut app = App {
        view: View::Repository,
        settings_host: support::HOST.into(),
        context: "Independent local repository".into(),
        ..Default::default()
    };
    super::super::dispatch(&mut app, CatalogueUiAction::Open);
    let modal = app.catalogue.as_mut().unwrap();
    modal.owner = Arc::new(CatalogueOwner {
        host: support::HOST.into(),
        expected: Some(closed.source().identity().clone()),
    });
    modal.source = Some(closed.source().source().clone());
    modal.discovery.recovery = Some(recovery.clone());
    (app, recovery)
}
pub(crate) fn baseline() -> Vec<GhStep> {
    let mut steps = support::collection(&[support::row(0), support::row(1)], 20, 42, 42);
    steps.extend(unavailable());
    assert_eq!(steps.len(), 14);
    steps
}
// Ownership probes duplicate only pointers to genuine factory-produced facts.
// They are cfg(test), not source/access constructors.
pub(crate) fn equivalent_slot(slot: &CatalogueReReadRecoveryV1) -> Arc<CatalogueReReadRecoveryV1> {
    Arc::new(CatalogueReReadRecoveryV1 {
        original: slot.original.clone(),
        closed_current: slot.closed_current.clone(),
        choices: slot.choices.clone(),
    })
}
pub(crate) fn install_slot(app: &mut App, slot: Arc<CatalogueReReadRecoveryV1>) {
    app.catalogue.as_mut().unwrap().discovery.recovery = Some(slot);
}
pub(crate) fn retire_owner(app: &mut App) {
    let modal = app.catalogue.as_mut().unwrap();
    modal.owner = Arc::new(CatalogueOwner {
        host: modal.owner.host.clone(),
        expected: modal.owner.expected.clone(),
    });
}
pub(crate) fn closed_source(slot: &CatalogueReReadRecoveryV1) -> &Arc<CatalogueIntentSource> {
    &slot.closed_current
}
fn execute(
    fixture: &Fixture,
    app: &App,
    slot: Arc<CatalogueReReadRecoveryV1>,
) -> Result<PreparedView, String> {
    execute_recovery(
        app.catalogue.as_ref().unwrap().owner.clone(),
        slot,
        RepositoryCatalogueLimits::default(),
        &support::provider(fixture),
        &support::database(fixture),
        &CancellationToken::default(),
    )
}
fn safe(failure: &str) {
    for canary in [
        "PRIVATE_RECOVERY_DIAGNOSTIC",
        "ORIGINAL_UID42_DRAFT",
        "CONCURRENT_INTENT_CANARY",
        "repo0000",
    ] {
        assert!(
            !failure.contains(canary),
            "No raw private diagnostic: {canary}"
        );
    }
}
fn preserved(fixture: &Fixture, slot: &CatalogueReReadRecoveryV1) {
    let cache = Cache::open(&support::database(fixture)).unwrap();
    assert_eq!(
        cache
            .draft(slot.original.scope(), "original")
            .unwrap()
            .as_deref(),
        Some("ORIGINAL_UID42_DRAFT")
    );
    let foreign = Scope::github(support::HOST, Some(43)).unwrap();
    assert!(cache.saved_views(&foreign).unwrap().is_empty());
    assert!(cache.list(&foreign, None).unwrap().is_empty());
    assert!(cache.draft(&foreign, "original").unwrap().is_none());
}
fn edit(fixture: &Fixture, source: &CatalogueIntentSource) {
    let mut intent = source.intent().clone();
    intent
        .set(source.source(), "CONCURRENT_INTENT_CANARY", &[10000])
        .unwrap();
    let payload = serde_json::to_string(&intent.to_value().unwrap()).unwrap();
    rusqlite::Connection::open(support::database(fixture)).unwrap().execute(
        "INSERT INTO saved_views(host,login,name,payload) VALUES(?1,?2,?3,?4) ON CONFLICT(host,login,name) DO UPDATE SET payload=excluded.payload",
        rusqlite::params![source.scope().host,source.scope().login,source.saved_view_key(),payload]).unwrap();
}
fn wait(fixture: &Fixture, count: usize) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap_or_default()
        .lines()
        .count()
        != count
    {
        assert!(Instant::now() < deadline, "Real gh child barrier missing");
        thread::yield_now();
    }
}

#[test]
fn repo_catalog_original_recovery_genuine_gen2_to3_same_uid_rename_restores_only_exact_node_choices(
) {
    let mut fresh = support::row(0);
    fresh["permissions"]["pull"] = json!(false);
    let mut steps = baseline();
    let mut closing = support::collection(&[fresh], 20, 42, 42);
    closing[6] = support::get("user", json!({"id":42,"login":"renamed-account"}));
    steps.extend(closing);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let (app, slot) = retained_app(&fixture);
    assert_eq!(support::generation(&fixture, &slot.closed_current), 2);
    let result = execute(&fixture, &app, slot.clone()).unwrap();
    assert_eq!(
        result.verified_identity.as_ref().unwrap().login,
        "renamed-account"
    );
    let CataloguePreparedKind::Reloaded { guard, workspace } = result.catalogue.unwrap().kind
    else {
        panic!("Genuine fresh result required");
    };
    assert_eq!(workspace.query(), "repo0000");
    assert!(workspace.selected_ids().contains(&10000) && workspace.pins().contains(&10000));
    assert_eq!(
        guard.source().repositories()[0].permissions.pull,
        Some(false)
    );
    assert!(!Arc::ptr_eq(guard.source(), slot.original.source()));
    assert_eq!(guard.scope(), slot.original.scope());
    assert_eq!(support::generation(&fixture, &guard), 3);
    assert!(Cache::open(&support::database(&fixture))
        .unwrap()
        .validate_repository_catalogue_intent_source(&slot.closed_current)
        .is_err());
    preserved(&fixture, &slot);
    support::exact(&fixture, &steps);
}

#[test]
fn repo_catalog_original_recovery_repeated_closed403_advances_real_reservation_without_auto_retry()
{
    let mut steps = baseline();
    steps.extend(unavailable());
    steps.extend(support::collection(&[support::row(0)], 20, 42, 42));
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let (app, slot) = retained_app(&fixture);
    let result = execute(&fixture, &app, slot.clone()).unwrap();
    let CataloguePreparedKind::ReloadUnavailable { recovery: next } =
        result.catalogue.unwrap().kind
    else {
        panic!("Closed unavailable is not empty success");
    };
    assert!(!Arc::ptr_eq(&slot, &next));
    assert!(
        Arc::ptr_eq(&slot.original, &next.original) && Arc::ptr_eq(&slot.choices, &next.choices)
    );
    assert_eq!(support::generation(&fixture, &next.closed_current), 3);
    assert_eq!(next.closed_current.source().accounting().requests(), 7);
    let result = execute(&fixture, &app, next.clone()).unwrap();
    let CataloguePreparedKind::Reloaded { guard, .. } = result.catalogue.unwrap().kind else {
        panic!("Explicit subsequent recovery required");
    };
    assert_eq!(support::generation(&fixture, &guard), 4);
    preserved(&fixture, &slot);
    support::exact(&fixture, &steps);
}

#[test]
fn repo_catalog_original_recovery_missing_changed_node_and_complete_empty_are_unresolved_not_saved_absence(
) {
    for scenario in 0..3 {
        let rows = match scenario {
            0 => vec![support::row(1)],
            1 => {
                let mut row = support::row(0);
                row["node_id"] = json!("GENUINE_DIFFERENT_NODE");
                vec![row]
            }
            _ => vec![],
        };
        let mut steps = baseline();
        steps.extend(support::collection(&rows, 20, 42, 42));
        let fixture = Fixture::new();
        fixture.scripted_gh(&steps);
        let (app, slot) = retained_app(&fixture);
        let result = execute(&fixture, &app, slot.clone()).unwrap();
        let CataloguePreparedKind::Reloaded { guard, workspace } = result.catalogue.unwrap().kind
        else {
            panic!("Fresh unknown membership must be explicit");
        };
        assert!(workspace.selected_ids().is_empty());
        assert!(workspace
            .prepare_saved_intent(guard.intent(), "must-not-save")
            .is_err());
        assert!(workspace.unresolved_choices_notice().contains("10000"));
        assert!(Cache::open(&support::database(&fixture))
            .unwrap()
            .saved_views(slot.original.scope())
            .unwrap()
            .is_empty());
        assert_eq!(support::generation(&fixture, &guard), 3);
        preserved(&fixture, &slot);
        support::exact(&fixture, &steps);
    }
}

#[test]
fn repo_catalog_original_recovery_pre_and_final_uid43_never_read_foreign_private_sql_or_publish_rows(
) {
    for final_loss in [false, true] {
        let mut steps = baseline();
        if final_loss {
            steps.extend(support::collection(&[support::row(0)], 20, 42, 43));
        } else {
            steps.extend(support::principal(43));
        }
        let fixture = Fixture::new();
        fixture.scripted_gh(&steps);
        let (app, slot) = retained_app(&fixture);
        let database = if final_loss {
            support::database(&fixture)
        } else {
            fixture.root.join("never-opened-private.sqlite3")
        };
        let failure = execute_recovery(
            app.catalogue.as_ref().unwrap().owner.clone(),
            slot.clone(),
            RepositoryCatalogueLimits::default(),
            &support::provider(&fixture),
            &database,
            &CancellationToken::default(),
        )
        .err()
        .unwrap();
        safe(&failure);
        if !final_loss {
            assert!(!database.exists() && !database.with_extension("sqlite3-wal").exists());
        }
        assert_eq!(
            support::generation(&fixture, &slot.closed_current),
            if final_loss { 3 } else { 2 }
        );
        preserved(&fixture, &slot);
        support::exact(&fixture, &steps);
    }
}

#[test]
fn repo_catalog_original_recovery_concurrent_payload_or_epoch_cas_never_adopts_latest_or_enumerates(
) {
    for epoch in [false, true] {
        let mut steps = baseline();
        steps.extend(support::principal(42));
        steps.push(support::get("orgs/example", support::owner()));
        let fixture = Fixture::new();
        fixture.scripted_gh(&steps);
        let (app, slot) = retained_app(&fixture);
        if epoch {
            rusqlite::Connection::open(support::database(&fixture))
                .unwrap()
                .execute(
                    "UPDATE sync_reservations SET generation=77 WHERE source=?1",
                    [slot.closed_current.sync_source()],
                )
                .unwrap();
        } else {
            edit(&fixture, &slot.original);
        }
        let failure = execute(&fixture, &app, slot.clone()).err().unwrap();
        safe(&failure);
        assert_eq!(
            support::generation(&fixture, &slot.closed_current),
            if epoch { 77 } else { 2 }
        );
        if !epoch {
            let values = Cache::open(&support::database(&fixture))
                .unwrap()
                .saved_views(slot.original.scope())
                .unwrap();
            assert_eq!(values.len(), 1);
            assert!(format!("{values:?}").contains("CONCURRENT_INTENT_CANARY"));
        }
        preserved(&fixture, &slot);
        support::exact(&fixture, &steps);
    }
}

#[test]
fn repo_catalog_original_recovery_after_reservation_payload_edit_stops_before_final_proof_without_rollback(
) {
    let mut steps = baseline();
    let mut fresh = support::collection(&[support::row(0)], 20, 42, 42);
    fresh[3].delay_ms = 250;
    steps.extend(fresh[..4].iter().cloned());
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let (app, slot) = retained_app(&fixture);
    let owner = app.catalogue.as_ref().unwrap().owner.clone();
    let provider = support::provider(&fixture);
    let database = support::database(&fixture);
    let worker_slot = slot.clone();
    let worker = thread::spawn(move || {
        execute_recovery(
            owner,
            worker_slot,
            RepositoryCatalogueLimits::default(),
            &provider,
            &database,
            &CancellationToken::default(),
        )
    });
    wait(&fixture, 18);
    edit(&fixture, &slot.original);
    safe(&worker.join().unwrap().err().unwrap());
    assert_eq!(support::generation(&fixture, &slot.closed_current), 3);
    preserved(&fixture, &slot);
    support::exact(&fixture, &steps);
}

#[test]
fn repo_catalog_original_recovery_sql_lock_cancellation_preserves_original_generation_and_clock() {
    let mut steps = baseline();
    steps.extend(support::principal(42));
    steps.push(support::get("orgs/example", support::owner()));
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let (app, slot) = retained_app(&fixture);
    let lock = rusqlite::Connection::open(support::database(&fixture)).unwrap();
    lock.execute_batch("BEGIN IMMEDIATE").unwrap();
    let owner = app.catalogue.as_ref().unwrap().owner.clone();
    let worker_slot = slot.clone();
    let provider = support::provider(&fixture);
    let database = support::database(&fixture);
    let cancel = CancellationToken::default();
    let worker_cancel = cancel.clone();
    let started = Instant::now();
    let worker = thread::spawn(move || {
        execute_recovery(
            owner,
            worker_slot,
            RepositoryCatalogueLimits::default(),
            &provider,
            &database,
            &worker_cancel,
        )
    });
    wait(&fixture, 17);
    cancel.cancel();
    lock.execute_batch("ROLLBACK").unwrap();
    safe(&worker.join().unwrap().err().unwrap());
    assert!(started.elapsed() < Duration::from_secs(3));
    assert_eq!(support::generation(&fixture, &slot.closed_current), 2);
    preserved(&fixture, &slot);
    support::exact(&fixture, &steps);
}

#[test]
fn repo_catalog_original_recovery_preview_both_inputs_all_widths_no_private_canary_or_io() {
    let steps = baseline();
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let (mut app, slot) = retained_app(&fixture);
    let owner = app.catalogue.as_ref().unwrap().owner.clone();
    dispatch_recovery(&mut app, CatalogueRecoveryActionV1::Preview);
    let theme = app.theme();
    for width in [120, 80, 40] {
        let mut terminal = Terminal::new(TestBackend::new(width, 24)).unwrap();
        terminal
            .draw(|frame| {
                app.catalogue
                    .as_mut()
                    .unwrap()
                    .discovery
                    .verification
                    .as_mut()
                    .unwrap()
                    .draw(frame, frame.area(), theme)
            })
            .unwrap();
        let preview = app
            .catalogue
            .as_mut()
            .unwrap()
            .discovery
            .verification
            .as_mut()
            .unwrap();
        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("[Enter Verify]") && rendered.contains("[Esc Back]"));
        assert!(
            !rendered.contains("repo0000")
                && !rendered.contains("10000")
                && !rendered.contains("OWNER_71")
        );
        let rect = preview
            .hits
            .iter()
            .find(|(_, action)| *action == CatalogueRecoveryActionV1::Verify)
            .unwrap()
            .0;
        assert_eq!(
            preview.mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: rect.x,
                row: rect.y,
                modifiers: KeyModifiers::NONE
            }),
            Some(CatalogueRecoveryActionV1::Verify)
        );
        assert_eq!(
            preview.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Some(CatalogueRecoveryActionV1::Verify)
        );
        preview.clear_layout();
        assert_eq!(
            preview.mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: rect.x,
                row: rect.y,
                modifiers: KeyModifiers::NONE
            }),
            None
        );
    }
    dispatch_recovery(&mut app, CatalogueRecoveryActionV1::Back);
    assert!(owns_recovery(&app, &owner, &slot));
    assert!(app.catalogue.as_ref().unwrap().request.is_none());
    assert_eq!(support::generation(&fixture, &slot.closed_current), 2);
    support::exact(&fixture, &steps);
}

#[test]
fn repo_catalog_original_recovery_prequeue_cancel_is_zero_io_retention_not_new_current_authority() {
    let steps = baseline();
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let (app, slot) = retained_app(&fixture);
    let cancel = CancellationToken::default();
    cancel.cancel();
    let cold = fixture.root.join("cold-private.sqlite3");
    let result = execute_recovery(
        app.catalogue.as_ref().unwrap().owner.clone(),
        slot.clone(),
        RepositoryCatalogueLimits::default(),
        &support::provider(&fixture),
        &cold,
        &cancel,
    )
    .unwrap();
    assert!(
        matches!(result.catalogue.unwrap().kind, CataloguePreparedKind::Message(message) if message.contains("slot retained"))
    );
    assert!(result.verified_identity.is_none() && !cold.exists());
    assert_eq!(support::generation(&fixture, &slot.closed_current), 2);
    support::exact(&fixture, &steps);
}
