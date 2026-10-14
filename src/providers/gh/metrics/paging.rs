use super::*;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
#[path = "pr_checks_capture.rs"]
mod pr_checks_capture;

fn now() -> GithubResult<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_secs())
        .map_err(|_| {
            error(
                GithubErrorKind::Transport,
                "Observed UTC clock is unavailable; no metric timestamp invented",
            )
        })
}
pub(super) struct MetricsBudget {
    limits: MetricsReadLimits,
    started: Instant,
    pub observed_from: u64,
    pub requests: usize,
    pub bytes: usize,
    pub successful_diagnostic_bytes: usize,
    pub failed_reserved_bytes: usize,
    exhausted: bool,
}
impl MetricsBudget {
    pub fn new(limits: &MetricsReadLimits) -> GithubResult<Self> {
        limits.validate()?;
        Ok(Self {
            limits: limits.clone(),
            started: Instant::now(),
            observed_from: now()?,
            requests: 0,
            bytes: 0,
            successful_diagnostic_bytes: 0,
            failed_reserved_bytes: 0,
            exhausted: false,
        })
    }
    pub fn read(
        &mut self,
        provider: &GithubProvider,
        endpoint: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        self.read_capped(provider, endpoint, 4 * 1024 * 1024, cancel)
    }
    fn read_capped(
        &mut self,
        provider: &GithubProvider,
        endpoint: &str,
        response_cap: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
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
        let mut spec = provider.spec(vec![
            "api".into(),
            endpoint.into(),
            "--hostname".into(),
            provider.host.clone(),
            "--method".into(),
            "GET".into(),
        ]);
        spec.output_limit = left.min(response_cap);
        let output_cap = spec.output_limit;
        spec.timeout = provider.timeout.min(time);
        self.requests += 1;
        let output = match provider.runner.run(spec, cancel) {
            Ok(output) => output,
            Err(error_) => {
                // Both pipes share ONE capture cap. Sanitized failure bytes
                // cannot prove raw capture length; reserve the full known cap.
                self.failed_reserved_bytes = self
                    .failed_reserved_bytes
                    .checked_add(output_cap)
                    .ok_or_else(|| {
                        error(
                            GithubErrorKind::Transport,
                            "Metric failed capture reservation overflow",
                        )
                    })?;
                if matches!(
                    error_.kind,
                    FailureKind::OutputLimit | FailureKind::Timeout | FailureKind::Cancelled
                ) {
                    self.exhausted = true;
                }
                return Err(GithubError::from(error_));
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
                "Metric runner violated its pre-request byte cap",
            ));
        }
        serde_json::from_slice(&output.stdout)
            .map_err(|error_| error(GithubErrorKind::Json, error_.to_string()))
    }
    pub fn collect<T: Clone + PartialEq>(
        &mut self,
        provider: &GithubProvider,
        base: &str,
        mut parse: impl FnMut(&Value) -> GithubResult<(String, T, Vec<String>)>,
        cancel: &CancellationToken,
    ) -> GithubResult<(Vec<T>, MetricsReadCoverage)> {
        let before_bytes = self.bytes;
        let mut values = vec![];
        let mut seen: BTreeMap<String, T> = BTreeMap::new();
        let mut coverage = MetricsReadCoverage {
            enumeration_complete: false,
            metric_evidence_complete: true,
            complete_empty: false,
            requested_pages: 0,
            fetched_pages: 0,
            fetched_rows: 0,
            admitted_rows: 0,
            excluded_rows: 0,
            response_bytes: 0,
            reasons: vec![],
            status: MetricsReadStatus::Partial,
        };
        for page in 1..=self.limits.max_pages {
            if coverage.fetched_rows.saturating_add(self.limits.page_size) > self.limits.max_rows {
                coverage.reasons.push(
                    "Fixed-page row budget reached before requesting another metric page".into(),
                );
                break;
            }
            let separator = if base.contains('?') { "&" } else { "?" };
            let endpoint = format!(
                "{base}{separator}per_page={}&page={page}",
                self.limits.page_size
            );
            coverage.requested_pages += 1;
            let value = match self.read(provider, &endpoint, cancel) {
                Ok(value) => value,
                Err(error_)
                    if matches!(
                        error_.kind,
                        GithubErrorKind::Authentication
                            | GithubErrorKind::IdentityChanged
                            | GithubErrorKind::Permission
                            | GithubErrorKind::NotFound
                    ) =>
                {
                    return Err(error_)
                }
                Err(error_) => {
                    coverage.metric_evidence_complete = false;
                    coverage.reasons.push(error_.message.clone());
                    coverage.status = MetricsReadStatus::Unavailable(error_);
                    break;
                }
            };
            let rows = value.as_array().ok_or_else(|| {
                error(
                    GithubErrorKind::Json,
                    "Metric collection is not an actual scoped REST page",
                )
            })?;
            if rows.len() > self.limits.page_size
                || rows.len() > self.limits.max_rows.saturating_sub(coverage.fetched_rows)
            {
                return Err(error(
                    GithubErrorKind::Json,
                    "Metric endpoint exceeded its requested page/raw-row budget",
                ));
            }
            coverage.fetched_pages += 1;
            coverage.fetched_rows += rows.len();
            for row in rows {
                if serde_json::to_vec(row)
                    .map_err(|error_| error(GithubErrorKind::Json, error_.to_string()))?
                    .len()
                    > self.limits.max_document_bytes
                {
                    coverage.excluded_rows += 1;
                    coverage.metric_evidence_complete = false;
                    coverage.reasons.push(
                        "Observed metric row exceeds selected document bytes; not admitted".into(),
                    );
                    continue;
                }
                let (key, item, reasons) = parse(row)?;
                if !reasons.is_empty() {
                    coverage.metric_evidence_complete = false;
                    coverage.reasons.extend(reasons);
                }
                if let Some(old) = seen.get(&key) {
                    if old != &item {
                        return Err(error(GithubErrorKind::IdentityChanged,"Repeated metric identity changed while paging; no arbitrary body/state is selected"));
                    }
                    continue;
                }
                seen.insert(key, item.clone());
                values.push(item);
            }
            if rows.len() < self.limits.page_size {
                coverage.enumeration_complete = true;
                break;
            }
        }
        coverage.response_bytes = self.bytes - before_bytes;
        coverage.admitted_rows = values.len();
        if !coverage.enumeration_complete {
            coverage.metric_evidence_complete = false;
            coverage.reasons.push(
                "Current metric API collection was not exhausted; unseen rows are unknown".into(),
            );
        }
        coverage.complete_empty = coverage.enumeration_complete
            && coverage.fetched_rows == 0
            && coverage.excluded_rows == 0;
        coverage.reasons.sort();
        coverage.reasons.dedup();
        if coverage.enumeration_complete && coverage.metric_evidence_complete {
            coverage.status = MetricsReadStatus::Complete;
        } else if coverage.fetched_pages > 0 {
            coverage.status = MetricsReadStatus::Partial;
        }
        Ok((values, coverage))
    }
    pub fn read_document(
        &mut self,
        provider: &GithubProvider,
        endpoint: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        let value = self.read_capped(provider, endpoint, self.limits.max_document_bytes, cancel)?;
        if serde_json::to_vec(&value)
            .map_err(|error_| error(GithubErrorKind::Json, error_.to_string()))?
            .len()
            > self.limits.max_document_bytes
        {
            return Err(error(
                GithubErrorKind::Json,
                "Canonical metric proof exceeds selected document bytes; not admitted",
            ));
        }
        Ok(value)
    }
    pub fn observed_through(&self) -> GithubResult<u64> {
        let through = now()?;
        if through < self.observed_from {
            return Err(error(
                GithubErrorKind::Transport,
                "UTC clock moved backwards during metric observation; no interval invented",
            ));
        }
        Ok(through)
    }
}
pub(super) fn scope(
    provider: &GithubProvider,
    identity: &HostIdentity,
    name: &str,
) -> GithubResult<()> {
    if identity.host != provider.host || identity.account_id.is_none_or(|id| id == 0) {
        return Err(error(
            GithubErrorKind::IdentityChanged,
            "Metric reads require exact selected host and positive current numeric account",
        ));
    }
    validate_name(&identity.login)?;
    validate_repo(name)
}
pub(super) fn require_same_repo(
    original: &MetricsRepository,
    fresh: &MetricsRepository,
) -> GithubResult<()> {
    if original != fresh {
        return Err(error(
            GithubErrorKind::IdentityChanged,
            "Metric repository canonical ID/node/name changed; old cohort cannot be retargeted",
        ));
    }
    Ok(())
}
pub(super) fn require_same_parent(
    original: &MetricsPrObservation,
    fresh: &MetricsPrObservation,
) -> GithubResult<()> {
    if original.repository != fresh.repository
        || original.id != fresh.id
        || original.node_id != fresh.node_id
        || original.number != fresh.number
    {
        return Err(error(
            GithubErrorKind::IdentityChanged,
            "Metric event parent was replaced; old number/node cannot authorize another PR",
        ));
    }
    if original.head_sha != fresh.head_sha
        || original.state != fresh.state
        || original.draft != fresh.draft
        || original.created_at != fresh.created_at
        || original.merged_at != fresh.merged_at
        || original.closed_at != fresh.closed_at
        || original.merged != fresh.merged
        || original.observed_updated_at != fresh.observed_updated_at
        || original.requested_users != fresh.requested_users
        || original.requested_teams != fresh.requested_teams
    {
        return Err(error(GithubErrorKind::StaleHead,"Metric parent freshness changed during collection; historical completeness is not inferred from this reread"));
    }
    Ok(())
}
