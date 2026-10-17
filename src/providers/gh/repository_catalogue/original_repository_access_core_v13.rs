//! Shared ORIGINAL read-session machinery. Personal membership comes ONLY from
//! its actual single-repository producer, never a fabricated catalogue Read.
//! Budget, measured pool and prepaid lineage are moved, never reset.
use super::*;

enum OriginalRepositoryMembershipV13<'a> {
    Catalogue(&'a RepositoryCatalogueRead),
    Personal(Arc<PersonalRepositoryOriginalV13>),
    Notifications(Arc<notifications_v63::NotificationRepositoryOriginalV63>),
}
impl OriginalRepositoryMembershipV13<'_> {
    fn identity(&self) -> &HostIdentity {
        match self {
            Self::Catalogue(original) => original.identity(),
            Self::Personal(original) => &original.identity,
            Self::Notifications(original) => original.identity(),
        }
    }
    fn owner(&self) -> Option<&RepositoryCatalogueOwner> {
        match self {
            Self::Catalogue(original) => original.owner(),
            Self::Personal(original) => Some(&original.owner),
            Self::Notifications(_) => None,
        }
    }
    fn repository_at(&self, index: usize) -> OriginalRepositoryIdentityV63<'_> {
        match self {
            Self::Catalogue(original) => {
                OriginalRepositoryIdentityV63::Full(&original.repositories()[index])
            }
            Self::Personal(original) => {
                assert_eq!(
                    index, 0,
                    "closed personal membership has exactly one actual repository"
                );
                OriginalRepositoryIdentityV63::Full(original.current.repository())
            }
            Self::Notifications(original) => {
                OriginalRepositoryIdentityV63::Notification(&original.repositories()[index])
            }
        }
    }
    fn parser_context_v13<'a>(
        &self,
        current_owner: &'a RepositoryCatalogueOwner,
    ) -> parse::RepositoryParseContextV13<'a> {
        match self {
            Self::Catalogue(original) => parse::RepositoryParseContextV13::current_catalogue(
                original.context(),
                current_owner,
            ),
            Self::Personal(_) | Self::Notifications(_) => {
                parse::RepositoryParseContextV13::personal_observed_owner_v13(current_owner)
            }
        }
    }
}
/// Genuine borrowed identity ONLY. Notification membership lacks the fields
/// needed for a full repository DTO; never synthesize those absent values.
#[derive(Clone, Copy)]
enum OriginalRepositoryIdentityV63<'a> {
    Full(&'a RepositoryCatalogueRepository),
    Notification(&'a notifications_v63::NotificationRepositoryV63),
}
impl<'a> OriginalRepositoryIdentityV63<'a> {
    fn node(self) -> Option<&'a str> {
        match self {
            Self::Full(r) => r.node_id.as_deref(),
            Self::Notification(r) => Some(r.node()),
        }
    }
    fn name(self) -> &'a str {
        match self {
            Self::Full(r) => &r.full_name,
            Self::Notification(r) => r.name(),
        }
    }
    fn owner(self) -> Option<(u64, Option<&'a str>, &'a str)> {
        match self {
            Self::Full(r) => r.owner.as_ref().and_then(|a| {
                a.database_id
                    .filter(|n| *n > 0)
                    .map(|n| (n, a.node_id.as_deref(), a.actor.actor_type.as_str()))
            }),
            Self::Notification(r) => r.owner().map(|o| {
                (
                    o.id,
                    o.node_id.as_deref(),
                    match o.kind {
                        RepositoryCatalogueOwnerKind::User => "User",
                        RepositoryCatalogueOwnerKind::Organization => "Organization",
                    },
                )
            }),
        }
    }
}

pub(super) struct OriginalRepositoryAccessCoreV13<'a> {
    pub(super) provider: &'a GithubProvider,
    original: OriginalRepositoryMembershipV13<'a>,
    pub(super) identity: HostIdentity,
    pub(super) limits: RepositoryCatalogueAccessLimits,
    pub(super) budget: CatalogueBudget,
    pub(super) original_indices: std::collections::HashMap<u64, usize>,
    pub(super) references: BTreeMap<u64, IntentReference>,
    pub(super) verified: Vec<CurrentCatalogueRepository>,
    pub(super) failures: Vec<CurrentCatalogueFailure>,
    pub(super) repository_reads: usize,
    pub(super) metadata_bytes: usize,
    pub(super) external_requests_reserved: usize,
    pub(super) external_capture_reserved_bytes: usize,
    pub(super) original_cancellation: CancellationToken,
    pub(super) measured_external: Option<measured_reads_v1::MeasuredSessionPoolV1>,
    pub(super) measured_parent_reads_remaining: usize,
    pub(super) postwrite_reservation_v2:
        Option<postwrite_capture_v2::PostWriteSessionReservationV2>,
    pub(super) fatal: Option<GithubErrorKind>,
    notification_reads_v63: BTreeMap<u64, usize>,
}
pub(super) struct ClosedOriginalRepositoryAccessV13 {
    pub(super) identity: HostIdentity,
    pub(super) verified: Vec<CurrentCatalogueRepository>,
    pub(super) failures: Vec<CurrentCatalogueFailure>,
    pub(super) accounting: RepositoryCatalogueAccounting,
    pub(super) external_requests_reserved: usize,
    pub(super) external_capture_reserved_bytes: usize,
    pub(super) measured_external_accounting:
        Option<crate::providers::runner::ScopedReadAccountingV1>,
    pub(super) postwrite_capture_v2: Option<PostWriteCaptureAccountingV2>,
}
impl<'a> OriginalRepositoryAccessCoreV13<'a> {
    pub(super) fn from_started_notifications_v63(
        provider: &'a GithubProvider,
        original: Arc<notifications_v63::NotificationRepositoryOriginalV63>,
        identity: HostIdentity,
        limits: &RepositoryCatalogueAccessLimits,
        budget: CatalogueBudget,
        cancel: &CancellationToken,
    ) -> GithubResult<Self> {
        budget.check(cancel)?;
        if !original.matches_issuer(provider)
            || original.identity().host != identity.host
            || original.identity().account_id != identity.account_id
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Notification original issuer/numeric principal changed; no new budget/source",
            ));
        }
        let original_indices = original
            .repositories()
            .iter()
            .enumerate()
            .map(|(i, r)| (r.id(), i))
            .collect();
        Ok(Self {
            provider,
            original: OriginalRepositoryMembershipV13::Notifications(original),
            identity,
            limits: limits.clone(),
            budget,
            original_indices,
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
            notification_reads_v63: BTreeMap::new(),
        })
    }
    pub(super) fn from_started_personal_v13(
        provider: &'a GithubProvider,
        original: Arc<PersonalRepositoryOriginalV13>,
        identity: HostIdentity,
        limits: &RepositoryCatalogueAccessLimits,
        budget: CatalogueBudget,
        cancel: &CancellationToken,
        initial: Option<usize>,
    ) -> GithubResult<Self> {
        budget.check(cancel)?;
        let id = original.current.original().id;
        let references = if initial.is_some() {
            BTreeMap::from([(id, original.current.original().clone())])
        } else {
            BTreeMap::new()
        };
        Ok(Self {
            provider,
            original: OriginalRepositoryMembershipV13::Personal(original),
            identity,
            limits: limits.clone(),
            budget,
            original_indices: std::collections::HashMap::from([(id, 0)]),
            references,
            verified: Vec::new(),
            failures: Vec::new(),
            repository_reads: usize::from(initial.is_some()),
            metadata_bytes: initial.unwrap_or(0),
            external_requests_reserved: 0,
            external_capture_reserved_bytes: 0,
            original_cancellation: cancel.clone(),
            measured_external: None,
            measured_parent_reads_remaining: 0,
            postwrite_reservation_v2: None,
            fatal: None,
            notification_reads_v63: BTreeMap::new(),
        })
    }
    /// Takes ownership of the ALREADY-started original budget. No second user
    /// read, new clock, reset capture pool or synthetic membership is created.
    pub(super) fn from_started_catalogue_v13(
        provider: &'a GithubProvider,
        original: &'a RepositoryCatalogueRead,
        identity: HostIdentity,
        limits: &RepositoryCatalogueAccessLimits,
        budget: CatalogueBudget,
        cancel: &CancellationToken,
    ) -> GithubResult<Self> {
        let indices = original
            .repositories()
            .iter()
            .enumerate()
            .map(|(index, repo)| (repo.id, index))
            .collect();
        budget.check(cancel)?;
        Ok(Self {
            provider,
            original: OriginalRepositoryMembershipV13::Catalogue(original),
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
            notification_reads_v63: BTreeMap::new(),
        })
    }
}
impl OriginalRepositoryAccessCoreV13<'_> {
    pub(super) fn validate_personal_reference_v13(
        &self,
        reference: &IntentReference,
    ) -> GithubResult<()> {
        if let OriginalRepositoryMembershipV13::Personal(original) = &self.original {
            if original.reference() != reference {
                return Err(error(
                    GithubErrorKind::InvalidTarget,
                    "Personal original phase cannot select another repository/reference",
                ));
            }
        }
        Ok(())
    }
    pub(super) fn personal_closing_ready_v13(&self) -> bool {
        !matches!(&self.original, OriginalRepositoryMembershipV13::Personal(_))
            || (self.repository_reads >= 2
                && self.verified.len() == 1
                && self.failures.is_empty()
                && self.verified[0].repository().permissions.pull == Some(true))
    }
    pub(super) fn clamp_personal_capture_v13(&self, cap: usize) -> usize {
        match &self.original {
            OriginalRepositoryMembershipV13::Catalogue(_) => cap,
            OriginalRepositoryMembershipV13::Personal(_)
            | OriginalRepositoryMembershipV13::Notifications(_) => self
                .provider
                .scoped_read_window
                .map_or(cap, |(_, original_cap)| cap.min(original_cap)),
        }
    }
    pub(super) fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    /// Borrow the already observed row INSIDE this live original session.
    /// Intermediate metadata only: not a closing access receipt or write grant.
    pub(super) fn observation_v1(&self, id: u64) -> Option<&CurrentCatalogueRepository> {
        self.verified.iter().find(|row| row.original.id == id)
    }
    pub(super) fn deadline(&self) -> Instant {
        match &self.original {
            OriginalRepositoryMembershipV13::Catalogue(_) => self.budget.deadline(),
            OriginalRepositoryMembershipV13::Personal(_)
            | OriginalRepositoryMembershipV13::Notifications(_) => self
                .provider
                .scoped_read_window
                .map_or(self.budget.deadline(), |(deadline, _)| {
                    self.budget.deadline().min(deadline)
                }),
        }
    }
    pub(super) fn remaining(&self, cancel: &CancellationToken) -> GithubResult<Duration> {
        self.budget.check(cancel)?;
        Ok(self.deadline().saturating_duration_since(Instant::now()))
    }
    pub(super) fn remaining_bytes(&self) -> usize {
        self.budget
            .remaining_bytes()
            .saturating_sub(self.external_capture_reserved_bytes)
            .saturating_sub(self.postwrite_reserved_bytes_v2())
            .saturating_sub(self.measured_held_bytes())
    }
    pub(super) fn reserve_external_reads(
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
    pub(super) fn verify(
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
        if let OriginalRepositoryMembershipV13::Notifications(original) = &self.original {
            let valid = self
                .original_indices
                .get(&reference.id)
                .and_then(|i| original.reference(*i))
                .is_some_and(|actual| actual == *reference);
            if !valid
                || self
                    .notification_reads_v63
                    .get(&reference.id)
                    .copied()
                    .unwrap_or(0)
                    >= 2
            {
                return Err(error(GithubErrorKind::InvalidTarget,"Notification phase requires exact actual member and at most opening/closing repository reads"));
            }
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
                if matches!(
                    &self.original,
                    OriginalRepositoryMembershipV13::Notifications(_)
                ) {
                    *self.notification_reads_v63.entry(reference.id).or_default() += 1;
                }
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
                        | GithubErrorKind::OutputLimit
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
        let value = self
            .budget
            .original_repository_v9(self.provider, reference, cap, cancel)?;
        let prior = self
            .original_indices
            .get(&reference.id)
            .map(|index| self.original.repository_at(*index));
        let current_owner = parse::repository_owner_v13(&value)?;
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
            prior.and_then(OriginalRepositoryIdentityV63::owner)
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
        let context = self.original.parser_context_v13(&current_owner);
        let (repository, bytes) = parse::repository_with_context_v13(
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
            row.node()
                .is_some_and(|node| repository.node_id.as_deref() != Some(node))
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
                || prior.is_some_and(|row| !row.name().eq_ignore_ascii_case(&repository.full_name)))
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
    /// Original SQL/source/runtime guard runs BEFORE final2, with its elapsed
    /// time charged to SAME clock. After final2 only pure metadata is returned.
    pub(super) fn close_with_guard(
        mut self,
        cancel: &CancellationToken,
        guard: impl FnOnce() -> GithubResult<()>,
    ) -> GithubResult<ClosedOriginalRepositoryAccessV13> {
        let measured_external_accounting = self.seal_measured_external(cancel)?;
        self.budget.check(cancel)?;
        if let Some(kind) = &self.fatal {
            return Err(error(
                kind.clone(),
                "Original access scope refused; no closed private observations",
            ));
        }
        let notification_empty = matches!(&self.original,OriginalRepositoryMembershipV13::Notifications(original) if original.complete_empty());
        if let OriginalRepositoryMembershipV13::Notifications(original) = &self.original {
            if !original.complete_empty()
                && (!self.failures.is_empty()
                    || self
                        .verified
                        .iter()
                        .any(|r| r.repository().permissions.pull != Some(true))
                    || self.verified.len() != original.repositories().len()
                    || original
                        .repositories()
                        .iter()
                        .any(|r| self.notification_reads_v63.get(&r.id()) != Some(&2)))
            {
                return Err(error(
                    GithubErrorKind::IdentityChanged,
                    "Notification private rows lack actual opening/closing repository observations",
                ));
            }
        }
        if self.references.is_empty() && !notification_empty {
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
        let original_deadline = self.deadline();
        let postwrite_reservation_v2 = self.postwrite_reservation_v2;
        let deadline = postwrite_reservation_v2
            .as_ref()
            .map_or(original_deadline, |reservation| {
                original_deadline.min(reservation.deadline())
            });
        let original_cancellation = self.original_cancellation;
        let result = ClosedOriginalRepositoryAccessV13 {
            identity: current,
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
