//! NEW child of actual local owner. Real Git/pool arithmetic, no clock renewal.
use super::*;
use crate::{
    local_commit_actor_origin_v1_test_support::Real,
    repository_graph_v1::fixture_v2::ordinary,
    repository_graph_v1::GraphActionV1,
    workspace::{RepositoryMode, RowAction, Workspace},
};

#[test]
fn commit_graph_v2_owner_rejects_projection_position_change_before_success_or_error() {
    let real = Real::new("sha1");
    let tip = ordinary(&real, 430);
    let mut workspace = Workspace::load(&real.root, RepositoryMode::Graph, &tip, None)
        .unwrap()
        .unwrap();
    workspace
        .open(RowAction::Graph(GraphActionV1::More))
        .unwrap();
    let owner = OriginalLocalNavigationOwnerV1::begin(&workspace, Default::default()).unwrap();
    let state = workspace.graph_state_v1.clone().unwrap();
    let display = workspace.graph_display_v1.clone().unwrap();
    assert!(owner.can_admit(&workspace, true));
    real.clear_trace();
    workspace.graph_projection_start_v2 = 200;
    assert!(Arc::ptr_eq(
        &state,
        workspace.graph_state_v1.as_ref().unwrap()
    ));
    assert!(Arc::ptr_eq(
        &display,
        workspace.graph_display_v1.as_ref().unwrap()
    ));
    assert!(
        !owner.can_admit(&workspace, true),
        "Projection position is independently owned"
    );
    assert!(
        !owner.can_admit(&workspace, false),
        "Late errors cannot alter a changed position"
    );
    assert!(real.children().is_empty());
}

#[test]
fn commit_graph_v2_each_explicit_more_has_distinct_prequeue_token_clock_six_measured_children() {
    let real = Real::new("sha1");
    let tip = ordinary(&real, 630);
    let initial = Workspace::load(&real.root, RepositoryMode::Graph, &tip, None)
        .unwrap()
        .unwrap();
    let old_token = CancellationToken::default();
    let first = OriginalLocalNavigationOwnerV1::begin(&initial, old_token.clone()).unwrap();
    let first_deadline = first.operation().deadline();
    let mut next = initial.clone();
    real.clear_trace();
    next.open_original_with_operation_v1(RowAction::Graph(GraphActionV1::More), first.operation())
        .unwrap();
    let count = first.operation().seal().unwrap();
    assert_eq!(count.children_started(), 6);
    assert_eq!(count.requests_claimed(), 6);
    assert_eq!(count.max_requests(), 128);
    assert!(count.charged_bytes() < 64 * 1024 * 1024);
    assert_eq!(real.children().len(), 6);
    assert_eq!(next.graph_state_v1.as_ref().unwrap().loaded(), 400);
    assert_eq!(first.operation().deadline(), first_deadline);
    assert!(first.operation().check().unwrap_err().contains("completed"));
    assert!(first.matches_original(&initial));
    assert!(!first.matches_original(&next));
    // This is a NEW deliberate request, not a helper rebuilding the first pool.
    let actual_new_token = CancellationToken::default();
    let second = OriginalLocalNavigationOwnerV1::begin(&next, actual_new_token.clone()).unwrap();
    assert!(!Arc::ptr_eq(first.operation(), second.operation()));
    assert!(second.operation().deadline() > first_deadline);
    assert_eq!(
        second.operation().accounting().unwrap().children_started(),
        0
    );
    old_token.cancel();
    assert!(first.operation().check().is_err());
    assert!(second.operation().check().is_ok());
    let second_deadline = second.operation().deadline();
    real.clear_trace();
    next.open_original_with_operation_v1(RowAction::Graph(GraphActionV1::More), second.operation())
        .unwrap();
    assert_eq!(next.graph_state_v1.as_ref().unwrap().loaded(), 600);
    assert_eq!(second.operation().deadline(), second_deadline);
    let measured = second.operation().seal().unwrap();
    assert_eq!(measured.children_started(), 6);
    assert_eq!(real.children().len(), 6);
    assert!(second.operation().check().is_err());
    actual_new_token.cancel();
    let prior = real.children().len();
    assert!(next
        .open_original_with_operation_v1(RowAction::Graph(GraphActionV1::More), second.operation())
        .is_err());
    assert_eq!(
        real.children().len(),
        prior,
        "Old completed/canceled operation cannot spawn or be renewed"
    );
}
