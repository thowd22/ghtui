//! Actual closed image/Summary source, real journal/SQLite and strict offline
//! transport. These supplement separately declared actual key/mouse journeys.
use super::*;
use crate::{
    pr_write_journal::{PrPublicationReference, PrWriteIntent, PrWriteState},
    providers::runner::CommandRunner,
};
use operations::history_v3::{self as review_history, recovery_v11 as recovery};

fn summary_steps(h: &history::GitImagesV1) -> Vec<GhStep> {
    let mut steps = opening_steps(h);
    steps.extend(closing_steps(h));
    steps
}
fn current_steps(h: &history::GitImagesV1, selected: bool) -> Vec<GhStep> {
    let mut steps = opening_steps(h);
    let row = review(h, "PENDING", "CURRENT observed summary", 42);
    steps.push(catalogue_step(json!([row.clone()])));
    if selected {
        steps.extend(snapshot_steps(h, &row));
    }
    steps.extend(closing_steps(h));
    steps
}
struct Real {
    setup: Setup,
    app: App,
    api: GithubProvider,
    runner: CommandRunner,
    target: PrWriteTarget,
    reference: PrPublicationReference,
    key: String,
    encoded: String,
}
fn real(selected: bool, inspect: bool, ack: bool) -> Real {
    let mut s = inline_setup(|h, steps| {
        steps.extend(summary_steps(h));
        steps.extend(summary_steps(h));
        if inspect {
            steps.extend(current_steps(h, selected));
        }
        if ack {
            steps.extend(summary_steps(h));
        }
    });
    let (mut app, _) = owner(&mut s);
    let actual = ScopedReviewIntentV2::capture(&app).unwrap();
    let target = PrWriteTarget::new(
        &app.github.as_ref().unwrap().identity,
        actual.parent().summary(),
    )
    .unwrap();
    let key = actual.draft_key().unwrap();
    let body = "PROTECTED local review request • never replayed 界";
    let write = PrWriteIntent {
        command: if selected {
            "review-submit-v2"
        } else {
            "review-create-pending-v2"
        }
        .into(),
        selected_resource: if selected {
            "review/810".into()
        } else {
            key.clone()
        },
        observed_head: s.h.h.clone(),
        values: std::collections::BTreeMap::from([("body".into(), body.into())]),
    };
    let mut journal = PrWriteJournal::open(&database(&s.fixture)).unwrap();
    let permit = journal.begin(&target, &write).unwrap();
    journal.claim(&permit, &target, &write).unwrap();
    journal.mark_uncertain(&permit).unwrap();
    let reference = permit.marker().publication_reference();
    let encoded = format!("ORIGINAL protected draft {}", reference.payload_sha256);
    Cache::open(&database(&s.fixture))
        .unwrap()
        .put_draft(target.scope(), &key, &encoded)
        .unwrap();
    // This is the genuine already-loaded Summary details from the real source,
    // changing ONLY the local presentation. No Files/resource/source invented.
    app.pr_file_images = None;
    app.overlay = None;
    let github = app.github.take().unwrap();
    let api = provider(&s.fixture);
    app.github = Some(
        github
            .load_tab(
                crate::github_workspace::PrTab::Summary,
                &api,
                &Default::default(),
            )
            .unwrap(),
    );
    app.github.as_mut().unwrap().scoped_pr_resource = None;
    let runner = CommandRunner::default();
    let api = api.with_runner(runner.clone());
    let resource =
        crate::providers::gh::canonical_github_resource(HOST, &format!("{HOST}/{REPO}/pull/7"));
    runner.mark_ambiguous(&resource);
    s.sql_before = sql_state(&s.fixture);
    Real {
        setup: s,
        app,
        api,
        runner,
        target,
        reference,
        key,
        encoded,
    }
}
fn request(real: &Real) -> Arc<recovery::ReviewRecoveryRequestV11> {
    let intent = review_history::ReviewOperationHistoryIntentV3::capture(&real.app).unwrap();
    let list = review_history::load(
        intent.clone(),
        None,
        &real.api,
        &database(&real.setup.fixture),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    assert_eq!(
        list.rows(),
        1,
        "originless V2 blocker must be reachable, never dropped by V3-only selector"
    );
    let opened = review_history::load(
        intent,
        Some((&list, 0)),
        &real.api,
        &database(&real.setup.fixture),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    recovery::ReviewRecoveryRequestV11::capture(opened, 0).unwrap()
}
fn inspect(
    real: &Real,
    request: Arc<recovery::ReviewRecoveryRequestV11>,
) -> Arc<recovery::PreparedReviewRecoveryV11> {
    recovery::inspect(
        request,
        &real.api,
        &database(&real.setup.fixture),
        &Default::default(),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap()
}
fn assert_kept(real: &Real, state: PrWriteState) {
    let journal = PrWriteJournal::open(&database(&real.setup.fixture)).unwrap();
    assert_eq!(
        journal
            .original_publication(&real.target, &real.reference)
            .unwrap()
            .unwrap()
            .state,
        state
    );
    assert_eq!(
        Cache::open(&database(&real.setup.fixture))
            .unwrap()
            .draft(real.target.scope(), &real.key)
            .unwrap()
            .as_deref(),
        Some(real.encoded.as_str())
    );
    unchanged_git_and_exact_script(&real.setup);
}
#[test]
fn pending_reviews_v11_fresh_complete_unknown_create_inspection_then_explicit_once_local_ack_never_resends_or_deletes_body(
) {
    let real = real(false, true, true);
    let request = request(&real);
    let prepared = inspect(&real, request);
    assert!(prepared.available());
    assert_eq!(prepared.identity().account_id, Some(42));
    let mut viewport = Default::default();
    let window = prepared
        .display()
        .window(&mut viewport, 120, 28, 0)
        .unwrap();
    let text = window
        .rows()
        .map(|row| row.text())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("Attribution is UNKNOWN"));
    assert!(text.contains("NOT proof"));
    assert!(!text.contains(&real.encoded));
    let before = commands(&real.setup.fixture).len();
    let sql = sql_state(&real.setup.fixture);
    let result = recovery::acknowledge(
        &prepared,
        &real.api,
        &database(&real.setup.fixture),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    assert_eq!(result.changed, 1);
    assert_eq!(result.reference, real.reference);
    assert_eq!(
        commands(&real.setup.fixture).len(),
        before + summary_steps(&real.setup.h).len()
    );
    assert_eq!(
        sql_state(&real.setup.fixture),
        sql,
        "all draft/cache/body rows retained byte-exact; only journal acknowledgement changes"
    );
    let after = commands(&real.setup.fixture).len();
    assert!(recovery::acknowledge(
        &prepared,
        &real.api,
        &database(&real.setup.fixture),
        &Default::default(),
        Instant::now() + Duration::from_secs(30)
    )
    .is_err());
    assert_eq!(commands(&real.setup.fixture).len(), after);
    assert_kept(&real, PrWriteState::Acknowledged);
}
#[test]
fn pending_reviews_v11_selected_review_complete_current_comment_observation_refuses_newer_journal_or_runner_generation(
) {
    for runner in [false, true] {
        let real = real(true, true, false);
        let prepared = inspect(&real, request(&real));
        let before = commands(&real.setup.fixture).len();
        let path = database(&real.setup.fixture);
        if runner {
            let resource = crate::providers::gh::canonical_github_resource(
                HOST,
                &format!("{HOST}/{REPO}/pull/7"),
            );
            real.runner.mark_ambiguous(&resource);
        } else {
            rusqlite::Connection::open(&path)
                .unwrap()
                .execute("UPDATE pr_write_scopes SET generation=generation+1", [])
                .unwrap();
        }
        let sql = sql_state(&real.setup.fixture);
        assert!(recovery::acknowledge(
            &prepared,
            &real.api,
            &path,
            &Default::default(),
            Instant::now() + Duration::from_secs(30)
        )
        .is_err());
        assert_eq!(
            commands(&real.setup.fixture).len(),
            before,
            "stale original inspection cannot open another source or call transport"
        );
        assert_eq!(sql_state(&real.setup.fixture), sql);
        assert_kept(&real, PrWriteState::Uncertain);
    }
}
#[test]
fn pending_reviews_v11_cancelled_or_expired_original_inspection_is_zero_child_and_retains_exact_blocker(
) {
    for cancelled in [false, true] {
        let real = real(false, false, false);
        let request = request(&real);
        let before = commands(&real.setup.fixture).len();
        let token = CancellationToken::default();
        if cancelled {
            token.cancel();
        }
        let deadline = if cancelled {
            Instant::now() + Duration::from_secs(30)
        } else {
            Instant::now() - Duration::from_millis(1)
        };
        assert!(recovery::inspect(
            request,
            &real.api,
            &database(&real.setup.fixture),
            &Default::default(),
            &token,
            deadline
        )
        .is_err());
        assert_eq!(commands(&real.setup.fixture).len(), before);
        assert_kept(&real, PrWriteState::Uncertain);
    }
}
#[test]
fn pending_reviews_v11_sql_failure_after_same_runner_reconciliation_keeps_durable_blocker_and_original_body(
) {
    let real = real(false, true, true);
    let prepared = inspect(&real, request(&real));
    let path = database(&real.setup.fixture);
    let generation = PrWriteJournal::open(&path)
        .unwrap()
        .source_generation(&real.target)
        .unwrap();
    rusqlite::Connection::open(&path).unwrap().execute_batch("CREATE TRIGGER refuse_review_ack_v11 BEFORE UPDATE OF state ON pr_write_attempts WHEN NEW.state='acknowledged' BEGIN SELECT RAISE(ABORT,'actual fixture acknowledgement SQL failure'); END;").unwrap();
    assert!(recovery::acknowledge(
        &prepared,
        &real.api,
        &path,
        &Default::default(),
        Instant::now() + Duration::from_secs(30)
    )
    .is_err());
    assert_eq!(
        PrWriteJournal::open(&path)
            .unwrap()
            .source_generation(&real.target)
            .unwrap(),
        generation
    );
    assert_kept(&real, PrWriteState::Uncertain);
}
#[test]
fn pending_reviews_v11_exact_known_purpose_ref_database_and_numeric_scope_required_without_foreign_release(
) {
    let real = real(false, false, false);
    let _request = request(&real);
    let path = database(&real.setup.fixture);
    let journal = PrWriteJournal::open(&path).unwrap();
    let mut other = real.reference.clone();
    other.attempt_id += 1;
    assert!(journal
        .prepare_review_inspection_v11(&real.target, &other)
        .is_err());
    let foreign = PrWriteTarget::for_source(
        &Scope::github(HOST, Some(43)).unwrap(),
        REPO,
        real.target.node_id(),
        7,
    )
    .unwrap();
    assert!(journal
        .prepare_review_inspection_v11(&foreign, &real.reference)
        .is_err());
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute(
            "UPDATE pr_write_attempts SET command='thread-reply' WHERE attempt_id=?1",
            [real.reference.attempt_id],
        )
        .unwrap();
    assert!(journal
        .prepare_review_inspection_v11(&real.target, &real.reference)
        .is_err());
    assert_eq!(
        journal
            .original_publication(&real.target, &real.reference)
            .unwrap()
            .unwrap()
            .state,
        PrWriteState::Uncertain
    );
    unchanged_git_and_exact_script(&real.setup);
    assert_eq!(
        Cache::open(&path)
            .unwrap()
            .draft(real.target.scope(), &real.key)
            .unwrap()
            .as_deref(),
        Some(real.encoded.as_str())
    );
}
