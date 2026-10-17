//! Original-session binding for measured external read pools.
//! This is allocation/accounting only: every original access/SQL/principal
//! observation remains mandatory, and the original Read/ref/deadline is intact.
use super::*;
use crate::providers::runner::{ScopedReadAccountingV1, ScopedReadBudgetV1};
use std::sync::Arc;

/// The ORIGINAL session owns termination. A provider/spec Arc may escape, but
/// an early return/drop cannot let it continue after this read operation ends.
pub(super) struct MeasuredSessionPoolV1 {
    budget: Arc<ScopedReadBudgetV1>,
}
impl std::ops::Deref for MeasuredSessionPoolV1 {
    type Target = ScopedReadBudgetV1;
    fn deref(&self) -> &Self::Target {
        &self.budget
    }
}
impl Drop for MeasuredSessionPoolV1 {
    fn drop(&mut self) {
        self.budget.abort();
    }
}

/// Non-Clone, private construction from one genuine original session. Not an
/// access receipt; only bounds attach to subsequent independent READ specs.
pub struct MeasuredExternalReadWindowV1 {
    budget: Arc<ScopedReadBudgetV1>,
    max_requests: usize,
    capacity_bytes: usize,
}
impl std::fmt::Debug for MeasuredExternalReadWindowV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MeasuredExternalReadWindowV1")
            .field("max_requests", &self.max_requests)
            .field("shared_pool_capacity_bytes", &self.capacity_bytes)
            .finish_non_exhaustive()
    }
}
impl MeasuredExternalReadWindowV1 {
    pub fn deadline(&self) -> Instant {
        self.budget.deadline()
    }
    pub fn max_output_bytes(&self) -> usize {
        self.budget.max_child_bytes()
    }
    pub fn max_requests(&self) -> usize {
        self.max_requests
    }
    pub fn capacity_bytes(&self) -> usize {
        self.capacity_bytes
    }
    /// Worker-side bounded counters only; pending/sealed is NOT permissions or
    /// catalogue traversal completeness. Do not lock/query this during draw.
    pub fn accounting(&self) -> GithubResult<ScopedReadAccountingV1> {
        self.budget.accounting().map_err(Into::into)
    }
}
impl GithubProvider {
    pub fn with_measured_read_window(
        mut self,
        window: MeasuredExternalReadWindowV1,
    ) -> GithubResult<Self> {
        if self.scoped_read_budget.is_some() {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Measured original read budget cannot be replaced or reset on a clone",
            ));
        }
        // Existing narrower provider window/timeouts remain narrower. Neither
        // this attachment nor later with_timeout may extend the original clock.
        let deadline = self
            .scoped_read_window
            .map_or(window.deadline(), |(current, _)| {
                current.min(window.deadline())
            });
        let cap = self
            .scoped_read_window
            .map_or(window.max_output_bytes(), |(_, current)| {
                current.min(window.max_output_bytes())
            });
        self.scoped_read_window = Some((deadline, cap));
        self.scoped_read_budget = Some(window.budget);
        Ok(self)
    }
}
enum MeasuredPoolProfileV2 {
    Ordinary,
    Archive,
    Dispatch,
}
impl OriginalRepositoryAccessCoreV13<'_> {
    pub(super) fn reserve_workflow_edit_measured_v9(
        &mut self,
        original: &Arc<ScopedActionsWorkflowV2>,
        phase: postwrite_capture_v2::WorkflowEditPhaseV9,
        remaining_repository_reads: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<MeasuredExternalReadWindowV1> {
        if self.limits.transport.max_response_bytes > 2 * 1024 * 1024
            || !self
                .verified
                .iter()
                .rev()
                .find(|row| row.original().id == original.repository().id())
                .is_some_and(|row| original.matches_dispatch_pool_v7(self.provider, row))
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Actual original workflow and bounded source required before workflow-edit pool",
            ));
        }
        let cap = self
            .provider
            .scoped_read_window
            .map_or(256 * 1024, |(_, cap)| cap.min(256 * 1024));
        // SAME pool construction/accounting. `true` retains the original
        // issuer cap/deadline, not Dispatch endpoints or write permission.
        self.reserve_measured_pool_v2(
            phase.pre_reads(),
            cap,
            1024 * 1024,
            remaining_repository_reads,
            cancel,
            MeasuredPoolProfileV2::Dispatch,
        )
    }
    /// Preserve actual pre-consumed original bytes, planned remaining parent
    /// GET caps and final principal69,632 bytes, then allocate UP TO requested
    /// shared pool once. No requests*1MiB charge or first-member truncation.
    pub(super) fn reserve_measured_external_reads(
        &mut self,
        max_requests: usize,
        max_child_bytes: usize,
        max_pool_bytes: usize,
        remaining_repository_reads: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<MeasuredExternalReadWindowV1> {
        self.reserve_measured_pool_v2(
            max_requests,
            max_child_bytes,
            max_pool_bytes,
            remaining_repository_reads,
            cancel,
            MeasuredPoolProfileV2::Ordinary,
        )
    }
    /// SAME original Core and one pool, only after this actual producing
    /// runner's sealed artifact matches the latest observed original-ID row.
    /// Bounds lineage only; fresh artifact/SQL/principal closure is separate.
    pub(super) fn reserve_actions_archive_v2(
        &mut self,
        target: &Arc<ScopedActionsArtifactV2>,
        remaining_repository_reads: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<MeasuredExternalReadWindowV1> {
        if self.postwrite_reservation_v2.is_some()
            || !self
                .verified
                .iter()
                .rev()
                .find(|row| row.original().id == target.repository().id())
                .is_some_and(|current| target.matches_archive_pool_v2(self.provider, current))
        {
            return Err(error(GithubErrorKind::InvalidTarget,"Actual same-runner original artifact/repository observation required before archive allocation"));
        }
        let cap = self
            .provider
            .scoped_read_window
            .map_or(4 * 1024 * 1024, |(_, cap)| cap.min(4 * 1024 * 1024));
        self.reserve_measured_pool_v2(
            24,
            cap,
            8 * 1024 * 1024,
            remaining_repository_reads,
            cancel,
            MeasuredPoolProfileV2::Archive,
        )
    }
    pub(super) fn reserve_dispatch_reads_v7(
        &mut self,
        target: &Arc<crate::providers::gh::ScopedActionsWorkflowV2>,
        max_requests: usize,
        remaining_repository_reads: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<MeasuredExternalReadWindowV1> {
        if !(1..=28).contains(&max_requests)
            || max_requests
                .checked_add(6 + 3)
                .is_none_or(|children| children > crate::providers::gh::DISPATCH_CHILDREN_V7)
            || self.limits.transport.max_response_bytes > 2 * 1024 * 1024
            || !self
                .verified
                .iter()
                .rev()
                .find(|row| row.original().id == target.repository().id())
                .is_some_and(|current| target.matches_dispatch_pool_v7(self.provider, current))
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Actual original workflow/repository and dedicated Dispatch bounds required",
            ));
        }
        let cap = self
            .provider
            .scoped_read_window
            .map_or(512 * 1024, |(_, cap)| cap.min(512 * 1024));
        self.reserve_measured_pool_v2(
            max_requests,
            cap,
            1024 * 1024,
            remaining_repository_reads,
            cancel,
            MeasuredPoolProfileV2::Dispatch,
        )
    }
    fn reserve_measured_pool_v2(
        &mut self,
        max_requests: usize,
        max_child_bytes: usize,
        max_pool_bytes: usize,
        remaining_repository_reads: usize,
        cancel: &CancellationToken,
        profile: MeasuredPoolProfileV2,
    ) -> GithubResult<MeasuredExternalReadWindowV1> {
        let (archive, dispatch) = match profile {
            MeasuredPoolProfileV2::Ordinary => (false, false),
            MeasuredPoolProfileV2::Archive => (true, false),
            MeasuredPoolProfileV2::Dispatch => (false, true),
        };
        self.budget.check(cancel)?;
        if self.provider.scoped_read_budget.is_some()
            || self.measured_external.is_some()
            || self.external_requests_reserved != 0
            || !(1..=8192).contains(&max_requests)
            || !(1..=if archive {
                4 * 1024 * 1024
            } else {
                1024 * 1024
            })
                .contains(&max_child_bytes)
            || !(1..=64 * 1024 * 1024).contains(&max_pool_bytes)
            || remaining_repository_reads > 8192
            || self
                .repository_reads
                .checked_add(remaining_repository_reads)
                .is_none_or(|n| n > self.limits.max_repository_reads)
        {
            return Err(error(GithubErrorKind::InvalidTarget,"One bounded measured external pool and explicit remaining original parent reads are required; no reset/coalescing"));
        }
        let own_capacity = remaining_repository_reads
            .checked_mul(self.limits.transport.max_document_bytes)
            .and_then(|bytes| bytes.checked_add(FINAL_PRINCIPAL_CAP))
            .ok_or_else(|| {
                error(
                    GithubErrorKind::InvalidTarget,
                    "Original parent/final-proof capacity overflow; no child launched",
                )
            })?;
        let capacity=self.remaining_bytes().checked_sub(own_capacity)
            .map(|remaining|remaining.min(max_pool_bytes)).filter(|bytes|*bytes>0)
            .ok_or_else(||error(GithubErrorKind::Transport,"Whole read budget cannot preserve all original parent reads/final principal and a shared pool; no narrower scope chosen"))?;
        let measured_deadline = self
            .postwrite_reservation_v2
            .as_ref()
            .map_or(self.deadline(), |postwrite| {
                self.deadline().min(postwrite.deadline())
            });
        // Artifact-only immutable pool also retains the already captured
        // issuer's narrower deadline, just as it retains its narrower cap.
        // Generic and post-write paths below are otherwise byte-for-byte.
        let measured_deadline = if archive || dispatch {
            self.provider
                .scoped_read_window
                .map_or(measured_deadline, |(original, _)| {
                    original.min(measured_deadline)
                })
        } else {
            measured_deadline
        };
        // New post-write lineage also hard-binds its ORIGINAL issuer's child
        // cap inside the immutable pool, not merely a replaceable clone spec.
        // Existing generic (no post-write reservation) behavior is unchanged.
        let max_child_bytes = if self.postwrite_reservation_v2.is_some() || dispatch {
            self.provider
                .scoped_read_window
                .map_or(max_child_bytes, |(_, original_cap)| {
                    max_child_bytes.min(original_cap)
                })
        } else {
            self.clamp_personal_capture_v13(max_child_bytes)
        };
        let constructor = if archive {
            ScopedReadBudgetV1::new_actions_archive_v2
        } else {
            ScopedReadBudgetV1::new
        };
        let budget = constructor(
            measured_deadline,
            capacity,
            max_requests,
            max_child_bytes,
            &self.original_cancellation,
        )
        .map_err(GithubError::from)?;
        if let Some(postwrite) = &self.postwrite_reservation_v2 {
            postwrite.bind(&budget)?;
        }
        // The original session already bound its cancellation to parent/final
        // commands. The shared pool retains that same token for external clones.
        self.measured_parent_reads_remaining = remaining_repository_reads;
        self.external_requests_reserved = max_requests;
        self.measured_external = Some(MeasuredSessionPoolV1 {
            budget: budget.clone(),
        });
        Ok(MeasuredExternalReadWindowV1 {
            budget,
            max_requests,
            capacity_bytes: capacity,
        })
    }
    /// Used ONLY by existing original verify immediately before its real GET.
    pub(super) fn claim_measured_parent_read(
        &mut self,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        self.budget.check(cancel)?;
        if let Some(budget) = &self.measured_external {
            budget.check_clock(cancel).map_err(GithubError::from)?;
            let state = budget.accounting().map_err(GithubError::from)?;
            if state.sealed() || self.measured_parent_reads_remaining == 0 {
                self.fatal = Some(GithubErrorKind::Transport);
                return Err(error(
                    GithubErrorKind::Transport,
                    "Original parent-read allowance exhausted/sealed; no extra GET or clock reset",
                ));
            }
            self.measured_parent_reads_remaining -= 1;
        }
        Ok(())
    }
    /// Unknown/poisoned ledger is fully unavailable, never a zero byte charge.
    pub(super) fn measured_held_bytes(&self) -> usize {
        self.measured_external
            .as_ref()
            .map_or(0, |budget| budget.held_bytes().unwrap_or(usize::MAX))
    }
    /// Parent close calls this BEFORE original SQL callback/final principal.
    /// Seal requires no live children and permanently prevents all clone reads.
    pub(super) fn seal_measured_external(
        &self,
        cancel: &CancellationToken,
    ) -> GithubResult<Option<ScopedReadAccountingV1>> {
        self.measured_external
            .as_ref()
            .map(|budget| budget.seal(cancel).map_err(GithubError::from))
            .transpose()
    }
}

impl RepositoryCatalogueAccessSession<'_> {
    pub(crate) fn reserve_dispatch_reads_v7(
        &mut self,
        target: &Arc<crate::providers::gh::ScopedActionsWorkflowV2>,
        max_requests: usize,
        remaining_repository_reads: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<MeasuredExternalReadWindowV1> {
        self.core.reserve_dispatch_reads_v7(
            target,
            max_requests,
            remaining_repository_reads,
            cancel,
        )
    }
    /// One wrapper call; Core retains actual rows/pool/cancellation/clock. No
    /// catalogue imitation, foreign issuer or replacement allocation exists.
    pub(crate) fn reserve_actions_archive_v2(
        &mut self,
        target: &Arc<ScopedActionsArtifactV2>,
        remaining_repository_reads: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<MeasuredExternalReadWindowV1> {
        self.core
            .reserve_actions_archive_v2(target, remaining_repository_reads, cancel)
    }
    /// Delegate into the SAME original core; no replacement pool or clock.
    pub fn reserve_measured_external_reads(
        &mut self,
        max_requests: usize,
        max_child_bytes: usize,
        max_pool_bytes: usize,
        remaining_repository_reads: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<MeasuredExternalReadWindowV1> {
        self.core.reserve_measured_external_reads(
            max_requests,
            max_child_bytes,
            max_pool_bytes,
            remaining_repository_reads,
            cancel,
        )
    }
}

#[cfg(test)]
#[path = "actions_archive_core_v2_tests.rs"]
mod actions_archive_core_v2_tests;
#[cfg(test)]
#[path = "measured_reads_v1_tests.rs"]
mod tests;
