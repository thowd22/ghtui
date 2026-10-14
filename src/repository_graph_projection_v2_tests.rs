use super::*;
use crate::local_commit_actor_origin_v1_test_support::Real;
use crate::repository_graph_v1::fixture_v2::{all, ordinary};

#[test]
fn commit_graph_v2_projection_window_navigation_is_real_global_members_and_zero_git() {
    let real = Real::new("sha1");
    let tip = ordinary(&real, 430);
    let state = all(&real, &tip);
    real.clear_trace();
    let first = state
        .position_v2(&tip, GraphViewV1::Timeline, 0, 16, None)
        .unwrap();
    let older = state
        .projection_position_v2(&first, GraphWindowActionV2::Older)
        .unwrap();
    assert_eq!(older.projection_start, 200);
    assert_eq!(older.member.node, 200);
    let last = state
        .projection_position_v2(&older, GraphWindowActionV2::Last)
        .unwrap();
    assert_eq!(last.projection_start, 400);
    assert_eq!(last.member.node, 400);
    let show =
        GraphPresentationV1::prepare_window_v2(&state, &last.selected, last.view, 400).unwrap();
    assert!(show.rows()[0].label.contains("Visible commits 401–430"));
    assert!(show.rows()[0].label.contains("430 saved versions loaded"));
    assert_eq!(
        show.rows()
            .iter()
            .filter(|r| r.role != SemanticRole::Boundary
                && matches!(r.action, Some(RowAction::Commit(_))))
            .count(),
        30
    );
    assert!(state
        .projection_position_v2(&last, GraphWindowActionV2::Older)
        .is_err());
    let back = state
        .projection_position_v2(&last, GraphWindowActionV2::First)
        .unwrap();
    assert_eq!(back.projection_start, 0);
    assert_eq!(back.selected, tip);
    assert_eq!(back.horizontal, 16);
    assert!(real.children().is_empty());
}
#[test]
fn commit_graph_v2_parent_crosses200_window_without_source_change_or_guessed_row_identity() {
    let real = Real::new("sha256");
    let tip = ordinary(&real, 230);
    let state = all(&real, &tip);
    let selected = state.member_at_v2(199).unwrap();
    let parent = &selected.parents[0];
    let member = state.loaded_member_v2(parent).unwrap();
    assert_eq!(member.node, 200);
    let original = state.snapshot_v2();
    let pose = state
        .position_v2(parent, GraphViewV1::Timeline, 200, 8, None)
        .unwrap();
    let shown =
        GraphPresentationV1::prepare_window_v2(&state, parent, GraphViewV1::Timeline, 200).unwrap();
    assert!(original.same_source(&state.snapshot_v2()));
    assert_eq!(pose.member, member);
    assert_eq!(
        shown.rows()[shown.focus().unwrap()].action,
        Some(RowAction::Commit(parent.clone()))
    );
    assert!(shown.rows()[0].label.contains("201–230"));
}
