//! Original bounds cover inherited pipes even after the immediate child exits.
use super::*;

fn hold(fixture: &Fixture, step: usize, millis: u64) {
    fs::write(
        fixture
            .root
            .join(format!("gh-fixture/{step:03}.pipe-holder-ms")),
        millis.to_string(),
    )
    .unwrap();
}

fn helper_pid(fixture: &Fixture) -> u64 {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Ok(text) = fs::read_to_string(fixture.root.join("gh-fixture/000.pipe-holder-pid")) {
            if let Some(value) = text.strip_suffix('\n') {
                if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) {
                    let pid: u64 = value.parse().unwrap();
                    assert!(pid > 0 && pid <= i32::MAX as u64);
                    return pid;
                }
            }
        }
        assert!(
            Instant::now() < deadline,
            "complete private helper PID record"
        );
        thread::yield_now();
    }
}

fn naturally_gone(pid: u64) {
    // The fixture holder has a hard finite lifetime. Never reap or signal an
    // unrelated orphan; wait only for this private fixture's exact published PID.
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        if unsafe { libc::kill(pid as i32, 0) } == -1 {
            assert_eq!(
                std::io::Error::last_os_error().raw_os_error(),
                Some(libc::ESRCH)
            );
            return;
        }
        assert!(
            Instant::now() < deadline,
            "finite helper {pid} did not exit"
        );
        thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn process_runner_pipe_deadline_covers_parent_exit_and_inherited_both_pipes() {
    let fixture = Fixture::new();
    let mut step = GhStep::read(&["pipe-read"], "complete parent body");
    step.stderr = b"separate diagnostic".to_vec();
    fixture.scripted_gh(&[step]);
    hold(&fixture, 0, 2_000);
    let started = Instant::now();
    let result = CommandRunner::default().run(
        runner_spec(&fixture, &["pipe-read"]).timeout(Duration::from_millis(100)),
        &CancellationToken::default(),
    );
    let elapsed = started.elapsed();
    let pid = helper_pid(&fixture);
    naturally_gone(pid);
    let error = result.unwrap_err();
    assert_eq!(error.kind, FailureKind::Timeout);
    assert_eq!(
        error.exit_code,
        Some(0),
        "direct child really exited successfully"
    );
    assert!(!error.outcome_unknown);
    assert_eq!(error.stdout, b"complete parent body");
    assert_eq!(error.stderr, b"separate diagnostic");
    assert!(
        elapsed < Duration::from_secs(1),
        "pipe drain renewed the original deadline: {elapsed:?}"
    );
    assert_eq!(journal(&fixture).len(), 1);
    assert_process_gone(journal(&fixture)[0]["pid"].as_u64().unwrap());
}

#[test]
fn process_runner_pipe_cancel_after_real_parent_exit_stops_io_workers() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[GhStep::read(&["pipe-cancel"], "not a completed read")]);
    hold(&fixture, 0, 2_000);
    let cancel = CancellationToken::default();
    let token = cancel.clone();
    let spec = runner_spec(&fixture, &["pipe-cancel"]);
    let worker = thread::spawn(move || CommandRunner::default().run(spec, &token));
    let pid = helper_pid(&fixture);
    let deadline = Instant::now() + Duration::from_secs(1);
    let original = loop {
        if let Some(row) = journal(&fixture).first() {
            let original = row["pid"].as_u64().unwrap();
            if unsafe { libc::kill(original as i32, 0) } == -1 {
                assert_eq!(
                    std::io::Error::last_os_error().raw_os_error(),
                    Some(libc::ESRCH)
                );
                break original;
            }
        }
        assert!(
            Instant::now() < deadline,
            "actual direct child must be reaped before cancellation"
        );
        thread::yield_now();
    };
    assert_ne!(original, pid);
    let started = Instant::now();
    cancel.cancel();
    let result = worker.join().unwrap();
    let elapsed = started.elapsed();
    naturally_gone(pid);
    let error = result.unwrap_err();
    assert_eq!(error.kind, FailureKind::Cancelled);
    assert!(!error.outcome_unknown);
    assert!(
        elapsed < Duration::from_secs(1),
        "cancel waited for an external pipe holder: {elapsed:?}"
    );
    assert_eq!(journal(&fixture).len(), 1);
}

#[test]
fn process_runner_pipe_write_timeout_requires_fresh_inspection_and_explicit_ack() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[
        GhStep::read(&["write"], "accepted-but-incomplete"),
        GhStep::read(&["inspect"], "observed actual effect"),
        GhStep::read(&["write"], "explicit next write"),
    ]);
    hold(&fixture, 0, 2_000);
    let runner = CommandRunner::default();
    let result = runner.run(
        runner_spec(&fixture, &["write"])
            .write("pipe-resource")
            .timeout(Duration::from_millis(100)),
        &CancellationToken::default(),
    );
    let pid = helper_pid(&fixture);
    naturally_gone(pid);
    let error = result.unwrap_err();
    assert_eq!(error.kind, FailureKind::Timeout);
    assert!(error.outcome_unknown);
    let blocked = runner
        .run(
            runner_spec(&fixture, &["write"]).write("pipe-resource"),
            &CancellationToken::default(),
        )
        .unwrap_err();
    assert_eq!(blocked.kind, FailureKind::ReconciliationRequired);
    assert_eq!(
        journal(&fixture).len(),
        1,
        "no automatic or blocked duplicate child"
    );
    let generation = runner.resource_generation("pipe-resource");
    let observed = runner
        .run(
            runner_spec(&fixture, &["inspect"]).fresh_read(),
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(observed.stdout, b"observed actual effect");
    assert!(runner.mark_reconciled_generation("pipe-resource", generation));
    let next = runner
        .run(
            runner_spec(&fixture, &["write"]).write("pipe-resource"),
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(next.stdout, b"explicit next write");
    assert_eq!(journal(&fixture).len(), 3);
    assert!(journal(&fixture).iter().all(|row| row["matched"] == true));
}

#[test]
fn process_runner_pipe_normal_completion_preserves_raw_capture_and_separate_stderr() {
    let fixture = Fixture::new();
    let raw = b"raw\0\xff\x1b[31mcontent";
    let diagnostic = b"\x1b[31mseparate\x1b[0m\n";
    let mut step = GhStep::read(&["drain"], "");
    step.stdout = raw.to_vec();
    step.stderr = diagnostic.to_vec();
    fixture.scripted_gh(&[step]);
    hold(&fixture, 0, 100);
    let output = CommandRunner::default()
        .run(
            runner_spec(&fixture, &["drain"]).timeout(Duration::from_secs(2)),
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(output.stdout, raw);
    assert_eq!(output.stderr, b"separate\n");
    assert_eq!(output.capture.stdout_bytes(), raw.len());
    assert_eq!(output.capture.stderr_bytes(), diagnostic.len());
    assert_eq!(output.capture.total_bytes(), raw.len() + diagnostic.len());
    assert!(!output.stdout_text().contains('\x1b'));
    naturally_gone(helper_pid(&fixture));
    assert_eq!(journal(&fixture).len(), 1);
}
