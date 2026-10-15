use super::*;
use std::os::unix::fs::PermissionsExt;
fn special_cases(format: &str) {
    let temp = fixture(format);
    let root = temp.path();
    actual::git(root, &["reset", "--hard", "HEAD"], None);
    let raw = if cfg!(target_os = "macos") {
        vec![b"raw\npath".to_vec(), b"raw\\npath".to_vec()]
    } else {
        vec![b"raw\xfe".to_vec(), b"raw\xff".to_vec()]
    };
    for path in &raw {
        std::fs::write(
            root.join(OsString::from_vec(path.clone())),
            b"raw old bytes\n",
        )
        .unwrap();
    }
    for (name, body) in [
        ("old rename", b"exact rename bytes\n".as_slice()),
        ("empty delete", b""),
        ("binary delete", b"\0old"),
        ("binary modify", b"\0before"),
        ("mode", b"same mode content\n"),
    ] {
        std::fs::write(root.join(name), body).unwrap();
    }
    actual::git(root, &["add", "-A"], None);
    actual::git(root, &["commit", "-qm", "Actual special baseline"], None);
    for path in &raw {
        std::fs::write(
            root.join(OsString::from_vec(path.clone())),
            b"raw new bytes\n",
        )
        .unwrap();
    }
    std::fs::rename(root.join("old rename"), root.join("new rename")).unwrap();
    std::fs::remove_file(root.join("empty delete")).unwrap();
    std::fs::remove_file(root.join("binary delete")).unwrap();
    std::fs::write(root.join("empty add"), b"").unwrap();
    std::fs::write(root.join("binary add"), b"\0added").unwrap();
    std::fs::write(root.join("binary modify"), b"\0after").unwrap();
    std::fs::set_permissions(root.join("mode"), std::fs::Permissions::from_mode(0o755)).unwrap();
    actual::git(root, &["add", "-A"], None);
    let before = actual::snapshot(root);
    let repo = GitRepository::discover(root).unwrap().unwrap();
    let staged = repo.compare(&Comparison::Staged).unwrap();
    let expected = parse_diff_paths(&actual::git(
        root,
        &[
            "diff",
            "--cached",
            "--name-status",
            "-z",
            "--no-ext-diff",
            "--no-textconv",
            "--find-renames",
            "--",
        ],
        None,
    ))
    .unwrap();
    let observed = staged
        .diff
        .files
        .iter()
        .map(|f| (f.old_path.clone(), f.new_path.clone()))
        .collect::<Vec<_>>();
    assert_eq!(observed, expected);
    assert!(
        observed.contains(&(Some(b"empty delete".to_vec()), Some(b"empty add".to_vec()))),
        "installed Git correctly recognizes the exact empty-file rename"
    );
    assert!(observed.contains(&(Some(b"old rename".to_vec()), Some(b"new rename".to_vec()))));
    assert!(observed.contains(&(Some(b"mode".to_vec()), Some(b"mode".to_vec()))));
    for path in &raw {
        assert!(observed.contains(&(Some(path.clone()), Some(path.clone()))));
    }
    assert!(staged.diff.files.iter().filter(|f| f.binary).count() >= 3);
    assert_eq!(actual::snapshot(root), before);
    actual::make_eligible(root);
    let before = actual::snapshot(root);
    let working = repo.compare(&Comparison::Working).unwrap();
    assert!(working.diff.files.is_empty(),"unchanged worktree with eligible cached-apply entry produces genuine empty comparison, not fake0 rows");
    assert_eq!(actual::snapshot(root), before);
    empty_sides(format);
}
fn empty_sides(format: &str) {
    let temp = fixture(format);
    let root = temp.path();
    actual::git(root, &["reset", "--hard", "HEAD"], None);
    std::fs::write(root.join("empty add"), b"").unwrap();
    actual::git(root, &["add", "--", "empty add"], None);
    let before = actual::snapshot(root);
    let repo = GitRepository::discover(root).unwrap().unwrap();
    let added = repo.compare(&Comparison::Staged).unwrap();
    assert!(added
        .diff
        .files
        .iter()
        .any(|f| f.old_path.is_none() && f.new_path.as_deref() == Some(b"empty add")));
    assert_eq!(actual::snapshot(root), before);
    actual::git(root, &["commit", "-qm", "Genuine empty file"], None);
    std::fs::remove_file(root.join("empty add")).unwrap();
    actual::git(root, &["add", "-u"], None);
    let before = actual::snapshot(root);
    let deleted = repo.compare(&Comparison::Staged).unwrap();
    assert!(deleted
        .diff
        .files
        .iter()
        .any(|f| f.old_path.as_deref() == Some(b"empty add") && f.new_path.is_none()));
    assert_eq!(actual::snapshot(root), before);
}
#[test]
fn readonly_index_sha1_real_empty_binary_mode_rename_raw_and_nochange() {
    special_cases("sha1");
}
#[test]
fn readonly_index_sha256_real_empty_binary_mode_rename_raw_and_nochange() {
    special_cases("sha256");
}
#[test]
fn readonly_index_real_pure_edited_and_raw_quoted_copy_uses_same_canonical_parser() {
    let temp = fixture("sha1");
    let root = temp.path();
    actual::git(root, &["reset", "--hard", "HEAD"], None);
    let body = (0..32)
        .map(|n| format!("copy original {n}\n"))
        .collect::<String>();
    std::fs::write(root.join("copy source"), body.as_bytes()).unwrap();
    actual::git(root, &["add", "-A"], None);
    actual::git(root, &["commit", "-qm", "Actual copy baseline"], None);
    std::fs::write(root.join("copy target"), body.as_bytes()).unwrap();
    actual::git(root, &["add", "-A"], None);
    let patch = actual::git(
        root,
        &[
            "diff",
            "--cached",
            "--find-copies-harder",
            "--no-ext-diff",
            "--no-textconv",
            "--",
        ],
        None,
    );
    let stats = actual::git(
        root,
        &[
            "diff",
            "--cached",
            "--find-copies-harder",
            "--numstat",
            "-z",
            "--no-ext-diff",
            "--no-textconv",
            "--",
        ],
        None,
    );
    let diff = Diff::parse(&patch).unwrap();
    assert!(diff.files[0]
        .metadata
        .iter()
        .any(|m| m.starts_with(b"copy from ")));
    assert_eq!(
        super::super::readonly_diff_paths_v1::paths(&diff, &stats, MAX_OUTPUT).unwrap(),
        vec![(Some(b"copy source".to_vec()), Some(b"copy target".to_vec()))]
    );
    std::fs::write(
        root.join("copy target"),
        body.replacen("copy original 0", "copy edited 0", 1),
    )
    .unwrap();
    actual::git(root, &["add", "-A"], None);
    let patch = actual::git(
        root,
        &[
            "diff",
            "--cached",
            "--find-copies-harder",
            "--no-ext-diff",
            "--no-textconv",
            "--",
        ],
        None,
    );
    let stats = actual::git(
        root,
        &[
            "diff",
            "--cached",
            "--find-copies-harder",
            "--numstat",
            "-z",
            "--no-ext-diff",
            "--no-textconv",
            "--",
        ],
        None,
    );
    let diff = Diff::parse(&patch).unwrap();
    assert_eq!(
        super::super::readonly_diff_paths_v1::paths(&diff, &stats, MAX_OUTPUT).unwrap(),
        vec![(Some(b"copy source".to_vec()), Some(b"copy target".to_vec()))]
    );
    actual::git(root, &["reset", "--hard", "HEAD"], None);
    let old = if cfg!(target_os = "macos") {
        b"copy\told\n".to_vec()
    } else {
        b"copy\told\xfe".to_vec()
    };
    let new = if cfg!(target_os = "macos") {
        b"copy\nnew\t".to_vec()
    } else {
        b"copy\nnew\xff".to_vec()
    };
    std::fs::rename(
        root.join("copy source"),
        root.join(OsString::from_vec(old.clone())),
    )
    .unwrap();
    actual::git(root, &["add", "-A"], None);
    actual::git(
        root,
        &["commit", "-qm", "Actual raw quoted copy source"],
        None,
    );
    std::fs::copy(
        root.join(OsString::from_vec(old.clone())),
        root.join(OsString::from_vec(new.clone())),
    )
    .unwrap();
    actual::git(root, &["add", "-A"], None);
    let patch = actual::git(
        root,
        &[
            "diff",
            "--cached",
            "--find-copies-harder",
            "--no-ext-diff",
            "--no-textconv",
            "--",
        ],
        None,
    );
    let stats = actual::git(
        root,
        &[
            "diff",
            "--cached",
            "--find-copies-harder",
            "--numstat",
            "-z",
            "--no-ext-diff",
            "--no-textconv",
            "--",
        ],
        None,
    );
    let diff = Diff::parse(&patch).unwrap();
    assert!(diff.files[0]
        .metadata
        .iter()
        .any(|m| m.starts_with(b"copy from \"")));
    assert_eq!(
        super::super::readonly_diff_paths_v1::paths(&diff, &stats, MAX_OUTPUT).unwrap(),
        vec![(Some(old), Some(new))]
    );
}
#[test]
fn readonly_index_original_common_read_policy_control_changes_raw_with_actual_retained_runner() {
    let temp = fixture("sha256");
    let root = temp.path();
    let repository = GitRepository::discover(root).unwrap().unwrap();
    let before = actual::snapshot(root);
    // Exact former common READ argv/env, no trace inside the worktree.
    // Deliberate negative control, not production policy or write permission.
    let mut spec = CommandSpec::new("git")
        .args([
            "--no-pager",
            "--literal-pathspecs",
            "-c",
            "color.ui=false",
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--find-renames",
            "--",
        ])
        .cwd(root)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_PAGER", "cat")
        .env("GIT_EDITOR", "false")
        .env("GIT_SEQUENCE_EDITOR", "false")
        .env("GIT_ALLOW_PROTOCOL", "")
        .timeout(COMMAND_TIMEOUT);
    spec.output_limit = MAX_OUTPUT;
    let output = repository
        .actual_read_runner_v1()
        .run(spec, &CancellationToken::default())
        .unwrap();
    assert!(output
        .stdout
        .windows(b"working tracked bytes".len())
        .any(|s| s == b"working tracked bytes"));
    let mut after = actual::snapshot(root);
    assert_ne!(
        before.index, after.index,
        "actual original runner/read policy reproduces raw index refresh"
    );
    after.index.clone_from(&before.index);
    assert_eq!(before, after, "only raw stat-cache metadata changed");
}
#[test]
fn readonly_index_readonly_git_directory_and_foreign_index_lock_remain_untouched() {
    let temp = fixture("sha1");
    let root = temp.path();
    let git_dir = root.join(".git");
    let lock = git_dir.join("index.lock");
    assert!(!lock.exists());
    std::fs::write(&lock, b"foreign index lock must remain exact\n").unwrap();
    let before = actual::snapshot(root);
    let mode = std::fs::metadata(&git_dir).unwrap().permissions().mode();
    std::fs::set_permissions(&git_dir, std::fs::Permissions::from_mode(0o500)).unwrap();
    let read = (|| {
        let repo = GitRepository::discover(root)?.unwrap();
        repo.status()?;
        repo.fingerprint()?;
        repo.compare(&Comparison::Working)?;
        repo.compare(&Comparison::Staged)?;
        Ok::<_, GitError>(())
    })();
    std::fs::set_permissions(&git_dir, std::fs::Permissions::from_mode(mode)).unwrap();
    read.unwrap();
    assert_eq!(
        std::fs::read(lock).unwrap(),
        b"foreign index lock must remain exact\n"
    );
    assert_eq!(actual::snapshot(root), before);
}
#[test]
fn readonly_index_real_exit_two_is_failure_not_a_path_payload_or_retry() {
    let temp = fixture("sha1");
    let root = temp.path();
    std::fs::write(root.join("tracked"), b"trailing whitespace \n").unwrap();
    let before = actual::snapshot(root);
    let repo = GitRepository::discover(root).unwrap().unwrap();
    let result = run_git_bounded_policy_v1(
        root,
        ["diff", "--check", "--no-ext-diff", "--no-textconv"],
        None,
        false,
        &CancellationToken::default(),
        GitReadPolicyV1 {
            timeout: Duration::from_secs(5),
            output_limit: 4096,
            window: None,
            runner: Some(repo.actual_read_runner_v1()),
        },
    )
    .unwrap();
    assert!(!result.success);
    assert_eq!(result.exit_code, Some(2));
    assert!(repo
        .bytes_with_capture_limit_v2(
            ["diff", "--check", "--no-ext-diff", "--no-textconv"],
            None,
            4096
        )
        .is_err());
    assert_eq!(actual::snapshot(root), before);
}
#[test]
fn readonly_index_stale_original_mutable_source_refuses_without_index_changes() {
    let temp = fixture("sha256");
    let root = temp.path();
    let loaded = Workspace::load(root, RepositoryMode::Working, "HEAD", None)
        .unwrap()
        .unwrap();
    let intent = MutableDiffIntentV2::capture(&loaded).unwrap();
    let op = LocalGitNavigationReadV1::begin(intent.origin(), Default::default()).unwrap();
    let source = MutableDiffSourceV2::read(intent, &op).unwrap();
    std::fs::write(root.join("tracked"), b"independent changed working bytes\n").unwrap();
    let before = actual::snapshot(root);
    assert!(source.validate_current(&op).is_err());
    assert_eq!(actual::snapshot(root), before);
}

#[test]
fn readonly_index_real_typechange_two_patch_one_stat_record_fails_closed_without_mutation() {
    let temp = fixture("sha256");
    let root = temp.path();
    actual::git(root, &["reset", "--hard", "HEAD"], None);
    std::fs::remove_file(root.join("tracked")).unwrap();
    std::os::unix::fs::symlink("eligible", root.join("tracked")).unwrap();
    actual::git(root, &["add", "--", "tracked"], None);
    let patch = actual::git(
        root,
        &["diff", "--cached", "--no-ext-diff", "--no-textconv", "--"],
        None,
    );
    let stats = actual::git(
        root,
        &[
            "diff",
            "--cached",
            "--numstat",
            "-z",
            "--no-ext-diff",
            "--no-textconv",
            "--",
        ],
        None,
    );
    let diff = Diff::parse(&patch).unwrap();
    assert_eq!(
        diff.files.len(),
        2,
        "installed Git typechange is separate actual delete/add patch records"
    );
    assert_eq!(
        stats
            .split(|b| *b == 0)
            .filter(|r| r.ends_with(b"\ttracked"))
            .count(),
        1,
        "installed Git combines this typechange into one numstat record for two canonical patch files"
    );
    let before = actual::snapshot(root);
    let repo = GitRepository::discover(root).unwrap().unwrap();
    let error = repo.compare(&Comparison::Staged).unwrap_err();
    assert!(error
        .to_string()
        .contains("canonical changed-line count disagreement"));
    assert_eq!(actual::snapshot(root), before);
}
