use super::*;
use crate::local_commit_actor_origin_v1_test_support::{git, text, Real};
use std::{
    fs,
    os::unix::ffi::OsStringExt,
    time::{Duration, Instant},
};

fn workspace(real: &Real, mode: RepositoryMode) -> Workspace {
    Workspace::load(&real.root, mode, "HEAD", None)
        .unwrap()
        .unwrap()
}
fn operation(intent: &MutableDiffIntentV2) -> LocalGitNavigationReadV1 {
    LocalGitNavigationReadV1::begin(intent.origin(), Default::default()).unwrap()
}
fn source(real: &Real, mode: RepositoryMode) -> (Workspace, Arc<MutableDiffSourceV2>, usize) {
    let loaded = workspace(real, mode);
    let intent = MutableDiffIntentV2::capture(&loaded).unwrap();
    assert!(intent.owns(&loaded));
    real.clear_trace();
    let op = operation(&intent);
    let source = MutableDiffSourceV2::read(intent, &op).unwrap();
    let count = real.children().len();
    assert_eq!(op.accounting().unwrap().children_started(), count);
    assert!(op.accounting().unwrap().stdout_bytes() > 0);
    op.seal().unwrap();
    for child in real.children() {
        let argv = child["argv"].as_array().unwrap();
        assert!(!argv.iter().any(|arg| matches!(
            arg.as_str(),
            Some("fetch" | "rev-parse" | "--show-toplevel" | "--git-dir" | "--no-replace-objects")
        )));
    }
    (loaded, source, count)
}

#[test]
fn native_mutable_diff_source_sha1_working_and_staged_preserve_actual_states() {
    actual_states("sha1");
}
#[test]
fn native_mutable_diff_source_sha256_working_and_staged_preserve_actual_states() {
    actual_states("sha256");
}
fn actual_states(format: &str) {
    let real = Real::new(format);
    let before = real.snapshot();
    for mode in [RepositoryMode::Working, RepositoryMode::Staged] {
        let (loaded, source, count) = source(&real, mode);
        eprintln!("actual {format} {mode:?} native children={count}");
        assert!(source.complete());
        assert_eq!(source.members().len(), source.result().diff.files.len());
        assert_eq!(source.observed_members(), 1);
        assert_eq!(source.excluded(), 0);
        assert_eq!(source.members()[0].index(), 0);
        assert_eq!(
            source.members()[0].path(DiffSide::Old).unwrap().0,
            b"tracked"
        );
        assert_eq!(
            source.members()[0].path(DiffSide::New).unwrap().0,
            b"tracked"
        );
        assert!(source.members()[0].path(DiffSide::Both).is_none());
        assert_eq!(
            source.result().observed,
            loaded.comparison.as_ref().unwrap().observed
        );
        let patch = String::from_utf8_lossy(&source.result().patch);
        if mode == RepositoryMode::Working {
            assert!(patch.contains("-staged bytes") && patch.contains("+dirty bytes"));
            assert_eq!(source.intent().kind(), MutableDiffKindV2::Working);
            assert!(source.result().base.is_none() && source.result().head.is_none());
        } else {
            assert!(patch.contains("-saved original bytes") && patch.contains("+staged bytes"));
            assert_eq!(source.intent().kind(), MutableDiffKindV2::Staged);
            assert_eq!(source.result().base.as_deref(), Some(real.head.as_str()));
            assert!(source.result().head.is_none());
        }
        assert_eq!(real.snapshot(), before);
    }
}

#[test]
fn native_mutable_diff_source_sha1_uncommitted_genuine_compare_not_saved_identity() {
    uncommitted("sha1");
}
#[test]
fn native_mutable_diff_source_sha256_uncommitted_genuine_compare_not_saved_identity() {
    uncommitted("sha256");
}
fn uncommitted(format: &str) {
    let real = Real::new(format);
    let loaded = workspace(&real, RepositoryMode::Uncommitted);
    assert_eq!(loaded.mode, RepositoryMode::Uncommitted);
    assert_eq!(
        loaded.comparison.as_ref().unwrap().comparison,
        Comparison::Uncommitted
    );
    let intent = MutableDiffIntentV2::capture(&loaded).unwrap();
    let before = real.snapshot();
    real.clear_trace();
    let op = operation(&intent);
    let source = MutableDiffSourceV2::read(intent, &op).unwrap();
    assert_eq!(source.intent().kind(), MutableDiffKindV2::Uncommitted);
    assert_eq!(source.result().base.as_deref(), Some(real.head.as_str()));
    assert!(source.result().head.is_none());
    let patch = String::from_utf8_lossy(&source.result().patch);
    assert!(patch.contains("-saved original bytes") && patch.contains("+dirty bytes"));
    assert!(!patch.contains("+staged bytes"));
    assert_eq!(
        op.accounting().unwrap().children_started(),
        real.children().len()
    );
    eprintln!(
        "actual {format} Uncommitted native children={}",
        real.children().len()
    );
    assert_eq!(real.snapshot(), before);
}

#[test]
fn native_mutable_diff_source_raw_nul_members_rename_case_newline_and_byte_collisions() {
    let real = Real::new("sha256");
    let paths: [&[u8]; 7] = [
        b"raw\xfe",
        b"raw\xff",
        b"Case",
        b"case",
        b"line\nbreak",
        b"line\\nbreak",
        "raw�".as_bytes(),
    ];
    for (index, path) in paths.iter().enumerate() {
        fs::write(
            real.root.join(OsString::from_vec(path.to_vec())),
            format!("original unique {index}\n"),
        )
        .unwrap();
    }
    fs::write(real.root.join("rename old"), b"stable exact rename\n").unwrap();
    git(&real.root, &["add", "-A"], None);
    git(&real.root, &["commit", "-qm", "Raw baseline"], None);
    for (index, path) in paths.iter().enumerate() {
        fs::write(
            real.root.join(OsString::from_vec(path.to_vec())),
            format!("new unique {index}\n"),
        )
        .unwrap();
    }
    fs::rename(real.root.join("rename old"), real.root.join("rename new")).unwrap();
    git(&real.root, &["add", "-A"], None);
    let before = real.snapshot();
    let (_, source, _) = source(&real, RepositoryMode::Staged);
    assert_eq!(source.members().len(), 8);
    for path in paths {
        let member = source
            .members()
            .iter()
            .find(|member| {
                member
                    .path(DiffSide::New)
                    .is_some_and(|value| value.0 == path)
            })
            .unwrap();
        assert_eq!(member.path(DiffSide::Old).unwrap().0, path);
        let file = &source.result().diff.files[member.index()];
        assert_eq!(file.new_path.as_deref(), Some(path));
    }
    let renamed = source
        .members()
        .iter()
        .find(|member| {
            member
                .path(DiffSide::New)
                .is_some_and(|value| value.0 == b"rename new")
        })
        .unwrap();
    assert_eq!(renamed.path(DiffSide::Old).unwrap().0, b"rename old");
    let labels = source
        .members()
        .iter()
        .map(|member| member.label())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        labels.len(),
        8,
        "escaped visible labels cannot alias canonical raw paths"
    );
    assert_eq!(real.snapshot(), before);
}

#[test]
fn native_mutable_diff_source_intent_capture_is_pure_exact_and_rejects_wrong_scopes() {
    let real = Real::new("sha1");
    let loaded = workspace(&real, RepositoryMode::Working);
    real.clear_trace();
    let intent = MutableDiffIntentV2::capture(&loaded).unwrap();
    assert!(intent.owns(&loaded));
    assert!(real.children().is_empty());
    let separately_loaded = workspace(&real, RepositoryMode::Working);
    assert!(
        !intent.owns(&separately_loaded),
        "same path and bytes do not replace exact origin Arc"
    );
    let mut wrong = loaded.clone();
    wrong.revision = "some-other-loaded-revision".into();
    assert!(!intent.owns(&wrong));
    wrong = loaded.clone();
    wrong.mode = RepositoryMode::Staged;
    assert!(MutableDiffIntentV2::capture(&wrong).is_err());
    wrong = loaded.clone();
    wrong.actor_profile = Some(crate::identity::GitRole::Author);
    assert!(MutableDiffIntentV2::capture(&wrong).is_err());
    wrong = loaded.clone();
    wrong.directory = real.directory.path().to_owned();
    assert!(MutableDiffIntentV2::capture(&wrong).is_err());
    wrong = loaded.clone();
    wrong.revision = "x".repeat(4097);
    assert!(MutableDiffIntentV2::capture(&wrong).is_err());
    let saved = workspace(&real, RepositoryMode::CommitChanges);
    real.clear_trace();
    assert!(MutableDiffIntentV2::capture(&saved).is_err());
    assert!(real.children().is_empty());
}

#[test]
fn native_mutable_diff_source_canceled_sealed_and_expired_original_operations_start_no_child() {
    let real = Real::new("sha1");
    let intent = MutableDiffIntentV2::capture(&workspace(&real, RepositoryMode::Working)).unwrap();
    real.clear_trace();
    let token = CancellationToken::default();
    let canceled = LocalGitNavigationReadV1::begin(intent.origin(), token.clone()).unwrap();
    token.cancel();
    assert!(MutableDiffSourceV2::read(intent.clone(), &canceled).is_err());
    let sealed = operation(&intent);
    sealed.seal().unwrap();
    assert!(MutableDiffSourceV2::read(intent.clone(), &sealed).is_err());
    let expired = LocalGitNavigationReadV1::begin_under_deadline(
        intent.origin(),
        Default::default(),
        Instant::now() + Duration::from_millis(20),
    )
    .unwrap();
    while Instant::now() < expired.deadline() {
        std::thread::yield_now();
    }
    assert!(MutableDiffSourceV2::read(intent, &expired).is_err());
    assert!(real.children().is_empty());
    for op in [canceled, sealed, expired] {
        assert_eq!(op.accounting().unwrap().children_started(), 0);
    }
}

#[test]
fn native_mutable_diff_source_replaced_original_locator_refuses_before_git() {
    let real = Real::new("sha1");
    let intent = MutableDiffIntentV2::capture(&workspace(&real, RepositoryMode::Working)).unwrap();
    let op = operation(&intent);
    fs::rename(&real.root, real.directory.path().join("original-moved")).unwrap();
    fs::create_dir(&real.root).unwrap();
    real.clear_trace();
    assert!(MutableDiffSourceV2::read(intent, &op).is_err());
    assert!(real.children().is_empty());
    assert_eq!(op.accounting().unwrap().children_started(), 0);
}

#[test]
fn native_mutable_diff_source_stale_worktree_rejected_against_loaded_observation() {
    stale("worktree");
}
#[test]
fn native_mutable_diff_source_stale_index_rejected_against_loaded_observation() {
    stale("index");
}
#[test]
fn native_mutable_diff_source_stale_ref_rejected_against_loaded_observation() {
    stale("ref");
}
#[test]
fn native_mutable_diff_source_stale_remote_config_rejected_against_loaded_observation() {
    stale("config");
}
#[test]
fn native_mutable_diff_source_stale_untracked_bytes_rejected_against_loaded_observation() {
    stale("untracked");
}
fn stale(kind: &str) {
    let real = Real::new("sha1");
    let intent = MutableDiffIntentV2::capture(&workspace(&real, RepositoryMode::Working)).unwrap();
    match kind {
        "worktree" => fs::write(real.root.join("tracked"), b"new worktree\n").unwrap(),
        "index" => {
            git(&real.root, &["add", "--", "tracked"], None);
        }
        "ref" => {
            git(&real.root, &["tag", "new-observed-ref"], None);
        }
        "config" => {
            git(
                &real.root,
                &["config", "remote.other.url", "file:///no-transport"],
                None,
            );
        }
        "untracked" => fs::write(real.root.join("scratch"), b"different bytes\n").unwrap(),
        _ => unreachable!(),
    }
    let before = real.snapshot();
    real.clear_trace();
    let op = operation(&intent);
    let error = MutableDiffSourceV2::read(intent, &op).unwrap_err();
    assert!(error.contains("stale"), "{kind}: {error}");
    assert_eq!(
        op.accounting().unwrap().children_started(),
        real.children().len()
    );
    assert_eq!(real.snapshot(), before);
}

#[test]
fn native_mutable_diff_source_validate_current_performs_fresh_measured_read_not_hint() {
    let real = Real::new("sha1");
    let (_, source, _) = source(&real, RepositoryMode::Staged);
    real.clear_trace();
    let op = operation(source.intent());
    source.validate_current(&op).unwrap();
    assert_eq!(real.children().len(), 5);
    assert_eq!(op.accounting().unwrap().children_started(), 5);
    fs::write(real.root.join("scratch"), b"new untracked bytes\n").unwrap();
    assert!(source.validate_current(&op).unwrap_err().contains("stale"));
    assert_eq!(real.children().len(), 10);
    assert_eq!(op.accounting().unwrap().children_started(), 10);
    op.seal().unwrap();
    assert!(source.validate_current(&op).is_err());
    assert_eq!(real.children().len(), 10);
}

#[test]
fn native_mutable_diff_source_native_eight_mib_fs_limit_preserves_ordinary_thirty_two() {
    let real = Real::new("sha1");
    fs::write(real.root.join("scratch"), vec![b'z'; RAW_CAP + 1]).unwrap();
    let ordinary = real.repository.fingerprint().unwrap();
    let loaded = workspace(&real, RepositoryMode::Working);
    assert_eq!(loaded.comparison.as_ref().unwrap().observed, Some(ordinary));
    let intent = MutableDiffIntentV2::capture(&loaded).unwrap();
    let op = operation(&intent);
    let error = MutableDiffSourceV2::read(intent, &op).unwrap_err();
    assert!(error.contains("Untracked file exceeds"), "{error}");
    assert_eq!(
        fs::metadata(real.root.join("scratch")).unwrap().len(),
        (RAW_CAP + 1) as u64
    );
}

#[test]
fn native_mutable_diff_source_native_eight_mib_git_limit_preserves_ordinary_thirty_two() {
    let real = Real::new("sha1");
    let loaded = workspace(&real, RepositoryMode::Working);
    let intent = MutableDiffIntentV2::capture(&loaded).unwrap();
    fs::write(real.root.join("tracked"), vec![b'q'; RAW_CAP + 1024]).unwrap();
    let ordinary = real.repository.compare(&Comparison::Working).unwrap();
    assert!(ordinary.patch.len() > RAW_CAP && ordinary.patch.len() < MAX_OUTPUT);
    real.clear_trace();
    let op = operation(&intent);
    let error = MutableDiffSourceV2::read(intent, &op).unwrap_err();
    assert!(
        error.contains("output") || error.contains("capture"),
        "{error}"
    );
    assert_eq!(
        op.accounting().unwrap().children_started(),
        real.children().len()
    );
    assert!(op.accounting().unwrap().failed_reserved_bytes() >= RAW_CAP);
}

#[test]
fn native_mutable_diff_source_unborn_index_is_not_a_saved_root_authority() {
    let real = Real::new("sha256");
    git(&real.root, &["checkout", "--orphan", "unborn-native"], None);
    let loaded = workspace(&real, RepositoryMode::Staged);
    assert!(loaded.status.head.is_none());
    let intent = MutableDiffIntentV2::capture(&loaded).unwrap();
    let op = operation(&intent);
    let source = MutableDiffSourceV2::read(intent, &op).unwrap();
    assert_eq!(source.intent().kind(), MutableDiffKindV2::Staged);
    assert!(source.result().base.is_none() && source.result().head.is_none());
    assert!(source
        .members()
        .iter()
        .all(|member| member.path(DiffSide::Old).is_none()));
    assert_eq!(
        text(&real.root, &["symbolic-ref", "HEAD"]),
        "refs/heads/unborn-native"
    );
}

#[test]
fn native_mutable_diff_source_unborn_uncommitted_uses_actual_git_empty_tree_without_saved_endpoint()
{
    for format in ["sha1", "sha256"] {
        let real = Real::new(format);
        git(&real.root, &["checkout", "--orphan", "unborn-all"], None);
        let loaded = workspace(&real, RepositoryMode::Uncommitted);
        assert_eq!(loaded.mode, RepositoryMode::Uncommitted);
        let intent = MutableDiffIntentV2::capture(&loaded).unwrap();
        real.clear_trace();
        let op = operation(&intent);
        let source = MutableDiffSourceV2::read(intent, &op).unwrap();
        assert!(source.result().base.is_none() && source.result().head.is_none());
        assert!(source
            .members()
            .iter()
            .all(|member| member.path(DiffSide::Old).is_none()));
        assert!(String::from_utf8_lossy(&source.result().patch).contains("+dirty bytes"));
        let children = real.children();
        assert_eq!(
            children
                .iter()
                .filter(|event| event["argv"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|arg| arg.as_str() == Some("hash-object")))
                .count(),
            1
        );
        assert_eq!(op.accounting().unwrap().children_started(), children.len());
        eprintln!(
            "actual {format} unbornUncommitted native children={}",
            children.len()
        );
    }
}

#[test]
fn native_mutable_diff_source_real_large_catalogue_result_and_members_admit_exact_same_prefix() {
    let real = Real::new("sha1");
    let nested = real.root.join("a".repeat(240)).join("b".repeat(240));
    fs::create_dir_all(&nested).unwrap();
    for index in 0..2100 {
        fs::write(
            nested.join(format!("file-{index:04}-{}", "c".repeat(200))),
            b"new tracked content\n",
        )
        .unwrap();
    }
    git(&real.root, &["add", "-A"], None);
    let loaded = workspace(&real, RepositoryMode::Staged);
    let original = loaded.comparison.as_ref().unwrap();
    assert_eq!(original.diff.files.len(), 2102);
    assert!(original.patch.len() < RAW_CAP);
    let before = real.snapshot();
    let intent = MutableDiffIntentV2::capture(&loaded).unwrap();
    let op = operation(&intent);
    let source = MutableDiffSourceV2::read(intent, &op).unwrap();
    assert!(!source.complete());
    assert!(source.excluded() > 0);
    assert_eq!(source.observed_members(), 2102);
    assert_eq!(source.members().len(), source.result().diff.files.len());
    assert_eq!(source.members().len() + source.excluded(), 2102);
    assert_eq!(
        source.result().diff.files,
        original.diff.files[..source.members().len()]
    );
    for (index, member) in source.members().iter().enumerate() {
        assert_eq!(member.index(), index);
        assert_eq!(
            member.path(DiffSide::Old).map(|value| value.0.as_slice()),
            original.diff.files[index].old_path.as_deref()
        );
        assert_eq!(
            member.path(DiffSide::New).map(|value| value.0.as_slice()),
            original.diff.files[index].new_path.as_deref()
        );
    }
    assert_eq!(
        source.result().patch,
        original.patch,
        "raw complete capture retained; only admitted canonical members carry authority"
    );
    assert_eq!(real.snapshot(), before);
}

#[test]
fn native_mutable_diff_source_borrowed_preflight_rejects_missing_unsafe_duplicate_paths() {
    let real = Real::new("sha1");
    let loaded = workspace(&real, RepositoryMode::Working);
    let intent = MutableDiffIntentV2::capture(&loaded).unwrap();
    let op = operation(&intent);
    let actual = loaded.comparison.as_ref().unwrap();
    real.clear_trace();
    for path in [
        vec![b'a'; PATH_CAP + 1],
        b"../escape".to_vec(),
        b"/absolute".to_vec(),
        b"a//b".to_vec(),
        b"nul\0name".to_vec(),
        vec![],
    ] {
        let mut malformed = actual.clone();
        malformed.diff.files[0].new_path = Some(path);
        assert!(preflight_members(&malformed, &op).is_err());
    }
    let mut malformed = actual.clone();
    malformed.diff.files[0].new_path = None;
    malformed.diff.files[0].old_path = None;
    assert!(preflight_members(&malformed, &op).is_err());
    malformed = actual.clone();
    malformed.diff.files.push(malformed.diff.files[0].clone());
    assert!(preflight_members(&malformed, &op).is_err());
    assert!(
        real.children().is_empty(),
        "pure negative preflight cannot read Git"
    );
    assert_eq!(op.accounting().unwrap().children_started(), 0);
}

#[test]
fn native_mutable_diff_source_borrowed_fifty_thousand_member_cap_preserves_dense_prefix() {
    let real = Real::new("sha1");
    let loaded = workspace(&real, RepositoryMode::Working);
    let intent = MutableDiffIntentV2::capture(&loaded).unwrap();
    let op = operation(&intent);
    let mut bound_probe = loaded.comparison.as_ref().unwrap().clone();
    let file = bound_probe.diff.files[0].clone();
    bound_probe.diff.files = (0..=MEMBER_CAP)
        .map(|index| {
            let mut current = file.clone();
            // Four-byte single-side probes keep the independent display-byte
            // cap below8MiB so this test reaches the50k member boundary itself.
            let path = format!("{index:04x}").into_bytes();
            current.old_path = None;
            current.new_path = Some(path);
            current
        })
        .collect();
    real.clear_trace();
    assert_eq!(preflight_members(&bound_probe, &op).unwrap(), MEMBER_CAP);
    assert!(real.children().is_empty());
    op.seal().unwrap();
    assert!(preflight_members(&bound_probe, &op).is_err());
}

#[test]
fn native_mutable_diff_source_real_conflict_refuses_combined_source_without_invented_sides() {
    let real = Real::new("sha1");
    git(&real.root, &["reset", "--hard", "HEAD"], None);
    git(&real.root, &["checkout", "-qb", "conflicting-side"], None);
    fs::write(real.root.join("tracked"), b"SIDE conflict\n").unwrap();
    git(&real.root, &["add", "--", "tracked"], None);
    git(&real.root, &["commit", "-qm", "side conflict"], None);
    git(&real.root, &["checkout", "-q", "main"], None);
    fs::write(real.root.join("tracked"), b"MAIN conflict\n").unwrap();
    git(&real.root, &["add", "--", "tracked"], None);
    git(&real.root, &["commit", "-qm", "main conflict"], None);
    let intent = MutableDiffIntentV2::capture(&workspace(&real, RepositoryMode::Working)).unwrap();
    let result = std::process::Command::new("git")
        .current_dir(&real.root)
        .args(["merge", "--no-edit", "conflicting-side"])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(real.root.join(".git/MERGE_HEAD").exists());
    let before = real.snapshot();
    let merge_head = fs::read(real.root.join(".git/MERGE_HEAD")).unwrap();
    real.clear_trace();
    let op = operation(&intent);
    let error = MutableDiffSourceV2::read(intent, &op).unwrap_err();
    assert!(
        error.contains("conflict") || error.contains("combined"),
        "{error}"
    );
    assert_eq!(real.snapshot(), before);
    assert_eq!(
        fs::read(real.root.join(".git/MERGE_HEAD")).unwrap(),
        merge_head
    );
}
