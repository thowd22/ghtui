//! TASK-12 AC2: one bounded GraphQL read of the review decision and the head commit's
//! status-check rollup for the PRs an Inbox load just returned.
//!
//! The queue read (`search/issues`) carries neither, so the ordinary Inbox load follows it
//! with this read. It is read-only, takes its query in argv (no stdin), and sends one call per
//! [`INBOX_ROLLUP_BATCH_V1`] PRs in a fixed (repository, number) order. A PR GitHub does not
//! return (deleted, inaccessible, or a host without GraphQL) simply has no rollup, and the
//! caller keeps its session memo as the fallback.
use super::{error, validate_repo, GithubErrorKind, GithubProvider, GithubResult};
use crate::providers::runner::CancellationToken;
use serde_json::Value;

/// PRs per GraphQL call. Fifty aliased `repository.pullRequest` lookups stay far inside
/// GitHub's node limit and the request size of an ordinary read.
pub const INBOX_ROLLUP_BATCH_V1: usize = 50;

/// What GitHub reports for one PR. `None` fields mean GitHub returned null for them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InboxRollupV1 {
    pub repository: String,
    pub number: u64,
    /// `APPROVED`, `CHANGES_REQUESTED`, `REVIEW_REQUIRED`, or `None` when no review rule
    /// applies.
    pub review_decision: Option<String>,
    /// `SUCCESS`, `FAILURE`, `ERROR`, `PENDING`, `EXPECTED`, or `None` without any check.
    pub check_state: Option<String>,
    pub head_oid: String,
}

/// The exact GraphQL document for `targets`, in the order given.
pub fn inbox_rollup_query_v1(targets: &[(&str, u64)]) -> String {
    let mut query = String::from("query{");
    for (index, (repository, number)) in targets.iter().enumerate() {
        let (owner, name) = repository.split_once('/').unwrap_or((repository, ""));
        if index > 0 {
            query.push(' ');
        }
        query.push_str(&format!(
            "p{index}:repository(owner:\"{owner}\",name:\"{name}\"){{pullRequest(number:{number}){{reviewDecision commits(last:1){{nodes{{commit{{oid statusCheckRollup{{state}}}}}}}}}}}}"
        ));
    }
    query.push('}');
    query
}

/// The exact `gh` argv for one batch.
pub fn inbox_rollup_args_v1(host: &str, targets: &[(&str, u64)]) -> Vec<String> {
    vec![
        "api".into(),
        "graphql".into(),
        "--hostname".into(),
        host.into(),
        "-f".into(),
        format!("query={}", inbox_rollup_query_v1(targets)),
    ]
}

/// Sorts and de-duplicates targets the way the Inbox load sends them.
pub fn inbox_rollup_order_v1(targets: &mut Vec<(String, u64)>) {
    targets.sort_by(|a, b| {
        a.0.to_ascii_lowercase()
            .cmp(&b.0.to_ascii_lowercase())
            .then(a.1.cmp(&b.1))
    });
    targets.dedup_by(|a, b| a.0.eq_ignore_ascii_case(&b.0) && a.1 == b.1);
}

fn text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// Reads one response. A null alias (PR missing or not visible) yields no row.
pub fn parse_inbox_rollup_v1(
    targets: &[(&str, u64)],
    response: &Value,
) -> GithubResult<Vec<InboxRollupV1>> {
    let data = response
        .get("data")
        .filter(|data| data.is_object())
        .ok_or_else(|| error(GithubErrorKind::Json, "Inbox status response has no data"))?;
    let mut rows = Vec::new();
    for (index, (repository, number)) in targets.iter().enumerate() {
        let Some(pr) = data
            .get(format!("p{index}"))
            .and_then(|repo| repo.get("pullRequest"))
            .filter(|pr| pr.is_object())
        else {
            continue;
        };
        let commit = pr.pointer("/commits/nodes/0/commit");
        rows.push(InboxRollupV1 {
            repository: (*repository).to_owned(),
            number: *number,
            review_decision: text(pr.get("reviewDecision")),
            check_state: text(commit.and_then(|c| c.pointer("/statusCheckRollup/state"))),
            head_oid: text(commit.and_then(|c| c.get("oid"))).unwrap_or_default(),
        });
    }
    Ok(rows)
}

impl GithubProvider {
    /// One batch (at most [`INBOX_ROLLUP_BATCH_V1`] PRs). Read-only.
    pub fn inbox_rollup_v1(
        &self,
        targets: &[(&str, u64)],
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<InboxRollupV1>> {
        if targets.is_empty() {
            return Ok(Vec::new());
        }
        if targets.len() > INBOX_ROLLUP_BATCH_V1 {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Inbox status batch is too large",
            ));
        }
        for (repository, number) in targets {
            validate_repo(repository)?;
            if *number == 0 {
                return Err(error(
                    GithubErrorKind::InvalidTarget,
                    "Resource number must be positive",
                ));
            }
        }
        let bytes = self.execute(
            inbox_rollup_args_v1(&self.host, targets),
            None,
            None,
            cancel,
        )?;
        let response: Value = serde_json::from_slice(&bytes)
            .map_err(|e| error(GithubErrorKind::Json, e.to_string()))?;
        parse_inbox_rollup_v1(targets, &response)
    }
}

#[cfg(test)]
#[path = "inbox_rollup_v1_tests.rs"]
mod tests;
