//! Pure canonical current-selection targets for worker-prepared publication controls.
//! This file is staged separately until the ledger/controller activation handshake.
use super::*;
use crate::actions_write_journal::ActionsWriteKind;

impl ActionsCommand {
    /// Closed command metadata for worker-side resource-specific lease decoration.
    /// This is not a publication permit or a substitute for private plan binding.
    pub fn publication_kind(self) -> Option<ActionsWriteKind> {
        use ActionsCommand::*;
        Some(match self {
            RerunAll => ActionsWriteKind::RerunAll,
            RerunFailed => ActionsWriteKind::RerunFailed,
            RerunJob => ActionsWriteKind::RerunJob,
            Cancel => ActionsWriteKind::Cancel,
            ForceCancel => ActionsWriteKind::ForceCancel,
            DeleteRun => ActionsWriteKind::DeleteRun,
            DeleteLogs => ActionsWriteKind::DeleteLogs,
            ApproveFork => ActionsWriteKind::ApproveFork,
            EnableWorkflow => ActionsWriteKind::EnableWorkflow,
            DisableWorkflow => ActionsWriteKind::DisableWorkflow,
            DispatchWorkflow => ActionsWriteKind::DispatchWorkflow,
            DeleteArtifact => ActionsWriteKind::DeleteArtifact,
            ApproveDeployment => ActionsWriteKind::ApproveDeployment,
            RejectDeployment => ActionsWriteKind::RejectDeployment,
            DeleteCache => ActionsWriteKind::DeleteCache,
            SetSecret => ActionsWriteKind::SetSecret,
            DeleteSecret => ActionsWriteKind::DeleteSecret,
            SetVariable => ActionsWriteKind::SetVariable,
            DeleteVariable => ActionsWriteKind::DeleteVariable,
            EditPolicy => ActionsWriteKind::EditPolicy,
            EditEnvironment => ActionsWriteKind::EditEnvironment,
            RunnerLabels => ActionsWriteKind::RunnerLabels,
            RemoveRunner => ActionsWriteKind::RemoveRunner,
            EditRunnerGroup => ActionsWriteKind::EditRunnerGroup,
            GroupRunners => ActionsWriteKind::GroupRunners,
            GroupRepositories => ActionsWriteKind::GroupRepositories,
            CreateRunnerGroup => ActionsWriteKind::CreateRunnerGroup,
            DeleteRunnerGroup => ActionsWriteKind::DeleteRunnerGroup,
            ChooseScope | WorkflowRevision | SearchLogs | ExportLogs | FilterRuns
            | FilterCaches | ExportArtifact | ExtractArtifact => return None,
        })
    }
}

impl ActionsWorkspace {
    /// No display-row parsing, disk/API access, edited form values or global barrier.
    /// New/name-editable operations remain composable; the controller resolves and
    /// guards their actual immutable private prepared target before publication.
    pub fn current_publication_target(&self, command: ActionsCommand) -> Option<ActionsTarget> {
        use ActionsCommand::*;
        let repository = || self.repository().ok().map(str::to_owned);
        match command {
            RerunAll | RerunFailed | RerunJob | Cancel | ForceCancel | DeleteRun | DeleteLogs
            | ApproveFork => {
                let page = self.run.as_ref()?;
                let repository = repository()?;
                if page.attempt.run.id == 0 || !repository.eq_ignore_ascii_case(&page.repository) {
                    return None;
                }
                Some(ActionsTarget::Run {
                    repository: page.repository.clone(),
                    id: page.attempt.run.id,
                })
            }
            ApproveDeployment | RejectDeployment => {
                let page = self.run.as_ref()?;
                let gate = self.selected_deployment()?;
                let repository = repository()?;
                if page.attempt.run.id == 0
                    || page.attempt.run.attempt == 0
                    || gate.environment_id == 0
                    || !repository.eq_ignore_ascii_case(&page.repository)
                {
                    return None;
                }
                Some(ActionsTarget::DeploymentReview {
                    repository: page.repository.clone(),
                    run_id: page.attempt.run.id,
                    attempt: page.attempt.run.attempt,
                    head_sha: page.attempt.run.head_sha.clone(),
                    environment_ids: vec![gate.environment_id],
                })
            }
            EnableWorkflow | DisableWorkflow | DispatchWorkflow => {
                let workflow = &self.definition.as_ref()?.workflow;
                if workflow.id == 0 {
                    return None;
                }
                Some(ActionsTarget::Workflow {
                    repository: repository()?,
                    id: workflow.id,
                })
            }
            DeleteArtifact => {
                let artifact = self.artifact.as_ref()?;
                if artifact.id == 0 {
                    return None;
                }
                Some(ActionsTarget::Artifact {
                    repository: repository()?,
                    id: artifact.id,
                })
            }
            DeleteCache => {
                let cache = self.caches.get(self.selected)?;
                if cache.id == 0 {
                    return None;
                }
                Some(ActionsTarget::Cache {
                    repository: repository()?,
                    id: cache.id,
                })
            }
            DeleteSecret => {
                // Row zero is an explanation, not a secret identity.
                let secret = self.secrets.get(self.selected.checked_sub(1)?)?;
                Some(ActionsTarget::Secret {
                    scope: self.scope.clone(),
                    name: secret.name.clone(),
                })
            }
            DeleteVariable => {
                let variable = self.variables.get(self.selected)?;
                Some(ActionsTarget::Variable {
                    scope: self.scope.clone(),
                    name: variable.name.clone(),
                })
            }
            EditPolicy => {
                let policy = self.policy.as_ref()?;
                if policy.scope != self.scope {
                    return None;
                }
                Some(ActionsTarget::Policy {
                    scope: policy.scope.clone(),
                    section: policy.section,
                })
            }
            EditEnvironment => {
                let environment = self.selected_environment()?;
                let id = environment["id"].as_u64().filter(|id| *id > 0)?;
                let name = environment["name"]
                    .as_str()
                    .filter(|name| !name.is_empty())?;
                Some(ActionsTarget::EnvironmentProtection {
                    repository: repository()?,
                    name: name.into(),
                    environment_id: id,
                })
            }
            RunnerLabels | RemoveRunner => {
                let runner = self.runners.get(self.selected)?;
                if runner.id == 0 {
                    return None;
                }
                Some(ActionsTarget::Runner {
                    scope: self.scope.clone(),
                    id: runner.id,
                })
            }
            EditRunnerGroup | GroupRunners | GroupRepositories | DeleteRunnerGroup => {
                let ActionsScope::Organization { organization } = &self.scope else {
                    return None;
                };
                let group = self.runner_groups.get(self.selected)?;
                if group.id == 0 {
                    return None;
                }
                Some(if matches!(command, GroupRunners | GroupRepositories) {
                    ActionsTarget::GroupAssociations {
                        organization: organization.clone(),
                        id: group.id,
                        kind: if command == GroupRunners {
                            GroupAssociation::Runners
                        } else {
                            GroupAssociation::Repositories
                        },
                    }
                } else {
                    ActionsTarget::RunnerGroup {
                        organization: organization.clone(),
                        id: group.id,
                    }
                })
            }
            SetSecret | SetVariable | CreateRunnerGroup | ChooseScope | WorkflowRevision
            | SearchLogs | ExportLogs | FilterRuns | FilterCaches | ExportArtifact
            | ExtractArtifact => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
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
    #[test]
    fn actions_run_control_publication_target_uses_exact_page_resource_not_another_selected_repository(
    ) {
        let mut workspace = workspace();
        let run:WorkflowRun=serde_json::from_value(json!({"id":123,"workflow_id":7,"name":"CI","title":"fixture","head_sha":"a".repeat(40),"head_branch":"feature","event":"pull_request","status":"waiting","conclusion":"","attempt":2,"actor":{"login":"alice","actor_type":"User"},"triggering_actor":{"login":"octocat","actor_type":"User"},"created_at":"","updated_at":"","url":""})).unwrap();
        workspace.run = Some(RunPage {
            repository: "EXAMPLE/REPO".into(),
            attempt: RunAttempt {
                run,
                jobs: vec![],
                definition_note: String::new(),
            },
            selected_job: None,
            logs: None,
            logs_skipped_bytes: 0,
            annotations: vec![],
        });
        assert!(
            matches!(workspace.current_publication_target(ActionsCommand::Cancel),Some(ActionsTarget::Run {repository,id:123}) if repository == "EXAMPLE/REPO")
        );
        workspace.repositories[0] = "another/repo".into();
        assert!(workspace
            .current_publication_target(ActionsCommand::Cancel)
            .is_none());
    }
    #[test]
    fn actions_secrets_vars_publication_target_keeps_new_names_open_and_explanation_rows_unselected(
    ) {
        assert_eq!(
            ActionsCommand::SetSecret.publication_kind(),
            Some(ActionsWriteKind::SetSecret)
        );
        assert_eq!(
            ActionsCommand::ApproveDeployment.publication_kind(),
            Some(ActionsWriteKind::ApproveDeployment)
        );
        assert_eq!(
            ActionsCommand::Cancel.publication_kind(),
            Some(ActionsWriteKind::Cancel)
        );
        for local in [
            ActionsCommand::ChooseScope,
            ActionsCommand::WorkflowRevision,
            ActionsCommand::SearchLogs,
            ActionsCommand::ExportLogs,
            ActionsCommand::FilterRuns,
            ActionsCommand::FilterCaches,
            ActionsCommand::ExportArtifact,
            ActionsCommand::ExtractArtifact,
        ] {
            assert!(local.publication_kind().is_none());
        }
        let mut workspace = workspace();
        workspace.secrets.push(SecretMetadata {
            name: "TOKEN".into(),
            created_at: String::new(),
            updated_at: String::new(),
            visibility: None,
        });
        assert!(workspace
            .current_publication_target(ActionsCommand::SetSecret)
            .is_none());
        assert!(workspace
            .current_publication_target(ActionsCommand::SetVariable)
            .is_none());
        assert!(workspace
            .current_publication_target(ActionsCommand::CreateRunnerGroup)
            .is_none());
        assert!(workspace
            .current_publication_target(ActionsCommand::DeleteSecret)
            .is_none());
        workspace.selected = 1;
        assert!(
            matches!(workspace.current_publication_target(ActionsCommand::DeleteSecret),Some(ActionsTarget::Secret {name,..}) if name == "TOKEN")
        );
    }
    #[test]
    fn actions_settings_publication_target_separates_runner_group_association_kind_and_stale_scope()
    {
        let mut workspace = workspace();
        workspace.scope = ActionsScope::Organization {
            organization: "example".into(),
        };
        workspace.runner_groups.push(RunnerGroup {
            id: 9,
            name: "Builders".into(),
            visibility: "selected".into(),
            default: false,
            inherited: false,
            allows_public_repositories: false,
            restricted_to_workflows: false,
            selected_workflows: vec![],
            workflow_restrictions_read_only: false,
        });
        assert!(matches!(
            workspace.current_publication_target(ActionsCommand::GroupRunners),
            Some(ActionsTarget::GroupAssociations {
                id: 9,
                kind: GroupAssociation::Runners,
                ..
            })
        ));
        assert!(matches!(
            workspace.current_publication_target(ActionsCommand::GroupRepositories),
            Some(ActionsTarget::GroupAssociations {
                id: 9,
                kind: GroupAssociation::Repositories,
                ..
            })
        ));
        workspace.policy = Some(PolicySnapshot {
            scope: ActionsScope::Repository {
                repository: "example/repo".into(),
            },
            section: PolicySection::Permissions,
            value: json!({}),
        });
        assert!(workspace
            .current_publication_target(ActionsCommand::EditPolicy)
            .is_none());
    }
}
