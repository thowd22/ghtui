//! Staged strict SQLite/provider bridge contracts. No native Dashboard claim.
use super::*;
use crate::providers::{gh::GithubProvider, runner::CancellationToken};
use crate::test_support::{Fixture, GhStep};
use serde_json::json;
use std::{fs, process::Command};

#[derive(Default)]
struct Authority {
    phases: Vec<MetricAuthorityPhase>,
    refuse: Option<MetricAuthorityPhase>,
}
impl MetricsRetentionAuthority for Authority {
    fn verify(&mut self, target: &MetricEventTarget, phase: MetricAuthorityPhase) -> Result<()> {
        target.validate()?;
        self.phases.push(phase);
        if self.refuse == Some(phase) {
            bail!("Strict original-principal/access/journal authority refused");
        }
        Ok(())
    }
}
fn fixture_target(category: EventCategory) -> MetricEventTarget {
    MetricEventTarget {
        scope: Scope::github("github.invalid", Some(42)).unwrap(),
        repository: "example/repo".into(),
        repository_id: 99,
        repository_node: "R_99".into(),
        pr_id: 101,
        pr_node: "PR_1".into(),
        number: 1,
        category,
    }
}
fn intent() -> MetricReadIntent {
    MetricReadIntent {
        requested_from: Some(10),
        requested_through: 20,
    }
}
fn fact(target: &MetricEventTarget, id: u64) -> Fact {
    Fact {
        source: target.source().unwrap(),
        provider_node: format!("NODE_{id}"),
        event: SourceEvent {
            id,
            at: 11,
            kind: match target.category {
                EventCategory::Reviews => EventKind::ReviewSubmitted {
                    review_id: id,
                    state: SubmittedReviewState::Approved,
                },
                EventCategory::Timeline => EventKind::ReviewRequested { reviewer_id: 42 },
            },
            actor: MetricActor {
                account_id: Some(53),
                actor: crate::providers::gh::Actor {
                    login: "original-actor".into(),
                    actor_type: "User".into(),
                    ..Default::default()
                },
            },
        },
    }
}
fn batch(target: &MetricEventTarget, facts: Vec<Fact>, complete: bool) -> MetricCategoryBatch {
    MetricCategoryBatch {
        target: target.clone(),
        coverage: ObservationCoverage {
            enumeration_complete: complete,
            semantic_complete: complete,
            fetched_pages: 1,
            fetched_rows: facts.len(),
            excluded_rows: 0,
            response_bytes: 100,
            unavailable: false,
            reasons: if complete {
                vec![]
            } else {
                vec!["Bounded partial page".into()]
            },
        },
        facts,
        observed_from: 20,
        observed_through: 21,
    }
}
fn commit(
    cache: &mut Cache,
    target: &MetricEventTarget,
    batch: &MetricCategoryBatch,
) -> StoredMetricFrame {
    let mut authority = Authority::default();
    let permit = cache
        .reserve_metric_events(
            target,
            &intent(),
            &MetricsRetentionLimits::default(),
            &mut authority,
        )
        .unwrap();
    let frame = cache
        .commit_metric_events(&permit, batch, &mut authority)
        .unwrap();
    assert_eq!(
        authority.phases,
        [
            MetricAuthorityPhase::BeforeReserve,
            MetricAuthorityPhase::AfterReserve,
            MetricAuthorityPhase::BeforeCommit,
            MetricAuthorityPhase::AfterCommit
        ]
    );
    frame
}
fn checkpoint(cache: &Cache, target: &MetricEventTarget) -> Option<(u64, bool)> {
    cache
        .sync_checkpoint(&target.scope, &target.source().unwrap())
        .unwrap()
}

#[test]
fn metrics_retention_sql_actual_generation_original_intent_and_restart_not_lifetime() {
    let fixture = Fixture::new();
    let path = fixture.root.join("metrics.sqlite");
    let target = fixture_target(EventCategory::Timeline);
    let mut cache = Cache::open(&path).unwrap();
    let mut original = intent();
    let mut authority = Authority::default();
    let permit = cache
        .reserve_metric_events(
            &target,
            &original,
            &MetricsRetentionLimits::default(),
            &mut authority,
        )
        .unwrap();
    original.requested_from = Some(19); // Editing a later request cannot rewrite the permit.
    let frame = cache
        .commit_metric_events(
            &permit,
            &batch(&target, vec![fact(&target, 900)], true),
            &mut authority,
        )
        .unwrap();
    assert_eq!(frame.original_intent(), &intent());
    assert_eq!(frame.generation(), 1);
    assert_eq!(frame.observed_interval(), (20, 21));
    assert_eq!(checkpoint(&cache, &target), Some((1, false)));
    assert_eq!(
        cache
            .sync_members(&target.scope, &target.source().unwrap())
            .unwrap()
            .len(),
        3
    );
    drop(cache);
    let mut reopened = Cache::open(&path).unwrap();
    frame
        .validate(
            &reopened,
            &MetricsRetentionLimits::default(),
            &mut Authority::default(),
        )
        .unwrap();
    let next = reopened
        .reserve_metric_events(
            &target,
            &original,
            &MetricsRetentionLimits::default(),
            &mut Authority::default(),
        )
        .unwrap();
    assert_eq!(next.generation(), 2);
    assert!(frame
        .validate(
            &reopened,
            &MetricsRetentionLimits::default(),
            &mut Authority::default()
        )
        .is_err());
    assert_eq!(checkpoint(&reopened, &target), Some((1, false)));
    assert_eq!(next.original_intent().requested_from, Some(19));
}
#[test]
fn metrics_retention_partial_and_current_complete_empty_union_never_prune_or_promote_history() {
    let mut cache = Cache::memory().unwrap();
    let target = fixture_target(EventCategory::Timeline);
    let first = commit(
        &mut cache,
        &target,
        &batch(&target, vec![fact(&target, 900)], true),
    );
    let second = commit(
        &mut cache,
        &target,
        &batch(&target, vec![fact(&target, 901)], false),
    );
    assert_eq!(
        second
            .facts
            .iter()
            .map(|fact| fact.event.id)
            .collect::<Vec<_>>(),
        [900, 901]
    );
    assert_eq!(second.generation(), 2);
    assert!(!second.state.coverage.enumeration_complete);
    let empty = commit(&mut cache, &target, &batch(&target, vec![], true));
    assert_eq!(empty.facts.len(), 2);
    assert_eq!(empty.state.coverage.fetched_rows, 0);
    assert_eq!(empty.state.history_unavailable, HISTORY_UNAVAILABLE);
    assert_eq!(checkpoint(&cache, &target), Some((3, false)));
    assert!(first
        .validate(
            &cache,
            &MetricsRetentionLimits::default(),
            &mut Authority::default()
        )
        .is_err());
}
#[test]
fn metrics_retention_exact_account_repo_parent_and_category_cannot_alias() {
    let mut cache = Cache::memory().unwrap();
    let target = fixture_target(EventCategory::Timeline);
    let permit = cache
        .reserve_metric_events(
            &target,
            &intent(),
            &MetricsRetentionLimits::default(),
            &mut Authority::default(),
        )
        .unwrap();
    for variation in 0..5 {
        let mut other = target.clone();
        match variation {
            0 => other.scope = Scope::github("github.invalid", Some(43)).unwrap(),
            1 => other.repository_id = 100,
            2 => other.repository_node = "R_replaced".into(),
            3 => other.pr_id = 102,
            _ => other.category = EventCategory::Reviews,
        }
        assert_ne!(other.source().unwrap(), target.source().unwrap());
        assert!(cache
            .commit_metric_events(
                &permit,
                &batch(&other, vec![fact(&other, 900)], true),
                &mut Authority::default()
            )
            .is_err());
        assert!(cache
            .read_metric_events(
                &other,
                &MetricsRetentionLimits::default(),
                &mut Authority::default()
            )
            .unwrap()
            .is_none());
    }
    assert!(checkpoint(&cache, &target).is_none());
    assert!(cache
        .sync_members(&target.scope, &target.source().unwrap())
        .unwrap()
        .is_empty());
}
#[test]
fn metrics_retention_newer_generation_clear_and_revocation_fence_late_collection() {
    for operation in 0..3 {
        let mut cache = Cache::memory().unwrap();
        let target = fixture_target(EventCategory::Timeline);
        let old = cache
            .reserve_metric_events(
                &target,
                &intent(),
                &MetricsRetentionLimits::default(),
                &mut Authority::default(),
            )
            .unwrap();
        match operation {
            0 => {
                cache
                    .reserve_metric_events(
                        &target,
                        &intent(),
                        &MetricsRetentionLimits::default(),
                        &mut Authority::default(),
                    )
                    .unwrap();
            }
            1 => cache.clear_records(&target.scope).unwrap(),
            _ => {
                assert_eq!(
                    cache
                        .revoke_repository(&target.scope, target.repository())
                        .unwrap(),
                    1
                );
            }
        }
        assert!(cache
            .commit_metric_events(
                &old,
                &batch(&target, vec![fact(&target, 900)], true),
                &mut Authority::default()
            )
            .is_err());
        assert!(checkpoint(&cache, &target).is_none());
        assert!(cache
            .sync_members(&target.scope, &target.source().unwrap())
            .unwrap()
            .is_empty());
        assert_eq!(
            cache
                .sync_reservation(&target.scope, &target.source().unwrap())
                .unwrap()
                .is_some(),
            operation == 0
        );
    }
}
#[test]
fn metrics_retention_original_request_sql_edit_cannot_authorize_commit() {
    let mut cache = Cache::memory().unwrap();
    let target = fixture_target(EventCategory::Timeline);
    let permit = cache
        .reserve_metric_events(
            &target,
            &intent(),
            &MetricsRetentionLimits::default(),
            &mut Authority::default(),
        )
        .unwrap();
    let mut record = cache
        .get(&target.scope, RESERVATION, &target.source().unwrap())
        .unwrap()
        .unwrap();
    record.payload["intent"]["requested_through"] = json!(9999);
    cache.put(&target.scope, &record).unwrap();
    assert!(cache
        .commit_metric_events(
            &permit,
            &batch(&target, vec![fact(&target, 900)], true),
            &mut Authority::default()
        )
        .is_err());
    assert_eq!(permit.original_intent(), &intent());
    assert!(checkpoint(&cache, &target).is_none());
}
#[test]
fn metrics_retention_request_time_target_actor_immutable_but_dismissal_preserves_submission() {
    let mut cache = Cache::memory().unwrap();
    let target = fixture_target(EventCategory::Timeline);
    commit(
        &mut cache,
        &target,
        &batch(&target, vec![fact(&target, 900)], true),
    );
    for change in 0..3 {
        let mut changed = fact(&target, 900);
        match change {
            0 => changed.event.at = 12,
            1 => changed.provider_node = "EV_REPLACED".into(),
            _ => changed.event.actor.account_id = Some(54),
        }
        let permit = cache
            .reserve_metric_events(
                &target,
                &intent(),
                &MetricsRetentionLimits::default(),
                &mut Authority::default(),
            )
            .unwrap();
        assert!(cache
            .commit_metric_events(
                &permit,
                &batch(&target, vec![changed], true),
                &mut Authority::default()
            )
            .is_err());
        assert_eq!(checkpoint(&cache, &target), Some((1, false)));
    }
    let target = fixture_target(EventCategory::Reviews);
    commit(
        &mut cache,
        &target,
        &batch(&target, vec![fact(&target, 200)], true),
    );
    let mut dismissed = fact(&target, 200);
    dismissed.event.kind = EventKind::ReviewSubmitted {
        review_id: 200,
        state: SubmittedReviewState::Dismissed,
    };
    dismissed.event.actor.account_id = None;
    dismissed.event.actor.actor.login = "renamed-actor".into();
    let frame = commit(&mut cache, &target, &batch(&target, vec![dismissed], true));
    assert_eq!(frame.facts[0].event.at, 11);
    assert_eq!(frame.facts[0].event.actor.account_id, Some(53));
    assert_eq!(frame.facts[0].event.actor.actor.label(), "@renamed-actor");
    assert!(matches!(
        frame.facts[0].event.kind,
        EventKind::ReviewSubmitted {
            state: SubmittedReviewState::Dismissed,
            ..
        }
    ));
    let permit = cache
        .reserve_metric_events(
            &target,
            &intent(),
            &MetricsRetentionLimits::default(),
            &mut Authority::default(),
        )
        .unwrap();
    assert!(cache
        .commit_metric_events(
            &permit,
            &batch(&target, vec![fact(&target, 200)], true),
            &mut Authority::default()
        )
        .is_err());
}
#[test]
fn metrics_retention_corrupt_payload_or_missing_actual_state_member_refuses_snapshot() {
    for membership in [false, true] {
        let mut cache = Cache::memory().unwrap();
        let target = fixture_target(EventCategory::Timeline);
        let frame = commit(
            &mut cache,
            &target,
            &batch(&target, vec![fact(&target, 900)], true),
        );
        let source = target.source().unwrap();
        if membership {
            cache.connection.execute("DELETE FROM checkpoint_records WHERE host=?1 AND login=?2 AND source=?3 AND kind=?4", params![target.scope.host,target.scope.login,source,STATE]).unwrap();
        } else {
            let id = store::event_key(&source, &frame.facts[0]).unwrap();
            let mut record = cache.get(&target.scope, EVENT, &id).unwrap().unwrap();
            record.payload["event"]["actor"]["actor"]["login"] = json!("corrupted actor bytes");
            cache.put(&target.scope, &record).unwrap();
        }
        assert!(cache
            .read_metric_events(
                &target,
                &MetricsRetentionLimits::default(),
                &mut Authority::default()
            )
            .is_err());
        assert!(frame
            .validate(
                &cache,
                &MetricsRetentionLimits::default(),
                &mut Authority::default()
            )
            .is_err());
        assert_eq!(checkpoint(&cache, &target), Some((1, false)));
    }
}
#[test]
fn metrics_retention_utf8_preflight_happens_before_invalid_json_is_parsed() {
    let mut cache = Cache::memory().unwrap();
    let target = fixture_target(EventCategory::Timeline);
    let frame = commit(
        &mut cache,
        &target,
        &batch(&target, vec![fact(&target, 900)], true),
    );
    let id = store::event_key(&target.source().unwrap(), &frame.facts[0]).unwrap();
    let oversized_invalid_json = "é".repeat(150);
    cache
        .connection
        .execute(
            "UPDATE records SET payload=?1 WHERE host=?2 AND login=?3 AND kind=?4 AND id=?5",
            params![
                oversized_invalid_json,
                target.scope.host,
                target.scope.login,
                EVENT,
                id
            ],
        )
        .unwrap();
    let error = cache
        .read_metric_events(
            &target,
            &MetricsRetentionLimits {
                max_document_bytes: 256,
                ..Default::default()
            },
            &mut Authority::default(),
        )
        .unwrap_err();
    assert!(error.to_string().contains("footprint"), "{error}");
    assert!(!error.to_string().contains("expected value"));
    assert_eq!(
        cache
            .sync_footprint(&target.scope, &target.source().unwrap())
            .unwrap()
            .0,
        3
    );
}
#[test]
fn metrics_retention_failed_fresh_authority_is_not_access_and_post_commit_effect_remains_observable(
) {
    let mut cache = Cache::memory().unwrap();
    let target = fixture_target(EventCategory::Timeline);
    let mut denied = Authority {
        refuse: Some(MetricAuthorityPhase::BeforeReserve),
        ..Default::default()
    };
    assert!(cache
        .reserve_metric_events(
            &target,
            &intent(),
            &MetricsRetentionLimits::default(),
            &mut denied
        )
        .is_err());
    assert!(cache
        .sync_reservation(&target.scope, &target.source().unwrap())
        .unwrap()
        .is_none());
    let permit = cache
        .reserve_metric_events(
            &target,
            &intent(),
            &MetricsRetentionLimits::default(),
            &mut Authority::default(),
        )
        .unwrap();
    let mut denied = Authority {
        refuse: Some(MetricAuthorityPhase::AfterCommit),
        ..Default::default()
    };
    assert!(cache
        .commit_metric_events(
            &permit,
            &batch(&target, vec![fact(&target, 900)], true),
            &mut denied
        )
        .is_err());
    assert_eq!(
        denied.phases,
        [
            MetricAuthorityPhase::BeforeCommit,
            MetricAuthorityPhase::AfterCommit
        ]
    );
    assert_eq!(checkpoint(&cache, &target), Some((1, false))); // Stored, but no private output returned.
    let mut denied = Authority {
        refuse: Some(MetricAuthorityPhase::BeforeRead),
        ..Default::default()
    };
    assert!(cache
        .read_metric_events(&target, &MetricsRetentionLimits::default(), &mut denied)
        .is_err());
    assert_eq!(denied.phases, [MetricAuthorityPhase::BeforeRead]);
    let frame = cache
        .read_metric_events(
            &target,
            &MetricsRetentionLimits::default(),
            &mut Authority::default(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(frame.facts[0].event.id, 900);
}

#[test]
fn metrics_retention_final_authority_sql_fence_cannot_return_an_earlier_private_frame() {
    struct FencingAuthority {
        database: std::path::PathBuf,
        phase: MetricAuthorityPhase,
        fired: bool,
    }
    impl MetricsRetentionAuthority for FencingAuthority {
        fn verify(
            &mut self,
            target: &MetricEventTarget,
            phase: MetricAuthorityPhase,
        ) -> Result<()> {
            if phase == self.phase {
                let mut other = Cache::open(&self.database)?;
                other.clear_records(target.scope())?;
                self.fired = true;
            }
            Ok(())
        }
    }
    for phase in [
        MetricAuthorityPhase::AfterRead,
        MetricAuthorityPhase::AfterCommit,
    ] {
        let fixture = Fixture::new();
        let database = fixture.root.join("metrics-race.sqlite");
        let mut cache = Cache::open(&database).unwrap();
        let target = fixture_target(EventCategory::Timeline);
        let mut authority = FencingAuthority {
            database,
            phase,
            fired: false,
        };
        if phase == MetricAuthorityPhase::AfterRead {
            let frame = commit(
                &mut cache,
                &target,
                &batch(&target, vec![fact(&target, 900)], true),
            );
            assert!(cache
                .read_metric_events(&target, &MetricsRetentionLimits::default(), &mut authority)
                .is_err());
            assert!(frame
                .validate(
                    &cache,
                    &MetricsRetentionLimits::default(),
                    &mut Authority::default()
                )
                .is_err());
        } else {
            let reservation = cache
                .reserve_metric_events(
                    &target,
                    &intent(),
                    &MetricsRetentionLimits::default(),
                    &mut Authority::default(),
                )
                .unwrap();
            assert!(cache
                .commit_metric_events(
                    &reservation,
                    &batch(&target, vec![fact(&target, 900)], true),
                    &mut authority
                )
                .is_err());
        }
        assert!(authority.fired);
        assert!(checkpoint(&cache, &target).is_none());
        assert!(cache
            .sync_members(target.scope(), &target.source().unwrap())
            .unwrap()
            .is_empty());
    }
}

fn gh_read(endpoint: &str, value: Value) -> GhStep {
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
#[test]
fn metrics_retention_strict_provider_bridge_does_not_mint_history_or_team_viewer_membership() {
    let fixture = Fixture::new();
    let directory = fixture.repo();
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .args(args)
            .current_dir(&directory)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    };
    git(&["init", "--initial-branch=main"]);
    git(&["config", "user.name", "Metrics fixture"]);
    git(&["config", "user.email", "metrics@fixture.invalid"]);
    fs::write(directory.join("baseline.txt"), b"baseline\n").unwrap();
    git(&["add", "--", "baseline.txt"]);
    git(&["commit", "-m", "metrics baseline"]);
    fs::write(
        directory.join("dirty.txt"),
        b"untouched private dirty bytes\n",
    )
    .unwrap();
    let before = git(&["status", "--porcelain=v1", "-z"]);
    let repo = json!({"id":99,"node_id":"R_99","full_name":"example/repo"});
    let parent = json!({"id":101,"node_id":"PR_1","number":1,"title":"All-state PR","state":"open","draft":false,"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-12T00:00:00Z","head":{"sha":"a".repeat(40)},"merged":false,"merged_at":null,"closed_at":null,"base":{"repo":repo},"user":{"id":51,"login":"alice"},"requested_reviewers":[],"requested_teams":[]});
    let reviews = json!([
        {"id":200,"node_id":"REV_200","state":"PENDING","submitted_at":null,"user":{"id":53,"login":"reviewer"}},
        {"id":201,"node_id":"REV_201","state":"DISMISSED","submitted_at":"2026-10-02T00:00:00Z","user":{"id":53,"login":"reviewer"}}
    ]);
    let timeline = json!([
        {"id":900,"node_id":"EV_900","event":"review_requested","created_at":"2026-10-02T00:00:00Z","actor":{"id":54,"login":"requester"},"requested_reviewer":{"id":42,"login":"viewer"}},
        {"id":901,"node_id":"EV_901","event":"review_requested","created_at":"2026-10-03T00:00:00Z","actor":{"id":54,"login":"requester"},"requested_team":{"id":42,"node_id":"T_42","slug":"team"}},
        {"id":902,"node_id":"EV_902","event":"unsupported_future_review_change","created_at":"2026-10-04T00:00:00Z","actor":null}
    ]);
    fixture.scripted_gh(&[
        gh_read("repos/example/repo", repo.clone()),
        gh_read(
            "repos/example/repo/pulls?state=all&sort=created&direction=desc&per_page=100&page=1",
            json!([parent.clone()]),
        ),
        gh_read("repos/example/repo", repo.clone()),
        gh_read("repos/example/repo", repo.clone()),
        gh_read("repos/example/repo/pulls/1", parent.clone()),
        gh_read(
            "repos/example/repo/pulls/1/reviews?per_page=100&page=1",
            reviews,
        ),
        gh_read(
            "repos/example/repo/issues/1/timeline?per_page=100&page=1",
            timeline,
        ),
        gh_read("repos/example/repo", repo),
        gh_read("repos/example/repo/pulls/1", parent),
    ]);
    let gh = GithubProvider::new("github.invalid")
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"));
    let identity = HostIdentity {
        host: "github.invalid".into(),
        login: "viewer".into(),
        account_id: Some(42),
        gh_version: "fixture".into(),
    };
    let cancel = CancellationToken::default();
    let inventory = gh
        .repository_pr_inventory(
            &identity,
            "example/repo",
            None,
            &Default::default(),
            &cancel,
        )
        .unwrap();
    let observed = gh
        .pr_metric_events(&identity, &inventory.prs[0], &Default::default(), &cancel)
        .unwrap();
    let mut cache = Cache::memory().unwrap();
    let scope = MetricsScope::new(&identity, &["example/repo".into()]).unwrap();
    for category in [EventCategory::Reviews, EventCategory::Timeline] {
        let batch = MetricCategoryBatch::from_provider(&observed, category).unwrap();
        let frame = commit(&mut cache, &batch.target, &batch);
        let projection = frame
            .validated_projection(&scope, &observed.parent)
            .unwrap();
        assert!(!projection.complete && projection.from.is_none() && projection.through.is_none());
        assert_eq!(projection.generation, 1);
        assert_eq!(projection.events.len(), 1);
        assert!(!projection.covers_lifetime(1790812800, 1790899200));
        assert_eq!(
            projection.binding.as_ref().unwrap().parent.database_id,
            Some(101)
        );
        match category {
            EventCategory::Reviews => assert!(matches!(
                projection.events[0].kind,
                EventKind::ReviewSubmitted {
                    review_id: 201,
                    state: SubmittedReviewState::Dismissed
                }
            )),
            EventCategory::Timeline => {
                assert!(matches!(
                    projection.events[0].kind,
                    EventKind::ReviewRequested { reviewer_id: 42 }
                ));
                assert_eq!(projection.events[0].actor.account_id, Some(54));
                assert!(!frame.state.coverage.semantic_complete);
            }
        }
        let mut replaced = observed.parent.clone();
        replaced.id = 102;
        assert!(frame.validated_projection(&scope, &replaced).is_err());
    }
    let journal = fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(journal.len(), 9);
    assert!(journal
        .iter()
        .all(|row| row["matched"] == true && row["args"][5] == "GET" && row["stdin_hex"] == ""));
    assert_eq!(git(&["status", "--porcelain=v1", "-z"]), before);
    assert_eq!(
        fs::read(directory.join("dirty.txt")).unwrap(),
        b"untouched private dirty bytes\n"
    );
}
