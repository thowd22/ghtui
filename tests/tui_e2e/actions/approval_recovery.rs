//! Native Task57 recovery and partial-result journeys with independently verified effects.
use super::*;

fn waiting() -> Value {
    let mut run = raw_run();
    run["status"] = json!("waiting");
    run["conclusion"] = Value::Null;
    run
}
fn gates() -> Value {
    json!([{"environment":{"id":9,"name":"production"},"current_user_can_approve":true,"wait_timer":30,"reviewers":[{"type":"User","reviewer":{"id":42,"login":"octocat","type":"User"}}]}])
}
fn history() -> Value {
    json!([{"state":"approved","comment":"ship","environments":[{"id":9,"name":"production"}],"user":{"id":42,"login":"octocat","type":"User"}}])
}
fn initial_gates(run: &Value) -> Vec<GhStep> {
    let mut steps = initial();
    steps[2] = read(
        "repos/example/repo/actions/runs?per_page=100",
        json!({"total_count":1,"workflow_runs":[run]}),
        false,
    );
    steps.extend([
        read(
            "repos/example/repo/actions/runs/123/attempts/2",
            run.clone(),
            false,
        ),
        read(
            "repos/example/repo/actions/runs/123/attempts/2/jobs?per_page=100",
            json!([{"jobs":[]}]),
            true,
        ),
        read(
            "repos/example/repo/actions/runs/123/pending_deployments",
            gates(),
            false,
        ),
        read(
            "repos/example/repo/actions/runs/123/approvals",
            json!([]),
            false,
        ),
    ]);
    steps
}
fn active_account() -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        read("user", json!({"id":42,"login":"octocat"}), false),
    ]
}
fn open_gates(session: &mut Session, mouse: bool) {
    session.wait_for_screen("ghtui");
    choose(session, mouse, "Actions", b"4");
    session.wait_for_screen(&run_cell(123, 2));
    if mouse {
        session.click_visible_action(&run_cell(123, 2))
    } else {
        session.send_key(b"\r")
    }
    session.wait_for_screen("Run 123 • attempt 2");
    choose(session, mouse, "[G Gates]", b"G");
    session.wait_for_screen("Gate #9 • production");
}
fn approve(session: &mut Session, mouse: bool) {
    command(session, mouse, "Approve selected deployment");
    session.wait_for_screen("ACTIONS INTENT");
    focus_field(session, mouse, "Decision explanation", 1);
    session.paste("ship");
    choose(session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    session.wait_for_screen("production");
    if mouse {
        session.click_visible_action("[Confirm action]")
    } else {
        session.focus_visible_form_action("[Confirm action]");
        session.send_key(b"\r")
    }
}
fn close_explanation(session: &mut Session, mouse: bool) {
    choose(session, mouse, "[Esc Close]", b"\x1b");
    session
        .try_wait_for_condition(
            |screen| !screen.contains("Action unavailable"),
            "Approval explanation closed",
            std::time::Duration::from_secs(8),
        )
        .unwrap();
}
fn find_json_field(session: &mut Session, mouse: bool, field: &str) {
    for _ in 0..35 {
        if session.screen().contains(field) {
            return;
        }
        if mouse {
            session.wheel(60, 27, true)
        } else {
            session.send_key(b"\x1b[6~")
        }
        session.poll_output(std::time::Duration::from_millis(35));
    }
    session.wait_for_screen(field);
}
fn assert_single_exact_write(rows: &[Value]) {
    assert!(rows.iter().all(|row| row["matched"] == true));
    let writes = rows
        .iter()
        .filter(|row| row["args"][5] == "POST")
        .collect::<Vec<_>>();
    assert_eq!(
        writes.len(),
        1,
        "Neither refresh, inspection nor acknowledgement may resend an approval"
    );
    assert_eq!(
        writes[0]["args"][1],
        "repos/example/repo/actions/runs/123/pending_deployments"
    );
    let hex = writes[0]["stdin_hex"].as_str().unwrap();
    assert_eq!(hex.len() % 2, 0);
    let bytes = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        serde_json::from_slice::<Value>(&bytes).unwrap(),
        json!({"environment_ids":[9],"state":"approved","comment":"ship"})
    );
}

#[test]
fn actions_approvals_keyboard_uncertain_request_refresh_inspect_exact_gates_and_ack_without_retry()
{
    uncertain(false)
}
#[test]
fn actions_approvals_mouse_uncertain_request_refresh_inspect_exact_gates_and_ack_without_retry() {
    uncertain(true)
}
fn uncertain(mouse: bool) {
    let fixture = fixture();
    let unchanged = fixture.git(&["rev-parse", "HEAD"]);
    let run = waiting();
    let mut steps = initial_gates(&run);
    steps.extend(compose_identity_steps());
    steps.extend(active_account());
    steps.extend(run_guard(run.clone()));
    steps.push(read(
        "repos/example/repo/actions/runs/123/pending_deployments",
        gates(),
        false,
    ));
    let mut interrupted = write(
        "repos/example/repo/actions/runs/123/pending_deployments",
        Some(json!({"environment_ids":[9],"state":"approved","comment":"ship"})),
        Value::Null,
    );
    interrupted.status = 1;
    interrupted.stderr = b"connection closed after approval request; result unknown".to_vec();
    steps.push(interrupted);
    steps.extend(active_account());
    let unknown_count = steps.len();
    // A normal refresh observes possible effects but must not release the barrier.
    steps.extend(active_account());
    steps.extend([
        read(
            "repos/example/repo/actions/runs/123/pending_deployments",
            gates(),
            false,
        ),
        read(
            "repos/example/repo/actions/runs/123/approvals",
            history(),
            false,
        ),
    ]);
    let refresh_count = steps.len();
    steps.extend(compose_identity_steps());
    // Exact inspection separately brackets gates/history with exact run and principal reads.
    steps.extend(active_account());
    steps.extend([
        read("repos/example/repo/actions/runs/123", run.clone(), false),
        read(
            "repos/example/repo/actions/runs/123/pending_deployments",
            gates(),
            false,
        ),
        read(
            "repos/example/repo/actions/runs/123/approvals",
            history(),
            false,
        ),
        read("repos/example/repo/actions/runs/123", run.clone(), false),
    ]);
    steps.extend(active_account());
    let inspection_count = steps.len();
    steps.extend(active_account());
    steps.push(read(
        "repos/example/repo/actions/runs?per_page=100",
        json!({"total_count":1,"workflow_runs":[run]}),
        false,
    ));
    steps.extend(active_account());
    let acknowledged_count = steps.len();
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    open_gates(&mut session, mouse);
    journal(&mut session, &fixture, 7);
    approve(&mut session, mouse);
    session.wait_for_screen("result unknown");
    assert_single_exact_write(&journal(&mut session, &fixture, unknown_count));
    close_form(&mut session, mouse);
    choose(&mut session, mouse, "Refresh", b"r");
    journal(&mut session, &fixture, refresh_count);
    command(&mut session, mouse, "Approve selected deployment");
    session.wait_for_screen("Action unavailable");
    session.wait_for_screen("Original attempt #1");
    assert!(!session.screen().contains("ACTIONS INTENT"));
    assert_single_exact_write(&journal(&mut session, &fixture, refresh_count));
    close_explanation(&mut session, mouse);
    inspect_original_publication(&mut session, mouse, 1);
    assert_single_exact_write(&journal(&mut session, &fixture, inspection_count));
    find_json_field(&mut session, mouse, "\"approval_history\"");
    find_json_field(&mut session, mouse, "\"history_scope\"");
    session.wait_for_screen("Whole run ID");
    find_json_field(&mut session, mouse, "\"pending_gates\"");
    find_json_field(&mut session, mouse, "\"requested_attempt\": 2");
    find_json_field(&mut session, mouse, "\"selected_environment_ids\"");
    // Presenting exact data alone did not write, and explicit acknowledgement does not retry.
    assert_single_exact_write(&journal(&mut session, &fixture, inspection_count));
    command(&mut session, mouse, "Acknowledge inspected state");
    session.wait_for_screen("Original affected state acknowledged");
    assert_single_exact_write(&journal(&mut session, &fixture, acknowledged_count));
    let durable =
        publication_support::durable_rows(&fixture.root.join("config/ghtui/cache.sqlite3"));
    assert_eq!(durable.len(), 1);
    assert_eq!(durable[0].state, "acknowledged");
    assert_eq!(durable[0].context["run_id"], 123);
    assert_eq!(durable[0].context["attempt"], 2);
    assert_eq!(
        durable[0].target["DeploymentReview"]["environment_ids"],
        json!([9])
    );
    assert!(durable[0].receipt.is_none());
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), unchanged);
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    session.resize(80, 24);
    session.wait_for_screen("0 unresolved publications shown");
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn actions_approvals_keyboard_accepted_but_pending_and_unavailable_history_do_not_repeat() {
    partial(false)
}
#[test]
fn actions_approvals_mouse_accepted_but_pending_and_unavailable_history_do_not_repeat() {
    partial(true)
}
fn partial(mouse: bool) {
    let fixture = fixture();
    let unchanged = fixture.git(&["rev-parse", "HEAD"]);
    let run = waiting();
    let mut steps = initial_gates(&run);
    steps.extend(compose_identity_steps());
    steps.extend(active_account());
    steps.extend(run_guard(run.clone()));
    steps.extend([
        read("repos/example/repo/actions/runs/123/pending_deployments",gates(),false),
        write("repos/example/repo/actions/runs/123/pending_deployments",Some(json!({"environment_ids":[9],"state":"approved","comment":"ship"})),json!([{"id":109,"environment":"production","sha":run["head_sha"],"creator":{"id":42,"login":"octocat","type":"User"}}])),
        read("repos/example/repo/actions/runs/123/pending_deployments",gates(),false),
        read("repos/example/repo/actions/runs/123",run,false),
    ]);
    let mut unavailable = read(
        "repos/example/repo/actions/runs/123/approvals",
        Value::Null,
        false,
    );
    unavailable.status = 1;
    unavailable.stderr = b"HTTP 404: approval history unavailable on this host".to_vec();
    steps.push(unavailable);
    steps.extend(active_account());
    let accepted_count = steps.len();
    steps.extend(active_account());
    steps.extend([
        read(
            "repos/example/repo/actions/runs/123/pending_deployments",
            gates(),
            false,
        ),
        read(
            "repos/example/repo/actions/runs/123/approvals",
            history(),
            false,
        ),
    ]);
    let refresh_count = steps.len();
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    open_gates(&mut session, mouse);
    approve(&mut session, mouse);
    session.wait_for_screen("Decision approved accepted");
    session.wait_for_screen("Approval history unavailable");
    session.wait_for_screen("Gate #9 • production");
    assert!(!session.screen().contains("workflow completed"));
    assert_single_exact_write(&journal(&mut session, &fixture, accepted_count));
    command(&mut session, mouse, "Approve selected deployment");
    session.wait_for_screen("Action unavailable");
    session.wait_for_screen("Original attempt #1");
    assert!(!session.screen().contains("ACTIONS INTENT"));
    close_explanation(&mut session, mouse);
    choose(&mut session, mouse, "Refresh", b"r");
    session.wait_for_screen("Decision explanation: ship");
    assert_single_exact_write(&journal(&mut session, &fixture, refresh_count));
    command(&mut session, mouse, "Reject selected deployment");
    session.wait_for_screen("Action unavailable");
    session.wait_for_screen("Original attempt #1");
    close_explanation(&mut session, mouse);
    assert_single_exact_write(&journal(&mut session, &fixture, refresh_count));
    let durable =
        publication_support::durable_rows(&fixture.root.join("config/ghtui/cache.sqlite3"));
    assert_eq!(durable.len(), 1);
    assert_eq!(durable[0].state, "accepted_pending");
    assert_eq!(durable[0].context["run_id"], 123);
    assert_eq!(durable[0].context["attempt"], 2);
    assert_eq!(
        durable[0].receipt.as_ref().unwrap()["kind"],
        "deployment_accepted"
    );
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), unchanged);
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    session.resize(80, 24);
    session.wait_for_screen("Gate #9 • production");
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn actions_approvals_keyboard_same_login_account_reassignment_hides_private_recovery_state() {
    changed_principal(false)
}
#[test]
fn actions_approvals_mouse_same_login_account_reassignment_hides_private_recovery_state() {
    changed_principal(true)
}
fn changed_principal(mouse: bool) {
    let fixture = fixture();
    let unchanged = fixture.git(&["rev-parse", "HEAD"]);
    let run = waiting();
    let mut steps = initial_gates(&run);
    steps.extend(compose_identity_steps());
    steps.extend(active_account());
    steps.extend(run_guard(run.clone()));
    steps.push(read(
        "repos/example/repo/actions/runs/123/pending_deployments",
        gates(),
        false,
    ));
    let mut interrupted = write(
        "repos/example/repo/actions/runs/123/pending_deployments",
        Some(json!({"environment_ids":[9],"state":"approved","comment":"ship"})),
        Value::Null,
    );
    interrupted.status = 1;
    interrupted.stderr = b"connection closed after approval request; result unknown".to_vec();
    steps.push(interrupted);
    steps.extend(active_account());
    let unknown_count = steps.len();
    steps.extend(compose_identity_steps());
    steps.extend(active_account());
    steps.extend([
        read("repos/example/repo/actions/runs/123", run.clone(), false),
        read(
            "repos/example/repo/actions/runs/123/pending_deployments",
            gates(),
            false,
        ),
        read(
            "repos/example/repo/actions/runs/123/approvals",
            history(),
            false,
        ),
        read("repos/example/repo/actions/runs/123", run, false),
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        read("user", json!({"id":43,"login":"octocat"}), false),
    ]);
    steps.extend([
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        read("user", json!({"id":43,"login":"octocat"}), false),
    ]);
    let hidden_count = steps.len();
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    open_gates(&mut session, mouse);
    approve(&mut session, mouse);
    session.wait_for_screen("result unknown");
    assert_single_exact_write(&journal(&mut session, &fixture, unknown_count));
    close_form(&mut session, mouse);
    open_original_publication(&mut session, mouse, 1);
    session.wait_for_screen("private inspection hidden");
    assert_single_exact_write(&journal(&mut session, &fixture, hidden_count));
    let screen = session.screen();
    for private in [
        "Gate #9",
        "production",
        "Run 123",
        "ACTIONS INTENT",
        "READ-ONLY PREVIEW",
        "Inspect ORIGINAL Actions attempt",
        "Acknowledge inspected state",
    ] {
        assert!(
            !screen.contains(private),
            "Old-account private state survived numeric reassignment: {private}\n{screen}"
        );
    }
    let durable =
        publication_support::durable_rows(&fixture.root.join("config/ghtui/cache.sqlite3"));
    assert_eq!(durable.len(), 1);
    assert_eq!(durable[0].state, "uncertain");
    assert_eq!(durable[0].principal, "user-id:42");
    assert!(durable[0].receipt.is_none());
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), unchanged);
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    session.resize(80, 24);
    session.wait_for_screen("private inspection hidden");
    assert_single_exact_write(&journal(&mut session, &fixture, hidden_count));
    session.quit_and_assert_restored_terminal(mouse);
}
