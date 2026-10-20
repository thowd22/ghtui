//! TASK-99: History is one view. The mode, its palette command and its key bar are named
//! History; a blamed line's commit opens in History, which carries no file source of its own;
//! the status line and the last row name the loaded-history boundary. Real repositories only,
//! and nothing is written.
use crate::{
    app::{Action, App, View},
    git::RawPath,
    local_commit_actor_origin_v1_test_support::Real,
    repository_graph_v1::GraphCoverageV1,
    workspace::{FileAction, RepositoryMode, RowAction, Workspace},
};
use ratatui::{backend::TestBackend, Terminal};

fn history_app(real: &Real) -> App {
    let workspace = Workspace::load(&real.root, RepositoryMode::Graph, &real.original, None)
        .unwrap()
        .unwrap();
    let selected = workspace.graph_focus_row_v1().unwrap();
    App {
        view: View::Repository,
        repository_list: crate::ui_table::ListState {
            selected,
            ..Default::default()
        },
        workspace: Some(workspace),
        repository_visible_rows: 30,
        repository_content_focused: true,
        ..Default::default()
    }
}

fn screen(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| crate::ui::draw(frame, app)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn history_view_v99_mode_tab_and_command_are_named_history() {
    let (_, label, key) = RepositoryMode::ALL
        .into_iter()
        .find(|(mode, _, _)| *mode == RepositoryMode::Graph)
        .unwrap();
    assert_eq!((label, key), ("History", 'g'));
    let tab = crate::repository_ui::mode_controls(RepositoryMode::Graph)
        .into_iter()
        .find(|control| control.key == "g")
        .unwrap();
    assert_eq!(tab.label, "History");
    assert!(tab.selected);
    assert_eq!(
        tab.action,
        Action::RepositoryMode(RepositoryMode::Graph),
        "g and its tab open History"
    );
}

#[test]
fn history_view_v99_title_status_boundary_and_key_bar_read_like_the_view() {
    let real = Real::new("sha1");
    let before = real.snapshot();
    let mut app = history_app(&real);
    let text = screen(&mut app, 140, 42);
    let name = real
        .root
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let title = format!("History › {name} › all branches");
    assert!(text.contains(&title), "{title:?} missing\n{text}");
    assert!(text.contains("Read-only · 3 commits loaded"), "{text}");
    assert!(
        text.contains(GraphCoverageV1::AvailableWalkExhausted.label()),
        "the boundary row\n{text}"
    );
    assert!(!text.contains("History pan"), "{text}");
    assert!(!text.contains("g Graph"), "{text}");
    let keys = crate::repository_ui::footer_keys(&app);
    let order: Vec<&str> = keys.iter().map(|control| control.key.as_str()).collect();
    let at = |key: &str| {
        order
            .iter()
            .position(|candidate| *candidate == key)
            .unwrap_or_else(|| panic!("{key} missing from {order:?}"))
    };
    // `/ search  n N next match  f file history  enter diff  Y blame  c compare  y copy SHA`.
    let wanted = ["/", "n", "N", "f", "Enter", "Y", "C", "y"];
    for pair in wanted.windows(2) {
        assert!(
            at(pair[0]) < at(pair[1]),
            "{pair:?} out of order: {order:?}"
        );
    }
    let bar = text.lines().last().unwrap();
    // `n`/`N` wait for a filter and `f`/`Y` for a changed file (this commit changes none):
    // disabled controls leave the bar.
    for token in ["/ Search", "Enter Diff", "C Compare", "y Copy SHA"] {
        assert!(bar.contains(token), "{token:?} not painted: {bar}");
    }
    // A narrow pane drops the repository crumb before the title runs under the count.
    let narrow = screen(&mut app, 80, 24);
    assert!(narrow.contains("History ›"), "{narrow}");
    assert_eq!(real.snapshot(), before);
}

#[test]
fn history_view_v99_more_history_boundary_and_status_name_scrolling() {
    assert!(GraphCoverageV1::MoreAvailable
        .label()
        .contains("Loaded history ends here"));
    assert!(GraphCoverageV1::Capped.label().starts_with("History limit"));
}

#[test]
fn history_view_v99_blamed_line_commit_opens_history_without_a_file_source() {
    let real = Real::new("sha256");
    let before = real.snapshot();
    let mut workspace = Workspace::load(&real.root, RepositoryMode::Tree, &real.original, None)
        .unwrap()
        .unwrap();
    for action in [
        RowAction::File(RawPath(b"tracked".to_vec())),
        RowAction::FileProvenance(FileAction::Blame),
        RowAction::FileProvenance(FileAction::BlameLine(1)),
    ] {
        workspace.open(action).unwrap();
    }
    let responsible = workspace
        .file_provenance
        .as_ref()
        .unwrap()
        .selection
        .as_ref()
        .unwrap()
        .commit
        .clone();
    workspace
        .open(RowAction::FileProvenance(FileAction::Graph))
        .unwrap();
    assert_eq!(workspace.mode, RepositoryMode::Graph);
    assert!(
        workspace.file_provenance.is_none(),
        "History has no file source"
    );
    assert!(workspace.graph_visible_v1());
    assert_eq!(workspace.revision, responsible);
    let focus = workspace.graph_focus_row_v1().unwrap();
    assert_eq!(
        workspace.display_rows_v1()[focus].action,
        Some(RowAction::Commit(responsible))
    );
    // A file's History is the same view with its `path:` chip.
    let mut file = Workspace::load(&real.root, RepositoryMode::Tree, &real.original, None)
        .unwrap()
        .unwrap();
    file.open(RowAction::File(RawPath(b"tracked".to_vec())))
        .unwrap();
    file.open(RowAction::FileProvenance(FileAction::PathHistory))
        .unwrap();
    assert_eq!(file.mode, RepositoryMode::Graph);
    assert!(file.file_provenance.is_none());
    assert_eq!(
        file.history_filter_v993.spec.path,
        Some(RawPath(b"tracked".to_vec()))
    );
    assert_eq!(real.snapshot(), before);
}

fn newer_page() -> Action {
    Action::RepositoryRow(RowAction::Graph(
        crate::repository_graph_v1::GraphActionV1::Window(
            crate::repository_graph_v1::projection_v2::GraphWindowActionV2::Newer,
        ),
    ))
}

/// TASK-100: one status line on the pane's bottom border, and a clickable `↑ N newer
/// commits · < page` row when the window does not start at the newest commit.
#[test]
fn history_polish_v100_status_on_border_and_newer_boundary_row_at_80x24_and_140x42() {
    let real = Real::new("sha1");
    let before = real.snapshot();
    let mut app = history_app(&real);
    for (width, height) in [(80, 24), (140, 42)] {
        let text = screen(&mut app, width, height);
        let status: Vec<&str> = text
            .lines()
            .filter(|line| line.contains("Read-only · 3 commits loaded"))
            .collect();
        assert_eq!(status.len(), 1, "one status line\n{text}");
        assert!(
            status[0].starts_with("╰─ Read-only · 3 commits loaded"),
            "the status sits on the pane's bottom border\n{text}"
        );
        assert!(!text.contains("newer commit"), "{text}");
        assert!(!app.hits.iter().any(|hit| hit.action == newer_page()));
    }
    app.workspace.as_mut().unwrap().graph_projection_start_v2 = 200;
    for (width, height) in [(80, 24), (140, 42)] {
        let text = screen(&mut app, width, height);
        let lines: Vec<&str> = text.lines().collect();
        let y = lines
            .iter()
            .position(|line| line.contains("↑ 200 newer commits · < page"))
            .unwrap_or_else(|| panic!("boundary row missing\n{text}"));
        assert!(
            lines[y + 1].contains("GRAPH") && lines[y + 1].contains("MESSAGE"),
            "the boundary sits above the column titles\n{text}"
        );
        let hit = app
            .hits
            .iter()
            .find(|hit| hit.action == newer_page())
            .expect("the boundary row is a click target");
        assert_eq!(usize::from(hit.rect.y), y);
    }
    assert_eq!(real.snapshot(), before);
}
