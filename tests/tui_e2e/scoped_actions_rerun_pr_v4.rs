//! SOURCE ONLY / UNRUN: direct INITIAL genuine PR run -> selected-job rerun.
//! No resource detour, Catalogue/Files fabrication, live GitHub writes or URL IDs.
#[path = "scoped_actions_retained_run_history_v8.rs"]
mod retained_history_v8;
use super::*;
const FORM: &str = "Rerun selected job • original Actions";
const HISTORY: &str = "Original Rerun selected job publication";
const H: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
fn phase() -> Vec<GhStep> {
    let mut out = principal(42);
    out.push(get(&format!("repos/{FIRST}"), repository(0)));
    out.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    out.push(get(
        &format!("repos/{FIRST}/check-runs/600"),
        selected_check(),
    ));
    out.push(get(
        &format!("repos/{FIRST}/actions/runs/123"),
        observed_run(),
    ));
    out.push(get(
        &format!("repos/{FIRST}/actions/runs/123"),
        observed_run(),
    ));
    out.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    out.push(get(
        &format!("repos/{FIRST}/check-runs/600"),
        selected_check(),
    ));
    out.push(get(&format!("repos/{FIRST}"), repository(0)));
    out.extend(principal(42));
    assert_eq!(out.len(), 12);
    out
}
fn confirm() -> Vec<GhStep> {
    let mut out = principal(42);
    out.push(get(&format!("repos/{FIRST}"), repository(0)));
    out.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    out.push(get(
        &format!("repos/{FIRST}/check-runs/600"),
        selected_check(),
    ));
    out.push(get(
        &format!("repos/{FIRST}/actions/runs/123"),
        observed_run(),
    ));
    out.extend(principal(42));
    out.push(get(
        &format!("repos/{FIRST}/actions/runs/123"),
        observed_run(),
    ));
    out.push(get(
        &format!("repos/{FIRST}/actions/jobs/501"),
        observed_job(),
    )); // genuine omitted A
    out.push(get(
        &format!("repos/{FIRST}/actions/runs/123/attempts/2"),
        observed_run(),
    ));
    let other: Vec<Value> = (1000..1100)
        .map(|id| {
            let mut raw = observed_job();
            raw["id"] = json!(id);
            raw["node_id"] = json!(format!("J_{id}"));
            raw["check_run_url"] = json!(format!(
                "https://{HOST}/api/v3/repos/{FIRST}/check-runs/{id}"
            ));
            raw
        })
        .collect();
    out.push(get(
        &format!("repos/{FIRST}/actions/runs/123/attempts/2/jobs?per_page=100&page=1"),
        json!({"total_count":101,"jobs":other}),
    ));
    out.push(get(
        &format!("repos/{FIRST}/actions/runs/123/attempts/2/jobs?per_page=100&page=2"),
        json!({"total_count":101,"jobs":[observed_job()]}),
    ));
    out.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    out.push(get(
        &format!("repos/{FIRST}/check-runs/600"),
        selected_check(),
    ));
    out.push(get(
        &format!("repos/{FIRST}/actions/runs/123"),
        observed_run(),
    ));
    out.push(get(&format!("repos/{FIRST}"), repository(0)));
    out.extend(principal(42));
    let mut post = GhStep::read(
        &[
            "api",
            &format!("repos/{FIRST}/actions/jobs/501/rerun"),
            "--hostname",
            HOST,
            "--method",
            "POST",
            "--input",
            "-",
        ],
        "{}",
    );
    post.stdin = serde_json::to_vec(&json!({"enable_debug_logging":false})).unwrap();
    out.push(post);
    let mut queued = observed_run();
    queued["run_attempt"] = json!(3);
    queued["status"] = json!("queued");
    queued["conclusion"] = Value::Null;
    out.push(get(&format!("repos/{FIRST}/actions/runs/123"), queued));
    out.push(who(42));
    assert_eq!(
        out.len(),
        22,
        "Actual max PR selected-job cohort, not a prediction from reduced children"
    );
    out
}
fn complete_cells(session: &mut Session, after: usize, title: &str, words: &[&str]) {
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        if session.completed_cells_end() > after
            && session
                .completed_cells_screen()
                .and_then(|f| crate::support::overlay_body(f, title))
                .is_some_and(|body| words.iter().all(|word| body.contains(word)))
        {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "NEW owned PR rerun frame {title}: {words:?}\n{}",
            session.screen()
        );
    }
}
fn form_control(session: &mut Session, mouse: bool, title: &str, label: &str) {
    if mouse {
        let point = session.wait_for_owned_completed_action(title, label, &[]);
        session.click(point.0, point.1);
    } else {
        session.focus_completed_owned_action_v1(
            title,
            label,
            crate::support::completed_navigation_controls_v1::FocusAttributeV1::Inverse,
            if title == FORM {
                &["[Preview]", "[Confirm action]", "[Keep draft / Close]"]
            } else {
                &[
                    "[View original]",
                    "[F4 Acknowledge]",
                    "[Keep draft / Close]",
                ]
            },
            &[],
        );
        session.send_key(b"\r");
    }
}
fn unrelated(fixture: &Fixture) -> Vec<String> {
    let db = raw(fixture);
    ["SELECT json_array(host,login,resource,body) FROM drafts WHERE resource NOT LIKE 'scoped-actions-v3/%' ORDER BY host,login,resource",
    "SELECT json_array(host,login,name,payload) FROM saved_views ORDER BY host,login,name",
    "SELECT json_array(host,login,source,generation,complete) FROM checkpoints ORDER BY host,login,source",
    "SELECT json_array(host,login,kind,id,payload) FROM records ORDER BY host,login,kind,id",
    "SELECT json_array(attempt_id,state) FROM pr_write_attempts ORDER BY attempt_id"]
        .iter().flat_map(|q|db.prepare(q).unwrap().query_map([],|r|r.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>()).collect()
}
fn happy(mouse: bool) {
    let mut expected = steps(vec![]);
    assert_eq!(expected.len(), 61);
    expected.extend(phase());
    expected.extend(phase());
    expected.extend(confirm());
    assert_eq!(expected.len(), 107);
    let fixture = resource_configured(&expected);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    open_run(&mut session, &fixture, mouse);
    let sql = unrelated(&fixture);
    let trace = git_commands(&fixture);
    // No A2 resource/bootstrap route was invoked: all59 are actual PR/Checks.
    assert_eq!(commands(&session, &fixture).len(), 61);
    let after = session.completed_cells_end();
    if mouse {
        control(&mut session, true, "Rerun selected job", b"");
    } else {
        session.focus_completed_actions_control_v2("Rerun selected job");
        session.send_key(b"\r");
    }
    session.wait_for_command_count(&fixture, 73);
    complete_cells(
        &mut session,
        after,
        FORM,
        &[
            "ORIGINAL RERUN INTENT",
            "Original selected actual job database ID 501",
            "Enable debug logging?",
            "Numeric repository 10000",
            "Current repository:",
        ],
    );
    let after = session.completed_cells_end();
    form_control(&mut session, mouse, FORM, "[Preview]");
    session.wait_for_command_count(&fixture, 85);
    complete_cells(&mut session, after, FORM, &["RERUN PREVIEW", H, "501"]);
    let after = session.completed_cells_end();
    form_control(&mut session, mouse, FORM, "[Confirm action]");
    session.wait_for_command_count(&fixture, 107);
    complete_cells(
        &mut session,
        after,
        HISTORY,
        &[
            "GitHub accepted rerun",
            "completion NOT confirmed",
            "[View original]",
        ],
    );
    let db = raw(&fixture);
    let(state,claimed,kind,context,options,resources):(String,bool,String,String,String,String)=db.query_row(
        "SELECT state,claimed,kind_json,context_json,options_json,resources_json FROM actions_write_attempts",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).unwrap();
    assert_eq!(state, "accepted_pending");
    assert!(claimed);
    assert_eq!(kind, "\"rerun_job\"");
    let context: Value = serde_json::from_str(&context).unwrap();
    assert_eq!(context["run_id"], 123);
    assert_eq!(context["attempt"], 2);
    assert_eq!(context["job_id"], 501);
    assert_eq!(context["head_sha"], H);
    assert_eq!(context["original_run_v3"]["run_node"], "RUN_123");
    assert_eq!(context["original_run_v3"]["check_suite_id"], 900);
    assert_eq!(context["original_repository_v3"]["repository_id"], 10000);
    assert_eq!(context["original_pr_v3"]["number"], 7);
    assert_eq!(context["original_pr_v3"]["head_sha"], H);
    assert_eq!(
        serde_json::from_str::<Value>(&options).unwrap()["debug_logging"],
        false
    );
    let resources: Vec<String> = serde_json::from_str(&resources).unwrap();
    assert_eq!(
        resources,
        vec![
            format!("{HOST}/{FIRST}/run/123"),
            format!("{HOST}/repository-id/10000/actions/run/123")
        ]
    );
    let attempts: u64 = db
        .query_row("SELECT COUNT(*) FROM actions_write_attempts", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(attempts, 1);
    drop(db);
    let after = session.completed_cells_end();
    form_control(&mut session, mouse, HISTORY, "[Keep draft / Close]");
    session.wait_for_new_cursorless_condition(
        after,
        |f| crate::support::overlay_body(f, HISTORY).is_none(),
        "Original PR command closed",
        Duration::from_secs(8),
    );
    assert_eq!(unrelated(&fixture), sql);
    assert_eq!(git_state(&fixture), git);
    assert_eq!(git_commands(&fixture), trace);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    assert!(!session.screen().contains("GENUINE_PR_ARTIFACT"));
    assert!(!session.screen().contains(PRIVATE_DRAFT));
    assert_read_only_git(&fixture);
    session.quit_and_assert_restored_terminal(mouse);
    let rows = commands(&session, &fixture);
    assert_eq!(rows.len(), 107);
    for (row, step) in rows.iter().zip(&expected) {
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(row["matched"], true);
        assert_eq!(
            row["stdin_hex"],
            step.stdin
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
    }
    let writes: Vec<_> = rows
        .iter()
        .filter(|r| r["args"].as_array().unwrap().iter().any(|v| v == "POST"))
        .collect();
    assert_eq!(writes.len(), 1);
    assert_eq!(
        writes[0]["args"][1],
        format!("repos/{FIRST}/actions/jobs/501/rerun")
    );
}
#[test]
fn scoped_actions_rerun_v4_keyboard_initial_pr_direct_original_job_debug_false_max22_confirm() {
    happy(false);
}
#[test]
fn scoped_actions_rerun_v4_mouse_initial_pr_direct_original_job_debug_false_max22_confirm() {
    happy(true);
}
