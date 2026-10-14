use super::*;

#[test]
fn git_actor_association_provider_original_account_pre_switch_stops_before_repository() {
    let fixture = Fixture::new();
    let local = raw_local(&fixture, "sha1");
    let mut steps = initial();
    steps.extend(principal(43, "octocat"));
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
        GithubErrorKind::IdentityChanged
    ));
    reads(&fixture, 9);
}
#[test]
fn git_actor_association_provider_measured_failed_capture_preserves_closing_budget_and_no_retry() {
    let fixture = Fixture::new();
    let local = raw_local(&fixture, "sha1");
    let mut steps = protocol(local.sha(), response(local.sha()), 42);
    steps[10].status = 1;
    steps[10].stdout.clear();
    steps[10].stderr = b"ordinary transport error\n".to_vec();
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
        GithubErrorKind::Transport
    ));
    reads(&fixture, 14);
    assert_eq!(
        journal(&fixture)
            .iter()
            .filter(|row| row["args"][1]
                .as_str()
                .is_some_and(|arg| arg.contains("/commits/")))
            .count(),
        1,
        "Failed read is never automatically resent"
    );
}
#[test]
fn git_actor_association_provider_original_deadline_stops_after_delayed_commit_not_new_clock() {
    let fixture = Fixture::new();
    let local = raw_local(&fixture, "sha1");
    let mut steps = protocol(local.sha(), response(local.sha()), 42);
    steps[10].delay_ms = 2000;
    fixture.scripted_gh(&steps[..11]);
    let (cache, original) = source(&fixture);
    let limits = LocalCommitActorReadLimits {
        max_duration: Duration::from_millis(400),
        ..Default::default()
    };
    let started = Instant::now();
    assert!(matches!(
        provider(&fixture)
            .local_commit_actor_read(
                target(&original, &local),
                &cache,
                &limits,
                &Default::default(),
                || Ok(())
            )
            .unwrap_err()
            .kind,
        GithubErrorKind::Transport
    ));
    assert!(started.elapsed() < Duration::from_secs(2));
    reads(&fixture, 11);
}
#[test]
fn git_actor_association_provider_actual_opaque_actor_repository_and_owner_collisions_refuse() {
    for mutation in 0..3 {
        let fixture = Fixture::new();
        let local = raw_local(&fixture, "sha1");
        let mut value = response(local.sha());
        match mutation {
            0 => value["author"]["node_id"] = json!("REPO_actor_101"),
            1 => value["author"]["node_id"] = json!("OWNER_actor_71"),
            _ => value["committer"]["node_id"] = value["author"]["node_id"].clone(),
        };
        fixture.scripted_gh(&protocol(local.sha(), value, 42));
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
            GithubErrorKind::IdentityChanged
        ));
        reads(&fixture, 14);
    }
}
