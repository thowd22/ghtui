//! Closed Base observations from the SAME original Core/measured allocation.
//! Neither a row/index nor a receipt is a source or permission constructor.
use super::*;
use crate::providers::runner::ScopedReadBudgetV1;
use sha2::{Digest, Sha256};
use std::{
    sync::{Arc, Weak},
    time::Instant,
};

pub(crate) const METADATA_BASE_POST_CAPS_V32: [usize; 12] = METADATA_TEXT_POST_CAPS_V25;
pub(crate) const BASE_PAGE_CAP_V32: usize = 128 * 1024;
const RAW_ROWS_CAP: usize = 1024 * 1024;
const ROW_CAP: usize = 16 * 1024;

fn failure(kind: GithubErrorKind) -> GithubError {
    error(
        kind,
        "Exact original Base branch/source/principal unavailable; no fallback or resend",
    )
}
fn full_sha(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit())
}
pub(super) fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 1024
        && value != "@"
        && value != "HEAD"
        && !value.starts_with('-')
        && !value.starts_with('/')
        && !value.ends_with('/')
        && !value.ends_with('.')
        && !value.contains("..")
        && !value.contains("@{")
        && !value
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || "~^:?*[\\".contains(c))
        && value
            .split('/')
            .all(|p| !p.is_empty() && !p.starts_with('.') && !p.ends_with(".lock"))
}
fn encoded(value: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(char::from(b));
        } else {
            write!(out, "%{b:02X}").expect("String formatting cannot fail");
        }
    }
    out
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct BaseCatalogueLimitsV32 {
    pub(crate) pages: usize,
    pub(crate) rows: usize,
}
impl Default for BaseCatalogueLimitsV32 {
    fn default() -> Self {
        Self {
            pages: 10,
            rows: 1000,
        }
    }
}
impl BaseCatalogueLimitsV32 {
    pub(crate) fn validate(self) -> GithubResult<()> {
        if !(1..=10).contains(&self.pages) || !(1..=1000).contains(&self.rows) {
            return Err(failure(GithubErrorKind::InvalidTarget));
        }
        Ok(())
    }
    pub(crate) fn measured_requests(self) -> usize {
        self.pages + 3
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BaseCoverageStatusV32 {
    Complete,
    Partial,
    Unavailable,
}
pub(crate) struct BaseCoverageV32 {
    pub(crate) status: BaseCoverageStatusV32,
    pub(crate) requested_pages: usize,
    pub(crate) fetched_pages: usize,
    pub(crate) fetched_rows: usize,
    pub(crate) excluded_rows: usize,
    pub(crate) duplicates: usize,
    pub(crate) reason: Option<&'static str>,
}
pub(crate) struct BaseBranchRowV32 {
    name: Option<Arc<str>>,
    tip: Option<Arc<str>>,
    protected: Option<bool>,
    disabled: Option<&'static str>,
}
impl BaseBranchRowV32 {
    pub(crate) fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    pub(crate) fn tip(&self) -> Option<&str> {
        self.tip.as_deref()
    }
    pub(crate) fn protected(&self) -> Option<bool> {
        self.protected
    }
    pub(crate) fn disabled(&self) -> Option<&'static str> {
        self.disabled
    }
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.name.as_ref().map_or(0, |s| s.len())
            + self.tip.as_ref().map_or(0, |s| s.len())
    }
}
struct Count {
    bytes: usize,
    limit: usize,
    hash: Sha256,
}
impl std::io::Write for Count {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(bytes.len())
            .filter(|n| *n <= self.limit)
            .ok_or_else(|| std::io::Error::other("Bounded Base observation exceeded"))?;
        self.hash.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn bounded_digest(raw: &Value, limit: usize) -> GithubResult<(usize, [u8; 32])> {
    let mut count = Count {
        bytes: 0,
        limit,
        hash: Sha256::new(),
    };
    serde_json::to_writer(&mut count, raw).map_err(|_| failure(GithubErrorKind::Json))?;
    Ok((count.bytes, count.hash.finalize().into()))
}
struct BaseReadBindingV32 {
    issuer: ThreadPhaseIssuerV16,
    pool: Weak<ScopedReadBudgetV1>,
    cancel: CancellationToken,
    deadline: Instant,
}
fn measured(
    provider: &GithubProvider,
    cancel: &CancellationToken,
) -> GithubResult<(Arc<ScopedReadBudgetV1>, Instant)> {
    let pool = provider
        .scoped_read_budget
        .as_ref()
        .ok_or_else(|| failure(GithubErrorKind::InvalidTarget))?;
    let deadline = provider
        .scoped_read_window
        .ok_or_else(|| failure(GithubErrorKind::InvalidTarget))?
        .0
        .min(pool.deadline());
    pool.check_clock(cancel).map_err(GithubError::from)?;
    if cancel.is_cancelled()
        || Instant::now() >= deadline
        || pool.accounting().map_err(GithubError::from)?.sealed()
    {
        return Err(failure(GithubErrorKind::Transport));
    }
    Ok((Arc::clone(pool), deadline))
}
impl BaseReadBindingV32 {
    fn validate_lineage(
        &self,
        provider: &GithubProvider,
        pool: &Arc<ScopedReadBudgetV1>,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        if !self.issuer.matches(provider)
            || !self.pool.ptr_eq(&Arc::downgrade(pool))
            || !self.cancel.same_instance(cancel)
            || !pool.original_token_matches_v63(cancel)
            || cancel.is_cancelled()
            || Instant::now() >= deadline.min(self.deadline).min(pool.deadline())
        {
            return Err(failure(GithubErrorKind::IdentityChanged));
        }
        Ok(())
    }
    fn validate(
        &self,
        provider: &GithubProvider,
        cancel: &CancellationToken,
    ) -> GithubResult<Arc<ScopedReadBudgetV1>> {
        let (pool, deadline) = measured(provider, cancel)?;
        self.validate_lineage(provider, &pool, cancel, deadline)?;
        Ok(pool)
    }
}
pub(crate) struct PrMetadataBaseReadV32 {
    parent: Arc<PrResourceParentV1>,
    binding: Arc<BaseReadBindingV32>,
    generation: u64,
}
impl PrMetadataBaseReadV32 {
    pub(crate) fn parent(&self) -> &Arc<PrResourceParentV1> {
        &self.parent
    }
    pub(crate) fn same_observation(&self, other: &Self) -> bool {
        self.generation == other.generation
            && self.binding.pool.ptr_eq(&other.binding.pool)
            && self.binding.cancel.same_instance(&other.binding.cancel)
            && self.binding.deadline == other.binding.deadline
            && GithubProvider::same_thread_history_parent_v1(&self.parent, &other.parent)
    }
}
pub(crate) struct OriginalMetadataBaseV32 {
    observed: Arc<PrMetadataBaseReadV32>,
    identity: HostIdentity,
    can_update: bool,
    measured: Arc<ScopedReadBudgetV1>,
}
impl OriginalMetadataBaseV32 {
    pub(crate) fn observed(&self) -> &Arc<PrMetadataBaseReadV32> {
        &self.observed
    }
    pub(crate) fn parent(&self) -> &Arc<PrResourceParentV1> {
        self.observed.parent()
    }
    pub(crate) fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub(crate) fn can_update(&self) -> bool {
        self.can_update
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.observed.binding.deadline
    }
    pub(crate) fn validate_live(&self) -> GithubResult<()> {
        let pool = &self.measured;
        pool.check_clock(&self.observed.binding.cancel)
            .map_err(GithubError::from)?;
        if pool.accounting().map_err(GithubError::from)?.sealed() {
            return Err(failure(GithubErrorKind::Transport));
        }
        Ok(())
    }
    pub(crate) fn require_change(&self, selection: &BaseSelectionV32) -> GithubResult<()> {
        if self.observed_no_change_v39(selection)? {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Selected Base branch is unchanged; no request advances its frozen PR base commit",
            ));
        }
        if !self.can_update
            || self.parent().summary().pr.state != "open"
            || self.parent().summary().merged
        {
            return Err(failure(GithubErrorKind::Permission));
        }
        Ok(())
    }
    /// Closed factual NoChange check on the actual raw/capability producer.
    /// It is never a source, preview, permission or old pool lifetime renewal.
    pub(crate) fn observed_no_change_v39(
        &self,
        selection: &BaseSelectionV32,
    ) -> GithubResult<bool> {
        self.validate_live()?;
        if !GithubProvider::same_thread_history_parent_v1(self.parent(), selection.parent()) {
            return Err(failure(GithubErrorKind::IdentityChanged));
        }
        Ok(selection.name() == self.parent().summary().base_ref.as_str())
    }
}
pub(crate) struct BaseCatalogueV32 {
    actual: Arc<OriginalMetadataBaseV32>,
    rows: Vec<BaseBranchRowV32>,
    coverage: BaseCoverageV32,
    retained: usize,
}
impl BaseCatalogueV32 {
    pub(crate) fn actual(&self) -> &Arc<OriginalMetadataBaseV32> {
        &self.actual
    }
    pub(crate) fn rows(&self) -> &[BaseBranchRowV32] {
        &self.rows
    }
    pub(crate) fn coverage(&self) -> &BaseCoverageV32 {
        &self.coverage
    }
    pub(crate) fn retained(&self) -> usize {
        self.retained
    }
    pub(crate) fn choose(self: &Arc<Self>, index: usize) -> GithubResult<Arc<BaseSelectionV32>> {
        let row = self
            .rows
            .get(index)
            .filter(|row| row.disabled.is_none() && row.name.is_some() && row.tip.is_some())
            .ok_or_else(|| failure(GithubErrorKind::InvalidTarget))?;
        if self.coverage.status == BaseCoverageStatusV32::Unavailable || row.name().is_none() {
            return Err(failure(GithubErrorKind::Unsupported));
        }
        Ok(Arc::new(BaseSelectionV32 {
            catalogue: Arc::clone(self),
            index,
        }))
    }
}
pub(crate) struct BaseSelectionV32 {
    catalogue: Arc<BaseCatalogueV32>,
    index: usize,
}
impl BaseSelectionV32 {
    pub(crate) fn name(&self) -> &str {
        self.catalogue.rows[self.index]
            .name()
            .expect("typed actual row")
    }
    pub(crate) fn tip(&self) -> &str {
        self.catalogue.rows[self.index]
            .tip()
            .expect("typed actual full tip")
    }
    pub(crate) fn parent(&self) -> &Arc<PrResourceParentV1> {
        self.catalogue.actual.parent()
    }
    pub(crate) fn same_allocation(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.catalogue, &other.catalogue) && self.index == other.index
    }
}

/// Description only, made inside the one-take publisher after before_write.
/// It is NOT a transition, receipt, Current, source or permission.
pub(super) struct BaseReceiptExpectationV32<'a> {
    actual: &'a OriginalMetadataBaseV32,
    selection: &'a Arc<BaseSelectionV32>,
}
impl BaseReceiptExpectationV32<'_> {
    pub(super) fn old(&self) -> &PrResourceParentV1 {
        self.actual.parent()
    }
    pub(super) fn base(&self) -> &str {
        self.selection.tip()
    }
    pub(super) fn base_ref(&self) -> &str {
        self.selection.name()
    }
}
pub(crate) struct BaseTransitionV32 {
    old: Arc<PrResourceParentV1>,
    new: Arc<PrResourceParentV1>,
    selection: Arc<BaseSelectionV32>,
    binding: Arc<BaseReadBindingV32>,
    measured: Arc<ScopedReadBudgetV1>,
}
impl BaseTransitionV32 {
    pub(crate) fn old(&self) -> &Arc<PrResourceParentV1> {
        &self.old
    }
    pub(crate) fn parent(&self) -> &Arc<PrResourceParentV1> {
        &self.new
    }
    pub(super) fn base(&self) -> &str {
        self.selection.tip()
    }
    pub(super) fn base_ref(&self) -> &str {
        self.selection.name()
    }
    pub(crate) fn matches_selection(&self, selected: &BaseSelectionV32) -> bool {
        self.selection.same_allocation(selected)
    }
}
/// Stable actual identity only. B/ref and comparison-derived counts are parsed
/// anew under a CLOSED expectation; ordinary same_parent is never relaxed.
pub(super) fn stable_parent(old: &PrResourceParentV1, new: &PrResourceParentV1) -> bool {
    old.database_id == new.database_id
        && old.author_account_id == new.author_account_id
        && old.repository_id == new.repository_id
        && old.repository_node == new.repository_node
        && old.head_repository == new.head_repository
        && old.summary.pr.host == new.summary.pr.host
        && old.summary.pr.id == new.summary.pr.id
        && old.summary.pr.repository == new.summary.pr.repository
        && old.summary.pr.number == new.summary.pr.number
        && old.summary.head_sha == new.summary.head_sha
        && old.summary.head_ref == new.summary.head_ref
        && old.summary.pr.state == new.summary.pr.state
        && old.summary.pr.draft == new.summary.pr.draft
        && old.summary.merged == new.summary.merged
}
pub(crate) struct PrMetadataBaseReceiptV32 {
    transition: Arc<BaseTransitionV32>,
    observation: Weak<PrMetadataBaseReadV32>,
    pool: Weak<ScopedReadBudgetV1>,
    cancel: CancellationToken,
    deadline: Instant,
}
impl PrMetadataBaseReceiptV32 {
    pub(crate) fn publication_cutoff_v51(&self) -> Instant {
        self.deadline
    }
    pub(crate) fn transition(&self) -> &Arc<BaseTransitionV32> {
        &self.transition
    }
    pub(crate) fn parent(&self) -> &Arc<PrResourceParentV1> {
        self.transition.parent()
    }
    pub(crate) fn validates_refresh(
        &self,
        actual: &OriginalMetadataBaseV32,
        selection: &BaseSelectionV32,
        cancel: &CancellationToken,
    ) -> bool {
        self.observation.ptr_eq(&Arc::downgrade(actual.observed()))
            && self.pool.ptr_eq(&actual.observed.binding.pool)
            && self.cancel.same_instance(cancel)
            && actual.observed.binding.cancel.same_instance(cancel)
            && !cancel.is_cancelled()
            && Instant::now() < self.deadline.min(actual.deadline())
            && Arc::ptr_eq(self.transition.old(), actual.parent())
            && self.transition.matches_selection(selection)
    }
    pub(in crate::providers::gh) fn validates_paid_context_v51(
        &self,
        actual: &OriginalMetadataBaseV32,
        selection: &BaseSelectionV32,
        cancel: &CancellationToken,
        pool: &Weak<ScopedReadBudgetV1>,
    ) -> bool {
        self.validates_refresh(actual, selection, cancel) && self.pool.ptr_eq(pool)
    }
}
pub(crate) enum PrMetadataBaseFailureV32 {
    NotStarted(GithubError),
    WriteMayHaveStarted(GithubError),
}
/// Point-in-time current Base read, never a Base transition, selected member,
/// source or permission. It carries the actual pre-child runner generation.
pub(crate) struct BaseRecoveryCurrentV32 {
    actual: OriginalMetadataBaseV32,
}
impl BaseRecoveryCurrentV32 {
    pub(crate) fn actual(&self) -> &OriginalMetadataBaseV32 {
        &self.actual
    }
    pub(crate) fn cancellation(&self) -> &CancellationToken {
        &self.actual.observed.binding.cancel
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.actual.observed.binding.deadline
    }
    pub(crate) fn matches_target(&self, target: &crate::pr_write_journal::PrWriteTarget) -> bool {
        let p = self.actual.parent();
        crate::storage::Scope::github(
            &self.actual.identity().host,
            self.actual.identity().account_id,
        )
        .ok()
        .as_ref()
            == Some(target.scope())
            && target.repository() == p.repository()
            && target.node_id() == p.node_id()
            && target.number() == p.number()
    }
}

/// One fresh explicit read phase, not a paid-token extension or a source.
/// Constructed only from an actual Base OwnFence and its actual V20 attempt.
pub(crate) struct OriginalBaseRefreshReadV36 {
    own: Arc<crate::storage::conversation_refresh_v19::ConversationOwnFenceV19>,
    transition: Arc<BaseTransitionV32>,
    attempt:
        Option<Arc<crate::storage::conversation_refresh_v19::PersonalConversationReadAttemptV20>>,
    binding: Arc<BaseReadBindingV32>,
}
impl OriginalBaseRefreshReadV36 {
    fn validate(&self, provider: &GithubProvider, cancel: &CancellationToken) -> GithubResult<()> {
        self.binding.validate(provider, cancel)?;
        if !self.transition.binding.issuer.matches(provider)
            || !Arc::ptr_eq(self.own.terminal().parent(), self.transition.parent())
            || self
                .own
                .terminal()
                .base_transition_v32()
                .is_none_or(|actual| !Arc::ptr_eq(actual, &self.transition))
            || !GithubProvider::same_thread_history_parent_v1(
                self.own.source().parent(),
                self.transition.old(),
            )
        {
            return Err(failure(GithubErrorKind::IdentityChanged));
        }
        match (&self.attempt, self.own.is_personal_v20()) {
            (Some(attempt), true) if attempt.matches_own(&self.own) => attempt
                .matches_phase(cancel, self.binding.deadline)
                .map_err(|_| failure(GithubErrorKind::IdentityChanged))?,
            (None, false) => {}
            _ => return Err(failure(GithubErrorKind::IdentityChanged)),
        }
        self.own
            .terminal()
            .validate_file_v19()
            .map_err(|_| failure(GithubErrorKind::IdentityChanged))
    }
}

impl GithubProvider {
    pub(crate) fn inspect_original_base_v32(
        &self,
        actual: OriginalMetadataBaseV32,
        cancel: &CancellationToken,
    ) -> GithubResult<BaseRecoveryCurrentV32> {
        actual.validate_live()?;
        let pool = actual.observed.binding.validate(self, cancel)?;
        if !Arc::ptr_eq(&pool, &actual.measured) {
            return Err(failure(GithubErrorKind::IdentityChanged));
        }
        self.validate_original_base_generation_v32(&actual, cancel, actual.deadline())?;
        Ok(BaseRecoveryCurrentV32 { actual })
    }
    pub(crate) fn validate_base_inspection_v32(
        &self,
        current: &BaseRecoveryCurrentV32,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        self.validate_original_base_generation_v32(&current.actual, cancel, deadline)
    }
    pub(crate) fn reconcile_base_inspection_v32(
        &self,
        current: BaseRecoveryCurrentV32,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        self.validate_base_inspection_v32(&current, cancel, deadline)?;
        self.runner
            .reconcile_review_generation_v11(
                &self.resource(
                    current.actual.parent().repository(),
                    current.actual.parent().number(),
                ),
                current.actual.observed.generation,
                cancel,
                deadline.min(current.deadline()),
            )
            .map_err(GithubError::from)
    }
    pub(crate) fn begin_original_base_refresh_read_v36(
        &self,
        own: &Arc<crate::storage::conversation_refresh_v19::ConversationOwnFenceV19>,
        attempt: Option<
            &Arc<crate::storage::conversation_refresh_v19::PersonalConversationReadAttemptV20>,
        >,
        cancel: &CancellationToken,
    ) -> GithubResult<OriginalBaseRefreshReadV36> {
        let (pool, deadline) = measured(self, cancel)?;
        let transition = Arc::clone(
            own.terminal()
                .base_transition_v32()
                .ok_or_else(|| failure(GithubErrorKind::InvalidTarget))?,
        );
        let descriptor = OriginalBaseRefreshReadV36 {
            own: Arc::clone(own),
            transition,
            attempt: attempt.map(Arc::clone),
            binding: Arc::new(BaseReadBindingV32 {
                issuer: ThreadPhaseIssuerV16::capture(self)?,
                pool: Arc::downgrade(&pool),
                cancel: cancel.clone(),
                deadline,
            }),
        };
        descriptor.validate(self, cancel)?;
        Ok(descriptor)
    }
    /// Fixed opening/closing GETs on the NEW actual Core's measured pool.
    /// The paid parser remains on its original token/clock and is not reused.
    pub(crate) fn pr_base_refresh_summary_v36(
        &self,
        current: &CurrentCatalogueRepository,
        actual: &OriginalBaseRefreshReadV36,
        cancel: &CancellationToken,
    ) -> GithubResult<ConversationRawSummaryV19> {
        actual.validate(self, cancel)?;
        let expected = actual.transition.parent();
        if current.repository().id != expected.repository_id()
            || current.original().id != expected.repository_id()
            || current.repository().node_id.as_deref() != Some(expected.repository_node())
            || current.repository().full_name != expected.repository()
        {
            return Err(failure(GithubErrorKind::IdentityChanged));
        }
        let raw = document(
            self,
            &format!(
                "repos/{}/pulls/{}",
                expected.repository(),
                expected.number()
            ),
            128 * 1024,
            cancel,
            // This is one of the closed refresh's two summary GETs. The
            // original Core already reserved all seven measured children;
            // the final summary must consume its ONE remaining slot, like
            // original_detail_parent_raw_v19, not demand a phantom eighth.
            true,
        )?;
        let parent = parse::parent_expected_v13(
            self,
            current,
            ExpectedPrParentV13::ConfirmedBaseV32(&actual.transition),
            &raw,
        )?;
        if !parse::same_parent(expected, &parent) {
            return Err(failure(GithubErrorKind::StaleHead));
        }
        let summary = ConversationRawSummaryV19::from_actual_raw_v19(self, parent, &raw)?;
        actual.validate(self, cancel)?;
        Ok(summary)
    }
    /// Metadata context in a NEW explicit Base Refresh is checked under that
    /// actual fresh binding/Iread, NEVER the expired original paid binding.
    pub(in crate::providers::gh) fn parse_base_refresh_context_v51(
        &self,
        current: &CurrentCatalogueRepository,
        actual: &OriginalBaseRefreshReadV36,
        raw: &Value,
        cancel: &CancellationToken,
    ) -> GithubResult<PrResourceParentV1> {
        actual.validate(self, cancel)?;
        let expected = actual.transition.parent();
        if current.repository().id != expected.repository_id()
            || current.original().id != expected.repository_id()
            || current.repository().node_id.as_deref() != Some(expected.repository_node())
            || current.repository().full_name != expected.repository()
        {
            return Err(failure(GithubErrorKind::IdentityChanged));
        }
        let parent = parse::parent_expected_v13(
            self,
            current,
            ExpectedPrParentV13::ConfirmedBaseV32(&actual.transition),
            raw,
        )?;
        if !parse::same_parent(expected, &parent) {
            return Err(failure(GithubErrorKind::StaleHead));
        }
        actual.validate(self, cancel)?;
        Ok(parent)
    }
    pub(in crate::providers::gh) fn metadata_base_refresh_target_v51<'a>(
        &self,
        current: &CurrentCatalogueRepository,
        actual: &'a OriginalBaseRefreshReadV36,
        cancel: &CancellationToken,
    ) -> GithubResult<&'a PrResourceParentV1> {
        actual.validate(self, cancel)?;
        let parent = actual.transition.parent();
        if current.repository().id != parent.repository_id()
            || current.original().id != parent.repository_id()
            || current.repository().node_id.as_deref() != Some(parent.repository_node())
            || current.repository().full_name != parent.repository()
        {
            return Err(failure(GithubErrorKind::IdentityChanged));
        }
        Ok(parent)
    }
    /// Only actual two-response cursor2 transitions can select expected-new B.
    /// Fixed paid raw GET callers only; no arbitrary summary/DTO constructor.
    pub(in crate::providers::gh) fn parse_confirmed_base_parent_v32(
        &self,
        current: &CurrentCatalogueRepository,
        transition: &BaseTransitionV32,
        raw: &Value,
    ) -> GithubResult<PrResourceParentV1> {
        transition.binding.validate_lineage(
            self,
            &transition.measured,
            &transition.binding.cancel,
            transition.binding.deadline,
        )?;
        if current.repository().id != transition.new.repository_id()
            || current.original().id != transition.new.repository_id()
            || current.repository().node_id.as_deref() != Some(transition.new.repository_node())
            || current.repository().full_name != transition.new.repository()
        {
            return Err(failure(GithubErrorKind::IdentityChanged));
        }
        parse::parent_expected_v13(
            self,
            current,
            ExpectedPrParentV13::ConfirmedBaseV32(transition),
            raw,
        )
    }
    pub(crate) fn pr_original_metadata_base_v32(
        &self,
        current: &CurrentCatalogueRepository,
        intent: &PrParentIntentV13,
        cancel: &CancellationToken,
    ) -> GithubResult<Arc<PrMetadataBaseReadV32>> {
        let (pool, deadline) = measured(self, cancel)?;
        let binding = Arc::new(BaseReadBindingV32 {
            issuer: ThreadPhaseIssuerV16::capture(self)?,
            pool: Arc::downgrade(&pool),
            cancel: cancel.clone(),
            deadline,
        });
        let generation = self
            .runner
            .review_resource_generation_v11(
                &self.resource(&current.repository().full_name, intent.number()),
                cancel,
                deadline,
            )
            .map_err(GithubError::from)?;
        let parent = self.pr_original_detail_parent_v17(current, intent, cancel)?;
        Ok(Arc::new(PrMetadataBaseReadV32 {
            parent: Arc::new(parent),
            binding,
            generation,
        }))
    }
    pub(crate) fn observe_original_metadata_base_v32(
        &self,
        observed: Arc<PrMetadataBaseReadV32>,
        identity: &HostIdentity,
        cancel: &CancellationToken,
    ) -> GithubResult<OriginalMetadataBaseV32> {
        let measured = observed.binding.validate(self, cancel)?;
        if identity.host != self.host
            || identity
                .account_id
                .is_none_or(|id| !(1..=i64::MAX as u64).contains(&id))
            || observed.parent().summary().pr.host != self.host
        {
            return Err(failure(GithubErrorKind::IdentityChanged));
        }
        let can_update = self.original_metadata_can_update_v25(observed.parent(), cancel)?;
        observed.binding.validate(self, cancel)?;
        Ok(OriginalMetadataBaseV32 {
            observed,
            identity: identity.clone(),
            can_update,
            measured,
        })
    }
    pub(crate) fn metadata_base_catalogue_v32(
        &self,
        actual: Arc<OriginalMetadataBaseV32>,
        limits: BaseCatalogueLimitsV32,
        cancel: &CancellationToken,
    ) -> GithubResult<Arc<BaseCatalogueV32>> {
        limits.validate()?;
        actual.observed.binding.validate(self, cancel)?;
        let mut rows: Vec<BaseBranchRowV32> = Vec::with_capacity(limits.rows);
        let mut names: BTreeMap<String, [u8; 32]> = BTreeMap::new();
        let mut raw_bytes = 0usize;
        let mut normalized_bytes = 0usize;
        let mut coverage = BaseCoverageV32 {
            status: BaseCoverageStatusV32::Partial,
            requested_pages: 0,
            fetched_pages: 0,
            fetched_rows: 0,
            excluded_rows: 0,
            duplicates: 0,
            reason: Some("Bounded branch observation; unseen branches are unknown"),
        };
        'pages: for page in 1..=limits.pages {
            actual.observed.binding.validate(self, cancel)?;
            coverage.requested_pages += 1;
            let raw = match self.api(
                &format!(
                    "repos/{}/branches?per_page=100&page={page}",
                    actual.parent().repository()
                ),
                "GET",
                None,
                None,
                false,
                cancel,
            ) {
                Ok(raw) => raw,
                Err(err)
                    if matches!(
                        err.kind,
                        GithubErrorKind::Authentication
                            | GithubErrorKind::IdentityChanged
                            | GithubErrorKind::OutputLimit
                    ) =>
                {
                    return Err(err)
                }
                Err(err) => {
                    coverage.reason = Some(match err.kind {
                        GithubErrorKind::Permission => {
                            "Branch endpoint forbidden (HTTP 403); no empty catalogue inferred"
                        }
                        _ => "Branch endpoint unavailable; no empty catalogue inferred",
                    });
                    if rows.is_empty() {
                        coverage.status = BaseCoverageStatusV32::Unavailable;
                    }
                    break;
                }
            };
            let Some(page_rows) = raw.as_array() else {
                coverage.reason = Some("Malformed branch page; no empty catalogue inferred");
                if rows.is_empty() {
                    coverage.status = BaseCoverageStatusV32::Unavailable;
                }
                break;
            };
            if page_rows.len() > 100 {
                return Err(failure(GithubErrorKind::Json));
            }
            coverage.fetched_pages += 1;
            for row in page_rows {
                coverage.fetched_rows += 1;
                let (bytes, digest) = match bounded_digest(row, ROW_CAP) {
                    Ok(v) => v,
                    Err(_) => {
                        coverage.excluded_rows += 1;
                        continue;
                    }
                };
                if raw_bytes
                    .checked_add(bytes)
                    .is_none_or(|n| n > RAW_ROWS_CAP)
                    || rows.len() >= limits.rows
                {
                    coverage.reason = Some(
                        "Branch row/retained byte limit reached; unseen branches remain unknown",
                    );
                    break 'pages;
                }
                raw_bytes += bytes;
                let name = row
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|n| valid_name(n));
                let tip = row["commit"]["sha"].as_str().filter(|s| full_sha(s));
                let next_normalized = normalized_bytes
                    .checked_add(name.map_or(0, str::len))
                    .and_then(|n| n.checked_add(tip.map_or(0, str::len)))
                    .filter(|n| *n <= 1024 * 1024)
                    .ok_or_else(|| failure(GithubErrorKind::Unsupported))?;
                let peak = next_normalized
                    .checked_mul(2)
                    .and_then(|n| {
                        n.checked_add(rows.capacity() * std::mem::size_of::<BaseBranchRowV32>())
                    })
                    .and_then(|n| n.checked_add(limits.rows * (std::mem::size_of::<String>() + 64)))
                    .and_then(|n| {
                        n.checked_add(BASE_PAGE_CAP_V32 + actual.parent().retained_heap_bytes_v2())
                    })
                    .filter(|n| *n <= 8 * 1024 * 1024);
                if peak.is_none() {
                    return Err(failure(GithubErrorKind::Unsupported));
                }
                if let Some(name) = name {
                    if let Some(previous) = names.get(name) {
                        if previous != &digest {
                            return Err(failure(GithubErrorKind::IdentityChanged));
                        }
                        coverage.duplicates += 1;
                        continue;
                    }
                    names.insert(name.into(), digest);
                }
                normalized_bytes = next_normalized;
                let disabled = if name.is_none() || tip.is_none() {
                    Some("Branch name/full tip unavailable; no target guessed")
                } else if !actual.can_update() {
                    Some("Current object update capability not allowed")
                } else {
                    None
                };
                if disabled.is_some() {
                    coverage.excluded_rows += 1;
                }
                rows.push(BaseBranchRowV32 {
                    name: name.map(Arc::from),
                    tip: tip.map(Arc::from),
                    protected: row.get("protected").and_then(Value::as_bool),
                    disabled,
                });
            }
            if page_rows.len() < 100 {
                if coverage.excluded_rows == 0 {
                    coverage.status = BaseCoverageStatusV32::Complete;
                    coverage.reason = None;
                } else {
                    coverage.reason = Some(
                        "Some branch rows are unavailable; only known validated members selectable",
                    );
                }
                break;
            }
        }
        actual.observed.binding.validate(self, cancel)?;
        let retained = std::mem::size_of::<BaseCatalogueV32>()
            + rows.capacity() * std::mem::size_of::<BaseBranchRowV32>()
            + rows.iter().map(BaseBranchRowV32::bytes).sum::<usize>()
            + actual.parent().retained_heap_bytes_v2();
        if retained > 2 * 1024 * 1024 {
            return Err(failure(GithubErrorKind::Unsupported));
        }
        Ok(Arc::new(BaseCatalogueV32 {
            actual,
            rows,
            coverage,
            retained,
        }))
    }
    pub(crate) fn validate_selected_metadata_base_v32(
        &self,
        actual: &OriginalMetadataBaseV32,
        selected: &BaseSelectionV32,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        actual.observed.binding.validate(self, cancel)?;
        if !GithubProvider::same_thread_history_parent_v1(actual.parent(), selected.parent()) {
            return Err(failure(GithubErrorKind::IdentityChanged));
        }
        let row = self.api(
            &format!(
                "repos/{}/branches/{}",
                actual.parent().repository(),
                encoded(selected.name())
            ),
            "GET",
            None,
            None,
            false,
            cancel,
        )?;
        bounded_digest(&row, ROW_CAP)?;
        if row["name"].as_str() != Some(selected.name())
            || row["commit"]["sha"].as_str() != Some(selected.tip())
        {
            return Err(failure(GithubErrorKind::StaleHead));
        }
        actual.observed.binding.validate(self, cancel)?;
        Ok(())
    }
    pub(crate) fn validate_original_base_generation_v32(
        &self,
        actual: &OriginalMetadataBaseV32,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        let deadline = deadline
            .min(actual.deadline())
            .min(actual.measured.deadline());
        actual
            .observed
            .binding
            .validate_lineage(self, &actual.measured, cancel, deadline)?;
        if self
            .runner
            .review_resource_generation_v11(
                &self.resource(actual.parent().repository(), actual.parent().number()),
                cancel,
                deadline.min(actual.deadline()),
            )
            .map_err(GithubError::from)?
            != actual.observed.generation
        {
            return Err(failure(GithubErrorKind::IdentityChanged));
        }
        Ok(())
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Actual original publisher, selected member, same prepaid cursor and final SQL/UID callback remain explicit"
    )]
    pub(crate) fn publish_original_metadata_base_v32(
        &self,
        measured_provider: &GithubProvider,
        captures: &mut PostWriteCaptureAllowanceV2,
        actual: &OriginalMetadataBaseV32,
        current: &CurrentCatalogueRepository,
        selection: &Arc<BaseSelectionV32>,
        cancel: &CancellationToken,
        before_write: impl FnOnce() -> GithubResult<HostIdentity>,
    ) -> Result<PrMetadataBaseReceiptV32, PrMetadataBaseFailureV32> {
        let mut invoked = false;
        let result = (|| {
            actual
                .observed
                .binding
                .validate(measured_provider, cancel)?;
            captures.validate_prewrite(measured_provider, cancel)?;
            actual.require_change(selection)?;
            let identity = before_write()?;
            if !identity.same_account(actual.identity()) {
                return Err(failure(GithubErrorKind::IdentityChanged));
            }
            let expected = BaseReceiptExpectationV32 { actual, selection };
            let parent = actual.parent();
            let endpoint = format!("repos/{}/pulls/{}", parent.repository(), parent.number());
            let input = serde_json::to_value(PrTextEdit {
                base: Some(selection.name().into()),
                ..Default::default()
            })
            .map_err(|_| failure(GithubErrorKind::Json))?;
            let bind = |raw: &Value| {
                if raw.get("errors").is_some_and(|v| !v.is_null()) || raw.get("message").is_some() {
                    return Err(failure(GithubErrorKind::AmbiguousWrite));
                }
                parse::parent_expected_v13(
                    self,
                    current,
                    ExpectedPrParentV13::BaseReceiptV32(&expected),
                    raw,
                )
            };
            invoked = true;
            let raw = captures.api(
                self,
                &endpoint,
                "PATCH",
                Some(input),
                Some(self.resource(parent.repository(), parent.number())),
                cancel,
            )?;
            let patch = bind(&raw)?;
            let raw = captures.api(self, &endpoint, "GET", None, None, cancel)?;
            let get = bind(&raw)?;
            if !parse::same_parent(&patch, &get) || captures.consumed_requests() != 2 {
                return Err(failure(GithubErrorKind::AmbiguousWrite));
            }
            Ok(PrMetadataBaseReceiptV32 {
                transition: Arc::new(BaseTransitionV32 {
                    old: Arc::clone(parent),
                    new: Arc::new(get),
                    selection: Arc::clone(selection),
                    binding: Arc::clone(&actual.observed.binding),
                    measured: Arc::clone(&actual.measured),
                }),
                observation: Arc::downgrade(actual.observed()),
                pool: actual.observed.binding.pool.clone(),
                cancel: cancel.clone(),
                deadline: actual.deadline().min(captures.deadline()),
            })
        })();
        result.map_err(|err| {
            if invoked {
                self.runner.mark_ambiguous(
                    &self.resource(actual.parent().repository(), actual.parent().number()),
                );
                PrMetadataBaseFailureV32::WriteMayHaveStarted(err)
            } else {
                PrMetadataBaseFailureV32::NotStarted(err)
            }
        })
    }
}
