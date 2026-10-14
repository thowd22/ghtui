//! Later effects come only from fresh opaque account/resource-bound observations.
use super::*;
use ghtui::providers::gh::{AcceptedDispatchTransition, AcceptedRunTransition};

fn run_inspection(values: Vec<Value>) -> Vec<GhStep> {
    let mut steps = account_steps(42);
    steps.push(page_step(
        "repos/example/repo/actions/runs?per_page=100",
        json!([{"workflow_runs":values}]),
    ));
    steps.extend(account_steps(42));
    steps
}
fn positive_identity() -> HostIdentity {
    HostIdentity {
        account_id: Some(42),
        ..identity()
    }
}
fn run_target_identity() -> ActionsTarget {
    ActionsTarget::Run {
        repository: "example/repo".into(),
        id: 123,
    }
}
fn assert_reads(fixture: &Fixture, count: usize) {
    let rows = journal(fixture);
    assert_eq!(rows.len(), count);
    assert!(rows.iter().all(|row| row["matched"] == true));
    assert!(!rows
        .iter()
        .any(|row| row["args"][5] == "POST" || row["args"][5] == "DELETE"));
}

#[test]
fn actions_run_control_accepted_transition_retains_original_attempt_and_actual_conclusion() {
    for (action, attempt, conclusion, expected) in [
        (
            RunMutation::Cancel,
            2,
            "cancelled",
            AcceptedRunTransition::RequestedCancellationObserved { attempt: 2 },
        ),
        (
            RunMutation::Cancel,
            2,
            "success",
            AcceptedRunTransition::DifferentCompletionObserved {
                attempt: 2,
                conclusion: "success".into(),
            },
        ),
        (
            RunMutation::RerunAll { debug: false },
            3,
            "success",
            AcceptedRunTransition::OriginalAttemptAdvanced {
                requested_attempt: 2,
                current_attempt: 3,
            },
        ),
        (
            RunMutation::RerunAll { debug: false },
            2,
            "success",
            AcceptedRunTransition::StillPending {
                current_attempt: 2,
                status: "completed".into(),
                conclusion: "success".into(),
            },
        ),
        (
            RunMutation::DeleteLogs,
            2,
            "success",
            AcceptedRunTransition::LogsEffectNotObservable,
        ),
    ] {
        let fixture = Fixture::new();
        let mut run = run_value();
        run["run_attempt"] = json!(attempt);
        run["conclusion"] = json!(conclusion);
        fixture.scripted_gh(&run_inspection(vec![run]));
        let observation = provider(&fixture)
            .inspect_actions_target(
                &positive_identity(),
                &run_target_identity(),
                &CancellationToken::default(),
            )
            .unwrap();
        assert_eq!(
            observation
                .run_transition(123, 2, &run_target().head_sha, action)
                .unwrap(),
            expected
        );
        assert_reads(&fixture, 5);
    }
    let fixture = Fixture::new();
    fixture.scripted_gh(&run_inspection(vec![]));
    let observation = provider(&fixture)
        .inspect_actions_target(
            &positive_identity(),
            &run_target_identity(),
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(
        observation
            .run_transition(123, 2, &run_target().head_sha, RunMutation::Cancel)
            .unwrap(),
        AcceptedRunTransition::RunNotFoundInObservedCatalogue
    );
    assert_reads(&fixture, 5);
}

#[test]
fn actions_run_control_accepted_transition_rejects_context_drift_and_new_publication() {
    let fixture = Fixture::new();
    let mut steps = run_inspection(vec![run_value()]);
    steps.push(GhStep::read(&["simulate-later-publication"], ""));
    fixture.scripted_gh(&steps);
    let runner = CommandRunner::default();
    let provider = provider(&fixture).with_runner(runner.clone());
    let observation = provider
        .inspect_actions_target(
            &positive_identity(),
            &run_target_identity(),
            &CancellationToken::default(),
        )
        .unwrap();
    for (id, attempt, head) in [
        (124, 2, run_target().head_sha),
        (123, 0, run_target().head_sha),
        (123, 3, run_target().head_sha),
        (123, 2, "b".repeat(40)),
    ] {
        assert!(observation
            .run_transition(id, attempt, &head, RunMutation::Cancel)
            .is_err());
    }
    assert_eq!(journal(&fixture).len(), 5);
    runner
        .run(
            runner_spec(&fixture, &["simulate-later-publication"])
                .write("github.invalid/example/repo/run/123"),
            &CancellationToken::default(),
        )
        .unwrap();
    let error = observation
        .run_transition(123, 2, &run_target().head_sha, RunMutation::Cancel)
        .unwrap_err();
    assert!(matches!(error.kind, GithubErrorKind::StaleHead));
    assert_reads(&fixture, 6);
}

#[test]
fn actions_approvals_accepted_fork_transition_requires_known_state_and_original_event() {
    for (event, state, valid) in [
        ("pull_request", "queued", true),
        ("pull_request", "", false),
        ("push", "queued", false),
    ] {
        let fixture = Fixture::new();
        let mut run = run_value();
        run["event"] = json!(event);
        run["status"] = json!(state);
        run["conclusion"] = Value::Null;
        fixture.scripted_gh(&run_inspection(vec![run]));
        let observation = provider(&fixture)
            .inspect_actions_target(
                &positive_identity(),
                &run_target_identity(),
                &CancellationToken::default(),
            )
            .unwrap();
        let transition =
            observation.run_transition(123, 2, &run_target().head_sha, RunMutation::ApproveFork);
        if valid {
            assert_eq!(
                transition.unwrap(),
                AcceptedRunTransition::ForkApprovalGatePassed {
                    attempt: 2,
                    status: "queued".into()
                }
            );
        } else {
            assert!(transition.is_err());
        }
        assert_reads(&fixture, 5);
    }
}

fn workflow_inspection(values: Vec<Value>, total: u64) -> Vec<GhStep> {
    let mut steps = account_steps(42);
    steps.extend([
        page_step("repos/example/repo/actions/workflows?per_page=100",json!([{"workflows":[{"id":42,"name":"unit","path":".github/workflows/unit.yml","state":"active"}]}])),
        page_step("repos/example/repo/actions/workflows/42/runs?per_page=100",json!([{"total_count":total,"workflow_runs":values}])),
    ]);
    steps.extend(account_steps(42));
    steps
}

#[test]
fn workflow_controls_accepted_dispatch_requires_positive_exact_launch_correlation() {
    for (present, total, expected) in [
        (
            true,
            1,
            AcceptedDispatchTransition::CorrelatedRunObserved {
                run_id: 123,
                attempt: 2,
                head_sha: run_target().head_sha,
            },
        ),
        (
            false,
            10,
            AcceptedDispatchTransition::CorrelationNotObserved {
                catalogue_complete: false,
            },
        ),
        (
            false,
            0,
            AcceptedDispatchTransition::CorrelationNotObserved {
                catalogue_complete: true,
            },
        ),
    ] {
        let fixture = Fixture::new();
        let mut run = run_value();
        run["event"] = json!("workflow_dispatch");
        fixture.scripted_gh(&workflow_inspection(
            if present { vec![run] } else { vec![] },
            total,
        ));
        let observation = provider(&fixture)
            .inspect_actions_target(
                &positive_identity(),
                &ActionsTarget::Workflow {
                    repository: "example/repo".into(),
                    id: 42,
                },
                &CancellationToken::default(),
            )
            .unwrap();
        assert_eq!(
            observation
                .dispatch_transition(42, &run_target().head_sha, Some(123))
                .unwrap(),
            expected
        );
        assert_eq!(
            observation
                .dispatch_transition(42, &run_target().head_sha, None)
                .unwrap(),
            AcceptedDispatchTransition::UncorrelatedAcceptance
        );
        assert!(observation
            .dispatch_transition(43, &run_target().head_sha, Some(123))
            .is_err());
        if present {
            assert!(observation
                .dispatch_transition(42, &"b".repeat(40), Some(123))
                .is_err());
        }
        assert_reads(&fixture, 6);
    }
    let fixture = Fixture::new();
    fixture.scripted_gh(&workflow_inspection(vec![run_value()], 1));
    let observation = provider(&fixture)
        .inspect_actions_target(
            &positive_identity(),
            &ActionsTarget::Workflow {
                repository: "example/repo".into(),
                id: 42,
            },
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(observation
        .dispatch_transition(42, &run_target().head_sha, Some(123))
        .is_err());
    assert_reads(&fixture, 6);
}
