//! Staged contracts for the additive history compatibility change; still UNRUN.
use super::*;
use crate::{
    providers::gh::{Actor, HostIdentity},
    storage::Scope,
};

fn source() -> (DashboardEvidence, PrTarget) {
    let scope = MetricsScope::new(
        &HostIdentity {
            host: "github.invalid".into(),
            login: "octocat".into(),
            account_id: Some(42),
            gh_version: "fixture".into(),
        },
        &["example/a".into()],
    )
    .unwrap();
    let target = PrTarget {
        repository: "example/a".into(),
        number: 7,
        repository_id: Some(9),
        database_id: Some(107),
        node_id: Some("genuine-pr-node".into()),
        source_record_id: None,
    };
    let actor = MetricActor {
        account_id: Some(45),
        actor: Actor {
            login: "reviewer".into(),
            actor_type: "User".into(),
            ..Actor::default()
        },
    };
    let reviews = EventCollection {
        binding: Some(EventBinding {
            scope: Scope::github("github.invalid", Some(42)).unwrap(),
            parent: target.clone(),
            category: EventCategory::Reviews,
        }),
        events: vec![SourceEvent {
            id: 50,
            at: 200,
            kind: EventKind::ReviewSubmitted {
                review_id: 50,
                state: SubmittedReviewState::Dismissed,
            },
            actor: actor.clone(),
        }],
        from: Some(100),
        through: Some(1000),
        complete: true,
        source: "actual-sql-retained-reviews-v1".into(),
        generation: 7,
        basis: HistoryBasis::ApiVisibleRetainedBackfill {
            limitations: vec![],
        },
        reasons: vec![],
    };
    (
        DashboardEvidence {
            scope,
            cohort: PrCohortKind::SelectedRepositoryInventory,
            prs: vec![PrEvidence {
                target: target.clone(),
                cohort_member: true,
                title: "Exact parent".into(),
                author: actor,
                state: PrState::Open,
                draft: Some(false),
                head_sha: Some("a".repeat(40)),
                created_at: Some(100),
                review_requested_now: Some(false),
                timeline: EventCollection::unavailable(),
                reviews,
            }],
            pr_coverage: EvidenceCoverage {
                state: CoverageState::Complete,
                complete_empty: false,
                fetched_at: Some(1000),
            },
            checks: vec![],
            check_coverage: EvidenceCoverage::unavailable("No checks"),
            runs: vec![],
            run_coverage: EvidenceCoverage::unavailable("No workflows"),
        },
        target,
    )
}
fn first(input: &DashboardEvidence) -> MetricCard {
    dashboard(input, 1000, UtcWindow::new(0, 1000).unwrap())
        .unwrap()
        .cards
        .into_iter()
        .find(|card| card.kind == MetricKind::FirstReview)
        .unwrap()
}

#[test]
fn metrics_history_generic_complete_bool_and_legacy_serialization_never_mint_available_basis() {
    let (mut input, _) = source();
    input.prs[0].reviews.basis = HistoryBasis::Unavailable;
    assert!(!input.prs[0].reviews.covers_lifetime(100, 1000));
    assert_eq!(first(&input).value, MetricValue::Unavailable);
    assert_eq!(first(&input).unknown, 1);
    let mut encoded = serde_json::to_value(&input.prs[0].reviews).unwrap();
    encoded.as_object_mut().unwrap().remove("basis");
    let legacy: EventCollection = serde_json::from_value(encoded).unwrap();
    assert_eq!(legacy.basis, HistoryBasis::Unavailable);
    assert!(!legacy.covers_lifetime(100, 1000));
}

#[test]
fn metrics_history_available_api_first_latency_retains_actual_ids_and_mandatory_erased_gap_label() {
    let (input, target) = source();
    let card = first(&input);
    assert_eq!(
        card.value,
        MetricValue::MeanSeconds {
            sum: 100,
            samples: 1
        }
    );
    assert_eq!(
        card.contributors[0].evidence,
        vec![MetricTarget::Review {
            pr: target,
            review_id: 50
        }]
    );
    assert_eq!(card.contributors[0].actors[1].account_id, Some(45));
    assert!(card.label.contains("API-visible"));
    assert!(card.formula.contains("API-visible"));
    let CoverageState::Partial(reasons) = card.coverage.state else {
        panic!("API traversal cannot prove erased history")
    };
    assert!(reasons
        .iter()
        .any(|reason| reason == API_VISIBLE_HISTORY_LIMITATION));
    assert_eq!(
        card.unknown, 0,
        "Source limitation is not a missing visible event"
    );
}

#[test]
fn metrics_history_capped_missing_interval_or_generation_cannot_promote_first_observed_sample() {
    let (input, _) = source();
    let mut capped = input.clone();
    capped.prs[0].reviews.complete = false;
    assert_eq!(first(&capped).value, MetricValue::Unavailable);
    assert_eq!(first(&capped).unknown, 1);
    let mut missing = input.clone();
    missing.prs[0].reviews.from = None;
    assert_eq!(first(&missing).value, MetricValue::Unavailable);
    let mut missing = input;
    missing.prs[0].reviews.generation = 0;
    assert_eq!(first(&missing).value, MetricValue::Unavailable);
    assert_eq!(
        missing.prs[0].reviews.events[0].id, 50,
        "Observed source fact remains retained"
    );
}

#[test]
fn metrics_history_original_asof_projection_excludes_later_facts_without_erasing_retained_events() {
    let (mut input, _) = source();
    let mut later = input.prs[0].reviews.events[0].clone();
    later.id = 51;
    later.at = 1001;
    later.kind = EventKind::ReviewSubmitted {
        review_id: 51,
        state: SubmittedReviewState::Commented,
    };
    input.prs[0].reviews.events.push(later);
    assert_eq!(
        first(&input).value,
        MetricValue::Unavailable,
        "A mixed-cutoff projection must fail closed"
    );
    let retained = input.prs[0].reviews.events.clone();
    input.prs[0].reviews.events.retain(|event| event.at <= 1000);
    assert_eq!(
        first(&input).value,
        MetricValue::MeanSeconds {
            sum: 100,
            samples: 1
        }
    );
    assert_eq!(retained.len(), 2);
    assert_eq!(retained[1].at, 1001);
    input.prs[0].reviews.basis = HistoryBasis::ApiVisibleRetainedBackfill {
        limitations: vec!["oversized".repeat(512)],
    };
    assert_eq!(
        first(&input).value,
        MetricValue::Unavailable,
        "Bounded source metadata is required too"
    );
}
