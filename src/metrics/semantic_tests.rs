//! Source-only closed request-target facet contracts. Register with the reviewed
//! semantic/history compatibility activation; these are formula tests, not API proof.
use super::*;
use crate::providers::gh::{Actor, HostIdentity};

fn event(id: u64, at: u64, kind: EventKind) -> SourceEvent {
    SourceEvent {
        id,
        at,
        kind,
        actor: MetricActor {
            account_id: None,
            actor: Actor::default(),
        },
    }
}
fn source(open_events: Vec<SourceEvent>, merged_events: Vec<SourceEvent>) -> DashboardEvidence {
    let scope = MetricsScope::new(
        &HostIdentity {
            host: "github.invalid".into(),
            account_id: Some(42),
            login: "viewer".into(),
            gh_version: "fixture".into(),
        },
        &["example/repo".into()],
    )
    .unwrap();
    let prs = [open_events, merged_events]
        .into_iter()
        .enumerate()
        .map(|(index, events)| {
            let number = index as u64 + 1;
            let target = PrTarget {
                repository: "example/repo".into(),
                number,
                repository_id: Some(99),
                database_id: Some(100 + number),
                node_id: Some(format!("PR_{number}")),
                source_record_id: None,
            };
            let timeline = EventCollection {
                binding: Some(
                    EventBinding::for_parent(&scope, target.clone(), EventCategory::Timeline)
                        .unwrap(),
                ),
                events,
                from: Some(100),
                through: Some(1000),
                complete: true,
                basis: HistoryBasis::ApiVisibleRetainedBackfill {
                    limitations: vec![],
                },
                source: format!("actual-timeline-source-{number}"),
                generation: 1,
                reasons: vec![],
            };
            PrEvidence {
                target,
                cohort_member: true,
                title: format!("Canonical PR {number}"),
                author: MetricActor {
                    account_id: None,
                    actor: Actor::default(),
                },
                state: if index == 0 {
                    PrState::Open
                } else {
                    PrState::Merged
                },
                draft: Some(false),
                head_sha: Some("a".repeat(40)),
                created_at: Some(100),
                review_requested_now: Some(index == 0),
                timeline,
                reviews: EventCollection::unavailable(),
            }
        })
        .collect();
    DashboardEvidence {
        scope,
        cohort: PrCohortKind::SelectedRepositoryInventory,
        prs,
        pr_coverage: EvidenceCoverage {
            state: CoverageState::Complete,
            complete_empty: false,
            fetched_at: Some(1000),
        },
        checks: vec![],
        check_coverage: EvidenceCoverage::unavailable("No checks"),
        runs: vec![],
        run_coverage: EvidenceCoverage::unavailable("No runs"),
    }
}
fn card(source: &DashboardEvidence, kind: MetricKind) -> MetricCard {
    dashboard(source, 1000, UtcWindow::new(0, 1000).unwrap())
        .unwrap()
        .cards
        .into_iter()
        .find(|card| card.kind == kind)
        .unwrap()
}

#[test]
fn metrics_semantic_known_team_id_equal_viewer_never_overrides_actual_direct_request() {
    let source = source(
        vec![
            event(900, 500, EventKind::ReviewRequested { reviewer_id: 42 }),
            event(901, 900, EventKind::ReviewRequestedTeam { team_id: 42 }),
            event(
                902,
                950,
                EventKind::ReviewRequestRemovedTeam { team_id: 42 },
            ),
        ],
        vec![event(950, 800, EventKind::Merged)],
    );
    let wait = card(&source, MetricKind::ReviewWait);
    assert_eq!(
        wait.value,
        MetricValue::MeanSeconds {
            sum: 500,
            samples: 1
        }
    );
    assert_eq!(wait.unknown, 0);
    assert!(matches!(
        wait.contributors[0].evidence.as_slice(),
        [MetricTarget::Event { event_id: 900, .. }]
    ));
    assert!(source.prs[0]
        .timeline
        .events
        .iter()
        .any(|event| event.id == 901
            && matches!(event.kind, EventKind::ReviewRequestedTeam { team_id: 42 })));
}

#[test]
fn metrics_semantic_team_only_request_stays_unknown_but_actual_merge_latency_is_usable() {
    let source = source(
        vec![event(
            901,
            500,
            EventKind::ReviewRequestedTeam { team_id: 42 },
        )],
        vec![
            event(951, 500, EventKind::ReviewRequestedTeam { team_id: 42 }),
            event(952, 800, EventKind::Merged),
        ],
    );
    let wait = card(&source, MetricKind::ReviewWait);
    assert_eq!(wait.value, MetricValue::Unavailable);
    assert_eq!(wait.unknown, 1);
    assert!(
        wait.contributors[0].evidence.is_empty(),
        "No team-to-user alias"
    );
    let merge = card(&source, MetricKind::MergeLatency);
    assert_eq!(
        merge.value,
        MetricValue::MeanSeconds {
            sum: 700,
            samples: 1
        }
    );
    assert_eq!(merge.unknown, 0);
    assert!(matches!(
        merge.contributors[1].evidence.as_slice(),
        [MetricTarget::Event { event_id: 952, .. }]
    ));
    assert!(matches!(merge.coverage.state, CoverageState::Partial(_)));
}

#[test]
fn metrics_semantic_unknown_request_target_blocks_wait_not_actual_merge_or_actorless_timestamp() {
    let source = source(
        vec![
            event(900, 500, EventKind::ReviewRequested { reviewer_id: 42 }),
            event(901, 600, EventKind::ReviewRequestRemovedUnknownTarget),
        ],
        vec![
            event(950, 500, EventKind::ReviewRequestedUnknownTarget),
            event(951, 800, EventKind::Merged),
        ],
    );
    let wait = card(&source, MetricKind::ReviewWait);
    assert_eq!(wait.value, MetricValue::Unavailable);
    assert_eq!(wait.unknown, 1);
    let merge = card(&source, MetricKind::MergeLatency);
    assert_eq!(
        merge.value,
        MetricValue::MeanSeconds {
            sum: 700,
            samples: 1
        }
    );
    assert_eq!(merge.unknown, 0);
    assert!(merge.contributors[1]
        .actors
        .iter()
        .all(|actor| actor.account_id.is_none()));
    assert!(matches!(
        merge.contributors[1].evidence.as_slice(),
        [MetricTarget::Event { event_id: 951, .. }]
    ));
}
