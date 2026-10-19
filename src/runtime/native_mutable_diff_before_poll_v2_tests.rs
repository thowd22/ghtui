use super::*;
use crate::{
    app::{Action, HitRegion},
    git::MutableDiffSourceV2,
    local_commit_actor_origin_v1_test_support::Real,
    repository_native_diff_v1::{NativeDiffOptionChangeV1, NativeDiffStateV1},
    workspace::{RepositoryMode, Workspace},
};
use ratatui::layout::Rect;
use std::time::Duration;
type State = NativeDiffStateV1<MutableDiffSourceV2>;
fn loaded(real: &Real, mode: RepositoryMode) -> Workspace {
    Workspace::load(&real.root, mode, "HEAD", None)
        .unwrap()
        .unwrap()
}

#[test]
fn native_mutable_diff_worker_actual_same_b_owner_sealed_rows_and_mutable_source_identity() {
    for mode in [RepositoryMode::Working, RepositoryMode::Staged] {
        let real = Real::new("sha256");
        let workspace = loaded(&real, mode);
        let mut state = State::open_mutable(&workspace, 118).unwrap();
        let request = state.request().unwrap();
        let owner = OriginalDiffWorkerOwnerV1::begin(
            &workspace,
            None,
            Arc::new(OriginalDiffWorkV1::MutableNative(request)),
            Default::default(),
        )
        .unwrap();
        let before = real.snapshot();
        real.clear_trace();
        let prepared = owner.prepare().unwrap();
        assert!(prepared.matches(&owner));
        let identity = prepared.source_identity_v1().clone();
        assert!(identity.mutable());
        assert_eq!(identity.checkout_head(), Some(real.head.as_str()));
        assert!(
            identity.actor_commit().is_none(),
            "unsaved actors cannot inherit checkout commit identity"
        );
        assert!(identity
            .detail_text()
            .contains(if mode == RepositoryMode::Working {
                "working files vs index; mutable"
            } else {
                "index vs HEAD; mutable"
            }));
        prepared
            .publish_native_mutable_v2(&workspace, &mut state)
            .unwrap();
        assert!(state.current_frame().is_some());
        let accounting = owner.accounting().unwrap();
        assert!(accounting.sealed());
        assert_eq!(
            accounting.children_started(),
            if mode == RepositoryMode::Working {
                22
            } else {
                23
            }
        );
        assert_eq!(accounting.children_started(), real.children().len());
        assert_eq!(accounting.in_flight_requests(), 0);
        assert!(
            workspace.original_comparison_v1.is_none() && workspace.original_diff_side_v1.is_none()
        );
        assert_eq!(real.snapshot(), before);
    }
}
#[test]
fn native_mutable_diff_worker_existing_background_moves_exact_typed_result_without_workspace_clone()
{
    let real = Real::new("sha1");
    let workspace = loaded(&real, RepositoryMode::Working);
    let origin = workspace.actor_origin.as_ref().unwrap().clone();
    let observed = workspace.comparison.as_ref().unwrap().observed;
    let mut state = State::open_mutable(&workspace, 118).unwrap();
    let request = state.request().unwrap();
    let work = Arc::new(OriginalDiffWorkV1::MutableNative(request));
    let mut app = App {
        view: View::Repository,
        workspace: Some(workspace),
        native_mutable_diff_v2: Some(state),
        refresh_requested: false,
        ..Default::default()
    };
    let before = real.snapshot();
    real.clear_trace();
    let mut background = Background::new();
    assert!(background.submit_original_diff_sides_v1(&mut app, work));
    let owner = background
        .current_original_diff_owner_v1
        .as_ref()
        .unwrap()
        .clone();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while app.loading[View::Repository.index()] {
        background.poll(&mut app);
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(app
        .native_mutable_diff_v2
        .as_ref()
        .unwrap()
        .current_frame()
        .is_some());
    assert!(app.native_diff_v1.is_none() && app.original_diff_picker_v1.is_none());
    let workspace = app.workspace.as_ref().unwrap();
    assert!(Arc::ptr_eq(
        workspace.actor_origin.as_ref().unwrap(),
        &origin
    ));
    assert_eq!(workspace.comparison.as_ref().unwrap().observed, observed);
    assert!(app.repository_source_identity.as_ref().unwrap().mutable());
    assert!(background.current_original_diff_owner_v1.is_none());
    assert!(owner.accounting().unwrap().sealed());
    assert_eq!(owner.accounting().unwrap().children_started(), 22);
    assert_eq!(real.snapshot(), before);
}
fn queued(kind: u8) {
    for input in 0..10 {
        let real = Real::new("sha1");
        let workspace = loaded(&real, RepositoryMode::Working);
        let mut state = State::open_mutable(&workspace, 118).unwrap();
        let request = state.request().unwrap();
        let token = CancellationToken::default();
        let owner = OriginalDiffWorkerOwnerV1::begin(
            &workspace,
            None,
            Arc::new(OriginalDiffWorkV1::MutableNative(request)),
            token.clone(),
        )
        .unwrap();
        let mut app = App {
            view: View::Repository,
            workspace: Some(workspace),
            native_mutable_diff_v2: Some(state),
            refresh_requested: false,
            ..Default::default()
        };
        let mut background = Background::new();
        background.generation += 1;
        let generation = background.generation;
        background
            .current
            .insert(View::Repository.index(), (generation, token.clone()));
        background.current_original_diff_owner_v1 = Some(owner.clone());
        background.current_original_diff_generation_v1 = Some(generation);
        let captured = owner.clone();
        background.requests.as_ref().unwrap().try_send(Request{original_stash_recovery_owner_v1:None,view:View::Repository,generation,cancellation:token,catalogue_owner:None,catalogue_recovery:None,remote_scope_owner:None,local_actor_owner:None,remote_repository_owner:None,local_navigation_owner:None,original_tree_owner_v1:None,original_diff_owner_v1:Some(owner.clone()),work:Box::new(move|_|{
            let prepared=captured.prepare()?;
            match kind{0=>Ok(PreparedView{original_diff_result_v1:Some(prepared),..Default::default()}),1=>Err("Actual sealed mutable error".into()),_=>panic!("Controlled genuine mutable panic after actual source preparation and seal")}
        })}).unwrap();
        let completion = background
            .completions
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert_eq!(completion.result.is_ok(), kind == 0);
        assert!(owner.accounting().unwrap().sealed());
        assert_eq!(owner.accounting().unwrap().children_started(), 22);
        match input {
            0 => app
                .native_mutable_diff_v2
                .as_mut()
                .unwrap()
                .query
                .insert("dirty")
                .unwrap(),
            1 => app.native_mutable_diff_v2.as_mut().unwrap().width = 38,
            2 => app
                .native_mutable_diff_v2
                .as_mut()
                .unwrap()
                .option(NativeDiffOptionChangeV1::Layout)
                .unwrap(),
            3 => {
                app.native_mutable_diff_v2
                    .as_mut()
                    .unwrap()
                    .request()
                    .unwrap();
            }
            4 => {
                app.native_mutable_diff_v2 = None;
                app.original_diff_cancel_v1 = true;
            }
            5 => app.repository_load_requested = true,
            6 => app
                .native_mutable_diff_v2
                .as_mut()
                .unwrap()
                .clear_selection(),
            7 => app.related_pr_requested_v1 = true,
            8 => app.original_diff_work_v1 = Some(owner.work().clone()),
            _ => app.cancel_requested = true,
        }
        app.status = "New mutable input owns status".into();
        app.loading[View::Repository.index()] = true;
        app.repository_list.selected = 8;
        app.repository_list.top = 7;
        app.hits = vec![HitRegion {
            rect: Rect::new(2, 3, 4, 1),
            action: Action::Quit,
        }];
        let origin = app
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
        assert_eq!(app.status, "New mutable input owns status");
        assert!(app.loading[View::Repository.index()]);
        assert_eq!(
            (app.repository_list.selected, app.repository_list.top),
            (8, 7)
        );
        assert_eq!(app.hits.len(), 1);
        assert_eq!(app.hits[0].action, Action::Quit);
        assert!(app.repository_source_identity.is_none());
        assert!(app
            .native_mutable_diff_v2
            .as_ref()
            .is_none_or(|state| state.frame().is_none()));
        assert!(Arc::ptr_eq(
            app.workspace
                .as_ref()
                .unwrap()
                .actor_origin
                .as_ref()
                .unwrap(),
            &origin
        ));
        assert!(real.children().is_empty());
        assert_eq!(real.snapshot(), before);
        if input == 4 {
            assert!(background.cancel_current_original_diff_v1());
            assert!(owner.cancellation().is_cancelled());
        }
    }
}
#[test]
fn native_mutable_diff_worker_queued_actual_success_new_query_width_purpose_close_and_cancel_veto_before_effects(
) {
    queued(0);
}
#[test]
fn native_mutable_diff_worker_queued_actual_error_new_query_width_purpose_close_and_cancel_veto_before_effects(
) {
    queued(1);
}
#[test]
fn native_mutable_diff_worker_queued_actual_panic_new_query_width_purpose_close_and_cancel_veto_before_effects(
) {
    queued(2);
}
