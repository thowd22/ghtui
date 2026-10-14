use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum MetricsHealthStatus {
    Queued,
    InProgress,
    Completed,
    Waiting,
    Requested,
    Pending,
    ActionRequired,
    Unknown(Option<String>),
}
impl MetricsHealthStatus {
    pub fn api_value(&self) -> Option<&str> {
        match self {
            Self::Queued => Some("queued"),
            Self::InProgress => Some("in_progress"),
            Self::Completed => Some("completed"),
            Self::Waiting => Some("waiting"),
            Self::Requested => Some("requested"),
            Self::Pending => Some("pending"),
            Self::ActionRequired => Some("action_required"),
            Self::Unknown(value) => value.as_deref(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum MetricsHealthConclusion {
    Success,
    Failure,
    Neutral,
    Cancelled,
    Skipped,
    TimedOut,
    ActionRequired,
    Stale,
    StartupFailure,
    Unknown(String),
}
impl MetricsHealthConclusion {
    pub fn api_value(&self) -> &str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Neutral => "neutral",
            Self::Cancelled => "cancelled",
            Self::Skipped => "skipped",
            Self::TimedOut => "timed_out",
            Self::ActionRequired => "action_required",
            Self::Stale => "stale",
            Self::StartupFailure => "startup_failure",
            Self::Unknown(value) => value,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum MetricsCommitStatusState {
    Success,
    Failure,
    Error,
    Pending,
    Unknown(Option<String>),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum MetricsCheckResult {
    CheckRun {
        status: MetricsHealthStatus,
        conclusion: Option<MetricsHealthConclusion>,
    },
    CommitStatus {
        state: MetricsCommitStatusState,
    },
}
impl MetricsCheckResult {
    pub fn kind(&self) -> &str {
        match self {
            Self::CheckRun { .. } => "check",
            Self::CommitStatus { .. } => "status",
        }
    }
    /// Formula bridge only; raw typed source state remains retained above.
    pub fn health_status(&self) -> &str {
        match self {
            Self::CheckRun {
                status: MetricsHealthStatus::Unknown(_),
                ..
            } => "unknown",
            Self::CheckRun { status, .. } => status.api_value().unwrap_or("unknown"),
            Self::CommitStatus {
                state: MetricsCommitStatusState::Pending,
            } => "pending",
            Self::CommitStatus {
                state: MetricsCommitStatusState::Unknown(_),
            } => "unknown",
            Self::CommitStatus { .. } => "completed",
        }
    }
    pub fn health_conclusion(&self) -> &str {
        match self {
            Self::CheckRun {
                conclusion: Some(MetricsHealthConclusion::Unknown(_)),
                ..
            } => "",
            Self::CheckRun { conclusion, .. } => conclusion
                .as_ref()
                .map_or("", MetricsHealthConclusion::api_value),
            Self::CommitStatus {
                state: MetricsCommitStatusState::Success,
            } => "success",
            Self::CommitStatus {
                state: MetricsCommitStatusState::Failure | MetricsCommitStatusState::Error,
            } => "failure",
            Self::CommitStatus { .. } => "",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MetricsCheckObservation {
    pub id: u64,
    pub node_id: Option<String>,
    pub name: Option<String>,
    pub head_sha: String,
    pub result: MetricsCheckResult,
    pub app_id: Option<u64>,
    pub actor: Option<MetricsActor>,
    pub started_at: Option<MetricsTimestamp>,
    pub completed_at: Option<MetricsTimestamp>,
    pub created_at: Option<MetricsTimestamp>,
    pub url: Option<String>,
    /// A repeated context cannot certify two simultaneously-current statuses.
    pub current_context_unique: bool,
    pub reasons: Vec<String>,
}
impl MetricsCheckObservation {
    pub fn health_status(&self) -> &str {
        if self.current_context_unique {
            self.result.health_status()
        } else {
            "unknown"
        }
    }
    pub fn health_conclusion(&self) -> &str {
        if self.current_context_unique {
            self.result.health_conclusion()
        } else {
            ""
        }
    }
}
#[derive(Clone, Debug)]
pub enum MetricsRequiredRules {
    Unavailable { reason: String },
}
#[derive(Clone, Debug)]
pub struct ExactPrCheckHealth {
    pub identity: HostIdentity,
    pub parent: MetricsPrObservation,
    pub check_runs: Vec<MetricsCheckObservation>,
    pub commit_statuses: Vec<MetricsCheckObservation>,
    pub check_run_coverage: MetricsReadCoverage,
    pub commit_status_coverage: MetricsReadCoverage,
    pub required_rules: MetricsRequiredRules,
    pub source_limitations: Vec<String>,
    pub observed_from: u64,
    pub observed_through: u64,
    pub requests: usize,
    pub raw_rows: usize,
    pub collection_pages: usize,
    pub aggregate_response_bytes: usize,
    pub successful_diagnostic_bytes: usize,
    /// Conservative shared stdout+stderr caps for failed commands, not actual bytes.
    pub failed_reserved_bytes: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MetricsRunObservation {
    pub repository: MetricsRepository,
    pub id: u64,
    pub node_id: String,
    pub workflow_id: u64,
    pub attempt: u64,
    pub head_sha: String,
    pub name: Option<String>,
    pub title: Option<String>,
    pub event: Option<String>,
    pub head_branch: Option<String>,
    pub status: MetricsHealthStatus,
    pub conclusion: Option<MetricsHealthConclusion>,
    pub actor: Option<MetricsActor>,
    pub triggering_actor: Option<MetricsActor>,
    pub created_at: Option<MetricsTimestamp>,
    pub started_at: Option<MetricsTimestamp>,
    /// Freshness metadata only: never exposed as completion or duration.
    pub observed_updated_at: Option<MetricsTimestamp>,
    pub url: Option<String>,
    pub reasons: Vec<String>,
}
#[derive(Clone, Debug)]
pub struct RepositoryRunHealth {
    pub identity: HostIdentity,
    pub repository: MetricsRepository,
    /// At most one highest OBSERVED attempt per genuine run ID; no historical claim.
    pub runs: Vec<MetricsRunObservation>,
    pub coverage: MetricsReadCoverage,
    pub source_limitations: Vec<String>,
    pub observed_from: u64,
    pub observed_through: u64,
    pub requests: usize,
    pub raw_rows: usize,
    pub collection_pages: usize,
    pub aggregate_response_bytes: usize,
    pub successful_diagnostic_bytes: usize,
    pub failed_reserved_bytes: usize,
}
