//! Native approvals journeys, activated with the coherent approval controller slice.
use super::*;

fn waiting_run() -> Value {
    let mut run = raw_run();
    run["status"] = json!("waiting");
    run["conclusion"] = Value::Null;
    run
}
fn gate(id: u64, name: &str, eligible: bool) -> Value {
    json!({"environment":{"id":id,"name":name},"current_user_can_approve":eligible,"wait_timer":30,"wait_timer_started_at":"2026-10-13T04:00:00Z","reviewers":[{"type":"User","reviewer":{"id":42,"login":"octocat","type":"User"}},{"type":"Team","reviewer":{"id":8,"slug":"security","name":"Security team"}}]})
}
fn history(state: &str, id: u64, name: &str, comment: &str) -> Value {
    json!({"state":state,"comment":comment,"environments":[{"id":id,"name":name}],"user":{"id":42,"login":"octocat","type":"User"}})
}
fn confirm_visible(session: &mut Session, mouse: bool) {
    if mouse {
        session.click_visible_action("[Confirm action]");
    } else {
        session.focus_visible_form_action("[Confirm action]");
        session.send_key(b"\r");
    }
}
fn close_notice(session: &mut Session, mouse: bool) {
    let point = session.wait_for_owned_completed_action(
        "Action unavailable • explanation only",
        "[Esc Close]",
        &[],
    );
    if mouse {
        session.click(point.0, point.1);
    } else {
        session.send_key(b"\x1b");
    }
    session
        .try_wait_for_condition(
            |screen| !screen.contains("Action unavailable"),
            "read-only approval notice closed",
            std::time::Duration::from_secs(8),
        )
        .unwrap();
}

#[test]
fn actions_approvals_keyboard_exact_environment_decisions_history_and_denied_gate() {
    deployment_decisions(false)
}
#[test]
fn actions_approvals_mouse_exact_environment_decisions_history_and_denied_gate() {
    deployment_decisions(true)
}
fn deployment_decisions(mouse: bool) {
    let fixture = fixture();
    let unchanged = fixture.git(&["rev-parse", "HEAD"]);
    let run = waiting_run();
    let production = gate(9, "production", true);
    let staging = gate(10, "staging", true);
    let restricted = gate(11, "restricted", false);
    let all = json!([production.clone(), staging.clone(), restricted.clone()]);
    let mut steps = initial();
    steps[2] = read(
        "repos/example/repo/actions/runs?per_page=100",
        json!({"total_count":1,"workflow_runs":[run.clone()]}),
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
            all.clone(),
            false,
        ),
        read(
            "repos/example/repo/actions/runs/123/approvals",
            json!([]),
            false,
        ),
    ]);
    steps.extend(compose_identity_steps());
    let cancelled_preview_count = steps.len();
    let approved = history("approved", 9, "production", "ship");
    let rejected = history("rejected", 10, "staging", "blocked");
    let mut confirmed_counts = vec![];
    for (id, state, comment, pending, remaining, history) in [
        (
            9,
            "approved",
            "ship",
            all,
            json!([staging.clone(), restricted.clone()]),
            json!([approved.clone()]),
        ),
        (
            10,
            "rejected",
            "blocked",
            json!([staging, restricted.clone()]),
            json!([restricted]),
            json!([approved, rejected]),
        ),
    ] {
        steps.extend(compose_identity_steps());
        steps.extend(current_account());
        steps.extend(run_guard(run.clone()));
        steps.extend([
            read("repos/example/repo/actions/runs/123/pending_deployments", pending, false),
            write("repos/example/repo/actions/runs/123/pending_deployments", Some(json!({"environment_ids":[id],"state":state,"comment":comment})), json!([{"id":100+id,"environment":if id==9 {"production"} else {"staging"},"sha":run["head_sha"],"creator":{"id":42,"login":"octocat","type":"User"}}])),
            read("repos/example/repo/actions/runs/123/pending_deployments", remaining, false),
            read("repos/example/repo/actions/runs/123", run.clone(), false),
            read("repos/example/repo/actions/runs/123/approvals", history, false),
        ]);
        steps.extend(current_account());
        confirmed_counts.push(steps.len());
    }
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "Actions", b"4");
    session.wait_for_screen("example/repo run 123");
    if mouse {
        session.click_visible_action("example/repo run 123");
    } else {
        session.send_key(b"\r");
    }
    session.wait_for_screen("Run 123 • attempt 2");
    choose(&mut session, mouse, "[G Gates]", b"G");
    session.wait_for_screen("Gate #9 • production");
    session.wait_for_screen("@octocat");
    session.wait_for_screen("Required team: security");
    session.wait_for_screen("server eligibility decides");
    journal(&mut session, &fixture, 7);
    command(&mut session, mouse, "Approve selected deployment");
    session.wait_for_screen("ACTIONS INTENT");
    focus_field(&mut session, mouse, "Decision explanation", 1);
    session.paste("ship");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    session.wait_for_screen("production");
    close_form(&mut session, mouse);
    journal(&mut session, &fixture, cancelled_preview_count);
    command(&mut session, mouse, "Approve selected deployment");
    session.wait_for_screen("ACTIONS INTENT");
    // The cancelled explanation remains local. It is not an already submitted effect.
    session.wait_for_screen("ship");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    confirm_visible(&mut session, mouse);
    session.wait_for_screen("Decision approved accepted");
    session.wait_for_screen("Gate #10 • staging");
    session.wait_for_screen("production#9");
    session.wait_for_screen("Decision explanation: ship");
    journal(&mut session, &fixture, confirmed_counts[0]);
    command(&mut session, mouse, "Reject selected deployment");
    session.wait_for_screen("ACTIONS INTENT");
    focus_field(&mut session, mouse, "Decision explanation", 1);
    session.paste("blocked");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    session.wait_for_screen("staging");
    confirm_visible(&mut session, mouse);
    session.wait_for_screen("Decision rejected accepted");
    session.wait_for_screen("Gate #11 • restricted");
    session.wait_for_screen("Decision explanation: blocked");
    let rows = journal(&mut session, &fixture, confirmed_counts[1]);
    let writes = rows
        .iter()
        .filter(|row| row["args"][5] == "POST")
        .collect::<Vec<_>>();
    assert_eq!(writes.len(), 2);
    let payloads = writes
        .iter()
        .map(|row| {
            let hex = row["stdin_hex"].as_str().unwrap();
            assert_eq!(hex.len() % 2, 0);
            let bytes = hex
                .as_bytes()
                .chunks_exact(2)
                .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
                .collect::<Vec<_>>();
            serde_json::from_slice::<Value>(&bytes).unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        payloads[0],
        json!({"environment_ids":[9],"state":"approved","comment":"ship"})
    );
    assert_eq!(
        payloads[1],
        json!({"environment_ids":[10],"state":"rejected","comment":"blocked"})
    );
    command(&mut session, mouse, "Approve selected deployment");
    session.wait_for_screen("Action unavailable");
    session.wait_for_screen("required-reviewer");
    assert!(!session.screen().contains("ACTIONS INTENT"));
    journal(&mut session, &fixture, confirmed_counts[1]);
    let durable =
        publication_support::durable_rows(&fixture.root.join("config/ghtui/cache.sqlite3"));
    assert_eq!(durable.len(), 2);
    assert!(durable.iter().all(|row| row.state == "accepted_pending"
        && row.context["run_id"] == 123
        && row.context["attempt"] == 2
        && row.context["head_sha"] == "a".repeat(40)));
    assert_eq!(durable[0].kind, json!("approve_deployment"));
    assert_eq!(durable[1].kind, json!("reject_deployment"));
    assert_eq!(
        durable[0].target["DeploymentReview"]["environment_ids"],
        json!([9])
    );
    assert_eq!(
        durable[1].target["DeploymentReview"]["environment_ids"],
        json!([10])
    );
    assert!(durable
        .iter()
        .all(|row| row.receipt.as_ref().unwrap()["kind"] == "deployment_accepted"));
    close_notice(&mut session, mouse);
    session.resize(80, 24);
    session.wait_for_screen("Gate #11 • restricted");
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), unchanged);
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn actions_approvals_keyboard_fork_approval_cancel_preview_and_confirm_exact_run() {
    fork_approval(false)
}
#[test]
fn actions_approvals_mouse_fork_approval_cancel_preview_and_confirm_exact_run() {
    fork_approval(true)
}
fn fork_approval(mouse: bool) {
    fork_approval_observed_footer(mouse, false);
}
#[test]
fn owned_input_frames_v1_keyboard_approval_actual_partial_footer_no_underlay_input() {
    fork_approval_observed_footer(false, true);
}
#[test]
fn owned_input_frames_v1_mouse_approval_actual_partial_footer_no_underlay_input() {
    fork_approval_observed_footer(true, true);
}
fn fork_approval_observed_footer(mouse: bool, controlled: bool) {
    let fixture = fixture();
    let mut awaiting = raw_run();
    awaiting["status"] = json!("completed");
    awaiting["conclusion"] = json!("action_required");
    let mut queued = awaiting.clone();
    queued["status"] = json!("queued");
    queued["conclusion"] = Value::Null;
    let mut steps = initial();
    steps[2] = read(
        "repos/example/repo/actions/runs?per_page=100",
        json!({"total_count":1,"workflow_runs":[awaiting.clone()]}),
        false,
    );
    steps.extend([
        read(
            "repos/example/repo/actions/runs/123/attempts/2",
            awaiting.clone(),
            false,
        ),
        read(
            "repos/example/repo/actions/runs/123/attempts/2/jobs?per_page=100",
            json!([{"jobs":[]}]),
            true,
        ),
    ]);
    steps.extend(compose_identity_steps());
    let cancelled_count = steps.len();
    steps.extend(compose_identity_steps());
    steps.extend(current_account());
    steps.extend(run_guard(awaiting));
    steps.extend([
        write(
            "repos/example/repo/actions/runs/123/approve",
            None,
            Value::Null,
        ),
        read("repos/example/repo/actions/runs/123", queued, false),
    ]);
    steps.extend(current_account());
    let confirmed_count = steps.len();
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "Actions", b"4");
    session.wait_for_screen("example/repo run 123");
    if mouse {
        session.click_visible_action("example/repo run 123");
    } else {
        session.send_key(b"\r");
    }
    session.wait_for_screen("Run 123 • attempt 2");
    command(&mut session, mouse, "Approve fork run");
    session.wait_for_screen("ACTIONS INTENT");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    session.wait_for_screen("Run 123 attempt 2");
    close_form(&mut session, mouse);
    journal(&mut session, &fixture, cancelled_count);
    command(&mut session, mouse, "Approve fork run");
    session.wait_for_screen("ACTIONS INTENT");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    confirm_visible(&mut session, mouse);
    session.wait_for_screen("requested state transition was observed");
    session.wait_for_screen("queued");
    let rows = journal(&mut session, &fixture, confirmed_count);
    assert_eq!(
        rows.iter().filter(|row| row["args"][5] == "POST").count(),
        1
    );
    assert!(rows
        .iter()
        .any(|row| row["args"][1] == "repos/example/repo/actions/runs/123/approve"));
    if controlled {
        session.hold_observed_output_after_literal_v1("Action unavailable • explanation only");
    }
    command(&mut session, mouse, "Approve fork run");
    session.wait_for_screen("Action unavailable");
    session.wait_for_screen("Original attempt #1");
    journal(&mut session, &fixture, confirmed_count);
    let durable =
        publication_support::durable_rows(&fixture.root.join("config/ghtui/cache.sqlite3"));
    assert_eq!(durable.len(), 1);
    assert_eq!(durable[0].kind, json!("approve_fork"));
    assert_eq!(durable[0].state, "accepted_pending");
    assert_eq!(durable[0].context["run_id"], 123);
    assert_eq!(durable[0].context["attempt"], 2);
    assert_eq!(
        durable[0].receipt.as_ref().unwrap()["kind"],
        "run_transition"
    );
    let close_started_after = session.completed_cursorless_end();
    let writes_before_close = controlled.then(|| session.observed_output_write_count_v1());
    let closed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        close_notice(&mut session, mouse)
    }));
    if controlled {
        let writes = session.finish_observed_output_hold_v1();
        let closing = &writes[writes_before_close.unwrap()..];
        eprintln!("Actual post-title closing journal after completed endpoint{close_started_after}: {closing:?}");
        if let Err(payload) = closed {
            std::panic::resume_unwind(payload);
        }
        assert_eq!(
            closing.len(),
            1,
            "Partial-title/footer wait must never page underlay"
        );
        assert!(
            closing[0].completed_cells_end > close_started_after,
            "Close requires the genuine completed overlay, not its partial title"
        );
    } else if let Err(payload) = closed {
        std::panic::resume_unwind(payload);
    }
    session.resize(80, 24);
    session.wait_for_screen("Run 123 • attempt 2");
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    session.quit_and_assert_restored_terminal(mouse);
}
