//! Source-only available API-history producer. Declare at the coordinated window.
//! No public DTO/complete bool can construct these reservation-bound proofs.
use super::*;
use sha2::Digest;
use std::fmt;
#[path = "history/scope.rs"]
mod scope_receipts;
pub use scope_receipts::*;

pub const API_VISIBLE_HISTORY_LIMITATIONS: &[&str] = &[
    "API-visible retained backfill only; deleted/erased/unavailable events remain unknown",
    "Bounded REST category traversal is not an atomic server snapshot or a full historical trend",
];
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum MetricsBackfillCategory {
    Reviews,
    Timeline,
}
#[derive(Clone, PartialEq, Eq)]
pub struct MetricsBackfillBinding {
    pub(crate) source: String,
    pub(crate) generation: u64,
    pub(crate) intent_sha256: String,
    pub(crate) issuer_sha256: String,
    pub(crate) requested_from: u64,
    pub(crate) requested_through: u64,
}
#[derive(Clone)]
pub struct MetricsBackfillRequest {
    identity: HostIdentity,
    parent: MetricsPrObservation,
    category: MetricsBackfillCategory,
    binding: MetricsBackfillBinding,
}
impl fmt::Debug for MetricsBackfillRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("MetricsBackfillRequest(<sealed original reservation>)")
    }
}
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
impl MetricsBackfillRequest {
    /// Storage-only issuer after actual current SQL reservation validation.
    /// Original storage fingerprints never appear in GitHub argv or request bodies.
    pub(crate) fn issue(
        identity: &HostIdentity,
        parent: &MetricsPrObservation,
        category: MetricsBackfillCategory,
        binding: MetricsBackfillBinding,
    ) -> GithubResult<Self> {
        let created = parent.created_at.as_ref().ok_or_else(|| {
            error(
                GithubErrorKind::InvalidTarget,
                "Original canonical PR creation timestamp unavailable; backfill cannot mint lifetime",
            )
        })?;
        if identity.account_id.is_none_or(|id| id == 0)
            || identity.host != parent.repository.host
            || parent.repository.id == 0
            || parent.id == 0
            || parent.number == 0
            || !valid_opaque(&parent.repository.node_id)
            || !valid_opaque(&parent.node_id)
            || parent
                .head_sha
                .as_ref()
                .is_none_or(|sha| !crate::search::model::valid_sha(sha))
            || binding.generation == 0
            || binding.generation > i64::MAX as u64
            || binding.requested_from != created.unix_seconds()
            || binding.requested_through < binding.requested_from
            || binding
                .source
                .strip_prefix("metrics-events-v1:")
                .is_none_or(|value| !hash(value))
            || !hash(&binding.intent_sha256)
            || !hash(&binding.issuer_sha256)
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Backfill requires the exact original positive reservation/parent/created-to-as_of intent",
            ));
        }
        validate_repo(&parent.repository.full_name)?;
        validate_name(&identity.login)?;
        Ok(Self {
            identity: identity.clone(),
            parent: parent.clone(),
            category,
            binding,
        })
    }
    pub fn category(&self) -> MetricsBackfillCategory {
        self.category
    }
    pub fn interval(&self) -> (u64, u64) {
        (self.binding.requested_from, self.binding.requested_through)
    }
    pub(crate) fn binding(&self) -> &MetricsBackfillBinding {
        &self.binding
    }
}
pub struct MetricsBackfillRequestPair {
    reviews: MetricsBackfillRequest,
    timeline: MetricsBackfillRequest,
}
impl fmt::Debug for MetricsBackfillRequestPair {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("MetricsBackfillRequestPair(<sealed exact-parent categories>)")
    }
}
impl MetricsBackfillRequestPair {
    pub fn new(
        reviews: MetricsBackfillRequest,
        timeline: MetricsBackfillRequest,
    ) -> GithubResult<Self> {
        if reviews.category != MetricsBackfillCategory::Reviews
            || timeline.category != MetricsBackfillCategory::Timeline
            || !reviews.identity.same_account(&timeline.identity)
            || reviews.parent != timeline.parent
            || reviews.interval() != timeline.interval()
            || reviews.binding.issuer_sha256 != timeline.binding.issuer_sha256
            || reviews.binding.source == timeline.binding.source
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Backfill pair must have the same original principal/issuer/parent/cutoff and distinct categories",
            ));
        }
        Ok(Self { reviews, timeline })
    }
}

/// Reservation-bound evidence created ONLY by actual complete transport traversal.
/// Not Serialize/Deserialize, and no public constructor or setters exist.
pub struct ApiVisibleMetricCategoryProof {
    request: MetricsBackfillRequest,
    observation_sha256: String,
    observed_from: u64,
    observed_through: u64,
}
impl fmt::Debug for ApiVisibleMetricCategoryProof {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ApiVisibleMetricCategoryProof(<verified API-visible original read>)")
    }
}
impl ApiVisibleMetricCategoryProof {
    pub fn category(&self) -> MetricsBackfillCategory {
        self.request.category
    }
    pub fn interval(&self) -> (u64, u64) {
        self.request.interval()
    }
    pub fn limitations(&self) -> &'static [&'static str] {
        API_VISIBLE_HISTORY_LIMITATIONS
    }
    pub(crate) fn binding(&self) -> &MetricsBackfillBinding {
        self.request.binding()
    }
    pub(crate) fn validates_observation(&self, value: &ExactPrMetricEvents) -> GithubResult<()> {
        if self.observed_from != value.observed_from
            || self.observed_through != value.observed_through
            || !self.request.identity.same_account(&value.identity)
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Backfill observation scope/clock differs from original proof",
            ));
        }
        require_same_parent(&self.request.parent, &value.parent)?;
        if category_digest(value, self.category())? != self.observation_sha256 {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Backfill event membership/content differs from original traversal",
            ));
        }
        Ok(())
    }
}
pub struct ApiVisibleMetricBackfill {
    observations: ExactPrMetricEvents,
    reviews: Option<ApiVisibleMetricCategoryProof>,
    timeline: Option<ApiVisibleMetricCategoryProof>,
    scope_receipt: MetricsScopeReceipt,
}
impl fmt::Debug for ApiVisibleMetricBackfill {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ApiVisibleMetricBackfill(<bounded API-visible observations>)")
    }
}
impl ApiVisibleMetricBackfill {
    pub fn scope_receipt(&self) -> &MetricsScopeReceipt {
        &self.scope_receipt
    }
    pub fn observations(&self) -> &ExactPrMetricEvents {
        &self.observations
    }
    pub fn category_proof(
        &self,
        category: MetricsBackfillCategory,
    ) -> Option<&ApiVisibleMetricCategoryProof> {
        match category {
            MetricsBackfillCategory::Reviews => self.reviews.as_ref(),
            MetricsBackfillCategory::Timeline => self.timeline.as_ref(),
        }
    }
}
fn category_digest(
    value: &ExactPrMetricEvents,
    category: MetricsBackfillCategory,
) -> GithubResult<String> {
    let rows = match category {
        MetricsBackfillCategory::Reviews => serde_json::to_value(&value.reviews),
        MetricsBackfillCategory::Timeline => serde_json::to_value(&value.timeline),
    }
    .map_err(|failure| error(GithubErrorKind::Json, failure.to_string()))?;
    let coverage = match category {
        MetricsBackfillCategory::Reviews => &value.review_coverage,
        MetricsBackfillCategory::Timeline => &value.timeline_coverage,
    };
    let status = match &coverage.status {
        MetricsReadStatus::Complete => "complete".to_owned(),
        MetricsReadStatus::Partial => "partial".to_owned(),
        MetricsReadStatus::Unavailable(failure) => format!("unavailable:{:?}", failure.kind),
    };
    let metadata = json!({
        "identity":value.identity,"parent":value.parent,"rows":rows,
        "observed_from":value.observed_from,"observed_through":value.observed_through,
        "requests":value.requests,"bytes":value.aggregate_response_bytes,
        "coverage":{
            "enumeration":coverage.enumeration_complete,"semantic":coverage.metric_evidence_complete,
            "empty":coverage.complete_empty,"requested_pages":coverage.requested_pages,
            "pages":coverage.fetched_pages,"raw_rows":coverage.fetched_rows,
            "admitted":coverage.admitted_rows,"excluded":coverage.excluded_rows,
            "bytes":coverage.response_bytes,"reasons":coverage.reasons,"status":status
        }
    });
    Ok(format!(
        "{:x}",
        sha2::Sha256::digest(
            serde_json::to_vec(&metadata)
                .map_err(|failure| error(GithubErrorKind::Json, failure.to_string()))?
        )
    ))
}
fn fresh_principal(
    provider: &GithubProvider,
    budget: &mut MetricsBudget,
    expected: &HostIdentity,
    cancel: &CancellationToken,
) -> GithubResult<HostIdentity> {
    let value = budget.read_document(provider, "user", cancel)?;
    let current = HostIdentity {
        host: provider.host.clone(),
        login: text(&value, "login"),
        account_id: Some(positive(&value, "id")?),
        gh_version: expected.gh_version.clone(),
    };
    validate_name(&current.login)?;
    if !current.same_account(expected) {
        return Err(error(
            GithubErrorKind::IdentityChanged,
            "Backfill original positive principal changed; no available-history proof issued",
        ));
    }
    Ok(current)
}
fn metric_rows_before_as_of(
    value: &ExactPrMetricEvents,
    category: MetricsBackfillCategory,
    from: u64,
) -> bool {
    match category {
        MetricsBackfillCategory::Reviews => value.reviews.iter().all(|review| {
            matches!(review.state, MetricsReviewState::Pending)
                || review.submitted_at.as_ref().is_some_and(|at| {
                    at.unix_seconds() >= from && at.unix_seconds() <= value.observed_through
                })
        }),
        MetricsBackfillCategory::Timeline => value.timeline.iter().all(|event| {
            matches!(event.kind, MetricsTimelineKind::Unrelated(_))
                || event.at.as_ref().is_some_and(|at| {
                    at.unix_seconds() >= from && at.unix_seconds() <= value.observed_through
                })
        }),
    }
}
impl GithubProvider {
    /// Actual original-reservation-bound API-visible backfill. This entry point
    /// explicitly adds bounded fresh `gh api user` brackets, not separate auth.
    pub fn metric_history_backfill(
        &self,
        pair: &MetricsBackfillRequestPair,
        limits: &MetricsReadLimits,
        cancel: &CancellationToken,
    ) -> GithubResult<ApiVisibleMetricBackfill> {
        let expected = &pair.reviews.parent;
        scope(self, &pair.reviews.identity, &expected.repository.full_name)?;
        limits.validate()?;
        let mut budget = MetricsBudget::new(limits)?;
        if pair.reviews.binding.requested_through > budget.observed_from {
            return Err(error(GithubErrorKind::InvalidTarget,"Original backfill as_of must be fixed before traversal, never moved to an observed end clock"));
        }
        let identity = fresh_principal(self, &mut budget, &pair.reviews.identity, cancel)?;
        let repo_endpoint = format!("repos/{}", expected.repository.full_name);
        let parent_endpoint = format!("{repo_endpoint}/pulls/{}", expected.number);
        let repo = repository(
            &self.host,
            &expected.repository.full_name,
            &budget.read_document(self, &repo_endpoint, cancel)?,
        )?;
        require_same_repo(&expected.repository, &repo)?;
        let parent = parse_pr(
            &repo,
            &budget.read_document(self, &parent_endpoint, cancel)?,
        )?;
        require_same_parent(expected, &parent)?;
        let (reviews, review_coverage) = budget.collect(
            self,
            &format!("{parent_endpoint}/reviews"),
            |raw| {
                let item = parse_review(&parent, raw)?;
                Ok((item.node_id.clone(), item.clone(), item.reasons.clone()))
            },
            cancel,
        )?;
        let mut review_ids = BTreeSet::new();
        if reviews.iter().any(|review| !review_ids.insert(review.id)) {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Backfill reviews contradict numeric/opaque identities",
            ));
        }
        let mut unidentified = 0usize;
        let (timeline, timeline_coverage) = budget.collect(
            self,
            &format!("{repo_endpoint}/issues/{}/timeline", parent.number),
            |raw| {
                let item = parse_timeline(&parent, raw)?;
                let key = if let Some(node) = &item.node_id {
                    format!("node:{node}")
                } else if let Some(id) = item.id {
                    format!("numeric:{id}")
                } else {
                    unidentified += 1;
                    format!("unknown:{unidentified}")
                };
                Ok((key, item.clone(), item.reasons.clone()))
            },
            cancel,
        )?;
        let mut timeline_ids = BTreeSet::new();
        if timeline
            .iter()
            .filter_map(|event| event.id)
            .any(|id| !timeline_ids.insert(id))
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Backfill timeline contradicts numeric/opaque identities",
            ));
        }
        let final_repo = repository(
            &self.host,
            &expected.repository.full_name,
            &budget.read_document(self, &repo_endpoint, cancel)?,
        )?;
        let final_scope_verified_at = std::time::Instant::now();
        require_same_repo(&repo, &final_repo)?;
        let final_parent = parse_pr(
            &repo,
            &budget.read_document(self, &parent_endpoint, cancel)?,
        )?;
        require_same_parent(&parent, &final_parent)?;
        fresh_principal(self, &mut budget, &identity, cancel)?;
        let scope_receipt = MetricsScopeReceipt::from_successful_reads(
            identity.clone(),
            final_parent.clone(),
            &budget,
            &[&review_coverage, &timeline_coverage],
            final_scope_verified_at,
            cancel,
        )?;
        let observations = ExactPrMetricEvents {
            identity,
            parent: final_parent,
            reviews,
            review_coverage,
            timeline,
            timeline_coverage,
            observed_from: budget.observed_from,
            observed_through: budget.observed_through()?,
            requests: budget.requests,
            aggregate_response_bytes: budget.bytes,
        };
        let mint = |request: &MetricsBackfillRequest,
                    coverage: &MetricsReadCoverage|
         -> GithubResult<Option<ApiVisibleMetricCategoryProof>> {
            if !coverage.enumeration_complete
                || !coverage.metric_evidence_complete
                || !matches!(coverage.status, MetricsReadStatus::Complete)
                || !metric_rows_before_as_of(
                    &observations,
                    request.category,
                    request.binding.requested_from,
                )
            {
                return Ok(None);
            }
            Ok(Some(ApiVisibleMetricCategoryProof {
                request: request.clone(),
                observation_sha256: category_digest(&observations, request.category)?,
                observed_from: observations.observed_from,
                observed_through: observations.observed_through,
            }))
        };
        let reviews = mint(&pair.reviews, &observations.review_coverage)?;
        let timeline = mint(&pair.timeline, &observations.timeline_coverage)?;
        Ok(ApiVisibleMetricBackfill {
            observations,
            reviews,
            timeline,
            scope_receipt,
        })
    }
}
