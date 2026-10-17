//! Data-only complete membership validation shared by legacy and original Core.
use super::*;
use crate::pr_metadata_actions::{PrMetadataCommand, PrMetadataMember, PrMetadataPlan};

pub(super) fn validate_result_v48(
    plan: &PrMetadataPlan,
    fresh: &PrMetadataCatalogueRead,
    expected: &[PrMetadataMember],
    raw: &Value,
) -> GithubResult<()> {
    validate_complete_v48(
        fresh,
        plan.command(),
        plan.selected(),
        expected,
        plan.preserved_review_members(),
        raw,
    )
}

pub(super) fn validate_complete_v48(
    fresh: &PrMetadataCatalogueRead,
    command: PrMetadataCommand,
    selected: Option<&PrMetadataMember>,
    expected: &[PrMetadataMember],
    preserved: &[PrMetadataMember],
    raw: &Value,
) -> GithubResult<()> {
    let pr = &raw["data"]["node"];
    let pr_id = fresh.parent_raw()["id"]
        .as_u64()
        .filter(|id| *id > 0)
        .ok_or_else(|| {
            error(
                GithubErrorKind::IdentityChanged,
                "Actual PR database identity unavailable",
            )
        })?;
    let database = pr["fullDatabaseId"].as_u64().or_else(|| {
        pr["fullDatabaseId"]
            .as_str()
            .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|s| s.parse().ok())
    });
    if pr["__typename"].as_str() != Some("PullRequest")
        || pr["id"].as_str() != Some(fresh.target().pr.id.as_str())
        || database != Some(pr_id)
        || pr["number"].as_u64() != Some(fresh.target().pr.number)
        || pr["repository"]["id"].as_str() != Some(fresh.canonical_repository().node_id.as_str())
        || !pr["repository"]["nameWithOwner"].as_str().is_some_and(|s| {
            s.eq_ignore_ascii_case(fresh.canonical_repository().full_name.as_str())
        })
        || pr["headRefOid"].as_str() != Some(fresh.target().head_sha.as_str())
        || pr["baseRefOid"].as_str() != Some(fresh.target().base_sha.as_str())
        || pr["headRefName"].as_str() != Some(fresh.target().head_ref.as_str())
        || pr["baseRefName"].as_str() != Some(fresh.target().base_ref.as_str())
        || pr["state"].as_str().map(str::to_ascii_lowercase).as_deref()
            != fresh.parent_raw()["state"].as_str()
        || pr["isDraft"].as_bool() != fresh.parent_raw()["draft"].as_bool()
    {
        return Err(error(
            GithubErrorKind::IdentityChanged,
            "Exact original GraphQL PR/repository/full head/base/ref/lifecycle identity changed",
        ));
    }
    let field = command.field();
    let owner = fresh
        .canonical_repository()
        .full_name
        .as_str()
        .split_once('/')
        .ok_or_else(|| {
            error(
                GithubErrorKind::InvalidTarget,
                "Canonical repository owner unavailable",
            )
        })?
        .0;
    if command == crate::pr_metadata_actions::PrMetadataCommand::AddTeam && selected.is_some() {
        let selected = selected.ok_or_else(|| {
            error(
                GithubErrorKind::InvalidTarget,
                "Selected genuine team unavailable",
            )
        })?;
        let candidate = &raw["data"]["candidate"];
        if candidate["__typename"].as_str() != Some("Team")
            || candidate["id"].as_str() != Some(selected.node())
            || candidate["slug"].as_str() != Some(selected.name())
            || !candidate["organization"]["login"]
                .as_str()
                .is_some_and(|s| s.eq_ignore_ascii_case(owner))
        {
            return Err(error(GithubErrorKind::IdentityChanged,"Selected actual Team node organization/slug unavailable or changed; no slug-only targeting"));
        }
    }
    let actual_nodes: Vec<(&str, &Value)> = if field == "milestone" {
        match pr.get("milestone") {
            Some(Value::Null) => vec![],
            Some(value) if value.is_object() => vec![("milestone", value)],
            _ => {
                return Err(error(
                    GithubErrorKind::Json,
                    "Milestone omitted/unknown is not empty or clear authority",
                ))
            }
        }
    } else {
        let conn = &pr[if matches!(field, "reviewers" | "teams") {
            "reviewRequests"
        } else {
            field
        }];
        let rows = conn["nodes"].as_array().ok_or_else(|| {
            error(
                GithubErrorKind::Json,
                "Current metadata connection omitted or malformed",
            )
        })?;
        if rows.len() > 100
            || conn["totalCount"].as_u64() != Some(rows.len() as u64)
            || conn["pageInfo"]["hasNextPage"].as_bool() != Some(false)
        {
            return Err(error(GithubErrorKind::InvalidTarget,"Current metadata connection partial/capped/unknown; no truncated replacement or result receipt"));
        }
        let mut selected = vec![];
        let mut request_nodes = BTreeSet::new();
        for row in rows {
            if matches!(field, "reviewers" | "teams") {
                if !row["id"]
                    .as_str()
                    .is_some_and(|id| valid_opaque(id) && request_nodes.insert(id))
                {
                    return Err(error(
                        GithubErrorKind::Json,
                        "Review request genuine node missing",
                    ));
                }
                let who = &row["requestedReviewer"];
                let kind=match who["__typename"].as_str() { Some("User")=>"reviewers",Some("Team")=>"teams",_=>return Err(error(GithubErrorKind::Unsupported,"Unknown/bot/mannequin/enterprise-team review target cannot prove current User/Team membership")) };
                selected.push((kind, who));
            } else {
                selected.push((field, row));
            }
        }
        selected
    };
    if actual_nodes.len() != expected.len() + preserved.len() {
        return Err(error(GithubErrorKind::StaleHead,"Actual complete current result User+Team/member count differs from original exact intent"));
    }
    let mut seen = BTreeSet::new();
    for (kind, raw) in actual_nodes {
        let node = raw["id"]
            .as_str()
            .filter(|s| valid_opaque(s))
            .ok_or_else(|| {
                error(
                    GithubErrorKind::Json,
                    "Actual field member opaque node unavailable",
                )
            })?;
        if !seen.insert(node) {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Duplicate actual field node cannot prove membership",
            ));
        }
        let required = if kind == field { expected } else { preserved };
        let actual = required.iter().find(|m| m.node() == node).ok_or_else(|| {
            error(
                GithubErrorKind::StaleHead,
                "Original actual member node absent/replaced; no name-only match",
            )
        })?;
        let key = match kind {
            "labels" => "name",
            "teams" => "slug",
            "milestone" => "title",
            _ => "login",
        };
        if raw[key].as_str() != Some(actual.name())
            || kind == "milestone" && raw["number"].as_u64() != actual.number()
            || kind == "teams"
                && !raw["organization"]["login"].as_str().is_some_and(|s| {
                    s.eq_ignore_ascii_case(owner)
                        && actual
                            .namespace()
                            .is_none_or(|known| known.eq_ignore_ascii_case(s))
                })
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Actual member value/namespace differs from original genuine typed node",
            ));
        }
    }
    Ok(())
}

/// Same exact REST positive-ID/node/opposite-set parser; no transport or clock.
pub(super) fn validate_rest_result_v48(
    plan: &PrMetadataPlan,
    parent: &Value,
    issue: &Value,
) -> GithubResult<()> {
    let name = plan.target().repository().to_ascii_lowercase();
    let expected = plan.after();
    let raw = match plan.command().field() {
        "reviewers" => parent.get("requested_reviewers"),
        "teams" => parent.get("requested_teams"),
        field => issue.get(field),
    }
    .ok_or_else(|| {
        error(
            GithubErrorKind::Json,
            "Selected actual REST readback omitted; unknown not confirmation",
        )
    })?;
    let rows: Vec<&Value> = if plan.command().field() == "milestone" {
        if raw.is_null() {
            vec![]
        } else {
            vec![raw]
        }
    } else {
        raw.as_array()
            .ok_or_else(|| error(GithubErrorKind::Json, "Actual REST field set unavailable"))?
            .iter()
            .collect()
    };
    if rows.len() > 100 || rows.len() != expected.len() {
        return Err(error(
            GithubErrorKind::AmbiguousWrite,
            "Actual REST positive ID result set does not equal requested effect",
        ));
    }
    let mut seen = BTreeSet::new();
    for row in rows {
        let actual = PrMetadataMember::from_rest(row, plan.command().field(), &name)
            .map_err(|e| error(GithubErrorKind::Json, e))?;
        if !seen.insert(actual.id()) || !expected.contains(&actual) {
            return Err(error(GithubErrorKind::AmbiguousWrite,"Actual REST database/node/value result differs; same-name replacement is not confirmation"));
        }
    }
    if matches!(plan.command().field(), "reviewers" | "teams") {
        let opposite = if plan.command().field() == "reviewers" {
            "teams"
        } else {
            "reviewers"
        };
        let key = if opposite == "teams" {
            "requested_teams"
        } else {
            "requested_reviewers"
        };
        let rows = parent.get(key).and_then(Value::as_array).ok_or_else(|| {
            error(
                GithubErrorKind::Json,
                "Preserved opposite review-request membership omitted; unknown is not empty",
            )
        })?;
        let preserved = plan.preserved_review_members();
        if rows.len() != preserved.len() || rows.len() > 100 {
            return Err(error(
                GithubErrorKind::AmbiguousWrite,
                "Preserved opposite actual review-request set changed after aggregate mutation",
            ));
        }
        let mut seen = BTreeSet::new();
        for row in rows {
            let actual = PrMetadataMember::from_rest(row, opposite, &name)
                .map_err(|e| error(GithubErrorKind::Json, e))?;
            if !seen.insert(actual.id()) || !preserved.contains(&actual) {
                return Err(error(GithubErrorKind::AmbiguousWrite,"Preserved opposite actual reviewer/team IDs/nodes differ; no name-only confirmation"));
            }
        }
    }
    Ok(())
}
