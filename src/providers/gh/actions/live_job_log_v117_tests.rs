use super::*;

#[test]
fn live_job_log_v117_readable_lines_drop_timestamps_and_group_markers() {
    let raw = "\u{feff}2026-10-13T12:00:00.0000000Z ##[group]Run cargo test\n2026-10-13T12:00:01.0000000Z running 3 tests\r\n2026-10-13T12:00:02.0000000Z ##[endgroup]\n2026-10-13T12:00:03.0000000Z ##[error]boom\n";
    let log = job_log_from_raw_v117(raw.as_bytes(), 0);
    assert_eq!(
        log.text,
        "── Run cargo test ──\nrunning 3 tests\n##[error]boom\n"
    );
    assert_eq!(log.skipped_bytes, 0);
}

#[test]
fn live_job_log_v117_tail_drops_partial_first_line_and_counts_it() {
    let log = job_log_from_raw_v117(b"tial line\n2026-10-13T12:00:03.0000000Z kept\n", 100);
    assert_eq!(log.text, "kept\n");
    assert_eq!(log.skipped_bytes, 110);
}

#[test]
fn live_job_log_v117_refuses_running_job_without_a_request() {
    let provider = GithubProvider::new("github.invalid").unwrap();
    let job = job_from_json(&serde_json::json!({
        "id": 501, "run_id": 123, "run_attempt": 1, "status": "in_progress"
    }));
    let error = provider
        .job_log_v117("example/repo", &job, &CancellationToken::default())
        .unwrap_err();
    assert_eq!(error.message, LIVE_LOG_PENDING_V117);
}
