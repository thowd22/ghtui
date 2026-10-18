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
