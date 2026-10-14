use super::*;
fn endpoint(kind: MetadataChoices) -> &'static str {
    match kind {
        MetadataChoices::Labels => "labels",
        MetadataChoices::Assignees => "assignees",
        MetadataChoices::Milestones => "milestones?state=open",
        MetadataChoices::Branches => "branches",
        MetadataChoices::Collaborators => "collaborators",
        MetadataChoices::Teams => "teams",
    }
}
pub(super) fn finish(coverage: &mut MetricsReadCoverage) {
    coverage.reasons.sort();
    coverage.reasons.dedup();
    if coverage.enumeration_complete && coverage.metric_evidence_complete {
        coverage.status = MetricsReadStatus::Complete;
    } else if !matches!(coverage.status, MetricsReadStatus::Unavailable(_))
        || coverage.admitted_rows > 0
    {
        coverage.status = MetricsReadStatus::Partial;
    }
}
pub(super) fn collect(
    provider: &GithubProvider,
    budget: &mut MetricsBudget,
    repo: &str,
    kind: MetadataChoices,
    limits: &PrMetadataReadLimits,
    cancel: &CancellationToken,
) -> GithubResult<(Vec<Value>, MetricsReadCoverage)> {
    let before_bytes = budget.bytes;
    let mut coverage = MetricsReadCoverage {
        enumeration_complete: false,
        metric_evidence_complete: true,
        complete_empty: false,
        requested_pages: 0,
        fetched_pages: 0,
        fetched_rows: 0,
        admitted_rows: 0,
        excluded_rows: 0,
        response_bytes: 0,
        reasons: vec![],
        status: MetricsReadStatus::Partial,
    };
    let base = format!("{repo}/{}", endpoint(kind));
    let mut rows = vec![];
    let mut seen: BTreeMap<String, Value> = BTreeMap::new();
    let mut nodes: BTreeMap<String, String> = BTreeMap::new();
    for page in 1..=limits.max_pages {
        if coverage.fetched_rows.saturating_add(limits.page_size) > limits.max_rows {
            coverage.reasons.push(
                "Fixed catalogue row budget reached before another gh page; unseen choices unknown"
                    .into(),
            );
            break;
        }
        let separator = if base.contains('?') { '&' } else { '?' };
        let endpoint = format!("{base}{separator}per_page={}&page={page}", limits.page_size);
        coverage.requested_pages += 1;
        let document = match budget.read_document(provider, &endpoint, cancel) {
            Ok(value) => value,
            Err(failure) => {
                if matches!(
                    failure.kind,
                    GithubErrorKind::Authentication | GithubErrorKind::IdentityChanged
                ) {
                    return Err(failure);
                }
                coverage.metric_evidence_complete = false;
                coverage.reasons.push(format!(
                    "Selected catalogue endpoint unavailable: {}",
                    failure.message
                ));
                coverage.status = MetricsReadStatus::Unavailable(failure);
                break;
            }
        };
        let Some(raw) = document.as_array() else {
            coverage.metric_evidence_complete = false;
            coverage.status = MetricsReadStatus::Unavailable(error(
                GithubErrorKind::Json,
                "Catalogue response is not a scoped REST array",
            ));
            coverage
                .reasons
                .push("Malformed catalogue response; no options or empty proof inferred".into());
            break;
        };
        if raw.len() > limits.page_size
            || raw.len() > limits.max_rows.saturating_sub(coverage.fetched_rows)
        {
            return Err(error(
                GithubErrorKind::Json,
                "Catalogue endpoint exceeded requested fixed page/raw-row cap",
            ));
        }
        coverage.fetched_pages += 1;
        coverage.fetched_rows += raw.len();
        for item in raw {
            if serde_json::to_vec(item)
                .map_err(|failure| error(GithubErrorKind::Json, failure.to_string()))?
                .len()
                > limits.max_row_bytes
            {
                coverage.excluded_rows += 1;
                coverage.metric_evidence_complete = false;
                coverage.reasons.push(
                    "Observed catalogue option exceeds row bytes; excluded, not silently missing"
                        .into(),
                );
                continue;
            }
            let key = match kind {
                MetadataChoices::Branches => item["name"]
                    .as_str()
                    .filter(|name| !name.is_empty())
                    .map(|name| format!("branch:{name}")),
                MetadataChoices::Milestones => item["number"]
                    .as_u64()
                    .filter(|id| *id > 0)
                    .map(|id| format!("milestone:{id}")),
                _ => item["id"]
                    .as_u64()
                    .filter(|id| *id > 0)
                    .map(|id| format!("numeric:{id}")),
            };
            // Unknown candidates are retained, never given position-derived
            // canonical aliases or silently dropped. Row eligibility belongs
            // to the pure picker; null IDs do not negate REST exhaustion.
            let Some(key) = key else {
                rows.push(item.clone());
                continue;
            };
            if let Some(node) = item.get("node_id").filter(|value| !value.is_null()) {
                if !node.as_str().is_some_and(valid_opaque) {
                    coverage
                        .reasons
                        .push("Supplied option opaque identity is malformed".into());
                } else if nodes
                    .insert(node.as_str().unwrap().into(), key.clone())
                    .is_some_and(|previous| previous != key)
                {
                    return Err(error(
                        GithubErrorKind::IdentityChanged,
                        "Catalogue genuine node conflicts with observed canonical membership",
                    ));
                }
            }
            if let Some(previous) = seen.get(&key) {
                if previous != item {
                    return Err(error(GithubErrorKind::IdentityChanged, "Repeated catalogue option changed during paging; no arbitrary alias selected"));
                }
                continue;
            }
            seen.insert(key, item.clone());
            rows.push(item.clone());
        }
        if raw.len() < limits.page_size {
            coverage.enumeration_complete = true;
            break;
        }
    }
    coverage.response_bytes = budget.bytes - before_bytes;
    coverage.admitted_rows = rows.len();
    if !coverage.enumeration_complete {
        coverage.metric_evidence_complete = false;
        coverage.reasons.push(
            "Bounded catalogue enumeration was not exhausted; remaining options unavailable".into(),
        );
    }
    coverage.complete_empty =
        coverage.enumeration_complete && coverage.fetched_rows == 0 && coverage.excluded_rows == 0;
    finish(&mut coverage);
    Ok((rows, coverage))
}
