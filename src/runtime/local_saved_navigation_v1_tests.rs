use super::*;
use crate::{
    app::Action,
    local_commit_actor_origin_v1_test_support::Real,
    workspace::{RepositoryMode, Workspace},
};
use std::time::{Duration, Instant};

fn app(real: &Real) -> App {
    App {
        workspace: Workspace::load(&real.root, RepositoryMode::Graph, &real.original, None)
            .unwrap(),
        view: View::Repository,
        refresh_requested: false,
        ..Default::default()
    }
}

#[test]
fn original_saved_navigation_runtime_genuine_remote_browser_facet_cannot_cross_local_publisher() {
    use crate::{
        remote_repository_v1::{
            read_remote_repository_v1, RemoteRepositoryEnvironmentV1, RemoteRepositoryOwnerV1,
            RemoteRepositoryPreparedV1,
        },
        remote_repository_v1_test_support as remote,
        test_support::Fixture,
    };
    let fixture = Fixture::new();
    let objects = remote::objects(&fixture);
    let mut steps = remote::source_steps();
    steps.extend(remote::prefix());
    steps.extend(remote::default_payload(&objects));
    steps.extend(remote::suffix(42));
    fixture.scripted_gh(&steps);
    let source = remote::source(&fixture);
    let owner = RemoteRepositoryOwnerV1::from_row(source.source().clone(), 0).unwrap();
    let result = read_remote_repository_v1(
        RemoteRepositoryEnvironmentV1 {
            provider: &remote::provider(&fixture),
            database: &fixture.root.join("cache.sqlite3"),
        },
        source.clone(),
        owner,
        1,
        crate::providers::gh::RemoteRepositoryRequestV1::DefaultTree,
        &Default::default(),
        &Default::default(),
    );
    assert!(matches!(&result, RemoteRepositoryPreparedV1::Ready { .. }));
    let real = Real::new("sha256");
    let before = real.snapshot();
    let mut app = app(&real);
    let local = app.workspace.as_ref().unwrap().clone();
    let mut background = Background::new();
    assert!(
        background.submit_original_local_v1(&mut app, None, move |_| Ok(PreparedView {
            workspace: Some(local),
            remote_repository: Some(
                crate::remote_repository_native_v1::RemoteRepositoryCompletionV1::Read(result)
            ),
            status: "FOREIGN_REMOTE_PUBLICATION".into(),
            ..Default::default()
        }))
    );
    wait(&mut background, &mut app, |app| {
        !app.loading[View::Repository.index()]
    });
    assert!(app.status.contains("foreign prepared kind"));
    assert!(!app.status.contains("FOREIGN_REMOTE_PUBLICATION"));
    assert!(app.remote_repository.is_none());
    assert!(app.remote_scope.is_none());
    assert_eq!(app.workspace.as_ref().unwrap().revision, real.original);
    assert_eq!(real.snapshot(), before);
    crate::storage::Cache::open(&fixture.root.join("cache.sqlite3"))
        .unwrap()
        .validate_repository_catalogue_intent_source(&source)
        .unwrap();
    remote::exact(&fixture, &steps);
    remote::preserved(&fixture, &objects);
}
fn wait(background: &mut Background, app: &mut App, ready: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ready(app) {
        background.poll(app);
        assert!(
            Instant::now() < deadline,
            "Actual Background request never completed"
        );
        std::thread::yield_now();
    }
}
#[test]
fn original_saved_navigation_runtime_actual_source_worker_seals_then_admits_exact_workspace() {
    let real = Real::new("sha1");
    let mut app = app(&real);
    let source = app.workspace.as_ref().unwrap().clone();
    let origin = source.actor_origin.as_ref().unwrap().clone();
    let mut background = Background::new();
    assert!(
        background.submit_original_local_v1(&mut app, None, move |owner| {
            let workspace = source.load_original_with_operation_v1(
                RepositoryMode::Tree,
                &source.revision,
                owner.operation(),
            )?;
            Ok(PreparedView {
                workspace: Some(workspace),
                status: "owned original local read".into(),
                ..Default::default()
            })
        })
    );
    wait(&mut background, &mut app, |app| {
        !app.loading[View::Repository.index()]
    });
    assert_eq!(app.status, "owned original local read");
    assert!(Arc::ptr_eq(
        app.workspace
            .as_ref()
            .unwrap()
            .actor_origin
            .as_ref()
            .unwrap(),
        &origin
    ));
    assert_eq!(app.workspace.as_ref().unwrap().mode, RepositoryMode::Tree);
    assert!(app.github.is_none() && app.actions.is_none());
}
#[test]
fn original_saved_navigation_runtime_foreign_private_facets_refused_before_private_effects() {
    let real = Real::new("sha1");
    let mut app = app(&real);
    let source = app.workspace.as_ref().unwrap().clone();
    let mut background = Background::new();
    assert!(
        background.submit_original_local_v1(&mut app, None, move |_owner| {
            Ok(PreparedView {
                workspace: Some(source),
                clear_private: true,
                status: "FORGED specialized success".into(),
                ..Default::default()
            })
        })
    );
    wait(&mut background, &mut app, |app| {
        !app.loading[View::Repository.index()]
    });
    assert!(app.status.contains("foreign prepared kind"));
    assert!(!app.status.contains("FORGED"));
    assert_eq!(app.workspace.as_ref().unwrap().mode, RepositoryMode::Graph);
}
#[test]
fn original_saved_navigation_runtime_retired_source_rejects_real_error_and_caught_panic_before_effects(
) {
    for panic in [false, true] {
        let real = Real::new("sha256");
        let mut app = app(&real);
        let mut background = Background::new();
        let (released, wait_for_release) = mpsc::channel();
        let (started, wait_for_start) = mpsc::channel();
        assert!(
            background.submit_original_local_v1(&mut app, None, move |_owner| {
                started.send(()).unwrap();
                wait_for_release.recv().unwrap();
                if panic {
                    panic!("controlled original local worker panic");
                } else {
                    Err("late original local error".into())
                }
            })
        );
        wait_for_start.recv_timeout(Duration::from_secs(3)).unwrap();
        app.workspace.as_mut().unwrap().actor_origin =
            Some(crate::git::OriginalLocalActorOriginV1::capture(&real.repository).unwrap());
        app.status = "new source owns its own status".into();
        app.loading[View::Repository.index()] = false;
        released.send(()).unwrap();
        let completion = background
            .completions
            .recv_timeout(Duration::from_secs(3))
            .unwrap();
        assert!(completion.result.is_err());
        let (sender, receiver) = mpsc::sync_channel(1);
        sender.send(completion).unwrap();
        background.completions = receiver;
        assert!(!background.poll(&mut app));
        assert_eq!(app.status, "new source owns its own status");
        assert!(!app.loading[View::Repository.index()]);
        assert!(app.github.is_none() && app.actions.is_none());
    }
}
#[test]
fn original_saved_navigation_runtime_actual_cancel_cannot_publish_success_after_owned_worker_cleanup(
) {
    let real = Real::new("sha1");
    let mut app = app(&real);
    let source = app.workspace.as_ref().unwrap().clone();
    let mut background = Background::new();
    let (started, wait_for_start) = mpsc::channel();
    let (release, wait_release) = mpsc::channel();
    assert!(
        background.submit_original_local_v1(&mut app, None, move |owner| {
            started.send(()).unwrap();
            wait_release.recv().unwrap();
            assert!(owner.operation().cancellation().is_cancelled());
            Ok(PreparedView {
                workspace: Some(source),
                status: "late canceled success".into(),
                ..Default::default()
            })
        })
    );
    wait_for_start.recv_timeout(Duration::from_secs(3)).unwrap();
    background.cancel(View::Repository);
    app.dispatch(Action::Cancel);
    app.loading[View::Repository.index()] = false;
    app.status = "current original cancellation".into();
    release.send(()).unwrap();
    let completion = background
        .completions
        .recv_timeout(Duration::from_secs(3))
        .unwrap();
    assert!(completion.result.is_err());
    let (sender, receiver) = mpsc::sync_channel(1);
    sender.send(completion).unwrap();
    background.completions = receiver;
    assert!(!background.poll(&mut app));
    assert_eq!(app.status, "current original cancellation");
    assert_eq!(app.workspace.as_ref().unwrap().mode, RepositoryMode::Graph);
}
