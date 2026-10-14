//! SOURCE-ONLY future child of provider_contracts::repository_catalogue.
//! Three genuine transport/SQLite contracts; no raw opaque source constructors.
//! Root owns parent registration, production changes and actual Cargo gates.
use super::*;
use ghtui::{
    repository_catalogue_save_v1::{load_catalogue_intent_source, save_catalogue_intent},
    storage::{Cache, Scope},
};
use rusqlite::{params, Connection, OpenFlags};
use std::process::Command;

fn renamed_source_steps(final_uid: u64, final_login: &str) -> Vec<GhStep> {
    let mut steps = org_steps(
        vec![json!([repository(101, "private-original")])],
        2,
        final_uid,
    );
    steps.last_mut().unwrap().stdout =
        serde_json::to_vec(&json!({"id":final_uid,"login":final_login})).unwrap();
    steps
}
fn named_principal(uid: u64, login: &str) -> Vec<GhStep> {
    vec![version(), get("user", json!({"id":uid,"login":login}))]
}
fn actual_git(fixture: &Fixture, args: &[&str]) -> Vec<u8> {
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
        "Actual fixture Git command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}
fn git_state(fixture: &Fixture) -> [Vec<u8>; 4] {
    [
        actual_git(fixture, &["rev-parse", "HEAD"]),
        actual_git(fixture, &["show-ref", "--head"]),
        actual_git(fixture, &["status", "--porcelain=v1", "-z"]),
        fs::read(fixture.repo().join(".git/index")).unwrap(),
    ]
}

#[test]
fn repo_catalog_provider_identity_rename_final_same_uid_preserves_original_and_uses_actual_final_label(
) {
    let fixture = Fixture::new();
    let steps = renamed_source_steps(42, "renamed-login");
    fixture.scripted_gh(&steps);
    let original = current();
    let loaded = provider(&fixture)
        .repository_catalogue_read(
            Some(&original),
            &org(),
            &limits(),
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(loaded.identity().host, "github.invalid");
    assert_eq!(loaded.identity().account_id, Some(42));
    assert_eq!(loaded.identity().login, "renamed-login");
    assert_eq!(loaded.context().identity(), loaded.identity());
    assert_eq!(original.login, "octocat");
    assert_eq!(original.account_id, Some(42));
    assert_eq!(loaded.source(), &org());
    assert_eq!(loaded.owner().unwrap().id, 71);
    assert_eq!(
        loaded.owner().unwrap().node_id.as_deref(),
        Some("OWNER_actual_71")
    );
    assert_eq!(loaded.repositories()[0].id, 101);
    assert_eq!(
        loaded.repositories()[0].node_id.as_deref(),
        Some("REPO_actual_101")
    );
    assert_eq!(
        loaded.repositories()[0].full_name,
        "example/private-original"
    );
    assert!(loaded.coverage().membership_complete);
    assert!(loaded.coverage().enumeration_exhausted);
    assert!(!loaded.coverage().complete_empty);
    assert_eq!(loaded.coverage().fetched_pages, 1);
    assert_eq!(loaded.coverage().raw_rows, 1);
    assert_eq!(loaded.accounting().requests(), 7);
    assert_eq!(
        loaded.accounting().successful_stdout_bytes(),
        steps.iter().map(|step| step.stdout.len()).sum::<usize>()
    );
    assert_eq!(loaded.accounting().successful_diagnostic_bytes(), 0);
    assert_eq!(loaded.accounting().failed_reserved_bytes(), 0);
    reads(&fixture, 7);
}

#[test]
fn repo_catalog_provider_identity_rename_same_login_new_uid_refuses_without_private_result_or_probe(
) {
    let fixture = Fixture::new();
    fixture.scripted_gh(&renamed_source_steps(43, "octocat"));
    let result = provider(&fixture).repository_catalogue_read(
        Some(&current()),
        &org(),
        &limits(),
        &CancellationToken::default(),
    );
    let Err(error) = result else {
        panic!("Reused login must not admit private rows under a different numeric account")
    };
    assert!(matches!(error.kind, GithubErrorKind::IdentityChanged));
    assert!(!error.message.contains("private-original"));
    assert!(!error.message.contains("REPO_actual_101"));
    assert!(!error.message.contains("Explicitly selected Rust repo"));
    reads(&fixture, 7); // No fallback, extra authentication or broadened query.
}

#[test]
fn repo_catalog_provider_identity_rename_guarded_source_and_save_stay_numeric_original_sql_and_git()
{
    let fixture = Fixture::new();
    actual_git(&fixture, &["init", "--quiet", "-b", "main"]);
    fs::write(
        fixture.repo().join("tracked.txt"),
        b"original Git baseline\n",
    )
    .unwrap();
    actual_git(&fixture, &["add", "tracked.txt"]);
    actual_git(
        &fixture,
        &[
            "-c",
            "user.name=Catalogue Fixture",
            "-c",
            "user.email=catalogue@example.invalid",
            "commit",
            "--quiet",
            "-m",
            "Original immutable catalogue fixture",
        ],
    );
    fs::write(
        fixture.repo().join("tracked.txt"),
        b"retained dirty Git bytes\n",
    )
    .unwrap();
    let original_git = git_state(&fixture);

    let mut steps = renamed_source_steps(42, "renamed-login");
    steps.extend(named_principal(42, "renamed-login"));
    steps.extend(named_principal(42, "renamed-again"));
    fixture.scripted_gh(&steps);
    let database = fixture.root.join("rename-original.sqlite3");
    let mut cache = Cache::open(&database).unwrap();
    let scope42 = Scope::github("github.invalid", Some(42)).unwrap();
    let scope43 = Scope::github("github.invalid", Some(43)).unwrap();
    cache
        .put_draft(&scope42, "original-review", "PRIVATE_ORIGINAL42_DRAFT")
        .unwrap();
    let provider = provider(&fixture);
    let original_identity = current();
    let source = load_catalogue_intent_source(
        &provider,
        &mut cache,
        Some(&original_identity),
        &org(),
        &limits(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(source.source().identity().login, "renamed-login");
    assert_eq!(source.scope(), &scope42);
    let original_generation = cache
        .sync_reservation(source.scope(), source.sync_source())
        .unwrap();
    assert!(original_generation.is_some());
    let mut candidate = source.intent().clone();
    candidate.set(source.source(), "focused", &[101]).unwrap();
    candidate.pin(source.source(), 101, true).unwrap();
    let saved = save_catalogue_intent(
        &provider,
        &mut cache,
        &source,
        candidate,
        Duration::from_secs(5),
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(Arc::ptr_eq(source.source(), saved.source()));
    assert_eq!(
        saved.source().identity().login,
        "renamed-login",
        "Save cannot rewrite the immutable original source or assert fresh permission"
    );
    assert_eq!(
        cache
            .sync_reservation(saved.scope(), saved.sync_source())
            .unwrap(),
        original_generation
    );
    cache
        .validate_repository_catalogue_intent_source(&saved)
        .unwrap();
    let value = cache
        .saved_view_bounded(&scope42, saved.saved_view_key(), 128 * 1024)
        .unwrap()
        .unwrap();
    assert_eq!(value["scope"]["host"], "github.invalid");
    assert_eq!(value["scope"]["login"], "user-id:42");
    assert_eq!(value["source"]["owner_id"], 71);
    assert_eq!(value["source"]["owner_node_id"], "OWNER_actual_71");
    assert_eq!(value["sets"]["focused"]["101"]["id"], 101);
    assert_eq!(
        value["sets"]["focused"]["101"]["node_id"],
        "REPO_actual_101"
    );
    assert_eq!(
        value["sets"]["focused"]["101"]["last_name"],
        "example/private-original"
    );
    assert_eq!(value["pins"]["101"]["id"], 101);
    assert_eq!(saved.intent().to_value().unwrap(), value);
    assert_eq!(
        cache.draft(&scope42, "original-review").unwrap().as_deref(),
        Some("PRIVATE_ORIGINAL42_DRAFT")
    );
    assert!(cache.saved_views(&scope43).unwrap().is_empty());
    assert!(cache.list(&scope43, None).unwrap().is_empty());
    assert!(cache.draft(&scope43, "original-review").unwrap().is_none());
    let legacy_login_scope = Scope::new("github.invalid", "octocat").unwrap();
    assert!(cache.saved_views(&legacy_login_scope).unwrap().is_empty());
    assert!(cache
        .draft(&legacy_login_scope, "original-review")
        .unwrap()
        .is_none());
    let raw = Connection::open_with_flags(&database, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let actual_payload: String = raw
        .query_row(
            "SELECT payload FROM saved_views WHERE host=?1 AND login=?2 AND name=?3",
            params!["github.invalid", "user-id:42", saved.saved_view_key()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&actual_payload).unwrap(),
        value
    );
    let actual_draft: String = raw
        .query_row(
            "SELECT body FROM drafts WHERE host=?1 AND login=?2 AND resource=?3",
            params!["github.invalid", "user-id:42", "original-review"],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(actual_draft.as_bytes(), b"PRIVATE_ORIGINAL42_DRAFT");
    let foreign_scope_rows: u64 = raw
        .query_row(
            "SELECT (SELECT count(*) FROM saved_views WHERE login IN ('user-id:43','octocat','renamed-login','renamed-again')) + (SELECT count(*) FROM drafts WHERE login IN ('user-id:43','octocat','renamed-login','renamed-again'))",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(foreign_scope_rows, 0, "No numeric43 or login-keyed copies");
    assert_eq!(git_state(&fixture), original_git);
    assert_eq!(
        fs::read(fixture.repo().join("tracked.txt")).unwrap(),
        b"retained dirty Git bytes\n"
    );
    reads(&fixture, 11); // Source7 + genuine version/user pre/post Save4 only.
}
