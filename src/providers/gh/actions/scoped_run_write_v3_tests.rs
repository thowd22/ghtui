//! SOURCE ONLY / UNRUN. Actual original factory, same measured Core/cursor,
//! strict offline children and independent SQL/Git. Not a native TUI claim.
use super::*;
use crate::{
    actions_workspace::ActionsAction,
    scoped_actions_test_support_v1 as f,
    scoped_actions_v1::{ScopedActionsRequestKindV1, ScopedActionsSourceV1},
    storage::Cache,
    test_support::{Fixture, GhStep},
};
use serde_json::json;
use std::{cell::Cell, fs, time::Duration};

fn running() -> Value {
    let mut value = f::run();
    value["status"] = json!("in_progress");
    value["conclusion"] = Value::Null;
    value
}
fn cancelled() -> Value {
    let mut value = running();
    value["status"] = json!("completed");
    value["conclusion"] = json!("cancelled");
    value
}
fn post(fail: bool) -> GhStep {
    let mut step = GhStep::read(
        &[
            "api",
            &format!("repos/{}/actions/runs/123/cancel", f::REPO),
            "--hostname",
            f::HOST,
            "--method",
            "POST",
        ],
        "{}",
    );
    if fail {
        step.status = 1;
        step.stderr = b"PRIVATE_ACTIONS_POST_FAILURE".to_vec();
    }
    step
}
struct Original {
    fixture: Fixture,
    provider: GithubProvider,
    source: Arc<ScopedActionsSourceV1>,
    target: Arc<ScopedActionsRunTargetV1>,
    identity: HostIdentity,
    all: Vec<GhStep>,
}
fn original(tail: Vec<GhStep>) -> Original {
    let mut operation = f::operation(ScopedActionsRequestKindV1::Attempt, 42);
    for step in &mut operation {
        if step.args.first().is_some_and(|arg| arg == "api")
            && step.args.get(1).is_some_and(|arg| {
                arg.ends_with("/actions/runs/123") || arg.ends_with("/actions/runs/123/attempts/2")
            })
        {
            step.stdout = running().to_string().into_bytes();
        }
    }
    operation.extend(tail);
    let (fixture, provider, mut app, all) = f::setup(operation);
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
    let target = prepared.page.run().unwrap().clone();
    assert_eq!(target.node_v2(), Some("WR_123"));
    assert_eq!(target.suite_v3(), Some(901));
    assert_eq!(target.run().status, "in_progress");
    Original {
        fixture,
        provider,
        source: prepared.page.source().clone(),
        target,
        identity: prepared.workspace.identity,
        all,
    }
}
fn exact(original: &Original) {
    let rows = fs::read_to_string(original.fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|row| serde_json::from_str::<Value>(row).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        rows.len(),
        original.all.len(),
        "Exact original children; no fallback or retry"
    );
    for (row, expected) in rows.iter().zip(&original.all) {
        assert_eq!(row["args"], json!(expected.args));
        assert_eq!(row["matched"], true);
        assert_eq!(row["stdin_hex"], "");
    }
}
fn begin_steps() -> Vec<GhStep> {
    let mut steps = f::principal(42);
    steps.push(f::get(&format!("repos/{}", f::REPO), f::repo()));
    steps
}
fn guard_steps(raw: Value) -> Vec<GhStep> {
    let mut steps = f::principal(42);
    steps.push(f::get(&format!("repos/{}/actions/runs/123", f::REPO), raw));
    steps
}
fn close_steps(uid: u64, final_principal: bool) -> Vec<GhStep> {
    let mut steps = vec![f::get(&format!("repos/{}", f::REPO), f::repo())];
    if final_principal {
        steps.extend(f::principal(uid));
    }
    steps
}
fn limits() -> RepositoryCatalogueAccessLimits {
    let mut limits = RepositoryCatalogueAccessLimits::default();
    limits.transport.max_response_bytes = 2 * 1024 * 1024;
    limits.transport.max_document_bytes = 64 * 1024;
    limits.transport.max_duration = Duration::from_secs(30);
    limits.max_repository_reads = 2;
    limits
}
fn sql_guard(original: &Original) -> GithubResult<()> {
    Cache::open(&f::database(&original.fixture))
        .unwrap()
        .validate_repository_catalogue_intent_source(original.source.original())
        .map_err(|_| {
            error(
                GithubErrorKind::IdentityChanged,
                "Actual original source SQL changed",
            )
        })
}
type GitState = (String, String, Vec<u8>, Vec<u8>, Vec<u8>);
fn assert_unchanged(original: &Original, sql: &[(String, String, String)], git: &GitState) {
    assert_eq!(f::sql_state(&original.fixture), sql);
    assert_eq!(&f::git_state(&original.fixture), git);
    exact(original);
}

#[test]
fn scoped_actions_publication_v3_same_binder_checks_every_actual_raw_receipt_pin() {
    let original = original(vec![]);
    let sql = f::sql_state(&original.fixture);
    let git = f::git_state(&original.fixture);
    let base = cancelled();
    let (observed, node, suite) = bind_scoped_actions_run_raw_v3(
        &base,
        f::REPO,
        &original.target,
        ScopedActionsRunRawPhaseV3::CancelReceipt,
    )
    .unwrap();
    assert_eq!(observed.conclusion, "cancelled");
    assert_eq!(node.as_deref(), Some("WR_123"));
    assert_eq!(suite, Some(901));
    let changes: &[(&[&str], Value)] = &[
        (&["repository", "id"], json!(10001)),
        (&["repository", "node_id"], json!("REPLACED_REPOSITORY")),
        (&["repository", "full_name"], json!("example/different")),
        (&["id"], json!(124)),
        (&["node_id"], json!("REPLACED_RUN")),
        (&["workflow_id"], json!(8)),
        (&["event"], json!("workflow_dispatch")),
        (&["head_sha"], json!("b".repeat(40))),
        (&["run_attempt"], json!(3)),
        (&["check_suite_id"], json!(902)),
    ];
    for (path, value) in changes {
        let mut changed = base.clone();
        if path.len() == 2 {
            changed[path[0]][path[1]] = value.clone();
        } else {
            changed[path[0]] = value.clone();
        }
        assert!(
            bind_scoped_actions_run_raw_v3(
                &changed,
                f::REPO,
                &original.target,
                ScopedActionsRunRawPhaseV3::CancelReceipt,
            )
            .is_err(),
            "Actual original pin must remain exact: {path:?}"
        );
    }
    assert!(
        bind_scoped_actions_run_raw_v3(
            &base,
            f::REPO,
            &original.target,
            ScopedActionsRunRawPhaseV3::UnchangedPrewrite,
        )
        .is_err(),
        "Completed receipt is not unchanged prewrite approval"
    );
    assert_unchanged(&original, &sql, &git);
}

#[test]
fn scoped_actions_publication_v3_cancel_same_original_pool_prepaid_receipt_and_exact_boundary() {
    for receipt in [running(), cancelled()] {
        let mut tail = begin_steps();
        tail.extend(guard_steps(running()));
        tail.extend(close_steps(42, true));
        tail.push(post(false));
        tail.push(f::get(
            &format!("repos/{}/actions/runs/123", f::REPO),
            receipt.clone(),
        ));
        let original = original(tail);
        let sql = f::sql_state(&original.fixture);
        let git = f::git_state(&original.fixture);
        let token = CancellationToken::default();
        let mut session = original
            .provider
            .begin_repository_catalogue_access(
                original.source.original().source(),
                &limits(),
                &token,
            )
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
        let calls = Cell::new(0);
        let outcome = original
            .provider
            .mutate_run_guarded_v3(
                &measured,
                &mut captures,
                &original.identity,
                &original.target,
                f::REPO,
                RunMutation::Cancel,
                &token,
                deadline,
                |boundary| {
                    calls.set(calls.get() + 1);
                    assert!(Arc::ptr_eq(boundary.original(), &original.target));
                    assert_eq!(boundary.fresh().id, 123);
                    assert_eq!(boundary.repository(), f::REPO);
                    assert_eq!(
                        boundary.endpoint(),
                        format!("repos/{}/actions/runs/123/cancel", f::REPO)
                    );
                    assert_eq!(boundary.method(), "POST");
                    assert!(boundary.input().is_none());
                    assert_eq!(
                        boundary.resource(),
                        "github.invalid/repository-id/10000/actions/run/123"
                    );
                    session.verify(original.source.reference(), &token)?;
                    let closed = session.close_with_guard(&token, || sql_guard(&original))?;
                    assert_eq!(closed.identity().account_id, Some(42));
                    let actual = closed.external_read_accounting().unwrap();
                    assert_eq!(actual.children_started(), 3);
                    assert!(actual.sealed());
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(calls.get(), 1);
        assert!(outcome.accepted);
        assert_eq!(outcome.completed, receipt["conclusion"] == "cancelled");
        assert_eq!(outcome.action, RunMutation::Cancel);
        assert_eq!(outcome.run.unwrap().id, 123);
        assert_eq!(captures.consumed_requests(), 2);
        assert_eq!(
            captures.reserved_requests(),
            3,
            "Final caller principal slot remains prepaid"
        );
        assert_unchanged(&original, &sql, &git);
    }
}

#[test]
fn scoped_actions_publication_v3_changed_prewrite_state_refuses_before_boundary_or_write() {
    let mut tail = begin_steps();
    tail.extend(guard_steps(cancelled()));
    let original = original(tail);
    let sql = f::sql_state(&original.fixture);
    let git = f::git_state(&original.fixture);
    let token = CancellationToken::default();
    let mut session = original
        .provider
        .begin_repository_catalogue_access(original.source.original().source(), &limits(), &token)
        .unwrap();
    session.verify(original.source.reference(), &token).unwrap();
    let mut captures = session
        .reserve_postwrite_capture_v2(&[64 * 1024, 512 * 1024], 1, &token)
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
    let calls = Cell::new(0);
    let result = original.provider.mutate_run_guarded_v3(
        &measured,
        &mut captures,
        &original.identity,
        &original.target,
        f::REPO,
        RunMutation::Cancel,
        &token,
        deadline,
        |_| {
            calls.set(1);
            Ok(())
        },
    );
    assert!(matches!(
        result,
        Err(ActionsInvocationFailureV3::NotStarted(_))
    ));
    assert_eq!(calls.get(), 0);
    assert_eq!(captures.consumed_requests(), 0);
    drop(session);
    assert_unchanged(&original, &sql, &git);
}

#[test]
fn scoped_actions_publication_v3_changed_final_numeric_uid_or_sql_never_invokes_post() {
    for sql_failure in [false, true] {
        let mut tail = begin_steps();
        tail.extend(guard_steps(running()));
        tail.extend(close_steps(43, !sql_failure));
        let original = original(tail);
        let sql = f::sql_state(&original.fixture);
        let git = f::git_state(&original.fixture);
        let token = CancellationToken::default();
        let mut session = original
            .provider
            .begin_repository_catalogue_access(
                original.source.original().source(),
                &limits(),
                &token,
            )
            .unwrap();
        session.verify(original.source.reference(), &token).unwrap();
        let mut captures = session
            .reserve_postwrite_capture_v2(&[64 * 1024, 512 * 1024], 1, &token)
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
        let result = original.provider.mutate_run_guarded_v3(
            &measured,
            &mut captures,
            &original.identity,
            &original.target,
            f::REPO,
            RunMutation::Cancel,
            &token,
            deadline,
            |_| {
                session.verify(original.source.reference(), &token)?;
                if sql_failure {
                    let mut cache = Cache::open(&f::database(&original.fixture)).unwrap();
                    let scope = original.source.original().scope();
                    let sync = original.source.original().sync_source();
                    let old = cache.sync_reservation(scope, sync).unwrap().unwrap();
                    assert_eq!(cache.reserve_sync_generation(scope, sync).unwrap(), old + 1);
                    assert!(cache
                        .validate_repository_catalogue_intent_source(original.source.original())
                        .is_err());
                }
                session.close_with_guard(&token, || sql_guard(&original))?;
                Ok(())
            },
        );
        assert!(matches!(
            result,
            Err(ActionsInvocationFailureV3::NotStarted(_))
        ));
        assert_eq!(captures.consumed_requests(), 0);
        assert_unchanged(&original, &sql, &git);
    }
}

#[test]
fn scoped_actions_publication_v3_cancellation_after_actual_close_is_not_an_invocation() {
    let mut tail = begin_steps();
    tail.extend(guard_steps(running()));
    tail.extend(close_steps(42, true));
    let original = original(tail);
    let sql = f::sql_state(&original.fixture);
    let git = f::git_state(&original.fixture);
    let token = CancellationToken::default();
    let mut session = original
        .provider
        .begin_repository_catalogue_access(original.source.original().source(), &limits(), &token)
        .unwrap();
    session.verify(original.source.reference(), &token).unwrap();
    let mut captures = session
        .reserve_postwrite_capture_v2(&[64 * 1024, 512 * 1024], 1, &token)
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
    let result = original.provider.mutate_run_guarded_v3(
        &measured,
        &mut captures,
        &original.identity,
        &original.target,
        f::REPO,
        RunMutation::Cancel,
        &token,
        deadline,
        |_| {
            session.verify(original.source.reference(), &token)?;
            session.close_with_guard(&token, || sql_guard(&original))?;
            token.cancel();
            Ok(())
        },
    );
    assert!(matches!(
        result,
        Err(ActionsInvocationFailureV3::NotStarted(_))
    ));
    assert_eq!(captures.consumed_requests(), 0);
    assert_unchanged(&original, &sql, &git);
}

#[test]
fn scoped_actions_publication_v3_raw_receipt_identity_drift_is_uncertain_not_completion() {
    for path in ["node_id", "event", "check_suite_id", "repository"] {
        let mut receipt = cancelled();
        receipt[path] = match path {
            "check_suite_id" => json!(902),
            "repository" => json!({"id":10001,"node_id":"R_10001","full_name":f::REPO}),
            _ => json!("CHANGED_AFTER_WRITE"),
        };
        let mut tail = begin_steps();
        tail.extend(guard_steps(running()));
        tail.extend(close_steps(42, true));
        tail.push(post(false));
        tail.push(f::get(
            &format!("repos/{}/actions/runs/123", f::REPO),
            receipt,
        ));
        let original = original(tail);
        let sql = f::sql_state(&original.fixture);
        let git = f::git_state(&original.fixture);
        let token = CancellationToken::default();
        let mut session = original
            .provider
            .begin_repository_catalogue_access(
                original.source.original().source(),
                &limits(),
                &token,
            )
            .unwrap();
        session.verify(original.source.reference(), &token).unwrap();
        let mut captures = session
            .reserve_postwrite_capture_v2(&[64 * 1024, 512 * 1024], 1, &token)
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
        let result = original.provider.mutate_run_guarded_v3(
            &measured,
            &mut captures,
            &original.identity,
            &original.target,
            f::REPO,
            RunMutation::Cancel,
            &token,
            deadline,
            |_| {
                session.verify(original.source.reference(), &token)?;
                session.close_with_guard(&token, || sql_guard(&original))?;
                Ok(())
            },
        );
        let Err(ActionsInvocationFailureV3::WriteMayHaveStarted(failure)) = result else {
            panic!("Actual POST plus mismatched raw receipt must retain uncertainty");
        };
        assert!(matches!(failure.kind, GithubErrorKind::AmbiguousWrite));
        assert_eq!(captures.consumed_requests(), 2);
        assert_unchanged(&original, &sql, &git);
    }
}

#[test]
fn scoped_actions_publication_v3_post_failure_consumes_once_and_never_reads_a_receipt() {
    let mut tail = begin_steps();
    tail.extend(guard_steps(running()));
    tail.extend(close_steps(42, true));
    tail.push(post(true));
    let original = original(tail);
    let sql = f::sql_state(&original.fixture);
    let git = f::git_state(&original.fixture);
    let token = CancellationToken::default();
    let mut session = original
        .provider
        .begin_repository_catalogue_access(original.source.original().source(), &limits(), &token)
        .unwrap();
    session.verify(original.source.reference(), &token).unwrap();
    let mut captures = session
        .reserve_postwrite_capture_v2(&[64 * 1024, 512 * 1024], 1, &token)
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
    let result = original.provider.mutate_run_guarded_v3(
        &measured,
        &mut captures,
        &original.identity,
        &original.target,
        f::REPO,
        RunMutation::Cancel,
        &token,
        deadline,
        |_| {
            session.verify(original.source.reference(), &token)?;
            session.close_with_guard(&token, || sql_guard(&original))?;
            Ok(())
        },
    );
    assert!(matches!(
        result,
        Err(ActionsInvocationFailureV3::WriteMayHaveStarted(_))
    ));
    assert_eq!(captures.consumed_requests(), 1);
    assert_unchanged(&original, &sql, &git);
}

#[test]
fn scoped_actions_publication_v3_sibling_producer_token_deadline_or_operation_cannot_start() {
    for which in 0..5 {
        let original = original(begin_steps());
        let sql = f::sql_state(&original.fixture);
        let git = f::git_state(&original.fixture);
        let token = CancellationToken::default();
        let mut session = original
            .provider
            .begin_repository_catalogue_access(
                original.source.original().source(),
                &limits(),
                &token,
            )
            .unwrap();
        session.verify(original.source.reference(), &token).unwrap();
        let mut captures = session
            .reserve_postwrite_capture_v2(&[64 * 1024, 512 * 1024], 1, &token)
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
        let foreign = GithubProvider::new(f::HOST)
            .unwrap()
            .with_program(original.fixture.root.join("bin/gh"))
            .with_env("GH_FIXTURE_DIR", original.fixture.root.join("gh-fixture"));
        let sibling_token = CancellationToken::default();
        if which == 3 {
            token.cancel();
        }
        let calls = Cell::new(0);
        let result = if which == 0 {
            foreign.mutate_run_guarded_v3(
                &measured,
                &mut captures,
                &original.identity,
                &original.target,
                f::REPO,
                RunMutation::Cancel,
                &token,
                deadline,
                |_| {
                    calls.set(1);
                    Ok(())
                },
            )
        } else {
            original.provider.mutate_run_guarded_v3(
                &measured,
                &mut captures,
                &original.identity,
                &original.target,
                f::REPO,
                if which == 4 {
                    RunMutation::ForceCancel
                } else {
                    RunMutation::Cancel
                },
                if which == 1 { &sibling_token } else { &token },
                if which == 2 { Instant::now() } else { deadline },
                |_| {
                    calls.set(1);
                    Ok(())
                },
            )
        };
        assert!(matches!(
            result,
            Err(ActionsInvocationFailureV3::NotStarted(_))
        ));
        assert_eq!(calls.get(), 0);
        assert_eq!(captures.consumed_requests(), 0);
        drop(session);
        assert_unchanged(&original, &sql, &git);
    }
}

#[test]
fn scoped_actions_publication_v3_legacy_none_cancel_argv_order_and_receipt_stay_exact() {
    let mut tail = f::principal(42);
    tail.push(f::get(
        &format!("repos/{}/actions/runs/123", f::REPO),
        running(),
    ));
    tail.push(post(false));
    tail.push(f::get(
        &format!("repos/{}/actions/runs/123", f::REPO),
        cancelled(),
    ));
    let original = original(tail);
    let sql = f::sql_state(&original.fixture);
    let git = f::git_state(&original.fixture);
    let outcome = original
        .provider
        .mutate_run(
            &original.identity,
            f::REPO,
            original.target.run(),
            RunMutation::Cancel,
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(outcome.accepted && outcome.completed);
    assert_eq!(outcome.run.unwrap().conclusion, "cancelled");
    assert_unchanged(&original, &sql, &git);
}
fn attempt_one(mut raw: Value) -> Value {
    raw["run_attempt"] = json!(1);
    raw
}
fn original_attempt_one(tail: Vec<GhStep>) -> Original {
    // The initial real catalogue is A2. A genuine bounded Refresh observes A1,
    // then its actual opaque member is selected through the ordinary factory.
    // No private target, raw Source or WorkflowRun capability is edited.
    let mut operation = f::operation(ScopedActionsRequestKindV1::Catalogue, 42);
    operation.extend(f::operation(ScopedActionsRequestKindV1::Attempt, 42));
    for step in &mut operation {
        for arg in &mut step.args {
            *arg = arg.replace("/attempts/2", "/attempts/1");
        }
        let Ok(mut raw) = serde_json::from_slice::<Value>(&step.stdout) else {
            continue;
        };
        if raw.get("workflow_runs").is_some() {
            raw["workflow_runs"][0] = attempt_one(running());
        } else if raw["id"] == 123 && raw["workflow_id"] == 7 {
            raw = attempt_one(running());
        }
        step.stdout = raw.to_string().into_bytes();
    }
    operation.extend(tail);
    let (fixture, provider, mut app, all) = f::setup(operation);
    let refreshed = f::read(&mut app, &provider, &fixture, ActionsAction::Refresh).unwrap();
    assert_eq!(refreshed.page.run_target(123, 1).unwrap().run().attempt, 1);
    f::install(&mut app, refreshed);
    let prepared = f::read(
        &mut app,
        &provider,
        &fixture,
        ActionsAction::OpenRun {
            repository: f::REPO.into(),
            id: 123,
            attempt: 1,
        },
    )
    .unwrap();
    let target = prepared.page.run().unwrap().clone();
    assert_eq!(target.run().attempt, 1);
    assert_eq!(target.run().status, "in_progress");
    Original {
        fixture,
        provider,
        source: prepared.page.source().clone(),
        target,
        identity: prepared.workspace.identity,
        all,
    }
}
fn invalid_raw_attempts() -> Vec<Option<Value>> {
    vec![
        None,
        Some(Value::Null),
        Some(json!(0)),
        Some(json!("1")),
        Some(json!(1.5)),
    ]
}
fn change_attempt(mut raw: Value, value: Option<Value>) -> Value {
    if let Some(value) = value {
        raw["run_attempt"] = value;
    } else {
        raw.as_object_mut().unwrap().remove("run_attempt");
    }
    raw
}
#[test]
fn scoped_actions_publication_v3_genuine_attempt_one_requires_actual_raw_prewrite_and_receipt_field(
) {
    let original = original_attempt_one(vec![]);
    let sql = f::sql_state(&original.fixture);
    let git = f::git_state(&original.fixture);
    for phase in [
        ScopedActionsRunRawPhaseV3::UnchangedPrewrite,
        ScopedActionsRunRawPhaseV3::CancelReceipt,
    ] {
        let raw = attempt_one(running());
        assert_eq!(
            bind_scoped_actions_run_raw_v3(&raw, f::REPO, &original.target, phase)
                .unwrap()
                .0
                .attempt,
            1
        );
        for invalid in invalid_raw_attempts() {
            let raw = change_attempt(raw.clone(), invalid);
            assert!(
                bind_scoped_actions_run_raw_v3(&raw, f::REPO, &original.target, phase).is_err(),
                "a genuine original A1 cannot inherit parser fallback1 as publication evidence"
            );
        }
    }
    assert_unchanged(&original, &sql, &git);
}
#[test]
fn scoped_actions_publication_v3_genuine_attempt_one_missing_or_invalid_prewrite_never_reaches_boundary(
) {
    for invalid in invalid_raw_attempts() {
        let wrong = change_attempt(attempt_one(running()), invalid);
        let mut tail = begin_steps();
        tail.extend(guard_steps(wrong));
        tail.extend(close_steps(42, true));
        let original = original_attempt_one(tail);
        let sql = f::sql_state(&original.fixture);
        let git = f::git_state(&original.fixture);
        let token = CancellationToken::default();
        let mut session = original
            .provider
            .begin_repository_catalogue_access(
                original.source.original().source(),
                &limits(),
                &token,
            )
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
        let calls = Cell::new(0);
        let result = original.provider.mutate_run_guarded_v3(
            &measured,
            &mut captures,
            &original.identity,
            &original.target,
            f::REPO,
            RunMutation::Cancel,
            &token,
            deadline,
            |_| {
                // Safe causal negative control: old fallback1 reaches this
                // witness, but even the old source cannot send an unplanned POST.
                calls.set(calls.get() + 1);
                Err(error(
                    GithubErrorKind::InvalidTarget,
                    "Unexpected boundary witness",
                ))
            },
        );
        assert!(matches!(
            result,
            Err(ActionsInvocationFailureV3::NotStarted(_))
        ));
        assert_eq!(
            calls.get(),
            0,
            "invalid raw A1 refuses before actual publication boundary"
        );
        assert_eq!(captures.consumed_requests(), 0);
        session.verify(original.source.reference(), &token).unwrap();
        session
            .close_with_guard(&token, || sql_guard(&original))
            .unwrap();
        assert_unchanged(&original, &sql, &git);
    }
}
#[test]
fn scoped_actions_publication_v3_genuine_attempt_one_invalid_receipt_is_uncertain_not_confirmed() {
    for invalid in invalid_raw_attempts() {
        let wrong = change_attempt(attempt_one(cancelled()), invalid);
        let mut tail = begin_steps();
        tail.extend(guard_steps(attempt_one(running())));
        tail.extend(close_steps(42, true));
        tail.push(post(false));
        tail.push(f::get(
            &format!("repos/{}/actions/runs/123", f::REPO),
            wrong,
        ));
        let original = original_attempt_one(tail);
        let sql = f::sql_state(&original.fixture);
        let git = f::git_state(&original.fixture);
        let token = CancellationToken::default();
        let mut session = original
            .provider
            .begin_repository_catalogue_access(
                original.source.original().source(),
                &limits(),
                &token,
            )
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
        let calls = Cell::new(0);
        let result = original.provider.mutate_run_guarded_v3(
            &measured,
            &mut captures,
            &original.identity,
            &original.target,
            f::REPO,
            RunMutation::Cancel,
            &token,
            deadline,
            |boundary| {
                assert!(Arc::ptr_eq(boundary.original(), &original.target));
                assert_eq!(boundary.fresh().attempt, 1);
                calls.set(calls.get() + 1);
                session.verify(original.source.reference(), &token)?;
                session.close_with_guard(&token, || sql_guard(&original))?;
                Ok(())
            },
        );
        assert!(matches!(
            result,
            Err(ActionsInvocationFailureV3::WriteMayHaveStarted(_))
        ));
        assert_eq!(calls.get(), 1);
        assert_eq!(
            captures.consumed_requests(),
            2,
            "one POST and SAME invalid receipt, never retry"
        );
        assert_unchanged(&original, &sql, &git);
    }
}
