//! Source-only combined producer/SQL reservation tests. Register with history
//! factory/conversion + exact atomic core compatibility; no native claim here.
use super::*;
use crate::providers::{gh::MetricsBackfillRequestPair, runner::CancellationToken};
use crate::test_support::{Fixture, GhStep};
use std::fs;

#[derive(Default)]
struct Authority;
impl MetricsRetentionAuthority for Authority {
    fn verify(&mut self, target: &MetricEventTarget, _: MetricAuthorityPhase) -> Result<()> {
        target.validate()
    }
}
fn identity() -> HostIdentity {
    HostIdentity {
        host: "github.invalid".into(),
        login: "viewer".into(),
        account_id: Some(42),
        gh_version: "fixture".into(),
    }
}
fn repo() -> Value {
    json!({"id":99,"node_id":"R_99","full_name":"example/repo"})
}
fn parent() -> Value {
    json!({"id":101,"node_id":"PR_1","number":1,"title":"Historical source","state":"open","draft":false,"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-12T00:00:00Z","merged_at":null,"closed_at":null,"merged":false,"head":{"sha":"a".repeat(40)},"base":{"repo":repo()},"user":{"id":51,"login":"alice"},"requested_reviewers":[],"requested_teams":[]})
}
fn gh_step(endpoint: &str, value: Value) -> GhStep {
    GhStep {
        args: vec![
            "api".into(),
            endpoint.into(),
            "--hostname".into(),
            "github.invalid".into(),
            "--method".into(),
            "GET".into(),
        ],
        stdin: vec![],
        stdout: serde_json::to_vec(&value).unwrap(),
        stderr: vec![],
        status: 0,
        delay_ms: 0,
    }
}
fn inventory() -> Vec<GhStep> {
    vec![
        gh_step("repos/example/repo", repo()),
        gh_step(
            "repos/example/repo/pulls?state=all&sort=created&direction=desc&per_page=100&page=1",
            json!([parent()]),
        ),
        gh_step("repos/example/repo", repo()),
    ]
}
fn review(id: u64, at: &str) -> Value {
    json!({"id":id,"node_id":format!("REV_{id}"),"state":"COMMENTED","submitted_at":at,"user":{"id":53,"login":"reviewer"}})
}
fn event(id: u64, name: &str) -> Value {
    json!({"id":id,"node_id":format!("EV_{id}"),"event":name,"created_at":"2026-10-02T00:00:00Z","actor":{"id":54,"login":"requester"},"requested_reviewer":{"id":42,"login":"viewer"}})
}
fn history(page_size: usize, reviews: Value, timeline: Value) -> Vec<GhStep> {
    vec![
        gh_step("user", json!({"id":42,"login":"viewer"})),
        gh_step("repos/example/repo", repo()),
        gh_step("repos/example/repo/pulls/1", parent()),
        gh_step(
            &format!("repos/example/repo/pulls/1/reviews?per_page={page_size}&page=1"),
            reviews,
        ),
        gh_step(
            &format!("repos/example/repo/issues/1/timeline?per_page={page_size}&page=1"),
            timeline,
        ),
        gh_step("repos/example/repo", repo()),
        gh_step("repos/example/repo/pulls/1", parent()),
        gh_step("user", json!({"id":42,"login":"viewer"})),
    ]
}
fn gh(fixture: &Fixture) -> crate::providers::gh::GithubProvider {
    crate::providers::gh::GithubProvider::new("github.invalid")
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"))
}
fn load_parent(fixture: &Fixture) -> MetricsPrObservation {
    gh(fixture)
        .repository_pr_inventory(
            &identity(),
            "example/repo",
            None,
            &Default::default(),
            &CancellationToken::default(),
        )
        .unwrap()
        .prs
        .remove(0)
}
fn reserve(
    cache: &mut Cache,
    parent: &MetricsPrObservation,
    category: EventCategory,
    cutoff: u64,
) -> MetricEventsReservation {
    let target = MetricEventTarget::from_provider(&identity(), parent, category).unwrap();
    cache
        .reserve_metric_events(
            &target,
            &MetricReadIntent {
                requested_from: Some(parent.created_at.as_ref().unwrap().unix_seconds()),
                requested_through: cutoff,
            },
            &Default::default(),
            &mut Authority,
        )
        .unwrap()
}
fn pair(
    cache: &Cache,
    parent: &MetricsPrObservation,
    reviews: &MetricEventsReservation,
    timeline: &MetricEventsReservation,
) -> MetricsBackfillRequestPair {
    MetricsBackfillRequestPair::new(
        reviews
            .backfill_request(cache, &identity(), parent, &mut Authority)
            .unwrap(),
        timeline
            .backfill_request(cache, &identity(), parent, &mut Authority)
            .unwrap(),
    )
    .unwrap()
}
fn journal(fixture: &Fixture, count: usize) -> Vec<Value> {
    let rows = fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), count);
    assert!(rows
        .iter()
        .all(|row| row["matched"] == true && row["args"][5] == "GET" && row["stdin_hex"] == ""));
    rows
}
const CUTOFF: u64 = 1791158400; // Actual requested 2026-10-05 UTC, before traversal.

fn card_from_collection(
    parent: &MetricsPrObservation,
    collection: EventCollection,
    kind: crate::metrics::MetricKind,
) -> crate::metrics::MetricCard {
    use crate::metrics::{
        dashboard, DashboardEvidence, EvidenceCoverage, PrCohortKind, PrEvidence, PrState,
        UtcWindow,
    };
    let scope = MetricsScope::new(&identity(), &["example/repo".into()]).unwrap();
    let binding = collection.binding.as_ref().unwrap();
    let target = binding.parent.clone();
    let category = binding.category;
    let mut pr = PrEvidence {
        target,
        cohort_member: true,
        title: parent.title.clone(),
        author: MetricActor {
            account_id: parent.author.account_id,
            actor: parent.author.actor.clone(),
        },
        state: PrState::Open,
        draft: Some(false),
        head_sha: parent.head_sha.clone(),
        created_at: parent.created_at.as_ref().map(|at| at.unix_seconds()),
        review_requested_now: parent
            .requested_users
            .as_ref()
            .map(|users| users.iter().any(|user| user.account_id == Some(42))),
        timeline: EventCollection::unavailable(),
        reviews: EventCollection::unavailable(),
    };
    match category {
        EventCategory::Reviews => pr.reviews = collection,
        EventCategory::Timeline => pr.timeline = collection,
    }
    let input = DashboardEvidence {
        scope,
        cohort: PrCohortKind::SelectedRepositoryInventory,
        prs: vec![pr],
        pr_coverage: EvidenceCoverage {
            state: crate::metrics::CoverageState::Complete,
            complete_empty: false,
            fetched_at: Some(CUTOFF),
        },
        checks: vec![],
        check_coverage: EvidenceCoverage::unavailable("No check source"),
        runs: vec![],
        run_coverage: EvidenceCoverage::unavailable("No run source"),
    };
    dashboard(&input, CUTOFF, UtcWindow::new(1790812800, CUTOFF).unwrap())
        .unwrap()
        .cards
        .into_iter()
        .find(|card| card.kind == kind)
        .unwrap()
}

fn project(frame: &StoredMetricFrame, parent: &MetricsPrObservation) -> EventCollection {
    frame
        .validated_projection(
            &MetricsScope::new(&identity(), &["example/repo".into()]).unwrap(),
            parent,
        )
        .unwrap()
}

fn commit_available(
    cache: &mut Cache,
    reservation: &MetricEventsReservation,
    backfill: &crate::providers::gh::ApiVisibleMetricBackfill,
) -> StoredMetricFrame {
    let positive =
        AvailableMetricCategoryBatch::from_backfill(cache, reservation, backfill, &mut Authority)
            .unwrap()
            .unwrap();
    cache
        .commit_metric_history(reservation, positive, &mut Authority)
        .unwrap()
}

#[test]
fn metrics_history_actual_complete_traversal_original_sql_tokens_and_atomic_clipped_projection() {
    let fixture = Fixture::new();
    let mut steps = inventory();
    steps.extend(history(
        100,
        json!([
            review(201, "2026-10-02T00:00:00Z"),
            review(202, "2026-10-06T00:00:00Z")
        ]),
        json!([event(900, "review_requested")]),
    ));
    fixture.scripted_gh(&steps);
    let parent = load_parent(&fixture);
    let mut cache = Cache::open(&fixture.root.join("history.sqlite")).unwrap();
    let reviews = reserve(&mut cache, &parent, EventCategory::Reviews, CUTOFF);
    let timeline = reserve(&mut cache, &parent, EventCategory::Timeline, CUTOFF);
    let requests = pair(&cache, &parent, &reviews, &timeline);
    let debug = format!("{requests:?}");
    assert!(!debug.contains("history.sqlite") && !debug.contains(&reviews.saved.intent_sha256));
    let backfill = gh(&fixture)
        .metric_history_backfill(
            &requests,
            &Default::default(),
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(backfill.observations().requests, 8);
    let proof = backfill
        .category_proof(MetricsBackfillCategory::Reviews)
        .unwrap();
    assert_eq!(proof.interval(), (1790812800, CUTOFF));
    assert!(proof
        .limitations()
        .iter()
        .any(|limitation| limitation.contains("erased")));
    let available =
        AvailableMetricCategoryBatch::from_backfill(&cache, &reviews, &backfill, &mut Authority)
            .unwrap()
            .unwrap();
    assert_eq!(available.current.facts.len(), 2); // Later observation remains retained.
    let committed = cache
        .commit_metric_history(&reviews, available, &mut Authority)
        .unwrap();
    assert_eq!(
        cache
            .sync_checkpoint(
                reviews.target().scope(),
                &reviews.target().source().unwrap()
            )
            .unwrap(),
        Some((1, false))
    );
    let scope = MetricsScope::new(&identity(), &["example/repo".into()]).unwrap();
    let state = cache
        .get(
            reviews.target().scope(),
            STATE,
            &reviews.target().source().unwrap(),
        )
        .unwrap()
        .unwrap();
    assert!(state.payload["available_history"].is_object());
    assert_eq!(state.payload["available_history"]["original_generation"], 1);
    let reread = cache
        .read_metric_events(reviews.target(), &Default::default(), &mut Authority)
        .unwrap()
        .unwrap();
    assert_eq!(reread, committed);
    let prepared = reread.validated_projection(&scope, &parent).unwrap();
    assert!(prepared.complete && prepared.covers_lifetime(1790812800, CUTOFF));
    assert!(matches!(
        prepared.basis,
        crate::metrics::HistoryBasis::ApiVisibleRetainedBackfill { .. }
    ));
    assert_eq!(
        prepared
            .events
            .iter()
            .map(|event| event.id)
            .collect::<Vec<_>>(),
        [201]
    );
    assert_eq!(prepared.events[0].actor.account_id, Some(53));
    let rows = journal(&fixture, 11);
    assert!(rows.iter().all(
        |row| !row.to_string().contains(&reviews.saved.intent_sha256)
            && !row.to_string().contains("metrics-events-v1:")
    ));
}
#[test]
fn metrics_history_pair_wrong_category_cutoff_and_issuer_refuse_without_extra_transport() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&inventory());
    let parent = load_parent(&fixture);
    let mut cache = Cache::open(&fixture.root.join("original.sqlite")).unwrap();
    let mut other = Cache::open(&fixture.root.join("other.sqlite")).unwrap();
    let reviews = reserve(&mut cache, &parent, EventCategory::Reviews, CUTOFF);
    let timeline = reserve(&mut cache, &parent, EventCategory::Timeline, CUTOFF);
    let foreign = reserve(&mut other, &parent, EventCategory::Timeline, CUTOFF);
    let r = reviews
        .backfill_request(&cache, &identity(), &parent, &mut Authority)
        .unwrap();
    let t = timeline
        .backfill_request(&cache, &identity(), &parent, &mut Authority)
        .unwrap();
    assert!(MetricsBackfillRequestPair::new(t.clone(), r.clone()).is_err());
    assert!(MetricsBackfillRequestPair::new(r.clone(), r.clone()).is_err());
    assert!(MetricsBackfillRequestPair::new(
        r.clone(),
        foreign
            .backfill_request(&other, &identity(), &parent, &mut Authority)
            .unwrap()
    )
    .is_err());
    assert!(reviews
        .backfill_request(&other, &identity(), &parent, &mut Authority)
        .is_err());
    let later = reserve(&mut cache, &parent, EventCategory::Timeline, CUTOFF + 1);
    assert!(MetricsBackfillRequestPair::new(
        r,
        later
            .backfill_request(&cache, &identity(), &parent, &mut Authority)
            .unwrap()
    )
    .is_err());
    journal(&fixture, 3);
}
#[test]
fn metrics_history_fenced_tokens_and_old_completion_cannot_move_to_new_generation() {
    let fixture = Fixture::new();
    let mut steps = inventory();
    steps.extend(history(
        100,
        json!([review(201, "2026-10-02T00:00:00Z")]),
        json!([]),
    ));
    fixture.scripted_gh(&steps);
    let parent = load_parent(&fixture);
    let mut cache = Cache::open(&fixture.root.join("generation.sqlite")).unwrap();
    let reviews = reserve(&mut cache, &parent, EventCategory::Reviews, CUTOFF);
    let timeline = reserve(&mut cache, &parent, EventCategory::Timeline, CUTOFF);
    let backfill = gh(&fixture)
        .metric_history_backfill(
            &pair(&cache, &parent, &reviews, &timeline),
            &Default::default(),
            &CancellationToken::default(),
        )
        .unwrap();
    let newer = reserve(&mut cache, &parent, EventCategory::Reviews, CUTOFF);
    assert_eq!(newer.generation(), 2);
    assert!(AvailableMetricCategoryBatch::from_backfill(
        &cache,
        &reviews,
        &backfill,
        &mut Authority
    )
    .is_err());
    assert!(
        AvailableMetricCategoryBatch::from_backfill(&cache, &newer, &backfill, &mut Authority)
            .is_err()
    );
    cache
        .revoke_repository(reviews.target().scope(), "example/repo")
        .unwrap();
    assert!(newer
        .backfill_request(&cache, &identity(), &parent, &mut Authority)
        .is_err());
    assert!(cache
        .sync_checkpoint(
            reviews.target().scope(),
            &reviews.target().source().unwrap()
        )
        .unwrap()
        .is_none());
    journal(&fixture, 11);
}
#[test]
fn metrics_history_capped_and_unsupported_categories_never_mint_complete_proof() {
    for capped in [false, true] {
        let fixture = Fixture::new();
        let mut steps = inventory();
        let reviews = if capped {
            json!([
                review(201, "2026-10-02T00:00:00Z"),
                review(202, "2026-10-03T00:00:00Z")
            ])
        } else {
            json!([review(201, "2026-10-02T00:00:00Z")])
        };
        let timeline = if capped {
            json!([event(900, "review_requested")])
        } else {
            json!([event(900, "future_unknown_review_reset")])
        };
        steps.extend(history(if capped { 2 } else { 100 }, reviews, timeline));
        fixture.scripted_gh(&steps);
        let parent = load_parent(&fixture);
        let mut cache = Cache::memory().unwrap();
        let reviews = reserve(&mut cache, &parent, EventCategory::Reviews, CUTOFF);
        let timeline = reserve(&mut cache, &parent, EventCategory::Timeline, CUTOFF);
        let limits = crate::providers::gh::MetricsReadLimits {
            max_pages: 1,
            page_size: if capped { 2 } else { 100 },
            ..Default::default()
        };
        let result = gh(&fixture)
            .metric_history_backfill(
                &pair(&cache, &parent, &reviews, &timeline),
                &limits,
                &CancellationToken::default(),
            )
            .unwrap();
        let (unknown, known) = if capped {
            (&reviews, &timeline)
        } else {
            (&timeline, &reviews)
        };
        assert!(AvailableMetricCategoryBatch::from_backfill(
            &cache,
            unknown,
            &result,
            &mut Authority
        )
        .unwrap()
        .is_none());
        assert!(AvailableMetricCategoryBatch::from_backfill(
            &cache,
            known,
            &result,
            &mut Authority
        )
        .unwrap()
        .is_some());
        assert!(cache
            .sync_checkpoint(
                unknown.target().scope(),
                &unknown.target().source().unwrap()
            )
            .unwrap()
            .is_none());
        journal(&fixture, 11);
    }
}
#[test]
fn metrics_history_changed_parent_or_principal_discards_positive_history() {
    for change in 0..3 {
        let fixture = Fixture::new();
        let mut steps = inventory();
        let mut read = history(100, json!([review(201, "2026-10-02T00:00:00Z")]), json!([]));
        let expected = match change {
            0 => {
                let mut changed = parent();
                changed["head"]["sha"] = json!("b".repeat(40));
                read[6] = gh_step("repos/example/repo/pulls/1", changed);
                read.truncate(7);
                crate::providers::gh::GithubErrorKind::StaleHead
            }
            1 => {
                read[0] = gh_step("user", json!({"id":43,"login":"viewer"}));
                read.truncate(1);
                crate::providers::gh::GithubErrorKind::IdentityChanged
            }
            _ => {
                read[7] = gh_step("user", json!({"id":43,"login":"viewer"}));
                crate::providers::gh::GithubErrorKind::IdentityChanged
            }
        };
        let count = 3 + read.len();
        steps.extend(read);
        fixture.scripted_gh(&steps);
        let parent = load_parent(&fixture);
        let mut cache = Cache::memory().unwrap();
        let reviews = reserve(&mut cache, &parent, EventCategory::Reviews, CUTOFF);
        let timeline = reserve(&mut cache, &parent, EventCategory::Timeline, CUTOFF);
        let actual = gh(&fixture)
            .metric_history_backfill(
                &pair(&cache, &parent, &reviews, &timeline),
                &Default::default(),
                &CancellationToken::default(),
            )
            .unwrap_err()
            .kind;
        use crate::providers::gh::GithubErrorKind::{IdentityChanged, StaleHead};
        assert!(matches!(
            (actual, expected),
            (StaleHead, StaleHead) | (IdentityChanged, IdentityChanged)
        ));
        assert!(cache
            .sync_checkpoint(
                reviews.target().scope(),
                &reviews.target().source().unwrap()
            )
            .unwrap()
            .is_none());
        journal(&fixture, count);
    }
}
#[test]
fn metrics_history_future_cutoff_cancellation_and_capped_principal_stop_before_more_calls() {
    for case in 0..3 {
        let fixture = Fixture::new();
        let mut steps = inventory();
        if case == 2 {
            steps.push(gh_step("user", json!({"id":42,"login":"viewer"})));
        }
        fixture.scripted_gh(&steps);
        let parent = load_parent(&fixture);
        let mut cache = Cache::memory().unwrap();
        let cutoff = if case == 0 { u64::MAX } else { CUTOFF };
        let reviews = reserve(&mut cache, &parent, EventCategory::Reviews, cutoff);
        let timeline = reserve(&mut cache, &parent, EventCategory::Timeline, cutoff);
        let token = CancellationToken::default();
        if case == 1 {
            token.cancel();
        }
        let limits = crate::providers::gh::MetricsReadLimits {
            max_response_bytes: if case == 2 { 1 } else { 65536 },
            ..Default::default()
        };
        assert!(gh(&fixture)
            .metric_history_backfill(&pair(&cache, &parent, &reviews, &timeline), &limits, &token)
            .is_err());
        assert!(cache
            .sync_checkpoint(
                reviews.target().scope(),
                &reviews.target().source().unwrap()
            )
            .unwrap()
            .is_none());
        journal(&fixture, if case == 2 { 4 } else { 3 });
    }
}

#[test]
fn metrics_history_no_posthoc_promotion_and_partial_reread_preserves_old_interval_without_pruning()
{
    let fixture = Fixture::new();
    let mut steps = inventory();
    steps.extend(history(
        100,
        json!([
            review(201, "2026-10-02T00:00:00Z"),
            review(202, "2026-10-06T00:00:00Z")
        ]),
        json!([]),
    ));
    steps.extend(history(
        2,
        json!([
            review(201, "2026-10-02T00:00:00Z"),
            review(203, "2026-10-07T00:00:00Z")
        ]),
        json!([]),
    ));
    fixture.scripted_gh(&steps);
    let parent = load_parent(&fixture);
    let mut cache = Cache::memory().unwrap();
    let reviews = reserve(&mut cache, &parent, EventCategory::Reviews, CUTOFF);
    let timeline = reserve(&mut cache, &parent, EventCategory::Timeline, CUTOFF);
    let first = gh(&fixture)
        .metric_history_backfill(
            &pair(&cache, &parent, &reviews, &timeline),
            &Default::default(),
            &CancellationToken::default(),
        )
        .unwrap();
    let positive =
        AvailableMetricCategoryBatch::from_backfill(&cache, &reviews, &first, &mut Authority)
            .unwrap()
            .unwrap();
    cache
        .commit_metric_history(&reviews, positive, &mut Authority)
        .unwrap();
    let ordinary_timeline =
        MetricCategoryBatch::from_provider(first.observations(), EventCategory::Timeline).unwrap();
    cache
        .commit_metric_events(&timeline, &ordinary_timeline, &mut Authority)
        .unwrap();
    let late_positive =
        AvailableMetricCategoryBatch::from_backfill(&cache, &timeline, &first, &mut Authority)
            .unwrap()
            .unwrap();
    assert!(cache
        .commit_metric_history(&timeline, late_positive, &mut Authority)
        .is_err());
    let unchanged = cache
        .get(
            timeline.target().scope(),
            STATE,
            &timeline.target().source().unwrap(),
        )
        .unwrap()
        .unwrap();
    assert!(unchanged.payload.get("available_history").is_none());
    // Equal generation cannot change a committed source, including post-hoc upgrade.
    let repeated =
        AvailableMetricCategoryBatch::from_backfill(&cache, &reviews, &first, &mut Authority)
            .unwrap()
            .unwrap();
    assert!(cache
        .commit_metric_history(&reviews, repeated, &mut Authority)
        .is_err());
    let later_cutoff = 1791417600; // Requested 2026-10-08, not a claimed trend.
    let new_reviews = reserve(&mut cache, &parent, EventCategory::Reviews, later_cutoff);
    let new_timeline = reserve(&mut cache, &parent, EventCategory::Timeline, later_cutoff);
    let limits = crate::providers::gh::MetricsReadLimits {
        max_pages: 1,
        page_size: 2,
        ..Default::default()
    };
    let partial = gh(&fixture)
        .metric_history_backfill(
            &pair(&cache, &parent, &new_reviews, &new_timeline),
            &limits,
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(AvailableMetricCategoryBatch::from_backfill(
        &cache,
        &new_reviews,
        &partial,
        &mut Authority
    )
    .unwrap()
    .is_none());
    let current =
        MetricCategoryBatch::from_provider(partial.observations(), EventCategory::Reviews).unwrap();
    let frame = cache
        .commit_metric_events(&new_reviews, &current, &mut Authority)
        .unwrap();
    assert_eq!(frame.generation(), 2);
    assert_eq!(frame.facts.len(), 3); // Older missing202 and late203 both retained.
    let collection = frame
        .validated_projection(
            &MetricsScope::new(&identity(), &["example/repo".into()]).unwrap(),
            &parent,
        )
        .unwrap();
    assert_eq!(collection.through, Some(CUTOFF));
    assert!(!collection.covers_lifetime(1790812800, later_cutoff));
    assert_eq!(
        collection
            .events
            .iter()
            .map(|event| event.id)
            .collect::<Vec<_>>(),
        [201]
    );
    assert_eq!(
        cache
            .sync_members(
                new_reviews.target().scope(),
                &new_reviews.target().source().unwrap()
            )
            .unwrap()
            .len(),
        5
    );
    journal(&fixture, 19);
}

#[test]
fn metrics_history_partial_new_earlier_review_invalidates_basis_until_complete_reread() {
    use crate::metrics::{HistoryBasis, MetricKind, MetricTarget, MetricValue};
    let fixture = Fixture::new();
    let mut steps = inventory();
    steps.extend(history(
        100,
        json!([review(201, "2026-10-03T00:00:00Z")]),
        json!([]),
    ));
    let discovered = json!([
        review(201, "2026-10-03T00:00:00Z"),
        review(202, "2026-10-02T00:00:00Z")
    ]);
    steps.extend(history(2, discovered.clone(), json!([])));
    steps.extend(history(100, discovered, json!([])));
    fixture.scripted_gh(&steps);
    let parent = load_parent(&fixture);
    let mut cache = Cache::open(&fixture.root.join("history.sqlite")).unwrap();
    let reviews = reserve(&mut cache, &parent, EventCategory::Reviews, CUTOFF);
    let timeline = reserve(&mut cache, &parent, EventCategory::Timeline, CUTOFF);
    let complete = gh(&fixture)
        .metric_history_backfill(
            &pair(&cache, &parent, &reviews, &timeline),
            &Default::default(),
            &CancellationToken::default(),
        )
        .unwrap();
    let first = commit_available(&mut cache, &reviews, &complete);
    assert_eq!(project(&first, &parent).events.len(), 1);
    let reviews = reserve(&mut cache, &parent, EventCategory::Reviews, CUTOFF);
    let timeline = reserve(&mut cache, &parent, EventCategory::Timeline, CUTOFF);
    let partial = gh(&fixture)
        .metric_history_backfill(
            &pair(&cache, &parent, &reviews, &timeline),
            &crate::providers::gh::MetricsReadLimits {
                max_pages: 1,
                page_size: 2,
                ..Default::default()
            },
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(partial
        .category_proof(MetricsBackfillCategory::Reviews)
        .is_none());
    let frame = cache
        .commit_metric_events(
            &reviews,
            &MetricCategoryBatch::from_provider(partial.observations(), EventCategory::Reviews)
                .unwrap(),
            &mut Authority,
        )
        .unwrap();
    assert_eq!(frame.generation(), 2);
    assert_eq!(frame.facts.len(), 2, "Observed facts must not be pruned");
    assert!(frame.state.available_history.is_none());
    let unknown = project(&frame, &parent);
    assert_eq!(unknown.basis, HistoryBasis::Unavailable);
    assert!(!unknown.complete);
    let card = card_from_collection(&parent, unknown, MetricKind::FirstReview);
    assert_eq!(card.value, MetricValue::Unavailable);
    assert_eq!(card.unknown, 1);
    let reread = cache
        .read_metric_events(reviews.target(), &Default::default(), &mut Authority)
        .unwrap()
        .unwrap();
    assert_eq!(
        reread, frame,
        "Unavailable proof is durable across SQL reads"
    );
    let reviews = reserve(&mut cache, &parent, EventCategory::Reviews, CUTOFF);
    let timeline = reserve(&mut cache, &parent, EventCategory::Timeline, CUTOFF);
    let fresh = gh(&fixture)
        .metric_history_backfill(
            &pair(&cache, &parent, &reviews, &timeline),
            &Default::default(),
            &CancellationToken::default(),
        )
        .unwrap();
    let frame = commit_available(&mut cache, &reviews, &fresh);
    assert_eq!(frame.generation(), 3);
    let known = card_from_collection(&parent, project(&frame, &parent), MetricKind::FirstReview);
    assert_eq!(
        known.value,
        MetricValue::MeanSeconds {
            sum: 86400,
            samples: 1
        }
    );
    assert_eq!(known.unknown, 0);
    assert!(matches!(
        known.contributors[0].evidence.as_slice(),
        [MetricTarget::Review { review_id: 202, .. }]
    ));
    journal(&fixture, 27);
}

#[test]
fn metrics_history_partial_new_request_or_removal_never_inherits_complete_latest_wait() {
    use crate::metrics::{HistoryBasis, MetricKind, MetricValue};
    for name in ["review_requested", "review_request_removed"] {
        let fixture = Fixture::new();
        let mut requested_parent = parent();
        requested_parent["requested_reviewers"] = json!([{"id":42,"login":"viewer"}]);
        let mut steps = inventory();
        steps[1] = gh_step(
            "repos/example/repo/pulls?state=all&sort=created&direction=desc&per_page=100&page=1",
            json!([requested_parent]),
        );
        let mut first = history(100, json!([]), json!([event(900, "review_requested")]));
        for index in [2, 6] {
            first[index] = gh_step("repos/example/repo/pulls/1", requested_parent.clone());
        }
        steps.extend(first);
        let mut discovered = event(901, name);
        discovered["created_at"] = json!("2026-10-03T00:00:00Z");
        let mut partial = history(
            2,
            json!([]),
            json!([event(900, "review_requested"), discovered]),
        );
        for index in [2, 6] {
            partial[index] = gh_step("repos/example/repo/pulls/1", requested_parent.clone());
        }
        steps.extend(partial);
        fixture.scripted_gh(&steps);
        let parent = load_parent(&fixture);
        let mut cache = Cache::memory().unwrap();
        let reviews = reserve(&mut cache, &parent, EventCategory::Reviews, CUTOFF);
        let timeline = reserve(&mut cache, &parent, EventCategory::Timeline, CUTOFF);
        let full = gh(&fixture)
            .metric_history_backfill(
                &pair(&cache, &parent, &reviews, &timeline),
                &Default::default(),
                &CancellationToken::default(),
            )
            .unwrap();
        let frame = commit_available(&mut cache, &timeline, &full);
        let known = card_from_collection(&parent, project(&frame, &parent), MetricKind::ReviewWait);
        assert_eq!(
            known.value,
            MetricValue::MeanSeconds {
                sum: 259200,
                samples: 1
            }
        );
        let reviews = reserve(&mut cache, &parent, EventCategory::Reviews, CUTOFF);
        let timeline = reserve(&mut cache, &parent, EventCategory::Timeline, CUTOFF);
        let partial = gh(&fixture)
            .metric_history_backfill(
                &pair(&cache, &parent, &reviews, &timeline),
                &crate::providers::gh::MetricsReadLimits {
                    max_pages: 1,
                    page_size: 2,
                    ..Default::default()
                },
                &CancellationToken::default(),
            )
            .unwrap();
        let frame = cache
            .commit_metric_events(
                &timeline,
                &MetricCategoryBatch::from_provider(
                    partial.observations(),
                    EventCategory::Timeline,
                )
                .unwrap(),
                &mut Authority,
            )
            .unwrap();
        assert_eq!(frame.facts.len(), 2);
        assert!(frame.state.available_history.is_none());
        let current = project(&frame, &parent);
        assert_eq!(current.basis, HistoryBasis::Unavailable);
        assert_eq!(current.events.len(), 2);
        let unknown = card_from_collection(&parent, current, MetricKind::ReviewWait);
        assert_eq!(unknown.value, MetricValue::Unavailable);
        assert_eq!(unknown.unknown, 1);
        journal(&fixture, 19);
    }
}

#[test]
fn metrics_history_new_complete_view_cannot_bless_unmatched_prior_partial_facts() {
    use crate::metrics::{HistoryBasis, MetricKind, MetricValue};
    let fixture = Fixture::new();
    let mut steps = inventory();
    steps.extend(history(
        2,
        json!([
            review(201, "2026-10-02T00:00:00Z"),
            review(202, "2026-10-03T00:00:00Z")
        ]),
        json!([]),
    ));
    steps.extend(history(
        100,
        json!([review(201, "2026-10-02T00:00:00Z")]),
        json!([]),
    ));
    fixture.scripted_gh(&steps);
    let parent = load_parent(&fixture);
    let mut cache = Cache::memory().unwrap();
    let reviews = reserve(&mut cache, &parent, EventCategory::Reviews, CUTOFF);
    let timeline = reserve(&mut cache, &parent, EventCategory::Timeline, CUTOFF);
    let partial = gh(&fixture)
        .metric_history_backfill(
            &pair(&cache, &parent, &reviews, &timeline),
            &crate::providers::gh::MetricsReadLimits {
                max_pages: 1,
                page_size: 2,
                ..Default::default()
            },
            &CancellationToken::default(),
        )
        .unwrap();
    cache
        .commit_metric_events(
            &reviews,
            &MetricCategoryBatch::from_provider(partial.observations(), EventCategory::Reviews)
                .unwrap(),
            &mut Authority,
        )
        .unwrap();
    let reviews = reserve(&mut cache, &parent, EventCategory::Reviews, CUTOFF);
    let timeline = reserve(&mut cache, &parent, EventCategory::Timeline, CUTOFF);
    let full = gh(&fixture)
        .metric_history_backfill(
            &pair(&cache, &parent, &reviews, &timeline),
            &Default::default(),
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(full
        .category_proof(MetricsBackfillCategory::Reviews)
        .is_some());
    let frame = commit_available(&mut cache, &reviews, &full);
    assert_eq!(
        frame.facts.len(),
        2,
        "Missing prior partial fact remains retained"
    );
    assert!(frame.state.available_history.is_none());
    let current = project(&frame, &parent);
    assert_eq!(current.basis, HistoryBasis::Unavailable);
    let unknown = card_from_collection(&parent, current, MetricKind::FirstReview);
    assert_eq!(unknown.value, MetricValue::Unavailable);
    assert_eq!(unknown.unknown, 1);
    journal(&fixture, 19);
}

#[test]
fn metrics_history_new_complete_view_retains_prior_sealed_deleted_members() {
    use crate::metrics::{HistoryBasis, MetricKind, MetricValue};
    let fixture = Fixture::new();
    let mut steps = inventory();
    steps.extend(history(
        100,
        json!([review(201, "2026-10-02T00:00:00Z")]),
        json!([]),
    ));
    steps.extend(history(
        100,
        json!([review(202, "2026-10-03T00:00:00Z")]),
        json!([]),
    ));
    fixture.scripted_gh(&steps);
    let parent = load_parent(&fixture);
    let mut cache = Cache::memory().unwrap();
    for generation in 1..=2 {
        let reviews = reserve(&mut cache, &parent, EventCategory::Reviews, CUTOFF);
        let timeline = reserve(&mut cache, &parent, EventCategory::Timeline, CUTOFF);
        let full = gh(&fixture)
            .metric_history_backfill(
                &pair(&cache, &parent, &reviews, &timeline),
                &Default::default(),
                &CancellationToken::default(),
            )
            .unwrap();
        let frame = commit_available(&mut cache, &reviews, &full);
        assert_eq!(frame.generation(), generation);
        if generation == 2 {
            assert_eq!(frame.facts.len(), 2);
            let current = project(&frame, &parent);
            assert!(matches!(
                current.basis,
                HistoryBasis::ApiVisibleRetainedBackfill { .. }
            ));
            assert_eq!(current.events.len(), 2);
            let known = card_from_collection(&parent, current, MetricKind::FirstReview);
            assert_eq!(
                known.value,
                MetricValue::MeanSeconds {
                    sum: 86400,
                    samples: 1
                }
            );
            assert_eq!(known.unknown, 0);
            assert_eq!(frame.state.available_history.unwrap().members.len(), 2);
        }
    }
    journal(&fixture, 19);
}

#[test]
fn metrics_history_projection_rejects_changed_or_missing_original_creation_boundary() {
    let fixture = Fixture::new();
    let mut steps = inventory();
    steps.extend(history(
        100,
        json!([review(201, "2026-10-02T00:00:00Z")]),
        json!([]),
    ));
    let mut changed = parent();
    changed["created_at"] = json!("2026-10-02T00:00:00Z");
    steps.extend([
        gh_step("repos/example/repo", repo()),
        gh_step(
            "repos/example/repo/pulls?state=all&sort=created&direction=desc&per_page=100&page=1",
            json!([changed]),
        ),
        gh_step("repos/example/repo", repo()),
    ]);
    fixture.scripted_gh(&steps);
    let original = load_parent(&fixture);
    let mut cache = Cache::memory().unwrap();
    let reviews = reserve(&mut cache, &original, EventCategory::Reviews, CUTOFF);
    let timeline = reserve(&mut cache, &original, EventCategory::Timeline, CUTOFF);
    let full = gh(&fixture)
        .metric_history_backfill(
            &pair(&cache, &original, &reviews, &timeline),
            &Default::default(),
            &CancellationToken::default(),
        )
        .unwrap();
    let frame = commit_available(&mut cache, &reviews, &full);
    let changed = load_parent(&fixture);
    let scope = MetricsScope::new(&identity(), &["example/repo".into()]).unwrap();
    assert_eq!(changed.id, original.id);
    assert_eq!(changed.node_id, original.node_id);
    assert!(frame.validated_projection(&scope, &changed).is_err());
    let mut missing = original.clone();
    missing.created_at = None;
    assert!(frame.validated_projection(&scope, &missing).is_err());
    assert!(frame.validated_projection(&scope, &original).is_ok());
    journal(&fixture, 14);
}

#[test]
fn metrics_history_sealed_timeline_retains_team_and_unknown_targets_without_user_aliases() {
    use crate::metrics::{HistoryBasis, MetricKind, MetricValue};
    for unknown in [false, true] {
        let fixture = Fixture::new();
        let mut original = parent();
        original["requested_reviewers"] = json!([{"id":42,"login":"viewer"}]);
        let mut steps = inventory();
        steps[1] = gh_step(
            "repos/example/repo/pulls?state=all&sort=created&direction=desc&per_page=100&page=1",
            json!([original]),
        );
        let mut team = event(901, "review_requested");
        team.as_object_mut().unwrap().remove("requested_reviewer");
        team["requested_team"] = json!({"id":42,"node_id":"TEAM_42","slug":"reviewers"});
        team["created_at"] = json!("2026-10-03T00:00:00Z");
        let mut rows = vec![event(900, "review_requested"), team];
        if unknown {
            let mut removed = event(902, "review_request_removed");
            removed
                .as_object_mut()
                .unwrap()
                .remove("requested_reviewer");
            removed["created_at"] = json!("2026-10-04T00:00:00Z");
            rows.push(removed);
        }
        let mut traversal = history(100, json!([]), json!(rows));
        for index in [2, 6] {
            traversal[index] = gh_step("repos/example/repo/pulls/1", original.clone());
        }
        steps.extend(traversal);
        fixture.scripted_gh(&steps);
        let parent = load_parent(&fixture);
        let mut cache = Cache::open(&fixture.root.join("history.sqlite")).unwrap();
        let reviews = reserve(&mut cache, &parent, EventCategory::Reviews, CUTOFF);
        let timeline = reserve(&mut cache, &parent, EventCategory::Timeline, CUTOFF);
        let full = gh(&fixture)
            .metric_history_backfill(
                &pair(&cache, &parent, &reviews, &timeline),
                &Default::default(),
                &CancellationToken::default(),
            )
            .unwrap();
        assert!(full
            .category_proof(MetricsBackfillCategory::Timeline)
            .is_some());
        let frame = commit_available(&mut cache, &timeline, &full);
        assert_eq!(frame.facts.len(), if unknown { 3 } else { 2 });
        let stored_team = frame
            .facts
            .iter()
            .find(|fact| fact.event.id == 901)
            .unwrap();
        assert_eq!(stored_team.provider_node, "EV_901");
        assert!(matches!(
            stored_team.event.kind,
            EventKind::ReviewRequestedTeam { team_id: 42 }
        ));
        if unknown {
            assert!(frame.facts.iter().any(|fact| fact.event.id == 902
                && matches!(
                    fact.event.kind,
                    EventKind::ReviewRequestRemovedUnknownTarget
                )));
        }
        let collection = project(&frame, &parent);
        assert!(matches!(
            &collection.basis,
            HistoryBasis::ApiVisibleRetainedBackfill { .. }
        ));
        assert!(collection.complete && collection.covers_lifetime(1790812800, CUTOFF));
        let wait = card_from_collection(&parent, collection, MetricKind::ReviewWait);
        if unknown {
            assert_eq!(wait.value, MetricValue::Unavailable);
            assert_eq!(wait.unknown, 1);
        } else {
            assert_eq!(
                wait.value,
                MetricValue::MeanSeconds {
                    sum: 259200,
                    samples: 1
                }
            );
            assert_eq!(wait.unknown, 0);
            assert!(matches!(
                wait.contributors[0].evidence.as_slice(),
                [crate::metrics::MetricTarget::Event { event_id: 900, .. }]
            ));
        }
        assert_eq!(
            cache
                .read_metric_events(timeline.target(), &Default::default(), &mut Authority)
                .unwrap()
                .as_ref(),
            Some(&frame)
        );
        journal(&fixture, 11);
    }
}
