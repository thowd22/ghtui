//! Required publication verification and optional refresh are distinct worker phases.
use super::{publication::ActionsDisposition, *};
use crate::actions_write_journal::{ActionsReceipt, ActionsReceiptKind};
use crate::providers::gh::{
    ActionsInvocationFailureV3, ScopedActionsRunTargetV1, ScopedRunInvocationRefV3,
};
use std::{sync::Arc, time::Instant};
mod native_dispatch_v7;
mod native_workflow_remote_v9;

/// The durable caller records this typed disposition before optional follow-up reads.
#[derive(Clone, Debug)]
pub struct ActionsPublication {
    pub message: String,
    pub workspace: ActionsWorkspace,
    pub disposition: ActionsDisposition,
    refresh: PostPublicationRefresh,
}
#[derive(Clone, Debug)]
pub struct ActionsRefresh {
    pub outcome: ActionsOutcome,
    /// Actual optional I/O failure or observed immutable-context drift only.
    /// Existing warnings/message text are never interpreted as a failure proof.
    pub refresh_failed: bool,
}
#[derive(Clone, Debug)]
enum PostPublicationRefresh {
    None,
    Deployment {
        repository: String,
        run: Box<WorkflowRun>,
    },
    CacheUsage {
        repository: String,
    },
}
fn verified_receipt(kind: ActionsReceiptKind, resource_id: Option<u64>) -> ActionsDisposition {
    ActionsDisposition::VerifiedEffect {
        receipt: ActionsReceipt {
            kind,
            resource_id,
            observed_attempt: None,
            observed_run_id: None,
            original_dispatch_run_v7: None,
            workflow_remote_v9: None,
        },
    }
}
fn adopt_run_result_v3(
    workspace: &mut ActionsWorkspace,
    repository: &str,
    run_id: u64,
    updated: Option<WorkflowRun>,
) {
    if let Some(updated) = updated {
        for item in &mut workspace.runs {
            if item.repository == repository && item.run.id == updated.id {
                item.run = updated.clone();
            }
        }
        if let Some(page) = &mut workspace.run {
            if page.attempt.run.attempt != updated.attempt {
                page.attempt.jobs.clear();
                page.selected_job = None;
                page.logs = None;
                page.annotations.clear();
            }
            page.attempt.run = updated;
        }
    } else {
        workspace
            .runs
            .retain(|item| item.repository != repository || item.run.id != run_id);
        workspace.run = None;
        workspace.view = ActionsView::Runs;
    }
}
fn finish_publication_v3(
    mut workspace: ActionsWorkspace,
    message: String,
    disposition: ActionsDisposition,
    refresh: PostPublicationRefresh,
) -> ActionsPublication {
    workspace.reconciliation_target = None;
    workspace.observation = None;
    workspace.fetched_at = Some(now());
    workspace.prepare_rows();
    ActionsPublication {
        message,
        workspace,
        disposition,
        refresh,
    }
}
impl ActionsPlan {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn publish_workflow_guarded_v6<F>(
        &self,
        original_provider: &GithubProvider,
        measured: &GithubProvider,
        captures: &mut crate::providers::gh::PostWriteCaptureAllowanceV2,
        source: &crate::scoped_actions_v1::ScopedActionsSourceV1,
        original: &Arc<crate::providers::gh::ScopedActionsWorkflowV2>,
        attempt_id: i64,
        enabled: bool,
        cancel: &CancellationToken,
        deadline: Instant,
        before_write: F,
    ) -> Result<ActionsPublication, ActionsInvocationFailureV3>
    where
        F: FnOnce(crate::providers::gh::ScopedWorkflowInvocationV6<'_>) -> GithubResult<()>,
    {
        if attempt_id <= 0 {
            return Err(ActionsInvocationFailureV3::NotStarted(plain_error(
                "Actual workflow journal attempt required",
            )));
        }
        self.scoped_workflow_state_descriptor_v6(original_provider, source, original, enabled)
            .map_err(ActionsInvocationFailureV3::NotStarted)?;
        let outcome = original_provider.mutate_workflow_enabled_guarded_v6(
            measured,
            captures,
            &self.identity,
            original,
            enabled,
            cancel,
            deadline,
            before_write,
        )?;
        if !Arc::ptr_eq(outcome.original(), original) || outcome.enabled() != enabled {
            return Err(ActionsInvocationFailureV3::WriteMayHaveStarted(
                plain_error("Original workflow receipt context changed"),
            ));
        }
        let receipt = ActionsReceipt {
            kind: ActionsReceiptKind::WorkflowState,
            resource_id: Some(original.model().id),
            observed_attempt: None,
            observed_run_id: None,
            original_dispatch_run_v7: None,
            workflow_remote_v9: None,
        };
        let disposition = if outcome.state_observed() {
            ActionsDisposition::VerifiedEffect { receipt }
        } else {
            ActionsDisposition::Accepted {
                receipt,
                observation: crate::actions_workspace::ActionsAcceptedObservation::WorkflowState {
                    workflow_id: original.model().id,
                    desired_enabled: enabled,
                    observed_state: outcome.observed().model().state.clone(),
                },
            }
        };
        let updated = outcome.observed().model();
        // The actual scoped Form owns the original immutable Definition.
        // This return workspace is metadata only: never clone its YAML/jobs or
        // expand every YAML/matrix row merely to discard them in native publish.
        // Legacy None keeps its existing full-workspace publication path.
        let mut workspace = ActionsWorkspace::new(
            self.identity.clone(),
            vec![original.repository().name().into()],
        );
        workspace.view = ActionsView::PublicationInspection;
        workspace.publication_inspection = Some(ActionsPublicationInspectionPage {
            attempt_id,
            heading: format!("Original workflow publication #{attempt_id} • actual state observation"),
            lines: vec![format!("Workflow {} • path {} • state {}", updated.id, updated.path, updated.state),
                "HTTP204 is acceptance; current observed state is not completion or causation. No YAML rows or new command authority in this result.".into()],
            acknowledgement: None,
        });
        Ok(finish_publication_v3(
            workspace,
            if outcome.state_observed() {
                "GitHub accepted workflow state request; exact same workflow state observed • causation not claimed".into()
            } else {
                "HTTP204 accepted • requested workflow state not yet observed • no retry".into()
            },
            disposition,
            PostPublicationRefresh::None,
        ))
    }
    /// New whole-run controls reuse the SAME actual invocation and disposition
    /// body. Only actual private HTTP204 + exact run lookup + repository close
    /// can construct deletion acceptance; no bounded catalogue absence proof.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn publish_run_guarded_v5<F>(
        &self,
        original_provider: &GithubProvider,
        measured: &GithubProvider,
        captures: &mut crate::providers::gh::PostWriteCaptureAllowanceV2,
        source: &crate::scoped_actions_v1::ScopedActionsSourceV1,
        original: &Arc<ScopedActionsRunTargetV1>,
        action: RunMutation,
        cancel: &CancellationToken,
        deadline: Instant,
        before_write: F,
    ) -> Result<ActionsPublication, ActionsInvocationFailureV3>
    where
        F: FnOnce(ScopedRunInvocationRefV3<'_>) -> GithubResult<()>,
    {
        if !matches!(
            action,
            RunMutation::ForceCancel | RunMutation::DeleteRun | RunMutation::DeleteLogs
        ) {
            return Err(ActionsInvocationFailureV3::NotStarted(plain_error(
                "No scoped whole-run control adapter",
            )));
        }
        self.scoped_run_descriptor_v4(original_provider, source, original, None, action)
            .map_err(ActionsInvocationFailureV3::NotStarted)?;
        let repository = self
            .workspace
            .repository()
            .map_err(ActionsInvocationFailureV3::NotStarted)?;
        let closed = original_provider.mutate_run_guarded_v5(
            measured,
            captures,
            &self.identity,
            original,
            repository,
            action,
            source.original().source(),
            cancel,
            deadline,
            before_write,
        )?;
        let disposition = if action == RunMutation::DeleteRun {
            if !closed.deletion_accepted() {
                return Err(ActionsInvocationFailureV3::WriteMayHaveStarted(
                    plain_error("Actual fixed original HTTP204 acceptance unavailable"),
                ));
            }
            ActionsDisposition::Accepted {
                receipt: ActionsReceipt {
                    kind: ActionsReceiptKind::RunDeletionAccepted,
                    resource_id: Some(original.run().id),
                    observed_attempt: Some(original.run().attempt),
                    observed_run_id: None,
                    original_dispatch_run_v7: None,
                    workflow_remote_v9: None,
                },
                observation:
                    crate::actions_workspace::ActionsAcceptedObservation::RunDeletionAcceptance {
                        run_id: original.run().id,
                        requested_attempt: original.run().attempt,
                        head_sha: original.run().head_sha.clone(),
                        availability: closed.deletion_availability().ok_or_else(|| {
                            ActionsInvocationFailureV3::WriteMayHaveStarted(plain_error(
                                "Actual original lookup/repository closure unavailable",
                            ))
                        })?,
                    },
            }
        } else {
            self.run_disposition(closed.outcome())
                .map_err(ActionsInvocationFailureV3::WriteMayHaveStarted)?
        };
        let outcome = closed.into_outcome();
        let mut workspace = self.workspace.clone();
        // Retaining the exact original presentation for deletion does NOT claim
        // it still exists; the acceptance message/typed availability is explicit.
        if action != RunMutation::DeleteRun {
            adopt_run_result_v3(&mut workspace, repository, original.run().id, outcome.run);
        }
        Ok(finish_publication_v3(
            workspace,
            outcome.explanation,
            disposition,
            PostPublicationRefresh::None,
        ))
    }
    /// Same private Plan/disposition/update model and same provider mutation
    /// body; this adapter grants no authority to public target/preview labels.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn publish_run_guarded_v3<F>(
        &self,
        original_provider: &GithubProvider,
        measured: &GithubProvider,
        captures: &mut crate::providers::gh::PostWriteCaptureAllowanceV2,
        source: &crate::scoped_actions_v1::ScopedActionsSourceV1,
        original: &Arc<ScopedActionsRunTargetV1>,
        cancel: &CancellationToken,
        deadline: Instant,
        before_write: F,
    ) -> Result<ActionsPublication, ActionsInvocationFailureV3>
    where
        F: FnOnce(ScopedRunInvocationRefV3<'_>) -> GithubResult<()>,
    {
        self.publish_run_guarded_v4(
            original_provider,
            measured,
            captures,
            source,
            original,
            None,
            RunMutation::Cancel,
            cancel,
            deadline,
            before_write,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn publish_run_guarded_v4<F>(
        &self,
        original_provider: &GithubProvider,
        measured: &GithubProvider,
        captures: &mut crate::providers::gh::PostWriteCaptureAllowanceV2,
        source: &crate::scoped_actions_v1::ScopedActionsSourceV1,
        original: &Arc<ScopedActionsRunTargetV1>,
        selected_job: Option<&Arc<ScopedActionsJobTargetV1>>,
        action: RunMutation,
        cancel: &CancellationToken,
        deadline: Instant,
        before_write: F,
    ) -> Result<ActionsPublication, ActionsInvocationFailureV3>
    where
        F: FnOnce(ScopedRunInvocationRefV3<'_>) -> GithubResult<()>,
    {
        self.scoped_run_descriptor_v4(original_provider, source, original, selected_job, action)
            .map_err(ActionsInvocationFailureV3::NotStarted)?;
        let repository = self
            .workspace
            .repository()
            .map_err(ActionsInvocationFailureV3::NotStarted)?;
        let outcome = if action == RunMutation::Cancel {
            original_provider.mutate_run_guarded_v3(
                measured,
                captures,
                &self.identity,
                original,
                repository,
                action,
                cancel,
                deadline,
                before_write,
            )
        } else {
            original_provider.mutate_run_guarded_v4(
                measured,
                captures,
                &self.identity,
                original,
                selected_job,
                repository,
                action,
                cancel,
                deadline,
                before_write,
            )
        }?;
        let disposition = self
            .run_disposition(&outcome)
            .map_err(ActionsInvocationFailureV3::WriteMayHaveStarted)?;
        let mut workspace = self.workspace.clone();
        adopt_run_result_v3(&mut workspace, repository, original.run().id, outcome.run);
        Ok(finish_publication_v3(
            workspace,
            outcome.explanation,
            disposition,
            PostPublicationRefresh::None,
        ))
    }
    /// Caller must first persist and claim the matching descriptor's single-use permit.
    /// Local browsing/export bypasses that remote journal, not its normal confirmations.
    pub fn publish(
        &self,
        gh: &GithubProvider,
        confirmed: bool,
        cancel: &CancellationToken,
    ) -> GithubResult<ActionsPublication> {
        if !confirmed {
            return Err(plain_error("Actions effect was not confirmed"));
        }
        let descriptor = self.publication_descriptor(gh)?;
        if descriptor.is_none() {
            let outcome = self.execute_legacy_effects(gh, confirmed, cancel)?;
            return Ok(ActionsPublication {
                message: outcome.message,
                workspace: outcome.workspace,
                disposition: ActionsDisposition::LocalOnly,
                refresh: PostPublicationRefresh::None,
            });
        }
        let mut workspace = self.workspace.clone();
        let repository = workspace.repository()?.to_owned();
        let identity = &self.identity;
        let (message, disposition, refresh) = match &self.operation {
            ActionsOperation::Run { run, action } => {
                let outcome = gh.mutate_run(identity, &repository, run, *action, cancel)?;
                let disposition = self.run_disposition(&outcome)?;
                adopt_run_result_v3(&mut workspace, &repository, run.id, outcome.run);
                (
                    outcome.explanation,
                    disposition,
                    PostPublicationRefresh::None,
                )
            }
            ActionsOperation::Dispatch { definition, inputs } => {
                let outcome =
                    gh.dispatch_workflow(identity, &repository, definition, inputs, cancel)?;
                let disposition = self.dispatch_disposition(&outcome)?;
                if let Some(run) = outcome.run {
                    workspace.runs.insert(
                        0,
                        ScopedRun {
                            repository: repository.clone(),
                            run,
                        },
                    );
                }
                (
                    outcome.explanation,
                    disposition,
                    PostPublicationRefresh::None,
                )
            }
            ActionsOperation::Deployment { run, review } => {
                let outcome = gh.review_deployments(identity, &repository, run, review, cancel)?;
                let disposition = self.deployment_disposition(&outcome)?;
                workspace.deployments = outcome.remaining;
                for id in &review.environment_ids {
                    if workspace
                        .deployments
                        .iter()
                        .any(|gate| gate.environment_id == *id)
                    {
                        workspace.accepted_deployments.insert(*id);
                    }
                }
                (format!("Decision {} accepted • selected {:?} • remaining gates {} • selected gates no longer pending {}",
                    if review.decision == DeploymentDecision::Approved {"approved"} else {"rejected"},review.environment_ids,workspace.deployments.len(),outcome.completed),
                    disposition,PostPublicationRefresh::Deployment {repository,run:Box::new(run.clone())})
            }
            ActionsOperation::DeleteCache(cache) => {
                workspace.caches = gh.delete_cache(identity, &repository, cache, cancel)?;
                workspace.cache_key = None;
                workspace.cache_ref = None;
                workspace.selected = 0;
                (
                    format!(
                        "Only cache {} deleted; unrelated caches preserved",
                        cache.id
                    ),
                    verified_receipt(ActionsReceiptKind::CacheDeleted, Some(cache.id)),
                    PostPublicationRefresh::CacheUsage { repository },
                )
            }
            ActionsOperation::CreateGroup { org, request } => {
                let group = gh.create_runner_group(identity, org, request, cancel)?;
                let id = group.id;
                workspace.runner_groups.push(group);
                (
                    format!("Runner group {id} created and policy/memberships confirmed"),
                    verified_receipt(ActionsReceiptKind::SettingsMetadata, Some(id)),
                    PostPublicationRefresh::None,
                )
            }
            _ => {
                // These operations have no optional post-publication I/O. Their
                // typed provider performs its exact required effect verification.
                // The legacy body will become a private helper at activation.
                let outcome = self.execute_legacy_effects(gh, true, cancel)?;
                workspace = outcome.workspace;
                let disposition = if descriptor.is_none() {
                    ActionsDisposition::LocalOnly
                } else {
                    let (kind, id) = match &self.operation {
                        ActionsOperation::WorkflowState { workflow, .. } => {
                            (ActionsReceiptKind::WorkflowState, Some(workflow.id))
                        }
                        ActionsOperation::DeleteArtifact(artifact) => {
                            (ActionsReceiptKind::ArtifactDeleted, Some(artifact.id))
                        }
                        ActionsOperation::SetSecret { .. } | ActionsOperation::DeleteSecret(_) => {
                            (ActionsReceiptKind::SecretMetadata, None)
                        }
                        ActionsOperation::SetVariable { .. }
                        | ActionsOperation::DeleteVariable(_) => {
                            (ActionsReceiptKind::VariableMetadata, None)
                        }
                        ActionsOperation::Environment { observed, .. } => (
                            ActionsReceiptKind::SettingsMetadata,
                            observed["id"].as_u64().filter(|id| *id > 0),
                        ),
                        ActionsOperation::RunnerLabels { runner, .. }
                        | ActionsOperation::RemoveRunner(runner) => {
                            (ActionsReceiptKind::SettingsMetadata, Some(runner.id))
                        }
                        ActionsOperation::EditGroup { observed, .. }
                        | ActionsOperation::DeleteGroup { observed, .. } => {
                            (ActionsReceiptKind::SettingsMetadata, Some(observed.id))
                        }
                        ActionsOperation::GroupAssociations { group, .. } => {
                            (ActionsReceiptKind::SettingsMetadata, Some(group.id))
                        }
                        ActionsOperation::Policy { .. } => {
                            (ActionsReceiptKind::SettingsMetadata, None)
                        }
                        _ => {
                            return Err(GithubError {
                                kind: GithubErrorKind::AmbiguousWrite,
                                message: "Published Actions effect lacks a closed typed receipt"
                                    .into(),
                            })
                        }
                    };
                    verified_receipt(kind, id)
                };
                (outcome.message, disposition, PostPublicationRefresh::None)
            }
        };
        Ok(finish_publication_v3(
            workspace,
            message,
            disposition,
            refresh,
        ))
    }
}
impl ActionsPublication {
    /// Run only after durable confirmation/acceptance was stored. Failures here
    /// never resend a mutation or alter the already-recorded disposition.
    pub fn refresh(self, gh: &GithubProvider, cancel: &CancellationToken) -> ActionsOutcome {
        self.refresh_checked(gh, cancel).outcome
    }
    pub fn refresh_checked(
        mut self,
        gh: &GithubProvider,
        cancel: &CancellationToken,
    ) -> ActionsRefresh {
        let mut refresh_failed = false;
        match &self.refresh {
            PostPublicationRefresh::None => {}
            PostPublicationRefresh::CacheUsage { repository } => {
                self.workspace.cache_usage = match gh.cache_usage(repository, cancel) {
                    Ok(usage) => Some(usage),
                    Err(error) => {
                        refresh_failed = true;
                        self.workspace.warnings.push(format!(
                            "Deletion confirmed, but usage refresh is unavailable: {}",
                            error.message
                        ));
                        None
                    }
                };
            }
            PostPublicationRefresh::Deployment { repository, run } => {
                let run_state = match gh.run(repository, run.id, cancel) {
                    Ok(fresh)
                        if fresh.head_sha == run.head_sha
                            && fresh.attempt == run.attempt
                            && fresh.workflow_id == run.workflow_id =>
                    {
                        let state = format!("{} / {}", fresh.status, fresh.conclusion);
                        if let Some(page) = &mut self.workspace.run {
                            page.attempt.run = fresh;
                        }
                        state
                    }
                    Ok(_) => {
                        refresh_failed = true;
                        self.workspace.warnings.push("Decision accepted, but run identity advanced; reload its exact attempt before another decision".into());
                        "identity advanced; reload required".into()
                    }
                    Err(error) => {
                        refresh_failed = true;
                        self.workspace.warnings.push(format!(
                            "Decision accepted; run refresh unavailable: {}",
                            error.message
                        ));
                        "refresh unavailable".into()
                    }
                };
                self.workspace.approval_history_error = None;
                self.workspace.approval_history =
                    match gh.deployment_review_history(repository, run.id, cancel) {
                        Ok(history) => history,
                        Err(error) => {
                            refresh_failed = true;
                            self.workspace.approval_history_error = Some(error.message.clone());
                            self.workspace.warnings.push(format!(
                                "Decision accepted; approval history unavailable: {}",
                                error.message
                            ));
                            vec![]
                        }
                    };
                self.message.push_str(&format!(" • run {run_state}"));
            }
        }
        self.workspace.prepare_rows();
        self.workspace.warnings.push(self.message.clone());
        ActionsRefresh {
            outcome: ActionsOutcome {
                message: self.message,
                workspace: self.workspace,
            },
            refresh_failed,
        }
    }
}

#[cfg(test)]
mod tests;
