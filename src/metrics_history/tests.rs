//! SOURCE-ONLY real strict provider + SQLite worker contracts. No module/root
//! exports/Cargo/native gate has run; declarations are not passing evidence.
use super::*;
use crate::{
    metrics::{MetricKind, MetricValue, UtcWindow},
    metrics_repository::repository_evidence,
    providers::gh::{GithubError, GithubErrorKind, MetricsReadLimits},
    storage::Scope,
    test_support::{Fixture, GhStep},
};
use serde_json::{json, Value};
#[path = "boundary_tests.rs"]
mod boundary_tests;
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

fn identity(uid: u64) -> HostIdentity {
    HostIdentity {
        host: "github.invalid".into(),
        login: "viewer".into(),
        account_id: Some(uid),
        gh_version: "fixture".into(),
    }
}
fn repo(tools: bool) -> Value {
    json!({"id":if tools{100}else{99},"node_id":if tools{"R_100"}else{"R_99"},"full_name":if tools{"example/tools"}else{"example/repo"}})
}
fn parent(number: u64) -> Value {
    let merged = number == 2;
    json!({"id":100+number,"node_id":format!("PR_{number}"),"number":number,"title":format!("Canonical history PR {number}"),"state":if merged{"closed"}else{"open"},"draft":false,"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-12T00:00:00Z","merged_at":if merged{json!("2026-10-03T00:00:00Z")}else{Value::Null},"closed_at":if merged{json!("2026-10-03T00:00:00Z")}else{Value::Null},"merged":merged,"head":{"sha":"a".repeat(40)},"base":{"repo":repo(false)},"user":{"id":51,"login":"alice"},"requested_reviewers":if merged{json!([])}else{json!([{"id":42,"login":"viewer"}])},"requested_teams":[]})
}
fn get(endpoint: &str, value: Value) -> GhStep {
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
fn user(uid: u64) -> GhStep {
    get("user", json!({"id":uid,"login":"viewer"}))
}
fn inventory(two: bool, tools: bool) -> Vec<GhStep> {
    let name = if tools {
        "example/tools"
    } else {
        "example/repo"
    };
    vec![
        get(&format!("repos/{name}"), repo(tools)),
        get(
            &format!(
                "repos/{name}/pulls?state=all&sort=created&direction=desc&per_page=100&page=1"
            ),
            if tools {
                json!([])
            } else if two {
                json!([parent(1), parent(2)])
            } else {
                json!([parent(1)])
            },
        ),
        get(&format!("repos/{name}"), repo(tools)),
    ]
}
fn canonical(number: u64) -> Vec<GhStep> {
    vec![
        get("repos/example/repo", repo(false)),
        get(
            &format!("repos/example/repo/pulls/{number}"),
            parent(number),
        ),
        get("repos/example/repo", repo(false)),
    ]
}
fn pre(number: u64) -> Vec<GhStep> {
    let mut reads = vec![user(42)];
    reads.extend(canonical(number));
    reads.push(user(42));
    reads
}
fn backfill(number: u64) -> Vec<GhStep> {
    let event = if number == 1 {
        json!({"id":900,"node_id":"EV_Request","event":"review_requested","created_at":"2026-10-02T00:00:00Z","actor":{"id":54,"login":"requester"},"requested_reviewer":{"id":42,"login":"viewer"}})
    } else {
        json!({"id":901,"node_id":"EV_Merge","event":"merged","created_at":"2026-10-03T00:00:00Z","actor":null})
    };
    vec![
        user(42),
        get("repos/example/repo", repo(false)),
        get(
            &format!("repos/example/repo/pulls/{number}"),
            parent(number),
        ),
        get(
            &format!("repos/example/repo/pulls/{number}/reviews?per_page=100&page=1"),
            json!([{"id":200+number,"node_id":format!("REV_{number}"),"state":"DISMISSED","submitted_at":"2026-10-02T00:00:00Z","user":null,"commit_id":null}]),
        ),
        get(
            &format!("repos/example/repo/issues/{number}/timeline?per_page=100&page=1"),
            json!([event]),
        ),
        get("repos/example/repo", repo(false)),
        get(
            &format!("repos/example/repo/pulls/{number}"),
            parent(number),
        ),
        user(42),
    ]
}
fn load_steps(two: bool) -> Vec<GhStep> {
    let mut reads = vec![];
    for number in 1..=if two { 2 } else { 1 } {
        reads.extend(pre(number));
        reads.extend(backfill(number));
    }
    reads
}
fn publication(two: bool, extra: bool) -> Vec<GhStep> {
    let mut reads = vec![user(42)];
    for number in 1..=if two { 2 } else { 1 } {
        reads.extend(canonical(number));
    }
    if extra {
        reads.push(get("repos/example/tools", repo(true)));
    }
    reads.push(user(42));
    reads
}
fn provider(fixture: &Fixture) -> GithubProvider {
    GithubProvider::new("github.invalid")
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"))
}
fn request(fixture: &Fixture, two: bool, extra: bool) -> MetricsHistoryRequest {
    let database = fixture.root.join("history.sqlite");
    let _cache = Cache::open(&database).unwrap();
    let guard = Arc::new(
        MetricsReadGuard::capture(
            &database,
            &Scope::github("github.invalid", Some(42)).unwrap(),
        )
        .unwrap(),
    );
    let gh = provider(fixture);
    let mut inventories = vec![gh
        .repository_pr_inventory(
            &identity(42),
            "example/repo",
            None,
            &MetricsReadLimits::default(),
            &CancellationToken::default(),
        )
        .unwrap()];
    if extra {
        inventories.push(
            gh.repository_pr_inventory(
                &identity(42),
                "example/tools",
                None,
                &MetricsReadLimits::default(),
                &CancellationToken::default(),
            )
            .unwrap(),
        );
    }
    let declared = if extra {
        vec!["example/repo".into(), "example/tools".into()]
    } else {
        vec!["example/repo".into()]
    };
    let evidence = Arc::new(repository_evidence(&identity(42), &declared, &inventories).unwrap());
    let as_of = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let snapshot =
        Arc::new(dashboard(&evidence, as_of, UtcWindow::new(1790812800, as_of).unwrap()).unwrap());
    let observations = Arc::from(
        inventories
            .iter()
            .flat_map(|inventory| inventory.prs.clone())
            .collect::<Vec<_>>(),
    );
    let repositories = Arc::from(
        inventories
            .iter()
            .map(|inventory| inventory.repository.clone())
            .collect::<Vec<_>>(),
    );
    let mut workspace =
        MetricsWorkspace::prepare(evidence, snapshot, declared, observations, repositories)
            .unwrap();
    workspace.read_guard = Some(guard);
    let selected_parents = workspace
        .evidence
        .prs
        .iter()
        .take(if two { 2 } else { 1 })
        .map(|pr| pr.target.clone())
        .collect();
    MetricsHistoryRequest {
        workspace,
        database,
        selected_parents,
        limits: MetricsHistoryLimits::default(),
    }
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
fn count_records(database: &PathBuf, kind: &str, uid: u64) -> u64 {
    rusqlite::Connection::open(database)
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM records WHERE host='github.invalid' AND login=?1 AND kind=?2",
            rusqlite::params![format!("user-id:{uid}"), kind],
            |row| row.get(0),
        )
        .unwrap()
}

#[test]
fn metrics_history_worker_two_real_parents_retained_values_ids_and_exact_two_phase_accounting() {
    let fixture = Fixture::new();
    let mut protocol = inventory(true, false);
    protocol.extend(load_steps(true));
    protocol.extend(publication(true, false));
    fixture.scripted_gh(&protocol);
    let request = request(&fixture, true, false);
    let as_of = request.workspace.snapshot.as_of;
    let database = request.database.clone();
    let pending = load_history(
        request,
        &identity(42),
        &provider(&fixture),
        &CancellationToken::default(),
    )
    .unwrap();
    let debug = format!("{pending:?}");
    assert!(
        !debug.contains("alice")
            && !debug.contains("example/repo")
            && !debug.contains("history.sqlite")
    );
    journal(&fixture, 29);
    let prepared = pending
        .publish(
            &identity(42),
            &provider(&fixture),
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(prepared.usage.requests, 34);
    assert_eq!(prepared.usage.requested_pages, 4);
    assert_eq!(prepared.usage.fetched_pages, 4);
    assert_eq!(prepared.usage.raw_rows, 4);
    assert_eq!(
        prepared.usage.successful_stdout_bytes,
        protocol[3..]
            .iter()
            .map(|step| step.stdout.len())
            .sum::<usize>()
    );
    assert_eq!(prepared.usage.successful_diagnostic_bytes, 0);
    assert_eq!(prepared.usage.failed_reserved_bytes, 0);
    assert_eq!(prepared.frames.len(), 4);
    assert!(
        prepared
            .frames
            .iter()
            .all(|frame| frame.generation() == 1
                && frame.original_intent().requested_through == as_of)
    );
    let card = |kind| {
        prepared
            .snapshot
            .cards
            .iter()
            .find(|card| card.kind == kind)
            .unwrap()
    };
    assert_eq!(
        card(MetricKind::ReviewWait).value,
        MetricValue::MeanSeconds {
            sum: u128::from(as_of - 1790899200),
            samples: 1
        }
    );
    assert_eq!(
        card(MetricKind::FirstReview).value,
        MetricValue::MeanSeconds {
            sum: 172800,
            samples: 2
        }
    );
    assert_eq!(
        card(MetricKind::MergeLatency).value,
        MetricValue::MeanSeconds {
            sum: 172800,
            samples: 1
        }
    );
    assert!(card(MetricKind::FirstReview)
        .contributors
        .iter()
        .all(|record| record.actors[1].account_id.is_none()));
    assert_eq!(count_records(&database, "metrics-state-v1", 42), 4);
    assert_eq!(count_records(&database, "metrics-event-v1", 42), 4);
    assert_eq!(count_records(&database, "metrics-state-v1", 43), 0);
    journal(&fixture, 37);
}

#[test]
fn metrics_history_worker_reused_login_new_uid_stops_before_reservation_or_private_row_reuse() {
    let fixture = Fixture::new();
    let mut protocol = inventory(false, false);
    protocol.push(user(43));
    fixture.scripted_gh(&protocol);
    let request = request(&fixture, false, false);
    let database = request.database.clone();
    let error = load_history(
        request,
        &identity(42),
        &provider(&fixture),
        &CancellationToken::default(),
    )
    .err()
    .unwrap();
    assert!(matches!(
        error.downcast_ref::<GithubError>().unwrap().kind,
        GithubErrorKind::IdentityChanged
    ));
    assert_eq!(count_records(&database, "metrics-reservation-v1", 42), 0);
    assert_eq!(count_records(&database, "metrics-event-v1", 43), 0);
    journal(&fixture, 4);
}

#[test]
fn metrics_history_worker_final_root_identity_switch_exposes_no_pending_evidence_and_sends_no_more_reads(
) {
    let fixture = Fixture::new();
    let mut protocol = inventory(false, false);
    protocol.extend(load_steps(false));
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
    assert!(pending
        .publish(
            &identity(43),
            &provider(&fixture),
            &CancellationToken::default()
        )
        .is_err());
    assert_eq!(count_records(&database, "metrics-state-v1", 42), 2);
    assert_eq!(count_records(&database, "metrics-state-v1", 43), 0);
    journal(&fixture, 16);
}

#[test]
fn metrics_history_worker_actual_clear_between_stages_fences_committed_epoch_without_rebasing() {
    let fixture = Fixture::new();
    let mut protocol = inventory(false, false);
    protocol.extend(load_steps(false));
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
    Cache::open(&database)
        .unwrap()
        .clear_records(&Scope::github("github.invalid", Some(42)).unwrap())
        .unwrap();
    assert!(pending
        .publish(
            &identity(42),
            &provider(&fixture),
            &CancellationToken::default()
        )
        .is_err());
    assert_eq!(count_records(&database, "metrics-event-v1", 42), 0);
    journal(&fixture, 16);
}

#[test]
fn metrics_history_worker_caller_short_lease_is_not_renewed_by_sql_or_positive_dto() {
    let fixture = Fixture::new();
    let mut protocol = inventory(false, false);
    protocol.extend(pre(1));
    fixture.scripted_gh(&protocol);
    let mut request = request(&fixture, false, false);
    request.limits.sql_lease = Duration::from_nanos(1);
    let database = request.database.clone();
    assert!(load_history(
        request,
        &identity(42),
        &provider(&fixture),
        &CancellationToken::default()
    )
    .is_err());
    assert_eq!(count_records(&database, "metrics-reservation-v1", 42), 0);
    journal(&fixture, 8);
}

#[test]
fn metrics_history_worker_global_request_cap_rejects_before_first_history_child_not_per_parent_reset(
) {
    let fixture = Fixture::new();
    fixture.scripted_gh(&inventory(true, false));
    let mut request = request(&fixture, true, false);
    request.limits.max_requests = 20;
    let database = request.database.clone();
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

#[test]
fn metrics_history_worker_cancel_after_load_preserves_original_facts_but_never_publishes_or_refetches(
) {
    let fixture = Fixture::new();
    let mut protocol = inventory(false, false);
    protocol.extend(load_steps(false));
    fixture.scripted_gh(&protocol);
    let request = request(&fixture, false, false);
    let database = request.database.clone();
    let token = CancellationToken::default();
    let pending = load_history(request, &identity(42), &provider(&fixture), &token).unwrap();
    token.cancel();
    assert!(pending
        .publish(&identity(42), &provider(&fixture), &token)
        .is_err());
    assert_eq!(count_records(&database, "metrics-state-v1", 42), 2);
    journal(&fixture, 16);
}

#[test]
fn metrics_history_worker_all_known_headers_require_real_access_even_when_history_selected_other_repo(
) {
    let fixture = Fixture::new();
    let mut protocol = inventory(false, false);
    protocol.extend(inventory(false, true));
    protocol.extend(load_steps(false));
    let mut final_reads = publication(false, true);
    final_reads.truncate(5);
    final_reads[4].status = 1;
    final_reads[4].stdout.clear();
    final_reads[4].stderr = b"gh: denied (HTTP 403)".to_vec();
    protocol.extend(final_reads);
    fixture.scripted_gh(&protocol);
    let request = request(&fixture, false, true);
    let database = request.database.clone();
    let pending = load_history(
        request,
        &identity(42),
        &provider(&fixture),
        &CancellationToken::default(),
    )
    .unwrap();
    let error = pending
        .publish(
            &identity(42),
            &provider(&fixture),
            &CancellationToken::default(),
        )
        .err()
        .unwrap();
    assert!(matches!(
        error.downcast_ref::<GithubError>().unwrap().kind,
        GithubErrorKind::Permission
    ));
    assert_eq!(count_records(&database, "metrics-state-v1", 42), 2);
    journal(&fixture, 24);
}

#[test]
fn metrics_history_worker_unavailable_capture_budget_cannot_create_source_after_partial_user_response(
) {
    let fixture = Fixture::new();
    let mut protocol = inventory(false, false);
    protocol.push(user(42));
    fixture.scripted_gh(&protocol);
    let mut request = request(&fixture, false, false);
    request.limits.max_charged_bytes = 10;
    let database = request.database.clone();
    assert!(load_history(
        request,
        &identity(42),
        &provider(&fixture),
        &CancellationToken::default()
    )
    .is_err());
    assert_eq!(count_records(&database, "metrics-reservation-v1", 42), 0);
    journal(&fixture, 4);
}
