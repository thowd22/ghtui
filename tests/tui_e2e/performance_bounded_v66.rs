//! TASK-66 AC3 `performance`: large work stays bounded and the interface stays responsive
//! while it runs. Two real-binary PTY journeys, each by keyboard and by mouse, judged with the
//! same report, budgets and build factor as `performance.rs` (see `docs/performance.md`):
//!
//! - **Many avatars.** An Inbox of 150 PRs by 150 different authors, each with its own avatar
//!   URL. The image transport (an offline fake `curl`) holds every fetch for a second. The
//!   application may run at most 4 fetches at once and requests at most 128 actors; each URL
//!   is fetched once; the on-disk cache keeps at most 128 records; and moving through the list
//!   while the fetches run is answered within the input-to-render budget.
//! - **History scrolled while loading.** The 20,000-commit repository's History is scrolled
//!   past its first 200 commits, which reads the next page. A Git wrapper holds that read for
//!   4 s. Scrolling on past the boundary while it loads starts no second read, and scrolling
//!   back through the loaded rows is answered within budget while the read is still held.
use crate::performance::{
    budget_factor, fixture_shape, idle_window, large_fixture, large_repo, launch_perf, measure,
    micros, perf_binary, perf_lock, proc_sample, render_samples, wait_history_at, Report,
    COLD_LARGE_LOAD_CEILING_MS, COMMITS, INPUT_TO_RENDER_P95_MS, PEAK_RSS_CEILING_KIB,
    RENDER_P95_MS,
};
use crate::support::{
    history_fixture::{history_loaded_count, history_pane, history_ready, history_rows, RepoState},
    selected_row, view_title, Fixture, GhStep, Session,
};
use ghtui::{config::Config, render_timings_v66 as timings};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    fs,
    path::Path,
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

// ---------------------------------------------------------------------------------------
// Journey A: many distinct avatars.
// ---------------------------------------------------------------------------------------

/// PRs in the Inbox, each by its own author with its own avatar URL (three queues of 50).
const AVATAR_PRS: usize = 150;
const AVATAR_QUEUE: usize = 50;
/// The application's concurrent avatar fetch bound (`identity::MAX_WORKERS`).
const AVATAR_WORKERS: usize = 4;
/// The actors one surface requests images for (`actor_avatar_projection_v1::ROWS`) and the
/// records the avatar cache keeps (`identity::MAX_CACHE_RECORDS`).
const AVATAR_ROWS: usize = 128;
/// Every fake fetch holds this long, as a slow network would.
const AVATAR_HOLD_SECONDS: u64 = 1;
const AVATAR_HOST: &str = "avatars.githubusercontent.com";

fn avatar_title(index: usize) -> String {
    format!("Avatar load PR {index:03}")
}

fn avatar_pr(index: usize) -> Value {
    // Newest first in index order: one minute apart.
    let minutes = 23 * 60 + 59 - index;
    json!({
        "node_id": format!("PR_AVATAR_{index:03}"),
        "number": index + 1,
        "title": avatar_title(index),
        "body": "Avatar load fixture",
        "repository_url": "https://api.github.com/repos/org/avatars",
        "html_url": format!("https://github.com/org/avatars/pull/{}", index + 1),
        "user": {
            "login": format!("author{index:03}"),
            "type": "User",
            "avatar_url": format!("https://{AVATAR_HOST}/u/{}?v=4", 7000 + index),
        },
        "state": "open",
        "draft": false,
        "created_at": "2026-10-01T00:00:00Z",
        "updated_at": format!("2026-10-13T{:02}:{:02}:00Z", minutes / 60, minutes % 60),
    })
}

/// Startup identity, the three Inbox queues (50 distinct PRs each) and the rollup batches.
fn avatar_steps() -> Vec<GhStep> {
    let mut steps = crate::support::empty_inbox_steps("octocat");
    for (queue, step) in steps.iter_mut().skip(4).enumerate() {
        let items: Vec<Value> = (queue * AVATAR_QUEUE..(queue + 1) * AVATAR_QUEUE)
            .map(avatar_pr)
            .collect();
        step.stdout = serde_json::to_vec(
            &json!({"total_count": AVATAR_QUEUE, "incomplete_results": false, "items": items}),
        )
        .unwrap();
    }
    let rollup = crate::support::inbox_rollup_unknown_for("github.com", &steps);
    steps.extend(rollup);
    steps
}

fn png_bytes() -> Vec<u8> {
    use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
    let pixels = RgbaImage::from_fn(2, 2, |x, _| {
        if x == 0 {
            Rgba([0, 160, 80, 255])
        } else {
            Rgba([240, 200, 0, 255])
        }
    });
    let mut encoded = std::io::Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(pixels)
        .write_to(&mut encoded, ImageFormat::Png)
        .unwrap();
    encoded.into_inner()
}

/// The offline image transport: it records each URL and how many fetches were running when it
/// started (itself included), holds the fetch, records completion and prints a real PNG.
fn install_slow_curl(fixture: &Fixture) -> std::path::PathBuf {
    let run = fixture.root.join("avatar-run");
    fs::create_dir_all(run.join("running")).unwrap();
    let image = run.join("avatar.png");
    fs::write(&image, png_bytes()).unwrap();
    let script = format!(
        "#!/bin/sh\nd='{run}'\nurl=''\nprev=''\nfor a in \"$@\"; do if [ \"$prev\" = '--url' ]; then url=\"$a\"; fi; prev=\"$a\"; done\n\
         : > \"$d/running/$$\"\nn=$(/bin/ls \"$d/running\" | /usr/bin/wc -l)\n\
         printf '%s %s\\n' \"$n\" \"$url\" >> \"$d/starts\"\n/bin/sleep {AVATAR_HOLD_SECONDS}\n\
         /bin/rm -f \"$d/running/$$\"\nprintf '%s\\n' \"$url\" >> \"$d/done\"\n/bin/cat '{image}'\n",
        run = run.display(),
        image = image.display(),
    );
    crate::fixture_shell_publication_v1::install(&fixture.root.join("bin/curl"), script.as_bytes())
        .unwrap();
    run
}

fn lines(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// Live children of `pid` whose command line fetches a public avatar (the fixture transport
/// runs as `/bin/sh <script> … --url https://avatars…`).
fn avatar_children(pid: i32) -> usize {
    let Ok(tasks) = fs::read_dir(format!("/proc/{pid}/task")) else {
        return 0;
    };
    tasks
        .flatten()
        .filter_map(|task| fs::read_to_string(task.path().join("children")).ok())
        .flat_map(|children| {
            children
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .filter(|child| {
            fs::read(format!("/proc/{child}/cmdline"))
                .is_ok_and(|cmdline| String::from_utf8_lossy(&cmdline).contains(AVATAR_HOST))
        })
        .count()
}

/// Samples the application's live avatar children every millisecond until stopped.
struct ChildSampler {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<(usize, usize)>,
}

impl ChildSampler {
    fn start(pid: i32) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let handle = std::thread::spawn(move || {
            let (mut peak, mut samples) = (0, 0);
            while !flag.load(Ordering::Relaxed) {
                peak = peak.max(avatar_children(pid));
                samples += 1;
                std::thread::park_timeout(Duration::from_millis(1));
            }
            (peak, samples)
        });
        Self { stop, handle }
    }

    /// `(peak live avatar children, samples taken)`.
    fn finish(self) -> (usize, usize) {
        self.stop.store(true, Ordering::Relaxed);
        self.handle.thread().unpark();
        self.handle.join().unwrap()
    }
}

/// The index of the PR on the Inbox's selected row.
fn selected_avatar_pr(frame: &vt100::Screen) -> Option<usize> {
    selected_title_index(&selected_row(frame, &view_title("Inbox"))?)
}

/// The first PR row the Inbox list shows, with its screen position. A list row holds the PR
/// reference left of its title (the preview's title line has no reference before it).
fn top_avatar_row(frame: &vt100::Screen) -> Option<(usize, (u16, u16))> {
    let contents = frame.contents();
    contents.lines().enumerate().find_map(|(y, line)| {
        let reference = line.find("org/avatars#")?;
        let at = line.find("Avatar load PR ")?;
        (reference < at).then_some(())?;
        let index = selected_title_index(&line[at..])?;
        let x = line[..at].chars().count() as u16;
        Some((index, (x, y as u16)))
    })
}

/// One input over the Inbox list, timed to the first completed frame that shows its effect.
/// The keyboard moves the selection one row (Down/Up). The mouse turns the wheel over the
/// list, which scrolls it (a click on a row would open that PR instead).
fn inbox_move(session: &mut Session, mouse: bool, down: bool) -> Duration {
    let frame = session.completed_cursorless_screen().unwrap();
    if mouse {
        let (top, (x, y)) = top_avatar_row(frame)
            .unwrap_or_else(|| panic!("no Inbox list row\n{}", session.screen()));
        return measure(
            session,
            &format!("Inbox wheel from top row {top}"),
            Duration::from_secs(10),
            |session| session.wheel(x, y, down),
            move |frame| top_avatar_row(frame).is_some_and(|(now, _)| now != top),
        );
    }
    let from = selected_avatar_pr(frame)
        .unwrap_or_else(|| panic!("no selected Inbox PR\n{}", session.screen()));
    let to = if down { from + 1 } else { from - 1 };
    measure(
        session,
        &format!("Inbox selection to {}", avatar_title(to)),
        Duration::from_secs(10),
        |session| {
            session.send_key(if down { b"\x1b[B" } else { b"\x1b[A" });
        },
        move |frame| selected_avatar_pr(frame) == Some(to),
    )
}

fn avatar_journey(mouse: bool) {
    let _lock = perf_lock();
    let name = format!(
        "performance_v66_avatars_{}",
        if mouse { "mouse" } else { "keyboard" }
    );
    let mut report = Report::new(&name);
    let fixture = Fixture::new();
    // No scheduled Inbox refresh during the journey: every gh call is scripted.
    Config {
        refresh_seconds: 3600,
        avatars: true,
        ..Config::default()
    }
    .save(&fixture.root.join("config/ghtui/config.toml"))
    .unwrap();
    let steps = avatar_steps();
    fixture.scripted_gh(&steps);
    let run = install_slow_curl(&fixture);
    let render = fixture.root.join("render-avatars.txt");
    let env = [(timings::ENV, render.to_str().unwrap())];

    let mut s = launch_perf(&fixture, &fixture.repo(), 120, 40, &env);
    s.wait_for_screen("ghtui");
    // The startup identity check finishes before the Inbox load is asked for (a refresh sent
    // while it still ran once failed with `database is locked`; see docs/performance.md).
    s.wait_for_screen("Use Refresh to load");
    s.wait_for_settled_footer();
    let pid = s.owned_application_pid();
    let sampler = ChildSampler::start(pid);
    s.choose(mouse, "[r Refresh]", b"r");
    s.wait_for_screen(&avatar_title(0));
    s.wait_for_command_count(&fixture, steps.len());
    s.wait_for_cursorless_condition(
        |frame| selected_avatar_pr(frame) == Some(0),
        "first Inbox PR selected",
        Duration::from_secs(10),
    );
    // The list is in index order (newest first), so neighbours are index ± 1.
    let screen = s.screen();
    let shown: Vec<usize> = (0..AVATAR_PRS)
        .filter(|index| screen.contains(&avatar_title(*index)))
        .collect();
    report.require(
        "inbox_order",
        shown.len() >= 8 && shown.windows(2).all(|pair| pair[1] == pair[0] + 1),
        format!("visible PRs {shown:?}"),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while lines(&run.join("starts")).is_empty() {
        assert!(
            Instant::now() < deadline,
            "no avatar fetch started\n{}",
            s.screen()
        );
        s.poll_output(Duration::from_millis(10));
    }

    // Move through the list and back while the avatars load.
    let moves = if mouse { 7 } else { 20 };
    let mut samples = Vec::new();
    for down in [true, false] {
        for _ in 0..moves {
            samples.push(micros(inbox_move(&mut s, mouse, down)));
        }
    }
    let done_during_input = lines(&run.join("done")).len();
    report.p95(
        "input_to_render_while_avatars_load_120x40",
        &samples,
        INPUT_TO_RENDER_P95_MS,
    );

    // 80x24: the same list, the same inputs, while fetches may still run.
    s.resize(80, 24);
    s.wait_for_cursorless_condition(
        |frame| selected_avatar_pr(frame) == Some(0),
        "Inbox at 80x24 keeps the selection",
        Duration::from_secs(10),
    );
    let mut narrow = Vec::new();
    for down in [true, false] {
        for _ in 0..3 {
            narrow.push(micros(inbox_move(&mut s, mouse, down)));
        }
    }
    report.p95(
        "input_to_render_while_avatars_load_80x24",
        &narrow,
        INPUT_TO_RENDER_P95_MS,
    );
    s.resize(120, 40);
    s.wait_for_cursorless_condition(
        |frame| selected_avatar_pr(frame) == Some(0),
        "Inbox back at 120x40",
        Duration::from_secs(10),
    );

    // Every requested avatar completes, one bounded batch at a time.
    let all_started = Instant::now();
    let deadline = all_started + Duration::from_secs(120);
    let mut stable_since: Option<(usize, Instant)> = None;
    loop {
        s.poll_output(Duration::from_millis(50));
        let started = lines(&run.join("starts")).len();
        let done = lines(&run.join("done")).len();
        let settled = started > 0 && started == done && avatar_children(pid) == 0;
        match stable_since {
            Some((count, since)) if settled && count == started => {
                if since.elapsed() >= Duration::from_millis(1500) {
                    break;
                }
            }
            _ if settled => stable_since = Some((started, Instant::now())),
            _ => stable_since = None,
        }
        assert!(
            Instant::now() < deadline,
            "avatar fetches never settled: {started} started, {done} done"
        );
    }
    let (sampled_peak, sample_count) = sampler.finish();
    let starts = lines(&run.join("starts"));
    let snapshot_peak = starts
        .iter()
        .filter_map(|line| line.split_whitespace().next()?.parse::<usize>().ok())
        .max()
        .unwrap_or(0);
    let urls: Vec<&str> = starts
        .iter()
        .filter_map(|line| line.split_whitespace().nth(1))
        .collect();
    let distinct: BTreeSet<&str> = urls.iter().copied().collect();
    let cache_records = fs::read_dir(fixture.root.join("config/ghtui/avatars"))
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.file_name().to_string_lossy().starts_with("avatar-"))
                .count()
        })
        .unwrap_or(0);
    report.record(
        "avatars",
        json!({"prs": AVATAR_PRS, "fetches": urls.len(), "distinct_urls": distinct.len(),
            "done_when_input_ended": done_during_input, "sampled_peak_children": sampled_peak,
            "child_samples": sample_count, "transport_peak_concurrency": snapshot_peak,
            "cache_records": cache_records, "hold_seconds": AVATAR_HOLD_SECONDS,
            "all_settled_after_input_ms": all_started.elapsed().as_millis() as u64}),
    );
    report.require(
        "avatar_concurrency_bounded",
        (1..=AVATAR_WORKERS).contains(&sampled_peak)
            && (2..=AVATAR_WORKERS).contains(&snapshot_peak),
        format!(
            "peak {sampled_peak} live avatar children over {sample_count} samples, \
             {snapshot_peak} fetches running at once by the transport (bound {AVATAR_WORKERS})"
        ),
    );
    report.require(
        "avatar_requests_bounded_and_deduplicated",
        distinct.len() == urls.len()
            && urls.len() <= AVATAR_ROWS
            && urls.len() >= AVATAR_ROWS / 2
            && urls
                .iter()
                .all(|url| url.starts_with("https://") && url.contains(AVATAR_HOST)),
        format!(
            "{} fetches of {} distinct URLs for {AVATAR_PRS} authors (bound {AVATAR_ROWS})",
            urls.len(),
            distinct.len()
        ),
    );
    report.require(
        "input_overlapped_avatar_loading",
        done_during_input < urls.len(),
        format!(
            "{done_during_input} of {} fetches done when the input ended",
            urls.len()
        ),
    );
    report.require(
        "avatar_cache_bounded",
        (1..=AVATAR_ROWS).contains(&cache_records),
        format!("{cache_records} cache records (bound {AVATAR_ROWS})"),
    );

    let end = proc_sample(pid);
    report.record(
        "resources_after_journey",
        json!({"rss_kib": end.rss_kib, "peak_rss_kib": end.peak_rss_kib,
            "threads": end.threads}),
    );
    report.require(
        "peak_rss",
        end.peak_rss_kib <= PEAK_RSS_CEILING_KIB,
        format!(
            "peak RSS {} KiB (ceiling {PEAK_RSS_CEILING_KIB} KiB)",
            end.peak_rss_kib
        ),
    );
    idle_window(
        &mut report,
        &mut s,
        "after_avatars",
        &fixture.root.join("no-git-trace"),
    );
    s.quit_and_assert_restored_terminal(mouse);
    report.p95(
        "render_cost_120x40",
        &render_samples(&render, 120, 40),
        RENDER_P95_MS,
    );
    let journal = s.inspect_command_journal(&fixture);
    report.require(
        "gh_calls_scripted",
        journal.lines().count() == steps.len()
            && journal.lines().all(|line| {
                serde_json::from_str::<Value>(line).is_ok_and(|record| record["matched"] == true)
            }),
        format!(
            "{} gh calls, {} scripted",
            journal.lines().count(),
            steps.len()
        ),
    );
    report.finish(
        json!({"inbox_prs": AVATAR_PRS, "distinct_authors": AVATAR_PRS,
        "avatar_hold_seconds": AVATAR_HOLD_SECONDS, "app": perf_binary()}),
    );
}

#[test]
fn performance_v66_keyboard_many_avatars_bounded_and_responsive() {
    avatar_journey(false)
}
#[test]
fn performance_v66_mouse_many_avatars_bounded_and_responsive() {
    avatar_journey(true)
}

// ---------------------------------------------------------------------------------------
// Journey B: the 20,000-commit History scrolled while its next page loads.
// ---------------------------------------------------------------------------------------

/// How long the wrapper holds History's continuation read (`git log --skip=…`).
const PAGE_HOLD_SECONDS: u64 = 4;
const FIRST_PAGE: usize = 200;

/// Wraps Git so that History's next-page read marks that it started and then waits before it
/// runs; every other Git command runs at once. Returns the marker's path.
fn install_slow_continuation_git(fixture: &Fixture) -> std::path::PathBuf {
    let found = Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .unwrap();
    assert!(found.status.success());
    let git = String::from_utf8(found.stdout).unwrap().trim().to_owned();
    let marker = fixture.root.join("continuation-held");
    let script = format!(
        "#!/bin/sh\ncase \" $* \" in *' --skip=0 '*) ;; *' --skip='*) : > '{marker}'; /bin/sleep {PAGE_HOLD_SECONDS};; esac\nexec '{git}' \"$@\"\n",
        marker = marker.display()
    );
    crate::fixture_shell_publication_v1::install(&fixture.root.join("bin/git"), script.as_bytes())
        .unwrap();
    marker
}

/// History continuation reads started so far (from the GIT_TRACE2_EVENT journal). The first
/// page is read with `--skip=0`; only later offsets are continuations.
fn continuation_reads(trace: &Path) -> Vec<String> {
    fs::read_to_string(trace)
        .unwrap_or_default()
        .lines()
        .filter(|line| line.contains("\"event\":\"start\"") && line.contains("\"--skip="))
        .map(|line| {
            let at = line.find("\"--skip=").unwrap() + 1;
            line[at..].split('"').next().unwrap_or_default().to_owned()
        })
        .filter(|skip| skip != "--skip=0")
        .collect()
}

fn history_loading(frame: &vt100::Screen) -> bool {
    let text = frame.contents();
    text.contains("Esc Cancel") || text.contains("Loading ")
}

/// What a scroll can change: the first shown row, the selected row and the loading state.
fn history_view(frame: &vt100::Screen) -> (Option<String>, Option<String>, bool) {
    let rows = history_rows(frame);
    (
        rows.first().map(|row| row.sha.clone()),
        rows.iter()
            .find(|row| row.selected)
            .map(|row| row.sha.clone()),
        history_loading(frame),
    )
}

/// One scroll event: PageDown/PageUp by keyboard, the wheel over the History table by mouse.
fn scroll(session: &mut Session, mouse: bool, down: bool) {
    if mouse {
        let frame = session.completed_cursorless_screen().unwrap();
        let (left, right, top, bottom) = history_pane(frame)
            .unwrap_or_else(|| panic!("History pane not shown\n{}", session.screen()));
        session.wheel((left + right) / 2, (top + bottom) / 2, down);
    } else if down {
        session.send_key(b"\x1b[6~");
    } else {
        session.send_key(b"\x1b[5~");
    }
}

/// Times one scroll to the first completed frame whose History view changed.
fn timed_scroll(session: &mut Session, mouse: bool, down: bool) -> Duration {
    let before = history_view(session.completed_cursorless_screen().unwrap());
    measure(
        session,
        if down {
            "History scroll down"
        } else {
            "History scroll up"
        },
        Duration::from_secs(10),
        |session| scroll(session, mouse, down),
        move |frame| history_view(frame) != before,
    )
}

fn history_scroll_journey(mouse: bool) {
    let _lock = perf_lock();
    let repo = large_repo();
    let fixture = large_fixture(repo);
    let held = install_slow_continuation_git(&fixture);
    let name = format!(
        "performance_v66_history_scroll_{}",
        if mouse { "mouse" } else { "keyboard" }
    );
    let mut report = Report::new(&name);
    let baseline = RepoState::capture(&fixture.repo());
    let trace = fixture.root.join("git-trace2.jsonl");
    let render = fixture.root.join("render-scroll.txt");
    let env = [
        ("GIT_TRACE2_EVENT", trace.to_str().unwrap()),
        (timings::ENV, render.to_str().unwrap()),
    ];
    let mut s = launch_perf(&fixture, &fixture.repo(), 120, 40, &env);
    s.wait_for_screen("Recent commits");
    s.wait_for_settled_footer();
    s.choose(mouse, "[g History]", b"g");
    wait_history_at(&mut s, &repo.shas[0]);
    s.wait_for_settled_footer();
    let first = history_loaded_count(&s.screen());
    report.require(
        "history_first_page_bounded",
        first == Some(FIRST_PAGE),
        format!("{first:?} commits loaded first (page {FIRST_PAGE})"),
    );

    // Scroll down through the loaded page until History starts reading the next one. A
    // commit detail read may also show loading; only the held page read ends the scrolling.
    let mut scrolling = Vec::new();
    let mut events = 0;
    while !held.exists() {
        assert!(
            events < 200,
            "History never read past its first page\n{}",
            s.screen()
        );
        scrolling.push(micros(timed_scroll(&mut s, mouse, true)));
        events += 1;
        let deadline = Instant::now() + Duration::from_secs(5);
        while !held.exists()
            && history_loading(s.completed_cursorless_screen().unwrap())
            && Instant::now() < deadline
        {
            s.poll_output(Duration::from_millis(5));
        }
    }
    report.record("scroll_events_to_next_page", json!(events));
    s.wait_for_cursorless_condition(
        history_loading,
        "History shows the next page loading",
        Duration::from_secs(5),
    );

    // While the read is held: more scrolling past the boundary starts no second read, and
    // scrolling back up through loaded rows is answered within budget.
    for _ in 0..3 {
        scroll(&mut s, mouse, true);
    }
    let mut during = Vec::new();
    for _ in 0..8 {
        if !history_loading(s.completed_cursorless_screen().unwrap()) {
            break;
        }
        let sample = timed_scroll(&mut s, mouse, false);
        // Git records the read's start only once the wrapper releases it: until then the
        // answered scroll happened while the page read was still held.
        if history_loading(s.completed_cursorless_screen().unwrap())
            && continuation_reads(&trace).is_empty()
        {
            during.push(micros(sample));
        }
    }
    report.require(
        "input_while_history_page_loads",
        during.len() >= 3,
        format!(
            "{} scrolls answered while the {PAGE_HOLD_SECONDS} s read was held",
            during.len()
        ),
    );
    report.p95(
        "input_to_render_while_history_page_loads",
        &during,
        INPUT_TO_RENDER_P95_MS,
    );
    let second = 2 * FIRST_PAGE;
    s.wait_for_cursorless_condition(
        |frame| {
            !history_loading(frame)
                && history_ready(frame)
                && history_loaded_count(&frame.contents()) == Some(second)
        },
        "the next History page loaded",
        Duration::from_millis(COLD_LARGE_LOAD_CEILING_MS),
    );
    s.wait_for_settled_footer();
    let reads = continuation_reads(&trace);
    report.require(
        "history_one_continuation_read",
        reads == [format!("--skip={FIRST_PAGE}")],
        format!("continuation reads {reads:?}"),
    );
    let loaded = history_loaded_count(&s.screen());
    report.require(
        "history_bounded_after_scroll",
        loaded.is_some_and(|count| count < COMMITS),
        format!("{loaded:?} of {COMMITS} commits loaded"),
    );
    report.p95(
        "history_scroll_input_to_render_120x40",
        &scrolling,
        INPUT_TO_RENDER_P95_MS,
    );

    // Where the table stands once the page has loaded (the window it shows and its bar).
    let (top, bar, _) = history_view(s.completed_cursorless_screen().unwrap());
    let position = |sha: &Option<String>| {
        sha.as_ref().and_then(|short| {
            repo.shas
                .iter()
                .position(|full| !short.is_empty() && full.starts_with(short.as_str()))
        })
    };
    report.record(
        "history_after_next_page",
        json!({"first_shown_row": position(&top), "selected_row": position(&bar)}),
    );

    // 80x24: scrolling through the loaded window stays as fast. The direction follows the
    // bar: down from the upper half of the 200-row window, up from the lower half.
    s.resize(80, 24);
    s.wait_for_cursorless_condition(history_ready, "History at 80x24", Duration::from_secs(20));
    let down = position(&bar).is_none_or(|row| row % FIRST_PAGE < FIRST_PAGE / 2);
    let mut narrow = Vec::new();
    for _ in 0..6 {
        narrow.push(micros(timed_scroll(&mut s, mouse, down)));
    }
    report.p95(
        "history_scroll_input_to_render_80x24",
        &narrow,
        INPUT_TO_RENDER_P95_MS,
    );
    s.resize(120, 40);
    s.wait_for_cursorless_condition(
        history_ready,
        "History back at 120x40",
        Duration::from_secs(20),
    );
    s.wait_for_settled_footer();

    let end = proc_sample(s.owned_application_pid());
    report.record(
        "resources_after_journey",
        json!({"rss_kib": end.rss_kib, "peak_rss_kib": end.peak_rss_kib,
            "threads": end.threads, "git_children_total": crate::performance::git_starts(&trace)}),
    );
    report.require(
        "peak_rss",
        end.peak_rss_kib <= PEAK_RSS_CEILING_KIB,
        format!(
            "peak RSS {} KiB (ceiling {PEAK_RSS_CEILING_KIB} KiB)",
            end.peak_rss_kib
        ),
    );
    idle_window(&mut report, &mut s, "after_history_scroll", &trace);
    s.quit_and_assert_restored_terminal(mouse);
    report.p95(
        "render_cost_120x40",
        &render_samples(&render, 120, 40),
        RENDER_P95_MS,
    );
    report.require(
        "read_only_git_state",
        RepoState::capture(&fixture.repo()) == baseline,
        "HEAD, refs, status, index and config unchanged".into(),
    );
    let journal = s.inspect_command_journal(&fixture);
    report.require(
        "no_github_calls",
        journal.is_empty(),
        format!("gh journal: {journal:?}"),
    );
    let mut shape = fixture_shape(repo);
    shape["continuation_hold_seconds"] = json!(PAGE_HOLD_SECONDS);
    shape["budget_factor"] = json!(budget_factor());
    report.finish(shape);
}

#[test]
fn performance_v66_keyboard_history_scrolled_while_next_page_loads() {
    history_scroll_journey(false)
}
#[test]
fn performance_v66_mouse_history_scrolled_while_next_page_loads() {
    history_scroll_journey(true)
}

// ---------------------------------------------------------------------------------------
// Code tests: the fixtures these journeys rely on.
// ---------------------------------------------------------------------------------------

#[test]
fn performance_v66_avatar_fixture_has_distinct_authors_and_urls() {
    let steps = avatar_steps();
    let mut logins = BTreeSet::new();
    let mut urls = BTreeSet::new();
    let mut prs = 0;
    for step in steps
        .iter()
        .filter(|step| step.args.get(1).is_some_and(|arg| arg == "search/issues"))
    {
        let page: Value = serde_json::from_slice(&step.stdout).unwrap();
        for item in page["items"].as_array().unwrap() {
            prs += 1;
            logins.insert(item["user"]["login"].as_str().unwrap().to_owned());
            urls.insert(item["user"]["avatar_url"].as_str().unwrap().to_owned());
        }
    }
    assert_eq!(
        (prs, logins.len(), urls.len()),
        (AVATAR_PRS, AVATAR_PRS, AVATAR_PRS)
    );
    // More authors than one surface requests images for.
    const { assert!(AVATAR_PRS > AVATAR_ROWS) };
    // One rollup read per 50 PRs follows the three queues.
    let rollups = steps
        .iter()
        .filter(|step| step.args.get(1).is_some_and(|arg| arg == "graphql"))
        .count();
    assert_eq!(rollups, AVATAR_PRS.div_ceil(50));
    assert_eq!(
        selected_title_index("▌ Avatar load PR 042 · org/avatars"),
        Some(42)
    );
}

fn selected_title_index(row: &str) -> Option<usize> {
    let at = row.find("Avatar load PR ")? + "Avatar load PR ".len();
    row.get(at..at + 3)?.parse().ok()
}

#[test]
fn performance_v66_continuation_reads_parse_trace_argv() {
    let directory = tempfile::tempdir().unwrap();
    let trace = directory.path().join("trace.jsonl");
    fs::write(
        &trace,
        concat!(
            "{\"event\":\"start\",\"argv\":[\"git\",\"log\",\"--max-count=200\"]}\n",
            "{\"event\":\"start\",\"argv\":[\"git\",\"log\",\"--skip=0\",\"--stdin\"]}\n",
            "{\"event\":\"start\",\"argv\":[\"git\",\"log\",\"--skip=200\",\"--stdin\"]}\n",
            "{\"event\":\"exit\",\"argv\":[\"git\",\"log\",\"--skip=400\"]}\n",
        ),
    )
    .unwrap();
    assert_eq!(continuation_reads(&trace), ["--skip=200"]);
    assert!(continuation_reads(&directory.path().join("missing")).is_empty());
}
