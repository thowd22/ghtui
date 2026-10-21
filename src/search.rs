//! Shared query semantics and bounded content matching. Ranked index integration is separate.
use std::{collections::BTreeMap, fmt, ops::Range};

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

pub mod collaboration;
pub mod commits;
pub mod corpus;
pub mod fusion;
pub mod fuzzy;
pub mod hydration;
pub mod index;
pub mod managed;
pub mod model;
pub mod remote;
pub mod scan;
pub mod sources;
pub mod tokenizer;
pub mod workspace;

#[cfg(test)]
mod relevance_quality_tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchMode {
    Ranked,
    Exact,
    Regex,
    GitHub,
    /// TASK-121: the default. Live, fuzzy-ranked (smart case, all words first) over file
    /// names, file lines and loaded conversations of the same local scope as Exact.
    Fuzzy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Query {
    pub mode: SearchMode,
    pub text: String,
    pub terms: Vec<String>,
    pub filters: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryError(pub String);
impl fmt::Display for QueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for QueryError {}

impl Query {
    pub fn parse(input: &str, mode: SearchMode) -> Result<Self, QueryError> {
        if input.len() > 16 * 1024 {
            return Err(QueryError("Search query is too long".into()));
        }
        let mut query = Self {
            mode,
            text: String::new(),
            terms: vec![],
            filters: BTreeMap::new(),
        };
        for (token, quoted) in tokenize(input)? {
            if let Some((name, value)) = token.split_once(':') {
                if [
                    "repo", "org", "set", "type", "path", "language", "author", "label", "state",
                    "after", "before", "rev",
                ]
                .contains(&name)
                {
                    if value.is_empty() {
                        return Err(QueryError(format!("Filter {name} needs a value")));
                    }
                    if matches!(name, "after" | "before") && !valid_date(value) {
                        return Err(QueryError(format!("{name} needs a YYYY-MM-DD date")));
                    }
                    if name == "state" && !["open", "closed", "merged", "all"].contains(&value) {
                        return Err(QueryError(
                            "State must be open, closed, merged, or all".into(),
                        ));
                    }
                    if name == "type"
                        && ![
                            "file",
                            "commit",
                            "pr",
                            "issue",
                            "comment",
                            "run",
                            "workflow",
                            "repository",
                        ]
                        .contains(&value)
                    {
                        return Err(QueryError(format!("Unknown result type '{value}'")));
                    }
                    query
                        .filters
                        .entry(name.into())
                        .or_default()
                        .push(value.into());
                    continue;
                }
                if !quoted
                    && !token.contains("::")
                    && name.chars().all(char::is_alphabetic)
                    && !value.starts_with('/')
                {
                    return Err(QueryError(format!(
                        "Unknown filter '{name}'; quote a literal containing ':'"
                    )));
                }
            }
            query.terms.push(token);
        }
        query.text = query.terms.join(" ");
        if let (Some(after), Some(before)) = (query.first("after"), query.first("before")) {
            if after > before {
                return Err(QueryError("The start date is after the end date".into()));
            }
        }
        if mode == SearchMode::Regex && !query.text.is_empty() {
            Pattern::compile(&query)?;
        }
        Ok(query)
    }

    pub fn first(&self, filter: &str) -> Option<&str> {
        self.filters.get(filter)?.first().map(String::as_str)
    }

    /// Explicit provider semantics: unsupported constraints are errors, never dropped.
    pub fn github_query(&self, kind: RemoteKind) -> Result<String, QueryError> {
        if self.mode == SearchMode::Regex {
            return Err(QueryError(
                "GitHub API code search does not support regex; use local Regex mode".into(),
            ));
        }
        let allowed = match kind {
            RemoteKind::Code => &["repo", "org", "path", "language"][..],
            RemoteKind::PullRequests | RemoteKind::Issues => {
                &["repo", "org", "author", "label", "state", "after", "before"][..]
            }
            RemoteKind::Repositories => &["org", "language"][..],
        };
        let mut parts = self
            .terms
            .iter()
            .map(|term| quote_term(term))
            .collect::<Vec<_>>();
        for (name, values) in &self.filters {
            if !allowed.contains(&name.as_str()) {
                return Err(QueryError(format!("Filter '{name}' is unavailable for this GitHub search; choose a compatible mode")));
            }
            for value in values {
                let term = match name.as_str() {
                    "after" => format!("created:>={value}"),
                    "before" => format!("created:<={value}"),
                    "state" if value == "all" => continue,
                    "state" => format!("is:{value}"),
                    _ => format!("{name}:{}", quote_term(value)),
                };
                parts.push(term);
            }
        }
        Ok(parts.join(" "))
    }
}

#[derive(Debug, Clone, Copy)]
pub enum RemoteKind {
    Code,
    PullRequests,
    Issues,
    Repositories,
}

fn quote_term(value: &str) -> String {
    if value.chars().any(char::is_whitespace) {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        value.to_owned()
    }
}

fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    if bytes
        .iter()
        .enumerate()
        .any(|(index, b)| index != 4 && index != 7 && !b.is_ascii_digit())
    {
        return false;
    }
    let year = value[..4].parse::<u32>().unwrap_or(0);
    let month = value[5..7].parse::<usize>().unwrap_or(0);
    let day = value[8..].parse::<u32>().unwrap_or(0);
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    year > 0 && (1..=12).contains(&month) && day > 0 && day <= days[month - 1]
}

fn tokenize(input: &str) -> Result<Vec<(String, bool)>, QueryError> {
    let mut output = Vec::new();
    let mut token = String::new();
    let mut quoted = false;
    let mut quote = None;
    let mut chars = input.chars().peekable();
    while let Some(character) = chars.next() {
        if let Some(end) = quote {
            if character == end {
                quote = None;
            } else if character == '\\'
                && chars
                    .peek()
                    .is_some_and(|next| *next == end || *next == '\\')
            {
                token.push(chars.next().unwrap());
            } else {
                token.push(character);
            }
        } else if character == '"' || character == '\'' {
            quote = Some(character);
            quoted = true;
        } else if character.is_whitespace() {
            if !token.is_empty() {
                output.push((std::mem::take(&mut token), quoted));
                quoted = false;
            }
        } else {
            token.push(character);
        }
    }
    if quote.is_some() {
        return Err(QueryError("Unclosed quoted search text".into()));
    }
    if !token.is_empty() {
        output.push((token, quoted));
    }
    Ok(output)
}

pub struct Pattern {
    regex: Regex,
}
impl Pattern {
    pub fn compile(query: &Query) -> Result<Self, QueryError> {
        if query.text.is_empty() {
            return Err(QueryError("Enter text to search".into()));
        }
        if !matches!(query.mode, SearchMode::Exact | SearchMode::Regex) {
            return Err(QueryError(
                "Content scanning needs Exact or Regex mode".into(),
            ));
        }
        let pattern = if query.mode == SearchMode::Exact {
            regex::escape(&query.text)
        } else {
            query.text.clone()
        };
        let regex = RegexBuilder::new(&pattern)
            .size_limit(1024 * 1024)
            .dfa_size_limit(8 * 1024 * 1024)
            .build()
            .map_err(|error| QueryError(format!("Invalid or excessive regex: {error}")))?;
        Ok(Self { regex })
    }
    pub fn find<'a>(&'a self, line: &'a str) -> impl Iterator<Item = Range<usize>> + 'a {
        self.regex.find_iter(line).map(|matched| matched.range())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceDocument {
    pub host: String,
    pub login: String,
    pub repository: String,
    pub revision: String,
    pub path: String,
    pub kind: String,
    pub text: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub document: usize,
    pub line: usize,
    pub column: usize,
    pub range: Range<usize>,
    pub preview: String,
}
#[derive(Debug, Clone)]
pub struct ScanLimits {
    pub max_file_bytes: usize,
    pub max_total_bytes: usize,
    pub max_matches: usize,
}
impl Default for ScanLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: 2 * 1024 * 1024,
            max_total_bytes: 64 * 1024 * 1024,
            max_matches: 1000,
        }
    }
}
#[derive(Debug, Default)]
pub struct ScanResult {
    pub hits: Vec<Hit>,
    pub scanned: usize,
    pub skipped_large: usize,
    pub skipped_binary: usize,
    pub complete: bool,
    pub cancelled: bool,
}

pub fn scan_documents(
    query: &Query,
    documents: &[SourceDocument],
    limits: &ScanLimits,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ScanResult, QueryError> {
    for filter in query.filters.keys() {
        if !["repo", "org", "type", "rev"].contains(&filter.as_str()) {
            return Err(QueryError(format!(
                "Filter '{filter}' needs source metadata support before this scan can run"
            )));
        }
    }
    let pattern = Pattern::compile(query)?;
    let mut result = ScanResult {
        complete: true,
        ..Default::default()
    };
    let mut bytes = 0usize;
    for (index, document) in documents.iter().enumerate() {
        if cancelled() {
            result.cancelled = true;
            result.complete = false;
            break;
        }
        if !matches_document(query, document) {
            continue;
        }
        if document.text.contains('\0') {
            result.skipped_binary += 1;
            continue;
        }
        if document.text.len() > limits.max_file_bytes {
            result.skipped_large += 1;
            result.complete = false;
            continue;
        }
        bytes = bytes.saturating_add(document.text.len());
        if bytes > limits.max_total_bytes {
            result.complete = false;
            break;
        }
        result.scanned += 1;
        for (line_index, line) in document.text.lines().enumerate() {
            if cancelled() {
                result.cancelled = true;
                result.complete = false;
                return Ok(result);
            }
            for range in pattern.find(line) {
                if result.hits.len() >= limits.max_matches {
                    result.complete = false;
                    return Ok(result);
                }
                result.hits.push(Hit {
                    document: index,
                    line: line_index + 1,
                    column: line[..range.start].chars().count() + 1,
                    range,
                    preview: line.into(),
                });
            }
        }
    }
    Ok(result)
}

fn matches_document(query: &Query, document: &SourceDocument) -> bool {
    for (filter, values) in &query.filters {
        let actual = match filter.as_str() {
            "repo" => document.repository.as_str(),
            "type" => document.kind.as_str(),
            "rev" => document.revision.as_str(),
            "org" => document.repository.split('/').next().unwrap_or_default(),
            // Other constraints require provider metadata; never silently ignore them.
            _ => return false,
        };
        if !values.iter().any(|value| value == actual) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    fn doc(repository: &str, path: &str, text: &str) -> SourceDocument {
        SourceDocument {
            host: "github.com".into(),
            login: "alice".into(),
            repository: repository.into(),
            revision: "abc".into(),
            path: path.into(),
            kind: "file".into(),
            text: text.into(),
        }
    }

    #[test]
    fn query_modes_parse_phrases_identifiers_and_scopes() {
        let query = Query::parse(
            "repo:org/a type:file \"error handling\" std::io::Error",
            SearchMode::Ranked,
        )
        .unwrap();
        assert_eq!(query.terms, ["error handling", "std::io::Error"]);
        assert_eq!(query.first("repo"), Some("org/a"));
        assert!(Query::parse("after:2026-02-30 auth", SearchMode::Ranked).is_err());
        assert!(Query::parse(
            "after:2026-10-12 before:2026-01-01 auth",
            SearchMode::Ranked
        )
        .is_err());
        assert!(Query::parse("\"unfinished", SearchMode::Exact).is_err());
        assert!(Query::parse("typo:value", SearchMode::Exact).is_err());
    }
    #[test]
    fn query_modes_remote_rejects_unsupported_constraints() {
        let query = Query::parse("repo:org/a rev:abc auth", SearchMode::GitHub).unwrap();
        assert!(query
            .github_query(RemoteKind::Code)
            .unwrap_err()
            .0
            .contains("rev"));
        let regex = Query::parse("auth\\d+", SearchMode::Regex).unwrap();
        assert!(regex.github_query(RemoteKind::Code).is_err());
        let query = Query::parse(
            "repo:org/a author:alice state:open \"error handling\"",
            SearchMode::GitHub,
        )
        .unwrap();
        let remote = query.github_query(RemoteKind::PullRequests).unwrap();
        assert!(remote.contains("\"error handling\""));
        assert!(remote.contains("is:open"));
    }
    #[test]
    fn exact_search_preserves_literal_regex_unicode_and_line_coordinates() {
        let documents = [
            doc("org/a", "src/a.rs", "日本語 foo.bar\nfooXbar\n"),
            doc("org/b", "b.rs", "foo.bar"),
        ];
        let query = Query::parse("repo:org/a foo.bar", SearchMode::Exact).unwrap();
        let result = scan_documents(&query, &documents, &ScanLimits::default(), || false).unwrap();
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].column, 5);
        assert_eq!(result.hits[0].line, 1);
        let query = Query::parse("repo:org/a foo.bar", SearchMode::Regex).unwrap();
        assert_eq!(
            scan_documents(&query, &documents, &ScanLimits::default(), || false)
                .unwrap()
                .hits
                .len(),
            2
        );
    }
    #[test]
    fn exact_search_bounds_and_cancellation_are_visible() {
        let documents = [
            doc("org/a", "a", "auth auth"),
            doc("org/a", "binary", "auth\0"),
            doc("org/a", "large", "auth auth auth"),
        ];
        let query = Query::parse("auth", SearchMode::Exact).unwrap();
        let limits = ScanLimits {
            max_file_bytes: 10,
            max_matches: 10,
            ..Default::default()
        };
        let result = scan_documents(&query, &documents, &limits, || false).unwrap();
        assert_eq!(result.hits.len(), 2);
        assert_eq!(result.skipped_binary, 1);
        assert_eq!(result.skipped_large, 1);
        assert!(!result.complete);
        let result = scan_documents(&query, &documents, &limits, || true).unwrap();
        assert!(result.cancelled);
        assert!(result.hits.is_empty());
        let limits = ScanLimits {
            max_matches: 1,
            ..Default::default()
        };
        assert!(
            !scan_documents(&query, &documents, &limits, || false)
                .unwrap()
                .complete
        );
    }
    #[test]
    fn exact_search_rejects_invalid_regex_and_empty_scans() {
        assert!(Query::parse("[", SearchMode::Regex).is_err());
        let query = Query::parse("repo:org/a", SearchMode::Exact).unwrap();
        assert!(Pattern::compile(&query).is_err());
    }

    #[test]
    fn exact_search_keeps_zero_width_matches_and_rejects_unsupported_scope() {
        let document = doc("org/a", "a", "first\n\nlast");
        let query = Query::parse("^$", SearchMode::Regex).unwrap();
        let result = scan_documents(
            &query,
            std::slice::from_ref(&document),
            &ScanLimits::default(),
            || false,
        )
        .unwrap();
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].line, 2);
        let query = Query::parse("author:alice first", SearchMode::Exact).unwrap();
        assert!(scan_documents(&query, &[document], &ScanLimits::default(), || false).is_err());
    }
}
