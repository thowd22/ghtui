//! TASK-65 `complete_journeys`, Actions: one session on the real executable goes Actions ->
//! a waiting run -> its failed job -> the failed step's log (the next-failure jump finds the
//! `##[error]` line under that step's header) -> the run's artifact list -> the artifact's
//! verified archive, exported to a temporary directory after Preview and Confirm (bytes
//! checked on disk) -> the run's pending deployment gate.
//!
//! The first approval meets an offline gh at its pre-write run guard. No POST is sent, the
//! gate stays pending, and the original attempt stays recorded as uncertain: Approve stays
//! disabled until that exact attempt is inspected and explicitly acknowledged. A second,
//! explicitly confirmed approval then sends exactly one review request, which the strict fake
//! gh checks byte for byte. Keyboard and mouse, 140x40 with a resize to 80x24 and back, and
//! the checkout is never touched.
use crate::actions::{
    artifact_download, artifact_value, artifact_zip, button, choose, close_form, command,
    compose_identity_steps, confirm, current_account, fixture, focus_field, initial,
    inspect_original_publication, journal, publication_support, raw_run, read, run_cell, run_guard,
    wait_row, wait_selected, write,
};
use crate::support::{GhStep, Session};
use serde_json::{json, Value};

const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const COMMENT: &str = "ship";
const PENDING: &str = "repos/example/repo/actions/runs/123/pending_deployments";

/// Run 123 attempt 2 waits on a deployment gate after its test job failed.
fn waiting_run() -> Value {
    let mut run = raw_run();
    run["status"] = json!("waiting");
    run["conclusion"] = Value::Null;
    run
}
fn jobs() -> GhStep {
    let job = |id: u64, name: &str, conclusion: &str, steps: Value| {
        json!({"id":id,"run_id":123,"run_attempt":2,"head_sha":SHA,"name":name,"status":"completed",
            "conclusion":conclusion,"runner_name":"hosted","labels":["ubuntu-latest"],
            "check_run_url":format!("https://github.invalid/api/v3/repos/example/repo/check-runs/{id}"),
            "steps":steps})
    };
    read(
        "repos/example/repo/actions/runs/123/attempts/2/jobs?per_page=100",
        json!([{"jobs":[
            job(500, "build", "success", json!([{"number":1,"name":"Compile","status":"completed","conclusion":"success"}])),
            job(501, "matrix test linux", "failure", json!([
                {"number":1,"name":"Set up job","status":"completed","conclusion":"success"},
                {"number":2,"name":"Build","status":"completed","conclusion":"success"},
                {"number":3,"name":"Run tests","status":"completed","conclusion":"failure"}
            ]))
        ]}]),
        true,
    )
}
fn open_run() -> [GhStep; 2] {
    [
        read(
            "repos/example/repo/actions/runs/123/attempts/2",
            waiting_run(),
            false,
        ),
        jobs(),
    ]
}
/// The failed job's log: three steps, the runner's error line inside `Run tests`.
fn log() -> GhStep {
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
        "matrix test linux\tSet up job\t2026-10-13T12:00:01.0000000Z Runner image ubuntu-24.04\n\
matrix test linux\tBuild\t2026-10-13T12:00:20.0000000Z Finished release profile\n\
matrix test linux\tRun tests\t2026-10-13T12:00:48.0000000Z running 12 tests\n\
matrix test linux\tRun tests\t2026-10-13T12:01:30.0000000Z test checkout::keeps_head ... FAILED\n\
matrix test linux\tRun tests\t2026-10-13T12:03:17.0000000Z ##[error]Process completed with exit code 101.\n",
    )
}
fn artifacts(listed: &Value) -> GhStep {
    read(
        "repos/example/repo/actions/runs/123/artifacts?per_page=100",
        json!([{ "artifacts": [listed] }]),
        true,
    )
}
fn gate() -> Value {
    json!([{"environment":{"id":9,"name":"production"},"current_user_can_approve":true,"wait_timer":30,
        "reviewers":[{"type":"User","reviewer":{"id":42,"login":"octocat","type":"User"}}]}])
}
fn gates(history: Value) -> [GhStep; 2] {
    [
        read(PENDING, gate(), false),
        read(
            "repos/example/repo/actions/runs/123/approvals",
            history,
            false,
        ),
    ]
}
fn approved_history() -> Value {
    json!([{"state":"approved","comment":COMMENT,"environments":[{"id":9,"name":"production"}],
        "user":{"id":42,"login":"octocat","type":"User"}}])
}
/// Open the Approve form, type the explanation, Preview, then Confirm. After an acknowledged
/// original attempt the form first restores that ORIGINAL reference; F9 starts a new intent.
fn approve(session: &mut Session, mouse: bool, restored: bool) {
    command(session, mouse, "Approve selected deployment");
    if restored {
        session.wait_for_screen("ORIGINAL ACTIONS ATTEMPT #1");
        session.wait_for_screen("Original inspection explicitly acknowledged");
        choose(session, mouse, "[F9 New intent]", b"\x1b[20~");
        // The original-reference frame also says ACTIONS INTENT; wait for the new draft.
        session
            .try_wait_for_condition(
                |screen| {
                    screen.contains("ACTIONS INTENT")
                        && screen.contains("[Confirm action]")
                        && !screen.contains("[View original]")
                        && !screen.contains("ORIGINAL ACTIONS ATTEMPT #1")
                },
                "new local approval intent",
                std::time::Duration::from_secs(8),
            )
            .unwrap_or_else(|error| {
                session.save_artifacts();
                panic!("{error}; artifacts: {}", session.artifact_dir.display());
            });
    }
    session.wait_for_screen("ACTIONS INTENT");
    focus_field(session, mouse, "Decision explanation", 1);
    // A draft kept from an earlier attempt is replaced, never appended to.
    session.send_key(b"\x15");
    session.paste(COMMENT);
    choose(session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    session.wait_for_screen("production");
    if mouse {
        session.click_visible_action("[Confirm action]");
    } else {
        session.focus_visible_form_action("[Confirm action]");
        session.send_key(b"\r");
    }
}
fn next_failure(session: &mut Session, mouse: bool) {
    if mouse {
        button(session, "[> Next failure]");
    } else {
        session.send_key(b">");
    }
}
fn back(session: &mut Session, mouse: bool) {
    choose(session, mouse, "[b Back]", b"b");
}
/// Decodes one journal row's stdin, which the fake gh records as hex.
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
fn writes(rows: &[Value]) -> Vec<&Value> {
    rows.iter()
        .filter(|row| {
            matches!(
                row["args"][5].as_str(),
                Some("POST" | "PUT" | "PATCH" | "DELETE")
            )
        })
        .collect()
}

#[test]
fn complete_journeys_v65_keyboard_actions_failed_step_log_artifact_export_offline_approval() {
    journey(false)
}
#[test]
fn complete_journeys_v65_mouse_actions_failed_step_log_artifact_export_offline_approval() {
    journey(true)
}

fn journey(mouse: bool) {
    let fixture = fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let report = b"cross-feature artifact report\nfailing test: checkout::keeps_head\n";
    let zip = artifact_zip(&[("report.txt", report)]);
    let listed = artifact_value(41, "build-output", &zip, false);

    let mut steps = initial();
    steps[2] = read(
        "repos/example/repo/actions/runs?per_page=100",
        json!({"total_count":1,"workflow_runs":[waiting_run()]}),
        false,
    );
    steps.extend(open_run());
    let opened = steps.len();
    steps.push(log());
    let logged = steps.len();
    // Back to the run re-reads the pinned attempt.
    steps.extend(open_run());
    steps.push(artifacts(&listed));
    steps.extend([
        read(
            "repos/example/repo/actions/artifacts/41",
            listed.clone(),
            false,
        ),
        artifact_download(41, zip.clone()),
    ]);
    let downloaded = steps.len();
    // The explicit export: OPEN (2 principals), pre-effect, filesystem effect, final principal.
    steps.extend(compose_identity_steps());
    steps.extend(current_account());
    steps.extend(current_account());
    let exported = steps.len();
    // Back to the artifact list, back to the run, then its deployment gates.
    steps.push(artifacts(&listed));
    steps.extend(open_run());
    steps.extend(gates(json!([])));
    let gated = steps.len();
    // Approve: OPEN, confirm-time principal, then the pre-write run guard is offline.
    steps.extend(compose_identity_steps());
    steps.extend(current_account());
    let mut offline = run_guard(waiting_run());
    offline[2].stdout.clear();
    offline[2].stderr = b"error connecting to github.invalid: network is unreachable\n".to_vec();
    offline[2].status = 1;
    steps.extend(offline);
    steps.extend(current_account());
    let unknown = steps.len();
    // Inspect the exact original attempt: restore it, then bracket run/gates/history reads.
    steps.extend(compose_identity_steps());
    steps.extend(current_account());
    steps.push(read(
        "repos/example/repo/actions/runs/123",
        waiting_run(),
        false,
    ));
    steps.extend(gates(json!([])));
    steps.push(read(
        "repos/example/repo/actions/runs/123",
        waiting_run(),
        false,
    ));
    steps.extend(current_account());
    let inspected = steps.len();
    // Acknowledge releases the barrier locally and refreshes the catalogue.
    steps.extend(current_account());
    steps.push(steps[2].clone());
    steps.extend(current_account());
    let acknowledged = steps.len();
    // Back to Runs, reopen the run and its gates.
    steps.push(steps[2].clone());
    steps.extend(open_run());
    steps.extend(gates(json!([])));
    let regated = steps.len();
    // Reopening restores the acknowledged ORIGINAL reference before its fields.
    steps.extend(current_account());
    steps.extend(compose_identity_steps());
    steps.extend(current_account());
    steps.extend(run_guard(waiting_run()));
    steps.extend([
        read(PENDING, gate(), false),
        write(
            PENDING,
            Some(json!({"environment_ids":[9],"state":"approved","comment":COMMENT})),
            json!([{"id":109,"environment":"production","sha":SHA,
                "creator":{"id":42,"login":"octocat","type":"User"}}]),
        ),
        read(PENDING, json!([]), false),
        read("repos/example/repo/actions/runs/123", waiting_run(), false),
        read(
            "repos/example/repo/actions/runs/123/approvals",
            approved_history(),
            false,
        ),
    ]);
    steps.extend(current_account());
    let approved = steps.len();
    fixture.scripted_gh(&steps);

    let mut session = Session::launch(&fixture, 140, 40);
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "Actions", b"4");
    session.wait_for_screen(&run_cell(123, 2));
    if mouse {
        session.click_visible_action(&run_cell(123, 2));
    } else {
        session.send_key(b"\r");
    }
    // The run opens on its failed job; opening it lists the steps.
    wait_row(&mut session, &["matrix test linux", "501"]);
    wait_selected(&mut session, "matrix test linux");
    journal(&mut session, &fixture, opened);
    if mouse {
        session.click_visible_action("matrix test linux");
    } else {
        session.send_key(b"\r");
    }
    wait_row(&mut session, &["3 Run tests", "failure"]);
    wait_selected(&mut session, "matrix test linux");
    // Find the failing step, then open its job log from that step row.
    next_failure(&mut session, mouse);
    wait_selected(&mut session, "3 Run tests");
    if mouse {
        session.click_visible_action("3 Run tests");
    } else {
        session.send_key(b"\r");
    }
    session.wait_for_screen("Runner image ubuntu-24.04");
    next_failure(&mut session, mouse);
    wait_selected(
        &mut session,
        "##[error]Process completed with exit code 101.",
    );
    let screen = session.screen();
    let line_of = |text: &str| {
        screen
            .lines()
            .position(|line| line.contains(text))
            .unwrap_or_else(|| panic!("{text:?} not painted:\n{screen}"))
    };
    let failing_step = line_of("── Run tests ──");
    assert!(line_of("── Build ──") < failing_step);
    assert!(failing_step < line_of("test checkout::keeps_head ... FAILED"));
    assert!(failing_step < line_of("##[error]Process completed"));
    assert!(!screen.contains("2026-10-13T12:03:17"), "{screen}");
    let rows = journal(&mut session, &fixture, logged);
    assert!(rows[logged - 1]["args"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| arg == "501"));
    // The narrow layout keeps the found error line selected; the wide one comes back.
    session.resize(80, 24);
    wait_selected(&mut session, "exit code 101");
    session.resize(140, 40);
    wait_selected(&mut session, "exit code 101");

    back(&mut session, mouse);
    wait_row(&mut session, &["build", "success", "500"]);
    wait_selected(&mut session, "matrix test linux");
    choose(&mut session, mouse, "[a Artifacts]", b"a");
    session.wait_for_screen("build-output #41");
    session.wait_for_screen("attempt not exposed");
    if mouse {
        session.click_visible_action("build-output #41");
    } else {
        session.send_key(b"\r");
    }
    session.wait_for_screen("report.txt");
    session.wait_for_screen("SHA256 digest verified");
    journal(&mut session, &fixture, downloaded);

    // Download to a temporary directory: Preview, then an explicit Confirm.
    let downloads = fixture.root.join("downloads");
    std::fs::create_dir(&downloads).unwrap();
    let export = downloads.join("build-output.zip");
    command(&mut session, mouse, "Export artifact ZIP");
    session.wait_for_screen("ACTIONS INTENT");
    session.paste(export.to_str().unwrap());
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    assert!(!export.exists(), "Preview must not write the archive");
    confirm(&mut session, mouse, 1);
    session.wait_for_screen("without overwrite or execution");
    journal(&mut session, &fixture, exported);
    assert_eq!(std::fs::read(&export).unwrap(), zip);
    assert_eq!(
        std::fs::read_dir(&downloads).unwrap().count(),
        1,
        "only the chosen archive is written"
    );

    // The artifact page has no attempt cell and lists report.txt; the list is the reverse.
    back(&mut session, mouse);
    session
        .try_wait_for_condition(
            |screen| screen.contains("attempt not exposed") && !screen.contains("report.txt"),
            "artifact list after Back",
            std::time::Duration::from_secs(8),
        )
        .unwrap();
    journal(&mut session, &fixture, exported + 1);
    back(&mut session, mouse);
    wait_row(&mut session, &["matrix test linux", "501"]);
    choose(&mut session, mouse, "[G Gates]", b"G");
    session.wait_for_screen("Gate #9 • production");
    journal(&mut session, &fixture, gated);

    // Offline during the approval: nothing is sent and the original attempt is kept.
    approve(&mut session, mouse, false);
    session.wait_for_screen("error connecting to github.invalid");
    let rows = journal(&mut session, &fixture, unknown);
    assert!(writes(&rows).is_empty(), "the offline guard sent no write");
    close_form(&mut session, mouse);
    session.wait_for_screen("Gate #9 • production");
    let durable =
        publication_support::durable_rows(&fixture.root.join("config/ghtui/cache.sqlite3"));
    assert_eq!(durable.len(), 1);
    assert_eq!(durable[0].state, "uncertain");
    assert_eq!(durable[0].kind, json!("approve_deployment"));
    assert_eq!(
        durable[0].target["DeploymentReview"]["environment_ids"],
        json!([9])
    );
    assert!(durable[0].receipt.is_none());
    // The kept attempt blocks a second approval until it is inspected and acknowledged.
    command(&mut session, mouse, "Approve selected deployment");
    session.wait_for_screen("Action unavailable");
    session.wait_for_screen("Original attempt #1");
    assert!(!session.screen().contains("ACTIONS INTENT"));
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session
        .try_wait_for_condition(
            |screen| !screen.contains("Action unavailable"),
            "approval explanation closed",
            std::time::Duration::from_secs(8),
        )
        .unwrap();
    journal(&mut session, &fixture, unknown);
    inspect_original_publication(&mut session, mouse, 1);
    let rows = journal(&mut session, &fixture, inspected);
    assert!(writes(&rows).is_empty(), "inspection never resends");
    command(&mut session, mouse, "Acknowledge inspected state");
    session.wait_for_screen("Original affected state acknowledged");
    let rows = journal(&mut session, &fixture, acknowledged);
    assert!(writes(&rows).is_empty(), "acknowledgement never resends");

    // A fresh, explicitly confirmed approval sends exactly one review request.
    back(&mut session, mouse);
    let run_row = run_cell(123, 2);
    wait_row(&mut session, &["CI matrix", run_row.as_str()]);
    journal(&mut session, &fixture, acknowledged + 1);
    if mouse {
        session.click_visible_action(&run_cell(123, 2));
    } else {
        session.send_key(b"\r");
    }
    wait_row(&mut session, &["matrix test linux", "501"]);
    choose(&mut session, mouse, "[G Gates]", b"G");
    session.wait_for_screen("Gate #9 • production");
    journal(&mut session, &fixture, regated);
    approve(&mut session, mouse, true);
    session.wait_for_screen("Decision approved accepted");
    session.wait_for_screen(&format!("Decision explanation: {COMMENT}"));
    let rows = journal(&mut session, &fixture, approved);
    let sent = writes(&rows);
    assert_eq!(sent.len(), 1, "exactly one approval request");
    assert_eq!(sent[0]["args"][1], PENDING);
    assert_eq!(sent[0]["args"][5], "POST");
    assert_eq!(
        stdin_json(sent[0]),
        json!({"environment_ids":[9],"state":"approved","comment":COMMENT})
    );
    let durable =
        publication_support::durable_rows(&fixture.root.join("config/ghtui/cache.sqlite3"));
    assert_eq!(durable.len(), 2);
    assert_eq!(durable[0].state, "acknowledged");
    assert!(durable[0].receipt.is_none());
    assert_eq!(durable[1].state, "accepted_pending");
    assert_eq!(durable[1].kind, json!("approve_deployment"));
    assert_eq!(
        durable[1].receipt.as_ref().unwrap()["kind"],
        "deployment_accepted"
    );
    assert!(durable.iter().all(|row| row.context["run_id"] == 123
        && row.context["attempt"] == 2
        && row.context["head_sha"] == SHA));

    session.resize(80, 24);
    session.wait_for_screen(&format!("Decision explanation: {COMMENT}"));
    assert_eq!(std::fs::read(&export).unwrap(), zip);
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    session.quit_and_assert_restored_terminal(mouse);
}
