//! Private bounds-only forwarding for a deliberate remote-to-local READ.
//! No source/account/mapping authority. Default Git/runner paths unchanged.
use super::runner::{
    CancellationToken, CommandFailure, CommandKind, CommandOutput, CommandRunner, CommandSpec,
    FailureKind, ScopedReadBudgetV1,
};
use std::{sync::Arc, time::Instant};

#[derive(Clone)]
pub(crate) struct RemoteGitReadWindowV1 {
    runner: CommandRunner,
    pool: Arc<ScopedReadBudgetV1>,
    deadline: Instant,
    max_child_bytes: usize,
    original: CancellationToken,
}
impl std::fmt::Debug for RemoteGitReadWindowV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RemoteGitReadWindowV1(<bounds only; no mapping/auth>)")
    }
}
impl RemoteGitReadWindowV1 {
    pub(crate) fn deadline_v1(&self) -> Instant {
        self.deadline
    }
    pub(crate) fn actual_runner_v1(&self) -> &CommandRunner {
        &self.runner
    }
    pub(crate) fn same_operation_v2(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.pool, &other.pool)
            && self.deadline == other.deadline
            && self.runner.same_instance(&other.runner)
    }
    pub(crate) fn bounds_completed_v1(&self) -> Result<bool, CommandFailure> {
        Ok(self.pool.accounting()?.sealed())
    }
    pub(crate) fn new(
        runner: CommandRunner,
        pool: Arc<ScopedReadBudgetV1>,
        deadline: Instant,
        max_child_bytes: usize,
        original: CancellationToken,
    ) -> Self {
        Self {
            runner,
            pool,
            deadline,
            max_child_bytes,
            original,
        }
    }
    pub(crate) fn check(&self, cancel: &CancellationToken) -> Result<(), CommandFailure> {
        if self.original.is_cancelled() || cancel.is_cancelled() || Instant::now() >= self.deadline
        {
            return Err(failure("Original remote/local read expired or cancelled"));
        }
        self.pool.check_clock(cancel)
    }
    pub(crate) fn run(
        &self,
        mut spec: CommandSpec,
        cancel: &CancellationToken,
    ) -> Result<CommandOutput, CommandFailure> {
        self.check(cancel)?;
        if spec.kind != CommandKind::Read
            || spec.program != "git"
            || spec.scoped_read_budget.is_some()
        {
            return Err(failure(
                "Remote/local read forwarding refuses writes, foreign programs or nested budgets",
            ));
        }
        spec.timeout = spec
            .timeout
            .min(self.deadline.saturating_duration_since(Instant::now()));
        spec.output_limit = spec.output_limit.min(self.max_child_bytes);
        spec.scoped_read_budget = Some(self.pool.clone());
        spec.original_read_cancellation = Some(self.original.clone());
        // Each operation's actual read must be charged, never borrow a result
        // from a differently bounded in-flight legacy Git child.
        spec.coalesce_read = false;
        self.actual_runner_v1().run(spec, cancel)
    }
}
fn failure(message: &str) -> CommandFailure {
    CommandFailure {
        kind: FailureKind::Cancelled,
        message: message.into(),
        exit_code: None,
        stdout: vec![],
        stderr: vec![],
        outcome_unknown: false,
    }
}
