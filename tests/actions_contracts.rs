#[path = "support/providers.rs"]
#[allow(dead_code)] // Shared Git/provider fixture API; this suite uses its Actions subset.
mod support;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ghtui::{
    actions_ui::{draw, keyboard_action},
    actions_workspace::{
        ActionsAction, ActionsCommand, ActionsPlan, ActionsView, ActionsWorkspace, RunPage,
    },
    config::ThemeName,
    providers::{
        gh::{
            ActionsArtifact, ActionsTarget, Actor, ArtifactArchive, ArtifactLimits, GithubProvider,
            HostIdentity, RunAttempt, WorkflowRun,
        },
        runner::CancellationToken,
    },
    theme::{ColorMode, SemanticToken, Theme},
};
use ratatui::{backend::TestBackend, Terminal};
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs};
use support::{Fixture, GhStep};
fn identity() -> HostIdentity {
    HostIdentity {
        host: "github.invalid".into(),
        login: "octocat".into(),
        account_id: Some(42),
        gh_version: "gh version 2.101.0".into(),
    }
}
fn provider(fixture: &Fixture) -> GithubProvider {
    GithubProvider::new("github.invalid")
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"))
}
fn args(endpoint: &str) -> Vec<String> {
    [
        "api",
        endpoint,
        "--hostname",
        "github.invalid",
        "--method",
        "GET",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}
fn step(endpoint: &str, value: Value, pages: bool) -> GhStep {
    let mut args = args(endpoint);
    if pages {
        args.extend(["--paginate".into(), "--slurp".into()]);
    }
    GhStep {
        args,
        stdin: vec![],
        stdout: serde_json::to_vec(&value).unwrap(),
        stderr: vec![],
        status: 0,
        delay_ms: 0,
    }
}
fn raw_run(id: u64) -> Value {
    json!({"id":id,"workflow_id":7,"name":"CI matrix","display_title":"Fix checkout","head_sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","head_branch":"feature","event":"pull_request","status":"completed","conclusion":"failure","run_attempt":2,"created_at":"2026-10-13T12:00:00Z","actor":{"login":"alice","type":"User"},"triggering_actor":{"login":"octocat","type":"User"}})
}
fn run() -> WorkflowRun {
    WorkflowRun {
        id: 123,
        workflow_id: 7,
        name: "CI matrix".into(),
        title: "Fix checkout".into(),
        head_sha: "a".repeat(40),
        head_branch: "feature".into(),
        event: "pull_request".into(),
        status: "completed".into(),
        conclusion: "failure".into(),
        attempt: 2,
        actor: Actor {
            login: "alice".into(),
            actor_type: "User".into(),
            ..Actor::default()
        },
        triggering_actor: Actor {
            login: "octocat".into(),
            actor_type: "User".into(),
            ..Actor::default()
        },
        created_at: "2026-10-13T12:00:00Z".into(),
        updated_at: String::new(),
        url: String::new(),
    }
}
fn workspace() -> ActionsWorkspace {
    ActionsWorkspace::new(identity(), vec!["example/repo".into()])
}
fn journal(fixture: &Fixture) -> Vec<Value> {
    fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn actions_catalog_records_cross_repo_partial_scope_without_inventing_success() {
    let fixture = Fixture::new();
    let mut unavailable = step(
        "repos/z/denied/actions/runs?per_page=100",
        Value::Null,
        true,
    );
    unavailable.status = 1;
    unavailable.stderr = b"HTTP 403: permission denied".to_vec();
    fixture.scripted_gh(&[
        step(
            "repos/example/repo/actions/runs?per_page=100",
            json!({"total_count":1,"workflow_runs":[raw_run(123)]}),
            false,
        ),
        unavailable,
    ]);
    let mut workspace =
        ActionsWorkspace::new(identity(), vec!["z/denied".into(), "example/repo".into()]);
    workspace
        .load_catalog(&provider(&fixture), &CancellationToken::default())
        .unwrap();
    assert_eq!(workspace.runs.len(), 1);
    assert!(!workspace.coverage.complete);
    assert!(workspace.coverage.reasons[0].contains("z/denied"));
    assert!(workspace.rows[0].label.contains("attempt 2"));
    assert_eq!(workspace.rows[0].role, SemanticToken::Failure);
    assert_eq!(workspace.actors()[0].login, "alice");
    assert_eq!(journal(&fixture).len(), 2);
}

#[test]
fn actions_keyboard_and_mouse_resolve_same_exact_run_attempt_at_small_and_large_sizes() {
    for mouse in [false, true] {
        for (width, height) in [(80, 24), (120, 40)] {
            let fixture = Fixture::new();
            fixture.scripted_gh(&[step("repos/example/repo/actions/runs?per_page=100",json!({"total_count":1,"workflow_runs":[raw_run(123)]}),false),step("repos/example/repo/actions/runs/123/attempts/2",raw_run(123),false),step("repos/example/repo/actions/runs/123/attempts/2/jobs?per_page=100",json!([{"jobs":[{"id":501,"run_id":123,"run_attempt":2,"head_sha":"a".repeat(40),"name":"test linux","status":"completed","conclusion":"failure","steps":[]}]}]),true)]);
            let mut workspace = workspace();
            let gh = provider(&fixture);
            workspace
                .load_catalog(&gh, &CancellationToken::default())
                .unwrap();
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            let mut hits = vec![];
            terminal
                .draw(|frame| {
                    let area = frame.area();
                    hits = draw(
                        frame,
                        &mut workspace,
                        area,
                        Theme::new(ThemeName::Dark, ColorMode::NoColor),
                        &Default::default(),
                    )
                    .hits;
                })
                .unwrap();
            let action = if mouse {
                hits.into_iter()
                    .find(|hit| matches!(hit.action, ActionsAction::OpenRun { .. }))
                    .unwrap()
                    .action
            } else {
                keyboard_action(
                    KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                    &workspace,
                )
                .unwrap()
            };
            workspace
                .apply(&gh, &action, &CancellationToken::default())
                .unwrap();
            assert_eq!(workspace.view, ActionsView::Run);
            let page = workspace.run.as_ref().unwrap();
            assert_eq!(page.attempt.run.id, 123);
            assert_eq!(page.attempt.run.attempt, 2);
            assert_eq!(page.attempt.jobs[0].id, 501);
            assert_eq!(journal(&fixture).len(), 3);
            assert!(journal(&fixture).iter().all(|row| row["matched"] == true));
        }
    }
}

#[test]
fn actions_secrets_vars_secret_plan_masks_values_in_preview_debug_and_rejects_unconfirmed_publish()
{
    let fixture = Fixture::new();
    let mut workspace = workspace();
    workspace.view = ActionsView::Secrets;
    workspace.prepare_rows();
    let fields = workspace.fields(ActionsCommand::SetSecret).unwrap();
    assert!(fields
        .iter()
        .any(|field| field.key == "value" && field.secret));
    let values = BTreeMap::from([
        ("name".into(), "DEPLOY_TOKEN".into()),
        ("value".into(), "synthetic-provider-secret".into()),
    ]);
    let plan = ActionsPlan::prepare(
        &workspace,
        ActionsCommand::SetSecret,
        &values,
        &provider(&fixture),
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(!plan
        .preview
        .join("\n")
        .contains("synthetic-provider-secret"));
    assert!(!format!("{plan:?}").contains("synthetic-provider-secret"));
    assert!(plan
        .execute(&provider(&fixture), false, &CancellationToken::default())
        .is_err());
    assert!(journal(&fixture).is_empty());
}

#[test]
fn actions_run_control_plan_targets_snapshot_ids_not_display_row_strings() {
    let fixture = Fixture::new();
    let mut workspace = workspace();
    workspace.run = Some(RunPage {
        repository: "example/repo".into(),
        attempt: RunAttempt {
            run: run(),
            jobs: vec![],
            definition_note: "dependencies unavailable".into(),
        },
        selected_job: None,
        logs: None,
        annotations: vec![],
    });
    workspace.view = ActionsView::Run;
    workspace.prepare_rows();
    workspace.rows[0].label = "run 999 attempt 99 fabricated display".into();
    let plan = ActionsPlan::prepare(
        &workspace,
        ActionsCommand::RerunAll,
        &BTreeMap::from([("debug".into(), "false".into())]),
        &provider(&fixture),
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(plan.preview.join("\n").contains("Run 123 attempt 2"));
    assert!(!plan.preview.join("\n").contains("run 999"));
    assert!(journal(&fixture).is_empty());
}

#[test]
fn actions_log_search_regex_is_bounded_and_semantic_highlights_survive_reprepare() {
    let fixture = Fixture::new();
    let mut workspace = workspace();
    workspace.view = ActionsView::Logs;
    workspace.run = Some(RunPage {
        repository: "example/repo".into(),
        attempt: RunAttempt {
            run: run(),
            jobs: vec![],
            definition_note: String::new(),
        },
        selected_job: None,
        logs: Some("compile ok\nerror 42\nerror 7\n".into()),
        annotations: vec![],
    });
    workspace.prepare_rows();
    let values = BTreeMap::from([
        ("query".into(), r"error \d+".into()),
        ("regex".into(), "true".into()),
    ]);
    let plan = ActionsPlan::prepare(
        &workspace,
        ActionsCommand::SearchLogs,
        &values,
        &provider(&fixture),
        &CancellationToken::default(),
    )
    .unwrap();
    let mut outcome = plan
        .execute(&provider(&fixture), true, &CancellationToken::default())
        .unwrap();
    assert!(outcome.message.contains("2 matching"));
    outcome.workspace.prepare_rows();
    assert_eq!(
        outcome
            .workspace
            .rows
            .iter()
            .filter(|row| row.role == SemanticToken::Changed)
            .count(),
        2
    );
    assert!(journal(&fixture).is_empty());
    let invalid = BTreeMap::from([
        ("query".into(), "[".into()),
        ("regex".into(), "true".into()),
    ]);
    assert!(ActionsPlan::prepare(
        &workspace,
        ActionsCommand::SearchLogs,
        &invalid,
        &provider(&fixture),
        &CancellationToken::default()
    )
    .is_err());
}

#[test]
fn actions_unverified_account_can_browse_but_not_compose_remote_mutations() {
    let mut workspace = workspace();
    workspace.identity.account_id = None;
    workspace.view = ActionsView::Variables;
    workspace.prepare_rows();
    assert!(workspace
        .disabled_reason(ActionsCommand::SetVariable)
        .is_some());
    assert!(workspace
        .disabled_reason(ActionsCommand::ChooseScope)
        .is_none());
    assert!(workspace.fields(ActionsCommand::SetVariable).is_err());
    workspace.identity.account_id = Some(0);
    assert!(workspace.fields(ActionsCommand::SetVariable).is_err());
}

#[test]
fn actions_paged_controls_keep_every_available_or_unavailable_action_inspectable() {
    for (width, height) in [(40, 12), (80, 24), (120, 40)] {
        let fixture = Fixture::new();
        let mut workspace = workspace();
        workspace.view = ActionsView::Run;
        workspace.run = Some(RunPage {
            repository: "example/repo".into(),
            attempt: RunAttempt {
                run: run(),
                jobs: vec![],
                definition_note: String::new(),
            },
            selected_job: None,
            logs: None,
            annotations: vec![],
        });
        workspace.prepare_rows();
        let expected = workspace
            .controls
            .iter()
            .map(|control| {
                format!(
                    "{:?}",
                    control.disabled_reason.as_ref().map_or_else(
                        || control.action.clone(),
                        |reason| ActionsAction::Unavailable(format!("{}: {reason}", control.label))
                    )
                )
            })
            .collect::<std::collections::BTreeSet<_>>();
        let mut seen = std::collections::BTreeSet::new();
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        for _ in 0..workspace.controls.len() + 1 {
            let mut next = false;
            terminal
                .draw(|frame| {
                    let output = draw(
                        frame,
                        &mut workspace,
                        frame.area(),
                        Theme::new(ThemeName::Dark, ColorMode::LowColor),
                        &Default::default(),
                    );
                    for hit in output.hits {
                        if hit.action == ActionsAction::ControlsPage(1) {
                            next = true;
                        }
                        seen.insert(format!("{:?}", hit.action));
                    }
                })
                .unwrap();
            if !next {
                break;
            }
            workspace
                .apply(
                    &provider(&fixture),
                    &ActionsAction::ControlsPage(1),
                    &CancellationToken::default(),
                )
                .unwrap();
        }
        assert!(
            expected.is_subset(&seen),
            "Missing {}x{} controls: {:?}",
            width,
            height,
            expected.difference(&seen).collect::<Vec<_>>()
        );
        assert!(journal(&fixture).is_empty());
    }
}

#[test]
fn mouse_keyboard_actions_disabled_intent_has_same_read_only_mouse_and_keyboard_notice() {
    let fixture = Fixture::new();
    let mut workspace = workspace();
    workspace.view = ActionsView::Variables;
    workspace.identity.account_id = None;
    workspace.prepare_rows();
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    let mut notice = None;
    for _ in 0..workspace.controls.len() + 1 {
        terminal
            .draw(|frame| {
                let output = draw(
                    frame,
                    &mut workspace,
                    frame.area(),
                    Theme::new(ThemeName::Dark, ColorMode::LowColor),
                    &Default::default(),
                );
                notice = output.hits.into_iter().find_map(|hit| match hit.action {
                    ActionsAction::Unavailable(reason) if reason.starts_with("Set variable:") => {
                        Some(ActionsAction::Unavailable(reason))
                    }
                    _ => None,
                });
            })
            .unwrap();
        if notice.is_some() {
            break;
        }
        workspace.controls_page += 1;
    }
    let action = notice.expect("Visible disabled control still has a shared mouse/Tab hit");
    assert!(
        matches!(&action, ActionsAction::Unavailable(reason) if reason.contains("verified account"))
    );
    workspace
        .apply(&provider(&fixture), &action, &CancellationToken::default())
        .unwrap();
    assert!(workspace
        .warnings
        .last()
        .unwrap()
        .contains("verified account"));
    assert!(workspace.fields(ActionsCommand::SetVariable).is_err());
    assert!(journal(&fixture).is_empty());
}

#[test]
fn actions_empty_scope_and_reserved_global_shortcuts_never_launch_gh() {
    let fixture = Fixture::new();
    let mut workspace = ActionsWorkspace::new(identity(), vec![]);
    workspace
        .load_catalog(&provider(&fixture), &CancellationToken::default())
        .unwrap();
    assert!(!workspace.coverage.complete);
    assert!(workspace.coverage.reasons[0].contains("No repository scope"));
    assert!(journal(&fixture).is_empty());
    for key in ['1', '2', '3', '4', '5', 's'] {
        assert!(keyboard_action(
            KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE),
            &workspace
        )
        .is_none());
    }
    assert_eq!(
        keyboard_action(
            KeyEvent::new(KeyCode::Char('W'), KeyModifiers::SHIFT),
            &workspace
        ),
        Some(ActionsAction::View(ActionsView::Workflows))
    );
}

#[test]
fn actions_cache_filter_reads_exact_key_ref_and_keeps_usage_repo_wide() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[
        step("repos/example/repo/actions/caches?per_page=100&key=cargo%2Flinux&ref=refs%2Fheads%2Fmain",json!([{"actions_caches":[{"id":71,"key":"cargo/linux/v1","ref":"refs/heads/main","version":"v1","size_in_bytes":2048}]}]),true),
        step("repos/example/repo/actions/cache/usage",json!({"active_caches_count":2,"active_caches_size_in_bytes":6144}),false),
    ]);
    let mut workspace = workspace();
    workspace.view = ActionsView::Caches;
    workspace.prepare_rows();
    let values = BTreeMap::from([
        ("key".into(), "cargo/linux".into()),
        ("ref".into(), "refs/heads/main".into()),
    ]);
    let plan = ActionsPlan::prepare(
        &workspace,
        ActionsCommand::FilterCaches,
        &values,
        &provider(&fixture),
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(journal(&fixture).is_empty());
    assert!(plan
        .execute(&provider(&fixture), false, &CancellationToken::default())
        .is_err());
    assert!(journal(&fixture).is_empty());
    let mut outcome = plan
        .execute(&provider(&fixture), true, &CancellationToken::default())
        .unwrap();
    assert_eq!(outcome.workspace.caches[0].id, 71);
    assert!(outcome
        .workspace
        .rows
        .iter()
        .any(|row| row.label.contains("6144 bytes in 2 caches (not filtered)")));
    assert!(outcome
        .workspace
        .rows
        .iter()
        .any(|row| row.label.contains("1 displayed")));
    assert!(outcome
        .workspace
        .fields(ActionsCommand::DeleteCache)
        .is_ok());
    outcome.workspace.selected = 1; // Summary row has no typed cache ID.
    assert!(outcome
        .workspace
        .fields(ActionsCommand::DeleteCache)
        .is_err());
    assert_eq!(journal(&fixture).len(), 2);
}

#[test]
fn actions_run_control_uncertain_target_blocks_remote_plans_but_keeps_reads_and_inspection() {
    let fixture = Fixture::new();
    let mut workspace = workspace();
    workspace.view = ActionsView::Run;
    workspace.run = Some(RunPage {
        repository: "example/repo".into(),
        attempt: RunAttempt {
            run: run(),
            jobs: vec![],
            definition_note: String::new(),
        },
        selected_job: None,
        logs: None,
        annotations: vec![],
    });
    workspace.mark_uncertain(
        Some(ActionsTarget::Run {
            repository: "example/repo".into(),
            id: 123,
        }),
        "Readback unavailable",
    );
    assert!(workspace.observation.is_none());
    assert!(workspace
        .controls
        .iter()
        .any(|control| control.action == ActionsAction::Reconcile));
    for command in [
        ActionsCommand::RerunAll,
        ActionsCommand::DeleteRun,
        ActionsCommand::SetSecret,
        ActionsCommand::SetVariable,
    ] {
        assert!(workspace
            .disabled_reason(command)
            .unwrap()
            .contains("refresh alone does not unlock"));
        assert!(ActionsPlan::prepare(
            &workspace,
            command,
            &BTreeMap::new(),
            &provider(&fixture),
            &CancellationToken::default()
        )
        .is_err());
    }
    for command in [
        ActionsCommand::ChooseScope,
        ActionsCommand::FilterRuns,
        ActionsCommand::FilterCaches,
        ActionsCommand::SearchLogs,
    ] {
        assert!(workspace.disabled_reason(command).is_none());
    }
    assert!(journal(&fixture).is_empty());
}

#[test]
fn actions_approvals_reviewer_history_rows_keep_canonical_gate_selection_and_disabled_hits() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[
        step("repos/example/repo/actions/runs/123/pending_deployments",json!([
            {"environment":{"id":9,"name":"production"},"current_user_can_approve":true,"reviewers":[{"type":"User","reviewer":{"id":42,"login":"octocat","type":"User"}},{"type":"Team","reviewer":{"id":8,"slug":"security","name":"Security"}}]},
            {"environment":{"id":10,"name":"restricted"},"current_user_can_approve":false}
        ]),false),
        step("repos/example/repo/actions/runs/123/approvals",json!([{ "state":"approved","comment":"ship","environments":[{"id":11,"name":"prior"}],"user":{"id":42,"login":"octocat","type":"User"}}]),false),
    ]);
    let mut workspace = workspace();
    workspace.run = Some(RunPage {
        repository: "example/repo".into(),
        attempt: RunAttempt {
            run: run(),
            jobs: vec![],
            definition_note: String::new(),
        },
        selected_job: None,
        logs: None,
        annotations: vec![],
    });
    workspace
        .apply(
            &provider(&fixture),
            &ActionsAction::View(ActionsView::Approvals),
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(journal(&fixture).len(), 2);
    let reviewer = workspace
        .rows
        .iter()
        .position(|row| {
            row.actor
                .as_ref()
                .is_some_and(|actor| actor.login == "octocat")
                && row.label.contains("Required reviewer")
        })
        .unwrap();
    workspace.select(reviewer);
    workspace.rows[reviewer].label = "Environment999 display spoof".into();
    let fields = workspace.fields(ActionsCommand::ApproveDeployment).unwrap();
    assert_eq!(fields[0].value, "9");
    assert!(workspace
        .resource_key(ActionsCommand::ApproveDeployment)
        .unwrap()
        .contains("environment/9"));
    let denied = workspace
        .rows
        .iter()
        .position(|row| row.label.starts_with("Gate #10"))
        .unwrap();
    workspace.select(denied);
    assert!(workspace.fields(ActionsCommand::ApproveDeployment).is_err());
    assert!(workspace
        .disabled_reason(ActionsCommand::RejectDeployment)
        .unwrap()
        .contains("required-reviewer"));
    assert!(workspace.controls.iter().any(|control| control.action
        == ActionsAction::Compose(ActionsCommand::ApproveDeployment)
        && control.disabled_reason.is_some()));
    for (width, height) in [(80, 24), (120, 40)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let mut found = false;
        for _ in 0..workspace.controls.len() + 1 {
            terminal.draw(|frame| {
                let output=draw(frame,&mut workspace,frame.area(),Theme::new(ThemeName::Dark,ColorMode::LowColor),&Default::default());
                found|=output.hits.iter().any(|hit|matches!(&hit.action,ActionsAction::Unavailable(reason) if reason.starts_with("Approve selected deployment:") && reason.contains("required-reviewer")));
            }).unwrap();
            if found {
                break;
            }
            workspace.controls_page += 1;
        }
        assert!(
            found,
            "Denied approval remains a meaningful mouse/Tab explanation hit at {width}x{height}"
        );
        workspace.controls_page = 0;
    }
    let history = workspace
        .rows
        .iter()
        .position(|row| row.label.contains("prior#11"))
        .unwrap();
    workspace.select(history);
    assert!(workspace
        .disabled_reason(ActionsCommand::ApproveDeployment)
        .unwrap()
        .contains("history row"));
    assert_eq!(
        keyboard_action(
            KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE),
            &workspace
        ),
        Some(ActionsAction::View(ActionsView::Approvals))
    );
    assert_eq!(
        journal(&fixture).len(),
        2,
        "All selection/rendering is provider-free"
    );
}

#[test]
fn actions_approvals_partial_acceptance_blocks_repeat_and_labels_unavailable_history() {
    let fixture = Fixture::new();
    let gate = json!({"environment":{"id":9,"name":"production"},"current_user_can_approve":true});
    let mut publish = step(
        "repos/example/repo/actions/runs/123/pending_deployments",
        json!([{"id":44,"environment":"production","creator":{"id":42,"login":"octocat"}}]),
        false,
    );
    publish.args[5] = "POST".into();
    publish.args.extend(["--input".into(), "-".into()]);
    publish.stdin =
        serde_json::to_vec(&json!({"environment_ids":[9],"state":"approved","comment":"ship"}))
            .unwrap();
    let mut unavailable = step(
        "repos/example/repo/actions/runs/123/approvals",
        Value::Null,
        false,
    );
    unavailable.status = 1;
    unavailable.stderr = b"HTTP404 history unsupported".to_vec();
    fixture.scripted_gh(&[
        step(
            "repos/example/repo/actions/runs/123/pending_deployments",
            json!([gate.clone()]),
            false,
        ),
        step(
            "repos/example/repo/actions/runs/123/approvals",
            json!([]),
            false,
        ),
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        step("user", json!({"id":42,"login":"octocat"}), false),
        step("repos/example/repo/actions/runs/123", raw_run(123), false),
        step(
            "repos/example/repo/actions/runs/123/pending_deployments",
            json!([gate.clone()]),
            false,
        ),
        publish,
        step(
            "repos/example/repo/actions/runs/123/pending_deployments",
            json!([gate]),
            false,
        ),
        step("repos/example/repo/actions/runs/123", raw_run(123), false),
        unavailable,
    ]);
    let mut workspace = workspace();
    let client = provider(&fixture);
    workspace.run = Some(RunPage {
        repository: "example/repo".into(),
        attempt: RunAttempt {
            run: run(),
            jobs: vec![],
            definition_note: String::new(),
        },
        selected_job: None,
        logs: None,
        annotations: vec![],
    });
    workspace
        .apply(
            &client,
            &ActionsAction::View(ActionsView::Approvals),
            &CancellationToken::default(),
        )
        .unwrap();
    let plan = ActionsPlan::prepare(
        &workspace,
        ActionsCommand::ApproveDeployment,
        &BTreeMap::from([("id".into(), "9".into()), ("comment".into(), "ship".into())]),
        &client,
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(plan.preview.join("\n").contains("production #9"));
    assert!(plan
        .execute(&client, false, &CancellationToken::default())
        .is_err());
    assert_eq!(journal(&fixture).len(), 2);
    let outcome = plan
        .execute(&client, true, &CancellationToken::default())
        .unwrap();
    assert!(outcome.message.contains("Decision approved accepted"));
    assert!(outcome
        .message
        .contains("selected gates no longer pending false"));
    assert!(outcome
        .workspace
        .rows
        .iter()
        .any(|row| row.label.contains("Approval history unavailable")
            && row.label.contains("unsupported")));
    assert!(outcome
        .workspace
        .disabled_reason(ActionsCommand::ApproveDeployment)
        .unwrap()
        .contains("do not repeat"));
    assert!(outcome
        .workspace
        .fields(ActionsCommand::RejectDeployment)
        .is_err());
    let rows = journal(&fixture);
    assert_eq!(rows.len(), 10);
    assert!(rows.iter().all(|row| row["matched"] == true));
    assert_eq!(
        rows.iter().filter(|row| row["args"][5] == "POST").count(),
        1
    );
}

#[test]
fn actions_artifacts_worker_prepares_text_image_binary_and_terminal_fallback_without_gh() {
    use image::ImageEncoder;
    use std::{
        io::{Cursor, Write},
        sync::Arc,
    };
    let fixture = Fixture::new();
    let mut png = vec![];
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            &[240, 40, 20, 255].repeat(64),
            8,
            8,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in [
        ("report.txt", b"safe \x1b[31mtext\x1b[0m\n".as_slice()),
        ("pixel.png", png.as_slice()),
        ("binary.bin", [0, 255, 1, 2].as_slice()),
        ("broken.png", b"\x89PNG\r\n\x1a\nbad".as_slice()),
    ] {
        writer
            .start_file(
                name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    let bytes = writer.finish().unwrap().into_inner();
    let mut workspace = workspace();
    workspace.view = ActionsView::Artifact;
    workspace.artifact = Some(ActionsArtifact {
        id: 41,
        name: "fixture-output".into(),
        run_id: 123,
        attempt: None,
        head_sha: "a".repeat(40),
        size_in_bytes: bytes.len() as u64,
        expired: false,
        created_at: String::new(),
        updated_at: String::new(),
        expires_at: String::new(),
        digest: None,
    });
    workspace.archive = Some(Arc::new(
        ArtifactArchive::inspect(bytes, None, ArtifactLimits::default()).unwrap(),
    ));
    for (index, expected) in [
        (0, "safe text"),
        (1, "8×8 original pixels"),
        (2, "00 ff 01 02"),
        (3, "Image decode unavailable"),
    ] {
        workspace
            .apply(
                &provider(&fixture),
                &ActionsAction::ArtifactEntry(index),
                &CancellationToken::default(),
            )
            .unwrap();
        assert!(workspace
            .artifact_preview
            .as_ref()
            .unwrap()
            .contains(expected));
        assert!(!workspace
            .artifact_preview
            .as_ref()
            .unwrap()
            .contains('\x1b'));
        if index == 1 {
            for (mode, width, height) in [
                (ColorMode::TrueColor, 120, 40),
                (ColorMode::LowColor, 80, 24),
                (ColorMode::NoColor, 80, 24),
            ] {
                let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                terminal
                    .draw(|frame| {
                        draw(
                            frame,
                            &mut workspace,
                            frame.area(),
                            Theme::new(ThemeName::Dark, mode),
                            &Default::default(),
                        );
                    })
                    .unwrap();
                let cells = &terminal.backend().buffer().content;
                if mode == ColorMode::TrueColor {
                    assert!(cells.iter().any(|cell| cell.symbol() == "▀"));
                } else {
                    let screen = cells.iter().map(|cell| cell.symbol()).collect::<String>();
                    assert!(screen.contains("color preview needs a truecolor terminal"));
                    assert!(!cells.iter().any(|cell| cell.symbol() == "▀"));
                }
            }
        }
    }
    let cancel = CancellationToken::default();
    cancel.cancel();
    assert!(workspace
        .apply(
            &provider(&fixture),
            &ActionsAction::ArtifactEntry(0),
            &cancel
        )
        .is_err());
    assert!(journal(&fixture).is_empty());
}

fn jobs_step(attempt: u64) -> GhStep {
    step(
        &format!("repos/example/repo/actions/runs/123/attempts/{attempt}/jobs?per_page=100"),
        json!([{"jobs":[{"id":500 + attempt,"run_id":123,"run_attempt":attempt,"head_sha":"a".repeat(40),"name":"test linux","status":"completed","conclusion":"success","steps":[]}]}]),
        true,
    )
}
fn run_state(attempt: u64, status: &str, conclusion: Value) -> Value {
    let mut run = raw_run(123);
    run["run_attempt"] = json!(attempt);
    run["status"] = json!(status);
    run["conclusion"] = conclusion;
    run
}
#[test]
fn v1_actions_controls_watch_follows_rerun_attempt_and_stops_when_finished() {
    let fixture = Fixture::new();
    let running = run_state(3, "in_progress", Value::Null);
    let finished = run_state(3, "completed", json!("success"));
    fixture.scripted_gh(&[
        step(
            "repos/example/repo/actions/runs/123/attempts/2",
            raw_run(123),
            false,
        ),
        jobs_step(2),
        step(
            "repos/example/repo/actions/runs/123",
            running.clone(),
            false,
        ),
        step(
            "repos/example/repo/actions/runs/123/attempts/3",
            running,
            false,
        ),
        jobs_step(3),
        step(
            "repos/example/repo/actions/runs/123",
            finished.clone(),
            false,
        ),
        step(
            "repos/example/repo/actions/runs/123/attempts/3",
            finished,
            false,
        ),
        jobs_step(3),
    ]);
    let gh = provider(&fixture);
    let cancel = CancellationToken::default();
    let mut workspace = workspace();
    workspace
        .apply(
            &gh,
            &ActionsAction::OpenRun {
                repository: "example/repo".into(),
                id: 123,
                attempt: 2,
            },
            &cancel,
        )
        .unwrap();
    workspace.run.as_mut().unwrap().selected_job = Some(502);
    assert!(workspace.watch_applies());
    workspace.toggle_watch().unwrap();
    assert!(workspace.watching);
    assert!(workspace
        .controls
        .iter()
        .any(|control| control.label == "[w Stop watch]"));
    workspace
        .apply(&gh, &ActionsAction::Refresh, &cancel)
        .unwrap();
    // The watch picked up the rerun's newer attempt; the old job selection is dropped.
    let page = workspace.run.as_ref().unwrap();
    assert_eq!(page.attempt.run.attempt, 3);
    assert_eq!(page.selected_job, None);
    assert!(workspace.watching);
    assert_eq!(
        workspace.watch_note.as_deref(),
        Some("Run 123 attempt 3 is in progress")
    );
    workspace
        .apply(&gh, &ActionsAction::Refresh, &cancel)
        .unwrap();
    assert!(!workspace.watching);
    assert_eq!(
        workspace.watch_note.as_deref(),
        Some("Watch ended: run 123 attempt 3 finished • success")
    );
    assert!(workspace
        .controls
        .iter()
        .any(|control| control.label == "[w Watch]"));
    assert_eq!(journal(&fixture).len(), 8);
    assert!(journal(&fixture).iter().all(|row| row["matched"] == true));
}
#[test]
fn v1_actions_controls_watch_is_explained_outside_runs_and_run_views() {
    let mut workspace = workspace();
    workspace.prepare_rows();
    assert!(workspace
        .controls
        .iter()
        .any(|control| control.label == "[w Watch]" && control.action == ActionsAction::Watch));
    workspace.view = ActionsView::Secrets;
    let reason = workspace.toggle_watch().unwrap_err();
    assert!(reason.contains("open Runs or a run"));
    assert!(!workspace.watching);
    workspace.view = ActionsView::Runs;
    workspace.toggle_watch().unwrap();
    workspace.toggle_watch().unwrap();
    assert!(!workspace.watching);
    assert_eq!(workspace.watch_note.as_deref(), Some("Watch stopped"));
}
#[test]
fn v1_actions_controls_run_list_refresh_keeps_the_selected_run() {
    let fixture = Fixture::new();
    let mut newer = raw_run(124);
    newer["created_at"] = json!("2026-10-14T12:00:00Z");
    fixture.scripted_gh(&[
        step(
            "repos/example/repo/actions/runs?per_page=100",
            json!({"total_count":1,"workflow_runs":[raw_run(123)]}),
            false,
        ),
        step(
            "repos/example/repo/actions/runs?per_page=100",
            json!({"total_count":2,"workflow_runs":[newer, raw_run(123)]}),
            false,
        ),
    ]);
    let gh = provider(&fixture);
    let cancel = CancellationToken::default();
    let mut workspace = workspace();
    workspace.load_catalog(&gh, &cancel).unwrap();
    workspace.toggle_watch().unwrap();
    workspace
        .apply(&gh, &ActionsAction::Refresh, &cancel)
        .unwrap();
    assert_eq!(workspace.runs.len(), 2);
    assert_eq!(workspace.selected, 1);
    assert_eq!(workspace.runs[workspace.selected].run.id, 123);
    assert!(workspace.watching);
}
#[test]
fn v1_actions_controls_large_artifact_opens_without_download_and_keeps_delete() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[]);
    let mut workspace = workspace();
    workspace.view = ActionsView::Artifacts;
    workspace.artifacts = vec![ActionsArtifact {
        id: 44,
        name: "release-binaries".into(),
        run_id: 123,
        attempt: None,
        head_sha: "a".repeat(40),
        size_in_bytes: 60_468_414,
        expired: false,
        created_at: String::new(),
        updated_at: String::new(),
        expires_at: "2026-10-20T12:00:00Z".into(),
        digest: None,
    }];
    workspace.prepare_rows();
    assert!(workspace.rows[0]
        .label
        .contains("release-binaries #44 • 57.7 MiB"));
    workspace
        .apply(
            &provider(&fixture),
            &ActionsAction::OpenArtifact(44),
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(workspace.view, ActionsView::Artifact);
    assert!(workspace.archive.is_none());
    assert!(journal(&fixture).is_empty(), "no download was attempted");
    let rows = workspace
        .rows
        .iter()
        .map(|row| row.label.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rows.contains("Not downloaded: 57.7 MiB is over ghtui's 16.0 MiB"));
    assert!(rows.contains("gh run download 123 -R example/repo -n release-binaries"));
    for command in [
        ActionsCommand::ExportArtifact,
        ActionsCommand::ExtractArtifact,
    ] {
        assert!(workspace
            .disabled_reason(command)
            .unwrap()
            .contains("57.7 MiB is over the 16.0 MiB in-app limit"));
    }
    assert_eq!(
        workspace.disabled_reason(ActionsCommand::DeleteArtifact),
        None
    );
    assert_eq!(
        ghtui::actions_workspace::ARTIFACT_IN_APP_LIMIT,
        ArtifactLimits::default().compressed_bytes as u64
    );
}
#[test]
fn v1_actions_controls_sizes_empty_artifacts_and_single_bracket_labels() {
    use ghtui::actions_workspace::human_bytes;
    assert_eq!(human_bytes(0), "0 B");
    assert_eq!(human_bytes(1023), "1023 B");
    assert_eq!(human_bytes(118_660), "115.9 KiB");
    assert_eq!(human_bytes(10_311_685), "9.8 MiB");
    assert_eq!(human_bytes(3 * 1024 * 1024 * 1024), "3.0 GiB");
    let mut workspace = workspace();
    workspace.view = ActionsView::Artifacts;
    workspace.prepare_rows();
    assert_eq!(
        workspace.rows[0].label,
        "This run has no artifacts (or they were already deleted)."
    );
    workspace.view = ActionsView::Runs;
    workspace.prepare_rows();
    workspace.toggle_watch().unwrap();
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| {
            let area = frame.area();
            draw(
                frame,
                &mut workspace,
                area,
                Theme::new(ThemeName::Dark, ColorMode::NoColor),
                &Default::default(),
            );
        })
        .unwrap();
    let screen = terminal
        .backend()
        .buffer()
        .content()
        .chunks(120)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(screen.contains("[r Refresh]"));
    assert!(!screen.contains("[[r Refresh]]"));
    assert!(screen.contains("[w Stop watch]"));
    assert!(screen.contains("Watching: refreshes every 5s • w stops"));
}
