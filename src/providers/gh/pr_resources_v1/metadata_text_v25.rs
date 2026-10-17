//! SAME actual raw-parent factory, purpose-closed original title/description.
//! A selected value is observed data; only fresh Core closure permits its one
//! explicit write. No DTO constructor, generic transport or saved-ID authority.
use super::*;
use crate::providers::runner::ScopedReadBudgetV1;
use sha2::{Digest, Sha256};
use std::{
    sync::{Arc, Weak},
    time::Instant,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrMetadataTextFieldV25 {
    Title,
    Description,
}
impl PrMetadataTextFieldV25 {
    pub fn label(self) -> &'static str {
        match self {
            Self::Title => "Title",
            Self::Description => "Description",
        }
    }
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Description => "description",
        }
    }
    pub(crate) fn command(self) -> &'static str {
        match self {
            Self::Title => "metadata-title-v25",
            Self::Description => "metadata-description-v25",
        }
    }
    pub(crate) fn form_kind(self) -> crate::forms::PrFormKind {
        match self {
            Self::Title => crate::forms::PrFormKind::Title,
            Self::Description => crate::forms::PrFormKind::Description,
        }
    }
    pub(crate) fn validate_requested(self, value: &str) -> GithubResult<()> {
        if value.len() > 65_536 || self == Self::Title && value.trim().is_empty() {
            return Err(metadata_failure(GithubErrorKind::InvalidTarget));
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) const METADATA_TEXT_CAPABILITY_QUERY_V25: &str =
    crate::providers::gh::pr::METADATA_TEXT_CAPABILITY_QUERY_V25;
pub(crate) const METADATA_TEXT_POST_CAPS_V25: [usize; 12] = [
    512 * 1024,
    512 * 1024,
    64 * 1024,
    128 * 1024,
    128 * 1024,
    512 * 1024,
    512 * 1024,
    512 * 1024,
    512 * 1024,
    512 * 1024,
    128 * 1024,
    64 * 1024,
];
pub(crate) const METADATA_TAB_TEXT_POST_CAPS_V51: [usize; 16] = [
    512 * 1024,
    512 * 1024,
    64 * 1024,
    128 * 1024,
    128 * 1024,
    512 * 1024,
    512 * 1024,
    512 * 1024,
    512 * 1024,
    512 * 1024,
    128 * 1024,
    128 * 1024,
    128 * 1024,
    128 * 1024,
    128 * 1024,
    64 * 1024,
];

pub(crate) struct PrMetadataTextReadV25 {
    parent: Arc<PrResourceParentV1>,
    field: PrMetadataTextFieldV25,
    value: Arc<str>,
    explicitly_null: bool,
    runner_generation: Option<u64>,
    phase: Option<Arc<MetadataReadBindingV25>>,
}
struct MetadataReadBindingV25 {
    issuer: ThreadPhaseIssuerV16,
    pool: Weak<ScopedReadBudgetV1>,
    cancellation: CancellationToken,
    deadline: Instant,
}
impl MetadataReadBindingV25 {
    /// Allocation lineage only. Successful original close deliberately aborts
    /// its read pool; this check neither proves closure nor admits another read.
    fn validate_lineage(
        &self,
        provider: &GithubProvider,
        pool: &Arc<ScopedReadBudgetV1>,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        if !self.issuer.matches(provider)
            || !self.pool.ptr_eq(&Arc::downgrade(pool))
            || !self.cancellation.same_instance(cancel)
            || !pool.original_token_matches_v63(cancel)
            || cancel.is_cancelled()
            || Instant::now() >= deadline.min(self.deadline).min(pool.deadline())
        {
            return Err(metadata_failure(GithubErrorKind::IdentityChanged));
        }
        Ok(())
    }
    fn validate(
        &self,
        provider: &GithubProvider,
        pool: &Arc<ScopedReadBudgetV1>,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        self.validate_lineage(provider, pool, cancel, deadline)?;
        pool.check_clock(cancel).map_err(GithubError::from)
    }
}
impl std::fmt::Debug for PrMetadataTextReadV25 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PrMetadataTextReadV25(<actual bounded private field; no permission>)")
    }
}
impl PrMetadataTextReadV25 {
    pub(crate) fn parent(&self) -> &Arc<PrResourceParentV1> {
        &self.parent
    }
    pub(crate) fn field(&self) -> PrMetadataTextFieldV25 {
        self.field
    }
    pub(crate) fn value(&self) -> &str {
        &self.value
    }
    pub(crate) fn explicitly_null(&self) -> bool {
        self.explicitly_null
    }
    pub(crate) fn same_observation(&self, other: &Self) -> bool {
        self.field == other.field
            && self.value == other.value
            && self.explicitly_null == other.explicitly_null
            && self.runner_generation == other.runner_generation
            && match (&self.phase, &other.phase) {
                (Some(a), Some(b)) => {
                    a.pool.ptr_eq(&b.pool)
                        && a.cancellation.same_instance(&b.cancellation)
                        && a.deadline == b.deadline
                }
                (None, None) => true,
                _ => false,
            }
            && GithubProvider::same_thread_history_parent_v1(&self.parent, &other.parent)
    }
    fn from_actual_raw(
        parent: PrResourceParentV1,
        raw: &Value,
        field: PrMetadataTextFieldV25,
        runner_generation: Option<u64>,
        phase: Option<Arc<MetadataReadBindingV25>>,
    ) -> GithubResult<Self> {
        let (value, explicitly_null) = match (
            field,
            raw.get(match field {
                PrMetadataTextFieldV25::Title => "title",
                PrMetadataTextFieldV25::Description => "body",
            }),
        ) {
            (_, Some(Value::String(value))) if value.len() <= 65_536 => (value.as_str(), false),
            (PrMetadataTextFieldV25::Description, Some(Value::Null)) => ("", true),
            _ => return Err(metadata_failure(GithubErrorKind::Json)),
        };
        Ok(Self {
            parent: Arc::new(parent),
            field,
            value: Arc::from(value),
            explicitly_null,
            runner_generation,
            phase,
        })
    }
}

pub(crate) struct OriginalMetadataTextV25 {
    observed: Arc<PrMetadataTextReadV25>,
    identity: HostIdentity,
    can_update: bool,
    measured: Arc<ScopedReadBudgetV1>,
    cancellation: CancellationToken,
    deadline: Instant,
}
impl std::fmt::Debug for OriginalMetadataTextV25 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OriginalMetadataTextV25(<same measured producer/capability; no approval>)")
    }
}
impl OriginalMetadataTextV25 {
    pub(crate) fn publication_cutoff_v51(&self) -> Instant {
        self.deadline.min(self.measured.deadline())
    }
    pub(crate) fn validate_live_source_v25(&self) -> GithubResult<()> {
        if self.cancellation.is_cancelled()
            || Instant::now() >= self.deadline
            || self
                .measured
                .accounting()
                .map_err(GithubError::from)?
                .sealed()
        {
            return Err(metadata_failure(GithubErrorKind::Transport));
        }
        self.measured
            .check_clock(&self.cancellation)
            .map_err(GithubError::from)
    }
    pub(crate) fn observed(&self) -> &Arc<PrMetadataTextReadV25> {
        &self.observed
    }
    pub(crate) fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub(crate) fn can_update(&self) -> bool {
        self.can_update
    }
    pub(crate) fn prepare_requested(&self, requested: &str) -> GithubResult<()> {
        self.observed.field.validate_requested(requested)?;
        if !self.can_update {
            return Err(metadata_failure(GithubErrorKind::Permission));
        }
        if requested == self.observed.value() {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "No change: requested metadata exactly matches current state",
            ));
        }
        Ok(())
    }
}

pub(crate) struct PrMetadataTextReceiptV25 {
    parent: Arc<PrResourceParentV1>,
    field: PrMetadataTextFieldV25,
    observed_digest: [u8; 32],
    original_observation: Weak<PrMetadataTextReadV25>,
    measured: Weak<ScopedReadBudgetV1>,
    cancellation: CancellationToken,
    deadline: Instant,
}
impl PrMetadataTextReceiptV25 {
    /// Restriction-only check used by the fixed paid Metadata context child.
    /// No pool getter or allowance/source constructor is exposed.
    pub(in crate::providers::gh) fn validates_paid_context_v51(
        &self,
        actual: &OriginalMetadataTextV25,
        cancel: &CancellationToken,
        measured: &Weak<ScopedReadBudgetV1>,
    ) -> bool {
        self.validates_refresh(actual, cancel) && self.measured.ptr_eq(measured)
    }
    pub(crate) fn publication_cutoff_v51(&self) -> Instant {
        self.deadline
    }
    pub(crate) fn parent(&self) -> &Arc<PrResourceParentV1> {
        &self.parent
    }
    pub(crate) fn field(&self) -> PrMetadataTextFieldV25 {
        self.field
    }
    pub(crate) fn digest(&self) -> &[u8; 32] {
        &self.observed_digest
    }
    pub(crate) fn validates_refresh(
        &self,
        actual: &OriginalMetadataTextV25,
        cancel: &CancellationToken,
    ) -> bool {
        self.original_observation
            .ptr_eq(&Arc::downgrade(actual.observed()))
            && self.measured.ptr_eq(&Arc::downgrade(&actual.measured))
            && self.cancellation.same_instance(cancel)
            && actual.cancellation.same_instance(cancel)
            && !cancel.is_cancelled()
            && Instant::now() < self.deadline.min(actual.deadline)
            && self.field == actual.observed.field
            && GithubProvider::same_thread_history_parent_v1(
                self.parent(),
                actual.observed.parent(),
            )
    }
    pub(crate) fn matches_value(&self, value: &str) -> bool {
        self.observed_digest == <[u8; 32]>::from(Sha256::digest(value.as_bytes()))
    }
}
pub(crate) enum PrMetadataTextFailureV25 {
    NotStarted(GithubError),
    WriteMayHaveStarted(GithubError),
}
/// Private point-in-time READ observation, not a receipt, permission, cause
/// attribution or a publication source. No deserialize/arbitrary-ID factory.
pub(crate) struct MetadataRecoveryCurrentV25 {
    actual: OriginalMetadataTextV25,
    generation: u64,
}
impl MetadataRecoveryCurrentV25 {
    pub(crate) fn actual(&self) -> &OriginalMetadataTextV25 {
        &self.actual
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.actual.deadline
    }
    pub(crate) fn cancellation(&self) -> &CancellationToken {
        &self.actual.cancellation
    }
    pub(crate) fn matches_target(&self, target: &crate::pr_write_journal::PrWriteTarget) -> bool {
        let p = self.actual.observed.parent();
        crate::storage::Scope::github(&self.actual.identity.host, self.actual.identity.account_id)
            .ok()
            .as_ref()
            == Some(target.scope())
            && target.repository() == p.repository()
            && target.node_id() == p.node_id()
            && target.number() == p.number()
    }
    fn validate(
        &self,
        provider: &GithubProvider,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        provider.validate_original_metadata_generation_v25(&self.actual, cancel, deadline)
    }
}
fn metadata_failure(kind: GithubErrorKind) -> GithubError {
    error(
        kind,
        "Exact original metadata field/source/principal unavailable; no fallback or resend",
    )
}
fn metadata_measured(
    provider: &GithubProvider,
    cancel: &CancellationToken,
) -> GithubResult<(Arc<ScopedReadBudgetV1>, Instant)> {
    let measured = provider
        .scoped_read_budget
        .as_ref()
        .ok_or_else(|| metadata_failure(GithubErrorKind::InvalidTarget))?;
    let deadline = provider
        .scoped_read_window
        .ok_or_else(|| metadata_failure(GithubErrorKind::InvalidTarget))?
        .0
        .min(measured.deadline());
    measured.check_clock(cancel).map_err(GithubError::from)?;
    if cancel.is_cancelled()
        || Instant::now() >= deadline
        || measured.accounting().map_err(GithubError::from)?.sealed()
    {
        return Err(metadata_failure(GithubErrorKind::Transport));
    }
    Ok((Arc::clone(measured), deadline))
}
impl GithubProvider {
    /// Pure existing-runner/lineage check, including the original deadline.
    /// Permitted after successful final UID/close; this is NOT closure proof,
    /// a live-pool check or permission to read from the deliberately aborted pool.
    pub(crate) fn validate_original_metadata_generation_v25(
        &self,
        actual: &OriginalMetadataTextV25,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        if !actual.cancellation.same_instance(cancel) {
            return Err(metadata_failure(GithubErrorKind::IdentityChanged));
        }
        let deadline = deadline
            .min(actual.deadline)
            .min(actual.measured.deadline());
        actual
            .observed
            .phase
            .as_ref()
            .ok_or_else(|| metadata_failure(GithubErrorKind::InvalidTarget))?
            .validate_lineage(self, &actual.measured, cancel, deadline)?;
        let p = actual.observed.parent();
        let generation = self
            .runner
            .review_resource_generation_v11(
                &self.resource(p.repository(), p.number()),
                cancel,
                deadline,
            )
            .map_err(GithubError::from)?;
        if Some(generation) != actual.observed.runner_generation {
            return Err(metadata_failure(GithubErrorKind::StaleHead));
        }
        Ok(())
    }
    pub(crate) fn inspect_original_metadata_v25(
        &self,
        actual: OriginalMetadataTextV25,
        cancel: &CancellationToken,
    ) -> GithubResult<MetadataRecoveryCurrentV25> {
        actual.validate_live_source_v25()?;
        let (pool, _) = metadata_measured(self, cancel)?;
        if !Arc::ptr_eq(&pool, &actual.measured) {
            return Err(metadata_failure(GithubErrorKind::IdentityChanged));
        }
        let generation = actual
            .observed
            .runner_generation
            .ok_or_else(|| metadata_failure(GithubErrorKind::InvalidTarget))?;
        let observed = MetadataRecoveryCurrentV25 { actual, generation };
        observed.validate(self, cancel, observed.deadline())?;
        Ok(observed)
    }
    pub(crate) fn validate_metadata_inspection_v25(
        &self,
        actual: &MetadataRecoveryCurrentV25,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        actual.validate(self, cancel, deadline)
    }
    pub(crate) fn reconcile_metadata_inspection_v25(
        &self,
        actual: MetadataRecoveryCurrentV25,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        actual.validate(self, cancel, deadline)?;
        let p = actual.actual.observed.parent();
        self.runner
            .reconcile_review_generation_v11(
                &self.resource(p.repository(), p.number()),
                actual.generation,
                cancel,
                deadline.min(actual.deadline()),
            )
            .map_err(GithubError::from)
    }
    /// ONE existing measured raw GET/parser; selected body is not taken from
    /// the intentionally stripped parent DTO. Missing is never explicit null.
    pub(crate) fn pr_original_metadata_text_v25(
        &self,
        current: &CurrentCatalogueRepository,
        intent: &PrParentIntentV13,
        field: PrMetadataTextFieldV25,
        cancel: &CancellationToken,
    ) -> GithubResult<Arc<PrMetadataTextReadV25>> {
        let (pool, deadline) = metadata_measured(self, cancel)?;
        let phase = Arc::new(MetadataReadBindingV25 {
            issuer: ThreadPhaseIssuerV16::capture(self)?,
            pool: Arc::downgrade(&pool),
            cancellation: cancel.clone(),
            deadline,
        });
        // Capture BEFORE the first field read, not after a concurrent writer
        // could have finished. Closing comparison retains this same witness.
        let generation = self
            .runner
            .review_resource_generation_v11(
                &self.resource(current.repository().full_name.as_str(), intent.number()),
                cancel,
                deadline,
            )
            .map_err(GithubError::from)?;
        let (parent, raw) = self.original_detail_parent_raw_v19(current, intent, cancel)?;
        Ok(Arc::new(PrMetadataTextReadV25::from_actual_raw(
            parent,
            &raw,
            field,
            Some(generation),
            Some(phase),
        )?))
    }
    /// Exactly one error-free capability query on the SAME measured pool.
    /// Its false value is usable read data, not permission or account loss.
    pub(crate) fn observe_original_metadata_text_v25(
        &self,
        observed: Arc<PrMetadataTextReadV25>,
        identity: &HostIdentity,
        cancel: &CancellationToken,
    ) -> GithubResult<OriginalMetadataTextV25> {
        let (measured, deadline) = metadata_measured(self, cancel)?;
        observed
            .phase
            .as_ref()
            .ok_or_else(|| metadata_failure(GithubErrorKind::InvalidTarget))?
            .validate(self, &measured, cancel, deadline)?;
        let parent = observed.parent();
        if identity.host != self.host
            || parent.summary().pr.host != self.host
            || identity
                .account_id
                .is_none_or(|id| !(1..=i64::MAX as u64).contains(&id))
        {
            return Err(metadata_failure(GithubErrorKind::IdentityChanged));
        }
        let can_update = self.original_metadata_can_update_v25(parent, cancel)?;
        metadata_measured(self, cancel)?;
        Ok(OriginalMetadataTextV25 {
            observed,
            identity: identity.clone(),
            can_update,
            measured,
            cancellation: cancel.clone(),
            deadline,
        })
    }
    /// SAME existing single-field serializer, fixed PATCH and independent GET.
    /// No guards/readbacks outside the original prepaid cursor are hidden.
    #[expect(
        clippy::too_many_arguments,
        reason = "Actual producer/pool, original selected field/parent, fixed cursor and source-close callback remain explicit"
    )]
    pub(crate) fn publish_original_metadata_text_v25(
        &self,
        actual: &GithubProvider,
        captures: &mut PostWriteCaptureAllowanceV2,
        observed: &OriginalMetadataTextV25,
        current: &CurrentCatalogueRepository,
        original: &PrParentIntentV13,
        requested: &str,
        cancel: &CancellationToken,
        before_write: impl FnOnce() -> GithubResult<HostIdentity>,
    ) -> Result<PrMetadataTextReceiptV25, PrMetadataTextFailureV25> {
        let mut invoked = false;
        let result = (|| {
            observed.prepare_requested(requested)?;
            captures.validate_prewrite(actual, cancel)?;
            let (measured, deadline) = metadata_measured(actual, cancel)?;
            if !Arc::ptr_eq(&measured, &observed.measured)
                || !observed.cancellation.same_instance(cancel)
                || deadline > observed.deadline
                || Instant::now() >= observed.deadline
            {
                return Err(metadata_failure(GithubErrorKind::IdentityChanged));
            }
            let identity = before_write()?;
            if identity.host != observed.identity.host
                || identity.account_id != observed.identity.account_id
            {
                return Err(metadata_failure(GithubErrorKind::IdentityChanged));
            }
            if cancel.is_cancelled() || Instant::now() >= deadline.min(captures.deadline()) {
                return Err(metadata_failure(GithubErrorKind::Transport));
            }
            let parent = observed.observed.parent();
            let endpoint = format!("repos/{}/pulls/{}", parent.repository(), parent.number());
            let edit = match observed.observed.field {
                PrMetadataTextFieldV25::Title => PrTextEdit {
                    title: Some(requested.into()),
                    ..Default::default()
                },
                PrMetadataTextFieldV25::Description => PrTextEdit {
                    body: Some(requested.into()),
                    ..Default::default()
                },
            };
            let input =
                serde_json::to_value(edit).map_err(|_| metadata_failure(GithubErrorKind::Json))?;
            invoked = true;
            let response = captures.api(
                self,
                &endpoint,
                "PATCH",
                Some(input),
                Some(self.resource(parent.repository(), parent.number())),
                cancel,
            )?;
            let bind = |raw: &Value| -> GithubResult<PrMetadataTextReadV25> {
                if raw.get("errors").is_some_and(|v| !v.is_null()) || raw.get("message").is_some() {
                    return Err(metadata_failure(GithubErrorKind::AmbiguousWrite));
                }
                let fresh = parent_expected_v13(
                    self,
                    current,
                    ExpectedPrParentV13::PersonalIntent(original),
                    raw,
                )?;
                let fresh = PrMetadataTextReadV25::from_actual_raw(
                    fresh,
                    raw,
                    observed.observed.field,
                    None,
                    None,
                )?;
                if !GithubProvider::same_thread_history_parent_v1(parent, fresh.parent())
                    || fresh.value() != requested
                {
                    return Err(metadata_failure(GithubErrorKind::AmbiguousWrite));
                }
                Ok(fresh)
            };
            bind(&response)?;
            let readback = captures.api(self, &endpoint, "GET", None, None, cancel)?;
            let fresh = bind(&readback)?;
            Ok(PrMetadataTextReceiptV25 {
                parent: Arc::clone(fresh.parent()),
                field: fresh.field(),
                observed_digest: Sha256::digest(fresh.value().as_bytes()).into(),
                original_observation: Arc::downgrade(observed.observed()),
                measured: Arc::downgrade(&observed.measured),
                cancellation: cancel.clone(),
                deadline: observed.deadline.min(captures.deadline()),
            })
        })();
        result.map_err(|failure| {
            if invoked {
                self.runner.mark_ambiguous(&self.resource(
                    observed.observed.parent().repository(),
                    observed.observed.parent().number(),
                ));
                PrMetadataTextFailureV25::WriteMayHaveStarted(failure)
            } else {
                PrMetadataTextFailureV25::NotStarted(failure)
            }
        })
    }
}
