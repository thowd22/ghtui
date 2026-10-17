//! ONE fixed original PR GET, same strict parent parser, with raw body-null
//! identity retained as a digest. Never a DTO/raw-Value-to-Source constructor.
use super::*;
use crate::providers::gh::pr_issue_links_v87::bounded::IssueLinkHeapV87;
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub(in crate::providers::gh) struct IssueLinkParentV87 {
    parent: Arc<PrResourceParentV1>,
    body_null: bool,
    body_digest: String,
}
impl IssueLinkParentV87 {
    pub(in crate::providers::gh) fn parent(&self) -> &Arc<PrResourceParentV1> {
        &self.parent
    }
    pub(in crate::providers::gh) fn matches_body(&self, raw: &Value) -> bool {
        match raw.get("body") {
            Some(Value::Null) => self.body_null,
            Some(Value::String(s)) if s.len() <= 65_536 => {
                !self.body_null && format!("{:x}", Sha256::digest(s.as_bytes())) == self.body_digest
            }
            _ => false,
        }
    }
    pub(in crate::providers::gh) fn body_digest(&self) -> &str {
        &self.body_digest
    }
    pub(in crate::providers::gh) fn body_null(&self) -> bool {
        self.body_null
    }
    pub(in crate::providers::gh) fn same_observation(&self, other: &Self) -> bool {
        self.body_null == other.body_null
            && self.body_digest == other.body_digest
            && same_parent(&self.parent, &other.parent)
            && self.parent.summary().pr.title == other.parent.summary().pr.title
    }
}
impl GithubProvider {
    pub(in crate::providers::gh) fn parse_issue_manual_parent_v94(
        &self,
        current: &CurrentCatalogueRepository,
        actual: &crate::providers::gh::IssueLinkManualReadV94,
        raw: &Value,
        cancel: &CancellationToken,
    ) -> GithubResult<PrResourceParentV1> {
        actual.validate_live_v94(self, cancel)?;
        let parent = parent_expected_v13(
            self,
            current,
            ExpectedPrParentV13::PersonalIntent(actual.original_intent_v94()),
            raw,
        )?;
        if !actual.matches_parent_v94(&parent, raw) {
            return Err(safe(GithubErrorKind::StaleHead));
        }
        actual.validate_live_v94(self, cancel)?;
        Ok(parent)
    }
    pub(in crate::providers::gh) fn parse_paid_issue_parent_v87(
        &self,
        current: &CurrentCatalogueRepository,
        actual: &Arc<crate::providers::gh::OriginalIssueLinksV87>,
        receipt: &crate::providers::gh::IssueLinkReceiptV87,
        raw: &Value,
        cancel: &CancellationToken,
    ) -> GithubResult<PrResourceParentV1> {
        receipt.validate_epoch_v87(self, actual, cancel)?;
        let parent = parse::parent(self, current, actual.parent().summary(), raw)?;
        if !same_parent(actual.parent(), &parent) || !actual.matches_raw_body_v87(raw) {
            return Err(safe(GithubErrorKind::StaleHead));
        }
        Ok(parent)
    }
    pub(in crate::providers::gh) fn original_issue_link_parent_v87(
        &self,
        current: &CurrentCatalogueRepository,
        intent: &PrParentIntentV13,
        heap: &mut IssueLinkHeapV87,
        cancel: &CancellationToken,
    ) -> GithubResult<IssueLinkParentV87> {
        self.issue_link_parent_read_v94(current, intent, heap, cancel, false)
            .map(|(parent, _)| parent)
    }
    /// SAME fixed actual PR response, additionally converted to its bounded
    /// summary for the closed explicit Issue read. Not a DTO/raw constructor.
    pub(in crate::providers::gh) fn fresh_issue_link_parent_v94(
        &self,
        current: &CurrentCatalogueRepository,
        intent: &PrParentIntentV13,
        heap: &mut IssueLinkHeapV87,
        cancel: &CancellationToken,
    ) -> GithubResult<(IssueLinkParentV87, ConversationRawSummaryV19)> {
        let (parent, summary) =
            self.issue_link_parent_read_v94(current, intent, heap, cancel, true)?;
        Ok((
            parent,
            summary.ok_or_else(|| safe(GithubErrorKind::InvalidTarget))?,
        ))
    }
    fn issue_link_parent_read_v94(
        &self,
        current: &CurrentCatalogueRepository,
        intent: &PrParentIntentV13,
        heap: &mut IssueLinkHeapV87,
        cancel: &CancellationToken,
        with_summary: bool,
    ) -> GithubResult<(IssueLinkParentV87, Option<ConversationRawSummaryV19>)> {
        if current.repository().permissions.pull != Some(true)
            || intent.identity().host != self.host
        {
            return Err(safe(GithubErrorKind::IdentityChanged));
        }
        checked(self, cancel, 128 * 1024, 0)?;
        let reservation = heap.reserve_document(128 * 1024)?;
        let mut spec = self.spec(vec![
            "api".into(),
            format!(
                "repos/{}/pulls/{}",
                current.repository().full_name,
                intent.number()
            ),
            "--hostname".into(),
            self.host.clone(),
            "--method".into(),
            "GET".into(),
        ]);
        spec.output_limit = spec.output_limit.min(128 * 1024);
        let output = self
            .runner
            .run(spec, cancel)
            .map_err(|failure| safe(GithubError::from(failure).kind))?;
        let document = heap.decode(reservation, &output.stdout)?;
        let raw = document.value();
        let body_null = match raw.get("body") {
            Some(Value::Null) => true,
            Some(Value::String(s)) if s.len() <= 65_536 => false,
            _ => return Err(safe(GithubErrorKind::Json)),
        };
        // Parser's bounded actor/title/ref/node/repository copies, actual Arc
        // relocation, digest and formatting scratch coexist with the raw tree.
        // Its fields have separate strict4KiB limits; this fixed128KiB debit
        // exceeds their combined requested layouts and survives every move.
        heap.charge(128 * 1024 + 2 * std::mem::size_of::<PrResourceParentV1>() + 256)?;
        let parent = parent_expected_v13(
            self,
            current,
            ExpectedPrParentV13::PersonalIntent(intent),
            raw,
        )?;
        if parent.retained_heap_bytes_v2() > 128 * 1024 {
            return Err(safe(GithubErrorKind::Transport));
        }
        let body_digest = format!(
            "{:x}",
            Sha256::digest(
                raw.get("body")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .as_bytes()
            )
        );
        let (parent, summary) = if with_summary {
            // Parent DTO, summary/body/actor conversion and its Arc relocation
            // coexist with the already-charged raw tree BEFORE any copies.
            heap.charge(128 * 1024 + 2 * std::mem::size_of::<ConversationRawSummaryV19>())?;
            let summary = ConversationRawSummaryV19::from_actual_raw_v19(self, parent, raw)?;
            (Arc::clone(summary.parent()), Some(summary))
        } else {
            (Arc::new(parent), None)
        };
        let result = IssueLinkParentV87 {
            parent,
            body_null,
            body_digest,
        };
        drop(output);
        heap.release_document(document)?;
        Ok((result, summary))
    }
}
