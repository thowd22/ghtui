use super::*;

/// An actual validated UTC source timestamp. Never synthesized from updated_at.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MetricsTimestamp {
    pub(super) raw: String,
    pub(super) seconds: u64,
}
impl MetricsTimestamp {
    pub fn raw(&self) -> &str {
        &self.raw
    }
    pub fn unix_seconds(&self) -> u64 {
        self.seconds
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MetricsRepository {
    pub host: String,
    pub id: u64,
    pub node_id: String,
    pub full_name: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MetricsActor {
    pub account_id: Option<u64>,
    pub actor: Actor,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MetricsTeam {
    pub id: u64,
    pub node_id: Option<String>,
    pub slug: String,
    pub name: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum MetricsPrState {
    Open,
    Closed,
    Unknown(String),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MetricsPrObservation {
    pub repository: MetricsRepository,
    pub id: u64,
    pub node_id: String,
    pub number: u64,
    pub title: String,
    pub author: MetricsActor,
    pub state: MetricsPrState,
    pub draft: Option<bool>,
    pub created_at: Option<MetricsTimestamp>,
    pub merged_at: Option<MetricsTimestamp>,
    pub closed_at: Option<MetricsTimestamp>,
    pub merged: Option<bool>,
    pub head_sha: Option<String>,
    pub requested_users: Option<Vec<MetricsActor>>,
    pub requested_teams: Option<Vec<MetricsTeam>>,
    /// Optimistic freshness version only. It is NOT an event/duration endpoint.
    pub observed_updated_at: Option<MetricsTimestamp>,
    pub reasons: Vec<String>,
}
#[derive(Clone, Debug)]
pub struct MetricsReadLimits {
    pub max_pages: usize,
    pub page_size: usize,
    pub max_rows: usize,
    pub max_document_bytes: usize,
    pub max_response_bytes: usize,
    pub max_duration: Duration,
}
impl Default for MetricsReadLimits {
    fn default() -> Self {
        Self {
            max_pages: 10,
            page_size: 100,
            max_rows: 1000,
            max_document_bytes: 256 * 1024,
            max_response_bytes: 16 * 1024 * 1024,
            max_duration: Duration::from_secs(30),
        }
    }
}
impl MetricsReadLimits {
    pub(super) fn validate(&self) -> GithubResult<()> {
        if !(1..=100).contains(&self.max_pages)
            || !(1..=100).contains(&self.page_size)
            || !(1..=10000).contains(&self.max_rows)
            || self.page_size > self.max_rows
            || !(1..=4 * 1024 * 1024).contains(&self.max_document_bytes)
            || !(1..=32 * 1024 * 1024).contains(&self.max_response_bytes)
            || self.max_duration.is_zero()
            || self.max_duration > Duration::from_secs(300)
        {
            return Err(error(GithubErrorKind::InvalidTarget,"Metrics require bounded positive pages, fixed page size, rows, document/response bytes and source duration"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub enum MetricsReadStatus {
    Complete,
    Partial,
    Unavailable(GithubError),
}
#[derive(Clone, Debug)]
pub struct MetricsReadCoverage {
    /// Exhaustion of this current API collection only, not lifetime/backfill.
    pub enumeration_complete: bool,
    /// All relevant observed metric metadata was understood and present.
    pub metric_evidence_complete: bool,
    pub complete_empty: bool,
    pub requested_pages: usize,
    pub fetched_pages: usize,
    pub fetched_rows: usize,
    pub admitted_rows: usize,
    pub excluded_rows: usize,
    pub response_bytes: usize,
    pub reasons: Vec<String>,
    pub status: MetricsReadStatus,
}
#[derive(Clone, Debug)]
pub struct RepositoryPrInventory {
    pub identity: HostIdentity,
    pub repository: MetricsRepository,
    pub prs: Vec<MetricsPrObservation>,
    pub coverage: MetricsReadCoverage,
    pub observed_from: u64,
    pub observed_through: u64,
    pub requests: usize,
    pub aggregate_response_bytes: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum MetricsReviewState {
    Pending,
    Approved,
    ChangesRequested,
    Commented,
    Dismissed,
    Unknown(String),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MetricsReviewObservation {
    pub id: u64,
    pub node_id: String,
    pub state: MetricsReviewState,
    pub submitted_at: Option<MetricsTimestamp>,
    pub actor: MetricsActor,
    pub commit_sha: Option<String>,
    pub reasons: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum MetricsRequestedReviewer {
    User(MetricsActor),
    Team(MetricsTeam),
    Unknown,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum MetricsTimelineKind {
    ReviewRequested {
        reviewer: MetricsRequestedReviewer,
    },
    ReviewRequestRemoved {
        reviewer: MetricsRequestedReviewer,
    },
    ReviewSubmitted {
        review: MetricsReviewObservation,
    },
    ReviewDismissed {
        review_id: Option<u64>,
    },
    Merged,
    Closed,
    Reopened,
    ReadyForReview,
    ConvertedToDraft,
    /// Known unrelated kinds do not become metric facts; their rows still count.
    Unrelated(String),
    /// Unknown future or unsupported relevant events prevent semantic completeness.
    Unsupported(String),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MetricsTimelineObservation {
    pub id: Option<u64>,
    pub node_id: Option<String>,
    pub at: Option<MetricsTimestamp>,
    pub actor: MetricsActor,
    pub kind: MetricsTimelineKind,
    pub reasons: Vec<String>,
}
#[derive(Clone, Debug)]
pub struct ExactPrMetricEvents {
    pub identity: HostIdentity,
    pub parent: MetricsPrObservation,
    pub reviews: Vec<MetricsReviewObservation>,
    pub review_coverage: MetricsReadCoverage,
    pub timeline: Vec<MetricsTimelineObservation>,
    pub timeline_coverage: MetricsReadCoverage,
    /// The interval of these reads, not an invented retained lifetime interval.
    pub observed_from: u64,
    pub observed_through: u64,
    pub requests: usize,
    pub aggregate_response_bytes: usize,
}
