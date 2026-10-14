//! Required publication verification and optional refresh are distinct worker phases.
use super::{publication::ActionsDisposition, *};
use crate::actions_write_journal::{ActionsReceipt, ActionsReceiptKind};

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
        },
    }
}
impl ActionsPlan {
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
                if let Some(updated) = outcome.run {
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
                        .retain(|item| item.repository != repository || item.run.id != run.id);
                    workspace.run = None;
                    workspace.view = ActionsView::Runs;
                }
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
        workspace.reconciliation_target = None;
        workspace.observation = None;
        workspace.fetched_at = Some(now());
        workspace.prepare_rows();
        Ok(ActionsPublication {
            message,
            workspace,
            disposition,
            refresh,
        })
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
