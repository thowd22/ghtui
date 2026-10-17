//! Closed roles11..14 only, on the original prepaid16 cursor. No endpoint,
//! raw output, generic API, pool/provider getter or independent read is exposed.
use super::*;
use crate::providers::gh::pr_resources_v1::metadata_context_v51::{
    MetadataContextDocumentsV51, MetadataRetainedDebitV51, MetadataSummaryDocumentV51,
    PaidBaseMetadataInputV64,
};
use crate::providers::gh::{
    ActualMetadataTabContextV51, BaseSelectionV32, OriginalMetadataBaseV32,
    OriginalMetadataTextV25, PrMetadataBaseReceiptV32, PrMetadataTextReceiptV25,
};
enum PaidMetadataTargetV51<'a> {
    Text(&'a OriginalMetadataTextV25, &'a PrMetadataTextReceiptV25),
    Base(
        &'a OriginalMetadataBaseV32,
        &'a BaseSelectionV32,
        &'a PrMetadataBaseReceiptV32,
    ),
}

impl PostWriteCaptureAllowanceV2 {
    pub(in crate::providers::gh) fn metadata_text_closing_summary_v51(
        &mut self,
        provider: &GithubProvider,
        actual: &OriginalMetadataTextV25,
        receipt: &PrMetadataTextReceiptV25,
        current: &CurrentCatalogueRepository,
        debit: MetadataRetainedDebitV51,
        cancel: &CancellationToken,
    ) -> GithubResult<crate::providers::gh::ConversationRawSummaryV19> {
        self.capture_metadata_summary_v51(
            provider,
            PaidMetadataTargetV51::Text(actual, receipt),
            current,
            debit,
            cancel,
        )
    }
    pub(in crate::providers::gh) fn base_metadata_closing_summary_v51(
        &mut self,
        provider: &GithubProvider,
        input: PaidBaseMetadataInputV64<'_>,
        current: &CurrentCatalogueRepository,
        debit: MetadataRetainedDebitV51,
        cancel: &CancellationToken,
    ) -> GithubResult<crate::providers::gh::ConversationRawSummaryV19> {
        let PaidBaseMetadataInputV64 {
            actual,
            selection,
            receipt,
        } = input;
        self.capture_metadata_summary_v51(
            provider,
            PaidMetadataTargetV51::Base(actual, selection, receipt),
            current,
            debit,
            cancel,
        )
    }
    fn capture_metadata_summary_v51(
        &mut self,
        provider: &GithubProvider,
        target: PaidMetadataTargetV51<'_>,
        current: &CurrentCatalogueRepository,
        debit: MetadataRetainedDebitV51,
        cancel: &CancellationToken,
    ) -> GithubResult<crate::providers::gh::ConversationRawSummaryV19> {
        self.check_issuer(provider, cancel)?;
        let measured = self.lineage.measured.get().ok_or_else(|| {
            error(
                GithubErrorKind::InvalidTarget,
                "Actual Metadata cursor lineage unavailable",
            )
        })?;
        let valid = match &target {
            PaidMetadataTargetV51::Text(actual, receipt) => {
                receipt.validates_paid_context_v51(actual, cancel, measured)
            }
            PaidMetadataTargetV51::Base(actual, selection, receipt) => {
                receipt.validates_paid_context_v51(actual, selection, cancel, measured)
            }
        };
        if !valid
            || self.caps.len() != 16
            || self.next != 10
            || self.caps[10] > 128 * 1024
            || self.lineage.state.load(Ordering::Acquire) != CLOSED
            || provider.scoped_read_budget.is_some()
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Exact original paid Metadata closing-summary role unavailable",
            ));
        }
        // Construct the complete actual retained-list debit BEFORE spend,
        // child and parse. No raw Value is decoded outside the bounded visitor.
        let mut document = MetadataSummaryDocumentV51::new(debit)?;
        let parent = match &target {
            PaidMetadataTargetV51::Text(actual, _) => actual.observed().parent(),
            PaidMetadataTargetV51::Base(_, _, receipt) => receipt.parent(),
        };
        let endpoint = format!("repos/{}/pulls/{}", parent.repository(), parent.number());
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
        let bytes = bounded
            .runner
            .run(spec, &self.cancellation)
            .map_err(|e| {
                error(
                    GithubError::from(e).kind,
                    "Original paid Metadata summary unavailable; private response omitted",
                )
            })?
            .stdout;
        self.check_issuer(provider, cancel)?;
        document.accept_fixed_summary(&bytes)?;
        let summary = match target {
            PaidMetadataTargetV51::Text(actual, _) => {
                document.finish_text_paid(provider, current, actual)?
            }
            PaidMetadataTargetV51::Base(_, _, receipt) => {
                document.finish_base_paid(provider, current, receipt)?
            }
        };
        self.check_issuer(provider, cancel)?;
        Ok(summary)
    }
    pub(in crate::providers::gh) fn metadata_tab_context_v51(
        &mut self,
        provider: &GithubProvider,
        actual: &OriginalMetadataTextV25,
        receipt: &PrMetadataTextReceiptV25,
        current: &CurrentCatalogueRepository,
        retained_snapshot_bytes: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<ActualMetadataTabContextV51> {
        self.capture_metadata_context_v51(
            provider,
            PaidMetadataTargetV51::Text(actual, receipt),
            current,
            retained_snapshot_bytes,
            cancel,
        )
    }
    pub(in crate::providers::gh) fn base_metadata_context_v51(
        &mut self,
        provider: &GithubProvider,
        input: PaidBaseMetadataInputV64<'_>,
        current: &CurrentCatalogueRepository,
        retained_snapshot_bytes: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<ActualMetadataTabContextV51> {
        let PaidBaseMetadataInputV64 {
            actual,
            selection,
            receipt,
        } = input;
        self.capture_metadata_context_v51(
            provider,
            PaidMetadataTargetV51::Base(actual, selection, receipt),
            current,
            retained_snapshot_bytes,
            cancel,
        )
    }
    fn capture_metadata_context_v51(
        &mut self,
        provider: &GithubProvider,
        target: PaidMetadataTargetV51<'_>,
        current: &CurrentCatalogueRepository,
        retained_snapshot_bytes: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<ActualMetadataTabContextV51> {
        self.check_issuer(provider, cancel)?;
        let measured = self.lineage.measured.get().ok_or_else(|| {
            error(
                GithubErrorKind::InvalidTarget,
                "Actual Metadata cursor lineage unavailable",
            )
        })?;
        if self.caps.len() != 16
            || self.next != 11
            || self.lineage.state.load(Ordering::Acquire) != CLOSED
            || provider.scoped_read_budget.is_some()
            || !match &target {
                PaidMetadataTargetV51::Text(actual, receipt) => {
                    receipt.validates_paid_context_v51(actual, cancel, measured)
                }
                PaidMetadataTargetV51::Base(actual, selection, receipt) => {
                    receipt.validates_paid_context_v51(actual, selection, cancel, measured)
                }
            }
            || self.caps[11..15].iter().any(|cap| *cap > 128 * 1024)
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Exact original paid Metadata context role/receipt/lineage unavailable",
            ));
        }
        let parent = match &target {
            PaidMetadataTargetV51::Text(actual, _) => actual.observed().parent(),
            PaidMetadataTargetV51::Base(_, _, receipt) => receipt.parent(),
        };
        let name = parent.repository();
        let number = parent.number();
        let endpoints = [
            format!("repos/{name}/pulls/{number}"),
            format!("repos/{name}/issues/{number}"),
            format!("repos/{name}"),
            format!("repos/{name}/pulls/{number}"),
        ];
        let mut documents = MetadataContextDocumentsV51::new(retained_snapshot_bytes)?;
        for endpoint in endpoints {
            // Spend BEFORE the sole child. The exact original issuer/token/
            // inherited deadline/cap are enforced by the unchanged kernel.
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
            let bytes = bounded
                .runner
                .run(spec, &self.cancellation)
                .map_err(|failure| {
                    error(
                        GithubError::from(failure).kind,
                        "Original paid Metadata context unavailable; private response omitted",
                    )
                })?
                .stdout;
            self.check_issuer(provider, cancel)?;
            documents.accept_fixed_document(&bytes)?;
        }
        let context = match target {
            PaidMetadataTargetV51::Text(actual, _) => {
                documents.finish(provider, current, actual)?
            }
            PaidMetadataTargetV51::Base(_, _, receipt) => {
                documents.finish_base(provider, current, receipt)?
            }
        };
        self.check_issuer(provider, cancel)?;
        Ok(context)
    }
}
