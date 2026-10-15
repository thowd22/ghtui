//! Actual original-source/transport contracts for the shared core extraction.
//! No personal source constructor exists in this prerequisite.
use super::*;

struct KeptGitV13 {
    head: String,
    index: Vec<u8>,
    work: Vec<u8>,
    config: Vec<u8>,
}
fn git_fixture_v13(fixture: &Fixture) -> KeptGitV13 {
    git(fixture, &["init", "-q"]);
    git(fixture, &["config", "user.name", "Original access core"]);
    git(fixture, &["config", "user.email", "core@example.invalid"]);
    fs::write(fixture.repo().join("kept.txt"), b"original\n").unwrap();
    git(fixture, &["add", "--", "kept.txt"]);
    git(
        fixture,
        &["commit", "-qm", "Actual original access fixture"],
    );
    fs::write(fixture.repo().join("kept.txt"), b"dirty unchanged\n").unwrap();
    KeptGitV13 {
        head: git(fixture, &["rev-parse", "HEAD"]),
        index: fs::read(fixture.repo().join(".git/index")).unwrap(),
        work: fs::read(fixture.repo().join("kept.txt")).unwrap(),
        config: fs::read(fixture.repo().join(".git/config")).unwrap(),
    }
}
fn retained_v13(
    fixture: &Fixture,
    source: &CatalogueIntentSource,
    kept: &KeptGitV13,
    steps: &[GhStep],
) {
    let cache = Cache::open(&database(fixture)).unwrap();
    cache
        .validate_repository_catalogue_intent_source(source)
        .unwrap();
    assert_eq!(
        cache.draft(source.scope(), "original").unwrap().as_deref(),
        Some("PRIVATE_SOURCE_DRAFT")
    );
    assert!(cache
        .list(&Scope::github("github.invalid", Some(43)).unwrap(), None)
        .unwrap()
        .is_empty());
    assert_eq!(git(fixture, &["rev-parse", "HEAD"]), kept.head);
    assert_eq!(
        fs::read(fixture.repo().join(".git/index")).unwrap(),
        kept.index
    );
    assert_eq!(
        fs::read(fixture.repo().join("kept.txt")).unwrap(),
        kept.work
    );
    assert_eq!(
        fs::read(fixture.repo().join(".git/config")).unwrap(),
        kept.config
    );
    exact(fixture, steps);
}

#[test]
fn repo_catalog_measured_personal_core_v13_actual_catalogue_wrapper_keeps_started_budget_provenance_and_one_prepaid_cursor(
) {
    let mut steps = source_steps(1);
    steps.extend(principal());
    for _ in 0..3 {
        steps.push(get(&format!("repos/{}", name(0)), repository(0)));
    }
    steps.extend(principal());
    steps.push(get("repos/example/repo0000/issues/9", json!({"id":9})));
    assert_eq!(steps.len(), 15);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let source = original(&fixture);
    let kept = git_fixture_v13(&fixture);
    let provider = provider(&fixture);
    let cancel = CancellationToken::default();
    let mut session = provider
        .begin_repository_catalogue_access(source.source(), &limits(), &cancel)
        .unwrap();
    let mut allowance = session
        .reserve_postwrite_capture_v2(&[128], 2, &cancel)
        .unwrap();
    let window = session
        .reserve_measured_external_reads(1, 1024, 4096, 2, &cancel)
        .unwrap();
    assert_eq!(
        session.core.budget.accounting.requests(),
        2,
        "Actual opening version/user remain charged, not reset by core move"
    );
    session.verify(&reference(0), &cancel).unwrap();
    let measured = provider.clone().with_measured_read_window(window).unwrap();
    allowance.validate_prewrite(&measured, &cancel).unwrap();
    measured.repository(&name(0), &cancel).unwrap();
    session.verify(&reference(0), &cancel).unwrap();
    let closed = session
        .close_with_guard(&cancel, || {
            Cache::open(&database(&fixture))
                .unwrap()
                .validate_repository_catalogue_intent_source(&source)
                .map_err(|_| {
                    error(
                        GithubErrorKind::IdentityChanged,
                        "Actual original source changed",
                    )
                })
        })
        .unwrap();
    assert_eq!(closed.original_source(), source.source().source());
    assert_eq!(closed.original_owner(), source.source().owner());
    assert_eq!(closed.identity().account_id, Some(42));
    assert_eq!(closed.verified().len(), 1);
    assert!(closed.failures().is_empty());
    assert_eq!(closed.accounting().requests(), 6);
    let actual = closed.external_read_accounting().unwrap();
    assert!(actual.sealed());
    assert_eq!(actual.children_started(), 1);
    assert_eq!(actual.requests_claimed(), 1);
    assert_eq!(closed.external_requests_reserved(), 1);
    assert_eq!(
        closed.postwrite_capture_v2().unwrap().reserved_requests(),
        1
    );
    assert_eq!(
        closed.charged_bytes(),
        closed.accounting().charged_bytes() + actual.charged_bytes() + 128
    );
    assert!(
        measured.repository(&name(0), &cancel).is_err(),
        "Closed read pool cannot be reused even by same runner"
    );
    assert_eq!(
        allowance
            .api(
                &provider,
                "repos/example/repo0000/issues/9",
                "GET",
                None,
                None,
                &cancel
            )
            .unwrap()["id"],
        9
    );
    assert!(allowance
        .api(
            &provider,
            "repos/example/repo0000/issues/9",
            "GET",
            None,
            None,
            &cancel
        )
        .is_err());
    assert_eq!(allowance.consumed_requests(), 1);
    retained_v13(&fixture, &source, &kept, &steps);
}

#[test]
fn repo_catalog_measured_personal_core_v13_drop_aborts_exact_measured_and_prepaid_lineage_without_closing_or_new_child(
) {
    let mut steps = source_steps(1);
    steps.extend(principal());
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let source = original(&fixture);
    let kept = git_fixture_v13(&fixture);
    let provider = provider(&fixture);
    let cancel = CancellationToken::default();
    let mut session = provider
        .begin_repository_catalogue_access(source.source(), &limits(), &cancel)
        .unwrap();
    let mut allowance = session
        .reserve_postwrite_capture_v2(&[128], 2, &cancel)
        .unwrap();
    let window = session
        .reserve_measured_external_reads(1, 1024, 4096, 2, &cancel)
        .unwrap();
    let measured = provider.clone().with_measured_read_window(window).unwrap();
    let actual = measured.scoped_read_budget.as_ref().unwrap().clone();
    allowance.validate_prewrite(&measured, &cancel).unwrap();
    drop(session);
    assert!(actual.check_clock(&cancel).is_err());
    assert!(measured.repository(&name(0), &cancel).is_err());
    assert!(allowance.validate_prewrite(&measured, &cancel).is_err());
    assert!(allowance
        .api(
            &provider,
            "repos/example/repo0000/issues/9",
            "GET",
            None,
            None,
            &cancel
        )
        .is_err());
    assert_eq!(allowance.consumed_requests(), 0);
    retained_v13(&fixture, &source, &kept, &steps);
}

#[test]
fn repo_catalog_measured_personal_core_v13_original_sql_close_failure_never_activates_postwrite_or_sends_final_principal(
) {
    let mut steps = source_steps(1);
    steps.extend(principal());
    for _ in 0..3 {
        steps.push(get(&format!("repos/{}", name(0)), repository(0)));
    }
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let source = original(&fixture);
    let kept = git_fixture_v13(&fixture);
    let provider = provider(&fixture);
    let cancel = CancellationToken::default();
    let mut session = provider
        .begin_repository_catalogue_access(source.source(), &limits(), &cancel)
        .unwrap();
    let mut allowance = session
        .reserve_postwrite_capture_v2(&[128], 2, &cancel)
        .unwrap();
    let window = session
        .reserve_measured_external_reads(1, 1024, 4096, 2, &cancel)
        .unwrap();
    session.verify(&reference(0), &cancel).unwrap();
    let measured = provider.clone().with_measured_read_window(window).unwrap();
    measured.repository(&name(0), &cancel).unwrap();
    session.verify(&reference(0), &cancel).unwrap();
    let reached = AtomicBool::new(false);
    assert!(session
        .close_with_guard(&cancel, || {
            Cache::open(&database(&fixture))
                .unwrap()
                .validate_repository_catalogue_intent_source(&source)
                .unwrap();
            reached.store(true, Ordering::Release);
            Err(error(
                GithubErrorKind::IdentityChanged,
                "Actual original source guard refusal",
            ))
        })
        .is_err());
    assert!(reached.load(Ordering::Acquire));
    assert!(measured.repository(&name(0), &cancel).is_err());
    assert!(allowance
        .api(
            &provider,
            "repos/example/repo0000/issues/9",
            "GET",
            None,
            None,
            &cancel
        )
        .is_err());
    assert_eq!(allowance.consumed_requests(), 0);
    retained_v13(&fixture, &source, &kept, &steps);
}

#[test]
fn repo_catalog_measured_personal_core_v13_actual_owner_id_node_and_kind_changes_remain_refusals_not_personal_fallback(
) {
    for variant in 0..3 {
        let mut changed = repository(0);
        match variant {
            0 => changed["owner"]["id"] = json!(72),
            1 => changed["owner"]["node_id"] = json!("OWNER_OTHER"),
            _ => changed["owner"]["type"] = json!("User"),
        }
        let mut steps = source_steps(1);
        steps.extend(principal());
        steps.push(get(&format!("repos/{}", name(0)), changed));
        steps.extend(principal());
        let fixture = Fixture::new();
        fixture.scripted_gh(&steps);
        let source = original(&fixture);
        let kept = git_fixture_v13(&fixture);
        let provider = provider(&fixture);
        let cancel = CancellationToken::default();
        let mut session = provider
            .begin_repository_catalogue_access(source.source(), &limits(), &cancel)
            .unwrap();
        assert!(matches!(
            session.verify(&reference(0), &cancel).unwrap_err().kind,
            GithubErrorKind::IdentityChanged
        ));
        let closed = session.close(&cancel).unwrap();
        assert!(closed.verified().is_empty());
        assert_eq!(closed.failures().len(), 1);
        assert_eq!(closed.original_owner().unwrap().id, 71);
        assert_eq!(
            closed.original_source(),
            &RepositoryCatalogueSource::Organization("example".into())
        );
        retained_v13(&fixture, &source, &kept, &steps);
    }
}

#[test]
fn repo_catalog_measured_personal_core_v13_public_user_parser_context_still_refuses_private_repo_without_inventory_relabel(
) {
    let mut user = owner();
    user["type"] = json!("User");
    let mut public = repository(0);
    public["owner"] = user.clone();
    public["private"] = json!(false);
    let mut private = public.clone();
    private["private"] = json!(true);
    let mut steps = principal();
    steps.push(get("users/example", user.clone()));
    steps.push(get(
        "users/example/repos?type=owner&sort=full_name&direction=asc&per_page=100&page=1",
        json!([public]),
    ));
    steps.push(get("users/example", user));
    steps.extend(principal());
    steps.extend(principal());
    steps.push(get(&format!("repos/{}", name(0)), private));
    steps.extend(principal());
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let provider = provider(&fixture);
    let cancel = CancellationToken::default();
    let mut cache = Cache::open(&database(&fixture)).unwrap();
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    cache
        .put_draft(&scope, "original", "PRIVATE_SOURCE_DRAFT")
        .unwrap();
    let source = load_catalogue_intent_source(
        &provider,
        &mut cache,
        None,
        &RepositoryCatalogueSource::PublicUser("example".into()),
        &RepositoryCatalogueLimits::default(),
        &cancel,
    )
    .unwrap();
    let kept = git_fixture_v13(&fixture);
    let mut session = provider
        .begin_repository_catalogue_access(source.source(), &limits(), &cancel)
        .unwrap();
    assert!(matches!(
        session.verify(&reference(0), &cancel).unwrap_err().kind,
        GithubErrorKind::Json
    ));
    let closed = session.close(&cancel).unwrap();
    assert!(closed.verified().is_empty());
    assert_eq!(closed.failures().len(), 1);
    assert_eq!(
        closed.original_source(),
        &RepositoryCatalogueSource::PublicUser("example".into())
    );
    retained_v13(&fixture, &source, &kept, &steps);
}

#[test]
fn repo_catalog_measured_personal_core_v13_original_budget_cancellation_and_token_do_not_renew_after_core_transfer(
) {
    let mut steps = source_steps(1);
    steps.extend(principal());
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let source = original(&fixture);
    let kept = git_fixture_v13(&fixture);
    let provider = provider(&fixture);
    let cancel = CancellationToken::default();
    let mut small = limits();
    small.transport.max_response_bytes = 32 * 1024;
    let mut session = provider
        .begin_repository_catalogue_access(source.source(), &small, &cancel)
        .unwrap();
    assert_eq!(session.core.budget.accounting.requests(), 2);
    assert!(session
        .reserve_postwrite_capture_v2(&[256 * 1024], 2, &cancel)
        .is_err());
    assert!(session
        .reserve_measured_external_reads(1, 1024, 4096, 2, &cancel)
        .is_err());
    assert!(session.core.measured_external.is_none());
    assert!(session.core.postwrite_reservation_v2.is_none());
    assert!(session
        .reserve_postwrite_capture_v2(&[128], 0, &CancellationToken::default())
        .is_err());
    cancel.cancel();
    assert!(session
        .verify(&reference(0), &CancellationToken::default())
        .is_err());
    assert!(session.close(&CancellationToken::default()).is_err());
    retained_v13(&fixture, &source, &kept, &steps);
    let expired = provider
        .clone()
        .with_scoped_read_window(Instant::now() - Duration::from_millis(1), 1024)
        .unwrap();
    assert!(expired
        .begin_repository_catalogue_access(
            source.source(),
            &limits(),
            &CancellationToken::default()
        )
        .is_err());
    exact(&fixture, &steps);
}
