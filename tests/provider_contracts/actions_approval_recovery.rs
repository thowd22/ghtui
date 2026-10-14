//! Task57 exact approval recovery contracts with strict offline command effects.
use super::*;

fn approval_target() -> ActionsTarget {
    ActionsTarget::DeploymentReview {
        repository: "example/repo".into(),
        run_id: 123,
        attempt: 2,
        head_sha: run_target().head_sha,
        environment_ids: vec![9, 10],
    }
}
fn pending_gates() -> Value {
    json!([{"environment":{"id":10,"name":"staging"},"current_user_can_approve":true,"wait_timer":30,"reviewers":[]}])
}
fn review_history() -> Value {
    json!([{"state":"approved","comment":"ship","environments":[{"id":9,"name":"production"}],"user":{"id":42,"login":"octocat","type":"User"}}])
}
fn inspection_steps(initial: Value, fresh: Value, final_account: u64) -> Vec<GhStep> {
    let mut steps = account_steps(42);
    steps.extend([
        step(api_args("repos/example/repo/actions/runs/123"), initial),
        step(
            api_args("repos/example/repo/actions/runs/123/pending_deployments"),
            pending_gates(),
        ),
        step(
            api_args("repos/example/repo/actions/runs/123/approvals"),
            review_history(),
        ),
        step(api_args("repos/example/repo/actions/runs/123"), fresh),
    ]);
    steps.extend(account_steps(final_account));
    steps
}

#[test]
fn actions_approvals_recovery_inspects_exact_effects_without_guessing_attempt_history_or_unlocking()
{
    let fixture = Fixture::new();
    let mut observed_run = run_value();
    observed_run["run_attempt"] = json!(3);
    let mut steps = inspection_steps(observed_run.clone(), observed_run, 42);
    let mut newer = GhStep::read(&["newer-write"], "");
    newer.status = 1;
    newer.stderr = b"HTTP 503 after request publication".to_vec();
    steps.push(newer);
    fixture.scripted_gh(&steps);
    let runner = CommandRunner::default();
    let resource = "github.invalid/example/repo/run/123";
    runner.mark_ambiguous(resource);
    let gh = provider(&fixture).with_runner(runner.clone());
    let observation = gh
        .inspect_actions_target(
            &verified_identity(),
            &approval_target(),
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(
        observation.state()["selected_environment_ids"],
        json!([9, 10])
    );
    assert_eq!(observation.state()["requested_attempt"], 2);
    assert_eq!(observation.state()["run"]["attempt"], 3);
    assert_eq!(
        observation.state()["pending_gates"][0]["environment_id"],
        10
    );
    assert_eq!(
        observation.state()["approval_history"][0]["environments"][0]["id"],
        9
    );
    assert_eq!(observation.state()["approval_history"][0]["account_id"], 42);
    assert_eq!(observation.state()["history_scope"], "Whole run ID");
    assert!(observation.state()["history_limitations"][1]
        .as_str()
        .unwrap()
        .contains("No selected-attempt"));
    assert_eq!(observation.state()["verified_principal"]["account_id"], 42);
    let blocked = runner
        .run(
            runner_spec(&fixture, &["must-not-publish"]).write(resource),
            &CancellationToken::default(),
        )
        .unwrap_err();
    assert_eq!(blocked.kind, FailureKind::ReconciliationRequired);
    assert_eq!(journal(&fixture).len(), 8);
    assert!(journal(&fixture)
        .iter()
        .all(|entry| entry["matched"] == true));
    assert!(journal(&fixture).iter().all(|entry| entry["args"]
        .as_array()
        .is_some_and(|args| !args.iter().any(|arg| arg == "POST"))));
    assert!(matches!(
        provider(&fixture)
            .acknowledge_actions_observation(observation.clone())
            .unwrap_err()
            .kind,
        GithubErrorKind::IdentityChanged
    ));
    let stale = observation.clone();
    gh.acknowledge_actions_observation(observation).unwrap();
    let failed = runner
        .run(
            runner_spec(&fixture, &["newer-write"]).write(resource),
            &CancellationToken::default(),
        )
        .unwrap_err();
    assert!(failed.outcome_unknown);
    assert!(matches!(
        gh.acknowledge_actions_observation(stale).unwrap_err().kind,
        GithubErrorKind::StaleHead
    ));
    assert_eq!(
        runner
            .run(
                runner_spec(&fixture, &["never-retry"]).write(resource),
                &CancellationToken::default()
            )
            .unwrap_err()
            .kind,
        FailureKind::ReconciliationRequired
    );
    assert_eq!(journal(&fixture).len(), 9);
}

#[test]
fn actions_approvals_recovery_rejects_invalid_targets_and_unverified_principals_before_transport() {
    for (attempt, head, ids) in [
        (0, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", vec![9]),
        (2, "bad", vec![9]),
        (2, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", vec![]),
        (2, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", vec![0]),
        (2, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", vec![9, 9]),
    ] {
        let fixture = Fixture::new();
        fixture.scripted_gh(&[]);
        let target = ActionsTarget::DeploymentReview {
            repository: "example/repo".into(),
            run_id: 123,
            attempt,
            head_sha: head.into(),
            environment_ids: ids,
        };
        assert!(provider(&fixture)
            .inspect_actions_target(&verified_identity(), &target, &CancellationToken::default())
            .is_err());
        assert!(journal(&fixture).is_empty());
    }
    for principal in [
        identity(),
        HostIdentity {
            account_id: Some(0),
            ..verified_identity()
        },
        HostIdentity {
            host: "other.invalid".into(),
            ..verified_identity()
        },
    ] {
        let fixture = Fixture::new();
        fixture.scripted_gh(&[]);
        assert!(provider(&fixture)
            .inspect_actions_target(
                &principal,
                &approval_target(),
                &CancellationToken::default()
            )
            .is_err());
        assert!(
            journal(&fixture).is_empty(),
            "Unverified or wrong-host identity must not reach gh"
        );
    }
}

#[test]
fn actions_approvals_recovery_rejects_same_login_reassignment_and_mid_read_run_changes() {
    let resource = "github.invalid/example/repo/run/123";
    for (kind, mut fresh, final_account, expected_calls) in [
        (GithubErrorKind::IdentityChanged, run_value(), 43, 8),
        (
            GithubErrorKind::StaleHead,
            {
                let mut run = run_value();
                run["run_attempt"] = json!(3);
                run
            },
            42,
            6,
        ),
        (
            GithubErrorKind::StaleHead,
            {
                let mut run = run_value();
                run["workflow_id"] = json!(99);
                run
            },
            42,
            6,
        ),
    ] {
        let fixture = Fixture::new();
        // Both head values remain equal: this proves account ID/attempt/workflow checks,
        // rather than accidentally exercising only a changed-commit guard.
        fresh["head_sha"] = run_value()["head_sha"].clone();
        let mut steps = inspection_steps(run_value(), fresh, final_account);
        steps.truncate(expected_calls);
        fixture.scripted_gh(&steps);
        let runner = CommandRunner::default();
        runner.mark_ambiguous(resource);
        let gh = provider(&fixture).with_runner(runner.clone());
        let actual = gh
            .inspect_actions_target(
                &verified_identity(),
                &approval_target(),
                &CancellationToken::default(),
            )
            .unwrap_err()
            .kind;
        assert_eq!(
            std::mem::discriminant(&actual),
            std::mem::discriminant(&kind),
            "Expected {kind:?}; received {actual:?}"
        );
        assert_eq!(
            runner
                .run(
                    runner_spec(&fixture, &["never-publish"]).write(resource),
                    &CancellationToken::default()
                )
                .unwrap_err()
                .kind,
            FailureKind::ReconciliationRequired
        );
        assert_eq!(journal(&fixture).len(), expected_calls);
        assert!(journal(&fixture)
            .iter()
            .all(|entry| entry["matched"] == true));
    }
}

#[test]
fn actions_approvals_recovery_missing_history_is_not_empty_history_and_does_not_unlock() {
    let fixture = Fixture::new();
    let mut steps = account_steps(42);
    steps.extend([
        step(api_args("repos/example/repo/actions/runs/123"), run_value()),
        step(
            api_args("repos/example/repo/actions/runs/123/pending_deployments"),
            pending_gates(),
        ),
    ]);
    let mut unavailable = step(
        api_args("repos/example/repo/actions/runs/123/approvals"),
        Value::Null,
    );
    unavailable.status = 1;
    unavailable.stderr = b"HTTP 404: history not available on this host".to_vec();
    steps.push(unavailable);
    fixture.scripted_gh(&steps);
    let runner = CommandRunner::default();
    let resource = "github.invalid/example/repo/run/123";
    runner.mark_ambiguous(resource);
    let gh = provider(&fixture).with_runner(runner.clone());
    assert!(gh
        .inspect_actions_target(
            &verified_identity(),
            &approval_target(),
            &CancellationToken::default()
        )
        .is_err());
    assert_eq!(
        runner
            .run(
                runner_spec(&fixture, &["not-acknowledged"]).write(resource),
                &CancellationToken::default()
            )
            .unwrap_err()
            .kind,
        FailureKind::ReconciliationRequired
    );
    assert_eq!(journal(&fixture).len(), 5);
    assert!(journal(&fixture)
        .iter()
        .all(|entry| entry["matched"] == true));
}
