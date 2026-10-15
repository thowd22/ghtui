//! SOURCE-ONLY current READ verification. Original saved names/IDs are intent;
//! only actual gh brackets and exact repo GETs supply current observations.
use super::*;
use crate::repository_catalogue_intent_v1::IntentReference;

#[path = "measured_reads_v1.rs"]
mod measured_reads_v1;
pub use measured_reads_v1::MeasuredExternalReadWindowV1;
#[path = "postwrite_capture_v2.rs"]
mod postwrite_capture_v2;
pub use postwrite_capture_v2::{PostWriteCaptureAccountingV2, PostWriteCaptureAllowanceV2};
pub(crate) use postwrite_capture_v2::{ThreadReactionHttpAckV3, ThreadReactionHttpRequestV3};

#[path = "original_repository_access_core_v13.rs"]
mod original_repository_access_core_v13;
use original_repository_access_core_v13::OriginalRepositoryAccessCoreV13;

const FINAL_PRINCIPAL_CAP: usize = 4096 + 64 * 1024;
const MAX_REFERENCES: usize = 4096;

#[derive(Clone, Debug)]
pub struct RepositoryCatalogueAccessLimits {
    pub transport: RepositoryCatalogueLimits,
    pub max_references: usize,
    pub max_repository_reads: usize,
}
impl Default for RepositoryCatalogueAccessLimits {
    fn default() -> Self {
        Self {
            transport: RepositoryCatalogueLimits::default(),
            max_references: 4096,
            max_repository_reads: 8192,
        }
    }
}
impl RepositoryCatalogueAccessLimits {
    fn validate(&self) -> GithubResult<()> {
        self.transport.validate()?;
        if !(1..=MAX_REFERENCES).contains(&self.max_references)
            || !(1..=8192).contains(&self.max_repository_reads)
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Current access needs bounded explicit reference/read counts",
            ));
        }
        Ok(())
    }
}

/// Bounds only, NOT a permission/authentication receipt. This is a truthful
/// conservative reservation of external legacy children, not observed bytes.
pub struct ExternalCatalogueReadWindow {
    deadline: Instant,
    requests: usize,
    max_output_bytes: usize,
    reserved_bytes: usize,
}
impl std::fmt::Debug for ExternalCatalogueReadWindow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExternalCatalogueReadWindow")
            .field("reserved_requests", &self.requests)
            .field("conservative_capture_bytes", &self.reserved_bytes)
            .finish_non_exhaustive()
    }
}
impl ExternalCatalogueReadWindow {
    pub fn deadline(&self) -> Instant {
        self.deadline
    }
    pub fn requests(&self) -> usize {
        self.requests
    }
    pub fn max_output_bytes(&self) -> usize {
        self.max_output_bytes
    }
    pub fn reserved_bytes(&self) -> usize {
        self.reserved_bytes
    }
}

/// Ephemeral observation. Do not publish before the owning session CLOSES or
/// reinterpret a nullable flag as denial/grant. No serde/Clone constructor.
pub struct CurrentCatalogueRepository {
    original: IntentReference,
    repository: RepositoryCatalogueRepository,
    name_changed: bool,
}
impl std::fmt::Debug for CurrentCatalogueRepository {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CurrentCatalogueRepository(<original ID-bound observed read>)")
    }
}
impl CurrentCatalogueRepository {
    pub fn original(&self) -> &IntentReference {
        &self.original
    }
    pub fn repository(&self) -> &RepositoryCatalogueRepository {
        &self.repository
    }
    pub fn name_changed(&self) -> bool {
        self.name_changed
    }
}
pub struct CurrentCatalogueFailure {
    original: IntentReference,
    kind: GithubErrorKind,
}
impl std::fmt::Debug for CurrentCatalogueFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CurrentCatalogueFailure")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}
impl CurrentCatalogueFailure {
    pub fn original(&self) -> &IntentReference {
        &self.original
    }
    pub fn kind(&self) -> &GithubErrorKind {
        &self.kind
    }
}
pub struct ClosedCatalogueAccess {
    identity: HostIdentity,
    original_source: RepositoryCatalogueSource,
    original_owner: Option<RepositoryCatalogueOwner>,
    verified: Vec<CurrentCatalogueRepository>,
    failures: Vec<CurrentCatalogueFailure>,
    accounting: RepositoryCatalogueAccounting,
    external_requests_reserved: usize,
    external_capture_reserved_bytes: usize,
    measured_external_accounting: Option<crate::providers::runner::ScopedReadAccountingV1>,
    postwrite_capture_v2: Option<PostWriteCaptureAccountingV2>,
}
impl std::fmt::Debug for ClosedCatalogueAccess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ClosedCatalogueAccess(<closed read observation; not mutation authority>)")
    }
}
impl ClosedCatalogueAccess {
    pub fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    /// Original factory metadata, NOT a newly invented source/read receipt.
    pub fn original_source(&self) -> &RepositoryCatalogueSource {
        &self.original_source
    }
    pub fn original_owner(&self) -> Option<&RepositoryCatalogueOwner> {
        self.original_owner.as_ref()
    }
    pub fn verified(&self) -> &[CurrentCatalogueRepository] {
        &self.verified
    }
    pub fn observation(&self, id: u64) -> Option<&CurrentCatalogueRepository> {
        self.verified.iter().find(|row| row.original.id == id)
    }
    pub fn failures(&self) -> &[CurrentCatalogueFailure] {
        &self.failures
    }
    pub fn accounting(&self) -> &RepositoryCatalogueAccounting {
        &self.accounting
    }
    pub fn external_requests_reserved(&self) -> usize {
        self.external_requests_reserved
    }
    pub fn external_capture_reserved_bytes(&self) -> usize {
        self.external_capture_reserved_bytes
    }
    pub fn charged_bytes(&self) -> usize {
        self.accounting
            .charged_bytes()
            .saturating_add(
                self.measured_external_accounting
                    .as_ref()
                    .map_or(self.external_capture_reserved_bytes, |actual| {
                        actual.charged_bytes()
                    }),
            )
            .saturating_add(
                self.postwrite_capture_v2
                    .as_ref()
                    .map_or(0, PostWriteCaptureAccountingV2::reserved_bytes),
            )
    }
    /// Conservative reserved post-close captures, NOT observed bytes/children
    /// or permission. Measured original prewrite observations stay separate.
    pub fn postwrite_capture_v2(&self) -> Option<&PostWriteCaptureAccountingV2> {
        self.postwrite_capture_v2.as_ref()
    }
    pub fn external_read_accounting(
        &self,
    ) -> Option<&crate::providers::runner::ScopedReadAccountingV1> {
        self.measured_external_accounting.as_ref()
    }
    pub fn source_limitations(&self) -> &'static [&'static str] {
        &[
            "Non-atomic account/repository observations; permission may change after read",
            "Original saved references remain navigation intent, never mutation authority",
            "External bytes are measured only when an actual runner report is present; otherwise conservative reservation",
        ]
    }
}

/// Public catalogue ABI and provenance remain unchanged. The one original
/// budget, pool and closure live in the private shared implementation.
pub struct RepositoryCatalogueAccessSession<'a> {
    original: &'a RepositoryCatalogueRead,
    core: OriginalRepositoryAccessCoreV13<'a>,
}
impl std::fmt::Debug for RepositoryCatalogueAccessSession<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RepositoryCatalogueAccessSession(<pending original numeric scope>)")
    }
}
impl GithubProvider {
    pub fn begin_repository_catalogue_access<'a>(
        &'a self,
        original: &'a RepositoryCatalogueRead,
        limits: &RepositoryCatalogueAccessLimits,
        cancel: &CancellationToken,
    ) -> GithubResult<RepositoryCatalogueAccessSession<'a>> {
        limits.validate()?;
        validate_original(&self.host, original.identity())?;
        if original.repositories().len() > 100000 {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Original catalogue index exceeds its hard bounded row count",
            ));
        }
        let mut budget = CatalogueBudget::new(&limits.transport)?;
        // The original operation's cancellation also governs closing SQL and
        // principal reads, even if a caller later supplies a fresh token.
        budget.original_cancellation = Some(cancel.clone());
        let identity = principal(self, &mut budget, cancel).map_err(safe_failure)?;
        same_principal(original.identity(), &identity)?;
        let core = OriginalRepositoryAccessCoreV13::from_started_catalogue_v13(
            self, original, identity, limits, budget, cancel,
        )?;
        Ok(RepositoryCatalogueAccessSession { original, core })
    }
}
impl RepositoryCatalogueAccessSession<'_> {
    /// Borrow an already performed actual GET; no query, authority or renewed
    /// clock. Used after budget reservation without duplicating that read.
    pub(crate) fn observed_original_v1(&self, id: u64) -> Option<&CurrentCatalogueRepository> {
        self.core
            .verified
            .iter()
            .rev()
            .find(|row| row.original.id == id)
    }
    pub fn identity(&self) -> &HostIdentity {
        self.core.identity()
    }
    /// Intermediate actual row only, never closing/source/write authority.
    pub(crate) fn observation_v1(&self, id: u64) -> Option<&CurrentCatalogueRepository> {
        self.core.observation_v1(id)
    }
    pub fn deadline(&self) -> Instant {
        self.core.deadline()
    }
    pub fn remaining(&self, cancel: &CancellationToken) -> GithubResult<Duration> {
        self.core.remaining(cancel)
    }
    pub fn reserve_external_reads(
        &mut self,
        requests: usize,
        max_output_bytes: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<ExternalCatalogueReadWindow> {
        self.core
            .reserve_external_reads(requests, max_output_bytes, cancel)
    }
    pub fn verify(
        &mut self,
        reference: &IntentReference,
        cancel: &CancellationToken,
    ) -> GithubResult<&CurrentCatalogueRepository> {
        self.core.verify(reference, cancel)
    }
    pub fn close(self, cancel: &CancellationToken) -> GithubResult<ClosedCatalogueAccess> {
        self.close_with_guard(cancel, || Ok(()))
    }
    /// Same original guard/final principal and accounting; wrapper adds ONLY
    /// genuine catalogue provenance, never invented personal inventory labels.
    pub fn close_with_guard(
        self,
        cancel: &CancellationToken,
        guard: impl FnOnce() -> GithubResult<()>,
    ) -> GithubResult<ClosedCatalogueAccess> {
        // Bounded provenance copies precede core's final original clock check.
        let original_source = self.original.source().clone();
        let original_owner = self.original.owner().cloned();
        let closed = self.core.close_with_guard(cancel, guard)?;
        Ok(ClosedCatalogueAccess {
            identity: closed.identity,
            original_source,
            original_owner,
            verified: closed.verified,
            failures: closed.failures,
            accounting: closed.accounting,
            external_requests_reserved: closed.external_requests_reserved,
            external_capture_reserved_bytes: closed.external_capture_reserved_bytes,
            measured_external_accounting: closed.measured_external_accounting,
            postwrite_capture_v2: closed.postwrite_capture_v2,
        })
    }
}

fn validate_reference(reference: &IntentReference) -> GithubResult<()> {
    if reference.id == 0
        || reference.last_name.len() > 256
        || reference.node_id.as_ref().is_some_and(|node| {
            node.is_empty() || node.len() > 1024 || node.chars().any(char::is_control)
        })
    {
        return Err(error(
            GithubErrorKind::InvalidTarget,
            "Explicit repository intent identity is invalid/bounded",
        ));
    }
    validate_repo(&reference.last_name)
}
fn same_principal(original: &HostIdentity, current: &HostIdentity) -> GithubResult<()> {
    if original.host != current.host
        || original.account_id.is_none_or(|uid| uid == 0)
        || original.account_id != current.account_id
    {
        return Err(error(
            GithubErrorKind::IdentityChanged,
            "Original numeric host/account changed; pending private observations remain hidden",
        ));
    }
    Ok(())
}
fn safe_failure(failure: GithubError) -> GithubError {
    error(failure.kind,"Original repository read/access boundary unavailable; no private diagnostic or default scope admitted")
}
