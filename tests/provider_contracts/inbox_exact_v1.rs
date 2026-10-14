//! SOURCE-ONLY future child of provider_contracts; real strict transport + SQLite.
//! No public JSON/access constructor, ignored test or accepted-default snapshot.
use super::*;
use ghtui::{
    github_workspace::{Freshness, GithubWorkspace},
    storage::{Cache, Record, Scope},
};
use rusqlite::{Connection, OpenFlags};
use std::path::Path;

const REPO: &str = "example/repo";
const OLD: &str = "PRIVATE_OLD_EXACT_SCHEMA_DRAFT";

fn current(host: &str) -> HostIdentity {
    HostIdentity {
        host: host.into(),
        login: "octocat".into(),
        account_id: Some(42),
        gh_version: "Original display only".into(),
    }
}
fn api_host(host: &str) -> String {
    if host == "github.com" {
        "api.github.com".into()
    } else if host == "tenant.ghe.com" {
        "api.tenant.ghe.com".into()
    } else {
        host.into()
    }
}
fn raw_pr(host: &str, repo: &str, number: u64, id: u64) -> Value {
    let mut value = pr(number, repo);
    let api = api_host(host);
    let prefix = if host == "enterprise.invalid" {
        "api/v3/"
    } else {
        ""
    };
    value["id"] = json!(id);
    value["node_id"] = json!(format!("OBSERVED_issue_search_{id}"));
    value["repository_url"] = json!(format!("https://{api}/{prefix}repos/{repo}"));
    value["html_url"] = json!(format!("https://{host}/{repo}/pull/{number}"));
    value["url"] = json!(format!(
        "https://{api}/{prefix}repos/{repo}/issues/{number}"
    ));
    value["pull_request"] = json!({
        "url":format!("https://{api}/{prefix}repos/{repo}/pulls/{number}"),
        "html_url":format!("https://{host}/{repo}/pull/{number}"),
    });
    value
}
fn get_for(host: &str, endpoint: &str, value: Value) -> GhStep {
    let mut args = api_args(endpoint);
    args[3] = host.into();
    step(args, value)
}
fn account(host: &str) -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.99.0 (2026-10-01)\n"),
        get_for(host, "user", json!({"login":"octocat","id":42})),
    ]
}
fn page(host: &str, queue: &str, repository: &str, number: usize, value: Value) -> GhStep {
    let mut args = api_args("search/issues");
    args[3] = host.into();
    args.extend([
        "-f".into(),
        format!("q=is:pr is:open {queue}:octocat repo:{repository}"),
        "-f".into(),
        "per_page=100".into(),
        "-f".into(),
        format!("page={number}"),
    ]);
    step(args, value)
}
fn response(total: u64, items: Vec<Value>, incomplete: bool) -> Value {
    json!({"total_count":total,"incomplete_results":incomplete,"items":items})
}
fn tail(host: &str) -> Vec<GhStep> {
    vec![
        page(host, "assignee", REPO, 1, response(0, vec![], false)),
        page(
            host,
            "review-requested",
            REPO,
            1,
            response(0, vec![], false),
        ),
    ]
}
fn seed(fixture: &Fixture, host: &str) -> std::path::PathBuf {
    let path = fixture.root.join("exact-schema.sqlite3");
    let mut cache = Cache::open(&path).unwrap();
    let scope = Scope::github(host, Some(42)).unwrap();
    let workspace = GithubWorkspace::empty(current(host), vec![REPO.into()], Default::default());
    let source = workspace.source();
    cache.put_draft(&scope, "original-review", OLD).unwrap();
    let old = ghtui::providers::gh::PullRequest {
        host: host.into(),
        id: "old_observed_node".into(),
        repository: REPO.into(),
        number: 91,
        title: "Old row must not become fresh zero".into(),
        body: OLD.into(),
        url: format!("https://{host}/{REPO}/pull/91"),
        author: Default::default(),
        state: "open".into(),
        draft: false,
        created_at: String::new(),
        updated_at: String::new(),
        queues: [Queue::Assigned].into_iter().collect(),
    };
    assert!(cache
        .commit_sync(
            &scope,
            &source,
            4,
            &[
                Record::new(
                    "inbox-meta",
                    &source,
                    "",
                    json!({"coverage":{"loaded":1,"total_matches":1,"complete":true,"reasons":[]}})
                ),
                Record::new(
                    "pr",
                    format!("{REPO}#91"),
                    REPO,
                    serde_json::to_value(old).unwrap()
                ),
            ],
            true
        )
        .unwrap());
    path
}
fn sql_state(path: &Path) -> Vec<(String, String, String, String)> {
    let raw = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let mut query = raw.prepare("SELECT host,login,kind||':'||id,payload FROM records UNION ALL SELECT host,login,'draft:'||resource,body FROM drafts UNION ALL SELECT host,login,'checkpoint:'||source,CAST(generation AS TEXT)||':'||complete FROM checkpoints ORDER BY 1,2,3").unwrap();
    let rows = query
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap();
    rows.collect::<rusqlite::Result<_>>().unwrap()
}
fn check_journal(fixture: &Fixture, steps: &[GhStep]) {
    let actual = journal(fixture);
    assert_eq!(actual.len(), steps.len());
    for (actual, expected) in actual.iter().zip(steps) {
        assert_eq!(actual["matched"], true);
        assert_eq!(actual["args"], json!(expected.args));
        assert_eq!(actual["stdin_hex"], "");
        assert!(expected
            .args
            .iter()
            .all(|arg| arg != "POST" && arg != "PATCH" && arg != "DELETE"));
    }
}
fn run(fixture: &Fixture, host: &str, path: &Path) -> anyhow::Result<GithubWorkspace> {
    let provider = GithubProvider::new(host)
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"));
    GithubWorkspace::empty(current(host), vec![REPO.into()], Default::default())
        .refresh_exact_scope(
            &Scope::github(host, Some(42)).unwrap(),
            &provider,
            path,
            &Default::default(),
        )
}
fn refuses(value: Value, reason: &str) {
    let fixture = Fixture::new();
    let path = seed(&fixture, "github.invalid");
    let before = sql_state(&path);
    let mut steps = account("github.invalid");
    steps.push(page("github.invalid", "author", REPO, 1, value));
    fixture.scripted_gh(&steps);
    let error = run(&fixture, "github.invalid", &path)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains(reason),
        "Expected static refusal, got {error}"
    );
    assert!(!error.contains(OLD));
    assert!(!error.contains("PRIVATE_SEARCH_CANARY"));
    assert_eq!(
        sql_state(&path),
        before,
        "No false-complete/pruning/checkpoint/draft publication"
    );
    check_journal(&fixture, &steps); // Identity2 + first actual page only.
}

#[test]
fn repo_catalog_exact_schema_genuine_empty_requires_all_three_observed_queue_envelopes() {
    let fixture = Fixture::new();
    let path = seed(&fixture, "github.invalid");
    let mut steps = account("github.invalid");
    steps.push(page(
        "github.invalid",
        "author",
        REPO,
        1,
        response(0, vec![], false),
    ));
    steps.extend(tail("github.invalid"));
    fixture.scripted_gh(&steps);
    let loaded = run(&fixture, "github.invalid", &path).unwrap();
    assert!(loaded.inbox.pull_requests.is_empty());
    assert!(loaded.inbox.coverage.complete);
    assert_eq!(loaded.inbox.coverage.total_matches, 0);
    assert_eq!(loaded.generation, 5);
    assert!(matches!(loaded.freshness, Freshness::Fresh { .. }));
    let cache = Cache::open(&path).unwrap();
    assert_eq!(
        cache
            .draft(&loaded.scope().unwrap(), "original-review")
            .unwrap()
            .as_deref(),
        Some(OLD)
    );
    check_journal(&fixture, &steps);
}
#[test]
fn repo_catalog_exact_schema_unknown_total_or_incomplete_never_defaults_successful_zero() {
    for field in ["total_count", "incomplete_results"] {
        let mut values = vec![
            None,
            Some(Value::Null),
            Some(json!("0")),
            Some(json!(-1)),
            Some(json!({})),
            Some(json!(1.5)),
        ];
        if field == "total_count" {
            values.push(Some(json!(false)));
        }
        for value in values {
            let mut raw = response(0, vec![], false);
            match value {
                None => {
                    raw.as_object_mut().unwrap().remove(field);
                }
                Some(value) => raw[field] = value,
            }
            refuses(
                raw,
                if field == "total_count" {
                    "total_count"
                } else {
                    "incomplete_results"
                },
            );
        }
    }
}
#[test]
fn repo_catalog_exact_schema_missing_nonarray_and_over100_items_refuse_before_admission() {
    for value in [
        None,
        Some(Value::Null),
        Some(json!({})),
        Some(json!("PRIVATE_SEARCH_CANARY")),
    ] {
        let mut raw = response(0, vec![], false);
        match value {
            None => {
                raw.as_object_mut().unwrap().remove("items");
            }
            Some(value) => raw["items"] = value,
        }
        refuses(raw, "items array");
    }
    refuses(
        response(
            101,
            (1..=101)
                .map(|n| raw_pr("github.invalid", REPO, n, 2000 + n))
                .collect(),
            false,
        ),
        "100-row page",
    );
}
#[test]
fn repo_catalog_exact_schema_positive_id_number_and_actual_opaque_node_are_not_fabricated() {
    for field in ["id", "number", "node_id"] {
        for value in [
            Value::Null,
            json!(0),
            json!(-1),
            json!(""),
            json!("PRIVATE_SEARCH_CANARY\u{001b}"),
            json!({}),
        ] {
            let mut raw = raw_pr("github.invalid", REPO, 7, 2007);
            raw[field] = value;
            refuses(
                response(1, vec![raw], false),
                if field == "node_id" {
                    "canonical string"
                } else {
                    "positive numeric identity"
                },
            );
        }
    }
}
#[test]
fn repo_catalog_exact_schema_pr_marker_and_origin_kind_number_are_actual_not_repo_url_split() {
    for mutation in 0..9 {
        let mut raw = raw_pr("github.invalid", REPO, 7, 2007);
        let reason = match mutation {
            0 => {
                raw.as_object_mut().unwrap().remove("pull_request");
                "pull_request linkage"
            }
            1 => {
                raw["pull_request"] = Value::Null;
                "pull_request linkage"
            }
            2 => {
                raw["repository_url"] = json!("https://foreign.invalid/repos/example/repo");
                "source URL"
            }
            3 => {
                raw["html_url"] = json!("https://github.invalid/example/repo/issues/7");
                "source URL"
            }
            4 => {
                raw["pull_request"]["url"] =
                    json!("https://github.invalid/repos/example/repo/pulls/8");
                "source URL"
            }
            5 => {
                raw["repository_url"] = json!("https://github.invalid/repos/example%2Frepo");
                "source URL"
            }
            6 => {
                raw["html_url"] = json!("https://github.invalid/example/repo/pull/7#private");
                "source URL"
            }
            7 => {
                raw["repository_url"] =
                    json!("https://github.invalid/repos/example/repo?retarget=1");
                "source URL"
            }
            _ => {
                raw["url"] = json!("https://github.invalid/repos/foreign/repo/issues/7");
                "source URL"
            }
        };
        refuses(response(1, vec![raw], false), reason);
    }
}
#[test]
fn repo_catalog_exact_schema_canonical_public_enterprise_residency_origins_and_repo_case_work() {
    for host in [
        "github.invalid",
        "github.com",
        "enterprise.invalid",
        "tenant.ghe.com",
    ] {
        let fixture = Fixture::new();
        let path = seed(&fixture, host);
        let mut steps = account(host);
        steps.push(page(
            host,
            "author",
            REPO,
            1,
            response(1, vec![raw_pr(host, "Example/Repo", 7, 2007)], false),
        ));
        steps.extend(tail(host));
        fixture.scripted_gh(&steps);
        let loaded = run(&fixture, host, &path).unwrap();
        assert_eq!(loaded.inbox.pull_requests[0].repository, REPO);
        assert_eq!(
            loaded.inbox.pull_requests[0].id,
            "OBSERVED_issue_search_2007"
        );
        assert_eq!(loaded.inbox.pull_requests[0].number, 7);
        assert!(loaded.inbox.coverage.complete);
        assert_eq!(loaded.identity.host, host);
        check_journal(&fixture, &steps);
    }
}
#[test]
fn repo_catalog_exact_schema_duplicate_queue_rows_do_not_satisfy_reported_unique_total() {
    let fixture = Fixture::new();
    let path = seed(&fixture, "github.invalid");
    let row = raw_pr("github.invalid", REPO, 7, 2007);
    let mut steps = account("github.invalid");
    steps.push(page(
        "github.invalid",
        "author",
        REPO,
        1,
        response(2, vec![row.clone(), row], false),
    ));
    steps.push(page(
        "github.invalid",
        "author",
        REPO,
        2,
        response(2, vec![], false),
    ));
    steps.extend(tail("github.invalid"));
    fixture.scripted_gh(&steps);
    let loaded = run(&fixture, "github.invalid", &path).unwrap();
    assert_eq!(loaded.inbox.pull_requests.len(), 1);
    assert_eq!(loaded.inbox.coverage.total_matches, 2);
    assert!(!loaded.inbox.coverage.complete);
    assert!(loaded
        .inbox
        .coverage
        .reasons
        .iter()
        .any(|reason| reason.contains("count twice")));
    assert!(loaded
        .inbox
        .coverage
        .reasons
        .iter()
        .any(|reason| reason.contains("1 of 2")));
    let raw = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let old_members:u64=raw.query_row("SELECT count(*) FROM records WHERE host='github.invalid' AND login='user-id:42' AND kind='pr' AND id='example/repo#91'",[],|row|row.get(0)).unwrap();
    assert_eq!(
        old_members, 1,
        "Partial enumeration cannot prune original retained records"
    );
    check_journal(&fixture, &steps); // Exactly6; duplicate never hides needed page2.
}
#[test]
fn repo_catalog_exact_schema_moving_totals_incomplete_and_known_caps_preserve_partial_not_absence()
{
    for (first, second, incomplete) in [
        (2, 1, false),
        (2, 0, false),
        (1001, 1001, false),
        (2, 2, true),
    ] {
        let fixture = Fixture::new();
        let path = seed(&fixture, "github.invalid");
        let mut steps = account("github.invalid");
        steps.push(page(
            "github.invalid",
            "author",
            REPO,
            1,
            response(
                first,
                vec![raw_pr("github.invalid", REPO, 7, 2007)],
                incomplete,
            ),
        ));
        steps.push(page(
            "github.invalid",
            "author",
            REPO,
            2,
            response(second, vec![], false),
        ));
        steps.extend(tail("github.invalid"));
        fixture.scripted_gh(&steps);
        let loaded = run(&fixture, "github.invalid", &path).unwrap();
        assert_eq!(loaded.inbox.pull_requests.len(), 1);
        assert_eq!(loaded.inbox.coverage.total_matches, first);
        assert!(!loaded.inbox.coverage.complete);
        assert!(!loaded.inbox.coverage.reasons.is_empty());
        check_journal(&fixture, &steps);
    }
}
#[test]
fn repo_catalog_exact_schema_conflicting_search_identities_and_total_overflow_never_commit() {
    for mutation in 0..3 {
        let fixture = Fixture::new();
        let path = seed(&fixture, "github.invalid");
        let before = sql_state(&path);
        let row = raw_pr("github.invalid", REPO, 7, 2007);
        let mut next = row.clone();
        match mutation {
            0 => next["node_id"] = json!("different_opaque_node"),
            1 => next["id"] = json!(9007),
            _ => next["number"] = json!(8),
        };
        let mut steps = account("github.invalid");
        if mutation == 2 {
            next = raw_pr("github.invalid", REPO, 8, 2007);
        }
        steps.push(page(
            "github.invalid",
            "author",
            REPO,
            1,
            response(1, vec![row], false),
        ));
        steps.push(page(
            "github.invalid",
            "assignee",
            REPO,
            1,
            response(1, vec![next], false),
        ));
        fixture.scripted_gh(&steps);
        let error = run(&fixture, "github.invalid", &path)
            .unwrap_err()
            .to_string();
        assert!(error.contains("identities contradict"));
        assert_eq!(sql_state(&path), before);
        check_journal(&fixture, &steps);
    }
    let fixture = Fixture::new();
    let path = seed(&fixture, "github.invalid");
    let before = sql_state(&path);
    let mut steps = account("github.invalid");
    steps.push(page(
        "github.invalid",
        "author",
        REPO,
        1,
        response(u64::MAX, vec![], false),
    ));
    steps.push(page(
        "github.invalid",
        "assignee",
        REPO,
        1,
        response(1, vec![], false),
    ));
    fixture.scripted_gh(&steps);
    let error = run(&fixture, "github.invalid", &path)
        .unwrap_err()
        .to_string();
    assert!(error.contains("totals overflow"));
    assert_eq!(sql_state(&path), before);
    check_journal(&fixture, &steps);
}
#[test]
fn repo_catalog_exact_schema_legacy_personal_inbox_keeps_old_default_envelope_behavior() {
    let fixture = Fixture::new();
    let steps = ["author", "assignee", "review-requested"]
        .map(|queue| page("github.invalid", queue, REPO, 1, json!({"items":[]})));
    fixture.scripted_gh(&steps);
    let loaded = provider(&fixture)
        .personal_inbox(
            &current("github.invalid"),
            &[REPO.into()],
            &Default::default(),
        )
        .unwrap();
    assert!(loaded.coverage.complete && loaded.pull_requests.is_empty());
    assert_eq!(loaded.coverage.total_matches, 0);
    check_journal(&fixture, &steps); // Deliberately unchanged Legacy; NOT exact admission.
}
