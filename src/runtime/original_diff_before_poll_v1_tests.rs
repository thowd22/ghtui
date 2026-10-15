use super::*;
use crate::{
    app::{Action, HitRegion},
    git::{Comparison, OriginalComparisonIntentV1},
    original_diff_sides_v1_test_support as fixture,
    repository_diff_sides_v1::OriginalDiffPickerV1,
    repository_diff_sides_worker_v1::{OriginalDiffWorkV1, OriginalDiffWorkerOwnerV1},
    workspace::{RepositoryMode, RowAction},
};
use ratatui::layout::Rect;
use std::time::Duration;

fn completion_before_new_input(kind: u8) {
    for input in 0..12 {
        let (real, a, b, _) = fixture::fixture("sha256");
        let workspace = fixture::workspace(&real, Comparison::BranchChanges { base: a, head: b });
        let mut picker =
            OriginalDiffPickerV1::new(OriginalComparisonIntentV1::capture(&workspace).unwrap());
        let request = picker.request().unwrap();
        let work = Arc::new(OriginalDiffWorkV1::Query(request.clone()));
        let cancellation = CancellationToken::default();
        let owner = OriginalDiffWorkerOwnerV1::begin(
            &workspace,
            Some(&picker),
            work.clone(),
            cancellation.clone(),
        )
        .unwrap();
        let mut app = App {
            view: View::Repository,
            workspace: Some(workspace),
            original_diff_picker_v1: Some(picker),
            refresh_requested: false,
            ..Default::default()
        };
        let mut background = Background::new();
        background.generation += 1;
        let generation = background.generation;
        background
            .current
            .insert(View::Repository.index(), (generation, cancellation.clone()));
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
                cancellation,
                catalogue_owner: None,
                catalogue_recovery: None,
                remote_scope_owner: None,
                local_actor_owner: None,
                remote_repository_owner: None,
                local_navigation_owner: None,
                original_tree_owner_v1: None,
                original_diff_owner_v1: Some(owner.clone()),
                work: Box::new(move |_| {
                    let result = captured.prepare()?;
                    match kind {
                        0 => Ok(PreparedView {
                            original_diff_result_v1: Some(result),
                            ..Default::default()
                        }),
                        1 => Err("actual sealed original Diff failure".into()),
                        _ => panic!("controlled actual Diff worker panic after sealed source read"),
                    }
                }),
            })
            .unwrap();
        let completion = background
            .completions
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert_eq!(completion.result.is_ok(), kind == 0);
        let accounting = owner.accounting().unwrap();
        assert!(accounting.sealed());
        assert_eq!(accounting.children_started(), 9);
        assert_eq!(accounting.in_flight_requests(), 0);
        match input {
            0 => app.repository_load_requested = true,
            1 => app.repository_action = Some(RowAction::Actor(crate::identity::GitRole::Author)),
            2 => app.original_diff_work_v1 = Some(work.clone()),
            3 => app.original_diff_cancel_v1 = true,
            4 => app.original_tree_cancel_v1 = true,
            5 => app.cancel_requested = true,
            6 => app.refresh_requested = true,
            7 => {
                let tree = crate::git::OriginalTreeIntentV1::capture(
                    &crate::workspace::Workspace::load(
                        &real.root,
                        RepositoryMode::Tree,
                        &real.original,
                        None,
                    )
                    .unwrap()
                    .unwrap(),
                )
                .unwrap();
                let mut jump = crate::repository_tree_jump_v1::OriginalTreeJumpV1::new(tree);
                app.original_tree_work_v1 = Some(Arc::new(
                    crate::repository_tree_worker_v1::OriginalTreeWorkV1::Query(
                        jump.request().unwrap(),
                    ),
                ));
            }
            8 => {
                background.current_original_diff_owner_v1 = Some(
                    OriginalDiffWorkerOwnerV1::begin(
                        app.workspace.as_ref().unwrap(),
                        app.original_diff_picker_v1.as_ref(),
                        work.clone(),
                        Default::default(),
                    )
                    .unwrap(),
                )
            }
            9 => background.current_original_diff_generation_v1 = Some(generation + 1),
            10 => app.related_pr_requested_v1 = true,
            _ => app.related_pr_cancel_v1 = true,
        }
        app.status = "new input owns status".into();
        app.loading[View::Repository.index()] = true;
        app.selected = 7;
        app.scroll = 3;
        app.hits = vec![HitRegion {
            rect: Rect::new(4, 5, 6, 1),
            action: Action::RepositoryMode(RepositoryMode::Working),
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
        assert_eq!(app.status, "new input owns status");
        assert!(app.loading[View::Repository.index()]);
        assert_eq!((app.selected, app.scroll), (7, 3));
        assert_eq!(app.hits.len(), 1);
        assert_eq!(
            app.hits[0].action,
            Action::RepositoryMode(RepositoryMode::Working)
        );
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
            &request,
            app.original_diff_picker_v1
                .as_ref()
                .unwrap()
                .pending()
                .unwrap()
        ));
        assert!(app
            .workspace
            .as_ref()
            .unwrap()
            .original_comparison_v1
            .is_none());
        assert!(real.children().is_empty());
        assert_eq!(real.snapshot(), before);
        if matches!(input, 8 | 9) {
            assert_eq!(
                background.current.get(&View::Repository.index()).unwrap().0,
                generation,
                "A stale logical owner must not consume the actual shared physical slot"
            );
        }
        if matches!(input, 3 | 10 | 11) {
            assert!(background.cancel_current_original_diff_v1(),"The actual completed-but-vetoed original Diff still owns logical loading until its Close is consumed");
            assert!(owner.cancellation().is_cancelled());
            assert!(background.current_original_diff_owner_v1.is_none());
        }
    }
}

#[test]
fn original_diff_sides_before_poll_new_input_rejects_actual_completed_success_before_effects() {
    completion_before_new_input(0);
}
#[test]
fn original_diff_sides_before_poll_new_input_rejects_actual_sealed_error_before_effects() {
    completion_before_new_input(1);
}
#[test]
fn original_diff_sides_before_poll_new_input_rejects_actual_caught_panic_before_effects() {
    completion_before_new_input(2);
}

fn mixed_result(has_owner: bool) {
    let (real, a, b, _) = fixture::fixture("sha1");
    let workspace = fixture::workspace(&real, Comparison::BranchChanges { base: a, head: b });
    let mut picker =
        OriginalDiffPickerV1::new(OriginalComparisonIntentV1::capture(&workspace).unwrap());
    let cancellation = CancellationToken::default();
    let request = picker.request().unwrap();
    let owner = OriginalDiffWorkerOwnerV1::begin(
        &workspace,
        Some(&picker),
        Arc::new(OriginalDiffWorkV1::Query(request)),
        cancellation.clone(),
    )
    .unwrap();
    let mut app = App {
        view: View::Repository,
        workspace: Some(workspace),
        original_diff_picker_v1: Some(picker),
        refresh_requested: false,
        ..Default::default()
    };
    let mut background = Background::new();
    background.generation += 1;
    background.current.insert(
        View::Repository.index(),
        (background.generation, cancellation.clone()),
    );
    background.current_original_diff_owner_v1 = Some(owner.clone());
    background.current_original_diff_generation_v1 = Some(background.generation);
    let captured = owner.clone();
    background
        .requests
        .as_ref()
        .unwrap()
        .try_send(Request {
            original_stash_recovery_owner_v1: None,
            view: View::Repository,
            generation: background.generation,
            cancellation,
            catalogue_owner: None,
            catalogue_recovery: None,
            remote_scope_owner: None,
            local_actor_owner: None,
            remote_repository_owner: None,
            local_navigation_owner: None,
            original_tree_owner_v1: None,
            original_diff_owner_v1: has_owner.then_some(owner),
            work: Box::new(move |_| {
                Ok(PreparedView {
                    original_diff_result_v1: Some(captured.prepare()?),
                    clear_private: true,
                    status: "foreign status must not publish".into(),
                    ..Default::default()
                })
            }),
        })
        .unwrap();
    let completion = background
        .completions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    assert!(completion.result.is_ok());
    app.status = "current status".into();
    app.loading[View::Repository.index()] = true;
    let before = real.snapshot();
    real.clear_trace();
    let (sender, receiver) = mpsc::sync_channel(1);
    sender.send(completion).unwrap();
    background.completions = receiver;
    assert!(!background.poll(&mut app));
    assert_eq!(app.status, "current status");
    assert!(app.loading[View::Repository.index()]);
    assert!(app
        .workspace
        .as_ref()
        .unwrap()
        .original_comparison_v1
        .is_none());
    assert!(real.children().is_empty());
    assert_eq!(real.snapshot(), before);
    assert!(background.current.contains_key(&View::Repository.index()));
}
#[test]
fn original_diff_sides_mixed_private_facet_rejected_before_any_effect() {
    mixed_result(true);
}
#[test]
fn original_diff_sides_ownerless_typed_result_rejected_before_any_effect() {
    mixed_result(false);
}
#[test]
fn original_diff_sides_cancel_does_not_cancel_an_unrelated_repository_job() {
    let mut background = Background::new();
    let cancellation = CancellationToken::default();
    background
        .current
        .insert(View::Repository.index(), (1, cancellation.clone()));
    assert!(!background.cancel_current_original_diff_v1());
    assert!(!cancellation.is_cancelled());
    assert!(background.current.contains_key(&View::Repository.index()));
}

#[test]
fn original_diff_sides_stale_logical_close_does_not_cancel_new_repository_generation() {
    let (real, a, b, _) = fixture::fixture("sha1");
    let workspace = fixture::workspace(&real, Comparison::BranchChanges { base: a, head: b });
    let mut picker =
        OriginalDiffPickerV1::new(OriginalComparisonIntentV1::capture(&workspace).unwrap());
    let old_token = CancellationToken::default();
    let request = picker.request().unwrap();
    let owner = OriginalDiffWorkerOwnerV1::begin(
        &workspace,
        Some(&picker),
        Arc::new(OriginalDiffWorkV1::Query(request)),
        old_token.clone(),
    )
    .unwrap();
    let new_token = CancellationToken::default();
    let mut background = Background::new();
    background.current_original_diff_owner_v1 = Some(owner.clone());
    background.current_original_diff_generation_v1 = Some(4);
    background
        .current
        .insert(View::Repository.index(), (5, new_token.clone()));
    let before = real.snapshot();
    real.clear_trace();
    assert!(!background.cancel_current_original_diff_v1());
    assert!(!old_token.is_cancelled() && !new_token.is_cancelled());
    assert_eq!(
        background.current.get(&View::Repository.index()).unwrap().0,
        5
    );
    assert!(Arc::ptr_eq(
        background.current_original_diff_owner_v1.as_ref().unwrap(),
        &owner
    ));
    assert_eq!(background.current_original_diff_generation_v1, Some(4));
    assert!(real.children().is_empty());
    assert_eq!(real.snapshot(), before);
}

#[test]
fn original_diff_sides_fresh_same_basis_comparison_arc_retires_both_success_and_error() {
    let (real, a, b, _) = fixture::fixture("sha256");
    let mut workspace = fixture::workspace(&real, Comparison::BranchChanges { base: a, head: b });
    let source = fixture::source(&workspace);
    source.install(&mut workspace).unwrap();
    let owner = crate::navigation_local_owner_v1::OriginalLocalNavigationOwnerV1::begin(
        &workspace,
        Default::default(),
    )
    .unwrap();
    let fresh = fixture::source(&workspace);
    assert!(!Arc::ptr_eq(&source, &fresh));
    assert_eq!(source.intent().head(), fresh.intent().head());
    assert_eq!(
        source.intent().content_base(),
        fresh.intent().content_base()
    );
    fresh.install(&mut workspace).unwrap();
    let before = real.snapshot();
    real.clear_trace();
    assert!(!owner.can_admit(&workspace, true));
    assert!(!owner.can_admit(&workspace, false));
    assert!(real.children().is_empty());
    assert_eq!(real.snapshot(), before);
}

#[test]
fn original_diff_sides_fresh_source_graph_back_publishes_exact_chosen_blame_line102_before_navigation(
) {
    use crate::{
        git::RawPath,
        navigation_local_source_v1::OriginalSavedVisitsV1,
        providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
        workspace::{FileAction, ProvenanceView, Workspace},
    };
    let real = crate::local_commit_actor_origin_v1_test_support::Real::new("sha256");
    let body = (1..=145)
        .map(|line| format!("saved nested line {line:03}\n"))
        .collect::<String>();
    let responsible = fixture::commit(
        &real,
        &fixture::tree(&real, &[(b"history.txt", body.as_bytes(), "100644")]),
        None,
        "Responsible",
    );
    let nested = fixture::tree(&real, &[(b"renamed.txt", body.as_bytes(), "100644")]);
    let root = String::from_utf8(crate::local_commit_actor_origin_v1_test_support::git(
        &real.root,
        &["mktree", "-z"],
        Some(format!("040000 tree {nested}\tnested\0").as_bytes()),
    ))
    .unwrap()
    .trim()
    .to_owned();
    let chosen = fixture::commit(&real, &root, Some(&responsible), "Chosen");
    let mut workspace = Workspace::load(&real.root, RepositoryMode::Tree, &chosen, None)
        .unwrap()
        .unwrap();
    workspace
        .open(RowAction::Directory(RawPath(b"nested".to_vec())))
        .unwrap();
    workspace
        .open(RowAction::File(RawPath(b"nested/renamed.txt".to_vec())))
        .unwrap();
    for action in [
        FileAction::Blame,
        FileAction::Next,
        FileAction::BlameLine(102),
        FileAction::Graph,
    ] {
        workspace.open(RowAction::FileProvenance(action)).unwrap();
    }
    let mut visits = OriginalSavedVisitsV1::default();
    let visit = visits.capture_graph_v2(&workspace).unwrap();
    let request = visit
        .graph_request_position_v2(&workspace.graph_position_v2().unwrap())
        .unwrap();
    let operation = LocalGitNavigationReadV1::begin(visit.origin(), Default::default()).unwrap();
    let restored = visit
        .restore_graph_position_v2(&request, &operation)
        .unwrap();
    operation.seal().unwrap();
    let before = real.snapshot();
    real.clear_trace();
    let worker = restored.clone();
    let mut app = App {
        view: View::Repository,
        selected: restored.graph_focus_row_v1().unwrap(),
        scroll: 40,
        repository_visible_rows: 10,
        repository_content_focused: true,
        workspace: Some(restored),
        ..Default::default()
    };
    let mut background = Background::new();
    assert!(
        background.submit_original_local_v1(&mut app, None, move |owner| {
            let mut workspace = worker;
            workspace.open_original_with_operation_v1(
                RowAction::FileProvenance(FileAction::Back),
                owner.operation(),
            )?;
            Ok(PreparedView {
                workspace: Some(workspace),
                status: "Actual exact Blame Back completed".into(),
                ..Default::default()
            })
        })
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while app.loading[View::Repository.index()] {
        background.poll(&mut app);
        assert!(
            std::time::Instant::now() < deadline,
            "Genuine original Back did not publish"
        );
        std::thread::yield_now();
    }
    assert_eq!(app.status, "Actual exact Blame Back completed");
    let workspace = app.workspace.as_ref().unwrap();
    let state = workspace.file_provenance.as_ref().unwrap();
    assert_eq!((&workspace.revision, &state.revision), (&chosen, &chosen));
    assert_eq!(state.path.0, b"nested/renamed.txt");
    assert_eq!(state.view, ProvenanceView::Blame);
    assert_eq!(state.blame_start, 101);
    let selected = state.selection.as_ref().unwrap();
    assert_eq!(selected.commit, responsible);
    assert_eq!(selected.path.as_ref().unwrap().0, b"history.txt");
    assert_eq!(
        (selected.original_line, selected.final_line),
        (Some(102), Some(102))
    );
    assert_eq!(
        workspace.display_rows_v1()[app.selected].action,
        Some(RowAction::FileProvenance(FileAction::BlameLine(102)))
    );
    assert_eq!(workspace.provenance_focus_row(), Some(app.selected));
    assert!(app.selected >= app.scroll && app.selected < app.scroll + app.repository_visible_rows);
    assert!(!real.children().is_empty());
    fixture::assert_no_discovery(&real);
    assert_eq!(real.snapshot(), before);
}
