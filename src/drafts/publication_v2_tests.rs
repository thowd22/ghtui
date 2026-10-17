//! Real ordered queue/SQLite contracts. Timed-out acceptance is never no-write
//! evidence; publication activity is released by completion, not caller Drop.
use super::*;
use std::sync::mpsc::{self, Receiver};
use std::{fs, os::unix::fs::PermissionsExt};

const RESOURCE: &str =
    "review-draft/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
fn scope() -> Scope {
    Scope::github("github.invalid", Some(42)).unwrap()
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(3)
}
fn fixture() -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let path = directory.path().join("cache.sqlite3");
    Cache::open(&path).unwrap();
    (directory, path)
}
fn controlled(path: &Path) -> (DraftWriter, Receiver<Message>) {
    let (sender, receiver) = mpsc::sync_channel(8);
    let (_, acks) = mpsc::channel();
    (
        DraftWriter {
            sender,
            acks,
            worker: None,
            pending: BTreeMap::new(),
            producer: Arc::new(DraftProducerV4::new(path.into())),
        },
        receiver,
    )
}
fn dispatch(writer: &DraftWriter, message: Message, cache: &Result<Cache, String>) {
    writer.producer.publication_capacity_v2();
    match message {
        Message::Save(scope, resource, body, revision) => {
            let result = cache.as_ref().map_err(Clone::clone).and_then(|cache| {
                cache
                    .put_draft(&scope, &resource, &body)
                    .map_err(|error| error.to_string())
            });
            writer.producer.complete(revision, &result);
        }
        Message::PublicationV2(message) => message.run(cache),
        _ => panic!("this contract expects actual Save or publication message"),
    }
}
fn next(receiver: &Receiver<Message>) -> Message {
    receiver.recv_timeout(Duration::from_secs(2)).unwrap()
}
fn finish(mut writer: DraftWriter, receiver: Receiver<Message>) {
    writer.pending.clear();
    drop(receiver);
    drop(writer);
}
fn initial(writer: &mut DraftWriter, receiver: &Receiver<Message>, cache: &Result<Cache, String>) {
    writer
        .save(scope(), RESOURCE.into(), "original body".into())
        .unwrap();
    dispatch(writer, next(receiver), cache);
}
fn wait_activity(writer: &DraftWriter, expected: usize) {
    let end = deadline();
    loop {
        if writer.producer.lock().unwrap().legacy_active == expected {
            return;
        }
        assert!(
            Instant::now() < end,
            "actual original publication activity never reached expected count"
        );
        thread::yield_now();
    }
}

#[test]
fn pending_reviews_v2_draft_publication_coalesced_cutoff_waits_original_revision_without_replaying_body(
) {
    let (_directory, path) = fixture();
    let cache = Ok(Cache::open(&path).unwrap());
    let (mut writer, receiver) = controlled(&path);
    for index in 0..8 {
        writer
            .save(
                scope(),
                format!("older-{index}"),
                format!("older body{index}"),
            )
            .unwrap();
    }
    writer
        .save(scope(), RESOURCE.into(), "exact coalesced body".into())
        .unwrap();
    let revision = writer.pending.values().next().unwrap().revision;
    let token = CancellationToken::default();
    let client = writer.client();
    let fence = client
        .capture_publication_v2(&path, &scope(), RESOURCE, &token, deadline())
        .unwrap();
    assert_eq!(fence.snapshot.revision, revision);
    assert!(writer
        .producer
        .lock()
        .unwrap()
        .outstanding
        .contains(&revision));
    let waiting = thread::spawn(move || {
        client.save_publication_and_wait_v2(fence, "exact durable reference".into(), &token)
    });
    for _ in 0..8 {
        dispatch(&writer, next(&receiver), &cache);
    }
    writer.poll();
    dispatch(&writer, next(&receiver), &cache);
    let message = next(&receiver);
    assert!(matches!(message, Message::PublicationV2(_)));
    dispatch(&writer, message, &cache);
    let ack = waiting.join().unwrap().unwrap();
    ack.check_current(
        &writer.client(),
        &path,
        &scope(),
        RESOURCE,
        &ack.fence.cancel,
    )
    .unwrap();
    assert!(writer.pending.is_empty());
    assert!(writer.producer.lock().unwrap().outstanding.is_empty());
    assert_eq!(
        cache
            .as_ref()
            .unwrap()
            .draft(&scope(), RESOURCE)
            .unwrap()
            .as_deref(),
        Some("exact durable reference")
    );
    for index in 0..8 {
        assert_eq!(
            cache
                .as_ref()
                .unwrap()
                .draft(&scope(), &format!("older-{index}"))
                .unwrap()
                .as_deref(),
            Some(format!("older body{index}").as_str())
        );
    }
    finish(writer, receiver);
}

#[test]
fn pending_reviews_v2_draft_publication_full_queue_expiry_never_enqueues_or_retries_body() {
    let (_directory, path) = fixture();
    let cache = Ok(Cache::open(&path).unwrap());
    let (mut writer, receiver) = controlled(&path);
    initial(&mut writer, &receiver, &cache);
    // Genuine old API fills this SAME bounded queue; its API is unchanged.
    writer
        .client()
        .save_pending(
            (0..8)
                .map(|index| {
                    (
                        scope(),
                        format!("queued-{index}"),
                        format!("queued body{index}"),
                    )
                })
                .collect(),
        )
        .unwrap();
    let token = CancellationToken::default();
    let client = writer.client();
    let fence = client
        .capture_publication_v2(
            &path,
            &scope(),
            RESOURCE,
            &token,
            Instant::now() + Duration::from_millis(80),
        )
        .unwrap();
    let failure = client
        .save_publication_and_wait_v2(fence, "must never enqueue".into(), &token)
        .unwrap_err();
    assert!(!failure.acknowledgement_unknown());
    assert!(failure.to_string().contains("deadline"));
    assert_eq!(writer.producer.lock().unwrap().legacy_active, 0);
    for _ in 0..8 {
        let message = next(&receiver);
        assert!(matches!(message, Message::Save(..)));
        dispatch(&writer, message, &cache);
    }
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    assert_eq!(
        cache
            .as_ref()
            .unwrap()
            .draft(&scope(), RESOURCE)
            .unwrap()
            .as_deref(),
        Some("original body")
    );
    finish(writer, receiver);
}

#[test]
fn pending_reviews_v2_draft_publication_accepted_cancel_keeps_activity_until_actual_queued_skip_and_blocks_corpus(
) {
    for expire in [false, true] {
        let (_directory, path) = fixture();
        let cache = Ok(Cache::open(&path).unwrap());
        let (mut writer, receiver) = controlled(&path);
        initial(&mut writer, &receiver, &cache);
        let token = CancellationToken::default();
        let cancel = token.clone();
        let client = writer.client();
        let fence = client
            .capture_publication_v2(
                &path,
                &scope(),
                RESOURCE,
                &token,
                if expire {
                    Instant::now() + Duration::from_millis(80)
                } else {
                    deadline()
                },
            )
            .unwrap();
        let waiting = thread::spawn(move || {
            client.save_publication_and_wait_v2(fence, "must not reach SQL".into(), &token)
        });
        let accepted = next(&receiver);
        assert!(matches!(accepted, Message::PublicationV2(_)));
        if !expire {
            cancel.cancel();
        }
        assert!(waiting
            .join()
            .unwrap()
            .unwrap_err()
            .acknowledgement_unknown());
        assert_eq!(
            writer.producer.lock().unwrap().legacy_active,
            1,
            "caller exit does not retire accepted activity"
        );
        assert!(writer
            .capture_corpus_preview_v4(
                scope(),
                "corpus-management:original".into(),
                "new corpus".into(),
                &path,
                deadline()
            )
            .unwrap_err()
            .contains("still active"));
        dispatch(&writer, accepted, &cache);
        assert_eq!(writer.producer.lock().unwrap().legacy_active, 0);
        assert_eq!(
            cache
                .as_ref()
                .unwrap()
                .draft(&scope(), RESOURCE)
                .unwrap()
                .as_deref(),
            Some("original body")
        );
        finish(writer, receiver);
    }
}

#[test]
fn pending_reviews_v2_draft_publication_newer_revision_before_sql_skips_old_and_preserves_exact_new_body(
) {
    let (_directory, path) = fixture();
    let cache = Ok(Cache::open(&path).unwrap());
    let (mut writer, receiver) = controlled(&path);
    initial(&mut writer, &receiver, &cache);
    let token = CancellationToken::default();
    let client = writer.client();
    let fence = client
        .capture_publication_v2(&path, &scope(), RESOURCE, &token, deadline())
        .unwrap();
    let waiting = thread::spawn(move || {
        client.save_publication_and_wait_v2(fence, "obsolete reference".into(), &token)
    });
    let accepted = next(&receiver);
    writer
        .save(scope(), RESOURCE.into(), "exact NEWER body".into())
        .unwrap();
    dispatch(&writer, accepted, &cache);
    let failure = waiting.join().unwrap().unwrap_err();
    assert!(!failure.acknowledgement_unknown());
    dispatch(&writer, next(&receiver), &cache);
    assert_eq!(
        cache
            .as_ref()
            .unwrap()
            .draft(&scope(), RESOURCE)
            .unwrap()
            .as_deref(),
        Some("exact NEWER body")
    );
    assert_eq!(writer.producer.lock().unwrap().legacy_active, 0);
    finish(writer, receiver);
}

#[test]
fn pending_reviews_v2_draft_publication_real_held_sql_timeout_retains_completion_activity_and_newer_save_wins(
) {
    let (_directory, path) = fixture();
    let mut writer = DraftWriter::new(path.clone());
    writer
        .save(scope(), RESOURCE.into(), "original body".into())
        .unwrap();
    let end = deadline();
    while !writer.producer.lock().unwrap().outstanding.is_empty() {
        writer.poll();
        assert!(Instant::now() < end);
        thread::yield_now();
    }
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute_batch("BEGIN IMMEDIATE;").unwrap();
    let token = CancellationToken::default();
    let client = writer.client();
    let fence = client
        .capture_publication_v2(
            &path,
            &scope(),
            RESOURCE,
            &token,
            Instant::now() + Duration::from_millis(300),
        )
        .unwrap();
    let waiting = thread::spawn(move || {
        client.save_publication_and_wait_v2(fence, "accepted reference".into(), &token)
    });
    wait_activity(&writer, 1);
    let failure = waiting.join().unwrap().unwrap_err();
    assert!(failure.acknowledgement_unknown());
    assert_eq!(
        writer.producer.lock().unwrap().legacy_active,
        1,
        "actual held queue/SQL completion still owns activity after timeout"
    );
    assert!(writer
        .capture_corpus_preview_v4(
            scope(),
            "corpus-management:original".into(),
            "new corpus".into(),
            &path,
            deadline()
        )
        .unwrap_err()
        .contains("still active"));
    writer
        .save(
            scope(),
            RESOURCE.into(),
            "newer body while original SQL held".into(),
        )
        .unwrap();
    sql.execute_batch("COMMIT;").unwrap();
    wait_activity(&writer, 0);
    let end = deadline();
    while !writer.producer.lock().unwrap().outstanding.is_empty() {
        writer.poll();
        assert!(Instant::now() < end);
        thread::yield_now();
    }
    assert_eq!(
        Cache::open(&path)
            .unwrap()
            .draft(&scope(), RESOURCE)
            .unwrap()
            .as_deref(),
        Some("newer body while original SQL held")
    );
    assert!(Cache::open(&path)
        .unwrap()
        .draft(
            &Scope::github("github.invalid", Some(43)).unwrap(),
            RESOURCE
        )
        .unwrap()
        .is_none());
    drop(writer);
}

#[test]
fn pending_reviews_v2_draft_publication_ack_is_exact_original_client_database_scope_resource_token_and_revision(
) {
    let (_directory, path) = fixture();
    let cache = Ok(Cache::open(&path).unwrap());
    let (mut writer, receiver) = controlled(&path);
    initial(&mut writer, &receiver, &cache);
    let token = CancellationToken::default();
    let original = token.clone();
    let client = writer.client();
    let fence = client
        .capture_publication_v2(&path, &scope(), RESOURCE, &token, deadline())
        .unwrap();
    let waiting = thread::spawn(move || {
        client.save_publication_and_wait_v2(fence, "durable reference".into(), &token)
    });
    dispatch(&writer, next(&receiver), &cache);
    let ack = waiting.join().unwrap().unwrap();
    ack.check_current(&writer.client(), &path, &scope(), RESOURCE, &original)
        .unwrap();
    assert!(ack
        .check_current(
            &writer.client(),
            &path,
            &scope(),
            RESOURCE,
            &CancellationToken::default()
        )
        .is_err());
    assert!(ack
        .check_current(
            &writer.client(),
            &path,
            &Scope::github("github.invalid", Some(43)).unwrap(),
            RESOURCE,
            &original
        )
        .is_err());
    assert!(ack
        .check_current(
            &writer.client(),
            &path,
            &scope(),
            "another-resource",
            &original
        )
        .is_err());
    assert!(ack
        .check_current(
            &writer.client(),
            &path.with_file_name("other.sqlite"),
            &scope(),
            RESOURCE,
            &original
        )
        .is_err());
    let (other, other_receiver) = controlled(&path);
    assert!(ack
        .check_current(&other.client(), &path, &scope(), RESOURCE, &original)
        .is_err());
    finish(other, other_receiver);
    writer
        .save(scope(), RESOURCE.into(), "newer normal body".into())
        .unwrap();
    assert!(ack
        .check_current(&writer.client(), &path, &scope(), RESOURCE, &original)
        .is_err());
    dispatch(&writer, next(&receiver), &cache);
    finish(writer, receiver);
}

#[test]
fn pending_reviews_v2_draft_publication_disconnect_shutdown_worker_death_and_sibling_token_fail_closed(
) {
    let (_directory, path) = fixture();
    let (writer, receiver) = controlled(&path);
    let token = CancellationToken::default();
    let client = writer.client();
    let fence = client
        .capture_publication_v2(&path, &scope(), RESOURCE, &token, deadline())
        .unwrap();
    drop(receiver);
    assert!(!client
        .save_publication_and_wait_v2(fence, "not accepted".into(), &token)
        .unwrap_err()
        .acknowledgement_unknown());
    assert_eq!(writer.producer.lock().unwrap().legacy_active, 0);
    drop(writer);
    let (writer, receiver) = controlled(&path);
    let token = CancellationToken::default();
    let client = writer.client();
    let fence = client
        .capture_publication_v2(&path, &scope(), RESOURCE, &token, deadline())
        .unwrap();
    assert!(!client
        .save_publication_and_wait_v2(fence, "wrong token".into(), &CancellationToken::default())
        .unwrap_err()
        .acknowledgement_unknown());
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    finish(writer, receiver);
    let (writer, receiver) = controlled(&path);
    let token = CancellationToken::default();
    let client = writer.client();
    let fence = client
        .capture_publication_v2(&path, &scope(), RESOURCE, &token, deadline())
        .unwrap();
    let waiting = thread::spawn(move || {
        client.save_publication_and_wait_v2(fence, "accepted before death".into(), &token)
    });
    let accepted = next(&receiver);
    drop(accepted);
    writer.producer.close();
    assert!(waiting
        .join()
        .unwrap()
        .unwrap_err()
        .acknowledgement_unknown());
    assert_eq!(writer.producer.lock().unwrap().legacy_active, 0);
    finish(writer, receiver);
}

#[test]
fn pending_reviews_v2_draft_publication_initial_absence_is_affirmative_virgin_only_and_cleanup_keeps_same_clock(
) {
    let (_directory, path) = fixture();
    let cache = Ok(Cache::open(&path).unwrap());
    let (writer, receiver) = controlled(&path);
    let token = CancellationToken::default();
    let original = token.clone();
    let client = writer.client();
    let fence = client
        .capture_publication_v2(&path, &scope(), RESOURCE, &token, deadline())
        .unwrap();
    assert!(fence.snapshot.observed_absence);
    let waiting = thread::spawn(move || {
        client.save_publication_and_wait_v2(
            fence,
            "untouched explicit initial intent".into(),
            &token,
        )
    });
    dispatch(&writer, next(&receiver), &cache);
    let ack = waiting.join().unwrap().unwrap();
    let captured_deadline = ack.fence.deadline;
    assert!(writer
        .client()
        .capture_publication_v2(&path, &scope(), RESOURCE, &original, deadline())
        .unwrap_err()
        .to_string()
        .contains("Register"));
    let client = writer.client();
    let cleanup = thread::spawn(move || client.remove_publication_and_wait_v2(ack, &original));
    let message = next(&receiver);
    let Message::PublicationV2(ref publication) = message else {
        panic!("same queue cleanup")
    };
    assert_eq!(publication.fence.deadline, captured_deadline);
    dispatch(&writer, message, &cache);
    cleanup.join().unwrap().unwrap();
    assert!(cache
        .as_ref()
        .unwrap()
        .draft(&scope(), RESOURCE)
        .unwrap()
        .is_none());
    finish(writer, receiver);
}

#[test]
fn pending_reviews_v2_draft_publication_expired_or_closed_before_acceptance_and_encoded_bound_change_nothing(
) {
    let (_directory, path) = fixture();
    let cache = Ok(Cache::open(&path).unwrap());
    let (mut writer, receiver) = controlled(&path);
    initial(&mut writer, &receiver, &cache);
    let token = CancellationToken::default();
    let client = writer.client();
    assert!(client
        .capture_publication_v2(
            &path,
            &scope(),
            RESOURCE,
            &token,
            Instant::now() - Duration::from_secs(1)
        )
        .is_err());
    let fence = client
        .capture_publication_v2(&path, &scope(), RESOURCE, &token, deadline())
        .unwrap();
    assert!(!client
        .save_publication_and_wait_v2(fence, "x".repeat(128 * 1024 + 1), &token)
        .unwrap_err()
        .acknowledgement_unknown());
    writer.producer.close();
    assert!(client
        .capture_publication_v2(&path, &scope(), RESOURCE, &token, deadline())
        .is_err());
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    assert_eq!(writer.producer.lock().unwrap().legacy_active, 0);
    assert_eq!(
        cache
            .as_ref()
            .unwrap()
            .draft(&scope(), RESOURCE)
            .unwrap()
            .as_deref(),
        Some("original body")
    );
    finish(writer, receiver);
}

#[test]
fn pending_reviews_v2_draft_publication_actual_older_outstanding_sql_failure_then_latest_success_cannot_publish(
) {
    let (_directory, path) = fixture();
    let cache = Ok(Cache::open(&path).unwrap());
    let (mut writer, receiver) = controlled(&path);
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute_batch("CREATE TRIGGER refuse_original_older_draft BEFORE INSERT ON drafts WHEN NEW.resource='older-failing' BEGIN SELECT RAISE(ABORT,'actual original older draft refused'); END;").unwrap();
    writer
        .save(
            scope(),
            "older-failing".into(),
            "original older body".into(),
        )
        .unwrap();
    writer
        .save(scope(), RESOURCE.into(), "exact latest success".into())
        .unwrap();
    let token = CancellationToken::default();
    let client = writer.client();
    let fence = client
        .capture_publication_v2(&path, &scope(), RESOURCE, &token, deadline())
        .unwrap();
    let waiting = thread::spawn(move || {
        client.save_publication_and_wait_v2(
            fence,
            "no durable publication reference".into(),
            &token,
        )
    });
    dispatch(&writer, next(&receiver), &cache);
    dispatch(&writer, next(&receiver), &cache);
    let failure = waiting.join().unwrap().unwrap_err();
    assert!(!failure.acknowledgement_unknown());
    assert!(failure.to_string().contains("captured outstanding"));
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    assert!(cache
        .as_ref()
        .unwrap()
        .draft(&scope(), "older-failing")
        .unwrap()
        .is_none());
    assert_eq!(
        cache
            .as_ref()
            .unwrap()
            .draft(&scope(), RESOURCE)
            .unwrap()
            .as_deref(),
        Some("exact latest success")
    );
    assert_eq!(writer.producer.lock().unwrap().legacy_active, 0);
    finish(writer, receiver);
}

#[test]
fn pending_reviews_v2_draft_publication_superseded_ticket_and_stale_completion_cannot_replace_current_capture(
) {
    let (_directory, path) = fixture();
    let cache = Ok(Cache::open(&path).unwrap());
    let (mut writer, receiver) = controlled(&path);
    initial(&mut writer, &receiver, &cache);
    let token = CancellationToken::default();
    let original = token.clone();
    let client = writer.client();
    let old = client
        .capture_publication_v2(&path, &scope(), RESOURCE, &token, deadline())
        .unwrap();
    let current = client
        .capture_publication_v2(&path, &scope(), RESOURCE, &token, deadline())
        .unwrap();
    assert!(old.snapshot.ticket < current.snapshot.ticket);
    assert_eq!(old.snapshot.revision, current.snapshot.revision);
    assert!(client
        .save_publication_and_wait_v2(old, "obsolete capture".into(), &token)
        .unwrap_err()
        .to_string()
        .contains("ticket"));
    // An already completed revision is NOT outstanding at the new cutoff.
    writer.producer.complete(
        Some(current.snapshot.revision),
        &Err("late duplicate diagnostic".into()),
    );
    let waiting = thread::spawn(move || {
        client.save_publication_and_wait_v2(current, "current actual reference".into(), &token)
    });
    dispatch(&writer, next(&receiver), &cache);
    let ack = waiting.join().unwrap().unwrap();
    ack.check_current(&writer.client(), &path, &scope(), RESOURCE, &original)
        .unwrap();
    assert_eq!(
        cache
            .as_ref()
            .unwrap()
            .draft(&scope(), RESOURCE)
            .unwrap()
            .as_deref(),
        Some("current actual reference")
    );
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    finish(writer, receiver);
}

#[test]
fn pending_reviews_v2_draft_publication_capture_slot_does_not_hijack_corpus_cutoff_but_actual_activity_retires_it(
) {
    let (_directory, path) = fixture();
    let cache = Ok(Cache::open(&path).unwrap());
    let (mut writer, receiver) = controlled(&path);
    let resource = "corpus-management:original";
    let token = CancellationToken::default();
    let corpus = writer
        .capture_corpus_preview_v4(
            scope(),
            resource.into(),
            "original corpus body".into(),
            &path,
            deadline(),
        )
        .unwrap();
    dispatch(&writer, next(&receiver), &cache);
    corpus.check_current(&token).unwrap();
    let client = writer.client();
    let first = client
        .capture_publication_v2(&path, &scope(), resource, &token, deadline())
        .unwrap();
    let second = client
        .capture_publication_v2(&path, &scope(), resource, &token, deadline())
        .unwrap();
    corpus.check_current(&token).unwrap();
    assert!(client
        .save_publication_and_wait_v2(first, "old ticket".into(), &token)
        .is_err());
    corpus.check_current(&token).unwrap();
    let waiting = thread::spawn(move || {
        client.save_publication_and_wait_v2(second, "one explicit queued reference".into(), &token)
    });
    let accepted = next(&receiver);
    assert!(corpus.check_current(&CancellationToken::default()).is_err());
    dispatch(&writer, accepted, &cache);
    waiting.join().unwrap().unwrap();
    assert!(
        corpus.check_current(&CancellationToken::default()).is_err(),
        "completion never renews an old Corpus fence"
    );
    let fresh = writer
        .capture_corpus_preview_v4(
            scope(),
            resource.into(),
            "new explicit corpus intent".into(),
            &path,
            deadline(),
        )
        .unwrap();
    dispatch(&writer, next(&receiver), &cache);
    fresh.check_current(&CancellationToken::default()).unwrap();
    finish(writer, receiver);
}

fn atomic_cleanup_ack(
    writer: &mut DraftWriter,
    receiver: &Receiver<Message>,
    cache: &Result<Cache, String>,
    path: &Path,
    token: &CancellationToken,
) -> (DraftPublicationAckV2, usize) {
    initial(writer, receiver, cache);
    let client = writer.client();
    let fence = client
        .capture_publication_v2(path, &scope(), RESOURCE, token, deadline())
        .unwrap();
    let body="{\"version\":3,\"body\":\"ORIGINAL acknowledged body 界 \\u0000\",\"publication\":{\"attempt\":7}}".to_owned();
    let allocation = body.as_ptr() as usize;
    let cancellation = token.clone();
    let waiting =
        thread::spawn(move || client.save_publication_and_wait_v2(fence, body, &cancellation));
    dispatch(writer, next(receiver), cache);
    let ack = waiting.join().unwrap().unwrap();
    assert_eq!(
        ack.saved_body.as_ptr() as usize,
        allocation,
        "the SAME accepted bounded allocation moves into ACK; no reconstructed body/history clone"
    );
    ack.check_current(&writer.client(), path, &scope(), RESOURCE, token)
        .unwrap();
    (ack, allocation)
}
fn atomic_cleanup_once(
    writer: &DraftWriter,
    receiver: &Receiver<Message>,
    cache: &Result<Cache, String>,
    ack: DraftPublicationAckV2,
    allocation: usize,
    token: &CancellationToken,
) -> Result<(), DraftPublicationErrorV2> {
    let client = writer.client();
    let cancellation = token.clone();
    let waiting = thread::spawn(move || client.remove_publication_and_wait_v2(ack, &cancellation));
    let accepted = next(receiver);
    let Message::PublicationV2(ref message) = accepted else {
        panic!("one actual accepted publication cleanup message required");
    };
    let PublicationEffectV2::Remove(ref expected) = message.effect else {
        panic!("cleanup must carry actual acknowledged body, not unconditional removal");
    };
    assert_eq!(
        expected.as_ptr() as usize,
        allocation,
        "same nonclone ACK moves the original allocation once into actual Remove"
    );
    dispatch(writer, accepted, cache);
    let outcome = waiting.join().unwrap();
    assert!(
        matches!(receiver.try_recv(), Err(mpsc::TryRecvError::Empty)),
        "no SQL/body retry or extra queue message"
    );
    assert_eq!(
        writer.producer.lock().unwrap().legacy_active,
        0,
        "actual completion, not caller exit, releases original activity"
    );
    outcome
}

#[test]
fn metadata_post_uid_cleanup_v82_narrowed_actual_queue_cutoff_preserves_acknowledged_sql_body() {
    let (_directory, path) = fixture();
    let cache = Ok(Cache::open(&path).unwrap());
    let (mut writer, receiver) = controlled(&path);
    let token = CancellationToken::default();
    let (ack, allocation) = atomic_cleanup_ack(&mut writer, &receiver, &cache, &path, &token);
    let body = cache.as_ref().unwrap().draft(&scope(), RESOURCE).unwrap();
    let original_deadline = ack.fence.deadline;
    let cutoff = Instant::now() + Duration::from_millis(750);
    assert!(cutoff < original_deadline);
    let client = writer.client();
    let cancellation = token.clone();
    let waiting =
        thread::spawn(move || client.remove_publication_before_v82(ack, &cancellation, cutoff));
    let accepted = next(&receiver);
    let Message::PublicationV2(ref message) = accepted else {
        panic!("same actual Remove queue")
    };
    assert_eq!(message.fence.deadline, cutoff);
    let PublicationEffectV2::Remove(ref actual) = message.effect else {
        panic!("original body CAS")
    };
    assert_eq!(actual.as_ptr() as usize, allocation);
    assert_eq!(writer.producer.lock().unwrap().legacy_active, 1);
    thread::sleep(cutoff.saturating_duration_since(Instant::now()) + Duration::from_millis(5));
    assert!(waiting
        .join()
        .unwrap()
        .unwrap_err()
        .acknowledgement_unknown());
    dispatch(&writer, accepted, &cache);
    assert_eq!(
        cache.as_ref().unwrap().draft(&scope(), RESOURCE).unwrap(),
        body
    );
    assert_eq!(writer.producer.lock().unwrap().legacy_active, 0);
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    finish(writer, receiver);
}

#[test]
fn metadata_post_uid_cleanup_v82_later_requested_cutoff_never_renews_original_ack() {
    let (_directory, path) = fixture();
    let cache = Ok(Cache::open(&path).unwrap());
    let (mut writer, receiver) = controlled(&path);
    let token = CancellationToken::default();
    let (ack, allocation) = atomic_cleanup_ack(&mut writer, &receiver, &cache, &path, &token);
    let original_deadline = ack.fence.deadline;
    let client = writer.client();
    let waiting = thread::spawn(move || {
        client.remove_publication_before_v82(
            ack,
            &token,
            original_deadline + Duration::from_secs(10),
        )
    });
    let accepted = next(&receiver);
    let Message::PublicationV2(ref message) = accepted else {
        panic!("same actual Remove queue")
    };
    assert_eq!(message.fence.deadline, original_deadline);
    let PublicationEffectV2::Remove(ref actual) = message.effect else {
        panic!("same acknowledged body")
    };
    assert_eq!(actual.as_ptr() as usize, allocation);
    dispatch(&writer, accepted, &cache);
    waiting.join().unwrap().unwrap();
    assert_eq!(
        cache.as_ref().unwrap().draft(&scope(), RESOURCE).unwrap(),
        None
    );
    assert_eq!(writer.producer.lock().unwrap().legacy_active, 0);
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    finish(writer, receiver);
}

#[test]
fn pending_reviews_v2_draft_publication_atomic_cleanup_external_replacement_preserves_newer_exact_bytes_despite_queue_current(
) {
    let (_directory, path) = fixture();
    let cache = Ok(Cache::open(&path).unwrap());
    let (mut writer, receiver) = controlled(&path);
    let token = CancellationToken::default();
    let (ack, allocation) = atomic_cleanup_ack(&mut writer, &receiver, &cache, &path, &token);
    let external = Cache::open(&path).unwrap();
    let replacement = "EXTERNAL newer draft 界\n\0still exact";
    external.put_draft(&scope(), RESOURCE, replacement).unwrap();
    ack.check_current(&writer.client(), &path, &scope(), RESOURCE, &token)
        .unwrap(); // genuinely queue-current, NOT SQL equality
    let error =
        atomic_cleanup_once(&writer, &receiver, &cache, ack, allocation, &token).unwrap_err();
    assert!(matches!(error, DraftPublicationErrorV2::Storage(_)));
    assert!(error.to_string().contains("changed or is absent"));
    assert_eq!(
        external.draft(&scope(), RESOURCE).unwrap().as_deref(),
        Some(replacement)
    );
    finish(writer, receiver);
}

#[test]
fn pending_reviews_v2_draft_publication_atomic_cleanup_external_deletion_is_refusal_not_fabricated_success_or_retry(
) {
    let (_directory, path) = fixture();
    let cache = Ok(Cache::open(&path).unwrap());
    let (mut writer, receiver) = controlled(&path);
    let token = CancellationToken::default();
    let (ack, allocation) = atomic_cleanup_ack(&mut writer, &receiver, &cache, &path, &token);
    let external = Cache::open(&path).unwrap();
    external.remove_draft(&scope(), RESOURCE).unwrap();
    ack.check_current(&writer.client(), &path, &scope(), RESOURCE, &token)
        .unwrap();
    assert!(matches!(
        atomic_cleanup_once(&writer, &receiver, &cache, ack, allocation, &token),
        Err(DraftPublicationErrorV2::Storage(_))
    ));
    assert!(external.draft(&scope(), RESOURCE).unwrap().is_none());
    finish(writer, receiver);
}

#[test]
fn pending_reviews_v2_draft_publication_atomic_cleanup_unchanged_body_deletes_exactly_owned_row_other_principal_host_and_resource_untouched(
) {
    let (_directory, path) = fixture();
    let cache = Ok(Cache::open(&path).unwrap());
    let (mut writer, receiver) = controlled(&path);
    let token = CancellationToken::default();
    let (ack, allocation) = atomic_cleanup_ack(&mut writer, &receiver, &cache, &path, &token);
    let same_body = ack.saved_body.as_str();
    let other_uid = Scope::github("github.invalid", Some(43)).unwrap();
    let other_host = Scope::github("other.invalid", Some(42)).unwrap();
    let db = cache.as_ref().unwrap();
    db.put_draft(&other_uid, RESOURCE, same_body).unwrap();
    db.put_draft(&other_host, RESOURCE, same_body).unwrap();
    db.put_draft(&scope(), "other-exact-resource", same_body)
        .unwrap();
    let expected = same_body.to_owned(); // independent oracle only; production ACK/Remove allocation remains unchanged
    atomic_cleanup_once(&writer, &receiver, &cache, ack, allocation, &token).unwrap();
    assert!(db.draft(&scope(), RESOURCE).unwrap().is_none());
    for (scope, resource) in [
        (&other_uid, RESOURCE),
        (&other_host, RESOURCE),
        (&scope(), "other-exact-resource"),
    ] {
        assert_eq!(
            db.draft(scope, resource).unwrap().as_deref(),
            Some(expected.as_str())
        );
    }
    let independently_observed: i64 =
        rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap()
            .query_row("SELECT count(*) FROM drafts", [], |row| row.get(0))
            .unwrap();
    assert_eq!(independently_observed, 3);
    finish(writer, receiver);
}
