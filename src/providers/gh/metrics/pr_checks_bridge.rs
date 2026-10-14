//! Bounded PR CheckRun -> exact Actions attempt, shared with the native viewer.
//! Read observation only. No mutation, permission-to-write or durable authority.
use super::*;
use std::{fmt, sync::Arc};
#[path = "pr_checks_logs.rs"]
mod logs;
pub use logs::*;
#[path = "pr_checks_scoped_v1.rs"]
mod scoped_v1;
use scoped_v1::ScopedPrCheckBindingV1;
#[path = "pr_checks_choices_v1.rs"]
mod choices_v1;
pub use choices_v1::*;
#[path = "pr_check_capture_v2.rs"]
mod capture_v2;
pub use capture_v2::*;
#[path = "pr_check_annotations_v2.rs"]
mod annotations_v2;
#[path = "pr_check_member_read_v2.rs"]
mod member_v2;
pub use annotations_v2::*;
#[path = "pr_check_attempt_advance_v2.rs"]
mod advance_v2;
pub use advance_v2::{PrCheckAttemptAdvanceV2, PrCheckRefreshReadV2};
#[path = "pr_check_selection_core_v2.rs"]
mod selection_core_v2;

#[derive(Clone, Debug)]
pub struct PrCheckRunReadLimits {
    pub max_run_pages: usize,
    pub max_job_pages: usize,
    pub page_size: usize,
    pub max_rows: usize,
    pub max_row_bytes: usize,
    pub max_document_bytes: usize,
    pub max_response_bytes: usize,
    pub max_projected_items: usize,
    pub max_prepared_bytes: usize,
    pub max_duration: Duration,
}
impl Default for PrCheckRunReadLimits {
    fn default() -> Self {
        Self {
            max_run_pages: 2,
            max_job_pages: 10,
            page_size: 100,
            max_rows: 1000,
            max_row_bytes: 16 * 1024,
            max_document_bytes: 128 * 1024,
            max_response_bytes: 2 * 1024 * 1024,
            max_projected_items: 5000,
            max_prepared_bytes: 2 * 1024 * 1024,
            max_duration: Duration::from_secs(30),
        }
    }
}
impl PrCheckRunReadLimits {
    fn selected(&self) -> GithubResult<MetricsReadLimits> {
        if !(1..=10).contains(&self.max_run_pages)
            || !(1..=10).contains(&self.max_job_pages)
            || !(1..=100).contains(&self.page_size)
            || !(1..=1000).contains(&self.max_rows)
            || self.page_size > self.max_rows
            || !(1..=64 * 1024).contains(&self.max_row_bytes)
            || !(1..=128 * 1024).contains(&self.max_document_bytes)
            || self.max_row_bytes > self.max_document_bytes
            || !(1..=2 * 1024 * 1024).contains(&self.max_response_bytes)
            || !(1..=10000).contains(&self.max_projected_items)
            || !(1..=4 * 1024 * 1024).contains(&self.max_prepared_bytes)
            || self.max_duration.is_zero()
            || self.max_duration > Duration::from_secs(30)
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "PR check bridge requires bounded pages, fixed rows, raw captures and one deadline",
            ));
        }
        Ok(MetricsReadLimits {
            max_pages: self.max_run_pages.max(self.max_job_pages),
            page_size: self.page_size,
            max_rows: self.max_rows,
            max_document_bytes: self.max_document_bytes,
            max_response_bytes: self.max_response_bytes,
            max_duration: self.max_duration,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrCheckJobAttemptOrigin {
    JsonObserved,
    ExactEndpointBound,
}
impl PrCheckJobAttemptOrigin {
    pub fn label(self) -> &'static str {
        match self {
            Self::JsonObserved => {
                "Attempt supplied by job JSON and verified against exact endpoint"
            }
            Self::ExactEndpointBound => {
                "Attempt bound to the verified exact-attempt endpoint; JSON omitted it"
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PrCheckBridgePageCoverage {
    pub requested_pages: usize,
    pub fetched_pages: usize,
    pub raw_rows: usize,
    pub admitted_rows: usize,
    pub excluded_rows: usize,
    pub total_matches: Option<u64>,
    pub enumeration_complete: bool,
    pub reasons: Vec<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrCheckBridgeUnavailable {
    NoCorrelatedRunObserved,
    PartialRunCatalogue,
    AmbiguousRunCorrelation,
    NoCorrelatedJobObserved,
    PartialJobCatalogue,
    AmbiguousJobCorrelation,
}
impl PrCheckBridgeUnavailable {
    pub fn label(self) -> &'static str {
        match self {
            Self::NoCorrelatedRunObserved => "No Actions run observed for this actual check suite and head",
            Self::PartialRunCatalogue => "Run catalogue is partial; no exact run correlation observed",
            Self::AmbiguousRunCorrelation => "More than one run claims this check suite; no arbitrary run selected",
            Self::NoCorrelatedJobObserved => "Selected check has no job in the observed current attempt; it may be old or not allocated",
            Self::PartialJobCatalogue => "Jobs are partial; selected check attempt cannot be identified",
            Self::AmbiguousJobCorrelation => "More than one job claims the selected CheckRun; no arbitrary job selected",
        }
    }
}

/// Constructed only after fresh canonical pre/closing reads. Not serializable.
pub struct PrCheckRunObservation {
    identity: HostIdentity,
    original: PrCheckOriginal,
    repository: MetricsRepository,
    pr_database_id: u64,
    check_node: String,
    suite_id: u64,
    link: Option<PrCheckRunLink>,
    unavailable: Option<PrCheckBridgeUnavailable>,
    run_coverage: PrCheckBridgePageCoverage,
    job_coverage: PrCheckBridgePageCoverage,
    limits: PrCheckRunReadLimits,
    requests: usize,
    stdout_bytes: usize,
    diagnostic_bytes: usize,
    failed_reserved_bytes: usize,
    observed_from: u64,
    observed_through: u64,
    producer: CommandRunner,
    run_candidates: Vec<PrCheckRunCandidateV1>,
    selected_run: Option<Arc<PrCheckSelectedRunV1>>,
    choice_unavailable: Option<PrCheckChoiceUnavailableV1>,
}
impl fmt::Debug for PrCheckRunObservation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PrCheckRunObservation(<canonical read observation; not write authority>)")
    }
}
impl PrCheckRunObservation {
    pub fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub fn repository(&self) -> &MetricsRepository {
        &self.repository
    }
    pub fn pr_number(&self) -> u64 {
        self.original.number
    }
    pub fn pr_database_id(&self) -> u64 {
        self.pr_database_id
    }
    pub fn pr_node(&self) -> &str {
        &self.original.pr_node
    }
    pub fn head_sha(&self) -> &str {
        &self.original.head
    }
    pub fn check_id(&self) -> u64 {
        self.original.check_id
    }
    pub fn check_node(&self) -> &str {
        &self.check_node
    }
    pub fn suite_id(&self) -> u64 {
        self.suite_id
    }
    pub fn link(&self) -> Option<&PrCheckRunLink> {
        self.link.as_ref()
    }
    pub fn unavailable(&self) -> Option<PrCheckBridgeUnavailable> {
        self.unavailable
    }
    pub fn run_coverage(&self) -> &PrCheckBridgePageCoverage {
        &self.run_coverage
    }
    pub fn job_coverage(&self) -> &PrCheckBridgePageCoverage {
        &self.job_coverage
    }
    pub fn limits(&self) -> &PrCheckRunReadLimits {
        &self.limits
    }
    pub fn requests(&self) -> usize {
        self.requests
    }
    pub fn successful_stdout_bytes(&self) -> usize {
        self.stdout_bytes
    }
    pub fn successful_diagnostic_bytes(&self) -> usize {
        self.diagnostic_bytes
    }
    pub fn failed_reserved_bytes(&self) -> usize {
        self.failed_reserved_bytes
    }
    pub fn observed_from(&self) -> u64 {
        self.observed_from
    }
    pub fn observed_through(&self) -> u64 {
        self.observed_through
    }
    pub fn matches_original(
        &self,
        identity: &HostIdentity,
        original: &PrSummary,
        check: &Check,
    ) -> bool {
        // This observation already requires a positive REST principal. A login
        // rename is presentation, while a reused login with a different ID must
        // never match. Do not reuse legacy login-sensitive identity heuristics.
        self.identity.host == identity.host
            && self.identity.account_id.is_some_and(|id| id > 0)
            && self.identity.account_id == identity.account_id
            && self.original.matches(original, check)
    }
}

pub struct PrCheckRunLink {
    attempt: Arc<RunAttempt>,
    selected_job: u64,
    sources: BTreeMap<u64, PrCheckJobAttemptOrigin>,
    run_node: String,
}
impl fmt::Debug for PrCheckRunLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PrCheckRunLink(<verified check/suite/run/attempt/job correlation>)")
    }
}
impl PrCheckRunLink {
    pub fn attempt(&self) -> &Arc<RunAttempt> {
        &self.attempt
    }
    pub fn selected_job(&self) -> u64 {
        self.selected_job
    }
    pub fn job_attempt_origin(&self, job: u64) -> Option<PrCheckJobAttemptOrigin> {
        self.sources.get(&job).copied()
    }
}

struct PrCheckOriginal {
    host: String,
    repository: String,
    number: u64,
    pr_node: String,
    head: String,
    base: String,
    head_ref: String,
    base_ref: String,
    state: String,
    draft: bool,
    merged: bool,
    check_id: u64,
}
impl PrCheckOriginal {
    fn new(
        provider: &GithubProvider,
        identity: &HostIdentity,
        original: &PrSummary,
        check: &Check,
    ) -> GithubResult<Self> {
        scope(provider, identity, &original.pr.repository)?;
        if original.pr.host != provider.host
            || identity.account_id.is_some_and(|id| id > i64::MAX as u64)
            || original.pr.number == 0
            || original.pr.number > i64::MAX as u64
            || !valid_opaque(&original.pr.id)
            || !crate::search::model::valid_sha(&original.head_sha)
            || !crate::search::model::valid_sha(&original.base_sha)
            || check.id == 0
            || check.id > i64::MAX as u64
            || check.head_sha != original.head_sha
            || check.kind != "check"
            || !matches!(original.pr.state.as_str(), "open" | "closed")
            || [original.head_ref.as_str(), original.base_ref.as_str()]
                .iter()
                .any(|v| !valid_opaque(v))
        {
            return Err(error(GithubErrorKind::InvalidTarget, "Bridge needs an original positive principal/canonical PR/full head and a genuine CheckRun; status contexts, short SHA and URL guesses are not run identity"));
        }
        Ok(Self {
            host: provider.host.clone(),
            repository: original.pr.repository.to_ascii_lowercase(),
            number: original.pr.number,
            pr_node: original.pr.id.clone(),
            head: original.head_sha.clone(),
            base: original.base_sha.clone(),
            head_ref: original.head_ref.clone(),
            base_ref: original.base_ref.clone(),
            state: original.pr.state.clone(),
            draft: original.pr.draft,
            merged: original.merged,
            check_id: check.id,
        })
    }
    fn matches(&self, original: &PrSummary, check: &Check) -> bool {
        self.host == original.pr.host
            && self
                .repository
                .eq_ignore_ascii_case(&original.pr.repository)
            && self.number == original.pr.number
            && self.pr_node == original.pr.id
            && self.head == original.head_sha
            && self.base == original.base_sha
            && self.head_ref == original.head_ref
            && self.base_ref == original.base_ref
            && self.state == original.pr.state
            && self.draft == original.pr.draft
            && self.merged == original.merged
            && self.check_id == check.id
            && check.kind == "check"
            && check.head_sha == self.head
    }
}

fn positive_bounded(raw: &Value, field: &str) -> GithubResult<u64> {
    let id = positive(raw, field)?;
    if id > i64::MAX as u64 {
        return Err(error(
            GithubErrorKind::Json,
            "Canonical bridge numeric ID exceeds positive storage range",
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
fn current_repo(
    provider: &GithubProvider,
    original: &PrCheckOriginal,
    raw: &Value,
) -> GithubResult<MetricsRepository> {
    let repo = repository(&provider.host, &original.repository, raw)?;
    positive_bounded(raw, "id")?;
    if !exact_api_url(
        &provider.host,
        &format!("repos/{}", original.repository),
        &raw["url"],
    ) {
        return Err(error(
            GithubErrorKind::IdentityChanged,
            "Check bridge current repository source differs from original host/name",
        ));
    }
    if raw["permissions"]["pull"].as_bool() != Some(true) {
        return Err(error(
            GithubErrorKind::Permission,
            "Current repository read permission unverified; private check/run discarded",
        ));
    }
    Ok(repo)
}
fn current_parent(
    provider: &GithubProvider,
    original: &PrCheckOriginal,
    repo: &MetricsRepository,
    raw: &Value,
) -> GithubResult<u64> {
    let id = positive_bounded(raw, "id")?;
    if raw["number"].as_u64() != Some(original.number)
        || raw["node_id"].as_str() != Some(original.pr_node.as_str())
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
    {
        return Err(error(GithubErrorKind::StaleHead, "Original canonical PR/node/full head/base/ref/lifecycle changed; check bridge not retargeted"));
    }
    let base = repository(&provider.host, &original.repository, &raw["base"]["repo"])?;
    require_same_repo(repo, &base)?;
    Ok(id)
}
#[derive(PartialEq, Eq)]
struct CheckProof {
    node: String,
    suite: u64,
}
fn check_proof(
    provider: &GithubProvider,
    original: &PrCheckOriginal,
    raw: &Value,
) -> GithubResult<CheckProof> {
    let node = node(raw, "node_id")?;
    let suite = positive_bounded(&raw["check_suite"], "id")?;
    if positive_bounded(raw, "id")? != original.check_id
        || raw["head_sha"].as_str() != Some(original.head.as_str())
        || !exact_api_url(
            &provider.host,
            &format!(
                "repos/{}/check-runs/{}",
                original.repository, original.check_id
            ),
            &raw["url"],
        )
    {
        return Err(error(
            GithubErrorKind::StaleHead,
            "Selected genuine CheckRun/full head/source identity changed",
        ));
    }
    Ok(CheckProof { node, suite })
}
struct ObservedRun {
    raw_node: String,
    run: WorkflowRun,
}
fn observed_run(
    provider: &GithubProvider,
    original: &PrCheckOriginal,
    repo: &MetricsRepository,
    check: &CheckProof,
    raw: &Value,
) -> GithubResult<ObservedRun> {
    let run = run_from_json(raw);
    if positive_bounded(raw, "id")? != run.id
        || positive_bounded(raw, "workflow_id")? != run.workflow_id
        || positive_bounded(raw, "run_attempt")? != run.attempt
        || positive_bounded(raw, "check_suite_id")? != check.suite
        || run.head_sha != original.head
        || !valid_opaque(&run.event)
        || !valid_opaque(&run.name)
        || run.title.len() > 4096
        || !exact_api_url(
            &provider.host,
            &format!("repos/{}/actions/runs/{}", original.repository, run.id),
            &raw["url"],
        )
    {
        return Err(error(GithubErrorKind::StaleHead, "Actual workflow run differs from selected suite/head/positive workflow/attempt/event/source"));
    }
    require_same_repo(
        repo,
        &repository(&provider.host, &original.repository, &raw["repository"])?,
    )?;
    Ok(ObservedRun {
        raw_node: node(raw, "node_id")?,
        run,
    })
}
fn same_run(first: &ObservedRun, next: &ObservedRun) -> GithubResult<()> {
    if first.raw_node != next.raw_node
        || first.run.id != next.run.id
        || first.run.workflow_id != next.run.workflow_id
        || first.run.attempt != next.run.attempt
        || first.run.head_sha != next.run.head_sha
        || first.run.event != next.run.event
    {
        return Err(error(
            GithubErrorKind::StaleHead,
            "Run/workflow/event/actual attempt changed during check bridge reads",
        ));
    }
    Ok(())
}

/// Private provenance: constructed only after the actual exact-attempt response
/// verifies the previously observed run. An arbitrary bool cannot produce it.
struct ExactAttemptEndpoint {
    repository: String,
    run_id: u64,
    attempt: u64,
    head: String,
}
struct JobProjectionBudget {
    items_left: usize,
    bytes_left: usize,
}
impl JobProjectionBudget {
    fn new(limits: &PrCheckRunReadLimits) -> Self {
        Self {
            items_left: limits.max_projected_items,
            bytes_left: limits.max_prepared_bytes,
        }
    }
    /// Raw row bytes alone do not bound expanded empty-object/string vectors.
    /// Count and reserve BEFORE the shared parser clones any job collections.
    fn reserve(&mut self, raw: &Value) -> GithubResult<()> {
        let mut items = 1usize;
        let mut bytes = std::mem::size_of::<ActionsJob>();
        for field in [
            "name",
            "status",
            "conclusion",
            "head_sha",
            "started_at",
            "completed_at",
            "runner_name",
            "runner_group_name",
            "check_run_url",
        ] {
            if let Some(value) = raw.get(field).and_then(Value::as_str) {
                bytes = bytes.saturating_add(value.len());
            }
        }
        for (field, record_size) in [
            ("steps", std::mem::size_of::<JobStep>()),
            ("labels", std::mem::size_of::<String>()),
        ] {
            match raw.get(field) {
                Some(Value::Array(values)) if values.len() <= 1000 => {
                    items = items.saturating_add(values.len());
                    bytes = bytes.saturating_add(values.len().saturating_mul(record_size));
                    for value in values {
                        if field == "labels" {
                            let label =
                                value.as_str().filter(|v| v.len() <= 1024).ok_or_else(|| {
                                    error(
                                        GithubErrorKind::Json,
                                        "Supplied job label is malformed/oversized",
                                    )
                                })?;
                            bytes = bytes.saturating_add(label.len());
                        } else {
                            if !value.is_object() {
                                return Err(error(
                                    GithubErrorKind::Json,
                                    "Supplied job step is not an actual object",
                                ));
                            }
                            for key in
                                ["name", "status", "conclusion", "started_at", "completed_at"]
                            {
                                if let Some(text) = value.get(key).and_then(Value::as_str) {
                                    bytes = bytes.saturating_add(text.len());
                                }
                            }
                        }
                    }
                }
                None | Some(Value::Null) => {}
                _ => {
                    return Err(error(
                        GithubErrorKind::Json,
                        "Job collections are malformed or exceed the bounded count",
                    ))
                }
            }
        }
        if items > self.items_left || bytes > self.bytes_left {
            return Err(error(GithubErrorKind::Unsupported, "Prepared job item/byte budget reached before cloning; exact viewer unavailable, not complete empty"));
        }
        self.items_left -= items;
        self.bytes_left -= bytes;
        Ok(())
    }
}
fn job_at(
    raw: &Value,
    proof: &ExactAttemptEndpoint,
    projection: &mut JobProjectionBudget,
) -> GithubResult<(ActionsJob, PrCheckJobAttemptOrigin)> {
    let origin = match raw.get("run_attempt") {
        None => PrCheckJobAttemptOrigin::ExactEndpointBound,
        Some(_) if positive_bounded(raw, "run_attempt")? == proof.attempt => {
            PrCheckJobAttemptOrigin::JsonObserved
        }
        Some(_) => {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Supplied job attempt differs from the verified exact-attempt endpoint",
            ))
        }
    };
    positive_bounded(raw, "id")?;
    node(raw, "node_id")?;
    if positive_bounded(raw, "run_id")? != proof.run_id
        || raw["head_sha"].as_str() != Some(proof.head.as_str())
    {
        return Err(error(
            GithubErrorKind::StaleHead,
            "Job does not belong to actual run and exact full head",
        ));
    }
    if !raw["name"].as_str().is_some_and(valid_opaque) {
        return Err(error(
            GithubErrorKind::Json,
            "Job name is unavailable or exceeds its bounded identifier",
        ));
    }
    projection.reserve(raw)?;
    // Activation needs only pub(super) visibility on the existing shared parser;
    // no duplicated parser or synthetic JSON field. This is a canonical binding.
    let mut job = super::super::super::actions::job_from_json(raw);
    job.attempt = proof.attempt;
    Ok((job, origin))
}

struct PageRequest<'a> {
    endpoint: &'a str,
    field: &'a str,
    max_pages: usize,
}
fn pages(
    provider: &GithubProvider,
    budget: &mut MetricsBudget,
    limits: &PrCheckRunReadLimits,
    query: PageRequest<'_>,
    shared_rows: &mut usize,
    cancel: &CancellationToken,
) -> GithubResult<(Vec<Value>, PrCheckBridgePageCoverage)> {
    let mut coverage = PrCheckBridgePageCoverage::default();
    let mut rows = Vec::new();
    for page in 1..=query.max_pages {
        if shared_rows.saturating_add(limits.page_size) > limits.max_rows {
            coverage.reasons.push(
                "Shared fixed-page raw-row budget reached; remaining membership unknown".into(),
            );
            break;
        }
        let separator = if query.endpoint.contains('?') {
            '&'
        } else {
            '?'
        };
        coverage.requested_pages += 1;
        let value = budget.read_document(
            provider,
            &format!(
                "{}{separator}per_page={}&page={page}",
                query.endpoint, limits.page_size
            ),
            cancel,
        )?;
        let total = value["total_count"].as_u64().ok_or_else(|| {
            error(
                GithubErrorKind::Json,
                "Bridge page omitted actual total_count; no completeness inferred",
            )
        })?;
        if coverage.total_matches.is_some_and(|old| old != total) {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Bridge collection total changed while paging; no mixed membership chosen",
            ));
        }
        coverage.total_matches = Some(total);
        let values = value[query.field].as_array().ok_or_else(|| {
            error(
                GithubErrorKind::Json,
                "Bridge page has no actual resource collection",
            )
        })?;
        if values.len() > limits.page_size
            || values.len() > limits.max_rows.saturating_sub(*shared_rows)
        {
            return Err(error(
                GithubErrorKind::Json,
                "Bridge endpoint exceeded its exact requested page/row budget",
            ));
        }
        coverage.fetched_pages += 1;
        coverage.raw_rows += values.len();
        *shared_rows += values.len();
        for row in values {
            let bytes = serde_json::to_vec(row)
                .map_err(|e| error(GithubErrorKind::Json, e.to_string()))?
                .len();
            if bytes > limits.max_row_bytes {
                coverage.excluded_rows += 1;
                coverage.reasons.push(
                    "Oversized observed row excluded before cloning; membership partial".into(),
                );
            } else {
                rows.push(row.clone());
                coverage.admitted_rows += 1;
            }
        }
        if coverage.raw_rows as u64 > total {
            return Err(error(
                GithubErrorKind::Json,
                "Bridge raw row count contradicts actual total_count",
            ));
        }
        if values.len() < limits.page_size {
            coverage.enumeration_complete =
                coverage.raw_rows as u64 == total && coverage.excluded_rows == 0;
            break;
        }
    }
    if !coverage.enumeration_complete {
        coverage.reasons.push(
            "Catalogue enumeration is partial; no absence or selected check attempt inferred"
                .into(),
        );
    }
    Ok((rows, coverage))
}

impl GithubProvider {
    /// Own one bounded clock/capture budget. Happy one-run/one-job protocol has
    /// twelve API GETs. Caller must independently bracket prepared publication.
    pub fn pr_check_run(
        &self,
        identity: &HostIdentity,
        original: &PrSummary,
        selected: &Check,
        limits: &PrCheckRunReadLimits,
        cancel: &CancellationToken,
    ) -> GithubResult<PrCheckRunObservation> {
        self.pr_check_run_with_guard(identity, original, selected, limits, cancel, |_| Ok(()))
    }
    /// The caller retains its original local visibility/barrier guard. Closing
    /// SQL is checked before the final canonical repository and principal reads;
    /// callback time consumes this same source budget, never a new admission.
    pub fn pr_check_run_with_guard(
        &self,
        identity: &HostIdentity,
        original: &PrSummary,
        selected: &Check,
        limits: &PrCheckRunReadLimits,
        cancel: &CancellationToken,
        guard: impl FnMut(PrCheckReadPhase) -> GithubResult<()>,
    ) -> GithubResult<PrCheckRunObservation> {
        self.pr_check_run_impl_v1(identity, original, selected, limits, cancel, guard, None)
    }
    #[allow(clippy::too_many_arguments)]
    fn pr_check_run_impl_v1(
        &self,
        identity: &HostIdentity,
        original: &PrSummary,
        selected: &Check,
        limits: &PrCheckRunReadLimits,
        cancel: &CancellationToken,
        mut guard: impl FnMut(PrCheckReadPhase) -> GithubResult<()>,
        scoped: Option<&ScopedPrCheckBindingV1<'_>>,
    ) -> GithubResult<PrCheckRunObservation> {
        let selected_limits = limits.selected()?;
        let original = PrCheckOriginal::new(self, identity, original, selected)?;
        let mut budget = MetricsBudget::new(&selected_limits)?;
        let requester = principal(self, &mut budget, identity, cancel)?;
        let repo_path = format!("repos/{}", original.repository);
        let parent_path = format!("{repo_path}/pulls/{}", original.number);
        let check_path = format!("{repo_path}/check-runs/{}", original.check_id);
        let raw = budget.read_document(self, &repo_path, cancel)?;
        let repo = current_repo(self, &original, &raw)?;
        if let Some(binding) = scoped {
            binding.repository(&raw)?;
        }
        let raw = budget.read_document(self, &parent_path, cancel)?;
        let pr_id = current_parent(self, &original, &repo, &raw)?;
        if let Some(binding) = scoped {
            binding.parent(self, &raw)?;
        }
        let raw = budget.read_document(self, &check_path, cancel)?;
        let check = check_proof(self, &original, &raw)?;
        if let Some(binding) = scoped {
            binding.check(&raw)?;
        }
        budget.pr_check_guard(cancel, || guard(PrCheckReadPhase::BeforeRunCollection))?;
        let mut shared_rows = 0;
        let (raw_runs, run_coverage) = pages(
            self,
            &mut budget,
            limits,
            PageRequest {
                endpoint: &format!(
                    "{repo_path}/actions/runs?check_suite_id={}&head_sha={}",
                    check.suite, original.head
                ),
                field: "workflow_runs",
                max_pages: limits.max_run_pages,
            },
            &mut shared_rows,
            cancel,
        )?;
        let mut runs = BTreeMap::new();
        for raw in &raw_runs {
            let run = observed_run(self, &original, &repo, &check, raw)?;
            if runs.insert(run.run.id, run).is_some() {
                return Err(error(GithubErrorKind::Json, "Duplicate run identity during bridge pagination; no complete membership inferred"));
            }
        }
        let run_candidates = if scoped.is_some() {
            choices_v1::retain_candidates_v1(&runs, limits)?
        } else {
            vec![]
        };
        let mut selected_run = None;
        let mut link = None;
        let mut job_coverage = PrCheckBridgePageCoverage::default();
        let mut unavailable = if runs.is_empty() {
            Some(if run_coverage.enumeration_complete {
                PrCheckBridgeUnavailable::NoCorrelatedRunObserved
            } else {
                PrCheckBridgeUnavailable::PartialRunCatalogue
            })
        } else if runs.len() > 1 {
            Some(PrCheckBridgeUnavailable::AmbiguousRunCorrelation)
        } else {
            None
        };
        if runs.len() == 1 {
            let run = runs.into_values().next().unwrap();
            let read = choices_v1::attempt_jobs_v1(
                self,
                &original,
                &repo,
                &check,
                &run,
                &mut budget,
                limits,
                &mut shared_rows,
                cancel,
                scoped.is_some(),
                |_| Ok(()),
            )?;
            job_coverage = read.coverage;
            unavailable = match read.matched.as_slice() {
                [job] => {
                    link = Some(PrCheckRunLink {
                        attempt: read.attempt.clone(),
                        selected_job: *job,
                        sources: read.sources,
                        run_node: read.run_node.clone(),
                    });
                    None
                }
                [] => Some(if job_coverage.enumeration_complete {
                    PrCheckBridgeUnavailable::NoCorrelatedJobObserved
                } else {
                    PrCheckBridgeUnavailable::PartialJobCatalogue
                }),
                _ => Some(PrCheckBridgeUnavailable::AmbiguousJobCorrelation),
            };
            if scoped.is_some() {
                selected_run = Some(Arc::new(PrCheckSelectedRunV1 {
                    attempt: read.attempt,
                    run_node: read.run_node,
                    members: read.members,
                    selected_job: link.as_ref().map(|link| link.selected_job),
                    selection: PrCheckSelectionOriginV1::ObservedOnly,
                }));
            }
        }
        let raw = budget.read_document(self, &check_path, cancel)?;
        let closing_check = check_proof(self, &original, &raw)?;
        if let Some(binding) = scoped {
            binding.check(&raw)?;
        }
        if check != closing_check {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Selected CheckRun node/suite changed during correlation",
            ));
        }
        let raw = budget.read_document(self, &parent_path, cancel)?;
        let closing_pr = current_parent(self, &original, &repo, &raw)?;
        if let Some(binding) = scoped {
            binding.parent(self, &raw)?;
        }
        if closing_pr != pr_id {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Original PR database ID changed during check bridge reads",
            ));
        }
        // Preserve a local error but let actual closing resource/account loss
        // take priority. An exhausted/cancelled budget launches no next child.
        let local = budget.pr_check_guard(cancel, || guard(PrCheckReadPhase::BeforeFinalProof));
        let raw = budget.read_document(self, &repo_path, cancel)?;
        let closing_repo = current_repo(self, &original, &raw)?;
        if let Some(binding) = scoped {
            binding.repository(&raw)?;
        }
        require_same_repo(&repo, &closing_repo)?;
        let requester = principal(self, &mut budget, &requester, cancel)?;
        local.map_err(|failure| GithubError {
            kind: failure.kind,
            message: "Original local PR-check visibility guard unavailable; private results remain hidden".into(),
        })?;
        Ok(PrCheckRunObservation {
            identity: requester,
            original,
            repository: closing_repo,
            pr_database_id: closing_pr,
            check_node: closing_check.node,
            suite_id: closing_check.suite,
            link,
            unavailable,
            run_coverage,
            job_coverage,
            limits: limits.clone(),
            requests: budget.requests,
            stdout_bytes: budget.bytes,
            diagnostic_bytes: budget.successful_diagnostic_bytes,
            failed_reserved_bytes: budget.failed_reserved_bytes,
            observed_from: budget.observed_from,
            observed_through: budget.observed_through()?,
            producer: self.runner.clone(),
            run_candidates,
            selected_run,
            choice_unavailable: None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrCheckReadPhase {
    BeforeRunCollection,
    BeforeFinalProof,
}
