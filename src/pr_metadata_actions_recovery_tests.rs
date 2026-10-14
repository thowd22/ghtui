//! Source-only actual SQL repair cases, not yet registered/executed.
use super::*;
use crate::storage::{
    Cache, Record, ReservedSyncBatch, SyncBinding, SyncBindingKind, SyncNamespace,
};
#[test]
fn pr_metadata_actions_label_read_recovery_repairs_confirmed_fence_sql_failure_and_tracks_pending_epoch_without_resend(
) {
    let (f, plan, mut journal) = setup(PrMetadataCommand::AddLabel);
    let database = journal.database_path().to_owned();
    let scope = plan.target().scope().clone();
    let mut cache = Cache::open(&database).unwrap();
    let source =
        "collaboration-v1:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let binding = SyncBinding {
        namespace: SyncNamespace::Collaboration,
        repository_id: 410,
        repository: "example/repo".into(),
        resource: "PR_7".into(),
        kind: SyncBindingKind::PullRequestBody,
    };
    let record = Record::new(
        "fixture-discussion",
        "kept-body",
        "example/repo",
        json!({"body":"Unchanged retained bytes ☃","labels":["existing ☃"]}),
    );
    let generation = cache
        .reserve_resource_sync(&scope, source, &binding)
        .unwrap();
    assert!(cache
        .commit_reserved_batch(
            &scope,
            ReservedSyncBatch {
                source,
                generation,
                records: std::slice::from_ref(&record),
                complete: false,
                deletion_authoritative: false
            }
        )
        .unwrap());
    Connection::open(&database).unwrap().execute_batch("CREATE TRIGGER reject_label_fence BEFORE INSERT ON sync_mutation_fences BEGIN SELECT RAISE(ABORT,'metadata fence SQL failed'); END;").unwrap();
    let provider = provider(&f);
    let limits = limits();
    let cancel = CancellationToken::default();
    let result = plan.execute(
        execution_context(&mut journal, &provider, &limits, &cancel),
        |_| Ok(()),
        |_, _| panic!("failed structured fence cannot publish an optional current view"),
    );
    let PrMetadataExecution::ConfirmedButRefreshFailed {
        publication,
        private_visible: false,
        ..
    } = result
    else {
        panic!("expected durable receipt before actual metadata SQL fence failure")
    };
    assert_eq!(
        cache
            .binding_status(&scope, source)
            .unwrap()
            .unwrap()
            .visible_generation,
        Some(generation)
    );
    assert_sql(&journal, &plan, "confirmed", 1);
    Connection::open(&database)
        .unwrap()
        .execute_batch("DROP TRIGGER reject_label_fence")
        .unwrap();
    let repaired = repair_label_filter_reads(&database, &identity(), plan.original()).unwrap();
    assert_eq!(repaired.unavailable_reason(), None);
    repaired.validate().unwrap();
    assert_eq!(
        cache
            .binding_status(&scope, source)
            .unwrap()
            .unwrap()
            .visible_generation,
        None
    );
    assert_eq!(
        cache
            .get(&scope, &record.kind, &record.id)
            .unwrap()
            .unwrap()
            .payload,
        record.payload
    );
    let old = restore_original(
        &journal,
        &identity(),
        plan.original(),
        &publication.publication_reference(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(old.attempt_id, publication.attempt_id);
    assert_commands(&f, 26, 1);
    let pending = journal.begin(plan.target(), plan.intent()).unwrap();
    assert!(pending.marker().confirmed.is_none());
    assert!(repaired.validate().is_err());
    let unknown = repair_label_filter_reads(&database, &identity(), plan.original()).unwrap();
    assert!(unknown
        .unavailable_reason()
        .unwrap()
        .contains("no invocation/effect inferred"));
    unknown.validate().unwrap();
    assert_commands(&f, 26, 1);
}
#[test]
fn pr_metadata_actions_label_read_recovery_utf8_preflight_refuses_oversized_original_without_body_or_receipt_release(
) {
    let command = PrMetadataCommand::AddLabel;
    let f = Fixture::new();
    f.scripted_gh(&catalogue(command, &parent(), vec![candidate(command)]));
    let plan = plan(&f, command);
    let mut journal = PrWriteJournal::open(&f.root.join("metadata.sqlite")).unwrap();
    let marker = journal.begin(plan.target(), plan.intent()).unwrap();
    Connection::open(journal.database_path())
        .unwrap()
        .execute(
            "UPDATE pr_write_attempts SET selected_resource=?1 WHERE attempt_id=?2",
            params![
                format!("labels:{}", "界".repeat(3000)),
                marker.marker().attempt_id
            ],
        )
        .unwrap();
    let result = repair_label_filter_reads(journal.database_path(), &identity(), plan.original());
    assert!(matches!(result,Err(ref error) if error.contains("8 KiB UTF8")));
    assert_commands(&f, 8, 0);
    let mut changed = identity();
    changed.account_id = Some(43);
    let foreign =
        repair_label_filter_reads(journal.database_path(), &changed, plan.original()).unwrap();
    assert_eq!(foreign.unavailable_reason(), None);
    foreign.validate().unwrap();
    assert_commands(&f, 8, 0);
}
