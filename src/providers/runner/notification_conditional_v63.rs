//! Opaque normally-reaped notification GET capture. No generic ignore-exit or
//! failure.stdout reinterpretation. Private bytes never implement Clone/serde.
use super::*;
use crate::providers::gh::notifications_v63::NotificationConditionalRequestV63;

pub(crate) struct NotificationConditionalOutputV63 {
    stdout: Vec<u8>,
    exit: i32,
    capture: CommandCapture,
}
impl std::fmt::Debug for NotificationConditionalOutputV63 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NotificationConditionalOutputV63(<private raw protocol capture redacted>)")
    }
}
impl std::fmt::Display for NotificationConditionalOutputV63 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}
impl NotificationConditionalOutputV63 {
    pub(in crate::providers) fn stdout_v63(&self) -> &[u8] {
        &self.stdout
    }
    pub(in crate::providers) fn exit_v63(&self) -> i32 {
        self.exit
    }
    pub(in crate::providers) fn capture_v63(&self) -> CommandCapture {
        self.capture
    }
}
impl CommandRunner {
    pub(crate) fn run_notification_conditional_get_v63(
        &self,
        request: &NotificationConditionalRequestV63,
        cancel: &CancellationToken,
    ) -> Result<NotificationConditionalOutputV63, CommandFailure> {
        let spec = request.runner_spec_v63(self, cancel)?.clone();
        let result = if let Some(budget) = spec.scoped_read_budget.clone() {
            budget.run_notification_conditional_v63(spec, cancel)
        } else {
            execute_mode_v63(
                &spec,
                cancel,
                None,
                ExecutePolicyV63::NotificationConditionalGet,
            )
        }
        .map_err(redact_conditional_failure_v63)?;
        // A timeout/cancel while joining pipes cannot turn into reusable 304.
        request.runner_spec_v63(self, cancel)?;
        if !matches!(result.exit_code, 0 | 1) {
            return Err(failure(
                FailureKind::Exit,
                "Conditional GET was not normally reaped with documented exit0/1",
                false,
            ));
        }
        Ok(NotificationConditionalOutputV63 {
            stdout: result.stdout,
            exit: result.exit_code,
            capture: result.capture,
        })
    }
}
fn redact_conditional_failure_v63(error: CommandFailure) -> CommandFailure {
    // Unlike the unchanged ordinary runner diagnostic ABI, this private raw
    // protocol path must not hand headers/private JSON to Debug or UI errors.
    CommandFailure {
        kind: error.kind,
        exit_code: error.exit_code,
        outcome_unknown: false,
        message: "Notification conditional GET failed/refused; private protocol output withheld"
            .into(),
        stdout: Vec::new(),
        stderr: Vec::new(),
    }
}
