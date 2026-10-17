//! W9 durable metadata selects READs only. No saved row issues a live Source,
//! Workflow/Commit, permission, retry, invocation or reconstructed own proof.
use super::*;
use crate::providers::gh::ScopedHistoricalWorkflowRemoteReadV9;
use std::sync::Arc;

pub(crate) struct ActionsHistoricalWorkflowRemoteKeyV9 {
    reference: ActionsPublicationReference,
    repository_id: u64,
    repository_node: String,
    workflow_id: u64,
    edit: ActionsOriginalWorkflowEditContextV9,
    receipt: Option<WorkflowRemoteReceiptV9>,
}
impl ActionsHistoricalWorkflowRemoteKeyV9 {
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
    pub(crate) fn edit(&self) -> &ActionsOriginalWorkflowEditContextV9 {
        &self.edit
    }
    pub(crate) fn receipt(&self) -> Option<&WorkflowRemoteReceiptV9> {
        self.receipt.as_ref()
    }
    pub(crate) fn retained_bytes(&self) -> usize {
        // All metadata is already length-preflighted by the SAME actual scoped
        // snapshot. This fixed additional envelope includes BOTH bounded clones
        // of the <2KiB context/receipt scalars; no private body is present.
        std::mem::size_of::<Self>() + 16 * 1024
    }
}
fn key(marker: &ActionsWriteMarker) -> Result<ActionsHistoricalWorkflowRemoteKeyV9> {
    marker.validate_metadata()?;
    let repository = marker
        .context
        .original_repository_v3
        .as_ref()
        .context("Actual original W9 repository metadata missing")?;
    let edit = marker
        .context
        .workflow_edit_v9
        .as_ref()
        .context("Not original W9 remote history")?;
    if !matches!(
        (marker.kind, edit.phase),
        (
            ActionsWriteKind::PublishWorkflowBranch,
            WorkflowRemotePhaseV9::Branch
        ) | (
            ActionsWriteKind::CreateWorkflowPr,
            WorkflowRemotePhaseV9::Pr
        )
    ) {
        bail!("Not the exact original W9 remote kind/phase");
    }
    Ok(ActionsHistoricalWorkflowRemoteKeyV9 {
        reference: marker.publication_reference(),
        repository_id: repository.repository_id,
        repository_node: repository.repository_node.clone(),
        workflow_id: marker
            .context
            .original_workflow_v6
            .as_ref()
            .context("Actual W9 workflow key missing")?
            .workflow_id,
        edit: edit.clone(),
        receipt: marker
            .receipt
            .as_ref()
            .and_then(|receipt| receipt.workflow_remote_v9.clone()),
    })
}
impl ActionsHistorySnapshotV5 {
    pub(crate) fn workflow_remote_key_v9(&self) -> Result<ActionsHistoricalWorkflowRemoteKeyV9> {
        key(self
            .selected
            .as_ref()
            .context("Choose an actual original W9 history row")?)
    }
}
pub(crate) struct ActionsWorkflowRemoteAckV9 {
    path: PathBuf,
    file: (u64, u64),
    marker: ActionsWriteMarker,
    generations: Vec<i64>,
    key: ActionsHistoricalWorkflowRemoteKeyV9,
    observed: ScopedHistoricalWorkflowRemoteReadV9,
    row: row_fence_v6::HistoryRowFenceV6,
    successor: Option<Arc<ActionsOwnedWorkflowRemoteHistoryV9>>,
}
impl ActionsWorkflowRemoteAckV9 {
    pub(crate) fn retained_bytes(&self) -> usize {
        // SAME 128KiB raw-marker preflight, existing decoder and raw fence.
        // Bound the marker/alias Vec nodes separately from its captured bytes.
        std::mem::size_of::<Self>()
            + self.path.capacity()
            + 512 * 1024
            + self.generations.capacity() * std::mem::size_of::<i64>()
            + self.key.retained_bytes()
            + self.observed.retained_bytes()
            + self.row.retained_bytes()
            + self
                .successor
                .as_ref()
                .map_or(0, |proof| proof.retained_bytes())
    }
}
pub(crate) fn workflow_remote_ack_available_v9(
    marker: &ActionsWriteMarker,
    observed: &ScopedHistoricalWorkflowRemoteReadV9,
) -> bool {
    matches!(
        marker.state,
        ActionsWriteState::Pending | ActionsWriteState::Uncertain | ActionsWriteState::Confirmed
    ) && matches!(
        marker.kind,
        ActionsWriteKind::PublishWorkflowBranch | ActionsWriteKind::CreateWorkflowPr
    ) && observed.available()
}
impl ActionsWriteJournal {
    /// Only a bounded discriminator for choosing the original read profile.
    /// It neither decodes a live target nor trusts a caller's arbitrary kind.
    pub(crate) fn workflow_remote_history_profile_v9(
        &self,
        scope: &Scope,
        reference: Option<&ActionsPublicationReference>,
    ) -> Result<bool> {
        let Some(reference) = reference else {
            // Worker-only allocation discriminator BEFORE Core. Bounded
            // scalar output, exact current scope/canonical kind bytes, no
            // context decoding or saved identity/source reconstruction. The
            // later coherent snapshot still validates EVERY selected marker.
            let found:i64=self.connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM actions_write_attempts WHERE host=?1 AND principal=?2 AND kind_json IN('\"publish_workflow_branch\"','\"create_workflow_pr\"') LIMIT 1)",
                params![scope.host,scope.login],|row|row.get(0))?;
            return match found {
                0 => Ok(false),
                1 => Ok(true),
                _ => bail!("Original W9 history allocation discriminator unavailable"),
            };
        };
        let raw: String = self.connection.query_row(
            "SELECT kind_json FROM actions_write_attempts WHERE attempt_id=?1 AND host=?2 AND principal=?3 AND fingerprint_sha256=?4 AND length(CAST(kind_json AS BLOB)) BETWEEN 1 AND 64",
            params![reference.attempt_id,scope.host,scope.login,reference.fingerprint_sha256],
            |row|row.get(0))?;
        let kind: ActionsWriteKind = serde_json::from_str(&raw)?;
        Ok(matches!(
            kind,
            ActionsWriteKind::PublishWorkflowBranch | ActionsWriteKind::CreateWorkflowPr
        ))
    }
    pub(crate) fn prepare_workflow_remote_ack_v9(
        &self,
        snapshot: &ActionsHistorySnapshotV5,
        observed: ScopedHistoricalWorkflowRemoteReadV9,
        provider: &GithubProvider,
        cancel: &CancellationToken,
        owned: Option<&Arc<ActionsOwnedWorkflowRemoteHistoryV9>>,
    ) -> Result<ActionsWorkflowRemoteAckV9> {
        self.validate_history_v5(snapshot)?;
        let marker = snapshot
            .selected
            .as_ref()
            .context("Actual W9 row required")?;
        let key = key(marker)?;
        if !observed.matches(provider, &key, cancel)
            || !workflow_remote_ack_available_v9(marker, &observed)
        {
            bail!("Exact W9 resource currently unavailable; protection retained, no causal claim");
        }
        let row = row_fence_v6::HistoryRowFenceV6::capture(&self.connection, marker)?;
        let successor = owned
            .map(|proof| {
                self.validate_owned_workflow_remote_history_v9(proof)?;
                if proof.reference() != marker.publication_reference() {
                    bail!("Own W9 READ proof belongs to a different original publication");
                }
                proof.next_for_ack(marker, ActionsWriteState::Acknowledged)
            })
            .transpose()?;
        Ok(ActionsWorkflowRemoteAckV9 {
            path: self.path.clone(),
            file: snapshot.file,
            marker: marker.clone(),
            generations: snapshot
                .generations
                .clone()
                .context("Actual W9 alias generations missing")?,
            key,
            observed,
            row,
            successor,
        })
    }
    pub(crate) fn validate_workflow_remote_ack_v9(
        &self,
        ack: &ActionsWorkflowRemoteAckV9,
    ) -> Result<()> {
        if !ack.observed.runner_barrier_current_v9()
            || self.path != ack.path
            || file(&self.path)? != ack.file
            || generations(&self.connection, &ack.marker.target)? != ack.generations
            || !ack.row.matches_state_v6(&self.connection, &ack.marker)?
        {
            bail!("Exact W9 ACK raw metadata/alias generations changed");
        }
        self.validate_scoped_marker_saved_v3(&ack.marker)
    }
    pub(crate) fn acknowledge_workflow_remote_v9(
        &mut self,
        provider: &GithubProvider,
        ack: ActionsWorkflowRemoteAckV9,
        identity: &HostIdentity,
        cancel: &CancellationToken,
        partial: &std::sync::atomic::AtomicBool,
    ) -> Result<Option<Arc<ActionsOwnedWorkflowRemoteHistoryV9>>> {
        if self.path != ack.path
            || file(&self.path)? != ack.file
            || Scope::github(&identity.host, identity.account_id)? != ack.marker.target.scope
            || !ack.observed.matches(provider, &ack.key, cancel)
        {
            bail!("Actual final W9 read/principal/local file changed");
        }
        self.validate_workflow_remote_ack_v9(&ack)?;
        if ack
            .observed
            .acknowledge_observed_pr_barrier_v9(provider, &ack.key, cancel)?
        {
            // O(1) status-only evidence. A later CAS/file/Clear refusal MUST
            // keep the raw SQL row protected and disclose partial local ACK.
            partial.store(true, std::sync::atomic::Ordering::Release);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if !ack.observed.matches(provider, &ack.key, cancel)
            || file(&self.path)? != ack.file
            || generations(&transaction, &ack.marker.target)? != ack.generations
        {
            bail!("W9 ACK expired or alias generation changed while waiting for lock");
        }
        ack.row
            .acknowledge_state_v8(&transaction, &ack.marker, ActionsWriteState::Acknowledged)?;
        advance(&transaction, &ack.marker.target)?;
        if file(&self.path)? != ack.file {
            bail!("Original W9 ACK file changed during local CAS; protection retained");
        }
        transaction.commit()?;
        partial.store(false, std::sync::atomic::Ordering::Release);
        // Raw row, next state and optional own successor were prepared BEFORE
        // final UID. No source recapture/JSON/clone follows the successful CAS.
        Ok(ack.successor)
    }
}
