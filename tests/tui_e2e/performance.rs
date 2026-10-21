//! TASK-66 `performance`: real-binary PTY benchmarks over a large generated repository,
//! judged against the p95 budgets in the architecture specification (doc-2) and recorded with
//! the hardware, fixture shape, build profile, sample counts, percentiles, RSS, CPU, disk and
//! subprocess counts. Budgets and the debug-build factor are documented in
//! `docs/performance.md`.
//!
//! Fixture (built once per test process with `git fast-import`, copied fresh per journey):
//! 20,000 linear commits over a 5,000-file working tree, a commit that adds a 50,000-file tree
//! and one that removes it again, and a HEAD commit that rewrites a 2.5 MB file (a ~5 MB diff).
//! Actions logs (a 20 MB job log) are measured in `actions::perf_log_v66`.
//!
//! Every journey runs by keyboard and by mouse, at 120x40 with an 80x24 segment, and verifies
//! the visible outcome plus the Git state (read-only browsing changes nothing) and the gh
//! journal (no GitHub call). Measurements are collected first and judged at the end, so one
//! run reports every metric even when one is over budget.
use crate::support::{
    history_fixture::{history_pane, history_ready, selected_history_row, RepoState},
    Fixture, Session,
};
use ghtui::{config::Config, render_timings_v66 as timings};
use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

/// Performance journeys run one at a time so their timings do not compete with each other.
pub(crate) static PERF_LOCK: Mutex<()> = Mutex::new(());

pub(crate) fn perf_lock() -> std::sync::MutexGuard<'static, ()> {
    PERF_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

// ---------------------------------------------------------------------------------------
// Budgets (doc-2 "Performance targets") and the build-profile factor.
// ---------------------------------------------------------------------------------------

/// First usable cached frame.
pub(crate) const FIRST_FRAME_MS: u64 = 300;
/// p95 input-to-render.
pub(crate) const INPUT_TO_RENDER_P95_MS: u64 = 50;
/// p95 render cost at 120x40.
pub(crate) const RENDER_P95_MS: u64 = 16;
/// Warmed search first page.
pub(crate) const SEARCH_FIRST_PAGE_MS: u64 = 100;
/// Cached selected-file diff.
pub(crate) const CACHED_DIFF_MS: u64 = 150;

/// Separate ceilings (no doc budget: initial indexing, cold large loads, memory, idle CPU).
const INDEXING_CEILING_MS: u64 = 120_000;
const COLD_LARGE_LOAD_CEILING_MS: u64 = 20_000;
const PEAK_RSS_CEILING_KIB: u64 = 1024 * 1024;
const IDLE_CPU_CEILING_PERCENT: f64 = 50.0;
const IDLE_WINDOW: Duration = Duration::from_secs(2);

/// The doc budgets are for a release build. The PTY journeys drive the unoptimized test
/// binary on a shared development host, so each budget is multiplied by this factor
/// (`GHTUI_PERF_BUDGET_FACTOR` overrides it; 1 applies the doc budgets exactly).
pub(crate) fn budget_factor() -> u64 {
    std::env::var("GHTUI_PERF_BUDGET_FACTOR")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|factor| *factor > 0)
        .unwrap_or(if cfg!(debug_assertions) { 10 } else { 1 })
}

// ---------------------------------------------------------------------------------------
// Report: every measurement with its budget, judged together at the end.
// ---------------------------------------------------------------------------------------

pub(crate) struct Report {
    name: String,
    metrics: Vec<Value>,
    violations: Vec<String>,
    started: Instant,
}

impl Report {
    pub(crate) fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            metrics: Vec::new(),
            violations: Vec::new(),
            started: Instant::now(),
        }
    }

    /// A single timing judged against `budget_ms` × the build factor.
    pub(crate) fn budget(&mut self, metric: &str, value: Duration, budget_ms: u64) {
        let ms = value.as_millis() as u64;
        let ceiling = budget_ms * budget_factor();
        if ms > ceiling {
            self.violations.push(format!(
                "{metric}: {ms} ms > {ceiling} ms (budget {budget_ms} ms × {})",
                budget_factor()
            ));
        }
        self.metrics
            .push(json!({"metric": metric, "kind": "budget", "ms": ms,
            "budget_ms": budget_ms, "ceiling_ms": ceiling}));
    }

    /// A p95 over samples judged against `budget_ms` × the build factor.
    pub(crate) fn p95(&mut self, metric: &str, samples_us: &[u64], budget_ms: u64) {
        let Some(stats) = timings::stats(samples_us) else {
            self.violations.push(format!("{metric}: no samples"));
            return;
        };
        let ceiling_us = budget_ms * 1000 * budget_factor();
        if stats.p95 > ceiling_us {
            self.violations.push(format!(
                "{metric}: p95 {} us > {ceiling_us} us (budget {budget_ms} ms × {}) over {} samples",
                stats.p95,
                budget_factor(),
                stats.count
            ));
        }
        self.metrics
            .push(json!({"metric": metric, "kind": "p95", "unit": "us",
            "stats": stats, "budget_ms": budget_ms, "ceiling_us": ceiling_us}));
    }

    /// A measurement with its own fixed ceiling, outside the doc budgets (indexing, cold loads).
    pub(crate) fn separate(&mut self, metric: &str, value: Duration, ceiling_ms: u64) {
        let ms = value.as_millis() as u64;
        if ms > ceiling_ms {
            self.violations.push(format!(
                "{metric}: {ms} ms > separate ceiling {ceiling_ms} ms"
            ));
        }
        self.metrics
            .push(json!({"metric": metric, "kind": "separate", "ms": ms,
            "ceiling_ms": ceiling_ms}));
    }

    /// A recorded value with no ceiling of its own.
    pub(crate) fn record(&mut self, metric: &str, value: Value) {
        self.metrics
            .push(json!({"metric": metric, "kind": "record", "value": value}));
    }

    pub(crate) fn require(&mut self, metric: &str, ok: bool, detail: String) {
        if !ok {
            self.violations.push(format!("{metric}: {detail}"));
        }
        self.metrics
            .push(json!({"metric": metric, "kind": "check", "ok": ok, "detail": detail}));
    }

    /// Writes the JSON report, prints a summary, and fails on any violation.
    pub(crate) fn finish(self, fixture_shape: Value) {
        let report = json!({
            "test": self.name,
            "hardware": hardware(),
            "build_profile": if cfg!(debug_assertions) { "debug (test profile)" } else { "release" },
            "budget_factor": budget_factor(),
            "fixture": fixture_shape,
            "metrics": self.metrics,
            "violations": self.violations,
            "journey_seconds": self.started.elapsed().as_secs_f64(),
        });
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/tui-artifacts/perf-v66");
        let _ = fs::create_dir_all(&directory);
        let path = directory.join(format!("{}.json", self.name));
        let _ = fs::write(&path, serde_json::to_vec_pretty(&report).unwrap());
        eprintln!("[perf-v66] {} -> {}", self.name, path.display());
        for metric in report["metrics"].as_array().unwrap() {
            eprintln!("[perf-v66]   {metric}");
        }
        assert!(
            self.violations.is_empty(),
            "performance budgets exceeded in {}:\n  {}\nreport: {}",
            self.name,
            self.violations.join("\n  "),
            path.display()
        );
    }
}

pub(crate) fn hardware() -> Value {
    let cpuinfo = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let model = cpuinfo
        .lines()
        .find_map(|line| line.strip_prefix("model name"))
        .map(|rest| rest.trim_start_matches([' ', '\t', ':']).to_owned())
        .unwrap_or_default();
    let meminfo = fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let field = |name: &str| {
        meminfo
            .lines()
            .find_map(|line| line.strip_prefix(name))
            .and_then(|rest| rest.split_whitespace().next()?.parse::<u64>().ok())
    };
    json!({
        "os": std::env::consts::OS,
        "kernel": fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default().trim(),
        "cpu": model,
        "logical_cpus": std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0),
        "mem_total_kib": field("MemTotal:"),
        "mem_available_kib": field("MemAvailable:"),
        "loadavg": fs::read_to_string("/proc/loadavg").unwrap_or_default().trim(),
    })
}

// ---------------------------------------------------------------------------------------
// Process resources, from /proc: RSS, peak RSS, CPU time and live children.
// ---------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub(crate) struct ProcSample {
    pub rss_kib: u64,
    pub peak_rss_kib: u64,
    pub cpu_ticks: u64,
    pub threads: u64,
    pub children: usize,
    pub at: Instant,
}

pub(crate) fn proc_sample(pid: i32) -> ProcSample {
    let status = fs::read_to_string(format!("/proc/{pid}/status")).expect("application status");
    let field = |name: &str| {
        status
            .lines()
            .find_map(|line| line.strip_prefix(name))
            .and_then(|rest| rest.split_whitespace().next()?.parse::<u64>().ok())
            .unwrap_or(0)
    };
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).expect("application stat");
    // Fields after `(comm)`: state is field 3, utime 14 and stime 15 (1-based).
    let after = &stat[stat.rfind(')').expect("stat comm") + 2..];
    let fields: Vec<&str> = after.split_whitespace().collect();
    let ticks = |index: usize| fields[index - 3].parse::<u64>().unwrap_or(0);
    ProcSample {
        rss_kib: field("VmRSS:"),
        peak_rss_kib: field("VmHWM:"),
        cpu_ticks: ticks(14) + ticks(15),
        threads: field("Threads:"),
        children: live_children(pid),
        at: Instant::now(),
    }
}

fn live_children(pid: i32) -> usize {
    let Ok(tasks) = fs::read_dir(format!("/proc/{pid}/task")) else {
        return 0;
    };
    tasks
        .flatten()
        .filter_map(|task| fs::read_to_string(task.path().join("children")).ok())
        .map(|children| children.split_whitespace().count())
        .sum()
}

pub(crate) fn cpu_percent(before: &ProcSample, after: &ProcSample) -> f64 {
    // SAFETY: sysconf has no preconditions.
    let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as f64;
    let seconds = after.at.duration_since(before.at).as_secs_f64().max(0.001);
    (after.cpu_ticks.saturating_sub(before.cpu_ticks)) as f64 / hz / seconds * 100.0
}

/// Lets the application sit idle (no input) and reports CPU use, live children, and how many
/// Git children started meanwhile (from the GIT_TRACE2_EVENT journal).
pub(crate) fn idle_window(report: &mut Report, session: &mut Session, label: &str, trace: &Path) {
    let pid = session.owned_application_pid();
    let git_before = git_starts(trace);
    let before = proc_sample(pid);
    session.poll_output(IDLE_WINDOW);
    let after = proc_sample(pid);
    let git_after = git_starts(trace);
    let cpu = cpu_percent(&before, &after);
    report.record(
        &format!("{label}_idle"),
        json!({"cpu_percent": cpu, "rss_kib": after.rss_kib, "peak_rss_kib": after.peak_rss_kib,
            "threads": after.threads, "live_children": after.children,
            "git_children_started": git_after - git_before}),
    );
    report.require(
        &format!("{label}_idle_cpu"),
        cpu <= IDLE_CPU_CEILING_PERCENT,
        format!("{cpu:.1}% of one CPU while idle (ceiling {IDLE_CPU_CEILING_PERCENT}%)"),
    );
    report.require(
        &format!("{label}_idle_children"),
        after.children == 0 && git_after == git_before,
        format!(
            "{} live children, {} Git children started while idle",
            after.children,
            git_after - git_before
        ),
    );
}

/// Git processes started so far: one trace2 `start` event per Git process.
pub(crate) fn git_starts(trace: &Path) -> usize {
    fs::read_to_string(trace)
        .unwrap_or_default()
        .lines()
        .filter(|line| line.contains("\"event\":\"start\""))
        .count()
}

pub(crate) fn tree_bytes(path: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(path) else {
        return fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
    };
    entries
        .flatten()
        .map(|entry| match entry.file_type() {
            Ok(kind) if kind.is_dir() => tree_bytes(&entry.path()),
            _ => entry.metadata().map(|meta| meta.len()).unwrap_or(0),
        })
        .sum()
}

// ---------------------------------------------------------------------------------------
// Frame timing: input written -> first NEW completed frame that shows the expected state.
// ---------------------------------------------------------------------------------------

pub(crate) fn measure(
    session: &mut Session,
    label: &str,
    timeout: Duration,
    act: impl FnOnce(&mut Session),
    done: impl Fn(&vt100::Screen) -> bool,
) -> Duration {
    let after = session.completed_cursorless_end();
    let started = Instant::now();
    act(session);
    loop {
        session.poll_output(Duration::from_millis(1));
        if session.completed_cursorless_end() > after
            && session.completed_cursorless_screen().is_some_and(&done)
        {
            return started.elapsed();
        }
        if started.elapsed() > timeout {
            session.save_artifacts();
            panic!(
                "No completed frame for {label} within {timeout:?}\n{}; artifacts: {}",
                session.screen(),
                session.artifact_dir.display()
            );
        }
    }
}

/// [`measure`] for owned modals with a text cursor (the tree Jump filter): their frames are
/// never cursorless, so the live screen text is read, and `done` must name text that the
/// screen did not show before the input.
fn measure_text(
    session: &mut Session,
    label: &str,
    timeout: Duration,
    act: impl FnOnce(&mut Session),
    done: impl Fn(&str) -> bool,
) -> Duration {
    assert!(
        !done(&session.screen()),
        "{label}: already shown before the input"
    );
    let started = Instant::now();
    act(session);
    loop {
        session.poll_output(Duration::from_millis(1));
        if done(&session.screen()) {
            return started.elapsed();
        }
        if started.elapsed() > timeout {
            session.save_artifacts();
            panic!(
                "No screen for {label} within {timeout:?}\n{}; artifacts: {}",
                session.screen(),
                session.artifact_dir.display()
            );
        }
    }
}

pub(crate) fn micros(duration: Duration) -> u64 {
    duration.as_micros() as u64
}

/// Render samples written by the application at exit (GHTUI_RENDER_TIMINGS), by size.
pub(crate) fn render_samples(path: &Path, cols: u16, rows: u16) -> Vec<u64> {
    let text = fs::read_to_string(path).expect("render timings written at exit");
    let (samples, _) = timings::parse(&text).expect("well-formed render timings");
    samples
        .into_iter()
        .filter(|sample| sample.cols == cols && sample.rows == rows)
        .map(|sample| sample.micros)
        .collect()
}

// ---------------------------------------------------------------------------------------
// The large repository, generated once with git fast-import.
// ---------------------------------------------------------------------------------------

pub(crate) const COMMITS: usize = 20_000;
pub(crate) const SOURCE_FILES: usize = 5_000;
pub(crate) const WIDE_DIRS: usize = 500;
pub(crate) const WIDE_FILES_PER_DIR: usize = 100;
pub(crate) const HUGE_LINES: usize = 40_000;
const BASE_TIME: u64 = 1_700_000_000;
const WIDE_BLOB: &str = "wide tree blob v66\n";
const ALPHA: usize = 1010;
const OMEGA: usize = 4040;

pub(crate) fn source_path(file: usize) -> String {
    format!("src/m{:02}/f{:03}.rs", file / 100, file % 100)
}

fn source_body(file: usize, revision: usize) -> String {
    let mut body = format!(
        "// {} revision {revision}\npub fn item_{file}() -> usize {{ {revision} }}\n",
        source_path(file)
    );
    if file == ALPHA {
        body.push_str("// perfneedlealpha marker\n");
    }
    if file == OMEGA {
        body.push_str("// perfneedleomega marker\n");
    }
    body
}

/// Unique lines on each side, so Git's diff discards them instead of aligning them.
fn huge_body(side: &str) -> String {
    let pad = "x".repeat(46);
    (0..HUGE_LINES)
        .map(|line| format!("{side} line {line:06} {pad}\n"))
        .collect()
}

fn data(stream: &mut Vec<u8>, body: &[u8]) {
    stream.extend_from_slice(format!("data {}\n", body.len()).as_bytes());
    stream.extend_from_slice(body);
    stream.push(b'\n');
}

fn commit(stream: &mut Vec<u8>, index: usize, message: &str) {
    let time = BASE_TIME + index as u64 * 60;
    stream.extend_from_slice(
        format!(
            "commit refs/heads/main\nauthor Perf Author <perf@example.invalid> {time} +0000\n\
             committer Perf Author <perf@example.invalid> {time} +0000\n"
        )
        .as_bytes(),
    );
    data(stream, message.as_bytes());
}

/// The fast-import stream: 19,996 source commits, then the wide tree added and removed, then
/// the huge file added and rewritten at HEAD.
pub(crate) fn fast_import_stream() -> Vec<u8> {
    let mut stream = Vec::with_capacity(16 * 1024 * 1024);
    stream.extend_from_slice(b"blob\nmark :1\n");
    data(&mut stream, WIDE_BLOB.as_bytes());
    let small = COMMITS - 4;
    for index in 0..small {
        let file = index % SOURCE_FILES;
        let path = source_path(file);
        commit(&mut stream, index, &format!("Change {path} ({index})"));
        stream.extend_from_slice(format!("M 100644 inline {path}\n").as_bytes());
        data(&mut stream, source_body(file, index).as_bytes());
    }
    commit(&mut stream, small, "Add wide tree v66");
    for dir in 0..WIDE_DIRS {
        for file in 0..WIDE_FILES_PER_DIR {
            stream.extend_from_slice(
                format!("M 100644 :1 wide/d{dir:03}/f{file:02}.txt\n").as_bytes(),
            );
        }
    }
    commit(&mut stream, small + 1, "Remove wide tree v66");
    stream.extend_from_slice(b"D wide\n");
    commit(&mut stream, small + 2, "Add huge file v66");
    stream.extend_from_slice(b"M 100644 inline big/huge.txt\n");
    data(&mut stream, huge_body("old").as_bytes());
    commit(&mut stream, small + 3, "Rewrite huge file v66");
    stream.extend_from_slice(b"M 100644 inline big/huge.txt\n");
    data(&mut stream, huge_body("new").as_bytes());
    stream
}

fn git_in(directory: &Path, args: &[&str], stdin: Option<&[u8]>) -> String {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(bytes) = stdin {
        child.stdin.take().unwrap().write_all(bytes).unwrap();
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

pub(crate) struct LargeRepo {
    template: PathBuf,
    /// Every commit, newest first (the History order of this linear history).
    pub shas: Vec<String>,
    pub build: Duration,
}

/// Moves every working file's mtime an hour back (`cp -a` keeps it for each fixture copy).
fn settle_working_files(root: &Path) {
    let old = std::time::SystemTime::now() - Duration::from_secs(3600);
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).unwrap() {
            let entry = entry.unwrap();
            if entry.file_name() == ".git" {
                continue;
            }
            let kind = entry.file_type().unwrap();
            if kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_file() {
                fs::OpenOptions::new()
                    .write(true)
                    .open(entry.path())
                    .unwrap()
                    .set_modified(old)
                    .unwrap();
            }
        }
    }
}

pub(crate) fn large_repo() -> &'static LargeRepo {
    static REPO: OnceLock<LargeRepo> = OnceLock::new();
    REPO.get_or_init(|| {
        let started = Instant::now();
        let template = tempfile::tempdir().unwrap().keep().join("repo");
        fs::create_dir_all(&template).unwrap();
        git_in(
            &template,
            &["init", "--quiet", "--initial-branch=main"],
            None,
        );
        git_in(&template, &["config", "user.name", "Perf Author"], None);
        git_in(
            &template,
            &["config", "user.email", "perf@example.invalid"],
            None,
        );
        git_in(
            &template,
            &["fast-import", "--quiet"],
            Some(&fast_import_stream()),
        );
        git_in(&template, &["reset", "--quiet", "--hard", "main"], None);
        // A checkout written seconds before the search is "racy": its timestamps cannot yet
        // prove a file unchanged, so warm Ranked reuse (TASK-106) would rightly collect again.
        // The benchmark models a checkout nobody is editing, written well before the session.
        settle_working_files(&template);
        let shas: Vec<String> = git_in(&template, &["rev-list", "main"], None)
            .lines()
            .map(str::to_owned)
            .collect();
        assert_eq!(shas.len(), COMMITS, "generated history length");
        LargeRepo {
            template,
            shas,
            build: started.elapsed(),
        }
    })
}

pub(crate) fn fixture_shape(repo: &LargeRepo) -> Value {
    json!({
        "commits": COMMITS,
        "working_tree_files": SOURCE_FILES + 1,
        "wide_tree_files": WIDE_DIRS * WIDE_FILES_PER_DIR,
        "huge_file_lines_per_side": HUGE_LINES,
        "huge_file_bytes_per_side": huge_body("old").len(),
        "generation_seconds": repo.build.as_secs_f64(),
    })
}

/// A fresh copy of the large repository in a new fixture, with its index refreshed so
/// read-only browsing has nothing left to update.
pub(crate) fn large_fixture(repo: &LargeRepo) -> Fixture {
    let fixture = Fixture::new();
    let status = Command::new("cp")
        .arg("-a")
        .arg(repo.template.join("."))
        .arg(fixture.repo())
        .status()
        .unwrap();
    assert!(status.success(), "copy the generated repository");
    fixture.git(&["update-index", "-q", "--refresh"]);
    Config {
        refresh_seconds: 3600,
        avatars: false,
        ..Config::default()
    }
    .save(&fixture.root.join("config/ghtui/config.toml"))
    .unwrap();
    fixture
}

// ---------------------------------------------------------------------------------------
// Journey helpers.
// ---------------------------------------------------------------------------------------

const BACK_KEY: &[u8] = b"\x1b[1;3D";
const HELP_TITLE: &str = "Help • keyboard and mouse";
const JUMP: &str = "Jump to saved path • original source";
/// Inside the first 50,000 rows the Jump inventory admits (it lists `src/` first).
const WIDE_TARGET: &str = "wide/d250/f50.txt";

fn selected_is(frame: &vt100::Screen, sha: &str) -> bool {
    selected_history_row(frame).is_some_and(|row| !row.sha.is_empty() && sha.starts_with(&row.sha))
}

/// `true` when the History detail (or the screen) shows every part.
fn shows(frame: &vt100::Screen, parts: &[&str]) -> bool {
    let text = frame.contents();
    parts.iter().all(|part| text.contains(part))
}

fn wait_shows(session: &mut Session, parts: &[&str], timeout: Duration) {
    session.wait_for_cursorless_condition(|frame| shows(frame, parts), &parts.join(" + "), timeout);
}

/// Clicks `text` right of the History table (inside the commit detail).
fn click_in_detail(session: &mut Session, text: &str) {
    let screen = session.completed_cursorless_screen().unwrap();
    let (_, right, _, _) = history_pane(screen).expect("History pane");
    let rows = screen.size().0;
    let found = (0..rows).find_map(|y| {
        let line: String = (right + 1..screen.size().1)
            .map(|x| {
                screen
                    .cell(y, x)
                    .map(|cell| cell.contents())
                    .filter(|contents| !contents.is_empty())
                    .unwrap_or_else(|| " ".into())
            })
            .collect();
        line.find(text)
            .map(|at| (right + 1 + line[..at].chars().count() as u16, y))
    });
    let (x, y) =
        found.unwrap_or_else(|| panic!("{text:?} not in the detail\n{}", session.screen()));
    session.click(x, y);
}

/// Moves the History bar by keyboard (Down/Up) or mouse (a click on the row) and times each
/// move to the completed frame that shows the new row selected.
fn history_samples(
    session: &mut Session,
    shas: &[String],
    mouse: bool,
    path: &[usize],
    samples: &mut Vec<u64>,
) {
    let mut at = path[0];
    for &next in &path[1..] {
        let target = shas[next].clone();
        let duration = measure(
            session,
            &format!("History row {next}"),
            Duration::from_secs(10),
            |session| {
                if mouse {
                    session.click_history_row(&target);
                } else if next > at {
                    session.send_key(b"\x1b[B");
                } else {
                    session.send_key(b"\x1b[A");
                }
            },
            |frame| selected_is(frame, &target),
        );
        samples.push(micros(duration));
        at = next;
    }
}

fn there_and_back(rows: usize) -> Vec<usize> {
    (0..=rows).chain((0..rows).rev()).collect()
}

fn wait_history_at(session: &mut Session, sha: &str) {
    let sha = sha.to_owned();
    session.wait_for_cursorless_condition(
        |frame| history_ready(frame) && selected_is(frame, &sha),
        "History ready with the expected row selected",
        Duration::from_secs(20),
    );
}

fn open_diff(session: &mut Session, mouse: bool, label: &str) -> Duration {
    measure(
        session,
        label,
        Duration::from_millis(COLD_LARGE_LOAD_CEILING_MS * 2),
        |session| {
            if mouse {
                click_in_detail(session, "enter diff");
            } else {
                session.send_key(b"\r");
            }
        },
        |frame| {
            shows(frame, &["Saved commit changes", "Compared parent 1:"])
                && !frame.contents().contains("Loading ")
        },
    )
}

fn back_to_history(session: &mut Session, mouse: bool, sha: &str) {
    session.choose(mouse, "Alt-← Back", BACK_KEY);
    wait_history_at(session, sha);
    session.wait_for_settled_footer();
}

/// Waits for the HEAD commit's detail: the 5 MB rewrite with its bounded counts.
fn wait_head_detail(session: &mut Session) {
    let counts = format!("1 file · +{HUGE_LINES} −{HUGE_LINES}");
    wait_shows(
        session,
        &[&counts, "big/huge.txt", "enter diff"],
        Duration::from_millis(COLD_LARGE_LOAD_CEILING_MS),
    );
}

// ---------------------------------------------------------------------------------------
// Journey 1 (cold start): startup, History over 20k commits, the 5 MB diff, the 50k-file tree.
// ---------------------------------------------------------------------------------------

fn history_journey(mouse: bool) {
    let _lock = perf_lock();
    let repo = large_repo();
    let fixture = large_fixture(repo);
    let name = format!(
        "performance_v66_history_{}",
        if mouse { "mouse" } else { "keyboard" }
    );
    let mut report = Report::new(&name);
    let baseline = RepoState::capture(&fixture.repo());
    let trace = fixture.root.join("git-trace2.jsonl");
    let render = fixture.root.join("render-1.txt");
    let env = [
        ("GIT_TRACE2_EVENT", trace.to_str().unwrap()),
        (timings::ENV, render.to_str().unwrap()),
    ];

    // Cold start: spawn -> first completed frame with the repository's recent commits.
    let spawned = Instant::now();
    let mut s = Session::launch_from_with_env(&fixture, &fixture.repo(), 120, 40, &env);
    s.wait_for_cursorless_condition(
        |frame| frame.contents().contains("Recent commits"),
        "first usable frame",
        Duration::from_millis(COLD_LARGE_LOAD_CEILING_MS),
    );
    report.budget(
        "startup_cold_first_usable_frame",
        spawned.elapsed(),
        FIRST_FRAME_MS,
    );
    s.wait_for_settled_footer();
    idle_window(&mut report, &mut s, "startup", &trace);

    // History: the first page of 20,000 commits.
    let opened = measure(
        &mut s,
        "History first page",
        Duration::from_millis(COLD_LARGE_LOAD_CEILING_MS),
        |s| s.choose(mouse, "[g History]", b"g"),
        history_ready,
    );
    report.budget("history_first_page", opened, FIRST_FRAME_MS);
    wait_history_at(&mut s, &repo.shas[0]);
    let loaded = crate::support::history_fixture::history_loaded_count(&s.screen());
    report.record("history_loaded_commits", json!(loaded));
    report.require(
        "history_bounded_page",
        loaded.is_some_and(|count| count < COMMITS),
        format!("{loaded:?} of {COMMITS} commits loaded at once"),
    );
    wait_head_detail(&mut s);
    s.wait_for_settled_footer();

    // Input-to-render at 120x40 while commit details (two of them 50,000 files) load behind.
    let mut wide_samples = Vec::new();
    let path = there_and_back(15);
    history_samples(&mut s, &repo.shas, mouse, &path, &mut wide_samples);
    report.p95(
        "input_to_render_120x40",
        &wide_samples,
        INPUT_TO_RENDER_P95_MS,
    );
    wait_history_at(&mut s, &repo.shas[0]);

    // The 5 MB diff: first open, then the cached reopen.
    wait_head_detail(&mut s);
    let cold = open_diff(&mut s, mouse, "first 5 MB diff open");
    report.separate("diff_5mb_first_open", cold, COLD_LARGE_LOAD_CEILING_MS);
    s.wait_for_screen("big/huge.txt");
    back_to_history(&mut s, mouse, &repo.shas[0]);
    wait_head_detail(&mut s);
    let cached = open_diff(&mut s, mouse, "cached 5 MB diff reopen");
    report.budget("diff_5mb_cached_reopen", cached, CACHED_DIFF_MS);
    back_to_history(&mut s, mouse, &repo.shas[0]);

    // 80x24: the same History, the same inputs.
    s.resize(80, 24);
    wait_history_at(&mut s, &repo.shas[0]);
    let mut narrow_samples = Vec::new();
    history_samples(
        &mut s,
        &repo.shas,
        mouse,
        &there_and_back(5),
        &mut narrow_samples,
    );
    report.p95(
        "input_to_render_80x24",
        &narrow_samples,
        INPUT_TO_RENDER_P95_MS,
    );
    s.resize(120, 40);
    wait_history_at(&mut s, &repo.shas[0]);
    s.wait_for_settled_footer();

    // The 50,000-file commit: a bounded detail, its saved tree, and a path search in it.
    let wide = repo.shas[3].clone();
    s.open_saved_graph_subject(mouse, "Add wide tree v66", &wide);
    let wide_files = WIDE_DIRS * WIDE_FILES_PER_DIR;
    wait_shows(
        &mut s,
        &[&format!("{wide_files} files · +{wide_files}"), "↓ more"],
        Duration::from_millis(COLD_LARGE_LOAD_CEILING_MS),
    );
    s.wait_for_settled_footer();
    let tree = measure(
        &mut s,
        "saved 50k-file tree",
        Duration::from_millis(COLD_LARGE_LOAD_CEILING_MS),
        |s| s.choose(mouse, "[t Tree]", b"t"),
        |frame| shows(frame, &["Saved file tree", &wide]),
    );
    report.separate("tree_50k_open", tree, COLD_LARGE_LOAD_CEILING_MS);
    let jump = measure_text(
        &mut s,
        "saved tree path index",
        Duration::from_millis(COLD_LARGE_LOAD_CEILING_MS),
        |s| s.choose(mouse, "[J Jump]", b"J"),
        |text| {
            text.contains(JUMP)
                && (text.contains("Complete saved tree") || text.contains("Partial saved tree"))
        },
    );
    report.separate("tree_50k_jump_index", jump, COLD_LARGE_LOAD_CEILING_MS);
    // The inventory is bounded (50,000 rows); past that it says which paths stay unknown.
    let inventory = s.screen();
    report.record(
        "tree_50k_jump_inventory",
        json!(inventory
            .lines()
            .find(|line| line.contains(" original rows · "))
            .map(|line| line.trim_matches(['│', ' ']).to_owned())),
    );
    report.require(
        "tree_50k_jump_bounded",
        inventory.contains("50000 original rows"),
        "the Jump inventory admits at most 50,000 rows".into(),
    );
    if mouse {
        s.click_visible_action("Filter:");
    } else {
        s.send_key(b"/");
    }
    s.send_key(b"\x01");
    let row = format!("file · {WIDE_TARGET}");
    let found = measure_text(
        &mut s,
        "path search in the 50k-file tree",
        Duration::from_millis(COLD_LARGE_LOAD_CEILING_MS),
        |s| s.paste(WIDE_TARGET),
        |text| text.contains(&row) && text.contains(" known matches"),
    );
    report.budget(
        "tree_50k_path_search_first_page",
        found,
        SEARCH_FIRST_PAGE_MS,
    );
    if mouse {
        s.click_visible_action(&row);
    } else {
        s.send_key(b"\r");
        select_row_by_keyboard(&mut s, &row);
    }
    s.wait_for_overlay_close(JUMP);
    s.wait_for_screen(WIDE_BLOB.trim());
    s.wait_for_screen(&wide);
    s.wait_for_settled_footer();

    let end = proc_sample(s.owned_application_pid());
    report.record(
        "resources_after_journey",
        json!({"rss_kib": end.rss_kib, "peak_rss_kib": end.peak_rss_kib,
            "threads": end.threads, "git_children_total": git_starts(&trace)}),
    );
    report.require(
        "peak_rss",
        end.peak_rss_kib <= PEAK_RSS_CEILING_KIB,
        format!(
            "peak RSS {} KiB (ceiling {PEAK_RSS_CEILING_KIB} KiB)",
            end.peak_rss_kib
        ),
    );
    idle_window(&mut report, &mut s, "after_large_loads", &trace);
    s.quit_and_assert_restored_terminal(mouse);

    let render_wide = render_samples(&render, 120, 40);
    report.p95("render_cost_120x40", &render_wide, RENDER_P95_MS);
    let render_narrow = render_samples(&render, 80, 24);
    report.p95("render_cost_80x24", &render_narrow, RENDER_P95_MS);
    report.record(
        "disk",
        json!({"config_and_cache_bytes": tree_bytes(&fixture.root.join("config/ghtui"))
            + tree_bytes(&fixture.root.join("cache")),
            "repository_bytes": tree_bytes(&fixture.repo())}),
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
    report.finish(fixture_shape(repo));
}

/// Down until the row is the inverted selection, then Enter (the tree Jump list).
fn select_row_by_keyboard(session: &mut Session, label: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        session.poll_output(Duration::from_millis(1));
        if let Some((column, row)) = session.locate(label) {
            if session.cell_inverted(row, column) {
                session.send_key(b"\r");
                return;
            }
        }
        assert!(
            Instant::now() < deadline,
            "Row never keyboard-selected: {label}\n{}",
            session.screen()
        );
        session.send_key(b"\x1b[B");
        session.poll_output(Duration::from_millis(50));
    }
}

#[test]
fn performance_v66_keyboard_large_history_diff_tree_budgets() {
    history_journey(false)
}
#[test]
fn performance_v66_mouse_large_history_diff_tree_budgets() {
    history_journey(true)
}

// ---------------------------------------------------------------------------------------
// Journey 2 (warm start): the same repository after a first run; search first pages and a
// keypress served while the ranked index builds.
// ---------------------------------------------------------------------------------------

fn type_query(session: &mut Session, mouse: bool, query: &str) {
    if mouse {
        session.wait_for_settled_footer();
        session.click_visible_action("[/ Query]");
    } else {
        session.send_key(b"/");
    }
    session.send_key(b"\x01");
    session.paste(query);
    session.wait_for_screen(query);
}

fn submit_query(session: &mut Session, mouse: bool) {
    if mouse {
        session.click_visible_action("[Enter Search]");
    } else {
        session.send_key(b"\r");
    }
}

fn search_settled(frame: &vt100::Screen, parts: &[&str]) -> bool {
    shows(frame, parts) && !frame.contents().contains("Loading Search")
}

/// Every ranked source's committed and reserved generation, from the application's cache.
fn ranked_generations(fixture: &Fixture) -> Vec<(String, String, u64)> {
    let connection = rusqlite::Connection::open_with_flags(
        fixture.root.join("config/ghtui/cache.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let mut statement = connection
        .prepare(
            "SELECT 'published',source,generation FROM checkpoints WHERE source LIKE 'search-index:%'
            UNION ALL SELECT 'reserved',source,generation FROM sync_reservations WHERE source LIKE 'search-index:%'
            ORDER BY 1,2",
        )
        .unwrap();
    let rows = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap();
    rows.map(Result::unwrap).collect()
}

fn search_journey(mouse: bool) {
    let _lock = perf_lock();
    let repo = large_repo();
    let fixture = large_fixture(repo);
    let name = format!(
        "performance_v66_search_{}",
        if mouse { "mouse" } else { "keyboard" }
    );
    let mut report = Report::new(&name);
    let baseline = RepoState::capture(&fixture.repo());
    let trace = fixture.root.join("git-trace2.jsonl");

    // A first run creates the cache; the measured start is the warm one after it.
    let render_first = fixture.root.join("render-first.txt");
    let mut s = Session::launch_from_with_env(
        &fixture,
        &fixture.repo(),
        120,
        40,
        &[(timings::ENV, render_first.to_str().unwrap())],
    );
    s.wait_for_screen("Recent commits");
    s.wait_for_settled_footer();
    s.quit_and_assert_restored_terminal(mouse);

    let render = fixture.root.join("render-2.txt");
    let env = [
        ("GIT_TRACE2_EVENT", trace.to_str().unwrap()),
        (timings::ENV, render.to_str().unwrap()),
    ];
    let spawned = Instant::now();
    let mut s = Session::launch_from_with_env(&fixture, &fixture.repo(), 120, 40, &env);
    s.wait_for_cursorless_condition(
        |frame| frame.contents().contains("Recent commits"),
        "warm first usable frame",
        Duration::from_millis(COLD_LARGE_LOAD_CEILING_MS),
    );
    report.budget(
        "startup_warm_first_usable_frame",
        spawned.elapsed(),
        FIRST_FRAME_MS,
    );
    s.wait_for_settled_footer();
    s.choose(mouse, "3 Search", b"3");
    s.wait_for_screen("Selected local search corpus ready");

    // Exact: the first page of a query that matches every one of the 5,000 source files.
    type_query(&mut s, mouse, "pub fn item_");
    let exact = measure(
        &mut s,
        "exact search first page",
        Duration::from_millis(COLD_LARGE_LOAD_CEILING_MS),
        |s| submit_query(s, mouse),
        |frame| search_settled(frame, &["results • Exact", ".rs"]),
    );
    report.budget("search_exact_first_page", exact, SEARCH_FIRST_PAGE_MS);

    // Ranked, first query: the index builds (indexing, measured separately). A keypress made
    // while it builds is served at input-to-render speed.
    s.wait_for_settled_footer();
    s.choose(mouse, "n Ranked", b"n");
    s.wait_for_settled_footer();
    type_query(&mut s, mouse, "perfneedlealpha");
    let index_started = Instant::now();
    submit_query(&mut s, mouse);
    s.wait_for_cursorless_condition(
        |frame| frame.contents().contains("Loading Search"),
        "ranked search running",
        Duration::from_secs(10),
    );
    let help = measure(
        &mut s,
        "Help while the ranked index builds",
        Duration::from_secs(10),
        |s| s.choose(mouse, "[? Help]", b"?"),
        |frame| frame.contents().contains(HELP_TITLE),
    );
    report.budget(
        "input_to_render_while_indexing",
        help,
        INPUT_TO_RENDER_P95_MS,
    );
    if mouse {
        s.click_visible_action("[Esc Close]");
    } else {
        s.send_key(b"\x1b");
    }
    s.wait_for_overlay_close(HELP_TITLE);
    let alpha = format!("f{:03}.rs", ALPHA % 100);
    s.wait_for_cursorless_condition(
        |frame| search_settled(frame, &["1 results • Ranked", &alpha]),
        "first ranked results",
        Duration::from_millis(INDEXING_CEILING_MS),
    );
    report.separate(
        "search_ranked_initial_indexing",
        index_started.elapsed(),
        INDEXING_CEILING_MS,
    );

    let first_generations = ranked_generations(&fixture);

    // Ranked, warmed: an unchanged corpus reuses the committed index.
    s.wait_for_settled_footer();
    type_query(&mut s, mouse, "perfneedleomega");
    let omega = format!("f{:03}.rs", OMEGA % 100);
    let warm = measure(
        &mut s,
        "warmed ranked first page",
        Duration::from_millis(INDEXING_CEILING_MS),
        |s| submit_query(s, mouse),
        |frame| search_settled(frame, &["1 results • Ranked", &omega]),
    );
    // TASK-106: an unchanged checkout reuses the committed generation without collecting or
    // reserving, so the warm query meets the warmed search first-page budget.
    report.budget("search_ranked_warm_first_page", warm, SEARCH_FIRST_PAGE_MS);
    // Storage effect: the warm query reserved and published nothing.
    let warm_generations = ranked_generations(&fixture);
    report.require(
        "search_ranked_warm_reused_generation",
        !first_generations.is_empty() && warm_generations == first_generations,
        format!("after first query {first_generations:?}, after warm query {warm_generations:?}"),
    );
    s.wait_for_settled_footer();

    // 80x24: the same results stay usable after a resize.
    s.resize(80, 24);
    s.wait_for_screen("1 results • Ranked");
    s.resize(120, 40);
    s.wait_for_screen("1 results • Ranked");
    s.wait_for_settled_footer();

    let end = proc_sample(s.owned_application_pid());
    report.record(
        "resources_after_journey",
        json!({"rss_kib": end.rss_kib, "peak_rss_kib": end.peak_rss_kib,
            "threads": end.threads, "git_children_total": git_starts(&trace)}),
    );
    report.require(
        "peak_rss",
        end.peak_rss_kib <= PEAK_RSS_CEILING_KIB,
        format!(
            "peak RSS {} KiB (ceiling {PEAK_RSS_CEILING_KIB} KiB)",
            end.peak_rss_kib
        ),
    );
    idle_window(&mut report, &mut s, "after_search", &trace);
    s.quit_and_assert_restored_terminal(mouse);

    report.p95(
        "render_cost_120x40",
        &render_samples(&render, 120, 40),
        RENDER_P95_MS,
    );
    report.record(
        "disk",
        json!({"config_and_cache_bytes": tree_bytes(&fixture.root.join("config/ghtui"))
            + tree_bytes(&fixture.root.join("cache")),
            "search_index_bytes": tree_bytes(&fixture.root.join("config/ghtui/index"))}),
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
    report.finish(fixture_shape(repo));
}

#[test]
fn performance_v66_keyboard_warm_start_search_and_input_while_indexing() {
    search_journey(false)
}
#[test]
fn performance_v66_mouse_warm_start_search_and_input_while_indexing() {
    search_journey(true)
}

// ---------------------------------------------------------------------------------------
// Code tests: statistics, sample format, and the fixture's shape.
// ---------------------------------------------------------------------------------------

#[test]
fn performance_v66_percentiles_use_nearest_rank() {
    assert_eq!(timings::percentile(&[], 95.0), None);
    assert_eq!(timings::percentile(&[7], 95.0), Some(7));
    let samples: Vec<u64> = (1..=100).rev().collect();
    assert_eq!(timings::percentile(&samples, 50.0), Some(50));
    assert_eq!(timings::percentile(&samples, 95.0), Some(95));
    assert_eq!(timings::percentile(&samples, 100.0), Some(100));
    assert_eq!(timings::percentile(&samples, 0.0), Some(1));
    assert_eq!(timings::percentile(&samples, 101.0), None);
    // 20 samples: p95 is the 19th smallest, so one outlier never hides in the p95.
    let mut twenty = vec![10; 19];
    twenty.push(900);
    assert_eq!(timings::percentile(&twenty, 95.0), Some(10));
    twenty[0] = 800;
    assert_eq!(timings::percentile(&twenty, 95.0), Some(800));
    let stats = timings::stats(&[5, 1, 3]).unwrap();
    assert_eq!(
        (stats.count, stats.min, stats.p50, stats.p95, stats.max),
        (3, 1, 3, 5, 5)
    );
}

#[test]
fn performance_v66_render_samples_round_trip_and_reject_malformed_lines() {
    let samples = [
        timings::RenderSample {
            cols: 120,
            rows: 40,
            micros: 1500,
        },
        timings::RenderSample {
            cols: 80,
            rows: 24,
            micros: 900,
        },
    ];
    let text = timings::format(&samples, 3);
    assert_eq!(text, "120 40 1500\n80 24 900\ndropped 3\n");
    assert_eq!(timings::parse(&text).unwrap(), (samples.to_vec(), 3));
    assert!(timings::parse("120 40\n").is_err());
    assert!(timings::parse("120 forty 7\n").is_err());
    assert!(timings::parse("dropped many\n").is_err());
}

#[test]
fn performance_v66_report_judges_budgets_with_the_build_factor() {
    let factor = budget_factor();
    let mut report = Report::new("performance_v66_self_check");
    report.budget("within", Duration::from_millis(50 * factor), 50);
    report.budget("over", Duration::from_millis(50 * factor + 1), 50);
    report.p95("p95_within", &[1000, 2000, 50_000 * factor], 50);
    report.p95("p95_empty", &[], 50);
    report.separate("separate_over", Duration::from_millis(11), 10);
    report.require("check", true, "ok".into());
    assert_eq!(report.violations.len(), 3, "{:?}", report.violations);
    assert!(report.violations[0].starts_with("over:"));
    assert!(report.violations[1].starts_with("p95_empty:"));
    assert!(report.violations[2].starts_with("separate_over:"));
}

#[test]
fn performance_v66_fixture_stream_has_the_documented_shape() {
    let stream = fast_import_stream();
    let text = String::from_utf8_lossy(&stream);
    assert_eq!(text.matches("\ncommit refs/heads/main\n").count(), COMMITS);
    assert!(text.starts_with("blob\nmark :1\n"));
    assert_eq!(
        text.matches("M 100644 :1 wide/").count(),
        WIDE_DIRS * WIDE_FILES_PER_DIR
    );
    assert_eq!(text.matches("\nD wide\n").count(), 1);
    // Each side of the rewritten file is about 2.5 MB of unique lines: a ~5 MB diff.
    let side = huge_body("old");
    assert_eq!(side.lines().count(), HUGE_LINES);
    assert!(
        side.len() > 2_400_000 && side.len() < 2_700_000,
        "{}",
        side.len()
    );
    let old: std::collections::HashSet<&str> = side.lines().collect();
    assert_eq!(old.len(), HUGE_LINES, "every old line is unique");
    assert!(!huge_body("new").lines().any(|line| old.contains(line)));
    // Every source file exists in the working tree, the needles exactly once each.
    const { assert!(COMMITS - 4 >= SOURCE_FILES) };
    assert!(text.contains("perfneedlealpha") && text.contains("perfneedleomega"));
    assert!(source_body(ALPHA, 1).contains("perfneedlealpha"));
    assert!(!source_body(OMEGA, 1).contains("perfneedlealpha"));
    assert_eq!(source_path(4040), "src/m40/f040.rs");
}
