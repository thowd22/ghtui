//! Bounded PR metadata catalogue. Its metrics child declaration permits private
//! reuse of the common transport budget without widening it.
//! This observation is NOT authentication, journal or publication authority.
use super::*;
#[path = "metadata_catalogue/members_validation_v48.rs"]
mod members_validation_v48;
#[path = "metadata_catalogue/original_members_v48.rs"]
mod original_members_v48;
#[path = "metadata_catalogue/actions.rs"]
mod publication_actions;
pub(crate) use original_members_v48::{
    ActualMembersPageContextV61, MembersMetadataHistoryReadV62, MembersWriteV48,
    MetadataMembersObservationV48, MetadataMembersReadPurposeV48,
    MetadataMembersRecoveryCurrentV48, OriginalMetadataMembersV48, PrMetadataMembersFailureV48,
    PrMetadataMembersReceiptV48, METADATA_MEMBERS_POST_CAPS_V48, METADATA_MEMBERS_REFRESH_CAPS_V48,
    METADATA_MEMBERS_TAB_POST_CAPS_V62, METADATA_MEMBERS_TAB_REFRESH_CAPS_V62,
};
pub(in crate::providers::gh) use original_members_v48::{
    MembersVisibilityV48, MetadataMembersContextV48, PreparedMembersRequestV48,
};
pub use publication_actions::{
    pr_metadata_members_query, PrMetadataPublicationSession, PrMetadataPublished,
    PR_METADATA_REQUEST_REVIEWS_MUTATION, PR_METADATA_UPDATE_MUTATION,
};
#[path = "metadata_catalogue/branch.rs"]
mod branch;
#[path = "metadata_catalogue/context.rs"]
pub(in crate::providers::gh) mod context;
#[path = "metadata_catalogue/model.rs"]
mod model;
#[path = "metadata_catalogue/paging.rs"]
mod paging;
#[path = "metrics/pr_checks_bridge.rs"]
mod pr_checks_bridge;
use context::*;
pub use model::*;
use paging::*;
pub use pr_checks_bridge::*;

impl GithubProvider {
    /// Own N+7 read protocol, N actual fixed catalogue page requests. No legacy
    /// list/context calls, discovery, or mutation is hidden inside this method.
    pub fn metadata_catalogue_read(
        &self,
        identity: &HostIdentity,
        original: &PrSummary,
        original_context: &PrContext,
        kind: MetadataChoices,
        limits: &PrMetadataReadLimits,
        cancel: &CancellationToken,
    ) -> GithubResult<PrMetadataCatalogueRead> {
        let selected = limits.as_metrics()?;
        preflight(self, identity, original, original_context)?;
        let mut budget = MetricsBudget::new(&selected)?;
        let requester = principal(self, &mut budget, identity, cancel)?;
        let name = original.pr.repository.to_ascii_lowercase();
        let repo_endpoint = format!("repos/{name}");
        let parent_endpoint = format!("{repo_endpoint}/pulls/{}", original.pr.number);
        let issue_endpoint = format!("{repo_endpoint}/issues/{}", original.pr.number);
        let repository_raw = budget.read_document(self, &repo_endpoint, cancel)?;
        let canonical_repo = repository(&self.host, &name, &repository_raw)?;
        if canonical_repo.id != original_context.repository.id {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Original catalogue repository database identity was replaced",
            ));
        }
        let parent_raw = budget.read_document(self, &parent_endpoint, cancel)?;
        parent_identity(self, original, &canonical_repo, &parent_raw)?;
        let issue_raw = budget.read_document(self, &issue_endpoint, cancel)?;
        issue_identity(self, original, &issue_raw)?;
        let (rows, mut coverage) =
            collect(self, &mut budget, &repo_endpoint, kind, limits, cancel)?;
        let final_repo_raw = budget.read_document(self, &repo_endpoint, cancel)?;
        let final_repo = repository(&self.host, &name, &final_repo_raw)?;
        require_same_repo(&canonical_repo, &final_repo)?;
        let final_parent_raw = budget.read_document(self, &parent_endpoint, cancel)?;
        parent_identity(self, original, &final_repo, &final_parent_raw)?;
        require_same_context(&parent_raw, &final_parent_raw, &issue_raw)?;
        let requester = principal(self, &mut budget, &requester, cancel)?;
        let (target, context, reasons) = observed_context(
            self,
            original,
            &final_repo_raw,
            &final_parent_raw,
            issue_raw,
        )?;
        if !reasons.is_empty() {
            coverage.metric_evidence_complete = false;
            coverage.reasons.extend(reasons);
        }
        finish(&mut coverage);
        let accounting = PrMetadataReadAccounting {
            requests: budget.requests,
            successful_stdout_bytes: budget.bytes,
            successful_diagnostic_bytes: budget.successful_diagnostic_bytes,
            failed_reserved_bytes: budget.failed_reserved_bytes,
        };
        Ok(PrMetadataCatalogueRead {
            identity: requester,
            target,
            context,
            kind,
            rows,
            coverage,
            repository: final_repo,
            parent_raw: final_parent_raw,
            repository_raw: final_repo_raw,
            observed_interval: (budget.observed_from, budget.observed_through()?),
            accounting,
        })
    }
}
