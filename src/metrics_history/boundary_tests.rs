use super::*;

#[test]
fn metrics_history_worker_actual_pending_publication_between_phases_rejects_without_remote_resend()
{
    use crate::pr_write_journal::{PrWriteIntent, PrWriteJournal, PrWriteTarget};
    use std::collections::BTreeMap;
    let fixture = Fixture::new();
    let mut protocol = inventory(false, false);
    protocol.extend(load_steps(false));
    fixture.scripted_gh(&protocol);
    let request = request(&fixture, false, false);
    let database = request.database.clone();
    let original = request.workspace.observations[0].clone();
    let pending = load_history(
        request,
        &identity(42),
        &provider(&fixture),
        &CancellationToken::default(),
    )
    .unwrap();
    let mut journal_writer = PrWriteJournal::open(&database).unwrap();
    let target = PrWriteTarget::for_source(
        &Scope::github("github.invalid", Some(42)).unwrap(),
        &original.repository.full_name,
        &original.node_id,
        original.number,
    )
    .unwrap();
    let permit = journal_writer
        .begin(
            &target,
            &PrWriteIntent {
                command: "new-comment".into(),
                selected_resource: "pr".into(),
                observed_head: original.head_sha.as_ref().unwrap().clone(),
                values: BTreeMap::from([(
                    "body".into(),
                    "Prepared only: never invoked or confirmed".into(),
                )]),
            },
        )
        .unwrap();
    drop(permit);
    assert!(pending
        .publish(
            &identity(42),
            &provider(&fixture),
            &CancellationToken::default()
        )
        .is_err());
    let sql = rusqlite::Connection::open(&database).unwrap();
    let (attempts,pending_count,confirmed):(u64,u64,u64) = sql.query_row(
        "SELECT COUNT(*),COALESCE(SUM(state='pending'),0),COALESCE(SUM(confirmed_json IS NOT NULL),0) FROM pr_write_attempts",
        [], |row|Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
    ).unwrap();
    assert_eq!((attempts, pending_count, confirmed), (1, 1, 0));
    assert_eq!(count_records(&database, "metrics-state-v1", 42), 2);
    journal(&fixture, 16);
}

#[test]
fn metrics_history_worker_unsupported_observation_retains_known_facts_without_first_or_latest_invention(
) {
    let fixture = Fixture::new();
    let mut protocol = inventory(false, false);
    protocol.extend(pre(1));
    let mut traversal = backfill(1);
    let mut reviews: Value = serde_json::from_slice(&traversal[3].stdout).unwrap();
    reviews[0]["submitted_at"] = Value::Null;
    traversal[3] = get(
        "repos/example/repo/pulls/1/reviews?per_page=100&page=1",
        reviews,
    );
    let mut events: Value = serde_json::from_slice(&traversal[4].stdout).unwrap();
    events.as_array_mut().unwrap().push(json!({"id":901,"node_id":"UnknownFutureReviewReset","event":"future_review_reset","created_at":"2026-10-03T00:00:00Z","actor":null}));
    traversal[4] = get(
        "repos/example/repo/issues/1/timeline?per_page=100&page=1",
        events,
    );
    protocol.extend(traversal);
    protocol.extend(publication(false, false));
    fixture.scripted_gh(&protocol);
    let request = request(&fixture, false, false);
    let database = request.database.clone();
    let pending = load_history(
        request,
        &identity(42),
        &provider(&fixture),
        &CancellationToken::default(),
    )
    .unwrap();
    let prepared = pending
        .publish(
            &identity(42),
            &provider(&fixture),
            &CancellationToken::default(),
        )
        .unwrap();
    for kind in [MetricKind::FirstReview, MetricKind::ReviewWait] {
        let card = prepared
            .snapshot
            .cards
            .iter()
            .find(|card| card.kind == kind)
            .unwrap();
        assert_eq!(card.value, MetricValue::Unavailable);
        assert_eq!(card.unknown, 1);
        assert!(card.contributors[0].evidence.is_empty());
    }
    assert_eq!(
        count_records(&database, "metrics-event-v1", 42),
        1,
        "Actual request fact survives partial unknown source"
    );
    assert!(prepared.evidence.prs[0]
        .timeline
        .events
        .iter()
        .any(|event| event.id == 900));
    assert!(prepared.evidence.prs[0].reviews.events.is_empty());
    assert!(
        !prepared.evidence.prs[0].timeline.complete && !prepared.evidence.prs[0].reviews.complete
    );
    assert_eq!(prepared.usage.raw_rows, 3);
    journal(&fixture, 21);
}

#[test]
fn metrics_history_worker_duplicate_weak_replaced_or_foreign_selection_is_not_a_transport_target() {
    for invalid in 0..4 {
        let fixture = Fixture::new();
        fixture.scripted_gh(&inventory(false, false));
        let mut request = request(&fixture, false, false);
        let database = request.database.clone();
        match invalid {
            0 => request
                .selected_parents
                .push(request.selected_parents[0].clone()),
            1 => request.selected_parents[0].database_id = None,
            2 => request.selected_parents[0].node_id = Some("OtherGenuineParentNotLoaded".into()),
            _ => request.selected_parents[0].repository = "example/not-declared".into(),
        }
        assert!(load_history(
            request,
            &identity(42),
            &provider(&fixture),
            &CancellationToken::default()
        )
        .is_err());
        assert_eq!(count_records(&database, "metrics-reservation-v1", 42), 0);
        journal(&fixture, 3);
    }
}
