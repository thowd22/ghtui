//! Bounded navigation intent, NEVER a raw-parent/source/access constructor.
//! Personal search issue IDs and cached DTO database IDs are not retained.
use super::*;
use std::sync::Arc;

#[derive(Debug)]
pub struct PrParentIntentV13 {
    identity: HostIdentity,
    repository: String,
    number: u64,
    node: String,
    head: String,
    base: String,
    head_ref: String,
    base_ref: String,
    state: String,
    draft: bool,
    merged: bool,
}
impl PrParentIntentV13 {
    pub(crate) fn matches_observed_page_v13(&self, summary: &PrSummary) -> bool {
        self.identity.host == summary.pr.host
            && self.repository == summary.pr.repository
            && self.number == summary.pr.number
            && self.node == summary.pr.id
            && self.head == summary.head_sha
            && self.base == summary.base_sha
            && self.head_ref == summary.head_ref
            && self.base_ref == summary.base_ref
            && self.state == summary.pr.state
            && self.draft == summary.pr.draft
            && self.merged == summary.merged
    }
    /// Cached, bounded metadata only. The caller must capture its actual page
    /// allocation separately; this value grants no private read or write.
    pub(crate) fn from_observed_page_v13(
        identity: &HostIdentity,
        summary: &PrSummary,
    ) -> GithubResult<Arc<Self>> {
        let valid_ref = |value: &str| {
            !value.is_empty() && value.len() <= 1024 && !value.chars().any(char::is_control)
        };
        if identity.host != summary.pr.host
            || identity
                .account_id
                .is_none_or(|id| id == 0 || id > i64::MAX as u64)
            || identity.host.len() > 256
            || identity.gh_version.len() > 256
            || identity.login.len() > 100
            || identity.login.chars().any(char::is_control)
            || summary.pr.number == 0
            || summary.pr.number > i64::MAX as u64
            || summary.pr.repository.len() > 256
            || !opaque(&summary.pr.id)
            || !full_sha(&summary.head_sha)
            || !full_sha(&summary.base_sha)
            || !valid_ref(&summary.head_ref)
            || !valid_ref(&summary.base_ref)
            || !matches!(summary.pr.state.as_str(), "open" | "closed" | "merged")
            || summary.merged != (summary.pr.state == "merged")
        {
            return Err(safe(GithubErrorKind::InvalidTarget));
        }
        validate_repo(&summary.pr.repository)?;
        Ok(Arc::new(Self {
            identity: identity.clone(),
            repository: summary.pr.repository.clone(),
            number: summary.pr.number,
            node: summary.pr.id.clone(),
            head: summary.head_sha.clone(),
            base: summary.base_sha.clone(),
            head_ref: summary.head_ref.clone(),
            base_ref: summary.base_ref.clone(),
            state: summary.pr.state.clone(),
            draft: summary.pr.draft,
            merged: summary.merged,
        }))
    }
    pub fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub fn repository(&self) -> &str {
        &self.repository
    }
    pub fn number(&self) -> u64 {
        self.number
    }
    pub fn node_id(&self) -> &str {
        &self.node
    }
    pub fn head_sha(&self) -> &str {
        &self.head
    }
    pub fn base_sha(&self) -> &str {
        &self.base
    }
}

/// A borrowed expected tuple, not a parent observation. Both cases enter the
/// SAME existing raw parser; only the actual HTTP response supplies numeric IDs.
#[derive(Clone, Copy)]
pub(super) enum ExpectedPrParentV13<'a> {
    ExistingSummary(&'a PrSummary),
    PersonalIntent(&'a PrParentIntentV13),
    /// Publisher-private expectation, not a receipt or a source constructor.
    BaseReceiptV32(&'a BaseReceiptExpectationV32<'a>),
    /// Only the actual two-response sealed transition may enter paid refresh.
    ConfirmedBaseV32(&'a BaseTransitionV32),
    /// State varies ONLY open/closed; no DTO/known-ID/source construction.
    StateReceiptV68(&'a StateReceiptExpectationV68<'a>),
    ConfirmedStateV68(&'a StateTransitionV68),
    /// Private resource-family expectation borrowed from the SAME actual raw
    /// response after a genuine typed notification target has been checked.
    /// This is not an arbitrary DTO-to-parent or source constructor.
    NotificationObservedV63(&'a NotificationParentExpectedV63<'a>),
}
pub(super) struct NotificationParentExpectedV63<'a> {
    pub(super) host: &'a str,
    pub(super) number: u64,
    pub(super) raw: &'a Value,
}
impl ExpectedPrParentV13<'_> {
    pub(super) fn state_old_parent_v68(&self) -> Option<&PrResourceParentV1> {
        match self {
            Self::StateReceiptV68(v) => Some(v.old()),
            Self::ConfirmedStateV68(v) => Some(v.old().as_ref()),
            _ => None,
        }
    }
    pub(super) fn base_old_parent_v32(&self) -> Option<&PrResourceParentV1> {
        match self {
            Self::BaseReceiptV32(v) => Some(v.old()),
            Self::ConfirmedBaseV32(v) => Some(v.old().as_ref()),
            _ => None,
        }
    }
    pub(super) fn host(&self) -> &str {
        match self {
            Self::ExistingSummary(v) => &v.pr.host,
            Self::PersonalIntent(v) => &v.identity.host,
            Self::BaseReceiptV32(v) => &v.old().summary().pr.host,
            Self::ConfirmedBaseV32(v) => &v.old().summary().pr.host,
            Self::StateReceiptV68(v) => &v.old().summary().pr.host,
            Self::ConfirmedStateV68(v) => &v.old().summary().pr.host,
            Self::NotificationObservedV63(v) => v.host,
        }
    }
    pub(super) fn number(&self) -> u64 {
        match self {
            Self::ExistingSummary(v) => v.pr.number,
            Self::PersonalIntent(v) => v.number,
            Self::BaseReceiptV32(v) => v.old().number(),
            Self::ConfirmedBaseV32(v) => v.old().number(),
            Self::StateReceiptV68(v) => v.old().number(),
            Self::ConfirmedStateV68(v) => v.old().number(),
            Self::NotificationObservedV63(v) => v.number,
        }
    }
    pub(super) fn node(&self) -> &str {
        match self {
            Self::ExistingSummary(v) => &v.pr.id,
            Self::PersonalIntent(v) => &v.node,
            Self::BaseReceiptV32(v) => v.old().node_id(),
            Self::ConfirmedBaseV32(v) => v.old().node_id(),
            Self::StateReceiptV68(v) => v.old().node_id(),
            Self::ConfirmedStateV68(v) => v.old().node_id(),
            Self::NotificationObservedV63(v) => v.raw["node_id"].as_str().unwrap_or(""),
        }
    }
    pub(super) fn head(&self) -> &str {
        match self {
            Self::ExistingSummary(v) => &v.head_sha,
            Self::PersonalIntent(v) => &v.head,
            Self::BaseReceiptV32(v) => v.old().head_sha(),
            Self::ConfirmedBaseV32(v) => v.old().head_sha(),
            Self::StateReceiptV68(v) => v.old().head_sha(),
            Self::ConfirmedStateV68(v) => v.old().head_sha(),
            Self::NotificationObservedV63(v) => v.raw["head"]["sha"].as_str().unwrap_or(""),
        }
    }
    pub(super) fn base(&self) -> &str {
        match self {
            Self::ExistingSummary(v) => &v.base_sha,
            Self::PersonalIntent(v) => &v.base,
            Self::BaseReceiptV32(v) => v.base(),
            Self::ConfirmedBaseV32(v) => v.base(),
            Self::StateReceiptV68(v) => v.old().base_tip_sha(),
            Self::ConfirmedStateV68(v) => v.old().base_tip_sha(),
            Self::NotificationObservedV63(v) => v.raw["base"]["sha"].as_str().unwrap_or(""),
        }
    }
    pub(super) fn head_ref(&self) -> &str {
        match self {
            Self::ExistingSummary(v) => &v.head_ref,
            Self::PersonalIntent(v) => &v.head_ref,
            Self::BaseReceiptV32(v) => &v.old().summary().head_ref,
            Self::ConfirmedBaseV32(v) => &v.old().summary().head_ref,
            Self::StateReceiptV68(v) => &v.old().summary().head_ref,
            Self::ConfirmedStateV68(v) => &v.old().summary().head_ref,
            Self::NotificationObservedV63(v) => v.raw["head"]["ref"].as_str().unwrap_or(""),
        }
    }
    pub(super) fn base_ref(&self) -> &str {
        match self {
            Self::ExistingSummary(v) => &v.base_ref,
            Self::PersonalIntent(v) => &v.base_ref,
            Self::BaseReceiptV32(v) => v.base_ref(),
            Self::ConfirmedBaseV32(v) => v.base_ref(),
            Self::StateReceiptV68(v) => &v.old().summary().base_ref,
            Self::ConfirmedStateV68(v) => &v.old().summary().base_ref,
            Self::NotificationObservedV63(v) => v.raw["base"]["ref"].as_str().unwrap_or(""),
        }
    }
    pub(super) fn state(&self) -> &str {
        match self {
            Self::ExistingSummary(v) => &v.pr.state,
            Self::PersonalIntent(v) => &v.state,
            Self::BaseReceiptV32(v) => &v.old().summary().pr.state,
            Self::ConfirmedBaseV32(v) => &v.old().summary().pr.state,
            Self::StateReceiptV68(v) => v.state(),
            Self::ConfirmedStateV68(v) => v.state(),
            Self::NotificationObservedV63(v) => v.raw["state"].as_str().unwrap_or(""),
        }
    }
    pub(super) fn draft(&self) -> bool {
        match self {
            Self::ExistingSummary(v) => v.pr.draft,
            Self::PersonalIntent(v) => v.draft,
            Self::BaseReceiptV32(v) => v.old().summary().pr.draft,
            Self::ConfirmedBaseV32(v) => v.old().summary().pr.draft,
            Self::StateReceiptV68(v) => v.old().summary().pr.draft,
            Self::ConfirmedStateV68(v) => v.old().summary().pr.draft,
            Self::NotificationObservedV63(v) => v.raw["draft"].as_bool().unwrap_or(false),
        }
    }
    pub(super) fn merged(&self) -> bool {
        match self {
            Self::ExistingSummary(v) => v.merged,
            Self::PersonalIntent(v) => v.merged,
            Self::BaseReceiptV32(v) => v.old().summary().merged,
            Self::ConfirmedBaseV32(v) => v.old().summary().merged,
            Self::StateReceiptV68(v) => v.old().summary().merged,
            Self::ConfirmedStateV68(v) => v.old().summary().merged,
            Self::NotificationObservedV63(v) => v.raw["merged"].as_bool().unwrap_or(false),
        }
    }
}
