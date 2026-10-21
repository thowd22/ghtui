//! TASK-107: bounded log tails and streamed full-log export.
use super::*;
use crate::providers::runner::{CommandRunner, CommandSpec, FailureKind, StdoutSinkV107};

const MIB: usize = 1024 * 1024;

fn raw_line(job: &str, step: &str, n: usize) -> String {
    format!("{job}\t{step}\t2026-10-13T12:00:01.0000000Z log line {n:08} \u{1b}[31mred\u{1b}[0m\n")
}
fn large_fixture(lines: usize) -> String {
    (0..lines)
        .map(|n| raw_line(if n % 2 == 0 { "build" } else { "test" }, "Run", n))
        .collect()
}
/// A real child that writes `bytes` bytes of log lines; nothing comes from the network.
fn writer_spec(bytes: usize, limit: usize, sink: StdoutSinkV107) -> CommandSpec {
    let mut spec = CommandSpec::new("sh")
        .args(["-c", &format!("yes 'tail line' | head -c {bytes}")])
        .fresh_read()
        .timeout(std::time::Duration::from_secs(60));
    spec.output_limit = limit;
    spec.stdout_sink_v107 = sink;
    spec
}

#[test]
fn actions_log_tail_v107_partial_first_line_is_dropped_and_counted() {
    let raw = format!("{}{}", "ial line\n", raw_line("build", "Run", 7));
    let tail = run_log_tail_from_raw(raw.as_bytes(), 100);
    assert_eq!(tail.skipped_bytes, 100 + "ial line\n".len() as u64);
    assert_eq!(tail.text, "── Run ──\nlog line 00000007 red\n");
    // A complete log keeps every line and reports nothing skipped.
    let whole = run_log_tail_from_raw(raw.as_bytes(), 0);
    assert_eq!(whole.skipped_bytes, 0);
    assert!(whole.text.starts_with("ial line\n"));
}

#[test]
fn actions_log_tail_v107_size_labels_are_readable() {
    assert_eq!(log_size_label(512), "512 B");
    assert_eq!(log_size_label(16 * MIB as u64), "16.0 MiB");
    assert_eq!(log_size_label(1536), "1.5 KiB");
}

#[test]
fn actions_log_tail_v107_streamed_export_matches_loaded_rendering() {
    for raw in [
        large_fixture(5_000),
        (0..300)
            .map(|n| raw_line("build", "Run", n))
            .collect::<String>(),
        "plain line\r\nbuild\tUNKNOWN STEP\t\u{feff}2026-10-09T14:22:28.9Z first\n".to_owned(),
    ] {
        let expected = redact(&sanitize_terminal(&readable_run_log(&raw)), &[]);
        let mut out = Vec::new();
        let lines =
            stream_readable_run_log(&mut std::io::Cursor::new(raw.as_bytes()), &mut out).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), expected);
        assert_eq!(lines, expected.lines().count() as u64);
    }
}

#[test]
fn actions_log_tail_v107_runner_tail_sink_keeps_only_the_last_bytes() {
    let total = 20 * MIB;
    let output = CommandRunner::default()
        .run(
            writer_spec(total, MIB, StdoutSinkV107::Tail),
            &Default::default(),
        )
        .unwrap();
    // Memory holds exactly the bound; everything before it is counted, not kept.
    assert_eq!(output.stdout.len(), MIB);
    assert!(output.stdout.capacity() <= MIB + 8192);
    assert_eq!(output.stdout_skipped_bytes, (total - MIB) as u64);
    assert!(output.stdout.ends_with(b"tail line\n"));
    let tail = run_log_tail_from_raw(&output.stdout, output.stdout_skipped_bytes);
    assert!(tail.skipped_bytes > (total - MIB) as u64);
    assert!(tail.text.lines().all(|line| line == "tail line"));
    assert_eq!(tail.text.len() as u64 + tail.skipped_bytes, total as u64);
    // The default bounded sink still refuses the same output.
    let refused = CommandRunner::default()
        .run(
            writer_spec(total, MIB, StdoutSinkV107::Bounded),
            &Default::default(),
        )
        .unwrap_err();
    assert_eq!(refused.kind, FailureKind::OutputLimit);
}

#[test]
fn actions_log_tail_v107_runner_file_sink_streams_everything_to_disk() {
    let total = 20 * MIB;
    let file = tempfile::tempfile().unwrap();
    let output = CommandRunner::default()
        .run(
            writer_spec(
                total,
                64 * MIB,
                StdoutSinkV107::File(std::sync::Arc::new(file.try_clone().unwrap())),
            ),
            &Default::default(),
        )
        .unwrap();
    assert!(output.stdout.is_empty(), "nothing is held in memory");
    assert_eq!(file.metadata().unwrap().len(), total as u64);
    // The disk bound still applies.
    let small = tempfile::tempfile().unwrap();
    let over = CommandRunner::default()
        .run(
            writer_spec(total, MIB, StdoutSinkV107::File(std::sync::Arc::new(small))),
            &Default::default(),
        )
        .unwrap_err();
    assert_eq!(over.kind, FailureKind::OutputLimit);
}
