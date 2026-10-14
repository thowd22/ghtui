use super::*;
use crate::test_support::{Fixture, GhStep};
use serde_json::{json, Value};

const HOST: &str = "github.invalid";
fn account(id: u64) -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        GhStep::read(
            &["api", "user", "--hostname", HOST, "--method", "GET"],
            &json!({"id":id,"login":"octocat"}).to_string(),
        ),
    ]
}
fn raw(node: Option<&str>, number: u64) -> Value {
    let mut raw = json!({"id":700,"node_id":node,"number":number,"title":"needle parent","body":"needle exact remote match","repository_url":"https://github.invalid/api/v3/repos/example/repo","html_url":format!("https://github.invalid/example/repo/pull/{number}"),"repository":{"id":99,"full_name":"example/repo"},"pull_request":{"url":format!("https://github.invalid/api/v3/repos/example/repo/pulls/{number}")},"user":{"id":42,"login":"octocat","type":"User"},"state":"open"});
    if node.is_none() {
        raw.as_object_mut().unwrap().remove("node_id");
    }
    raw
}
fn page(items: Vec<Value>) -> GhStep {
    GhStep::read(
        &[
            "api",
            "search/issues",
            "--hostname",
            HOST,
            "--method",
            "GET",
            "--header",
            "Accept: application/vnd.github.text-match+json",
            "-f",
            "q=needle repo:example/repo is:pr",
            "-f",
            "per_page=100",
            "-f",
            "page=1",
        ],
        &json!({"total_count":items.len(),"items":items,"incomplete_results":false}).to_string(),
    )
}
fn provider(fixture: &Fixture) -> GithubProvider {
    GithubProvider::new(HOST)
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"))
}
fn search() -> SearchWorkspace {
    let mut search = SearchWorkspace::new(Scope::github(HOST, Some(42)).unwrap(), Vec::new());
    search.mode = SearchMode::Ranked;
    search.remote_identity = Some(HostIdentity {
        host: HOST.into(),
        login: "octocat".into(),
        account_id: Some(42),
        gh_version: "fixture".into(),
    });
    search.remote_repositories = vec!["example/repo".into()];
    search
}
fn policy() -> BlendPolicy {
    BlendPolicy {
        enabled: true,
        sources: BTreeSet::from([RankingSource::GithubPullRequests]),
        ..Default::default()
    }
}
fn commands(fixture: &Fixture, expected: usize) {
    let rows = std::fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), expected);
    assert!(rows.iter().all(|row| row["matched"] == true));
    assert!(rows
        .iter()
        .all(|row| row["args"][0] == "--version" || row["args"][5] == "GET"));
}
fn access() -> GhStep {
    GhStep::read(
        &[
            "api",
            "repos/example/repo",
            "--hostname",
            HOST,
            "--method",
            "GET",
        ],
        &json!({"id":99,"full_name":"example/repo","permissions":{"pull":true}}).to_string(),
    )
}
#[test]
fn blended_ranked_worker_raw_numeric_opaque_collision_never_aliases_or_overwrites_destination() {
    let fixture = Fixture::new();
    let database = fixture.root.join("cache.sqlite3");
    let mut cache = Cache::open(&database).unwrap();
    let mut steps = account(42);
    steps.push(page(vec![raw(Some("700"), 7), raw(None, 8)]));
    steps.extend([access(), access()]);
    steps.extend(account(42));
    fixture.scripted_gh(&steps);
    let mut search = search();
    let evidence = execute(
        &mut search,
        "needle",
        &policy(),
        &mut cache,
        &fixture.root.join("index"),
        &database,
        &provider(&fixture),
        &CancellationToken::default(),
    )
    .unwrap();
    let page = search.page.as_ref().unwrap();
    assert_eq!(page.hits.len(), 1);
    assert!(!page.coverage.complete);
    assert_eq!(search.remote_item(0).unwrap().number, Some(7));
    assert_eq!(search.remote_item(0).unwrap().id, "700");
    assert!(matches!(
        page.hits[0].document.destination,
        crate::search::model::Destination::PullRequest { number: 7, .. }
    ));
    assert!(page.hits[0]
        .matched_fields
        .iter()
        .any(|field| field.contains("RRF k=60: GitHub PRs #1")));
    assert!(evidence.rows[0].contains("2 loaded / 1 admitted / 1 hidden"));
    evidence.validate(&cache).unwrap();
    assert!(cache
        .saved_view(&search.scope, "search-blend-policy:v1")
        .unwrap()
        .is_none());
    commands(&fixture, 7);
}
#[test]
fn blended_ranked_worker_final_positive_principal_change_never_installs_remote_snippets() {
    let fixture = Fixture::new();
    let database = fixture.root.join("cache.sqlite3");
    let mut cache = Cache::open(&database).unwrap();
    let mut steps = account(42);
    steps.push(page(vec![raw(Some("PR_7"), 7)]));
    steps.extend([access(), access()]);
    steps.extend(account(43));
    fixture.scripted_gh(&steps);
    let mut search = search();
    let error = execute(
        &mut search,
        "needle",
        &policy(),
        &mut cache,
        &fixture.root.join("index"),
        &database,
        &provider(&fixture),
        &CancellationToken::default(),
    )
    .unwrap_err();
    assert!(error.contains("Account changed"));
    assert!(search.remote_items.is_empty());
    assert!(search.page.as_ref().unwrap().hits.is_empty());
    assert!(search.blend_evidence.is_none());
    commands(&fixture, 7);
}
#[test]
fn blended_ranked_worker_remote_only_explicit_scope_and_unsupported_source_zero_search_requests() {
    let fixture = Fixture::new();
    let database = fixture.root.join("cache.sqlite3");
    let mut cache = Cache::open(&database).unwrap();
    let mut steps = account(42);
    steps.extend(account(42));
    fixture.scripted_gh(&steps);
    let mut search = search();
    let evidence = execute(
        &mut search,
        "needle type:comment",
        &policy(),
        &mut cache,
        &fixture.root.join("index"),
        &database,
        &provider(&fixture),
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(search.page.as_ref().unwrap().hits.is_empty());
    assert!(!search.page.as_ref().unwrap().coverage.complete);
    assert!(evidence.rows[0].contains("unsupported"));
    assert!(search.remote_items.is_empty());
    assert!(!fixture.root.join("index").exists());
    commands(&fixture, 4);
}
