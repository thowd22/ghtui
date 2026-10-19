//! Native operational settings rows and exact before/after previews prepared outside drawing.
use super::*;
use serde_json::json;

pub(super) fn environment_rows(environments: &[Value]) -> (Vec<ActionsRow>, Vec<Option<u64>>) {
    let mut rows = Vec::new();
    let mut identities = Vec::new();
    for environment in environments {
        let id = environment["id"].as_u64().filter(|id| *id > 0);
        let name = environment["name"]
            .as_str()
            .unwrap_or("unknown environment");
        rows.push(ActionsRow {
            cells: Default::default(),
            label: format!(
                "Environment #{} • {}",
                id.map_or_else(|| "unknown".into(), |id| id.to_string()),
                name
            ),
            role: SemanticToken::Head,
            actor: None,
            action: None,
        });
        rows.push(ActionsRow::text(match environment_branch_mode(environment) {
            Ok(DeploymentBranchMode::Protected) => "Deployment branches: only protected branches",
            Ok(DeploymentBranchMode::Selected) => "Deployment branches: selected branch/tag patterns; inspect the pattern catalog",
            Ok(DeploymentBranchMode::All) => "Deployment branches: all (explicit null branch policy)",
            Err(_) => "Deployment branch policy unavailable or inconsistent; no editable default is guessed.",
        }));
        let rules = environment["protection_rules"].as_array();
        if rules.is_none_or(Vec::is_empty) {
            rows.push(ActionsRow::text(
                "No protection rules reported for this environment.",
            ));
        }
        for rule in rules.into_iter().flatten() {
            match rule["type"].as_str() {
                Some("wait_timer") => rows.push(ActionsRow::text(format!(
                    "Wait timer: {} minutes before deployment",
                    rule["wait_timer"]
                        .as_u64()
                        .map_or_else(|| "unknown".into(), |timer| timer.to_string())
                ))),
                Some("required_reviewers") => {
                    rows.push(ActionsRow::text(format!(
                        "Prevent self-review: {} • GitHub enforces actual reviewer eligibility",
                        rule["prevent_self_review"]
                            .as_bool()
                            .map_or("unknown", |prevent| if prevent { "yes" } else { "no" })
                    )));
                    for reviewer in rule["reviewers"].as_array().into_iter().flatten() {
                        let value = &reviewer["reviewer"];
                        let reviewer_id = value["id"]
                            .as_u64()
                            .filter(|id| *id > 0)
                            .map_or_else(|| "unknown".into(), |id| id.to_string());
                        if reviewer["type"] == "Team" {
                            rows.push(ActionsRow::text(format!(
                                "Required team: {} • {} • team ID {}",
                                value["slug"].as_str().unwrap_or("unknown team"),
                                value["name"].as_str().unwrap_or(""),
                                reviewer_id
                            )));
                        } else {
                            rows.push(ActionsRow { cells: Default::default(),
                                label: format!("Required reviewer • numeric account {reviewer_id}"),
                                role: SemanticToken::Actor,
                                actor: Some(Actor {
                                    login: value["login"].as_str().unwrap_or("").into(),
                                    display_name: value["name"].as_str().map(str::to_owned),
                                    avatar_url: value["avatar_url"].as_str().map(str::to_owned),
                                    actor_type: value["type"]
                                        .as_str()
                                        .or_else(|| reviewer["type"].as_str())
                                        .unwrap_or("Unknown")
                                        .into(),
                                }),
                                action: None,
                            });
                        }
                    }
                }
                Some(kind) => rows.push(ActionsRow::text(format!(
                    "Protection rule: {kind} • rule ID {} • no human reviewer identity is guessed",
                    rule["id"]
                        .as_u64()
                        .filter(|id| *id > 0)
                        .map_or_else(|| "unknown".into(), |id| id.to_string())
                ))),
                None => rows.push(ActionsRow::text(
                    "Unknown protection rule returned by this host; no editable meaning is guessed.",
                )),
            }
        }
        identities.resize(rows.len(), id);
    }
    if environments.is_empty() {
        rows.push(ActionsRow::text(
            "No environments reported in this repository.",
        ));
        identities.push(None);
    }
    (rows, identities)
}

pub(super) fn environment_branch_mode(environment: &Value) -> GithubResult<DeploymentBranchMode> {
    match environment.get("deployment_branch_policy") {
        Some(Value::Null) => Ok(DeploymentBranchMode::All),
        Some(policy)
            if policy["protected_branches"].as_bool() == Some(true)
                && policy["custom_branch_policies"].as_bool() == Some(false) =>
        {
            Ok(DeploymentBranchMode::Protected)
        }
        Some(policy)
            if policy["protected_branches"].as_bool() == Some(false)
                && policy["custom_branch_policies"].as_bool() == Some(true) =>
        {
            Ok(DeploymentBranchMode::Selected)
        }
        _ => Err(GithubError {
            kind: GithubErrorKind::Unsupported,
            message: "GitHub did not expose an unambiguous current branch policy. No all-branches default or broad hidden change is guessed.".into(),
        }),
    }
}

/// Prepare each JSON field on its own line instead of a horizontal, truncated Debug value.
pub(super) fn value_preview(label: &str, value: &Value) -> Vec<String> {
    let mut lines = vec![label.into()];
    lines.extend(
        serde_json::to_string_pretty(value)
            .unwrap_or_else(|_| "Value unavailable; no setting is guessed".into())
            .lines()
            .map(str::to_owned),
    );
    lines
}

pub(super) fn policy_preview(observed: &PolicySnapshot, edit: &PolicyEdit) -> Vec<String> {
    let requested = match edit {
        PolicyEdit::RepositoryPermissions {
            enabled,
            allowed_actions,
            sha_pinning_required,
        } => {
            json!({"enabled":enabled,"allowed_actions":allowed_actions,"sha_pinning_required":sha_pinning_required})
        }
        PolicyEdit::OrganizationPermissions {
            enabled_repositories,
            allowed_actions,
            sha_pinning_required,
        } => {
            json!({"enabled_repositories":enabled_repositories,"allowed_actions":allowed_actions,"sha_pinning_required":sha_pinning_required})
        }
        PolicyEdit::SelectedActions {
            github_owned_allowed,
            verified_allowed,
            patterns_allowed,
        } => {
            json!({"github_owned_allowed":github_owned_allowed,"verified_allowed":verified_allowed,"patterns_allowed":patterns_allowed})
        }
        PolicyEdit::WorkflowToken {
            default_workflow_permissions,
            can_approve_pull_request_reviews,
        } => {
            json!({"default_workflow_permissions":default_workflow_permissions,"can_approve_pull_request_reviews":can_approve_pull_request_reviews})
        }
        PolicyEdit::ForkWorkflows {
            run_workflows_from_fork_pull_requests,
            send_write_tokens_to_workflows,
            send_secrets_and_variables,
            require_approval_for_fork_pr_workflows,
        } => {
            json!({"run_workflows_from_fork_pull_requests":run_workflows_from_fork_pull_requests,"send_write_tokens_to_workflows":send_write_tokens_to_workflows,"send_secrets_and_variables":send_secrets_and_variables,"require_approval_for_fork_pr_workflows":require_approval_for_fork_pr_workflows})
        }
        PolicyEdit::ForkApproval { approval_policy } => json!({"approval_policy":approval_policy}),
        PolicyEdit::Retention { days } => json!({"days":days}),
    };
    let mut lines = value_preview("Before: selected Actions policy", &observed.value);
    lines.extend(value_preview(
        "After: explicitly requested policy fields",
        &requested,
    ));
    lines
}

pub(super) fn environment_preview(observed: &Value, edit: &EnvironmentEdit) -> Vec<String> {
    let branch = match edit.branch_mode {
        DeploymentBranchMode::All => Value::Null,
        DeploymentBranchMode::Protected => {
            json!({"protected_branches":true,"custom_branch_policies":false})
        }
        DeploymentBranchMode::Selected => {
            json!({"protected_branches":false,"custom_branch_policies":true})
        }
    };
    let requested = json!({
        "environment_id":observed["id"],
        "environment_name":observed["name"],
        "wait_timer":edit.wait_timer,
        "prevent_self_review":edit.prevent_self_review,
        "reviewers":edit.reviewers.iter().map(|reviewer|json!({"type":reviewer.reviewer_type,"id":reviewer.id})).collect::<Vec<_>>(),
        "deployment_branch_policy":branch,
    });
    let mut lines = value_preview("Before: exact selected environment", observed);
    lines.extend(value_preview(
        "After: requested protection fields",
        &requested,
    ));
    lines.push(
        "Other server protection rules are not interpreted as editable human approvals.".into(),
    );
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_settings_environment_rows_keep_canonical_parent_ids_and_honest_actor_kinds() {
        let environments = vec![
            json!({"id":9,"name":"#999 is only display text","deployment_branch_policy":null,"protection_rules":[{"type":"required_reviewers","prevent_self_review":true,"reviewers":[{"type":"User","reviewer":{"id":42,"login":"octocat","type":"User"}},{"type":"Team","reviewer":{"id":8,"slug":"security","name":"Security"}},{"type":"FutureActor","reviewer":{"login":"future"}}]}]}),
            json!({"id":10,"name":"staging","deployment_branch_policy":{"protected_branches":true,"custom_branch_policies":false},"protection_rules":[{"type":"wait_timer","wait_timer":30}]}),
        ];
        let (rows, ids) = environment_rows(&environments);
        assert_eq!(rows.len(), ids.len());
        let user = rows
            .iter()
            .position(|row| {
                row.actor
                    .as_ref()
                    .is_some_and(|actor| actor.login == "octocat")
            })
            .unwrap();
        assert_eq!(ids[user], Some(9));
        assert_eq!(rows[user].actor.as_ref().unwrap().actor_type, "User");
        let team = rows
            .iter()
            .position(|row| row.label.contains("Required team: security"))
            .unwrap();
        assert_eq!(ids[team], Some(9));
        assert!(rows[team].actor.is_none());
        let future = rows
            .iter()
            .position(|row| {
                row.actor
                    .as_ref()
                    .is_some_and(|actor| actor.login == "future")
            })
            .unwrap();
        assert_eq!(ids[future], Some(9));
        assert_eq!(
            rows[future].actor.as_ref().unwrap().actor_type,
            "FutureActor"
        );
        assert!(rows
            .iter()
            .zip(ids)
            .any(|(row, id)| row.label.contains("Wait timer: 30") && id == Some(10)));
        let mut workspace = ActionsWorkspace::new(
            HostIdentity {
                host: "github.invalid".into(),
                login: "octocat".into(),
                account_id: Some(42),
                gh_version: "gh version 2.101.0".into(),
            },
            vec!["example/repo".into()],
        );
        workspace.environments = environments;
        workspace.view = ActionsView::Environments;
        workspace.prepare_rows();
        workspace.select(user);
        assert!(workspace
            .resource_key(ActionsCommand::EditEnvironment)
            .unwrap()
            .ends_with("/id/9"));
        assert_eq!(
            workspace.fields(ActionsCommand::EditEnvironment).unwrap()[0].value,
            "#999 is only display text"
        );
        let isolated = tempfile::tempdir().unwrap();
        let provider = GithubProvider::new("github.invalid")
            .unwrap()
            .with_program(isolated.path().join("gh-must-not-run"));
        let failure = ActionsPlan::prepare(
            &workspace,
            ActionsCommand::EditEnvironment,
            &std::collections::BTreeMap::from([("name".into(), "staging".into())]),
            &provider,
            &CancellationToken::default(),
        )
        .unwrap_err();
        assert!(failure
            .message
            .contains("bound to the selected environment ID/name"));
        let (empty, ids) = environment_rows(&[]);
        assert_eq!(ids, [None]);
        assert!(empty[0].label.contains("No environments reported"));
    }

    #[test]
    fn actions_settings_branch_policy_never_guesses_all_for_missing_conflicting_or_future_state() {
        for value in [
            json!({}),
            json!({"deployment_branch_policy":{}}),
            json!({"deployment_branch_policy":{"protected_branches":true,"custom_branch_policies":true}}),
            json!({"deployment_branch_policy":{"protected_branches":false,"custom_branch_policies":false}}),
        ] {
            assert!(environment_branch_mode(&value).is_err());
            let (rows, _) = environment_rows(&[
                json!({"id":9,"name":"prod","deployment_branch_policy":value.get("deployment_branch_policy").cloned().unwrap_or(json!("unknown"))}),
            ]);
            assert!(rows
                .iter()
                .any(|row| row.label.contains("no editable default is guessed")));
        }
        assert_eq!(
            environment_branch_mode(&json!({"deployment_branch_policy":null})).unwrap(),
            DeploymentBranchMode::All
        );
        assert_eq!(environment_branch_mode(&json!({"deployment_branch_policy":{"protected_branches":false,"custom_branch_policies":true}})).unwrap(),DeploymentBranchMode::Selected);
    }

    #[test]
    fn actions_settings_policy_preview_names_before_after_without_conflating_requested_and_observed_values(
    ) {
        let snapshot = PolicySnapshot {
            scope: ActionsScope::Repository {
                repository: "example/repo".into(),
            },
            section: PolicySection::WorkflowToken,
            value: json!({"default_workflow_permissions":"read","can_approve_pull_request_reviews":false,"future_server_field":"preserved context"}),
        };
        let edit = PolicyEdit::WorkflowToken {
            default_workflow_permissions: WorkflowTokenPermission::Write,
            can_approve_pull_request_reviews: true,
        };
        let lines = policy_preview(&snapshot, &edit);
        let before = lines
            .iter()
            .position(|line| line.starts_with("Before:"))
            .unwrap();
        let after = lines
            .iter()
            .position(|line| line.starts_with("After:"))
            .unwrap();
        assert!(before < after);
        assert!(lines[before..after]
            .iter()
            .any(|line| line.contains("\"read\"")));
        assert!(lines[before..after]
            .iter()
            .any(|line| line.contains("future_server_field")));
        assert!(lines[after..].iter().any(|line| line.contains("\"write\"")));
        assert!(!lines[after..]
            .iter()
            .any(|line| line.contains("future_server_field")));
    }
}
