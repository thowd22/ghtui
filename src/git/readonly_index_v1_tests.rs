use super::*;
use crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1;
use crate::workspace::{RepositoryMode, Workspace};
#[path = "../../tests/support/readonly_index_v1.rs"]
mod actual;
#[path = "readonly_index_real_cases_v1_tests.rs"]
mod real_cases;

fn fixture(format: &str) -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    actual::prepare(temp.path(), format);
    temp
}
#[test]
fn readonly_index_sha1_ordinary_git_negative_changes_raw_not_semantic() {
    negative("sha1");
}
#[test]
fn readonly_index_sha256_ordinary_git_negative_changes_raw_not_semantic() {
    negative("sha256");
}
fn negative(format: &str) {
    let temp = fixture(format);
    let before = actual::snapshot(temp.path());
    actual::ordinary_diff(temp.path());
    let mut after = actual::snapshot(temp.path());
    assert_ne!(
        before.index, after.index,
        "negative control must actually refresh metadata on installed Git"
    );
    after.index.clone_from(&before.index);
    assert_eq!(
        before, after,
        "ordinary Git changed stat cache only, not semantic stages or checkout"
    );
}
#[test]
fn readonly_index_sha1_status_fingerprint_working_staged_native_preserve_raw() {
    reads("sha1");
}
#[test]
fn readonly_index_sha256_status_fingerprint_working_staged_native_preserve_raw() {
    reads("sha256");
}
fn reads(format: &str) {
    let temp = fixture(format);
    let root = fs_canonical(temp.path());
    let before = actual::snapshot(&root);
    let trace = actual::git_path(&root, "readonly-trace.jsonl");
    let _guard = register_actor_origin_test_trace(&root, &trace);
    let repo = GitRepository::discover(&root).unwrap().unwrap();
    assert!(!repo.status().unwrap().entries.is_empty());
    repo.fingerprint().unwrap();
    for mode in [RepositoryMode::Working, RepositoryMode::Staged] {
        let loaded = Workspace::load(&root, mode, "HEAD", None);
        assert_eq!(
            actual::snapshot(&root),
            before,
            "actual workspace loading itself preserves raw index before accepting any result"
        );
        let loaded = loaded.unwrap().unwrap();
        let intent = MutableDiffIntentV2::capture(&loaded).unwrap();
        let operation =
            LocalGitNavigationReadV1::begin(intent.origin(), Default::default()).unwrap();
        let before_children = traced_children(&trace);
        let source = MutableDiffSourceV2::read(intent, &operation).unwrap();
        assert!(!source.members().is_empty());
        source.validate_current(&operation).unwrap();
        let accounting = operation.seal().unwrap();
        assert!(accounting.children_started() > 0 && accounting.stdout_bytes() > 0);
        assert_eq!(
            accounting.children_started(),
            traced_children(&trace) - before_children,
            "all actual children use the one original measured operation"
        );
        eprintln!(
            "actual readonly_index {format} {mode:?} original measured children={}",
            accounting.children_started()
        );
        assert_eq!(actual::snapshot(&root), before);
    }
    for line in std::fs::read_to_string(trace).unwrap().lines() {
        let event: serde_json::Value = serde_json::from_str(line).unwrap();
        if event["event"] != "start" {
            continue;
        }
        let argv = event["argv"].as_array().unwrap();
        assert_eq!(
            argv[1..7]
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>(),
            [
                "--no-pager",
                "--literal-pathspecs",
                "-c",
                "color.ui=false",
                "-c",
                "diff.autoRefreshIndex=false"
            ]
        );
    }
}
fn fs_canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap()
}
fn traced_children(path: &Path) -> usize {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|s| serde_json::from_str::<serde_json::Value>(s).unwrap()["event"] == "start")
        .count()
}

#[test]
fn readonly_index_linked_raw_worktree_preserves_independent_common_and_index_state() {
    let temp = fixture("sha256");
    let component = if cfg!(target_os = "macos") {
        b"linked-\n".to_vec()
    } else {
        b"linked-\xff".to_vec()
    };
    let linked = temp.path().join(OsString::from_vec(component));
    let output = std::process::Command::new("git")
        .current_dir(temp.path())
        .args(["worktree", "add", "-qb", "linked-native"])
        .arg(&linked)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    actual::prepare(&linked, "sha256");
    let before = actual::snapshot(&linked);
    let main_index = std::fs::read(temp.path().join(".git/index")).unwrap();
    let repo = GitRepository::discover(&linked).unwrap().unwrap();
    assert_ne!(repo.git_dir, repo.common_dir);
    for comparison in [Comparison::Working, Comparison::Staged] {
        let result = repo.compare(&comparison).unwrap();
        assert!(!result.diff.files.is_empty());
    }
    repo.status().unwrap();
    repo.fingerprint().unwrap();
    assert_eq!(actual::snapshot(&linked), before);
    assert_eq!(
        std::fs::read(temp.path().join(".git/index")).unwrap(),
        main_index
    );
}

#[test]
fn readonly_index_read_policy_does_not_rewrite_explicit_write_argv_or_grant_window_writes() {
    let temp = fixture("sha1");
    let root = fs_canonical(temp.path());
    let trace = actual::git_path(&root, "policy-trace.jsonl");
    let _guard = register_actor_origin_test_trace(&root, &trace);
    let runner = CommandRunner::default();
    let cancel = CancellationToken::default();
    let output = run_git_bounded_policy_v1(
        &root,
        ["config", "fixture.explicitWrite", "true"],
        None,
        true,
        &cancel,
        GitReadPolicyV1 {
            timeout: Duration::from_secs(5),
            output_limit: 1024,
            window: None,
            runner: Some(&runner),
        },
    )
    .unwrap();
    assert!(output.success);
    assert_eq!(
        actual::git(&root, &["config", "--get", "fixture.explicitWrite"], None),
        b"true\n"
    );
    let events = std::fs::read_to_string(&trace).unwrap();
    let start: serde_json::Value = events
        .lines()
        .map(|s| serde_json::from_str::<serde_json::Value>(s).unwrap())
        .find(|v| v["event"] == "start")
        .unwrap();
    assert_eq!(
        start["argv"],
        serde_json::json!([
            "git",
            "--no-pager",
            "--literal-pathspecs",
            "-c",
            "color.ui=false",
            "config",
            "fixture.explicitWrite",
            "true"
        ])
    );
    let loaded = Workspace::load(&root, RepositoryMode::Working, "HEAD", None)
        .unwrap()
        .unwrap();
    let intent = MutableDiffIntentV2::capture(&loaded).unwrap();
    let op = LocalGitNavigationReadV1::begin(intent.origin(), cancel.clone()).unwrap();
    let window = op.window();
    assert!(run_git_bounded_policy_v1(
        &root,
        ["config", "fixture.forbidden", "true"],
        None,
        true,
        &cancel,
        GitReadPolicyV1 {
            timeout: Duration::from_secs(5),
            output_limit: 1024,
            window: Some(&window),
            runner: Some(&runner)
        }
    )
    .is_err());
    assert_eq!(op.accounting().unwrap().children_started(), 0);
    assert!(
        !String::from_utf8_lossy(&actual::git(&root, &["config", "--local", "--list"], None))
            .contains("fixture.forbidden")
    );
}

#[test]
fn readonly_index_canceled_sealed_and_expired_original_operation_start_zero_children() {
    let temp = fixture("sha256");
    let root = fs_canonical(temp.path());
    let before = actual::snapshot(&root);
    let loaded = Workspace::load(&root, RepositoryMode::Working, "HEAD", None)
        .unwrap()
        .unwrap();
    let intent = MutableDiffIntentV2::capture(&loaded).unwrap();
    let canceled = CancellationToken::default();
    let op = LocalGitNavigationReadV1::begin(intent.origin(), canceled.clone()).unwrap();
    canceled.cancel();
    assert!(MutableDiffSourceV2::read(intent.clone(), &op).is_err());
    assert_eq!(op.accounting().unwrap().children_started(), 0);
    let sealed = LocalGitNavigationReadV1::begin(intent.origin(), Default::default()).unwrap();
    sealed.seal().unwrap();
    assert!(MutableDiffSourceV2::read(intent.clone(), &sealed).is_err());
    assert_eq!(sealed.accounting().unwrap().children_started(), 0);
    let expired = std::time::Instant::now()
        .checked_sub(Duration::from_millis(1))
        .unwrap();
    assert!(LocalGitNavigationReadV1::begin_under_deadline(
        intent.origin(),
        Default::default(),
        expired
    )
    .is_err());
    assert_eq!(actual::snapshot(&root), before);
}

#[test]
fn readonly_index_capture_failure_and_missing_checkout_do_not_refresh_or_create_index() {
    let temp = fixture("sha1");
    let root = fs_canonical(temp.path());
    let before = actual::snapshot(&root);
    let error = run_git_bounded_policy_v1(
        &root,
        ["diff", "--no-ext-diff"],
        None,
        false,
        &CancellationToken::default(),
        GitReadPolicyV1 {
            timeout: Duration::from_secs(5),
            output_limit: 1,
            window: None,
            runner: None,
        },
    )
    .err()
    .expect("actual capture failure");
    assert!(error.to_string().contains("output") || error.to_string().contains("capture"));
    assert_eq!(actual::snapshot(&root), before);
    let absent = root.join("missing-checkout");
    assert!(GitRepository::discover(&absent).is_err());
    assert!(!absent.exists());
}
