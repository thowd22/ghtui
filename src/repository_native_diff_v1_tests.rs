use super::*;
use crate::{
    diff::LineKind, git::Comparison, original_diff_sides_v1_test_support as fixture,
    providers::runner::CancellationToken,
};

fn loaded() -> (
    crate::local_commit_actor_origin_v1_test_support::Real,
    Workspace,
    String,
    String,
    String,
) {
    let (real, a, b, c) = fixture::fixture("sha256");
    let mut workspace = fixture::workspace(
        &real,
        Comparison::BranchChanges {
            base: a.clone(),
            head: b.clone(),
        },
    );
    fixture::source(&workspace).install(&mut workspace).unwrap();
    (real, workspace, a, b, c)
}
fn shared(workspace: &Workspace) -> NativeDiffStateV1 {
    let mut state = NativeDiffStateV1::open(workspace, 118).unwrap();
    state.file = workspace
        .original_comparison_v1
        .as_ref()
        .unwrap()
        .members()
        .iter()
        .find(|member| {
            member
                .path(crate::git::OriginalComparisonSideV1::New)
                .is_some_and(|path| path.0 == b"shared")
        })
        .unwrap()
        .index();
    state
}
fn publish(state: &mut NativeDiffStateV1) {
    let request = state.request().unwrap();
    let operation =
        LocalGitNavigationReadV1::begin(request.intent().origin(), Default::default()).unwrap();
    let prepared = request.prepare(&operation).unwrap();
    assert_eq!(operation.seal().unwrap().children_started(), 0);
    assert!(state.accept(prepared));
}
fn ticket(state: &NativeDiffStateV1, kind: LineKind) -> NativeDiffLineTicketV1 {
    let frame = state.current_frame().unwrap();
    (0..frame.rows().len())
        .flat_map(|row| [frame.ticket(row, false), frame.ticket(row, true)])
        .flatten()
        .find(|ticket| ticket.key().canonical().kind == kind)
        .unwrap()
}

#[test]
fn native_diff_controller_cached_original_actual_c_source_reuses_arc_zero_git_no_checkout() {
    let (real, workspace, a, b, c) = loaded();
    let source = workspace.original_comparison_v1.as_ref().unwrap();
    let before = real.snapshot();
    real.clear_trace();
    let mut state = shared(&workspace);
    publish(&mut state);
    assert_eq!(state.intent().requested_base(), Some(a.as_str()));
    assert_eq!(state.intent().content_base(), c);
    assert_eq!(state.intent().head(), b);
    assert!(Arc::ptr_eq(state.source().unwrap(), source));
    assert!(Arc::ptr_eq(state.current_frame().unwrap().source(), source));
    let actors = state.actors().unwrap().clone();
    assert!(Arc::ptr_eq(&actors.source, source));
    assert!(actors.author_label.contains("Target Writer"));
    assert!(actors.committer_label.contains("Target Recorder"));
    assert!(actors.author_label.contains("[local Git]"));
    assert!(actors.author.login.is_none() && actors.committer.login.is_none());
    assert!(actors.author.avatar_url.is_none() && actors.committer.avatar_url.is_none());
    state.query.insert("new").unwrap();
    publish(&mut state);
    assert!(
        Arc::ptr_eq(state.actors().unwrap(), &actors),
        "query preparation retains genuine bounded role labels, not another actor/source clone"
    );
    assert!(real.children().is_empty());
    assert_eq!(real.snapshot(), before);
}

#[test]
fn native_diff_controller_original_query_width_and_new_session_reject_late_prepared_without_effects(
) {
    let (real, workspace, _, _, _) = loaded();
    let mut state = shared(&workspace);
    let first = state.request().unwrap();
    let operation =
        LocalGitNavigationReadV1::begin(first.intent().origin(), Default::default()).unwrap();
    let prepared = first.prepare(&operation).unwrap();
    operation.seal().unwrap();
    state.query.insert("new").unwrap();
    state.width = 38;
    let current = state.request().unwrap();
    assert!(!state.accept(prepared));
    assert!(state.accepts(&current));
    assert!(state.frame().is_none());
    assert!(!state.fail(&first, "old captured panic"));
    let mut foreign = shared(&workspace);
    let foreign_request = foreign.request().unwrap();
    assert!(!Arc::ptr_eq(&foreign_request.session, &current.session));
    assert!(!foreign.accepts(&current));
    let before = real.snapshot();
    real.clear_trace();
    state.width = 118;
    publish(&mut state);
    assert_eq!(state.current_frame().unwrap().width(), 118);
    assert_eq!(state.current_frame().unwrap().query().text(), "new");
    assert!(real.children().is_empty());
    assert_eq!(real.snapshot(), before);
}

#[test]
fn native_diff_controller_current_pending_pointer_cannot_hide_new_query_width_options_or_range_input(
) {
    let (_real, workspace, _, _, _) = loaded();
    for input in 0..6 {
        let mut state = shared(&workspace);
        publish(&mut state);
        let request = state.request().unwrap();
        match input {
            0 => state.query.insert("new").unwrap(),
            1 => state.width = 38,
            2 => state.file = 0,
            3 => state.option(NativeDiffOptionChangeV1::Layout).unwrap(),
            4 => state.clear_selection(),
            _ => state.regex = !state.regex,
        }
        assert!(Arc::ptr_eq(state.pending().unwrap(), &request));
        assert!(
            !state.accepts(&request),
            "Current pending pointer alone is not input-epoch authority"
        );
        assert!(state.current_frame().is_none());
    }
}

#[test]
fn native_diff_controller_invalid_replacement_retires_pending_and_old_interactive_width_epoch() {
    let (_real, workspace, _, _, _) = loaded();
    let mut state = shared(&workspace);
    publish(&mut state);
    let old_ticket = ticket(&state, LineKind::Added);
    let request = state.request().unwrap();
    state.width = 6;
    assert!(state.request().is_err());
    assert!(state.pending().is_none());
    assert!(!state.accepts(&request));
    assert!(state.frame().is_some() && state.current_frame().is_none());
    assert!(state.select(old_ticket, false).is_err());
    state.width = 118;
    state.query.text = "x".repeat(257);
    assert!(state.request().is_err());
    assert!(state.current_frame().is_none());
}

#[test]
fn native_diff_controller_unified_original_range_survives_hidden_generated_query_split_and_file_focus(
) {
    let (real, workspace, _, _, _) = loaded();
    let mut state = shared(&workspace);
    publish(&mut state);
    let first = ticket(&state, LineKind::Removed);
    state.select(first, false).unwrap();
    publish(&mut state);
    let next = ticket(&state, LineKind::Added);
    state.select(next, true).unwrap();
    publish(&mut state);
    let original = state.range().unwrap().clone();
    assert_eq!(original.changed().len(), 2);
    assert_eq!(original.side(), DiffSide::Both);
    let before = real.snapshot();
    real.clear_trace();
    state
        .option(NativeDiffOptionChangeV1::MarkGenerated(state.file))
        .unwrap();
    state
        .option(NativeDiffOptionChangeV1::CollapseGenerated)
        .unwrap();
    state.option(NativeDiffOptionChangeV1::Wrap).unwrap();
    state.option(NativeDiffOptionChangeV1::Layout).unwrap();
    state.query.insert("TARGET").unwrap();
    state.width = 38;
    publish(&mut state);
    assert_eq!(state.range().unwrap().changed(), original.changed());
    assert_eq!(state.current_frame().unwrap().hidden_selected(), 2);
    state.file = 0;
    assert_ne!(state.file, original.file());
    publish(&mut state);
    assert_eq!(state.range().unwrap().file(), original.file());
    assert_eq!(state.current_frame().unwrap().hidden_selected(), 2);
    assert!(Arc::ptr_eq(
        state.range().unwrap().source(),
        original.source()
    ));
    assert!(real.children().is_empty());
    assert_eq!(real.snapshot(), before);
}

#[test]
fn native_diff_controller_generated_mark_targets_captured_file_not_new_focus() {
    let (_real, workspace, _, _, _) = loaded();
    let mut state = shared(&workspace);
    publish(&mut state);
    let original = state.file;
    state
        .option(NativeDiffOptionChangeV1::MarkGenerated(original))
        .unwrap();
    state.file = 0;
    assert_ne!(state.file, original);
    publish(&mut state);
    assert!(state
        .current_frame()
        .unwrap()
        .options()
        .marked_generated
        .contains(&original));
    assert!(!state
        .current_frame()
        .unwrap()
        .options()
        .marked_generated
        .contains(&0));
}

#[test]
fn native_diff_controller_actual_cancellation_and_completed_original_operation_zero_child_refuse() {
    let (real, workspace, _, _, _) = loaded();
    let mut state = shared(&workspace);
    let request = state.request().unwrap();
    real.clear_trace();
    let token = CancellationToken::default();
    let operation =
        LocalGitNavigationReadV1::begin(request.intent().origin(), token.clone()).unwrap();
    token.cancel();
    assert!(request.prepare(&operation).is_err());
    assert_eq!(operation.accounting().unwrap().children_started(), 0);
    let operation =
        LocalGitNavigationReadV1::begin(request.intent().origin(), Default::default()).unwrap();
    operation.seal().unwrap();
    assert!(request.prepare(&operation).is_err());
    assert!(real.children().is_empty());
}

#[test]
fn native_diff_controller_stale_epoch_and_pending_hunk_cannot_select_neighbour_or_new_source() {
    let (_real, workspace, _, _, _) = loaded();
    let mut state = shared(&workspace);
    publish(&mut state);
    let old = ticket(&state, LineKind::Added);
    state.width = 38;
    publish(&mut state);
    assert!(state.select(old, false).is_err());
    let current = ticket(&state, LineKind::Added);
    state.select_hunk(current.key().hunk()).unwrap();
    let pending = state.request().unwrap();
    assert!(state.select(current, false).is_err());
    assert!(state.accepts(&pending));
}

#[test]
fn native_diff_controller_saved_factory_refuses_actual_mutable_working_without_any_child() {
    let (real, _, _, _, _) = loaded();
    let working = Workspace::load(
        &real.root,
        crate::workspace::RepositoryMode::Working,
        &real.head,
        None,
    )
    .unwrap()
    .unwrap();
    let before = real.snapshot();
    real.clear_trace();
    assert!(NativeDiffStateV1::open(&working, 118).is_err());
    assert!(real.children().is_empty());
    assert_eq!(real.snapshot(), before);
}
