//! Exact queue ownership plus real SQLite ordering; no metadata/SQL retries.
use super::*;
use crate::providers::runner::CancellationToken;
use std::{
    fs,
    sync::mpsc,
    time::{Duration, Instant},
};

const RESOURCE: &str = "corpus-management:2f6f726967696e616c";
fn scope() -> Scope {
    Scope::github("github.invalid", Some(42)).unwrap()
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(3)
}
fn controlled() -> (DraftWriter, Receiver<Message>) {
    let (sender, receiver) = mpsc::sync_channel(8);
    let (_, acks) = mpsc::channel();
    (
        DraftWriter {
            sender,
            acks,
            worker: None,
            pending: BTreeMap::new(),
            producer: Arc::new(DraftProducerV4::new(PathBuf::from("fixed.sqlite"))),
        },
        receiver,
    )
}
fn capture(writer: &mut DraftWriter) -> Arc<CorpusDraftFenceV4> {
    writer
        .capture_corpus_preview_v4(
            scope(),
            RESOURCE.into(),
            "current body".into(),
            std::path::Path::new("fixed.sqlite"),
            deadline(),
        )
        .unwrap()
}
fn acknowledge(writer: &DraftWriter, receiver: &Receiver<Message>) -> (u64, String) {
    let Message::Save(_, _, body, Some(revision)) =
        receiver.recv_timeout(Duration::from_secs(1)).unwrap()
    else {
        panic!("ordinary accepted save")
    };
    writer.producer.complete(Some(revision), &Ok(()));
    (revision, body)
}
fn finish(mut writer: DraftWriter, receiver: Receiver<Message>) {
    // Controlled unit fixture drains accepted messages before ordinary Drop.
    writer.pending.clear();
    drop(receiver);
    drop(writer);
}

#[test]
fn cache_storage_corpus_fence_v4_actual_sqlite_held_writer_drains_all_prior_without_replaying_body()
{
    let directory = tempfile::Builder::new()
        .permissions(std::os::unix::fs::PermissionsExt::from_mode(0o700))
        .tempdir()
        .unwrap();
    let path = directory.path().join("cache.sqlite");
    let cache = Cache::open(&path).unwrap();
    cache.put_draft(&scope(), RESOURCE, "seed").unwrap();
    drop(cache);
    let mut writer = DraftWriter::new(path.clone());
    writer
        .client()
        .save_and_wait(scope(), "initialize".into(), "ready".into())
        .unwrap();
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute_batch("BEGIN IMMEDIATE; UPDATE drafts SET body='held real SQL transaction';")
        .unwrap();
    for index in 0..16 {
        writer
            .save(
                scope(),
                format!("older-{index}"),
                format!("accepted older {index}"),
            )
            .unwrap();
    }
    let fence = writer
        .capture_corpus_preview_v4(
            scope(),
            RESOURCE.into(),
            "exact current body".into(),
            &path,
            deadline(),
        )
        .unwrap();
    assert!(!writer.pending.is_empty());
    assert!(writer.producer.lock().unwrap().outstanding.len() <= 25);
    let client = writer.client();
    let token = CancellationToken::default();
    let (reply, result) = mpsc::sync_channel(1);
    let waiting_fence = fence.clone();
    let worker = thread::spawn(move || {
        reply
            .send(client.wait_corpus_preview_v4(&waiting_fence, &token))
            .unwrap()
    });
    assert!(matches!(result.try_recv(), Err(mpsc::TryRecvError::Empty)));
    assert!(
        path.with_file_name("cache.sqlite-journal")
            .metadata()
            .unwrap()
            .len()
            > 0
    );
    sql.execute_batch("COMMIT;").unwrap();
    let end = deadline();
    while !writer.pending.is_empty() {
        writer.poll();
        assert!(
            Instant::now() < end,
            "existing UI poll must transfer original revisions"
        );
        thread::yield_now();
    }
    result
        .recv_timeout(Duration::from_secs(3))
        .unwrap()
        .unwrap();
    worker.join().unwrap();
    fence.check_current(&CancellationToken::default()).unwrap();
    assert!(writer.producer.lock().unwrap().outstanding.is_empty());
    let cache = Cache::open(&path).unwrap();
    assert_eq!(
        cache.draft(&scope(), RESOURCE).unwrap().as_deref(),
        Some("exact current body")
    );
    for index in 0..16 {
        assert_eq!(
            cache
                .draft(&scope(), &format!("older-{index}"))
                .unwrap()
                .as_deref(),
            Some(format!("accepted older {index}").as_str())
        );
    }
    drop(writer);
    assert_eq!(
        cache
            .draft(
                &Scope::github("github.invalid", Some(43)).unwrap(),
                RESOURCE
            )
            .unwrap(),
        None
    );
}

#[test]
fn cache_storage_corpus_fence_v4_captured_older_keys_wait_and_precompleted_ack_has_no_lost_wakeup()
{
    let (mut writer, receiver) = controlled();
    writer
        .save(scope(), "older unrelated key".into(), "older".into())
        .unwrap();
    let fence = capture(&mut writer);
    let Message::Save(_, _, _, Some(old)) = receiver.recv().unwrap() else {
        panic!()
    };
    let Message::Save(_, _, _, Some(current)) = receiver.recv().unwrap() else {
        panic!()
    };
    // Even current completion cannot satisfy the older outstanding cutoff.
    writer.producer.complete(Some(current), &Ok(()));
    assert!(writer.producer.lock().unwrap().outstanding.contains(&old));
    writer.producer.complete(Some(old), &Ok(()));
    writer
        .client()
        .wait_corpus_preview_v4(&fence, &CancellationToken::default())
        .unwrap();
    finish(writer, receiver);
}

#[test]
fn cache_storage_corpus_fence_v4_pending_transfer_preserves_revision_and_superseded_body_never_overwrites_newer(
) {
    let (mut writer, receiver) = controlled();
    for index in 0..8 {
        writer
            .save(scope(), format!("queued-{index}"), index.to_string())
            .unwrap();
    }
    writer
        .save(scope(), RESOURCE.into(), "old coalesced".into())
        .unwrap();
    let old = writer.pending.values().next().unwrap().revision;
    acknowledge(&writer, &receiver);
    let fence = capture(&mut writer); // queue now has room, supersedes pending body
    assert!(writer.pending.is_empty());
    assert!(!writer.producer.lock().unwrap().outstanding.contains(&old));
    let mut bodies = Vec::new();
    while !writer.producer.lock().unwrap().outstanding.is_empty() {
        bodies.push(acknowledge(&writer, &receiver).1);
    }
    assert!(!bodies.iter().any(|body| body == "old coalesced"));
    writer
        .client()
        .wait_corpus_preview_v4(&fence, &CancellationToken::default())
        .unwrap();
    finish(writer, receiver);

    let (mut writer, receiver) = controlled();
    for index in 0..8 {
        writer
            .save(scope(), format!("queued-{index}"), index.to_string())
            .unwrap();
    }
    let fence = capture(&mut writer);
    let original = writer.pending.values().next().unwrap().revision;
    acknowledge(&writer, &receiver);
    writer.poll();
    assert!(writer.pending.is_empty());
    let mut transferred = None;
    while !writer.producer.lock().unwrap().outstanding.is_empty() {
        let (revision, body) = acknowledge(&writer, &receiver);
        if body == "current body" {
            transferred = Some(revision);
        }
    }
    assert_eq!(transferred, Some(original));
    writer
        .client()
        .wait_corpus_preview_v4(&fence, &CancellationToken::default())
        .unwrap();
    finish(writer, receiver);
}

#[test]
fn cache_storage_corpus_fence_v4_full_coalescer_is_bounded_and_refused_capture_never_rebases_latest(
) {
    let (mut writer, receiver) = controlled();
    for index in 0..24 {
        writer
            .save(scope(), format!("key-{index}"), index.to_string())
            .unwrap();
    }
    assert_eq!(writer.pending.len(), 16);
    let revision = writer.producer.lock().unwrap().revision;
    assert!(writer
        .capture_corpus_preview_v4(
            scope(),
            RESOURCE.into(),
            "not accepted".into(),
            std::path::Path::new("fixed.sqlite"),
            deadline()
        )
        .unwrap_err()
        .contains("full"));
    assert_eq!(writer.producer.lock().unwrap().revision, revision);
    assert_eq!(writer.producer.lock().unwrap().outstanding.len(), 24);
    assert_eq!(writer.pending.len(), 16);
    finish(writer, receiver);
}

#[test]
fn cache_storage_corpus_fence_v4_cancel_expiry_shutdown_foreign_producer_and_numeric_database_refuse(
) {
    let (mut writer, receiver) = controlled();
    let fence = capture(&mut writer);
    let token = CancellationToken::default();
    token.cancel();
    assert!(writer
        .client()
        .wait_corpus_preview_v4(&fence, &token)
        .unwrap_err()
        .contains("cancelled"));
    let (foreign, foreign_receiver) = controlled();
    assert!(foreign
        .client()
        .wait_corpus_preview_v4(&fence, &CancellationToken::default())
        .unwrap_err()
        .contains("another draft producer"));
    let revision = writer.producer.lock().unwrap().revision;
    for (account, database, deadline) in [
        (
            Scope::new("github.invalid", "alice").unwrap(),
            "fixed.sqlite",
            deadline(),
        ),
        (scope(), "different.sqlite", deadline()),
        (scope(), "fixed.sqlite", Instant::now()),
    ] {
        assert!(writer
            .capture_corpus_preview_v4(
                account,
                RESOURCE.into(),
                "bad".into(),
                std::path::Path::new(database),
                deadline
            )
            .is_err());
    }
    assert_eq!(writer.producer.lock().unwrap().revision, revision);
    writer.producer.close();
    assert!(writer
        .client()
        .wait_corpus_preview_v4(&fence, &CancellationToken::default())
        .unwrap_err()
        .contains("closed"));
    finish(writer, receiver);
    finish(foreign, foreign_receiver);

    let (mut writer, receiver) = controlled();
    let fence = writer
        .capture_corpus_preview_v4(
            scope(),
            RESOURCE.into(),
            "body".into(),
            std::path::Path::new("fixed.sqlite"),
            Instant::now() + Duration::from_millis(30),
        )
        .unwrap();
    assert!(writer
        .client()
        .wait_corpus_preview_v4(&fence, &CancellationToken::default())
        .unwrap_err()
        .contains("deadline expired"));
    finish(writer, receiver);
}

#[test]
fn cache_storage_corpus_fence_v4_failure_supersession_transfer_and_disconnection_never_publish() {
    let (mut writer, receiver) = controlled();
    let fence = capture(&mut writer);
    let Message::Save(_, _, _, revision) = receiver.recv().unwrap() else {
        panic!()
    };
    writer
        .producer
        .complete(revision, &Err("actual storage failure".into()));
    assert!(writer
        .client()
        .wait_corpus_preview_v4(&fence, &CancellationToken::default())
        .unwrap_err()
        .contains("actual storage failure"));
    let fence = capture(&mut writer);
    writer
        .save(scope(), RESOURCE.into(), "newer intent".into())
        .unwrap();
    assert!(fence
        .check_current(&CancellationToken::default())
        .unwrap_err()
        .contains("superseded"));
    let fence = capture(&mut writer);
    let transferred = writer.take_pending();
    assert!(transferred.is_empty());
    assert!(fence
        .check_current(&CancellationToken::default())
        .unwrap_err()
        .contains("transferred"));
    finish(writer, receiver);
    let (mut writer, receiver) = controlled();
    drop(receiver);
    assert!(writer
        .capture_corpus_preview_v4(
            scope(),
            RESOURCE.into(),
            "body".into(),
            std::path::Path::new("fixed.sqlite"),
            deadline()
        )
        .unwrap_err()
        .contains("unavailable"));
    drop(writer);
}

#[test]
fn cache_storage_corpus_fence_v4_actual_unavailable_storage_is_acknowledged_failure_not_ready() {
    let directory = tempfile::tempdir().unwrap();
    let mut writer = DraftWriter::new(directory.path().to_owned());
    let fence = writer
        .capture_corpus_preview_v4(
            scope(),
            RESOURCE.into(),
            "body".into(),
            directory.path(),
            deadline(),
        )
        .unwrap();
    assert!(writer
        .client()
        .wait_corpus_preview_v4(&fence, &CancellationToken::default())
        .is_err());
    assert!(fs::read_dir(directory.path()).unwrap().next().is_none());
}

#[test]
fn cache_storage_corpus_fence_v4_older_failure_and_newer_unrelated_intent_are_not_silent_success() {
    let (mut writer, receiver) = controlled();
    writer
        .save(scope(), "older original key".into(), "older".into())
        .unwrap();
    let fence = capture(&mut writer);
    let Message::Save(_, _, _, Some(older)) = receiver.recv().unwrap() else {
        panic!()
    };
    acknowledge(&writer, &receiver);
    writer
        .producer
        .complete(Some(older), &Err("older accepted write failed".into()));
    assert!(writer
        .client()
        .wait_corpus_preview_v4(&fence, &CancellationToken::default())
        .unwrap_err()
        .contains("older accepted write failed"));
    let fence = capture(&mut writer);
    acknowledge(&writer, &receiver);
    writer
        .client()
        .wait_corpus_preview_v4(&fence, &CancellationToken::default())
        .unwrap();
    assert!(
        matches!(receiver.try_recv(), Err(mpsc::TryRecvError::Empty)),
        "wait performs no body replay or queue send"
    );
    writer
        .save(
            Scope::github("github.invalid", Some(43)).unwrap(),
            "unrelated future intent".into(),
            "must not be drained by original worker".into(),
        )
        .unwrap();
    assert!(fence
        .check_current(&CancellationToken::default())
        .unwrap_err()
        .contains("superseded"));
    finish(writer, receiver);
}

#[test]
fn cache_storage_corpus_fence_v4_actual_writer_drop_wakes_original_client_and_cancellation_is_bounded(
) {
    let (mut writer, receiver) = controlled();
    let fence = capture(&mut writer);
    let client = writer.client();
    let _uncompleted_actual_message = receiver.recv().unwrap();
    drop(writer); // real Drop closes the producer before sending Shutdown
    assert!(client
        .wait_corpus_preview_v4(&fence, &CancellationToken::default())
        .unwrap_err()
        .contains("closed"));
    assert!(matches!(receiver.recv().unwrap(), Message::Shutdown));

    let (mut writer, receiver) = controlled();
    let fence = capture(&mut writer);
    let token = CancellationToken::default();
    let cancel = token.clone();
    let client = writer.client();
    let (entered, entry) = mpsc::sync_channel(1);
    let waiting = thread::spawn(move || {
        entered.send(()).unwrap();
        client.wait_corpus_preview_v4(&fence, &token)
    });
    entry.recv_timeout(Duration::from_secs(1)).unwrap();
    let start = Instant::now();
    cancel.cancel();
    assert!(waiting.join().unwrap().unwrap_err().contains("cancelled"));
    assert!(
        start.elapsed() < Duration::from_millis(250),
        "no indefinite send/recv while cancelled"
    );
    finish(writer, receiver);
}

#[test]
fn cache_storage_corpus_fence_v4_actual_blocked_legacy_publication_excludes_new_cutoff() {
    let (mut writer, receiver) = controlled();
    let client = writer.client();
    let publication = thread::spawn(move || {
        client.save_and_wait(
            scope(),
            "foreign original publication".into(),
            "original durable attempt".into(),
        )
    });
    let Message::SaveAndWait(_, _, body, reply) =
        receiver.recv_timeout(Duration::from_secs(1)).unwrap()
    else {
        panic!("actual legacy ordered queue message")
    };
    assert_eq!(body, "original durable attempt");
    // Receipt withheld: genuine in-flight call, not a scheduler-delay guess.
    let refused = writer.capture_corpus_preview_v4(
        scope(),
        RESOURCE.into(),
        "new body".into(),
        std::path::Path::new("fixed.sqlite"),
        deadline(),
    );
    reply.send(Ok(())).unwrap();
    publication.join().unwrap().unwrap();
    assert!(
        refused.is_err(),
        "must not capture an original cutoff during a still-active legacy publication"
    );
    let fresh = capture(&mut writer);
    acknowledge(&writer, &receiver);
    writer
        .client()
        .wait_corpus_preview_v4(&fresh, &CancellationToken::default())
        .unwrap();
    finish(writer, receiver);
}

#[test]
fn cache_storage_corpus_fence_v4_legacy_full_batch_exit_notifies_and_activity_bound_fails_closed() {
    let (mut writer, receiver) = controlled();
    for index in 0..7 {
        writer
            .save(scope(), format!("full-{index}"), "older accepted".into())
            .unwrap();
    }
    let original = capture(&mut writer);
    let client = writer.client();
    let batch = thread::spawn(move || {
        client.save_pending(vec![
            (
                scope(),
                "legacy original".into(),
                "first original body".into(),
            ),
            (
                scope(),
                "legacy original".into(),
                "second original body".into(),
            ),
        ])
    });
    let end = deadline();
    while writer.producer.lock().unwrap().legacy_active == 0 {
        assert!(Instant::now() < end);
        thread::yield_now();
    }
    assert!(original
        .check_current(&CancellationToken::default())
        .is_err());
    let refused = writer.capture_corpus_preview_v4(
        scope(),
        RESOURCE.into(),
        "must not be accepted".into(),
        std::path::Path::new("fixed.sqlite"),
        deadline(),
    );
    assert!(refused.unwrap_err().contains("still active"));
    assert!(writer.pending.is_empty());
    // Disconnect the ACTUAL full queue. RAII must release the in-flight batch
    // guard on its error path rather than stranding an acknowledgement waiter.
    drop(receiver);
    assert!(batch.join().unwrap().unwrap_err().contains("closed"));
    assert_eq!(writer.producer.lock().unwrap().legacy_active, 0);
    drop(writer);

    let (mut writer, receiver) = controlled();
    let guards = (0..25)
        .map(|_| writer.producer.begin_legacy().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(writer.producer.lock().unwrap().legacy_active, 25);
    assert!(writer.producer.begin_legacy().is_err());
    assert!(writer
        .capture_corpus_preview_v4(
            scope(),
            RESOURCE.into(),
            "closed".into(),
            std::path::Path::new("fixed.sqlite"),
            deadline()
        )
        .is_err());
    drop(guards);
    assert_eq!(writer.producer.lock().unwrap().legacy_active, 0);
    finish(writer, receiver);
}
