use crate::{
    config::validate_repository,
    identity::ActorIdentity,
    providers::gh::{Actor, HostIdentity},
    search::model::valid_sha,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetricsScope {
    host: String,
    account_id: u64,
    login: String,
    repositories: BTreeSet<String>,
}
impl MetricsScope {
    pub(crate) fn validate(&self) -> Result<(), String> {
        let identity = HostIdentity {
            host: self.host.clone(),
            login: self.login.clone(),
            account_id: Some(self.account_id),
            gh_version: String::new(),
        };
        let checked = Self::new(
            &identity,
            &self.repositories.iter().cloned().collect::<Vec<_>>(),
        )?;
        if &checked != self {
            return Err(
                "Metric scope must retain its positive canonical principal/repositories".into(),
            );
        }
        Ok(())
    }
    pub fn new(identity: &HostIdentity, repositories: &[String]) -> Result<Self, String> {
        let account_id = identity
            .account_id
            .filter(|id| *id > 0)
            .ok_or("Metrics require a freshly verified positive account ID")?;
        if identity.host.is_empty()
            || identity
                .host
                .bytes()
                .any(|byte| !byte.is_ascii_alphanumeric() && !b".-".contains(&byte))
            || identity.login.is_empty()
        {
            return Err("Metrics require an explicit canonical host and current login".into());
        }
        let repositories = repositories
            .iter()
            .map(|repository| {
                validate_repository(repository).map_err(|error| error.to_string())?;
                Ok(repository.to_ascii_lowercase())
            })
            .collect::<Result<BTreeSet<_>, String>>()?;
        if repositories.is_empty() || repositories.len() > 64 {
            return Err("Choose1..64 explicit metric repositories".into());
        }
        Ok(Self {
            host: identity.host.to_ascii_lowercase(),
            account_id,
            login: identity.login.to_ascii_lowercase(),
            repositories,
        })
    }
    pub fn host(&self) -> &str {
        &self.host
    }
    pub fn account_id(&self) -> u64 {
        self.account_id
    }
    pub fn login(&self) -> &str {
        &self.login
    }
    pub fn repositories(&self) -> &BTreeSet<String> {
        &self.repositories
    }
    pub fn accepts(&self, repository: &str) -> bool {
        self.repositories.contains(&repository.to_ascii_lowercase())
    }
    pub fn matches(&self, identity: &HostIdentity) -> bool {
        identity.account_id == Some(self.account_id)
            && identity.host.eq_ignore_ascii_case(&self.host)
            && identity.login.eq_ignore_ascii_case(&self.login)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UtcWindow {
    pub start: u64,
    pub end: u64,
}
impl UtcWindow {
    pub fn new(start: u64, end: u64) -> Result<Self, String> {
        if start >= end {
            Err("UTC window requires start < end; boundaries are [start,end)".into())
        } else {
            Ok(Self { start, end })
        }
    }
    pub fn contains(self, at: u64) -> bool {
        at >= self.start && at < self.end
    }
}
pub fn utc_seconds(value: &str) -> Option<u64> {
    crate::github_workspace::timestamp_seconds(value)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoverageState {
    Complete,
    Partial(Vec<String>),
    Unavailable(String),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceCoverage {
    pub state: CoverageState,
    /// A producer explicitly enumerated an empty source; not inferred from Vec::is_empty.
    pub complete_empty: bool,
    pub fetched_at: Option<u64>,
}
impl EvidenceCoverage {
    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            state: CoverageState::Unavailable(reason.into()),
            complete_empty: false,
            fetched_at: None,
        }
    }
    pub fn partial(&mut self, reason: impl Into<String>) {
        let reason = reason.into();
        match &mut self.state {
            CoverageState::Complete => self.state = CoverageState::Partial(vec![reason]),
            CoverageState::Partial(reasons) => {
                if !reasons.contains(&reason) {
                    reasons.push(reason)
                }
            }
            CoverageState::Unavailable(_) => {}
        }
        self.complete_empty = false;
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrCohortKind {
    PersonalQueueUnion,
    SelectedRepositoryInventory,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrState {
    Open,
    Merged,
    ClosedWithoutMergeEvidence,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PrTarget {
    pub repository: String,
    pub number: u64,
    pub repository_id: Option<u64>,
    /// Positive PR database ID supplied by the dedicated typed provider only.
    pub database_id: Option<u64>,
    /// Genuine opaque node from a typed provider sidecar, never inferred from raw IDs.
    pub node_id: Option<String>,
    /// Legacy source identifier retained as evidence only; not an opaque-node assertion.
    pub source_record_id: Option<String>,
}
impl PrTarget {
    pub fn validate(&self, scope: &MetricsScope) -> Result<(), String> {
        validate_repository(&self.repository).map_err(|error| error.to_string())?;
        if self.number == 0
            || !scope.accepts(&self.repository)
            || self.repository_id == Some(0)
            || self.database_id == Some(0)
            || self
                .node_id
                .as_ref()
                .is_some_and(|node| node.is_empty() || node.chars().any(char::is_control))
        {
            return Err(
                "Metric PR is outside the explicit scope or lacks a positive number".into(),
            );
        }
        Ok(())
    }
    pub fn has_canonical_identity(&self) -> bool {
        self.repository_id.is_some_and(|id| id > 0)
            && self.database_id.is_some_and(|id| id > 0)
            && self.node_id.as_ref().is_some_and(|node| !node.is_empty())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum MetricTarget {
    PullRequest(PrTarget),
    Review {
        pr: PrTarget,
        review_id: u64,
    },
    Event {
        pr: PrTarget,
        event_id: u64,
    },
    Check {
        pr: PrTarget,
        head_sha: String,
        check_id: u64,
        kind: String,
    },
    Run {
        repository: String,
        repository_id: Option<u64>,
        run_id: u64,
        attempt: u64,
        workflow_id: u64,
        head_sha: String,
    },
}
impl MetricTarget {
    pub fn validate(&self, scope: &MetricsScope) -> Result<(), String> {
        match self {
            Self::PullRequest(pr) => pr.validate(scope),
            Self::Review { pr, review_id }
            | Self::Event {
                pr,
                event_id: review_id,
            } => {
                pr.validate(scope)?;
                if *review_id == 0 {
                    Err("Metric source event/review requires a positive stable ID".into())
                } else {
                    Ok(())
                }
            }
            Self::Check {
                pr,
                head_sha,
                check_id,
                kind,
            } => {
                pr.validate(scope)?;
                if *check_id == 0
                    || !valid_sha(head_sha)
                    || !matches!(kind.as_str(), "check" | "status")
                {
                    Err("Metric check requires its exact kind/ID/full head SHA".into())
                } else {
                    Ok(())
                }
            }
            Self::Run {
                repository,
                repository_id,
                run_id,
                attempt,
                workflow_id,
                head_sha,
            } => {
                if !scope.accepts(repository)
                    || *repository_id == Some(0)
                    || *run_id == 0
                    || *attempt == 0
                    || *workflow_id == 0
                    || !valid_sha(head_sha)
                {
                    Err(
                        "Metric run requires its exact scoped run/attempt/workflow/full head SHA"
                            .into(),
                    )
                } else {
                    Ok(())
                }
            }
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetricActor {
    pub account_id: Option<u64>,
    pub actor: Actor,
}
impl MetricActor {
    pub fn identity(&self, scope: &MetricsScope) -> ActorIdentity {
        ActorIdentity::github(scope.host(), &self.actor)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubmittedReviewState {
    Approved,
    ChangesRequested,
    Commented,
    Dismissed,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    ReviewRequested {
        reviewer_id: u64,
    },
    ReviewRequestRemoved {
        reviewer_id: u64,
    },
    ReviewSubmitted {
        review_id: u64,
        state: SubmittedReviewState,
    },
    Merged,
    Reopened,
    Closed,
    ReadyForReview,
    ConvertedToDraft,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceEvent {
    pub id: u64,
    pub at: u64,
    pub kind: EventKind,
    pub actor: MetricActor,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventCategory {
    Reviews,
    Timeline,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventBinding {
    /// Stable host/numeric principal, independent of mutable login or view repository set.
    pub scope: crate::storage::Scope,
    pub parent: PrTarget,
    pub category: EventCategory,
}
impl EventBinding {
    pub fn for_parent(
        scope: &MetricsScope,
        parent: PrTarget,
        category: EventCategory,
    ) -> Result<Self, String> {
        scope.validate()?;
        parent.validate(scope)?;
        Ok(Self {
            scope: crate::storage::Scope::github(scope.host(), Some(scope.account_id()))
                .map_err(|error| error.to_string())?,
            parent,
            category,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventCollection {
    pub binding: Option<EventBinding>,
    pub events: Vec<SourceEvent>,
    /// Explicit retained/backfilled category interval. A current file-page flag is not this proof.
    pub from: Option<u64>,
    pub through: Option<u64>,
    pub complete: bool,
    pub source: String,
    pub generation: u64,
    pub reasons: Vec<String>,
}
impl EventCollection {
    pub fn unavailable() -> Self {
        Self {
            binding: None,
            events: vec![],
            from: None,
            through: None,
            complete: false,
            source: String::new(),
            generation: 0,
            reasons: vec!["No retained category events/backfill coverage".into()],
        }
    }
    pub fn covers_lifetime(&self, created: u64, as_of: u64) -> bool {
        self.complete
            && !self.source.is_empty()
            && self.generation > 0
            && self.from.is_some_and(|from| from <= created)
            && self.through.is_some_and(|through| through >= as_of)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrEvidence {
    pub target: PrTarget,
    /// Extra selected-detail proof may support checks without fabricating queue membership.
    pub cohort_member: bool,
    pub title: String,
    pub author: MetricActor,
    pub state: PrState,
    pub draft: Option<bool>,
    pub head_sha: Option<String>,
    pub created_at: Option<u64>,
    pub review_requested_now: Option<bool>,
    pub timeline: EventCollection,
    pub reviews: EventCollection,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Health {
    Success,
    Failure,
    Pending,
    ActionRequired,
    Cancelled,
    Neutral,
    Unknown,
}
pub fn health(status: &str, conclusion: &str) -> Health {
    match status.to_ascii_lowercase().as_str() {
        "completed" => match conclusion.to_ascii_lowercase().as_str() {
            "success" => Health::Success,
            "failure" | "timed_out" | "startup_failure" => Health::Failure,
            "action_required" => Health::ActionRequired,
            "cancelled" => Health::Cancelled,
            "skipped" | "neutral" | "stale" => Health::Neutral,
            _ => Health::Unknown,
        },
        "queued" | "waiting" | "pending" | "requested" | "in_progress" => {
            if conclusion.is_empty() {
                Health::Pending
            } else {
                Health::Unknown
            }
        }
        "action_required" => Health::ActionRequired,
        _ => Health::Unknown,
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckEvidence {
    pub target: MetricTarget,
    pub name: String,
    pub health: Health,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunEvidence {
    pub target: MetricTarget,
    pub title: String,
    pub actor: MetricActor,
    pub created_at: Option<u64>,
    pub observed_at: u64,
    pub health: Health,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DashboardEvidence {
    pub scope: MetricsScope,
    pub cohort: PrCohortKind,
    pub prs: Vec<PrEvidence>,
    pub pr_coverage: EvidenceCoverage,
    pub checks: Vec<CheckEvidence>,
    pub check_coverage: EvidenceCoverage,
    pub runs: Vec<RunEvidence>,
    pub run_coverage: EvidenceCoverage,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MetricKind {
    OpenPrs,
    NeedsMyReview,
    OpenAge,
    ReviewWait,
    FirstReview,
    MergeLatency,
    Checks,
    Workflows,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MetricValue {
    Count(u64),
    MeanSeconds {
        sum: u128,
        samples: u64,
    },
    HealthCounts {
        success: u64,
        failure: u64,
        pending: u64,
        action_required: u64,
        cancelled: u64,
        neutral: u64,
        unknown: u64,
    },
    Unavailable,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Contributor {
    pub target: MetricTarget,
    pub title: String,
    pub actors: Vec<MetricActor>,
    pub measured_seconds: Option<u64>,
    pub health: Option<Health>,
    pub evidence: Vec<MetricTarget>,
    pub note: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MetricCard {
    pub kind: MetricKind,
    pub label: String,
    pub formula: String,
    pub cohort: String,
    pub value: MetricValue,
    pub coverage: EvidenceCoverage,
    pub contributors: Vec<Contributor>,
    pub unknown: u64,
    pub censored: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DashboardSnapshot {
    pub scope: MetricsScope,
    pub as_of: u64,
    pub window: UtcWindow,
    pub cards: Vec<MetricCard>,
    pub notices: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MetricsAction {
    Window(UtcWindow),
    Repository(String),
    AllSelected,
    Card(MetricKind),
    Open(MetricTarget),
    Back,
    Refresh,
}
