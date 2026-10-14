//! Opaque short-lived current scope receipts, NOT history or SQL authorization.
//! This module stays undeclared until the joint history/capture activation.
use super::*;
use std::time::Instant;

pub const METRICS_SCOPE_RECEIPT_MAX_AGE: Duration = Duration::from_secs(5);
const MAX_PARENTS: usize = 64;
const MAX_REPOSITORIES: usize = 64;
const MAX_ORIGINAL_METADATA: usize = 32 * 1024 * 1024;

/// Actual successful kept captures and conservative failed capture reservations.
/// No Serialize/Deserialize or public constructor exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MetricsReadAccounting {
    requests: usize,
    requested_pages: usize,
    fetched_pages: usize,
    raw_rows: usize,
    successful_stdout_bytes: usize,
    successful_diagnostic_bytes: usize,
    failed_reserved_bytes: usize,
}
impl MetricsReadAccounting {
    pub fn requests(&self) -> usize {
        self.requests
    }
    pub fn requested_pages(&self) -> usize {
        self.requested_pages
    }
    pub fn fetched_pages(&self) -> usize {
        self.fetched_pages
    }
    pub fn raw_rows(&self) -> usize {
        self.raw_rows
    }
    pub fn successful_stdout_bytes(&self) -> usize {
        self.successful_stdout_bytes
    }
    pub fn successful_diagnostic_bytes(&self) -> usize {
        self.successful_diagnostic_bytes
    }
    pub fn failed_reserved_bytes(&self) -> usize {
        self.failed_reserved_bytes
    }
    pub fn charged_bytes(&self) -> usize {
        self.successful_stdout_bytes
            .saturating_add(self.successful_diagnostic_bytes)
            .saturating_add(self.failed_reserved_bytes)
    }
    pub(super) fn from_budget(
        budget: &MetricsBudget,
        categories: &[&MetricsReadCoverage],
    ) -> GithubResult<Self> {
        let mut value = Self {
            requests: budget.requests,
            requested_pages: 0,
            fetched_pages: 0,
            raw_rows: 0,
            successful_stdout_bytes: budget.bytes,
            successful_diagnostic_bytes: budget.successful_diagnostic_bytes,
            failed_reserved_bytes: budget.failed_reserved_bytes,
        };
        for category in categories {
            value.requested_pages = value
                .requested_pages
                .checked_add(category.requested_pages)
                .ok_or_else(accounting_overflow)?;
            value.fetched_pages = value
                .fetched_pages
                .checked_add(category.fetched_pages)
                .ok_or_else(accounting_overflow)?;
            value.raw_rows = value
                .raw_rows
                .checked_add(category.fetched_rows)
                .ok_or_else(accounting_overflow)?;
        }
        value
            .successful_stdout_bytes
            .checked_add(value.successful_diagnostic_bytes)
            .and_then(|bytes| bytes.checked_add(value.failed_reserved_bytes))
            .ok_or_else(accounting_overflow)?;
        Ok(value)
    }
}
fn accounting_overflow() -> GithubError {
    error(
        GithubErrorKind::Transport,
        "Opaque metrics scope accounting overflow",
    )
}

/// Only actual bounded provider reads issue this proof. Private target/clock,
/// no serde, and no public bool/date/DTO constructor. Root must ALSO check the
/// original journals, SQL reservation epochs and cancellation on every SQL hook.
pub struct MetricsScopeReceipt {
    identity: HostIdentity,
    parent: MetricsPrObservation,
    observed_interval: (u64, u64),
    verified_at: Instant,
    accounting: MetricsReadAccounting,
}
impl fmt::Debug for MetricsScopeReceipt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MetricsScopeReceipt(<sealed fresh canonical scope>)")
    }
}
impl MetricsScopeReceipt {
    pub fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub fn parent(&self) -> &MetricsPrObservation {
        &self.parent
    }
    pub fn observed_interval(&self) -> (u64, u64) {
        self.observed_interval
    }
    /// A batch shares these counters: charge the batch ONCE, never per receipt.
    pub fn accounting(&self) -> &MetricsReadAccounting {
        &self.accounting
    }
    pub fn validate_for(
        &self,
        identity: &HostIdentity,
        original: &MetricsPrObservation,
        max_age: Duration,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        validate_age(self.verified_at, max_age, cancel)?;
        if !self.identity.same_account(identity) || identity.account_id.is_none_or(|id| id == 0) {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Opaque metrics scope receipt belongs to another positive principal",
            ));
        }
        require_same_parent(&self.parent, original)
    }
    /// Parent module calls this only after its actual successful final bracket.
    pub(super) fn from_successful_reads(
        identity: HostIdentity,
        parent: MetricsPrObservation,
        budget: &MetricsBudget,
        categories: &[&MetricsReadCoverage],
        verified_at: Instant,
        cancel: &CancellationToken,
    ) -> GithubResult<Self> {
        validate_age(verified_at, METRICS_SCOPE_RECEIPT_MAX_AGE, cancel)?;
        Ok(Self {
            identity,
            parent,
            observed_interval: (budget.observed_from, budget.observed_through()?),
            verified_at,
            accounting: MetricsReadAccounting::from_budget(budget, categories)?,
        })
    }
}
fn validate_age(
    verified_at: Instant,
    max_age: Duration,
    cancel: &CancellationToken,
) -> GithubResult<()> {
    if max_age.is_zero()
        || max_age > METRICS_SCOPE_RECEIPT_MAX_AGE
        || cancel.is_cancelled()
        || verified_at.elapsed() > max_age
    {
        return Err(error(GithubErrorKind::Transport, "Metrics SQL-only scope lease expired/cancelled or requested age exceeds fixed five-second maximum"));
    }
    Ok(())
}

pub struct MetricsScopeReceiptBatch {
    identity: HostIdentity,
    receipts: Vec<MetricsScopeReceipt>,
    repositories: Vec<MetricsRepository>,
    repository_verified_at: Vec<Instant>,
    accounting: MetricsReadAccounting,
    observed_interval: (u64, u64),
}
impl fmt::Debug for MetricsScopeReceiptBatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MetricsScopeReceiptBatch(<sealed fresh canonical repository group>)")
    }
}
impl MetricsScopeReceiptBatch {
    pub fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub fn receipts(&self) -> &[MetricsScopeReceipt] {
        &self.receipts
    }
    pub fn checked_repositories(&self) -> &[MetricsRepository] {
        &self.repositories
    }
    pub fn accounting(&self) -> &MetricsReadAccounting {
        &self.accounting
    }
    pub fn observed_interval(&self) -> (u64, u64) {
        self.observed_interval
    }
    /// Requires the EXACT original checked union; a subset is not batch proof.
    pub fn validate_repositories(
        &self,
        identity: &HostIdentity,
        original: &[MetricsRepository],
        max_age: Duration,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        if !self.identity.same_account(identity)
            || identity.account_id.is_none_or(|id| id == 0)
            || original.len() != self.repositories.len()
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Scope receipt batch principal/whole repository set changed",
            ));
        }
        let expected: BTreeMap<_, _> = original
            .iter()
            .map(|repo| (&repo.full_name, repo))
            .collect();
        if expected.len() != original.len()
            || self.repositories.iter().any(|repo| {
                expected
                    .get(&repo.full_name)
                    .is_none_or(|original| *original != repo)
            })
        {
            return Err(error(GithubErrorKind::IdentityChanged, "Scope receipt checked repository canonical IDs/nodes/names differ from original union"));
        }
        for verified_at in &self.repository_verified_at {
            validate_age(*verified_at, max_age, cancel)?;
        }
        for receipt in &self.receipts {
            receipt.validate_for(identity, receipt.parent(), max_age, cancel)?;
        }
        Ok(())
    }
}

fn preflight_repository(
    provider: &GithubProvider,
    identity: &HostIdentity,
    repo: &MetricsRepository,
) -> GithubResult<()> {
    scope(provider, identity, &repo.full_name)?;
    if repo.host != provider.host
        || repo.id == 0
        || !valid_opaque(&repo.node_id)
        || repo.full_name != repo.full_name.to_ascii_lowercase()
    {
        return Err(error(
            GithubErrorKind::InvalidTarget,
            "Scope receipt requires positive original canonical repository IDs/nodes/host/name",
        ));
    }
    Ok(())
}
fn preflight_parent(
    provider: &GithubProvider,
    identity: &HostIdentity,
    parent: &MetricsPrObservation,
    limits: &MetricsReadLimits,
) -> GithubResult<usize> {
    preflight_repository(provider, identity, &parent.repository)?;
    if parent.id == 0
        || parent.number == 0
        || !valid_opaque(&parent.node_id)
        || parent
            .head_sha
            .as_ref()
            .is_none_or(|head| !crate::search::model::valid_sha(head))
    {
        return Err(error(
            GithubErrorKind::InvalidTarget,
            "Scope receipt requires genuine original positive PR IDs/node/number/full head",
        ));
    }
    // Bounded write sink fails before any oversized original is cloned or sent.
    struct Count {
        bytes: usize,
        limit: usize,
    }
    impl std::io::Write for Count {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.bytes = self
                .bytes
                .checked_add(bytes.len())
                .ok_or_else(|| std::io::Error::other("Original scope metadata overflow"))?;
            if self.bytes > self.limit {
                return Err(std::io::Error::other(
                    "Original scope metadata exceeds document cap",
                ));
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count {
        bytes: 0,
        limit: limits.max_document_bytes,
    };
    serde_json::to_writer(&mut count, parent).map_err(|_| {
        error(
            GithubErrorKind::InvalidTarget,
            "Original canonical parent exceeds bounded scope metadata",
        )
    })?;
    Ok(count.bytes)
}
impl GithubProvider {
    /// Exactly five successful reads, no history enumeration or SQL authority.
    pub fn metric_scope_receipt(
        &self,
        identity: &HostIdentity,
        original: &MetricsPrObservation,
        limits: &MetricsReadLimits,
        cancel: &CancellationToken,
    ) -> GithubResult<MetricsScopeReceipt> {
        let mut batch =
            self.metric_scope_receipts(identity, std::slice::from_ref(original), limits, cancel)?;
        Ok(batch.receipts.remove(0))
    }
    /// Exactly 3P+2 successful reads for the explicit unique parent selection.
    pub fn metric_scope_receipts(
        &self,
        identity: &HostIdentity,
        originals: &[MetricsPrObservation],
        limits: &MetricsReadLimits,
        cancel: &CancellationToken,
    ) -> GithubResult<MetricsScopeReceiptBatch> {
        self.metric_scope_receipts_with_repositories(identity, originals, &[], limits, cancel)
    }
    /// Exactly 3P+Q+2: extra headers not represented by selected parents receive
    /// one real canonical repository GET under the same principal bracket.
    pub fn metric_scope_receipts_with_repositories(
        &self,
        identity: &HostIdentity,
        originals: &[MetricsPrObservation],
        extras: &[MetricsRepository],
        limits: &MetricsReadLimits,
        cancel: &CancellationToken,
    ) -> GithubResult<MetricsScopeReceiptBatch> {
        limits.validate()?;
        if originals.len() > MAX_PARENTS
            || extras.len() > MAX_REPOSITORIES
            || originals.is_empty() && extras.is_empty()
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Scope receipt batch requires bounded explicit canonical parents/repositories",
            ));
        }
        let mut repositories: BTreeMap<String, MetricsRepository> = BTreeMap::new();
        let mut parent_ids = BTreeSet::new();
        let mut parent_nodes = BTreeSet::new();
        let mut parent_numbers = BTreeSet::new();
        let mut original_metadata = 0usize;
        for parent in originals {
            original_metadata = original_metadata.checked_add(preflight_parent(self, identity, parent, limits)?)
                .filter(|bytes| *bytes <= MAX_ORIGINAL_METADATA)
                .ok_or_else(|| error(GithubErrorKind::InvalidTarget, "Explicit original parent group exceeds aggregate metadata budget before cloning or transport"))?;
            if !parent_ids.insert((parent.repository.id, parent.id))
                || !parent_nodes.insert(parent.node_id.clone())
                || !parent_numbers.insert((parent.repository.full_name.clone(), parent.number))
            {
                return Err(error(
                    GithubErrorKind::InvalidTarget,
                    "Scope batch repeats or contradicts genuine canonical parent membership",
                ));
            }
            if let Some(previous) = repositories.insert(
                parent.repository.full_name.clone(),
                parent.repository.clone(),
            ) {
                require_same_repo(&previous, &parent.repository)?;
            }
        }
        for repo in extras {
            preflight_repository(self, identity, repo)?;
            if repositories
                .insert(repo.full_name.clone(), repo.clone())
                .is_some()
            {
                return Err(error(GithubErrorKind::InvalidTarget, "Extra checked headers must be unique and disjoint from selected parent repositories"));
            }
        }
        let mut repo_ids = BTreeSet::new();
        let mut repo_nodes = BTreeSet::new();
        if repositories.len() > MAX_REPOSITORIES
            || repositories
                .values()
                .any(|repo| !repo_ids.insert(repo.id) || !repo_nodes.insert(repo.node_id.clone()))
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Scope batch contradicts canonical repository database/node membership",
            ));
        }
        let mut budget = MetricsBudget::new(limits)?;
        let identity = fresh_principal(self, &mut budget, identity, cancel)?;
        let mut verified: BTreeMap<String, Instant> = BTreeMap::new();
        let mut parents = Vec::with_capacity(originals.len());
        for original in originals {
            let endpoint = format!("repos/{}", original.repository.full_name);
            let repo = repository(
                &self.host,
                &original.repository.full_name,
                &budget.read_document(self, &endpoint, cancel)?,
            )?;
            require_same_repo(&original.repository, &repo)?;
            let parent = parse_pr(
                &repo,
                &budget.read_document(
                    self,
                    &format!("{endpoint}/pulls/{}", original.number),
                    cancel,
                )?,
            )?;
            require_same_parent(original, &parent)?;
            let parent_verified_at = Instant::now();
            let final_repo = repository(
                &self.host,
                &repo.full_name,
                &budget.read_document(self, &endpoint, cancel)?,
            )?;
            require_same_repo(&repo, &final_repo)?;
            let repo_verified_at = Instant::now();
            verified.insert(repo.full_name, repo_verified_at);
            parents.push((parent, parent_verified_at));
        }
        for original in extras {
            let endpoint = format!("repos/{}", original.full_name);
            let repo = repository(
                &self.host,
                &original.full_name,
                &budget.read_document(self, &endpoint, cancel)?,
            )?;
            require_same_repo(original, &repo)?;
            verified.insert(repo.full_name, Instant::now());
        }
        fresh_principal(self, &mut budget, &identity, cancel)?;
        let accounting = MetricsReadAccounting::from_budget(&budget, &[])?;
        let observed_interval = (budget.observed_from, budget.observed_through()?);
        let receipts = parents
            .into_iter()
            .map(|(parent, verified_at)| {
                MetricsScopeReceipt::from_successful_reads(
                    identity.clone(),
                    parent,
                    &budget,
                    &[],
                    verified_at,
                    cancel,
                )
            })
            .collect::<GithubResult<Vec<_>>>()?;
        let mut checked = vec![];
        let mut clocks = vec![];
        for repo in repositories.into_values() {
            let instant = verified.get(&repo.full_name).copied().ok_or_else(|| {
                error(
                    GithubErrorKind::IdentityChanged,
                    "No actual scope observation for original header",
                )
            })?;
            validate_age(instant, METRICS_SCOPE_RECEIPT_MAX_AGE, cancel)?;
            checked.push(repo);
            clocks.push(instant);
        }
        Ok(MetricsScopeReceiptBatch {
            identity,
            receipts,
            repositories: checked,
            repository_verified_at: clocks,
            accounting,
            observed_interval,
        })
    }
}

#[cfg(test)]
mod accounting_tests {
    use super::*;

    #[test]
    fn metrics_scope_accounting_overflow_refuses_instead_of_saturating_a_proof() {
        let mut budget = MetricsBudget::new(&MetricsReadLimits::default()).unwrap();
        budget.bytes = usize::MAX;
        budget.successful_diagnostic_bytes = 1;
        assert!(matches!(
            MetricsReadAccounting::from_budget(&budget, &[])
                .unwrap_err()
                .kind,
            GithubErrorKind::Transport
        ));
    }

    #[test]
    fn metrics_scope_receipt_fixed_monotonic_expiry_cannot_be_renewed_by_a_new_validation() {
        let verified_at = Instant::now().checked_sub(Duration::from_secs(6)).unwrap();
        let token = CancellationToken::default();
        assert!(validate_age(verified_at, METRICS_SCOPE_RECEIPT_MAX_AGE, &token).is_err());
        assert!(validate_age(verified_at, Duration::from_secs(1), &token).is_err());
        assert!(validate_age(Instant::now(), Duration::from_secs(6), &token).is_err());
    }
}
