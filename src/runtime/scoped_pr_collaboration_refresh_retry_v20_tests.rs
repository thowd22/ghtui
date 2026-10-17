//! ACTUAL accepted request/owner/Background worker, never fabricated completion.
use super::*;
use crate::{
    app::Action,
    forms::FormAction,
    personal_pr_entry_v13::tests::{
        self as personal,
        collaboration_v17::{
            protocol::publication::refresh_v19 as fixture, refresh_v19::refresh_phase,
        },
    },
    scoped_pr_collaboration_refresh_v19::return_v19,
    storage::conversation_refresh_v19::observe_next_actual_reservation_v20,
};
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

fn queued(
    phases: Vec<crate::test_support::GhStep>,
) -> (
    personal::Setup,
    Background,
    crate::drafts::DraftWriter,
    Arc<native::ScopedPrCollaborationWorkV17>,
) {
    let mut tail = fixture::unavailable_tail();
    tail.extend(phases);
    let (mut s, bg, writer) = fixture::background(tail);
    fixture::paint(&mut s.app);
    s.app
        .dispatch(Action::Form(FormAction::RefreshConversationV19));
    native::prepare_requested(&mut s.app);
    let work = s.app.scoped_pr_collaboration_work_v17.take().unwrap();
    (s, bg, writer, work)
}

#[test]
fn conversation_refresh_retry_v20_actual_full_disconnected_overflow_and_occupied_queue_do_not_consume_ready(
) {
    for mode in 0..5 {
        let (mut s, mut bg, writer, work) = queued(Vec::new());
        let (tx, rx) = mpsc::sync_channel(0);
        let mut receiver = None;
        let (finish, wait) = mpsc::sync_channel(1);
        match mode {
            0 => {
                bg.requests = Some(tx);
                receiver = Some(rx);
            }
            1 => {
                drop(rx);
                bg.requests = Some(tx);
            }
            2 => bg.requests = None,
            3 => bg.generation = u64::MAX,
            _ => assert!(bg.submit(&mut s.app, View::Inbox, move |_| {
                wait.recv_timeout(Duration::from_secs(8)).unwrap();
                Ok(PreparedView::default())
            })),
        }
        let generation = bg.generation;
        let slot = bg.current.get(&View::Inbox.index()).cloned();
        let before = fixture::exact_sql(&s);
        let commands = std::fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
        let status = s.app.status.clone();
        let loading = s.app.loading;
        let ticket = s.app.form.as_ref().unwrap().presentation_v4().0.clone();
        let epoch = s.app.form.as_ref().unwrap().presentation_v4().1;
        assert!(!bg.submit_scoped_pr_collaboration_v17(
            &mut s.app,
            work,
            s.api.clone(),
            personal::database(&s.fixture),
            writer.client()
        ));
        assert_eq!(bg.generation, generation);
        assert_eq!(s.app.status, status);
        assert_eq!(s.app.loading, loading);
        match slot {
            Some((g, t)) => {
                assert_eq!(bg.current[&View::Inbox.index()].0, g);
                assert!(bg.current[&View::Inbox.index()].1.same_instance(&t));
                assert!(!t.is_cancelled());
            }
            None => assert!(bg.current.is_empty()),
        }
        assert!(s.app.scoped_pr_collaboration_owner_v17.is_none());
        assert!(Arc::ptr_eq(
            s.app.form.as_ref().unwrap().presentation_v4().0,
            &ticket
        ));
        assert_eq!(s.app.form.as_ref().unwrap().presentation_v4().1, epoch);
        assert_eq!(fixture::exact_sql(&s), before);
        assert_eq!(
            std::fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap(),
            commands
        );
        if mode == 4 {
            finish.send(()).unwrap();
            let completion = bg.completions.recv_timeout(Duration::from_secs(8)).unwrap();
            assert_eq!(completion.view, View::Inbox);
        } else {
            assert!(return_v19::capture_request(&s.app).is_ok());
        }
        drop(receiver);
    }
}

#[test]
fn conversation_refresh_retry_v20_actual_background_caught_panic_and_cancel_settle_exact_original_worker(
) {
    for mode in 0..3 {
        let phase = if mode == 0 {
            Vec::new()
        } else {
            refresh_phase()[..4].to_vec()
        };
        let (mut s, mut bg, writer, work) = queued(phase);
        let actual_sender = bg.requests.as_ref().unwrap().clone();
        // Wrap the REAL accepted request only. Owner, phase, generation, token,
        // closure and final completion remain the actual Background protocol.
        let (capture, requests) = mpsc::sync_channel(1);
        bg.requests = Some(capture);
        assert!(bg.submit_scoped_pr_collaboration_v17(
            &mut s.app,
            work,
            s.api.clone(),
            personal::database(&s.fixture),
            writer.client()
        ));
        let mut request = requests.recv_timeout(Duration::from_secs(2)).unwrap();
        let owner = s.app.scoped_pr_collaboration_owner_v17.clone().unwrap();
        let original = request.work;
        let (entered, arrived) = mpsc::sync_channel(1);
        let (release, resume) = mpsc::sync_channel(1);
        request.work = Box::new(move |token| {
            if mode == 0 {
                panic!("ACTUAL_V20_BEFORE_RESERVATION_BACKGROUND_PANIC");
            }
            observe_next_actual_reservation_v20(move || {
                if mode == 1 {
                    panic!("ACTUAL_V20_AFTER_COMMIT_BACKGROUND_PANIC");
                }
                entered.send(()).unwrap();
                resume.recv_timeout(Duration::from_secs(8)).unwrap();
            });
            original(token)
        });
        actual_sender.send(request).unwrap();
        bg.requests = Some(actual_sender);
        if mode == 2 {
            arrived.recv_timeout(Duration::from_secs(8)).unwrap();
            assert!(bg.cancel_scoped_pr_collaboration_v17(&mut s.app));
            assert!(owner.cancellation().is_cancelled());
            fixture::paint(&mut s.app);
            assert!(return_v19::capture_request(&s.app).is_err());
            let before = fixture::exact_sql(&s);
            let status = s.app.status.clone();
            let loading = s.app.loading;
            release.send(()).unwrap();
            let mut completion = bg.completions.recv_timeout(Duration::from_secs(8)).unwrap();
            assert_eq!(
                bg.poll_scoped_pr_collaboration_v17(&mut s.app, &mut completion),
                Some(false)
            );
            assert_eq!(fixture::exact_sql(&s), before);
            assert_eq!(s.app.status, status);
            assert_eq!(s.app.loading, loading);
        } else {
            let until = Instant::now() + Duration::from_secs(8);
            while s.app.loading[View::Inbox.index()] {
                bg.poll(&mut s.app);
                assert!(Instant::now() < until);
                std::thread::yield_now();
            }
            assert!(!owner.cancellation().is_cancelled());
        }
        fixture::paint(&mut s.app);
        assert!(
            return_v19::capture_request(&s.app).is_ok(),
            "Only after actual worker settlement"
        );
        crate::personal_pr_entry_v13::tests::collaboration_v17::protocol::publication::exact_protocol(&s);
    }
}
