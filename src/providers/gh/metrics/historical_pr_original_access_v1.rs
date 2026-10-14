//! SOURCE ONLY: current access to an original journal's SAME canonical parent.
//! Historical observed head is NOT required to equal current head. No writable
//! proof conversion, draft adoption, original receipt/body restoration or SQL.
use super::legacy_pr_draft_access_v1::{
    exact_api_url, positive_bounded, repo_observation, same_principal, user_observation,
    LegacyPrDraftAccessLimits, LegacyPrDraftReadWindow,
};
use super::*;
use crate::pr_write_journal::PrWriteTarget;
use std::{fmt, sync::Arc, time::Instant};

const MAX_AGE: Duration = Duration::from_secs(5);
const CALLS: usize = 5;

/// Actual current facts, never historical DB identity or a mutation condition.
/// Values are only emitted by the opaque read below; no public constructor.
#[derive(Clone, PartialEq, Eq)]
pub struct HistoricalCurrentPrSource {
    database_id: u64,
    node: String,
    number: u64,
    head: String,
    base: String,
    head_ref: String,
    base_ref: String,
    state: String,
    draft: bool,
    merged: bool,
    head_repository: Option<MetricsRepository>,
}
impl fmt::Debug for HistoricalCurrentPrSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("HistoricalCurrentPrSource(<observed current source; not receipt>)")
    }
}
impl HistoricalCurrentPrSource {
    pub fn database_id(&self) -> u64 {
        self.database_id
    }
    pub fn node_id(&self) -> &str {
        &self.node
    }
    pub fn number(&self) -> u64 {
        self.number
    }
    pub fn head_sha(&self) -> &str {
        &self.head
    }
    pub fn base_sha(&self) -> &str {
        &self.base
    }
    pub fn head_ref(&self) -> &str {
        &self.head_ref
    }
    pub fn base_ref(&self) -> &str {
        &self.base_ref
    }
    pub fn state(&self) -> &str {
        &self.state
    }
    pub fn draft(&self) -> bool {
        self.draft
    }
    pub fn merged(&self) -> bool {
        self.merged
    }
    pub fn head_repository(&self) -> Option<&MetricsRepository> {
        self.head_repository.as_ref()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HistoricalPrOriginalAccounting {
    requests: usize,
    stdout: usize,
    diagnostics: usize,
    failed_reserved: usize,
}
impl HistoricalPrOriginalAccounting {
    pub fn requests(&self) -> usize {
        self.requests
    }
    pub fn successful_stdout_bytes(&self) -> usize {
        self.stdout
    }
    pub fn successful_diagnostic_bytes(&self) -> usize {
        self.diagnostics
    }
    pub fn failed_reserved_bytes(&self) -> usize {
        self.failed_reserved
    }
    pub fn charged_bytes(&self) -> usize {
        self.stdout
            .saturating_add(self.diagnostics)
            .saturating_add(self.failed_reserved)
    }
}
/// Purpose-specific, private-constructor/nonSerde proof. No conversion to
/// LegacyPrDraftAccess, public raw/boolean constructor or publication permit.
pub struct HistoricalPrOriginalAccess {
    producer: CommandRunner,
    window: Arc<()>,
    verified_at: Instant,
    identity: HostIdentity,
    target: PrWriteTarget,
    repository: MetricsRepository,
    current: HistoricalCurrentPrSource,
    limits: LegacyPrDraftAccessLimits,
    accounting: HistoricalPrOriginalAccounting,
}
impl fmt::Debug for HistoricalPrOriginalAccess {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(
            "HistoricalPrOriginalAccess(<current read-only parent access; recorded head separate>)",
        )
    }
}
impl HistoricalPrOriginalAccess {
    pub fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub fn target(&self) -> &PrWriteTarget {
        &self.target
    }
    pub fn repository(&self) -> &MetricsRepository {
        &self.repository
    }
    pub fn current_source(&self) -> &HistoricalCurrentPrSource {
        &self.current
    }
    pub fn accounting(&self) -> &HistoricalPrOriginalAccounting {
        &self.accounting
    }
    pub fn validate_for(
        &self,
        provider: &GithubProvider,
        identity: &HostIdentity,
        target: &PrWriteTarget,
        window: &LegacyPrDraftReadWindow,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        validate_target(provider, identity, target)?;
        window.remaining(cancel)?;
        if self.target != *target
            || !Arc::ptr_eq(&self.window, &window.issuer)
            || !provider.runner.same_instance(&self.producer)
            || self.verified_at.elapsed() > MAX_AGE
            || !same_principal(&self.identity, identity)
        {
            return Err(error(GithubErrorKind::IdentityChanged,"Historical original access belongs to another target/producer/window/principal or expired"));
        }
        Ok(())
    }
}
/// Borrowed genuine observed context for a Root-owned bounded SQL/cache/journal
/// guard. Constructed only after actual canonical PRGET3, BEFORE repoGET4 and
/// userGET5. It is neither a public boolean authority nor a writable proof.
pub struct HistoricalOriginalClosingContext<'a> {
    identity: &'a HostIdentity,
    target: &'a PrWriteTarget,
    repository: &'a MetricsRepository,
    current: &'a HistoricalCurrentPrSource,
}
impl<'a> HistoricalOriginalClosingContext<'a> {
    pub fn identity(&self) -> &HostIdentity {
        self.identity
    }
    pub fn target(&self) -> &PrWriteTarget {
        self.target
    }
    pub fn repository(&self) -> &MetricsRepository {
        self.repository
    }
    pub fn current_source(&self) -> &HistoricalCurrentPrSource {
        self.current
    }
}
struct HistoricalReadRequest<'a> {
    identity: &'a HostIdentity,
    target: &'a PrWriteTarget,
    limits: &'a LegacyPrDraftAccessLimits,
    window: &'a LegacyPrDraftReadWindow,
    prior: Option<&'a HistoricalPrOriginalAccess>,
    cancel: &'a CancellationToken,
}
fn validate_target(
    provider: &GithubProvider,
    identity: &HostIdentity,
    target: &PrWriteTarget,
) -> GithubResult<()> {
    scope(provider, identity, target.repository())?;
    let actual =
        crate::storage::Scope::github(&identity.host, identity.account_id).map_err(|_| {
            error(
                GithubErrorKind::InvalidTarget,
                "Historical original requires positive numeric scope",
            )
        })?;
    if &actual != target.scope()
        || target.scope().host != provider.host
        || !valid_opaque(target.node_id())
        || target.number() == 0
        || target.number() > i64::MAX as u64
        || target.repository().len() > 4096
        || target.scope().host.len() > 253
    {
        return Err(error(GithubErrorKind::InvalidTarget,"Historical original target lacks the same positive canonical scope/node/repository/number"));
    }
    Ok(())
}
fn current_parent(
    provider: &GithubProvider,
    target: &PrWriteTarget,
    repo: &MetricsRepository,
    raw: &Value,
) -> GithubResult<HistoricalCurrentPrSource> {
    let id = positive_bounded(raw, "id")?;
    let node_id = node(raw, "node_id")?;
    let number = positive_bounded(raw, "number")?;
    if node_id != target.node_id()
        || number != target.number()
        || !exact_api_url(
            &provider.host,
            &format!("repos/{}/pulls/{}", target.repository(), target.number()),
            &raw["url"],
        )
        || !raw["html_url"].as_str().is_some_and(|url| {
            url.eq_ignore_ascii_case(&format!(
                "https://{}/{}/pull/{}",
                provider.host,
                target.repository(),
                target.number()
            ))
        })
    {
        return Err(error(
            GithubErrorKind::IdentityChanged,
            "Current resource is not the original opaque PR node/repository/number",
        ));
    }
    let sha = |side: &str| {
        raw[side]["sha"]
            .as_str()
            .filter(|value| crate::search::model::valid_sha(value))
            .map(str::to_owned)
            .ok_or_else(|| {
                error(
                    GithubErrorKind::Json,
                    "Current original-parent source lacks actual full Git identity",
                )
            })
    };
    let reference = |side: &str| {
        raw[side]["ref"]
            .as_str()
            .filter(|value| {
                !value.is_empty() && value.len() <= 4096 && !value.chars().any(char::is_control)
            })
            .map(str::to_owned)
            .ok_or_else(|| {
                error(
                    GithubErrorKind::Json,
                    "Current original-parent source ref unavailable",
                )
            })
    };
    let state = raw["state"]
        .as_str()
        .filter(|value| matches!(*value, "open" | "closed"))
        .ok_or_else(|| {
            error(
                GithubErrorKind::Unsupported,
                "Current lifecycle state unavailable; no default inferred",
            )
        })?;
    let draft = raw["draft"].as_bool().ok_or_else(|| {
        error(
            GithubErrorKind::Unsupported,
            "Current draft state unavailable",
        )
    })?;
    let merged = raw["merged"].as_bool().ok_or_else(|| {
        error(
            GithubErrorKind::Unsupported,
            "Current merge state unavailable",
        )
    })?;
    if merged && state != "closed" {
        return Err(error(
            GithubErrorKind::Json,
            "Current lifecycle observations contradict",
        ));
    }
    let base = repository(&provider.host, target.repository(), &raw["base"]["repo"])?;
    require_same_repo(repo, &base)?;
    let head_repository = match raw["head"].get("repo") {
        Some(Value::Null) => None,
        Some(raw) if raw.is_object() => {
            let name = raw["full_name"]
                .as_str()
                .ok_or_else(|| error(GithubErrorKind::Json, "Current head source name missing"))?;
            let observed = repository(&provider.host, name, raw)?;
            positive_bounded(raw, "id")?;
            Some(observed)
        }
        _ => {
            return Err(error(
                GithubErrorKind::Unsupported,
                "Current head source omitted/unknown; no deleted/source substitution",
            ))
        }
    };
    Ok(HistoricalCurrentPrSource {
        database_id: id,
        node: node_id,
        number,
        head: sha("head")?,
        base: sha("base")?,
        head_ref: reference("head")?,
        base_ref: reference("base")?,
        state: state.into(),
        draft,
        merged,
        head_repository,
    })
}
impl GithubProvider {
    pub fn historical_pr_original_access_v1(
        &self,
        identity: &HostIdentity,
        target: &PrWriteTarget,
        limits: &LegacyPrDraftAccessLimits,
        window: &LegacyPrDraftReadWindow,
        cancel: &CancellationToken,
    ) -> GithubResult<HistoricalPrOriginalAccess> {
        validate_target(self, identity, target)?;
        self.read_historical_original_v1(
            HistoricalReadRequest {
                identity,
                target,
                limits,
                window,
                prior: None,
                cancel,
            },
            |_| Ok(()),
        )
    }
    pub fn revalidate_historical_pr_original_access_v1(
        &self,
        prior: &HistoricalPrOriginalAccess,
        identity: &HostIdentity,
        window: &LegacyPrDraftReadWindow,
        cancel: &CancellationToken,
    ) -> GithubResult<HistoricalPrOriginalAccess> {
        prior.validate_for(self, identity, &prior.target, window, cancel)?;
        self.revalidate_historical_pr_original_access_with_guard_v1(
            prior,
            identity,
            window,
            cancel,
            |_| Ok(()),
        )
    }
    /// Native original viewers MUST use this guarded variant for all actual
    /// original/cache/journal recapture; pure compatibility variant above is
    /// resource/principal access only. No private SQL may follow the returned
    /// completion. Hook elapsed time stays inside both original/source clocks.
    pub fn revalidate_historical_pr_original_access_with_guard_v1(
        &self,
        prior: &HistoricalPrOriginalAccess,
        identity: &HostIdentity,
        window: &LegacyPrDraftReadWindow,
        cancel: &CancellationToken,
        closing: impl FnOnce(&HistoricalOriginalClosingContext<'_>) -> GithubResult<()>,
    ) -> GithubResult<HistoricalPrOriginalAccess> {
        prior.validate_for(self, identity, &prior.target, window, cancel)?;
        self.read_historical_original_v1(
            HistoricalReadRequest {
                identity,
                target: &prior.target,
                limits: &prior.limits,
                window,
                prior: Some(prior),
                cancel,
            },
            closing,
        )
    }
    fn read_historical_original_v1(
        &self,
        request: HistoricalReadRequest<'_>,
        guard: impl FnOnce(&HistoricalOriginalClosingContext<'_>) -> GithubResult<()>,
    ) -> GithubResult<HistoricalPrOriginalAccess> {
        let HistoricalReadRequest {
            identity,
            target,
            limits,
            window,
            prior,
            cancel,
        } = request;
        let selected = limits.selected(window, cancel)?;
        let mut budget = MetricsBudget::new(&selected)?;
        let mut read = |endpoint: &str| {
            let remaining = window.remaining(cancel)?;
            let provider = self.clone().with_timeout(self.timeout.min(remaining));
            let value = budget.read_document(&provider, endpoint, cancel)?;
            window.remaining(cancel)?;
            Ok::<_, GithubError>(value)
        };
        let first = user_observation(self, identity, &read("user")?)?;
        let endpoint = format!("repos/{}", target.repository());
        let repo = repo_observation(self, target.repository(), &read(&endpoint)?)?;
        if let Some(prior) = prior {
            require_same_repo(&prior.repository, &repo)?;
        }
        let current = current_parent(
            self,
            target,
            &repo,
            &read(&format!("{endpoint}/pulls/{}", target.number()))?,
        )?;
        if prior.is_some_and(|prior| prior.current != current) {
            return Err(error(GithubErrorKind::StaleHead,"Observed current parent/source moved while reading historical original; history not retargeted"));
        }
        guard(&HistoricalOriginalClosingContext {
            identity: &first,
            target,
            repository: &repo,
            current: &current,
        })
        .map_err(|failure| {
            error(
                failure.kind,
                "Original closing guard unavailable; private history and local diagnostics hidden",
            )
        })?;
        window.remaining(cancel)?;
        let closing = repo_observation(self, target.repository(), &read(&endpoint)?)?;
        require_same_repo(&repo, &closing)?;
        let identity = user_observation(self, &first, &read("user")?)?;
        window.remaining(cancel)?;
        let accounting = HistoricalPrOriginalAccounting {
            requests: budget.requests,
            stdout: budget.bytes,
            diagnostics: budget.successful_diagnostic_bytes,
            failed_reserved: budget.failed_reserved_bytes,
        };
        if accounting.requests != CALLS || accounting.charged_bytes() > limits.max_response_bytes {
            return Err(error(
                GithubErrorKind::Transport,
                "Historical original closing captures incomplete; private admission unavailable",
            ));
        }
        Ok(HistoricalPrOriginalAccess {
            producer: self.runner.clone(),
            window: window.issuer.clone(),
            verified_at: Instant::now(),
            identity,
            target: target.clone(),
            repository: closing,
            current,
            limits: *limits,
            accounting,
        })
    }
}

#[cfg(test)]
#[path = "historical_pr_original_access_v1_tests.rs"]
mod tests;
