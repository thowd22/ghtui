//! TASK-66: opt-in render-cost samples for the performance budgets.
//!
//! When `GHTUI_RENDER_TIMINGS` names a file, the event loop times every frame it draws
//! (building the frame and writing the changed cells) and writes the samples there when the
//! application exits: one `cols rows micros` line per frame. Without the variable nothing is
//! timed or written. Samples are bounded so a long session never grows without limit.
//!
//! The same module holds the percentile arithmetic the benchmark journeys use, so the code
//! that reads samples and the code that judges them against budgets agree.
use std::{
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};

/// The environment variable that turns render timing on.
pub const ENV: &str = "GHTUI_RENDER_TIMINGS";
/// At most this many frames are kept (about 1.2 MiB of samples).
pub const MAX_SAMPLES: usize = 100_000;

/// One drawn frame: its terminal size and how long drawing it took.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderSample {
    pub cols: u16,
    pub rows: u16,
    pub micros: u64,
}

/// Render-cost recorder; inert unless [`ENV`] was set at startup.
#[derive(Debug, Default)]
pub struct RenderTimings {
    path: Option<PathBuf>,
    samples: Vec<RenderSample>,
    dropped: u64,
}

impl RenderTimings {
    pub fn from_env() -> Self {
        let path = std::env::var_os(ENV)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
        Self {
            path,
            samples: Vec::new(),
            dropped: 0,
        }
    }

    pub fn enabled(&self) -> bool {
        self.path.is_some()
    }

    /// Starts timing one frame, or nothing when timing is off.
    pub fn start(&self) -> Option<Instant> {
        self.enabled().then(Instant::now)
    }

    pub fn record(&mut self, started: Option<Instant>, cols: u16, rows: u16) {
        let Some(started) = started else {
            return;
        };
        self.push(RenderSample {
            cols,
            rows,
            micros: duration_micros(started.elapsed()),
        });
    }

    fn push(&mut self, sample: RenderSample) {
        if self.samples.len() < MAX_SAMPLES {
            self.samples.push(sample);
        } else {
            self.dropped = self.dropped.saturating_add(1);
        }
    }

    pub fn samples(&self) -> &[RenderSample] {
        &self.samples
    }

    /// Writes the samples to the configured file (replacing it). A write failure is ignored:
    /// timing is a measurement aid and must never change how the application exits.
    pub fn finish(&self) {
        let Some(path) = &self.path else {
            return;
        };
        let _ = std::fs::File::create(path)
            .and_then(|mut file| file.write_all(format(&self.samples, self.dropped).as_bytes()));
    }
}

fn duration_micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

/// `cols rows micros` lines, then `dropped N` when the sample cap was reached.
pub fn format(samples: &[RenderSample], dropped: u64) -> String {
    let mut text = String::with_capacity(samples.len() * 16);
    for sample in samples {
        text.push_str(&format!(
            "{} {} {}\n",
            sample.cols, sample.rows, sample.micros
        ));
    }
    if dropped > 0 {
        text.push_str(&format!("dropped {dropped}\n"));
    }
    text
}

/// Reads [`format`] output back. Malformed lines are an error, never silently skipped.
pub fn parse(text: &str) -> Result<(Vec<RenderSample>, u64), String> {
    let mut samples = Vec::new();
    let mut dropped = 0;
    for line in text.lines() {
        if let Some(count) = line.strip_prefix("dropped ") {
            dropped = count
                .parse()
                .map_err(|_| format!("bad dropped line: {line:?}"))?;
            continue;
        }
        let fields: Vec<&str> = line.split(' ').collect();
        let [cols, rows, micros] = fields.as_slice() else {
            return Err(format!("bad sample line: {line:?}"));
        };
        let bad = |_| format!("bad sample line: {line:?}");
        samples.push(RenderSample {
            cols: cols.parse().map_err(bad)?,
            rows: rows.parse().map_err(bad)?,
            micros: micros.parse().map_err(bad)?,
        });
    }
    Ok((samples, dropped))
}

/// Summary statistics over a set of samples (any unit).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Stats {
    pub count: usize,
    pub min: u64,
    pub p50: u64,
    pub p95: u64,
    pub max: u64,
}

/// Nearest-rank percentile (`p` in 0..=100) of `samples`; `None` when there are none.
pub fn percentile(samples: &[u64], p: f64) -> Option<u64> {
    if samples.is_empty() || !(0.0..=100.0).contains(&p) {
        return None;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let rank = ((p / 100.0) * sorted.len() as f64).ceil() as usize;
    Some(sorted[rank.clamp(1, sorted.len()) - 1])
}

pub fn stats(samples: &[u64]) -> Option<Stats> {
    Some(Stats {
        count: samples.len(),
        min: *samples.iter().min()?,
        p50: percentile(samples, 50.0)?,
        p95: percentile(samples, 95.0)?,
        max: *samples.iter().max()?,
    })
}
