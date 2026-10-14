//! SOURCE-ONLY strict provider + realSQLite/Git contracts; UNREGISTERED/UNRUN.
use super::*;
use crate::{
    github_workspace::{Freshness, PrKey},
    providers::gh::{
        Coverage, PullRequest, Queue, RepositoryCatalogueLimits, RepositoryCatalogueSource,
    },
    repository_catalogue_save_v1::load_catalogue_intent_source,
    storage::{Record, Scope},
    test_support::{Fixture, GhStep},
};
use serde_json::{json, Value};
use std::{fs, os::unix::fs::PermissionsExt, process::Command};

fn get(endpoint: &str, value: Value) -> GhStep {
    GhStep::read(
        &[
            "api",
            endpoint,
            "--hostname",
            "github.invalid",
            "--method",
            "GET",
        ],
        &value.to_string(),
    )
}
fn principal(uid: u64, login: &str) -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.99.0 (2026-10-01)\n"),
        get("user", json!({"id":uid,"login":login})),
    ]
}
fn owner() -> Value {
    json!({"id":71,"node_id":"OWNER_71","login":"example","type":"Organization"})
}
fn repo(id: u64, name: &str, pull: Value) -> Value {
    json!({"id":id,"node_id":format!("R_{id}"),"full_name":name,"owner":owner(),"permissions":{"pull":pull}})
}
fn source_steps(repos: &[(u64, &str)]) -> Vec<GhStep> {
    let mut steps = principal(42, "octocat");
    steps.push(get("orgs/example", owner()));
    steps.push(get(
        "orgs/example/repos?type=all&sort=full_name&direction=asc&per_page=100&page=1",
        json!(repos
            .iter()
            .map(|(id, name)| repo(*id, name, json!(true)))
            .collect::<Vec<_>>()),
    ));
    steps.push(get("orgs/example", owner()));
    steps.extend(principal(42, "octocat"));
    steps
}
fn provider(fixture: &Fixture) -> GithubProvider {
    GithubProvider::new("github.invalid")
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"))
}
fn database(fixture: &Fixture) -> std::path::PathBuf {
    fixture.root.join("cache.sqlite3")
}
fn load(fixture: &Fixture) -> Arc<CatalogueIntentSource> {
    let mut cache = Cache::open(&database(fixture)).unwrap();
    cache
        .put_draft(
            &Scope::github("github.invalid", Some(42)).unwrap(),
            "original-review",
            "PRIVATE_DRAFT_PRESERVED",
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
fn active(original: &CatalogueIntentSource, ids: &[u64]) -> Arc<ActiveRemoteScopeV1> {
    Arc::new(ActiveRemoteScopeV1::explicit_selection(original.source(), ids).unwrap())
}
fn issue(repository: &str) -> Value {
    let database_id = if repository == "example/two" {
        1008
    } else {
        1007
    };
    json!({"id":database_id,"node_id":format!("PR_{repository}_7"),"number":7,"title":"Fresh actual scoped PR",
      "body":"Fresh scoped body","repository_url":format!("https://github.invalid/repos/{repository}"),
      "html_url":format!("https://github.invalid/{repository}/pull/7"),"state":"open","draft":false,
      "pull_request":{"url":format!("https://github.invalid/repos/{repository}/pulls/7"),
        "html_url":format!("https://github.invalid/{repository}/pull/7")},
      "created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-14T00:00:00Z",
      "user":{"id":42,"login":"octocat","type":"User"}})
}
fn query(repository: &str, qualifier: &str, login: &str) -> GhStep {
    let rows = if qualifier == "author" {
        vec![issue(repository)]
    } else {
        vec![]
    };
    GhStep::read(
        &[
            "api",
            "search/issues",
            "--hostname",
            "github.invalid",
            "--method",
            "GET",
            "-f",
            &format!("q=is:pr is:open {qualifier}:{login} repo:{repository}"),
            "-f",
            "per_page=100",
            "-f",
            "page=1",
        ],
        &json!({"total_count":rows.len(),"incomplete_results":false,"items":rows}).to_string(),
    )
}
fn inbox(names: &[&str], inner_uid: u64) -> Vec<GhStep> {
    let mut steps = principal(inner_uid, "octocat");
    if inner_uid == 42 {
        for qualifier in ["author", "assignee", "review-requested"] {
            for name in names {
                steps.push(query(name, qualifier, "octocat"));
            }
        }
    }
    steps
}
fn happy(repos: &[(u64, &str)], names: &[&str]) -> Vec<GhStep> {
    let mut steps = principal(42, "octocat");
    for ((id, name), canonical) in repos.iter().zip(names) {
        steps.push(get(
            &format!("repos/{name}"),
            repo(*id, canonical, json!(true)),
        ));
    }
    steps.extend(inbox(names, 42));
    for ((id, original), canonical) in repos.iter().zip(names) {
        steps.push(get(
            &format!("repos/{original}"),
            repo(*id, canonical, json!(true)),
        ));
    }
    steps.extend(principal(42, "final-rename"));
    steps
}

#[test]
fn repo_catalog_scoped_inbox_default_empty_fenced_and_unresolved_never_query_or_rebase() {
    for kind in 0..3 {
        let fixture = Fixture::new();
        let repos = [(410, "example/one")];
        let mut steps = source_steps(&repos);
        steps.extend(principal(42, "octocat"));
        fixture.scripted_gh(&steps);
        let original = load(&fixture);
        let scope = match kind {
            0 => ActiveRemoteScopeV1::default_personal(original.source().identity()).unwrap(),
            1 => ActiveRemoteScopeV1::explicit_selection(original.source(), &[]).unwrap(),
            _ => ActiveRemoteScopeV1::unavailable(
                crate::active_remote_scope_v1::RemoteScopeFenceV1::PrivacyFence,
            ),
        };
        let cold = fixture.root.join("unopened/tagged.sqlite3");
        let error = refresh(
            original,
            Arc::new(scope),
            &provider(&fixture),
            &cold,
            &CancellationToken::default(),
        )
        .unwrap_err();
        assert!(matches!(error, ScopedInboxFailureV1::ScopeUnavailable));
        safe(&error);
        assert!(!cold.parent().unwrap().exists());
        assert_eq!(steps.len(), 7 + 2);
        exact(&fixture, &steps);
    }
    let fixture = Fixture::new();
    let mut steps = source_steps(&[(410, "example/one"), (411, "example/two")]);
    steps.extend(principal(42, "octocat"));
    steps.extend(principal(42, "octocat"));
    steps.extend(source_steps(&[(410, "example/one")]));
    steps.extend(principal(42, "octocat"));
    fixture.scripted_gh(&steps);
    let first = load(&fixture);
    let mut selected = first.intent().clone();
    selected.set(first.source(), "both", &[410, 411]).unwrap();
    let mut cache = Cache::open(&database(&fixture)).unwrap();
    let saved = crate::repository_catalogue_save_v1::save_catalogue_intent(
        &provider(&fixture),
        &mut cache,
        &first,
        selected,
        std::time::Duration::from_secs(30),
        &CancellationToken::default(),
    )
    .unwrap();
    let current = load(&fixture);
    let unresolved = ActiveRemoteScopeV1::explicitly_activate_saved_set(
        saved.intent(),
        current.source(),
        "both",
    )
    .unwrap();
    assert!(matches!(
        unresolved.consumer(current.source().identity()),
        RemoteScopeConsumerV1::Unresolved(_)
    ));
    let cold = fixture.root.join("unresolved/no-cache.sqlite3");
    let error = refresh(
        current,
        Arc::new(unresolved),
        &provider(&fixture),
        &cold,
        &CancellationToken::default(),
    )
    .unwrap_err();
    assert!(matches!(error, ScopedInboxFailureV1::ScopeUnavailable));
    safe(&error);
    assert!(!cold.parent().unwrap().exists());
    assert_eq!(steps.len(), 7 + 4 + 7 + 2);
    exact(&fixture, &steps);
}
fn exact(fixture: &Fixture, steps: &[GhStep]) {
    let rows = fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), steps.len());
    for (row, step) in rows.iter().zip(steps) {
        assert_eq!(row["matched"], true);
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(row["stdin_hex"], "");
        assert!(!step.args.iter().any(|arg| matches!(
            arg.as_str(),
            "POST" | "PATCH" | "DELETE" | "--paginate" | "--slurp"
        )));
    }
}
fn safe(error: &ScopedInboxFailureV1) {
    let message = error.to_string();
    for canary in [
        "PRIVATE_DRAFT",
        "PRIVATE_OLD_CACHE",
        "PRIVATE_SQL_CANARY",
        "PRIVATE_DIAGNOSTIC",
    ] {
        assert!(!message.contains(canary));
    }
}
fn seed_old(fixture: &Fixture, name: &str) {
    let mut cache = Cache::open(&database(fixture)).unwrap();
    let workspace = GithubWorkspace::empty(
        HostIdentity {
            host: "github.invalid".into(),
            login: "octocat".into(),
            account_id: Some(42),
            gh_version: "old".into(),
        },
        vec![name.into()],
        Default::default(),
    );
    let old = PullRequest {
        host: "github.invalid".into(),
        id: "PR_OLD_CACHED_NODE".into(),
        repository: name.into(),
        number: 7,
        title: "PRIVATE_OLD_CACHE_CANARY".into(),
        body: "PRIVATE_OLD_CACHE_BODY".into(),
        url: String::new(),
        author: Default::default(),
        state: "open".into(),
        draft: false,
        created_at: String::new(),
        updated_at: String::new(),
        queues: [Queue::Authored].into_iter().collect(),
    };
    cache.commit_sync(&workspace.scope().unwrap(),&workspace.source(),4,&[
      Record::new("inbox-meta",workspace.source(),"",json!({"coverage":Coverage {loaded:1,total_matches:1,complete:true,reasons:vec![]}})),
      Record::new("pr",PrKey::from(&old).label(),name,serde_json::to_value(old).unwrap()),
    ],true).unwrap();
}
fn git(fixture: &Fixture, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(fixture.repo())
        .args(args)
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

#[test]
fn repo_catalog_scoped_inbox_one_repo_happy11_rename_closed_refs_and_git_sql_preserved() {
    let fixture = Fixture::new();
    let repos = [(410, "example/old")];
    let mut steps = source_steps(&repos);
    steps.extend(happy(&repos, &["example/new"]));
    fixture.scripted_gh(&steps);
    let original = load(&fixture);
    let scope = active(&original, &[410]);
    seed_old(&fixture, "example/new");
    git(&fixture, &["init", "--initial-branch=main"]);
    git(&fixture, &["config", "user.name", "Scoped inbox fixture"]);
    git(
        &fixture,
        &["config", "user.email", "scoped@example.invalid"],
    );
    fs::write(fixture.repo().join("file.txt"), b"saved\n").unwrap();
    git(&fixture, &["add", "--", "file.txt"]);
    git(&fixture, &["commit", "-m", "Original Git"]);
    fs::write(fixture.repo().join("file.txt"), b"untouched dirty bytes\n").unwrap();
    let head = git(&fixture, &["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let before_intent = original.intent().to_value().unwrap();
    let output = refresh(
        original.clone(),
        scope.clone(),
        &provider(&fixture),
        &database(&fixture),
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(steps.len(), 7 + 11);
    exact(&fixture, &steps);
    assert_eq!(output.reserved_legacy_children, 32);
    assert_eq!(output.reserved_legacy_capture_bytes, 32 * 1024 * 1024);
    let measured = &output.measured_legacy_read;
    assert!(measured.sealed());
    assert_eq!(measured.children_started(), 5);
    assert_eq!(measured.requests_claimed(), 5);
    assert_eq!(measured.max_requests(), 32);
    assert_eq!(measured.successful_children(), 5);
    assert_eq!(
        measured.stdout_bytes(),
        steps[10..15]
            .iter()
            .map(|step| step.stdout.len())
            .sum::<usize>()
    );
    assert_eq!(measured.stderr_bytes(), 0);
    assert_eq!(measured.failed_reserved_bytes(), 0);
    assert_eq!(measured.in_flight_reserved_bytes(), 0);
    assert_eq!(output.closed_identity.account_id, Some(42));
    assert_eq!(output.closed_identity.login, "final-rename");
    assert_eq!(output.workspace.identity.login, "final-rename");
    assert_eq!(output.workspace.inbox.identity.login, "final-rename");
    assert_eq!(output.workspace.repositories, ["example/new"]);
    assert_eq!(output.workspace.inbox.pull_requests.len(), 1);
    assert_eq!(
        output.workspace.inbox.pull_requests[0].id,
        "PR_example/new_7"
    );
    assert_eq!(
        output.workspace.inbox.pull_requests[0].title,
        "Fresh actual scoped PR"
    );
    assert!(matches!(
        output.workspace.freshness,
        Freshness::Fresh { .. }
    ));
    let RemoteScopeConsumerV1::Explicit(refs) =
        output.current_scope.consumer(&output.closed_identity)
    else {
        panic!("closed original scope expected");
    };
    assert_eq!(refs.references()[0].original().last_name, "example/old");
    assert_eq!(
        refs.references()[0].observed().unwrap().last_name,
        "example/new"
    );
    let RemoteScopeConsumerV1::Explicit(old) = scope.consumer(original.source().identity()) else {
        panic!();
    };
    assert_eq!(
        old.references()[0].observed().unwrap().last_name,
        "example/old"
    );
    assert_eq!(original.intent().to_value().unwrap(), before_intent);
    let cache = Cache::open(&database(&fixture)).unwrap();
    cache
        .validate_repository_catalogue_intent_source(&original)
        .unwrap();
    assert_eq!(
        cache
            .draft(original.scope(), "original-review")
            .unwrap()
            .as_deref(),
        Some("PRIVATE_DRAFT_PRESERVED")
    );
    assert_eq!(git(&fixture, &["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(
        fs::read(fixture.repo().join("file.txt")).unwrap(),
        b"untouched dirty bytes\n"
    );
}

#[test]
fn repo_catalog_scoped_inbox_two_repo_happy16_shares32mib_measures8_not_claim62_actual_children() {
    let fixture = Fixture::new();
    let repos = [(410, "example/one"), (411, "example/two")];
    let mut steps = source_steps(&repos);
    steps.extend(happy(&repos, &["example/one", "example/two"]));
    fixture.scripted_gh(&steps);
    let original = load(&fixture);
    let scope = active(&original, &[410, 411]);
    let output = refresh(
        original.clone(),
        scope,
        &provider(&fixture),
        &database(&fixture),
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(steps.len(), 7 + 16);
    exact(&fixture, &steps);
    assert_eq!(output.reserved_legacy_children, 62);
    assert_eq!(output.reserved_legacy_capture_bytes, 32 * 1024 * 1024);
    let measured = &output.measured_legacy_read;
    assert!(measured.sealed());
    assert_eq!(measured.capacity_bytes(), 32 * 1024 * 1024);
    assert_eq!(measured.children_started(), 8);
    assert_eq!(measured.requests_claimed(), 8);
    assert_eq!(measured.max_requests(), 62);
    assert_eq!(measured.successful_children(), 8);
    assert_eq!(
        measured.stdout_bytes(),
        steps[11..19]
            .iter()
            .map(|step| step.stdout.len())
            .sum::<usize>()
    );
    assert_eq!(measured.stderr_bytes(), 0);
    assert_eq!(measured.failed_reserved_bytes(), 0);
    assert_eq!(measured.in_flight_reserved_bytes(), 0);
    assert_eq!(
        output.workspace.repositories,
        ["example/one", "example/two"]
    );
    assert_eq!(output.workspace.inbox.pull_requests.len(), 2);
    assert_eq!(output.workspace.inbox.coverage.loaded, 2);
    assert_eq!(output.workspace.inbox.coverage.total_matches, 2);
    assert!(output.workspace.inbox.coverage.complete);
    assert!(output
        .workspace
        .inbox
        .pull_requests
        .iter()
        .any(|pr| pr.repository == "example/one"));
    assert!(output
        .workspace
        .inbox
        .pull_requests
        .iter()
        .any(|pr| pr.repository == "example/two"));
}

#[test]
fn repo_catalog_scoped_inbox_three_repo_happy21_measures_all11_children_and_preserves_original_sql()
{
    let fixture = Fixture::new();
    let repos = [
        (410, "example/one"),
        (411, "example/two"),
        (412, "example/three"),
    ];
    let mut steps = source_steps(&repos);
    let mut reads = happy(&repos, &["example/one", "example/two", "example/three"]);
    // Every observed search record has distinct numeric/node identity; no search
    // Issue ID is promoted to canonical PR database/node authority.
    for step in &mut reads {
        if step.args.get(1).is_some_and(|arg| arg == "search/issues") {
            let mut value: Value = serde_json::from_slice(&step.stdout).unwrap();
            for item in value["items"].as_array_mut().unwrap() {
                let repository = item["repository_url"].as_str().unwrap();
                let index = repos
                    .iter()
                    .position(|(_, name)| repository.ends_with(name))
                    .unwrap();
                item["id"] = json!(30000 + index);
                item["node_id"] = json!(format!("SEARCH_{index}_7"));
            }
            step.stdout = value.to_string().into_bytes();
        }
    }
    steps.extend(reads);
    fixture.scripted_gh(&steps);
    let original = load(&fixture);
    let original_intent = original.intent().to_value().unwrap();
    let scope = active(&original, &[410, 411, 412]);
    let output = refresh(
        original.clone(),
        scope,
        &provider(&fixture),
        &database(&fixture),
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(steps.len(), 7 + 21);
    exact(&fixture, &steps);
    assert_eq!(output.reserved_legacy_children, 92);
    assert_eq!(output.reserved_legacy_capture_bytes, 32 * 1024 * 1024);
    let measured = &output.measured_legacy_read;
    assert!(measured.sealed());
    assert_eq!(measured.max_requests(), 92);
    assert_eq!(measured.children_started(), 11);
    assert_eq!(measured.requests_claimed(), 11);
    assert_eq!(measured.successful_children(), 11);
    assert_eq!(
        measured.stdout_bytes(),
        steps[12..23]
            .iter()
            .map(|step| step.stdout.len())
            .sum::<usize>()
    );
    assert_eq!(measured.stderr_bytes(), 0);
    assert_eq!(measured.failed_reserved_bytes(), 0);
    assert_eq!(
        output.workspace.repositories,
        ["example/one", "example/two", "example/three"]
    );
    assert_eq!(output.workspace.inbox.pull_requests.len(), 3);
    assert_eq!(output.workspace.inbox.coverage.loaded, 3);
    assert_eq!(output.workspace.inbox.coverage.total_matches, 3);
    assert!(output.workspace.inbox.coverage.complete);
    for (index, (_, repository)) in repos.iter().enumerate() {
        let pr = output
            .workspace
            .inbox
            .pull_requests
            .iter()
            .find(|pr| pr.repository == *repository)
            .unwrap();
        assert_eq!(pr.id, format!("SEARCH_{index}_7"));
        assert_eq!(pr.number, 7);
    }
    let RemoteScopeConsumerV1::Explicit(current) =
        output.current_scope.consumer(&output.closed_identity)
    else {
        panic!("whole original three-repository scope must remain explicit");
    };
    assert_eq!(current.references().len(), 3);
    for (current, (id, name)) in current.references().iter().zip(&repos) {
        assert_eq!(current.original().id, *id);
        assert_eq!(current.original().last_name, *name);
    }
    let cache = Cache::open(&database(&fixture)).unwrap();
    cache
        .validate_repository_catalogue_intent_source(&original)
        .unwrap();
    assert_eq!(
        cache
            .draft(original.scope(), "original-review")
            .unwrap()
            .as_deref(),
        Some("PRIVATE_DRAFT_PRESERVED")
    );
    assert_eq!(original.intent().to_value().unwrap(), original_intent);
    assert!(cache
        .list(&Scope::github("github.invalid", Some(43)).unwrap(), None)
        .unwrap()
        .is_empty());
}

#[test]
fn repo_catalog_scoped_inbox_inner_uid43_same_login_stops_before_sql_query_or_rebase() {
    let fixture = Fixture::new();
    let repos = [(410, "example/one")];
    let mut steps = source_steps(&repos);
    steps.extend(principal(42, "octocat"));
    steps.push(get(
        "repos/example/one",
        repo(410, "example/one", json!(true)),
    ));
    steps.extend(inbox(&["example/one"], 43));
    fixture.scripted_gh(&steps);
    let original = load(&fixture);
    let scope = active(&original, &[410]);
    seed_old(&fixture, "example/one");
    fs::set_permissions(database(&fixture), fs::Permissions::from_mode(0o644)).unwrap();
    let error = refresh(
        original,
        scope,
        &provider(&fixture),
        &database(&fixture),
        &CancellationToken::default(),
    )
    .unwrap_err();
    assert!(matches!(error, ScopedInboxFailureV1::LegacyReadUnavailable));
    safe(&error);
    assert_eq!(
        fs::metadata(database(&fixture))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o644,
        "inner scope failure cannot open/repair private Cache"
    );
    assert_eq!(steps.len(), 7 + 5);
    exact(&fixture, &steps);
    let sql = rusqlite::Connection::open_with_flags(
        database(&fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let other: i64 = sql
        .query_row(
            "SELECT count(*) FROM records WHERE login='user-id:43'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(other, 0);
}

#[test]
fn repo_catalog_scoped_inbox_moving_canonical_after_collection_returns_no_closed_page() {
    let fixture = Fixture::new();
    let repos = [(410, "example/one")];
    let mut steps = source_steps(&repos);
    let mut operation = happy(&repos, &["example/one"]);
    operation.truncate(9);
    operation[8] = get(
        "repos/example/one",
        repo(410, "example/moved-during-read", json!(true)),
    );
    steps.extend(operation);
    fixture.scripted_gh(&steps);
    let original = load(&fixture);
    let scope = active(&original, &[410]);
    let error = refresh(
        original.clone(),
        scope.clone(),
        &provider(&fixture),
        &database(&fixture),
        &CancellationToken::default(),
    )
    .unwrap_err();
    assert!(matches!(error, ScopedInboxFailureV1::CanonicalChanged));
    safe(&error);
    assert_eq!(steps.len(), 7 + 9);
    exact(&fixture, &steps);
    let RemoteScopeConsumerV1::Explicit(refs) = scope.consumer(original.source().identity()) else {
        panic!();
    };
    assert_eq!(
        refs.references()[0].observed().unwrap().last_name,
        "example/one"
    );
}

#[test]
fn repo_catalog_scoped_inbox_sql_cas_invalid_before_final2_hides_page_preserves_edit() {
    let fixture = Fixture::new();
    let repos = [(410, "example/one")];
    let mut steps = source_steps(&repos);
    let mut operation = happy(&repos, &["example/one"]);
    operation.truncate(9);
    steps.extend(operation);
    fixture.scripted_gh(&steps);
    let original = load(&fixture);
    let scope = active(&original, &[410]);
    let sql = rusqlite::Connection::open(database(&fixture)).unwrap();
    sql.execute_batch(&format!("CREATE TRIGGER concurrent_original_intent AFTER INSERT ON checkpoints WHEN NEW.source LIKE 'personal-inbox:%' BEGIN INSERT INTO saved_views(host,login,name,payload) VALUES('github.invalid','user-id:42','{}','{{\"concurrent\":\"PRIVATE_SQL_CANARY\"}}'); END;",original.saved_view_key())).unwrap();
    drop(sql);
    let before = original.intent().to_value().unwrap();
    let error = refresh(
        original.clone(),
        scope,
        &provider(&fixture),
        &database(&fixture),
        &CancellationToken::default(),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        ScopedInboxFailureV1::OriginalBoundary(CatalogueIntentBoundaryError::SavedIntentChanged)
    ));
    safe(&error);
    assert_eq!(steps.len(), 7 + 9);
    exact(&fixture, &steps);
    assert_eq!(original.intent().to_value().unwrap(), before);
    let cache = Cache::open(&database(&fixture)).unwrap();
    assert_eq!(
        cache
            .saved_view_bounded(original.scope(), original.saved_view_key(), 128 * 1024)
            .unwrap()
            .unwrap()["concurrent"],
        "PRIVATE_SQL_CANARY"
    );
    assert_eq!(
        cache
            .draft(original.scope(), "original-review")
            .unwrap()
            .as_deref(),
        Some("PRIVATE_DRAFT_PRESERVED")
    );
    // Existing exact refresh commits fresh DERIVATIVE inbox before close. This
    // source guard failure returns no model; it does NOT pretend SQL was rolled
    // back or that original intent was replaced/approved.
    let inbox = GithubWorkspace::empty(
        original.source().identity().clone(),
        vec!["example/one".into()],
        Default::default(),
    );
    assert_eq!(
        cache
            .sync_checkpoint(original.scope(), &inbox.source())
            .unwrap(),
        Some((1, true))
    );
}

#[test]
fn repo_catalog_scoped_inbox_legacy_error_and_unknown_pull_never_admit_stale_or_empty() {
    for unknown in [false, true] {
        let fixture = Fixture::new();
        let repos = [(410, "example/one")];
        let mut steps = source_steps(&repos);
        steps.extend(principal(42, "octocat"));
        steps.push(get(
            "repos/example/one",
            repo(
                410,
                "example/one",
                if unknown { Value::Null } else { json!(true) },
            ),
        ));
        if unknown {
            steps.extend(principal(42, "octocat"));
        } else {
            steps.extend(principal(42, "octocat"));
            let mut failed = query("example/one", "author", "octocat");
            failed.status = 1;
            failed.stdout.clear();
            failed.stderr = b"HTTP403 PRIVATE_DIAGNOSTIC".to_vec();
            steps.push(failed);
        }
        fixture.scripted_gh(&steps);
        let original = load(&fixture);
        let scope = active(&original, &[410]);
        seed_old(&fixture, "example/one");
        let error = refresh(
            original.clone(),
            scope,
            &provider(&fixture),
            &database(&fixture),
            &CancellationToken::default(),
        )
        .unwrap_err();
        assert!(matches!(
            (&error, unknown),
            (ScopedInboxFailureV1::CurrentReadUnavailable, true)
                | (ScopedInboxFailureV1::LegacyReadUnavailable, false)
        ));
        safe(&error);
        exact(&fixture, &steps);
        assert_eq!(steps.len(), 7 + if unknown { 5 } else { 6 });
        let cache = Cache::open(&database(&fixture)).unwrap();
        let inbox = GithubWorkspace::empty(
            original.source().identity().clone(),
            vec!["example/one".into()],
            Default::default(),
        );
        assert_eq!(
            cache
                .sync_checkpoint(original.scope(), &inbox.source())
                .unwrap(),
            Some((4, true))
        );
    }
}
