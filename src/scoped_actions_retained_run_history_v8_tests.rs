//! SOURCE ONLY/UNRUN: actual PR source -> SAME Confirm -> retained history.
use super::*;
use crate::{
    actions_write_journal::{ActionsOwnedRunHistoryV8, ActionsWriteJournal},
    forms::FormAction,
    scoped_actions_native_v1::{self as native, publication_v3 as publication},
    scoped_actions_publication_test_support_v3 as w,
    scoped_actions_publication_ui_v3::{self as ui, ComposerPhaseV3},
};
#[path = "scoped_actions_retained_run_history_cas_v8_tests.rs"]
mod cas;
fn model(a: u64, status: &str, conclusion: &str) -> Value {
    let mut v = actual_run(REPO);
    v["run_attempt"] = json!(a);
    v["status"] = json!(status);
    v["conclusion"] = if conclusion.is_empty() {
        Value::Null
    } else {
        json!(conclusion)
    };
    v
}
fn with_run(mut steps: Vec<GhStep>, v: &Value) -> Vec<GhStep> {
    for s in &mut steps {
        if s.args.get(1).is_some_and(|s| {
            s.ends_with("/actions/runs/123") || s.ends_with("/actions/runs/123/attempts/2")
        }) {
            s.stdout = v.to_string().into_bytes();
        }
    }
    steps
}
fn history(v: Option<Value>, uid: u64) -> Vec<GhStep> {
    let mut out = principal(42, "history-start");
    out.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    out.push(get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)));
    for _ in 0..2 {
        let mut s = get(
            &format!("repos/{REPO}/actions/runs/123"),
            v.clone().unwrap_or(Value::Null),
        );
        if v.is_none() {
            s.status = 1;
            s.stdout.clear();
            s.stderr = b"gh: Not Found (HTTP 404)".to_vec();
        }
        out.push(s);
    }
    out.push(get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)));
    out.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    out.extend(principal(uid, "history-end"));
    assert_eq!(out.len(), 10);
    out
}
fn confirm(
    command: ActionsCommand,
    before: &Value,
    receipt: Option<Value>,
    uid: u64,
) -> Vec<GhStep> {
    let mut out = principal(42, "open-current");
    out.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    out.push(get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)));
    out.push(get(
        &format!("repos/{REPO}/check-runs/600"),
        actual_check(REPO),
    ));
    out.push(get(
        &format!("repos/{REPO}/actions/runs/123"),
        before.clone(),
    ));
    // The shared mutation receives the genuine Preview Plan.identity, whose
    // opening principal is open-current. No Confirm-only label can replace it.
    out.extend(principal(42, "open-current"));
    out.push(get(
        &format!("repos/{REPO}/actions/runs/123"),
        before.clone(),
    ));
    if command == ActionsCommand::RerunJob {
        let mut job = actual_job(REPO);
        job["run_attempt"] = json!(2);
        out.push(get(&format!("repos/{REPO}/actions/jobs/501"), job.clone()));
        // Same production ExactAttemptJobs reader and positive page membership,
        // not direct job labels/URLs promoted into an attempt proof.
        out.push(get(
            &format!("repos/{REPO}/actions/runs/123/attempts/2"),
            before.clone(),
        ));
        out.push(get(
            &format!("repos/{REPO}/actions/runs/123/attempts/2/jobs?per_page=100&page=1"),
            json!({"total_count":1,"jobs":[job]}),
        ));
    }
    out.push(get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)));
    out.push(get(
        &format!("repos/{REPO}/check-runs/600"),
        actual_check(REPO),
    ));
    out.push(get(
        &format!("repos/{REPO}/actions/runs/123"),
        before.clone(),
    ));
    out.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    out.extend(principal(42, "open-current"));
    let path = match command {
        ActionsCommand::RerunAll => "/rerun",
        ActionsCommand::RerunFailed => "/rerun-failed-jobs",
        ActionsCommand::RerunJob => "/rerun",
        ActionsCommand::Cancel => "/cancel",
        ActionsCommand::ForceCancel => "/force-cancel",
        ActionsCommand::DeleteRun => "",
        ActionsCommand::DeleteLogs => "/logs",
        _ => panic!("closed Run kind"),
    };
    let endpoint = if command == ActionsCommand::RerunJob {
        format!("repos/{REPO}/actions/jobs/501{path}")
    } else {
        format!("repos/{REPO}/actions/runs/123{path}")
    };
    let delete = matches!(
        command,
        ActionsCommand::DeleteRun | ActionsCommand::DeleteLogs
    );
    let rerun = matches!(
        command,
        ActionsCommand::RerunAll | ActionsCommand::RerunFailed | ActionsCommand::RerunJob
    );
    let mut args: Vec<String> = vec![
        "api".into(),
        endpoint,
        "--hostname".into(),
        HOST.into(),
        "--method".into(),
        if delete {
            "DELETE".into()
        } else {
            "POST".into()
        },
    ];
    if rerun {
        args.extend(["--input".into(), "-".into()]);
    }
    if delete || command == ActionsCommand::ForceCancel {
        args.push("--include".into());
    }
    let mut post = GhStep::read(
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
        if delete {
            "HTTP/2.0 204 No Content\nContent-Length: 0\r\n\r\n"
        } else if command == ActionsCommand::ForceCancel {
            "HTTP/2.0 202 Accepted\nContent-Length: 0\r\n\r\n"
        } else {
            "{}"
        },
    );
    if rerun {
        post.stdin = serde_json::to_vec(&json!({"enable_debug_logging":false})).unwrap();
    }
    out.push(post);
    let missing = receipt.is_none();
    let mut s = get(
        &format!("repos/{REPO}/actions/runs/123"),
        receipt.unwrap_or(Value::Null),
    );
    if missing {
        s.status = 1;
        s.stdout.clear();
        s.stderr = b"gh: Not Found (HTTP 404)".to_vec();
    }
    out.push(s);
    if command == ActionsCommand::DeleteRun {
        out.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    }
    // Final prepaid identity closes the SAME genuine Confirm principal. It
    // cannot silently adopt a different fixture phase label after publication.
    out.push(get("user", json!({"id":uid,"login":"open-current"})));
    out
}
fn exact(f: &Fixture, steps: &[GhStep]) {
    let text = std::fs::read_to_string(f.root.join("gh-fixture/commands.jsonl")).unwrap();
    let rows: Vec<Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(rows.len(), steps.len());
    for (r, s) in rows.iter().zip(steps) {
        assert_eq!(r["matched"], true);
        assert_eq!(r["args"], json!(s.args));
        assert_eq!(
            r["stdin_hex"],
            s.stdin
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
    }
}
fn accepted(
    command: ActionsCommand,
    tail: Vec<GhStep>,
) -> (
    Fixture,
    GithubProvider,
    App,
    Vec<GhStep>,
    crate::drafts::DraftWriter,
) {
    accepted_with_guard(command, tail, None)
}
fn accepted_with_guard(
    command: ActionsCommand,
    tail: Vec<GhStep>,
    guard_login: Option<&str>,
) -> (
    Fixture,
    GithubProvider,
    App,
    Vec<GhStep>,
    crate::drafts::DraftWriter,
) {
    let running = matches!(
        command,
        ActionsCommand::Cancel
            | ActionsCommand::ForceCancel
            | ActionsCommand::DeleteRun
            | ActionsCommand::DeleteLogs
    );
    let initial = if running {
        model(2, "in_progress", "")
    } else {
        model(2, "completed", "failure")
    };
    let receipt = if !running {
        Some(model(3, "queued", ""))
    } else if command == ActionsCommand::DeleteRun {
        None
    } else {
        Some(initial.clone())
    };
    let mut steps = with_run(whole(&bridge_steps(REPO)), &initial);
    steps.extend(with_run(phase(), &initial));
    steps.extend(with_run(phase(), &initial));
    let mut confirmation = confirm(command, &initial, receipt, 42);
    if let Some(login) = guard_login {
        confirmation[7] = get("user", json!({"id":42,"login":login}));
        confirmation.truncate(8);
    }
    steps.extend(confirmation);
    steps.extend(tail);
    let f = Fixture::new();
    f.scripted_gh(&steps);
    let p = provider(&f);
    let mut app = app_checked(&f);
    let value = load_bridge_using(&f, &mut app, &Default::default(), &p);
    install(&mut app, value);
    app.dispatch(Action::Close);
    assert!(
        app.pr_check_continuation_cancel_v2
            && app.pr_check_continuation_request_v2.is_none()
            && app.pr_check_continuation_owner_v2.is_none()
            && app.pr_check_watch_v2.is_none()
    );
    app.pr_check_continuation_cancel_v2 = false;
    crate::scoped_actions_rerun_test_support_v4::open(&mut app, &p, &f, command);
    if command == ActionsCommand::ForceCancel {
        app.form
            .as_mut()
            .unwrap()
            .dispatch(&FormAction::Choice(0, 1));
        w::paint(&mut app, 120, 40);
    }
    w::preview(&mut app, &p, &f);
    assert_eq!(crate::scoped_actions_rerun_test_support_v4::preview(&app).plan().identity.login,
        "open-current","Actual Preview Plan identity, not a Confirm phase label, is passed to the same mutation guard");
    let work = w::request(&mut app, FormAction::Submit);
    let writer = crate::drafts::DraftWriter::new(database(&f));
    let owner =
        native::ScopedActionsOwnerV1::new_publication_v3(work, &database(&f), &writer.client())
            .unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let value = publication::execute(&owner, &p, &database(&f), Some(&writer.client())).unwrap();
    assert!(publication::publish(&mut app, &owner, value));
    app.scoped_actions_owner_v1 = None;
    w::paint(&mut app, 120, 40);
    (f, p, app, steps, writer)
}
#[test]
fn scoped_actions_retained_history_v8_actual_preview_login_cannot_be_replaced_by_confirm_label() {
    let (f, _p, app, all, _writer) = accepted_with_guard(
        ActionsCommand::RerunAll,
        vec![],
        Some("different-confirm-phase-label"),
    );
    assert!(
        app.form.is_none(),
        "Failed original prewrite identity closes only its own private presentation"
    );
    assert_eq!(w::result_sql(&f), vec![("pending".into(), true, None)]);
    assert!(!all
        .iter()
        .any(|step| step.args.iter().any(|arg| arg == "POST")));
    exact(&f, &all);
}
fn proof(app: &App) -> Arc<ActionsOwnedRunHistoryV8> {
    ui::capture(app.form.as_ref().unwrap(), ComposerPhaseV3::Inspect)
        .unwrap()
        .owned_history_v8()
        .unwrap()
        .clone()
}
fn read_work(app: &mut App, action: FormAction) -> Arc<native::ScopedActionsOwnerV1> {
    let owner = native::ScopedActionsOwnerV1::new(w::request(app, action)).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    owner
}
fn read(app: &mut App, p: &GithubProvider, f: &Fixture, action: FormAction) -> Result<(), String> {
    let owner = read_work(app, action);
    let v = publication::execute(&owner, p, &database(f), None)?;
    assert!(publication::publish(app, &owner, v));
    app.scoped_actions_owner_v1 = None;
    w::paint(app, 120, 40);
    Ok(())
}
fn status(f: &Fixture) -> String {
    rusqlite::Connection::open(database(f))
        .unwrap()
        .query_row(
            "SELECT state FROM actions_write_attempts ORDER BY attempt_id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap()
}

#[test]
fn scoped_actions_retained_history_v8_actual_acceptance_four_complete_controls_geometry_focus_and_meaning(
) {
    use crossterm::event::{KeyCode, KeyModifiers};
    let (f, p, mut app, all, _drafts) = accepted(
        ActionsCommand::DeleteLogs,
        history(Some(model(2, "completed", "cancelled")), 42),
    );
    read(&mut app, &p, &f, FormAction::Submit).unwrap();
    let sql = sql_state(&f);
    let git = git_state_resource(&f);
    for (width, height) in [(80, 20), (80, 24), (120, 40)] {
        for errors in [false, true] {
            let form = app.form.as_mut().unwrap();
            form.error = if errors {
                "Visible original error; protection retained".into()
            } else {
                String::new()
            };
            form.focus = form.fields.len();
            form.preview_scroll = usize::MAX;
            app.hits.clear();
            let rendered = w::paint(&mut app, width, height);
            for label in [
                "[View original]",
                "[F4 Acknowledge]",
                "[F6 ACK receipt]",
                "[Keep draft / Close]",
            ] {
                assert!(
                    rendered.contains(label),
                    "Every whole control must actually paint at{width}x{height}"
                );
            }
            let actions = [
                FormAction::Submit,
                FormAction::AcknowledgeActionsV3,
                FormAction::AcknowledgeActionsAcceptanceV5,
                FormAction::Close,
            ];
            let mut rects = Vec::new();
            for action in actions {
                let matching: Vec<_> = app
                    .hits
                    .iter()
                    .filter(|hit| hit.action == Action::Form(action.clone()))
                    .collect();
                assert_eq!(matching.len(), 1);
                let rect = matching[0].rect;
                assert!(!rect.is_empty() && rect.right() <= width && rect.bottom() <= height);
                for old in &rects {
                    assert!(rect.intersection(*old).is_empty());
                }
                rects.push(rect);
            }
            let form = app.form.as_mut().unwrap();
            let first = form.fields.len();
            let count = if errors { 6 } else { 4 };
            for step in 1..=count * 2 {
                assert!(form.key(KeyCode::Tab, KeyModifiers::empty()).is_none());
                assert_eq!(form.focus, first + step % count);
            }
            for step in 1..=count * 2 {
                assert!(form.key(KeyCode::BackTab, KeyModifiers::SHIFT).is_none());
                assert_eq!(form.focus, first + (count - step % count) % count);
            }
            // Closed typed history, not button wording, retains the full
            // explanation and cannot promote effect/deletion into confirmation.
            assert!(form
                .plan_lines()
                .iter()
                .any(|line| line.contains("EFFECT UNVERIFIED")
                    && line.contains("F4 does not confirm deletion")
                    && line.contains("acceptance")));
        }
    }
    app.hits.clear();
    let tiny = w::paint(&mut app, 40, 10);
    assert!(tiny.contains("[Esc Close]") && !tiny.contains("[F6 ACK receipt]"));
    assert_eq!(sql_state(&f), sql);
    assert_eq!(git_state_resource(&f), git);
    exact(&f, &all);
}
fn happy(command: ActionsCommand, value: Option<Value>, acceptance: bool) {
    let mut tail = history(value.clone(), 42);
    tail.extend(history(value.clone(), 42));
    tail.extend(history(value, 42));
    let (f, p, mut app, steps, _writer) = accepted(command, tail);
    let git = git_state_resource(&f);
    let sql = sql_state(&f);
    let old = proof(&app);
    assert!(app
        .scoped_actions_v1
        .as_ref()
        .unwrap()
        .source_for_test_v107()
        .pr_origin_v2()
        .unwrap()
        .validate_local(&database(&f))
        .is_err());
    read(&mut app, &p, &f, FormAction::Submit).unwrap();
    read(
        &mut app,
        &p,
        &f,
        if acceptance {
            FormAction::AcknowledgeActionsAcceptanceV5
        } else {
            FormAction::AcknowledgeActionsV3
        },
    )
    .unwrap();
    let new = proof(&app);
    assert!(!Arc::ptr_eq(&old, &new));
    let j = ActionsWriteJournal::open(&database(&f)).unwrap();
    assert!(j.validate_owned_run_history_v8(&old).is_err());
    j.validate_owned_run_history_v8(&new).unwrap();
    assert_eq!(
        status(&f),
        if acceptance {
            "acknowledged"
        } else {
            "transition_observed"
        }
    );
    read(&mut app, &p, &f, FormAction::Submit).unwrap();
    assert_eq!(git_state_resource(&f), git);
    assert_eq!(sql_state(&f), sql);
    exact(&f, &steps);
}
#[test]
fn scoped_actions_retained_history_v8_own_pr_cancel_inspect_ack_then_inspect() {
    happy(
        ActionsCommand::Cancel,
        Some(model(2, "completed", "cancelled")),
        false,
    );
}
#[test]
fn scoped_actions_retained_history_v8_own_pr_rerun_all_a_advance_observation_only() {
    happy(
        ActionsCommand::RerunAll,
        Some(model(3, "queued", "")),
        false,
    );
}
#[test]
fn scoped_actions_retained_history_v8_own_pr_rerun_failed_a_advance() {
    happy(
        ActionsCommand::RerunFailed,
        Some(model(3, "queued", "")),
        false,
    );
}
#[test]
fn scoped_actions_retained_history_v8_own_pr_rerun_job_actual_member_a_advance() {
    happy(
        ActionsCommand::RerunJob,
        Some(model(3, "queued", "")),
        false,
    );
}
#[test]
fn scoped_actions_retained_history_v8_own_pr_force_observed_transition() {
    happy(
        ActionsCommand::ForceCancel,
        Some(model(2, "completed", "cancelled")),
        false,
    );
}
#[test]
fn scoped_actions_retained_history_v8_own_pr_other_completion_no_causality() {
    happy(
        ActionsCommand::Cancel,
        Some(model(2, "completed", "success")),
        false,
    );
}
#[test]
fn scoped_actions_retained_history_v8_own_pr_deleted404_acceptance_only() {
    happy(ActionsCommand::DeleteRun, None, true);
}
#[test]
fn scoped_actions_retained_history_v8_own_pr_logs_acceptance_only() {
    happy(
        ActionsCommand::DeleteLogs,
        Some(model(2, "in_progress", "")),
        true,
    );
}
#[test]
fn scoped_actions_retained_history_v8_active_cancel_no_ack() {
    let (f, p, mut app, s, _w) = accepted(
        ActionsCommand::Cancel,
        history(Some(model(2, "in_progress", "")), 42),
    );
    read(&mut app, &p, &f, FormAction::Submit).unwrap();
    assert!(!ui::acknowledgement_ready(app.form.as_ref().unwrap()));
    assert_eq!(status(&f), "accepted_pending");
    exact(&f, &s);
}
#[test]
fn scoped_actions_retained_history_v8_logs_never_normal_ack() {
    let (f, p, mut app, s, _w) = accepted(
        ActionsCommand::DeleteLogs,
        history(Some(model(2, "in_progress", "")), 42),
    );
    read(&mut app, &p, &f, FormAction::Submit).unwrap();
    assert!(!ui::acknowledgement_ready(app.form.as_ref().unwrap()));
    assert!(ui::acceptance_ready_v5(app.form.as_ref().unwrap()));
    exact(&f, &s);
}
fn raw_drift(column: &str) {
    let (f, _p, app, s, _w) = accepted(ActionsCommand::RerunAll, vec![]);
    let proof = proof(&app);
    assert_eq!(
        rusqlite::Connection::open(database(&f))
            .unwrap()
            .execute(
                &format!("UPDATE actions_write_attempts SET {column}={column}||' '"),
                []
            )
            .unwrap(),
        1
    );
    assert!(ActionsWriteJournal::open(&database(&f))
        .unwrap()
        .validate_owned_run_history_v8(&proof)
        .is_err());
    assert_eq!(status(&f), "accepted_pending");
    exact(&f, &s);
}
#[test]
fn scoped_actions_retained_history_v8_raw_receipt_not_decoded_equality() {
    raw_drift("receipt_json");
}
#[test]
fn scoped_actions_retained_history_v8_raw_context_not_decoded_equality() {
    raw_drift("context_json");
}
#[test]
fn scoped_actions_retained_history_v8_raw_options_not_decoded_equality() {
    raw_drift("options_json");
}
#[test]
fn scoped_actions_retained_history_v8_raw_resources_not_decoded_equality() {
    raw_drift("resources_json");
}
#[test]
fn scoped_actions_retained_history_v8_raw_target_not_decoded_equality() {
    raw_drift("target_json");
}
fn scalar_drift(sql: &str) {
    let (f, _p, app, steps, _writer) = accepted(ActionsCommand::RerunAll, vec![]);
    let actual = proof(&app);
    let db = rusqlite::Connection::open(database(&f)).unwrap();
    assert_eq!(db.execute(sql, []).unwrap(), 1);
    let journal = ActionsWriteJournal::open(&database(&f)).unwrap();
    assert!(journal.validate_owned_run_history_v8(&actual).is_err());
    assert_eq!(status(&f), "accepted_pending");
    exact(&f, &steps);
}
#[test]
fn scoped_actions_retained_history_v8_claimed_flag_not_reconstructed() {
    scalar_drift("UPDATE actions_write_attempts SET claimed=0");
}
#[test]
fn scoped_actions_retained_history_v8_created_at_is_exact_original_row() {
    scalar_drift("UPDATE actions_write_attempts SET created_at=created_at+1");
}
#[test]
fn scoped_actions_retained_history_v8_alias_generation_drift() {
    let (f, _p, app, s, _w) = accepted(ActionsCommand::RerunAll, vec![]);
    let proof = proof(&app);
    assert!(
        rusqlite::Connection::open(database(&f))
            .unwrap()
            .execute(
                "UPDATE actions_write_generations SET generation=generation+1",
                []
            )
            .unwrap()
            > 0
    );
    assert!(ActionsWriteJournal::open(&database(&f))
        .unwrap()
        .validate_owned_run_history_v8(&proof)
        .is_err());
    exact(&f, &s);
}
#[test]
fn scoped_actions_retained_history_v8_original_pr_checkpoint_never_waived() {
    let (f, _p, app, s, _w) = accepted(ActionsCommand::RerunAll, vec![]);
    let proof = proof(&app);
    let source = app
        .scoped_actions_v1
        .as_ref()
        .unwrap()
        .source_for_test_v107();
    let original = source.original();
    let mut cache = crate::storage::Cache::open(&database(&f)).unwrap();
    assert!(cache
        .sync_checkpoint(original.scope(), original.sync_source())
        .unwrap()
        .is_none());
    let old = cache
        .sync_reservation(original.scope(), original.sync_source())
        .unwrap()
        .expect("Actual original catalogue reservation, not fictional checkpoint");
    let next = cache
        .reserve_sync_generation(original.scope(), original.sync_source())
        .unwrap();
    assert_eq!(next, old + 1);
    assert_eq!(
        cache
            .sync_reservation(original.scope(), original.sync_source())
            .unwrap(),
        Some(next)
    );
    ActionsWriteJournal::open(&database(&f))
        .unwrap()
        .validate_owned_run_history_v8(&proof)
        .unwrap();
    assert!(app
        .scoped_actions_v1
        .as_ref()
        .unwrap()
        .source_for_test_v107()
        .validate_owned_run_history_v8(
            &database(&f),
            &ActionsWriteJournal::open(&database(&f)).unwrap(),
            &proof
        )
        .is_err());
    exact(&f, &s);
}
#[test]
fn scoped_actions_retained_history_v8_two_actual404s_keep_pool_for_original_closure() {
    let (f, p, app, all, _writer) = accepted(ActionsCommand::DeleteRun, history(None, 42));
    let original = proof(&app);
    let source = app
        .scoped_actions_v1
        .as_ref()
        .unwrap()
        .source_for_test_v107()
        .clone();
    let before = (w::result_sql(&f), sql_state(&f), git_state_resource(&f));
    let cancel = crate::providers::runner::CancellationToken::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let phase = p.clone().with_catalogue_read_deadline(deadline);
    let mut session = phase
        .begin_repository_catalogue_access(
            source.original().source(),
            &crate::scoped_actions_publication_v3::access_limits(),
            &cancel,
        )
        .unwrap();
    let window = session
        .reserve_measured_external_reads(6, 512 * 1024, 1024 * 1024, 2, &cancel)
        .unwrap();
    let measured = phase.clone().with_measured_read_window(window).unwrap();
    session.verify(source.reference(), &cancel).unwrap();
    let current = session.observed_original_v1(source.reference().id).unwrap();
    let parent = source
        .pr_origin_v2()
        .unwrap()
        .fresh_history_parent_v5(&measured, current, &cancel)
        .unwrap();
    let journal = ActionsWriteJournal::open(&database(&f)).unwrap();
    let snapshot = journal
        .scoped_history_v5(
            &measured,
            session.identity(),
            current,
            Some(&parent),
            Some(&original.reference()),
        )
        .unwrap();
    let key = snapshot.key().unwrap();
    let first = measured
        .scoped_actions_retained_run_history_v8(current, &key, original.original(), &cancel)
        .unwrap();
    let second = measured
        .scoped_actions_retained_run_history_v8(current, &key, original.original(), &cancel)
        .unwrap();
    assert!(first.run().is_none() && second.run().is_none());
    assert!(first.same_observation(&second));
    assert!(first.availability_label().contains("NOT proof of deletion"));
    let closing = source
        .pr_origin_v2()
        .unwrap()
        .fresh_history_parent_v5(&measured, current, &cancel)
        .unwrap();
    assert!(GithubProvider::same_thread_history_parent_v1(
        &parent, &closing
    ));
    session.verify(source.reference(), &cancel).unwrap();
    let closed = session
        .close_with_guard(&cancel, || {
            source.validate_owned_run_history_v8(&database(&f), &journal, &original)
        })
        .unwrap();
    let accounting = closed.external_read_accounting().unwrap();
    assert_eq!(accounting.children_started(), 4);
    assert_eq!(accounting.successful_children(), 2);
    assert_eq!(accounting.failed_reserved_bytes(), 2 * 256 * 1024);
    assert!(accounting.charged_bytes() < accounting.capacity_bytes());
    assert!(accounting.capacity_bytes() <= 1024 * 1024);
    assert!(accounting.sealed());
    assert_eq!(closed.identity().account_id, Some(42));
    assert_eq!(
        (w::result_sql(&f), sql_state(&f), git_state_resource(&f),),
        before,
        "Read alone must not acknowledge unavailable/deleted effects"
    );
    exact(&f, &all);
}
fn oversized_retained_read_v117(original_cap: usize, padding: usize) {
    let mut value = model(3, "queued", "");
    // Even unused JSON is real captured private output and must count.
    value["unused_private_payload"] = json!("x".repeat(padding));
    let mut tail = history(Some(value), 42);
    tail.truncate(5);
    let (f, p, mut app, all, _writer) = accepted(ActionsCommand::RerunAll, tail);
    let original = proof(&app);
    let before = (w::result_sql(&f), sql_state(&f), git_state_resource(&f));
    let owner = read_work(&mut app, FormAction::Submit);
    let bounded = p
        .with_scoped_read_window(owner.work().deadline(), original_cap)
        .unwrap();
    assert!(publication::execute(&owner, &bounded, &database(&f), None).is_err());
    ActionsWriteJournal::open(&database(&f))
        .unwrap()
        .validate_owned_run_history_v8(&original)
        .unwrap();
    assert_eq!(
        (w::result_sql(&f), sql_state(&f), git_state_resource(&f),),
        before,
        "Oversized source cannot release/ACK/rewrite the original protected row"
    );
    assert_eq!(status(&f), "accepted_pending");
    exact(&f, &all);
}
#[test]
fn scoped_actions_retained_history_v8_raw_over256k_refuses_without_premature_ack() {
    oversized_retained_read_v117(512 * 1024, 256 * 1024 + 1);
}
#[test]
fn scoped_actions_retained_history_v8_original64k_cap_is_never_widened_by_raw256k_bound() {
    oversized_retained_read_v117(64 * 1024, 64 * 1024 + 1);
}
#[test]
fn scoped_actions_retained_history_v8_foreign_actual_database_cannot_adopt() {
    let (f, _p, app, s, _w) = accepted(ActionsCommand::RerunAll, vec![]);
    let proof = proof(&app);
    let (o, _op, oa, os, _ow) = accepted(ActionsCommand::RerunAll, vec![]);
    assert!(oa
        .scoped_actions_v1
        .as_ref()
        .unwrap()
        .source_for_test_v107()
        .validate_owned_run_history_v8(
            &database(&o),
            &ActionsWriteJournal::open(&database(&o)).unwrap(),
            &proof
        )
        .is_err());
    exact(&f, &s);
    exact(&o, &os);
}
fn malformed(v: Value) {
    let mut tail = history(Some(v), 42);
    tail.truncate(5);
    let (f, p, mut app, s, _w) = accepted(ActionsCommand::RerunAll, tail);
    assert!(read(&mut app, &p, &f, FormAction::Submit).is_err());
    assert_eq!(status(&f), "accepted_pending");
    exact(&f, &s);
}
#[test]
fn scoped_actions_retained_history_v8_omitted_actual_attempt_not_default_one() {
    let mut v = model(3, "queued", "");
    v.as_object_mut().unwrap().remove("run_attempt");
    malformed(v);
}
#[test]
fn scoped_actions_retained_history_v8_null_actual_attempt_not_default_one() {
    let mut v = model(3, "queued", "");
    v["run_attempt"] = Value::Null;
    malformed(v);
}
#[test]
fn scoped_actions_retained_history_v8_zero_string_fraction_attempts_refuse() {
    for a in [json!(0), json!("3"), json!(3.5)] {
        let mut v = model(3, "queued", "");
        v["run_attempt"] = a;
        malformed(v);
    }
}
#[test]
fn scoped_actions_retained_history_v8_node_suite_event_head_pins_not_retargeted() {
    for (k, val) in [
        ("node_id", json!("OTHER")),
        ("check_suite_id", json!(999)),
        ("event", json!("push")),
        ("head_sha", json!("b".repeat(40))),
    ] {
        let mut v = model(3, "queued", "");
        v[k] = val;
        malformed(v);
    }
}
#[test]
fn scoped_actions_retained_history_v8_current_parent_drift_refuses() {
    let mut tail = history(Some(model(3, "queued", "")), 42);
    let mut v = resource_parent(REPO);
    v["head"]["sha"] = json!("b".repeat(40));
    tail[6].stdout = v.to_string().into_bytes();
    tail.truncate(7);
    let (f, p, mut app, s, _w) = accepted(ActionsCommand::RerunAll, tail);
    assert!(read(&mut app, &p, &f, FormAction::Submit).is_err());
    exact(&f, &s);
}
#[test]
fn scoped_actions_retained_history_v8_current_numeric_uid_loss() {
    let (f, p, mut app, s, _w) = accepted(
        ActionsCommand::RerunAll,
        history(Some(model(3, "queued", "")), 999),
    );
    assert!(read(&mut app, &p, &f, FormAction::Submit).is_err());
    assert!(!app
        .form
        .as_ref()
        .unwrap()
        .plan_lines()
        .iter()
        .any(|line| line.contains("saved original A/H above remain historical")));
    exact(&f, &s);
}
#[test]
fn scoped_actions_retained_history_v8_newer_overlay_rejects_late_result() {
    let (f, p, mut app, s, _w) = accepted(
        ActionsCommand::RerunAll,
        history(Some(model(3, "queued", "")), 42),
    );
    let owner = read_work(&mut app, FormAction::Submit);
    let v = publication::execute(&owner, &p, &database(&f), None).unwrap();
    app.overlay = Some(crate::app::Overlay::Help);
    let status = app.status.clone();
    let hits = format!("{:?}", app.hits);
    assert!(!publication::publish(&mut app, &owner, v));
    assert_eq!(app.status, status);
    assert_eq!(format!("{:?}", app.hits), hits);
    exact(&f, &s);
}
