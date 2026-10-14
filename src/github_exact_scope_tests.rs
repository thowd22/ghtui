//! SOURCE-ONLY exact-scope refresh contracts; undeclared and UNRUN until Root
//! applies the shared refresh seam. Strict real gh children plus private SQLite.
use super::*;
use crate::test_support::{Fixture, GhStep};
use serde_json::Value;
use std::{fs, os::unix::fs::PermissionsExt};

const REPOSITORY: &str = "example/repo";
const OLD42: &str = "PRIVATE_CANARY_OLD42_SAME_SLUG";
const OLD43: &str = "PRIVATE_CANARY_OLD43_SAME_LOGIN";

fn identity(uid: u64, login: &str) -> HostIdentity {
    HostIdentity {
        host: "github.invalid".into(),
        login: login.into(),
        gh_version: "old presentation version".into(),
        account_id: Some(uid),
    }
}
fn workspace(uid: u64) -> GithubWorkspace {
    GithubWorkspace::empty(
        identity(uid, "same-login"),
        vec![REPOSITORY.into()],
        BTreeSet::new(),
    )
}
fn version() -> GhStep {
    GhStep::read(&["--version"], "gh version 2.71.0 (strict fixture)\n")
}
fn user(value: Value) -> GhStep {
    GhStep::read(
        &[
            "api",
            "user",
            "--hostname",
            "github.invalid",
            "--method",
            "GET",
        ],
        &value.to_string(),
    )
}
fn principal(uid: Value, login: &str) -> Vec<GhStep> {
    vec![version(), user(json!({"id":uid,"login":login}))]
}
fn query(login: &str, qualifier: &str, rows: Value) -> GhStep {
    let result = json!({"total_count": rows.as_array().unwrap().len(), "incomplete_results":false,"items":rows});
    GhStep::read(
        &[
            "api",
            "search/issues",
            "--hostname",
            "github.invalid",
            "--method",
            "GET",
            "-f",
            &format!("q=is:pr is:open {qualifier}:{login} repo:{REPOSITORY}"),
            "-f",
            "per_page=100",
            "-f",
            "page=1",
        ],
        &result.to_string(),
    )
}
fn current_pr() -> Value {
    // Actual new returned PR identity. Neither old cached same-slug node nor
    // issue's numeric ID is promoted into repository-ID access authority.
    json!({"id":2007,"node_id":"PR_new_2007","number":7,"title":"Fresh observed replacement",
      "body":"Fresh read body","repository_url":"https://github.invalid/repos/example/repo",
      "html_url":"https://github.invalid/example/repo/pull/7","state":"open","draft":false,
      "pull_request":{"url":"https://github.invalid/repos/example/repo/pulls/7",
        "html_url":"https://github.invalid/example/repo/pull/7"},
      "created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-14T00:00:00Z",
      "user":{"id":51,"login":"fresh-actor","type":"User"}})
}
fn successful_reads(login: &str) -> Vec<GhStep> {
    let mut steps = principal(json!(42), login);
    steps.push(query(login, "author", json!([current_pr()])));
    steps.push(query(login, "assignee", json!([])));
    steps.push(query(login, "review-requested", json!([])));
    steps
}
fn provider(fixture: &Fixture) -> GithubProvider {
    GithubProvider::new("github.invalid")
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"))
}
fn exact_journal(fixture: &Fixture, steps: &[GhStep]) {
    let rows: Vec<Value> = fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), steps.len());
    for (row, step) in rows.iter().zip(steps) {
        assert_eq!(row["matched"], true);
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(row["stdin_hex"], "");
        assert!(step
            .args
            .iter()
            .all(|arg| arg != "POST" && arg != "PATCH" && arg != "DELETE"));
    }
}
fn old_pr(uid: u64) -> PullRequest {
    PullRequest {
        host: "github.invalid".into(),
        repository: REPOSITORY.into(),
        id: format!("PR_old_{uid}_7007"),
        number: 7,
        title: if uid == 42 { OLD42 } else { OLD43 }.into(),
        body: format!("PRIVATE_BODY_{uid}"),
        url: "https://github.invalid/example/repo/pull/7".into(),
        author: Default::default(),
        state: "open".into(),
        draft: false,
        created_at: "2026-10-01T00:00:00Z".into(),
        updated_at: "2026-10-13T00:00:00Z".into(),
        queues: [Queue::Assigned].into_iter().collect(),
    }
}
fn seed(fixture: &Fixture) -> std::path::PathBuf {
    let path = fixture.root.join("cache.sqlite3");
    let mut cache = Cache::open(&path).unwrap();
    for (uid, generation) in [(42, 4), (43, 9)] {
        let original = workspace(uid);
        let scope = original.scope().unwrap();
        let source = original.source();
        assert!(cache
            .commit_sync(
                &scope,
                &source,
                generation,
                &[
                    Record::new(
                        "inbox-meta", &source, "",
                        json!({"coverage":{"loaded":1,"total_matches":1,"complete":true,"reasons":[]}}),
                    ),
                    Record::new("pr", "example/repo#7", REPOSITORY, serde_json::to_value(old_pr(uid)).unwrap()),
                ],
                true,
            )
            .unwrap());
        cache
            .put_draft(&scope, "keep-original", &format!("PRIVATE_DRAFT_{uid}"))
            .unwrap();
    }
    path
}
fn snapshot(path: &Path) -> Vec<(String, String, String, String)> {
    // Independent fixture inspector does not repair Cache permissions, decode
    // private JSON, or write the production database.
    let sql =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let mut statement = sql.prepare(
        "SELECT host,login,kind||':'||id,payload FROM records UNION ALL SELECT host,login,'draft:'||resource,body FROM drafts UNION ALL SELECT host,login,'checkpoint:'||source,CAST(generation AS TEXT)||':'||complete FROM checkpoints ORDER BY 1,2,3",
    ).unwrap();
    let rows = statement
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap();
    rows.collect::<rusqlite::Result<Vec<_>>>().unwrap()
}
fn error_text(result: Result<GithubWorkspace>) -> String {
    let error = result.unwrap_err().to_string();
    assert!(!error.contains(OLD42));
    assert!(!error.contains(OLD43));
    assert!(!error.contains("PRIVATE_BODY"));
    assert!(!error.contains("PRIVATE_DRAFT"));
    error
}

#[test]
fn repo_catalog_exact_refresh_changed_numeric_uid_never_opens_cache_or_queries() {
    let fixture = Fixture::new();
    let path = seed(&fixture);
    let before = snapshot(&path);
    // Cache::open would repair0644 to0600. Unchanged mode is independent
    // evidence that the real scope failure occurs BEFORE that private opener.
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    let steps = principal(json!(43), "same-login");
    fixture.scripted_gh(&steps);
    let expected = Scope::github("github.invalid", Some(42)).unwrap();
    let error = error_text(workspace(42).refresh_exact_scope(
        &expected,
        &provider(&fixture),
        &path,
        &CancellationToken::default(),
    ));
    assert!(error.contains("Exact inbox scope changed"));
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o644
    );
    assert_eq!(snapshot(&path), before);
    exact_journal(&fixture, &steps);

    let cold = Fixture::new();
    cold.scripted_gh(&steps);
    let unopened = cold.root.join("must-not-create/cache.sqlite3");
    assert!(error_text(workspace(42).refresh_exact_scope(
        &expected,
        &provider(&cold),
        &unopened,
        &CancellationToken::default()
    ))
    .contains("Exact inbox scope changed"));
    assert!(!unopened.exists());
    assert!(!unopened.parent().unwrap().exists());
    exact_journal(&cold, &steps);
}

#[test]
fn repo_catalog_exact_refresh_auth_null_zero_and_foreign_scope_fail_before_sql() {
    for (uid, denied, expected_host) in [
        (Value::Null, false, "github.invalid"),
        (json!(0), false, "github.invalid"),
        (json!(42), true, "github.invalid"),
        (json!(42), false, "other.invalid"),
    ] {
        let fixture = Fixture::new();
        let mut steps = principal(uid, "same-login");
        if denied {
            steps[1].status = 1;
            steps[1].stdout.clear();
            steps[1].stderr = b"HTTP401 authentication expired".to_vec();
        }
        fixture.scripted_gh(&steps);
        let path = fixture.root.join("private-must-stay-cold/cache.sqlite3");
        let expected = Scope::github(expected_host, Some(42)).unwrap();
        let message = error_text(workspace(42).refresh_exact_scope(
            &expected,
            &provider(&fixture),
            &path,
            &CancellationToken::default(),
        ));
        assert!(!message.is_empty());
        assert!(!path.parent().unwrap().exists());
        exact_journal(&fixture, &steps);
    }
}

#[test]
fn repo_catalog_exact_refresh_fresh_same_slug_new_node_ignores_old_cached_frames() {
    let fixture = Fixture::new();
    let path = seed(&fixture);
    let mut prior = workspace(42).cached(&path).unwrap();
    assert_eq!(prior.visible()[0].title, OLD42);
    prior.filter = InboxFilter::Assigned;
    let expected = prior.scope().unwrap();
    // A legacy sync_frame/cached read would deserialize this invalidJSON. Exact
    // refresh needs only checkpoint metadata and NEW provider bytes.
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute("UPDATE records SET payload='{broken_PRIVATE_CANARY_OLD42' WHERE login='user-id:42' AND kind='pr'", []).unwrap();
    drop(sql);
    let steps = successful_reads("same-login");
    fixture.scripted_gh(&steps);
    let refreshed = prior
        .refresh_exact_scope(
            &expected,
            &provider(&fixture),
            &path,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(refreshed.generation, 5);
    assert_eq!(refreshed.filter, InboxFilter::All);
    assert_eq!(refreshed.identity.account_id, Some(42));
    assert_eq!(refreshed.inbox.pull_requests.len(), 1);
    assert_eq!(refreshed.inbox.coverage.loaded, 1);
    assert_eq!(refreshed.inbox.coverage.total_matches, 1);
    assert!(refreshed.inbox.coverage.complete);
    assert!(matches!(refreshed.freshness, Freshness::Fresh { .. }));
    assert_eq!(refreshed.visible()[0].id, "PR_new_2007");
    assert_eq!(refreshed.visible()[0].title, "Fresh observed replacement");
    assert_eq!(refreshed.visible()[0].author.login, "fresh-actor");
    assert!(refreshed.pr.is_none());
    assert!(!format!("{refreshed:?}").contains(OLD42));
    assert!(!format!("{refreshed:?}").contains(OLD43));
    exact_journal(&fixture, &steps); // Exactly identity2 +3 explicit repo queues.
    let cache = Cache::open(&path).unwrap();
    assert_eq!(
        cache.draft(&expected, "keep-original").unwrap().as_deref(),
        Some("PRIVATE_DRAFT_42")
    );
    assert_eq!(
        cache
            .draft(
                &Scope::github("github.invalid", Some(43)).unwrap(),
                "keep-original"
            )
            .unwrap()
            .as_deref(),
        Some("PRIVATE_DRAFT_43")
    );
}

#[test]
fn repo_catalog_exact_refresh_same_uid_login_rename_uses_actual_new_presentation() {
    let fixture = Fixture::new();
    let path = seed(&fixture);
    let expected = Scope::github("github.invalid", Some(42)).unwrap();
    let steps = successful_reads("legitimate-rename");
    fixture.scripted_gh(&steps);
    // Even an old workspace from43 is not a cache/current selection source.
    // Only its EXPLICIT requested repos/exclusions are carried into empty42.
    let prior = workspace(43).cached(&path).unwrap();
    assert_eq!(prior.visible()[0].title, OLD43);
    let refreshed = prior
        .refresh_exact_scope(
            &expected,
            &provider(&fixture),
            &path,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(refreshed.identity.account_id, Some(42));
    assert_eq!(refreshed.identity.login, "legitimate-rename");
    assert_eq!(refreshed.inbox.identity.login, "legitimate-rename");
    assert_eq!(refreshed.scope().unwrap(), expected);
    assert_eq!(refreshed.repositories, [REPOSITORY]);
    assert_eq!(refreshed.generation, 5);
    assert_eq!(refreshed.inbox.pull_requests[0].id, "PR_new_2007");
    assert!(!format!("{refreshed:?}").contains(OLD43));
    exact_journal(&fixture, &steps);
}

#[test]
fn repo_catalog_exact_refresh_failed_collection_never_returns_stale_or_partial_old_counts() {
    for failed_queue in 0..3 {
        let fixture = Fixture::new();
        let path = seed(&fixture);
        let prior = workspace(42).cached(&path).unwrap();
        assert_eq!(prior.visible()[0].title, OLD42);
        let expected = prior.scope().unwrap();
        let before = snapshot(&path);
        let mut steps = successful_reads("same-login");
        steps.truncate(3 + failed_queue);
        let failure = steps.last_mut().unwrap();
        failure.status = 1;
        failure.stdout.clear();
        failure.stderr = b"HTTP403 repository access denied".to_vec();
        fixture.scripted_gh(&steps);
        let message = error_text(prior.refresh_exact_scope(
            &expected,
            &provider(&fixture),
            &path,
            &CancellationToken::default(),
        ));
        assert!(!message.is_empty());
        assert_eq!(
            snapshot(&path),
            before,
            "no partial/failed source commit or draft mutation"
        );
        exact_journal(&fixture, &steps);
    }
}

#[test]
fn repo_catalog_exact_refresh_explicit_empty_scope_never_becomes_personal_all() {
    let fixture = Fixture::new();
    let steps = principal(json!(42), "same-login");
    fixture.scripted_gh(&steps);
    let path = fixture.root.join("cold-empty/cache.sqlite3");
    let mut prior = workspace(42);
    prior.repositories.clear();
    prior.inbox.pull_requests.push(old_pr(42));
    let expected = prior.scope().unwrap();
    assert!(error_text(prior.refresh_exact_scope(
        &expected,
        &provider(&fixture),
        &path,
        &CancellationToken::default()
    ))
    .contains("personal all-scope queries are forbidden"));
    assert!(!path.parent().unwrap().exists());
    exact_journal(&fixture, &steps);
}

#[test]
fn repo_catalog_exact_refresh_does_not_change_default_offline_or_account_cache_behavior() {
    for uid in [42, 43] {
        let fixture = Fixture::new();
        let path = seed(&fixture);
        let prior = workspace(42).cached(&path).unwrap();
        let mut steps = principal(json!(uid), "same-login");
        let mut failure = query("same-login", "author", json!([]));
        failure.status = 1;
        failure.stdout.clear();
        failure.stderr = b"HTTP503 offline default behavior".to_vec();
        steps.push(failure);
        fixture.scripted_gh(&steps);
        let unchanged = prior
            .refresh(&provider(&fixture), &path, &CancellationToken::default())
            .unwrap();
        assert_eq!(unchanged.identity.account_id, Some(uid));
        assert_eq!(
            unchanged.visible()[0].title,
            if uid == 42 { OLD42 } else { OLD43 }
        );
        assert!(matches!(unchanged.freshness, Freshness::Offline { .. }));
        exact_journal(&fixture, &steps);
    }
}

#[test]
fn repo_catalog_exact_refresh_rejects_foreign_result_and_allows_actual_repository_case() {
    for observed_repository in ["other/private", "Example/Repo"] {
        let fixture = Fixture::new();
        let path = seed(&fixture);
        let expected = workspace(42).scope().unwrap();
        let before = snapshot(&path);
        let mut returned = current_pr();
        returned["repository_url"] = json!(format!(
            "https://github.invalid/repos/{observed_repository}"
        ));
        returned["html_url"] = json!(format!(
            "https://github.invalid/{observed_repository}/pull/7"
        ));
        returned["title"] = json!("OBSERVED_SCOPE_RESULT");
        let mut steps = principal(json!(42), "same-login");
        steps.push(query("same-login", "author", json!([returned])));
        if observed_repository != "other/private" {
            steps.push(query("same-login", "assignee", json!([])));
            steps.push(query("same-login", "review-requested", json!([])));
        }
        fixture.scripted_gh(&steps);
        let result = workspace(42).refresh_exact_scope(
            &expected,
            &provider(&fixture),
            &path,
            &CancellationToken::default(),
        );
        if observed_repository == "other/private" {
            assert!(error_text(result).contains("source URL contradicts"));
            assert_eq!(
                snapshot(&path),
                before,
                "foreign PR cannot publish any source frame"
            );
        } else {
            let observed = result.unwrap();
            assert_eq!(observed.inbox.pull_requests.len(), 1);
            assert_eq!(observed.inbox.pull_requests[0].repository, REPOSITORY);
            assert_eq!(observed.inbox.pull_requests[0].id, "PR_new_2007");
            assert_eq!(
                observed.inbox.pull_requests[0].title,
                "OBSERVED_SCOPE_RESULT"
            );
            assert!(matches!(observed.freshness, Freshness::Fresh { .. }));
        }
        exact_journal(&fixture, &steps);
    }
}
