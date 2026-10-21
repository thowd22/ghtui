//! Embedded ripgrep searcher/matcher over canonical bounded source documents.
use super::{
    corpus::{Corpus, CorpusLimits, RepositorySelection},
    model::{Coverage, Document, SearchHit},
    Query, QueryError, ScanLimits, SearchMode,
};
use crate::{providers::runner::CancellationToken, storage::Scope};
use grep_matcher::Matcher;
use grep_regex::RegexMatcherBuilder;
use grep_searcher::{sinks, SearcherBuilder};

#[derive(Debug, Clone)]
pub struct SearchPage {
    pub query: Query,
    pub hits: Vec<SearchHit>,
    pub coverage: Coverage,
}

fn compile(query: &Query, limits: &ScanLimits) -> Result<grep_regex::RegexMatcher, QueryError> {
    if !matches!(query.mode, SearchMode::Exact | SearchMode::Regex) {
        return Err(QueryError("Choose Exact or Regex for a local content scan; ranked and remote search use their own providers".into()));
    }
    if query.text.is_empty() {
        return Err(QueryError("Enter text to search".into()));
    }
    if limits.max_matches == 0 {
        return Err(QueryError("Match budget must be greater than zero".into()));
    }
    let pattern = if query.mode == SearchMode::Exact {
        regex::escape(&query.text)
    } else {
        query.text.clone()
    };
    RegexMatcherBuilder::new()
        .multi_line(true)
        .unicode(true)
        .size_limit(1024 * 1024)
        .dfa_size_limit(8 * 1024 * 1024)
        .build(&pattern)
        .map_err(|error| QueryError(format!("Invalid or excessive regex: {error}")))
}

pub fn exact_search(
    corpus: &Corpus,
    query: &Query,
    limits: &ScanLimits,
    cancellation: &CancellationToken,
) -> Result<SearchPage, QueryError> {
    let matcher = compile(query, limits)?;
    let mut page = SearchPage {
        query: query.clone(),
        hits: Vec::new(),
        coverage: corpus.coverage.clone(),
    };
    let mut total_bytes = 0usize;
    for document in &corpus.documents {
        if !scan_document(
            &matcher,
            &mut page,
            document,
            limits,
            &mut total_bytes,
            cancellation,
        )? {
            break;
        }
    }
    sort_hits(&mut page);
    Ok(page)
}

/// TASK-112: Exact/Regex over the selected checkouts that reads and scans one document at a
/// time and stops reading once the match budget (the first page) is full. The hits are the
/// same as [`Corpus::collect`] followed by [`exact_search`]: selections are visited by
/// repository and files in sorted path order, which is the collected corpus's order, so the
/// documents left unread could only have produced later matches. Coverage counts only what
/// was read and says how many enumerated files were left unread.
pub fn exact_search_streaming(
    scope: Scope,
    selections: &[RepositorySelection],
    corpus_limits: &CorpusLimits,
    query: &Query,
    limits: &ScanLimits,
    cancellation: &CancellationToken,
) -> Result<SearchPage, QueryError> {
    let matcher = compile(query, limits)?;
    let mut ordered = selections.to_vec();
    ordered.sort_by(|a, b| a.repository.cmp(&b.repository));
    let mut page = SearchPage {
        query: query.clone(),
        hits: Vec::new(),
        // Only the scan's own notes; the corpus coverage replaces it below.
        coverage: Coverage {
            complete: true,
            ..Default::default()
        },
    };
    let mut total_bytes = 0usize;
    let mut failure = None;
    let corpus = Corpus::collect_with(
        scope,
        &ordered,
        corpus_limits,
        cancellation,
        &mut |documents| {
            let Some(document) = documents.pop() else {
                return true;
            };
            match scan_document(
                &matcher,
                &mut page,
                &document,
                limits,
                &mut total_bytes,
                cancellation,
            ) {
                Ok(more) => more,
                Err(error) => {
                    failure = Some(error);
                    false
                }
            }
        },
    )?;
    if let Some(error) = failure {
        return Err(error);
    }
    // The scan's own budget notes (match/byte budget, cancellation) join the corpus coverage.
    let scan = std::mem::take(&mut page.coverage);
    page.coverage = corpus.coverage;
    page.coverage.complete &= scan.complete;
    page.coverage.cancelled |= scan.cancelled;
    page.coverage.skipped_large += scan.skipped_large;
    page.coverage.reasons.extend(scan.reasons);
    sort_hits(&mut page);
    Ok(page)
}

/// Scans one document into `page`; `false` once the scan must stop (match or byte budget
/// reached, or cancelled).
fn scan_document(
    matcher: &grep_regex::RegexMatcher,
    page: &mut SearchPage,
    document: &Document,
    limits: &ScanLimits,
    total_bytes: &mut usize,
    cancellation: &CancellationToken,
) -> Result<bool, QueryError> {
    let exact = page.query.mode == SearchMode::Exact;
    // Construct lazily so files with no matches do not duplicate their source body.
    let mut shared_document = None;
    if cancellation.is_cancelled() {
        page.coverage.cancelled = true;
        page.coverage.complete = false;
        return Ok(false);
    }
    if !document.matches(&page.query) {
        return Ok(true);
    }
    if document.body.len() > limits.max_file_bytes {
        page.coverage.skipped_large += 1;
        page.coverage.complete = false;
        return Ok(true);
    }
    *total_bytes = total_bytes.saturating_add(document.body.len());
    if *total_bytes > limits.max_total_bytes {
        page.coverage.complete = false;
        page.coverage
            .reasons
            .push("Exact/Regex scan reached its total-byte budget".into());
        return Ok(false);
    }
    let mut searcher = SearcherBuilder::new()
        .line_number(true)
        .multi_line(true)
        .bom_sniffing(false)
        .heap_limit(Some(limits.max_file_bytes))
        .build();
    searcher
        .search_slice(
            matcher,
            document.body.as_bytes(),
            sinks::UTF8(|first_line, matched: &str| {
                if cancellation.is_cancelled() {
                    page.coverage.cancelled = true;
                    page.coverage.complete = false;
                    return Ok(false);
                }
                matcher
                    .find_iter(matched.as_bytes(), |found| {
                        if page.hits.len() >= limits.max_matches {
                            page.coverage.complete = false;
                            return false;
                        }
                        let prefix = &matched[..found.start()];
                        let line = first_line as usize
                            + prefix.bytes().filter(|byte| *byte == b'\n').count();
                        let line_start = prefix.rfind('\n').map_or(0, |index| index + 1);
                        let column = matched[line_start..found.start()].chars().count() + 1;
                        let line_end = matched[found.start()..]
                            .find('\n')
                            .map_or(matched.len(), |index| found.start() + index);
                        let whole_line = &matched[line_start..line_end];
                        let relative = found.start() - line_start;
                        let start = whole_line[..relative]
                            .char_indices()
                            .rev()
                            .nth(80)
                            .map_or(0, |(index, _)| index);
                        let end = whole_line[relative..]
                            .char_indices()
                            .nth(240)
                            .map_or(whole_line.len(), |(index, _)| relative + index);
                        let snippet = whole_line[start..end].to_owned();
                        let from = relative - start;
                        let to = (found.end().min(line_end) - line_start).min(end) - start;
                        page.hits.push(SearchHit {
                            document: std::sync::Arc::clone(
                                shared_document
                                    .get_or_insert_with(|| std::sync::Arc::new(document.clone())),
                            ),
                            line,
                            column,
                            highlights: std::iter::once(from..to).collect(),
                            snippet,
                            score: 1.0,
                            matched_fields: vec![if exact {
                                "body literal".into()
                            } else {
                                "body regex".into()
                            }],
                        });
                        !cancellation.is_cancelled() && page.hits.len() < limits.max_matches
                    })
                    .map_err(|error| std::io::Error::other(format!("Matcher failed: {error}")))?;
                Ok(!cancellation.is_cancelled() && page.hits.len() < limits.max_matches)
            }),
        )
        .map_err(|error| QueryError(format!("Content scan failed: {error}")))?;
    if cancellation.is_cancelled() {
        page.coverage.cancelled = true;
        page.coverage.complete = false;
        return Ok(false);
    }
    if page.hits.len() >= limits.max_matches {
        page.coverage.complete = false;
        page.coverage.reasons.push(
            "Exact/Regex scan reached its match budget; narrow scope or raise the explicit limit"
                .into(),
        );
        return Ok(false);
    }
    Ok(true)
}

/// At most this many lines of one document stay in a fuzzy page, so one noisy file never
/// pushes every other file off the list.
pub const FUZZY_HITS_PER_DOCUMENT: usize = 20;
/// Context kept before the first highlighted character of a fuzzy snippet.
const FUZZY_LEAD: usize = 16;
/// Characters a fuzzy snippet keeps.
const FUZZY_SNIPPET: usize = 240;

/// TASK-121: the fuzzy ranking over the same bounded documents as Exact/Regex. Every document
/// is read (a ranking cannot stop at the first page); each one's file name and lines are
/// ranked by the shared [`crate::fuzzy_rank_v1`] finder, at most
/// [`FUZZY_HITS_PER_DOCUMENT`] lines per document, the best `max_matches` overall.
struct FuzzyScan {
    ranker: crate::fuzzy_rank_v1::FuzzyRankerV1,
    hits: Vec<(crate::fuzzy_rank_v1::FuzzyMatchV1, SearchHit)>,
    total: usize,
    total_bytes: usize,
}
impl FuzzyScan {
    fn new(query: &Query) -> Result<Self, QueryError> {
        if query.mode != SearchMode::Fuzzy {
            return Err(QueryError("Fuzzy ranking needs Fuzzy mode".into()));
        }
        let ranker = crate::fuzzy_rank_v1::FuzzyRankerV1::new(&query.text);
        if ranker.is_empty() {
            return Err(QueryError("Enter text to search".into()));
        }
        Ok(Self {
            ranker,
            hits: Vec::new(),
            total: 0,
            total_bytes: 0,
        })
    }

    fn snippet(&mut self, text: &str) -> (String, Vec<std::ops::Range<usize>>, usize) {
        let positions = self.ranker.positions(text);
        let first = positions.first().copied().unwrap_or(0);
        let chars = text.char_indices().map(|(at, _)| at).collect::<Vec<_>>();
        let mut start = first.saturating_sub(FUZZY_LEAD);
        while start < first && text[chars[start]..].starts_with(char::is_whitespace) {
            start += 1;
        }
        let end = (start + FUZZY_SNIPPET).min(chars.len());
        let byte = |index: usize| chars.get(index).copied().unwrap_or(text.len());
        let snippet = text[byte(start)..byte(end)].to_owned();
        let shifted = positions
            .iter()
            .filter(|&&position| position >= start && position < end)
            .map(|position| position - start)
            .collect::<Vec<_>>();
        let ranges = crate::fuzzy_rank_v1::char_positions_to_byte_ranges(&snippet, &shifted);
        (snippet, ranges, first + 1)
    }

    /// Ranks one document; `false` once the scan must stop (byte budget or cancelled).
    fn document(
        &mut self,
        page: &mut SearchPage,
        document: &Document,
        limits: &ScanLimits,
        cancellation: &CancellationToken,
    ) -> bool {
        if cancellation.is_cancelled() {
            page.coverage.cancelled = true;
            page.coverage.complete = false;
            return false;
        }
        if !document.matches(&page.query) {
            return true;
        }
        if document.body.len() > limits.max_file_bytes {
            page.coverage.skipped_large += 1;
            page.coverage.complete = false;
            return true;
        }
        self.total_bytes = self.total_bytes.saturating_add(document.body.len());
        if self.total_bytes > limits.max_total_bytes {
            page.coverage.complete = false;
            page.coverage
                .reasons
                .push("Fuzzy search reached its total-byte budget".into());
            return false;
        }
        let mut found = Vec::new();
        // A file's own name is a result too (line 0 marks it).
        let file = matches!(
            document.destination,
            super::model::Destination::LocalFile { .. }
        );
        if file {
            if let Some(rank) = self.ranker.rank(&document.title) {
                found.push((rank, 0usize));
            }
        }
        for (index, line) in document.body.lines().enumerate() {
            if index % 1024 == 0 && cancellation.is_cancelled() {
                page.coverage.cancelled = true;
                page.coverage.complete = false;
                return false;
            }
            if line.len() > 64 * 1024 {
                continue;
            }
            if let Some(rank) = self.ranker.rank(line) {
                found.push((rank, index + 1));
            }
        }
        if found.is_empty() {
            return true;
        }
        self.total += found.len();
        found.sort_by(|(a, la), (b, lb)| a.better(b).then(la.cmp(lb)));
        found.truncate(FUZZY_HITS_PER_DOCUMENT);
        let shared = std::sync::Arc::new(document.clone());
        let lines = document.body.lines().collect::<Vec<_>>();
        for (rank, line) in found {
            let text = if line == 0 {
                document.title.as_str()
            } else {
                lines[line - 1]
            };
            let (snippet, highlights, column) = self.snippet(text);
            self.hits.push((
                rank,
                SearchHit {
                    document: std::sync::Arc::clone(&shared),
                    line: line.max(1),
                    column: if line == 0 { 1 } else { column },
                    highlights,
                    snippet,
                    score: rank.score as f32 + if rank.whole { 1_000_000.0 } else { 0.0 },
                    matched_fields: vec![if line == 0 {
                        FUZZY_FILE_NAME.into()
                    } else {
                        FUZZY_TEXT.into()
                    }],
                },
            ));
        }
        // Keep memory bounded: drop the worst once well past the page.
        if self.hits.len() > limits.max_matches.saturating_mul(2).max(64) {
            self.hits.sort_by(|(a, _), (b, _)| a.better(b));
            self.hits.truncate(limits.max_matches);
        }
        true
    }

    fn finish(mut self, page: &mut SearchPage, limits: &ScanLimits) {
        self.hits.sort_by(|(a, _), (b, _)| a.better(b));
        if self.hits.len() > limits.max_matches {
            self.hits.truncate(limits.max_matches);
        }
        if self.total > self.hits.len() {
            page.coverage.reasons.push(format!(
                "Best {} of {} fuzzy matches shown; type more to narrow",
                self.hits.len(),
                self.total
            ));
        }
        // Grouped by file: files in order of their best line, each file's name first and then
        // its lines in file order.
        let mut order: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
        for (position, (_, hit)) in self.hits.iter().enumerate() {
            let key = std::sync::Arc::as_ptr(&hit.document) as usize;
            order.entry(key).or_insert(position);
        }
        let mut hits = self.hits;
        hits.sort_by(|(_, a), (_, b)| {
            let (ka, kb) = (
                std::sync::Arc::as_ptr(&a.document) as usize,
                std::sync::Arc::as_ptr(&b.document) as usize,
            );
            order[&ka].cmp(&order[&kb]).then_with(|| {
                let name = |hit: &SearchHit| hit.matched_fields[0] != FUZZY_FILE_NAME;
                name(a).cmp(&name(b)).then(a.line.cmp(&b.line))
            })
        });
        page.hits = hits.into_iter().map(|(_, hit)| hit).collect();
    }
}

/// Why a fuzzy result matched, shown in its preview.
pub const FUZZY_FILE_NAME: &str = "file name";
pub const FUZZY_TEXT: &str = "fuzzy text";

/// TASK-121: fuzzy ranking over a collected corpus (saved versions, cached conversations and
/// commit messages included).
pub fn fuzzy_search(
    corpus: &Corpus,
    query: &Query,
    limits: &ScanLimits,
    cancellation: &CancellationToken,
) -> Result<SearchPage, QueryError> {
    let mut scan = FuzzyScan::new(query)?;
    let mut page = SearchPage {
        query: query.clone(),
        hits: Vec::new(),
        coverage: corpus.coverage.clone(),
    };
    for document in &corpus.documents {
        if !scan.document(&mut page, document, limits, cancellation) {
            break;
        }
    }
    scan.finish(&mut page, limits);
    Ok(page)
}

/// TASK-121: fuzzy ranking over the selected working files that reads one document at a time
/// (nothing is retained beyond its ranked lines). Every file is read: a ranking cannot stop
/// at the first page.
pub fn fuzzy_search_streaming(
    scope: Scope,
    selections: &[RepositorySelection],
    corpus_limits: &CorpusLimits,
    query: &Query,
    limits: &ScanLimits,
    cancellation: &CancellationToken,
) -> Result<SearchPage, QueryError> {
    let mut scan = FuzzyScan::new(query)?;
    let mut ordered = selections.to_vec();
    ordered.sort_by(|a, b| a.repository.cmp(&b.repository));
    let mut page = SearchPage {
        query: query.clone(),
        hits: Vec::new(),
        coverage: Coverage {
            complete: true,
            ..Default::default()
        },
    };
    let corpus = Corpus::collect_with(
        scope,
        &ordered,
        corpus_limits,
        cancellation,
        &mut |documents| {
            let Some(document) = documents.pop() else {
                return true;
            };
            scan.document(&mut page, &document, limits, cancellation)
        },
    )?;
    let own = std::mem::take(&mut page.coverage);
    page.coverage = corpus.coverage;
    page.coverage.complete &= own.complete;
    page.coverage.cancelled |= own.cancelled;
    page.coverage.skipped_large += own.skipped_large;
    page.coverage.reasons.extend(own.reasons);
    scan.finish(&mut page, limits);
    Ok(page)
}

fn sort_hits(page: &mut SearchPage) {
    page.hits.sort_by(|a, b| {
        (
            &a.document.key.repository,
            &a.document.title,
            a.line,
            a.column,
        )
            .cmp(&(
                &b.document.key.repository,
                &b.document.title,
                b.line,
                b.column,
            ))
    });
}
