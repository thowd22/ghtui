//! Private child of CatalogueBudget; actual child/SQL guards.
use super::*;
use crate::{
    repository_catalogue_read_limits_test_support_v1 as support,
    test_support::{Fixture, GhStep},
};
use std::{fs, sync::mpsc, thread};

fn exact(fixture: &Fixture, steps: &[GhStep]) {
    support::exact(fixture, steps);
}

#[test]
fn repo_catalog_read_limits_deadline_conservative_window_preserves_short_clock_and_actual_failed_cap(
) {
    let fixture = Fixture::new();
    let mut slow = GhStep::read(&["slow"], "UNADMITTED_SOURCE");
    slow.delay_ms = 1000;
    fixture.scripted_gh(&[slow.clone()]);
    let deadline = Instant::now() + Duration::from_millis(180);
    let provider = support::provider(&fixture)
        .with_scoped_read_window(deadline, 64)
        .unwrap();
    let mut budget = CatalogueBudget::new(&RepositoryCatalogueLimits::default()).unwrap();
    let start = Instant::now();
    assert!(budget
        .command(
            &provider,
            vec!["slow".into()],
            4096,
            &CancellationToken::default()
        )
        .is_err());
    assert!(start.elapsed() < Duration::from_secs(3));
    assert_eq!(budget.accounting.failed_reserved_bytes(), 64);
    assert_eq!(budget.accounting.requests(), 1);
    assert!(budget
        .command(
            &provider.clone().with_timeout(Duration::from_secs(300)),
            vec!["no-next".into()],
            4096,
            &CancellationToken::default()
        )
        .is_err());
    assert_eq!(
        budget.accounting.requests(),
        1,
        "Expired existing window is refused before another claim/spawn"
    );
    exact(&fixture, &[slow]);
}

#[test]
fn repo_catalog_read_limits_deadline_measured_pool_keeps_original_shared_clock_counts_and_no_replacement(
) {
    let rows = [support::row(0)];
    let mut steps = support::collection(&rows, 20, 42, 42);
    steps.extend(support::principal(42));
    steps.push(support::get("repos/example/repo0000", support::row(0)));
    let mut slow = GhStep::read(&["slow"], "UNADMITTED_SOURCE");
    slow.delay_ms = 1000;
    steps.push(slow);
    steps.extend(support::principal(42));
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let original = support::load(&fixture, &RepositoryCatalogueLimits::default());
    let provider = support::provider(&fixture);
    let cancel = CancellationToken::default();
    let mut session = provider
        .begin_repository_catalogue_access(
            original.source(),
            &RepositoryCatalogueAccessLimits::default(),
            &cancel,
        )
        .unwrap();
    session
        .verify(
            &crate::repository_catalogue_intent_v1::IntentReference {
                id: 10000,
                node_id: Some("R_10000".into()),
                last_name: "example/repo0000".into(),
            },
            &cancel,
        )
        .unwrap();
    let window = session
        .reserve_measured_external_reads(7, 128, 1024, 0, &cancel)
        .unwrap();
    let deadline = Instant::now() + Duration::from_millis(180);
    let scoped = provider
        .clone()
        .with_measured_read_window(window)
        .unwrap()
        .with_catalogue_read_deadline(deadline);
    let mut budget = CatalogueBudget::new(&RepositoryCatalogueLimits::default()).unwrap();
    assert!(budget
        .command(&scoped, vec!["slow".into()], 4096, &cancel)
        .is_err());
    assert_eq!(budget.accounting.failed_reserved_bytes(), 128);
    let newer = scoped
        .clone()
        .with_catalogue_read_deadline(Instant::now() + Duration::from_secs(30))
        .with_timeout(Duration::from_secs(300));
    assert!(budget
        .command(&newer, vec!["no-next".into()], 4096, &cancel)
        .is_err());
    assert_eq!(budget.accounting.requests(), 1);
    let closed = session.close(&cancel).unwrap();
    let actual = closed.external_read_accounting().unwrap();
    assert_eq!(actual.children_started(), 1);
    assert_eq!(actual.requests_claimed(), 1);
    assert_eq!(actual.failed_reserved_bytes(), 128);
    assert_eq!(actual.successful_children(), 0);
    assert_eq!(steps.len(), 13);
    exact(&fixture, &steps);
}

#[test]
fn repo_catalog_read_limits_deadline_combined_stderr_failures_charge_narrow_actual_cap_not_document_cap(
) {
    let fixture = Fixture::new();
    let mut overflow = GhStep::read(&["overflow"], &"a".repeat(40));
    overflow.stderr = vec![1; 40];
    let mut okay = GhStep::read(&["okay"], "ok");
    okay.stderr = b"\x1b[0m\x01\x02".to_vec();
    let steps = [overflow, okay];
    fixture.scripted_gh(&steps);
    let provider = support::provider(&fixture)
        .with_scoped_read_window(Instant::now() + Duration::from_secs(3), 64)
        .unwrap();
    let mut budget = CatalogueBudget::new(&RepositoryCatalogueLimits::default()).unwrap();
    assert!(budget
        .command(
            &provider,
            vec!["overflow".into()],
            4 * 1024 * 1024,
            &CancellationToken::default()
        )
        .is_err());
    assert_eq!(budget.accounting.failed_reserved_bytes(), 64);
    assert_eq!(
        budget
            .command(
                &provider,
                vec!["okay".into()],
                4 * 1024 * 1024,
                &CancellationToken::default()
            )
            .unwrap(),
        b"ok"
    );
    assert_eq!(budget.accounting.successful_stdout_bytes(), 2);
    assert_eq!(budget.accounting.successful_diagnostic_bytes(), 6);
    assert_eq!(budget.accounting.charged_bytes(), 72);
    exact(&fixture, &steps);
}

#[test]
fn repo_catalog_read_limits_deadline_default4mib_capture_is_not_reduced_and_existing_small_cap_stays_small(
) {
    let fixture = Fixture::new();
    let output = "a".repeat(1100000);
    let step = GhStep::read(&["large"], &output);
    fixture.scripted_gh(std::slice::from_ref(&step));
    let provider = support::provider(&fixture)
        .with_catalogue_read_deadline(Instant::now() + Duration::from_secs(3));
    let mut budget = CatalogueBudget::new(&RepositoryCatalogueLimits::default()).unwrap();
    assert_eq!(
        budget
            .command(
                &provider,
                vec!["large".into()],
                4 * 1024 * 1024,
                &CancellationToken::default()
            )
            .unwrap()
            .len(),
        1100000
    );
    assert_eq!(budget.accounting.successful_stdout_bytes(), 1100000);
    assert_eq!(budget.accounting.failed_reserved_bytes(), 0);
    let narrow = provider
        .clone()
        .with_scoped_read_window(Instant::now() + Duration::from_secs(2), 64)
        .unwrap()
        .with_catalogue_read_deadline(Instant::now() + Duration::from_secs(30));
    let spec = narrow.spec(vec!["not-launched".into()]);
    assert_eq!(spec.output_limit, 64);
    assert!(spec.timeout <= Duration::from_secs(2));
    exact(&fixture, &[step]);
}

#[test]
fn repo_catalog_read_limits_deadline_cancel_or_expiry_inside_sql_wait_has_no_reservation_commit_page_or_final(
) {
    for cancel_inside in [false, true] {
        let rows = [support::row(0)];
        let mut steps = support::collection(&rows, 20, 42, 42);
        steps.extend(support::principal(42));
        steps.push(support::get("orgs/example", support::owner()));
        let fixture = Fixture::new();
        fixture.scripted_gh(&steps);
        let original = support::load(&fixture, &RepositoryCatalogueLimits::default());
        let mut cache = crate::storage::Cache::open(&support::database(&fixture)).unwrap();
        let blocker = rusqlite::Connection::open(support::database(&fixture)).unwrap();
        blocker.execute_batch("BEGIN IMMEDIATE").unwrap();
        let token = CancellationToken::default();
        let worker_token = token.clone();
        let old = original.clone();
        let provider = support::provider(&fixture);
        let (entered, waiting) = mpsc::channel();
        let deadline = Instant::now() + Duration::from_secs(1);
        let worker = thread::spawn(move || {
            provider
                .with_catalogue_read_deadline(deadline)
                .repository_catalogue_read_guarded(
                    Some(old.source().identity()),
                    old.source().source(),
                    &RepositoryCatalogueLimits::default(),
                    &worker_token,
                    |phase, context| {
                        if phase != RepositoryCataloguePhase::BeforeEnumeration {
                            return Ok(());
                        }
                        entered.send(()).unwrap();
                        cache
                            .reserve_repository_catalogue_reread(
                                &old,
                                context,
                                deadline,
                                &worker_token,
                            )
                            .map(|_| ())
                            .map_err(|_| {
                                error(
                                    GithubErrorKind::Transport,
                                    "Original SQL deadline/cancel refused",
                                )
                            })
                    },
                )
        });
        waiting
            .recv_timeout(Duration::from_secs(3))
            .expect("Genuine current owner+principal reached SQL callback");
        if cancel_inside {
            token.cancel();
        } else {
            while Instant::now() < deadline {
                thread::yield_now();
            }
        }
        blocker.execute_batch("COMMIT").unwrap();
        assert!(worker.join().unwrap().is_err());
        assert_eq!(
            support::generation(&fixture, &original),
            1,
            "No committed child reservation after SQL wait cancellation/expiry"
        );
        assert_eq!(steps.len(), 10);
        exact(&fixture, &steps);
        let events = fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
        assert!(
            events.lines().skip(7).all(|line| !line.contains("page=1")),
            "No new page after original SQL callback refusal"
        );
        assert_eq!(
            crate::storage::Cache::open(&support::database(&fixture))
                .unwrap()
                .draft(original.scope(), "original")
                .unwrap()
                .as_deref(),
            Some("ORIGINAL_UID42_DRAFT")
        );
    }
}
