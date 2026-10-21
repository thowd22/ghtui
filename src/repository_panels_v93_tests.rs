//! TASK-93 Repository panels: the Overview as a Files panel (untracked files collapsed to a
//! count and a toggle) and a Recent commits panel, previewed from 130 columns; rows keep their
//! click targets, the wheel scrolls one panel, keys walk the visible rows, and browsing leaves
//! the working tree and index unchanged.
use super::*;
use crate::{
    app::{App, View},
    workspace::RepositoryMode,
};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};
use std::{fs, path::Path, process::Command};

fn git(directory: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Two commits, one modified tracked file, one staged new file and four untracked files.
fn fixture() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();
    git(path, &["init", "--initial-branch=main"]);
    git(path, &["config", "user.name", "Fixture Author"]);
    git(path, &["config", "user.email", "fixture@example.invalid"]);
    for (file, subject) in [("a.txt", "First note"), ("b.txt", "Second note")] {
        fs::write(path.join(file), subject).unwrap();
        git(path, &["add", file]);
        git(path, &["commit", "-m", subject]);
    }
    fs::write(path.join("a.txt"), "edited\n").unwrap();
    fs::write(path.join("staged.txt"), "new\n").unwrap();
    git(path, &["add", "staged.txt"]);
    for index in 0..4 {
        fs::write(path.join(format!("noise-{index}.log")), "noise\n").unwrap();
    }
    let head = git(path, &["rev-parse", "HEAD"]).trim().to_owned();
    (dir, head)
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

fn draw(app: &mut App, width: u16, height: u16) -> (Buffer, Vec<String>) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| crate::ui::draw(frame, app)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    let lines = (0..height)
        .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
        .collect();
    (buffer, lines)
}

fn key(app: &mut App, code: KeyCode) {
    app.handle_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
}

fn mouse(app: &mut App, kind: MouseEventKind, column: u16, row: u16) {
    app.handle_event(Event::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }));
}

fn row_index(app: &App, action: &RowAction) -> usize {
    app.workspace
        .as_ref()
        .unwrap()
        .display_rows_v1()
        .iter()
        .position(|row| row.action.as_ref() == Some(action))
        .unwrap()
}

fn file(name: &str) -> RowAction {
    RowAction::WorkingFile(crate::git::RawPath(name.as_bytes().to_vec()))
}

#[test]
fn design_panels_v93_sort_collapses_untracked_files_into_a_count() {
    let (dir, head) = fixture();
    let app = app(dir.path());
    let workspace = app.workspace.as_ref().unwrap();
    assert!(active(&app));
    let collapsed = sort(workspace, false);
    let rows = workspace.display_rows_v1();
    assert_eq!(collapsed.untracked, 4);
    let labels = |indexes: &[usize]| {
        indexes
            .iter()
            .map(|index| rows[*index].label.clone())
            .collect::<Vec<_>>()
    };
    let files = labels(&collapsed.files);
    assert!(files.iter().any(|label| label.starts_with("Branch: main")));
    assert!(files.iter().any(|label| label.ends_with("a.txt")));
    assert!(files.iter().any(|label| label.ends_with("staged.txt")));
    assert!(!files.iter().any(|label| label.contains("noise-")));
    // The commits move to their own panel; the old heading row is hidden.
    let commits = labels(&collapsed.commits);
    assert_eq!(commits.len(), 2, "{commits:?}");
    assert!(commits[0].contains(&head[..8]));
    let heading = rows
        .iter()
        .position(|row| row.label == COMMITS_TITLE)
        .unwrap();
    assert!(collapsed.hidden[heading]);
    assert_eq!(collapsed.hidden.iter().filter(|hidden| **hidden).count(), 5);
    let shown = sort(workspace, true);
    assert_eq!(shown.untracked, 4);
    assert_eq!(shown.files.len(), collapsed.files.len() + 4);
    assert_eq!(shown.hidden.iter().filter(|hidden| **hidden).count(), 1);
    // Panel heights: both fit, or the smaller need is met first.
    assert_eq!(heights(30, 5, 3), (7, 23));
    assert_eq!(heights(12, 20, 3), (7, 5));
    assert_eq!(heights(12, 3, 20), (5, 7));
    assert_eq!(heights(12, 20, 20), (6, 6));
}

#[test]
fn design_panels_v93_draw_files_and_commits_panels_with_preview_at_140_and_stacked_at_80() {
    let (dir, head) = fixture();
    let index_bytes = fs::read(dir.path().join(".git/index")).unwrap();
    let mut app = app(dir.path());
    let selection = app.theme().role(crate::theme::Role::Selection).bg.unwrap();
    let commit = row_index(&app, &RowAction::Commit(head.clone()));
    app.repository_list.selected = commit;
    for (width, height) in [(140u16, 42u16), (80, 24)] {
        let (buffer, lines) = draw(&mut app, width, height);
        let screen = lines.join("\n");
        assert!(screen.contains("Repository • focused view"), "{screen}");
        assert!(screen.contains("Files · 6 changed"), "{screen}");
        assert!(screen.contains(COMMITS_TITLE), "{screen}");
        assert!(
            screen.contains("untracked: 4 hidden · u shows them"),
            "{screen}"
        );
        assert!(!screen.contains("noise-0.log"), "{screen}");
        // File rows are aligned cells: status, stage, path.
        let a = lines.iter().find(|line| line.contains("a.txt")).unwrap();
        let staged = lines
            .iter()
            .find(|line| line.contains("staged.txt"))
            .unwrap();
        assert!(a.contains("Modified"), "{a}");
        assert!(a.contains("unstaged"), "{a}");
        assert!(staged.contains("Added"), "{staged}");
        assert_eq!(a.find("a.txt"), staged.find("staged.txt"), "{screen}");
        assert!(!screen.contains("[unstaged]"), "{screen}");
        // The selected commit carries the `▌` bar on the Selection background.
        let hit = app
            .hits
            .iter()
            .find(|hit| {
                hit.action == Action::RepositoryRow(RowAction::Commit(head.clone()))
                    && hit.rect.width > 20
            })
            .unwrap();
        assert_eq!(buffer[(hit.rect.x, hit.rect.y)].symbol(), "▌");
        assert_eq!(buffer[(hit.rect.x + 3, hit.rect.y)].bg, selection);
        assert!(lines[usize::from(hit.rect.y)].contains(&head[..7]));
        // The full id stays in the Saved detail row at every width.
        assert!(screen.contains(&format!("Viewed: {head}")), "{screen}");
        // Every visible file and commit row is a click target, plus the toggle.
        for action in [
            file("a.txt"),
            file("staged.txt"),
            RowAction::Commit(head.clone()),
        ] {
            assert!(
                app.hits
                    .iter()
                    .any(|hit| hit.action == Action::RepositoryRow(action.clone())),
                "{action:?}\n{screen}"
            );
        }
        assert!(app
            .hits
            .iter()
            .any(|hit| hit.action == Action::RepositoryPanelsV93(PanelActionV93::ToggleUntracked)));
        let previewed = app.preview_area_v92.is_some();
        assert_eq!(previewed, width == 140, "{screen}");
        if previewed {
            // The preview: the full id and both UTC times of the selected version.
            assert!(screen.contains(&format!("Commit    {head}")), "{screen}");
            assert!(screen.contains("Authored"), "{screen}");
            assert!(screen.contains(" UTC"), "{screen}");
            assert!(screen.contains("Fixture Author <fixture@example.invalid>"));
            assert!(screen.contains("Enter Open"), "{screen}");
        }
    }
    // Browsing read nothing into the checkout.
    assert_eq!(
        fs::read(dir.path().join(".git/index")).unwrap(),
        index_bytes
    );
    assert_eq!(fs::read(dir.path().join("a.txt")).unwrap(), b"edited\n");
}

#[test]
fn design_panels_v93_toggle_untracked_by_key_and_click_and_keys_skip_collapsed_rows() {
    let (dir, _) = fixture();
    let mut app = app(dir.path());
    let a = row_index(&app, &file("a.txt"));
    app.repository_list.selected = a;
    draw(&mut app, 80, 24);
    // The key bar names the toggle; `u` resolves to the same action as its click target.
    let keys = crate::repository_ui::footer_keys(&app);
    let toggle = keys
        .iter()
        .find(|control| control.key == "u")
        .expect("untracked toggle in the key bar");
    assert_eq!(
        toggle.action,
        Action::RepositoryPanelsV93(PanelActionV93::ToggleUntracked)
    );
    assert_eq!(toggle.label, "Show untracked");
    // Down walks the visible file rows and then the commits, never a collapsed file.
    let staged = row_index(&app, &file("staged.txt"));
    let mut visited = vec![];
    for _ in 0..6 {
        key(&mut app, KeyCode::Down);
        visited.push(app.repository_list.selected);
    }
    let rows = app.workspace.as_ref().unwrap().display_rows_v1().to_vec();
    assert!(visited.contains(&staged) || a > staged);
    assert!(
        visited
            .iter()
            .all(|index| !rows[*index].label.contains("noise-")),
        "{visited:?}"
    );
    // `u` shows them; the panel paints them and keyboard movement reaches them.
    key(&mut app, KeyCode::Char('u'));
    assert!(app.repository_panels_v93.show_untracked);
    let (_, lines) = draw(&mut app, 80, 40);
    let screen = lines.join("\n");
    assert!(screen.contains("noise-0.log"), "{screen}");
    assert!(
        screen.contains("untracked: 4 shown · u hides them"),
        "{screen}"
    );
    let noise = row_index(&app, &file("noise-0.log"));
    app.repository_list.selected = noise;
    draw(&mut app, 80, 40);
    // Clicking the toggle row collapses them again and moves the selection off them, so
    // Stage cannot reach a file that is no longer painted.
    let rect = app
        .hits
        .iter()
        .find(|hit| hit.action == Action::RepositoryPanelsV93(PanelActionV93::ToggleUntracked))
        .unwrap()
        .rect;
    mouse(
        &mut app,
        MouseEventKind::Down(MouseButton::Left),
        rect.x + 4,
        rect.y,
    );
    assert!(!app.repository_panels_v93.show_untracked);
    assert!(!selection_hidden(&app));
    assert_ne!(app.repository_list.selected, noise);
    app.repository_list.selected = noise;
    app.dispatch(Action::LocalQuick(crate::local_actions::QuickLocal::Stage));
    assert!(app.local_command.is_none());
    assert!(app.status.contains("collapsed"), "{}", app.status);
    assert!(git(dir.path(), &["status", "--porcelain"]).contains("?? noise-0.log"));
}

#[test]
fn design_panels_v93_wheel_scrolls_the_panel_under_the_pointer_only() {
    let (dir, _) = fixture();
    for index in 0..30 {
        fs::write(dir.path().join(format!("tracked-{index:02}.txt")), "x").unwrap();
    }
    git(dir.path(), &["add", "."]);
    git(
        dir.path(),
        &[
            "reset",
            "-q",
            "--",
            "noise-0.log",
            "noise-1.log",
            "noise-2.log",
            "noise-3.log",
        ],
    );
    let mut app = app(dir.path());
    draw(&mut app, 80, 24);
    let files = app.repository_panels_v93.areas[0];
    let commits = app.repository_panels_v93.areas[1];
    assert!(!files.is_empty() && !commits.is_empty());
    let before = app.repository_panels_v93.tops;
    mouse(
        &mut app,
        MouseEventKind::ScrollDown,
        files.x + 5,
        files.y + 2,
    );
    assert_eq!(app.repository_panels_v93.tops[0], before[0] + 3);
    assert_eq!(app.repository_panels_v93.tops[1], before[1]);
    let (_, lines) = draw(&mut app, 80, 24);
    // The wheel moves the view, not the selection: the viewport stays where it was put.
    assert_eq!(app.repository_panels_v93.tops[0], before[0] + 3);
    assert!(lines.join("\n").contains("Files ·"));
}

/// The changed-files picker over a real two-commit comparison, open in Repository.
fn picker_app() -> (crate::local_commit_actor_origin_v1_test_support::Real, App) {
    use crate::original_diff_sides_v1_test_support as support;
    let (real, a, b, _) = support::fixture("sha256");
    let workspace = support::workspace(
        &real,
        crate::git::Comparison::BranchChanges { base: a, head: b },
    );
    let picker = support::state(&workspace);
    let app = App {
        view: View::Repository,
        workspace: Some(workspace),
        original_diff_picker_v1: Some(picker),
        ..Default::default()
    };
    (real, app)
}

/// The row and column of the rounded frame whose top border carries `title`.
fn top_left(lines: &[String], title: &str) -> Option<(usize, usize)> {
    lines.iter().enumerate().find_map(|(y, line)| {
        if !line.contains(title) {
            return None;
        }
        line.find('╭').map(|x| (y, line[..x].chars().count()))
    })
}

fn click_at(app: &mut App, column: u16, row: u16) {
    app.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }));
}

fn column_of(line: &str, text: &str) -> u16 {
    let x = line
        .find(text)
        .unwrap_or_else(|| panic!("{text} in {line}"));
    line[..x].chars().count() as u16
}

#[test]
fn design_panels_v93_takeover_draws_inside_the_content_pane_at_80_and_140() {
    const TITLE: &str = "Compare saved file sides";
    for (width, height) in [(80u16, 24u16), (140, 42)] {
        let (_real, mut app) = picker_app();
        let (_, lines) = draw(&mut app, width, height);
        let header = usize::from(crate::ui_chrome::header_rows(&app, width));
        // The shell header keeps the tabs; the viewer's frame starts right under it.
        assert!(lines[0].contains("Repository"), "{lines:#?}");
        assert_eq!(top_left(&lines, TITLE), Some((header, 0)), "{lines:#?}");
        // Its bottom border sits right above the key bar, which leads with Back.
        let bottom = &lines[usize::from(height) - 2];
        assert!(
            bottom.starts_with('╰') && bottom.ends_with('╯'),
            "{lines:#?}"
        );
        let footer = &lines[usize::from(height) - 1];
        assert!(
            footer.contains("Esc Back") && footer.contains("Ctrl-C Quit"),
            "{footer}"
        );
        // The viewer keeps its own controls inside the pane.
        assert!(lines[usize::from(height) - 3].contains("[Esc Close]"));
        let pane = app.takeover_v93.pane.unwrap();
        assert_eq!(
            (pane.y, pane.height),
            (header as u16, height - header as u16 - 1)
        );
        assert!(app.original_diff_picker_v1.is_some());
    }
}

#[test]
fn design_panels_v93_takeover_keyboard_esc_and_back_key_close_the_viewer() {
    let (_real, mut app) = picker_app();
    draw(&mut app, 80, 24);
    key(&mut app, KeyCode::Esc);
    assert!(app.original_diff_picker_v1.is_none());
    assert_eq!(app.view, View::Repository);
    let (_real, mut app) = picker_app();
    draw(&mut app, 140, 42);
    // The configured Back key (Alt-←) sends the viewer its own Esc.
    app.handle_event(Event::Key(KeyEvent::new(KeyCode::Left, KeyModifiers::ALT)));
    assert!(app.original_diff_picker_v1.is_none());
    assert_eq!(app.view, View::Repository);
    let (_, lines) = draw(&mut app, 140, 42);
    assert!(top_left(&lines, "Compare saved file sides").is_none());
}

#[test]
fn design_panels_v93_takeover_mouse_header_back_footer_and_tabs_pass_through() {
    for (width, height) in [(80u16, 24u16), (140, 42)] {
        // The key bar's Esc Back closes the viewer.
        let (_real, mut app) = picker_app();
        let (_, lines) = draw(&mut app, width, height);
        let x = column_of(&lines[usize::from(height) - 1], "Esc Back");
        click_at(&mut app, x, height - 1);
        assert!(app.original_diff_picker_v1.is_none(), "{width}");
        // The header's Back does the same.
        let (_real, mut app) = picker_app();
        draw(&mut app, width, height);
        let back = app
            .takeover_v93
            .hits
            .iter()
            .find(|(_, action)| matches!(action, takeover::ChromeActionV93::Back))
            .map(|(rect, _)| *rect)
            .unwrap();
        assert!(back.y < app.takeover_v93.pane.unwrap().y);
        click_at(&mut app, back.x, back.y);
        assert!(app.original_diff_picker_v1.is_none(), "{width}");
        // A view tab switches views and the viewer retires.
        let (_real, mut app) = picker_app();
        let (_, lines) = draw(&mut app, width, height);
        let x = column_of(&lines[0], "Search");
        click_at(&mut app, x, 0);
        assert_eq!(app.view, View::Search);
        draw(&mut app, width, height);
        assert!(app.original_diff_picker_v1.is_none());
        // A click inside the pane stays the viewer's own (its Close button).
        let (_real, mut app) = picker_app();
        let (_, lines) = draw(&mut app, width, height);
        let row = usize::from(height) - 3;
        let x = column_of(&lines[row], "[Esc Close]");
        click_at(&mut app, x + 1, row as u16);
        assert!(app.original_diff_picker_v1.is_none(), "{width}");
        assert_eq!(app.view, View::Repository);
    }
}
