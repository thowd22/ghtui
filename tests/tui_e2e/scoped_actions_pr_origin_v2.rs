//! SOURCE-ONLY real-binary PR-check→shared Actions resources, no fake Catalogue.
use super::*;
fn actual_artifact() -> Value {
    json!({"id":701,"node_id":"ART_701","name":"GENUINE_PR_ARTIFACT","expired":false,"workflow_run":{"id":123,"repository_id":10000,"head_sha":"a".repeat(40)}})
}
fn resources(uid: u64) -> Vec<GhStep> {
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
    let mut list = get(
        &format!("repos/{FIRST}/actions/runs/123/artifacts?per_page=100"),
        json!([{"total_count":1,"artifacts":[actual_artifact()]}]),
    );
    list.args.extend(["--paginate".into(), "--slurp".into()]);
    out.push(list);
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
    out.extend(principal(uid));
    assert_eq!(out.len(), 13);
    out
}
fn complete(session: &mut Session, after: usize, values: &[&str]) {
    session.wait_for_new_cursorless_condition(
        after,
        |frame| {
            let text = frame.contents();
            // A bracketed control is a key-bar token (compact below 100 columns).
            values.iter().all(|value| {
                if value.starts_with('[') {
                    crate::support::controls::control_painted(frame, value)
                } else {
                    text.contains(value)
                }
            })
        },
        "Actual closed PR-origin Actions frame",
        Duration::from_secs(8),
    );
}
fn happy(mouse: bool) {
    let mut expected = steps(vec![]);
    expected.extend(resources(42));
    let fixture = resource_configured(&expected);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    open_run(&mut session, &fixture, mouse);
    let sql = sql_state(&fixture);
    let trace = git_commands(&fixture);
    let after = session.completed_cursorless_end();
    control(&mut session, mouse, "[a Artifacts]", b"a");
    session.wait_for_command_count(&fixture, 72);
    complete(
        &mut session,
        after,
        &["GENUINE_PR_ARTIFACT", "run 123", "attempt not exposed"],
    );
    assert!(!session.screen().contains(DETAIL_BODY));
    assert!(!session.screen().contains(PRIVATE_DRAFT));
    session.resize(if mouse { 80 } else { 120 }, if mouse { 24 } else { 40 });
    session.wait_for_main_frame();
    let after = session.completed_cursorless_end();
    control(&mut session, mouse, "[b Back]", b"b");
    complete(
        &mut session,
        after,
        &["Run 123", "attempt 2", &"a".repeat(40)],
    );
    let after = session.completed_cursorless_end();
    control(&mut session, mouse, "[b Back]", b"b");
    complete(
        &mut session,
        after,
        &["Exact observed build", "[Enter Open check]"],
    );
    assert!(!session.screen().contains("GENUINE_PR_ARTIFACT"));
    assert_eq!(commands(&session, &fixture).len(), 72);
    unchanged(&session, &fixture, &sql, &expected);
    assert_eq!(git_state(&fixture), git);
    assert_eq!(git_commands(&fixture), trace);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    session.quit_and_assert_restored_terminal(mouse);
    exact_reads(&session, &fixture, &expected);
    no_attempts(&fixture);
}
fn drift(mouse: bool) {
    for changed_head in [true, false] {
        let mut op = resources(if changed_head { 42 } else { 43 });
        if changed_head {
            let mut raw = parent_resource();
            raw["head"]["sha"] = json!("b".repeat(40));
            op[8] = get(&format!("repos/{FIRST}/pulls/7"), raw);
            op.truncate(9);
        }
        let mut expected = steps(vec![]);
        expected.extend(op);
        let fixture = resource_configured(&expected);
        let git = git_state(&fixture);
        let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
        let mut session = launch(&fixture, mouse);
        open_run(&mut session, &fixture, mouse);
        let sql = sql_state(&fixture);
        let trace = git_commands(&fixture);
        let after = session.completed_cursorless_end();
        control(&mut session, mouse, "[a Artifacts]", b"a");
        session.wait_for_command_count(&fixture, expected.len());
        complete(
            &mut session,
            after,
            &["Original Actions read unavailable", "private source hidden"],
        );
        assert!(!session.screen().contains("GENUINE_PR_ARTIFACT"));
        assert!(!session.screen().contains("Run 123"));
        assert!(!session.screen().contains(DETAIL_BODY));
        unchanged(&session, &fixture, &sql, &expected);
        assert_eq!(git_state(&fixture), git);
        assert_eq!(git_commands(&fixture), trace);
        assert_eq!(
            fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
            config
        );
        session.quit_and_assert_restored_terminal(mouse);
        exact_reads(&session, &fixture, &expected);
        no_attempts(&fixture);
    }
}
#[test]
fn scoped_actions_resources_v2_keyboard_genuine_pr_checks_artifacts_exact_back_no_fallback() {
    happy(false);
}
#[test]
fn scoped_actions_resources_v2_mouse_genuine_pr_checks_artifacts_exact_back_no_fallback() {
    happy(true);
}
#[test]
fn scoped_actions_resources_v2_keyboard_pr_closing_head_numeric_uid_refuses_private_resource() {
    drift(false);
}
#[test]
fn scoped_actions_resources_v2_mouse_pr_closing_head_numeric_uid_refuses_private_resource() {
    drift(true);
}
