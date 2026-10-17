//! The fixed SAME prepaid cursor: mutation role0 only, never arbitrary input.
use super::*;
use crate::providers::gh::pr_issue_links_v87::IssueLinkPublicationFailureV87;
use crate::providers::gh::pr_issue_links_v87::PreparedIssueLinkRequestV87;
use crate::providers::gh::pr_issue_links_v87::{
    refresh::IssueLinkPaidRoleV87, IssueLinkReceiptV87, OriginalIssueLinksV87,
};
use crate::providers::runner::IssueLinkMutationRunV87;
use std::sync::Arc;

impl PostWriteCaptureAllowanceV2 {
    pub(in crate::providers::gh) fn issue_link_metadata_context_v87(
        &mut self,
        provider: &GithubProvider,
        actual: &Arc<OriginalIssueLinksV87>,
        receipt: &IssueLinkReceiptV87,
        current: &CurrentCatalogueRepository,
        retained: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<crate::providers::gh::ActualMetadataTabContextV51> {
        use crate::providers::gh::pr_resources_v1::metadata_context_v51::MetadataContextDocumentsV51;
        self.check_issuer(provider, cancel)?;
        let pool = self
            .lineage
            .measured
            .get()
            .ok_or_else(|| {
                error(
                    GithubErrorKind::InvalidTarget,
                    "Actual Issue context pool missing",
                )
            })?
            .clone();
        receipt.validates_paid_pool_v87(provider, actual, &pool, cancel)?;
        if !actual.metadata_tab_v87()
            || self.caps.len() != 19
            || self.next != 12
            || self.caps[12..16].iter().any(|cap| *cap > 128 * 1024)
            || self.lineage.state.load(Ordering::Acquire) != CLOSED
            || provider.scoped_read_budget.is_some()
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Actual Issue Metadata four-context role differs",
            ));
        }
        let parent = actual.parent();
        let endpoints = [
            format!("repos/{}/pulls/{}", parent.repository(), parent.number()),
            format!("repos/{}/issues/{}", parent.repository(), parent.number()),
            format!("repos/{}", parent.repository()),
            format!("repos/{}/pulls/{}", parent.repository(), parent.number()),
        ];
        let mut documents = MetadataContextDocumentsV51::new(retained)?;
        for endpoint in endpoints {
            receipt.validates_paid_pool_v87(provider, actual, &pool, cancel)?;
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
                .map_err(GithubError::from)?;
            self.check_issuer(provider, cancel)?;
            receipt.validates_paid_pool_v87(provider, actual, &pool, cancel)?;
            documents.accept_fixed_document(&output.stdout)?;
        }
        let context =
            documents.finish_issue_link_v87(provider, current, actual, receipt, cancel)?;
        self.check_issuer(provider, cancel)?;
        Ok(context)
    }
    pub(in crate::providers::gh) fn issue_link_fixed_role_v87(
        &mut self,
        provider: &GithubProvider,
        actual: &Arc<OriginalIssueLinksV87>,
        receipt: &IssueLinkReceiptV87,
        role: IssueLinkPaidRoleV87,
        cancel: &CancellationToken,
    ) -> GithubResult<crate::providers::runner::CommandOutput> {
        self.check_issuer(provider, cancel)?;
        let pool = self
            .lineage
            .measured
            .get()
            .ok_or_else(|| {
                error(
                    GithubErrorKind::InvalidTarget,
                    "Actual Issue paid pool missing",
                )
            })?
            .clone();
        receipt.validates_paid_pool_v87(provider, actual, &pool, cancel)?;
        let metadata = actual.metadata_tab_v87();
        let (slot, cap) = role.slot(metadata);
        if self.caps.len() != (if metadata { 19 } else { 15 })
            || self.next != slot
            || self.caps[slot] > cap
            || self.lineage.state.load(Ordering::Acquire) != CLOSED
            || provider.scoped_read_budget.is_some()
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Actual fixed Issue paid role/cursor differs",
            ));
        }
        let endpoint = role.endpoint(provider, actual)?;
        let input = role.input(actual)?;
        let bounded = self.spend_ordinary_slot_v2(provider, cancel)?;
        let mut args = vec![
            "api".into(),
            endpoint,
            "--hostname".into(),
            bounded.host.clone(),
            "--method".into(),
            if input.is_some() {
                "POST".into()
            } else {
                "GET".into()
            },
        ];
        if input.is_some() {
            args.extend(["--input".into(), "-".into()]);
        }
        if role.is_list() {
            args.extend(["--paginate".into(), "--slurp".into()]);
        }
        let mut spec = bounded.spec(args);
        spec.output_limit = spec.output_limit.min(cap);
        if let Some(input) = input {
            spec.stdin = input;
        }
        let output = bounded
            .runner
            .run(spec, &self.cancellation)
            .map_err(GithubError::from)?;
        self.check_issuer(provider, cancel)?;
        receipt.validates_paid_pool_v87(provider, actual, &pool, cancel)?;
        Ok(output)
    }
    pub(in crate::providers::gh) fn issue_link_mutation_v87(
        &mut self,
        provider: &GithubProvider,
        request: &mut PreparedIssueLinkRequestV87,
        cancel: &CancellationToken,
    ) -> Result<IssueLinkMutationRunV87, IssueLinkPublicationFailureV87> {
        let bounded = (|| {
            self.check_issuer(provider, cancel)?;
            let metadata = request.metadata_profile_v87()?;
            let expected = if metadata { 19 } else { 15 };
            if self.caps.len() != expected || self.next != 0 || self.caps[0] > 128 * 1024 {
                return Err(error(
                    GithubErrorKind::InvalidTarget,
                    "Actual Issue-link cursor/profile differs",
                ));
            }
            let pool = self.lineage.measured.get().ok_or_else(|| {
                error(
                    GithubErrorKind::InvalidTarget,
                    "Actual Issue-link prewrite pool missing",
                )
            })?;
            request.matches_paid_pool_v87(provider, pool, cancel)?;
            let bounded = self.spend_ordinary_slot_v2(provider, cancel)?;
            request.bind_paid_v87(&bounded, cancel)?;
            Ok(bounded)
        })()
        .map_err(IssueLinkPublicationFailureV87::NotStarted)?;
        let result = bounded
            .runner
            .run_issue_link_pair_v87(request, &self.cancellation)
            .map_err(|failure| {
                let unknown = failure.outcome_unknown;
                let error = GithubError::from(failure);
                if unknown {
                    IssueLinkPublicationFailureV87::WriteMayHaveStarted(error)
                } else {
                    IssueLinkPublicationFailureV87::NotStarted(error)
                }
            })?;
        self.check_issuer(provider, cancel)
            .map_err(IssueLinkPublicationFailureV87::WriteMayHaveStarted)?;
        request
            .bind_run_v87(&result)
            .map_err(IssueLinkPublicationFailureV87::WriteMayHaveStarted)?;
        Ok(result)
    }
    pub(in crate::providers::gh) fn issue_link_readback_v87(
        &mut self,
        provider: &GithubProvider,
        request: &PreparedIssueLinkRequestV87,
        role: crate::providers::gh::pr_issue_links_v87::IssueLinkReadbackRoleV87,
        heap: &mut crate::providers::gh::pr_issue_links_v87::bounded::IssueLinkHeapV87,
        cancel: &CancellationToken,
    ) -> GithubResult<crate::providers::gh::pr_issue_links_v87::bounded::IssueDocumentV87> {
        self.check_issuer(provider, cancel)?;
        let slot = match role {
            crate::providers::gh::pr_issue_links_v87::IssueLinkReadbackRoleV87::Pr => 1,
            crate::providers::gh::pr_issue_links_v87::IssueLinkReadbackRoleV87::Issue => 2,
        };
        let metadata = request.metadata_profile_v87()?;
        if self.caps.len() != (if metadata { 19 } else { 15 })
            || self.next != slot
            || self.caps[slot] > 512 * 1024
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Actual Issue-link readback role differs",
            ));
        }
        let bounded = self.spend_ordinary_slot_v2(provider, cancel)?;
        let result = request.paid_readback_v87(&bounded, role, heap, cancel)?;
        self.check_issuer(provider, cancel)?;
        Ok(result)
    }
}
