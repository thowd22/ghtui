//! SOURCE ONLY / UNRUN. Real original factory/Core and actual strict GH
//! children, shared exact parser, independent Git/SQLite effects.
use super::*;
use crate::{
    actions_workspace::ActionsAction,
    scoped_actions_test_support_v1 as f,
    scoped_actions_v1::{ScopedActionsRequestKindV1 as K, ScopedActionsSourceV1},
    storage::Cache,
    test_support::{Fixture, GhStep},
};
use serde_json::json;
use std::time::Duration;

struct Original {
    fixture: Fixture,
    provider: GithubProvider,
    source: Arc<ScopedActionsSourceV1>,
    run: Arc<ScopedActionsRunTargetV1>,
    job: Arc<ScopedActionsJobTargetV1>,
    identity: HostIdentity,
    steps: Vec<GhStep>,
}
fn original(tail: Vec<GhStep>) -> Original {
    let mut steps = f::operation(K::Attempt, 42);
    steps.extend(tail);
    let (fixture, provider, mut app, all) = f::setup(steps);
    let prepared = f::read(
        &mut app,
        &provider,
        &fixture,
        ActionsAction::OpenRun {
            repository: f::REPO.into(),
            id: 123,
            attempt: 2,
        },
    )
    .unwrap();
    let run = prepared.page.run().unwrap().clone();
    let job = prepared.page.job(501).unwrap();
    assert!(Arc::ptr_eq(job.run(), &run));
    Original {
        fixture,
        provider,
        source: prepared.page.source().clone(),
        identity: prepared.workspace.identity,
        run,
        job,
        steps: all,
    }
}
fn begin() -> Vec<GhStep> {
    let mut steps = f::principal(42);
    steps.push(f::get(&format!("repos/{}", f::REPO), f::repo()));
    steps
}
fn closing() -> Vec<GhStep> {
    let mut steps = vec![f::get(&format!("repos/{}", f::REPO), f::repo())];
    steps.extend(f::principal(42));
    steps
}
fn direct(raw: Value) -> GhStep {
    f::get(&format!("repos/{}/actions/jobs/501", f::REPO), raw)
}
fn attempt(raw: Value) -> GhStep {
    f::get(
        &format!("repos/{}/actions/runs/123/attempts/2", f::REPO),
        raw,
    )
}
fn members(page: u8, total: u64, rows: Vec<Value>) -> GhStep {
    f::get(
        &format!(
            "repos/{}/actions/runs/123/attempts/2/jobs?per_page=100&page={page}",
            f::REPO
        ),
        json!({"total_count":total,"jobs":rows}),
    )
}
fn supplied() -> Value {
    let mut raw = f::job();
    raw["run_attempt"] = json!(2);
    raw
}
fn full(first: u64) -> Vec<Value> {
    (first..first + 100)
        .map(|id| {
            let mut job = f::job();
            job["id"] = json!(id);
            job["node_id"] = json!(format!("JOB_{id}"));
            job["check_run_url"] = json!(format!(
                "https://{}/api/v3/repos/{}/check-runs/{id}",
                f::HOST,
                f::REPO
            ));
            job
        })
        .collect()
}
fn check(mut reads: Vec<GhStep>, success: bool) {
    let mut tail = begin();
    tail.append(&mut reads);
    tail.extend(closing());
    let original = original(tail);
    let sql = f::sql_state(&original.fixture);
    let git = f::git_state(&original.fixture);
    let token = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let phase = original
        .provider
        .clone()
        .with_catalogue_read_deadline(deadline);
    let mut limits = RepositoryCatalogueAccessLimits::default();
    limits.transport.max_response_bytes = 2 * 1024 * 1024;
    limits.transport.max_document_bytes = 64 * 1024;
    limits.transport.max_duration = Duration::from_secs(30);
    limits.max_repository_reads = 2;
    let mut session = phase
        .begin_repository_catalogue_access(original.source.original().source(), &limits, &token)
        .unwrap();
    session.verify(original.source.reference(), &token).unwrap();
    let window = session
        // The actual opening repository GET above is already charged. Only
        // the one closing repository GET below remains on this same session.
        .reserve_measured_external_reads(15, 512 * 1024, 1024 * 1024, 1, &token)
        .unwrap();
    let measured = phase.clone().with_measured_read_window(window).unwrap();
    let value = verify(
        &measured,
        f::REPO,
        &original.run,
        &original.job,
        &token,
        deadline,
    );
    assert_eq!(
        value.is_ok(),
        success,
        "actual guarded membership result: {value:?}"
    );
    session.verify(original.source.reference(), &token).unwrap();
    session
        .close_with_guard(&token, || {
            Cache::open(&f::database(&original.fixture))
                .unwrap()
                .validate_repository_catalogue_intent_source(original.source.original())
                .map_err(|_| refused())
        })
        .unwrap();
    assert_eq!(f::sql_state(&original.fixture), sql);
    assert_eq!(f::git_state(&original.fixture), git);
    f::exact(&original.fixture, &original.steps);
}
#[test]
fn scoped_actions_rerun_v4_same_raw_receipt_accepts_original_attempt_without_claiming_advance() {
    let original = original(vec![]);
    let sql = f::sql_state(&original.fixture);
    let git = f::git_state(&original.fixture);
    let (run, node, suite) = bind_scoped_actions_run_raw_v3(
        &f::run(),
        f::REPO,
        &original.run,
        ScopedActionsRunRawPhaseV3::RerunReceipt,
    )
    .unwrap();
    assert_eq!(run.attempt, 2);
    assert_eq!(node.as_deref(), Some("WR_123"));
    assert_eq!(suite, Some(901));
    assert_eq!(f::sql_state(&original.fixture), sql);
    assert_eq!(f::git_state(&original.fixture), git);
    f::exact(&original.fixture, &original.steps);
}
#[test]
fn scoped_actions_rerun_v4_new_raw_attempt_is_only_rerun_receipt_not_cancel_or_read() {
    let original = original(vec![]);
    let mut raw = f::run();
    raw["run_attempt"] = json!(3);
    raw["status"] = json!("queued");
    raw["conclusion"] = Value::Null;
    let (run, _, _) = bind_scoped_actions_run_raw_v3(
        &raw,
        f::REPO,
        &original.run,
        ScopedActionsRunRawPhaseV3::RerunReceipt,
    )
    .unwrap();
    assert_eq!(run.attempt, 3);
    assert_eq!(run.status, "queued");
    for phase in [
        ScopedActionsRunRawPhaseV3::Read,
        ScopedActionsRunRawPhaseV3::UnchangedPrewrite,
        ScopedActionsRunRawPhaseV3::CancelReceipt,
    ] {
        assert!(bind_scoped_actions_run_raw_v3(&raw, f::REPO, &original.run, phase).is_err());
    }
    f::exact(&original.fixture, &original.steps);
}
#[test]
fn scoped_actions_rerun_v4_regressed_receipt_attempt_never_counts_as_rerun() {
    let original = original(vec![]);
    let mut raw = f::run();
    raw["run_attempt"] = json!(1);
    assert!(bind_scoped_actions_run_raw_v3(
        &raw,
        f::REPO,
        &original.run,
        ScopedActionsRunRawPhaseV3::RerunReceipt
    )
    .is_err());
    f::exact(&original.fixture, &original.steps);
}
#[test]
fn scoped_actions_rerun_v4_receipt_keeps_actual_repository_run_node_event_suite_and_head() {
    let original = original(vec![]);
    for (path, value) in [
        (vec!["id"], json!(124)),
        (vec!["node_id"], json!("OTHER_RUN")),
        (vec!["workflow_id"], json!(8)),
        (vec!["event"], json!("workflow_dispatch")),
        (vec!["head_sha"], json!("b".repeat(40))),
        (vec!["check_suite_id"], json!(902)),
        (vec!["repository", "id"], json!(10001)),
        (vec!["repository", "node_id"], json!("OTHER_REPOSITORY")),
        (vec!["repository", "full_name"], json!("other/name")),
    ] {
        let mut raw = f::run();
        raw["run_attempt"] = json!(3);
        if path.len() == 2 {
            raw[path[0]][path[1]] = value;
        } else {
            raw[path[0]] = value;
        }
        assert!(bind_scoped_actions_run_raw_v3(
            &raw,
            f::REPO,
            &original.run,
            ScopedActionsRunRawPhaseV3::RerunReceipt
        )
        .is_err());
    }
    f::exact(&original.fixture, &original.steps);
}
#[test]
fn scoped_actions_rerun_v4_publication_attempt_requires_real_positive_json_not_default_one() {
    let original = original(vec![]);
    for value in [Value::Null, json!(0), json!("2"), json!(2.5)] {
        let mut raw = f::run();
        raw["run_attempt"] = value;
        assert!(bind_scoped_actions_run_raw_v3(
            &raw,
            f::REPO,
            &original.run,
            ScopedActionsRunRawPhaseV3::RerunReceipt
        )
        .is_err());
    }
    let mut raw = f::run();
    raw.as_object_mut().unwrap().remove("run_attempt");
    assert!(bind_scoped_actions_run_raw_v3(
        &raw,
        f::REPO,
        &original.run,
        ScopedActionsRunRawPhaseV3::RerunReceipt
    )
    .is_err());
    f::exact(&original.fixture, &original.steps);
}
#[test]
fn scoped_actions_rerun_v4_receipt_guard_does_not_create_new_source_or_touch_sql_git() {
    let original = original(vec![]);
    let sql = f::sql_state(&original.fixture);
    let git = f::git_state(&original.fixture);
    let before = original.run.clone();
    let mut raw = f::run();
    raw["run_attempt"] = json!(3);
    let _ = bind_scoped_actions_run_raw_v3(
        &raw,
        f::REPO,
        &original.run,
        ScopedActionsRunRawPhaseV3::RerunReceipt,
    )
    .unwrap();
    assert!(Arc::ptr_eq(&before, &original.run));
    assert_eq!(original.run.run().attempt, 2);
    assert_eq!(f::sql_state(&original.fixture), sql);
    assert_eq!(f::git_state(&original.fixture), git);
    f::exact(&original.fixture, &original.steps);
}
#[test]
fn scoped_actions_rerun_v4_actual_supplied_attempt_uses_exact_original_membership() {
    check(
        vec![
            direct(supplied()),
            attempt(f::run()),
            members(1, 1, vec![supplied()]),
        ],
        true,
    );
}
#[test]
fn scoped_actions_rerun_v4_genuinely_omitted_attempt_is_bound_not_defaulted() {
    check(
        vec![
            direct(f::job()),
            attempt(f::run()),
            members(1, 1, vec![f::job()]),
        ],
        true,
    );
}
#[test]
fn scoped_actions_rerun_v4_null_zero_string_fraction_or_mismatched_attempt_never_falls_back() {
    for value in [Value::Null, json!(0), json!("2"), json!(2.5), json!(3)] {
        let mut raw = f::job();
        raw["run_attempt"] = value;
        check(vec![direct(raw)], false);
    }
}
#[test]
fn scoped_actions_rerun_v4_direct_id_run_head_node_or_check_url_change_refuses() {
    for (key, value) in [
        ("id", json!(502)),
        ("run_id", json!(124)),
        ("head_sha", json!("b".repeat(40))),
        ("node_id", json!("OTHER_JOB")),
        ("check_run_url", json!("https://github.invalid/other")),
    ] {
        let mut raw = supplied();
        raw[key] = value;
        check(vec![direct(raw), attempt(f::run())], false);
    }
}
#[test]
fn scoped_actions_rerun_v4_exact_attempt_raw_pins_are_checked_before_membership() {
    for (key, value) in [
        ("node_id", json!("OTHER_RUN")),
        ("workflow_id", json!(8)),
        ("event", json!("workflow_dispatch")),
        ("check_suite_id", json!(902)),
        ("run_attempt", json!(3)),
    ] {
        let mut raw = f::run();
        raw[key] = value;
        check(vec![direct(f::job()), attempt(raw)], false);
    }
    let mut raw = f::run();
    raw["repository"]["id"] = json!(10001);
    check(vec![direct(f::job()), attempt(raw)], false);
}
#[test]
fn scoped_actions_rerun_v4_selected_job_on_second_bounded_page_is_positive_partial_evidence() {
    check(
        vec![
            direct(f::job()),
            attempt(f::run()),
            members(1, 300, full(1000)),
            members(2, 300, vec![f::job()]),
        ],
        true,
    );
}
#[test]
fn scoped_actions_rerun_v4_missing_after_two_full_pages_is_not_complete_absence() {
    check(
        vec![
            direct(f::job()),
            attempt(f::run()),
            members(1, 300, full(1000)),
            members(2, 300, full(2000)),
        ],
        false,
    );
}
#[test]
fn scoped_actions_rerun_v4_duplicate_selected_member_is_not_adopted() {
    check(
        vec![
            direct(f::job()),
            attempt(f::run()),
            members(1, 2, vec![f::job(), f::job()]),
        ],
        false,
    );
}
#[test]
fn scoped_actions_rerun_v4_membership_node_or_attempt_differs_from_direct_and_original() {
    for (key, value) in [
        ("node_id", json!("OTHER_JOB")),
        ("run_attempt", Value::Null),
        ("head_sha", json!("b".repeat(40))),
    ] {
        let mut raw = f::job();
        raw[key] = value;
        check(
            vec![
                direct(f::job()),
                attempt(f::run()),
                members(1, 1, vec![raw]),
            ],
            false,
        );
    }
}
#[test]
fn scoped_actions_rerun_v4_malformed_or_drifting_page_envelope_refuses() {
    check(
        vec![
            direct(f::job()),
            attempt(f::run()),
            members(1, 100, full(1000)),
            members(2, 100, vec![f::job()]),
        ],
        false,
    );
    check(
        vec![
            direct(f::job()),
            attempt(f::run()),
            members(1, 0, vec![f::job()]),
        ],
        false,
    );
    check(
        vec![
            direct(f::job()),
            attempt(f::run()),
            members(1, 300, full(1000)),
            members(2, 301, vec![f::job()]),
        ],
        false,
    );
}
#[test]
fn scoped_actions_rerun_v4_cancelled_actual_original_read_has_zero_extra_children() {
    let original = original(vec![]);
    let sql = f::sql_state(&original.fixture);
    let git = f::git_state(&original.fixture);
    let token = CancellationToken::default();
    token.cancel();
    assert!(verify(
        &original.provider,
        f::REPO,
        &original.run,
        &original.job,
        &token,
        Instant::now() + Duration::from_secs(30)
    )
    .is_err());
    assert_eq!(f::sql_state(&original.fixture), sql);
    assert_eq!(f::git_state(&original.fixture), git);
    f::exact(&original.fixture, &original.steps);
}
fn write_step(action: RunMutation, fails: bool) -> GhStep {
    let (suffix, debug): (String, bool) = match action {
        RunMutation::RerunAll { debug } => ("actions/runs/123/rerun".into(), debug),
        RunMutation::RerunFailed { debug } => ("actions/runs/123/rerun-failed-jobs".into(), debug),
        RunMutation::RerunJob { debug, .. } => ("actions/jobs/501/rerun".into(), debug),
        _ => panic!("Closed rerun test operation"),
    };
    let mut step = GhStep::read(
        &[
            "api",
            &format!("repos/{}/{suffix}", f::REPO),
            "--hostname",
            f::HOST,
            "--method",
            "POST",
            "--input",
            "-",
        ],
        "{}",
    );
    step.stdin = serde_json::to_vec(&json!({"enable_debug_logging":debug})).unwrap();
    if fails {
        step.status = 1;
        step.stderr = b"PRIVATE_RERUN_FAILURE".to_vec();
    }
    step
}
fn exact_writes(original: &Original) {
    let rows =
        std::fs::read_to_string(original.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    let rows = rows
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), original.steps.len(), "No extra child/retry");
    for (row, expected) in rows.iter().zip(&original.steps) {
        assert_eq!(row["args"], json!(expected.args));
        assert_eq!(row["matched"], true);
        let hex = expected
            .stdin
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        assert_eq!(row["stdin_hex"], hex);
    }
}
fn guarded_write(action: RunMutation, receipt: Value, fails: bool, refuse_before: bool) {
    let mut tail = begin();
    tail.extend(f::principal(42));
    tail.push(f::get(
        &format!("repos/{}/actions/runs/123", f::REPO),
        f::run(),
    ));
    if matches!(action, RunMutation::RerunJob { .. }) {
        tail.extend([
            direct(f::job()),
            attempt(f::run()),
            members(1, 1, vec![f::job()]),
        ]);
    }
    tail.extend(closing());
    if !refuse_before {
        tail.push(write_step(action, fails));
        if !fails {
            tail.push(f::get(
                &format!("repos/{}/actions/runs/123", f::REPO),
                receipt.clone(),
            ));
        }
    }
    let original = original(tail);
    let sql = f::sql_state(&original.fixture);
    let git = f::git_state(&original.fixture);
    let token = CancellationToken::default();
    let mut limits = RepositoryCatalogueAccessLimits::default();
    limits.transport.max_response_bytes = 2 * 1024 * 1024;
    limits.transport.max_document_bytes = 64 * 1024;
    limits.transport.max_duration = Duration::from_secs(30);
    limits.max_repository_reads = 2;
    let mut session = original
        .provider
        .begin_repository_catalogue_access(original.source.original().source(), &limits, &token)
        .unwrap();
    session.verify(original.source.reference(), &token).unwrap();
    let mut captures = session
        .reserve_postwrite_capture_v2(&[64 * 1024, 512 * 1024, 64 * 1024], 1, &token)
        .unwrap();
    let window = session
        .reserve_measured_external_reads(15, 512 * 1024, 1024 * 1024, 1, &token)
        .unwrap();
    let deadline = window.deadline();
    let measured = original
        .provider
        .clone()
        .with_measured_read_window(window)
        .unwrap();
    let calls = std::cell::Cell::new(0);
    let selected = matches!(action, RunMutation::RerunJob { .. }).then_some(&original.job);
    let result=original.provider.mutate_run_guarded_v4(&measured,&mut captures,&original.identity,
        &original.run,selected,f::REPO,action,&token,deadline,|boundary|{
            calls.set(calls.get()+1);
            assert!(Arc::ptr_eq(boundary.original(),&original.run));assert_eq!(boundary.action_v4(),action);
            assert_eq!(boundary.method(),"POST");
            assert_eq!(boundary.input(),Some(&json!({"enable_debug_logging":match action{
                RunMutation::RerunAll{debug}|RunMutation::RerunFailed{debug}|RunMutation::RerunJob{debug,..}=>debug,_=>false,
            }})));
            assert_eq!(boundary.selected_job_v4().map(|job|job.id()),selected.map(|job|job.id()));
            session.verify(original.source.reference(),&token)?;
            session.close_with_guard(&token,||Cache::open(&f::database(&original.fixture)).unwrap()
                .validate_repository_catalogue_intent_source(original.source.original()).map_err(|_|refused()))?;
            if refuse_before {Err(refused())}else{Ok(())}
        });
    assert_eq!(calls.get(), 1);
    if refuse_before {
        assert!(matches!(
            result,
            Err(super::super::scoped_run_write_v3::ActionsInvocationFailureV3::NotStarted(_))
        ));
        assert_eq!(captures.consumed_requests(), 0);
    } else if fails || receipt["node_id"] != "WR_123" {
        assert!(matches!(
            result,
            Err(
                super::super::scoped_run_write_v3::ActionsInvocationFailureV3::WriteMayHaveStarted(
                    _
                )
            )
        ));
        assert_eq!(captures.consumed_requests(), if fails { 1 } else { 2 });
    } else {
        let outcome = result.unwrap();
        assert!(outcome.accepted);
        assert_eq!(outcome.action, action);
        let observed = outcome.run.unwrap();
        assert_eq!(observed.attempt, receipt["run_attempt"].as_u64().unwrap());
        assert_eq!(
            outcome.completed,
            observed.attempt > 2,
            "means attempt transition, NOT workflow success"
        );
        assert_eq!(captures.consumed_requests(), 2);
    }
    assert_eq!(f::sql_state(&original.fixture), sql);
    assert_eq!(f::git_state(&original.fixture), git);
    exact_writes(&original);
}
#[test]
fn scoped_actions_rerun_v4_all_false_uses_same_prepaid_post_and_unchanged_attempt_acceptance() {
    guarded_write(
        RunMutation::RerunAll { debug: false },
        f::run(),
        false,
        false,
    );
}
#[test]
fn scoped_actions_rerun_v4_failed_true_new_attempt_is_not_workflow_completion() {
    let mut receipt = f::run();
    receipt["run_attempt"] = json!(3);
    receipt["status"] = json!("queued");
    receipt["conclusion"] = Value::Null;
    guarded_write(
        RunMutation::RerunFailed { debug: true },
        receipt,
        false,
        false,
    );
}
#[test]
fn scoped_actions_rerun_v4_job_false_same_original_membership_and_fixed_database_id_post() {
    guarded_write(
        RunMutation::RerunJob {
            job_id: 501,
            debug: false,
        },
        f::run(),
        false,
        false,
    );
}
#[test]
fn scoped_actions_rerun_v4_all_true_debug_is_exact_json_not_an_arbitrary_debugger_flag() {
    guarded_write(
        RunMutation::RerunAll { debug: true },
        f::run(),
        false,
        false,
    );
}
#[test]
fn scoped_actions_rerun_v4_failed_actual_post_error_is_uncertain_with_no_receipt_or_retry() {
    guarded_write(
        RunMutation::RerunFailed { debug: false },
        f::run(),
        true,
        false,
    );
}
#[test]
fn scoped_actions_rerun_v4_original_source_boundary_refusal_consumes_no_write_capture() {
    guarded_write(
        RunMutation::RerunAll { debug: false },
        f::run(),
        false,
        true,
    );
}
#[test]
fn scoped_actions_rerun_v4_actual_receipt_node_drift_is_uncertain_after_exactly_one_post() {
    let mut receipt = f::run();
    receipt["node_id"] = json!("OTHER_RUN");
    guarded_write(
        RunMutation::RerunAll { debug: false },
        receipt,
        false,
        false,
    );
}
#[test]
fn scoped_actions_rerun_v4_legacy_none_job_keeps_direct_explicit_attempt_and_six_child_protocol() {
    let action = RunMutation::RerunJob {
        job_id: 501,
        debug: true,
    };
    let mut tail = f::principal(42);
    tail.push(f::get(
        &format!("repos/{}/actions/runs/123", f::REPO),
        f::run(),
    ));
    tail.extend([
        direct(supplied()),
        write_step(action, false),
        f::get(&format!("repos/{}/actions/runs/123", f::REPO), f::run()),
    ]);
    let original = original(tail);
    let sql = f::sql_state(&original.fixture);
    let git = f::git_state(&original.fixture);
    let identity = &original.identity;
    let value = original
        .provider
        .mutate_run(
            identity,
            f::REPO,
            original.run.run(),
            action,
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(value.accepted);
    assert!(!value.completed);
    exact_writes(&original);
    assert_eq!(f::sql_state(&original.fixture), sql);
    assert_eq!(f::git_state(&original.fixture), git);
}
#[test]
fn scoped_actions_rerun_v4_legacy_none_omission_still_refuses_without_endpoint_fallback() {
    let mut tail = f::principal(42);
    tail.push(f::get(
        &format!("repos/{}/actions/runs/123", f::REPO),
        f::run(),
    ));
    tail.push(direct(f::job()));
    let original = original(tail);
    let sql = f::sql_state(&original.fixture);
    let git = f::git_state(&original.fixture);
    let identity = &original.identity;
    assert!(original
        .provider
        .mutate_run(
            identity,
            f::REPO,
            original.run.run(),
            RunMutation::RerunJob {
                job_id: 501,
                debug: false
            },
            &CancellationToken::default()
        )
        .is_err());
    f::exact(&original.fixture, &original.steps);
    assert_eq!(f::sql_state(&original.fixture), sql);
    assert_eq!(f::git_state(&original.fixture), git);
}
#[test]
fn scoped_actions_rerun_v4_expired_original_clock_has_zero_extra_children() {
    let original = original(vec![]);
    let sql = f::sql_state(&original.fixture);
    let git = f::git_state(&original.fixture);
    assert!(verify(
        &original.provider,
        f::REPO,
        &original.run,
        &original.job,
        &CancellationToken::default(),
        Instant::now()
    )
    .is_err());
    assert_eq!(f::sql_state(&original.fixture), sql);
    assert_eq!(f::git_state(&original.fixture), git);
    f::exact(&original.fixture, &original.steps);
}
