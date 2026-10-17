//! One original producer/Core for both actual origins. Only successful genuine
//! source closure emits this read provenance; no DTO/source constructor exists.
use crate::{
    pr_actions::PrCommand,
    pr_collaboration_origin_v17::PrCollaborationOpenIntentV17,
    providers::{
        gh::{
            CurrentCatalogueRepository, GithubError, GithubErrorKind, GithubProvider, GithubResult,
            HostIdentity, OriginalConversationCommentV17, OriginalConversationReactionsV17,
            OriginalConversationSubscriptionV17, OriginalPrAccessSessionV13,
            OriginalPrClosedAccessV13, PersonalPrAccessSessionV13, PrResourceParentV1,
            RepositoryCatalogueAccessLimits, ThreadPhaseIssuerV16,
        },
        runner::CancellationToken,
    },
    scoped_pr_observed_tabs_v1::resources::{PrLocalSourceBoundaryV13, ResourceLocalGuard},
    storage::{Cache, ClosedPersonalParentBoundaryV16},
};
use std::{path::Path, sync::Arc, time::Instant};
#[path = "scoped_pr_collaboration_initial_v17.rs"]
pub(crate) mod initial_v17;
#[path = "scoped_pr_issue_links_phase_v87.rs"]
pub(crate) mod issue_links_phase_v87;
#[path = "scoped_pr_metadata_base_phase_v32.rs"]
pub(crate) mod metadata_base_phase_v32;
#[path = "scoped_pr_metadata_context_v51.rs"]
pub(crate) mod metadata_context_v51;
#[path = "scoped_pr_metadata_members_phase_v48.rs"]
pub(crate) mod metadata_members_phase_v48;
#[path = "scoped_pr_metadata_phase_v25.rs"]
pub(crate) mod metadata_phase_v25;
#[path = "scoped_pr_collaboration_phase_v17.rs"]
pub(crate) mod phase_v17;
#[path = "scoped_pr_state_phase_v68.rs"]
pub(crate) mod state_phase_v68;
#[path = "scoped_pr_state_source_v68.rs"]
pub(crate) mod state_v68;
enum ClosedOriginV17 {
    Catalogue(Box<OriginalPrClosedAccessV13>),
    Personal(Arc<ClosedPersonalParentBoundaryV16>),
}
pub(crate) enum ConversationOpenObservationV17 {
    New,
    Comment(Arc<OriginalConversationCommentV17>),
    Reactions(OriginalConversationReactionsV17),
    Subscription(OriginalConversationSubscriptionV17),
}
pub(crate) struct PrCollaborationSourceV17 {
    opening: Arc<PrCollaborationOpenIntentV17>,
    parent: Arc<PrResourceParentV1>,
    identity: HostIdentity,
    closed: ClosedOriginV17,
    // SAME pre-final-UID guard, moved rather than discarded. Used only by
    // worker-side local draft recoverability checks; never source renewal.
    local_recovery_v20: ResourceLocalGuard,
}
pub(crate) struct OpenedConversationV17 {
    pub(crate) source: Arc<PrCollaborationSourceV17>,
}
impl std::fmt::Debug for PrCollaborationSourceV17 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "PrCollaborationSourceV17(<actual closed original parent; no write permission>)",
        )
    }
}
enum SessionV17<'a> {
    Catalogue(OriginalPrAccessSessionV13<'a>),
    Personal(PersonalPrAccessSessionV13<'a>),
}
fn failure() -> GithubError {
    GithubError {
        kind: GithubErrorKind::IdentityChanged,
        message: "Exact original Conversation source/account/database unavailable; no fallback"
            .into(),
    }
}
fn live(cancel: &CancellationToken, deadline: Instant) -> GithubResult<()> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        Err(failure())
    } else {
        Ok(())
    }
}
impl SessionV17<'_> {
    fn reserve_state_v68(
        &mut self,
        cancel: &CancellationToken,
    ) -> GithubResult<crate::providers::gh::MeasuredExternalReadWindowV1> {
        match self {
            Self::Catalogue(v) => {
                v.reserve_measured_external_reads(4, 128 * 1024, 8 * 1024 * 1024, 1, cancel)
            }
            Self::Personal(v) => {
                v.reserve_measured_external_reads(4, 128 * 1024, 8 * 1024 * 1024, cancel)
            }
        }
    }
    fn reserve_issue_link_manual_v94(
        &mut self,
        metadata: bool,
        cancel: &CancellationToken,
    ) -> GithubResult<crate::providers::gh::MeasuredExternalReadWindowV1> {
        let caps: &[usize] = if metadata {
            &crate::providers::gh::ISSUE_LINK_METADATA_REFRESH_CAPS_V87
        } else {
            &crate::providers::gh::ISSUE_LINK_REFRESH_CAPS_V87
        };
        // The SAME original Core owns its first and last three roles. Only
        // the fixed intervening13/17 are carved once from that original pool.
        let measured = &caps[3..caps.len().checked_sub(3).ok_or_else(failure)?];
        let requests = measured.len();
        let total = measured
            .iter()
            .try_fold(0usize, |sum, cap| sum.checked_add(*cap))
            .ok_or_else(failure)?;
        match self {
            Self::Catalogue(v) => {
                v.reserve_measured_external_reads(requests, 512 * 1024, total, 1, cancel)
            }
            Self::Personal(v) => {
                v.reserve_measured_external_reads(requests, 512 * 1024, total, cancel)
            }
        }
    }
    fn reserve_issue_link_inspection_v87(
        &mut self,
        cancel: &CancellationToken,
    ) -> GithubResult<crate::providers::gh::MeasuredExternalReadWindowV1> {
        match self {
            Self::Catalogue(v) => {
                v.reserve_measured_external_reads(8, 512 * 1024, 8 * 1024 * 1024, 1, cancel)
            }
            Self::Personal(v) => {
                v.reserve_measured_external_reads(8, 512 * 1024, 8 * 1024 * 1024, cancel)
            }
        }
    }
    fn reserve_issue_link_open_v87(
        &mut self,
        pages: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<crate::providers::gh::MeasuredExternalReadWindowV1> {
        if !(1..=4).contains(&pages) {
            return Err(failure());
        }
        let requests = pages.checked_add(4).ok_or_else(failure)?;
        match self {
            Self::Catalogue(v) => {
                v.reserve_measured_external_reads(requests, 512 * 1024, 8 * 1024 * 1024, 1, cancel)
            }
            Self::Personal(v) => {
                v.reserve_measured_external_reads(requests, 512 * 1024, 8 * 1024 * 1024, cancel)
            }
        }
    }
    fn reserve_metadata_refresh_v51(
        &mut self,
        cancel: &CancellationToken,
    ) -> GithubResult<crate::providers::gh::MeasuredExternalReadWindowV1> {
        match self {
            Self::Catalogue(v) => {
                v.reserve_measured_external_reads(11, 512 * 1024, 8 * 1024 * 1024, 1, cancel)
            }
            Self::Personal(v) => {
                v.reserve_measured_external_reads(11, 512 * 1024, 8 * 1024 * 1024, cancel)
            }
        }
    }
    fn reserve_metadata_section_v51(
        &mut self,
        cancel: &CancellationToken,
    ) -> GithubResult<crate::providers::gh::MeasuredExternalReadWindowV1> {
        match self {
            Self::Catalogue(v) => {
                v.reserve_measured_external_reads(4, 128 * 1024, 8 * 1024 * 1024, 1, cancel)
            }
            Self::Personal(v) => {
                v.reserve_measured_external_reads(4, 128 * 1024, 8 * 1024 * 1024, cancel)
            }
        }
    }
    fn identity(&self) -> &HostIdentity {
        match self {
            Self::Catalogue(v) => v.identity(),
            Self::Personal(v) => v.identity(),
        }
    }
    fn current(
        &self,
        opening: &PrCollaborationOpenIntentV17,
    ) -> GithubResult<&CurrentCatalogueRepository> {
        match self {
            Self::Catalogue(v) => v
                .observation_v1(
                    opening
                        .catalogue_navigation()
                        .ok_or_else(failure)?
                        .original()
                        .id,
                )
                .ok_or_else(failure),
            Self::Personal(v) => Ok(v.observation()),
        }
    }
    fn reserve(
        &mut self,
        cancel: &CancellationToken,
    ) -> GithubResult<crate::providers::gh::MeasuredExternalReadWindowV1> {
        match self {
            Self::Catalogue(v) => {
                v.reserve_measured_external_reads(8, 1024 * 1024, 8 * 1024 * 1024, 1, cancel)
            }
            Self::Personal(v) => {
                v.reserve_measured_external_reads(8, 1024 * 1024, 8 * 1024 * 1024, cancel)
            }
        }
    }
    fn reserve_refresh_v19(
        &mut self,
        cancel: &CancellationToken,
    ) -> GithubResult<crate::providers::gh::MeasuredExternalReadWindowV1> {
        match self {
            Self::Catalogue(v) => {
                v.reserve_measured_external_reads(7, 512 * 1024, 8 * 1024 * 1024, 1, cancel)
            }
            Self::Personal(v) => {
                v.reserve_measured_external_reads(7, 512 * 1024, 8 * 1024 * 1024, cancel)
            }
        }
    }
    fn reserve_metadata_text_v25(
        &mut self,
        cancel: &CancellationToken,
    ) -> GithubResult<crate::providers::gh::MeasuredExternalReadWindowV1> {
        match self {
            Self::Catalogue(v) => {
                v.reserve_measured_external_reads(3, 128 * 1024, 8 * 1024 * 1024, 1, cancel)
            }
            Self::Personal(v) => {
                v.reserve_measured_external_reads(3, 128 * 1024, 8 * 1024 * 1024, cancel)
            }
        }
    }
    fn reserve_metadata_members_v48(
        &mut self,
        limits: &crate::providers::gh::PrMetadataReadLimits,
        cancel: &CancellationToken,
    ) -> GithubResult<crate::providers::gh::MeasuredExternalReadWindowV1> {
        if !(1..=10).contains(&limits.max_pages) {
            return Err(failure());
        }
        let requests = limits.max_pages.checked_add(4).ok_or_else(failure)?;
        match self {
            Self::Catalogue(v) => {
                v.reserve_measured_external_reads(requests, 128 * 1024, 8 * 1024 * 1024, 1, cancel)
            }
            Self::Personal(v) => {
                v.reserve_measured_external_reads(requests, 128 * 1024, 8 * 1024 * 1024, cancel)
            }
        }
    }
    fn reserve_metadata_members_inspection_v48(
        &mut self,
        cancel: &CancellationToken,
    ) -> GithubResult<crate::providers::gh::MeasuredExternalReadWindowV1> {
        match self {
            Self::Catalogue(v) => {
                v.reserve_measured_external_reads(4, 128 * 1024, 8 * 1024 * 1024, 1, cancel)
            }
            Self::Personal(v) => {
                v.reserve_measured_external_reads(4, 128 * 1024, 8 * 1024 * 1024, cancel)
            }
        }
    }
    fn reserve_metadata_members_list_v48(
        &mut self,
        cancel: &CancellationToken,
    ) -> GithubResult<crate::providers::gh::MeasuredExternalReadWindowV1> {
        match self {
            Self::Catalogue(v) => {
                v.reserve_measured_external_reads(2, 128 * 1024, 1024 * 1024, 1, cancel)
            }
            Self::Personal(v) => {
                v.reserve_measured_external_reads(2, 128 * 1024, 1024 * 1024, cancel)
            }
        }
    }
    fn reserve_metadata_members_manual_v48(
        &mut self,
        cancel: &CancellationToken,
    ) -> GithubResult<crate::providers::gh::MeasuredExternalReadWindowV1> {
        let caps = crate::providers::gh::METADATA_MEMBERS_REFRESH_CAPS_V48;
        let requests = caps.len();
        let child = caps.iter().copied().max().ok_or_else(failure)?;
        let total = caps
            .iter()
            .try_fold(0usize, |sum, cap| sum.checked_add(*cap))
            .ok_or_else(failure)?;
        match self {
            Self::Catalogue(v) => {
                v.reserve_measured_external_reads(requests, child, total, 1, cancel)
            }
            Self::Personal(v) => v.reserve_measured_external_reads(requests, child, total, cancel),
        }
    }
    fn reserve_metadata_members_tab_manual_v62(
        &mut self,
        cancel: &CancellationToken,
    ) -> GithubResult<crate::providers::gh::MeasuredExternalReadWindowV1> {
        let caps = crate::providers::gh::METADATA_MEMBERS_TAB_REFRESH_CAPS_V62;
        let total = caps
            .iter()
            .try_fold(0usize, |sum, cap| sum.checked_add(*cap))
            .ok_or_else(failure)?;
        match self {
            Self::Catalogue(session) => {
                session.reserve_measured_external_reads(caps.len(), 512 * 1024, total, 1, cancel)
            }
            Self::Personal(session) => {
                session.reserve_measured_external_reads(caps.len(), 512 * 1024, total, cancel)
            }
        }
    }
    fn reserve_metadata_base_v32(
        &mut self,
        requests: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<crate::providers::gh::MeasuredExternalReadWindowV1> {
        if !(3..=13).contains(&requests) {
            return Err(failure());
        }
        match self {
            Self::Catalogue(v) => {
                v.reserve_measured_external_reads(requests, 128 * 1024, 8 * 1024 * 1024, 1, cancel)
            }
            Self::Personal(v) => {
                v.reserve_measured_external_reads(requests, 128 * 1024, 8 * 1024 * 1024, cancel)
            }
        }
    }
}
impl PrCollaborationSourceV17 {
    /// Retain the SAME original Source/guard across State repair preparation.
    /// This does not recapture an epoch or tolerate a changed reservation.
    pub(crate) fn validate_state_cache_repair_boundary_v75(
        &self,
        repair: &crate::pr_write_journal::state_origin_v68::StateCacheRepairV74,
    ) -> GithubResult<()> {
        if !std::ptr::eq(self, repair.source().as_ref()) {
            return Err(failure());
        }
        self.local_recovery_v20
            .validate_state_cache_repair_boundary_v75(repair)
    }
    pub(crate) fn admits_personal_refresh_producer_v20(&self, provider: &GithubProvider) -> bool {
        matches!(&self.closed,ClosedOriginV17::Personal(boundary) if boundary.admits_producer(provider))
    }
    pub(crate) fn validate_local_draft_recovery_v20(
        &self,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        live(cancel, deadline)?;
        if database != self.opening.database() {
            return Err(failure());
        }
        self.local_recovery_v20
            .validate_boundary_v13(database, self.local_boundary())?;
        live(cancel, deadline)
    }
    pub(crate) fn opening(&self) -> &Arc<PrCollaborationOpenIntentV17> {
        &self.opening
    }
    pub(crate) fn parent(&self) -> &Arc<PrResourceParentV1> {
        &self.parent
    }
    pub(crate) fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub(crate) fn current(&self) -> GithubResult<&CurrentCatalogueRepository> {
        match &self.closed {
            ClosedOriginV17::Catalogue(v) => v
                .observation(
                    self.opening
                        .catalogue_navigation()
                        .ok_or_else(failure)?
                        .original()
                        .id,
                )
                .ok_or_else(failure),
            ClosedOriginV17::Personal(v)
                if Arc::ptr_eq(v.parent(), &self.parent) && v.scope() == self.opening.scope() =>
            {
                Ok(v.observation_v17())
            }
            ClosedOriginV17::Personal(_) => Err(failure()),
        }
    }
    pub(crate) fn local_boundary(&self) -> PrLocalSourceBoundaryV13<'_> {
        match &self.closed {
            ClosedOriginV17::Catalogue(_) => PrLocalSourceBoundaryV13::Catalogue(
                self.opening
                    .catalogue_source()
                    .expect("closed exact origin"),
            ),
            ClosedOriginV17::Personal(v) => PrLocalSourceBoundaryV13::PersonalParent(v),
        }
    }
    #[cfg(test)]
    pub(crate) fn owns_presentation(&self, app: &crate::app::App) -> bool {
        self.opening.owns_presentation(app)
    }
}
/// No arbitrary provider/window escapes this purpose. Parent/selected
/// observations are prepared under ONE actual Core and only admitted AFTER
/// local SQL + original final principal closure.
#[cfg(test)]
pub(crate) fn open(
    opening: Arc<PrCollaborationOpenIntentV17>,
    command: PrCommand,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<OpenedConversationV17> {
    open_with_preparation(
        opening,
        command,
        provider,
        database,
        cancel,
        deadline,
        |_, _, _, _| Ok(()),
    )
    .map(|(opened, ())| opened)
}
/// Production Open prepares fields, bounded draft restoration and actor/layout
/// material BEFORE actual final UID. The simple wrapper is factory evidence,
/// not an excuse to materialize a private Form after source closure.
#[cfg(test)]
pub(crate) fn open_with_preparation<T>(
    opening: Arc<PrCollaborationOpenIntentV17>,
    command: PrCommand,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    prepare: impl FnOnce(
        &Arc<PrResourceParentV1>,
        &HostIdentity,
        &ConversationOpenObservationV17,
        &ResourceLocalGuard,
    ) -> GithubResult<T>,
) -> GithubResult<(OpenedConversationV17, T)> {
    open_with_lineage_preparation_v18(
        opening,
        command,
        true,
        provider,
        database,
        cancel,
        deadline,
        |parent, identity, observed, local, _| prepare(parent, identity, observed, local),
        |_| Ok(()),
    )
}
/// SAME producer, with an explicit later user phase and a closed live-read
/// Base. The added SQL witness is checked in the existing final Core guard.
#[expect(
    clippy::too_many_arguments,
    reason = "One original phase with caller-bracketed typed preparation and exact final SQL witness; no provider escape"
)]
pub(crate) fn open_with_lineage_preparation_v18<T>(
    opening: Arc<PrCollaborationOpenIntentV17>,
    command: PrCommand,
    initial: bool,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    prepare: impl FnOnce(
        &Arc<PrResourceParentV1>,
        &HostIdentity,
        &ConversationOpenObservationV17,
        &ResourceLocalGuard,
        &crate::pr_write_journal::conversation_action_lineage_v18::ConversationActionBaseV18,
    ) -> GithubResult<T>,
    validate: impl Fn(&T) -> GithubResult<()>,
) -> GithubResult<(OpenedConversationV17, T)> {
    if !opening.admits_conversation_v51() {
        return Err(failure());
    }
    let completed = initial_v17::run(
        opening.clone(),
        initial,
        provider,
        database,
        cancel,
        deadline,
        |read| {
            let local = ResourceLocalGuard::capture_boundary_v13(
                database,
                read.boundary,
                read.identity,
                read.parent.summary(),
            )?;
            let observed = read.observe(command)?;
            let base = read.action_base_v18(command, &observed)?;
            let material = prepare(read.parent, read.identity, &observed, &local, &base)?;
            Ok((local, material))
        },
        |(local, material), boundary| {
            local.validate_boundary_v13(database, boundary)?;
            validate(material)
        },
    )?;
    let (local_recovery_v20, material) = completed.material;
    Ok((
        OpenedConversationV17 {
            source: Arc::new(PrCollaborationSourceV17 {
                opening,
                parent: completed.parent,
                identity: completed.identity,
                closed: completed.closed,
                local_recovery_v20,
            }),
        },
        material,
    ))
}

/// SAME actual initial producer, closed metadata purpose. Preparation receives
/// a genuine field/capability observation, never a fabricated PrCommand/DTO.
#[expect(
    clippy::too_many_arguments,
    reason = "One original phase and exact preparation/final SQL witness, with no transport/source escape"
)]
pub(crate) fn open_metadata_text_v25<T>(
    opening: Arc<PrCollaborationOpenIntentV17>,
    field: crate::providers::gh::PrMetadataTextFieldV25,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    prepare: impl FnOnce(
        &crate::providers::gh::OriginalMetadataTextV25,
        &ResourceLocalGuard,
    ) -> GithubResult<T>,
    validate: impl Fn(&T) -> GithubResult<()>,
) -> GithubResult<(
    Arc<PrCollaborationSourceV17>,
    crate::providers::gh::OriginalMetadataTextV25,
    T,
)> {
    if !opening.admits_metadata_field_v51(field) {
        return Err(failure());
    }
    let completed = initial_v17::run_metadata_text_v25(
        Arc::clone(&opening),
        true,
        field,
        provider,
        database,
        cancel,
        deadline,
        |mut read| {
            let local = ResourceLocalGuard::capture_boundary_v13(
                database,
                read.boundary,
                read.identity,
                read.parent.summary(),
            )?;
            let actual = read.observe_metadata_text_v25()?;
            let material = prepare(&actual, &local)?;
            Ok((local, actual, material))
        },
        |(local, _, material), boundary| {
            local.validate_boundary_v13(database, boundary)?;
            validate(material)
        },
    )?;
    let (local_recovery_v20, actual, material) = completed.material;
    provider.validate_original_metadata_generation_v25(&actual, cancel, deadline)?;
    let source = Arc::new(PrCollaborationSourceV17 {
        opening,
        parent: completed.parent,
        identity: completed.identity,
        closed: completed.closed,
        local_recovery_v20,
    });
    Ok((source, actual, material))
}

/// READ-only Metadata purpose: the same normal Core closes; its closed
/// producer is dropped, never converted to a writer/source with a blocker.
#[expect(
    clippy::too_many_arguments,
    reason = "Closed original read purpose and exact original SQL snapshot callback stay explicit"
)]
pub(crate) fn with_metadata_history_v25<T>(
    opening: Arc<PrCollaborationOpenIntentV17>,
    field: crate::providers::gh::PrMetadataTextFieldV25,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    prepare: impl FnOnce(
        crate::providers::gh::MetadataRecoveryCurrentV25,
        PrLocalSourceBoundaryV13<'_>,
    ) -> GithubResult<T>,
    validate: impl Fn(&T, PrLocalSourceBoundaryV13<'_>) -> GithubResult<()>,
) -> GithubResult<(T, HostIdentity)> {
    if !opening.admits_metadata_field_v51(field) {
        return Err(failure());
    }
    let completed = initial_v17::run_metadata_text_v25(
        opening,
        false,
        field,
        provider,
        database,
        cancel,
        deadline,
        |mut read| {
            let observation = read.inspect_metadata_v25()?;
            prepare(observation, read.boundary)
        },
        validate,
    )?;
    Ok((completed.material, completed.identity))
}
/// Closed read-only Base history/inspection: exact current Base is observed,
/// but no eligible branch selection, publisher source or write permit exists.
pub(crate) fn with_base_history_v32<T>(
    opening: Arc<PrCollaborationOpenIntentV17>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    prepare: impl FnOnce(
        crate::providers::gh::BaseRecoveryCurrentV32,
        PrLocalSourceBoundaryV13<'_>,
    ) -> GithubResult<T>,
    validate: impl Fn(&T, PrLocalSourceBoundaryV13<'_>) -> GithubResult<()>,
) -> GithubResult<(T, HostIdentity)> {
    let completed = initial_v17::run_base_history_v32(
        opening,
        provider,
        database,
        cancel,
        deadline,
        |mut read| {
            let current = read.inspect_base_v32()?;
            prepare(current, read.boundary)
        },
        validate,
    )?;
    Ok((completed.material, completed.identity))
}

/// Read-only History/Inspect wrapper: closes the SAME initial Core and drops
/// its closed producer after final UID. It CANNOT emit a publication source.
pub(crate) fn with_history_read_v17<T>(
    opening: Arc<PrCollaborationOpenIntentV17>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    prepare: impl FnOnce(initial_v17::InitialReadV17<'_>) -> GithubResult<T>,
    validate: impl Fn(&T, PrLocalSourceBoundaryV13<'_>) -> GithubResult<()>,
) -> GithubResult<(T, HostIdentity)> {
    let completed = initial_v17::run(
        opening, false, provider, database, cancel, deadline, prepare, validate,
    )?;
    Ok((completed.material, completed.identity))
}

/// Closed explicit fresh-read purpose on the SAME actual initial Core. No
/// source is issued and no DraftClient/journal mutation is reachable here.
pub(crate) fn with_refresh_read_v19<T>(
    opening: Arc<PrCollaborationOpenIntentV17>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    prepare: impl FnOnce(initial_v17::RefreshReadV19<'_>) -> GithubResult<T>,
    validate: impl Fn(&T, PrLocalSourceBoundaryV13<'_>) -> GithubResult<()>,
) -> GithubResult<initial_v17::ClosedRefreshReadV19<T>> {
    let completed = initial_v17::run_refresh_v19(
        opening, provider, database, cancel, deadline, prepare, validate,
    )?;
    Ok(completed)
}
#[expect(
    clippy::too_many_arguments,
    reason = "Exact current Metadata OwnFence/Personal settlement on SAME explicit fresh Core"
)]
pub(crate) fn with_metadata_tab_refresh_v51<T>(
    own: &Arc<crate::storage::conversation_refresh_v19::ConversationOwnFenceV19>,
    attempt: Option<
        &Arc<crate::storage::conversation_refresh_v19::PersonalConversationReadAttemptV20>,
    >,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    prepare: impl FnOnce(initial_v17::RefreshReadV19<'_>) -> GithubResult<T>,
    validate: impl Fn(&T, PrLocalSourceBoundaryV13<'_>) -> GithubResult<()>,
) -> GithubResult<initial_v17::ClosedRefreshReadV19<T>> {
    if own.is_personal_v20() && !own.source().admits_personal_refresh_producer_v20(provider) {
        return Err(failure());
    }
    initial_v17::run_metadata_refresh_v51(
        own, attempt, provider, database, cancel, deadline, prepare, validate,
    )
}
#[expect(
    clippy::too_many_arguments,
    reason = "One explicit Personal read carries its actual non-permission reservation attempt"
)]
pub(crate) fn with_personal_refresh_read_v20<T>(
    opening: Arc<PrCollaborationOpenIntentV17>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    attempt: &Arc<crate::storage::conversation_refresh_v19::PersonalConversationReadAttemptV20>,
    prepare: impl FnOnce(initial_v17::RefreshReadV19<'_>) -> GithubResult<T>,
    validate: impl Fn(&T, PrLocalSourceBoundaryV13<'_>) -> GithubResult<()>,
) -> GithubResult<initial_v17::ClosedRefreshReadV19<T>> {
    initial_v17::run_refresh_attempt_v20(
        opening,
        provider,
        database,
        cancel,
        deadline,
        Some(attempt),
        prepare,
        validate,
    )
}

/// Explicit read from the actual Base terminal only. Derive its expected-new
/// parent here; saved reference/DTOs cannot supply another transition.
#[expect(
    clippy::too_many_arguments,
    reason = "One typed original OwnFence/current attempt and exact existing preparation/SQL closure"
)]
pub(crate) fn with_base_refresh_read_v36<T>(
    own: &Arc<crate::storage::conversation_refresh_v19::ConversationOwnFenceV19>,
    attempt: Option<
        &Arc<crate::storage::conversation_refresh_v19::PersonalConversationReadAttemptV20>,
    >,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    prepare: impl FnOnce(initial_v17::RefreshReadV19<'_>) -> GithubResult<T>,
    validate: impl Fn(&T, PrLocalSourceBoundaryV13<'_>) -> GithubResult<()>,
) -> GithubResult<initial_v17::ClosedRefreshReadV19<T>> {
    live(cancel, deadline)?;
    let source = own.source();
    if database != source.opening.database()
        || database != own.terminal().database()
        || own.terminal().base_transition_v32().is_none()
        || !Arc::ptr_eq(source, own.terminal().source_v19())
    {
        return Err(failure());
    }
    if own.is_personal_v20() {
        let attempt = attempt.ok_or_else(failure)?;
        if !attempt.matches_own(own) || !source.admits_personal_refresh_producer_v20(provider) {
            return Err(failure());
        }
        attempt
            .matches_phase(cancel, deadline)
            .map_err(|_| failure())?;
    } else {
        if attempt.is_some() {
            return Err(failure());
        }
        own.validate_before_read(cancel, deadline)
            .map_err(|_| failure())?;
    }
    initial_v17::run_base_refresh_v36(
        Arc::clone(source.opening()),
        own,
        attempt,
        provider,
        database,
        cancel,
        deadline,
        prepare,
        validate,
    )
}

pub(crate) struct StateRefreshReadV68<'a> {
    pub(crate) own: &'a Arc<crate::storage::conversation_refresh_v19::ConversationOwnFenceV19>,
    pub(crate) attempt: Option<
        &'a Arc<crate::storage::conversation_refresh_v19::PersonalConversationReadAttemptV20>,
    >,
}
pub(crate) fn with_state_refresh_read_v68<T>(
    request: StateRefreshReadV68<'_>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    prepare: impl FnOnce(initial_v17::RefreshReadV19<'_>) -> GithubResult<T>,
    validate: impl Fn(&T, PrLocalSourceBoundaryV13<'_>) -> GithubResult<()>,
) -> GithubResult<initial_v17::ClosedRefreshReadV19<T>> {
    live(cancel, deadline)?;
    let source = request.own.source();
    if database != source.opening.database()
        || database != request.own.terminal().database()
        || request.own.terminal().state_transition_v68().is_none()
        || !Arc::ptr_eq(source, request.own.terminal().source_v19())
    {
        return Err(failure());
    }
    if request.own.is_personal_v20() {
        let attempt = request.attempt.ok_or_else(failure)?;
        if !attempt.matches_own(request.own)
            || !source.admits_personal_refresh_producer_v20(provider)
        {
            return Err(failure());
        }
        attempt
            .matches_phase(cancel, deadline)
            .map_err(|_| failure())?;
    } else {
        if request.attempt.is_some() {
            return Err(failure());
        }
        request
            .own
            .validate_before_read(cancel, deadline)
            .map_err(|_| failure())?;
    }
    initial_v17::run_state_refresh_v68(
        initial_v17::StateRefreshReadV68 {
            own: request.own,
            attempt: request.attempt,
        },
        provider,
        database,
        cancel,
        deadline,
        prepare,
        validate,
    )
}
