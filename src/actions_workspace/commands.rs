//! Typed Actions forms and effect previews. No arbitrary gh/API/JSON command execution.
use super::*;
use std::{collections::BTreeMap, path::PathBuf};
mod native_dispatch_v7;
mod publication;
pub use publication::*;
mod publication_execution;
pub use publication_execution::{ActionsPublication, ActionsRefresh};

#[derive(Clone, Debug)]
pub struct ActionsField {
    pub key: String,
    pub label: String,
    pub value: String,
    pub multiline: bool,
    pub secret: bool,
    pub choices: Vec<String>,
}
fn field(key: &str, label: &str, value: impl Into<String>) -> ActionsField {
    ActionsField {
        key: key.into(),
        label: label.into(),
        value: value.into(),
        multiline: false,
        secret: false,
        choices: vec![],
    }
}
fn choice(key: &str, label: &str, value: impl Into<String>, options: &[&str]) -> ActionsField {
    ActionsField {
        choices: options.iter().map(|value| (*value).into()).collect(),
        ..field(key, label, value)
    }
}
fn selected<'a, T>(values: &'a [T], workspace: &ActionsWorkspace) -> Option<&'a T> {
    values.get(workspace.selected)
}
fn string(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().into()
}
fn boolean(value: &Value, key: &str) -> String {
    value[key].as_bool().unwrap_or(false).to_string()
}
impl ActionsCommand {
    pub fn key(self) -> String {
        format!("{self:?}").to_ascii_lowercase()
    }
}
impl ActionsWorkspace {
    /// ONE schema for existing legacy Forms and closed original-run Forms.
    /// This is presentation, never target/permission or confirmation authority.
    pub(crate) fn original_run_fields_v4(command: ActionsCommand) -> Option<Vec<ActionsField>> {
        use ActionsCommand::*;
        match command {
            Cancel | DeleteRun | DeleteLogs => Some(vec![]),
            ForceCancel => Some(vec![choice(
                "force_ack",
                "Escalate force cancellation (bypasses always conditions)?",
                "false",
                &["false", "true"],
            )]),
            RerunAll | RerunFailed | RerunJob => Some(vec![choice(
                "debug",
                "Enable debug logging?",
                "false",
                &["false", "true"],
            )]),
            _ => None,
        }
    }
    /// Draft identity uses typed IDs and immutable comparison context, never display rows.
    pub fn resource_key(&self, command: ActionsCommand) -> GithubResult<String> {
        use ActionsCommand::*;
        if command == ChooseScope && self.repositories.is_empty() {
            return Ok(format!(
                "{}/settings-scope/{:?}",
                self.identity.host, self.scope
            ));
        }
        let repo = self.repository()?;
        let key = match command {
            RerunAll | RerunFailed | RerunJob | Cancel | ForceCancel | DeleteRun | DeleteLogs
            | ApproveFork | SearchLogs | ExportLogs => {
                let page = self
                    .run
                    .as_ref()
                    .ok_or_else(|| plain_error("Inspect an exact run attempt first"))?;
                format!(
                    "run/{}/attempt/{}/commit/{}/job/{}",
                    page.attempt.run.id,
                    page.attempt.run.attempt,
                    page.attempt.run.head_sha,
                    page.selected_job
                        .map(|id| id.to_string())
                        .unwrap_or("all".into())
                )
            }
            ApproveDeployment | RejectDeployment => {
                let run = self
                    .run
                    .as_ref()
                    .ok_or_else(|| plain_error("Inspect an exact run first"))?;
                let gate = self
                    .selected_deployment()
                    .ok_or_else(|| plain_error("Select an exact pending environment"))?;
                format!(
                    "run/{}/attempt/{}/commit/{}/environment/{}",
                    run.attempt.run.id,
                    run.attempt.run.attempt,
                    run.attempt.run.head_sha,
                    gate.environment_id
                )
            }
            WorkflowRevision | EnableWorkflow | DisableWorkflow | DispatchWorkflow => {
                let definition = self
                    .definition
                    .as_ref()
                    .ok_or_else(|| plain_error("Inspect a workflow first"))?;
                format!(
                    "workflow/{}/commit/{}/ref/{}",
                    definition.workflow.id, definition.commit_sha, definition.revision
                )
            }
            ExportArtifact | ExtractArtifact | DeleteArtifact => {
                let artifact = self
                    .artifact
                    .as_ref()
                    .ok_or_else(|| plain_error("Inspect an artifact first"))?;
                format!(
                    "artifact/{}/run/{}/commit/{}",
                    artifact.id, artifact.run_id, artifact.head_sha
                )
            }
            DeleteCache => {
                let cache = selected(&self.caches, self)
                    .ok_or_else(|| plain_error("Select a cache first"))?;
                format!("cache/{}/version/{}", cache.id, cache.version)
            }
            SetSecret | DeleteSecret => format!(
                "scope/{:?}/secret/{}",
                self.scope,
                self.secrets
                    .get(self.selected.saturating_sub(1))
                    .map(|secret| secret.name.as_str())
                    .unwrap_or("new")
            ),
            SetVariable | DeleteVariable => format!(
                "scope/{:?}/variable/{}",
                self.scope,
                selected(&self.variables, self)
                    .map(|variable| variable.name.as_str())
                    .unwrap_or("new")
            ),
            EditPolicy => format!("scope/{:?}/policy/{:?}", self.scope, self.policy_section),
            EditEnvironment => {
                let environment = self
                    .selected_environment()
                    .ok_or_else(|| plain_error("Select an environment first"))?;
                let id = environment["id"]
                    .as_u64()
                    .filter(|id| *id > 0)
                    .ok_or_else(|| plain_error("Select a positive canonical environment ID"))?;
                format!("environment/{}/id/{id}", string(environment, "name"))
            }
            RunnerLabels | RemoveRunner => format!(
                "scope/{:?}/runner/{}",
                self.scope,
                selected(&self.runners, self)
                    .ok_or_else(|| plain_error("Select a runner first"))?
                    .id
            ),
            EditRunnerGroup | GroupRunners | GroupRepositories | DeleteRunnerGroup => format!(
                "scope/{:?}/runner-group/{}",
                self.scope,
                selected(&self.runner_groups, self)
                    .ok_or_else(|| plain_error("Select a runner group first"))?
                    .id
            ),
            CreateRunnerGroup => format!("scope/{:?}/runner-group/new", self.scope),
            FilterRuns => "run-catalog/filter".into(),
            FilterCaches => "cache-catalog/filter".into(),
            ChooseScope => format!("scope/{:?}", self.scope),
        };
        Ok(format!("{}/{repo}/{key}", self.identity.host))
    }
    pub fn fields(&self, command: ActionsCommand) -> GithubResult<Vec<ActionsField>> {
        if let Some(reason) = self.disabled_reason(command) {
            return Err(plain_error(reason));
        }
        use ActionsCommand::*;
        let org = matches!(self.scope, ActionsScope::Organization { .. });
        let target_id = |id: Option<u64>| {
            field(
                "id",
                "Exact selected ID",
                id.map(|id| id.to_string()).unwrap_or_default(),
            )
        };
        let mut fields = match command {
            ChooseScope => vec![
                choice(
                    "kind",
                    "Settings scope",
                    "repository",
                    &["repository", "environment", "organization"],
                ),
                field("name", "Environment or organization name", ""),
            ],
            WorkflowRevision => vec![field(
                "revision",
                "Inspect workflow at branch, tag or commit",
                self.definition
                    .as_ref()
                    .map(|definition| definition.revision.as_str())
                    .unwrap_or("HEAD"),
            )],
            SearchLogs => vec![
                field(
                    "query",
                    "Search loaded logs",
                    self.log_search
                        .as_ref()
                        .map(|(query, _)| query.as_str())
                        .unwrap_or(""),
                ),
                choice(
                    "regex",
                    "Use regular expression?",
                    "false",
                    &["false", "true"],
                ),
            ],
            FilterRuns => vec![
                field(
                    "actor",
                    "Actor login",
                    self.filter.actor.as_deref().unwrap_or(""),
                ),
                field(
                    "branch",
                    "Branch",
                    self.filter.branch.as_deref().unwrap_or(""),
                ),
                field(
                    "event",
                    "Trigger event",
                    self.filter.event.as_deref().unwrap_or(""),
                ),
                field(
                    "status",
                    "Run status or conclusion",
                    self.filter.status.as_deref().unwrap_or(""),
                ),
                field(
                    "sha",
                    "Exact commit SHA",
                    self.filter.head_sha.as_deref().unwrap_or(""),
                ),
            ],
            FilterCaches => vec![
                field(
                    "key",
                    "Cache key prefix",
                    self.cache_key.as_deref().unwrap_or(""),
                ),
                field(
                    "ref",
                    "Exact Git ref (blank for all)",
                    self.cache_ref.as_deref().unwrap_or(""),
                ),
            ],
            RerunAll | RerunFailed | RerunJob => {
                Self::original_run_fields_v4(command).expect("closed rerun schema")
            }
            EnableWorkflow | DisableWorkflow | Cancel | ForceCancel | DeleteRun | DeleteLogs
            | ApproveFork => vec![],
            DispatchWorkflow => {
                let definition = self
                    .definition
                    .as_ref()
                    .ok_or_else(|| plain_error("Inspect workflow definition first"))?;
                let mut fields = vec![field(
                    "revision",
                    "Branch or tag to run (V on the workflow page inspects another)",
                    &definition.revision,
                )];
                for input in &definition.dispatch_inputs {
                    let default = input
                        .default
                        .as_ref()
                        .map(|value| {
                            value
                                .as_str()
                                .map(str::to_owned)
                                .unwrap_or_else(|| value.to_string())
                        })
                        .unwrap_or_default();
                    let mut input_field = field(
                        &format!("input:{}", input.name),
                        &format!(
                            "{}{} — {}",
                            input.name,
                            if input.required { " (required)" } else { "" },
                            input.description
                        ),
                        default,
                    );
                    input_field.choices = if input.input_type == DispatchInputType::Boolean {
                        vec!["false".into(), "true".into()]
                    } else {
                        input.options.clone()
                    };
                    fields.push(input_field);
                }
                fields
            }
            ExportArtifact | ExtractArtifact | ExportLogs => vec![field(
                "destination",
                "New absolute destination (never overwrites)",
                "",
            )],
            DeleteArtifact => vec![],
            ApproveDeployment | RejectDeployment => vec![
                target_id(self.selected_deployment().map(|gate| gate.environment_id)),
                field("comment", "Decision explanation", ""),
            ],
            DeleteCache => vec![target_id(
                selected(&self.caches, self).map(|cache| cache.id),
            )],
            SetSecret | DeleteSecret => vec![field(
                "name",
                "Secret name",
                self.secrets
                    .get(self.selected.saturating_sub(1))
                    .map(|secret| secret.name.as_str())
                    .unwrap_or(""),
            )],
            SetVariable | DeleteVariable => {
                let observed = selected(&self.variables, self);
                vec![field(
                    "name",
                    "Variable name",
                    observed
                        .map(|variable| variable.name.as_str())
                        .unwrap_or(""),
                )]
            }
            EditPolicy => {
                let snapshot = self
                    .policy
                    .as_ref()
                    .ok_or_else(|| plain_error("Inspect policy first"))?;
                let value = &snapshot.value;
                match snapshot.section {
                    PolicySection::Permissions => vec![
                        if org {
                            choice(
                                "enabled_repositories",
                                "Enabled repositories",
                                string(value, "enabled_repositories"),
                                &["all", "private", "selected"],
                            )
                        } else {
                            choice(
                                "enabled",
                                "Enable Actions?",
                                boolean(value, "enabled"),
                                &["false", "true"],
                            )
                        },
                        choice(
                            "allowed_actions",
                            "Allowed actions",
                            string(value, "allowed_actions"),
                            &["all", "local_only", "selected"],
                        ),
                        choice(
                            "sha_pinning_required",
                            "Require full SHA pins?",
                            boolean(value, "sha_pinning_required"),
                            &["false", "true"],
                        ),
                    ],
                    PolicySection::SelectedActions => vec![
                        choice(
                            "github_owned_allowed",
                            "Allow GitHub-owned actions?",
                            boolean(value, "github_owned_allowed"),
                            &["false", "true"],
                        ),
                        choice(
                            "verified_allowed",
                            "Allow verified actions?",
                            boolean(value, "verified_allowed"),
                            &["false", "true"],
                        ),
                        field(
                            "patterns",
                            "Allowed action patterns, one per line",
                            value["patterns_allowed"]
                                .as_array()
                                .map(|values| {
                                    values
                                        .iter()
                                        .filter_map(Value::as_str)
                                        .collect::<Vec<_>>()
                                        .join("\n")
                                })
                                .unwrap_or_default(),
                        ),
                    ],
                    PolicySection::WorkflowToken => vec![
                        choice(
                            "permission",
                            "Default token permission",
                            string(value, "default_workflow_permissions"),
                            &["read", "write"],
                        ),
                        choice(
                            "approve_reviews",
                            "May workflows approve PR reviews?",
                            boolean(value, "can_approve_pull_request_reviews"),
                            &["false", "true"],
                        ),
                    ],
                    PolicySection::ForkWorkflows => [
                        "run_workflows_from_fork_pull_requests",
                        "send_write_tokens_to_workflows",
                        "send_secrets_and_variables",
                        "require_approval_for_fork_pr_workflows",
                    ]
                    .into_iter()
                    .map(|key| choice(key, key, boolean(value, key), &["false", "true"]))
                    .collect(),
                    PolicySection::ForkApproval => vec![choice(
                        "approval_policy",
                        "External contributor approval policy",
                        string(value, "approval_policy"),
                        &[
                            "first_time_contributors_new_to_github",
                            "first_time_contributors",
                            "all_external_contributors",
                        ],
                    )],
                    PolicySection::Retention => vec![field(
                        "days",
                        "Artifact/log retention days",
                        value["days"]
                            .as_u64()
                            .map(|days| days.to_string())
                            .unwrap_or_default(),
                    )],
                }
            }
            EditEnvironment => {
                let environment = self
                    .selected_environment()
                    .ok_or_else(|| plain_error("Select an environment"))?;
                let rules = environment["protection_rules"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                let wait = rules
                    .iter()
                    .find(|rule| rule["type"] == "wait_timer")
                    .and_then(|rule| rule["wait_timer"].as_u64())
                    .unwrap_or(0);
                let review = rules
                    .iter()
                    .find(|rule| rule["type"] == "required_reviewers");
                let reviewers = review
                    .and_then(|rule| rule["reviewers"].as_array())
                    .map(|reviewers| {
                        reviewers
                            .iter()
                            .map(|reviewer| {
                                format!(
                                    "{}:{}",
                                    reviewer["type"].as_str().unwrap_or("User"),
                                    reviewer["reviewer"]["id"]
                                )
                            })
                            .collect::<Vec<_>>()
                            .join(",")
                    })
                    .unwrap_or_default();
                let branch = match settings::environment_branch_mode(environment)? {
                    DeploymentBranchMode::All => "all",
                    DeploymentBranchMode::Protected => "protected",
                    DeploymentBranchMode::Selected => "selected",
                };
                vec![
                    field("name", "Selected environment", string(environment, "name")),
                    field("wait", "Wait timer minutes", wait.to_string()),
                    choice(
                        "prevent_self_review",
                        "Prevent self-review?",
                        review
                            .map(|rule| boolean(rule, "prevent_self_review"))
                            .unwrap_or("false".into()),
                        &["false", "true"],
                    ),
                    field(
                        "reviewers",
                        "Reviewer identities (User:123,Team:456)",
                        reviewers,
                    ),
                    choice(
                        "branches",
                        "Allowed deployment branches",
                        branch,
                        &["all", "protected", "selected"],
                    ),
                ]
            }
            RunnerLabels | RemoveRunner => {
                let runner = selected(&self.runners, self);
                let mut fields = vec![target_id(runner.map(|runner| runner.id))];
                if command == RunnerLabels {
                    fields.push(field(
                        "labels",
                        "Custom labels, comma-separated",
                        runner
                            .map(|runner| {
                                runner
                                    .labels
                                    .iter()
                                    .filter(|label| label.label_type == "custom")
                                    .map(|label| label.name.as_str())
                                    .collect::<Vec<_>>()
                                    .join(",")
                            })
                            .unwrap_or_default(),
                    ));
                }
                fields
            }
            EditRunnerGroup | CreateRunnerGroup => {
                let group = if command == CreateRunnerGroup {
                    None
                } else {
                    selected(&self.runner_groups, self)
                };
                let mut fields = vec![];
                if command == EditRunnerGroup {
                    fields.push(target_id(group.map(|group| group.id)));
                }
                fields.extend([
                    field(
                        "name",
                        "Group name",
                        group.map(|group| group.name.as_str()).unwrap_or(""),
                    ),
                    choice(
                        "visibility",
                        "Repository visibility",
                        group
                            .map(|group| group.visibility.as_str())
                            .unwrap_or("selected"),
                        &["all", "private", "selected"],
                    ),
                    choice(
                        "allow_public",
                        "Allow public repositories?",
                        group
                            .is_some_and(|group| group.allows_public_repositories)
                            .to_string(),
                        &["false", "true"],
                    ),
                    choice(
                        "restrict_workflows",
                        "Restrict to selected workflows?",
                        group
                            .is_some_and(|group| group.restricted_to_workflows)
                            .to_string(),
                        &["false", "true"],
                    ),
                    field(
                        "workflows",
                        "Selected workflow paths, one per line",
                        group
                            .map(|group| group.selected_workflows.join("\n"))
                            .unwrap_or_default(),
                    ),
                ]);
                if command == CreateRunnerGroup {
                    fields.extend([
                        field(
                            "repositories",
                            "Selected repository IDs, comma-separated",
                            "",
                        ),
                        field("runners", "Runner IDs, comma-separated", ""),
                    ]);
                }
                fields
            }
            GroupRunners | GroupRepositories => vec![
                target_id(selected(&self.runner_groups, self).map(|group| group.id)),
                field(
                    "selected_ids",
                    "New complete set of IDs, comma-separated",
                    "",
                ),
            ],
            DeleteRunnerGroup => vec![target_id(
                selected(&self.runner_groups, self).map(|group| group.id),
            )],
        };
        if command == SetSecret {
            fields.push(ActionsField {
                secret: true,
                ..field("value", "New secret value (masked, never persisted)", "")
            });
        }
        if command == SetVariable {
            fields.push(ActionsField {
                multiline: true,
                ..field(
                    "value",
                    "Variable value",
                    selected(&self.variables, self)
                        .map(|variable| variable.value.as_str())
                        .unwrap_or(""),
                )
            });
        }
        if org && matches!(command, SetSecret | SetVariable) {
            fields.extend([
                choice(
                    "visibility",
                    "Visibility",
                    "private",
                    &["all", "private", "selected"],
                ),
                field(
                    if command == SetSecret {
                        "repositories"
                    } else {
                        "repository_ids"
                    },
                    if command == SetSecret {
                        "Selected repository names in this organization (no owner/ prefix)"
                    } else {
                        "Selected repository numeric IDs"
                    },
                    "",
                ),
            ]);
        }
        for field in &mut fields {
            if matches!(field.key.as_str(), "patterns" | "workflows" | "comment") {
                field.multiline = true;
            }
        }
        Ok(fields)
    }
}

#[derive(Clone)]
struct SecretValue(Vec<u8>);
impl std::fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[REDACTED]")
    }
}
/// TASK-107: the exact attempt (and job) whose whole log Export re-reads to disk.
#[derive(Clone, Debug)]
struct FullLogSource {
    repository: String,
    run: WorkflowRun,
    job: Option<ActionsJob>,
    skipped_bytes: u64,
}
#[derive(Clone, Debug)]
enum ActionsOperation {
    Filter(RunFilter),
    CacheFilter {
        key: Option<String>,
        ref_name: Option<String>,
    },
    Scope(ActionsScope),
    Revision {
        workflow: Workflow,
        revision: String,
    },
    LogSearch {
        query: String,
        regex: bool,
    },
    /// TASK-53: the loaded, bounded log text written to a new local file.
    LogExport {
        text: String,
        context: String,
        destination: PathBuf,
        /// TASK-107: set when only the log's tail is loaded; Export then re-reads the
        /// whole log from gh straight to disk instead of writing the tail.
        full: Option<Box<FullLogSource>>,
    },
    Run {
        run: WorkflowRun,
        action: RunMutation,
    },
    WorkflowState {
        workflow: Workflow,
        enabled: bool,
    },
    Dispatch {
        definition: WorkflowDefinition,
        inputs: BTreeMap<String, Value>,
    },
    Archive {
        archive: Arc<ArtifactArchive>,
        destination: PathBuf,
        extract: bool,
    },
    DeleteArtifact(ActionsArtifact),
    Deployment {
        run: WorkflowRun,
        review: DeploymentReview,
    },
    DeleteCache(ActionCache),
    SetSecret {
        request: SecretUpdate,
        value: SecretValue,
    },
    DeleteSecret(SecretMetadata),
    SetVariable {
        observed: Option<ActionsVariable>,
        edit: VariableEdit,
    },
    DeleteVariable(ActionsVariable),
    Policy {
        observed: PolicySnapshot,
        edit: PolicyEdit,
    },
    Environment {
        name: String,
        observed: Value,
        edit: EnvironmentEdit,
    },
    RunnerLabels {
        runner: SelfHostedRunner,
        labels: Vec<String>,
    },
    RemoveRunner(SelfHostedRunner),
    EditGroup {
        org: String,
        observed: RunnerGroup,
        edit: RunnerGroupEdit,
    },
    GroupAssociations {
        org: String,
        group: RunnerGroup,
        edit: GroupAssociationEdit,
    },
    CreateGroup {
        org: String,
        request: CreateRunnerGroup,
    },
    DeleteGroup {
        org: String,
        observed: RunnerGroup,
    },
}
#[derive(Clone)]
pub struct ActionsPlan {
    pub command: ActionsCommand,
    pub identity: HostIdentity,
    pub preview: Vec<String>,
    pub target: Option<ActionsTarget>,
    workspace: ActionsWorkspace,
    operation: ActionsOperation,
}
impl std::fmt::Debug for ActionsPlan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActionsPlan")
            .field("command", &self.command)
            .field(
                "content",
                &"[REDACTED workspace/definition/inputs/operation]",
            )
            .finish()
    }
}
#[derive(Clone, Debug)]
pub struct ActionsOutcome {
    pub message: String,
    pub workspace: ActionsWorkspace,
}
fn value<'a>(values: &'a BTreeMap<String, String>, key: &str) -> &'a str {
    values.get(key).map(String::as_str).unwrap_or("")
}
fn bool_input(values: &BTreeMap<String, String>, key: &str) -> GithubResult<bool> {
    match value(values, key) {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(plain_error(format!("{key} must be true or false"))),
    }
}
fn id_input(values: &BTreeMap<String, String>, key: &str) -> GithubResult<u64> {
    value(values, key)
        .parse::<u64>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| plain_error(format!("{key} needs a positive exact ID")))
}
fn list(values: &BTreeMap<String, String>, key: &str) -> Vec<String> {
    value(values, key)
        .split([',', '\n'])
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}
fn ids(values: &BTreeMap<String, String>, key: &str) -> GithubResult<Vec<u64>> {
    let mut output = list(values, key)
        .iter()
        .map(|value| {
            value
                .parse::<u64>()
                .ok()
                .filter(|id| *id > 0)
                .ok_or_else(|| plain_error("IDs must be positive integers"))
        })
        .collect::<GithubResult<Vec<_>>>()?;
    output.sort_unstable();
    if output.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(plain_error("Duplicate target IDs are not permitted"));
    }
    Ok(output)
}
fn visibility(values: &BTreeMap<String, String>, key: &str) -> GithubResult<ScopeVisibility> {
    match value(values, key) {
        "all" => Ok(ScopeVisibility::All),
        "private" => Ok(ScopeVisibility::Private),
        "selected" => Ok(ScopeVisibility::Selected),
        _ => Err(plain_error("Choose all, private or selected visibility")),
    }
}
fn organization(scope: &ActionsScope) -> GithubResult<String> {
    match scope {
        ActionsScope::Organization { organization } => Ok(organization.clone()),
        _ => Err(plain_error("Choose an explicit organization scope first")),
    }
}
fn group_edit(values: &BTreeMap<String, String>) -> GithubResult<RunnerGroupEdit> {
    Ok(RunnerGroupEdit {
        name: value(values, "name").into(),
        visibility: visibility(values, "visibility")?,
        allows_public_repositories: bool_input(values, "allow_public")?,
        restricted_to_workflows: bool_input(values, "restrict_workflows")?,
        selected_workflows: list(values, "workflows"),
    })
}

impl ActionsPlan {
    /// Only the private operation can require this conditional CLI flag.
    /// This is local syntax checking, not current host/permission authority.
    pub(crate) fn check_cli_syntax_v1(
        &self,
        gh: &GithubProvider,
        current: &HostIdentity,
    ) -> GithubResult<()> {
        if let ActionsOperation::SetSecret { request, .. } = &self.operation {
            if matches!(self.workspace.scope, ActionsScope::Organization { .. })
                && request.policy.as_ref().is_some_and(|policy| {
                    policy.visibility == ScopeVisibility::Selected
                        && policy.selected_repositories.is_empty()
                })
            {
                return gh.require_cli_syntax_v1(current, CliRequirement::SecretEmptySelection);
            }
        }
        Ok(())
    }
    /// Preparation gathers a typed effect; publishing requires execute(..., true, ...).
    pub fn prepare(
        workspace: &ActionsWorkspace,
        command: ActionsCommand,
        values: &BTreeMap<String, String>,
        gh: &GithubProvider,
        cancel: &CancellationToken,
    ) -> GithubResult<Self> {
        if let Some(reason) = workspace.disabled_reason(command) {
            return Err(plain_error(reason));
        }
        use ActionsCommand::*;
        let repo = if command == ChooseScope && value(values, "kind") == "organization" {
            String::new()
        } else {
            workspace.repository()?.to_owned()
        };
        let current_run = || {
            workspace
                .run
                .as_ref()
                .map(|page| page.attempt.run.clone())
                .ok_or_else(|| plain_error("Inspect an exact run attempt first"))
        };
        let mut target = None;
        let operation = match command {
            ChooseScope => ActionsOperation::Scope(match value(values, "kind") {
                "repository" => ActionsScope::Repository {
                    repository: repo.clone(),
                },
                "environment" => ActionsScope::Environment {
                    repository: repo.clone(),
                    environment: value(values, "name").into(),
                },
                "organization" => ActionsScope::Organization {
                    organization: value(values, "name").into(),
                },
                _ => {
                    return Err(plain_error(
                        "Choose repository, environment or organization scope",
                    ))
                }
            }),
            FilterRuns => ActionsOperation::Filter(RunFilter {
                actor: nonempty(value(values, "actor")),
                branch: nonempty(value(values, "branch")),
                event: nonempty(value(values, "event")),
                status: nonempty(value(values, "status")),
                head_sha: nonempty(value(values, "sha")),
                ..RunFilter::default()
            }),
            FilterCaches => ActionsOperation::CacheFilter {
                key: nonempty(value(values, "key")),
                ref_name: nonempty(value(values, "ref")),
            },
            WorkflowRevision => ActionsOperation::Revision {
                workflow: workspace
                    .definition
                    .as_ref()
                    .ok_or_else(|| plain_error("Inspect a workflow first"))?
                    .workflow
                    .clone(),
                revision: value(values, "revision").into(),
            },
            SearchLogs => {
                let query = value(values, "query");
                if query.len() > 1024 {
                    return Err(plain_error("Log query exceeds its 1024-byte budget"));
                }
                let regex = bool_input(values, "regex")?;
                if regex {
                    regex::RegexBuilder::new(query)
                        .size_limit(1024 * 1024)
                        .dfa_size_limit(1024 * 1024)
                        .build()
                        .map_err(|error| plain_error(error.to_string()))?;
                }
                ActionsOperation::LogSearch {
                    query: query.into(),
                    regex,
                }
            }
            ExportLogs => {
                let destination = PathBuf::from(value(values, "destination"));
                if !destination.is_absolute() {
                    return Err(plain_error("Enter an explicit absolute new destination"));
                }
                let page = workspace
                    .run
                    .as_ref()
                    .ok_or_else(|| plain_error("Inspect an exact run attempt first"))?;
                let full = (page.logs_skipped_bytes > 0)
                    .then(|| {
                        let job = page
                            .selected_job
                            .map(|id| {
                                page.attempt
                                    .jobs
                                    .iter()
                                    .find(|job| job.id == id)
                                    .cloned()
                                    .ok_or_else(|| plain_error("Job belongs to another attempt"))
                            })
                            .transpose()?;
                        Ok::<_, GithubError>(Box::new(FullLogSource {
                            repository: page.repository.clone(),
                            run: page.attempt.run.clone(),
                            job,
                            skipped_bytes: page.logs_skipped_bytes,
                        }))
                    })
                    .transpose()?;
                ActionsOperation::LogExport {
                    full,
                    text: page
                        .logs
                        .clone()
                        .ok_or_else(|| plain_error("Load the logs first"))?,
                    context: format!(
                        "run {} attempt {}{}",
                        page.attempt.run.id,
                        page.attempt.run.attempt,
                        page.selected_job
                            .map(|job| format!(" job {job}"))
                            .unwrap_or_default()
                    ),
                    destination,
                }
            }
            RerunAll | RerunFailed | RerunJob | Cancel | ForceCancel | DeleteRun | DeleteLogs
            | ApproveFork => {
                let run = current_run()?;
                target = Some(ActionsTarget::Run {
                    repository: repo.clone(),
                    id: run.id,
                });
                let action = match command {
                    RerunAll => RunMutation::RerunAll {
                        debug: bool_input(values, "debug")?,
                    },
                    RerunFailed => RunMutation::RerunFailed {
                        debug: bool_input(values, "debug")?,
                    },
                    RerunJob => RunMutation::RerunJob {
                        job_id: workspace
                            .run
                            .as_ref()
                            .and_then(|page| page.selected_job)
                            .ok_or_else(|| plain_error("Open a job before rerunning it"))?,
                        debug: bool_input(values, "debug")?,
                    },
                    Cancel => RunMutation::Cancel,
                    ForceCancel => RunMutation::ForceCancel,
                    DeleteRun => RunMutation::DeleteRun,
                    DeleteLogs => RunMutation::DeleteLogs,
                    _ => RunMutation::ApproveFork,
                };
                ActionsOperation::Run { run, action }
            }
            EnableWorkflow | DisableWorkflow => {
                let workflow = workspace
                    .definition
                    .as_ref()
                    .ok_or_else(|| plain_error("Inspect a workflow first"))?
                    .workflow
                    .clone();
                target = Some(ActionsTarget::Workflow {
                    repository: repo.clone(),
                    id: workflow.id,
                });
                ActionsOperation::WorkflowState {
                    workflow,
                    enabled: command == EnableWorkflow,
                }
            }
            DispatchWorkflow => {
                let definition = workspace
                    .definition
                    .as_ref()
                    .ok_or_else(|| plain_error("Inspect workflow at the dispatch ref first"))?
                    .clone();
                if value(values, "revision") != definition.revision {
                    return Err(plain_error(
                        "To run a different branch or tag, close this form, press V on the workflow page and inspect that revision first",
                    ));
                }
                let mut inputs = BTreeMap::new();
                for input in &definition.dispatch_inputs {
                    let entered = value(values, &format!("input:{}", input.name));
                    if entered.is_empty() && !input.required {
                        continue;
                    }
                    let parsed = match input.input_type {
                        DispatchInputType::Boolean => match entered {
                            "true" => Value::Bool(true),
                            "false" => Value::Bool(false),
                            _ => {
                                return Err(plain_error(format!("{} needs a boolean", input.name)))
                            }
                        },
                        DispatchInputType::Number => serde_json::from_str::<Value>(entered)
                            .ok()
                            .filter(Value::is_number)
                            .ok_or_else(|| {
                                plain_error(format!("{} needs a finite JSON number", input.name))
                            })?,
                        _ => Value::String(entered.into()),
                    };
                    inputs.insert(input.name.clone(), parsed);
                }
                let environments = if definition
                    .dispatch_inputs
                    .iter()
                    .any(|input| input.input_type == DispatchInputType::Environment)
                {
                    gh.environments(&repo, cancel)?
                        .iter()
                        .filter_map(|environment| environment["name"].as_str().map(str::to_owned))
                        .collect::<Vec<_>>()
                } else {
                    vec![]
                };
                definition.validated_inputs(&inputs, &environments)?;
                target = Some(ActionsTarget::Workflow {
                    repository: repo.clone(),
                    id: definition.workflow.id,
                });
                ActionsOperation::Dispatch { definition, inputs }
            }
            ExportArtifact | ExtractArtifact => {
                let destination = PathBuf::from(value(values, "destination"));
                if !destination.is_absolute() {
                    return Err(plain_error("Enter an explicit absolute new destination"));
                }
                ActionsOperation::Archive {
                    archive: workspace
                        .archive
                        .as_ref()
                        .ok_or_else(|| plain_error("Inspect the artifact archive first"))?
                        .clone(),
                    destination,
                    extract: command == ExtractArtifact,
                }
            }
            DeleteArtifact => {
                let artifact = workspace
                    .artifact
                    .clone()
                    .ok_or_else(|| plain_error("Inspect an artifact first"))?;
                target = Some(ActionsTarget::Artifact {
                    repository: repo.clone(),
                    id: artifact.id,
                });
                ActionsOperation::DeleteArtifact(artifact)
            }
            ApproveDeployment | RejectDeployment => {
                let id = id_input(values, "id")?;
                let gate = workspace
                    .deployments
                    .iter()
                    .find(|gate| gate.environment_id == id)
                    .ok_or_else(|| {
                        plain_error("Choose a currently displayed pending environment ID")
                    })?;
                if !gate.current_user_can_approve {
                    return Err(plain_error(
                        "The current actor is not eligible to decide this deployment",
                    ));
                }
                let run = current_run()?;
                target = Some(ActionsTarget::DeploymentReview {
                    repository: repo.clone(),
                    run_id: run.id,
                    attempt: run.attempt,
                    head_sha: run.head_sha.clone(),
                    environment_ids: vec![id],
                });
                ActionsOperation::Deployment {
                    run,
                    review: DeploymentReview {
                        environment_ids: vec![id],
                        decision: if command == ApproveDeployment {
                            DeploymentDecision::Approved
                        } else {
                            DeploymentDecision::Rejected
                        },
                        comment: value(values, "comment").into(),
                    },
                }
            }
            DeleteCache => {
                let id = id_input(values, "id")?;
                let cache = workspace
                    .caches
                    .iter()
                    .find(|cache| cache.id == id)
                    .cloned()
                    .ok_or_else(|| plain_error("Choose an exact displayed cache ID"))?;
                target = Some(ActionsTarget::Cache {
                    repository: repo.clone(),
                    id,
                });
                ActionsOperation::DeleteCache(cache)
            }
            SetSecret | DeleteSecret => {
                let name = value(values, "name");
                let observed = workspace
                    .secrets
                    .iter()
                    .find(|secret| secret.name.eq_ignore_ascii_case(name))
                    .cloned();
                target = Some(ActionsTarget::Secret {
                    scope: workspace.scope.clone(),
                    name: name.into(),
                });
                if command == DeleteSecret {
                    ActionsOperation::DeleteSecret(
                        observed
                            .ok_or_else(|| plain_error("Choose an existing displayed secret"))?,
                    )
                } else {
                    ActionsOperation::SetSecret {
                        request: SecretUpdate {
                            name: name.into(),
                            observed,
                            policy: if matches!(workspace.scope, ActionsScope::Organization { .. })
                            {
                                Some(SecretPolicy {
                                    visibility: visibility(values, "visibility")?,
                                    selected_repositories: list(values, "repositories"),
                                })
                            } else {
                                None
                            },
                        },
                        value: SecretValue(value(values, "value").as_bytes().to_vec()),
                    }
                }
            }
            SetVariable | DeleteVariable => {
                let name = value(values, "name");
                let observed = workspace
                    .variables
                    .iter()
                    .find(|variable| variable.name.eq_ignore_ascii_case(name))
                    .cloned();
                target = Some(ActionsTarget::Variable {
                    scope: workspace.scope.clone(),
                    name: name.into(),
                });
                if command == DeleteVariable {
                    ActionsOperation::DeleteVariable(
                        observed
                            .ok_or_else(|| plain_error("Choose an existing displayed variable"))?,
                    )
                } else {
                    ActionsOperation::SetVariable {
                        observed,
                        edit: VariableEdit {
                            name: name.into(),
                            value: value(values, "value").into(),
                            visibility: if matches!(
                                workspace.scope,
                                ActionsScope::Organization { .. }
                            ) {
                                Some(visibility(values, "visibility")?)
                            } else {
                                None
                            },
                            selected_repository_ids: if matches!(
                                workspace.scope,
                                ActionsScope::Organization { .. }
                            ) && value(values, "visibility")
                                == "selected"
                            {
                                Some(ids(values, "repository_ids")?)
                            } else {
                                None
                            },
                        },
                    }
                }
            }
            EditPolicy => {
                let observed = workspace
                    .policy
                    .clone()
                    .ok_or_else(|| plain_error("Inspect policy first"))?;
                let edit = policy_edit(&observed, values)?;
                target = Some(ActionsTarget::Policy {
                    scope: workspace.scope.clone(),
                    section: observed.section,
                });
                ActionsOperation::Policy { observed, edit }
            }
            EditEnvironment => {
                let name = value(values, "name");
                let observed = workspace
                    .selected_environment()
                    .cloned()
                    .ok_or_else(|| plain_error("Choose a displayed environment"))?;
                if observed["name"] != name {
                    return Err(plain_error("This form is bound to the selected environment ID/name. Select another environment in the catalog instead of editing its identity."));
                }
                let wait_timer = value(values, "wait").parse::<u64>().map_err(|_| {
                    plain_error("Wait timer must be a nonnegative number of minutes")
                })?;
                let reviewers = list(values, "reviewers")
                    .iter()
                    .map(|item| {
                        let (kind, id) = item.split_once(':').ok_or_else(|| {
                            plain_error("Use User:123 or Team:456 reviewer identities")
                        })?;
                        if !matches!(kind, "User" | "Team") {
                            return Err(plain_error("Reviewer kind must be User or Team"));
                        }
                        Ok(EnvironmentReviewer {
                            reviewer_type: kind.into(),
                            id: id
                                .parse::<u64>()
                                .ok()
                                .filter(|id| *id > 0)
                                .ok_or_else(|| plain_error("Reviewer ID must be positive"))?,
                        })
                    })
                    .collect::<GithubResult<Vec<_>>>()?;
                let branch_mode = match value(values, "branches") {
                    "all" => DeploymentBranchMode::All,
                    "protected" => DeploymentBranchMode::Protected,
                    "selected" => DeploymentBranchMode::Selected,
                    _ => {
                        return Err(plain_error(
                            "Choose all, protected or selected deployment branches",
                        ))
                    }
                };
                target = Some(ActionsTarget::EnvironmentProtection {
                    repository: repo.clone(),
                    name: name.into(),
                    environment_id: observed["id"].as_u64().filter(|id| *id > 0).ok_or_else(
                        || plain_error("Select the original positive environment ID"),
                    )?,
                });
                ActionsOperation::Environment {
                    name: name.into(),
                    observed,
                    edit: EnvironmentEdit {
                        wait_timer,
                        prevent_self_review: bool_input(values, "prevent_self_review")?,
                        reviewers,
                        branch_mode,
                    },
                }
            }
            RunnerLabels | RemoveRunner => {
                let id = id_input(values, "id")?;
                let runner = workspace
                    .runners
                    .iter()
                    .find(|runner| runner.id == id)
                    .cloned()
                    .ok_or_else(|| plain_error("Choose a displayed runner ID"))?;
                target = Some(ActionsTarget::Runner {
                    scope: workspace.scope.clone(),
                    id,
                });
                if command == RunnerLabels {
                    ActionsOperation::RunnerLabels {
                        runner,
                        labels: list(values, "labels"),
                    }
                } else {
                    ActionsOperation::RemoveRunner(runner)
                }
            }
            EditRunnerGroup | GroupRunners | GroupRepositories | DeleteRunnerGroup => {
                let org = organization(&workspace.scope)?;
                let id = id_input(values, "id")?;
                let group = workspace
                    .runner_groups
                    .iter()
                    .find(|group| group.id == id)
                    .cloned()
                    .ok_or_else(|| plain_error("Choose a displayed runner group ID"))?;
                target = Some(ActionsTarget::RunnerGroup {
                    organization: org.clone(),
                    id,
                });
                match command {
                    EditRunnerGroup => ActionsOperation::EditGroup {
                        org,
                        observed: group,
                        edit: group_edit(values)?,
                    },
                    DeleteRunnerGroup => ActionsOperation::DeleteGroup {
                        org,
                        observed: group,
                    },
                    _ => {
                        let kind = if command == GroupRunners {
                            GroupAssociation::Runners
                        } else {
                            GroupAssociation::Repositories
                        };
                        let observed_ids = gh
                            .group_associations(&org, id, kind.clone(), cancel)?
                            .iter()
                            .map(|item| {
                                item["id"].as_u64().ok_or_else(|| {
                                    plain_error("Association omitted its canonical ID")
                                })
                            })
                            .collect::<GithubResult<Vec<_>>>()?;
                        target = Some(ActionsTarget::GroupAssociations {
                            organization: org.clone(),
                            id,
                            kind: kind.clone(),
                        });
                        ActionsOperation::GroupAssociations {
                            org,
                            group,
                            edit: GroupAssociationEdit {
                                kind,
                                observed_ids,
                                selected_ids: ids(values, "selected_ids")?,
                            },
                        }
                    }
                }
            }
            CreateRunnerGroup => {
                let org = organization(&workspace.scope)?;
                let request = crate::providers::gh::CreateRunnerGroup {
                    settings: group_edit(values)?,
                    repository_ids: ids(values, "repositories")?,
                    runner_ids: ids(values, "runners")?,
                };
                target = Some(ActionsTarget::RunnerGroupCreation {
                    organization: org.clone(),
                    name: request.settings.name.clone(),
                });
                ActionsOperation::CreateGroup { org, request }
            }
        };
        let workflow_preview = workflow_preview_lines(command, &operation, &repo);
        let mut preview = vec![command.label().into(), workspace.context()];
        if let Some(lines) = &workflow_preview {
            preview.extend(lines.iter().cloned());
        } else {
            preview.push(format!("Settings scope: {:?}", workspace.scope));
            if let Some(target) = &target {
                preview.push(format!("Exact affected target: {target:?}"));
            }
        }
        if matches!(&operation, ActionsOperation::SetSecret { request, .. }
            if matches!(workspace.scope, ActionsScope::Organization { .. })
                && request.policy.as_ref().is_some_and(|policy| policy.visibility == ScopeVisibility::Selected && policy.selected_repositories.is_empty()))
        {
            preview.push("Selected repositories: none (explicit empty selection). The secret will be stored in this organization but available to no repositories.".into());
        }
        match &operation {
            ActionsOperation::Policy { observed, edit } => {
                preview.extend(settings::policy_preview(observed, edit))
            }
            ActionsOperation::Environment { observed, edit, .. } => {
                preview.extend(settings::environment_preview(observed, edit))
            }
            _ => {}
        }
        if let ActionsOperation::LogExport {
            full: Some(full), ..
        } = &operation
        {
            preview.push(format!(
                "Only the end of this log is loaded (first {} skipped). Export re-reads the full log from GitHub and streams it to the file.",
                crate::providers::gh::log_size_label(full.skipped_bytes)
            ));
        }
        if let ActionsOperation::Deployment { review, .. } = &operation {
            for id in &review.environment_ids {
                if let Some(gate) = workspace
                    .deployments
                    .iter()
                    .find(|gate| gate.environment_id == *id)
                {
                    preview.push(format!("Environment {} #{} • server eligibility {} • wait {} minutes • decision {:?}",gate.environment_name,id,gate.current_user_can_approve,gate.wait_timer_minutes,review.decision));
                }
            }
        }
        if let Some(run) = &workspace.run {
            if matches!(
                command,
                RerunAll
                    | RerunFailed
                    | RerunJob
                    | Cancel
                    | ForceCancel
                    | DeleteRun
                    | DeleteLogs
                    | ApproveFork
                    | ApproveDeployment
                    | RejectDeployment
            ) {
                preview.push(format!(
                    "Run {} attempt {} • SHA {} • selected job {:?}",
                    run.attempt.run.id,
                    run.attempt.run.attempt,
                    run.attempt.run.head_sha,
                    run.selected_job
                ));
            }
        }
        let fields = workspace.fields(command)?;
        for (key, entered) in values.iter().filter(|_| workflow_preview.is_none()) {
            if fields.iter().any(|field| field.key == *key && field.secret) {
                preview.push(format!("{key}: [masked transient input; never stored]"));
            } else {
                preview.push(format!(
                    "{key}: {}",
                    crate::providers::runner::sanitize_terminal(entered)
                ));
            }
        }
        preview.push("No effects occur until explicit confirmation. Current account and target state are checked again before remote writes.".into());
        Ok(Self {
            command,
            identity: workspace.identity.clone(),
            preview,
            target,
            workspace: workspace.clone(),
            operation,
        })
    }
    pub fn execute(
        &self,
        gh: &GithubProvider,
        confirmed: bool,
        cancel: &CancellationToken,
    ) -> GithubResult<ActionsOutcome> {
        Ok(self.publish(gh, confirmed, cancel)?.refresh(gh, cancel))
    }
    /// Internal compatibility body for operations with no optional post-write reads.
    /// The typed publisher handles asynchronous/run/cache effects separately.
    fn execute_legacy_effects(
        &self,
        gh: &GithubProvider,
        confirmed: bool,
        cancel: &CancellationToken,
    ) -> GithubResult<ActionsOutcome> {
        if !confirmed {
            return Err(plain_error("Actions effect was not confirmed"));
        }
        let mut workspace = self.workspace.clone();
        let repo = match &self.operation {
            ActionsOperation::Scope(_)
            | ActionsOperation::Filter(_)
            | ActionsOperation::LogSearch { .. }
            | ActionsOperation::LogExport { .. }
            | ActionsOperation::Archive { .. } => String::new(),
            _ => workspace.repository()?.to_owned(),
        };
        let identity = &self.identity;
        let scope = workspace.scope.clone();
        let message = match &self.operation {
            ActionsOperation::Filter(filter) => {
                workspace.filter = filter.clone();
                workspace.load_catalog(gh, cancel)?;
                "Run filters applied".into()
            }
            ActionsOperation::CacheFilter { key, ref_name } => {
                workspace.cache_key = key.clone();
                workspace.cache_ref = ref_name.clone();
                workspace.selected = 0;
                workspace.top = 0;
                workspace.view = ActionsView::Caches;
                workspace.load_view(gh, cancel)?;
                "Cache key/ref filters applied; repository-wide usage remains unfiltered".into()
            }
            ActionsOperation::Scope(scope) => {
                workspace.apply(gh, &ActionsAction::Scope(scope.clone()), cancel)?;
                "Actions settings scope selected".into()
            }
            ActionsOperation::Revision { workflow, revision } => {
                workspace.definition =
                    Some(gh.workflow_definition(&repo, workflow, revision, cancel)?);
                "Workflow revision inspected; no dispatch occurred".into()
            }
            ActionsOperation::LogSearch { query, regex } => {
                workspace.log_search = Some((query.clone(), *regex));
                workspace.prepare_rows();
                let matcher = if *regex {
                    Some(
                        regex::RegexBuilder::new(query)
                            .size_limit(1024 * 1024)
                            .dfa_size_limit(1024 * 1024)
                            .build()
                            .map_err(|error| plain_error(error.to_string()))?,
                    )
                } else {
                    None
                };
                let mut matches = 0;
                for (index, row) in workspace.rows.iter_mut().enumerate() {
                    if matcher
                        .as_ref()
                        .map_or(row.label.contains(query), |matcher| {
                            matcher.is_match(&row.label)
                        })
                    {
                        row.role = SemanticToken::Changed;
                        if matches == 0 {
                            workspace.selected = index;
                        }
                        matches += 1;
                    }
                }
                format!("{matches} matching log lines • bounded loaded logs only")
            }
            ActionsOperation::Run { run, action } => {
                let outcome = gh.mutate_run(identity, &repo, run, *action, cancel)?;
                if let Some(updated) = outcome.run {
                    for item in &mut workspace.runs {
                        if item.repository == repo && item.run.id == updated.id {
                            item.run = updated.clone();
                        }
                    }
                    if let Some(page) = &mut workspace.run {
                        if page.attempt.run.attempt != updated.attempt {
                            page.attempt.jobs.clear();
                            page.selected_job = None;
                            page.logs = None;
                            page.logs_skipped_bytes = 0;
                            page.annotations.clear();
                        }
                        page.attempt.run = updated;
                    }
                } else {
                    workspace
                        .runs
                        .retain(|item| item.repository != repo || item.run.id != run.id);
                    workspace.run = None;
                    workspace.view = ActionsView::Runs;
                }
                outcome.explanation
            }
            ActionsOperation::WorkflowState { workflow, enabled } => {
                let updated =
                    gh.set_workflow_enabled(identity, &repo, workflow, *enabled, cancel)?;
                if let Some(definition) = &mut workspace.definition {
                    definition.workflow = updated.clone();
                }
                for item in &mut workspace.workflows {
                    if item.id == updated.id {
                        *item = updated.clone();
                    }
                }
                format!(
                    "Workflow {} is now {} (GitHub confirmed)",
                    updated.name,
                    crate::actions_workspace::workflow_state_label(&updated.state)
                )
            }
            ActionsOperation::Dispatch { definition, inputs } => {
                let outcome = gh.dispatch_workflow(identity, &repo, definition, inputs, cancel)?;
                if let Some(run) = outcome.run {
                    workspace.runs.insert(
                        0,
                        ScopedRun {
                            repository: repo.clone(),
                            run,
                        },
                    );
                }
                outcome.explanation
            }
            ActionsOperation::LogExport {
                full: Some(full),
                context,
                destination,
                ..
            } => {
                let lines = gh.export_run_log_at(
                    &full.repository,
                    &full.run,
                    full.job.as_ref(),
                    destination,
                    cancel,
                )?;
                format!(
                    "Full log for {context} ({lines} lines, re-read from GitHub) written to {}; nothing was overwritten",
                    destination.display()
                )
            }
            ActionsOperation::LogExport {
                text,
                context,
                destination,
                full: None,
            } => {
                export_log_new(text, destination)?;
                format!(
                    "Log for {context} ({} lines) written to {}; nothing was overwritten",
                    text.lines().count(),
                    destination.display()
                )
            }
            ActionsOperation::Archive {
                archive,
                destination,
                extract,
            } => {
                let details = if *extract {
                    format!("{} files", archive.extract_new(destination, cancel)?.len())
                } else {
                    archive.export_zip(destination)?;
                    "ZIP".into()
                };
                format!(
                    "Artifact {details} created without overwrite or execution at {}",
                    destination.display()
                )
            }
            ActionsOperation::DeleteArtifact(artifact) => {
                workspace.artifacts = gh.delete_artifact(identity, &repo, artifact, cancel)?;
                workspace.artifact = None;
                workspace.archive = None;
                workspace.artifact_preview = None;
                workspace.artifact_image = None;
                workspace.artifact_image_rows = None;
                // The provider reads back this exact run's catalogue, preserving
                // unrelated artifacts without broadening the displayed scope.
                workspace.view = ActionsView::Artifacts;
                format!("Artifact {} deletion confirmed", artifact.id)
            }
            ActionsOperation::Deployment { run, review } => {
                let outcome = gh.review_deployments(identity, &repo, run, review, cancel)?;
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
                let run_state = match gh.run(&repo, run.id, cancel) {
                    Ok(fresh) if fresh.head_sha == run.head_sha && fresh.attempt == run.attempt => {
                        let state = format!("{} / {}", fresh.status, fresh.conclusion);
                        if let Some(page) = &mut workspace.run {
                            page.attempt.run = fresh;
                        }
                        state
                    }
                    Ok(_) => {
                        workspace.warnings.push("Decision accepted, but run identity advanced; reload its exact attempt before another decision".into());
                        "identity advanced; reload required".into()
                    }
                    Err(error) => {
                        workspace.warnings.push(format!(
                            "Decision accepted; run refresh unavailable: {}",
                            error.message
                        ));
                        "refresh unavailable".into()
                    }
                };
                workspace.approval_history_error = None;
                workspace.approval_history =
                    match gh.deployment_review_history(&repo, run.id, cancel) {
                        Ok(history) => history,
                        Err(error) => {
                            workspace.approval_history_error = Some(error.message.clone());
                            workspace.warnings.push(format!(
                                "Decision accepted; approval history unavailable: {}",
                                error.message
                            ));
                            vec![]
                        }
                    };
                format!(
                    "Decision {} accepted • selected {:?} • remaining gates {} • selected gates no longer pending {} • run {}",
                    if review.decision == DeploymentDecision::Approved {"approved"} else {"rejected"},
                    review.environment_ids,
                    workspace.deployments.len(),
                    outcome.completed,
                    run_state
                )
            }
            ActionsOperation::DeleteCache(cache) => {
                workspace.caches = gh.delete_cache(identity, &repo, cache, cancel)?;
                workspace.cache_usage = match gh.cache_usage(&repo, cancel) {
                    Ok(usage) => Some(usage),
                    Err(error) => {
                        workspace.warnings.push(format!(
                            "Deletion confirmed, but usage refresh is unavailable: {}",
                            error.message
                        ));
                        None
                    }
                };
                // The provider returns the full post-delete catalogue so unrelated caches
                // can be independently inspected; do not mislabel it as filtered.
                workspace.cache_key = None;
                workspace.cache_ref = None;
                workspace.selected = 0;
                format!(
                    "Only cache {} deleted; unrelated caches preserved",
                    cache.id
                )
            }
            ActionsOperation::SetSecret { request, value } => {
                let secret = std::str::from_utf8(&value.0)
                    .map_err(|_| plain_error("Secret input is not valid UTF-8"))?;
                let updated = gh.set_secret(identity, &scope, request, secret, cancel)?;
                workspace
                    .secrets
                    .retain(|secret| secret.name != updated.name);
                workspace.secrets.push(updated);
                "Secret metadata confirmed. Value was not read back or persisted".into()
            }
            ActionsOperation::DeleteSecret(secret) => {
                workspace.secrets = gh.delete_secret(identity, &scope, secret, cancel)?;
                format!("Secret {} deletion confirmed", secret.name)
            }
            ActionsOperation::SetVariable { observed, edit } => {
                let updated = gh.set_variable(identity, &scope, observed.as_ref(), edit, cancel)?;
                workspace
                    .variables
                    .retain(|variable| variable.name != updated.name);
                workspace.variables.push(updated);
                format!("Variable {} value confirmed", edit.name)
            }
            ActionsOperation::DeleteVariable(variable) => {
                workspace.variables = gh.delete_variable(identity, &scope, variable, cancel)?;
                format!("Variable {} deletion confirmed", variable.name)
            }
            ActionsOperation::Policy { observed, edit } => {
                workspace.policy = Some(gh.edit_actions_policy(identity, observed, edit, cancel)?);
                "Actions policy changed and read-back confirmed".into()
            }
            ActionsOperation::Environment {
                name,
                observed,
                edit,
            } => {
                let updated = gh.edit_environment(identity, &repo, observed, edit, cancel)?;
                if let Some(environment) = workspace
                    .environments
                    .iter_mut()
                    .find(|environment| environment["name"] == name.as_str())
                {
                    *environment = updated;
                }
                "Environment protection changed and read-back confirmed".into()
            }
            ActionsOperation::RunnerLabels { runner, labels } => {
                let updated = gh.set_runner_labels(identity, &scope, runner, labels, cancel)?;
                if let Some(runner) = workspace
                    .runners
                    .iter_mut()
                    .find(|runner| runner.id == updated.id)
                {
                    *runner = updated;
                }
                "Custom runner labels replaced; built-in labels preserved".into()
            }
            ActionsOperation::RemoveRunner(runner) => {
                workspace.runners = gh.remove_runner(identity, &scope, runner, cancel)?;
                format!(
                    "Runner {} removed; no service installed or compute provisioned",
                    runner.id
                )
            }
            ActionsOperation::EditGroup {
                org,
                observed,
                edit,
            } => {
                let updated = gh.edit_runner_group(identity, org, observed, edit, cancel)?;
                workspace
                    .runner_groups
                    .retain(|group| group.id != updated.id);
                workspace.runner_groups.push(updated);
                "Runner group policy read-back confirmed".into()
            }
            ActionsOperation::GroupAssociations { org, group, edit } => {
                let updated = gh.set_group_associations(identity, org, group, edit, cancel)?;
                format!(
                    "Runner group {} association set confirmed: {} entries",
                    group.id,
                    updated.len()
                )
            }
            ActionsOperation::CreateGroup { org, request } => {
                let group = gh.create_runner_group(identity, org, request, cancel)?;
                let id = group.id;
                workspace.runner_groups.push(group);
                format!("Runner group {id} created and policy/memberships confirmed")
            }
            ActionsOperation::DeleteGroup { org, observed } => {
                workspace.runner_groups =
                    gh.delete_runner_group(identity, org, observed, cancel)?;
                format!("Runner group {} deletion confirmed", observed.id)
            }
        };
        workspace.fetched_at = Some(now());
        workspace.prepare_rows();
        Ok(ActionsOutcome { message, workspace })
    }
}
fn nonempty(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.trim().into())
}
fn policy_edit(
    snapshot: &PolicySnapshot,
    values: &BTreeMap<String, String>,
) -> GithubResult<PolicyEdit> {
    let allowed = || match value(values, "allowed_actions") {
        "all" => Ok(AllowedActions::All),
        "local_only" => Ok(AllowedActions::LocalOnly),
        "selected" => Ok(AllowedActions::Selected),
        _ => Err(plain_error("Choose all, local_only or selected actions")),
    };
    Ok(match snapshot.section {
        PolicySection::Permissions => {
            if matches!(snapshot.scope, ActionsScope::Organization { .. }) {
                PolicyEdit::OrganizationPermissions {
                    enabled_repositories: visibility(values, "enabled_repositories")?,
                    allowed_actions: allowed()?,
                    sha_pinning_required: bool_input(values, "sha_pinning_required")?,
                }
            } else {
                PolicyEdit::RepositoryPermissions {
                    enabled: bool_input(values, "enabled")?,
                    allowed_actions: allowed()?,
                    sha_pinning_required: bool_input(values, "sha_pinning_required")?,
                }
            }
        }
        PolicySection::SelectedActions => PolicyEdit::SelectedActions {
            github_owned_allowed: bool_input(values, "github_owned_allowed")?,
            verified_allowed: bool_input(values, "verified_allowed")?,
            patterns_allowed: list(values, "patterns"),
        },
        PolicySection::WorkflowToken => PolicyEdit::WorkflowToken {
            default_workflow_permissions: match value(values, "permission") {
                "read" => WorkflowTokenPermission::Read,
                "write" => WorkflowTokenPermission::Write,
                _ => {
                    return Err(plain_error(
                        "Choose read or write workflow token permission",
                    ))
                }
            },
            can_approve_pull_request_reviews: bool_input(values, "approve_reviews")?,
        },
        PolicySection::ForkWorkflows => PolicyEdit::ForkWorkflows {
            run_workflows_from_fork_pull_requests: bool_input(
                values,
                "run_workflows_from_fork_pull_requests",
            )?,
            send_write_tokens_to_workflows: bool_input(values, "send_write_tokens_to_workflows")?,
            send_secrets_and_variables: bool_input(values, "send_secrets_and_variables")?,
            require_approval_for_fork_pr_workflows: bool_input(
                values,
                "require_approval_for_fork_pr_workflows",
            )?,
        },
        PolicySection::ForkApproval => PolicyEdit::ForkApproval {
            approval_policy: match value(values, "approval_policy") {
                "first_time_contributors_new_to_github" => {
                    ForkApprovalPolicy::FirstTimeContributorsNewToGithub
                }
                "first_time_contributors" => ForkApprovalPolicy::FirstTimeContributors,
                "all_external_contributors" => ForkApprovalPolicy::AllExternalContributors,
                _ => {
                    return Err(plain_error(
                        "Choose an explicit fork contributor approval policy",
                    ))
                }
            },
        },
        PolicySection::Retention => PolicyEdit::Retention {
            days: id_input(values, "days")?,
        },
    })
}

/// Plain-language preview for workflow enable/disable/dispatch: names the
/// workflow, file, branch/tag and every input value GitHub will receive.
fn workflow_preview_lines(
    command: ActionsCommand,
    operation: &ActionsOperation,
    repo: &str,
) -> Option<Vec<String>> {
    let sanitize = crate::providers::runner::sanitize_terminal;
    match (command, operation) {
        (_, ActionsOperation::WorkflowState { workflow, enabled }) => Some(vec![
            format!(
                "Workflow: {} ({}) in {repo}",
                sanitize(&workflow.name),
                sanitize(&workflow.path)
            ),
            format!(
                "Effect: {}",
                if *enabled {
                    "GitHub starts running this workflow again on its triggers"
                } else {
                    "GitHub stops starting new runs of this workflow until it is enabled again; existing runs and history are kept"
                }
            ),
        ]),
        (ActionsCommand::DispatchWorkflow, ActionsOperation::Dispatch { definition, inputs }) => {
            let mut lines = vec![
                format!(
                    "Workflow: {} ({}) in {repo}",
                    sanitize(&definition.workflow.name),
                    sanitize(&definition.workflow.path)
                ),
                format!(
                    "Branch or tag: {} (commit {})",
                    sanitize(&definition.revision),
                    definition.commit_sha
                ),
                "Effect: GitHub starts one new workflow_dispatch run of this workflow".into(),
            ];
            if definition.dispatch_inputs.is_empty() {
                lines.push("Inputs: none".into());
            }
            for input in &definition.dispatch_inputs {
                lines.push(match inputs.get(&input.name) {
                    Some(Value::String(text)) => {
                        format!("Input {}: {}", input.name, sanitize(text))
                    }
                    Some(value) => format!("Input {}: {value}", input.name),
                    None => format!(
                        "Input {}: (not sent; GitHub uses the workflow default)",
                        input.name
                    ),
                });
            }
            Some(lines)
        }
        _ => None,
    }
}

/// Writes loaded log text to a new file: staged in the destination's directory, then
/// persisted without clobbering an existing path.
fn export_log_new(text: &str, destination: &std::path::Path) -> GithubResult<()> {
    use std::io::Write;
    let parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| plain_error("Choose a destination inside an existing directory"))?;
    let mut staging = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| plain_error(format!("Log export failed: {error}")))?;
    staging
        .write_all(text.as_bytes())
        .and_then(|_| staging.as_file().sync_all())
        .map_err(|error| plain_error(format!("Log export failed: {error}")))?;
    staging.persist_noclobber(destination).map_err(|_| {
        plain_error("Log export destination already exists or cannot be created; choose a new explicit path")
    })?;
    Ok(())
}

#[cfg(test)]
mod log_export_tests {
    use super::export_log_new;

    #[test]
    fn log_export_creates_new_file_and_never_overwrites() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("run-123.log");
        export_log_new("first line\n##[error]boom\n", &destination).unwrap();
        assert_eq!(
            std::fs::read_to_string(&destination).unwrap(),
            "first line\n##[error]boom\n"
        );
        let refused = export_log_new("other text\n", &destination).unwrap_err();
        assert!(refused.message.contains("already exists"));
        assert_eq!(
            std::fs::read_to_string(&destination).unwrap(),
            "first line\n##[error]boom\n"
        );
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
        assert!(export_log_new("x", std::path::Path::new("/")).is_err());
    }
}
