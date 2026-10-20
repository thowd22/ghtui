use super::*;
use crate::local_commit_actor_origin_v1_test_support::Real;
use crate::repository_graph_v1::fixture_v2::ordinary;
use crate::repository_graph_v1::{projection_v2::GraphWindowActionV2, GraphViewV1};
use crate::workspace::RepositoryMode;

#[test]
fn commit_graph_v2_registry_position_update_does_not_mint_generation_discard_is_graph_only() {
    let real = Real::new("sha1");
    let tip = ordinary(&real, 230);
    let ordinary = Workspace::load(&real.root, RepositoryMode::Overview, &tip, None)
        .unwrap()
        .unwrap();
    let normal = OriginalSavedVisitV1::capture(&ordinary).unwrap();
    let mut visits = OriginalSavedVisitsV1::default();
    visits.remember_checked_v2(normal.clone()).unwrap();
    let mut graph = Workspace::load(&real.root, RepositoryMode::Graph, &tip, None)
        .unwrap()
        .unwrap();
    let first = visits.capture_graph_v2(&graph).unwrap();
    visits.remember_checked_v2(first.clone()).unwrap();
    let position = graph.graph_position_v2().unwrap();
    let pending = first.graph_request_position_v2(&position).unwrap();
    graph.graph_view_v1 = GraphViewV1::Details;
    graph.graph_horizontal_v1 = 8;
    let changed = visits.capture_graph_v2(&graph).unwrap();
    assert_eq!(first.key(), changed.key());
    assert!(Arc::ptr_eq(
        first.graph_lease_v2().unwrap(),
        changed.graph_lease_v2().unwrap()
    ));
    visits.remember_checked_v2(changed).unwrap();
    assert_eq!(visits.entries.len(), 2);
    assert_eq!(pending.position().view, GraphViewV1::Timeline);
    assert_eq!(pending.position().horizontal, 0);
    let before = visits
        .graph_retention_v2
        .as_ref()
        .unwrap()
        .accounting()
        .unwrap();
    assert_eq!(visits.discard_graph_back_v2(), 1);
    assert!(visits.find(normal.key(), &real.root).is_some());
    assert!(visits.find(first.key(), &real.root).is_none());
    assert_eq!(
        visits
            .graph_retention_v2
            .as_ref()
            .unwrap()
            .accounting()
            .unwrap(),
        before
    );
    // Actual original/PENDING references preserve the pages, but no registry
    // label/name lookup can fabricate a new Graph source after discard.
    drop(pending);
    drop(first);
    assert_eq!(
        visits
            .graph_retention_v2
            .as_ref()
            .unwrap()
            .accounting()
            .unwrap()
            .sources,
        0
    );
}
#[test]
fn commit_graph_v2_requested_pose_foreign_actual_pages_and_compare_index_refuse_without_reads() {
    let real = Real::new("sha1");
    let tip = ordinary(&real, 230);
    let graph = Workspace::load(&real.root, RepositoryMode::Graph, &tip, None)
        .unwrap()
        .unwrap();
    let mut visits = OriginalSavedVisitsV1::default();
    let visit = visits.capture_graph_v2(&graph).unwrap();
    let pose = graph.graph_position_v2().unwrap();
    let request = visit.graph_request_position_v2(&pose).unwrap();
    let another = Workspace::load(&real.root, RepositoryMode::Graph, &tip, None)
        .unwrap()
        .unwrap();
    let foreign = visits.capture_graph_v2(&another).unwrap();
    let operation = LocalGitNavigationReadV1::begin(visit.origin(), Default::default()).unwrap();
    real.clear_trace();
    assert!(foreign
        .restore_graph_position_v2(&request, &operation)
        .unwrap_err()
        .contains("exact retained source"));
    assert!(real.children().is_empty());
    let mut invalid = pose;
    invalid.comparison_parent = Some(64);
    invalid.view = GraphViewV1::Comparison;
    assert!(visit.graph_request_position_v2(&invalid).is_err());
    assert!(real.children().is_empty());
    // Pure projection has no way to broaden the genuine known200-member set.
    assert!(graph
        .graph_state_v1
        .as_ref()
        .unwrap()
        .projection_position_v2(request.position(), GraphWindowActionV2::Older)
        .is_err());
}
