//! Fixed two-resource manual Issue operation on the EXISTING write map.
//! No raw-spec/resource-vector/closure API can enqueue a multi-resource write.
use super::*;
use crate::providers::gh::pr_issue_links_v87::PreparedIssueLinkRequestV87;
use std::sync::MutexGuard;

type IssueLinkLocksV87 = ([Arc<Mutex<ResourceState>>; 2], [usize; 2]);

pub(in crate::providers) struct IssueLinkMutationRunV87 {
    output: CommandOutput,
    successors: [u64; 2],
    ticket: std::sync::Weak<()>,
}
impl IssueLinkMutationRunV87 {
    pub(in crate::providers) fn matches_ticket_v87(&self, ticket: &Arc<()>) -> bool {
        self.ticket.ptr_eq(&Arc::downgrade(ticket))
    }
    pub(in crate::providers) fn successors(&self) -> [u64; 2] {
        self.successors
    }
    pub(in crate::providers) fn output(&self) -> &CommandOutput {
        &self.output
    }
}
fn phase(cancel: &CancellationToken, deadline: Instant) -> Result<(), CommandFailure> {
    if cancel.is_cancelled() {
        return Err(failure(
            FailureKind::Cancelled,
            "Original Issue-link wait cancelled",
            false,
        ));
    }
    if Instant::now() >= deadline {
        return Err(failure(
            FailureKind::Timeout,
            "Original Issue-link deadline expired",
            false,
        ));
    }
    Ok(())
}
fn lock<'a>(
    resource: &'a Arc<Mutex<ResourceState>>,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<MutexGuard<'a, ResourceState>, CommandFailure> {
    loop {
        phase(cancel, deadline)?;
        match resource.try_lock() {
            Ok(guard) => {
                phase(cancel, deadline)?;
                return Ok(guard);
            }
            Err(std::sync::TryLockError::Poisoned(_)) => {
                return Err(failure(
                    FailureKind::ReconciliationRequired,
                    "Actual Issue-link resource state unavailable",
                    false,
                ))
            }
            Err(std::sync::TryLockError::WouldBlock) => thread::sleep(Duration::from_millis(5)),
        }
    }
}
impl CommandRunner {
    pub(in crate::providers) fn reconcile_issue_link_inspection_v87(
        &self,
        observed: &crate::providers::gh::pr_issue_links_v87::IssueLinkRecoveryCurrentV87,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<(), CommandFailure> {
        let (resources, expected, deadline) = observed
            .runner_parts_v87(self, cancel, deadline)
            .map_err(|_| {
                failure(
                    FailureKind::ReconciliationRequired,
                    "Exact fresh two-resource inspection unavailable or consumed",
                    false,
                )
            })?;
        let (locks, order) = self.issue_link_locks_v87(resources, cancel, deadline)?;
        let mut first = lock(&locks[0], cancel, deadline)?;
        let mut second = lock(&locks[1], cancel, deadline)?;
        phase(cancel, deadline)?;
        if first.generation != expected[order[0]] || second.generation != expected[order[1]] {
            return Err(failure(
                FailureKind::ReconciliationRequired,
                "Newer PR/Issue version cannot be cleared by old inspection",
                false,
            ));
        }
        first.ambiguous = false;
        second.ambiguous = false;
        Ok(())
    }
    fn issue_link_locks_v87(
        &self,
        resources: &[String; 2],
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<IssueLinkLocksV87, CommandFailure> {
        if resources[0] == resources[1]
            || resources
                .iter()
                .any(|r| r.is_empty() || r.len() > 4096 || r.chars().any(char::is_control))
        {
            return Err(failure(
                FailureKind::ReconciliationRequired,
                "Two distinct actual bounded Issue-link resources required",
                false,
            ));
        }
        let order = if resources[0] < resources[1] {
            [0, 1]
        } else {
            [1, 0]
        };
        loop {
            phase(cancel, deadline)?;
            match self.state.try_lock() {
                Ok(mut state) => {
                    let first = state
                        .writes
                        .entry(resources[order[0]].clone())
                        .or_default()
                        .clone();
                    let second = state
                        .writes
                        .entry(resources[order[1]].clone())
                        .or_default()
                        .clone();
                    phase(cancel, deadline)?;
                    return Ok(([first, second], order));
                }
                Err(std::sync::TryLockError::Poisoned(_)) => {
                    return Err(failure(
                        FailureKind::ReconciliationRequired,
                        "Actual Issue-link execution state unavailable",
                        false,
                    ))
                }
                Err(std::sync::TryLockError::WouldBlock) => thread::sleep(Duration::from_millis(5)),
            }
        }
    }
    /// Bounded scalar observation only. It cannot clear ambiguity or invoke a
    /// child. Both ordinary PR writers and this pair use these exact same locks.
    pub(crate) fn issue_link_generations_v87(
        &self,
        resources: &[String; 2],
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<[u64; 2], CommandFailure> {
        let (locks, order) = self.issue_link_locks_v87(resources, cancel, deadline)?;
        let first = lock(&locks[0], cancel, deadline)?;
        let second = lock(&locks[1], cancel, deadline)?;
        phase(cancel, deadline)?;
        let mut generations = [0; 2];
        generations[order[0]] = first.generation;
        generations[order[1]] = second.generation;
        Ok(generations)
    }
    pub(crate) fn validate_issue_link_generations_v87(
        &self,
        resources: &[String; 2],
        expected: [u64; 2],
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<(), CommandFailure> {
        if self.issue_link_generations_v87(resources, cancel, deadline)? != expected {
            return Err(failure(
                FailureKind::ReconciliationRequired,
                "A newer write changed the original PR or selected Issue",
                false,
            ));
        }
        Ok(())
    }
    pub(in crate::providers) fn run_issue_link_pair_v87(
        &self,
        request: &PreparedIssueLinkRequestV87,
        cancel: &CancellationToken,
    ) -> Result<IssueLinkMutationRunV87, CommandFailure> {
        let (spec, resources, expected, deadline) = request.runner_parts_v87(self, cancel)?;
        let (locks, order) = self.issue_link_locks_v87(resources, cancel, deadline)?;
        let mut first = lock(&locks[0], cancel, deadline)?;
        let mut second = lock(&locks[1], cancel, deadline)?;
        if first.ambiguous
            || second.ambiguous
            || first.generation != expected[order[0]]
            || second.generation != expected[order[1]]
        {
            return Err(failure(
                FailureKind::ReconciliationRequired,
                "Original PR/Issue changed or has an unknown write; inspect without resending",
                false,
            ));
        }
        // Overflow is a refusal BEFORE either current generation is changed.
        let next_first = first.generation.checked_add(1).ok_or_else(|| {
            failure(
                FailureKind::ReconciliationRequired,
                "PR/Issue generation exhausted",
                false,
            )
        })?;
        let next_second = second.generation.checked_add(1).ok_or_else(|| {
            failure(
                FailureKind::ReconciliationRequired,
                "PR/Issue generation exhausted",
                false,
            )
        })?;
        request.runner_parts_v87(self, cancel)?;
        phase(cancel, deadline)?;
        first.generation = next_first;
        second.generation = next_second;
        // No nested run(Write), new queue/map, coalescing or renewed deadline.
        // Absolute I includes all waiting above, including both sorted locks.
        let output = match execute(spec, cancel, None) {
            Ok(output) => output,
            Err(error) => {
                if error.outcome_unknown {
                    first.ambiguous = true;
                    second.ambiguous = true;
                }
                return Err(error);
            }
        };
        // Validate success while BOTH locks are still held. A malformed success
        // cannot leave a gap in which an ordinary PR writer sees no barrier.
        if !request.accepts_mutation_output_v87(&output) {
            first.ambiguous = true;
            second.ambiguous = true;
            return Err(failure(
                FailureKind::ReconciliationRequired,
                "Issue-link response was unusable; both resources require fresh inspection",
                true,
            ));
        }
        // A mutation response alone is NOT the completed two-sided receipt.
        // Keep both barriers while the actual independent readbacks run. Even
        // expiry/cancel before the provider receives this result cannot leave
        // a usable ordinary writer gap on either affected resource.
        first.ambiguous = true;
        second.ambiguous = true;
        let mut successors = [0; 2];
        successors[order[0]] = next_first;
        successors[order[1]] = next_second;
        Ok(IssueLinkMutationRunV87 {
            output,
            successors,
            ticket: request.runner_ticket_v87(),
        })
    }
    pub(in crate::providers) fn mark_issue_link_pair_unusable_v87(
        &self,
        request: &PreparedIssueLinkRequestV87,
        cancel: &CancellationToken,
    ) -> Result<(), CommandFailure> {
        let (resources, expected, deadline) = request.runner_postwrite_v87(self, cancel)?;
        let (locks, order) = self.issue_link_locks_v87(resources, cancel, deadline)?;
        let mut first = lock(&locks[0], cancel, deadline)?;
        let mut second = lock(&locks[1], cancel, deadline)?;
        if first.generation != expected[order[0]] || second.generation != expected[order[1]] {
            return Err(failure(
                FailureKind::ReconciliationRequired,
                "A newer PR/Issue generation cannot be adopted by this response",
                true,
            ));
        }
        first.ambiguous = true;
        second.ambiguous = true;
        Ok(())
    }
    pub(in crate::providers) fn complete_issue_link_receipt_v87(
        &self,
        request: &PreparedIssueLinkRequestV87,
        cancel: &CancellationToken,
    ) -> Result<(), CommandFailure> {
        let (resources, expected, deadline) = request.runner_confirmed_parts_v87(self, cancel)?;
        let (locks, order) = self.issue_link_locks_v87(resources, cancel, deadline)?;
        let mut first = lock(&locks[0], cancel, deadline)?;
        let mut second = lock(&locks[1], cancel, deadline)?;
        phase(cancel, deadline)?;
        if first.generation != expected[order[0]] || second.generation != expected[order[1]] {
            return Err(failure(
                FailureKind::ReconciliationRequired,
                "A newer PR/Issue version cannot be cleared by an old receipt",
                true,
            ));
        }
        first.ambiguous = false;
        second.ambiguous = false;
        Ok(())
    }
}
