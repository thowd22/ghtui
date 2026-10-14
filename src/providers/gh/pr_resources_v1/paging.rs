use super::*;

fn failed(coverage: &mut PrResourceCoverageV1, failure: GithubError) {
    coverage.failure = Some(failure.kind);
    coverage
        .reason("Selected API source unavailable; known rows retained only after closing proof");
}
fn can_fetch(coverage: &mut PrResourceCoverageV1, limits: &PrResourceReadLimitsV1) -> bool {
    if coverage.requested_pages >= limits.max_pages_per_source
        || coverage
            .raw_rows
            .checked_add(100)
            .is_none_or(|rows| rows > limits.max_rows_per_source)
    {
        coverage.reason(
            "Selected source page/raw-row limit reached before next request; unseen rows unknown",
        );
        false
    } else {
        true
    }
}
pub(super) fn collect_files(
    provider: &GithubProvider,
    parent: &PrResourceParentV1,
    limits: &PrResourceReadLimitsV1,
    cancel: &CancellationToken,
) -> GithubResult<(Vec<ObservedPrFileV1>, PrResourceCoverageV1)> {
    let mut coverage = PrResourceCoverageV1 {
        semantic_complete: true,
        reported_total: parent.changed_files,
        ..Default::default()
    };
    let mut files = vec![];
    let mut seen = BTreeSet::new();
    while can_fetch(&mut coverage, limits) {
        let page = coverage.requested_pages + 1;
        let endpoint = format!(
            "repos/{}/pulls/{}/files?per_page=100&page={page}",
            parent.summary.pr.repository, parent.summary.pr.number
        );
        coverage.requested_pages += 1;
        let raw = match document(
            provider,
            &endpoint,
            limits.max_document_bytes,
            cancel,
            false,
        ) {
            Ok(raw) => raw,
            Err(failure) => {
                failed(&mut coverage, failure);
                break;
            }
        };
        let Some(rows) = raw.as_array().filter(|rows| rows.len() <= 100) else {
            failed(&mut coverage, safe(GithubErrorKind::Json));
            break;
        };
        coverage.fetched_pages += 1;
        coverage.raw_rows = coverage
            .raw_rows
            .checked_add(rows.len())
            .ok_or_else(|| safe(GithubErrorKind::Json))?;
        for raw in rows {
            let value = match file(raw, &mut coverage) {
                Ok(value) => value,
                Err(failure) if matches!(failure.kind, GithubErrorKind::Json) => {
                    coverage
                        .reason("Unverified file row excluded; no path or zero counters guessed");
                    continue;
                }
                Err(failure) => return Err(failure),
            };
            if !seen.insert(value.path.clone()) {
                coverage.reason("Repeated file path while paging; no complete snapshot inferred");
                continue;
            }
            files.push(value);
        }
        coverage.admitted_rows = files.len();
        if rows.len() < 100 {
            coverage.enumeration_exhausted = true;
            break;
        }
    }
    if coverage.reported_total != Some(files.len() as u64) {
        coverage.reason("Actual changed-files total missing or differs from retained file rows");
    }
    Ok((files, coverage))
}
pub(super) fn collect_checks(
    provider: &GithubProvider,
    parent: &PrResourceParentV1,
    status: bool,
    limits: &PrResourceReadLimitsV1,
    cancel: &CancellationToken,
) -> GithubResult<(Vec<ObservedPrCheckV1>, PrResourceCoverageV1)> {
    let mut coverage = PrResourceCoverageV1 {
        semantic_complete: true,
        ..Default::default()
    };
    let mut values: Vec<ObservedPrCheckV1> = vec![];
    let mut ids = BTreeSet::new();
    let mut nodes = BTreeMap::new();
    let mut contexts = BTreeSet::new();
    while can_fetch(&mut coverage, limits) {
        let page = coverage.requested_pages + 1;
        let endpoint = if status {
            format!(
                "repos/{}/commits/{}/status?per_page=100&page={page}",
                parent.summary.pr.repository, parent.summary.head_sha
            )
        } else {
            format!(
                "repos/{}/commits/{}/check-runs?filter=latest&per_page=100&page={page}",
                parent.summary.pr.repository, parent.summary.head_sha
            )
        };
        coverage.requested_pages += 1;
        let raw = match document(
            provider,
            &endpoint,
            limits.max_document_bytes,
            cancel,
            false,
        ) {
            Ok(raw) => raw,
            Err(failure) => {
                failed(&mut coverage, failure);
                break;
            }
        };
        if status {
            status_header(provider, &raw, parent)?;
        }
        let Some(total) = raw["total_count"].as_u64() else {
            failed(&mut coverage, safe(GithubErrorKind::Json));
            break;
        };
        let Some(rows) = raw[if status { "statuses" } else { "check_runs" }]
            .as_array()
            .filter(|rows| rows.len() <= 100)
        else {
            failed(&mut coverage, safe(GithubErrorKind::Json));
            break;
        };
        if coverage.reported_total.is_some_and(|old| old != total) {
            coverage.reason("Reported result total moved while paging; no atomic complete source");
        }
        coverage.reported_total = Some(total);
        coverage.fetched_pages += 1;
        coverage.raw_rows = coverage
            .raw_rows
            .checked_add(rows.len())
            .ok_or_else(|| safe(GithubErrorKind::Json))?;
        for raw in rows {
            let mut value = match check(raw, parent, status, &mut coverage) {
                Ok(value) => value,
                Err(failure) if matches!(failure.kind, GithubErrorKind::Json) => {
                    coverage.reason(
                        "Unverified result row excluded; no numeric/node or head alias guessed",
                    );
                    continue;
                }
                Err(failure) => return Err(failure),
            };
            if let Some(node) = &value.node_id {
                if nodes
                    .insert(node.clone(), value.id)
                    .is_some_and(|id| id != value.id)
                {
                    return Err(safe(GithubErrorKind::IdentityChanged));
                }
            }
            if !ids.insert(value.id) {
                coverage
                    .reason("Repeated result ID while paging; no arbitrary current body selected");
                continue;
            }
            if status
                && value
                    .name
                    .as_ref()
                    .is_some_and(|name| !contexts.insert(name.to_ascii_lowercase()))
            {
                coverage.reason("Repeated case-insensitive current context; no arbitrary successful result selected");
                value.current_context_unique = false;
                for old in &mut values {
                    if old
                        .name
                        .as_ref()
                        .zip(value.name.as_ref())
                        .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b))
                    {
                        old.current_context_unique = false;
                    }
                }
            }
            values.push(value);
        }
        coverage.admitted_rows = values.len();
        if rows.len() < 100 {
            coverage.enumeration_exhausted = true;
            break;
        }
    }
    if coverage.reported_total != Some(values.len() as u64) {
        coverage.reason("Actual result total missing or differs from retained rows");
    }
    Ok((values, coverage))
}
