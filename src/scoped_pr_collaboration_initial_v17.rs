//! ONE actual initial Core/parent producer shared by Draft and protected
//! History. Only the Draft wrapper can issue PrCollaborationSourceV17.
use super::*;
pub(crate) struct InitialReadV17<'a> {
    pub parent: &'a Arc<PrResourceParentV1>,
    pub identity: &'a HostIdentity,
    pub boundary: PrLocalSourceBoundaryV13<'a>,
    measured: &'a GithubProvider,
    opening: &'a PrCollaborationOpenIntentV17,
    cancel: &'a CancellationToken,
    effective_cutoff_v51: Instant,
    refresh_v19: Option<crate::providers::gh::ConversationDetailSnapshotV19>,
    metadata_members_v48: Option<Arc<crate::providers::gh::OriginalMetadataMembersV48>>,
    metadata_text_v25: Option<Arc<crate::providers::gh::PrMetadataTextReadV25>>,
    metadata_base_v32: Option<Arc<crate::providers::gh::PrMetadataBaseReadV32>>,
    metadata_section_v51: Option<crate::providers::gh::ActualMetadataTabContextV51>,
    page_supplement_v61: crate::providers::gh::ConversationPageSupplementV61,
    state_v68: Option<Arc<crate::providers::gh::PrStateReadV68>>,
    issue_links_catalogue_v87: Option<Arc<crate::providers::gh::IssueLinkCatalogueV87>>,
    issue_links_inspection_v87: Option<Arc<crate::providers::gh::OriginalIssueLinksV87>>,
}
pub(crate) struct RefreshReadV19<'a> {
    pub identity: &'a HostIdentity,
    pub boundary: PrLocalSourceBoundaryV13<'a>,
    pub snapshot: crate::providers::gh::ConversationDetailSnapshotV19,
    pub page_supplement_v61: crate::providers::gh::ConversationPageSupplementV61,
    pub preparation: RefreshReadPreparationV19,
    pub attempt_v20: Option<
        &'a Arc<crate::storage::conversation_refresh_v19::PersonalConversationReadAttemptV20>,
    >,
}
pub(crate) struct RefreshReadPreparationV19 {
    parent: Arc<PrResourceParentV1>,
    identity: HostIdentity,
    marker: Arc<()>,
    cutoff: Instant,
}
impl RefreshReadPreparationV19 {
    pub(crate) fn cutoff_v51(&self) -> Instant {
        self.cutoff
    }
    pub(crate) fn parent(&self) -> &Arc<PrResourceParentV1> {
        &self.parent
    }
    pub(crate) fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub(crate) fn marker_v19(&self) -> std::sync::Weak<()> {
        Arc::downgrade(&self.marker)
    }
}
pub(crate) struct ClosedRefreshReadV19<T> {
    material: T,
    identity: HostIdentity,
    marker: Arc<()>,
    cutoff: Instant,
}
pub(crate) struct RefreshReadCloseSealV19(Arc<()>, Instant);
impl<T> ClosedRefreshReadV19<T> {
    pub(crate) fn into_material_v19(self) -> (T, HostIdentity, RefreshReadCloseSealV19) {
        (
            self.material,
            self.identity,
            RefreshReadCloseSealV19(self.marker, self.cutoff),
        )
    }
}
impl RefreshReadCloseSealV19 {
    pub(crate) fn cutoff_v51(&self) -> Instant {
        self.1
    }
    pub(crate) fn matches_v19(&self, marker: &std::sync::Weak<()>) -> bool {
        marker.ptr_eq(&Arc::downgrade(&self.0))
    }
}
#[derive(Clone, Copy)]
enum IssueOpenPolicyV102 {
    #[cfg(test)]
    Legacy,
    NativeSchema,
}
pub(crate) enum IssueOpenCoreResultV102<T> {
    Closed(T),
    Unsupported(crate::providers::gh::IssueHostUnsupportedV102),
}
struct CoreInputV102<'a> {
    opening: Arc<PrCollaborationOpenIntentV17>,
    initial: bool,
    purpose: PurposeV19<'a>,
    provider: &'a GithubProvider,
    database: &'a Path,
    cancel: &'a CancellationToken,
    deadline: Instant,
}
#[derive(Clone, Copy)]
enum PurposeV19<'a> {
    Existing,
    StateV68(
        crate::pr_state_model::PrStateCommand,
        Option<&'a crate::storage::StateAckQueueBoundaryV78>,
    ),
    MetadataSectionV51,
    MetadataTextV25(crate::providers::gh::PrMetadataTextFieldV25),
    MetadataMembersV48(
        crate::pr_metadata_actions::PrMetadataCommand,
        &'a crate::providers::gh::PrMetadataReadLimits,
    ),
    MetadataMembersInspectionV48(crate::pr_metadata_actions::PrMetadataCommand),
    MetadataMembersListV48,
    IssueLinksOpenV87 {
        command: crate::pr_issue_links_v87::IssueLinkCommandV87,
        repository: Option<&'a crate::providers::gh::IssueLinkRepositoryChoiceV87>,
        max_pages: usize,
        policy: IssueOpenPolicyV102,
    },
    IssueLinksHistoryV87(Option<&'a crate::pr_write_journal::issue_link_origin_v87::history_v87::IssueLinkRecoveryWitnessV87>,usize),
    MetadataBaseV32(Option<crate::providers::gh::BaseCatalogueLimitsV32>),
    RefreshExact,
    RefreshStateV68 {
        own: &'a Arc<crate::storage::conversation_refresh_v19::ConversationOwnFenceV19>,
        attempt: Option<
            &'a Arc<crate::storage::conversation_refresh_v19::PersonalConversationReadAttemptV20>,
        >,
    },
    RefreshMetadataTabV51(
        Option<
            &'a Arc<crate::storage::conversation_refresh_v19::PersonalConversationReadAttemptV20>,
        >,
    ),
    RefreshPersonalV20(
        &'a Arc<crate::storage::conversation_refresh_v19::PersonalConversationReadAttemptV20>,
    ),
    RefreshBaseV36 {
        own: &'a Arc<crate::storage::conversation_refresh_v19::ConversationOwnFenceV19>,
        attempt: Option<
            &'a Arc<crate::storage::conversation_refresh_v19::PersonalConversationReadAttemptV20>,
        >,
    },
    RefreshBaseMetadataV51 {
        own: &'a Arc<crate::storage::conversation_refresh_v19::ConversationOwnFenceV19>,
        attempt: Option<
            &'a Arc<crate::storage::conversation_refresh_v19::PersonalConversationReadAttemptV20>,
        >,
    },
    RefreshMembersV48 {
        own: &'a Arc<crate::storage::conversation_refresh_v19::ConversationOwnFenceV19>,
        attempt: Option<
            &'a Arc<crate::storage::conversation_refresh_v19::PersonalConversationReadAttemptV20>,
        >,
    },
    RefreshIssueLinksV94 {
        own: &'a Arc<crate::storage::conversation_refresh_v19::ConversationOwnFenceV19>,
        attempt: Option<
            &'a Arc<crate::storage::conversation_refresh_v19::PersonalConversationReadAttemptV20>,
        >,
        descriptor: &'a crate::pr_write_journal::conversation_refresh_v19::IssueLinkManualDescriptorV94,
    },
}
impl InitialReadV17<'_> {
    pub(super) fn state_action_base_v68(
        &self,
        actual: &crate::providers::gh::OriginalPrStateV68,
    ) -> GithubResult<crate::pr_write_journal::state_origin_v68::lineage_v68::StateActionBaseV68>
    {
        actual.require()?;
        if !self.opening.admits_state_v68(actual.observed().command()) {
            return Err(failure());
        }
        crate::pr_write_journal::state_origin_v68::lineage_v68::StateActionBaseV68::from_open_read(
            self, actual,
        )
        .map_err(|_| failure())
    }
    pub(super) fn observe_state_v68(
        &mut self,
    ) -> GithubResult<crate::providers::gh::OriginalPrStateV68> {
        self.measured.observe_original_state_v68(
            self.state_v68.take().ok_or_else(failure)?,
            self.identity,
            self.cancel,
        )
    }
    pub(super) fn inspect_state_v74(
        &mut self,
    ) -> GithubResult<crate::providers::gh::StateRecoveryCurrentV68> {
        let actual = Arc::new(self.observe_state_v68()?);
        self.measured
            .inspect_original_state_v68(actual, self.cancel)
    }
    pub(super) fn inspect_issue_link_v87(
        &mut self,
    ) -> GithubResult<crate::providers::gh::IssueLinkRecoveryCurrentV87> {
        self.measured.inspect_original_issue_link_v87(
            self.issue_links_inspection_v87.take().ok_or_else(failure)?,
            self.cancel,
        )
    }
    pub(super) fn take_issue_link_catalogue_v87(
        &mut self,
    ) -> GithubResult<Arc<crate::providers::gh::IssueLinkCatalogueV87>> {
        self.issue_links_catalogue_v87.take().ok_or_else(failure)
    }
    pub(super) fn take_metadata_section_v51(
        &mut self,
    ) -> GithubResult<crate::providers::gh::ActualMetadataTabContextV51> {
        self.metadata_section_v51.take().ok_or_else(failure)
    }
    pub(super) fn effective_cutoff_v51(&self) -> Instant {
        self.effective_cutoff_v51
    }
    pub(super) fn inspect_metadata_members_v48(
        &mut self,
    ) -> GithubResult<crate::providers::gh::MetadataMembersRecoveryCurrentV48> {
        let actual = self.take_metadata_members_v48()?;
        self.measured
            .inspect_original_metadata_members_v48(actual, self.cancel)
    }
    pub(super) fn take_metadata_members_v48(
        &mut self,
    ) -> GithubResult<Arc<crate::providers::gh::OriginalMetadataMembersV48>> {
        self.metadata_members_v48.take().ok_or_else(failure)
    }
    pub(super) fn inspect_base_v32(
        &mut self,
    ) -> GithubResult<crate::providers::gh::BaseRecoveryCurrentV32> {
        let actual = self.observe_metadata_base_v32()?;
        self.measured.inspect_original_base_v32(actual, self.cancel)
    }
    pub(super) fn observe_metadata_base_v32(
        &mut self,
    ) -> GithubResult<crate::providers::gh::OriginalMetadataBaseV32> {
        self.measured.observe_original_metadata_base_v32(
            self.metadata_base_v32.take().ok_or_else(failure)?,
            self.identity,
            self.cancel,
        )
    }
    pub(super) fn base_catalogue_v32(
        &mut self,
        limits: crate::providers::gh::BaseCatalogueLimitsV32,
    ) -> GithubResult<Arc<crate::providers::gh::BaseCatalogueV32>> {
        let actual = Arc::new(self.observe_metadata_base_v32()?);
        self.measured
            .metadata_base_catalogue_v32(actual, limits, self.cancel)
    }
    pub(super) fn inspect_metadata_v25(
        &mut self,
    ) -> GithubResult<crate::providers::gh::MetadataRecoveryCurrentV25> {
        let actual = self.observe_metadata_text_v25()?;
        self.measured
            .inspect_original_metadata_v25(actual, self.cancel)
    }
    pub(super) fn observe_metadata_text_v25(
        &mut self,
    ) -> GithubResult<crate::providers::gh::OriginalMetadataTextV25> {
        let actual = self.metadata_text_v25.take().ok_or_else(failure)?;
        self.measured
            .observe_original_metadata_text_v25(actual, self.identity, self.cancel)
    }
    /// Data-only original database name, never access or a transport escape.
    /// Used by the closed action Base factory while this real read is live.
    pub(crate) fn original_database_v18(&self) -> &Path {
        self.opening.database()
    }
    pub(crate) fn action_base_v18(
        &self,
        command: PrCommand,
        observed: &ConversationOpenObservationV17,
    ) -> GithubResult<
        crate::pr_write_journal::conversation_action_lineage_v18::ConversationActionBaseV18,
    > {
        // Preserve the ACTUAL typed capability refusal before the structural
        // anyhow Base validator erases error kinds. Permission is not account
        // loss and must not retire an already-admitted private READ page.
        // Base keeps its own duplicate requirement as defense in depth.
        match observed {
            ConversationOpenObservationV17::Comment(comment) => comment.require(command)?,
            ConversationOpenObservationV17::Reactions(reactions) => {
                reactions.original().require(command)?;
            }
            _ => {}
        }
        crate::pr_write_journal::conversation_action_lineage_v18::ConversationActionBaseV18::from_open_read(self, command, observed)
            .map_err(|_| failure())
    }
    pub(super) fn observe(
        &self,
        command: PrCommand,
    ) -> GithubResult<ConversationOpenObservationV17> {
        Ok(match command {
            PrCommand::NewComment => ConversationOpenObservationV17::New,
            PrCommand::Subscription => ConversationOpenObservationV17::Subscription(
                self.measured.observe_conversation_subscription_v17(
                    self.parent.clone(),
                    self.identity,
                    self.cancel,
                )?,
            ),
            PrCommand::EditComment
            | PrCommand::DeleteComment
            | PrCommand::AddReaction
            | PrCommand::RemoveReaction => {
                let comment = self.measured.observe_conversation_comment_v17(
                    self.parent.clone(),
                    self.identity,
                    self.opening.selected_comment().ok_or_else(failure)?,
                    self.cancel,
                )?;
                if matches!(command, PrCommand::AddReaction | PrCommand::RemoveReaction) {
                    ConversationOpenObservationV17::Reactions(
                        self.measured
                            .observe_conversation_reactions_v17(comment, self.cancel)?,
                    )
                } else {
                    ConversationOpenObservationV17::Comment(comment)
                }
            }
            _ => return Err(failure()),
        })
    }
    pub(crate) fn inspect(
        &self,
        seed: &crate::pr_write_journal::conversation_history_v17::ConversationOriginSeedV17,
    ) -> GithubResult<crate::providers::gh::ConversationRecoveryCurrentV17> {
        self.measured.inspect_conversation_outcome_v17(
            self.parent.clone(),
            self.identity,
            seed,
            self.cancel,
        )
    }
}
pub(super) struct CompletedV17<T> {
    pub material: T,
    pub parent: Arc<PrResourceParentV1>,
    pub identity: HostIdentity,
    pub closed: ClosedOriginV17,
    effective_cutoff_v51: Instant,
}
// This private boundary keeps original deadline, source, initial-token rule,
// preparation and typed final SQL witness distinct. It is not a provider API.
#[expect(
    clippy::too_many_arguments,
    reason = "One private caller-bracketed original Core; no replacement session or purpose fallback"
)]
pub(super) fn run<T>(
    opening: Arc<PrCollaborationOpenIntentV17>,
    initial: bool,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    prepare: impl FnOnce(InitialReadV17<'_>) -> GithubResult<T>,
    validate: impl Fn(&T, PrLocalSourceBoundaryV13<'_>) -> GithubResult<()>,
) -> GithubResult<CompletedV17<T>> {
    if !opening.admits_conversation_v51() {
        return Err(failure());
    }
    run_purpose_v19(
        opening,
        initial,
        PurposeV19::Existing,
        provider,
        database,
        cancel,
        deadline,
        prepare,
        validate,
    )
}
pub(super) struct StateReadRequestV68<'a> {
    pub(super) opening: Arc<PrCollaborationOpenIntentV17>,
    pub(super) command: crate::pr_state_model::PrStateCommand,
    pub(super) initial: bool,
    pub(super) inspection_cache_v78: Option<&'a crate::storage::StateAckQueueBoundaryV78>,
}
pub(super) fn run_state_v68<T>(
    request: StateReadRequestV68<'_>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    prepare: impl FnOnce(InitialReadV17<'_>) -> GithubResult<T>,
    validate: impl Fn(&T, PrLocalSourceBoundaryV13<'_>) -> GithubResult<()>,
) -> GithubResult<CompletedV17<T>> {
    if !request.opening.admits_state_v68(request.command) {
        return Err(failure());
    }
    run_purpose_v19(
        request.opening,
        request.initial,
        PurposeV19::StateV68(request.command, request.inspection_cache_v78),
        provider,
        database,
        cancel,
        deadline,
        prepare,
        validate,
    )
}
#[expect(
    clippy::too_many_arguments,
    reason = "Same original initial Core; closed metadata purpose preserves exact caller preparation and final SQL witness"
)]
pub(super) fn run_metadata_text_v25<T>(
    opening: Arc<PrCollaborationOpenIntentV17>,
    initial: bool,
    field: crate::providers::gh::PrMetadataTextFieldV25,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    prepare: impl FnOnce(InitialReadV17<'_>) -> GithubResult<T>,
    validate: impl Fn(&T, PrLocalSourceBoundaryV13<'_>) -> GithubResult<()>,
) -> GithubResult<CompletedV17<T>> {
    if !opening.admits_metadata_field_v51(field) {
        return Err(failure());
    }
    run_purpose_v19(
        opening,
        initial,
        PurposeV19::MetadataTextV25(field),
        provider,
        database,
        cancel,
        deadline,
        prepare,
        validate,
    )
}
pub(super) fn run_metadata_members_v48<T>(
    request: super::metadata_members_phase_v48::MetadataMembersOpenV48<'_>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    prepare: impl FnOnce(InitialReadV17<'_>) -> GithubResult<T>,
    validate: impl Fn(&T, PrLocalSourceBoundaryV13<'_>) -> GithubResult<()>,
) -> GithubResult<CompletedV17<T>> {
    run_purpose_v19(
        request.opening,
        true,
        PurposeV19::MetadataMembersV48(request.command, request.limits),
        provider,
        database,
        cancel,
        deadline,
        prepare,
        validate,
    )
}
#[cfg(test)]
pub(super) fn run_issue_link_open_v87<T>(
    request: super::issue_links_phase_v87::IssueLinkOpenV87<'_>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    prepare: impl FnOnce(InitialReadV17<'_>) -> GithubResult<T>,
    validate: impl Fn(&T, PrLocalSourceBoundaryV13<'_>) -> GithubResult<()>,
) -> GithubResult<CompletedV17<T>> {
    run_purpose_v19(
        request.opening,
        true,
        PurposeV19::IssueLinksOpenV87 {
            command: request.command,
            repository: request.repository,
            max_pages: request.max_pages,
            policy: IssueOpenPolicyV102::Legacy,
        },
        provider,
        database,
        cancel,
        deadline,
        prepare,
        validate,
    )
}
pub(super) fn run_issue_link_open_native_v102<T>(
    request: super::issue_links_phase_v87::IssueLinkOpenV87<'_>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    prepare: impl FnOnce(InitialReadV17<'_>) -> GithubResult<T>,
    validate: impl Fn(&T, PrLocalSourceBoundaryV13<'_>) -> GithubResult<()>,
) -> GithubResult<IssueOpenCoreResultV102<CompletedV17<T>>> {
    run_purpose_result_v102(
        CoreInputV102 {
            opening: request.opening,
            initial: true,
            purpose: PurposeV19::IssueLinksOpenV87 {
                command: request.command,
                repository: request.repository,
                max_pages: request.max_pages,
                policy: IssueOpenPolicyV102::NativeSchema,
            },
            provider,
            database,
            cancel,
            deadline,
        },
        prepare,
        validate,
    )
}
pub(super) fn run_issue_link_history_v87<T>(
    request: super::issue_links_phase_v87::IssueLinkHistoryOpenV87<'_>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    prepare: impl FnOnce(InitialReadV17<'_>) -> GithubResult<T>,
    validate: impl Fn(&T, PrLocalSourceBoundaryV13<'_>) -> GithubResult<()>,
) -> GithubResult<CompletedV17<T>> {
    run_purpose_v19(
        request.opening,
        false,
        PurposeV19::IssueLinksHistoryV87(request.inspect, request.inherited),
        provider,
        database,
        cancel,
        deadline,
        prepare,
        validate,
    )
}
pub(super) fn run_metadata_members_history_v48<T>(
    request: super::metadata_members_phase_v48::MetadataMembersHistoryOpenV48,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    prepare: impl FnOnce(InitialReadV17<'_>) -> GithubResult<T>,
    validate: impl Fn(&T, PrLocalSourceBoundaryV13<'_>) -> GithubResult<()>,
) -> GithubResult<CompletedV17<T>> {
    let purpose = request.inspect.map_or(
        PurposeV19::MetadataMembersListV48,
        PurposeV19::MetadataMembersInspectionV48,
    );
    run_purpose_v19(
        request.opening,
        false,
        purpose,
        provider,
        database,
        cancel,
        deadline,
        prepare,
        validate,
    )
}
pub(super) fn run_metadata_section_v51<T>(
    opening: Arc<PrCollaborationOpenIntentV17>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    prepare: impl FnOnce(InitialReadV17<'_>) -> GithubResult<T>,
    validate: impl Fn(&T, PrLocalSourceBoundaryV13<'_>) -> GithubResult<()>,
) -> GithubResult<CompletedV17<T>> {
    if !opening.admits_conversation_v51() {
        return Err(failure());
    }
    run_purpose_v19(
        opening,
        true,
        PurposeV19::MetadataSectionV51,
        provider,
        database,
        cancel,
        deadline,
        prepare,
        validate,
    )
}
#[expect(
    clippy::too_many_arguments,
    reason = "Same initial Core and purpose-closed bounded branch catalogue; exact original SQL preparation/close"
)]
pub(super) fn run_metadata_base_catalogue_v32<T>(
    opening: Arc<PrCollaborationOpenIntentV17>,
    limits: crate::providers::gh::BaseCatalogueLimitsV32,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    prepare: impl FnOnce(InitialReadV17<'_>) -> GithubResult<T>,
    validate: impl Fn(&T, PrLocalSourceBoundaryV13<'_>) -> GithubResult<()>,
) -> GithubResult<CompletedV17<T>> {
    limits.validate()?;
    if !opening.admits_metadata_base_v51() {
        return Err(failure());
    }
    run_purpose_v19(
        opening,
        true,
        PurposeV19::MetadataBaseV32(Some(limits)),
        provider,
        database,
        cancel,
        deadline,
        prepare,
        validate,
    )
}
/// Explicit NEW read phase only; no source issuance, draft/revision/ACK,
/// publication allowance or old exhausted Core/token renewal.
pub(super) fn run_base_history_v32<T>(
    opening: Arc<PrCollaborationOpenIntentV17>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    prepare: impl FnOnce(InitialReadV17<'_>) -> GithubResult<T>,
    validate: impl Fn(&T, PrLocalSourceBoundaryV13<'_>) -> GithubResult<()>,
) -> GithubResult<CompletedV17<T>> {
    if !opening.admits_metadata_base_v51() {
        return Err(failure());
    }
    run_purpose_v19(
        opening,
        false,
        PurposeV19::MetadataBaseV32(None),
        provider,
        database,
        cancel,
        deadline,
        prepare,
        validate,
    )
}
pub(super) fn run_refresh_v19<T>(
    opening: Arc<PrCollaborationOpenIntentV17>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    prepare: impl FnOnce(RefreshReadV19<'_>) -> GithubResult<T>,
    validate: impl Fn(&T, PrLocalSourceBoundaryV13<'_>) -> GithubResult<()>,
) -> GithubResult<ClosedRefreshReadV19<T>> {
    run_refresh_attempt_v20(
        opening, provider, database, cancel, deadline, None, prepare, validate,
    )
}
pub(super) fn run_issue_link_manual_refresh_v94<T>(
    request: super::issue_links_phase_v87::IssueLinkManualCoreV94<'_>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    prepare: impl FnOnce(RefreshReadV19<'_>) -> GithubResult<T>,
    validate: impl Fn(&T, PrLocalSourceBoundaryV13<'_>) -> GithubResult<()>,
) -> GithubResult<ClosedRefreshReadV19<T>> {
    let super::issue_links_phase_v87::IssueLinkManualCoreV94 {
        phase: request,
        descriptor,
    } = request;
    run_refresh_purpose_v36(
        Arc::clone(request.own.source().opening()),
        provider,
        database,
        cancel,
        deadline,
        PurposeV19::RefreshIssueLinksV94 {
            own: request.own,
            attempt: request.attempt,
            descriptor,
        },
        request.attempt,
        prepare,
        validate,
    )
}

pub(super) struct StateRefreshReadV68<'a> {
    pub(super) own: &'a Arc<crate::storage::conversation_refresh_v19::ConversationOwnFenceV19>,
    pub(super) attempt: Option<
        &'a Arc<crate::storage::conversation_refresh_v19::PersonalConversationReadAttemptV20>,
    >,
}
pub(super) fn run_state_refresh_v68<T>(
    request: StateRefreshReadV68<'_>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    prepare: impl FnOnce(RefreshReadV19<'_>) -> GithubResult<T>,
    validate: impl Fn(&T, PrLocalSourceBoundaryV13<'_>) -> GithubResult<()>,
) -> GithubResult<ClosedRefreshReadV19<T>> {
    run_refresh_purpose_v36(
        Arc::clone(request.own.source().opening()),
        provider,
        database,
        cancel,
        deadline,
        PurposeV19::RefreshStateV68 {
            own: request.own,
            attempt: request.attempt,
        },
        request.attempt,
        prepare,
        validate,
    )
}
pub(super) fn run_metadata_members_manual_refresh_v48<T>(
    request: super::metadata_members_phase_v48::MetadataMembersManualPhaseV48<'_>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    prepare: impl FnOnce(RefreshReadV19<'_>) -> GithubResult<T>,
    validate: impl Fn(&T, PrLocalSourceBoundaryV13<'_>) -> GithubResult<()>,
) -> GithubResult<ClosedRefreshReadV19<T>> {
    run_refresh_purpose_v36(
        Arc::clone(request.own.source().opening()),
        provider,
        database,
        cancel,
        deadline,
        PurposeV19::RefreshMembersV48 {
            own: request.own,
            attempt: request.attempt,
        },
        request.attempt,
        prepare,
        validate,
    )
}
#[expect(
    clippy::too_many_arguments,
    reason = "Same closed explicit read, optional ACTUAL Personal reservation attempt; no provider escape"
)]
pub(super) fn run_refresh_attempt_v20<T>(
    opening: Arc<PrCollaborationOpenIntentV17>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    attempt: Option<
        &Arc<crate::storage::conversation_refresh_v19::PersonalConversationReadAttemptV20>,
    >,
    prepare: impl FnOnce(RefreshReadV19<'_>) -> GithubResult<T>,
    validate: impl Fn(&T, PrLocalSourceBoundaryV13<'_>) -> GithubResult<()>,
) -> GithubResult<ClosedRefreshReadV19<T>> {
    run_refresh_purpose_v36(
        opening,
        provider,
        database,
        cancel,
        deadline,
        attempt.map_or(PurposeV19::RefreshExact, PurposeV19::RefreshPersonalV20),
        attempt,
        prepare,
        validate,
    )
}
#[expect(
    clippy::too_many_arguments,
    reason = "Same actual Base terminal/attempt and existing read preparation/final SQL callback"
)]
pub(super) fn run_base_refresh_v36<T>(
    opening: Arc<PrCollaborationOpenIntentV17>,
    own: &Arc<crate::storage::conversation_refresh_v19::ConversationOwnFenceV19>,
    attempt: Option<
        &Arc<crate::storage::conversation_refresh_v19::PersonalConversationReadAttemptV20>,
    >,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    prepare: impl FnOnce(RefreshReadV19<'_>) -> GithubResult<T>,
    validate: impl Fn(&T, PrLocalSourceBoundaryV13<'_>) -> GithubResult<()>,
) -> GithubResult<ClosedRefreshReadV19<T>> {
    let purpose = if opening.metadata_tab_v51() {
        PurposeV19::RefreshBaseMetadataV51 { own, attempt }
    } else {
        PurposeV19::RefreshBaseV36 { own, attempt }
    };
    run_refresh_purpose_v36(
        opening, provider, database, cancel, deadline, purpose, attempt, prepare, validate,
    )
}
#[expect(
    clippy::too_many_arguments,
    reason = "Private closed normal-or-Base read purpose; no arbitrary expected parent or source constructor"
)]
fn run_refresh_purpose_v36<T>(
    opening: Arc<PrCollaborationOpenIntentV17>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    purpose: PurposeV19<'_>,
    attempt: Option<
        &Arc<crate::storage::conversation_refresh_v19::PersonalConversationReadAttemptV20>,
    >,
    prepare: impl FnOnce(RefreshReadV19<'_>) -> GithubResult<T>,
    validate: impl Fn(&T, PrLocalSourceBoundaryV13<'_>) -> GithubResult<()>,
) -> GithubResult<ClosedRefreshReadV19<T>> {
    let marker = Arc::new(());
    let completed = run_purpose_v19(
        opening,
        false,
        purpose,
        provider,
        database,
        cancel,
        deadline,
        |mut read| {
            let snapshot = read.refresh_v19.take().ok_or_else(failure)?;
            let page_supplement_v61 = match read.metadata_section_v51.take() {
                Some(context) => {
                    if !matches!(
                        read.page_supplement_v61,
                        crate::providers::gh::ConversationPageSupplementV61::None
                    ) {
                        return Err(failure());
                    }
                    crate::providers::gh::ConversationPageSupplementV61::metadata_v79(
                        &snapshot, context,
                    )?
                }
                None => std::mem::replace(
                    &mut read.page_supplement_v61,
                    crate::providers::gh::ConversationPageSupplementV61::None,
                ),
            };
            prepare(RefreshReadV19 {
                identity: read.identity,
                boundary: read.boundary,
                snapshot,
                page_supplement_v61,
                preparation: RefreshReadPreparationV19 {
                    parent: Arc::clone(read.parent),
                    identity: read.identity.clone(),
                    marker: Arc::clone(&marker),
                    cutoff: read.effective_cutoff_v51,
                },
                attempt_v20: attempt,
            })
        },
        validate,
    )?;
    Ok(ClosedRefreshReadV19 {
        material: completed.material,
        identity: completed.identity,
        marker,
        cutoff: completed.effective_cutoff_v51,
    })
}
#[expect(
    clippy::too_many_arguments,
    reason = "One closed actual Metadata terminal/current Personal attempt and same new Core/read closure"
)]
pub(super) fn run_metadata_refresh_v51<T>(
    own: &Arc<crate::storage::conversation_refresh_v19::ConversationOwnFenceV19>,
    attempt: Option<
        &Arc<crate::storage::conversation_refresh_v19::PersonalConversationReadAttemptV20>,
    >,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    prepare: impl FnOnce(RefreshReadV19<'_>) -> GithubResult<T>,
    validate: impl Fn(&T, PrLocalSourceBoundaryV13<'_>) -> GithubResult<()>,
) -> GithubResult<ClosedRefreshReadV19<T>> {
    own.terminal()
        .validate_metadata_tab_read_v51(own.source())
        .map_err(|_| failure())?;
    if let Some(attempt) = attempt {
        if !attempt.matches_own(own) || attempt.matches_phase(cancel, deadline).is_err() {
            return Err(failure());
        }
    } else {
        own.validate_before_read(cancel, deadline)
            .map_err(|_| failure())?;
    }
    run_refresh_purpose_v36(
        own.source().opening().clone(),
        provider,
        database,
        cancel,
        deadline,
        PurposeV19::RefreshMetadataTabV51(attempt),
        attempt,
        prepare,
        validate,
    )
}
#[expect(
    clippy::too_many_arguments,
    reason = "One existing original producer, closed existing-vs-read-refresh purpose and exact caller preparation/local validation; no second Core"
)]
fn run_purpose_v19<T>(
    opening: Arc<PrCollaborationOpenIntentV17>,
    initial: bool,
    purpose: PurposeV19<'_>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    prepare: impl FnOnce(InitialReadV17<'_>) -> GithubResult<T>,
    validate: impl Fn(&T, PrLocalSourceBoundaryV13<'_>) -> GithubResult<()>,
) -> GithubResult<CompletedV17<T>> {
    match run_purpose_result_v102(CoreInputV102{opening,initial,purpose,provider,database,cancel,deadline},prepare,validate)? {
        IssueOpenCoreResultV102::Closed(completed)=>Ok(completed),
        IssueOpenCoreResultV102::Unsupported(_)=>Err(GithubError{kind:GithubErrorKind::Unsupported,message:"Original manual Issue-link observation unavailable; no substituted target or resend".into()}),
    }
}
fn run_purpose_result_v102<T>(
    input: CoreInputV102<'_>,
    prepare: impl FnOnce(InitialReadV17<'_>) -> GithubResult<T>,
    validate: impl Fn(&T, PrLocalSourceBoundaryV13<'_>) -> GithubResult<()>,
) -> GithubResult<IssueOpenCoreResultV102<CompletedV17<T>>> {
    let CoreInputV102 {
        opening,
        initial,
        purpose,
        provider,
        database,
        cancel,
        deadline,
    } = input;
    if database != opening.database()
        || initial
            && (!cancel.same_instance(opening.cancellation()) || deadline > opening.deadline())
    {
        return Err(failure());
    }
    let deadline = deadline.min(
        provider
            .original_pr_read_deadline_v13()
            .ok_or_else(failure)?,
    );
    live(cancel, deadline)?;
    if let PurposeV19::StateV68(_, Some(original_cache)) = purpose {
        original_cache
            .validate_at_database_v78(database)
            .map_err(|_| failure())?;
    }
    let provider = provider
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)?;
    let mut limits = RepositoryCatalogueAccessLimits::default();
    limits.transport.max_document_bytes = 128 * 1024;
    limits.transport.max_response_bytes = 8 * 1024 * 1024;
    limits.max_repository_reads = 2;
    let mut session = if let Some(source) = opening.catalogue_source() {
        let mut session = provider.begin_original_pr_access_v13(
            crate::providers::gh::OriginalPrMembershipRefV13::Catalogue(source.source()),
            &limits,
            cancel,
        )?;
        session.verify(
            opening
                .catalogue_navigation()
                .ok_or_else(failure)?
                .original(),
            cancel,
        )?;
        SessionV17::Catalogue(session)
    } else {
        SessionV17::Personal(provider.begin_personal_pr_access_v13(
            opening.personal().ok_or_else(failure)?.parent(),
            &limits,
            cancel,
        )?)
    };
    if crate::storage::Scope::github(&session.identity().host, session.identity().account_id)
        .map_err(|_| failure())?
        != *opening.scope()
    {
        return Err(failure());
    }
    let measured = provider.clone().with_measured_read_window(match purpose {
        PurposeV19::Existing => session.reserve(cancel)?,
        PurposeV19::StateV68(_, _) => session.reserve_state_v68(cancel)?,
        PurposeV19::MetadataSectionV51 => session.reserve_metadata_section_v51(cancel)?,
        PurposeV19::RefreshMetadataTabV51(_) => session.reserve_metadata_refresh_v51(cancel)?,
        PurposeV19::RefreshBaseMetadataV51 { .. } => {
            session.reserve_metadata_refresh_v51(cancel)?
        }
        PurposeV19::MetadataMembersV48(_, limits) => {
            session.reserve_metadata_members_v48(limits, cancel)?
        }
        PurposeV19::MetadataMembersInspectionV48(_) => {
            session.reserve_metadata_members_inspection_v48(cancel)?
        }
        PurposeV19::MetadataMembersListV48 => session.reserve_metadata_members_list_v48(cancel)?,
        PurposeV19::IssueLinksOpenV87 { max_pages, .. } => {
            session.reserve_issue_link_open_v87(max_pages, cancel)?
        }
        PurposeV19::IssueLinksHistoryV87(Some(_), _) => {
            session.reserve_issue_link_inspection_v87(cancel)?
        }
        PurposeV19::IssueLinksHistoryV87(None, _) => {
            session.reserve_metadata_members_list_v48(cancel)?
        }
        PurposeV19::RefreshIssueLinksV94 { own, .. } => session.reserve_issue_link_manual_v94(
            own.source().opening().issue_link_metadata_tab_v87(),
            cancel,
        )?,
        PurposeV19::RefreshMembersV48 { own, .. } => {
            if opening
                .metadata_members_tab_v61(own.terminal().members_command_v48().ok_or_else(failure)?)
            {
                session.reserve_metadata_members_tab_manual_v62(cancel)?
            } else {
                session.reserve_metadata_members_manual_v48(cancel)?
            }
        }
        PurposeV19::MetadataTextV25(_) => session.reserve_metadata_text_v25(cancel)?,
        PurposeV19::MetadataBaseV32(limits) => session.reserve_metadata_base_v32(
            limits.map_or(3, |limits| limits.measured_requests()),
            cancel,
        )?,
        PurposeV19::RefreshExact
        | PurposeV19::RefreshStateV68 { .. }
        | PurposeV19::RefreshPersonalV20(_)
        | PurposeV19::RefreshBaseV36 { .. } => session.reserve_refresh_v19(cancel)?,
    })?;
    // Every following local reservation/descriptor/close uses the actual
    // carved original deadline, not the wider request upper bound.
    let deadline = deadline.min(
        measured
            .original_pr_read_deadline_v13()
            .ok_or_else(failure)?,
    );
    let base_refresh_v36 = match purpose {
        PurposeV19::RefreshBaseV36 { own, attempt }
        | PurposeV19::RefreshBaseMetadataV51 { own, attempt } => {
            Some(measured.begin_original_base_refresh_read_v36(own, attempt, cancel)?)
        }
        _ => None,
    };
    let mut members_refresh_v48 = match purpose {
        PurposeV19::RefreshMembersV48 { own, .. } => {
            Some(measured.begin_members_manual_refresh_v48(
                session.current(&opening)?,
                own,
                session.identity(),
                cancel,
            )?)
        }
        _ => None,
    };
    let mut issue_refresh_v94 = match purpose {
        PurposeV19::RefreshIssueLinksV94 { descriptor, .. } => {
            Some(measured.begin_issue_link_manual_read_v94(
                session.current(&opening)?,
                descriptor,
                session.identity(),
                cancel,
            )?)
        }
        _ => None,
    };
    let raw_summary = match purpose {
        PurposeV19::RefreshStateV68 { own, attempt } => {
            Some(measured.pr_state_refresh_summary_v68(
                session.current(&opening)?,
                own,
                attempt,
                cancel,
            )?)
        }
        PurposeV19::RefreshIssueLinksV94 { .. } => Some(
            issue_refresh_v94
                .as_mut()
                .ok_or_else(failure)?
                .take_summary_v94()?,
        ),
        PurposeV19::RefreshBaseMetadataV51 { .. } => {
            Some(measured.pr_base_metadata_refresh_opening_summary_v51(
                session.current(&opening)?,
                base_refresh_v36.as_ref().ok_or_else(failure)?,
                cancel,
            )?)
        }
        PurposeV19::RefreshMembersV48 { .. } => Some(
            members_refresh_v48
                .as_mut()
                .ok_or_else(failure)?
                .take_summary_v48()?,
        ),
        PurposeV19::RefreshBaseV36 { .. } => Some(measured.pr_base_refresh_summary_v36(
            session.current(&opening)?,
            base_refresh_v36.as_ref().ok_or_else(failure)?,
            cancel,
        )?),
        PurposeV19::Existing
        | PurposeV19::StateV68(_, _)
        | PurposeV19::MetadataSectionV51
        | PurposeV19::MetadataTextV25(_)
        | PurposeV19::MetadataBaseV32(_)
        | PurposeV19::MetadataMembersV48(_, _)
        | PurposeV19::MetadataMembersInspectionV48(_)
        | PurposeV19::MetadataMembersListV48 => None,
        PurposeV19::IssueLinksOpenV87 { .. } | PurposeV19::IssueLinksHistoryV87(..) => None,
        PurposeV19::RefreshMetadataTabV51(_) => {
            Some(measured.pr_metadata_refresh_opening_summary_v51(
                session.current(&opening)?,
                opening.parent(),
                cancel,
            )?)
        }
        PurposeV19::RefreshExact | PurposeV19::RefreshPersonalV20(_) => {
            Some(measured.pr_conversation_refresh_summary_v19(
                session.current(&opening)?,
                opening.parent(),
                cancel,
            )?)
        }
    };
    let inspection_limits_v48 = crate::providers::gh::PrMetadataReadLimits::default();
    let issue_links_catalogue_v87 = match purpose {
        PurposeV19::IssueLinksOpenV87 {
            command,
            repository,
            max_pages,
            policy,
        } => {
            let read = crate::providers::gh::IssueLinksOpenReadV87 {
                current: session.current(&opening)?,
                intent: opening.parent(),
                identity: session.identity(),
                command,
                repository,
                max_pages,
            };
            let outcome = match policy {
                #[cfg(test)]
                IssueOpenPolicyV102::Legacy => measured
                    .original_issue_link_catalogue_v87(read, cancel)
                    .map(crate::providers::gh::IssueCatalogueResultV102::Ready)?,
                IssueOpenPolicyV102::NativeSchema => {
                    measured.original_issue_link_catalogue_native_v102(read, cancel)?
                }
            };
            match outcome {
                crate::providers::gh::IssueCatalogueResultV102::Ready(catalogue) => Some(catalogue),
                crate::providers::gh::IssueCatalogueResultV102::Unsupported(proof) => {
                    // No material/SQL/final UID/Completed is produced. SAME
                    // session owns abort; a provider Arc cannot revive it.
                    drop(measured);
                    drop(session);
                    return Ok(IssueOpenCoreResultV102::Unsupported(proof));
                }
            }
        }
        _ => None,
    };
    let issue_links_inspection_v87 = match purpose {
        PurposeV19::IssueLinksHistoryV87(Some(witness), inherited) => {
            Some(measured.observe_issue_link_inspection_v87(
                crate::providers::gh::IssueLinksInspectionReadV87 {
                    current: session.current(&opening)?,
                    intent: opening.parent(),
                    identity: session.identity(),
                    witness,
                    metadata: opening.issue_link_metadata_tab_v87(),
                    inherited,
                },
                cancel,
            )?)
        }
        _ => None,
    };
    let metadata_members_v48 = match purpose {
        PurposeV19::MetadataMembersV48(command, _)
        | PurposeV19::MetadataMembersInspectionV48(command) => {
            let (limits, mut read_purpose) = match purpose {
                PurposeV19::MetadataMembersV48(_, limits) => (
                    limits,
                    crate::providers::gh::MetadataMembersReadPurposeV48::Eligible,
                ),
                PurposeV19::MetadataMembersInspectionV48(_) => (
                    &inspection_limits_v48,
                    crate::providers::gh::MetadataMembersReadPurposeV48::Inspection,
                ),
                _ => unreachable!("closed member purpose matched above"),
            };
            if opening.metadata_members_history_tab_v62() {
                read_purpose = match read_purpose {
                    crate::providers::gh::MetadataMembersReadPurposeV48::Eligible => {
                        crate::providers::gh::MetadataMembersReadPurposeV48::MetadataEligibleV62
                    }
                    crate::providers::gh::MetadataMembersReadPurposeV48::Inspection => {
                        crate::providers::gh::MetadataMembersReadPurposeV48::MetadataInspectionV62
                    }
                    _ => return Err(failure()),
                };
            }
            Some(Arc::new(measured.observe_original_metadata_members_v48(
                crate::providers::gh::MetadataMembersObservationV48 {
                    current: session.current(&opening)?,
                    intent: opening.parent(),
                    identity: session.identity(),
                    command,
                    limits,
                    selected_plan: None,
                    purpose: read_purpose,
                    retained_v62: None,
                },
                cancel,
            )?))
        }
        _ => None,
    };
    let metadata_text_v25 = match purpose {
        PurposeV19::MetadataTextV25(field) => Some(measured.pr_original_metadata_text_v25(
            session.current(&opening)?,
            opening.parent(),
            field,
            cancel,
        )?),
        _ => None,
    };
    let state_v68 = match purpose {
        PurposeV19::StateV68(command, _) => Some(measured.pr_original_state_read_v68(
            session.current(&opening)?,
            opening.parent(),
            command,
            cancel,
        )?),
        _ => None,
    };
    let metadata_base_v32 = match purpose {
        PurposeV19::MetadataBaseV32(_) => Some(measured.pr_original_metadata_base_v32(
            session.current(&opening)?,
            opening.parent(),
            cancel,
        )?),
        _ => None,
    };
    let metadata_section_v51 = match purpose {
        PurposeV19::MetadataSectionV51 => Some(measured.pr_metadata_context_section_v51(
            session.current(&opening)?,
            opening.parent(),
            cancel,
        )?),
        _ => None,
    };
    let metadata_section_parent_v51 = metadata_section_v51
        .as_ref()
        .map(|actual| Arc::clone(actual.parent()));
    let mut members_history_read_v62: Option<crate::providers::gh::MembersMetadataHistoryReadV62> =
        if matches!(purpose, PurposeV19::MetadataMembersListV48)
            && opening.metadata_members_history_tab_v62()
        {
            Some(measured.begin_members_metadata_history_v62(
                session.current(&opening)?,
                opening.parent(),
                cancel,
            )?)
        } else {
            None
        };
    let parent = match raw_summary.as_ref() {
        _ if state_v68.is_some() => Arc::clone(state_v68.as_ref().ok_or_else(failure)?.parent()),
        _ if issue_links_inspection_v87.is_some() => Arc::clone(
            issue_links_inspection_v87
                .as_ref()
                .ok_or_else(failure)?
                .parent(),
        ),
        _ if issue_links_catalogue_v87.is_some() => Arc::clone(
            issue_links_catalogue_v87
                .as_ref()
                .ok_or_else(failure)?
                .parent(),
        ),
        _ if metadata_section_parent_v51.is_some() => {
            Arc::clone(metadata_section_parent_v51.as_ref().ok_or_else(failure)?)
        }
        _ if metadata_members_v48.is_some() => {
            Arc::clone(metadata_members_v48.as_ref().ok_or_else(failure)?.parent())
        }
        _ if metadata_base_v32.is_some() => {
            Arc::clone(metadata_base_v32.as_ref().ok_or_else(failure)?.parent())
        }
        Some(summary) => Arc::clone(summary.parent()),
        None if metadata_text_v25.is_some() => {
            Arc::clone(metadata_text_v25.as_ref().ok_or_else(failure)?.parent())
        }
        None => {
            if let Some(read) = members_history_read_v62.as_ref() {
                Arc::clone(read.parent())
            } else {
                Arc::new(measured.pr_original_detail_parent_v17(
                    session.current(&opening)?,
                    opening.parent(),
                    cancel,
                )?)
            }
        }
    };
    let summary_witness = raw_summary.as_ref().map(|s| s.witness_v19());
    let metadata_witness_v25 = metadata_text_v25.as_ref().map(Arc::clone);
    let members_witness_v48 = metadata_members_v48.as_ref().map(Arc::clone);
    let base_witness_v32 = metadata_base_v32.as_ref().map(Arc::clone);
    if let PurposeV19::StateV68(_, Some(original_cache)) = purpose {
        original_cache
            .validate_at_database_v78(database)
            .map_err(|_| failure())?;
    }
    let issue_catalogue_witness_v87 = issue_links_catalogue_v87.as_ref().map(Arc::clone);
    let issue_inspection_witness_v87 = issue_links_inspection_v87.as_ref().map(Arc::clone);
    let prepared = match &session {
        SessionV17::Personal(v) => Some({
            let mut cache = Cache::open(database).map_err(|_| failure())?;
            let attempt = match purpose {
                PurposeV19::RefreshPersonalV20(attempt) => Some(attempt),
                PurposeV19::RefreshStateV68 { attempt, .. } => attempt,
                PurposeV19::RefreshMetadataTabV51(attempt) => attempt,
                PurposeV19::RefreshBaseV36 { attempt, .. } => attempt,
                PurposeV19::RefreshBaseMetadataV51 { attempt, .. } => attempt,
                PurposeV19::RefreshMembersV48 { attempt, .. } => attempt,
                PurposeV19::RefreshIssueLinksV94 { attempt, .. } => attempt,
                _ => None,
            };
            if let Some(attempt) = attempt {
                cache.prepare_personal_conversation_read_boundary_v20(
                    attempt,v.original(),parent.clone(),ThreadPhaseIssuerV16::capture(&provider)?,cancel,deadline,
                ).map_err(|_| GithubError {
                    kind: if cancel.is_cancelled() || Instant::now()>=deadline {GithubErrorKind::Transport}else{GithubErrorKind::Unsupported},
                    message:"Exact Personal read reservation unavailable; no write resent or epoch inferred".into(),
                })?
            } else {
                cache
                    .prepare_personal_parent_boundary_v16(
                        v.original(),
                        parent.clone(),
                        ThreadPhaseIssuerV16::capture(&provider)?,
                        cancel,
                        deadline,
                    )
                    .map_err(|_| failure())?
            }
        }),
        _ => None,
    };
    let boundary = match prepared.as_ref() {
        Some(v) => PrLocalSourceBoundaryV13::PersonalReservation(v.reservation()),
        None => {
            PrLocalSourceBoundaryV13::Catalogue(opening.catalogue_source().ok_or_else(failure)?)
        }
    };
    let metadata_null_v51 = if matches!(
        purpose,
        PurposeV19::RefreshMetadataTabV51(_) | PurposeV19::RefreshBaseMetadataV51 { .. }
    ) {
        raw_summary.as_ref().map(|s| s.body_null_v51())
    } else {
        None
    };
    let mut page_supplement_v61 = crate::providers::gh::ConversationPageSupplementV61::None;
    let refresh_v19 = raw_summary
        .map(|summary| {
            if let Some(read) = issue_refresh_v94.as_mut() {
                return measured.collect_issue_link_manual_details_v94(summary, read, cancel);
            }
            match members_refresh_v48
                .as_ref()
                .filter(|read| read.metadata_tab_v62())
            {
                Some(read) => {
                    measured.collect_members_metadata_manual_details_v62(read, summary, cancel)
                }
                None => measured.collect_conversation_details_measured_v19(summary, cancel),
            }
        })
        .map(|snapshot| {
            snapshot.and_then(|snapshot| {
                if let Some(read) = issue_refresh_v94.as_mut() {
                    let context = measured.finish_issue_link_manual_context_v94(
                        session.current(&opening)?,
                        read,
                        &snapshot,
                        cancel,
                    )?;
                    page_supplement_v61 =
                        crate::providers::gh::ConversationPageSupplementV61::issue_links_v87(
                            &snapshot, context,
                        )?;
                    return Ok(snapshot);
                }
                match members_refresh_v48.as_mut() {
                    Some(read) => {
                        let (snapshot, supplement) = if read.metadata_tab_v62() {
                            measured.finish_members_metadata_manual_v62(
                                session.current(&opening)?,
                                read,
                                opening.parent(),
                                snapshot,
                                cancel,
                            )?
                        } else {
                            measured.attach_members_manual_context_v48(snapshot, read, cancel)?
                        };
                        page_supplement_v61 = supplement;
                        Ok(snapshot)
                    }
                    None => Ok(snapshot),
                }
            })
        })
        .transpose()?;
    let metadata_section_v51 = if matches!(purpose, PurposeV19::RefreshMetadataTabV51(_)) {
        Some(measured.pr_metadata_tab_refresh_context_v51(
            session.current(&opening)?,
            opening.parent(),
            refresh_v19.as_ref().ok_or_else(failure)?,
            cancel,
        )?)
    } else if matches!(purpose, PurposeV19::RefreshBaseMetadataV51 { .. }) {
        Some(measured.pr_base_metadata_refresh_context_v51(
            session.current(&opening)?,
            base_refresh_v36.as_ref().ok_or_else(failure)?,
            refresh_v19.as_ref().ok_or_else(failure)?,
            cancel,
        )?)
    } else {
        metadata_section_v51
    };
    // Capture the actual retained snapshot+context debit BEFORE caller moves
    // them into prepared page/SQL material. Never decode final rawPR first.
    let metadata_summary_debit_v51 = if matches!(
        purpose,
        PurposeV19::RefreshMetadataTabV51(_) | PurposeV19::RefreshBaseMetadataV51 { .. }
    ) {
        Some(
            refresh_v19
                .as_ref()
                .ok_or_else(failure)?
                .metadata_summary_debit_v51(metadata_section_v51.as_ref())?,
        )
    } else {
        None
    };
    let material = prepare(InitialReadV17 {
        parent: &parent,
        identity: session.identity(),
        boundary,
        measured: &measured,
        opening: &opening,
        cancel,
        effective_cutoff_v51: deadline,
        refresh_v19,
        metadata_members_v48,
        metadata_text_v25,
        metadata_base_v32,
        metadata_section_v51,
        page_supplement_v61,
        state_v68,
        issue_links_catalogue_v87,
        issue_links_inspection_v87,
    })?;
    let closing = match summary_witness {
        _ if issue_refresh_v94.is_some() => measured.close_issue_link_manual_read_v94(
            session.current(&opening)?,
            issue_refresh_v94.as_mut().ok_or_else(failure)?,
            cancel,
        )?,
        _ if issue_inspection_witness_v87.is_some() => measured.close_original_issue_links_v87(
            issue_inspection_witness_v87.as_ref().ok_or_else(failure)?,
            session.current(&opening)?,
            opening.parent(),
            cancel,
        )?,
        _ if issue_catalogue_witness_v87.is_some() => measured
            .close_original_issue_link_catalogue_v87(
                issue_catalogue_witness_v87.as_ref().ok_or_else(failure)?,
                session.current(&opening)?,
                opening.parent(),
                cancel,
            )?,
        // Context already paid openingPR→issue→repo→closingPR. Never append
        // a fifth measured child; SAME closing repository/SQL/finalUID follow.
        _ if metadata_section_parent_v51.is_some() => {
            Arc::clone(metadata_section_parent_v51.as_ref().ok_or_else(failure)?)
        }
        _ if members_refresh_v48.is_some() => measured.close_members_manual_refresh_v48(
            session.current(&opening)?,
            members_refresh_v48.as_ref().ok_or_else(failure)?,
            opening.parent(),
            cancel,
        )?,
        _ if members_witness_v48.is_some() => {
            let actual = members_witness_v48.as_ref().ok_or_else(failure)?;
            measured.close_original_metadata_members_v48(
                actual,
                session.current(&opening)?,
                opening.parent(),
                cancel,
            )?;
            Arc::clone(actual.parent())
        }
        _ if base_witness_v32.is_some() => {
            let original = base_witness_v32.as_ref().ok_or_else(failure)?;
            let closing = measured.pr_original_metadata_base_v32(
                session.current(&opening)?,
                opening.parent(),
                cancel,
            )?;
            if !original.same_observation(&closing) {
                return Err(failure());
            }
            Arc::clone(closing.parent())
        }
        Some(witness) => {
            let summary = if let PurposeV19::RefreshStateV68 { own, attempt } = purpose {
                measured.pr_state_refresh_summary_v68(
                    session.current(&opening)?,
                    own,
                    attempt,
                    cancel,
                )?
            } else {
                match (base_refresh_v36.as_ref(), metadata_summary_debit_v51) {
                    (Some(actual), Some(debit)) => measured.pr_base_metadata_refresh_summary_v51(
                        session.current(&opening)?,
                        actual,
                        debit,
                        cancel,
                    )?,
                    (None, Some(debit)) => measured.pr_metadata_refresh_summary_v51(
                        session.current(&opening)?,
                        opening.parent(),
                        debit,
                        cancel,
                    )?,
                    (Some(actual), None) => measured.pr_base_refresh_summary_v36(
                        session.current(&opening)?,
                        actual,
                        cancel,
                    )?,
                    (None, None) => measured.pr_conversation_refresh_summary_v19(
                        session.current(&opening)?,
                        opening.parent(),
                        cancel,
                    )?,
                }
            };
            if !witness.matches_v19(&summary) {
                return Err(failure());
            }
            if metadata_null_v51.is_some_and(|is_null| is_null != summary.body_null_v51()) {
                return Err(failure());
            }
            Arc::clone(summary.parent())
        }
        None if metadata_witness_v25.is_some() => {
            let original = metadata_witness_v25.as_ref().ok_or_else(failure)?;
            let closing = measured.pr_original_metadata_text_v25(
                session.current(&opening)?,
                opening.parent(),
                original.field(),
                cancel,
            )?;
            if !original.same_observation(&closing) {
                return Err(failure());
            }
            Arc::clone(closing.parent())
        }
        None => {
            if let Some(read) = members_history_read_v62.as_mut() {
                measured.close_members_metadata_history_v62(
                    read,
                    session.current(&opening)?,
                    opening.parent(),
                    cancel,
                )?
            } else {
                Arc::new(measured.pr_original_detail_parent_v17(
                    session.current(&opening)?,
                    opening.parent(),
                    cancel,
                )?)
            }
        }
    };
    if !GithubProvider::same_thread_history_parent_v1(&parent, &closing) {
        return Err(failure());
    }
    validate(&material, boundary)?;
    if let Some(read) = issue_refresh_v94.as_ref() {
        read.validate_live_before_close_v94(&measured, cancel)?;
    }
    let ticket = match (&session, prepared.as_ref()) {
        (SessionV17::Personal(v), Some(p)) => Some(
            p.verify_for_close(v.original(), &closing)
                .map_err(|_| failure())?,
        ),
        _ => None,
    };
    live(cancel, deadline)?;
    let (closed, identity) = match session {
        SessionV17::Catalogue(mut session) => {
            session.verify(
                opening
                    .catalogue_navigation()
                    .ok_or_else(failure)?
                    .original(),
                cancel,
            )?;
            let closed = session.close_with_guard(cancel, || {
                live(cancel, deadline)?;
                if let Some(read) = issue_refresh_v94.as_ref() {
                    read.validate_generation_v94(&measured, cancel)?;
                }
                validate(&material, boundary)
            })?;
            let identity = closed.identity().clone();
            (ClosedOriginV17::Catalogue(Box::new(closed)), identity)
        }
        SessionV17::Personal(session) => {
            let closed = session.close_with_guard(cancel, || {
                live(cancel, deadline)?;
                if let Some(read) = issue_refresh_v94.as_ref() {
                    read.validate_generation_v94(&measured, cancel)?;
                }
                validate(&material, boundary)
            })?;
            let identity = closed.identity().clone();
            let boundary = prepared
                .ok_or_else(failure)?
                .bind_after_close(closed, ticket.ok_or_else(failure)?)
                .map_err(|_| failure())?;
            (ClosedOriginV17::Personal(Arc::new(boundary)), identity)
        }
    };
    live(cancel, deadline)?;
    if let Some(read) = issue_refresh_v94.as_ref() {
        read.validate_generation_v94(&measured, cancel)?;
    }
    Ok(IssueOpenCoreResultV102::Closed(CompletedV17 {
        material,
        parent,
        identity,
        closed,
        effective_cutoff_v51: deadline,
    }))
}
