//! Real-binary Task14 journeys using strict offline transport and real local Git.
//! Checks -> exact shared Actions attempt/job/logs -> original owned Back.
use super::*;
use ghtui::{actions_write_journal::ActionsWriteJournal, pr_write_journal::PrWriteJournal};

const CHECK: &str = "Rust tests fail";
/// The job row shows the job's name (its id is in the ID column).
const JOB: &str = "matrix test linux";
const LOG: &str = "genuine exact-attempt log failure";
const HIDDEN_LOG: &str = "UID43_PRIVATE_LOG_CANARY";
const BRIDGE_READ_ONLY_REASON: &str = "This PR-check route retains an exact original run/job/log read. Additional catalogues need their own fresh source admission; no broader repository or cached permission is substituted.";
fn api_url(path: &str) -> String {
    format!("https://github.invalid/api/v3/{path}")
}
fn repo() -> Value {
    json!({"id":410,"node_id":"R_bridge_410","full_name":"example/repo","url":api_url("repos/example/repo"),"html_url":"https://github.invalid/example/repo","permissions":{"pull":true}})
}
fn canonical_parent() -> Value {
    let mut raw = pr();
    raw["id"] = json!(1007);
    raw["merged"] = json!(false);
    raw["url"] = json!(api_url("repos/example/repo/pulls/7"));
    raw["head"]["repo"] = repo();
    raw["base"]["repo"] = repo();
    raw
}
fn substitute(value: &mut Value) {
    match value {
        Value::Object(map)
            if map
                .get("node_id")
                .is_some_and(|node| node == "PR_example/repo_7") =>
        {
            *value = canonical_parent()
        }
        Value::Object(map) => {
            for child in map.values_mut() {
                substitute(child);
            }
        }
        Value::Array(values) => {
            for child in values {
                substitute(child);
            }
        }
        _ => {}
    }
}
fn canonical_steps(steps: Vec<GhStep>) -> Vec<GhStep> {
    steps
        .into_iter()
        .map(|mut call| {
            if let Ok(mut value) = serde_json::from_slice::<Value>(&call.stdout) {
                substitute(&mut value);
                call.stdout = serde_json::to_vec(&value).unwrap();
            }
            call
        })
        .collect()
}
fn raw_check() -> Value {
    json!({"id":600,"node_id":"CR_bridge_600","name":CHECK,"status":"completed","conclusion":"failure","head_sha":"a".repeat(40),"url":api_url("repos/example/repo/check-runs/600"),"details_url":"https://foreign.invalid/other/repo/actions/runs/999","check_suite":{"id":77}})
}
fn raw_run() -> Value {
    json!({"id":123,"node_id":"WR_bridge_123","workflow_id":7,"check_suite_id":77,"run_attempt":2,"name":"CI matrix","display_title":"Exact bridge source","head_sha":"a".repeat(40),"head_branch":"feature","event":"pull_request","status":"completed","conclusion":"failure","repository":repo(),"url":api_url("repos/example/repo/actions/runs/123"),"html_url":"https://github.invalid/example/repo/actions/runs/123","actor":{"id":51,"login":"alice","type":"User"},"triggering_actor":{"id":42,"login":"octocat","type":"User"}})
}
fn raw_job() -> Value {
    json!({"id":501,"node_id":"J_bridge_501","run_id":123,"name":"matrix test linux","head_sha":"a".repeat(40),"status":"completed","conclusion":"failure","runner_name":"hosted","labels":["ubuntu-latest"],"url":api_url("repos/example/repo/actions/jobs/501"),"check_run_url":api_url("repos/example/repo/check-runs/600"),"steps":[{"number":3,"name":"Run tests","status":"completed","conclusion":"failure"}]})
}
fn get(path: &str, value: Value) -> GhStep {
    step(path, "GET", value, false)
}
fn user(id: u64) -> GhStep {
    get("user", json!({"id":id,"login":"octocat","type":"User"}))
}
fn startup() -> Vec<GhStep> {
    let mut calls = canonical_steps(initial());
    // A lexically earlier inbox repository would be the generic Actions default.
    // The bridge must still use selected example/repo, never aaa/other.
    let mut author: Value = serde_json::from_slice(&calls[4].stdout).unwrap();
    let mut other = pr();
    other["node_id"] = json!("PR_aaa/other_8");
    other["id"] = json!(1008);
    other["number"] = json!(8);
    other["title"] = json!("Older unrelated PR");
    other["repository_url"] = json!("https://api.github.invalid/repos/aaa/other");
    other["html_url"] = json!("https://github.invalid/aaa/other/pull/8");
    other["updated_at"] = json!("2026-10-12T00:00:00Z");
    author["items"].as_array_mut().unwrap().push(other);
    author["total_count"] = json!(2);
    calls[4].stdout = serde_json::to_vec(&author).unwrap();
    calls.extend(canonical_steps(details(vec![starter()])));
    calls.push(step(
        &format!(
            "repos/example/repo/commits/{}/check-runs?per_page=100",
            "a".repeat(40)
        ),
        "GET",
        json!([{"check_runs":[raw_check()]}]),
        true,
    ));
    calls.push(step(
        &format!(
            "repos/example/repo/commits/{}/statuses?per_page=100",
            "a".repeat(40)
        ),
        "GET",
        json!([[]]),
        true,
    ));
    calls
}
fn bridge() -> Vec<GhStep> {
    let mut calls = identity();
    calls.extend([
        user(42),
        get("repos/example/repo", repo()),
        get("repos/example/repo/pulls/7", canonical_parent()),
        get("repos/example/repo/check-runs/600", raw_check()),
        get(
            &format!(
                "repos/example/repo/actions/runs?check_suite_id=77&head_sha={}&per_page=100&page=1",
                "a".repeat(40)
            ),
            json!({"total_count":1,"workflow_runs":[raw_run()]}),
        ),
        get("repos/example/repo/actions/runs/123/attempts/2", raw_run()),
        get(
            "repos/example/repo/actions/runs/123/attempts/2/jobs?per_page=100&page=1",
            json!({"total_count":1,"jobs":[raw_job()]}),
        ),
        get("repos/example/repo/actions/runs/123", raw_run()),
        get("repos/example/repo/check-runs/600", raw_check()),
        get("repos/example/repo/pulls/7", canonical_parent()),
        get("repos/example/repo", repo()),
        user(42),
    ]);
    calls.extend(identity());
    calls
}
fn log_read(failure: Option<bool>) -> Vec<GhStep> {
    let mut body = format!("{LOG}\n");
    for line in 0..40 {
        body.push_str(&format!("bridge log line {line:02}\n"));
    }
    let mut calls = vec![
        user(42),
        get("repos/example/repo", repo()),
        get("repos/example/repo/pulls/7", canonical_parent()),
        get("repos/example/repo/actions/runs/123", raw_run()),
        GhStep::read(
            &[
                "run",
                "view",
                "123",
                "--repo",
                "github.invalid/example/repo",
                "--attempt",
                "2",
                "--log",
                "--job",
                "501",
            ],
            if failure.is_some() { HIDDEN_LOG } else { &body },
        ),
        get("repos/example/repo/actions/runs/123", raw_run()),
        get("repos/example/repo/pulls/7", canonical_parent()),
        get("repos/example/repo", repo()),
        user(42),
    ];
    if let Some(expired) = failure {
        if expired {
            calls[8].status = 1;
            calls[8].stdout.clear();
            calls[8].stderr = b"gh: PRIVATE_AUTH_DIAGNOSTIC_CANARY (HTTP 401)".to_vec();
        } else {
            calls[8] = user(43);
        }
    }
    calls
}
fn fixture() -> Fixture {
    let fixture = configured();
    fixture.init_git();
    fixture.commit_file("tracked.txt", b"baseline\n", "bridge baseline");
    fs::write(fixture.repo().join("tracked.txt"), b"dirty exact bytes\n").unwrap();
    PrWriteJournal::open(&cache_path(&fixture)).unwrap();
    ActionsWriteJournal::open(&cache_path(&fixture)).unwrap();
    fixture
}
fn phase_ready(session: &mut Session, fixture: &Fixture, mouse: bool) {
    // Real Git fixtures intentionally start in the detected local workspace.
    // Explicitly enter Inbox before its refresh; the local view must not issue
    // GitHub requests or silently change the selected repository scope.
    session.wait_for_screen("Local Git workspace ready");
    records(session, fixture, 0);
    choose(session, mouse, "1 Inbox", b"1");
    session.wait_for_screen("Inbox • focused view");
    open(session, mouse);
    choose(session, mouse, "[k Checks]", b"k");
    session.wait_for_screen(CHECK);
    records(session, fixture, 16);
    let before_open = session.completed_cursorless_end();
    if mouse {
        session.click_visible_action("[Enter Open check]");
    } else {
        // `Enter Open check` is a key-bar token: one Enter opens the selected check.
        session.wait_for_control("[Enter Open check]");
        session.send_key(b"\r");
    }
    session
        .try_wait_for_condition(
            |screen| {
                screen.contains("Run 123 • attempt 2")
                    && screen.contains(JOB)
                    && screen.contains("Attempt source: verified exact endpoint")
            },
            "actual check/run/attempt/job projection",
            Duration::from_secs(8),
        )
        .unwrap();
    session.wait_for_new_cursorless_condition(
        before_open,
        |frame| {
            frame.contents().contains("Run 123 • attempt 2")
                && frame.contents().contains(JOB)
                && frame
                    .contents()
                    .contains("Attempt source: verified exact endpoint")
                && crate::support::locate_control(frame, "[Esc Cancel]").is_none()
        },
        "completed original run/attempt/job before control input",
        Duration::from_secs(8),
    );
    records(session, fixture, 32);
}
fn actions_control(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    session.activate_completed_actions_control(mouse, label, key);
}
fn disabled_bridge_controls(session: &mut Session, fixture: &Fixture, mouse: bool) {
    // Disabled shared controls leave the key bar. Their key still explains why, and the
    // mouse reaches the same explanation through the control's palette entry.
    for (label, key) in [
        ("[r Refresh]", b"r".as_slice()),
        ("[w Watch]", b"w".as_slice()),
    ] {
        session.first_completed_actions_page(mouse);
        assert!(session.completed_actions_control(label).is_none());
        if mouse {
            session.invoke_via_palette(true, label);
        } else {
            session.send_key(key);
        }
        session
            .try_wait_for_condition(
                |screen| {
                    screen.contains("Action unavailable")
                        && screen.contains("[Esc Close]")
                        // The label prefix can wrap the reason; its exact text is checked
                        // below with wrapping whitespace removed.
                        && screen.contains("fresh source admission")
                },
                "same complete shared bridge explanation for disabled click/key",
                Duration::from_secs(8),
            )
            .unwrap();
        let explanation = session.wait_for_overlay_body("Action unavailable");
        // Remove only wrapping whitespace: the complete fixed reason remains
        // exact even if a rendered line boundary splits a word.
        let flatten = |value: &str| {
            value
                .chars()
                .filter(|character| !character.is_whitespace())
                .collect::<String>()
        };
        assert!(flatten(&explanation).contains(&flatten(BRIDGE_READ_ONLY_REASON)));
        assert_eq!(mutation_count(&records(session, fixture, 32)), 0);
        let before_close = session.completed_cursorless_end();
        if mouse {
            session.click_visible_action("[Esc Close]");
        } else {
            session.send_key(b"\x1b");
        }
        session
            .try_wait_for_condition(
                |screen| {
                    !screen.contains("Action unavailable")
                        && screen.contains("Run 123 • attempt 2")
                        && screen.contains(JOB)
                        && screen.contains("Attempt source: verified exact endpoint")
                },
                "exact original run/job/source restored after disabled explanation",
                Duration::from_secs(8),
            )
            .unwrap();
        session.wait_for_new_cursorless_condition(
            before_close,
            |frame| {
                !frame.contents().contains("Action unavailable")
                    && frame.contents().contains("Run 123 • attempt 2")
                    && frame.contents().contains(JOB)
            },
            "completed original Actions after owned explanation Close",
            Duration::from_secs(8),
        );
        assert!(session.completed_cursorless_end() > before_close);
        assert!(!session.screen().contains("Stop watch"));
        records(session, fixture, 32);
    }
    // The explanations do not alter the retained row/source or send any provider request.
    session.first_completed_actions_page(mouse);
    records(session, fixture, 32);
}
fn effects(session: &mut Session, fixture: &Fixture, head: &str, index: &[u8]) {
    let rows = records(session, fixture, 41);
    assert_eq!(mutation_count(&rows), 0);
    assert!(rows.iter().all(|row| row["stdin_hex"] == ""));
    assert!(rows
        .iter()
        .all(|row| !row["args"].as_array().unwrap().iter().any(|arg| arg
            .as_str()
            .is_some_and(|s| s.contains("aaa/other") || s.contains("foreign.invalid")))));
    assert_eq!(
        rows[36]["args"],
        json!([
            "run",
            "view",
            "123",
            "--repo",
            "github.invalid/example/repo",
            "--attempt",
            "2",
            "--log",
            "--job",
            "501"
        ])
    );
    let sql = Connection::open(cache_path(fixture)).unwrap();
    for table in ["pr_write_attempts", "actions_write_attempts"] {
        let count: i64 = sql
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0, "read-only bridge cannot create a receipt/claim");
    }
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(
        fs::read(fixture.repo().join("tracked.txt")).unwrap(),
        b"dirty exact bytes\n"
    );
    assert_eq!(
        fs::read(fixture.repo().join("unrelated.txt")).unwrap(),
        b"unchanged local bytes\n"
    );
}
#[test]
fn pr_checks_bridge_keyboard_exact_source_job_logs_and_owned_back_test_framework() {
    happy(false)
}
#[test]
fn pr_checks_bridge_mouse_exact_source_job_logs_and_owned_back_test_framework() {
    happy(true)
}
fn happy(mouse: bool) {
    let fixture = fixture();
    let mut calls = startup();
    calls.extend(bridge());
    calls.extend(log_read(None));
    assert_eq!(calls.len(), 41);
    fixture.scripted_gh(&calls);
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let mut session = Session::launch(&fixture, 120, 40);
    phase_ready(&mut session, &fixture, mouse);
    disabled_bridge_controls(&mut session, &fixture, mouse);
    session.wait_for_screen("AL alice");
    session.wait_for_screen("Exact commit aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    session.resize(80, 24);
    session.wait_for_screen("Run 123 • attempt 2");
    for _ in 0..16 {
        if session.screen().contains(JOB) {
            break;
        }
        if mouse {
            session.wheel(20, 14, true);
        } else {
            session.send_key(b"\x1b[6~");
        }
        session.poll_output(Duration::from_millis(20));
    }
    session.wait_for_screen(JOB);
    let before_job = session.completed_cursorless_end();
    if mouse {
        session.click_visible_action(JOB);
    } else {
        session.focus_visible_action(JOB);
        session.send_key(b"\r");
    }
    session.wait_for_screen("3 Run tests");
    session.wait_for_screen("Run tests");
    session.wait_for_new_cursorless_condition(
        before_job,
        |frame| {
            frame.contents().contains("3 Run tests")
                && frame.contents().contains("Run tests")
                && crate::support::locate_control(frame, "[Esc Cancel]").is_none()
        },
        "completed exact selected job details before paged Logs control",
        Duration::from_secs(8),
    );
    records(&mut session, &fixture, 32);
    actions_control(&mut session, mouse, "[l Logs]", b"l");
    session.wait_for_screen(LOG);
    records(&mut session, &fixture, 41);
    for _ in 0..32 {
        if session.screen().contains("bridge log line 35") {
            break;
        }
        if mouse {
            session.wheel(20, 14, true);
        } else {
            session.send_key(b"\x1b[6~");
        }
        session.poll_output(Duration::from_millis(20));
    }
    session.wait_for_screen("bridge log line 35");
    for _ in 0..32 {
        if session.screen().contains(LOG) {
            break;
        }
        if mouse {
            session.wheel(20, 14, false);
        } else {
            session.send_key(b"\x1b[5~");
        }
        session.poll_output(Duration::from_millis(20));
    }
    session.wait_for_screen(LOG);
    actions_control(&mut session, mouse, "[b Back to PR checks]", b"b");
    session
        .try_wait_for_condition(
            |screen| {
                crate::support::controls::text_has_control(screen, "[k Checks]")
                    && screen.contains(CHECK)
                    && !screen.contains(LOG)
            },
            "same original PR checks route restored without transport",
            Duration::from_secs(8),
        )
        .unwrap();
    effects(&mut session, &fixture, &head, &index);
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn pr_checks_bridge_keyboard_late_same_login_uid_reassignment_hides_private_logs() {
    private_loss(false, false)
}
#[test]
fn pr_checks_bridge_mouse_late_same_login_uid_reassignment_hides_private_logs() {
    private_loss(true, false)
}
#[test]
fn pr_checks_bridge_keyboard_late_auth_expiry_hides_private_logs() {
    private_loss(false, true)
}
#[test]
fn pr_checks_bridge_mouse_late_auth_expiry_hides_private_logs() {
    private_loss(true, true)
}
fn private_loss(mouse: bool, expired: bool) {
    let fixture = fixture();
    let mut calls = startup();
    calls.extend(bridge());
    calls.extend(log_read(Some(expired)));
    assert_eq!(calls.len(), 41);
    fixture.scripted_gh(&calls);
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let mut session = Session::launch(&fixture, 120, 40);
    phase_ready(&mut session, &fixture, mouse);
    session.resize(80, 24);
    actions_control(&mut session, mouse, "[l Logs]", b"l");
    records(&mut session, &fixture, 41);
    session.wait_for_screen("PR check context changed • private pages hidden");
    assert!(!session.screen().contains(JOB));
    assert!(!session.screen().contains(CHECK));
    assert!(!session.screen().contains("Exact bridge source"));
    let output = String::from_utf8_lossy(&session.transcript);
    assert!(!output.contains(HIDDEN_LOG));
    assert!(!output.contains("PRIVATE_AUTH_DIAGNOSTIC_CANARY"));
    effects(&mut session, &fixture, &head, &index);
    session.quit_and_assert_restored_terminal(mouse);
}
