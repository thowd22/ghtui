//! TASK-66 `performance`: a 20 MB Actions job log stays bounded and the interface stays
//! responsive while it loads. The strict fake gh holds the log request for a fixed delay, so a
//! Help keypress answered before that delay ends was provably served while the load ran. The
//! log exceeds the 4 MiB command output cap: the application refuses it with a visible reason
//! instead of buffering it, and its memory does not grow by the log's size.
use super::*;
use crate::performance::{
    measure, perf_lock, proc_sample, Report, INPUT_TO_RENDER_P95_MS, RENDER_P95_MS,
};
use ghtui::render_timings_v66 as timings;
use std::time::Instant;

const LOG_BYTES: usize = 20 * 1024 * 1024;
const LOG_DELAY_MS: u64 = 3000;
/// The application may not keep more than this much extra memory for a refused 20 MB log.
const LOG_RSS_GROWTH_CEILING_KIB: u64 = 64 * 1024;
const HELP_TITLE: &str = "Help • keyboard and mouse";

fn large_log() -> Vec<u8> {
    let mut log = Vec::with_capacity(LOG_BYTES + 256);
    let mut line = 0usize;
    while log.len() < LOG_BYTES {
        log.extend_from_slice(
            format!(
                "matrix test linux\tRun tests\t2026-10-13T12:00:01.0000000Z perf log line {line:08} {}\n",
                "y".repeat(40)
            )
            .as_bytes(),
        );
        line += 1;
    }
    log
}

fn journey(mouse: bool) {
    let _lock = perf_lock();
    let name = format!(
        "performance_v66_actions_log_{}",
        if mouse { "mouse" } else { "keyboard" }
    );
    let mut report = Report::new(&name);
    let fixture = fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let status = fixture.git(&["status", "--porcelain=v2"]);
    let mut steps = initial();
    steps.extend(open_steps());
    let mut log = GhStep::read(
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
    log.stdout = large_log();
    log.delay_ms = LOG_DELAY_MS;
    steps.push(log);
    let loaded = steps.len();
    fixture.scripted_gh(&steps);
    let render = fixture.root.join("render.txt");

    let mut session = Session::launch_binary(
        std::path::Path::new(env!("CARGO_BIN_EXE_ghtui")),
        &fixture,
        120,
        40,
        &[(timings::ENV, render.to_str().unwrap())],
    );
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "Actions", b"4");
    session.wait_for_screen(&run_cell(123, 2));
    if mouse {
        session.click_visible_action(&run_cell(123, 2));
    } else {
        session.send_key(b"\r");
    }
    wait_row(&mut session, &["matrix test linux", "501"]);
    session.wait_for_settled_footer();
    let pid = session.owned_application_pid();
    let before = proc_sample(pid);

    // The log request starts (the fake gh journals it before its delay) ...
    choose(&mut session, mouse, "[l Logs]", b"l");
    session.wait_for_command_count(&fixture, loaded);
    let request_seen = Instant::now();
    // ... and a keypress made now is answered while gh is still holding the log.
    let help = measure(
        &mut session,
        "Help while a 20 MB log loads",
        std::time::Duration::from_secs(10),
        |session| session.choose(mouse, "[? Help]", b"?"),
        |frame| frame.contents().contains(HELP_TITLE),
    );
    let served_after_request = request_seen.elapsed();
    report.budget(
        "input_to_render_while_log_loads",
        help,
        INPUT_TO_RENDER_P95_MS,
    );
    report.require(
        "keypress_served_during_load",
        served_after_request < std::time::Duration::from_millis(LOG_DELAY_MS),
        format!(
            "Help painted {} ms after the log request started (gh holds it {LOG_DELAY_MS} ms)",
            served_after_request.as_millis()
        ),
    );
    session.choose(mouse, "[Esc Close]", b"\x1b");
    session.wait_for_overlay_close(HELP_TITLE);

    // The oversized log is refused with its reason, after the delay, without buffering it.
    if let Err(error) = session.try_wait_for(
        "exceeded its bounded capture",
        std::time::Duration::from_millis(LOG_DELAY_MS + 15_000),
    ) {
        session.save_artifacts();
        panic!("{error}; artifacts: {}", session.artifact_dir.display());
    }
    report.record(
        "log_outcome_after_request_ms",
        json!(request_seen.elapsed().as_millis() as u64),
    );
    session.wait_for_settled_footer();
    let after = proc_sample(pid);
    let growth = after.rss_kib.saturating_sub(before.rss_kib);
    report.record(
        "log_memory",
        json!({"log_bytes": LOG_BYTES, "rss_before_kib": before.rss_kib,
            "rss_after_kib": after.rss_kib, "peak_rss_kib": after.peak_rss_kib,
            "peak_growth_kib": after.peak_rss_kib.saturating_sub(before.peak_rss_kib)}),
    );
    report.require(
        "log_memory_bounded",
        growth <= LOG_RSS_GROWTH_CEILING_KIB
            && after.peak_rss_kib.saturating_sub(before.peak_rss_kib) <= LOG_RSS_GROWTH_CEILING_KIB,
        format!(
            "RSS grew {growth} KiB, peak grew {} KiB (ceiling {LOG_RSS_GROWTH_CEILING_KIB} KiB)",
            after.peak_rss_kib.saturating_sub(before.peak_rss_kib)
        ),
    );

    // Exactly the scripted reads ran, each matched; nothing was written to GitHub or Git.
    let records: Vec<Value> = session
        .inspect_command_journal(&fixture)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    report.require(
        "strict_gh_journal",
        records.len() == loaded && records.iter().all(|record| record["matched"] == true),
        format!(
            "{} of {loaded} scripted gh reads, all matched",
            records.len()
        ),
    );
    session.quit_and_assert_restored_terminal(mouse);
    report.p95(
        "render_cost_120x40",
        &timings::parse(&std::fs::read_to_string(&render).unwrap())
            .unwrap()
            .0
            .into_iter()
            .filter(|sample| (sample.cols, sample.rows) == (120, 40))
            .map(|sample| sample.micros)
            .collect::<Vec<_>>(),
        RENDER_P95_MS,
    );
    report.require(
        "git_unchanged",
        fixture.git(&["rev-parse", "HEAD"]) == head
            && fixture.git(&["status", "--porcelain=v2"]) == status,
        "HEAD and status unchanged".into(),
    );
    report.finish(json!({"actions_log_bytes": LOG_BYTES, "gh_delay_ms": LOG_DELAY_MS}));
}

#[test]
fn performance_v66_keyboard_actions_20mb_log_bounded_and_responsive() {
    journey(false)
}
#[test]
fn performance_v66_mouse_actions_20mb_log_bounded_and_responsive() {
    journey(true)
}
