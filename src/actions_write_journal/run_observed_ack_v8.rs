//! SAME actual historical read and durable Run ACK. No source/permission
//! recovery and no generic legacy ACK changes.
use super::*;
use std::sync::Arc;
pub(crate) enum ActionsRunHistoryAckV8 {
    Observed(Box<ActionsRunObservedAckV8>),
    Acceptance(
        Box<ActionsHistoryAckV5>,
        Option<Arc<ActionsOwnedRunHistoryV8>>,
    ),
}
impl ActionsRunHistoryAckV8 {
    pub(crate) fn retained_bytes(&self) -> usize {
        match self {
            Self::Observed(a) => a.retained_bytes(),
            Self::Acceptance(a, p) => {
                512 * 1024
                    + a.retained_row_fence_bytes_v6()
                    + p.as_ref().map_or(0, |p| p.retained_bytes())
            }
        }
    }
}
pub(crate) struct ActionsRunObservedAckV8 {
    path: PathBuf,
    file: (u64, u64),
    inspection: ActionsJournalInspection,
    read: ScopedHistoricalRunReadV5,
    key: ActionsHistoricalRunKeyV5,
    row: row_fence_v6::HistoryRowFenceV6,
    next: ActionsWriteState,
    owned_successor: Option<Arc<ActionsOwnedRunHistoryV8>>,
}
impl ActionsRunObservedAckV8 {
    pub(crate) fn retained_bytes(&self) -> usize {
        512 * 1024
            + self.row.retained_bytes()
            + self
                .owned_successor
                .as_ref()
                .map_or(0, |p| p.retained_bytes())
    }
}
impl ActionsWriteJournal {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare_run_history_ack_v8(
        &self,
        snapshot: &ActionsHistorySnapshotV5,
        inspection: Option<ActionsJournalInspection>,
        read: ScopedHistoricalRunReadV5,
        provider: &GithubProvider,
        cancel: &CancellationToken,
        acceptance_only: bool,
        owned: Option<&Arc<ActionsOwnedRunHistoryV8>>,
    ) -> Result<ActionsRunHistoryAckV8> {
        if acceptance_only
            && snapshot.selected().is_some_and(|m| {
                matches!(
                    m.kind,
                    ActionsWriteKind::DeleteRun | ActionsWriteKind::DeleteLogs
                )
            })
        {
            let ack = self.prepare_history_acceptance_v5(snapshot, read, provider, cancel)?;
            let successor = owned
                .map(|p| {
                    self.validate_owned_run_history_v8(p)?;
                    p.next_for_ack(&ack.marker, ActionsWriteState::Acknowledged)
                })
                .transpose()?;
            Ok(ActionsRunHistoryAckV8::Acceptance(Box::new(ack), successor))
        } else {
            Ok(ActionsRunHistoryAckV8::Observed(Box::new(
                self.prepare_run_observed_ack_v8(
                    snapshot,
                    inspection.context("Original observed Run ACK missing")?,
                    read,
                    provider,
                    cancel,
                    acceptance_only,
                    owned,
                )?,
            )))
        }
    }
    pub(crate) fn validate_run_history_ack_v8(&self, ack: &ActionsRunHistoryAckV8) -> Result<()> {
        match ack {
            ActionsRunHistoryAckV8::Observed(a) => self.validate_run_observed_ack_v8(a),
            ActionsRunHistoryAckV8::Acceptance(a, _) => self.validate_history_ack_v5(a),
        }
    }
    pub(crate) fn acknowledge_run_history_v8(
        &mut self,
        provider: &GithubProvider,
        ack: ActionsRunHistoryAckV8,
        identity: &HostIdentity,
        cancel: &CancellationToken,
    ) -> Result<Option<Arc<ActionsOwnedRunHistoryV8>>> {
        match ack {
            ActionsRunHistoryAckV8::Observed(a) => {
                self.acknowledge_run_observed_v8(provider, *a, identity, cancel)
            }
            ActionsRunHistoryAckV8::Acceptance(a, p) => {
                self.acknowledge_history_acceptance_v5(provider, *a, identity, cancel)?;
                Ok(p)
            }
        }
    }
    pub(crate) fn prepare_run_history_inspection_v8(
        &self,
        snapshot: &ActionsHistorySnapshotV5,
        read: &ScopedHistoricalRunReadV5,
        provider: &GithubProvider,
        identity: &HostIdentity,
        current: &CurrentCatalogueRepository,
        cancel: &CancellationToken,
    ) -> Result<ActionsJournalInspection> {
        self.validate_history_v5(snapshot)?;
        let marker = snapshot
            .selected()
            .context("Actual original Run history required")?;
        if marker.context.original_run_v3.is_none()
            || marker.context.original_workflow_v6.is_some()
            || !matches!(
                marker.kind,
                ActionsWriteKind::Cancel
                    | ActionsWriteKind::ForceCancel
                    | ActionsWriteKind::RerunAll
                    | ActionsWriteKind::RerunFailed
                    | ActionsWriteKind::RerunJob
                    | ActionsWriteKind::DeleteLogs
            )
            || !matches!(
                marker.state,
                ActionsWriteState::Pending
                    | ActionsWriteState::Uncertain
                    | ActionsWriteState::AcceptedPending
            )
        {
            bail!("No observed-state ACK policy for this exact Run result");
        }
        let key = snapshot.key()?;
        let observation = provider
            .scoped_historical_run_observation_v8(identity, read, &key, current, cancel)
            .map_err(anyhow::Error::msg)?;
        let inspection = ActionsJournalInspection {
            issuer: self.path.clone(),
            marker: marker.clone(),
            generations: generations(&self.connection, &marker.target)?,
            observation,
        };
        self.validate_scoped_inspection_v3(&inspection)?;
        Ok(inspection)
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare_run_observed_ack_v8(
        &self,
        snapshot: &ActionsHistorySnapshotV5,
        inspection: ActionsJournalInspection,
        read: ScopedHistoricalRunReadV5,
        provider: &GithubProvider,
        cancel: &CancellationToken,
        acceptance_only: bool,
        owned: Option<&Arc<ActionsOwnedRunHistoryV8>>,
    ) -> Result<ActionsRunObservedAckV8> {
        self.validate_history_v5(snapshot)?;
        self.validate_scoped_inspection_v3(&inspection)?;
        let marker = snapshot.selected().context("Original Run row missing")?;
        if marker.publication_reference() != inspection.marker.publication_reference()
            || marker.state != inspection.marker.state
            || marker.receipt != inspection.marker.receipt
            || marker.context != inspection.marker.context
            || marker.options != inspection.marker.options
        {
            bail!("Actual displayed Run inspection changed");
        }
        let key = snapshot.key()?;
        if !read.matches(provider, &key, cancel) {
            bail!("Original historical read expired/changed");
        }
        let next = if acceptance_only {
            require_unverified_acceptance(marker, &inspection.observation)?;
            ActionsWriteState::Acknowledged
        } else if marker.state == ActionsWriteState::AcceptedPending {
            require_accepted_transition(marker, &inspection.observation)?;
            ActionsWriteState::TransitionObserved
        } else {
            ActionsWriteState::Acknowledged
        };
        let row = row_fence_v6::HistoryRowFenceV6::capture(&self.connection, marker)?;
        let owned_successor = owned
            .map(|proof| {
                self.validate_owned_run_history_v8(proof)?;
                proof.next_for_ack(marker, next)
            })
            .transpose()?;
        Ok(ActionsRunObservedAckV8 {
            path: self.path.clone(),
            file: snapshot.file,
            inspection,
            read,
            key,
            row,
            next,
            owned_successor,
        })
    }
    pub(crate) fn validate_run_observed_ack_v8(
        &self,
        pending: &ActionsRunObservedAckV8,
    ) -> Result<()> {
        if self.path != pending.path
            || file(&self.path)? != pending.file
            || generations(&self.connection, &pending.inspection.marker.target)?
                != pending.inspection.generations
            || !pending
                .row
                .matches_state_v6(&self.connection, &pending.inspection.marker)?
        {
            bail!("Original Run ACK row/file/generation changed");
        }
        self.validate_scoped_inspection_v3(&pending.inspection)
    }
    pub(crate) fn acknowledge_run_observed_v8(
        &mut self,
        provider: &GithubProvider,
        pending: ActionsRunObservedAckV8,
        current: &HostIdentity,
        cancel: &CancellationToken,
    ) -> Result<Option<Arc<ActionsOwnedRunHistoryV8>>> {
        if self.path != pending.path
            || file(&self.path)? != pending.file
            || Scope::github(&current.host, current.account_id)?
                != pending.inspection.marker.target.scope
            // Original scoped receipts bind positive numeric UID, not a
            // mutable display login. Both observations must close that SAME
            // host/UID; this does not change the legacy None ACK policy.
            || Scope::github(
                &pending.inspection.observation.identity().host,
                pending.inspection.observation.identity().account_id,
            )? != pending.inspection.marker.target.scope
            || !pending.read.matches(provider, &pending.key, cancel)
        {
            bail!("Original final Run ACK scope/read changed");
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if !pending.read.matches(provider, &pending.key, cancel)
            || generations(&transaction, &pending.inspection.marker.target)?
                != pending.inspection.generations
        {
            bail!("Original Run ACK expired or generation changed waiting for lock");
        }
        // SAME runner-generation acknowledgement, not publication. It can advance
        // the in-memory invalidation fence before a later SQLite failure; the exact
        // durable row remains protected and still blocks every fresh publication.
        // No rollback/retry/lease drop or successful UI witness is inferred.
        provider
            .acknowledge_actions_observation(pending.inspection.observation)
            .map_err(anyhow::Error::msg)?;
        pending
            .row
            .acknowledge_state_v8(&transaction, &pending.inspection.marker, pending.next)?;
        advance(&transaction, &pending.inspection.marker.target)?;
        if !pending.read.matches(provider, &pending.key, cancel) {
            bail!("Original Run provider generation changed before ACK commit; SQL protection retained");
        }
        transaction.commit()?;
        Ok(pending.owned_successor)
    }
}
