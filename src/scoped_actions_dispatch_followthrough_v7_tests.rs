//! Genuine POST200 result enters the SAME canonical read engine. Unrun until
//! Root verification; no literal DTO becomes a Source, receipt or run target.
use super::*;

fn returned_read_phase(
    kind: crate::scoped_actions_v1::ScopedActionsRequestKindV1,
    uid: u64,
) -> Vec<GhStep> {
    fn actual_member(value: &mut Value) {
        match value {
            Value::Array(values) => values.iter_mut().for_each(actual_member),
            Value::Object(fields) => {
                if fields.get("id").and_then(Value::as_u64) == Some(123)
                    && fields.contains_key("workflow_id")
                {
                    *value = returned_run();
                    return;
                }
                if fields.contains_key("head_sha") {
                    fields.insert("head_sha".into(), json!(C));
                }
                if fields.get("run_id").and_then(Value::as_u64) == Some(123) {
                    fields.insert("run_id".into(), json!(124));
                    fields.insert("run_attempt".into(), json!(1));
                }
                if fields.contains_key("check_suite") {
                    fields.insert("check_suite".into(), json!({"id":902}));
                }
                fields.values_mut().for_each(actual_member);
            }
            _ => {}
        }
    }
    let mut steps = f::operation(kind, uid);
    for step in &mut steps {
        for arg in &mut step.args {
            *arg = arg
                .replace("/runs/123", "/runs/124")
                .replace("/attempts/2", "/attempts/1");
        }
        if step.args.first().is_some_and(|arg| arg == "run") {
            assert_eq!(step.args[2], "123");
            step.args[2] = "124".into();
            let index = step.args.iter().position(|arg| arg == "--attempt").unwrap();
            assert_eq!(step.args[index + 1], "2");
            step.args[index + 1] = "1".into();
        } else if let Ok(mut value) = serde_json::from_slice::<Value>(&step.stdout) {
            actual_member(&mut value);
            step.stdout = serde_json::to_vec(&value).unwrap();
        }
    }
    steps
}
fn continue_read(
    app: &mut App,
    provider: &GithubProvider,
    fixture: &Fixture,
    action: ActionsAction,
) {
    assert!(native::intercept(app, &action));
    let work = native::take_work(app).expect("Actual same-owner returned-run continuation");
    install_work(app, provider, fixture, work);
}

#[test]
fn scoped_actions_dispatch_v7_actual_returned_run_jobs_logs_annotations_and_pure_receipt_back() {
    use crate::scoped_actions_v1::ScopedActionsRequestKindV1 as K;
    let protocol = DispatchProtocolV7::Detailed2026Run200;
    let mut tail = confirm_phase(protocol, 42);
    tail.extend(returned_read_phase(K::Attempt, 42));
    tail.extend(returned_read_phase(K::Attempt, 42));
    // This THIRD read is OpenJob(501), not an unselected Run/Jobs page.
    // The shared real reader brackets even an Attempt-only selected member
    // with two exact job GETs; keep both, do not waive its fresh membership.
    let mut selected = returned_read_phase(K::Attempt, 42);
    let jobs = selected
        .iter()
        .position(|step| {
            step.args.iter().any(|arg| {
                arg == &format!(
                    "repos/{}/actions/runs/124/attempts/1/jobs?per_page=100",
                    f::REPO
                )
            })
        })
        .unwrap();
    let mut job = f::job();
    job["run_id"] = json!(124);
    job["run_attempt"] = json!(1);
    job["head_sha"] = json!(C);
    selected.splice(
        jobs + 1..jobs + 1,
        [
            f::get(&format!("repos/{}/actions/jobs/501", f::REPO), job.clone()),
            f::get(&format!("repos/{}/actions/jobs/501", f::REPO), job),
        ],
    );
    tail.extend(selected);
    tail.extend(returned_read_phase(K::Logs, 42));
    tail.extend(returned_read_phase(K::Annotations, 42));
    let (f, p, mut app, all) = prepared(protocol, tail);
    let source = app.scoped_actions_v1.as_ref().unwrap().source.clone();
    let page = app.scoped_actions_v1.as_ref().unwrap().page.clone();
    let old_position = (
        app.actions.as_ref().unwrap().selected,
        app.actions.as_ref().unwrap().top,
    );
    let _drafts = confirm(&mut app, &p, &f);
    let reference = app.form.as_ref().unwrap().actions_publication.clone();
    let sql = f::sql_state(&f);
    let git = f::git_state(&f);
    let old_receipt = metadata(&f);
    let work = w::request(&mut app, FormAction::OpenDispatchRunV7);
    install_work(&mut app, &p, &f, work);
    assert!(app.form.is_none() && app.overlay.is_none());
    assert!(Arc::ptr_eq(
        &app.scoped_actions_v1.as_ref().unwrap().source,
        &source
    ));
    assert_eq!(
        app.actions
            .as_ref()
            .unwrap()
            .run
            .as_ref()
            .unwrap()
            .attempt
            .run
            .id,
        124
    );
    assert_eq!(app.actions.as_ref().unwrap().view, ActionsView::Run);
    continue_read(&mut app, &p, &f, ActionsAction::View(ActionsView::Jobs));
    assert_eq!(app.actions.as_ref().unwrap().view, ActionsView::Jobs);
    continue_read(&mut app, &p, &f, ActionsAction::OpenJob(501));
    assert_eq!(
        app.actions
            .as_ref()
            .unwrap()
            .run
            .as_ref()
            .unwrap()
            .selected_job,
        Some(501)
    );
    continue_read(&mut app, &p, &f, ActionsAction::Logs(Some(501)));
    assert!(app
        .actions
        .as_ref()
        .unwrap()
        .run
        .as_ref()
        .unwrap()
        .logs
        .as_ref()
        .unwrap()
        .contains("PRIVATE_EXACT_JOB_501_LOG"));
    continue_read(&mut app, &p, &f, ActionsAction::Annotations(501));
    assert!(app
        .actions
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .any(|row| row.label.contains("PRIVATE_CATALOGUE_ANNOTATION")));
    assert!(native::intercept(&mut app, &ActionsAction::Back));
    assert!(native::take_work(&mut app).is_none());
    assert_eq!(app.overlay, Some(crate::app::Overlay::Form));
    assert_eq!(app.form.as_ref().unwrap().actions_publication, reference);
    assert_eq!(
        (
            app.actions.as_ref().unwrap().selected,
            app.actions.as_ref().unwrap().top
        ),
        old_position
    );
    assert!(Arc::ptr_eq(
        &app.scoped_actions_v1.as_ref().unwrap().page,
        &page
    ));
    assert!(Arc::ptr_eq(
        &app.scoped_actions_v1.as_ref().unwrap().source,
        &source
    ));
    assert!(!app.form.as_ref().unwrap().plan_ready());
    assert_eq!(metadata(&f), old_receipt);
    readonly(&f, &sql, &git, &all);
}

#[test]
fn scoped_actions_dispatch_v7_returned_read_denies_writers_resources_search_watch_without_io() {
    use crate::scoped_actions_v1::ScopedActionsRequestKindV1 as K;
    let protocol = DispatchProtocolV7::Detailed2026Run200;
    let mut tail = confirm_phase(protocol, 42);
    tail.extend(returned_read_phase(K::Attempt, 42));
    let (f, p, mut app, all) = prepared(protocol, tail);
    let _drafts = confirm(&mut app, &p, &f);
    let work = w::request(&mut app, FormAction::OpenDispatchRunV7);
    install_work(&mut app, &p, &f, work);
    let before = (f::sql_state(&f), f::git_state(&f), metadata(&f));
    for action in [
        ActionsAction::Compose(ActionsCommand::Cancel),
        ActionsAction::Compose(ActionsCommand::RerunAll),
        ActionsAction::View(ActionsView::Artifacts),
        ActionsAction::Compose(ActionsCommand::SearchLogs),
        ActionsAction::Watch,
    ] {
        assert!(native::intercept(&mut app, &action));
        assert!(app.scoped_actions_work_v1.is_none() && app.scoped_actions_owner_v1.is_none());
        assert_eq!(app.overlay, Some(crate::app::Overlay::Notice));
        app.dispatch(crate::app::Action::Close);
    }
    assert_eq!((f::sql_state(&f), f::git_state(&f), metadata(&f)), before);
    exact(&f, &all);
}

#[test]
fn scoped_actions_dispatch_v7_task54_w7_actual_held_open_run_resize_retires_only_matching_busy_receipt(
) {
    use crate::scoped_actions_v1::ScopedActionsRequestKindV1 as K;
    use std::time::{Duration, Instant};
    let protocol = DispatchProtocolV7::Detailed2026Run200;
    let mut tail = confirm_phase(protocol, 42);
    let mut held = returned_read_phase(K::Attempt, 42);
    held[3].delay_ms = 60_000;
    held.truncate(4);
    tail.extend(held);
    let (f, p, mut app, all) = prepared(protocol, tail);
    let _drafts = confirm(&mut app, &p, &f);
    let original = metadata(&f);
    let sql = f::sql_state(&f);
    let git = f::git_state(&f);
    let source = app.scoped_actions_v1.as_ref().unwrap().source.clone();
    let work = w::request(&mut app, FormAction::OpenDispatchRunV7);
    let owner = native::ScopedActionsOwnerV1::new(work.clone()).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let worker_owner = owner.clone();
    let worker_provider = p.clone();
    let database = f::database(&f);
    let worker = std::thread::spawn(move || {
        super::super::super::execute(&worker_owner, &worker_provider, &database, None)
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    let identity = loop {
        let journal = std::fs::read_to_string(f.root.join("gh-fixture/commands.jsonl")).unwrap();
        let rows = journal
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();
        if rows.len() == all.len() {
            let pid = i32::try_from(rows.last().unwrap()["pid"].as_i64().unwrap()).unwrap();
            let child = crate::owned_pty_processes_test_support::process_identity(pid)
                .unwrap()
                .unwrap();
            let parent = crate::owned_pty_processes_test_support::process_identity(
                std::process::id() as i32,
            )
            .unwrap()
            .unwrap();
            assert_eq!(child.parent, parent.pid);
            assert_eq!(child.uid, parent.uid);
            assert_eq!(child.session, parent.session);
            assert!(!child.zombie && child.birth >= parent.birth);
            break child;
        }
        assert!(
            Instant::now() < deadline,
            "Actual held original Run GET: {journal}"
        );
        std::thread::yield_now();
    };
    assert!(app.form.as_ref().unwrap().submitting);
    assert!(super::super::super::resize(&mut app));
    assert!(work.cancellation().is_cancelled());
    assert!(worker.join().unwrap().is_err());
    assert!(
        crate::owned_pty_processes_test_support::process_identity(identity.pid)
            .unwrap()
            .is_none()
    );
    let form = app
        .form
        .as_ref()
        .expect("Same protected receipt remains recoverable");
    assert!(!form.submitting && !form.plan_ready());
    assert_eq!(app.overlay, Some(crate::app::Overlay::Form));
    assert!(Arc::ptr_eq(
        &app.scoped_actions_v1.as_ref().unwrap().source,
        &source
    ));
    assert_eq!(metadata(&f), original);
    readonly(&f, &sql, &git, &all);
}

pub(super) fn history_phase(protocol: DispatchProtocolV7, uid: u64) -> Vec<GhStep> {
    let mut body = vec![versioned(
        &format!("repos/{}/actions/workflows/7", f::REPO),
        r::workflow(),
        protocol,
    )];
    if protocol.detailed() {
        body.push(versioned(
            &format!("repos/{}/actions/runs/124", f::REPO),
            returned_run(),
            protocol,
        ));
    }
    r::wrap(body, uid)
}

#[test]
fn scoped_actions_dispatch_v7_actual_own_confirm_inspect_acceptance_ack_and_reinspect_no_resend() {
    for protocol in [
        DispatchProtocolV7::Compatible2022Acceptance204,
        DispatchProtocolV7::Detailed2026Run200,
    ] {
        let mut tail = confirm_phase(protocol, 42);
        tail.extend(history_phase(protocol, 42));
        tail.extend(history_phase(protocol, 42));
        tail.extend(history_phase(protocol, 42));
        let (f, p, mut app, all) = prepared(protocol, tail);
        let source = app.scoped_actions_v1.as_ref().unwrap().source.clone();
        let _drafts = confirm(&mut app, &p, &f);
        let original = metadata(&f);
        let work = w::request(&mut app, FormAction::Submit);
        install_work(&mut app, &p, &f, work);
        assert!(app
            .form
            .as_ref()
            .unwrap()
            .plan_lines()
            .iter()
            .any(|line| line.contains("acceptance")
                && (line.contains("not workflow completion")
                    || line.contains("no actual run ID"))));
        let work = w::request(&mut app, FormAction::AcknowledgeActionsAcceptanceV5);
        install_work(&mut app, &p, &f, work);
        assert_eq!(metadata(&f).0, "acknowledged");
        assert_eq!(
            (metadata(&f).1, metadata(&f).2, metadata(&f).3),
            (original.1, original.2, original.3)
        );
        assert!(app
            .form
            .as_ref()
            .unwrap()
            .plan_lines()
            .iter()
            .any(|line| line.contains("LOCAL ONLY") && line.contains("no POST/retry")));
        assert!(!app.form.as_ref().unwrap().plan_ready());
        let work = w::request(&mut app, FormAction::Submit);
        install_work(&mut app, &p, &f, work);
        assert!(Arc::ptr_eq(
            &app.scoped_actions_v1.as_ref().unwrap().source,
            &source
        ));
        assert_eq!(metadata(&f).0, "acknowledged");
        assert!(
            dispatch_ui::capture(app.form.as_ref().unwrap(), DispatchPhaseV7::Inspect)
                .unwrap()
                .owned_history()
                .is_some()
        );
        exact(&f, &all);
    }
}

#[test]
fn scoped_actions_dispatch_v7_original_raw_context_options_receipt_drift_never_releases_or_reissues(
) {
    for column in ["context_json", "options_json", "receipt_json"] {
        let protocol = DispatchProtocolV7::Detailed2026Run200;
        let mut tail = confirm_phase(protocol, 42);
        tail.extend(history_phase(protocol, 42));
        let (f, p, mut app, all) = prepared(protocol, tail);
        let _drafts = confirm(&mut app, &p, &f);
        let work = w::request(&mut app, FormAction::Submit);
        install_work(&mut app, &p, &f, work);
        let database = rusqlite::Connection::open(f::database(&f)).unwrap();
        assert_eq!(
            database
                .execute(
                    &format!("UPDATE actions_write_attempts SET {column}={column}||' '"),
                    []
                )
                .unwrap(),
            1
        );
        let sql = f::sql_state(&f);
        let original = metadata(&f);
        let work = w::request(&mut app, FormAction::AcknowledgeActionsAcceptanceV5);
        let owner = native::ScopedActionsOwnerV1::new(work).unwrap();
        app.scoped_actions_owner_v1 = Some(owner.clone());
        assert!(super::super::super::execute(&owner, &p, &f::database(&f), None).is_err());
        assert_eq!(metadata(&f), original);
        assert_eq!(metadata(&f).0, "accepted_pending");
        assert_eq!(f::sql_state(&f), sql);
        exact(&f, &all);
    }
}
