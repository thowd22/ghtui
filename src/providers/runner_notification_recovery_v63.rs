//! Closed notification frontend to the SAME existing bounded write-state lock.
//! No saved ID, raw resource string or caller generation can acknowledge it.
use super::*;
impl CommandRunner {
    pub(crate) fn notification_inspection_generation_v63(
        &self,
        target: &crate::providers::gh::notifications_v63::NotificationWriteOriginV63,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<u64, CommandFailure> {
        if !target.matches_runner_v63(self) || !target.phase_matches(cancel, deadline) {
            return Err(failure(
                FailureKind::ReconciliationRequired,
                "Original notification producing runner/token/clock changed",
                false,
            ));
        }
        // Existing exact try-lock/5ms/original-clock implementation. Old Review
        // behavior and diagnostics stay byte-for-byte; no second lock/map.
        self.review_resource_generation_v11(&target.runner_resource_v63(), cancel, deadline)
    }
    pub(crate) fn reconcile_notification_inspection_v63(
        &self,
        inspection: &crate::providers::gh::NotificationInspectionV63,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<(), CommandFailure> {
        let (target, generation) = inspection
            .runner_gate_v63(self, cancel, deadline)
            .ok_or_else(|| {
                failure(
                    FailureKind::ReconciliationRequired,
                    "Fresh exact original notification inspection required; no barrier released",
                    false,
                )
            })?;
        self.reconcile_review_generation_v11(
            &target.runner_resource_v63(),
            generation,
            cancel,
            deadline,
        )
    }
}
