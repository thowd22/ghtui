//! Source-only additional actual installed-Git failure contracts, unregistered.
use super::*;
use crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1;
use crate::workspace::original_saved_navigation_v1::tests::NestedReal;
use std::fs;

fn operation(workspace: &Workspace) -> LocalGitNavigationReadV1 {
    LocalGitNavigationReadV1::begin(workspace.actor_origin.as_ref().unwrap(), Default::default())
        .unwrap()
}
fn held(fixture: &NestedReal, sha: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    assert!(crate::git::original_actor_saved_sha(sha));
    let path = fixture
        .real
        .root
        .join(".git/objects")
        .join(&sha[..2])
        .join(&sha[2..]);
    assert!(path.is_file());
    let held = fixture.real.directory.path().join(format!("held-{sha}"));
    fs::rename(&path, &held).unwrap();
    (path, held)
}

#[test]
fn original_saved_boundaries_missing_blob_has_actual_child_no_head_file_and_explicit_repair() {
    let fixture = NestedReal::new("sha256");
    let before = fixture.real.snapshot();
    let mut workspace = fixture.workspace();
    fixture.action(
        &mut workspace,
        RowAction::Directory(RawPath(b"nested".to_vec())),
    );
    let blob = crate::local_commit_actor_origin_v1_test_support::text(
        &fixture.real.root,
        &[
            "rev-parse",
            &format!("{}:nested/renamed.txt", fixture.chosen),
        ],
    );
    let (path, held) = held(&fixture, &blob);
    let op = operation(&workspace);
    fixture.real.clear_trace();
    assert!(workspace
        .open_original_with_operation_v1(RowAction::File(fixture.path.clone()), &op)
        .is_err());
    let failed = op.seal().unwrap();
    assert!(failed.children_started() > 0);
    assert_eq!(failed.children_started(), fixture.real.children().len());
    assert_eq!(workspace.revision, fixture.chosen);
    assert!(workspace.preview.is_none());
    assert!(!path.exists());
    fs::rename(held, path).unwrap();
    fixture.action(&mut workspace, RowAction::File(fixture.path.clone()));
    assert!(workspace
        .preview
        .as_ref()
        .unwrap()
        .content
        .starts_with(b"actual saved line 001\n"));
    assert_eq!(workspace.preview.as_ref().unwrap().revision, fixture.chosen);
    assert_eq!(fixture.real.snapshot(), before);
}

#[test]
fn original_saved_boundaries_missing_chosen_commit_history_requires_explicit_same_object_repair() {
    let fixture = NestedReal::new("sha1");
    let before = fixture.real.snapshot();
    let mut workspace = fixture.workspace();
    fixture.file(&mut workspace);
    let (path, held) = held(&fixture, &fixture.chosen);
    let op = operation(&workspace);
    fixture.real.clear_trace();
    assert!(workspace
        .open_original_with_operation_v1(RowAction::FileProvenance(FileAction::History), &op)
        .is_err());
    assert!(op.seal().unwrap().children_started() > 0);
    assert!(!path.exists());
    assert_eq!(workspace.revision, fixture.chosen);
    assert_ne!(
        workspace.selected_commit.as_ref().unwrap().id,
        crate::local_commit_actor_origin_v1_test_support::text(
            &fixture.real.root,
            &["rev-parse", "HEAD"]
        )
    );
    fs::rename(held, path).unwrap();
    fixture.action(
        &mut workspace,
        RowAction::FileProvenance(FileAction::History),
    );
    let history = workspace
        .file_provenance
        .as_ref()
        .unwrap()
        .history
        .as_ref()
        .unwrap();
    assert_eq!(history.revision, fixture.chosen);
    assert!(history
        .entries
        .iter()
        .any(|entry| entry.commit.id == fixture.responsible));
    assert_eq!(fixture.real.snapshot(), before);
}

#[test]
fn original_saved_boundaries_deleted_working_path_keeps_saved_blob_and_original_rename_blame() {
    let fixture = NestedReal::new("sha256");
    let before = fixture.real.snapshot();
    let mut workspace = fixture.workspace();
    fixture.action(
        &mut workspace,
        RowAction::Directory(RawPath(b"nested".to_vec())),
    );
    let path = fixture.real.root.join("nested/renamed.txt");
    let held = fixture.real.directory.path().join("user-dirty-file");
    fs::rename(&path, &held).unwrap();
    fixture.action(&mut workspace, RowAction::File(fixture.path.clone()));
    assert_eq!(workspace.preview.as_ref().unwrap().revision, fixture.chosen);
    assert!(!path.exists());
    assert!(workspace
        .preview
        .as_ref()
        .unwrap()
        .content
        .starts_with(b"actual saved line 001\n"));
    fixture.action(
        &mut workspace,
        RowAction::FileProvenance(FileAction::BlameAt(2)),
    );
    let selection = workspace
        .file_provenance
        .as_ref()
        .unwrap()
        .selection
        .as_ref()
        .unwrap();
    assert_eq!(selection.commit, fixture.responsible);
    assert_eq!(selection.path.as_ref().unwrap().0, b"history.txt");
    assert_eq!(selection.original_line, Some(2));
    assert!(!path.exists());
    assert_eq!(fs::read(&held).unwrap(), b"user dirty renamed bytes\n");
    fs::rename(held, path).unwrap();
    assert_eq!(fixture.real.snapshot(), before);
}

#[test]
fn original_saved_boundaries_same_working_root_replaced_git_directory_stops_before_child() {
    use crate::local_commit_actor_origin_v1_test_support::git;
    let fixture = NestedReal::new("sha1");
    let mut workspace = fixture.workspace();
    fixture.file(&mut workspace);
    let original = fixture.real.root.join(".git");
    let held = fixture.real.directory.path().join("held-git");
    fs::rename(&original, &held).unwrap();
    let replacement = fixture.real.directory.path().join("replacement");
    git(
        fixture.real.directory.path(),
        &[
            "clone",
            "--quiet",
            "--no-hardlinks",
            held.to_str().unwrap(),
            replacement.to_str().unwrap(),
        ],
        None,
    );
    fs::rename(replacement.join(".git"), &original).unwrap();
    let before = fixture.real.snapshot();
    let op = operation(&workspace);
    fixture.real.clear_trace();
    let err = workspace
        .open_original_with_operation_v1(RowAction::FileProvenance(FileAction::History), &op)
        .unwrap_err();
    assert!(err.contains("source/locator changed"));
    assert_eq!(op.accounting().unwrap().children_started(), 0);
    assert!(fixture.real.children().is_empty());
    assert_eq!(fixture.real.snapshot(), before);
}

#[test]
fn original_saved_boundaries_symlink_observation_is_not_fabricated_regular_line_or_original_file() {
    use crate::local_commit_actor_origin_v1_test_support::{git, text};
    let fixture = NestedReal::new("sha1");
    std::os::unix::fs::symlink("nested/renamed.txt", fixture.real.root.join("saved-link")).unwrap();
    git(&fixture.real.root, &["add", "--", "saved-link"], None);
    git(
        &fixture.real.root,
        &[
            "commit",
            "--only",
            "-qm",
            "Typed saved symlink",
            "--",
            "saved-link",
        ],
        None,
    );
    let sha = text(&fixture.real.root, &["rev-parse", "HEAD"]);
    let before = fixture.real.snapshot();
    let mut workspace = Workspace::load(&fixture.real.root, RepositoryMode::Tree, &sha, None)
        .unwrap()
        .unwrap();
    fixture.action(
        &mut workspace,
        RowAction::File(RawPath(b"saved-link".to_vec())),
    );
    assert_eq!(
        workspace.preview.as_ref().unwrap().kind,
        crate::repository_view::BlobPreviewKind::Symlink
    );
    fixture.action(&mut workspace, RowAction::FileProvenance(FileAction::Blame));
    assert!(workspace
        .rows
        .iter()
        .any(|row| row.label.contains("not a regular text file")));
    assert!(workspace
        .file_provenance
        .as_ref()
        .unwrap()
        .selection
        .is_none());
    assert!(workspace.file_provenance.as_ref().unwrap().blame.is_empty());
    assert!(workspace
        .provenance_available(&FileAction::OriginalFile)
        .unwrap_err()
        .contains("exact history entry"));
    for target in [
        FileAction::Graph,
        FileAction::Diff,
        FileAction::OriginalFile,
    ] {
        assert!(workspace.rows.iter().any(|row| {
            row.label.starts_with('[')
                && row.role == SemanticRole::Boundary
                && row.action.as_ref() == Some(&RowAction::FileProvenance(target.clone()))
        }));
        assert!(workspace.provenance_available(&target).is_err());
    }
    assert!(!workspace.rows.iter().any(|row| matches!(
        row.action,
        Some(RowAction::FileProvenance(FileAction::BlameLine(_)))
    )));
    assert_eq!(fixture.real.snapshot(), before);
}
