//! Explicit environment, runner-group and usage operations. No compute provisioning or billing writes.
use super::settings::setting_name;
use super::*;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DeploymentPatternType {
    Branch,
    Tag,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeploymentPattern {
    pub id: u64,
    pub name: String,
    pub pattern_type: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeploymentPatternEdit {
    pub observed: Option<DeploymentPattern>,
    pub name: String,
    pub pattern_type: DeploymentPatternType,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CustomProtectionRule {
    pub id: u64,
    pub enabled: bool,
    pub app: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateRunnerGroup {
    pub settings: RunnerGroupEdit,
    pub repository_ids: Vec<u64>,
    pub runner_ids: Vec<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HostedRunner {
    pub id: u64,
    pub name: String,
    pub group_id: u64,
    pub platform: String,
    pub status: String,
    pub maximum_runners: u64,
    pub image: Value,
    pub machine_size: Value,
    pub last_active_on: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CacheLimit {
    pub max_cache_size_gb: u64,
    pub max_cache_retention_days: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrganizationCacheUsage {
    pub total_active_caches_count: u64,
    pub total_active_caches_size_in_bytes: u64,
    pub repositories: Vec<Value>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkflowUsage {
    pub billable: Value,
    pub run_duration_ms: u64,
    pub limitation: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SettingMembershipEdit {
    pub name: String,
    pub secret: bool,
    pub observed_ids: Vec<u64>,
    pub selected_ids: Vec<u64>,
}
fn environment_prefix(repo: &str, name: &str) -> GithubResult<String> {
    validate_repo(repo)?;
    nonempty_parameter(name)?;
    Ok(format!("repos/{repo}/environments/{}", url_component(name)))
}
fn pattern_from_json(value: &Value) -> GithubResult<DeploymentPattern> {
    let id = number(value, "id");
    let name = text(value, "name");
    if id == 0 || name.is_empty() {
        return Err(error(
            GithubErrorKind::Json,
            "Deployment pattern returned no canonical ID/name",
        ));
    }
    Ok(DeploymentPattern {
        id,
        name,
        pattern_type: value["type"].as_str().map(str::to_owned),
    })
}
fn custom_from_json(value: &Value) -> GithubResult<CustomProtectionRule> {
    let id = number(value, "id");
    if id == 0 || number(&value["app"], "id") == 0 {
        return Err(error(
            GithubErrorKind::Json,
            "Custom deployment rule has no rule/App identity",
        ));
    }
    Ok(CustomProtectionRule {
        id,
        enabled: boolean(value, "enabled"),
        app: value["app"].clone(),
    })
}
fn distinct_ids(ids: &[u64]) -> GithubResult<()> {
    if ids.contains(&0) || ids.iter().collect::<BTreeSet<_>>().len() != ids.len() {
        Err(error(
            GithubErrorKind::InvalidTarget,
            "Choose distinct positive IDs for this exact scope",
        ))
    } else {
        Ok(())
    }
}
impl GithubProvider {
    pub fn deployment_patterns(
        &self,
        repo: &str,
        name: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<DeploymentPattern>> {
        self.api_list(
            &format!(
                "{}/deployment-branch-policies?per_page=100",
                environment_prefix(repo, name)?
            ),
            Some("branch_policies"),
            cancel,
        )?
        .iter()
        .map(pattern_from_json)
        .collect()
    }
    pub fn set_deployment_pattern(
        &self,
        identity: &HostIdentity,
        repo: &str,
        environment: &str,
        edit: &DeploymentPatternEdit,
        cancel: &CancellationToken,
    ) -> GithubResult<DeploymentPattern> {
        nonempty_parameter(&edit.name)?;
        self.actions_identity(identity, cancel)?;
        let state = self.environment(repo, environment, cancel)?;
        if !boolean(&state["deployment_branch_policy"], "custom_branch_policies") {
            return Err(error(GithubErrorKind::InvalidTarget,"Enable selected deployment branches/tags in the environment before adding patterns"));
        }
        let prefix = format!(
            "{}/deployment-branch-policies",
            environment_prefix(repo, environment)?
        );
        let kind = match edit.pattern_type {
            DeploymentPatternType::Branch => "branch",
            DeploymentPatternType::Tag => "tag",
        };
        let (endpoint, method, payload) = if let Some(observed) = &edit.observed {
            let current = pattern_from_json(&self.api(
                &format!("{prefix}/{}", observed.id),
                "GET",
                None,
                None,
                false,
                cancel,
            )?)?;
            if current != *observed {
                return Err(error(
                    GithubErrorKind::StaleHead,
                    "Deployment pattern changed; refresh before editing",
                ));
            }
            if observed
                .pattern_type
                .as_deref()
                .is_some_and(|value| value != kind)
            {
                return Err(error(GithubErrorKind::Unsupported,"The public update endpoint changes the pattern name only; changing branch/tag type requires explicit delete and create"));
            }
            (
                format!("{prefix}/{}", observed.id),
                "PUT",
                json!({"name":edit.name}),
            )
        } else {
            if self
                .deployment_patterns(repo, environment, cancel)?
                .iter()
                .any(|pattern| {
                    pattern.name == edit.name
                        && pattern.pattern_type.as_deref().unwrap_or("branch") == kind
                })
            {
                return Err(error(GithubErrorKind::InvalidTarget,"This deployment pattern already exists; inspect it rather than creating a duplicate"));
            }
            (
                prefix.clone(),
                "POST",
                json!({"name":edit.name,"type":kind}),
            )
        };
        let resource = format!("{}/{prefix}", self.host);
        let value = self.api(
            &endpoint,
            method,
            Some(payload),
            Some(resource.clone()),
            false,
            cancel,
        )?;
        let created = pattern_from_json(&value)
            .map_err(|error| self.actions_ambiguous(&resource, &error.message))?;
        if edit
            .observed
            .as_ref()
            .is_some_and(|observed| observed.id != created.id)
        {
            return Err(self.actions_ambiguous(
                &resource,
                "Deployment pattern response did not preserve the edited ID",
            ));
        }
        let fresh = pattern_from_json(&self.after_write(
            &resource,
            self.api(
                &format!("{prefix}/{}", created.id),
                "GET",
                None,
                None,
                false,
                cancel,
            ),
        )?)
        .map_err(|error| self.actions_ambiguous(&resource, &error.message))?;
        if fresh.name != edit.name
            || fresh
                .pattern_type
                .as_deref()
                .is_some_and(|value| value != kind)
        {
            return Err(self.actions_ambiguous(
                &resource,
                "Deployment pattern readback differs from the confirmed effect",
            ));
        }
        Ok(fresh)
    }
    pub fn delete_deployment_pattern(
        &self,
        identity: &HostIdentity,
        repo: &str,
        environment: &str,
        observed: &DeploymentPattern,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<DeploymentPattern>> {
        self.actions_identity(identity, cancel)?;
        let prefix = format!(
            "{}/deployment-branch-policies",
            environment_prefix(repo, environment)?
        );
        if pattern_from_json(&self.api(
            &format!("{prefix}/{}", observed.id),
            "GET",
            None,
            None,
            false,
            cancel,
        )?)? != *observed
        {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Deployment pattern changed; refresh before deletion",
            ));
        }
        let resource = format!("{}/{prefix}", self.host);
        self.api(
            &format!("{prefix}/{}", observed.id),
            "DELETE",
            None,
            Some(resource.clone()),
            false,
            cancel,
        )?;
        let fresh = self.after_write(
            &resource,
            self.deployment_patterns(repo, environment, cancel),
        )?;
        if fresh.iter().any(|pattern| pattern.id == observed.id) {
            return Err(self.actions_ambiguous(
                &resource,
                "Deployment pattern is still visible after deletion; reconcile",
            ));
        }
        Ok(fresh)
    }
    pub fn custom_protection_rules(
        &self,
        repo: &str,
        name: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<CustomProtectionRule>> {
        self.api_list(
            &format!(
                "{}/deployment_protection_rules?per_page=100",
                environment_prefix(repo, name)?
            ),
            Some("custom_deployment_protection_rules"),
            cancel,
        )?
        .iter()
        .map(custom_from_json)
        .collect()
    }
    pub fn custom_protection_apps(
        &self,
        repo: &str,
        name: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<Value>> {
        let value = self.api(
            &format!(
                "{}/deployment_protection_rules/apps?per_page=100",
                environment_prefix(repo, name)?
            ),
            "GET",
            None,
            None,
            true,
            cancel,
        )?;
        let pages = value.as_array().ok_or_else(|| {
            error(
                GithubErrorKind::Json,
                "Custom protection integration pages are missing",
            )
        })?;
        let mut apps = Vec::new();
        let mut seen = BTreeSet::new();
        let mut recognized = false;
        for page in pages {
            let parts = if let Some(parts) = page.as_array() {
                parts.clone()
            } else {
                vec![page.clone()]
            };
            for part in parts {
                if let Some(integrations) =
                    part["available_custom_deployment_protection_rule_integrations"].as_array()
                {
                    recognized = true;
                    for app in integrations {
                        let id = number(app, "id");
                        if id == 0 {
                            return Err(error(
                                GithubErrorKind::Json,
                                "Custom protection integration has no App ID",
                            ));
                        }
                        if seen.insert(id) {
                            apps.push(app.clone());
                        }
                    }
                }
            }
        }
        if !recognized {
            return Err(error(GithubErrorKind::Unsupported, "This host did not expose the custom protection integration list; no empty/available state is inferred"));
        }
        Ok(apps)
    }
    pub fn enable_custom_protection(
        &self,
        identity: &HostIdentity,
        repo: &str,
        environment: &str,
        app_id: u64,
        cancel: &CancellationToken,
    ) -> GithubResult<CustomProtectionRule> {
        if app_id == 0 {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Select an installed eligible deployment protection App",
            ));
        }
        self.actions_identity(identity, cancel)?;
        if !self
            .custom_protection_apps(repo, environment, cancel)?
            .iter()
            .any(|app| number(app, "id") == app_id)
        {
            return Err(error(GithubErrorKind::Permission,"The selected App is not an eligible installed protection integration in this environment"));
        }
        if self
            .custom_protection_rules(repo, environment, cancel)?
            .iter()
            .any(|rule| number(&rule.app, "id") == app_id && rule.enabled)
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "This App protection rule is already enabled",
            ));
        }
        let endpoint = format!(
            "{}/deployment_protection_rules",
            environment_prefix(repo, environment)?
        );
        let resource = format!("{}/{endpoint}", self.host);
        let value = self.api(
            &endpoint,
            "POST",
            Some(json!({"integration_id":app_id})),
            Some(resource.clone()),
            false,
            cancel,
        )?;
        let rule = custom_from_json(&value)
            .map_err(|error| self.actions_ambiguous(&resource, &error.message))?;
        let fresh = custom_from_json(&self.after_write(
            &resource,
            self.api(
                &format!("{endpoint}/{}", rule.id),
                "GET",
                None,
                None,
                false,
                cancel,
            ),
        )?)
        .map_err(|error| self.actions_ambiguous(&resource, &error.message))?;
        if !fresh.enabled || number(&fresh.app, "id") != app_id {
            return Err(self.actions_ambiguous(
                &resource,
                "Custom protection App/rule state was not confirmed",
            ));
        }
        Ok(fresh)
    }
    pub fn disable_custom_protection(
        &self,
        identity: &HostIdentity,
        repo: &str,
        environment: &str,
        observed: &CustomProtectionRule,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<CustomProtectionRule>> {
        self.actions_identity(identity, cancel)?;
        let endpoint = format!(
            "{}/deployment_protection_rules",
            environment_prefix(repo, environment)?
        );
        if custom_from_json(&self.api(
            &format!("{endpoint}/{}", observed.id),
            "GET",
            None,
            None,
            false,
            cancel,
        )?)? != *observed
        {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Custom protection rule changed; refresh before disabling",
            ));
        }
        let resource = format!("{}/{endpoint}", self.host);
        self.api(
            &format!("{endpoint}/{}", observed.id),
            "DELETE",
            None,
            Some(resource.clone()),
            false,
            cancel,
        )?;
        let rules = self.after_write(
            &resource,
            self.custom_protection_rules(repo, environment, cancel),
        )?;
        if rules
            .iter()
            .any(|rule| rule.id == observed.id && rule.enabled)
        {
            return Err(self.actions_ambiguous(
                &resource,
                "Custom protection rule remains enabled; reconcile",
            ));
        }
        Ok(rules)
    }
    pub fn hosted_runners(
        &self,
        org: &str,
        group: Option<u64>,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<HostedRunner>> {
        validate_name(org)?;
        let endpoint = match group {
            Some(id) if id > 0 => {
                format!("orgs/{org}/actions/runner-groups/{id}/hosted-runners?per_page=100")
            }
            Some(_) => {
                return Err(error(
                    GithubErrorKind::InvalidTarget,
                    "Select a valid runner group",
                ))
            }
            None => format!("orgs/{org}/actions/hosted-runners?per_page=100"),
        };
        let runners = self
            .api_list(&endpoint, Some("runners"), cancel)?
            .iter()
            .map(|v| HostedRunner {
                id: number(v, "id"),
                name: text(v, "name"),
                group_id: number(v, "runner_group_id"),
                platform: text(v, "platform"),
                status: text(v, "status"),
                maximum_runners: number(v, "maximum_runners"),
                image: v["image"].clone(),
                machine_size: v["machine_size_details"].clone(),
                last_active_on: text(v, "last_active_on"),
            })
            .collect::<Vec<_>>();
        if runners
            .iter()
            .any(|runner| runner.id == 0 || group.is_some_and(|group| runner.group_id != group))
        {
            return Err(error(
                GithubErrorKind::Json,
                "Hosted runner does not belong to the requested group or has no ID",
            ));
        }
        Ok(runners)
    }
    pub fn create_runner_group(
        &self,
        identity: &HostIdentity,
        org: &str,
        request: &CreateRunnerGroup,
        cancel: &CancellationToken,
    ) -> GithubResult<RunnerGroup> {
        validate_name(org)?;
        nonempty_parameter(&request.settings.name)?;
        distinct_ids(&request.repository_ids)?;
        distinct_ids(&request.runner_ids)?;
        if request.settings.visibility != ScopeVisibility::Selected
            && !request.repository_ids.is_empty()
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Explicit repository membership requires selected runner group visibility",
            ));
        }
        self.actions_identity(identity, cancel)?;
        if self
            .runner_groups(org, cancel)?
            .iter()
            .any(|group| group.name == request.settings.name)
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "A runner group with this name exists; inspect it before creating another",
            ));
        }
        let endpoint = format!("orgs/{org}/actions/runner-groups");
        let resource = format!("{}/{endpoint}", self.host);
        let settings = &request.settings;
        let value=self.api(&endpoint,"POST",Some(json!({"name":settings.name,"visibility":settings.visibility,"allows_public_repositories":settings.allows_public_repositories,"restricted_to_workflows":settings.restricted_to_workflows,"selected_workflows":settings.selected_workflows,"selected_repository_ids":request.repository_ids,"runners":request.runner_ids})),Some(resource.clone()),false,cancel)?;
        let id = number(&value, "id");
        if id == 0 {
            return Err(self.actions_ambiguous(
                &resource,
                "Runner group creation did not return its canonical ID",
            ));
        }
        let group = self.after_write(&resource, self.runner_group(org, id, cancel))?;
        let visibility = match settings.visibility {
            ScopeVisibility::All => "all",
            ScopeVisibility::Private => "private",
            ScopeVisibility::Selected => "selected",
        };
        if group.name != settings.name
            || group.inherited
            || group.visibility != visibility
            || group.allows_public_repositories != settings.allows_public_repositories
            || group.restricted_to_workflows != settings.restricted_to_workflows
            || settings.restricted_to_workflows
                && group.selected_workflows != settings.selected_workflows
        {
            return Err(self.actions_ambiguous(
                &resource,
                "New runner group identity/settings were not confirmed",
            ));
        }
        for (kind, expected) in [
            (GroupAssociation::Runners, &request.runner_ids),
            (GroupAssociation::Repositories, &request.repository_ids),
        ] {
            if kind == GroupAssociation::Repositories
                && settings.visibility != ScopeVisibility::Selected
            {
                continue;
            }
            let actual = self
                .after_write(&resource, self.group_associations(org, id, kind, cancel))?
                .iter()
                .map(|v| number(v, "id"))
                .collect::<BTreeSet<_>>();
            if actual != expected.iter().copied().collect() {
                return Err(self.actions_ambiguous(&resource,"New runner group memberships were not confirmed; reconcile creation without duplicating it"));
            }
        }
        Ok(group)
    }
    pub fn delete_runner_group(
        &self,
        identity: &HostIdentity,
        org: &str,
        observed: &RunnerGroup,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<RunnerGroup>> {
        if observed.default || observed.inherited {
            return Err(error(
                GithubErrorKind::Permission,
                "Default/inherited runner groups cannot be removed from this scope",
            ));
        }
        self.actions_identity(identity, cancel)?;
        if self.runner_group(org, observed.id, cancel)? != *observed {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Runner group changed; refresh before deleting its associations",
            ));
        }
        let endpoint = format!("orgs/{org}/actions/runner-groups/{}", observed.id);
        let resource = format!("{}/{endpoint}", self.host);
        self.api(
            &endpoint,
            "DELETE",
            None,
            Some(resource.clone()),
            false,
            cancel,
        )?;
        let groups = self.after_write(&resource, self.runner_groups(org, cancel))?;
        if groups.iter().any(|group| group.id == observed.id) {
            return Err(self.actions_ambiguous(
                &resource,
                "Runner group is still visible after deletion; reconcile",
            ));
        }
        Ok(groups)
    }
    pub fn cache_limits(
        &self,
        scope: &ActionsScope,
        cancel: &CancellationToken,
    ) -> GithubResult<CacheLimit> {
        let prefix = match scope {
            ActionsScope::Repository { repository } => {
                validate_repo(repository)?;
                format!("repos/{repository}")
            }
            ActionsScope::Organization { organization } => {
                validate_name(organization)?;
                format!("organizations/{organization}")
            }
            _ => {
                return Err(error(
                    GithubErrorKind::InvalidTarget,
                    "Cache limits belong to a repository/organization, not an environment",
                ))
            }
        };
        let storage = self.api(
            &format!("{prefix}/actions/cache/storage-limit"),
            "GET",
            None,
            None,
            false,
            cancel,
        )?;
        let max = storage["max_cache_size_gb"].as_u64().ok_or_else(|| {
            error(
                GithubErrorKind::Unsupported,
                "This host does not expose cache storage-limit units",
            )
        })?;
        let retention = self.api(
            &format!("{prefix}/actions/cache/retention-limit"),
            "GET",
            None,
            None,
            false,
            cancel,
        )?;
        let days = retention["max_cache_retention_days"]
            .as_u64()
            .ok_or_else(|| {
                error(
                    GithubErrorKind::Unsupported,
                    "This host does not expose cache retention limits",
                )
            })?;
        Ok(CacheLimit {
            max_cache_size_gb: max,
            max_cache_retention_days: Some(days),
        })
    }
    pub fn organization_cache_usage(
        &self,
        org: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<OrganizationCacheUsage> {
        validate_name(org)?;
        let value = self.api(
            &format!("orgs/{org}/actions/cache/usage"),
            "GET",
            None,
            None,
            false,
            cancel,
        )?;
        let repositories = self.api_list(
            &format!("orgs/{org}/actions/cache/usage-by-repository?per_page=100"),
            Some("repository_cache_usages"),
            cancel,
        )?;
        for repo in &repositories {
            validate_repo(&text(repo, "full_name"))?;
        }
        Ok(OrganizationCacheUsage {
            total_active_caches_count: number(&value, "total_active_caches_count"),
            total_active_caches_size_in_bytes: number(&value, "total_active_caches_size_in_bytes"),
            repositories,
        })
    }
    pub fn workflow_usage(
        &self,
        repo: &str,
        run: u64,
        cancel: &CancellationToken,
    ) -> GithubResult<WorkflowUsage> {
        validate_pr(repo, run)?;
        let value=self.api(&format!("repos/{repo}/actions/runs/{run}/timing"),"GET",None,None,false,cancel).map_err(|failure|{if failure.message.contains("404")||failure.message.contains("410"){error(GithubErrorKind::Unsupported,"The workflow usage endpoint is closing down and is unavailable on this host; elapsed job duration is not a substitute for billed cost")}else{failure}})?;
        if !value["billable"].is_object() {
            return Err(error(
                GithubErrorKind::Unsupported,
                "The host did not return workflow usage data; no zero usage is inferred",
            ));
        }
        Ok(WorkflowUsage{billable:value["billable"].clone(),run_duration_ms:number(&value,"run_duration_ms"),limitation:"Legacy workflow usage is closing down; platform values are milliseconds, include reruns, and exclude billing multipliers/rounding. Elapsed job time is not billed cost.".into()})
    }
    pub fn edit_setting_memberships(
        &self,
        identity: &HostIdentity,
        scope: &ActionsScope,
        edit: &SettingMembershipEdit,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<Repository>> {
        setting_name(&edit.name)?;
        if !matches!(scope, ActionsScope::Organization { .. }) {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Selected repository membership belongs to organization secrets/variables",
            ));
        }
        distinct_ids(&edit.selected_ids)?;
        self.actions_identity(identity, cancel)?;
        let endpoint = format!(
            "{}/{}/{}",
            scope.prefix()?,
            if edit.secret { "secrets" } else { "variables" },
            edit.name
        );
        let metadata = self.api(&endpoint, "GET", None, None, false, cancel)?;
        if text(&metadata, "name") != edit.name || text(&metadata, "visibility") != "selected" {
            return Err(error(GithubErrorKind::InvalidTarget,"This setting does not have selected visibility; secret visibility changes require a new masked value because existing values cannot be read"));
        }
        let current = self.selected_setting_repositories(scope, &edit.name, edit.secret, cancel)?;
        if current.iter().map(|repo| repo.id).collect::<BTreeSet<_>>()
            != edit.observed_ids.iter().copied().collect()
        {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Selected repository memberships changed; refresh before replacing them",
            ));
        }
        let endpoint = format!("{endpoint}/repositories");
        let resource = format!("{}/{endpoint}", self.host);
        self.api(
            &endpoint,
            "PUT",
            Some(json!({"selected_repository_ids":edit.selected_ids})),
            Some(resource.clone()),
            false,
            cancel,
        )?;
        let fresh = self.after_write(
            &resource,
            self.selected_setting_repositories(scope, &edit.name, edit.secret, cancel),
        )?;
        if fresh.iter().map(|repo| repo.id).collect::<BTreeSet<_>>()
            != edit.selected_ids.iter().copied().collect()
        {
            return Err(self.actions_ambiguous(&resource,"Selected setting repositories were not confirmed; reconcile without rewriting a secret value"));
        }
        Ok(fresh)
    }
}
