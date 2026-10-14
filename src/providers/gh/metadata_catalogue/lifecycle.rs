//! SOURCE ONLY: exact retained catalogue-origin lifecycle observation.
//! No constructor/serde/write/ACK authority is exposed. This never publishes.
use super::*;
use std::{fmt, sync::Arc, time::Instant};

pub const PR_LIFECYCLE_OBSERVATION_QUERY: &str = "query($id:ID!){node(id:$id){__typename ...on PullRequest{id fullDatabaseId number headRefOid baseRefOid headRefName baseRefName state isDraft merged repository{id nameWithOwner} viewerCanClose viewerCanReopen viewerCanUpdate viewerCanUpdateBranch}}}";

#[derive(Clone, Debug)]
pub struct PrLifecycleReadLimits {
    pub max_document_bytes: usize,
    pub max_response_bytes: usize,
    pub max_duration: Duration,
}
impl Default for PrLifecycleReadLimits {
    fn default() -> Self {
        Self {
            max_document_bytes: 128 * 1024,
            max_response_bytes: 1024 * 1024,
            max_duration: Duration::from_secs(30),
        }
    }
}
impl PrLifecycleReadLimits {
    fn metrics(&self) -> GithubResult<MetricsReadLimits> {
        if !(1..=128 * 1024).contains(&self.max_document_bytes)
            || !(1..=2 * 1024 * 1024).contains(&self.max_response_bytes)
            || self.max_duration.is_zero()
            || self.max_duration > Duration::from_secs(60)
        {
            return Err(error(GithubErrorKind::InvalidTarget,"Lifecycle requires positive bounded canonical/aggregate capture and original duration"));
        }
        Ok(MetricsReadLimits {
            max_pages: 1,
            page_size: 1,
            max_rows: 1,
            max_document_bytes: self.max_document_bytes,
            max_response_bytes: self.max_response_bytes,
            max_duration: self.max_duration,
        })
    }
}

/// Honest field provenance, not a bool permission/eligibility constructor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrLifecycleBoolean {
    Observed(bool),
    Null,
    Missing,
    Malformed,
    NotLoaded,
}
impl PrLifecycleBoolean {
    pub fn label(self) -> &'static str {
        match self {
            Self::Observed(true) => "observed true",
            Self::Observed(false) => "observed false",
            Self::Null => "Source unknown (null)",
            Self::Missing => "Source unknown (missing)",
            Self::Malformed => "Source unknown (malformed)",
            Self::NotLoaded => "Source unknown (not loaded)",
        }
    }
}
#[derive(Clone, Debug)]
pub struct PrLifecycleCapabilities {
    close: PrLifecycleBoolean,
    reopen: PrLifecycleBoolean,
    update_object: PrLifecycleBoolean,
    update_branch: PrLifecycleBoolean,
    reason: Option<&'static str>,
}
impl PrLifecycleCapabilities {
    pub fn close(&self) -> PrLifecycleBoolean {
        self.close
    }
    pub fn reopen(&self) -> PrLifecycleBoolean {
        self.reopen
    }
    /// Generic object-edit property, NOT draft/ready/lock/merge eligibility.
    pub fn update_object(&self) -> PrLifecycleBoolean {
        self.update_object
    }
    /// False also means already-up-to-date; it is not solely permission denied.
    pub fn update_branch(&self) -> PrLifecycleBoolean {
        self.update_branch
    }
    pub fn unavailable_reason(&self) -> Option<&'static str> {
        self.reason
    }
    pub fn draft_ready_eligibility(&self) -> PrLifecycleBoolean {
        PrLifecycleBoolean::NotLoaded
    }
    pub fn lock_eligibility(&self) -> PrLifecycleBoolean {
        PrLifecycleBoolean::NotLoaded
    }
    pub fn merge_eligibility(&self) -> PrLifecycleBoolean {
        PrLifecycleBoolean::NotLoaded
    }
    pub fn queue_state(&self) -> PrLifecycleBoolean {
        PrLifecycleBoolean::NotLoaded
    }
    pub fn auto_merge_state(&self) -> PrLifecycleBoolean {
        PrLifecycleBoolean::NotLoaded
    }
    pub fn repository_merge_settings(&self) -> PrLifecycleBoolean {
        PrLifecycleBoolean::NotLoaded
    }
    pub fn required_checks_and_rules(&self) -> PrLifecycleBoolean {
        PrLifecycleBoolean::NotLoaded
    }
}

/// Metadata-only callback input produced after actual scoped reads. Not Clone,
/// serde, a mutation permit or a reusable current-access authority token.
pub struct PrLifecycleSqlGuardContext<'a> {
    identity: &'a HostIdentity,
    repository: &'a MetricsRepository,
    original: &'a PrSummary,
    pr_id: u64,
    issue_id: u64,
    issue_node: &'a str,
    deadline: Instant,
}
impl fmt::Debug for PrLifecycleSqlGuardContext<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PrLifecycleSqlGuardContext(<exact scoped metadata; no publication authority>)")
    }
}
impl PrLifecycleSqlGuardContext<'_> {
    pub fn identity(&self) -> &HostIdentity {
        self.identity
    }
    pub fn repository(&self) -> &MetricsRepository {
        self.repository
    }
    pub fn pr_id(&self) -> u64 {
        self.pr_id
    }
    pub fn pr_node(&self) -> &str {
        &self.original.pr.id
    }
    pub fn pr_number(&self) -> u64 {
        self.original.pr.number
    }
    pub fn head(&self) -> &str {
        &self.original.head_sha
    }
    pub fn base(&self) -> &str {
        &self.original.base_sha
    }
    pub fn issue_id(&self) -> u64 {
        self.issue_id
    }
    pub fn issue_node(&self) -> &str {
        self.issue_node
    }
    /// Scheduling cutoff only, never principal/resource/write authority.
    pub fn deadline(&self) -> Instant {
        self.deadline
    }
}

pub struct PrLifecycleObservation {
    origin: Arc<PrMetadataCatalogueRead>,
    identity: HostIdentity,
    repository: MetricsRepository,
    pr_id: u64,
    issue_id: u64,
    issue_node: String,
    locked: PrLifecycleBoolean,
    capabilities: PrLifecycleCapabilities,
    interval: (u64, u64),
    accounting: PrMetadataReadAccounting,
}
impl fmt::Debug for PrLifecycleObservation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PrLifecycleObservation(<retained original; bounded fresh readonly facts>)")
    }
}
impl PrLifecycleObservation {
    pub fn origin(&self) -> &Arc<PrMetadataCatalogueRead> {
        &self.origin
    }
    pub fn original(&self) -> &PrSummary {
        self.origin.target()
    }
    pub fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub fn repository(&self) -> &MetricsRepository {
        &self.repository
    }
    pub fn pr_id(&self) -> u64 {
        self.pr_id
    }
    pub fn issue_id(&self) -> u64 {
        self.issue_id
    }
    pub fn issue_node(&self) -> &str {
        &self.issue_node
    }
    pub fn locked(&self) -> PrLifecycleBoolean {
        self.locked
    }
    pub fn capabilities(&self) -> &PrLifecycleCapabilities {
        &self.capabilities
    }
    pub fn observed_interval(&self) -> (u64, u64) {
        self.interval
    }
    pub fn accounting(&self) -> &PrMetadataReadAccounting {
        &self.accounting
    }
}

impl GithubProvider {
    /// Eight API children under ONE original budget, zero mutations. The hook
    /// is a caller-owned bounded metadata SQL guard, not proof that a write was
    /// invoked or confirmed. Its error cannot obscure closing identity loss.
    pub fn pr_lifecycle_observation(
        &self,
        origin: Arc<PrMetadataCatalogueRead>,
        limits: &PrLifecycleReadLimits,
        mut sql_guard: impl FnMut(&PrLifecycleSqlGuardContext<'_>) -> GithubResult<()>,
        cancel: &CancellationToken,
    ) -> GithubResult<PrLifecycleObservation> {
        let selected = limits.metrics()?;
        let original = origin.target();
        preflight(self, origin.identity(), original, origin.context())?;
        if origin
            .identity()
            .account_id
            .is_none_or(|id| id == 0 || id > i64::MAX as u64)
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Retained lifecycle origin needs a positive bounded numeric principal",
            ));
        }
        let expected_repo = origin.canonical_repository();
        let pr_id = bounded_id(origin.parent_raw(), "id")?;
        let issue_id = bounded_id(&origin.context().issue_metadata, "id")?;
        let issue_node = origin.context().issue_metadata["node_id"]
            .as_str()
            .filter(|value| valid_opaque(value))
            .ok_or_else(|| {
                error(
                    GithubErrorKind::InvalidTarget,
                    "Original actual issue node unavailable",
                )
            })?
            .to_owned();
        // The origin is an existing opaque gh-produced observation, not public
        // raw IDs/context bools assembled into a new authority object.
        require_parent(self, &origin, expected_repo, origin.parent_raw())?;
        issue_identity(self, original, &origin.context().issue_metadata)?;
        let mut budget = MetricsBudget::new(&selected)?;
        let requester = lifecycle_principal(self, &mut budget, origin.identity(), cancel)?;
        let repository_endpoint = format!("repos/{}", expected_repo.full_name);
        let parent_endpoint = format!("{repository_endpoint}/pulls/{}", original.pr.number);
        let issue_endpoint = format!("{repository_endpoint}/issues/{}", original.pr.number);
        let repo_raw = budget
            .read_document(self, &repository_endpoint, cancel)
            .map_err(safe_failure)?;
        let repo = canonical_repository(self, &origin, &repo_raw)?;
        let parent_raw = budget
            .read_document(self, &parent_endpoint, cancel)
            .map_err(safe_failure)?;
        require_parent(self, &origin, &repo, &parent_raw)?;
        let issue_raw = budget
            .read_document(self, &issue_endpoint, cancel)
            .map_err(safe_failure)?;
        issue_identity(self, original, &issue_raw)?;
        if bounded_id(&issue_raw, "id")? != issue_id
            || issue_raw["node_id"].as_str() != Some(issue_node.as_str())
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Exact original issue database/node changed; no issue/PR ID alias",
            ));
        }
        if issue_raw.get("state") != parent_raw.get("state") {
            return Err(error(
                GithubErrorKind::StaleHead,
                "PR and independently linked issue lifecycle disagree",
            ));
        }
        if origin.context().issue_metadata.get("locked").is_some()
            && origin.context().issue_metadata.get("locked") != issue_raw.get("locked")
        {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Original conversation lock changed since retained source",
            ));
        }
        let graph_result = budget.lifecycle_graphql_capture(
            self,
            PR_LIFECYCLE_OBSERVATION_QUERY,
            &original.pr.id,
            cancel,
        );
        let mut failure = None;
        let capabilities = match graph_result {
            Ok(raw) => match capabilities(&origin, &repo, &raw) {
                Ok(value) => value,
                Err(error_) => {
                    failure = Some(error_);
                    unavailable_capabilities()
                }
            },
            Err(error_) => {
                // Ordinary feature/endpoint failure may be unavailable, but
                // typed account/resource failure cannot admit private facts.
                if matches!(
                    error_.kind,
                    GithubErrorKind::Authentication
                        | GithubErrorKind::IdentityChanged
                        | GithubErrorKind::Permission
                        | GithubErrorKind::NotFound
                ) {
                    failure = Some(safe_failure(error_));
                }
                unavailable_capabilities()
            }
        };
        // A capped/cancelled/expired source cannot launch even a closing read.
        budget.lifecycle_assert_active(cancel)?;
        let final_parent_result = budget
            .read_document(self, &parent_endpoint, cancel)
            .and_then(|raw| {
                require_parent(self, &origin, &repo, &raw)?;
                if parent_raw["updated_at"] != raw["updated_at"] {
                    return Err(error(
                        GithubErrorKind::StaleHead,
                        "PR changed during lifecycle observation",
                    ));
                }
                Ok(())
            });
        if let Err(error_) = final_parent_result {
            failure = Some(safe_failure(error_));
        }
        budget.lifecycle_assert_active(cancel)?;
        if failure.is_none() {
            let context = PrLifecycleSqlGuardContext {
                identity: &requester,
                repository: &repo,
                original,
                pr_id,
                issue_id,
                issue_node: &issue_node,
                deadline: budget.lifecycle_deadline(),
            };
            // Check the original deadline/cancel before and AFTER local SQL;
            // success/error never starts a replacement source budget.
            if let Err(error_) = sql_guard(&context) {
                failure = Some(error_);
            }
        }
        budget.lifecycle_assert_active(cancel)?;
        let closing_repo = budget
            .read_document(self, &repository_endpoint, cancel)
            .and_then(|raw| canonical_repository(self, &origin, &raw));
        // Even a local SQL or ordinary final repo failure must not win over a
        // new numeric principal/auth failure. No child follows exhaustion.
        let final_identity = lifecycle_principal(self, &mut budget, &requester, cancel)?;
        let closing_repo = closing_repo.map_err(safe_failure)?;
        budget.lifecycle_finish(cancel)?;
        if let Some(error_) = failure {
            return Err(error_);
        }
        let interval = (budget.observed_from, budget.observed_through()?);
        let accounting = PrMetadataReadAccounting {
            requests: budget.requests,
            successful_stdout_bytes: budget.bytes,
            successful_diagnostic_bytes: budget.successful_diagnostic_bytes,
            failed_reserved_bytes: budget.failed_reserved_bytes,
        };
        Ok(PrLifecycleObservation {
            origin,
            identity: final_identity,
            repository: closing_repo,
            pr_id,
            issue_id,
            issue_node,
            locked: observed_boolean(&issue_raw, "locked"),
            capabilities,
            interval,
            accounting,
        })
    }
}
fn bounded_id(raw: &Value, key: &str) -> GithubResult<u64> {
    positive(raw, key).and_then(|id| {
        if id <= i64::MAX as u64 {
            Ok(id)
        } else {
            Err(error(
                GithubErrorKind::InvalidTarget,
                "Canonical lifecycle ID exceeds signed database domain",
            ))
        }
    })
}
fn lifecycle_principal(
    provider: &GithubProvider,
    budget: &mut MetricsBudget,
    expected: &HostIdentity,
    cancel: &CancellationToken,
) -> GithubResult<HostIdentity> {
    principal(provider, budget, expected, cancel).map_err(|failure| {
        if matches!(failure.kind, GithubErrorKind::Json) {
            error(GithubErrorKind::IdentityChanged,"Current lifecycle numeric principal is unverified; prior private facts must not be reused")
        } else {
            safe_failure(failure)
        }
    })
}
fn canonical_repository(
    provider: &GithubProvider,
    origin: &PrMetadataCatalogueRead,
    raw: &Value,
) -> GithubResult<MetricsRepository> {
    let repo = repository(
        &provider.host,
        &origin.canonical_repository().full_name,
        raw,
    )?;
    if repo.id > i64::MAX as u64 {
        return Err(error(
            GithubErrorKind::InvalidTarget,
            "Canonical repository database ID out of bounds",
        ));
    }
    require_same_repo(origin.canonical_repository(), &repo)?;
    match raw["permissions"].get("pull") {
        Some(Value::Bool(true)) => {}
        Some(Value::Bool(false)) => {
            return Err(error(
                GithubErrorKind::Permission,
                "Current canonical repository read permission is denied",
            ))
        }
        _ => {
            return Err(error(
                GithubErrorKind::Unsupported,
                "Current canonical repository read capability is unknown, not granted or denied",
            ))
        }
    }
    Ok(repo)
}
fn require_parent(
    provider: &GithubProvider,
    origin: &PrMetadataCatalogueRead,
    repo: &MetricsRepository,
    raw: &Value,
) -> GithubResult<()> {
    parent_identity(provider, origin.target(), repo, raw)?;
    if bounded_id(raw, "id")? != bounded_id(origin.parent_raw(), "id")?
        || raw["head"]["ref"].as_str() != Some(origin.target().head_ref.as_str())
        || raw["base"]["ref"].as_str() != Some(origin.target().base_ref.as_str())
        || raw.get("state") != origin.parent_raw().get("state")
        || raw.get("draft") != origin.parent_raw().get("draft")
        || raw.get("merged") != origin.parent_raw().get("merged")
        || !matches!(raw["state"].as_str(), Some("open" | "closed"))
        || raw["draft"].as_bool().is_none()
        || raw["merged"].as_bool().is_none()
    {
        return Err(error(
            GithubErrorKind::StaleHead,
            "Original lifecycle PR database/node/full comparison/ref/state changed or unknown",
        ));
    }
    if raw["merged"] == true && (raw["state"] != "closed" || raw["draft"] != false) {
        return Err(error(
            GithubErrorKind::Json,
            "Original PR merged/draft/state is inconsistent",
        ));
    }
    Ok(())
}
fn observed_boolean(raw: &Value, key: &str) -> PrLifecycleBoolean {
    match raw.get(key) {
        Some(Value::Bool(value)) => PrLifecycleBoolean::Observed(*value),
        Some(Value::Null) => PrLifecycleBoolean::Null,
        None => PrLifecycleBoolean::Missing,
        _ => PrLifecycleBoolean::Malformed,
    }
}
fn unavailable_capabilities() -> PrLifecycleCapabilities {
    PrLifecycleCapabilities {
        close: PrLifecycleBoolean::NotLoaded,
        reopen: PrLifecycleBoolean::NotLoaded,
        update_object: PrLifecycleBoolean::NotLoaded,
        update_branch: PrLifecycleBoolean::NotLoaded,
        reason: Some(
            "Core lifecycle GraphQL source unavailable; no eligibility or host support inferred",
        ),
    }
}
fn capabilities(
    origin: &PrMetadataCatalogueRead,
    repo: &MetricsRepository,
    raw: &Value,
) -> GithubResult<PrLifecycleCapabilities> {
    if raw
        .get("errors")
        .is_some_and(|errors| !errors.is_null() && !errors.as_array().is_some_and(Vec::is_empty))
    {
        return Ok(unavailable_capabilities());
    }
    let node = &raw["data"]["node"];
    // BigInt is a documented decimal STRING, not deprecated databaseId or a
    // synthetic numeric/node alias. Missing nullable ID disables capabilities.
    let Some(database) = node["fullDatabaseId"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 19 && s.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|id| *id > 0 && *id <= i64::MAX as u64)
    else {
        return Ok(unavailable_capabilities());
    };
    let original = origin.target();
    let state = if origin.parent_raw()["merged"] == true {
        "MERGED"
    } else if origin.parent_raw()["state"] == "open" {
        "OPEN"
    } else {
        "CLOSED"
    };
    if node["__typename"].as_str() != Some("PullRequest")
        || node["id"].as_str() != Some(original.pr.id.as_str())
        || database != bounded_id(origin.parent_raw(), "id")?
        || node["number"].as_u64() != Some(original.pr.number)
        || node["repository"]["id"].as_str() != Some(repo.node_id.as_str())
        || !node["repository"]["nameWithOwner"]
            .as_str()
            .is_some_and(|s| s.eq_ignore_ascii_case(&repo.full_name))
        || node["headRefOid"].as_str() != Some(original.head_sha.as_str())
        || node["baseRefOid"].as_str() != Some(original.base_sha.as_str())
        || node["headRefName"].as_str() != Some(original.head_ref.as_str())
        || node["baseRefName"].as_str() != Some(original.base_ref.as_str())
        || node["state"].as_str() != Some(state)
        || node["isDraft"] != origin.parent_raw()["draft"]
        || node["merged"] != origin.parent_raw()["merged"]
    {
        return Err(error(GithubErrorKind::IdentityChanged,"Lifecycle capability data belongs to another actual original PR/repository/comparison/state"));
    }
    Ok(PrLifecycleCapabilities {
        close: observed_boolean(node, "viewerCanClose"),
        reopen: observed_boolean(node, "viewerCanReopen"),
        update_object: observed_boolean(node, "viewerCanUpdate"),
        update_branch: observed_boolean(node, "viewerCanUpdateBranch"),
        reason: None,
    })
}
fn safe_failure(error_: GithubError) -> GithubError {
    error(error_.kind,"Lifecycle current access or canonical observation failed; private prior facts must not be reused")
}
