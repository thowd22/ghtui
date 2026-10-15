use super::*;
use crate::{
    git::Operation,
    local_commit_actor_origin_v1_test_support::{git, Real},
    providers::runner::CancellationToken,
    repository_native_diff_v1::NativeDiffOptionChangeV1,
    repository_view::DiffSide,
    workspace::RepositoryMode,
};
use std::{
    fs,
    os::unix::ffi::OsStringExt,
    time::{Duration, Instant},
};
fn load(real: &Real, mode: RepositoryMode) -> Workspace {
    Workspace::load(&real.root, mode, "HEAD", None)
        .unwrap()
        .unwrap()
}
// Unlike default `git diff`, these independent observations cannot refresh a
// clean index's stat-cache after --cached apply. Preserve the raw-byte oracle.
fn write_effect_state(real: &Real) -> Vec<Vec<u8>> {
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
                "--no-ext-diff",
            ],
            None,
        ),
        git(
            &real.root,
            &[
                "-c",
                "diff.autoRefreshIndex=false",
                "diff",
                "--binary",
                "--no-ext-diff",
            ],
            None,
        ),
        fs::read(real.root.join("tracked")).unwrap(),
        fs::read(real.root.join("scratch")).unwrap(),
        fs::read(real.root.join(".git/config")).unwrap(),
        git(&real.root, &["cat-file", "commit", &real.original], None),
    ]
}
fn publish(state: &mut State) {
    let request = state.request().unwrap();
    let op =
        LocalGitNavigationReadV1::begin(request.intent().origin(), Default::default()).unwrap();
    let prepared = request.prepare(&op).unwrap();
    op.seal().unwrap();
    assert!(state.accept(prepared));
}
fn selection(workspace: &Workspace) -> State {
    let mut state = State::open_mutable(workspace, 118).unwrap();
    publish(&mut state);
    state.select_hunk(0).unwrap();
    publish(&mut state);
    state
}
fn prepare(
    request: &Arc<NativeMutablePreviewRequestV2>,
    real: &Real,
) -> Arc<PreparedNativeMutablePlanV2> {
    let op =
        LocalGitNavigationReadV1::begin(request.source().intent().origin(), Default::default())
            .unwrap();
    real.clear_trace();
    let prepared = request.prepare(&op).unwrap();
    let accounting = op.seal().unwrap();
    assert_eq!(accounting.children_started(), real.children().len());
    assert!(accounting.children_started() > 10 && accounting.children_started() < 128);
    assert_eq!(accounting.in_flight_requests(), 0);
    prepared
}
#[test]
fn native_mutable_plan_retired_confirm_before_queue_and_after_binding_cancel_exact_original_token_zero_git(
) {
    for bound in [false, true] {
        let real = Real::new("sha1");
        let workspace = load(&real, RepositoryMode::Working);
        let state = selection(&workspace);
        let preview = NativeMutablePreviewRequestV2::capture(&state, &workspace).unwrap();
        let prepared = prepare(&preview, &real);
        let confirm =
            NativeMutableConfirmRequestV2::capture(&prepared, &state, &workspace).unwrap();
        let op =
            LocalGitNavigationReadV1::begin(preview.source().intent().origin(), Default::default())
                .unwrap();
        if bound {
            confirm.bind_original_operation_v2(&op).unwrap();
        }
        let before = write_effect_state(&real);
        real.clear_trace();
        confirm.retire();
        let error = match confirm.prepare(&op) {
            Err(error) => error,
            Ok(_) => panic!("retired explicit purpose cannot write"),
        };
        assert!(error.contains("retired"));
        assert_eq!(op.cancellation().is_cancelled(), bound);
        assert!(real.children().is_empty());
        assert_eq!(write_effect_state(&real), before);
        let replacement =
            LocalGitNavigationReadV1::begin(preview.source().intent().origin(), Default::default())
                .unwrap();
        assert!(confirm.prepare(&replacement).is_err());
        assert!(real.children().is_empty());
    }
}
#[test]
fn native_mutable_plan_actual_one_new_line_uses_original_membership_not_whole_hunk() {
    let real = Real::new("sha256");
    fs::write(
        real.root.join("tracked"),
        b"staged bytes\nselected one\nnot selected\n",
    )
    .unwrap();
    let workspace = load(&real, RepositoryMode::Working);
    let mut state = State::open_mutable(&workspace, 118).unwrap();
    publish(&mut state);
    let frame = state.current_frame().unwrap();
    let row = frame
        .rows()
        .iter()
        .position(|row| row.left().text().contains("selected one"))
        .unwrap();
    let ticket = frame.ticket(row, false).unwrap();
    state.select(ticket, false).unwrap();
    publish(&mut state);
    assert_eq!(state.range().unwrap().changed().len(), 1);
    let request = NativeMutablePreviewRequestV2::capture(&state, &workspace).unwrap();
    let prepared = prepare(&request, &real);
    let confirm = NativeMutableConfirmRequestV2::capture(&prepared, &state, &workspace).unwrap();
    let op =
        LocalGitNavigationReadV1::begin(request.source().intent().origin(), Default::default())
            .unwrap();
    let outcome = confirm.prepare(&op).unwrap();
    assert!(outcome.note().contains("completed"));
    op.seal().unwrap();
    assert_eq!(
        git(&real.root, &["show", ":tracked"], None),
        b"staged bytes\nselected one\n"
    );
    assert_eq!(
        fs::read(real.root.join("tracked")).unwrap(),
        b"staged bytes\nselected one\nnot selected\n"
    );
}
#[test]
fn native_mutable_plan_working_and_staged_same_canonical_planner_real_git_dry_run_without_write() {
    for mode in [RepositoryMode::Working, RepositoryMode::Staged] {
        let real = Real::new("sha256");
        let workspace = load(&real, mode);
        let state = selection(&workspace);
        let before = real.snapshot();
        real.clear_trace();
        let request = NativeMutablePreviewRequestV2::capture(&state, &workspace).unwrap();
        assert!(
            real.children().is_empty(),
            "pure input captures only exact Arcs"
        );
        let prepared = prepare(&request, &real);
        assert!(prepared.matches(&request, &state, &workspace));
        let plan = prepared.plan();
        assert!(
            plan.context.comparison.is_none(),
            "Native Arc plan never duplicates a Workspace/diff body into form context"
        );
        assert_eq!(
            plan.mutation.observed,
            request.source().result().observed.unwrap()
        );
        assert_eq!(plan.context.selected_lines, request.range().changed());
        assert_eq!(plan.context.selected_path.as_ref().unwrap().0, b"tracked");
        assert!(plan.preview[0].contains("no changes until confirmation"));
        assert!(
            plan.preview[0].starts_with(if mode == RepositoryMode::Working {
                "Stage selected lines"
            } else {
                "Unstage selected lines"
            })
        );
        let Operation::ApplyPatch { patch, reverse } = &plan.mutation.operation else {
            panic!("shared exact ApplyPatch plan");
        };
        assert!(
            !reverse,
            "direction-aware shared unstage patch applies forward to actual index"
        );
        assert!(patch
            .windows(b"staged bytes".len())
            .any(|part| part == b"staged bytes"));
        git(
            &real.root,
            &["apply", "--cached", "--check", "--whitespace=nowarn"],
            Some(patch),
        );
        assert_eq!(
            real.snapshot(),
            before,
            "actual Git dry-run and preview preserve index/worktree/refs/config"
        );
    }
}
#[test]
fn native_mutable_plan_fresh_external_work_index_ref_and_untracked_changes_reject_before_plan() {
    for change in 0..4 {
        let real = Real::new("sha1");
        let workspace = load(&real, RepositoryMode::Working);
        let state = selection(&workspace);
        let request = NativeMutablePreviewRequestV2::capture(&state, &workspace).unwrap();
        match change {
            0 => fs::write(real.root.join("tracked"), b"external work\n").unwrap(),
            1 => {
                git(&real.root, &["add", "--", "tracked"], None);
            }
            2 => {
                git(&real.root, &["tag", "external-plan-ref"], None);
            }
            _ => fs::write(real.root.join("scratch"), b"external scratch\n").unwrap(),
        }
        let before = real.snapshot();
        real.clear_trace();
        let op =
            LocalGitNavigationReadV1::begin(request.source().intent().origin(), Default::default())
                .unwrap();
        let error = request
            .prepare(&op)
            .err()
            .expect("genuine changed state refuses plan");
        assert!(error.contains("stale"), "{error}");
        assert_eq!(op.seal().unwrap().children_started(), 5);
        assert_eq!(real.children().len(), 5);
        assert_eq!(real.snapshot(), before);
    }
}
#[test]
fn native_mutable_plan_original_canceled_sealed_or_expired_window_cannot_prepare_or_renew() {
    let real = Real::new("sha1");
    let workspace = load(&real, RepositoryMode::Working);
    let state = selection(&workspace);
    let request = NativeMutablePreviewRequestV2::capture(&state, &workspace).unwrap();
    for kind in 0..3 {
        let token = CancellationToken::default();
        let op = if kind == 2 {
            LocalGitNavigationReadV1::begin_under_deadline(
                request.source().intent().origin(),
                token.clone(),
                Instant::now() + Duration::from_millis(20),
            )
            .unwrap()
        } else {
            LocalGitNavigationReadV1::begin(request.source().intent().origin(), token.clone())
                .unwrap()
        };
        match kind {
            0 => token.cancel(),
            1 => {
                op.seal().unwrap();
            }
            _ => {
                while Instant::now() < op.deadline() {
                    std::thread::yield_now();
                }
            }
        }
        let before = real.snapshot();
        real.clear_trace();
        assert!(request.prepare(&op).is_err());
        assert!(real.children().is_empty());
        assert_eq!(op.accounting().unwrap().children_started(), 0);
        assert_eq!(real.snapshot(), before);
    }
}
#[test]
fn native_mutable_plan_exact_session_frame_range_and_request_pointer_reject_late_preview() {
    for change in 0..6 {
        let real = Real::new("sha1");
        let workspace = load(&real, RepositoryMode::Working);
        let mut state = selection(&workspace);
        let request = NativeMutablePreviewRequestV2::capture(&state, &workspace).unwrap();
        let prepared = prepare(&request, &real);
        let other = NativeMutablePreviewRequestV2::capture(&state, &workspace).unwrap();
        assert!(
            !prepared.matches(&other, &state, &workspace),
            "identical source/range does not replace producing request Arc"
        );
        match change {
            0 => state.query.insert("dirty").unwrap(),
            1 => state.width = 38,
            2 => state.option(NativeDiffOptionChangeV1::Layout).unwrap(),
            3 => state.clear_selection(),
            4 => {
                state.request().unwrap();
            }
            _ => state = selection(&workspace),
        }
        let before = real.snapshot();
        real.clear_trace();
        assert!(!request.accepts(&state));
        assert!(!prepared.matches(&request, &state, &workspace));
        assert!(real.children().is_empty());
        assert_eq!(real.snapshot(), before);
    }
}
#[test]
fn native_mutable_plan_missing_selection_or_genuine_empty_comparison_has_no_preview_authority() {
    let real = Real::new("sha1");
    let workspace = load(&real, RepositoryMode::Working);
    let mut state = State::open_mutable(&workspace, 118).unwrap();
    assert!(NativeMutablePreviewRequestV2::capture(&state, &workspace).is_err());
    publish(&mut state);
    assert!(NativeMutablePreviewRequestV2::capture(&state, &workspace).is_err());
    git(&real.root, &["add", "--", "tracked"], None);
    let workspace = load(&real, RepositoryMode::Working);
    let mut state = State::open_mutable(&workspace, 118).unwrap();
    publish(&mut state);
    assert!(state.source().unwrap().complete() && state.source().unwrap().members().is_empty());
    let before = real.snapshot();
    real.clear_trace();
    assert!(NativeMutablePreviewRequestV2::capture(&state, &workspace).is_err());
    assert!(real.children().is_empty());
    assert_eq!(real.snapshot(), before);
}
#[test]
fn native_mutable_plan_original_distinct_raw_paths_keep_exact_patch_targets_and_dry_run() {
    let real = Real::new("sha256");
    #[cfg(not(target_os = "macos"))]
    let paths: [&[u8]; 2] = [b"raw\xfe", b"raw\xff"];
    // APFS requires valid UTF-8 names. A real newline and a literal backslash-n
    // remain distinct raw paths without skipping or collapsing identity proof.
    #[cfg(target_os = "macos")]
    let paths: [&[u8]; 2] = [b"raw\nname", br"raw\nname"];
    assert_ne!(paths[0], paths[1]);
    for (path, body) in [
        (paths[0], b"FE original\n".as_slice()),
        (paths[1], b"FF original\n".as_slice()),
    ] {
        fs::write(
            real.root.join(std::ffi::OsString::from_vec(path.to_vec())),
            body,
        )
        .unwrap();
    }
    git(&real.root, &["add", "-A"], None);
    git(
        &real.root,
        &["commit", "-qm", "raw selected baseline"],
        None,
    );
    for (path, body) in [
        (paths[0], b"FE changed\n".as_slice()),
        (paths[1], b"FF changed\n".as_slice()),
    ] {
        fs::write(
            real.root.join(std::ffi::OsString::from_vec(path.to_vec())),
            body,
        )
        .unwrap();
    }
    let workspace = load(&real, RepositoryMode::Working);
    let mut state = State::open_mutable(&workspace, 118).unwrap();
    publish(&mut state);
    let before = real.snapshot();
    let mut patches = Vec::new();
    for (path, expected) in [
        (paths[0], b"+FE changed".as_slice()),
        (paths[1], b"+FF changed".as_slice()),
    ] {
        state.file = state
            .source()
            .unwrap()
            .members()
            .iter()
            .find(|member| {
                member
                    .path(DiffSide::New)
                    .is_some_and(|actual| actual.0 == path)
            })
            .unwrap()
            .index();
        state.clear_selection();
        publish(&mut state);
        state.select_hunk(0).unwrap();
        publish(&mut state);
        let request = NativeMutablePreviewRequestV2::capture(&state, &workspace).unwrap();
        assert_eq!(
            request.source().members()[request.range().file()]
                .path(DiffSide::New)
                .unwrap()
                .0,
            path
        );
        let prepared = prepare(&request, &real);
        let Operation::ApplyPatch { patch, .. } = &prepared.plan().mutation.operation else {
            panic!("actual selected patch");
        };
        assert!(patch.windows(expected.len()).any(|part| part == expected));
        git(&real.root, &["apply", "--cached", "--check"], Some(patch));
        patches.push(patch.clone());
        assert_eq!(real.snapshot(), before);
    }
    assert_ne!(patches[0], patches[1]);
    assert!(workspace.original_comparison_v1.is_none());
}

#[test]
fn native_mutable_plan_distinct_confirm_actual_stage_unstage_one_write_shared_original_runner() {
    for mode in [RepositoryMode::Working, RepositoryMode::Staged] {
        let real = Real::new("sha256");
        let workspace = load(&real, mode);
        let state = selection(&workspace);
        let preview = NativeMutablePreviewRequestV2::capture(&state, &workspace).unwrap();
        let plan = prepare(&preview, &real);
        let before = real.snapshot();
        real.clear_trace();
        let confirm = NativeMutableConfirmRequestV2::capture(&plan, &state, &workspace).unwrap();
        assert!(confirm.owns(&workspace) && confirm.accepts(&state));
        assert!(
            real.children().is_empty(),
            "explicit confirmation input clones only exact bound Arcs"
        );
        let op =
            LocalGitNavigationReadV1::begin(confirm.source().intent().origin(), Default::default())
                .unwrap();
        confirm
            .source()
            .intent()
            .origin()
            .with_saved_navigation_read_v1(&op, |repo| {
                let error = repo
                    .execute(&confirm.plan().mutation, true)
                    .unwrap_err()
                    .to_string();
                assert!(
                    error.contains("Read-only"),
                    "ordinary execute still refuses a live read window: {error}"
                );
                Ok(())
            })
            .unwrap();
        assert!(
            real.children().is_empty(),
            "read-window possession never becomes write permission"
        );
        let outcome = confirm.prepare(&op).unwrap();
        assert!(outcome.matches(&confirm));
        assert!(outcome.note().contains("completed"));
        let after_write_count = real.children().len();
        assert!(confirm
            .prepare(&op)
            .err()
            .unwrap()
            .contains("already attempted"));
        assert_eq!(real.children().len(),after_write_count,"once-only reservation rejects duplicate in the still-live original operation before Git");
        let accounting = op.seal().unwrap();
        let children = real.children();
        assert_eq!(
            children.len(),
            accounting.children_started() + 1,
            "one separately authorized write is NOT misreported in the READ ledger"
        );
        let writes = children
            .iter()
            .filter(|event| {
                event["argv"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|arg| arg.as_str() == Some("apply"))
            })
            .count();
        assert_eq!(writes, 1);
        let index = git(&real.root, &["show", ":tracked"], None);
        assert_eq!(
            index,
            if mode == RepositoryMode::Working {
                b"dirty bytes\n".as_slice()
            } else {
                b"saved original bytes\n".as_slice()
            }
        );
        assert_eq!(
            fs::read(real.root.join("tracked")).unwrap(),
            b"dirty bytes\n"
        );
        let after = write_effect_state(&real);
        assert_ne!(after[1], before[1]);
        for index in [0, 4, 5, 6, 7] {
            assert_eq!(after[index], before[index]);
        }
        if mode == RepositoryMode::Working {
            assert!(git(
                &real.root,
                &["-c", "diff.autoRefreshIndex=false", "diff", "--binary"],
                None
            )
            .is_empty());
        } else {
            assert!(git(
                &real.root,
                &[
                    "-c",
                    "diff.autoRefreshIndex=false",
                    "diff",
                    "--cached",
                    "--binary"
                ],
                None
            )
            .is_empty());
        }
        real.clear_trace();
        assert!(confirm.prepare(&op).is_err());
        assert!(real.children().is_empty());
        let replay =
            LocalGitNavigationReadV1::begin(confirm.source().intent().origin(), Default::default())
                .unwrap();
        assert!(confirm
            .prepare(&replay)
            .err()
            .unwrap()
            .contains("Original confirmation operation"));
        assert!(real.children().is_empty());
        assert_eq!(replay.seal().unwrap().children_started(), 0);
        assert_eq!(write_effect_state(&real), after);
    }
}
#[test]
fn native_mutable_plan_confirm_fresh_work_index_ref_untracked_change_refuses_before_any_write() {
    for change in 0..4 {
        let real = Real::new("sha1");
        let workspace = load(&real, RepositoryMode::Working);
        let state = selection(&workspace);
        let preview = NativeMutablePreviewRequestV2::capture(&state, &workspace).unwrap();
        let plan = prepare(&preview, &real);
        let confirm = NativeMutableConfirmRequestV2::capture(&plan, &state, &workspace).unwrap();
        match change {
            0 => fs::write(real.root.join("tracked"), b"external work\n").unwrap(),
            1 => {
                git(&real.root, &["add", "--", "tracked"], None);
            }
            2 => {
                git(&real.root, &["tag", "external-confirm-ref"], None);
            }
            _ => fs::write(real.root.join("scratch"), b"external scratch\n").unwrap(),
        }
        let before = real.snapshot();
        real.clear_trace();
        let op =
            LocalGitNavigationReadV1::begin(confirm.source().intent().origin(), Default::default())
                .unwrap();
        let error = confirm.prepare(&op).err().unwrap();
        assert!(error.contains("stale"), "{error}");
        assert!(error.contains("no automatic retry"));
        assert_eq!(op.seal().unwrap().children_started(), 5);
        assert_eq!(real.children().len(), 5);
        assert!(!real.children().iter().any(|event| event["argv"]
            .as_array()
            .unwrap()
            .iter()
            .any(|arg| arg.as_str() == Some("apply"))));
        assert_eq!(real.snapshot(), before);
    }
}
#[test]
fn native_mutable_plan_confirm_source_scope_canceled_sealed_expired_and_foreign_window_zero_git() {
    let real = Real::new("sha1");
    let workspace = load(&real, RepositoryMode::Working);
    let state = selection(&workspace);
    let preview = NativeMutablePreviewRequestV2::capture(&state, &workspace).unwrap();
    let plan = prepare(&preview, &real);
    for kind in 0..4 {
        let confirm = NativeMutableConfirmRequestV2::capture(&plan, &state, &workspace).unwrap();
        let token = CancellationToken::default();
        let op = if kind == 2 {
            LocalGitNavigationReadV1::begin_under_deadline(
                confirm.source().intent().origin(),
                token.clone(),
                Instant::now() + Duration::from_millis(20),
            )
            .unwrap()
        } else {
            LocalGitNavigationReadV1::begin(confirm.source().intent().origin(), token.clone())
                .unwrap()
        };
        match kind {
            0 => token.cancel(),
            1 => {
                op.seal().unwrap();
            }
            2 => {
                while Instant::now() < op.deadline() {
                    std::thread::yield_now();
                }
            }
            _ => {}
        }
        let before = real.snapshot();
        real.clear_trace();
        if kind == 3 {
            let foreign = LocalGitNavigationReadV1::begin(
                confirm.source().intent().origin(),
                Default::default(),
            )
            .unwrap();
            let error = confirm
                .source()
                .intent()
                .origin()
                .with_saved_navigation_read_v1(&foreign, |repo| {
                    repo.execute_native_selected_v2(&confirm, &op)
                        .map_err(|error| error.to_string())
                })
                .unwrap_err();
            assert!(error.contains("exact original repository/runner/read operation"));
            foreign.seal().unwrap();
        } else {
            assert!(confirm.prepare(&op).is_err());
        }
        assert!(real.children().is_empty());
        assert_eq!(op.accounting().unwrap().children_started(), 0);
        assert_eq!(real.snapshot(), before);
    }
}
#[test]
fn native_mutable_plan_confirm_capture_rechecks_exact_current_parent_epoch_without_git() {
    for change in 0..5 {
        let real = Real::new("sha1");
        let workspace = load(&real, RepositoryMode::Working);
        let mut state = selection(&workspace);
        let preview = NativeMutablePreviewRequestV2::capture(&state, &workspace).unwrap();
        let plan = prepare(&preview, &real);
        match change {
            0 => state.query.insert("changed").unwrap(),
            1 => state.width = 38,
            2 => state.option(NativeDiffOptionChangeV1::Wrap).unwrap(),
            3 => state.clear_selection(),
            _ => state = selection(&workspace),
        }
        let before = real.snapshot();
        real.clear_trace();
        assert!(NativeMutableConfirmRequestV2::capture(&plan, &state, &workspace).is_err());
        assert!(real.children().is_empty());
        assert_eq!(real.snapshot(), before);
    }
}
