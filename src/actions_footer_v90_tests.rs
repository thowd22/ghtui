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

#[test]
fn design_footer_v90_actions_scope_filter_rerun_and_acknowledge_have_keys() {
    let mut page = workspace(Some(42));
    let commands = [
        (
            ActionsCommand::ChooseScope,
            'o',
            "o Choose Actions scope",
            "o Scope",
        ),
        (ActionsCommand::FilterRuns, 'f', "f Filter runs", "f Filter"),
        (
            ActionsCommand::RerunAll,
            'A',
            "A Rerun all jobs",
            "A Rerun all",
        ),
        (
            ActionsCommand::RerunFailed,
            'F',
            "F Rerun failed jobs",
            "F Rerun failed",
        ),
    ];
    page.controls = commands
        .iter()
        .map(|(command, ..)| ActionsControl {
            label: command.label().into(),
            action: ActionsAction::Compose(*command),
            disabled_reason: None,
        })
        .chain([
            ActionsControl {
                label: "Acknowledge inspected state".into(),
                action: ActionsAction::Acknowledge,
                disabled_reason: None,
            },
            ActionsControl {
                label: "Acknowledge inspected acceptance (effect unverified)".into(),
                action: ActionsAction::PublicationAcknowledgeAcceptance(9),
                disabled_reason: None,
            },
        ])
        .collect();
    let keys = footer_keys(&page);
    for (command, key, wide, compact) in commands {
        assert_eq!(
            press(&keys, key),
            Some(Action::Actions(ActionsAction::Compose(command)))
        );
        let control = keys
            .iter()
            .find(|control| control.key == key.to_string())
            .unwrap();
        assert_eq!(control.token(false), wide);
        assert_eq!(control.token(true), compact);
    }
    // Both acknowledgements share K; the first enabled one keeps it and the other stays a
    // click target and palette entry.
    assert_eq!(
        press(&keys, 'K'),
        Some(Action::Actions(ActionsAction::Acknowledge))
    );
    let acceptance = keys
        .iter()
        .find(|control| {
            control.action == Action::Actions(ActionsAction::PublicationAcknowledgeAcceptance(9))
        })
        .unwrap();
    assert!(acceptance.key.is_empty());
    // Alone on its page, the acceptance acknowledgement owns K.
    page.controls
        .retain(|control| control.action == ActionsAction::PublicationAcknowledgeAcceptance(9));
    assert_eq!(
        press(&footer_keys(&page), 'K'),
        Some(Action::Actions(
            ActionsAction::PublicationAcknowledgeAcceptance(9)
        ))
    );
    // The new keys never shadow a Runs/Run page control or a catalogue tab.
    let mut runs = workspace(Some(42));
    for page in [ActionsView::Runs, ActionsView::Run] {
        runs.view = page;
        runs.prepare_rows();
        let keys = footer_keys(&runs);
        for key in ['o', 'f', 'A', 'F'] {
            let bearers = keys
                .iter()
                .filter(|control| control.key == key.to_string())
                .count();
            assert!(bearers <= 1, "{key} has {bearers} bearers on {page:?}");
        }
    }
}

#[test]
fn task67_actions_enter_open_token_on_run_and_job_rows_runs_the_row_action() {
    let open_run = ActionsAction::OpenRun {
        repository: "example/repo".into(),
        id: 123,
        attempt: 2,
    };
    let row = |action: Option<ActionsAction>| crate::actions_workspace::ActionsRow {
        cells: Default::default(),
        label: "row".into(),
        role: crate::theme::SemanticToken::Normal,
        actor: None,
        action,
    };
    for action in [open_run.clone(), ActionsAction::OpenJob(501)] {
        let mut page = workspace(Some(42));
        page.rows = vec![row(Some(action.clone()))];
        page.selected = 0;
        let keys = footer_keys(&page);
        let open = keys
            .iter()
            .find(|control| control.key == "Enter")
            .expect("Enter Open on a run or job row");
        assert_eq!(open.label, "Open");
        assert_eq!(open.action, Action::Actions(action.clone()));
        // It sits right after Refresh, and Refresh still leads the bar.
        let refresh = keys
            .iter()
            .position(|control| control.action == Action::Refresh)
            .unwrap();
        assert_eq!(keys[refresh + 1].key, "Enter");
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(
            resolve_key(&keys, enter),
            Some(Action::Actions(action.clone())),
            "the Enter key does exactly what the painted token and a click on the row do"
        );
        assert_eq!(keyboard_action(enter, &page), Some(action.clone()));
        // Painted at 80x24 beside Refresh, and a click target.
        let mut app = App {
            view: crate::app::View::Actions,
            actions: Some(page),
            ..Default::default()
        };
        let rows = screen(&mut app, 80, 24);
        let footer = &rows[23];
        assert!(footer.starts_with(" r Refresh  Enter Open  "), "{footer:?}");
        assert!(app
            .hits
            .iter()
            .any(|hit| hit.rect.y == 23 && hit.action == Action::Actions(action.clone())));
    }
    // An information row has nothing to open, so no token is painted.
    let mut page = workspace(Some(42));
    page.rows = vec![row(None)];
    assert!(footer_keys(&page)
        .iter()
        .all(|control| control.key != "Enter"));
}
