//! Actual installed-Git success followed by exactly one failed refresh child.
use std::{
    fs,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};
fn quote(path: &Path) -> Vec<u8> {
    let mut out = vec![b'\''];
    for byte in path.as_os_str().as_bytes() {
        if *byte == b'\'' {
            out.extend_from_slice(b"'\\''");
        } else {
            out.push(*byte);
        }
    }
    out.push(b'\'');
    out
}
pub fn install(root: &Path) {
    install_mode(root, false)
}
pub fn install_held_reload(root: &Path) {
    install_mode(root, true)
}
fn install_mode(root: &Path, hold: bool) {
    let git = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|p| p.join("git"))
        .find(|p| p.is_file())
        .map(|p| fs::canonicalize(p).unwrap())
        .unwrap();
    let records = root.join("stash-phase-argv-v2");
    fs::create_dir(&records).unwrap();
    let mut script = b"#!/bin/sh\nset -C\nrecords=".to_vec();
    script.extend(quote(&records));
    script.extend(b"\nsucceeded=");
    script.extend(quote(&root.join("stash-command-succeeded")));
    script.extend(b"\nfailed=");
    script.extend(quote(&root.join("stash-refresh-failed")));
    script.extend(b"\nreal_git=");
    script.extend(quote(&git));
    script.extend(b"\nprintf '%s\\000' \"$@\" > \"$records/$$.argv\" || exit 91\n");
    if hold {
        script.extend(b"held=");
        script.extend(quote(&root.join("stash-refresh-held")));
        script.extend(b"\nrelease=");
        script.extend(quote(&root.join("stash-refresh-release")));
        script.extend(b"\nif [ -f \"$succeeded\" ] && [ ! -f \"$held\" ]; then \"$real_git\" \"$@\" > \"$records/$$.out\"; result=$?; printf '%s\\000' \"$@\" > \"$held\"; count=0; while [ ! -f \"$release\" ]; do count=$((count + 1)); if [ \"$count\" -ge 500 ]; then exit 92; fi; sleep 0.01; done; cat \"$records/$$.out\"; exit \"$result\"; fi\n");
    } else {
        script.extend(b"if [ -f \"$succeeded\" ] && [ ! -f \"$failed\" ]; then printf '%s\\000' \"$@\" > \"$failed\"; printf '%s\\n' 'fatal: fixture first refresh child refused after actual stash success' >&2; exit 73; fi\n");
    }
    script.extend(b"case \" $* \" in *' stash pop '*|*' stash apply '*) \"$real_git\" \"$@\"; result=$?; if [ \"$result\" -eq 0 ]; then printf '%s\\n' 'actual installed Git exit0' > \"$succeeded\"; fi; exit \"$result\";; esac\nexec \"$real_git\" \"$@\"\n");
    crate::fixture_shell_publication_v1::install(&root.join("bin/git"), &script).unwrap();
}
pub fn records(root: &Path) -> Vec<Vec<Vec<u8>>> {
    fs::read_dir(root.join("stash-phase-argv-v2"))
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .path()
                .extension()
                .is_some_and(|v| v == "argv")
        })
        .map(|e| {
            let bytes = fs::read(e.unwrap().path()).unwrap();
            assert_eq!(bytes.last(), Some(&0));
            bytes[..bytes.len() - 1]
                .split(|b| *b == 0)
                .map(<[u8]>::to_vec)
                .collect()
        })
        .collect()
}
pub fn replace_held_locator(root: &Path, repository: &Path) -> PathBuf {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !root.join("stash-refresh-held").exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "Actual refresh child did not reach hold"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let held = fs::read(root.join("stash-refresh-held")).unwrap();
    assert!(held.windows(9).any(|w| w == b"rev-parse"));
    assert_eq!(
        fs::read(root.join("stash-command-succeeded")).unwrap(),
        b"actual installed Git exit0\n"
    );
    let original = root.join("original-after-success");
    fs::rename(repository, &original).unwrap();
    fs::rename(root.join("replacement"), repository).unwrap();
    fs::write(
        root.join("stash-refresh-release"),
        b"release original child\n",
    )
    .unwrap();
    original
}
pub fn replacement(root: &Path) {
    let path = root.join("replacement");
    fs::create_dir(&path).unwrap();
    git(&path, &["init", "-q", "--initial-branch=main"]);
    git(&path, &["config", "user.name", "Different Local Actor"]);
    git(
        &path,
        &["config", "user.email", "replacement@example.invalid"],
    );
    git(&path, &["config", "commit.gpgsign", "false"]);
    fs::write(path.join("file"), b"replacement private body\n").unwrap();
    git(&path, &["add", "file"]);
    git(
        &path,
        &["commit", "-qm", "replacement must not be accepted"],
    );
}
pub fn assert_success_and_failed_refresh(root: &Path) {
    assert_eq!(
        fs::read(root.join("stash-command-succeeded")).unwrap(),
        b"actual installed Git exit0\n"
    );
    let failed = fs::read(root.join("stash-refresh-failed")).unwrap();
    assert_eq!(failed.last(), Some(&0));
    assert!(
        failed.windows(9).any(|w| w == b"rev-parse"),
        "The deliberately refused child must actually be the post-success discovery refresh"
    );
    let records = records(root);
    assert_eq!(
        records
            .iter()
            .filter(|r| r
                .windows(2)
                .any(|w| w == [b"stash".to_vec(), b"pop".to_vec()]))
            .count(),
        1
    );
    assert!(!records.iter().any(|r| r
        .windows(2)
        .any(|w| w == [b"stash".to_vec(), b"drop".to_vec()])));
}
pub fn child_path(root: &Path) -> std::ffi::OsString {
    std::env::join_paths(
        std::iter::once(root.join("bin"))
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap()
}
pub fn git(root: &Path, args: &[&str]) -> Vec<u8> {
    let output = std::process::Command::new("git")
        .current_dir(root)
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
    output.stdout
}
pub fn setup(root: &Path) -> (PathBuf, String) {
    let repo = root.join("repo");
    fs::create_dir(&repo).unwrap();
    fs::create_dir(root.join("bin")).unwrap();
    git(&repo, &["init", "-q", "--initial-branch=main"]);
    git(&repo, &["config", "user.name", "Actual Local Actor"]);
    git(&repo, &["config", "user.email", "local@example.invalid"]);
    git(&repo, &["config", "commit.gpgsign", "false"]);
    fs::write(repo.join("file"), b"base\n").unwrap();
    git(&repo, &["add", "file"]);
    git(&repo, &["commit", "-qm", "base"]);
    fs::write(repo.join("file"), b"successful stash content\n").unwrap();
    git(&repo, &["stash", "push", "-m", "phase witness"]);
    let sha = String::from_utf8(git(&repo, &["rev-parse", "refs/stash"]))
        .unwrap()
        .trim()
        .to_owned();
    (repo, sha)
}
