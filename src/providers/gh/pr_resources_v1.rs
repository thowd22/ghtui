//! SOURCE-ONLY exact selected-PR resources. Reuses existing gh runner and the
//! owning original catalogue measured window; no auth/client/write stack.
use super::*;
use crate::providers::gh::search::canonical_source_url_matches;
#[path = "pr_file_comment_v75.rs"]
mod file_comment_v75;
pub(crate) use file_comment_v75::*;
mod metadata_base_v32;
pub(in crate::providers::gh) mod metadata_context_v51;
pub(crate) use metadata_context_v51::ActualMetadataTabContextV51;
pub use metadata_context_v51::MetadataDisplayFactsV51;
mod metadata_members_parent_v48;
pub(in crate::providers::gh) use metadata_members_parent_v48::MetadataMembersParentV48;
mod issue_links_parent_v87;
mod metadata_text_v25;
mod state_v68;
pub(in crate::providers::gh) use issue_links_parent_v87::IssueLinkParentV87;
use metadata_base_v32::BaseReceiptExpectationV32;
pub(crate) use metadata_base_v32::{
    BaseCatalogueLimitsV32, BaseCatalogueV32, BaseCoverageStatusV32, BaseRecoveryCurrentV32,
    BaseSelectionV32, BaseTransitionV32, OriginalMetadataBaseV32, PrMetadataBaseFailureV32,
    PrMetadataBaseReadV32, PrMetadataBaseReceiptV32, METADATA_BASE_POST_CAPS_V32,
};
pub(in crate::providers::gh) use state_v68::PreparedStateRequestV68;
pub(crate) use state_v68::StatePublishInputV68;
use state_v68::StateReceiptExpectationV68;
pub(crate) use state_v68::{
    OriginalPrStateV68, PrStateReadV68, StateCapabilityV68, StatePublicationFailureV68,
    StateReceiptV68, StateRecoveryCurrentV68, StateTransitionV68, STATE_POST_CAPS_V68,
};
mod model;
pub use metadata_text_v25::PrMetadataTextFieldV25;
#[cfg(test)]
pub(crate) use metadata_text_v25::METADATA_TEXT_CAPABILITY_QUERY_V25;
pub(crate) use metadata_text_v25::{
    MetadataRecoveryCurrentV25, OriginalMetadataTextV25, PrMetadataTextFailureV25,
    PrMetadataTextReadV25, PrMetadataTextReceiptV25, METADATA_TAB_TEXT_POST_CAPS_V51,
    METADATA_TEXT_POST_CAPS_V25,
};
mod notification_parent_v63;
mod paging;
mod parent_intent_v13;
mod parse;
use parent_intent_v13::ExpectedPrParentV13;
pub use parent_intent_v13::PrParentIntentV13;
#[path = "pr_resources_v1/pr_checks_binding_v1.rs"]
mod pr_checks_binding_v1;
pub use model::*;
#[path = "pr_immutable_images_v1.rs"]
mod immutable_images;
#[path = "pr_thread_history_parent_v1.rs"]
mod thread_history_parent_v1;
#[path = "pr_thread_postwrite_parent_v2.rs"]
mod thread_postwrite_parent_v2;
pub use immutable_images::*;
#[path = "pr_review_postwrite_parent_v2.rs"]
mod review_postwrite_parent_v2;
use paging::*;
use parse::*;
pub(in crate::providers::gh) use review_postwrite_parent_v2::MetadataMembersPostDataV48;
pub use thread_history_parent_v1::ThreadHistoryParentObservationV1;
#[path = "pr_thread_detail_parent_v4.rs"]
mod thread_detail_parent_v4;
pub(crate) use thread_detail_parent_v4::ThreadDetailParentObservationV4;

fn safe(kind: GithubErrorKind) -> GithubError {
    error(
        kind,
        "Selected PR resource source unavailable; private data not substituted",
    )
}
fn full_sha(sha: &str) -> bool {
    matches!(sha.len(), 40 | 64) && sha.bytes().all(|byte| byte.is_ascii_hexdigit())
}
fn opaque(node: &str) -> bool {
    !node.is_empty() && node.len() <= 1024 && !node.chars().any(char::is_control)
}
fn checked(
    provider: &GithubProvider,
    cancel: &CancellationToken,
    cap: usize,
    hold: usize,
) -> GithubResult<()> {
    let budget = provider
        .scoped_read_budget
        .as_ref()
        .ok_or_else(|| safe(GithubErrorKind::InvalidTarget))?;
    budget.check_clock(cancel).map_err(GithubError::from)?;
    let state = budget.accounting().map_err(GithubError::from)?;
    let required = cap
        .checked_add(hold)
        .ok_or_else(|| safe(GithubErrorKind::InvalidTarget))?;
    if state.sealed()
        || state.in_flight_requests() != 0
        || state
            .charged_bytes()
            .checked_add(required)
            .is_none_or(|needed| needed > state.capacity_bytes())
        || state
            .requests_claimed()
            .checked_add(if hold == 0 { 1 } else { 2 })
            .is_none_or(|needed| needed > state.max_requests())
    {
        return Err(safe(GithubErrorKind::Transport));
    }
    Ok(())
}
fn document(
    provider: &GithubProvider,
    endpoint: &str,
    cap: usize,
    cancel: &CancellationToken,
    closing: bool,
) -> GithubResult<Value> {
    checked(provider, cancel, cap, if closing { 0 } else { cap })?;
    // Same ordinary API GET argv as existing api(), but preflight raw UTF8 bytes
    // BEFORE JSON parse. Captures/failures are charged by the attached runner.
    let mut spec = provider.spec(vec![
        "api".into(),
        endpoint.into(),
        "--hostname".into(),
        provider.host.clone(),
        "--method".into(),
        "GET".into(),
    ]);
    spec.output_limit = spec.output_limit.min(cap);
    let bytes = provider
        .runner
        .run(spec, cancel)
        .map_err(|failure| safe(GithubError::from(failure).kind))?
        .stdout;
    if bytes.len() > cap {
        return Err(safe(GithubErrorKind::Json));
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| safe(GithubErrorKind::Json))?;
    if !value.is_object() && !value.is_array() {
        return Err(safe(GithubErrorKind::Json));
    }
    Ok(value)
}
fn raw_patch(
    provider: &GithubProvider,
    original: &PrSummary,
    cap: usize,
    closing_cap: usize,
    cancel: &CancellationToken,
) -> GithubResult<Vec<u8>> {
    checked(provider, cancel, cap, closing_cap)?;
    // Exactly existing pr_diff argv, with no lossy String conversion, escape
    // flag retry, API fallback, hidden mutable ref or local checkout.
    let mut spec = provider.spec(vec![
        "pr".into(),
        "diff".into(),
        original.pr.number.to_string(),
        "--repo".into(),
        format!("{}/{}", provider.host, original.pr.repository),
        "--color".into(),
        "never".into(),
    ]);
    spec.output_limit = spec.output_limit.min(cap);
    provider
        .runner
        .run(spec, cancel)
        .map(|output| output.stdout)
        .map_err(|failure| safe(GithubError::from(failure).kind))
}
impl GithubProvider {
    /// Read-only Personal SUMMARY/HISTORY, on the caller's SAME original
    /// session/measured pool. No Files/source/DTO or write permission minted.
    pub(crate) fn pr_personal_history_parent_v75(
        &self,
        current: &CurrentCatalogueRepository,
        intent: &PrParentIntentV13,
        cancel: &CancellationToken,
    ) -> GithubResult<PrResourceParentV1> {
        self.pr_original_detail_parent_v17(current, intent, cancel)
    }
    /// SAME actual raw-parent parser and measured GET. The bounded expectation
    /// is navigation only; original Core/current repository supply read proof.
    /// No source, transport getter or publication constructor is introduced.
    pub(crate) fn pr_original_detail_parent_v17(
        &self,
        current: &CurrentCatalogueRepository,
        intent: &PrParentIntentV13,
        cancel: &CancellationToken,
    ) -> GithubResult<PrResourceParentV1> {
        self.original_detail_parent_raw_v19(current, intent, cancel)
            .map(|(parent, _)| parent)
    }
    // SAME existing child/parser. The raw Value stays inside this provider
    // family; only the purpose-closed data wrapper below may retain its bounded
    // emitted summary. No general raw-parent or measured provider getter.
    fn original_detail_parent_raw_v19(
        &self,
        current: &CurrentCatalogueRepository,
        intent: &PrParentIntentV13,
        cancel: &CancellationToken,
    ) -> GithubResult<(PrResourceParentV1, Value)> {
        if current.repository().permissions.pull != Some(true)
            || intent.identity().host != self.host
        {
            return Err(safe(GithubErrorKind::IdentityChanged));
        }
        let raw = document(
            self,
            &format!(
                "repos/{}/pulls/{}",
                current.repository().full_name,
                intent.number()
            ),
            128 * 1024,
            cancel,
            true,
        )?;
        let parent = parent_expected_v13(
            self,
            current,
            ExpectedPrParentV13::PersonalIntent(intent),
            &raw,
        )?;
        Ok((parent, raw))
    }
    pub(crate) fn pr_conversation_refresh_summary_v19(
        &self,
        current: &CurrentCatalogueRepository,
        intent: &PrParentIntentV13,
        cancel: &CancellationToken,
    ) -> GithubResult<ConversationRawSummaryV19> {
        let (parent, raw) = self.original_detail_parent_raw_v19(current, intent, cancel)?;
        ConversationRawSummaryV19::from_actual_raw_v19(self, parent, &raw)
    }
    /// Fresh raw parent in the caller's SAME measured original-source READ.
    /// This intermediate receipt never grants permission to publish a comment.
    pub(crate) fn pr_inline_original_parent_v1(
        &self,
        current: &CurrentCatalogueRepository,
        original: &PrResourceReadV1,
        limits: &PrResourceReadLimitsV1,
        cancel: &CancellationToken,
    ) -> GithubResult<PrResourceParentV1> {
        limits.validate()?;
        if original.kind != PrResourceKindV1::Files {
            return Err(safe(GithubErrorKind::InvalidTarget));
        }
        let endpoint = format!(
            "repos/{}/pulls/{}",
            current.repository().full_name,
            original.parent.number()
        );
        let value = document(self, &endpoint, limits.max_document_bytes, cancel, true)?;
        let fresh = parent(self, current, &original.parent.summary, &value)?;
        if !immutable_images::original_parent(&original.parent, &fresh) {
            return Err(safe(GithubErrorKind::StaleHead));
        }
        Ok(fresh)
    }
    /// Actual raw parent before/after, explicitly bounded resource pages. Fresh
    /// principal/repo admission is owned by the SAME original session, not this
    /// intermediate value; caller MUST close it with original local guards.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn pr_resource_read_v1(
        &self,
        current: &CurrentCatalogueRepository,
        identity: &HostIdentity,
        original: &PrSummary,
        kind: PrResourceKindV1,
        limits: &PrResourceReadLimitsV1,
        cancel: &CancellationToken,
        after_opening: impl FnOnce(&PrResourceParentV1) -> GithubResult<()>,
    ) -> GithubResult<PrResourceReadV1> {
        self.pr_resource_read_expected_v13(
            current,
            identity,
            ExpectedPrParentV13::ExistingSummary(original),
            kind,
            limits,
            cancel,
            after_opening,
        )
    }
    /// Personal navigation is an expectation only, never a synthetic Summary
    /// or CatalogueRead. All actual parent fields still come from one parser.
    pub(crate) fn pr_resource_read_personal_v13(
        &self,
        current: &CurrentCatalogueRepository,
        intent: &PrParentIntentV13,
        kind: PrResourceKindV1,
        limits: &PrResourceReadLimitsV1,
        cancel: &CancellationToken,
        after_opening: impl FnOnce(&PrResourceParentV1) -> GithubResult<()>,
    ) -> GithubResult<PrResourceReadV1> {
        self.pr_resource_read_expected_v13(
            current,
            intent.identity(),
            ExpectedPrParentV13::PersonalIntent(intent),
            kind,
            limits,
            cancel,
            after_opening,
        )
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "One original resource phase keeps actual repository, numeric identity, closed expectation, bounded source and opening guard distinct"
    )]
    fn pr_resource_read_expected_v13(
        &self,
        current: &CurrentCatalogueRepository,
        identity: &HostIdentity,
        original: ExpectedPrParentV13<'_>,
        kind: PrResourceKindV1,
        limits: &PrResourceReadLimitsV1,
        cancel: &CancellationToken,
        after_opening: impl FnOnce(&PrResourceParentV1) -> GithubResult<()>,
    ) -> GithubResult<PrResourceReadV1> {
        limits.validate()?;
        if identity.host != self.host
            || identity
                .account_id
                .is_none_or(|id| id == 0 || id > i64::MAX as u64)
            || original.host() != self.host
            || original.number() == 0
            || !full_sha(original.head())
            || !full_sha(original.base())
            || !opaque(original.node())
            || current.repository().permissions.pull != Some(true)
            || current.repository().id == 0
            || current
                .repository()
                .node_id
                .as_deref()
                .is_none_or(|node| !opaque(node))
        {
            return Err(safe(GithubErrorKind::InvalidTarget));
        }
        let endpoint = format!(
            "repos/{}/pulls/{}",
            current.repository().full_name,
            original.number()
        );
        let opening = parent_expected_v13(
            self,
            current,
            original,
            &document(self, &endpoint, limits.max_document_bytes, cancel, false)?,
        )?;
        checked(
            self,
            cancel,
            limits.max_document_bytes,
            limits.max_document_bytes,
        )?;
        after_opening(&opening).map_err(|failure| safe(failure.kind))?;
        checked(
            self,
            cancel,
            limits.max_document_bytes,
            limits.max_document_bytes,
        )?;
        let mut observed = PrResourceReadV1 {
            kind,
            parent: opening,
            files: vec![],
            checks: vec![],
            statuses: vec![],
            file_coverage: Default::default(),
            check_coverage: Default::default(),
            status_coverage: Default::default(),
            patch: None,
            patch_failure: None,
        };
        match kind {
            PrResourceKindV1::Files => {
                let (files, coverage) = collect_files(self, &observed.parent, limits, cancel)?;
                observed.files = files;
                observed.file_coverage = coverage;
                match raw_patch(
                    self,
                    observed.parent.summary(),
                    limits.max_patch_bytes,
                    limits.max_document_bytes,
                    cancel,
                ) {
                    Ok(bytes) => observed.patch = Some(bytes),
                    Err(failure) => observed.patch_failure = Some(failure.kind),
                }
            }
            PrResourceKindV1::Checks => {
                let (checks, coverage) =
                    collect_checks(self, &observed.parent, false, limits, cancel)?;
                observed.checks = checks;
                observed.check_coverage = coverage;
                let (statuses, coverage) =
                    collect_checks(self, &observed.parent, true, limits, cancel)?;
                observed.statuses = statuses;
                observed.status_coverage = coverage;
            }
        }
        let closing = match kind {
            PrResourceKindV1::Files => {
                self.pr_inline_original_parent_v1(current, &observed, limits, cancel)?
            }
            PrResourceKindV1::Checks => parent_expected_v13(
                self,
                current,
                original,
                &document(self, &endpoint, limits.max_document_bytes, cancel, true)?,
            )?,
        };
        if !same_parent(&observed.parent, &closing) {
            return Err(safe(GithubErrorKind::StaleHead));
        }
        observed.parent = closing;
        checked_finished(self, cancel)?;
        Ok(observed)
    }
}
fn checked_finished(provider: &GithubProvider, cancel: &CancellationToken) -> GithubResult<()> {
    provider
        .scoped_read_budget
        .as_ref()
        .ok_or_else(|| safe(GithubErrorKind::InvalidTarget))?
        .check_clock(cancel)
        .map_err(Into::into)
}
