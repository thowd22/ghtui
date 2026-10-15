use super::*;
use crate::local_commit_actor_origin_v1_test_support::{git, text};
use std::{fs, path::PathBuf};
struct Real {
    directory: tempfile::TempDir,
    root: PathBuf,
    repository: GitRepository,
    origin: Arc<OriginalLocalActorOriginV1>,
    sha: String,
    head: String,
}
impl Real {
    fn new(format: &str) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("checkout");
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
        git(&root, &["config", "user.name", "Local Actor"], None);
        git(
            &root,
            &["config", "user.email", "local@example.invalid"],
            None,
        );
        fs::write(root.join("conflict"), b"base\n").unwrap();
        fs::write(root.join("stable"), b"unchanged\n").unwrap();
        git(&root, &["add", "--", "conflict", "stable"], None);
        git(&root, &["commit", "-qm", "base"], None);
        fs::write(root.join("conflict"), b"stashed\n").unwrap();
        git(&root, &["stash", "push", "-m", "private stash label"], None);
        let sha = text(&root, &["rev-parse", "refs/stash"]);
        fs::write(root.join("conflict"), b"ours\n").unwrap();
        git(&root, &["commit", "-qam", "current"], None);
        let head = text(&root, &["rev-parse", "HEAD"]);
        let repository = GitRepository::discover(&root).unwrap().unwrap();
        let origin = OriginalLocalActorOriginV1::capture(&repository).unwrap();
        Self {
            directory,
            root,
            repository,
            origin,
            sha,
            head,
        }
    }
    fn plan(&self) -> MutationPlan {
        self.repository
            .plan(Operation::StashApply {
                id: self.sha.clone(),
                pop: true,
            })
            .unwrap()
    }
    fn failed(&self) -> Arc<FailedStashIntentV1> {
        self.repository
            .execute_stash_with_inspection_v1(&self.plan(), true, self.origin.clone())
            .unwrap_err()
            .failed_intent()
            .unwrap()
            .clone()
    }
    fn bytes(&self) -> Vec<u8> {
        git(&self.root, &["ls-files", "--stage", "-z"], None)
    }
}
#[test]
fn stash_recovery_actual_conflicted_pop_sha1_sha256_retains_exact_stash_and_reads_three_stages_without_reconciliation(
) {
    for format in ["sha1", "sha256"] {
        let real = Real::new(format);
        let failed = real.failed();
        assert_eq!(failed.sha(), real.sha);
        assert!(failed.pop());
        assert_eq!(text(&real.root, &["rev-parse", "refs/stash"]), real.sha);
        assert_eq!(text(&real.root, &["rev-parse", "HEAD"]), real.head);
        assert!(!real.root.join(".git/MERGE_HEAD").exists());
        let before = real.bytes();
        let file = fs::read(real.root.join("conflict")).unwrap();
        real.repository
            .actual_read_runner_v1()
            .mark_ambiguous(&format!("git:{}", real.root.display()));
        let operation =
            crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1::begin(
                &real.origin,
                Default::default(),
            )
            .unwrap();
        let (status, stages, present) = real.origin.inspect_stash_v1(&failed, &operation).unwrap();
        let accounting = operation.seal().unwrap();
        assert!(present);
        assert_eq!(status.conflicts(), 1);
        assert!(status.operation.is_none());
        assert_eq!(
            stages.iter().map(|s| s.stage).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert!(accounting.children_started() > 0);
        assert_eq!(real.bytes(), before);
        assert_eq!(fs::read(real.root.join("conflict")).unwrap(), file);
        assert_eq!(fs::read(real.root.join("stable")).unwrap(), b"unchanged\n");
        let write = real
            .repository
            .plan(Operation::Stage(vec![RawPath(b"conflict".to_vec())]))
            .unwrap();
        assert!(
            real.repository
                .execute(&write, true)
                .unwrap_err()
                .to_string()
                .contains("unknown"),
            "Read must not reconcile the actual retained ambiguity barrier"
        );
    }
}
#[test]
fn stash_recovery_no_confirmation_or_foreign_source_cannot_capture_failed_witness_or_write() {
    let a = Real::new("sha1");
    let b = Real::new("sha1");
    let before = a.bytes();
    let plan = a.plan();
    assert!(a
        .repository
        .execute_stash_with_inspection_v1(&plan, false, a.origin.clone())
        .unwrap_err()
        .failed_intent()
        .is_none());
    assert!(a
        .repository
        .execute_stash_with_inspection_v1(&plan, true, b.origin.clone())
        .unwrap_err()
        .failed_intent()
        .is_none());
    assert_eq!(a.bytes(), before);
    assert_eq!(text(&a.root, &["rev-parse", "refs/stash"]), a.sha);
}
#[test]
fn stash_recovery_recreated_root_and_cancel_refuse_before_original_observer_children() {
    let real = Real::new("sha1");
    let failed = real.failed();
    let kept = real.directory.path().join("original-kept");
    fs::rename(&real.root, &kept).unwrap();
    git(
        real.directory.path(),
        &[
            "clone",
            "-q",
            "--no-hardlinks",
            kept.to_str().unwrap(),
            real.root.to_str().unwrap(),
        ],
        None,
    );
    let operation =
        crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1::begin(
            &real.origin,
            Default::default(),
        )
        .unwrap();
    assert!(real.origin.inspect_stash_v1(&failed, &operation).is_err());
    assert_eq!(operation.accounting().unwrap().children_started(), 0);
    let cancel = CancellationToken::default();
    cancel.cancel();
    assert!(
        crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1::begin(
            &real.origin,
            cancel
        )
        .is_err(),
        "Cancelled original pool is refused before allocation/child read"
    );
}
#[test]
fn stash_recovery_genuine_stash_absence_is_observation_not_automatic_drop() {
    let real = Real::new("sha1");
    let failed = real.failed();
    git(&real.root, &["stash", "drop", "--", "stash@{0}"], None);
    let before = real.bytes();
    let operation =
        crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1::begin(
            &real.origin,
            Default::default(),
        )
        .unwrap();
    let (_, stages, present) = real.origin.inspect_stash_v1(&failed, &operation).unwrap();
    operation.seal().unwrap();
    assert!(!present);
    assert_eq!(stages.len(), 3);
    assert_eq!(real.bytes(), before);
}
