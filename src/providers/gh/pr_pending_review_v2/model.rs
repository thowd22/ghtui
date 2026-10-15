use super::*;

#[derive(Clone, Debug)]
pub struct ReviewReadLimitsV2 {
    pub max_pages: usize,
    pub max_reviews: usize,
    pub max_comments: usize,
    pub max_document_bytes: usize,
}
impl Default for ReviewReadLimitsV2 {
    fn default() -> Self {
        Self {
            max_pages: 2,
            max_reviews: 200,
            max_comments: 200,
            max_document_bytes: 1024 * 1024,
        }
    }
}
impl ReviewReadLimitsV2 {
    pub fn validate(&self) -> GithubResult<()> {
        if !(1..=30).contains(&self.max_pages)
            || !(1..=3000).contains(&self.max_reviews)
            || !(1..=3000).contains(&self.max_comments)
            || !(1..=1024 * 1024).contains(&self.max_document_bytes)
        {
            return Err(refusal(
                GithubErrorKind::InvalidTarget,
                "Review read bounds must be explicit and finite",
            ));
        }
        Ok(())
    }
}

/// Retains only actual scalar identity from a genuine raw parent. No restored
/// metadata or actor login can construct this parent or grant permission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewParentV2 {
    host: Arc<str>,
    repository: Arc<str>,
    repository_id: u64,
    repository_node: Arc<str>,
    pr_id: u64,
    pr_node: Arc<str>,
    number: u64,
    head: Arc<str>,
    base: Arc<str>,
    author_account_id: Option<u64>,
    observed_by: u64,
}
impl ReviewParentV2 {
    pub(super) fn capture(
        identity: &HostIdentity,
        parent: &PrResourceParentV1,
    ) -> GithubResult<Arc<Self>> {
        let uid = numeric_account(identity, &parent.summary().pr.host)?;
        Ok(Arc::new(Self {
            host: Arc::from(identity.host.as_str()),
            repository: Arc::from(parent.repository()),
            repository_id: parent.repository_id(),
            repository_node: Arc::from(parent.repository_node()),
            pr_id: parent.database_id(),
            pr_node: Arc::from(parent.node_id()),
            number: parent.number(),
            head: Arc::from(parent.head_sha()),
            base: Arc::from(parent.base_tip_sha()),
            author_account_id: parent.author_account_id(),
            observed_by: uid,
        }))
    }
    pub fn repository(&self) -> &str {
        &self.repository
    }
    pub fn host(&self) -> &str {
        &self.host
    }
    pub fn repository_id(&self) -> u64 {
        self.repository_id
    }
    pub fn repository_node(&self) -> &str {
        &self.repository_node
    }
    pub fn pr_id(&self) -> u64 {
        self.pr_id
    }
    pub fn pr_node(&self) -> &str {
        &self.pr_node
    }
    pub fn number(&self) -> u64 {
        self.number
    }
    pub fn head(&self) -> &str {
        &self.head
    }
    pub fn base(&self) -> &str {
        &self.base
    }
    pub fn observed_by(&self) -> u64 {
        self.observed_by
    }
    pub fn author_account_id(&self) -> Option<u64> {
        self.author_account_id
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObservedReviewStateV2 {
    Pending,
    Approved,
    ChangesRequested,
    Commented,
    Dismissed,
    Unknown(Arc<str>),
}
impl ObservedReviewStateV2 {
    pub(super) fn parse(value: Arc<str>) -> Self {
        match value.as_ref() {
            "PENDING" => Self::Pending,
            "APPROVED" => Self::Approved,
            "CHANGES_REQUESTED" => Self::ChangesRequested,
            "COMMENTED" => Self::Commented,
            "DISMISSED" => Self::Dismissed,
            _ => Self::Unknown(value),
        }
    }
    pub fn label(&self) -> &str {
        match self {
            Self::Pending => "PENDING — not submitted",
            Self::Approved => "APPROVED",
            Self::ChangesRequested => "CHANGES REQUESTED",
            Self::Commented => "COMMENTED",
            Self::Dismissed => "DISMISSED",
            Self::Unknown(_) => "UNKNOWN observed review state",
        }
    }
}

/// Provider-produced read observation, never write permission. No Deserialize.
#[derive(Clone, Debug)]
pub struct ReviewRecordV2 {
    pub(super) parent: Arc<ReviewParentV2>,
    pub(super) id: u64,
    pub(super) node: Arc<str>,
    pub(super) author: Actor,
    pub(super) author_id: Option<u64>,
    pub(super) body: Arc<str>,
    pub(super) state: ObservedReviewStateV2,
    pub(super) commit: Arc<str>,
    pub(super) submitted_at: Option<Arc<str>>,
}
impl ReviewRecordV2 {
    pub fn parent(&self) -> &Arc<ReviewParentV2> {
        &self.parent
    }
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn node(&self) -> &str {
        &self.node
    }
    pub fn author(&self) -> &Actor {
        &self.author
    }
    pub fn author_account_id(&self) -> Option<u64> {
        self.author_id
    }
    pub fn body(&self) -> &str {
        &self.body
    }
    pub fn state(&self) -> &ObservedReviewStateV2 {
        &self.state
    }
    pub fn commit(&self) -> &str {
        &self.commit
    }
    pub fn submitted_at(&self) -> Option<&str> {
        self.submitted_at.as_deref()
    }
    /// An actual observed timestamp only, NOT protected-branch permission or
    /// inferred chronology. Unknown/malformed timestamps stay readable but
    /// cannot enable a submitted-review-only request.
    pub fn has_observed_submission_v2(&self) -> bool {
        self.submitted_at()
            .and_then(crate::github_workspace::timestamp_seconds)
            .is_some()
    }
    pub fn is_observed_own_pending(&self) -> bool {
        self.author_id == Some(self.parent.observed_by)
            && self.state == ObservedReviewStateV2::Pending
    }
    pub(super) fn same_observation(&self, other: &Self) -> bool {
        self.parent == other.parent
            && self.id == other.id
            && self.node == other.node
            && self.author_id == other.author_id
            && self.body == other.body
            && self.state == other.state
            && self.commit == other.commit
            && self.submitted_at == other.submitted_at
    }
}

#[derive(Clone, Debug)]
pub struct ReviewCommentRecordV2 {
    pub(super) id: u64,
    pub(super) node: Arc<str>,
    pub(super) review_id: u64,
    pub(super) author: Actor,
    pub(super) author_id: Option<u64>,
    pub(super) body: Arc<str>,
    pub(super) path: Arc<str>,
    pub(super) commit: Arc<str>,
    pub(super) side: Option<DiffSide>,
    pub(super) line: Option<u64>,
    pub(super) start_line: Option<u64>,
    pub(super) start_side: Option<DiffSide>,
    pub(super) in_reply_to: Option<u64>,
}
impl ReviewCommentRecordV2 {
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn node(&self) -> &str {
        &self.node
    }
    pub fn review_id(&self) -> u64 {
        self.review_id
    }
    pub fn author(&self) -> &Actor {
        &self.author
    }
    pub fn author_account_id(&self) -> Option<u64> {
        self.author_id
    }
    pub fn body(&self) -> &str {
        &self.body
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn commit(&self) -> &str {
        &self.commit
    }
    pub fn side(&self) -> Option<DiffSide> {
        self.side
    }
    pub fn line(&self) -> Option<u64> {
        self.line
    }
    pub fn start_line(&self) -> Option<u64> {
        self.start_line
    }
    pub fn start_side(&self) -> Option<DiffSide> {
        self.start_side
    }
    pub fn in_reply_to(&self) -> Option<u64> {
        self.in_reply_to
    }
    pub(super) fn same_observation(&self, other: &Self) -> bool {
        self.id == other.id
            && self.node == other.node
            && self.review_id == other.review_id
            && self.author_id == other.author_id
            && self.body == other.body
            && self.path == other.path
            && self.commit == other.commit
            && self.side == other.side
            && self.line == other.line
            && self.start_line == other.start_line
            && self.start_side == other.start_side
            && self.in_reply_to == other.in_reply_to
    }
}

#[derive(Clone, Debug)]
pub struct ReviewCatalogueV2 {
    pub(super) parent: Arc<ReviewParentV2>,
    pub(super) rows: Arc<[Arc<ReviewRecordV2>]>,
    pub(super) complete: bool,
    pub(super) pages: usize,
}
impl ReviewCatalogueV2 {
    pub fn parent(&self) -> &Arc<ReviewParentV2> {
        &self.parent
    }
    pub fn rows(&self) -> &[Arc<ReviewRecordV2>] {
        &self.rows
    }
    pub fn complete(&self) -> bool {
        self.complete
    }
    pub fn pages(&self) -> usize {
        self.pages
    }
}

#[derive(Clone, Debug)]
pub struct ReviewSnapshotV2 {
    pub(super) review: Arc<ReviewRecordV2>,
    pub(super) comments: Arc<[ReviewCommentRecordV2]>,
    pub(super) complete: bool,
}
impl ReviewSnapshotV2 {
    pub fn review(&self) -> &Arc<ReviewRecordV2> {
        &self.review
    }
    pub fn comments(&self) -> &[ReviewCommentRecordV2] {
        &self.comments
    }
    pub fn complete(&self) -> bool {
        self.complete
    }
    pub(crate) fn same_observation(&self, other: &Self) -> bool {
        self.complete
            && other.complete
            && self.review.same_observation(&other.review)
            && self.comments.len() == other.comments.len()
            && self
                .comments
                .iter()
                .zip(other.comments.iter())
                .all(|(a, b)| a.same_observation(b))
    }
}

#[derive(Debug)]
pub enum ReviewPublicationFailureV2 {
    NotStarted(GithubError),
    WriteMayHaveStarted(GithubError),
}

/// Only real mutation plus independently checked readback issues a receipt.
/// The planner returns safe scalar IDs, never this body capability after fence.
#[derive(Clone, Debug)]
pub struct ReviewReceiptV2 {
    pub(super) snapshot: Option<ReviewSnapshotV2>,
    pub(super) review_id: u64,
    pub(super) review_node: Arc<str>,
    pub(super) discarded: bool,
}
impl ReviewReceiptV2 {
    pub fn snapshot(&self) -> Option<&ReviewSnapshotV2> {
        self.snapshot.as_ref()
    }
    pub fn review_id(&self) -> u64 {
        self.review_id
    }
    pub fn review_node(&self) -> &str {
        &self.review_node
    }
    pub fn discarded(&self) -> bool {
        self.discarded
    }
}
