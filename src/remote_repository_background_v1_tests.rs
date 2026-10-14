//! UNRUN channel-barrier contracts for genuine remote origins and local jobs.
use super::*;
use crate::{
    remote_repository_native_v1 as native,
    remote_repository_v1::RemoteBrowseActionV1,
    remote_repository_v1_test_support as fx,
    test_support::Fixture,
    workspace::{RepositoryMode, Workspace},
};
use std::time::{Duration, Instant};
fn initial(f: &Fixture, o: &fx::Objects) -> (App, Vec<crate::test_support::GhStep>) {
    let mut steps = fx::source_steps();
    steps.extend(fx::prefix());
    steps.extend(fx::default_payload(o));
    steps.extend(fx::suffix(42));
    f.scripted_gh(&steps);
    let source = fx::source(f);
    let state = native::initial(
        source,
        0,
        &fx::provider(f),
        &f.root.join("cache.sqlite3"),
        &Default::default(),
    )
    .unwrap();
    let mut app = App {
        view: View::Repository,
        workspace: Workspace::load(&f.repo(), RepositoryMode::Tree, &o.commit, None).unwrap(),
        ..Default::default()
    };
    native::install_initial(&mut app, state);
    (app, steps)
}
fn drain(background: &mut Background, app: &mut App) {
    let until = Instant::now() + Duration::from_secs(8);
    while !background.current.is_empty() {
        assert!(
            Instant::now() < until,
            "actual completed worker must be observed"
        );
        background.poll(app);
        std::thread::yield_now();
    }
}
#[test]
fn remote_tree_v1_runtime_retired_owner_errors_and_panics_cannot_clear_local_or_current_scope() {
    for panic in [false, true] {
        let fixture = Fixture::new();
        let o = fx::objects(&fixture);
        let (mut app, steps) = initial(&fixture, &o);
        let mut background = Background::new();
        native::dispatch(&mut app, RemoteBrowseActionV1::Branches);
        let request = native::take_request(&mut app).unwrap();
        let (entered_tx, entered_rx) = mpsc::sync_channel(0);
        let (release_tx, release_rx) = mpsc::sync_channel(0);
        assert!(
            background.submit_remote_repository_v1(&mut app, request.owner, move |_| {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                if panic {
                    panic!("retired private worker panic");
                }
                Err("PRIVATE_RETIRED_ERROR".into())
            })
        );
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        native::retire(&mut app);
        app.status = "CURRENT_STATUS".into();
        app.context = "CURRENT_CONTEXT".into();
        release_tx.send(()).unwrap();
        drain(&mut background, &mut app);
        assert_eq!(app.status, "CURRENT_STATUS");
        assert_eq!(app.context, "CURRENT_CONTEXT");
        assert!(app.workspace.is_some());
        assert!(app.remote_scope.is_none());
        assert!(!app.loading[View::Repository.index()]);
        fx::exact(&fixture, &steps);
        fx::preserved(&fixture, &o);
    }
}
#[test]
fn remote_tree_v1_runtime_owned_failure_hides_remote_not_local_and_never_rebases() {
    let fixture = Fixture::new();
    let o = fx::objects(&fixture);
    let (mut app, steps) = initial(&fixture, &o);
    let original_revision = app.workspace.as_ref().unwrap().revision.clone();
    app.content[View::Inbox.index()] = vec!["PRIVATE_OLD_INBOX".into()];
    let mut background = Background::new();
    native::dispatch(&mut app, RemoteBrowseActionV1::Branches);
    let request = native::take_request(&mut app).unwrap();
    assert!(
        background.submit_remote_repository_v1(&mut app, request.owner, move |_| Err(
            "PRIVATE_CURRENT_DIAGNOSTIC".into()
        ))
    );
    drain(&mut background, &mut app);
    assert!(app.remote_repository.is_none());
    assert!(app.status.contains("private pages hidden"));
    assert!(!app.status.contains("PRIVATE_CURRENT_DIAGNOSTIC"));
    assert_eq!(app.workspace.as_ref().unwrap().revision, original_revision);
    assert!(app.content[View::Inbox.index()]
        .iter()
        .all(|line| !line.contains("PRIVATE_OLD_INBOX")));
    fx::exact(&fixture, &steps);
    fx::preserved(&fixture, &o);
}
#[test]
fn remote_tree_v1_runtime_browser_cancel_preserves_live_real_local_repository_worker() {
    let fixture = Fixture::new();
    let o = fx::objects(&fixture);
    let (mut app, steps) = initial(&fixture, &o);
    let mut background = Background::new();
    let (local_enter, local_entered) = mpsc::sync_channel(0);
    let (local_release, local_released) = mpsc::sync_channel(0);
    let path = fixture.repo();
    let commit = o.commit.clone();
    assert!(
        background.submit(&mut app, View::Repository, move |cancel| {
            local_enter.send(()).unwrap();
            local_released.recv().unwrap();
            Ok(PreparedView {
                workspace: Workspace::load_with_cancellation(
                    &path,
                    RepositoryMode::Tree,
                    &commit,
                    None,
                    cancel,
                )?,
                ..Default::default()
            })
        })
    );
    local_entered.recv_timeout(Duration::from_secs(5)).unwrap();
    native::dispatch(&mut app, RemoteBrowseActionV1::Branches);
    let request = native::take_request(&mut app).unwrap();
    let (remote_enter, remote_entered) = mpsc::sync_channel(0);
    let (remote_release, remote_released) = mpsc::sync_channel(0);
    assert!(
        background.submit_remote_repository_v1(&mut app, request.owner, move |_| {
            remote_enter.send(()).unwrap();
            remote_released.recv().unwrap();
            Err("old remote result".into())
        })
    );
    remote_entered.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(background.current.contains_key(&View::Repository.index()));
    assert!(background.current.contains_key(&6));
    native::retire(&mut app);
    background.cancel_remote_repository_v1();
    assert!(background.current.contains_key(&View::Repository.index()));
    assert!(app.loading[View::Repository.index()]);
    remote_release.send(()).unwrap();
    local_release.send(()).unwrap();
    drain(&mut background, &mut app);
    assert_eq!(app.workspace.as_ref().unwrap().revision, o.commit);
    assert!(!app.loading[View::Repository.index()]);
    fx::exact(&fixture, &steps);
    fx::preserved(&fixture, &o);
}
