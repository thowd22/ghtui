//! Genuine server-PENDING review sources. This is deliberately independent of
//! the submitted-thread comment producer. No deserialized record grants writes.
use super::*;
use crate::providers::runner::ScopedReadBudgetV1;

#[path = "pending_comment_v3/read.rs"]
mod read;
#[path = "pending_comment_v3/write.rs"]
mod write;

/// A current provider-issued pending review observation, not a write permit.
/// Same producer/token/deadline must survive into the one Confirm operation.
pub struct PendingReviewOriginalV3 {
    snapshot: Arc<ReviewSnapshotV2>,
    producer: Arc<ScopedReadBudgetV1>,
    cancellation: CancellationToken,
    deadline: Instant,
}
impl std::fmt::Debug for PendingReviewOriginalV3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PendingReviewOriginalV3(<actual own PENDING source; no approval>)")
    }
}
impl PendingReviewOriginalV3 {
    pub fn snapshot(&self) -> &Arc<ReviewSnapshotV2> {
        &self.snapshot
    }
    pub fn parent(&self) -> &Arc<ReviewParentV2> {
        self.snapshot.review().parent()
    }
    pub fn principal(&self) -> u64 {
        self.parent().observed_by()
    }
    pub fn deadline(&self) -> Instant {
        self.deadline
    }
    pub fn has_current_commit(&self) -> bool {
        self.snapshot.review().commit() == self.parent().head()
    }
    fn original_producer(
        &self,
        measured: &GithubProvider,
        cancel: &CancellationToken,
        requested: Instant,
    ) -> GithubResult<Instant> {
        let deadline = requested.min(self.deadline);
        phase(cancel, deadline)?;
        if !self.cancellation.same_instance(cancel)
            || measured
                .scoped_read_budget
                .as_ref()
                .is_none_or(|current| !Arc::ptr_eq(current, &self.producer))
        {
            return Err(refusal(
                GithubErrorKind::InvalidTarget,
                "Pending review must retain its actual measured producer and original token",
            ));
        }
        Ok(deadline)
    }
}

/// Actual membership plus current capabilities from the SAME original producer.
/// No history/draft/current-login guess can construct or select another member.
pub struct PendingCommentOriginalV3 {
    review: Arc<PendingReviewOriginalV3>,
    index: usize,
    can_update: Option<bool>,
    can_delete: Option<bool>,
    updated_at: Arc<str>,
}
impl std::fmt::Debug for PendingCommentOriginalV3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PendingCommentOriginalV3(<actual pending member/capabilities>)")
    }
}
impl PendingCommentOriginalV3 {
    pub fn review(&self) -> &Arc<PendingReviewOriginalV3> {
        &self.review
    }
    pub fn comment(&self) -> &ReviewCommentRecordV2 {
        &self.review.snapshot.comments()[self.index]
    }
    pub fn viewer_can_update(&self) -> Option<bool> {
        self.can_update
    }
    pub fn viewer_can_delete(&self) -> Option<bool> {
        self.can_delete
    }
    pub fn updated_at(&self) -> &str {
        &self.updated_at
    }
    pub fn same_observation(&self, other: &Self) -> bool {
        self.review
            .snapshot
            .same_observation(&other.review.snapshot)
            && self.index == other.index
            && self.can_update == other.can_update
            && self.can_delete == other.can_delete
            && self.updated_at == other.updated_at
    }
}

/// A requested effect only; the source-owned planner closes SQL and principal
/// before an ordinary prepaid write child. Add never submits the pending review.
#[derive(Clone, Copy)]
pub enum PendingCommentMutationV3<'a> {
    Add {
        review: &'a PendingReviewOriginalV3,
        comment: &'a InlineComment,
    },
    Edit {
        comment: &'a PendingCommentOriginalV3,
        body: &'a str,
    },
    Delete {
        comment: &'a PendingCommentOriginalV3,
    },
}

pub struct PendingCommentReceiptV3 {
    snapshot: Option<ReviewSnapshotV2>,
    original_review_id: u64,
    review_and_comment_absent: bool,
    comment_id: u64,
    comment_node: Arc<str>,
    deleted: bool,
}
impl std::fmt::Debug for PendingCommentReceiptV3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PendingCommentReceiptV3(<typed mutation plus exact readback>)")
    }
}
impl PendingCommentReceiptV3 {
    pub fn snapshot(&self) -> Option<&ReviewSnapshotV2> {
        self.snapshot.as_ref()
    }
    pub fn original_review_id(&self) -> u64 {
        self.original_review_id
    }
    /// Observed current absence only, NOT an explicit review-discard action or
    /// a claim that deleting the comment caused the review to disappear.
    pub fn review_and_comment_absent(&self) -> bool {
        self.review_and_comment_absent
    }
    pub fn comment_id(&self) -> u64 {
        self.comment_id
    }
    pub fn comment_node(&self) -> &str {
        &self.comment_node
    }
    pub fn deleted(&self) -> bool {
        self.deleted
    }
}

pub(super) fn positive_bigint(value: &Value) -> Option<u64> {
    positive(value).or_else(|| {
        value
            .as_str()
            .filter(|text| {
                !text.is_empty()
                    && text.len() <= 19
                    && !text.starts_with('0')
                    && text.bytes().all(|byte| byte.is_ascii_digit())
            })
            .and_then(|text| text.parse::<u64>().ok())
            .filter(|id| *id > 0 && *id <= i64::MAX as u64)
    })
}
pub(super) fn error_free(value: &Value) -> bool {
    match value.get("errors") {
        None | Some(Value::Null) => true,
        Some(Value::Array(errors)) => errors.is_empty(),
        _ => false,
    }
}
fn optional_capability(value: &Value) -> GithubResult<Option<bool>> {
    match value {
        Value::Null => Ok(None),
        Value::Bool(value) => Ok(Some(*value)),
        _ => Err(refusal(
            GithubErrorKind::Json,
            "Pending comment capability has invalid protocol type",
        )),
    }
}
