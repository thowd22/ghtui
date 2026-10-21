//! TASK-109: the next History page keeps the bar and the viewport on the commits the reader
//! scrolled to. Real repositories only; nothing is written.
use super::*;
use crate::{
    app::{Action, View},
    local_commit_actor_origin_v1_test_support::Real,
    repository_graph_v1::{fixture_v2::ordinary, GraphActionV1},
    workspace::RepositoryMode,
};

fn history_app(real: &Real, tip: &str) -> App {
    let workspace = Workspace::load(&real.root, RepositoryMode::Graph, tip, None)
        .unwrap()
        .unwrap();
    App {
        view: View::Repository,
        repository_list: crate::ui_table::ListState {
            selected: workspace.graph_focus_row_v1().unwrap(),
            ..Default::default()
        },
        workspace: Some(workspace),
        repository_visible_rows: 30,
        repository_content_focused: true,
        ..Default::default()
    }
}

fn row_of(workspace: &Workspace, id: &str) -> usize {
    workspace
        .graph_action_index_v1(&RowAction::Commit(id.to_owned()))
        .unwrap()
}

#[test]
fn history_place_v109_next_page_keeps_bar_and_viewport_commits() {
    let real = Real::new("sha1");
    let tip = ordinary(&real, 230);
    let before = real.snapshot();
    let mut app = history_app(&real, &tip);
    let workspace = app.workspace.clone().unwrap();
    assert_eq!(workspace.graph_state_v1.as_ref().unwrap().loaded(), 200);
    // The reader moved the bar to the 190th commit and scrolled the viewport above it.
    let commits: Vec<usize> = (0..workspace.display_rows_v1().len())
        .filter(|index| commit_at(&workspace, *index).is_some())
        .collect();
    let bar_row = commits[190];
    let top_row = commits[170];
    app.repository_list.selected = bar_row;
    app.repository_list.top = top_row;
    let bar = commit_at(&workspace, bar_row).unwrap();
    let top = commit_at(&workspace, top_row).unwrap();

    // Asking for the next page (as the boundary scroll does) records that place.
    app.dispatch(Action::RepositoryRow(RowAction::Graph(GraphActionV1::More)));
    let place = app.history_place_v109.clone().expect("place recorded");
    assert_eq!(place.bar.as_deref(), Some(bar.as_str()));
    assert_eq!(place.top, Some((top.clone(), 0)));
    assert_eq!(place.loaded, 200);
    assert_eq!(
        app.workspace.as_ref().unwrap().revision,
        bar,
        "the next window opens on the commit under the bar"
    );

    // The page read the runtime runs, then the install that used to land on row 0.
    let mut next = app.workspace.clone().unwrap();
    next.open(RowAction::Graph(GraphActionV1::More)).unwrap();
    assert_eq!(next.graph_state_v1.as_ref().unwrap().loaded(), 230);
    app.repository_list.selected = 0;
    app.repository_list.top = 0;
    assert!(restore(&mut app, &next, &place));
    assert_eq!(app.repository_list.selected, row_of(&next, &bar));
    assert_eq!(app.repository_list.top, row_of(&next, &top));
    assert_eq!(commit_at(&next, app.repository_list.selected), Some(bar));
    assert_eq!(commit_at(&next, app.repository_list.top), Some(top));
    assert_eq!(real.snapshot(), before, "History stays read-only");
}

#[test]
fn history_place_v109_wheel_scroll_keeps_viewport_with_bar_on_newest() {
    let real = Real::new("sha1");
    let tip = ordinary(&real, 230);
    let mut app = history_app(&real, &tip);
    let workspace = app.workspace.clone().unwrap();
    let newest = app.repository_list.selected;
    // The wheel moves only the viewport: the bar stays on the newest commit.
    app.repository_list.top = newest + 180;
    let top = commit_at(&workspace, newest + 180).unwrap();
    let place = capture(&app).unwrap();
    assert_eq!(place.bar, commit_at(&workspace, newest));
    let mut next = workspace.clone();
    next.open(RowAction::Graph(GraphActionV1::More)).unwrap();
    app.repository_list.top = 0;
    assert!(restore(&mut app, &next, &place));
    assert_eq!(app.repository_list.selected, newest);
    assert_eq!(commit_at(&next, app.repository_list.top), Some(top));
}

#[test]
fn history_place_v109_is_not_adopted_without_a_larger_walk() {
    let real = Real::new("sha1");
    let tip = ordinary(&real, 230);
    let mut app = history_app(&real, &tip);
    let workspace = app.workspace.clone().unwrap();
    app.repository_list.top = 50;
    let place = capture(&app).unwrap();
    app.repository_list.top = 0;
    assert!(!restore(&mut app, &workspace, &place));
    assert_eq!(app.repository_list.top, 0);
    // Any other History row action forgets a recorded place.
    app.history_place_v109 = Some(place);
    let id = commit_at(&workspace, 5).unwrap();
    app.dispatch(Action::RepositoryRow(RowAction::Commit(id)));
    assert_eq!(app.history_place_v109, None);
}
