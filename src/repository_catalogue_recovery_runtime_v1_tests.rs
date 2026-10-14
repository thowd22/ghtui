//! Source-only Request/Completion barriers, genuine closed source fixtures.
//! Poison result DTOs are publisher-ownership probes, never API/access proofs.
use super::*;
use crate::{
    github_workspace::GithubWorkspace,
    repository_catalogue_native::original_recovery_v1::tests as recovery,
    repository_catalogue_read_limits_test_support_v1 as support, test_support::Fixture,
};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

fn poll(background: &mut Background, app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while background.current.contains_key(&5) {
        background.poll(app);
        assert!(
            Instant::now() < deadline,
            "Actual recovery completion barrier missing"
        );
        thread::yield_now();
    }
}
fn attach_remote(
    app: &mut App,
    slot: &crate::repository_catalogue_native::CatalogueReReadRecoveryV1,
) {
    app.github = Some(GithubWorkspace::empty(
        recovery::closed_source(slot).source().identity().clone(),
        vec!["example/private-current".into()],
        Default::default(),
    ));
    app.content[View::Inbox.index()] = vec!["CURRENT_PRIVATE_FRAME".into()];
    app.status = "CURRENT_STATUS".into();
}

#[test]
fn repo_catalog_original_recovery_runtime_detached_slot_refuses_before_status_or_spawn() {
    let fixture = Fixture::new();
    let steps = recovery::baseline();
    fixture.scripted_gh(&steps);
    let (mut app, slot) = recovery::retained_app(&fixture);
    attach_remote(&mut app, &slot);
    let owner = app.catalogue.as_ref().unwrap().owner.clone();
    let foreign_slot = recovery::equivalent_slot(&slot);
    let mut background = Background::new();
    let ran = Arc::new(AtomicBool::new(false));
    let observed = ran.clone();
    assert!(
        !background.submit_catalogue_recovery(&mut app, owner, foreign_slot, move |_| {
            observed.store(true, Ordering::SeqCst);
            Err("Must not run".into())
        })
    );
    assert!(!ran.load(Ordering::SeqCst) && background.current.is_empty());
    assert_eq!(app.status, "CURRENT_STATUS");
    assert_eq!(app.github.as_ref().unwrap().identity.account_id, Some(42));
    assert_eq!(
        support::generation(&fixture, recovery::closed_source(&slot)),
        2
    );
    support::exact(&fixture, &steps);
}

#[test]
fn repo_catalog_original_recovery_runtime_slot_replacement_rejects_success_error_and_panic_before_effects(
) {
    for outcome in 0..3 {
        let fixture = Fixture::new();
        let steps = recovery::baseline();
        fixture.scripted_gh(&steps);
        let (mut app, slot) = recovery::retained_app(&fixture);
        attach_remote(&mut app, &slot);
        let owner = app.catalogue.as_ref().unwrap().owner.clone();
        let mut background = Background::new();
        let (started, live) = mpsc::sync_channel(1);
        let (release, waiting) = mpsc::sync_channel(1);
        let poison = recovery::closed_source(&slot).source().identity().clone();
        assert!(background.submit_catalogue_recovery(
            &mut app,
            owner.clone(),
            slot.clone(),
            move |_| {
                started.send(()).unwrap();
                waiting.recv_timeout(Duration::from_secs(3)).unwrap();
                match outcome {
                    0 => Ok(PreparedView {
                        clear_private: true,
                        verified_identity: Some(poison),
                        status: "LATE_PRIVATE_STATUS".into(),
                        ..Default::default()
                    }),
                    1 => Err("LATE_PRIVATE_ERROR".into()),
                    _ => panic!("LATE_PRIVATE_PANIC"),
                }
            }
        ));
        live.recv_timeout(Duration::from_secs(3)).unwrap();
        let replacement = recovery::equivalent_slot(&slot);
        recovery::install_slot(&mut app, replacement.clone());
        release.send(()).unwrap();
        poll(&mut background, &mut app);
        assert!(crate::repository_catalogue_native::owns_recovery(
            &app,
            &owner,
            &replacement
        ));
        assert_eq!(app.status, "CURRENT_STATUS");
        assert_eq!(app.context, "Independent local repository");
        assert_eq!(app.content[View::Inbox.index()], ["CURRENT_PRIVATE_FRAME"]);
        assert_eq!(app.github.as_ref().unwrap().identity.account_id, Some(42));
        assert_eq!(
            support::generation(&fixture, recovery::closed_source(&slot)),
            2
        );
        support::exact(&fixture, &steps);
    }
}

#[test]
fn repo_catalog_original_recovery_runtime_retired_modal_owner_rejects_late_clear_even_same_slot() {
    let fixture = Fixture::new();
    let steps = recovery::baseline();
    fixture.scripted_gh(&steps);
    let (mut app, slot) = recovery::retained_app(&fixture);
    attach_remote(&mut app, &slot);
    let owner = app.catalogue.as_ref().unwrap().owner.clone();
    let mut background = Background::new();
    let (started, live) = mpsc::sync_channel(1);
    let (release, waiting) = mpsc::sync_channel(1);
    assert!(
        background.submit_catalogue_recovery(&mut app, owner, slot.clone(), move |_| {
            started.send(()).unwrap();
            waiting.recv_timeout(Duration::from_secs(3)).unwrap();
            Ok(PreparedView {
                clear_private: true,
                status: "RETIRED_OWNER_CLEAR".into(),
                ..Default::default()
            })
        })
    );
    live.recv_timeout(Duration::from_secs(3)).unwrap();
    recovery::retire_owner(&mut app);
    release.send(()).unwrap();
    poll(&mut background, &mut app);
    assert!(app.github.is_some());
    assert_eq!(app.status, "CURRENT_STATUS");
    assert_eq!(app.content[View::Inbox.index()], ["CURRENT_PRIVATE_FRAME"]);
    assert_eq!(
        support::generation(&fixture, recovery::closed_source(&slot)),
        2
    );
    support::exact(&fixture, &steps);
}

#[test]
fn repo_catalog_original_recovery_runtime_owned_failure_retires_slot_and_private_views_not_local_context(
) {
    let fixture = Fixture::new();
    let steps = recovery::baseline();
    fixture.scripted_gh(&steps);
    let (mut app, slot) = recovery::retained_app(&fixture);
    attach_remote(&mut app, &slot);
    let owner = app.catalogue.as_ref().unwrap().owner.clone();
    let mut background = Background::new();
    assert!(
        background.submit_catalogue_recovery(&mut app, owner, slot.clone(), |_| Err(
            "PRIVATE_OWNED_FAILURE".into()
        ))
    );
    poll(&mut background, &mut app);
    assert!(app.github.is_none());
    assert!(!app.catalogue.as_ref().unwrap().discovery.has_recovery());
    assert!(!app.content[View::Inbox.index()]
        .iter()
        .any(|line| line.contains("CURRENT_PRIVATE_FRAME")));
    assert_eq!(app.view, View::Repository);
    assert_eq!(app.context, "Independent local repository");
    assert_eq!(
        support::generation(&fixture, recovery::closed_source(&slot)),
        2
    );
    support::exact(&fixture, &steps);
}
