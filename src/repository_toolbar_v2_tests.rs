use crate::{
    app::{Action, App, View},
    local_commit_actor_origin_v1_test_support::Real,
    workspace::{RepositoryMode, RowAction, Workspace},
};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{backend::TestBackend, Terminal};

fn row_text(terminal: &Terminal<TestBackend>, y: u16) -> String {
    let buffer = terminal.backend().buffer();
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect()
}

#[test]
fn staging_repository_toolbar_v2_full_hunk_hit_and_key_mouse_canonical_effect_at80_120() {
    let real = Real::new("sha256");
    let workspace = Workspace::load(&real.root, RepositoryMode::Working, "HEAD", None)
        .unwrap()
        .unwrap();
    let selected = workspace
        .display_rows_v1()
        .iter()
        .position(|row| matches!(row.action, Some(RowAction::DiffLine(_))))
        .unwrap();
    let before = real.snapshot();
    real.clear_trace();
    for width in [80, 120] {
        let mut app = App {
            view: View::Repository,
            workspace: Some(workspace.clone()),
            repository_list: crate::ui_table::ListState {
                selected,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut terminal = Terminal::new(TestBackend::new(width, 24)).unwrap();
        terminal
            .draw(|frame| crate::ui::draw(frame, &mut app))
            .unwrap();
        // The bracketed toolbar rows are gone: Hunk is a key-bar token in the last row.
        let hit = app
            .hits
            .iter()
            .find(|hit| hit.action == Action::RepositoryHunk)
            .unwrap()
            .clone();
        assert_eq!(hit.rect.y, 23, "Hunk lives in the key bar at {width}");
        let painted = (hit.rect.x..hit.rect.right())
            .map(|x| terminal.backend().buffer()[(x, hit.rect.y)].symbol())
            .collect::<String>();
        assert_eq!(painted, "h Hunk");
        assert!(hit.rect.right() <= width);
        let screen = (0..24)
            .map(|y| row_text(&terminal, y))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!screen.contains("[h Hunk]"), "{screen}");
        assert!(!screen.contains("[+ Stage]"), "{screen}");
        assert!(screen.contains(" w Working "), "{screen}");
        assert!(row_text(&terminal, 23).contains("+ Stage  - Unstage  K Commit  h Hunk"));
        let mut keyboard = App {
            view: View::Repository,
            workspace: Some(workspace.clone()),
            repository_list: crate::ui_table::ListState {
                selected,
                ..Default::default()
            },
            ..Default::default()
        };
        keyboard.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char('h'),
            KeyModifiers::NONE,
        )));
        app.handle_event(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: hit.rect.x,
            row: hit.rect.y,
            modifiers: KeyModifiers::NONE,
        }));
        let mouse_selection = app
            .repository_selection
            .as_ref()
            .expect("actual mouse hunk selection");
        let key_selection = keyboard
            .repository_selection
            .as_ref()
            .expect("actual keyboard hunk selection");
        assert_eq!(
            (&mouse_selection.start, &mouse_selection.end),
            (&key_selection.start, &key_selection.end)
        );
        assert_eq!(app.status, keyboard.status);
    }
    assert!(real.children().is_empty());
    assert_eq!(real.snapshot(), before);
}

#[test]
fn design_footer_v90_repository_keys_tabs_and_compaction() {
    let real = Real::new("sha1");
    let workspace = Workspace::load(&real.root, RepositoryMode::Working, "HEAD", None)
        .unwrap()
        .unwrap();
    let before = real.snapshot();
    real.clear_trace();
    let app = App {
        view: View::Repository,
        workspace: Some(workspace.clone()),
        ..Default::default()
    };
    let mut keys = super::footer_keys(&app);
    keys.extend(crate::footer::global_tail(&app));
    // Every key appears once, so a key press means exactly one control.
    let mut seen = std::collections::HashSet::new();
    assert!(keys.iter().all(|control| seen.insert(control.key.clone())));
    // Modes are tabs, never footer tokens; their keys still resolve.
    let tokens = |width: u16| {
        crate::footer::layout(&keys, width)
            .into_iter()
            .map(|placed| keys[placed.index].token(placed.compact))
            .collect::<Vec<_>>()
    };
    let wide = tokens(140);
    assert!(wide.iter().all(|token| token != "w Working"));
    for token in [
        "+ Stage",
        "- Unstage",
        "K Commit",
        "h Hunk",
        "v Native diff",
        "C Compare",
        "m Local operations",
        "? Help",
        ": Commands",
        "q Quit",
    ] {
        assert!(
            wide.iter().any(|placed| placed == token),
            "{token} in {wide:?}"
        );
    }
    let narrow = tokens(80);
    assert!(narrow.iter().any(|token| token == "v Diff"), "{narrow:?}");
    assert!(narrow.iter().any(|token| token == ": Cmds"), "{narrow:?}");
    for token in ["+ Stage", "- Unstage", "K Commit", "h Hunk", "m Git ops"] {
        assert!(
            narrow.iter().any(|placed| placed == token),
            "{token} in {narrow:?}"
        );
    }
    let total = narrow
        .iter()
        .map(|token| token.chars().count())
        .sum::<usize>()
        + 2 * narrow.len().saturating_sub(1)
        + 1;
    assert!(total <= 80, "{narrow:?}");
    for (code, expected) in [
        (
            KeyCode::Char('w'),
            Action::RepositoryMode(RepositoryMode::Working),
        ),
        (
            KeyCode::Char('g'),
            Action::RepositoryMode(RepositoryMode::Graph),
        ),
        (KeyCode::Char('h'), Action::RepositoryHunk),
        (KeyCode::Char('m'), Action::LocalMenu),
        (KeyCode::Char('C'), Action::Compare),
        (
            KeyCode::Char('K'),
            Action::LocalQuick(crate::local_actions::QuickLocal::Commit),
        ),
    ] {
        assert_eq!(
            crate::footer::resolve_key(&keys, KeyEvent::new(code, KeyModifiers::NONE)),
            Some(expected)
        );
    }
    // A disabled key leaves the bar and explains itself.
    let mut busy = App {
        view: View::Repository,
        workspace: Some(workspace),
        ..Default::default()
    };
    busy.loading[View::Repository.index()] = true;
    let busy_keys = super::footer_keys(&busy);
    let jump = busy_keys.iter().find(|control| control.key == "J").unwrap();
    assert!(jump.disabled.is_some());
    assert!(matches!(
        crate::footer::resolve_key(
            &busy_keys,
            KeyEvent::new(KeyCode::Char('J'), KeyModifiers::NONE)
        ),
        Some(Action::Explain(reason)) if reason.contains("Wait")
    ));
    assert!(crate::footer::layout(&busy_keys, 140)
        .iter()
        .all(|placed| busy_keys[placed.index].key != "J"));
    // Without a repository the modes still resolve and the actions explain.
    let empty = App {
        view: View::Repository,
        ..Default::default()
    };
    let empty_keys = super::footer_keys(&empty);
    assert_eq!(
        crate::footer::resolve_key(
            &empty_keys,
            KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE)
        ),
        Some(Action::RepositoryMode(RepositoryMode::Tree))
    );
    assert!(matches!(
        crate::footer::resolve_key(
            &empty_keys,
            KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE)
        ),
        Some(Action::Explain(_))
    ));
    assert!(real.children().is_empty());
    assert_eq!(real.snapshot(), before);
}

#[test]
fn design_help_v97_repository_help_and_commands_cover_every_footer_key() {
    let real = Real::new("sha1");
    let before = real.snapshot();
    let mut checked = 0;
    for mode in [RepositoryMode::Working, RepositoryMode::Graph] {
        let workspace = Workspace::load(&real.root, mode, "HEAD", None)
            .unwrap()
            .unwrap();
        real.clear_trace();
        let app = App {
            view: View::Repository,
            workspace: Some(workspace),
            ..Default::default()
        };
        checked += crate::navigation_ui::assert_help_and_palette_cover_footer_v97(
            &app,
            &format!("Repository {mode:?}"),
        );
    }
    assert!(checked > 10, "{checked}");
    // Preparing Help and Commands leaves the working tree and index untouched.
    assert_eq!(real.snapshot(), before);
}

/// TASK-67: a saved Tree paints `Enter Open` for its selected file or folder, and that token,
/// the Enter key and a click on the row run the same row action. Other modes keep their bars.
#[test]
fn novice_usability_v67_saved_tree_footer_shows_enter_open_for_the_selected_row() {
    let real = Real::new("sha1");
    let before = real.snapshot();
    let workspace = Workspace::load(&real.root, RepositoryMode::Tree, "HEAD", None)
        .unwrap()
        .unwrap();
    let rows = workspace.display_rows_v1();
    let index = rows
        .iter()
        .position(|row| {
            matches!(
                row.action,
                Some(RowAction::File(_) | RowAction::Directory(_) | RowAction::Tree(_))
            )
        })
        .unwrap_or_else(|| {
            panic!(
                "a saved Tree lists an openable row: {:?}",
                rows.iter().map(|row| &row.label).collect::<Vec<_>>()
            )
        });
    let expected = Action::RepositoryRow(rows[index].action.clone().unwrap());
    let mut app = App {
        view: View::Repository,
        workspace: Some(workspace.clone()),
        ..Default::default()
    };
    app.repository_list.selected = index;
    // While a Tab-focused control owns Enter, the bar does not offer Open.
    app.repository_content_focused = false;
    assert!(super::footer_keys(&app)
        .iter()
        .all(|control| control.key != "Enter"));
    app.repository_content_focused = true;
    let mut keys = super::footer_keys(&app);
    let open = keys
        .iter()
        .find(|control| control.key == "Enter")
        .expect("Enter Open in a saved Tree");
    assert_eq!(open.label, "Open");
    assert_eq!(open.action, expected);
    keys.extend(crate::footer::global_tail(&app));
    let narrow = crate::footer::layout(&keys, 80)
        .into_iter()
        .map(|placed| keys[placed.index].token(placed.compact))
        .collect::<Vec<_>>();
    assert!(
        narrow.iter().any(|token| token == "Enter Open"),
        "{narrow:?}"
    );
    assert!(
        narrow.iter().any(|token| token == "r Refresh"),
        "{narrow:?}"
    );
    assert_eq!(
        crate::footer::resolve_key(&keys, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        Some(expected)
    );
    // A heading row has no action, so the bar does not offer Open for it.
    if let Some(heading) = rows.iter().position(|row| row.action.is_none()) {
        app.repository_list.selected = heading;
        assert!(super::footer_keys(&app)
            .iter()
            .all(|control| control.key != "Enter"));
    }
    // Working mode is unchanged: no Enter Open token.
    let working = App {
        view: View::Repository,
        workspace: Some(
            Workspace::load(&real.root, RepositoryMode::Working, "HEAD", None)
                .unwrap()
                .unwrap(),
        ),
        repository_content_focused: true,
        ..Default::default()
    };
    assert!(super::footer_keys(&working)
        .iter()
        .all(|control| control.label != "Open"));
    assert_eq!(real.snapshot(), before);
}
