//! Constructor is called only by the SAME native member publisher, after its
//! typed six-child receipt and durable Confirmed row. No history constructor.
use super::*;
pub(crate) struct MetadataMembersTerminalV48<'a> {
    pub(crate) source: &'a Arc<PrCollaborationSourceV17>,
    pub(crate) actual: &'a Arc<crate::providers::gh::OriginalMetadataMembersV48>,
    pub(crate) plan: &'a crate::pr_metadata_actions::PrMetadataPlan,
    pub(crate) receipt: &'a crate::providers::gh::PrMetadataMembersReceiptV48,
    pub(crate) permit: &'a PrWritePermit,
    pub(crate) local: &'a ResourceLocalGuard,
    pub(crate) confirmed_generation: i64,
    pub(crate) effect: &'a PrConfirmedEffect,
    pub(crate) cancel: &'a CancellationToken,
}
impl PrWriteJournal {
    pub(crate) fn prepare_native_metadata_members_terminal_v48(
        &self,
        request: MetadataMembersTerminalV48<'_>,
    ) -> Result<Arc<ConversationTerminalWitnessV19>> {
        let MetadataMembersTerminalV48 {
            source,
            actual,
            plan,
            receipt,
            permit,
            local,
            confirmed_generation,
            effect,
            cancel,
        } = request;
        if permit.issuer != self.path
            || !permit.claimed.load(Ordering::Acquire)
            || source.opening().database() != self.path
            || !receipt.validates_refresh(actual, plan, cancel)
            || receipt.effect() != effect
            || !GithubProvider::same_thread_history_parent_v1(source.parent(), actual.parent())
            || Scope::github(&actual.identity().host, actual.identity().account_id)?
                != *source.opening().scope()
        {
            bail!("Actual member receipt/source/selected plan/permit differs");
        }
        local
            .validate_owned_boundary_v17(
                &self.path,
                source.local_boundary(),
                permit.marker(),
                confirmed_generation,
                PrWriteState::Confirmed,
                Some(effect),
            )
            .map_err(anyhow::Error::msg)?;
        let reference = permit.marker.publication_reference();
        let target = PrWriteTarget::new(actual.identity(), actual.parent().summary())?;
        let marker = self
            .original_publication(&target, &reference)?
            .context("Actual confirmed member original absent")?;
        if marker.state != PrWriteState::Confirmed || marker.confirmed.as_ref() != Some(effect) {
            bail!("Actual member receipt is not durably Confirmed");
        }
        let tx = self.connection.unchecked_transaction()?;
        let record = self.metadata_members_origin_v48(&target, &marker)?;
        if !record.seed.matches_parent(actual.parent())
            || record.seed.command()? != plan.command()
            || !record.seed.matches_effect_v48(effect)?
        {
            bail!("Actual member original differs from this typed publisher/receipt");
        }
        let database = review_recovery_v11::DatabaseV11::capture(&self.path)?;
        let attempt = ExactRowV19::capture(&tx, "pr_write_attempts", "attempt_id,host,principal,pr_node,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at", marker.attempt_id, 8192)?;
        let origin = ExactRowV19::capture(&tx, "pr_write_metadata_origins_v25",
            "attempt_id,repository_id,pr_id,origin_json,base_key_v48,action_tag_v48,parent_ack_attempt_v48", marker.attempt_id, 17 * 1024)?;
        let categories = if matches!(
            plan.command(),
            crate::pr_metadata_actions::PrMetadataCommand::AddLabel
                | crate::pr_metadata_actions::PrMetadataCommand::RemoveLabel
        ) {
            vec![
                crate::storage::SyncBindingKind::PullRequestBody,
                crate::storage::SyncBindingKind::ConversationComment,
                crate::storage::SyncBindingKind::Review,
                crate::storage::SyncBindingKind::InlineComment,
            ]
        } else {
            vec![]
        };
        let witness = Arc::new(ConversationTerminalWitnessV19 {
            purpose_v25: TerminalPurposeV25::MetadataMembersV48(plan.command()),
            conversation_draft_resource_v88: None,
            publication_cutoff_v51: actual.deadline().min(receipt.deadline()),
            original: Arc::clone(source),
            database,
            path: self.path.clone(),
            target,
            reference,
            generation: confirmed_generation,
            repository_id: actual.parent().repository_id(),
            parent: Arc::clone(receipt.parent()),
            categories,
            attempt,
            origin,
            state_disposition_v68: None,
        });
        witness.check_capacity_v51()?;
        witness.validate_transaction_v19(&tx)?;
        tx.commit()?;
        witness.validate_current_v19(self)?;
        Ok(witness)
    }
}
