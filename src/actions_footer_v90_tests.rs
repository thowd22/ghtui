//! TASK-90 Actions adapter: catalogue tabs above the content, every other prepared control in
//! the key bar by priority, keys resolving to the painted token's action, and disabled keys
//! that explain themselves.
use super::*;
use crate::{
    actions_workspace::{ActionsCommand, ActionsControl, NEXT_FAILURE},
    app::App,
    footer::{layout, resolve_key},
    providers::gh::HostIdentity,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{backend::TestBackend, Terminal};

fn workspace(account: Option<u64>) -> ActionsWorkspace {
    let mut workspace = ActionsWorkspace::new(
        HostIdentity {
            host: "github.invalid".into(),
            login: "octocat".into(),
            account_id: account,
            gh_version: "offline".into(),
        },
        vec!["example/repo".into(), "example/other".into()],
    );
    workspace.prepare_rows();
    workspace
}

fn screen(app: &mut App, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| crate::ui::draw(frame, app)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
        .collect()
}

fn press(keys: &[KeyControl], key: char) -> Option<Action> {
    resolve_key(keys, KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE))
}

#[test]
fn design_footer_v90_actions_tabs_replace_toolbar_rows_and_bar_compacts_at_80() {
    for (width, height) in [(140, 42), (80, 24)] {
        let mut app = App {
            view: crate::app::View::Actions,
            actions: Some(workspace(Some(42))),
            ..Default::default()
        };
        let rows = screen(&mut app, width, height);
        let text = rows.join("\n");
        // The retired pager, repository buttons and static hint never paint.
        for retired in [
            "change control page",
            "Controls →",
            "[← Previous repo]",
            "[r Refresh]",
            "unavailable:",
            "[] controls",
        ] {
            assert!(
                !text.contains(retired),
                "{retired} painted at {width}x{height}"
            );
        }
        // The catalogue is one tab row; Runs is current and its key is painted.
        let tabs = rows
            .iter()
            .position(|row| row.contains(" R Runs ") && row.contains(" W Workflows "))
            .expect("catalogue tab strip");
        assert!(
            tabs < 6,
            "tabs lead the Actions content at {width}x{height}"
        );
        let footer = &rows[usize::from(height) - 1];
        assert!(footer.starts_with(" r Refresh  "), "{footer:?}");
        assert!(footer.trim_end().ends_with("? Help  : Commands  q Quit") == (width >= 100));
        if width < 100 {
            assert!(
                footer.contains(": Cmds") && footer.contains("P Pubs"),
                "{footer:?}"
            );
        } else {
            assert!(
                footer.contains("P Publications") && footer.contains("} Next repo"),
                "{footer:?}"
            );
        }
        // Every key-bar token and tab is a click target with the action its key resolves to.
        let keys = footer_keys(app.actions.as_ref().unwrap());
        // Refresh is the global binding; its token carries the same global action.
        assert_eq!(press(&keys, 'r'), Some(Action::Refresh));
        assert!(app.hits.iter().any(|hit| hit.action == Action::Refresh));
        for (key, action) in [
            ('W', ActionsAction::View(ActionsView::Workflows)),
            ('P', ActionsAction::PublicationList),
            ('}', ActionsAction::Repository(1)),
        ] {
            assert_eq!(press(&keys, key), Some(Action::Actions(action.clone())));
            if key != '}' || width >= 100 {
                assert!(
                    app.hits
                        .iter()
                        .any(|hit| hit.action == Action::Actions(action.clone())),
                    "{key} has no click target at {width}x{height}"
                );
            }
        }
    }
}

#[test]
fn design_footer_v90_actions_disabled_command_leaves_bar_and_its_key_explains() {
    let mut workspace = workspace(None);
    workspace.view = ActionsView::Run;
    workspace.controls.push(ActionsControl {
        label: ActionsCommand::Cancel.label().into(),
        action: ActionsAction::Compose(ActionsCommand::Cancel),
        disabled_reason: workspace.disabled_reason(ActionsCommand::Cancel),
    });
    let keys = footer_keys(&workspace);
    let reason = match press(&keys, 'C') {
        Some(Action::Explain(reason)) => reason,
        other => panic!("C must explain, got {other:?}"),
    };
    assert!(reason.starts_with("Cancel run: ") && reason.contains("verified account"));
    for width in [140, 80] {
        assert!(layout(&keys, width)
            .iter()
            .all(|placed| keys[placed.index].key != "C"));
    }
}

#[test]
fn design_footer_v90_actions_shared_key_goes_to_the_enabled_control() {
    let mut workspace = workspace(Some(42));
    workspace.controls = vec![
        ActionsControl {
            label: "X Enable workflow".into(),
            action: ActionsAction::Compose(ActionsCommand::EnableWorkflow),
            disabled_reason: Some("already enabled".into()),
        },
        ActionsControl {
            label: "X Disable workflow".into(),
            action: ActionsAction::Compose(ActionsCommand::DisableWorkflow),
            disabled_reason: None,
        },
        ActionsControl {
            label: NEXT_FAILURE.into(),
            action: ActionsAction::Select(3),
            disabled_reason: None,
        },
    ];
    let keys = footer_keys(&workspace);
    assert_eq!(
        press(&keys, 'X'),
        Some(Action::Actions(ActionsAction::Compose(
            ActionsCommand::DisableWorkflow
        )))
    );
    assert_eq!(
        press(&keys, '>'),
        Some(Action::Actions(ActionsAction::Select(3)))
    );
    let placed = layout(&keys, 80)
        .into_iter()
        .map(|placed| keys[placed.index].token(placed.compact))
        .collect::<Vec<_>>();
    assert_eq!(
        placed,
        ["X Disable", "> Next fail", "{ Prev repo", "} Next repo"]
    );
}

#[test]
fn design_help_v97_actions_help_and_commands_cover_every_footer_key() {
    for (account, state) in [(Some(42), "verified Actions"), (None, "unverified Actions")] {
        let app = App {
            view: crate::app::View::Actions,
            actions: Some(workspace(account)),
            ..Default::default()
        };
        assert!(crate::navigation_ui::assert_help_and_palette_cover_footer_v97(&app, state) > 3);
    }
}
