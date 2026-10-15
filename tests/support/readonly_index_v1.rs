//! Genuine installed-Git stat-cache fixture and independently read-only observers.
use std::{
    ffi::OsString,
    fs,
    io::Write,
    os::unix::{ffi::OsStringExt, fs::PermissionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub fn git(root: &Path, args: &[&str], input: Option<&[u8]>) -> Vec<u8> {
    invoke(root, args, input, true)
}
fn invoke(root: &Path, args: &[&str], input: Option<&[u8]>, readonly: bool) -> Vec<u8> {
    let mut command = Command::new("git");
    command.current_dir(root).arg("--no-optional-locks");
    if readonly {
        command.args(["-c", "diff.autoRefreshIndex=false"]);
    }
    command
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
        "installed Git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}
pub fn prepare(root: &Path, format: &str) {
    git(
        root,
        &[
            "init",
            "-q",
            "--initial-branch=main",
            &format!("--object-format={format}"),
        ],
        None,
    );
    git(root, &["config", "user.name", "Index fixture"], None);
    git(
        root,
        &["config", "user.email", "index@example.invalid"],
        None,
    );
    git(root, &["config", "commit.gpgsign", "false"], None);
    git(root, &["config", "diff.autoRefreshIndex", "true"], None);
    fs::write(root.join("tracked"), b"saved tracked bytes\n").unwrap();
    fs::write(root.join("eligible"), b"saved eligible bytes\n").unwrap();
    git(root, &["add", "--", "tracked", "eligible"], None);
    git(
        root,
        &["commit", "--allow-empty", "-qm", "Original index baseline"],
        None,
    );
    fs::write(root.join("tracked"), b"staged tracked bytes\n").unwrap();
    git(root, &["add", "--", "tracked"], None);
    fs::write(root.join("tracked"), b"working tracked bytes\n").unwrap();
    fs::write(root.join("scratch"), b"untracked exact bytes\n").unwrap();
    make_eligible(root);
}
pub fn make_eligible(root: &Path) {
    // Reset only this fixture's index entry, then apply a genuine cached patch.
    git(root, &["reset", "-q", "HEAD", "--", "eligible"], None);
    fs::write(root.join("eligible"), b"staged eligible bytes\n").unwrap();
    let patch = git(root, &["diff", "--no-ext-diff", "--", "eligible"], None);
    assert!(patch
        .windows(b"+staged eligible bytes".len())
        .any(|s| s == b"+staged eligible bytes"));
    git(
        root,
        &["apply", "--cached", "--whitespace=nowarn", "-"],
        Some(&patch),
    );
    let debug = git(root, &["ls-files", "--debug", "--", "eligible"], None);
    assert!(
        String::from_utf8_lossy(&debug).contains("ctime: 0:0"),
        "actual cached apply must leave zero stat-cache eligibility"
    );
}
pub fn git_path(root: &Path, name: &str) -> PathBuf {
    let bytes = git(root, &["rev-parse", "--git-path", name], None);
    assert_eq!(bytes.last(), Some(&b'\n'));
    let path = PathBuf::from(OsString::from_vec(bytes[..bytes.len() - 1].to_vec()));
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}
#[derive(Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub index: Vec<u8>,
    pub stages: Vec<u8>,
    pub head: Vec<u8>,
    pub head_file: Vec<u8>,
    pub refs: Vec<u8>,
    pub config: Vec<u8>,
    pub status: Vec<u8>,
    pub staged: Vec<u8>,
    pub working: Vec<u8>,
    pub files: Vec<(Vec<u8>, u32)>,
}
pub fn snapshot(root: &Path) -> Snapshot {
    let path = git_path(root, "index");
    let index = fs::read(&path).unwrap();
    let result = Snapshot {
        index,
        stages: git(root, &["ls-files", "--stage", "-z"], None),
        head: git(root, &["rev-parse", "HEAD"], None),
        head_file: fs::read(git_path(root, "HEAD")).unwrap(),
        refs: git(root, &["show-ref", "--head"], None),
        config: fs::read(git_path(root, "config")).unwrap(),
        status: git(root, &["status", "--porcelain=v2", "-z"], None),
        staged: git(
            root,
            &["diff", "--cached", "--binary", "--no-ext-diff"],
            None,
        ),
        working: git(root, &["diff", "--binary", "--no-ext-diff"], None),
        files: ["tracked", "eligible", "scratch"]
            .into_iter()
            .map(|name| {
                (
                    fs::read(root.join(name)).unwrap(),
                    fs::metadata(root.join(name)).unwrap().permissions().mode(),
                )
            })
            .collect(),
    };
    assert_eq!(
        fs::read(path).unwrap(),
        result.index,
        "independent observers themselves must preserve exact raw index"
    );
    result
}
pub fn ordinary_diff(root: &Path) {
    invoke(root, &["diff", "--no-ext-diff"], None, false);
}
