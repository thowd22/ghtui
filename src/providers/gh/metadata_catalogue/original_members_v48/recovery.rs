//! Fresh read-only current membership for exact local Inspect/ACK. This type
//! cannot be constructed from a record, receipt, DTO, known ID or saved origin.
use super::*;
fn inspection_purpose_v242(purpose: MetadataMembersReadPurposeV48) -> bool {
    matches!(
        purpose,
        MetadataMembersReadPurposeV48::Inspection
            | MetadataMembersReadPurposeV48::MetadataInspectionV62
    )
}
pub(crate) struct MetadataMembersRecoveryCurrentV48 {
    actual: Arc<OriginalMetadataMembersV48>,
}
impl MetadataMembersRecoveryCurrentV48 {
    pub(crate) fn actual(&self) -> &Arc<OriginalMetadataMembersV48> {
        &self.actual
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.actual.deadline
    }
    pub(crate) fn cancellation(&self) -> &CancellationToken {
        &self.actual.cancellation
    }
    pub(crate) fn matches_target(&self, target: &crate::pr_write_journal::PrWriteTarget) -> bool {
        let parent = self.actual.parent();
        crate::storage::Scope::github(
            &self.actual.identity().host,
            self.actual.identity().account_id,
        )
        .is_ok_and(|scope| &scope == target.scope())
            && parent.repository() == target.repository()
            && parent.node_id() == target.node_id()
            && parent.number() == target.number()
    }
}
impl GithubProvider {
    pub(crate) fn inspect_original_metadata_members_v48(
        &self,
        actual: Arc<OriginalMetadataMembersV48>,
        cancel: &CancellationToken,
    ) -> GithubResult<MetadataMembersRecoveryCurrentV48> {
        let (pool, deadline) = measured(self, cancel)?;
        if !inspection_purpose_v242(actual.purpose) || !Arc::ptr_eq(&actual.pool, &pool) {
            return Err(refused(
                GithubErrorKind::InvalidTarget,
                "Actual read-only member inspection required",
            ));
        }
        actual.validate_lineage(self, cancel, deadline)?;
        Ok(MetadataMembersRecoveryCurrentV48 { actual })
    }
    pub(crate) fn validate_metadata_members_inspection_v48(
        &self,
        observed: &MetadataMembersRecoveryCurrentV48,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        if !inspection_purpose_v242(observed.actual.purpose) {
            return Err(refused(
                GithubErrorKind::InvalidTarget,
                "Wrong member inspection purpose",
            ));
        }
        observed
            .actual
            .validate_lineage(self, cancel, deadline.min(observed.deadline()))
    }
    pub(crate) fn reconcile_metadata_members_inspection_v48(
        &self,
        observed: MetadataMembersRecoveryCurrentV48,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        self.validate_metadata_members_inspection_v48(&observed, cancel, deadline)?;
        let parent = observed.actual.parent();
        self.runner
            .reconcile_review_generation_v11(
                &self.resource(parent.repository(), parent.number()),
                observed.actual.runner_generation,
                cancel,
                deadline.min(observed.deadline()),
            )
            .map_err(GithubError::from)
    }
}

#[cfg(test)]
mod purpose_v242_tests {
    use super::*;
    #[test]
    fn pr_metadata_members_v242_inspection_purpose_excludes_both_eligible_families() {
        assert!(inspection_purpose_v242(
            MetadataMembersReadPurposeV48::Inspection
        ));
        assert!(inspection_purpose_v242(
            MetadataMembersReadPurposeV48::MetadataInspectionV62
        ));
        assert!(!inspection_purpose_v242(
            MetadataMembersReadPurposeV48::Eligible
        ));
        assert!(!inspection_purpose_v242(
            MetadataMembersReadPurposeV48::MetadataEligibleV62
        ));
    }
}
