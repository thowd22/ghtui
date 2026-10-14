use super::*;
pub(super) fn preflight(
    provider: &GithubProvider,
    identity: &HostIdentity,
    original: &PrSummary,
    context: &PrContext,
) -> GithubResult<()> {
    scope(provider, identity, &original.pr.repository)?;
    if original.pr.host != provider.host
        || original.pr.number == 0
        || !valid_opaque(&original.pr.id)
        || !crate::search::model::valid_sha(&original.head_sha)
        || !crate::search::model::valid_sha(&original.base_sha)
        || context.repository.host != provider.host
        || context.repository.id == 0
        || !context
            .repository
            .full_name
            .eq_ignore_ascii_case(&original.pr.repository)
        || context.base_repository.id != context.repository.id
        || context.base_repository.host != provider.host
        || !context
            .base_repository
            .full_name
            .eq_ignore_ascii_case(&original.pr.repository)
        || context.head_sha != original.head_sha
        || context.base_sha != original.base_sha
    {
        return Err(error(GithubErrorKind::InvalidTarget, "Catalogue requires original positive canonical requester/repository/PR node/number/full head and base context"));
    }
    Ok(())
}
pub(super) fn principal(
    provider: &GithubProvider,
    budget: &mut MetricsBudget,
    expected: &HostIdentity,
    cancel: &CancellationToken,
) -> GithubResult<HostIdentity> {
    let raw = budget.read_document(provider, "user", cancel)?;
    let identity = HostIdentity {
        host: provider.host.clone(),
        login: text(&raw, "login"),
        account_id: Some(positive(&raw, "id")?),
        gh_version: expected.gh_version.clone(),
    };
    validate_name(&identity.login)?;
    if identity.host != expected.host
        || identity.account_id != expected.account_id
        || expected.account_id.is_none_or(|id| id == 0)
    {
        return Err(error(
            GithubErrorKind::IdentityChanged,
            "Catalogue original positive numeric principal changed; private rows discarded",
        ));
    }
    Ok(identity)
}
pub(super) fn parent_identity(
    provider: &GithubProvider,
    original: &PrSummary,
    repo: &MetricsRepository,
    raw: &Value,
) -> GithubResult<()> {
    positive(raw, "id")?;
    if positive(raw, "number")? != original.pr.number
        || raw["node_id"].as_str() != Some(original.pr.id.as_str())
        || raw["head"]["sha"].as_str() != Some(original.head_sha.as_str())
        || raw["base"]["sha"].as_str() != Some(original.base_sha.as_str())
    {
        return Err(error(
            GithubErrorKind::StaleHead,
            "Catalogue original PR database/node/number/full head/base identity changed",
        ));
    }
    let base_repo = repository(&provider.host, &repo.full_name, &raw["base"]["repo"])?;
    require_same_repo(repo, &base_repo)
}
pub(super) fn issue_identity(
    provider: &GithubProvider,
    original: &PrSummary,
    raw: &Value,
) -> GithubResult<()> {
    positive(raw, "id")?;
    if !raw["node_id"].as_str().is_some_and(valid_opaque)
        || raw["number"].as_u64() != Some(original.pr.number)
        || !valid_html_url(
            &provider.host,
            &original.pr.repository,
            original.pr.number,
            &raw["html_url"],
        )
        || !valid_pull_api_url(
            &provider.host,
            &original.pr.repository,
            original.pr.number,
            &raw["pull_request"]["url"],
        )
        || !valid_html_url(
            &provider.host,
            &original.pr.repository,
            original.pr.number,
            &raw["pull_request"]["html_url"],
        )
    {
        return Err(error(
            GithubErrorKind::IdentityChanged,
            "Catalogue issue metadata belongs to another original PR/repository",
        ));
    }
    Ok(())
}
fn valid_pull_api_url(host: &str, repo: &str, number: u64, value: &Value) -> bool {
    let residency = host
        .strip_suffix(".ghe.com")
        .is_some_and(|tenant| !tenant.is_empty() && !tenant.contains('.'));
    let api_host = if host == "github.com" {
        "api.github.com".to_owned()
    } else if residency {
        format!("api.{host}")
    } else {
        host.to_owned()
    };
    value.as_str().is_some_and(|url| {
        url.eq_ignore_ascii_case(&format!("https://{api_host}/repos/{repo}/pulls/{number}"))
            || host != "github.com"
                && !residency
                && url.eq_ignore_ascii_case(&format!(
                    "https://{api_host}/api/v3/repos/{repo}/pulls/{number}"
                ))
    })
}
fn valid_html_url(host: &str, repo: &str, number: u64, value: &Value) -> bool {
    value.as_str().is_some_and(|url| {
        url.eq_ignore_ascii_case(&format!("https://{host}/{repo}/pull/{number}"))
    })
}
pub(super) fn require_same_context(first: &Value, last: &Value, issue: &Value) -> GithubResult<()> {
    for field in [
        "id",
        "node_id",
        "number",
        "state",
        "draft",
        "merged",
        "updated_at",
        "title",
        "body",
        "user",
        "labels",
        "assignees",
        "milestone",
        "requested_reviewers",
        "requested_teams",
        "maintainer_can_modify",
    ] {
        if first.get(field) != last.get(field) {
            return Err(error(GithubErrorKind::StaleHead,"PR metadata/lifecycle changed during catalogue reads; original form not retargeted"));
        }
    }
    for field in [
        "title",
        "body",
        "state",
        "user",
        "labels",
        "assignees",
        "milestone",
    ] {
        if first.get(field).is_some()
            && issue.get(field).is_some()
            && first.get(field) != issue.get(field)
        {
            return Err(error(
                GithubErrorKind::StaleHead,
                "PR and issue metadata observations disagree; no mixed context chosen",
            ));
        }
    }
    Ok(())
}
pub(super) fn observed_context(
    provider: &GithubProvider,
    original: &PrSummary,
    repo_raw: &Value,
    parent: &Value,
    issue: Value,
) -> GithubResult<(PrSummary, PrContext, Vec<String>)> {
    let mut reasons = vec![];
    for field in [
        "state",
        "draft",
        "merged",
        "updated_at",
        "title",
        "requested_reviewers",
        "requested_teams",
        "maintainer_can_modify",
    ] {
        if parent.get(field).is_none_or(Value::is_null) {
            reasons.push(format!(
                "Current PR {field} metadata unavailable; no default grants catalogue eligibility"
            ));
        }
    }
    for field in ["labels", "assignees"] {
        if !issue[field].is_array() {
            reasons.push(format!("Current issue {field} metadata unavailable"));
        }
    }
    let repository = provider.repo_from_json(repo_raw)?;
    if repo_raw["permissions"]["push"].as_bool().is_none() {
        reasons.push(
            "Current requester mutation capability missing; unverified, not allowed or denied"
                .into(),
        );
    }
    let target = PrSummary {
        pr: provider.pr_from_json(parent, Some(&repository.full_name))?,
        head_sha: original.head_sha.clone(),
        base_sha: original.base_sha.clone(),
        head_ref: text(&parent["head"], "ref"),
        base_ref: text(&parent["base"], "ref"),
        merged: boolean(parent, "merged"),
        auto_merge: !parent.get("auto_merge").unwrap_or(&Value::Null).is_null(),
        merge_commit_sha: parent["merge_commit_sha"].as_str().map(str::to_owned),
        mergeable: parent["mergeable"].as_bool(),
        mergeable_state: text(parent, "mergeable_state"),
        additions: number(parent, "additions"),
        deletions: number(parent, "deletions"),
        changed_files: number(parent, "changed_files"),
    };
    let head_repository = match parent["head"].get("repo") {
        Some(Value::Null) => None,
        Some(raw) => Some(provider.repo_from_json(raw)?),
        None => {
            reasons.push(
                "Head repository provenance omitted; unknown, not same-repository proof".into(),
            );
            None
        }
    };
    let base_repository = provider.repo_from_json(&parent["base"]["repo"])?;
    let requested_reviewers = parent["requested_reviewers"]
        .as_array()
        .into_iter()
        .flatten()
        .map(Actor::from_json)
        .collect();
    let mut requested_teams = vec![];
    if let Some(teams) = parent["requested_teams"].as_array() {
        for team in teams {
            let Some(id) = team["id"].as_u64().filter(|id| *id > 0) else {
                reasons.push("Requested team genuine ID missing; no ID zero substituted".into());
                continue;
            };
            requested_teams.push(RequestedTeam {
                id,
                slug: text(team, "slug"),
                name: text(team, "name"),
            });
        }
    }
    let context = PrContext {
        is_cross_repository: head_repository
            .as_ref()
            .is_some_and(|head| head.id != base_repository.id),
        maintainer_can_modify: parent["maintainer_can_modify"].as_bool().unwrap_or(false),
        repository,
        issue_metadata: issue,
        requested_reviewers,
        requested_teams,
        head_repository,
        base_repository,
        head_sha: original.head_sha.clone(),
        base_sha: original.base_sha.clone(),
        source_urls: vec![text(parent, "html_url")],
    };
    Ok((target, context, reasons))
}
