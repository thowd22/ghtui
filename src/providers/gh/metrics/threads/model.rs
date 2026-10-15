//! Source-only typed thread provenance; no original-tree verification is minted.
use super::*;

#[derive(Clone, Debug)]
pub struct MetricsThreadLimits {
    pub max_thread_pages: usize,
    pub thread_page_size: usize,
    pub max_threads: usize,
    pub max_comment_pages_per_thread: usize,
    pub comment_page_size: usize,
    pub max_comments: usize,
    pub max_graphql_pages: usize,
    pub max_document_bytes: usize,
    pub max_response_bytes: usize,
    pub max_duration: Duration,
}
impl Default for MetricsThreadLimits {
    fn default() -> Self {
        Self {
            max_thread_pages: 20,
            thread_page_size: 25,
            max_threads: 1_000,
            max_comment_pages_per_thread: 10,
            comment_page_size: 10,
            max_comments: 5_000,
            max_graphql_pages: 100,
            max_document_bytes: 256 * 1024,
            max_response_bytes: 16 * 1024 * 1024,
            max_duration: Duration::from_secs(30),
        }
    }
}
impl MetricsThreadLimits {
    pub(super) fn validate(&self) -> GithubResult<()> {
        if !(1..=100).contains(&self.max_thread_pages)
            || !(1..=100).contains(&self.thread_page_size)
            || !(1..=10_000).contains(&self.max_threads)
            || self.thread_page_size > self.max_threads
            || !(1..=100).contains(&self.max_comment_pages_per_thread)
            || !(1..=100).contains(&self.comment_page_size)
            || !(1..=50_000).contains(&self.max_comments)
            || self.comment_page_size > self.max_comments
            || !(1..=1_000).contains(&self.max_graphql_pages)
            || !(1..=4 * 1024 * 1024).contains(&self.max_document_bytes)
            || !(1..=32 * 1024 * 1024).contains(&self.max_response_bytes)
            || self.max_duration.is_zero()
            || self.max_duration > Duration::from_secs(300)
        {
            return Err(error(GithubErrorKind::InvalidTarget, "Thread evidence needs bounded positive outer/nested pages, raw rows, documents, aggregate bytes and duration"));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum MetricsThreadSide {
    Left,
    Right,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MetricsThreadPlacement {
    /// Raw UTF-8 observed placement, not a proven original path.
    pub observed_path: Option<String>,
    pub observed_side: Option<MetricsThreadSide>,
    pub observed_start_side: Option<MetricsThreadSide>,
    pub line: Option<u64>,
    pub start_line: Option<u64>,
    pub original_line: Option<u64>,
    pub original_start_line: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum MetricsThreadReplyTo {
    /// Only a positively present JSON null establishes the original comment.
    Original,
    Reply(String),
    Unknown,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MetricsThreadComment {
    pub node_id: String,
    /// Actual GraphQL fullDatabaseId only, never a decoded/parsed opaque node.
    pub database_id: Option<u64>,
    pub reply_to: MetricsThreadReplyTo,
    pub author: Option<MetricsActor>,
    pub body: Option<String>,
    pub created_at: Option<MetricsTimestamp>,
    pub updated_at: Option<MetricsTimestamp>,
    pub observed_path: Option<String>,
    pub line: Option<u64>,
    pub start_line: Option<u64>,
    pub original_line: Option<u64>,
    pub original_start_line: Option<u64>,
    pub commit_sha: Option<String>,
    pub original_commit_sha: Option<String>,
    pub reasons: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MetricsOriginalCommentCandidate {
    pub comment_node_id: String,
    pub original_line: Option<u64>,
    pub original_start_line: Option<u64>,
    pub original_commit_sha: Option<String>,
    /// Candidate only. GitHub does not expose originalPath/originalDiffSide.
    pub observed_path: Option<String>,
}
#[derive(Clone, Debug)]
pub struct MetricsReviewThreadObservation {
    pub parent_node_id: String,
    pub node_id: String,
    /// Actual schema enum only. Nullable/omitted/unknown is never inferred
    /// from line coordinates (file-level and outdated threads may have none).
    pub subject_type: Option<MetricsThreadSubjectV1>,
    pub resolved: Option<bool>,
    pub outdated: Option<bool>,
    pub viewer_can_reply: Option<bool>,
    pub viewer_can_resolve: Option<bool>,
    pub viewer_can_unresolve: Option<bool>,
    pub resolved_by: Option<MetricsActor>,
    pub placement: MetricsThreadPlacement,
    pub comments: Vec<MetricsThreadComment>,
    pub comment_coverage: MetricsReadCoverage,
    pub original_comment: Option<MetricsOriginalCommentCandidate>,
    pub state_reasons: Vec<String>,
    pub provenance_reasons: Vec<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetricsThreadSubjectV1 {
    File,
    Line,
}
#[derive(Clone, Debug)]
pub struct MetricsThreadProvenanceCoverage {
    pub comment_enumeration_complete: bool,
    pub actors_complete: bool,
    pub content_complete: bool,
    pub original_candidates_complete: bool,
    /// No provider observation is an immutable old-tree verification.
    pub original_tree_verification_available: bool,
    pub reasons: Vec<String>,
}
#[derive(Clone, Debug)]
pub struct MetricsReviewThreadsSnapshot {
    pub identity: HostIdentity,
    pub parent: MetricsPrObservation,
    pub threads: Vec<MetricsReviewThreadObservation>,
    pub state_coverage: MetricsReadCoverage,
    pub provenance_coverage: MetricsThreadProvenanceCoverage,
    pub observed_from: u64,
    pub observed_through: u64,
    pub requests: usize,
    pub graphql_pages: usize,
    pub raw_comments: usize,
    pub aggregate_response_bytes: usize,
    pub successful_diagnostic_bytes: usize,
    /// Conservative failed shared stdout+stderr reservations, never actual bytes.
    pub failed_reserved_bytes: usize,
}
