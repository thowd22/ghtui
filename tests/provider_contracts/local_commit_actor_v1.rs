//! Strict real-Git/real-SQL/genuine-catalogue actor contracts.
//! No public DTO or boolean stands in for the actual source factory.
use super::*;
#[path = "local_commit_actor_boundaries_v1.rs"]
mod boundaries;
#[path = "local_commit_actor_denial_v1.rs"]
mod denial;
#[path = "local_commit_actor_initial_sql_v1.rs"]
mod initial_sql;
#[path = "local_commit_actor_owner_pin_v1.rs"]
mod owner_pin;
#[path = "related_pr_v1.rs"]
mod related_pr_v1;
use ghtui::{
    git::{GitRepository, OriginalLocalCommitActors},
    identity::GitRole,
    providers::gh::{
        LocalCommitActorReadLimits, LocalCommitActorReadTarget, LocalCommitActorUnavailable,
        LocalCommitRoleReport, RepositoryCatalogueSource,
    },
    repository_catalogue_save_v1::load_catalogue_intent_source,
    storage::{Cache, CatalogueIntentSource, Scope},
};
use std::process::{Command, Stdio};

fn get(endpoint: &str, value: Value) -> GhStep {
    step(api_args(endpoint), value)
}
fn principal(uid: u64, login: &str) -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.99.0 (2026-10-01)\n"),
        get("user", json!({"id":uid,"login":login})),
    ]
}
fn owner() -> Value {
    json!({"id":71,"node_id":"OWNER_actor_71","login":"example","type":"Organization"})
}
fn repo() -> Value {
    json!({"id":101,"node_id":"REPO_actor_101","full_name":"example/original","owner":owner(),"permissions":{"pull":true},"private":true,"archived":false})
}
fn initial() -> Vec<GhStep> {
    let mut steps = principal(42, "octocat");
    steps.push(get("orgs/example", owner()));
    steps.push(get(
        "orgs/example/repos?type=all&sort=full_name&direction=asc&per_page=100&page=1",
        json!([repo()]),
    ));
    steps.push(get("orgs/example", owner()));
    steps.extend(principal(42, "octocat"));
    steps
}
fn host_actor(id: u64, login: &str) -> Value {
    json!({"id":id,"node_id":format!("ACTOR_original_{id}"),"login":login,"name":format!("Observed {login}"),"type":"User","avatar_url":format!("https://avatars.githubusercontent.com/u/{id}?v=4"),"url":format!("https://github.invalid/api/v3/users/{login}")})
}
fn response(sha: &str) -> Value {
    json!({"sha":sha,"url":format!("https://github.invalid/api/v3/repos/example/original/commits/{sha}"),"html_url":format!("https://github.invalid/example/original/commit/{sha}"),"commit":{"url":format!("https://github.invalid/api/v3/repos/example/original/git/commits/{sha}"),"author":{"name":"Raw Writer","email":"write@example.invalid","date":"2023-11-14T22:13:20Z"},"committer":{"name":"Raw Recorder","email":"record@example.invalid","date":"2023-11-14T22:15:00Z"}},"author":host_actor(201,"host-writer"),"committer":host_actor(202,"host-recorder")})
}
fn protocol(sha: &str, value: Value, closing_uid: u64) -> Vec<GhStep> {
    let mut steps = initial();
    steps.extend(principal(42, "renamed-requester"));
    steps.push(get("repos/example/original", repo()));
    steps.push(get(
        &format!("repos/example/original/commits/{sha}?per_page=1"),
        value,
    ));
    steps.push(get("repos/example/original", repo()));
    steps.extend(principal(closing_uid, "octocat"));
    steps
}
fn git(root: &std::path::Path, args: &[&str], input: Option<&[u8]>) -> Vec<u8> {
    let mut cmd = Command::new("git");
    cmd.current_dir(root)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if input.is_some() {
        cmd.stdin(Stdio::piped());
    }
    let mut child = cmd.spawn().unwrap();
    if let Some(input) = input {
        child.stdin.take().unwrap().write_all(input).unwrap();
    }
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "Git fixture {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}
fn raw_local(fixture: &Fixture, format: &str) -> Arc<OriginalLocalCommitActors> {
    git(
        &fixture.repo(),
        &[
            "init",
            "-q",
            "--initial-branch=main",
            &format!("--object-format={format}"),
        ],
        None,
    );
    git(
        &fixture.repo(),
        &["config", "user.name", "Fixture Original"],
        None,
    );
    git(
        &fixture.repo(),
        &["config", "user.email", "fixture@example.invalid"],
        None,
    );
    fs::write(fixture.repo().join("tracked"), b"original fixture\n").unwrap();
    git(&fixture.repo(), &["add", "tracked"], None);
    git(&fixture.repo(), &["commit", "-qm", "root"], None);
    let tree = String::from_utf8(git(&fixture.repo(), &["rev-parse", "HEAD^{tree}"], None))
        .unwrap()
        .trim()
        .to_owned();
    let raw=format!("tree {tree}\nauthor Raw Writer <write@example.invalid> 1700000000 +0530\ncommitter Raw Recorder <record@example.invalid> 1700000100 -0700\n\nOriginal raw host evidence\n");
    let sha = String::from_utf8(git(
        &fixture.repo(),
        &["hash-object", "-t", "commit", "-w", "--stdin"],
        Some(raw.as_bytes()),
    ))
    .unwrap()
    .trim()
    .to_owned();
    git(
        &fixture.repo(),
        &["update-ref", "refs/heads/main", &sha],
        None,
    );
    fs::write(fixture.repo().join("tracked"), b"dirty original\n").unwrap();
    let repository = GitRepository::discover(fixture.repo()).unwrap().unwrap();
    Arc::new(
        OriginalLocalCommitActors::read(
            &repository,
            &sha,
            &Default::default(),
            &Default::default(),
        )
        .unwrap(),
    )
}
fn source(fixture: &Fixture) -> (Cache, Arc<CatalogueIntentSource>) {
    let mut cache = Cache::open(&fixture.root.join("actor.sqlite3")).unwrap();
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    cache
        .put_draft(&scope, "actor-unsent", "original42 actor draft")
        .unwrap();
    let loaded = load_catalogue_intent_source(
        &provider(fixture),
        &mut cache,
        None,
        &RepositoryCatalogueSource::Organization("example".into()),
        &Default::default(),
        &Default::default(),
    )
    .unwrap();
    (cache, Arc::new(loaded))
}
fn target<'a>(
    original: &'a Arc<CatalogueIntentSource>,
    local: &'a Arc<OriginalLocalCommitActors>,
) -> LocalCommitActorReadTarget<'a> {
    LocalCommitActorReadTarget {
        original,
        selected_index: 0,
        local,
    }
}
fn reads(fixture: &Fixture, count: usize) {
    let rows = journal(fixture);
    assert_eq!(rows.len(), count);
    assert!(rows
        .iter()
        .all(|row| row["matched"] == true && row["stdin_hex"] == ""));
    assert!(rows
        .iter()
        .all(|row| row["args"][0] == "--version" || row["args"][5] == "GET"));
    assert!(!rows
        .iter()
        .any(|row| row["args"]
            .as_array()
            .unwrap()
            .iter()
            .any(|arg| arg == "--paginate"
                || arg == "--slurp"
                || arg == "PATCH"
                || arg == "POST"
                || arg.as_str().is_some_and(|arg| arg.starts_with("search/")))));
}
fn unchanged(fixture: &Fixture, cache: &Cache, head: &[u8], index: &[u8]) {
    assert_eq!(git(&fixture.repo(), &["rev-parse", "HEAD"], None), head);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(
        fs::read(fixture.repo().join("tracked")).unwrap(),
        b"dirty original\n"
    );
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    assert_eq!(
        cache.draft(&scope, "actor-unsent").unwrap().as_deref(),
        Some("original42 actor draft")
    );
    let other = Scope::github("github.invalid", Some(43)).unwrap();
    assert!(cache.list(&other, None).unwrap().is_empty());
    assert!(cache.saved_views(&other).unwrap().is_empty());
    assert!(cache.draft(&other, "actor-unsent").unwrap().is_none());
}

#[test]
fn git_actor_association_provider_genuine_sha1_sha256_both_roles_seven_reads_no_mapping_claim() {
    for format in ["sha1", "sha256"] {
        let fixture = Fixture::new();
        let local = raw_local(&fixture, format);
        fixture.scripted_gh(&protocol(local.sha(), response(local.sha()), 42));
        let (cache, original) = source(&fixture);
        let head = git(&fixture.repo(), &["rev-parse", "HEAD"], None);
        let index = fs::read(fixture.repo().join(".git/index")).unwrap();
        let report = provider(&fixture)
            .local_commit_actor_read(
                target(&original, &local),
                &cache,
                &Default::default(),
                &Default::default(),
                || Ok(()),
            )
            .unwrap();
        assert!(Arc::ptr_eq(report.original_source(), &original));
        assert!(Arc::ptr_eq(report.original_local(), &local));
        assert_eq!(report.sha(), local.sha());
        assert_eq!(report.repository().id(), 101);
        assert_eq!(report.repository().node_id(), "REPO_actor_101");
        assert_eq!(report.identity().account_id, Some(42));
        for (role, id, login) in [
            (GitRole::Author, 201, "host-writer"),
            (GitRole::Committer, 202, "host-recorder"),
        ] {
            let LocalCommitRoleReport::Reported(actor) = report.role(role) else {
                panic!("Actual supplied role must remain reported")
            };
            assert_eq!(actor.database_id(), id);
            assert_eq!(actor.actor().login, login);
            assert_eq!(actor.display().git_role, Some(role));
            assert!(actor.label().contains(&format!("@{login}")));
        }
        assert_eq!(
            report.label(),
            "GitHub reports this commit in selected repository"
        );
        assert!(report
            .limitations()
            .iter()
            .any(|value| value.contains("not a verified local-repository mapping")));
        assert_eq!(report.accounting().accounting().requests(), 6);
        assert_eq!(
            report
                .accounting()
                .external_read_accounting()
                .unwrap()
                .children_started(),
            1
        );
        let ledger = report.accounting().external_read_accounting().unwrap();
        assert_eq!(
            ledger.stdout_bytes(),
            serde_json::to_vec(&response(local.sha())).unwrap().len()
        );
        assert_eq!(ledger.stderr_bytes(), 0);
        assert_eq!(ledger.failed_reserved_bytes(), 0);
        assert!(ledger.sealed());
        reads(&fixture, 14);
        unchanged(&fixture, &cache, &head, &index);
    }
}
#[test]
fn git_actor_association_provider_null_roles_are_independent_not_opposite_account() {
    for field in ["author", "committer"] {
        let fixture = Fixture::new();
        let local = raw_local(&fixture, "sha1");
        let mut value = response(local.sha());
        value[field] = Value::Null;
        fixture.scripted_gh(&protocol(local.sha(), value, 42));
        let (cache, original) = source(&fixture);
        let report = provider(&fixture)
            .local_commit_actor_read(
                target(&original, &local),
                &cache,
                &Default::default(),
                &Default::default(),
                || Ok(()),
            )
            .unwrap();
        let role = if field == "author" {
            GitRole::Author
        } else {
            GitRole::Committer
        };
        assert!(matches!(
            report.role(role),
            LocalCommitRoleReport::Unavailable(LocalCommitActorUnavailable::NoReportedAccount)
        ));
        let opposite = if role == GitRole::Author {
            GitRole::Committer
        } else {
            GitRole::Author
        };
        assert!(matches!(
            report.role(opposite),
            LocalCommitRoleReport::Reported(_)
        ));
        reads(&fixture, 14);
    }
}
#[test]
fn git_actor_association_provider_raw_name_email_seconds_full_sha_and_url_mismatch_refuse() {
    for mutation in 0..6 {
        let fixture = Fixture::new();
        let local = raw_local(&fixture, "sha1");
        let mut value = response(local.sha());
        match mutation {
            0 => value["sha"] = json!("a".repeat(40)),
            1 => value["commit"]["author"]["name"] = json!("Raw Recorder"),
            2 => value["commit"]["author"]["email"] = json!("record@example.invalid"),
            3 => value["commit"]["committer"]["date"] = json!("2023-11-14T22:15:01Z"),
            4 => {
                value["url"] = json!(format!(
                    "https://elsewhere.invalid/repos/example/original/commits/{}",
                    local.sha()
                ))
            }
            _ => value["author"]["url"] = json!("https://github.invalid/api/v3/users/wrong-user"),
        };
        fixture.scripted_gh(&protocol(local.sha(), value, 42));
        let (cache, original) = source(&fixture);
        let failure = provider(&fixture)
            .local_commit_actor_read(
                target(&original, &local),
                &cache,
                &Default::default(),
                &Default::default(),
                || Ok(()),
            )
            .unwrap_err();
        assert!(matches!(failure.kind, GithubErrorKind::IdentityChanged));
        reads(&fixture, 14);
    }
}
#[test]
fn git_actor_association_provider_missing_identity_type_and_optional_display_limits_stay_unknown() {
    for mutation in 0..6 {
        let fixture = Fixture::new();
        let local = raw_local(&fixture, "sha1");
        let mut value = response(local.sha());
        match mutation {
            0 => value["commit"]["author"]
                .as_object_mut()
                .unwrap()
                .remove("name"),
            1 => value["author"].as_object_mut().unwrap().remove("id"),
            2 => value["author"].as_object_mut().unwrap().remove("type"),
            3 => value["author"].as_object_mut().unwrap().remove("login"),
            4 => {
                value["author"]["name"] = json!("x".repeat(4097));
                None
            }
            _ => {
                value["author"]["node_id"] = json!(0);
                None
            }
        };
        fixture.scripted_gh(&protocol(local.sha(), value, 42));
        let (cache, original) = source(&fixture);
        let report = provider(&fixture)
            .local_commit_actor_read(
                target(&original, &local),
                &cache,
                &Default::default(),
                &Default::default(),
                || Ok(()),
            )
            .unwrap();
        assert!(matches!(
            report.role(GitRole::Author),
            LocalCommitRoleReport::Unavailable(_)
        ));
        assert!(matches!(
            report.role(GitRole::Committer),
            LocalCommitRoleReport::Reported(_)
        ));
        reads(&fixture, 14);
    }
}
#[test]
fn git_actor_association_provider_repo_recreated_owner_drift_and_unknown_pull_do_not_fetch_commit()
{
    for mutation in 0..4 {
        let fixture = Fixture::new();
        let local = raw_local(&fixture, "sha1");
        let mut steps = initial();
        steps.extend(principal(42, "octocat"));
        let mut value = repo();
        match mutation {
            0 => value["id"] = json!(102),
            1 => value["node_id"] = json!("REPO_changed"),
            2 => value["owner"]["id"] = json!(72),
            _ => value["permissions"]["pull"] = Value::Null,
        };
        steps.push(get("repos/example/original", value));
        steps.extend(principal(42, "octocat"));
        fixture.scripted_gh(&steps);
        let (cache, original) = source(&fixture);
        let failure = provider(&fixture)
            .local_commit_actor_read(
                target(&original, &local),
                &cache,
                &Default::default(),
                &Default::default(),
                || Ok(()),
            )
            .unwrap_err();
        assert!(matches!(
            failure.kind,
            GithubErrorKind::IdentityChanged | GithubErrorKind::Unsupported
        ));
        reads(&fixture, 12);
    }
}
#[test]
fn git_actor_association_provider_same_login_uid_switch_and_final_auth_loss_hide_observation() {
    for authentication in [false, true] {
        let fixture = Fixture::new();
        let local = raw_local(&fixture, "sha1");
        let mut steps = protocol(local.sha(), response(local.sha()), 43);
        if authentication {
            steps[13].status = 1;
            steps[13].stdout.clear();
            steps[13].stderr = b"HTTP 401: Requires authentication\n".to_vec();
        }
        fixture.scripted_gh(&steps);
        let (cache, original) = source(&fixture);
        let failure = provider(&fixture)
            .local_commit_actor_read(
                target(&original, &local),
                &cache,
                &Default::default(),
                &Default::default(),
                || Ok(()),
            )
            .unwrap_err();
        assert!(matches!(
            failure.kind,
            GithubErrorKind::IdentityChanged | GithubErrorKind::Authentication
        ));
        assert_eq!(
            cache
                .draft(
                    &Scope::github("github.invalid", Some(42)).unwrap(),
                    "actor-unsent"
                )
                .unwrap()
                .as_deref(),
            Some("original42 actor draft")
        );
        reads(&fixture, 14);
    }
}
#[test]
fn git_actor_association_provider_actual_source_clear_before_final_account_stops_publication() {
    let fixture = Fixture::new();
    let local = raw_local(&fixture, "sha1");
    let steps = protocol(local.sha(), response(local.sha()), 42);
    fixture.scripted_gh(&steps[..12]);
    let (cache, original) = source(&fixture);
    let failure = provider(&fixture)
        .local_commit_actor_read(
            target(&original, &local),
            &cache,
            &Default::default(),
            &Default::default(),
            || {
                let mut concurrent = Cache::open(&fixture.root.join("actor.sqlite3")).unwrap();
                concurrent.clear_scope(original.scope(), false).unwrap();
                Ok(())
            },
        )
        .unwrap_err();
    assert!(matches!(failure.kind, GithubErrorKind::IdentityChanged));
    reads(&fixture, 12);
    assert_eq!(
        cache
            .draft(original.scope(), "actor-unsent")
            .unwrap()
            .as_deref(),
        Some("original42 actor draft")
    );
}
#[test]
fn git_actor_association_provider_true_cancel_before_final_principal_launches_no_extra_child() {
    let fixture = Fixture::new();
    let local = raw_local(&fixture, "sha1");
    let steps = protocol(local.sha(), response(local.sha()), 42);
    fixture.scripted_gh(&steps[..12]);
    let (cache, original) = source(&fixture);
    let cancel = CancellationToken::default();
    let failure = provider(&fixture)
        .local_commit_actor_read(
            target(&original, &local),
            &cache,
            &Default::default(),
            &cancel,
            || {
                cancel.cancel();
                Ok(())
            },
        )
        .unwrap_err();
    assert!(matches!(failure.kind, GithubErrorKind::Transport));
    reads(&fixture, 12);
}
#[test]
fn git_actor_association_provider_bad_selection_and_whole_cap_refuse_before_private_commit() {
    for mode in 0..2 {
        let fixture = Fixture::new();
        let local = raw_local(&fixture, "sha1");
        let mut steps = initial();
        if mode == 1 {
            steps.extend(principal(42, "octocat"));
            steps.push(get("repos/example/original", repo()));
        }
        fixture.scripted_gh(&steps);
        let (cache, original) = source(&fixture);
        let mut input = target(&original, &local);
        let mut limits = LocalCommitActorReadLimits::default();
        if mode == 0 {
            input.selected_index = usize::MAX
        } else {
            limits.max_response_bytes = 100000
        };
        assert!(provider(&fixture)
            .local_commit_actor_read(input, &cache, &limits, &Default::default(), || Ok(()))
            .is_err());
        reads(&fixture, if mode == 0 { 7 } else { 10 });
    }
}
