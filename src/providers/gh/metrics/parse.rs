use super::*;

pub(super) fn positive(value: &Value, field: &str) -> GithubResult<u64> {
    value[field].as_u64().filter(|id| *id > 0).ok_or_else(|| {
        error(
            GithubErrorKind::Json,
            format!("Metrics source lacks a positive canonical {field}"),
        )
    })
}
fn optional_id(value: Option<&Value>) -> GithubResult<Option<u64>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .filter(|id| *id > 0)
            .map(Some)
            .ok_or_else(|| {
                error(
                    GithubErrorKind::Json,
                    "Supplied metric actor/reference ID is not a positive integer",
                )
            }),
    }
}
pub(super) fn valid_opaque(value: &str) -> bool {
    !value.is_empty() && value.len() <= 1024 && !value.chars().any(char::is_control)
}
pub(super) fn node(value: &Value, field: &str) -> GithubResult<String> {
    let value = value[field].as_str().filter(|id| valid_opaque(id));
    value.map(str::to_owned).ok_or_else(|| {
        error(
            GithubErrorKind::Json,
            format!("Metrics source lacks an actual bounded opaque {field}"),
        )
    })
}
fn optional_node(value: &Value, field: &str) -> GithubResult<Option<String>> {
    match value.get(field) {
        None | Some(Value::Null) => Ok(None),
        _ => node(value, field).map(Some),
    }
}
fn timestamp(value: &Value, field: &str, reasons: &mut Vec<String>) -> Option<MetricsTimestamp> {
    let value = value.get(field)?;
    if value.is_null() {
        return None;
    }
    let parsed = value.as_str().and_then(|raw| {
        crate::github_workspace::timestamp_seconds(raw).map(|seconds| MetricsTimestamp {
            raw: raw.into(),
            seconds,
        })
    });
    if parsed.is_none() {
        reasons.push(format!(
            "Supplied {field} is not a validated actual UTC timestamp"
        ));
    }
    parsed
}
fn optional_bool(value: &Value, field: &str, reasons: &mut Vec<String>) -> Option<bool> {
    match value.get(field) {
        None | Some(Value::Null) => None,
        Some(value) => match value.as_bool() {
            Some(value) => Some(value),
            None => {
                reasons.push(format!("Supplied {field} is not boolean metadata"));
                None
            }
        },
    }
}
pub(super) fn actor(value: &Value) -> GithubResult<MetricsActor> {
    if !value.is_null() && !value.is_object() {
        return Err(error(
            GithubErrorKind::Json,
            "Metric actor metadata is malformed",
        ));
    }
    Ok(MetricsActor {
        account_id: optional_id(value.get("id"))?,
        actor: Actor::from_json(value),
    })
}
fn team(value: &Value) -> GithubResult<MetricsTeam> {
    Ok(MetricsTeam {
        id: positive(value, "id")?,
        node_id: optional_node(value, "node_id")?,
        slug: text(value, "slug"),
        name: value["name"].as_str().map(str::to_owned),
    })
}
fn api_path(host: &str, url: &str, expected: &str, api: bool) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let Some((authority, path)) = rest.split_once('/') else {
        return false;
    };
    let lower = host.to_ascii_lowercase();
    let residency = lower
        .strip_suffix(".ghe.com")
        .is_some_and(|tenant| !tenant.is_empty() && !tenant.contains('.'));
    let api_host = if lower == "github.com" {
        "api.github.com".into()
    } else if residency {
        format!("api.{host}")
    } else {
        host.into()
    };
    if !authority.eq_ignore_ascii_case(if api { &api_host } else { host }) || path.contains('?') {
        return false;
    }
    let path = path.split('#').next().unwrap_or_default();
    let path = if api && lower != "github.com" && !residency {
        path.strip_prefix("api/v3/").unwrap_or(path)
    } else {
        path
    };
    path.eq_ignore_ascii_case(expected)
}
fn optional_url(
    value: &Value,
    field: &str,
    host: &str,
    expected: &str,
    api: bool,
) -> GithubResult<()> {
    if let Some(url) = value.get(field).filter(|value| !value.is_null()) {
        if !url
            .as_str()
            .is_some_and(|url| api_path(host, url, expected, api))
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Metric source URL contradicts its exact host/repository/parent",
            ));
        }
    }
    Ok(())
}
pub(super) fn repository(host: &str, name: &str, value: &Value) -> GithubResult<MetricsRepository> {
    validate_repo(name)?;
    let full_name = text(value, "full_name");
    validate_repo(&full_name)?;
    if !name.eq_ignore_ascii_case(&full_name) {
        return Err(error(
            GithubErrorKind::IdentityChanged,
            "Repository metric observation has a different canonical name",
        ));
    }
    optional_url(value, "url", host, &format!("repos/{name}"), true)?;
    optional_url(value, "html_url", host, name, false)?;
    Ok(MetricsRepository {
        host: host.to_ascii_lowercase(),
        id: positive(value, "id")?,
        node_id: node(value, "node_id")?,
        full_name: full_name.to_ascii_lowercase(),
    })
}
pub(super) fn parse_pr(
    repo: &MetricsRepository,
    value: &Value,
) -> GithubResult<MetricsPrObservation> {
    let number = positive(value, "number")?;
    let base = repository(&repo.host, &repo.full_name, &value["base"]["repo"])?;
    if &base != repo {
        return Err(error(
            GithubErrorKind::IdentityChanged,
            "PR base repository was replaced or differs from the exact metric inventory",
        ));
    }
    optional_url(
        value,
        "url",
        &repo.host,
        &format!("repos/{}/pulls/{number}", repo.full_name),
        true,
    )?;
    optional_url(
        value,
        "html_url",
        &repo.host,
        &format!("{}/pull/{number}", repo.full_name),
        false,
    )?;
    let mut reasons = vec![];
    let created_at = timestamp(value, "created_at", &mut reasons);
    let merged_at = timestamp(value, "merged_at", &mut reasons);
    let closed_at = timestamp(value, "closed_at", &mut reasons);
    let observed_updated_at = timestamp(value, "updated_at", &mut reasons);
    if created_at.is_none() {
        reasons.push("Original created_at unavailable; age/cohort lifetime remains unknown".into());
    }
    let state = match text(value, "state").as_str() {
        "open" => MetricsPrState::Open,
        "closed" => MetricsPrState::Closed,
        _ => {
            reasons.push("Current PR state unavailable/unsupported".into());
            MetricsPrState::Unknown(text(value, "state"))
        }
    };
    let draft = optional_bool(value, "draft", &mut reasons);
    if draft.is_none() {
        reasons.push("Current draft state unavailable; review cohort is unknown".into());
    }
    let merged = optional_bool(value, "merged", &mut reasons);
    if merged == Some(false) && merged_at.is_some()
        || matches!(state, MetricsPrState::Open) && merged_at.is_some()
    {
        return Err(error(
            GithubErrorKind::Json,
            "PR merge/state metadata contradicts its actual merge timestamp",
        ));
    }
    let head_sha = match value["head"].get("sha") {
        None | Some(Value::Null) => None,
        Some(Value::String(sha)) if validate_sha(sha).is_ok() => Some(sha.clone()),
        _ => {
            reasons.push(
                "Exact PR head is unavailable/malformed; check health cannot be attributed".into(),
            );
            None
        }
    };
    let requested_users = match value.get("requested_reviewers") {
        None | Some(Value::Null) => None,
        Some(Value::Array(rows)) => Some(rows.iter().map(actor).collect::<GithubResult<Vec<_>>>()?),
        _ => {
            return Err(error(
                GithubErrorKind::Json,
                "Requested user inventory is malformed",
            ))
        }
    };
    let requested_teams = match value.get("requested_teams") {
        None | Some(Value::Null) => None,
        Some(Value::Array(rows)) => Some(rows.iter().map(team).collect::<GithubResult<Vec<_>>>()?),
        _ => {
            return Err(error(
                GithubErrorKind::Json,
                "Requested team inventory is malformed",
            ))
        }
    };
    if requested_users.is_none() || requested_teams.is_none() {
        reasons.push(
            "Current requested reviewer/team inventory omitted; no empty membership is inferred"
                .into(),
        );
    }
    if requested_users
        .as_ref()
        .is_some_and(|users| users.iter().any(|user| user.account_id.is_none()))
    {
        reasons
            .push("Requested user identity missing; review membership cannot be inferred".into());
    }
    if requested_teams
        .as_ref()
        .is_some_and(|teams| !teams.is_empty())
    {
        reasons
            .push("Team review requests exist; current viewer membership is not inferred".into());
    }
    Ok(MetricsPrObservation {
        repository: repo.clone(),
        id: positive(value, "id")?,
        node_id: node(value, "node_id")?,
        number,
        title: text(value, "title"),
        author: actor(&value["user"])?,
        state,
        draft,
        created_at,
        merged_at,
        closed_at,
        merged,
        head_sha,
        requested_users,
        requested_teams,
        observed_updated_at,
        reasons,
    })
}
pub(super) fn parse_review(
    parent: &MetricsPrObservation,
    value: &Value,
) -> GithubResult<MetricsReviewObservation> {
    optional_url(
        value,
        "pull_request_url",
        &parent.repository.host,
        &format!(
            "repos/{}/pulls/{}",
            parent.repository.full_name, parent.number
        ),
        true,
    )?;
    let state = match text(value, "state").to_ascii_uppercase().as_str() {
        "PENDING" => MetricsReviewState::Pending,
        "APPROVED" => MetricsReviewState::Approved,
        "CHANGES_REQUESTED" => MetricsReviewState::ChangesRequested,
        "COMMENTED" => MetricsReviewState::Commented,
        "DISMISSED" => MetricsReviewState::Dismissed,
        _ => MetricsReviewState::Unknown(text(value, "state")),
    };
    let mut reasons = vec![];
    let submitted_at = timestamp(value, "submitted_at", &mut reasons);
    if !matches!(state, MetricsReviewState::Pending) && submitted_at.is_none() {
        reasons.push(
            "Submitted review timestamp missing; first-review evidence remains unknown".into(),
        );
    }
    if matches!(state, MetricsReviewState::Unknown(_)) {
        reasons.push("Unsupported review state cannot become an assumed submission".into());
    }
    if matches!(state, MetricsReviewState::Pending) && submitted_at.is_some() {
        reasons
            .push("Pending review has contradictory submitted_at; not submitted evidence".into());
    }
    let commit_sha = match value.get("commit_id") {
        None | Some(Value::Null) => None,
        Some(Value::String(sha)) if validate_sha(sha).is_ok() => Some(sha.clone()),
        _ => {
            reasons.push("Review source commit is malformed; no current-head substitution".into());
            None
        }
    };
    Ok(MetricsReviewObservation {
        id: positive(value, "id")?,
        node_id: node(value, "node_id")?,
        state,
        submitted_at,
        actor: actor(&value["user"])?,
        commit_sha,
        reasons,
    })
}
fn requested(value: &Value) -> GithubResult<MetricsRequestedReviewer> {
    let supplied_user = value
        .get("requested_reviewer")
        .is_some_and(|value| !value.is_null());
    let supplied_team = value
        .get("requested_team")
        .is_some_and(|value| !value.is_null());
    if supplied_user && supplied_team {
        return Err(error(
            GithubErrorKind::Json,
            "Review request supplies contradictory user/team target kinds",
        ));
    }
    if supplied_user {
        if value["requested_reviewer"]["type"] == "Team"
            || value["requested_reviewer"]["type"] == "Organization"
        {
            return Err(error(
                GithubErrorKind::Json,
                "Review requested_reviewer is not an actual user reference",
            ));
        }
        let actor = actor(&value["requested_reviewer"])?;
        return Ok(if actor.account_id.is_some() {
            MetricsRequestedReviewer::User(actor)
        } else {
            MetricsRequestedReviewer::Unknown
        });
    }
    if supplied_team {
        return team(&value["requested_team"]).map(MetricsRequestedReviewer::Team);
    }
    Ok(MetricsRequestedReviewer::Unknown)
}
pub(super) fn parse_timeline(
    parent: &MetricsPrObservation,
    value: &Value,
) -> GithubResult<MetricsTimelineObservation> {
    let name = text(value, "event");
    let mut reasons = vec![];
    let at = timestamp(
        value,
        if name == "reviewed" {
            "submitted_at"
        } else {
            "created_at"
        },
        &mut reasons,
    );
    let kind = match name.as_str() {
        "review_requested" => MetricsTimelineKind::ReviewRequested {
            reviewer: requested(value)?,
        },
        "review_request_removed" => MetricsTimelineKind::ReviewRequestRemoved {
            reviewer: requested(value)?,
        },
        "reviewed" => MetricsTimelineKind::ReviewSubmitted {
            review: parse_review(parent, value)?,
        },
        "review_dismissed" => MetricsTimelineKind::ReviewDismissed {
            review_id: match value["dismissed_review"].get("review_id") {
                Some(Value::String(id))
                    if !id.is_empty()
                        && !id.starts_with('0')
                        && id.bytes().all(|byte| byte.is_ascii_digit()) =>
                {
                    id.parse::<u64>().ok().filter(|id| *id > 0)
                }
                Some(Value::String(_)) => None,
                other => optional_id(other)?,
            },
        },
        "merged" => MetricsTimelineKind::Merged,
        "closed" => MetricsTimelineKind::Closed,
        "reopened" => MetricsTimelineKind::Reopened,
        "ready_for_review" => MetricsTimelineKind::ReadyForReview,
        "converted_to_draft" => MetricsTimelineKind::ConvertedToDraft,
        "assigned"
        | "unassigned"
        | "labeled"
        | "unlabeled"
        | "milestoned"
        | "demilestoned"
        | "commented"
        | "referenced"
        | "cross-referenced"
        | "committed"
        | "head_ref_force_pushed"
        | "head_ref_deleted"
        | "head_ref_restored"
        | "base_ref_changed"
        | "base_ref_force_pushed"
        | "mentioned"
        | "subscribed"
        | "unsubscribed" => MetricsTimelineKind::Unrelated(name.clone()),
        _ => MetricsTimelineKind::Unsupported(name.clone()),
    };
    let relevant = !matches!(kind, MetricsTimelineKind::Unrelated(_));
    let id = optional_id(value.get("id"))?;
    let node_id = optional_node(value, "node_id")?;
    if name != "reviewed" {
        if let Some(id) = id {
            optional_url(
                value,
                "url",
                &parent.repository.host,
                &format!("repos/{}/issues/events/{id}", parent.repository.full_name),
                true,
            )?;
        }
    }
    if relevant && (at.is_none() || id.is_none() || node_id.is_none()) {
        reasons
            .push("Relevant timeline event lacks actual timestamp/numeric/opaque identity".into());
    }
    if matches!(
        &kind,
        MetricsTimelineKind::ReviewRequested {
            reviewer: MetricsRequestedReviewer::Unknown | MetricsRequestedReviewer::Team(_)
        } | MetricsTimelineKind::ReviewRequestRemoved {
            reviewer: MetricsRequestedReviewer::Unknown | MetricsRequestedReviewer::Team(_)
        }
    ) {
        reasons
            .push("Review target is missing or a team; viewer membership is not inferred".into());
    }
    if matches!(kind, MetricsTimelineKind::Unsupported(_)) {
        reasons
            .push("Unsupported metric-relevant timeline event prevents complete evidence".into());
    }
    if matches!(
        kind,
        MetricsTimelineKind::ReviewDismissed { review_id: None }
    ) {
        reasons.push("Dismissal has no positive original review ID; no review is inferred".into());
    }
    if let MetricsTimelineKind::ReviewSubmitted { review } = &kind {
        reasons.extend(review.reasons.clone());
    }
    let actor = actor(if name == "reviewed" {
        &value["user"]
    } else {
        &value["actor"]
    })?;
    Ok(MetricsTimelineObservation {
        id,
        node_id,
        at,
        actor,
        kind,
        reasons,
    })
}
