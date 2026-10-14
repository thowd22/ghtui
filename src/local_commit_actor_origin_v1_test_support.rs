//! Only actual installed-Git fixtures; no constructed origin or actor token.
use crate::git::GitRepository;
use serde_json::Value;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub(crate) fn git(root: &Path, args: &[&str], input: Option<&[u8]>) -> Vec<u8> {
    let mut command = Command::new("git");
    command
        .current_dir(root)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if input.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command.spawn().unwrap();
    if let Some(input) = input {
        child.stdin.take().unwrap().write_all(input).unwrap();
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "Real Git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}
pub(crate) fn text(root: &Path, args: &[&str]) -> String {
    String::from_utf8(git(root, args, None))
        .unwrap()
        .trim()
        .to_owned()
}
pub(crate) struct Real {
    _trace_guard: crate::git::ActorOriginTraceGuard,
    pub directory: tempfile::TempDir,
    pub root: PathBuf,
    pub repository: GitRepository,
    pub original: String,
    pub head: String,
    pub raw: Vec<u8>,
    pub trace: PathBuf,
}
impl Real {
    pub fn new(format: &str) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("repo");
        fs::create_dir(&root).unwrap();
        git(
            &root,
            &[
                "init",
                "-q",
                "--initial-branch=main",
                &format!("--object-format={format}"),
            ],
            None,
        );
        git(
            &root,
            &["config", "user.name", "Current Local Recorder"],
            None,
        );
        git(
            &root,
            &["config", "user.email", "current@example.invalid"],
            None,
        );
        git(&root, &["config", "commit.gpgsign", "false"], None);
        fs::write(root.join("tracked"), b"saved original bytes\n").unwrap();
        git(&root, &["add", "--", "tracked"], None);
        git(&root, &["commit", "-qm", "Real root"], None);
        let root_commit = text(&root, &["rev-parse", "HEAD"]);
        let tree = text(&root, &["rev-parse", "HEAD^{tree}"]);
        let raw = format!("tree {tree}\nparent {root_commit}\nauthor Original Writer <writer@example.invalid> 1700000000 +0530\ncommitter Original Recorder <record@example.invalid> 1700000100 -0700\n\nOriginal saved actors\n").into_bytes();
        let original = String::from_utf8(git(
            &root,
            &["hash-object", "-t", "commit", "-w", "--stdin"],
            Some(&raw),
        ))
        .unwrap()
        .trim()
        .to_owned();
        let next = format!("tree {tree}\nparent {original}\nauthor Later Writer <later@example.invalid> 1700000200 +0000\ncommitter Later Recorder <later-record@example.invalid> 1700000300 +0000\n\nLater checkout actors\n");
        let head = String::from_utf8(git(
            &root,
            &["hash-object", "-t", "commit", "-w", "--stdin"],
            Some(next.as_bytes()),
        ))
        .unwrap()
        .trim()
        .to_owned();
        git(&root, &["update-ref", "refs/heads/main", &head], None);
        git(&root, &["tag", "original-source", &original], None);
        fs::write(root.join("tracked"), b"staged bytes\n").unwrap();
        git(&root, &["add", "--", "tracked"], None);
        fs::write(root.join("tracked"), b"dirty bytes\n").unwrap();
        fs::write(root.join("scratch"), b"untracked bytes\n").unwrap();
        let trace = directory.path().join("actual-git-trace.jsonl");
        let repository = GitRepository::discover(&root).unwrap().unwrap();
        // Git's actual root is canonical even when platform TMPDIR is a symlink.
        let root = repository.root.clone();
        let trace_guard = crate::git::register_actor_origin_test_trace(&root, &trace);
        fs::write(&trace, b"").unwrap();
        Self {
            directory,
            root,
            repository,
            original,
            head,
            raw,
            trace,
            _trace_guard: trace_guard,
        }
    }
    pub fn clear_trace(&self) {
        fs::write(&self.trace, b"").unwrap();
    }
    pub fn children(&self) -> Vec<Value> {
        fs::read_to_string(&self.trace)
            .expect("Actual initialized Git trace remains available")
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .filter(|event| event["event"] == "start")
            .collect()
    }
    pub fn snapshot(&self) -> Vec<Vec<u8>> {
        vec![
            git(&self.root, &["show-ref", "--head"], None),
            fs::read(self.root.join(".git/index")).unwrap(),
            git(
                &self.root,
                &["diff", "--cached", "--binary", "--no-ext-diff"],
                None,
            ),
            git(&self.root, &["diff", "--binary", "--no-ext-diff"], None),
            fs::read(self.root.join("tracked")).unwrap(),
            fs::read(self.root.join("scratch")).unwrap(),
            fs::read(self.root.join(".git/config")).unwrap(),
            git(&self.root, &["cat-file", "commit", &self.original], None),
        ]
    }
}
