//! Factored canonical Actions namespace; no subprocess, runner or database access.
//! Staged separately until coordinated provider/journal/snapshot activation.
use super::*;

pub(crate) fn canonical_actions_host(host: &str) -> GithubResult<String> {
    let host = host.to_ascii_lowercase();
    if host.is_empty()
        || !host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        || host.starts_with('-')
    {
        return Err(error(
            GithubErrorKind::InvalidTarget,
            "Invalid GitHub hostname",
        ));
    }
    Ok(host)
}

pub(crate) fn canonical_github_resource(host: &str, resource: &str) -> String {
    let mut parts = resource.split('/').map(str::to_owned).collect::<Vec<_>>();
    if parts
        .first()
        .is_none_or(|resource_host| !resource_host.eq_ignore_ascii_case(host))
    {
        return resource.into();
    }
    parts[0] = host.to_owned();
    let identity_segments: &[usize] = match parts.get(1).map(String::as_str) {
        Some("repos") => &[2, 3],
        Some("orgs") => &[2],
        _ => &[1, 2],
    };
    for index in identity_segments {
        if let Some(segment) = parts.get_mut(*index) {
            segment.make_ascii_lowercase();
        }
    }
    // Only scoped settings namespaces have case-insensitive environment/setting
    // names; never normalize a similarly named branch inside a PR creation key.
    let settings_start = match parts.get(1).map(String::as_str) {
        Some("repos") if parts.get(4).is_some_and(|part| part == "environments") => {
            if let Some(environment) = parts.get_mut(5) {
                environment.make_ascii_lowercase();
            }
            Some(6)
        }
        Some("repos") if parts.get(4).is_some_and(|part| part == "actions") => Some(5),
        Some("orgs") if parts.get(3).is_some_and(|part| part == "actions") => Some(4),
        _ => None,
    };
    if let Some(index) = settings_start {
        if parts.get(index).is_some_and(|kind| {
            matches!(
                kind.as_str(),
                "secret" | "secrets" | "variable" | "variables"
            )
        }) {
            if let Some(name) = parts.get_mut(index + 1) {
                name.make_ascii_uppercase();
            }
        }
    }
    parts.join("/")
}

pub fn canonical_actions_resource_keys(
    host: &str,
    target: &ActionsTarget,
) -> GithubResult<Vec<String>> {
    let host = canonical_actions_host(host)?;
    match target {
        ActionsTarget::Run { repository, id }
        | ActionsTarget::Workflow { repository, id }
        | ActionsTarget::Artifact { repository, id }
        | ActionsTarget::Cache { repository, id } => validate_pr(repository, *id)?,
        ActionsTarget::Runner { scope, id } => {
            scope.prefix()?;
            if *id == 0 || matches!(scope, ActionsScope::Environment { .. }) {
                return Err(error(
                    GithubErrorKind::InvalidTarget,
                    "Select a positive runner in a repository or organization scope",
                ));
            }
        }
        ActionsTarget::RunnerGroup { organization, id }
        | ActionsTarget::GroupAssociations {
            organization, id, ..
        } => {
            validate_name(organization)?;
            if *id == 0 {
                return Err(error(
                    GithubErrorKind::InvalidTarget,
                    "Select a positive runner-group ID",
                ));
            }
        }
        ActionsTarget::RunnerGroupCreation { organization, name } => {
            validate_name(organization)?;
            nonempty_parameter(name)?;
        }
        ActionsTarget::Environment { repository, name } => {
            validate_repo(repository)?;
            nonempty_parameter(name)?;
        }
        ActionsTarget::DeploymentPatterns {
            repository,
            environment,
        }
        | ActionsTarget::CustomProtection {
            repository,
            environment,
        } => {
            validate_repo(repository)?;
            nonempty_parameter(environment)?;
        }
        ActionsTarget::Secret { scope, name }
        | ActionsTarget::Variable { scope, name }
        | ActionsTarget::SettingMemberships { scope, name, .. } => {
            scope.prefix()?;
            setting_name(name)?;
        }
        ActionsTarget::DeploymentReview { .. }
        | ActionsTarget::EnvironmentProtection { .. }
        | ActionsTarget::Policy { .. } => {}
    }
    observation_resource_keys(&host, target)
}
fn observation_resource_keys(host: &str, target: &ActionsTarget) -> GithubResult<Vec<String>> {
    let key = match target {
        ActionsTarget::Run { repository, id } => format!("{}/{repository}/run/{id}", host),
        ActionsTarget::DeploymentReview {
            repository,
            run_id,
            attempt,
            head_sha,
            environment_ids,
        } => {
            validate_pr(repository, *run_id)?;
            validate_sha(head_sha)?;
            if *attempt == 0
                || environment_ids.is_empty()
                || environment_ids.contains(&0)
                || environment_ids.iter().collect::<BTreeSet<_>>().len() != environment_ids.len()
            {
                return Err(error(
                    GithubErrorKind::InvalidTarget,
                    "Inspect distinct positive environment IDs and an exact run attempt/head",
                ));
            }
            format!("{}/{repository}/run/{run_id}", host)
        }
        ActionsTarget::Workflow { repository, id } => {
            format!("{}/{repository}/workflow/{id}", host)
        }
        ActionsTarget::Artifact { repository, id } => {
            format!("{}/{repository}/artifact/{id}", host)
        }
        ActionsTarget::Cache { repository, id } => {
            format!("{}/{repository}/cache/{id}", host)
        }
        ActionsTarget::Policy { scope, section } => {
            let suffix = match section {
                PolicySection::Permissions => "permissions",
                PolicySection::SelectedActions => "permissions/selected-actions",
                PolicySection::WorkflowToken => "permissions/workflow",
                PolicySection::ForkWorkflows => "permissions/fork-pr-workflows-private-repos",
                PolicySection::ForkApproval => "permissions/fork-pr-contributor-approval",
                PolicySection::Retention => "permissions/artifact-and-log-retention",
            };
            format!("{}/{}/{suffix}", host, scope.prefix()?)
        }
        ActionsTarget::Environment { repository, name } => format!(
            "{}/repos/{repository}/environments/{}",
            host,
            url_component(name)
        ),
        ActionsTarget::EnvironmentProtection {
            repository,
            name,
            environment_id,
        } => {
            validate_repo(repository)?;
            nonempty_parameter(name)?;
            if *environment_id == 0 {
                return Err(error(
                    GithubErrorKind::InvalidTarget,
                    "Inspect the original positive environment ID, not only a reused name",
                ));
            }
            format!(
                "{}/repos/{repository}/environments/{}",
                host,
                url_component(name)
            )
        }
        ActionsTarget::Runner { scope, id } => {
            let base = format!("{}/{}/runners/{id}", host, scope.prefix()?);
            return Ok(vec![
                canonical_github_resource(host, &base),
                canonical_github_resource(host, &format!("{base}/labels")),
            ]);
        }
        ActionsTarget::RunnerGroup { organization, id } => {
            format!("{}/orgs/{organization}/actions/runner-groups/{id}", host)
        }
        ActionsTarget::RunnerGroupCreation { organization, .. } => {
            format!("{}/orgs/{organization}/actions/runner-groups", host)
        }
        ActionsTarget::GroupAssociations {
            organization,
            id,
            kind,
        } => format!(
            "{}/orgs/{organization}/actions/runner-groups/{id}/{}",
            host,
            if *kind == GroupAssociation::Runners {
                "runners"
            } else {
                "repositories"
            }
        ),
        ActionsTarget::DeploymentPatterns {
            repository,
            environment,
        } => format!(
            "{}/repos/{repository}/environments/{}/deployment-branch-policies",
            host,
            url_component(environment)
        ),
        ActionsTarget::CustomProtection {
            repository,
            environment,
        } => format!(
            "{}/repos/{repository}/environments/{}/deployment_protection_rules",
            host,
            url_component(environment)
        ),
        ActionsTarget::Secret { scope, name } => {
            format!("{}/{}/secret/{name}", host, scope.prefix()?)
        }
        ActionsTarget::Variable { scope, name } => {
            format!("{}/{}/variable/{name}", host, scope.prefix()?)
        }
        ActionsTarget::SettingMemberships {
            scope,
            name,
            secret,
        } => format!(
            "{}/{}/{}/{name}/repositories",
            host,
            scope.prefix()?,
            if *secret { "secrets" } else { "variables" }
        ),
    };
    Ok(vec![canonical_github_resource(host, &key)])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actions_factored_namespace_preserves_foreign_host_and_case_sensitive_nonsettings_suffixes() {
        assert_eq!(
            canonical_github_resource("github.invalid", "other.invalid/EXAMPLE/Repo/run/9"),
            "other.invalid/EXAMPLE/Repo/run/9"
        );
        assert_eq!(
            canonical_github_resource(
                "github.invalid",
                "GITHUB.INVALID/Example/Repo/pr-create/refs/heads/Secret/MAIN"
            ),
            "github.invalid/example/repo/pr-create/refs/heads/Secret/MAIN"
        );
        assert_eq!(
            canonical_github_resource(
                "github.invalid",
                "GITHUB.INVALID/repos/Example/Repo/environments/Prod%2FEU/secret/deploy_token"
            ),
            "github.invalid/repos/example/repo/environments/prod%2feu/secret/DEPLOY_TOKEN"
        );
    }
    #[test]
    fn actions_factored_namespace_matches_provider_exact_aliases_without_io() {
        let provider = GithubProvider::new("github.invalid").unwrap();
        let expected: [&[&str]; 5] = [
            &["github.invalid/example/repo/run/123"],
            &[
                "github.invalid/orgs/example/actions/runners/9",
                "github.invalid/orgs/example/actions/runners/9/labels",
            ],
            &["github.invalid/repos/example/repo/environments/prod%2feu/secret/DEPLOY_TOKEN"],
            &["github.invalid/orgs/example/actions/runner-groups/9/repositories"],
            &["github.invalid/example/repo/run/123"],
        ];
        for (index, target) in [
            ActionsTarget::Run {
                repository: "EXAMPLE/Repo".into(),
                id: 123,
            },
            ActionsTarget::Runner {
                scope: ActionsScope::Organization {
                    organization: "Example".into(),
                },
                id: 9,
            },
            ActionsTarget::Secret {
                scope: ActionsScope::Environment {
                    repository: "Example/Repo".into(),
                    environment: "Prod/EU".into(),
                },
                name: "deploy_token".into(),
            },
            ActionsTarget::GroupAssociations {
                organization: "Example".into(),
                id: 9,
                kind: GroupAssociation::Repositories,
            },
            ActionsTarget::DeploymentReview {
                repository: "Example/Repo".into(),
                run_id: 123,
                attempt: 2,
                head_sha: "a".repeat(40),
                environment_ids: vec![9],
            },
        ]
        .into_iter()
        .enumerate()
        {
            let keys = canonical_actions_resource_keys("GITHUB.INVALID", &target).unwrap();
            assert_eq!(
                keys.iter().map(String::as_str).collect::<Vec<_>>(),
                expected[index]
            );
            assert_eq!(keys, provider.canonical_actions_resources(&target).unwrap());
        }
        for invalid_host in ["", "-invalid", "github.invalid/path", "github.invalid\n"] {
            assert!(canonical_actions_resource_keys(
                invalid_host,
                &ActionsTarget::Run {
                    repository: "example/repo".into(),
                    id: 123
                }
            )
            .is_err());
        }
        assert!(canonical_actions_resource_keys(
            "github.invalid",
            &ActionsTarget::Run {
                repository: "example/repo".into(),
                id: 0
            }
        )
        .is_err());
    }
}
