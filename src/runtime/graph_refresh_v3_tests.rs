//! NEW unregistered private runtime child: real source factories/channel
//! barriers, no fake route/proof constructor and no direct completion substitute.
use super::*;
use crate::{
    local_commit_actor_origin_v1_test_support::Real,
    repository_graph_reread_native_v3,
    repository_graph_reread_v3::GraphRereadPreviewV3,
    workspace::{RepositoryMode, Workspace},
};
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};
fn app(real: &Real) -> App {
    App {
        view: View::Repository,
        workspace: Workspace::load(&real.root, RepositoryMode::Graph, &real.original, None)
            .unwrap(),
        refresh_requested: false,
        ..Default::default()
    }
}
fn wait(background: &mut Background, app: &mut App, ready: impl Fn(&App) -> bool) {
    let end = Instant::now() + Duration::from_secs(5);
    while !ready(app) {
        background.poll(app);
        assert!(Instant::now() < end);
        std::thread::yield_now();
    }
}

#[test]
fn commit_graph_v3_runtime_same_source_two_actual_operations_cannot_replay_retained_facet() {
    let real = Real::new("sha256");
    let mut app = app(&real);
    let preview = GraphRereadPreviewV3::capture(
        app.workspace.as_ref().unwrap(),
        &mut app.original_saved_visits,
    )
    .unwrap();
    let first_owner = crate::navigation_local_owner_v1::OriginalLocalNavigationOwnerV1::begin(
        app.workspace.as_ref().unwrap(),
        Default::default(),
    )
    .unwrap();
    let prepared = repository_graph_reread_native_v3::work(&preview, &first_owner).unwrap();
    first_owner.operation().seal().unwrap();
    let before = app
        .original_saved_visits
        .graph_retention_accounting_v2()
        .unwrap()
        .unwrap();
    assert_eq!(before.sources, 2);
    let original = app
        .workspace
        .as_ref()
        .unwrap()
        .graph_state_v1
        .as_ref()
        .unwrap()
        .clone();
    let original_display = app
        .workspace
        .as_ref()
        .unwrap()
        .graph_display_v1
        .as_ref()
        .unwrap()
        .clone();
    let baseline = real.snapshot();
    real.clear_trace();
    let mut background = Background::new();
    let operation_a = first_owner.operation().clone();
    assert!(
        background.submit_original_local_v1(&mut app, None, move |owner| {
            assert!(!Arc::ptr_eq(&operation_a, owner.operation()));
            assert_eq!(
                owner.operation().accounting().unwrap().children_started(),
                0
            );
            Ok(prepared)
        })
    );
    let completion = background
        .completions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    assert_eq!(
        completion
            .local_navigation_owner
            .as_ref()
            .unwrap()
            .operation()
            .accounting()
            .unwrap()
            .children_started(),
        0
    );
    let (tx, rx) = mpsc::channel();
    background.completions = rx;
    let status = app.status.clone();
    let context = app.context.clone();
    let loading = app.loading;
    let selection = app.selected;
    tx.send(completion).unwrap();
    assert!(!background.poll(&mut app));
    assert_eq!(app.status, status);
    assert_eq!(app.context, context);
    assert_eq!(app.loading, loading);
    assert_eq!(app.selected, selection);
    assert!(Arc::ptr_eq(
        &original,
        app.workspace
            .as_ref()
            .unwrap()
            .graph_state_v1
            .as_ref()
            .unwrap()
    ));
    assert!(Arc::ptr_eq(
        &original_display,
        app.workspace
            .as_ref()
            .unwrap()
            .graph_display_v1
            .as_ref()
            .unwrap()
    ));
    assert_eq!(
        app.original_saved_visits
            .graph_retention_accounting_v2()
            .unwrap()
            .unwrap()
            .sources,
        1
    );
    assert!(real.children().is_empty());
    assert_eq!(real.snapshot(), baseline);
}

#[test]
fn commit_graph_v3_runtime_mixed_actual_navigation_restore_refuses_before_retention_or_ui_effects()
{
    use crate::navigation_trail::{NavigationBookmark, NavigationPosition, NavigationScope};
    let real = Real::new("sha1");
    let mut app = app(&real);
    let preview = GraphRereadPreviewV3::capture(
        app.workspace.as_ref().unwrap(),
        &mut app.original_saved_visits,
    )
    .unwrap();
    let original = app
        .workspace
        .as_ref()
        .unwrap()
        .graph_state_v1
        .as_ref()
        .unwrap()
        .clone();
    let mut background = Background::new();
    assert!(
        background.submit_original_local_v1(&mut app, None, move |owner| {
            repository_graph_reread_native_v3::work(&preview, &owner)
        })
    );
    let mut completion = background
        .completions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    assert_eq!(
        app.original_saved_visits
            .graph_retention_accounting_v2()
            .unwrap()
            .unwrap()
            .sources,
        2
    );
    let bookmark = NavigationBookmark::new(
        View::Repository,
        NavigationScope::local(real.root.clone()).unwrap(),
        b"actual other navigation intent".to_vec(),
        "local-saved-source-v1".into(),
        vec![],
        "Other original intent".into(),
        NavigationPosition::default(),
    )
    .unwrap();
    let token = app.navigation_trail.begin(bookmark.clone()).unwrap();
    completion.result.as_mut().unwrap().navigation_restore =
        Some(crate::navigation_state::NavigationRestore {
            graph_position_v2: None,
            token,
            bookmark,
            original_local: None,
        });
    assert!(!local_saved_navigation_v1::original_local_prepared_only_v1(
        completion.result.as_ref().unwrap()
    ));
    let status = app.status.clone();
    let loading = app.loading;
    let (tx, rx) = mpsc::channel();
    background.completions = rx;
    real.clear_trace();
    tx.send(completion).unwrap();
    assert!(!background.poll(&mut app));
    assert_eq!(app.status, status);
    assert_eq!(app.loading, loading);
    assert!(app.navigation_trail.is_pending());
    assert!(Arc::ptr_eq(
        &original,
        app.workspace
            .as_ref()
            .unwrap()
            .graph_state_v1
            .as_ref()
            .unwrap()
    ));
    assert_eq!(
        app.original_saved_visits
            .graph_retention_accounting_v2()
            .unwrap()
            .unwrap()
            .sources,
        1
    );
    assert!(real.children().is_empty());
}
#[test]
fn commit_graph_v3_runtime_actual_reread_closes_original_window_then_admits_same_ledger_visit() {
    let real = Real::new("sha1");
    let mut app = app(&real);
    let preview = GraphRereadPreviewV3::capture(
        app.workspace.as_ref().unwrap(),
        &mut app.original_saved_visits,
    )
    .unwrap();
    let original = app
        .workspace
        .as_ref()
        .unwrap()
        .graph_state_v1
        .as_ref()
        .unwrap()
        .snapshot_v2();
    let baseline = real.snapshot();
    let mut background = Background::new();
    assert!(
        background.submit_original_local_v1(&mut app, None, move |owner| {
            repository_graph_reread_native_v3::work(&preview, &owner)
        })
    );
    wait(&mut background, &mut app, |app| {
        !app.loading[View::Repository.index()]
    });
    assert!(app.status.starts_with("Original Graph re-read"));
    let current = app.workspace.as_ref().unwrap();
    assert!(!original.same_source(&current.graph_state_v1.as_ref().unwrap().snapshot_v2()));
    assert!(app.original_saved_visits.graph_visit_v2(current).is_some());
    assert_eq!(
        app.original_saved_visits
            .graph_retention_accounting_v2()
            .unwrap()
            .unwrap()
            .sources,
        2
    );
    assert_eq!(real.snapshot(), baseline);
}
#[test]
fn commit_graph_v3_runtime_late_actual_tentative_lease_drops_before_new_owner_status_or_private_effects(
) {
    let real = Real::new("sha1");
    let mut app = app(&real);
    let preview = GraphRereadPreviewV3::capture(
        app.workspace.as_ref().unwrap(),
        &mut app.original_saved_visits,
    )
    .unwrap();
    let before = app
        .original_saved_visits
        .graph_retention_accounting_v2()
        .unwrap()
        .unwrap();
    let (ready_tx, ready_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let mut background = Background::new();
    assert!(
        background.submit_original_local_v1(&mut app, None, move |owner| {
            let prepared = repository_graph_reread_native_v3::work(&preview, &owner)?;
            ready_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            done_tx.send(()).unwrap();
            Ok(prepared)
        })
    );
    ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(
        app.original_saved_visits
            .graph_retention_accounting_v2()
            .unwrap()
            .unwrap()
            .sources,
        before.sources + 1
    );
    app.workspace =
        Workspace::load(&real.root, RepositoryMode::Graph, &real.original, None).unwrap();
    let actual_new = app
        .workspace
        .as_ref()
        .unwrap()
        .graph_state_v1
        .as_ref()
        .unwrap()
        .clone();
    app.status = "Genuine newer owner remains".into();
    app.context = "Newer context".into();
    app.loading[View::Repository.index()] = false;
    release_tx.send(()).unwrap();
    done_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    wait(&mut background, &mut app, |app| {
        app.original_saved_visits
            .graph_retention_accounting_v2()
            .unwrap()
            .unwrap()
            .sources
            == before.sources
    });
    assert_eq!(app.status, "Genuine newer owner remains");
    assert_eq!(app.context, "Newer context");
    assert!(Arc::ptr_eq(
        app.workspace
            .as_ref()
            .unwrap()
            .graph_state_v1
            .as_ref()
            .unwrap(),
        &actual_new
    ));
}
#[test]
fn commit_graph_v3_runtime_foreign_generic_prepared_facet_cannot_insert_or_clear_private_state() {
    let real = Real::new("sha256");
    let mut app = app(&real);
    let preview = GraphRereadPreviewV3::capture(
        app.workspace.as_ref().unwrap(),
        &mut app.original_saved_visits,
    )
    .unwrap();
    let owner = crate::navigation_local_owner_v1::OriginalLocalNavigationOwnerV1::begin(
        app.workspace.as_ref().unwrap(),
        Default::default(),
    )
    .unwrap();
    let mut prepared = repository_graph_reread_native_v3::work(&preview, &owner).unwrap();
    owner.operation().seal().unwrap();
    prepared.status = "FOREIGN_FACET".into();
    prepared.clear_private = true;
    assert!(!local_saved_navigation_v1::original_local_prepared_only_v1(
        &prepared
    ));
    assert!(!original_tree_background_v1::original_tree_prepared_only_v1(&prepared));
    let original = app
        .workspace
        .as_ref()
        .unwrap()
        .graph_state_v1
        .as_ref()
        .unwrap()
        .clone();
    let mut background = Background::new();
    assert!(background.submit(&mut app, View::Repository, move |_| Ok(prepared)));
    let end = Instant::now() + Duration::from_secs(5);
    while background.current.contains_key(&View::Repository.index()) {
        background.poll(&mut app);
        assert!(Instant::now() < end);
        std::thread::yield_now();
    }
    assert!(!app.status.contains("FOREIGN_FACET"));
    assert!(Arc::ptr_eq(
        app.workspace
            .as_ref()
            .unwrap()
            .graph_state_v1
            .as_ref()
            .unwrap(),
        &original
    ));
    assert_eq!(
        app.original_saved_visits
            .graph_retention_accounting_v2()
            .unwrap()
            .unwrap()
            .sources,
        1
    );
}

#[test]
fn commit_graph_v3_runtime_cancel_after_tentative_actual_admission_releases_lease_no_late_publication(
) {
    let real = Real::new("sha1");
    let mut app = app(&real);
    let preview = GraphRereadPreviewV3::capture(
        app.workspace.as_ref().unwrap(),
        &mut app.original_saved_visits,
    )
    .unwrap();
    let original = app
        .workspace
        .as_ref()
        .unwrap()
        .graph_state_v1
        .as_ref()
        .unwrap()
        .clone();
    let before = app
        .original_saved_visits
        .graph_retention_accounting_v2()
        .unwrap()
        .unwrap();
    let (ready_tx, ready_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let mut background = Background::new();
    assert!(
        background.submit_original_local_v1(&mut app, None, move |owner| {
            let prepared = repository_graph_reread_native_v3::work(&preview, &owner)?;
            ready_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            done_tx.send(()).unwrap();
            Ok(prepared) // Actual enclosing original worker seals with canceled token.
        })
    );
    ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(
        app.original_saved_visits
            .graph_retention_accounting_v2()
            .unwrap()
            .unwrap()
            .sources,
        before.sources + 1
    );
    background.cancel(View::Repository);
    app.loading[View::Repository.index()] = false;
    app.status = "Explicit original cancellation".into();
    release_tx.send(()).unwrap();
    done_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    wait(&mut background, &mut app, |app| {
        app.original_saved_visits
            .graph_retention_accounting_v2()
            .unwrap()
            .unwrap()
            .sources
            == before.sources
    });
    assert_eq!(app.status, "Explicit original cancellation");
    assert!(Arc::ptr_eq(
        app.workspace
            .as_ref()
            .unwrap()
            .graph_state_v1
            .as_ref()
            .unwrap(),
        &original
    ));
}
