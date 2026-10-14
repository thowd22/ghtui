//! Native, permission-gated operational settings; every edit has a specific typed section.
use super::*;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PolicySection {
    Permissions,
    SelectedActions,
    WorkflowToken,
    ForkWorkflows,
    ForkApproval,
    Retention,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PolicySnapshot {
    pub scope: ActionsScope,
    pub section: PolicySection,
    pub value: Value,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AllowedActions {
    All,
    LocalOnly,
    Selected,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WorkflowTokenPermission {
    Read,
    Write,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ForkApprovalPolicy {
    FirstTimeContributorsNewToGithub,
    FirstTimeContributors,
    AllExternalContributors,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PolicyEdit {
    RepositoryPermissions {
        enabled: bool,
        allowed_actions: AllowedActions,
        sha_pinning_required: bool,
    },
    OrganizationPermissions {
        enabled_repositories: ScopeVisibility,
        allowed_actions: AllowedActions,
        sha_pinning_required: bool,
    },
    SelectedActions {
        github_owned_allowed: bool,
        verified_allowed: bool,
        patterns_allowed: Vec<String>,
    },
    WorkflowToken {
        default_workflow_permissions: WorkflowTokenPermission,
        can_approve_pull_request_reviews: bool,
    },
    ForkWorkflows {
        run_workflows_from_fork_pull_requests: bool,
        send_write_tokens_to_workflows: bool,
        send_secrets_and_variables: bool,
        require_approval_for_fork_pr_workflows: bool,
    },
    ForkApproval {
        approval_policy: ForkApprovalPolicy,
    },
    Retention {
        days: u64,
    },
}
impl PolicyEdit {
    fn payload(&self, scope: &ActionsScope) -> GithubResult<(PolicySection, Value)> {
        let (section, payload) = match self {
            Self::RepositoryPermissions {
                enabled,
                allowed_actions,
                sha_pinning_required,
            } => {
                if !matches!(scope, ActionsScope::Repository { .. }) {
                    return Err(error(
                        GithubErrorKind::InvalidTarget,
                        "Repository permission edits need a repository scope",
                    ));
                }
                (
                    PolicySection::Permissions,
                    json!({"enabled":enabled,"allowed_actions":allowed_actions,"sha_pinning_required":sha_pinning_required}),
                )
            }
            Self::OrganizationPermissions {
                enabled_repositories,
                allowed_actions,
                sha_pinning_required,
            } => {
                if !matches!(scope, ActionsScope::Organization { .. }) {
                    return Err(error(
                        GithubErrorKind::InvalidTarget,
                        "Organization permission edits need an organization scope",
                    ));
                }
                (
                    PolicySection::Permissions,
                    json!({"enabled_repositories":enabled_repositories,"allowed_actions":allowed_actions,"sha_pinning_required":sha_pinning_required}),
                )
            }
            Self::SelectedActions {
                github_owned_allowed,
                verified_allowed,
                patterns_allowed,
            } => {
                if patterns_allowed.len() > 1000 {
                    return Err(error(
                        GithubErrorKind::InvalidTarget,
                        "Too many allowed action patterns",
                    ));
                }
                for pattern in patterns_allowed {
                    nonempty_parameter(pattern)?;
                }
                (
                    PolicySection::SelectedActions,
                    json!({"github_owned_allowed":github_owned_allowed,"verified_allowed":verified_allowed,"patterns_allowed":patterns_allowed}),
                )
            }
            Self::WorkflowToken {
                default_workflow_permissions,
                can_approve_pull_request_reviews,
            } => (
                PolicySection::WorkflowToken,
                json!({"default_workflow_permissions":default_workflow_permissions,"can_approve_pull_request_reviews":can_approve_pull_request_reviews}),
            ),
            Self::ForkWorkflows {
                run_workflows_from_fork_pull_requests,
                send_write_tokens_to_workflows,
                send_secrets_and_variables,
                require_approval_for_fork_pr_workflows,
            } => (
                PolicySection::ForkWorkflows,
                json!({"run_workflows_from_fork_pull_requests":run_workflows_from_fork_pull_requests,"send_write_tokens_to_workflows":send_write_tokens_to_workflows,"send_secrets_and_variables":send_secrets_and_variables,"require_approval_for_fork_pr_workflows":require_approval_for_fork_pr_workflows}),
            ),
            Self::ForkApproval { approval_policy } => (
                PolicySection::ForkApproval,
                json!({"approval_policy":approval_policy}),
            ),
            Self::Retention { days } => {
                if *days == 0 || *days > 365 {
                    return Err(error(GithubErrorKind::InvalidTarget,"Choose retention between 1 and 365 days; the owning enterprise may impose a lower maximum"));
                }
                (PolicySection::Retention, json!({"days":days}))
            }
        };
        Ok((section, payload))
    }
}
fn policy_endpoint(scope: &ActionsScope, section: PolicySection) -> GithubResult<String> {
    if matches!(scope, ActionsScope::Environment { .. }) {
        return Err(error(
            GithubErrorKind::InvalidTarget,
            "Environment protection rules use the separate environment editor",
        ));
    }
    let suffix = match section {
        PolicySection::Permissions => "permissions",
        PolicySection::SelectedActions => "permissions/selected-actions",
        PolicySection::WorkflowToken => "permissions/workflow",
        PolicySection::ForkWorkflows => "permissions/fork-pr-workflows-private-repos",
        PolicySection::ForkApproval => "permissions/fork-pr-contributor-approval",
        PolicySection::Retention => "permissions/artifact-and-log-retention",
    };
    Ok(format!("{}/{suffix}", scope.prefix()?))
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunnerLabel {
    pub id: u64,
    pub name: String,
    pub label_type: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SelfHostedRunner {
    pub id: u64,
    pub name: String,
    pub os: String,
    pub status: String,
    pub busy: bool,
    pub labels: Vec<RunnerLabel>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunnerGroup {
    pub id: u64,
    pub name: String,
    pub visibility: String,
    pub default: bool,
    pub inherited: bool,
    pub allows_public_repositories: bool,
    pub restricted_to_workflows: bool,
    pub selected_workflows: Vec<String>,
    pub workflow_restrictions_read_only: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunnerGroupEdit {
    pub name: String,
    pub visibility: ScopeVisibility,
    pub allows_public_repositories: bool,
    pub restricted_to_workflows: bool,
    pub selected_workflows: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum GroupAssociation {
    Runners,
    Repositories,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroupAssociationEdit {
    pub kind: GroupAssociation,
    pub observed_ids: Vec<u64>,
    pub selected_ids: Vec<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnvironmentReviewer {
    pub reviewer_type: String,
    pub id: u64,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum DeploymentBranchMode {
    All,
    Protected,
    Selected,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EnvironmentEdit {
    pub wait_timer: u64,
    pub prevent_self_review: bool,
    pub reviewers: Vec<EnvironmentReviewer>,
    pub branch_mode: DeploymentBranchMode,
}
fn runner_from_json(v: &Value) -> SelfHostedRunner {
    SelfHostedRunner {
        id: number(v, "id"),
        name: text(v, "name"),
        os: text(v, "os"),
        status: text(v, "status"),
        busy: boolean(v, "busy"),
        labels: v["labels"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|v| RunnerLabel {
                id: number(v, "id"),
                name: text(v, "name"),
                label_type: text(v, "type"),
            })
            .collect(),
    }
}
fn group_from_json(v: &Value) -> RunnerGroup {
    RunnerGroup {
        id: number(v, "id"),
        name: text(v, "name"),
        visibility: text(v, "visibility"),
        default: boolean(v, "default"),
        inherited: boolean(v, "inherited"),
        allows_public_repositories: boolean(v, "allows_public_repositories"),
        restricted_to_workflows: boolean(v, "restricted_to_workflows"),
        selected_workflows: v["selected_workflows"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        workflow_restrictions_read_only: boolean(v, "workflow_restrictions_read_only"),
    }
}
fn runner_prefix(scope: &ActionsScope) -> GithubResult<String> {
    if matches!(scope, ActionsScope::Environment { .. }) {
        return Err(error(
            GithubErrorKind::InvalidTarget,
            "Runners belong to repository or organization scopes, not an environment",
        ));
    }
    scope.prefix()
}

impl GithubProvider {
    pub fn actions_policy(
        &self,
        scope: &ActionsScope,
        section: PolicySection,
        cancel: &CancellationToken,
    ) -> GithubResult<PolicySnapshot> {
        let value = self.api(
            &policy_endpoint(scope, section)?,
            "GET",
            None,
            None,
            false,
            cancel,
        )?;
        if !value.is_object() {
            return Err(error(
                GithubErrorKind::Unsupported,
                "The host did not expose this Actions policy section",
            ));
        }
        Ok(PolicySnapshot {
            scope: scope.clone(),
            section,
            value,
        })
    }
    pub fn edit_actions_policy(
        &self,
        identity: &HostIdentity,
        observed: &PolicySnapshot,
        edit: &PolicyEdit,
        cancel: &CancellationToken,
    ) -> GithubResult<PolicySnapshot> {
        let (section, payload) = edit.payload(&observed.scope)?;
        if section != observed.section {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "This editor does not match the selected policy section",
            ));
        }
        self.actions_identity(identity, cancel)?;
        let current = self.actions_policy(&observed.scope, section, cancel)?;
        for key in payload.as_object().expect("typed policy object").keys() {
            if current.value.get(key) != observed.value.get(key) {
                return Err(error(
                    GithubErrorKind::StaleHead,
                    format!("Policy {key} changed since the preview; refresh before overwriting"),
                ));
            }
        }
        if matches!(section, PolicySection::Retention) {
            if let Some(max) = current.value["maximum_allowed_days"].as_u64() {
                if payload["days"].as_u64().unwrap_or(0) > max {
                    return Err(error(
                        GithubErrorKind::Permission,
                        "Requested retention exceeds the owning policy maximum",
                    ));
                }
            }
        }
        let endpoint = policy_endpoint(&observed.scope, section)?;
        let resource = format!("{}/{endpoint}", self.host);
        self.api(
            &endpoint,
            "PUT",
            Some(payload.clone()),
            Some(resource.clone()),
            false,
            cancel,
        )?;
        let result = self.after_write(
            &resource,
            self.actions_policy(&observed.scope, section, cancel),
        )?;
        for (key, value) in payload.as_object().expect("typed policy object") {
            if result.value.get(key) != Some(value) {
                return Err(self.actions_ambiguous(
                    &resource,
                    "Actions policy readback differs from the confirmed change; reconcile",
                ));
            }
        }
        Ok(result)
    }
    pub fn runners(
        &self,
        scope: &ActionsScope,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<SelfHostedRunner>> {
        Ok(self
            .api_list(
                &format!("{}/runners?per_page=100", runner_prefix(scope)?),
                Some("runners"),
                cancel,
            )?
            .iter()
            .map(runner_from_json)
            .collect())
    }
    pub fn runner(
        &self,
        scope: &ActionsScope,
        id: u64,
        cancel: &CancellationToken,
    ) -> GithubResult<SelfHostedRunner> {
        if id == 0 {
            return Err(error(GithubErrorKind::InvalidTarget, "Select a runner ID"));
        }
        let runner = runner_from_json(&self.api(
            &format!("{}/runners/{id}", runner_prefix(scope)?),
            "GET",
            None,
            None,
            false,
            cancel,
        )?);
        if runner.id != id {
            return Err(error(
                GithubErrorKind::Json,
                "Runner returned the wrong identity",
            ));
        }
        Ok(runner)
    }
    pub fn set_runner_labels(
        &self,
        identity: &HostIdentity,
        scope: &ActionsScope,
        observed: &SelfHostedRunner,
        labels: &[String],
        cancel: &CancellationToken,
    ) -> GithubResult<SelfHostedRunner> {
        if labels.iter().collect::<BTreeSet<_>>().len() != labels.len() {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Custom runner labels must be distinct",
            ));
        }
        for label in labels {
            nonempty_parameter(label)?;
        }
        self.actions_identity(identity, cancel)?;
        if self.runner(scope, observed.id, cancel)? != *observed {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Runner state or labels changed; refresh before replacement",
            ));
        }
        let endpoint = format!("{}/runners/{}/labels", runner_prefix(scope)?, observed.id);
        let resource = format!("{}/{endpoint}", self.host);
        self.api(
            &endpoint,
            "PUT",
            Some(json!({"labels":labels})),
            Some(resource.clone()),
            false,
            cancel,
        )?;
        let fresh = self.after_write(&resource, self.runner(scope, observed.id, cancel))?;
        let actual = fresh
            .labels
            .iter()
            .filter(|label| label.label_type == "custom")
            .map(|label| label.name.to_lowercase())
            .collect::<BTreeSet<_>>();
        let expected = labels
            .iter()
            .map(|label| label.to_lowercase())
            .collect::<BTreeSet<_>>();
        let observed_read_only = observed
            .labels
            .iter()
            .filter(|label| label.label_type != "custom")
            .map(|label| {
                (
                    label.id,
                    label.name.to_ascii_lowercase(),
                    label.label_type.as_str(),
                )
            })
            .collect::<BTreeSet<_>>();
        let fresh_read_only = fresh
            .labels
            .iter()
            .filter(|label| label.label_type != "custom")
            .map(|label| {
                (
                    label.id,
                    label.name.to_ascii_lowercase(),
                    label.label_type.as_str(),
                )
            })
            .collect::<BTreeSet<_>>();
        if observed_read_only != fresh_read_only {
            return Err(self.actions_ambiguous(&resource,"Read-only runner labels changed or disappeared during custom-label replacement; their preservation is not confirmed. Inspect this exact runner before another write."));
        }
        if actual != expected {
            return Err(self.actions_ambiguous(&resource,"Custom runner labels were not confirmed; read-only built-in labels are never edited"));
        }
        Ok(fresh)
    }
    pub fn remove_runner(
        &self,
        identity: &HostIdentity,
        scope: &ActionsScope,
        observed: &SelfHostedRunner,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<SelfHostedRunner>> {
        self.actions_identity(identity, cancel)?;
        if self.runner(scope, observed.id, cancel)? != *observed {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Runner state changed; refresh before removal",
            ));
        }
        if observed.busy {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "The selected runner is busy; removal is blocked until it is idle",
            ));
        }
        let endpoint = format!("{}/runners/{}", runner_prefix(scope)?, observed.id);
        let resource = format!("{}/{endpoint}", self.host);
        self.api(
            &endpoint,
            "DELETE",
            None,
            Some(resource.clone()),
            false,
            cancel,
        )?;
        let fresh = self.after_write(&resource, self.runners(scope, cancel))?;
        if fresh.iter().any(|r| r.id == observed.id) {
            return Err(self.actions_ambiguous(
                &resource,
                "Runner is still associated after removal; reconcile",
            ));
        }
        Ok(fresh)
    }
    pub fn runner_groups(
        &self,
        org: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<RunnerGroup>> {
        validate_name(org)?;
        Ok(self
            .api_list(
                &format!("orgs/{org}/actions/runner-groups?per_page=100"),
                Some("runner_groups"),
                cancel,
            )?
            .iter()
            .map(group_from_json)
            .collect())
    }
    pub fn runner_group(
        &self,
        org: &str,
        id: u64,
        cancel: &CancellationToken,
    ) -> GithubResult<RunnerGroup> {
        validate_name(org)?;
        if id == 0 {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Select a runner group",
            ));
        }
        let group = group_from_json(&self.api(
            &format!("orgs/{org}/actions/runner-groups/{id}"),
            "GET",
            None,
            None,
            false,
            cancel,
        )?);
        if group.id != id {
            return Err(error(
                GithubErrorKind::Json,
                "Runner group returned the wrong ID",
            ));
        }
        Ok(group)
    }
    pub fn edit_runner_group(
        &self,
        identity: &HostIdentity,
        org: &str,
        observed: &RunnerGroup,
        edit: &RunnerGroupEdit,
        cancel: &CancellationToken,
    ) -> GithubResult<RunnerGroup> {
        nonempty_parameter(&edit.name)?;
        for workflow in &edit.selected_workflows {
            nonempty_parameter(workflow)?;
        }
        if observed.inherited || observed.workflow_restrictions_read_only {
            return Err(error(GithubErrorKind::Permission,"Inherited or read-only runner group restrictions must be changed by their owning scope"));
        }
        self.actions_identity(identity, cancel)?;
        if self.runner_group(org, observed.id, cancel)? != *observed {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Runner group changed; refresh before editing",
            ));
        }
        let endpoint = format!("orgs/{org}/actions/runner-groups/{}", observed.id);
        let resource = format!("{}/{endpoint}", self.host);
        let payload = json!({"name":edit.name,"visibility":edit.visibility,"allows_public_repositories":edit.allows_public_repositories,"restricted_to_workflows":edit.restricted_to_workflows,"selected_workflows":edit.selected_workflows});
        self.api(
            &endpoint,
            "PATCH",
            Some(payload),
            Some(resource.clone()),
            false,
            cancel,
        )?;
        let fresh = self.after_write(&resource, self.runner_group(org, observed.id, cancel))?;
        if fresh.name != edit.name
            || fresh.visibility
                != match edit.visibility {
                    ScopeVisibility::All => "all",
                    ScopeVisibility::Private => "private",
                    ScopeVisibility::Selected => "selected",
                }
            || fresh.allows_public_repositories != edit.allows_public_repositories
            || fresh.restricted_to_workflows != edit.restricted_to_workflows
            || edit.restricted_to_workflows && fresh.selected_workflows != edit.selected_workflows
        {
            return Err(self.actions_ambiguous(
                &resource,
                "Runner group policy was not confirmed; reconcile",
            ));
        }
        Ok(fresh)
    }
    pub fn group_associations(
        &self,
        org: &str,
        id: u64,
        kind: GroupAssociation,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<Value>> {
        validate_name(org)?;
        if id == 0 {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Select a runner group",
            ));
        }
        let key = match kind {
            GroupAssociation::Runners => "runners",
            GroupAssociation::Repositories => "repositories",
        };
        self.api_list(
            &format!("orgs/{org}/actions/runner-groups/{id}/{key}?per_page=100"),
            Some(key),
            cancel,
        )
    }
    pub fn set_group_associations(
        &self,
        identity: &HostIdentity,
        org: &str,
        group: &RunnerGroup,
        edit: &GroupAssociationEdit,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<Value>> {
        let kind = edit.kind.clone();
        let observed_ids = &edit.observed_ids;
        let selected_ids = &edit.selected_ids;
        if selected_ids.contains(&0)
            || selected_ids.iter().collect::<BTreeSet<_>>().len() != selected_ids.len()
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Choose distinct positive association IDs",
            ));
        }
        self.actions_identity(identity, cancel)?;
        if self.runner_group(org, group.id, cancel)? != *group || group.inherited {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Runner group changed or is inherited; refresh in its owning scope",
            ));
        }
        if kind == GroupAssociation::Repositories && group.visibility != "selected" {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Repository associations require selected group visibility",
            ));
        }
        let actual = self
            .group_associations(org, group.id, kind.clone(), cancel)?
            .iter()
            .map(|v| number(v, "id"))
            .collect::<BTreeSet<_>>();
        if actual != observed_ids.iter().copied().collect() {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Group associations changed; refresh before replacing memberships",
            ));
        }
        let (key, payload) = match kind {
            GroupAssociation::Runners => ("runners", json!({"runners":selected_ids})),
            GroupAssociation::Repositories => (
                "repositories",
                json!({"selected_repository_ids":selected_ids}),
            ),
        };
        let endpoint = format!("orgs/{org}/actions/runner-groups/{}/{key}", group.id);
        let resource = format!("{}/{endpoint}", self.host);
        self.api(
            &endpoint,
            "PUT",
            Some(payload),
            Some(resource.clone()),
            false,
            cancel,
        )?;
        let fresh = self.after_write(
            &resource,
            self.group_associations(org, group.id, kind, cancel),
        )?;
        if fresh
            .iter()
            .map(|v| number(v, "id"))
            .collect::<BTreeSet<_>>()
            != selected_ids.iter().copied().collect()
        {
            return Err(self.actions_ambiguous(
                &resource,
                "Group associations did not match the confirmed IDs; reconcile",
            ));
        }
        Ok(fresh)
    }
    pub fn environments(&self, repo: &str, cancel: &CancellationToken) -> GithubResult<Vec<Value>> {
        validate_repo(repo)?;
        self.api_list(
            &format!("repos/{repo}/environments?per_page=100"),
            Some("environments"),
            cancel,
        )
    }
    pub fn environment(
        &self,
        repo: &str,
        name: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        validate_repo(repo)?;
        nonempty_parameter(name)?;
        let value = self.api(
            &format!("repos/{repo}/environments/{}", url_component(name)),
            "GET",
            None,
            None,
            false,
            cancel,
        )?;
        if text(&value, "name") != name || number(&value, "id") == 0 {
            return Err(error(
                GithubErrorKind::Json,
                "Environment returned the wrong identity",
            ));
        }
        Ok(value)
    }
    pub fn edit_environment(
        &self,
        identity: &HostIdentity,
        repo: &str,
        observed: &Value,
        edit: &EnvironmentEdit,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        if number(observed, "id") == 0 {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Select a positive canonical environment identity before changing protection rules",
            ));
        }
        if edit.wait_timer > 43_200
            || edit.reviewers.len() > 6
            || edit
                .reviewers
                .iter()
                .any(|r| r.id == 0 || !matches!(r.reviewer_type.as_str(), "User" | "Team"))
        {
            return Err(error(GithubErrorKind::InvalidTarget,"Environment protection requires a 0–43,200 minute timer and at most six verified User/Team reviewers"));
        }
        let name = text(observed, "name");
        self.actions_identity(identity, cancel)?;
        let fresh = self.environment(repo, &name, cancel)?;
        if fresh != *observed {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Environment settings changed; refresh before editing protection rules",
            ));
        }
        let branch = match edit.branch_mode {
            DeploymentBranchMode::All => Value::Null,
            DeploymentBranchMode::Protected => {
                json!({"protected_branches":true,"custom_branch_policies":false})
            }
            DeploymentBranchMode::Selected => {
                json!({"protected_branches":false,"custom_branch_policies":true})
            }
        };
        let reviewers = edit
            .reviewers
            .iter()
            .map(|r| json!({"type":r.reviewer_type,"id":r.id}))
            .collect::<Vec<_>>();
        let endpoint = format!("repos/{repo}/environments/{}", url_component(&name));
        let resource = format!("{}/{endpoint}", self.host);
        self.api(&endpoint,"PUT",Some(json!({"wait_timer":edit.wait_timer,"prevent_self_review":edit.prevent_self_review,"reviewers":reviewers,"deployment_branch_policy":branch})),Some(resource.clone()),false,cancel)?;
        let result = self.after_write(&resource, self.environment(repo, &name, cancel))?;
        if number(&result, "id") != number(observed, "id") {
            return Err(self.actions_ambiguous(&resource, "Environment identity changed despite matching name/rules; no different environment is confirmed. Inspect the original target before another write."));
        }
        let rules = result["protection_rules"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let timer = rules
            .iter()
            .find(|r| text(r, "type") == "wait_timer")
            .map(|r| number(r, "wait_timer"))
            .unwrap_or(0);
        let review = rules
            .iter()
            .find(|r| text(r, "type") == "required_reviewers");
        let actual = review
            .and_then(|r| r["reviewers"].as_array())
            .into_iter()
            .flatten()
            .map(|r| (text(r, "type"), number(&r["reviewer"], "id")))
            .collect::<BTreeSet<_>>();
        let expected = edit
            .reviewers
            .iter()
            .map(|r| (r.reviewer_type.clone(), r.id))
            .collect::<BTreeSet<_>>();
        if timer != edit.wait_timer
            || actual != expected
            || review.is_some_and(|r| boolean(r, "prevent_self_review")) != edit.prevent_self_review
            || result["deployment_branch_policy"] != branch
        {
            return Err(self.actions_ambiguous(
                &resource,
                "Environment protection readback differs from the confirmed settings; reconcile",
            ));
        }
        Ok(result)
    }
}
