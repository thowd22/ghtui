//! TASK-118/119: the worktree stack, the way back and the stale `core.worktree` screen on a
//! real App and real temporary repositories; browsing changes no Git state.
use super::*;
use crate::{
    app::{App, View},
    repository_panels_v93::{sort, Panel, PanelActionV93},
    workspace::{OverviewLinkV119, RepositoryMode, RowAction, Workspace},
};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{backend::TestBackend, Terminal};
use std::process::Command;

fn git(directory: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "Fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// A main checkout with three linked worktrees under one folder; `two` has changes.
fn fixture() -> (tempfile::TempDir, PathBuf, Vec<PathBuf>) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("main");
    std::fs::create_dir(&root).unwrap();
    git(&root, &["init", "-q", "--initial-branch=main"]);
    std::fs::write(root.join("a.txt"), "a\n").unwrap();
    git(&root, &["add", "a.txt"]);
    git(&root, &["commit", "-qm", "First note"]);
    let mut linked = vec![];
    for name in ["one", "two", "three"] {
        git(&root, &["branch", &format!("agent/{name}")]);
        let path = temp.path().join("trees").join(name);
        git(
            &root,
            &[
                "worktree",
                "add",
                "-q",
                path.to_str().unwrap(),
                &format!("agent/{name}"),
            ],
        );
        linked.push(path);
    }
    std::fs::write(linked[1].join("a.txt"), "changed\n").unwrap();
    (temp, root, linked)
}

fn app(path: &Path) -> App {
    let workspace = Workspace::load(path, RepositoryMode::Overview, "HEAD", None)
        .unwrap()
        .unwrap();
    App {
        view: View::Repository,
        workspace: Some(workspace),
        repository_content_focused: true,
        ..Default::default()
    }
}

fn draw(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| crate::ui::draw(frame, app)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn key(app: &mut App, code: KeyCode) {
    app.handle_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
}

fn refs(root: &Path) -> String {
    git(root, &["for-each-ref", "--format=%(refname) %(objectname)"])
}

#[test]
fn worktree_stack_v118_overview_sections_expand_open_and_come_back() {
    let (_temp, root, linked) = fixture();
    let before = refs(&root);
    let mut app = app(&root);
    let rows = app.workspace.as_ref().unwrap().display_rows_v1().to_vec();
    // Plain sections: no internal words in the Overview rows.
    for row in &rows {
        for word in ["saved version", "In use", "Viewed"] {
            assert!(!row.label.contains(word), "{}", row.label);
        }
    }
    let sorted = sort(app.workspace.as_ref().unwrap(), &app.repository_panels_v93);
    for panel in [
        Panel::Branch,
        Panel::Changes,
        Panel::Worktrees,
        Panel::Stashes,
        Panel::Commits,
    ] {
        assert!(sorted.present[panel.index()], "{panel:?}");
    }
    // The stack is collapsed behind its summary row.
    let summary = rows
        .iter()
        .position(|row| {
            row.action == Some(RowAction::OverviewV119(OverviewLinkV119::ToggleWorktrees))
        })
        .unwrap();
    assert!(rows[summary]
        .label
        .starts_with("4 worktrees: this checkout + 3 linked · 1 with changes"));
    let stack: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| matches!(row.action, Some(RowAction::OpenWorktree(_))))
        .map(|(index, _)| index)
        .collect();
    assert_eq!(stack.len(), 4);
    assert!(stack.iter().all(|index| sorted.hidden[*index]));
    let screen = draw(&mut app, 80, 24);
    assert!(screen.contains("▸ 4 worktrees"), "{screen}");
    assert!(!screen.contains("⎇ agent/two"), "{screen}");
    // The full id and UTC time of the last commit stay visible at 80 columns.
    let head = git(&root, &["rev-parse", "HEAD"]).trim().to_owned();
    assert!(screen.contains("Last commit · "), "{screen}");
    assert!(screen.contains(&format!("  {head}")), "{screen}");
    // Enter on the summary row expands the stack (keyboard).
    app.repository_list.selected = summary;
    key(&mut app, KeyCode::Enter);
    assert!(app.repository_panels_v93.show_worktrees);
    let screen = draw(&mut app, 120, 40);
    assert!(screen.contains("▾ 4 worktrees"), "{screen}");
    let main_row = screen.lines().find(|line| line.contains("⎇ main")).unwrap();
    assert!(
        main_row.contains("main checkout · you are here · clean"),
        "{main_row}"
    );
    let two = screen
        .lines()
        .find(|line| line.contains("⎇ agent/two"))
        .unwrap();
    assert!(two.contains("1 changed · …/two"), "{two}");
    assert!(screen.contains("Linked worktrees are in"), "{screen}");
    // A click on a worktree row opens it in place, remembering the way back.
    let hit = app
        .hits
        .iter()
        .find(|hit| hit.action == Action::RepositoryRow(RowAction::OpenWorktree(linked[1].clone())))
        .unwrap()
        .clone();
    app.dispatch(hit.action);
    assert_eq!(
        app.repository_directory_request_v41,
        Some(linked[1].clone())
    );
    assert!(app.repository_load_requested);
    assert!(app
        .repository_home_v118
        .as_ref()
        .is_some_and(|home| same(home, &root)));
    // The loaded worktree names itself and offers the way back by key and click.
    app.workspace = Some(
        Workspace::load(&linked[1], RepositoryMode::Overview, "HEAD", None)
            .unwrap()
            .unwrap(),
    );
    assert!(away(&app).is_some_and(|back| back.starts_with("Back to ")));
    let screen = draw(&mut app, 120, 40);
    assert!(screen.contains("Branch · Worktree: agent/two"), "{screen}");
    assert!(screen.contains("◂ Back to "), "{screen}");
    assert!(app
        .hits
        .iter()
        .any(|hit| hit.action == Action::WorktreeV118(WorktreeActionV118::Back)));
    let keys = crate::repository_ui::footer_keys(&app);
    assert!(keys.iter().any(|control| control.key == "B"
        && control.action == Action::WorktreeV118(WorktreeActionV118::Back)));
    app.repository_load_requested = false;
    key(&mut app, KeyCode::Char('B'));
    assert!(app.repository_load_requested);
    assert!(app
        .repository_directory_request_v41
        .as_ref()
        .is_some_and(|path| same(path, &root)));
    assert_eq!(app.repository_home_v118, None);
    // Browsing changed no refs, no file and no index.
    assert_eq!(refs(&root), before);
    assert_eq!(git(&linked[1], &["status", "--porcelain"]), " M a.txt\n");
    assert_eq!(git(&root, &["status", "--porcelain"]), "");
}

#[test]
fn overview_v119_change_rows_open_working_and_staged_and_recovery_is_collapsed() {
    let (_temp, root, _) = fixture();
    std::fs::write(root.join("a.txt"), "edited\n").unwrap();
    std::fs::write(root.join("b.txt"), "staged\n").unwrap();
    git(&root, &["add", "b.txt"]);
    // A commit left behind by an amend shows up behind the recovery entry.
    git(&root, &["commit", "-qm", "Draft"]);
    git(&root, &["commit", "-q", "--amend", "-m", "Final"]);
    std::fs::write(root.join("b.txt"), "staged again\n").unwrap();
    git(&root, &["add", "b.txt"]);
    let mut app = app(&root);
    let rows = app.workspace.as_ref().unwrap().display_rows_v1().to_vec();
    let recovery = rows
        .iter()
        .position(|row| {
            row.action == Some(RowAction::OverviewV119(OverviewLinkV119::ToggleRecovery))
        })
        .unwrap();
    let sorted = sort(app.workspace.as_ref().unwrap(), &app.repository_panels_v93);
    let rescue = rows
        .iter()
        .position(|row| row.label.starts_with("↺ "))
        .unwrap();
    assert!(sorted.hidden[rescue] && !sorted.hidden[recovery]);
    // The first Down from the Branch row still reaches the first changed file: the counts
    // and buttons are links outside the keyboard order.
    let branch = rows
        .iter()
        .position(|row| row.label.starts_with("Branch: main"))
        .unwrap();
    app.repository_list.selected = branch;
    key(&mut app, KeyCode::Down);
    assert!(matches!(
        rows[app.repository_list.selected].action,
        Some(RowAction::WorkingFile(_))
    ));
    let screen = draw(&mut app, 120, 40);
    assert!(
        screen.contains("Changes · 1 staged · 1 unstaged"),
        "{screen}"
    );
    assert!(
        screen.contains("Unstaged: 1 file changed · w opens the Working diff"),
        "{screen}"
    );
    assert!(
        screen.contains("Staged: 1 file ready to commit · i opens the Staged diff"),
        "{screen}"
    );
    assert!(screen.contains("[Push to publish this branch"), "{screen}");
    for action in [
        Action::RepositoryMode(RepositoryMode::Working),
        Action::RepositoryMode(RepositoryMode::Staged),
        Action::LocalCommand(crate::local_actions::LocalCommand::Push),
    ] {
        assert!(
            app.hits.iter().any(|hit| hit.action == action),
            "{action:?}"
        );
    }
    // The Push button's key is in the bar.
    assert!(crate::repository_ui::footer_keys(&app)
        .iter()
        .any(|control| control.key == "p"
            && control.action == Action::LocalCommand(crate::local_actions::LocalCommand::Push)));
    // A click on the recovery entry shows the candidates; the Staged row opens its diff.
    let hit = app
        .hits
        .iter()
        .find(|hit| {
            hit.action
                == Action::RepositoryRow(RowAction::OverviewV119(OverviewLinkV119::ToggleRecovery))
        })
        .unwrap()
        .clone();
    app.dispatch(hit.action);
    assert!(app.repository_panels_v93.show_recovery);
    let screen = draw(&mut app, 120, 40);
    assert!(screen.contains("↺ "), "{screen}");
    app.dispatch(Action::RepositoryPanelsV93(PanelActionV93::ToggleRecovery));
    assert!(!app.repository_panels_v93.show_recovery);
    // A click on the Staged count opens the Staged diff.
    app.dispatch(Action::RepositoryMode(RepositoryMode::Staged));
    assert_eq!(app.repository_mode, RepositoryMode::Staged);
    assert!(app.repository_load_requested);
}

#[test]
fn core_worktree_v118_screen_offers_choices_previews_and_confirms_by_key_and_click() {
    let (temp, root, linked) = fixture();
    let elsewhere = temp.path().join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    git(
        &root,
        &["config", "core.worktree", elsewhere.to_str().unwrap()],
    );
    let (workspace, mismatch) = load(
        &root,
        RepositoryMode::Overview,
        "HEAD",
        None,
        Default::default(),
        false,
    )
    .unwrap();
    assert!(workspace.is_none());
    let mut app = App {
        view: View::Repository,
        ..Default::default()
    };
    install(&mut app, mismatch.unwrap());
    let screen = draw(&mut app, 80, 24);
    for text in [
        "This checkout's Git settings point at another folder",
        "Git uses instead:",
        "core.worktree =",
        "O  Open the folder Git uses",
        "W  Show the linked worktrees of this repository (3)",
        "m  Local operations: unset core.worktree",
    ] {
        assert!(screen.contains(text), "{text}\n{screen}");
    }
    // Keyboard: Down then Enter shows the linked worktrees; a click opens one.
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Enter);
    let screen = draw(&mut app, 80, 24);
    assert!(screen.contains("⎇ agent/one"), "{screen}");
    let open = app
        .hits
        .iter()
        .find(|hit| hit.action == Action::WorktreeV118(WorktreeActionV118::Open(linked[0].clone())))
        .unwrap()
        .clone();
    app.dispatch(open.action);
    assert_eq!(
        app.repository_directory_request_v41,
        Some(linked[0].clone())
    );
    app.repository_directory_request_v41 = None;
    // `m` previews; Esc cancels; the preview again and `y` asks the worker to unset.
    key(&mut app, KeyCode::Char('m'));
    let screen = draw(&mut app, 100, 30);
    assert!(screen.contains("Preview: unset core.worktree"), "{screen}");
    assert!(screen.contains("config --unset core.worktree"), "{screen}");
    assert!(screen.contains("no files change"), "{screen}");
    key(&mut app, KeyCode::Esc);
    assert!(!app.core_worktree_v118.as_ref().unwrap().preview);
    assert!(app.core_worktree_unset_v118.is_none());
    key(&mut app, KeyCode::Char('m'));
    key(&mut app, KeyCode::Char('y'));
    let request = app.core_worktree_unset_v118.take().unwrap();
    // Nothing was written before the worker runs the confirmed request.
    assert_eq!(
        git(&root, &["config", "core.worktree"]).trim(),
        elsewhere.to_str().unwrap()
    );
    let (workspace, mismatch) = unset_and_load(&request, Default::default()).unwrap();
    assert!(mismatch.is_none());
    assert!(workspace.is_some_and(|workspace| same(&workspace.directory, &root)));
    assert!(Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["config", "core.worktree"])
        .output()
        .unwrap()
        .stdout
        .is_empty());
}
