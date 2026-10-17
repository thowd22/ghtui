//! Read exact affected state, present it, then explicitly acknowledge; never retry a mutation here.
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ActionsTarget {
    Run {
        repository: String,
        id: u64,
    },
    DeploymentReview {
        repository: String,
        run_id: u64,
        attempt: u64,
        head_sha: String,
        environment_ids: Vec<u64>,
    },
    Workflow {
        repository: String,
        id: u64,
    },
    Artifact {
        repository: String,
        id: u64,
    },
    Cache {
        repository: String,
        id: u64,
    },
    Policy {
        scope: ActionsScope,
        section: PolicySection,
    },
    Environment {
        repository: String,
        name: String,
    },
    EnvironmentProtection {
        repository: String,
        name: String,
        environment_id: u64,
    },
    Runner {
        scope: ActionsScope,
        id: u64,
    },
    RunnerGroup {
        organization: String,
        id: u64,
    },
    RunnerGroupCreation {
        organization: String,
        name: String,
    },
    GroupAssociations {
        organization: String,
        id: u64,
        kind: GroupAssociation,
    },
    DeploymentPatterns {
        repository: String,
        environment: String,
    },
    CustomProtection {
        repository: String,
        environment: String,
    },
    Secret {
        scope: ActionsScope,
        name: String,
    },
    Variable {
        scope: ActionsScope,
        name: String,
    },
    SettingMemberships {
        scope: ActionsScope,
        name: String,
        secret: bool,
    },
}
/// Not deserializable: acknowledgement must use a fresh provider-produced observation,
/// never an arbitrary resource key from cached/user data.
#[derive(Clone, Debug)]
pub struct ActionsObservation {
    host: String,
    identity: HostIdentity,
    producer: CommandRunner,
    target: ActionsTarget,
    state: Value,
    resources: Vec<String>,
    generations: Vec<u64>,
    original_resource_v3: Option<String>,
}
/// A later state observation is not a correlated receipt for the requested decision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeploymentTransition {
    StillPending {
        environment_ids: Vec<u64>,
    },
    GatesNoLongerPending {
        environment_ids: Vec<u64>,
        run_status: String,
    },
    OriginalAttemptAdvanced {
        requested_attempt: u64,
        current_attempt: u64,
    },
}
/// Describes what happened to the original context, not which actor/request caused it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AcceptedRunTransition {
    StillPending {
        current_attempt: u64,
        status: String,
        conclusion: String,
    },
    OriginalAttemptAdvanced {
        requested_attempt: u64,
        current_attempt: u64,
    },
    RequestedCancellationObserved {
        attempt: u64,
    },
    DifferentCompletionObserved {
        attempt: u64,
        conclusion: String,
    },
    ForkApprovalGatePassed {
        attempt: u64,
        status: String,
    },
    /// The legacy run inventory has no total/complete-coverage proof. Not deletion.
    RunNotFoundInObservedCatalogue,
    /// No run field authoritatively says that its logs were deleted.
    LogsEffectNotObservable,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AcceptedDispatchTransition {
    UncorrelatedAcceptance,
    CorrelationNotObserved {
        catalogue_complete: bool,
    },
    CorrelatedRunObserved {
        run_id: u64,
        attempt: u64,
        head_sha: String,
    },
}
impl ActionsObservation {
    pub fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub fn resource_keys(&self) -> &[String] {
        &self.resources
    }
    pub fn target(&self) -> &ActionsTarget {
        &self.target
    }
    pub fn state(&self) -> &Value {
        &self.state
    }
    fn require_current_proof(&self, expected_resource: String) -> GithubResult<()> {
        let expected_resource = self
            .original_resource_v3
            .as_ref()
            .unwrap_or(&expected_resource);
        if self.identity.account_id.is_none_or(|id| id == 0)
            || !self.identity.host.eq_ignore_ascii_case(&self.host)
            || self.resources.len() != 1
            || self.resources.first() != Some(expected_resource)
            || self.generations.len() != 1
        {
            return Err(error(GithubErrorKind::IdentityChanged,
                "Actions observation lacks a positive current principal and exact canonical resource proof"));
        }
        if self.producer.resource_generation(&self.resources[0]) != self.generations[0] {
            return Err(error(
                GithubErrorKind::StaleHead,
                "A newer publication invalidated this Actions transition observation",
            ));
        }
        Ok(())
    }
    pub fn run_transition(
        &self,
        original_run_id: u64,
        original_attempt: u64,
        original_head_sha: &str,
        action: RunMutation,
    ) -> GithubResult<AcceptedRunTransition> {
        let ActionsTarget::Run { repository, id } = &self.target else {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Run transition requires the exact provider-produced run observation",
            ));
        };
        if *id != original_run_id
            || original_run_id == 0
            || original_attempt == 0
            || original_attempt > i64::MAX as u64
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Run transition does not match the original positive run/attempt",
            ));
        }
        validate_sha(original_head_sha)?;
        self.require_current_proof(self.producer_resource(repository, *id))?;
        if self.state.is_null() {
            return Ok(AcceptedRunTransition::RunNotFoundInObservedCatalogue);
        }
        let run: WorkflowRun = serde_json::from_value(self.state.clone())
            .map_err(|failure| error(GithubErrorKind::Json, failure.to_string()))?;
        if run.id != *id || run.attempt < original_attempt || run.head_sha != original_head_sha {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Observed run does not identify the original immutable run/attempt/commit",
            ));
        }
        if run.workflow_id == 0
            || action == RunMutation::ApproveFork && run.event != "pull_request"
            || matches!(action, RunMutation::RerunJob { job_id: 0, .. })
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Observed run does not identify an eligible original workflow/event/job context",
            ));
        }
        if !matches!(
            run.status.as_str(),
            "queued"
                | "requested"
                | "pending"
                | "waiting"
                | "in_progress"
                | "completed"
                | "action_required"
        ) || run.status == "completed" && run.conclusion.is_empty()
        {
            return Err(error(
                GithubErrorKind::Json,
                "Observed run lacks a known nonblank transition state",
            ));
        }
        if action == RunMutation::DeleteLogs {
            return Ok(AcceptedRunTransition::LogsEffectNotObservable);
        }
        if run.attempt > original_attempt {
            return Ok(AcceptedRunTransition::OriginalAttemptAdvanced {
                requested_attempt: original_attempt,
                current_attempt: run.attempt,
            });
        }
        if matches!(action, RunMutation::Cancel | RunMutation::ForceCancel)
            && run.status == "completed"
        {
            return Ok(if run.conclusion == "cancelled" {
                AcceptedRunTransition::RequestedCancellationObserved {
                    attempt: run.attempt,
                }
            } else {
                AcceptedRunTransition::DifferentCompletionObserved {
                    attempt: run.attempt,
                    conclusion: run.conclusion,
                }
            });
        }
        if action == RunMutation::ApproveFork
            && run.status != "action_required"
            && run.conclusion != "action_required"
        {
            return Ok(AcceptedRunTransition::ForkApprovalGatePassed {
                attempt: run.attempt,
                status: run.status,
            });
        }
        Ok(AcceptedRunTransition::StillPending {
            current_attempt: run.attempt,
            status: run.status,
            conclusion: run.conclusion,
        })
    }
    pub fn dispatch_transition(
        &self,
        workflow_id: u64,
        workflow_commit: &str,
        observed_run_id: Option<u64>,
    ) -> GithubResult<AcceptedDispatchTransition> {
        let ActionsTarget::Workflow { repository, id } = &self.target else {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Dispatch transition requires the exact provider-produced workflow observation",
            ));
        };
        if *id != workflow_id || workflow_id == 0 || observed_run_id == Some(0) {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Dispatch transition lacks the original positive workflow/correlated run identity",
            ));
        }
        validate_sha(workflow_commit)?;
        self.require_current_proof(format!(
            "{}/{}/workflow/{id}",
            self.host.to_ascii_lowercase(),
            repository.to_ascii_lowercase()
        ))?;
        let Some(observed_run_id) = observed_run_id else {
            return Ok(AcceptedDispatchTransition::UncorrelatedAcceptance);
        };
        let catalog: RunCatalog = serde_json::from_value(self.state["recent_runs"].clone())
            .map_err(|failure| error(GithubErrorKind::Json, failure.to_string()))?;
        let Some(run) = catalog.runs.iter().find(|run| run.id == observed_run_id) else {
            return Ok(AcceptedDispatchTransition::CorrelationNotObserved {
                catalogue_complete: catalog.coverage.complete,
            });
        };
        if run.workflow_id != workflow_id
            || run.head_sha != workflow_commit
            || run.event != "workflow_dispatch"
            || run.attempt == 0
        {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Correlated dispatch run has another original workflow/commit/event/attempt",
            ));
        }
        Ok(AcceptedDispatchTransition::CorrelatedRunObserved {
            run_id: run.id,
            attempt: run.attempt,
            head_sha: run.head_sha.clone(),
        })
    }
    pub fn deployment_transition(&self) -> GithubResult<DeploymentTransition> {
        let ActionsTarget::DeploymentReview {
            repository,
            run_id,
            attempt,
            head_sha,
            environment_ids,
        } = &self.target
        else {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "This producer observation is not an exact deployment-review context",
            ));
        };
        if self.identity.account_id.is_none_or(|id| id == 0)
            || self.state["verified_principal"]["account_id"].as_u64() != self.identity.account_id
            || self.state["verified_principal"]["host"] != self.identity.host
            || self.state["verified_principal"]["login"] != self.identity.login
            || self.state["requested_attempt"].as_u64() != Some(*attempt)
            || self.state["requested_head_sha"] != *head_sha
            || self.resources != vec![self.producer_resource(repository, *run_id)]
        {
            return Err(error(GithubErrorKind::IdentityChanged, "Deployment observation lacks its verified original principal/resource/context proof"));
        }
        if self
            .resources
            .iter()
            .zip(&self.generations)
            .any(|(key, version)| self.producer.resource_generation(key) != *version)
        {
            return Err(error(
                GithubErrorKind::StaleHead,
                "A newer publication invalidated this deployment observation",
            ));
        }
        let run: WorkflowRun = serde_json::from_value(self.state["run"].clone())
            .map_err(|error| super::error(GithubErrorKind::Json, error.to_string()))?;
        let gates: Vec<PendingDeployment> =
            serde_json::from_value(self.state["pending_gates"].clone())
                .map_err(|error| super::error(GithubErrorKind::Json, error.to_string()))?;
        if run.id != *run_id
            || run.head_sha != *head_sha
            || run.attempt < *attempt
            || gates.iter().any(|gate| gate.environment_id == 0)
        {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Observed run/gates do not identify the original immutable approval context",
            ));
        }
        if run.attempt > *attempt {
            return Ok(DeploymentTransition::OriginalAttemptAdvanced {
                requested_attempt: *attempt,
                current_attempt: run.attempt,
            });
        }
        let pending = environment_ids
            .iter()
            .copied()
            .filter(|id| gates.iter().any(|gate| gate.environment_id == *id))
            .collect::<Vec<_>>();
        if pending.is_empty() {
            Ok(DeploymentTransition::GatesNoLongerPending {
                environment_ids: environment_ids.clone(),
                run_status: run.status,
            })
        } else {
            Ok(DeploymentTransition::StillPending {
                environment_ids: pending,
            })
        }
    }
    fn producer_resource(&self, repository: &str, run_id: u64) -> String {
        format!(
            "{}/{}/run/{run_id}",
            self.host.to_ascii_lowercase(),
            repository.to_ascii_lowercase()
        )
    }
}
fn observation_json<T: Serialize>(value: T) -> GithubResult<Value> {
    serde_json::to_value(value)
        .map_err(|error| super::error(GithubErrorKind::Json, error.to_string()))
}
impl GithubProvider {
    /// Persist only provider-authoritative aliases; this accessor never starts a child.
    pub fn canonical_actions_resources(&self, target: &ActionsTarget) -> GithubResult<Vec<String>> {
        canonical_actions_resource_keys(&self.host, target)
    }
    pub fn inspect_actions_target(
        &self,
        identity: &HostIdentity,
        target: &ActionsTarget,
        cancel: &CancellationToken,
    ) -> GithubResult<ActionsObservation> {
        if matches!(
            target,
            ActionsTarget::DeploymentReview { .. } | ActionsTarget::EnvironmentProtection { .. }
        ) && (!identity.host.eq_ignore_ascii_case(&self.host)
            || identity.account_id.is_none_or(|id| id == 0))
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Exact settings/approval inspection requires a verified positive account ID on this GitHub host",
            ));
        }
        let expected_resources = self.canonical_actions_resources(target)?;
        let generations = expected_resources
            .iter()
            .map(|key| self.runner.resource_generation(key))
            .collect::<Vec<_>>();
        self.actions_identity(identity, cancel)?;
        let (state, resources) = match target {
            ActionsTarget::Run { repository, id } => {
                validate_pr(repository, *id)?;
                let values = self.runs(repository, None, cancel)?;
                (
                    observation_json(values.iter().find(|run| run.id == *id))?,
                    vec![format!("{}/{repository}/run/{id}", self.host)],
                )
            }
            ActionsTarget::DeploymentReview {
                repository,
                run_id,
                attempt,
                head_sha,
                environment_ids,
            } => {
                let initial = self.run(repository, *run_id, cancel)?;
                if initial.head_sha != *head_sha {
                    return Err(error(GithubErrorKind::StaleHead,"Approval inspection returned another run commit; no different context is acknowledged"));
                }
                let gates = self.pending_deployments(repository, *run_id, cancel)?;
                let history = self.deployment_review_history(repository, *run_id, cancel)?;
                let fresh = self.run(repository, *run_id, cancel)?;
                if fresh.head_sha != initial.head_sha
                    || fresh.attempt != initial.attempt
                    || fresh.workflow_id != initial.workflow_id
                {
                    return Err(error(GithubErrorKind::StaleHead,"Run identity/attempt changed while reading approval effects; inspect again"));
                }
                self.actions_identity(identity, cancel)?;
                (
                    json!({"run":fresh,"requested_attempt":attempt,"requested_head_sha":head_sha,"selected_environment_ids":environment_ids,"pending_gates":gates,"approval_history":history,"history_scope":"Whole run ID","history_limitations":["No record ID, timestamp or attempt is provided by the API.","No selected-attempt or failed-request correlation is guessed."],"verified_principal":{"host":identity.host,"login":identity.login,"account_id":identity.account_id}}),
                    vec![format!("{}/{repository}/run/{run_id}", self.host)],
                )
            }
            ActionsTarget::Workflow { repository, id } => {
                validate_pr(repository, *id)?;
                let values = self.workflows(repository, cancel)?;
                let workflow = values.iter().find(|workflow| workflow.id == *id);
                let catalog = self.run_catalog(
                    repository,
                    &RunFilter {
                        workflow_id: workflow.map(|workflow| workflow.id),
                        ..RunFilter::default()
                    },
                    cancel,
                )?;
                (
                    json!({"workflow":workflow,"recent_runs":catalog,"run_scope":if workflow.is_some(){"selected workflow ID"}else{"repository catalog; workflow is absent"},"dispatch_note":"An accepted dispatch may be asynchronous or uncorrelated on older hosts. Inspect exact run IDs/ref/SHA/actors; no absent or similarly-named run is guessed and no dispatch is automatically repeated."}),
                    vec![format!("{}/{repository}/workflow/{id}", self.host)],
                )
            }
            ActionsTarget::Artifact { repository, id } => {
                validate_pr(repository, *id)?;
                let values = self.artifacts(repository, None, cancel)?;
                (
                    observation_json(values.iter().find(|artifact| artifact.id == *id))?,
                    vec![format!("{}/{repository}/artifact/{id}", self.host)],
                )
            }
            ActionsTarget::Cache { repository, id } => {
                validate_pr(repository, *id)?;
                let values = self.action_caches(repository, None, None, cancel)?;
                (
                    observation_json(values.iter().find(|cache| cache.id == *id))?,
                    vec![format!("{}/{repository}/cache/{id}", self.host)],
                )
            }
            ActionsTarget::Policy { scope, section } => {
                let snapshot = self.actions_policy(scope, *section, cancel)?;
                let suffix = match section {
                    PolicySection::Permissions => "permissions",
                    PolicySection::SelectedActions => "permissions/selected-actions",
                    PolicySection::WorkflowToken => "permissions/workflow",
                    PolicySection::ForkWorkflows => "permissions/fork-pr-workflows-private-repos",
                    PolicySection::ForkApproval => "permissions/fork-pr-contributor-approval",
                    PolicySection::Retention => "permissions/artifact-and-log-retention",
                };
                (
                    snapshot.value,
                    vec![format!("{}/{}/{suffix}", self.host, scope.prefix()?)],
                )
            }
            ActionsTarget::Environment { repository, name } => {
                let values = self.environments(repository, cancel)?;
                nonempty_parameter(name)?;
                (
                    observation_json(values.iter().find(|value| text(value, "name") == *name))?,
                    vec![format!(
                        "{}/repos/{repository}/environments/{}",
                        self.host,
                        url_component(name)
                    )],
                )
            }
            ActionsTarget::EnvironmentProtection {
                repository,
                name,
                environment_id,
            } => {
                let environments = self.environments(repository, cancel)?;
                let original = environments
                    .iter()
                    .filter(|environment| environment["id"].as_u64() == Some(*environment_id))
                    .collect::<Vec<_>>();
                let current = environments
                    .iter()
                    .filter(|environment| {
                        environment["name"]
                            .as_str()
                            .is_some_and(|value| value.eq_ignore_ascii_case(name))
                    })
                    .collect::<Vec<_>>();
                if original.len() > 1 || current.len() > 1 {
                    return Err(error(GithubErrorKind::Json, "Environment inventory contains ambiguous duplicate IDs or names; no target is acknowledged"));
                }
                if current
                    .first()
                    .is_some_and(|environment| environment["id"].as_u64().is_none_or(|id| id == 0))
                {
                    return Err(error(GithubErrorKind::Json, "The current environment name has no positive identity; no target is acknowledged"));
                }
                self.actions_identity(identity, cancel)?;
                (
                    json!({"requested_environment_id":environment_id,"requested_environment_name":name,"original_by_id":original.first(),"current_by_name":current.first(),"identity_note":"A reused name is not the original environment. No outcome or automatic retry is inferred.","verified_principal":{"host":identity.host,"login":identity.login,"account_id":identity.account_id}}),
                    vec![format!(
                        "{}/repos/{repository}/environments/{}",
                        self.host,
                        url_component(name)
                    )],
                )
            }
            ActionsTarget::Runner { scope, id } => {
                if *id == 0 {
                    return Err(error(GithubErrorKind::InvalidTarget, "Select a runner"));
                }
                let values = self.runners(scope, cancel)?;
                let resource = format!("{}/{}/runners/{id}", self.host, scope.prefix()?);
                (
                    observation_json(values.iter().find(|runner| runner.id == *id))?,
                    vec![resource.clone(), format!("{resource}/labels")],
                )
            }
            ActionsTarget::RunnerGroup { organization, id } => {
                if *id == 0 {
                    return Err(error(
                        GithubErrorKind::InvalidTarget,
                        "Select a runner group",
                    ));
                }
                let values = self.runner_groups(organization, cancel)?;
                (
                    observation_json(values.iter().find(|group| group.id == *id))?,
                    vec![format!(
                        "{}/orgs/{organization}/actions/runner-groups/{id}",
                        self.host
                    )],
                )
            }
            ActionsTarget::RunnerGroupCreation { organization, name } => {
                nonempty_parameter(name)?;
                let values = self.runner_groups(organization, cancel)?;
                (
                    observation_json(
                        values
                            .iter()
                            .filter(|group| group.name == *name)
                            .collect::<Vec<_>>(),
                    )?,
                    vec![format!(
                        "{}/orgs/{organization}/actions/runner-groups",
                        self.host
                    )],
                )
            }
            ActionsTarget::GroupAssociations {
                organization,
                id,
                kind,
            } => {
                let values = self.group_associations(organization, *id, kind.clone(), cancel)?;
                let key = match kind {
                    GroupAssociation::Runners => "runners",
                    GroupAssociation::Repositories => "repositories",
                };
                (
                    observation_json(values)?,
                    vec![format!(
                        "{}/orgs/{organization}/actions/runner-groups/{id}/{key}",
                        self.host
                    )],
                )
            }
            ActionsTarget::DeploymentPatterns {
                repository,
                environment,
            } => {
                let values = self.deployment_patterns(repository, environment, cancel)?;
                (
                    observation_json(values)?,
                    vec![format!(
                        "{}/repos/{repository}/environments/{}/deployment-branch-policies",
                        self.host,
                        url_component(environment)
                    )],
                )
            }
            ActionsTarget::CustomProtection {
                repository,
                environment,
            } => {
                let values = self.custom_protection_rules(repository, environment, cancel)?;
                (
                    observation_json(values)?,
                    vec![format!(
                        "{}/repos/{repository}/environments/{}/deployment_protection_rules",
                        self.host,
                        url_component(environment)
                    )],
                )
            }
            ActionsTarget::Secret { scope, name } => {
                super::settings::setting_name(name)?;
                let values = self.secret_metadata(scope, cancel)?;
                (
                    observation_json(
                        values
                            .iter()
                            .find(|secret| secret.name.eq_ignore_ascii_case(name)),
                    )?,
                    vec![format!("{}/{}/secret/{name}", self.host, scope.prefix()?)],
                )
            }
            ActionsTarget::Variable { scope, name } => {
                super::settings::setting_name(name)?;
                let values = self.variables(scope, cancel)?;
                (
                    observation_json(
                        values
                            .iter()
                            .find(|variable| variable.name.eq_ignore_ascii_case(name)),
                    )?,
                    vec![format!("{}/{}/variable/{name}", self.host, scope.prefix()?)],
                )
            }
            ActionsTarget::SettingMemberships {
                scope,
                name,
                secret,
            } => {
                let values = self.selected_setting_repositories(scope, name, *secret, cancel)?;
                (
                    observation_json(values)?,
                    vec![format!(
                        "{}/{}/{}/{name}/repositories",
                        self.host,
                        scope.prefix()?,
                        if *secret { "secrets" } else { "variables" }
                    )],
                )
            }
        };
        if !matches!(
            target,
            ActionsTarget::DeploymentReview { .. } | ActionsTarget::EnvironmentProtection { .. }
        ) {
            self.actions_identity(identity, cancel)?;
        }
        let resources = resources
            .into_iter()
            .map(|key| self.canonical_resource(&key))
            .collect::<Vec<_>>();
        if resources != expected_resources
            || resources
                .iter()
                .zip(&generations)
                .any(|(key, version)| self.runner.resource_generation(key) != *version)
        {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Affected Actions state changed during inspection; refresh it before acknowledging",
            ));
        }
        Ok(ActionsObservation {
            host: self.host.clone(),
            identity: identity.clone(),
            producer: self.runner.clone(),
            target: target.clone(),
            state,
            resources,
            generations,
            original_resource_v3: None,
        })
    }
    /// No HTTP or DTO promotion: this borrows a genuine bounded raw-run factory
    /// outcome from this SAME runner. The caller still must close original
    /// repository/SQL/UID proof before displaying or consuming the observation.
    pub(crate) fn scoped_run_observation_v3(
        &self,
        identity: &HostIdentity,
        run: &std::sync::Arc<ScopedActionsRunTargetV1>,
        cancel: &CancellationToken,
    ) -> GithubResult<ActionsObservation> {
        if cancel.is_cancelled()
            || !run.original_writer_v3(self)
            || identity.account_id.is_none_or(|id| id == 0)
            || identity.host != self.host
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Original observation principal unavailable",
            ));
        }
        self.scoped_run_observation_model_v8(
            identity,
            run.repository_id(),
            run.repository(),
            run.run(),
        )
    }
    pub(crate) fn scoped_historical_run_observation_v8(
        &self,
        identity: &HostIdentity,
        read: &ScopedHistoricalRunReadV5,
        key: &crate::actions_write_journal::ActionsHistoricalRunKeyV5,
        current: &CurrentCatalogueRepository,
        cancel: &CancellationToken,
    ) -> GithubResult<ActionsObservation> {
        if identity.account_id.is_none_or(|id| id == 0) || identity.host != self.host {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Original historical principal unavailable",
            ));
        }
        if current.repository().id != key.repository_id()
            || current.repository().node_id.as_deref() != Some(key.repository_node())
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Original historical repository changed",
            ));
        }
        let run = read.observed_for_v8(self, key, cancel)?;
        let observation = self.scoped_run_observation_model_v8(
            identity,
            key.repository_id(),
            &current.repository().full_name,
            run,
        )?;
        if !read.matches(self, key, cancel) {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Original historical observation generation changed",
            ));
        }
        Ok(observation)
    }
    fn scoped_run_observation_model_v8(
        &self,
        identity: &HostIdentity,
        repository_id: u64,
        repository: &str,
        run: &WorkflowRun,
    ) -> GithubResult<ActionsObservation> {
        let key = format!(
            "{}/repository-id/{}/actions/run/{}",
            self.host, repository_id, run.id
        );
        let generation = self.runner.resource_generation(&key);
        let state = serde_json::to_value(run).map_err(|_| {
            error(
                GithubErrorKind::Json,
                "Original run observation unavailable",
            )
        })?;
        Ok(ActionsObservation {
            host: self.host.clone(),
            identity: identity.clone(),
            producer: self.runner.clone(),
            target: ActionsTarget::Run {
                repository: repository.into(),
                id: run.id,
            },
            state,
            resources: vec![key.clone()],
            generations: vec![generation],
            original_resource_v3: Some(key),
        })
    }
    /// Call only after the native UI presented this state and the user acknowledged the effects.
    pub fn acknowledge_actions_observation(
        &self,
        observation: ActionsObservation,
    ) -> GithubResult<()> {
        if observation.host != self.host || !self.runner.same_instance(&observation.producer) {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "The observation belongs to another GitHub host or provider execution state",
            ));
        }
        let versions = observation
            .resources
            .into_iter()
            .zip(observation.generations)
            .collect::<Vec<_>>();
        if !self.runner.mark_reconciled_generations(&versions) {
            return Err(error(GithubErrorKind::StaleHead,"This observation predates a newer write; inspect its effects again before acknowledging"));
        }
        Ok(())
    }
}
