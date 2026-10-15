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
impl RepositoryCatalogueAccessSession<'_> {
    /// Preserve actual pre-consumed original bytes, planned remaining parent
    /// GET caps and final principal69,632 bytes, then allocate UP TO requested
    /// shared pool once. No requests*1MiB charge or first-member truncation.
    pub fn reserve_measured_external_reads(
        &mut self,
        max_requests: usize,
        max_child_bytes: usize,
        max_pool_bytes: usize,
        remaining_repository_reads: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<MeasuredExternalReadWindowV1> {
        self.budget.check(cancel)?;
        if self.provider.scoped_read_budget.is_some()
            || self.measured_external.is_some()
            || self.external_requests_reserved != 0
            || !(1..=8192).contains(&max_requests)
            || !(1..=1024 * 1024).contains(&max_child_bytes)
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
        // New post-write lineage also hard-binds its ORIGINAL issuer's child
        // cap inside the immutable pool, not merely a replaceable clone spec.
        // Existing generic (no post-write reservation) behavior is unchanged.
        let max_child_bytes = if self.postwrite_reservation_v2.is_some() {
            self.provider
                .scoped_read_window
                .map_or(max_child_bytes, |(_, original_cap)| {
                    max_child_bytes.min(original_cap)
                })
        } else {
            max_child_bytes
        };
        let budget = ScopedReadBudgetV1::new(
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

#[cfg(test)]
#[path = "measured_reads_v1_tests.rs"]
mod tests;
