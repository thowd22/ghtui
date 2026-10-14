use super::*;
use crate::{
    app::{Action, HitRegion},
    git::OriginalTreeIntentV1,
    local_commit_actor_origin_v1_test_support::Real,
    repository_tree_jump_v1::OriginalTreeJumpV1,
    workspace::{RepositoryMode, RowAction, Workspace},
};
use ratatui::layout::Rect;
use std::time::Duration;

fn completion_before_new_intent(kind: u8) {
    for input in 0..6 {
        let real = Real::new("sha256");
        let workspace = Workspace::load(&real.root, RepositoryMode::Tree, &real.original, None)
            .unwrap()
            .unwrap();
        let mut jump = OriginalTreeJumpV1::new(OriginalTreeIntentV1::capture(&workspace).unwrap());
        let work = Arc::new(OriginalTreeWorkV1::Query(jump.request().unwrap()));
        let cancellation = CancellationToken::default();
        let owner = OriginalTreeWorkerOwnerV1::begin(
            &workspace,
            Some(&jump),
            work.clone(),
            cancellation.clone(),
        )
        .unwrap();
        let original_request = jump.pending().unwrap().clone();
        let mut app = App {
            view: View::Repository,
            workspace: Some(workspace),
            original_tree_jump_v1: Some(jump),
            refresh_requested: false,
            ..Default::default()
        };
        let mut background = Background::new();
        background.generation += 1;
        let generation = background.generation;
        background
            .current
            .insert(View::Repository.index(), (generation, cancellation.clone()));
        let captured = owner.clone();
        background
            .requests
            .as_ref()
            .unwrap()
            .try_send(Request {
                view: View::Repository,
                generation,
                cancellation,
                catalogue_owner: None,
                catalogue_recovery: None,
                remote_scope_owner: None,
                local_actor_owner: None,
                remote_repository_owner: None,
                local_navigation_owner: None,
                original_tree_owner_v1: Some(owner.clone()),
                work: Box::new(move |_| {
                    let prepared = captured.prepare()?;
                    match kind {
                        0 => Ok(PreparedView {
                            original_tree_result_v1: Some(prepared),
                            ..Default::default()
                        }),
                        1 => Err("actual owned sealed failure".into()),
                        _ => panic!("controlled actual Tree worker panic after sealed source read"),
                    }
                }),
            })
            .unwrap();
        let completion = background
            .completions
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert_eq!(completion.result.is_ok(), kind == 0);
        assert!(owner.accounting().unwrap().sealed());
        assert_eq!(owner.accounting().unwrap().children_started(), 1);
        match input {
            0 => app.repository_load_requested = true,
            1 => app.repository_action = Some(RowAction::Actor(crate::identity::GitRole::Author)),
            2 => app.original_tree_work_v1 = Some(work.clone()),
            3 => app.original_tree_cancel_v1 = true,
            4 => app.cancel_requested = true,
            _ => app.refresh_requested = true,
        }
        app.status = "new input owns status".into();
        app.loading[View::Repository.index()] = true;
        app.selected = 7;
        app.scroll = 3;
        app.hits = vec![HitRegion {
            rect: Rect::new(4, 5, 6, 1),
            action: Action::RepositoryMode(RepositoryMode::Working),
        }];
        let old_origin = app
            .workspace
            .as_ref()
            .unwrap()
            .actor_origin
            .as_ref()
            .unwrap()
            .clone();
        let before = real.snapshot();
        real.clear_trace();
        let (sender, receiver) = mpsc::sync_channel(1);
        sender.send(completion).unwrap();
        background.completions = receiver;
        assert!(!background.poll(&mut app));
        assert_eq!(app.status, "new input owns status");
        assert!(app.loading[View::Repository.index()]);
        assert_eq!((app.selected, app.scroll), (7, 3));
        assert_eq!(app.hits.len(), 1);
        assert_eq!(
            app.hits[0].action,
            Action::RepositoryMode(RepositoryMode::Working)
        );
        assert!(Arc::ptr_eq(
            &old_origin,
            app.workspace
                .as_ref()
                .unwrap()
                .actor_origin
                .as_ref()
                .unwrap()
        ));
        assert!(Arc::ptr_eq(
            &original_request,
            app.original_tree_jump_v1
                .as_ref()
                .unwrap()
                .pending()
                .unwrap()
        ));
        assert!(real.children().is_empty());
        assert_eq!(real.snapshot(), before);
    }
}

#[test]
fn original_tree_viewport_before_poll_new_input_rejects_actual_completed_success_before_effects() {
    completion_before_new_intent(0);
}
#[test]
fn original_tree_viewport_before_poll_new_input_rejects_actual_sealed_error_before_effects() {
    completion_before_new_intent(1);
}
#[test]
fn original_tree_viewport_before_poll_new_input_rejects_actual_caught_panic_before_effects() {
    completion_before_new_intent(2);
}

#[test]
fn original_tree_viewport_actual_member_completion_width_and_field_are_owned_at_input_before_draw()
{
    use crate::repository_tree_viewport_v2::{TreeDetailFieldV2, TreeViewportActionV2};
    for change in 0..2 {
        for kind in 0..3 {
            let (real, workspace, jump) = crate::repository_tree_viewport_v2_test_support::ready(
                "sha256",
                &[b"actual-width-source.rs"],
            );
            let mut app = App {
                view: View::Repository,
                workspace: Some(workspace),
                original_tree_jump_v1: Some(jump),
                refresh_requested: false,
                ..Default::default()
            };
            crate::repository_tree_viewport_native_v2::queue_member(&mut app);
            let work = app.original_tree_work_v1.take().unwrap();
            let cancellation = CancellationToken::default();
            let owner = OriginalTreeWorkerOwnerV1::begin(
                app.workspace.as_ref().unwrap(),
                app.original_tree_jump_v1.as_ref(),
                work.clone(),
                cancellation.clone(),
            )
            .unwrap();
            let mut background = Background::new();
            background.generation += 1;
            background.current.insert(
                View::Repository.index(),
                (background.generation, cancellation.clone()),
            );
            background.current_original_tree_owner_v1 = Some(owner.clone());
            let captured = owner.clone();
            background
                .requests
                .as_ref()
                .unwrap()
                .try_send(Request {
                    view: View::Repository,
                    generation: background.generation,
                    cancellation,
                    catalogue_owner: None,
                    catalogue_recovery: None,
                    remote_scope_owner: None,
                    local_actor_owner: None,
                    remote_repository_owner: None,
                    local_navigation_owner: None,
                    original_tree_owner_v1: Some(owner.clone()),
                    work: Box::new(move |_| {
                        let result = captured.prepare()?;
                        match kind {
                            0 => Ok(PreparedView {
                                original_tree_result_v1: Some(result),
                                ..Default::default()
                            }),
                            1 => Err("actual sealed metadata failure".into()),
                            _ => panic!("controlled genuine sealed metadata panic"),
                        }
                    }),
                })
                .unwrap();
            let completion = background
                .completions
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
            assert_eq!(completion.result.is_ok(), kind == 0);
            assert!(owner.accounting().unwrap().sealed());
            assert_eq!(owner.accounting().unwrap().children_started(), 0);
            if change == 0 {
                app.handle_event(crossterm::event::Event::Resize(40, 24));
            } else {
                crate::repository_tree_viewport_native_v2::dispatch(
                    &mut app,
                    TreeViewportActionV2::DetailField(TreeDetailFieldV2::Saved),
                );
            }
            // Main has consumed the newer flag but has not canceled/queued it or drawn.
            let desired = app.original_tree_work_v1.take().unwrap();
            assert!(crate::repository_tree_viewport_native_v2::work_current(
                &app, &desired
            ));
            assert!(!crate::repository_tree_viewport_native_v2::work_current(
                &app, &work
            ));
            assert!(owner.can_admit(app.workspace.as_ref().unwrap(),app.original_tree_jump_v1.as_ref(),completion.result.is_ok()),"Only desired metadata epoch/purpose, not a fake source change, must reject this result");
            app.status = "new desired metadata input owns status".into();
            app.selected = 7;
            app.scroll = 3;
            app.hits = vec![HitRegion {
                rect: Rect::new(2, 3, 4, 1),
                action: Action::Help,
            }];
            let before = real.snapshot();
            real.clear_trace();
            let (sender, receiver) = mpsc::sync_channel(1);
            sender.send(completion).unwrap();
            background.completions = receiver;
            assert!(!background.poll(&mut app));
            assert_eq!(app.status, "new desired metadata input owns status");
            assert_eq!((app.selected, app.scroll), (7, 3));
            assert_eq!(app.hits[0].action, Action::Help);
            assert!(Arc::ptr_eq(
                app.original_tree_viewport_v2.pending.as_ref().unwrap(),
                &desired
            ));
            assert!(app
                .original_tree_viewport_v2
                .cache
                .ready(
                    app.original_tree_jump_v1.as_ref().unwrap(),
                    crate::repository_tree_viewport_native_v2::member_width(
                        app.original_tree_viewport_v2.width
                    ),
                    app.original_tree_viewport_v2.field
                )
                .is_none());
            assert_eq!(real.snapshot(), before);
            assert!(real.children().is_empty());
        }
    }
}

#[test]
fn original_tree_viewport_closed_folder_same_retained_source_cannot_revive_exact_old_purpose() {
    use crate::repository_tree_viewport_v2::{TreeFolderIntentV2, TreeViewportActionV2};
    let (real, workspace, jump, projection) =
        crate::repository_tree_viewport_v2_test_support::deep("sha256");
    let intent =
        TreeFolderIntentV2::capture(projection.clone(), projection.breadcrumbs().len() - 1)
            .unwrap();
    let mut app = App {
        view: View::Repository,
        workspace: Some(workspace),
        original_tree_jump_v1: Some(jump),
        refresh_requested: false,
        ..Default::default()
    };
    crate::repository_tree_viewport_native_v2::dispatch(
        &mut app,
        TreeViewportActionV2::InspectFolder(intent.clone()),
    );
    let work = app.original_tree_work_v1.take().unwrap();
    let owner = OriginalTreeWorkerOwnerV1::begin(
        app.workspace.as_ref().unwrap(),
        app.original_tree_jump_v1.as_ref(),
        work.clone(),
        Default::default(),
    )
    .unwrap();
    let result = owner.prepare().unwrap();
    assert_eq!(owner.accounting().unwrap().children_started(), 0);
    crate::repository_tree_viewport_native_v2::dispatch(
        &mut app,
        TreeViewportActionV2::CloseFolder,
    );
    assert!(Arc::ptr_eq(
        &projection,
        app.workspace
            .as_ref()
            .unwrap()
            .tree_navigation_v1
            .as_ref()
            .unwrap()
    ));
    assert!(intent.is_current(app.workspace.as_ref().unwrap()));
    assert!(owner.can_admit(
        app.workspace.as_ref().unwrap(),
        app.original_tree_jump_v1.as_ref(),
        true
    ));
    assert!(!crate::repository_tree_viewport_native_v2::work_current(
        &app, &work
    ));
    let before = real.snapshot();
    real.clear_trace();
    app.status = "Closed purpose stays closed".into();
    app.hits = vec![HitRegion {
        rect: Rect::new(2, 3, 4, 1),
        action: Action::Help,
    }];
    assert!(!publish_original_tree_v1(
        &mut app,
        &owner,
        Ok(PreparedView {
            original_tree_result_v1: Some(result),
            ..Default::default()
        })
    ));
    assert!(
        app.original_tree_viewport_v2.folder_intent.is_none()
            && app.original_tree_viewport_v2.folder.is_none()
    );
    assert_eq!(app.status, "Closed purpose stays closed");
    assert_eq!(app.hits[0].action, Action::Help);
    assert!(real.children().is_empty());
    assert_eq!(real.snapshot(), before);
}

#[test]
fn original_tree_viewport_retired_metadata_cancel_never_cancels_unrelated_repository_read() {
    let (real, workspace, jump) =
        crate::repository_tree_viewport_v2_test_support::ready("sha1", &[b"actual.rs"]);
    let mut app = App {
        view: View::Repository,
        workspace: Some(workspace),
        original_tree_jump_v1: Some(jump),
        refresh_requested: false,
        ..Default::default()
    };
    crate::repository_tree_viewport_native_v2::queue_member(&mut app);
    let work = app.original_tree_work_v1.take().unwrap();
    crate::repository_tree_viewport_native_v2::retire(&mut app);
    let cancellation = CancellationToken::default();
    let mut background = Background::new();
    background
        .current
        .insert(View::Repository.index(), (42, cancellation.clone()));
    assert!(!background.cancel_original_tree_metadata_v2(&work));
    assert!(!background.cancel_current_original_tree_v2());
    assert!(!cancellation.is_cancelled());
    assert_eq!(background.current[&View::Repository.index()].0, 42);
    assert!(real.children().is_empty());
}
