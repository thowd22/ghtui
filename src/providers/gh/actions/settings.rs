//! Scoped configuration; existing secret values are deliberately not part of any model.
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ActionsScope {
    Repository {
        repository: String,
    },
    Environment {
        repository: String,
        environment: String,
    },
    Organization {
        organization: String,
    },
}
impl ActionsScope {
    pub(super) fn prefix(&self) -> GithubResult<String> {
        match self {
            Self::Repository { repository } => {
                validate_repo(repository)?;
                Ok(format!("repos/{repository}/actions"))
            }
            Self::Environment {
                repository,
                environment,
            } => {
                validate_repo(repository)?;
                nonempty_parameter(environment)?;
                Ok(format!(
                    "repos/{repository}/environments/{}",
                    url_component(environment)
                ))
            }
            Self::Organization { organization } => {
                validate_name(organization)?;
                Ok(format!("orgs/{organization}/actions"))
            }
        }
    }
    fn gh_args(&self, host: &str) -> GithubResult<Vec<String>> {
        self.prefix()?;
        Ok(match self {
            Self::Repository { repository } => {
                vec!["--repo".into(), format!("{host}/{repository}")]
            }
            Self::Environment {
                repository,
                environment,
            } => vec![
                "--repo".into(),
                format!("{host}/{repository}"),
                "--env".into(),
                environment.clone(),
            ],
            Self::Organization { organization } => vec!["--org".into(), organization.clone()],
        })
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ScopeVisibility {
    All,
    Private,
    Selected,
}
impl ScopeVisibility {
    fn label(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Private => "private",
            Self::Selected => "selected",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SecretPolicy {
    pub visibility: ScopeVisibility,
    pub selected_repositories: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SecretUpdate {
    pub name: String,
    pub observed: Option<SecretMetadata>,
    pub policy: Option<SecretPolicy>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecretMetadata {
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub visibility: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActionsVariable {
    pub name: String,
    pub value: String,
    pub created_at: String,
    pub updated_at: String,
    pub visibility: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VariableEdit {
    pub name: String,
    pub value: String,
    pub visibility: Option<ScopeVisibility>,
    pub selected_repository_ids: Option<Vec<u64>>,
}
pub(super) fn setting_name(name: &str) -> GithubResult<()> {
    if name.is_empty()
        || name.len() > 128
        || name.to_ascii_uppercase().starts_with("GITHUB_")
        || name.as_bytes()[0].is_ascii_digit()
        || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err(error(GithubErrorKind::InvalidTarget,"Secret/variable names must use letters, numbers or underscores, may not begin with a digit, and may not use the GITHUB_ prefix"));
    }
    Ok(())
}
fn secret_from_json(v: &Value) -> SecretMetadata {
    SecretMetadata {
        name: text(v, "name"),
        created_at: text(v, "created_at"),
        updated_at: text(v, "updated_at"),
        visibility: v["visibility"].as_str().map(str::to_owned),
    }
}
fn variable_from_json(v: &Value) -> ActionsVariable {
    ActionsVariable {
        name: text(v, "name"),
        value: text(v, "value"),
        created_at: text(v, "created_at"),
        updated_at: text(v, "updated_at"),
        visibility: v["visibility"].as_str().map(str::to_owned),
    }
}

impl GithubProvider {
    fn scope_resource(&self, scope: &ActionsScope, kind: &str, name: &str) -> GithubResult<String> {
        Ok(self.canonical_resource(&format!("{}/{}/{kind}/{name}", self.host, scope.prefix()?)))
    }
    pub fn secret_metadata(
        &self,
        scope: &ActionsScope,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<SecretMetadata>> {
        Ok(self
            .api_list(
                &format!("{}/secrets?per_page=100", scope.prefix()?),
                Some("secrets"),
                cancel,
            )?
            .iter()
            .map(secret_from_json)
            .collect())
    }
    pub fn variables(
        &self,
        scope: &ActionsScope,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<ActionsVariable>> {
        Ok(self
            .api_list(
                &format!("{}/variables?per_page=100", scope.prefix()?),
                Some("variables"),
                cancel,
            )?
            .iter()
            .map(variable_from_json)
            .collect())
    }
    pub fn variable(
        &self,
        scope: &ActionsScope,
        name: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<ActionsVariable> {
        setting_name(name)?;
        let v = self.api(
            &format!("{}/variables/{name}", scope.prefix()?),
            "GET",
            None,
            None,
            false,
            cancel,
        )?;
        let variable = variable_from_json(&v);
        if !variable.name.eq_ignore_ascii_case(name) {
            return Err(error(
                GithubErrorKind::Json,
                "Variable metadata returned the wrong name",
            ));
        }
        Ok(variable)
    }
    /// The caller supplies a transient masked value; this operation never returns or stores it.
    pub fn set_secret(
        &self,
        identity: &HostIdentity,
        scope: &ActionsScope,
        request: &SecretUpdate,
        value: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<SecretMetadata> {
        let name = request.name.as_str();
        let policy = request.policy.as_ref();
        setting_name(name)?;
        if value.is_empty() {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Enter a nonempty secret value; existing values are not retrievable",
            ));
        }
        let mut args = vec![
            "secret".into(),
            "set".into(),
            name.into(),
            "--app".into(),
            "actions".into(),
        ];
        args.extend(scope.gh_args(&self.host)?);
        match (scope, policy) {
            (ActionsScope::Organization { .. }, Some(policy)) => {
                args.extend(["--visibility".into(), policy.visibility.label().into()]);
                if policy.visibility == ScopeVisibility::Selected {
                    if policy.selected_repositories.is_empty() {
                        args.push("--no-repos-selected".into());
                    } else {
                        for repo in &policy.selected_repositories {
                            validate_name(repo)?;
                        }
                        args.extend(["--repos".into(), policy.selected_repositories.join(",")]);
                    }
                } else if !policy.selected_repositories.is_empty() {
                    return Err(error(
                        GithubErrorKind::InvalidTarget,
                        "Selected repository membership requires selected visibility",
                    ));
                }
            }
            (ActionsScope::Organization { .. }, None) => {
                return Err(error(
                    GithubErrorKind::InvalidTarget,
                    "Organization secrets need an explicit visibility policy",
                ))
            }
            (_, Some(_)) => {
                return Err(error(
                    GithubErrorKind::InvalidTarget,
                    "Repository/environment secrets do not accept organization visibility",
                ))
            }
            (_, None) => {}
        }
        let current_identity = self.actions_current_identity(identity, cancel)?;
        if matches!(scope, ActionsScope::Organization { .. })
            && policy.is_some_and(|policy| {
                policy.visibility == ScopeVisibility::Selected
                    && policy.selected_repositories.is_empty()
            })
        {
            self.require_cli_syntax_v1(&current_identity, CliRequirement::SecretEmptySelection)?;
        }
        let current = self.secret_metadata(scope, cancel)?;
        if current
            .iter()
            .find(|secret| secret.name.eq_ignore_ascii_case(name))
            != request.observed.as_ref()
        {
            return Err(error(GithubErrorKind::StaleHead, "Secret metadata changed since the form was opened; refresh before replacing its unreadable value"));
        }
        let resource = self.scope_resource(scope, "secret", name)?;
        let mut spec = self
            .spec(args)
            .input(value.as_bytes().to_vec())
            .write(resource.clone());
        spec.sensitive_values.push(value.into());
        self.runner.run(spec, cancel).map_err(GithubError::from)?;
        let metadata=self.after_write(&resource,self.secret_metadata(scope,cancel))?.into_iter().find(|s|s.name.eq_ignore_ascii_case(name)).ok_or_else(||self.actions_ambiguous(&resource,"Secret metadata did not confirm the selected name; reconcile without repeating the secret submission"))?;
        if let Some(policy) = policy {
            if metadata.visibility.as_deref() != Some(policy.visibility.label()) {
                return Err(self.actions_ambiguous(
                    &resource,
                    "Secret visibility was not confirmed; inspect the scope policy before retrying",
                ));
            }
            if policy.visibility == ScopeVisibility::Selected {
                let repositories = self.after_write(
                    &resource,
                    self.selected_setting_repositories(scope, name, true, cancel),
                )?;
                let actual = repositories
                    .iter()
                    .filter_map(|repo| {
                        repo.full_name
                            .split_once('/')
                            .map(|(_, name)| name.to_lowercase())
                    })
                    .collect::<BTreeSet<_>>();
                let expected = policy
                    .selected_repositories
                    .iter()
                    .map(|name| name.to_lowercase())
                    .collect::<BTreeSet<_>>();
                if actual != expected {
                    return Err(self.actions_ambiguous(&resource, "Secret selected-repository membership was not confirmed; reconcile its policy before retrying"));
                }
            }
        }
        Ok(metadata)
    }
    pub fn delete_secret(
        &self,
        identity: &HostIdentity,
        scope: &ActionsScope,
        target: &SecretMetadata,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<SecretMetadata>> {
        setting_name(&target.name)?;
        self.actions_identity(identity, cancel)?;
        let current = self.secret_metadata(scope, cancel)?;
        if current.iter().find(|s| s.name == target.name) != Some(target) {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Selected secret metadata changed or disappeared; refresh before deletion",
            ));
        }
        let resource = self.scope_resource(scope, "secret", &target.name)?;
        self.api(
            &format!("{}/secrets/{}", scope.prefix()?, target.name),
            "DELETE",
            None,
            Some(resource.clone()),
            false,
            cancel,
        )?;
        let remaining = self.after_write(&resource, self.secret_metadata(scope, cancel))?;
        if remaining.iter().any(|s| s.name == target.name) {
            return Err(self.actions_ambiguous(
                &resource,
                "Secret metadata is still visible after deletion; reconcile",
            ));
        }
        Ok(remaining)
    }
    pub fn set_variable(
        &self,
        identity: &HostIdentity,
        scope: &ActionsScope,
        observed: Option<&ActionsVariable>,
        edit: &VariableEdit,
        cancel: &CancellationToken,
    ) -> GithubResult<ActionsVariable> {
        setting_name(&edit.name)?;
        let prefix = scope.prefix()?;
        let mut payload = json!({"name":edit.name,"value":edit.value});
        match scope {
            ActionsScope::Organization { .. } => {
                let visibility = edit.visibility.ok_or_else(|| {
                    error(
                        GithubErrorKind::InvalidTarget,
                        "Organization variables need an explicit visibility",
                    )
                })?;
                payload["visibility"] = json!(visibility);
                if let Some(ids) = &edit.selected_repository_ids {
                    if visibility != ScopeVisibility::Selected
                        || ids.contains(&0)
                        || ids.iter().collect::<BTreeSet<_>>().len() != ids.len()
                    {
                        return Err(error(
                            GithubErrorKind::InvalidTarget,
                            "Choose distinct valid repository IDs with selected visibility",
                        ));
                    }
                    payload["selected_repository_ids"] = json!(ids);
                }
            }
            _ => {
                if edit.visibility.is_some() || edit.selected_repository_ids.is_some() {
                    return Err(error(
                        GithubErrorKind::InvalidTarget,
                        "Selected-repository policies apply only to organization variables",
                    ));
                }
            }
        }
        self.actions_identity(identity, cancel)?;
        if let Some(observed) = observed {
            if self.variable(scope, &observed.name, cancel)? != *observed {
                return Err(error(
                    GithubErrorKind::StaleHead,
                    "Variable changed since its form was opened; refresh before overwriting",
                ));
            }
        } else if self
            .variables(scope, cancel)?
            .iter()
            .any(|v| v.name.eq_ignore_ascii_case(&edit.name))
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Variable already exists; inspect it before updating",
            ));
        }
        let resource = self.scope_resource(
            scope,
            "variable",
            observed.map(|v| v.name.as_str()).unwrap_or(&edit.name),
        )?;
        let (endpoint, method) = match observed {
            Some(v) => (format!("{prefix}/variables/{}", v.name), "PATCH"),
            None => (format!("{prefix}/variables"), "POST"),
        };
        self.api(
            &endpoint,
            method,
            Some(payload),
            Some(resource.clone()),
            false,
            cancel,
        )?;
        let value = self.after_write(&resource, self.variable(scope, &edit.name, cancel))?;
        if value.value != edit.value
            || edit
                .visibility
                .is_some_and(|visibility| value.visibility.as_deref() != Some(visibility.label()))
        {
            return Err(self.actions_ambiguous(&resource,"Variable value/visibility was not confirmed after writing; reconcile before retrying"));
        }
        Ok(value)
    }
    pub fn delete_variable(
        &self,
        identity: &HostIdentity,
        scope: &ActionsScope,
        target: &ActionsVariable,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<ActionsVariable>> {
        self.actions_identity(identity, cancel)?;
        if self.variable(scope, &target.name, cancel)? != *target {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Variable changed or disappeared; refresh before deletion",
            ));
        }
        let resource = self.scope_resource(scope, "variable", &target.name)?;
        self.api(
            &format!("{}/variables/{}", scope.prefix()?, target.name),
            "DELETE",
            None,
            Some(resource.clone()),
            false,
            cancel,
        )?;
        let remaining = self.after_write(&resource, self.variables(scope, cancel))?;
        if remaining.iter().any(|v| v.name == target.name) {
            return Err(self.actions_ambiguous(
                &resource,
                "Variable is still visible after deletion; reconcile",
            ));
        }
        Ok(remaining)
    }
    pub fn selected_setting_repositories(
        &self,
        scope: &ActionsScope,
        name: &str,
        secret: bool,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<Repository>> {
        setting_name(name)?;
        if !matches!(scope, ActionsScope::Organization { .. }) {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Selected repositories are an organization-level policy",
            ));
        }
        self.api_list(
            &format!(
                "{}/{}/{name}/repositories?per_page=100",
                scope.prefix()?,
                if secret { "secrets" } else { "variables" }
            ),
            Some("repositories"),
            cancel,
        )?
        .iter()
        .map(|v| self.repo_from_json(v))
        .collect()
    }
    pub fn reconcile_actions_scope(
        &self,
        scope: &ActionsScope,
        kind: &str,
        name: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        match kind {
            "secret" => {
                self.secret_metadata(scope, cancel)?;
            }
            "variable" => {
                self.variables(scope, cancel)?;
            }
            _ => {
                return Err(error(
                    GithubErrorKind::InvalidTarget,
                    "Only a named secret or variable scope can be reconciled here",
                ))
            }
        };
        self.runner
            .mark_reconciled(&self.scope_resource(scope, kind, name)?);
        Ok(())
    }
}
