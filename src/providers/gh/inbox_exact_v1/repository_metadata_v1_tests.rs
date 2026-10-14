//! Strict actual factory/session/SQLite contracts for repository metadata.
use super::*;
use crate::{
    active_remote_scope_v1::ActiveRemoteScopeV1,
    github_workspace::{Freshness, GithubWorkspace, PrKey},
    providers::{
        gh::{
            Coverage, GithubProvider, PullRequest, Queue, RepositoryCatalogueAccessLimits,
            RepositoryCatalogueLimits, RepositoryCatalogueSource,
        },
        runner::CancellationToken,
    },
    repository_catalogue_intent_v1::IntentReference,
    repository_catalogue_save_v1::load_catalogue_intent_source,
    storage::{Cache, CatalogueIntentSource, Record, Scope},
    test_support::{Fixture, GhStep},
};
use rusqlite::{Connection, OpenFlags};
use serde_json::json;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

const HOST: &str = "github.invalid";
const NAME: &str = "example/repo";
const OLD: &str = "PRIVATE_ORIGINAL_METADATA_QUEUE_CANARY";
fn get(endpoint: &str, value: Value) -> GhStep {
    GhStep::read(
        &["api", endpoint, "--hostname", HOST, "--method", "GET"],
        &value.to_string(),
    )
}
fn principal(uid: u64) -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.99.0 (2026-10-01)\n"),
        get("user", json!({"id":uid,"login":"octocat"})),
    ]
}
fn owner() -> Value {
    json!({"id":71,"node_id":"O_71","login":"example","type":"Organization"})
}
fn repo(id: u64, name: &str, node: Option<&str>) -> Value {
    json!({"id":id,"node_id":node,"full_name":name,"owner":owner(),"permissions":{"pull":true}})
}
fn source(rows: Value) -> Vec<GhStep> {
    let mut steps = principal(42);
    steps.push(get("orgs/example", owner()));
    steps.push(get(
        "orgs/example/repos?type=all&sort=full_name&direction=asc&per_page=100&page=1",
        rows,
    ));
    steps.push(get("orgs/example", owner()));
    steps.extend(principal(42));
    steps
}
fn row(name: &str, metadata: Value) -> Value {
    json!({"id":2007,"node_id":"ISSUE_SEARCH_2007_NOT_REPOSITORY_OR_PR","number":7,"title":"Actual current queue item","body":"Observed actual search body","state":"open","draft":false,
    "repository_url":format!("https://{HOST}/api/v3/repos/{name}"),"html_url":format!("https://{HOST}/{name}/pull/7"),
    "pull_request":{"url":format!("https://{HOST}/api/v3/repos/{name}/pulls/7"),"html_url":format!("https://{HOST}/{name}/pull/7")},
    "created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-14T00:00:00Z","user":{"id":51,"login":"actual-author","type":"User"},"repository":metadata})
}
fn query(name: &str, qualifier: &str, metadata: Value) -> GhStep {
    GhStep::read(
        &[
            "api",
            "search/issues",
            "--hostname",
            HOST,
            "--method",
            "GET",
            "-f",
            &format!("q=is:pr is:open {qualifier}:octocat repo:{name}"),
            "-f",
            "per_page=100",
            "-f",
            "page=1",
        ],
        &json!({"total_count":1,"incomplete_results":false,"items":[row(name,metadata)]})
            .to_string(),
    )
}
fn provider(fixture: &Fixture) -> GithubProvider {
    GithubProvider::new(HOST)
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"))
}
fn database(fixture: &Fixture) -> PathBuf {
    fixture.root.join("metadata.sqlite3")
}
fn git(fixture: &Fixture, arguments: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(arguments)
        .current_dir(fixture.repo())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
fn load(fixture: &Fixture) -> Arc<CatalogueIntentSource> {
    let mut cache = Cache::open(&database(fixture)).unwrap();
    cache
        .put_draft(
            &Scope::github(HOST, Some(42)).unwrap(),
            "original-private-draft",
            OLD,
        )
        .unwrap();
    Arc::new(
        load_catalogue_intent_source(
            &provider(fixture),
            &mut cache,
            None,
            &RepositoryCatalogueSource::Organization("example".into()),
            &RepositoryCatalogueLimits::default(),
            &CancellationToken::default(),
        )
        .unwrap(),
    )
}
fn reference(source: &CatalogueIntentSource, index: usize) -> IntentReference {
    let row = &source.source().repositories()[index];
    IntentReference {
        id: row.id,
        node_id: row.node_id.clone(),
        last_name: row.full_name.clone(),
    }
}
fn seed(fixture: &Fixture, name: &str, identity: &HostIdentity) {
    let mut cache = Cache::open(&database(fixture)).unwrap();
    let workspace = GithubWorkspace::empty(identity.clone(), vec![name.into()], Default::default());
    let old = PullRequest {
        host: HOST.into(),
        repository: name.into(),
        id: "OLD_SEARCH_ID".into(),
        number: 91,
        title: OLD.into(),
        body: OLD.into(),
        url: String::new(),
        author: Default::default(),
        state: "open".into(),
        draft: false,
        created_at: String::new(),
        updated_at: String::new(),
        queues: [Queue::Assigned].into_iter().collect(),
    };
    assert!(cache.commit_sync(&workspace.scope().unwrap(),&workspace.source(),4,&[Record::new("inbox-meta",workspace.source(),"",json!({"coverage":Coverage{loaded:1,total_matches:1,complete:true,reasons:vec![]}})),Record::new("pr",PrKey::from(&old).label(),name,serde_json::to_value(old).unwrap())],true).unwrap());
}
fn state(fixture: &Fixture) -> Vec<String> {
    let db =
        Connection::open_with_flags(database(fixture), OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    ["SELECT json_array(host,login,resource,body) FROM drafts ORDER BY host,login,resource","SELECT json_array(host,login,name,payload) FROM saved_views ORDER BY host,login,name","SELECT json_array(host,login,source,generation,complete) FROM checkpoints ORDER BY host,login,source","SELECT json_array(host,login,kind,id,payload) FROM records ORDER BY host,login,kind,id"]
        .iter().flat_map(|query|db.prepare(query).unwrap().query_map([],|row|row.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>()).collect()
}
fn exact(fixture: &Fixture, steps: &[GhStep]) {
    let rows = fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), steps.len());
    for (actual, step) in rows.iter().zip(steps) {
        assert_eq!(actual["args"], json!(step.args));
        assert_eq!(actual["matched"], true);
        assert_eq!(actual["stdin_hex"], "");
        assert!(!step.args.iter().any(|arg| matches!(
            arg.as_str(),
            "POST" | "PUT" | "PATCH" | "DELETE" | "--paginate" | "--slurp" | "graphql"
        )));
    }
}
fn protocol(metadata: Value, name: &str, complete: bool, node: Option<&str>) -> Vec<GhStep> {
    let mut steps = principal(42);
    steps.push(get(&format!("repos/{NAME}"), repo(410, name, node)));
    steps.extend(principal(42));
    steps.push(query(name, "author", metadata.clone()));
    if complete {
        steps.push(query(name, "assignee", metadata.clone()));
        steps.push(query(name, "review-requested", metadata));
        steps.push(get(&format!("repos/{NAME}"), repo(410, name, node)));
        steps.extend(principal(42));
    }
    steps
}

#[test]
fn exact_inbox_repository_metadata_supplied_id_node_contradictions_stop_before_sql_commit_or_fallback(
) {
    for metadata in [
        json!({"id":999,"node_id":"R_410","full_name":NAME}),
        json!({"id":410,"node_id":"R_recreated","full_name":NAME}),
    ] {
        let fixture = Fixture::new();
        let mut steps = source(json!([repo(410, NAME, Some("R_410"))]));
        steps.extend(protocol(metadata, NAME, false, Some("R_410")));
        fixture.scripted_gh(&steps);
        let original = load(&fixture);
        seed(&fixture, NAME, original.source().identity());
        let before = state(&fixture);
        let active =
            Arc::new(ActiveRemoteScopeV1::explicit_selection(original.source(), &[410]).unwrap());
        let error = crate::scoped_inbox_route_v1::refresh(
            original.clone(),
            active,
            &provider(&fixture),
            &database(&fixture),
            &CancellationToken::default(),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            crate::scoped_inbox_route_v1::ScopedInboxFailureV1::LegacyReadUnavailable
        ));
        assert!(!error.to_string().contains(OLD));
        assert_eq!(steps.len(), 13);
        exact(&fixture, &steps);
        assert_eq!(state(&fixture), before);
    }
}

fn complete_case(
    metadata: Value,
    name: &str,
    node: Option<&str>,
) -> (Fixture, crate::scoped_inbox_route_v1::ScopedInboxResultV1) {
    let fixture = Fixture::new();
    let mut steps = source(json!([repo(410, NAME, node)]));
    steps.extend(protocol(metadata, name, true, node));
    fixture.scripted_gh(&steps);
    let original = load(&fixture);
    seed(&fixture, name, original.source().identity());
    git(&fixture, &["init", "--initial-branch=main"]);
    git(&fixture, &["config", "user.name", "Metadata fixture"]);
    git(
        &fixture,
        &["config", "user.email", "metadata@example.invalid"],
    );
    fs::write(fixture.repo().join("dirty.txt"), b"original\n").unwrap();
    git(&fixture, &["add", "--", "dirty.txt"]);
    git(&fixture, &["commit", "-m", "Metadata baseline"]);
    fs::write(fixture.repo().join("dirty.txt"), b"untouched dirty\n").unwrap();
    let head = git(&fixture, &["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = git(&fixture, &["status", "--porcelain=v2", "-z"]);
    let active =
        Arc::new(ActiveRemoteScopeV1::explicit_selection(original.source(), &[410]).unwrap());
    let result = crate::scoped_inbox_route_v1::refresh(
        original.clone(),
        active,
        &provider(&fixture),
        &database(&fixture),
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(steps.len(), 18);
    exact(&fixture, &steps);
    assert_eq!(git(&fixture, &["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(git(&fixture, &["status", "--porcelain=v2", "-z"]), status);
    let cache = Cache::open(&database(&fixture)).unwrap();
    cache
        .validate_repository_catalogue_intent_source(&original)
        .unwrap();
    assert_eq!(
        cache
            .draft(original.scope(), "original-private-draft")
            .unwrap()
            .as_deref(),
        Some(OLD)
    );
    assert_eq!(
        cache
            .sync_checkpoint(original.scope(), &result.workspace.source())
            .unwrap(),
        Some((5, true))
    );
    assert!(!result
        .workspace
        .inbox
        .pull_requests
        .iter()
        .any(|row| row.body == OLD));
    assert!(result.workspace.inbox.coverage.complete);
    assert!(matches!(
        result.workspace.freshness,
        Freshness::Fresh { .. }
    ));
    (fixture, result)
}
#[test]
fn exact_inbox_repository_metadata_known_match_keeps_issue_id_namespace_and_real_git_draft() {
    let (_fixture, result) = complete_case(
        json!({"id":410,"node_id":"R_410","full_name":NAME}),
        NAME,
        Some("R_410"),
    );
    assert_eq!(
        result.workspace.inbox.pull_requests[0].id,
        "ISSUE_SEARCH_2007_NOT_REPOSITORY_OR_PR"
    );
    assert_eq!(result.workspace.inbox.pull_requests[0].queues.len(), 3);
    assert_eq!(result.workspace.inbox.coverage.total_matches, 3);
}
#[test]
fn exact_inbox_repository_metadata_missing_null_optional_fields_remain_unknown_not_empty_denial() {
    for metadata in [
        Value::Null,
        json!({}),
        json!({"id":null,"node_id":null}),
        json!({"id":410}),
        json!({"node_id":"R_410"}),
    ] {
        let (_fixture, result) = complete_case(metadata, NAME, Some("R_410"));
        assert_eq!(result.workspace.inbox.pull_requests.len(), 1);
        assert_eq!(result.workspace.inbox.pull_requests[0].number, 7);
    }
}
#[test]
fn exact_inbox_repository_metadata_same_id_canonical_rename_case_only_name_not_node_alias() {
    let (_fixture, result) = complete_case(
        json!({"id":410,"node_id":"R_410","full_name":"EXAMPLE/RENAMED"}),
        "example/renamed",
        Some("R_410"),
    );
    assert_eq!(result.workspace.repositories, ["example/renamed"]);
    assert_eq!(
        result.workspace.inbox.pull_requests[0].repository,
        "example/renamed"
    );
}
#[test]
fn exact_inbox_repository_metadata_genuine_unknown_current_node_does_not_derive_search_node() {
    let (_fixture, result) = complete_case(
        json!({"id":410,"node_id":"OBSERVED_UNVERIFIED_SEARCH_REPO_NODE"}),
        NAME,
        None,
    );
    assert_eq!(result.workspace.inbox.pull_requests.len(), 1);
    let fixture = Fixture::new();
    let mut steps = source(json!([repo(410, NAME, None)]));
    steps.extend(principal(42));
    steps.push(get(&format!("repos/{NAME}"), repo(410, NAME, None)));
    fixture.scripted_gh(&steps);
    let original = load(&fixture);
    let provider = provider(&fixture);
    let mut session = provider
        .begin_repository_catalogue_access(
            original.source(),
            &RepositoryCatalogueAccessLimits::default(),
            &CancellationToken::default(),
        )
        .unwrap();
    let identity = session.identity().clone();
    let metadata = ExactInboxRepositoryMetadataV1::from_current(
        &identity,
        session
            .verify(&reference(&original, 0), &CancellationToken::default())
            .unwrap(),
    )
    .unwrap();
    assert!(metadata.repositories[NAME].node_id.is_none());
    // Truly absent metadata is not permission denial or an observed node.
    metadata
        .validate_optional_repository(NAME, &json!({"id":2007,"node_id":"ISSUE_SEARCH_ONLY"}))
        .unwrap();
    assert!(metadata.repositories[NAME].node_id.is_none());
    exact(&fixture, &steps);
}
#[test]
fn exact_inbox_repository_metadata_fresh_uid43_no_scope_rebase_before_private_sql_or_search() {
    let fixture = Fixture::new();
    let mut steps = source(json!([repo(410, NAME, Some("R_410"))]));
    steps.extend(principal(42));
    steps.push(get(
        &format!("repos/{NAME}"),
        repo(410, NAME, Some("R_410")),
    ));
    steps.extend(principal(43));
    fixture.scripted_gh(&steps);
    let original = load(&fixture);
    seed(&fixture, NAME, original.source().identity());
    fs::set_permissions(database(&fixture), fs::Permissions::from_mode(0o644)).unwrap();
    let before = state(&fixture);
    let active =
        Arc::new(ActiveRemoteScopeV1::explicit_selection(original.source(), &[410]).unwrap());
    let error = crate::scoped_inbox_route_v1::refresh(
        original,
        active,
        &provider(&fixture),
        &database(&fixture),
        &CancellationToken::default(),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        crate::scoped_inbox_route_v1::ScopedInboxFailureV1::LegacyReadUnavailable
    ));
    exact(&fixture, &steps);
    assert_eq!(steps.len(), 12);
    assert_eq!(
        fs::metadata(database(&fixture))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o644
    );
    assert_eq!(state(&fixture), before);
}
#[test]
fn exact_inbox_repository_metadata_only_genuine_rows_bounded_scope_and_duplicate_observation_refuse(
) {
    let fixture = Fixture::new();
    let inventory = (0..65)
        .map(|i| {
            repo(
                10000 + i,
                &format!("example/repo{i:04}"),
                Some(&format!("R_{}", 10000 + i)),
            )
        })
        .collect::<Vec<_>>();
    let mut steps = source(json!(inventory));
    steps.extend(principal(42));
    for i in 0..65 {
        steps.push(get(
            &format!("repos/example/repo{i:04}"),
            repo(
                10000 + i,
                &format!("example/repo{i:04}"),
                Some(&format!("R_{}", 10000 + i)),
            ),
        ));
    }
    fixture.scripted_gh(&steps);
    let original = load(&fixture);
    let provider = provider(&fixture);
    let mut session = provider
        .begin_repository_catalogue_access(
            original.source(),
            &RepositoryCatalogueAccessLimits::default(),
            &CancellationToken::default(),
        )
        .unwrap();
    let identity = session.identity().clone();
    let mut expected = ExactInboxRepositoryMetadataV1::from_current(
        &identity,
        session
            .verify(&reference(&original, 0), &CancellationToken::default())
            .unwrap(),
    )
    .unwrap();
    for i in 1..64 {
        expected
            .capture_current(
                session
                    .verify(&reference(&original, i), &CancellationToken::default())
                    .unwrap(),
            )
            .unwrap();
    }
    assert_eq!(expected.repositories.len(), 64);
    assert!(expected
        .capture_current(
            session
                .verify(&reference(&original, 64), &CancellationToken::default())
                .unwrap()
        )
        .is_err());
    assert_eq!(expected.repositories.len(), 64);
    let names = (0..64)
        .map(|i| format!("example/repo{i:04}"))
        .collect::<Vec<_>>();
    expected.validate_scope(&identity, &names).unwrap();
    assert!(expected.validate_scope(&identity, &[]).is_err());
    assert!(expected.validate_scope(&identity, &names[..1]).is_err());
    let mut duplicate = names.clone();
    duplicate[1] = duplicate[0].clone();
    assert!(expected.validate_scope(&identity, &duplicate).is_err());
    let mut reused = identity.clone();
    reused.account_id = Some(43);
    assert!(expected.validate_scope(&reused, &names).is_err());
    let mut foreign_host = identity.clone();
    foreign_host.host = "other.invalid".into();
    assert!(expected.validate_scope(&foreign_host, &names).is_err());
    let mut rename = identity;
    rename.login = "legitimate-rename".into();
    expected.validate_scope(&rename, &names).unwrap();
    exact(&fixture, &steps);
}
fn wait(path: &Path, count: usize) {
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        if fs::read_to_string(path)
            .ok()
            .is_some_and(|text| text.lines().count() >= count)
        {
            return;
        }
        assert!(Instant::now() < end);
        thread::sleep(Duration::from_millis(2));
    }
}
#[test]
fn exact_inbox_repository_metadata_same_window_capture_cancel_never_commits_after_failed_query() {
    for mode in 0..3 {
        let fixture = Fixture::new();
        let mut steps = source(json!([repo(410, NAME, Some("R_410"))]));
        let mut op = protocol(json!({"id":999}), NAME, false, Some("R_410"));
        if mode != 0 {
            op[5].delay_ms = 2000;
        } else {
            op[5].stderr = "🦀".repeat(300000).into_bytes();
        }
        steps.extend(op);
        fixture.scripted_gh(&steps);
        let original = load(&fixture);
        seed(&fixture, NAME, original.source().identity());
        let before = state(&fixture);
        let token = CancellationToken::default();
        let cancelled = if mode == 1 {
            let path = fixture.root.join("gh-fixture/commands.jsonl");
            let cancel = token.clone();
            Some(thread::spawn(move || {
                wait(&path, 13);
                cancel.cancel();
            }))
        } else {
            None
        };
        if mode == 2 {
            let provider = provider(&fixture);
            let mut limits = RepositoryCatalogueAccessLimits::default();
            limits.transport.max_response_bytes = 64 * 1024 * 1024;
            limits.transport.max_duration = Duration::from_secs(1);
            let mut session = provider
                .begin_repository_catalogue_access(original.source(), &limits, &token)
                .unwrap();
            let identity = session.identity().clone();
            let metadata = ExactInboxRepositoryMetadataV1::from_current(
                &identity,
                session.verify(&reference(&original, 0), &token).unwrap(),
            )
            .unwrap();
            let window = session
                .reserve_external_reads(32, 1024 * 1024, &token)
                .unwrap();
            let scoped = provider
                .clone()
                .with_scoped_read_window(window.deadline(), window.max_output_bytes())
                .unwrap();
            assert!(
                GithubWorkspace::empty(identity, vec![NAME.into()], Default::default())
                    .refresh_exact_scope_current(
                        original.scope(),
                        &metadata,
                        &scoped,
                        &database(&fixture),
                        &token
                    )
                    .is_err()
            );
            assert!(
                session.remaining(&token).is_err(),
                "same original session expired, not a fresh per-phase clock"
            );
        } else {
            let active = Arc::new(
                ActiveRemoteScopeV1::explicit_selection(original.source(), &[410]).unwrap(),
            );
            let error = crate::scoped_inbox_route_v1::refresh(
                original,
                active,
                &provider(&fixture),
                &database(&fixture),
                &token,
            )
            .unwrap_err();
            assert!(matches!(
                error,
                crate::scoped_inbox_route_v1::ScopedInboxFailureV1::LegacyReadUnavailable
            ));
            assert!(!error.to_string().contains(OLD));
        }
        if let Some(handle) = cancelled {
            handle.join().unwrap();
        }
        exact(&fixture, &steps);
        assert_eq!(state(&fixture), before);
    }
}
