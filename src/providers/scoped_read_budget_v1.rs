//! Measured external READ bounds. No auth/access/publication receipt.
//! Parent Runner/GH/session hooks are UNAPPLIED; every clock belongs to the
//! original producer, not this pool. Successful counters come only from execute.
use super::*;

const MAX_POOL_BYTES: usize = 64 * 1024 * 1024;
const MAX_CHILD_BYTES: usize = 1024 * 1024;
const MAX_REQUESTS: usize = 8192;

/// Frozen bounded accounting, not source completeness, permission or identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScopedReadAccountingV1 {
    capacity_bytes: usize,
    max_requests: usize,
    requests_claimed: usize,
    children_started: usize,
    successful_children: usize,
    stdout_bytes: usize,
    stderr_bytes: usize,
    failed_reserved_bytes: usize,
    in_flight_requests: usize,
    in_flight_reserved_bytes: usize,
    sealed: bool,
}
impl ScopedReadAccountingV1 {
    pub fn capacity_bytes(&self) -> usize {
        self.capacity_bytes
    }
    pub fn max_requests(&self) -> usize {
        self.max_requests
    }
    /// Pre-spawn claims include failed spawn/pre-spawn race, not only children.
    pub fn requests_claimed(&self) -> usize {
        self.requests_claimed
    }
    pub fn children_started(&self) -> usize {
        self.children_started
    }
    pub fn successful_children(&self) -> usize {
        self.successful_children
    }
    pub fn stdout_bytes(&self) -> usize {
        self.stdout_bytes
    }
    pub fn stderr_bytes(&self) -> usize {
        self.stderr_bytes
    }
    /// No sealed counters exist in current CommandFailure, so failed claims
    /// permanently consume their FULL cap, never redacted diagnostic length.
    pub fn failed_reserved_bytes(&self) -> usize {
        self.failed_reserved_bytes
    }
    pub fn charged_bytes(&self) -> usize {
        self.stdout_bytes + self.stderr_bytes + self.failed_reserved_bytes
    }
    pub fn in_flight_requests(&self) -> usize {
        self.in_flight_requests
    }
    pub fn in_flight_reserved_bytes(&self) -> usize {
        self.in_flight_reserved_bytes
    }
    pub fn sealed(&self) -> bool {
        self.sealed
    }
}

/// No Clone/serde/public constructor. Internal Arc handles may share only this
/// immutable original deadline/capacity and monotonic accounting; no reset.
pub(crate) struct ScopedReadBudgetV1 {
    deadline: Instant,
    original_cancellation: CancellationToken,
    aborted: CancellationToken,
    max_child_bytes: usize,
    state: Mutex<ScopedReadAccountingV1>,
}
impl std::fmt::Debug for ScopedReadBudgetV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedReadBudgetV1(<shared original clock; bounds only>)")
    }
}
impl ScopedReadBudgetV1 {
    pub(crate) fn new(
        deadline: Instant,
        capacity_bytes: usize,
        max_requests: usize,
        max_child_bytes: usize,
        original_cancellation: &CancellationToken,
    ) -> Result<Arc<Self>, CommandFailure> {
        Self::new_with_child_ceiling(
            deadline,
            capacity_bytes,
            max_requests,
            max_child_bytes,
            original_cancellation,
            MAX_CHILD_BYTES,
        )
    }
    pub(crate) fn new_git_navigation(
        deadline: Instant,
        capacity_bytes: usize,
        max_requests: usize,
        max_child_bytes: usize,
        original_cancellation: &CancellationToken,
    ) -> Result<Arc<Self>, CommandFailure> {
        if max_requests > 128 {
            return Err(bounds_failure(
                FailureKind::OutputLimit,
                "Git navigation child cap exceeds128; no command launched",
            ));
        }
        Self::new_with_child_ceiling(
            deadline,
            capacity_bytes,
            max_requests,
            max_child_bytes,
            original_cancellation,
            32 * 1024 * 1024,
        )
    }
    /// Private Actions-archive allocation only. The caller must already have
    /// clamped max_child_bytes to the original captured provider's cap. The
    /// immutable pool, not a subsequently replaceable provider clone, binds it.
    pub(crate) fn new_actions_archive_v2(
        deadline: Instant,
        capacity_bytes: usize,
        max_requests: usize,
        max_child_bytes: usize,
        original_cancellation: &CancellationToken,
    ) -> Result<Arc<Self>, CommandFailure> {
        if max_requests > 24 || capacity_bytes > 8 * 1024 * 1024 {
            return Err(bounds_failure(
                FailureKind::OutputLimit,
                "Actions archive pool exceeds original24-child/8MiB bound",
            ));
        }
        Self::new_with_child_ceiling(
            deadline,
            capacity_bytes,
            max_requests,
            max_child_bytes,
            original_cancellation,
            4 * 1024 * 1024,
        )
    }
    fn new_with_child_ceiling(
        deadline: Instant,
        capacity_bytes: usize,
        max_requests: usize,
        max_child_bytes: usize,
        original_cancellation: &CancellationToken,
        child_ceiling: usize,
    ) -> Result<Arc<Self>, CommandFailure> {
        if !(1..=MAX_POOL_BYTES).contains(&capacity_bytes)
            || !(1..=MAX_REQUESTS).contains(&max_requests)
            || !(1..=child_ceiling).contains(&max_child_bytes)
        {
            return Err(bounds_failure(
                FailureKind::OutputLimit,
                "Shared read pool/request/child limits are invalid; no command launched",
            ));
        }
        if Instant::now() >= deadline {
            return Err(bounds_failure(
                FailureKind::Timeout,
                "Original shared read deadline already expired; no clock reset",
            ));
        }
        if original_cancellation.is_cancelled() {
            return Err(bounds_failure(
                FailureKind::Cancelled,
                "Original read was cancelled before shared pool allocation",
            ));
        }
        Ok(Arc::new(Self {
            deadline,
            original_cancellation: original_cancellation.clone(),
            aborted: CancellationToken::default(),
            max_child_bytes,
            state: Mutex::new(ScopedReadAccountingV1 {
                capacity_bytes,
                max_requests,
                requests_claimed: 0,
                children_started: 0,
                successful_children: 0,
                stdout_bytes: 0,
                stderr_bytes: 0,
                failed_reserved_bytes: 0,
                in_flight_requests: 0,
                in_flight_reserved_bytes: 0,
                sealed: false,
            }),
        }))
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }
    pub(crate) fn max_child_bytes(&self) -> usize {
        self.max_child_bytes
    }
    pub(crate) fn abort(&self) {
        self.aborted.cancel();
        if let Ok(mut state) = self.state.lock() {
            state.sealed = true;
        }
    }
    pub(crate) fn accounting(&self) -> Result<ScopedReadAccountingV1, CommandFailure> {
        self.state
            .lock()
            .map(|state| *state)
            .map_err(|_| unavailable())
    }
    /// Catalogue's own reads cannot consume the unsealed external pool. Once
    /// sealed, actual captures/failure reservations remain charged forever.
    pub(crate) fn held_bytes(&self) -> Result<usize, CommandFailure> {
        let state = self.accounting()?;
        Ok(if state.sealed {
            state.charged_bytes() + state.in_flight_reserved_bytes()
        } else {
            state.capacity_bytes
        })
    }
    pub(crate) fn seal(
        &self,
        cancel: &CancellationToken,
    ) -> Result<ScopedReadAccountingV1, CommandFailure> {
        let mut state = self.state.lock().map_err(|_| unavailable())?;
        // Seal even when rejecting in-flight work: no new child may be queued
        // afterward. The original caller must not close a private source then.
        state.sealed = true;
        self.check_clock(cancel)?;
        if state.in_flight_requests != 0 || state.in_flight_reserved_bytes != 0 {
            return Err(bounds_failure(
                FailureKind::ReconciliationRequired,
                "Shared read pool still has in-flight children; original proof cannot close",
            ));
        }
        Ok(*state)
    }
    pub(crate) fn check_clock(&self, cancel: &CancellationToken) -> Result<(), CommandFailure> {
        if self.original_cancellation.is_cancelled()
            || self.aborted.is_cancelled()
            || cancel.is_cancelled()
        {
            Err(bounds_failure(
                FailureKind::Cancelled,
                "Original shared read cancelled; no next child",
            ))
        } else if Instant::now() >= self.deadline {
            Err(bounds_failure(
                FailureKind::Timeout,
                "Original shared read deadline expired; no next child",
            ))
        } else {
            Ok(())
        }
    }
    fn claim(
        self: &Arc<Self>,
        spec: &CommandSpec,
        cancel: &CancellationToken,
    ) -> Result<ReadCaptureClaimV1, CommandFailure> {
        self.check_clock(cancel)?;
        if !matches!(spec.kind, CommandKind::Read) || spec.coalesce_read {
            return Err(bounds_failure(FailureKind::ReconciliationRequired,"Measured shared read requires an independent fresh READ; no writes or older coalesced observations"));
        }
        if spec.timeout.is_zero() || spec.output_limit == 0 {
            return Err(bounds_failure(
                FailureKind::OutputLimit,
                "Measured child has no remaining command/capture allowance",
            ));
        }
        let mut state = self.state.lock().map_err(|_| unavailable())?;
        self.check_clock(cancel)?;
        if state.sealed || state.requests_claimed >= state.max_requests {
            return Err(bounds_failure(
                FailureKind::OutputLimit,
                "Original shared read pool is sealed or child cap reached; no extra command",
            ));
        }
        let free = state
            .capacity_bytes
            .saturating_sub(state.charged_bytes())
            .saturating_sub(state.in_flight_reserved_bytes);
        let reserved = free.min(self.max_child_bytes).min(spec.output_limit);
        if reserved == 0 {
            return Err(bounds_failure(
                FailureKind::OutputLimit,
                "Original shared capture pool exhausted; no next command or smaller scope chosen",
            ));
        }
        state.requests_claimed += 1;
        state.in_flight_requests += 1;
        state.in_flight_reserved_bytes += reserved;
        Ok(ReadCaptureClaimV1 {
            budget: self.clone(),
            reserved,
            started: false,
            settled: false,
        })
    }
    pub(super) fn run(
        self: &Arc<Self>,
        mut spec: CommandSpec,
        cancel: &CancellationToken,
    ) -> RunResult {
        let mut claim = self.claim(&spec, cancel)?;
        spec.output_limit = claim.reserved;
        spec.timeout = spec
            .timeout
            .min(self.deadline.saturating_duration_since(Instant::now()));
        let result = execute(&spec, cancel, Some(&mut claim));
        claim.finish(&result)?;
        result
    }
}

pub(super) struct ReadCaptureClaimV1 {
    budget: Arc<ScopedReadBudgetV1>,
    reserved: usize,
    started: bool,
    settled: bool,
}
impl ReadCaptureClaimV1 {
    pub(super) fn original_cancelled(&self) -> bool {
        self.budget.original_cancellation.is_cancelled() || self.budget.aborted.is_cancelled()
    }
    pub(super) fn original_deadline_expired(&self) -> bool {
        Instant::now() >= self.budget.deadline
    }
    /// Called by execute IMMEDIATELY before Command::spawn, after all setup.
    pub(super) fn before_spawn(&self, cancel: &CancellationToken) -> Result<(), CommandFailure> {
        self.budget.check_clock(cancel)?;
        let state = self.budget.state.lock().map_err(|_| unavailable())?;
        self.budget.check_clock(cancel)?;
        if state.sealed {
            Err(bounds_failure(
                FailureKind::OutputLimit,
                "Original read pool sealed before spawn; no child launched",
            ))
        } else {
            Ok(())
        }
    }
    /// Called ONLY after the real std::process::Command::spawn returned a child.
    pub(super) fn started(&mut self) -> Result<(), CommandFailure> {
        if self.started {
            return Err(unavailable());
        }
        let mut state = self.budget.state.lock().map_err(|_| unavailable())?;
        state.children_started += 1;
        self.started = true;
        Ok(())
    }
    fn finish(&mut self, result: &RunResult) -> Result<(), CommandFailure> {
        let mut state = self.budget.state.lock().map_err(|_| unavailable())?;
        if self.settled
            || state.in_flight_requests == 0
            || state.in_flight_reserved_bytes < self.reserved
        {
            return Err(unavailable());
        }
        match result {
            Ok(output) if self.started && output.capture.total_bytes() <= self.reserved => {
                state.successful_children += 1;
                state.stdout_bytes += output.capture.stdout_bytes();
                state.stderr_bytes += output.capture.stderr_bytes();
            }
            Ok(_) => {
                return Err(bounds_failure(
                    FailureKind::OutputLimit,
                    "Sealed child capture contradicts original reservation; output not admitted",
                ))
            }
            Err(_) => state.failed_reserved_bytes += self.reserved,
        }
        state.in_flight_requests -= 1;
        state.in_flight_reserved_bytes -= self.reserved;
        self.settled = true;
        Ok(())
    }
}
impl Drop for ReadCaptureClaimV1 {
    fn drop(&mut self) {
        if !self.settled {
            // Catch-unwind/early return must never release an unmeasured claim.
            // A poisoned ledger stays inaccessible/held, never guessed zero.
            if let Ok(mut state) = self.budget.state.lock() {
                state.failed_reserved_bytes += self.reserved;
                state.in_flight_requests = state.in_flight_requests.saturating_sub(1);
                state.in_flight_reserved_bytes =
                    state.in_flight_reserved_bytes.saturating_sub(self.reserved);
            }
        }
    }
}
fn bounds_failure(kind: FailureKind, message: &'static str) -> CommandFailure {
    failure(kind, message, false)
}
fn unavailable() -> CommandFailure {
    bounds_failure(
        FailureKind::OutputLimit,
        "Shared original capture ledger unavailable; no byte count or access guessed",
    )
}

#[cfg(test)]
#[path = "scoped_read_budget_v1_tests.rs"]
mod tests;
