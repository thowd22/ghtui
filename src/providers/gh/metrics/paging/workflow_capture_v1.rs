//! SOURCE ONLY raw phase factored from paging::read_capped. Activation must
//! forward existing JSON reads here so there is ONE capture-accounting policy.
use super::*;
use crate::providers::runner::CommandOutput;

impl MetricsBudget {
    pub(in crate::providers::gh::metrics) fn capture_gh_read_v1(
        &mut self,
        provider: &GithubProvider,
        args: Vec<String>,
        response_cap: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<CommandOutput> {
        let valid = args.len() == 1 && args[0] == "--version"
            || args.len() == 6
                && args[0] == "api"
                && args[2] == "--hostname"
                && args[3] == provider.host
                && args[4] == "--method"
                && args[5] == "GET";
        if !valid || response_cap == 0 {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Bounded capture accepts only gh version or exact same-host read API argv",
            ));
        }
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
                "Metric read budget/cancellation reached before another gh request",
            ));
        }
        let mut spec = provider.spec(args);
        spec.output_limit = left.min(response_cap);
        let cap = spec.output_limit;
        spec.timeout = provider.timeout.min(time);
        self.requests = self.requests.checked_add(1).ok_or_else(|| {
            error(
                GithubErrorKind::Transport,
                "Metric request counter overflow",
            )
        })?;
        let output = match provider.runner.run(spec, cancel) {
            Ok(output) => output,
            Err(failure) => {
                self.failed_reserved_bytes =
                    self.failed_reserved_bytes.checked_add(cap).ok_or_else(|| {
                        error(
                            GithubErrorKind::Transport,
                            "Metric failed capture reservation overflow",
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
            .ok_or_else(|| error(GithubErrorKind::Json, "Metric byte budget overflow"))?;
        self.successful_diagnostic_bytes = self
            .successful_diagnostic_bytes
            .checked_add(output.capture.stderr_bytes())
            .ok_or_else(|| {
                error(
                    GithubErrorKind::Transport,
                    "Metric diagnostic capture accounting overflow",
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
                GithubErrorKind::Json,
                "Metric runner violated its pre-request byte cap",
            ));
        }
        Ok(output)
    }
}
