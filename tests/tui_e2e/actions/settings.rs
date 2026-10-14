//! Native Task60 policy, environment and runner journeys with independently verified effects.
use super::*;

fn put(endpoint: &str, input: Value, output: Value) -> GhStep {
    let mut request = write(endpoint, Some(input), output);
    request.args[5] = "PUT".into();
    request
}
fn account_steps() -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        read("user", json!({"id":42,"login":"octocat"}), false),
    ]
}
fn next_field(session: &mut Session, mouse: bool, label: &str) {
    // A later field can already be visible at 120x40. Wait for the actual
    // viewport movement, not merely the old label, before clicking its choice.
    let previous_row = session
        .screen()
        .lines()
        .position(|line| line.contains(label));
    choose(session, mouse, "[F6 Fields↓]", b"\x1b[17~");
    session
        .try_wait_for_condition(
            |screen| {
                screen
                    .lines()
                    .position(|line| line.contains(label))
                    .is_some_and(|row| Some(row) != previous_row)
            },
            &format!("Field viewport moved to {label}"),
            std::time::Duration::from_secs(8),
        )
        .unwrap();
}
fn confirm_preview(session: &mut Session, mouse: bool) {
    if mouse {
        session.click_visible_action("[Confirm action]");
    } else {
        session.focus_visible_form_action("[Confirm action]");
        session.send_key(b"\r");
    }
}
fn select_row(session: &mut Session, mouse: bool, label: &str) {
    session.wait_for_screen(label);
    if mouse {
        session.click_visible_action(label);
        session
            .try_wait_for_condition(
                |screen| {
                    screen
                        .lines()
                        .any(|line| line.contains(label) && line.contains('>'))
                },
                &format!("Exact visible runner {label} gained mouse selection"),
                std::time::Duration::from_secs(8),
            )
            .unwrap();
        return;
    }
    session.send_key(b"\x1b[H");
    for _ in 0..20 {
        session.poll_output(std::time::Duration::from_millis(40));
        if session
            .screen()
            .lines()
            .any(|line| line.contains(label) && line.contains('>'))
        {
            return;
        }
        session.send_key(b"\x1b[B");
    }
    panic!("Exact visible runner {label} did not gain keyboard selection");
}
fn stdin_json(row: &Value) -> Value {
    let hex = row["stdin_hex"].as_str().unwrap();
    assert_eq!(hex.len() % 2, 0);
    let bytes = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect::<Vec<_>>();
    serde_json::from_slice(&bytes).unwrap()
}

#[test]
fn actions_settings_keyboard_policy_environment_runner_labels_and_cancelled_removal() {
    settings(false)
}
#[test]
fn actions_settings_mouse_policy_environment_runner_labels_and_cancelled_removal() {
    settings(true)
}
fn settings(mouse: bool) {
    let fixture = fixture();
    let unchanged = fixture.git(&["rev-parse", "HEAD"]);
    let token =
        json!({"default_workflow_permissions":"read","can_approve_pull_request_reviews":false});
    let changed_token =
        json!({"default_workflow_permissions":"write","can_approve_pull_request_reviews":true});
    let environment =
        json!({"id":9,"name":"prod/eu","protection_rules":[],"deployment_branch_policy":null});
    let changed_environment = json!({"id":9,"name":"prod/eu","protection_rules":[{"type":"wait_timer","wait_timer":30},{"type":"required_reviewers","prevent_self_review":true,"reviewers":[{"type":"User","reviewer":{"id":42,"login":"octocat","type":"User"}}]}],"deployment_branch_policy":{"protected_branches":true,"custom_branch_policies":false}});
    let runner = json!({"id":9,"name":"builder","os":"linux","status":"online","busy":false,"labels":[{"id":1,"name":"self-hosted","type":"read-only"},{"id":2,"name":"old","type":"custom"}]});
    let changed_runner = json!({"id":9,"name":"builder","os":"linux","status":"online","busy":false,"labels":[{"id":1,"name":"self-hosted","type":"read-only"},{"id":3,"name":"gpu","type":"custom"},{"id":4,"name":"fast","type":"custom"}]});
    let busy = json!({"id":10,"name":"overloaded","os":"linux","status":"online","busy":true,"labels":[{"id":1,"name":"self-hosted","type":"read-only"}]});
    let mut steps = initial();
    steps.extend([
        read(
            "repos/example/repo/actions/permissions",
            json!({"enabled":true,"allowed_actions":"all","sha_pinning_required":false}),
            false,
        ),
        read(
            "repos/example/repo/actions/permissions/workflow",
            token.clone(),
            false,
        ),
    ]);
    let policy_catalogue_count = steps.len();
    steps.extend(compose_identity_steps());
    let policy_open_count = steps.len();
    steps.extend(current_account());
    steps.extend(account_steps());
    steps.extend([
        read(
            "repos/example/repo/actions/permissions/workflow",
            token,
            false,
        ),
        put(
            "repos/example/repo/actions/permissions/workflow",
            changed_token.clone(),
            Value::Null,
        ),
        read(
            "repos/example/repo/actions/permissions/workflow",
            changed_token,
            false,
        ),
    ]);
    steps.extend(current_account());
    let policy_confirmed_count = steps.len();
    steps.push(read(
        "repos/example/repo/environments?per_page=100",
        json!([{"environments":[environment.clone()]}]),
        true,
    ));
    steps.extend(compose_identity_steps());
    let environment_open_count = steps.len();
    steps.extend(current_account());
    steps.extend(account_steps());
    steps.extend([
        read("repos/example/repo/environments/prod%2Feu",environment,false),
        put("repos/example/repo/environments/prod%2Feu",json!({"wait_timer":30,"prevent_self_review":true,"reviewers":[{"type":"User","id":42}],"deployment_branch_policy":{"protected_branches":true,"custom_branch_policies":false}}),changed_environment.clone()),
        read("repos/example/repo/environments/prod%2Feu",changed_environment,false),
    ]);
    steps.extend(current_account());
    let environment_confirmed_count = steps.len();
    steps.push(read(
        "repos/example/repo/actions/runners?per_page=100",
        json!([{"runners":[runner.clone(),busy]}]),
        true,
    ));
    steps.extend(compose_identity_steps());
    let runner_open_count = steps.len();
    steps.extend(current_account());
    steps.extend(account_steps());
    steps.extend([
        read("repos/example/repo/actions/runners/9", runner, false),
        put(
            "repos/example/repo/actions/runners/9/labels",
            json!({"labels":["gpu","fast"]}),
            json!({"labels":changed_runner["labels"]}),
        ),
        read(
            "repos/example/repo/actions/runners/9",
            changed_runner,
            false,
        ),
    ]);
    steps.extend(current_account());
    steps.extend(compose_identity_steps());
    let final_count = steps.len();
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "Actions", b"4");
    session.wait_for_screen("example/repo run 123");
    choose(&mut session, mouse, "Policies", b"p");
    session.wait_for_screen("\"enabled\": true");
    command(&mut session, mouse, "Workflow token");
    session.wait_for_screen("default_workflow_permissions");
    journal(&mut session, &fixture, policy_catalogue_count);
    command(&mut session, mouse, "Edit selected Actions policy");
    session.wait_for_screen("ACTIONS INTENT");
    if mouse {
        session.click_visible_action("[write]")
    } else {
        session.send_key(b" ")
    }
    next_field(&mut session, mouse, "May workflows approve PR reviews?");
    if mouse {
        session.click_visible_action("[true]")
    } else {
        session.send_key(b" ")
    }
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    session.wait_for_screen("Before: selected Actions policy");
    session.wait_for_screen("After: explicitly requested policy fields");
    journal(&mut session, &fixture, policy_open_count);
    confirm_preview(&mut session, mouse);
    session.wait_for_screen("Actions policy changed and read-back confirmed");
    journal(&mut session, &fixture, policy_confirmed_count);
    choose(&mut session, mouse, "Environments", b"e");
    session.wait_for_screen("Environment #9 • prod/eu");
    command(&mut session, mouse, "Edit selected environment protection");
    session.wait_for_screen("ACTIONS INTENT");
    session.resize(80, 24);
    next_field(&mut session, mouse, "Wait timer minutes");
    session.send_key(b"\x15");
    session.paste("30");
    next_field(&mut session, mouse, "Prevent self-review?");
    if mouse {
        session.click_visible_action("[true]")
    } else {
        session.send_key(b" ")
    }
    next_field(&mut session, mouse, "Reviewer identities");
    session.paste("User:42");
    next_field(&mut session, mouse, "Allowed deployment branches");
    if mouse {
        session.click_visible_action("[protected]")
    } else {
        session.send_key(b" ")
    }
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    journal(&mut session, &fixture, environment_open_count);
    confirm_preview(&mut session, mouse);
    session.wait_for_screen("Environment protection changed and read-back confirmed");
    session.wait_for_screen("Wait timer: 30 minutes");
    session.resize(120, 40);
    session.wait_for_screen("@octocat");
    session.wait_for_screen("Deployment branches: only protected branches");
    journal(&mut session, &fixture, environment_confirmed_count);
    choose(&mut session, mouse, "Runners", b"n");
    session.wait_for_screen("#9 • builder");
    command(&mut session, mouse, "Replace custom runner labels");
    session.wait_for_screen("ACTIONS INTENT");
    next_field(&mut session, mouse, "Custom labels, comma-separated");
    session.send_key(b"\x15");
    session.paste("gpu,fast");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    journal(&mut session, &fixture, runner_open_count);
    confirm_preview(&mut session, mouse);
    session.wait_for_screen("Custom runner labels replaced; built-in labels preserved");
    session.wait_for_screen("self-hosted (read-only), gpu (custom), fast (custom)");
    select_row(&mut session, mouse, "#9 • builder");
    command(&mut session, mouse, "Remove selected runner");
    session.wait_for_screen("ACTIONS INTENT");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    close_form(&mut session, mouse);
    let rows = journal(&mut session, &fixture, final_count);
    let writes = rows
        .iter()
        .filter(|row| row["args"][5] == "PUT")
        .collect::<Vec<_>>();
    assert_eq!(writes.len(), 3);
    assert_eq!(
        stdin_json(writes[0]),
        json!({"default_workflow_permissions":"write","can_approve_pull_request_reviews":true})
    );
    assert_eq!(
        stdin_json(writes[1]),
        json!({"wait_timer":30,"prevent_self_review":true,"reviewers":[{"type":"User","id":42}],"deployment_branch_policy":{"protected_branches":true,"custom_branch_policies":false}})
    );
    assert_eq!(stdin_json(writes[2]), json!({"labels":["gpu","fast"]}));
    assert!(!rows.iter().any(|row| row["args"][5] == "DELETE"));
    select_row(&mut session, mouse, "#10 • overloaded");
    command(&mut session, mouse, "Remove selected runner");
    session.wait_for_screen("Action unavailable");
    session.wait_for_screen("runner is busy");
    assert!(!session.screen().contains("ACTIONS INTENT"));
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session
        .try_wait_for_condition(
            |screen| !screen.contains("Action unavailable"),
            "Busy-runner explanation closed",
            std::time::Duration::from_secs(8),
        )
        .unwrap();
    journal(&mut session, &fixture, final_count);
    let durable =
        publication_support::durable_rows(&fixture.root.join("config/ghtui/cache.sqlite3"));
    assert_eq!(durable.len(), 3);
    assert!(durable.iter().all(|row| row.state == "confirmed"));
    assert_eq!(durable[0].kind, json!("edit_policy"));
    assert_eq!(durable[1].kind, json!("edit_environment"));
    assert_eq!(durable[2].kind, json!("runner_labels"));
    assert_eq!(
        durable[1].target["EnvironmentProtection"]["environment_id"],
        9
    );
    assert_eq!(durable[2].target["Runner"]["id"], 9);
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), unchanged);
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    session.resize(80, 24);
    session.wait_for_screen("#10 • overloaded");
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn actions_settings_keyboard_changed_environment_id_requires_exact_inspection_and_ack_no_retry() {
    changed_environment_identity(false)
}
#[test]
fn actions_settings_mouse_changed_environment_id_requires_exact_inspection_and_ack_no_retry() {
    changed_environment_identity(true)
}
fn find_json(session: &mut Session, mouse: bool, field: &str) {
    for _ in 0..30 {
        if session.screen().contains(field) {
            return;
        }
        if mouse {
            session.wheel(60, 27, true);
        } else {
            session.send_key(b"\x1b[6~");
        }
        session.poll_output(std::time::Duration::from_millis(35));
    }
    session.wait_for_screen(field);
}
fn changed_environment_identity(mouse: bool) {
    let fixture = fixture();
    let unchanged = fixture.git(&["rev-parse", "HEAD"]);
    let before =
        json!({"id":9,"name":"prod/eu","protection_rules":[],"deployment_branch_policy":null});
    let other = json!({"id":99,"name":"prod/eu","protection_rules":[{"type":"wait_timer","wait_timer":30}],"deployment_branch_policy":null});
    let mut steps = initial();
    steps.push(read(
        "repos/example/repo/environments?per_page=100",
        json!([{"environments":[before.clone()]}]),
        true,
    ));
    steps.extend(compose_identity_steps());
    steps.extend(account_steps());
    steps.extend(account_steps());
    steps.extend([
        read("repos/example/repo/environments/prod%2Feu",before,false),
        put("repos/example/repo/environments/prod%2Feu",json!({"wait_timer":30,"prevent_self_review":false,"reviewers":[],"deployment_branch_policy":null}),other.clone()),
        read("repos/example/repo/environments/prod%2Feu",other.clone(),false),
    ]);
    steps.extend(account_steps());
    let unknown_count = steps.len();
    // Refresh observes the reused name but never releases the original write barrier.
    steps.extend(account_steps());
    steps.push(read(
        "repos/example/repo/environments?per_page=100",
        json!([{"environments":[other.clone()]}]),
        true,
    ));
    let refresh_count = steps.len();
    steps.extend(compose_identity_steps()); // Fresh principal on Publications open.
                                            // Inspection explicitly keeps requested ID9 separate from current name owner99.
    steps.extend(account_steps());
    steps.push(read(
        "repos/example/repo/environments?per_page=100",
        json!([{"environments":[other]}]),
        true,
    ));
    steps.extend(account_steps());
    let inspection_count = steps.len();
    steps.extend(account_steps());
    steps.push(read(
        "repos/example/repo/actions/runs?per_page=100",
        json!([{"total_count":1,"workflow_runs":[raw_run()]}]),
        true,
    ));
    steps.extend(account_steps());
    let acknowledged_count = steps.len();
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "Actions", b"4");
    session.wait_for_screen("example/repo run 123");
    choose(&mut session, mouse, "Environments", b"e");
    session.wait_for_screen("Environment #9 • prod/eu");
    command(&mut session, mouse, "Edit selected environment protection");
    session.wait_for_screen("ACTIONS INTENT");
    next_field(&mut session, mouse, "Wait timer minutes");
    session.send_key(b"\x15");
    session.paste("30");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    confirm_preview(&mut session, mouse);
    session.wait_for_screen("Environment identity changed");
    journal(&mut session, &fixture, unknown_count);
    close_form(&mut session, mouse);
    choose(&mut session, mouse, "Refresh", b"r");
    session.wait_for_screen("Environment #99 • prod/eu");
    journal(&mut session, &fixture, refresh_count);
    command(&mut session, mouse, "Edit selected environment protection");
    session.wait_for_screen("Action unavailable");
    session.wait_for_screen("Original attempt #1");
    assert!(!session.screen().contains("ACTIONS INTENT"));
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session
        .try_wait_for_condition(
            |screen| !screen.contains("Action unavailable"),
            "Environment uncertainty explanation closed",
            std::time::Duration::from_secs(8),
        )
        .unwrap();
    inspect_original_publication(&mut session, mouse, 1);
    journal(&mut session, &fixture, inspection_count);
    find_json(&mut session, mouse, "\"current_by_name\"");
    session.wait_for_screen("\"id\": 99");
    find_json(&mut session, mouse, "\"original_by_id\": null");
    find_json(&mut session, mouse, "\"requested_environment_id\": 9");
    command(&mut session, mouse, "Acknowledge inspected state");
    session.wait_for_screen("Original affected state acknowledged");
    let rows = journal(&mut session, &fixture, acknowledged_count);
    let writes = rows
        .iter()
        .filter(|row| row["args"][5] == "PUT")
        .collect::<Vec<_>>();
    assert_eq!(writes.len(), 1);
    assert_eq!(
        writes[0]["args"][1],
        "repos/example/repo/environments/prod%2Feu"
    );
    assert_eq!(
        stdin_json(writes[0]),
        json!({"wait_timer":30,"prevent_self_review":false,"reviewers":[],"deployment_branch_policy":null})
    );
    assert!(!rows
        .iter()
        .any(|row| row["args"][5] == "POST" || row["args"][5] == "DELETE"));
    let durable =
        publication_support::durable_rows(&fixture.root.join("config/ghtui/cache.sqlite3"));
    assert_eq!(durable.len(), 1);
    assert_eq!(durable[0].state, "acknowledged");
    assert_eq!(
        durable[0].target["EnvironmentProtection"]["environment_id"],
        9
    );
    assert!(durable[0].receipt.is_none());
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), unchanged);
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    session.resize(80, 24);
    session.wait_for_screen("0 unresolved publications shown");
    session.quit_and_assert_restored_terminal(mouse);
}
