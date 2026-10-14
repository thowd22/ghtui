#![allow(dead_code)]
use ghtui::git::GitRepository;
use std::ffi::OsStr;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use tempfile::TempDir;

pub struct Fixture {
    pub directory: TempDir,
    pub root: PathBuf,
    pub repository: GitRepository,
}

impl Fixture {
    pub fn unborn() -> Self {
        let directory = tempfile::Builder::new()
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let root = directory.path().join("repo");
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&root)
            .unwrap();
        git(&root, ["init", "-b", "main"]);
        git(&root, ["config", "user.name", "Fixture Author"]);
        git(&root, ["config", "user.email", "fixture@example.test"]);
        git(&root, ["config", "commit.gpgsign", "false"]);
        git(&root, ["config", "core.hooksPath", ".git/hooks"]);
        let repository = GitRepository::discover(&root).unwrap().unwrap();
        Self {
            directory,
            root,
            repository,
        }
    }

    pub fn linear() -> Self {
        let fixture = Self::unborn();
        fixture.write("readme.txt", b"first\nsecond\n");
        fixture.commit("Root commit");
        fixture.write("readme.txt", b"first\nchanged\n");
        fixture.commit("Update readme");
        fixture
    }

    pub fn divergent() -> Self {
        let fixture = Self::linear();
        git(&fixture.root, ["switch", "-c", "topic"]);
        fixture.write("topic.txt", b"topic only\n");
        fixture.commit("Topic change");
        git(&fixture.root, ["switch", "main"]);
        fixture.write("main.txt", b"main only\n");
        fixture.commit("Main change");
        fixture
    }

    pub fn write(&self, path: impl AsRef<Path>, content: &[u8]) {
        let path = self.root.join(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    pub fn commit(&self, message: &str) -> String {
        git(&self.root, ["add", "--all"]);
        git(&self.root, ["commit", "-m", message]);
        self.head()
    }

    pub fn commit_index(&self, message: &str) -> String {
        git(&self.root, ["commit", "-m", message]);
        self.head()
    }

    pub fn index_blob(&self, path: &[u8], content: &[u8]) -> String {
        index_blob(&self.root, path, content)
    }

    pub fn head(&self) -> String {
        String::from_utf8(git(&self.root, ["rev-parse", "HEAD"]).stdout)
            .unwrap()
            .trim()
            .to_owned()
    }
}

/// Git objects retain arbitrary path bytes even when macOS APFS cannot create such a file.
/// This is a real index/tree fixture, not a mocked parser or a skipped filesystem assertion.
pub fn index_blob(directory: &Path, path: &[u8], content: &[u8]) -> String {
    let blob =
        String::from_utf8(git_input(directory, ["hash-object", "-w", "--stdin"], content).stdout)
            .unwrap()
            .trim()
            .to_owned();
    let mut record = format!("100644 {blob}\t").into_bytes();
    record.extend_from_slice(path);
    record.push(0);
    git_input(directory, ["update-index", "-z", "--index-info"], &record);
    blob
}

/// Physical filename coverage matches the supported filesystem, without ignoring the test.
pub fn unusual_worktree_path() -> Vec<u8> {
    if cfg!(target_os = "macos") {
        "tab\tline\n日本語".as_bytes().to_vec()
    } else {
        b"tab\tline\n\xff".to_vec()
    }
}

pub fn git_input<I, S>(directory: &Path, args: I, input: &[u8]) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut child = Command::new("git")
        .args(args)
        .current_dir(directory)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_DATE", "2026-01-01T00:00:00+00:00")
        .env("GIT_COMMITTER_DATE", "2026-01-01T00:00:00+00:00")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

pub fn try_git<I, S>(directory: &Path, args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Command::new("git")
        .args(args)
        .current_dir(directory)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_DATE", "2026-01-01T00:00:00+00:00")
        .env("GIT_COMMITTER_DATE", "2026-01-01T00:00:00+00:00")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .unwrap()
}

pub fn git<I, S>(directory: &Path, args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = try_git(directory, args);
    assert!(
        output.status.success(),
        "Git fixture command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}
