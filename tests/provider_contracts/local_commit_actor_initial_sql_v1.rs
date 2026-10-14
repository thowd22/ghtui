//! Fresh remote identity/resource facts precede private original SQL validation.
use super::*;

#[test]
fn git_actor_association_provider_stale_sql_does_not_preempt_actual_pre_uid_switch() {
    let fixture = Fixture::new();
    let local = raw_local(&fixture, "sha1");
    let mut steps = initial();
    steps.extend(principal(43, "octocat"));
    fixture.scripted_gh(&steps);
    let (mut cache, original) = source(&fixture);
    cache.clear_scope(original.scope(), false).unwrap();
    assert!(cache
        .validate_repository_catalogue_intent_source(&original)
        .is_err());
    let head = git(&fixture.repo(), &["rev-parse", "HEAD"], None);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
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
    reads(&fixture, 9);
    assert!(!journal(&fixture).iter().skip(7).any(|row| row["args"][1]
        .as_str()
        .is_some_and(|arg| arg.starts_with("repos/"))));
    unchanged(&fixture, &cache, &head, &index);
}

#[test]
fn git_actor_association_provider_foreign_sql_issuer_fails_only_after_genuine_current_repo() {
    let fixture = Fixture::new();
    let local = raw_local(&fixture, "sha1");
    let mut steps = initial();
    steps.extend(principal(42, "octocat"));
    steps.push(get("repos/example/original", repo()));
    fixture.scripted_gh(&steps);
    let (original_cache, original) = source(&fixture);
    let foreign = Cache::open(&fixture.root.join("foreign-actor.sqlite3")).unwrap();
    let failure = provider(&fixture)
        .local_commit_actor_read(
            target(&original, &local),
            &foreign,
            &Default::default(),
            &Default::default(),
            || Ok(()),
        )
        .unwrap_err();
    assert!(matches!(failure.kind, GithubErrorKind::IdentityChanged));
    reads(&fixture, 10);
    assert!(!journal(&fixture).iter().any(|row| row["args"][1]
        .as_str()
        .is_some_and(|arg| arg.contains("/commits/"))));
    assert_eq!(
        original_cache
            .draft(original.scope(), "actor-unsent")
            .unwrap()
            .as_deref(),
        Some("original42 actor draft")
    );
    assert!(foreign.list(original.scope(), None).unwrap().is_empty());
    assert!(foreign.saved_views(original.scope()).unwrap().is_empty());
}

#[test]
fn git_actor_association_provider_replaced_original_local_directory_stops_before_any_actor_gh() {
    let fixture = Fixture::new();
    let local = raw_local(&fixture, "sha1");
    let steps = initial();
    fixture.scripted_gh(&steps);
    let (cache, original) = source(&fixture);
    let saved_git = fixture.repo().join("original-git-retained");
    fs::rename(fixture.repo().join(".git"), &saved_git).unwrap();
    let config = fs::read(saved_git.join("config")).unwrap();
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
    reads(&fixture, 7);
    assert_eq!(fs::read(saved_git.join("config")).unwrap(), config);
    assert_eq!(
        cache
            .draft(original.scope(), "actor-unsent")
            .unwrap()
            .as_deref(),
        Some("original42 actor draft")
    );
    assert!(cache
        .list(&Scope::github("github.invalid", Some(43)).unwrap(), None)
        .unwrap()
        .is_empty());
    fs::rename(&saved_git, fixture.repo().join(".git")).unwrap();
    local.recheck_original_source(&Default::default()).unwrap();
    assert_eq!(
        String::from_utf8(git(&fixture.repo(), &["rev-parse", "HEAD"], None))
            .unwrap()
            .trim(),
        local.sha()
    );
}
