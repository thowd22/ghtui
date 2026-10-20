//! Private-operation-derived durable publication descriptors. No send or database access.
//!
//! The descriptor comes from the private prepared operation, never rendered rows, public preview
//! strings, mutable form contents, or an interpreted success message.
use super::*;
use crate::actions_write_journal::{
    ActionsImmutableContext, ActionsReceipt, ActionsReceiptKind, ActionsSafeOptions,
    ActionsWriteIntent, ActionsWriteKind, ActionsWriteTarget,
};
use serde::Serialize;
use std::io::{self, Write};

#[cfg(test)]
#[path = "scoped_publication_v3_tests.rs"]
mod scoped_publication_v3_tests;

/// Secret/body bindings remain in the redacted, non-serializable intent only.
#[derive(Clone, Debug)]
pub struct ActionsPublicationDescriptor {
    pub target: ActionsWriteTarget,
    pub intent: ActionsWriteIntent,
}

struct BindingWriter(Vec<u8>);
impl Write for BindingWriter {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        if self.0.len().saturating_add(input.len()) > 1024 * 1024 {
            return Err(io::Error::other("Actions transient binding exceeds 1 MiB"));
        }
        self.0.extend_from_slice(input);
        Ok(input.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn binding(input: &impl Serialize) -> GithubResult<Vec<u8>> {
    let mut output = BindingWriter(Vec::new());
    // Do not expose serialization errors: their implementation may contain content.
    serde_json::to_writer(&mut output, input)
        .map_err(|_| plain_error("Actions transient binding is invalid or exceeds its budget"))?;
    Ok(output.0)
}
fn run_context(run: &WorkflowRun) -> ActionsImmutableContext {
    ActionsImmutableContext {
        run_id: Some(run.id),
        attempt: Some(run.attempt),
        head_sha: Some(run.head_sha.clone()),
        ..ActionsImmutableContext::default()
    }
}
struct PreparedDescriptor {
    command: ActionsCommand,
    kind: ActionsWriteKind,
    target: ActionsTarget,
    context: ActionsImmutableContext,
    options: ActionsSafeOptions,
    payload: Vec<u8>,
}
impl PreparedDescriptor {
    fn new(command: ActionsCommand, kind: ActionsWriteKind, target: ActionsTarget) -> Self {
        Self {
            command,
            kind,
            target,
            context: ActionsImmutableContext::default(),
            options: ActionsSafeOptions::default(),
            payload: vec![],
        }
    }
    fn bind(mut self, input: &impl Serialize) -> GithubResult<Self> {
        self.payload = binding(input)?;
        Ok(self)
    }
}

impl ActionsPlan {
    pub(crate) fn scoped_workflow_owned_bytes_v6(&self) -> GithubResult<usize> {
        let ActionsOperation::WorkflowState { workflow, .. } = &self.operation else {
            return Err(plain_error("Not an actual workflow state Plan"));
        };
        let workspace = crate::actions_workspace::scoped_read_v1::retained_bytes(&self.workspace)
            .map_err(|_| plain_error("Workflow Plan model exceeds original bound"))?;
        let lines = self.preview.capacity() * std::mem::size_of::<String>()
            + self.preview.iter().map(String::capacity).sum::<usize>();
        let target = match &self.target {
            Some(ActionsTarget::Workflow { repository, .. }) => repository.capacity(),
            _ => 0,
        };
        let bytes = std::mem::size_of::<Self>()
            + workspace
            + lines
            + target
            + workflow.name.capacity()
            + workflow.path.capacity()
            + workflow.state.capacity()
            + self.identity.host.capacity()
            + self.identity.login.capacity()
            + self.identity.gh_version.capacity();
        if bytes > 8 * 1024 * 1024 {
            return Err(plain_error("Workflow Plan exceeds8MiB"));
        }
        Ok(bytes)
    }
    pub(crate) fn scoped_workflow_state_descriptor_v6(
        &self,
        provider: &GithubProvider,
        source: &crate::scoped_actions_v1::ScopedActionsSourceV1,
        original: &std::sync::Arc<crate::providers::gh::ScopedActionsWorkflowV2>,
        enabled: bool,
    ) -> GithubResult<ActionsPublicationDescriptor> {
        use crate::actions_write_journal::{
            ActionsOriginalRepositoryContextV3, ActionsOriginalWorkflowContextV6,
        };
        let ActionsOperation::WorkflowState {
            workflow,
            enabled: actual,
        } = &self.operation
        else {
            return Err(plain_error("No actual workflow state operation"));
        };
        let reference = source.reference();
        if *actual != enabled
            || !original.original_writer_v6(provider)
            || workflow.id != original.model().id
            || workflow.path != original.model().path
            || workflow.state != original.model().state
            || original.repository().id() != reference.id
            || original.repository().node() != reference.node_id.as_deref()
            || self.workspace.repository()? != original.repository().name()
            || crate::storage::Scope::github(&self.identity.host, self.identity.account_id)
                .ok()
                .as_ref()
                != Some(source.original().scope())
            || !state_eligible_v6(original.model(), enabled)
        {
            return Err(plain_error(
                "Original workflow source/identity/state/choice differs",
            ));
        }
        let mut descriptor = self
            .publication_descriptor(provider)?
            .ok_or_else(|| plain_error("Workflow state has no descriptor"))?;
        descriptor.intent.context.original_repository_v3 =
            Some(ActionsOriginalRepositoryContextV3 {
                repository_id: reference.id,
                repository_node: original
                    .repository()
                    .node()
                    .ok_or_else(|| plain_error("Original workflow repository node missing"))?
                    .into(),
                original_name: reference.last_name.clone(),
            });
        descriptor.intent.context.original_workflow_v6 = Some(ActionsOriginalWorkflowContextV6 {
            workflow_id: workflow.id,
            node: original
                .node_v6()
                .ok_or_else(|| plain_error("Original workflow node missing"))?
                .into(),
            path: workflow.path.clone(),
        });
        descriptor.intent.context.original_pr_v3 =
            source.pr_origin_v2().map(|pr| pr.durable_pr_context_v3());
        descriptor.target = ActionsWriteTarget::from_original_context_v3(
            source.original().scope(),
            descriptor.target.target(),
            &descriptor.intent.context,
        )
        .map_err(|_| plain_error("Original workflow aliases invalid"))?;
        descriptor
            .intent
            .fingerprint(&descriptor.target)
            .map_err(|_| plain_error("Original workflow metadata differs"))?;
        Ok(descriptor)
    }
    pub(crate) fn scoped_retained_bytes_v3(&self) -> GithubResult<usize> {
        let ActionsOperation::Run { run, action } = &self.operation else {
            return Err(plain_error(
                "No bounded original-source presentation for this operation",
            ));
        };
        if !matches!(
            action,
            RunMutation::Cancel
                | RunMutation::ForceCancel
                | RunMutation::DeleteRun
                | RunMutation::DeleteLogs
                | RunMutation::RerunAll { .. }
                | RunMutation::RerunFailed { .. }
                | RunMutation::RerunJob { .. }
        ) {
            return Err(plain_error(
                "No bounded original run presentation for this operation",
            ));
        }
        let workspace = crate::actions_workspace::scoped_read_v1::retained_bytes(&self.workspace)
            .map_err(plain_error)?;
        let preview = self.preview.iter().try_fold(
            self.preview
                .capacity()
                .saturating_mul(std::mem::size_of::<String>()),
            |sum, line| {
                sum.checked_add(line.capacity())
                    .ok_or_else(|| plain_error("Original Actions presentation allocation overflow"))
            },
        )?;
        let target = match &self.target {
            Some(ActionsTarget::Run { repository, .. }) => repository.capacity(),
            _ => 0,
        };
        let bytes = std::mem::size_of::<Self>()
            .saturating_add(workspace)
            .saturating_add(crate::actions_workspace::scoped_read_v1::run_owned_bytes(
                run,
            ))
            .saturating_add(preview)
            .saturating_add(target)
            .saturating_add(self.identity.host.capacity())
            .saturating_add(self.identity.login.capacity())
            .saturating_add(self.identity.gh_version.capacity());
        if bytes > crate::scoped_actions_v1::PREPARED_LIMIT_V1 {
            return Err(plain_error(
                "Original Actions Plan exceeds8MiB newly prepared allocation",
            ));
        }
        Ok(bytes)
    }
    /// Actual current source facts are mandatory for the scoped adapter. Public
    /// preview/target labels or restored metadata cannot create this descriptor.
    pub(crate) fn scoped_cancel_descriptor_v3(
        &self,
        provider: &GithubProvider,
        source: &crate::scoped_actions_v1::ScopedActionsSourceV1,
        original: &std::sync::Arc<crate::providers::gh::ScopedActionsRunTargetV1>,
    ) -> GithubResult<ActionsPublicationDescriptor> {
        self.scoped_run_descriptor_v4(provider, source, original, None, RunMutation::Cancel)
    }
    pub(crate) fn scoped_run_descriptor_v4(
        &self,
        provider: &GithubProvider,
        source: &crate::scoped_actions_v1::ScopedActionsSourceV1,
        original: &std::sync::Arc<crate::providers::gh::ScopedActionsRunTargetV1>,
        selected_job: Option<&std::sync::Arc<crate::providers::gh::ScopedActionsJobTargetV1>>,
        requested: RunMutation,
    ) -> GithubResult<ActionsPublicationDescriptor> {
        use crate::actions_write_journal::{
            ActionsOriginalRepositoryContextV3, ActionsOriginalRunContextV3,
        };
        let ActionsOperation::Run { run, action } = &self.operation else {
            return Err(plain_error(
                "No reviewed original-source adapter for this Actions operation",
            ));
        };
        let supported = match requested {
            RunMutation::Cancel
            | RunMutation::ForceCancel
            | RunMutation::DeleteRun
            | RunMutation::DeleteLogs
            | RunMutation::RerunAll { .. }
            | RunMutation::RerunFailed { .. } => selected_job.is_none(),
            RunMutation::RerunJob { job_id, .. } => selected_job.is_some_and(|job| {
                job.id() == job_id
                    && job.run().original_writer_v3(provider)
                    && job.run().same_watch_pin_v2(original)
            }),
            _ => false,
        };
        if !supported || *action != requested {
            return Err(plain_error(
                "Private original run command/options/member differ",
            ));
        }
        let reference = source.reference();
        let node = original
            .node_v2()
            .ok_or_else(|| plain_error("Actual original run node unavailable"))?;
        let repository_node = original
            .repository_node()
            .ok_or_else(|| plain_error("Actual original repository node unavailable"))?;
        if !original.original_writer_v3(provider)
            || original.repository_id() != reference.id
            || reference.node_id.as_deref() != Some(repository_node)
            || source.original().scope().host != self.identity.host
            || crate::storage::Scope::github(&self.identity.host, self.identity.account_id)
                .ok()
                .as_ref()
                != Some(source.original().scope())
            || self.workspace.repository()? != original.repository()
            || run.id != original.run().id
            || run.workflow_id != original.run().workflow_id
            || run.attempt != original.run().attempt
            || run.head_sha != original.run().head_sha
            || run.event != original.run().event
            || run.status != original.run().status
            || run.conclusion != original.run().conclusion
        {
            return Err(plain_error(
                "Original source and private Cancel run/attempt/head/target differ",
            ));
        }
        let mut descriptor = self
            .publication_descriptor(provider)?
            .ok_or_else(|| plain_error("Original Cancel has no remote descriptor"))?;
        descriptor.intent.context.original_repository_v3 =
            Some(ActionsOriginalRepositoryContextV3 {
                repository_id: reference.id,
                repository_node: repository_node.into(),
                original_name: reference.last_name.clone(),
            });
        descriptor.intent.context.original_run_v3 = Some(ActionsOriginalRunContextV3 {
            run_node: node.into(),
            workflow_id: run.workflow_id,
            check_suite_id: original.suite_v3(),
        });
        descriptor.intent.context.original_pr_v3 = source
            .pr_origin_v2()
            .map(|origin| origin.durable_pr_context_v3());
        descriptor.target = ActionsWriteTarget::from_original_context_v3(
            source.original().scope(),
            descriptor.target.target(),
            &descriptor.intent.context,
        )
        .map_err(|_| plain_error("Exact original/current/numeric Actions aliases are invalid"))?;
        descriptor
            .intent
            .fingerprint(&descriptor.target)
            .map_err(|_| plain_error("Original Cancel descriptor is inconsistent"))?;
        Ok(descriptor)
    }
    /// Pure: no subprocess, publication, persistence, or secret-value diagnostics.
    pub fn publication_descriptor(
        &self,
        provider: &GithubProvider,
    ) -> GithubResult<Option<ActionsPublicationDescriptor>> {
        if !self.identity.same_account(&self.workspace.identity)
            || !self.identity.host.eq_ignore_ascii_case(provider.host())
        {
            return Err(plain_error("Prepared Actions principal was changed"));
        }
        let local_command = match &self.operation {
            ActionsOperation::Filter(_) => Some(ActionsCommand::FilterRuns),
            ActionsOperation::CacheFilter { .. } => Some(ActionsCommand::FilterCaches),
            ActionsOperation::Scope(_) => Some(ActionsCommand::ChooseScope),
            ActionsOperation::Revision { .. } => Some(ActionsCommand::WorkflowRevision),
            ActionsOperation::LogSearch { .. } => Some(ActionsCommand::SearchLogs),
            ActionsOperation::LogExport { .. } => Some(ActionsCommand::ExportLogs),
            ActionsOperation::Archive { extract, .. } => Some(if *extract {
                ActionsCommand::ExtractArtifact
            } else {
                ActionsCommand::ExportArtifact
            }),
            _ => None,
        };
        if let Some(command) = local_command {
            if command != self.command || self.target.is_some() {
                return Err(plain_error("Prepared local Actions operation was changed"));
            }
            return Ok(None);
        }
        let repository = self.workspace.repository()?.to_owned();
        let scope = self.workspace.scope.clone();
        let prepared = match &self.operation {
            ActionsOperation::Run { run, action } => {
                let (command, kind, debug, job_id) = match action {
                    RunMutation::RerunAll { debug } => (
                        ActionsCommand::RerunAll,
                        ActionsWriteKind::RerunAll,
                        *debug,
                        None,
                    ),
                    RunMutation::RerunFailed { debug } => (
                        ActionsCommand::RerunFailed,
                        ActionsWriteKind::RerunFailed,
                        *debug,
                        None,
                    ),
                    RunMutation::RerunJob { job_id, debug } => (
                        ActionsCommand::RerunJob,
                        ActionsWriteKind::RerunJob,
                        *debug,
                        Some(*job_id),
                    ),
                    RunMutation::Cancel => (
                        ActionsCommand::Cancel,
                        ActionsWriteKind::Cancel,
                        false,
                        None,
                    ),
                    RunMutation::ForceCancel => (
                        ActionsCommand::ForceCancel,
                        ActionsWriteKind::ForceCancel,
                        false,
                        None,
                    ),
                    RunMutation::DeleteRun => (
                        ActionsCommand::DeleteRun,
                        ActionsWriteKind::DeleteRun,
                        false,
                        None,
                    ),
                    RunMutation::DeleteLogs => (
                        ActionsCommand::DeleteLogs,
                        ActionsWriteKind::DeleteLogs,
                        false,
                        None,
                    ),
                    RunMutation::ApproveFork => (
                        ActionsCommand::ApproveFork,
                        ActionsWriteKind::ApproveFork,
                        false,
                        None,
                    ),
                };
                let mut descriptor = PreparedDescriptor::new(
                    command,
                    kind,
                    ActionsTarget::Run {
                        repository: repository.clone(),
                        id: run.id,
                    },
                )
                .bind(&(run, action))?;
                descriptor.context = run_context(run);
                descriptor.context.job_id = job_id;
                descriptor.options.debug_logging = debug;
                descriptor
            }
            ActionsOperation::WorkflowState { workflow, enabled } => {
                let mut descriptor = PreparedDescriptor::new(
                    if *enabled {
                        ActionsCommand::EnableWorkflow
                    } else {
                        ActionsCommand::DisableWorkflow
                    },
                    if *enabled {
                        ActionsWriteKind::EnableWorkflow
                    } else {
                        ActionsWriteKind::DisableWorkflow
                    },
                    ActionsTarget::Workflow {
                        repository: repository.clone(),
                        id: workflow.id,
                    },
                )
                .bind(&(workflow, enabled))?;
                descriptor.options.enabled = Some(*enabled);
                descriptor
            }
            ActionsOperation::Dispatch { definition, inputs } => {
                let mut descriptor = PreparedDescriptor::new(
                    ActionsCommand::DispatchWorkflow,
                    ActionsWriteKind::DispatchWorkflow,
                    ActionsTarget::Workflow {
                        repository: repository.clone(),
                        id: definition.workflow.id,
                    },
                )
                .bind(&(definition, inputs))?;
                descriptor.context.workflow_commit = Some(definition.commit_sha.clone());
                descriptor
            }
            ActionsOperation::DeleteArtifact(artifact) => PreparedDescriptor::new(
                ActionsCommand::DeleteArtifact,
                ActionsWriteKind::DeleteArtifact,
                ActionsTarget::Artifact {
                    repository: repository.clone(),
                    id: artifact.id,
                },
            )
            .bind(artifact)?,
            ActionsOperation::Deployment { run, review } => {
                let approved = review.decision == DeploymentDecision::Approved;
                let mut descriptor = PreparedDescriptor::new(
                    if approved {
                        ActionsCommand::ApproveDeployment
                    } else {
                        ActionsCommand::RejectDeployment
                    },
                    if approved {
                        ActionsWriteKind::ApproveDeployment
                    } else {
                        ActionsWriteKind::RejectDeployment
                    },
                    ActionsTarget::DeploymentReview {
                        repository: repository.clone(),
                        run_id: run.id,
                        attempt: run.attempt,
                        head_sha: run.head_sha.clone(),
                        environment_ids: review.environment_ids.clone(),
                    },
                )
                .bind(&(run, review))?;
                descriptor.context = run_context(run);
                descriptor.options.selected_ids = review.environment_ids.clone();
                descriptor
            }
            ActionsOperation::DeleteCache(cache) => PreparedDescriptor::new(
                ActionsCommand::DeleteCache,
                ActionsWriteKind::DeleteCache,
                ActionsTarget::Cache {
                    repository: repository.clone(),
                    id: cache.id,
                },
            )
            .bind(cache)?,
            ActionsOperation::SetSecret { request, value } => PreparedDescriptor::new(
                ActionsCommand::SetSecret,
                ActionsWriteKind::SetSecret,
                ActionsTarget::Secret {
                    scope: scope.clone(),
                    name: request.name.clone(),
                },
            )
            .bind(&(request, &value.0))?,
            ActionsOperation::DeleteSecret(secret) => PreparedDescriptor::new(
                ActionsCommand::DeleteSecret,
                ActionsWriteKind::DeleteSecret,
                ActionsTarget::Secret {
                    scope: scope.clone(),
                    name: secret.name.clone(),
                },
            )
            .bind(secret)?,
            ActionsOperation::SetVariable { observed, edit } => PreparedDescriptor::new(
                ActionsCommand::SetVariable,
                ActionsWriteKind::SetVariable,
                ActionsTarget::Variable {
                    scope: scope.clone(),
                    name: edit.name.clone(),
                },
            )
            .bind(&(observed, edit))?,
            ActionsOperation::DeleteVariable(variable) => PreparedDescriptor::new(
                ActionsCommand::DeleteVariable,
                ActionsWriteKind::DeleteVariable,
                ActionsTarget::Variable {
                    scope: scope.clone(),
                    name: variable.name.clone(),
                },
            )
            .bind(variable)?,
            ActionsOperation::Policy { observed, edit } => PreparedDescriptor::new(
                ActionsCommand::EditPolicy,
                ActionsWriteKind::EditPolicy,
                ActionsTarget::Policy {
                    scope: observed.scope.clone(),
                    section: observed.section,
                },
            )
            .bind(&(observed, edit))?,
            ActionsOperation::Environment {
                name,
                observed,
                edit,
            } => PreparedDescriptor::new(
                ActionsCommand::EditEnvironment,
                ActionsWriteKind::EditEnvironment,
                ActionsTarget::EnvironmentProtection {
                    repository: repository.clone(),
                    name: name.clone(),
                    environment_id: observed["id"].as_u64().filter(|id| *id > 0).ok_or_else(
                        || plain_error("Environment publication lacks its original positive ID"),
                    )?,
                },
            )
            .bind(&(observed, edit))?,
            ActionsOperation::RunnerLabels { runner, labels } => PreparedDescriptor::new(
                ActionsCommand::RunnerLabels,
                ActionsWriteKind::RunnerLabels,
                ActionsTarget::Runner {
                    scope: scope.clone(),
                    id: runner.id,
                },
            )
            .bind(&(runner, labels))?,
            ActionsOperation::RemoveRunner(runner) => PreparedDescriptor::new(
                ActionsCommand::RemoveRunner,
                ActionsWriteKind::RemoveRunner,
                ActionsTarget::Runner {
                    scope: scope.clone(),
                    id: runner.id,
                },
            )
            .bind(runner)?,
            ActionsOperation::EditGroup {
                org,
                observed,
                edit,
            } => PreparedDescriptor::new(
                ActionsCommand::EditRunnerGroup,
                ActionsWriteKind::EditRunnerGroup,
                ActionsTarget::RunnerGroup {
                    organization: org.clone(),
                    id: observed.id,
                },
            )
            .bind(&(observed, edit))?,
            ActionsOperation::GroupAssociations { org, group, edit } => {
                let runners = edit.kind == GroupAssociation::Runners;
                let mut descriptor = PreparedDescriptor::new(
                    if runners {
                        ActionsCommand::GroupRunners
                    } else {
                        ActionsCommand::GroupRepositories
                    },
                    if runners {
                        ActionsWriteKind::GroupRunners
                    } else {
                        ActionsWriteKind::GroupRepositories
                    },
                    ActionsTarget::GroupAssociations {
                        organization: org.clone(),
                        id: group.id,
                        kind: edit.kind.clone(),
                    },
                )
                .bind(&(group, edit))?;
                descriptor.options.selected_ids = edit.selected_ids.clone();
                descriptor
            }
            ActionsOperation::CreateGroup { org, request } => PreparedDescriptor::new(
                ActionsCommand::CreateRunnerGroup,
                ActionsWriteKind::CreateRunnerGroup,
                ActionsTarget::RunnerGroupCreation {
                    organization: org.clone(),
                    name: request.settings.name.clone(),
                },
            )
            .bind(request)?,
            ActionsOperation::DeleteGroup { org, observed } => PreparedDescriptor::new(
                ActionsCommand::DeleteRunnerGroup,
                ActionsWriteKind::DeleteRunnerGroup,
                ActionsTarget::RunnerGroup {
                    organization: org.clone(),
                    id: observed.id,
                },
            )
            .bind(observed)?,
            ActionsOperation::Filter(_)
            | ActionsOperation::CacheFilter { .. }
            | ActionsOperation::Scope(_)
            | ActionsOperation::Revision { .. }
            | ActionsOperation::LogSearch { .. }
            | ActionsOperation::LogExport { .. }
            | ActionsOperation::Archive { .. } => {
                return Err(plain_error(
                    "Local operation unexpectedly reached remote publication preparation",
                ));
            }
        };
        if self.command != prepared.command
            || self
                .target
                .as_ref()
                .map(serde_json::to_value)
                .transpose()
                .map_err(|_| plain_error("Invalid public Actions target"))?
                != Some(
                    serde_json::to_value(&prepared.target)
                        .map_err(|_| plain_error("Invalid private Actions target"))?,
                )
        {
            return Err(plain_error(
                "Prepared Actions command or exact affected target was changed",
            ));
        }
        let target = ActionsWriteTarget::new(provider, &self.workspace.identity, &prepared.target)
            .map_err(|_| {
                plain_error("Cannot bind publication to a verified principal/canonical resource")
            })?;
        let intent = ActionsWriteIntent::new(
            prepared.kind,
            prepared.context,
            prepared.options,
            &prepared.payload,
        )
        .map_err(|_| plain_error("Cannot bind the closed Actions publication intent"))?;
        intent.fingerprint(&target).map_err(|_| {
            plain_error("Publication context is inconsistent with its exact target")
        })?;
        Ok(Some(ActionsPublicationDescriptor { target, intent }))
    }
}

/// Closed outcomes for durable state. Neither diagnostics nor message text is a receipt.
#[derive(Clone, Debug)]
pub enum ActionsDisposition {
    LocalOnly,
    VerifiedEffect {
        receipt: ActionsReceipt,
    },
    Accepted {
        receipt: ActionsReceipt,
        observation: ActionsAcceptedObservation,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunTransitionObservation {
    OriginalStateStillPresent,
    CompatibleTransitionObserved,
    DifferentTransitionObserved,
}
/// These are transient presentation facts, not correlated completion claims.
#[derive(Clone, Debug)]
pub enum ActionsAcceptedObservation {
    WorkflowState {
        workflow_id: u64,
        desired_enabled: bool,
        observed_state: String,
    },
    RunDeletionAcceptance {
        run_id: u64,
        requested_attempt: u64,
        head_sha: String,
        availability: ScopedDeletedRunAvailabilityV5,
    },
    Run {
        run_id: u64,
        requested_attempt: u64,
        observed_attempt: u64,
        head_sha: String,
        status: String,
        conclusion: String,
        transition: RunTransitionObservation,
    },
    Dispatch {
        workflow_id: u64,
        workflow_commit: String,
        observed_run_id: Option<u64>,
    },
    Deployment {
        run_id: u64,
        attempt: u64,
        head_sha: String,
        environment_ids: Vec<u64>,
        still_pending_environment_ids: Vec<u64>,
    },
}

fn incompatible_result(message: &'static str) -> GithubError {
    GithubError {
        kind: GithubErrorKind::AmbiguousWrite,
        message: message.into(),
    }
}
impl ActionsPlan {
    pub(super) fn run_disposition(
        &self,
        outcome: &RunMutationOutcome,
    ) -> GithubResult<ActionsDisposition> {
        let ActionsOperation::Run { run, action } = &self.operation else {
            return Err(incompatible_result(
                "Run receipt does not belong to this private prepared operation",
            ));
        };
        if outcome.action != *action || !outcome.accepted {
            return Err(incompatible_result(
                "Run publication lacks the requested typed acceptance",
            ));
        }
        let kind = match action {
            RunMutation::DeleteRun => ActionsReceiptKind::RunDeleted,
            RunMutation::DeleteLogs => ActionsReceiptKind::LogsDeleted,
            _ => ActionsReceiptKind::RunTransition,
        };
        if *action == RunMutation::DeleteRun {
            if outcome.run.is_some() || !outcome.completed {
                return Err(incompatible_result(
                    "Deleted run has no confirmed exact-catalogue absence",
                ));
            }
            return Ok(ActionsDisposition::VerifiedEffect {
                receipt: ActionsReceipt {
                    kind,
                    resource_id: Some(run.id),
                    observed_attempt: None,
                    observed_run_id: None,
                    original_dispatch_run_v7: None,
                    workflow_remote_v9: None,
                },
            });
        }
        let observed = outcome.run.as_ref().ok_or_else(|| {
            incompatible_result("Accepted run effect lacks its current exact run observation")
        })?;
        let rerun = matches!(
            action,
            RunMutation::RerunAll { .. }
                | RunMutation::RerunFailed { .. }
                | RunMutation::RerunJob { .. }
        );
        if observed.id != run.id
            || observed.workflow_id != run.workflow_id
            || observed.head_sha != run.head_sha
            || observed.attempt < run.attempt
            || !rerun && observed.attempt != run.attempt
        {
            return Err(incompatible_result(
                "Post-publication run identity or original attempt changed unexpectedly",
            ));
        }
        if !matches!(
            observed.status.as_str(),
            "queued"
                | "requested"
                | "pending"
                | "waiting"
                | "in_progress"
                | "completed"
                | "action_required"
        ) || observed.status == "completed" && observed.conclusion.is_empty()
        {
            return Err(incompatible_result(
                "Post-publication run state is absent or not a recognized observable state",
            ));
        }
        let receipt = ActionsReceipt {
            kind,
            resource_id: Some(run.id),
            observed_attempt: Some(observed.attempt),
            observed_run_id: None,
            original_dispatch_run_v7: None,
            workflow_remote_v9: None,
        };
        let cancelling = matches!(action, RunMutation::Cancel | RunMutation::ForceCancel);
        if cancelling && observed.status == "completed" && observed.conclusion == "cancelled" {
            return Ok(ActionsDisposition::VerifiedEffect { receipt });
        }
        let transition = if rerun && observed.attempt > run.attempt
            || *action == RunMutation::ApproveFork
                && observed.status != "action_required"
                && observed.conclusion != "action_required"
        {
            RunTransitionObservation::CompatibleTransitionObserved
        } else if observed.status == run.status
            && observed.conclusion == run.conclusion
            && observed.attempt == run.attempt
        {
            RunTransitionObservation::OriginalStateStillPresent
        } else {
            // A run finishing successfully/failing while cancellation was in flight
            // is not a verified cancellation, even if provider.completed was true.
            RunTransitionObservation::DifferentTransitionObserved
        };
        Ok(ActionsDisposition::Accepted {
            receipt,
            observation: ActionsAcceptedObservation::Run {
                run_id: run.id,
                requested_attempt: run.attempt,
                observed_attempt: observed.attempt,
                head_sha: run.head_sha.clone(),
                status: observed.status.clone(),
                conclusion: observed.conclusion.clone(),
                transition,
            },
        })
    }
    pub(super) fn dispatch_disposition(
        &self,
        outcome: &DispatchOutcome,
    ) -> GithubResult<ActionsDisposition> {
        let ActionsOperation::Dispatch { definition, .. } = &self.operation else {
            return Err(incompatible_result(
                "Dispatch receipt does not belong to this private prepared operation",
            ));
        };
        if !outcome.accepted
            || outcome.run.as_ref().is_some_and(|run| {
                run.id == 0
                    || run.attempt == 0
                    || run.workflow_id != definition.workflow.id
                    || run.head_sha != definition.commit_sha
                    || run.event != "workflow_dispatch"
            })
        {
            return Err(incompatible_result(
                "Dispatch acceptance/run identity does not match the exact workflow commit/event",
            ));
        }
        Ok(ActionsDisposition::Accepted {
            receipt: ActionsReceipt {
                kind: ActionsReceiptKind::Dispatch,
                resource_id: Some(definition.workflow.id),
                observed_attempt: outcome.run.as_ref().map(|run| run.attempt),
                observed_run_id: outcome.run.as_ref().map(|run| run.id),
                original_dispatch_run_v7: None,
                workflow_remote_v9: None,
            },
            observation: ActionsAcceptedObservation::Dispatch {
                workflow_id: definition.workflow.id,
                workflow_commit: definition.commit_sha.clone(),
                observed_run_id: outcome.run.as_ref().map(|run| run.id),
            },
        })
    }
    pub(super) fn deployment_disposition(
        &self,
        outcome: &DeploymentReviewOutcome,
    ) -> GithubResult<ActionsDisposition> {
        let ActionsOperation::Deployment { run, review } = &self.operation else {
            return Err(incompatible_result(
                "Deployment receipt does not belong to this private prepared operation",
            ));
        };
        if outcome.decision != review.decision || outcome.environment_ids != review.environment_ids
        {
            return Err(incompatible_result(
                "Deployment acceptance identifies another requested decision/environment set",
            ));
        }
        let mut seen = std::collections::BTreeSet::new();
        if outcome
            .remaining
            .iter()
            .any(|gate| gate.environment_id == 0 || !seen.insert(gate.environment_id))
        {
            return Err(incompatible_result(
                "Deployment observation has missing or duplicate canonical gate identities",
            ));
        }
        let still_pending_environment_ids = review
            .environment_ids
            .iter()
            .copied()
            .filter(|id| seen.contains(id))
            .collect::<Vec<_>>();
        if outcome.completed != still_pending_environment_ids.is_empty() {
            return Err(incompatible_result(
                "Deployment completed flag contradicts its actual pending-gate observation",
            ));
        }
        // Gate absence and uncorrelated whole-run approval history cannot prove
        // that this request caused this specific decision. Persist AcceptedPending
        // and let fresh producer-owned inspection describe its later transition.
        Ok(ActionsDisposition::Accepted {
            receipt: ActionsReceipt {
                kind: ActionsReceiptKind::DeploymentAccepted,
                resource_id: Some(run.id),
                observed_attempt: Some(run.attempt),
                observed_run_id: None,
                original_dispatch_run_v7: None,
                workflow_remote_v9: None,
            },
            observation: ActionsAcceptedObservation::Deployment {
                run_id: run.id,
                attempt: run.attempt,
                head_sha: run.head_sha.clone(),
                environment_ids: review.environment_ids.clone(),
                still_pending_environment_ids,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace() -> ActionsWorkspace {
        ActionsWorkspace::new(
            HostIdentity {
                host: "github.invalid".into(),
                login: "octocat".into(),
                account_id: Some(42),
                gh_version: "fixture".into(),
            },
            vec!["example/repo".into()],
        )
    }
    fn run() -> WorkflowRun {
        WorkflowRun {
            id: 123,
            workflow_id: 7,
            name: "CI".into(),
            title: "Fixture".into(),
            head_sha: "a".repeat(40),
            head_branch: "feature".into(),
            event: "pull_request".into(),
            status: "in_progress".into(),
            conclusion: String::new(),
            attempt: 2,
            actor: Actor::default(),
            triggering_actor: Actor::default(),
            created_at: String::new(),
            updated_at: String::new(),
            url: String::new(),
        }
    }
    fn plan(
        operation: ActionsOperation,
        command: ActionsCommand,
        target: Option<ActionsTarget>,
    ) -> ActionsPlan {
        let workspace = workspace();
        ActionsPlan {
            command,
            identity: workspace.identity.clone(),
            preview: vec!["preview is not authority".into()],
            target,
            workspace,
            operation,
        }
    }
    fn run_plan(action: RunMutation) -> ActionsPlan {
        let command = match action {
            RunMutation::RerunJob { .. } => ActionsCommand::RerunJob,
            RunMutation::RerunAll { .. } => ActionsCommand::RerunAll,
            RunMutation::DeleteLogs => ActionsCommand::DeleteLogs,
            _ => ActionsCommand::Cancel,
        };
        plan(
            ActionsOperation::Run { run: run(), action },
            command,
            Some(ActionsTarget::Run {
                repository: "example/repo".into(),
                id: 123,
            }),
        )
    }
    #[test]
    fn actions_run_control_publication_uses_private_target_and_rejects_public_drift() {
        let provider = GithubProvider::new("github.invalid")
            .unwrap()
            .with_program("ghtui-fixture-must-not-execute");
        let original = run_plan(RunMutation::RerunJob {
            job_id: 501,
            debug: true,
        });
        let descriptor = original.publication_descriptor(&provider).unwrap().unwrap();
        assert_eq!(descriptor.intent.kind, ActionsWriteKind::RerunJob);
        assert_eq!(descriptor.intent.context.run_id, Some(123));
        assert_eq!(descriptor.intent.context.attempt, Some(2));
        assert_eq!(descriptor.intent.context.job_id, Some(501));
        assert!(descriptor.intent.options.debug_logging);
        assert!(descriptor
            .target
            .resource_keys()
            .iter()
            .any(|key| key.ends_with("/run/123")));
        let mut changed = original.clone();
        changed.command = ActionsCommand::DeleteRun;
        assert!(changed.publication_descriptor(&provider).is_err());
        changed = original.clone();
        changed.identity.account_id = Some(43);
        assert!(changed.publication_descriptor(&provider).is_err());
        changed = original.clone();
        changed.target = Some(ActionsTarget::Run {
            repository: "example/repo".into(),
            id: 999,
        });
        assert!(changed.publication_descriptor(&provider).is_err());
        changed = original;
        changed.preview = vec!["false display label".into()];
        assert!(changed.publication_descriptor(&provider).is_ok());
    }
    #[test]
    fn actions_run_control_local_descriptor_cannot_bypass_journal_by_changed_command() {
        let provider = GithubProvider::new("github.invalid")
            .unwrap()
            .with_program("ghtui-fixture-must-not-execute");
        let mut local = plan(
            ActionsOperation::Filter(RunFilter::default()),
            ActionsCommand::FilterRuns,
            None,
        );
        assert!(local.publication_descriptor(&provider).unwrap().is_none());
        local.command = ActionsCommand::DeleteRun;
        assert!(local.publication_descriptor(&provider).is_err());
    }
    #[test]
    fn actions_secrets_publication_fingerprint_and_debug_exclude_content_and_length() {
        let provider = GithubProvider::new("github.invalid")
            .unwrap()
            .with_program("ghtui-fixture-must-not-execute");
        let build = |value: &[u8]| {
            plan(
                ActionsOperation::SetSecret {
                    request: SecretUpdate {
                        name: "BUILD_TOKEN".into(),
                        observed: None,
                        policy: None,
                    },
                    value: SecretValue(value.to_vec()),
                },
                ActionsCommand::SetSecret,
                Some(ActionsTarget::Secret {
                    scope: workspace().scope,
                    name: "BUILD_TOKEN".into(),
                }),
            )
        };
        let first = build(b"private-secret-contents")
            .publication_descriptor(&provider)
            .unwrap()
            .unwrap();
        let second = build(b"different-length-secret")
            .publication_descriptor(&provider)
            .unwrap()
            .unwrap();
        assert_eq!(
            first.intent.fingerprint(&first.target).unwrap(),
            second.intent.fingerprint(&second.target).unwrap()
        );
        let debug = format!("{first:?}");
        assert!(!debug.contains("private-secret-contents"));
        assert!(!debug.contains("different-length-secret"));
        assert!(debug.contains("redacted memory-only"));
        assert!(binding(&"x".repeat(1024 * 1024 + 1)).is_err());
    }
    #[test]
    fn actions_run_control_typed_receipt_does_not_confuse_failure_with_cancel() {
        let plan = run_plan(RunMutation::Cancel);
        let mut observed = run();
        observed.status = "completed".into();
        observed.conclusion = "failure".into();
        let mut outcome = RunMutationOutcome {
            action: RunMutation::Cancel,
            run: Some(observed),
            accepted: true,
            completed: true,
            explanation: "Pretend completion text is not authority".into(),
        };
        assert!(matches!(
            plan.run_disposition(&outcome).unwrap(),
            ActionsDisposition::Accepted {
                observation: ActionsAcceptedObservation::Run {
                    transition: RunTransitionObservation::DifferentTransitionObserved,
                    ..
                },
                ..
            }
        ));
        outcome.run.as_mut().unwrap().conclusion = "cancelled".into();
        assert!(matches!(
            plan.run_disposition(&outcome).unwrap(),
            ActionsDisposition::VerifiedEffect { .. }
        ));
        outcome.run.as_mut().unwrap().attempt = 3;
        assert!(matches!(
            plan.run_disposition(&outcome).unwrap_err().kind,
            GithubErrorKind::AmbiguousWrite
        ));
    }
    #[test]
    fn actions_run_control_rerun_and_logs_acceptance_never_claim_workflow_completion() {
        let plan = run_plan(RunMutation::RerunAll { debug: false });
        let mut observed = run();
        observed.attempt = 3;
        observed.status = "queued".into();
        let outcome = RunMutationOutcome {
            action: RunMutation::RerunAll { debug: false },
            run: Some(observed),
            accepted: true,
            completed: true,
            explanation: "completed".into(),
        };
        assert!(matches!(
            plan.run_disposition(&outcome).unwrap(),
            ActionsDisposition::Accepted {
                observation: ActionsAcceptedObservation::Run {
                    transition: RunTransitionObservation::CompatibleTransitionObserved,
                    ..
                },
                ..
            }
        ));
        let plan = run_plan(RunMutation::DeleteLogs);
        let outcome = RunMutationOutcome {
            action: RunMutation::DeleteLogs,
            run: Some(run()),
            accepted: true,
            completed: false,
            explanation: "confirmed".into(),
        };
        assert!(matches!(
            plan.run_disposition(&outcome).unwrap(),
            ActionsDisposition::Accepted {
                receipt: ActionsReceipt {
                    kind: ActionsReceiptKind::LogsDeleted,
                    ..
                },
                ..
            }
        ));
    }
    #[test]
    fn actions_approvals_typed_receipt_preserves_original_gates_without_false_decision_confirmation(
    ) {
        let review = DeploymentReview {
            environment_ids: vec![9],
            decision: DeploymentDecision::Approved,
            comment: "transient body".into(),
        };
        let target = ActionsTarget::DeploymentReview {
            repository: "example/repo".into(),
            run_id: 123,
            attempt: 2,
            head_sha: "a".repeat(40),
            environment_ids: vec![9],
        };
        let plan = plan(
            ActionsOperation::Deployment { run: run(), review },
            ActionsCommand::ApproveDeployment,
            Some(target),
        );
        let provider = GithubProvider::new("github.invalid")
            .unwrap()
            .with_program("ghtui-fixture-must-not-execute");
        let descriptor = plan.publication_descriptor(&provider).unwrap().unwrap();
        assert_eq!(descriptor.intent.context.attempt, Some(2));
        assert_eq!(descriptor.intent.options.selected_ids, vec![9]);
        let mut outcome = DeploymentReviewOutcome {
            decision: DeploymentDecision::Approved,
            environment_ids: vec![9],
            deployments: vec![],
            remaining: vec![],
            completed: true,
        };
        assert!(
            matches!(plan.deployment_disposition(&outcome).unwrap(), ActionsDisposition::Accepted {
            observation: ActionsAcceptedObservation::Deployment { still_pending_environment_ids, .. }, ..
        } if still_pending_environment_ids.is_empty())
        );
        outcome.environment_ids = vec![10];
        assert!(plan.deployment_disposition(&outcome).is_err());
    }
    #[test]
    fn actions_workflow_controls_dispatch_receipt_binds_exact_workflow_commit_not_completion_text()
    {
        let definition = WorkflowDefinition {
            workflow: Workflow {
                id: 7,
                name: "CI".into(),
                path: ".github/workflows/ci.yml".into(),
                state: "active".into(),
            },
            revision: "feature".into(),
            commit_sha: "a".repeat(40),
            yaml: "fixture".into(),
            triggers: vec![],
            dispatch_inputs: vec![],
            jobs: vec![],
        };
        let plan = plan(
            ActionsOperation::Dispatch {
                definition,
                inputs: BTreeMap::new(),
            },
            ActionsCommand::DispatchWorkflow,
            Some(ActionsTarget::Workflow {
                repository: "example/repo".into(),
                id: 7,
            }),
        );
        let mut outcome = DispatchOutcome {
            accepted: true,
            run: None,
            explanation: "Completed text must not make a receipt".into(),
        };
        assert!(matches!(
            plan.dispatch_disposition(&outcome).unwrap(),
            ActionsDisposition::Accepted {
                observation: ActionsAcceptedObservation::Dispatch {
                    observed_run_id: None,
                    ..
                },
                ..
            }
        ));
        let mut observed = run();
        observed.event = "workflow_dispatch".into();
        outcome.run = Some(observed);
        assert!(matches!(
            plan.dispatch_disposition(&outcome).unwrap(),
            ActionsDisposition::Accepted {
                observation: ActionsAcceptedObservation::Dispatch {
                    observed_run_id: Some(123),
                    ..
                },
                ..
            }
        ));
        outcome.run.as_mut().unwrap().head_sha = "b".repeat(40);
        assert!(plan.dispatch_disposition(&outcome).is_err());
        outcome.run.as_mut().unwrap().head_sha = "a".repeat(40);
        outcome.run.as_mut().unwrap().attempt = 0;
        assert!(plan.dispatch_disposition(&outcome).is_err());
    }
    #[test]
    fn actions_approvals_missing_fork_run_state_is_not_a_compatible_transition() {
        let plan = plan(
            ActionsOperation::Run {
                run: run(),
                action: RunMutation::ApproveFork,
            },
            ActionsCommand::ApproveFork,
            Some(ActionsTarget::Run {
                repository: "example/repo".into(),
                id: 123,
            }),
        );
        let mut observed = run();
        observed.status.clear();
        observed.conclusion.clear();
        let outcome = RunMutationOutcome {
            action: RunMutation::ApproveFork,
            run: Some(observed),
            accepted: true,
            completed: true,
            explanation: "success".into(),
        };
        assert!(matches!(
            plan.run_disposition(&outcome).unwrap_err().kind,
            GithubErrorKind::AmbiguousWrite
        ));
    }
}
