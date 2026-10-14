//! UNRUN installed-Git forwarding and fresh original-actor contracts.
use super::*;
use crate::{
    providers::runner::{CommandRunner, ScopedReadBudgetV1},
    remote_repository_v1_test_support as fx,
    test_support::Fixture,
};
use std::time::Instant;
#[test]
fn remote_tree_v1_git_window_is_original_shared_capture_and_never_executes_writes() {
    let fixture = Fixture::new();
    let objects = fx::objects(&fixture);
    let cancel = CancellationToken::default();
    let pool = ScopedReadBudgetV1::new(
        Instant::now() + Duration::from_secs(30),
        8 * 1024 * 1024,
        32,
        1024 * 1024,
        &cancel,
    )
    .unwrap();
    let producer = CommandRunner::default();
    let window = RemoteGitReadWindowV1::new(
        producer.clone(),
        pool.clone(),
        pool.deadline(),
        1024 * 1024,
        cancel.clone(),
    );
    let repository = GitRepository::discover_with_remote_read_window_v1(
        &fixture.repo(),
        cancel.clone(),
        window.clone(),
    )
    .unwrap()
    .unwrap();
    assert!(repository.actual_read_runner_v1().same_instance(&producer));
    assert_eq!(
        repository.resolve_original(&objects.commit).unwrap(),
        objects.commit
    );
    assert_eq!(
        repository
            .blob(
                &objects.commit,
                &RawPath::new("nested/日本語.txt".as_bytes().to_vec())
            )
            .unwrap(),
        objects.bytes
    );
    assert!(pool.accounting().unwrap().requests_claimed() >= 6);
    let claimed = pool.accounting().unwrap().requests_claimed();
    assert!(window
        .run(
            crate::providers::runner::CommandSpec::new("git")
                .args(["checkout", "main"])
                .cwd(fixture.repo())
                .write("fixture-write"),
            &cancel
        )
        .is_err());
    assert_eq!(
        pool.accounting().unwrap().requests_claimed(),
        claimed,
        "write is refused before claim/spawn"
    );
    assert!(
        repository
            .clone()
            .strip_navigation_read_window_v1()
            .is_err(),
        "live policy cannot reset/escape"
    );
    cancel.cancel();
    assert!(repository.resolve_original(&objects.commit).is_err());
    fx::preserved(&fixture, &objects);
}
#[test]
fn remote_tree_v1_completed_handoff_origin_allows_fresh_raw_two_but_old_read_cannot_revive() {
    let fixture = Fixture::new();
    let objects = fx::objects(&fixture);
    let loading = CancellationToken::default();
    let pool = ScopedReadBudgetV1::new(
        Instant::now() + Duration::from_secs(30),
        8 * 1024 * 1024,
        32,
        1024 * 1024,
        &loading,
    )
    .unwrap();
    let producer = CommandRunner::default();
    let window = RemoteGitReadWindowV1::new(
        producer.clone(),
        pool.clone(),
        pool.deadline(),
        1024 * 1024,
        loading.clone(),
    );
    let repository = GitRepository::discover_with_remote_read_window_v1(
        &fixture.repo(),
        loading.clone(),
        window,
    )
    .unwrap()
    .unwrap();
    let origin = OriginalLocalActorOriginV1::capture(&repository).unwrap();
    origin.recheck_loading().unwrap();
    pool.seal(&loading).unwrap();
    let stripped = repository
        .clone()
        .strip_navigation_read_window_v1()
        .unwrap();
    assert!(stripped.actual_read_runner_v1().same_instance(&producer));
    assert!(!stripped
        .actual_read_runner_v1()
        .same_instance(&CommandRunner::default()));
    assert!(stripped.navigation_read_window.is_none());
    assert_eq!(
        stripped.resolve_original(&objects.commit).unwrap(),
        objects.commit
    );
    assert!(
        repository.resolve_original(&objects.commit).is_err(),
        "completed operation itself stays sealed"
    );
    loading.cancel();
    assert!(origin.recheck_loading().is_err());
    let fresh = CancellationToken::default();
    let raw = origin
        .read_original(&objects.commit, &Default::default(), &fresh)
        .unwrap();
    assert_eq!(raw.source(), origin.source());
    assert_eq!(raw.sha(), objects.commit);
    assert!(
        repository
            .clone()
            .with_cancellation(fresh)
            .resolve_original(&objects.commit)
            .is_err(),
        "new token does not unseal old policy"
    );
    fx::preserved(&fixture, &objects);
}
