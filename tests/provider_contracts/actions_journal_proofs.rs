//! Canonical provider proof contracts used by durable Actions publication journals.
use super::*;
use ghtui::providers::gh::DeploymentTransition;

fn target() -> ActionsTarget {
    ActionsTarget::DeploymentReview {
        repository: "example/repo".into(),
        run_id: 123,
        attempt: 2,
        head_sha: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
        environment_ids: vec![9],
    }
}
fn gate(id: u64) -> Value {
    json!({"environment":{"id":id,"name":format!("environment-{id}")},"wait_timer":0,"wait_timer_started_at":null,"current_user_can_approve":true,"reviewers":[]})
}
fn inspection(run: Value, gates: Vec<Value>) -> Vec<GhStep> {
    let mut steps = account_steps(42);
    steps.extend([
        step(api_args("repos/example/repo/actions/runs/123"), run.clone()),
        step(
            api_args("repos/example/repo/actions/runs/123/pending_deployments"),
            json!(gates),
        ),
        step(
            api_args("repos/example/repo/actions/runs/123/approvals"),
            json!([]),
        ),
        step(api_args("repos/example/repo/actions/runs/123"), run),
    ]);
    steps.extend(account_steps(42));
    steps
}

#[test]
fn actions_run_control_journal_canonical_resources_validate_targets_without_gh() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[]);
    let provider = provider(&fixture);
    assert_eq!(
        provider
            .canonical_actions_resources(&ActionsTarget::Run {
                repository: "EXAMPLE/REPO".into(),
                id: 123
            })
            .unwrap(),
        vec!["github.invalid/example/repo/run/123"]
    );
    assert_eq!(
        provider
            .canonical_actions_resources(&ActionsTarget::Runner {
                scope: ActionsScope::Repository {
                    repository: "EXAMPLE/REPO".into()
                },
                id: 9
            })
            .unwrap(),
        vec![
            "github.invalid/repos/example/repo/actions/runners/9",
            "github.invalid/repos/example/repo/actions/runners/9/labels"
        ]
    );
    for target in [
        ActionsTarget::Run {
            repository: "example/repo".into(),
            id: 0,
        },
        ActionsTarget::Artifact {
            repository: "invalid".into(),
            id: 9,
        },
        ActionsTarget::EnvironmentProtection {
            repository: "example/repo".into(),
            name: "prod".into(),
            environment_id: 0,
        },
        ActionsTarget::Runner {
            scope: ActionsScope::Environment {
                repository: "example/repo".into(),
                environment: "prod".into(),
            },
            id: 9,
        },
    ] {
        assert!(provider.canonical_actions_resources(&target).is_err());
    }
    assert!(journal(&fixture).is_empty());
}

#[test]
fn actions_run_control_journal_generic_inspection_brackets_positive_principal_and_keeps_barrier() {
    for changed in [false, true] {
        let fixture = Fixture::new();
        let mut steps = account_steps(42);
        steps.push(page_step(
            "repos/example/repo/actions/runs?per_page=100",
            json!([{"workflow_runs":[run_value()]}]),
        ));
        steps.extend(account_steps(if changed { 43 } else { 42 }));
        fixture.scripted_gh(&steps);
        let runner = CommandRunner::default();
        let key = "github.invalid/example/repo/run/123";
        runner.mark_ambiguous(key);
        let provider = provider(&fixture).with_runner(runner.clone());
        let observed = provider.inspect_actions_target(
            &verified_identity(),
            &ActionsTarget::Run {
                repository: "example/repo".into(),
                id: 123,
            },
            &CancellationToken::default(),
        );
        if changed {
            assert!(matches!(
                observed.unwrap_err().kind,
                GithubErrorKind::IdentityChanged
            ));
        } else {
            let observed = observed.unwrap();
            assert_eq!(observed.identity().account_id, Some(42));
            assert_eq!(observed.resource_keys(), &[key]);
            assert!(observed.deployment_transition().is_err());
        }
        assert_eq!(
            runner
                .run(
                    runner_spec(&fixture, &["never-publish"]).write(key),
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
fn actions_approvals_journal_transition_distinguishes_pending_absence_and_original_attempt_advance()
{
    for mode in [0, 1, 2] {
        let fixture = Fixture::new();
        let mut run = run_value();
        if mode == 2 {
            run["run_attempt"] = json!(3);
        }
        fixture.scripted_gh(&inspection(
            run,
            if mode == 1 {
                vec![gate(10)]
            } else {
                vec![gate(9)]
            },
        ));
        let observed = provider(&fixture)
            .inspect_actions_target(
                &verified_identity(),
                &target(),
                &CancellationToken::default(),
            )
            .unwrap();
        let transition = observed.deployment_transition().unwrap();
        match mode {
            0 => assert_eq!(
                transition,
                DeploymentTransition::StillPending {
                    environment_ids: vec![9]
                }
            ),
            1 => assert!(
                matches!(transition,DeploymentTransition::GatesNoLongerPending{environment_ids, ..} if environment_ids==vec![9])
            ),
            _ => assert_eq!(
                transition,
                DeploymentTransition::OriginalAttemptAdvanced {
                    requested_attempt: 2,
                    current_attempt: 3
                }
            ),
        }
        assert_eq!(observed.identity().account_id, Some(42));
        assert_eq!(journal(&fixture).len(), 8);
    }
}

#[test]
fn actions_approvals_journal_old_observation_cannot_release_a_newer_publication_generation() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&inspection(run_value(), vec![]));
    let runner = CommandRunner::default();
    let provider = provider(&fixture).with_runner(runner.clone());
    let observed = provider
        .inspect_actions_target(
            &verified_identity(),
            &target(),
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(matches!(
        observed.deployment_transition().unwrap(),
        DeploymentTransition::GatesNoLongerPending { .. }
    ));
    runner.mark_ambiguous("github.invalid/example/repo/run/123");
    assert!(matches!(
        observed.deployment_transition().unwrap_err().kind,
        GithubErrorKind::StaleHead
    ));
    assert_eq!(journal(&fixture).len(), 8);
}
