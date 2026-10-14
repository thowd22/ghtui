use super::*;
use crate::{
    git::OriginalLocalActorOriginV1,
    local_commit_actor_origin_v1_test_support::{git, Real},
    providers::runner::CommandSpec,
};

fn origin(real: &Real) -> Arc<OriginalLocalActorOriginV1> {
    OriginalLocalActorOriginV1::capture(&real.repository).unwrap()
}
fn allocation(
    real: &Real,
    bytes: usize,
    children: usize,
    child: usize,
) -> LocalGitNavigationReadV1 {
    LocalGitNavigationReadV1::allocate(
        real.repository.actual_read_runner_v1().clone(),
        Default::default(),
        Instant::now() + Duration::from_secs(5),
        bytes,
        children,
        child,
    )
    .unwrap()
}
fn spec(real: &Real, arguments: &[&str]) -> CommandSpec {
    let mut spec = CommandSpec::new("git")
        .args(arguments)
        .cwd(&real.root)
        .env("GIT_TRACE2_EVENT", real.trace.as_os_str())
        .env("GIT_ALLOW_PROTOCOL", "")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_OPTIONAL_LOCKS", "0");
    spec.output_limit = 32 * 1024 * 1024;
    spec
}

#[test]
fn original_saved_navigation_git_budget_admits_actual_two_mib_blob_without_gh_cap_change() {
    let real = Real::new("sha256");
    let body = vec![b'k'; 2 * 1024 * 1024];
    let blob = String::from_utf8(git(
        &real.root,
        &["hash-object", "-w", "--stdin"],
        Some(&body),
    ))
    .unwrap()
    .trim()
    .to_owned();
    let before = real.snapshot();
    real.clear_trace();
    let operation = LocalGitNavigationReadV1::begin(&origin(&real), Default::default()).unwrap();
    let output = operation
        .window()
        .run(
            spec(&real, &["cat-file", "blob", &blob]),
            operation.cancellation(),
        )
        .unwrap();
    assert_eq!(output.stdout, body);
    assert_eq!(real.children().len(), 1);
    let accounting = operation.seal().unwrap();
    assert_eq!(accounting.successful_children(), 1);
    assert_eq!(accounting.stdout_bytes(), body.len());
    assert_eq!(accounting.failed_reserved_bytes(), 0);
    assert!(accounting.sealed());
    assert_eq!(real.snapshot(), before);
    assert!(
        ScopedReadBudgetV1::new(
            Instant::now() + Duration::from_secs(1),
            MAX_TOTAL_BYTES,
            128,
            2 * 1024 * 1024,
            &Default::default()
        )
        .is_err(),
        "Existing GH constructor still refuses >1MiB child"
    );
}

#[test]
fn original_saved_navigation_git_budget_failed_output_consumes_actual_full_cap_and_blocks_next() {
    let real = Real::new("sha1");
    let blob = String::from_utf8(git(
        &real.root,
        &["hash-object", "-w", "--stdin"],
        Some(&vec![b'x'; 8192]),
    ))
    .unwrap()
    .trim()
    .to_owned();
    let operation = allocation(&real, 256, 128, 256);
    real.clear_trace();
    assert!(operation
        .window()
        .run(
            spec(&real, &["cat-file", "blob", &blob]),
            operation.cancellation()
        )
        .is_err());
    let charged = operation.accounting().unwrap();
    assert_eq!(charged.children_started(), 1);
    assert_eq!(charged.failed_reserved_bytes(), 256);
    assert_eq!(charged.charged_bytes(), 256);
    assert!(operation
        .window()
        .run(spec(&real, &["rev-parse", "HEAD"]), &Default::default())
        .is_err());
    assert_eq!(operation.accounting().unwrap().children_started(), 1);
    assert_eq!(real.children().len(), 1);
}

#[test]
fn original_saved_navigation_git_budget_clone_seal_and_distinct_token_cannot_reset_ledger() {
    let real = Real::new("sha1");
    let operation = allocation(&real, 4096, 1, 4096);
    let first = operation.window();
    let second = first.clone();
    real.clear_trace();
    assert_eq!(
        first
            .run(
                spec(&real, &["rev-parse", "HEAD"]),
                operation.cancellation()
            )
            .unwrap()
            .stdout,
        format!("{}\n", real.head).as_bytes()
    );
    assert!(second
        .run(spec(&real, &["rev-parse", "HEAD"]), &Default::default())
        .is_err());
    assert_eq!(real.children().len(), 1);
    assert_eq!(operation.seal().unwrap().requests_claimed(), 1);
    assert!(first
        .run(spec(&real, &["rev-parse", "HEAD"]), &Default::default())
        .is_err());
    assert_eq!(operation.accounting().unwrap().children_started(), 1);
}

#[test]
fn original_saved_navigation_git_budget_queue_time_not_worker_entry_and_zero_children_after_expiry()
{
    let real = Real::new("sha1");
    let original = origin(&real);
    let deadline = Instant::now() + Duration::from_millis(5);
    let operation =
        LocalGitNavigationReadV1::begin_under_deadline(&original, Default::default(), deadline)
            .unwrap();
    while Instant::now() < deadline {
        std::thread::yield_now();
    }
    real.clear_trace();
    assert!(operation
        .window()
        .run(spec(&real, &["rev-parse", "HEAD"]), &Default::default())
        .is_err());
    assert_eq!(operation.deadline(), deadline);
    assert_eq!(operation.accounting().unwrap().children_started(), 0);
    assert!(real.children().is_empty());
}

#[test]
fn original_saved_navigation_git_budget_cancel_or_drop_does_not_revive_with_new_token() {
    let real = Real::new("sha256");
    let token = CancellationToken::default();
    let operation = LocalGitNavigationReadV1::begin(&origin(&real), token.clone()).unwrap();
    let window = operation.window();
    token.cancel();
    real.clear_trace();
    assert!(window
        .run(spec(&real, &["rev-parse", "HEAD"]), &Default::default())
        .is_err());
    assert_eq!(operation.accounting().unwrap().children_started(), 0);
    drop(operation);
    assert!(window
        .run(spec(&real, &["rev-parse", "HEAD"]), &Default::default())
        .is_err());
    assert!(real.children().is_empty());
}

#[test]
fn original_saved_navigation_git_budget_rejects_write_kind_foreign_program_and_nested_pool() {
    let real = Real::new("sha1");
    let operation = allocation(&real, 8192, 128, 4096);
    let before = real.snapshot();
    real.clear_trace();
    let write = spec(
        &real,
        &["update-ref", "refs/heads/should-not-exist", &real.original],
    )
    .write("fixture-only");
    assert!(operation
        .window()
        .run(write, operation.cancellation())
        .is_err());
    assert!(operation
        .window()
        .run(
            CommandSpec::new("sh").args(["-c", "exit 0"]),
            operation.cancellation()
        )
        .is_err());
    let mut nested = spec(&real, &["rev-parse", "HEAD"]);
    nested.scoped_read_budget = Some(operation.pool.clone());
    assert!(operation
        .window()
        .run(nested, operation.cancellation())
        .is_err());
    assert!(real.children().is_empty());
    assert_eq!(operation.accounting().unwrap().requests_claimed(), 0);
    assert_eq!(real.snapshot(), before);
}

#[test]
fn original_saved_navigation_git_budget_bounds_refuse_before_any_pool_child() {
    let real = Real::new("sha1");
    let original = origin(&real);
    real.clear_trace();
    for deadline in [
        Instant::now() - Duration::from_secs(1),
        Instant::now() + Duration::from_secs(31),
    ] {
        assert!(LocalGitNavigationReadV1::begin_under_deadline(
            &original,
            Default::default(),
            deadline
        )
        .is_err());
    }
    for (bytes, children, child) in [
        (0, 1, 1),
        (MAX_TOTAL_BYTES + 1, 1, 1),
        (1, 0, 1),
        (1, 129, 1),
        (1, 1, 0),
        (1, 1, MAX_CHILD_BYTES + 1),
    ] {
        assert!(LocalGitNavigationReadV1::allocate(
            real.repository.actual_read_runner_v1().clone(),
            Default::default(),
            Instant::now() + Duration::from_secs(1),
            bytes,
            children,
            child
        )
        .is_err());
    }
    assert!(real.children().is_empty());
}
