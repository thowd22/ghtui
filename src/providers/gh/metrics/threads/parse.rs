use super::*;

pub(super) fn opaque(value: &str) -> bool {
    !value.is_empty() && value.len() <= 512 && !value.chars().any(char::is_control)
}
fn actual_node(raw: &Value, field: &str) -> GithubResult<String> {
    raw[field]
        .as_str()
        .filter(|id| opaque(id))
        .map(str::to_owned)
        .ok_or_else(|| {
            error(
                GithubErrorKind::Json,
                format!("Thread source omitted genuine bounded {field}"),
            )
        })
}
pub(super) fn bigint(value: &Value) -> Option<u64> {
    match value {
        Value::Number(_) => value.as_u64().filter(|id| *id > 0),
        Value::String(raw)
            if !raw.is_empty()
                && raw.len() <= 20
                && !raw.starts_with('0')
                && raw.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            raw.parse::<u64>().ok().filter(|id| *id > 0)
        }
        _ => None,
    }
}
pub(super) fn graph_repo(raw: &Value, parent: &MetricsPrObservation) -> GithubResult<()> {
    if actual_node(raw, "id")? != parent.repository.node_id
        || positive(raw, "databaseId")? != parent.repository.id
        || !raw["nameWithOwner"]
            .as_str()
            .is_some_and(|name| name.eq_ignore_ascii_case(&parent.repository.full_name))
    {
        return Err(error(
            GithubErrorKind::IdentityChanged,
            "GraphQL thread repository differs from exact REST canonical identity",
        ));
    }
    Ok(())
}
pub(super) fn graph_parent(raw: &Value, parent: &MetricsPrObservation) -> GithubResult<()> {
    if actual_node(raw, "id")? != parent.node_id
        || bigint(&raw["fullDatabaseId"]) != Some(parent.id)
        || positive(raw, "number")? != parent.number
    {
        return Err(error(
            GithubErrorKind::IdentityChanged,
            "GraphQL thread parent differs from exact original PR database/node/number",
        ));
    }
    graph_repo(&raw["repository"], parent)?;
    if !raw["headRefOid"].as_str().is_some_and(|head| {
        crate::search::model::valid_sha(head) && Some(head) == parent.head_sha.as_deref()
    }) {
        return Err(error(
            GithubErrorKind::StaleHead,
            "GraphQL thread current full head differs from original PR observation",
        ));
    }
    Ok(())
}
fn boolean(raw: &Value, field: &str, reasons: &mut Vec<String>) -> Option<bool> {
    let value = raw[field].as_bool();
    if value.is_none() {
        reasons.push(format!(
            "{field} omitted/null/malformed; unknown is not false"
        ));
    }
    value
}
fn coordinate(raw: &Value, field: &str, reasons: &mut Vec<String>) -> Option<u64> {
    match raw.get(field) {
        None | Some(Value::Null) => None,
        Some(value) => match value
            .as_u64()
            .filter(|line| (1..=i32::MAX as u64).contains(line))
        {
            Some(line) => Some(line),
            None => {
                reasons.push(format!("{field} is not a positive GraphQL line coordinate"));
                None
            }
        },
    }
}
fn range(
    raw: &Value,
    end: &str,
    start: &str,
    reasons: &mut Vec<String>,
) -> (Option<u64>, Option<u64>) {
    let line = coordinate(raw, end, reasons);
    let start_line = coordinate(raw, start, reasons);
    if start_line.is_some_and(|start| line.is_none_or(|end| start > end)) {
        reasons.push(format!(
            "{start}/{end} range unavailable; original/current coordinates are never substituted"
        ));
        (None, None)
    } else {
        (line, start_line)
    }
}
fn side(raw: &Value, field: &str, reasons: &mut Vec<String>) -> Option<MetricsThreadSide> {
    match raw.get(field) {
        None | Some(Value::Null) => None,
        Some(Value::String(side)) if side == "LEFT" => Some(MetricsThreadSide::Left),
        Some(Value::String(side)) if side == "RIGHT" => Some(MetricsThreadSide::Right),
        _ => {
            reasons.push(format!("{field} is not a known observed diff side"));
            None
        }
    }
}
fn path(raw: &Value, reasons: &mut Vec<String>) -> Option<String> {
    match raw.get("path") {
        None | Some(Value::Null) => None,
        Some(Value::String(path))
            if !path.is_empty()
                && path.len() <= 4096
                && !path.starts_with('/')
                && !path.contains('\0')
                && !path.split('/').any(|part| matches!(part, "" | "." | "..")) =>
        {
            Some(path.clone())
        }
        _ => {
            reasons.push("Observed path is not bounded safe raw UTF-8 repository placement".into());
            None
        }
    }
}
fn commit(raw: &Value, field: &str, reasons: &mut Vec<String>) -> Option<String> {
    match raw.get(field) {
        None | Some(Value::Null) => None,
        Some(value) => match value["oid"]
            .as_str()
            .filter(|sha| crate::search::model::valid_sha(sha))
        {
            Some(sha) => Some(sha.into()),
            None => {
                reasons.push(format!("{field} is not an actual full immutable commit; current head never substitutes"));
                None
            }
        },
    }
}
fn timestamp(raw: &Value, field: &str, reasons: &mut Vec<String>) -> Option<MetricsTimestamp> {
    match raw.get(field) {
        None | Some(Value::Null) => None,
        Some(value) => {
            let parsed = value.as_str().and_then(|value| {
                crate::github_workspace::timestamp_seconds(value).map(|seconds| MetricsTimestamp {
                    raw: value.into(),
                    seconds,
                })
            });
            if parsed.is_none() {
                reasons.push(format!(
                    "{field} is not a validated actual UTC source timestamp"
                ));
            }
            parsed
        }
    }
}
fn graph_actor(raw: &Value, field: &str, reasons: &mut Vec<String>) -> Option<MetricsActor> {
    let Some(value) = raw.get(field).filter(|value| value.is_object()) else {
        reasons.push(format!(
            "{field} actor unavailable; no author/resolver or UID0 invented"
        ));
        return None;
    };
    if ["login", "name", "avatarUrl", "__typename"]
        .iter()
        .any(|field| {
            value
                .get(field)
                .filter(|value| !value.is_null())
                .is_some_and(|value| value.as_str().is_none_or(|value| value.len() > 4096))
        })
    {
        reasons.push(format!(
            "{field} actor display metadata exceeds bounded UTF-8 fields"
        ));
        return None;
    }
    let account_id = match value
        .get("databaseId")
        .filter(|_| value["__typename"] == "User")
    {
        None | Some(Value::Null) => None,
        Some(id) => id.as_u64().filter(|id| *id > 0),
    };
    if account_id.is_none() {
        reasons.push(format!(
            "{field} positive numeric User identity unavailable"
        ));
    }
    let actor = Actor::from_json(value);
    if actor.login.is_empty() {
        reasons.push(format!(
            "{field} username unavailable; keep shared unknown-actor fallback"
        ));
    }
    Some(MetricsActor { account_id, actor })
}
pub(super) fn empty_coverage() -> MetricsReadCoverage {
    MetricsReadCoverage {
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
    }
}
pub(super) fn finalize(coverage: &mut MetricsReadCoverage) {
    coverage.complete_empty =
        coverage.enumeration_complete && coverage.fetched_rows == 0 && coverage.excluded_rows == 0;
    coverage.reasons.sort();
    coverage.reasons.dedup();
    if coverage.enumeration_complete && coverage.metric_evidence_complete {
        coverage.status = MetricsReadStatus::Complete;
    } else if coverage.fetched_pages > 0 {
        coverage.status = MetricsReadStatus::Partial;
    }
}
pub(super) struct ConnectionPage<'a> {
    pub nodes: &'a [Value],
    pub total: u64,
    pub next: Option<String>,
}
pub(super) fn connection(raw: &Value, first: usize) -> GithubResult<ConnectionPage<'_>> {
    let nodes = raw["nodes"].as_array().ok_or_else(|| {
        error(
            GithubErrorKind::Json,
            "Thread connection omitted actual nodes array",
        )
    })?;
    let total = raw["totalCount"].as_u64().ok_or_else(|| {
        error(
            GithubErrorKind::Json,
            "Thread connection omitted actual nonnegative totalCount",
        )
    })?;
    if nodes.len() > first {
        return Err(error(
            GithubErrorKind::Json,
            "GraphQL thread/comment source exceeded requested first/raw-row budget",
        ));
    }
    let next = match raw["pageInfo"]["hasNextPage"].as_bool() {
        Some(false) => None,
        Some(true) => Some(
            raw["pageInfo"]["endCursor"]
                .as_str()
                .filter(|cursor| valid_opaque(cursor))
                .ok_or_else(|| {
                    error(
                        GithubErrorKind::Json,
                        "Thread continuation omitted genuine bounded cursor",
                    )
                })?
                .into(),
        ),
        None => {
            return Err(error(
                GithubErrorKind::Json,
                "Thread page omitted hasNextPage; incomplete is not empty/complete",
            ))
        }
    };
    Ok(ConnectionPage { nodes, total, next })
}
pub(super) fn thread(
    raw: &Value,
    parent: &MetricsPrObservation,
) -> GithubResult<MetricsReviewThreadObservation> {
    graph_parent(&raw["pullRequest"], parent)?;
    graph_repo(&raw["repository"], parent)?;
    let mut state_reasons = vec![];
    let mut provenance_reasons = vec![];
    let resolved = boolean(raw, "isResolved", &mut state_reasons);
    let outdated = boolean(raw, "isOutdated", &mut state_reasons);
    let (line, start_line) = range(raw, "line", "startLine", &mut provenance_reasons);
    let (original_line, original_start_line) = range(
        raw,
        "originalLine",
        "originalStartLine",
        &mut provenance_reasons,
    );
    let placement = MetricsThreadPlacement {
        observed_path: path(raw, &mut provenance_reasons),
        observed_side: side(raw, "diffSide", &mut provenance_reasons),
        observed_start_side: side(raw, "startDiffSide", &mut provenance_reasons),
        line,
        start_line,
        original_line,
        original_start_line,
    };
    let resolved_by = graph_actor(raw, "resolvedBy", &mut provenance_reasons);
    Ok(MetricsReviewThreadObservation {
        parent_node_id: parent.node_id.clone(),
        node_id: actual_node(raw, "id")?,
        subject_type: match raw["subjectType"].as_str() {
            Some("FILE") => Some(MetricsThreadSubjectV1::File),
            Some("LINE") => Some(MetricsThreadSubjectV1::Line),
            _ => None,
        },
        resolved,
        outdated,
        viewer_can_reply: boolean(raw, "viewerCanReply", &mut provenance_reasons),
        viewer_can_resolve: boolean(raw, "viewerCanResolve", &mut provenance_reasons),
        viewer_can_unresolve: boolean(raw, "viewerCanUnresolve", &mut provenance_reasons),
        resolved_by,
        placement,
        comments: vec![],
        comment_coverage: empty_coverage(),
        original_comment: None,
        state_reasons,
        provenance_reasons,
    })
}
pub(super) fn comment(
    raw: &Value,
    parent: &MetricsPrObservation,
) -> GithubResult<MetricsThreadComment> {
    graph_parent(&raw["pullRequest"], parent)?;
    graph_repo(&raw["repository"], parent)?;
    let mut reasons = vec![];
    let database_id = match raw.get("fullDatabaseId") {
        None | Some(Value::Null) => None,
        Some(value) => {
            let id = bigint(value);
            if id.is_none() {
                reasons
                    .push("Supplied fullDatabaseId is not a positive canonical u64 BigInt".into());
            }
            id
        }
    };
    let reply_to = match raw.get("replyTo") {
        Some(Value::Null) => MetricsThreadReplyTo::Original,
        Some(value) if value.is_object() => MetricsThreadReplyTo::Reply(actual_node(value, "id")?),
        _ => {
            reasons.push(
                "replyTo omitted/unknown; comment position cannot establish original author".into(),
            );
            MetricsThreadReplyTo::Unknown
        }
    };
    let (line, start_line) = range(raw, "line", "startLine", &mut reasons);
    let (original_line, original_start_line) =
        range(raw, "originalLine", "originalStartLine", &mut reasons);
    let body = raw["body"].as_str().map(str::to_owned);
    if body.is_none() {
        reasons.push("Comment body unavailable; null is not empty text".into());
    }
    let author = graph_actor(raw, "author", &mut reasons);
    let created_at = timestamp(raw, "createdAt", &mut reasons);
    let updated_at = timestamp(raw, "updatedAt", &mut reasons);
    Ok(MetricsThreadComment {
        node_id: actual_node(raw, "id")?,
        database_id,
        reply_to,
        author,
        body,
        created_at,
        updated_at,
        observed_path: path(raw, &mut reasons),
        line,
        start_line,
        original_line,
        original_start_line,
        commit_sha: commit(raw, "commit", &mut reasons),
        original_commit_sha: commit(raw, "originalCommit", &mut reasons),
        reasons,
    })
}
