use super::*;
use crate::{
    app::{Action, HitRegion},
    git::MutableDiffSourceV2,
    local_commit_actor_origin_v1_test_support::{git, Real},
    providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
    repository_native_diff_v1::{NativeDiffOptionChangeV1, NativeDiffStateV1},
    repository_native_mutable_plan_native_v2::{self as ui, NativeMutablePlanActionV2 as A},
    workspace::{RepositoryMode, Workspace},
};
use std::{
    fs,
    time::{Duration, Instant},
};
#[path = "native_mutable_plan_submit_failure_v2_tests.rs"]
mod submit_failure_v2_tests;
fn state(real: &Real) -> Vec<Vec<u8>> {
    vec![
        git(&real.root, &["show-ref", "--head"], None),
        fs::read(real.root.join(".git/index")).unwrap(),
        git(
            &real.root,
            &[
                "-c",
                "diff.autoRefreshIndex=false",
                "diff",
                "--cached",
                "--binary",
            ],
            None,
        ),
        git(
            &real.root,
            &["-c", "diff.autoRefreshIndex=false", "diff", "--binary"],
            None,
        ),
        fs::read(real.root.join("tracked")).unwrap(),
        fs::read(real.root.join("scratch")).unwrap(),
        fs::read(real.root.join(".git/config")).unwrap(),
    ]
}
fn loaded(real: &Real, mode: RepositoryMode) -> Workspace {
    Workspace::load(&real.root, mode, "HEAD", None)
        .unwrap()
        .unwrap()
}
fn publish_projection(state: &mut NativeDiffStateV1<MutableDiffSourceV2>) {
    let request = state.request().unwrap();
    let op =
        LocalGitNavigationReadV1::begin(request.intent().origin(), Default::default()).unwrap();
    let prepared = request.prepare(&op).unwrap();
    op.seal().unwrap();
    assert!(state.accept(prepared));
}
fn app(real: &Real, mode: RepositoryMode) -> App {
    let workspace = loaded(real, mode);
    let mut state = NativeDiffStateV1::open_mutable(&workspace, 118).unwrap();
    publish_projection(&mut state);
    state.select_hunk(0).unwrap();
    publish_projection(&mut state);
    App {
        view: View::Repository,
        workspace: Some(workspace),
        native_mutable_diff_v2: Some(state),
        refresh_requested: false,
        ..Default::default()
    }
}
fn submit(background: &mut Background, app: &mut App) {
    let work = app.original_diff_work_v1.take().unwrap();
    assert!(background.submit_original_diff_sides_v1(app, work));
    let deadline = Instant::now() + Duration::from_secs(8);
    while app.loading[View::Repository.index()] {
        background.poll(app);
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}
fn paint(app: &mut App) {
    let backend = ratatui::backend::TestBackend::new(120, 40);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            assert!(ui::draw(frame, app, frame.area()));
        })
        .unwrap();
}
#[test]
fn native_mutable_plan_actual_b_queue_preview_no_write_then_explicit_stage_unstage_one_attempt_and_retired_hits(
) {
    for mode in [RepositoryMode::Working, RepositoryMode::Staged] {
        let real = Real::new("sha256");
        let expected_saved = git(&real.root, &["show", "HEAD:tracked"], None);
        let mut app = app(&real, mode);
        let before = state(&real);
        let mut background = Background::new();
        ui::begin(&mut app);
        submit(&mut background, &mut app);
        paint(&mut app);
        assert_eq!(state(&real), before);
        assert!(app.status.contains("no changes until explicit Confirm"));
        ui::dispatch(&mut app, A::Confirm);
        let work = app.original_diff_work_v1.as_ref().unwrap().clone();
        ui::dispatch(&mut app, A::Confirm);
        assert!(
            Arc::ptr_eq(&work, app.original_diff_work_v1.as_ref().unwrap()),
            "second Confirm cannot capture another purpose"
        );
        real.clear_trace();
        submit(&mut background, &mut app);
        let after = state(&real);
        assert_ne!(after[1], before[1]);
        for field in [0, 4, 5, 6] {
            assert_eq!(after[field], before[field]);
        }
        assert_eq!(
            git(&real.root, &["show", ":tracked"], None),
            if mode == RepositoryMode::Working {
                b"dirty bytes\n".as_slice()
            } else {
                expected_saved.as_slice()
            }
        );
        let events = real.children();
        assert_eq!(
            events
                .iter()
                .filter(|event| event["argv"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|a| a.as_str() == Some("apply")))
                .count(),
            1
        );
        let state = app.native_mutable_diff_v2.as_ref().unwrap();
        assert!(
            state.current_frame().is_none() && state.range().is_none() && state.pending().is_none()
        );
        assert!(background.current_original_diff_owner_v1.is_none());
        let count = real.children().len();
        ui::dispatch(&mut app, A::Confirm);
        assert!(app.original_diff_work_v1.is_none());
        assert_eq!(real.children().len(), count);
    }
}
fn veto(kind: u8) {
    for confirmed in [false, true] {
        for input in 0..10 {
            let real = Real::new("sha1");
            let mut app = app(&real, RepositoryMode::Working);
            let mut background = Background::new();
            ui::begin(&mut app);
            if confirmed {
                submit(&mut background, &mut app);
                paint(&mut app);
                ui::dispatch(&mut app, A::Confirm);
            }
            let work = app.original_diff_work_v1.take().unwrap();
            let token = CancellationToken::default();
            let owner = OriginalDiffWorkerOwnerV1::begin(
                app.workspace.as_ref().unwrap(),
                None,
                work,
                token.clone(),
            )
            .unwrap();
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
                        match kind {
                            0 => Ok(PreparedView {
                                original_diff_result_v1: Some(prepared),
                                ..Default::default()
                            }),
                            1 => Err("Actual sealed selected-plan error".into()),
                            _ => panic!(
                                "Controlled selected-plan panic after actual prepare and seal"
                            ),
                        }
                    }),
                })
                .unwrap();
            let completion = background
                .completions
                .recv_timeout(Duration::from_secs(8))
                .unwrap();
            assert_eq!(completion.result.is_ok(), kind == 0);
            assert!(owner.accounting().unwrap().sealed());
            match input {
                0 => app
                    .native_mutable_diff_v2
                    .as_mut()
                    .unwrap()
                    .query
                    .insert("different")
                    .unwrap(),
                1 => app.native_mutable_diff_v2.as_mut().unwrap().width = 38,
                2 => app
                    .native_mutable_diff_v2
                    .as_mut()
                    .unwrap()
                    .option(NativeDiffOptionChangeV1::Layout)
                    .unwrap(),
                3 => app
                    .native_mutable_diff_v2
                    .as_mut()
                    .unwrap()
                    .clear_selection(),
                4 => ui::retire(&mut app),
                5 => app.repository_load_requested = true,
                6 => app.related_pr_requested_v1 = true,
                7 => app.original_diff_work_v1 = Some(owner.work().clone()),
                8 => app.cancel_requested = true,
                _ => app.workspace = Some(loaded(&real, RepositoryMode::Working)),
            }
            app.status = "New selected input owns effects".into();
            app.loading[View::Repository.index()] = true;
            app.repository_list.selected = 8;
            app.repository_list.top = 7;
            app.hits = vec![HitRegion {
                rect: ratatui::layout::Rect::new(2, 3, 4, 1),
                action: Action::Quit,
            }];
            let before = state(&real);
            real.clear_trace();
            let (sender, receiver) = mpsc::sync_channel(1);
            sender.send(completion).unwrap();
            background.completions = receiver;
            assert!(!background.poll(&mut app));
            assert_eq!(app.status, "New selected input owns effects");
            assert!(app.loading[View::Repository.index()]);
            assert_eq!(
                (app.repository_list.selected, app.repository_list.top),
                (8, 7)
            );
            assert_eq!(app.hits.len(), 1);
            assert_eq!(app.hits[0].action, Action::Quit);
            assert!(app.repository_source_identity.is_none());
            assert!(real.children().is_empty());
            assert_eq!(state(&real), before);
            assert!(background.cancel_current_original_diff_v1());
            assert!(owner.cancellation().is_cancelled());
            assert!(background.current_original_diff_owner_v1.is_none());
        }
    }
}
#[test]
fn native_mutable_plan_actual_sealed_preview_and_confirm_success_newer_intent_veto_before_effects()
{
    veto(0);
}
#[test]
fn native_mutable_plan_actual_sealed_preview_and_confirm_error_newer_intent_veto_before_effects() {
    veto(1);
}
#[test]
fn native_mutable_plan_actual_sealed_preview_and_confirm_panic_newer_intent_veto_before_effects() {
    veto(2);
}
