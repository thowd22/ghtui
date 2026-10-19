//! SOURCE ONLY / UNRUN. Actual original PR→Workflows→Definition→Confirm, no
//! name-only repository detour or fabricated Catalogue/Workflow/Run source.
use super::*;
#[path = "scoped_actions_workflow_corrective_v6.rs"]
mod corrective_v6;
use crate::scoped_actions_owned_body_v120 as owned_v120;
const FORM: &str = "Disable workflow • original whole workflow";
const HISTORY: &str = "Original workflow publication";
fn workflow(state: &str) -> Value {
    json!({"id":7,"node_id":"WF_7","name":"PRIVATE_PR_WORKFLOW","path":".github/workflows/ci.yml","state":state})
}
fn wf(state: &str) -> GhStep {
    get(
        &format!("repos/{FIRST}/actions/workflows/7"),
        workflow(state),
    )
}
fn bracket(body: Vec<GhStep>) -> Vec<GhStep> {
    let mut out = principal(42);
    out.push(get(&format!("repos/{FIRST}"), repository(0)));
    out.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    out.push(get(
        &format!("repos/{FIRST}/check-runs/600"),
        selected_check(),
    ));
    out.extend(body);
    out.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    out.push(get(
        &format!("repos/{FIRST}/check-runs/600"),
        selected_check(),
    ));
    out.push(get(&format!("repos/{FIRST}"), repository(0)));
    out.extend(principal(42));
    out
}
fn list() -> Vec<GhStep> {
    let mut step = get(
        &format!("repos/{FIRST}/actions/workflows?per_page=100"),
        json!([{"total_count":1,"workflows":[workflow("active")]}]),
    );
    step.args.extend(["--paginate".into(), "--slurp".into()]);
    let out = bracket(vec![step]);
    assert_eq!(out.len(), 11);
    out
}
fn definition() -> Vec<GhStep> {
    let sha = "c".repeat(40);
    let out=bracket(vec![wf("active"),get(&format!("repos/{FIRST}/commits/main"),json!({"sha":sha})),GhStep::read(&["api",&format!("repos/{FIRST}/contents/.github/workflows/ci.yml?ref={sha}"),"--hostname",HOST,"--method","GET","--header","Accept: application/vnd.github.raw+json","--allow-escape-sequences"],"name: PRIVATE_PR_WORKFLOW\non: [push]\njobs:\n  actual:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo NEVER_EXECUTED\n"),wf("active")]);
    assert_eq!(out.len(), 14);
    out
}
fn phase() -> Vec<GhStep> {
    let out = bracket(vec![wf("active"), wf("active")]);
    assert_eq!(out.len(), 12);
    out
}
fn confirm(uid: u64) -> Vec<GhStep> {
    let mut out = principal(42);
    out.push(get(&format!("repos/{FIRST}"), repository(0)));
    out.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    out.push(get(
        &format!("repos/{FIRST}/check-runs/600"),
        selected_check(),
    ));
    out.push(wf("active"));
    out.extend(principal(42));
    out.push(wf("active"));
    out.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    out.push(get(
        &format!("repos/{FIRST}/check-runs/600"),
        selected_check(),
    ));
    out.push(wf("active"));
    out.push(get(&format!("repos/{FIRST}"), repository(0)));
    out.extend(principal(42));
    out.push(GhStep::read(
        &[
            "api",
            &format!("repos/{FIRST}/actions/workflows/7/disable"),
            "--hostname",
            HOST,
            "--method",
            "PUT",
            "--include",
        ],
        "HTTP/2.0 204 No Content\nContent-Length: 0\r\n\r\n",
    ));
    out.push(wf("disabled_manually"));
    out.push(who(uid));
    assert_eq!(out.len(), 18);
    out
}
fn cells(session: &mut Session, after: usize, title: &str, words: &[&str]) {
    let until = std::time::Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        if session.completed_cells_end() > after
            && session
                .completed_cells_screen()
                .and_then(|frame| crate::support::overlay_body(frame, title))
                .is_some_and(|body| {
                    words.iter().all(|word| {
                        if *word == "NOT observed" {
                            crate::owned_form_prose_v120::adjacent_literal(&body, word)
                        } else {
                            body.contains(word)
                        }
                    })
                })
        {
            return;
        }
        assert!(
            std::time::Instant::now() < until,
            "Actual NEW PR workflow frame {title} {words:?}\n{}",
            session.screen()
        );
    }
}
fn form_action(session: &mut Session, mouse: bool, title: &str, label: &str) {
    if mouse {
        let (x, y) = session.wait_for_owned_completed_action(title, label, &[]);
        session.click(x, y);
    } else {
        let body = session
            .completed_cells_screen()
            .and_then(|frame| crate::support::overlay_body(frame, title))
            .expect("Actual completed owned workflow controls");
        let mut controls = if title == FORM {
            vec!["[Preview]", "[Confirm action]", "[Keep draft / Close]"]
        } else {
            vec![
                "[View original]",
                "[F4 Acknowledge locally]",
                "[Keep draft / Close]",
            ]
        };
        if body.contains("[F7 Error↑]") {
            controls.extend(["[F7 Error↑]", "[F8 Error↓]"]);
        } else if body.contains("[F7 Info↑]") {
            controls.extend(["[F7 Info↑]", "[F8 Info↓]"]);
        }
        session.focus_completed_owned_action_v1(
            title,
            label,
            crate::support::completed_navigation_controls_v1::FocusAttributeV1::Inverse,
            &controls,
            &[],
        );
        session.send_key(b"\r");
    }
}
fn original_workflow_row(session: &mut Session, mouse: bool) {
    if mouse {
        session.click_visible_action("PRIVATE_PR_WORKFLOW");
        return;
    }
    let after = session.completed_cursorless_end();
    session.send_key(b"\x1b[H");
    session.wait_for_new_cursorless_condition(
        after,
        |f| f.contents().lines().any(|line| line.contains('▌')),
        "Actual original workflow Home cursor",
        Duration::from_secs(8),
    );
    for _ in 0..32 {
        let line = session
            .completed_cursorless_screen()
            .unwrap()
            .contents()
            .lines()
            .find(|line| line.contains('▌'))
            .expect("Actual selected workflow row")
            .to_owned();
        if line.contains("PRIVATE_PR_WORKFLOW") {
            session.send_key(b"\r");
            return;
        }
        let after = session.completed_cursorless_end();
        session.send_key(b"\x1b[B");
        session.wait_for_new_cursorless_condition(
            after,
            |f| {
                f.contents()
                    .lines()
                    .find(|row| row.contains('▌'))
                    .is_some_and(|row| row != line)
            },
            "Actual Down changes selected canonical row",
            Duration::from_secs(8),
        );
    }
    panic!("Actual workflow member not reachable");
}
fn unrelated(fixture: &Fixture) -> Vec<String> {
    let db = raw(fixture);
    ["SELECT json_array(host,login,resource,body) FROM drafts WHERE resource NOT LIKE 'scoped-actions-v6/%' ORDER BY host,login,resource","SELECT json_array(host,login,name,payload) FROM saved_views ORDER BY host,login,name","SELECT json_array(host,login,kind,id,payload) FROM records ORDER BY host,login,kind,id","SELECT json_array(host,login,source,generation,complete) FROM checkpoints ORDER BY host,login,source","SELECT json_array(attempt_id,state) FROM pr_write_attempts ORDER BY attempt_id"].iter().flat_map(|sql|db.prepare(sql).unwrap().query_map([],|row|row.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>()).collect()
}
fn open_original_form(session: &mut Session, fixture: &Fixture, mouse: bool) {
    let after = session.completed_cursorless_end();
    control(session, mouse, "Workflows", b"W");
    session.wait_for_command_count(fixture, 70);
    session.wait_for_new_cursorless_condition(
        after,
        |frame| {
            let text = frame.contents();
            text.contains("PRIVATE_PR_WORKFLOW")
                && !text.contains("Requested ref main")
                && !text.contains("read pending")
        },
        "Actual original PR workflow catalogue",
        Duration::from_secs(8),
    );
    let after = session.completed_cursorless_end();
    original_workflow_row(session, mouse);
    session.wait_for_command_count(fixture, 84);
    session.wait_for_new_cursorless_condition(
        after,
        |frame| {
            let text = frame.contents();
            text.contains("Requested ref main")
                && text.contains("cccccccccccccccccccccccccccccccccccccccc")
        },
        "Actual requested definition commit distinct from PR H",
        Duration::from_secs(8),
    );
    let after = session.completed_cells_end();
    if mouse {
        control(session, true, "Disable workflow", b"");
    } else {
        session.focus_completed_actions_control_v2("Disable workflow");
        session.send_key(b"\r");
    }
    session.wait_for_command_count(fixture, 96);
    cells(
        session,
        after,
        FORM,
        &["entire repository • all refs", "workflow 7", "[Preview]"],
    );
    let after = session.completed_cells_end();
    form_action(session, mouse, FORM, "[Preview]");
    session.wait_for_command_count(fixture, 108);
    cells(
        session,
        after,
        FORM,
        &["WHOLE WORKFLOW PREVIEW", "[Confirm action]"],
    );
}
fn journey(mouse: bool) {
    for changed_uid in [false, true] {
        let mut expected = steps(vec![]);
        assert_eq!(expected.len(), 59);
        expected.extend(list());
        expected.extend(definition());
        expected.extend(phase());
        expected.extend(phase());
        expected.extend(confirm(if changed_uid { 999 } else { 42 }));
        assert_eq!(expected.len(), 126);
        let fixture = resource_configured(&expected);
        let git = git_state(&fixture);
        let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
        let mut session = launch(&fixture, mouse);
        open_run(&mut session, &fixture, mouse);
        let before = unrelated(&fixture);
        let trace = git_commands(&fixture);
        open_original_form(&mut session, &fixture, mouse);
        let after = session.completed_cells_end();
        form_action(&mut session, mouse, FORM, "[Confirm action]");
        session.wait_for_command_count(&fixture, 126);
        if changed_uid {
            session.wait_for_new_cursorless_condition(
                after,
                |frame| {
                    let text = frame.contents();
                    text.contains("private source hidden")
                        && !text.contains("PRIVATE_PR_WORKFLOW")
                        && !text.contains(DETAIL_BODY)
                        && crate::support::main_footer_visible(frame)
                },
                "Actual same-login/different-UID workflow receipt privacy fence",
                Duration::from_secs(8),
            );
        } else {
            cells(
                &mut session,
                after,
                HISTORY,
                &["same workflow state observed", "[View original]"],
            );
            let after = session.completed_cells_end();
            form_action(&mut session, mouse, HISTORY, "[Keep draft / Close]");
            session.wait_for_new_cursorless_condition(
                after,
                |frame| {
                    !frame.contents().contains(HISTORY)
                        && crate::support::main_footer_visible(frame)
                },
                "Original PR workflow Close remains usable",
                Duration::from_secs(8),
            );
        }
        let db = raw(&fixture);
        let (state, context): (String, String) = db
            .query_row(
                "SELECT state,context_json FROM actions_write_attempts",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(state, "confirmed");
        let context: Value = serde_json::from_str(&context).unwrap();
        assert_eq!(context["original_repository_v3"]["repository_id"], 10000);
        assert_eq!(context["original_workflow_v6"]["node"], "WF_7");
        assert_eq!(context["original_pr_v3"]["number"], 7);
        assert_eq!(context["original_pr_v3"]["head_sha"], "a".repeat(40));
        assert!(context.get("original_run_v3").is_none());
        assert_eq!(git_state(&fixture), git);
        assert_eq!(git_commands(&fixture), trace);
        assert_eq!(unrelated(&fixture), before);
        assert_eq!(
            fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
            config
        );
        session.quit_and_assert_restored_terminal(mouse);
        let rows = commands(&session, &fixture);
        assert_eq!(rows.len(), expected.len());
        for (row, step) in rows.iter().zip(&expected) {
            assert_eq!(row["args"], json!(step.args));
            assert_eq!(row["matched"], true);
            assert_eq!(row["stdin_hex"], "");
        }
        assert_eq!(
            rows.iter()
                .filter(|row| row["args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|arg| arg == "PUT"))
                .count(),
            1
        );
    }
}
#[test]
fn scoped_actions_workflow_v6_keyboard_genuine_pr_disable_exact18_and_changed_uid_fence() {
    journey(false)
}
#[test]
fn scoped_actions_workflow_v6_mouse_genuine_pr_disable_exact18_and_changed_uid_fence() {
    journey(true)
}
