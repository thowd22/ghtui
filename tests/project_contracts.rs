//! Normal owned project metadata: never infer a checkout from the build directory.
use ghtui::git::GitRepository;
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn git(directory: &Path, args: &[&str]) -> Output {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "owned fixture git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn source_bytes(directory: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn collect(root: &Path, relative: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(root.join(relative)).unwrap() {
            let entry = entry.unwrap();
            let name = relative.join(entry.file_name());
            let kind = entry.file_type().unwrap();
            if kind.is_dir() {
                collect(root, &name, out);
            } else {
                assert!(kind.is_file(), "actual source must be a regular file");
                out.insert(name.clone(), fs::read(root.join(name)).unwrap());
            }
        }
    }
    let mut bytes = BTreeMap::from([(
        PathBuf::from("Cargo.toml"),
        fs::read(directory.join("Cargo.toml")).unwrap(),
    )]);
    collect(directory, Path::new("src"), &mut bytes);
    bytes
}

#[derive(Debug, PartialEq, Eq)]
struct ReadOnlyWitness {
    source: BTreeMap<PathBuf, Vec<u8>>,
    index: Option<Vec<u8>>,
    head: Vec<u8>,
    config: Vec<u8>,
    refs: Vec<u8>,
    status: Vec<u8>,
    remotes: Vec<u8>,
    untracked: Option<Vec<u8>>,
}

impl ReadOnlyWitness {
    fn capture(directory: &Path) -> Self {
        Self {
            source: source_bytes(directory),
            index: fs::read(directory.join(".git/index")).ok(),
            head: fs::read(directory.join(".git/HEAD")).unwrap(),
            config: fs::read(directory.join(".git/config")).unwrap(),
            refs: git(
                directory,
                &[
                    "for-each-ref",
                    "--format=%(refname)%00%(objectname)%00%(symref)",
                ],
            )
            .stdout,
            status: git(
                directory,
                &["status", "--porcelain=v2", "-z", "--untracked-files=all"],
            )
            .stdout,
            remotes: git(directory, &["remote", "-v"]).stdout,
            untracked: fs::read(directory.join("untracked-canary.txt")).ok(),
        }
    }
}

#[test]
fn project_git_init_standard_metadata_root_subdirectory_and_no_remote() {
    let temp = tempfile::Builder::new()
        .prefix("ghtui-owned-project-contract-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let directory = temp.path();
    let actual_source = source_bytes(Path::new(env!("CARGO_MANIFEST_DIR")));
    for (relative, bytes) in &actual_source {
        let file = directory.join(relative);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, bytes).unwrap();
    }
    assert_eq!(source_bytes(directory), actual_source);
    git(directory, &["init", "--initial-branch=main"]);
    git(directory, &["config", "user.name", "Owned Project Fixture"]);
    git(
        directory,
        &["config", "user.email", "project@example.invalid"],
    );
    assert!(
        directory.join(".git").is_dir(),
        "normal owned .git directory"
    );
    assert!(git(directory, &["remote", "-v"]).stdout.is_empty());
    let expected = directory.canonicalize().unwrap();
    for state in [
        "unborn",
        "unborn staged",
        "committed",
        "dirty index and worktree",
    ] {
        match state {
            "unborn staged" => {
                git(directory, &["add", "--", "Cargo.toml"]);
            }
            "committed" => {
                git(directory, &["add", "--", "Cargo.toml", "src"]);
                git(
                    directory,
                    &[
                        "-c",
                        "commit.gpgSign=false",
                        "commit",
                        "-m",
                        "Actual project source",
                    ],
                );
            }
            "dirty index and worktree" => {
                let file = directory.join("src/lib.rs");
                let mut bytes = fs::read(&file).unwrap();
                bytes.extend_from_slice(b"\n// staged project fixture change\n");
                fs::write(&file, &bytes).unwrap();
                git(directory, &["add", "--", "src/lib.rs"]);
                bytes.extend_from_slice(b"// separate unstaged project fixture change\n");
                fs::write(file, bytes).unwrap();
                fs::write(
                    directory.join("untracked-canary.txt"),
                    b"never add or alter this file\n",
                )
                .unwrap();
            }
            "unborn" => {}
            _ => unreachable!(),
        }
        let before = ReadOnlyWitness::capture(directory);
        assert!(
            before.remotes.is_empty(),
            "initial fixture must have no remote"
        );
        if state == "unborn" {
            assert!(before.index.is_none());
        }
        if state == "unborn staged" {
            assert!(before.index.is_some());
        }
        if state == "committed" {
            assert!(before.status.is_empty());
        }
        if state == "dirty index and worktree" {
            assert!(before.status.windows(5).any(|bytes| bytes == b"1 MM "));
            assert!(before
                .status
                .split(|byte| *byte == 0)
                .any(|record| record == b"? untracked-canary.txt"));
        }
        for start in [directory.to_path_buf(), directory.join("src")] {
            let repository = GitRepository::discover(&start)
                .unwrap()
                .expect("Owned project must use its normal local .git metadata");
            assert_eq!(repository.root.canonicalize().unwrap(), expected);
            let status = repository.status().unwrap();
            assert_eq!(status.branch.as_deref(), Some("main"));
            assert_eq!(status.head.is_none(), state.starts_with("unborn"));
            assert_eq!(
                ReadOnlyWitness::capture(directory),
                before,
                "read-only {state} from {start:?}"
            );
        }
    }
}
