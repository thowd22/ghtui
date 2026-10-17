//! Sole Base publisher: exact ACK and original SQL close precede one PATCH;
//! both raw receipts seal B0→B1 before either comparison fence can be prepared.
use super::*;
use crate::{
    drafts::{DraftClient, DraftPublicationFenceV2},
    pr_actions::PrPlan,
    pr_write_journal::{self, PrWriteJournal, PrWriteState},
    providers::gh::PrMetadataBaseFailureV32,
    scoped_pr_collaboration_plan_v17::{
        ConversationPublicationStateV17 as State, ConversationPublicationV17 as Outcome,
        DraftCleanupV85,
    },
};
#[cfg(test)]
type AfterConfirmedHookV32 = Option<Box<dyn FnOnce(i64)>>;
#[cfg(test)]
std::thread_local! {static AFTER_CONFIRMED_V32:std::cell::RefCell<AfterConfirmedHookV32>=std::cell::RefCell::new(None);}
#[cfg(test)]
pub(crate) fn observe_next_actual_base_confirmed_v32(observer: impl FnOnce(i64) + 'static) {
    AFTER_CONFIRMED_V32.with(|slot| {
        assert!(slot.borrow().is_none());
        *slot.borrow_mut() = Some(Box::new(observer));
    });
}
#[cfg(test)]
fn after_actual_confirmed_v32(attempt: i64) {
    AFTER_CONFIRMED_V32.with(|slot| {
        let observer = slot.borrow_mut().take();
        if let Some(observer) = observer {
            observer(attempt);
        }
    });
}
fn draft_matches(
    database: &Path,
    intent: &BaseIntentV32,
    encoded: &str,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<()> {
    check(cancel, deadline)?;
    if Cache::open(database)
        .map_err(|_| failure())?
        .bounded_draft(intent.source.opening().scope(), &intent.key, 128 * 1024)
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
fn post_failure() -> GithubError {
    GithubError { kind: GithubErrorKind::Unsupported, message: "Confirmed Base local cleanup/fresh page unavailable; protected receipt retained, never resend".into() }
}
impl PrPlan {
    pub(crate) fn execute_original_metadata_base_v32(
        preview: &Arc<BasePreviewV32>,
        provider: &GithubProvider,
        database: &Path,
        drafts: &DraftClient,
        fence: DraftPublicationFenceV2,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Outcome> {
        check(cancel, deadline)?;
        if !preview.useful.load(Ordering::Acquire)
            || preview.used.swap(true, Ordering::AcqRel)
            || fence.deadline() != deadline
        {
            return Err(failure());
        }
        let intent = &preview.intent;
        let write = preview.seed.write(&preview.values).map_err(|_| failure())?;
        intent.source.with_metadata_base_phase_v32(provider,database,cancel,deadline,&preview.selection,true,|mut phase| {
            let local=phase.local(database)?;
            let actual=phase.actual().clone();
            let fresh=BaseOriginSeedV32::from_producer(&intent.source,&actual,&preview.selection,database).map_err(|_|failure())?;
            if fresh.write(&preview.values).map_err(|_|failure())?.digest().map_err(|_|failure())? != write.digest().map_err(|_|failure())? { return Err(failure()); }
            let mut captures=phase.take_captures()?;
            let mut journal=PrWriteJournal::open(database).map_err(|_|failure())?;
            let permit=journal.begin_metadata_base_v32(local.target_v17(),&write,&fresh).map_err(|_|failure())?;
            let reference=permit.marker().publication_reference();
            let encoded=crate::scoped_pr_collaboration_draft_v17::encode_base_draft_v32(&preview.values,Some(&reference)).map_err(|_|failure())?;
            let begun=local.generation_v17().checked_add(1).ok_or_else(failure)?;
            let claimed=begun.checked_add(1).ok_or_else(failure)?;
            let confirmed=claimed.checked_add(1).ok_or_else(failure)?;
            let ack=match Self::claim_persisted_v17(&mut journal,&permit,local.target_v17(),&write,|marker| {
                let ack=drafts.save_publication_and_wait_v2(fence,encoded.clone(),cancel).map_err(|e|e.to_string())?;
                local.validate_owned_boundary_v17(database,intent.source.local_boundary(),marker,begun,PrWriteState::Pending,None).map_err(|e|e.to_string())?;
                draft_matches(database,intent,&encoded,cancel,deadline).map_err(|e|e.to_string())?;
                ack.check_current(drafts,database,intent.source.opening().scope(),&intent.key,cancel)?;
                Ok(ack)
            }) { Ok(ack)=>ack, Err(_)=>return Ok(protected(reference,State::NotStartedProtected,"Base draft/reference ACK or claim unavailable; not invoked",None)) };
            let result=phase.publish(provider,&mut captures,&ack,drafts,database,&intent.key,|| {
                local.validate_owned_boundary_v17(database,intent.source.local_boundary(),permit.marker(),claimed,PrWriteState::Pending,None)?;
                draft_matches(database,intent,&encoded,cancel,deadline)?;
                ack.check_current(drafts,database,intent.source.opening().scope(),&intent.key,cancel).map_err(|_|failure())
            });
            let remote=match result {
                Ok(receipt)=>receipt,
                Err(PrMetadataBaseFailureV32::NotStarted(error))=>return Ok(protected(reference,State::NotStartedProtected,
                    if matches!(error.kind,GithubErrorKind::Permission) { "Actual Base update permission refused before invocation" }
                    else { "Exact original Base/capability/selected tip/ACK refused before invocation" },None)),
                Err(PrMetadataBaseFailureV32::WriteMayHaveStarted(error))=> {
                    let _=journal.mark_uncertain(&permit);
                    let _=crate::pr_projection::invalidate_unresolved_discussions(database,actual.identity(),intent.source.parent().summary(),&reference);
                    return Ok(protected(reference,State::Uncertain,
                        if matches!(error.kind,GithubErrorKind::Permission) { "Base invocation denied/unresolved; inspect exact original, never resend" }
                        else { "Base invocation/receipt unresolved; inspect exact original, never resend" },None));
                }
            };
            if !fresh.matches_receipt(&remote) {
                let _=journal.mark_uncertain(&permit);
                let _=crate::pr_projection::invalidate_unresolved_discussions(database,actual.identity(),intent.source.parent().summary(),&reference);
                return Ok(protected(reference,State::Uncertain,"Base receipt differs from actual selected origin; never resend",None));
            }
            let effect=pr_write_journal::metadata_base_origin_v32::effect(&remote);
            let observation="Exact Base B1/ref observed independently by PATCH and GET; observation is not causal proof";
            if journal.mark_confirmed(&permit,&effect).is_err() {
                let _=crate::pr_projection::invalidate_unresolved_discussions(database,actual.identity(),intent.source.parent().summary(),&reference);
                return Ok(protected(reference,State::ConfirmedJournalUnavailable,observation,Some(effect)));
            }
            #[cfg(test)]
            after_actual_confirmed_v32(permit.marker().attempt_id);
            let terminal=match journal.prepare_native_base_terminal_v32(&intent.source,&actual,&preview.selection,&remote,&permit,&local,confirmed,&effect,cancel) {
                Ok(t)=>t,
                Err(_)=> {
                    let _=crate::pr_projection::invalidate_confirmed_discussions(database,actual.identity(),intent.source.parent().summary(),&reference);
                    return Ok(protected(reference,State::ConfirmedVisibilityUnavailable,observation,Some(effect)));
                }
            };
            let fallback=crate::storage::conversation_refresh_v19::PreparedConversationOwnFenceV19::prepare(Arc::clone(&terminal),cancel,deadline).ok();
            let return_prepared=crate::scoped_pr_collaboration_refresh_v19::return_v19::PreparedConversationReturnV19::prepare(&intent.source,&terminal,actual.identity(),cancel,deadline).ok();
            let mut draft_cleanup=DraftCleanupV85::Retained;
            let cleanup_cutoff=deadline.min(captures.deadline()).min(actual.deadline()).min(remote.publication_cutoff_v51());
            let prepare = |snapshot, metadata:Option<crate::providers::gh::ActualMetadataTabContextV51>, preparation:&crate::providers::gh::ConversationRefreshPreparationV19| {
                (|| {
                    let binding=ack.atomic_charge_v85(drafts,crate::drafts::AtomicDraftInputV85{
                        database,scope:intent.source.opening().scope(),resource:&intent.key,encoded:&encoded,reference:&reference,terminal:&terminal,
                    },cancel).map_err(|_|failure())?;
                    let supplement=if let Some(context)=metadata {
                        crate::providers::gh::ConversationPageSupplementV61::metadata_v79(&snapshot,context)?
                    }else{crate::providers::gh::ConversationPageSupplementV61::None};
                    let prepared=crate::scoped_pr_collaboration_refresh_v19::PreparedConversationPageV19::prepare_publication_atomic_v85(
                        &intent.source,Arc::clone(&terminal),snapshot,supplement,preparation,binding,cancel)?;
                    local.validate_owned_boundary_v17(database,intent.source.local_boundary(),permit.marker(),confirmed,PrWriteState::Confirmed,Some(&effect))?;
                    draft_matches(database,intent,&encoded,cancel,deadline)?;
                    ack.check_current(drafts,database,intent.source.opening().scope(),&intent.key,cancel).map_err(|_|failure())?;
                    terminal.validate_current_v19(&journal).map_err(|_|failure())?;
                    Ok::<_,GithubError>(prepared)
                })().map_err(|_|post_failure())
            };
            let closed = if intent.source.opening().metadata_tab_v51() {
                provider.prepare_confirmed_base_metadata_refresh_v51(&actual,&preview.selection,&remote,intent.source.current()?,&mut captures,cancel,deadline,|snapshot,context,preparation|prepare(snapshot,Some(context),preparation))
            } else {
                provider.prepare_confirmed_base_refresh_v32(&actual,&preview.selection,&remote,intent.source.current()?,&mut captures,cancel,deadline,|snapshot,preparation|prepare(snapshot,None,preparation))
            };
            let account_lost=closed.as_ref().err().is_some_and(|e|matches!(e.kind,GithubErrorKind::Authentication|GithubErrorKind::IdentityChanged));
            let refreshed_page_v19=match closed {
                Ok(closed)=>match crate::scoped_pr_collaboration_refresh_v19::PreparedConversationPageV19::admit_publication_atomic_v85(closed,drafts,ack,cancel,cleanup_cutoff) {
                    Ok(page)=>{draft_cleanup=DraftCleanupV85::Removed;Some(page)},
                    Err(error)=>{if error.unknown(){draft_cleanup=DraftCleanupV85::Unknown;}None},
                },Err(_)=>None,
            };
            let own_fence=if refreshed_page_v19.is_none() && draft_cleanup!=DraftCleanupV85::Unknown {fallback.and_then(|f|f.commit(cancel,deadline).ok())}else{None};
            let return_v19=match(return_prepared,own_fence.as_ref()){(Some(p),Some(own)) if !account_lost=>p.bind(Arc::clone(own)).ok(),_=>None};
            Ok(Outcome {observation,state:if refreshed_page_v19.is_some(){State::Confirmed}else{State::ConfirmedVisibilityUnavailable},reference,effect:Some(effect),draft_cleanup,refreshed_page_v19,return_v19})
        })
    }
}
