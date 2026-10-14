use super::*;
use crate::local_commit_actor_origin_v1_test_support as fixture;
use fixture::{git, text, Real};
use std::{fs, time::Duration};

#[test]
fn git_actor_origin_real_sha1_sha256_retained_handle_reads_two_children_no_rediscovery() {
    for format in ["sha1", "sha256"] {
        let real = Real::new(format);
        let before = real.snapshot();
        real.clear_trace();
        let origin = OriginalLocalActorOriginV1::capture(&real.repository).unwrap();
        assert!(
            real.children().is_empty(),
            "Source observation adds no Git child"
        );
        let token = origin
            .read_original(&real.original, &Default::default(), &Default::default())
            .unwrap();
        assert_eq!(token.sha(), real.original);
        assert_ne!(token.sha(), real.head);
        assert_eq!(token.source(), origin.source());
        assert_eq!(token.author().name_bytes(), b"Original Writer");
        assert_eq!(token.committer().name_bytes(), b"Original Recorder");
        let children = real.children();
        assert_eq!(
            children.len(),
            2,
            "Actual raw reads, no discover/ref/status children"
        );
        for (event, expected) in children.iter().zip(["--batch-check", "--batch"]) {
            let argv = event["argv"].as_array().unwrap();
            assert!(argv.iter().any(|arg| arg == "cat-file"));
            assert!(argv.iter().any(|arg| arg == expected));
            assert!(argv.iter().any(|arg| arg == "--no-replace-objects"));
            assert!(!argv.iter().any(|arg| arg == "rev-parse"));
        }
        assert_eq!(real.snapshot(), before);
        assert_eq!(
            git(&real.root, &["cat-file", "commit", &real.original], None),
            real.raw
        );
    }
}
#[test]
fn git_actor_origin_real_same_path_same_sha_new_inode_refuses_before_token_children() {
    let real = Real::new("sha1");
    let origin = OriginalLocalActorOriginV1::capture(&real.repository).unwrap();
    let retired = real.directory.path().join("original-kept");
    fs::rename(&real.root, &retired).unwrap();
    git(
        real.directory.path(),
        &[
            "clone",
            "--quiet",
            "--no-hardlinks",
            retired.to_str().unwrap(),
            real.root.to_str().unwrap(),
        ],
        None,
    );
    assert_eq!(
        text(&real.root, &["rev-parse", "refs/tags/original-source"]),
        real.original
    );
    fs::write(real.root.join("scratch"), b"replacement untracked bytes\n").unwrap();
    let before = real.snapshot();
    real.clear_trace();
    let error = origin
        .read_original(&real.original, &Default::default(), &Default::default())
        .unwrap_err();
    assert!(error.0.contains("source/locator changed"));
    assert!(
        real.children().is_empty(),
        "Neither raw read may rebind a replacement source"
    );
    assert_eq!(real.snapshot(), before);
    assert!(retired.join(".git").is_dir());
}
#[test]
fn git_actor_origin_real_linked_worktree_locator_to_existing_same_sha_refuses_before_children() {
    let real = Real::new("sha1");
    let first = real.directory.path().join("linked-original");
    let second = real.directory.path().join("linked-replacement");
    for path in [&first, &second] {
        git(
            &real.root,
            &[
                "worktree",
                "add",
                "--detach",
                path.to_str().unwrap(),
                &real.original,
            ],
            None,
        );
    }
    let repository = GitRepository::discover(&first).unwrap().unwrap();
    let _linked_trace = crate::git::register_actor_origin_test_trace(&repository.root, &real.trace);
    let origin = OriginalLocalActorOriginV1::capture(&repository).unwrap();
    let first_locator = fs::read(first.join(".git")).unwrap();
    fs::write(first.join(".git"), fs::read(second.join(".git")).unwrap()).unwrap();
    assert!(repository.git_dir.is_dir() && repository.common_dir.is_dir());
    assert_eq!(text(&second, &["rev-parse", "HEAD"]), real.original);
    real.clear_trace();
    assert!(origin
        .read_original(&real.original, &Default::default(), &Default::default())
        .is_err());
    assert!(real.children().is_empty());
    fs::write(first.join(".git"), first_locator).unwrap();
    origin.recheck(&Default::default()).unwrap();
    assert_eq!(text(&first, &["rev-parse", "HEAD"]), real.original);
}
#[test]
fn git_actor_origin_fresh_request_cancel_not_finished_loading_token_and_no_raw_revival() {
    let real = Real::new("sha1");
    let loading = CancellationToken::default();
    let repository = real.repository.clone().with_cancellation(loading.clone());
    let origin = OriginalLocalActorOriginV1::capture(&repository).unwrap();
    loading.cancel();
    let request = CancellationToken::default();
    real.clear_trace();
    let token = origin
        .read_original(&real.original, &Default::default(), &request)
        .unwrap();
    assert_eq!(real.children().len(), 2);
    request.cancel();
    assert!(
        token.remaining(&Default::default()).is_err(),
        "Raw token does not get a new clock/cancel"
    );
    real.clear_trace();
    assert!(origin
        .read_original(&real.original, &Default::default(), &request)
        .is_err());
    assert!(real.children().is_empty());
}
#[test]
fn git_actor_origin_expired_original_preflight_and_invalid_sha_launch_zero_children() {
    let real = Real::new("sha256");
    let origin = OriginalLocalActorOriginV1::capture(&real.repository).unwrap();
    let before = real.snapshot();
    real.clear_trace();
    assert!(origin
        .read_original_under_deadline(
            &real.original,
            &Default::default(),
            Instant::now() - Duration::from_secs(1),
            &Default::default()
        )
        .unwrap_err()
        .0
        .contains("deadline"));
    for bad in [
        real.original[..12].to_owned(),
        real.original.to_uppercase(),
        "0".repeat(64),
        "x".repeat(4097),
    ] {
        assert!(origin
            .read_original(&bad, &Default::default(), &Default::default())
            .is_err());
    }
    assert!(real.children().is_empty());
    assert_eq!(real.snapshot(), before);
}
#[test]
fn git_actor_origin_same_absolute_deadline_survives_source_preflight_and_raw_token() {
    let real = Real::new("sha1");
    let origin = OriginalLocalActorOriginV1::capture(&real.repository).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let token = origin
        .read_original_under_deadline(
            &real.original,
            &Default::default(),
            deadline,
            &Default::default(),
        )
        .unwrap();
    assert!(
        token.deadline() <= deadline,
        "No fresh duration after filesystem/source preflight"
    );
    assert_eq!(token.source(), origin.source());
}
#[test]
fn git_actor_origin_capture_bounds_paths_before_handle_copy_and_never_formats_source_payload() {
    let real = Real::new("sha1");
    let mut bad = real.repository.clone();
    bad.git_dir = std::path::PathBuf::from("x".repeat(4097));
    assert!(OriginalLocalActorOriginV1::capture(&bad)
        .unwrap_err()
        .0
        .contains("bound"));
    let canceled = CancellationToken::default();
    canceled.cancel();
    assert!(OriginalLocalActorOriginV1::capture(
        &real.repository.clone().with_cancellation(canceled)
    )
    .is_err());
    let origin = OriginalLocalActorOriginV1::capture(&real.repository).unwrap();
    assert!(!format!("{origin:?}").contains(real.root.to_str().unwrap()));
}

#[test]
fn git_actor_origin_real_scoped_trace_guard_unregisters_on_unwind_without_global_env() {
    let real = Real::new("sha1");
    let linked = real.directory.path().join("guard-original");
    git(
        &real.root,
        &[
            "worktree",
            "add",
            "--detach",
            linked.to_str().unwrap(),
            &real.original,
        ],
        None,
    );
    let repository = GitRepository::discover(&linked).unwrap().unwrap();
    let result = std::panic::catch_unwind(|| {
        let _guard = register_fixture_trace(&repository.root, &real.trace);
        panic!("Controlled fixture unwind");
    });
    assert!(result.is_err());
    let traces = FIXTURE_TRACES.get().unwrap().lock().unwrap();
    assert!(!traces.contains_key(&repository.root));
    assert!(
        traces.contains_key(&real.root),
        "Other fixture's original root remains registered"
    );
}

#[test]
fn git_actor_origin_real_symlink_directory_uses_actual_root_and_positive_trace_children() {
    let real = Real::new("sha1");
    let alias = real.directory.path().join("source-alias");
    std::os::unix::fs::symlink(&real.root, &alias).unwrap();
    let repository = GitRepository::discover(&alias).unwrap().unwrap();
    assert_eq!(repository.root, real.repository.root);
    let origin = OriginalLocalActorOriginV1::capture(&repository).unwrap();
    let before = real.snapshot();
    real.clear_trace();
    let raw = origin
        .read_original(&real.original, &Default::default(), &Default::default())
        .unwrap();
    assert_eq!(raw.sha(), real.original);
    let children = real.children();
    assert_eq!(
        children.len(),
        2,
        "A canonical/symlink root must never give a falsely empty trace"
    );
    assert!(children.iter().all(|event| event["argv"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| arg == "cat-file")));
    assert_eq!(raw.source(), origin.source());
    assert_eq!(real.snapshot(), before);
}
