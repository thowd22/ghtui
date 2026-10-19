//! Workflow page usability: readable summary, key/click parity and plain previews.
use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::json;
use std::collections::BTreeMap;

fn definition(state: &str, triggers: &[&str]) -> WorkflowDefinition {
    WorkflowDefinition {
        workflow: Workflow {
            id: 7,
            name: "Build dispatch".into(),
            path: ".github/workflows/ci.yml".into(),
            state: state.into(),
        },
        revision: "main".into(),
        commit_sha: "a".repeat(40),
        yaml: "name: Build dispatch\n".into(),
        triggers: triggers.iter().map(|t| (*t).to_owned()).collect(),
        dispatch_inputs: vec![
            DispatchInput {
                name: "debug".into(),
                description: "Debug build".into(),
                input_type: DispatchInputType::Boolean,
                required: false,
                default: Some(json!(false)),
                options: vec![],
            },
            DispatchInput {
                name: "mode".into(),
                description: "Build mode".into(),
                input_type: DispatchInputType::Choice,
                required: true,
                default: Some(json!("safe")),
                options: vec!["safe".into(), "fast".into()],
            },
        ],
        jobs: vec![
            DefinitionJob {
                id: "build".into(),
                name: "build".into(),
                needs: vec![],
                reusable_workflow: None,
                matrix: Value::Null,
                environment: Value::Null,
            },
            DefinitionJob {
                id: "report".into(),
                name: "report".into(),
                needs: vec!["build".into()],
                reusable_workflow: None,
                matrix: Value::Null,
                environment: Value::Null,
            },
        ],
    }
}

fn workspace(state: &str, triggers: &[&str]) -> ActionsWorkspace {
    let mut workspace = ActionsWorkspace::new(
        HostIdentity {
            host: "github.invalid".into(),
            login: "octocat".into(),
            account_id: Some(42),
            gh_version: "2.101.0".into(),
        },
        vec!["example/repo".into()],
    );
    workspace.view = ActionsView::Workflow;
    workspace.definition = Some(definition(state, triggers));
    workspace.prepare_rows();
    workspace
}

/// What the key does in the Actions view: its key-bar control (a disabled one explains), then
/// the hand-written view keys.
fn key(workspace: &ActionsWorkspace, code: char) -> Option<ActionsAction> {
    let event = KeyEvent::new(KeyCode::Char(code), KeyModifiers::SHIFT);
    match crate::footer::resolve_key(&crate::actions_ui::footer_keys(workspace), event) {
        Some(crate::app::Action::Actions(action)) => Some(action),
        Some(crate::app::Action::Explain(reason)) => Some(ActionsAction::Unavailable(reason)),
        Some(other) => panic!("unexpected {other:?}"),
        None => crate::actions_ui::keyboard_action(event, workspace),
    }
}

#[test]
fn v1_actions_workflows_summary_explains_manual_runs_inputs_and_jobs() {
    let rows = workflow_summary_rows(&definition("active", &["push", "workflow_dispatch"]));
    assert_eq!(
        rows[0],
        format!(
            "Build dispatch • active • revision main • exact commit {}",
            "a".repeat(40)
        )
    );
    assert!(rows.contains(
        &"Manual runs: press D to dispatch on main with the 2 input(s) below".to_owned()
    ));
    assert!(rows
        .contains(&"Input debug • true/false • optional • default false • Debug build".to_owned()));
    assert!(rows.contains(
        &"Input mode • one of safe, fast • required • default safe • Build mode".to_owned()
    ));
    // Empty needs/matrix no longer render as dangling jargon.
    assert!(rows.contains(&"Definition job build".to_owned()));
    assert!(rows.contains(&"Definition job report • needs build".to_owned()));
    assert!(!rows.iter().any(|row| row.contains("reusable none")));

    let no_manual = workflow_summary_rows(&definition("active", &["push"]));
    assert!(
        no_manual.contains(&"Manual runs: not available (no workflow_dispatch trigger)".to_owned())
    );
    let disabled = workflow_summary_rows(&definition("disabled_manually", &["workflow_dispatch"]));
    assert!(disabled[0].contains("• disabled manually •"));
    assert!(disabled.contains(&"Manual runs: enable the workflow first (X)".to_owned()));
}

#[test]
fn v1_actions_workflows_keys_resolve_to_the_same_visible_controls_as_clicks() {
    let active = workspace("active", &["workflow_dispatch"]);
    let labels = active
        .controls
        .iter()
        .map(|control| control.label.as_str())
        .collect::<Vec<_>>();
    for label in [
        "V Choose workflow revision",
        "X Enable workflow",
        "X Disable workflow",
        "D Dispatch workflow",
    ] {
        assert!(labels.contains(&label), "{label} missing from {labels:?}");
    }
    assert_eq!(
        key(&active, 'D'),
        Some(ActionsAction::Compose(ActionsCommand::DispatchWorkflow))
    );
    assert_eq!(
        key(&active, 'X'),
        Some(ActionsAction::Compose(ActionsCommand::DisableWorkflow))
    );
    assert_eq!(
        key(&active, 'V'),
        Some(ActionsAction::Compose(ActionsCommand::WorkflowRevision))
    );

    let disabled = workspace("disabled_manually", &["workflow_dispatch"]);
    assert_eq!(
        key(&disabled, 'X'),
        Some(ActionsAction::Compose(ActionsCommand::EnableWorkflow))
    );
    assert_eq!(
        key(&disabled, 'D'),
        Some(ActionsAction::Unavailable(
            "D Dispatch workflow: Enable this workflow (X) before dispatching it".into()
        ))
    );
    assert_eq!(
        disabled
            .disabled_reason(ActionsCommand::DisableWorkflow)
            .as_deref(),
        Some("Workflow already disabled")
    );

    let push_only = workspace("active", &["push"]);
    assert!(matches!(
        key(&push_only, 'D'),
        Some(ActionsAction::Unavailable(reason)) if reason.contains("no workflow_dispatch trigger")
    ));
    // Outside the definition page the letters keep their old meaning (none).
    let mut runs = workspace("active", &["workflow_dispatch"]);
    runs.view = ActionsView::Runs;
    runs.prepare_rows();
    assert_eq!(key(&runs, 'D'), None);
    assert_eq!(key(&runs, 'X'), None);
}

#[test]
fn v1_actions_workflows_preview_names_workflow_ref_and_every_input_plainly() {
    let gh = GithubProvider::new("github.invalid")
        .unwrap()
        .with_program("/nonexistent/ghtui-no-gh");
    let cancel = CancellationToken::default();
    let active = workspace("active", &["workflow_dispatch"]);
    let values = BTreeMap::from([
        ("revision".to_owned(), "main".to_owned()),
        ("input:debug".to_owned(), String::new()),
        ("input:mode".to_owned(), "fast".to_owned()),
    ]);
    let plan = ActionsPlan::prepare(
        &active,
        ActionsCommand::DispatchWorkflow,
        &values,
        &gh,
        &cancel,
    )
    .unwrap();
    let preview = plan.preview.join("\n");
    assert!(preview.contains("Workflow: Build dispatch (.github/workflows/ci.yml) in example/repo"));
    assert!(preview.contains(&format!("Branch or tag: main (commit {})", "a".repeat(40))));
    assert!(preview.contains("Input mode: fast"));
    assert!(preview.contains("Input debug: (not sent; GitHub uses the workflow default)"));
    assert!(!preview.contains("Settings scope:"));
    assert!(!preview.contains("Exact affected target:"));
    assert!(!preview.contains("input:mode"));

    let wrong_ref = BTreeMap::from([
        ("revision".to_owned(), "feature".to_owned()),
        ("input:mode".to_owned(), "fast".to_owned()),
    ]);
    let error = ActionsPlan::prepare(
        &active,
        ActionsCommand::DispatchWorkflow,
        &wrong_ref,
        &gh,
        &cancel,
    )
    .err()
    .unwrap();
    assert!(error.message.contains("press V on the workflow page"));

    let plan = ActionsPlan::prepare(
        &active,
        ActionsCommand::DisableWorkflow,
        &BTreeMap::new(),
        &gh,
        &cancel,
    )
    .unwrap();
    let preview = plan.preview.join("\n");
    assert!(preview.contains("Workflow: Build dispatch (.github/workflows/ci.yml) in example/repo"));
    assert!(preview.contains("GitHub stops starting new runs of this workflow"));
}
