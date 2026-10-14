//! Provider-only fixture helpers, usable without terminal dependencies.
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};
use tempfile::TempDir;
pub struct Fixture {
    pub root: PathBuf,
    _temp: TempDir,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self._temp.disable_cleanup(true);
            eprintln!(
                "Provider fixture failure artifacts retained at {}",
                self.root.display()
            );
        }
    }
}
#[derive(Clone)]
pub struct GhStep {
    pub args: Vec<String>,
    pub stdin: Vec<u8>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub status: i32,
    pub delay_ms: u64,
}
impl GhStep {
    pub fn read(args: &[&str], stdout: &str) -> Self {
        Self {
            args: args.iter().map(|s| (*s).into()).collect(),
            stdin: vec![],
            stdout: stdout.as_bytes().to_vec(),
            stderr: vec![],
            status: 0,
            delay_ms: 0,
        }
    }
}
impl Fixture {
    pub fn new() -> Self {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        let temp = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let root = temp.path().to_path_buf();
        for name in ["bin", "repo", "gh-fixture"] {
            fs::DirBuilder::new()
                .mode(0o700)
                .create(root.join(name))
                .unwrap();
        }
        let fixture = Self { root, _temp: temp };
        fixture.scripted_gh(&[]);
        fixture
    }
    pub fn repo(&self) -> PathBuf {
        self.root.join("repo")
    }
    pub fn scripted_gh(&self, steps: &[GhStep]) {
        static BINARY: OnceLock<PathBuf> = OnceLock::new();
        let binary = BINARY.get_or_init(|| {
            let dir = tempfile::tempdir().unwrap().keep();
            let source = dir.join("fake_gh.rs");
            fs::write(&source, include_str!("../fixtures/fake_gh.rs")).unwrap();
            let binary = dir.join("fake-gh");
            let output = Command::new("rustc")
                .args(["--edition=2021"])
                .arg(source)
                .arg("-o")
                .arg(&binary)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            binary
        });
        let target = self.root.join("bin/gh");
        if !target.exists() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(binary, &target).unwrap();
            #[cfg(not(unix))]
            {
                fs::copy(binary, &target).unwrap();
            }
        }
        for (index, step) in steps.iter().enumerate() {
            let prefix = self.root.join("gh-fixture").join(format!("{index:03}"));
            write(&prefix, "args", step.args.join("\n").as_bytes());
            write(&prefix, "stdin", &step.stdin);
            write(&prefix, "stdout", &step.stdout);
            write(&prefix, "stderr", &step.stderr);
            write(&prefix, "status", step.status.to_string().as_bytes());
            write(&prefix, "delay-ms", step.delay_ms.to_string().as_bytes());
        }
    }
}
fn write(prefix: &Path, suffix: &str, content: &[u8]) {
    fs::write(
        PathBuf::from(format!("{}.{}", prefix.display(), suffix)),
        content,
    )
    .unwrap();
}
