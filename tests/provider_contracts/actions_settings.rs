//! Exact setting identity, permission and readback contracts with strict offline effects.
use super::*;

fn runner_value() -> Value {
    json!({"id":9,"name":"builder","os":"linux","status":"online","busy":false,"labels":[{"id":1,"name":"self-hosted","type":"read-only"},{"id":2,"name":"old","type":"custom"}]})
}
fn runner_target() -> SelfHostedRunner {
    SelfHostedRunner {
        id: 9,
        name: "builder".into(),
        os: "linux".into(),
        status: "online".into(),
        busy: false,
        labels: vec![
            RunnerLabel {
                id: 1,
                name: "self-hosted".into(),
                label_type: "read-only".into(),
            },
            RunnerLabel {
                id: 2,
                name: "old".into(),
                label_type: "custom".into(),
            },
        ],
    }
}
fn environment_edit() -> EnvironmentEdit {
    EnvironmentEdit {
        wait_timer: 30,
        prevent_self_review: true,
        reviewers: vec![EnvironmentReviewer {
            reviewer_type: "User".into(),
            id: 42,
        }],
        branch_mode: DeploymentBranchMode::Protected,
    }
}
fn before_environment() -> Value {
    json!({"id":9,"name":"prod/eu","protection_rules":[],"deployment_branch_policy":null})
}
fn after_environment(id: u64) -> Value {
    json!({"id":id,"name":"prod/eu","protection_rules":[{"type":"wait_timer","wait_timer":30},{"type":"required_reviewers","prevent_self_review":true,"reviewers":[{"type":"User","reviewer":{"id":42,"login":"octocat","type":"User"}}]}],"deployment_branch_policy":{"protected_branches":true,"custom_branch_policies":false}})
}

#[test]
fn actions_settings_environment_readback_cannot_substitute_same_name_with_another_positive_id() {
    let fixture = Fixture::new();
    let before = before_environment();
    let mut steps = account_steps(42);
    steps.extend([
        step(api_args("repos/example/repo/environments/prod%2Feu"),before.clone()),
        write_step("repos/example/repo/environments/prod%2Feu","PUT",Some(json!({"wait_timer":30,"prevent_self_review":true,"reviewers":[{"type":"User","id":42}],"deployment_branch_policy":{"protected_branches":true,"custom_branch_policies":false}})),after_environment(99)),
        step(api_args("repos/example/repo/environments/prod%2Feu"),after_environment(99)),
    ]);
    fixture.scripted_gh(&steps);
    let runner = CommandRunner::default();
    let gh = provider(&fixture).with_runner(runner.clone());
    assert!(matches!(
        gh.edit_environment(
            &verified_identity(),
            "example/repo",
            &before,
            &environment_edit(),
            &CancellationToken::default()
        )
        .unwrap_err()
        .kind,
        GithubErrorKind::AmbiguousWrite
    ));
    assert_eq!(journal(&fixture).len(), 5);
    let resource = "github.invalid/repos/example/repo/environments/prod%2feu";
    assert_eq!(
        runner
            .run(
                runner_spec(&fixture, &["never-repeat"]).write(resource),
                &CancellationToken::default()
            )
            .unwrap_err()
            .kind,
        FailureKind::ReconciliationRequired
    );
    assert_eq!(journal(&fixture).len(), 5);
    for id in [Some(0), None] {
        let fixture = Fixture::new();
        fixture.scripted_gh(&[]);
        let mut invalid = before_environment();
        match id {
            Some(id) => invalid["id"] = json!(id),
            None => {
                invalid.as_object_mut().unwrap().remove("id");
            }
        }
        assert!(provider(&fixture)
            .edit_environment(
                &verified_identity(),
                "example/repo",
                &invalid,
                &environment_edit(),
                &CancellationToken::default()
            )
            .is_err());
        assert!(
            journal(&fixture).is_empty(),
            "Missing canonical environment identity must fail before gh"
        );
    }
}

#[test]
fn actions_settings_runner_label_receipt_must_preserve_read_only_labels_not_only_custom_names() {
    for labels in [
        json!([{"id":3,"name":"gpu","type":"custom"}]),
        json!([{"id":1,"name":"different-platform","type":"read-only"},{"id":3,"name":"gpu","type":"custom"}]),
    ] {
        let fixture = Fixture::new();
        let mut after = runner_value();
        after["labels"] = labels;
        let mut steps = account_steps(42);
        steps.extend([
            step(
                api_args("repos/example/repo/actions/runners/9"),
                runner_value(),
            ),
            write_step(
                "repos/example/repo/actions/runners/9/labels",
                "PUT",
                Some(json!({"labels":["gpu"]})),
                json!({"labels":after["labels"]}),
            ),
            step(api_args("repos/example/repo/actions/runners/9"), after),
        ]);
        fixture.scripted_gh(&steps);
        let runner = CommandRunner::default();
        let gh = provider(&fixture).with_runner(runner.clone());
        assert!(matches!(
            gh.set_runner_labels(
                &verified_identity(),
                &ActionsScope::Repository {
                    repository: "example/repo".into()
                },
                &runner_target(),
                &["gpu".into()],
                &CancellationToken::default()
            )
            .unwrap_err()
            .kind,
            GithubErrorKind::AmbiguousWrite
        ));
        assert_eq!(journal(&fixture).len(), 5);
        assert_eq!(
            runner
                .run(
                    runner_spec(&fixture, &["never-repeat"])
                        .write("github.invalid/repos/example/repo/actions/runners/9/labels"),
                    &CancellationToken::default()
                )
                .unwrap_err()
                .kind,
            FailureKind::ReconciliationRequired
        );
        assert_eq!(journal(&fixture).len(), 5);
    }
}

#[test]
fn actions_settings_policy_staleness_permission_and_current_retention_maximum_block_publication() {
    let scope = ActionsScope::Repository {
        repository: "example/repo".into(),
    };
    for denied in [false, true] {
        let fixture = Fixture::new();
        let mut fresh = step(
            api_args("repos/example/repo/actions/permissions/workflow"),
            json!({"default_workflow_permissions":"write","can_approve_pull_request_reviews":false}),
        );
        if denied {
            fresh.status = 1;
            fresh.stderr = b"HTTP 403: Administration permission required".to_vec();
        }
        let mut steps = account_steps(42);
        steps.push(fresh);
        fixture.scripted_gh(&steps);
        let snapshot = ghtui::providers::gh::PolicySnapshot {
            scope: scope.clone(),
            section: PolicySection::WorkflowToken,
            value: json!({"default_workflow_permissions":"read","can_approve_pull_request_reviews":false}),
        };
        let edit = PolicyEdit::WorkflowToken {
            default_workflow_permissions: ghtui::providers::gh::WorkflowTokenPermission::Write,
            can_approve_pull_request_reviews: true,
        };
        let failure = provider(&fixture)
            .edit_actions_policy(
                &verified_identity(),
                &snapshot,
                &edit,
                &CancellationToken::default(),
            )
            .unwrap_err();
        if denied {
            assert!(matches!(failure.kind, GithubErrorKind::Permission))
        } else {
            assert!(matches!(failure.kind, GithubErrorKind::StaleHead))
        }
        let journal = journal(&fixture);
        assert_eq!(journal.len(), 3);
        assert!(journal.iter().all(|row| row["matched"] == true));
        assert!(!journal.iter().any(|row| row["args"][5] == "PUT"));
    }
    let fixture = Fixture::new();
    let policy = json!({"days":7,"maximum_allowed_days":30});
    let mut steps = account_steps(42);
    steps.push(step(
        api_args("repos/example/repo/actions/permissions/artifact-and-log-retention"),
        policy.clone(),
    ));
    fixture.scripted_gh(&steps);
    let snapshot = ghtui::providers::gh::PolicySnapshot {
        scope,
        section: PolicySection::Retention,
        value: policy,
    };
    assert!(matches!(
        provider(&fixture)
            .edit_actions_policy(
                &verified_identity(),
                &snapshot,
                &PolicyEdit::Retention { days: 31 },
                &CancellationToken::default()
            )
            .unwrap_err()
            .kind,
        GithubErrorKind::Permission
    ));
    assert_eq!(journal(&fixture).len(), 3);
}

#[test]
fn actions_settings_environment_recovery_separates_original_id_from_reused_name_and_never_retries()
{
    let fixture = Fixture::new();
    let mut steps = account_steps(42);
    steps.push(page_step(
        "repos/example/repo/environments?per_page=100",
        json!([{"environments":[after_environment(99)]}]),
    ));
    steps.extend(account_steps(42));
    steps.push(GhStep::read(&["after-ack"], "accepted"));
    fixture.scripted_gh(&steps);
    let runner = CommandRunner::default();
    let resource = "github.invalid/repos/example/repo/environments/prod%2feu";
    runner.mark_ambiguous(resource);
    let gh = provider(&fixture).with_runner(runner.clone());
    let target = ActionsTarget::EnvironmentProtection {
        repository: "example/repo".into(),
        name: "prod/eu".into(),
        environment_id: 9,
    };
    let observation = gh
        .inspect_actions_target(&verified_identity(), &target, &CancellationToken::default())
        .unwrap();
    assert_eq!(observation.state()["requested_environment_id"], 9);
    assert_eq!(observation.state()["requested_environment_name"], "prod/eu");
    assert!(observation.state()["original_by_id"].is_null());
    assert_eq!(observation.state()["current_by_name"]["id"], 99);
    assert_eq!(observation.state()["verified_principal"]["account_id"], 42);
    assert!(observation.state()["identity_note"]
        .as_str()
        .unwrap()
        .contains("not the original environment"));
    assert_eq!(
        runner
            .run(
                runner_spec(&fixture, &["no-retry"]).write(resource),
                &CancellationToken::default()
            )
            .unwrap_err()
            .kind,
        FailureKind::ReconciliationRequired
    );
    assert_eq!(journal(&fixture).len(), 5);
    gh.acknowledge_actions_observation(observation).unwrap();
    assert_eq!(
        runner
            .run(
                runner_spec(&fixture, &["after-ack"]).write(resource),
                &CancellationToken::default()
            )
            .unwrap()
            .stdout,
        b"accepted"
    );
    assert_eq!(journal(&fixture).len(), 6);
    assert!(!journal(&fixture).iter().any(|row| row["args"]
        .as_array()
        .is_some_and(|args| args.iter().any(|arg| arg == "PUT"))));

    let fixture = Fixture::new();
    let mut steps = account_steps(42);
    steps.push(page_step(
        "repos/example/repo/environments?per_page=100",
        json!([{"environments":[before_environment(),after_environment(99)]}]),
    ));
    fixture.scripted_gh(&steps);
    assert!(matches!(
        provider(&fixture)
            .inspect_actions_target(&verified_identity(), &target, &CancellationToken::default())
            .unwrap_err()
            .kind,
        GithubErrorKind::Json
    ));
    assert_eq!(journal(&fixture).len(), 3);
}
