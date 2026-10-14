//! SOURCE ONLY: private MetricsBudget child; no new clock/transport/auth stack.
use super::*;

impl MetricsBudget {
    pub(in crate::providers::gh::metrics) fn lifecycle_deadline(&self) -> Instant {
        self.started + self.limits.max_duration
    }
    pub(in crate::providers::gh::metrics) fn lifecycle_finish(
        &self,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        let charged = self
            .bytes
            .saturating_add(self.successful_diagnostic_bytes)
            .saturating_add(self.failed_reserved_bytes);
        if self.exhausted
            || charged > self.limits.max_response_bytes
            || self.started.elapsed() >= self.limits.max_duration
            || cancel.is_cancelled()
        {
            return Err(error(GithubErrorKind::Transport,"Lifecycle observation did not finish within its original capture/deadline/cancellation budget"));
        }
        Ok(())
    }
    pub(in crate::providers::gh::metrics) fn lifecycle_assert_active(
        &self,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        let charged = self
            .bytes
            .saturating_add(self.successful_diagnostic_bytes)
            .saturating_add(self.failed_reserved_bytes);
        if self.exhausted
            || charged >= self.limits.max_response_bytes
            || self.started.elapsed() >= self.limits.max_duration
            || cancel.is_cancelled()
        {
            return Err(error(GithubErrorKind::Transport,
                "Original lifecycle capture/deadline/cancellation budget exhausted; no later child or SQL hook"));
        }
        Ok(())
    }
    pub(in crate::providers::gh::metrics) fn lifecycle_graphql_capture(
        &mut self,
        provider: &GithubProvider,
        query: &'static str,
        node: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        self.lifecycle_assert_active(cancel)?;
        let charged = self
            .bytes
            .saturating_add(self.successful_diagnostic_bytes)
            .saturating_add(self.failed_reserved_bytes);
        let cap = self
            .limits
            .max_response_bytes
            .saturating_sub(charged)
            .min(self.limits.max_document_bytes);
        let remaining = self
            .limits
            .max_duration
            .saturating_sub(self.started.elapsed());
        let mut spec = provider.spec(vec![
            "api".into(),
            "graphql".into(),
            "--hostname".into(),
            provider.host.clone(),
            "--method".into(),
            "POST".into(),
            "--input".into(),
            "-".into(),
        ]);
        spec.stdin =
            serde_json::to_vec(&json!({"query":query,"variables":{"id":node}})).map_err(|_| {
                error(
                    GithubErrorKind::InvalidTarget,
                    "Readonly lifecycle query cannot be encoded",
                )
            })?;
        // The fixed query and already bounded canonical node are not user API
        // input. No write resource is attached: GraphQL POST is a READ here.
        spec.output_limit = cap;
        spec.timeout = provider.timeout.min(remaining);
        self.lifecycle_assert_active(cancel)?;
        spec.timeout = provider.timeout.min(
            self.limits
                .max_duration
                .saturating_sub(self.started.elapsed()),
        );
        self.requests += 1;
        let output = match provider.runner.run(spec, cancel) {
            Ok(output) => output,
            Err(failure) => {
                self.failed_reserved_bytes =
                    self.failed_reserved_bytes.checked_add(cap).ok_or_else(|| {
                        error(
                            GithubErrorKind::Transport,
                            "Lifecycle failed capture accounting overflow",
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
                    "Lifecycle stdout capture overflow",
                )
            })?;
        self.successful_diagnostic_bytes = self
            .successful_diagnostic_bytes
            .checked_add(output.capture.stderr_bytes())
            .ok_or_else(|| {
                error(
                    GithubErrorKind::Transport,
                    "Lifecycle diagnostic capture overflow",
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
                "Lifecycle raw capture contradicts its immutable pre-request cap",
            ));
        }
        serde_json::from_slice(&output.stdout).map_err(|_| {
            error(
                GithubErrorKind::Json,
                "Lifecycle GraphQL returned malformed bounded JSON",
            )
        })
    }
}
