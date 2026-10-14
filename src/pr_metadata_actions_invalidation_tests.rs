//! Source-only SQL structured filter contract, not a substitute for the pending
//! Task65 real-binary label-filter/source/Open journey.
use super::*;
use crate::storage::{
    Cache, Record, ReservedSyncBatch, Scope, SyncBinding, SyncBindingKind, SyncNamespace,
};
#[test]
fn pr_metadata_actions_label_filter_fence_preserves_body_bytes_fences_late_rows_and_limits_exact_scope(
) {
    let (f, plan, mut journal) = setup(PrMetadataCommand::AddLabel);
    let database = journal.database_path().to_owned();
    let scope = plan.target().scope().clone();
    let mut cache = Cache::open(&database).unwrap();
    let mut records = vec![];
    for (index, kind) in [
        SyncBindingKind::PullRequestBody,
        SyncBindingKind::ConversationComment,
        SyncBindingKind::Review,
        SyncBindingKind::InlineComment,
    ]
    .into_iter()
    .enumerate()
    {
        let source = format!("collaboration-v1:{:064x}", index + 1);
        let binding = SyncBinding {
            namespace: SyncNamespace::Collaboration,
            repository_id: 410,
            repository: "example/repo".into(),
            resource: "PR_7".into(),
            kind,
        };
        let record = Record::new(
            "fixture-discussion",
            format!("doc-{index}"),
            "example/repo",
            json!({"body":"Genuine unchanged Unicode body ☃", "labels":["existing ☃"]}),
        );
        let generation = cache
            .reserve_resource_sync(&scope, &source, &binding)
            .unwrap();
        assert!(cache
            .commit_reserved_batch(
                &scope,
                ReservedSyncBatch {
                    source: &source,
                    generation,
                    records: std::slice::from_ref(&record),
                    complete: false,
                    deletion_authoritative: false
                }
            )
            .unwrap());
        let late = cache
            .reserve_resource_sync(&scope, &source, &binding)
            .unwrap();
        records.push((source, binding, record, late));
    }
    let foreign = Scope::github("github.invalid", Some(43)).unwrap();
    let source =
        "collaboration-v1:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
    let binding = SyncBinding {
        namespace: SyncNamespace::Collaboration,
        repository_id: 410,
        repository: "example/repo".into(),
        resource: "PR_7".into(),
        kind: SyncBindingKind::PullRequestBody,
    };
    let record = Record::new(
        "fixture-discussion",
        "foreign",
        "example/repo",
        json!({"body":"Other principal bytes", "labels":["foreign"]}),
    );
    let generation = cache
        .reserve_resource_sync(&foreign, source, &binding)
        .unwrap();
    assert!(cache
        .commit_reserved_batch(
            &foreign,
            ReservedSyncBatch {
                source,
                generation,
                records: std::slice::from_ref(&record),
                complete: false,
                deletion_authoritative: false
            }
        )
        .unwrap());
    let p = provider(&f);
    let limits = limits();
    let cancel = CancellationToken::default();
    let result = plan.execute(
        execution_context(&mut journal, &p, &limits, &cancel),
        |_| Ok(()),
        |_, impact| {
            assert_eq!(impact.hidden_label_sources().len(), 4);
            assert_eq!(impact.source_epoch(), 3);
            let reader = Cache::open(&database).unwrap();
            for (source, _, record, _) in &records {
                assert_eq!(
                    reader
                        .binding_status(&scope, source)
                        .unwrap()
                        .unwrap()
                        .visible_generation,
                    None
                );
                assert_eq!(
                    reader
                        .get(&scope, &record.kind, &record.id)
                        .unwrap()
                        .unwrap()
                        .payload,
                    record.payload
                );
            }
            Ok(PrMetadataRefresh {
                private_visible: true,
            })
        },
    );
    let PrMetadataExecution::Confirmed { publication, .. } = result else {
        panic!("expected confirmed actual label effect and structured hook")
    };
    for (source, binding, record, late) in records {
        assert!(!cache
            .commit_reserved_batch(
                &scope,
                ReservedSyncBatch {
                    source: &source,
                    generation: late,
                    records: std::slice::from_ref(&record),
                    complete: false,
                    deletion_authoritative: false
                }
            )
            .unwrap());
        let generation = cache
            .reserve_resource_sync(&scope, &source, &binding)
            .unwrap();
        let mut fresh = record.clone();
        fresh.payload["labels"] = json!(["existing ☃", "new 🐛"]);
        assert!(cache
            .commit_reserved_batch(
                &scope,
                ReservedSyncBatch {
                    source: &source,
                    generation,
                    records: std::slice::from_ref(&fresh),
                    complete: false,
                    deletion_authoritative: false
                }
            )
            .unwrap());
        assert_eq!(
            cache
                .get(&scope, &record.kind, &record.id)
                .unwrap()
                .unwrap()
                .payload["body"],
            record.payload["body"]
        );
    }
    assert_eq!(
        cache
            .binding_status(&foreign, source)
            .unwrap()
            .unwrap()
            .visible_generation,
        Some(generation)
    );
    let replay = invalidate_confirmed_metadata(
        &database,
        &identity(),
        plan.original(),
        &publication.publication_reference(),
    )
    .unwrap();
    assert!(replay.hidden_label_sources().is_empty());
    assert_commands(&f, 26, 1);
}
