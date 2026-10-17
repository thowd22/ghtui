//! Conservative transport reservations, NOT source or mutation authority.
//! One original session retains its parent/final proof and ONE measured pool;
//! bounded ordinary post-close children spend these already-reserved slots.
use super::*;
#[path = "postwrite_capture_v2/workflow_git_v9.rs"]
mod workflow_git_v9;
pub(crate) use workflow_git_v9::WorkflowCleanupSealV9;
pub(crate) use workflow_git_v9::{WorkflowEditPhaseV9, WorkflowGitRoleV9, WorkflowPostCaptureV9};
#[path = "postwrite_capture_v2/file_comment_created_v75.rs"]
mod file_comment_created_v75;
use crate::providers::runner::ScopedReadBudgetV1;
#[path = "postwrite_capture_v2/actions_dispatch_ack_v7.rs"]
mod actions_dispatch_ack_v7;
#[path = "postwrite_capture_v2/actions_run_ack_v5.rs"]
mod actions_run_ack_v5;
#[path = "postwrite_capture_v2/actions_workflow_ack_v6.rs"]
mod actions_workflow_ack_v6;
#[path = "postwrite_capture_v2/conversation_detail_list_v19.rs"]
mod conversation_detail_list_v19;
#[path = "postwrite_capture_v2/conversation_http_v17.rs"]
mod conversation_http_v17;
#[path = "postwrite_capture_v2/empty_http_ack_v5.rs"]
mod empty_http_ack_v5;
#[path = "postwrite_capture_v2/issue_link_v87.rs"]
mod issue_link_v87;
#[path = "postwrite_capture_v2/members_metadata_v62.rs"]
mod members_metadata_v62;
#[path = "postwrite_capture_v2/metadata_context_v51.rs"]
mod metadata_context_v51;
#[path = "postwrite_capture_v2/metadata_members_request_v48.rs"]
mod metadata_members_request_v48;
#[path = "postwrite_capture_v2/state_request_v68.rs"]
mod state_request_v68;
pub(in crate::providers::gh) use members_metadata_v62::{
    MembersMetadataClosingInputV62, MembersMetadataContextInputV62, MembersMetadataRoleV62,
};
#[path = "postwrite_capture_v2/notifications_http_v63.rs"]
pub(in crate::providers::gh::repository_catalogue) mod notifications_http_v63;
#[path = "postwrite_capture_v2/pinned_http_response_v5.rs"]
mod pinned_http_response_v5;
#[path = "postwrite_capture_v2/workflow_branch_http_v9.rs"]
mod workflow_branch_http_v9;
pub(in crate::providers::gh) use workflow_branch_http_v9::{
    workflow_branch_response_v9, workflow_history_response_v9,
};
#[path = "postwrite_capture_v2/review_comment_delete_v2.rs"]
mod review_comment_delete_v2;
#[path = "postwrite_capture_v2/thread_reaction_http_v3.rs"]
mod thread_reaction_http_v3;
use std::sync::{
    atomic::{AtomicU8, Ordering},
    Arc, OnceLock, Weak,
};
pub(crate) use thread_reaction_http_v3::{ThreadReactionHttpAckV3, ThreadReactionHttpRequestV3};

const PENDING: u8 = 0;
const CLOSED: u8 = 1;
const ABORTED: u8 = 2;

#[derive(Clone, Debug)]
pub struct PostWriteCaptureAccountingV2 {
    requests: usize,
    reserved_bytes: usize,
}
impl PostWriteCaptureAccountingV2 {
    pub fn reserved_requests(&self) -> usize {
        self.requests
    }
    pub fn reserved_bytes(&self) -> usize {
        self.reserved_bytes
    }
}
struct LineageV2 {
    state: AtomicU8,
    measured: OnceLock<Weak<ScopedReadBudgetV1>>,
}
pub(super) struct PostWriteSessionReservationV2 {
    metadata: PostWriteCaptureAccountingV2,
    lineage: Arc<LineageV2>,
    deadline: Instant,
}
impl PostWriteSessionReservationV2 {
    pub(super) fn metadata(&self) -> PostWriteCaptureAccountingV2 {
        self.metadata.clone()
    }
    pub(super) fn deadline(&self) -> Instant {
        self.deadline
    }
    pub(super) fn bind(&self, budget: &Arc<ScopedReadBudgetV1>) -> GithubResult<()> {
        self.lineage
            .measured
            .set(Arc::downgrade(budget))
            .map_err(|_| {
                error(
                    GithubErrorKind::InvalidTarget,
                    "Original post-write reservation cannot replace its measured producer",
                )
            })
    }
    pub(super) fn complete(&self) -> GithubResult<()> {
        if self.lineage.measured.get().is_none() {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Post-write reservation requires its one actual measured prewrite pool",
            ));
        }
        self.lineage
            .state
            .compare_exchange(PENDING, CLOSED, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| ())
            .map_err(|_| {
                error(
                    GithubErrorKind::InvalidTarget,
                    "Original post-write reservation lineage already retired",
                )
            })
    }
}
impl Drop for PostWriteSessionReservationV2 {
    fn drop(&mut self) {
        let _ = self.lineage.state.compare_exchange(
            PENDING,
            ABORTED,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }
}

/// Non-Clone and privately constructed. Its fixed slot cursor cannot be reset,
/// but consuming it proves only bounded transport, not access or confirmation.
pub struct PostWriteCaptureAllowanceV2 {
    issuer: GithubProvider,
    cancellation: CancellationToken,
    deadline: Instant,
    caps: Box<[usize]>,
    next: usize,
    lineage: Arc<LineageV2>,
}
impl std::fmt::Debug for PostWriteCaptureAllowanceV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostWriteCaptureAllowanceV2")
            .field("reserved_requests", &self.caps.len())
            .field("consumed_requests", &self.next)
            .finish_non_exhaustive()
    }
}
impl PostWriteCaptureAllowanceV2 {
    pub fn deadline(&self) -> Instant {
        self.deadline
    }
    pub fn reserved_requests(&self) -> usize {
        self.caps.len()
    }
    pub fn reserved_bytes(&self) -> usize {
        self.caps.iter().sum()
    }
    pub fn consumed_requests(&self) -> usize {
        self.next
    }
    fn check_issuer(
        &self,
        provider: &GithubProvider,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        if !self.cancellation.same_instance(cancel)
            || cancel.is_cancelled()
            || Instant::now() >= self.deadline
        {
            return Err(error(
                GithubErrorKind::Transport,
                "Original post-write token changed/cancelled or its absolute deadline expired",
            ));
        }
        if !self.issuer.runner.same_instance(&provider.runner)
            || self.issuer.host != provider.host
            || self.issuer.program != provider.program
            || self.issuer.env != provider.env
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Post-write reservation belongs to another actual provider/runner",
            ));
        }
        Ok(())
    }
    /// Exact same measured allocation, not merely equal limits/deadlines.
    pub(crate) fn validate_prewrite(
        &self,
        provider: &GithubProvider,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        self.check_issuer(provider, cancel)?;
        let actual = self
            .lineage
            .measured
            .get()
            .and_then(Weak::upgrade)
            .ok_or_else(|| {
                error(
                    GithubErrorKind::InvalidTarget,
                    "Original measured prewrite producer is absent",
                )
            })?;
        if self.lineage.state.load(Ordering::Acquire) != PENDING
            || provider
                .scoped_read_budget
                .as_ref()
                .is_none_or(|budget| !Arc::ptr_eq(budget, &actual))
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Post-write reservation cannot adopt a sibling or sealed read window",
            ));
        }
        actual.check_clock(cancel).map_err(Into::into)
    }
    /// Called only by typed publication/receipt implementation. The slot is
    /// consumed BEFORE the ordinary child; failure is never a retry permit.
    pub(crate) fn api(
        &mut self,
        provider: &GithubProvider,
        endpoint: &str,
        method: &str,
        input: Option<Value>,
        write: Option<String>,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        let bounded = self.spend_ordinary_slot_v2(provider, cancel)?;
        let result = bounded.api(endpoint, method, input, write, false, &self.cancellation);
        self.check_issuer(provider, cancel)?;
        result
    }
    // A transport reservation only. Both the existing JSON path and the one
    // closed HTTP-204 delete path spend this SAME cursor before one child.
    fn spend_ordinary_slot_v2(
        &mut self,
        provider: &GithubProvider,
        cancel: &CancellationToken,
    ) -> GithubResult<GithubProvider> {
        self.check_issuer(provider, cancel)?;
        if self.lineage.state.load(Ordering::Acquire) != CLOSED
            || provider.scoped_read_budget.is_some()
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Original source close must finish before ordinary post-write transport",
            ));
        }
        let cap = *self.caps.get(self.next).ok_or_else(|| {
            error(
                GithubErrorKind::Transport,
                "Fixed original post-write child allowance exhausted; no extra child or retry",
            )
        })?;
        self.next += 1;
        // Existing narrower provider windows and timeouts remain narrower.
        let (deadline, cap) = provider
            .scoped_read_window
            .map_or((self.deadline, cap), |(original, original_cap)| {
                (original.min(self.deadline), original_cap.min(cap))
            });
        provider.clone().with_scoped_read_window(deadline, cap)
    }
}

impl OriginalRepositoryAccessCoreV13<'_> {
    /// Reserve exactly once BEFORE measured allocation. All bytes are a fixed
    /// conservative combined stdout/stderr capture charge, never measured wire
    /// bytes or mutation permission. Existing generic reservations stay separate.
    pub(super) fn reserve_postwrite_capture_v2(
        &mut self,
        capture_caps: &[usize],
        remaining_repository_reads: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<PostWriteCaptureAllowanceV2> {
        self.budget.check(cancel)?;
        if !self.original_cancellation.same_instance(cancel)
            || self.provider.scoped_read_budget.is_some()
            || self.measured_external.is_some()
            || self.external_requests_reserved != 0
            || self.postwrite_reservation_v2.is_some()
            || !(1..=32).contains(&capture_caps.len())
            || capture_caps
                .iter()
                .any(|cap| !(1..=1024 * 1024).contains(cap))
            || self
                .repository_reads
                .checked_add(remaining_repository_reads)
                .is_none_or(|reads| reads > self.limits.max_repository_reads)
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "One bounded original post-write reservation must precede measured allocation",
            ));
        }
        let deadline = self
            .provider
            .scoped_read_window
            .map_or(self.deadline(), |(queued, _)| self.deadline().min(queued));
        if Instant::now() >= deadline {
            return Err(error(
                GithubErrorKind::Transport,
                "Original queued post-write deadline expired before reservation",
            ));
        }
        let effective_caps = capture_caps
            .iter()
            .map(|cap| {
                self.provider
                    .scoped_read_window
                    .map_or(*cap, |(_, original_cap)| (*cap).min(original_cap))
            })
            .collect::<Vec<_>>();
        let reserved_bytes = effective_caps
            .iter()
            .try_fold(0usize, |bytes, cap| bytes.checked_add(*cap))
            .ok_or_else(|| {
                error(
                    GithubErrorKind::InvalidTarget,
                    "Post-write capture reservation overflow",
                )
            })?;
        let required = remaining_repository_reads
            .checked_mul(self.limits.transport.max_document_bytes)
            .and_then(|bytes| bytes.checked_add(FINAL_PRINCIPAL_CAP))
            .and_then(|bytes| bytes.checked_add(reserved_bytes))
            .ok_or_else(|| {
                error(
                    GithubErrorKind::InvalidTarget,
                    "Original parent/final/post-write capture reservation overflow",
                )
            })?;
        if required >= self.remaining_bytes() {
            return Err(error(GithubErrorKind::Transport,"Original whole capture cannot preserve parent/final proof and post-write receipt allowance"));
        }
        let lineage = Arc::new(LineageV2 {
            state: AtomicU8::new(PENDING),
            measured: OnceLock::new(),
        });
        self.postwrite_reservation_v2 = Some(PostWriteSessionReservationV2 {
            metadata: PostWriteCaptureAccountingV2 {
                requests: effective_caps.len(),
                reserved_bytes,
            },
            lineage: lineage.clone(),
            deadline,
        });
        Ok(PostWriteCaptureAllowanceV2 {
            issuer: self.provider.clone(),
            cancellation: self.original_cancellation.clone(),
            deadline,
            caps: effective_caps.into(),
            next: 0,
            lineage,
        })
    }
    pub(super) fn postwrite_reserved_bytes_v2(&self) -> usize {
        self.postwrite_reservation_v2
            .as_ref()
            .map_or(0, |reservation| reservation.metadata.reserved_bytes)
    }
}

impl RepositoryCatalogueAccessSession<'_> {
    /// Same fixed original cursor/reservation, never a second allocation.
    pub fn reserve_postwrite_capture_v2(
        &mut self,
        capture_caps: &[usize],
        remaining_repository_reads: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<PostWriteCaptureAllowanceV2> {
        self.core
            .reserve_postwrite_capture_v2(capture_caps, remaining_repository_reads, cancel)
    }
}
