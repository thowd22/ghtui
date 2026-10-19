//! Daily-use repository view contracts over real temporary Git repositories.
use crate::{
    diff::FileDiff,
    git::RawPath,
    repository_view::diff_file_title_v1,
    workspace::{RepositoryMode, RowAction, Workspace},
};
use std::{fs, path::Path, process::Command};

fn run(directory: &Path, args: &[&str]) -> String {
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
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn file(old: Option<&str>, new: Option<&str>) -> FileDiff {
    FileDiff {
        old_path: old.map(|path| path.as_bytes().to_vec()),
        new_path: new.map(|path| path.as_bytes().to_vec()),
        hunks: vec![],
        binary: false,
        metadata: vec![],
    }
}

#[test]
fn v1_repo_views_diff_file_titles_name_the_change_plainly() {
    assert_eq!(
        diff_file_title_v1(&file(Some("a.rs"), Some("a.rs"))),
        "a.rs"
    );
    assert_eq!(
        diff_file_title_v1(&file(Some("old.rs"), Some("new.rs"))),
        "Renamed: old.rs → new.rs"
    );
    assert_eq!(diff_file_title_v1(&file(None, Some("n.rs"))), "Added: n.rs");
    assert_eq!(
        diff_file_title_v1(&file(Some("d.rs"), None)),
        "Deleted: d.rs"
    );
    assert_eq!(crate::workspace::change_word_v1("M"), "modified");
    assert_eq!(crate::workspace::change_word_v1("R100"), "renamed");
    assert_eq!(crate::workspace::change_word_v1("A"), "added");
}

#[test]
fn v1_repo_views_commit_changes_hide_raw_index_lines_and_same_name_arrows() {
    let dir = tempfile::tempdir().unwrap();
    run(dir.path(), &["init", "--initial-branch=main"]);
    run(dir.path(), &["config", "user.name", "Fixture"]);
    run(
        dir.path(),
        &["config", "user.email", "fixture@example.invalid"],
    );
    fs::write(dir.path().join("kept.txt"), b"one\n").unwrap();
    fs::write(dir.path().join("gone.txt"), b"bye\n").unwrap();
    run(dir.path(), &["add", "."]);
    run(dir.path(), &["commit", "-m", "base"]);
    fs::write(dir.path().join("kept.txt"), b"two\n").unwrap();
    fs::write(dir.path().join("fresh.txt"), b"new\n").unwrap();
    fs::remove_file(dir.path().join("gone.txt")).unwrap();
    run(dir.path(), &["add", "-A"]);
    run(dir.path(), &["commit", "-m", "change"]);
    let head = run(dir.path(), &["rev-parse", "HEAD"]).trim().to_owned();
    let view = Workspace::load(dir.path(), RepositoryMode::CommitChanges, &head, None)
        .unwrap()
        .unwrap();
    let labels = view
        .rows
        .iter()
        .map(|row| row.label.as_str())
        .collect::<Vec<_>>();
    assert!(labels.contains(&"kept.txt"), "{labels:#?}");
    assert!(labels.contains(&"Added: fresh.txt"), "{labels:#?}");
    assert!(labels.contains(&"Deleted: gone.txt"), "{labels:#?}");
    assert!(!labels.iter().any(|label| label.starts_with("index ")));
    assert!(!labels
        .iter()
        .any(|label| label.contains("kept.txt → kept.txt")));
    assert!(labels.iter().any(|label| label.contains("+two")));
}

#[test]
fn v1_repo_views_tree_lists_folders_first_with_children_under_their_folder() {
    let dir = tempfile::tempdir().unwrap();
    run(dir.path(), &["init", "--initial-branch=main"]);
    run(dir.path(), &["config", "user.name", "Fixture"]);
    run(
        dir.path(),
        &["config", "user.email", "fixture@example.invalid"],
    );
    for path in [
        "src/main.rs",
        "src-old.txt",
        ".config",
        "README",
        "docs/a.md",
    ] {
        let path = dir.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"x\n").unwrap();
    }
    run(dir.path(), &["add", "."]);
    run(dir.path(), &["commit", "-m", "saved"]);
    let mut workspace = Workspace::load(dir.path(), RepositoryMode::Tree, "HEAD", None)
        .unwrap()
        .unwrap();
    workspace
        .open(RowAction::Directory(RawPath::new(b"src".to_vec())))
        .unwrap();
    let rows = workspace
        .tree
        .as_ref()
        .unwrap()
        .visible_rows()
        .into_iter()
        .map(|row| row.path.display())
        .collect::<Vec<_>>();
    assert_eq!(
        rows,
        [
            "docs",
            "src",
            "src/main.rs",
            ".config",
            "README",
            "src-old.txt"
        ]
    );
}

#[test]
fn v1_repo_views_back_from_a_tree_file_reselects_it_and_preserves_checkout() {
    let dir = tempfile::tempdir().unwrap();
    run(dir.path(), &["init", "--initial-branch=main"]);
    run(dir.path(), &["config", "user.name", "Fixture"]);
    run(
        dir.path(),
        &["config", "user.email", "fixture@example.invalid"],
    );
    fs::create_dir(dir.path().join("nested")).unwrap();
    fs::write(dir.path().join("a.txt"), b"a\n").unwrap();
    fs::write(dir.path().join("nested/deep.txt"), b"saved deep\n").unwrap();
    fs::write(dir.path().join("z.txt"), b"z\n").unwrap();
    run(dir.path(), &["add", "."]);
    run(dir.path(), &["commit", "-m", "saved"]);
    fs::write(dir.path().join("nested/deep.txt"), b"dirty\n").unwrap();
    let index = fs::read(dir.path().join(".git/index")).unwrap();
    let mut workspace = Workspace::load(dir.path(), RepositoryMode::Tree, "HEAD", None)
        .unwrap()
        .unwrap();
    assert_eq!(workspace.tree_file_return_v1(), None);
    let nested = RawPath::new(b"nested".to_vec());
    workspace
        .open(RowAction::Directory(nested.clone()))
        .unwrap();
    // Re-rendering after a directory toggle keeps that directory selected.
    assert_eq!(
        workspace
            .tree_focus_row_v1()
            .map(|row| &workspace.rows[row].action),
        Some(&Some(RowAction::Directory(nested)))
    );
    let deep = RawPath::new(b"nested/deep.txt".to_vec());
    workspace.open(RowAction::File(deep.clone())).unwrap();
    assert_eq!(workspace.tree_file_return_v1(), Some(&deep));
    assert_eq!(
        workspace.tree_focus_row_v1(),
        None,
        "the file preview is shown"
    );
    let back = Workspace::load(
        dir.path(),
        RepositoryMode::Tree,
        &workspace.revision,
        workspace.tree.clone(),
    )
    .unwrap()
    .unwrap();
    let row = back.tree_focus_row_v1().expect("the opened file row");
    assert_eq!(back.rows[row].action, Some(RowAction::File(deep)));
    assert!(back.rows[row].label.contains("deep.txt"));
    assert_eq!(
        fs::read(dir.path().join("nested/deep.txt")).unwrap(),
        b"dirty\n"
    );
    assert_eq!(fs::read(dir.path().join(".git/index")).unwrap(), index);
}

/// Draws the whole app and returns the buffer plus its lines.
fn draw_overview(
    app: &mut crate::app::App,
    width: u16,
    height: u16,
) -> (ratatui::buffer::Buffer, Vec<String>) {
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| crate::ui::draw(frame, app)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    let lines = (0..height)
        .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
        .collect();
    (buffer, lines)
}

fn commit_x(app: &crate::app::App, head: &str) -> u16 {
    app.hits
        .iter()
        .find(|hit| {
            hit.action == crate::app::Action::RepositoryRow(RowAction::Commit(head.to_owned()))
        })
        .unwrap()
        .rect
        .x
}

#[test]
fn design_table_v91_repository_overview_commits_align_and_keep_full_sha_in_details() {
    use crate::app::Action;
    let dir = tempfile::tempdir().unwrap();
    run(dir.path(), &["init", "--initial-branch=main"]);
    run(dir.path(), &["config", "user.name", "Fixture Author"]);
    run(
        dir.path(),
        &["config", "user.email", "fixture@example.invalid"],
    );
    let long = "Explain why a stale core.worktree pointing elsewhere confuses discovery";
    let subjects = ["First note", "Explain why", "Second note"];
    for (file, subject) in [
        ("a.txt", "First note"),
        ("b.txt", long),
        ("c.txt", "Second note"),
    ] {
        fs::write(dir.path().join(file), subject.as_bytes()).unwrap();
        run(dir.path(), &["add", file]);
        run(dir.path(), &["commit", "-m", subject]);
    }
    fs::write(dir.path().join("a.txt"), b"uncommitted edit\n").unwrap();
    let head = run(dir.path(), &["rev-parse", "HEAD"]).trim().to_owned();
    let seconds = run(dir.path(), &["log", "-1", "--format=%at"])
        .trim()
        .parse::<i64>()
        .unwrap();
    let date = crate::git_actor_labels_v1::utc_date(seconds);
    let index = fs::read(dir.path().join(".git/index")).unwrap();
    let workspace = Workspace::load(dir.path(), RepositoryMode::Overview, "HEAD", None)
        .unwrap()
        .unwrap();
    let first = workspace
        .rows
        .iter()
        .position(|row| row.action == Some(RowAction::Commit(head.clone())))
        .unwrap();
    // The producer keeps its label (palette, plans) and adds the table cells.
    let row = &workspace.rows[first];
    assert!(row.label.contains(&head[..8]), "{}", row.label);
    let cells = row.cells.as_deref().unwrap();
    assert_eq!(cells.id, head);
    assert_eq!(cells.subject, "Second note");
    assert_eq!(cells.actors, "Fixture Author");
    assert_eq!(cells.authored, seconds);
    let mut app = crate::app::App {
        view: crate::app::View::Repository,
        repository_list: crate::ui_table::ListState {
            selected: first,
            ..Default::default()
        },
        workspace: Some(workspace),
        repository_content_focused: true,
        ..Default::default()
    };
    let selection = app.theme().role(crate::theme::Role::Selection).bg.unwrap();
    let accent = app.theme().role(crate::theme::Role::Accent).fg.unwrap();
    for (width, height) in [(140u16, 42u16), (80, 24)] {
        let (buffer, lines) = draw_overview(&mut app, width, height);
        let screen = lines.join("\n");
        // The panels only: at 140 the preview pane repeats the selected subject.
        let list_width = app
            .preview_area_v92
            .map_or(usize::from(width), |area| usize::from(area.x));
        let list_lines = lines
            .iter()
            .map(|line| line.chars().take(list_width).collect::<String>())
            .collect::<Vec<_>>();
        let commit_lines = list_lines
            .iter()
            .enumerate()
            .filter(|(_, line)| subjects.iter().any(|subject| line.contains(subject)))
            .collect::<Vec<_>>();
        assert_eq!(commit_lines.len(), 3, "{screen}");
        // Every subject starts in the same column, right after a 7-character id.
        let subject_x = commit_lines
            .iter()
            .map(|(_, line)| {
                let byte = subjects
                    .iter()
                    .find_map(|subject| line.find(subject))
                    .unwrap();
                crate::editor::display_width(&line[..byte])
            })
            .collect::<Vec<_>>();
        assert!(
            subject_x.windows(2).all(|pair| pair[0] == pair[1]),
            "{screen}"
        );
        let (y, selected) = commit_lines
            .iter()
            .find(|(_, line)| line.contains("Second note"))
            .unwrap();
        assert!(
            selected.contains(&format!("{}  Second note", &head[..7])),
            "{screen}"
        );
        assert!(!selected.contains(&head[..8]), "{selected}");
        assert!(selected.contains("Fixture Author"), "{selected}");
        // In the Recent commits panel (TASK-93) the long subject is cut with an ellipsis,
        // never wrapped; at 140 the preview beside the panels takes the rest of the width.
        assert!(
            !commit_lines.iter().any(|(_, line)| line.contains(long)),
            "{screen}"
        );
        assert!(
            commit_lines
                .iter()
                .any(|(_, line)| line.contains("Explain why") && line.contains('…')),
            "{screen}"
        );
        // The UTC date leaves the narrow panel first; the full id stays in the Saved detail
        // row, and at 140 the preview shows the full id with the authored UTC time.
        assert!(!selected.contains(&date), "{screen}");
        assert!(screen.contains(&format!("Saved: {head}")), "{screen}");
        assert_eq!(
            screen.contains(&format!("Commit    {head}")),
            width == 140,
            "{screen}"
        );
        assert_eq!(
            screen.contains(&format!("Authored  {date}")),
            width == 140,
            "{screen}"
        );
        // `▌` bar on the Selection background, no reverse video, accent while focused.
        let hit = app
            .hits
            .iter()
            .find(|hit| hit.action == Action::RepositoryRow(RowAction::Commit(head.clone())))
            .unwrap();
        assert_eq!(usize::from(hit.rect.y), *y);
        assert_eq!(buffer[(hit.rect.x, hit.rect.y)].symbol(), "▌");
        assert_eq!(buffer[(hit.rect.x, hit.rect.y)].fg, accent);
        for x in hit.rect.x..hit.rect.right() {
            let cell = &buffer[(x, hit.rect.y)];
            assert_eq!(cell.bg, selection, "x={x}\n{screen}");
            assert!(!cell.modifier.contains(ratatui::style::Modifier::REVERSED));
        }
        // Every commit row stays a click target in its panel; the old section heading is
        // now the panel title.
        assert_eq!(
            app.hits
                .iter()
                .filter(
                    |hit| matches!(hit.action, Action::RepositoryRow(RowAction::Commit(_)))
                        && hit.rect.x == commit_x(&app, &head)
                )
                .count(),
            3
        );
        assert!(lines
            .iter()
            .any(|line| line.contains("▶ Recent commits — saved versions ─")));
    }
    // Read-only browsing leaves the checkout alone.
    assert_eq!(fs::read(dir.path().join(".git/index")).unwrap(), index);
    assert_eq!(
        fs::read(dir.path().join("a.txt")).unwrap(),
        b"uncommitted edit\n"
    );
}
