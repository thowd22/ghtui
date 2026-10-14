//! Embedded ripgrep searcher/matcher over canonical bounded source documents.
use super::{
    corpus::Corpus,
    model::{Coverage, SearchHit},
    Query, QueryError, ScanLimits, SearchMode,
};
use crate::providers::runner::CancellationToken;
use grep_matcher::Matcher;
use grep_regex::RegexMatcherBuilder;
use grep_searcher::{sinks, SearcherBuilder};

#[derive(Debug, Clone)]
pub struct SearchPage {
    pub query: Query,
    pub hits: Vec<SearchHit>,
    pub coverage: Coverage,
}

pub fn exact_search(
    corpus: &Corpus,
    query: &Query,
    limits: &ScanLimits,
    cancellation: &CancellationToken,
) -> Result<SearchPage, QueryError> {
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
    let matcher = RegexMatcherBuilder::new()
        .multi_line(true)
        .unicode(true)
        .size_limit(1024 * 1024)
        .dfa_size_limit(8 * 1024 * 1024)
        .build(&pattern)
        .map_err(|error| QueryError(format!("Invalid or excessive regex: {error}")))?;
    let mut page = SearchPage {
        query: query.clone(),
        hits: Vec::new(),
        coverage: corpus.coverage.clone(),
    };
    let mut total_bytes = 0usize;
    for document in &corpus.documents {
        // Construct lazily so files with no matches do not duplicate their source body.
        let mut shared_document = None;
        if cancellation.is_cancelled() {
            page.coverage.cancelled = true;
            page.coverage.complete = false;
            break;
        }
        if !document.matches(query) {
            continue;
        }
        if document.body.len() > limits.max_file_bytes {
            page.coverage.skipped_large += 1;
            page.coverage.complete = false;
            continue;
        }
        total_bytes = total_bytes.saturating_add(document.body.len());
        if total_bytes > limits.max_total_bytes {
            page.coverage.complete = false;
            page.coverage
                .reasons
                .push("Exact/Regex scan reached its total-byte budget".into());
            break;
        }
        let mut searcher = SearcherBuilder::new()
            .line_number(true)
            .multi_line(true)
            .bom_sniffing(false)
            .heap_limit(Some(limits.max_file_bytes))
            .build();
        searcher
            .search_slice(
                &matcher,
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
                                    shared_document.get_or_insert_with(|| {
                                        std::sync::Arc::new(document.clone())
                                    }),
                                ),
                                line,
                                column,
                                highlights: std::iter::once(from..to).collect(),
                                snippet,
                                score: 1.0,
                                matched_fields: vec![if query.mode == SearchMode::Exact {
                                    "body literal".into()
                                } else {
                                    "body regex".into()
                                }],
                            });
                            !cancellation.is_cancelled() && page.hits.len() < limits.max_matches
                        })
                        .map_err(|error| {
                            std::io::Error::other(format!("Matcher failed: {error}"))
                        })?;
                    Ok(!cancellation.is_cancelled() && page.hits.len() < limits.max_matches)
                }),
            )
            .map_err(|error| QueryError(format!("Content scan failed: {error}")))?;
        if cancellation.is_cancelled() {
            page.coverage.cancelled = true;
            page.coverage.complete = false;
            break;
        }
        if page.hits.len() >= limits.max_matches {
            page.coverage.complete = false;
            page.coverage.reasons.push("Exact/Regex scan reached its match budget; narrow scope or raise the explicit limit".into());
            break;
        }
    }
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
    Ok(page)
}
