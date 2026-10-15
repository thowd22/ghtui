//! Actual SQLite causal gap: mark_confirmed really completed, terminal fence
//! intentionally not called. This does NOT claim a real process crash or GH
//! write; code-only journal/read-guard recovery supplements native workflows.
use super::*;
#[path = "discussion_reaction_empty_impact_v3_tests.rs"]
mod reaction_empty_v3;
#[path = "discussion_comment_receipt_recovery_v2_tests.rs"]
mod selected_comments_v2;
use rusqlite::OptionalExtension;

fn thread_target(scope_id: u64, category: DiscussionKind) -> DiscussionTarget {
    let mut target = target(category);
    target.scope = Scope::github("github.invalid", Some(scope_id)).unwrap();
    target
}
fn thread_write(command: &str, selected: String) -> PrWriteIntent {
    PrWriteIntent {
        command: command.into(),
        selected_resource: selected,
        observed_head: "a".repeat(40),
        values: BTreeMap::from([(
            "body".into(),
            if command == "thread-reply" {
                "actual original reply fixture"
            } else {
                ""
            }
            .into(),
        )]),
    }
}
fn selected(command: &str) -> String {
    format!(
        "{}/{}",
        if command == "thread-reply" {
            "thread-reply"
        } else {
            "thread-state"
        },
        "a".repeat(64)
    )
}
fn effect(command: &str) -> PrConfirmedEffect {
    PrConfirmedEffect {
        kind: if command == "thread-reply" {
            PrEffectKind::Comment
        } else {
            PrEffectKind::Thread
        },
        resource_id: match command {
            "thread-reply" => "901",
            "thread-resolve" => "resolved",
            "thread-reopen" => "unresolved",
            _ => panic!("closed thread purpose"),
        }
        .into(),
        node_id: Some(
            if command == "thread-reply" {
                "COMMENT_ACTUAL_901"
            } else {
                "THREAD_ACTUAL_9"
            }
            .into(),
        ),
    }
}
fn record_confirmed(
    database: &Path,
    target: &DiscussionTarget,
    write: &PrWriteIntent,
    receipt: &PrConfirmedEffect,
) -> PrPublicationReference {
    let key = PrWriteTarget::for_source(
        &target.scope,
        &target.repository,
        &target.parent_node,
        target.number,
    )
    .unwrap();
    let mut journal = PrWriteJournal::open(database).unwrap();
    let permit = journal.begin(&key, write).unwrap();
    journal.claim(&permit, &key, write).unwrap();
    journal.mark_confirmed(&permit, receipt).unwrap();
    permit.marker().publication_reference()
}
fn raw_records(database: &Path) -> Vec<String> {
    Connection::open(database).unwrap().prepare("SELECT json_array(host,login,kind,id,payload) FROM records ORDER BY host,login,kind,id").unwrap().query_map([],|row|row.get(0)).unwrap().map(Result::unwrap).collect()
}
fn ordinal(database: &Path) -> Option<u64> {
    Connection::open(database).unwrap().query_row("SELECT mutation_ordinal FROM sync_mutation_fences WHERE host='github.invalid' AND login='user-id:42' AND repository='example/repo' AND resource='PR_guard_7' AND kind='inline_comment'",[],|row|row.get(0)).optional().unwrap()
}
fn visible(cache: &Cache, target: &DiscussionTarget) -> Option<u64> {
    cache
        .binding_status(&target.scope, &target.source())
        .unwrap()
        .unwrap()
        .visible_generation
}

#[test]
fn review_threads_v1_actual_confirmed_without_terminal_call_next_read_recovers_same_attempt_inline_only_once_and_scope_isolation(
) {
    for command in ["thread-reply", "thread-resolve", "thread-reopen"] {
        let (_dir, database) = fixture();
        let inline = thread_target(42, DiscussionKind::InlineComment);
        let review = thread_target(42, DiscussionKind::Review);
        let foreign = thread_target(43, DiscussionKind::InlineComment);
        let conversation = thread_target(42, DiscussionKind::ConversationComment);
        let (mut cache, old_inline) = cached(&database, &inline);
        let (_, review_generation) = cached(&database, &review);
        let (_, foreign_generation) = cached(&database, &foreign);
        let (_, conversation_generation) = cached(&database, &conversation);
        let sql = Connection::open(&database).unwrap();
        sql.execute("UPDATE records SET payload=?1 WHERE host=?2 AND login=?3 AND id=?4",rusqlite::params![json!({"body":"actual retained old inline bytes","context":{"resolved":false},"thread":"THREAD_ACTUAL_9"}).to_string(),inline.scope.host,inline.scope.login,inline.source()]).unwrap();
        let before = raw_records(&database);
        let receipt = record_confirmed(
            &database,
            &inline,
            &thread_write(command, selected(command)),
            &effect(command),
        );
        assert_eq!(ordinal(&database), None, "No terminal fence was called");
        assert_eq!(visible(&cache, &inline), Some(old_inline));
        let key = PrWriteTarget::for_source(
            &inline.scope,
            &inline.repository,
            &inline.parent_node,
            inline.number,
        )
        .unwrap();
        let discovered = PrWriteJournal::open(&database)
            .unwrap()
            .latest_source_receipt(&key, SyncBindingKind::InlineComment)
            .unwrap()
            .expect("Actual known thread receipt discovered from ordinary read guard category");
        assert_eq!(discovered.publication_reference(), receipt);
        assert_eq!(discovered.command, command);
        let guard = guard_discussion_reads(&database, &inline.scope, std::slice::from_ref(&inline))
            .unwrap();
        assert_eq!(guard.recovered_sources(), [inline.source()].as_slice());
        assert!(guard.blocked_sources().is_empty());
        assert_eq!(ordinal(&database), Some(receipt.attempt_id as u64));
        assert_eq!(
            visible(&cache, &inline),
            None,
            "Both old inline body and structured context.resolved are hidden together"
        );
        assert_eq!(visible(&cache, &review), Some(review_generation));
        assert_eq!(visible(&cache, &foreign), Some(foreign_generation));
        assert_eq!(
            visible(&cache, &conversation),
            Some(conversation_generation)
        );
        assert_eq!(
            raw_records(&database),
            before,
            "No source body/context bytes parsed, erased or rewritten by recovery"
        );
        let fresh = cache
            .reserve_resource_sync(&inline.scope, &inline.source(), &inline.binding().unwrap())
            .unwrap();
        assert!(cache.commit_reserved_sync(&inline.scope,&inline.source(),fresh,&[Record::new("collaboration-document-v1",inline.source(),&inline.repository,json!({"body":"explicit new hydration","context":{"resolved":command=="thread-resolve"}}))],true).unwrap());
        let repeated =
            guard_discussion_reads(&database, &inline.scope, std::slice::from_ref(&inline))
                .unwrap();
        assert!(repeated.recovered_sources().is_empty());
        assert_eq!(ordinal(&database), Some(receipt.attempt_id as u64));
        assert_eq!(
            visible(&cache, &inline),
            Some(fresh),
            "Same attempt cannot fence later explicit hydration twice"
        );
        let (state,count):(String,u64)=sql.query_row("SELECT state,(SELECT count(*) FROM pr_write_attempts) FROM pr_write_attempts WHERE attempt_id=?1",[receipt.attempt_id],|row|Ok((row.get(0)?,row.get(1)?))).unwrap();
        assert_eq!(
            (state.as_str(), count),
            ("confirmed", 1),
            "Read does not reconcile, retry or release durable marker"
        );
    }
}

#[test]
fn review_threads_v1_newest_malformed_known_confirmed_receipt_refuses_without_older_valid_revival()
{
    for command in ["thread-reply", "thread-resolve", "thread-reopen"] {
        for malformed in [
            "wrong-prefix/",
            "thread-state/ABC",
            "thread-reply/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaA",
        ] {
            let (_dir, database) = fixture();
            let inline = thread_target(42, DiscussionKind::InlineComment);
            let (mut cache, _) = cached(&database, &inline);
            let old = record_confirmed(
                &database,
                &inline,
                &thread_write(command, selected(command)),
                &effect(command),
            );
            guard_discussion_reads(&database, &inline.scope, std::slice::from_ref(&inline))
                .unwrap();
            let fresh = cache
                .reserve_resource_sync(&inline.scope, &inline.source(), &inline.binding().unwrap())
                .unwrap();
            assert!(cache.commit_reserved_sync(&inline.scope,&inline.source(),fresh,&[Record::new("collaboration-document-v1",inline.source(),&inline.repository,json!({"body":"private later hydration must not be admitted through an old receipt","context":{"resolved":true}}))],true).unwrap());
            let newer = record_confirmed(
                &database,
                &inline,
                &thread_write(command, malformed.into()),
                &effect(command),
            );
            assert!(newer.attempt_id > old.attempt_id);
            let key = PrWriteTarget::for_source(
                &inline.scope,
                &inline.repository,
                &inline.parent_node,
                inline.number,
            )
            .unwrap();
            let actual = PrWriteJournal::open(&database)
                .unwrap()
                .latest_source_receipt(&key, SyncBindingKind::InlineComment)
                .unwrap()
                .unwrap();
            assert_eq!(
                actual.publication_reference(),
                newer,
                "Known purpose BEFORE shape prevents falling back to old valid receipt"
            );
            let before = raw_records(&database);
            assert!(
                guard_discussion_reads(&database, &inline.scope, std::slice::from_ref(&inline))
                    .is_err(),
                "Closed typed validator refuses malformed newest; no private read admission"
            );
            assert_eq!(
                ordinal(&database),
                Some(old.attempt_id as u64),
                "Malformed receipt cannot create a valid fence"
            );
            assert_eq!(
                visible(&cache, &inline),
                Some(fresh),
                "Failure is not false successful hydration/fence evidence"
            );
            assert_eq!(raw_records(&database), before);
        }
    }
}

#[test]
fn review_threads_v1_newest_known_thread_receipt_metadata_cap_and_typed_effect_refuse_before_fencing(
) {
    for mode in [0, 1, 2] {
        let (_dir, database) = fixture();
        let inline = thread_target(42, DiscussionKind::InlineComment);
        let (cache, generation) = cached(&database, &inline);
        let mut receipt = effect("thread-resolve");
        if mode == 1 {
            receipt.resource_id = "unresolved".into();
        }
        if mode == 2 {
            receipt.node_id = None;
        }
        let reference = record_confirmed(
            &database,
            &inline,
            &thread_write("thread-resolve", selected("thread-resolve")),
            &receipt,
        );
        if mode == 0 {
            Connection::open(&database)
                .unwrap()
                .execute(
                    "UPDATE pr_write_attempts SET selected_resource=?1 WHERE attempt_id=?2",
                    rusqlite::params![
                        format!("thread-state/{}", "a".repeat(8193)),
                        reference.attempt_id
                    ],
                )
                .unwrap();
        }
        let before = raw_records(&database);
        assert!(
            guard_discussion_reads(&database, &inline.scope, std::slice::from_ref(&inline))
                .is_err()
        );
        assert_eq!(ordinal(&database), None);
        assert_eq!(visible(&cache, &inline), Some(generation));
        assert_eq!(raw_records(&database), before);
    }
}
