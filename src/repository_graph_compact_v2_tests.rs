//! DECLARED ONLY after Root admission. Actual page factories, no test DTO proof.
use super::*;
use crate::local_commit_actor_origin_v1_test_support::{git, text, Real};
use crate::repository_graph_v1::fixture_v2::{all, first, more, ordinary};
use std::collections::HashSet;

#[test]
fn commit_graph_v2_ordinary10000_full_loaded_raw_and_compact_fit_unchanged8mib() {
    let real = Real::new("sha1");
    let tip = ordinary(&real, 10_000);
    let baseline = real.snapshot();
    let state = all(&real, &tip);
    assert_eq!(state.loaded(), 10_000);
    assert_eq!(state.pages.len(), 50);
    assert_eq!(state.coverage(), &GraphCoverageV1::AvailableWalkExhausted);
    assert!(state.retained_bytes_v2() < MAX_RETAINED);
    assert!(
        state.topology_v2().bytes() < 2 * 1024 * 1024,
        "Ordinary numeric topology is bounded, not body copying"
    );
    let ids = text(&real.root, &["rev-list", "--topo-order", &tip])
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    for (index, id) in ids.iter().enumerate() {
        let member = state.topology.member(id).unwrap();
        assert_eq!(member.node as usize, index);
        assert_eq!(state.member_at_v2(index).unwrap().id, *id);
        if index + 1 < ids.len() {
            assert_eq!(
                state.selected_commit(id).unwrap().parents,
                vec![ids[index + 1].clone()]
            );
        }
    }
    for start in (0..10_000).step_by(200) {
        let shown = GraphPresentationV1::prepare_window_v2(
            &state,
            &ids[start],
            GraphViewV1::Timeline,
            start,
        )
        .unwrap();
        assert!(state.retained_bytes_v2() + shown.retained_bytes_v2() <= MAX_RETAINED);
        assert_eq!(
            shown
                .rows()
                .iter()
                .filter(|r| r.role != SemanticRole::Boundary
                    && matches!(r.action, Some(RowAction::Commit(_))))
                .count(),
            200
        );
        assert!(shown.rows()[0]
            .label
            .contains(&format!("showing {}–{}", start + 1, start + 200)));
        assert!(shown.rows()[0]
            .label
            .starts_with("Read-only · 10000 commits loaded"));
    }
    assert_eq!(real.snapshot(), baseline);
}
#[test]
fn commit_graph_v2_actual_ordered_octopus_edges_colors_ancestors_and_prefix_are_exact() {
    let real = Real::new("sha256");
    let tree = text(&real.root, &["rev-parse", "HEAD^{tree}"]);
    let parents = (0..4)
        .map(|n| {
            String::from_utf8(git(
                &real.root,
                &[
                    "commit-tree",
                    &tree,
                    "-p",
                    &real.original,
                    "-m",
                    &format!("Ordered parent {n}"),
                ],
                None,
            ))
            .unwrap()
            .trim()
            .to_owned()
        })
        .collect::<Vec<_>>();
    let mut argv = vec!["commit-tree", tree.as_str()];
    for parent in &parents {
        argv.extend(["-p", parent.as_str()]);
    }
    argv.extend(["-m", "Ordered octopus"]);
    let tip = String::from_utf8(git(&real.root, &argv, None))
        .unwrap()
        .trim()
        .to_owned();
    let state = all(&real, &tip);
    let node = &state.topology.rows()[state.topology.member(&tip).unwrap().node as usize];
    let edges = state.topology.edges(node);
    assert_eq!(edges.len(), 4);
    assert_eq!(
        edges
            .iter()
            .map(|e| (
                usize::from(e.order),
                state.topology.id(&state.pages, e.parent).unwrap()
            ))
            .collect::<Vec<_>>(),
        parents
            .iter()
            .enumerate()
            .map(|(n, p)| (n, p.as_str()))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        edges.iter().map(|e| e.color).collect::<HashSet<_>>().len(),
        4
    );
    let ancestors = state.topology.ancestors(&tip).unwrap();
    // The walk also covers the checked-out branch, whose newer commit is not an ancestor.
    for (index, related) in ancestors.iter().enumerate() {
        assert_eq!(*related, state.member_at_v2(index).unwrap().id != real.head);
    }
    let real2 = Real::new("sha1");
    let tip2 = ordinary(&real2, 230);
    let before = first(&real2, &tip2);
    let prefix = before.topology.rows().to_vec();
    let next = more(&before);
    assert_eq!(&next.topology.rows()[..200], prefix);
    assert_eq!(next.walk_tip(), tip2);
    assert_eq!(next.loaded(), 230);
}
#[test]
fn commit_graph_v2_actual_oversized_subjects_refuse_at_shared_active_budget_not_silent_drop() {
    let real = Real::new("sha1");
    let tree = text(&real.root, &["rev-parse", "HEAD^{tree}"]);
    let mut tip = real.original.clone();
    let subject = "a".repeat(7200);
    for _ in 0..400 {
        tip = String::from_utf8(git(
            &real.root,
            &["commit-tree", &tree, "-p", &tip, "-m", &subject],
            None,
        ))
        .unwrap()
        .trim()
        .to_owned();
    }
    // Compact body may fit the first page, but full prepared Details/Timeline
    // must respect the SAME raw+topology+actual row8MiB bound.
    let state = more(&first(&real, &tip));
    let result = GraphPresentationV1::prepare_window_v2(&state, &tip, GraphViewV1::Timeline, 0);
    assert!(
        result.is_err(),
        "Large labels must not bypass shared active bound"
    );
    assert_eq!(state.loaded(), 400);
    assert_eq!(state.walk_tip(), tip);
}
