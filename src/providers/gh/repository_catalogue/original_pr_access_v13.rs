//! Closed original-pr membership routing, not a new provider/session budget.
//! Catalogue public ABI is unchanged. Personal is an actual producer original.
use super::*;

pub(crate) enum OriginalPrMembershipRefV13<'a> {
    Catalogue(&'a RepositoryCatalogueRead),
    Personal(&'a Arc<PersonalRepositoryOriginalV13>),
    Notifications(&'a Arc<NotificationPrSourceV63>),
}
pub(crate) struct OriginalPrAccessSessionV13<'a> {
    core: OriginalRepositoryAccessCoreV13<'a>,
}
pub(crate) struct OriginalPrClosedAccessV13 {
    closed: original_repository_access_core_v13::ClosedOriginalRepositoryAccessV13,
}
impl std::fmt::Debug for OriginalPrAccessSessionV13<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OriginalPrAccessSessionV13(<same original core; pending read only>)")
    }
}
impl GithubProvider {
    /// Bounds only: preserves the actual issuer's shorter queued window/cap.
    /// Never issues a source, measured allocation or renewed permission.
    pub(crate) fn with_original_pr_read_bounds_v13(
        mut self,
        deadline: Instant,
        cap: usize,
    ) -> GithubResult<Self> {
        if !(1..=1024 * 1024).contains(&cap) {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Original PR capture bound invalid",
            ));
        }
        self.scoped_read_window = Some(match self.scoped_read_window {
            Some((original, original_cap)) => (original.min(deadline), original_cap.min(cap)),
            None => (deadline, cap),
        });
        Ok(self)
    }
    pub(crate) fn original_pr_read_deadline_v13(&self) -> Option<Instant> {
        self.scoped_read_window.map(|(deadline, _)| deadline)
    }
    pub(crate) fn begin_original_pr_access_v13<'a>(
        &'a self,
        original: OriginalPrMembershipRefV13<'a>,
        limits: &RepositoryCatalogueAccessLimits,
        cancel: &CancellationToken,
    ) -> GithubResult<OriginalPrAccessSessionV13<'a>> {
        let core = match original {
            OriginalPrMembershipRefV13::Catalogue(read) => {
                self.begin_repository_catalogue_access(read, limits, cancel)?
                    .core
            }
            OriginalPrMembershipRefV13::Personal(original) => {
                limits.validate()?;
                validate_original(&self.host, original.identity())?;
                if self.scoped_read_budget.is_some() || limits.max_repository_reads < 2 {
                    return Err(error(GithubErrorKind::InvalidTarget,
                        "Original personal source needs one unallocated phase and both repository brackets"));
                }
                let mut budget = CatalogueBudget::new(&limits.transport)?;
                budget.original_cancellation = Some(cancel.clone());
                let identity = principal(self, &mut budget, cancel).map_err(safe_failure)?;
                same_principal(original.identity(), &identity)?;
                OriginalRepositoryAccessCoreV13::from_started_personal_v13(
                    self,
                    original.clone(),
                    identity,
                    limits,
                    budget,
                    cancel,
                    None,
                )?
            }
            OriginalPrMembershipRefV13::Notifications(source) => {
                limits.validate()?;
                validate_original(&self.host, source.identity())?;
                if self.scoped_read_budget.is_some()
                    || limits.max_repository_reads < 2
                    || !source.original().matches_issuer(self)
                {
                    return Err(error(
                        GithubErrorKind::IdentityChanged,
                        "Actual notification PR issuer and both fresh repository brackets required",
                    ));
                }
                let mut budget = CatalogueBudget::new(&limits.transport)?;
                budget.original_cancellation = Some(cancel.clone());
                let identity = principal(self, &mut budget, cancel).map_err(safe_failure)?;
                same_principal(source.identity(), &identity)?;
                OriginalRepositoryAccessCoreV13::from_started_notifications_v63(
                    self,
                    source.original().clone(),
                    identity,
                    limits,
                    budget,
                    cancel,
                )?
            }
        };
        let session = OriginalPrAccessSessionV13 { core };
        session.remaining(cancel)?;
        Ok(session)
    }
}
impl OriginalPrAccessSessionV13<'_> {
    pub(crate) fn identity(&self) -> &HostIdentity {
        self.core.identity()
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.core.deadline()
    }
    pub(crate) fn remaining(&self, cancel: &CancellationToken) -> GithubResult<Duration> {
        let remaining = self.core.remaining(cancel)?;
        if Instant::now() >= self.deadline() {
            return Err(error(
                GithubErrorKind::Transport,
                "Original queued PR deadline expired; no source phase renewed",
            ));
        }
        Ok(remaining)
    }
    pub(crate) fn verify(
        &mut self,
        reference: &IntentReference,
        cancel: &CancellationToken,
    ) -> GithubResult<&CurrentCatalogueRepository> {
        self.remaining(cancel)?;
        self.core.validate_personal_reference_v13(reference)?;
        self.core.verify(reference, cancel)
    }
    pub(crate) fn observation_v1(&self, id: u64) -> Option<&CurrentCatalogueRepository> {
        self.core.observation_v1(id)
    }
    pub(crate) fn reserve_measured_external_reads(
        &mut self,
        requests: usize,
        child_bytes: usize,
        pool_bytes: usize,
        repository_reads: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<MeasuredExternalReadWindowV1> {
        self.remaining(cancel)?;
        self.core.reserve_measured_external_reads(
            requests,
            child_bytes,
            pool_bytes,
            repository_reads,
            cancel,
        )
    }
    pub(crate) fn reserve_postwrite_capture_v2(
        &mut self,
        caps: &[usize],
        repository_reads: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<PostWriteCaptureAllowanceV2> {
        self.remaining(cancel)?;
        self.core
            .reserve_postwrite_capture_v2(caps, repository_reads, cancel)
    }
    /// Caller completes actual opening and closing repository reads. This does
    /// NOT append a hidden third GET or reopen SQL after final principal.
    pub(crate) fn close_with_guard(
        self,
        cancel: &CancellationToken,
        guard: impl FnOnce() -> GithubResult<()>,
    ) -> GithubResult<OriginalPrClosedAccessV13> {
        self.remaining(cancel)?;
        if !self.core.personal_closing_ready_v13() {
            return Err(error(GithubErrorKind::IdentityChanged,
                "Original personal source lacks both repository observations/current read permission"));
        }
        let deadline = self.deadline();
        let closed = self.core.close_with_guard(cancel, guard)?;
        if cancel.is_cancelled() || Instant::now() >= deadline {
            return Err(error(
                GithubErrorKind::Transport,
                "Original PR closure expired before pure result admission",
            ));
        }
        Ok(OriginalPrClosedAccessV13 { closed })
    }
}
impl OriginalPrClosedAccessV13 {
    pub(crate) fn identity(&self) -> &HostIdentity {
        &self.closed.identity
    }
    pub(crate) fn verified(&self) -> &[CurrentCatalogueRepository] {
        &self.closed.verified
    }
    pub(crate) fn observation(&self, id: u64) -> Option<&CurrentCatalogueRepository> {
        self.verified().iter().find(|row| row.original.id == id)
    }
    pub(crate) fn external_read_accounting(
        &self,
    ) -> Option<&crate::providers::runner::ScopedReadAccountingV1> {
        self.closed.measured_external_accounting.as_ref()
    }

    /// Test-only observation of the genuine Core's prepaid capture counters.
    /// This is accounting, not a fresh session or permission to issue reads.
    #[cfg(test)]
    pub(crate) fn postwrite_capture_v2(&self) -> Option<&PostWriteCaptureAccountingV2> {
        self.closed.postwrite_capture_v2.as_ref()
    }
}
