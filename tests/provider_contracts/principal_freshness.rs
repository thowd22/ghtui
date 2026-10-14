//! Actual fresh-read regressions across principal/request security boundaries.
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

fn wait_dispatched(fixture: &Fixture, count: usize) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while journal(fixture).len() != count {
        assert!(
            Instant::now() < deadline,
            "Exact fixture child was not dispatched"
        );
        thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn process_runner_gh_provider_auth_capabilities_fresh_principal_must_not_join_preboundary_user_read(
) {
    let fixture = Fixture::new();
    let version = GhStep::read(&["--version"], "gh version 2.101.0\n");
    let mut old_user = step(api_args("user"), json!({"id":42,"login":"octocat"}));
    old_user.delay_ms = 2_000;
    fixture.scripted_gh(&[
        version.clone(),
        old_user,
        version,
        step(api_args("user"), json!({"id":43,"login":"octocat"})),
    ]);
    let provider = provider(&fixture);
    let old = provider.clone();
    let complete = Arc::new(AtomicBool::new(false));
    let old_complete = complete.clone();
    let worker = thread::spawn(move || {
        let result = old.identity(&CancellationToken::default()).unwrap();
        old_complete.store(true, Ordering::Release);
        result
    });
    wait_dispatched(&fixture, 2);
    assert!(
        !complete.load(Ordering::Acquire),
        "Delayed fixture overlap expired before the new principal boundary"
    );
    let fresh = provider.identity(&CancellationToken::default()).unwrap();
    assert_eq!(worker.join().unwrap().account_id, Some(42));
    assert_eq!(
        fresh.account_id,
        Some(43),
        "A fresh principal must not inherit the older in-flight user response"
    );
    let calls = journal(&fixture);
    assert_eq!(calls.len(), 4);
    assert!(calls.iter().all(|call| call["matched"] == true));
    assert_eq!(calls[1]["args"], calls[3]["args"]);
    assert_ne!(calls[1]["pid"], calls[3]["pid"]);
}

#[test]
fn process_runner_fresh_read_launches_independent_child_while_default_read_is_in_flight() {
    let fixture = Fixture::new();
    let mut old = GhStep::read(&["observe"], "pre-boundary state");
    old.delay_ms = 300;
    fixture.scripted_gh(&[old, GhStep::read(&["observe"], "post-boundary state")]);
    let runner = CommandRunner::default();
    let owner = runner.clone();
    let spec = runner_spec(&fixture, &["observe"]);
    let worker = thread::spawn(move || owner.run(spec, &CancellationToken::default()).unwrap());
    wait_dispatched(&fixture, 1);
    let fresh = runner
        .run(
            runner_spec(&fixture, &["observe"]).fresh_read(),
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(fresh.stdout, b"post-boundary state");
    assert_eq!(worker.join().unwrap().stdout, b"pre-boundary state");
    let calls = journal(&fixture);
    assert_eq!(calls.len(), 2);
    assert!(calls.iter().all(|call| call["matched"] == true));
    assert_ne!(calls[0]["pid"], calls[1]["pid"]);
}

#[test]
fn process_runner_fresh_read_cancellation_does_not_cancel_other_subscriber_or_leak_child() {
    let fixture = Fixture::new();
    let mut old = GhStep::read(&["observe"], "older read survives");
    old.delay_ms = 300;
    let mut fresh = GhStep::read(&["observe"], "fresh read must be cancelled");
    fresh.delay_ms = 2_000;
    fixture.scripted_gh(&[old, fresh]);
    let runner = CommandRunner::default();
    let owner = runner.clone();
    let spec = runner_spec(&fixture, &["observe"]);
    let worker = thread::spawn(move || owner.run(spec, &CancellationToken::default()).unwrap());
    wait_dispatched(&fixture, 1);
    let cancel = CancellationToken::default();
    let fresh_cancel = cancel.clone();
    let fresh_runner = runner.clone();
    let spec = runner_spec(&fixture, &["observe"]).fresh_read();
    let fresh_worker = thread::spawn(move || fresh_runner.run(spec, &fresh_cancel));
    wait_dispatched(&fixture, 2);
    cancel.cancel();
    assert_eq!(
        fresh_worker.join().unwrap().unwrap_err().kind,
        FailureKind::Cancelled
    );
    assert_eq!(worker.join().unwrap().stdout, b"older read survives");
    let calls = journal(&fixture);
    assert_eq!(calls.len(), 2);
    let pid = calls[1]["pid"].as_u64().unwrap() as libc::pid_t;
    // The cancelled child must have been killed and reaped, not left executing.
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ESRCH)
    );
}

#[test]
fn process_runner_fresh_read_keeps_output_cap_and_reaps_oversized_child() {
    let fixture = Fixture::new();
    let mut output = GhStep::read(&["large"], "");
    output.stdout = vec![b'x'; 64 * 1024];
    fixture.scripted_gh(&[output]);
    let mut spec = runner_spec(&fixture, &["large"]).fresh_read();
    spec.output_limit = 256;
    let error = CommandRunner::default()
        .run(spec, &CancellationToken::default())
        .unwrap_err();
    assert_eq!(error.kind, FailureKind::OutputLimit);
    assert!(!error.outcome_unknown);
    let calls = journal(&fixture);
    assert_eq!(calls.len(), 1);
    assert!(calls[0]["matched"] == true);
    let pid = calls[0]["pid"].as_u64().unwrap() as libc::pid_t;
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ESRCH)
    );
}

#[test]
fn process_runner_gh_provider_auth_capabilities_private_resource_read_does_not_join_preboundary_account_data(
) {
    let fixture = Fixture::new();
    let mut old = pr(7, "example/repo");
    old["body"] = json!("private old-account body");
    let mut fresh = pr(7, "example/repo");
    fresh["body"] = json!("new-account current body");
    let mut old_read = step(api_args("repos/example/repo/pulls/7"), old);
    old_read.delay_ms = 300;
    fixture.scripted_gh(&[
        old_read,
        step(api_args("repos/example/repo/pulls/7"), fresh),
    ]);
    let provider = provider(&fixture);
    let owner = provider.clone();
    let worker = thread::spawn(move || {
        owner
            .pr_summary("example/repo", 7, &CancellationToken::default())
            .unwrap()
    });
    wait_dispatched(&fixture, 1);
    let fresh = provider
        .pr_summary("example/repo", 7, &CancellationToken::default())
        .unwrap();
    assert_eq!(fresh.pr.body, "new-account current body");
    assert_eq!(worker.join().unwrap().pr.body, "private old-account body");
    let calls = journal(&fixture);
    assert_eq!(calls.len(), 2);
    assert!(calls.iter().all(|call| call["matched"] == true));
    assert_eq!(calls[0]["args"], calls[1]["args"]);
    assert_ne!(calls[0]["pid"], calls[1]["pid"]);
}
