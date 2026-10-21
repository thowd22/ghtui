//! Genuine original producer, actual strict transport and accepted same-queue
//! messages. No fabricated Source, prepared projection, close seal or receipt.
use super::*;
use crate::{
    forms::Form,
    personal_pr_entry_v13::tests::collaboration_v17::protocol::{self, publication as actual},
    pr_actions::{PrCommand, PrPlan},
    scoped_pr_collaboration_plan_v17::{ConversationPublicationStateV17 as State, DraftCleanupV85},
    scoped_pr_collaboration_ui_v17 as ui,
};
use std::thread;

#[derive(Clone, Copy, Debug)]
enum Fault {
    None,
    RawReceipt,
    Clear,
    DeleteTrigger,
    ProjectionTrigger,
    NewProducerEpoch,
    LostReply,
    CancelBeforeCommit,
    LateAckCancel,
}
fn controlled(path: &Path) -> (DraftWriter, mpsc::Receiver<Message>) {
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
fn next(receiver: &mpsc::Receiver<Message>) -> Message {
    receiver
        .recv_timeout(Duration::from_secs(8))
        .expect("Actual bounded draft queue message")
}
fn dispatch(
    writer: &DraftWriter,
    message: Message,
    cache: Result<Cache, String>,
) -> Result<Cache, String> {
    let producer = writer.producer.clone();
    thread::spawn(move || {
        producer.publication_capacity_v2();
        match message {
            Message::PublicationV2(message) => message.run(&cache),
            Message::Save(scope, resource, body, revision) => {
                let result = cache.as_ref().map_err(Clone::clone).and_then(|cache| {
                    cache
                        .put_draft(&scope, &resource, &body)
                        .map_err(|e| e.to_string())
                });
                producer.complete(revision, &result);
                result.unwrap();
            }
            _ => panic!("Only actual editing Save or closed publication expected"),
        }
        cache
    })
    .join()
    .unwrap()
}
fn current_body(s: &crate::personal_pr_entry_v13::tests::Setup, form: &Form) -> Option<String> {
    Cache::open(&crate::personal_pr_entry_v13::tests::database(&s.fixture))
        .unwrap()
        .draft(&form.scope(), &form.resource())
        .unwrap()
}
fn actual_case(fault: Fault) {
    fn send<T: Send>() {}
    send::<AtomicProjectionV85>();
    send::<PublicationMessageV2>();
    let command = PrCommand::NewComment;
    let mut tail = protocol::phase(command, false, Some(true));
    tail.extend(actual::receipts(command));
    tail.extend(actual::visibility());
    let (s, form, preview) = actual::prepared(command, tail);
    let path = crate::personal_pr_entry_v13::tests::database(&s.fixture);
    let (mut writer, receiver) = controlled(&path);
    let mut cache = Ok(Cache::open(&path).unwrap());
    writer
        .save(
            form.scope(),
            form.resource(),
            ui::encoded_draft(&form).unwrap(),
        )
        .unwrap();
    cache = dispatch(&writer, next(&receiver), cache);
    let client = writer.client();
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let fence = client
        .capture_publication_v2(&path, &form.scope(), &form.resource(), &cancel, deadline)
        .unwrap();
    let api = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    thread::scope(|scope| {
        let api = &api;
        let client = &client;
        let path = &path;
        let preview = &preview;
        let token = &cancel;
        let publisher = scope.spawn(move || {
            PrPlan::execute_original_conversation_v17(
                preview, api, path, client, fence, token, deadline,
            )
            .unwrap()
        });
        // The real publisher first demands its actual reference Save ACK.
        let saved = next(&receiver);
        assert!(
            matches!(&saved,Message::PublicationV2(message) if matches!(&message.effect,PublicationEffectV2::Save(_)))
        );
        cache = dispatch(&writer, saved, cache);
        let Message::PublicationV2(mut message) = next(&receiver) else {
            panic!("Actual atomic publication expected")
        };
        assert!(matches!(
            &message.effect,
            PublicationEffectV2::CommitConversation(_)
        ));
        actual::exact_protocol(&s); // every original child, including finalUID, matched
        let body =
            current_body(&s, &form).expect("Protected draft exists before atomic transaction");
        let before_body = body.clone();
        let db = rusqlite::Connection::open(path).unwrap();
        match fault {
            Fault::RawReceipt=>assert_eq!(db.execute("UPDATE pr_write_attempts SET confirmed_json='{}'",[]).unwrap(),1),
            Fault::Clear=>Cache::open(path).unwrap().clear_records(&form.scope()).unwrap(),
            Fault::DeleteTrigger=>db.execute_batch("CREATE TRIGGER atomic_v85_delete_refusal BEFORE DELETE ON drafts WHEN OLD.resource LIKE 'conversation-draft-v17/%' BEGIN SELECT RAISE(ABORT,'atomic exact DELETE refusal');END;").unwrap(),
            Fault::ProjectionTrigger=>db.execute_batch("CREATE TRIGGER atomic_v85_row_refusal BEFORE INSERT ON records WHEN NEW.kind='collaboration-document-v1' BEGIN SELECT RAISE(ABORT,'atomic row refusal');END;").unwrap(),
            Fault::NewProducerEpoch=>{
                writer.save(form.scope(),form.resource(),body).unwrap();
                cache=dispatch(&writer,next(&receiver),cache);
                assert_eq!(current_body(&s,&form).as_deref(),Some(before_body.as_str()));
            }
            Fault::CancelBeforeCommit=>cancel.cancel(),
            Fault::None|Fault::LostReply|Fault::LateAckCancel=>{},
        }
        let before = actual::refresh_v19::exact_sql(&s);
        let (proxy, observed) = mpsc::sync_channel(1);
        let PublicationReplyV85::Atomic(original) =
            std::mem::replace(&mut message.reply, PublicationReplyV85::Atomic(proxy))
        else {
            panic!("Actual atomic reply required")
        };
        cache = dispatch(&writer, Message::PublicationV2(message), cache);
        assert!(
            cache.is_ok(),
            "The actual queue worker retained its owned Cache"
        );
        let result = observed.recv_timeout(Duration::from_secs(8)).unwrap();
        if matches!(fault, Fault::None | Fault::LostReply | Fault::LateAckCancel) {
            assert!(result.is_ok());
            assert_eq!(current_body(&s, &form), None);
            assert_ne!(actual::refresh_v19::exact_sql(&s), before);
        } else {
            assert!(matches!(
                &result,
                Err(AtomicPublicationFailureV85::Retained(_))
            ));
            assert_eq!(actual::refresh_v19::exact_sql(&s),before,
                "Raw CAS/Clear/trigger/currentness refusal rolls back BOTH draft and every projection/fence/source row");
            assert_eq!(
                current_body(&s, &form).as_deref(),
                Some(before_body.as_str())
            );
        }
        if matches!(fault, Fault::LateAckCancel) {
            // SQL is positively committed, but its actual original token ends
            // BEFORE the caller admits the real ACK: cleanup remains Unknown.
            cancel.cancel();
            let _ = original.send(result);
        } else if matches!(fault, Fault::CancelBeforeCommit) {
            // Even a real worker refusal cannot be claimed as acknowledged by
            // a caller whose original token has already ended.
            let _ = original.send(result);
        } else if matches!(fault, Fault::LostReply) {
            // Actual SQL committed, but no reply reaches the original waiter.
            // Its accepted operation is UNKNOWN, never falsely Retained.
            drop(original);
            drop(result);
        } else {
            assert!(original.send(result).is_ok());
        }
        let outcome = publisher.join().unwrap();
        match fault {
            Fault::None => {
                assert_eq!(outcome.state, State::Confirmed);
                assert_eq!(outcome.draft_cleanup, DraftCleanupV85::Removed);
                assert!(outcome.refreshed_page_v19.is_some());
            }
            Fault::LostReply | Fault::LateAckCancel | Fault::CancelBeforeCommit => {
                assert_eq!(outcome.draft_cleanup, DraftCleanupV85::Unknown);
                assert!(outcome.refreshed_page_v19.is_none());
                assert!(outcome.return_v19.is_none());
                assert_eq!(
                    outcome.draft_caption_v85(),
                    "cleanup unconfirmed (local ACK unknown) • inspect durable state; never resend"
                );
                assert_eq!(
                    current_body(&s, &form),
                    matches!(fault, Fault::CancelBeforeCommit).then_some(before_body.clone())
                );
            }
            _ => {
                assert_eq!(outcome.draft_cleanup, DraftCleanupV85::Retained);
                assert!(outcome.refreshed_page_v19.is_none());
                assert_eq!(
                    current_body(&s, &form).as_deref(),
                    Some(before_body.as_str())
                );
            }
        }
        actual::exact_protocol(&s);
        assert!(
            matches!(receiver.try_recv(), Err(mpsc::TryRecvError::Empty)),
            "No retry/resend/second cleanup message"
        );
        assert_eq!(writer.producer.lock().unwrap().legacy_active, 0);
    });
    drop(receiver);
    drop(writer);
}

#[test]
fn conversation_refresh_v19_atomic_actual_worker_commits_exact_projection_and_draft_together() {
    actual_case(Fault::None)
}
#[test]
fn conversation_refresh_v19_atomic_actual_worker_raw_cas_clear_and_both_triggers_rollback_full_sql()
{
    for fault in [
        Fault::RawReceipt,
        Fault::Clear,
        Fault::DeleteTrigger,
        Fault::ProjectionTrigger,
    ] {
        actual_case(fault)
    }
}
#[test]
fn conversation_refresh_v19_atomic_actual_worker_same_body_new_epoch_and_committed_lost_reply_are_honest(
) {
    for fault in [Fault::NewProducerEpoch, Fault::LostReply] {
        actual_case(fault)
    }
}

#[test]
fn conversation_refresh_v19_atomic_actual_worker_accepted_cancel_rolls_back_and_late_committed_ack_stays_unknown(
) {
    for fault in [Fault::CancelBeforeCommit, Fault::LateAckCancel] {
        actual_case(fault);
    }
}

#[derive(Clone, Copy)]
enum Admission {
    Full,
    Disconnected,
    Capacity,
}
fn expected_protected_after_confirmed_comment_v253(
    s: &crate::personal_pr_entry_v13::tests::Setup,
) -> Vec<String> {
    use rusqlite::types::{Value as SqlValue, ValueRef};
    // This oracle runs BEFORE returning the real reference Save ACK. The
    // publisher has not claimed/sent/confirmed yet. Independently model ONLY
    // the fixture's expected remote comment receipt and its two scope advances;
    // never snapshot post-outcome state or exempt unrelated rows/columns.
    let db = rusqlite::Connection::open_with_flags(
        crate::personal_pr_entry_v13::tests::database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let tables = db
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .filter(|name| name == "drafts" || name.starts_with("pr_write_"))
        .collect::<Vec<_>>();
    let mut expected = Vec::new();
    let mut attempts_changed = 0;
    let mut scopes_changed = 0;
    for table in tables {
        let mut query = db
            .prepare(&format!("SELECT * FROM \"{}\"", table.replace('"', "\"\"")))
            .unwrap();
        let count = query.column_count();
        let mut rows = query
            .query_map([], |row| {
                (0..count)
                    .map(|i| row.get::<_, SqlValue>(i))
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        for row in &mut rows {
            if table == "pr_write_attempts"
                && row[1..4]
                    == [
                        SqlValue::Text("github.invalid".into()),
                        SqlValue::Text("user-id:42".into()),
                        SqlValue::Text("PERSONAL_PR_1007".into()),
                    ]
            {
                assert_eq!(row.len(), 13);
                assert_eq!(row[0], SqlValue::Integer(1));
                assert_eq!(row[6], SqlValue::Text("conversation-create-v17".into()));
                assert_eq!(row[10], SqlValue::Text("pending".into()));
                assert_eq!(row[11], SqlValue::Null);
                row[10] = SqlValue::Text("confirmed".into());
                row[11] = SqlValue::Text(
                    r#"{"kind":"comment","resource_id":"901","node_id":"IC_901"}"#.into(),
                );
                attempts_changed += 1;
            } else if table == "pr_write_scopes"
                && row[..3]
                    == [
                        SqlValue::Text("github.invalid".into()),
                        SqlValue::Text("user-id:42".into()),
                        SqlValue::Text("PERSONAL_PR_1007".into()),
                    ]
            {
                assert_eq!(row.len(), 4);
                assert_eq!(row[3], SqlValue::Integer(1));
                row[3] = SqlValue::Integer(3); // actual claim + finish, exactly +2
                scopes_changed += 1;
            }
        }
        let mut rendered = rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|value| format!("{:?}", ValueRef::from(value)))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        rendered.sort();
        expected.extend(rendered.into_iter().map(|row| format!("{table}:{row:?}")));
    }
    assert_eq!((attempts_changed, scopes_changed), (1, 1));
    expected
}
fn refused_before_atomic(fault: Admission) {
    let command = PrCommand::NewComment;
    let mut tail = protocol::phase(command, false, Some(true));
    tail.extend(actual::receipts(command));
    tail.extend(actual::visibility());
    if matches!(fault, Admission::Capacity) {
        // The before-copy bound refuses preparation BEFORE paid final UID.
        assert!(tail.last().unwrap().args.iter().any(|a| a == "user"));
        tail.pop();
    }
    let (s, form, preview) = actual::prepared(command, tail);
    let path = crate::personal_pr_entry_v13::tests::database(&s.fixture);
    let (mut writer, receiver) = controlled(&path);
    let mut cache = Ok(Cache::open(&path).unwrap());
    writer
        .save(
            form.scope(),
            form.resource(),
            ui::encoded_draft(&form).unwrap(),
        )
        .unwrap();
    cache = dispatch(&writer, next(&receiver), cache);
    let client = writer.client();
    let token = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let fence = client
        .capture_publication_v2(&path, &form.scope(), &form.resource(), &token, deadline)
        .unwrap();
    let api = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    thread::scope(|scope| {
        let publisher = scope.spawn(|| {
            PrPlan::execute_original_conversation_v17(
                &preview, &api, &path, &client, fence, &token, deadline,
            )
            .unwrap()
        });
        let Message::PublicationV2(mut message) = next(&receiver) else {
            panic!("Actual reference Save must precede atomic projection");
        };
        assert!(matches!(&message.effect, PublicationEffectV2::Save(_)));
        let (proxy, observed) = mpsc::sync_channel(1);
        let PublicationReplyV85::Draft(original) =
            std::mem::replace(&mut message.reply, PublicationReplyV85::Draft(proxy))
        else {
            panic!("Actual reference Save ACK expected");
        };
        cache = dispatch(&writer, Message::PublicationV2(message), cache);
        assert!(cache.is_ok());
        let mut ack = observed
            .recv_timeout(Duration::from_secs(8))
            .unwrap()
            .unwrap();
        let body = current_body(&s, &form).unwrap();
        let protected = expected_protected_after_confirmed_comment_v253(&s);
        match fault {
            Admission::Capacity => {
                // Genuine acknowledged allocation, enlarged only in this
                // hostile allocator-bound fixture. Length/body stay exact.
                ack.saved_body.reserve(8 * 1024 * 1024);
                assert_eq!(ack.saved_body, body);
                assert!(ack.saved_body.capacity() >= 8 * 1024 * 1024);
                assert!(original.send(Ok(ack)).is_ok());
                let outcome = publisher.join().unwrap();
                assert_eq!(outcome.draft_cleanup, DraftCleanupV85::Retained);
                assert!(outcome.refreshed_page_v19.is_none());
                assert!(matches!(
                    receiver.try_recv(),
                    Err(mpsc::TryRecvError::Empty)
                ));
                actual::exact_protocol(&s);
            }
            Admission::Full => {
                // Exercise the actual bounded channel, not a fabricated
                // QueueFull result. These inert sentinels are never run.
                for _ in 0..8 {
                    assert!(writer.sender.try_send(Message::Shutdown).is_ok());
                }
                // Restrict the real ACK's clock; no new source/read deadline.
                ack.fence.deadline = ack
                    .fence
                    .deadline
                    .min(Instant::now() + Duration::from_secs(5));
                assert!(original.send(Ok(ack)).is_ok());
                let outcome = publisher.join().unwrap();
                assert_eq!(outcome.draft_cleanup, DraftCleanupV85::Retained);
                assert!(outcome.refreshed_page_v19.is_none());
                for _ in 0..8 {
                    assert!(matches!(receiver.try_recv(), Ok(Message::Shutdown)));
                }
                assert!(matches!(
                    receiver.try_recv(),
                    Err(mpsc::TryRecvError::Empty)
                ));
                actual::exact_protocol(&s);
            }
            Admission::Disconnected => {
                drop(receiver);
                assert!(original.send(Ok(ack)).is_ok());
                let outcome = publisher.join().unwrap();
                assert_eq!(outcome.draft_cleanup, DraftCleanupV85::Retained);
                assert!(outcome.refreshed_page_v19.is_none());
                actual::exact_protocol(&s);
            }
        }
        assert_eq!(current_body(&s, &form).as_deref(), Some(body.as_str()));
        assert_eq!(actual::refresh_v19::protected_rows_v84(&s), protected);
        assert_eq!(writer.producer.lock().unwrap().legacy_active, 0);
    });
    // A controlled writer has no thread; disconnect before its Shutdown Drop.
    drop(writer);
}

#[test]
fn conversation_refresh_v19_atomic_actual_full_disconnected_channels_and_preclone_capacity_refuse_without_cleanup(
) {
    for fault in [
        Admission::Full,
        Admission::Disconnected,
        Admission::Capacity,
    ] {
        refused_before_atomic(fault);
    }
}

#[test]
fn conversation_refresh_v19_atomic_actual_current_other_resource_same_body_ack_refuses_before_copy_and_sql(
) {
    let command = PrCommand::NewComment;
    let mut tail = protocol::phase(command, false, Some(true));
    tail.extend(actual::receipts(command));
    tail.extend(actual::visibility());
    let (s, form, preview) = actual::prepared(command, tail);
    let path = crate::personal_pr_entry_v13::tests::database(&s.fixture);
    let scope = form.scope();
    let resource = form.resource();
    let (mut writer, receiver) = controlled(&path);
    let mut cache = Ok(Cache::open(&path).unwrap());
    writer
        .save(
            scope.clone(),
            resource.clone(),
            ui::encoded_draft(&form).unwrap(),
        )
        .unwrap();
    cache = dispatch(&writer, next(&receiver), cache);
    let client = writer.client();
    let token = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let fence = client
        .capture_publication_v2(&path, &scope, &resource, &token, deadline)
        .unwrap();
    let api = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    let (prepared, _seal, body, mut cache) = thread::scope(|threads| {
        let publisher = threads.spawn(|| {
            PrPlan::execute_original_conversation_v17(
                &preview, &api, &path, &client, fence, &token, deadline,
            )
            .unwrap()
        });
        // This actual publisher produces its real reference Save, terminal,
        // prepared projection and final-UID seal. Do not fabricate any of them.
        cache = dispatch(&writer, next(&receiver), cache);
        let Message::PublicationV2(message) = next(&receiver) else {
            panic!("Actual closed atomic publication required");
        };
        let PublicationMessageV2 {
            fence: queued_fence,
            effect,
            activity,
            reply,
        } = *message;
        let PublicationEffectV2::CommitConversation(data) = effect else {
            panic!("Actual prepared projection required");
        };
        // Do not dispatch SQL or synthesize an ACK from the entered fence.
        // End this actual activity before obtaining independently current ACKs.
        drop(activity);
        drop(queued_fence);
        drop(reply);
        let outcome = publisher.join().unwrap();
        assert_eq!(outcome.draft_cleanup, DraftCleanupV85::Unknown);
        assert!(outcome.refreshed_page_v19.is_none());
        assert!(outcome.return_v19.is_none());
        assert_eq!(writer.producer.lock().unwrap().legacy_active, 0);
        let AtomicProjectionV85 {
            prepared,
            seal,
            body,
        } = *data;
        assert_eq!(current_body(&s, &form).as_deref(), Some(body.as_str()));
        actual::exact_protocol(&s);
        (prepared, seal, body, cache)
    });
    let foreign = format!("{resource}/different-current-resource-v86");
    assert_ne!(resource, foreign);
    for (key, expected) in [(&resource, true), (&foreign, false)] {
        // SAME actual producer/database/account, SAME encoded bytes. Each key
        // gets a new editing revision plus a real durable publication Save ACK.
        writer
            .save(scope.clone(), key.clone(), body.clone())
            .unwrap();
        cache = dispatch(&writer, next(&receiver), cache);
        let fence = client
            .capture_publication_v2(&path, &scope, key, &token, deadline)
            .unwrap();
        let (ack, returned) = thread::scope(|threads| {
            let waiter = threads.spawn(|| {
                client
                    .save_publication_and_wait_v2(fence, body.clone(), &token)
                    .unwrap()
            });
            let message = next(&receiver);
            assert!(matches!(
                &message,
                Message::PublicationV2(message)
                    if matches!(&message.effect, PublicationEffectV2::Save(saved) if saved == &body)
            ));
            let returned = dispatch(&writer, message, cache);
            (waiter.join().unwrap(), returned)
        });
        cache = returned;
        // A foreign key is NOT rejected merely for being stale or unknown.
        ack.check_current(&client, &path, &scope, key, &token)
            .unwrap();
        assert!(Arc::ptr_eq(&ack.fence.producer, &writer.producer));
        assert_eq!(ack.saved_body, body);
        assert_eq!(ack.fence.snapshot.scope, scope);
        assert_eq!(ack.fence.snapshot.resource, *key);
        let before_sql = actual::refresh_v19::exact_sql(&s);
        let before_commands =
            std::fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
        assert_eq!(
            prepared.accepts_atomic_resource_v86(&ack, &client, key, &body, &token),
            expected,
            "Only the actual terminal's selected resource may bind a current ACK"
        );
        assert_eq!(actual::refresh_v19::exact_sql(&s), before_sql);
        assert_eq!(
            std::fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap(),
            before_commands
        );
        ack.check_current(&client, &path, &scope, key, &token)
            .unwrap();
        assert!(matches!(
            receiver.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
        assert_eq!(writer.producer.lock().unwrap().legacy_active, 0);
        actual::exact_protocol(&s);
    }
    assert!(cache.is_ok());
    assert_eq!(current_body(&s, &form).as_deref(), Some(body.as_str()));
    assert_eq!(
        cache
            .as_ref()
            .unwrap()
            .draft(&scope, &foreign)
            .unwrap()
            .as_deref(),
        Some(body.as_str())
    );
    drop(receiver);
    drop(writer);
}
