//! Actual native confirmed privacy-fence result, never a journal-only replay,
//! access/current source/ACK/write permit. Its explicit next phase gets a NEW
//! real Core; only this exact local counter transition is explained here.
use super::*;
use crate::scoped_pr_observed_tabs_v1::resources::PrLocalSourceBoundaryV13;
use crate::storage::{
    personal_pr_source_v13::parent_boundary_v16::{
        PersonalConversationFenceV19, PreparedPersonalConversationFenceV19,
    },
    repository_catalogue_intents_v1::conversation_continuation_v19::{
        CatalogueOwnFenceV19, PreparedCatalogueOwnFenceV19,
    },
};
#[path = "personal_retry_v20.rs"]
mod personal_retry_v20;
#[cfg(test)]
pub(crate) use personal_retry_v20::{
    after_actual_reservation_v20, observe_next_actual_reservation_v20,
};
pub(crate) use personal_retry_v20::{
    PersonalConversationReadAttemptV20, PersonalConversationReservationWitnessV20,
};

enum PreparedFamilyV19 {
    Catalogue(PreparedCatalogueOwnFenceV19),
    Personal(PreparedPersonalConversationFenceV19),
}
enum FamilyV19 {
    Catalogue(CatalogueOwnFenceV19),
    Personal(Box<PersonalConversationFenceV19>),
}
pub(crate) struct PreparedConversationOwnFenceV19 {
    cache: Cache,
    terminal: Arc<ConversationTerminalWitnessV19>,
    epoch: EpochV19,
    fence: Option<bindings::PreparedFenceV19>,
    state_fence_v68: Option<state_filter_v68::PreparedStateFenceV68>,
    family: PreparedFamilyV19,
    publication_cutoff_v51: Instant,
}
pub(crate) struct ConversationOwnFenceV19 {
    terminal: Arc<ConversationTerminalWitnessV19>,
    family: FamilyV19,
}
fn fence_plan(
    tx: &Transaction<'_>,
    terminal: &ConversationTerminalWitnessV19,
) -> Result<Option<bindings::PreparedFenceV19>> {
    if terminal.categories().is_empty() {
        return Ok(None);
    }
    Ok(Some(bindings::prepare_fence_v19(
        tx,
        terminal.target().scope(),
        &SyncFence {
            namespace: SyncNamespace::Collaboration,
            repository_id: Some(terminal.repository_id()),
            repository: terminal.target().repository().to_ascii_lowercase(),
            resource: terminal.target().node_id().into(),
            kinds: terminal.categories().to_vec(),
        },
        Some(terminal.reference().attempt_id as u64),
    )?))
}
fn clock(cancel: &CancellationToken, deadline: Instant) -> Result<()> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        bail!("Original Conversation fence/read cancelled or expired");
    }
    Ok(())
}
impl PreparedConversationOwnFenceV19 {
    /// The terminal constructor can only be reached by the actual successful
    /// typed native publisher after its own durable Confirmed transaction.
    pub(crate) fn prepare(
        terminal: Arc<ConversationTerminalWitnessV19>,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<Self> {
        let publication_cutoff_v51 = deadline.min(terminal.publication_cutoff_v51());
        let deadline = publication_cutoff_v51;
        clock(cancel, deadline)?;
        terminal.validate_file_v19()?;
        let cache = Cache::open(terminal.database())?;
        let tx = cache.connection.unchecked_transaction()?;
        terminal.validate_transaction_v19(&tx)?;
        let fence = fence_plan(&tx, &terminal)?;
        let state_fence_v68 = state_filter_v68::PreparedStateFenceV68::prepare(&tx, &terminal)?;
        let fenced = fence.as_ref().is_some_and(|f| f.actually_fences())
            || state_fence_v68
                .as_ref()
                .is_some_and(|f| f.actually_fences());
        let epoch = EpochV19::capture(&tx, &terminal, terminal.source_v19())?;
        tx.commit()?;
        let source = terminal.source_v19();
        let family = if let Some(catalogue) = source.opening().catalogue_source() {
            PreparedFamilyV19::Catalogue(cache.prepare_catalogue_own_fence_v19(catalogue, fenced)?)
        } else if let PrLocalSourceBoundaryV13::PersonalParent(parent) = source.local_boundary() {
            PreparedFamilyV19::Personal(
                cache.prepare_personal_conversation_fence_v19(parent, fenced)?,
            )
        } else {
            bail!("Not an actual native Catalogue/Personal Conversation source");
        };
        let tx = cache.connection.unchecked_transaction()?;
        terminal.validate_transaction_v19(&tx)?;
        epoch.validate(&tx)?;
        tx.commit()?;
        clock(cancel, deadline)?;
        Ok(Self {
            cache,
            terminal,
            epoch,
            fence,
            state_fence_v68,
            family,
            publication_cutoff_v51,
        })
    }
    /// Fixed prepared SQL only, including when a preceding final UID already
    /// succeeded. No raw row/receipt/body parsing/materialization occurs here.
    pub(crate) fn commit(
        mut self,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<Arc<ConversationOwnFenceV19>> {
        let deadline = deadline.min(self.publication_cutoff_v51);
        clock(cancel, deadline)?;
        self.terminal.validate_file_v19()?;
        let _busy = crate::storage::busy_v1::until(&self.cache.connection, deadline)?;
        let tx = self
            .cache
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        clock(cancel, deadline)?;
        self.terminal.validate_file_v19()?;
        self.terminal.validate_transaction_v19(&tx)?;
        self.epoch.validate(&tx)?;
        if let Some(fence) = &self.fence {
            bindings::apply_prepared_fence_v19(&tx, fence)?;
        }
        if let Some(fence) = &self.state_fence_v68 {
            fence.apply(&tx)?;
        }
        let family = match self.family {
            PreparedFamilyV19::Catalogue(v) => FamilyV19::Catalogue(v.bind_committed_v19(&tx)?),
            PreparedFamilyV19::Personal(v) => FamilyV19::Personal(v.bind_committed_v19(&tx)?),
        };
        clock(cancel, deadline)?;
        self.terminal.validate_file_v19()?;
        tx.commit()?;
        Ok(Arc::new(ConversationOwnFenceV19 {
            terminal: self.terminal,
            family,
        }))
    }
}
impl ConversationOwnFenceV19 {
    pub(crate) fn terminal(&self) -> &Arc<ConversationTerminalWitnessV19> {
        &self.terminal
    }
    pub(crate) fn source(&self) -> &Arc<PrCollaborationSourceV17> {
        self.terminal.source_v19()
    }
    fn validate_once_v19(&self, cache: &Cache) -> Result<()> {
        if self.terminal.state_transition_v68().is_some() {
            self.terminal.validate_own_fence_profile_v45()?;
            let tx = cache.connection.unchecked_transaction()?;
            let state = state_filter_v68::PreparedStateFenceV68::prepare(&tx, &self.terminal)?
                .context("Exact State own-fence absent")?;
            if state.actually_fences() {
                bail!("State once-fence moved/missing; old receipt cannot renew it");
            }
            tx.commit()?;
            return Ok(());
        }
        let base = self.terminal.base_transition_v32();
        if let Some(transition) = base {
            if self.terminal.categories()
                != [SyncBindingKind::Review, SyncBindingKind::InlineComment]
                || !Arc::ptr_eq(self.terminal.parent(), transition.parent())
            {
                bail!("Exact typed Base terminal/category/transition differs");
            }
        } else {
            self.terminal.validate_own_fence_profile_v45()?;
        }
        let members_label = self.terminal.members_command_v48().is_some_and(|command| {
            matches!(
                command,
                crate::pr_metadata_actions::PrMetadataCommand::AddLabel
                    | crate::pr_metadata_actions::PrMetadataCommand::RemoveLabel
            )
        });
        for kind in self.terminal.categories() {
            let query=match (kind,base.is_some(),members_label) {
                (SyncBindingKind::ConversationComment,false,_)=>"SELECT mutation_ordinal FROM sync_mutation_fences WHERE host=?1 AND login=?2 AND namespace='collaboration' AND repository=?3 AND resource=?4 AND kind='conversation_comment'",
                (SyncBindingKind::PullRequestBody,false,_)=>"SELECT mutation_ordinal FROM sync_mutation_fences WHERE host=?1 AND login=?2 AND namespace='collaboration' AND repository=?3 AND resource=?4 AND kind='pull_request_body'",
                (SyncBindingKind::Review,true,false)|(SyncBindingKind::Review,false,true)=>"SELECT mutation_ordinal FROM sync_mutation_fences WHERE host=?1 AND login=?2 AND namespace='collaboration' AND repository=?3 AND resource=?4 AND kind='review'",
                (SyncBindingKind::InlineComment,true,false)|(SyncBindingKind::InlineComment,false,true)=>"SELECT mutation_ordinal FROM sync_mutation_fences WHERE host=?1 AND login=?2 AND namespace='collaboration' AND repository=?3 AND resource=?4 AND kind='inline_comment'",
                _=>bail!("Unknown closed Conversation/Base own-fence category"),
            };
            let ordinal: Option<u64> = cache
                .connection
                .query_row(
                    query,
                    params![
                        self.terminal.target().scope().host,
                        self.terminal.target().scope().login,
                        self.terminal.target().repository().to_ascii_lowercase(),
                        self.terminal.target().node_id()
                    ],
                    |r| r.get(0),
                )
                .optional()?;
            if ordinal != Some(self.terminal.reference().attempt_id as u64) {
                bail!("Exact original Conversation once-fence moved; no older receipt replay");
            }
        }
        Ok(())
    }
    /// Worker start only, BEFORE creating the new Core. This is a local
    /// invalidation witness, NOT fresh UID/repository/parent permission.
    pub(crate) fn validate_before_read(
        &self,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<()> {
        clock(cancel, deadline)?;
        self.terminal.validate_file_v19()?;
        let journal = crate::pr_write_journal::PrWriteJournal::open(self.terminal.database())?;
        self.terminal.validate_current_v19(&journal)?;
        let cache = Cache::open(self.terminal.database())?;
        match &self.family {
            FamilyV19::Catalogue(v) => cache.validate_catalogue_own_fence_v19(v)?,
            FamilyV19::Personal(v) => cache.validate_personal_conversation_fence_v19(v)?,
        }
        self.validate_once_v19(&cache)?;
        clock(cancel, deadline)
    }
    /// NEW genuine initial read's normal Personal reservation is the ONLY
    /// permitted local transition. Catalogue remains its exact inactive own
    /// counter until the post-UID atomic successor commit; no normal API is
    /// taught to accept an inactive original or a saved-ID surrogate.
    pub(crate) fn validate_during_read(
        &self,
        cache: &Cache,
        boundary: PrLocalSourceBoundaryV13<'_>,
    ) -> Result<()> {
        self.terminal.validate_file_v19()?;
        match (&self.family, boundary) {
            (FamilyV19::Catalogue(v), PrLocalSourceBoundaryV13::Catalogue(actual))
                if self
                    .source()
                    .opening()
                    .catalogue_source()
                    .is_some_and(|old| std::ptr::eq(old.as_ref(), actual)) =>
            {
                cache.validate_catalogue_own_fence_v19(v)?
            }
            (FamilyV19::Personal(v), PrLocalSourceBoundaryV13::PersonalReservation(actual)) => {
                cache.validate_personal_conversation_read_v19(v, actual)?
            }
            _ => bail!("Actual explicit Conversation Core/local-boundary family differs"),
        }
        self.validate_once_v19(cache)?;
        Ok(())
    }
    pub(super) fn catalogue_for_read_v19(
        &self,
        cache: &Cache,
        actual: &crate::scoped_pr_collaboration_source_v17::initial_v17::RefreshReadPreparationV19,
    ) -> Result<Option<PreparedCatalogueContinuationV19>> {
        match &self.family {
            FamilyV19::Catalogue(v) => Ok(Some(
                cache.prepare_catalogue_after_own_fence_v19(v, actual)?,
            )),
            FamilyV19::Personal(_) => Ok(None),
        }
    }
}
