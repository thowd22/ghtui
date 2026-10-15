//! Actual same B queue/seal/publisher: stale full-side success/error/panic has no
//! app, loading, geometry, storage or source effects before rejection.
use super::*;
use crate::repository_view::DiffSide;

#[test]
fn native_full_side_queued_actual_success_newer_scope_query_width_back_close_veto_before_effects() {
    queued(0);
}
#[test]
fn native_full_side_queued_actual_sealed_error_newer_scope_query_width_back_close_veto_before_effects(
) {
    queued(1);
}
#[test]
fn native_full_side_queued_actual_caught_panic_newer_scope_query_width_back_close_veto_before_effects(
) {
    queued(2);
}
fn queued(outcome: u8) {
    for input in 0..9 {
        let (real, a, b, _) = fixture::fixture("sha256");
        let mut workspace =
            fixture::workspace(&real, Comparison::BranchChanges { base: a, head: b });
        fixture::source(&workspace).install(&mut workspace).unwrap();
        let original = workspace.original_comparison_v1.as_ref().unwrap().clone();
        let before = real.snapshot();
        let mut state = NativeDiffStateV1::open(&workspace, 118).unwrap();
        let initial = state.request().unwrap();
        let operation =
            crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1::begin(
                initial.intent().origin(),
                Default::default(),
            )
            .unwrap();
        let prepared = initial.prepare(&operation).unwrap();
        operation.seal().unwrap();
        assert!(state.accept(prepared));
        state.open_full_side(DiffSide::New).unwrap();
        let request = state.request().unwrap();
        let token = CancellationToken::default();
        let owner = OriginalDiffWorkerOwnerV1::begin(
            &workspace,
            None,
            Arc::new(OriginalDiffWorkV1::Native(request)),
            token.clone(),
        )
        .unwrap();
        let mut app = App {
            view: View::Repository,
            workspace: Some(workspace),
            native_diff_v1: Some(state),
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
        real.clear_trace();
        background
            .requests
            .as_ref()
            .unwrap()
            .try_send(Request {
                original_stash_recovery_owner_v1: None,
                view: View::Repository,
                generation,
                cancellation: token,
                catalogue_owner: None,
                catalogue_recovery: None,
                remote_scope_owner: None,
                local_actor_owner: None,
                remote_repository_owner: None,
                local_navigation_owner: None,
                original_tree_owner_v1: None,
                original_diff_owner_v1: Some(owner.clone()),
                work: Box::new(move |_| {
                    let prepared = captured.prepare()?;
                    match outcome {
                        0 => Ok(PreparedView {
                            original_diff_result_v1: Some(prepared),
                            ..Default::default()
                        }),
                        1 => Err("Actual sealed full-side error".into()),
                        _ => panic!(
                            "Controlled actual full-side panic AFTER genuine sealed material"
                        ),
                    }
                }),
            })
            .unwrap();
        let completion = background
            .completions
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert_eq!(completion.result.is_ok(), outcome == 0);
        let accounting = owner.accounting().unwrap();
        assert!(accounting.sealed());
        assert!(accounting.children_started() > 0);
        assert_eq!(accounting.children_started(), real.children().len());
        match input {
            0 => app
                .native_diff_v1
                .as_mut()
                .unwrap()
                .query
                .insert("NEW")
                .unwrap(),
            1 => app.native_diff_v1.as_mut().unwrap().width = 38,
            2 => {
                assert!(app.native_diff_v1.as_mut().unwrap().back_to_patch());
            }
            3 => app.native_diff_v1.as_mut().unwrap().file = 1,
            4 => {
                app.native_diff_v1 = None;
                app.original_diff_cancel_v1 = true;
            }
            5 => app.repository_load_requested = true,
            6 => {
                app.native_diff_v1.as_mut().unwrap().request().unwrap();
            }
            7 => app.cancel_requested = true,
            _ => app
                .native_diff_v1
                .as_mut()
                .unwrap()
                .open_full_side(DiffSide::Old)
                .unwrap(),
        }
        app.status = "New full-side input owns status".into();
        app.loading[View::Repository.index()] = true;
        app.selected = 8;
        app.scroll = 7;
        app.hits = vec![HitRegion {
            rect: Rect::new(2, 3, 4, 1),
            action: crate::app::Action::Quit,
        }];
        let (sender, receiver) = mpsc::sync_channel(1);
        sender.send(completion).unwrap();
        background.completions = receiver;
        assert!(!background.poll(&mut app));
        assert_eq!(app.status, "New full-side input owns status");
        assert!(app.loading[View::Repository.index()]);
        assert_eq!((app.selected, app.scroll), (8, 7));
        assert_eq!(app.hits.len(), 1);
        assert_eq!(app.hits[0].action, crate::app::Action::Quit);
        assert!(app.repository_source_identity.is_none());
        assert!(app
            .native_diff_v1
            .as_ref()
            .is_none_or(|state| state.current_full_side().is_none()));
        assert!(Arc::ptr_eq(
            &original,
            app.workspace
                .as_ref()
                .unwrap()
                .original_comparison_v1
                .as_ref()
                .unwrap()
        ));
        assert_eq!(real.snapshot(), before);
        if input == 4 {
            assert!(background.cancel_current_original_diff_v1());
            assert!(owner.cancellation().is_cancelled());
        }
    }
}
