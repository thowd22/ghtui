//! Real original first/list/repo source, strict actual gh children and private
//! SQLite. These are declarations only until Root executes the source cohort.
use super::tests::{bounded, collection, load, row};
use super::*;
use crate::{
    repository_catalogue_read_limits_test_support_v1 as support,
    storage::NotificationRequestedV63 as A,
    test_support::{Fixture, GhStep},
};
use serde_json::json;
const ID: &str = "9007199254740993";
/// Publication owns a mixed read/write script. The central catalogue helper
/// deliberately remains READ-only; every actual child here must instead match
/// this exact task-owned script, including request bytes and response fixture.
fn exact_publication(f: &Fixture, steps: &[GhStep]) {
    let rows = std::fs::read_to_string(f.root.join("gh-fixture/commands.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), steps.len(), "No missing or additional gh child");
    for (index, (actual, expected)) in rows.iter().zip(steps).enumerate() {
        assert_eq!(actual["step"], index);
        assert_eq!(actual["matched"], true);
        assert_eq!(actual["args"], json!(expected.args));
        let stdin = expected
            .stdin
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(actual["stdin_hex"], stdin);
        assert!(actual["pid"].as_u64().is_some_and(|pid| pid > 0));
        let prefix = f.root.join("gh-fixture").join(format!("{index:03}"));
        for (suffix, bytes) in [
            ("args", expected.args.join("\n").into_bytes()),
            ("stdin", expected.stdin.clone()),
            ("stdout", expected.stdout.clone()),
            ("stderr", expected.stderr.clone()),
            ("status", expected.status.to_string().into_bytes()),
        ] {
            assert_eq!(
                std::fs::read(format!("{}.{suffix}", prefix.display())).unwrap(),
                bytes,
                "The strict matched child consumes this exact response fixture"
            );
        }
    }
}
fn thread(unread: bool) -> Value {
    let mut value = row();
    value["unread"] = json!(unread);
    if !unread {
        value["last_read_at"] = json!("2026-10-15T18:01:00Z");
    }
    value
}
fn subscription(ignored: bool) -> Value {
    json!({"subscribed":!ignored,"ignored":ignored,"reason":null,"created_at":"2026-10-15T18:00:00Z","url":format!("https://github.invalid/api/v3/notifications/threads/{ID}/subscription"),"thread_url":format!("https://github.invalid/api/v3/notifications/threads/{ID}")})
}
pub(super) fn pre(action: A, ignored: bool) -> Vec<GhStep> {
    let mut steps = support::principal(42);
    steps.push(support::get("repos/example/repo0000", support::row(0)));
    steps.push(support::get(
        &format!("notifications/threads/{ID}"),
        thread(true),
    ));
    if action != A::MarkRead {
        steps.push(support::get(
            &format!("notifications/threads/{ID}/subscription"),
            subscription(ignored),
        ));
    }
    steps.push(support::get(
        &format!("notifications/threads/{ID}"),
        thread(true),
    ));
    if action != A::MarkRead {
        steps.push(support::get(
            &format!("notifications/threads/{ID}/subscription"),
            subscription(ignored),
        ));
    }
    steps.push(support::get("repos/example/repo0000", support::row(0)));
    steps.extend(support::principal(42));
    steps
}
fn write(action: A) -> GhStep {
    let endpoint = if action == A::MarkRead {
        format!("notifications/threads/{ID}")
    } else {
        format!("notifications/threads/{ID}/subscription")
    };
    let mut args = vec![
        "api",
        endpoint.as_str(),
        "--hostname",
        support::HOST,
        "--method",
        if action == A::MarkRead {
            "PATCH"
        } else {
            "PUT"
        },
        "--include",
    ];
    let raw = if action == A::MarkRead {
        "HTTP/2.0 205 Reset Content\nContent-Length: 0\r\n\r\n".into()
    } else {
        args.extend(["--input", "-"]);
        format!("HTTP/2.0 200 OK\n\r\n{}", subscription(action == A::Ignore))
    };
    let mut step = GhStep::read(&args, &raw);
    if action != A::MarkRead {
        step.stdin = serde_json::to_vec(&json!({"ignored":action==A::Ignore})).unwrap();
    }
    step
}
fn post(action: A, final_uid: u64) -> Vec<GhStep> {
    let mut repo = support::row(0);
    repo["url"] = json!("https://github.invalid/api/v3/repos/example/repo0000");
    vec![
        write(action),
        support::get(
            &if action == A::MarkRead {
                format!("notifications/threads/{ID}")
            } else {
                format!("notifications/threads/{ID}/subscription")
            },
            if action == A::MarkRead {
                thread(false)
            } else {
                subscription(action == A::Ignore)
            },
        ),
        support::get("user", json!({"id":42,"login":"actual"})),
        support::get("repos/example/repo0000", repo),
        support::get("user", json!({"id":final_uid,"login":"actual"})),
    ]
}
fn setup(f: &Fixture, steps: &[GhStep]) -> (Cache, GithubProvider, Arc<ClosedNotificationReadV63>) {
    f.scripted_gh(steps);
    let deadline = Instant::now() + Duration::from_secs(30);
    // Exercise the SAME narrowed selected-operation window as real execute().
    // The initial generic read helper permits1MiB; selected requests are256KiB.
    let provider = bounded(f, deadline).with_notification_read_window_v63(deadline);
    assert_eq!(provider.scoped_read_window, Some((deadline, PAGE)));
    let mut cache = Cache::open(&support::database(f)).unwrap();
    let token = CancellationToken::default();
    let source = load(f, &mut cache, &provider, &token, deadline);
    (cache, provider, source)
}
fn preview(
    provider: &GithubProvider,
    source: &Arc<ClosedNotificationReadV63>,
    cache: &Cache,
    action: A,
) -> Arc<NotificationActionPreviewV63> {
    provider
        .preview_notification_action_v63(
            &source.select_row_v63(0).unwrap(),
            action,
            cache,
            &CancellationToken::default(),
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap()
}
#[test]
fn notifications_publication_v63_actual205_exact_readback_and_original_sql_confirm_retire_old_source(
) {
    let f = Fixture::new();
    let mut steps = collection(false, 42);
    steps.extend(pre(A::MarkRead, false));
    steps.extend(pre(A::MarkRead, false));
    steps.extend(post(A::MarkRead, 42));
    let (mut cache, provider, source) = setup(&f, &steps);
    let p = preview(&provider, &source, &cache, A::MarkRead);
    assert!(Arc::ptr_eq(p.source(), &source));
    assert_eq!(p.requested(), A::MarkRead);
    let token = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let result = provider.publish_notification_action_v63(&p, &mut cache, &token, deadline);
    assert_eq!(result.state(), NotificationPublicationStateV63::Confirmed);
    assert!(result.invoked());
    assert!(result.hide_private());
    let reference = result.reference().unwrap();
    assert!(reference.attempt_id() > 0);
    assert!(source.validate_local(&cache, &token, deadline).is_err());
    let db = rusqlite::Connection::open(support::database(&f)).unwrap();
    let (state, receipt): (String, String) = db
        .query_row(
            "SELECT state,receipt_json FROM notification_write_attempts_v63 WHERE id=?1",
            [reference.attempt_id()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(state, "confirmed");
    let receipt: Value = serde_json::from_str(&receipt).unwrap();
    assert_eq!(receipt["observed_unread"], false);
    assert_eq!(receipt["thread"], ID);
    assert_eq!(receipt["account_id"], 42);
    assert!(!format!("{p:?}").contains("PRIVATE_REAL"));
    assert_eq!(steps.len(), 28);
    exact_publication(&f, &steps);
}
#[test]
fn notifications_publication_v63_ignore_and_unignore_are_exact_thread_state_not_pr_subscription() {
    for action in [A::Ignore, A::Unignore] {
        let f = Fixture::new();
        let old = action == A::Unignore;
        let mut steps = collection(false, 42);
        steps.extend(pre(action, old));
        steps.extend(pre(action, old));
        steps.extend(post(action, 42));
        let (mut cache, provider, source) = setup(&f, &steps);
        let p = preview(&provider, &source, &cache, action);
        let result = provider.publish_notification_action_v63(
            &p,
            &mut cache,
            &CancellationToken::default(),
            Instant::now() + Duration::from_secs(30),
        );
        assert_eq!(result.state(), NotificationPublicationStateV63::Confirmed);
        assert_eq!(steps.len(), 32);
        exact_publication(&f, &steps);
        assert!(steps.iter().all(|s| !s
            .args
            .iter()
            .any(|a| a.contains("graphql") || a.contains("issues/") || a == "DELETE")));
    }
}
#[test]
fn notifications_publication_v63_wrong205_or_wrong_readback_is_protected_unknown_never_resends() {
    for wrong_ack in [true, false] {
        let f = Fixture::new();
        let mut steps = collection(false, 42);
        steps.extend(pre(A::MarkRead, false));
        steps.extend(pre(A::MarkRead, false));
        if wrong_ack {
            let mut ack = write(A::MarkRead);
            ack.stdout = b"HTTP/2 200 OK\n\n".to_vec();
            steps.push(ack);
        } else {
            steps.push(write(A::MarkRead));
            steps.push(support::get(
                &format!("notifications/threads/{ID}"),
                thread(true),
            ));
        }
        let (mut cache, provider, source) = setup(&f, &steps);
        let p = preview(&provider, &source, &cache, A::MarkRead);
        let result = provider.publish_notification_action_v63(
            &p,
            &mut cache,
            &CancellationToken::default(),
            Instant::now() + Duration::from_secs(30),
        );
        assert_eq!(result.state(), NotificationPublicationStateV63::Unknown);
        assert!(result.reference().is_some());
        assert!(result.hide_private());
        let connection = rusqlite::Connection::open(support::database(&f)).unwrap();
        let (state, count): (String, i64) = connection
            .query_row(
                "SELECT state,COUNT(*) FROM notification_write_attempts_v63",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(state, "uncertain");
        assert_eq!(count, 1);
        exact_publication(&f, &steps);
    }
}
#[test]
fn notifications_publication_v63_final_uid_loss_keeps_actual_confirmed_receipt_but_hides_private() {
    let f = Fixture::new();
    let mut steps = collection(false, 42);
    steps.extend(pre(A::MarkRead, false));
    steps.extend(pre(A::MarkRead, false));
    steps.extend(post(A::MarkRead, 43));
    let (mut cache, provider, source) = setup(&f, &steps);
    let p = preview(&provider, &source, &cache, A::MarkRead);
    let result = provider.publish_notification_action_v63(
        &p,
        &mut cache,
        &CancellationToken::default(),
        Instant::now() + Duration::from_secs(30),
    );
    assert_eq!(
        result.state(),
        NotificationPublicationStateV63::ConfirmedSourceUnavailable
    );
    assert!(result.hide_private());
    let connection = rusqlite::Connection::open(support::database(&f)).unwrap();
    let state: String = connection
        .query_row(
            "SELECT state FROM notification_write_attempts_v63",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(state, "confirmed");
    exact_publication(&f, &steps);
}
#[test]
fn notifications_publication_v63_changed_target_during_confirm_refuses_before_claim_or_write() {
    for field in ["repository", "updated_at"] {
        let f = Fixture::new();
        let mut steps = collection(false, 42);
        steps.extend(pre(A::MarkRead, false));
        steps.extend(support::principal(42));
        steps.push(support::get("repos/example/repo0000", support::row(0)));
        let mut changed = row();
        if field == "repository" {
            changed["repository"]["id"] = json!(20000);
            changed["repository"]["node_id"] = json!("R_NEW");
        } else {
            changed["updated_at"] = json!("2026-10-15T18:02:00Z");
        }
        steps.push(support::get(
            &format!("notifications/threads/{ID}"),
            changed,
        ));
        let (mut cache, provider, source) = setup(&f, &steps);
        let p = preview(&provider, &source, &cache, A::MarkRead);
        let result = provider.publish_notification_action_v63(
            &p,
            &mut cache,
            &CancellationToken::default(),
            Instant::now() + Duration::from_secs(30),
        );
        assert_eq!(result.state(), NotificationPublicationStateV63::NotStarted);
        let db = rusqlite::Connection::open(support::database(&f)).unwrap();
        assert_eq!(
            db.query_row(
                "SELECT COUNT(*) FROM notification_write_attempts_v63",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        support::exact(&f, &steps);
    }
}
#[test]
fn notifications_publication_v63_cancelled_original_phase_and_foreign_runner_start_zero_write_children(
) {
    let f = Fixture::new();
    let mut steps = collection(false, 42);
    steps.extend(pre(A::MarkRead, false));
    let (mut cache, provider, source) = setup(&f, &steps);
    let p = preview(&provider, &source, &cache, A::MarkRead);
    let token = CancellationToken::default();
    token.cancel();
    let result = provider.publish_notification_action_v63(
        &p,
        &mut cache,
        &token,
        Instant::now() + Duration::from_secs(30),
    );
    assert_eq!(result.state(), NotificationPublicationStateV63::NotStarted);
    // Retain the selected cap refusal independently of issuer rejection. A
    // same-runner clone with a wider bound is NOT an accepted selected phase.
    let wider = provider
        .clone()
        .with_scoped_read_window(provider.scoped_read_window.unwrap().0, 1024 * 1024)
        .unwrap();
    assert!(source.original.matches_issuer(&wider));
    assert_eq!(
        wider
            .publish_notification_action_v63(
                &p,
                &mut cache,
                &CancellationToken::default(),
                Instant::now() + Duration::from_secs(30),
            )
            .state(),
        NotificationPublicationStateV63::NotStarted
    );
    let foreign = Fixture::new();
    foreign.scripted_gh(&[]);
    let original_deadline = Instant::now() + Duration::from_secs(30);
    let other =
        bounded(&foreign, original_deadline).with_notification_read_window_v63(original_deadline);
    assert!(!source.original.matches_issuer(&other));
    let result = other.publish_notification_action_v63(
        &p,
        &mut cache,
        &CancellationToken::default(),
        Instant::now() + Duration::from_secs(30),
    );
    assert_eq!(result.state(), NotificationPublicationStateV63::NotStarted);
    support::exact(&foreign, &[]);
    support::exact(&f, &steps);
}
#[test]
fn notifications_publication_v63_actual_failed_final_prewrite_uid_keeps_pending_and_starts_zero_post(
) {
    let f = Fixture::new();
    let mut steps = collection(false, 42);
    steps.extend(pre(A::MarkRead, false));
    let mut confirm = pre(A::MarkRead, false);
    *confirm.last_mut().unwrap() = support::get("user", json!({"id":43,"login":"different"}));
    steps.extend(confirm);
    let (mut cache, provider, source) = setup(&f, &steps);
    let p = preview(&provider, &source, &cache, A::MarkRead);
    let result = provider.publish_notification_action_v63(
        &p,
        &mut cache,
        &CancellationToken::default(),
        Instant::now() + Duration::from_secs(30),
    );
    assert_eq!(result.state(), NotificationPublicationStateV63::NotStarted);
    assert!(!result.invoked());
    assert!(result.hide_private());
    assert!(result.reference().is_some());
    let db = rusqlite::Connection::open(support::database(&f)).unwrap();
    let (state, receipt): (String, Option<String>) = db
        .query_row(
            "SELECT state,receipt_json FROM notification_write_attempts_v63",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(state, "pending");
    assert!(receipt.is_none());
    assert_eq!(steps.len(), 23);
    support::exact(&f, &steps);
}
