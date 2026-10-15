//! SOURCE ONLY / UNRUN. Actual binary/source/Git fixtures, strict stdin,
//! both input methods, owned resize/cancel and exact no-effect witnesses.
use super::*;

fn current_browser(h: &history::GitImagesV1, outdated: bool) -> Vec<GhStep> {
    let mut steps = thread_phase(h);
    let mut value: Value = serde_json::from_slice(&steps[9].stdout).unwrap();
    let thread = &mut value["data"]["node"]["reviewThreads"]["nodes"][0];
    thread["isOutdated"] = json!(outdated);
    thread["diffSide"] = json!("RIGHT");
    thread["startDiffSide"] = Value::Null;
    thread["line"] = json!(2);
    thread["comments"]["nodes"][0]["line"] = json!(2);
    steps[9].stdout = serde_json::to_vec(&value).unwrap();
    steps
}
fn selected_phase(h: &history::GitImagesV1) -> Vec<GhStep> {
    let browser = current_browser(h, false);
    let value: Value = serde_json::from_slice(&browser[9].stdout).unwrap();
    let mut node = value["data"]["node"]["reviewThreads"]["nodes"][0].clone();
    node["__typename"] = json!("PullRequestReviewThread");
    node["pullRequest"]["baseRefOid"] = json!(h.b);
    node["comments"]["nodes"][0]["pullRequest"]["baseRefOid"] = json!(h.b);
    let mut graph = GhStep::read(
        &[
            "api",
            "graphql",
            "--hostname",
            HOST,
            "--method",
            "POST",
            "--input",
            "-",
        ],
        &json!({"data":{"node":node}}).to_string(),
    );
    graph.stdin=serde_json::to_vec(&json!({"query":ghtui::providers::gh::METRICS_THREAD_COMMENTS_QUERY,"variables":{"id":"ACTUAL_NATIVE_ENTRY_THREAD_9","first":100,"after":null}})).unwrap();
    let mut parent = parent_resource();
    h.bind_parent(&mut parent);
    let mut steps = principal(42);
    for (endpoint, value) in [
        (format!("repos/{FIRST}"), repository(0)),
        (format!("repos/{FIRST}/pulls/7"), parent.clone()),
        (format!("repos/{FIRST}"), repository(0)),
        (format!("repos/{FIRST}/pulls/7"), parent.clone()),
    ] {
        steps.push(get(&endpoint, value));
    }
    steps.push(graph);
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent));
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.extend(principal(42));
    assert_eq!(steps.len(), 11);
    steps
}
fn exact(session: &Session, fixture: &Fixture, steps: &[GhStep]) {
    let calls = commands(session, fixture);
    assert_eq!(calls.len(), steps.len());
    for (ordinal, (actual, step)) in calls.iter().zip(steps).enumerate() {
        assert_eq!(actual["step"], ordinal);
        assert_eq!(actual["matched"], true);
        assert_eq!(actual["args"], json!(step.args));
        assert_eq!(
            actual["stdin_hex"],
            step.stdin
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        assert!(!step
            .args
            .iter()
            .any(|arg| matches!(arg.as_str(), "PUT" | "PATCH" | "DELETE")));
        if step.args.iter().any(|arg| arg == "POST") {
            assert_eq!(step.args[1], "graphql");
            let input: Value = serde_json::from_slice(&step.stdin).unwrap();
            assert!(
                matches!(input["query"].as_str(),Some(q) if q==METRICS_REVIEW_THREADS_QUERY || q==ghtui::providers::gh::METRICS_THREAD_COMMENTS_QUERY)
            );
            assert!(input["query"].as_str().unwrap().starts_with("query("));
        }
    }
}
fn ready(session: &mut Session, outdated: bool) {
    session.wait_for_cursorless_condition(
        |frame| {
            crate::support::overlay_body(frame, THREADS).is_some_and(|body| {
                body.contains("[g Current code]")
                    && body.contains("native-thread-actor")
                    && body.contains(if outdated {
                        "OUTDATED"
                    } else {
                        "CURRENT PLACEMENT"
                    })
            })
        },
        "current original thread actor/placement and clickable code control",
        Duration::from_secs(8),
    );
}
fn journey(mouse: bool, mode: u8) {
    let (fixture, h, mut steps) = fixture_and_steps(|_| {});
    assert_ne!(
        h.m, h.h,
        "Observed comment C remains independent from review H"
    );
    steps.extend(current_browser(&h, mode == 1));
    if mode != 1 {
        let mut phase = selected_phase(&h);
        if mode == 2 {
            phase[6].delay_ms = 6000;
            phase.truncate(7);
        }
        steps.extend(phase);
    }
    fixture.scripted_gh(&steps);
    let git = readonly_git(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    files_selected(&mut session, &fixture, mouse);
    let sql = sql_state(&fixture);
    let trace = git_commands(&fixture);
    choose(&mut session, mouse, "[v Native diff]", b"v");
    session.wait_for_command_count(&fixture, 58);
    nready(&mut session, "new-no-newline");
    // Capture and later compare the original viewport at the SAME geometry;
    // mouse journeys start at 80x24, while nested thread resize ends at120x40.
    session.resize_owned_modal(120, 40, "Native PR diff •", &["[o Options]", "[q Quit]"]);
    nready(&mut session, "new-no-newline");
    focus(&mut session, mouse, "new-no-newline");
    let native = actual_native_body(&mut session);
    act(&mut session, mouse, NATIVE, "[T Threads]", b"T");
    session.wait_for_command_count(&fixture, 75);
    ready(&mut session, mode == 1);
    for (width, height) in [(80, 24), (120, 40)] {
        session.resize_owned_header(width, height, THREADS, &["[g Current code]", "[Esc Back]"]);
        ready(&mut session, mode == 1);
    }
    let browser = session.wait_for_overlay_body(THREADS);
    let before = session.completed_cursorless_end();
    act(&mut session, mouse, THREADS, "[g Current code]", b"g");
    if mode == 0 {
        session.wait_for_command_count(&fixture, 86);
        session.wait_for_new_cursorless_condition(
            before,
            |frame| {
                crate::support::overlay_body(frame, "Native PR diff •").is_some_and(|body| {
                    body.contains("[b Back thread]") && body.contains("new-no-newline")
                }) && crate::support::overlay_body(frame, THREADS).is_none()
            },
            "fresh exact canonical cell focus with retained thread Back",
            Duration::from_secs(8),
        );
        let after = session.completed_cursorless_end();
        act(&mut session, mouse, NATIVE, "[b Back thread]", b"b");
        session.wait_for_new_cursorless_condition(
            after,
            |frame| {
                crate::support::overlay_body(frame, THREADS).as_deref() == Some(browser.as_str())
            },
            "pure Back preserves exact thread selection/body/filter/scroll",
            Duration::from_secs(8),
        );
        assert_eq!(commands(&session, &fixture).len(), 86);
    } else if mode == 1 {
        session.wait_for_new_cursorless_condition(
            before,
            |frame| {
                crate::support::overlay_body(frame, THREADS)
                    .is_some_and(|body| body.contains(BODY) && body.contains("OUTDATED"))
                    && frame
                        .contents()
                        .contains("Only an explicitly current LINE thread")
            },
            "outdated source does not jump, request or infer current side",
            Duration::from_secs(8),
        );
        assert_eq!(commands(&session, &fixture).len(), 75);
    } else {
        session.wait_for_command_count(&fixture, 82);
        let pid = commands(&session, &fixture).last().unwrap()["pid"]
            .as_i64()
            .unwrap() as i32;
        let owner = OwnedPtySession::capture(session.owned_application_pid() as u32).unwrap();
        let child = process_identity(pid)
            .unwrap()
            .expect("Actual six-second current-code read live before cancel");
        assert!(owner.owns(&child));
        assert!(!child.zombie);
        act(&mut session, mouse, THREADS, "[Esc Back]", b"\x1b");
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            match process_identity(child.pid) {
                Ok(None) => break,
                Ok(Some(current))
                    if (current.uid, current.birth, current.session)
                        != (child.uid, child.birth, child.session) =>
                {
                    break
                }
                Ok(Some(_)) => {}
                Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {}
                Err(error) => panic!("Original child observation failed: {error}"),
            }
            assert!(
                Instant::now() < deadline,
                "Exact original child identity must disappear"
            );
            session.poll_output(Duration::from_millis(1));
        }
        session.wait_for_cursorless_condition(
            |frame| {
                crate::support::overlay_body(frame, "Native PR diff •")
                    .is_some_and(|body| body == native && !body.contains("[b Back thread]"))
                    && crate::support::overlay_body(frame, THREADS).is_none()
            },
            "cancel restores only original Native patch, never a late focus",
            Duration::from_secs(8),
        );
        assert_eq!(commands(&session, &fixture).len(), 82);
    }
    if mode != 2 {
        act(&mut session, mouse, THREADS, "[Esc Back]", b"\x1b");
        session.wait_for_cursorless_condition(
            |frame| {
                crate::support::overlay_body(frame, "Native PR diff •")
                    .is_some_and(|body| body == native)
            },
            "original Native range/query/options/viewport exact after pure return",
            Duration::from_secs(8),
        );
    }
    exact(&session, &fixture, &steps);
    assert_eq!(sql_state(&fixture), sql);
    no_attempts(&fixture);
    assert_eq!(readonly_git(&fixture), git);
    assert_eq!(git_commands(&fixture), trace);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    assert_read_only_git(&fixture);
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn review_threads_v4_current_code_keyboard_fresh_native_canonical_cell_and_pure_back() {
    journey(false, 0);
}
#[test]
fn review_threads_v4_current_code_mouse_fresh_native_canonical_cell_and_pure_back() {
    journey(true, 0);
}
#[test]
fn review_threads_v4_current_code_keyboard_outdated_no_jump_or_read() {
    journey(false, 1);
}
#[test]
fn review_threads_v4_current_code_mouse_outdated_no_jump_or_read() {
    journey(true, 1);
}
#[test]
fn review_threads_v4_current_code_keyboard_actual_held_read_cancel_no_late_focus() {
    journey(false, 2);
}
#[test]
fn review_threads_v4_current_code_mouse_actual_held_read_cancel_no_late_focus() {
    journey(true, 2);
}
