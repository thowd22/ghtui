//! Purpose-closed facade on the existing original session/Core. This Open is a
//! read-only chooser; only successful real final-UID closure emits its Source.
use super::*;
use crate::pr_issue_links_v87::IssueLinkCommandV87;
pub(crate) struct IssueLinkManualPhaseV94<'a> {
    pub(crate) own: &'a Arc<crate::storage::conversation_refresh_v19::ConversationOwnFenceV19>,
    pub(crate) attempt: Option<
        &'a Arc<crate::storage::conversation_refresh_v19::PersonalConversationReadAttemptV20>,
    >,
}
pub(super) struct IssueLinkManualCoreV94<'a> {
    pub(super) phase: IssueLinkManualPhaseV94<'a>,
    pub(super) descriptor:
        &'a crate::pr_write_journal::conversation_refresh_v19::IssueLinkManualDescriptorV94,
}
pub(crate) fn with_issue_link_manual_refresh_v94<T>(
    request: IssueLinkManualPhaseV94<'_>,
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
    let command = request
        .own
        .terminal()
        .issue_link_command_v87()
        .ok_or_else(failure)?;
    if database != source.opening().database()
        || database != request.own.terminal().database()
        || !source.opening().admits_issue_links_v87(command)
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
    // Only the actual terminal/raw journal lineage can supply this bounded
    // read target. Capture BEFORE any opening Core children; saved Issue
    // fields never construct a Source, permission, receipt or arbitrary read.
    let descriptor =
        crate::pr_write_journal::conversation_refresh_v19::IssueLinkManualDescriptorV94::capture(
            request.own,
        )
        .map_err(|_| failure())?;
    initial_v17::run_issue_link_manual_refresh_v94(
        IssueLinkManualCoreV94 {
            phase: request,
            descriptor: &descriptor,
        },
        provider,
        database,
        cancel,
        deadline,
        prepare,
        validate,
    )
}
use crate::providers::gh::{IssueLinkCatalogueV87, IssueLinkRepositoryChoiceV87};
use crate::providers::gh::{
    IssueLinkChoiceV87, IssueLinkPlanV87, IssueLinkPublicationFailureV87, IssueLinkReadPurposeV87,
    IssueLinkReceiptV87, IssueLinkWriteV87, OriginalIssueLinksV87, OriginalPrMembershipRefV13,
    PostWriteCaptureAllowanceV2,
};

pub(crate) struct IssueLinkOpenV87<'a> {
    pub(crate) opening: Arc<PrCollaborationOpenIntentV17>,
    pub(crate) command: IssueLinkCommandV87,
    pub(crate) repository: Option<&'a IssueLinkRepositoryChoiceV87>,
    pub(crate) max_pages: usize,
}
pub(crate) struct IssueLinkHistoryOpenV87<'a> {
    pub(crate) opening:Arc<PrCollaborationOpenIntentV17>,
    pub(crate) inspect:Option<&'a crate::pr_write_journal::issue_link_origin_v87::history_v87::IssueLinkRecoveryWitnessV87>,
    pub(crate) inherited:usize,
}
pub(crate) fn with_issue_link_history_v87<T>(
    request: IssueLinkHistoryOpenV87<'_>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    prepare: impl FnOnce(
        &Arc<PrResourceParentV1>,
        &HostIdentity,
        PrLocalSourceBoundaryV13<'_>,
        Option<crate::providers::gh::IssueLinkRecoveryCurrentV87>,
    ) -> GithubResult<T>,
    validate: impl Fn(&T, PrLocalSourceBoundaryV13<'_>) -> GithubResult<()>,
) -> GithubResult<(T, HostIdentity)> {
    if !request.opening.admits_issue_link_history_v87() {
        return Err(failure());
    }
    request
        .inherited
        .checked_add(2 * 1024 * 1024)
        .filter(|n| *n <= 4 * 1024 * 1024)
        .ok_or_else(failure)?;
    let inspect = request.inspect.is_some();
    let completed = initial_v17::run_issue_link_history_v87(
        request,
        provider,
        database,
        cancel,
        deadline,
        |mut read| {
            let current = if inspect {
                Some(read.inspect_issue_link_v87()?)
            } else {
                None
            };
            prepare(read.parent, read.identity, read.boundary, current)
        },
        validate,
    )?;
    // Its Core successful close is mandatory, but this read-only wrapper
    // deliberately drops the closed producer: it cannot issue a Source.
    Ok((completed.material, completed.identity))
}
#[cfg(test)]
pub(crate) fn open_issue_links_v87<T>(
    request: IssueLinkOpenV87<'_>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    prepare: impl FnOnce(&Arc<IssueLinkCatalogueV87>, &ResourceLocalGuard) -> GithubResult<T>,
) -> GithubResult<(Arc<PrCollaborationSourceV17>, Arc<IssueLinkCatalogueV87>, T)> {
    match open_issue_links_result_v102(
        IssueOpenRequestV102 {
            request,
            policy: IssueSourceOpenPolicyV102::Legacy,
        },
        provider,
        database,
        cancel,
        deadline,
        prepare,
    )? {
        initial_v17::IssueOpenCoreResultV102::Closed(opened) => Ok(opened),
        initial_v17::IssueOpenCoreResultV102::Unsupported(_) => Err(failure()),
    }
}
pub(crate) type OpenedIssueV102<T> = (Arc<PrCollaborationSourceV17>, Arc<IssueLinkCatalogueV87>, T);
#[derive(Clone, Copy)]
enum IssueSourceOpenPolicyV102 {
    #[cfg(test)]
    Legacy,
    NativeSchema,
}
struct IssueOpenRequestV102<'a> {
    request: IssueLinkOpenV87<'a>,
    policy: IssueSourceOpenPolicyV102,
}
pub(crate) fn open_issue_links_native_v102<T>(
    request: IssueLinkOpenV87<'_>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    prepare: impl FnOnce(&Arc<IssueLinkCatalogueV87>, &ResourceLocalGuard) -> GithubResult<T>,
) -> GithubResult<initial_v17::IssueOpenCoreResultV102<OpenedIssueV102<T>>> {
    open_issue_links_result_v102(
        IssueOpenRequestV102 {
            request,
            policy: IssueSourceOpenPolicyV102::NativeSchema,
        },
        provider,
        database,
        cancel,
        deadline,
        prepare,
    )
}
fn open_issue_links_result_v102<T>(
    input: IssueOpenRequestV102<'_>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    prepare: impl FnOnce(&Arc<IssueLinkCatalogueV87>, &ResourceLocalGuard) -> GithubResult<T>,
) -> GithubResult<initial_v17::IssueOpenCoreResultV102<OpenedIssueV102<T>>> {
    let IssueOpenRequestV102 { request, policy } = input;
    if !request.opening.admits_issue_links_v87(request.command) {
        return Err(failure());
    }
    let opening = Arc::clone(&request.opening);
    let prepare_read = |mut read: initial_v17::InitialReadV17<'_>| {
        let local = ResourceLocalGuard::capture_boundary_v13(
            database,
            read.boundary,
            read.identity,
            read.parent.summary(),
        )?;
        let catalogue = read.take_issue_link_catalogue_v87()?;
        let material = prepare(&catalogue, &local)?;
        Ok((local, catalogue, material))
    };
    let completed = match policy {
        #[cfg(test)]
        IssueSourceOpenPolicyV102::Legacy => {
            initial_v17::IssueOpenCoreResultV102::Closed(initial_v17::run_issue_link_open_v87(
                request,
                provider,
                database,
                cancel,
                deadline,
                prepare_read,
                |(local, _, _), boundary| local.validate_boundary_v13(database, boundary),
            )?)
        }
        IssueSourceOpenPolicyV102::NativeSchema => initial_v17::run_issue_link_open_native_v102(
            request,
            provider,
            database,
            cancel,
            deadline,
            prepare_read,
            |(local, _, _), boundary| local.validate_boundary_v13(database, boundary),
        )?,
    };
    let completed = match completed {
        initial_v17::IssueOpenCoreResultV102::Closed(completed) => completed,
        initial_v17::IssueOpenCoreResultV102::Unsupported(proof) => {
            return Ok(initial_v17::IssueOpenCoreResultV102::Unsupported(proof))
        }
    };
    let (local_recovery_v20, catalogue, material) = completed.material;
    provider.validate_issue_link_catalogue_generation_v87(&catalogue, cancel, deadline)?;
    let source = Arc::new(PrCollaborationSourceV17 {
        opening,
        parent: completed.parent,
        identity: completed.identity,
        closed: completed.closed,
        local_recovery_v20,
    });
    Ok(initial_v17::IssueOpenCoreResultV102::Closed((
        source, catalogue, material,
    )))
}

#[derive(Clone, Copy)]
pub(crate) enum IssueLinkPhasePurposeV87<'a> {
    Preview,
    Confirm(&'a IssueLinkPlanV87),
}
pub(crate) struct IssueLinkPhaseRequestV87<'a> {
    pub(crate) command: IssueLinkCommandV87,
    pub(crate) choice: &'a IssueLinkChoiceV87,
    pub(crate) retained: Option<&'a OriginalIssueLinksV87>,
    pub(crate) purpose: IssueLinkPhasePurposeV87<'a>,
}
pub(crate) struct IssueLinkPhaseV87<'a> {
    source: &'a PrCollaborationSourceV17,
    session: OriginalPrAccessSessionV13<'a>,
    measured: GithubProvider,
    actual: Arc<OriginalIssueLinksV87>,
    plan: IssueLinkPlanV87,
    captures: Option<PostWriteCaptureAllowanceV2>,
    cancel: &'a CancellationToken,
    deadline: Instant,
}
pub(crate) struct IssuePreviewCallbacksV102<R, N> {
    pub(crate) ready: R,
    pub(crate) no_change: N,
}
#[derive(Clone, Copy)]
enum IssuePreviewPolicyV102 {
    Legacy,
    NativePreview,
}
struct IssuePhaseRequestV102<'a> {
    request: IssueLinkPhaseRequestV87<'a>,
    policy: IssuePreviewPolicyV102,
}
pub(crate) struct IssueLinkPublicationV87<'a> {
    pub(crate) captures: &'a mut PostWriteCaptureAllowanceV2,
    pub(crate) ack: &'a crate::drafts::DraftPublicationAckV2,
    pub(crate) drafts: &'a crate::drafts::DraftClient,
    pub(crate) database: &'a Path,
    pub(crate) key: &'a str,
}
/// Only the real phase can move its session into this close step. It carries
/// no public constructor, provider getter, new window or Source factory.
struct IssueLinkCloseV87<'a> {
    source: &'a PrCollaborationSourceV17,
    session: OriginalPrAccessSessionV13<'a>,
    measured: GithubProvider,
    cancel: &'a CancellationToken,
    deadline: Instant,
}
impl PrCollaborationSourceV17 {
    pub(crate) fn with_issue_link_phase_v87<T>(
        &self,
        request: IssueLinkPhaseRequestV87<'_>,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
        prepare: impl FnOnce(IssueLinkPhaseV87<'_>) -> GithubResult<T>,
    ) -> GithubResult<T> {
        self.with_issue_phase_input_v102(
            IssuePhaseRequestV102 {
                request,
                policy: IssuePreviewPolicyV102::Legacy,
            },
            provider,
            database,
            cancel,
            deadline,
            IssuePreviewCallbacksV102 {
                ready: prepare,
                no_change: |_| Err(failure()),
            },
        )
    }
    pub(crate) fn with_issue_link_native_preview_v102<T>(
        &self,
        request: IssueLinkPhaseRequestV87<'_>,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
        callbacks: IssuePreviewCallbacksV102<
            impl FnOnce(IssueLinkPhaseV87<'_>) -> GithubResult<T>,
            impl FnOnce(crate::providers::gh::IssueNoChangeV102) -> GithubResult<T>,
        >,
    ) -> GithubResult<T> {
        if !matches!(request.purpose, IssueLinkPhasePurposeV87::Preview) {
            return Err(failure());
        }
        self.with_issue_phase_input_v102(
            IssuePhaseRequestV102 {
                request,
                policy: IssuePreviewPolicyV102::NativePreview,
            },
            provider,
            database,
            cancel,
            deadline,
            callbacks,
        )
    }
    fn with_issue_phase_input_v102<T>(
        &self,
        input: IssuePhaseRequestV102<'_>,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
        callbacks: IssuePreviewCallbacksV102<
            impl FnOnce(IssueLinkPhaseV87<'_>) -> GithubResult<T>,
            impl FnOnce(crate::providers::gh::IssueNoChangeV102) -> GithubResult<T>,
        >,
    ) -> GithubResult<T> {
        let IssuePhaseRequestV102 { request, policy } = input;
        if database != self.opening.database()
            || !self.opening.admits_issue_links_v87(request.command)
        {
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
        let metadata = self.opening.issue_link_metadata_tab_v87();
        let captures = match request.purpose {
            IssueLinkPhasePurposeV87::Preview => None,
            IssueLinkPhasePurposeV87::Confirm(_) => Some(session.reserve_postwrite_capture_v2(
                if metadata {
                    &crate::providers::gh::ISSUE_LINK_METADATA_POST_CAPS_V87
                } else {
                    &crate::providers::gh::ISSUE_LINK_POST_CAPS_V87
                },
                1,
                cancel,
            )?),
        };
        let window =
            session.reserve_measured_external_reads(8, 512 * 1024, 8 * 1024 * 1024, 1, cancel)?;
        let measured = provider.clone().with_measured_read_window(window)?;
        let actual = measured.observe_original_issue_links_v87(
            crate::providers::gh::IssueLinksSelectedReadV87 {
                current: session
                    .observation_v1(self.current()?.original().id)
                    .ok_or_else(failure)?,
                intent: self.opening.parent(),
                identity: session.identity(),
                command: request.command,
                choice: request.choice,
                purpose: if metadata {
                    IssueLinkReadPurposeV87::MetadataEligibleV87
                } else {
                    IssueLinkReadPurposeV87::Eligible
                },
                retained: request.retained,
            },
            cancel,
        )?;
        if !GithubProvider::same_thread_history_parent_v1(self.parent(), actual.parent()) {
            return Err(failure());
        }
        let plan = match request.purpose {
            IssueLinkPhasePurposeV87::Preview => match policy {
                IssuePreviewPolicyV102::Legacy => actual.prepare_requested()?,
                IssuePreviewPolicyV102::NativePreview => {
                    match actual.prepare_requested_native_v102()? {
                        crate::providers::gh::IssueRequestedV102::Plan(plan) => plan,
                        crate::providers::gh::IssueRequestedV102::NoChange(proof) => {
                            // Failed Preview is not a successful close. The SAME
                            // session-owned measured pool aborts before callback.
                            drop(actual);
                            drop(measured);
                            drop(session);
                            drop(captures);
                            return (callbacks.no_change)(proof);
                        }
                    }
                }
            },
            IssueLinkPhasePurposeV87::Confirm(accepted) => actual.rebind_requested_v87(accepted)?,
        };
        let deadline = deadline.min(actual.deadline()).min(
            captures
                .as_ref()
                .map_or(deadline, PostWriteCaptureAllowanceV2::deadline),
        );
        (callbacks.ready)(IssueLinkPhaseV87 {
            source: self,
            session,
            measured,
            actual,
            plan,
            captures,
            cancel,
            deadline,
        })
    }
}
impl IssueLinkPhaseV87<'_> {
    pub(crate) fn actual(&self) -> &Arc<OriginalIssueLinksV87> {
        &self.actual
    }
    pub(crate) fn plan(&self) -> &IssueLinkPlanV87 {
        &self.plan
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
    ) -> GithubResult<(Arc<OriginalIssueLinksV87>, IssueLinkPlanV87, HostIdentity)> {
        let Self {
            source,
            session,
            measured,
            actual,
            plan,
            cancel,
            deadline,
            ..
        } = self;
        let identity = IssueLinkCloseV87 {
            source,
            session,
            measured,
            cancel,
            deadline,
        }
        .finish(&actual, guard)?;
        Ok((actual, plan, identity))
    }
    pub(crate) fn publish(
        self,
        ordinary: &GithubProvider,
        publication: IssueLinkPublicationV87<'_>,
        guard: impl FnOnce() -> GithubResult<()>,
    ) -> Result<IssueLinkReceiptV87, IssueLinkPublicationFailureV87> {
        self.actual
            .validate_publication_clone_v87(&self.measured, self.cancel)
            .map_err(IssueLinkPublicationFailureV87::NotStarted)?;
        let measured = self.measured.clone();
        let Self {
            source,
            session,
            measured: closing_provider,
            actual,
            plan,
            cancel,
            deadline,
            ..
        } = self;
        let closing = IssueLinkCloseV87 {
            source,
            session,
            measured: closing_provider,
            cancel,
            deadline,
        };
        ordinary.publish_original_issue_link_v87(
            IssueLinkWriteV87 {
                measured: &measured,
                actual: &actual,
                plan: &plan,
                captures: publication.captures,
                cancel,
                deadline,
            },
            || {
                let identity = closing.finish(&actual, guard)?;
                publication
                    .ack
                    .check_current(
                        publication.drafts,
                        publication.database,
                        source.opening.scope(),
                        publication.key,
                        cancel,
                    )
                    .map_err(|_| failure())?;
                live(cancel, deadline)?;
                Ok(identity)
            },
        )
    }
}
impl IssueLinkCloseV87<'_> {
    fn finish(
        self,
        actual: &Arc<OriginalIssueLinksV87>,
        guard: impl FnOnce() -> GithubResult<()>,
    ) -> GithubResult<HostIdentity> {
        live(self.cancel, self.deadline)?;
        self.measured.close_original_issue_links_v87(
            actual,
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
                message: "Original closing PR repository write role unavailable; no Issue mutation"
                    .into(),
            });
        }
        let closed = session.close_with_guard(self.cancel, || {
            live(self.cancel, self.deadline)?;
            guard()
        })?;
        self.measured.validate_original_issue_link_generations_v87(
            actual,
            self.cancel,
            self.deadline,
        )?;
        Ok(closed.identity().clone())
    }
}
