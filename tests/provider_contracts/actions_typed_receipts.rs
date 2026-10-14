//! Exact original attempts and actual outcomes, not a success-message receipt.
use super::*;

fn running() -> (WorkflowRun, Value) {
    let mut target = run_target();
    target.status = "in_progress".into();
    target.conclusion.clear();
    let mut value = run_value();
    value["status"] = json!("in_progress");
    value["conclusion"] = Value::Null;
    (target, value)
}

#[test]
fn actions_run_control_cancellation_racing_other_completion_is_only_accepted() {
    for action in [RunMutation::Cancel, RunMutation::ForceCancel] {
        for conclusion in ["success", "failure", "timed_out", "skipped"] {
            let fixture = Fixture::new();
            let (target, initial) = running();
            let mut observed = run_value();
            observed["conclusion"] = json!(conclusion);
            let mut steps = run_guard(initial);
            steps.extend([
                write_step(
                    &format!(
                        "repos/example/repo/actions/runs/123/{}",
                        if action == RunMutation::Cancel {
                            "cancel"
                        } else {
                            "force-cancel"
                        }
                    ),
                    "POST",
                    None,
                    Value::Null,
                ),
                step(api_args("repos/example/repo/actions/runs/123"), observed),
            ]);
            fixture.scripted_gh(&steps);
            let outcome = provider(&fixture)
                .mutate_run(
                    &identity(),
                    "example/repo",
                    &target,
                    action,
                    &CancellationToken::default(),
                )
                .unwrap();
            assert!(outcome.accepted);
            assert!(!outcome.completed, "{action:?}: {conclusion}");
            assert_eq!(outcome.run.unwrap().conclusion, conclusion);
            assert!(outcome
                .explanation
                .contains("do not issue the mutation again"));
            assert_eq!(journal(&fixture).len(), 5);
            assert!(journal(&fixture).iter().all(|row| row["matched"] == true));
            assert_eq!(
                journal(&fixture)
                    .iter()
                    .filter(|row| row["args"][5] == "POST")
                    .count(),
                1
            );
        }
    }
}

#[test]
fn actions_run_control_postwrite_original_attempt_drift_is_ambiguous_and_blocks_retry() {
    for action in [
        RunMutation::Cancel,
        RunMutation::ForceCancel,
        RunMutation::DeleteLogs,
        RunMutation::ApproveFork,
        RunMutation::RerunAll { debug: false },
    ] {
        let fixture = Fixture::new();
        let (mut target, mut initial) =
            if matches!(action, RunMutation::Cancel | RunMutation::ForceCancel) {
                running()
            } else {
                (run_target(), run_value())
            };
        if action == RunMutation::ApproveFork {
            target.event = "pull_request".into();
            target.status = "action_required".into();
            target.conclusion = "action_required".into();
            initial["event"] = json!("pull_request");
            initial["status"] = json!("action_required");
            initial["conclusion"] = json!("action_required");
        }
        let mut observed = initial.clone();
        observed["run_attempt"] = json!(if matches!(action, RunMutation::RerunAll { .. }) {
            1
        } else {
            3
        });
        let suffix = match action {
            RunMutation::Cancel => "cancel",
            RunMutation::ForceCancel => "force-cancel",
            RunMutation::DeleteLogs => "logs",
            RunMutation::ApproveFork => "approve",
            _ => "rerun",
        };
        let method = if action == RunMutation::DeleteLogs {
            "DELETE"
        } else {
            "POST"
        };
        let mut steps = run_guard(initial.clone());
        steps.extend([
            write_step(
                &format!("repos/example/repo/actions/runs/123/{suffix}"),
                method,
                if matches!(action, RunMutation::RerunAll { .. }) {
                    Some(json!({"enable_debug_logging":false}))
                } else {
                    None
                },
                Value::Null,
            ),
            step(api_args("repos/example/repo/actions/runs/123"), observed),
        ]);
        steps.extend(run_guard(initial));
        fixture.scripted_gh(&steps);
        let provider = provider(&fixture);
        let first = provider
            .mutate_run(
                &identity(),
                "example/repo",
                &target,
                action,
                &CancellationToken::default(),
            )
            .unwrap_err();
        assert!(matches!(first.kind, GithubErrorKind::AmbiguousWrite));
        assert!(first.message.contains("original attempt"));
        let retry = provider
            .mutate_run(
                &identity(),
                "example/repo",
                &target,
                action,
                &CancellationToken::default(),
            )
            .unwrap_err();
        assert!(matches!(retry.kind, GithubErrorKind::AmbiguousWrite));
        let rows = journal(&fixture);
        assert_eq!(rows.len(), 8);
        assert!(rows.iter().all(|row| row["matched"] == true));
        assert_eq!(
            rows.iter().filter(|row| row["args"][5] == method).count(),
            1
        );
    }
}

#[test]
fn actions_approvals_blank_postwrite_fork_status_cannot_imply_gate_passed() {
    for state in [Value::Null, json!("future-unrecognized-state")] {
        let fixture = Fixture::new();
        let mut target = run_target();
        target.event = "pull_request".into();
        target.status = "action_required".into();
        target.conclusion.clear();
        let mut initial = run_value();
        initial["event"] = json!("pull_request");
        initial["status"] = json!("action_required");
        initial["conclusion"] = Value::Null;
        let mut observed = initial.clone();
        observed["status"] = state;
        observed["conclusion"] = Value::Null;
        let mut steps = run_guard(initial);
        steps.extend([
            write_step(
                "repos/example/repo/actions/runs/123/approve",
                "POST",
                None,
                Value::Null,
            ),
            step(api_args("repos/example/repo/actions/runs/123"), observed),
        ]);
        fixture.scripted_gh(&steps);
        let error = provider(&fixture)
            .mutate_run(
                &identity(),
                "example/repo",
                &target,
                RunMutation::ApproveFork,
                &CancellationToken::default(),
            )
            .unwrap_err();
        assert!(matches!(error.kind, GithubErrorKind::AmbiguousWrite));
        assert!(error.message.contains("unrecognized"));
        assert_eq!(journal(&fixture).len(), 5);
        assert!(journal(&fixture).iter().all(|row| row["matched"] == true));
    }
}
