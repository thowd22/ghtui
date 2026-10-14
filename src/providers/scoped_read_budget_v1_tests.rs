//! UNREGISTERED/UNRUN common runner bounds contracts; real strict child capture.
use super::*;
use crate::test_support::{Fixture, GhStep};
use serde_json::{json, Value};

fn spec(fixture: &Fixture, name: &str, budget: &Arc<ScopedReadBudgetV1>) -> CommandSpec {
    let mut spec = CommandSpec::new(fixture.root.join("bin/gh"))
        .fresh_read()
        .args([name])
        .env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"));
    spec.scoped_read_budget = Some(budget.clone());
    spec
}
fn budget(bytes: usize, children: usize, cap: usize) -> Arc<ScopedReadBudgetV1> {
    ScopedReadBudgetV1::new(
        Instant::now() + Duration::from_secs(4),
        bytes,
        children,
        cap,
        &CancellationToken::default(),
    )
    .unwrap()
}
fn journal(fixture: &Fixture) -> Vec<Value> {
    std::fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn wait(fixture: &Fixture, count: usize) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while journal(fixture).len() != count {
        assert!(
            Instant::now() < deadline,
            "Actual strict child did not start"
        );
        thread::yield_now();
    }
}
fn exact(fixture: &Fixture, steps: &[GhStep]) {
    let rows = journal(fixture);
    assert_eq!(rows.len(), steps.len());
    for (row, step) in rows.iter().zip(steps) {
        assert_eq!(row["matched"], true);
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(row["stdin_hex"], "");
    }
}

#[test]
fn repo_catalog_measured_runner_actual_success_reuses_pool_and_counts_raw_stdout_stderr() {
    let fixture = Fixture::new();
    let mut steps = Vec::new();
    for ordinal in 0..6 {
        let mut step = GhStep::read(&[&format!("read-{ordinal}")], "secret");
        step.stderr = b"\x1b[0m\x01\x02".to_vec();
        steps.push(step);
    }
    fixture.scripted_gh(&steps);
    let budget = budget(96, 6, 64);
    let runner = CommandRunner::default();
    for ordinal in 0..6 {
        let mut spec = spec(&fixture, &format!("read-{ordinal}"), &budget);
        spec.sensitive_values = vec!["secret".into()];
        let output = runner.run(spec, &CancellationToken::default()).unwrap();
        assert_eq!(output.stdout, b"[REDACTED]");
        assert!(output.stderr.is_empty());
        assert_eq!(output.capture.stdout_bytes(), 6);
        assert_eq!(output.capture.stderr_bytes(), 6);
    }
    let measured = budget.seal(&CancellationToken::default()).unwrap();
    assert_eq!(measured.children_started(), 6);
    assert_eq!(measured.requests_claimed(), 6);
    assert_eq!(measured.successful_children(), 6);
    assert_eq!(measured.stdout_bytes(), 36);
    assert_eq!(measured.stderr_bytes(), 36);
    assert_eq!(measured.charged_bytes(), 72);
    assert_eq!(measured.failed_reserved_bytes(), 0);
    assert_eq!(measured.in_flight_reserved_bytes(), 0);
    assert!(measured.sealed());
    assert_eq!(budget.held_bytes().unwrap(), 72);
    exact(&fixture, &steps);
}

#[test]
fn repo_catalog_measured_runner_combined_pipe_overflow_charges_full_cap_before_next_child() {
    let fixture = Fixture::new();
    let mut step = GhStep::read(&["combined"], &"a".repeat(40));
    step.stderr = vec![1; 40];
    fixture.scripted_gh(std::slice::from_ref(&step));
    let budget = budget(64, 2, 64);
    let runner = CommandRunner::default();
    let error = runner
        .run(
            spec(&fixture, "combined", &budget),
            &CancellationToken::default(),
        )
        .unwrap_err();
    assert!(matches!(error.kind, FailureKind::OutputLimit));
    assert!(!error.outcome_unknown);
    let error = runner
        .run(
            spec(&fixture, "no-next", &budget),
            &CancellationToken::default(),
        )
        .unwrap_err();
    assert!(matches!(error.kind, FailureKind::OutputLimit));
    let measured = budget.seal(&CancellationToken::default()).unwrap();
    assert_eq!(measured.children_started(), 1);
    assert_eq!(measured.requests_claimed(), 1);
    assert_eq!(measured.failed_reserved_bytes(), 64);
    assert_eq!(measured.charged_bytes(), 64);
    exact(&fixture, &[step]);
}

#[test]
fn repo_catalog_measured_runner_multiple_failed_children_never_charge_redacted_zero() {
    let fixture = Fixture::new();
    let mut steps = Vec::new();
    for name in ["failure-one", "failure-two"] {
        let mut step = GhStep::read(&[name], "");
        step.status = 1;
        step.stderr = vec![1, 2, 3, 4];
        steps.push(step);
    }
    fixture.scripted_gh(&steps);
    let budget = budget(128, 3, 64);
    let runner = CommandRunner::default();
    for name in ["failure-one", "failure-two"] {
        let error = runner
            .run(spec(&fixture, name, &budget), &CancellationToken::default())
            .unwrap_err();
        assert!(matches!(error.kind, FailureKind::Exit));
        assert!(error.stderr.is_empty());
    }
    assert!(runner
        .run(
            spec(&fixture, "third-must-not-run", &budget),
            &CancellationToken::default()
        )
        .is_err());
    let measured = budget.seal(&CancellationToken::default()).unwrap();
    assert_eq!(measured.children_started(), 2);
    assert_eq!(measured.requests_claimed(), 2);
    assert_eq!(measured.successful_children(), 0);
    assert_eq!(measured.stdout_bytes(), 0);
    assert_eq!(measured.stderr_bytes(), 0);
    assert_eq!(measured.failed_reserved_bytes(), 128);
    assert_eq!(measured.charged_bytes(), 128);
    exact(&fixture, &steps);
}

#[test]
fn repo_catalog_measured_runner_clones_share_concurrent_reservations_not_coalesced_results() {
    let fixture = Fixture::new();
    let mut first = GhStep::read(&["first"], "first");
    first.delay_ms = 300;
    let second = GhStep::read(&["second"], "second");
    let steps = [first, second];
    fixture.scripted_gh(&steps);
    let budget = budget(64, 3, 64);
    let runner = CommandRunner::default();
    let source = spec(&fixture, "first", &budget);
    let worker = runner.clone();
    let handle = thread::spawn(move || worker.run(source, &CancellationToken::default()));
    wait(&fixture, 1);
    let active = budget.accounting().unwrap();
    assert_eq!(active.in_flight_requests(), 1);
    assert_eq!(active.in_flight_reserved_bytes(), 64);
    assert!(runner
        .run(
            spec(&fixture, "second", &budget),
            &CancellationToken::default()
        )
        .is_err());
    assert_eq!(
        journal(&fixture).len(),
        1,
        "A clone cannot allocate outside the same live pool"
    );
    assert_eq!(handle.join().unwrap().unwrap().stdout, b"first");
    assert_eq!(
        runner
            .clone()
            .run(
                spec(&fixture, "second", &budget),
                &CancellationToken::default()
            )
            .unwrap()
            .stdout,
        b"second"
    );
    let measured = budget.seal(&CancellationToken::default()).unwrap();
    assert_eq!(measured.children_started(), 2);
    assert_eq!(measured.requests_claimed(), 2);
    assert_eq!(measured.charged_bytes(), 11);
    assert_eq!(measured.failed_reserved_bytes(), 0);
    exact(&fixture, &steps);
}

#[test]
fn repo_catalog_measured_runner_write_coalesce_count_cap_and_postseal_never_spawn() {
    let fixture = Fixture::new();
    let step = GhStep::read(&["one"], "ok");
    fixture.scripted_gh(std::slice::from_ref(&step));
    let budget = budget(64, 1, 64);
    let runner = CommandRunner::default();
    let error = runner
        .run(
            spec(&fixture, "write", &budget).write("github/private"),
            &CancellationToken::default(),
        )
        .unwrap_err();
    assert!(matches!(error.kind, FailureKind::ReconciliationRequired));
    let mut coalesced = spec(&fixture, "coalesced", &budget);
    coalesced.coalesce_read = true;
    assert!(runner
        .run(coalesced, &CancellationToken::default())
        .is_err());
    assert!(journal(&fixture).is_empty());
    assert_eq!(budget.accounting().unwrap().requests_claimed(), 0);
    runner
        .run(
            spec(&fixture, "one", &budget),
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(runner
        .run(
            spec(&fixture, "too-many", &budget),
            &CancellationToken::default()
        )
        .is_err());
    let frozen = budget.seal(&CancellationToken::default()).unwrap();
    assert!(runner
        .run(
            spec(&fixture, "after-final-proof", &budget),
            &CancellationToken::default()
        )
        .is_err());
    assert_eq!(budget.accounting().unwrap(), frozen);
    assert_eq!(runner.resource_generation("github/private"), 0);
    exact(&fixture, &[step]);
}

#[test]
fn repo_catalog_measured_runner_original_deadline_cannot_be_extended_by_spec_timeout() {
    let fixture = Fixture::new();
    let mut step = GhStep::read(&["slow"], "unadmitted");
    step.delay_ms = 1000;
    fixture.scripted_gh(std::slice::from_ref(&step));
    let started = Instant::now();
    let budget = ScopedReadBudgetV1::new(
        started + Duration::from_millis(220),
        64,
        2,
        64,
        &CancellationToken::default(),
    )
    .unwrap();
    let error = CommandRunner::default()
        .run(
            spec(&fixture, "slow", &budget).timeout(Duration::from_secs(30)),
            &CancellationToken::default(),
        )
        .unwrap_err();
    assert!(matches!(error.kind, FailureKind::Timeout));
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(CommandRunner::default()
        .run(
            spec(&fixture, "new-clock", &budget),
            &CancellationToken::default()
        )
        .is_err());
    let accounting = budget.accounting().unwrap();
    assert_eq!(accounting.children_started(), 1);
    assert_eq!(accounting.failed_reserved_bytes(), 64);
    assert!(budget.seal(&CancellationToken::default()).is_err());
    exact(&fixture, &[step]);
}

#[test]
fn repo_catalog_measured_runner_pre_and_live_cancel_prevent_next_child_and_reap_actual_pid() {
    let fixture = Fixture::new();
    let mut step = GhStep::read(&["cancel-live"], "unadmitted");
    step.delay_ms = 1000;
    fixture.scripted_gh(std::slice::from_ref(&step));
    let token = CancellationToken::default();
    let budget =
        ScopedReadBudgetV1::new(Instant::now() + Duration::from_secs(4), 128, 3, 64, &token)
            .unwrap();
    let cancelled = CancellationToken::default();
    cancelled.cancel();
    assert!(CommandRunner::default()
        .run(spec(&fixture, "pre-cancelled", &budget), &cancelled)
        .is_err());
    assert_eq!(budget.accounting().unwrap().requests_claimed(), 0);
    assert!(journal(&fixture).is_empty());
    // A clone must not evade original cancellation by passing a fresh token.
    let child_token = CancellationToken::default();
    let source = spec(&fixture, "cancel-live", &budget);
    let worker = thread::spawn(move || CommandRunner::default().run(source, &child_token));
    wait(&fixture, 1);
    let pid = i32::try_from(journal(&fixture)[0]["pid"].as_u64().unwrap()).unwrap();
    assert!(pid > 0);
    token.cancel();
    let error = worker.join().unwrap().unwrap_err();
    assert!(matches!(error.kind, FailureKind::Cancelled));
    // SAFETY: signal0 inspects only the strict fixture's positive child PID.
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ESRCH)
    );
    assert!(CommandRunner::default()
        .run(
            spec(&fixture, "cancel-next", &budget),
            &CancellationToken::default()
        )
        .is_err());
    let measured = budget.accounting().unwrap();
    assert_eq!(measured.children_started(), 1);
    assert_eq!(measured.failed_reserved_bytes(), 64);
    assert_eq!(measured.requests_claimed(), 1);
    assert!(budget.seal(&token).is_err());
    exact(&fixture, &[step]);
}

#[test]
fn repo_catalog_measured_runner_caught_claim_panic_retains_full_reservation_not_zero_children() {
    let fixture = Fixture::new();
    let step = GhStep::read(&["after-drop"], "ok");
    fixture.scripted_gh(std::slice::from_ref(&step));
    let budget = budget(96, 2, 64);
    let source = spec(&fixture, "claimed-but-not-spawned", &budget);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _claim = budget
            .claim(&source, &CancellationToken::default())
            .unwrap();
        panic!("deliberate original claim owner failure before execute");
    }));
    assert!(result.is_err());
    assert!(journal(&fixture).is_empty());
    let before = budget.accounting().unwrap();
    assert_eq!(before.requests_claimed(), 1);
    assert_eq!(before.children_started(), 0);
    assert_eq!(before.failed_reserved_bytes(), 64);
    assert_eq!(before.in_flight_reserved_bytes(), 0);
    CommandRunner::default()
        .run(
            spec(&fixture, "after-drop", &budget),
            &CancellationToken::default(),
        )
        .unwrap();
    let measured = budget.seal(&CancellationToken::default()).unwrap();
    assert_eq!(measured.charged_bytes(), 66);
    assert_eq!(measured.children_started(), 1);
    assert_eq!(measured.requests_claimed(), 2);
    exact(&fixture, &[step]);
}

#[test]
fn repo_catalog_measured_runner_inflight_seal_blocks_new_clones_until_original_request_refuses() {
    let fixture = Fixture::new();
    let mut step = GhStep::read(&["in-flight"], "ok");
    step.delay_ms = 300;
    fixture.scripted_gh(std::slice::from_ref(&step));
    let budget = budget(128, 3, 64);
    let source = spec(&fixture, "in-flight", &budget);
    let worker =
        thread::spawn(move || CommandRunner::default().run(source, &CancellationToken::default()));
    wait(&fixture, 1);
    let error = budget.seal(&CancellationToken::default()).unwrap_err();
    assert!(matches!(error.kind, FailureKind::ReconciliationRequired));
    assert!(budget.accounting().unwrap().sealed());
    assert!(CommandRunner::default()
        .run(
            spec(&fixture, "late-clone", &budget),
            &CancellationToken::default()
        )
        .is_err());
    worker.join().unwrap().unwrap();
    assert_eq!(budget.accounting().unwrap().children_started(), 1);
    exact(&fixture, &[step]);
}
