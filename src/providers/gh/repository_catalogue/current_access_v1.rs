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

pub struct RepositoryCatalogueAccessSession<'a> {
    provider: &'a GithubProvider,
    original: &'a RepositoryCatalogueRead,
    identity: HostIdentity,
    limits: RepositoryCatalogueAccessLimits,
    budget: CatalogueBudget,
    original_indices: std::collections::HashMap<u64, usize>,
    references: BTreeMap<u64, IntentReference>,
    verified: Vec<CurrentCatalogueRepository>,
    failures: Vec<CurrentCatalogueFailure>,
    repository_reads: usize,
    metadata_bytes: usize,
    external_requests_reserved: usize,
    external_capture_reserved_bytes: usize,
    original_cancellation: CancellationToken,
    measured_external: Option<measured_reads_v1::MeasuredSessionPoolV1>,
    measured_parent_reads_remaining: usize,
    postwrite_reservation_v2: Option<postwrite_capture_v2::PostWriteSessionReservationV2>,
    fatal: Option<GithubErrorKind>,
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
        let indices = original
            .repositories()
            .iter()
            .enumerate()
            .map(|(index, repo)| (repo.id, index))
            .collect();
        budget.check(cancel)?;
        Ok(RepositoryCatalogueAccessSession {
            provider: self,
            original,
            identity,
            limits: limits.clone(),
            budget,
            original_indices: indices,
            references: BTreeMap::new(),
            verified: Vec::new(),
            failures: Vec::new(),
            repository_reads: 0,
            metadata_bytes: 0,
            external_requests_reserved: 0,
            external_capture_reserved_bytes: 0,
            original_cancellation: cancel.clone(),
            measured_external: None,
            measured_parent_reads_remaining: 0,
            postwrite_reservation_v2: None,
            fatal: None,
        })
    }
}
impl RepositoryCatalogueAccessSession<'_> {
    pub fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    /// Borrow the already observed row INSIDE this live original session.
    /// Intermediate metadata only: not a closing access receipt or write grant.
    pub(crate) fn observation_v1(&self, id: u64) -> Option<&CurrentCatalogueRepository> {
        self.verified.iter().find(|row| row.original.id == id)
    }
    pub fn deadline(&self) -> Instant {
        self.budget.deadline()
    }
    pub fn remaining(&self, cancel: &CancellationToken) -> GithubResult<Duration> {
        self.budget.check(cancel)?;
        Ok(self.deadline().saturating_duration_since(Instant::now()))
    }
    fn remaining_bytes(&self) -> usize {
        self.budget
            .remaining_bytes()
            .saturating_sub(self.external_capture_reserved_bytes)
            .saturating_sub(self.postwrite_reserved_bytes_v2())
            .saturating_sub(self.measured_held_bytes())
    }
    pub fn reserve_external_reads(
        &mut self,
        requests: usize,
        max_output_bytes: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<ExternalCatalogueReadWindow> {
        self.budget.check(cancel)?;
        if self.measured_external.is_some() || self.postwrite_reservation_v2.is_some() {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Original measured pool cannot mix a legacy reservation/reset",
            ));
        }
        if !(1..=8192).contains(&requests) || !(1..=1024 * 1024).contains(&max_output_bytes) {
            return Err(error(GithubErrorKind::InvalidTarget,"External read reservation requires explicit bounded children and <=1MiB shared capture per child"));
        }
        let bytes = requests.checked_mul(max_output_bytes).ok_or_else(|| {
            error(
                GithubErrorKind::InvalidTarget,
                "External capture reservation overflow",
            )
        })?;
        if bytes > self.remaining_bytes().saturating_sub(FINAL_PRINCIPAL_CAP) {
            return Err(error(GithubErrorKind::Transport,"External conservative capture reservation cannot preserve original final account proof; no legacy child may start"));
        }
        self.external_requests_reserved = self
            .external_requests_reserved
            .checked_add(requests)
            .ok_or_else(|| {
                error(
                    GithubErrorKind::InvalidTarget,
                    "External child count overflow",
                )
            })?;
        self.external_capture_reserved_bytes = self
            .external_capture_reserved_bytes
            .checked_add(bytes)
            .ok_or_else(|| {
                error(
                    GithubErrorKind::InvalidTarget,
                    "External capture ledger overflow",
                )
            })?;
        Ok(ExternalCatalogueReadWindow {
            deadline: self.deadline(),
            requests,
            max_output_bytes,
            reserved_bytes: bytes,
        })
    }
    pub fn verify(
        &mut self,
        reference: &IntentReference,
        cancel: &CancellationToken,
    ) -> GithubResult<&CurrentCatalogueRepository> {
        self.budget.check(cancel)?;
        if let Some(kind) = &self.fatal {
            return Err(error(
                kind.clone(),
                "Original access scope already refused; start an explicit new request",
            ));
        }
        validate_reference(reference)
            .inspect_err(|_| self.fatal = Some(GithubErrorKind::InvalidTarget))?;
        if self.original.owner().is_some_and(|owner| {
            !reference
                .last_name
                .split('/')
                .next()
                .is_some_and(|name| name.eq_ignore_ascii_case(&owner.login))
        }) {
            self.fatal = Some(GithubErrorKind::InvalidTarget);
            return Err(error(GithubErrorKind::InvalidTarget,"Original reference lies outside its genuine declared owner namespace; no locator rebase"));
        }
        if let Some(previous) = self.references.get(&reference.id) {
            if previous != reference {
                self.fatal = Some(GithubErrorKind::IdentityChanged);
                return Err(error(
                    GithubErrorKind::IdentityChanged,
                    "One original ID received conflicting locator/node intents; no rebase",
                ));
            }
        } else {
            if self.references.len() >= self.limits.max_references {
                self.fatal = Some(GithubErrorKind::InvalidTarget);
                return Err(error(
                    GithubErrorKind::InvalidTarget,
                    "Explicit original reference cap reached before another GET",
                ));
            }
            self.references.insert(reference.id, reference.clone());
        }
        if self.repository_reads >= self.limits.max_repository_reads {
            self.fatal = Some(GithubErrorKind::InvalidTarget);
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Original repository-read cap reached before another GET",
            ));
        }
        let cap = self
            .remaining_bytes()
            .saturating_sub(FINAL_PRINCIPAL_CAP)
            .min(self.limits.transport.max_document_bytes);
        if cap == 0 {
            self.fatal = Some(GithubErrorKind::Transport);
            return Err(error(
                GithubErrorKind::Transport,
                "Original whole capture budget reserves closing account proof; no repository GET",
            ));
        }
        self.claim_measured_parent_read(cancel)?;
        self.repository_reads += 1;
        let result = self.observe(reference, cap, cancel);
        match result {
            Ok(observation) => {
                self.failures
                    .retain(|failure| failure.original.id != reference.id);
                let index = if let Some(index) = self
                    .verified
                    .iter()
                    .position(|row| row.original.id == reference.id)
                {
                    self.verified[index] = observation;
                    index
                } else {
                    self.verified.push(observation);
                    self.verified.len() - 1
                };
                Ok(self
                    .observation_v1(self.verified[index].original.id)
                    .expect("actual observed original repository retained"))
            }
            Err(failure) => {
                if matches!(
                    failure.kind,
                    GithubErrorKind::Authentication
                        | GithubErrorKind::Transport
                        | GithubErrorKind::RateLimited
                        | GithubErrorKind::MissingGh
                        | GithubErrorKind::Unsupported
                ) {
                    self.fatal = Some(failure.kind.clone());
                }
                self.verified.retain(|row| row.original.id != reference.id);
                self.failures.retain(|row| row.original.id != reference.id);
                self.failures.push(CurrentCatalogueFailure {
                    original: reference.clone(),
                    kind: failure.kind.clone(),
                });
                Err(safe_failure(failure))
            }
        }
    }
    fn observe(
        &mut self,
        reference: &IntentReference,
        cap: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<CurrentCatalogueRepository> {
        let value = self.budget.json(
            self.provider,
            &format!("repos/{}", reference.last_name),
            cap,
            cancel,
        )?;
        let prior = self
            .original_indices
            .get(&reference.id)
            .map(|index| &self.original.repositories()[*index]);
        let raw_owner = value
            .get("owner")
            .filter(|owner| owner.is_object())
            .ok_or_else(|| {
                error(
                    GithubErrorKind::IdentityChanged,
                    "Current owner namespace is unavailable, not guessed from a repository name",
                )
            })?;
        let login = raw_owner
            .get("login")
            .and_then(Value::as_str)
            .filter(|login| !login.is_empty() && login.len() <= 100)
            .ok_or_else(|| {
                error(
                    GithubErrorKind::IdentityChanged,
                    "Current owner canonical name is unavailable",
                )
            })?;
        let current_owner_source =
            match raw_owner.get("type").and_then(Value::as_str) {
                Some("Organization") => RepositoryCatalogueSource::Organization(login.into()),
                Some("User") => RepositoryCatalogueSource::PublicUser(login.into()),
                _ => return Err(error(
                    GithubErrorKind::IdentityChanged,
                    "Current owner resource kind is unverified; no node/number namespace inferred",
                )),
            };
        let current_owner = parse::owner(raw_owner, &current_owner_source, login)?;
        let current_kind = match current_owner.kind {
            RepositoryCatalogueOwnerKind::Organization => "Organization",
            RepositoryCatalogueOwnerKind::User => "User",
        };
        let previous_owner = if let Some(owner) = self.original.owner() {
            Some((
                owner.id,
                owner.node_id.as_deref(),
                match owner.kind {
                    RepositoryCatalogueOwnerKind::Organization => "Organization",
                    RepositoryCatalogueOwnerKind::User => "User",
                },
            ))
        } else {
            prior.and_then(|row| row.owner.as_ref()).and_then(|actor| {
                actor.database_id.filter(|id| *id > 0).map(|id| {
                    (
                        id,
                        actor.node_id.as_deref(),
                        actor.actor.actor_type.as_str(),
                    )
                })
            })
        };
        if previous_owner.is_some_and(|(id, node, kind)| {
            id != current_owner.id
                || node.is_some_and(|node| current_owner.node_id.as_deref() != Some(node))
                || kind != current_kind
        }) {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Genuine original owner database/node/kind changed; no transfer admission",
            ));
        }
        // Private PARSER context only, after exact genuine original owner
        // identity was checked. This admits actual login/leaf rename without
        // constructing a new Read, source receipt or permission from metadata.
        let context = RepositoryCatalogueContext {
            identity: self.identity.clone(),
            source: self.original.source().clone(),
            owner: self.original.owner().map(|_| current_owner.clone()),
        };
        let (repository, bytes) = parse::repository(
            &value,
            &context,
            self.limits.transport.max_document_bytes,
            self.limits
                .transport
                .max_metadata_bytes
                .saturating_sub(self.metadata_bytes),
        )
        .map_err(|_| {
            error(
                GithubErrorKind::Json,
                "Current repository metadata is invalid/bounded; no legacy default permission",
            )
        })?;
        if repository.id != reference.id
            || reference
                .node_id
                .as_ref()
                .is_some_and(|node| repository.node_id.as_ref() != Some(node))
        {
            return Err(error(GithubErrorKind::IdentityChanged,"Current repository does not match original database/node identity; no name alias or recreation admission"));
        }
        if prior.is_some_and(|row| {
            row.node_id
                .as_ref()
                .is_some_and(|node| repository.node_id.as_ref() != Some(node))
        }) {
            return Err(error(GithubErrorKind::IdentityChanged,"Genuine original observed repository node changed; unverified intent cannot weaken it"));
        }
        let current = repository.owner.as_ref().ok_or_else(|| {
            error(
                GithubErrorKind::IdentityChanged,
                "Current canonical owner identity is unknown; no namespace guessed",
            )
        })?;
        if repository
            .node_id
            .as_ref()
            .is_some_and(|node| current.node_id.as_ref() == Some(node))
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Global opaque node contradicts repository/owner namespace",
            ));
        }
        let name_changed = repository.full_name != reference.last_name;
        if previous_owner.is_none()
            && (!repository
                .full_name
                .eq_ignore_ascii_case(&reference.last_name)
                || prior
                    .is_some_and(|row| !row.full_name.eq_ignore_ascii_case(&repository.full_name)))
        {
            return Err(error(GithubErrorKind::IdentityChanged,"Rename lacks genuine original owner proof; do not infer ownership from a cached name"));
        }
        self.metadata_bytes = self.metadata_bytes.checked_add(bytes).ok_or_else(|| {
            error(
                GithubErrorKind::Transport,
                "Current observation metadata ledger overflow",
            )
        })?;
        self.budget.check(cancel)?;
        Ok(CurrentCatalogueRepository {
            original: reference.clone(),
            repository,
            name_changed,
        })
    }
    pub fn close(self, cancel: &CancellationToken) -> GithubResult<ClosedCatalogueAccess> {
        self.close_with_guard(cancel, || Ok(()))
    }
    /// Original SQL/source/runtime guard runs BEFORE final2, with its elapsed
    /// time charged to SAME clock. After final2 only pure metadata is returned.
    pub fn close_with_guard(
        mut self,
        cancel: &CancellationToken,
        guard: impl FnOnce() -> GithubResult<()>,
    ) -> GithubResult<ClosedCatalogueAccess> {
        let measured_external_accounting = self.seal_measured_external(cancel)?;
        self.budget.check(cancel)?;
        if let Some(kind) = &self.fatal {
            return Err(error(
                kind.clone(),
                "Original access scope refused; no closed private observations",
            ));
        }
        if self.references.is_empty() {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "No explicit original repository selected; never default to all",
            ));
        }
        guard().map_err(safe_failure)?;
        self.budget.check(cancel)?;
        if self.remaining_bytes() < FINAL_PRINCIPAL_CAP {
            return Err(error(
                GithubErrorKind::Transport,
                "Closing account proof capture allowance exhausted; observations hidden",
            ));
        }
        let current = principal(self.provider, &mut self.budget, cancel).map_err(safe_failure)?;
        same_principal(self.original.identity(), &current)?;
        self.budget.check(cancel)?;
        if self
            .budget
            .accounting
            .charged_bytes()
            .saturating_add(
                measured_external_accounting
                    .as_ref()
                    .map_or(self.external_capture_reserved_bytes, |actual| {
                        actual.charged_bytes()
                    }),
            )
            .saturating_add(self.postwrite_reserved_bytes_v2())
            > self.limits.transport.max_response_bytes
        {
            return Err(error(GithubErrorKind::Transport,"Original transport plus conservative external capture budget exceeded; no publication"));
        }
        let postwrite_capture_v2 = self
            .postwrite_reservation_v2
            .as_ref()
            .map(|reservation| reservation.metadata());
        let postwrite_reservation_v2 = self.postwrite_reservation_v2;
        let deadline = postwrite_reservation_v2
            .as_ref()
            .map_or(self.budget.deadline(), |reservation| {
                self.budget.deadline().min(reservation.deadline())
            });
        let original_cancellation = self.original_cancellation;
        let result = ClosedCatalogueAccess {
            identity: current,
            original_source: self.original.source().clone(),
            original_owner: self.original.owner().cloned(),
            verified: self.verified,
            failures: self.failures,
            accounting: self.budget.accounting,
            external_requests_reserved: self.external_requests_reserved,
            external_capture_reserved_bytes: measured_external_accounting
                .as_ref()
                .map_or(self.external_capture_reserved_bytes, |actual| {
                    actual.capacity_bytes()
                }),
            measured_external_accounting,
            postwrite_capture_v2,
        };
        if cancel.is_cancelled()
            || original_cancellation.is_cancelled()
            || Instant::now() >= deadline
        {
            return Err(error(
                GithubErrorKind::Transport,
                "Original source close expired before post-write reservation activation",
            ));
        }
        if let Some(reservation) = &postwrite_reservation_v2 {
            reservation.complete()?;
        }
        Ok(result)
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
