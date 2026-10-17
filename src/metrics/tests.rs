//! Fixed-clock contracts. Staged with the module, never considered passing until Cargo runs.
use super::*;
use crate::{
    actions_workspace::{ActionsWorkspace, ScopedRun},
    github_workspace::{Freshness, GithubWorkspace},
    providers::gh::{Actor, Coverage, HostIdentity, PullRequest, Queue, WorkflowRun},
};
use serde_json::json;
fn identity() -> HostIdentity {
    HostIdentity {
        host: "github.invalid".into(),
        login: "octocat".into(),
        account_id: Some(42),
        gh_version: "fixture".into(),
    }
}
fn scope() -> MetricsScope {
    MetricsScope::new(&identity(), &["example/a".into(), "example/b".into()]).unwrap()
}
fn actor(login: &str) -> MetricActor {
    MetricActor {
        account_id: Some(45),
        actor: Actor {
            login: login.into(),
            actor_type: "User".into(),
            ..Actor::default()
        },
    }
}
fn complete(empty: bool) -> EvidenceCoverage {
    EvidenceCoverage {
        state: CoverageState::Complete,
        complete_empty: empty,
        fetched_at: Some(1000),
    }
}
fn pr(number: u64) -> PrEvidence {
    PrEvidence {
        target: PrTarget {
            repository: "example/a".into(),
            number,
            repository_id: Some(9),
            database_id: Some(100 + number),
            node_id: Some(format!("actual-fixture-pr-node-{number}")),
            source_record_id: None,
        },
        cohort_member: true,
        title: format!("PR {number}"),
        author: actor("author"),
        state: PrState::Open,
        draft: Some(false),
        head_sha: Some("a".repeat(40)),
        created_at: Some(100),
        review_requested_now: Some(true),
        timeline: EventCollection::unavailable(),
        reviews: EventCollection::unavailable(),
    }
}
fn history(events: Vec<SourceEvent>) -> EventCollection {
    let category = if events
        .first()
        .is_none_or(|event| matches!(event.kind, EventKind::ReviewSubmitted { .. }))
    {
        EventCategory::Reviews
    } else {
        EventCategory::Timeline
    };
    EventCollection {
        binding: Some(EventBinding::for_parent(&scope(), pr(1).target, category).unwrap()),
        events,
        from: Some(0),
        through: Some(1000),
        complete: true,
        source: "retained-category-v1".into(),
        generation: 7,
        reasons: vec![],
    }
}
#[test]
fn metrics_event_backfill_cannot_cross_parent_account_or_category_bindings() {
    let mut item = pr(1);
    item.reviews = history(vec![event(
        10,
        200,
        EventKind::ReviewSubmitted {
            review_id: 10,
            state: SubmittedReviewState::Approved,
        },
    )]);
    let mut narrowed = evidence(vec![item.clone()]);
    narrowed.scope = MetricsScope::new(&identity(), &["example/a".into()]).unwrap();
    assert_eq!(
        card(&snapshot(&narrowed), MetricKind::FirstReview).value,
        MetricValue::MeanSeconds {
            sum: 100,
            samples: 1
        },
        "View repository selection is not a new historical principal"
    );
    let mut wrong_principal = item.clone();
    wrong_principal.reviews.binding.as_mut().unwrap().scope =
        crate::storage::Scope::github("github.invalid", Some(43)).unwrap();
    assert_eq!(
        card(
            &snapshot(&evidence(vec![wrong_principal])),
            MetricKind::FirstReview
        )
        .value,
        MetricValue::Unavailable
    );
    item.reviews.binding.as_mut().unwrap().parent.number = 2;
    assert_eq!(
        card(
            &snapshot(&evidence(vec![item.clone()])),
            MetricKind::FirstReview
        )
        .value,
        MetricValue::Unavailable
    );
    item.reviews.binding.as_mut().unwrap().parent.number = 1;
    item.reviews.binding.as_mut().unwrap().category = EventCategory::Timeline;
    assert_eq!(
        card(&snapshot(&evidence(vec![item])), MetricKind::FirstReview).value,
        MetricValue::Unavailable
    );
    let mut corrupted = serde_json::to_value(scope()).unwrap();
    corrupted["account_id"] = json!(0);
    let invalid: MetricsScope = serde_json::from_value(corrupted).unwrap();
    let mut input = evidence(vec![]);
    input.scope = invalid;
    assert!(dashboard(&input, 1000, UtcWindow::new(0, 1000).unwrap()).is_err());
}
fn event(id: u64, at: u64, kind: EventKind) -> SourceEvent {
    SourceEvent {
        id,
        at,
        kind,
        actor: actor("reviewer"),
    }
}
fn evidence(prs: Vec<PrEvidence>) -> DashboardEvidence {
    DashboardEvidence {
        scope: scope(),
        cohort: PrCohortKind::SelectedRepositoryInventory,
        pr_coverage: complete(prs.is_empty()),
        prs,
        checks: vec![],
        check_coverage: EvidenceCoverage::unavailable("not loaded"),
        runs: vec![],
        run_coverage: EvidenceCoverage::unavailable("not loaded"),
    }
}
fn snapshot(evidence: &DashboardEvidence) -> DashboardSnapshot {
    dashboard(evidence, 1000, UtcWindow::new(0, 1000).unwrap()).unwrap()
}
fn card(snapshot: &DashboardSnapshot, kind: MetricKind) -> &MetricCard {
    snapshot
        .cards
        .iter()
        .find(|card| card.kind == kind)
        .unwrap()
}
fn run(id: u64, attempt: u64, outcome: Health, created: Option<u64>) -> RunEvidence {
    RunEvidence {
        target: MetricTarget::Run {
            repository: "example/a".into(),
            repository_id: Some(9),
            run_id: id,
            attempt,
            workflow_id: 70,
            head_sha: "a".repeat(40),
        },
        title: format!("Run {id}/{attempt}"),
        actor: actor("operator"),
        created_at: created,
        observed_at: 900,
        health: outcome,
    }
}

#[test]
fn metrics_scope_uses_positive_uid_explicit_repositories_and_fails_cross_scope_sources() {
    let mut wrong = identity();
    wrong.account_id = None;
    assert!(MetricsScope::new(&wrong, &["example/a".into()]).is_err());
    assert!(MetricsScope::new(&identity(), &[]).is_err());
    let mut input = evidence(vec![pr(1)]);
    input.prs[0].target.repository = "other/private".into();
    assert!(dashboard(&input, 1000, UtcWindow::new(0, 1000).unwrap()).is_err());
    wrong.account_id = Some(43);
    let workspace = GithubWorkspace::empty(wrong, vec![], Default::default());
    assert!(current_evidence(scope(), Some(&workspace), None).is_err());
    let mut raw = pr(1).target;
    raw.repository_id = None;
    raw.node_id = None;
    raw.source_record_id = Some("12345".into());
    assert!(!raw.has_canonical_identity());
    assert!(pr(1).target.has_canonical_identity());
}
#[test]
fn metrics_utc_half_open_windows_validate_leap_dates_and_do_not_clamp_future_ages() {
    assert!(utc_seconds("2024-02-29T00:00:00Z").is_some());
    assert!(utc_seconds("2023-02-29T00:00:00Z").is_none());
    assert!(utc_seconds("2026-10-13T24:00:00Z").is_none());
    let window = UtcWindow::new(100, 200).unwrap();
    assert!(window.contains(100));
    assert!(!window.contains(200));
    assert!(UtcWindow::new(100, 100).is_err());
    assert!(dashboard(&evidence(vec![]), 100, UtcWindow::new(0, 200).unwrap()).is_err());
}

#[test]
fn metrics_relative_inventory_future_day_year_and_leap_boundaries_keep_real_cohorts() {
    for date in [
        "2035-01-01T23:59:59Z",
        "2035-12-31T23:59:59Z",
        "2036-02-29T23:59:59Z",
    ] {
        let anchor = utc_seconds(date).unwrap();
        let mut recent = pr(1);
        recent.created_at = Some(anchor - 3 * 86_400);
        let mut old_bot = pr(2);
        old_bot.created_at = Some(anchor - 15 * 86_400);
        old_bot.review_requested_now = Some(false);
        old_bot.author.actor.actor_type = "Bot".into();
        old_bot.author.actor.login = "metrics-bot".into();
        old_bot.author.account_id = Some(52);
        let mut unknown = pr(3);
        unknown.created_at = None;
        unknown.draft = Some(true);
        unknown.author.account_id = None;
        unknown.author.actor = Actor::default();
        let input = evidence(vec![recent, old_bot, unknown]);
        for as_of in [anchor, anchor + 1] {
            let seven = dashboard(
                &input,
                as_of,
                UtcWindow::new(as_of - 7 * 86_400, as_of).unwrap(),
            )
            .unwrap();
            let thirty = dashboard(
                &input,
                as_of,
                UtcWindow::new(as_of - 30 * 86_400, as_of).unwrap(),
            )
            .unwrap();
            assert_eq!(
                card(&seven, MetricKind::OpenPrs).value,
                MetricValue::Count(3)
            );
            assert_eq!(
                card(&seven, MetricKind::NeedsMyReview).value,
                MetricValue::Count(1)
            );
            let age = card(&seven, MetricKind::OpenAge);
            assert_eq!(
                age.value,
                MetricValue::MeanSeconds {
                    sum: u128::from(18 * 86_400 + 2 * (as_of - anchor)),
                    samples: 2,
                }
            );
            assert_eq!(
                age.unknown, 1,
                "Missing creation stays unknown, not a zero age"
            );
            for kind in [MetricKind::FirstReview, MetricKind::MergeLatency] {
                let numbers = |snapshot: &DashboardSnapshot| {
                    card(snapshot, kind)
                        .contributors
                        .iter()
                        .map(|c| match &c.target {
                            MetricTarget::PullRequest(pr) => pr.number,
                            _ => panic!("PR cohort must keep actual PR target"),
                        })
                        .collect::<Vec<_>>()
                };
                assert_eq!(
                    numbers(&seven),
                    vec![1, 3],
                    "Recent plus unknown, not old bot: {date}"
                );
                assert_eq!(
                    numbers(&thirty),
                    vec![1, 2, 3],
                    "Thirty days genuinely includes old bot: {date}"
                );
                assert_eq!(card(&seven, kind).value, MetricValue::Unavailable);
                assert!(
                    card(&seven, kind).unknown > 0,
                    "Absent retained events cannot become a historical zero"
                );
            }
            assert_eq!(input.prs[1].author.actor.actor_type, "Bot");
            assert!(input.prs[2].created_at.is_none() && input.prs[2].author.account_id.is_none());
        }
    }
}

#[test]
fn metrics_relative_inventory_actual_computation_keeps_start_inclusive_end_exclusive() {
    let as_of = utc_seconds("2035-01-02T00:00:00Z").unwrap();
    let start = as_of - 7 * 86_400;
    let mut at_start = pr(1);
    at_start.created_at = Some(start);
    let mut at_end = pr(2);
    at_end.created_at = Some(as_of);
    let mut before_start = pr(3);
    before_start.created_at = Some(start - 1);
    let mut missing = pr(4);
    missing.created_at = None;
    let output = dashboard(
        &evidence(vec![at_start, at_end, before_start, missing]),
        as_of,
        UtcWindow::new(start, as_of).unwrap(),
    )
    .unwrap();
    for kind in [MetricKind::FirstReview, MetricKind::MergeLatency] {
        let actual = card(&output, kind);
        assert_eq!(
            actual
                .contributors
                .iter()
                .map(|c| match &c.target {
                    MetricTarget::PullRequest(pr) => pr.number,
                    _ => panic!("Expected canonical PR target"),
                })
                .collect::<Vec<_>>(),
            vec![1, 4]
        );
        assert_eq!(
            actual.unknown,
            if kind == MetricKind::FirstReview {
                2
            } else {
                1
            }
        );
        assert_eq!(
            actual.censored,
            if kind == MetricKind::FirstReview {
                0
            } else {
                1
            }
        );
        assert_eq!(actual.value, MetricValue::Unavailable);
    }
}
#[test]
fn metrics_open_includes_drafts_review_queue_excludes_drafts_and_deduplicates_pr_ids() {
    let first = pr(1);
    let mut draft = pr(2);
    draft.draft = Some(true);
    let mut closed = pr(3);
    closed.state = PrState::ClosedWithoutMergeEvidence;
    let output = snapshot(&evidence(vec![first.clone(), first, draft, closed]));
    assert_eq!(
        card(&output, MetricKind::OpenPrs).value,
        MetricValue::Count(2)
    );
    assert_eq!(
        card(&output, MetricKind::NeedsMyReview).value,
        MetricValue::Count(1)
    );
    assert_eq!(card(&output, MetricKind::OpenPrs).contributors.len(), 2);
}
#[test]
fn metrics_reopened_age_uses_original_creation_and_draft_age_remains_in_cohort() {
    let mut reopened = pr(1);
    reopened.timeline = history(vec![event(9, 900, EventKind::Reopened)]);
    let mut draft = pr(2);
    draft.draft = Some(true);
    draft.created_at = Some(200);
    let output = snapshot(&evidence(vec![reopened, draft]));
    assert_eq!(
        card(&output, MetricKind::OpenAge).value,
        MetricValue::MeanSeconds {
            sum: 1700,
            samples: 2
        }
    );
    assert!(card(&output, MetricKind::OpenAge)
        .formula
        .contains("reopened time does not reset"));
}
#[test]
fn metrics_missing_future_dates_and_missing_draft_state_are_unknown_with_contributor_ids() {
    let mut missing = pr(2);
    missing.created_at = None;
    missing.draft = None;
    let mut future = pr(3);
    future.created_at = Some(1001);
    let mut missing_request = pr(4);
    missing_request.review_requested_now = None;
    let output = snapshot(&evidence(vec![pr(1), missing, future, missing_request]));
    let age = card(&output, MetricKind::OpenAge);
    assert_eq!(
        age.value,
        MetricValue::MeanSeconds {
            sum: 1800,
            samples: 2
        }
    );
    assert_eq!(age.unknown, 2);
    assert_eq!(age.contributors.len(), 4);
    assert!(matches!(age.coverage.state, CoverageState::Partial(_)));
    assert_eq!(card(&output, MetricKind::NeedsMyReview).unknown, 2);
}
#[test]
fn metrics_only_complete_empty_producer_can_show_zero_missing_partial_empty_is_unavailable() {
    let mut input = evidence(vec![]);
    assert_eq!(
        card(&snapshot(&input), MetricKind::OpenPrs).value,
        MetricValue::Count(0)
    );
    input.pr_coverage.complete_empty = false;
    assert_eq!(
        card(&snapshot(&input), MetricKind::OpenPrs).value,
        MetricValue::Unavailable
    );
    input.pr_coverage.state = CoverageState::Partial(vec!["one repo failed".into()]);
    assert_eq!(
        card(&snapshot(&input), MetricKind::NeedsMyReview).value,
        MetricValue::Unavailable
    );
    input.pr_coverage.state = CoverageState::Unavailable("no source".into());
    assert_eq!(
        card(&snapshot(&input), MetricKind::OpenPrs).value,
        MetricValue::Unavailable
    );
}
#[test]
fn metrics_review_request_wait_selects_last_genuine_viewer_request_not_an_update_or_other_user() {
    let mut item = pr(1);
    item.timeline = history(vec![
        event(1, 200, EventKind::ReviewRequested { reviewer_id: 43 }),
        event(2, 300, EventKind::ReviewRequested { reviewer_id: 42 }),
        event(3, 400, EventKind::ReviewRequestRemoved { reviewer_id: 42 }),
        event(4, 700, EventKind::ReviewRequested { reviewer_id: 42 }),
    ]);
    let output = snapshot(&evidence(vec![item]));
    let wait = card(&output, MetricKind::ReviewWait);
    assert_eq!(
        wait.value,
        MetricValue::MeanSeconds {
            sum: 300,
            samples: 1
        }
    );
    assert_eq!(
        wait.contributors[0].evidence,
        vec![MetricTarget::Event {
            pr: pr(1).target,
            event_id: 4
        }]
    );
}
#[test]
fn metrics_same_second_request_removal_and_missing_category_backfill_do_not_guess_order() {
    let mut item = pr(1);
    item.timeline = history(vec![
        event(2, 300, EventKind::ReviewRequested { reviewer_id: 42 }),
        event(1, 300, EventKind::ReviewRequestRemoved { reviewer_id: 42 }),
    ]);
    assert_eq!(
        card(
            &snapshot(&evidence(vec![item.clone()])),
            MetricKind::ReviewWait
        )
        .value,
        MetricValue::Unavailable
    );
    item.timeline.events.pop();
    item.timeline.generation = 0;
    assert_eq!(
        card(&snapshot(&evidence(vec![item])), MetricKind::ReviewWait).value,
        MetricValue::Unavailable
    );
}
#[test]
fn metrics_first_submitted_review_retains_dismissed_timestamp_actors_and_excludes_pending() {
    let raw = observed_reviews(&[
        json!({"id":1,"state":"PENDING","submitted_at":"1970-01-01T00:02:00Z"}),
        json!({"id":2,"state":"DISMISSED","submitted_at":"1970-01-01T00:02:30Z","user":{"login":"reviewer","id":45,"type":"User"}}),
    ]);
    assert_eq!(raw.events.len(), 1);
    assert!(!raw.complete);
    let mut item = pr(1);
    item.reviews = history(raw.events);
    let output = snapshot(&evidence(vec![item]));
    let first = card(&output, MetricKind::FirstReview);
    assert_eq!(
        first.value,
        MetricValue::MeanSeconds {
            sum: 50,
            samples: 1
        }
    );
    assert_eq!(
        first.contributors[0].actors[1].identity(&scope()).label(),
        "@reviewer"
    );
    assert_eq!(first.contributors[0].actors[1].account_id, Some(45));
}
#[test]
fn metrics_partial_earliest_reviews_never_promote_observed_sample_into_first_review_truth() {
    let mut item = pr(1);
    item.reviews = history(vec![event(
        1,
        500,
        EventKind::ReviewSubmitted {
            review_id: 90,
            state: SubmittedReviewState::Commented,
        },
    )]);
    item.reviews.complete = false;
    let output = snapshot(&evidence(vec![item]));
    assert_eq!(
        card(&output, MetricKind::FirstReview).value,
        MetricValue::Unavailable
    );
    assert_eq!(card(&output, MetricKind::FirstReview).unknown, 1);
}
#[test]
fn metrics_unreviewed_and_closed_unmerged_are_censored_not_zero_latency() {
    let mut item = pr(1);
    item.reviews = history(vec![]);
    item.state = PrState::ClosedWithoutMergeEvidence;
    let output = snapshot(&evidence(vec![item]));
    assert_eq!(card(&output, MetricKind::FirstReview).censored, 1);
    assert_eq!(
        card(&output, MetricKind::FirstReview).value,
        MetricValue::Unavailable
    );
    assert_eq!(card(&output, MetricKind::MergeLatency).censored, 1);
}
#[test]
fn metrics_merge_latency_requires_actual_merged_event_and_does_not_reset_for_reopening() {
    let mut item = pr(1);
    item.state = PrState::Merged;
    item.timeline = history(vec![
        event(1, 500, EventKind::Closed),
        event(2, 600, EventKind::Reopened),
        event(3, 900, EventKind::Merged),
    ]);
    let output = snapshot(&evidence(vec![item.clone()]));
    assert_eq!(
        card(&output, MetricKind::MergeLatency).value,
        MetricValue::MeanSeconds {
            sum: 800,
            samples: 1
        }
    );
    item.timeline.events.pop();
    assert_eq!(
        card(&snapshot(&evidence(vec![item])), MetricKind::MergeLatency).value,
        MetricValue::Unavailable
    );
}
#[test]
fn metrics_personal_queue_union_is_never_reported_as_repository_historical_cohort() {
    let mut item = pr(1);
    item.reviews = history(vec![event(
        1,
        200,
        EventKind::ReviewSubmitted {
            review_id: 10,
            state: SubmittedReviewState::Approved,
        },
    )]);
    let mut input = evidence(vec![item]);
    input.cohort = PrCohortKind::PersonalQueueUnion;
    let output = snapshot(&input);
    let first = card(&output, MetricKind::FirstReview);
    assert_eq!(
        first.value,
        MetricValue::MeanSeconds {
            sum: 100,
            samples: 1
        }
    );
    assert!(matches!(first.coverage.state, CoverageState::Partial(_)));
    assert!(first.cohort.contains("not repository-wide"));
    assert!(output.notices[0].contains("not historical trends"));
}
#[test]
fn metrics_workflow_health_uses_highest_observed_attempt_not_number_of_attempts_or_update_duration()
{
    let mut input = evidence(vec![]);
    input.runs = vec![
        run(100, 1, Health::Failure, Some(100)),
        run(100, 2, Health::Pending, Some(100)),
        run(101, 1, Health::Success, Some(100)),
        run(102, 1, Health::Cancelled, Some(100)),
    ];
    input.run_coverage = complete(false);
    let output = snapshot(&input);
    let workflows = card(&output, MetricKind::Workflows);
    assert_eq!(
        workflows.value,
        MetricValue::HealthCounts {
            success: 1,
            failure: 0,
            pending: 1,
            action_required: 0,
            cancelled: 1,
            neutral: 0,
            unknown: 0
        }
    );
    assert_eq!(workflows.contributors.len(), 3);
    assert!(matches!(
        workflows.contributors[0].target,
        MetricTarget::Run {
            run_id: 100,
            attempt: 2,
            ..
        }
    ));
    assert!(workflows
        .contributors
        .iter()
        .all(|row| row.measured_seconds.is_none()));
}
#[test]
fn metrics_run_created_window_is_half_open_and_missing_dates_keep_exact_unknown_destinations() {
    let mut input = evidence(vec![]);
    input.runs = vec![
        run(100, 2, Health::Success, Some(100)),
        run(101, 1, Health::Success, Some(200)),
        run(102, 1, Health::Success, None),
    ];
    input.run_coverage = complete(false);
    let output = dashboard(&input, 1000, UtcWindow::new(100, 200).unwrap()).unwrap();
    let workflows = card(&output, MetricKind::Workflows);
    assert_eq!(workflows.contributors.len(), 2);
    assert_eq!(workflows.unknown, 1);
    assert!(matches!(
        workflows.contributors[1].target,
        MetricTarget::Run { run_id: 102, .. }
    ));
    assert!(matches!(
        workflows.coverage.state,
        CoverageState::Partial(_)
    ));
}
#[test]
fn metrics_exact_head_check_health_distinguishes_pending_unknown_and_cannot_attach_prefix_alias() {
    let mut input = evidence(vec![pr(1)]);
    input.checks = vec![CheckEvidence {
        target: MetricTarget::Check {
            pr: pr(1).target,
            head_sha: "a".repeat(40),
            check_id: 8,
            kind: "check".into(),
        },
        name: "ci".into(),
        health: health("in_progress", ""),
    }];
    input.check_coverage = complete(false);
    let output = snapshot(&input);
    assert!(matches!(
        card(&output, MetricKind::Checks).value,
        MetricValue::HealthCounts {
            pending: 1,
            failure: 0,
            ..
        }
    ));
    if let MetricTarget::Check { head_sha, .. } = &mut input.checks[0].target {
        *head_sha = "b".repeat(40);
    }
    assert!(dashboard(&input, 1000, UtcWindow::new(0, 1000).unwrap()).is_err());
    assert_eq!(health("completed", ""), Health::Unknown);
    assert_eq!(
        health("completed", "action_required"),
        Health::ActionRequired
    );
}
#[test]
fn metrics_conflicting_canonical_pr_run_ids_or_unavailable_nonempty_sources_fail_closed() {
    let mut input = evidence(vec![pr(1)]);
    let mut duplicate = pr(1);
    duplicate.title = "contradictory same ID".into();
    input.prs.push(duplicate);
    assert!(dashboard(&input, 1000, UtcWindow::new(0, 1000).unwrap()).is_err());
    input = evidence(vec![]);
    let mut contradictory = run(1, 2, Health::Success, Some(100));
    if let MetricTarget::Run { head_sha, .. } = &mut contradictory.target {
        *head_sha = "b".repeat(40);
    }
    input.runs = vec![run(1, 1, Health::Failure, Some(100)), contradictory];
    input.run_coverage = complete(false);
    assert!(dashboard(&input, 1000, UtcWindow::new(0, 1000).unwrap()).is_err());
    input = evidence(vec![pr(1)]);
    input.pr_coverage = EvidenceCoverage::unavailable("inaccessible");
    assert!(dashboard(&input, 1000, UtcWindow::new(0, 1000).unwrap()).is_err());
}
#[test]
fn metrics_current_inbox_adapter_never_uses_summed_queue_total_as_denominator_or_history_proof() {
    let mut github =
        GithubWorkspace::empty(identity(), vec!["example/a".into()], Default::default());
    github.freshness = Freshness::Fresh { fetched_at: 1000 };
    github.inbox.pull_requests = vec![PullRequest {
        host: "github.invalid".into(),
        id: "PR_source".into(),
        repository: "example/a".into(),
        number: 7,
        title: "Scoped personal PR".into(),
        body: String::new(),
        url: String::new(),
        author: Actor {
            login: "octocat".into(),
            actor_type: "User".into(),
            ..Actor::default()
        },
        state: "open".into(),
        draft: false,
        created_at: "1970-01-01T00:01:40Z".into(),
        updated_at: "1970-01-01T00:10:00Z".into(),
        queues: [Queue::Authored, Queue::Assigned, Queue::ReviewRequested]
            .into_iter()
            .collect(),
    }];
    github.inbox.coverage = Coverage {
        loaded: 1,
        total_matches: 3,
        complete: true,
        reasons: vec![],
    };
    let projected = current_evidence(scope(), Some(&github), None).unwrap();
    let output = snapshot(&projected);
    assert_eq!(
        card(&output, MetricKind::OpenPrs).value,
        MetricValue::Count(1)
    );
    assert_eq!(
        card(&output, MetricKind::NeedsMyReview).value,
        MetricValue::Count(1)
    );
    assert_eq!(
        card(&output, MetricKind::ReviewWait).value,
        MetricValue::Unavailable
    );
    assert_eq!(
        card(&output, MetricKind::FirstReview).value,
        MetricValue::Unavailable
    );
}
#[test]
fn metrics_current_actions_adapter_retains_attempt_actor_ids_without_update_timestamp_duration() {
    let mut actions =
        ActionsWorkspace::new(identity(), vec!["example/a".into(), "example/b".into()]);
    actions.fetched_at = Some(900);
    actions.coverage = Coverage {
        loaded: 1,
        total_matches: 1,
        complete: true,
        reasons: vec![],
    };
    actions.runs = vec![ScopedRun {
        repository: "example/a".into(),
        run: WorkflowRun {
            id: 8,
            workflow_id: 70,
            name: "ci".into(),
            title: "Actual run".into(),
            head_sha: "a".repeat(40),
            head_branch: "main".into(),
            event: "push".into(),
            status: "completed".into(),
            conclusion: "success".into(),
            attempt: 3,
            actor: actor("initial").actor,
            triggering_actor: actor("rerunner").actor,
            created_at: "1970-01-01T00:01:40Z".into(),
            updated_at: "1970-01-01T00:15:00Z".into(),
            url: String::new(),
        },
    }];
    let projected = current_evidence(scope(), None, Some(&actions)).unwrap();
    let output = snapshot(&projected);
    let row = &card(&output, MetricKind::Workflows).contributors[0];
    assert!(matches!(
        row.target,
        MetricTarget::Run {
            run_id: 8,
            attempt: 3,
            ..
        }
    ));
    assert_eq!(row.actors[0].identity(&scope()).label(), "@rerunner");
    assert_eq!(row.measured_seconds, None);
    actions.filter.status = Some("success".into());
    let filtered = current_evidence(scope(), None, Some(&actions)).unwrap();
    assert!(matches!(
        filtered.run_coverage.state,
        CoverageState::Partial(_)
    ));
}
