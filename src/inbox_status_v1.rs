//! TASK-12 AC2: review and check state on Inbox rows.
//!
//! The Inbox queue read (`search/issues`) carries no review decision or check rollup, so the
//! ordinary Inbox load follows it with one GraphQL read per 50 rows
//! (`providers::gh::inbox_rollup_v1`) and keeps the result on the workspace
//! (`GithubWorkspace::inbox_rollup_v1`). This memo is the fallback for rows that read did not
//! cover (a host without GraphQL, a failed batch, a scoped catalogue Inbox): it is derived only
//! from data this session already fetched, the reviews of a PR page that was opened and the
//! checks of its Checks tab. Nothing here starts I/O. A page read after the rollup (same or
//! newer `updated_at`) wins over the rollup, keeping the rollup's checks for the same head.
//!
//! The memo belongs to one verified account; another account (or a hidden identity) never
//! sees it. A row whose PR changed on GitHub after the page was read (a newer `updated_at` in
//! the queue) keeps its last state but is labelled stale, and a row never opened says so
//! instead of inventing a status.
use std::collections::BTreeMap;

use serde_json::Value;

use crate::github_workspace::{GithubWorkspace, PrKey, PrPage};
use crate::providers::gh::{Check, PullRequest};
use crate::theme::Role;

/// The review state the opened PR's submitted reviews show (latest review per reviewer).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewState {
    Approved,
    ChangesRequested,
    /// Only comment-only or dismissed reviews.
    Reviewed,
    NoReviews,
    /// GitHub's `REVIEW_REQUIRED` decision: a rule needs an approving review.
    ReviewRequired,
    /// GitHub reported no review decision (no review rule applies).
    NoDecision,
}
impl ReviewState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Approved => "Approved",
            Self::ChangesRequested => "Changes requested",
            Self::Reviewed => "Reviewed",
            Self::NoReviews => "No reviews",
            Self::ReviewRequired => "Review required",
            Self::NoDecision => "No review decision",
        }
    }
}

/// The rollup of the head commit's check runs and commit statuses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckState {
    Passing,
    Failing,
    Pending,
    NoChecks,
}
impl CheckState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Passing => "checks pass",
            Self::Failing => "checks fail",
            Self::Pending => "checks pending",
            Self::NoChecks => "no checks",
        }
    }
}

/// What one opened PR page showed. `checks` is `None` until its Checks tab loaded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowStatus {
    pub review: ReviewState,
    pub checks: Option<CheckState>,
    pub head_sha: String,
    pub updated_at: String,
}

/// The status line for one Inbox row: its cell text and theme role.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowLabel {
    pub text: String,
    pub role: Role,
    pub known: bool,
}

/// Text shown for a row whose PR page was never read in this session.
pub const NOT_LOADED: &str = "review/checks not loaded";

#[derive(Clone, Debug, Default)]
pub struct InboxStatusMemo {
    account: Option<(String, u64)>,
    rows: BTreeMap<PrKey, RowStatus>,
}

/// The case-insensitive key shared by the memo and the workspace rollup.
pub fn row_key(host: &str, repository: &str, number: u64) -> PrKey {
    PrKey {
        host: host.to_ascii_lowercase(),
        repository: repository.to_ascii_lowercase(),
        number,
    }
}

/// Latest decisive review per reviewer: any outstanding change request wins, then an
/// approval. A dismissed review clears that reviewer's decision; pending (unsubmitted)
/// reviews are the viewer's own drafts and never count.
pub fn review_state(reviews: &[Value]) -> ReviewState {
    let mut decisions: BTreeMap<String, &str> = BTreeMap::new();
    let mut any = false;
    for review in reviews {
        let state = review.get("state").and_then(Value::as_str).unwrap_or("");
        let login = review
            .pointer("/user/login")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_ascii_lowercase();
        match state {
            "APPROVED" | "CHANGES_REQUESTED" => {
                any = true;
                decisions.insert(login, state);
            }
            "DISMISSED" => {
                any = true;
                decisions.remove(&login);
            }
            "COMMENTED" => any = true,
            _ => {}
        }
    }
    if decisions
        .values()
        .any(|state| *state == "CHANGES_REQUESTED")
    {
        ReviewState::ChangesRequested
    } else if decisions.values().any(|state| *state == "APPROVED") {
        ReviewState::Approved
    } else if any {
        ReviewState::Reviewed
    } else {
        ReviewState::NoReviews
    }
}

/// Any failed conclusion fails the rollup; otherwise any unfinished check keeps it pending.
pub fn check_state(checks: &[Check]) -> CheckState {
    if checks.is_empty() {
        return CheckState::NoChecks;
    }
    let failed = |check: &Check| {
        matches!(
            check.conclusion.to_ascii_lowercase().as_str(),
            "failure" | "error" | "timed_out" | "cancelled" | "action_required" | "startup_failure"
        )
    };
    if checks.iter().any(failed) {
        CheckState::Failing
    } else if checks
        .iter()
        .any(|check| !check.status.eq_ignore_ascii_case("completed"))
    {
        CheckState::Pending
    } else {
        CheckState::Passing
    }
}

impl RowStatus {
    /// What the Inbox load's GraphQL read reported, stamped with the queue's `updated_at`.
    pub fn from_rollup_v1(row: &crate::providers::gh::InboxRollupV1, updated_at: &str) -> Self {
        let review = match row.review_decision.as_deref() {
            Some("APPROVED") => ReviewState::Approved,
            Some("CHANGES_REQUESTED") => ReviewState::ChangesRequested,
            Some("REVIEW_REQUIRED") => ReviewState::ReviewRequired,
            _ => ReviewState::NoDecision,
        };
        let checks = match row.check_state.as_deref() {
            None => CheckState::NoChecks,
            Some("SUCCESS") => CheckState::Passing,
            Some("FAILURE" | "ERROR") => CheckState::Failing,
            Some(_) => CheckState::Pending,
        };
        Self {
            review,
            checks: Some(checks),
            head_sha: row.head_oid.clone(),
            updated_at: updated_at.to_owned(),
        }
    }
}

impl InboxStatusMemo {
    fn owner(github: &GithubWorkspace) -> Option<(String, u64)> {
        Some((
            github.identity.host.to_ascii_lowercase(),
            github.identity.account_id?,
        ))
    }

    /// Records what the open PR page shows. Called while painting the GitHub view; it only
    /// reads the page already in memory. Another account (or a hidden one) clears the memo.
    pub fn observe(&mut self, github: &GithubWorkspace) {
        let owner = Self::owner(github);
        if owner != self.account {
            self.rows.clear();
            self.account = owner.clone();
        }
        if owner.is_none() {
            return;
        }
        if let Some(page) = &github.pr {
            self.record(page);
        }
    }

    fn record(&mut self, page: &PrPage) {
        let summary = &page.details.summary;
        let pr = &summary.pr;
        let key = row_key(&pr.host, &pr.repository, pr.number);
        let old = self.rows.get(&key);
        let checks = match &page.checks {
            Some(checks) => Some(check_state(checks)),
            // A Checks tab read earlier for this same head still holds.
            None => old
                .filter(|old| old.head_sha == summary.head_sha)
                .and_then(|old| old.checks),
        };
        let status = RowStatus {
            review: review_state(&page.details.reviews),
            checks,
            head_sha: summary.head_sha.clone(),
            updated_at: pr.updated_at.clone(),
        };
        if old != Some(&status) {
            self.rows.insert(key, status);
        }
    }

    pub fn get(&self, github: &GithubWorkspace, pr: &PullRequest) -> Option<&RowStatus> {
        if Self::owner(github).is_none() || Self::owner(github) != self.account {
            return None;
        }
        self.rows.get(&row_key(&pr.host, &pr.repository, pr.number))
    }

    /// The row's status text: review and checks with words (colour is redundant), marked
    /// stale when the queue shows a newer update than the page that was read.
    pub fn label(&self, github: &GithubWorkspace, pr: &PullRequest) -> RowLabel {
        let rollup = Self::owner(github).and(github.inbox_rollup_v1.get(&row_key(
            &pr.host,
            &pr.repository,
            pr.number,
        )));
        let merged;
        let status = match (self.get(github, pr), rollup) {
            (Some(page), Some(rollup)) if page.updated_at >= rollup.updated_at => {
                merged = RowStatus {
                    checks: page.checks.or_else(|| {
                        (page.head_sha == rollup.head_sha)
                            .then_some(rollup.checks)
                            .flatten()
                    }),
                    ..page.clone()
                };
                Some(&merged)
            }
            (_, Some(rollup)) => Some(rollup),
            (page, None) => page,
        };
        let Some(status) = status else {
            return RowLabel {
                text: NOT_LOADED.into(),
                role: Role::TextMuted,
                known: false,
            };
        };
        let checks = status.checks.map_or("checks not loaded", CheckState::label);
        let stale = !pr.updated_at.is_empty() && pr.updated_at != status.updated_at;
        let text = format!(
            "{}, {checks}{}",
            status.review.label(),
            if stale { " (stale)" } else { "" }
        );
        let role = if stale {
            Role::TextMuted
        } else if status.review == ReviewState::ChangesRequested
            || status.checks == Some(CheckState::Failing)
        {
            Role::Failure
        } else if status.checks == Some(CheckState::Pending) {
            Role::Warning
        } else if status.review == ReviewState::Approved
            && matches!(
                status.checks,
                Some(CheckState::Passing | CheckState::NoChecks)
            )
        {
            Role::Success
        } else {
            Role::Text
        };
        RowLabel {
            text,
            role,
            known: true,
        }
    }
}

#[cfg(test)]
#[path = "inbox_status_v1_tests.rs"]
mod tests;
