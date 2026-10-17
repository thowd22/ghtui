//! SAME durable original-marker and acknowledgement machinery, bound to an
//! actual current opaque numeric run read. Saved metadata is never a source.
use super::*;
use crate::providers::gh::ScopedActionsRunTargetV1;
use std::sync::Arc;
pub(crate) struct ActionsScopedAckV3 {
    inspection: ActionsJournalInspection,
    next: ActionsWriteState,
}

impl ActionsWriteJournal {
    /// Recover a crash-before-draft-ACK marker from actual bounded SAME-account
    /// SQL. This returns descriptive reference only; actual numeric source/run
    /// is still mandatory, and incomplete/ambiguous metadata never means absent.
    pub(crate) fn scoped_reference_for_run_v3(
        &self,
        provider: &GithubProvider,
        identity: &HostIdentity,
        run: &Arc<ScopedActionsRunTargetV1>,
    ) -> Result<Option<ActionsPublicationReference>> {
        let scope = Scope::github(&identity.host, identity.account_id)?;
        let transaction = self.connection.unchecked_transaction()?;
        let (count,bytes):(i64,i64)=transaction.query_row(
            "SELECT COUNT(*),COALESCE(SUM(length(CAST(target_json AS BLOB))+length(CAST(resources_json AS BLOB))+length(CAST(kind_json AS BLOB))+length(CAST(context_json AS BLOB))+length(CAST(options_json AS BLOB))+length(CAST(fingerprint_sha256 AS BLOB))+COALESCE(length(CAST(receipt_json AS BLOB)),0)),0) FROM actions_write_attempts WHERE host=?1 AND principal=?2 AND state IN('pending','uncertain','accepted_pending')",
            params![scope.host,scope.login],|row|Ok((row.get(0)?,row.get(1)?)))?;
        if !(0..=256).contains(&count) || !(0..=8 * 1024 * 1024).contains(&bytes) {
            bail!("Original unresolved inventory incomplete/exceeds bounded metadata; not absence");
        }
        let ids = {
            let mut statement=transaction.prepare("SELECT attempt_id FROM actions_write_attempts WHERE host=?1 AND principal=?2 AND state IN('pending','uncertain','accepted_pending') ORDER BY attempt_id")?;
            let values = statement
                .query_map(params![scope.host, scope.login], |row| row.get::<_, i64>(0))?;
            values.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let mut selected = None;
        for id in ids {
            // Preflight each allocation using the existing bounded original read
            // on THIS transaction; no nested transaction or all-body collection.
            let size:i64=transaction.query_row("SELECT length(CAST(target_json AS BLOB))+length(CAST(resources_json AS BLOB))+length(CAST(kind_json AS BLOB))+length(CAST(context_json AS BLOB))+length(CAST(options_json AS BLOB))+length(CAST(fingerprint_sha256 AS BLOB))+COALESCE(length(CAST(receipt_json AS BLOB)),0) FROM actions_write_attempts WHERE attempt_id=?1 AND host=?2 AND principal=?3",params![id,scope.host,scope.login],|row|row.get(0))?;
            if !(1..=128 * 1024).contains(&size) {
                bail!("Original unresolved marker exceeds bounded metadata");
            }
            let marker = read_attempt(&transaction, &scope, id)?
                .context("Original unresolved marker changed")?;
            availability::validate_saved_marker(provider, &marker)?;
            if marker
                .context
                .original_repository_v3
                .as_ref()
                .is_some_and(|repo| repo.repository_id == run.repository_id())
                && marker.context.run_id == Some(run.run().id)
            {
                if selected.is_some() {
                    bail!("More than one protected original run attempt requires explicit selection; none adopted");
                }
                selected = Some(marker.publication_reference());
            }
        }
        transaction.commit()?;
        if let Some(reference) = &selected {
            self.scoped_marker_v3(provider, identity, reference, run)?;
        }
        Ok(selected)
    }
    pub(crate) fn validate_scoped_marker_saved_v3(
        &self,
        expected: &ActionsWriteMarker,
    ) -> Result<()> {
        let actual = bounded_original::read_checked_attempt(
            &self.connection,
            &expected.target.scope,
            expected.attempt_id,
        )?
        .context("Original marker unavailable")?;
        if actual.publication_reference() != expected.publication_reference()
            || actual.context != expected.context
            || actual.kind != expected.kind
            || actual.state != expected.state
            || actual.receipt != expected.receipt
            || !actual.target.same_resources(&expected.target)
        {
            bail!("Original protected scoped marker changed");
        }
        Ok(())
    }
    pub(crate) fn scoped_marker_v3(
        &self,
        provider: &GithubProvider,
        identity: &HostIdentity,
        reference: &ActionsPublicationReference,
        run: &Arc<ScopedActionsRunTargetV1>,
    ) -> Result<ActionsWriteMarker> {
        let scope = Scope::github(&identity.host, identity.account_id)?;
        let marker =
            bounded_original::read_checked_attempt(&self.connection, &scope, reference.attempt_id)?
                .context("Original protected Actions reference unavailable")?;
        availability::validate_saved_marker(provider, &marker)?;
        let repo = marker
            .context
            .original_repository_v3
            .as_ref()
            .context("Legacy reference is not a scoped source")?;
        let saved = marker
            .context
            .original_run_v3
            .as_ref()
            .context("Original run node unavailable")?;
        if marker.publication_reference() != *reference
            || !matches!(
                marker.kind,
                ActionsWriteKind::Cancel
                    | ActionsWriteKind::ForceCancel
                    | ActionsWriteKind::DeleteLogs
                    | ActionsWriteKind::RerunAll
                    | ActionsWriteKind::RerunFailed
                    | ActionsWriteKind::RerunJob
            )
            || !run.original_writer_v3(provider)
            || repo.repository_id != run.repository_id()
            || Some(repo.repository_node.as_str()) != run.repository_node()
            || Some(saved.run_node.as_str()) != run.node_v2()
            || saved.workflow_id != run.run().workflow_id
            || saved.check_suite_id != run.suite_v3()
            || marker.context.run_id != Some(run.run().id)
            || marker
                .context
                .attempt
                .is_none_or(|original| original > run.run().attempt)
            || marker.context.head_sha.as_deref() != Some(run.run().head_sha.as_str())
        {
            bail!("Original protected marker does not match actual numeric repository/run/node/attempt/head");
        }
        Ok(marker)
    }
    pub(crate) fn prepare_scoped_inspection_v3(
        &self,
        provider: &GithubProvider,
        identity: &HostIdentity,
        reference: &ActionsPublicationReference,
        run: &Arc<ScopedActionsRunTargetV1>,
        cancel: &CancellationToken,
    ) -> Result<ActionsJournalInspection> {
        let marker = self.scoped_marker_v3(provider, identity, reference, run)?;
        if !matches!(
            marker.state,
            ActionsWriteState::Pending
                | ActionsWriteState::Uncertain
                | ActionsWriteState::AcceptedPending
        ) {
            bail!("Original result no longer needs acknowledgement");
        }
        let before = generations(&self.connection, &marker.target)?;
        let observation = provider
            .scoped_run_observation_v3(identity, run, cancel)
            .map_err(anyhow::Error::msg)?;
        let value = ActionsJournalInspection {
            issuer: self.path.clone(),
            marker,
            generations: before,
            observation,
        };
        self.validate_scoped_inspection_v3(&value)?;
        Ok(value)
    }
    pub(crate) fn validate_scoped_inspection_v3(
        &self,
        inspection: &ActionsJournalInspection,
    ) -> Result<()> {
        if inspection.issuer != self.path
            || inspection.marker.context.original_repository_v3.is_none()
            || generations(&self.connection, &inspection.marker.target)? != inspection.generations
        {
            bail!("Original scoped inspection generation changed; protection remains");
        }
        let actual = bounded_original::read_checked_attempt(
            &self.connection,
            &inspection.marker.target.scope,
            inspection.marker.attempt_id,
        )?
        .context("Original marker unavailable")?;
        if serde_json::to_vec(&actual.publication_reference())?
            != serde_json::to_vec(&inspection.marker.publication_reference())?
            || actual.context != inspection.marker.context
            || actual.kind != inspection.marker.kind
            || actual.state != inspection.marker.state
            || actual.receipt != inspection.marker.receipt
            || !actual.target.same_resources(&inspection.marker.target)
        {
            bail!("Original scoped marker changed; no acknowledgement adopted");
        }
        // Reuse SAME typed original transition/source-generation check.
        inspection
            .observation
            .run_transition(
                actual.context.run_id.context("Original run missing")?,
                actual.context.attempt.context("Original attempt missing")?,
                actual
                    .context
                    .head_sha
                    .as_deref()
                    .context("Original head missing")?,
                match actual.kind {
                    ActionsWriteKind::Cancel => RunMutation::Cancel,
                    ActionsWriteKind::ForceCancel => RunMutation::ForceCancel,
                    ActionsWriteKind::DeleteLogs => RunMutation::DeleteLogs,
                    ActionsWriteKind::RerunAll => RunMutation::RerunAll {
                        debug: actual.options.debug_logging,
                    },
                    ActionsWriteKind::RerunFailed => RunMutation::RerunFailed {
                        debug: actual.options.debug_logging,
                    },
                    ActionsWriteKind::RerunJob => RunMutation::RerunJob {
                        job_id: actual
                            .context
                            .job_id
                            .context("Original selected job missing")?,
                        debug: actual.options.debug_logging,
                    },
                    _ => bail!("No reviewed original run transition for this protected action"),
                },
            )
            .map_err(anyhow::Error::msg)?;
        Ok(())
    }
    pub(crate) fn same_scoped_inspection_v3(
        &self,
        old: &ActionsJournalInspection,
        fresh: &ActionsJournalInspection,
    ) -> Result<()> {
        self.validate_scoped_inspection_v3(old)?;
        self.validate_scoped_inspection_v3(fresh)?;
        if old.issuer != fresh.issuer
            || old.generations != fresh.generations
            || old.marker.publication_reference() != fresh.marker.publication_reference()
            || old.marker.state != fresh.marker.state
            || old.marker.receipt != fresh.marker.receipt
            || old.marker.context != fresh.marker.context
            || old.observation.state() != fresh.observation.state()
            || !old.marker.target.same_resources(&fresh.marker.target)
        {
            bail!("Displayed original inspection changed; explicitly inspect again");
        }
        Ok(())
    }
    /// Full marker/observation work BEFORE final original source/SQL/UID close.
    pub(crate) fn prepare_scoped_ack_v3(
        &self,
        inspection: ActionsJournalInspection,
    ) -> Result<ActionsScopedAckV3> {
        self.validate_scoped_inspection_v3(&inspection)?;
        let next = if inspection.marker.state == ActionsWriteState::AcceptedPending {
            require_accepted_transition(&inspection.marker, &inspection.observation)?;
            ActionsWriteState::TransitionObserved
        } else {
            ActionsWriteState::Acknowledged
        };
        Ok(ActionsScopedAckV3 { inspection, next })
    }
    pub(crate) fn prepare_scoped_acceptance_ack_v5(
        &self,
        inspection: ActionsJournalInspection,
    ) -> Result<ActionsScopedAckV3> {
        self.validate_scoped_inspection_v3(&inspection)?;
        require_unverified_acceptance(&inspection.marker, &inspection.observation)?;
        Ok(ActionsScopedAckV3 {
            inspection,
            next: ActionsWriteState::Acknowledged,
        })
    }
    pub(crate) fn prepare_scoped_ack_mode_v5(
        &self,
        inspection: ActionsJournalInspection,
        acceptance_only: bool,
    ) -> Result<ActionsScopedAckV3> {
        if acceptance_only {
            self.prepare_scoped_acceptance_ack_v5(inspection)
        } else {
            self.prepare_scoped_ack_v3(inspection)
        }
    }
    pub(crate) fn validate_scoped_ack_v3(&self, pending: &ActionsScopedAckV3) -> Result<()> {
        self.validate_scoped_inspection_v3(&pending.inspection)
    }
    /// Called ONLY after SAME original source/SQL/final UID close. No full body
    /// parse/API/new observation follows; SAME commit rechecks exact generations
    /// and state in Immediate transaction before this local bookkeeping write.
    pub(crate) fn acknowledge_scoped_inspection_v3(
        &mut self,
        provider: &GithubProvider,
        pending: ActionsScopedAckV3,
        current: &HostIdentity,
    ) -> Result<()> {
        let ActionsScopedAckV3 { inspection, next } = pending;
        if inspection.issuer != self.path {
            bail!("Original ACK belongs to another journal");
        }
        if !current.same_account(inspection.observation.identity())
            || Scope::github(&current.host, current.account_id)? != inspection.marker.target.scope
        {
            bail!("Original final numeric principal changed; protection remains");
        }
        provider
            .acknowledge_actions_observation(inspection.observation)
            .map_err(anyhow::Error::msg)?;
        self.acknowledge_closed_v3(inspection.marker, inspection.generations, next)
    }
}
