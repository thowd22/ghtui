//! Same existing per-resource write state; bounded recovery observations never
//! enqueue a process or grant a write. Legacy reconciliation remains unchanged.
use super::*;

fn phase(cancel: &CancellationToken, deadline: Instant) -> Result<(), CommandFailure> {
    if cancel.is_cancelled() {
        return Err(failure(
            FailureKind::Cancelled,
            "Review recovery cancelled",
            false,
        ));
    }
    if Instant::now() >= deadline {
        return Err(failure(
            FailureKind::Timeout,
            "Original review recovery deadline expired",
            false,
        ));
    }
    Ok(())
}
impl CommandRunner {
    fn review_resource_state_v11(
        &self,
        resource: &str,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<Arc<Mutex<ResourceState>>, CommandFailure> {
        if resource.is_empty() || resource.len() > 4096 || resource.chars().any(char::is_control) {
            return Err(failure(
                FailureKind::ReconciliationRequired,
                "Exact bounded review resource required",
                false,
            ));
        }
        loop {
            phase(cancel, deadline)?;
            match self.state.try_lock() {
                Ok(mut state) => {
                    let lock = state.writes.entry(resource.to_owned()).or_default().clone();
                    phase(cancel, deadline)?;
                    return Ok(lock);
                }
                Err(std::sync::TryLockError::Poisoned(_)) => {
                    return Err(failure(
                        FailureKind::ReconciliationRequired,
                        "Review recovery execution state unavailable",
                        false,
                    ))
                }
                Err(std::sync::TryLockError::WouldBlock) => thread::sleep(Duration::from_millis(5)),
            }
        }
    }
    pub(crate) fn review_resource_generation_v11(
        &self,
        resource: &str,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<u64, CommandFailure> {
        let lock = self.review_resource_state_v11(resource, cancel, deadline)?;
        loop {
            phase(cancel, deadline)?;
            match lock.try_lock() {
                Ok(state) => {
                    phase(cancel, deadline)?;
                    return Ok(state.generation);
                }
                Err(std::sync::TryLockError::Poisoned(_)) => {
                    return Err(failure(
                        FailureKind::ReconciliationRequired,
                        "Review recovery resource unavailable",
                        false,
                    ))
                }
                Err(std::sync::TryLockError::WouldBlock) => thread::sleep(Duration::from_millis(5)),
            }
        }
    }
    pub(crate) fn reconcile_review_generation_v11(
        &self,
        resource: &str,
        generation: u64,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<(), CommandFailure> {
        let lock = self.review_resource_state_v11(resource, cancel, deadline)?;
        loop {
            phase(cancel, deadline)?;
            match lock.try_lock() {
                Ok(mut state) => {
                    phase(cancel, deadline)?;
                    if state.generation != generation {
                        return Err(failure(
                            FailureKind::ReconciliationRequired,
                            "A newer write began; inspect the actual current outcome again",
                            false,
                        ));
                    }
                    state.ambiguous = false;
                    return Ok(());
                }
                Err(std::sync::TryLockError::Poisoned(_)) => {
                    return Err(failure(
                        FailureKind::ReconciliationRequired,
                        "Review recovery resource unavailable",
                        false,
                    ))
                }
                Err(std::sync::TryLockError::WouldBlock) => thread::sleep(Duration::from_millis(5)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pending_reviews_v11_existing_real_writer_lock_cannot_extend_recovery_deadline_or_clear_ambiguity(
    ) {
        let runner = CommandRunner::default();
        let resource = "review-recovery-real-locked-child";
        let child_runner = runner.clone();
        let child = thread::spawn(move || {
            child_runner.run(
                CommandSpec::new("/bin/sh")
                    .args(["-c", "sleep 0.25"])
                    .write(resource)
                    .timeout(Duration::from_secs(2)),
                &CancellationToken::default(),
            )
        });
        let until = Instant::now() + Duration::from_secs(2);
        loop {
            let lock = runner.state.lock().unwrap().writes.get(resource).cloned();
            if lock.is_some_and(|lock| {
                matches!(lock.try_lock(), Err(std::sync::TryLockError::WouldBlock))
            }) {
                break;
            }
            assert!(
                Instant::now() < until,
                "actual installed child writer owns existing resource lock"
            );
            thread::yield_now();
        }
        let started = Instant::now();
        let token = CancellationToken::default();
        let error = runner
            .review_resource_generation_v11(resource, &token, started + Duration::from_millis(20))
            .unwrap_err();
        assert_eq!(error.kind, FailureKind::Timeout);
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(
            child.join().unwrap().is_ok(),
            "actual held child reaped by SAME runner"
        );
        assert_eq!(
            runner
                .review_resource_generation_v11(
                    resource,
                    &token,
                    Instant::now() + Duration::from_secs(1)
                )
                .unwrap(),
            1
        );
    }
    #[test]
    fn pending_reviews_v11_cancel_expiry_newer_generation_and_original_reconcile_use_same_resource_state(
    ) {
        let runner = CommandRunner::default();
        let token = CancellationToken::default();
        token.cancel();
        assert_eq!(
            runner
                .review_resource_generation_v11(
                    "never-created",
                    &token,
                    Instant::now() + Duration::from_secs(1)
                )
                .unwrap_err()
                .kind,
            FailureKind::Cancelled
        );
        assert!(!runner
            .state
            .lock()
            .unwrap()
            .writes
            .contains_key("never-created"));
        let token = CancellationToken::default();
        let resource = "actual-ambiguous-resource";
        runner.mark_ambiguous(resource);
        let generation = runner
            .review_resource_generation_v11(
                resource,
                &token,
                Instant::now() + Duration::from_secs(1),
            )
            .unwrap();
        runner.mark_ambiguous(resource);
        assert!(runner
            .reconcile_review_generation_v11(
                resource,
                generation,
                &token,
                Instant::now() + Duration::from_secs(1)
            )
            .is_err());
        let lock = runner
            .state
            .lock()
            .unwrap()
            .writes
            .get(resource)
            .unwrap()
            .clone();
        assert!(lock.lock().unwrap().ambiguous);
        assert!(runner
            .reconcile_review_generation_v11(
                resource,
                generation + 1,
                &token,
                Instant::now() - Duration::from_millis(1)
            )
            .is_err());
        assert!(lock.lock().unwrap().ambiguous);
        runner
            .reconcile_review_generation_v11(
                resource,
                generation + 1,
                &token,
                Instant::now() + Duration::from_secs(1),
            )
            .unwrap();
        let state = lock.lock().unwrap();
        assert!(!state.ambiguous);
        assert_eq!(state.generation, generation + 1);
    }
}
