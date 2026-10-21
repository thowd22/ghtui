//! TASK-116: the type-to-filter log finder. Pure and bounded: it ranks the
//! lines of one already-retained log snapshot and never reads GitHub, SQL or
//! Git. Fuzzy (nucleo, smart case) is the default; exact text and regex stay
//! optional toggles that share the same bounded snapshot limits.
use crate::pr_checks_native::scoped::choices::continuations_v2::LoadedLogSearchModeV2 as Mode;
use std::time::Instant;

/// At most this many ranked lines are retained for the live list; the count
/// still reports every matching line.
pub const LIVE_CAP_V1: usize = 500;

/// Runner error markers, compiler/test failures and nonzero exits. Shared by
/// "Jump to first error" and the n/N steps that follow it.
pub const ERROR_PATTERN_V1: &str = r"##\[error\]|(?i:\berror(\[[^\]]*\])?:)|(?i:\bfatal:)|\bFAILED\b|npm ERR!|(?i:exit code [1-9][0-9]*)";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FinderHitV1 {
    /// Zero-based line index in the snapshot.
    pub line: usize,
    pub score: u32,
    /// Sorted, unique char indices of the matched characters.
    pub positions: Vec<usize>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FinderRankingV1 {
    /// Every matching line, even beyond the retained cap.
    pub total: usize,
    /// Lines in the snapshot.
    pub lines: usize,
    /// Best first (fuzzy) or in log order (exact/regex), capped.
    pub hits: Vec<FinderHitV1>,
}

fn bounded(logs: &str, query: &str) -> Result<(), String> {
    if query.len() > 1024
        || query.contains(['\n', '\r', '\0'])
        || logs.len() > 512 * 1024
        || logs.lines().count() > 5000
    {
        return Err("Log or query exceeds the bounded single-line search limits".into());
    }
    Ok(())
}

fn expired(deadline: Instant) -> Result<(), String> {
    if Instant::now() >= deadline {
        Err("Log search took too long; refine the query".into())
    } else {
        Ok(())
    }
}

pub(crate) fn bounded_regex_v1(query: &str) -> Result<regex::Regex, String> {
    regex::RegexBuilder::new(query)
        .size_limit(1024 * 1024)
        .dfa_size_limit(1024 * 1024)
        .build()
        .map_err(|_| "Regex unavailable or exceeds its bounded program".to_owned())
}

/// Whether one line matches in fuzzy mode (used by the shared indices helper).
pub(crate) struct FuzzyLineMatcherV1(crate::fuzzy_finder::FuzzyQueryV1);
impl FuzzyLineMatcherV1 {
    pub(crate) fn new(query: &str) -> Self {
        Self(crate::fuzzy_finder::FuzzyQueryV1::new(query))
    }
    pub(crate) fn is_match(&mut self, line: &str) -> bool {
        self.0.score(line).is_some()
    }
}

fn byte_ranges_to_chars(line: &str, ranges: impl Iterator<Item = (usize, usize)>) -> Vec<usize> {
    let mut out = Vec::new();
    for (start, end) in ranges {
        let first = line[..start].chars().count();
        let count = line[start..end].chars().count();
        out.extend(first..first + count);
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// Ranks matching lines for the live list. An empty query matches nothing:
/// the finder asks the user to type instead of listing the whole log.
pub fn rank_log_lines_v1(
    logs: &str,
    query: &str,
    mode: Mode,
    cap: usize,
    deadline: Instant,
) -> Result<FinderRankingV1, String> {
    bounded(logs, query)?;
    let lines = logs.lines().count();
    if query.trim().is_empty() {
        return Ok(FinderRankingV1 {
            total: 0,
            lines,
            hits: vec![],
        });
    }
    let mut hits = Vec::new();
    let mut total = 0usize;
    match mode {
        Mode::Fuzzy => {
            // The shared TASK-122 finder: lines that contain every typed word
            // as written (smart case) rank before scattered fuzzy matches, like
            // a finder user expects: "##[error]" beats "e r r o r" even when
            // nucleo's word-boundary bonuses favour the scattered letters.
            let (count, ranked) =
                crate::fuzzy_finder::rank_v1(query, logs.lines(), cap, |index| {
                    if index % 256 == 0 {
                        expired(deadline)
                    } else {
                        Ok(())
                    }
                })?;
            total = count;
            hits = ranked
                .into_iter()
                .map(|hit| FinderHitV1 {
                    line: hit.index,
                    score: hit.score,
                    positions: hit.positions,
                })
                .collect();
        }
        Mode::ExactText => {
            for (index, line) in logs.lines().enumerate() {
                if index % 256 == 0 {
                    expired(deadline)?;
                }
                if line.contains(query) {
                    total += 1;
                    if hits.len() < cap {
                        hits.push(FinderHitV1 {
                            line: index,
                            score: 0,
                            positions: byte_ranges_to_chars(
                                line,
                                line.match_indices(query).map(|(at, m)| (at, at + m.len())),
                            ),
                        });
                    }
                }
            }
        }
        Mode::Regex => {
            let regex = bounded_regex_v1(query)?;
            for (index, line) in logs.lines().enumerate() {
                if index % 256 == 0 {
                    expired(deadline)?;
                }
                if regex.is_match(line) {
                    total += 1;
                    if hits.len() < cap {
                        hits.push(FinderHitV1 {
                            line: index,
                            score: 0,
                            positions: byte_ranges_to_chars(
                                line,
                                regex
                                    .find_iter(line)
                                    .filter(|m| !m.is_empty())
                                    .map(|m| (m.start(), m.end())),
                            ),
                        });
                    }
                }
            }
        }
    }
    Ok(FinderRankingV1 { total, lines, hits })
}

/// The first line (zero-based) that looks like an error, in log order.
pub fn first_error_line_v1(logs: &str) -> Option<usize> {
    let regex = bounded_regex_v1(ERROR_PATTERN_V1).ok()?;
    logs.lines().position(|line| regex.is_match(line))
}

/// Picks the char window of `line` to show in `width` columns so the first
/// highlighted char stays visible. Returns the first char index shown.
pub fn window_start_v1(line: &str, positions: &[usize], width: usize) -> usize {
    let chars = line.chars().count();
    if chars <= width || width < 8 {
        return 0;
    }
    let first = positions.first().copied().unwrap_or(0);
    let last = positions.last().copied().unwrap_or(0);
    if last < width.saturating_sub(1) {
        return 0;
    }
    // Keep a little left context before the first match, never past the end.
    first
        .saturating_sub(width / 4)
        .min(chars.saturating_sub(width.saturating_sub(1)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn later() -> Instant {
        Instant::now() + Duration::from_secs(5)
    }

    const LOG: &str = "2026-10-21T10:00:00Z Run cargo test\n\
        compiling ghtui v0.1.1\n\
        error[E0425]: cannot find value `x` in this scope\n\
        test parser::roundtrip ... FAILED\n\
        e r r o r scattered letters\n\
        ##[error]Process completed with exit code 101.\n\
        done\n";

    #[test]
    fn log_finder_v1_fuzzy_ranks_contiguous_matches_first_and_counts_all() {
        let ranking = rank_log_lines_v1(LOG, "error", Mode::Fuzzy, 500, later()).unwrap();
        assert_eq!(ranking.lines, 7);
        // "e r r o r" also fuzzy-matches; it must rank below the real words.
        assert!(ranking.total >= 3, "{ranking:?}");
        let order: Vec<usize> = ranking.hits.iter().map(|h| h.line).collect();
        let scattered = order.iter().position(|&l| l == 4).unwrap();
        assert!(order.iter().position(|&l| l == 2).unwrap() < scattered);
        assert!(order.iter().position(|&l| l == 5).unwrap() < scattered);
        assert_eq!(ranking.hits.len(), ranking.total);
        let mut fuzzy = FuzzyLineMatcherV1::new("error");
        assert!(fuzzy.is_match("e r r o r"));
        assert!(!fuzzy.is_match("done"));
    }

    #[test]
    fn log_finder_v1_fuzzy_is_smart_case_and_highlights_matched_chars() {
        let ranking = rank_log_lines_v1(LOG, "cmpl", Mode::Fuzzy, 500, later()).unwrap();
        let hit = ranking.hits.iter().find(|h| h.line == 1).unwrap();
        let line: Vec<char> = LOG.lines().nth(1).unwrap().chars().collect();
        let picked: String = hit.positions.iter().map(|&i| line[i]).collect();
        assert_eq!(picked, "cmpl");
        // Uppercase in the query makes it case-sensitive.
        let upper = rank_log_lines_v1(LOG, "FAILED", Mode::Fuzzy, 500, later()).unwrap();
        assert_eq!(upper.total, 1);
        assert_eq!(upper.hits[0].line, 3);
        let lower = rank_log_lines_v1(LOG, "failed", Mode::Fuzzy, 500, later()).unwrap();
        assert!(lower.hits.iter().any(|h| h.line == 3));
    }

    #[test]
    fn log_finder_v1_exact_and_regex_keep_log_order_and_char_positions() {
        let text = "a 界 error\nnone\nerror error\n";
        let exact = rank_log_lines_v1(text, "error", Mode::ExactText, 500, later()).unwrap();
        assert_eq!(exact.total, 2);
        assert_eq!(exact.hits[0].line, 0);
        // Char (not byte) indices: "a 界 " is 4 chars.
        assert_eq!(exact.hits[0].positions, vec![4, 5, 6, 7, 8]);
        assert_eq!(exact.hits[1].positions.len(), 10);
        let regex = rank_log_lines_v1(text, "^n.ne$", Mode::Regex, 500, later()).unwrap();
        assert_eq!(regex.total, 1);
        assert_eq!(regex.hits[0].positions, vec![0, 1, 2, 3]);
        assert!(rank_log_lines_v1(text, "[", Mode::Regex, 500, later()).is_err());
    }

    #[test]
    fn log_finder_v1_empty_query_cap_and_bounds() {
        let empty = rank_log_lines_v1(LOG, "  ", Mode::Fuzzy, 500, later()).unwrap();
        assert_eq!((empty.total, empty.hits.len(), empty.lines), (0, 0, 7));
        let big: String = (0..3000).map(|i| format!("line {i} error\n")).collect();
        let capped = rank_log_lines_v1(&big, "error", Mode::Fuzzy, 50, later()).unwrap();
        assert_eq!(capped.total, 3000);
        assert_eq!(capped.hits.len(), 50);
        let past = Instant::now() - Duration::from_millis(1);
        assert!(rank_log_lines_v1(&big, "error", Mode::Fuzzy, 50, past).is_err());
        assert!(rank_log_lines_v1(LOG, "a\nb", Mode::Fuzzy, 50, later()).is_err());
    }

    #[test]
    fn log_finder_v1_first_error_finds_compiler_test_and_runner_markers() {
        assert_eq!(first_error_line_v1(LOG), Some(2));
        assert_eq!(
            first_error_line_v1("ok\nall good\nexit code 0\n##[error]boom\n"),
            Some(3)
        );
        assert_eq!(first_error_line_v1("ok\ntest x ... FAILED\n"), Some(1));
        assert_eq!(first_error_line_v1("Error: Process completed\n"), Some(0));
        assert_eq!(first_error_line_v1("no errors here\nerrors=0\n"), None);
    }

    #[test]
    fn log_finder_v1_window_keeps_first_match_visible() {
        let line = format!("{}needle", "x".repeat(100));
        let positions: Vec<usize> = (100..106).collect();
        let start = window_start_v1(&line, &positions, 40);
        assert!(start <= 100 && start + 40 > 105, "{start}");
        assert_eq!(window_start_v1("short", &[0], 40), 0);
    }
}
