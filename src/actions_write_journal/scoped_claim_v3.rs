//! Bounded SAME-journal claimed-state/generation fence, not a provider permit.
//! The explicit Confirm's original source/ACK/principal checks are separate.
use super::*;
use std::{os::unix::fs::MetadataExt, sync::Arc};
#[path = "owned_history_fence_v8.rs"]
mod owned_history_fence_v8;
#[path = "owned_run_history_v8.rs"]
mod owned_run_history_v8;
pub(crate) use owned_run_history_v8::ActionsOwnedRunHistoryV8;
#[path = "owned_workflow_history_v6.rs"]
mod workflow_history_v6;
pub(crate) use workflow_history_v6::ActionsOwnedWorkflowHistoryV6;
#[path = "owned_dispatch_history_v7.rs"]
mod dispatch_history_v7;
pub(crate) use dispatch_history_v7::ActionsOwnedDispatchHistoryV7;
#[path = "owned_workflow_remote_history_v9.rs"]
mod workflow_remote_history_v9;
pub(crate) use workflow_remote_history_v9::ActionsOwnedWorkflowRemoteHistoryV9;
pub(super) use workflow_remote_history_v9::WorkflowPrPriorBranchFenceV9;

pub(crate) struct ActionsClaimStampV3 {
    path: PathBuf,
    file: (u64, u64),
    marker: ActionsWriteMarker,
    generations: Vec<i64>,
    force_escalation_v5: Option<std::sync::Arc<ScopedForceEscalationV5>>,
    workflow_pr_prior_branch_v9: Option<Arc<WorkflowPrPriorBranchFenceV9>>,
}
impl ActionsClaimStampV3 {
    pub(crate) fn matches_pr_origin_v4(
        &self,
        target: &crate::pr_write_journal::PrWriteTarget,
        database: &Path,
        file: (u64, u64),
    ) -> bool {
        self.path == database
            && self.file == file
            && self.marker.target.scope() == target.scope()
            && self
                .marker
                .context
                .original_pr_v3
                .as_ref()
                .is_some_and(|pr| pr.node == target.node_id() && pr.number == target.number())
    }
}

#[cfg(test)]
#[path = "scoped_claim_v3_tests.rs"]
mod tests;
fn file(path: &Path) -> Result<(u64, u64)> {
    let metadata = std::fs::symlink_metadata(path)?;
    // SAFETY: supported Linux/macOS effective UID query has no side effects.
    let uid = unsafe { libc::geteuid() };
    if !metadata.is_file()
        || metadata.uid() != uid
        || metadata.nlink() != 1
        || metadata.mode() & 0o077 != 0
    {
        bail!("Original Actions journal file is no longer its private original inode");
    }
    Ok((metadata.dev(), metadata.ino()))
}
fn same_metadata(actual: &ActionsWriteMarker, expected: &ActionsWriteMarker) -> bool {
    actual.attempt_id == expected.attempt_id
        && actual.target.same_resources(&expected.target)
        && matches!((&actual.target.target, &expected.target.target),
            (ActionsTarget::Run { repository, id }, ActionsTarget::Run { repository: old, id: old_id })
                |(ActionsTarget::Workflow { repository, id }, ActionsTarget::Workflow { repository: old, id: old_id })
                if repository == old && id == old_id)
        && actual.kind == expected.kind
        && actual.context == expected.context
        && actual.options == expected.options
        && actual.fingerprint_sha256 == expected.fingerprint_sha256
}
fn same_marker(actual: &ActionsWriteMarker, expected: &ActionsWriteMarker) -> bool {
    same_metadata(actual, expected)
        && actual.state == ActionsWriteState::Pending
        && actual.receipt.is_none()
}
fn expected_generations(permit: &ActionsWritePermit, increments: i64) -> Result<Option<Vec<i64>>> {
    permit
        .scoped_begin_generations_v3
        .as_ref()
        .map(|original| {
            original
                .iter()
                .map(|generation| {
                    generation
                        .checked_add(increments)
                        .filter(|generation| *generation > 0)
                        .context("Original Actions generation overflow")
                })
                .collect()
        })
        .transpose()
}
pub(super) fn before_claim(
    connection: &rusqlite::Transaction<'_>,
    permit: &ActionsWritePermit,
) -> Result<()> {
    if let Some(prior) = &permit.workflow_pr_prior_branch_v9 {
        prior.validate_retained_v9(connection, &permit.issuer, 1)?;
    }
    if let Some(proof) = &permit.force_escalation_v5 {
        force_escalation_v5::validate_retained_in_transaction(connection, proof)?;
    }
    if let Some(expected) = expected_generations(permit, 0)? {
        if generations(connection, &permit.marker.target)? != expected {
            bail!("Original scoped Actions generation changed before claim; no permission adopted");
        }
    }
    Ok(())
}
pub(super) fn before_finish(
    connection: &rusqlite::Transaction<'_>,
    permit: &ActionsWritePermit,
) -> Result<()> {
    if let Some(prior) = &permit.workflow_pr_prior_branch_v9 {
        prior.validate_retained_v9(
            connection,
            &permit.issuer,
            1 + i64::from(permit.used.load(Ordering::Acquire)),
        )?;
    }
    if let Some(proof) = &permit.force_escalation_v5 {
        force_escalation_v5::validate_retained_in_transaction(connection, proof)?;
    }
    if let Some(expected) =
        expected_generations(permit, i64::from(permit.used.load(Ordering::Acquire)))?
    {
        if generations(connection, &permit.marker.target)? != expected {
            bail!("Original scoped Actions generation changed before receipt persistence; pending protection retained");
        }
    }
    Ok(())
}
fn claimed(connection: &Connection, marker: &ActionsWriteMarker) -> Result<bool> {
    connection.query_row(
        "SELECT claimed FROM actions_write_attempts WHERE attempt_id=?1 AND host=?2 AND principal=?3 AND fingerprint_sha256=?4 AND state='pending'",
        params![marker.attempt_id, marker.target.scope.host, marker.target.scope.login, marker.fingerprint_sha256],
        |row| row.get::<_, bool>(0),
    ).optional().map(|value| value == Some(true)).map_err(Into::into)
}
impl ActionsWriteJournal {
    pub(crate) fn capture_claim_stamp_v3(
        &self,
        permit: &ActionsWritePermit,
    ) -> Result<ActionsClaimStampV3> {
        if let Some(prior) = &permit.workflow_pr_prior_branch_v9 {
            prior.validate_retained_v9(&self.connection, &self.path, 2)?;
        }
        if self.path != permit.issuer
            || !permit.used.load(Ordering::Acquire)
            || permit.marker.context.original_repository_v3.is_none()
        {
            bail!("Only the SAME actually claimed original Actions permit can be fenced");
        }
        let marker = bounded_original::read_checked_attempt(
            &self.connection,
            &permit.marker.target.scope,
            permit.marker.attempt_id,
        )?
        .context("Original claimed Actions marker unavailable")?;
        marker.validate_metadata()?;
        if !same_marker(&marker, &permit.marker) || !claimed(&self.connection, &marker)? {
            bail!("Original claimed Actions marker changed before invocation");
        }
        let file = file(&self.path)?;
        if let Some(proof) = &permit.force_escalation_v5 {
            force_escalation_v5::validate_retained(&self.connection, proof)?;
        }
        let generations = generations(&self.connection, &marker.target)?;
        if generations.len() != marker.target.resources.len()
            || generations.iter().any(|generation| *generation <= 0)
            || expected_generations(permit, 1)?.as_ref() != Some(&generations)
        {
            bail!("Original Actions resource generation is missing or invalid");
        }
        Ok(ActionsClaimStampV3 {
            path: self.path.clone(),
            file,
            marker,
            generations,
            force_escalation_v5: permit.force_escalation_v5.clone(),
            workflow_pr_prior_branch_v9: permit.workflow_pr_prior_branch_v9.clone(),
        })
    }
    pub(crate) fn validate_claim_stamp_v3(&self, stamp: &ActionsClaimStampV3) -> Result<()> {
        if let Some(prior) = &stamp.workflow_pr_prior_branch_v9 {
            prior.validate_retained_v9(&self.connection, &self.path, 2)?;
        }
        if let Some(proof) = &stamp.force_escalation_v5 {
            force_escalation_v5::validate_retained(&self.connection, proof)?;
        }
        if self.path != stamp.path
            || std::fs::canonicalize(&self.path)? != self.path
            || file(&self.path)? != stamp.file
            || generations(&self.connection, &stamp.marker.target)? != stamp.generations
        {
            bail!("Original Actions journal or exact resource generation changed before POST");
        }
        let marker = bounded_original::read_checked_attempt(
            &self.connection,
            &stamp.marker.target.scope,
            stamp.marker.attempt_id,
        )?
        .context("Original claimed Actions marker unavailable")?;
        marker.validate_metadata()?;
        if !same_marker(&marker, &stamp.marker) || !claimed(&self.connection, &marker)? {
            bail!("Original Actions claimed marker is no longer current; no POST authorized");
        }
        Ok(())
    }
    /// Same original claimed marker after exactly one durable terminal state
    /// transition. No new source/auth/ACK authority or generation is minted.
    pub(crate) fn validate_result_stamp_v3(
        &self,
        stamp: &ActionsClaimStampV3,
        state: ActionsWriteState,
        receipt: Option<&ActionsReceipt>,
    ) -> Result<()> {
        if let Some(prior) = &stamp.workflow_pr_prior_branch_v9 {
            prior.validate_retained_v9(
                &self.connection,
                &self.path,
                2 + i64::from(state != ActionsWriteState::Pending),
            )?;
        }
        if let Some(proof) = &stamp.force_escalation_v5 {
            force_escalation_v5::validate_retained(&self.connection, proof)?;
        }
        if !matches!(
            state,
            ActionsWriteState::Pending
                | ActionsWriteState::Uncertain
                | ActionsWriteState::Confirmed
                | ActionsWriteState::AcceptedPending
        ) {
            bail!("No original publication result state adapter");
        }
        let expected = stamp
            .generations
            .iter()
            .map(|generation| {
                generation
                    .checked_add(i64::from(state != ActionsWriteState::Pending))
                    .context("Original Actions result generation overflow")
            })
            .collect::<Result<Vec<_>>>()?;
        if self.path != stamp.path
            || file(&self.path)? != stamp.file
            || generations(&self.connection, &stamp.marker.target)? != expected
        {
            bail!("Original Actions result journal/generation changed before visibility");
        }
        let actual = bounded_original::read_checked_attempt(
            &self.connection,
            &stamp.marker.target.scope,
            stamp.marker.attempt_id,
        )?
        .context("Original Actions result unavailable")?;
        actual.validate_metadata()?;
        if !same_metadata(&actual, &stamp.marker)
            || actual.state != state
            || actual.receipt.as_ref() != receipt
        {
            bail!("Original Actions result differs from its exact typed receipt/state");
        }
        Ok(())
    }
}
