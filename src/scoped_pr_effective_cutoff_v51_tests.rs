//! Actual native/Core/SQLite cutoff evidence. No fabricated producer, seal,
//! clock, source or receipt; these are code contracts, not PTY claims.
use super::*;
fn until(cutoff: Instant) {
    let bound = Instant::now() + Duration::from_secs(8);
    while Instant::now() < cutoff {
        assert!(
            Instant::now() < bound,
            "Actual issuer wait exceeds test bound"
        );
        std::thread::yield_now();
    }
}
fn no_slot(s: &Setup, background: &Background) {
    assert!(!s.app.loading[View::Inbox.index()]);
    assert!(channel::generation(background).is_none());
    assert!(s.app.scoped_pr_collaboration_owner_v17.is_none());
}
#[test]
fn pr_metadata_original_v25_effective_cutoff_v51_narrower_live_paid_projection_accepts_wider_request(
) {
    for field in [Field::Title, Field::Description] {
        let (s, form, preview) = prepared(field, paid(field));
        let issuer = Instant::now() + Duration::from_secs(10);
        let outcome = execute_with_issuer_v51(&s, &form, &preview, Some(issuer)).unwrap();
        assert_eq!(outcome.state, State::Confirmed);
        assert!(outcome.refreshed_page_v19.is_some());
        assert!(outcome.return_v19.is_none());
        assert!(!outcome.draft_retained());
        let journal = PrWriteJournal::open(&database(&s.fixture)).unwrap();
        let target = target(&s, &form);
        let marker = journal
            .original_publication(&target, &outcome.reference)
            .unwrap()
            .unwrap();
        assert_eq!(marker.state, PrWriteState::Confirmed);
        assert!(marker.confirmed.is_some());
        assert!(Instant::now() < issuer);
        assert!(journal.blocking(&target).unwrap().is_none());
        exact(&s, s.steps.len());
    }
}
#[test]
fn pr_metadata_original_v25_effective_cutoff_v51_narrower_live_native_fallback_and_new_read_are_same_effect(
) {
    for catalogue in [true, false] {
        let field = if catalogue {
            Field::Title
        } else {
            Field::Description
        };
        let (mut s, mut background, writer, _, issuer) = returned_with_issuer_v51(
            catalogue,
            read_phase(field, true),
            Some(Duration::from_secs(4)),
        );
        assert!(Instant::now() < issuer.unwrap());
        let original = protected(&s);
        let read_issuer = Instant::now() + Duration::from_secs(4);
        submit_read_with_issuer_v51(&mut s, &mut background, &writer, Some(read_issuer));
        settle(&mut s, &mut background);
        assert!(s.app.scoped_pr_collaboration_return_v19.is_none());
        assert!(s.app.form.is_none());
        assert!(s.app.github.as_ref().unwrap().pr.is_some());
        assert!(Instant::now() < read_issuer);
        assert_eq!(protected(&s), original);
        no_slot(&s, &background);
        exact(&s, s.steps.len());
    }
}
#[test]
fn pr_metadata_original_v25_effective_cutoff_v51_expired_old_publication_does_not_expire_new_genuine_read(
) {
    for catalogue in [true, false] {
        let field = if catalogue {
            Field::Title
        } else {
            Field::Description
        };
        let (mut s, mut background, writer, _, issuer) = returned_with_issuer_v51(
            catalogue,
            read_phase(field, true),
            Some(Duration::from_secs(4)),
        );
        let original = protected(&s);
        until(issuer.unwrap());
        // The old operation is expired, but this explicit action gets a NEW
        // normal Core/window/token; no old source/access lease is renewed.
        assert!(return_v19::capture_request(&s.app).is_ok());
        submit_read(&mut s, &mut background, &writer);
        settle(&mut s, &mut background);
        assert!(s.app.scoped_pr_collaboration_return_v19.is_none());
        assert!(s.app.form.is_none());
        assert!(s.app.github.as_ref().unwrap().pr.is_some());
        assert_eq!(protected(&s), original);
        no_slot(&s, &background);
        exact(&s, s.steps.len());
    }
}
#[test]
fn pr_metadata_original_v25_effective_cutoff_v51_held_final_uid_expiry_keeps_exact_sql_and_return()
{
    let field = Field::Description;
    let mut read = read_phase(field, true);
    read.last_mut().unwrap().delay_ms = 2000;
    let (mut s, mut background, writer, _, _) = returned_with_issuer_v51(false, read, None);
    let issuer = Instant::now() + Duration::from_secs(3);
    submit_read_with_issuer_v51(&mut s, &mut background, &writer, Some(issuer));
    let held = evidence::StoppedV19::final_uid(&s);
    let before = evidence::exact_sql(&s);
    let original = protected(&s);
    until(issuer + Duration::from_millis(20));
    drop(held);
    settle(&mut s, &mut background);
    assert!(s.app.github.is_none());
    assert!(s.app.scoped_pr_collaboration_return_v19.is_some());
    assert_eq!(evidence::exact_sql(&s), before);
    assert_eq!(protected(&s), original);
    no_slot(&s, &background);
    exact(&s, s.steps.len());
}
#[test]
fn pr_metadata_original_v25_effective_cutoff_v51_final_uid_then_lock_expiry_refuses_when_wider_request_still_live(
) {
    let field = Field::Title;
    let mut read = read_phase(field, true);
    read.last_mut().unwrap().delay_ms = 2000;
    let (mut s, mut background, writer, _, _) = returned_with_issuer_v51(true, read, None);
    let issuer = Instant::now() + Duration::from_secs(3);
    let request_upper = issuer + Duration::from_secs(20);
    submit_read_with_issuer_v51(&mut s, &mut background, &writer, Some(issuer));
    let held = evidence::StoppedV19::final_uid(&s);
    let before = evidence::exact_sql(&s);
    let original = protected(&s);
    let mut locker = writable(&s);
    let lock = locker
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    drop(held);
    // Final UID may finish, but the REAL Immediate lock cannot be acquired
    // until just after I. Old D would still be live and incorrectly publish.
    until(issuer + Duration::from_millis(100));
    assert!(Instant::now() < request_upper);
    lock.commit().unwrap();
    settle(&mut s, &mut background);
    assert!(s.app.github.is_none());
    assert!(s.app.scoped_pr_collaboration_return_v19.is_some());
    assert_eq!(evidence::exact_sql(&s), before);
    assert_eq!(protected(&s), original);
    no_slot(&s, &background);
    exact(&s, s.steps.len());
}
#[test]
fn pr_metadata_original_v25_effective_cutoff_v51_durable_confirmed_expiry_cannot_release_fallback_fence_or_return(
) {
    let field = Field::Description;
    let mut calls = paid(field);
    assert_eq!(calls.len(), 21);
    calls[19].delay_ms = 2000;
    calls.truncate(20);
    let (s, form, preview) = prepared(field, calls);
    let issuer = Instant::now() + Duration::from_secs(3);
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| execute_with_issuer_v51(&s, &form, &preview, Some(issuer)));
        // Existing owned-process helper stops this exact explicitly marked
        // paid closing-PR child, AFTER the actual durable Confirmed receipt.
        let held = evidence::StoppedV19::final_uid(&s);
        let before = evidence::exact_sql(&s);
        let journal = PrWriteJournal::open(&database(&s.fixture)).unwrap();
        let marker = journal
            .latest_matching(
                &target(&s, &form),
                &preview.seed.write(&preview.values).unwrap(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(marker.state, PrWriteState::Confirmed);
        assert!(marker.confirmed.is_some());
        until(issuer + Duration::from_millis(20));
        drop(held);
        let outcome = worker.join().unwrap().unwrap();
        assert_eq!(outcome.state, State::ConfirmedVisibilityUnavailable);
        assert!(outcome.draft_retained());
        assert!(outcome.refreshed_page_v19.is_none());
        assert!(outcome.return_v19.is_none());
        assert_eq!(evidence::exact_sql(&s), before);
        let db = writable(&s);
        assert_eq!(
            db.query_row("SELECT count(*) FROM sync_mutation_fences", [], |row| row
                .get::<_, usize>(
                0
            ))
            .unwrap(),
            0
        );
        assert_eq!(outcome.reference, marker.publication_reference());
        exact(&s, s.steps.len());
    });
}
