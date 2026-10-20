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
    assert!(show.rows()[0].label.contains("showing 401–430 · < > page"));
    assert!(show.rows()[0]
        .label
        .starts_with("Read-only · 430 commits loaded"));
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
#[test]
fn v1_repo_views_linear_graph_has_one_row_per_commit_with_names_and_dates() {
    let real = Real::new("sha1");
    let tip = ordinary(&real, 30);
    let state = all(&real, &tip);
    let shown =
        GraphPresentationV1::prepare_window_v2(&state, &tip, GraphViewV1::Timeline, 0).unwrap();
    let commits = shown
        .rows()
        .iter()
        .filter(|row| matches!(row.action, Some(RowAction::Commit(_))))
        .collect::<Vec<_>>();
    assert_eq!(commits.len(), 30, "one row per linear commit");
    assert!(
        !shown.rows().iter().any(|row| row.label.starts_with("↳")),
        "a straight single-parent lane needs no connector row"
    );
    // The summary row, the commits and the coverage boundary only.
    assert_eq!(shown.rows().len(), 1 + 30 + 1);
    assert_eq!(shown.rows()[0].label, "Read-only · 30 commits loaded");
    let selected = &shown.rows()[shown.focus().unwrap()];
    // HEAD is a typed chip before its current branch; every name is listed.
    let names = selected
        .label
        .strip_prefix(&format!(
            "▶ selected {} Ordinary graph version 00029 (HEAD, ",
            &tip[..8]
        ))
        .and_then(|rest| rest.split_once(") • "))
        .map(|(names, _)| names)
        .unwrap_or_else(|| panic!("{}", selected.label));
    let mut names = names.split(", ").collect::<Vec<_>>();
    names.sort_unstable();
    assert_eq!(names, ["graph-v2", "main"]);
    assert!(selected
        .label
        .ends_with("author Graph Writer • committer Graph Recorder • 2023-11-14"));
    let plain = commits[1];
    assert!(plain.label.contains(" Ordinary graph version 00028 • "));
    assert!(!plain.label.contains("ancestor"));
}
