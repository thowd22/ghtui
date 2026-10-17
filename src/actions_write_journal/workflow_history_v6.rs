//! Historical workflow READ/ACK only. No serialized record becomes a live
//! Workflow, Form approval, provider source or invocation permit.
use super::*;
use crate::providers::gh::ScopedHistoricalWorkflowReadV6;
pub(crate) struct ActionsHistoricalWorkflowKeyV6 {
    reference: ActionsPublicationReference,
    repository_id: u64,
    repository_node: String,
    workflow_id: u64,
    node: String,
    path: String,
}
impl ActionsHistoricalWorkflowKeyV6 {
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
}
fn key(marker: &ActionsWriteMarker) -> Result<ActionsHistoricalWorkflowKeyV6> {
    marker.validate_metadata()?;
    let repo = marker
        .context
        .original_repository_v3
        .as_ref()
        .context("Original workflow repository missing")?;
    let workflow = marker
        .context
        .original_workflow_v6
        .as_ref()
        .context("Not original workflow history")?;
    if !matches!(
        marker.kind,
        ActionsWriteKind::EnableWorkflow | ActionsWriteKind::DisableWorkflow
    ) {
        bail!("No workflow history adapter for this kind");
    }
    Ok(ActionsHistoricalWorkflowKeyV6 {
        reference: marker.publication_reference(),
        repository_id: repo.repository_id,
        repository_node: repo.repository_node.clone(),
        workflow_id: workflow.workflow_id,
        node: workflow.node.clone(),
        path: workflow.path.clone(),
    })
}
impl ActionsHistorySnapshotV5 {
    pub(crate) fn workflow_key_v6(&self) -> Result<ActionsHistoricalWorkflowKeyV6> {
        key(self
            .selected
            .as_ref()
            .context("Actual workflow history row required")?)
    }
}
pub(crate) struct ActionsWorkflowAckV6 {
    path: PathBuf,
    file: (u64, u64),
    marker: ActionsWriteMarker,
    generations: Vec<i64>,
    key: ActionsHistoricalWorkflowKeyV6,
    observed: ScopedHistoricalWorkflowReadV6,
    row: row_fence_v6::HistoryRowFenceV6,
    next: ActionsWriteState,
    owned_successor: Option<std::sync::Arc<ActionsOwnedWorkflowHistoryV6>>,
}
impl ActionsWorkflowAckV6 {
    pub(crate) fn retained_bytes(&self) -> usize {
        self.row.retained_bytes()
            + 512 * 1024
            + self
                .owned_successor
                .as_ref()
                .map_or(0, |proof| proof.retained_bytes())
    }
}
pub(crate) fn workflow_ack_available_v6(
    marker: &ActionsWriteMarker,
    observed: &ScopedHistoricalWorkflowReadV6,
) -> bool {
    observed.workflow().is_some_and(|workflow| {
        if marker.state == ActionsWriteState::AcceptedPending {
            marker.options.enabled.is_some_and(|enabled| {
                workflow.state
                    == if enabled {
                        "active"
                    } else {
                        "disabled_manually"
                    }
            })
        } else {
            matches!(
                marker.state,
                ActionsWriteState::Pending | ActionsWriteState::Uncertain
            )
        }
    })
}
impl ActionsWriteJournal {
    pub(crate) fn prepare_workflow_ack_v6(
        &self,
        snapshot: &ActionsHistorySnapshotV5,
        observed: ScopedHistoricalWorkflowReadV6,
        provider: &GithubProvider,
        cancel: &CancellationToken,
        owned: Option<&std::sync::Arc<ActionsOwnedWorkflowHistoryV6>>,
    ) -> Result<ActionsWorkflowAckV6> {
        self.validate_history_v5(snapshot)?;
        let marker = snapshot
            .selected
            .as_ref()
            .context("Actual workflow row required")?;
        let key = key(marker)?;
        if !observed.matches(provider, &key, cancel)
            || !workflow_ack_available_v6(marker, &observed)
        {
            bail!("Original workflow state unavailable or accepted transition still pending; no release");
        }
        let row = row_fence_v6::HistoryRowFenceV6::capture(&self.connection, marker)?;
        let next = if marker.state == ActionsWriteState::AcceptedPending {
            ActionsWriteState::TransitionObserved
        } else {
            ActionsWriteState::Acknowledged
        };
        let owned_successor = owned
            .map(|proof| {
                self.validate_owned_workflow_history_v6(proof)?;
                proof.next_for_ack(marker, next)
            })
            .transpose()?;
        Ok(ActionsWorkflowAckV6 {
            path: self.path.clone(),
            file: snapshot.file,
            marker: marker.clone(),
            generations: snapshot
                .generations
                .clone()
                .context("Original generations missing")?,
            key,
            observed,
            row,
            next,
            owned_successor,
        })
    }
    pub(crate) fn validate_workflow_ack_v6(&self, ack: &ActionsWorkflowAckV6) -> Result<()> {
        if self.path != ack.path
            || file(&self.path)? != ack.file
            || generations(&self.connection, &ack.marker.target)? != ack.generations
        {
            bail!("Original workflow ACK generation/file changed");
        }
        self.validate_scoped_marker_saved_v3(&ack.marker)
    }
    pub(crate) fn acknowledge_workflow_v6(
        &mut self,
        provider: &GithubProvider,
        ack: ActionsWorkflowAckV6,
        identity: &HostIdentity,
        cancel: &CancellationToken,
    ) -> Result<Option<std::sync::Arc<ActionsOwnedWorkflowHistoryV6>>> {
        if self.path != ack.path
            || file(&self.path)? != ack.file
            || Scope::github(&identity.host, identity.account_id)? != ack.marker.target.scope
            || !ack.observed.matches(provider, &ack.key, cancel)
        {
            bail!("Original final workflow scope/read changed");
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if !ack.observed.matches(provider, &ack.key, cancel)
            || generations(&transaction, &ack.marker.target)? != ack.generations
        {
            bail!("Original workflow ACK expired or generation changed waiting for lock");
        }
        ack.row
            .acknowledge_workflow_v6(&transaction, &ack.marker, ack.next)?;
        advance(&transaction, &ack.marker.target)?;
        transaction.commit()?;
        // Prepared BEFORE final UID; release only after the exact successful
        // Immediate raw-row/generation CAS. No readback/parse/clone follows it.
        Ok(ack.owned_successor)
    }
}
