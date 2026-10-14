//! One explicit local Git read operation. Bounds only, never source/access.
//! Parent constructor/export hooks are unapplied until Root admission.
use crate::git::OriginalLocalActorOriginV1;
use crate::providers::{
    remote_git_read_window_v1::RemoteGitReadWindowV1,
    runner::{CancellationToken, CommandRunner, ScopedReadAccountingV1, ScopedReadBudgetV1},
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

const MAX_CHILD_BYTES: usize = 32 * 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;
const MAX_CHILDREN: usize = 128;
const MAX_DURATION: Duration = Duration::from_secs(30);

/// Captured BEFORE queueing by the actual Background owner. No Clone/serde;
/// all genuine handle/window clones share this one measured monotonic ledger.
pub struct LocalGitNavigationReadV1 {
    window: RemoteGitReadWindowV1,
    pool: Arc<ScopedReadBudgetV1>,
    cancellation: CancellationToken,
    deadline: Instant,
}
impl std::fmt::Debug for LocalGitNavigationReadV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LocalGitNavigationReadV1(<original local read bounds; no access>)")
    }
}
impl LocalGitNavigationReadV1 {
    pub fn begin(
        origin: &OriginalLocalActorOriginV1,
        actual_request_token: CancellationToken,
    ) -> Result<Self, String> {
        let deadline = Instant::now()
            .checked_add(MAX_DURATION)
            .ok_or("Original local navigation deadline unavailable")?;
        Self::begin_under_deadline(origin, actual_request_token, deadline)
    }
    /// A shorter caller deadline is retained. This cannot renew an old operation.
    pub(crate) fn begin_under_deadline(
        origin: &OriginalLocalActorOriginV1,
        actual_request_token: CancellationToken,
        deadline: Instant,
    ) -> Result<Self, String> {
        Self::allocate(
            origin
                .original_repository_v1()
                .actual_read_runner_v1()
                .clone(),
            actual_request_token,
            deadline,
            MAX_TOTAL_BYTES,
            MAX_CHILDREN,
            MAX_CHILD_BYTES,
        )
    }
    fn allocate(
        original_runner: CommandRunner,
        actual_request_token: CancellationToken,
        deadline: Instant,
        max_bytes: usize,
        max_children: usize,
        max_child_bytes: usize,
    ) -> Result<Self, String> {
        let now = Instant::now();
        if deadline
            .checked_duration_since(now)
            .is_none_or(|left| left.is_zero() || left > MAX_DURATION)
            || !(1..=MAX_TOTAL_BYTES).contains(&max_bytes)
            || !(1..=MAX_CHILDREN).contains(&max_children)
            || !(1..=MAX_CHILD_BYTES).contains(&max_child_bytes)
        {
            return Err(
                "Local navigation needs its original <=30s/64MiB/128-child read bounds".into(),
            );
        }
        let pool = ScopedReadBudgetV1::new_git_navigation(
            deadline,
            max_bytes,
            max_children,
            max_child_bytes,
            &actual_request_token,
        )
        .map_err(|error| error.to_string())?;
        let window = RemoteGitReadWindowV1::new(
            original_runner,
            pool.clone(),
            deadline,
            max_child_bytes,
            actual_request_token.clone(),
        );
        Ok(Self {
            window,
            pool,
            cancellation: actual_request_token,
            deadline,
        })
    }
    pub fn cancellation(&self) -> &CancellationToken {
        &self.cancellation
    }
    pub fn deadline(&self) -> Instant {
        self.deadline
    }
    pub fn check(&self) -> Result<(), String> {
        if self
            .pool
            .accounting()
            .map_err(|error| error.to_string())?
            .sealed()
        {
            return Err(
                "Original local navigation operation is completed; start a NEW explicit read"
                    .into(),
            );
        }
        self.window
            .check(&self.cancellation)
            .map_err(|error| error.to_string())
    }
    pub fn accounting(&self) -> Result<ScopedReadAccountingV1, String> {
        self.pool.accounting().map_err(|error| error.to_string())
    }
    /// Seal ONLY after all operation work/source guards. No further child may
    /// be admitted, even with another token or a clone of the original handle.
    pub fn seal(&self) -> Result<ScopedReadAccountingV1, String> {
        self.pool
            .seal(&self.cancellation)
            .map_err(|error| error.to_string())
    }
    pub(crate) fn window(&self) -> RemoteGitReadWindowV1 {
        self.window.clone()
    }
}
impl Drop for LocalGitNavigationReadV1 {
    fn drop(&mut self) {
        self.pool.abort();
    }
}

#[cfg(test)]
#[path = "local_git_navigation_read_v1_tests.rs"]
mod tests;
