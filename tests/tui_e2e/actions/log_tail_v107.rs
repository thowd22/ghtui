//! TASK-107: a job log larger than the 4 MiB capture shows its LAST part with a truncation
//! notice (how much was skipped, and that Export saves the full log) at 140x40 and 80x24.
//! The notice row opens Export by Enter and by click; Export re-reads the whole log from the
//! strict fake gh and streams it to a new file. No GitHub write, no Git change.
use super::*;

const CAP: usize = 4 * 1024 * 1024;
const LOG_BYTES: usize = 6 * 1024 * 1024;

fn raw_log() -> (Vec<u8>, usize) {
    let mut log = Vec::with_capacity(LOG_BYTES + 256);
    log.extend_from_slice(
        b"matrix test linux\tRun tests\t2026-10-13T12:00:00.0000000Z FIRST LINE MARKER\n",
    );
    let mut lines = 1;
    while log.len() < LOG_BYTES {
        log.extend_from_slice(
            format!(
                "matrix test linux\tRun tests\t2026-10-13T12:00:01.0000000Z tail log line {lines:08} {}\n",
                "z".repeat(30)
            )
            .as_bytes(),
        );
        lines += 1;
    }
    log.extend_from_slice(
        b"matrix test linux\tRun tests\t2026-10-13T12:00:02.0000000Z ##[error]LAST LINE MARKER\n",
    );
    (log, lines + 1)
}
/// What the view must report: bytes before the first whole line of the kept 4 MiB tail,
/// and that line's readable text.
fn expected_tail(raw: &[u8]) -> (String, String) {
    let cut = raw.len() - CAP;
    let newline = raw[cut..].iter().position(|b| *b == b'\n').unwrap();
    let skipped = (cut + newline + 1) as u64;
    let rest = &raw[cut + newline + 1..];
    let first = String::from_utf8_lossy(&rest[..rest.iter().position(|b| *b == b'\n').unwrap()])
        .to_string();
    let text = first.rsplit_once("Z ").unwrap().1.to_owned();
    (ghtui::providers::gh::log_size_label(skipped), text)
}
fn log_step(raw: &[u8]) -> GhStep {
    let mut step = GhStep::read(
        &[
            "run",
            "view",
            "123",
            "--repo",
            "github.invalid/example/repo",
            "--attempt",
            "2",
            "--log",
            "--job",
            "501",
        ],
        "",
    );
    step.stdout = raw.to_vec();
    step
}

#[test]
fn v1_actions_logs_keyboard_large_log_shows_tail_and_exports_full_log() {
    journey(false)
}
#[test]
fn v1_actions_logs_mouse_large_log_shows_tail_and_exports_full_log() {
    journey(true)
}

fn journey(mouse: bool) {
    let fixture = fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let status = fixture.git(&["status", "--porcelain=v2"]);
    let (raw, raw_lines) = raw_log();
    let (skipped, first_kept) = expected_tail(&raw);
    assert_eq!(skipped, "2.0 MiB");
    let notice = format!("▲ Truncated: first {skipped} skipped");
    let mut steps = initial();
    steps.extend(open_steps());
    steps.push(log_step(&raw));
    let loaded = steps.len();
    steps.extend(compose_identity_steps());
    let opened = steps.len();
    // Export re-reads the whole log between the principal checks around the effect.
    steps.extend(current_account());
    steps.push(log_step(&raw));
    steps.extend(current_account());
    let exported = steps.len();
    fixture.scripted_gh(&steps);
    let exports = fixture.root.join("exports");
    std::fs::create_dir_all(&exports).unwrap();
    let destination = exports.join("run-123-job-501-full.log");

    let mut session = Session::launch(&fixture, 140, 40);
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "Actions", b"4");
    session.wait_for_screen(&run_cell(123, 2));
    if mouse {
        session.click_visible_action(&run_cell(123, 2));
    } else {
        session.send_key(b"\r");
    }
    wait_row(&mut session, &["matrix test linux", "501"]);
    choose(&mut session, mouse, "[l Logs]", b"l");
    // The tail is shown, led by the notice; the skipped head is not.
    session.wait_for_screen(&format!("{notice} • x Export log saves the full log"));
    session.wait_for_screen(&first_kept);
    session.wait_for_control("[x Export log]");
    assert!(!session.screen().contains("FIRST LINE MARKER"));
    assert!(!session.screen().contains("exceeded its bounded capture"));
    journal(&mut session, &fixture, loaded);
    if !mouse {
        // The very end of the log is there.
        session.send_key(b"\x1b[F");
        session.wait_for_screen("##[error]LAST LINE MARKER");
        session.send_key(b"\x1b[H");
        session.wait_for_screen(&notice);
    }
    // At 80x24 the notice still states what was skipped.
    session.resize(80, 24);
    session.wait_for_screen(&notice);
    session.resize(140, 40);
    session.wait_for_screen(&notice);
    session.wait_for_control("[x Export log]");

    // The notice row opens Export: Enter on it, or a click on it.
    if mouse {
        session.click_visible_action(&notice);
    } else {
        session.send_key(b"\r");
    }
    session.wait_for_screen("ACTIONS INTENT");
    session.wait_for_screen("New absolute destination (never overwrites)");
    session.paste(destination.to_str().unwrap());
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    session.wait_for_screen("Only the end of this log is loaded");
    journal(&mut session, &fixture, opened);
    assert!(!destination.exists(), "preview never writes the file");
    confirm(&mut session, mouse, 1);
    session.wait_for_screen("Full log for run 123 attempt 2 job 501");
    let rows = journal(&mut session, &fixture, exported);

    // The file holds the WHOLE readable log, not the 4 MiB tail.
    let written = std::fs::read_to_string(&destination).unwrap();
    let lines: Vec<&str> = written.lines().collect();
    assert_eq!(
        lines.len(),
        raw_lines + 1,
        "one step header plus every line"
    );
    assert_eq!(lines[0], "── Run tests ──");
    assert_eq!(lines[1], "FIRST LINE MARKER");
    assert_eq!(*lines.last().unwrap(), "##[error]LAST LINE MARKER");
    // Readable lines drop gh's job/step/timestamp prefix, so the file is smaller than the raw
    // 6 MiB; what proves it is the full log is the skipped head: every line before the tail.
    assert!(written.contains("\ntail log line 00000001 "));
    let skipped_head = lines
        .iter()
        .position(|line| *line == first_kept)
        .expect("the first line the view kept");
    assert!(
        skipped_head > 10_000,
        "{skipped_head} lines before the kept tail"
    );
    assert!(!written.contains("2026-10-13T12:00:01"));
    assert_eq!(std::fs::read_dir(&exports).unwrap().count(), 1);
    // Reads only: the log twice and the principal; nothing written to GitHub or Git.
    assert!(rows
        .iter()
        .all(|row| row["args"].as_array().is_some_and(|args| !args
            .iter()
            .any(|arg| matches!(arg.as_str(), Some("POST" | "PUT" | "PATCH" | "DELETE"))))));
    assert_eq!(
        rows.iter()
            .filter(|row| row["args"]
                .as_array()
                .is_some_and(|args| args.iter().any(|arg| arg == "--log")))
            .count(),
        2
    );
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.git(&["status", "--porcelain=v2"]), status);
    session.quit_and_assert_restored_terminal(mouse);
}
