//! Fixed Metadata-Members paid roles on the SAME once-reserved22 cursor.
//! No arbitrary endpoint/body/provider/cursor constructor or secondary client.
use super::*;
use crate::pr_metadata_actions::PrMetadataPlan;
use crate::providers::gh::pr_resources_v1::metadata_context_v51::{
    MetadataContextDocumentsV51, MetadataMembersHeapV62, MetadataRetainedDebitV51,
    MetadataSummaryDocumentV51,
};
use crate::providers::gh::{
    ActualMetadataTabContextV51, ConversationDetailSnapshotV19, OriginalMetadataMembersV48,
    PrMetadataMembersReceiptV48,
};

pub(in crate::providers::gh) enum MembersMetadataRoleV62 {
    ReceiptRepository,
    ReceiptParent,
    ReceiptIssue,
    ReceiptUser,
    VisibilityUser,
    VisibilityRepository,
    VisibilityParent,
    VisibilityIssue,
    FinalUser,
}
impl PostWriteCaptureAllowanceV2 {
    pub(in crate::providers::gh) fn members_metadata_detail_list_v62(
        &mut self,
        provider: &GithubProvider,
        actual: &Arc<OriginalMetadataMembersV48>,
        context: &MetadataMembersContextV48,
        list: crate::providers::gh::ConversationDetailListV19,
        heap: &mut MetadataMembersHeapV62,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<Value>> {
        use crate::providers::gh::ConversationDetailListV19 as List;
        let slot = match list {
            List::Comments => 11,
            List::Commits => 12,
            List::Files => 13,
            List::Reviews => 14,
            List::Timeline => 15,
        };
        self.check_issuer(provider, cancel)?;
        let pool = self.lineage.measured.get().ok_or_else(|| {
            error(
                GithubErrorKind::InvalidTarget,
                "Actual member cursor lineage unavailable",
            )
        })?;
        if !actual.metadata_tab_v62()
            || !context.matches_actual_v62(actual)
            || self.caps.len() != 22
            || self.next != slot
            || self.caps[slot] > 512 * 1024
            || self.lineage.state.load(Ordering::Acquire) != CLOSED
            || provider.scoped_read_budget.is_some()
            || heap.retained_bound() < context.retained_bound_v62()?
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Actual Metadata member detail role differs",
            ));
        }
        context.validate_metadata_pool_v62(provider, pool, cancel, self.deadline)?;
        let endpoint = list.endpoint_v19(provider, actual.parent())?;
        let bounded = self.spend_ordinary_slot_v2(provider, cancel)?;
        let mut spec = bounded.spec(vec![
            "api".into(),
            endpoint,
            "--hostname".into(),
            bounded.host.clone(),
            "--method".into(),
            "GET".into(),
            "--paginate".into(),
            "--slurp".into(),
        ]);
        spec.output_limit = spec.output_limit.min(512 * 1024);
        let output = bounded
            .runner
            .run(spec, &self.cancellation)
            .map_err(|failure| {
                error(
                    GithubError::from(failure).kind,
                    "Actual Metadata member detail unavailable",
                )
            })?;
        self.check_issuer(provider, cancel)?;
        heap.detail_pages(&output.stdout)
    }
    pub(in crate::providers::gh) fn members_metadata_closing_summary_v62(
        &mut self,
        provider: &GithubProvider,
        input: MembersMetadataClosingInputV62<'_>,
        cancel: &CancellationToken,
    ) -> GithubResult<ConversationRawSummaryV19> {
        let MembersMetadataClosingInputV62 {
            actual,
            current,
            context,
            debit,
        } = input;
        self.check_issuer(provider, cancel)?;
        let pool = self.lineage.measured.get().ok_or_else(|| {
            error(
                GithubErrorKind::InvalidTarget,
                "Actual member cursor lineage unavailable",
            )
        })?;
        if self.caps.len() != 22
            || self.next != 16
            || self.caps[16] > 128 * 1024
            || !context.matches_actual_v62(actual)
            || self.lineage.state.load(Ordering::Acquire) != CLOSED
            || provider.scoped_read_budget.is_some()
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Actual Metadata member closing-summary role differs",
            ));
        }
        context.validate_metadata_pool_v62(provider, pool, cancel, self.deadline)?;
        let mut document =
            MetadataSummaryDocumentV51::new(debit.with_members_context_v62(context)?)?;
        let endpoint = format!(
            "repos/{}/pulls/{}",
            actual.parent().repository(),
            actual.parent().number()
        );
        let bounded = self.spend_ordinary_slot_v2(provider, cancel)?;
        let mut spec = bounded.spec(vec![
            "api".into(),
            endpoint,
            "--hostname".into(),
            bounded.host.clone(),
            "--method".into(),
            "GET".into(),
        ]);
        spec.output_limit = spec.output_limit.min(128 * 1024);
        let output = bounded
            .runner
            .run(spec, &self.cancellation)
            .map_err(|failure| {
                error(
                    GithubError::from(failure).kind,
                    "Actual Metadata member closing observation unavailable",
                )
            })?;
        self.check_issuer(provider, cancel)?;
        document.accept_fixed_summary(&output.stdout)?;
        document.finish_members_closing_v62(provider, current, context)
    }
    pub(in crate::providers::gh) fn members_metadata_role_v62(
        &mut self,
        provider: &GithubProvider,
        actual: &OriginalMetadataMembersV48,
        role: MembersMetadataRoleV62,
        heap: &mut MetadataMembersHeapV62,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        self.check_issuer(provider, cancel)?;
        let parent = actual.parent();
        let (slot, endpoint) = match role {
            MembersMetadataRoleV62::ReceiptRepository => {
                (2, format!("repos/{}", parent.repository()))
            }
            MembersMetadataRoleV62::ReceiptParent => (
                3,
                format!("repos/{}/pulls/{}", parent.repository(), parent.number()),
            ),
            MembersMetadataRoleV62::ReceiptIssue => (
                4,
                format!("repos/{}/issues/{}", parent.repository(), parent.number()),
            ),
            MembersMetadataRoleV62::ReceiptUser => (5, "user".into()),
            MembersMetadataRoleV62::VisibilityUser => (6, "user".into()),
            MembersMetadataRoleV62::VisibilityRepository => {
                (7, format!("repos/{}", parent.repository()))
            }
            MembersMetadataRoleV62::VisibilityParent => (
                8,
                format!("repos/{}/pulls/{}", parent.repository(), parent.number()),
            ),
            MembersMetadataRoleV62::VisibilityIssue => (
                9,
                format!("repos/{}/issues/{}", parent.repository(), parent.number()),
            ),
            MembersMetadataRoleV62::FinalUser => (21, "user".into()),
        };
        if !actual.metadata_tab_v62()
            || self.caps.len() != 22
            || self.next != slot
            || self.lineage.state.load(Ordering::Acquire) != CLOSED
            || provider.scoped_read_budget.is_some()
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Actual Metadata member fixed role unavailable",
            ));
        }
        let bounded = self.spend_ordinary_slot_v2(provider, cancel)?;
        let mut spec = bounded.spec(vec![
            "api".into(),
            endpoint,
            "--hostname".into(),
            bounded.host.clone(),
            "--method".into(),
            "GET".into(),
        ]);
        spec.output_limit = spec.output_limit.min(128 * 1024);
        let output = bounded
            .runner
            .run(spec, &self.cancellation)
            .map_err(|failure| {
                error(
                    GithubError::from(failure).kind,
                    "Actual Metadata member paid observation unavailable",
                )
            })?;
        self.check_issuer(provider, cancel)?;
        heap.object(&output.stdout)
    }
    pub(in crate::providers::gh) fn members_metadata_current_v62(
        &mut self,
        provider: &GithubProvider,
        actual: &OriginalMetadataMembersV48,
        plan: &PrMetadataPlan,
        heap: &mut MetadataMembersHeapV62,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        if self.caps.len() != 22 || self.next != 10 || !actual.metadata_tab_v62() {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Actual Metadata member current-field role unavailable",
            ));
        }
        let request =
            crate::providers::gh::PreparedMembersRequestV48::prepare_metadata_current_v62(
                actual, plan, provider, cancel,
            )?;
        self.metadata_members_request_v48(provider, request, Some(heap), cancel)
    }
    pub(in crate::providers::gh) fn members_metadata_context_v62(
        &mut self,
        provider: &GithubProvider,
        input: MembersMetadataContextInputV62<'_>,
        cancel: &CancellationToken,
    ) -> GithubResult<ActualMetadataTabContextV51> {
        let MembersMetadataContextInputV62 {
            actual,
            plan,
            receipt,
            current,
            snapshot,
            retained,
        } = input;
        self.check_issuer(provider, cancel)?;
        let measured = self.lineage.measured.get().ok_or_else(|| {
            error(
                GithubErrorKind::InvalidTarget,
                "Actual member cursor lineage unavailable",
            )
        })?;
        if self.caps.len() != 22
            || self.next != 17
            || self.caps[17..21].iter().any(|cap| *cap > 128 * 1024)
            || self.lineage.state.load(Ordering::Acquire) != CLOSED
            || provider.scoped_read_budget.is_some()
            || !receipt.validates_metadata_context_v62(provider, actual, plan, cancel, measured)
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Actual Metadata member final context/receipt/role differs",
            ));
        }
        let parent = snapshot.parent();
        let endpoints = [
            format!("repos/{}/pulls/{}", parent.repository(), parent.number()),
            format!("repos/{}/issues/{}", parent.repository(), parent.number()),
            format!("repos/{}", parent.repository()),
            format!("repos/{}/pulls/{}", parent.repository(), parent.number()),
        ];
        // Retained SAME snapshot+actual producer/old structured context debit
        // is checked before the first child/parse, never a new empty model.
        let mut documents = MetadataContextDocumentsV51::new(retained)?;
        for endpoint in endpoints {
            let bounded = self.spend_ordinary_slot_v2(provider, cancel)?;
            let mut spec = bounded.spec(vec![
                "api".into(),
                endpoint,
                "--hostname".into(),
                bounded.host.clone(),
                "--method".into(),
                "GET".into(),
            ]);
            spec.output_limit = spec.output_limit.min(128 * 1024);
            let output = bounded
                .runner
                .run(spec, &self.cancellation)
                .map_err(|failure| {
                    error(
                        GithubError::from(failure).kind,
                        "Actual Metadata member final context unavailable",
                    )
                })?;
            self.check_issuer(provider, cancel)?;
            documents.accept_fixed_document(&output.stdout)?;
        }
        let context =
            documents.finish_members_paid_v62(provider, current, actual, plan, snapshot)?;
        self.check_issuer(provider, cancel)?;
        receipt.validate_postwrite_context_v78(provider, actual, plan, cancel)?;
        Ok(context)
    }
}
pub(in crate::providers::gh) struct MembersMetadataContextInputV62<'a> {
    pub(in crate::providers::gh) actual: &'a Arc<OriginalMetadataMembersV48>,
    pub(in crate::providers::gh) plan: &'a PrMetadataPlan,
    pub(in crate::providers::gh) receipt: &'a PrMetadataMembersReceiptV48,
    pub(in crate::providers::gh) current: &'a CurrentCatalogueRepository,
    pub(in crate::providers::gh) snapshot: &'a ConversationDetailSnapshotV19,
    pub(in crate::providers::gh) retained: usize,
}
pub(in crate::providers::gh) struct MembersMetadataClosingInputV62<'a> {
    pub(in crate::providers::gh) actual: &'a Arc<OriginalMetadataMembersV48>,
    pub(in crate::providers::gh) current: &'a CurrentCatalogueRepository,
    pub(in crate::providers::gh) context: &'a MetadataMembersContextV48,
    pub(in crate::providers::gh) debit: MetadataRetainedDebitV51,
}
