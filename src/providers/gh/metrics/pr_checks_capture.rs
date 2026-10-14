//! Private paging child sharing the exact existing budget fields.
use super::*;

impl MetricsBudget {
    pub(in crate::providers::gh::metrics) fn pr_check_guard(
        &self,
        cancel: &CancellationToken,
        guard: impl FnOnce() -> GithubResult<()>,
    ) -> GithubResult<()> {
        let available = || {
            !self.exhausted
                && !cancel.is_cancelled()
                && self.started.elapsed() < self.limits.max_duration
                && self
                    .bytes
                    .saturating_add(self.successful_diagnostic_bytes)
                    .saturating_add(self.failed_reserved_bytes)
                    < self.limits.max_response_bytes
        };
        if !available() {
            return Err(error(GithubErrorKind::Transport, "Original check read deadline/cancellation/capture budget reached before local guard"));
        }
        let result = guard();
        if !available() {
            return Err(error(
                GithubErrorKind::Transport,
                "Local check guard consumed the original deadline; no source proof admitted",
            ));
        }
        result
    }
    pub(in crate::providers::gh::metrics) fn pr_check_log_capture(
        &mut self,
        provider: &GithubProvider,
        args: Vec<String>,
        max_bytes: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<String> {
        let left = self.limits.max_response_bytes.saturating_sub(
            self.bytes
                .saturating_add(self.successful_diagnostic_bytes)
                .saturating_add(self.failed_reserved_bytes),
        );
        let time = self
            .limits
            .max_duration
            .saturating_sub(self.started.elapsed());
        if self.exhausted || left == 0 || time.is_zero() || cancel.is_cancelled() {
            return Err(error(
                GithubErrorKind::Transport,
                "Original check log budget/cancellation reached; no later child launched",
            ));
        }
        let mut spec = provider.spec(args);
        spec.output_limit = left.min(max_bytes);
        spec.timeout = provider.timeout.min(time);
        let cap = spec.output_limit;
        self.requests += 1;
        let output = match provider.runner.run(spec, cancel) {
            Ok(value) => value,
            Err(failure) => {
                self.failed_reserved_bytes =
                    self.failed_reserved_bytes.checked_add(cap).ok_or_else(|| {
                        error(
                            GithubErrorKind::Transport,
                            "Check log failed capture accounting overflow",
                        )
                    })?;
                if matches!(
                    failure.kind,
                    FailureKind::OutputLimit | FailureKind::Timeout | FailureKind::Cancelled
                ) {
                    self.exhausted = true;
                }
                return Err(GithubError::from(failure));
            }
        };
        self.bytes = self
            .bytes
            .checked_add(output.capture.stdout_bytes())
            .ok_or_else(|| {
                error(
                    GithubErrorKind::Transport,
                    "Check log stdout accounting overflow",
                )
            })?;
        self.successful_diagnostic_bytes = self
            .successful_diagnostic_bytes
            .checked_add(output.capture.stderr_bytes())
            .ok_or_else(|| {
                error(
                    GithubErrorKind::Transport,
                    "Check log diagnostic accounting overflow",
                )
            })?;
        if output.capture.stdout_bytes() != output.stdout.len()
            || output.capture.total_bytes() > cap
            || self
                .bytes
                .saturating_add(self.successful_diagnostic_bytes)
                .saturating_add(self.failed_reserved_bytes)
                > self.limits.max_response_bytes
        {
            self.exhausted = true;
            return Err(error(
                GithubErrorKind::Transport,
                "Check log raw capture contradicts its pre-request shared cap",
            ));
        }
        Ok(super::super::super::super::runner::redact(
            &super::super::super::super::runner::sanitize_terminal(&String::from_utf8_lossy(
                &output.stdout,
            )),
            &[],
        ))
    }
}
