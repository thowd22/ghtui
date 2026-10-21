//! Same PrPlan/journal lifecycle, closed Original protocol mode. No separate
//! Catalogue/Personal write algorithm, reset pool, restored approval or retry.
use crate::{
    drafts::{DraftClient, DraftPublicationFenceV2},
    pr_actions::{PrOperation, PrPlan},
    pr_write_journal::{
        conversation_history_v17::ConversationOriginSeedV17, PrConfirmedEffect, PrEffectKind,
        PrPublicationReference, PrWriteJournal, PrWriteState,
    },
    providers::{
        gh::{
            ConversationPublicationFailureV17, ConversationReceiptV17, GithubError,
            GithubErrorKind, GithubProvider, GithubResult, HostIdentity,
        },
        runner::CancellationToken,
    },
    scoped_pr_collaboration_draft_v17::{encode_draft_v17, ConversationDraftIntentV17},
    storage::Cache,
};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Instant,
};

pub(crate) struct ConversationPreviewV17 {
    intent: Arc<ConversationDraftIntentV17>,
    values: Arc<BTreeMap<String, String>>,
    operation: PrOperation,
    seed: ConversationOriginSeedV17,
    lineage_seed_v18: Option<
        crate::pr_write_journal::conversation_action_lineage_v18::ConversationActionOriginSeedV18,
    >,
    lines: Arc<[String]>,
    display: Arc<crate::prepared_form_text_v1::PreparedFormTextV1>,
    identity: HostIdentity,
    useful: AtomicBool,
    used: AtomicBool,
}
impl std::fmt::Debug for ConversationPreviewV17 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "ConversationPreviewV17(<fresh same-original read; explicit Confirm still required>)",
        )
    }
}
fn refused() -> GithubError {
    GithubError {
        kind: GithubErrorKind::IdentityChanged,
        message: "Original Conversation draft/source/approval changed; no publication or fallback"
            .into(),
    }
}
fn postwrite_local_unavailable_v19() -> GithubError {
    GithubError {
        kind: GithubErrorKind::Unsupported,
        message: "Durably confirmed Conversation local projection/draft cleanup unavailable; original reference retained, never resend".into(),
    }
}
fn check(cancel: &CancellationToken, deadline: Instant) -> GithubResult<()> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        Err(refused())
    } else {
        Ok(())
    }
}
fn draft_matches(
    database: &Path,
    intent: &ConversationDraftIntentV17,
    encoded: &str,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<()> {
    check(cancel, deadline)?;
    if Cache::open(database)
        .map_err(|_| refused())?
        .bounded_draft(intent.source.opening().scope(), &intent.key, 128 * 1024)
        .map_err(|_| refused())?
        .as_deref()
        != Some(encoded)
    {
        return Err(refused());
    }
    check(cancel, deadline)
}
impl ConversationPreviewV17 {
    #[cfg(test)]
    pub(crate) fn origin_seed_v18(&self) -> &ConversationOriginSeedV17 {
        &self.seed
    }
    pub(crate) fn intent(&self) -> &Arc<ConversationDraftIntentV17> {
        &self.intent
    }
    pub(crate) fn values(&self) -> &Arc<BTreeMap<String, String>> {
        &self.values
    }
    pub(crate) fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub(crate) fn lines(&self) -> &[String] {
        &self.lines
    }
    pub(crate) fn display(&self) -> &Arc<crate::prepared_form_text_v1::PreparedFormTextV1> {
        &self.display
    }
    pub(crate) fn usefully_painted_v18(&self) -> bool {
        self.useful.load(Ordering::Acquire)
    }
    /// Actual current useful prepared-Doc paint, not Preview completion alone.
    pub(crate) fn painted_v17(&self, useful: bool) {
        self.useful.store(useful, Ordering::Release);
    }
    pub(crate) fn prepare(
        intent: Arc<ConversationDraftIntentV17>,
        values: Arc<BTreeMap<String, String>>,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Arc<Self>> {
        check(cancel, deadline)?;
        if intent.needs_new_v18 {
            return Err(refused());
        }
        encode_draft_v17(&values, None).map_err(|_| refused())?;
        let allowed = crate::pr_actions::fields_v17(
            intent.command,
            intent.selected.as_ref(),
            &intent.reactions,
            intent.source.identity().account_id,
            intent.subscription.as_ref(),
        );
        if allowed.len() != values.len() || allowed.iter().any(|f| !values.contains_key(f.key)) {
            return Err(refused());
        }
        let operation = PrOperation::from_values_v17(
            intent.command,
            intent.selected.as_ref(),
            &intent.reactions,
            intent.source.identity().account_id,
            &values,
        )
        .map_err(|_| refused())?;
        let (seed, lineage_seed_v18, lines, display, identity) =
            intent
                .source
                .with_phase_v17(provider, database, cancel, deadline, None, |phase| {
                    let local = phase.local(database)?;
                    let actual = phase.observe(&operation)?;
                    if let Some(current) = actual.subscription() {
                        if intent
                            .subscription
                            .as_ref()
                            .is_none_or(|old| !old.same_observation(current.current()))
                        {
                            return Err(refused());
                        }
                    }
                    let seed = ConversationOriginSeedV17::from_producer(
                        &intent.source,
                        &actual,
                        &operation,
                        database,
                    )
                    .map_err(|_| refused())?;
                    let lineage_seed=if let Some(revision)=&intent.lineage_v18 {
                        PrWriteJournal::open(database).map_err(|_|refused())?
                            .validate_conversation_revision_v18(&intent.action_base_v18,revision).map_err(|_|refused())?;
                        Some(crate::pr_write_journal::conversation_action_lineage_v18::ConversationActionOriginSeedV18::from_revision(&seed,&intent.action_base_v18,revision).map_err(|_|refused())?)
                    }else{None};
                    if let Some(lineage)=&lineage_seed {lineage.write(&values)}else{seed.write(&values)}.map_err(|_|refused())?;
                    // Raw input escaped ONLY for display. Actual typed request
                    // values remain exact, including controls/newlines/Unicode.
                    let mut lines = intent.original_lines.iter().cloned().collect::<Vec<_>>();
                    if let Some(comment) = actual.comment() {
                        lines.push(format!(
                            "BEFORE • actual comment #{} • author account {}",
                            comment.observed().id,
                            crate::scoped_actions_workflow_ui_v6::account_caption_v25(comment.author_account())
                        ));
                        lines.extend(
                            comment
                                .observed()
                                .body
                                .lines()
                                .map(|s| crate::git::display_bytes(s.as_bytes())),
                        );
                    }
                    lines.push("REQUESTED EFFECT • nothing sent before explicit Confirm".into());
                    for (key, value) in values.iter() {
                        lines.push(format!("{}:", key));
                        lines.extend(
                            value
                                .lines()
                                .map(|s| crate::git::display_bytes(s.as_bytes())),
                        );
                    }
                    if lines.len() > 50_000
                        || lines
                            .iter()
                            .try_fold(0usize, |n, s| n.checked_add(s.len()))
                            .is_none_or(|n| n > 8 * 1024 * 1024)
                    {
                        return Err(refused());
                    }
                    let display = crate::prepared_form_text_v1::PreparedFormTextV1::prepare(
                        &lines, cancel, deadline,
                    )
                    .map_err(|_| refused())?;
                    let lines: Arc<[String]> = lines.into();
                    let identity = phase.finish(|| {
                        local.validate_boundary_v13(database, intent.source.local_boundary())?;
                        if let Some(revision)=&intent.lineage_v18 {
                            PrWriteJournal::open(database).map_err(|_|refused())?
                                .validate_conversation_revision_v18(&intent.action_base_v18,revision).map_err(|_|refused())?;
                        }
                        Ok(())
                    })?;
                    Ok((seed, lineage_seed, lines, display, identity))
                })?;
        Ok(Arc::new(Self {
            intent,
            values,
            operation,
            seed,
            lineage_seed_v18,
            lines,
            display,
            identity,
            useful: AtomicBool::new(false),
            used: AtomicBool::new(false),
        }))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConversationPublicationStateV17 {
    NotStartedProtected,
    Uncertain,
    Confirmed,
    ConfirmedVisibilityUnavailable,
    ConfirmedJournalUnavailable,
}
/// Safe outcome metadata plus a CLOSED actual fresh page/commit result or
/// actual own-fence witness. Neither can be reconstructed from receipt/history
/// IDs; an unknown ACK never says that nothing persisted.
pub(crate) struct ConversationPublicationV17 {
    pub(crate) observation: &'static str,
    pub(crate) state: ConversationPublicationStateV17,
    pub(crate) reference: PrPublicationReference,
    pub(crate) effect: Option<PrConfirmedEffect>,
    pub(crate) draft_cleanup: DraftCleanupV85,
    pub(crate) refreshed_page_v19:
        Option<crate::scoped_pr_collaboration_refresh_v19::AdmittedConversationPageV19>,
    pub(crate) return_v19: Option<
        crate::scoped_pr_collaboration_refresh_v19::return_v19::PreparedConversationReturnV19,
    >,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DraftCleanupV85 {
    Retained,
    Removed,
    Unknown,
}
impl ConversationPublicationV17 {
    #[cfg(test)]
    pub(crate) fn draft_retained(&self) -> bool {
        match self.draft_cleanup {
            DraftCleanupV85::Retained => true,
            DraftCleanupV85::Removed => false,
            DraftCleanupV85::Unknown => panic!(
                "Cleanup Unknown cannot satisfy an old retained/removed boolean oracle; assert the actual tri-state"
            ),
        }
    }
    pub(crate) fn draft_caption_v85(&self) -> &'static str {
        match self.draft_cleanup {
            // TASK-104: the same three words as the legacy composer outcome.
            DraftCleanupV85::Retained => "draft retained",
            DraftCleanupV85::Removed => "draft deleted after exact ACK cleanup",
            DraftCleanupV85::Unknown => {
                "cleanup unconfirmed (local ACK unknown) • inspect durable state; never resend"
            }
        }
    }
}
fn receipt(receipt: &ConversationReceiptV17) -> PrConfirmedEffect {
    match receipt {
        ConversationReceiptV17::Comment { comment, .. } => PrConfirmedEffect {
            kind: PrEffectKind::Comment,
            resource_id: comment.id.to_string(),
            node_id: Some(comment.node_id.clone()),
        },
        ConversationReceiptV17::Deleted { id, node } => PrConfirmedEffect {
            kind: PrEffectKind::CommentDeleted,
            resource_id: id.to_string(),
            node_id: Some(node.clone()),
        },
        ConversationReceiptV17::Reaction { reaction, .. } => PrConfirmedEffect {
            kind: PrEffectKind::Reaction,
            resource_id: reaction.id.to_string(),
            node_id: Some(reaction.node_id.clone()),
        },
        ConversationReceiptV17::RemovedReaction { id, node } => PrConfirmedEffect {
            kind: PrEffectKind::ReactionRemoved,
            resource_id: id.to_string(),
            node_id: Some(node.clone()),
        },
        ConversationReceiptV17::Subscription { state } => PrConfirmedEffect {
            kind: PrEffectKind::Subscription,
            resource_id: match state {
                crate::providers::gh::Subscription::Subscribed => "subscribed",
                crate::providers::gh::Subscription::Unsubscribed => "unsubscribed",
                crate::providers::gh::Subscription::Ignored => "ignored",
            }
            .into(),
            node_id: None,
        },
    }
}
impl PrPlan {
    /// Native Confirm ONLY. The nonClone fence was captured before accepted
    /// enqueue; this method does not create a fence or infer a missing ACK.
    pub(crate) fn execute_original_conversation_v17(
        preview: &Arc<ConversationPreviewV17>,
        provider: &GithubProvider,
        database: &Path,
        drafts: &DraftClient,
        fence: DraftPublicationFenceV2,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<ConversationPublicationV17> {
        check(cancel, deadline)?;
        if !preview.useful.load(Ordering::Acquire)
            || preview.used.swap(true, Ordering::AcqRel)
            || fence.deadline() != deadline
        {
            return Err(refused());
        }
        let intent = &preview.intent;
        let write = if let Some(lineage) = &preview.lineage_seed_v18 {
            lineage.write(&preview.values)
        } else {
            preview.seed.write(&preview.values)
        }
        .map_err(|_| refused())?;
        intent.source.with_phase_v17(
            provider,
            database,
            cancel,
            deadline,
            Some(&preview.operation),
            |mut phase| {
                let local = phase.local(database)?;
                let actual = phase.observe(&preview.operation)?;
                let fresh_seed = ConversationOriginSeedV17::from_producer(
                    &intent.source,
                    &actual,
                    &preview.operation,
                    database,
                )
                .map_err(|_| refused())?;
                let fresh_lineage=if let Some(revision)=&intent.lineage_v18 {
                    PrWriteJournal::open(database).map_err(|_|refused())?
                        .validate_conversation_revision_v18(&intent.action_base_v18,revision).map_err(|_|refused())?;
                    Some(crate::pr_write_journal::conversation_action_lineage_v18::ConversationActionOriginSeedV18::from_revision(&fresh_seed,&intent.action_base_v18,revision).map_err(|_|refused())?)
                }else{None};
                if (if let Some(lineage)=&fresh_lineage {lineage.write(&preview.values)}else{fresh_seed.write(&preview.values)})
                    .map_err(|_| refused())?
                    .digest()
                    .map_err(|_| refused())?
                    != write.digest().map_err(|_| refused())?
                {
                    return Err(refused());
                }
                let mut captures = phase.take_captures()?;
                let mut journal = PrWriteJournal::open(database).map_err(|_| refused())?;
                let permit = (if let Some(lineage)=&fresh_lineage {
                    journal.begin_conversation_action_v18(local.target_v17(),&write,lineage)
                }else{journal.begin_conversation_v17(local.target_v17(), &write, &fresh_seed)}).map_err(|_|refused())?;
                let reference = permit.marker().publication_reference();
                let encoded =
                    encode_draft_v17(&preview.values, Some(&reference)).map_err(|_| refused())?;
                let begun = local.generation_v17().checked_add(1).ok_or_else(refused)?;
                let claimed = begun.checked_add(1).ok_or_else(refused)?;
                let confirmed = claimed.checked_add(1).ok_or_else(refused)?;
                // SAME shared lifecycle as public legacy PrPlan: durable begin
                // -> actual body/reference save + bounded ACK -> exact claim.
                let ack = match Self::claim_persisted_v17(
                    &mut journal,
                    &permit,
                    local.target_v17(),
                    &write,
                    |marker| {
                        let ack = drafts
                            .save_publication_and_wait_v2(fence, encoded.clone(), cancel)
                            .map_err(|e| e.to_string())?;
                        local
                            .validate_owned_boundary_v17(
                                database,
                                intent.source.local_boundary(),
                                marker,
                                begun,
                                PrWriteState::Pending,
                                None,
                            )
                            .map_err(|e| e.to_string())?;
                        if let Some(lineage)=&fresh_lineage {
                            PrWriteJournal::open(database).map_err(|e|e.to_string())?
                                .validate_owned_conversation_action_v18(lineage,&reference).map_err(|e|e.to_string())?;
                        }
                        draft_matches(database, intent, &encoded, cancel, deadline)
                            .map_err(|e| e.to_string())?;
                        ack.check_current(
                            drafts,
                            database,
                            intent.source.opening().scope(),
                            &intent.key,
                            cancel,
                        )?;
                        Ok(ack)
                    },
                ) {
                    Ok(ack) => ack,
                    Err(_) => {
                        return Ok(ConversationPublicationV17 {
                            state: ConversationPublicationStateV17::NotStartedProtected,
                            observation:
                                "Actual draft acknowledgement/claim unavailable; no invocation",
                            reference,
                            effect: None,
                            draft_cleanup: DraftCleanupV85::Retained,
                            refreshed_page_v19:None,return_v19:None,
                        })
                    }
                };
                let result = phase.publish(
                    provider,
                    &mut captures,
                    &actual,
                    &preview.operation,
                    &ack,
                    drafts,
                    database,
                    &intent.key,
                    || {
                        local.validate_owned_boundary_v17(
                            database,
                            intent.source.local_boundary(),
                            permit.marker(),
                            claimed,
                            PrWriteState::Pending,
                            None,
                        )?;
                        if let Some(lineage)=&fresh_lineage {
                            PrWriteJournal::open(database).map_err(|_|refused())?
                                .validate_owned_conversation_action_v18(lineage,&reference).map_err(|_|refused())?;
                        }
                        draft_matches(database, intent, &encoded, cancel, deadline)?;
                        ack.check_current(
                            drafts,
                            database,
                            intent.source.opening().scope(),
                            &intent.key,
                            cancel,
                        )
                        .map_err(|_| refused())
                    },
                );
                let remote = match result {
                    Ok(remote) => remote,
                    Err(ConversationPublicationFailureV17::NotStarted(error)) => {
                        return Ok(ConversationPublicationV17 {
                            state: ConversationPublicationStateV17::NotStartedProtected,
                            observation: if matches!(error.kind,GithubErrorKind::Permission) {
                                "Actual capability/ownership refused before invocation"
                            } else {
                                "Original source/draft/clock refused before invocation"
                            },
                            reference,
                            effect: None,
                            draft_cleanup: DraftCleanupV85::Retained,
                            refreshed_page_v19:None,return_v19:None,
                        })
                    }
                    Err(ConversationPublicationFailureV17::WriteMayHaveStarted(error)) => {
                        let _ = journal.mark_uncertain(&permit);
                        // Last intentional source retirement; never private
                        // SQL/body/source cleanup after this fence.
                        let _ = crate::pr_projection::invalidate_unresolved_discussions(
                            database,
                            intent.source.identity(),
                            intent.source.parent().summary(),
                            &reference,
                        );
                        return Ok(ConversationPublicationV17 {
                            state: ConversationPublicationStateV17::Uncertain,
                            observation: if matches!(error.kind,GithubErrorKind::Permission) {
                                "Invocation denied/unresolved; inspect exact protected attempt"
                            } else {
                                "Invocation or receipt failed; outcome remains unknown"
                            },
                            reference,
                            effect: None,
                            draft_cleanup: DraftCleanupV85::Retained,
                            refreshed_page_v19:None,return_v19:None,
                        });
                    }
                };
                let effect = receipt(&remote);
                let observation = remote.observed_label_v17();
                if journal.mark_confirmed(&permit, &effect).is_err() {
                    let _ = crate::pr_projection::invalidate_unresolved_discussions(
                        database,
                        intent.source.identity(),
                        intent.source.parent().summary(),
                        &reference,
                    );
                    return Ok(ConversationPublicationV17 {
                        state: ConversationPublicationStateV17::ConfirmedJournalUnavailable,
                        observation,
                        reference,
                        effect: Some(effect),
                        draft_cleanup: DraftCleanupV85::Retained,
                        refreshed_page_v19:None,return_v19:None,
                    });
                }
                let mut draft_cleanup = DraftCleanupV85::Retained;
                // Actual native SAME-source durable receipt only. All raw row,
                // origin, epoch/fence and bounded fallback metadata is captured
                // BEFORE the final postwrite UID, never from saved IDs later.
                let terminal=match journal.prepare_native_conversation_terminal_v19(
                    &intent.source,&actual,&permit,&local,confirmed,&effect,
                ) {Ok(terminal)=>terminal,Err(_)=>{
                    // Receipt is ALREADY durably Confirmed. A later local
                    // preparation refusal must not be mislabeled account loss
                    // or make that real effect eligible for resend.
                    let _=crate::pr_projection::invalidate_confirmed_discussions(database,actual.identity(),intent.source.parent().summary(),&reference);
                    return Ok(ConversationPublicationV17 {observation,state:ConversationPublicationStateV17::ConfirmedVisibilityUnavailable,reference,effect:Some(effect),draft_cleanup:DraftCleanupV85::Retained,refreshed_page_v19:None,return_v19:None});
                }};
                let fallback=crate::storage::conversation_refresh_v19::PreparedConversationOwnFenceV19::prepare(
                    Arc::clone(&terminal),cancel,deadline,
                ).ok();
                let return_prepared=crate::scoped_pr_collaboration_refresh_v19::return_v19::PreparedConversationReturnV19::prepare(&intent.source,&terminal,actual.identity(),cancel,deadline).ok();
                let cleanup_cutoff=deadline.min(captures.deadline()).min(actual.publication_cutoff_v51());
                let closed = provider
                    .prepare_confirmed_refresh_v19(
                        &actual,
                        intent.source.current()?,
                        &mut captures,
                        cancel,
                        deadline,
                        |snapshot,preparation| {
                            // This callback performs LOCAL preparation/SQL/
                            // ACK checks only. Its refusal is not an observed
                            // account change. Actual provider UID/repo/parent
                            // checks outside this closure retain their exact
                            // Authentication/IdentityChanged privacy meaning.
                            (|| {
                            let binding=ack.atomic_charge_v85(drafts,crate::drafts::AtomicDraftInputV85{
                                database,scope:intent.source.opening().scope(),resource:&intent.key,
                                encoded:&encoded,reference:&reference,terminal:&terminal,
                            },cancel).map_err(|_|refused())?;
                            let prepared=crate::scoped_pr_collaboration_refresh_v19::PreparedConversationPageV19::prepare_publication_atomic_v85(
                                &intent.source,Arc::clone(&terminal),snapshot,
                                crate::providers::gh::ConversationPageSupplementV61::None,preparation,binding,cancel,
                            )?;
                            local.validate_owned_boundary_v17(
                                database,
                                intent.source.local_boundary(),
                                permit.marker(),
                                confirmed,
                                PrWriteState::Confirmed,
                                Some(&effect),
                            )?;
                            if let Some(lineage)=&fresh_lineage {
                                journal.validate_owned_conversation_action_v18(lineage,&reference).map_err(|_|refused())?;
                            }
                            draft_matches(database, intent, &encoded, cancel, deadline)?;
                            ack.check_current(
                                drafts,
                                database,
                                intent.source.opening().scope(),
                                &intent.key,
                                cancel,
                            )
                            .map_err(|_| refused())?;
                            terminal.validate_current_v19(&journal).map_err(|_|refused())?;
                            Ok::<_, GithubError>(prepared)
                            })().map_err(|_|postwrite_local_unavailable_v19())
                        },
                    );
                // ONE exact raw terminal/source/Clear CAS performs the SAME
                // category fence THEN preencoded fresh rows/bindings and local
                // successor in one transaction. No invalidation after it can
                // erase the just-published frame. EMPTY effect stays EMPTY.
                // A genuine authentication/principal loss never retains an old
                // private Workspace merely because a local own-fence is valid.
                let account_lost_v19=closed.as_ref().err().is_some_and(|error|matches!(error.kind,GithubErrorKind::Authentication|GithubErrorKind::IdentityChanged));
                let refreshed_page_v19=match closed {
                    Ok(closed)=>match crate::scoped_pr_collaboration_refresh_v19::PreparedConversationPageV19::admit_publication_atomic_v85(closed,drafts,ack,cancel,cleanup_cutoff) {
                        Ok(page)=>{draft_cleanup=DraftCleanupV85::Removed;Some(page)},
                        Err(error)=>{if error.unknown(){draft_cleanup=DraftCleanupV85::Unknown;}None},
                    },
                    Err(_)=>None,
                };
                let own_fence_v19=if refreshed_page_v19.is_none() && draft_cleanup!=DraftCleanupV85::Unknown {fallback.and_then(|f|f.commit(cancel,deadline).ok())}else{None};
                let return_v19=match (return_prepared,own_fence_v19.as_ref()) {(Some(prepared),Some(own)) if !account_lost_v19=>prepared.bind(Arc::clone(own)).ok(),_=>None};
                Ok(ConversationPublicationV17 {
                    observation,
                    state: if refreshed_page_v19.is_some() {
                        ConversationPublicationStateV17::Confirmed
                    } else {
                        ConversationPublicationStateV17::ConfirmedVisibilityUnavailable
                    },
                    reference,
                    effect: Some(effect),
                    draft_cleanup,
                    refreshed_page_v19,return_v19,
                })
            },
        )
    }
}
