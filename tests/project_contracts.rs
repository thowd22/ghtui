//! Normal project metadata and read-only discovery, not a relocated Git workaround.
use ghtui::git::GitRepository;
use std::{path::Path, process::Command};

#[test]
fn project_git_init_standard_metadata_root_subdirectory_and_no_remote() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"));
    let expected = directory.canonicalize().unwrap();
    for start in [directory.to_path_buf(), directory.join("src")] {
        let repository = GitRepository::discover(&start)
            .unwrap()
            .expect("Project must use normal local .git metadata");
        assert_eq!(repository.root.canonicalize().unwrap(), expected);
        repository.status().unwrap();
    }
    let metadata = directory.join(".git");
    assert!(
        metadata.is_dir() || metadata.is_file(),
        "Ordinary Git checkout/worktree metadata is required"
    );
    // Locally initialized project has no remote. CI's checkout remote is not
    // mistaken for a remote created by these read-only discovery operations.
    let before = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(["remote", "-v"])
        .output()
        .unwrap();
    assert!(before.status.success());
    GitRepository::discover(directory)
        .unwrap()
        .unwrap()
        .status()
        .unwrap();
    let after = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(["remote", "-v"])
        .output()
        .unwrap();
    assert!(after.status.success());
    assert_eq!(before.stdout, after.stdout);
}
