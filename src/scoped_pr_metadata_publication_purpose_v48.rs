//! Private closed Text|Members adapters for the SAME PrPlan lifecycle. No
//! generic strategy/callback, provider getter, clock or publication engine.
use super::*;
use crate::{
    providers::gh::{
        OriginalMetadataMembersV48, OriginalMetadataTextV25, PrMetadataMembersReceiptV48,
        PrMetadataTextReceiptV25,
    },
    scoped_pr_collaboration_source_v17::{
        metadata_members_phase_v48::{
            MetadataMembersPhasePurposeV48, MetadataMembersPhaseV48, MetadataMembersPublicationV48,
        },
        metadata_phase_v25::MetadataTextPhaseV25,
    },
    scoped_pr_collaboration_ui_v17::metadata_members_v48::MetadataMembersPreviewV48,
    scoped_pr_issue_links_v87::IssueLinkPreviewV87,
};

pub(super) enum PurposeV48<'a> {
    Text(&'a Arc<MetadataPreviewV25>),
    Members(&'a Arc<MetadataMembersPreviewV48>),
    StateV68(&'a Arc<crate::scoped_pr_collaboration_ui_v17::state_v68::StatePreviewV68>),
    IssueLinksV87(&'a Arc<IssueLinkPreviewV87>),
}
pub(super) enum PhaseV48<'a> {
    Text(MetadataTextPhaseV25<'a>),
    Members(MetadataMembersPhaseV48<'a>),
    StateV68(crate::scoped_pr_collaboration_source_v17::state_phase_v68::StatePhaseV68<'a>),
    IssueLinksV87(
        crate::scoped_pr_collaboration_source_v17::issue_links_phase_v87::IssueLinkPhaseV87<'a>,
    ),
}
pub(super) enum ActualV48 {
    Text(Arc<OriginalMetadataTextV25>),
    Members(Arc<OriginalMetadataMembersV48>),
    StateV68(Arc<crate::providers::gh::OriginalPrStateV68>),
    IssueLinksV87(
        Arc<crate::providers::gh::OriginalIssueLinksV87>,
        crate::providers::gh::IssueLinkPlanV87,
    ),
}
pub(super) enum SeedV48 {
    Text(Box<MetadataOriginSeedV25>),
    Members(Box<crate::pr_write_journal::metadata_members_v48::MetadataMembersOriginSeedV48>),
    StateV68(Box<crate::pr_write_journal::state_origin_v68::StateOriginSeedV68>),
    IssueLinksV87(Box<crate::pr_write_journal::issue_link_origin_v87::IssueLinkOriginSeedV87>),
}
pub(super) enum ReceiptV48 {
    Text(PrMetadataTextReceiptV25),
    Members(PrMetadataMembersReceiptV48),
    StateV68(crate::providers::gh::StateReceiptV68),
    IssueLinksV87(Box<crate::providers::gh::IssueLinkReceiptV87>),
}

#[test]
fn pr_metadata_issue_link_v87_closed_receipt_keeps_large_observation_out_of_inline_adapter() {
    let prior_maximum = std::mem::size_of::<PrMetadataTextReceiptV25>()
        .max(std::mem::size_of::<PrMetadataMembersReceiptV48>())
        .max(std::mem::size_of::<crate::providers::gh::StateReceiptV68>());
    let layout_slack = 2 * std::mem::align_of::<ReceiptV48>();
    assert!(std::mem::size_of::<ReceiptV48>() <= prior_maximum + layout_slack);
    assert!(
        std::mem::size_of::<crate::providers::gh::IssueLinkReceiptV87>()
            > std::mem::size_of::<ReceiptV48>()
    );
}
pub(super) enum FailureV48 {
    NotStarted(GithubError),
    WriteMayHaveStarted(GithubError),
}
pub(super) enum ProjectionInvalidationKindV74 {
    Unresolved,
    Confirmed,
}
pub(super) struct ProjectionInvalidationV74<'a, 'p> {
    pub(super) purpose: &'a PurposeV48<'p>,
    pub(super) actual: &'a ActualV48,
    pub(super) journal: &'a PrWriteJournal,
    pub(super) local: &'a crate::scoped_pr_observed_tabs_v1::resources::ResourceLocalGuard,
    pub(super) reference: &'a PrPublicationReference,
    pub(super) generation: i64,
    pub(super) cancel: &'a CancellationToken,
    pub(super) deadline: Instant,
    pub(super) kind: ProjectionInvalidationKindV74,
}
impl PurposeV48<'_> {
    pub(super) fn is_state_v68(&self) -> bool {
        matches!(self, Self::StateV68(_))
    }
    pub(super) fn observation(&self) -> &'static str {
        match self {
            Self::Text(_) => "Exact requested metadata observed by PATCH receipt plus independent GET; observation is not causal proof",
            Self::Members(_) => "Exact requested metadata observed by mutation, complete GraphQL and independent REST IDs; observation is not causal proof",
            Self::StateV68(_)=>"Exact requested PR state observed by typed GraphQL result and independent REST readback; no expected-head CAS or exclusive causation claimed",
            Self::IssueLinksV87(_) => "Exact manual development link observed by mutation and independent PR/Issue readbacks; observation is not causal proof",
        }
    }
    pub(super) fn source(
        &self,
    ) -> &Arc<crate::scoped_pr_collaboration_source_v17::PrCollaborationSourceV17> {
        match self {
            Self::Text(preview) => &preview.intent.source,
            Self::Members(preview) => &preview.intent.source,
            Self::StateV68(preview) => &preview.intent.source,
            Self::IssueLinksV87(preview) => &preview.intent.source,
        }
    }
    pub(super) fn key(&self) -> &str {
        match self {
            Self::Text(preview) => &preview.intent.key,
            Self::Members(preview) => &preview.key,
            Self::StateV68(preview) => &preview.key,
            Self::IssueLinksV87(preview) => &preview.key,
        }
    }
    pub(super) fn values(&self) -> &BTreeMap<String, String> {
        match self {
            Self::Text(preview) => &preview.values,
            Self::Members(preview) => &preview.values,
            Self::StateV68(preview) => &preview.values,
            Self::IssueLinksV87(preview) => &preview.values,
        }
    }
    pub(super) fn consume(&self) -> bool {
        let (useful, used) = match self {
            Self::Text(preview) => (&preview.useful, &preview.used),
            Self::Members(preview) => (&preview.useful, &preview.used),
            Self::StateV68(preview) => (&preview.useful, &preview.used),
            Self::IssueLinksV87(preview) if !preview.needs_new => (&preview.useful, &preview.used),
            Self::IssueLinksV87(_) => return false,
        };
        useful.load(Ordering::Acquire) && !used.swap(true, Ordering::AcqRel)
    }
    pub(super) fn write(&self) -> GithubResult<crate::pr_write_journal::PrWriteIntent> {
        match self {
            Self::Text(preview) => preview.seed.write(self.values()),
            Self::Members(preview) => preview.seed.write(self.values()),
            Self::StateV68(preview) => preview.seed.write(self.values()),
            Self::IssueLinksV87(preview) => preview.seed.write(self.values()),
        }
        .map_err(|_| failure())
    }
    pub(super) fn with_phase<T>(
        &self,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
        run: impl FnOnce(PhaseV48<'_>) -> GithubResult<T>,
    ) -> GithubResult<T> {
        match self {
            Self::StateV68(preview) => self.source().with_state_phase_v68(
                crate::scoped_pr_collaboration_source_v17::state_phase_v68::StatePhaseRequestV68 {
                    provider,
                    database,
                    cancel,
                    deadline,
                    command: preview.intent.command,
                    postwrite: true,
                },
                |phase| run(PhaseV48::StateV68(phase)),
            ),
            Self::IssueLinksV87(preview) => self.source().with_issue_link_phase_v87(
                crate::scoped_pr_collaboration_source_v17::issue_links_phase_v87::IssueLinkPhaseRequestV87 {
                    command:preview.intent.command,choice:&preview.intent.choice,
                    retained:Some(&preview.actual),
                    purpose:crate::scoped_pr_collaboration_source_v17::issue_links_phase_v87::IssueLinkPhasePurposeV87::Confirm(&preview.plan),
                },provider,database,cancel,deadline,|phase|run(PhaseV48::IssueLinksV87(phase))),
            Self::Text(preview) => self.source().with_metadata_text_phase_v25(
                provider,
                database,
                cancel,
                deadline,
                preview.intent.field,
                true,
                |phase| run(PhaseV48::Text(phase)),
            ),
            Self::Members(preview) => self.source().with_metadata_members_phase_v48(
                provider,
                database,
                cancel,
                deadline,
                if preview
                    .intent
                    .source
                    .opening()
                    .metadata_members_tab_v61(preview.plan.command())
                {
                    MetadataMembersPhasePurposeV48::MetadataConfirmV62(
                        &preview.plan,
                        &preview.actual,
                    )
                } else {
                    MetadataMembersPhasePurposeV48::Confirm(&preview.plan, preview.actual.limits())
                },
                |phase| run(PhaseV48::Members(phase)),
            ),
        }
    }
    pub(super) fn seed(
        &self,
        phase: &PhaseV48<'_>,
        actual: &ActualV48,
        database: &Path,
    ) -> GithubResult<SeedV48> {
        crate::pr_write_journal::metadata_members_v48::reserve_origin_box_moves_v79()
            .map_err(|_| failure())?;
        match (self,phase, actual) {
            (Self::StateV68(preview),PhaseV48::StateV68(_),ActualV48::StateV68(actual))=>{
                actual.require()?;
                let current=crate::pr_write_journal::state_origin_v68::StateOriginSeedV68::from_producer(self.source(),actual,database).map_err(|_|failure())?;
                return preview.bind_fresh_seed(current,database).map(Box::new).map(SeedV48::StateV68);
            }
            (Self::IssueLinksV87(preview),PhaseV48::IssueLinksV87(phase),ActualV48::IssueLinksV87(actual,plan))=>{
                let current=crate::pr_write_journal::issue_link_origin_v87::IssueLinkOriginSeedV87::from_producer(self.source(),actual,plan,database).map_err(|_|failure())?;
                return preview.bind_fresh_seed(current,phase,database).map(|seed|SeedV48::IssueLinksV87(Box::new(seed)));
            }
            (Self::Text(preview),PhaseV48::Text(_), ActualV48::Text(actual)) => {
                if actual.observed().value() != preview.intent.before.as_ref() { return Err(failure()); }
                MetadataOriginSeedV25::from_producer(self.source(), actual, &self.values()["body"], database).map(|seed| SeedV48::Text(Box::new(seed)))
            }
            (Self::Members(preview),PhaseV48::Members(phase), ActualV48::Members(actual)) => {
                let current=crate::pr_write_journal::metadata_members_v48::MetadataMembersOriginSeedV48::from_producer(
                    self.source(), actual, &preview.plan, self.values(), database).map_err(|_|failure())?;
                return preview.bind_fresh_seed_v52(current,phase,database).map(|seed| SeedV48::Members(Box::new(seed)));
            }
            _ => return Err(failure()),
        }.map_err(|_| failure())
    }
}
impl PhaseV48<'_> {
    pub(super) fn local(
        &self,
        database: &Path,
    ) -> GithubResult<crate::scoped_pr_observed_tabs_v1::resources::ResourceLocalGuard> {
        match self {
            Self::Text(phase) => phase.local(database),
            Self::Members(phase) => phase.local(database),
            Self::StateV68(phase) => phase.local(database),
            Self::IssueLinksV87(phase) => phase.local(database),
        }
    }
    pub(super) fn actual(&self) -> GithubResult<ActualV48> {
        Ok(match self {
            Self::Text(phase) => ActualV48::Text(Arc::clone(phase.actual())),
            Self::Members(phase) => ActualV48::Members(Arc::clone(phase.actual())),
            Self::StateV68(phase) => ActualV48::StateV68(Arc::clone(phase.actual())),
            Self::IssueLinksV87(phase) => ActualV48::IssueLinksV87(
                Arc::clone(phase.actual()),
                phase.actual().prepare_requested()?,
            ),
        })
    }
    pub(super) fn deadline(&self) -> Instant {
        match self {
            Self::Text(phase) => phase.effective_deadline_v48(),
            Self::Members(phase) => phase.deadline(),
            Self::StateV68(phase) => phase.deadline(),
            Self::IssueLinksV87(phase) => phase.deadline(),
        }
    }
    pub(super) fn take_captures(
        &mut self,
    ) -> GithubResult<crate::providers::gh::PostWriteCaptureAllowanceV2> {
        match self {
            Self::Text(phase) => phase.take_captures(),
            Self::Members(phase) => phase.take_captures(),
            Self::StateV68(phase) => phase.take_captures(),
            Self::IssueLinksV87(phase) => phase.take_captures(),
        }
    }
    pub(super) fn publish(
        self,
        purpose: &PurposeV48<'_>,
        provider: &GithubProvider,
        captures: &mut crate::providers::gh::PostWriteCaptureAllowanceV2,
        publication: PublicationAckV48<'_>,
        guard: impl FnOnce() -> GithubResult<()>,
    ) -> Result<ReceiptV48, FailureV48> {
        match (self, purpose) {
            (Self::StateV68(phase),PurposeV48::StateV68(_))=>phase.publish(provider,captures,
                crate::scoped_pr_collaboration_source_v17::state_phase_v68::StatePublicationAckV68 {
                    permit:publication.permit,ack:publication.ack,drafts:publication.drafts,key:purpose.key(),
                },guard).map(ReceiptV48::StateV68).map_err(|e|match e {
                    crate::providers::gh::StatePublicationFailureV68::NotStarted(e)=>FailureV48::NotStarted(e),
                    crate::providers::gh::StatePublicationFailureV68::WriteMayHaveStarted(e)=>FailureV48::WriteMayHaveStarted(e),
                }),
            (Self::IssueLinksV87(phase),PurposeV48::IssueLinksV87(_))=>phase.publish(provider,
                crate::scoped_pr_collaboration_source_v17::issue_links_phase_v87::IssueLinkPublicationV87{
                    captures,ack:publication.ack,drafts:publication.drafts,database:publication.database,key:purpose.key(),
                },guard).map(|receipt| ReceiptV48::IssueLinksV87(Box::new(receipt))).map_err(|failure|match failure{
                    crate::providers::gh::IssueLinkPublicationFailureV87::NotStarted(error)=>FailureV48::NotStarted(error),
                    crate::providers::gh::IssueLinkPublicationFailureV87::WriteMayHaveStarted(error)=>FailureV48::WriteMayHaveStarted(error),
                }),
            (Self::Text(phase), PurposeV48::Text(_)) => phase
                .publish(
                    provider,
                    captures,
                    &purpose.values()["body"],
                    publication.ack,
                    publication.drafts,
                    publication.database,
                    purpose.key(),
                    guard,
                )
                .map(ReceiptV48::Text)
                .map_err(|failure| match failure {
                    crate::providers::gh::PrMetadataTextFailureV25::NotStarted(error) => {
                        FailureV48::NotStarted(error)
                    }
                    crate::providers::gh::PrMetadataTextFailureV25::WriteMayHaveStarted(error) => {
                        FailureV48::WriteMayHaveStarted(error)
                    }
                }),
            (Self::Members(phase), PurposeV48::Members(preview)) => phase
                .publish(
                    provider,
                    MetadataMembersPublicationV48 {
                        plan: &preview.plan,
                        captures,
                        ack: publication.ack,
                        drafts: publication.drafts,
                        database: publication.database,
                        key: purpose.key(),
                    },
                    guard,
                )
                .map(ReceiptV48::Members)
                .map_err(|failure| match failure {
                    crate::providers::gh::PrMetadataMembersFailureV48::NotStarted(error) => {
                        FailureV48::NotStarted(error)
                    }
                    crate::providers::gh::PrMetadataMembersFailureV48::WriteMayHaveStarted(
                        error,
                    ) => FailureV48::WriteMayHaveStarted(error),
                }),
            _ => Err(FailureV48::NotStarted(failure())),
        }
    }
}
pub(super) struct PublicationAckV48<'a> {
    pub(super) permit: &'a crate::pr_write_journal::PrWritePermit,
    pub(super) ack: &'a crate::drafts::DraftPublicationAckV2,
    pub(super) drafts: &'a crate::drafts::DraftClient,
    pub(super) database: &'a Path,
}
impl ActualV48 {
    pub(super) fn identity(&self) -> &HostIdentity {
        match self {
            Self::Text(actual) => actual.identity(),
            Self::Members(actual) => actual.identity(),
            Self::StateV68(actual) => actual.identity(),
            Self::IssueLinksV87(actual, _) => actual.identity(),
        }
    }
}
impl SeedV48 {
    pub(super) fn invalidate_projection_v74(
        &self,
        request: ProjectionInvalidationV74<'_, '_>,
    ) -> anyhow::Result<()> {
        let ProjectionInvalidationV74 {
            purpose,
            actual,
            journal,
            local,
            reference,
            generation,
            cancel,
            deadline,
            kind,
        } = request;
        match (self, actual, purpose) {
            (Self::StateV68(seed), ActualV48::StateV68(actual), PurposeV48::StateV68(_)) => {
                let repair = journal.prepare_original_state_cache_repair_v74(
                    crate::pr_write_journal::state_origin_v68::StateRepairOriginalV74 {
                        source: purpose.source(),
                        actual,
                        seed,
                        reference,
                        local,
                        generation,
                        cancel,
                        deadline,
                    },
                )?;
                crate::storage::PreparedStateCacheRepairV74::prepare(repair, cancel, deadline)?
                    .commit(cancel, deadline)
            }
            (Self::Text(_), ActualV48::Text(_), PurposeV48::Text(_))
            | (Self::Members(_), ActualV48::Members(_), PurposeV48::Members(_)) => {
                let result = match kind {
                    ProjectionInvalidationKindV74::Unresolved => {
                        crate::pr_projection::invalidate_unresolved_discussions(
                            purpose.source().opening().database(),
                            actual.identity(),
                            purpose.source().parent().summary(),
                            reference,
                        )
                    }
                    ProjectionInvalidationKindV74::Confirmed => {
                        crate::pr_projection::invalidate_confirmed_discussions(
                            purpose.source().opening().database(),
                            actual.identity(),
                            purpose.source().parent().summary(),
                            reference,
                        )
                    }
                };
                result.map(|_| ()).map_err(anyhow::Error::msg)
            }
            _ => anyhow::bail!("Mixed State/text invalidation purpose refused"),
        }
    }
    pub(super) fn confirm_v68(
        &self,
        journal: &mut PrWriteJournal,
        permit: &crate::pr_write_journal::PrWritePermit,
        receipt: &ReceiptV48,
        claimed_generation: i64,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> anyhow::Result<()> {
        match (self, receipt) {
            (Self::StateV68(seed), ReceiptV48::StateV68(receipt)) => {
                journal.mark_state_confirmed_v68(
                    permit,
                    seed,
                    receipt,
                    claimed_generation,
                    cancel,
                    deadline,
                )?;
                Ok(())
            }
            (Self::Text(_), ReceiptV48::Text(_)) | (Self::Members(_), ReceiptV48::Members(_)) => {
                journal.mark_confirmed(permit, &receipt.effect())
            }
            _ => anyhow::bail!("Mixed closed State/metadata receipt cannot finish a publication"),
        }
    }
    pub(super) fn validate_owned_v52(
        &self,
        journal: &PrWriteJournal,
        reference: &PrPublicationReference,
    ) -> GithubResult<()> {
        match self {
            Self::Text(_) => Ok(()),
            Self::Members(seed) => journal
                .validate_owned_members_v52(seed, reference)
                .map_err(|_| failure()),
            Self::StateV68(seed) => journal
                .validate_owned_state_v68(seed, reference)
                .map_err(|_| failure()),
            Self::IssueLinksV87(seed) => journal
                .validate_owned_issue_link_v87(seed, reference)
                .map_err(|_| failure()),
        }
    }
    pub(super) fn write(
        &self,
        values: &BTreeMap<String, String>,
    ) -> GithubResult<crate::pr_write_journal::PrWriteIntent> {
        match self {
            Self::Text(seed) => seed.write(values),
            Self::Members(seed) => seed.write(values),
            Self::StateV68(seed) => seed.write(values),
            Self::IssueLinksV87(seed) => seed.write(values),
        }
        .map_err(|_| failure())
    }
    pub(super) fn begin(
        &self,
        journal: &mut PrWriteJournal,
        target: &crate::pr_write_journal::PrWriteTarget,
        write: &crate::pr_write_journal::PrWriteIntent,
    ) -> GithubResult<crate::pr_write_journal::PrWritePermit> {
        match self {
            Self::Text(seed) => journal.begin_metadata_text_v25(target, write, seed),
            Self::Members(seed) => journal.begin_metadata_members_v48(target, write, seed),
            Self::StateV68(seed) => journal.begin_state_v68(target, write, seed),
            Self::IssueLinksV87(seed) => journal.begin_issue_link_v87(target, write, seed),
        }
        .map_err(|_| failure())
    }
    pub(super) fn matches_receipt(&self, receipt: &ReceiptV48) -> bool {
        match (self, receipt) {
            (Self::Text(seed), ReceiptV48::Text(receipt)) => seed.matches_receipt(receipt),
            (Self::Members(seed), ReceiptV48::Members(receipt)) => {
                seed.matches_receipt(receipt.effect()).unwrap_or(false)
            }
            (Self::StateV68(seed), ReceiptV48::StateV68(receipt)) => {
                seed.matches_receipt_v68(receipt).unwrap_or(false)
            }
            (Self::IssueLinksV87(seed), ReceiptV48::IssueLinksV87(receipt)) => {
                seed.matches_receipt(receipt.effect()).unwrap_or(false)
            }
            _ => false,
        }
    }
}
impl ReceiptV48 {
    pub(super) fn effect(&self) -> crate::pr_write_journal::PrConfirmedEffect {
        match self {
            Self::Text(receipt) => crate::pr_write_journal::metadata_origin_v25::effect(receipt),
            Self::Members(receipt) => receipt.effect().clone(),
            Self::StateV68(receipt) => receipt.effect(),
            Self::IssueLinksV87(receipt) => receipt.effect().clone(),
        }
    }
    pub(super) fn terminal(
        &self,
        request: TerminalPreparationV48<'_>,
    ) -> anyhow::Result<
        Arc<crate::pr_write_journal::conversation_refresh_v19::ConversationTerminalWitnessV19>,
    > {
        let TerminalPreparationV48 {
            journal,
            provider,
            source,
            actual,
            permit,
            local,
            generation,
            effect,
            cancel,
            purpose,
        } = request;
        match (self, actual, purpose) {
            (Self::StateV68(receipt), ActualV48::StateV68(actual), PurposeV48::StateV68(_)) => {
                journal.prepare_native_state_terminal_v68(
                    crate::pr_write_journal::conversation_refresh_v19::StateTerminalV68 {
                        source,
                        actual,
                        receipt,
                        permit,
                        local,
                        generation,
                        effect,
                        cancel,
                    },
                )
            }
            (
                Self::IssueLinksV87(receipt),
                ActualV48::IssueLinksV87(actual, plan),
                PurposeV48::IssueLinksV87(_),
            ) => journal.prepare_native_issue_link_terminal_v87(
                crate::pr_write_journal::conversation_refresh_v19::IssueLinkTerminalV87 {
                    source,
                    provider,
                    actual,
                    plan,
                    receipt,
                    permit,
                    local,
                    confirmed_generation: generation,
                    cancel,
                },
            ),
            (Self::Text(receipt), ActualV48::Text(actual), PurposeV48::Text(_)) => journal
                .prepare_native_metadata_terminal_v25(
                    source, actual, receipt, permit, local, generation, effect, cancel,
                ),
            (Self::Members(receipt), ActualV48::Members(actual), PurposeV48::Members(preview)) => {
                journal.prepare_native_metadata_members_terminal_v48(
                    crate::pr_write_journal::conversation_refresh_v19::MetadataMembersTerminalV48 {
                        source,
                        actual,
                        plan: &preview.plan,
                        receipt,
                        permit,
                        local,
                        confirmed_generation: generation,
                        effect,
                        cancel,
                    },
                )
            }
            _ => anyhow::bail!("Mixed native metadata terminal purpose"),
        }
    }
}
pub(super) struct TerminalPreparationV48<'a> {
    pub(super) journal: &'a PrWriteJournal,
    pub(super) provider: &'a GithubProvider,
    pub(super) source: &'a Arc<crate::scoped_pr_collaboration_source_v17::PrCollaborationSourceV17>,
    pub(super) actual: &'a ActualV48,
    pub(super) purpose: &'a PurposeV48<'a>,
    pub(super) permit: &'a crate::pr_write_journal::PrWritePermit,
    pub(super) local: &'a crate::scoped_pr_observed_tabs_v1::resources::ResourceLocalGuard,
    pub(super) generation: i64,
    pub(super) effect: &'a crate::pr_write_journal::PrConfirmedEffect,
    pub(super) cancel: &'a CancellationToken,
}
pub(super) struct ConfirmedPageV48<'a> {
    pub(super) provider: &'a GithubProvider,
    pub(super) actual: &'a ActualV48,
    pub(super) purpose: &'a PurposeV48<'a>,
    pub(super) captures: &'a mut crate::providers::gh::PostWriteCaptureAllowanceV2,
    pub(super) cancel: &'a CancellationToken,
    pub(super) deadline: Instant,
}
impl ReceiptV48 {
    pub(super) fn refresh<T>(
        &self,
        request: ConfirmedPageV48<'_>,
        prepare: impl FnOnce(
            crate::providers::gh::ConversationDetailSnapshotV19,
            crate::providers::gh::ConversationPageSupplementV61,
            &crate::providers::gh::ConversationRefreshPreparationV19,
        ) -> GithubResult<T>,
    ) -> GithubResult<crate::providers::gh::ClosedConversationRefreshV19<T>> {
        let ConfirmedPageV48 {
            provider,
            actual,
            purpose,
            captures,
            cancel,
            deadline,
        } = request;
        match (self, actual, purpose) {
            (Self::StateV68(receipt), ActualV48::StateV68(actual), PurposeV48::StateV68(_)) => {
                provider.prepare_confirmed_state_refresh_v68(
                    crate::providers::gh::StateRefreshV68 {
                        actual,
                        receipt,
                        current: purpose.source().current()?,
                        captures,
                        cancel,
                        deadline,
                    },
                    |snapshot, preparation| {
                        prepare(
                            snapshot,
                            crate::providers::gh::ConversationPageSupplementV61::None,
                            preparation,
                        )
                    },
                )
            }
            (
                Self::IssueLinksV87(receipt),
                ActualV48::IssueLinksV87(actual, plan),
                PurposeV48::IssueLinksV87(_),
            ) => provider.prepare_confirmed_issue_link_refresh_v87(
                crate::providers::gh::IssueLinkRefreshV87 {
                    actual,
                    plan,
                    receipt,
                    current: purpose.source().current()?,
                    captures,
                    cancel,
                    deadline,
                },
                prepare,
            ),
            (Self::Text(receipt), ActualV48::Text(actual), PurposeV48::Text(_)) => {
                if purpose.source().opening().metadata_tab_v51() {
                    provider.prepare_confirmed_metadata_tab_refresh_v51(
                        actual,
                        receipt,
                        purpose.source().current()?,
                        captures,
                        cancel,
                        deadline,
                        |snapshot, context, preparation| {
                            let supplement =
                                crate::providers::gh::ConversationPageSupplementV61::metadata_v79(
                                    &snapshot, context,
                                )?;
                            prepare(snapshot, supplement, preparation)
                        },
                    )
                } else {
                    provider.prepare_confirmed_metadata_refresh_v25(
                        actual,
                        receipt,
                        purpose.source().current()?,
                        captures,
                        cancel,
                        deadline,
                        |snapshot, preparation| {
                            prepare(
                                snapshot,
                                crate::providers::gh::ConversationPageSupplementV61::None,
                                preparation,
                            )
                        },
                    )
                }
            }
            (Self::Members(receipt), ActualV48::Members(actual), PurposeV48::Members(preview)) => {
                let request = crate::providers::gh::MembersRefreshV48 {
                    actual,
                    plan: &preview.plan,
                    receipt,
                    current: purpose.source().current()?,
                    captures,
                    cancel,
                    deadline,
                };
                if purpose
                    .source()
                    .opening()
                    .metadata_members_tab_v61(preview.plan.command())
                {
                    provider.prepare_confirmed_members_metadata_refresh_v62(request, prepare)
                } else {
                    provider.prepare_confirmed_metadata_members_refresh_v48(request, prepare)
                }
            }
            _ => Err(failure()),
        }
    }
}
