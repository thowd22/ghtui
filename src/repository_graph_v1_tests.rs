//! Source-only real graph observations and prepared projection contracts.
use super::*;
use crate::{
    git::{GitRepository, OriginalGraphReadV1},
    local_commit_actor_origin_v1_test_support::{git, text, Real},
    providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
};
fn state(real: &Real, tip: &str) -> Arc<OriginalGraphStateV1> {
    let origin = crate::git::OriginalLocalActorOriginV1::capture(&real.repository).unwrap();
    let operation = LocalGitNavigationReadV1::begin(&origin, Default::default()).unwrap();
    let page = origin
        .with_saved_navigation_read_v1(&operation, |repository| {
            repository
                .original_graph_page_v1(OriginalGraphReadV1::Start(tip), operation.deadline())
                .map_err(|error| error.to_string())
        })
        .unwrap();
    operation.seal().unwrap();
    OriginalGraphStateV1::start(origin, page).unwrap()
}
fn chain(real: &Real, count: usize) -> String {
    let tree = text(&real.root, &["rev-parse", "HEAD^{tree}"]);
    let mut parent = real.original.clone();
    for n in 0..count {
        parent = String::from_utf8(git(
            &real.root,
            &[
                "commit-tree",
                &tree,
                "-p",
                &parent,
                "-m",
                &format!("Actual paged version {n:03}"),
            ],
            None,
        ))
        .unwrap()
        .trim()
        .into();
    }
    parent
}
fn octopus(real: &Real) -> (String, Vec<String>) {
    let tree = text(&real.root, &["rev-parse", "HEAD^{tree}"]);
    let mut parents = Vec::new();
    for n in 0..4 {
        parents.push(
            String::from_utf8(git(
                &real.root,
                &[
                    "commit-tree",
                    &tree,
                    "-p",
                    &real.original,
                    "-m",
                    &format!("Actual original parent {n}"),
                ],
                None,
            ))
            .unwrap()
            .trim()
            .to_owned(),
        );
    }
    let mut args = vec!["commit-tree", tree.as_str()];
    for parent in &parents {
        args.extend(["-p", parent]);
    }
    args.extend(["-m", "Actual ordered four-parent merge"]);
    (
        String::from_utf8(git(&real.root, &args, None))
            .unwrap()
            .trim()
            .into(),
        parents,
    )
}
#[test]
fn commit_graph_v1_exact_cursor_prefix_palette_and_selected_ancestor_keep_immutable_walk_tip() {
    let real = Real::new("sha1");
    let tip = chain(&real, 240);
    let baseline = real.snapshot();
    let first = state(&real, &tip);
    assert_eq!(first.loaded(), 200);
    assert_eq!(first.coverage(), &GraphCoverageV1::MoreAvailable);
    let selected = first.member_at_v2(100).unwrap().id.clone();
    let prefix = first.topology.rows().to_vec();
    let operation = LocalGitNavigationReadV1::begin(first.origin(), Default::default()).unwrap();
    let page = first
        .origin()
        .with_saved_navigation_read_v1(&operation, |repository| {
            repository
                .original_graph_page_v1(
                    OriginalGraphReadV1::Continue(first.last_page()),
                    operation.deadline(),
                )
                .map_err(|error| error.to_string())
        })
        .unwrap();
    operation.seal().unwrap();
    let next = OriginalGraphStateV1::append(&first, page).unwrap();
    assert_eq!(&next.topology.rows()[..200], prefix);
    // 240 chained versions, the original and its root, plus the checked-out
    // main commit: the walk covers every branch, not only the opened tip.
    assert_eq!(next.loaded(), 243);
    assert_eq!(next.walk_tip(), tip);
    assert_ne!(next.walk_tip(), selected);
    let shown = GraphPresentationV1::prepare(&next, &selected, GraphViewV1::Timeline).unwrap();
    assert!(shown.focus().is_some());
    assert_eq!(next.coverage(), &GraphCoverageV1::AvailableWalkExhausted);
    assert!(!next.last_page().has_more());
    assert_eq!(real.snapshot(), baseline);
}
#[test]
fn commit_graph_v1_real_octopus_has_all_ordered_connectors_and_parent_actions() {
    let real = Real::new("sha1");
    let (tip, parents) = octopus(&real);
    let state = state(&real, &tip);
    let node = state.topology.rows()[state.loaded_member_v2(&tip).unwrap().node as usize];
    assert_eq!(state.topology.edges(&node).len(), 4);
    assert_eq!(state.selected_commit(&tip).unwrap().parents, parents);
    let timeline = GraphPresentationV1::prepare(&state, &tip, GraphViewV1::Timeline).unwrap();
    // Same-row joins replace the old `↳ parent` connector rows: the octopus
    // row starts three new lanes (`●─┬─┬─╮`) and each parent keeps its own row.
    assert!(timeline.rows().iter().all(|row| !row.label.contains('↳')));
    let row = timeline.commit_row(&tip).unwrap();
    let graph = timeline.paint(row).unwrap()[..]
        .iter()
        .filter(|span| span.color.is_some())
        .map(|span| span.text.as_str())
        .collect::<String>();
    assert_eq!(graph.trim_end().matches(['┬', '╮']).count(), 3, "{graph:?}");
    for parent in &parents {
        assert!(timeline.commit_row(parent).is_some());
    }
    let choices = GraphPresentationV1::prepare(&state, &tip, GraphViewV1::Parents).unwrap();
    for index in 0..4 {
        let key = crate::navigation_state::row_key(&RowAction::Graph(
            GraphActionV1::CompareParent(index),
        ))
        .unwrap();
        let prepared_index = choices.navigation_row(&key).unwrap();
        assert_eq!(
            choices.rows()[prepared_index].action,
            Some(RowAction::Graph(GraphActionV1::CompareParent(index)))
        );
        assert!(choices
            .rows()
            .iter()
            .any(|row| row.action == Some(RowAction::Graph(GraphActionV1::CompareParent(index)))));
        assert!(state.parent_reason(&tip, index).is_none());
    }
}
#[test]
fn commit_graph_v1_details_legend_back_are_worker_prepared_without_git_or_body_loss() {
    let real = Real::new("sha1");
    let state = state(&real, &real.original);
    real.clear_trace();
    let timeline =
        GraphPresentationV1::prepare(&state, &real.original, GraphViewV1::Timeline).unwrap();
    let details =
        GraphPresentationV1::prepare(&state, &real.original, GraphViewV1::Details).unwrap();
    assert!(details
        .rows()
        .iter()
        .any(|row| row.label.contains("Author: Original Writer")));
    assert!(details
        .rows()
        .iter()
        .any(|row| row.label.contains("Committer: Original Recorder")));
    let legend = GraphPresentationV1::prepare(&state, &real.original, GraphViewV1::Legend).unwrap();
    assert!(legend
        .rows()
        .iter()
        .any(|row| row.label.contains("never permanent branch")));
    let back = GraphPresentationV1::prepare(&state, &real.original, GraphViewV1::Timeline).unwrap();
    assert_eq!(
        timeline
            .rows()
            .iter()
            .map(|row| &row.label)
            .collect::<Vec<_>>(),
        back.rows().iter().map(|row| &row.label).collect::<Vec<_>>()
    );
    assert!(real.children().is_empty());
}
#[test]
fn commit_graph_v1_actual_depth_one_is_shallow_boundary_not_root_or_loadable_more() {
    let real = Real::new("sha1");
    let clone = real.directory.path().join("shallow-graph");
    git(
        real.directory.path(),
        &[
            "clone",
            "--depth=1",
            &format!("file://{}", real.root.display()),
            clone.to_str().unwrap(),
        ],
        None,
    );
    let repository = GitRepository::discover(&clone).unwrap().unwrap();
    let origin = crate::git::OriginalLocalActorOriginV1::capture(&repository).unwrap();
    let operation = LocalGitNavigationReadV1::begin(&origin, Default::default()).unwrap();
    let page = origin
        .with_saved_navigation_read_v1(&operation, |repository| {
            repository
                .original_graph_page_v1(
                    OriginalGraphReadV1::Start(&real.head),
                    operation.deadline(),
                )
                .map_err(|error| error.to_string())
        })
        .unwrap();
    operation.seal().unwrap();
    let state = OriginalGraphStateV1::start(origin, page).unwrap();
    assert_eq!(state.coverage(), &GraphCoverageV1::ShallowBoundary);
    assert_eq!(state.loaded(), 1);
    assert!(!state.last_page().has_more());
    assert_eq!(
        state.selected_commit(&real.head).unwrap().parents,
        vec![real.original.clone()]
    );
    assert!(state
        .parent_reason(&real.head, 0)
        .unwrap()
        .contains("Shallow"));
    assert!(
        !crate::repository_view::explain_commit(state.selected_commit(&real.head).unwrap())
            .contains("root commit")
    );
}
#[test]
fn commit_graph_v1_projection_spans_keep_role_symbols_color_slots_and_current_index() {
    let real = Real::new("sha256");
    let (tip, parents) = octopus(&real);
    let state = state(&real, &tip);
    let projected = GraphPresentationV1::prepare(&state, &tip, GraphViewV1::Timeline).unwrap();
    let focus = projected.focus().unwrap();
    let selected = &projected.rows()[focus];
    assert_eq!(selected.role, SemanticRole::Head);
    assert_eq!(selected.action, Some(RowAction::Commit(tip.clone())));
    assert!(projected
        .paint(focus)
        .unwrap()
        .iter()
        .any(|span| span.color.is_some() && span.text.contains('●')));
    // The octopus row draws its dot and three new parent lanes in four palette slots.
    let lanes = state.lane_rows_v991(0, state.loaded()).unwrap();
    let lane = &lanes[state.loaded_member_v2(&tip).unwrap().node as usize];
    let slots = lane
        .cells
        .iter()
        .filter(|cell| matches!(cell.glyph, '●' | '┬' | '╮'))
        .filter_map(|cell| cell.slot)
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(
        slots.len(),
        4,
        "Four ordered parent histories must retain four path palette slots"
    );
    let painted = projected
        .paint(focus)
        .unwrap()
        .iter()
        .filter(|span| span.text.trim() != "")
        .filter_map(|span| span.color)
        .collect::<std::collections::HashSet<_>>();
    assert!(slots.is_subset(&painted), "{slots:?} ⊆ {painted:?}");
    for parent in &parents {
        assert!(projected
            .rows()
            .iter()
            .any(|row| row.role == SemanticRole::Reference
                && row.label.starts_with(&parent[..8])
                && row.action == Some(RowAction::Commit(parent.clone()))));
    }
    for mode in [
        crate::theme::ColorMode::TrueColor,
        crate::theme::ColorMode::NoColor,
    ] {
        let theme = crate::theme::Theme::from_settings(0, mode == crate::theme::ColorMode::NoColor);
        if mode == crate::theme::ColorMode::NoColor {
            assert_eq!(theme.graph(0), theme.graph(1));
        } else {
            assert_ne!(theme.graph(0), theme.graph(1));
        }
        for row in &lanes {
            for slot in row.cells.iter().filter_map(|cell| cell.slot) {
                assert!(slot < 8, "lane slots stay inside the theme palette");
                assert_eq!(theme.graph(slot), theme.graph(slot + 8));
            }
        }
    }
}
#[test]
fn commit_graph_v1_continuation_refuses_changed_shallow_epoch_and_original_source() {
    let real = Real::new("sha1");
    let tip = chain(&real, 205);
    let state = state(&real, &tip);
    std::fs::write(real.root.join(".git/shallow"), format!("{tip}\n")).unwrap();
    let operation = LocalGitNavigationReadV1::begin(state.origin(), Default::default()).unwrap();
    real.clear_trace();
    let result = state
        .origin()
        .with_saved_navigation_read_v1(&operation, |repository| {
            repository
                .original_graph_page_v1(
                    OriginalGraphReadV1::Continue(state.last_page()),
                    operation.deadline(),
                )
                .map_err(|error| error.to_string())
        });
    assert!(result.unwrap_err().contains("shallow walk changed"));
    assert_eq!(state.loaded(), 200);
    assert_eq!(state.walk_tip(), tip);
    assert!(!real
        .children()
        .iter()
        .any(|row| row["argv"].as_array().is_some_and(|argv| argv
            .iter()
            .any(|item| item == "--batch" || item == "--batch-check"))));
}
#[test]
fn commit_graph_v1_retained_lane_and_body_budget_refuses_before_graph_projection_or_model_copy() {
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
    let origin = crate::git::OriginalLocalActorOriginV1::capture(&real.repository).unwrap();
    let operation = LocalGitNavigationReadV1::begin(&origin, Default::default()).unwrap();
    let page = origin
        .with_saved_navigation_read_v1(&operation, |repository| {
            repository
                .original_graph_page_v1(OriginalGraphReadV1::Start(&tip), operation.deadline())
                .map_err(|error| error.to_string())
        })
        .unwrap();
    operation.seal().unwrap();
    assert_eq!(page.commits().len(), 200);
    let first = OriginalGraphStateV1::start(origin, page).unwrap();
    let operation = LocalGitNavigationReadV1::begin(first.origin(), Default::default()).unwrap();
    let second = first
        .origin()
        .with_saved_navigation_read_v1(&operation, |repository| {
            repository
                .original_graph_page_v1(
                    OriginalGraphReadV1::Continue(first.last_page()),
                    operation.deadline(),
                )
                .map_err(|e| e.to_string())
        })
        .unwrap();
    operation.seal().unwrap();
    let state = OriginalGraphStateV1::append(&first, second).unwrap();
    assert_eq!(state.loaded(), 400);
    assert!(state.retained_bytes_v2() <= MAX_RETAINED);
    let error = GraphPresentationV1::prepare(&state, &tip, GraphViewV1::Timeline).unwrap_err();
    assert!(error.contains("8MiB"));
    assert_eq!(
        state.loaded(),
        400,
        "Budget refusal does not truncate/drop actual known members"
    );
}
