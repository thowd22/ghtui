//! SOURCE ONLY / UNRUN. SAME pinned HTTP grammar + actual gh/Core/Run factories.
use super::*;
use crate::{
    scoped_actions_test_support_v1 as f,
    scoped_actions_v1::{ScopedActionsRequestKindV1 as K, ScopedActionsSourceV1},
    test_support::{Fixture, GhStep},
};
use serde_json::{json, Value};
use std::time::{Duration, Instant};
fn valid(bytes: &[u8], expected: &str) -> bool {
    super::super::empty_http_ack_v5::validate(bytes, expected).is_ok()
}
#[test]
fn scoped_actions_control_v5_force202_exact_pinned_lf_crlf_framing() {
    for bytes in [
        b"HTTP/2.0 202 Accepted\n\n".as_slice(),
        b"HTTP/2.0 202 Accepted\nContent-Length: 0\r\nX-Request-Id: actual\r\n\r\n".as_slice(),
        b"HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\n\r\n".as_slice(),
    ] {
        assert!(valid(bytes, "202"));
    }
}
#[test]
fn scoped_actions_control_v5_force202_never_accepts_delete204_or_success200() {
    for code in ["200", "201", "204", "302", "404"] {
        assert!(!valid(
            format!("HTTP/2.0 {code} Other\n\n").as_bytes(),
            "202"
        ));
    }
}
#[test]
fn scoped_actions_control_v5_delete204_never_accepts_force202_or_null_payload() {
    for bytes in [b"HTTP/2.0 202 Accepted\n\n".as_slice(), b"null", b"{}", b""] {
        assert!(!valid(bytes, "204"));
    }
}
#[test]
fn scoped_actions_control_v5_ack_rejects_continue_preface_or_concatenated_messages() {
    for expected in ["202", "204"] {
        let bytes = format!("HTTP/1.1 100 Continue\r\n\r\nHTTP/2.0 {expected} OK\n\n");
        assert!(!valid(bytes.as_bytes(), expected));
    }
}
#[test]
fn scoped_actions_control_v5_ack_rejects_nonempty_missing_boundary_and_trailing_content() {
    for expected in ["202", "204"] {
        for suffix in ["\n", "\n\n{}", "\n\nnull", "\nContent-Length: 1\n\nx"] {
            assert!(!valid(
                format!("HTTP/2.0 {expected} OK{suffix}").as_bytes(),
                expected
            ));
        }
    }
}
#[test]
fn scoped_actions_control_v5_ack_rejects_controls_duplicate_lengths_transfer_and_mixed_lines() {
    for expected in ["202", "204"] {
        for headers in [
            "Content-Length: 0\nContent-Length: 0\n\n",
            "Transfer-Encoding: chunked\n\n",
            "X-Private: \u{1b}[2J\n\n",
            "X-Bidi: \u{202e}\n\n",
            "X-Good: ok\r\nX-Bad: mixed\n\n",
        ] {
            assert!(!valid(
                format!("HTTP/2.0 {expected} OK\n{headers}").as_bytes(),
                expected
            ));
        }
    }
}
#[test]
fn scoped_actions_control_v5_ack_enforces_actual_status_header_count_line_and_total_caps() {
    assert!(!valid(&vec![b'x'; 16 * 1024 + 1], "202"));
    assert!(!valid(
        format!("HTTP/2.0 204 OK\n{}\n", "X-Test: x\n".repeat(129)).as_bytes(),
        "204"
    ));
    assert!(!valid(
        format!("HTTP/2.0 202 OK\nX-Long: {}\n\n", "x".repeat(4097)).as_bytes(),
        "202"
    ));
    assert!(!valid(b"HTTP/2.0 204 OK\nX-Test: \xff\n\n", "204"));
}
#[test]
fn scoped_actions_control_v5_existing_review204_exact_pinned_examples_stay_unchanged() {
    for bytes in [
        b"HTTP/2.0 204 No Content\n\r\n".as_slice(),
        b"HTTP/1.1 204 No Content\nDate: Thu, 15 Oct 2026 09:00:00 GMT\r\n\r\n".as_slice(),
    ] {
        assert!(valid(bytes, "204"));
        assert!(super::super::review_comment_delete_v2::exact_204_for_test_v107(bytes).is_ok());
    }
}
struct Case {
    fixture: Fixture,
    p: GithubProvider,
    source: Arc<ScopedActionsSourceV1>,
    run: Arc<ScopedActionsRunTargetV1>,
    steps: Vec<GhStep>,
}
fn command(action: RunMutation, body: &[u8]) -> GhStep {
    let base = format!("repos/{}/actions/runs/123", f::REPO);
    let (path, method) = match action {
        RunMutation::ForceCancel => (format!("{base}/force-cancel"), "POST"),
        RunMutation::DeleteRun => (base, "DELETE"),
        RunMutation::DeleteLogs => (format!("{base}/logs"), "DELETE"),
        _ => panic!("Closed three controls"),
    };
    let mut value = GhStep::read(
        &[
            "api",
            &path,
            "--hostname",
            f::HOST,
            "--method",
            method,
            "--include",
        ],
        "",
    );
    value.stdout = body.to_vec();
    value
}
fn original(action: RunMutation, ack: Option<GhStep>) -> Case {
    let mut steps = f::operation(K::Attempt, 42);
    if action == RunMutation::ForceCancel {
        for step in &mut steps {
            if step
                .args
                .get(1)
                .is_some_and(|path| path.ends_with("/runs/123") || path.ends_with("/attempts/2"))
            {
                let mut raw = f::run();
                raw["status"] = json!("in_progress");
                raw["conclusion"] = Value::Null;
                step.stdout = raw.to_string().into_bytes();
            }
        }
    }
    steps.extend(f::principal(42));
    steps.push(f::get(&format!("repos/{}", f::REPO), f::repo()));
    steps.push(f::get(&format!("repos/{}", f::REPO), f::repo()));
    steps.extend(f::principal(42));
    if let Some(step) = ack {
        steps.push(step);
    }
    let (fixture, p, mut app, all) = f::setup(steps);
    let value = f::read(
        &mut app,
        &p,
        &fixture,
        crate::actions_workspace::ActionsAction::OpenRun {
            repository: f::REPO.into(),
            id: 123,
            attempt: 2,
        },
    )
    .unwrap();
    Case {
        fixture,
        p,
        source: value.page.source().clone(),
        run: value.page.run().unwrap().clone(),
        steps: all,
    }
}
fn closed(case: &Case) -> (PostWriteCaptureAllowanceV2, CancellationToken) {
    let cancel = CancellationToken::default();
    let mut limits = RepositoryCatalogueAccessLimits::default();
    limits.transport.max_response_bytes = 2 * 1024 * 1024;
    limits.transport.max_document_bytes = 64 * 1024;
    limits.transport.max_duration = Duration::from_secs(30);
    limits.max_repository_reads = 2;
    let mut session = case
        .p
        .begin_repository_catalogue_access(case.source.original().source(), &limits, &cancel)
        .unwrap();
    session.verify(case.source.reference(), &cancel).unwrap();
    let captures = session
        .reserve_postwrite_capture_v2(&[16 * 1024], 1, &cancel)
        .unwrap();
    let window = session
        .reserve_measured_external_reads(1, 16 * 1024, 64 * 1024, 1, &cancel)
        .unwrap();
    let _measured = case.p.clone().with_measured_read_window(window).unwrap();
    session.verify(case.source.reference(), &cancel).unwrap();
    session
        .close_with_guard(&cancel, || {
            crate::storage::Cache::open(&f::database(&case.fixture))
                .unwrap()
                .validate_repository_catalogue_intent_source(case.source.original())
                .map_err(|_| {
                    error(
                        GithubErrorKind::IdentityChanged,
                        "Original test SQL changed",
                    )
                })
        })
        .unwrap();
    (captures, cancel)
}
#[test]
fn scoped_actions_control_v5_actual_delete_pool_is_carved_after_prepaid_and_closing_capacity_not_guaranteed_one_mib(
) {
    let case = original(RunMutation::DeleteRun, None);
    let cancel = CancellationToken::default();
    let mut limits = RepositoryCatalogueAccessLimits::default();
    limits.transport.max_response_bytes = 2 * 1024 * 1024;
    limits.transport.max_document_bytes = 64 * 1024;
    limits.max_repository_reads = 2;
    let mut session = case
        .p
        .begin_repository_catalogue_access(case.source.original().source(), &limits, &cancel)
        .unwrap();
    let caps = [16 * 1024, 512 * 1024, 64 * 1024, 64 * 1024];
    let cursor = session
        .reserve_postwrite_capture_v2(&caps, 2, &cancel)
        .unwrap();
    let window = session
        .reserve_measured_external_reads(15, 512 * 1024, 1024 * 1024, 2, &cancel)
        .unwrap();
    assert_eq!(cursor.reserved_requests(), 4);
    assert_eq!(cursor.reserved_bytes(), caps.iter().sum::<usize>());
    assert_eq!(window.max_requests(), 15);
    assert!(window.capacity_bytes() > 0 && window.capacity_bytes() <= 1024 * 1024);
    assert!(window.max_output_bytes() <= 512 * 1024);
    // The actual remaining pool is already carved after fixed postwrite and
    // original repository/principal reservations, not a fresh 1MiB promise.
    assert!(window.capacity_bytes() + cursor.reserved_bytes() <= 2 * 1024 * 1024);
    assert!(cursor.deadline() <= window.deadline());
    let measured = case.p.clone().with_measured_read_window(window).unwrap();
    session.verify(case.source.reference(), &cancel).unwrap();
    session.verify(case.source.reference(), &cancel).unwrap();
    session
        .close_with_guard(&cancel, || {
            case.source.validate_local(&f::database(&case.fixture))
        })
        .unwrap();
    assert_eq!(cursor.consumed_requests(), 0);
    drop(measured);
    exact(&case);
}
fn exact(case: &Case) {
    let rows =
        std::fs::read_to_string(case.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    assert_eq!(
        rows.lines().count(),
        case.steps.len(),
        "Exact actual original children; no extra write/read/retry"
    );
    for (line, expected) in rows.lines().zip(&case.steps) {
        let row: Value = serde_json::from_str(line).unwrap();
        assert_eq!(row["args"], json!(expected.args));
        assert_eq!(row["matched"], true);
        assert_eq!(row["stdin_hex"], "");
    }
}
#[test]
fn scoped_actions_control_v5_force_actual_core_cursor202_binds_same_original_once() {
    let action = RunMutation::ForceCancel;
    let case = original(action, Some(command(action, b"HTTP/2.0 202 Accepted\n\n")));
    let sql = f::sql_state(&case.fixture);
    let git = f::git_state(&case.fixture);
    let (mut slots, cancel) = closed(&case);
    let ack = slots
        .scoped_run_empty_ack_v5(&case.p, &case.run, action, &cancel)
        .unwrap();
    assert!(Arc::ptr_eq(ack.original(), &case.run));
    assert_eq!(ack.action(), action);
    assert_eq!(ack.status(), 202);
    assert_eq!(slots.consumed_requests(), 1);
    assert!(
        slots
            .scoped_run_empty_ack_v5(&case.p, &case.run, action, &cancel)
            .is_err(),
        "One slot cannot replay"
    );
    assert_eq!(f::sql_state(&case.fixture), sql);
    assert_eq!(f::git_state(&case.fixture), git);
    exact(&case);
}
#[test]
fn scoped_actions_control_v5_deletes_actual_core_cursor204_preserves_distinct_fixed_endpoints() {
    for action in [RunMutation::DeleteRun, RunMutation::DeleteLogs] {
        let case = original(
            action,
            Some(command(
                action,
                b"HTTP/2.0 204 No Content\nContent-Length: 0\r\n\r\n",
            )),
        );
        let sql = f::sql_state(&case.fixture);
        let git = f::git_state(&case.fixture);
        let (mut slots, cancel) = closed(&case);
        let ack = slots
            .scoped_run_empty_ack_v5(&case.p, &case.run, action, &cancel)
            .unwrap();
        assert!(Arc::ptr_eq(ack.original(), &case.run));
        assert_eq!(ack.action(), action);
        assert_eq!(ack.status(), 204);
        assert_eq!(slots.consumed_requests(), 1);
        assert_eq!(f::sql_state(&case.fixture), sql);
        assert_eq!(f::git_state(&case.fixture), git);
        exact(&case);
    }
}
#[test]
fn scoped_actions_control_v5_ack_private_body_after_actual_post_is_error_not_receipt() {
    let action = RunMutation::ForceCancel;
    let case = original(
        action,
        Some(command(action, b"HTTP/2.0 202 Accepted\n\nPRIVATE_BODY")),
    );
    let (mut slots, cancel) = closed(&case);
    assert!(slots
        .scoped_run_empty_ack_v5(&case.p, &case.run, action, &cancel)
        .is_err());
    assert_eq!(slots.consumed_requests(), 1);
    exact(&case);
}
#[test]
fn scoped_actions_control_v5_cancelled_expired_foreign_token_cannot_spend_or_invoke() {
    for mode in 0..3 {
        let action = RunMutation::DeleteLogs;
        let case = original(action, None);
        let sql = f::sql_state(&case.fixture);
        let git = f::git_state(&case.fixture);
        let (mut slots, cancel) = closed(&case);
        let other = CancellationToken::default();
        if mode == 0 {
            cancel.cancel();
        } else if mode == 1 {
            slots.deadline = Instant::now() - Duration::from_millis(1);
        }
        assert!(slots
            .scoped_run_empty_ack_v5(
                &case.p,
                &case.run,
                action,
                if mode == 2 { &other } else { &cancel }
            )
            .is_err());
        assert_eq!(slots.consumed_requests(), 0);
        assert_eq!(f::sql_state(&case.fixture), sql);
        assert_eq!(f::git_state(&case.fixture), git);
        exact(&case);
    }
}
#[test]
fn scoped_actions_control_v5_foreign_actual_runner_or_unapproved_action_has_zero_effect() {
    let action = RunMutation::DeleteRun;
    let case = original(action, None);
    let (mut slots, cancel) = closed(&case);
    let other = f::provider(&case.fixture);
    assert!(slots
        .scoped_run_empty_ack_v5(&other, &case.run, action, &cancel)
        .is_err());
    assert!(slots
        .scoped_run_empty_ack_v5(
            &case.p,
            &case.run,
            RunMutation::RerunAll { debug: false },
            &cancel
        )
        .is_err());
    assert_eq!(slots.consumed_requests(), 0);
    exact(&case);
}
