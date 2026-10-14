//! Runner metadata regressions for raw kept-byte accounting.
use super::*;

#[test]
fn raw_capture_accounting_keeps_control_only_stderr_count_even_when_sanitized_empty() {
    let fixture = Fixture::new();
    let stderr = b"\x1b[31m\x1b[0m\x01\x02".to_vec();
    fixture.scripted_gh(&[GhStep {
        args: vec!["capture-controls".into()],
        stdout: b"payload".to_vec(),
        stderr: stderr.clone(),
        stdin: vec![],
        status: 0,
        delay_ms: 0,
    }]);
    let output = CommandRunner::default()
        .run(
            runner_spec(&fixture, &["capture-controls"]),
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(output.stdout, b"payload");
    assert!(output.stderr.is_empty());
    assert_eq!(output.capture.stdout_bytes(), 7);
    assert_eq!(output.capture.stderr_bytes(), stderr.len());
    assert_eq!(output.capture.total_bytes(), 7 + stderr.len());
    assert_eq!(journal(&fixture).len(), 1);
    assert_eq!(journal(&fixture)[0]["matched"], true);
}

#[test]
fn raw_capture_accounting_is_kept_bytes_before_multibyte_redaction_not_display_length() {
    let fixture = Fixture::new();
    let raw_stdout = b"private-value".to_vec();
    let stderr = "\x1b[31mé界\x1b[0m ghp_fixture_secret".as_bytes().to_vec();
    fixture.scripted_gh(&[GhStep {
        args: vec!["capture-redaction".into()],
        stdout: raw_stdout.clone(),
        stderr: stderr.clone(),
        stdin: vec![],
        status: 0,
        delay_ms: 0,
    }]);
    let mut spec = runner_spec(&fixture, &["capture-redaction"]);
    spec.sensitive_values = vec!["private-value".into()];
    let output = CommandRunner::default()
        .run(spec, &CancellationToken::default())
        .unwrap();
    assert_eq!(output.stdout, b"[REDACTED]");
    assert_eq!(output.capture.stdout_bytes(), raw_stdout.len());
    assert_eq!(output.capture.stderr_bytes(), stderr.len());
    assert_eq!(
        output.capture.total_bytes(),
        raw_stdout.len() + stderr.len()
    );
    assert_ne!(
        output.capture.total_bytes(),
        output.stdout.len() + output.stderr.len()
    );
    assert_eq!(output.stderr_text(), "é界 [REDACTED]");
    assert!(!output.stdout_text().contains("private-value"));
    assert_eq!(journal(&fixture)[0]["matched"], true);
}

#[test]
fn raw_capture_accounting_shared_dual_pipe_limit_cannot_return_success_metadata() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[GhStep {
        args: vec!["capture-overflow".into()],
        stdout: "é".repeat(3000).into_bytes(),
        stderr: "界".repeat(3000).into_bytes(),
        stdin: vec![],
        status: 0,
        delay_ms: 0,
    }]);
    let mut spec = runner_spec(&fixture, &["capture-overflow"]);
    spec.output_limit = 1024;
    let error = CommandRunner::default()
        .run(spec, &CancellationToken::default())
        .unwrap_err();
    assert!(matches!(error.kind, FailureKind::OutputLimit));
    assert!(!error.outcome_unknown);
    assert_eq!(journal(&fixture).len(), 1);
    assert_eq!(journal(&fixture)[0]["matched"], true);
}

#[test]
fn raw_capture_accounting_cancel_and_deadline_never_return_a_success_receipt() {
    let fixture = Fixture::new();
    let token = CancellationToken::default();
    token.cancel();
    let err = CommandRunner::default()
        .run(runner_spec(&fixture, &["no-launch"]), &token)
        .unwrap_err();
    assert!(matches!(err.kind, FailureKind::Cancelled));
    assert!(journal(&fixture).is_empty());
    fixture.scripted_gh(&[GhStep {
        args: vec!["capture-deadline".into()],
        stdout: b"not-yet".to_vec(),
        stderr: vec![],
        stdin: vec![],
        status: 0,
        delay_ms: 1000,
    }]);
    let mut spec = runner_spec(&fixture, &["capture-deadline"]);
    spec.timeout = Duration::from_millis(250);
    let err = CommandRunner::default()
        .run(spec, &CancellationToken::default())
        .unwrap_err();
    assert!(matches!(err.kind, FailureKind::Timeout));
    assert_eq!(journal(&fixture).len(), 1);
    assert_eq!(journal(&fixture)[0]["matched"], true);
}
