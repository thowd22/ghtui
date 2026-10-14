//! SOURCE-ONLY six strict contracts for Root's closed-navigation hook. Sources
//! and closed observations are produced by actual gh transport, never DTO casts.
use crate::{
    active_remote_scope_v1::{
        ActiveRemoteScopeV1, RemoteScopeConsumerV1, RemoteScopeErrorV1, RemoteScopeFenceV1,
    },
    providers::{
        gh::{
            ClosedCatalogueAccess, GithubErrorKind, GithubProvider, HostIdentity,
            RepositoryCatalogueAccessLimits, RepositoryCatalogueLimits, RepositoryCatalogueRead,
            RepositoryCatalogueSource,
        },
        runner::CancellationToken,
    },
    repository_catalogue_intent_v1::{CatalogueIntentV1, IntentReference},
    storage::{Cache, Scope},
    test_support::{Fixture, GhStep},
};
use serde_json::{json, Value};

const HOST: &str = "github.invalid";
fn get(endpoint: &str, value: Value) -> GhStep {
    get_at(HOST, endpoint, value)
}
fn get_at(host: &str, endpoint: &str, value: Value) -> GhStep {
    GhStep::read(
        &["api", endpoint, "--hostname", host, "--method", "GET"],
        &value.to_string(),
    )
}
fn account(uid: u64, login: &str) -> Vec<GhStep> {
    account_at(HOST, uid, login)
}
fn account_at(host: &str, uid: u64, login: &str) -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.99.0 (2026-10-01)\n"),
        get_at(host, "user", json!({"id":uid,"login":login})),
    ]
}
fn owner(id: u64, login: &str) -> Value {
    json!({"id":id,"node_id":format!("OWNER_scope_{id}"),"login":login,"type":"Organization"})
}
fn repo(id: u64, name: &str, node: Option<&str>, owner: &Value) -> Value {
    json!({"id":id,"node_id":node,"full_name":name,"owner":owner,"private":true,"fork":false,"archived":false,"permissions":{"pull":true},"language":"Rust","default_branch":"main"})
}
fn initial(
    host: &str,
    uid: u64,
    source: &RepositoryCatalogueSource,
    owner: &Value,
    rows: &[Value],
) -> Vec<GhStep> {
    let mut steps = account_at(host, uid, "octocat");
    if let RepositoryCatalogueSource::Organization(name) = source {
        steps.push(get_at(host, &format!("orgs/{name}"), owner.clone()));
    } else {
        assert!(matches!(
            source,
            RepositoryCatalogueSource::AuthenticatedAccessible
        ));
    }
    for page in 0..rows.len() / 100 + 1 {
        let endpoint = match source {
            RepositoryCatalogueSource::Organization(name) => format!("orgs/{name}/repos?type=all&sort=full_name&direction=asc&per_page=100&page={}",page+1),
            RepositoryCatalogueSource::AuthenticatedAccessible => format!("user/repos?visibility=all&affiliation=owner,collaborator,organization_member&sort=full_name&direction=asc&per_page=100&page={}",page+1),
            _ => unreachable!("No synthetic public-user fixture authority"),
        };
        let start = page * 100;
        steps.push(get_at(
            host,
            &endpoint,
            json!(&rows[start..(start + 100).min(rows.len())]),
        ));
    }
    if let RepositoryCatalogueSource::Organization(name) = source {
        steps.push(get_at(host, &format!("orgs/{name}"), owner.clone()));
    }
    steps.extend(account_at(host, uid, "octocat"));
    steps
}
fn actual(
    source: RepositoryCatalogueSource,
    original_owner: Value,
    rows: Vec<Value>,
    tail: Vec<GhStep>,
) -> (Fixture, GithubProvider, RepositoryCatalogueRead, usize) {
    actual_at(HOST, 42, source, original_owner, rows, tail)
}
fn actual_at(
    host: &str,
    uid: u64,
    source: RepositoryCatalogueSource,
    original_owner: Value,
    rows: Vec<Value>,
    tail: Vec<GhStep>,
) -> (Fixture, GithubProvider, RepositoryCatalogueRead, usize) {
    let fixture = Fixture::new();
    let mut steps = initial(host, uid, &source, &original_owner, &rows);
    let loaded_count = steps.len();
    steps.extend(tail);
    fixture.scripted_gh(&steps);
    let provider = GithubProvider::new(host)
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"));
    let read = provider
        .repository_catalogue_read(
            None,
            &source,
            &RepositoryCatalogueLimits::default(),
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(read.repositories().len(), rows.len());
    reads(&fixture, loaded_count);
    (fixture, provider, read, loaded_count)
}
fn reads(fixture: &Fixture, expected: usize) -> Vec<Value> {
    let journal = std::fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    let rows = journal
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), expected);
    assert!(expected > 0);
    assert!(rows
        .iter()
        .all(|row| row["matched"] == true && row["stdin_hex"] == ""));
    assert!(rows
        .iter()
        .all(|row| row["args"][0] == "--version" || row["args"][5] == "GET"));
    assert!(!rows.iter().any(
        |row| row["args"].as_array().unwrap().iter().any(|arg| matches!(
            arg.as_str(),
            Some("POST" | "PATCH" | "DELETE" | "graphql" | "--paginate" | "--slurp")
        ))
    ));
    rows
}
fn refs(
    scope: &ActiveRemoteScopeV1,
    identity: &HostIdentity,
) -> Vec<(IntentReference, Option<IntentReference>)> {
    let rows = match scope.consumer(identity) {
        RemoteScopeConsumerV1::Explicit(rows) => rows.references(),
        RemoteScopeConsumerV1::Unresolved(rows) => rows,
        _ => panic!("Original explicit references, never a default/empty fallback"),
    };
    rows.iter()
        .map(|row| (row.original().clone(), row.observed().cloned()))
        .collect()
}
fn close_all(
    provider: &GithubProvider,
    read: &RepositoryCatalogueRead,
    references: &[IntentReference],
) -> ClosedCatalogueAccess {
    let mut session = provider
        .begin_repository_catalogue_access(
            read,
            &RepositoryCatalogueAccessLimits::default(),
            &CancellationToken::default(),
        )
        .unwrap();
    for reference in references {
        session
            .verify(reference, &CancellationToken::default())
            .unwrap();
    }
    session.close(&CancellationToken::default()).unwrap()
}

#[test]
fn active_remote_scope_closed_genuine_leaf_and_owner_rename_update_only_observed_navigation() {
    for owner_renamed in [false, true] {
        let old_owner = owner(71, "example");
        let new_owner = owner(
            71,
            if owner_renamed {
                "renamed-owner"
            } else {
                "example"
            },
        );
        let current_name = if owner_renamed {
            "renamed-owner/current-leaf"
        } else {
            "example/current-leaf"
        };
        let mut tail = account(42, "octocat");
        tail.push(get(
            "repos/example/original",
            repo(410, current_name, Some("R_410"), &new_owner),
        ));
        tail.extend(account(42, "octocat"));
        let (fixture, provider, read, loaded) = actual(
            RepositoryCatalogueSource::Organization("example".into()),
            old_owner.clone(),
            vec![repo(410, "example/original", Some("R_410"), &old_owner)],
            tail,
        );
        assert_eq!(loaded, 7);
        let mut intent = CatalogueIntentV1::new(read.context()).unwrap();
        intent.set(&read, "OriginalCase", &[410]).unwrap();
        let saved = intent.to_value().unwrap();
        let original = intent.sets()["OriginalCase"][&410].clone();
        let mut scope =
            ActiveRemoteScopeV1::explicitly_activate_saved_set(&intent, &read, "OriginalCase")
                .unwrap();
        let source_before = serde_json::to_value(read.repositories()).unwrap();
        let sql_scope = Scope::github(HOST, Some(42)).unwrap();
        let cache = Cache::open(&fixture.root.join("closed-scope.sqlite3")).unwrap();
        cache
            .save_view(&sql_scope, "original-catalogue", &saved)
            .unwrap();
        cache
            .put_draft(&sql_scope, "original-draft", "ORIGINAL42 draft")
            .unwrap();
        let mut session = provider
            .begin_repository_catalogue_access(&read, &Default::default(), &Default::default())
            .unwrap();
        assert!(session
            .verify(&original, &Default::default())
            .unwrap()
            .name_changed());
        let closed = session
            .close_with_guard(&Default::default(), || {
                assert_eq!(
                    cache
                        .saved_view_bounded(&sql_scope, "original-catalogue", 128 * 1024)
                        .unwrap()
                        .unwrap(),
                    saved
                );
                Ok(())
            })
            .unwrap();
        let before_hook = reads(&fixture, 12);
        scope.observe_closed_access(&read, &closed).unwrap();
        let observed = refs(&scope, closed.identity());
        assert_eq!(observed[0].0, original);
        assert_eq!(observed[0].1.as_ref().unwrap().id, 410);
        assert_eq!(
            observed[0].1.as_ref().unwrap().node_id.as_deref(),
            Some("R_410")
        );
        assert_eq!(observed[0].1.as_ref().unwrap().last_name, current_name);
        assert_eq!(intent.to_value().unwrap(), saved);
        assert_eq!(
            serde_json::to_value(read.repositories()).unwrap(),
            source_before
        );
        assert_eq!(read.owner().unwrap().login, "example");
        assert_eq!(closed.observation(410).unwrap().original(), &original);
        assert_eq!(
            cache
                .saved_view_bounded(&sql_scope, "original-catalogue", 128 * 1024)
                .unwrap()
                .unwrap(),
            saved
        );
        assert_eq!(
            cache
                .draft(&sql_scope, "original-draft")
                .unwrap()
                .as_deref(),
            Some("ORIGINAL42 draft")
        );
        assert_eq!(
            reads(&fixture, 12),
            before_hook,
            "Pure navigation update adds no transport/write"
        );
    }
}

#[test]
fn active_remote_scope_closed_same_uid_login_rename_is_navigation_not_reused_login_adoption() {
    for final_uid in [42, 43] {
        let original_owner = owner(71, "example");
        let mut tail = account(42, "first-new-login");
        tail.push(get(
            "repos/example/original",
            repo(410, "example/new-leaf", Some("R_410"), &original_owner),
        ));
        tail.extend(account(
            final_uid,
            if final_uid == 42 {
                "second-new-login"
            } else {
                "octocat"
            },
        ));
        let (fixture, provider, read, _) = actual(
            RepositoryCatalogueSource::Organization("example".into()),
            original_owner.clone(),
            vec![repo(
                410,
                "example/original",
                Some("R_410"),
                &original_owner,
            )],
            tail,
        );
        let mut scope = ActiveRemoteScopeV1::explicit_selection(&read, &[410]).unwrap();
        let before = refs(&scope, read.identity());
        let mut session = provider
            .begin_repository_catalogue_access(&read, &Default::default(), &Default::default())
            .unwrap();
        session.verify(&before[0].0, &Default::default()).unwrap();
        let result = session.close(&Default::default());
        if final_uid == 42 {
            let closed = result.unwrap();
            assert_eq!(closed.identity().login, "second-new-login");
            scope.observe_closed_access(&read, &closed).unwrap();
            assert!(matches!(
                scope.consumer(closed.identity()),
                RemoteScopeConsumerV1::Explicit(_)
            ));
            assert_eq!(refs(&scope, closed.identity())[0].0, before[0].0);
            assert_eq!(
                refs(&scope, closed.identity())[0]
                    .1
                    .as_ref()
                    .unwrap()
                    .last_name,
                "example/new-leaf"
            );
        } else {
            assert!(matches!(
                result.unwrap_err().kind,
                GithubErrorKind::IdentityChanged
            ));
            assert_eq!(refs(&scope, read.identity()), before);
            let changed = HostIdentity {
                account_id: Some(43),
                ..read.identity().clone()
            };
            assert!(matches!(
                scope.consumer(&changed),
                RemoteScopeConsumerV1::Fenced(RemoteScopeFenceV1::PrincipalChanged)
            ));
        }
        reads(&fixture, 12);
    }
}

#[test]
fn active_remote_scope_closed_denied_or_recreated_member_becomes_unresolved_without_all_fallback() {
    for mutation in 0..3 {
        let original_owner = owner(71, "example");
        let mut tail = account(42, "octocat");
        let mut current = repo(
            if mutation == 1 { 411 } else { 410 },
            "example/original",
            Some(if mutation == 2 {
                "R_recreated"
            } else {
                "R_410"
            }),
            &original_owner,
        );
        let mut response = get("repos/example/original", current.take());
        if mutation == 0 {
            response.stdout.clear();
            response.status = 1;
            response.stderr = b"gh: private denied CANARY (HTTP 403)\n".to_vec();
        }
        tail.push(response);
        tail.extend(account(42, "octocat"));
        let (fixture, provider, read, _) = actual(
            RepositoryCatalogueSource::Organization("example".into()),
            original_owner.clone(),
            vec![repo(
                410,
                "example/original",
                Some("R_410"),
                &original_owner,
            )],
            tail,
        );
        let mut scope = ActiveRemoteScopeV1::explicit_selection(&read, &[410]).unwrap();
        let original = refs(&scope, read.identity())[0].0.clone();
        let mut session = provider
            .begin_repository_catalogue_access(&read, &Default::default(), &Default::default())
            .unwrap();
        let error = session.verify(&original, &Default::default()).unwrap_err();
        assert!(matches!(
            error.kind,
            GithubErrorKind::Permission | GithubErrorKind::IdentityChanged
        ));
        assert!(!error.message.contains("CANARY"));
        let closed = session.close(&Default::default()).unwrap();
        assert!(closed.verified().is_empty());
        assert_eq!(closed.failures().len(), 1);
        scope.observe_closed_access(&read, &closed).unwrap();
        let RemoteScopeConsumerV1::Unresolved(rows) = scope.consumer(closed.identity()) else {
            panic!("Failed current member must never become all repositories")
        };
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].original(), &original);
        assert!(rows[0].observed().is_none());
        reads(&fixture, 12);
    }
}

#[test]
fn active_remote_scope_closed_foreign_source_owner_and_original_locator_cannot_retarget() {
    let original_owner = owner(71, "example");
    let (original_fixture, _, original, _) = actual(
        RepositoryCatalogueSource::Organization("example".into()),
        original_owner.clone(),
        vec![repo(
            410,
            "example/original",
            Some("R_410"),
            &original_owner,
        )],
        vec![],
    );
    let mut scope = ActiveRemoteScopeV1::explicit_selection(&original, &[410]).unwrap();
    let original_refs = refs(&scope, original.identity());
    for foreign_source in [false, true] {
        let foreign_owner = owner(if foreign_source { 71 } else { 72 }, "example");
        let source = if foreign_source {
            RepositoryCatalogueSource::AuthenticatedAccessible
        } else {
            RepositoryCatalogueSource::Organization("example".into())
        };
        let mut tail = account(42, "octocat");
        tail.push(get(
            "repos/example/original",
            repo(
                410,
                "example/foreign-current",
                Some("R_410"),
                &foreign_owner,
            ),
        ));
        tail.extend(account(42, "octocat"));
        let (fixture, provider, read, loaded) = actual(
            source,
            foreign_owner.clone(),
            vec![repo(410, "example/original", Some("R_410"), &foreign_owner)],
            tail,
        );
        let reference = ActiveRemoteScopeV1::explicit_selection(&read, &[410]).unwrap();
        let closed = close_all(
            &provider,
            &read,
            &[refs(&reference, read.identity())[0].0.clone()],
        );
        assert_eq!(
            scope.observe_closed_access(&original, &closed),
            Err(RemoteScopeErrorV1::OriginalSourceChanged)
        );
        assert_eq!(refs(&scope, original.identity()), original_refs);
        reads(&fixture, loaded + 5);
    }
    for (host, uid) in [(HOST, 43), ("elsewhere.invalid", 42)] {
        let mut tail = account_at(host, uid, "octocat");
        tail.push(get_at(
            host,
            "repos/example/original",
            repo(410, "example/foreign-scope", Some("R_410"), &original_owner),
        ));
        tail.extend(account_at(host, uid, "octocat"));
        let (fixture, provider, read, _) = actual_at(
            host,
            uid,
            RepositoryCatalogueSource::Organization("example".into()),
            original_owner.clone(),
            vec![repo(
                410,
                "example/original",
                Some("R_410"),
                &original_owner,
            )],
            tail,
        );
        let original_from_foreign = ActiveRemoteScopeV1::explicit_selection(&read, &[410]).unwrap();
        let closed = close_all(
            &provider,
            &read,
            &[refs(&original_from_foreign, read.identity())[0].0.clone()],
        );
        assert_eq!(closed.identity().account_id, Some(uid));
        assert_eq!(closed.identity().host, host);
        assert_eq!(
            scope.observe_closed_access(&original, &closed),
            Err(RemoteScopeErrorV1::OriginalSourceChanged)
        );
        assert_eq!(refs(&scope, original.identity()), original_refs);
        reads(&fixture, 12);
    }
    // A real exact-ID GET may validate a different user-entered navigation
    // locator. It still cannot rewrite the hook's exact immutable original ref.
    let mut tail = account(42, "octocat");
    tail.push(get(
        "repos/example/different-locator",
        repo(410, "example/current", Some("R_410"), &original_owner),
    ));
    tail.extend(account(42, "octocat"));
    let (fixture, provider, read, _) = actual(
        RepositoryCatalogueSource::Organization("example".into()),
        original_owner.clone(),
        vec![repo(
            410,
            "example/original",
            Some("R_410"),
            &original_owner,
        )],
        tail,
    );
    let other = IntentReference {
        last_name: "example/different-locator".into(),
        ..original_refs[0].0.clone()
    };
    let closed = close_all(&provider, &read, &[other]);
    scope.observe_closed_access(&original, &closed).unwrap();
    let rows = refs(&scope, original.identity());
    assert_eq!(rows[0].0, original_refs[0].0);
    assert!(rows[0].1.is_none());
    assert!(matches!(
        scope.consumer(original.identity()),
        RemoteScopeConsumerV1::Unresolved(_)
    ));
    reads(&fixture, 12);
    reads(&original_fixture, 7);
}

#[test]
fn active_remote_scope_closed_preclone_text_budget_refusal_is_atomic_for_original_observations() {
    let original_owner = owner(71, "example");
    let rows = (0..150)
        .map(|index| {
            repo(
                10000 + index,
                &format!("example/repo{index:04}"),
                None,
                &original_owner,
            )
        })
        .collect::<Vec<_>>();
    let mut tail = account(42, "octocat");
    for index in 0..150 {
        let node = format!("N{index:04}{}", "x".repeat(995));
        assert_eq!(node.len(), 1000);
        tail.push(get(
            &format!("repos/example/repo{index:04}"),
            repo(
                10000 + index,
                &format!("example/repo{index:04}"),
                Some(&node),
                &original_owner,
            ),
        ));
    }
    tail.extend(account(42, "octocat"));
    let (fixture, provider, read, loaded) = actual(
        RepositoryCatalogueSource::Organization("example".into()),
        original_owner,
        rows,
        tail,
    );
    assert_eq!(loaded, 8);
    let mut scope =
        ActiveRemoteScopeV1::explicit_selection(&read, &(10000..10150).collect::<Vec<_>>())
            .unwrap();
    let before = refs(&scope, read.identity());
    let source_before = serde_json::to_value(read.repositories()).unwrap();
    let closed = close_all(
        &provider,
        &read,
        &before.iter().map(|row| row.0.clone()).collect::<Vec<_>>(),
    );
    assert_eq!(closed.verified().len(), 150);
    let journal = reads(&fixture, 162);
    assert_eq!(
        scope.observe_closed_access(&read, &closed),
        Err(RemoteScopeErrorV1::CopiedTextLimit)
    );
    assert_eq!(
        refs(&scope, read.identity()),
        before,
        "Refusal must not partially copy/promote first members"
    );
    assert_eq!(
        serde_json::to_value(read.repositories()).unwrap(),
        source_before
    );
    assert_eq!(reads(&fixture, 162), journal);
}

#[test]
fn active_remote_scope_closed_fenced_default_and_explicit_empty_never_release_or_expand() {
    let original_owner = owner(71, "example");
    let mut tail = account(42, "octocat");
    tail.push(get(
        "repos/example/original",
        repo(410, "example/current", Some("R_410"), &original_owner),
    ));
    tail.extend(account(42, "octocat"));
    let (fixture, provider, read, _) = actual(
        RepositoryCatalogueSource::Organization("example".into()),
        original_owner.clone(),
        vec![repo(
            410,
            "example/original",
            Some("R_410"),
            &original_owner,
        )],
        tail,
    );
    let selected = ActiveRemoteScopeV1::explicit_selection(&read, &[410]).unwrap();
    let closed = close_all(
        &provider,
        &read,
        &[refs(&selected, read.identity())[0].0.clone()],
    );
    let mut fenced = selected.clone();
    fenced.fence(RemoteScopeFenceV1::PrivacyFence);
    assert_eq!(
        fenced.observe_closed_access(&read, &closed),
        Err(RemoteScopeErrorV1::OriginalSourceChanged)
    );
    assert!(matches!(
        fenced.consumer(closed.identity()),
        RemoteScopeConsumerV1::Fenced(RemoteScopeFenceV1::PrivacyFence)
    ));
    let mut default = ActiveRemoteScopeV1::default_personal(read.identity()).unwrap();
    assert_eq!(
        default.observe_closed_access(&read, &closed),
        Err(RemoteScopeErrorV1::OriginalSourceChanged)
    );
    assert!(matches!(
        default.consumer(closed.identity()),
        RemoteScopeConsumerV1::DefaultPersonal
    ));
    let mut empty = ActiveRemoteScopeV1::explicit_selection(&read, &[]).unwrap();
    empty.observe_closed_access(&read, &closed).unwrap();
    assert!(matches!(
        empty.consumer(closed.identity()),
        RemoteScopeConsumerV1::ExplicitEmpty
    ));
    let mut unavailable =
        ActiveRemoteScopeV1::unavailable(RemoteScopeFenceV1::PrincipalUnavailable);
    assert!(unavailable.observe_closed_access(&read, &closed).is_err());
    assert!(matches!(
        unavailable.consumer(closed.identity()),
        RemoteScopeConsumerV1::Fenced(RemoteScopeFenceV1::PrincipalUnavailable)
    ));
    reads(&fixture, 12);
}
