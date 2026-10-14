use super::*;

pub(super) struct CatalogueBudget {
    limits: RepositoryCatalogueLimits,
    started: Instant,
    /// None for discovery; current-access sessions bind their ORIGINAL token.
    pub(super) original_cancellation: Option<CancellationToken>,
    pub observed_from: u64,
    pub accounting: RepositoryCatalogueAccounting,
}
impl CatalogueBudget {
    pub fn new(limits: &RepositoryCatalogueLimits) -> GithubResult<Self> {
        Ok(Self {
            limits: limits.clone(),
            started: Instant::now(),
            original_cancellation: None,
            observed_from: utc_now()?,
            accounting: RepositoryCatalogueAccounting::default(),
        })
    }
    pub fn remaining_bytes(&self) -> usize {
        self.limits
            .max_response_bytes
            .saturating_sub(self.accounting.charged_bytes())
    }
    pub fn deadline(&self) -> Instant {
        self.started
            .checked_add(self.limits.max_duration)
            .expect("validated bounded catalogue duration")
    }
    pub fn check(&self, cancel: &CancellationToken) -> GithubResult<()> {
        if cancel.is_cancelled()
            || self
                .original_cancellation
                .as_ref()
                .is_some_and(CancellationToken::is_cancelled)
            || self.started.elapsed() >= self.limits.max_duration
        {
            Err(error(GithubErrorKind::Transport,"Original catalogue deadline/cancellation expired; no lease, reset or automatic resend"))
        } else {
            Ok(())
        }
    }
    pub fn command(
        &mut self,
        provider: &GithubProvider,
        args: Vec<String>,
        response_cap: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<u8>> {
        self.check(cancel)?;
        let cap = self.remaining_bytes().min(response_cap);
        if cap == 0 {
            return Err(error(
                GithubErrorKind::Transport,
                "Original catalogue whole capture budget exhausted before another gh command",
            ));
        }
        let mut spec = provider.spec(args);
        spec.original_read_cancellation = self.original_cancellation.clone();
        let cap = cap.min(spec.output_limit);
        spec.output_limit = cap;
        spec.timeout = spec.timeout.min(provider.timeout).min(
            self.limits
                .max_duration
                .saturating_sub(self.started.elapsed()),
        );
        if cap == 0 || spec.timeout.is_zero() {
            return Err(error(
                GithubErrorKind::Transport,
                "Original read window exhausted before another catalogue child",
            ));
        }
        self.accounting.requests += 1;
        let output = match provider.runner.run(spec, cancel) {
            Ok(output) => output,
            Err(failure) => {
                self.accounting.failed_reserved_bytes = self
                    .accounting
                    .failed_reserved_bytes
                    .checked_add(cap)
                    .ok_or_else(|| {
                        error(
                            GithubErrorKind::Transport,
                            "Catalogue failed capture reservation overflow",
                        )
                    })?;
                return Err(failure.into());
            }
        };
        self.accounting.stdout_bytes = self
            .accounting
            .stdout_bytes
            .checked_add(output.capture.stdout_bytes())
            .ok_or_else(|| {
                error(
                    GithubErrorKind::Transport,
                    "Catalogue stdout accounting overflow",
                )
            })?;
        self.accounting.diagnostic_bytes = self
            .accounting
            .diagnostic_bytes
            .checked_add(output.capture.stderr_bytes())
            .ok_or_else(|| {
                error(
                    GithubErrorKind::Transport,
                    "Catalogue diagnostic accounting overflow",
                )
            })?;
        if output.capture.stdout_bytes() != output.stdout.len()
            || output.capture.total_bytes() > cap
            || self.accounting.charged_bytes() > self.limits.max_response_bytes
        {
            return Err(error(
                GithubErrorKind::Transport,
                "Catalogue raw shared capture counters disagree; no invented byte coverage",
            ));
        }
        self.check(cancel)?;
        Ok(output.stdout)
    }
    pub fn json(
        &mut self,
        provider: &GithubProvider,
        endpoint: &str,
        cap: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        let raw = self.command(
            provider,
            vec![
                "api".into(),
                endpoint.into(),
                "--hostname".into(),
                provider.host.clone(),
                "--method".into(),
                "GET".into(),
            ],
            cap,
            cancel,
        )?;
        serde_json::from_slice(&raw).map_err(|_| {
            error(
                GithubErrorKind::Json,
                "Catalogue endpoint did not return bounded valid JSON",
            )
        })
    }
}

#[cfg(test)]
#[path = "read_limits_deadline_v1_tests.rs"]
mod read_limits_deadline_v1_tests;
