//! Undeclared child of navigation_local_owner_v1: actual opaque graph pages.
use super::*;
use crate::{
    git::OriginalGraphReadV1,
    local_commit_actor_origin_v1_test_support::Real,
    repository_graph_v1::OriginalGraphStateV1,
    workspace::{RepositoryMode, RowAction},
};
#[test]
fn commit_graph_v1_owner_rejects_equal_graph_text_under_a_different_actual_graph_arc() {
    let real = Real::new("sha1");
    let w = Workspace::load(&real.root, RepositoryMode::Graph, &real.original, None)
        .unwrap()
        .unwrap();
    let owner = OriginalLocalNavigationOwnerV1::begin(&w, Default::default()).unwrap();
    assert!(owner.can_admit(&w, true));
    let origin = w.actor_origin.as_ref().unwrap();
    let operation = LocalGitNavigationReadV1::begin(origin, Default::default()).unwrap();
    let page = origin
        .with_saved_navigation_read_v1(&operation, |repository| {
            repository
                .original_graph_page_v1(
                    OriginalGraphReadV1::Start(&real.original),
                    operation.deadline(),
                )
                .map_err(|e| e.to_string())
        })
        .unwrap();
    operation.seal().unwrap();
    let second = OriginalGraphStateV1::start(origin.clone(), page).unwrap();
    assert_eq!(
        second.walk_tip(),
        w.graph_state_v1.as_ref().unwrap().walk_tip()
    );
    let original = w.graph_state_v1.as_ref().unwrap();
    assert_eq!(second.topology_v2().rows(), original.topology_v2().rows());
    assert_eq!(second.loaded(), original.loaded());
    for index in 0..original.loaded() {
        assert_eq!(second.member_at_v2(index), original.member_at_v2(index));
    }
    let mut replacement = w.clone();
    replacement.graph_state_v1 = Some(second);
    assert!(Arc::ptr_eq(
        replacement.actor_origin.as_ref().unwrap(),
        w.actor_origin.as_ref().unwrap()
    ));
    assert_eq!(replacement.revision, w.revision);
    assert!(!owner.matches_original(&replacement));
    assert!(!owner.can_admit(&replacement, false));
    assert!(!owner.can_admit(&replacement, true));
}
#[test]
fn commit_graph_v1_owner_cancel_or_loaded_selection_loss_refuses_both_success_and_error() {
    let real = Real::new("sha256");
    let mut w = Workspace::load(&real.root, RepositoryMode::Graph, &real.original, None)
        .unwrap()
        .unwrap();
    let token = CancellationToken::default();
    let owner = OriginalLocalNavigationOwnerV1::begin(&w, token.clone()).unwrap();
    let parent = w.selected_commit.as_ref().unwrap().parents[0].clone();
    w.open(RowAction::Commit(parent)).unwrap();
    assert!(!owner.can_admit(&w, false));
    assert!(!owner.can_admit(&w, true));
    let current = OriginalLocalNavigationOwnerV1::begin(&w, token.clone()).unwrap();
    assert!(current.matches_original(&w));
    token.cancel();
    assert!(!current.matches_original(&w));
    assert!(!current.can_admit(&w, false));
    assert!(!current.can_admit(&w, true));
}
