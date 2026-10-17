//! Metadata purpose of the SAME PrPlan durable lifecycle. The only remote
//! invocation is the closed original-field producer; all twelve receipt/page
//! children spend the original prepaid cursor exactly once.
use super::*;
use crate::{
    drafts::{DraftClient, DraftPublicationFenceV2},
    pr_actions::PrPlan,
    pr_write_journal::{self, PrWriteJournal, PrWriteState},
    scoped_pr_collaboration_plan_v17::{
        ConversationPublicationStateV17 as State, ConversationPublicationV17 as Outcome,
        DraftCleanupV85,
    },
};
#[path = "scoped_pr_metadata_publication_purpose_v48.rs"]
mod purpose_v48;
use purpose_v48::{
    FailureV48, ProjectionInvalidationKindV74, ProjectionInvalidationV74, PublicationAckV48,
    PurposeV48,
};

fn draft_matches(
    database: &Path,
    purpose: &PurposeV48<'_>,
    encoded: &str,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<()> {
    check(cancel, deadline)?;
    if Cache::open(database)
        .map_err(|_| failure())?
        .bounded_draft(
            purpose.source().opening().scope(),
            purpose.key(),
            128 * 1024,
        )
        .map_err(|_| failure())?
        .as_deref()
        != Some(encoded)
    {
        return Err(failure());
    }
    check(cancel, deadline)
}
fn protected(
    reference: PrPublicationReference,
    state: State,
    observation: &'static str,
    effect: Option<pr_write_journal::PrConfirmedEffect>,
) -> Outcome {
    Outcome {
        observation,
        state,
        reference,
        effect,
        draft_cleanup: DraftCleanupV85::Retained,
        refreshed_page_v19: None,
        return_v19: None,
    }
}
fn local_post_failure() -> GithubError {
    GithubError { kind: GithubErrorKind::Unsupported,
        message: "Confirmed metadata local cleanup/page unavailable; protected receipt retained, never resend".into() }
}
impl PrPlan {
    pub(crate) fn execute_original_issue_links_v87(
        preview: &Arc<crate::scoped_pr_issue_links_v87::IssueLinkPreviewV87>,
        provider: &GithubProvider,
        database: &Path,
        drafts: &DraftClient,
        fence: DraftPublicationFenceV2,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Outcome> {
        execute_metadata_v48(
            PurposeV48::IssueLinksV87(preview),
            provider,
            database,
            drafts,
            fence,
            cancel,
            deadline,
        )
    }

    pub(crate) fn execute_original_state_v68(
        preview: &Arc<crate::scoped_pr_collaboration_ui_v17::state_v68::StatePreviewV68>,
        provider: &GithubProvider,
        database: &Path,
        drafts: &DraftClient,
        fence: DraftPublicationFenceV2,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Outcome> {
        execute_metadata_v48(
            PurposeV48::StateV68(preview),
            provider,
            database,
            drafts,
            fence,
            cancel,
            deadline,
        )
    }
    pub(crate) fn execute_original_metadata_text_v25(
        preview: &Arc<MetadataPreviewV25>,
        provider: &GithubProvider,
        database: &Path,
        drafts: &DraftClient,
        fence: DraftPublicationFenceV2,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Outcome> {
        execute_metadata_v48(
            PurposeV48::Text(preview),
            provider,
            database,
            drafts,
            fence,
            cancel,
            deadline,
        )
    }
    pub(crate) fn execute_original_metadata_members_v48(
        preview: &Arc<
            crate::scoped_pr_collaboration_ui_v17::metadata_members_v48::MetadataMembersPreviewV48,
        >,
        provider: &GithubProvider,
        database: &Path,
        drafts: &DraftClient,
        fence: DraftPublicationFenceV2,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Outcome> {
        execute_metadata_v48(
            PurposeV48::Members(preview),
            provider,
            database,
            drafts,
            fence,
            cancel,
            deadline,
        )
    }
}
fn execute_metadata_v48(
    purpose: PurposeV48<'_>,
    provider: &GithubProvider,
    database: &Path,
    drafts: &DraftClient,
    fence: DraftPublicationFenceV2,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<Outcome> {
    check(cancel, deadline)?;
    if !purpose.consume() || !purpose.is_state_v68() && fence.deadline() != deadline {
        return Err(failure());
    }
    let deadline = deadline.min(fence.deadline());
    let write = purpose.write()?;
    purpose.with_phase(provider, database, cancel, deadline, |mut phase| {
                let deadline = deadline.min(phase.deadline());
                let local = phase.local(database)?;
                let actual = phase.actual()?;
                let fresh = purpose.seed(&phase,&actual, database)?;
                if fresh.write(purpose.values())?.digest().map_err(|_| failure())?
                    != write.digest().map_err(|_| failure())? { return Err(failure()); }
                let mut captures = phase.take_captures()?;
                let mut journal = PrWriteJournal::open(database).map_err(|_| failure())?;
                let permit = fresh.begin(&mut journal, local.target_v17(), &write)?;
                let reference = permit.marker().publication_reference();
                let encoded = encode_draft_v17(purpose.values(), Some(&reference)).map_err(|_| failure())?;
                let begun = local.generation_v17().checked_add(1).ok_or_else(failure)?;
                let claimed = begun.checked_add(1).ok_or_else(failure)?;
                let confirmed = claimed.checked_add(1).ok_or_else(failure)?;
                let ack = match PrPlan::claim_persisted_v17(&mut journal, &permit,
                    local.target_v17(), &write, |marker| {
                        let ack = drafts.save_publication_and_wait_v2(fence, encoded.clone(), cancel).map_err(|e| e.to_string())?;
                        local.validate_owned_boundary_v17(database, purpose.source().local_boundary(),
                            marker, begun, PrWriteState::Pending, None).map_err(|e| e.to_string())?;
                        draft_matches(database, &purpose, &encoded, cancel, deadline).map_err(|e| e.to_string())?;
                        ack.check_current(drafts, database, purpose.source().opening().scope(), purpose.key(), cancel)?;
                        Ok(ack)
                    }) {
                    Ok(ack) => ack,
                    Err(_) => return Ok(protected(reference, State::NotStartedProtected,
                        "Metadata body/reference acknowledgement or claim unavailable; not invoked", None)),
                };
                let result = phase.publish(&purpose, provider, &mut captures, PublicationAckV48 { permit:&permit,ack: &ack, drafts, database }, || {
                        local.validate_owned_boundary_v17(database, purpose.source().local_boundary(),
                            permit.marker(), claimed, PrWriteState::Pending, None)?;
                        draft_matches(database, &purpose, &encoded, cancel, deadline)?;
                        fresh.validate_owned_v52(&journal,&reference)?;
                        ack.check_current(drafts, database, purpose.source().opening().scope(), purpose.key(), cancel).map_err(|_| failure())
                    });
                let remote = match result {
                    Ok(receipt) => receipt,
                    Err(FailureV48::NotStarted(error)) => return Ok(protected(reference,
                        State::NotStartedProtected, if matches!(error.kind, GithubErrorKind::Permission) {
                            "Actual metadata permission refused before invocation"
                        } else { "Exact original metadata/source/ACK refused before invocation" }, None)),
                    Err(FailureV48::WriteMayHaveStarted(error)) => {
                        let uncertain_generation=if journal.mark_uncertain(&permit).is_ok() {confirmed}else{claimed};
                        // Terminal fence is LAST: no private reload/SQL after it.
                        let _ = fresh.invalidate_projection_v74(ProjectionInvalidationV74 {purpose:&purpose,actual:&actual,journal:&journal,local:&local,reference:&reference,generation:uncertain_generation,cancel,deadline,kind:ProjectionInvalidationKindV74::Unresolved});
                        return Ok(protected(reference, State::Uncertain,
                            if matches!(error.kind, GithubErrorKind::Permission) { "Metadata invocation denied/unresolved; inspect exact original, never resend" }
                            else { "Metadata invocation or receipt unresolved; inspect exact original, never resend" }, None));
                    }
                };
                if !fresh.matches_receipt(&remote) {
                    let uncertain_generation=if journal.mark_uncertain(&permit).is_ok() {confirmed}else{claimed};
                    let _ = fresh.invalidate_projection_v74(ProjectionInvalidationV74 {purpose:&purpose,actual:&actual,journal:&journal,local:&local,reference:&reference,generation:uncertain_generation,cancel,deadline,kind:ProjectionInvalidationKindV74::Unresolved});
                    return Ok(protected(reference, State::Uncertain,
                        "Actual metadata receipt differs from requested origin; inspect original, never resend", None));
                }
                let effect = remote.effect();
                let observation = purpose.observation();
                if fresh.confirm_v68(&mut journal,&permit,&remote,claimed,cancel,deadline).is_err() {
                    let _ = fresh.invalidate_projection_v74(ProjectionInvalidationV74 {purpose:&purpose,actual:&actual,journal:&journal,local:&local,reference:&reference,generation:claimed,cancel,deadline,kind:ProjectionInvalidationKindV74::Unresolved});
                    return Ok(protected(reference, State::ConfirmedJournalUnavailable, observation, Some(effect)));
                }
                let terminal = match remote.terminal(purpose_v48::TerminalPreparationV48 { journal: &journal, provider, source: purpose.source(), actual: &actual,
                    purpose: &purpose, permit: &permit, local: &local, generation: confirmed, effect: &effect, cancel }) {
                    Ok(witness) => witness,
                    Err(_) => {
                        let _ = fresh.invalidate_projection_v74(ProjectionInvalidationV74 {purpose:&purpose,actual:&actual,journal:&journal,local:&local,reference:&reference,generation:confirmed,cancel,deadline,kind:ProjectionInvalidationKindV74::Confirmed});
                        return Ok(protected(reference, State::ConfirmedVisibilityUnavailable, observation, Some(effect)));
                    }
                };
                let fallback = crate::storage::conversation_refresh_v19::PreparedConversationOwnFenceV19::prepare(
                    Arc::clone(&terminal), cancel, deadline).ok();
                let return_prepared = crate::scoped_pr_collaboration_refresh_v19::return_v19::PreparedConversationReturnV19::prepare(
                    purpose.source(), &terminal, actual.identity(), cancel, deadline).ok();
                let mut draft_cleanup = DraftCleanupV85::Retained;
                let cleanup_cutoff=deadline.min(captures.deadline());
                let closed = remote.refresh(purpose_v48::ConfirmedPageV48 { provider, actual: &actual, purpose: &purpose, captures: &mut captures, cancel, deadline }, |snapshot, supplement, preparation| {
                        (|| {
                            let binding=ack.atomic_charge_v85(drafts,crate::drafts::AtomicDraftInputV85{
                                database,scope:purpose.source().opening().scope(),resource:purpose.key(),encoded:&encoded,reference:&reference,terminal:&terminal,
                            },cancel).map_err(|_|failure())?;
                            let prepared = crate::scoped_pr_collaboration_refresh_v19::PreparedConversationPageV19::prepare_publication_atomic_v85(
                                purpose.source(), Arc::clone(&terminal), snapshot, supplement, preparation,binding, cancel)?;
                            local.validate_owned_boundary_v17(database, purpose.source().local_boundary(),
                                permit.marker(), confirmed, PrWriteState::Confirmed, Some(&effect))?;
                            draft_matches(database, &purpose, &encoded, cancel, deadline)?;
                            ack.check_current(drafts, database, purpose.source().opening().scope(), purpose.key(), cancel).map_err(|_| failure())?;
                            terminal.validate_current_v19(&journal).map_err(|_| failure())?;
                            Ok::<_, GithubError>(prepared)
                        })().map_err(|_| local_post_failure())
                    });
                let account_lost = closed.as_ref().err().is_some_and(|e| matches!(e.kind,
                    GithubErrorKind::Authentication | GithubErrorKind::IdentityChanged));
                let refreshed_page_v19=match closed {
                    Ok(closed)=>match crate::scoped_pr_collaboration_refresh_v19::PreparedConversationPageV19::admit_publication_atomic_v85(closed,drafts,ack,cancel,cleanup_cutoff) {
                        Ok(page)=>{draft_cleanup=DraftCleanupV85::Removed;Some(page)},
                        Err(error)=>{if error.unknown(){draft_cleanup=DraftCleanupV85::Unknown;}None},
                    },Err(_)=>None,
                };
                let own_fence = if refreshed_page_v19.is_none() && draft_cleanup!=DraftCleanupV85::Unknown { fallback.and_then(|f| f.commit(cancel, deadline).ok()) } else { None };
                let return_v19 = match (return_prepared, own_fence.as_ref()) {
                    (Some(prepared), Some(own)) if !account_lost => prepared.bind(Arc::clone(own)).ok(), _ => None,
                };
                Ok(Outcome { observation, state: if refreshed_page_v19.is_some() { State::Confirmed } else { State::ConfirmedVisibilityUnavailable },
                    reference, effect: Some(effect), draft_cleanup, refreshed_page_v19, return_v19 })
            })
}
