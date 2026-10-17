//! Repository-wide metric evidence from bounded canonical gh reads.
//! API enumeration is not retained historical/backfill authority or a generation.
use super::*;
mod access;
#[path = "metadata_catalogue.rs"]
mod metadata_catalogue;
pub(in crate::providers::gh) use metadata_catalogue::{
    MembersVisibilityV48, MetadataMembersContextV48, PreparedMembersRequestV48,
};
mod model;
mod paging;
mod parse;
mod threads;
pub use metadata_catalogue::*;
// Pure validators/converter only; no MetricsBudget/transport/source escapes.
pub(in crate::providers::gh) use metadata_catalogue::context::{
    issue_identity as metadata_context_issue_identity_v51,
    observed_context as metadata_context_observed_v51,
    require_same_context as metadata_context_consistent_v51,
};
pub(crate) use metadata_catalogue::{
    ActualMembersPageContextV61, MembersMetadataHistoryReadV62, MembersWriteV48,
    MetadataMembersObservationV48, MetadataMembersReadPurposeV48,
    MetadataMembersRecoveryCurrentV48, OriginalMetadataMembersV48, PrMetadataMembersFailureV48,
    PrMetadataMembersReceiptV48, METADATA_MEMBERS_POST_CAPS_V48, METADATA_MEMBERS_REFRESH_CAPS_V48,
    METADATA_MEMBERS_TAB_POST_CAPS_V62, METADATA_MEMBERS_TAB_REFRESH_CAPS_V62,
};
pub use model::*;
use paging::*;
use parse::*;
pub use threads::*;

impl GithubProvider {
    /// Explicit selected-repository ALL-state inventory, never a personal queue
    /// union or search endpoint denominator. Root owns fresh principal brackets.
    pub fn repository_pr_inventory(
        &self,
        identity: &HostIdentity,
        name: &str,
        expected: Option<&MetricsRepository>,
        limits: &MetricsReadLimits,
        cancel: &CancellationToken,
    ) -> GithubResult<RepositoryPrInventory> {
        scope(self, identity, name)?;
        limits.validate()?;
        if let Some(expected) = expected {
            if expected.host != self.host
                || !expected.full_name.eq_ignore_ascii_case(name)
                || expected.id == 0
                || !valid_opaque(&expected.node_id)
            {
                return Err(error(
                    GithubErrorKind::InvalidTarget,
                    "Pinned metric repository is outside the current canonical scope",
                ));
            }
        }
        let mut budget = MetricsBudget::new(limits)?;
        let endpoint = format!("repos/{name}");
        let repo = repository(
            &self.host,
            name,
            &budget.read_document(self, &endpoint, cancel)?,
        )?;
        if let Some(expected) = expected {
            require_same_repo(expected, &repo)?;
        }
        let (prs, coverage) = budget.collect(
            self,
            &format!("{endpoint}/pulls?state=all&sort=created&direction=desc"),
            |raw| {
                let item = parse_pr(&repo, raw)?;
                let key = item.node_id.clone();
                let reasons = item.reasons.clone();
                Ok((key, item, reasons))
            },
            cancel,
        )?;
        let mut numeric = BTreeSet::new();
        let mut numbers = BTreeSet::new();
        if prs
            .iter()
            .any(|pr| !numeric.insert(pr.id) || !numbers.insert(pr.number))
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Repository PR inventory has contradictory numeric/node/number identities",
            ));
        }
        let final_repo = repository(
            &self.host,
            name,
            &budget.read_document(self, &endpoint, cancel)?,
        )?;
        require_same_repo(&repo, &final_repo)?;
        Ok(RepositoryPrInventory {
            identity: identity.clone(),
            repository: repo,
            prs,
            coverage,
            observed_from: budget.observed_from,
            observed_through: budget.observed_through()?,
            requests: budget.requests,
            aggregate_response_bytes: budget.bytes,
        })
    }
    /// Current exact-parent review/timeline collections. No actual retained
    /// generation or lifetime/backfill interval is manufactured by these reads.
    pub fn pr_metric_events(
        &self,
        identity: &HostIdentity,
        original: &MetricsPrObservation,
        limits: &MetricsReadLimits,
        cancel: &CancellationToken,
    ) -> GithubResult<ExactPrMetricEvents> {
        let repo = &original.repository;
        scope(self, identity, &repo.full_name)?;
        limits.validate()?;
        if repo.host != self.host
            || repo.id == 0
            || !valid_opaque(&repo.node_id)
            || original.id == 0
            || original.number == 0
            || !valid_opaque(&original.node_id)
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Metric events need the original positive canonical repository and PR node/IDs",
            ));
        }
        let mut budget = MetricsBudget::new(limits)?;
        let repo_endpoint = format!("repos/{}", repo.full_name);
        let fresh_repo = repository(
            &self.host,
            &repo.full_name,
            &budget.read_document(self, &repo_endpoint, cancel)?,
        )?;
        require_same_repo(repo, &fresh_repo)?;
        let parent_endpoint = format!("{repo_endpoint}/pulls/{}", original.number);
        let parent = parse_pr(repo, &budget.read_document(self, &parent_endpoint, cancel)?)?;
        require_same_parent(original, &parent)?;
        let (reviews, review_coverage) = budget.collect(
            self,
            &format!("{parent_endpoint}/reviews"),
            |raw| {
                let item = parse_review(&parent, raw)?;
                let key = item.node_id.clone();
                let reasons = item.reasons.clone();
                Ok((key, item, reasons))
            },
            cancel,
        )?;
        let mut review_ids = BTreeSet::new();
        if reviews.iter().any(|review| !review_ids.insert(review.id)) {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Review inventory has contradictory numeric/opaque identities",
            ));
        }
        let mut unknown_ordinal = 0usize;
        let (timeline, timeline_coverage) = budget.collect(
            self,
            &format!("{repo_endpoint}/issues/{}/timeline", original.number),
            |raw| {
                let item = parse_timeline(&parent, raw)?;
                // Unknown/no-ID observations remain distinct and semantic-partial;
                // never fabricate a numeric ID to make them metric contributors.
                let key = if let Some(node) = &item.node_id {
                    format!("node:{node}")
                } else if let Some(id) = item.id {
                    format!("numeric:{id}")
                } else {
                    unknown_ordinal += 1;
                    format!("unidentified-observation:{unknown_ordinal}")
                };
                let reasons = item.reasons.clone();
                Ok((key, item, reasons))
            },
            cancel,
        )?;
        let mut event_ids = BTreeSet::new();
        if timeline
            .iter()
            .filter_map(|event| event.id)
            .any(|id| !event_ids.insert(id))
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Timeline inventory has contradictory numeric/opaque event identities",
            ));
        }
        let final_repo = repository(
            &self.host,
            &repo.full_name,
            &budget.read_document(self, &repo_endpoint, cancel)?,
        )?;
        require_same_repo(repo, &final_repo)?;
        let final_parent = parse_pr(repo, &budget.read_document(self, &parent_endpoint, cancel)?)?;
        require_same_parent(&parent, &final_parent)?;
        Ok(ExactPrMetricEvents {
            identity: identity.clone(),
            parent: final_parent,
            reviews,
            review_coverage,
            timeline,
            timeline_coverage,
            observed_from: budget.observed_from,
            observed_through: budget.observed_through()?,
            requests: budget.requests,
            aggregate_response_bytes: budget.bytes,
        })
    }
}
