//! Only the actual same native State publisher may issue this exact durable
//! terminal; history/ref/disposition JSON alone cannot construct it.
use super::*;
pub(crate) struct StateTerminalV68<'a> {
    pub(crate) source: &'a Arc<PrCollaborationSourceV17>,
    pub(crate) actual: &'a crate::providers::gh::OriginalPrStateV68,
    pub(crate) receipt: &'a crate::providers::gh::StateReceiptV68,
    pub(crate) permit: &'a PrWritePermit,
    pub(crate) local: &'a ResourceLocalGuard,
    pub(crate) generation: i64,
    pub(crate) effect: &'a PrConfirmedEffect,
    pub(crate) cancel: &'a CancellationToken,
}
impl PrWriteJournal {
    pub(crate) fn prepare_native_state_terminal_v68(
        &self,
        request: StateTerminalV68<'_>,
    ) -> Result<Arc<ConversationTerminalWitnessV19>> {
        let StateTerminalV68 {
            source,
            actual,
            receipt,
            permit,
            local,
            generation,
            effect,
            cancel,
        } = request;
        if permit.issuer != self.path
            || !permit.claimed.load(Ordering::Acquire)
            || source.opening().database() != self.path
            || !source
                .opening()
                .admits_state_v68(actual.observed().command())
            || receipt.reference() != &permit.marker().publication_reference()
            || !receipt.validates_refresh(actual, cancel)
            || receipt.effect() != *effect
            || !GithubProvider::same_thread_history_parent_v1(
                source.parent(),
                actual.observed().parent(),
            )
            || Scope::github(&actual.identity().host, actual.identity().account_id)?
                != *source.opening().scope()
        {
            bail!("Actual State receipt/source/claimed reference differs");
        }
        local
            .validate_owned_boundary_v17(
                &self.path,
                source.local_boundary(),
                permit.marker(),
                generation,
                PrWriteState::Confirmed,
                Some(effect),
            )
            .map_err(anyhow::Error::msg)?;
        let reference = permit.marker().publication_reference();
        let target = PrWriteTarget::new(actual.identity(), actual.observed().parent().summary())?;
        let marker = self
            .original_publication(&target, &reference)?
            .context("State durable terminal missing")?;
        if marker.state != PrWriteState::Confirmed || marker.confirmed.as_ref() != Some(effect) {
            bail!("State effect is not durably Confirmed");
        }
        let origin = self.state_origin_v68(&target, &marker)?;
        if !origin.seed.matches_actual_v68(actual) {
            bail!("State origin differs from this actual publisher");
        }
        let database = review_recovery_v11::DatabaseV11::capture(&self.path)?;
        let tx = self.connection.unchecked_transaction()?;
        let attempt=ExactRowV19::capture(&tx,"pr_write_attempts","attempt_id,host,principal,pr_node,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at",marker.attempt_id,8192)?;
        let origin=ExactRowV19::capture(&tx,"pr_write_state_origins_v68","attempt_id,repository_id,pr_id,issue_id,command,base_key_v71,action_tag_v71,parent_ack_attempt_v71,origin_json",marker.attempt_id,18*1024)?;
        let disposition = ExactRowV19::capture(
            &tx,
            "pr_write_state_dispositions_v68",
            "attempt_id,kind,receipt_json",
            marker.attempt_id,
            8192 + 64,
        )?;
        let witness = Arc::new(ConversationTerminalWitnessV19 {
            conversation_draft_resource_v88: None,
            purpose_v25: TerminalPurposeV25::StateV68(Arc::clone(receipt.transition())),
            publication_cutoff_v51: actual
                .publication_cutoff_v68()
                .min(receipt.publication_cutoff_v68()),
            original: Arc::clone(source),
            database,
            path: self.path.clone(),
            target,
            reference,
            generation,
            repository_id: receipt.parent().repository_id(),
            parent: Arc::clone(receipt.parent()),
            categories: vec![],
            attempt,
            origin,
            state_disposition_v68: Some(disposition),
        });
        witness.check_capacity_v51()?;
        witness.validate_transaction_v19(&tx)?;
        tx.commit()?;
        witness.validate_current_v19(self)?;
        Ok(witness)
    }
}
