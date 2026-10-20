//! Graph key-bar controls: visible focus on the placed tokens, disabled keys explain, and one
//! Back meaning. Actual Tab terminal proof remains in both native paging bundles.
use super::*;
use crate::{local_commit_actor_origin_v1_test_support::Real, workspace::RepositoryMode};
use ratatui::{backend::TestBackend, layout::Rect, style::Modifier, Terminal};

#[test]
fn commit_graph_v2_key_bar_focus_is_visible_and_disabled_graph_keys_explain() {
    let real = Real::new("sha1");
    let workspace = Workspace::load(&real.root, RepositoryMode::Graph, &real.original, None)
        .unwrap()
        .unwrap();
    real.clear_trace();
    let controls = controls(&workspace, false);
    assert!(!controls.is_empty());
    for width in [120, 80, 40] {
        let placed = crate::footer::layout(&controls, width).len();
        assert!(placed > 0 || width < 10);
        for selected in 0..placed {
            let mut terminal = Terminal::new(TestBackend::new(width, 2)).unwrap();
            let mut targets = Vec::new();
            terminal
                .draw(|frame| {
                    targets = crate::footer::render(
                        frame,
                        Rect::new(0, 1, width, 1),
                        Theme::from_settings(0, false),
                        &controls,
                        Some(selected),
                    );
                })
                .unwrap();
            assert_eq!(targets.len(), placed);
            for (index, (rect, action)) in targets.iter().enumerate() {
                let cell = &terminal.backend().buffer()[(rect.x, rect.y)];
                assert_eq!(
                    cell.modifier.contains(Modifier::UNDERLINED),
                    index == selected,
                    "Only the focused key-bar token is underlined at {width}"
                );
                assert!(
                    !matches!(action, Action::Explain(_)),
                    "Disabled graph keys are not painted"
                );
            }
        }
    }
    // While the original graph is read every graph key leaves the bar and explains itself.
    let busy = super::controls(&workspace, true);
    assert!(crate::footer::layout(&busy, 120).is_empty());
    for control in &busy {
        assert_eq!(control.disabled.as_deref(), Some(BUSY));
    }
    assert!(real.children().is_empty());
}

#[test]
fn commit_graph_v2_responsible_file_subview_keeps_one_back_meaning_in_outer_toolbar() {
    use crate::{
        git::RawPath,
        providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
        workspace::{FileAction, RowAction},
    };
    let real = Real::new("sha1");
    let mut workspace = Workspace::load(&real.root, RepositoryMode::Tree, &real.original, None)
        .unwrap()
        .unwrap();
    for action in [
        RowAction::File(RawPath(b"tracked".to_vec())),
        RowAction::FileProvenance(FileAction::Blame),
    ] {
        let operation = LocalGitNavigationReadV1::begin(
            workspace.actor_origin.as_ref().unwrap(),
            Default::default(),
        )
        .unwrap();
        workspace
            .open_original_with_operation_v1(action, &operation)
            .unwrap();
        operation.seal().unwrap();
    }
    // The responsible commit of a blamed line opens in History (TASK-99), which carries no
    // file source: its one Back is the graph's.
    for action in [
        RowAction::FileProvenance(FileAction::BlameLine(1)),
        RowAction::FileProvenance(FileAction::Graph),
        RowAction::Graph(GraphActionV1::Details),
    ] {
        workspace.open(action).unwrap();
    }
    assert!(workspace.file_provenance.is_none());
    assert!(workspace.graph_retention_visible_v2());
    real.clear_trace();
    for width in [120, 80] {
        let mut app = crate::app::App {
            view: crate::app::View::Repository,
            workspace: Some(workspace.clone()),
            ..Default::default()
        };
        let mut terminal = Terminal::new(TestBackend::new(width, 40)).unwrap();
        terminal
            .draw(|frame| crate::ui::draw(frame, &mut app))
            .unwrap();
        assert!(app.hits.iter().any(|hit| hit.rect.y == 39
            && hit.action == Action::RepositoryRow(RowAction::Graph(GraphActionV1::Back))));
        assert!(!app
            .hits
            .iter()
            .any(|hit| hit.action
                == Action::RepositoryRow(RowAction::FileProvenance(FileAction::Back))));
        let keys = crate::repository_ui::footer_keys(&app);
        let backs = keys
            .iter()
            .filter(|control| control.key == "u")
            .collect::<Vec<_>>();
        assert_eq!(backs.len(), 1, "One Back meaning in the key bar");
        assert_eq!(
            backs[0].action,
            Action::RepositoryRow(RowAction::Graph(GraphActionV1::Back))
        );
        assert!(!keys.iter().any(|control| control.action
            == Action::RepositoryRow(RowAction::FileProvenance(FileAction::Back))));
        assert_eq!(
            key(&workspace, KeyCode::Char('u')),
            Some(GraphActionV1::Back)
        );
    }
    assert!(real.children().is_empty());
}
