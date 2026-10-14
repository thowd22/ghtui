use super::*;
use crate::{
    git::original_tree_inventory_v1::tests::fixture_tree,
    local_commit_actor_origin_v1_test_support::Real,
};

fn fixture() -> (Real, OriginalTreeJumpV1) {
    let real = Real::new("sha256");
    let workspace = fixture_tree(
        &real,
        &[
            b"alpha.rs",
            b"alphabets.rs",
            b"case.rs",
            b"same-\xfe",
            b"same-\xff",
            b"long shared prefix one.rs",
            b"long shared prefix two.rs",
        ],
    );
    let intent = OriginalTreeIntentV1::capture(&workspace).unwrap();
    let state = OriginalTreeJumpV1::new(intent);
    (real, state)
}
fn prepare(
    state: &mut OriginalTreeJumpV1,
    request: Arc<OriginalTreeRequestV1>,
) -> Arc<OriginalTreeFrameV1> {
    let op =
        LocalGitNavigationReadV1::begin(request.intent().origin(), Default::default()).unwrap();
    let frame = OriginalTreeFrameV1::prepare(request, &op).unwrap();
    op.seal().unwrap();
    assert!(state.accept(frame.clone()));
    frame
}
#[test]
fn original_tree_jump_old_ticket_same_member_new_query_refuses_with_zero_filter_children() {
    let (real, mut state) = fixture();
    let request = state.request().unwrap();
    let frame = prepare(&mut state, request);
    let ticket = frame.ticket(0).unwrap();
    let first = ticket.index();
    real.clear_trace();
    let effect = state
        .apply(OriginalTreeJumpActionV1::Insert("alpha".into()))
        .unwrap();
    assert!(state
        .apply(OriginalTreeJumpActionV1::Activate(ticket.clone()))
        .is_err());
    let OriginalTreeJumpEffectV1::Request(request) = effect else {
        panic!("Query must use actual worker request")
    };
    let frame = prepare(&mut state, request);
    assert!(frame.matches().iter().any(|row| row.index() == first));
    assert!(state
        .apply(OriginalTreeJumpActionV1::Activate(ticket))
        .is_err());
    assert!(real.children().is_empty());
}
#[test]
fn original_tree_jump_query_paste_selection_caret_graphemes_and_raw_bounds() {
    let (_real, mut state) = fixture();
    let request = state.request().unwrap();
    prepare(&mut state, request);
    state
        .apply(OriginalTreeJumpActionV1::Paste("café👩‍👩‍👧".into()))
        .unwrap();
    state.apply(OriginalTreeJumpActionV1::Backspace).unwrap();
    assert_eq!(state.query.text, "café");
    state.apply(OriginalTreeJumpActionV1::SelectAll).unwrap();
    state
        .apply(OriginalTreeJumpActionV1::Insert("alpha".into()))
        .unwrap();
    assert_eq!(state.query.text, "alpha");
    let before = state.query.text.clone();
    assert!(state
        .apply(OriginalTreeJumpActionV1::Paste("x".repeat(257)))
        .is_err());
    assert_eq!(state.query.text, before);
    assert!(state
        .apply(OriginalTreeJumpActionV1::Paste("bad\nname".into()))
        .is_err());
    state
        .apply(OriginalTreeJumpActionV1::Edge {
            end: false,
            select: false,
        })
        .unwrap();
    assert_eq!(state.query.cursor, 0);
}
#[test]
fn original_tree_jump_rejected_input_notice_survives_same_query_worker_acceptance_until_valid_new_input(
) {
    let (real, mut state) = fixture();
    let first = state.request().unwrap();
    prepare(&mut state, first);
    real.clear_trace();
    let OriginalTreeJumpEffectV1::Request(request) = state
        .apply(OriginalTreeJumpActionV1::Insert("3".into()))
        .unwrap()
    else {
        panic!("actual pending typed query required");
    };
    let operation =
        LocalGitNavigationReadV1::begin(request.intent().origin(), Default::default()).unwrap();
    let prepared = OriginalTreeFrameV1::prepare(request, &operation).unwrap();
    operation.seal().unwrap();
    let reason = state
        .apply(OriginalTreeJumpActionV1::Paste("x".repeat(257)))
        .err()
        .expect("oversized input must be rejected");
    assert_eq!(state.query.text, "3");
    assert!(state.accept(prepared));
    assert_eq!(state.status, reason);
    assert!(
        state.status.contains("limited to256 raw bytes") && state.status.contains("rejected input")
    );
    state.apply(OriginalTreeJumpActionV1::SelectAll).unwrap();
    let OriginalTreeJumpEffectV1::Request(request) = state
        .apply(OriginalTreeJumpActionV1::Insert("alpha".into()))
        .unwrap()
    else {
        panic!("valid correction requires actual new query");
    };
    prepare(&mut state, request);
    assert!(state.status.contains("known matches"));
    assert!(!state.status.contains("rejected input"));
    assert!(real.children().is_empty());
}
#[test]
fn original_tree_jump_full_name_detail_and_raw_identity_never_display_position_alias() {
    let (_real, mut state) = fixture();
    let request = state.request().unwrap();
    let frame = prepare(&mut state, request);
    for row in frame.matches() {
        let joined = row.detail().join("");
        let member = &frame.inventory().members()[row.index()];
        assert!(joined.contains(&format!("Exact saved path: {}", member.display())));
        assert!(joined.contains(member.object_id()));
    }
    let a = frame
        .matches()
        .iter()
        .position(|row| frame.inventory().members()[row.index()].path().0 == b"same-\xfe")
        .unwrap();
    let b = frame
        .matches()
        .iter()
        .position(|row| frame.inventory().members()[row.index()].path().0 == b"same-\xff")
        .unwrap();
    let ta = frame.ticket(a).unwrap();
    let tb = frame.ticket(b).unwrap();
    assert_ne!(ta, tb);
    assert_ne!(ta.member().path(), tb.member().path());
    assert!(matches!(
        state.apply(OriginalTreeJumpActionV1::Activate(tb)),
        Ok(OriginalTreeJumpEffectV1::Open(_))
    ));
}
#[test]
fn original_tree_viewport_escaped_raw_search_keys_resolve_exact_genuine_member_with_zero_git_reads()
{
    let (real, mut state) = fixture();
    let initial = state.request().unwrap();
    prepare(&mut state, initial);
    real.clear_trace();
    for (query, path) in [
        ("same-\\xff", b"same-\xff".as_slice()),
        ("same-\\xfe", b"same-\xfe".as_slice()),
    ] {
        state.apply(OriginalTreeJumpActionV1::SelectAll).unwrap();
        let OriginalTreeJumpEffectV1::Request(request) = state
            .apply(OriginalTreeJumpActionV1::Paste(query.into()))
            .unwrap()
        else {
            panic!("actual new query request required");
        };
        let frame = prepare(&mut state, request);
        assert_eq!(frame.known_matches(), 1);
        assert_eq!(frame.ticket(0).unwrap().member().path().0, path);
        assert_eq!(
            frame.inventory().members()[frame.matches()[0].index()].matching_text(),
            query
        );
    }
    assert!(real.children().is_empty());
}
#[test]
fn original_tree_jump_stale_request_late_failure_and_close_do_not_replace_current_frame() {
    let (_real, mut state) = fixture();
    let old = state.request().unwrap();
    let current = state.request().unwrap();
    let op = LocalGitNavigationReadV1::begin(old.intent().origin(), Default::default()).unwrap();
    let old_frame = OriginalTreeFrameV1::prepare(old.clone(), &op).unwrap();
    op.seal().unwrap();
    assert!(!state.accept(old_frame));
    assert!(!state.fail(&old, "late wrong source"));
    let frame = prepare(&mut state, current);
    assert!(Arc::ptr_eq(state.frame().unwrap(), &frame));
    state.apply(OriginalTreeJumpActionV1::Close).unwrap();
    assert!(state.frame().is_none());
    assert!(!state.fail(&old, "late closed"));
}
#[test]
fn original_tree_jump_no_match_unknown_absence_focus_and_navigation_stay_honest() {
    let (real, mut state) = fixture();
    let request = state.request().unwrap();
    prepare(&mut state, request);
    real.clear_trace();
    let OriginalTreeJumpEffectV1::Request(request) = state
        .apply(OriginalTreeJumpActionV1::Paste("no-such-known-path".into()))
        .unwrap()
    else {
        panic!("Worker required")
    };
    let frame = prepare(&mut state, request);
    assert!(frame.matches().is_empty());
    assert!(state.apply(OriginalTreeJumpActionV1::Choose).is_err());
    assert!(real.children().is_empty());
    state
        .apply(OriginalTreeJumpActionV1::FocusNext(false))
        .unwrap();
    assert_eq!(state.focus, OriginalTreeFocusV1::List);
}
