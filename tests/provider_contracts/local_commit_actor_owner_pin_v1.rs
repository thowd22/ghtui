use super::*;

#[test]
fn git_actor_association_provider_row_owner_node_cannot_be_weakened_by_nullable_owner_endpoint() {
    let fixture = Fixture::new();
    let local = raw_local(&fixture, "sha1");
    let mut steps = initial();
    for index in [2, 4] {
        let mut raw = owner();
        raw["node_id"] = Value::Null;
        steps[index] = get("orgs/example", raw);
    }
    steps.extend(principal(42, "octocat"));
    let mut changed = repo();
    changed["owner"]["node_id"] = json!("OWNER_replaced_but_same_database_id");
    steps.push(get("repos/example/original", changed));
    steps.extend(principal(42, "octocat"));
    fixture.scripted_gh(&steps);
    let (cache, original) = source(&fixture);
    assert!(original.source().owner().unwrap().node_id.is_none());
    assert_eq!(
        original.source().repositories()[0]
            .owner
            .as_ref()
            .unwrap()
            .node_id
            .as_deref(),
        Some("OWNER_actor_71")
    );
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
    reads(&fixture, 12);
}
