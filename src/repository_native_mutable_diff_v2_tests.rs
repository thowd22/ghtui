use super::*;
use crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1;
use crate::{
    diff::LineKind,
    git::{MutableDiffKindV2, MutableDiffSourceV2},
    identity::ActorKind,
    local_commit_actor_origin_v1_test_support::{git, Real},
    workspace::RepositoryMode,
};

type State = NativeDiffStateV1<MutableDiffSourceV2>;
fn loaded(real: &Real, mode: RepositoryMode) -> Workspace {
    Workspace::load(&real.root, mode, "HEAD", None)
        .unwrap()
        .unwrap()
}
fn publish(state: &mut State, real: &Real) -> usize {
    let request = state.request().unwrap();
    let op =
        LocalGitNavigationReadV1::begin(request.intent().origin(), Default::default()).unwrap();
    real.clear_trace();
    let prepared = request.prepare(&op).unwrap();
    let count = real.children().len();
    assert_eq!(op.seal().unwrap().children_started(), count);
    assert!(state.accept(prepared));
    count
}
fn ticket(state: &State, kind: LineKind) -> NativeDiffLineTicketV1<MutableDiffSourceV2> {
    let frame = state.current_frame().unwrap();
    (0..frame.rows().len())
        .flat_map(|row| [frame.ticket(row, false), frame.ticket(row, true)])
        .flatten()
        .find(|ticket| ticket.key().canonical().kind == kind)
        .unwrap()
}

#[test]
fn native_mutable_diff_core_actual_working_and_staged_generic_rows_no_saved_or_actor_mint() {
    for mode in [RepositoryMode::Working, RepositoryMode::Staged] {
        let real = Real::new("sha256");
        let workspace = loaded(&real, mode);
        let before = real.snapshot();
        real.clear_trace();
        let mut state = State::open_mutable(&workspace, 118).unwrap();
        assert!(state.owns(&workspace));
        assert!(state.source().is_none() && real.children().is_empty());
        assert_eq!(
            publish(&mut state, &real),
            if mode == RepositoryMode::Working {
                22
            } else {
                23
            }
        );
        let source = state.source().unwrap();
        assert!(Arc::ptr_eq(source, state.current_frame().unwrap().source()));
        assert_eq!(
            source.result().observed,
            workspace.comparison.as_ref().unwrap().observed
        );
        assert_eq!(
            source.members()[0].path(DiffSide::New).unwrap().0,
            b"tracked"
        );
        assert!(source.members()[0].path(DiffSide::Both).is_none());
        let old = ticket(&state, LineKind::Removed);
        let new = ticket(&state, LineKind::Added);
        assert!(Arc::ptr_eq(old.key().source(), source) && Arc::ptr_eq(new.key().source(), source));
        let patch = String::from_utf8_lossy(&source.result().patch);
        if mode == RepositoryMode::Working {
            assert_eq!(source.intent().kind(), MutableDiffKindV2::Working);
            assert!(patch.contains("-staged bytes") && patch.contains("+dirty bytes"));
            assert!(source.result().base.is_none() && source.result().head.is_none());
        } else {
            assert_eq!(source.intent().kind(), MutableDiffKindV2::Staged);
            assert!(patch.contains("-saved original bytes") && patch.contains("+staged bytes"));
            assert_eq!(source.result().base.as_deref(), Some(real.head.as_str()));
            assert!(source.result().head.is_none());
        }
        let actors = state.actors().unwrap();
        for actor in [&actors.author, &actors.committer] {
            assert_eq!(actor.kind, ActorKind::Unknown);
            assert!(
                actor.git_role.is_none()
                    && actor.login.is_none()
                    && actor.email.is_none()
                    && actor.avatar_url.is_none()
                    && !actor.host_associated
            );
        }
        assert!(actors.author_label.contains("author unknown"));
        assert!(!actors.author_label.contains("Later Writer"));
        assert!(actors.header_label.contains("observed snapshot"));
        assert!(workspace.original_comparison_v1.is_none());
        assert_eq!(real.snapshot(), before);
    }
}

#[test]
fn native_mutable_diff_core_cached_projection_fresh_checks_exact_range_same_arc_no_writes() {
    let real = Real::new("sha1");
    let workspace = loaded(&real, RepositoryMode::Working);
    let before = real.snapshot();
    let mut state = State::open_mutable(&workspace, 118).unwrap();
    publish(&mut state, &real);
    let source = state.source().unwrap().clone();
    let actors = state.actors().unwrap().clone();
    state
        .select(ticket(&state, LineKind::Removed), false)
        .unwrap();
    assert_eq!(publish(&mut state, &real), 10);
    state.select(ticket(&state, LineKind::Added), true).unwrap();
    assert_eq!(publish(&mut state, &real), 10);
    let original = state.range().unwrap().clone();
    assert_eq!(original.side(), DiffSide::Both);
    assert_eq!(original.changed(), &[(0, 0), (0, 1)]);
    assert!(Arc::ptr_eq(original.source(), &source));
    for option in [
        NativeDiffOptionChangeV1::Layout,
        NativeDiffOptionChangeV1::Wrap,
        NativeDiffOptionChangeV1::MarkGenerated(0),
        NativeDiffOptionChangeV1::CollapseGenerated,
    ] {
        state.option(option).unwrap();
        state.width = 38;
        assert_eq!(publish(&mut state, &real), 10);
        assert!(Arc::ptr_eq(state.source().unwrap(), &source));
        assert!(Arc::ptr_eq(state.actors().unwrap(), &actors));
        assert!(Arc::ptr_eq(state.range().unwrap(), &original));
    }
    assert_eq!(state.current_frame().unwrap().hidden_selected(), 2);
    assert_eq!(real.snapshot(), before);
}

#[test]
fn native_mutable_diff_core_stale_worktree_index_ref_and_untracked_refuse_cached_epoch() {
    for change in 0..4 {
        let real = Real::new("sha1");
        let workspace = loaded(&real, RepositoryMode::Working);
        let mut state = State::open_mutable(&workspace, 118).unwrap();
        publish(&mut state, &real);
        let source = state.source().unwrap().clone();
        state.query.insert("dirty").unwrap();
        let request = state.request().unwrap();
        match change {
            0 => std::fs::write(real.root.join("tracked"), b"external tracked\n").unwrap(),
            1 => {
                git(&real.root, &["add", "--", "tracked"], None);
            }
            2 => {
                git(&real.root, &["tag", "external-ref"], None);
            }
            _ => std::fs::write(real.root.join("scratch"), b"external scratch\n").unwrap(),
        }
        let before = real.snapshot();
        real.clear_trace();
        let op =
            LocalGitNavigationReadV1::begin(request.intent().origin(), Default::default()).unwrap();
        let error = request
            .prepare(&op)
            .err()
            .expect("fresh read must reject changed actual state");
        assert!(error.contains("stale"), "{error}");
        assert_eq!(op.seal().unwrap().children_started(), 5);
        assert!(state.fail(&request, &error));
        assert!(state.current_frame().is_none() && state.pending().is_none());
        assert!(Arc::ptr_eq(state.source().unwrap(), &source));
        assert!(state.range().is_none());
        assert_eq!(real.snapshot(), before);
    }
}

#[test]
fn native_mutable_diff_core_late_query_width_option_or_new_session_rejects_without_effects() {
    let real = Real::new("sha1");
    let workspace = loaded(&real, RepositoryMode::Staged);
    for change in 0..4 {
        let mut state = State::open_mutable(&workspace, 118).unwrap();
        let request = state.request().unwrap();
        let op =
            LocalGitNavigationReadV1::begin(request.intent().origin(), Default::default()).unwrap();
        let prepared = request.prepare(&op).unwrap();
        op.seal().unwrap();
        match change {
            0 => state.query.insert("staged").unwrap(),
            1 => state.width = 38,
            2 => state.option(NativeDiffOptionChangeV1::Layout).unwrap(),
            _ => state = State::open_mutable(&workspace, 118).unwrap(),
        }
        let before = real.snapshot();
        real.clear_trace();
        assert!(!state.accept(prepared));
        assert!(!state.fail(&request, "late panic"));
        assert!(state.source().is_none() && state.frame().is_none() && state.range().is_none());
        assert!(real.children().is_empty());
        assert_eq!(real.snapshot(), before);
    }
}

#[test]
fn native_mutable_diff_core_genuine_complete_empty_source_has_no_row_or_range_authority() {
    let real = Real::new("sha1");
    git(&real.root, &["add", "--", "tracked"], None);
    let workspace = loaded(&real, RepositoryMode::Working);
    let before = real.snapshot();
    let mut state = State::open_mutable(&workspace, 118).unwrap();
    publish(&mut state, &real);
    let source = state.source().unwrap();
    assert!(source.complete() && source.observed_members() == 0 && source.members().is_empty());
    assert!(state.metadata_note().unwrap().contains("No changed files"));
    assert!(state.current_frame().is_none() && state.range().is_none());
    assert!(state.select_hunk(0).is_err());
    assert!(state.actors().unwrap().author.git_role.is_none());
    assert_eq!(real.snapshot(), before);
}

#[test]
fn native_mutable_diff_core_bad_regex_retains_source_not_current_hits_or_new_range() {
    let real = Real::new("sha1");
    let workspace = loaded(&real, RepositoryMode::Working);
    let before = real.snapshot();
    let mut state = State::open_mutable(&workspace, 118).unwrap();
    publish(&mut state, &real);
    state
        .select(ticket(&state, LineKind::Added), false)
        .unwrap();
    publish(&mut state, &real);
    let range = state.range().unwrap().clone();
    let frame = state.current_frame().unwrap().clone();
    state.regex = true;
    state.query.insert("[").unwrap();
    publish(&mut state, &real);
    assert!(state.current_frame().is_none());
    assert!(Arc::ptr_eq(state.frame().unwrap(), &frame));
    assert!(Arc::ptr_eq(state.range().unwrap(), &range));
    assert!(state.metadata_note().unwrap().contains("Error:"));
    state.query.clear();
    publish(&mut state, &real);
    assert!(state.current_frame().is_some());
    assert!(Arc::ptr_eq(state.range().unwrap(), &range));
    assert_eq!(real.snapshot(), before);
}
