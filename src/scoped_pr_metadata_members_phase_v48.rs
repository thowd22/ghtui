//! Member-only facade on the SAME original Core, one measured allocation and
//! prepaid cursor. No raw provider escapes the named member operations.
use super::*;
use crate::{
    pr_metadata_actions::{PrMetadataCommand, PrMetadataPlan},
    providers::gh::{
        MembersWriteV48, MetadataMembersObservationV48, OriginalMetadataMembersV48,
        OriginalPrMembershipRefV13, PostWriteCaptureAllowanceV2, PrMetadataMembersFailureV48,
        PrMetadataMembersReceiptV48, PrMetadataReadLimits, METADATA_MEMBERS_POST_CAPS_V48,
    },
};
pub(crate) struct MetadataMembersPublicationV48<'a> {
    pub(crate) plan: &'a PrMetadataPlan,
    pub(crate) captures: &'a mut PostWriteCaptureAllowanceV2,
    pub(crate) ack: &'a crate::drafts::DraftPublicationAckV2,
    pub(crate) drafts: &'a crate::drafts::DraftClient,
    pub(crate) database: &'a Path,
    pub(crate) key: &'a str,
}

pub(crate) struct MetadataMembersOpenV48<'a> {
    pub(crate) opening: Arc<PrCollaborationOpenIntentV17>,
    pub(crate) command: PrMetadataCommand,
    pub(crate) limits: &'a PrMetadataReadLimits,
}
pub(crate) struct MetadataMembersHistoryOpenV48 {
    pub(crate) opening: Arc<PrCollaborationOpenIntentV17>,
    pub(crate) inspect: Option<PrMetadataCommand>,
}
pub(crate) struct MetadataMembersManualPhaseV48<'a> {
    pub(crate) own: &'a Arc<crate::storage::conversation_refresh_v19::ConversationOwnFenceV19>,
    pub(crate) attempt: Option<
        &'a Arc<crate::storage::conversation_refresh_v19::PersonalConversationReadAttemptV20>,
    >,
}
pub(crate) fn with_metadata_members_manual_refresh_v48<T>(
    request: MetadataMembersManualPhaseV48<'_>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    prepare: impl FnOnce(initial_v17::RefreshReadV19<'_>) -> GithubResult<T>,
    validate: impl Fn(&T, PrLocalSourceBoundaryV13<'_>) -> GithubResult<()>,
) -> GithubResult<initial_v17::ClosedRefreshReadV19<T>> {
    let source = request.own.source();
    let deadline = deadline.min(
        provider
            .original_pr_read_deadline_v13()
            .ok_or_else(failure)?,
    );
    live(cancel, deadline)?;
    if database != source.opening().database()
        || database != request.own.terminal().database()
        || request.own.terminal().members_command_v48().is_none()
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
    initial_v17::run_metadata_members_manual_refresh_v48(
        request, provider, database, cancel, deadline, prepare, validate,
    )
}
pub(crate) fn with_metadata_members_history_v48<T>(
    request: MetadataMembersHistoryOpenV48,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    prepare: impl FnOnce(
        &Arc<PrResourceParentV1>,
        &HostIdentity,
        PrLocalSourceBoundaryV13<'_>,
        Option<crate::providers::gh::MetadataMembersRecoveryCurrentV48>,
    ) -> GithubResult<T>,
    validate: impl Fn(&T, PrLocalSourceBoundaryV13<'_>) -> GithubResult<()>,
) -> GithubResult<(T, HostIdentity)> {
    if !request.opening.admits_metadata_members_history_v61() {
        return Err(failure());
    }
    let inspect = request.inspect.is_some();
    let completed = initial_v17::run_metadata_members_history_v48(
        request,
        provider,
        database,
        cancel,
        deadline,
        |mut read| {
            let observed = if inspect {
                Some(read.inspect_metadata_members_v48()?)
            } else {
                None
            };
            prepare(read.parent, read.identity, read.boundary, observed)
        },
        validate,
    )?;
    // Deliberately drop the closed producer. History never emits a Source.
    Ok((completed.material, completed.identity))
}
pub(crate) fn open_metadata_members_v48<T>(
    request: MetadataMembersOpenV48<'_>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    prepare: impl FnOnce(&Arc<OriginalMetadataMembersV48>, &ResourceLocalGuard) -> GithubResult<T>,
) -> GithubResult<(
    Arc<PrCollaborationSourceV17>,
    Arc<OriginalMetadataMembersV48>,
    T,
)> {
    request.limits.validate_original_members_v48()?;
    if !request.opening.admits_metadata_members_v61(request.command) {
        return Err(failure());
    }
    let opening = Arc::clone(&request.opening);
    let completed = initial_v17::run_metadata_members_v48(
        request,
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
            let actual = read.take_metadata_members_v48()?;
            let material = prepare(&actual, &local)?;
            Ok((local, actual, material))
        },
        |(local, _, _), boundary| local.validate_boundary_v13(database, boundary),
    )?;
    let (local_recovery_v20, actual, material) = completed.material;
    provider.validate_original_members_generation_v48(
        &actual,
        cancel,
        deadline.min(actual.deadline()),
    )?;
    let source = Arc::new(PrCollaborationSourceV17 {
        opening,
        parent: completed.parent,
        identity: completed.identity,
        closed: completed.closed,
        local_recovery_v20,
    });
    Ok((source, actual, material))
}

#[derive(Clone, Copy)]
pub(crate) enum MetadataMembersPhasePurposeV48<'a> {
    Preview(&'a PrMetadataPlan, &'a PrMetadataReadLimits),
    Confirm(&'a PrMetadataPlan, &'a PrMetadataReadLimits),
    MetadataPreviewV62(&'a PrMetadataPlan, &'a OriginalMetadataMembersV48),
    MetadataConfirmV62(&'a PrMetadataPlan, &'a OriginalMetadataMembersV48),
}
impl MetadataMembersPhasePurposeV48<'_> {
    fn plan(&self) -> &PrMetadataPlan {
        match self {
            Self::Preview(plan, _) | Self::Confirm(plan, _) => plan,
            Self::MetadataPreviewV62(plan, _) | Self::MetadataConfirmV62(plan, _) => plan,
        }
    }
    fn limits(&self) -> &PrMetadataReadLimits {
        match self {
            Self::Preview(_, limits) | Self::Confirm(_, limits) => limits,
            Self::MetadataPreviewV62(_, prior) | Self::MetadataConfirmV62(_, prior) => {
                prior.limits()
            }
        }
    }
    fn retained_v62(&self) -> Option<&OriginalMetadataMembersV48> {
        match self {
            Self::MetadataPreviewV62(_, prior) | Self::MetadataConfirmV62(_, prior) => Some(prior),
            Self::Preview(..) | Self::Confirm(..) => None,
        }
    }
}
pub(crate) struct MetadataMembersPhaseV48<'a> {
    source: &'a PrCollaborationSourceV17,
    session: OriginalPrAccessSessionV13<'a>,
    measured: GithubProvider,
    actual: Arc<OriginalMetadataMembersV48>,
    cancel: &'a CancellationToken,
    deadline: Instant,
    captures: Option<PostWriteCaptureAllowanceV2>,
}
impl PrCollaborationSourceV17 {
    pub(crate) fn with_metadata_members_phase_v48<T>(
        &self,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
        purpose: MetadataMembersPhasePurposeV48<'_>,
        prepare: impl FnOnce(MetadataMembersPhaseV48<'_>) -> GithubResult<T>,
    ) -> GithubResult<T> {
        purpose.limits().validate_original_members_v48()?;
        if !self
            .opening
            .admits_metadata_members_v61(purpose.plan().command())
        {
            return Err(failure());
        }
        let metadata_v62 = self
            .opening
            .metadata_members_tab_v61(purpose.plan().command());
        if metadata_v62 != purpose.retained_v62().is_some() {
            return Err(failure());
        }
        if database != self.opening.database() {
            return Err(failure());
        }
        let deadline = deadline.min(
            provider
                .original_pr_read_deadline_v13()
                .ok_or_else(failure)?,
        );
        live(cancel, deadline)?;
        let provider = provider
            .clone()
            .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)?;
        let limits = RepositoryCatalogueAccessLimits {
            transport: crate::providers::gh::RepositoryCatalogueLimits {
                max_document_bytes: 128 * 1024,
                max_response_bytes: 8 * 1024 * 1024,
                ..Default::default()
            },
            max_repository_reads: 2,
            ..Default::default()
        };
        let original = match &self.closed {
            ClosedOriginV17::Catalogue(_) => OriginalPrMembershipRefV13::Catalogue(
                self.opening
                    .catalogue_source()
                    .ok_or_else(failure)?
                    .source(),
            ),
            ClosedOriginV17::Personal(boundary) => {
                if !boundary.admits_producer(&provider) {
                    return Err(failure());
                }
                OriginalPrMembershipRefV13::Personal(boundary.original())
            }
        };
        let mut session = provider.begin_original_pr_access_v13(original, &limits, cancel)?;
        session.verify(self.current()?.original(), cancel)?;
        if crate::storage::Scope::github(&session.identity().host, session.identity().account_id)
            .map_err(|_| failure())?
            != *self.opening.scope()
        {
            return Err(failure());
        }
        let captures = match purpose {
            MetadataMembersPhasePurposeV48::Confirm(_, _)
            | MetadataMembersPhasePurposeV48::MetadataConfirmV62(_, _) => {
                Some(session.reserve_postwrite_capture_v2(
                    if metadata_v62 {
                        &crate::providers::gh::METADATA_MEMBERS_TAB_POST_CAPS_V62
                    } else {
                        &METADATA_MEMBERS_POST_CAPS_V48
                    },
                    1,
                    cancel,
                )?)
            }
            MetadataMembersPhasePurposeV48::Preview(_, _)
            | MetadataMembersPhasePurposeV48::MetadataPreviewV62(_, _) => None,
        };
        let read_limits = purpose.limits();
        let requests = read_limits.max_pages.checked_add(4).ok_or_else(failure)?;
        let window = session.reserve_measured_external_reads(
            requests,
            128 * 1024,
            8 * 1024 * 1024,
            1,
            cancel,
        )?;
        let measured = provider.clone().with_measured_read_window(window)?;
        let plan = purpose.plan();
        let actual = Arc::new(
            measured.observe_original_metadata_members_v48(
                MetadataMembersObservationV48 {
                    current: session
                        .observation_v1(self.current()?.original().id)
                        .ok_or_else(failure)?,
                    intent: self.opening.parent(),
                    identity: session.identity(),
                    command: plan.command(),
                    limits: read_limits,
                    selected_plan: Some(plan),
                    purpose: if metadata_v62 {
                        crate::providers::gh::MetadataMembersReadPurposeV48::MetadataEligibleV62
                    } else {
                        crate::providers::gh::MetadataMembersReadPurposeV48::Eligible
                    },
                    retained_v62: purpose.retained_v62(),
                },
                cancel,
            )?,
        );
        if !GithubProvider::same_thread_history_parent_v1(self.parent(), actual.parent()) {
            return Err(failure());
        }
        actual.require_mutation_v48()?;
        plan.validate_original_members_v48(&actual)
            .map_err(|message| GithubError {
                kind: GithubErrorKind::InvalidTarget,
                message,
            })?;
        let deadline = deadline.min(actual.deadline()).min(
            captures
                .as_ref()
                .map_or(deadline, |cursor| cursor.deadline()),
        );
        prepare(MetadataMembersPhaseV48 {
            source: self,
            session,
            measured,
            actual,
            cancel,
            deadline,
            captures,
        })
    }
}
impl MetadataMembersPhaseV48<'_> {
    pub(crate) fn actual(&self) -> &Arc<OriginalMetadataMembersV48> {
        &self.actual
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }
    pub(crate) fn local(&self, database: &Path) -> GithubResult<ResourceLocalGuard> {
        ResourceLocalGuard::capture_boundary_v13(
            database,
            self.source.local_boundary(),
            self.actual.identity(),
            self.actual.parent().summary(),
        )
    }
    pub(crate) fn take_captures(&mut self) -> GithubResult<PostWriteCaptureAllowanceV2> {
        self.captures.take().ok_or_else(failure)
    }
    pub(crate) fn finish(
        self,
        guard: impl FnOnce() -> GithubResult<()>,
    ) -> GithubResult<HostIdentity> {
        live(self.cancel, self.deadline)?;
        self.measured.close_original_metadata_members_v48(
            &self.actual,
            self.session
                .observation_v1(self.source.current()?.original().id)
                .ok_or_else(failure)?,
            self.source.opening.parent(),
            self.cancel,
        )?;
        let mut session = self.session;
        session.verify(self.source.current()?.original(), self.cancel)?;
        let closing = session
            .observation_v1(self.source.current()?.original().id)
            .ok_or_else(failure)?
            .repository();
        if closing.archived != Some(false)
            || ![
                closing.permissions.push,
                closing.permissions.maintain,
                closing.permissions.admin,
            ]
            .contains(&Some(true))
        {
            return Err(GithubError {
                kind: GithubErrorKind::Permission,
                message: "Original closing repository metadata write capability unavailable".into(),
            });
        }
        let closed = session.close_with_guard(self.cancel, || {
            live(self.cancel, self.deadline)?;
            guard()
        })?;
        live(self.cancel, self.deadline)?;
        self.measured.validate_original_members_generation_v48(
            &self.actual,
            self.cancel,
            self.deadline,
        )?;
        Ok(closed.identity().clone())
    }
    pub(crate) fn publish(
        self,
        ordinary: &GithubProvider,
        request: MetadataMembersPublicationV48<'_>,
        guard: impl FnOnce() -> GithubResult<()>,
    ) -> Result<PrMetadataMembersReceiptV48, PrMetadataMembersFailureV48> {
        let measured = self.measured.clone();
        let actual = Arc::clone(&self.actual);
        let source = self.source;
        let cancel = self.cancel;
        let deadline = self.deadline;
        let current = source
            .current()
            .map_err(PrMetadataMembersFailureV48::NotStarted)?;
        ordinary.publish_original_metadata_members_v48(
            MembersWriteV48 {
                measured: &measured,
                actual: &actual,
                plan: request.plan,
                current,
                captures: request.captures,
                cancel,
                deadline,
            },
            || {
                let identity = self.finish(guard)?;
                live(cancel, deadline)?;
                request
                    .ack
                    .check_current(
                        request.drafts,
                        request.database,
                        source.opening.scope(),
                        request.key,
                        cancel,
                    )
                    .map_err(|_| failure())?;
                live(cancel, deadline)?;
                Ok(identity)
            },
        )
    }
}
