//! Strict repository inventory and current-event contracts.
use super::*;
use ghtui::providers::gh::{
    MetricsPrState, MetricsReadLimits, MetricsReadStatus, MetricsRepository,
    MetricsRequestedReviewer, MetricsReviewState, MetricsTimelineKind,
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
fn pull(number: u64) -> Value {
    json!({"id":100+number,"node_id":format!("PR_{number}"),"number":number,"title":format!("Repo-wide PR {number}"),"state":"open","draft":false,"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-12T00:00:00Z","merged_at":null,"closed_at":null,"merged":false,"head":{"sha":"a".repeat(40)},"base":{"repo":repo()},"user":{"id":51,"login":"alice","type":"User"},"requested_reviewers":[],"requested_teams":[],"url":format!("https://github.invalid/api/v3/repos/example/repo/pulls/{number}")})
}
fn budget() -> MetricsReadLimits {
    MetricsReadLimits {
        max_pages: 2,
        page_size: 2,
        max_rows: 10,
        max_document_bytes: 8192,
        max_response_bytes: 65536,
        max_duration: Duration::from_secs(10),
    }
}
fn event_budget() -> MetricsReadLimits {
    MetricsReadLimits {
        page_size: 100,
        max_rows: 1000,
        ..budget()
    }
}
fn get(endpoint: &str, value: Value) -> GhStep {
    step(api_args(endpoint), value)
}
fn inventory_page(page: u64, rows: Vec<Value>) -> GhStep {
    get(
        &format!(
            "repos/example/repo/pulls?state=all&sort=created&direction=desc&per_page=2&page={page}"
        ),
        json!(rows),
    )
}
fn inventory_steps(rows: Vec<Value>) -> Vec<GhStep> {
    vec![
        get("repos/example/repo", repo()),
        inventory_page(1, rows),
        get("repos/example/repo", repo()),
    ]
}
fn events_steps(parent: Value, reviews: Vec<Value>, timeline: Vec<Value>) -> Vec<GhStep> {
    vec![
        get("repos/example/repo", repo()),
        get("repos/example/repo/pulls/1", parent.clone()),
        get(
            "repos/example/repo/pulls/1/reviews?per_page=100&page=1",
            json!(reviews),
        ),
        get(
            "repos/example/repo/issues/1/timeline?per_page=100&page=1",
            json!(timeline),
        ),
        get("repos/example/repo", repo()),
        get("repos/example/repo/pulls/1", parent),
    ]
}
fn assert_reads(fixture: &Fixture, count: usize) {
    let rows = journal(fixture);
    assert_eq!(rows.len(), count);
    assert!(rows
        .iter()
        .all(|row| row["matched"] == true && row["args"][5] == "GET" && row["stdin_hex"] == ""));
    assert!(
        !rows
            .iter()
            .any(|row| row["args"][1].as_str().unwrap().starts_with("search/")),
        "Personal search/inbox cannot become repo denominator"
    );
}
#[test]
fn metrics_provider_invalid_scope_limits_and_pin_refuse_before_transport() {
    let fixture = Fixture::new();
    let gh = provider(&fixture);
    let valid = budget();
    for invalid in [
        MetricsReadLimits {
            max_pages: 0,
            ..valid.clone()
        },
        MetricsReadLimits {
            page_size: 0,
            ..valid.clone()
        },
        MetricsReadLimits {
            max_rows: 1,
            ..valid.clone()
        },
        MetricsReadLimits {
            max_document_bytes: 0,
            ..valid.clone()
        },
        MetricsReadLimits {
            max_response_bytes: 0,
            ..valid.clone()
        },
        MetricsReadLimits {
            max_duration: Duration::ZERO,
            ..valid.clone()
        },
    ] {
        assert!(gh
            .repository_pr_inventory(
                &current(),
                "example/repo",
                None,
                &invalid,
                &CancellationToken::default()
            )
            .is_err());
    }
    for account_id in [None, Some(0)] {
        assert!(gh
            .repository_pr_inventory(
                &HostIdentity {
                    account_id,
                    ..current()
                },
                "example/repo",
                None,
                &valid,
                &CancellationToken::default()
            )
            .is_err());
    }
    let pin = MetricsRepository {
        host: "github.invalid".into(),
        id: 0,
        node_id: "R_99".into(),
        full_name: "example/repo".into(),
    };
    assert!(gh
        .repository_pr_inventory(
            &current(),
            "example/repo",
            Some(&pin),
            &valid,
            &CancellationToken::default()
        )
        .is_err());
    assert!(journal(&fixture).is_empty());
}
#[test]
fn metrics_provider_repository_all_state_inventory_is_not_personal_queue_and_retains_nullable_facts(
) {
    let fixture = Fixture::new();
    let mut closed = pull(2);
    closed["state"] = json!("closed");
    closed["merged"] = json!(true);
    closed["merged_at"] = json!("2026-10-03T00:00:00Z");
    closed["closed_at"] = closed["merged_at"].clone();
    let mut draft = pull(1);
    draft["draft"] = json!(true);
    let mut steps = inventory_steps(vec![draft, closed]);
    steps.insert(2, inventory_page(2, vec![]));
    fixture.scripted_gh(&steps);
    let snapshot = provider(&fixture)
        .repository_pr_inventory(
            &current(),
            "example/repo",
            None,
            &budget(),
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(snapshot.prs.len(), 2);
    assert_eq!(snapshot.repository.id, 99);
    assert_eq!(snapshot.repository.node_id, "R_99");
    assert_eq!(snapshot.prs[0].draft, Some(true));
    assert!(matches!(snapshot.prs[1].state, MetricsPrState::Closed));
    assert_eq!(
        snapshot.prs[0].created_at.as_ref().unwrap().unix_seconds(),
        1790812800
    );
    assert_eq!(
        snapshot.prs[1].merged_at.as_ref().unwrap().raw(),
        "2026-10-03T00:00:00Z"
    );
    assert!(
        snapshot.coverage.enumeration_complete
            && snapshot.coverage.metric_evidence_complete
            && !snapshot.coverage.complete_empty
    );
    assert_eq!(snapshot.coverage.fetched_rows, 2);
    assert_eq!(snapshot.requests, 4);
    assert!(snapshot.observed_through >= snapshot.observed_from);
    assert_reads(&fixture, 4);
}
#[test]
fn metrics_provider_page_document_caps_and_complete_empty_never_hide_unobserved_cohort() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&inventory_steps(vec![pull(1), pull(2)]));
    let limits = MetricsReadLimits {
        max_pages: 1,
        max_rows: 2,
        ..budget()
    };
    let snapshot = provider(&fixture)
        .repository_pr_inventory(
            &current(),
            "example/repo",
            None,
            &limits,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(snapshot.prs.len(), 2);
    assert!(
        !snapshot.coverage.enumeration_complete
            && !snapshot.coverage.metric_evidence_complete
            && !snapshot.coverage.complete_empty
    );
    assert!(matches!(
        snapshot.coverage.status,
        MetricsReadStatus::Partial
    ));
    assert_reads(&fixture, 3);
    let fixture = Fixture::new();
    fixture.scripted_gh(&inventory_steps(vec![]));
    let empty = provider(&fixture)
        .repository_pr_inventory(
            &current(),
            "example/repo",
            None,
            &budget(),
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(
        empty.prs.is_empty()
            && empty.coverage.enumeration_complete
            && empty.coverage.metric_evidence_complete
            && empty.coverage.complete_empty
    );
    assert_reads(&fixture, 3);
    let fixture = Fixture::new();
    let mut huge = pull(1);
    huge["body"] = json!("private excluded body ".repeat(200));
    fixture.scripted_gh(&inventory_steps(vec![huge]));
    let excluded = provider(&fixture)
        .repository_pr_inventory(
            &current(),
            "example/repo",
            None,
            &MetricsReadLimits {
                max_document_bytes: 256,
                ..budget()
            },
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(
        excluded.prs.is_empty()
            && excluded.coverage.enumeration_complete
            && !excluded.coverage.metric_evidence_complete
            && !excluded.coverage.complete_empty
    );
    assert_eq!(excluded.coverage.excluded_rows, 1);
    assert_reads(&fixture, 3);
}

#[test]
fn metrics_provider_raw_capture_failed_shared_cap_exhaustion_cannot_launch_closing_scope_or_return_empty(
) {
    let fixture = Fixture::new();
    let mut unavailable = inventory_page(1, vec![]);
    unavailable.status = 1;
    unavailable.stderr = b"offline fixture transport".to_vec();
    fixture.scripted_gh(&[get("repos/example/repo", repo()), unavailable]);
    let error = provider(&fixture)
        .repository_pr_inventory(
            &current(),
            "example/repo",
            None,
            &budget(),
            &CancellationToken::default(),
        )
        .unwrap_err();
    assert!(matches!(error.kind, GithubErrorKind::Transport));
    assert!(error.message.contains("before another gh request"));
    assert_reads(&fixture, 2);
}

#[test]
fn metrics_provider_raw_capture_control_only_success_stderr_consumes_budget_before_first_list_page()
{
    let fixture = Fixture::new();
    let mut initial = get("repos/example/repo", repo());
    initial.stderr = [1u8, 2u8].into_iter().cycle().take(6000).collect();
    let exact_capture = initial.stdout.len() + initial.stderr.len();
    assert!(exact_capture < budget().max_document_bytes);
    fixture.scripted_gh(&[initial]);
    let limits = MetricsReadLimits {
        max_response_bytes: exact_capture,
        ..budget()
    };
    let error = provider(&fixture)
        .repository_pr_inventory(
            &current(),
            "example/repo",
            None,
            &limits,
            &CancellationToken::default(),
        )
        .unwrap_err();
    assert!(matches!(error.kind, GithubErrorKind::Transport));
    assert!(error.message.contains("before another gh request"));
    assert_reads(&fixture, 1);
}
#[test]
fn metrics_provider_same_name_repository_recreation_wrong_base_and_byte_exhaustion_fail_closed() {
    let fixture = Fixture::new();
    let mut replaced = repo();
    replaced["id"] = json!(100);
    replaced["node_id"] = json!("R_100");
    fixture.scripted_gh(&[
        get("repos/example/repo", repo()),
        inventory_page(1, vec![pull(1)]),
        get("repos/example/repo", replaced),
    ]);
    assert!(matches!(
        provider(&fixture)
            .repository_pr_inventory(
                &current(),
                "example/repo",
                None,
                &budget(),
                &CancellationToken::default()
            )
            .unwrap_err()
            .kind,
        GithubErrorKind::IdentityChanged
    ));
    assert_reads(&fixture, 3);
    let fixture = Fixture::new();
    let mut wrong = pull(1);
    wrong["base"]["repo"]["id"] = json!(200);
    fixture.scripted_gh(&[
        get("repos/example/repo", repo()),
        inventory_page(1, vec![wrong]),
    ]);
    assert!(provider(&fixture)
        .repository_pr_inventory(
            &current(),
            "example/repo",
            None,
            &budget(),
            &CancellationToken::default()
        )
        .is_err());
    assert_reads(&fixture, 2);
    let fixture = Fixture::new();
    let first = get("repos/example/repo", repo());
    let bytes = first.stdout.len();
    fixture.scripted_gh(&[first, inventory_page(1, vec![pull(1)])]);
    assert!(provider(&fixture)
        .repository_pr_inventory(
            &current(),
            "example/repo",
            None,
            &MetricsReadLimits {
                max_response_bytes: bytes + 1,
                ..budget()
            },
            &CancellationToken::default()
        )
        .is_err());
    assert_reads(&fixture, 2); // No extra final read after capped child.
}
fn review(id: u64, state: &str, at: Value) -> Value {
    json!({"id":id,"node_id":format!("REV_{id}"),"state":state,"submitted_at":at,"user":{"id":53,"login":"reviewer","type":"User"},"pull_request_url":"https://github.invalid/api/v3/repos/example/repo/pulls/1","commit_id":"a".repeat(40)})
}
fn event(id: u64, name: &str) -> Value {
    json!({"id":id,"node_id":format!("EV_{id}"),"event":name,"created_at":"2026-10-02T00:00:00Z","actor":{"id":54,"login":"requester","type":"User"},"url":format!("https://github.invalid/api/v3/repos/example/repo/issues/events/{id}")})
}
#[test]
fn metrics_provider_exact_pending_dismissed_rerequest_team_and_unknown_events_keep_distinct_evidence(
) {
    let fixture = Fixture::new();
    let parent = pull(1);
    let mut steps = inventory_steps(vec![parent.clone()]);
    let mut request = event(900, "review_requested");
    request["requested_reviewer"] = json!({"id":42,"login":"octocat","type":"User"});
    let mut removed = event(901, "review_request_removed");
    removed["requested_reviewer"] = request["requested_reviewer"].clone();
    let mut again = request.clone();
    again["id"] = json!(902);
    again["node_id"] = json!("EV_902");
    again["url"] = json!("https://github.invalid/api/v3/repos/example/repo/issues/events/902");
    let mut team = event(903, "review_requested");
    team["requested_team"] = json!({"id":42,"node_id":"T_42","slug":"security","name":"Security"});
    let mut dismissed = review(201, "DISMISSED", json!("2026-10-02T00:00:00Z"));
    dismissed["user"] = Value::Null;
    steps.extend(events_steps(
        parent,
        vec![review(200, "PENDING", Value::Null), dismissed],
        vec![
            request,
            removed,
            again,
            team,
            event(904, "future_review_reset"),
        ],
    ));
    fixture.scripted_gh(&steps);
    let gh = provider(&fixture);
    let inventory = gh
        .repository_pr_inventory(
            &current(),
            "example/repo",
            None,
            &budget(),
            &CancellationToken::default(),
        )
        .unwrap();
    let events = gh
        .pr_metric_events(
            &current(),
            &inventory.prs[0],
            &event_budget(),
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(
        matches!(events.reviews[0].state, MetricsReviewState::Pending)
            && events.reviews[0].submitted_at.is_none()
    );
    assert!(
        matches!(events.reviews[1].state, MetricsReviewState::Dismissed)
            && events.reviews[1].submitted_at.is_some()
    );
    assert_eq!(events.reviews[1].actor.actor.label(), "unknown actor");
    assert!(
        events.review_coverage.enumeration_complete
            && events.review_coverage.metric_evidence_complete
    );
    assert!(
        events.timeline_coverage.enumeration_complete
            && !events.timeline_coverage.metric_evidence_complete
    );
    assert!(
        matches!(events.timeline[2].kind,MetricsTimelineKind::ReviewRequested{reviewer:MetricsRequestedReviewer::User(ref actor)}if actor.account_id==Some(42))
    );
    assert!(
        matches!(events.timeline[3].kind,MetricsTimelineKind::ReviewRequested{reviewer:MetricsRequestedReviewer::Team(ref team)}if team.id==42)
    );
    assert!(matches!(
        events.timeline[4].kind,
        MetricsTimelineKind::Unsupported(_)
    ));
    assert_eq!(events.requests, 6);
    assert_reads(&fixture, 9);
}
#[test]
fn metrics_provider_missing_dates_draft_requests_and_malformed_submission_never_use_updated_time() {
    let fixture = Fixture::new();
    let mut parent = pull(1);
    parent["state"] = json!("closed");
    parent["draft"] = Value::Null;
    parent["created_at"] = Value::Null;
    parent["updated_at"] = json!("2026-12-31T00:00:00Z");
    parent
        .as_object_mut()
        .unwrap()
        .remove("requested_reviewers");
    fixture.scripted_gh(&inventory_steps(vec![parent]));
    let snapshot = provider(&fixture)
        .repository_pr_inventory(
            &current(),
            "example/repo",
            None,
            &budget(),
            &CancellationToken::default(),
        )
        .unwrap();
    let pr = &snapshot.prs[0];
    assert!(
        pr.created_at.is_none()
            && pr.merged_at.is_none()
            && pr.closed_at.is_none()
            && pr.draft.is_none()
            && pr.requested_users.is_none()
    );
    assert!(pr.observed_updated_at.is_some());
    assert!(!snapshot.coverage.metric_evidence_complete);
    assert_reads(&fixture, 3);
    let fixture = Fixture::new();
    let parent = pull(1);
    let mut steps = inventory_steps(vec![parent.clone()]);
    steps.extend(events_steps(
        parent,
        vec![review(201, "APPROVED", json!("not-a-time"))],
        vec![],
    ));
    fixture.scripted_gh(&steps);
    let gh = provider(&fixture);
    let inventory = gh
        .repository_pr_inventory(
            &current(),
            "example/repo",
            None,
            &budget(),
            &CancellationToken::default(),
        )
        .unwrap();
    let events = gh
        .pr_metric_events(
            &current(),
            &inventory.prs[0],
            &event_budget(),
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(
        events.reviews[0].submitted_at.is_none()
            && events.review_coverage.enumeration_complete
            && !events.review_coverage.metric_evidence_complete
    );
    assert_reads(&fixture, 9);
}
#[test]
fn metrics_provider_replaced_parent_and_wrong_review_destination_refuse_historical_authority() {
    for changed_review in [false, true] {
        let fixture = Fixture::new();
        let parent = pull(1);
        let mut steps = inventory_steps(vec![parent.clone()]);
        let mut events = events_steps(
            parent,
            vec![review(200, "APPROVED", json!("2026-10-02T00:00:00Z"))],
            vec![],
        );
        if changed_review {
            let mut wrong = review(200, "APPROVED", json!("2026-10-02T00:00:00Z"));
            wrong["pull_request_url"] =
                json!("https://github.invalid/api/v3/repos/example/repo/pulls/2");
            events[2] = get(
                "repos/example/repo/pulls/1/reviews?per_page=100&page=1",
                json!([wrong]),
            );
            events.truncate(3);
        } else {
            let mut replaced = pull(1);
            replaced["node_id"] = json!("PR_REPLACED");
            events[5] = get("repos/example/repo/pulls/1", replaced);
        }
        let event_count = events.len();
        steps.extend(events);
        fixture.scripted_gh(&steps);
        let gh = provider(&fixture);
        let inventory = gh
            .repository_pr_inventory(
                &current(),
                "example/repo",
                None,
                &budget(),
                &CancellationToken::default(),
            )
            .unwrap();
        assert!(matches!(
            gh.pr_metric_events(
                &current(),
                &inventory.prs[0],
                &event_budget(),
                &CancellationToken::default()
            )
            .unwrap_err()
            .kind,
            GithubErrorKind::IdentityChanged
        ));
        assert_reads(&fixture, 3 + event_count);
    }
}
#[test]
fn metrics_provider_cancelled_and_offline_inventory_is_not_complete_empty_or_silently_retried() {
    let fixture = Fixture::new();
    let token = CancellationToken::default();
    token.cancel();
    assert!(provider(&fixture)
        .repository_pr_inventory(&current(), "example/repo", None, &budget(), &token)
        .is_err());
    assert!(journal(&fixture).is_empty());
    let fixture = Fixture::new();
    let mut unavailable = inventory_page(1, vec![]);
    unavailable.status = 1;
    unavailable.stderr = b"offline fixture transport".to_vec();
    fixture.scripted_gh(&[
        get("repos/example/repo", repo()),
        unavailable,
        get("repos/example/repo", repo()),
    ]);
    // A failed list reserves its ONE configured 4 MiB shared capture cap.
    // Retaining an unavailable observation still requires genuine closing
    // repository access; explicitly budget those two canonical responses.
    let offline_budget = MetricsReadLimits {
        max_response_bytes: 4 * 1024 * 1024 + 2 * serde_json::to_vec(&repo()).unwrap().len(),
        ..budget()
    };
    let snapshot = provider(&fixture)
        .repository_pr_inventory(
            &current(),
            "example/repo",
            None,
            &offline_budget,
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(
        snapshot.prs.is_empty()
            && !snapshot.coverage.enumeration_complete
            && !snapshot.coverage.complete_empty
            && !snapshot.coverage.metric_evidence_complete
    );
    assert!(matches!(
        snapshot.coverage.status,
        MetricsReadStatus::Unavailable(_)
    ));
    assert_reads(&fixture, 3);
}
