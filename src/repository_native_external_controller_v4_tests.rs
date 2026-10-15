//! Genuine source/operation/controller contracts; actual-binary input remains
//! separately mandatory. No manufactured rows, source, source clock or permit.
use super::*;
use crate::{
    providers::owned_pty_viewer_v4::ViewerAdmissionV4,
    repository_native_diff_v1::NativeDiffStateV1,
    repository_native_external_ui_v4::{ExternalInputV4, NativeExternalLaunchV4},
};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;

fn loaded() -> (Real, crate::workspace::Workspace, NativeDiffStateV1) {
    let (real, a, b, _) = f::fixture("sha256");
    let mut workspace = f::workspace(&real, Comparison::BranchChanges { base: a, head: b });
    f::source(&workspace).install(&mut workspace).unwrap();
    let mut state = NativeDiffStateV1::open(&workspace, 80).unwrap();
    state.file = state
        .source()
        .unwrap()
        .members()
        .iter()
        .find(|m| {
            m.path(OriginalComparisonSideV1::New)
                .is_some_and(|p| p.0 == b"shared")
        })
        .unwrap()
        .index();
    project(&mut state, &workspace);
    (real, workspace, state)
}
fn project(state: &mut NativeDiffStateV1, workspace: &crate::workspace::Workspace) {
    let request = state.request().unwrap();
    let op = operation(workspace);
    let prepared = request.prepare(&op).unwrap();
    assert_eq!(op.seal().unwrap().children_started(), 0);
    assert!(state.accept(prepared));
}
fn launch<S: crate::repository_native_diff_content_v2::NativeDiffPurposeV2>(
    state: &mut NativeDiffStateV1<S>,
    tool: ExternalToolV4,
) -> Arc<NativeExternalLaunchV4> {
    state.choose_external_v4(Rect::new(0, 0, 82, 40)).unwrap();
    let admission = ViewerAdmissionV4::new();
    if tool == ExternalToolV4::Difftastic {
        let event = Event::Key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));
        assert!(matches!(
            state
                .external_v4
                .as_mut()
                .unwrap()
                .event(&event, true, &admission),
            ExternalInputV4::Captured
        ));
    }
    let event = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let ExternalInputV4::Launch(launch) = state
        .external_v4
        .as_mut()
        .unwrap()
        .event(&event, true, &admission)
    else {
        panic!("explicit original Launch must be captured")
    };
    state.begin_external_v4(launch.clone()).unwrap();
    launch
}
fn original_bounds<'a>(
    operation: &'a LocalGitNavigationReadV1,
    launch: &Arc<NativeExternalLaunchV4>,
    tools: &Tools,
) -> ExternalRendererBoundsV4<'a, LocalGitNavigationReadV1> {
    let mut bounds = ExternalRendererBoundsV4::local(operation, launch.display.clone()).unwrap();
    bounds.programs_override = Some((tools.renderer.clone(), tools.pager.program.clone()));
    bounds
}
#[test]
fn native_external_diff_controller_same_original_request_captured_patch_and_exact_range_return() {
    let (real, workspace, mut state) = loaded();
    let before = observed(&real);
    let hunk = state.current_frame().unwrap().hunks()[0].hunk();
    state.select_hunk(hunk).unwrap();
    project(&mut state, &workspace);
    state.query.insert("TARGET").unwrap();
    project(&mut state, &workspace);
    let range = state.capture_current_range_v2().unwrap();
    let frame = state.current_frame().unwrap().clone();
    let source = state.source().unwrap().clone();
    let tools = tools(
        ExternalToolV4::Delta,
        "process.stdout.write('original-rendered');",
    );
    let launch = launch(&mut state, ExternalToolV4::Delta);
    let deadline = launch.display.deadline();
    let request = state.request().unwrap();
    let op = operation(&workspace);
    let bounds = original_bounds(&op, &launch, &tools);
    assert!(
        request.prepare(&op).is_err(),
        "ordinary projection may not inherit an external process context"
    );
    let prepared = request.prepare_external_v4(&op, &bounds).unwrap();
    assert!(Arc::ptr_eq(prepared.source(), &source));
    assert!(prepared.frame().is_none() && !prepared.source_only());
    assert_eq!(op.seal().unwrap().children_started(), 0);
    assert!(state.accept(prepared));
    assert!(state.current_frame().is_none());
    assert!(state.capture_current_range_v2().is_err());
    assert_eq!(launch.display.deadline(), deadline);
    assert!(!state.return_external_v4(82));
    assert!(range.matches(&state));
    assert!(Arc::ptr_eq(state.current_frame().unwrap(), &frame));
    assert_eq!(state.query.text, "TARGET");
    assert!(launch.display.cancellation().is_cancelled());
    assert!(!state.accepts(&request));
    assert_eq!(observed(&real), before);
}
#[test]
fn native_external_diff_controller_foreign_operation_or_foreign_launch_refuses_before_any_tool() {
    let (real, workspace, mut state) = loaded();
    let before = observed(&real);
    let tools = tools(
        ExternalToolV4::Delta,
        "process.stdout.write('must-not-run');",
    );
    let launch = launch(&mut state, ExternalToolV4::Delta);
    let request = state.request().unwrap();
    let op = operation(&workspace);
    let foreign = operation(&workspace);
    let bounds = original_bounds(&foreign, &launch, &tools);
    assert!(request.prepare_external_v4(&op, &bounds).is_err());
    assert!(!tools.evidence.exists());
    let mut wrong = original_bounds(&op, &launch, &tools);
    wrong.display = DisplayPermitV4::launch();
    assert!(request.prepare_external_v4(&op, &wrong).is_err());
    assert!(!tools.evidence.exists());
    assert_eq!(op.accounting().unwrap().children_started(), 0);
    assert_eq!(foreign.accounting().unwrap().children_started(), 0);
    assert_eq!(observed(&real), before);
}
#[test]
fn native_external_diff_controller_queued_success_and_error_reject_new_query_without_effects() {
    let (real, workspace, mut state) = loaded();
    let before = observed(&real);
    let source = state.source().unwrap().clone();
    let tools = tools(
        ExternalToolV4::Delta,
        "process.stdout.write('late-private-output');",
    );
    let launch = launch(&mut state, ExternalToolV4::Delta);
    let request = state.request().unwrap();
    let op = operation(&workspace);
    let bounds = original_bounds(&op, &launch, &tools);
    let prepared = request.prepare_external_v4(&op, &bounds).unwrap();
    op.seal().unwrap();
    state.query.insert("NEWER_QUERY").unwrap();
    let status = state.status.clone();
    assert!(!state.accept(prepared));
    assert!(!state.fail(&request, "PRIVATE obsolete error"));
    assert_eq!(state.status, status);
    assert!(Arc::ptr_eq(state.source().unwrap(), &source));
    assert!(!state.external_v4.as_ref().unwrap().is_ready());
    assert_eq!(observed(&real), before);
}
#[test]
fn native_external_diff_controller_renderer_failure_restores_current_original_frame_not_source_only(
) {
    let (real, workspace, mut state) = loaded();
    let before = observed(&real);
    let frame = state.current_frame().unwrap().clone();
    let tools = tools(
        ExternalToolV4::Delta,
        "process.stderr.write('PRIVATE-BODY');process.exit(2);",
    );
    let launch = launch(&mut state, ExternalToolV4::Delta);
    let request = state.request().unwrap();
    let op = operation(&workspace);
    let bounds = original_bounds(&op, &launch, &tools);
    let fallback = request.prepare_external_v4(&op, &bounds).unwrap();
    op.seal().unwrap();
    assert!(fallback.frame().is_none() && !fallback.source_only());
    assert!(state.accept(fallback));
    assert!(Arc::ptr_eq(state.current_frame().unwrap(), &frame));
    let notice = state.take_external_notice_v4().unwrap();
    assert!(notice.contains("exited2") && !notice.contains("PRIVATE-BODY"));
    assert!(state.metadata_note().is_none());
    assert!(state.external_v4.is_none());
    assert_eq!(observed(&real), before);
}
#[test]
fn native_external_diff_controller_structural_uses_same_full_side_factories_actual_c_b_and_zero_new_parser(
) {
    let (real, workspace, mut state) = loaded();
    let before = observed(&real);
    let tools = tools(
        ExternalToolV4::Difftastic,
        "process.stdout.write('structural-current');",
    );
    let launch = launch(&mut state, ExternalToolV4::Difftastic);
    let request = state.request().unwrap();
    let op = operation(&workspace);
    let bounds = original_bounds(&op, &launch, &tools);
    let prepared = request.prepare_external_v4(&op, &bounds).unwrap();
    let accounting = op.seal().unwrap();
    assert!(accounting.children_started() > 0);
    let actual: serde_json::Value =
        serde_json::from_slice(&fs::read(&tools.evidence).unwrap()).unwrap();
    assert_eq!(actual["old"], hex(b"COMMON old bytes\n"));
    assert_eq!(actual["new"], hex(b"TARGET new bytes\n"));
    assert!(state.accept(prepared));
    assert!(!state.return_external_v4(82));
    assert!(state.current_frame().is_some());
    assert_eq!(observed(&real), before);
}
#[test]
fn native_external_diff_controller_mutable_full_sides_fit_same_original_pool_and_preserve_git() {
    use crate::{git::MutableDiffSourceV2, workspace::RepositoryMode};
    for format in ["sha1", "sha256"] {
        let real = Real::new(format);
        let before = observed(&real);
        for (mode, old, new) in [
            (RepositoryMode::Working, "staged bytes\n", "dirty bytes\n"),
            (
                RepositoryMode::Staged,
                "saved original bytes\n",
                "staged bytes\n",
            ),
            (
                RepositoryMode::Uncommitted,
                "saved original bytes\n",
                "dirty bytes\n",
            ),
        ] {
            let workspace = crate::workspace::Workspace::load(&real.root, mode, "HEAD", None)
                .unwrap()
                .unwrap();
            let mut state =
                NativeDiffStateV1::<MutableDiffSourceV2>::open_mutable(&workspace, 80).unwrap();
            let first = state.request().unwrap();
            let first_op = operation(&workspace);
            let prepared = first.prepare(&first_op).unwrap();
            first_op.seal().unwrap();
            assert!(state.accept(prepared));
            let source = state.source().unwrap().clone();
            let frame = state.current_frame().unwrap().clone();
            let tools = tools(
                ExternalToolV4::Difftastic,
                "process.stdout.write('actual mutable structural output');",
            );
            let launch = launch(&mut state, ExternalToolV4::Difftastic);
            let request = state.request().unwrap();
            real.clear_trace();
            let op = operation(&workspace);
            let bounds = original_bounds(&op, &launch, &tools);
            let prepared = request.prepare_external_v4(&op, &bounds).unwrap();
            let accounting = op.seal().unwrap();
            assert!(accounting.children_started() > 0 && accounting.children_started() <= 128);
            assert_eq!(accounting.children_started(), real.children().len());
            eprintln!(
                "actual {format} {mode:?} structural original-pool children={}",
                accounting.children_started()
            );
            let actual: serde_json::Value =
                serde_json::from_slice(&fs::read(&tools.evidence).unwrap()).unwrap();
            assert_eq!(actual["old"], hex(old.as_bytes()));
            assert_eq!(actual["new"], hex(new.as_bytes()));
            assert!(!Path::new(actual["cwd"].as_str().unwrap()).exists());
            assert!(state.accept(prepared));
            assert!(state.current_frame().is_none());
            assert!(!state.return_external_v4(82));
            assert!(Arc::ptr_eq(state.current_frame().unwrap(), &frame));
            assert!(Arc::ptr_eq(state.source().unwrap(), &source));
            assert!(state.take_external_notice_v4().is_none());
            assert_eq!(observed(&real), before);
        }
    }
}
#[test]
fn native_external_diff_controller_final_raw_stamp_change_refuses_success_and_tool_failure() {
    use std::os::unix::fs::MetadataExt;
    for tool_fails in [false, true] {
        let real = Real::new("sha256");
        let workspace = crate::workspace::Workspace::load(
            &real.root,
            crate::workspace::RepositoryMode::Working,
            "HEAD",
            None,
        )
        .unwrap()
        .unwrap();
        let mut state =
            NativeDiffStateV1::<crate::git::MutableDiffSourceV2>::open_mutable(&workspace, 80)
                .unwrap();
        let first = state.request().unwrap();
        let first_op = operation(&workspace);
        let prepared = first.prepare(&first_op).unwrap();
        first_op.seal().unwrap();
        assert!(state.accept(prepared));
        let before = observed(&real);
        let path = real.root.join("tracked");
        assert_ne!(fs::metadata(&path).unwrap().mtime(), 2);
        let tools = tools(
            ExternalToolV4::Difftastic,
            &format!(
                "fs.utimesSync({},1,2);process.stdout.write('actual structural output');{}",
                serde_json::to_string(&path).unwrap(),
                if tool_fails { "process.exit(2);" } else { "" }
            ),
        );
        let launch = launch(&mut state, ExternalToolV4::Difftastic);
        let request = state.request().unwrap();
        real.clear_trace();
        let op = operation(&workspace);
        let bounds = original_bounds(&op, &launch, &tools);
        let error = request.prepare_external_v4(&op, &bounds).err().unwrap();
        let accounting = op.seal().unwrap();
        assert!(
            error.contains("Full-side content/evidence changed"),
            "{error}"
        );
        assert!(accounting.children_started() > 0 && accounting.children_started() <= 128);
        assert_eq!(accounting.children_started(), real.children().len());
        assert!(
            tools.evidence.exists(),
            "the actual renderer reached its body"
        );
        assert_eq!(fs::metadata(&path).unwrap().mtime(), 2);
        assert_eq!(
            observed(&real),
            before,
            "bytes and Git truth did not change"
        );
        assert!(state.fail(&request, &error));
        assert!(state.current_frame().is_none());
        assert!(state.status.contains("NOT CURRENT"));
        assert!(state.take_external_notice_v4().is_none());
    }
}
#[test]
fn native_external_diff_controller_preparing_resize_cancels_exact_launch_and_requires_explicit_native_back(
) {
    let (real, workspace, mut state) = loaded();
    let before = observed(&real);
    let launch = launch(&mut state, ExternalToolV4::Delta);
    let request = state.request().unwrap();
    let input = state.external_v4.as_mut().unwrap().event(
        &Event::Resize(40, 24),
        false,
        &ViewerAdmissionV4::new(),
    );
    assert!(matches!(input, ExternalInputV4::CancelPreparation));
    state.cancel_external_preparation_v4(40);
    assert!(launch.display.cancellation().is_cancelled());
    assert!(!state.accepts(&request));
    assert!(!state.fail(&request, "retired panic"));
    assert!(state.pending().is_none());
    assert!(state.current_frame().is_none());
    assert!(state.external_v4.is_some());
    let children_before_draw = real.children().len();
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(40, 24)).unwrap();
    terminal
        .draw(|frame| {
            state.external_v4.as_mut().unwrap().draw(
                frame,
                frame.area(),
                crate::theme::Theme::from_settings(0, false),
                &ViewerAdmissionV4::new(),
            );
        })
        .unwrap();
    let visible = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
        .replace('│', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(visible.contains("Resize cancelled the original preparation."));
    assert!(visible.contains("Launch is not retried."));
    assert!(visible.contains("WHOLE ORIGINAL CAPTURED COMPARISON"));
    assert!(visible.contains("[Enter Launch]") && visible.contains("[Esc Back]"));
    assert_eq!(
        real.children().len(),
        children_before_draw,
        "draw performs no Git reads"
    );
    assert!(
        state.return_external_v4(40),
        "explicit Back is what asks the existing native worker to reproject"
    );
    project(&mut state, &workspace);
    assert_eq!(state.current_frame().unwrap().width(), 38);
    assert_eq!(observed(&real), before);
}
#[test]
fn native_external_diff_controller_tiny_chooser_cannot_hide_source_and_launch_any_child() {
    let (real, _, mut state) = loaded();
    let before = observed(&real);
    state.choose_external_v4(Rect::new(0, 0, 14, 4)).unwrap();
    let event = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let admission = ViewerAdmissionV4::new();
    assert!(matches!(
        state
            .external_v4
            .as_mut()
            .unwrap()
            .event(&event, true, &admission),
        ExternalInputV4::Captured
    ));
    assert!(state.pending().is_none());
    assert!(state.current_frame().is_some());
    assert!(!admission.blocked());
    assert_eq!(observed(&real), before);
}
#[test]
fn native_external_diff_controller_actual_mutable_source_change_is_error_not_selectable_optional_fallback(
) {
    let real = Real::new("sha256");
    let workspace = crate::workspace::Workspace::load(
        &real.root,
        crate::workspace::RepositoryMode::Working,
        "HEAD",
        None,
    )
    .unwrap()
    .unwrap();
    let mut state =
        NativeDiffStateV1::<crate::git::MutableDiffSourceV2>::open_mutable(&workspace, 80).unwrap();
    let first = state.request().unwrap();
    let first_op = operation(&workspace);
    let prepared = first.prepare(&first_op).unwrap();
    first_op.seal().unwrap();
    assert!(state.accept(prepared));
    let tools = tools(
        ExternalToolV4::Delta,
        "process.stdout.write('must-not-run');",
    );
    let launch = launch(&mut state, ExternalToolV4::Delta);
    let request = state.request().unwrap();
    fs::write(
        real.root.join("tracked"),
        b"actual externally replaced working bytes\n",
    )
    .unwrap();
    let after_user_edit = observed(&real);
    let op = operation(&workspace);
    let bounds = original_bounds(&op, &launch, &tools);
    let error = request.prepare_external_v4(&op, &bounds).err().unwrap();
    op.seal().unwrap();
    assert!(!tools.evidence.exists());
    assert!(state.fail(&request, &error));
    assert!(state.current_frame().is_none());
    assert!(state.capture_current_range_v2().is_err());
    assert!(state.status.contains("NOT CURRENT"));
    assert!(state.take_external_notice_v4().is_none());
    assert_eq!(observed(&real), after_user_edit);
}
#[test]
fn native_external_diff_controller_actual_binary_full_sides_refuse_tool_without_fabricating_text() {
    let real = Real::new("sha1");
    let old = f::commit(
        &real,
        &f::tree(&real, &[(b"binary", b"\0old actual bytes\n", "100644")]),
        None,
        "Binary-old",
    );
    let new = f::commit(
        &real,
        &f::tree(&real, &[(b"binary", b"\0new actual bytes\n", "100644")]),
        Some(&old),
        "Binary-new",
    );
    let mut workspace = f::workspace(
        &real,
        Comparison::Snapshots {
            base: old,
            head: new,
        },
    );
    f::source(&workspace).install(&mut workspace).unwrap();
    let mut state = NativeDiffStateV1::open(&workspace, 80).unwrap();
    project(&mut state, &workspace);
    let frame = state.current_frame().unwrap().clone();
    let before = observed(&real);
    let tools = tools(
        ExternalToolV4::Difftastic,
        "process.stdout.write('must-not-run');",
    );
    let launch = launch(&mut state, ExternalToolV4::Difftastic);
    let request = state.request().unwrap();
    let op = operation(&workspace);
    let bounds = original_bounds(&op, &launch, &tools);
    let fallback = request.prepare_external_v4(&op, &bounds).unwrap();
    op.seal().unwrap();
    assert!(!tools.evidence.exists());
    assert!(state.accept(fallback));
    assert!(Arc::ptr_eq(state.current_frame().unwrap(), &frame));
    assert!(state.range().is_none());
    assert!(state
        .take_external_notice_v4()
        .unwrap()
        .contains("special content stays native"));
    assert_eq!(observed(&real), before);
}
