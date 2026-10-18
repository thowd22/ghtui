//! Daily-use Actions controls: live run watch and large-artifact handling.
//! Real-binary journeys with a strict offline fake gh, keyboard and mouse.
use super::*;
use std::time::Duration;

fn run_with(status: &str, conclusion: Value) -> Value {
    let mut run = raw_run();
    run["status"] = json!(status);
    run["conclusion"] = conclusion;
    run
}
fn attempt_steps(run: &Value) -> Vec<GhStep> {
    let mut steps = open_steps();
    steps[0].stdout = serde_json::to_vec(run).unwrap();
    steps
}
fn open_run(session: &mut Session, mouse: bool) {
    session.wait_for_screen("ghtui");
    choose(session, mouse, "Actions", b"4");
    session.wait_for_screen("example/repo run 123");
    if mouse {
        session.click_visible_action("example/repo run 123")
    } else {
        session.send_key(b"\r")
    };
    session.wait_for_screen("Run 123 • attempt 2");
}
fn wait_long(session: &mut Session, text: &str) {
    if let Err(error) = session.try_wait_for(text, Duration::from_secs(15)) {
        session.save_artifacts();
        panic!("{error}; artifacts: {}", session.artifact_dir.display());
    }
}

fn watch_until_finished(mouse: bool) {
    let fixture = fixture();
    let running = run_with("in_progress", Value::Null);
    let finished = run_with("completed", json!("success"));
    let mut steps = initial();
    steps[2].stdout =
        serde_json::to_vec(&json!({"total_count":1,"workflow_runs":[running.clone()]})).unwrap();
    steps.extend(attempt_steps(&running));
    let opened = steps.len();
    // Each watch tick: account check, current run (follows the newest attempt), attempt and jobs.
    for run in [&running, &finished] {
        steps.extend(current_account());
        steps.push(read(
            "repos/example/repo/actions/runs/123",
            run.clone(),
            false,
        ));
        steps.extend(attempt_steps(run));
    }
    let watched = steps.len();
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    open_run(&mut session, mouse);
    journal(&mut session, &fixture, opened);
    choose(&mut session, mouse, "[w Watch]", b"w");
    session.wait_for_screen("Watching: refreshes every 5s");
    session.wait_for_screen("[w Stop watch]");
    wait_long(&mut session, "Run 123 attempt 2 is in progress");
    wait_long(
        &mut session,
        "Watch ended: run 123 attempt 2 finished • success",
    );
    session.wait_for_screen("[w Watch]");
    journal(&mut session, &fixture, watched);
    // Finished runs are not polled again.
    session.poll_output(Duration::from_secs(6));
    let rows = journal(&mut session, &fixture, watched);
    assert!(rows
        .iter()
        .all(|row| row["args"][0] != "api" || row["args"][5] == "GET"));
    session.resize(80, 24);
    session.wait_for_screen("Watch ended");
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn v1_actions_controls_keyboard_watch_updates_live_and_stops_when_run_finishes() {
    watch_until_finished(false)
}
#[test]
fn v1_actions_controls_mouse_watch_updates_live_and_stops_when_run_finishes() {
    watch_until_finished(true)
}

fn watch_stop_on_runs(mouse: bool) {
    let fixture = fixture();
    let mut newer = raw_run();
    newer["id"] = json!(124);
    newer["created_at"] = json!("2026-10-14T12:00:00Z");
    newer["status"] = json!("queued");
    newer["conclusion"] = Value::Null;
    let mut steps = initial();
    steps.extend(current_account());
    steps.push(read(
        "repos/example/repo/actions/runs?per_page=100",
        json!({"total_count":2,"workflow_runs":[newer, raw_run()]}),
        false,
    ));
    let watched = steps.len();
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "Actions", b"4");
    session.wait_for_screen("example/repo run 123");
    choose(&mut session, mouse, "[w Watch]", b"w");
    session.wait_for_screen("Watching: refreshes every 5s");
    // A newer run appears without a manual refresh; the selection stays on run 123.
    wait_long(&mut session, "example/repo run 124");
    session
        .try_wait_for_condition(
            |screen| {
                screen
                    .lines()
                    .any(|line| line.contains("> ") && line.contains("example/repo run 123"))
            },
            "selection kept on run 123",
            Duration::from_secs(8),
        )
        .unwrap();
    journal(&mut session, &fixture, watched);
    choose(&mut session, mouse, "[w Stop watch]", b"w");
    session.wait_for_screen("Watch stopped");
    session.poll_output(Duration::from_secs(6));
    journal(&mut session, &fixture, watched);
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn v1_actions_controls_keyboard_watch_run_list_keeps_selection_and_stops() {
    watch_stop_on_runs(false)
}
#[test]
fn v1_actions_controls_mouse_watch_run_list_keeps_selection_and_stops() {
    watch_stop_on_runs(true)
}

fn large_artifact(mouse: bool) {
    let fixture = fixture();
    let mut big = artifact_value(44, "release-binaries", b"unused", false);
    big["size_in_bytes"] = json!(60_468_414u64);
    let mut steps = initial();
    steps.extend(open_steps());
    steps.push(read(
        "repos/example/repo/actions/runs/123/artifacts?per_page=100",
        json!([{"artifacts":[big.clone()]}]),
        true,
    ));
    let listed = steps.len();
    steps.extend(compose_identity_steps());
    steps.extend(compose_identity_steps());
    steps.extend(current_account());
    steps.extend([
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        read("user", json!({"id":42,"login":"octocat"}), false),
        read("repos/example/repo/actions/artifacts/44", big, false),
    ]);
    let mut deletion = write("repos/example/repo/actions/artifacts/44", None, Value::Null);
    deletion.args[5] = "DELETE".into();
    steps.extend([
        deletion,
        read(
            "repos/example/repo/actions/runs/123/artifacts?per_page=100",
            json!([{"artifacts":[]}]),
            true,
        ),
    ]);
    steps.extend(current_account());
    let deleted = steps.len();
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    open_run(&mut session, mouse);
    choose(&mut session, mouse, "Artifacts", b"a");
    session.wait_for_screen("release-binaries #44 • 57.7 MiB");
    if mouse {
        session.click_visible_action("release-binaries #44");
    } else {
        session.send_key(b"\r");
    }
    // Opening never downloads an over-limit archive.
    session.wait_for_screen("Not downloaded: 57.7 MiB is over ghtui's 16.0 MiB");
    session.wait_for_screen("gh run download 123 -R example/repo -n release-binaries");
    session.wait_for_screen("Export artifact ZIP unavailable: 57.7 MiB is over");
    journal(&mut session, &fixture, listed);
    // A cancelled deletion sends nothing.
    command(&mut session, mouse, "Delete artifact");
    session.wait_for_screen("ACTIONS INTENT");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    close_form(&mut session, mouse);
    journal(&mut session, &fixture, listed + 4);
    command(&mut session, mouse, "Delete artifact");
    session.wait_for_screen("ACTIONS INTENT");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    confirm(&mut session, mouse, 0);
    session.wait_for_screen("Artifact 44 deletion confirmed");
    session.wait_for_screen("This run has no artifacts");
    let rows = journal(&mut session, &fixture, deleted);
    let writes = rows
        .iter()
        .filter(|row| row["args"][5] != "GET" && row["args"][0] == "api")
        .collect::<Vec<_>>();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0]["args"][5], "DELETE");
    assert!(!rows
        .iter()
        .any(|row| row["args"][1] == "repos/example/repo/actions/artifacts/44/zip"));
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    session.resize(80, 24);
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn v1_actions_controls_keyboard_large_artifact_explains_download_and_deletes() {
    large_artifact(false)
}
#[test]
fn v1_actions_controls_mouse_large_artifact_explains_download_and_deletes() {
    large_artifact(true)
}
