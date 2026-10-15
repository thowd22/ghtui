use super::*;
use crate::{
    app::HitRegion,
    git::Comparison,
    original_diff_sides_v1_test_support as fixture,
    repository_native_diff_v1::{NativeDiffOptionChangeV1, NativeDiffStateV1},
};
use ratatui::layout::Rect;
use std::time::Duration;

#[test]
fn native_diff_worker_genuine_initial_nine_children_same_original_seal_source_and_readonly_bytes() {
    let (real, a, b, c) = fixture::fixture("sha256");
    let workspace = fixture::workspace(
        &real,
        Comparison::BranchChanges {
            base: a.clone(),
            head: b.clone(),
        },
    );
    let mut state = NativeDiffStateV1::open(&workspace, 118).unwrap();
    let request = state.request().unwrap();
    let owner = OriginalDiffWorkerOwnerV1::begin(
        &workspace,
        None,
        Arc::new(OriginalDiffWorkV1::Native(request)),
        Default::default(),
    )
    .unwrap();
    let before = real.snapshot();
    real.clear_trace();
    let prepared = owner.prepare().unwrap();
    assert!(prepared.matches(&owner));
    let accounting = owner.accounting().unwrap();
    assert!(accounting.sealed());
    assert_eq!(
        (
            accounting.children_started(),
            accounting.in_flight_requests()
        ),
        (9, 0)
    );
    prepared.publish_native_v1(&workspace, &mut state).unwrap();
    let frame = state.current_frame().unwrap();
    assert_eq!(frame.source().intent().requested_base(), Some(a.as_str()));
    assert_eq!(frame.source().intent().content_base(), c);
    assert_eq!(frame.source().intent().head(), b);
    assert!(
        workspace.original_comparison_v1.is_none(),
        "Native modal cannot clone or replace underlying Workspace bodies"
    );
    fixture::assert_no_discovery(&real);
    assert_eq!(real.snapshot(), before);
}

#[test]
fn native_diff_worker_existing_background_moves_cached_frame_identity_zero_child_no_legacy_publish()
{
    let (real, a, b, _) = fixture::fixture("sha1");
    let mut workspace = fixture::workspace(&real, Comparison::BranchChanges { base: a, head: b });
    let source = fixture::source(&workspace);
    source.install(&mut workspace).unwrap();
    let mut state = NativeDiffStateV1::open(&workspace, 118).unwrap();
    let request = state.request().unwrap();
    let work = Arc::new(OriginalDiffWorkV1::Native(request));
    let mut app = App {
        view: View::Repository,
        workspace: Some(workspace),
        native_diff_v1: Some(state),
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
    assert!(Arc::ptr_eq(
        app.native_diff_v1
            .as_ref()
            .unwrap()
            .current_frame()
            .unwrap()
            .source(),
        &source
    ));
    assert!(Arc::ptr_eq(
        app.workspace
            .as_ref()
            .unwrap()
            .original_comparison_v1
            .as_ref()
            .unwrap(),
        &source
    ));
    assert!(app.repository_source_identity.is_some());
    assert!(background.current_original_diff_owner_v1.is_none());
    assert!(owner.accounting().unwrap().sealed());
    assert_eq!(owner.accounting().unwrap().children_started(), 0);
    assert!(real.children().is_empty());
    assert_eq!(real.snapshot(), before);
}

fn queued_native(kind: u8, source_only: bool) {
    for input in 0..8 {
        let (real, a, b, _) = fixture::fixture("sha256");
        let mut workspace =
            fixture::workspace(&real, Comparison::BranchChanges { base: a, head: b });
        fixture::source(&workspace).install(&mut workspace).unwrap();
        let mut state = NativeDiffStateV1::open(&workspace, 118).unwrap();
        if source_only {
            state.regex = true;
            state.query.insert("[").unwrap();
        }
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
        background
            .requests
            .as_ref()
            .unwrap()
            .try_send(Request {
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
                    match kind {
                        0 => Ok(PreparedView {
                            original_diff_result_v1: Some(prepared),
                            ..Default::default()
                        }),
                        1 => Err("Actual sealed Native error".into()),
                        _ => panic!(
                            "Controlled genuine Native panic AFTER sealed source preparation"
                        ),
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
        match input {
            0 => app
                .native_diff_v1
                .as_mut()
                .unwrap()
                .query
                .insert("new")
                .unwrap(),
            1 => app.native_diff_v1.as_mut().unwrap().width = 38,
            2 => app
                .native_diff_v1
                .as_mut()
                .unwrap()
                .option(NativeDiffOptionChangeV1::Layout)
                .unwrap(),
            3 => {
                app.native_diff_v1.as_mut().unwrap().request().unwrap();
            }
            4 => {
                app.native_diff_v1 = None;
                app.original_diff_cancel_v1 = true;
            }
            5 => app.repository_load_requested = true,
            6 => {
                app.native_diff_v1.as_mut().unwrap().clear_selection();
            }
            _ => app.cancel_requested = true,
        }
        app.status = "New Native input owns status".into();
        app.loading[View::Repository.index()] = true;
        app.selected = 8;
        app.scroll = 7;
        app.hits = vec![HitRegion {
            rect: Rect::new(2, 3, 4, 1),
            action: crate::app::Action::Quit,
        }];
        let origin = app
            .workspace
            .as_ref()
            .unwrap()
            .actor_origin
            .as_ref()
            .unwrap()
            .clone();
        let source = app
            .workspace
            .as_ref()
            .unwrap()
            .original_comparison_v1
            .as_ref()
            .unwrap()
            .clone();
        let before = real.snapshot();
        real.clear_trace();
        let (sender, receiver) = mpsc::sync_channel(1);
        sender.send(completion).unwrap();
        background.completions = receiver;
        assert!(!background.poll(&mut app));
        assert_eq!(app.status, "New Native input owns status");
        assert!(app.loading[View::Repository.index()]);
        assert_eq!((app.selected, app.scroll), (8, 7));
        assert_eq!(app.hits.len(), 1);
        assert_eq!(app.hits[0].action, crate::app::Action::Quit);
        assert!(app.repository_source_identity.is_none());
        assert!(app
            .native_diff_v1
            .as_ref()
            .is_none_or(|state| state.frame().is_none()));
        assert!(Arc::ptr_eq(
            &origin,
            app.workspace
                .as_ref()
                .unwrap()
                .actor_origin
                .as_ref()
                .unwrap()
        ));
        assert!(Arc::ptr_eq(
            &source,
            app.workspace
                .as_ref()
                .unwrap()
                .original_comparison_v1
                .as_ref()
                .unwrap()
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
fn native_diff_worker_queued_actual_success_new_query_width_purpose_cancel_rejected_before_effects()
{
    queued_native(0, false);
}
#[test]
fn native_diff_worker_queued_actual_sealed_error_new_query_width_purpose_cancel_rejected_before_effects(
) {
    queued_native(1, false);
}
#[test]
fn native_diff_worker_queued_actual_caught_panic_new_query_width_purpose_cancel_rejected_before_effects(
) {
    queued_native(2, false);
}

#[test]
fn native_diff_metadata_worker_queued_source_only_new_query_width_purpose_cancel_rejected_before_effects(
) {
    queued_native(0, true);
}
#[test]
fn native_diff_metadata_worker_queued_sealed_error_new_query_width_purpose_cancel_rejected_before_effects(
) {
    queued_native(1, true);
}
#[test]
fn native_diff_metadata_worker_queued_caught_panic_new_query_width_purpose_cancel_rejected_before_effects(
) {
    queued_native(2, true);
}
