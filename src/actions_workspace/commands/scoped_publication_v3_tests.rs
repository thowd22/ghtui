//! SOURCE ONLY / UNRUN. Genuine original read factories, not serialized DTO
//! sources. The descriptor is descriptive; provider/Confirm guards remain.
use super::*;
use crate::{
    actions_workspace::ActionsAction,
    scoped_actions_test_support_v1 as f,
    scoped_actions_v1::{PreparedScopedActionsV1, ScopedActionsRequestKindV1},
};
use serde_json::json;

fn prepared(
    no_node: bool,
) -> (
    crate::test_support::Fixture,
    GithubProvider,
    PreparedScopedActionsV1,
    Vec<crate::test_support::GhStep>,
) {
    let mut tail = f::operation(ScopedActionsRequestKindV1::Attempt, 42);
    for step in &mut tail {
        if step.args.first().is_some_and(|arg| arg == "api")
            && step.args.get(1).is_some_and(|arg| {
                arg.ends_with("/actions/runs/123") || arg.ends_with("/actions/runs/123/attempts/2")
            })
        {
            let mut run = f::run();
            run["status"] = json!("in_progress");
            run["conclusion"] = Value::Null;
            if no_node {
                run["node_id"] = Value::Null;
            }
            step.stdout = run.to_string().into_bytes();
        }
    }
    let (fixture, provider, mut app, steps) = f::setup(tail);
    let value = f::read(
        &mut app,
        &provider,
        &fixture,
        ActionsAction::OpenRun {
            repository: f::REPO.into(),
            id: 123,
            attempt: 2,
        },
    )
    .unwrap();
    (fixture, provider, value, steps)
}
fn plan(prepared: &PreparedScopedActionsV1, provider: &GithubProvider) -> ActionsPlan {
    ActionsPlan::prepare(
        &prepared.workspace,
        ActionsCommand::Cancel,
        &Default::default(),
        provider,
        &CancellationToken::default(),
    )
    .unwrap()
}

#[test]
fn scoped_actions_publication_v3_private_plan_derives_actual_source_node_and_all_aliases() {
    let (fixture, provider, prepared, steps) = prepared(false);
    let sql = f::sql_state(&fixture);
    let git = f::git_state(&fixture);
    let plan = plan(&prepared, &provider);
    let legacy = plan.publication_descriptor(&provider).unwrap().unwrap();
    assert!(legacy.intent.context.original_repository_v3.is_none());
    let descriptor = plan
        .scoped_cancel_descriptor_v3(
            &provider,
            prepared.page.source(),
            prepared.page.run().unwrap(),
        )
        .unwrap();
    let context = &descriptor.intent.context;
    assert_eq!(context.run_id, Some(123));
    assert_eq!(context.attempt, Some(2));
    assert_eq!(
        context.head_sha.as_deref(),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
    let repository = context.original_repository_v3.as_ref().unwrap();
    assert_eq!(repository.repository_id, 10000);
    assert_eq!(repository.repository_node, "R_10000");
    assert_eq!(repository.original_name, f::REPO);
    let run = context.original_run_v3.as_ref().unwrap();
    assert_eq!(run.run_node, "WR_123");
    assert_eq!(run.workflow_id, 7);
    assert_eq!(run.check_suite_id, Some(901));
    assert!(
        context.original_pr_v3.is_none(),
        "Repository origin cannot invent a PR"
    );
    assert_eq!(
        descriptor.target.resource_keys(),
        [
            "github.invalid/example/repo0000/run/123".to_string(),
            "github.invalid/repository-id/10000/actions/run/123".to_string(),
        ]
    );
    assert_ne!(
        descriptor.intent.fingerprint(&descriptor.target).unwrap(),
        legacy.intent.fingerprint(&legacy.target).unwrap()
    );
    assert_eq!(f::sql_state(&fixture), sql);
    assert_eq!(f::git_state(&fixture), git);
    f::exact(&fixture, &steps);
}
#[test]
fn scoped_actions_publication_v3_public_plan_labels_identity_or_targets_cannot_change_private_effect(
) {
    let (fixture, provider, prepared, steps) = prepared(false);
    let sql = f::sql_state(&fixture);
    let git = f::git_state(&fixture);
    for which in 0..3 {
        let mut plan = plan(&prepared, &provider);
        match which {
            0 => plan.command = ActionsCommand::ForceCancel,
            1 => {
                plan.target = Some(ActionsTarget::Run {
                    repository: "example/other".into(),
                    id: 123,
                })
            }
            _ => plan.identity.account_id = Some(43),
        }
        assert!(plan
            .scoped_cancel_descriptor_v3(
                &provider,
                prepared.page.source(),
                prepared.page.run().unwrap(),
            )
            .is_err());
    }
    assert_eq!(f::sql_state(&fixture), sql);
    assert_eq!(f::git_state(&fixture), git);
    f::exact(&fixture, &steps);
}
#[test]
fn scoped_actions_publication_v3_equal_provider_strings_do_not_adopt_another_actual_runner() {
    let (fixture, provider, prepared, steps) = prepared(false);
    let original = plan(&prepared, &provider);
    let other = f::provider(&fixture);
    assert!(
        original
            .scoped_cancel_descriptor_v3(
                &other,
                prepared.page.source(),
                prepared.page.run().unwrap(),
            )
            .is_err(),
        "Equal host/program/env is not the producing runner"
    );
    f::exact(&fixture, &steps);
}
#[test]
fn scoped_actions_publication_v3_unknown_actual_run_node_is_readable_but_not_writable() {
    let (fixture, provider, prepared, steps) = prepared(true);
    assert!(prepared.page.run().unwrap().node_v2().is_none());
    let plan = plan(&prepared, &provider);
    assert!(
        plan.publication_descriptor(&provider).unwrap().is_some(),
        "Legacy None API is unchanged"
    );
    assert!(
        plan.scoped_cancel_descriptor_v3(
            &provider,
            prepared.page.source(),
            prepared.page.run().unwrap(),
        )
        .is_err(),
        "Never generate a run node from an ID/label"
    );
    f::exact(&fixture, &steps);
}

#[test]
fn scoped_actions_control_v5_all_three_plans_use_same_actual_run_capacity_charge_without_effects() {
    let (fixture, provider, prepared, steps) = prepared(false);
    let sql = f::sql_state(&fixture);
    let git = f::git_state(&fixture);
    for (command, action) in [
        (ActionsCommand::ForceCancel, RunMutation::ForceCancel),
        (ActionsCommand::DeleteRun, RunMutation::DeleteRun),
        (ActionsCommand::DeleteLogs, RunMutation::DeleteLogs),
    ] {
        let plan = ActionsPlan::prepare(
            &prepared.workspace,
            command,
            &Default::default(),
            &provider,
            &CancellationToken::default(),
        )
        .unwrap();
        let bytes = plan.scoped_retained_bytes_v3().unwrap();
        assert!(
            bytes
                >= crate::actions_workspace::scoped_read_v1::retained_bytes(&plan.workspace)
                    .unwrap()
        );
        assert!(bytes <= crate::scoped_actions_v1::PREPARED_LIMIT_V1);
        let descriptor = plan
            .scoped_run_descriptor_v4(
                &provider,
                prepared.page.source(),
                prepared.page.run().unwrap(),
                None,
                action,
            )
            .unwrap();
        assert_eq!(descriptor.intent.context.run_id, Some(123));
        assert_eq!(descriptor.intent.context.attempt, Some(2));
        assert!(descriptor.intent.context.original_run_v3.is_some());
    }
    assert_eq!(f::sql_state(&fixture), sql);
    assert_eq!(f::git_state(&fixture), git);
    f::exact(&fixture, &steps);
}
