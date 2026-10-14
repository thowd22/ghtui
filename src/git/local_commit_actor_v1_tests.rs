//! Original-token contracts use actual Git objects, plus focused exact parser
//! and the SAME installed-Git command adapter deadline/combined-capture gates.
use super::*;
#[path = "local_commit_actor_source_v1_tests.rs"]
mod source_continuity;
use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
    time::Instant,
};

fn git(root: &Path, args: &[&str], input: Option<&[u8]>) -> Vec<u8> {
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
        "Actual git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}
fn text(root: &Path, args: &[&str]) -> String {
    String::from_utf8(git(root, args, None))
        .unwrap()
        .trim()
        .into()
}
fn fixture(
    format: &str,
    author: &str,
    committer: &str,
    body: &str,
) -> (tempfile::TempDir, GitRepository, String, Vec<u8>) {
    let dir = tempfile::tempdir().unwrap();
    git(
        dir.path(),
        &[
            "init",
            "-q",
            "--initial-branch=main",
            &format!("--object-format={format}"),
        ],
        None,
    );
    git(
        dir.path(),
        &["config", "user.name", "Fixture Original"],
        None,
    );
    git(
        dir.path(),
        &["config", "user.email", "fixture@example.invalid"],
        None,
    );
    fs::write(dir.path().join("tracked"), b"saved bytes\n").unwrap();
    git(dir.path(), &["add", "tracked"], None);
    git(dir.path(), &["commit", "-qm", "root"], None);
    let tree = text(dir.path(), &["rev-parse", "HEAD^{tree}"]);
    let parent = text(dir.path(), &["rev-parse", "HEAD"]);
    let raw=format!("tree {tree}\nparent {parent}\nauthor {author}\ncommitter {committer}\ngpgsig opaque source bytes\n ignored continuation, not verified signature\n\n{body}\n").into_bytes();
    let sha = String::from_utf8(git(
        dir.path(),
        &["hash-object", "-t", "commit", "-w", "--stdin"],
        Some(&raw),
    ))
    .unwrap()
    .trim()
    .to_owned();
    git(dir.path(), &["update-ref", "refs/heads/main", &sha], None);
    fs::write(dir.path().join("tracked"), b"dirty source bytes\n").unwrap();
    fs::write(dir.path().join("scratch"), b"untracked source bytes\n").unwrap();
    let repo = GitRepository::discover(dir.path()).unwrap().unwrap();
    (dir, repo, sha, raw)
}
#[derive(Debug, PartialEq, Eq)]
struct PreservedGitStateV1 {
    references: Vec<u8>,
    index: Vec<u8>,
    tracked: Vec<u8>,
    untracked: Vec<u8>,
    staged_diff: Vec<u8>,
}
fn preserved(root: &Path) -> PreservedGitStateV1 {
    PreservedGitStateV1 {
        references: git(root, &["show-ref", "--head"], None),
        index: fs::read(root.join(".git/index")).unwrap(),
        tracked: fs::read(root.join("tracked")).unwrap(),
        untracked: fs::read(root.join("scratch")).unwrap(),
        staged_diff: git(root, &["diff", "--cached", "--binary"], None),
    }
}

#[test]
fn git_actor_association_local_real_sha1_sha256_original_fields_and_git_effects() {
    for format in ["sha1", "sha256"] {
        let (dir, repo, sha, raw) = fixture(
            format,
            "Raw Writer <write@example.invalid> 1700000000 +0530",
            "Raw Recorder <record@example.invalid> 1700000100 -0700",
            "Actual saved evidence",
        );
        let before = preserved(dir.path());
        let token =
            OriginalLocalCommitActors::read(&repo, &sha, &Default::default(), &Default::default())
                .unwrap();
        assert_eq!(token.sha(), sha);
        assert_eq!(token.sha().len(), if format == "sha256" { 64 } else { 40 });
        assert_eq!(token.author().name_bytes(), b"Raw Writer");
        assert_eq!(token.author().email_bytes(), b"write@example.invalid");
        assert_eq!(token.author().seconds(), 1700000000);
        assert_eq!(token.author().timezone(), b"+0530");
        assert_eq!(token.committer().name_bytes(), b"Raw Recorder");
        assert_eq!(token.committer().seconds(), 1700000100);
        assert_eq!(token.committer().timezone(), b"-0700");
        assert!(
            token.stdout_bytes() > raw.len()
                && token.stdout_bytes() <= token.reserved_capture_bytes()
        );
        assert_eq!(preserved(dir.path()), before);
        assert_eq!(git(dir.path(), &["cat-file", "commit", &sha], None), raw);
    }
}
#[test]
fn git_actor_association_local_empty_and_escaped_bytes_are_raw_not_lossy_display() {
    let (_dir, repo, sha, _) = fixture(
        "sha1",
        " <> 1700000000 +0000",
        "Raw\u{202e}Recorder <record@example.invalid> 1700000100 +0000",
        "Empty actor",
    );
    let token =
        OriginalLocalCommitActors::read(&repo, &sha, &Default::default(), &Default::default())
            .unwrap();
    assert!(token.author().name_bytes().is_empty() && token.author().email_bytes().is_empty());
    assert_eq!(
        token.committer().name_bytes(),
        "Raw\u{202e}Recorder".as_bytes()
    );
    assert_ne!(
        token.committer().name_bytes(),
        display_bytes(token.committer().name_bytes()).as_bytes()
    );
}
#[test]
fn git_actor_association_local_original_objects_ignore_replacement_and_graft_actor_rewrites() {
    let (dir, repo, sha, raw) = fixture(
        "sha1",
        "Original Writer <original@example.invalid> 1700000000 +0000",
        "Original Recorder <record@example.invalid> 1700000100 +0000",
        "Original",
    );
    let replaced = raw
        .iter()
        .map(|byte| if *byte == b'O' { b'X' } else { *byte })
        .collect::<Vec<_>>();
    let replacement = String::from_utf8(git(
        dir.path(),
        &["hash-object", "-t", "commit", "-w", "--stdin"],
        Some(&replaced),
    ))
    .unwrap()
    .trim()
    .to_owned();
    git(dir.path(), &["replace", &sha, &replacement], None);
    fs::write(dir.path().join(".git/info/grafts"), format!("{sha}\n")).unwrap();
    assert!(
        String::from_utf8(git(dir.path(), &["cat-file", "commit", &sha], None))
            .unwrap()
            .contains("Xriginal Writer")
    );
    let before = preserved(dir.path());
    let token =
        OriginalLocalCommitActors::read(&repo, &sha, &Default::default(), &Default::default())
            .unwrap();
    assert_eq!(token.author().name_bytes(), b"Original Writer");
    assert_eq!(preserved(dir.path()), before);
}
#[test]
fn git_actor_association_local_duplicate_missing_non_utf8_and_oversize_headers_refuse() {
    for raw in [
        b"author A <a> 1 +0000\nauthor B <b> 1 +0000\ncommitter C <c> 1 +0000\n\nx".as_slice(),
        b"author A <a> 1 +0000\n\nx",
        b"author A\xff <a> 1 +0000\ncommitter C <c> 1 +0000\n\nx",
        b"author A <a> 1 +2500\ncommitter C <c> 1 +0000\n\nx",
    ] {
        assert!(parse_headers(raw).is_err());
    }
    let raw = format!(
        "author {} <a> 1 +0000\ncommitter C <c> 1 +0000\n\nx",
        "a".repeat(4097)
    );
    assert!(parse_headers(raw.as_bytes()).is_err());
    let (dir, repo, sha, _) = fixture(
        "sha1",
        "A <a> 1 +0000",
        "C <c> 1 +0000",
        &"x".repeat(MAX_OBJECT_BYTES),
    );
    let before = preserved(dir.path());
    let failure =
        OriginalLocalCommitActors::read(&repo, &sha, &Default::default(), &Default::default())
            .unwrap_err();
    assert!(failure.0.contains("before object allocation"));
    assert_eq!(preserved(dir.path()), before);
}
#[test]
fn git_actor_association_local_original_cancel_and_aggregate_deadline_cannot_rebase() {
    let (dir, repo, sha, _) = fixture("sha1", "A <a> 1 +0000", "C <c> 1 +0000", "Original");
    let before = preserved(dir.path());
    let cancel = CancellationToken::default();
    cancel.cancel();
    assert!(OriginalLocalCommitActors::read(&repo, &sha, &Default::default(), &cancel).is_err());
    let original = CancellationToken::default();
    let token =
        OriginalLocalCommitActors::read(&repo, &sha, &Default::default(), &original).unwrap();
    original.cancel();
    assert!(token.remaining(&Default::default()).is_err());
    let mut token =
        OriginalLocalCommitActors::read(&repo, &sha, &Default::default(), &Default::default())
            .unwrap();
    token.deadline = Instant::now();
    assert!(token
        .remaining(&Default::default())
        .unwrap_err()
        .0
        .contains("deadline"));
    assert_eq!(preserved(dir.path()), before);
}
#[test]
fn git_actor_association_local_actual_shared_runner_deadline_and_combined_capture_caps() {
    let (dir, _, _, _) = fixture("sha1", "A <a> 1 +0000", "C <c> 1 +0000", "Original");
    let before = preserved(dir.path());
    let started = Instant::now();
    let timeout = run_git_bounded_with_kind(
        dir.path(),
        ["-c", "alias.actor-wait=!/bin/sleep 10", "actor-wait"],
        None,
        false,
        &Default::default(),
        Duration::from_millis(50),
        128,
    )
    .err()
    .expect("Actual bounded installed-Git timeout must fail");
    assert!(
        timeout.0.contains("Command exceeded its deadline"),
        "Actual bounded runner failure: {}",
        timeout.0
    );
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(run_git_bounded_with_kind(
        dir.path(),
        [
            "-c",
            "alias.actor-output=!printf '1234567890'; printf 'abcdefghij' >&2",
            "actor-output"
        ],
        None,
        false,
        &Default::default(),
        Duration::from_secs(1),
        12
    )
    .is_err());
    assert!(run_git_bounded_with_kind(
        dir.path(),
        ["rev-parse", "HEAD"],
        None,
        false,
        &Default::default(),
        Duration::ZERO,
        128
    )
    .is_err());
    assert_eq!(preserved(dir.path()), before);
}
