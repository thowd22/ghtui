//! Genuine single-repository personal PR reads on the SAME original core.
//! No catalogue inventory, saved-set provenance, auth/client or queue is made.
use super::*;

/// Created ONLY after the initial actual user and exact repository GET. This
/// original allocation is read provenance, not current write permission.
pub struct PersonalRepositoryOriginalV13 {
    pub(super) identity: HostIdentity,
    pub(super) owner: RepositoryCatalogueOwner,
    pub(super) current: CurrentCatalogueRepository,
}
impl std::fmt::Debug for PersonalRepositoryOriginalV13 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "PersonalRepositoryOriginalV13(<actual single-repository original; no inventory>)",
        )
    }
}
impl PersonalRepositoryOriginalV13 {
    pub fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub fn reference(&self) -> &IntentReference {
        self.current.original()
    }
    pub fn repository(&self) -> &RepositoryCatalogueRepository {
        self.current.repository()
    }
}

/// One live original phase. Drop closes the SAME measured/prepaid lineage.
pub struct PersonalPrAccessSessionV13<'a> {
    original: Arc<PersonalRepositoryOriginalV13>,
    intent: Arc<PrParentIntentV13>,
    core: OriginalRepositoryAccessCoreV13<'a>,
}
impl std::fmt::Debug for PersonalPrAccessSessionV13<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PersonalPrAccessSessionV13(<pending actual original phase>)")
    }
}
pub struct ClosedPersonalPrAccessV13 {
    original: Arc<PersonalRepositoryOriginalV13>,
    closed: original_repository_access_core_v13::ClosedOriginalRepositoryAccessV13,
}
impl std::fmt::Debug for ClosedPersonalPrAccessV13 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "ClosedPersonalPrAccessV13(<closed single-repository read; not write authority>)",
        )
    }
}
impl ClosedPersonalPrAccessV13 {
    pub fn original(&self) -> &Arc<PersonalRepositoryOriginalV13> {
        &self.original
    }
    pub fn identity(&self) -> &HostIdentity {
        &self.closed.identity
    }
    pub fn observation(&self) -> &CurrentCatalogueRepository {
        &self.closed.verified[0]
    }
    pub fn accounting(&self) -> &RepositoryCatalogueAccounting {
        &self.closed.accounting
    }
    pub fn external_read_accounting(
        &self,
    ) -> Option<&crate::providers::runner::ScopedReadAccountingV1> {
        self.closed.measured_external_accounting.as_ref()
    }
    pub fn postwrite_capture_v2(&self) -> Option<&PostWriteCaptureAccountingV2> {
        self.closed.postwrite_capture_v2.as_ref()
    }
    pub fn charged_bytes(&self) -> usize {
        self.closed
            .accounting
            .charged_bytes()
            .saturating_add(
                self.closed
                    .measured_external_accounting
                    .as_ref()
                    .map_or(self.closed.external_capture_reserved_bytes, |a| {
                        a.charged_bytes()
                    }),
            )
            .saturating_add(
                self.closed
                    .postwrite_capture_v2
                    .as_ref()
                    .map_or(0, PostWriteCaptureAccountingV2::reserved_bytes),
            )
    }
}
fn check_original_v13(
    provider: &GithubProvider,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<()> {
    if cancel.is_cancelled()
        || Instant::now() >= deadline
        || provider
            .scoped_read_window
            .is_some_and(|(bound, _)| Instant::now() >= bound)
    {
        return Err(error(
            GithubErrorKind::Transport,
            "Original personal read expired/cancelled; no renewed phase or result",
        ));
    }
    Ok(())
}
fn start_budget_v13(
    provider: &GithubProvider,
    limits: &RepositoryCatalogueAccessLimits,
    cancel: &CancellationToken,
) -> GithubResult<CatalogueBudget> {
    limits.validate()?;
    if provider.scoped_read_budget.is_some() || limits.max_repository_reads < 2 {
        return Err(error(GithubErrorKind::InvalidTarget, "Personal phase needs its original unallocated pool and two bounded repository observations"));
    }
    let mut budget = CatalogueBudget::new(&limits.transport)?;
    budget.original_cancellation = Some(cancel.clone());
    check_original_v13(provider, cancel, budget.deadline())?;
    Ok(budget)
}
impl GithubProvider {
    /// Foreground-created intent is bounded cached metadata ONLY. Its issue ID
    /// is not a PR database ID; actual identities are read here in the worker.
    pub fn begin_personal_pr_access_v13<'a>(
        &'a self,
        intent: &Arc<PrParentIntentV13>,
        limits: &RepositoryCatalogueAccessLimits,
        cancel: &CancellationToken,
    ) -> GithubResult<PersonalPrAccessSessionV13<'a>> {
        validate_original(&self.host, intent.identity())?;
        let mut budget = start_budget_v13(self, limits, cancel)?;
        let identity = principal(self, &mut budget, cancel).map_err(safe_failure)?;
        same_principal(intent.identity(), &identity)?;
        let cap = budget
            .remaining_bytes()
            .saturating_sub(FINAL_PRINCIPAL_CAP)
            .min(limits.transport.max_document_bytes);
        if cap == 0 {
            return Err(error(
                GithubErrorKind::Transport,
                "Initial personal repository cannot consume final principal capacity",
            ));
        }
        let value = budget.json(self, &format!("repos/{}", intent.repository()), cap, cancel)?;
        let owner = parse::repository_owner_v13(&value)?;
        let (repository, metadata_bytes) = parse::repository_with_context_v13(
            &value,
            &parse::RepositoryParseContextV13::personal_observed_owner_v13(&owner),
            limits.transport.max_document_bytes,
            limits.transport.max_metadata_bytes,
        )
        .map_err(|_| {
            error(
                GithubErrorKind::Json,
                "Actual personal repository metadata exceeds its bound or is invalid",
            )
        })?;
        if repository.id > i64::MAX as u64
            || owner.id > i64::MAX as u64
            || !repository
                .full_name
                .eq_ignore_ascii_case(intent.repository())
            || repository
                .node_id
                .as_deref()
                .is_none_or(|node| node.is_empty())
            || owner.node_id.as_deref().is_none_or(|node| node.is_empty())
            || owner.node_id == repository.node_id
            || repository.permissions.pull != Some(true)
        {
            return Err(error(GithubErrorKind::IdentityChanged, "Actual personal repository/owner identity or read permission unavailable; no name fallback"));
        }
        let reference = IntentReference {
            id: repository.id,
            node_id: repository.node_id.clone(),
            last_name: repository.full_name.clone(),
        };
        let original = Arc::new(PersonalRepositoryOriginalV13 {
            identity: identity.clone(),
            owner,
            current: CurrentCatalogueRepository {
                original: reference,
                repository,
                name_changed: false,
            },
        });
        let core = OriginalRepositoryAccessCoreV13::from_started_personal_v13(
            self,
            original.clone(),
            identity,
            limits,
            budget,
            cancel,
            Some(metadata_bytes),
        )?;
        check_original_v13(self, cancel, core.deadline())?;
        Ok(PersonalPrAccessSessionV13 {
            original,
            intent: intent.clone(),
            core,
        })
    }
    /// Fresh phase on the EXACT original single repository allocation. It is
    /// never reconstructed from a closed catalogue or serialized DTO.
    pub fn begin_personal_pr_source_access_v13<'a>(
        &'a self,
        original: &Arc<PersonalRepositoryOriginalV13>,
        intent: &Arc<PrParentIntentV13>,
        limits: &RepositoryCatalogueAccessLimits,
        cancel: &CancellationToken,
    ) -> GithubResult<PersonalPrAccessSessionV13<'a>> {
        validate_original(&self.host, original.identity())?;
        same_principal(original.identity(), intent.identity())?;
        if !original
            .reference()
            .last_name
            .eq_ignore_ascii_case(intent.repository())
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Personal source cannot select a different original repository",
            ));
        }
        let mut budget = start_budget_v13(self, limits, cancel)?;
        let identity = principal(self, &mut budget, cancel).map_err(safe_failure)?;
        same_principal(original.identity(), &identity)?;
        let mut core = OriginalRepositoryAccessCoreV13::from_started_personal_v13(
            self,
            original.clone(),
            identity,
            limits,
            budget,
            cancel,
            None,
        )?;
        core.verify(original.reference(), cancel)?;
        check_original_v13(self, cancel, core.deadline())?;
        Ok(PersonalPrAccessSessionV13 {
            original: original.clone(),
            intent: intent.clone(),
            core,
        })
    }
}
impl PersonalPrAccessSessionV13<'_> {
    pub fn original(&self) -> &Arc<PersonalRepositoryOriginalV13> {
        &self.original
    }
    pub fn identity(&self) -> &HostIdentity {
        self.core.identity()
    }
    pub fn deadline(&self) -> Instant {
        self.core.deadline()
    }
    /// Live intermediate observation, never a current permission receipt.
    pub fn observation(&self) -> &CurrentCatalogueRepository {
        self.core
            .observation_v1(self.original.reference().id)
            .unwrap_or(&self.original.current)
    }
    pub fn remaining(&self, cancel: &CancellationToken) -> GithubResult<Duration> {
        check_original_v13(self.core.provider, cancel, self.deadline())?;
        self.core.remaining(cancel)
    }
    pub fn reserve_measured_external_reads(
        &mut self,
        requests: usize,
        child_bytes: usize,
        pool_bytes: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<MeasuredExternalReadWindowV1> {
        self.core
            .reserve_measured_external_reads(requests, child_bytes, pool_bytes, 1, cancel)
    }
    pub fn reserve_postwrite_capture_v2(
        &mut self,
        caps: &[usize],
        cancel: &CancellationToken,
    ) -> GithubResult<PostWriteCaptureAllowanceV2> {
        self.core.reserve_postwrite_capture_v2(caps, 1, cancel)
    }
    /// SAME existing raw parent/Files/page/patch reader on exactly one measured
    /// pool. Caller prepares bounded display/SQL guards BEFORE closing below.
    pub fn read_files_v13(
        &mut self,
        limits: &PrResourceReadLimitsV1,
        cancel: &CancellationToken,
        after_opening: impl FnOnce(&PrResourceParentV1) -> GithubResult<()>,
    ) -> GithubResult<PrResourceReadV1> {
        limits.validate()?;
        let window = self.reserve_measured_external_reads(
            limits.max_external_requests(PrResourceKindV1::Files),
            limits.max_document_bytes.max(limits.max_patch_bytes),
            limits.max_pool_bytes,
            cancel,
        )?;
        let measured = self
            .core
            .provider
            .clone()
            .with_measured_read_window(window)?;
        measured.pr_resource_read_personal_v13(
            self.observation(),
            &self.intent,
            PrResourceKindV1::Files,
            limits,
            cancel,
            after_opening,
        )
    }
    /// Actual closing repository AFTER caller's bounded preparation, then exact
    /// local guard and final original principal. No SQL or body work afterward.
    pub fn close_with_guard(
        mut self,
        cancel: &CancellationToken,
        guard: impl FnOnce() -> GithubResult<()>,
    ) -> GithubResult<ClosedPersonalPrAccessV13> {
        let deadline = self.deadline();
        check_original_v13(self.core.provider, cancel, deadline)?;
        if self
            .core
            .verify(self.original.reference(), cancel)?
            .repository()
            .permissions
            .pull
            != Some(true)
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Original personal repository no longer exposes observed read permission",
            ));
        }
        let original = self.original;
        let closed = self.core.close_with_guard(cancel, guard)?;
        if closed.verified.len() != 1
            || !closed.failures.is_empty()
            || cancel.is_cancelled()
            || Instant::now() >= deadline
        {
            return Err(error(
                GithubErrorKind::Transport,
                "Original personal closing proof unavailable; no private result admitted",
            ));
        }
        Ok(ClosedPersonalPrAccessV13 { original, closed })
    }
}
