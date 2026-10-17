//! Distinct Dispatch historical metadata/acceptance ACK, NEVER a live target.
use super::*;
use crate::providers::gh::ScopedHistoricalDispatchReadV7;
pub(crate) struct ActionsHistoricalDispatchKeyV7 {
    reference: ActionsPublicationReference,
    repository_id: u64,
    repository_node: String,
    workflow_id: u64,
    node: String,
    path: String,
    context: ActionsOriginalDispatchContextV7,
    run_id: Option<u64>,
    attempt: Option<u64>,
    accepted: bool,
    run: Option<ActionsDispatchRunReceiptV7>,
}
impl ActionsHistoricalDispatchKeyV7 {
    pub(crate) fn reference(&self) -> &ActionsPublicationReference {
        &self.reference
    }
    pub(crate) fn repository_id(&self) -> u64 {
        self.repository_id
    }
    pub(crate) fn repository_node(&self) -> &str {
        &self.repository_node
    }
    pub(crate) fn workflow_id(&self) -> u64 {
        self.workflow_id
    }
    pub(crate) fn node(&self) -> &str {
        &self.node
    }
    pub(crate) fn path(&self) -> &str {
        &self.path
    }
    pub(crate) fn context(&self) -> &ActionsOriginalDispatchContextV7 {
        &self.context
    }
    pub(crate) fn run_id(&self) -> Option<u64> {
        self.run_id
    }
    pub(crate) fn attempt(&self) -> Option<u64> {
        self.attempt
    }
    pub(crate) fn accepted(&self) -> bool {
        self.accepted
    }
    pub(crate) fn run_receipt(&self) -> Option<&ActionsDispatchRunReceiptV7> {
        self.run.as_ref()
    }
}
fn key(marker: &ActionsWriteMarker) -> Result<ActionsHistoricalDispatchKeyV7> {
    marker.validate_metadata()?;
    if marker.kind != ActionsWriteKind::DispatchWorkflow {
        return Err(anyhow::anyhow!("Not exact original Dispatch history"));
    }
    let repo = marker
        .context
        .original_repository_v3
        .as_ref()
        .context("Original Dispatch repository missing")?;
    let workflow = marker
        .context
        .original_workflow_v6
        .as_ref()
        .context("Original Dispatch workflow missing")?;
    let context = marker
        .context
        .original_dispatch_v7
        .as_ref()
        .context("Original Dispatch protocol/ref metadata missing")?;
    let receipt = marker
        .receipt
        .as_ref()
        .filter(|receipt| receipt.kind == ActionsReceiptKind::Dispatch);
    Ok(ActionsHistoricalDispatchKeyV7 {
        reference: marker.publication_reference(),
        repository_id: repo.repository_id,
        repository_node: repo.repository_node.clone(),
        workflow_id: workflow.workflow_id,
        node: workflow.node.clone(),
        path: workflow.path.clone(),
        context: context.clone(),
        run_id: receipt.and_then(|receipt| receipt.observed_run_id),
        attempt: receipt.and_then(|receipt| receipt.observed_attempt),
        accepted: receipt.is_some(),
        run: receipt.and_then(|receipt| receipt.original_dispatch_run_v7.clone()),
    })
}
impl ActionsHistorySnapshotV5 {
    pub(crate) fn dispatch_key_v7(&self) -> Result<ActionsHistoricalDispatchKeyV7> {
        key(self
            .selected
            .as_ref()
            .context("Actual Dispatch history row missing")?)
    }
}
pub(crate) fn dispatch_acceptance_available_v7(marker: &ActionsWriteMarker) -> bool {
    marker.kind == ActionsWriteKind::DispatchWorkflow
        && marker.state == ActionsWriteState::AcceptedPending
        && marker.context.original_dispatch_v7.is_some()
        && marker.receipt.as_ref().is_some_and(|receipt| {
            receipt.kind == ActionsReceiptKind::Dispatch
                && receipt.resource_id
                    == marker
                        .context
                        .original_workflow_v6
                        .as_ref()
                        .map(|workflow| workflow.workflow_id)
        })
}
pub(crate) struct ActionsDispatchAckV7 {
    path: PathBuf,
    file: (u64, u64),
    marker: ActionsWriteMarker,
    generations: Vec<i64>,
    key: ActionsHistoricalDispatchKeyV7,
    observed: ScopedHistoricalDispatchReadV7,
    row: row_fence_v6::HistoryRowFenceV6,
    owned_successor: Option<std::sync::Arc<ActionsOwnedDispatchHistoryV7>>,
}
impl ActionsDispatchAckV7 {
    pub(crate) fn retained_bytes(&self) -> usize {
        512 * 1024
            + self.row.retained_bytes()
            + self
                .owned_successor
                .as_ref()
                .map_or(0, |proof| proof.retained_bytes())
    }
}
impl ActionsWriteJournal {
    pub(crate) fn prepare_dispatch_ack_v7(
        &self,
        snapshot: &ActionsHistorySnapshotV5,
        observed: ScopedHistoricalDispatchReadV7,
        provider: &GithubProvider,
        cancel: &CancellationToken,
        owned: Option<&std::sync::Arc<ActionsOwnedDispatchHistoryV7>>,
    ) -> Result<ActionsDispatchAckV7> {
        self.validate_history_v5(snapshot)?;
        let marker = snapshot
            .selected
            .as_ref()
            .context("Original Dispatch row missing")?;
        let key = key(marker)?;
        if !dispatch_acceptance_available_v7(marker) || !observed.matches(provider, &key, cancel) {
            bail!("Exact accepted Dispatch inspection required; unknown is not acceptance");
        }
        let row = row_fence_v6::HistoryRowFenceV6::capture(&self.connection, marker)?;
        let owned_successor = owned
            .map(|proof| {
                self.validate_owned_dispatch_history_v7(proof)?;
                proof.next_for_ack(marker, ActionsWriteState::Acknowledged)
            })
            .transpose()?;
        Ok(ActionsDispatchAckV7 {
            path: self.path.clone(),
            file: snapshot.file,
            marker: marker.clone(),
            generations: snapshot
                .generations
                .clone()
                .context("Original Dispatch generations missing")?,
            key,
            observed,
            row,
            owned_successor,
        })
    }
    pub(crate) fn validate_dispatch_ack_v7(&self, ack: &ActionsDispatchAckV7) -> Result<()> {
        if self.path != ack.path
            || file(&self.path)? != ack.file
            || generations(&self.connection, &ack.marker.target)? != ack.generations
            || !ack.row.matches_state_v6(&self.connection, &ack.marker)?
        {
            bail!("Original Dispatch ACK raw row/file/generation changed");
        }
        self.validate_scoped_marker_saved_v3(&ack.marker)
    }
    pub(crate) fn acknowledge_dispatch_v7(
        &mut self,
        provider: &GithubProvider,
        ack: ActionsDispatchAckV7,
        identity: &HostIdentity,
        cancel: &CancellationToken,
    ) -> Result<Option<std::sync::Arc<ActionsOwnedDispatchHistoryV7>>> {
        if self.path != ack.path
            || file(&self.path)? != ack.file
            || Scope::github(&identity.host, identity.account_id)? != ack.marker.target.scope
            || !ack.observed.matches(provider, &ack.key, cancel)
        {
            bail!("Final original Dispatch principal/read changed");
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if !ack.observed.matches(provider, &ack.key, cancel)
            || generations(&transaction, &ack.marker.target)? != ack.generations
        {
            bail!("Original Dispatch expired or generation changed waiting for ACK lock");
        }
        ack.row
            .acknowledge_state_v8(&transaction, &ack.marker, ActionsWriteState::Acknowledged)?;
        advance(&transaction, &ack.marker.target)?;
        if !ack.observed.matches(provider, &ack.key, cancel) {
            bail!("Original Dispatch provider generation changed; SQL protection retained");
        }
        transaction.commit()?;
        Ok(ack.owned_successor)
    }
}
