//! Undeclared pure prepared search. Actual capture Arc, never Form/draft/auth.
use crate::providers::{gh::PrCheckLogsObservation, runner::CancellationToken};
use std::{sync::Arc, time::Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadedLogSearchModeV2 {
    ExactText,
    Regex,
}
pub struct LoadedLogMatchesV2 {
    capture: Arc<PrCheckLogsObservation>,
    query: String,
    mode: LoadedLogSearchModeV2,
    indices: Vec<usize>,
}
impl std::fmt::Debug for LoadedLogMatchesV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LoadedLogMatchesV2(<bounded captured snapshot; query redacted>)")
    }
}
impl LoadedLogMatchesV2 {
    pub fn capture(&self) -> &Arc<PrCheckLogsObservation> {
        &self.capture
    }
    pub fn query(&self) -> &str {
        &self.query
    }
    pub fn mode(&self) -> LoadedLogSearchModeV2 {
        self.mode
    }
    pub fn indices(&self) -> &[usize] {
        &self.indices
    }
    pub(crate) fn prepare(
        capture: Arc<PrCheckLogsObservation>,
        query: String,
        mode: LoadedLogSearchModeV2,
        deadline: Instant,
        cancel: &CancellationToken,
    ) -> Result<Self, String> {
        if query.len() > 1024
            || query.contains(['\n', '\r', '\0'])
            || capture.logs().len() > 512 * 1024
            || capture.logs().lines().count() > 5000
        {
            return Err("Loaded-log query/snapshot exceeds bounded single-line limits".into());
        }
        let check = || {
            if cancel.is_cancelled() || Instant::now() >= deadline {
                Err("Original loaded-log preparation expired; no new snapshot searched".to_owned())
            } else {
                Ok(())
            }
        };
        check()?;
        let regex = if mode == LoadedLogSearchModeV2::Regex {
            Some(
                regex::RegexBuilder::new(&query)
                    .size_limit(1024 * 1024)
                    .dfa_size_limit(1024 * 1024)
                    .build()
                    .map_err(|_| {
                        "Regex unavailable or exceeds its bounded program; original logs unchanged"
                            .to_owned()
                    })?,
            )
        } else {
            None
        };
        check()?;
        let mut indices = Vec::new();
        for (index, line) in capture.logs().lines().enumerate() {
            check()?;
            if regex
                .as_ref()
                .map_or(line.contains(&query), |r| r.is_match(line))
            {
                indices.push(index);
            }
        }
        check()?;
        Ok(Self {
            capture,
            query,
            mode,
            indices,
        })
    }
    pub fn belongs_to(&self, capture: &Arc<PrCheckLogsObservation>) -> bool {
        Arc::ptr_eq(&self.capture, capture)
    }
}
