//! Native Actions real-binary journeys: terminal input plus strict command/Git assertions.
use crate::support::{Fixture, GhStep, Session};
use ghtui::config::Config;
use serde_json::{json, Value};
use std::collections::BTreeMap;
#[path = "actions/actor_profile_v1.rs"]
mod actor_profile_v1;
#[path = "actions/approval_recovery.rs"]
mod approval_recovery;
#[path = "actions/approvals.rs"]
mod approvals;
#[path = "actions/cache_outcomes_v1.rs"]
mod cache_outcomes_v1;
#[path = "actions/commit_runs_v1.rs"]
mod commit_runs_v1;
#[path = "actions/controls_v1.rs"]
mod controls_v1;
#[path = "actions/environment_secrets_v1.rs"]
mod environment_secrets_v1;
#[path = "actions/lease_selection.rs"]
mod lease_selection;
#[path = "actions/limitations_v1.rs"]
mod limitations_v1;
#[path = "actions/live_run_v117.rs"]
mod live_run_v117;
#[path = "actions/log_export_v1.rs"]
mod log_export_v1;
#[path = "actions/log_tail_v107.rs"]
mod log_tail_v107;
#[path = "actions/optional_attempt_v1.rs"]
mod optional_attempt_v1;
#[path = "actions/origin_default_v1.rs"]
mod origin_default_v1;
#[path = "actions/perf_log_v66.rs"]
mod perf_log_v66;
#[path = "actions/principal_freshness.rs"]
mod principal_freshness;
#[path = "../support/actions_publication.rs"]
pub(crate) mod publication_support;
#[path = "actions/restart.rs"]
mod restart;
#[path = "actions/runner_groups_v1.rs"]
mod runner_groups_v1;
#[path = "actions/runs_focus_v1.rs"]
mod runs_focus_v1;
#[path = "actions/secret_cli_preflight.rs"]
mod secret_cli_preflight;
#[path = "actions/settings.rs"]
mod settings;
#[path = "actions/variables_scope_v1.rs"]
mod variables_scope_v1;
#[path = "actions/workflow_revision_v1.rs"]
mod workflow_revision_v1;
#[path = "actions/workflow_usability_v1.rs"]
mod workflow_usability_v1;

pub(crate) fn fixture() -> Fixture {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("unchanged.txt", b"preserve this checkout\n", "initial");
    Config {
        host: "github.invalid".into(),
        avatars: false,
        repository_sets: BTreeMap::from([("actions".into(), vec!["example/repo".into()])]),
        active_set: Some("actions".into()),
        ..Config::default()
    }
    .save(&fixture.root.join("config/ghtui/config.toml"))
    .unwrap();
    fixture
}
/// The Runs table's RUN cell (TASK-91): the run id, then `#attempt` after a rerun.
pub(crate) fn run_cell(id: u64, attempt: u64) -> String {
    if attempt > 1 {
        format!("{id} #{attempt}")
    } else {
        id.to_string()
    }
}
/// Waits until one painted line holds every part: the cells of one table row.
pub(crate) fn wait_row(session: &mut Session, parts: &[&str]) {
    let description = format!("one row with {parts:?}");
    if let Err(error) = session.try_wait_for_condition(
        |screen| {
            screen
                .lines()
                .any(|line| parts.iter().all(|part| line.contains(part)))
        },
        &description,
        std::time::Duration::from_secs(8),
    ) {
        session.save_artifacts();
        panic!("{error}; artifacts: {}", session.artifact_dir.display());
    }
}
/// The dispatched run 200 on `main`. At 120 columns the split list drops its RUN cell and the
/// preview pane titles the selected run instead; a full-width list keeps the RUN cell.
fn wait_dispatched_run(session: &mut Session) {
    let found = session.try_wait_for_condition(
        |screen| {
            let row = |line: &str| line.contains("main") && line.contains("workflow_dispatch");
            screen
                .lines()
                .any(|line| row(line) && line.contains(" 200"))
                || screen.lines().any(row) && screen.contains("Run 200 ")
        },
        "dispatched run 200 on main",
        std::time::Duration::from_secs(8),
    );
    if let Err(error) = found {
        session.save_artifacts();
        panic!("{error}; artifacts: {}", session.artifact_dir.display());
    }
}
/// Every screen line cut before the `Run N` preview pane, when one is painted (TASK-92).
fn list_pane(screen: &str) -> String {
    let x = screen.lines().find_map(|line| {
        let chars = line.chars().collect::<Vec<_>>();
        let x = chars.iter().skip(1).position(|cell| *cell == '╭')? + 1;
        chars[x..]
            .iter()
            .collect::<String>()
            .contains("Run ")
            .then_some(x)
    });
    screen
        .lines()
        .map(|line| match x {
            Some(x) => line.chars().take(x).collect::<String>(),
            None => line.to_owned(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}
/// The Actions list's `▌` selected row contains `text`.
fn selected_has(session: &Session, text: &str) -> bool {
    session
        .selected_row(&crate::support::view_title("Actions"))
        .is_some_and(|row| row.contains(text))
}
/// Waits until the Actions list paints its `▌` selection bar on the row containing `text`.
pub(crate) fn wait_selected(session: &mut Session, text: &str) -> String {
    session.wait_for_selected_row(&crate::support::view_title("Actions"), text)
}
fn api(endpoint: &str) -> Vec<String> {
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
pub(crate) fn read(endpoint: &str, value: Value, pages: bool) -> GhStep {
    let mut args = api(endpoint);
    if pages {
        args.extend(["--paginate".into(), "--slurp".into()]);
    }
    GhStep {
        args,
        stdout: serde_json::to_vec(&value).unwrap(),
        stdin: vec![],
        stderr: vec![],
        status: 0,
        delay_ms: 0,
    }
}
pub(crate) fn raw_run() -> Value {
    json!({"id":123,"workflow_id":7,"name":"CI matrix","display_title":"Fix checkout","head_sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","head_branch":"feature","event":"pull_request","status":"completed","conclusion":"failure","run_attempt":2,"created_at":"2026-10-13T12:00:00Z","actor":{"login":"alice","type":"User"},"triggering_actor":{"login":"octocat","type":"User"}})
}
pub(crate) fn initial() -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        read("user", json!({"id":42,"login":"octocat"}), false),
        read(
            "repos/example/repo/actions/runs?per_page=100",
            json!({"total_count":1,"workflow_runs":[raw_run()]}),
            false,
        ),
    ]
}
fn open_steps() -> Vec<GhStep> {
    vec![
        read(
            "repos/example/repo/actions/runs/123/attempts/2",
            raw_run(),
            false,
        ),
        read(
            "repos/example/repo/actions/runs/123/attempts/2/jobs?per_page=100",
            json!([{"jobs":[{"id":501,"run_id":123,"run_attempt":2,"head_sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","name":"matrix test linux","status":"completed","conclusion":"failure","runner_name":"hosted","labels":["ubuntu-latest"],"check_run_url":"https://github.invalid/api/v3/repos/example/repo/check-runs/600","steps":[{"number":3,"name":"Run tests","status":"completed","conclusion":"failure"}]}]}]),
            true,
        ),
    ]
}
pub(crate) fn choose(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    if mouse {
        button(session, label)
    } else {
        session.send_key(key)
    }
}
pub(crate) fn button(session: &mut Session, label: &str) {
    if matches!(
        label,
        "[Preview]"
            | "[External editor]"
            | "[Confirm action]"
            | "[Keep draft / Close]"
            | "[F9 New intent]"
    ) {
        // Composer footers can arrive after the worker prepared the form. Never
        // reach into the underlying view while waiting for the overlay's real hit.
        session.click_visible_action(label);
        return;
    }
    session.click_actions_control(label);
}
/// A command control: clicked (mouse), or Tab-focused in the key bar and activated with
/// Enter (keyboard). Commands that do not fit the bar are chosen through the palette by the
/// same input kind.
pub(crate) fn command(session: &mut Session, mouse: bool, label: &str) {
    if mouse {
        button(session, label);
    } else {
        session.focus_completed_actions_control_v2(label);
        session.send_key(b"\r");
    }
}
/// A command with a key of its own (TASK-64): clicked (mouse), or its key pressed on the
/// idle Actions page (keyboard). The key works whether or not its token fits the bar.
pub(crate) fn keyed(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    if mouse {
        button(session, label);
    } else {
        session.activate_completed_actions_control(false, label, key);
    }
}
pub(crate) fn write(endpoint: &str, input: Option<Value>, out: Value) -> GhStep {
    let mut step = read(endpoint, out, false);
    step.args[5] = "POST".into();
    if let Some(input) = input {
        step.args.extend(["--input".into(), "-".into()]);
        step.stdin = serde_json::to_vec(&input).unwrap();
    }
    step
}
pub(crate) fn run_guard(run: Value) -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        read("user", json!({"id":42,"login":"octocat"}), false),
        read("repos/example/repo/actions/runs/123", run, false),
    ]
}
pub(crate) fn current_account() -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        read("user", json!({"id":42,"login":"octocat"}), false),
    ]
}
/// Private draft OPEN has an explicit positive principal before and after loading.
pub(crate) fn compose_identity_steps() -> Vec<GhStep> {
    let mut steps = current_account();
    steps.extend(current_account());
    steps
}
pub(crate) fn confirm(session: &mut Session, mouse: bool, field_count: usize) {
    if mouse {
        session.click_visible_action("[Confirm action]");
    } else {
        for _ in 0..field_count + 2 {
            session.send_key(b"\t");
        }
        session.send_key(b"\r");
    }
}
pub(crate) fn close_form(session: &mut Session, mouse: bool) {
    if mouse {
        session.click_visible_action("[Keep draft / Close]");
    } else {
        session.send_key(b"\x1b");
    }
    if let Err(error) = session.try_wait_for_condition(
        |screen| {
            !screen.contains("READ-ONLY PREVIEW")
                && !screen.contains("ACTIONS INTENT")
                && !screen.contains("ORIGINAL ATTEMPT •")
                && !screen.contains("[Keep draft / Close]")
        },
        "Actions composer closed",
        std::time::Duration::from_secs(8),
    ) {
        session.save_artifacts();
        panic!("{error}; artifacts: {}", session.artifact_dir.display());
    }
}
pub(crate) fn inspect_original_publication(session: &mut Session, mouse: bool, attempt_id: i64) {
    open_original_publication(session, mouse, attempt_id);
    session.wait_for_screen(&format!("Inspect ORIGINAL Actions attempt #{attempt_id}"));
}
fn open_original_publication(session: &mut Session, mouse: bool, attempt_id: i64) {
    // Disabled control explanations also mention Publications; target its exact
    // rendered button, not another control whose explanatory text contains it.
    command(session, mouse, "[P Publications]");
    session.wait_for_screen("unresolved publications shown");
    let heading = format!("ORIGINAL ACTIONS ATTEMPT #{attempt_id} •");
    session.wait_for_screen(&heading);
    if mouse {
        session.click_visible_action(&heading);
    } else {
        session.send_key(b"\x1b[H");
        for _ in 0..768 {
            session.poll_output(std::time::Duration::from_millis(40));
            if session
                .selected_row(&crate::support::view_title("Actions"))
                .is_some_and(|row| row.contains(&heading) && row.matches(" • ").count() >= 2)
            {
                session.send_key(b"\r");
                return;
            }
            session.send_key(b"\x1b[B");
        }
        session.save_artifacts();
        panic!("Exact original publication {attempt_id} never gained keyboard selection");
    }
}
pub(crate) fn journal(session: &mut Session, fixture: &Fixture, count: usize) -> Vec<Value> {
    session.wait_for_command_count(fixture, count);
    let rows = session
        .inspect_command_journal(fixture)
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), count);
    assert!(rows.iter().all(|row| row["matched"] == true));
    rows
}

#[test]
fn actions_inventory_keyboard_catalog_actor_resize_and_read_only_git() {
    inventory(false)
}
#[test]
fn actions_inventory_mouse_catalog_actor_resize_and_read_only_git() {
    inventory(true)
}
fn inventory(mouse: bool) {
    let fixture = fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    fixture.scripted_gh(&initial());
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "Actions", b"4");
    session.wait_for_screen(&run_cell(123, 2));
    session.wait_for_screen("AL alice");
    session.wait_for_screen("1 runs of 1 reported");
    journal(&mut session, &fixture, 3);
    session.resize(80, 24);
    session.wait_for_screen(&run_cell(123, 2));
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    assert_eq!(
        std::fs::read(fixture.repo().join("unchanged.txt")).unwrap(),
        b"preserve this checkout\n"
    );
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn actions_run_view_keyboard_exact_attempt_job_logs_and_annotations() {
    run_view(false)
}

#[test]
fn mouse_keyboard_actions_disabled_keyboard_inspects_guard_without_requests() {
    disabled_control(false)
}
#[test]
fn mouse_keyboard_actions_disabled_mouse_inspects_guard_without_requests() {
    disabled_control(true)
}
fn disabled_control(mouse: bool) {
    let fixture = fixture();
    let mut steps = initial();
    steps.extend(open_steps());
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "Actions", b"4");
    session.wait_for_screen(&run_cell(123, 2));
    if mouse {
        session.click_visible_action(&run_cell(123, 2));
    } else {
        session.send_key(b"\r");
    }
    wait_row(&mut session, &["matrix test linux", "501"]);
    keyed(&mut session, mouse, "Rerun selected job", b"J");
    session.wait_for_screen("Action unavailable");
    session.wait_for_screen("Open the job to rerun before composing");
    assert!(!session.screen().contains("ACTIONS INTENT"));
    journal(&mut session, &fixture, 5);
    session.resize(80, 24);
    session.wait_for_screen("Action unavailable");
    session.wait_for_screen("Open the job to rerun before composing");
    if mouse {
        session.click_visible_action("[PgDn More]");
        session.click_visible_action("[PgUp Previous]");
        session.click_visible_action("[Esc Close]");
    } else {
        session.send_key(b"\x1b[6~\x1b[5~\x1b");
    }
    session
        .try_wait_for_condition(
            |screen| !screen.contains("Action unavailable"),
            "Disabled control explanation closed",
            std::time::Duration::from_secs(8),
        )
        .unwrap();
    let records = journal(&mut session, &fixture, 5);
    assert!(!records
        .iter()
        .any(|row| row["args"].as_array().is_some_and(|args| args
            .iter()
            .any(|arg| matches!(arg.as_str(), Some("POST" | "PUT" | "PATCH" | "DELETE"))))));
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn actions_run_view_mouse_exact_attempt_job_logs_and_annotations() {
    run_view(true)
}
fn run_view(mouse: bool) {
    let fixture = fixture();
    let mut steps = initial();
    steps.extend(open_steps());
    steps.push(GhStep::read(
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
        "matrix test linux\tRun tests\tERROR fixture assertion failed\nA useful next line\n",
    ));
    steps.push(read("repos/example/repo/check-runs/600/annotations?per_page=100",json!([[{"path":"src/lib.rs","start_line":4,"end_line":5,"annotation_level":"failure","title":"Fixture assertion","message":"Expected true"}]]),true));
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "Actions", b"4");
    session.wait_for_screen(&run_cell(123, 2));
    if mouse {
        session.click_visible_action(&run_cell(123, 2))
    } else {
        session.send_key(b"\r")
    };
    wait_row(&mut session, &["matrix test linux", "501"]);
    session.wait_for_screen("trigger pull_request");
    session.wait_for_screen("@octocat");
    journal(&mut session, &fixture, 5);
    if mouse {
        session.click_visible_action("matrix test linux")
    } else {
        for _ in 0..5 {
            session.send_key(b"\x1b[B");
        }
        session.send_key(b"\r");
    }
    session.wait_for_screen("3 Run tests");
    session.wait_for_screen("Run tests");
    choose(&mut session, mouse, "[l Logs]", b"l");
    session.wait_for_screen("ERROR fixture assertion failed");
    session.wait_for_screen("A useful next line");
    journal(&mut session, &fixture, 6);
    choose(&mut session, mouse, "[N Annotations]", b"N");
    session.wait_for_screen("src/lib.rs:4–5");
    session.wait_for_screen("Expected true");
    let records = journal(&mut session, &fixture, 7);
    assert!(records[5]["args"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| arg == "501"));
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    session.resize(80, 24);
    session.wait_for_screen("src/lib.rs:4–5");
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn actions_run_control_keyboard_exact_job_debug_watch_cancel_and_cancelled_delete() {
    run_control(false)
}
#[test]
fn actions_run_control_mouse_exact_job_debug_watch_cancel_and_cancelled_delete() {
    run_control(true)
}
fn run_control(mouse: bool) {
    let fixture = fixture();
    let mut steps = initial();
    steps.extend(open_steps());
    let mut queued = raw_run();
    queued["run_attempt"] = json!(3);
    queued["status"] = json!("queued");
    queued["conclusion"] = Value::Null;
    steps.extend(compose_identity_steps());
    let rerun_open_count = steps.len();
    steps.extend(current_account());
    steps.extend(run_guard(raw_run()));
    steps.extend([
        read("repos/example/repo/actions/jobs/501",json!({"id":501,"run_id":123,"run_attempt":2,"head_sha":"a".repeat(40),"status":"completed","conclusion":"failure"}),false),
        write("repos/example/repo/actions/jobs/501/rerun",Some(json!({"enable_debug_logging":true})),Value::Null),
        read("repos/example/repo/actions/runs/123",queued.clone(),false),
    ]);
    steps.extend(current_account());
    let rerun_count = steps.len();
    steps.extend(compose_identity_steps());
    steps.extend(publication_support::inspect_run(Some(queued.clone()), 42));
    let inspected_count = steps.len();
    steps.extend(current_account());
    steps.push(read(
        "repos/example/repo/actions/runs?per_page=100",
        json!({"total_count":1,"workflow_runs":[queued.clone()]}),
        false,
    ));
    steps.extend(current_account());
    let acknowledged_count = steps.len();
    steps.push(read(
        "repos/example/repo/actions/runs?per_page=100",
        json!({"total_count":1,"workflow_runs":[queued.clone()]}),
        false,
    ));
    steps.extend(publication_support::open_attempt(&queued, vec![]));
    steps.extend(compose_identity_steps());
    steps.extend(current_account());
    steps.extend(run_guard(queued.clone()));
    let mut cancelled = queued;
    cancelled["status"] = json!("completed");
    cancelled["conclusion"] = json!("cancelled");
    steps.extend([
        write(
            "repos/example/repo/actions/runs/123/cancel",
            None,
            Value::Null,
        ),
        read("repos/example/repo/actions/runs/123", cancelled, false),
    ]);
    steps.extend(current_account());
    let cancel_count = steps.len();
    steps.extend(compose_identity_steps());
    let final_count = steps.len();
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "Actions", b"4");
    session.wait_for_screen(&run_cell(123, 2));
    if mouse {
        session.click_visible_action(&run_cell(123, 2))
    } else {
        session.send_key(b"\r")
    };
    wait_row(&mut session, &["matrix test linux", "501"]);
    if mouse {
        session.click_visible_action("matrix test linux")
    } else {
        for _ in 0..5 {
            session.send_key(b"\x1b[B");
        }
        session.send_key(b"\r");
    }
    session.wait_for_screen("3 Run tests");
    keyed(&mut session, mouse, "Rerun selected job", b"J");
    session.wait_for_screen("ACTIONS INTENT");
    journal(&mut session, &fixture, rerun_open_count);
    // An attempted confirmation without Preview is a local validation error, not a request.
    confirm(&mut session, mouse, 1);
    session.wait_for_screen("Choose Preview");
    journal(&mut session, &fixture, rerun_open_count);
    if !mouse {
        session.send_key(b"\x1b[Z\x1b[Z\x1b[Z");
    }
    if mouse {
        session.click_visible_action("[true]")
    } else {
        session.send_key(b" ")
    };
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    session.wait_for_screen("debug: true");
    journal(&mut session, &fixture, rerun_open_count);
    confirm(&mut session, mouse, 1);
    session.wait_for_screen("attempt 3");
    session.wait_for_screen("requested state transition was observed");
    journal(&mut session, &fixture, rerun_count);
    let accepted =
        publication_support::durable_rows(&fixture.root.join("config/ghtui/cache.sqlite3"));
    assert_eq!(accepted.len(), 1);
    assert_eq!(accepted[0].state, "accepted_pending");
    assert_eq!(accepted[0].context["attempt"], 2);
    inspect_original_publication(&mut session, mouse, 1);
    journal(&mut session, &fixture, inspected_count);
    command(&mut session, mouse, "Acknowledge inspected state");
    session.wait_for_screen("Original affected state acknowledged");
    journal(&mut session, &fixture, acknowledged_count);
    choose(&mut session, mouse, "Runs", b"R");
    session.wait_for_screen(&run_cell(123, 3));
    if mouse {
        session.click_visible_action(&run_cell(123, 3));
    } else {
        session.send_key(b"\r");
    }
    session.wait_for_screen("Run 123 • attempt 3");
    // TASK-117: the queued attempt is followed live without pressing Watch.
    session.wait_for_screen("Live: updates every 5s");
    session.wait_for_control("[w Stop watch]");
    command(&mut session, mouse, "Cancel run");
    session.wait_for_screen("ACTIONS INTENT");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    confirm(&mut session, mouse, 0);
    session.wait_for_screen("cancelled");
    journal(&mut session, &fixture, cancel_count);
    // The cancelled run has completed, so its live updates stopped by themselves.
    session.wait_for_control("[w Watch]");
    keyed(&mut session, mouse, "Delete run", b"-");
    session.wait_for_screen("ACTIONS INTENT");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    close_form(&mut session, mouse);
    let rows = journal(&mut session, &fixture, final_count);
    assert_eq!(
        rows.iter().filter(|row| row["args"][5] == "POST").count(),
        2
    );
    assert!(!rows.iter().any(|row| row["args"][5] == "DELETE"));
    assert_eq!(
        rows.iter().find(|row| row["args"][5] == "POST").unwrap()["args"][1],
        "repos/example/repo/actions/jobs/501/rerun"
    );
    let durable =
        publication_support::durable_rows(&fixture.root.join("config/ghtui/cache.sqlite3"));
    assert_eq!(durable.len(), 2);
    assert_eq!(durable[0].state, "transition_observed");
    assert_eq!(durable[0].context["attempt"], 2);
    assert_eq!(
        durable[0].receipt.as_ref().unwrap()["kind"],
        "run_transition"
    );
    assert_eq!(durable[1].state, "confirmed");
    assert_eq!(durable[1].context["attempt"], 3);
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    session.resize(80, 24);
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn actions_secrets_vars_keyboard_masked_transient_secret_variable_and_cancelled_deletions() {
    secrets_vars(false)
}
#[test]
fn actions_secrets_vars_mouse_masked_transient_secret_variable_and_cancelled_deletions() {
    secrets_vars(true)
}
fn secrets_vars(mouse: bool) {
    const SECRET: &str = "synthetic-tui-secret-never-persist";
    let fixture = fixture();
    let mut steps = initial();
    steps.push(read(
        "repos/example/repo/actions/secrets?per_page=100",
        json!([{"secrets":[]}]),
        true,
    ));
    steps.extend(compose_identity_steps());
    let secret_open_count = steps.len();
    steps.extend(current_account());
    steps.extend([
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        read("user", json!({"id":42,"login":"octocat"}), false),
        read(
            "repos/example/repo/actions/secrets?per_page=100",
            json!([{"secrets":[]}]),
            true,
        ),
    ]);
    let mut set_secret = GhStep::read(
        &[
            "secret",
            "set",
            "DEPLOY_TOKEN",
            "--app",
            "actions",
            "--repo",
            "github.invalid/example/repo",
        ],
        "",
    );
    set_secret.stdin = SECRET.as_bytes().to_vec();
    steps.push(set_secret);
    steps.push(read("repos/example/repo/actions/secrets?per_page=100",json!([{"secrets":[{"name":"DEPLOY_TOKEN","created_at":"2026-10-13T12:00:00Z","updated_at":"2026-10-13T12:00:00Z"}]}]),true));
    steps.extend(current_account());
    let secret_confirmed_count = steps.len();
    steps.extend(compose_identity_steps());
    let delete_secret_open_count = steps.len();
    let variable = json!({"name":"MODE","value":"east","created_at":"2026-10-13T12:00:00Z","updated_at":"2026-10-13T12:00:00Z"});
    steps.push(read(
        "repos/example/repo/actions/variables?per_page=100",
        json!([{"variables":[variable.clone()]}]),
        true,
    ));
    steps.extend(compose_identity_steps());
    steps.extend(current_account());
    let mut changed = variable.clone();
    changed["value"] = json!("west");
    changed["updated_at"] = json!("2026-10-13T12:01:00Z");
    steps.extend([
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        read("user", json!({"id":42,"login":"octocat"}), false),
        read("repos/example/repo/actions/variables/MODE", variable, false),
    ]);
    let mut patch = write(
        "repos/example/repo/actions/variables/MODE",
        Some(json!({"name":"MODE","value":"west"})),
        changed.clone(),
    );
    patch.args[5] = "PATCH".into();
    steps.extend([
        patch,
        read("repos/example/repo/actions/variables/MODE", changed, false),
    ]);
    steps.extend(current_account());
    let variable_confirmed_count = steps.len();
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "Actions", b"4");
    session.wait_for_screen(&run_cell(123, 2));
    choose(&mut session, mouse, "Secrets", b"S");
    session.wait_for_screen("Existing secret values cannot be read");
    command(&mut session, mouse, "Set secret");
    session.wait_for_screen("ACTIONS INTENT");
    session.paste("DEPLOY_TOKEN");
    focus_field(&mut session, mouse, "New secret value", 1);
    session.paste(SECRET);
    session.wait_for_screen("Draft saved locally");
    assert!(!session.screen().contains(SECRET));
    choose(&mut session, mouse, "[External editor]", b"\x05");
    session.wait_for_screen("external-editor files are not used for secrets");
    journal(&mut session, &fixture, secret_open_count);
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    assert!(!session.screen().contains(SECRET));
    // Preview retains the active secret field; return keyboard focus to field0 before footer traversal.
    if !mouse {
        session.send_key(b"\x1b[Z");
    }
    confirm(&mut session, mouse, 2);
    session.wait_for_screen("Secret metadata confirmed");
    journal(&mut session, &fixture, secret_confirmed_count);
    command(&mut session, mouse, "Delete secret");
    session.wait_for_screen("ACTIONS INTENT");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    close_form(&mut session, mouse);
    journal(&mut session, &fixture, delete_secret_open_count);
    choose(&mut session, mouse, "Variables", b"v");
    session.wait_for_screen("MODE = east");
    command(&mut session, mouse, "Set variable");
    session.wait_for_screen("ACTIONS INTENT");
    focus_field(&mut session, mouse, "Variable value", 1);
    session.send_key(b"\x15");
    session.paste("west");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    if !mouse {
        session.send_key(b"\x1b[Z");
    }
    confirm(&mut session, mouse, 2);
    session.wait_for_screen("MODE = west");
    let records = journal(&mut session, &fixture, variable_confirmed_count);
    let secret_writes = records
        .iter()
        .filter(|row| row["args"][0] == "secret" && row["args"][1] == "set")
        .collect::<Vec<_>>();
    assert_eq!(secret_writes.len(), 1);
    assert_eq!(secret_writes[0]["stdin_hex"], "[REDACTED]");
    let durable =
        publication_support::durable_rows(&fixture.root.join("config/ghtui/cache.sqlite3"));
    assert_eq!(
        durable.len(),
        2,
        "Cancelled deletes must not allocate a publication"
    );
    assert!(durable
        .iter()
        .all(|row| row.state == "confirmed" && row.principal == "user-id:42"));
    assert_eq!(durable[0].kind, json!("set_secret"));
    assert_eq!(durable[1].kind, json!("set_variable"));
    assert_eq!(durable[0].target["Secret"]["name"], "DEPLOY_TOKEN");
    assert_eq!(durable[1].target["Variable"]["name"], "MODE");
    assert_eq!(
        durable[0].receipt.as_ref().unwrap()["kind"],
        "secret_metadata"
    );
    assert_eq!(
        durable[1].receipt.as_ref().unwrap()["kind"],
        "variable_metadata"
    );
    assert!(!session.inspect_command_journal(&fixture).contains(SECRET));
    assert!(!String::from_utf8_lossy(&session.transcript).contains(SECRET));
    assert!(!records.iter().any(|row| row["args"]
        .as_array()
        .is_some_and(|args| args.iter().any(|value| value == "DELETE"))));
    for path in product_files(&fixture.root.join("config")) {
        assert!(
            !std::fs::read(&path)
                .unwrap()
                .windows(SECRET.len())
                .any(|window| window == SECRET.as_bytes()),
            "secret persisted at {}",
            path.display()
        );
    }
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    session.resize(80, 24);
    session.wait_for_screen("MODE = west");
    session.quit_and_assert_restored_terminal(mouse);
}
pub(crate) fn focus_field(session: &mut Session, mouse: bool, label: &str, index: usize) {
    if mouse {
        session.click_owned_form_field("ACTIONS INTENT", label);
    } else {
        for _ in 0..index {
            session.send_key(b"\t");
        }
    }
}

const DISPATCH_YAML: &str = "name: Build dispatch\non:\n  workflow_dispatch:\n    inputs:\n      debug:\n        description: Debug build\n        type: boolean\n        default: false\n      mode:\n        description: Build mode\n        type: choice\n        default: safe\n        options: [safe, fast]\njobs:\n  build:\n    runs-on: ubuntu-latest\n    strategy:\n      matrix:\n        version: [stable, beta]\n    steps:\n      - run: echo fixture\n  report:\n    needs: build\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo report\n";

fn workflow(state: &str) -> Value {
    json!({"id":7,"name":"Build dispatch","path":".github/workflows/ci.yml","state":state})
}
fn workflow_guard(state: &str) -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        read("user", json!({"id":42,"login":"octocat"}), false),
        read(
            "repos/example/repo/actions/workflows/7",
            workflow(state),
            false,
        ),
    ]
}
#[test]
fn actions_workflow_controls_keyboard_typed_dispatch_disable_enable_and_cancel() {
    workflow_controls(false)
}
#[test]
fn actions_workflow_controls_mouse_typed_dispatch_disable_enable_and_cancel() {
    workflow_controls(true)
}
fn workflow_controls(mouse: bool) {
    let fixture = fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let mut steps = initial();
    steps.extend([
        read("repos/example/repo", json!({"id":1,"full_name":"example/repo","default_branch":"main"}), false),
        read("repos/example/repo/actions/workflows?per_page=100",json!([{"workflows":[workflow("active")]}]),true),
        read("repos/example/repo/commits/main",json!({"sha":"a".repeat(40)}),false),
        GhStep::read(&["api","repos/example/repo/contents/.github/workflows/ci.yml?ref=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","--hostname","github.invalid","--method","GET","--header","Accept: application/vnd.github.raw+json","--allow-escape-sequences"], DISPATCH_YAML),
    ]);
    let mut dispatched = raw_run();
    dispatched["id"] = json!(200);
    dispatched["run_attempt"] = json!(1);
    dispatched["event"] = json!("workflow_dispatch");
    dispatched["head_branch"] = json!("main");
    dispatched["status"] = json!("queued");
    dispatched["conclusion"] = Value::Null;
    steps.extend(compose_identity_steps());
    let disable_cancelled_count = steps.len();
    steps.extend(compose_identity_steps());
    let dispatch_open_count = steps.len();
    steps.extend(current_account());
    steps.extend(workflow_guard("active"));
    steps.extend([
        read(
            "repos/example/repo/commits/main",
            json!({"sha":"a".repeat(40)}),
            false,
        ),
        write(
            "repos/example/repo/actions/workflows/7/dispatches",
            Some(json!({"ref":"main","inputs":{"debug":false,"mode":"fast"}})),
            json!({"workflow_run_id":200}),
        ),
        read(
            "repos/example/repo/actions/runs/200",
            dispatched.clone(),
            false,
        ),
    ]);
    steps.extend(current_account());
    let dispatch_count = steps.len();
    steps.extend(compose_identity_steps());
    steps.extend(current_account());
    steps.extend([
        read(
            "repos/example/repo/actions/workflows?per_page=100",
            json!([{"workflows":[workflow("active")]}]),
            true,
        ),
        read(
            "repos/example/repo/actions/workflows/7/runs?per_page=100",
            json!({"total_count":1,"workflow_runs":[dispatched.clone()]}),
            false,
        ),
    ]);
    steps.extend(current_account());
    let inspected_count = steps.len();
    steps.extend(current_account());
    steps.push(read(
        "repos/example/repo/actions/runs?per_page=100",
        json!({"total_count":1,"workflow_runs":[dispatched.clone()]}),
        false,
    ));
    steps.extend(current_account());
    let acknowledged_count = steps.len();
    steps.extend([
        read("repos/example/repo", json!({"id":1,"full_name":"example/repo","default_branch":"main"}), false),
        read("repos/example/repo/actions/workflows?per_page=100",json!([{"workflows":[workflow("active")]}]),true),
        read("repos/example/repo/commits/main",json!({"sha":"a".repeat(40)}),false),
        GhStep::read(&["api","repos/example/repo/contents/.github/workflows/ci.yml?ref=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","--hostname","github.invalid","--method","GET","--header","Accept: application/vnd.github.raw+json","--allow-escape-sequences"], DISPATCH_YAML),
    ]);
    let mut state_counts = vec![];
    for (before, effect, after) in [
        ("active", "disable", "disabled_manually"),
        ("disabled_manually", "enable", "active"),
    ] {
        if effect == "disable" {
            steps.extend(current_account());
        } // Restore old shared workflow draft before F9 new intent.
        steps.extend(compose_identity_steps());
        steps.extend(current_account());
        steps.extend(workflow_guard(before));
        let mut change = write(
            &format!("repos/example/repo/actions/workflows/7/{effect}"),
            None,
            Value::Null,
        );
        change.args[5] = "PUT".into();
        steps.extend([
            change,
            read(
                "repos/example/repo/actions/workflows/7",
                workflow(after),
                false,
            ),
        ]);
        steps.extend(current_account());
        state_counts.push(steps.len());
    }
    steps.push(read(
        "repos/example/repo/actions/runs?per_page=100",
        json!({"total_count":1,"workflow_runs":[dispatched]}),
        false,
    ));
    let final_count = steps.len();
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "Actions", b"4");
    session.wait_for_screen(&run_cell(123, 2));
    choose(&mut session, mouse, "Workflows", b"W");
    session.wait_for_screen("Build dispatch");
    journal(&mut session, &fixture, 5);
    if mouse {
        session.click_visible_action("Build dispatch");
    } else {
        session.send_key(b"\r");
    }
    session.wait_for_screen("revision main");
    session.wait_for_screen("Input debug");
    session.wait_for_screen("Definition job report");
    journal(&mut session, &fixture, 7);
    command(&mut session, mouse, "Disable workflow");
    session.wait_for_screen("ACTIONS INTENT");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    close_form(&mut session, mouse);
    journal(&mut session, &fixture, disable_cancelled_count);
    command(&mut session, mouse, "Dispatch workflow");
    session.wait_for_screen("ACTIONS INTENT");
    focus_field(&mut session, mouse, "mode — Build mode", 2);
    if mouse {
        session.click_visible_action("[fast]");
    } else {
        session.send_key(b" ");
    }
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    session.wait_for_screen("fast");
    journal(&mut session, &fixture, dispatch_open_count);
    if !mouse {
        session.send_key(b"\x1b[Z\x1b[Z");
    }
    confirm(&mut session, mouse, 3);
    session.wait_for_screen("Workflow run #200 started on main");
    journal(&mut session, &fixture, dispatch_count);
    let accepted =
        publication_support::durable_rows(&fixture.root.join("config/ghtui/cache.sqlite3"));
    assert_eq!(accepted.len(), 1);
    assert_eq!(accepted[0].state, "accepted_pending");
    assert_eq!(
        accepted[0].receipt.as_ref().unwrap()["observed_run_id"],
        200
    );
    inspect_original_publication(&mut session, mouse, 1);
    journal(&mut session, &fixture, inspected_count);
    command(&mut session, mouse, "Acknowledge inspected state");
    session.wait_for_screen("Original affected state acknowledged");
    journal(&mut session, &fixture, acknowledged_count);
    choose(&mut session, mouse, "Workflows", b"W");
    session.wait_for_screen("Build dispatch");
    if mouse {
        session.click_visible_action("Build dispatch");
    } else {
        session.send_key(b"\r");
    }
    session.wait_for_screen("revision main");
    for (label, state, count) in [
        ("Disable workflow", "disabled manually", state_counts[0]),
        ("Enable workflow", "active", state_counts[1]),
    ] {
        command(&mut session, mouse, label);
        if label == "Disable workflow" {
            session.wait_for_screen("ORIGINAL ACTIONS ATTEMPT #1");
            session.wait_for_screen("Correlated launch: run #200");
            choose(&mut session, mouse, "[F9 New intent]", b"\x1b[20~");
        }
        session.wait_for_screen("ACTIONS INTENT");
        choose(&mut session, mouse, "[Preview]", b"\x10");
        session.wait_for_screen("READ-ONLY PREVIEW");
        confirm(&mut session, mouse, 0);
        session.wait_for_screen(&format!("Workflow Build dispatch is now {state}"));
        journal(&mut session, &fixture, count);
    }
    choose(&mut session, mouse, "Runs", b"R");
    wait_dispatched_run(&mut session);
    let rows = journal(&mut session, &fixture, final_count);
    let writes = rows
        .iter()
        .filter(|row| row["args"][5] == "POST" || row["args"][5] == "PUT")
        .collect::<Vec<_>>();
    assert_eq!(writes.len(), 3);
    assert_eq!(
        writes[0]["args"][1],
        "repos/example/repo/actions/workflows/7/dispatches"
    );
    let durable =
        publication_support::durable_rows(&fixture.root.join("config/ghtui/cache.sqlite3"));
    assert_eq!(durable.len(), 3);
    assert_eq!(durable[0].state, "transition_observed");
    assert_eq!(durable[0].context["workflow_commit"], "a".repeat(40));
    assert_eq!(durable[0].receipt.as_ref().unwrap()["kind"], "dispatch");
    assert!(durable[1..]
        .iter()
        .all(|row| row.state == "confirmed"
            && row.receipt.as_ref().unwrap()["kind"] == "workflow_state"));
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    session.resize(80, 24);
    wait_dispatched_run(&mut session);
    session.quit_and_assert_restored_terminal(mouse);
}

fn product_files(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut files = vec![];
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for item in std::fs::read_dir(directory).unwrap() {
            let path = item.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files
}

#[test]
fn actions_cache_keyboard_filter_usage_cancel_and_exact_delete_preserves_other_cache() {
    cache_management(false)
}
#[test]
fn actions_cache_mouse_filter_usage_cancel_and_exact_delete_preserves_other_cache() {
    cache_management(true)
}
fn cache_management(mouse: bool) {
    let fixture = fixture();
    let cache = json!({"id":71,"key":"cargo/linux/v1","ref":"refs/heads/main","version":"v1","size_in_bytes":2048,"created_at":"2026-10-12T12:00:00Z","last_accessed_at":"2026-10-13T12:00:00Z"});
    let other = json!({"id":72,"key":"docs/index","ref":"refs/heads/docs","version":"v2","size_in_bytes":4096,"created_at":"2026-10-11T12:00:00Z","last_accessed_at":"2026-10-12T12:00:00Z"});
    let usage = json!({"active_caches_count":2,"active_caches_size_in_bytes":6144});
    let updated_usage = json!({"active_caches_count":1,"active_caches_size_in_bytes":4096});
    let mut steps = initial();
    steps.extend([
        read(
            "repos/example/repo/actions/caches?per_page=100",
            json!([{"actions_caches":[cache.clone(),other.clone()]}]),
            true,
        ),
        read(
            "repos/example/repo/actions/cache/usage",
            usage.clone(),
            false,
        ),
    ]);
    let cache_catalogue_count = steps.len();
    steps.extend(compose_identity_steps());
    let filter_open_count = steps.len();
    steps.extend(current_account());
    steps.extend([
        read("repos/example/repo/actions/caches?per_page=100&key=cargo%2Flinux&ref=refs%2Fheads%2Fmain",json!([{"actions_caches":[cache.clone()]}]),true),
        read("repos/example/repo/actions/cache/usage",usage,false),
    ]);
    steps.extend(current_account());
    let filtered_count = steps.len();
    steps.extend(compose_identity_steps());
    let cancelled_open_count = steps.len();
    steps.extend(compose_identity_steps());
    steps.extend(current_account());
    steps.extend([
        GhStep::read(&["--version"],"gh version 2.101.0\n"),
        read("user",json!({"id":42,"login":"octocat"}),false),
        read("repos/example/repo/actions/caches?per_page=100&key=cargo%2Flinux%2Fv1&ref=refs%2Fheads%2Fmain",json!([{"actions_caches":[cache]}]),true),
    ]);
    let mut deletion = write("repos/example/repo/actions/caches/71", None, Value::Null);
    deletion.args[5] = "DELETE".into();
    steps.extend([
        deletion,
        read(
            "repos/example/repo/actions/caches?per_page=100",
            json!([{"actions_caches":[other]}]),
            true,
        ),
        read(
            "repos/example/repo/actions/cache/usage",
            updated_usage.clone(),
            false,
        ),
    ]);
    steps.extend(current_account());
    let deleted_count = steps.len();
    steps.extend([
        read(
            "repos/example/repo/actions/cache/usage",
            updated_usage,
            false,
        ),
        // TASK-58: Usage also reads the host's storage and retention limits.
        read(
            "repos/example/repo/actions/cache/storage-limit",
            json!({"max_cache_size_gb":10}),
            false,
        ),
        read(
            "repos/example/repo/actions/cache/retention-limit",
            json!({"max_cache_retention_days":7}),
            false,
        ),
    ]);
    let final_count = steps.len();
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "Actions", b"4");
    session.wait_for_screen(&run_cell(123, 2));
    choose(&mut session, mouse, "Caches", b"c");
    session.wait_for_screen("#71 • cargo/linux/v1");
    session.wait_for_screen("#72 • docs/index");
    session.wait_for_screen("6144 bytes in 2 caches");
    journal(&mut session, &fixture, cache_catalogue_count);
    command(&mut session, mouse, "Filter caches");
    session.wait_for_screen("ACTIONS INTENT");
    session.paste("cargo/linux");
    focus_field(&mut session, mouse, "Exact Git ref", 1);
    session.paste("refs/heads/main");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    journal(&mut session, &fixture, filter_open_count);
    if !mouse {
        session.send_key(b"\x1b[Z");
    }
    confirm(&mut session, mouse, 2);
    session.wait_for_screen("1 displayed");
    session.wait_for_screen("6144 bytes in 2 caches");
    assert!(!session.screen().contains("#72 • docs/index"));
    journal(&mut session, &fixture, filtered_count);
    command(&mut session, mouse, "Delete selected cache");
    session.wait_for_screen("ACTIONS INTENT");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    session.wait_for_screen("71");
    close_form(&mut session, mouse);
    journal(&mut session, &fixture, cancelled_open_count);
    command(&mut session, mouse, "Delete selected cache");
    session.wait_for_screen("ACTIONS INTENT");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    confirm(&mut session, mouse, 1);
    session.wait_for_screen("Only cache 71 deleted; unrelated caches preserved");
    session.wait_for_screen("#72 • docs/index");
    session.wait_for_screen("4096 bytes in 1 caches");
    assert!(!session.screen().contains("#71 • cargo/linux/v1"));
    journal(&mut session, &fixture, deleted_count);
    choose(&mut session, mouse, "Usage", b"u");
    session.wait_for_screen("Cache storage: 4096 bytes in 1 caches");
    session.wait_for_screen("Host cache limits: storage up to 10 GB • retention up to 7 days");
    let rows = journal(&mut session, &fixture, final_count);
    let writes = rows
        .iter()
        .filter(|row| row["args"][5] == "DELETE")
        .collect::<Vec<_>>();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0]["args"][1], "repos/example/repo/actions/caches/71");
    let durable =
        publication_support::durable_rows(&fixture.root.join("config/ghtui/cache.sqlite3"));
    assert_eq!(
        durable.len(),
        1,
        "Local filters and cancelled deletes are not remote publications"
    );
    assert_eq!(durable[0].state, "confirmed");
    assert_eq!(durable[0].kind, json!("delete_cache"));
    assert_eq!(durable[0].target["Cache"]["id"], 71);
    assert_eq!(
        durable[0].receipt.as_ref().unwrap()["kind"],
        "cache_deleted"
    );
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    session.resize(80, 24);
    session.wait_for_screen("Cache storage: 4096 bytes in 1 caches");
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn actions_run_control_keyboard_uncertain_cancel_refresh_inspect_ack_then_guarded_retry() {
    uncertain_cancel(false)
}
#[test]
fn actions_run_control_mouse_uncertain_cancel_refresh_inspect_ack_then_guarded_retry() {
    uncertain_cancel(true)
}
fn uncertain_cancel(mouse: bool) {
    let fixture = fixture();
    let mut queued = raw_run();
    queued["status"] = json!("queued");
    queued["conclusion"] = Value::Null;
    let catalogue = read(
        "repos/example/repo/actions/runs?per_page=100",
        json!({"total_count":1,"workflow_runs":[queued.clone()]}),
        false,
    );
    let attempt = read(
        "repos/example/repo/actions/runs/123/attempts/2",
        queued.clone(),
        false,
    );
    let jobs = read(
        "repos/example/repo/actions/runs/123/attempts/2/jobs?per_page=100",
        json!([{"jobs":[]}]),
        true,
    );
    let mut steps = initial();
    steps[2] = catalogue.clone();
    steps.extend([attempt.clone(), jobs.clone()]);
    steps.extend(compose_identity_steps());
    steps.extend(current_account());
    steps.extend(run_guard(queued.clone()));
    let mut uncertain = write(
        "repos/example/repo/actions/runs/123/cancel",
        None,
        Value::Null,
    );
    uncertain.status = 1;
    uncertain.stderr = b"connection closed after request; result unknown".to_vec();
    steps.push(uncertain);
    steps.extend(current_account());
    let unknown_count = steps.len();
    // A normal refresh observes data but never clears the failed-write barrier.
    steps.extend(current_account());
    steps.extend([attempt.clone(), jobs.clone()]);
    let refresh_count = steps.len();
    // List4 has no remote resource reads; inspect follows separately.
    steps.extend(compose_identity_steps());
    // Explicit original inspection produces an opaque fresh exact-target proof.
    steps.extend([
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        read("user", json!({"id":42,"login":"octocat"}), false),
        catalogue.clone(),
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        read("user", json!({"id":42,"login":"octocat"}), false),
    ]);
    let inspected_count = steps.len();
    // Explicit local ACK: fresh identity2, optional catalogue1, final identity2.
    steps.extend(current_account());
    steps.push(catalogue.clone());
    steps.extend(current_account());
    let acknowledged_count = steps.len();
    steps.push(catalogue);
    steps.extend([attempt, jobs]);
    let reopened_count = steps.len();
    // Reopening the old draft restores its ORIGINAL reference before fields.
    steps.extend(current_account());
    steps.extend(compose_identity_steps());
    let restored_count = steps.len();
    steps.extend(current_account());
    steps.extend(run_guard(queued.clone()));
    let mut cancelled = queued;
    cancelled["status"] = json!("completed");
    cancelled["conclusion"] = json!("cancelled");
    steps.extend([
        write(
            "repos/example/repo/actions/runs/123/cancel",
            None,
            Value::Null,
        ),
        read("repos/example/repo/actions/runs/123", cancelled, false),
    ]);
    steps.extend(current_account());
    let final_count = steps.len();
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "Actions", b"4");
    session.wait_for_screen(&run_cell(123, 2));
    if mouse {
        session.click_visible_action(&run_cell(123, 2));
    } else {
        session.send_key(b"\r");
    }
    session.wait_for_screen("Run 123 • attempt 2");
    command(&mut session, mouse, "Cancel run");
    session.wait_for_screen("ACTIONS INTENT");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    confirm(&mut session, mouse, 0);
    session.wait_for_screen("result unknown");
    journal(&mut session, &fixture, unknown_count);
    let original =
        publication_support::durable_rows(&fixture.root.join("config/ghtui/cache.sqlite3"));
    assert_eq!(original.len(), 1);
    assert_eq!(original[0].state, "uncertain");
    assert_eq!(original[0].kind, json!("cancel"));
    assert_eq!(original[0].context["run_id"], 123);
    assert_eq!(original[0].context["attempt"], 2);
    assert_eq!(original[0].context["head_sha"], "a".repeat(40));
    assert!(original[0].receipt.is_none());
    close_form(&mut session, mouse);
    choose(&mut session, mouse, "[r Refresh]", b"r");
    journal(&mut session, &fixture, refresh_count);
    command(&mut session, mouse, "Cancel run");
    session.wait_for_screen("Action unavailable");
    session.wait_for_screen("Original attempt #1");
    assert!(!session.screen().contains("ACTIONS INTENT"));
    journal(&mut session, &fixture, refresh_count);
    if mouse {
        session.click_visible_action("[Esc Close]");
    } else {
        session.send_key(b"\x1b");
    }
    session
        .try_wait_for_condition(
            |screen| !screen.contains("Action unavailable"),
            "Uncertain-write explanation closed",
            std::time::Duration::from_secs(8),
        )
        .unwrap();
    inspect_original_publication(&mut session, mouse, 1);
    journal(&mut session, &fixture, inspected_count);
    session.wait_for_screen("Original run: #123");
    // Inspecting and presenting the state alone still does not permit a mutation.
    let rows = journal(&mut session, &fixture, inspected_count);
    assert_eq!(
        rows.iter().filter(|row| row["args"][5] == "POST").count(),
        1
    );
    command(&mut session, mouse, "Acknowledge inspected state");
    session.wait_for_screen("Original affected state acknowledged");
    journal(&mut session, &fixture, acknowledged_count);
    let acknowledged =
        publication_support::durable_rows(&fixture.root.join("config/ghtui/cache.sqlite3"));
    assert_eq!(acknowledged.len(), 1);
    assert_eq!(acknowledged[0].state, "acknowledged");
    assert_eq!(acknowledged[0].context, original[0].context);
    assert!(acknowledged[0].receipt.is_none());
    choose(&mut session, mouse, "Runs", b"R");
    session.wait_for_screen(&run_cell(123, 2));
    if mouse {
        session.click_visible_action(&run_cell(123, 2));
    } else {
        session.send_key(b"\r");
    }
    session.wait_for_screen("Run 123 • attempt 2");
    journal(&mut session, &fixture, reopened_count);
    command(&mut session, mouse, "Cancel run");
    session.wait_for_screen("ORIGINAL ACTIONS ATTEMPT #1");
    session.wait_for_screen("Original run: #123");
    journal(&mut session, &fixture, restored_count);
    choose(&mut session, mouse, "[F9 New intent]", b"\x1b[20~");
    session.wait_for_screen("ACTIONS INTENT");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    confirm(&mut session, mouse, 0);
    session.wait_for_screen("cancelled");
    let rows = journal(&mut session, &fixture, final_count);
    let writes = rows
        .iter()
        .filter(|row| row["args"][5] == "POST")
        .collect::<Vec<_>>();
    assert_eq!(writes.len(), 2);
    assert!(writes
        .iter()
        .all(|row| row["args"][1] == "repos/example/repo/actions/runs/123/cancel"));
    let durable =
        publication_support::durable_rows(&fixture.root.join("config/ghtui/cache.sqlite3"));
    assert_eq!(durable.len(), 2);
    assert_eq!(durable[0].state, "acknowledged");
    assert_eq!(durable[1].state, "confirmed");
    assert_eq!(
        durable[1].receipt.as_ref().unwrap()["kind"],
        "run_transition"
    );
    assert_eq!(durable[1].receipt.as_ref().unwrap()["observed_attempt"], 2);
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    session.resize(80, 24);
    session.wait_for_screen("cancelled");
    session.quit_and_assert_restored_terminal(mouse);
}

fn artifact_png() -> Vec<u8> {
    use image::ImageEncoder;
    let mut bytes = vec![];
    let pixels = [240, 40, 20, 255].repeat(64);
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(&pixels, 8, 8, image::ExtendedColorType::Rgba8)
        .unwrap();
    bytes
}
pub(crate) fn artifact_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::{Cursor, Write};
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (path, bytes) in entries {
        archive
            .start_file(
                *path,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored)
                    .unix_permissions(0o755),
            )
            .unwrap();
        archive.write_all(bytes).unwrap();
    }
    archive.finish().unwrap().into_inner()
}
pub(crate) fn artifact_value(id: u64, name: &str, bytes: &[u8], expired: bool) -> Value {
    use sha2::{Digest, Sha256};
    json!({"id":id,"name":name,"size_in_bytes":bytes.len(),"expired":expired,"created_at":"2026-10-12T12:00:00Z","updated_at":"2026-10-12T12:00:00Z","expires_at":"2026-10-20T12:00:00Z","digest":format!("sha256:{:x}",Sha256::digest(bytes)),"workflow_run":{"id":123,"head_sha":"a".repeat(40)}})
}
pub(crate) fn artifact_download(id: u64, bytes: Vec<u8>) -> GhStep {
    let mut step = read(
        &format!("repos/example/repo/actions/artifacts/{id}/zip"),
        Value::Null,
        false,
    );
    step.args.push("--allow-escape-sequences".into());
    step.stdout = bytes;
    step
}
#[test]
fn actions_artifacts_keyboard_previews_exports_extracts_deletes_and_rejects_unsafe_archive() {
    artifact_management(false)
}
#[test]
fn actions_artifacts_mouse_previews_exports_extracts_deletes_and_rejects_unsafe_archive() {
    artifact_management(true)
}
fn artifact_management(mouse: bool) {
    use std::os::unix::fs::PermissionsExt;
    let fixture = fixture();
    let png = artifact_png();
    let text = b"artifact fixture report\nall checks explained\n";
    let zip = artifact_zip(&[
        ("report.txt", text),
        ("pixel.png", &png),
        ("binary.bin", &[0, 255, 1, 2]),
    ]);
    let unsafe_zip = artifact_zip(&[("../outside", b"never publish")]);
    let valid = artifact_value(41, "build-output", &zip, false);
    let expired = artifact_value(42, "old-output", &zip, true);
    let unsafe_value = artifact_value(43, "unsafe-output", &unsafe_zip, false);
    let mut steps = initial();
    steps.extend(open_steps());
    steps.extend([
        read(
            "repos/example/repo/actions/runs/123/artifacts?per_page=100",
            json!([{"artifacts":[valid.clone(),expired.clone(),unsafe_value.clone()]}]),
            true,
        ),
        read(
            "repos/example/repo/actions/artifacts/41",
            valid.clone(),
            false,
        ),
        artifact_download(41, zip.clone()),
    ]);
    let archive_read_count = steps.len();
    // Two local archive effects: OPEN4, pre2, filesystem effect, final2.
    for _ in 0..2 {
        steps.extend(compose_identity_steps());
        steps.extend(current_account());
        steps.extend(current_account());
    }
    let archive_effects_count = steps.len();
    // Existing destination refusal has no effect and checks pre2/final2.
    steps.extend(compose_identity_steps());
    steps.extend(current_account());
    steps.extend(current_account());
    let existing_export_count = steps.len();
    steps.extend(compose_identity_steps());
    let cancelled_delete_count = steps.len();
    steps.extend(compose_identity_steps());
    steps.extend(current_account());
    steps.extend([
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        read("user", json!({"id":42,"login":"octocat"}), false),
        read("repos/example/repo/actions/artifacts/41", valid, false),
    ]);
    let mut deletion = write("repos/example/repo/actions/artifacts/41", None, Value::Null);
    deletion.args[5] = "DELETE".into();
    steps.extend([
        deletion,
        read(
            "repos/example/repo/actions/runs/123/artifacts?per_page=100",
            json!([{"artifacts":[expired,unsafe_value.clone()]}]),
            true,
        ),
    ]);
    steps.extend(current_account());
    let confirmed_delete_count = steps.len();
    steps.extend([
        read(
            "repos/example/repo/actions/artifacts/43",
            unsafe_value,
            false,
        ),
        artifact_download(43, unsafe_zip),
    ]);
    let final_count = steps.len();
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "Actions", b"4");
    session.wait_for_screen(&run_cell(123, 2));
    if mouse {
        session.click_visible_action(&run_cell(123, 2));
    } else {
        session.send_key(b"\r");
    }
    wait_row(&mut session, &["matrix test linux", "501"]);
    choose(&mut session, mouse, "[a Artifacts]", b"a");
    session.wait_for_screen("build-output #41");
    session.wait_for_screen("attempt not exposed");
    journal(&mut session, &fixture, 6);
    if mouse {
        session.click_visible_action("build-output #41");
    } else {
        session.send_key(b"\r");
    }
    session.wait_for_screen("report.txt");
    session.wait_for_screen("SHA256 digest verified");
    journal(&mut session, &fixture, archive_read_count);
    if mouse {
        session.click_visible_action("report.txt");
    } else {
        session.send_key(b"\r");
    }
    session.wait_for_screen("artifact fixture report");
    session.wait_for_screen("all checks explained");
    if mouse {
        session.click_visible_action("pixel.png");
    } else {
        session.send_key(b"\x1b[H\x1b[B\r");
    }
    session.wait_for_screen("8×8 original pixels");
    if mouse {
        session.click_visible_action("binary.bin");
    } else {
        session.send_key(b"\x1b[H\x1b[B\x1b[B\r");
    }
    session.wait_for_screen("Binary bytes");
    session.wait_for_screen("00 ff 01 02");
    journal(&mut session, &fixture, archive_read_count);
    let export = fixture.root.join("explicit-output.zip");
    let extract = fixture.root.join("explicit-extracted");
    for (index, (label, key, path)) in [
        ("Export artifact ZIP", b"X", &export),
        ("Extract artifact into a new directory", b"U", &extract),
    ]
    .into_iter()
    .enumerate()
    {
        keyed(&mut session, mouse, label, key);
        session.wait_for_screen("ACTIONS INTENT");
        session.paste(path.to_str().unwrap());
        choose(&mut session, mouse, "[Preview]", b"\x10");
        session.wait_for_screen("READ-ONLY PREVIEW");
        confirm(&mut session, mouse, 1);
        session.wait_for_screen("without overwrite or execution");
        journal(&mut session, &fixture, archive_read_count + (index + 1) * 8);
    }
    assert_eq!(archive_effects_count, archive_read_count + 16);
    assert_eq!(std::fs::read(&export).unwrap(), zip);
    assert_eq!(std::fs::read(extract.join("report.txt")).unwrap(), text);
    assert_eq!(std::fs::read(extract.join("pixel.png")).unwrap(), png);
    assert_eq!(
        std::fs::metadata(extract.join("report.txt"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    // A second explicit export must preserve the existing bytes instead of overwriting.
    keyed(&mut session, mouse, "Export artifact ZIP", b"X");
    session.wait_for_screen("ACTIONS INTENT");
    session.send_key(b"\x15");
    session.paste(export.to_str().unwrap());
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    confirm(&mut session, mouse, 1);
    session.wait_for_screen("already exists");
    journal(&mut session, &fixture, existing_export_count);
    assert_eq!(std::fs::read(&export).unwrap(), zip);
    close_form(&mut session, mouse);
    keyed(&mut session, mouse, "Delete artifact", b"-");
    session.wait_for_screen("ACTIONS INTENT");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    close_form(&mut session, mouse);
    journal(&mut session, &fixture, cancelled_delete_count);
    keyed(&mut session, mouse, "Delete artifact", b"-");
    session.wait_for_screen("ACTIONS INTENT");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    confirm(&mut session, mouse, 0);
    session.wait_for_screen("Artifact 41 deletion confirmed");
    session.wait_for_screen("old-output #42");
    session.wait_for_screen("unsafe-output #43");
    journal(&mut session, &fixture, confirmed_delete_count);
    if mouse {
        session.click_visible_action("old-output #42");
    } else {
        session.send_key(b"\x1b[H\r");
    }
    session.wait_for_screen("Action unavailable");
    session
        .try_wait_for_condition(
            |screen| {
                screen
                    .chars()
                    .filter(|character| !character.is_whitespace() && *character != '│')
                    .collect::<String>()
                    .contains("GitHubcannotprovideitsarchive")
            },
            "Full expired-artifact explanation across terminal wrapping",
            std::time::Duration::from_secs(8),
        )
        .unwrap();
    if mouse {
        session.click_visible_action("[Esc Close]");
    } else {
        session.send_key(b"\x1b");
    }
    session
        .try_wait_for_condition(
            |screen| !screen.contains("Action unavailable"),
            "Expired artifact explanation closed",
            std::time::Duration::from_secs(8),
        )
        .unwrap();
    if mouse {
        session.click_visible_action("unsafe-output #43");
    } else {
        session.send_key(b"\x1b[H\x1b[B\r");
    }
    session.wait_for_screen("unsafe path");
    let records = journal(&mut session, &fixture, final_count);
    let writes = records
        .iter()
        .filter(|row| row["args"][5] == "DELETE")
        .collect::<Vec<_>>();
    assert_eq!(writes.len(), 1);
    assert_eq!(
        writes[0]["args"][1],
        "repos/example/repo/actions/artifacts/41"
    );
    assert!(!records
        .iter()
        .any(|row| row["args"][1] == "repos/example/repo/actions/artifacts/42"));
    let durable =
        publication_support::durable_rows(&fixture.root.join("config/ghtui/cache.sqlite3"));
    assert_eq!(
        durable.len(),
        1,
        "Local archive operations never allocate remote markers"
    );
    assert_eq!(durable[0].state, "confirmed");
    assert_eq!(durable[0].kind, json!("delete_artifact"));
    assert_eq!(durable[0].target["Artifact"]["id"], 41);
    assert_eq!(
        durable[0].receipt.as_ref().unwrap()["kind"],
        "artifact_deleted"
    );
    assert!(!fixture.root.join("outside").exists());
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    session.resize(80, 24);
    session.wait_for_screen("old-output #42");
    session.quit_and_assert_restored_terminal(mouse);
}

/// TASK-91: the Runs page is a table under a sticky header with the `▌` selection bar, short
/// SHAs, initials-chip actors and relative ages; opening a run keeps its full SHA and UTC
/// times in the detail rows above an inline jobs table. Rows stay keyboard and mouse targets
/// at 140x42 and 80x24, and browsing never sends a write or touches the checkout.
fn table_journey(mouse: bool) {
    let fixture = fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let mut other = raw_run();
    other["id"] = json!(124);
    other["name"] = json!("Release");
    other["head_branch"] = json!("main");
    other["event"] = json!("push");
    other["run_attempt"] = json!(1);
    other["conclusion"] = json!("success");
    other["head_sha"] = json!("b".repeat(40));
    other["created_at"] = json!("2026-10-12T12:00:00Z");
    let mut steps = initial();
    steps[2].stdout =
        serde_json::to_vec(&json!({"total_count":2,"workflow_runs":[raw_run(), other]})).unwrap();
    steps.extend(open_steps());
    // Choosing the Runs catalogue again re-reads the run list.
    steps.push(steps[2].clone());
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 140, 42);
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "Actions", b"4");
    for size in [(140, 42), (80, 24)] {
        session.resize(size.0, size.1);
        // Header, aligned cells, short SHAs, chips with logins, the attempt beside the id.
        wait_row(
            &mut session,
            &["WORKFLOW", "BRANCH", "EVENT", "COMMIT", "ACTOR", "AGE"],
        );
        wait_row(
            &mut session,
            &[
                "CI matrix",
                "feature",
                "pull_request",
                "aaaaaaa ",
                "AL alice",
                "123 #2",
            ],
        );
        wait_row(
            &mut session,
            &["Release", "main", "push", "bbbbbbb ", "124"],
        );
        // The list shows short SHAs; only the 140-column preview's Commit row has the full one.
        let list = list_pane(&session.screen());
        assert!(!list.contains(&"a".repeat(8)), "{list}");
        assert!(!session.screen().contains("@alice"));
        let row = wait_selected(&mut session, "CI matrix");
        assert!(row.starts_with("✗ CI matrix"), "{row:?}");
    }
    journal(&mut session, &fixture, 3);
    if mouse {
        session.click_visible_action(&run_cell(123, 2));
    } else {
        // Selection moves with the bar; Enter opens the selected run.
        session.send_key(b"\x1b[B");
        wait_selected(&mut session, "Release");
        session.send_key(b"\x1b[A");
        wait_selected(&mut session, "CI matrix");
        session.send_key(b"\r");
    }
    session.wait_for_screen("Run 123 • attempt 2");
    // Full SHA and UTC creation time stay in the detail rows.
    session.wait_for_screen(&format!("Exact commit {}", "a".repeat(40)));
    session.resize(140, 42);
    session.wait_for_screen("2026-10-13T12:00:00Z");
    wait_row(&mut session, &["JOB", "STATUS", "TOOK", "RUNNER", "ID"]);
    wait_row(
        &mut session,
        &[
            "matrix test linux",
            "failure",
            "hosted [ubuntu-latest]",
            "501",
        ],
    );
    // The run opens on its failed job; the actors are chips beside their role.
    wait_selected(&mut session, "matrix test linux");
    wait_row(&mut session, &["AL alice", "run actor"]);
    wait_row(&mut session, &["OC octocat", "triggering actor"]);
    journal(&mut session, &fixture, 5);
    // The inline jobs header is not a stop: Up from the job lands on the row above it.
    if mouse {
        session.click_visible_action("triggering actor");
        wait_selected(&mut session, "triggering actor");
        session.click_visible_action("matrix test linux");
    } else {
        session.send_key(b"\x1b[A");
        wait_selected(&mut session, "Attempt source");
        session.send_key(b"\x1b[B");
        wait_selected(&mut session, "matrix test linux");
        session.send_key(b"\r");
    }
    session.wait_for_screen("3 Run tests");
    wait_selected(&mut session, "matrix test linux");
    session.resize(80, 24);
    wait_selected(&mut session, "matrix test linux");
    wait_row(&mut session, &["3 Run tests", "failure"]);
    choose(&mut session, mouse, "Runs", b"R");
    wait_row(&mut session, &["CI matrix", "123 #2"]);
    wait_selected(&mut session, "CI matrix");
    let records = journal(&mut session, &fixture, 6);
    assert!(!records
        .iter()
        .any(|row| row["args"].as_array().is_some_and(|args| args
            .iter()
            .any(|arg| matches!(arg.as_str(), Some("POST" | "PUT" | "PATCH" | "DELETE"))))));
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn design_table_v91_actions_runs_table_keyboard_selection_bar_details_and_resize() {
    table_journey(false)
}
#[test]
fn design_table_v91_actions_runs_table_mouse_rows_open_and_keep_selection() {
    table_journey(true)
}

/// The `Run N` preview pane's columns on every screen line, when one is painted (TASK-92).
fn run_preview(screen: &str) -> Option<Vec<String>> {
    let x = screen.lines().find_map(|line| {
        let chars = line.chars().collect::<Vec<_>>();
        let x = chars
            .iter()
            .enumerate()
            .skip(1)
            .find(|(_, cell)| **cell == '╭')
            .map(|(x, _)| x)?;
        chars[x..]
            .iter()
            .collect::<String>()
            .contains("Run ")
            .then_some(x)
    })?;
    Some(
        screen
            .lines()
            .map(|line| line.chars().skip(x).collect())
            .collect(),
    )
}
fn wait_run_preview(session: &mut Session, parts: &[&str]) -> Vec<String> {
    session
        .try_wait_for_condition(
            |screen| {
                run_preview(screen).is_some_and(|lines| {
                    parts
                        .iter()
                        .all(|part| lines.iter().any(|line| line.contains(part)))
                })
            },
            &format!("a run preview holding {parts:?}"),
            std::time::Duration::from_secs(8),
        )
        .unwrap();
    run_preview(&session.screen()).unwrap()
}
/// TASK-92: at 140 columns the Runs list shows the selected run in a preview pane beside it,
/// with no read: its jobs show only once the run was opened. Tab or a click focuses the
/// preview (`▶` marker); Enter there or its `Enter Open` token opens the run as Enter on the
/// row does; back on Runs the preview lists the loaded jobs. Below 120 columns the list keeps
/// the whole pane. No write and no Git change.
fn split_journey(mouse: bool) {
    let fixture = fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let mut other = raw_run();
    other["id"] = json!(124);
    other["name"] = json!("Release");
    other["head_branch"] = json!("main");
    other["event"] = json!("push");
    other["run_attempt"] = json!(1);
    other["conclusion"] = json!("success");
    other["head_sha"] = json!("b".repeat(40));
    other["created_at"] = json!("2026-10-12T12:00:00Z");
    let mut steps = initial();
    steps[2].stdout =
        serde_json::to_vec(&json!({"total_count":2,"workflow_runs":[raw_run(), other]})).unwrap();
    steps.extend(open_steps());
    // Choosing the Runs catalogue again re-reads the run list.
    steps.push(steps[2].clone());
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 140, 42);
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "Actions", b"4");
    wait_selected(&mut session, "CI matrix");
    let preview = wait_run_preview(
        &mut session,
        &[
            "Run 123",
            "✗ FAILURE",
            "Enter Open",
            "CI matrix · pull_request · feature",
            "aaaaaaa · AL alice",
            &"a".repeat(40),
            "2026-10-13T12:00:00Z",
            "JOBS",
            "Jobs load when the run opens (Enter)",
        ],
    );
    assert!(!preview.iter().any(|line| line.contains("▶ Run")));
    journal(&mut session, &fixture, 3);
    let top = session
        .screen()
        .lines()
        .position(|line| line.contains("─ Run 123"))
        .unwrap() as u16;
    if mouse {
        session.click(110, top + 4);
        wait_run_preview(&mut session, &["▶ Run 123"]);
        let (x, y) = session.locate("Enter Open").expect("the Open token");
        session.click(x + 1, y);
    } else {
        session.send_key(b"\x1b[B");
        wait_selected(&mut session, "Release");
        wait_run_preview(
            &mut session,
            &["Run 124", "✓ SUCCESS", "Release · push · main"],
        );
        session.send_key(b"\x1b[A");
        wait_selected(&mut session, "CI matrix");
        wait_run_preview(&mut session, &["Run 123"]);
        journal(&mut session, &fixture, 3);
        session.send_key(b"\t");
        wait_run_preview(&mut session, &["▶ Run 123"]);
        session.send_key(b"\r");
    }
    session.wait_for_screen("Run 123 • attempt 2");
    journal(&mut session, &fixture, 5);
    // Back on Runs, the opened attempt's jobs are already loaded and show in the preview.
    choose(&mut session, mouse, "Runs", b"R");
    wait_selected(&mut session, "CI matrix");
    journal(&mut session, &fixture, 6);
    wait_run_preview(&mut session, &["Run 123", "matrix test linux", "FAILURE"]);
    session.resize(80, 24);
    session
        .try_wait_for_condition(
            |screen| run_preview(screen).is_none() && screen.contains("CI matrix"),
            "the run preview gone at 80x24",
            std::time::Duration::from_secs(8),
        )
        .unwrap();
    wait_selected(&mut session, "CI matrix");
    let records = journal(&mut session, &fixture, 6);
    assert!(!records
        .iter()
        .any(|row| row["args"].as_array().is_some_and(|args| args
            .iter()
            .any(|arg| matches!(arg.as_str(), Some("POST" | "PUT" | "PATCH" | "DELETE"))))));
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn design_split_v92_actions_runs_keyboard_preview_jobs_and_drill_down() {
    split_journey(false)
}
#[test]
fn design_split_v92_actions_runs_mouse_preview_jobs_and_drill_down() {
    split_journey(true)
}
