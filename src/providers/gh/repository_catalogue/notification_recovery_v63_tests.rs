//! Actual Pending SQL and current original Core; real owned writer locks, not
//! manufactured source/runner state. All declarations await Root execution.
use super::*;
use super::{
    publication_tests::pre,
    tests::{bounded, collection, expected},
};
use crate::{
    providers::runner::{CommandSpec, FailureKind},
    repository_catalogue_read_limits_test_support_v1 as support,
    storage::{NotificationRequestedV63 as A, NotificationWriteMarkerV63},
    test_support::{Fixture, GhStep},
};
fn setup(
    f: &Fixture,
    steps: &[GhStep],
) -> (
    Cache,
    GithubProvider,
    Arc<ClosedNotificationReadV63>,
    Arc<NotificationWriteMarkerV63>,
) {
    f.scripted_gh(steps);
    let deadline = Instant::now() + Duration::from_secs(30);
    // Same issuer, original clock and production selected-operation cap.
    let provider = bounded(f, deadline).with_notification_read_window_v63(deadline);
    assert_eq!(provider.scoped_read_window, Some((deadline, PAGE)));
    let mut cache = Cache::open(&support::database(f)).unwrap();
    let token = CancellationToken::default();
    let phase = provider
        .begin_notification_read_v63(
            &NotificationQueryV63::default(),
            &expected(),
            &mut cache,
            None,
            &token,
            deadline,
        )
        .unwrap();
    let origin = phase.write_origin_v63(0).unwrap();
    let permit = cache
        .begin_notification_write_v63(
            &origin,
            phase.local_witness_v63(),
            A::MarkRead,
            &token,
            deadline,
        )
        .unwrap();
    let mut markers = cache
        .notification_attempts_v63(&origin, phase.local_witness_v63(), &token, deadline)
        .unwrap();
    assert_eq!(markers.len(), 1);
    assert_eq!(markers[0].reference(), permit.marker().reference());
    let marker = Arc::new(markers.remove(0));
    let source = phase.finish(&cache, &token).unwrap();
    (cache, provider, source, marker)
}
fn script() -> Vec<GhStep> {
    let mut steps = collection(false, 42);
    steps.extend(pre(A::MarkRead, false));
    steps
}
fn inspect(
    provider: &GithubProvider,
    cache: &Cache,
    source: &Arc<ClosedNotificationReadV63>,
    marker: &Arc<NotificationWriteMarkerV63>,
    token: &CancellationToken,
) -> Arc<NotificationInspectionV63> {
    provider
        .inspect_notification_attempt_v63(
            &source.select_row_v63(0).unwrap(),
            marker.clone(),
            cache,
            token,
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap()
}
fn state(f: &Fixture) -> (String, String, String, u64) {
    rusqlite::Connection::open(support::database(f)).unwrap().query_row("SELECT state,intent_digest,origin_json,generation FROM notification_write_attempts_v63",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap()
}
fn held(
    f: &Fixture,
    provider: &GithubProvider,
    inspection: &NotificationInspectionV63,
    token: &CancellationToken,
) -> std::thread::JoinHandle<
    Result<crate::providers::runner::CommandOutput, crate::providers::runner::CommandFailure>,
> {
    let resource = inspection
        .runner_gate_v63(&provider.runner, token, inspection.deadline_v63())
        .unwrap()
        .0
        .runner_resource_v63();
    let runner = provider.runner.clone();
    let marker = f.root.join("owned-notification-writer-live");
    let path = marker.clone();
    let child = std::thread::spawn(move || {
        runner.run(
            CommandSpec::new("/bin/sh")
                .args([
                    "-c",
                    "printf live > \"$1\"; exec /bin/sleep 0.25",
                    "owned-notification-test",
                    path.to_str().unwrap(),
                ])
                .write(resource)
                .timeout(Duration::from_secs(2)),
            &CancellationToken::default(),
        )
    });
    let end = Instant::now() + Duration::from_secs(2);
    while !marker.exists() {
        assert!(
            Instant::now() < end,
            "actual owned writer must begin before bounded inspection"
        );
        std::thread::yield_now();
    }
    child
}
#[test]
fn notifications_recovery_v63_fresh_actual_inspect_then_explicit_local_ack_no_remote_resend() {
    let f = Fixture::new();
    let steps = script();
    let (mut cache, provider, source, marker) = setup(&f, &steps);
    let before = state(&f);
    let token = CancellationToken::default();
    let observation = inspect(&provider, &cache, &source, &marker, &token);
    let journal = std::fs::read(f.root.join("gh-fixture/commands.jsonl")).unwrap();
    assert_eq!(
        state(&f),
        before,
        "Inspect is readonly and never acknowledges"
    );
    cache
        .acknowledge_notification_inspection_v63(
            &provider,
            &observation,
            &token,
            observation.deadline_v63(),
        )
        .unwrap();
    let after = state(&f);
    assert_eq!(after.0, "acknowledged");
    assert_eq!(
        (&after.1, &after.2, after.3),
        (&before.1, &before.2, before.3)
    );
    assert_eq!(
        std::fs::read(f.root.join("gh-fixture/commands.jsonl")).unwrap(),
        journal
    );
    assert_eq!(steps.len(), 15);
    support::exact(&f, &steps);
}
#[test]
fn notifications_recovery_v63_actual_newer_owned_writer_generation_refuses_old_ack() {
    let f = Fixture::new();
    let steps = script();
    let (mut cache, provider, source, marker) = setup(&f, &steps);
    let before = state(&f);
    let token = CancellationToken::default();
    let observation = inspect(&provider, &cache, &source, &marker, &token);
    assert!(held(&f, &provider, &observation, &token)
        .join()
        .unwrap()
        .is_ok());
    assert!(cache
        .acknowledge_notification_inspection_v63(
            &provider,
            &observation,
            &token,
            observation.deadline_v63()
        )
        .is_err());
    assert_eq!(state(&f), before);
    support::exact(&f, &steps);
}
#[test]
fn notifications_recovery_v63_actual_held_writer_original_short_deadline_cannot_block_or_ack() {
    let f = Fixture::new();
    let steps = script();
    let (cache, provider, source, marker) = setup(&f, &steps);
    let token = CancellationToken::default();
    let observation = inspect(&provider, &cache, &source, &marker, &token);
    let child = held(&f, &provider, &observation, &token);
    let target = observation
        .runner_gate_v63(&provider.runner, &token, observation.deadline_v63())
        .unwrap()
        .0;
    let start = Instant::now();
    let error = provider
        .runner
        .notification_inspection_generation_v63(target, &token, start + Duration::from_millis(20))
        .unwrap_err();
    assert_eq!(error.kind, FailureKind::Timeout);
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(child.join().unwrap().is_ok());
    assert_eq!(state(&f).0, "pending");
    support::exact(&f, &steps);
}
#[test]
fn notifications_recovery_v63_actual_held_writer_cancel_cannot_release_original_barrier() {
    let f = Fixture::new();
    let steps = script();
    let (mut cache, provider, source, marker) = setup(&f, &steps);
    let token = CancellationToken::default();
    let observation = inspect(&provider, &cache, &source, &marker, &token);
    let child = held(&f, &provider, &observation, &token);
    token.cancel();
    let start = Instant::now();
    assert!(cache
        .acknowledge_notification_inspection_v63(
            &provider,
            &observation,
            &token,
            observation.deadline_v63()
        )
        .is_err());
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(child.join().unwrap().is_ok());
    assert_eq!(state(&f).0, "pending");
    support::exact(&f, &steps);
}
#[test]
fn notifications_recovery_v63_foreign_runner_sibling_token_and_newer_exact_sql_generation_refuse() {
    let f = Fixture::new();
    let steps = script();
    let (mut cache, provider, source, marker) = setup(&f, &steps);
    let token = CancellationToken::default();
    let observation = inspect(&provider, &cache, &source, &marker, &token);
    let foreign = Fixture::new();
    foreign.scripted_gh(&[]);
    let other = bounded(&foreign, observation.deadline_v63());
    assert!(cache
        .acknowledge_notification_inspection_v63(
            &other,
            &observation,
            &token,
            observation.deadline_v63()
        )
        .is_err());
    assert!(cache
        .acknowledge_notification_inspection_v63(
            &provider,
            &observation,
            &CancellationToken::default(),
            observation.deadline_v63()
        )
        .is_err());
    rusqlite::Connection::open(support::database(&f))
        .unwrap()
        .execute(
            "UPDATE notification_write_attempts_v63 SET generation=generation+1",
            [],
        )
        .unwrap();
    assert!(cache
        .acknowledge_notification_inspection_v63(
            &provider,
            &observation,
            &token,
            observation.deadline_v63()
        )
        .is_err());
    assert_eq!(state(&f).0, "pending");
    support::exact(&foreign, &[]);
    support::exact(&f, &steps);
}
#[test]
fn notifications_recovery_v63_real_sql_failure_after_runner_reconciliation_retains_durable_blocker()
{
    let f = Fixture::new();
    let steps = script();
    let (mut cache, provider, source, marker) = setup(&f, &steps);
    let before = state(&f);
    let token = CancellationToken::default();
    let observation = inspect(&provider, &cache, &source, &marker, &token);
    rusqlite::Connection::open(support::database(&f)).unwrap().execute_batch("CREATE TRIGGER own_notification_ack_failure BEFORE UPDATE ON notification_write_attempts_v63 WHEN NEW.state='acknowledged' BEGIN SELECT RAISE(FAIL,'actual local ACK failure'); END;").unwrap();
    assert!(cache
        .acknowledge_notification_inspection_v63(
            &provider,
            &observation,
            &token,
            observation.deadline_v63()
        )
        .is_err());
    assert_eq!(state(&f), before);
    let (origin, _, witness) = observation
        .local_gate_v63(&provider, &token, observation.deadline_v63())
        .unwrap();
    assert!(cache.begin_notification_write_v63(origin,witness,A::MarkRead,&token,observation.deadline_v63()).is_err(),"runner-only reconciliation cannot enable a new remote attempt while exact SQL blocker remains");
    support::exact(&f, &steps);
}
