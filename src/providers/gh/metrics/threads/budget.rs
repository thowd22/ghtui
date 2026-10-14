use super::*;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub(super) fn now() -> GithubResult<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_secs())
        .map_err(|_| {
            error(
                GithubErrorKind::Transport,
                "Thread observation UTC clock unavailable",
            )
        })
}
pub(super) struct ThreadBudget {
    pub limits: MetricsThreadLimits,
    pub observed_from: u64,
    pub requests: usize,
    pub bytes: usize,
    pub successful_diagnostic_bytes: usize,
    pub failed_reserved_bytes: usize,
    pub graphql_pages: usize,
    pub raw_comments: usize,
    started: Instant,
    exhausted: bool,
}
impl ThreadBudget {
    pub fn new(limits: &MetricsThreadLimits) -> GithubResult<Self> {
        limits.validate()?;
        Ok(Self {
            limits: limits.clone(),
            observed_from: now()?,
            requests: 0,
            bytes: 0,
            successful_diagnostic_bytes: 0,
            failed_reserved_bytes: 0,
            graphql_pages: 0,
            raw_comments: 0,
            started: Instant::now(),
            exhausted: false,
        })
    }
    fn run(
        &mut self,
        provider: &GithubProvider,
        endpoint: &str,
        body: Option<Value>,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        let remaining = self.limits.max_response_bytes.saturating_sub(
            self.bytes
                .saturating_add(self.successful_diagnostic_bytes)
                .saturating_add(self.failed_reserved_bytes),
        );
        let time = self
            .limits
            .max_duration
            .saturating_sub(self.started.elapsed());
        if self.exhausted || remaining == 0 || time.is_zero() || cancel.is_cancelled() {
            return Err(error(
                GithubErrorKind::Transport,
                "Thread aggregate byte/time/cancellation budget reached before another gh request",
            ));
        }
        let mut args = vec![
            "api".into(),
            endpoint.into(),
            "--hostname".into(),
            provider.host.clone(),
            "--method".into(),
            if body.is_some() { "POST" } else { "GET" }.into(),
        ];
        if body.is_some() {
            args.extend(["--input".into(), "-".into()]);
        }
        let mut spec = provider.spec(args);
        spec.output_limit = remaining.min(if body.is_some() {
            4 * 1024 * 1024
        } else {
            self.limits.max_document_bytes
        });
        spec.timeout = provider.timeout.min(time);
        let output_cap = spec.output_limit;
        if let Some(body) = body {
            spec.stdin = serde_json::to_vec(&body)
                .map_err(|err| error(GithubErrorKind::Json, err.to_string()))?;
        }
        self.requests += 1;
        let output = match provider.runner.run(spec, cancel) {
            Ok(output) => output,
            Err(err) => {
                self.failed_reserved_bytes = self
                    .failed_reserved_bytes
                    .checked_add(output_cap)
                    .ok_or_else(|| {
                        error(
                            GithubErrorKind::Transport,
                            "Failed thread shared capture reservation overflow",
                        )
                    })?;
                if matches!(
                    err.kind,
                    FailureKind::OutputLimit | FailureKind::Timeout | FailureKind::Cancelled
                ) {
                    self.exhausted = true;
                }
                return Err(GithubError::from(err));
            }
        };
        self.bytes = self
            .bytes
            .checked_add(output.capture.stdout_bytes())
            .ok_or_else(|| {
                error(
                    GithubErrorKind::Json,
                    "Thread response byte accounting overflow",
                )
            })?;
        self.successful_diagnostic_bytes = self
            .successful_diagnostic_bytes
            .checked_add(output.capture.stderr_bytes())
            .ok_or_else(|| {
                error(
                    GithubErrorKind::Transport,
                    "Successful thread diagnostic capture overflow",
                )
            })?;
        if output.capture.stdout_bytes() != output.stdout.len()
            || output.capture.total_bytes() > output_cap
            || self
                .bytes
                .saturating_add(self.successful_diagnostic_bytes)
                .saturating_add(self.failed_reserved_bytes)
                > self.limits.max_response_bytes
        {
            self.exhausted = true;
            return Err(error(
                GithubErrorKind::Json,
                "Thread runner exceeded pre-request aggregate response cap",
            ));
        }
        let value: Value = serde_json::from_slice(&output.stdout)
            .map_err(|err| error(GithubErrorKind::Json, err.to_string()))?;
        if value.get("errors").is_some_and(|errors| {
            !errors.is_null() && errors.as_array().is_none_or(|errors| !errors.is_empty())
        }) {
            return Err(error(
                GithubErrorKind::Json,
                "GraphQL thread page contains errors; partial data cannot prove complete evidence",
            ));
        }
        Ok(value)
    }
    pub fn get(
        &mut self,
        provider: &GithubProvider,
        endpoint: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        self.run(provider, endpoint, None, cancel)
    }
    pub fn graphql(
        &mut self,
        provider: &GithubProvider,
        query: &str,
        variables: Value,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        if self.graphql_pages >= self.limits.max_graphql_pages {
            return Err(error(
                GithubErrorKind::Transport,
                "Thread GraphQL page budget reached before another request",
            ));
        }
        self.graphql_pages += 1;
        self.run(
            provider,
            "graphql",
            Some(json!({"query":query,"variables":variables})),
            cancel,
        )
    }
    pub fn principal(
        &mut self,
        provider: &GithubProvider,
        expected: &HostIdentity,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        let current = self.get(provider, "user", cancel)?;
        if positive(&current, "id")? != expected.account_id.unwrap_or(0) {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Thread reader principal changed; no private thread evidence is published",
            ));
        }
        validate_name(&text(&current, "login"))?;
        Ok(())
    }
    pub fn through(&self) -> GithubResult<u64> {
        let through = now()?;
        if through < self.observed_from {
            return Err(error(
                GithubErrorKind::Transport,
                "UTC clock moved backwards during thread observation",
            ));
        }
        Ok(through)
    }
}
