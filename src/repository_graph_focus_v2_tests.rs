//! NEW private Graph UI child; visible global hit focus on wrapped/disabled
//! controls. Actual Tab terminal proof remains in both native paging bundles.
use super::*;
use crate::{local_commit_actor_origin_v1_test_support::Real, workspace::RepositoryMode};
use ratatui::{backend::TestBackend, style::Modifier, Terminal};

#[test]
fn commit_graph_v2_wrapped_global_focus_is_visible_and_content_focus_does_not_underline_controls() {
    let real = Real::new("sha1");
    let workspace = Workspace::load(&real.root, RepositoryMode::Graph, &real.original, None)
        .unwrap()
        .unwrap();
    real.clear_trace();
    for width in [120, 80, 40] {
        let controls = controls(&workspace);
        for selected in 0..controls.len() {
            let mut terminal = Terminal::new(TestBackend::new(width, 8)).unwrap();
            let prefix = vec![
                crate::app::HitRegion {
                    rect: Rect::new(0, 7, 1, 1),
                    action: Action::Help
                };
                3
            ];
            let mut hits = prefix.clone();
            terminal
                .draw(|frame| {
                    draw_controls(
                        frame,
                        &workspace,
                        &mut hits,
                        Rect::new(0, 0, width, 7),
                        Theme::from_settings(0, false),
                        false,
                        GraphControlFocusV2 {
                            hit: 3 + selected,
                            content_focused: false,
                        },
                    );
                })
                .unwrap();
            assert_eq!(hits.len(), 3 + controls.len());
            for (index, hit) in hits.iter().skip(3).enumerate() {
                let cell = &terminal.backend().buffer()[(hit.rect.x, hit.rect.y)];
                assert_eq!(cell.modifier.contains(Modifier::UNDERLINED),index==selected,
                    "Only exact globally focused control gets visible underline at {width}, including Explain controls");
            }
            hits = prefix;
            terminal
                .draw(|frame| {
                    draw_controls(
                        frame,
                        &workspace,
                        &mut hits,
                        Rect::new(0, 0, width, 7),
                        Theme::from_settings(0, true),
                        false,
                        GraphControlFocusV2 {
                            hit: 3 + selected,
                            content_focused: true,
                        },
                    );
                })
                .unwrap();
            assert!(hits
                .iter()
                .skip(3)
                .all(|hit| !terminal.backend().buffer()[(hit.rect.x, hit.rect.y)]
                    .modifier
                    .contains(Modifier::UNDERLINED)));
        }
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
        RowAction::FileProvenance(FileAction::History),
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
    let responsible = workspace
        .file_provenance
        .as_ref()
        .unwrap()
        .history
        .as_ref()
        .unwrap()
        .entries
        .first()
        .unwrap()
        .commit
        .id
        .clone();
    for action in [
        RowAction::FileProvenance(FileAction::HistoryEntry(responsible)),
        RowAction::FileProvenance(FileAction::Graph),
        RowAction::Graph(GraphActionV1::Details),
    ] {
        workspace.open(action).unwrap();
    }
    assert!(workspace.file_provenance.is_some());
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
            .draw(|frame| crate::repository_ui::draw(frame, &mut app, Rect::new(0, 0, width, 40)))
            .unwrap();
        assert!(app
            .hits
            .iter()
            .any(|hit| hit.action == Action::RepositoryRow(RowAction::Graph(GraphActionV1::Back))));
        assert!(!app
            .hits
            .iter()
            .any(|hit| hit.action
                == Action::RepositoryRow(RowAction::FileProvenance(FileAction::Back))));
        assert_eq!(
            key(&workspace, KeyCode::Char('u')),
            Some(GraphActionV1::Back)
        );
    }
    assert!(real.children().is_empty());
}
