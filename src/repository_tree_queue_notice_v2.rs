//! Exact current-request rejection/retry, not a new queue, clock or source.
use crate::{
    repository_tree_jump_v1::{OriginalTreeJumpV1, OriginalTreeRequestV1},
    repository_tree_worker_v1::{OriginalTreeWorkV1, OriginalTreeWorkerOwnerV1},
    workspace::Workspace,
};
use std::sync::Arc;

/// Only an actual current Query rejection can create this transient marker.
/// It is not serializeable authorization or a retained operation to resend.
pub struct OriginalTreeQueueFailureV2 {
    request: Arc<OriginalTreeRequestV1>,
    message: String,
}
impl std::fmt::Debug for OriginalTreeQueueFailureV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OriginalTreeQueueFailureV2(<exact original pending request>)")
    }
}
impl PartialEq for OriginalTreeQueueFailureV2 {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.request, &other.request)
    }
}
impl Eq for OriginalTreeQueueFailureV2 {}
impl OriginalTreeQueueFailureV2 {
    /// BEFORE owner construction Err / Full / Disconnected / an owned Query
    /// worker failure. Never clear a newer pending query or paint foreign data.
    pub fn reject(
        workspace: &Workspace,
        jump: &mut OriginalTreeJumpV1,
        work: &Arc<OriginalTreeWorkV1>,
        captured_owner: Option<&Arc<OriginalTreeWorkerOwnerV1>>,
        reason: &str,
    ) -> Option<Arc<Self>> {
        let OriginalTreeWorkV1::Query(request) = work.as_ref() else {
            return None;
        };
        if !work.owns(workspace, Some(jump))
            || !jump.accepts_request(request)
            || captured_owner.is_some_and(|owner| {
                !Arc::ptr_eq(owner.work(), work) || !owner.can_admit(workspace, Some(jump), false)
            })
        {
            return None;
        }
        let message = if reason.len() > 4096 {
            "Original read could not start; bounded diagnostic omitted. Close or Retry explicitly."
                .into()
        } else {
            format!(
                "{} • Retry is explicit; no original read resent",
                crate::providers::runner::sanitize_terminal(reason)
            )
        };
        if !jump.fail(request, &message) {
            return None;
        }
        Some(Arc::new(Self {
            request: request.clone(),
            message,
        }))
    }
    pub fn message(&self) -> &str {
        &self.message
    }
    /// Display identity of this exact rejected navigation request, not an
    /// operation receipt, retry authorization or a claim that capacity is free.
    pub fn request_epoch_v2(&self) -> u64 {
        self.request.epoch()
    }
    pub fn is_current(&self, workspace: &Workspace, jump: &OriginalTreeJumpV1) -> bool {
        jump.owns(workspace) && jump.failed_request_matches_v2(&self.request)
    }
    /// Pure navigation intent: Main MUST allocate its normal new Background
    /// token/OriginalLocalNavigationOwner BEFORE queueing the returned request.
    /// The failed owner, operation, deadline and receipt are not stored here.
    pub fn retry(
        self: &Arc<Self>,
        workspace: &Workspace,
        jump: &mut OriginalTreeJumpV1,
    ) -> Result<Arc<OriginalTreeWorkV1>, String> {
        if !self.is_current(workspace, jump) {
            return Err(
                "Original failure/request changed; no retry or new target was inferred".into(),
            );
        }
        Ok(Arc::new(OriginalTreeWorkV1::Query(jump.request()?)))
    }
}

/// Jump/navigation prequeue error has no Query pending marker to clear. Paint
/// only if its original work still owns the exact current frame/projection.
pub fn reject_navigation_v2(
    workspace: &Workspace,
    jump: Option<&mut OriginalTreeJumpV1>,
    work: &Arc<OriginalTreeWorkV1>,
    captured_owner: Option<&Arc<OriginalTreeWorkerOwnerV1>>,
    reason: &str,
) -> Option<String> {
    if matches!(work.as_ref(), OriginalTreeWorkV1::Query(_))
        || !work.owns(workspace, jump.as_deref())
        || captured_owner.is_some_and(|owner| {
            !Arc::ptr_eq(owner.work(), work) || !owner.can_admit(workspace, jump.as_deref(), false)
        })
    {
        return None;
    }
    let reason = if reason.len() > 4096 {
        "Original navigation queue unavailable; diagnostic omitted".into()
    } else {
        crate::providers::runner::sanitize_terminal(reason)
    };
    if let Some(jump) = jump {
        jump.status = reason.clone();
    }
    Some(reason)
}

#[cfg(test)]
#[path = "repository_tree_queue_notice_v2_tests.rs"]
mod tests;
