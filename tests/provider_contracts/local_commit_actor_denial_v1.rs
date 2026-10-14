use super::*;

#[test]
fn git_actor_association_provider_actual_repository_403_closes_same_principal_without_commit() {
    let fixture = Fixture::new();
    let local = raw_local(&fixture, "sha1");
    let mut steps = initial();
    steps.extend(principal(42, "octocat"));
    let mut denied = get("repos/example/original", Value::Null);
    denied.stdout.clear();
    denied.status = 1;
    denied.stderr = b"HTTP 403: resource inaccessible by integration\n".to_vec();
    steps.push(denied);
    steps.extend(principal(42, "octocat"));
    fixture.scripted_gh(&steps);
    let (cache, original) = source(&fixture);
    assert!(matches!(
        provider(&fixture)
            .local_commit_actor_read(
                target(&original, &local),
                &cache,
                &Default::default(),
                &Default::default(),
                || Ok(())
            )
            .unwrap_err()
            .kind,
        GithubErrorKind::Permission
    ));
    reads(&fixture, 12);
    assert!(!journal(&fixture).iter().any(|row| row["args"][1]
        .as_str()
        .is_some_and(|arg| arg.contains("/commits/"))));
    assert_eq!(
        cache
            .draft(original.scope(), "actor-unsent")
            .unwrap()
            .as_deref(),
        Some("original42 actor draft")
    );
}
