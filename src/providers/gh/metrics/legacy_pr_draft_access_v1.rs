//! SOURCE ONLY: exact current read admission for local legacy PR draft OPEN.
//! Undeclared. No history, catalogue, publication or permission-to-write proof.
use super::*;
use std::{fmt, sync::Arc, time::Instant};

const OUTER_MAX: Duration = Duration::from_secs(30);
const ADMISSION_MAX_AGE: Duration = Duration::from_secs(5);
const GROUP_REQUESTS: usize = 5;

/// Scheduling budget only, not authority. No Clone/serde; every pre/final read
/// group must use this SAME original clock/issuer, including after local errors.
pub struct LegacyPrDraftReadWindow {
    started: Instant,
    duration: Duration,
    issuer: Arc<()>,
}
impl fmt::Debug for LegacyPrDraftReadWindow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LegacyPrDraftReadWindow(<bounded original clock>)")
    }
}
impl LegacyPrDraftReadWindow {
    pub fn new(duration: Duration) -> GithubResult<Self> {
        if duration.is_zero() || duration > OUTER_MAX {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Legacy draft outer read window must be positive and at most thirty seconds",
            ));
        }
        Ok(Self {
            started: Instant::now(),
            duration,
            issuer: Arc::new(()),
        })
    }
    pub fn remaining(&self, cancel: &CancellationToken) -> GithubResult<Duration> {
        let remaining = self.duration.saturating_sub(self.started.elapsed());
        if remaining.is_zero() || cancel.is_cancelled() {
            return Err(error(GithubErrorKind::Transport,"Original legacy draft read window expired/cancelled; private admission unavailable"));
        }
        Ok(remaining)
    }
    /// Scheduling cutoff only; never a resource/principal authority token.
    pub fn deadline(&self) -> Instant {
        self.started + self.duration
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LegacyPrDraftAccessLimits {
    pub max_document_bytes: usize,
    pub max_response_bytes: usize,
    pub max_duration: Duration,
}
impl Default for LegacyPrDraftAccessLimits {
    fn default() -> Self {
        Self {
            max_document_bytes: 128 * 1024,
            max_response_bytes: 1024 * 1024,
            max_duration: Duration::from_secs(10),
        }
    }
}
impl LegacyPrDraftAccessLimits {
    fn selected(
        &self,
        window: &LegacyPrDraftReadWindow,
        cancel: &CancellationToken,
    ) -> GithubResult<MetricsReadLimits> {
        if !(1..=128 * 1024).contains(&self.max_document_bytes)
            || !(1..=1024 * 1024).contains(&self.max_response_bytes)
            || self.max_duration.is_zero()
            || self.max_duration > Duration::from_secs(10)
            || self
                .max_document_bytes
                .checked_mul(GROUP_REQUESTS)
                .is_none_or(|reserved| reserved > self.max_response_bytes)
        {
            return Err(error(GithubErrorKind::InvalidTarget,"Legacy draft access requires bounded canonical captures and space reserved for all five original closing reads"));
        }
        Ok(MetricsReadLimits {
            max_pages: 1,
            page_size: 1,
            max_rows: 1,
            max_document_bytes: self.max_document_bytes,
            max_response_bytes: self.max_response_bytes,
            max_duration: self.max_duration.min(window.remaining(cancel)?),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LegacyPrDraftAccessAccounting {
    requests: usize,
    stdout: usize,
    diagnostics: usize,
    failed_reserved: usize,
}
impl LegacyPrDraftAccessAccounting {
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

#[derive(Clone, PartialEq, Eq)]
struct Original {
    host: String,
    repository: String,
    node: String,
    number: u64,
    head: String,
    base: String,
    head_ref: String,
    base_ref: String,
    state: String,
    draft: bool,
    merged: bool,
}
impl Original {
    fn new(
        provider: &GithubProvider,
        identity: &HostIdentity,
        original: &PrSummary,
    ) -> GithubResult<Self> {
        scope(provider, identity, &original.pr.repository)?;
        if original.pr.host != provider.host
            || original.pr.number == 0
            || original.pr.number > i64::MAX as u64
            || !valid_opaque(&original.pr.id)
            || !crate::search::model::valid_sha(&original.head_sha)
            || !crate::search::model::valid_sha(&original.base_sha)
            || !matches!(original.pr.state.as_str(), "open" | "closed")
            || [&original.head_ref, &original.base_ref].iter().any(|name| {
                name.is_empty() || name.len() > 4096 || name.chars().any(char::is_control)
            })
        {
            return Err(error(GithubErrorKind::InvalidTarget,"Original legacy draft target lacks exact canonical PR node/number/full head/base/ref/lifecycle identity"));
        }
        Ok(Self {
            host: provider.host.clone(),
            repository: original.pr.repository.to_ascii_lowercase(),
            node: original.pr.id.clone(),
            number: original.pr.number,
            head: original.head_sha.clone(),
            base: original.base_sha.clone(),
            head_ref: original.head_ref.clone(),
            base_ref: original.base_ref.clone(),
            state: original.pr.state.clone(),
            draft: original.pr.draft,
            merged: original.merged,
        })
    }
}

/// Actual bounded original read proof. No public raw/DTO/bool constructor or
/// serde. First actual repository/PR establishes real IDs; revalidation binds
/// those IDs, source and original window. Never an SQL/journal/write permit.
pub struct LegacyPrDraftAccess {
    producer: CommandRunner,
    window: Arc<()>,
    verified_at: Instant,
    identity: HostIdentity,
    original: Original,
    repository: MetricsRepository,
    pr_database_id: u64,
    /// None means ACTUAL explicit deleted/null head repository, never omitted.
    head_repository: Option<MetricsRepository>,
    limits: LegacyPrDraftAccessLimits,
    accounting: LegacyPrDraftAccessAccounting,
}
impl fmt::Debug for LegacyPrDraftAccess {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LegacyPrDraftAccess(<sealed current original-parent read>)")
    }
}
impl LegacyPrDraftAccess {
    pub fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub fn repository(&self) -> &MetricsRepository {
        &self.repository
    }
    pub fn pr_database_id(&self) -> u64 {
        self.pr_database_id
    }
    pub fn head_repository(&self) -> Option<&MetricsRepository> {
        self.head_repository.as_ref()
    }
    pub fn accounting(&self) -> &LegacyPrDraftAccessAccounting {
        &self.accounting
    }
    pub fn validate_for(
        &self,
        provider: &GithubProvider,
        identity: &HostIdentity,
        original: &PrSummary,
        window: &LegacyPrDraftReadWindow,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        window.remaining(cancel)?;
        if !Arc::ptr_eq(&self.window, &window.issuer)
            || !provider.runner.same_instance(&self.producer)
            || self.verified_at.elapsed() > ADMISSION_MAX_AGE
            || !same_principal(&self.identity, identity)
            || self.original != Original::new(provider, identity, original)?
        {
            return Err(error(GithubErrorKind::IdentityChanged,"Legacy draft access is expired or belongs to another producer/window/principal/original parent"));
        }
        Ok(())
    }
}
fn same_principal(left: &HostIdentity, right: &HostIdentity) -> bool {
    left.host == right.host
        && left.account_id.is_some_and(|id| id > 0)
        && left.account_id == right.account_id
}
fn positive_bounded(value: &Value, field: &str) -> GithubResult<u64> {
    let id = positive(value, field)?;
    if id > i64::MAX as u64 {
        return Err(error(
            GithubErrorKind::Json,
            "Canonical legacy draft database identity exceeds positive storage range",
        ));
    }
    Ok(id)
}
fn exact_api_url(host: &str, path: &str, raw: &Value) -> bool {
    let residency = host
        .strip_suffix(".ghe.com")
        .is_some_and(|tenant| !tenant.is_empty() && !tenant.contains('.'));
    let api_host = if host == "github.com" {
        "api.github.com".into()
    } else if residency {
        format!("api.{host}")
    } else {
        host.to_owned()
    };
    raw.as_str().is_some_and(|url| {
        url.eq_ignore_ascii_case(&format!("https://{api_host}/{path}"))
            || host != "github.com"
                && !residency
                && url.eq_ignore_ascii_case(&format!("https://{api_host}/api/v3/{path}"))
    })
}
fn repo_observation(
    provider: &GithubProvider,
    name: &str,
    raw: &Value,
) -> GithubResult<MetricsRepository> {
    let repo = repository(&provider.host, name, raw)?;
    positive_bounded(raw, "id")?;
    if !exact_api_url(&provider.host, &format!("repos/{name}"), &raw["url"])
        || !raw["html_url"].as_str().is_some_and(|url| {
            url.eq_ignore_ascii_case(&format!("https://{}/{name}", provider.host))
        })
    {
        return Err(error(
            GithubErrorKind::IdentityChanged,
            "Actual repository source does not match the original host/name",
        ));
    }
    if raw["permissions"]["pull"].as_bool() != Some(true) {
        return Err(error(
            GithubErrorKind::Permission,
            "Actual current repository read permission unavailable; no private draft admission",
        ));
    }
    Ok(repo)
}
fn user_observation(
    provider: &GithubProvider,
    expected: &HostIdentity,
    raw: &Value,
) -> GithubResult<HostIdentity> {
    let id = positive_bounded(raw, "id")?;
    let login = text(raw, "login");
    validate_name(&login)?;
    if raw["type"].as_str() != Some("User") {
        return Err(error(
            GithubErrorKind::Unsupported,
            "Current principal type is missing/bot/unsupported; no user identity fabricated",
        ));
    }
    let current = HostIdentity {
        host: provider.host.clone(),
        login,
        account_id: Some(id),
        gh_version: expected.gh_version.clone(),
    };
    if !same_principal(expected, &current) {
        return Err(error(
            GithubErrorKind::IdentityChanged,
            "Original positive numeric principal changed during canonical draft access",
        ));
    }
    // CLI version is carried caller metadata, NOT observed by this user GET.
    Ok(current)
}
fn parent_observation(
    provider: &GithubProvider,
    original: &Original,
    repo: &MetricsRepository,
    raw: &Value,
) -> GithubResult<(u64, Option<MetricsRepository>)> {
    let id = positive_bounded(raw, "id")?;
    if positive_bounded(raw, "number")? != original.number
        || node(raw, "node_id")? != original.node
        || raw["head"]["sha"].as_str() != Some(original.head.as_str())
        || raw["base"]["sha"].as_str() != Some(original.base.as_str())
        || raw["head"]["ref"].as_str() != Some(original.head_ref.as_str())
        || raw["base"]["ref"].as_str() != Some(original.base_ref.as_str())
        || raw["state"].as_str() != Some(original.state.as_str())
        || raw["draft"].as_bool() != Some(original.draft)
        || raw["merged"].as_bool() != Some(original.merged)
        || !exact_api_url(
            &provider.host,
            &format!("repos/{}/pulls/{}", original.repository, original.number),
            &raw["url"],
        )
        || !raw["html_url"].as_str().is_some_and(|url| {
            url.eq_ignore_ascii_case(&format!(
                "https://{}/{}/pull/{}",
                provider.host, original.repository, original.number
            ))
        })
    {
        return Err(error(GithubErrorKind::StaleHead,"Actual original PR database/node/number/source/full head/base/ref/lifecycle changed or is unknown"));
    }
    let base = repository(&provider.host, &original.repository, &raw["base"]["repo"])?;
    require_same_repo(repo, &base)?;
    let head = match raw["head"].get("repo") {
        Some(Value::Null) => None,
        Some(raw) if raw.is_object() => {
            let name = raw["full_name"].as_str().ok_or_else(|| {
                error(
                    GithubErrorKind::Json,
                    "Actual head repository name unavailable; no source substituted",
                )
            })?;
            Some(repository(&provider.host, name, raw)?)
        }
        _ => return Err(error(
            GithubErrorKind::Unsupported,
            "Actual head repository source omitted/unknown; no deleted/current source fabricated",
        )),
    };
    Ok((id, head))
}

impl GithubProvider {
    /// Exactly five bounded API GETs on success, no --version/list/auth client:
    /// user/repo/PR/repo/user. Actual first header supplies real canonical IDs.
    pub fn legacy_pr_draft_access_v1(
        &self,
        identity: &HostIdentity,
        original: &PrSummary,
        limits: &LegacyPrDraftAccessLimits,
        window: &LegacyPrDraftReadWindow,
        cancel: &CancellationToken,
    ) -> GithubResult<LegacyPrDraftAccess> {
        let original = Original::new(self, identity, original)?;
        self.read_legacy_draft_access_v1(identity, original, limits, window, None, cancel)
    }
    /// Same five GETs, original issuer/clock/capture policy, actual observed IDs
    /// and source. Call even after local corruption where budget safely permits;
    /// a failed final access proof hides the prepared form AND local diagnostic.
    pub fn revalidate_legacy_pr_draft_access_v1(
        &self,
        prior: &LegacyPrDraftAccess,
        identity: &HostIdentity,
        window: &LegacyPrDraftReadWindow,
        cancel: &CancellationToken,
    ) -> GithubResult<LegacyPrDraftAccess> {
        window.remaining(cancel)?;
        if !Arc::ptr_eq(&prior.window, &window.issuer)
            || self.host != prior.original.host
            || !self.runner.same_instance(&prior.producer)
            || !same_principal(&prior.identity, identity)
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Original draft access cannot be rebased onto a new clock/provider/principal",
            ));
        }
        self.read_legacy_draft_access_v1(
            identity,
            prior.original.clone(),
            &prior.limits,
            window,
            Some(prior),
            cancel,
        )
    }
    fn read_legacy_draft_access_v1(
        &self,
        identity: &HostIdentity,
        original: Original,
        limits: &LegacyPrDraftAccessLimits,
        window: &LegacyPrDraftReadWindow,
        prior: Option<&LegacyPrDraftAccess>,
        cancel: &CancellationToken,
    ) -> GithubResult<LegacyPrDraftAccess> {
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
        let endpoint = format!("repos/{}", original.repository);
        let repo = repo_observation(self, &original.repository, &read(&endpoint)?)?;
        if let Some(prior) = prior {
            require_same_repo(&prior.repository, &repo)?;
        }
        let (pr_database_id, head_repository) = parent_observation(
            self,
            &original,
            &repo,
            &read(&format!("{endpoint}/pulls/{}", original.number))?,
        )?;
        if prior.is_some_and(|prior| {
            prior.pr_database_id != pr_database_id || prior.head_repository != head_repository
        }) {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Observed original PR database/head-source identity changed; no draft retarget",
            ));
        }
        let closing = repo_observation(self, &original.repository, &read(&endpoint)?)?;
        require_same_repo(&repo, &closing)?;
        let identity = user_observation(self, &first, &read("user")?)?;
        window.remaining(cancel)?;
        let accounting = LegacyPrDraftAccessAccounting {
            requests: budget.requests,
            stdout: budget.bytes,
            diagnostics: budget.successful_diagnostic_bytes,
            failed_reserved: budget.failed_reserved_bytes,
        };
        if accounting.requests != GROUP_REQUESTS
            || accounting.charged_bytes() > limits.max_response_bytes
        {
            return Err(error(
                GithubErrorKind::Transport,
                "Original canonical draft closing/capture budget incomplete; no admission",
            ));
        }
        Ok(LegacyPrDraftAccess {
            producer: self.runner.clone(),
            window: window.issuer.clone(),
            verified_at: Instant::now(),
            identity,
            original,
            repository: closing,
            pr_database_id,
            head_repository,
            limits: *limits,
            accounting,
        })
    }
}

#[cfg(test)]
#[path = "legacy_pr_draft_access_v1_tests.rs"]
mod tests;
