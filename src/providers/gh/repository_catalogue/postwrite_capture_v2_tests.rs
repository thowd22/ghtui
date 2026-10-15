//! Genuine original-source sessions, exact children and conservative captures.
use super::super::super::postwrite_capture_v2::PostWriteCaptureAllowanceV2;
use super::*;

struct ClosedFixture {
    fixture: Fixture,
    provider: GithubProvider,
    cancellation: CancellationToken,
    allowance: PostWriteCaptureAllowanceV2,
    closed: Option<ClosedCatalogueAccess>,
    original: Arc<CatalogueIntentSource>,
    steps: Vec<GhStep>,
}
fn post(value: Value) -> GhStep {
    GhStep::read(
        &[
            "api",
            "repos/example/repo0000/pulls/7/reviews",
            "--hostname",
            "github.invalid",
            "--method",
            "POST",
        ],
        &value.to_string(),
    )
}
fn prepare(
    caps: &[usize],
    queued: Option<(Duration, usize)>,
    close_failure: bool,
    post_steps: Vec<GhStep>,
) -> ClosedFixture {
    let mut steps = source_steps(1);
    steps.extend(principal());
    steps.push(get(&format!("repos/{}", name(0)), repository(0)));
    steps.push(get(&format!("repos/{}", name(0)), repository(0)));
    steps.push(get(&format!("repos/{}", name(0)), repository(0)));
    if !close_failure {
        steps.extend(principal());
    }
    steps.extend(post_steps);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let original = original(&fixture);
    let provider = provider(&fixture);
    let issuer = if let Some((duration, cap)) = queued {
        provider
            .clone()
            .with_scoped_read_window(Instant::now() + duration, cap)
            .unwrap()
    } else {
        provider.clone()
    };
    let cancellation = CancellationToken::default();
    let mut session = issuer
        .begin_repository_catalogue_access(original.source(), &limits(), &cancellation)
        .unwrap();
    let allowance = session
        .reserve_postwrite_capture_v2(caps, 2, &cancellation)
        .unwrap();
    let window = session
        .reserve_measured_external_reads(1, 1024, 4096, 2, &cancellation)
        .unwrap();
    assert!(window.deadline() <= allowance.deadline());
    session.verify(&reference(0), &cancellation).unwrap();
    let measured = issuer.clone().with_measured_read_window(window).unwrap();
    allowance
        .validate_prewrite(&measured, &cancellation)
        .unwrap();
    measured.repository(&name(0), &cancellation).unwrap();
    session.verify(&reference(0), &cancellation).unwrap();
    let closed = session.close_with_guard(&cancellation, || {
        Cache::open(&database(&fixture))
            .unwrap()
            .validate_repository_catalogue_intent_source(&original)
            .map_err(|_| {
                error(
                    GithubErrorKind::IdentityChanged,
                    "Actual original source SQL failed",
                )
            })?;
        if close_failure {
            Err(error(
                GithubErrorKind::IdentityChanged,
                "Actual original closing guard refused",
            ))
        } else {
            Ok(())
        }
    });
    if close_failure {
        assert!(closed.is_err());
    } else {
        assert!(closed.is_ok());
    }
    ClosedFixture {
        fixture,
        provider,
        cancellation,
        allowance,
        closed: closed.ok(),
        original,
        steps,
    }
}
fn retained(s: &ClosedFixture) {
    let cache = Cache::open(&database(&s.fixture)).unwrap();
    assert_eq!(
        cache
            .draft(s.original.scope(), "original")
            .unwrap()
            .as_deref(),
        Some("PRIVATE_SOURCE_DRAFT")
    );
    assert!(cache
        .list(&Scope::github("github.invalid", Some(43)).unwrap(), None)
        .unwrap()
        .is_empty());
    exact(&s.fixture, &s.steps);
}

#[test]
fn pending_reviews_v2_capture_reservation_too_small_original_budget_cannot_allocate_or_post() {
    let mut steps = source_steps(1);
    steps.extend(principal());
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let original = original(&fixture);
    let provider = provider(&fixture);
    let cancel = CancellationToken::default();
    let mut bounded = limits();
    bounded.transport.max_response_bytes = 512 * 1024;
    let mut session = provider
        .begin_repository_catalogue_access(original.source(), &bounded, &cancel)
        .unwrap();
    assert!(session
        .reserve_postwrite_capture_v2(&[256 * 1024, 256 * 1024], 2, &cancel)
        .is_err());
    exact(&fixture, &steps);
    assert!(session.postwrite_reservation_v2.is_none());
}

#[test]
fn pending_reviews_v2_capture_fixed_slots_are_separate_from_measured_reads_and_no_extra_child() {
    let mut s = prepare(
        &[128, 256],
        None,
        false,
        vec![
            post(json!({"id":810})),
            get(
                "repos/example/repo0000/pulls/7/reviews/810",
                json!({"id":810,"state":"PENDING"}),
            ),
        ],
    );
    let closed = s.closed.as_ref().unwrap();
    let measured = closed.external_read_accounting().unwrap();
    assert_eq!(measured.children_started(), 1);
    assert!(measured.sealed());
    let conservative = closed.postwrite_capture_v2().unwrap();
    assert_eq!(conservative.reserved_requests(), 2);
    assert_eq!(conservative.reserved_bytes(), 384);
    assert_eq!(closed.external_requests_reserved(), 1);
    assert_eq!(
        closed.charged_bytes(),
        closed.accounting().charged_bytes() + measured.charged_bytes() + 384
    );
    let first = s
        .allowance
        .api(
            &s.provider,
            "repos/example/repo0000/pulls/7/reviews",
            "POST",
            None,
            Some(s.provider.resource("example/repo0000", 7)),
            &s.cancellation,
        )
        .unwrap();
    assert_eq!(first["id"], 810);
    s.allowance
        .api(
            &s.provider,
            "repos/example/repo0000/pulls/7/reviews/810",
            "GET",
            None,
            None,
            &s.cancellation,
        )
        .unwrap();
    assert_eq!(s.allowance.consumed_requests(), 2);
    assert!(s
        .allowance
        .api(
            &s.provider,
            "repos/example/repo0000/pulls/7/reviews/810",
            "GET",
            None,
            None,
            &s.cancellation
        )
        .is_err());
    assert_eq!(s.allowance.consumed_requests(), 2);
    retained(&s);
}

#[test]
fn pending_reviews_v2_capture_failed_source_close_never_activates_postwrite_allowance() {
    let mut s = prepare(&[128], None, true, vec![]);
    assert!(s.closed.is_none());
    assert!(s
        .allowance
        .api(
            &s.provider,
            "repos/example/repo0000/pulls/7/reviews",
            "POST",
            None,
            Some(s.provider.resource("example/repo0000", 7)),
            &s.cancellation
        )
        .is_err());
    assert_eq!(s.allowance.consumed_requests(), 0);
    retained(&s);
}

#[test]
fn pending_reviews_v2_capture_sibling_token_or_original_cancellation_cannot_publish() {
    let mut s = prepare(&[128], None, false, vec![]);
    let sibling = CancellationToken::default();
    assert!(!sibling.same_instance(&s.cancellation));
    assert!(s
        .allowance
        .api(
            &s.provider,
            "repos/example/repo0000/pulls/7/reviews",
            "POST",
            None,
            Some(s.provider.resource("example/repo0000", 7)),
            &sibling
        )
        .is_err());
    assert_eq!(s.allowance.consumed_requests(), 0);
    s.cancellation.cancel();
    assert!(s
        .allowance
        .api(
            &s.provider,
            "repos/example/repo0000/pulls/7/reviews",
            "POST",
            None,
            Some(s.provider.resource("example/repo0000", 7)),
            &s.cancellation
        )
        .is_err());
    assert_eq!(s.allowance.consumed_requests(), 0);
    retained(&s);
}

#[test]
fn pending_reviews_v2_capture_less_restricted_same_runner_cannot_renew_original_queued_deadline() {
    let mut s = prepare(&[128], Some((Duration::from_secs(2), 1024)), false, vec![]);
    // Deliberately wait for THIS captured boundary, never a readiness sleep or
    // a renewed timeout on the less-restricted same-runner clone.
    std::thread::sleep(
        s.allowance
            .deadline()
            .saturating_duration_since(Instant::now()),
    );
    assert!(Instant::now() >= s.allowance.deadline());
    assert!(s
        .allowance
        .api(
            &s.provider,
            "repos/example/repo0000/pulls/7/reviews",
            "POST",
            None,
            Some(s.provider.resource("example/repo0000", 7)),
            &s.cancellation
        )
        .is_err());
    assert_eq!(s.allowance.consumed_requests(), 0);
    retained(&s);
}

#[test]
fn pending_reviews_v2_capture_less_restricted_same_runner_cannot_widen_original_child_capture() {
    let mut s = prepare(
        &[4096],
        Some((Duration::from_secs(10), 1024)),
        false,
        vec![post(json!({"actual_oversized_response":"X".repeat(2048)}))],
    );
    assert_eq!(s.allowance.reserved_bytes(), 1024);
    assert_eq!(
        s.closed
            .as_ref()
            .unwrap()
            .postwrite_capture_v2()
            .unwrap()
            .reserved_bytes(),
        1024
    );
    assert!(s
        .allowance
        .api(
            &s.provider,
            "repos/example/repo0000/pulls/7/reviews",
            "POST",
            None,
            Some(s.provider.resource("example/repo0000", 7)),
            &s.cancellation
        )
        .is_err());
    assert_eq!(s.allowance.consumed_requests(), 1);
    retained(&s);
}

#[test]
fn pending_reviews_v2_capture_measured_prewrite_pool_itself_retains_original_cap_after_clone_widening(
) {
    let mut steps = source_steps(1);
    steps.extend(principal());
    steps.push(get(&format!("repos/{}", name(0)), repository(0)));
    let mut oversized = repository(0);
    oversized["description"] = json!("actual oversized measured response".repeat(100));
    steps.push(get(&format!("repos/{}", name(0)), oversized));
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let original = original(&fixture);
    let provider = provider(&fixture);
    let issuer = provider
        .clone()
        .with_scoped_read_window(Instant::now() + Duration::from_secs(10), 1024)
        .unwrap();
    let cancel = CancellationToken::default();
    let mut session = issuer
        .begin_repository_catalogue_access(original.source(), &limits(), &cancel)
        .unwrap();
    let mut allowance = session
        .reserve_postwrite_capture_v2(&[128], 2, &cancel)
        .unwrap();
    let window = session
        .reserve_measured_external_reads(1, 4096, 8192, 2, &cancel)
        .unwrap();
    assert_eq!(
        window.max_output_bytes(),
        1024,
        "the actual immutable budget, not just clone spec, retains original cap"
    );
    session.verify(&reference(0), &cancel).unwrap();
    let measured = issuer.clone().with_measured_read_window(window).unwrap();
    let widened = measured
        .clone()
        .with_scoped_read_window(Instant::now() + Duration::from_secs(60), 4096)
        .unwrap();
    let budget = measured.scoped_read_budget.as_ref().unwrap();
    assert!(Arc::ptr_eq(
        budget,
        widened.scoped_read_budget.as_ref().unwrap()
    ));
    allowance.validate_prewrite(&widened, &cancel).unwrap();
    assert!(widened.repository(&name(0), &cancel).is_err());
    let accounting = budget.accounting().unwrap();
    assert_eq!(accounting.children_started(), 1);
    assert_eq!(accounting.successful_children(), 0);
    assert_eq!(accounting.failed_reserved_bytes(), 1024);
    assert_eq!(accounting.in_flight_requests(), 0);
    // No private result was accepted. Dropping the ORIGINAL producer aborts its
    // still-pending lineage, never fabricating a successful source close.
    drop(session);
    assert!(allowance
        .api(
            &provider,
            "repos/example/repo0000/pulls/7/reviews",
            "POST",
            None,
            Some(provider.resource("example/repo0000", 7)),
            &cancel
        )
        .is_err());
    assert_eq!(allowance.consumed_requests(), 0);
    assert_eq!(
        Cache::open(&database(&fixture))
            .unwrap()
            .draft(original.scope(), "original")
            .unwrap()
            .as_deref(),
        Some("PRIVATE_SOURCE_DRAFT")
    );
    exact(&fixture, &steps);
}
