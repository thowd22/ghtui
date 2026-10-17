//! One narrow accepted-Cancel exception. Saved metadata is descriptive only;
//! only a fresh actual source/run can create the process-local proof. Neither
//! begin nor claim modifies, acknowledges or releases the original Cancel.
use super::*;
use crate::{providers::gh::ScopedActionsRunTargetV1, scoped_actions_v1::ScopedActionsSourceV1};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionsForceEscalationContextV5 {
    pub cancel: ActionsPublicationReference,
    pub cancel_resources: Vec<String>,
}
pub(crate) struct ScopedForceEscalationV5 {
    issuer: PathBuf,
    source: Arc<ScopedActionsSourceV1>,
    run: Arc<ScopedActionsRunTargetV1>,
    original: ActionsWriteMarker,
    generations: Vec<i64>,
}
impl fmt::Debug for ScopedForceEscalationV5 {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.write_str(
            "ScopedForceEscalationV5(<one accepted original Cancel, not restored authority>)",
        )
    }
}
pub(super) fn validate_metadata(context: &ActionsImmutableContext) -> Result<()> {
    let Some(saved) = &context.force_escalation_v5 else {
        return Ok(());
    };
    if context.original_repository_v3.is_none()
        || context.original_run_v3.is_none()
        || saved.cancel.attempt_id <= 0
        || saved.cancel.fingerprint_sha256.len() != 64
        || !saved
            .cancel
            .fingerprint_sha256
            .bytes()
            .all(|b| b.is_ascii_hexdigit())
        || saved.cancel_resources.is_empty()
        || saved.cancel_resources.len() > 16
        || saved.cancel_resources.iter().any(|key| key.len() > 4096)
        || saved
            .cancel_resources
            .windows(2)
            .any(|keys| keys[0] >= keys[1])
    {
        bail!("Force escalation requires bounded exact descriptive original Cancel metadata");
    }
    Ok(())
}
pub(super) fn validate_aliases(
    host: &str,
    run: u64,
    saved: &ActionsForceEscalationContextV5,
) -> Result<()> {
    let numeric = format!("{host}/repository-id/");
    let numeric_suffix = format!("/actions/run/{run}");
    let name_suffix = format!("/run/{run}");
    for key in &saved.cancel_resources {
        if let Some(id) = key
            .strip_prefix(&numeric)
            .and_then(|s| s.strip_suffix(&numeric_suffix))
        {
            if id
                .parse::<u64>()
                .ok()
                .is_none_or(|id| id == 0 || id > i64::MAX as u64)
            {
                bail!("Original Cancel numeric alias is invalid");
            }
        } else {
            let repository = key
                .strip_prefix(&format!("{host}/"))
                .and_then(|s| s.strip_suffix(&name_suffix))
                .context("Original Cancel alias belongs to another host")?;
            let canonical = crate::providers::gh::canonical_actions_resource_keys(
                host,
                &ActionsTarget::Run {
                    repository: repository.into(),
                    id: run,
                },
            )
            .map_err(anyhow::Error::msg)?;
            if canonical.len() != 1 || &canonical[0] != key {
                bail!("Original Cancel alias is not canonical");
            }
        }
    }
    Ok(())
}
fn same_original(a: &ActionsWriteMarker, b: &ActionsWriteMarker) -> bool {
    a.publication_reference() == b.publication_reference()
        && a.target.same_resources(&b.target)
        && a.kind == b.kind
        && a.context == b.context
        && a.options == b.options
        && a.state == b.state
        && a.receipt == b.receipt
}
fn accepted_cancel(marker: &ActionsWriteMarker, context: &ActionsImmutableContext) -> Result<()> {
    marker.validate_metadata()?;
    let old_repo = marker
        .context
        .original_repository_v3
        .as_ref()
        .context("Old Cancel repository unavailable")?;
    let repository = context
        .original_repository_v3
        .as_ref()
        .context("Current original repository unavailable")?;
    if marker.kind != ActionsWriteKind::Cancel
        || marker.state != ActionsWriteState::AcceptedPending
        || marker.context.force_escalation_v5.is_some()
        || marker.context.run_id != context.run_id
        || marker.context.attempt != context.attempt
        || marker.context.head_sha != context.head_sha
        || marker.context.original_run_v3 != context.original_run_v3
        || marker.context.original_pr_v3 != context.original_pr_v3
        || old_repo.repository_id != repository.repository_id
        || old_repo.repository_node != repository.repository_node
        || marker.receipt.as_ref().is_none_or(|receipt| {
            receipt.kind != ActionsReceiptKind::RunTransition
                || receipt.resource_id != context.run_id
                || receipt.observed_attempt != context.attempt
        })
    {
        bail!("Only the SAME accepted normal Cancel/current active original may be escalated; all other protections remain");
    }
    Ok(())
}
impl ScopedForceEscalationV5 {
    pub(crate) fn metadata(&self) -> ActionsForceEscalationContextV5 {
        ActionsForceEscalationContextV5 {
            cancel: self.original.publication_reference(),
            cancel_resources: self.original.target.resources.clone(),
        }
    }
    pub(crate) fn matches(
        &self,
        source: &Arc<ScopedActionsSourceV1>,
        run: &Arc<ScopedActionsRunTargetV1>,
    ) -> bool {
        Arc::ptr_eq(&self.source, source) && Arc::ptr_eq(&self.run, run)
    }
}
impl ActionsWriteJournal {
    pub(crate) fn validate_prepared_force_v5(&self, proof: &ScopedForceEscalationV5) -> Result<()> {
        if self.path != proof.issuer
            || generations(&self.connection, &proof.original.target)? != proof.generations
        {
            bail!("Displayed original accepted Cancel/generation changed");
        }
        proof
            .source
            .validate_local(&self.path)
            .map_err(anyhow::Error::msg)?;
        validate_retained(&self.connection, proof)
    }
    /// Called on the fresh Confirm source BEFORE its final SQL/UID close.
    /// A new Force without a blocker needs no exception or saved metadata.
    pub(crate) fn prepare_force_escalation_v5(
        &self,
        provider: &GithubProvider,
        source: &Arc<ScopedActionsSourceV1>,
        run: &Arc<ScopedActionsRunTargetV1>,
        target: &ActionsWriteTarget,
        intent: &ActionsWriteIntent,
    ) -> Result<Option<Arc<ScopedForceEscalationV5>>> {
        if intent.kind != ActionsWriteKind::ForceCancel
            || intent.context.force_escalation_v5.is_some()
            || !run.original_writer_v3(provider)
            || run.run().status == "completed"
            || run.repository_id() != source.reference().id
            || source.reference().node_id.as_deref() != run.repository_node()
            || source.original().scope() != target.scope()
            || intent.context.run_id != Some(run.run().id)
            || intent.context.attempt != Some(run.run().attempt)
            || intent.context.head_sha.as_deref() != Some(run.run().head_sha.as_str())
            || intent
                .context
                .original_repository_v3
                .as_ref()
                .is_none_or(|repository| {
                    repository.repository_id != run.repository_id()
                        || Some(repository.repository_node.as_str()) != run.repository_node()
                })
            || intent.context.original_run_v3.as_ref().is_none_or(|saved| {
                Some(saved.run_node.as_str()) != run.node_v2()
                    || saved.workflow_id != run.run().workflow_id
                    || saved.check_suite_id != run.suite_v3()
            })
            || intent.context.original_pr_v3
                != source.pr_origin_v2().map(|p| p.durable_pr_context_v3())
        {
            bail!("Force escalation needs the actual fresh original active source/run/context");
        }
        source
            .validate_local(&self.path)
            .map_err(anyhow::Error::msg)?;
        intent.fingerprint(target)?;
        let transaction = self.connection.unchecked_transaction()?;
        scoped_context_v3::before_begin(&transaction, target, intent)?;
        let blocked = blocking(&transaction, target, intent.kind)?;
        let proof = match blocked.as_slice() {
            [] => None,
            [old] => {
                accepted_cancel(old, &intent.context)?;
                Some(Arc::new(ScopedForceEscalationV5 { issuer: self.path.clone(), source: source.clone(), run: run.clone(),
                    original: old.clone(), generations: generations(&transaction, &old.target)? }))
            },
            _ => bail!("Force escalation needs exactly ONE accepted original Cancel; ambiguous/other barriers remain"),
        };
        transaction.commit()?;
        Ok(proof)
    }
    pub(crate) fn begin_scoped_force_v5(
        &mut self,
        target: &ActionsWriteTarget,
        intent: &ActionsWriteIntent,
        proof: Arc<ScopedForceEscalationV5>,
    ) -> Result<ActionsWritePermit> {
        self.begin_with_force_v5(target, intent, Some(proof))
    }
}
pub(super) fn before_begin(
    transaction: &Connection,
    issuer: &Path,
    target: &ActionsWriteTarget,
    intent: &ActionsWriteIntent,
    proof: &ScopedForceEscalationV5,
    blocked: &[ActionsWriteMarker],
) -> Result<()> {
    if issuer != proof.issuer
        || intent.kind != ActionsWriteKind::ForceCancel
        || intent.context.force_escalation_v5.as_ref() != Some(&proof.metadata())
        || target.scope != proof.original.target.scope
        || blocked.len() != 1
        || !same_original(&blocked[0], &proof.original)
        || generations(transaction, &proof.original.target)? != proof.generations
    {
        bail!("Original accepted Cancel changed before atomic Force begin; nothing inserted");
    }
    accepted_cancel(&blocked[0], &intent.context)?;
    Ok(())
}
pub(super) fn validate_retained(
    connection: &Connection,
    proof: &ScopedForceEscalationV5,
) -> Result<()> {
    let old = bounded_original::read_checked_attempt(
        connection,
        &proof.original.target.scope,
        proof.original.attempt_id,
    )?;
    validate_retained_marker(old, proof)
}
pub(super) fn validate_retained_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    proof: &ScopedForceEscalationV5,
) -> Result<()> {
    let old = bounded_original::read_checked_attempt_in_transaction(
        transaction,
        &proof.original.target.scope,
        proof.original.attempt_id,
    )?;
    validate_retained_marker(old, proof)
}
fn validate_retained_marker(
    old: Option<ActionsWriteMarker>,
    proof: &ScopedForceEscalationV5,
) -> Result<()> {
    let old = old.context("Original accepted Cancel disappeared")?;
    if !same_original(&old, &proof.original) {
        bail!("Original accepted Cancel changed; Force protection remains");
    }
    Ok(())
}

#[cfg(test)]
#[path = "force_escalation_v5_tests.rs"]
mod tests;
