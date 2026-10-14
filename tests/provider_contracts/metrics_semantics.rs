//! Source-only strict semantic contracts. Register only with the reviewed
//! history/domain/bridge activation; no code/native execution claim here.
use super::*;
use ghtui::providers::gh::{
    ExactPrMetricEvents, MetricsPrObservation, MetricsReadCoverage, MetricsReadLimits,
    MetricsReadStatus, MetricsRequestedReviewer, MetricsReviewState, MetricsTimelineKind,
};

fn current() -> HostIdentity {
    HostIdentity {
        account_id: Some(42),
        ..identity()
    }
}
fn repo() -> Value {
    json!({"id":99,"node_id":"R_99","full_name":"example/repo","url":"https://github.invalid/api/v3/repos/example/repo"})
}
fn pull() -> Value {
    json!({"id":101,"node_id":"PR_1","number":1,"title":"Metric semantic parent","state":"open","draft":false,"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-12T00:00:00Z","merged_at":null,"closed_at":null,"merged":false,"head":{"sha":"a".repeat(40)},"base":{"repo":repo()},"user":{"id":51,"login":"alice"},"requested_reviewers":[{"id":42,"login":"octocat"}],"requested_teams":[]})
}
fn review(id: u64) -> Value {
    json!({"id":id,"node_id":format!("REV_{id}"),"state":"DISMISSED","submitted_at":"2026-10-02T00:00:00Z","user":null,"commit_id":null,"pull_request_url":"https://github.invalid/api/v3/repos/example/repo/pulls/1"})
}
fn event(id: u64, name: &str) -> Value {
    json!({"id":id,"node_id":format!("EV_{id}"),"event":name,"created_at":"2026-10-02T00:00:00Z","actor":null,"url":format!("https://github.invalid/api/v3/repos/example/repo/issues/events/{id}")})
}
fn get(endpoint: &str, value: Value) -> GhStep {
    step(api_args(endpoint), value)
}
fn steps(reviews: Value, timeline: Value) -> Vec<GhStep> {
    vec![
        get("repos/example/repo", repo()),
        get(
            "repos/example/repo/pulls?state=all&sort=created&direction=desc&per_page=100&page=1",
            json!([pull()]),
        ),
        get("repos/example/repo", repo()),
        get("repos/example/repo", repo()),
        get("repos/example/repo/pulls/1", pull()),
        get(
            "repos/example/repo/pulls/1/reviews?per_page=100&page=1",
            reviews,
        ),
        get(
            "repos/example/repo/issues/1/timeline?per_page=100&page=1",
            timeline,
        ),
        get("repos/example/repo", repo()),
        get("repos/example/repo/pulls/1", pull()),
    ]
}
fn parent(fixture: &Fixture) -> MetricsPrObservation {
    provider(fixture)
        .repository_pr_inventory(
            &current(),
            "example/repo",
            None,
            &MetricsReadLimits::default(),
            &CancellationToken::default(),
        )
        .unwrap()
        .prs
        .remove(0)
}
fn load(fixture: &Fixture) -> ExactPrMetricEvents {
    let parent = parent(fixture);
    provider(fixture)
        .pr_metric_events(
            &current(),
            &parent,
            &MetricsReadLimits::default(),
            &CancellationToken::default(),
        )
        .unwrap()
}
fn reads(fixture: &Fixture, count: usize) {
    let rows = journal(fixture);
    assert_eq!(rows.len(), count);
    assert!(rows
        .iter()
        .all(|row| row["matched"] == true && row["args"][5] == "GET" && row["stdin_hex"] == ""));
}
fn complete(coverage: &MetricsReadCoverage) {
    assert!(coverage.enumeration_complete && coverage.metric_evidence_complete);
    assert!(matches!(coverage.status, MetricsReadStatus::Complete));
}

#[test]
fn metrics_semantic_provider_documented_harmless_events_keep_raw_nulls_without_blocking_history() {
    let names = [
        "assigned",
        "unassigned",
        "labeled",
        "unlabeled",
        "milestoned",
        "demilestoned",
        "referenced",
        "cross-referenced",
        "head_ref_force_pushed",
        "head_ref_deleted",
        "head_ref_restored",
        "base_ref_changed",
        "mentioned",
        "subscribed",
        "unsubscribed",
        "automatic_base_change_failed",
        "automatic_base_change_succeeded",
        "connected",
        "disconnected",
        "deployed",
        "deployment_environment_changed",
        "locked",
        "unlocked",
        "marked_as_duplicate",
        "unmarked_as_duplicate",
        "pinned",
        "unpinned",
        "renamed",
        "user_blocked",
    ];
    let rows = names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let mut row = event(900 + index as u64, name);
            row["created_at"] = json!("unusable unrelated date");
            row
        })
        .collect::<Vec<_>>();
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps(json!([]), json!(rows)));
    let result = load(&fixture);
    complete(&result.timeline_coverage);
    assert_eq!(result.timeline_coverage.fetched_rows, names.len());
    for (row, name) in result.timeline.iter().zip(names) {
        assert!(matches!(&row.kind, MetricsTimelineKind::Unrelated(actual) if actual == name));
        assert!(row.at.is_none() && row.actor.account_id.is_none());
        assert!(
            !row.reasons.is_empty(),
            "Raw malformed provenance remains visible"
        );
    }
    reads(&fixture, 9);
}

#[test]
fn metrics_semantic_provider_comment_source_url_is_typed_and_never_cross_origin_or_repository() {
    for url in [
        "https://github.invalid/api/v3/repos/example/repo/issues/comments/900",
        "https://evil.invalid/api/v3/repos/example/repo/issues/comments/900",
        "https://github.invalid/api/v3/repos/example/other/issues/comments/900",
        "https://github.invalid/api/v3/repos/example/repo/issues/events/900",
    ] {
        let fixture = Fixture::new();
        let mut row = event(900, "commented");
        row["url"] = json!(url);
        let mut protocol = steps(json!([]), json!([row]));
        let valid = url == "https://github.invalid/api/v3/repos/example/repo/issues/comments/900";
        if !valid {
            protocol.truncate(7);
        }
        fixture.scripted_gh(&protocol);
        let parent = parent(&fixture);
        let result = provider(&fixture).pr_metric_events(
            &current(),
            &parent,
            &MetricsReadLimits::default(),
            &CancellationToken::default(),
        );
        if valid {
            let result = result.unwrap();
            complete(&result.timeline_coverage);
            assert_eq!(result.timeline[0].id, Some(900));
            assert_eq!(result.timeline[0].node_id.as_deref(), Some("EV_900"));
            assert!(
                matches!(&result.timeline[0].kind, MetricsTimelineKind::Unrelated(name) if name == "commented")
            );
        } else {
            assert!(matches!(
                result.unwrap_err().kind,
                GithubErrorKind::IdentityChanged
            ));
        }
        reads(&fixture, if valid { 9 } else { 7 });
    }
}

#[test]
fn metrics_semantic_provider_commit_source_url_uses_actual_full_sha_not_id_or_current_head() {
    for (path, sha, valid) in [
        ("repos/example/repo/git/commits/", "b".repeat(40), true),
        ("repos/example/repo/commits/", "c".repeat(64), true),
        ("repos/example/other/git/commits/", "b".repeat(40), false),
        ("repos/example/repo/git/commits/", "bbbbbbb".into(), false),
    ] {
        let fixture = Fixture::new();
        let row = json!({"event":"committed","sha":sha,"node_id":"GenuineCommitNode","url":format!("https://github.invalid/api/v3/{path}{sha}"),"author":null});
        let mut protocol = steps(json!([]), json!([row]));
        if !valid {
            protocol.truncate(7);
        }
        fixture.scripted_gh(&protocol);
        let parent = parent(&fixture);
        let result = provider(&fixture).pr_metric_events(
            &current(),
            &parent,
            &MetricsReadLimits::default(),
            &CancellationToken::default(),
        );
        if valid {
            let result = result.unwrap();
            complete(&result.timeline_coverage);
            assert!(
                matches!(&result.timeline[0].kind, MetricsTimelineKind::Unrelated(name) if name == "committed")
            );
            assert!(result.timeline[0].id.is_none() && result.timeline[0].at.is_none());
        } else {
            assert!(result.is_err());
        }
        reads(&fixture, if valid { 9 } else { 7 });
    }
}

#[test]
fn metrics_semantic_provider_unused_review_commit_preserves_actorless_submission_and_reason() {
    for commit in [
        Value::Null,
        json!("bbbbbbb"),
        json!("z".repeat(40)),
        json!("b".repeat(40)),
    ] {
        let fixture = Fixture::new();
        let mut row = review(201);
        row["commit_id"] = commit.clone();
        fixture.scripted_gh(&steps(json!([row]), json!([])));
        let result = load(&fixture);
        complete(&result.review_coverage);
        assert!(matches!(
            result.reviews[0].state,
            MetricsReviewState::Dismissed
        ));
        assert_eq!(result.reviews[0].id, 201);
        assert_eq!(
            result.reviews[0]
                .submitted_at
                .as_ref()
                .unwrap()
                .unix_seconds(),
            1790899200
        );
        assert!(result.reviews[0].actor.account_id.is_none());
        let full = commit.as_str().is_some_and(|sha| sha == "b".repeat(40));
        assert_eq!(result.reviews[0].commit_sha.is_some(), full);
        if !commit.is_null() && !full {
            assert!(result.reviews[0]
                .reasons
                .iter()
                .any(|reason| reason.contains("commit")));
        }
        reads(&fixture, 9);
    }
}

#[test]
fn metrics_semantic_provider_redundant_timeline_review_metadata_cannot_erase_actual_review_source()
{
    let fixture = Fixture::new();
    let mut redundant = review(202);
    redundant["event"] = json!("reviewed");
    redundant["submitted_at"] = Value::Null;
    redundant["url"] =
        json!("https://github.invalid/api/v3/repos/example/repo/pulls/1/reviews/202");
    let mut dismissed = event(900, "review_dismissed");
    dismissed["dismissed_review"] = json!({"review_id":null});
    dismissed["created_at"] = json!("unknown unused dismissal time");
    fixture.scripted_gh(&steps(json!([review(201)]), json!([redundant, dismissed])));
    let result = load(&fixture);
    complete(&result.review_coverage);
    complete(&result.timeline_coverage);
    assert!(result.reviews[0].submitted_at.is_some());
    assert!(
        matches!(&result.timeline[0].kind, MetricsTimelineKind::ReviewSubmitted{review} if review.id == 202 && review.submitted_at.is_none())
    );
    assert!(matches!(
        result.timeline[1].kind,
        MetricsTimelineKind::ReviewDismissed { review_id: None }
    ));
    assert!(result
        .timeline
        .iter()
        .all(|row| row.at.is_none() && !row.reasons.is_empty()));
    reads(&fixture, 9);
}

#[test]
fn metrics_semantic_provider_documented_convert_to_draft_supported_unknown_future_stays_incomplete()
{
    for name in [
        "convert_to_draft",
        "converted_to_draft",
        "future_review_reset",
    ] {
        let fixture = Fixture::new();
        fixture.scripted_gh(&steps(json!([]), json!([event(900, name)])));
        let result = load(&fixture);
        assert!(result.timeline_coverage.enumeration_complete);
        if name == "future_review_reset" {
            assert!(!result.timeline_coverage.metric_evidence_complete);
            assert!(
                matches!(&result.timeline[0].kind, MetricsTimelineKind::Unsupported(actual) if actual == name)
            );
        } else {
            complete(&result.timeline_coverage);
            assert!(matches!(
                result.timeline[0].kind,
                MetricsTimelineKind::ConvertedToDraft
            ));
        }
        reads(&fixture, 9);
    }
}

#[test]
fn metrics_semantic_provider_missing_relevant_clock_identity_and_submission_are_not_zero_or_complete(
) {
    for field in ["created_at", "id", "node_id"] {
        let fixture = Fixture::new();
        let mut missing = event(900, "merged");
        missing[field] = Value::Null;
        fixture.scripted_gh(&steps(json!([]), json!([missing])));
        let result = load(&fixture);
        assert!(result.timeline_coverage.enumeration_complete);
        assert!(!result.timeline_coverage.metric_evidence_complete);
        assert!(!result.timeline_coverage.complete_empty);
        assert!(matches!(
            result.timeline[0].kind,
            MetricsTimelineKind::Merged
        ));
        reads(&fixture, 9);
    }
    let fixture = Fixture::new();
    let mut missing = review(201);
    missing["submitted_at"] = Value::Null;
    fixture.scripted_gh(&steps(json!([missing]), json!([])));
    let result = load(&fixture);
    assert!(!result.review_coverage.metric_evidence_complete);
    assert!(result.reviews[0].submitted_at.is_none());
    reads(&fixture, 9);
}

#[test]
fn metrics_semantic_provider_known_team_and_unknown_request_targets_remain_distinct_source_facts() {
    let fixture = Fixture::new();
    let mut direct = event(900, "review_requested");
    direct["requested_reviewer"] = json!({"id":42,"login":"octocat","type":"User"});
    let mut team = event(901, "review_requested");
    team["requested_team"] = json!({"id":42,"node_id":"TEAM_42","slug":"reviewers"});
    let unknown = event(902, "review_request_removed");
    fixture.scripted_gh(&steps(json!([]), json!([direct, team, unknown])));
    let result = load(&fixture);
    complete(&result.timeline_coverage);
    assert!(
        matches!(&result.timeline[0].kind, MetricsTimelineKind::ReviewRequested{reviewer:MetricsRequestedReviewer::User(actor)} if actor.account_id == Some(42))
    );
    assert!(
        matches!(&result.timeline[1].kind, MetricsTimelineKind::ReviewRequested{reviewer:MetricsRequestedReviewer::Team(team)} if team.id == 42)
    );
    assert!(matches!(
        result.timeline[2].kind,
        MetricsTimelineKind::ReviewRequestRemoved {
            reviewer: MetricsRequestedReviewer::Unknown
        }
    ));
    assert!(result.timeline[1..]
        .iter()
        .all(|row| !row.reasons.is_empty()));
    reads(&fixture, 9);
}

#[test]
fn metrics_semantic_provider_same_numeric_id_across_real_comment_review_event_types_is_not_alias() {
    let fixture = Fixture::new();
    let mut requested = event(900, "review_requested");
    requested["requested_reviewer"] = json!({"id":42,"login":"octocat","type":"User"});
    let mut commented = event(900, "commented");
    commented["node_id"] = json!("COMMENT_900");
    commented["url"] =
        json!("https://github.invalid/api/v3/repos/example/repo/issues/comments/900");
    let mut reviewed = review(900);
    reviewed["event"] = json!("reviewed");
    reviewed["url"] = json!("https://github.invalid/api/v3/repos/example/repo/pulls/1/reviews/900");
    fixture.scripted_gh(&steps(
        json!([review(900)]),
        json!([requested, commented, reviewed]),
    ));
    let result = load(&fixture);
    complete(&result.review_coverage);
    complete(&result.timeline_coverage);
    assert_eq!(result.timeline.len(), 3);
    assert!(result.timeline.iter().all(|row| row.id == Some(900)));
    assert_eq!(result.timeline[0].node_id.as_deref(), Some("EV_900"));
    assert_eq!(result.timeline[1].node_id.as_deref(), Some("COMMENT_900"));
    assert_eq!(result.timeline[2].node_id.as_deref(), Some("REV_900"));
    assert!(matches!(
        result.timeline[0].kind,
        MetricsTimelineKind::ReviewRequested { .. }
    ));
    assert!(matches!(
        result.timeline[1].kind,
        MetricsTimelineKind::Unrelated(_)
    ));
    assert!(matches!(
        result.timeline[2].kind,
        MetricsTimelineKind::ReviewSubmitted { .. }
    ));
    reads(&fixture, 9);
}

#[test]
fn metrics_semantic_provider_same_real_namespace_or_opaque_type_contradictions_refuse_before_final()
{
    for kind in ["commented", "reviewed", "issue-event", "opaque-cross-type"] {
        let fixture = Fixture::new();
        let (first, mut second) = match kind {
            "commented" => {
                let mut first = event(900, "commented");
                first["node_id"] = json!("COMMENT_900_A");
                first["url"] =
                    json!("https://github.invalid/api/v3/repos/example/repo/issues/comments/900");
                let mut second = first.clone();
                second["node_id"] = json!("COMMENT_900_B");
                (first, second)
            }
            "reviewed" => {
                let mut first = review(900);
                first["event"] = json!("reviewed");
                first["node_id"] = json!("REV_900_A");
                let mut second = first.clone();
                second["node_id"] = json!("REV_900_B");
                (first, second)
            }
            _ => {
                let mut first = event(900, "review_requested");
                first["requested_reviewer"] = json!({"id":42,"login":"octocat","type":"User"});
                (first, event(900, "locked"))
            }
        };
        if kind == "issue-event" {
            second["node_id"] = json!("EV_900_LOCKED");
        } else if kind == "opaque-cross-type" {
            second = event(901, "commented");
            second["node_id"] = first["node_id"].clone();
            second["url"] =
                json!("https://github.invalid/api/v3/repos/example/repo/issues/comments/901");
        }
        let mut protocol = steps(json!([]), json!([first, second]));
        protocol.truncate(7);
        fixture.scripted_gh(&protocol);
        let parent = parent(&fixture);
        assert!(matches!(
            provider(&fixture)
                .pr_metric_events(
                    &current(),
                    &parent,
                    &MetricsReadLimits::default(),
                    &CancellationToken::default()
                )
                .unwrap_err()
                .kind,
            GithubErrorKind::IdentityChanged
        ));
        reads(&fixture, 7);
    }
}

#[test]
fn metrics_semantic_provider_nullable_nodes_keep_actual_numeric_namespaces_or_unknown_observations()
{
    for other in ["locked", "future_unknown_event", "committed"] {
        let fixture = Fixture::new();
        let mut comment = event(900, "commented");
        comment["node_id"] = Value::Null;
        comment["url"] =
            json!("https://github.invalid/api/v3/repos/example/repo/issues/comments/900");
        let mut second = event(900, other);
        second["node_id"] = Value::Null;
        if other == "committed" {
            second["sha"] = json!("c".repeat(40));
            second["url"] = json!(format!(
                "https://github.invalid/api/v3/repos/example/repo/commits/{}",
                "c".repeat(40)
            ));
        }
        fixture.scripted_gh(&steps(json!([]), json!([comment, second])));
        let result = load(&fixture);
        assert_eq!(
            result.timeline.len(),
            2,
            "Distinct actual resource kinds must not collide at the page collector"
        );
        assert!(result
            .timeline
            .iter()
            .all(|row| row.node_id.is_none() && row.id == Some(900)));
        assert!(result.timeline_coverage.enumeration_complete);
        if other == "future_unknown_event" {
            assert!(!result.timeline_coverage.metric_evidence_complete);
            assert!(matches!(
                result.timeline[1].kind,
                MetricsTimelineKind::Unsupported(_)
            ));
        } else {
            complete(&result.timeline_coverage);
        }
        reads(&fixture, 9);
    }
}

#[test]
fn metrics_semantic_provider_same_typed_numeric_resource_without_node_rejects_changed_observation()
{
    let fixture = Fixture::new();
    let mut first = event(900, "commented");
    first["node_id"] = Value::Null;
    first["url"] = json!("https://github.invalid/api/v3/repos/example/repo/issues/comments/900");
    let mut second = first.clone();
    second["created_at"] = json!("2026-10-03T00:00:00Z");
    let mut protocol = steps(json!([]), json!([first, second]));
    protocol.truncate(7);
    fixture.scripted_gh(&protocol);
    let parent = parent(&fixture);
    assert!(matches!(
        provider(&fixture)
            .pr_metric_events(
                &current(),
                &parent,
                &MetricsReadLimits::default(),
                &CancellationToken::default()
            )
            .unwrap_err()
            .kind,
        GithubErrorKind::IdentityChanged
    ));
    reads(&fixture, 7);
}
