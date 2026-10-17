use super::*;
use crate::{
    drafts::{DraftWriter, Message, PendingDraftV4},
    storage::{Cache, Scope},
};
use std::{collections::BTreeMap, path::PathBuf, sync::mpsc, thread};

fn private_sql_directory_v26() -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::Builder::new()
        .prefix("ghtui-close-cutoff-sql-v26-")
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let metadata = std::fs::symlink_metadata(directory.path()).unwrap();
    assert!(metadata.is_dir());
    assert_eq!(metadata.permissions().mode() & 0o777, 0o700);
    directory
}

fn scope() -> Scope {
    Scope::github("github.invalid", Some(42)).unwrap()
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(3)
}
fn controlled() -> (DraftWriter, mpsc::Receiver<Message>) {
    let (sender, receiver) = mpsc::sync_channel(8);
    let (_, acks) = mpsc::channel();
    (
        DraftWriter {
            sender,
            acks,
            worker: None,
            pending: BTreeMap::<_, PendingDraftV4>::new(),
            producer: Arc::new(DraftProducerV4::new(PathBuf::from("fixed.sqlite"))),
        },
        receiver,
    )
}
fn capture(writer: &DraftWriter, token: &CancellationToken) -> ConversationOpenReadCutoffV20 {
    writer
        .client()
        .capture_conversation_open_cutoff_v20(Path::new("fixed.sqlite"), token, deadline())
        .unwrap()
}
fn ack(
    writer: &DraftWriter,
    receiver: &mpsc::Receiver<Message>,
    result: Result<(), String>,
) -> u64 {
    let Message::Save(_, _, _, Some(revision)) =
        receiver.recv_timeout(Duration::from_secs(1)).unwrap()
    else {
        panic!("Actual accepted Save revision")
    };
    writer.producer.complete(Some(revision), &result);
    revision
}
fn finish(mut writer: DraftWriter, receiver: mpsc::Receiver<Message>) {
    writer.pending.clear();
    drop(receiver);
    drop(writer);
}

#[test]
fn conversation_comments_v20_cutoff_virgin_capture_has_no_send_revision_epoch_or_activity_effect() {
    let (writer, receiver) = controlled();
    let token = CancellationToken::default();
    let cutoff = capture(&writer, &token);
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    let state = writer.producer.lock().unwrap();
    assert_eq!(
        (state.revision, state.epoch, state.legacy_active),
        (0, 0, 0)
    );
    assert!(state.outstanding.is_empty());
    drop(state);
    writer
        .client()
        .wait_conversation_open_cutoff_v20(&cutoff, &token)
        .unwrap();
    drop(cutoff);
    finish(writer, receiver);
}

#[test]
fn conversation_comments_v20_occupied_read_capture_refuses_without_superseding_original_ticket() {
    let (writer, receiver) = controlled();
    let token = CancellationToken::default();
    let cutoff = capture(&writer, &token);
    assert!(writer
        .client()
        .capture_conversation_open_cutoff_v20(Path::new("fixed.sqlite"), &token, deadline())
        .is_err());
    cutoff.check(&writer.client(), &token).unwrap();
    drop(cutoff);
    let next = capture(&writer, &token);
    next.check(&writer.client(), &token).unwrap();
    drop(next);
    finish(writer, receiver);
}

#[test]
fn conversation_comments_v20_all_captured_outstanding_failures_poison_not_only_latest() {
    let (mut writer, receiver) = controlled();
    writer.save(scope(), "older".into(), "old".into()).unwrap();
    writer.save(scope(), "latest".into(), "new".into()).unwrap();
    let token = CancellationToken::default();
    let cutoff = capture(&writer, &token);
    ack(&writer, &receiver, Err("actual storage refusal".into()));
    ack(&writer, &receiver, Ok(()));
    assert_eq!(
        writer
            .client()
            .wait_conversation_open_cutoff_v20(&cutoff, &token),
        Err(ReadCutoffFailureV20::SaveFailed)
    );
    drop(cutoff);
    finish(writer, receiver);
}

#[test]
fn conversation_comments_v20_duplicate_stale_completion_cannot_poison_read_ticket() {
    let (mut writer, receiver) = controlled();
    writer.save(scope(), "older".into(), "old".into()).unwrap();
    let old = ack(&writer, &receiver, Ok(()));
    writer.save(scope(), "latest".into(), "new".into()).unwrap();
    let token = CancellationToken::default();
    let cutoff = capture(&writer, &token);
    writer
        .producer
        .complete(Some(old), &Err("stale duplicate".into()));
    ack(&writer, &receiver, Ok(()));
    writer
        .client()
        .wait_conversation_open_cutoff_v20(&cutoff, &token)
        .unwrap();
    drop(cutoff);
    finish(writer, receiver);
}

#[test]
fn conversation_comments_v20_new_revision_transfer_and_legacy_activity_retire_only_read_cutoff() {
    for mode in 0..3 {
        let (mut writer, receiver) = controlled();
        writer
            .save(scope(), "original".into(), "body".into())
            .unwrap();
        let token = CancellationToken::default();
        let cutoff = capture(&writer, &token);
        let activity = match mode {
            0 => {
                writer
                    .save(scope(), "newer".into(), "newer".into())
                    .unwrap();
                None
            }
            1 => {
                writer.take_pending();
                None
            }
            _ => Some(writer.producer.begin_legacy().unwrap()),
        };
        assert_eq!(
            cutoff.check(&writer.client(), &token),
            Err(ReadCutoffFailureV20::Unavailable)
        );
        drop(activity);
        drop(cutoff);
        finish(writer, receiver);
    }
}

#[test]
fn conversation_comments_v20_wrong_client_database_token_cancel_expiry_and_closed_refuse() {
    let (writer, receiver) = controlled();
    let (other, other_receiver) = controlled();
    let token = CancellationToken::default();
    assert!(writer
        .client()
        .capture_conversation_open_cutoff_v20(Path::new("other.sqlite"), &token, deadline())
        .is_err());
    assert!(writer
        .client()
        .capture_conversation_open_cutoff_v20(Path::new("fixed.sqlite"), &token, Instant::now())
        .is_err());
    let cutoff = capture(&writer, &token);
    assert_eq!(
        cutoff.check(&other.client(), &token),
        Err(ReadCutoffFailureV20::Unavailable)
    );
    assert_eq!(
        cutoff.check(&writer.client(), &CancellationToken::default()),
        Err(ReadCutoffFailureV20::Unavailable)
    );
    token.cancel();
    assert_eq!(
        cutoff.check(&writer.client(), &token),
        Err(ReadCutoffFailureV20::Unavailable)
    );
    drop(cutoff);
    writer.producer.close();
    assert!(writer
        .client()
        .capture_conversation_open_cutoff_v20(
            Path::new("fixed.sqlite"),
            &CancellationToken::default(),
            deadline()
        )
        .is_err());
    finish(writer, receiver);
    finish(other, other_receiver);
}

#[test]
fn conversation_comments_v20_read_capture_does_not_replace_actual_corpus_or_publication_wait() {
    let directory = private_sql_directory_v26();
    let path = directory.path().join("same-original.sqlite");
    let mut writer = DraftWriter::new(path.clone());
    let corpus = writer
        .capture_corpus_preview_v4(
            scope(),
            "corpus-management:actual".into(),
            "actual body".into(),
            &path,
            deadline(),
        )
        .unwrap();
    let token = CancellationToken::default();
    writer
        .client()
        .wait_corpus_preview_v4(&corpus, &token)
        .unwrap();
    let publication = writer
        .client()
        .capture_publication_v2(
            &path,
            &scope(),
            "corpus-management:actual",
            &token,
            deadline(),
        )
        .unwrap();
    let cutoff = writer
        .client()
        .capture_conversation_open_cutoff_v20(&path, &token, deadline())
        .unwrap();
    corpus.check_current(&token).unwrap();
    writer
        .client()
        .wait_conversation_open_cutoff_v20(&cutoff, &token)
        .unwrap();
    drop(cutoff);
    // A genuine publication wait accepts the SAME pre-read ticket; no new
    // publication capture was issued, and no read helper enters its activity.
    writer
        .client()
        .save_publication_and_wait_v2(publication, "actual body".into(), &token)
        .unwrap();
    assert_eq!(
        Cache::open(&path)
            .unwrap()
            .draft(&scope(), "corpus-management:actual")
            .unwrap()
            .as_deref(),
        Some("actual body")
    );
}

#[test]
fn conversation_comments_v20_actual_held_sql_save_and_coalescer_settle_before_read_without_replay()
{
    let directory = private_sql_directory_v26();
    let path = directory.path().join("draft.sqlite");
    let cache = Cache::open(&path).unwrap();
    cache.put_draft(&scope(), "original", "seed").unwrap();
    drop(cache);
    let mut writer = DraftWriter::new(path.clone());
    writer
        .client()
        .save_and_wait(scope(), "initialize".into(), "ready".into())
        .unwrap();
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute_batch("BEGIN IMMEDIATE; UPDATE drafts SET body=body;")
        .unwrap();
    for index in 0..20 {
        writer
            .save(scope(), format!("key-{index}"), format!("accepted-{index}"))
            .unwrap();
    }
    assert!(!writer.pending.is_empty());
    let token = CancellationToken::default();
    let client = writer.client();
    let cutoff = client
        .capture_conversation_open_cutoff_v20(&path, &token, deadline())
        .unwrap();
    let (send, receive) = mpsc::sync_channel(1);
    let worker = thread::spawn(move || {
        send.send(client.wait_conversation_open_cutoff_v20(&cutoff, &token))
            .unwrap()
    });
    assert!(matches!(receive.try_recv(), Err(mpsc::TryRecvError::Empty)));
    assert!(writer.producer.lock().unwrap().outstanding.len() >= 20);
    sql.execute_batch("COMMIT;").unwrap();
    let until = deadline();
    while !writer.pending.is_empty() {
        writer.poll();
        assert!(Instant::now() < until);
        thread::yield_now();
    }
    receive
        .recv_timeout(Duration::from_secs(3))
        .unwrap()
        .unwrap();
    worker.join().unwrap();
    let cache = Cache::open(&path).unwrap();
    for index in 0..20 {
        assert_eq!(
            cache.draft(&scope(), &format!("key-{index}")).unwrap(),
            Some(format!("accepted-{index}"))
        );
    }
    assert!(writer.producer.lock().unwrap().outstanding.is_empty());
}

#[test]
fn conversation_comments_v20_actual_failed_sql_save_is_typed_local_not_generic_provider_failure() {
    let directory = private_sql_directory_v26();
    let path = directory.path().join("draft.sqlite");
    drop(Cache::open(&path).unwrap());
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute_batch("CREATE TRIGGER refuse_save BEFORE INSERT ON drafts BEGIN SELECT RAISE(ABORT,'actual local save failure'); END;").unwrap();
    let mut writer = DraftWriter::new(path.clone());
    writer
        .save(scope(), "actual".into(), "retained by original Form".into())
        .unwrap();
    let token = CancellationToken::default();
    let client = writer.client();
    let cutoff = client
        .capture_conversation_open_cutoff_v20(&path, &token, deadline())
        .unwrap();
    assert_eq!(
        client.wait_conversation_open_cutoff_v20(&cutoff, &token),
        Err(ReadCutoffFailureV20::SaveFailed)
    );
    assert_eq!(
        sql.query_row("SELECT count(*) FROM drafts", [], |r| r.get::<_, u64>(0))
            .unwrap(),
        0
    );
}
