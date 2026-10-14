use super::*;

fn optional_text(raw: &Value, field: &str, reasons: &mut Vec<String>) -> Option<String> {
    match raw.get(field) {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) if value.len() <= 4096 => Some(value.clone()),
        _ => {
            reasons.push(format!(
                "{field} unavailable/malformed/over bounded UTF-8 metadata"
            ));
            None
        }
    }
}
fn optional_positive(raw: &Value, field: &str, reasons: &mut Vec<String>) -> Option<u64> {
    match raw.get(field) {
        None | Some(Value::Null) => None,
        Some(value) => match value.as_u64().filter(|id| *id > 0) {
            Some(id) => Some(id),
            None => {
                reasons.push(format!(
                    "{field} is not an actual positive numeric identity"
                ));
                None
            }
        },
    }
}
fn optional_node(raw: &Value, reasons: &mut Vec<String>) -> Option<String> {
    match raw.get("node_id") {
        Some(Value::String(node)) if valid_opaque(node) => Some(node.clone()),
        _ => {
            reasons
                .push("Actual opaque health node unavailable; numeric ID never substitutes".into());
            None
        }
    }
}
fn timestamp(raw: &Value, field: &str, reasons: &mut Vec<String>) -> Option<MetricsTimestamp> {
    match raw.get(field) {
        None | Some(Value::Null) => None,
        Some(value) => {
            let parsed = value.as_str().and_then(|raw| {
                crate::github_workspace::timestamp_seconds(raw).map(|seconds| MetricsTimestamp {
                    raw: raw.into(),
                    seconds,
                })
            });
            if parsed.is_none() {
                reasons.push(format!("{field} is not an actual validated UTC timestamp"));
            }
            parsed
        }
    }
}
fn actor(raw: &Value, field: &str, reasons: &mut Vec<String>) -> Option<MetricsActor> {
    let Some(value) = raw.get(field).filter(|value| value.is_object()) else {
        reasons.push(format!("{field} unavailable; no username or UID0 invented"));
        return None;
    };
    if ["login", "name", "avatar_url", "type"].iter().any(|field| {
        value
            .get(field)
            .filter(|value| !value.is_null())
            .is_some_and(|value| value.as_str().is_none_or(|text| text.len() > 4096))
    }) {
        reasons.push(format!(
            "{field} actor display fields exceed bounded UTF-8 metadata"
        ));
        return None;
    }
    let account_id = optional_positive(value, "id", reasons);
    let actor = Actor::from_json(value);
    if account_id.is_none() || actor.login.is_empty() {
        reasons.push(format!(
            "{field} positive numeric actor or username unknown"
        ));
    }
    Some(MetricsActor { account_id, actor })
}
fn status(raw: &Value, reasons: &mut Vec<String>) -> MetricsHealthStatus {
    match raw["status"].as_str() {
        Some("queued") => MetricsHealthStatus::Queued,
        Some("in_progress") => MetricsHealthStatus::InProgress,
        Some("completed") => MetricsHealthStatus::Completed,
        Some("waiting") => MetricsHealthStatus::Waiting,
        Some("requested") => MetricsHealthStatus::Requested,
        Some("pending") => MetricsHealthStatus::Pending,
        Some("action_required") => MetricsHealthStatus::ActionRequired,
        value => {
            reasons.push(
                "Current health status missing/unsupported; unknown never becomes success/pending"
                    .into(),
            );
            MetricsHealthStatus::Unknown(
                value.filter(|value| value.len() <= 4096).map(str::to_owned),
            )
        }
    }
}
fn conclusion(raw: &Value, reasons: &mut Vec<String>) -> Option<MetricsHealthConclusion> {
    match raw.get("conclusion") {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) => Some(match value.as_str() {
            "success" => MetricsHealthConclusion::Success,
            "failure" => MetricsHealthConclusion::Failure,
            "neutral" => MetricsHealthConclusion::Neutral,
            "cancelled" => MetricsHealthConclusion::Cancelled,
            "skipped" => MetricsHealthConclusion::Skipped,
            "timed_out" => MetricsHealthConclusion::TimedOut,
            "action_required" => MetricsHealthConclusion::ActionRequired,
            "stale" => MetricsHealthConclusion::Stale,
            "startup_failure" => MetricsHealthConclusion::StartupFailure,
            _ => {
                reasons.push("Observed conclusion unsupported; not silently healthy".into());
                MetricsHealthConclusion::Unknown(if value.len() <= 4096 {
                    value.clone()
                } else {
                    String::new()
                })
            }
        }),
        _ => {
            reasons.push("Observed conclusion malformed; null/unknown is not success".into());
            None
        }
    }
}
fn lifecycle(
    status: &MetricsHealthStatus,
    conclusion: &Option<MetricsHealthConclusion>,
    reasons: &mut Vec<String>,
) {
    if matches!(status, MetricsHealthStatus::Completed) && conclusion.is_none()
        || !matches!(
            status,
            MetricsHealthStatus::Completed | MetricsHealthStatus::ActionRequired
        ) && conclusion.is_some()
    {
        reasons.push("Status/conclusion lifecycle incomplete or contradictory; no assumed success or pending".into());
    }
}
fn same_sha(raw: &Value, field: &str, expected: Option<&str>) -> GithubResult<String> {
    let sha = raw[field]
        .as_str()
        .filter(|sha| crate::search::model::valid_sha(sha))
        .ok_or_else(|| {
            error(
                GithubErrorKind::Json,
                format!("{field} is not an actual full40/64 immutable commit identity"),
            )
        })?;
    if expected.is_some_and(|expected| !sha.eq_ignore_ascii_case(expected)) {
        return Err(error(
            GithubErrorKind::StaleHead,
            "Health observation differs from the exact selected full commit",
        ));
    }
    Ok(sha.to_ascii_lowercase())
}
fn api_url(host: &str, url: &str, path: &str) -> bool {
    let Some(raw) = url.strip_prefix("https://") else {
        return false;
    };
    let Some((authority, actual)) = raw.split_once('/') else {
        return false;
    };
    let residency = host
        .strip_suffix(".ghe.com")
        .is_some_and(|tenant| !tenant.is_empty() && !tenant.contains('.'));
    let api_host = if host == "github.com" {
        "api.github.com".into()
    } else if residency {
        format!("api.{host}")
    } else {
        host.into()
    };
    let actual = if host != "github.com" && !residency {
        actual.strip_prefix("api/v3/").unwrap_or(actual)
    } else {
        actual
    };
    authority.eq_ignore_ascii_case(&api_host)
        && actual.eq_ignore_ascii_case(path)
        && !url.chars().any(char::is_control)
}
fn source_url(
    raw: &Value,
    repo: &MetricsRepository,
    path: &str,
    reasons: &mut Vec<String>,
) -> GithubResult<Option<String>> {
    let url = optional_text(raw, "url", reasons);
    if url
        .as_ref()
        .is_some_and(|url| !api_url(&repo.host, url, path))
    {
        return Err(error(
            GithubErrorKind::IdentityChanged,
            "Health source URL contradicts exact host/repository/resource identity",
        ));
    }
    Ok(url)
}
pub(super) fn check(
    raw: &Value,
    parent: &MetricsPrObservation,
) -> GithubResult<MetricsCheckObservation> {
    let id = positive(raw, "id")?;
    let mut reasons = vec![];
    let head_sha = same_sha(raw, "head_sha", parent.head_sha.as_deref())?;
    let name = optional_text(raw, "name", &mut reasons).filter(|name| !name.is_empty());
    if name.is_none() {
        reasons.push("Check display name unavailable; genuine positive ID/head retained without invented name".into());
    }
    let status = status(raw, &mut reasons);
    let conclusion = conclusion(raw, &mut reasons);
    lifecycle(&status, &conclusion, &mut reasons);
    let started_at = timestamp(raw, "started_at", &mut reasons);
    let completed_at = timestamp(raw, "completed_at", &mut reasons);
    if started_at
        .as_ref()
        .zip(completed_at.as_ref())
        .is_some_and(|(start, end)| start.unix_seconds() > end.unix_seconds())
    {
        reasons.push("Actual check completion precedes start; duration remains unavailable".into());
    }
    Ok(MetricsCheckObservation {
        id,
        node_id: optional_node(raw, &mut reasons),
        name,
        head_sha,
        result: MetricsCheckResult::CheckRun { status, conclusion },
        app_id: optional_positive(&raw["app"], "id", &mut reasons),
        actor: actor(&raw["app"], "owner", &mut reasons),
        started_at,
        completed_at,
        created_at: None,
        url: source_url(
            raw,
            &parent.repository,
            &format!("repos/{}/check-runs/{id}", parent.repository.full_name),
            &mut reasons,
        )?,
        current_context_unique: true,
        reasons,
    })
}
pub(super) fn status_header(raw: &Value, parent: &MetricsPrObservation) -> GithubResult<()> {
    same_sha(raw, "sha", parent.head_sha.as_deref())?;
    let repo = repository(
        &parent.repository.host,
        &parent.repository.full_name,
        &raw["repository"],
    )?;
    require_same_repo(&parent.repository, &repo)
}
pub(super) fn commit_status(
    raw: &Value,
    parent: &MetricsPrObservation,
) -> GithubResult<MetricsCheckObservation> {
    let id = positive(raw, "id")?;
    let mut reasons = vec![];
    let name = optional_text(raw, "context", &mut reasons).filter(|name| !name.is_empty());
    if name.is_none() {
        reasons.push("Commit status context unavailable; keep genuine positive ID without arbitrary context alias".into());
    }
    let state = match raw["state"].as_str() {
        Some("success") => MetricsCommitStatusState::Success,
        Some("failure") => MetricsCommitStatusState::Failure,
        Some("error") => MetricsCommitStatusState::Error,
        Some("pending") => MetricsCommitStatusState::Pending,
        value => {
            reasons.push("Commit status state unknown; not a healthy/empty context".into());
            MetricsCommitStatusState::Unknown(
                value.filter(|value| value.len() <= 4096).map(str::to_owned),
            )
        }
    };
    let sha = parent
        .head_sha
        .as_ref()
        .ok_or_else(|| {
            error(
                GithubErrorKind::InvalidTarget,
                "Commit status requires actual original full head",
            )
        })?
        .to_ascii_lowercase();
    Ok(MetricsCheckObservation {
        id,
        node_id: optional_node(raw, &mut reasons),
        name,
        head_sha: sha.clone(),
        result: MetricsCheckResult::CommitStatus { state },
        app_id: None,
        actor: actor(raw, "creator", &mut reasons),
        started_at: None,
        completed_at: None,
        created_at: timestamp(raw, "created_at", &mut reasons),
        url: source_url(
            raw,
            &parent.repository,
            &format!("repos/{}/statuses/{sha}", parent.repository.full_name),
            &mut reasons,
        )?,
        current_context_unique: true,
        reasons,
    })
}
pub(super) fn run(raw: &Value, repo: &MetricsRepository) -> GithubResult<MetricsRunObservation> {
    let actual = repository(&repo.host, &repo.full_name, &raw["repository"])?;
    require_same_repo(repo, &actual)?;
    let id = positive(raw, "id")?;
    let workflow_id = positive(raw, "workflow_id")?;
    let attempt = positive(raw, "run_attempt")?;
    let node_id = node(raw, "node_id")?;
    let head_sha = same_sha(raw, "head_sha", None)?;
    let mut reasons = vec![];
    let status = status(raw, &mut reasons);
    let conclusion = conclusion(raw, &mut reasons);
    lifecycle(&status, &conclusion, &mut reasons);
    let created_at = timestamp(raw, "created_at", &mut reasons);
    if created_at.is_none() {
        reasons.push("Actual run created_at unavailable; window cohort is unknown".into());
    }
    let started_at = timestamp(raw, "run_started_at", &mut reasons);
    if created_at
        .as_ref()
        .zip(started_at.as_ref())
        .is_some_and(|(created, start)| created.unix_seconds() > start.unix_seconds())
    {
        reasons.push("Observed run start precedes original creation; no duration guessed".into());
    }
    Ok(MetricsRunObservation {
        repository: actual,
        id,
        node_id,
        workflow_id,
        attempt,
        head_sha,
        name: optional_text(raw, "name", &mut reasons),
        title: optional_text(raw, "display_title", &mut reasons),
        event: optional_text(raw, "event", &mut reasons),
        head_branch: optional_text(raw, "head_branch", &mut reasons),
        status,
        conclusion,
        actor: actor(raw, "actor", &mut reasons),
        triggering_actor: actor(raw, "triggering_actor", &mut reasons),
        created_at,
        started_at,
        observed_updated_at: timestamp(raw, "updated_at", &mut reasons),
        url: source_url(
            raw,
            repo,
            &format!("repos/{}/actions/runs/{id}", repo.full_name),
            &mut reasons,
        )?,
        reasons,
    })
}
