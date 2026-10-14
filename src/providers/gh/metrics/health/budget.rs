//! Dedicated source-only transport until common runner raw capture accounting lands.
use super::*;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

fn now() -> GithubResult<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_secs())
        .map_err(|_| {
            error(
                GithubErrorKind::Transport,
                "Health observation UTC clock unavailable",
            )
        })
}
pub(super) struct HealthTransport {
    limits: MetricsReadLimits,
    started: Instant,
    exhausted: bool,
    pub observed_from: u64,
    pub requests: usize,
    /// Exact successful API stdout bytes, not a network/diagnostic byte claim.
    pub bytes: usize,
    /// Exact kept successful stderr bytes BEFORE sanitization/redaction.
    pub successful_diagnostic_bytes: usize,
    /// Full shared stdout+stderr capture cap reserved on failed commands.
    pub failed_reserved_bytes: usize,
}
impl HealthTransport {
    pub fn new(limits: &MetricsReadLimits) -> GithubResult<Self> {
        limits.validate()?;
        Ok(Self {
            limits: limits.clone(),
            started: Instant::now(),
            exhausted: false,
            observed_from: now()?,
            requests: 0,
            bytes: 0,
            successful_diagnostic_bytes: 0,
            failed_reserved_bytes: 0,
        })
    }
    fn read_capped(
        &mut self,
        provider: &GithubProvider,
        endpoint: &str,
        cap: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        let left = self.limits.max_response_bytes.saturating_sub(
            self.bytes
                .saturating_add(self.successful_diagnostic_bytes)
                .saturating_add(self.failed_reserved_bytes),
        );
        let duration = self
            .limits
            .max_duration
            .saturating_sub(self.started.elapsed());
        if self.exhausted || left == 0 || duration.is_zero() || cancel.is_cancelled() {
            return Err(error(GithubErrorKind::Transport,"Health byte/time/cancellation budget reached before another gh request; final authority cannot be fabricated"));
        }
        let output_cap = left.min(cap);
        let mut spec = provider.spec(vec![
            "api".into(),
            endpoint.into(),
            "--hostname".into(),
            provider.host.clone(),
            "--method".into(),
            "GET".into(),
        ]);
        spec.output_limit = output_cap;
        spec.timeout = provider.timeout.min(duration);
        self.requests += 1;
        let output = match provider.runner.run(spec, cancel) {
            Ok(output) => output,
            Err(err) => {
                // The runner shares one capture limit across both pipes. Error
                // bytes are sanitized/redacted and cannot prove raw byte count.
                self.failed_reserved_bytes = self
                    .failed_reserved_bytes
                    .checked_add(output_cap)
                    .ok_or_else(|| {
                        error(
                            GithubErrorKind::Transport,
                            "Failed health capture reservation overflow",
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
                    GithubErrorKind::Transport,
                    "Successful health response accounting overflow",
                )
            })?;
        self.successful_diagnostic_bytes = self
            .successful_diagnostic_bytes
            .checked_add(output.capture.stderr_bytes())
            .ok_or_else(|| {
                error(
                    GithubErrorKind::Transport,
                    "Successful diagnostic capture overflow",
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
                GithubErrorKind::Transport,
                "Health runner exceeded its selected pre-request capture budget",
            ));
        }
        serde_json::from_slice(&output.stdout)
            .map_err(|err| error(GithubErrorKind::Json, err.to_string()))
    }
    pub fn read(
        &mut self,
        provider: &GithubProvider,
        endpoint: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        self.read_capped(provider, endpoint, 4 * 1024 * 1024, cancel)
    }
    pub fn read_document(
        &mut self,
        provider: &GithubProvider,
        endpoint: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        self.read_capped(provider, endpoint, self.limits.max_document_bytes, cancel)
    }
    pub fn observed_through(&self) -> GithubResult<u64> {
        let through = now()?;
        if through < self.observed_from {
            return Err(error(
                GithubErrorKind::Transport,
                "UTC clock moved backwards during observed health read",
            ));
        }
        Ok(through)
    }
}
